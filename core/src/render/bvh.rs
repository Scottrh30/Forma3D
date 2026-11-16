//! A bounding volume hierarchy over the scene's triangles, for fast ray
//! hits: binned surface-area-heuristic build, flat node array.

use glam::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Tri {
    pub a: Vec3,
    pub e1: Vec3,
    pub e2: Vec3,
    /// Vertex normals.
    pub n: [Vec3; 3],
    /// Index into `RenderScene::objects`.
    pub object: u32,
}

impl Tri {
    pub fn new(p: [Vec3; 3], n: [Vec3; 3], object: u32) -> Tri {
        Tri { a: p[0], e1: p[1] - p[0], e2: p[2] - p[0], n, object }
    }

    fn bounds(&self) -> (Vec3, Vec3) {
        let (b, c) = (self.a + self.e1, self.a + self.e2);
        (self.a.min(b).min(c), self.a.max(b).max(c))
    }

    fn centre(&self) -> Vec3 {
        self.a + (self.e1 + self.e2) / 3.0
    }

    /// Möller–Trumbore: distance and barycentrics (u, v).
    #[inline]
    pub fn hit(&self, o: Vec3, d: Vec3, t_max: f32) -> Option<(f32, f32, f32)> {
        let p = d.cross(self.e2);
        let det = self.e1.dot(p);
        if det.abs() < 1e-12 {
            return None;
        }
        let inv = 1.0 / det;
        let s = o - self.a;
        let u = s.dot(p) * inv;
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let q = s.cross(self.e1);
        let v = d.dot(q) * inv;
        if v < 0.0 || u + v > 1.0 {
            return None;
        }
        let t = self.e2.dot(q) * inv;
        (t > 1e-4 && t < t_max).then_some((t, u, v))
    }

    pub fn normal_at(&self, u: f32, v: f32) -> Vec3 {
        let n = self.n[0] * (1.0 - u - v) + self.n[1] * u + self.n[2] * v;
        n.try_normalize().unwrap_or_else(|| self.face_normal())
    }

    pub fn face_normal(&self) -> Vec3 {
        self.e1.cross(self.e2).normalize_or(Vec3::Y)
    }
}

#[derive(Clone, Copy, Debug)]
struct Node {
    lo: Vec3,
    hi: Vec3,
    /// Leaf: first triangle; inner: index of the left child (right is next).
    start: u32,
    /// Leaf: triangle count; inner: 0.
    count: u32,
}

pub struct Bvh {
    nodes: Vec<Node>,
    pub tris: Vec<Tri>,
}

#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub t: f32,
    pub tri: u32,
    pub u: f32,
    pub v: f32,
}

#[inline]
fn slab(lo: Vec3, hi: Vec3, o: Vec3, inv: Vec3, t_max: f32) -> Option<f32> {
    let t0 = (lo - o) * inv;
    let t1 = (hi - o) * inv;
    let tmin = t0.min(t1).max_element().max(0.0);
    let tmax = t0.max(t1).min_element().min(t_max);
    (tmin <= tmax).then_some(tmin)
}

const LEAF: usize = 4;
const BINS: usize = 12;

impl Bvh {
    pub fn build(mut tris: Vec<Tri>) -> Bvh {
        let mut nodes = Vec::with_capacity(tris.len() * 2 / LEAF + 1);
        if tris.is_empty() {
            return Bvh { nodes, tris };
        }
        let mut centres: Vec<Vec3> = tris.iter().map(|t| t.centre()).collect();
        let mut bounds: Vec<(Vec3, Vec3)> = tris.iter().map(|t| t.bounds()).collect();
        let mut order: Vec<u32> = (0..tris.len() as u32).collect();
        nodes.push(Node { lo: Vec3::ZERO, hi: Vec3::ZERO, start: 0, count: 0 });
        let mut stack = vec![(0usize, 0usize, tris.len())];
        while let Some((node, start, end)) = stack.pop() {
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            let (mut clo, mut chi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for &i in &order[start..end] {
                let (a, b) = bounds[i as usize];
                lo = lo.min(a);
                hi = hi.max(b);
                let c = centres[i as usize];
                clo = clo.min(c);
                chi = chi.max(c);
            }
            nodes[node].lo = lo;
            nodes[node].hi = hi;
            let n = end - start;
            let extent = chi - clo;
            let axis = if extent.x >= extent.y && extent.x >= extent.z { 0 } else if extent.y >= extent.z { 1 } else { 2 };
            if n <= LEAF || extent[axis] < 1e-6 {
                nodes[node].start = start as u32;
                nodes[node].count = n as u32;
                continue;
            }
            // Binned SAH along the longest axis of the centres.
            let mut bin_count = [0usize; BINS];
            let mut bin_bounds = [(Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)); BINS];
            let scale = BINS as f32 / extent[axis];
            let bin_of = |c: Vec3| (((c[axis] - clo[axis]) * scale) as usize).min(BINS - 1);
            for &i in &order[start..end] {
                let b = bin_of(centres[i as usize]);
                bin_count[b] += 1;
                let (a, c) = bounds[i as usize];
                bin_bounds[b].0 = bin_bounds[b].0.min(a);
                bin_bounds[b].1 = bin_bounds[b].1.max(c);
            }
            let area = |lo: Vec3, hi: Vec3| {
                let d = (hi - lo).max(Vec3::ZERO);
                d.x * d.y + d.y * d.z + d.z * d.x
            };
            let mut best = (f32::MAX, BINS / 2);
            for split in 1..BINS {
                let (mut l, mut r) = ((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)));
                let (mut nl, mut nr) = (0, 0);
                for (b, bb) in bin_bounds.iter().enumerate() {
                    if bin_count[b] == 0 {
                        continue;
                    }
                    if b < split {
                        l = (l.0.min(bb.0), l.1.max(bb.1));
                        nl += bin_count[b];
                    } else {
                        r = (r.0.min(bb.0), r.1.max(bb.1));
                        nr += bin_count[b];
                    }
                }
                if nl == 0 || nr == 0 {
                    continue;
                }
                let cost = area(l.0, l.1) * nl as f32 + area(r.0, r.1) * nr as f32;
                if cost < best.0 {
                    best = (cost, split);
                }
            }
            let slice = &mut order[start..end];
            let mid = if best.0 < f32::MAX {
                let mut i = 0;
                for j in 0..slice.len() {
                    if bin_of(centres[slice[j] as usize]) < best.1 {
                        slice.swap(i, j);
                        i += 1;
                    }
                }
                i
            } else {
                0
            };
            let mid = if mid == 0 || mid == slice.len() {
                slice.sort_by(|a, b| centres[*a as usize][axis].total_cmp(&centres[*b as usize][axis]));
                slice.len() / 2
            } else {
                mid
            };
            let left = nodes.len();
            nodes.push(Node { lo: Vec3::ZERO, hi: Vec3::ZERO, start: 0, count: 0 });
            let right = nodes.len();
            nodes.push(Node { lo: Vec3::ZERO, hi: Vec3::ZERO, start: 0, count: 0 });
            nodes[node].start = left as u32;
            stack.push((right, start + mid, end));
            stack.push((left, start, start + mid));
        }
        // Reorder triangles to leaf order.
        let reordered: Vec<Tri> = order.iter().map(|&i| tris[i as usize]).collect();
        tris = reordered;
        centres.clear();
        bounds.clear();
        Bvh { nodes, tris }
    }

    /// The closest hit along a ray (`d` need not be unit length).
    pub fn closest(&self, o: Vec3, d: Vec3, t_max: f32) -> Option<Hit> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv = Vec3::ONE / d;
        let mut best: Option<Hit> = None;
        let mut limit = t_max;
        let mut stack = [0u32; 64];
        let mut sp = 1usize;
        while sp > 0 {
            sp -= 1;
            let node = self.nodes[stack[sp] as usize];
            if slab(node.lo, node.hi, o, inv, limit).is_none() {
                continue;
            }
            if node.count > 0 {
                for i in node.start..node.start + node.count {
                    if let Some((t, u, v)) = self.tris[i as usize].hit(o, d, limit) {
                        limit = t;
                        best = Some(Hit { t, tri: i, u, v });
                    }
                }
            } else if sp + 2 <= stack.len() {
                let l = node.start;
                let r = node.start + 1;
                // Visit the nearer child first.
                let ln = self.nodes[l as usize];
                let rn = self.nodes[r as usize];
                let tl = slab(ln.lo, ln.hi, o, inv, limit);
                let tr = slab(rn.lo, rn.hi, o, inv, limit);
                match (tl, tr) {
                    (Some(a), Some(b)) => {
                        if a <= b {
                            stack[sp] = r;
                            stack[sp + 1] = l;
                        } else {
                            stack[sp] = l;
                            stack[sp + 1] = r;
                        }
                        sp += 2;
                    }
                    (Some(_), None) => {
                        stack[sp] = l;
                        sp += 1;
                    }
                    (None, Some(_)) => {
                        stack[sp] = r;
                        sp += 1;
                    }
                    (None, None) => {}
                }
            }
        }
        best
    }

    /// Whether anything blocks the ray before `t_max` (shadow rays).
    pub fn occluded(&self, o: Vec3, d: Vec3, t_max: f32) -> bool {
        self.first_blocker(o, d, t_max, |_| true).is_some()
    }

    /// The first triangle `accept` lets block the ray (shadow rays that
    /// skip objects a light doesn't affect).
    pub fn first_blocker(&self, o: Vec3, d: Vec3, t_max: f32, accept: impl Fn(u32) -> bool) -> Option<u32> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv = Vec3::ONE / d;
        let mut stack = [0u32; 64];
        let mut sp = 1usize;
        while sp > 0 {
            sp -= 1;
            let idx = stack[sp];
            let node = self.nodes[idx as usize];
            if slab(node.lo, node.hi, o, inv, t_max).is_none() {
                continue;
            }
            if node.count > 0 {
                for i in node.start..node.start + node.count {
                    let tri = &self.tris[i as usize];
                    if tri.hit(o, d, t_max).is_some() && accept(tri.object) {
                        return Some(i);
                    }
                }
            } else if sp + 2 <= stack.len() {
                stack[sp] = node.start;
                stack[sp + 1] = node.start + 1;
                sp += 2;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(z: f32, object: u32) -> Vec<Tri> {
        let n = [Vec3::NEG_Z; 3];
        let p = |x: f32, y: f32| Vec3::new(x, y, z);
        vec![Tri::new([p(-1.0, -1.0), p(1.0, -1.0), p(1.0, 1.0)], n, object), Tri::new([p(-1.0, -1.0), p(1.0, 1.0), p(-1.0, 1.0)], n, object)]
    }

    #[test]
    fn finds_the_nearest_of_many() {
        let mut tris = Vec::new();
        for k in 0..200 {
            tris.extend(quad(10.0 + k as f32, k));
        }
        let bvh = Bvh::build(tris.clone());
        let hit = bvh.closest(Vec3::new(0.1, 0.2, 0.0), Vec3::Z, f32::MAX).unwrap();
        assert!((hit.t - 10.0).abs() < 1e-4);
        assert_eq!(bvh.tris[hit.tri as usize].object, 0);
        assert!(bvh.closest(Vec3::new(5.0, 0.0, 0.0), Vec3::Z, f32::MAX).is_none());
        assert!(bvh.occluded(Vec3::ZERO, Vec3::Z, 11.0));
        assert!(!bvh.occluded(Vec3::ZERO, Vec3::Z, 9.0));
        assert!(bvh.first_blocker(Vec3::ZERO, Vec3::Z, 100.0, |o| o >= 5).is_some_and(|i| bvh.tris[i as usize].object >= 5));
        assert!(bvh.first_blocker(Vec3::ZERO, Vec3::Z, 100.0, |_| false).is_none());
        // Brute force agrees on random rays.
        let mut seed = 7u32;
        let mut rnd = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
        };
        for _ in 0..200 {
            let o = Vec3::new(rnd() * 3.0, rnd() * 3.0, rnd() * 300.0);
            let d = Vec3::new(rnd(), rnd(), rnd()).normalize_or(Vec3::Z);
            let brute = tris.iter().filter_map(|t| t.hit(o, d, f32::MAX).map(|h| h.0)).fold(f32::MAX, f32::min);
            let fast = bvh.closest(o, d, f32::MAX).map_or(f32::MAX, |h| h.t);
            assert!((brute - fast).abs() < 1e-3, "{brute} vs {fast}");
        }
    }
}
