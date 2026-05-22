use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Deserialize, Serialize)]
#[repr(C)]
pub struct Primary {
    /// degrees
    pub lat: f64,
    /// degrees
    pub long: f64,
    /// AMSL, meters
    pub alt: f64,

    /// phi, degrees
    pub roll: f32,
    /// psi, yaw, relative to true north, degrees
    pub heading: f32,
    /// theta, degrees
    pub pitch: f32,

    /// KIAS, knots
    pub kias: f32,
    /// KTAS, knots
    pub ktas: f32,
    /// VVI, ft/min
    pub vvi: f32,

    /// -1..1
    pub yoke_pitch: f32,
    /// -1..1
    pub yoke_roll: f32,
    /// -1..1
    pub yoke_heading: f32,

    /// -1..0 = Reverse, 0..1 = Normal
    pub throttle_jet: [f32; 4],
}

impl Primary {
    pub fn interpolate(&self, prev: &Self, t: f32) -> Self {
        Self {
            lat: lerp_f64(prev.lat, self.lat, t as f64),
            long: lerp_f64(prev.long, self.long, t as f64),
            alt: lerp_f64(prev.alt, self.alt, t as f64),
            roll: lerp(prev.roll, self.roll, t),
            heading: lerp(prev.heading, self.heading, t),
            pitch: lerp(prev.pitch, self.pitch, t),
            kias: lerp(prev.kias, self.kias, t),
            ktas: lerp(prev.ktas, self.ktas, t),
            vvi: lerp(prev.vvi, self.vvi, t),
            yoke_pitch: lerp(prev.yoke_pitch, self.yoke_pitch, t),
            yoke_roll: lerp(prev.yoke_roll, self.yoke_roll, t),
            yoke_heading: lerp(prev.yoke_heading, self.yoke_heading, t),
            throttle_jet: [
                lerp(prev.throttle_jet[0], self.throttle_jet[0], t),
                lerp(prev.throttle_jet[1], self.throttle_jet[1], t),
                lerp(prev.throttle_jet[2], self.throttle_jet[2], t),
                lerp(prev.throttle_jet[3], self.throttle_jet[3], t),
            ],
        }
    }
}

const fn lerp(a: f32, b: f32, t: f32) -> f32 {
    (a * t) + (b * (1.0 - t))
}

const fn lerp_f64(a: f64, b: f64, t: f64) -> f64 {
    (a * t) + (b * (1.0 - t))
}
