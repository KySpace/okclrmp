//! Color math stays in OKLCH until the display / import / export boundary.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Oklch {
    pub l: f64,
    pub c: f64,
    pub h: f64,
}

impl Oklch {
    pub fn new(l: f64, c: f64, h: f64) -> Self {
        Self { l, c, h }
    }
    pub fn channel(self, axis: usize) -> f64 {
        [self.l, self.c, self.h][axis]
    }
    pub fn set_channel(&mut self, axis: usize, v: f64) {
        match axis {
            0 => self.l = v,
            1 => self.c = v,
            _ => self.h = v,
        }
    }
    pub fn finite(self) -> bool {
        self.l.is_finite() && self.c.is_finite() && self.h.is_finite()
    }
    pub fn linear_rgb(self) -> [f64; 3] {
        let a = self.c * self.h.to_radians().cos();
        let b = self.c * self.h.to_radians().sin();
        let l = (self.l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
        let m = (self.l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
        let s = (self.l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
        [
            4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
            -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
            -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
        ]
    }
    pub fn in_gamut(self) -> bool {
        self.finite()
            && self.l >= 0.
            && self.l <= 1.
            && self.c >= 0.
            && self
                .linear_rgb()
                .iter()
                .all(|x| *x >= -1e-6 && *x <= 1.000001)
    }
    pub fn srgb(self, map_chroma: bool) -> [f64; 3] {
        let col = if map_chroma && !self.in_gamut() {
            Self::new(
                self.l.clamp(0., 1.),
                self.c.max(0.).min(max_chroma(self.l.clamp(0., 1.), self.h)),
                self.h,
            )
        } else {
            self
        };
        col.linear_rgb().map(|v| encode(v.clamp(0., 1.)))
    }
    pub fn rgba8(self, map: bool) -> [u8; 4] {
        let rgb = self.srgb(map).map(|v| (v * 255.).round() as u8);
        [rgb[0], rgb[1], rgb[2], 255]
    }
    pub fn hex(self, map: bool) -> String {
        let a = self.rgba8(map);
        format!("#{:02x}{:02x}{:02x}", a[0], a[1], a[2])
    }
    pub fn from_srgb(rgb: [f64; 3]) -> Self {
        let [r, g, b] = rgb.map(decode);
        let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
        let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
        let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
        let light = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
        let a = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
        let b = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
        let c = a.hypot(b);
        Self::new(
            light,
            c,
            if c < 1e-7 {
                0.
            } else {
                b.atan2(a).to_degrees().rem_euclid(360.)
            },
        )
    }
}

pub fn encode(v: f64) -> f64 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    }
}
pub fn decode(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
pub fn max_chroma(l: f64, h: f64) -> f64 {
    if !(0. ..=1.).contains(&l) {
        return 0.;
    }
    let (mut lo, mut hi) = (0., 0.5);
    for _ in 0..24 {
        let mid = (lo + hi) * 0.5;
        if Oklch::new(l, mid, h).in_gamut() {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn srgb_roundtrip_and_reference_red() {
        let red = Oklch::from_srgb([1., 0., 0.]);
        assert!((red.l - 0.62795536).abs() < 1e-6);
        assert!((red.c - 0.2576833).abs() < 1e-6);
        assert!((red.h - 29.233885).abs() < 1e-4);
        for rgb in [
            [0., 0., 0.],
            [1., 1., 1.],
            [0.2, 0.6, 0.9],
            [1., 0., 0.],
            [0., 1., 0.],
        ] {
            let out = Oklch::from_srgb(rgb).srgb(false);
            for i in 0..3 {
                assert!((rgb[i] - out[i]).abs() < 2e-6);
            }
        }
    }
    #[test]
    fn gamut_boundary_and_mapping() {
        for l in [0.1, 0.5, 0.9] {
            for h in [0., 90., 180., 270.] {
                let c = max_chroma(l, h);
                assert!(Oklch::new(l, c, h).in_gamut());
                assert!(!Oklch::new(l, c + 0.001, h).in_gamut());
                assert_eq!(
                    Oklch::new(l, 0.5, h).hex(true),
                    Oklch::new(l, c, h).hex(false)
                );
            }
        }
    }
}
