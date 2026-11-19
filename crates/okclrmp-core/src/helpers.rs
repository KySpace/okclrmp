//! Color helpers for labels and annotations.

use crate::Oklch;

/// Choose the app's adaptive, single-color annotation ink for a background.
///
/// Expects a finite OKLCH background with lightness in [0, 1]. The foreground
/// becomes white/black near the 0.65 lightness threshold, with more chroma and
/// less contrast toward the dark/bright endpoints. Its hue is offset by 120°.
/// Convert the result to RGB at the rendering boundary.
///
/// ```
/// use okclrmp_core::{Oklch, helpers::annotation_ink};
///
/// let background = Oklch::from_srgb([0.9, 0.9, 0.9]);
/// let text_rgba = annotation_ink(background).rgba8(true);
/// assert_eq!(text_rgba[3], 255);
/// ```
pub fn annotation_ink(background: Oklch) -> Oklch {
    let (l_thres, l_min, l_max) = (0.65, 0.87, 0.45);
    let frac_l_bias = if background.l > l_thres {
        (1. - background.l) / (1. - l_thres)
    } else {
        background.l / l_thres
    }; // frac = 0 means black or white background, frac = 1 means at grayish threshold
    let c_max = if background.l > l_thres { 0.07 } else { 0.11 };
    let l = if background.l > l_thres {
        l_max * (1. - frac_l_bias)
    } else {
        l_min + (1. - l_min) * frac_l_bias
    };
    Oklch::new(
        l,
        c_max * (1. - frac_l_bias),
        (background.h + 120.).rem_euclid(360.),
    )
}
