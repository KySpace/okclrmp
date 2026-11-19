pub mod appearance;
mod gamut_mesh;
pub mod gpu;
pub mod preview;
pub mod starting_points;

// Keep the app's existing module paths and types while sharing one implementation.
pub use okclrmp_core::{Colormap, LoadError, Oklch, SampleError};
pub use okclrmp_core::{color, constants, exchange, expr, helpers, model, spline};
