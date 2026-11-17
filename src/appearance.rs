//! App-owned colors shared by the native UI and WGPU plot decoration.
//! These do not participate in colormap conversion or export.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    #[default]
    Dark,
    Light,
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub background: u32,
    pub surface: u32,
    pub border: u32,
    pub text: u32,
    pub muted: u32,
    pub accent: u32,
    pub warning: u32,
    pub error_background: u32,
    pub channels: [u32; 3],
    pub label_background: u32,
    pub guide: u32,
    pub curve: u32,
    pub curve_outline: u32,
}

#[derive(Clone, Copy)]
pub struct PlotStyle {
    pub background: [f32; 3],
    pub checker: [f32; 3],
    pub grid: [f32; 3],
    pub volume_background: [f32; 3],
    pub volume_grid: [f32; 3],
}

impl Appearance {
    pub fn palette(self) -> Palette {
        match self {
            Self::Dark => Palette {
                background: 0x0c0f14,
                surface: 0x11161e,
                border: 0x28313e,
                text: 0xdce4ed,
                muted: 0x8492a6,
                accent: 0xa9d8bd,
                warning: 0xff987e,
                error_background: 0x362522,
                channels: [0xaacfe5, 0xe4c27e, 0xc6b3e8],
                label_background: 0x101722,
                guide: 0x344151,
                curve: 0xf3f7fc,
                curve_outline: 0x10151c,
            },
            Self::Light => Palette {
                background: 0xedf1f5,
                surface: 0xffffff,
                border: 0xc9d3df,
                text: 0x202a37,
                muted: 0x566272,
                accent: 0x246347,
                warning: 0xad3e24,
                error_background: 0xffeee8,
                channels: [0x246081, 0x805b0e, 0x7044a7],
                label_background: 0xf5f7fa,
                guide: 0xa0adbd,
                curve: 0x243445,
                curve_outline: 0xffffff,
            },
        }
    }
    pub fn plots(self) -> PlotStyle {
        match self {
            Self::Dark => PlotStyle {
                background: [0.085, 0.098, 0.12],
                checker: [0.102, 0.116, 0.14],
                grid: [0.68, 0.74, 0.8],
                volume_background: [0.047, 0.059, 0.078],
                volume_grid: [0.82, 0.89, 0.94],
            },
            Self::Light => PlotStyle {
                background: [0.985, 0.99, 0.995],
                checker: [0.965, 0.975, 0.985],
                grid: [0.40, 0.47, 0.56],
                volume_background: [0.95, 0.96, 0.975],
                volume_grid: [0.24, 0.30, 0.38],
            },
        }
    }
}
