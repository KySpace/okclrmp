//! Tessellate the actual RGB boundary faces, sharing their saturated edges.
//! A uniform H/L grid cuts across these moving creases and gives a sawtooth
//! silhouette. RGB-face patches also preserve the real non-convex blue fold.
use crate::color::Oklch;
use bytemuck::{Pod, Zeroable};

const LOWER_STEPS: usize = 32;
const UPPER_STEPS: usize = 64;
const ROWS: usize = LOWER_STEPS + UPPER_STEPS + 1;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

pub(crate) struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

#[derive(Clone, Copy)]
struct EdgePoint {
    rgb: [f64; 3],
    lch: Oklch,
}
impl EdgePoint {
    fn new(rgb: [f64; 3]) -> Self {
        Self {
            rgb,
            lch: Oklch::from_linear_rgb(rgb),
        }
    }
}

fn hue_delta(a: f64, b: f64) -> f64 {
    (b - a + 180.).rem_euclid(360.) - 180.
}

fn subdivide_edge(a: EdgePoint, b: EdgePoint, depth: usize, points: &mut Vec<EdgePoint>) {
    let mid = EdgePoint::new(std::array::from_fn(|k| (a.rgb[k] + b.rgb[k]) * 0.5));
    let dh = hue_delta(a.lch.h, b.lch.h);
    let hue_error = hue_delta(a.lch.h + dh * 0.5, mid.lch.h).abs();
    let refine = dh.abs() > 0.5
        || hue_error > 0.015
        || (mid.lch.l - (a.lch.l + b.lch.l) * 0.5).abs() > 0.0002
        || (mid.lch.c - (a.lch.c + b.lch.c) * 0.5).abs() > 0.0002;
    if refine && depth < 16 {
        subdivide_edge(a, mid, depth + 1, points);
        subdivide_edge(mid, b, depth + 1, points);
    } else {
        points.push(b);
    }
}

fn edge_columns() -> Vec<EdgePoint> {
    let corners = [
        [1., 0., 0.],
        [1., 1., 0.],
        [0., 1., 0.],
        [0., 1., 1.],
        [0., 0., 1.],
        [1., 0., 1.],
        [1., 0., 0.],
    ];
    let mut points = vec![EdgePoint::new(corners[0])];
    for pair in corners.windows(2) {
        subdivide_edge(
            EdgePoint::new(pair[0]),
            EdgePoint::new(pair[1]),
            0,
            &mut points,
        );
    }
    points
}

fn vertex(c: Oklch, rgb: [f64; 3]) -> Vertex {
    Vertex {
        position: [
            (c.h.rem_euclid(360.) / 360. * 1.8 - 0.9) as f32,
            (c.c / 0.4 * 1.2 - 0.3) as f32,
            (c.l * 1.3 - 0.65) as f32,
        ],
        color: rgb.map(|v| v.clamp(0., 1.) as f32),
    }
}

impl Mesh {
    fn polygon(&mut self, points: &[Vertex]) {
        if points.len() < 3 {
            return;
        }
        let start = self.vertices.len() as u32;
        self.vertices.extend_from_slice(points);
        for i in 1..points.len() - 1 {
            self.indices
                .extend([start, start + i as u32, start + i as u32 + 1]);
        }
    }
    fn triangle(&mut self, indices: [u32; 3]) {
        let mut points = indices.map(|i| self.vertices[i as usize]);
        let min = points
            .iter()
            .map(|v| v.position[0])
            .fold(f32::INFINITY, f32::min);
        let max = points
            .iter()
            .map(|v| v.position[0])
            .fold(f32::NEG_INFINITY, f32::max);
        if max - min <= 0.9 {
            self.indices.extend(indices);
            return;
        }
        // Unwrap and clip a hue-seam triangle; never stretch a face from 359°
        // back across the whole L/H floor to 0°.
        for p in &mut points {
            if p.position[0] < 0. {
                p.position[0] += 1.8;
            }
        }
        let low = clip(&points, true);
        let mut high = clip(&points, false);
        self.polygon(&low);
        for p in &mut high {
            p.position[0] -= 1.8;
        }
        self.polygon(&high);
        // Close both exposed chart edges to C=0. The same intersections are
        // shared geometrically by the roof and walls, so no seam cracks form.
        let mut seam = Vec::new();
        for i in 0..3 {
            let a = points[i];
            let b = points[(i + 1) % 3];
            if (a.position[0] < 0.9) != (b.position[0] < 0.9) {
                seam.push(intersection(a, b));
            }
        }
        if seam.len() == 2 {
            for x in [-0.9, 0.9] {
                let mut a = seam[0];
                let mut b = seam[1];
                a.position[0] = x;
                b.position[0] = x;
                let floor = |v: Vertex| {
                    let l = (v.position[2] as f64 + 0.65) / 1.3;
                    Vertex {
                        position: [x, -0.3, v.position[2]],
                        color: [l.powi(3) as f32; 3],
                    }
                };
                self.polygon(&[a, b, floor(b), floor(a)]);
            }
        }
    }
}

fn intersection(mut a: Vertex, mut b: Vertex) -> Vertex {
    if a.position[0] > b.position[0] {
        std::mem::swap(&mut a, &mut b);
    }
    let t = (0.9 - a.position[0]) / (b.position[0] - a.position[0]);
    let mut v = Vertex {
        position: std::array::from_fn(|k| a.position[k] + t * (b.position[k] - a.position[k])),
        color: std::array::from_fn(|k| a.color[k] + t * (b.color[k] - a.color[k])),
    };
    v.position[0] = 0.9;
    v
}

fn clip(points: &[Vertex], low: bool) -> Vec<Vertex> {
    let mut out = Vec::new();
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let inside_a = if low {
            a.position[0] <= 0.9
        } else {
            a.position[0] >= 0.9
        };
        let inside_b = if low {
            b.position[0] <= 0.9
        } else {
            b.position[0] >= 0.9
        };
        if inside_a {
            out.push(a);
        }
        if inside_a != inside_b {
            out.push(intersection(a, b));
        }
    }
    out
}

pub(crate) fn build() -> Mesh {
    let columns = edge_columns();
    let mut mesh = Mesh {
        vertices: Vec::with_capacity(columns.len() * ROWS),
        indices: Vec::new(),
    };
    for edge in &columns {
        for row in 0..ROWS {
            let v = if row <= LOWER_STEPS {
                let t = row as f64 / LOWER_STEPS as f64;
                vertex(
                    Oklch::new(edge.lch.l * t, edge.lch.c * t, edge.lch.h),
                    edge.rgb.map(|v| v * t.powi(3)),
                )
            } else if row == ROWS - 1 {
                // At white hue is undefined. Extend this column's limiting
                // hue to C=0 rather than snapping every column to H=0.
                let previous = mesh.vertices.last().unwrap().position[0];
                Vertex {
                    position: [previous, -0.3, 0.65],
                    color: [1.; 3],
                }
            } else {
                let t = (row - LOWER_STEPS) as f64 / UPPER_STEPS as f64;
                let rgb = edge.rgb.map(|v| v + (1. - v) * t);
                vertex(Oklch::from_linear_rgb(rgb), rgb)
            };
            mesh.vertices.push(v);
        }
    }
    for column in 0..columns.len() - 1 {
        for row in 0..ROWS - 1 {
            let a = (column * ROWS + row) as u32;
            let b = a + ROWS as u32;
            let c = a + 1;
            let d = b + 1;
            mesh.triangle([a, b, c]);
            mesh.triangle([c, b, d]);
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::max_chroma;

    #[test]
    fn surface_tracks_rgb_creases_without_lightness_grid_ripple() {
        let columns = edge_columns();
        let mesh = build();
        let (mut old_loss, mut new_loss) = (0., 0.);
        for (column, edge) in columns.iter().enumerate() {
            let ridge = mesh.vertices[column * ROWS + LOWER_STEPS];
            let mesh_c = (ridge.position[1] as f64 + 0.3) * 0.4 / 1.2;
            let mesh_l = (ridge.position[2] as f64 + 0.65) / 1.3;
            assert!((mesh_c - edge.lch.c).abs() < 2e-8 && (mesh_l - edge.lch.l).abs() < 4e-8);
            assert!(edge.rgb.contains(&0.) && edge.rgb.contains(&1.));
            let old_peak = (0..=80)
                .map(|i| max_chroma(i as f64 / 80., edge.lch.h))
                .fold(0., f64::max);
            old_loss += (edge.lch.c - old_peak).max(0.);
            new_loss += (edge.lch.c - mesh_c).abs();
        }
        assert!(old_loss > 0.1 && new_loss < old_loss * 0.001);
        println!(
            "{} adaptive ridge columns: mean chroma loss {:.7} -> {:.9}",
            columns.len(),
            old_loss / columns.len() as f64,
            new_loss / columns.len() as f64
        );
        assert!(
            mesh.indices
                .iter()
                .all(|i| (*i as usize) < mesh.vertices.len())
        );
        for v in &mesh.vertices {
            assert!(v.position[0] >= -0.900001 && v.position[0] <= 0.900001);
        }
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let xs = triangle.map(|i| mesh.vertices[i as usize].position[0]);
            assert!(
                xs.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                    - xs.iter().copied().fold(f32::INFINITY, f32::min)
                    <= 0.9
            );
        }
        // In particular, keep pure blue itself, rather than selecting the
        // inner intersection of the non-convex near-blue constant-hue slice.
        let blue = columns.iter().position(|p| p.rgb == [0., 0., 1.]).unwrap();
        assert_eq!(mesh.vertices[blue * ROWS + LOWER_STEPS].color, [0., 0., 1.]);
    }
}
