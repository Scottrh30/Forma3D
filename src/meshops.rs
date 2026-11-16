//! More modelling operations on `EditMesh`: Bevel, Knife, Bridge, Weld /
//! Optimize and Subdivide. Every operation keeps a closed mesh closed.

use crate::editmesh::{Edge, EditMesh};
use crate::generator::{catmull_clark, edge_key};
use glam::Vec3;
use std::collections::{BTreeSet, HashMap, HashSet};

impl EditMesh {
    /// Directed edges used by exactly one polygon with no partner going the
    /// other way: the mesh's open borders, as `from -> to`.
    pub fn border_edges(&self) -> Vec<(u32, u32)> {
        let mut directed = HashSet::new();
        for poly in &self.polys {
            for k in 0..poly.len() {
                directed.insert((poly[k], poly[(k + 1) % poly.len()]));
            }
        }
        let mut out: Vec<(u32, u32)> = directed.iter().copied().filter(|&(a, b)| !directed.contains(&(b, a))).collect();
        out.sort_unstable();
        out
    }

    /// Inserts new points into polygon edges: `map[edge]` lists points to put
    /// between the edge's ends, ordered from `edge.0` to `edge.1`.
    /// Polygons in `skip` are left alone.
    fn insert_edge_points(&mut self, map: &HashMap<Edge, Vec<u32>>, skip: &HashSet<usize>) {
        if map.is_empty() {
            return;
        }
        for (pi, poly) in self.polys.iter_mut().enumerate() {
            if skip.contains(&pi) {
                continue;
            }
            let mut out = Vec::with_capacity(poly.len() + 2);
            for k in 0..poly.len() {
                let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
                out.push(a);
                if let Some(pts) = map.get(&edge_key(a, b)) {
                    if a < b {
                        out.extend(pts.iter().copied());
                    } else {
                        out.extend(pts.iter().rev().copied());
                    }
                }
            }
            *poly = out;
        }
    }

    /// Fills each closed border loop that passes through one of `new_points`
    /// with a single polygon (the holes left at bevelled corners).
    fn fill_holes_touching(&mut self, new_points: &HashSet<u32>) {
        let border = self.border_edges();
        // The fill polygon runs against the border direction.
        let mut next: HashMap<u32, u32> = HashMap::new();
        let mut ambiguous = HashSet::new();
        for &(a, b) in &border {
            if next.insert(b, a).is_some() {
                ambiguous.insert(b);
            }
        }
        let mut used = HashSet::new();
        for &(_, start) in &border {
            if used.contains(&start) || ambiguous.contains(&start) {
                continue;
            }
            let mut lp = vec![start];
            let mut cur = start;
            let ok = loop {
                let Some(&n) = next.get(&cur) else { break false };
                if ambiguous.contains(&n) {
                    break false;
                }
                if n == start {
                    break true;
                }
                if lp.contains(&n) || lp.len() > border.len() {
                    break false;
                }
                lp.push(n);
                cur = n;
            };
            used.extend(lp.iter().copied());
            if ok && lp.len() >= 3 && lp.iter().any(|p| new_points.contains(p)) {
                self.polys.push(lp);
            }
        }
    }

    // --- Bevel -----------------------------------------------------------------

    /// Chamfers the selected edges by `offset` (cm). Each edge becomes a strip
    /// polygon; fully bevelled corners get a cap. Border edges are skipped.
    /// Returns the strip polygons.
    pub fn bevel_edges(&mut self, edges: &BTreeSet<Edge>, offset: f32) -> BTreeSet<u32> {
        let edge_polys = self.edge_polys();
        let sel: BTreeSet<Edge> = edges
            .iter()
            .map(|&(a, b)| edge_key(a, b))
            .filter(|e| edge_polys.get(e).is_some_and(|p| p.len() == 2))
            .collect();
        if sel.is_empty() || offset <= 0.0 {
            return BTreeSet::new();
        }
        // Never slide further than ~half of any edge at a bevelled point.
        let bevelled: HashSet<u32> = sel.iter().flat_map(|&(a, b)| [a, b]).collect();
        let mut max_off = f32::MAX;
        for &(a, b) in edge_polys.keys() {
            if bevelled.contains(&a) || bevelled.contains(&b) {
                max_off = max_off.min((self.points[a as usize] - self.points[b as usize]).length() * 0.45);
            }
        }
        let off = offset.min(max_off);

        enum Key {
            Slide(u32, u32), // point slid from .0 towards .1
            Corner(u32, usize),
        }
        let mut new_point: HashMap<(u32, u32, u8), u32> = HashMap::new();
        let mut created: HashSet<u32> = HashSet::new();
        let mut slide_points: HashMap<Edge, Vec<u32>> = HashMap::new();
        let polys = self.polys.clone();
        // New corner of each (polygon, corner index).
        let mut corner: HashMap<(usize, usize), u32> = HashMap::new();
        for (pi, poly) in polys.iter().enumerate() {
            let n = poly.len();
            for k in 0..n {
                let (u, v, w) = (poly[(k + n - 1) % n], poly[k], poly[(k + 1) % n]);
                let prev_sel = sel.contains(&edge_key(u, v));
                let next_sel = sel.contains(&edge_key(v, w));
                let key = match (prev_sel, next_sel) {
                    (false, false) => continue,
                    (false, true) => Key::Slide(v, u),
                    (true, false) => Key::Slide(v, w),
                    (true, true) => Key::Corner(v, pi),
                };
                let pv = self.points[v as usize];
                let dir = |t: u32| (self.points[t as usize] - pv).normalize_or_zero();
                let (map_key, pos) = match key {
                    Key::Slide(a, t) => ((a, t, 0u8), pv + dir(t) * off),
                    Key::Corner(a, f) => ((a, f as u32, 1u8), pv + (dir(u) + dir(w)) * off),
                };
                let idx = *new_point.entry(map_key).or_insert_with(|| {
                    self.points.push(pos);
                    let i = (self.points.len() - 1) as u32;
                    created.insert(i);
                    if let Key::Slide(a, t) = key {
                        // Remember it so untouched neighbours along this edge get it too.
                        slide_points.entry(edge_key(a, t)).or_default().push(i);
                    }
                    i
                });
                corner.insert((pi, k), idx);
            }
        }
        // Strips: one per selected edge, from the two polygons' new corners.
        let mut strips = Vec::new();
        for &(a, b) in &sel {
            let adj = &edge_polys[&(a, b)];
            let find = |pi: usize, x: u32| {
                let k = polys[pi].iter().position(|&p| p == x).expect("edge point in polygon");
                corner[&(pi, k)]
            };
            // Orient: f1 holds a -> b.
            let holds = |pi: usize| {
                let p = &polys[pi];
                (0..p.len()).any(|k| p[k] == a && p[(k + 1) % p.len()] == b)
            };
            let (f1, f2) = if holds(adj[0]) { (adj[0], adj[1]) } else { (adj[1], adj[0]) };
            let (a1, b1, a2, b2) = (find(f1, a), find(f1, b), find(f2, a), find(f2, b));
            let mut q = vec![b1, a1, a2, b2];
            q.dedup();
            if q.len() >= 3 && q.first() != q.last() {
                strips.push(q);
            }
        }
        // Moved corners replace the old point in each polygon.
        for (pi, poly) in self.polys.iter_mut().enumerate() {
            for (k, p) in poly.iter_mut().enumerate() {
                if let Some(&c) = corner.get(&(pi, k)) {
                    *p = c;
                }
            }
        }
        // Untouched polygons along a slid edge get the new point(s) too.
        let moved_polys: HashSet<usize> = HashSet::new();
        // Order points along each edge from the lower index end.
        for (e, pts) in slide_points.iter_mut() {
            let from = self.points[e.0 as usize];
            pts.sort_by(|&x, &y| {
                (self.points[x as usize] - from).length().total_cmp(&(self.points[y as usize] - from).length())
            });
        }
        // Only polygons whose corner was not slid along that edge need it; a
        // polygon whose corner already moved no longer has the old edge.
        self.insert_edge_points(&slide_points, &moved_polys);
        // A polygon left with the original point between two of its own new
        // points (a corner beside a chamfered edge) drops the original point.
        for poly in &mut self.polys {
            let n = poly.len();
            if n <= 3 {
                continue;
            }
            let mut drop = Vec::new();
            for k in 0..n {
                let (p, v, q) = (poly[(k + n - 1) % n], poly[k], poly[(k + 1) % n]);
                if bevelled.contains(&v) && created.contains(&p) && created.contains(&q) {
                    drop.push(k);
                }
            }
            for k in drop.into_iter().rev() {
                if poly.len() > 3 {
                    poly.remove(k);
                }
            }
        }
        let first_strip = self.polys.len() as u32;
        let strip_count = strips.len() as u32;
        self.polys.extend(strips);
        self.fill_holes_touching(&created);
        self.remove_unused_points();
        (first_strip..first_strip + strip_count).collect()
    }

    // --- Knife -----------------------------------------------------------------

    /// Cuts every polygon crossed by the plane (point, normal), or only the
    /// polygons in `only` when given. Returns the new edges.
    pub fn knife(&mut self, plane_point: Vec3, plane_normal: Vec3, only: Option<&BTreeSet<u32>>) -> Vec<Edge> {
        let n = plane_normal.normalize_or_zero();
        if n == Vec3::ZERO || self.points.is_empty() {
            return Vec::new();
        }
        let (lo, hi) = self.points.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(l, h), p| (l.min(*p), h.max(*p)));
        let eps = (hi - lo).max_element().max(1.0) * 1e-5;
        let dist: Vec<f32> = self.points.iter().map(|p| (*p - plane_point).dot(n)).collect();
        let side = |p: u32| {
            let d = dist[p as usize];
            if d > eps {
                1
            } else if d < -eps {
                -1
            } else {
                0
            }
        };
        let targets: HashSet<usize> = match only {
            Some(sel) => sel.iter().map(|&i| i as usize).filter(|&i| i < self.polys.len()).collect(),
            None => (0..self.polys.len()).collect(),
        };
        // New points on crossed edges of the target polygons.
        let mut on_edge: HashMap<Edge, Vec<u32>> = HashMap::new();
        for &pi in &targets {
            let poly = &self.polys[pi];
            for k in 0..poly.len() {
                let e = edge_key(poly[k], poly[(k + 1) % poly.len()]);
                if on_edge.contains_key(&e) {
                    continue;
                }
                let (sa, sb) = (side(e.0), side(e.1));
                if sa * sb < 0 {
                    let (da, db) = (dist[e.0 as usize], dist[e.1 as usize]);
                    let p = self.points[e.0 as usize].lerp(self.points[e.1 as usize], da / (da - db));
                    self.points.push(p);
                    on_edge.insert(e, vec![(self.points.len() - 1) as u32]);
                }
            }
        }
        let first_new = (self.points.len() - on_edge.len()) as u32;
        self.insert_edge_points(&on_edge, &HashSet::new());
        // Split target polygons that now have exactly two cut points.
        let is_cut = |p: u32, d: &[f32]| p >= first_new || d.get(p as usize).is_some_and(|x| x.abs() <= eps);
        let mut new_edges = Vec::new();
        let mut appended = Vec::new();
        for &pi in &targets {
            let poly = self.polys[pi].clone();
            let cuts: Vec<usize> = (0..poly.len()).filter(|&k| is_cut(poly[k], &dist)).collect();
            if cuts.len() != 2 {
                continue;
            }
            let (i, j) = (cuts[0], cuts[1]);
            let m = poly.len();
            if j - i == 1 || (i == 0 && j == m - 1) {
                continue; // the plane only runs along an existing edge
            }
            let first: Vec<u32> = poly[i..=j].to_vec();
            let second: Vec<u32> = poly[j..].iter().chain(&poly[..=i]).copied().collect();
            self.polys[pi] = first;
            appended.push(second);
            new_edges.push(edge_key(poly[i], poly[j]));
        }
        self.polys.extend(appended);
        new_edges
    }

    // --- Bridge ----------------------------------------------------------------

    /// Joins two border chains (`p`, `q`: points in border direction, i.e. each
    /// `p[i] -> p[i+1]` is used by an existing polygon). Closed loops pair up
    /// with the rotation that gives the shortest connections.
    fn bridge_chains(&mut self, p: &[u32], q: &[u32], closed: bool) -> usize {
        let pos = |i: u32| self.points[i as usize];
        let mut quads = Vec::new();
        if closed {
            let n = p.len();
            let best = (0..n)
                .min_by(|&s1, &s2| {
                    let cost = |s: usize| (0..n).map(|i| (pos(p[i]) - pos(q[(s + n - i) % n])).length()).sum::<f32>();
                    cost(s1).total_cmp(&cost(s2))
                })
                .unwrap_or(0);
            for i in 0..n {
                let qi = |k: usize| q[(best + n * 2 - k) % n];
                quads.push(vec![p[(i + 1) % n], p[i], qi(i), qi(i + 1)]);
            }
        } else {
            let n = p.len() - 1; // edges
            for i in 0..n {
                quads.push(vec![p[i + 1], p[i], q[n - i], q[n - i - 1]]);
            }
        }
        let mut made = 0;
        for mut quad in quads {
            quad.dedup();
            while quad.len() > 1 && quad.first() == quad.last() {
                quad.pop();
            }
            let unique: HashSet<u32> = quad.iter().copied().collect();
            if quad.len() >= 3 && unique.len() == quad.len() {
                self.polys.push(quad);
                made += 1;
            }
        }
        made
    }

    /// Splits directed border edges into chains; returns (points, closed).
    fn chains(edges: &[(u32, u32)]) -> Option<Vec<(Vec<u32>, bool)>> {
        let mut next: HashMap<u32, u32> = HashMap::new();
        let mut has_in = HashSet::new();
        for &(a, b) in edges {
            if next.insert(a, b).is_some() {
                return None; // branching
            }
            has_in.insert(b);
        }
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let mut starts: Vec<u32> = edges.iter().map(|e| e.0).filter(|a| !has_in.contains(a)).collect();
        starts.sort_unstable();
        for s in starts {
            let mut chain = vec![s];
            let mut cur = s;
            while let Some(&n) = next.get(&cur) {
                seen.insert(cur);
                chain.push(n);
                cur = n;
            }
            out.push((chain, false));
        }
        let mut rest: Vec<u32> = edges.iter().map(|e| e.0).filter(|a| !seen.contains(a)).collect();
        rest.sort_unstable();
        for s in rest {
            if seen.contains(&s) {
                continue;
            }
            let mut chain = vec![s];
            seen.insert(s);
            let mut cur = s;
            while let Some(&n) = next.get(&cur) {
                if n == s {
                    break;
                }
                seen.insert(n);
                chain.push(n);
                cur = n;
            }
            out.push((chain, true));
        }
        Some(out)
    }

    /// Bridges two selected border edges, or two border chains / loops with the
    /// same number of edges. Returns the number of polygons created.
    pub fn bridge_edges(&mut self, edges: &BTreeSet<Edge>) -> Result<usize, String> {
        let border: Vec<(u32, u32)> = self.border_edges().into_iter().filter(|&(a, b)| edges.contains(&edge_key(a, b))).collect();
        if border.len() < 2 {
            return Err("Bridge needs two open (border) edges or edge loops".to_owned());
        }
        let chains = Self::chains(&border).ok_or("The selected border edges branch")?;
        if chains.len() != 2 {
            return Err(format!("Bridge needs exactly two separate edge groups, found {}", chains.len()));
        }
        let ((p, pc), (q, qc)) = (&chains[0], &chains[1]);
        if pc != qc || p.len() != q.len() {
            return Err("Both sides of a bridge need the same number of edges".to_owned());
        }
        Ok(self.bridge_chains(p, q, *pc))
    }

    /// Removes two polygons with the same number of points and joins their
    /// outlines with a tube of quads (a hole through the object).
    pub fn bridge_polys(&mut self, a: u32, b: u32) -> Result<usize, String> {
        let (a, b) = (a as usize, b as usize);
        if a == b || a >= self.polys.len() || b >= self.polys.len() {
            return Err("Bridge needs two different polygons".to_owned());
        }
        if self.polys[a].len() != self.polys[b].len() {
            return Err("Both polygons need the same number of points".to_owned());
        }
        let shared = self.polys[a].iter().any(|p| self.polys[b].contains(p));
        if shared {
            return Err("The polygons must not touch".to_owned());
        }
        // Border direction of a removed polygon's hole is its own winding reversed.
        let mut pa = self.polys[a].clone();
        pa.reverse();
        let mut pb = self.polys[b].clone();
        pb.reverse();
        let (hi, lo) = (a.max(b), a.min(b));
        self.polys.remove(hi);
        self.polys.remove(lo);
        let made = self.bridge_chains(&pa, &pb, true);
        self.remove_unused_points();
        Ok(made)
    }

    // --- Weld / Optimize ---------------------------------------------------------

    /// Applies a point remapping (old -> new index) and cleans up polygons that
    /// collapse. Unused points are removed.
    fn merge_points(&mut self, remap: &HashMap<u32, u32>) {
        for poly in &mut self.polys {
            for p in poly.iter_mut() {
                if let Some(&t) = remap.get(p) {
                    *p = t;
                }
            }
            poly.dedup();
            while poly.len() > 1 && poly.first() == poly.last() {
                poly.pop();
            }
        }
        self.polys.retain(|p| {
            let unique: HashSet<u32> = p.iter().copied().collect();
            p.len() >= 3 && unique.len() == p.len()
        });
        self.remove_unused_points();
    }

    /// Welds the given points into one, placed at their centre.
    pub fn weld(&mut self, points: &BTreeSet<u32>) -> bool {
        let pts: Vec<u32> = points.iter().copied().filter(|&p| (p as usize) < self.points.len()).collect();
        if pts.len() < 2 {
            return false;
        }
        let centre = pts.iter().map(|&p| self.points[p as usize]).sum::<Vec3>() / pts.len() as f32;
        let target = pts[0];
        self.points[target as usize] = centre;
        let remap: HashMap<u32, u32> = pts[1..].iter().map(|&p| (p, target)).collect();
        self.merge_points(&remap);
        true
    }

    /// Merges points closer than `tolerance` (cm). Returns how many were merged.
    pub fn optimize(&mut self, tolerance: f32) -> usize {
        let tol = tolerance.max(1e-6);
        let cell = |p: Vec3| [(p.x / tol).floor() as i64, (p.y / tol).floor() as i64, (p.z / tol).floor() as i64];
        let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
        let mut remap: HashMap<u32, u32> = HashMap::new();
        for i in 0..self.points.len() as u32 {
            let p = self.points[i as usize];
            let c = cell(p);
            let mut found = None;
            'search: for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        if let Some(list) = grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz])
                            && let Some(&j) = list.iter().find(|&&j| (self.points[j as usize] - p).length() <= tol) {
                                found = Some(j);
                                break 'search;
                            }
                    }
                }
            }
            match found {
                Some(j) => {
                    remap.insert(i, j);
                }
                None => grid.entry(c).or_default().push(i),
            }
        }
        let merged = remap.len();
        if merged > 0 {
            self.merge_points(&remap);
        }
        merged
    }

    // --- Subdivide ---------------------------------------------------------------

    /// Catmull-Clark subdivision of the whole mesh (smooth, all quads).
    pub fn subdivide_smooth(&mut self) {
        let (points, polys) = catmull_clark(&self.points, &self.polys);
        self.points = points;
        self.polys = polys;
        self.remove_unused_points();
    }

    /// Splits each selected polygon into quads around its centre without
    /// smoothing. Neighbours get the edge midpoints so no cracks appear.
    /// Returns the new polygons (to keep them selected).
    pub fn subdivide_linear(&mut self, selected: &BTreeSet<u32>) -> BTreeSet<u32> {
        let sel: Vec<usize> = selected.iter().map(|&i| i as usize).filter(|&i| i < self.polys.len()).collect();
        let mut mids: HashMap<Edge, Vec<u32>> = HashMap::new();
        for &pi in &sel {
            let poly = self.polys[pi].clone();
            for k in 0..poly.len() {
                let e = edge_key(poly[k], poly[(k + 1) % poly.len()]);
                if let std::collections::hash_map::Entry::Vacant(slot) = mids.entry(e) {
                    let m = (self.points[e.0 as usize] + self.points[e.1 as usize]) * 0.5;
                    self.points.push(m);
                    slot.insert(vec![(self.points.len() - 1) as u32]);
                }
            }
        }
        let mut new_sel = BTreeSet::new();
        let mut appended = Vec::new();
        for &pi in &sel {
            let poly = self.polys[pi].clone();
            let c = self.poly_center(pi);
            self.points.push(c);
            let ci = (self.points.len() - 1) as u32;
            let n = poly.len();
            let mid = |a: u32, b: u32| mids[&edge_key(a, b)][0];
            for k in 0..n {
                let (prev, cur, next) = (poly[(k + n - 1) % n], poly[k], poly[(k + 1) % n]);
                let quad = vec![cur, mid(cur, next), ci, mid(prev, cur)];
                if k == 0 {
                    self.polys[pi] = quad;
                    new_sel.insert(pi as u32);
                } else {
                    appended.push(quad);
                }
            }
        }
        let skip: HashSet<usize> = sel.iter().copied().collect();
        self.insert_edge_points(&mids, &skip);
        let first = self.polys.len() as u32;
        new_sel.extend(first..first + appended.len() as u32);
        self.polys.extend(appended);
        new_sel
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::Geo;
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

    fn assert_closed(m: &EditMesh) {
        m.validate().unwrap();
        let mut directed: HashMap<(u32, u32), usize> = HashMap::new();
        for poly in &m.polys {
            for k in 0..poly.len() {
                *directed.entry((poly[k], poly[(k + 1) % poly.len()])).or_default() += 1;
            }
        }
        for (&(a, b), &n) in &directed {
            assert_eq!(n, 1, "edge {a}->{b} used {n} times in one direction");
            assert_eq!(directed.get(&(b, a)), Some(&1), "edge {a}-{b} is open or flipped");
        }
    }

    fn edge_where(m: &EditMesh, f: impl Fn(Vec3, Vec3) -> bool) -> Edge {
        m.edges().into_iter().find(|&(a, b)| f(m.points[a as usize], m.points[b as usize])).expect("edge")
    }

    fn top_face(m: &EditMesh) -> u32 {
        (0..m.polys.len()).find(|&i| m.poly_normal(i).y > 0.99).unwrap() as u32
    }

    #[test]
    fn bevel_one_cube_edge() {
        let mut m = cube();
        // The edge at x = 100, y = 100 (running along Z).
        let e = edge_where(&m, |a, b| a.x > 99.0 && b.x > 99.0 && a.y > 99.0 && b.y > 99.0);
        let strips = m.bevel_edges(&BTreeSet::from([e]), 20.0);
        assert_eq!(strips.len(), 1);
        assert_closed(&m);
        // Removes a prism with a 20 x 20 / 2 cross-section along 200 cm.
        let expected = 8.0e6 - 20.0 * 20.0 * 0.5 * 200.0;
        assert!((volume(&m) - expected).abs() < 10.0, "{} vs {expected}", volume(&m));
        assert_eq!(m.points.len(), 10);
        assert_eq!(m.polys.len(), 7, "end faces become pentagons, no extra triangles");
        assert!(m.polys.iter().filter(|p| p.len() == 5).count() == 2);
    }

    #[test]
    fn bevel_all_cube_edges_caps_the_corners() {
        let mut m = cube();
        let all: BTreeSet<Edge> = m.edges().into_iter().collect();
        m.bevel_edges(&all, 20.0);
        assert_closed(&m);
        assert_eq!(m.polys.len(), 6 + 12 + 8, "faces + strips + corner caps");
        let v = volume(&m);
        assert!(v < 8.0e6 - 12.0 * 200.0 * 150.0 && v > 7.0e6, "{v}");
    }

    #[test]
    fn bevel_two_edges_meeting_at_a_corner_and_an_edge_loop() {
        let mut m = cube();
        let e1 = edge_where(&m, |a, b| a.x > 99.0 && b.x > 99.0 && a.y > 99.0 && b.y > 99.0);
        let e2 = edge_where(&m, |a, b| a.z > 99.0 && b.z > 99.0 && a.y > 99.0 && b.y > 99.0);
        m.bevel_edges(&BTreeSet::from([e1, e2]), 15.0);
        assert_closed(&m);

        // A loop across a flat grid: points of valence 4 in the middle.
        let mut g = grid_cube(2);
        let seed = edge_where(&g, |a, b| a.y.abs() < 1e-3 && b.y.abs() < 1e-3);
        let lp: BTreeSet<Edge> = g.edge_loop(seed).into_iter().collect();
        assert_eq!(lp.len(), 8);
        let before = volume(&g);
        g.bevel_edges(&lp, 10.0);
        assert_closed(&g);
        assert!((volume(&g) - before).abs() < before * 1e-5, "a loop on flat faces does not change the shape");
    }

    #[test]
    fn bevel_offset_is_clamped() {
        let mut m = cube();
        let e = m.edges()[0];
        m.bevel_edges(&BTreeSet::from([e]), 1.0e6);
        assert_closed(&m);
        assert!(volume(&m) > 0.0);
    }

    #[test]
    fn knife_cuts_all_the_way_round() {
        let mut m = cube();
        // Horizontal plane at y = 30.
        let cut = m.knife(Vec3::new(0.0, 30.0, 0.0), Vec3::Y, None);
        assert_eq!(cut.len(), 4);
        assert_closed(&m);
        assert_eq!(m.polys.len(), 10);
        assert!((volume(&m) - 8.0e6).abs() < 8.0e6 * 1e-5);
        // A diagonal cut through a corner region still keeps the mesh closed.
        let mut d = cube();
        d.knife(Vec3::new(50.0, 50.0, 50.0), Vec3::new(1.0, 1.0, 1.0), None);
        assert_closed(&d);
        // A plane missing the object changes nothing.
        let mut miss = cube();
        assert!(miss.knife(Vec3::new(0.0, 500.0, 0.0), Vec3::Y, None).is_empty());
        assert_eq!(miss, cube());
    }

    #[test]
    fn knife_limited_to_selected_polygons_and_through_vertices() {
        let mut m = cube();
        let top = top_face(&m);
        let cut = m.knife(Vec3::ZERO, Vec3::X, Some(&BTreeSet::from([top])));
        assert_eq!(cut.len(), 1, "only the top face is split");
        assert_closed(&m);
        // A diagonal plane through two opposite corners of a face uses existing points.
        let mut d = cube();
        let top = top_face(&d);
        let cut = d.knife(Vec3::ZERO, Vec3::new(1.0, 0.0, -1.0), Some(&BTreeSet::from([top])));
        assert_eq!(cut.len(), 1);
        assert_eq!(d.points.len(), 8, "no new points: the cut runs corner to corner");
        assert_closed(&d);
    }

    #[test]
    fn bridge_two_polygons_makes_a_tunnel() {
        // Two boxes' facing sides... simplest: a cube; bridge top and bottom.
        let mut m = cube();
        let top = top_face(&m);
        let bottom = (0..m.polys.len()).find(|&i| m.poly_normal(i).y < -0.99).unwrap() as u32;
        // Shrink the top and bottom faces inwards first so the tunnel has walls.
        m.inset(&BTreeSet::from([top, bottom]), 50.0);
        let made = m.bridge_polys(top, bottom).unwrap();
        assert_eq!(made, 4);
        assert_closed(&m);
        // Inset moves corners 50 cm along the diagonal: the hole is (200 - 2 * 50 / sqrt 2) wide.
        let w = 200.0 - 2.0 * 50.0 / 2f32.sqrt();
        let expected = 8.0e6 - w * w * 200.0;
        assert!((volume(&m) - expected).abs() < expected * 1e-5, "{} vs {expected}", volume(&m));
        assert!(cube().bridge_polys(0, 0).is_err());
    }

    #[test]
    fn bridge_edges_closes_a_gap() {
        // Delete two opposite side faces of a cube, then bridge one pair of the
        // open edges: the result is a closed... open tube gets one wall back.
        let mut m = cube();
        let sides: BTreeSet<u32> = (0..6).filter(|&i| m.poly_normal(i as usize).x.abs() > 0.99).collect();
        m.delete_polys(&sides);
        assert_eq!(m.border_edges().len(), 8);
        // An edge that is not on a border cannot be bridged.
        let inner = edge_where(&m, |a, b| a.x > 99.0 && b.x < -99.0 && a.y > 99.0 && b.y > 99.0);
        assert!(m.bridge_edges(&BTreeSet::from([inner])).is_err());
        // The open vertical edges at x = -100 on the front and back faces.
        let front = edge_where(&m, |a, b| a.x < -99.0 && b.x < -99.0 && a.z > 99.0 && b.z > 99.0);
        let back = edge_where(&m, |a, b| a.x < -99.0 && b.x < -99.0 && a.z < -99.0 && b.z < -99.0);
        let made = m.bridge_edges(&BTreeSet::from([front, back])).unwrap();
        assert_eq!(made, 1);
        // The rebuilt side wall faces outwards (-X).
        let wall = m.polys.len() - 1;
        assert!(m.poly_normal(wall).x < -0.99, "{:?}", m.poly_normal(wall));
        // Two boxes 400 cm apart with their facing sides open: bridging the two
        // open loops joins them into one closed shape.
        let mut two = cube();
        let n = two.points.len() as u32;
        let other = cube();
        two.points.extend(other.points.iter().map(|p| *p + Vec3::new(400.0, 0.0, 0.0)));
        two.polys.extend(other.polys.iter().map(|p| p.iter().map(|i| i + n).collect::<Vec<_>>()));
        let facing: BTreeSet<u32> = (0..two.polys.len())
            .filter(|&i| {
                let (c, nrm) = (two.poly_center(i), two.poly_normal(i));
                (c.x - 100.0).abs() < 1e-3 && nrm.x > 0.99 || (c.x - 300.0).abs() < 1e-3 && nrm.x < -0.99
            })
            .map(|i| i as u32)
            .collect();
        assert_eq!(facing.len(), 2);
        two.delete_polys(&facing);
        let loops: BTreeSet<Edge> = two.border_edges().into_iter().map(|(a, b)| edge_key(a, b)).collect();
        assert_eq!(two.bridge_edges(&loops).unwrap(), 4);
        assert_closed(&two);
        let expected = 3.0 * 8.0e6;
        assert!((volume(&two) - expected).abs() < expected * 1e-5, "{}", volume(&two));
    }

    #[test]
    fn weld_and_optimize() {
        let mut m = cube();
        let top = top_face(&m);
        let pts: BTreeSet<u32> = m.polys[top as usize].iter().copied().collect();
        assert!(m.weld(&pts));
        // The top collapses to a point: a square pyramid.
        assert_eq!(m.points.len(), 5);
        assert_eq!(m.polys.len(), 5);
        assert_closed(&m);
        assert!((volume(&m) - 200.0 * 200.0 * 200.0 / 3.0).abs() < 10.0);

        // Optimize merges duplicated points (e.g. two halves that touch).
        let mut a = cube();
        let n = a.points.len() as u32;
        let copy = cube();
        a.points.extend(copy.points.iter().map(|p| *p + Vec3::new(200.0, 0.0, 0.0)));
        a.polys.extend(copy.polys.iter().map(|p| p.iter().map(|i| i + n).collect::<Vec<_>>()));
        assert_eq!(a.optimize(0.01), 4);
        assert_eq!(a.points.len(), 12);
    }

    #[test]
    fn subdivide_smooth_and_linear() {
        let mut m = cube();
        m.subdivide_smooth();
        assert_eq!(m.polys.len(), 24);
        assert_closed(&m);
        assert!(volume(&m) < 8.0e6);

        let mut l = cube();
        let top = top_face(&l);
        let new = l.subdivide_linear(&BTreeSet::from([top]));
        assert_eq!(new.len(), 4);
        assert_eq!(l.polys.len(), 9);
        assert_closed(&l);
        assert!((volume(&l) - 8.0e6).abs() < 1.0, "linear subdivision keeps the shape");
        // Neighbours received the midpoints: the four sides are now pentagons.
        assert_eq!(l.polys.iter().filter(|p| p.len() == 5).count(), 4);
    }
}
