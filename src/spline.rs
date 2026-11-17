//! Cubic Bézier editing in normalized projection coordinates. Handles are
//! offsets from their anchor, so moving an anchor preserves its handles.
use serde::{Deserialize, Serialize};

pub type Point = [f64; 2];
const EPS: f64 = 1e-10;
const MIN_HANDLE: f64 = 1e-6;
pub fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1]]
}
pub fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}
pub fn mul(a: Point, k: f64) -> Point {
    [a[0] * k, a[1] * k]
}
pub fn length(a: Point) -> f64 {
    a[0].hypot(a[1])
}
fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn mix(a: Point, b: Point, t: f64) -> Point {
    add(mul(a, 1. - t), mul(b, t))
}
fn mirror(v: Point) -> Point {
    [-v[0], v[1]]
}
fn unit(v: Point, fallback: Point) -> Point {
    let n = length(v);
    if n > EPS {
        mul(v, 1. / n)
    } else {
        let n = length(fallback);
        if n > EPS {
            mul(fallback, 1. / n)
        } else {
            [1., 0.]
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeMode {
    #[default]
    Corner,
    Smooth,
    Auto,
    Symmetric,
}
impl NodeMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Corner => "Corner",
            Self::Smooth => "Smooth",
            Self::Auto => "Auto C2",
            Self::Symmetric => "Symm. smooth",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Incoming,
    Outgoing,
}
impl Side {
    pub fn opposite(self) -> Self {
        match self {
            Self::Incoming => Self::Outgoing,
            Self::Outgoing => Self::Incoming,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub position: Point,
    pub incoming: Option<Point>,
    pub outgoing: Option<Point>,
    #[serde(default)]
    pub mode: NodeMode,
}
impl Node {
    fn new(position: Point) -> Self {
        Self {
            position,
            incoming: None,
            outgoing: None,
            mode: NodeMode::Corner,
        }
    }
    pub fn handle(&self, side: Side) -> Option<Point> {
        match side {
            Side::Incoming => self.incoming,
            Side::Outgoing => self.outgoing,
        }
    }
    fn set_handle(&mut self, side: Side, value: Option<Point>) {
        match side {
            Side::Incoming => self.incoming = value,
            Side::Outgoing => self.outgoing = value,
        }
    }
    pub fn handle_position(&self, side: Side) -> Option<Point> {
        self.handle(side).map(|h| add(self.position, h))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spline {
    pub nodes: Vec<Node>,
    /// Global parameter of the middle anchor. Independent smooth handle
    /// lengths require correspondingly different segment durations for C1.
    pub knot: f64,
    #[serde(default)]
    pub linear: bool,
    #[serde(default)]
    pub symmetric: bool,
}
impl Spline {
    pub fn line(a: Point, b: Point) -> Self {
        let mut first = Node::new(a);
        let mut last = Node::new(b);
        first.outgoing = Some(mul(sub(b, a), 1. / 3.));
        last.incoming = Some(mul(sub(a, b), 1. / 3.));
        Self {
            nodes: vec![first, last],
            knot: 0.5,
            linear: true,
            symmetric: false,
        }
    }
    /// Degree-elevate the old interpolating quadratic, then split its cubic
    /// exactly. Both its geometry and its parameterization are preserved.
    pub fn from_legacy(points: &[Point]) -> Self {
        let mut s = Self::line(points[0], points[points.len() - 1]);
        if points.len() == 3 {
            s.linear = false;
            let q = sub(mul(points[1], 2.), mul(add(points[0], points[2]), 0.5));
            s.nodes[0].outgoing = Some(mul(sub(q, points[0]), 2. / 3.));
            s.nodes[1].incoming = Some(mul(sub(q, points[2]), 2. / 3.));
            s.add_middle();
        }
        s
    }
    pub fn at(&self, t: f64) -> Point {
        if t <= 0. {
            return self.nodes[0].position;
        }
        if t >= 1. {
            return self.nodes.last().unwrap().position;
        }
        if self.linear {
            return mix(self.nodes[0].position, self.nodes[1].position, t);
        }
        let (i, u) = if self.nodes.len() == 2 {
            (0, t)
        } else if t <= self.knot {
            (0, t / self.knot)
        } else {
            (1, (t - self.knot) / (1. - self.knot))
        };
        let [a, b, c, d] = self.controls(i);
        let v = 1. - u;
        add(
            add(mul(a, v * v * v), mul(b, 3. * v * v * u)),
            add(mul(c, 3. * v * u * u), mul(d, u * u * u)),
        )
    }
    pub fn controls(&self, i: usize) -> [Point; 4] {
        let a = &self.nodes[i];
        let b = &self.nodes[i + 1];
        [
            a.position,
            a.handle_position(Side::Outgoing).unwrap_or(a.position),
            b.handle_position(Side::Incoming).unwrap_or(b.position),
            b.position,
        ]
    }
    pub fn parameter(&self, i: usize) -> f64 {
        if i == 0 {
            0.
        } else if i == self.nodes.len() - 1 {
            1.
        } else {
            self.knot
        }
    }
    pub fn axis(&self) -> f64 {
        (self.nodes[0].position[0] + self.nodes.last().unwrap().position[0]) * 0.5
    }
    pub fn add_middle(&mut self) {
        if self.nodes.len() != 2 {
            return;
        }
        if self.linear {
            let delta = sub(self.nodes[1].position, self.nodes[0].position);
            self.nodes[0].outgoing = Some(mul(delta, 1. / 3.));
            self.nodes[1].incoming = Some(mul(delta, -1. / 3.));
        }
        self.linear = false;
        let [a, b, c, d] = self.controls(0);
        let [ab, bc, cd] = [mix(a, b, 0.5), mix(b, c, 0.5), mix(c, d, 0.5)];
        let [left, right] = [mix(ab, bc, 0.5), mix(bc, cd, 0.5)];
        let p = mix(left, right, 0.5);
        self.nodes[0].outgoing = Some(sub(ab, a));
        self.nodes[1].incoming = Some(sub(cd, d));
        let middle = Node {
            position: p,
            incoming: Some(sub(left, p)),
            outgoing: Some(sub(right, p)),
            mode: NodeMode::Smooth,
        };
        self.nodes.insert(1, middle);
        self.knot = 0.5;
        if self.symmetric {
            self.enforce_symmetry(None);
        }
    }
    pub fn remove_middle(&mut self) {
        if self.nodes.len() != 3 {
            return;
        }
        // Preserve endpoint derivatives; this exactly reverses an untouched
        // de Casteljau split and is predictable when deleting an edited node.
        self.nodes[0].outgoing = self.nodes[0].outgoing.map(|v| mul(v, 1. / self.knot));
        self.nodes[2].incoming = self.nodes[2]
            .incoming
            .map(|v| mul(v, 1. / (1. - self.knot)));
        self.nodes.remove(1);
        self.knot = 0.5;
        if self.symmetric {
            self.enforce_symmetry(None);
        }
    }
    pub fn set_linear(&mut self, linear: bool) {
        if linear {
            self.remove_middle();
        }
        self.linear = linear;
    }
    fn break_auto(&mut self) {
        if self.nodes.len() == 3 && self.nodes[1].mode == NodeMode::Auto {
            self.nodes[1].mode = NodeMode::Smooth;
        }
    }
    pub fn move_node(&mut self, i: usize, position: Point) {
        if i >= self.nodes.len() {
            return;
        }
        self.break_auto();
        self.nodes[i].position = position;
        if self.symmetric {
            self.enforce_symmetry(Some(i));
        }
    }
    fn middle_direction(&self) -> Point {
        let node = &self.nodes[1];
        let incoming = node.incoming.unwrap_or([0.; 2]);
        let outgoing = node.outgoing.unwrap_or([0.; 2]);
        // Closest rotation of the handle pair with each length held fixed.
        unit(
            sub(
                mul(outgoing, length(outgoing)),
                mul(incoming, length(incoming)),
            ),
            sub(self.nodes[2].position, self.nodes[0].position),
        )
    }
    fn smooth_middle(&mut self, equal: bool) {
        let p = self.nodes[1].position;
        let mut direction = self.middle_direction();
        let mut ri = self.nodes[1].incoming.map(length).unwrap_or(0.);
        let mut ro = self.nodes[1].outgoing.map(length).unwrap_or(0.);
        if ri < EPS {
            ri = (length(sub(p, self.nodes[0].position)) / 3.).max(MIN_HANDLE);
        }
        if ro < EPS {
            ro = (length(sub(self.nodes[2].position, p)) / 3.).max(MIN_HANDLE);
        }
        if equal {
            ri = (ri + ro) * 0.5;
            ro = ri;
        }
        if self.symmetric {
            direction = if direction[0] < 0. {
                [-1., 0.]
            } else {
                [1., 0.]
            };
            ri = (ri + ro) * 0.5;
            ro = ri;
        }
        let incoming = Some(mul(direction, -ri));
        let outgoing = Some(mul(direction, ro));
        // Avoid a numerical rotation of an already aligned pair.
        let old = &self.nodes[1];
        let aligned = old.incoming.zip(old.outgoing).is_some_and(|(a, b)| {
            length(sub(
                mul(a, -1. / length(a).max(EPS)),
                mul(b, 1. / length(b).max(EPS)),
            )) < 1e-10
        });
        if !aligned || equal || self.symmetric || old.incoming.is_none() || old.outgoing.is_none() {
            self.nodes[1].incoming = incoming;
            self.nodes[1].outgoing = outgoing;
        }
        self.update_knot();
    }
    pub fn set_middle_mode(&mut self, mode: NodeMode) {
        if self.nodes.len() != 3 || self.linear {
            return;
        }
        match mode {
            NodeMode::Corner => {
                if self.nodes[1].mode == NodeMode::Corner {
                    self.nodes[1].incoming = None;
                    self.nodes[1].outgoing = None;
                }
                self.nodes[1].mode = mode;
            }
            NodeMode::Smooth | NodeMode::Symmetric => {
                self.smooth_middle(mode == NodeMode::Symmetric);
                self.nodes[1].mode = mode;
            }
            NodeMode::Auto => {
                let h = self.knot;
                let r = 1. - h;
                let p = self.nodes[1].position;
                let a = self.nodes[0]
                    .handle_position(Side::Outgoing)
                    .unwrap_or(self.nodes[0].position);
                let b = self.nodes[2]
                    .handle_position(Side::Incoming)
                    .unwrap_or(self.nodes[2].position);
                // Solve both first- and second-derivative matching at the
                // existing knot; anchors and endpoint handles stay fixed.
                let d = mul(sub(mul(sub(b, p), h / r), mul(sub(a, p), r / h)), 1.5);
                self.nodes[1].incoming = Some(mul(d, -h / 3.));
                self.nodes[1].outgoing = Some(mul(d, r / 3.));
                self.nodes[1].mode = mode;
            }
        }
    }
    fn update_knot(&mut self) {
        if self.nodes.len() != 3 {
            return;
        }
        let node = &self.nodes[1];
        let ri = node.incoming.map(length).unwrap_or(0.);
        let ro = node.outgoing.map(length).unwrap_or(0.);
        if ri + ro > EPS {
            self.knot = (ri / (ri + ro)).clamp(1e-8, 1. - 1e-8);
        }
    }
    pub fn drag_handle(&mut self, i: usize, side: Side, mut vector: Point) {
        if self.linear || i >= self.nodes.len() {
            return;
        }
        self.break_auto();
        let is_middle = self.nodes.len() == 3 && i == 1;
        let mode = self.nodes[i].mode;
        if is_middle && mode != NodeMode::Corner {
            let opposite = self.nodes[i].handle(side.opposite()).unwrap_or([0.; 2]);
            let opposite_length = length(opposite);
            if self.symmetric {
                vector[1] = 0.;
            }
            if mode == NodeMode::Smooth && opposite_length < EPS {
                vector = [0.; 2];
            } else if length(vector) < MIN_HANDLE {
                vector = mul(
                    unit(self.nodes[i].handle(side).unwrap_or([1., 0.]), [1., 0.]),
                    MIN_HANDLE,
                );
            }
            let other = if mode == NodeMode::Symmetric || self.symmetric {
                mul(vector, -1.)
            } else {
                mul(unit(vector, [1., 0.]), -opposite_length)
            };
            self.nodes[i].set_handle(side.opposite(), Some(other));
        }
        self.nodes[i].set_handle(side, Some(vector));
        if self.symmetric {
            if is_middle {
                self.nodes[i].set_handle(side.opposite(), Some(mirror(vector)));
            } else {
                let other = self.nodes.len() - 1 - i;
                self.nodes[other].set_handle(side.opposite(), Some(mirror(vector)));
            }
        }
        if is_middle && mode != NodeMode::Corner {
            self.update_knot();
        }
    }
    pub fn set_symmetry(&mut self, enabled: bool) {
        self.symmetric = enabled;
        if enabled {
            self.enforce_symmetry(None);
        }
    }
    fn enforce_symmetry(&mut self, moved: Option<usize>) {
        let last = self.nodes.len() - 1;
        let height = match moved {
            Some(0) => self.nodes[0].position[1],
            Some(i) if i == last => self.nodes[last].position[1],
            _ => 0.5 * (self.nodes[0].position[1] + self.nodes[last].position[1]),
        };
        self.nodes[0].position[1] = height;
        self.nodes[last].position[1] = height;
        let a = self.nodes[0].outgoing;
        let b = self.nodes[last].incoming;
        let handle = match moved {
            Some(i) if i == last => b.map(mirror),
            Some(0) => a,
            _ => match (a, b) {
                (None, None) => None,
                _ => Some(mul(
                    add(a.unwrap_or([0.; 2]), mirror(b.unwrap_or([0.; 2]))),
                    0.5,
                )),
            },
        };
        self.nodes[0].outgoing = handle;
        self.nodes[last].incoming = handle.map(mirror);
        self.knot = 0.5;
        if self.nodes.len() == 3 {
            self.nodes[1].position[0] = self.axis();
            let mode = self.nodes[1].mode;
            if mode == NodeMode::Auto {
                self.set_middle_mode(NodeMode::Auto);
            } else if mode != NodeMode::Corner {
                self.smooth_middle(true);
            } else {
                let a = self.nodes[1].incoming;
                let b = self.nodes[1].outgoing;
                let v = match (a, b) {
                    (None, None) => None,
                    _ => Some(mul(
                        add(a.unwrap_or([0.; 2]), mirror(b.unwrap_or([0.; 2]))),
                        0.5,
                    )),
                };
                self.nodes[1].incoming = v;
                self.nodes[1].outgoing = v.map(mirror);
            }
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !(2..=3).contains(&self.nodes.len()) || (self.linear && self.nodes.len() != 2) {
            return Err("A spline needs two or three nodes; linear mode needs two".into());
        }
        if !self.knot.is_finite() || !(1e-8..=1. - 1e-8).contains(&self.knot) {
            return Err("Spline knot must lie strictly between 0 and 1".into());
        }
        if self.nodes.iter().any(|n| {
            n.position
                .into_iter()
                .chain(n.incoming.into_iter().flatten())
                .chain(n.outgoing.into_iter().flatten())
                .any(|v| !v.is_finite())
        }) {
            return Err("Spline nodes and handles must be finite".into());
        }
        Ok(())
    }
}

/// Ctrl node snapping uses screen-space travel, including a non-square plot.
pub fn axis_delta(delta: Point, metric: Point) -> Point {
    if (delta[0] * metric[0]).abs() >= (delta[1] * metric[1]).abs() {
        [delta[0], 0.]
    } else {
        [0., delta[1]]
    }
}
/// Choose the closest of the x, y, and original handle-direction lines in
/// screen space. The anchor remains fixed and only the handle is projected.
pub fn snap_handle(vector: Point, original: Point, metric: Point) -> Point {
    let v = [vector[0] * metric[0], vector[1] * metric[1]];
    let original = [original[0] * metric[0], original[1] * metric[1]];
    let mut candidates = vec![[v[0], 0.], [0., v[1]]];
    if length(original) > EPS {
        let u = unit(original, [1., 0.]);
        candidates.push(mul(u, dot(v, u)));
    }
    let result = candidates
        .into_iter()
        .min_by(|a, b| length(sub(*a, v)).total_cmp(&length(sub(*b, v))))
        .unwrap();
    [
        result[0] / metric[0].max(EPS),
        result[1] / metric[1].max(EPS),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Anchor(usize),
    Handle(usize, Side),
}
impl Target {
    pub fn node(self) -> usize {
        match self {
            Self::Anchor(i) | Self::Handle(i, _) => i,
        }
    }
}

pub fn hit_test(spline: &Spline, p: Point, metric: Point) -> Option<Target> {
    let distance = |q: Point| ((q[0] - p[0]) * metric[0]).hypot((q[1] - p[1]) * metric[1]);
    let mut closest: Option<(f64, Target)> = None;
    for (i, node) in spline.nodes.iter().enumerate() {
        let d = distance(node.position);
        if d < 10. && closest.is_none_or(|(best, _)| d < best) {
            closest = Some((d, Target::Anchor(i)));
        }
    }
    if !spline.linear {
        for (i, n) in spline.nodes.iter().enumerate() {
            for side in [Side::Incoming, Side::Outgoing] {
                if (i == 0 && side == Side::Incoming)
                    || (i == spline.nodes.len() - 1 && side == Side::Outgoing)
                {
                    continue;
                }
                if let Some(q) = n.handle_position(side) {
                    let d = distance(q);
                    if d < 9. && closest.is_none_or(|(best, _)| d < best) {
                        closest = Some((d, Target::Handle(i, side)));
                    }
                }
            }
        }
    }
    closest.map(|(_, target)| target)
}

/// Reapply every pointer update to the drag-start geometry. This prevents
/// cumulative constraint drift and makes releasing Ctrl restore free motion.
pub struct Drag {
    pub target: Target,
    initial: Spline,
    start: Point,
    axis: Option<usize>,
    cyclic_x: bool,
}
impl Drag {
    pub fn new(initial: Spline, target: Target, start: Point) -> Self {
        Self {
            target,
            initial,
            start,
            axis: None,
            cyclic_x: false,
        }
    }
    pub fn cyclic_x(mut self, enabled: bool) -> Self {
        self.cyclic_x = enabled;
        self
    }
    pub fn update(&mut self, p: Point, ctrl: bool, metric: Point) -> Spline {
        let mut result = self.initial.clone();
        let mut delta = sub(p, self.start);
        match self.target {
            Target::Anchor(i) => {
                if ctrl {
                    if self.axis.is_none() && length(delta) > EPS {
                        self.axis = Some(
                            if (delta[0] * metric[0]).abs() >= (delta[1] * metric[1]).abs() {
                                0
                            } else {
                                1
                            },
                        );
                    }
                    if let Some(axis) = self.axis {
                        delta[1 - axis] = 0.;
                    }
                } else {
                    self.axis = None;
                }
                let mut pos = add(self.initial.nodes[i].position, delta);
                if !self.cyclic_x {
                    pos[0] = pos[0].clamp(0., 1.);
                }
                pos[1] = pos[1].clamp(0., 1.);
                result.move_node(i, pos);
            }
            Target::Handle(i, side) => {
                let original = self.initial.nodes[i].handle(side).unwrap_or([0.; 2]);
                let mut vector = add(original, delta);
                if ctrl {
                    vector = snap_handle(vector, original, metric);
                }
                result.drag_handle(i, side, vector);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn close(a: Point, b: Point) {
        assert!(length(sub(a, b)) < 1e-9, "{a:?} != {b:?}");
    }
    fn example() -> Spline {
        Spline::from_legacy(&[[0.05, 0.1], [0.45, 0.8], [0.95, 0.2]])
    }
    // Derivatives in the global [0,1] parameter, independently from the editor.
    fn join(s: &Spline) -> (Point, Point, Point, Point) {
        let [a, b, c, d] = s.controls(0);
        let [e, f, g, h] = s.controls(1);
        let k = s.knot;
        let r = 1. - k;
        let left_d = mul(sub(d, c), 3. / k);
        let right_d = mul(sub(f, e), 3. / r);
        let left_dd = mul(add(sub(d, mul(c, 2.)), b), 6. / (k * k));
        let right_dd = mul(add(sub(g, mul(f, 2.)), e), 6. / (r * r));
        let _ = (a, h);
        (left_d, right_d, left_dd, right_dd)
    }
    #[test]
    fn legacy_degree_elevation_and_split_preserve_the_entire_curve() {
        let points = [[0.1, 0.2], [0.37, 0.9], [0.95, 0.1]];
        let s = Spline::from_legacy(&points);
        for i in 0..=100 {
            let t = i as f64 / 100.;
            close(s.at(t), crate::model::interpolate(&points, t));
        }
        let (a, b, c, d) = join(&s);
        close(a, b);
        close(c, d);
        let mut two = s.clone();
        two.remove_middle();
        for i in 0..=100 {
            let t = i as f64 / 100.;
            close(two.at(t), s.at(t));
        }
        two.add_middle();
        for i in 0..=100 {
            let t = i as f64 / 100.;
            close(two.at(t), s.at(t));
        }
    }
    #[test]
    fn corner_is_a_two_step_action_and_smooth_restores_handles() {
        let mut s = example();
        let old = s.clone();
        s.set_middle_mode(NodeMode::Corner);
        assert_eq!(s.nodes[1].incoming, old.nodes[1].incoming);
        assert_eq!(s.nodes[1].outgoing, old.nodes[1].outgoing);
        s.drag_handle(1, Side::Incoming, [-0.12, 0.17]);
        let untouched = s.nodes[1].outgoing;
        assert_eq!(untouched, old.nodes[1].outgoing);
        s.set_middle_mode(NodeMode::Corner);
        assert_eq!(s.nodes[1].incoming, None);
        assert_eq!(s.nodes[1].outgoing, None);
        s.set_middle_mode(NodeMode::Smooth);
        assert!(s.nodes[1].incoming.is_some());
        let (a, b, _, _) = join(&s);
        close(a, b);
    }
    #[test]
    fn smooth_preserves_lengths_snaps_to_nearest_direction_and_is_idempotent() {
        let mut s = example();
        s.set_middle_mode(NodeMode::Corner);
        s.drag_handle(1, Side::Incoming, [-0.2, 0.1]);
        s.drag_handle(1, Side::Outgoing, [0.4, 0.2]);
        let ri = length(s.nodes[1].incoming.unwrap());
        let ro = length(s.nodes[1].outgoing.unwrap());
        let expected = unit(sub(mul([0.4, 0.2], ro), mul([-0.2, 0.1], ri)), [1., 0.]);
        s.set_middle_mode(NodeMode::Smooth);
        close(s.nodes[1].incoming.unwrap(), mul(expected, -ri));
        close(s.nodes[1].outgoing.unwrap(), mul(expected, ro));
        let unchanged = s.clone();
        s.set_middle_mode(NodeMode::Smooth);
        assert_eq!(s, unchanged);
        s.drag_handle(1, Side::Outgoing, [0.25, -0.3]);
        assert!((length(s.nodes[1].incoming.unwrap()) - ri).abs() < 1e-12);
        let (a, b, _, _) = join(&s);
        close(a, b);
    }
    #[test]
    fn auto_solves_c2_at_a_nonuniform_knot_then_manual_edit_keeps_c1() {
        let mut s = example();
        s.drag_handle(1, Side::Outgoing, [0.3, 0.15]);
        let anchors: Vec<_> = s.nodes.iter().map(|n| n.position).collect();
        let ends = (s.nodes[0].outgoing, s.nodes[2].incoming);
        s.set_middle_mode(NodeMode::Auto);
        let (a, b, c, d) = join(&s);
        close(a, b);
        close(c, d);
        assert_eq!(
            anchors,
            s.nodes.iter().map(|n| n.position).collect::<Vec<_>>()
        );
        assert_eq!(ends, (s.nodes[0].outgoing, s.nodes[2].incoming));
        let len = length(s.nodes[1].incoming.unwrap());
        s.drag_handle(1, Side::Outgoing, [0.4, -0.15]);
        assert_eq!(s.nodes[1].mode, NodeMode::Smooth);
        assert!((length(s.nodes[1].incoming.unwrap()) - len).abs() < 1e-12);
        let (a, b, c, d) = join(&s);
        close(a, b);
        assert!(length(sub(c, d)) > 1e-3);
    }
    #[test]
    fn symmetric_smooth_and_anchor_translation_keep_their_constraints() {
        let mut s = example();
        s.set_middle_mode(NodeMode::Symmetric);
        s.drag_handle(1, Side::Incoming, [-0.3, 0.12]);
        close(s.nodes[1].outgoing.unwrap(), [0.3, -0.12]);
        let (a, b, _, _) = join(&s);
        close(a, b);
        for i in 0..3 {
            let handles = (s.nodes[i].incoming, s.nodes[i].outgoing);
            s.move_node(i, [0.2 + i as f64 * 0.2, 0.4]);
            assert_eq!(handles, (s.nodes[i].incoming, s.nodes[i].outgoing));
        }
        let (a, b, _, _) = join(&s);
        close(a, b);
    }
    #[test]
    fn symmetry_tracks_the_axis_without_moving_the_other_endpoint_x() {
        for three in [false, true] {
            let mut s = example();
            if !three {
                s.remove_middle();
            }
            s.set_symmetry(true);
            let last = s.nodes.len() - 1;
            let right_x = s.nodes[last].position[0];
            s.move_node(0, [0.2, 0.3]);
            assert_eq!(s.nodes[last].position, [right_x, 0.3]);
            s.drag_handle(0, Side::Outgoing, [0.15, 0.22]);
            close(s.nodes[last].incoming.unwrap(), [-0.15, 0.22]);
            if three {
                assert_eq!(s.nodes[1].position[0], s.axis());
                s.move_node(1, [0.05, 0.7]);
                assert_eq!(s.nodes[1].position, [s.axis(), 0.7]);
                s.set_middle_mode(NodeMode::Corner);
                s.drag_handle(1, Side::Incoming, [-0.1, 0.15]);
                close(s.nodes[1].outgoing.unwrap(), [0.1, 0.15]);
                s.set_middle_mode(NodeMode::Smooth);
                close(
                    s.nodes[1].incoming.unwrap(),
                    mul(s.nodes[1].outgoing.unwrap(), -1.),
                );
                s.set_middle_mode(NodeMode::Auto);
                let (a, b, c, d) = join(&s);
                close(a, b);
                close(c, d);
            }
            for i in 0..=100 {
                let t = i as f64 / 100.;
                let a = s.at(t);
                let b = s.at(1. - t);
                close([2. * s.axis() - a[0], a[1]], b);
            }
        }
    }
    #[test]
    fn line_mode_removes_the_middle_and_uses_no_handles() {
        let mut s = example();
        s.set_linear(true);
        assert_eq!(s.nodes.len(), 2);
        let a = s.nodes[0].position;
        let b = s.nodes[1].position;
        for i in 0..=10 {
            let t = i as f64 / 10.;
            close(s.at(t), mix(a, b, t));
        }
        let old = s.clone();
        s.drag_handle(0, Side::Outgoing, [10., 10.]);
        assert_eq!(s, old);
        s.set_linear(false);
        s.add_middle();
        assert_eq!(s.nodes.len(), 3);
        assert!(!s.linear);
    }
    #[test]
    fn ctrl_snapping_uses_screen_metric_and_original_handle_line() {
        assert_eq!(axis_delta([0.1, 0.15], [800., 200.]), [0.1, 0.]);
        assert_eq!(axis_delta([0.1, 0.5], [800., 200.]), [0., 0.5]);
        close(
            snap_handle([0.3, 0.01], [0.1, 0.2], [800., 300.]),
            [0.3, 0.],
        );
        close(
            snap_handle([0.01, 0.4], [0.1, 0.2], [800., 300.]),
            [0., 0.4],
        );
        let v = snap_handle([0.21, 0.39], [0.1, 0.2], [800., 300.]);
        assert!((v[1] - 2. * v[0]).abs() < 1e-12);
    }
    #[test]
    fn serialized_spline_keeps_every_handle_mode_and_constraint() {
        let mut s = example();
        s.set_symmetry(true);
        s.set_middle_mode(NodeMode::Auto);
        let restored: Spline = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(s, restored);
        assert!(restored.validate().is_ok());
        let mut invalid = s;
        invalid.nodes[0].position[0] = f64::NAN;
        assert!(invalid.validate().is_err());
    }
    #[test]
    fn drag_hit_testing_modifiers_and_constraint_updates_share_the_ui_path() {
        let s = example();
        let metric = [800., 300.];
        assert_eq!(
            hit_test(&s, s.nodes[1].position, metric),
            Some(Target::Anchor(1))
        );
        let start = s.nodes[1].handle_position(Side::Outgoing).unwrap();
        assert_eq!(
            hit_test(&s, start, metric),
            Some(Target::Handle(1, Side::Outgoing))
        );
        let mut drag = Drag::new(s.clone(), Target::Anchor(1), s.nodes[1].position);
        let moved = drag.update(add(s.nodes[1].position, [0.12, 0.1]), true, metric);
        close(
            moved.nodes[1].position,
            add(s.nodes[1].position, [0.12, 0.]),
        );
        let moved = drag.update(add(s.nodes[1].position, [0.01, 0.15]), true, metric);
        close(
            moved.nodes[1].position,
            add(s.nodes[1].position, [0.01, 0.]),
        ); // latched x
        let moved = drag.update(add(s.nodes[1].position, [0.12, 0.1]), false, metric);
        close(
            moved.nodes[1].position,
            add(s.nodes[1].position, [0.12, 0.1]),
        );
        assert_eq!(moved.nodes[1].incoming, s.nodes[1].incoming);
        assert_eq!(moved.nodes[1].outgoing, s.nodes[1].outgoing);
        let mut handle_drag = Drag::new(s.clone(), Target::Handle(1, Side::Outgoing), start);
        let moved = handle_drag.update(add(s.nodes[1].position, [0.3, 0.003]), true, metric);
        assert!(moved.nodes[1].outgoing.unwrap()[1].abs() < 1e-12);
        let (a, b, _, _) = join(&moved);
        close(a, b);
        let mut direct = Drag::new(s.clone(), Target::Handle(1, Side::Outgoing), start);
        let end = add(start, [0.07, 0.04]);
        assert_eq!(
            handle_drag.update(end, false, metric),
            direct.update(end, false, metric)
        );
        let mut line = s;
        line.set_linear(true);
        let handle = line.nodes[0].handle_position(Side::Outgoing).unwrap();
        assert_eq!(hit_test(&line, handle, metric), None);
        let before = line.clone();
        line.add_middle();
        for i in 0..=30 {
            let t = i as f64 / 30.;
            close(before.at(t), line.at(t));
        }
    }
}
