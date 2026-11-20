//! Wavefront OBJ (with an MTL material library). OBJ is right-handed, Y up;
//! Forma3D writes and reads it in centimetres.

use super::{ExportItem, Imported, ImportedMaterial, Node, linear_to_srgb, mesh_from_rh, reversed, srgb_to_linear, to_rh};
use glam::{Mat4, Vec3};
use std::collections::HashMap;
use std::fmt::Write as _;

/// The OBJ file. Each item is its own object (`o`), with points in world
/// space (OBJ has no transforms) and its material named (`usemtl`).
pub fn write(items: &[ExportItem], scale: f32) -> Vec<u8> {
    let (obj, _) = write_with_mtl(items, scale, "materials.mtl");
    obj.into_bytes()
}

/// The OBJ text and its MTL library.
pub fn write_with_mtl(items: &[ExportItem], scale: f32, mtl_name: &str) -> (String, String) {
    let mut s = String::new();
    let _ = writeln!(s, "# Forma3D\nmtllib {mtl_name}");
    let mut base = 1usize;
    let mut mtl = String::from("# Forma3D\n");
    let mut seen = std::collections::HashSet::new();
    for it in items {
        let _ = writeln!(s, "o {}", it.name.replace(char::is_whitespace, "_"));
        for p in &it.mesh.points {
            let q = to_rh(it.world.transform_point3(*p)) * scale;
            let _ = writeln!(s, "v {} {} {}", q.x, q.y, q.z);
        }
        let mat = it.material.replace(char::is_whitespace, "_");
        let _ = writeln!(s, "usemtl {mat}");
        // Mirrored transforms flip the winding a second time.
        let flip = it.world.determinant() < 0.0;
        for poly in &it.mesh.polys {
            let poly = if flip { poly.clone() } else { reversed(poly) };
            s.push('f');
            for i in poly {
                let _ = write!(s, " {}", base + i as usize);
            }
            s.push('\n');
        }
        base += it.mesh.points.len();
        if seen.insert(mat.clone()) {
            let c = it.color.map(linear_to_srgb);
            let _ = writeln!(
                mtl,
                "newmtl {mat}\nKd {} {} {}\nKs 0.04 0.04 0.04\nNs {}\nPr {}\nPm {}\n",
                c[0],
                c[1],
                c[2],
                (1.0 - it.roughness) * 1000.0,
                it.roughness,
                it.metalness
            );
        }
    }
    (s, mtl)
}

/// Reads an OBJ. Materials come from an MTL text when given.
pub fn read(bytes: &[u8]) -> Result<Imported, String> {
    read_with_mtl(bytes, None)
}

pub fn read_with_mtl(bytes: &[u8], mtl: Option<&str>) -> Result<Imported, String> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Imported::default();
    let mut positions: Vec<Vec3> = Vec::new();
    let mut mat_index: HashMap<String, usize> = HashMap::new();
    if let Some(mtl) = mtl {
        read_mtl(mtl, &mut out, &mut mat_index);
    }
    // The object being filled: name, its points (file index -> local), polygons, material.
    struct Part {
        name: String,
        map: HashMap<usize, u32>,
        points: Vec<Vec3>,
        polys: Vec<Vec<u32>>,
        material: Option<usize>,
    }
    let new_part = |name: &str, material| Part { name: name.to_owned(), map: HashMap::new(), points: Vec::new(), polys: Vec::new(), material };
    let mut parts: Vec<Part> = vec![new_part("Object", None)];
    let mut current_mat: Option<usize> = None;
    let mut lines_skipped = false;
    let mut joined = String::new();
    for raw in text.lines() {
        // Lines ending in a backslash continue on the next.
        if let Some(stripped) = raw.strip_suffix('\\') {
            joined.push_str(stripped);
            joined.push(' ');
            continue;
        }
        joined.push_str(raw);
        let line = std::mem::take(&mut joined);
        let line = line.split('#').next().unwrap_or("").trim();
        let mut it = line.split_whitespace();
        let Some(tag) = it.next() else { continue };
        match tag {
            "v" => {
                let v: Vec<f32> = it.take(3).filter_map(|x| x.parse().ok()).collect();
                if v.len() == 3 {
                    positions.push(Vec3::new(v[0], v[1], v[2]));
                } else {
                    return Err(format!("bad vertex: {line}"));
                }
            }
            "o" | "g" => {
                let name = it.collect::<Vec<_>>().join(" ");
                let last = parts.last().unwrap();
                if last.polys.is_empty() {
                    let m = last.material;
                    *parts.last_mut().unwrap() = new_part(if name.is_empty() { "Object" } else { &name }, m.or(current_mat));
                } else if !(tag == "g" && name == last.name) {
                    parts.push(new_part(if name.is_empty() { "Object" } else { &name }, current_mat));
                }
            }
            "usemtl" => {
                let name = it.collect::<Vec<_>>().join(" ");
                let n = out.materials.len();
                let idx = *mat_index.entry(name.clone()).or_insert_with(|| {
                    out.materials.push(ImportedMaterial { name: name.clone(), color: [0.8; 3], roughness: None, metalness: None });
                    n
                });
                current_mat = Some(idx);
                let last = parts.last_mut().unwrap();
                if last.polys.is_empty() {
                    last.material = Some(idx);
                } else if last.material != Some(idx) {
                    // One material per object: start a new part.
                    let name = last.name.clone();
                    parts.push(new_part(&name, Some(idx)));
                }
            }
            "f" => {
                let part = parts.last_mut().unwrap();
                let mut poly = Vec::new();
                for tok in it {
                    let idx: i64 = tok.split('/').next().unwrap_or("").parse().map_err(|_| format!("bad face: {line}"))?;
                    let abs = if idx < 0 { positions.len() as i64 + idx } else { idx - 1 };
                    if abs < 0 || abs as usize >= positions.len() {
                        return Err(format!("face refers to a missing vertex: {line}"));
                    }
                    let abs = abs as usize;
                    let local = *part.map.entry(abs).or_insert_with(|| {
                        part.points.push(positions[abs]);
                        part.points.len() as u32 - 1
                    });
                    poly.push(local);
                }
                part.polys.push(poly);
            }
            "l" | "p" | "curv" | "surf" => lines_skipped = true,
            _ => {}
        }
    }
    if lines_skipped {
        out.notes.push("lines, points and curves were left out".to_owned());
    }
    for p in parts.into_iter().filter(|p| !p.polys.is_empty()) {
        out.nodes.push(Node { name: p.name, parent: None, local: Mat4::IDENTITY, mesh: Some(mesh_from_rh(p.points, p.polys)), material: p.material });
    }
    Ok(out)
}

fn read_mtl(text: &str, out: &mut Imported, index: &mut HashMap<String, usize>) {
    let mut current: Option<usize> = None;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let mut it = line.split_whitespace();
        match it.next() {
            Some("newmtl") => {
                let name = it.collect::<Vec<_>>().join(" ");
                out.materials.push(ImportedMaterial { name: name.clone(), color: [0.8; 3], roughness: None, metalness: None });
                index.insert(name, out.materials.len() - 1);
                current = Some(out.materials.len() - 1);
            }
            Some("Kd") => {
                let v: Vec<f32> = it.take(3).filter_map(|x| x.parse().ok()).collect();
                if let (Some(c), 3) = (current, v.len()) {
                    out.materials[c].color = [srgb_to_linear(v[0]), srgb_to_linear(v[1]), srgb_to_linear(v[2])];
                }
            }
            Some("Pr") => {
                if let (Some(c), Some(v)) = (current, it.next().and_then(|x| x.parse::<f32>().ok())) {
                    out.materials[c].roughness = Some(v);
                }
            }
            Some("Pm") => {
                if let (Some(c), Some(v)) = (current, it.next().and_then(|x| x.parse::<f32>().ok())) {
                    out.materials[c].metalness = Some(v);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Format, testing};
    use super::*;

    #[test]
    fn round_trips() {
        let back = testing::round_trip(Format::Obj);
        assert_eq!(back.mesh_count(), 2);
        // Quads stay quads.
        assert!(back.nodes[0].mesh.as_ref().unwrap().polys.iter().all(|p| p.len() == 4));
    }

    #[test]
    fn reads_mtl_colours_negative_indices_and_groups() {
        let obj = "mtllib a.mtl\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\ng first\nusemtl Blue\nf -4 -3 -2 -1\ng second\nf 1/1/1 2/2/2 3/3/3\n";
        let mtl = "newmtl Blue\nKd 0 0 1\n";
        let i = read_with_mtl(obj.as_bytes(), Some(mtl)).unwrap();
        assert_eq!(i.nodes.len(), 2);
        assert_eq!(i.nodes[0].name, "first");
        assert_eq!(i.materials[0].color, [0.0, 0.0, 1.0]);
        assert_eq!(i.nodes[1].mesh.as_ref().unwrap().polys[0].len(), 3);
        assert!(read(b"f 1 2 3\n").is_err());
    }
}
