//! Load and sample OKLCH Colormap Studio projects without a UI or GPU.
//!
//! ```no_run
//! use okclrmp_core::Colormap;
//!
//! let map = Colormap::load("colormap.okclrmp.json")?;
//! let rgba = map.sample_rgba8(0.75)?;
//! let texture = map.rgba8_lut(256)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! [`Colormap`] compiles expressions once, then samples the continuous curve.
//! It supports expressions, cubic/legacy quadratic splines, imported paths,
//! slider-bound constants, output inversion, and the saved RGB mapping policy.
//! [`Oklch`] samples retain original coordinates, including unwrapped hue and
//! out-of-gamut colors. Only RGB sampling applies gamut mapping or clipping.
//!
//! The public modules also expose the same project/editing types used by the
//! native app. The crate's only direct dependencies are Serde and serde_json.

pub mod color;
mod colormap;
pub mod constants;
pub mod exchange;
pub mod expr;
pub mod model;
pub mod spline;

pub use color::Oklch;
pub use colormap::{Colormap, LoadError, SampleError};
pub use model::Project;
