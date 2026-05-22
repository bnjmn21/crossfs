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

use crate::{FromUi, Mode, ToUi, config_dir, controls::Primary};

pub struct XPlaneAdapter {
    pub server_thread: JoinHandle<()>,
    pub to_ui: Receiver<ToUi>,
    pub from_ui: Sender<FromUi>,
}

impl XPlaneAdapter {
    pub fn new() -> Self {
        let (to_ui_send, to_ui_recv) = channel();
        let (from_ui_send, from_ui_recv) = channel();
        Self {
            to_ui: to_ui_recv,
            from_ui: from_ui_send,
            server_thread: create_server(to_ui_send, from_ui_recv),
        }
    }

    pub fn disconnect(&mut self) {}
}

impl Default for XPlaneAdapter {
    fn default() -> Self {
        Self::new()
    }
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
    UpdatePrimary(Primary),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum FromXPlane {
    Primary(Primary),
}

pub fn create_server(to_ui: Sender<ToUi>, from_ui: Receiver<FromUi>) -> JoinHandle<()> {
    thread::spawn(|| {
        to_ui.send(ToUi::SimLoading).unwrap();
        let (server, name) = IpcOneShotServer::<ChannelServerSide>::new().unwrap();
        store_server_name(&name);
        let (_server_recv, channels) = server.accept().unwrap();
        Server {
            to_ui,
            from_ui,
            to_xplane: channels.to_xplane,
            from_xplane: channels.from_xplane,
        }
        .main_loop();
    })
}

pub struct Server {
    to_ui: Sender<ToUi>,
    from_ui: Receiver<FromUi>,
    to_xplane: IpcSender<ToXPlane>,
    from_xplane: IpcReceiver<FromXPlane>,
}

impl Server {
    pub fn main_loop(self) {
        self.to_ui.send(ToUi::SimConnected).unwrap();
        self.to_xplane
            .send(ToXPlane::SetMode(Mode::Master))
            .unwrap();
        let mut primary = Primary::default();
        loop {
            match self.from_xplane.try_recv() {
                Ok(FromXPlane::Primary(new_primary)) => primary = new_primary,
                Err(ipc_channel::TryRecvError::IpcError(IpcError::Disconnected)) => {
                    delete_server_name();
                    self.to_ui.send(ToUi::SimDisconnected).unwrap();
                    return;
                }
                Err(ipc_channel::TryRecvError::Empty) => {}
                Err(err) => {
                    panic!("Error: {err}");
                }
            }
            match self.from_ui.try_recv() {
                Ok(FromUi::SimDisconnect) => {
                    delete_server_name();
                    return;
                }
                Err(TryRecvError::Empty) => {}
                Err(err) => {
                    panic!("Error: {err}");
                }
            }
            self.to_ui
                .send(ToUi::Info(crate::Info {
                    mode: Mode::Master,
                    lat: primary.lat,
                    long: primary.long,
                    altitude: primary.alt,
                }))
                .unwrap();
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
