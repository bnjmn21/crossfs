use std::{
    fmt::Debug,
    io::{ErrorKind, Read, Write},
    net::{IpAddr, TcpListener, TcpStream, ToSocketAddrs},
    sync::{
        Mutex,
        mpsc::{Receiver, Sender, channel},
    },
    thread,
};

use serde::{Deserialize, Serialize};

use crate::{Error, controls::Primary};

const MAX_PACKET_SIZE: u32 = 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToServerPacket {
    Auth { password: u64, master: bool },
    Primary(Primary),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FromServerPacket {
    Primary(Primary),
    AuthOk,
    AuthFail,
    DifferentMasterAlreadyConnected,
}

pub struct Client {
    stream: TcpStream,
}

impl Client {
    pub fn new(addr: impl ToSocketAddrs, password: u64, master: bool) -> Result<Client, Error> {
        let mut stream = TcpStream::connect(addr)?;
        stream.set_nodelay(true).unwrap();
        send_tcp(&mut stream, ToServerPacket::Auth { password, master })?;
        let answer: FromServerPacket = recv_tcp(&mut stream)?;
        match answer {
            FromServerPacket::AuthOk => {}
            FromServerPacket::AuthFail => return Err(Error::WrongPassword),
            FromServerPacket::DifferentMasterAlreadyConnected => {
                return Err(Error::DifferentMasterAlreadyConnected);
            }
            packet => panic!("Invalid packet received: {packet:?}"),
        }
        Ok(Client { stream })
    }

    pub fn run(
        &mut self,
        send: Receiver<ToServerPacket>,
        recv: Sender<FromServerPacket>,
    ) -> Result<(), Error> {
        loop {
            while let Some(packet) = recv_tcp_nonblocking(&mut self.stream)? {
                recv.send(packet).unwrap();
            }
            while let Ok(packet) = send.try_recv() {
                send_tcp(&mut self.stream, packet)?;
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct ServerState {
    master_connected: bool,
    clients: Vec<ConnectedClient>,
    current_id: usize,
}

pub fn server(ip: IpAddr, port: u16, password: u64, on_ready: impl FnOnce()) {
    let listener = TcpListener::bind((ip, port)).unwrap();
    let state = Mutex::new(ServerState::default());
    on_ready();
    thread::scope(|s| {
        println!("[Server] Serving...");
        for stream in listener.incoming() {
            let Ok(stream) = stream else {
                continue;
            };
            s.spawn(|| handle_client(stream, &state, password));
        }
    });
}

fn handle_client(
    mut stream: TcpStream,
    state: &Mutex<ServerState>,
    correct_password: u64,
) -> Result<(), Error> {
    let auth: ToServerPacket = recv_tcp(&mut stream)?;
    let mut is_master = false;
    if let ToServerPacket::Auth { password, master } = auth {
        if password != correct_password {
            send_tcp(&mut stream, FromServerPacket::AuthFail)?;
            return Ok(());
        }
        let mut state = state.lock().unwrap();
        if state.master_connected && master {
            send_tcp(
                &mut stream,
                FromServerPacket::DifferentMasterAlreadyConnected,
            )?;
            return Ok(());
        }
        if master {
            is_master = true;
            state.master_connected = true;
        }
        send_tcp(&mut stream, FromServerPacket::AuthOk)?;
    } else {
        println!("[Server] Received invalid packet during authentication.");
        return Ok(());
    }
    let (send, recv) = channel();
    let mut s = state.lock().unwrap();
    let id = s.current_id;
    s.current_id += 1;
    s.clients.push(ConnectedClient {
        send,
        is_master,
        id,
    });
    drop(s);
    loop {
        loop {
            let packet = recv_tcp_nonblocking::<ToServerPacket>(&mut stream);
            match packet {
                Ok(Some(ToServerPacket::Primary(p))) => {
                    let s = state.lock().unwrap();
                    for client in &s.clients {
                        if !client.is_master {
                            client
                                .send
                                .send(ToConnectedClient::Primary(p.clone()))
                                .unwrap();
                        }
                    }
                }
                Ok(Some(packet)) => {
                    println!("[Server] Received invalid packet: {packet:?}");
                }
                Ok(None) => break,
                Err(err) => {
                    println!("[Server] Closed connection: {err:?}");
                    let mut s = state.lock().unwrap();
                    s.clients.retain(|c| c.id != id);
                    return Ok(());
                }
            }
        }
        while let Ok(msg) = recv.try_recv() {
            match msg {
                ToConnectedClient::Primary(p) => {
                    send_tcp(&mut stream, FromServerPacket::Primary(p))?;
                }
            }
        }
    }
}

#[derive(Debug)]
struct ConnectedClient {
    send: Sender<ToConnectedClient>,
    is_master: bool,
    id: usize,
}

enum ToConnectedClient {
    Primary(Primary),
}

fn recv_tcp_nonblocking<T: for<'a> Deserialize<'a>>(
    stream: &mut TcpStream,
) -> Result<Option<T>, Error> {
    let mut size = [0u8, 0, 0, 0];
    stream.set_nonblocking(true).unwrap();
    match stream.read_exact(&mut size) {
        Ok(_) => (),
        Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    stream.set_nonblocking(false).unwrap();
    let size = u32::from_le_bytes(size);
    if size > MAX_PACKET_SIZE {
        return Err(Error::PacketTooLarge);
    }
    let mut data = vec![0; size as usize];
    stream.read_exact(&mut data)?;
    Ok(Some(postcard::from_bytes(&data)?))
}

fn recv_tcp<T: for<'a> Deserialize<'a>>(stream: &mut TcpStream) -> Result<T, Error> {
    let mut size = [0u8, 0, 0, 0];
    stream.read_exact(&mut size)?;
    let size = u32::from_le_bytes(size);
    if size > MAX_PACKET_SIZE {
        return Err(Error::PacketTooLarge);
    }
    let mut data = vec![0; size as usize];
    stream.read_exact(&mut data)?;
    Ok(postcard::from_bytes(&data).unwrap())
}

fn send_tcp(stream: &mut impl Write, data: impl Serialize) -> Result<(), Error> {
    let data = postcard::to_stdvec(&data).unwrap();
    let size = (data.len() as u32).to_le_bytes();
    stream.write_all(&size)?;
    stream.write_all(&data)?;
    stream.flush()?;
    Ok(())
}
