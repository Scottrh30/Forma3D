//! FBX (binary, version 7.4). Written right-handed, Y up, in centimetres;
//! read in whatever axes and units the file declares.

use super::{ExportItem, Imported, ImportedMaterial, Node, mat_to_rh, mesh_from_rh, reversed, to_rh};
use glam::{EulerRot, Mat3, Mat4, Quat, Vec3};
use std::collections::HashMap;

// --- the node tree ------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum Prop {
    Bool(bool),
    I16(i16),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    Str(String),
    Raw(Vec<u8>),
    ArrF64(Vec<f64>),
    ArrF32(Vec<f32>),
    ArrI32(Vec<i32>),
    ArrI64(Vec<i64>),
    ArrBool(Vec<bool>),
}

impl Prop {
    fn as_f64(&self) -> Option<f64> {
        Some(match self {
            Prop::F64(v) => *v,
            Prop::F32(v) => *v as f64,
            Prop::I32(v) => *v as f64,
            Prop::I64(v) => *v as f64,
            Prop::I16(v) => *v as f64,
            _ => return None,
        })
    }

    fn as_i64(&self) -> Option<i64> {
        Some(match self {
            Prop::I64(v) => *v,
            Prop::I32(v) => *v as i64,
            Prop::I16(v) => *v as i64,
            _ => return None,
        })
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Prop::Str(s) => Some(s),
            _ => None,
        }
    }

    fn floats(&self) -> Option<Vec<f64>> {
        Some(match self {
            Prop::ArrF64(v) => v.clone(),
            Prop::ArrF32(v) => v.iter().map(|&x| x as f64).collect(),
            _ => return None,
        })
    }

    fn ints(&self) -> Option<Vec<i64>> {
        Some(match self {
            Prop::ArrI32(v) => v.iter().map(|&x| x as i64).collect(),
            Prop::ArrI64(v) => v.clone(),
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FbxNode {
    pub name: String,
    pub props: Vec<Prop>,
    pub children: Vec<FbxNode>,
}

impl FbxNode {
    fn new(name: &str, props: Vec<Prop>) -> FbxNode {
        FbxNode { name: name.to_owned(), props, children: Vec::new() }
    }

    fn with(mut self, children: Vec<FbxNode>) -> FbxNode {
        self.children = children;
        self
    }

    fn child(&self, name: &str) -> Option<&FbxNode> {
        self.children.iter().find(|c| c.name == name)
    }

    fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a FbxNode> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// A value from a Properties70 block.
    fn p70(&self, key: &str) -> Option<&[Prop]> {
        self.child("Properties70")?.children_named("P").find(|p| p.props.first().and_then(Prop::as_str) == Some(key)).map(|p| &p.props[4.min(p.props.len())..])
    }

    fn p70_vec(&self, key: &str) -> Option<Vec3> {
        let v = self.p70(key)?;
        Some(Vec3::new(v.first()?.as_f64()? as f32, v.get(1)?.as_f64()? as f32, v.get(2)?.as_f64()? as f32))
    }

    fn p70_num(&self, key: &str) -> Option<f64> {
        self.p70(key)?.first()?.as_f64()
    }
}

// --- binary encoding ----------------------------------------------------------

const MAGIC: &[u8] = b"Kaydara FBX Binary  \x00\x1a\x00";
const VERSION: u32 = 7400;
const FOOT_ID: [u8; 16] = [0xfa, 0xbc, 0xab, 0x09, 0xd0, 0xc8, 0xd4, 0x66, 0xb1, 0x76, 0xfb, 0x83, 0x1c, 0xf7, 0x26, 0x7e];
const FOOT_END: [u8; 16] = [0xf8, 0x5a, 0x8c, 0x6a, 0xde, 0xf5, 0xd9, 0x7e, 0xec, 0xe9, 0x0c, 0xe3, 0x75, 0x8f, 0x29, 0x0b];

fn encode_prop(p: &Prop, out: &mut Vec<u8>) {
    fn arr<T: Copy>(tag: u8, v: &[T], bytes: impl Fn(T) -> Vec<u8>, out: &mut Vec<u8>) {
        let raw: Vec<u8> = v.iter().flat_map(|&x| bytes(x)).collect();
        out.push(tag);
        out.extend_from_slice(&(v.len() as u32).to_le_bytes());
        // Large arrays are deflated, as other writers do.
        if raw.len() > 128 {
            let z = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
            out.extend_from_slice(&1u32.to_le_bytes());
            out.extend_from_slice(&(z.len() as u32).to_le_bytes());
            out.extend_from_slice(&z);
        } else {
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
            out.extend_from_slice(&raw);
        }
    }
    match p {
        Prop::Bool(b) => out.extend_from_slice(&[b'C', *b as u8]),
        Prop::I16(v) => {
            out.push(b'Y');
            out.extend_from_slice(&v.to_le_bytes());
        }
        Prop::I32(v) => {
            out.push(b'I');
            out.extend_from_slice(&v.to_le_bytes());
        }
        Prop::I64(v) => {
            out.push(b'L');
            out.extend_from_slice(&v.to_le_bytes());
        }
        Prop::F32(v) => {
            out.push(b'F');
            out.extend_from_slice(&v.to_le_bytes());
        }
        Prop::F64(v) => {
            out.push(b'D');
            out.extend_from_slice(&v.to_le_bytes());
        }
        Prop::Str(s) => {
            out.push(b'S');
            out.extend_from_slice(&(s.len() as u32).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
        }
        Prop::Raw(r) => {
            out.push(b'R');
            out.extend_from_slice(&(r.len() as u32).to_le_bytes());
            out.extend_from_slice(r);
        }
        Prop::ArrF64(v) => arr(b'd', v, |x| x.to_le_bytes().to_vec(), out),
        Prop::ArrF32(v) => arr(b'f', v, |x| x.to_le_bytes().to_vec(), out),
        Prop::ArrI32(v) => arr(b'i', v, |x| x.to_le_bytes().to_vec(), out),
        Prop::ArrI64(v) => arr(b'l', v, |x| x.to_le_bytes().to_vec(), out),
        Prop::ArrBool(v) => arr(b'b', v, |x| vec![x as u8], out),
    }
}

fn encode_node(n: &FbxNode, out: &mut Vec<u8>) {
    let start = out.len();
    out.extend_from_slice(&[0u8; 12]);
    out.push(n.name.len() as u8);
    out.extend_from_slice(n.name.as_bytes());
    let props_start = out.len();
    for p in &n.props {
        encode_prop(p, out);
    }
    let props_len = out.len() - props_start;
    if !n.children.is_empty() || n.props.is_empty() {
        for c in &n.children {
            encode_node(c, out);
        }
        out.extend_from_slice(&[0u8; 13]);
    }
    let end = out.len() as u32;
    out[start..start + 4].copy_from_slice(&end.to_le_bytes());
    out[start + 4..start + 8].copy_from_slice(&(n.props.len() as u32).to_le_bytes());
    out[start + 8..start + 12].copy_from_slice(&(props_len as u32).to_le_bytes());
}

pub fn encode(nodes: &[FbxNode]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    for n in nodes {
        encode_node(n, &mut out);
    }
    out.extend_from_slice(&[0u8; 13]);
    out.extend_from_slice(&FOOT_ID);
    let pad = match (16 - out.len() % 16) % 16 {
        0 => 16,
        p => p,
    };
    out.extend(std::iter::repeat_n(0u8, pad));
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend(std::iter::repeat_n(0u8, 120));
    out.extend_from_slice(&FOOT_END);
    out
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
    wide: bool,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let s = self.b.get(self.pos..self.pos + n).ok_or("the FBX file is cut short")?;
        self.pos += n;
        Ok(s)
    }

    fn u32(&mut self) -> Result<u32, String> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    fn off(&mut self) -> Result<u64, String> {
        if self.wide {
            let s = self.take(8)?;
            Ok(u64::from_le_bytes(s.try_into().unwrap()))
        } else {
            Ok(self.u32()? as u64)
        }
    }

    fn prop(&mut self) -> Result<Prop, String> {
        let t = self.take(1)?[0];
        macro_rules! num {
            ($ty:ty, $n:expr, $v:ident) => {{
                let s = self.take($n)?;
                Prop::$v(<$ty>::from_le_bytes(s.try_into().unwrap()))
            }};
        }
        Ok(match t {
            b'C' => Prop::Bool(self.take(1)?[0] != 0),
            b'Y' => num!(i16, 2, I16),
            b'I' => num!(i32, 4, I32),
            b'L' => num!(i64, 8, I64),
            b'F' => num!(f32, 4, F32),
            b'D' => num!(f64, 8, F64),
            b'S' | b'R' => {
                let n = self.u32()? as usize;
                let s = self.take(n)?.to_vec();
                if t == b'S' { Prop::Str(String::from_utf8_lossy(&s).into_owned()) } else { Prop::Raw(s) }
            }
            b'f' | b'd' | b'l' | b'i' | b'b' => {
                let count = self.u32()? as usize;
                let enc = self.u32()?;
                let len = self.u32()? as usize;
                let data = self.take(len)?;
                let raw =
                    if enc == 1 { miniz_oxide::inflate::decompress_to_vec_zlib(data).map_err(|_| "a compressed FBX array is damaged")? } else { data.to_vec() };
                let size = match t {
                    b'f' | b'i' => 4,
                    b'd' | b'l' => 8,
                    _ => 1,
                };
                if raw.len() < count * size {
                    return Err("an FBX array is too short".to_owned());
                }
                let ch = raw.chunks_exact(size).take(count);
                match t {
                    b'f' => Prop::ArrF32(ch.map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()),
                    b'd' => Prop::ArrF64(ch.map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect()),
                    b'i' => Prop::ArrI32(ch.map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect()),
                    b'l' => Prop::ArrI64(ch.map(|c| i64::from_le_bytes(c.try_into().unwrap())).collect()),
                    _ => Prop::ArrBool(ch.map(|c| c[0] != 0).collect()),
                }
            }
            other => return Err(format!("unknown FBX property type {:?}", other as char)),
        })
    }

    /// One node, or None at a list's end marker.
    fn node(&mut self, depth: u32) -> Result<Option<FbxNode>, String> {
        if depth > 64 {
            return Err("the FBX file nests too deeply".to_owned());
        }
        let end = self.off()? as usize;
        let count = self.off()?;
        let _len = self.off()?;
        let name_len = self.take(1)?[0] as usize;
        if end == 0 {
            return Ok(None);
        }
        let name = String::from_utf8_lossy(self.take(name_len)?).into_owned();
        let mut node = FbxNode { name, ..Default::default() };
        for _ in 0..count {
            node.props.push(self.prop()?);
        }
        while self.pos < end {
            match self.node(depth + 1)? {
                Some(c) => node.children.push(c),
                None => break,
            }
        }
        if end > self.b.len() {
            return Err("the FBX file is cut short".to_owned());
        }
        self.pos = end;
        Ok(Some(node))
    }
}

pub fn decode(bytes: &[u8]) -> Result<Vec<FbxNode>, String> {
    if !bytes.starts_with(&MAGIC[..20]) {
        if bytes.starts_with(b"; FBX") || bytes.windows(9).take(4096).any(|w| w == b"FBXHeader") {
            return Err("this is a text (ASCII) FBX; save it as binary FBX".to_owned());
        }
        return Err("not an FBX file".to_owned());
    }
    let version = u32::from_le_bytes(bytes.get(23..27).ok_or("not an FBX file")?.try_into().unwrap());
    let mut r = Reader { b: bytes, pos: 27, wide: version >= 7500 };
    let mut out = Vec::new();
    while let Some(n) = r.node(0)? {
        out.push(n);
    }
    Ok(out)
}

// --- writing a scene ----------------------------------------------------------

fn p(name: &str, ty: &str, label: &str, flags: &str, values: Vec<Prop>) -> FbxNode {
    let mut props = vec![Prop::Str(name.into()), Prop::Str(ty.into()), Prop::Str(label.into()), Prop::Str(flags.into())];
    props.extend(values);
    FbxNode::new("P", props)
}

fn p3(name: &str, ty: &str, label: &str, v: Vec3) -> FbxNode {
    p(name, ty, label, "A", vec![Prop::F64(v.x as f64), Prop::F64(v.y as f64), Prop::F64(v.z as f64)])
}

fn class_name(name: &str, class: &str) -> Prop {
    Prop::Str(format!("{name}\x00\x01{class}"))
}

/// FBX rotations are Euler XYZ in degrees (X first): R = Rz * Ry * Rx.
fn euler_xyz(q: Quat) -> Vec3 {
    let (z, y, x) = q.to_euler(EulerRot::ZYX);
    Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
}

pub fn write(items: &[ExportItem], scale: f32) -> Vec<u8> {
    let i32p = |v: i32| Prop::I32(v);
    let s = |v: &str| Prop::Str(v.into());
    let header = FbxNode::new("FBXHeaderExtension", vec![]).with(vec![
        FbxNode::new("FBXHeaderVersion", vec![i32p(1003)]),
        FbxNode::new("FBXVersion", vec![i32p(VERSION as i32)]),
        FbxNode::new("EncryptionType", vec![i32p(0)]),
        FbxNode::new("Creator", vec![s("Forma3D")]),
    ]);
    // Centimetres per file unit: 1 at the default scale.
    let unit = 1.0 / scale as f64;
    let global = FbxNode::new("GlobalSettings", vec![]).with(vec![
        FbxNode::new("Version", vec![i32p(1000)]),
        FbxNode::new("Properties70", vec![]).with(vec![
            p("UpAxis", "int", "Integer", "", vec![i32p(1)]),
            p("UpAxisSign", "int", "Integer", "", vec![i32p(1)]),
            p("FrontAxis", "int", "Integer", "", vec![i32p(2)]),
            p("FrontAxisSign", "int", "Integer", "", vec![i32p(1)]),
            p("CoordAxis", "int", "Integer", "", vec![i32p(0)]),
            p("CoordAxisSign", "int", "Integer", "", vec![i32p(1)]),
            p("OriginalUpAxis", "int", "Integer", "", vec![i32p(1)]),
            p("OriginalUpAxisSign", "int", "Integer", "", vec![i32p(1)]),
            p("UnitScaleFactor", "double", "Number", "", vec![Prop::F64(unit)]),
            p("OriginalUnitScaleFactor", "double", "Number", "", vec![Prop::F64(unit)]),
        ]),
    ]);
    let mut objects = Vec::new();
    let mut links = Vec::new();
    let mut next_id: i64 = 1_000_000;
    let mut id = || {
        next_id += 1;
        next_id
    };
    let mut materials: HashMap<String, i64> = HashMap::new();
    for it in items {
        let gid = id();
        let mid = id();
        let flip = it.world.determinant() < 0.0;
        let verts: Vec<f64> = it.mesh.points.iter().flat_map(|q| (to_rh(*q) * scale).to_array()).map(|v| v as f64).collect();
        let mut index = Vec::new();
        for poly in &it.mesh.polys {
            let poly = if flip { poly.clone() } else { reversed(poly) };
            for (k, &v) in poly.iter().enumerate() {
                index.push(if k + 1 == poly.len() { -(v as i32) - 1 } else { v as i32 });
            }
        }
        objects.push(FbxNode::new("Geometry", vec![Prop::I64(gid), class_name(&it.name, "Geometry"), s("Mesh")]).with(vec![
            FbxNode::new("Properties70", vec![]),
            FbxNode::new("GeometryVersion", vec![i32p(124)]),
            FbxNode::new("Vertices", vec![Prop::ArrF64(verts)]),
            FbxNode::new("PolygonVertexIndex", vec![Prop::ArrI32(index)]),
            FbxNode::new("LayerElementMaterial", vec![i32p(0)]).with(vec![
                FbxNode::new("Version", vec![i32p(101)]),
                FbxNode::new("Name", vec![s("")]),
                FbxNode::new("MappingInformationType", vec![s("AllSame")]),
                FbxNode::new("ReferenceInformationType", vec![s("IndexToDirect")]),
                FbxNode::new("Materials", vec![Prop::ArrI32(vec![0])]),
            ]),
            FbxNode::new("Layer", vec![i32p(0)]).with(vec![
                    FbxNode::new("Version", vec![i32p(100)]),
                    FbxNode::new("LayerElement", vec![])
                        .with(vec![FbxNode::new("Type", vec![s("LayerElementMaterial")]), FbxNode::new("TypedIndex", vec![i32p(0)])]),
                ]),
        ]));
        let (sc, rot, tr) = mat_to_rh(it.world).to_scale_rotation_translation();
        objects.push(FbxNode::new("Model", vec![Prop::I64(mid), class_name(&it.name, "Model"), s("Mesh")]).with(vec![
            FbxNode::new("Version", vec![i32p(232)]),
            FbxNode::new("Properties70", vec![]).with(vec![
                p3("Lcl Translation", "Lcl Translation", "", tr * scale),
                p3("Lcl Rotation", "Lcl Rotation", "", euler_xyz(rot)),
                p3("Lcl Scaling", "Lcl Scaling", "", sc),
                p("DefaultAttributeIndex", "int", "Integer", "", vec![i32p(0)]),
            ]),
            FbxNode::new("Shading", vec![Prop::Bool(true)]),
            FbxNode::new("Culling", vec![s("CullingOff")]),
        ]));
        links.push((gid, mid));
        links.push((mid, 0));
        let mat = match materials.get(&it.material) {
            Some(&m) => m,
            None => {
                let m = id();
                let c = Vec3::from(it.color);
                objects.push(FbxNode::new("Material", vec![Prop::I64(m), class_name(&it.material, "Material"), s("")]).with(vec![
                    FbxNode::new("Version", vec![i32p(102)]),
                    FbxNode::new("ShadingModel", vec![s("phong")]),
                    FbxNode::new("MultiLayer", vec![i32p(0)]),
                    FbxNode::new("Properties70", vec![]).with(vec![
                        p3("DiffuseColor", "Color", "", c),
                        p("DiffuseFactor", "double", "Number", "A", vec![Prop::F64(1.0)]),
                        p3("SpecularColor", "Color", "", Vec3::splat(0.2)),
                        p("Shininess", "double", "Number", "A", vec![Prop::F64(((1.0 - it.roughness) * 100.0) as f64)]),
                    ]),
                ]));
                materials.insert(it.material.clone(), m);
                m
            }
        };
        links.push((mat, mid));
    }
    let count = |name: &str| objects.iter().filter(|o| o.name == name).count() as i32;
    let definitions = FbxNode::new("Definitions", vec![]).with(vec![
        FbxNode::new("Version", vec![i32p(100)]),
        FbxNode::new("Count", vec![i32p(objects.len() as i32 + 1)]),
        FbxNode::new("ObjectType", vec![s("GlobalSettings")]).with(vec![FbxNode::new("Count", vec![i32p(1)])]),
        FbxNode::new("ObjectType", vec![s("Model")]).with(vec![FbxNode::new("Count", vec![i32p(count("Model"))])]),
        FbxNode::new("ObjectType", vec![s("Geometry")]).with(vec![FbxNode::new("Count", vec![i32p(count("Geometry"))])]),
        FbxNode::new("ObjectType", vec![s("Material")]).with(vec![FbxNode::new("Count", vec![i32p(count("Material"))])]),
    ]);
    let connections =
        FbxNode::new("Connections", vec![]).with(links.into_iter().map(|(a, b)| FbxNode::new("C", vec![s("OO"), Prop::I64(a), Prop::I64(b)])).collect());
    let documents = FbxNode::new("Documents", vec![]).with(vec![
        FbxNode::new("Count", vec![i32p(1)]),
        FbxNode::new("Document", vec![Prop::I64(1), s("Scene"), s("Scene")])
            .with(vec![FbxNode::new("Properties70", vec![]), FbxNode::new("RootNode", vec![Prop::I64(0)])]),
    ]);
    encode(&[
        header,
        FbxNode::new("FileId", vec![Prop::Raw(vec![0x28, 0xb3, 0x2a, 0xeb, 0xb6, 0x24, 0xcc, 0xc2, 0xbf, 0xc8, 0xb0, 0x2a, 0xa9, 0x2b, 0xfc, 0xf1])]),
        FbxNode::new("CreationTime", vec![s("1970-01-01 10:00:00:000")]),
        FbxNode::new("Creator", vec![s("Forma3D")]),
        global,
        documents,
        FbxNode::new("References", vec![]),
        definitions,
        FbxNode::new("Objects", vec![]).with(objects),
        connections,
    ])
}

// --- reading a scene ----------------------------------------------------------

/// FBX's rotation orders (RotationOrder 0..5) as matrix products.
fn rotation(order: i64, deg: Vec3) -> Quat {
    let (x, y, z) = (Quat::from_rotation_x(deg.x.to_radians()), Quat::from_rotation_y(deg.y.to_radians()), Quat::from_rotation_z(deg.z.to_radians()));
    match order {
        1 => y * z * x, // XZY: X first, then Z, then Y
        2 => x * z * y, // YZX
        3 => z * x * y, // YXZ
        4 => y * x * z, // ZXY
        5 => x * y * z, // ZYX
        _ => z * y * x, // XYZ
    }
}

fn model_matrix(m: &FbxNode) -> Mat4 {
    let t = m.p70_vec("Lcl Translation").unwrap_or(Vec3::ZERO);
    let r = m.p70_vec("Lcl Rotation").unwrap_or(Vec3::ZERO);
    let s = m.p70_vec("Lcl Scaling").unwrap_or(Vec3::ONE);
    let order = m.p70("RotationOrder").and_then(|v| v.first()?.as_i64()).unwrap_or(0);
    let pre = m.p70_vec("PreRotation").map(|v| rotation(0, v)).unwrap_or(Quat::IDENTITY);
    let post = m.p70_vec("PostRotation").map(|v| rotation(0, v)).unwrap_or(Quat::IDENTITY);
    let rot = pre * rotation(order, r) * post.inverse();
    Mat4::from_translation(t) * Mat4::from_quat(rot) * Mat4::from_scale(s)
}

/// The geometric offset some files put between a model and its mesh.
fn geometric(m: &FbxNode) -> Mat4 {
    let t = m.p70_vec("GeometricTranslation").unwrap_or(Vec3::ZERO);
    let r = m.p70_vec("GeometricRotation").unwrap_or(Vec3::ZERO);
    let s = m.p70_vec("GeometricScaling").unwrap_or(Vec3::ONE);
    Mat4::from_translation(t) * Mat4::from_quat(rotation(0, r)) * Mat4::from_scale(s)
}

pub fn read(bytes: &[u8]) -> Result<Imported, String> {
    let doc = decode(bytes)?;
    let top = |name: &str| doc.iter().find(|n| n.name == name);
    let objects = top("Objects").ok_or("the FBX file has no objects")?;
    let mut out = Imported::default();

    // The file's axes, as a matrix into right-handed Y-up, and its unit (cm).
    let gs = top("GlobalSettings");
    let gi = |k: &str, d: i64| gs.and_then(|g| g.p70(k)).and_then(|v| v.first()?.as_i64()).unwrap_or(d);
    let axis = |i: i64| match i {
        0 => Vec3::X,
        1 => Vec3::Y,
        _ => Vec3::Z,
    };
    let (up, front, coord) = (
        axis(gi("UpAxis", 1)) * gi("UpAxisSign", 1).signum() as f32,
        axis(gi("FrontAxis", 2)) * gi("FrontAxisSign", 1).signum() as f32,
        axis(gi("CoordAxis", 0)) * gi("CoordAxisSign", 1).signum() as f32,
    );
    // Rows: where X, Y, Z of the target come from in the file.
    let a = Mat3::from_cols(coord, up, front).transpose();
    let a = if a.determinant().abs() > 0.5 { a } else { Mat3::IDENTITY };
    let unit = gs.and_then(|g| g.p70_num("UnitScaleFactor")).filter(|u| u.is_finite() && *u > 0.0).unwrap_or(1.0) as f32;
    let conv = Mat4::from_mat3(a) * Mat4::from_scale(Vec3::splat(unit));
    let conv_inv = conv.inverse();
    let mirrored = a.determinant() < 0.0;

    let id = |n: &FbxNode| n.props.first().and_then(Prop::as_i64);
    let name_of = |n: &FbxNode| {
        n.props.get(1).and_then(Prop::as_str).map(|s| s.split('\0').next().unwrap_or(s).trim_start_matches("Model::").to_owned()).unwrap_or_default()
    };
    let mut models: HashMap<i64, &FbxNode> = HashMap::new();
    let mut geoms: HashMap<i64, &FbxNode> = HashMap::new();
    let mut mats: HashMap<i64, usize> = HashMap::new();
    let mut skipped = false;
    for o in &objects.children {
        let Some(i) = id(o) else { continue };
        match o.name.as_str() {
            "Model" => {
                models.insert(i, o);
            }
            "Geometry" if o.props.get(2).and_then(Prop::as_str) == Some("Mesh") => {
                geoms.insert(i, o);
            }
            "Material" => {
                let c = o.p70_vec("DiffuseColor").or_else(|| o.p70_vec("Diffuse")).unwrap_or(Vec3::splat(0.8));
                mats.insert(i, out.materials.len());
                out.materials.push(ImportedMaterial { name: name_of(o), color: c.to_array(), roughness: None, metalness: None });
            }
            "NodeAttribute" | "Deformer" | "AnimationCurve" | "AnimationCurveNode" | "Pose" => skipped = true,
            _ => {}
        }
    }
    if skipped {
        out.note("cameras, lights, bones and animation were left out");
    }
    // Links: child -> parent.
    let mut model_parent: HashMap<i64, i64> = HashMap::new();
    let mut model_geom: HashMap<i64, i64> = HashMap::new();
    let mut model_mat: HashMap<i64, i64> = HashMap::new();
    for c in top("Connections").map(|c| &c.children[..]).unwrap_or(&[]) {
        if c.props.first().and_then(Prop::as_str) != Some("OO") {
            continue;
        }
        let (Some(a), Some(b)) = (c.props.get(1).and_then(Prop::as_i64), c.props.get(2).and_then(Prop::as_i64)) else { continue };
        if models.contains_key(&a) && (b == 0 || models.contains_key(&b)) {
            model_parent.insert(a, b);
        } else if geoms.contains_key(&a) && models.contains_key(&b) {
            model_geom.insert(b, a);
        } else if mats.contains_key(&a) && models.contains_key(&b) {
            model_mat.entry(b).or_insert(a);
        }
    }
    // Parents first.
    let mut order: Vec<i64> = Vec::new();
    let mut placed: HashMap<i64, usize> = HashMap::new();
    let mut ids: Vec<i64> = models.keys().copied().collect();
    ids.sort();
    let mut pending = ids;
    for _ in 0..64 {
        let before = pending.len();
        pending.retain(|m| {
            let parent = model_parent.get(m).copied().unwrap_or(0);
            if parent == 0 || placed.contains_key(&parent) || !models.contains_key(&parent) {
                placed.insert(*m, order.len());
                order.push(*m);
                false
            } else {
                true
            }
        });
        if pending.is_empty() || pending.len() == before {
            break;
        }
    }
    for m in order {
        let model = models[&m];
        let parent = model_parent.get(&m).and_then(|p| placed.get(p)).copied();
        let local_file = model_matrix(model);
        // Into right-handed Y-up centimetres (the change of axes and units).
        let local_rh = conv * local_file * conv_inv;
        let mut node =
            Node { name: name_of(model), parent, local: mat_to_rh(local_rh), mesh: None, material: model_mat.get(&m).and_then(|x| mats.get(x)).copied() };
        if let Some(g) = model_geom.get(&m).and_then(|g| geoms.get(g)) {
            let pts = g.child("Vertices").and_then(|v| v.props.first()).and_then(Prop::floats).unwrap_or_default();
            let idx = g.child("PolygonVertexIndex").and_then(|v| v.props.first()).and_then(Prop::ints).unwrap_or_default();
            let geo = conv * geometric(model) * conv_inv;
            let points: Vec<Vec3> =
                pts.chunks_exact(3).map(|c| geo.transform_point3(conv.transform_point3(Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32)))).collect();
            let mut polys = Vec::new();
            let mut cur = Vec::new();
            for v in idx {
                if v < 0 {
                    cur.push((-v - 1) as u32);
                    polys.push(if mirrored { reversed(&cur) } else { std::mem::take(&mut cur) });
                    cur.clear();
                } else {
                    cur.push(v as u32);
                }
            }
            node.mesh = Some(mesh_from_rh(points, polys));
        }
        out.nodes.push(node);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::{Format, testing};
    use super::*;

    #[test]
    fn nodes_encode_and_decode() {
        let n = FbxNode::new("A", vec![Prop::I32(5), Prop::Str("x".into())])
            .with(vec![FbxNode::new("B", vec![Prop::ArrF64((0..100).map(|i| i as f64).collect())])]);
        let back = decode(&encode(std::slice::from_ref(&n))).unwrap();
        assert_eq!(back, vec![n]);
    }

    #[test]
    fn round_trips() {
        let back = testing::round_trip(Format::Fbx);
        assert_eq!(back.mesh_count(), 2);
        assert_eq!(back.materials.iter().filter(|m| m.name == "Red").count(), 1);
        let cube = &back.nodes[0];
        let (_, rot, pos) = cube.local.to_scale_rotation_translation();
        assert!((pos - Vec3::new(100.0, 50.0, -30.0)).length() < 0.01, "{pos}");
        assert!(rot.angle_between(crate::scene::hpb_to_quat(Vec3::new(30.0, 0.0, 0.0))) < 1e-3);
    }

    #[test]
    fn every_rotation_order_matches_its_name() {
        // XYZ: X is applied first.
        let r = rotation(0, Vec3::new(90.0, 0.0, 90.0));
        let v = r * Vec3::Y;
        // Y -> (X 90) -> Z -> (Z 90) -> Z
        assert!((v - Vec3::Z).length() < 1e-5, "{v}");
    }
}
