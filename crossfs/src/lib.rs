use std::{
    env,
    fmt::{Debug, Display},
    path::PathBuf,
    sync::mpsc::{Receiver, Sender, channel},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    controls::Primary,
    net::{Client, FromServerPacket, ToServerPacket},
};

pub mod controls;
pub mod msfs;
pub mod net;
pub mod xplane;

const SEND_PRIMARY_INTERVAL: Duration = Duration::from_millis(200);
const INTERPOLATION_TIME: Duration = Duration::from_millis(250);

pub struct CrossFs {
    current_primary_recv: Primary,
    previous_primary_recv: Primary,
    primary_recv_time: Instant,
    last_send: Instant,
    tcp_send: Sender<ToServerPacket>,
    tcp_recv: Receiver<FromServerPacket>,
    sim: Box<dyn SimBackend>,
    master: bool,
    first_packet_recieved: Option<Instant>,
    server_mode_set: bool,
}

impl CrossFs {
    pub fn new(mut sim: Box<dyn SimBackend>, mut tcp_client: Client, master: bool) -> Self {
        let (tcp_send, send) = channel();
        let (recv, tcp_recv) = channel();
        thread::spawn(move || tcp_client.run(send, recv));

        if !sim.ready().unwrap() {
            println!("Waiting on sim.");
        }
        while !sim.ready().unwrap() {
            thread::sleep(Duration::from_millis(100));
        }
        println!("Connected to sim!");

        if master {
            sim.set_mode(Mode::Master).unwrap();
        }
        // if !master, we wait until we set the sim mode to Mode::Slave,
        // because initially the position is 0° N 0° E until the first primary is received.
        // If we immediatly set the mode to slave, the plane would get temporarily teleported to 0, 0.

        Self {
            current_primary_recv: Primary::default(),
            previous_primary_recv: Primary::default(),
            primary_recv_time: Instant::now(),
            last_send: Instant::now(),
            tcp_send,
            tcp_recv,
            sim,
            master,
            first_packet_recieved: None,
            server_mode_set: false,
        }
    }

    pub fn tick(&mut self) -> Result<(), Error> {
        if self.master {
            self.tick_master()?;
        } else {
            self.tick_slave()?;
        }
        Ok(())
    }

    fn tick_master(&mut self) -> Result<(), Error> {
        if Instant::now() - self.last_send > SEND_PRIMARY_INTERVAL {
            let primary = self.sim.read_primary()?;
            self.tcp_send
                .send(ToServerPacket::Primary(primary))
                .unwrap();
            self.last_send = Instant::now();
        }
        Ok(())
    }

    fn tick_slave(&mut self) -> Result<(), Error> {
        while let Ok(packet) = self.tcp_recv.try_recv() {
            #[allow(clippy::single_match, reason = "will add more packets later")]
            match packet {
                FromServerPacket::Primary(primary) => {
                    let now = Instant::now();
                    self.previous_primary_recv = self.get_interpolated();
                    self.current_primary_recv = primary;
                    self.primary_recv_time = now;
                    if !self.server_mode_set && self.first_packet_recieved.is_none() {
                        self.first_packet_recieved = Some(now);
                    }
                    if !self.server_mode_set
                        && let Some(time) = &self.first_packet_recieved
                        && now - *time > INTERPOLATION_TIME
                    {
                        self.server_mode_set = true;
                        self.sim.set_mode(Mode::Slave).unwrap();
                    }
                }
                _ => {}
            }
        }
        self.sim.set_primary(self.get_interpolated()).unwrap();
        Ok(())
    }

    fn get_interpolated(&self) -> Primary {
        let last_recv = Instant::now() - self.primary_recv_time;
        let t = (last_recv.as_secs_f32() / INTERPOLATION_TIME.as_secs_f32()).clamp(0.0, 1.0);
        self.current_primary_recv
            .interpolate(&self.previous_primary_recv, t)
    }
}

pub fn config_dir() -> PathBuf {
    cfg_select! {
        windows => env::home_dir().unwrap().join(".crossfs"),
        _ => env::home_dir().unwrap().join(".config/crossfs")
    }
}

pub trait SimBackend {
    fn disconnect(self);
    fn set_mode(&mut self, mode: Mode) -> Result<(), Error>;
    fn set_primary(&mut self, primary: Primary) -> Result<(), Error>;
    fn read_primary(&mut self) -> Result<Primary, Error>;
    fn ready(&mut self) -> Result<bool, Error>;
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Simulator disconnected.")]
    Disconnected,
    #[error("Packet too large.")]
    PacketTooLarge,
    #[error("Wrong password.")]
    WrongPassword,
    #[error("A different master is already connected.")]
    DifferentMasterAlreadyConnected,
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Error while deserializing packet: {0}")]
    Deserialization(#[from] postcard::Error),
}

#[derive(Debug, Default)]
pub struct Info {
    pub mode: Mode,
    pub lat: f64,
    pub long: f64,
    pub altitude: f64,
}

impl Display for Info {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Mode: {}, Lat: {:.6}°, Lon: {:.6}°, Alt: {:.2}m",
            self.mode, self.lat, self.long, self.altitude
        )
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum Mode {
    #[default]
    Off,
    Master,
    Slave,
}

impl Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Off => write!(f, "Disconnected"),
            Mode::Master => write!(f, "Master"),
            Mode::Slave => write!(f, "Slave"),
        }
    }
}
