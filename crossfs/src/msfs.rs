use std::{
    ffi::{CStr, c_double, c_void},
    ptr::null_mut,
};

use simconnect::*;

use crate::{Error, Mode, SimBackend, controls::Primary};

pub struct MsfsSim {
    sim_connect: *mut c_void,
    mode: Mode,
    frozen: FreezingSimVars,
    primary: Primary,
}

const S_OK: HRESULT = 0;

impl MsfsSim {
    pub fn new() -> Option<Self> {
        let mut sim_connect = null_mut();
        let result = unsafe {
            SimConnect_Open(
                &raw mut sim_connect,
                c"CrossFS".as_ptr(),
                null_mut(),
                0,
                null_mut(),
                0,
            )
        };
        if result != S_OK {
            panic!("SimConnect_Open returned error: {result:x}");
        }

        PrimarySimVars::build_data_def(sim_connect);
        FreezingSimVars::build_data_def(sim_connect);
        unsafe {
            SimConnect_RequestDataOnSimObject(
                sim_connect,
                RequestId::Primary as u32,
                DataDefinition::Primary as u32,
                SIMCONNECT_OBJECT_ID_USER,
                SIMCONNECT_PERIOD_SIMCONNECT_PERIOD_SIM_FRAME,
                0,
                0,
                0,
                0,
            );
            SimConnect_RequestDataOnSimObject(
                sim_connect,
                RequestId::Freezing as u32,
                DataDefinition::Freezing as u32,
                SIMCONNECT_OBJECT_ID_USER,
                SIMCONNECT_PERIOD_SIMCONNECT_PERIOD_SIM_FRAME,
                0,
                0,
                0,
                0,
            );
        }
        Some(Self {
            sim_connect,
            mode: Mode::Off,
            frozen: FreezingSimVars::default(),
            primary: Primary::default(),
        })
    }

    fn set_sim_frozen(&self, frozen: bool) {
        if (self.frozen.is_latitude_longitude_freeze_on != 0.0) ^ frozen {
            send_event(
                self.sim_connect,
                EventId::FreezeLatitudeLongitudeToggle,
                c"FREEZE_LATITUDE_LONGITUDE_TOGGLE",
            );
        }
        if (self.frozen.is_altitude_freeze_on != 0.0) ^ frozen {
            send_event(
                self.sim_connect,
                EventId::FreezeAltitudeToggle,
                c"FREEZE_ALTITUDE_TOGGLE",
            );
        }
        if (self.frozen.is_attitude_freeze_on != 0.0) ^ frozen {
            send_event(
                self.sim_connect,
                EventId::FreezeAttitudeToggle,
                c"FREEZE_ATTITUDE_TOGGLE",
            );
        }
    }

    pub fn close(self) {
        unsafe {
            SimConnect_Close(self.sim_connect);
        }
    }

    pub fn process_messages(&mut self) {
        loop {
            let mut data: *mut SIMCONNECT_RECV = null_mut();
            let mut len = 0;
            unsafe {
                let res = SimConnect_GetNextDispatch(self.sim_connect, &raw mut data, &raw mut len);
                if res != S_OK {
                    panic!("SimConnect_GetNextDispatch failed with error code {res:x}");
                }
                let id = (*data).dwID as i32;
                match id {
                    SIMCONNECT_RECV_ID_SIMCONNECT_RECV_ID_NULL => return,
                    SIMCONNECT_RECV_ID_SIMCONNECT_RECV_ID_EXCEPTION => {
                        let data = data as *const SIMCONNECT_RECV_EXCEPTION;
                        let exception = (*data).dwException;
                        panic!("SimConnect Exception: {:x}", exception);
                    }
                    SIMCONNECT_RECV_ID_SIMCONNECT_RECV_ID_OPEN => {
                        println!("Successfully connected to MSFS")
                    }
                    SIMCONNECT_RECV_ID_SIMCONNECT_RECV_ID_SIMOBJECT_DATA => {
                        self.handle_recv_simobject_data(data as _);
                    }
                    _ => {}
                }
            }
        }
    }

    unsafe fn handle_recv_simobject_data(&mut self, data: *const SIMCONNECT_RECV_SIMOBJECT_DATA) {
        const PRIMARY: u32 = RequestId::Primary as u32;
        const FREEZING: u32 = RequestId::Freezing as u32;
        unsafe {
            match (*data).dwRequestID {
                PRIMARY => {
                    let simobj_data: &PrimarySimVars = get_simobj_data(data);
                    self.primary = simobj_data.to_primary();
                }
                FREEZING => {
                    self.frozen = get_simobj_data::<FreezingSimVars>(data).clone();
                }
                _ => unreachable!(),
            }
        }
    }
}

unsafe fn get_simobj_data<'a, T>(data: *const SIMCONNECT_RECV_SIMOBJECT_DATA) -> &'a T {
    unsafe {
        let p = &raw const (*data).dwData as *const T;
        &*p
    }
}

impl SimBackend for MsfsSim {
    fn disconnect(self) {
        self.close();
    }

    fn set_mode(&mut self, mode: Mode) -> Result<(), Error> {
        self.mode = mode;
        if mode == Mode::Slave {
            self.set_sim_frozen(true);
        } else {
            self.set_sim_frozen(false);
        }
        Ok(())
    }

    fn read_primary(&mut self) -> Result<Primary, Error> {
        self.process_messages();
        Ok(self.primary.clone())
    }

    fn set_primary(&mut self, primary: Primary) -> Result<(), Error> {
        let mut primary_sim_vars = PrimarySimVars::from_primary(primary);
        self.process_messages();
        unsafe {
            let res = SimConnect_SetDataOnSimObject(
                self.sim_connect,
                DataDefinition::Primary as u32,
                SIMCONNECT_OBJECT_ID_USER,
                0,
                0,
                size_of::<PrimarySimVars>() as u32,
                &raw mut primary_sim_vars as *mut c_void,
            );
            if res != S_OK {
                panic!("SimConnect_SetDataOnSimObject failed with error {res:x}");
            }
        }
        Ok(())
    }

    fn ready(&mut self) -> Result<bool, Error> {
        self.process_messages();
        Ok(true)
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u32)]
enum DataDefinition {
    Primary,
    Freezing,
}

#[derive(Debug, Clone, Copy)]
#[allow(clippy::enum_variant_names)]
#[repr(u32)]
enum EventId {
    FreezeLatitudeLongitudeToggle,
    FreezeAltitudeToggle,
    FreezeAttitudeToggle,
}

#[derive(Debug, Clone, Copy)]
#[repr(u32)]
enum RequestId {
    Primary,
    Freezing,
}

#[derive(Debug, Clone)]
#[repr(C)]
struct PrimarySimVars {
    /// radians
    plane_latitude: c_double,
    /// radians
    plane_longitude: c_double,
    /// feet
    plane_altitude: c_double,
    /// radians (!!!)
    plane_bank_degrees: c_double,
    /// radians (!!!)
    plane_heading_degrees_true: c_double,
    /// radians (!!!)
    plane_pitch_degrees: c_double,
    /// knots
    airspeed_indicated: c_double,
    /// knots
    airspeed_true: c_double,
    /// ft/sec
    vertical_speed: c_double,
    /// -16k..0, roll
    yoke_x_position: c_double,
    /// -16k..0, pitch
    yoke_y_position: c_double,
    /// -16k..0, heading
    rudder_position: c_double,
    /// percent
    general_eng_throttle_lever_position: [c_double; 4],
}

impl PrimarySimVars {
    fn to_primary(&self) -> Primary {
        Primary {
            lat: self.plane_latitude.to_degrees(),
            long: self.plane_longitude.to_degrees(),
            alt: ft_to_m(self.plane_altitude),
            roll: self.plane_bank_degrees.to_degrees() as f32,
            heading: self.plane_heading_degrees_true.to_degrees() as f32,
            pitch: self.plane_pitch_degrees.to_degrees() as f32,
            kias: self.airspeed_indicated as f32,
            ktas: self.airspeed_true as f32,
            vvi: self.vertical_speed as f32 * 60.0,
            yoke_pitch: self.yoke_y_position as f32 / 8192.0 + 1.0,
            yoke_roll: self.yoke_x_position as f32 / 8192.0 + 1.0,
            yoke_heading: self.rudder_position as f32 / 8192.0 + 1.0,
            throttle: self
                .general_eng_throttle_lever_position
                .map(|v| v as f32 / 100.0),
        }
    }

    fn from_primary(primary: Primary) -> Self {
        Self {
            plane_latitude: primary.lat.to_radians(),
            plane_longitude: primary.long.to_radians(),
            plane_altitude: m_to_ft(primary.alt),
            plane_bank_degrees: primary.roll.to_radians() as f64,
            plane_heading_degrees_true: primary.heading.to_radians() as f64,
            plane_pitch_degrees: primary.pitch.to_radians() as f64,
            airspeed_indicated: primary.kias as f64,
            airspeed_true: primary.ktas as f64,
            vertical_speed: primary.vvi as f64 / 60.0,
            yoke_x_position: (primary.yoke_roll as f64 - 1.0) * 8192.0,
            yoke_y_position: (primary.yoke_pitch as f64 - 1.0) * 8192.0,
            rudder_position: (primary.yoke_heading as f64 - 1.0) * 8192.0,
            general_eng_throttle_lever_position: primary.throttle.map(|v| v as f64 * 100.0),
        }
    }

    fn build_data_def(sim_connect: *mut c_void) {
        const DEF: DataDefinition = DataDefinition::Primary;
        add_to_data_def(sim_connect, DEF, c"PLANE LATITUDE", c"radians");
        add_to_data_def(sim_connect, DEF, c"PLANE LONGITUDE", c"radians");
        add_to_data_def(sim_connect, DEF, c"PLANE ALTITUDE", c"feet");
        add_to_data_def(sim_connect, DEF, c"PLANE BANK DEGREES", c"radians");
        add_to_data_def(sim_connect, DEF, c"PLANE HEADING DEGREES TRUE", c"radians");
        add_to_data_def(sim_connect, DEF, c"PLANE PITCH DEGREES", c"radians");
        add_to_data_def(sim_connect, DEF, c"AIRSPEED INDICATED", c"knots");
        add_to_data_def(sim_connect, DEF, c"AIRSPEED TRUE", c"knots");
        add_to_data_def(sim_connect, DEF, c"VERTICAL SPEED", c"feet/second");
        add_to_data_def(sim_connect, DEF, c"YOKE X POSITION", c"position");
        add_to_data_def(sim_connect, DEF, c"YOKE Y POSITION", c"position");
        add_to_data_def(sim_connect, DEF, c"RUDDER POSITION", c"position");
        add_to_data_def(
            sim_connect,
            DEF,
            c"GENERAL ENG THROTTLE LEVER POSITION:0",
            c"percent",
        );
        add_to_data_def(
            sim_connect,
            DEF,
            c"GENERAL ENG THROTTLE LEVER POSITION:1",
            c"percent",
        );
        add_to_data_def(
            sim_connect,
            DEF,
            c"GENERAL ENG THROTTLE LEVER POSITION:2",
            c"percent",
        );
        add_to_data_def(
            sim_connect,
            DEF,
            c"GENERAL ENG THROTTLE LEVER POSITION:3",
            c"percent",
        );
    }
}

#[derive(Debug, Default, Clone)]
#[repr(C)]
struct FreezingSimVars {
    is_altitude_freeze_on: f64,
    is_attitude_freeze_on: f64,
    is_latitude_longitude_freeze_on: f64,
}

impl FreezingSimVars {
    fn build_data_def(sim_connect: *mut c_void) {
        const DEF: DataDefinition = DataDefinition::Freezing;
        add_to_data_def(sim_connect, DEF, c"IS ALTITUDE FREEZE ON", c"Bool");
        add_to_data_def(sim_connect, DEF, c"IS ATTITUDE FREEZE ON", c"Bool");
        add_to_data_def(
            sim_connect,
            DEF,
            c"IS LATITUDE LONGITUDE FREEZE ON",
            c"Bool",
        );
    }
}

fn add_to_data_def(
    sim_connect: *mut c_void,
    def: DataDefinition,
    name: &CStr,
    unit: &CStr,
) -> bool {
    unsafe {
        SimConnect_AddToDataDefinition(
            sim_connect,
            def as u32,
            name.as_ptr(),
            unit.as_ptr(),
            SIMCONNECT_DATATYPE_SIMCONNECT_DATATYPE_FLOAT64,
            0.0,
            SIMCONNECT_UNUSED,
        ) == S_OK
    }
}

fn send_event(sim_connect: *mut c_void, id: EventId, name: &CStr) {
    unsafe {
        SimConnect_MapClientEventToSimEvent(sim_connect, id as u32, name.as_ptr());
        SimConnect_TransmitClientEvent(
            sim_connect,
            SIMCONNECT_OBJECT_ID_USER,
            id as u32,
            0,
            SIMCONNECT_GROUP_PRIORITY_DEFAULT,
            SIMCONNECT_EVENT_FLAG_GROUPID_IS_PRIORITY,
        );
    }
}

fn m_to_ft(m: f64) -> f64 {
    m * 0.3048
}

fn ft_to_m(m: f64) -> f64 {
    m / 0.3048
}
