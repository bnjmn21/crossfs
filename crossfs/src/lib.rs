use std::{env, fmt::Display, path::PathBuf};

use serde::{Deserialize, Serialize};

pub mod controls;
pub mod xplane;

pub fn config_dir() -> PathBuf {
    cfg_select! {
        windows => env::home_dir().unwrap().join(".crossfs"),
        _ => env::home_dir().unwrap().join(".config/crossfs")
    }
}

pub enum ToUi {
    SimConnected,
    SimLoading,
    SimDisconnected,
    Info(Info),
}

pub enum FromUi {
    SimDisconnect,
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
