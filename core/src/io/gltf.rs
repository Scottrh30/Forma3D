//! glTF 2.0: binary GLB on export; GLB, or .gltf with embedded (data URI)
//! buffers, on import. glTF is right-handed, Y up, in metres.

use super::{ExportItem, Imported, ImportedMaterial, Node, mat_to_rh, mesh_from_rh, to_rh};
use base64::Engine as _;
use glam::{Mat4, Quat, Vec3};
use serde_json::{Value, json};

pub fn write(items: &[ExportItem], scale: f32) -> Vec<u8> {
    let mut bin: Vec<u8> = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    let mut meshes = Vec::new();
    let mut nodes = Vec::new();
    let mut materials = Vec::new();
    let mut mat_names: Vec<String> = Vec::new();
    for (k, it) in items.iter().enumerate() {
        // Points in object space; the node carries the transform.
        let geo = it.mesh.to_geo();
        let pos: Vec<Vec3> = geo.verts.iter().map(|v| to_rh(Vec3::from(v.pos)) * scale).collect();
        let nor: Vec<Vec3> = geo.verts.iter().map(|v| to_rh(Vec3::from(v.normal)).normalize_or(Vec3::Y)).collect();
        let mut idx: Vec<u32> = Vec::with_capacity(geo.tris.len());
        for t in geo.tris.chunks_exact(3) {
            // Mirroring Z reverses the winding.
            idx.extend_from_slice(&[t[0], t[2], t[1]]);
        }
        let (lo, hi) = pos.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(a, b), p| (a.min(*p), b.max(*p)));
        let mut push = |data: &[u8], target: u32| {
            while bin.len() % 4 != 0 {
                bin.push(0);
            }
            let offset = bin.len();
            bin.extend_from_slice(data);
            views.push(json!({"buffer": 0, "byteOffset": offset, "byteLength": data.len(), "target": target}));
            views.len() - 1
        };
        let flat = |v: &[Vec3]| v.iter().flat_map(|p| p.to_array()).flat_map(f32::to_le_bytes).collect::<Vec<u8>>();
        let pv = push(&flat(&pos), 34962);
        let nv = push(&flat(&nor), 34962);
        let iv = push(&idx.iter().flat_map(|i| i.to_le_bytes()).collect::<Vec<u8>>(), 34963);
        let a = accessors.len();
        accessors.push(json!({"bufferView": pv, "componentType": 5126, "count": pos.len(), "type": "VEC3", "min": lo.to_array(), "max": hi.to_array()}));
        accessors.push(json!({"bufferView": nv, "componentType": 5126, "count": nor.len(), "type": "VEC3"}));
        accessors.push(json!({"bufferView": iv, "componentType": 5125, "count": idx.len(), "type": "SCALAR"}));
        // One glTF material per Forma3D material.
        let mi = match mat_names.iter().position(|n| *n == it.material) {
            Some(i) => i,
            None => {
                mat_names.push(it.material.clone());
                materials.push(json!({
                    "name": it.material,
                    "pbrMetallicRoughness": {
                        "baseColorFactor": [it.color[0], it.color[1], it.color[2], 1.0],
                        "metallicFactor": it.metalness,
                        "roughnessFactor": it.roughness,
                    }
                }));
                materials.len() - 1
            }
        };
        meshes.push(json!({"name": it.name, "primitives": [{"attributes": {"POSITION": a, "NORMAL": a + 1}, "indices": a + 2, "material": mi, "mode": 4}]}));
        let m = mat_to_rh(it.world);
        let (s, r, t) = m.to_scale_rotation_translation();
        let mut node = json!({"name": it.name, "mesh": k});
        if m.determinant() < 0.0 || !s.is_finite() {
            let mut cols = m.to_cols_array();
            for c in &mut cols[12..15] {
                *c *= scale;
            }
            node["matrix"] = json!(cols);
        } else {
            node["translation"] = json!((t * scale).to_array());
            node["rotation"] = json!(r.to_array());
            node["scale"] = json!(s.to_array());
        }
        nodes.push(node);
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let doc = json!({
        "asset": {"version": "2.0", "generator": "Forma3D"},
        "scene": 0,
        "scenes": [{"nodes": (0..nodes.len()).collect::<Vec<_>>()}],
        "nodes": nodes,
        "meshes": meshes,
        "materials": materials,
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": bin.len()}],
    });
    let mut js = serde_json::to_vec(&doc).unwrap_or_default();
    while js.len() % 4 != 0 {
        js.push(b' ');
    }
    let total = 12 + 8 + js.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(js.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&js);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    out
}

pub fn read(bytes: &[u8]) -> Result<Imported, String> {
    let (doc, glb_bin): (Value, Option<&[u8]>) = if bytes.starts_with(b"glTF") {
        let u = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).ok_or("the GLB is cut short");
        let json_len = u(12)?;
        let json = bytes.get(20..20 + json_len).ok_or("the GLB is cut short")?;
        let doc = serde_json::from_slice(json).map_err(|e| e.to_string())?;
        let at = 20 + json_len;
        let bin = if bytes.len() >= at + 8 {
            let len = u(at)?;
            bytes.get(at + 8..at + 8 + len)
        } else {
            None
        };
        (doc, bin)
    } else {
        (serde_json::from_slice(bytes).map_err(|e| format!("not glTF: {e}"))?, None)
    };
    let mut out = Imported::default();
    // Buffers: the GLB's own, or base64 data URIs.
    let buffers: Vec<Vec<u8>> = doc["buffers"]
        .as_array()
        .map(|a| {
            a.iter()
                .enumerate()
                .map(|(i, b)| match b["uri"].as_str() {
                    None if i == 0 => glb_bin.map(<[u8]>::to_vec).unwrap_or_default(),
                    Some(uri) if uri.starts_with("data:") => {
                        uri.split_once(',').and_then(|(_, d)| base64::engine::general_purpose::STANDARD.decode(d).ok()).unwrap_or_default()
                    }
                    _ => Vec::new(),
                })
                .collect()
        })
        .unwrap_or_default();
    if doc["buffers"].as_array().is_some_and(|a| a.iter().any(|b| b["uri"].as_str().is_some_and(|u| !u.starts_with("data:")))) {
        return Err("this .gltf keeps its data in separate files; export it as GLB (binary glTF) instead".to_owned());
    }
    let accessor = |i: usize| -> Result<Vec<f32>, String> { read_accessor(&doc, &buffers, i) };
    for m in doc["materials"].as_array().into_iter().flatten() {
        let pbr = &m["pbrMetallicRoughness"];
        let c =
            pbr["baseColorFactor"].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.8) as f32).collect::<Vec<_>>()).unwrap_or(vec![0.8, 0.8, 0.8]);
        out.materials.push(ImportedMaterial {
            name: m["name"].as_str().unwrap_or("Material").to_owned(),
            color: [c[0], c.get(1).copied().unwrap_or(0.8), c.get(2).copied().unwrap_or(0.8)],
            roughness: Some(pbr["roughnessFactor"].as_f64().unwrap_or(1.0) as f32),
            metalness: Some(pbr["metallicFactor"].as_f64().unwrap_or(1.0) as f32),
        });
        if pbr["baseColorTexture"].is_object() {
            out.note("textures were left out (base colours are kept)");
        }
    }
    let nodes = doc["nodes"].as_array().cloned().unwrap_or_default();
    let scene = doc["scene"].as_u64().unwrap_or(0) as usize;
    let roots: Vec<usize> = doc["scenes"][scene]["nodes"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_u64().map(|x| x as usize)).collect())
        .unwrap_or_else(|| (0..nodes.len()).collect());
    // Depth-first, parents before children.
    let mut stack: Vec<(usize, Option<usize>, u32)> = roots.into_iter().rev().map(|r| (r, None, 0)).collect();
    while let Some((ni, parent, depth)) = stack.pop() {
        let Some(n) = nodes.get(ni) else { continue };
        if depth > 64 {
            continue;
        }
        let local = node_matrix(n);
        let name = n["name"].as_str().unwrap_or("Node").to_owned();
        let me = out.nodes.len();
        out.nodes.push(Node { name: name.clone(), parent, local: mat_to_rh(local), mesh: None, material: None });
        if let Some(mi) = n["mesh"].as_u64() {
            let prims = doc["meshes"][mi as usize]["primitives"].as_array().cloned().unwrap_or_default();
            let mut parts = Vec::new();
            for p in &prims {
                if p["mode"].as_u64().unwrap_or(4) != 4 {
                    out.note("lines, points and triangle strips were left out");
                    continue;
                }
                let Some(pa) = p["attributes"]["POSITION"].as_u64() else { continue };
                let pos = accessor(pa as usize)?;
                let points: Vec<Vec3> = pos.chunks_exact(3).map(|c| Vec3::new(c[0], c[1], c[2])).collect();
                let idx: Vec<u32> = match p["indices"].as_u64() {
                    Some(ia) => accessor(ia as usize)?.into_iter().map(|x| x as u32).collect(),
                    None => (0..points.len() as u32).collect(),
                };
                let polys: Vec<Vec<u32>> = idx.chunks_exact(3).map(|t| t.to_vec()).collect();
                parts.push((mesh_from_rh(points, polys), p["material"].as_u64().map(|x| x as usize)));
            }
            if parts.len() == 1 {
                let (m, mat) = parts.pop().unwrap();
                out.nodes[me].mesh = Some(weld(m));
                out.nodes[me].material = mat;
            } else {
                for (k, (m, mat)) in parts.into_iter().enumerate() {
                    out.nodes.push(Node { name: format!("{name} {}", k + 1), parent: Some(me), local: Mat4::IDENTITY, mesh: Some(weld(m)), material: mat });
                }
            }
        }
        for c in n["children"].as_array().into_iter().flatten().rev() {
            if let Some(c) = c.as_u64() {
                stack.push((c as usize, Some(me), depth + 1));
            }
        }
    }
    if doc["animations"].as_array().is_some_and(|a| !a.is_empty()) || doc["skins"].as_array().is_some_and(|a| !a.is_empty()) {
        out.note("animation and skinning were left out");
    }
    Ok(out)
}

/// glTF splits points at hard edges; joining equal points makes the mesh
/// editable as one surface.
fn weld(m: crate::editmesh::EditMesh) -> crate::editmesh::EditMesh {
    let mut index = std::collections::HashMap::new();
    let mut points = Vec::new();
    let remap: Vec<u32> = m
        .points
        .iter()
        .map(|p| {
            *index.entry(p.to_array().map(f32::to_bits)).or_insert_with(|| {
                points.push(*p);
                points.len() as u32 - 1
            })
        })
        .collect();
    let polys = m.polys.iter().map(|p| p.iter().map(|&i| remap[i as usize]).collect()).collect();
    crate::editmesh::EditMesh { points, polys }
}

fn node_matrix(n: &Value) -> Mat4 {
    if let Some(m) = n["matrix"].as_array().filter(|a| a.len() == 16) {
        let v: Vec<f32> = m.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect();
        return Mat4::from_cols_slice(&v);
    }
    let f =
        |key: &str, d: &[f32]| -> Vec<f32> { n[key].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or(d.to_vec()) };
    let t = f("translation", &[0.0; 3]);
    let r = f("rotation", &[0.0, 0.0, 0.0, 1.0]);
    let s = f("scale", &[1.0; 3]);
    if t.len() < 3 || r.len() < 4 || s.len() < 3 {
        return Mat4::IDENTITY;
    }
    Mat4::from_scale_rotation_translation(Vec3::new(s[0], s[1], s[2]), Quat::from_xyzw(r[0], r[1], r[2], r[3]).normalize(), Vec3::new(t[0], t[1], t[2]))
}

/// An accessor's values as floats (indices as whole numbers).
fn read_accessor(doc: &Value, buffers: &[Vec<u8>], i: usize) -> Result<Vec<f32>, String> {
    let a = &doc["accessors"][i];
    let count = a["count"].as_u64().ok_or("accessor without a count")? as usize;
    let comps = match a["type"].as_str() {
        Some("SCALAR") => 1,
        Some("VEC2") => 2,
        Some("VEC3") => 3,
        Some("VEC4") => 4,
        _ => return Err("unsupported accessor type".to_owned()),
    };
    let ct = a["componentType"].as_u64().unwrap_or(0);
    let size = match ct {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        5125 | 5126 => 4,
        _ => return Err("unsupported component type".to_owned()),
    };
    if a["sparse"].is_object() {
        return Err("sparse accessors aren't supported".to_owned());
    }
    let Some(vi) = a["bufferView"].as_u64() else { return Ok(vec![0.0; count * comps]) };
    let v = &doc["bufferViews"][vi as usize];
    let buf = buffers.get(v["buffer"].as_u64().unwrap_or(0) as usize).ok_or("missing buffer")?;
    let start = v["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
    let stride = v["byteStride"].as_u64().map(|s| s as usize).unwrap_or(comps * size);
    let mut out = Vec::with_capacity(count * comps);
    for e in 0..count {
        for c in 0..comps {
            let o = start + e * stride + c * size;
            let b = buf.get(o..o + size).ok_or("accessor reaches past its buffer")?;
            out.push(match ct {
                5126 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                5125 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32,
                5123 => u16::from_le_bytes([b[0], b[1]]) as f32,
                5122 => i16::from_le_bytes([b[0], b[1]]) as f32,
                5121 => b[0] as f32,
                _ => b[0] as i8 as f32,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::{Format, testing};

    #[test]
    fn round_trips_with_transforms_and_materials() {
        let back = testing::round_trip(Format::Gltf);
        assert_eq!(back.mesh_count(), 2);
        assert_eq!(back.materials[0].name, "Red");
        assert!((back.materials[0].color[0] - 0.8).abs() < 1e-5);
        // The cube keeps its own position and turn.
        let cube = &back.nodes[0];
        let (_, rot, pos) = cube.local.to_scale_rotation_translation();
        assert!((pos - glam::Vec3::new(100.0, 50.0, -30.0)).length() < 0.01, "{pos}");
        assert!(rot.angle_between(crate::scene::hpb_to_quat(glam::Vec3::new(30.0, 0.0, 0.0))) < 1e-3);
    }
}
