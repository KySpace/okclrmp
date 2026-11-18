use crate::{
    color::Oklch,
    constants::{Constant, deserialize_constants},
    expr::{self, Expr},
    spline::Spline,
};
use serde::{Deserialize, Serialize};

pub const C_MAX: f64 = 0.4;
pub const CURVE_SAMPLES: usize = 1025;
// Pitch is elevation above the horizon; zenith = π/2 - pitch.
pub const MIN_PITCH: f32 = -std::f32::consts::FRAC_PI_2;
pub const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Plane {
    Cl,
    Ch,
    Lh,
}
impl Plane {
    pub const ALL: [Self; 3] = [Self::Cl, Self::Ch, Self::Lh];
    pub fn axes(self) -> (usize, usize, usize) {
        match self {
            Self::Cl => (0, 1, 2),
            Self::Ch => (2, 1, 0),
            Self::Lh => (2, 0, 1),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Cl => "Chroma · Lightness",
            Self::Ch => "Chroma · Hue",
            Self::Lh => "Lightness · Hue",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Self::Cl => "C–L",
            Self::Ch => "C–H",
            Self::Lh => "L–H",
        }
    }
    pub fn project(self, c: Oklch) -> [f64; 2] {
        let (x, y, _) = self.axes();
        [normalized(c.channel(x), x), normalized(c.channel(y), y)]
    }
    pub fn project_at(self, c: Oklch, hue_origin: f64) -> [f64; 2] {
        let mut point = self.project(c);
        if self.axes().0 == 2 {
            point[0] = hue_position(c.h, hue_origin);
        }
        point
    }
    pub fn from_point(self, p: [f64; 2]) -> [f64; 2] {
        let (x, y, _) = self.axes();
        [denormalized(p[0], x), denormalized(p[1], y)]
    }
    pub fn spline_images(self, spline: &Spline, hue_origin: f64) -> Vec<Spline> {
        if self.axes().0 != 2 {
            return vec![spline.clone()];
        }
        [0., -1., 1.]
            .into_iter()
            .map(|shift| {
                let mut image = spline.clone();
                for node in &mut image.nodes {
                    node.position[0] = hue_position(node.position[0] * 360., hue_origin) + shift;
                }
                image
            })
            .collect()
    }
}
pub fn hue_position(hue: f64, origin: f64) -> f64 {
    normalized(hue - origin.rem_euclid(360.), 2)
}
pub fn normalized(v: f64, a: usize) -> f64 {
    match a {
        1 => v / C_MAX,
        2 => {
            let wrapped = v.rem_euclid(360.);
            if v > 0. && wrapped < 1e-10 {
                1.
            } else {
                wrapped / 360.
            }
        }
        _ => v,
    }
}
pub fn denormalized(v: f64, a: usize) -> f64 {
    v * match a {
        1 => C_MAX,
        2 => 360.,
        _ => 1.,
    }
}
pub fn axis_name(a: usize) -> &'static str {
    ["L", "C", "H"][a]
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Mode {
    Expressions,
    Spline(Plane),
    Imported,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub parameter: String,
    #[serde(default, deserialize_with = "deserialize_constants")]
    pub constants: Vec<Constant>,
    #[serde(default)]
    pub definitions: String,
    pub expressions: [String; 3],
    pub mode: Mode,
    /// Legacy quadratic anchors, retained only for reading older projects.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spline: Option<Spline>,
    /// In spline mode, the omitted channel is the owning plane's slice value.
    #[serde(default)]
    pub spline_constant: bool,
    pub imported: Vec<Oklch>,
    pub slices: [f64; 3],
    pub cursor: f64,
    pub sample_count: usize,
    pub map_chroma: bool,
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    /// Shared hue chart origin, in degrees. Curves retain unwrapped hue.
    #[serde(default)]
    pub hue_origin: f64,
    #[serde(default)]
    pub polar: bool,
    #[serde(default)]
    pub show_slice_contours: bool,
    #[serde(default)]
    pub orthographic: bool,
}
impl Default for Project {
    fn default() -> Self {
        Self::preset(0)
    }
}
impl Project {
    pub fn preset(i: usize) -> Self {
        let (name, constants, expressions, mode, nodes) = match i {
            1 => (
                "Linear sweep",
                "l_min=0.25; l_max=0.9; c_min=0.04; c_max=0.18; h_min=285; h_max=35",
                [
                    "l_min + (l_max-l_min)*t",
                    "c_min + (c_max-c_min)*t",
                    "h_min + (h_max-h_min)*t",
                ],
                Mode::Expressions,
                vec![],
            ),
            2 => (
                "Diverging · ember / iris",
                "c_max=0.30",
                [
                    "1 - 0.36*2*abs(t - 0.5)",
                    "abs(t - 0.5)*c_max",
                    "t < 0.5 ? 15 : 263",
                ],
                Mode::Expressions,
                vec![],
            ),
            3 => (
                "Turbo · spectral study",
                "l_max=0.92; l_range=0.52",
                [
                    "l_max-l_range*2*abs(t-0.5)",
                    "0.06 + 0.16*abs(sin(2*pi*t))",
                    "300 - 280*t - 25*sin(2*pi*t)",
                ],
                Mode::Expressions,
                vec![],
            ),
            _ => (
                "Monochrome · moss",
                "hue=136.59",
                ["t", "0.6*t*(1-t)", "hue"],
                Mode::Spline(Plane::Cl),
                vec![[0., 0.], [0.5, 0.15], [1., 0.]],
            ),
        };
        let mut project = Self {
            name: name.into(),
            parameter: "t".into(),
            constants: Constant::from_legacy(constants, "t").expect("valid preset constants"),
            definitions: String::new(),
            expressions: expressions.map(str::to_string),
            mode,
            nodes,
            spline: None,
            spline_constant: false,
            imported: vec![],
            slices: [0.65, 0.10, 136.59],
            cursor: 0.5,
            sample_count: 256,
            map_chroma: true,
            yaw: -0.65,
            pitch: 0.65,
            zoom: 1.,
            hue_origin: 0.,
            polar: false,
            show_slice_contours: false,
            orthographic: false,
        };
        project.migrate_spline().expect("valid preset spline");
        project
    }
    pub fn resolved_spline(&self) -> Result<Option<Spline>, String> {
        if let Some(spline) = &self.spline {
            spline.validate()?;
            return Ok(Some(spline.clone()));
        }
        let Mode::Spline(plane) = self.mode else {
            return Ok(None);
        };
        if !(2..=3).contains(&self.nodes.len())
            || self.nodes.iter().flatten().any(|v| !v.is_finite())
        {
            return Err("A spline needs two or three finite nodes".into());
        }
        let (x, y, _) = plane.axes();
        let points: Vec<_> = self
            .nodes
            .iter()
            .map(|p| [p[0] / denormalized(1., x), p[1] / denormalized(1., y)])
            .collect();
        Ok(Some(Spline::from_legacy(&points)))
    }
    pub fn migrate_spline(&mut self) -> Result<(), String> {
        self.spline = self.resolved_spline()?;
        self.nodes.clear();
        Ok(())
    }
    pub fn compile(&self) -> Result<Curve, String> {
        if !self.cursor.is_finite()
            || self.slices.iter().any(|x| !x.is_finite())
            || !self.yaw.is_finite()
            || !self.pitch.is_finite()
            || !self.zoom.is_finite()
            || !self.hue_origin.is_finite()
        {
            return Err("Project contains non-finite view values".into());
        }
        if !(0. ..=1.).contains(&self.cursor)
            || !(0. ..=1.).contains(&self.slices[0])
            || !(0. ..=C_MAX).contains(&self.slices[1])
            || !(0. ..=360.).contains(&self.slices[2])
            || !(0.5..=2.5).contains(&self.zoom)
            || !(MIN_PITCH..=MAX_PITCH).contains(&self.pitch)
        {
            return Err("Project view settings are outside the supported ranges".into());
        }
        if !(2..=4096).contains(&self.sample_count)
            || self.imported.len() > 16384
            || self.imported.iter().any(|c| !c.finite())
        {
            return Err("Project has invalid samples or reference colors".into());
        }
        let mut constants = std::collections::BTreeMap::new();
        for constant in &self.constants {
            if !expr::identifier(&constant.name)
                || expr::reserved(&constant.name)
                || constant.name == self.parameter
                || constants.contains_key(&constant.name)
            {
                return Err(format!("Invalid or duplicate constant ‘{}’", constant.name));
            }
            if constant.kind.constrain(constant.value) != Some(constant.value) {
                return Err(format!(
                    "Constant ‘{}’ is outside its {} range",
                    constant.name,
                    constant.kind.name()
                ));
            }
            constants.insert(constant.name.clone(), constant.value);
        }
        let symbols = expr::definitions(&self.definitions, &self.parameter, &constants)?;
        let mut channels = Vec::new();
        for a in 0..3 {
            if self.constant_axis() == Some(a) {
                channels.push(Expr::Number(self.slices[a]));
                continue;
            }
            let used = match self.mode {
                Mode::Expressions => true,
                Mode::Spline(p) => a == p.axes().2,
                Mode::Imported => false,
            };
            channels.push(if used {
                expr::compile_with_symbols(&self.expressions[a], &self.parameter, &symbols)
                    .map_err(|e| format!("{}: {e}", axis_name(a)))?
            } else {
                Expr::Number(0.)
            });
        }
        let spline = self.resolved_spline()?;
        if matches!(self.mode, Mode::Imported)
            && (self.imported.len() < 2 || self.imported.iter().any(|p| !p.finite()))
        {
            return Err("Import at least two finite colors first".into());
        }
        let curve = Curve {
            channels: channels.try_into().unwrap(),
            mode: self.mode.clone(),
            spline,
            imported: self.imported.clone(),
        };
        for i in 0..CURVE_SAMPLES {
            let t = i as f64 / (CURVE_SAMPLES - 1) as f64;
            if !curve.at(t).finite() {
                return Err(format!(
                    "Expression is undefined at {} = {t:.5}",
                    self.parameter
                ));
            }
        }
        Ok(curve)
    }
    pub fn enable_spline(&mut self, plane: Plane, curve: &Curve) {
        if self.mode == Mode::Spline(plane) {
            return;
        }
        let points: Vec<_> = [0., 0.5, 1.]
            .into_iter()
            .map(|t| {
                let p = curve.at(t);
                let (x, y, _) = plane.axes();
                [
                    p.channel(x) / denormalized(1., x),
                    p.channel(y) / denormalized(1., y),
                ]
            })
            .collect();
        self.spline = Some(Spline::from_legacy(&points));
        self.spline_constant = false;
        self.nodes.clear();
        self.mode = Mode::Spline(plane);
    }
    pub fn constant_axis(&self) -> Option<usize> {
        match self.mode {
            Mode::Spline(plane) if self.spline_constant => Some(plane.axes().2),
            _ => None,
        }
    }
    /// Returns whether the colormap changed, in addition to the slice view.
    pub fn set_slice(&mut self, axis: usize, value: f64) -> bool {
        let changed = self.slices[axis] != value;
        self.slices[axis] = value;
        changed && self.constant_axis() == Some(axis)
    }
    pub fn use_expressions(&mut self) {
        self.mode = Mode::Expressions;
        self.spline_constant = false;
    }
    pub fn set_polar(&mut self, polar: bool) {
        self.polar = polar;
        self.pitch = self.pitch.clamp(MIN_PITCH, MAX_PITCH);
    }
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.008;
        self.pitch = (self.pitch + dy * 0.008).clamp(MIN_PITCH, MAX_PITCH);
    }
    pub fn reset_view(&mut self) {
        self.yaw = -0.65;
        self.pitch = 0.65;
        self.zoom = 1.;
        self.hue_origin = 0.;
    }
    pub fn pan_hue(&mut self, horizontal_fraction: f64) {
        self.hue_origin = (self.hue_origin - horizontal_fraction * 360.).rem_euclid(360.);
    }
    pub fn connected_in_view(&self, a: Oklch, b: Oklch, plane: Option<Plane>) -> bool {
        connected(a, b)
            && if plane.is_some_and(|p| p.axes().0 == 2) || (plane.is_none() && !self.polar) {
                (hue_position(a.h, self.hue_origin) - hue_position(b.h, self.hue_origin)).abs()
                    < 0.5
            } else {
                true
            }
    }
}
#[derive(Clone, Debug)]
pub struct Curve {
    channels: [Expr; 3],
    pub mode: Mode,
    pub spline: Option<Spline>,
    imported: Vec<Oklch>,
}
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub t: f64,
    pub color: Oklch,
}
#[derive(Clone, Debug)]
pub struct Intersections {
    pub points: Vec<Sample>,
    pub coplanar: bool,
}
impl Curve {
    pub fn at(&self, t: f64) -> Oklch {
        let t = t.clamp(0., 1.);
        if self.mode == Mode::Imported {
            let f = t * (self.imported.len() - 1) as f64;
            let i = (f.floor() as usize).min(self.imported.len() - 2);
            let k = f - i as f64;
            let a = self.imported[i];
            let b = self.imported[i + 1];
            let dh = (b.h - a.h + 180.).rem_euclid(360.) - 180.;
            return Oklch::new(a.l + (b.l - a.l) * k, a.c + (b.c - a.c) * k, a.h + dh * k);
        }
        let mut c = Oklch::new(
            self.channels[0].eval(t),
            self.channels[1].eval(t),
            self.channels[2].eval(t),
        );
        if let Mode::Spline(plane) = self.mode {
            let p = plane.from_point(self.spline.as_ref().unwrap().at(t));
            let (x, y, _) = plane.axes();
            c.set_channel(x, p[0]);
            c.set_channel(y, p[1]);
        }
        c
    }
    pub fn sample(&self, n: usize) -> Vec<Sample> {
        (0..n)
            .map(|i| {
                let t = i as f64 / (n - 1).max(1) as f64;
                Sample {
                    t,
                    color: self.at(t),
                }
            })
            .collect()
    }
    pub fn intersections(&self, plane: Plane, slice: f64) -> Intersections {
        let axis = plane.axes().2;
        let samples = self.sample(CURVE_SAMPLES);
        let distance = |c: Oklch| {
            let d = c.channel(axis) - slice;
            if axis == 2 {
                (d + 180.).rem_euclid(360.) - 180.
            } else {
                d
            }
        };
        let eps = if axis == 2 { 1e-7 } else { 1e-10 };
        if samples.iter().all(|s| distance(s.color).abs() < eps) {
            return Intersections {
                points: vec![],
                coplanar: true,
            };
        }
        let mut roots: Vec<Sample> = vec![];
        // A constant sub-interval contributes its endpoints, not thousands of labels.
        let mut i = 0;
        while i < samples.len() {
            if distance(samples[i].color).abs() < eps {
                let start = i;
                while i + 1 < samples.len() && distance(samples[i + 1].color).abs() < eps {
                    i += 1;
                }
                roots.push(samples[start]);
                if i > start {
                    roots.push(samples[i]);
                }
            }
            i += 1;
        }
        for pair in samples.windows(2) {
            let a = pair[0];
            let b = pair[1];
            let da = distance(a.color);
            let db = distance(b.color);
            if da.abs() >= eps
                && db.abs() >= eps
                && da * db < 0.
                && (axis != 2 || (da - db).abs() < 180.)
            {
                let (mut lo, mut hi) = (a.t, b.t);
                let mut dl = da;
                for _ in 0..40 {
                    let m = (lo + hi) * 0.5;
                    let d = distance(self.at(m));
                    if dl * d <= 0. {
                        hi = m;
                    } else {
                        lo = m;
                        dl = d;
                    }
                }
                let t = (lo + hi) * 0.5;
                let color = self.at(t);
                // A ternary jump can straddle the slice without ever intersecting it.
                if distance(color).abs() < eps * 10. {
                    roots.push(Sample { t, color });
                }
            }
        }
        // Tangencies need not occur exactly on a sample or change sign. Minimize
        // the residual around local extrema and verify the actual curve value.
        for triple in samples.windows(3) {
            let [a, b, c] = [triple[0], triple[1], triple[2]];
            let [da, db, dc] = [distance(a.color), distance(b.color), distance(c.color)];
            if db.abs() > eps
                && da * db > 0.
                && db * dc > 0.
                && db.abs() < da.abs()
                && db.abs() < dc.abs()
            {
                let (mut lo, mut hi) = (a.t, c.t);
                for _ in 0..48 {
                    let x = lo + (hi - lo) / 3.;
                    let y = hi - (hi - lo) / 3.;
                    if distance(self.at(x)).abs() < distance(self.at(y)).abs() {
                        hi = y;
                    } else {
                        lo = x;
                    }
                }
                let t = (lo + hi) * 0.5;
                let color = self.at(t);
                if distance(color).abs() < eps {
                    roots.push(Sample { t, color });
                }
            }
        }
        roots.sort_by(|a, b| a.t.total_cmp(&b.t));
        roots.dedup_by(|a, b| (a.t - b.t).abs() < 1e-4);
        Intersections {
            points: roots,
            coplanar: false,
        }
    }
}
pub fn interpolate(nodes: &[[f64; 2]], t: f64) -> [f64; 2] {
    if nodes.len() == 2 {
        return std::array::from_fn(|i| nodes[0][i] * (1. - t) + nodes[1][i] * t);
    }
    // Interpolating quadratic: the middle handle lies ON the curve at t=0.5.
    let weights = [
        2. * (t - 0.5) * (t - 1.),
        4. * t * (1. - t),
        2. * t * (t - 0.5),
    ];
    std::array::from_fn(|i| (0..3).map(|j| nodes[j][i] * weights[j]).sum())
}
pub fn connected(a: Oklch, b: Oklch) -> bool {
    a.finite()
        && b.finite()
        && (a.l - b.l).abs() < 0.1
        && (a.c - b.c).abs() < 0.05
        && ((b.h - a.h + 180.).rem_euclid(360.) - 180.).abs() < 30.
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_constants_and_derived_variables_update_and_roundtrip() {
        use crate::constants::ConstantKind;
        let mut p = Project::preset(2);
        p.parameter = "v".into();
        p.definitions = "bias = 2 * abs(v - 0.5); lightness = 1 - 0.36*bias".into();
        p.expressions = [
            "lightness".into(),
            "bias*c_max/2".into(),
            "v < 0.5 ? 15 : 263".into(),
        ];
        let curve = p.compile().unwrap();
        for sample in curve.sample(101) {
            assert!((sample.color.l - (1. - 0.36 * 2. * (sample.t - 0.5).abs())).abs() < 1e-12);
            assert!((sample.color.c - (sample.t - 0.5).abs() * 0.3).abs() < 1e-12);
        }
        p.constants[0].value = 0.2;
        let changed = p.compile().unwrap();
        assert_eq!(changed.at(0.).c, 0.1);
        assert_eq!(changed.at(0.5).c, 0.);
        let json = serde_json::to_string(&p).unwrap();
        let restored: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.constants, p.constants);
        assert_eq!(restored.definitions, p.definitions);
        assert_eq!(restored.compile().unwrap().at(0.25), changed.at(0.25));
        p.constants[0].value = 1.01;
        assert!(p.compile().is_err());
        p.constants[0].kind = ConstantKind::Free;
        assert!(p.compile().is_ok());
        p.constants.push(p.constants[0].clone());
        assert!(p.compile().is_err());
        p.constants.pop();
        p.definitions = "bias=sqrt(v-0.5); lightness=bias".into();
        assert!(p.compile().is_err());
    }
    #[test]
    fn legacy_constant_assignments_migrate_without_changing_samples() {
        for i in 0..4 {
            let original = Project::preset(i);
            let mut json = serde_json::to_value(&original).unwrap();
            json.as_object_mut().unwrap().remove("definitions");
            let mut source = original
                .constants
                .iter()
                .map(|c| format!("{}={}", c.name, c.value))
                .collect::<Vec<_>>()
                .join(";");
            // Chained constant assignments and hue endpoint semantics survive.
            source.push_str("; first=0.2; second=1-first; h_end=360");
            json["constants"] = source.into();
            let migrated: Project = serde_json::from_value(json).unwrap();
            assert!(migrated.definitions.is_empty());
            assert_eq!(migrated.constants.last().unwrap().value, 360.);
            assert_eq!(
                original
                    .compile()
                    .unwrap()
                    .sample(101)
                    .iter()
                    .map(|s| s.color)
                    .collect::<Vec<_>>(),
                migrated
                    .compile()
                    .unwrap()
                    .sample(101)
                    .iter()
                    .map(|s| s.color)
                    .collect::<Vec<_>>()
            );
            let saved = serde_json::to_value(&migrated).unwrap();
            assert!(saved["constants"].is_array());
        }
    }
    #[test]
    fn spline_goes_through_all_nodes() {
        let n = [[0., 0.], [0.5, 0.15], [1., 0.]];
        assert_eq!(interpolate(&n, 0.5), n[1]);
        assert_eq!(interpolate(&n, 0.), n[0]);
        assert_eq!(interpolate(&n, 1.), n[2]);
    }
    #[test]
    fn presets_and_spline_axis_lock() {
        for i in 0..4 {
            assert!(Project::preset(i).compile().is_ok());
        }
        let mut p = Project::default();
        p.expressions[0] = "invalid".into();
        assert!(p.compile().is_ok());
        p.expressions[2] = "invalid".into();
        assert!(p.compile().is_err());
    }
    #[test]
    fn orbit_stops_at_both_poles_and_projection_options_roundtrip() {
        let mut p = Project {
            orthographic: true,
            ..Default::default()
        };
        p.set_polar(true);
        for _ in 0..3 {
            p.orbit(15., 1000.);
            assert_eq!(p.pitch, MAX_PITCH);
        }
        assert!(p.compile().is_ok());
        p.show_slice_contours = true;
        let json = serde_json::to_string(&p).unwrap();
        let mut restored: Project = serde_json::from_str(&json).unwrap();
        assert!(restored.polar && restored.show_slice_contours && restored.orthographic);
        assert_eq!(restored.pitch, MAX_PITCH);
        assert!(restored.compile().is_ok());
        restored.orbit(0., -1.);
        assert!(restored.pitch < MAX_PITCH);
        restored.orbit(0., 1000.);
        restored.set_polar(false);
        assert_eq!(restored.pitch, MAX_PITCH);
        assert!(restored.compile().is_ok());
        restored.orbit(0., -1000.);
        assert_eq!(restored.pitch, MIN_PITCH);
        let mut old = serde_json::to_value(&p).unwrap();
        old.as_object_mut().unwrap().remove("show_slice_contours");
        old.as_object_mut().unwrap().remove("orthographic");
        let old: Project = serde_json::from_value(old).unwrap();
        assert!(!old.show_slice_contours && !old.orthographic);
        restored.reset_view();
        assert_eq!(restored.pitch, 0.65);
        assert_eq!(restored.zoom, 1.);
        assert!(restored.orthographic && restored.show_slice_contours && !restored.polar);
        let mut invalid = p;
        invalid.pitch = MAX_PITCH + 0.01;
        assert!(invalid.compile().is_err());
        invalid.pitch = MIN_PITCH - 0.01;
        assert!(invalid.compile().is_err());
    }
    #[test]
    fn constant_slider_changes_the_orthogonal_channel_only_in_all_planes() {
        for plane in Plane::ALL {
            let mut p = Project::preset(3);
            let source = p.compile().unwrap();
            p.enable_spline(plane, &source);
            let original = p.compile().unwrap();
            let expressions = p.expressions.clone();
            let (x, y, axis) = plane.axes();
            p.spline_constant = true;
            assert_eq!(p.constant_axis(), Some(axis));
            let constant = p.compile().unwrap();
            for i in 0..=100 {
                let t = i as f64 / 100.;
                assert_eq!(constant.at(t).channel(axis), p.slices[axis]);
                assert_eq!(constant.at(t).channel(x), original.at(t).channel(x));
                assert_eq!(constant.at(t).channel(y), original.at(t).channel(y));
            }
            // Exercise both slider endpoints, including raw H=360, and an interior value.
            for value in [0., denormalized(1., axis), [0.37, 0.21, 271.25][axis]] {
                assert!(p.set_slice(axis, value));
                assert!(!p.set_slice(axis, value));
                let curve = p.compile().unwrap();
                assert!(
                    curve
                        .sample(101)
                        .iter()
                        .all(|s| s.color.channel(axis) == value)
                );
                assert!(curve.intersections(plane, value).coplanar);
                let csv =
                    crate::exchange::export(&curve, 17, crate::exchange::ExportSpace::Oklch, false);
                let colors =
                    crate::exchange::parse(&csv, crate::exchange::ImportSpace::Oklch).unwrap();
                assert!(
                    colors
                        .iter()
                        .all(|c| (c.channel(axis) - value).abs() < 1e-7)
                );
            }
            let before: Vec<_> = p
                .compile()
                .unwrap()
                .sample(31)
                .iter()
                .map(|s| s.color)
                .collect();
            assert!(!p.set_slice(x, [0.2, 0.1, 90.][x]));
            let after: Vec<_> = p
                .compile()
                .unwrap()
                .sample(31)
                .iter()
                .map(|s| s.color)
                .collect();
            assert_eq!(before, after);
            p.spline_constant = false;
            assert_eq!(p.constant_axis(), None);
            assert!(!p.set_slice(axis, [0.4, 0.2, 190.][axis]));
            let restored = p.compile().unwrap();
            for i in 0..=100 {
                assert_eq!(restored.at(i as f64 / 100.), original.at(i as f64 / 100.));
            }
            assert_eq!(p.expressions, expressions);
        }
    }
    #[test]
    fn constant_mode_roundtrips_and_does_not_leak_into_other_curve_modes() {
        let mut p = Project {
            spline_constant: true,
            ..Default::default()
        };
        p.set_slice(2, 271.25);
        let encoded = serde_json::to_string(&p).unwrap();
        let mut restored: Project = serde_json::from_str(&encoded).unwrap();
        assert_eq!(restored.constant_axis(), Some(2));
        assert!(
            restored
                .compile()
                .unwrap()
                .sample(21)
                .iter()
                .all(|s| s.color.h == 271.25)
        );
        let mut old = serde_json::to_value(&p).unwrap();
        old.as_object_mut().unwrap().remove("spline_constant");
        let old: Project = serde_json::from_value(old).unwrap();
        assert!(!old.spline_constant);
        assert!((old.compile().unwrap().at(0.3).h - 136.59).abs() < 1e-10);
        restored.use_expressions();
        assert!(!restored.spline_constant);
        assert_eq!(restored.constant_axis(), None);
        assert!((restored.compile().unwrap().at(0.3).h - 136.59).abs() < 1e-10);
        let curve = restored.compile().unwrap();
        restored.enable_spline(Plane::Ch, &curve);
        assert!(!restored.spline_constant);
        restored.spline_constant = true;
        assert_eq!(restored.constant_axis(), Some(0));
        let curve = restored.compile().unwrap();
        restored.enable_spline(Plane::Lh, &curve);
        assert!(!restored.spline_constant);
        assert_eq!(restored.constant_axis(), None);
        restored.mode = Mode::Imported;
        restored.imported = vec![Oklch::new(0.4, 0.05, 20.), Oklch::new(0.8, 0.15, 70.)];
        restored.spline_constant = true;
        assert_eq!(restored.constant_axis(), None);
        assert!(!restored.set_slice(2, 190.));
        let imported = restored.compile().unwrap();
        assert_eq!(imported.at(0.), restored.imported[0]);
        assert_eq!(imported.at(1.), restored.imported[1]);
    }
    #[test]
    fn constant_mode_preserves_the_inactive_expression_and_validates_its_slice() {
        let mut p = Project::default();
        p.expressions[2] = "saved_unknown_expression".into();
        assert!(p.compile().is_err());
        p.spline_constant = true;
        assert!(p.compile().is_ok());
        assert_eq!(p.expressions[2], "saved_unknown_expression");
        p.spline_constant = false;
        assert!(p.compile().is_err());
        p.spline_constant = true;
        p.set_slice(2, f64::NAN);
        assert!(p.compile().is_err());
        p.set_slice(2, 361.);
        assert!(p.compile().is_err());
    }
    #[test]
    fn intersections_and_jumps() {
        let c = Project::preset(2).compile().unwrap();
        assert_eq!(c.intersections(Plane::Cl, 100.).points.len(), 0);
        assert_eq!(c.intersections(Plane::Ch, 0.8).points.len(), 2);
        assert_eq!(c.intersections(Plane::Lh, 0.1).points.len(), 2);
        let mono = Project::default().compile().unwrap();
        assert!(mono.intersections(Plane::Cl, 136.59).coplanar);
    }
    #[test]
    fn custom_param_and_nonfinite_rejection() {
        let mut p = Project::preset(2);
        p.parameter = "v".into();
        p.expressions = p.expressions.map(|e| e.replace('t', "v"));
        assert!(p.compile().is_ok());
        p.expressions[0] = "sqrt(-1)".into();
        assert!(p.compile().is_err());
    }
    #[test]
    fn tangencies_and_constant_intervals() {
        let mut p = Project::preset(1);
        p.expressions[0] = "0.5 + (t-0.3333)^2".into();
        let roots = p.compile().unwrap().intersections(Plane::Ch, 0.5).points;
        assert_eq!(roots.len(), 1);
        assert!((roots[0].t - 0.3333).abs() < 1e-5);
        let c = Project::preset(2).compile().unwrap();
        assert_eq!(c.intersections(Plane::Cl, 15.).points.len(), 2);
    }
    #[test]
    fn saved_project_reproduces_curve_and_view() {
        let mut p = Project::preset(3);
        p.cursor = 0.37;
        p.slices = [0.42, 0.13, 220.];
        p.yaw = 1.2;
        let encoded = serde_json::to_string(&p).unwrap();
        let restored: Project = serde_json::from_str(&encoded).unwrap();
        assert_eq!(
            p.compile()
                .unwrap()
                .sample(37)
                .iter()
                .map(|s| s.color)
                .collect::<Vec<_>>(),
            restored
                .compile()
                .unwrap()
                .sample(37)
                .iter()
                .map(|s| s.color)
                .collect::<Vec<_>>()
        );
        assert_eq!(p.slices, restored.slices);
        assert_eq!(p.cursor, restored.cursor);
        assert_eq!(p.yaw, restored.yaw);
        let mut invalid = restored;
        invalid.sample_count = usize::MAX;
        assert!(invalid.compile().is_err());
    }
    #[test]
    fn old_project_files_migrate_exactly_in_all_planes_including_unwrapped_hue() {
        for plane in Plane::ALL {
            for count in [2, 3] {
                let (x, y, _) = plane.axes();
                let normalized = [[-0.1, 0.1], [0.4, 0.8], [1.2, 0.2]];
                let raw: Vec<_> = normalized
                    .iter()
                    .take(count)
                    .map(|p| plane.from_point(*p))
                    .collect();
                let p = Project {
                    mode: Mode::Spline(plane),
                    spline: None,
                    nodes: raw.clone(),
                    ..Default::default()
                };
                // No new spline field is present in the old JSON format.
                let encoded = serde_json::to_string(&p).unwrap();
                assert!(!encoded.contains("\"spline\""));
                let mut restored: Project = serde_json::from_str(&encoded).unwrap();
                restored.migrate_spline().unwrap();
                assert!(restored.nodes.is_empty());
                let curve = restored.compile().unwrap();
                for i in 0..=100 {
                    let t = i as f64 / 100.;
                    let old = interpolate(&raw, t);
                    let new = curve.at(t);
                    assert!((new.channel(x) - old[0]).abs() < 1e-9);
                    assert!((new.channel(y) - old[1]).abs() < 1e-9);
                }
            }
        }
    }
    #[test]
    fn cubic_project_roundtrip_keeps_constraints_and_all_channels() {
        use crate::spline::{NodeMode, Side};
        let mut p = Project::default();
        let s = p.spline.as_mut().unwrap();
        s.set_symmetry(true);
        s.drag_handle(0, Side::Outgoing, [0.2, 0.4]);
        s.set_middle_mode(NodeMode::Auto);
        let encoded = serde_json::to_string(&p).unwrap();
        assert!(!encoded.contains("\"nodes\":[["));
        let restored: Project = serde_json::from_str(&encoded).unwrap();
        assert_eq!(p.spline, restored.spline);
        let a = p.compile().unwrap();
        let b = restored.compile().unwrap();
        for i in 0..=100 {
            assert_eq!(a.at(i as f64 / 100.), b.at(i as f64 / 100.));
        }
    }
    #[test]
    fn hue_views_pan_periodically_without_changing_curve_or_export() {
        let mut p = Project::preset(1);
        p.expressions = ["0.6".into(), "0.1".into(), "350+20*t".into()];
        let original = p.compile().unwrap();
        p.pan_hue(1. / 6.);
        assert!((p.hue_origin - 300.).abs() < 1e-10);
        for plane in [Plane::Ch, Plane::Lh] {
            let a = plane.project_at(original.at(0.), p.hue_origin);
            let b = plane.project_at(original.at(1.), p.hue_origin);
            assert!((b[0] - a[0] - 20. / 360.).abs() < 1e-10);
            for pair in original.sample(101).windows(2) {
                assert!(p.connected_in_view(pair[0].color, pair[1].color, Some(plane)));
            }
        }
        p.pan_hue(4.);
        assert!((p.hue_origin - 300.).abs() < 1e-10);
        p.polar = true;
        for plane in [Plane::Ch, Plane::Lh] {
            let mut spline = p.clone();
            spline.enable_spline(plane, &original);
            let converted = spline.compile().unwrap();
            for i in 0..=100 {
                assert!(
                    (converted.at(i as f64 / 100.).h - original.at(i as f64 / 100.).h).abs() < 1e-9
                );
            }
        }
        let before =
            crate::exchange::export(&original, 31, crate::exchange::ExportSpace::Oklch, false);
        let after = crate::exchange::export(
            &p.compile().unwrap(),
            31,
            crate::exchange::ExportSpace::Oklch,
            false,
        );
        assert_eq!(before, after);
        let restored: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(restored.hue_origin, p.hue_origin);
        assert!(restored.polar);
        let mut json = serde_json::to_value(&p).unwrap();
        json.as_object_mut().unwrap().remove("hue_origin");
        json.as_object_mut().unwrap().remove("polar");
        let old: Project = serde_json::from_value(json).unwrap();
        assert_eq!(old.hue_origin, 0.);
        assert!(!old.polar);
    }
    #[test]
    fn spline_drag_across_zero_uses_unwrapped_hue_and_periodic_handle_hits() {
        use crate::spline::{Drag, Side, Target, hit_test};
        for plane in [Plane::Ch, Plane::Lh] {
            let s = Spline::from_legacy(&[[350. / 360., 0.2], [1., 0.4], [370. / 360., 0.3]]);
            let copies = plane.spline_images(&s, 300.);
            let anchor = plane.project_at(Oklch::new(0.2, 0.08, 350.), 300.);
            assert!(
                copies
                    .iter()
                    .any(|s| hit_test(s, anchor, [900., 400.]) == Some(Target::Anchor(0)))
            );
            let mut drag = Drag::new(s.clone(), Target::Anchor(0), anchor).cyclic_x(true);
            let moved = drag.update([anchor[0] + 30. / 360., anchor[1]], false, [900., 400.]);
            assert!((moved.nodes[0].position[0] * 360. - 380.).abs() < 1e-10);
            assert_eq!(moved.nodes[0].outgoing, s.nodes[0].outgoing);
            let start = s.nodes[1].handle_position(Side::Outgoing).unwrap();
            let image = plane.spline_images(&s, 300.).remove(0);
            let pos = image.nodes[1].handle_position(Side::Outgoing).unwrap();
            assert!((pos[0] - (start[0] - 300. / 360.)).abs() < 1e-10);
            let zero_copies = plane.spline_images(&s, 0.);
            let wrapped_handle = zero_copies[1].nodes[1]
                .handle_position(Side::Outgoing)
                .unwrap();
            assert_eq!(
                zero_copies
                    .iter()
                    .find_map(|s| hit_test(s, wrapped_handle, [900., 400.])),
                Some(Target::Handle(1, Side::Outgoing))
            );
            let full = Spline::line([0., 0.3], [1., 0.3]);
            let images = plane.spline_images(&full, 0.);
            assert_eq!(
                images
                    .iter()
                    .find_map(|s| hit_test(s, [0., 0.3], [900., 400.])),
                Some(Target::Anchor(0))
            );
            assert_eq!(
                images
                    .iter()
                    .find_map(|s| hit_test(s, [1., 0.3], [900., 400.])),
                Some(Target::Anchor(1))
            );
            assert_eq!(hue_position(450., 90.), hue_position(450., 450.));
        }
    }
    #[test]
    fn imported_hue_uses_the_short_arc_and_projection_keeps_360_endpoint() {
        let p = Project {
            mode: Mode::Imported,
            imported: vec![Oklch::new(0.5, 0.1, 350.), Oklch::new(0.7, 0.2, 10.)],
            ..Default::default()
        };
        let c = p.compile().unwrap();
        let middle = c.at(0.5);
        assert!((middle.h - 360.).abs() < 1e-10);
        assert!((middle.l - 0.6).abs() < 1e-10);
        assert_eq!(Plane::Ch.project(middle)[0], 1.);
        assert!(connected(c.at(0.49), middle));
        assert!(connected(middle, c.at(0.51)));
        assert!(!p.connected_in_view(middle, c.at(0.51), Some(Plane::Ch)));
        assert!(
            c.intersections(Plane::Cl, 0.)
                .points
                .iter()
                .any(|s| (s.t - 0.5).abs() < 1e-6)
        );
    }
}
