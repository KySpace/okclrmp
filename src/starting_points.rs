//! Built-in curve definitions transcribed from samples/. No saved workspace state.
use crate::{
    constants::{Constant, ConstantKind},
    model::{Mode, Plane, Project},
    spline::{Node, NodeMode, Spline},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StartingPoint {
    #[default]
    LinearGreen,
    LinearViridis,
    SymmetricGreenMagenta,
    SymmetricTurbo,
    CyclicHue,
}
impl StartingPoint {
    pub const ALL: [Self; 5] = [
        Self::LinearGreen,
        Self::LinearViridis,
        Self::SymmetricGreenMagenta,
        Self::SymmetricTurbo,
        Self::CyclicHue,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::LinearGreen => "Linear · Green",
            Self::LinearViridis => "Linear · Viridis",
            Self::SymmetricGreenMagenta => "Symmetric · Green / Magenta",
            Self::SymmetricTurbo => "Symmetric · Turbo",
            Self::CyclicHue => "Cyclic · Hue",
        }
    }
    pub fn group(self) -> &'static str {
        match self {
            Self::LinearGreen | Self::LinearViridis => "Linear",
            Self::SymmetricGreenMagenta | Self::SymmetricTurbo => "Symmetric",
            Self::CyclicHue => "Cyclic",
        }
    }
    /// Replace only the colormap. Workspace/reference settings remain independent.
    pub fn apply(self, project: &mut Project) {
        use ConstantKind::{Degree, Fraction};
        project.name = self.name().into();
        project.parameter = "t".into();
        project.definitions.clear();
        project.nodes.clear();
        project.spline = None;
        project.spline_constant = false;
        project.reverse_output = false;
        project.mode = Mode::Expressions;
        let expressions = match self {
            Self::LinearGreen => {
                project.constants = vec![
                    constant("hue", 142.53, Degree),
                    constant("c_0", 0., Fraction),
                    constant("c_1", 0.08, Fraction),
                    constant("c_c", 0.35, Fraction),
                    constant("l_min", 0.26, Fraction),
                ];
                [
                    "1-(1-l_min)*t",
                    "c_1*t^2 + c_c*t*(1-t) + c_0*(1-t)^2",
                    "hue",
                ]
            }
            Self::LinearViridis => {
                project.constants = vec![
                    constant("l_min", 0.2, Fraction),
                    constant("l_max", 0.85, Fraction),
                    constant("c_min", 0.02, Fraction),
                    constant("c_max", 0.15, Fraction),
                    constant("h_min", 340., Degree),
                    constant("h_max", 100., Degree),
                ];
                [
                    "l_min + (l_max-l_min)*t",
                    "c_min + (c_max-c_min)*t",
                    "h_min + (h_max-h_min)*t",
                ]
            }
            Self::SymmetricGreenMagenta => {
                project.parameter = "u".into();
                project.constants = vec![
                    constant("c_max", 0.2, Fraction),
                    constant("l_min", 0.7, Fraction),
                ];
                project.definitions = "bias = 2 * abs(u - 0.5)".into();
                ["1 - (1 - l_min)*bias", "bias*c_max", "u < 0.5 ? 142 : 327"]
            }
            Self::SymmetricTurbo => {
                project.constants = vec![
                    constant("l_max", 0.92, Fraction),
                    constant("l_range", 0.56, Fraction),
                ];
                project.definitions = "bias = 2 * abs(t - 0.5)".into();
                project.mode = Mode::Spline(Plane::Ch);
                project.spline = Some(Spline {
                    nodes: vec![
                        Node {
                            position: [0.7745716174443563, 0.1684500813484192],
                            incoming: None,
                            outgoing: Some([-0.005810457247274914, 0.404674232006073]),
                            mode: NodeMode::Corner,
                        },
                        Node {
                            position: [0.3804360934365024, 0.18075024485588065],
                            incoming: Some([0.2471532104191957, 0.]),
                            outgoing: Some([-0.2471532104191957, 0.]),
                            mode: NodeMode::Smooth,
                        },
                        Node {
                            position: [-0.01369943057135159, 0.1684500813484192],
                            incoming: Some([0.005810457247274914, 0.404674232006073]),
                            outgoing: None,
                            mode: NodeMode::Corner,
                        },
                    ],
                    knot: 0.5,
                    linear: false,
                    symmetric: true,
                });
                [
                    "l_max - l_range*bias",
                    "0.06 + 0.08*abs(sin(2*pi*t))",
                    "300 - 280*t - 25*sin(2*pi*t)",
                ]
            }
            Self::CyclicHue => {
                project.constants = vec![
                    constant("c", 0.122, Fraction),
                    constant("l", 0.741, Fraction),
                ];
                ["l", "c", "t*360 + 45"]
            }
        };
        project.expressions = expressions.map(str::to_string);
    }
    pub fn project(self) -> Project {
        let mut project = Project::default();
        self.apply(&mut project);
        let curve = project.compile().expect("valid built-in starting point");
        project.follow_slices(&curve);
        project
    }
}
fn constant(name: &str, value: f64, kind: ConstantKind) -> Constant {
    Constant {
        name: name.into(),
        value,
        kind,
        binding: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hardcoded_curves_match_the_five_sample_projects() {
        let sources = [
            include_str!("../samples/linear.green.json"),
            include_str!("../samples/linear.viridis.json"),
            include_str!("../samples/symm.greenmagenta.json"),
            include_str!("../samples/symm.turbo.json"),
            include_str!("../samples/cyclic.hue.json"),
        ];
        for (preset, source) in StartingPoint::ALL.into_iter().zip(sources) {
            let source: Project = serde_json::from_str(source).unwrap();
            let expected = source.compile().unwrap();
            let project = preset.project();
            let curve = project.compile().unwrap();
            for i in 0..=1024 {
                let t = i as f64 / 1024.;
                let (a, b) = (curve.at(t), expected.at(t));
                for axis in 0..3 {
                    assert!(
                        (a.channel(axis) - b.channel(axis)).abs() < 1e-12,
                        "{preset:?} at {t}, axis {axis}"
                    );
                }
            }
            assert!(project.imported.is_empty());
            assert!(project.constants.iter().all(|c| c.binding.is_none()));
            assert!(!project.reverse_output && project.orthographic && !project.polar);
            assert_eq!(project.hue_origin, 0.);
            assert_eq!(project.zoom, 1.);
            assert_eq!(
                project.spline.is_some(),
                preset == StartingPoint::SymmetricTurbo
            );
        }
    }
    #[test]
    fn applying_a_starting_point_preserves_workspace_and_description_roundtrips() {
        let mut project = Project {
            yaw: 1.2,
            pitch: 0.4,
            zoom: 1.5,
            hue_origin: 51.,
            polar: true,
            orthographic: false,
            sample_count: 64,
            map_chroma: false,
            imported: vec![crate::color::Oklch::new(0.5, 0.1, 45.)],
            ..Project::default()
        };
        StartingPoint::SymmetricTurbo.apply(&mut project);
        assert_eq!((project.yaw, project.pitch, project.zoom), (1.2, 0.4, 1.5));
        assert_eq!(project.hue_origin, 51.);
        assert!(project.polar && !project.orthographic);
        assert_eq!(project.sample_count, 64);
        assert!(!project.map_chroma);
        assert_eq!(project.imported.len(), 1);
        let before = project.compile().unwrap().at(0.23);
        project.name = "Custom description · 色彩".into();
        let restored: Project =
            serde_json::from_str(&serde_json::to_string(&project).unwrap()).unwrap();
        assert_eq!(restored.name, project.name);
        assert_eq!(restored.compile().unwrap().at(0.23), before);
    }
}
