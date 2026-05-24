use std::{
    fs,
    sync::mpsc::{Receiver, Sender, TryRecvError, channel},
    thread::{self, JoinHandle},
};

use ipc_channel::{
    IpcError,
    ipc::{IpcOneShotServer, IpcReceiver, IpcSender},
};
use serde::{Deserialize, Serialize};

use crate::{Error, Mode, SimBackend, config_dir, controls::Primary};

pub struct XPlaneSim {
    from_server: Receiver<FromXPlaneServer>,
    to_server: Sender<ToXPlaneServer>,
    current_primary: Primary,
    is_connected: bool,
}

impl XPlaneSim {
    pub fn new() -> Self {
        let (to_ui_send, from_server_recv) = channel();
        let (to_server_send, from_ui_recv) = channel();
        create_server(to_ui_send, from_ui_recv);
        Self {
            from_server: from_server_recv,
            to_server: to_server_send,
            current_primary: Primary::default(),
            is_connected: false,
        }
    }

    fn handle_from_server(&mut self) -> Result<(), Error> {
        loop {
            match self.from_server.try_recv() {
                Ok(FromXPlaneServer::Connected) => {
                    self.is_connected = true;
                }
                Ok(FromXPlaneServer::Disconnected) => {
                    self.is_connected = false;
                    return Err(Error::Disconnected);
                }
                Ok(FromXPlaneServer::Primary(primary)) => {
                    self.current_primary = primary;
                }
                Err(TryRecvError::Disconnected) => {
                    panic!("X-Plane server stopped without sending disconnect message");
                }
                Err(TryRecvError::Empty) => return Ok(()),
            }
        }
    }
}

impl SimBackend for XPlaneSim {
    fn disconnect(self) {
        self.to_server.send(ToXPlaneServer::Disconnect).unwrap();
    }

    fn set_mode(&mut self, mode: Mode) -> Result<(), Error> {
        self.handle_from_server()?;
        self.to_server.send(ToXPlaneServer::SetMode(mode)).unwrap();
        Ok(())
    }

    fn set_primary(&mut self, primary: Primary) -> Result<(), Error> {
        self.handle_from_server()?;
        self.to_server
            .send(ToXPlaneServer::SetPrimary(primary))
            .unwrap();
        Ok(())
    }

    fn read_primary(&mut self) -> Result<Primary, Error> {
        self.handle_from_server()?;
        Ok(self.current_primary.clone())
    }

    fn ready(&mut self) -> Result<bool, Error> {
        self.handle_from_server()?;
        Ok(self.is_connected)
    }
}

impl Default for XPlaneSim {
    fn default() -> Self {
        Self::new()
    }
}

enum ToXPlaneServer {
    SetPrimary(Primary),
    SetMode(Mode),
    Disconnect,
}

enum FromXPlaneServer {
    Connected,
    Disconnected,
    Primary(Primary),
}

#[derive(Serialize, Deserialize)]
pub struct ChannelServerSide {
    pub to_xplane: IpcSender<ToXPlane>,
    pub from_xplane: IpcReceiver<FromXPlane>,
}

impl ChannelServerSide {
    pub fn disconnect(&mut self) {
        self.to_xplane.send(ToXPlane::SetMode(Mode::Off)).unwrap();
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ToXPlane {
    SetMode(Mode),
    SetPrimary(Primary),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum FromXPlane {
    Primary(Primary),
}

fn create_server(
    from_server: Sender<FromXPlaneServer>,
    to_server: Receiver<ToXPlaneServer>,
) -> JoinHandle<()> {
    thread::spawn(|| {
        let (server, name) = IpcOneShotServer::<ChannelServerSide>::new().unwrap();
        store_server_name(&name);
        let (_server_recv, channels) = server.accept().unwrap();
        Server {
            from_server,
            to_server,
            to_xplane: channels.to_xplane,
            from_xplane: channels.from_xplane,
        }
        .main_loop();
    })
}

pub struct Server {
    from_server: Sender<FromXPlaneServer>,
    to_server: Receiver<ToXPlaneServer>,
    to_xplane: IpcSender<ToXPlane>,
    from_xplane: IpcReceiver<FromXPlane>,
}

impl Server {
    pub fn main_loop(self) {
        self.from_server.send(FromXPlaneServer::Connected).unwrap();
        self.to_xplane
            .send(ToXPlane::SetMode(Mode::Master))
            .unwrap();
        loop {
            match self.from_xplane.try_recv() {
                Ok(FromXPlane::Primary(primary)) => self
                    .from_server
                    .send(FromXPlaneServer::Primary(primary))
                    .unwrap(),
                Err(ipc_channel::TryRecvError::IpcError(IpcError::Disconnected)) => {
                    delete_server_name();
                    self.from_server
                        .send(FromXPlaneServer::Disconnected)
                        .unwrap();
                    return;
                }
                Err(ipc_channel::TryRecvError::Empty) => {}
                Err(err) => {
                    panic!("Error: {err}");
                }
            }
            match self.to_server.try_recv() {
                Ok(ToXPlaneServer::Disconnect) => {
                    delete_server_name();
                    return;
                }
                Ok(ToXPlaneServer::SetMode(mode)) => {
                    self.to_xplane.send(ToXPlane::SetMode(mode)).unwrap();
                }
                Ok(ToXPlaneServer::SetPrimary(primary)) => {
                    self.to_xplane.send(ToXPlane::SetPrimary(primary)).unwrap();
                }
                Err(TryRecvError::Empty) => {}
                Err(err) => {
                    panic!("Error: {err}");
                }
            }
        }
    }
}

pub fn store_server_name(name: &str) {
    fs::create_dir_all(config_dir()).unwrap();
    fs::write(config_dir().join("xplane_ipc"), name).unwrap();
}

pub fn delete_server_name() {
    fs::remove_file(config_dir().join("xplane_ipc")).unwrap();
}
