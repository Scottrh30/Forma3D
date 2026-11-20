//! STL (binary when writing; binary or ASCII when reading). STL is for 3D
//! printing: right-handed with Z up, in millimetres by default.

use super::{ExportItem, Imported, Node};
use crate::editmesh::EditMesh;
use glam::{Mat4, Vec3};
use std::collections::HashMap;

/// Forma3D (left-handed, Y up) to STL (right-handed, Z up): swapping Y and Z
/// is a mirror, so the winding is kept as it is.
fn to_stl(p: Vec3) -> Vec3 {
    Vec3::new(p.x, p.z, p.y)
}

pub fn write(items: &[ExportItem], scale: f32) -> Vec<u8> {
    let mut tris: Vec<[Vec3; 3]> = Vec::new();
    for it in items {
        let flip = it.world.determinant() < 0.0;
        for (i, _) in it.mesh.polys.iter().enumerate() {
            for t in it.mesh.triangulate(i) {
                let poly = &it.mesh.polys[i];
                let p = |k: usize| to_stl(it.world.transform_point3(it.mesh.points[poly[k] as usize])) * scale;
                // The axis swap mirrors, so the winding reverses (twice when
                // the object is mirrored too).
                tris.push(if flip { [p(t[0]), p(t[1]), p(t[2])] } else { [p(t[0]), p(t[2]), p(t[1])] });
            }
        }
    }
    let mut out = Vec::with_capacity(84 + tris.len() * 50);
    let mut header = [0u8; 80];
    let tag = b"Forma3D binary STL";
    header[..tag.len()].copy_from_slice(tag);
    out.extend_from_slice(&header);
    out.extend_from_slice(&(tris.len() as u32).to_le_bytes());
    for t in &tris {
        let n = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
        for v in [n, t[0], t[1], t[2]] {
            for c in v.to_array() {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        out.extend_from_slice(&0u16.to_le_bytes());
    }
    out
}

pub fn read(bytes: &[u8]) -> Result<Imported, String> {
    let tris = if is_binary(bytes) { read_binary(bytes)? } else { read_ascii(&String::from_utf8_lossy(bytes))? };
    if tris.is_empty() {
        return Err("the STL has no triangles".to_owned());
    }
    // Weld equal points so the mesh is connected.
    let mut index: HashMap<[u32; 3], u32> = HashMap::new();
    let mut points = Vec::new();
    let mut polys = Vec::with_capacity(tris.len());
    for t in &tris {
        let mut poly = Vec::with_capacity(3);
        // Reverse the winding (the axis swap below mirrors).
        for p in [t[0], t[2], t[1]] {
            let q = Vec3::new(p.x, p.z, p.y);
            let key = q.to_array().map(f32::to_bits);
            let i = *index.entry(key).or_insert_with(|| {
                points.push(q);
                points.len() as u32 - 1
            });
            poly.push(i);
        }
        polys.push(poly);
    }
    let mut out = Imported::default();
    out.nodes.push(Node { name: "STL".to_owned(), parent: None, local: Mat4::IDENTITY, mesh: Some(EditMesh { points, polys }), material: None });
    Ok(out)
}

fn is_binary(bytes: &[u8]) -> bool {
    if bytes.len() < 84 {
        return false;
    }
    let n = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
    // Exact size match is the reliable test (some binary files start with "solid").
    84 + n * 50 == bytes.len() || !bytes.starts_with(b"solid")
}

fn read_binary(bytes: &[u8]) -> Result<Vec<[Vec3; 3]>, String> {
    let n = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
    if bytes.len() < 84 + n * 50 {
        return Err("the STL is cut short".to_owned());
    }
    let f = |o: usize| f32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
    Ok((0..n)
        .map(|i| {
            let b = 84 + i * 50 + 12;
            let v = |k: usize| Vec3::new(f(b + k * 12), f(b + k * 12 + 4), f(b + k * 12 + 8));
            [v(0), v(1), v(2)]
        })
        .collect())
}

fn read_ascii(text: &str) -> Result<Vec<[Vec3; 3]>, String> {
    let mut tris = Vec::new();
    let mut cur: Vec<Vec3> = Vec::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("vertex") => {
                let v: Vec<f32> = it.take(3).filter_map(|x| x.parse().ok()).collect();
                if v.len() != 3 {
                    return Err(format!("bad vertex: {}", line.trim()));
                }
                cur.push(Vec3::new(v[0], v[1], v[2]));
            }
            Some("endloop") => {
                // Polygons with more points become a fan.
                for k in 1..cur.len().saturating_sub(1) {
                    tris.push([cur[0], cur[k], cur[k + 1]]);
                }
                cur.clear();
            }
            _ => {}
        }
    }
    Ok(tris)
}

#[cfg(test)]
mod tests {
    use super::super::{Format, testing};
    use super::*;

    #[test]
    fn round_trips() {
        let back = testing::round_trip(Format::Stl);
        assert_eq!(back.mesh_count(), 1);
    }

    #[test]
    fn reads_ascii() {
        let s = "solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 10 0 0\nvertex 0 10 0\nendloop\nendfacet\nendsolid t\n";
        let i = read(s.as_bytes()).unwrap();
        let m = i.nodes[0].mesh.as_ref().unwrap();
        assert_eq!(m.polys.len(), 1);
        // STL's Z (up) becomes Forma3D's Y.
        assert!(m.points.iter().all(|p| p.y == 0.0));
    }
}
