use std::fs::read_to_string;
use std::panic;

use crossfs::controls::Primary;
use crossfs::xplane::{ChannelServerSide, FromXPlane, ToXPlane};
use crossfs::{Mode, config_dir};
use ipc_channel::ipc::{IpcReceiver, IpcSender};
use ipc_channel::{IpcError, TryRecvError};
use xplm::data::borrowed::DataRef;
use xplm::data::{ArrayRead, ArrayReadWrite, DataRead, DataReadWrite, ReadWrite};
use xplm::flight_loop::{FlightLoop, FlightLoopCallback, LoopState};
use xplm::plugin::{Plugin, PluginInfo};
use xplm::{debugln, xplane_plugin};
use xplm_sys::{XPLMLocalToWorld, XPLMWorldToLocal};

struct CrossFs {
    to_xplane: IpcReceiver<ToXPlane>,
    from_xplane: IpcSender<FromXPlane>,
    mode: Mode,
}

struct PrimaryDataRefs {
    override_flightcontrol: DataRef<bool, ReadWrite>,
    local_x: DataRef<f64, ReadWrite>,
    local_y: DataRef<f64, ReadWrite>,
    local_z: DataRef<f64, ReadWrite>,

    phi: DataRef<f32, ReadWrite>,
    psi: DataRef<f32, ReadWrite>,
    theta: DataRef<f32, ReadWrite>,

    indicated_airspeed: DataRef<f32, ReadWrite>,
    true_airspeed: DataRef<f32, ReadWrite>,
    vh_ind_fpm: DataRef<f32, ReadWrite>,

    override_joystick: DataRef<bool, ReadWrite>,
    yoke_pitch_ratio: DataRef<f32, ReadWrite>,
    yoke_roll_ratio: DataRef<f32, ReadWrite>,
    yoke_heading_ratio: DataRef<f32, ReadWrite>,

    throttle_jet_rev_ratio: DataRef<[f32], ReadWrite>,
}

struct CrossFsFlightLoop {
    primary_datarefs: PrimaryDataRefs,
    connection: Option<CrossFs>,
}

impl CrossFsFlightLoop {
    fn new() -> Self {
        Self {
            primary_datarefs: PrimaryDataRefs::new(),
            connection: None,
        }
    }
}

impl FlightLoopCallback for CrossFsFlightLoop {
    fn flight_loop(&mut self, state: &mut LoopState) {
        if let Some(connection) = &mut self.connection {
            match connection.to_xplane.try_recv() {
                Ok(ToXPlane::SetMode(mode)) => {
                    connection.mode = mode;
                    if mode == Mode::Slave {
                        self.primary_datarefs.disable_physics();
                    } else {
                        self.primary_datarefs.enable_physics();
                    }
                }
                Ok(ToXPlane::UpdatePrimary(data)) => {
                    self.primary_datarefs.set(data);
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::IpcError(IpcError::Disconnected)) => {
                    self.connection.take();
                    return;
                }
                Err(TryRecvError::IpcError(err)) => {
                    panic!("Error: {err}");
                }
            }

            if connection.mode == Mode::Master {
                connection
                    .from_xplane
                    .send(FromXPlane::Primary(self.primary_datarefs.get()))
                    .unwrap();
            }
        } else {
            if state.counter() % 20 == 0 {
                let Ok(ipc_name) = read_to_string(config_dir().join("xplane_ipc")) else {
                    return;
                };
                let Ok(channel_sender) = IpcSender::<ChannelServerSide>::connect(ipc_name) else {
                    return;
                };
                let (to_xplane_send, to_xplane_recv) = ipc_channel::ipc::channel().unwrap();
                let (from_xplane_send, from_xplane_recv) = ipc_channel::ipc::channel().unwrap();
                let Ok(()) = channel_sender.send(ChannelServerSide {
                    to_xplane: to_xplane_send,
                    from_xplane: from_xplane_recv,
                }) else {
                    return;
                };
                self.connection = Some(CrossFs {
                    to_xplane: to_xplane_recv,
                    from_xplane: from_xplane_send,
                    mode: Mode::Off,
                });
            }
        }
    }
}

struct CrossFsPlugin {
    flight_loop: FlightLoop,
}

impl Plugin for CrossFsPlugin {
    type Error = std::convert::Infallible;

    fn start() -> Result<Self, Self::Error> {
        let mut flight_loop = FlightLoop::new(CrossFsFlightLoop::new());
        flight_loop.schedule_immediate();
        panic::set_hook(Box::new(|i| {
            debugln!("==== CROSSFS PANIC ====");
            debugln!("{:?}: {:?}", i.location(), i.payload_as_str());
        }));
        Ok(CrossFsPlugin { flight_loop })
    }

    fn enable(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: String::from("CrossFS"),
            signature: String::from("io.github.bnjm21.crossfs"),
            description: String::from("Plugin for connecting to CrossFS"),
        }
    }
}

xplane_plugin!(CrossFsPlugin);

impl PrimaryDataRefs {
    fn new() -> Self {
        Self {
            override_flightcontrol: DataRef::find("sim/operation/override/override_flightcontrol")
                .unwrap()
                .writeable()
                .unwrap(),
            local_x: DataRef::find("sim/flightmodel/position/local_x")
                .unwrap()
                .writeable()
                .unwrap(),
            local_y: DataRef::find("sim/flightmodel/position/local_y")
                .unwrap()
                .writeable()
                .unwrap(),
            local_z: DataRef::find("sim/flightmodel/position/local_z")
                .unwrap()
                .writeable()
                .unwrap(),
            phi: DataRef::find("sim/flightmodel/position/phi")
                .unwrap()
                .writeable()
                .unwrap(),
            psi: DataRef::find("sim/flightmodel/position/psi")
                .unwrap()
                .writeable()
                .unwrap(),
            theta: DataRef::find("sim/flightmodel/position/theta")
                .unwrap()
                .writeable()
                .unwrap(),
            indicated_airspeed: DataRef::find("sim/flightmodel/position/indicated_airspeed")
                .unwrap()
                .writeable()
                .unwrap(),
            true_airspeed: DataRef::find("sim/flightmodel/position/true_airspeed")
                .unwrap()
                .writeable()
                .unwrap(),
            vh_ind_fpm: DataRef::find("sim/flightmodel/position/vh_ind_fpm")
                .unwrap()
                .writeable()
                .unwrap(),
            override_joystick: DataRef::find("sim/operation/override/override_joystick")
                .unwrap()
                .writeable()
                .unwrap(),
            yoke_pitch_ratio: DataRef::find("sim/cockpit2/controls/yoke_pitch_ratio")
                .unwrap()
                .writeable()
                .unwrap(),
            yoke_roll_ratio: DataRef::find("sim/cockpit2/controls/yoke_roll_ratio")
                .unwrap()
                .writeable()
                .unwrap(),
            yoke_heading_ratio: DataRef::find("sim/cockpit2/controls/yoke_heading_ratio")
                .unwrap()
                .writeable()
                .unwrap(),
            throttle_jet_rev_ratio: DataRef::find(
                "sim/cockpit2/engine/actuators/throttle_jet_rev_ratio",
            )
            .unwrap()
            .writeable()
            .unwrap(),
        }
    }

    fn disable_physics(&mut self) {
        self.override_flightcontrol.set(true);
        self.override_joystick.set(true);
    }

    fn enable_physics(&mut self) {
        self.override_flightcontrol.set(false);
        self.override_joystick.set(false);
    }

    fn set(&mut self, data: Primary) {
        let (mut x, mut y, mut z) = (0.0, 0.0, 0.0);
        // SAFETY: Function is safe
        unsafe {
            XPLMWorldToLocal(
                data.lat, data.long, data.alt, &raw mut x, &raw mut y, &raw mut z,
            );
        }

        self.local_x.set(x);
        self.local_y.set(y);
        self.local_z.set(z);
        self.theta.set(data.pitch);
        self.phi.set(data.roll);
        self.psi.set(data.heading);
        self.indicated_airspeed.set(data.kias);
        self.true_airspeed.set(data.ktas);
        self.vh_ind_fpm.set(data.vvi);
        self.yoke_pitch_ratio.set(data.yoke_pitch);
        self.yoke_heading_ratio.set(data.yoke_heading);
        self.yoke_roll_ratio.set(data.yoke_roll);
        self.throttle_jet_rev_ratio.set(&data.throttle_jet);
    }

    fn get(&self) -> Primary {
        let (mut lat, mut long, mut alt) = (0.0, 0.0, 0.0);
        unsafe {
            XPLMLocalToWorld(
                self.local_x.get(),
                self.local_y.get(),
                self.local_z.get(),
                &raw mut lat,
                &raw mut long,
                &raw mut alt,
            );
        }
        let mut throttle_jet = [0.0; 4];
        self.throttle_jet_rev_ratio.get(&mut throttle_jet);
        Primary {
            lat,
            long,
            alt,
            roll: self.phi.get(),
            heading: self.psi.get(),
            pitch: self.theta.get(),
            kias: self.indicated_airspeed.get(),
            ktas: self.true_airspeed.get(),
            vvi: self.vh_ind_fpm.get(),
            yoke_pitch: self.yoke_pitch_ratio.get(),
            yoke_roll: self.yoke_roll_ratio.get(),
            yoke_heading: self.yoke_heading_ratio.get(),
            throttle_jet,
        }
    }
}
