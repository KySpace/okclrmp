//! Output preview color transforms, sampling, and reversible parameter positioning.
use crate::color::{Oklch, decode, encode};

/// Experimental single-color ink for an OKLCH background.
pub fn annotation_ink(background: Oklch) -> Oklch {
    let (l_thres, l_min, l_max) = (0.65, 0.87, 0.45);
    let frac_l_bias = if background.l > l_thres { 
        (1. - background.l) / (1. - l_thres) 
    } else { 
        background.l / l_thres 
    }; // frac = 0 means black or white background, frac = 1 means at grayish threshold
    let c_max = if background.l > l_thres { 0.07 } else { 0.11 };
    let l = if background.l > l_thres { l_max * (1. - frac_l_bias) } else { l_min + (1. - l_min) * frac_l_bias };
    Oklch::new(
        l,
        c_max * (1. - frac_l_bias),
        (background.h + 120.).rem_euclid(360.),
    )
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fallback {
    #[default]
    Original,
    Cmyk,
    Grayscale,
}
impl Fallback {
    pub const ALL: [Self; 3] = [Self::Original, Self::Cmyk, Self::Grayscale];
    pub fn label(self) -> &'static str {
        match self {
            Self::Original => "Original sRGB",
            Self::Cmyk => "CMYK · SWOP",
            Self::Grayscale => "Grayscale",
        }
    }
    pub fn short_label(self) -> &'static str {
        match self {
            Self::Original => "RGB",
            Self::Cmyk => "CMYK",
            Self::Grayscale => "Gray",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SamplingPreview {
    #[default]
    Continuous,
    Output,
    Samples(usize),
}
impl SamplingPreview {
    pub const COUNTS: [usize; 14] = [2, 3, 4, 5, 8, 9, 16, 32, 64, 128, 256, 512, 1024, 4096];
    pub fn count(self, output_count: usize) -> Option<usize> {
        match self {
            Self::Continuous => None,
            Self::Output => Some(output_count.clamp(2, 4096)),
            Self::Samples(n) => Some(n.clamp(2, 4096)),
        }
    }
    pub fn label(self, output_count: usize) -> String {
        match self {
            Self::Continuous => "Continuous".into(),
            Self::Output => format!("Output ({output_count})"),
            Self::Samples(n) => format!("{n} steps"),
        }
    }
    /// Equal-width swatches represent evenly sampled points including both ends.
    pub fn parameter(self, fraction: f64, output_count: usize, reverse: bool) -> f64 {
        let fraction = fraction.clamp(0., 1.);
        let value = if let Some(n) = self.count(output_count) {
            let index = ((fraction * n as f64).floor() as usize).min(n - 1);
            index as f64 / (n - 1) as f64
        } else {
            fraction
        };
        output_position(value, reverse)
    }
    /// A sampled selection is marked at its swatch center. Arbitrary curve
    /// inspection points select the nearest represented sample.
    pub fn position(self, parameter: f64, output_count: usize, reverse: bool) -> f64 {
        let value = output_position(parameter, reverse);
        if let Some(n) = self.count(output_count) {
            let index = (value * (n - 1) as f64).round();
            (index + 0.5) / n as f64
        } else {
            value
        }
    }
}
pub fn output_position(parameter: f64, reverse: bool) -> f64 {
    let value = parameter.clamp(0., 1.);
    if reverse { 1. - value } else { value }
}
pub fn grayscale(rgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = rgb.map(decode);
    [encode((0.2126 * r + 0.7152 * g + 0.0722 * b).clamp(0., 1.)); 3]
}
pub fn parameter_from_position(x: f64, left: f64, width: f64) -> Option<f64> {
    if !x.is_finite() || !left.is_finite() || !width.is_finite() || width <= 0. {
        return None;
    }
    Some(((x - left) / width).clamp(0., 1.))
}

#[cfg(windows)]
mod icc {
    use std::{
        ffi::OsString,
        os::windows::ffi::{OsStrExt, OsStringExt},
        path::Path,
    };
    use windows::{
        Win32::UI::ColorSystem::*,
        core::{PCWSTR, PWSTR},
    };

    struct Profile(isize);
    impl Profile {
        fn open(path: &Path) -> Result<Self, String> {
            let mut name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            let profile = PROFILE {
                dwType: PROFILE_FILENAME,
                pProfileData: name.as_mut_ptr().cast(),
                cbDataSize: (name.len() * 2) as u32,
            };
            // The filename buffer remains alive until OpenColorProfileW returns.
            let handle = unsafe { OpenColorProfileW(&profile, PROFILE_READ, 1, 3) };
            if handle == 0 {
                return Err(format!(
                    "Cannot open {}: {}",
                    path.display(),
                    windows::core::Error::from_thread()
                ));
            }
            Ok(Self(handle))
        }
    }
    impl Drop for Profile {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseColorProfile(Some(self.0));
            }
        }
    }
    struct Transform(isize);
    impl Transform {
        fn new(source: &Profile, target: &Profile) -> Result<Self, String> {
            let handle = unsafe {
                CreateMultiProfileTransform(
                    &[source.0, target.0],
                    &[INTENT_RELATIVE_COLORIMETRIC; 2],
                    BEST_MODE | USE_RELATIVE_COLORIMETRIC,
                    0,
                )
            };
            if handle == 0 {
                return Err(format!(
                    "Cannot create CMYK preview: {}",
                    windows::core::Error::from_thread()
                ));
            }
            Ok(Self(handle))
        }
    }
    impl Drop for Transform {
        fn drop(&mut self) {
            unsafe {
                let _ = DeleteColorTransform(self.0);
            }
        }
    }
    pub struct CmykProof {
        to_cmyk: Transform,
        to_rgb: Transform,
        _profiles: [Profile; 2],
    }
    impl CmykProof {
        pub fn new() -> Result<Self, String> {
            let mut bytes = 0;
            unsafe {
                let _ = GetColorDirectoryW(PCWSTR::null(), None, &mut bytes);
            }
            if bytes == 0 {
                return Err("Windows color profile directory is unavailable".into());
            }
            let mut buffer = vec![0u16; (bytes as usize).div_ceil(2)];
            unsafe {
                GetColorDirectoryW(PCWSTR::null(), Some(PWSTR(buffer.as_mut_ptr())), &mut bytes)
                    .ok()
                    .map_err(|e| e.to_string())?;
            }
            let end = buffer.iter().position(|v| *v == 0).unwrap_or(buffer.len());
            let directory = std::path::PathBuf::from(OsString::from_wide(&buffer[..end]));
            let rgb = Profile::open(&directory.join("sRGB Color Space Profile.icm"))?;
            let cmyk = Profile::open(&directory.join("RSWOP.icm"))?;
            let to_cmyk = Transform::new(&rgb, &cmyk)?;
            let to_rgb = Transform::new(&cmyk, &rgb)?;
            Ok(Self {
                to_cmyk,
                to_rgb,
                _profiles: [rgb, cmyk],
            })
        }
        pub fn convert(&self, source: &[[f64; 3]]) -> Result<Vec<[f64; 3]>, String> {
            if source.is_empty() {
                return Ok(vec![]);
            }
            if source.len() > u32::MAX as usize
                || source
                    .iter()
                    .flatten()
                    .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
            {
                return Err("CMYK preview needs finite sRGB values in [0, 1]".into());
            }
            let input: Vec<_> = source
                .iter()
                .map(|rgb| {
                    let [r, g, b] = rgb.map(|v| (v * 65535.).round() as u16);
                    let mut color = COLOR::default();
                    color.rgb = RGBCOLOR {
                        red: r,
                        green: g,
                        blue: b,
                    };
                    color
                })
                .collect();
            let mut cmyk = vec![COLOR::default(); source.len()];
            let mut output = vec![COLOR::default(); source.len()];
            // Both buffers contain exactly n COLOR elements. The successful
            // second translation initializes the RGB member read below.
            unsafe {
                TranslateColors(
                    self.to_cmyk.0,
                    input.as_ptr(),
                    source.len() as u32,
                    COLOR_RGB,
                    cmyk.as_mut_ptr(),
                    COLOR_CMYK,
                )
                .ok()
                .map_err(|e| e.to_string())?;
                TranslateColors(
                    self.to_rgb.0,
                    cmyk.as_ptr(),
                    source.len() as u32,
                    COLOR_CMYK,
                    output.as_mut_ptr(),
                    COLOR_RGB,
                )
                .ok()
                .map_err(|e| e.to_string())?;
            }
            Ok(output
                .iter()
                .map(|v| {
                    let rgb = unsafe { v.rgb };
                    [rgb.red, rgb.green, rgb.blue].map(|v| v as f64 / 65535.)
                })
                .collect())
        }
    }
}
#[cfg(windows)]
pub use icc::CmykProof;
#[cfg(not(windows))]
pub struct CmykProof;
#[cfg(not(windows))]
impl CmykProof {
    pub fn new() -> Result<Self, String> {
        Err("CMYK preview requires Windows and its SWOP profile".into())
    }
    pub fn convert(&self, _: &[[f64; 3]]) -> Result<Vec<[f64; 3]>, String> {
        Err("CMYK preview is unavailable".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sampled_preview_cells_scrub_and_invert_consistently() {
        for mode in [
            SamplingPreview::Continuous,
            SamplingPreview::Samples(4),
            SamplingPreview::Output,
        ] {
            for reverse in [false, true] {
                if let Some(n) = mode.count(16) {
                    for i in 0..n {
                        let center = (i as f64 + 0.5) / n as f64;
                        let t = mode.parameter(center, 16, reverse);
                        assert!(
                            (t - output_position(i as f64 / (n - 1) as f64, reverse)).abs() < 1e-12
                        );
                        assert!((mode.position(t, 16, reverse) - center).abs() < 1e-12);
                    }
                } else {
                    assert_eq!(
                        mode.parameter(0.25, 16, reverse),
                        if reverse { 0.75 } else { 0.25 }
                    );
                    assert_eq!(
                        mode.position(mode.parameter(0.25, 16, reverse), 16, reverse),
                        0.25
                    );
                }
                assert_eq!(
                    mode.parameter(-1., 16, reverse),
                    if reverse { 1. } else { 0. }
                );
                assert_eq!(
                    mode.parameter(2., 16, reverse),
                    if reverse { 0. } else { 1. }
                );
            }
        }
        assert_eq!(SamplingPreview::Output.count(64), Some(64));
        assert_eq!(SamplingPreview::Samples(4).count(64), Some(4));
    }
    #[test]
    fn grayscale_preserves_luminance_and_bar_input_clamps_to_endpoints() {
        for rgb in [
            [0., 0., 0.],
            [1., 1., 1.],
            [1., 0., 0.],
            [0., 1., 0.],
            [0.2, 0.4, 0.8],
        ] {
            let gray = grayscale(rgb);
            assert_eq!(gray[0], gray[1]);
            assert_eq!(gray[1], gray[2]);
            let [r, g, b] = rgb.map(decode);
            assert!((decode(gray[0]) - (0.2126 * r + 0.7152 * g + 0.0722 * b)).abs() < 1e-12);
        }
        assert_eq!(parameter_from_position(60., 10., 100.), Some(0.5));
        assert_eq!(parameter_from_position(-20., 10., 100.), Some(0.));
        assert_eq!(parameter_from_position(140., 10., 100.), Some(1.));
        assert_eq!(parameter_from_position(60., 10., 0.), None);
        assert_eq!(parameter_from_position(f64::NAN, 10., 100.), None);
    }
    #[test]
    #[cfg(windows)]
    #[ignore = "requires Windows sRGB and RSWOP ICC profiles"]
    fn cmyk_soft_proof_uses_the_installed_profile() {
        let proof = CmykProof::new().unwrap();
        let input = [
            [0., 0., 0.],
            [1., 1., 1.],
            [1., 0., 0.],
            [0., 1., 0.],
            [0., 0., 1.],
        ];
        let output = proof.convert(&input).unwrap();
        assert_eq!(output.len(), input.len());
        assert!(
            output
                .iter()
                .flatten()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v))
        );
        assert!(
            output
                .iter()
                .zip(input)
                .skip(2)
                .any(|(a, b)| a.iter().zip(b).any(|(x, y)| (x - y).abs() > 0.05))
        );
        assert!(output[0].iter().sum::<f64>() < output[1].iter().sum::<f64>());
        assert_eq!(proof.convert(&input).unwrap(), output);
        assert!(proof.convert(&[[f64::NAN, 0., 0.]]).is_err());
    }
}
