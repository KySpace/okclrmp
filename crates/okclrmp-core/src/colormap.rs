use crate::{Oklch, Project, model::Curve};
use serde::{Deserialize, Deserializer};
use std::{fmt, fs::File, io, path::Path};

/// An immutable, compiled colormap, independent of the app and renderer.
///
/// Positions follow the output colorbar: 0 is its left end, 1 its right end.
/// Saved inversion is applied to every sampling method. RGB methods also use
/// the saved chroma-mapping/clipping policy; OKLCH sampling remains unmapped.
/// The project sample count does not restrict continuous sampling or LUT size.
/// A colormap is `Clone + Send + Sync` and can be shared between render threads.
#[derive(Clone, Debug)]
pub struct Colormap {
    curve: Curve,
    map_chroma: bool,
    reversed: bool,
}

/// Failure to open, deserialize, or compile a project.
#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    Json(serde_json::Error),
    InvalidProject(String),
}
impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "Could not read colormap project: {error}"),
            Self::Json(error) => write!(f, "Could not deserialize colormap project: {error}"),
            Self::InvalidProject(error) => write!(f, "Invalid colormap project: {error}"),
        }
    }
}
impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::InvalidProject(_) => None,
        }
    }
}

/// Invalid sampling input or a non-finite result at a particular position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SampleError {
    NonFiniteParameter,
    /// `t` is the source curve parameter, not the position in inverted output.
    UndefinedColor {
        t: f64,
    },
    InvalidSampleCount,
}
impl fmt::Display for SampleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteParameter => write!(f, "Colormap position must be finite"),
            Self::UndefinedColor { t } => write!(f, "Colormap color is not finite at t = {t}"),
            Self::InvalidSampleCount => {
                write!(f, "A color lookup table needs at least two samples")
            }
        }
    }
}
impl std::error::Error for SampleError {}

impl Colormap {
    /// Load an existing app project file. No window or GPU is created.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, LoadError> {
        Self::from_reader(File::open(path).map_err(LoadError::Io)?)
    }

    /// Deserialize project JSON from a reader and compile its active curve.
    pub fn from_reader(reader: impl io::Read) -> Result<Self, LoadError> {
        let project = serde_json::from_reader(reader).map_err(LoadError::Json)?;
        Self::from_project(&project)
    }

    /// Deserialize project JSON and compile its active curve.
    pub fn from_json(json: &str) -> Result<Self, LoadError> {
        let project = serde_json::from_str(json).map_err(LoadError::Json)?;
        Self::from_project(&project)
    }

    /// Compile an app project using the app's migration and binding rules.
    /// The supplied project is unchanged. View metadata is not retained.
    pub fn from_project(project: &Project) -> Result<Self, LoadError> {
        let mut project = project.clone();
        project
            .migrate_spline()
            .map_err(LoadError::InvalidProject)?;
        project
            .sync_bound_slices()
            .map_err(LoadError::InvalidProject)?;
        let curve = project.compile().map_err(LoadError::InvalidProject)?;
        Ok(Self {
            curve,
            map_chroma: project.map_chroma,
            reversed: project.reverse_output,
        })
    }

    /// Sample original OKLCH coordinates (L normally 0–1, H in degrees).
    /// Finite positions outside [0, 1] clamp to the nearest endpoint.
    /// NaN/infinity and non-finite curve results return an error.
    pub fn sample(&self, t: f64) -> Result<Oklch, SampleError> {
        self.color_at(self.source_parameter(t)?)
    }

    /// Sample gamma-encoded sRGB channels in [0, 1], using the saved RGB policy.
    /// For linear-light RGB, decode these channels with [`crate::color::decode`].
    pub fn sample_srgb(&self, t: f64) -> Result<[f64; 3], SampleError> {
        self.srgb_at(self.source_parameter(t)?)
    }

    /// Sample opaque, gamma-encoded sRGB bytes [R, G, B, 255].
    pub fn sample_rgba8(&self, t: f64) -> Result<[u8; 4], SampleError> {
        self.rgba8_at(self.source_parameter(t)?)
    }

    /// Generate an evenly spaced RGBA8 lookup table, including both endpoints.
    /// Counts below two return an error. Output ordering and RGB conversion
    /// match the app's sampled output; LUT size is independent of its setting.
    pub fn rgba8_lut(&self, count: usize) -> Result<Vec<[u8; 4]>, SampleError> {
        if count < 2 {
            return Err(SampleError::InvalidSampleCount);
        }
        (0..count)
            .map(|i| {
                // Reverse indices instead of subtracting a rounded parameter
                // from 1, preserving the app's exact exported sample positions.
                let i = if self.reversed { count - 1 - i } else { i };
                self.rgba8_at(i as f64 / (count - 1) as f64)
            })
            .collect()
    }

    fn source_parameter(&self, t: f64) -> Result<f64, SampleError> {
        if !t.is_finite() {
            return Err(SampleError::NonFiniteParameter);
        }
        let t = t.clamp(0., 1.);
        Ok(if self.reversed { 1. - t } else { t })
    }

    fn color_at(&self, t: f64) -> Result<Oklch, SampleError> {
        let color = self.curve.at(t);
        if color.finite() {
            Ok(color)
        } else {
            Err(SampleError::UndefinedColor { t })
        }
    }

    fn srgb_at(&self, t: f64) -> Result<[f64; 3], SampleError> {
        let rgb = self.color_at(t)?.srgb(self.map_chroma);
        if rgb.iter().all(|channel| channel.is_finite()) {
            Ok(rgb)
        } else {
            Err(SampleError::UndefinedColor { t })
        }
    }

    fn rgba8_at(&self, t: f64) -> Result<[u8; 4], SampleError> {
        let [r, g, b] = self
            .srgb_at(t)?
            .map(|channel| (channel * 255.).round() as u8);
        Ok([r, g, b, 255])
    }
}

/// Deserialize directly from the same JSON representation as an app project.
/// Compiled expressions are not serialized; they are rebuilt once on load.
impl<'de> Deserialize<'de> for Colormap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let project = Project::deserialize(deserializer)?;
        Self::from_project(&project).map_err(serde::de::Error::custom)
    }
}
