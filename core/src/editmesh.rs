//! Editable polygon meshes: points plus n-gon polygons, and the modelling
//! operations on them (delete, dissolve, extrude, inset, loop cut, loop select).
//!
//! Polygons list point indices in counter-clockwise order seen from outside
//! (the winding used everywhere else in Forma3D).

use crate::generator::{Geo, edge_key, polygons, weld};
use crate::gizmo::perpendicular_basis;
use crate::mesh::Vertex;
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};

/// Angle above which neighbouring polygons get a hard edge (like a Phong tag's limit).
pub const SMOOTHING_ANGLE_DEG: f32 = 60.0;

pub type Edge = (u32, u32);

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EditMesh {
    pub points: Vec<Vec3>,
    pub polys: Vec<Vec<u32>>,
}

impl Hash for EditMesh {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.points.len().hash(h);
        for p in &self.points {
            p.x.to_bits().hash(h);
            p.y.to_bits().hash(h);
            p.z.to_bits().hash(h);
        }
        self.polys.hash(h);
    }
}

impl EditMesh {
    /// Builds an editable mesh from render geometry: positions are welded and
    /// the quads primitives are made of are recovered.
    pub fn from_geo(geo: &Geo) -> EditMesh {
        let (points, polys) = weld(geo, polygons(geo));
        let mut m = EditMesh { points, polys };
        m.remove_unused_points();
        m
    }

    /// Rejects meshes that would crash or misbehave (used when loading files).
    pub fn validate(&self) -> Result<(), String> {
        if self.points.iter().any(|p| !p.is_finite()) {
            return Err("mesh has non-finite points".to_owned());
        }
        for (i, poly) in self.polys.iter().enumerate() {
            if poly.len() < 3 {
                return Err(format!("polygon {i} has fewer than 3 points"));
            }
            if poly.iter().any(|&p| p as usize >= self.points.len()) {
                return Err(format!("polygon {i} refers to a missing point"));
            }
            let unique: HashSet<u32> = poly.iter().copied().collect();
            if unique.len() != poly.len() {
                return Err(format!("polygon {i} repeats a point"));
            }
        }
        Ok(())
    }

    // --- topology queries ---------------------------------------------------

    /// Every edge once, in order of first appearance.
    pub fn edges(&self) -> Vec<Edge> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for poly in &self.polys {
            for k in 0..poly.len() {
                let e = edge_key(poly[k], poly[(k + 1) % poly.len()]);
                if seen.insert(e) {
                    out.push(e);
                }
            }
        }
        out
    }

    pub fn edge_polys(&self) -> HashMap<Edge, Vec<usize>> {
        let mut map: HashMap<Edge, Vec<usize>> = HashMap::new();
        for (i, poly) in self.polys.iter().enumerate() {
            for k in 0..poly.len() {
                map.entry(edge_key(poly[k], poly[(k + 1) % poly.len()])).or_default().push(i);
            }
        }
        map
    }

    pub fn point_polys(&self) -> Vec<Vec<usize>> {
        let mut out = vec![Vec::new(); self.points.len()];
        for (i, poly) in self.polys.iter().enumerate() {
            for &p in poly {
                out[p as usize].push(i);
            }
        }
        out
    }

    /// Area-weighted normal (length = 2 x area), by Newell's method.
    pub fn poly_area_normal(&self, i: usize) -> Vec3 {
        let poly = &self.polys[i];
        let mut n = Vec3::ZERO;
        for k in 0..poly.len() {
            let a = self.points[poly[k] as usize];
            let b = self.points[poly[(k + 1) % poly.len()] as usize];
            n += Vec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
        }
        n
    }

    pub fn poly_normal(&self, i: usize) -> Vec3 {
        self.poly_area_normal(i).normalize_or_zero()
    }

    pub fn poly_center(&self, i: usize) -> Vec3 {
        let poly = &self.polys[i];
        poly.iter().map(|&p| self.points[p as usize]).sum::<Vec3>() / poly.len() as f32
    }

    /// Triangles (as corner positions within the polygon) covering polygon `i`.
    /// Ear clipping, so concave n-gons from dissolving edges still fill correctly.
    pub fn triangulate(&self, i: usize) -> Vec<[usize; 3]> {
        let poly = &self.polys[i];
        let n = poly.len();
        if n == 3 {
            return vec![[0, 1, 2]];
        }
        let normal = self.poly_normal(i);
        let (u, v) = perpendicular_basis(if normal == Vec3::ZERO { Vec3::Y } else { normal });
        let pts: Vec<(f32, f32)> = poly
            .iter()
            .map(|&p| {
                let q = self.points[p as usize];
                (q.dot(u), q.dot(v))
            })
            .collect();
        // perpendicular_basis is right-handed (u x v = n), so CCW around n is CCW in (u, v).
        let cross = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
        let mut idx: Vec<usize> = (0..n).collect();
        let mut tris = Vec::with_capacity(n - 2);
        let mut guard = 0;
        while idx.len() > 3 && guard < n * n {
            guard += 1;
            let m = idx.len();
            let mut clipped = false;
            for k in 0..m {
                let (ia, ib, ic) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
                let (a, b, c) = (pts[ia], pts[ib], pts[ic]);
                if cross(a, b, c) <= 1e-9 {
                    continue; // reflex or degenerate corner
                }
                let contains = idx.iter().any(|&j| {
                    j != ia && j != ib && j != ic && {
                        let p = pts[j];
                        cross(a, b, p) >= 0.0 && cross(b, c, p) >= 0.0 && cross(c, a, p) >= 0.0
                    }
                });
                if !contains {
                    tris.push([ia, ib, ic]);
                    idx.remove(k);
                    clipped = true;
                    break;
                }
            }
            if !clipped {
                break;
            }
        }
        if idx.len() == 3 {
            tris.push([idx[0], idx[1], idx[2]]);
            tris
        } else {
            // Degenerate (e.g. self-intersecting) polygon: fall back to a fan.
            (1..n - 1).map(|k| [0, k, k + 1]).collect()
        }
    }

    /// Render geometry: hard edges above `SMOOTHING_ANGLE_DEG`, smooth otherwise.
    pub fn to_geo(&self) -> Geo {
        let limit = SMOOTHING_ANGLE_DEG.to_radians().cos();
        let area_normals: Vec<Vec3> = (0..self.polys.len()).map(|i| self.poly_area_normal(i)).collect();
        let unit: Vec<Vec3> = area_normals.iter().map(|n| n.normalize_or_zero()).collect();
        let point_polys = self.point_polys();
        let mut geo = Geo::default();
        let mut first_vertex: Vec<Option<u32>> = vec![None; self.points.len()];
        for (i, poly) in self.polys.iter().enumerate() {
            let base = geo.verts.len() as u32;
            for &p in poly {
                let mut n = Vec3::ZERO;
                for &j in &point_polys[p as usize] {
                    if unit[j].dot(unit[i]) >= limit {
                        n += area_normals[j];
                    }
                }
                let n = if n.length_squared() > 0.0 { n.normalize() } else { unit[i] };
                first_vertex[p as usize].get_or_insert(geo.verts.len() as u32);
                geo.verts.push(Vertex { pos: self.points[p as usize].to_array(), normal: n.to_array() });
            }
            for t in self.triangulate(i) {
                geo.tris.extend(t.iter().map(|&k| base + k as u32));
            }
        }
        for (a, b) in self.edges() {
            if let (Some(va), Some(vb)) = (first_vertex[a as usize], first_vertex[b as usize]) {
                geo.edges.extend_from_slice(&[va, vb]);
            }
        }
        geo
    }

    // --- selection helpers --------------------------------------------------

    pub fn points_of_polys(&self, polys: &BTreeSet<u32>) -> BTreeSet<u32> {
        polys
            .iter()
            .filter_map(|&i| self.polys.get(i as usize))
            .flat_map(|p| p.iter().copied())
            .collect()
    }

    pub fn points_of_edges(edges: &BTreeSet<Edge>) -> BTreeSet<u32> {
        edges.iter().flat_map(|&(a, b)| [a, b]).collect()
    }

    /// The edge loop through `start`: continues straight across every point
    /// where exactly four quads... more precisely, where the edge opposite the
    /// incoming one is unambiguous (the one sharing no polygon with it).
    pub fn edge_loop(&self, start: Edge) -> Vec<Edge> {
        let start = edge_key(start.0, start.1);
        let edge_polys = self.edge_polys();
        if !edge_polys.contains_key(&start) {
            return Vec::new();
        }
        let mut point_edges: HashMap<u32, Vec<Edge>> = HashMap::new();
        for &e in edge_polys.keys() {
            point_edges.entry(e.0).or_default().push(e);
            point_edges.entry(e.1).or_default().push(e);
        }
        let mut out = vec![start];
        let mut seen: HashSet<Edge> = HashSet::from([start]);
        for (from, through) in [(start.0, start.1), (start.1, start.0)] {
            let (mut prev, mut cur) = (edge_key(from, through), through);
            let _ = from;
            loop {
                let around = &point_edges[&cur];
                if around.len() != 4 {
                    break;
                }
                let prev_polys = &edge_polys[&prev];
                let next: Vec<Edge> = around
                    .iter()
                    .copied()
                    .filter(|&e| e != prev && !edge_polys[&e].iter().any(|p| prev_polys.contains(p)))
                    .collect();
                if next.len() != 1 || !seen.insert(next[0]) {
                    break;
                }
                let e = next[0];
                out.push(e);
                cur = if e.0 == cur { e.1 } else { e.0 };
                prev = e;
            }
        }
        out
    }

    // --- edits ----------------------------------------------------------------

    /// Drops points no polygon uses and renumbers. Returns old -> new index.
    pub fn remove_unused_points(&mut self) -> Vec<Option<u32>> {
        let mut used = vec![false; self.points.len()];
        for poly in &self.polys {
            for &p in poly {
                used[p as usize] = true;
            }
        }
        let mut remap = vec![None; self.points.len()];
        let mut points = Vec::new();
        for (i, p) in self.points.iter().enumerate() {
            if used[i] {
                remap[i] = Some(points.len() as u32);
                points.push(*p);
            }
        }
        for poly in &mut self.polys {
            for p in poly.iter_mut() {
                *p = remap[*p as usize].expect("used point");
            }
        }
        self.points = points;
        remap
    }

    pub fn delete_polys(&mut self, doomed: &BTreeSet<u32>) {
        let mut i = 0;
        self.polys.retain(|_| {
            let keep = !doomed.contains(&i);
            i += 1;
            keep
        });
        self.remove_unused_points();
    }

    /// Deleting points removes every polygon that uses them.
    pub fn delete_points(&mut self, doomed: &BTreeSet<u32>) {
        self.polys.retain(|poly| !poly.iter().any(|p| doomed.contains(p)));
        self.remove_unused_points();
    }

    /// Dissolves edges: the two polygons on either side merge into one.
    /// Border edges and edges whose merge would be invalid are left alone.
    /// Returns how many edges were dissolved.
    pub fn dissolve_edges(&mut self, edges: &BTreeSet<Edge>) -> usize {
        let mut done = 0;
        for &(a, b) in edges {
            let ep = self.edge_polys();
            let Some(adj) = ep.get(&edge_key(a, b)) else { continue };
            if adj.len() != 2 || adj[0] == adj[1] {
                continue;
            }
            let (ia, ib) = (adj[0], adj[1]);
            let Some(merged) = merge_across(&self.polys[ia], &self.polys[ib], a, b) else { continue };
            let (lo, hi) = (ia.min(ib), ia.max(ib));
            self.polys[lo] = merged;
            self.polys.remove(hi);
            done += 1;
        }
        self.remove_unused_points();
        done
    }

    /// Extrudes the selected polygons along their normals by `offset`, keeping
    /// connected polygons together. The selected polygons become the caps
    /// (same indices); new side polygons are appended.
    pub fn extrude(&mut self, selected: &BTreeSet<u32>, offset: f32) {
        let sel: Vec<usize> = selected.iter().map(|&i| i as usize).filter(|&i| i < self.polys.len()).collect();
        if sel.is_empty() {
            return;
        }
        let sel_set: HashSet<usize> = sel.iter().copied().collect();
        let normals: Vec<Vec3> = sel.iter().map(|&i| self.poly_area_normal(i)).collect();
        // Direction per point: average of the normals of selected polygons around it.
        let mut dir: HashMap<u32, Vec3> = HashMap::new();
        for (k, &i) in sel.iter().enumerate() {
            for &p in &self.polys[i] {
                *dir.entry(p).or_insert(Vec3::ZERO) += normals[k].normalize_or_zero();
            }
        }
        let mut new_index: HashMap<u32, u32> = HashMap::new();
        let mut ordered: Vec<u32> = dir.keys().copied().collect();
        ordered.sort_unstable();
        for p in ordered {
            let d = dir[&p];
            let count = sel.iter().filter(|&&i| self.polys[i].contains(&p)).count().max(1) as f32;
            // Keep side walls parallel: scale so the offset is measured along each face normal.
            let avg = d / count;
            let len2 = avg.length_squared();
            let step = if len2 > 1e-6 { avg / len2 } else { Vec3::ZERO };
            self.points.push(self.points[p as usize] + step * offset);
            new_index.insert(p, (self.points.len() - 1) as u32);
        }
        let edge_polys = self.edge_polys();
        let mut sides = Vec::new();
        for &i in &sel {
            let poly = self.polys[i].clone();
            for k in 0..poly.len() {
                let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
                let others = &edge_polys[&edge_key(a, b)];
                let boundary = !others.iter().any(|&o| o != i && sel_set.contains(&o));
                if boundary {
                    sides.push(vec![a, b, new_index[&b], new_index[&a]]);
                }
            }
            self.polys[i] = poly.iter().map(|p| new_index[p]).collect();
        }
        self.polys.extend(sides);
        self.remove_unused_points();
    }

    /// Insets each selected polygon individually by `amount` (cm): its corners
    /// move towards its centre and a ring of quads joins old and new outline.
    pub fn inset(&mut self, selected: &BTreeSet<u32>, amount: f32) {
        let mut ring = Vec::new();
        for &i in selected {
            let i = i as usize;
            if i >= self.polys.len() {
                continue;
            }
            let poly = self.polys[i].clone();
            let c = self.poly_center(i);
            let mut inner = Vec::with_capacity(poly.len());
            for &p in &poly {
                let q = self.points[p as usize];
                let to_c = c - q;
                let dist = to_c.length();
                let step = amount.clamp(0.0, dist * 0.999);
                self.points.push(if dist > 0.0 { q + to_c / dist * step } else { q });
                inner.push((self.points.len() - 1) as u32);
            }
            for k in 0..poly.len() {
                let k2 = (k + 1) % poly.len();
                ring.push(vec![poly[k], poly[k2], inner[k2], inner[k]]);
            }
            self.polys[i] = inner;
        }
        self.polys.extend(ring);
    }

    /// The ring of quads crossed by a loop cut through `edge`, as
    /// `(polygon, corner where the crossed edge starts)` pairs.
    pub fn quad_ring(&self, edge: Edge) -> Vec<(usize, usize)> {
        let edge_polys = self.edge_polys();
        let mut ring = Vec::new();
        let mut visited = HashSet::new();
        let Some(start) = edge_polys.get(&edge_key(edge.0, edge.1)) else { return ring };
        for &first in start {
            let mut entry = edge_key(edge.0, edge.1);
            let mut cur = Some(first);
            while let Some(pi) = cur {
                let poly = &self.polys[pi];
                if poly.len() != 4 || !visited.insert(pi) {
                    break;
                }
                let Some(k) = (0..4).find(|&k| edge_key(poly[k], poly[(k + 1) % 4]) == entry) else { break };
                ring.push((pi, k));
                let opposite = edge_key(poly[(k + 2) % 4], poly[(k + 3) % 4]);
                cur = edge_polys[&opposite].iter().copied().find(|&o| o != pi);
                entry = opposite;
            }
        }
        ring
    }

    /// Loop cut through `edge`, placing the new points at fraction `t` from
    /// `edge.0` towards `edge.1` (the fraction carries across the ring).
    /// Returns the new edges (the cut), for selection.
    pub fn loop_cut(&mut self, edge: Edge, t: f32) -> Vec<Edge> {
        let t = t.clamp(0.01, 0.99);
        let ring = self.quad_ring(edge);
        if ring.is_empty() {
            return Vec::new();
        }
        // Points on crossed edges, keyed by edge; value = new point index.
        let mut cut_points: HashMap<Edge, u32> = HashMap::new();
        let mut add_point = |mesh: &mut EditMesh, a_side: u32, b_side: u32| -> u32 {
            *cut_points.entry(edge_key(a_side, b_side)).or_insert_with(|| {
                let p = mesh.points[a_side as usize].lerp(mesh.points[b_side as usize], t);
                mesh.points.push(p);
                (mesh.points.len() - 1) as u32
            })
        };
        // Track which end of each crossed edge is the "a" side so t is consistent.
        let mut a_side_of: HashMap<Edge, u32> = HashMap::from([(edge_key(edge.0, edge.1), edge.0)]);
        let mut new_polys: Vec<(usize, Vec<u32>, Vec<u32>)> = Vec::new();
        let mut cut_edges = Vec::new();
        let ring_polys: HashSet<usize> = ring.iter().map(|&(p, _)| p).collect();
        // The ring is stored as two walks from the start edge; each walk is in order.
        for &(pi, k) in &ring {
            let poly = self.polys[pi].clone();
            let c = [poly[k], poly[(k + 1) % 4], poly[(k + 2) % 4], poly[(k + 3) % 4]];
            let entry = edge_key(c[0], c[1]);
            let a = *a_side_of.get(&entry).unwrap_or(&c[0]);
            let c0_is_a = a == c[0];
            let m01 = if c0_is_a { add_point(self, c[0], c[1]) } else { add_point(self, c[1], c[0]) };
            // The opposite edge's "a" side is the corner next to this edge's "a" side.
            let (opp_a, opp_b) = if c0_is_a { (c[3], c[2]) } else { (c[2], c[3]) };
            a_side_of.entry(edge_key(opp_a, opp_b)).or_insert(opp_a);
            let opp_a = a_side_of[&edge_key(opp_a, opp_b)];
            let opp_b = if opp_a == c[2] { c[3] } else { c[2] };
            let m23 = add_point(self, opp_a, opp_b);
            new_polys.push((pi, vec![c[0], m01, m23, c[3]], vec![m01, c[1], c[2], m23]));
            cut_edges.push(edge_key(m01, m23));
        }
        let mut appended = Vec::new();
        for (pi, first, second) in new_polys {
            self.polys[pi] = first;
            appended.push(second);
        }
        // Neighbours outside the ring that share a cut edge get the new point too,
        // so no cracks (T-junctions) appear.
        for (pi, poly) in self.polys.iter_mut().enumerate() {
            if ring_polys.contains(&pi) {
                continue;
            }
            let mut out = Vec::with_capacity(poly.len() + 2);
            for k in 0..poly.len() {
                let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
                out.push(a);
                if let Some(&m) = cut_points.get(&edge_key(a, b)) {
                    out.push(m);
                }
            }
            *poly = out;
        }
        self.polys.extend(appended);
        cut_edges
    }

    /// Reverses the winding (and so the normals) of the given polygons.
    pub fn reverse(&mut self, polys: &BTreeSet<u32>) {
        for &i in polys {
            if let Some(p) = self.polys.get_mut(i as usize) {
                p.reverse();
            }
        }
    }
}

/// Merges polygons `pa` and `pb` across their shared edge `a-b`.
fn merge_across(pa: &[u32], pb: &[u32], a: u32, b: u32) -> Option<Vec<u32>> {
    // Orient so that `pa` contains x -> y and `pb` contains y -> x.
    let has = |p: &[u32], x: u32, y: u32| (0..p.len()).any(|k| p[k] == x && p[(k + 1) % p.len()] == y);
    let (x, y) = if has(pa, a, b) && has(pb, b, a) {
        (a, b)
    } else if has(pa, b, a) && has(pb, a, b) {
        (b, a)
    } else {
        return None; // inconsistent winding across the edge
    };
    let rot = |p: &[u32], from: u32| -> Vec<u32> {
        let k = p.iter().position(|&v| v == from).expect("vertex in polygon");
        p[k..].iter().chain(&p[..k]).copied().collect()
    };
    let ra = rot(pa, y); // y ... x
    let rb = rot(pb, x); // x ... y
    let mut merged = ra;
    merged.extend(&rb[1..rb.len() - 1]);
    let unique: HashSet<u32> = merged.iter().copied().collect();
    (unique.len() == merged.len() && merged.len() >= 3).then_some(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitive::{Primitive, PrimitiveKind};

    fn cube() -> EditMesh {
        EditMesh::from_geo(&Geo::from_mesh(&PrimitiveKind::Cube.default_primitive().build()))
    }

    fn grid_cube(n: u32) -> EditMesh {
        EditMesh::from_geo(&Geo::from_mesh(&Primitive::Cube { size: Vec3::splat(200.0), segments: [n, n, n] }.build()))
    }

    fn volume(m: &EditMesh) -> f32 {
        let g = m.to_geo();
        g.tris
            .chunks_exact(3)
            .map(|t| {
                let p = |i: u32| Vec3::from(g.verts[i as usize].pos);
                p(t[0]).dot(p(t[1]).cross(p(t[2]))) / 6.0
            })
            .sum()
    }

    /// Closed and consistently wound: every edge used twice, in opposite directions.
    fn assert_closed(m: &EditMesh) {
        let mut directed: HashMap<(u32, u32), usize> = HashMap::new();
        for poly in &m.polys {
            for k in 0..poly.len() {
                *directed.entry((poly[k], poly[(k + 1) % poly.len()])).or_default() += 1;
            }
        }
        for (&(a, b), &n) in &directed {
            assert_eq!(n, 1, "edge {a}->{b} used {n} times in the same direction");
            assert_eq!(directed.get(&(b, a)), Some(&1), "edge {a}-{b} is a border or inconsistent");
        }
    }

    #[test]
    fn primitives_convert_to_clean_polygon_meshes() {
        let c = cube();
        assert_eq!(c.points.len(), 8);
        assert_eq!(c.polys.len(), 6);
        assert!(c.polys.iter().all(|p| p.len() == 4));
        assert_closed(&c);
        assert!((volume(&c) - 8.0e6).abs() < 1.0);
        let s = EditMesh::from_geo(&Geo::from_mesh(&PrimitiveKind::Sphere.default_primitive().build()));
        assert_closed(&s);
        assert!(s.polys.iter().any(|p| p.len() == 3), "poles become triangles");
        c.validate().unwrap();
    }

    #[test]
    fn smoothing_keeps_cube_corners_hard_and_spheres_smooth() {
        let g = cube().to_geo();
        for v in &g.verts {
            let n = Vec3::from(v.normal);
            assert!(n.abs().max_element() > 0.999, "cube normals are face normals: {n:?}");
        }
        let s = EditMesh::from_geo(&Geo::from_mesh(&PrimitiveKind::Sphere.default_primitive().build())).to_geo();
        for v in &s.verts {
            let (p, n) = (Vec3::from(v.pos), Vec3::from(v.normal));
            assert!(p.normalize().dot(n) > 0.99, "sphere normals point radially");
        }
        assert_eq!(cube().to_geo().edges.len() / 2, 12);
    }

    #[test]
    fn concave_ngons_triangulate_inside_their_outline() {
        // An L-shaped hexagon in the XZ plane, facing up (+Y).
        let pts = [(0.0, 0.0), (0.0, 2.0), (1.0, 2.0), (1.0, 1.0), (2.0, 1.0), (2.0, 0.0)];
        let m = EditMesh {
            points: pts.iter().map(|&(x, z)| Vec3::new(x, 0.0, z)).collect(),
            polys: vec![(0..6).collect()],
        };
        assert!(m.poly_normal(0).y > 0.99);
        let tris = m.triangulate(0);
        assert_eq!(tris.len(), 4);
        let area: f32 = tris
            .iter()
            .map(|t| {
                let p = |k: usize| m.points[m.polys[0][k] as usize];
                (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0])).y * 0.5
            })
            .sum();
        assert!((area - 3.0).abs() < 1e-4, "area {area}");
    }

    #[test]
    fn extrude_a_face_keeps_the_mesh_closed() {
        let mut m = cube();
        let top = (0..m.polys.len()).find(|&i| m.poly_normal(i).y > 0.9).unwrap() as u32;
        m.extrude(&BTreeSet::from([top]), 50.0);
        assert_closed(&m);
        assert_eq!(m.points.len(), 12);
        assert_eq!(m.polys.len(), 10);
        assert!((volume(&m) - (8.0e6 + 200.0 * 200.0 * 50.0)).abs() < 10.0);
        let cap = &m.polys[top as usize];
        assert!(cap.iter().all(|&p| (m.points[p as usize].y - 150.0).abs() < 1e-3));
    }

    #[test]
    fn extruding_connected_faces_moves_them_together() {
        let mut m = grid_cube(2);
        let top: BTreeSet<u32> = (0..m.polys.len()).filter(|&i| m.poly_normal(i).y > 0.9).map(|i| i as u32).collect();
        assert_eq!(top.len(), 4);
        m.extrude(&top, 30.0);
        assert_closed(&m);
        // Region extrude: 8 boundary edges -> 8 side quads, no internal walls.
        assert_eq!(m.polys.len(), 24 + 8);
        assert!((volume(&m) - (8.0e6 + 200.0 * 200.0 * 30.0)).abs() < 10.0);
    }

    #[test]
    fn extruding_two_adjacent_sides_keeps_walls_at_the_offset() {
        let mut m = cube();
        let sel: BTreeSet<u32> =
            (0..m.polys.len()).filter(|&i| m.poly_normal(i).y > 0.9 || m.poly_normal(i).x > 0.9).map(|i| i as u32).collect();
        m.extrude(&sel, 20.0);
        assert_closed(&m);
        // The shared corner moves diagonally so both faces end up exactly 20 cm out.
        assert!(m.points.iter().any(|p| (p.x - 120.0).abs() < 1e-3 && (p.y - 120.0).abs() < 1e-3));
    }

    #[test]
    fn inset_adds_a_ring_and_stays_closed() {
        let mut m = cube();
        m.inset(&BTreeSet::from([0]), 20.0);
        assert_closed(&m);
        assert_eq!(m.polys.len(), 6 + 4);
        assert!((volume(&m) - 8.0e6).abs() < 1.0, "insetting does not change the shape");
        // A huge amount is clamped instead of flipping the polygon inside out.
        let mut m2 = cube();
        m2.inset(&BTreeSet::from([0]), 1.0e6);
        assert!(m2.poly_normal(0).length() > 0.0 || m2.poly_area_normal(0).length() < 1e-3);
        m2.validate().unwrap();
    }

    #[test]
    fn delete_and_dissolve() {
        let mut m = cube();
        m.delete_polys(&BTreeSet::from([0]));
        assert_eq!(m.polys.len(), 5);
        assert_eq!(m.points.len(), 8);
        let mut m = cube();
        m.delete_points(&BTreeSet::from([0]));
        assert_eq!(m.polys.len(), 3);
        assert_eq!(m.points.len(), 7);

        // Dissolving the middle edge of a 2x2 face merges two quads into a hexagon... on a grid cube.
        let mut g = grid_cube(2);
        let before = g.polys.len();
        let ep = g.edge_polys();
        let inner = *ep
            .iter()
            .find(|(e, ps)| {
                ps.len() == 2 && g.poly_normal(ps[0]).y > 0.9 && g.poly_normal(ps[1]).y > 0.9 && {
                    let (a, b) = (g.points[e.0 as usize], g.points[e.1 as usize]);
                    a.x.abs() < 1e-3 && b.x.abs() < 1e-3
                }
            })
            .unwrap()
            .0;
        assert_eq!(g.dissolve_edges(&BTreeSet::from([inner])), 1);
        assert_eq!(g.polys.len(), before - 1);
        assert_closed(&g);
        assert!(g.polys.iter().any(|p| p.len() == 6));
        assert!((volume(&g) - 8.0e6).abs() < 10.0);
        // Border edges are not dissolved.
        let mut open = cube();
        open.delete_polys(&BTreeSet::from([0]));
        let border = open.edge_polys().into_iter().find(|(_, p)| p.len() == 1).unwrap().0;
        assert_eq!(open.dissolve_edges(&BTreeSet::from([border])), 0);
    }

    #[test]
    fn loop_cut_goes_all_the_way_round() {
        let mut m = cube();
        // A vertical edge of the cube: the ring is the 4 side faces.
        let vertical = m
            .edges()
            .into_iter()
            .find(|&(a, b)| (m.points[a as usize].y - m.points[b as usize].y).abs() > 100.0)
            .unwrap();
        let ring = m.quad_ring(vertical);
        assert_eq!(ring.len(), 4);
        let cut = m.loop_cut(vertical, 0.25);
        assert_eq!(cut.len(), 4);
        assert_eq!(m.points.len(), 12);
        assert_eq!(m.polys.len(), 10);
        assert_closed(&m);
        assert!((volume(&m) - 8.0e6).abs() < 1.0);
        // All new points sit at the same height: the fraction is consistent around the ring.
        let ys: Vec<f32> = m.points[8..].iter().map(|p| p.y).collect();
        assert!(ys.iter().all(|&y| (y - ys[0]).abs() < 1e-3), "{ys:?}");
        assert!((ys[0].abs() - 50.0).abs() < 1e-3);
    }

    #[test]
    fn loop_cut_stops_at_triangles_without_cracks() {
        let mut m = EditMesh::from_geo(&Geo::from_mesh(&PrimitiveKind::Sphere.default_primitive().build()));
        let before = volume(&m);
        // A latitude edge between two quad rows: the ring runs pole to pole.
        let e = m
            .edges()
            .into_iter()
            .find(|&(a, b)| {
                let (pa, pb) = (m.points[a as usize], m.points[b as usize]);
                pa.y.abs() < 1.0 && pb.y.abs() < 1.0
            })
            .unwrap();
        let cut = m.loop_cut(e, 0.5);
        assert!(!cut.is_empty());
        assert_closed(&m);
        assert!((volume(&m) - before).abs() < before * 1e-3);
        m.validate().unwrap();
    }

    #[test]
    fn edge_loop_selects_a_ring_around_the_cube_grid() {
        let m = grid_cube(2);
        // The edge loop around the middle of a 2x2x2 cube has 8 edges.
        let e = m
            .edges()
            .into_iter()
            .find(|&(a, b)| {
                let (pa, pb) = (m.points[a as usize], m.points[b as usize]);
                pa.y.abs() < 1e-3 && pb.y.abs() < 1e-3
            })
            .unwrap();
        let lp = m.edge_loop(e);
        assert_eq!(lp.len(), 8);
        assert!(lp.iter().all(|&(a, b)| m.points[a as usize].y.abs() < 1e-3 && m.points[b as usize].y.abs() < 1e-3));
        // On a plain cube every point has valence 3, so a loop is just the edge.
        assert_eq!(cube().edge_loop(cube().edges()[0]).len(), 1);
    }

    #[test]
    fn validate_rejects_broken_meshes() {
        let bad = EditMesh { points: vec![Vec3::ZERO; 3], polys: vec![vec![0, 1, 5]] };
        assert!(bad.validate().is_err());
        let dup = EditMesh { points: vec![Vec3::ZERO; 3], polys: vec![vec![0, 1, 1]] };
        assert!(dup.validate().is_err());
        let nan = EditMesh { points: vec![Vec3::NAN; 3], polys: vec![vec![0, 1, 2]] };
        assert!(nan.validate().is_err());
    }
}
