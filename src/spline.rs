//! Spline objects: open or closed curves through control points, drawn with
//! the Spline Pen.
//!
//! Each point has an incoming and an outgoing tangent (offsets from the
//! point). With `Bezier` interpolation a segment is the cubic Bézier
//! `p0, p0 + out0, p1 + in1, p1`; zero tangents give a sharp corner, as in
//! Cinema 4D. `Linear` ignores the tangents.

use crate::mesh::{Mesh, Vertex};
use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplineType {
    Linear,
    Bezier,
}

impl SplineType {
    pub const ALL: [SplineType; 2] = [SplineType::Linear, SplineType::Bezier];

    pub fn label(self) -> &'static str {
        match self {
            SplineType::Linear => "Linear",
            SplineType::Bezier => "Bezier",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SplinePoint {
    pub pos: Vec3,
    /// Incoming tangent, relative to `pos`.
    pub tan_in: Vec3,
    /// Outgoing tangent, relative to `pos`.
    pub tan_out: Vec3,
}

impl SplinePoint {
    pub fn corner(pos: Vec3) -> Self {
        SplinePoint { pos, tan_in: Vec3::ZERO, tan_out: Vec3::ZERO }
    }

    /// A smooth point: the incoming tangent mirrors the outgoing one.
    pub fn smooth(pos: Vec3, tan_out: Vec3) -> Self {
        SplinePoint { pos, tan_in: -tan_out, tan_out }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spline {
    pub points: Vec<SplinePoint>,
    pub closed: bool,
    pub kind: SplineType,
    /// Line steps drawn per curved segment.
    pub steps: u32,
}

impl Default for Spline {
    fn default() -> Self {
        Spline { points: Vec::new(), closed: false, kind: SplineType::Bezier, steps: 12 }
    }
}

impl Spline {
    /// Number of segments (a closed spline joins the last point to the first).
    pub fn segment_count(&self) -> usize {
        match self.points.len() {
            0 | 1 => 0,
            n if self.closed => n,
            n => n - 1,
        }
    }

    /// The four Bézier control points of segment `i`.
    pub fn segment(&self, i: usize) -> [Vec3; 4] {
        let n = self.points.len();
        let (a, b) = (self.points[i % n], self.points[(i + 1) % n]);
        match self.kind {
            SplineType::Linear => [a.pos, a.pos.lerp(b.pos, 1.0 / 3.0), a.pos.lerp(b.pos, 2.0 / 3.0), b.pos],
            SplineType::Bezier => [a.pos, a.pos + a.tan_out, b.pos + b.tan_in, b.pos],
        }
    }

    fn segment_is_straight(&self, i: usize) -> bool {
        if self.kind == SplineType::Linear {
            return true;
        }
        let n = self.points.len();
        self.points[i % n].tan_out == Vec3::ZERO && self.points[(i + 1) % n].tan_in == Vec3::ZERO
    }

    /// Point on segment `i` at `t` in 0..1.
    pub fn eval(&self, i: usize, t: f32) -> Vec3 {
        let [p0, p1, p2, p3] = self.segment(i);
        let u = 1.0 - t;
        p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t)
    }

    /// The curve as a polyline (closed splines repeat the first point at the end).
    pub fn polyline(&self) -> Vec<Vec3> {
        let mut out = Vec::new();
        if self.points.is_empty() {
            return out;
        }
        out.push(self.points[0].pos);
        let steps = self.steps.clamp(1, 256) as usize;
        for i in 0..self.segment_count() {
            let k = if self.segment_is_straight(i) { 1 } else { steps };
            for s in 1..=k {
                out.push(self.eval(i, s as f32 / k as f32));
            }
        }
        out
    }

    /// The curve as a line mesh (no triangles) for drawing.
    pub fn line_mesh(&self) -> Mesh {
        let line = self.polyline();
        let vertices: Vec<Vertex> = line.iter().map(|p| Vertex { pos: p.to_array(), normal: [0.0, 1.0, 0.0] }).collect();
        let mut edges = Vec::new();
        for i in 1..vertices.len() {
            edges.extend_from_slice(&[i as u32 - 1, i as u32]);
        }
        Mesh::from_parts(vertices, Vec::new(), edges)
    }

    /// Splits segment `i` at `t` without changing the curve's shape (de
    /// Casteljau), inserting a point. Returns the new point's index.
    pub fn insert(&mut self, i: usize, t: f32) -> usize {
        let n = self.points.len();
        let [p0, p1, p2, p3] = self.segment(i);
        let t = t.clamp(0.0, 1.0);
        let (a, b, c) = (p0.lerp(p1, t), p1.lerp(p2, t), p2.lerp(p3, t));
        let (d, e) = (a.lerp(b, t), b.lerp(c, t));
        let mid = d.lerp(e, t);
        let new = if self.kind == SplineType::Bezier {
            self.points[i % n].tan_out = a - p0;
            self.points[(i + 1) % n].tan_in = c - p3;
            SplinePoint { pos: mid, tan_in: d - mid, tan_out: e - mid }
        } else {
            SplinePoint::corner(mid)
        };
        let at = i + 1;
        if at >= n {
            self.points.push(new);
            n
        } else {
            self.points.insert(at, new);
            at
        }
    }

    /// The nearest segment and parameter to `target`, measured by `dist`
    /// (for example screen-space distance). Returns (segment, t, distance).
    pub fn nearest(&self, mut dist: impl FnMut(Vec3) -> Option<f32>) -> Option<(usize, f32, f32)> {
        let mut best: Option<(usize, f32, f32)> = None;
        const SAMPLES: usize = 32;
        for i in 0..self.segment_count() {
            for s in 0..=SAMPLES {
                let t = s as f32 / SAMPLES as f32;
                if let Some(d) = dist(self.eval(i, t))
                    && best.is_none_or(|b| d < b.2)
                {
                    best = Some((i, t, d));
                }
            }
        }
        best
    }

    /// Keeps the numbers usable (from a hand-edited or damaged file).
    pub fn sanitized(&self) -> Spline {
        let finite = |v: Vec3| if v.is_finite() { v } else { Vec3::ZERO };
        Spline {
            points: self
                .points
                .iter()
                .map(|p| SplinePoint { pos: finite(p.pos), tan_in: finite(p.tan_in), tan_out: finite(p.tan_out) })
                .collect(),
            closed: self.closed,
            kind: self.kind,
            steps: self.steps.clamp(1, 256),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(closed: bool) -> Spline {
        Spline {
            points: [Vec3::ZERO, Vec3::X * 100.0, Vec3::new(100.0, 0.0, 100.0), Vec3::Z * 100.0]
                .into_iter()
                .map(SplinePoint::corner)
                .collect(),
            closed,
            kind: SplineType::Bezier,
            steps: 8,
        }
    }

    #[test]
    fn corners_draw_straight_and_closing_adds_a_segment() {
        let open = square(false);
        assert_eq!(open.segment_count(), 3);
        assert_eq!(open.polyline().len(), 4, "straight segments need no extra steps");
        let closed = square(true);
        assert_eq!(closed.segment_count(), 4);
        let line = closed.polyline();
        assert_eq!(line.first(), line.last());
        assert_eq!(closed.line_mesh().edges.len(), 8);
        assert!(closed.line_mesh().indices.is_empty(), "a spline has no faces");
    }

    #[test]
    fn tangents_curve_the_segment_through_its_ends() {
        let s = Spline {
            points: vec![SplinePoint::smooth(Vec3::ZERO, Vec3::Z * 50.0), SplinePoint::smooth(Vec3::X * 100.0, Vec3::Z * -50.0)],
            ..Default::default()
        };
        assert_eq!(s.eval(0, 0.0), Vec3::ZERO);
        assert!((s.eval(0, 1.0) - Vec3::X * 100.0).length() < 1e-4);
        // The middle bulges towards +Z.
        assert!(s.eval(0, 0.5).z > 30.0);
        assert_eq!(s.polyline().len(), 1 + 12);
        // Linear ignores the tangents.
        let linear = Spline { kind: SplineType::Linear, ..s.clone() };
        assert!(linear.eval(0, 0.5).z.abs() < 1e-4);
    }

    #[test]
    fn inserting_keeps_the_shape() {
        let mut s = Spline {
            points: vec![SplinePoint::smooth(Vec3::ZERO, Vec3::Z * 60.0), SplinePoint::smooth(Vec3::X * 100.0, Vec3::Z * -40.0)],
            ..Default::default()
        };
        let before: Vec<Vec3> = (0..=10).map(|k| s.eval(0, k as f32 / 10.0)).collect();
        let at = s.insert(0, 0.4);
        assert_eq!((at, s.points.len()), (1, 3));
        // The old curve's points still lie on the new two-segment curve.
        for p in before {
            let (_, _, d) = s.nearest(|q| Some(q.distance(p))).unwrap();
            assert!(d < 1.0, "moved by {d}");
        }
        // On a closed spline, splitting the closing segment appends a point.
        let mut c = square(true);
        assert_eq!(c.insert(3, 0.5), 4);
        assert!((c.points[4].pos - Vec3::Z * 50.0).length() < 1e-3);
    }

    #[test]
    fn sanitizing_removes_bad_numbers() {
        let mut s = square(false);
        s.points[1].pos = Vec3::NAN;
        s.steps = 0;
        let clean = s.sanitized();
        assert!(clean.polyline().iter().all(|p| p.is_finite()));
        assert_eq!(clean.steps, 1);
        assert!(Spline::default().polyline().is_empty());
    }
}
