//! Alembic (.abc, the Ogawa container that every current Alembic tool
//! writes). Written: one transform (Xform) per object holding its polygon
//! mesh (PolyMesh), right-handed, Y up; a single frame. Read: Xform,
//! PolyMesh and SubD objects at their first frame. Alembic has no
//! materials, so colours aren't kept. The old HDF5 container isn't read.
//!
//! Layout notes (from the format's public definition): an Ogawa file is a
//! 16-byte header then groups (a child count and child offsets) and data
//! blocks (a size and bytes); a data offset has its top bit set. The
//! archive's root group holds the file version, library version, the top
//! object, the archive's metadata, its time samplings and the shared
//! metadata strings. An object group holds its properties, its child
//! objects and its children's headers; a compound property group holds
//! its properties and their headers.

use super::{ExportItem, Imported, Node, identifier, mat_to_rh, mesh_from_rh, reversed, to_rh};
use glam::{DMat4, DVec3, Mat4, Vec3};
use std::collections::HashMap;

const DATA: u64 = 1 << 63;
const POD_BOOL: u32 = 0;
const POD_U8: u32 = 1;
const POD_I32: u32 = 6;
const POD_F32: u32 = 10;
const POD_F64: u32 = 11;

// --- writing ------------------------------------------------------------------

/// A 128-bit content key for a sample. Readers use these only to spot
/// repeated samples, so any well-mixed hash of the bytes does.
fn digest(bytes: &[u8]) -> [u8; 16] {
    let mut a: u64 = 0xcbf2_9ce4_8422_2325;
    let mut b: u64 = 0x9e37_79b9_7f4a_7c15 ^ bytes.len() as u64;
    for &x in bytes {
        a = (a ^ x as u64).wrapping_mul(0x0000_0100_0000_01b3);
        b = (b ^ x as u64).wrapping_mul(0xff51_afd7_ed55_8ccd).rotate_left(29);
    }
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&a.to_le_bytes());
    out[8..].copy_from_slice(&b.to_le_bytes());
    out
}

enum Prop {
    Compound { name: String, meta: String, props: Vec<Prop> },
    Scalar { name: String, meta: String, pod: u32, extent: u32, bytes: Vec<u8> },
    Array { name: String, meta: String, pod: u32, extent: u32, count: usize, bytes: Vec<u8> },
}

struct Obj {
    name: String,
    meta: String,
    props: Vec<Prop>,
    children: Vec<Obj>,
}

struct Writer {
    out: Vec<u8>,
    meta: Vec<String>,
}

impl Writer {
    fn data(&mut self, bytes: &[u8]) -> u64 {
        if bytes.is_empty() {
            return DATA;
        }
        let at = self.out.len() as u64;
        self.out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        self.out.extend_from_slice(bytes);
        at | DATA
    }

    fn group(&mut self, children: &[u64]) -> u64 {
        if children.is_empty() {
            return 0;
        }
        let at = self.out.len() as u64;
        self.out.extend_from_slice(&(children.len() as u64).to_le_bytes());
        for c in children {
            self.out.extend_from_slice(&c.to_le_bytes());
        }
        at
    }

    fn sample(&mut self, bytes: &[u8]) -> u64 {
        if bytes.is_empty() {
            return DATA;
        }
        let mut d = digest(bytes).to_vec();
        d.extend_from_slice(bytes);
        self.data(&d)
    }

    fn meta_index(&mut self, s: &str) -> u32 {
        if s.is_empty() {
            return 0;
        }
        if s.len() < 256 {
            if let Some(i) = self.meta.iter().position(|m| m == s) {
                return i as u32 + 1;
            }
            if self.meta.len() < 254 {
                self.meta.push(s.to_owned());
                return self.meta.len() as u32;
            }
        }
        255
    }

    fn compound(&mut self, props: &[Prop]) -> u64 {
        let mut kids = Vec::new();
        let mut headers = Vec::new();
        for p in props {
            let (name, meta) = match p {
                Prop::Compound { name, meta, props } => {
                    kids.push(self.compound(props));
                    (name, meta)
                }
                Prop::Scalar { name, meta, bytes, .. } => {
                    let d = self.sample(bytes);
                    kids.push(self.group(&[d]));
                    (name, meta)
                }
                Prop::Array { name, meta, bytes, .. } => {
                    let d = self.sample(bytes);
                    kids.push(self.group(&[d, DATA]));
                    (name, meta)
                }
            };
            let md = self.meta_index(meta);
            let max = name.len().max(meta.len()).max(1);
            let hint: u32 = if max < 256 {
                0
            } else if max < 65536 {
                1
            } else {
                2
            };
            let push = |h: &mut Vec<u8>, v: u32, hint: u32| match hint {
                0 => h.push(v as u8),
                1 => h.extend_from_slice(&(v as u16).to_le_bytes()),
                _ => h.extend_from_slice(&v.to_le_bytes()),
            };
            let mut info = (hint << 2) | (md << 20);
            match p {
                Prop::Compound { .. } => headers.extend_from_slice(&info.to_le_bytes()),
                Prop::Scalar { pod, extent, .. } => {
                    info |= 1 | (pod << 4) | 0x800 | 0x400 | (extent << 12);
                    headers.extend_from_slice(&info.to_le_bytes());
                    push(&mut headers, 1, hint);
                }
                Prop::Array { pod, extent, count, .. } => {
                    info |= 2 | u32::from(*count == 1) | (pod << 4) | 0x800 | (extent << 12);
                    headers.extend_from_slice(&info.to_le_bytes());
                    push(&mut headers, 1, hint);
                }
            }
            push(&mut headers, name.len() as u32, hint);
            headers.extend_from_slice(name.as_bytes());
            if md == 255 {
                push(&mut headers, meta.len() as u32, hint);
                headers.extend_from_slice(meta.as_bytes());
            }
        }
        if !headers.is_empty() {
            let h = self.data(&headers);
            kids.push(h);
        }
        self.group(&kids)
    }

    fn object(&mut self, o: &Obj) -> u64 {
        let props = self.compound(&o.props);
        let mut kids = vec![props];
        let mut headers = Vec::new();
        for c in &o.children {
            kids.push(self.object(c));
            headers.extend_from_slice(&(c.name.len() as u32).to_le_bytes());
            headers.extend_from_slice(c.name.as_bytes());
            let md = self.meta_index(&c.meta);
            headers.push(md as u8);
            if md == 255 {
                headers.extend_from_slice(&(c.meta.len() as u32).to_le_bytes());
                headers.extend_from_slice(c.meta.as_bytes());
            }
        }
        let hash = digest(&headers);
        headers.extend_from_slice(&hash);
        headers.extend_from_slice(&[0u8; 16]);
        let h = self.data(&headers);
        kids.push(h);
        self.group(&kids)
    }
}

fn scalar(name: &str, meta: &str, pod: u32, extent: u32, bytes: Vec<u8>) -> Prop {
    Prop::Scalar { name: name.to_owned(), meta: meta.to_owned(), pod, extent, bytes }
}

fn f64s(v: &[f64]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

pub fn write(items: &[ExportItem], scale: f32) -> Vec<u8> {
    let mut used: HashMap<String, usize> = HashMap::new();
    let mut top = Obj { name: "ABC".to_owned(), meta: String::new(), props: Vec::new(), children: Vec::new() };
    for it in items {
        let base = identifier(&it.name);
        let k = used.entry(base.clone()).or_insert(0);
        *k += 1;
        let name = if *k == 1 { base } else { format!("{base}_{k}") };
        let mut m = mat_to_rh(it.world);
        m.w_axis = (m.w_axis.truncate() * scale).extend(1.0);
        let flip = it.world.determinant() < 0.0;
        // Alembic faces run clockwise: Forma3D's own order, mirrored.
        let polys: Vec<Vec<u32>> = it.mesh.polys.iter().map(|p| if flip { reversed(p) } else { p.clone() }).collect();
        let pts: Vec<Vec3> = it.mesh.points.iter().map(|p| to_rh(*p) * scale).collect();
        let (lo, hi) = pts.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(a, b), p| (a.min(*p), b.max(*p)));
        let (lo, hi) = if pts.is_empty() { (Vec3::ZERO, Vec3::ZERO) } else { (lo, hi) };
        let counts: Vec<u8> = polys.iter().flat_map(|p| (p.len() as i32).to_le_bytes()).collect();
        let idx: Vec<u8> = polys.iter().flatten().flat_map(|i| (*i as i32).to_le_bytes()).collect();
        let pbytes: Vec<u8> = pts.iter().flat_map(|p| p.to_array()).flat_map(|f| f.to_le_bytes()).collect();
        let geom = Prop::Compound {
            name: ".geom".to_owned(),
            meta: "schema=AbcGeom_PolyMesh_v1;schemaBaseType=AbcGeom_GeomBase_v1".to_owned(),
            props: vec![
                scalar(".selfBnds", "interpretation=box", POD_F64, 6, f64s(&[lo.x as f64, lo.y as f64, lo.z as f64, hi.x as f64, hi.y as f64, hi.z as f64])),
                Prop::Array {
                    name: "P".to_owned(),
                    meta: "geoScope=vtx;interpretation=point".to_owned(),
                    pod: POD_F32,
                    extent: 3,
                    count: pts.len(),
                    bytes: pbytes,
                },
                Prop::Array { name: ".faceIndices".to_owned(), meta: String::new(), pod: POD_I32, extent: 1, count: idx.len() / 4, bytes: idx },
                Prop::Array { name: ".faceCounts".to_owned(), meta: String::new(), pod: POD_I32, extent: 1, count: polys.len(), bytes: counts },
            ],
        };
        let shape = Obj {
            name: format!("{name}Shape"),
            meta: "schema=AbcGeom_PolyMesh_v1;schemaBaseType=AbcGeom_GeomBase_v1;schemaObjTitle=AbcGeom_PolyMesh_v1:.geom".to_owned(),
            props: vec![geom],
            children: Vec::new(),
        };
        // Imath matrices are row-vector: their rows are glam's columns.
        let vals: Vec<f64> = m.to_cols_array().iter().map(|v| *v as f64).collect();
        let xform = Prop::Compound {
            name: ".xform".to_owned(),
            meta: "schema=AbcGeom_Xform_v3".to_owned(),
            props: vec![
                scalar(".inherits", "", POD_BOOL, 1, vec![1]),
                scalar(".ops", "", POD_U8, 1, vec![0x30]),
                scalar(".vals", "", POD_F64, 16, f64s(&vals)),
                scalar("isNotConstantIdentity", "", POD_BOOL, 1, vec![1]),
            ],
        };
        top.children.push(Obj {
            name,
            meta: "schema=AbcGeom_Xform_v3;schemaObjTitle=AbcGeom_Xform_v3:.xform".to_owned(),
            props: vec![xform],
            children: vec![shape],
        });
    }
    let mut w = Writer { out: b"Ogawa\xff\x00\x01\0\0\0\0\0\0\0\0".to_vec(), meta: Vec::new() };
    let version = w.data(&0i32.to_le_bytes());
    let library = w.data(&10803i32.to_le_bytes());
    let top_at = w.object(&top);
    let archive_meta = w.data(b"_ai_Application=Forma3D;_ai_AlembicVersion=Alembic 1.8.3 (written by Forma3D);_ai_Description=Forma3D scene");
    let mut ts = Vec::new();
    ts.extend_from_slice(&1u32.to_le_bytes());
    ts.extend_from_slice(&1.0f64.to_le_bytes());
    ts.extend_from_slice(&1u32.to_le_bytes());
    ts.extend_from_slice(&0.0f64.to_le_bytes());
    let times = w.data(&ts);
    let mut md = Vec::new();
    for m in &w.meta {
        md.push(m.len() as u8);
        md.extend_from_slice(m.as_bytes());
    }
    let meta = w.data(&md);
    let root = w.group(&[version, library, top_at, archive_meta, times, meta]);
    w.out[8..16].copy_from_slice(&root.to_le_bytes());
    w.out
}

// --- reading ------------------------------------------------------------------

struct Og<'a> {
    b: &'a [u8],
}

impl<'a> Og<'a> {
    fn u64_at(&self, at: usize) -> Result<u64, String> {
        self.b.get(at..at + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap())).ok_or_else(|| "the Alembic file is cut short".to_owned())
    }

    fn group(&self, off: u64) -> Result<Vec<u64>, String> {
        if off & DATA != 0 {
            return Err("expected a group, found data".to_owned());
        }
        if off == 0 {
            return Ok(Vec::new());
        }
        let at = off as usize;
        let n = self.u64_at(at)? as usize;
        if n > self.b.len() / 8 {
            return Err("a damaged Alembic group".to_owned());
        }
        (0..n).map(|i| self.u64_at(at + 8 + i * 8)).collect()
    }

    fn data(&self, off: u64) -> Result<&'a [u8], String> {
        if off & DATA == 0 {
            return Err("expected data, found a group".to_owned());
        }
        let at = (off & !DATA) as usize;
        if at == 0 {
            return Ok(&[]);
        }
        let n = self.u64_at(at)? as usize;
        self.b.get(at + 8..(at + 8).checked_add(n).ok_or("a damaged Alembic block")?).ok_or_else(|| "the Alembic file is cut short".to_owned())
    }
}

struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}

impl Cursor<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let s = self.b.get(self.i..self.i + n).ok_or("a damaged Alembic header")?;
        self.i += n;
        Ok(s)
    }

    fn uint(&mut self, hint: u32) -> Result<u32, String> {
        Ok(match hint {
            0 => self.take(1)?[0] as u32,
            1 => u16::from_le_bytes(self.take(2)?.try_into().unwrap()) as u32,
            _ => u32::from_le_bytes(self.take(4)?.try_into().unwrap()),
        })
    }

    fn text(&mut self, n: usize) -> Result<String, String> {
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
}

#[derive(Debug)]
struct PropHeader {
    name: String,
    meta: String,
    kind: u32, // 0 compound, 1 scalar, 2 array
    pod: u32,
    extent: u32,
    samples: u32,
}

#[derive(Clone, Copy)]
struct ObjRef {
    group: u64,
}

struct Reader<'a> {
    og: Og<'a>,
    meta: Vec<String>,
    notes: Vec<String>,
}

impl<'a> Reader<'a> {
    fn meta(&self, i: u32) -> String {
        if i == 0 { String::new() } else { self.meta.get(i as usize - 1).cloned().unwrap_or_default() }
    }

    /// A compound property group: its properties with their groups.
    fn props(&self, group: u64) -> Result<Vec<(PropHeader, u64)>, String> {
        let kids = self.og.group(group)?;
        let Some((&last, props)) = kids.split_last() else { return Ok(Vec::new()) };
        let mut c = Cursor { b: self.og.data(last)?, i: 0 };
        let mut out = Vec::new();
        for &g in props {
            let info = c.uint(2)?;
            let hint = (info >> 2) & 3;
            let kind = match info & 3 {
                0 => 0,
                1 => 1,
                _ => 2,
            };
            let (mut pod, mut extent, mut samples) = (0, 0, 0);
            if kind != 0 {
                pod = (info >> 4) & 0xf;
                extent = (info >> 12) & 0xff;
                samples = c.uint(hint)?;
                if info & 0x200 != 0 {
                    c.uint(hint)?;
                    c.uint(hint)?;
                }
                if info & 0x100 != 0 {
                    c.uint(hint)?;
                }
            }
            let n = c.uint(hint)? as usize;
            let name = c.text(n)?;
            let md = (info >> 20) & 0xff;
            let meta = if md == 0xff {
                let n = c.uint(hint)? as usize;
                c.text(n)?
            } else {
                self.meta(md)
            };
            out.push((PropHeader { name, meta, kind, pod, extent, samples }, g));
        }
        Ok(out)
    }

    /// The first sample's bytes (without its key).
    fn first_sample(&self, h: &PropHeader, group: u64) -> Result<&'a [u8], String> {
        if h.samples == 0 {
            return Ok(&[]);
        }
        let kids = self.og.group(group)?;
        let Some(&d) = kids.first() else { return Ok(&[]) };
        let d = self.og.data(d)?;
        Ok(if d.len() >= 16 { &d[16..] } else { &[] })
    }

    fn find<'p>(&self, props: &'p [(PropHeader, u64)], name: &str) -> Option<&'p (PropHeader, u64)> {
        props.iter().find(|(h, _)| h.name == name)
    }

    fn numbers(&self, props: &[(PropHeader, u64)], name: &str) -> Result<Vec<f64>, String> {
        let Some((h, g)) = self.find(props, name) else { return Ok(Vec::new()) };
        let b = self.first_sample(h, *g)?;
        Ok(match h.pod {
            POD_BOOL | POD_U8 => b.iter().map(|x| *x as f64).collect(),
            2 => b.iter().map(|x| *x as i8 as f64).collect(),
            3 => b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]) as f64).collect(),
            4 => b.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f64).collect(),
            5 => b.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap()) as f64).collect(),
            POD_I32 => b.chunks_exact(4).map(|c| i32::from_le_bytes(c.try_into().unwrap()) as f64).collect(),
            7 => b.chunks_exact(8).map(|c| u64::from_le_bytes(c.try_into().unwrap()) as f64).collect(),
            8 => b.chunks_exact(8).map(|c| i64::from_le_bytes(c.try_into().unwrap()) as f64).collect(),
            POD_F32 => b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap()) as f64).collect(),
            POD_F64 => b.chunks_exact(8).map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect(),
            _ => Vec::new(),
        })
    }

    fn xform(&mut self, props: &[(PropHeader, u64)]) -> Result<DMat4, String> {
        let Some((_, g)) = self.find(props, ".xform") else { return Ok(DMat4::IDENTITY) };
        let xp = self.props(*g)?;
        let ops = self.numbers(&xp, ".ops")?;
        let vals = self.numbers(&xp, ".vals")?;
        if self.find(&xp, ".inherits").is_some() && self.numbers(&xp, ".inherits")?.first() == Some(&0.0) {
            self.notes.push("an object that ignores its parent's transform was placed under it anyway".to_owned());
        }
        if let Some((h, _)) = self.find(&xp, ".vals")
            && h.samples > 1
        {
            self.notes.push("animated transforms were read at their first frame".to_owned());
        }
        let mut m = DMat4::IDENTITY;
        let mut k = 0;
        for op in ops {
            let kind = op as u32 >> 4;
            let n = match kind {
                0 | 1 => 3,
                2 => 4,
                3 => 16,
                4..=6 => 1,
                _ => return Err(format!("an unknown transform operation ({kind})")),
            };
            let v = |i: usize| vals.get(k + i).copied();
            let t = match kind {
                0 => DMat4::from_scale(DVec3::new(v(0).unwrap_or(1.0), v(1).unwrap_or(1.0), v(2).unwrap_or(1.0))),
                1 => DMat4::from_translation(DVec3::new(v(0).unwrap_or(0.0), v(1).unwrap_or(0.0), v(2).unwrap_or(0.0))),
                2 => {
                    let axis = DVec3::new(v(0).unwrap_or(0.0), v(1).unwrap_or(0.0), v(2).unwrap_or(1.0));
                    if axis.length_squared() > 0.0 { DMat4::from_axis_angle(axis.normalize(), v(3).unwrap_or(0.0).to_radians()) } else { DMat4::IDENTITY }
                }
                3 => {
                    if k + 16 <= vals.len() {
                        DMat4::from_cols_slice(&vals[k..k + 16])
                    } else {
                        DMat4::IDENTITY
                    }
                }
                4 => DMat4::from_rotation_x(v(0).unwrap_or(0.0).to_radians()),
                5 => DMat4::from_rotation_y(v(0).unwrap_or(0.0).to_radians()),
                _ => DMat4::from_rotation_z(v(0).unwrap_or(0.0).to_radians()),
            };
            m *= t;
            k += n;
        }
        Ok(m)
    }

    fn mesh(&mut self, props: &[(PropHeader, u64)]) -> Result<Option<super::EditMesh>, String> {
        let Some((_, g)) = self.find(props, ".geom") else { return Ok(None) };
        let gp = self.props(*g)?;
        if let Some((h, _)) = self.find(&gp, "P")
            && h.samples > 1
        {
            self.notes.push("animated meshes were read at their first frame".to_owned());
        }
        let p = self.numbers(&gp, "P")?;
        let idx = self.numbers(&gp, ".faceIndices")?;
        let counts = self.numbers(&gp, ".faceCounts")?;
        let pts: Vec<Vec3> = p.chunks_exact(3).map(|c| Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32)).collect();
        let mut polys = Vec::with_capacity(counts.len());
        let mut k = 0usize;
        for n in counts {
            let n = n.max(0.0) as usize;
            if k + n > idx.len() {
                break;
            }
            // Clockwise in the file: reverse to the usual right-handed order.
            let poly: Vec<u32> = idx[k..k + n].iter().rev().map(|i| i.max(0.0) as u32).collect();
            polys.push(poly);
            k += n;
        }
        Ok(Some(mesh_from_rh(pts, polys)))
    }

    fn walk(&mut self, obj: ObjRef, parent: Option<usize>, out: &mut Imported, depth: u32) -> Result<(), String> {
        if depth > 256 {
            return Err("the Alembic file nests too deeply".to_owned());
        }
        let kids = self.og.group(obj.group)?;
        if kids.len() < 2 {
            return Ok(());
        }
        let headers = self.og.data(*kids.last().unwrap())?;
        let mut c = Cursor { b: headers, i: 0 };
        let end = headers.len().saturating_sub(32);
        for &child in &kids[1..kids.len() - 1] {
            if c.i >= end {
                break;
            }
            let n = c.uint(2)? as usize;
            let name = c.text(n)?;
            let md = c.uint(0)?;
            let meta = if md == 0xff {
                let n = c.uint(2)? as usize;
                c.text(n)?
            } else {
                self.meta(md)
            };
            let schema = meta.split(';').find_map(|kv| kv.strip_prefix("schema=")).unwrap_or("").to_owned();
            let child_kids = self.og.group(child)?;
            let props = match child_kids.first() {
                Some(&g) => self.props(g)?,
                None => Vec::new(),
            };
            let me = out.nodes.len();
            if schema.starts_with("AbcGeom_Xform") {
                let m = self.xform(&props)?;
                out.nodes.push(Node { name, parent, local: mat_to_rh(m.as_mat4()), mesh: None, material: None });
                self.walk(ObjRef { group: child }, Some(me), out, depth + 1)?;
                // A transform holding just one mesh becomes that one object.
                let below: Vec<usize> = (me + 1..out.nodes.len()).filter(|&k| out.nodes[k].parent == Some(me)).collect();
                if below.len() == 1 && below[0] == out.nodes.len() - 1 && out.nodes[below[0]].local == Mat4::IDENTITY && out.nodes[me].mesh.is_none() {
                    let only = out.nodes.pop().unwrap();
                    out.nodes[me].mesh = only.mesh;
                }
            } else if schema.starts_with("AbcGeom_PolyMesh") || schema.starts_with("AbcGeom_SubD") {
                let mesh = self.mesh(&props)?;
                out.nodes.push(Node { name, parent, local: Mat4::IDENTITY, mesh, material: None });
                self.walk(ObjRef { group: child }, Some(me), out, depth + 1)?;
            } else if schema.starts_with("AbcGeom_FaceSet") {
                // Face groups (often material names): not kept.
            } else {
                let what = match schema.as_str() {
                    s if s.starts_with("AbcGeom_Camera") => "cameras",
                    s if s.starts_with("AbcGeom_Curve") => "curves",
                    s if s.starts_with("AbcGeom_Points") => "particles",
                    s if s.starts_with("AbcGeom_NuPatch") => "NURBS patches",
                    s if s.starts_with("AbcGeom_Light") => "lights",
                    _ => "objects of other kinds",
                };
                self.notes.push(format!("{what} were left out"));
                // Their children may still hold meshes.
                out.nodes.push(Node { name, parent, local: Mat4::IDENTITY, mesh: None, material: None });
                self.walk(ObjRef { group: child }, Some(me), out, depth + 1)?;
            }
        }
        Ok(())
    }
}

pub fn read(bytes: &[u8]) -> Result<Imported, String> {
    if bytes.len() >= 8 && &bytes[..8] == b"\x89HDF\r\n\x1a\n" {
        return Err("this Alembic file uses the old HDF5 container; save it again as Ogawa".to_owned());
    }
    if bytes.len() < 16 || &bytes[..5] != b"Ogawa" {
        return Err("not an Alembic file".to_owned());
    }
    let og = Og { b: bytes };
    let root = og.group(og.u64_at(8)?)?;
    if root.len() < 6 {
        return Err("a damaged Alembic file".to_owned());
    }
    let md = og.data(root[5])?;
    let mut meta = Vec::new();
    let mut i = 0;
    while i < md.len() {
        let n = md[i] as usize;
        meta.push(String::from_utf8_lossy(md.get(i + 1..i + 1 + n).ok_or("a damaged Alembic file")?).into_owned());
        i += 1 + n;
    }
    let mut r = Reader { og, meta, notes: Vec::new() };
    let mut out = Imported::default();
    r.walk(ObjRef { group: root[2] }, None, &mut out, 0)?;
    for n in std::mem::take(&mut r.notes) {
        out.note(n);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::{Format, testing};
    use super::*;

    #[test]
    fn round_trips() {
        let back = testing::round_trip(Format::Alembic);
        assert_eq!(back.mesh_count(), 2);
        // Each transform with its one mesh comes back as one object.
        assert_eq!(back.nodes.len(), 2);
    }

    #[test]
    fn rejects_other_files() {
        assert!(read(b"\x89HDF\r\n\x1a\n........").is_err());
        assert!(read(b"hello").is_err());
    }
}
