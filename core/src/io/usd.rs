//! USD as text (.usda). Written right-handed, Y up, in centimetres
//! (metersPerUnit 0.01), with UsdPreviewSurface materials. Read: Xform and
//! Mesh prims with their transforms, either up axis, any unit, and
//! materials' diffuse colours. Binary USD (.usdc, most .usdz) isn't read.

use super::{ExportItem, Imported, ImportedMaterial, Node, identifier, mat_to_rh, mesh_from_rh, reversed, to_rh};
use glam::{DMat4, EulerRot, Mat4, Quat, Vec3};
use std::collections::HashMap;
use std::fmt::Write as _;

pub fn write(items: &[ExportItem], scale: f32) -> Vec<u8> {
    let mut s = String::new();
    let mpu = 0.01 / scale as f64;
    let _ = writeln!(
        s,
        "#usda 1.0\n(\n    defaultPrim = \"World\"\n    doc = \"Forma3D\"\n    metersPerUnit = {mpu}\n    upAxis = \"Y\"\n)\n\ndef Xform \"World\"\n{{"
    );
    let mut names: HashMap<String, usize> = HashMap::new();
    let mut unique = |n: &str| {
        let base = identifier(n);
        let k = names.entry(base.clone()).or_insert(0);
        *k += 1;
        if *k == 1 { base } else { format!("{base}_{k}") }
    };
    let mut mats: Vec<(String, &ExportItem)> = Vec::new();
    let mut mat_of: HashMap<&str, String> = HashMap::new();
    for it in items {
        if !mat_of.contains_key(it.material.as_str()) {
            let n = unique(&it.material);
            mat_of.insert(&it.material, n.clone());
            mats.push((n, it));
        }
    }
    for it in items {
        let name = unique(&it.name);
        let m = mat_to_rh(it.world);
        let flip = it.world.determinant() < 0.0;
        let _ = writeln!(s, "    def Mesh \"{name}\" (\n        prepend apiSchemas = [\"MaterialBindingAPI\"]\n    )\n    {{");
        let c = m.to_cols_array();
        let row = |k: usize| format!("({}, {}, {}, {})", c[k * 4], c[k * 4 + 1], c[k * 4 + 2], if k == 3 { 1.0 } else { c[k * 4 + 3] });
        let t = (m.w_axis.truncate() * scale).to_array();
        let _ = writeln!(s, "        matrix4d xformOp:transform = ({}, {}, {}, ({}, {}, {}, 1))", row(0), row(1), row(2), t[0], t[1], t[2]);
        let _ = writeln!(s, "        uniform token[] xformOpOrder = [\"xformOp:transform\"]");
        let polys: Vec<Vec<u32>> = it.mesh.polys.iter().map(|p| if flip { p.clone() } else { reversed(p) }).collect();
        let counts: Vec<String> = polys.iter().map(|p| p.len().to_string()).collect();
        let idx: Vec<String> = polys.iter().flatten().map(|i| i.to_string()).collect();
        let pts: Vec<String> = it
            .mesh
            .points
            .iter()
            .map(|p| {
                let q = to_rh(*p) * scale;
                format!("({}, {}, {})", q.x, q.y, q.z)
            })
            .collect();
        let (lo, hi) = it.mesh.points.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(a, b), p| {
            let q = to_rh(*p) * scale;
            (a.min(q), b.max(q))
        });
        let _ = writeln!(s, "        float3[] extent = [({}, {}, {}), ({}, {}, {})]", lo.x, lo.y, lo.z, hi.x, hi.y, hi.z);
        let _ = writeln!(s, "        int[] faceVertexCounts = [{}]", counts.join(", "));
        let _ = writeln!(s, "        int[] faceVertexIndices = [{}]", idx.join(", "));
        let _ = writeln!(s, "        point3f[] points = [{}]", pts.join(", "));
        let _ = writeln!(s, "        uniform token orientation = \"rightHanded\"");
        let _ = writeln!(s, "        uniform token subdivisionScheme = \"none\"");
        let _ = writeln!(s, "        color3f[] primvars:displayColor = [({}, {}, {})]", it.color[0], it.color[1], it.color[2]);
        let _ = writeln!(s, "        rel material:binding = </World/Looks/{}>", mat_of[it.material.as_str()]);
        let _ = writeln!(s, "    }}\n");
    }
    let _ = writeln!(s, "    def Scope \"Looks\"\n    {{");
    for (n, it) in &mats {
        let _ = writeln!(
            s,
            "        def Material \"{n}\"\n        {{\n            token outputs:surface.connect = </World/Looks/{n}/Surface.outputs:surface>\n\n            def Shader \"Surface\"\n            {{\n                uniform token info:id = \"UsdPreviewSurface\"\n                color3f inputs:diffuseColor = ({}, {}, {})\n                float inputs:roughness = {}\n                float inputs:metallic = {}\n                token outputs:surface\n            }}\n        }}",
            it.color[0], it.color[1], it.color[2], it.roughness, it.metalness
        );
    }
    let _ = writeln!(s, "    }}\n}}");
    s.into_bytes()
}

// --- reading ------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Word(String),
    Str(String),
    Path(String),
    Num(f64),
    Punct(char),
}

fn tokenize(text: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '#' {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '"' || ch == '\'' {
            let triple = i + 2 < c.len() && c[i + 1] == ch && c[i + 2] == ch;
            let mut s = String::new();
            i += if triple { 3 } else { 1 };
            loop {
                if i >= c.len() {
                    return Err("a string never ends".to_owned());
                }
                if triple && i + 2 < c.len() && c[i] == ch && c[i + 1] == ch && c[i + 2] == ch {
                    i += 3;
                    break;
                }
                if !triple && c[i] == ch {
                    i += 1;
                    break;
                }
                if c[i] == '\\' && i + 1 < c.len() {
                    s.push(c[i + 1]);
                    i += 2;
                    continue;
                }
                s.push(c[i]);
                i += 1;
            }
            out.push(Tok::Str(s));
        } else if ch == '@' {
            // An asset path: @...@ (or @@@...@@@).
            let triple = c.get(i + 1) == Some(&'@') && c.get(i + 2) == Some(&'@');
            i += if triple { 3 } else { 1 };
            let mut s = String::new();
            while i < c.len() && c[i] != '@' {
                s.push(c[i]);
                i += 1;
            }
            i += if triple { 3 } else { 1 };
            out.push(Tok::Str(s));
        } else if ch == '<' {
            let mut s = String::new();
            i += 1;
            while i < c.len() && c[i] != '>' {
                s.push(c[i]);
                i += 1;
            }
            i += 1;
            out.push(Tok::Path(s));
        } else if "()[]{}=,;".contains(ch) {
            out.push(Tok::Punct(ch));
            i += 1;
        } else if ch == '-' || ch == '+' || ch == '.' || ch.is_ascii_digit() {
            let start = i;
            i += 1;
            while i < c.len() && (c[i].is_ascii_alphanumeric() || "+-.".contains(c[i])) {
                i += 1;
            }
            let s: String = c[start..i].iter().collect();
            match s.parse::<f64>() {
                Ok(v) => out.push(Tok::Num(v)),
                Err(_) => match s.as_str() {
                    "-inf" => out.push(Tok::Num(f64::NEG_INFINITY)),
                    "inf" => out.push(Tok::Num(f64::INFINITY)),
                    _ => out.push(Tok::Word(s)),
                },
            }
        } else {
            let start = i;
            while i < c.len() && (c[i].is_alphanumeric() || "_:.".contains(c[i])) {
                i += 1;
            }
            if i == start {
                i += 1;
                continue;
            }
            out.push(Tok::Word(c[start..i].iter().collect()));
        }
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq)]
enum Val {
    Num(f64),
    Text(String),
    Path(String),
    List(Vec<Val>),
    None,
}

impl Val {
    fn num(&self) -> Option<f64> {
        match self {
            Val::Num(v) => Some(*v),
            _ => None,
        }
    }

    fn text(&self) -> Option<&str> {
        match self {
            Val::Text(s) => Some(s),
            _ => None,
        }
    }

    fn list(&self) -> &[Val] {
        match self {
            Val::List(l) => l,
            _ => &[],
        }
    }

    fn vec3(&self) -> Option<Vec3> {
        let l = self.list();
        Some(Vec3::new(l.first()?.num()? as f32, l.get(1)?.num()? as f32, l.get(2)?.num()? as f32))
    }
}

#[derive(Debug, Default)]
struct Prim {
    kind: String,
    name: String,
    attrs: HashMap<String, Val>,
    children: Vec<Prim>,
    has_refs: bool,
}

struct Parser {
    t: Vec<Tok>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.t.get(self.i).cloned();
        self.i += 1;
        t
    }

    fn eat(&mut self, ch: char) -> bool {
        if self.peek() == Some(&Tok::Punct(ch)) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    /// Skips a bracketed group (the opening bracket already read).
    fn skip_group(&mut self, open: char) {
        let close = match open {
            '(' => ')',
            '[' => ']',
            _ => '}',
        };
        let mut depth = 1;
        while let Some(t) = self.next() {
            if t == Tok::Punct(open) {
                depth += 1;
            } else if t == Tok::Punct(close) {
                depth -= 1;
                if depth == 0 {
                    return;
                }
            }
        }
    }

    fn value(&mut self, depth: u32) -> Val {
        if depth > 32 {
            return Val::None;
        }
        match self.next() {
            Some(Tok::Num(v)) => Val::Num(v),
            Some(Tok::Str(s)) | Some(Tok::Word(s)) => Val::Text(s),
            Some(Tok::Path(p)) => Val::Path(p),
            Some(Tok::Punct(c)) if c == '(' || c == '[' => {
                let close = if c == '(' { ')' } else { ']' };
                let mut items = Vec::new();
                loop {
                    if self.eat(close) || self.peek().is_none() {
                        break;
                    }
                    if self.eat(',') {
                        continue;
                    }
                    items.push(self.value(depth + 1));
                }
                Val::List(items)
            }
            Some(Tok::Punct('{')) => {
                // timeSamples / dictionaries: keep the first entry's value.
                let mut first = Val::None;
                loop {
                    if self.eat('}') || self.peek().is_none() {
                        break;
                    }
                    let _key = self.next();
                    if self.eat(':') || matches!(self.peek(), Some(Tok::Word(w)) if w == ":") {
                        self.next();
                    }
                    if !self.eat('=') {
                        let _ = matches!(self.peek(), Some(Tok::Punct(':')));
                    }
                    let v = self.value(depth + 1);
                    if first == Val::None {
                        first = v;
                    }
                    self.eat(',');
                }
                first
            }
            _ => Val::None,
        }
    }

    fn prim(&mut self, kind_or_name: Option<String>, depth: u32) -> Result<Prim, String> {
        let mut prim = Prim::default();
        match (kind_or_name, self.peek().cloned()) {
            (Some(kind), Some(Tok::Str(name))) => {
                self.i += 1;
                prim.kind = kind;
                prim.name = name;
            }
            (None, Some(Tok::Str(name))) => {
                self.i += 1;
                prim.name = name;
            }
            _ => return Err("a prim without a name".to_owned()),
        }
        if self.eat('(') {
            // Metadata: note references and payloads (not followed).
            let start = self.i;
            self.skip_group('(');
            prim.has_refs = self.t[start..self.i].iter().any(|t| matches!(t, Tok::Word(w) if w == "references" || w == "payload"));
        }
        if !self.eat('{') {
            return Ok(prim);
        }
        self.body(&mut prim, depth)?;
        Ok(prim)
    }

    fn body(&mut self, prim: &mut Prim, depth: u32) -> Result<(), String> {
        if depth > 64 {
            return Err("the USD file nests too deeply".to_owned());
        }
        loop {
            let Some(t) = self.next() else { return Ok(()) };
            match t {
                Tok::Punct('}') => return Ok(()),
                Tok::Word(w) if w == "def" || w == "over" || w == "class" => {
                    let kind = match self.peek() {
                        Some(Tok::Word(k)) => {
                            let k = k.clone();
                            self.i += 1;
                            Some(k)
                        }
                        _ => None,
                    };
                    let child = self.prim(kind, depth + 1)?;
                    if w == "def" {
                        prim.children.push(child);
                    }
                }
                Tok::Word(w) if w == "variantSet" || w == "variantSets" => {
                    // Skip to the end of the set.
                    while let Some(t) = self.next() {
                        if t == Tok::Punct('{') {
                            self.skip_group('{');
                            break;
                        }
                        if t == Tok::Punct('=') && w == "variantSets" {
                            self.value(0);
                            break;
                        }
                    }
                }
                Tok::Word(w) if w == "rel" => {
                    let Some(Tok::Word(name)) = self.next() else { continue };
                    let v = if self.eat('=') { self.value(0) } else { Val::None };
                    if self.eat('(') {
                        self.skip_group('(');
                    }
                    prim.attrs.insert(name, v);
                }
                Tok::Word(mut ty) => {
                    // [custom] [uniform|varying] type[[]] name [= value] [(meta)]
                    while ty == "custom"
                        || ty == "uniform"
                        || ty == "varying"
                        || ty == "prepend"
                        || ty == "append"
                        || ty == "delete"
                        || ty == "add"
                        || ty == "reorder"
                    {
                        match self.next() {
                            Some(Tok::Word(next)) => ty = next,
                            _ => break,
                        }
                    }
                    if self.eat('[') {
                        self.eat(']');
                    }
                    let Some(Tok::Word(name)) = self.next() else { continue };
                    if self.eat('=') {
                        let v = self.value(0);
                        let key = name.trim_end_matches(".timeSamples").to_owned();
                        prim.attrs.entry(key).or_insert(v);
                    }
                    if self.eat('(') {
                        self.skip_group('(');
                    }
                }
                Tok::Punct('(') => self.skip_group('('),
                _ => {}
            }
        }
    }
}

fn prim_matrix(p: &Prim) -> Mat4 {
    let order: Vec<String> = p.attrs.get("xformOpOrder").map(|v| v.list().iter().filter_map(|x| x.text().map(str::to_owned)).collect()).unwrap_or_default();
    let mut m = Mat4::IDENTITY;
    for op in order {
        if op == "!resetXformStack!" {
            continue;
        }
        let (inverse, name) = match op.strip_prefix("!invert!") {
            Some(n) => (true, n.to_owned()),
            None => (false, op.clone()),
        };
        let Some(v) = p.attrs.get(&name) else { continue };
        let kind = name.trim_start_matches("xformOp:").split(':').next().unwrap_or("");
        let d = |x: Vec3| Vec3::new(x.x.to_radians(), x.y.to_radians(), x.z.to_radians());
        let one = |v: &Val| v.num().unwrap_or(0.0) as f32;
        let t = match kind {
            "translate" => Mat4::from_translation(v.vec3().unwrap_or(Vec3::ZERO)),
            "scale" => Mat4::from_scale(v.vec3().unwrap_or(Vec3::ONE)),
            "rotateX" => Mat4::from_rotation_x(one(v).to_radians()),
            "rotateY" => Mat4::from_rotation_y(one(v).to_radians()),
            "rotateZ" => Mat4::from_rotation_z(one(v).to_radians()),
            // rotateXYZ: X first (matrix Rz * Ry * Rx), and so on.
            "rotateXYZ" | "rotateXZY" | "rotateYXZ" | "rotateYZX" | "rotateZXY" | "rotateZYX" => {
                let r = d(v.vec3().unwrap_or(Vec3::ZERO));
                let (qx, qy, qz) = (Quat::from_rotation_x(r.x), Quat::from_rotation_y(r.y), Quat::from_rotation_z(r.z));
                let q = match kind {
                    "rotateXYZ" => qz * qy * qx,
                    "rotateXZY" => qy * qz * qx,
                    "rotateYXZ" => qz * qx * qy,
                    "rotateYZX" => qx * qz * qy,
                    "rotateZXY" => qy * qx * qz,
                    _ => qx * qy * qz,
                };
                Mat4::from_quat(q)
            }
            "orient" => {
                let l = v.list();
                let f = |k: usize| l.get(k).and_then(Val::num).unwrap_or(0.0) as f32;
                // USD quaternions are (real, i, j, k).
                Mat4::from_quat(Quat::from_xyzw(f(1), f(2), f(3), f(0)).normalize())
            }
            "transform" => {
                let rows: Vec<f64> = v.list().iter().flat_map(|r| r.list().iter().filter_map(Val::num).collect::<Vec<_>>()).collect();
                if rows.len() == 16 {
                    // USD rows are glam columns.
                    DMat4::from_cols_slice(&rows).as_mat4()
                } else {
                    Mat4::IDENTITY
                }
            }
            _ => Mat4::IDENTITY,
        };
        m *= if inverse { t.inverse() } else { t };
    }
    m
}

pub fn read(bytes: &[u8]) -> Result<Imported, String> {
    if bytes.starts_with(b"PXR-USDC") {
        return Err("this is binary USD (usdc); save it as text (.usda)".to_owned());
    }
    if bytes.starts_with(b"PK") {
        return Err("USDZ packages aren't supported; export as .usda".to_owned());
    }
    let text = String::from_utf8_lossy(bytes);
    if !text.trim_start().starts_with("#usda") {
        return Err("not a text USD file".to_owned());
    }
    let toks = tokenize(&text)?;
    let mut p = Parser { t: toks, i: 0 };
    // Layer metadata.
    let mut up = "Y".to_owned();
    let mut mpu = 0.01;
    if p.eat('(') {
        while let Some(t) = p.next() {
            match t {
                Tok::Punct(')') => break,
                Tok::Word(k) if k == "upAxis" || k == "metersPerUnit" => {
                    p.eat('=');
                    match p.value(0) {
                        Val::Text(s) if k == "upAxis" => up = s,
                        Val::Num(v) if k == "metersPerUnit" && v > 0.0 => mpu = v,
                        _ => {}
                    }
                }
                Tok::Punct(c) if "([{".contains(c) => p.skip_group(c),
                _ => {}
            }
        }
    }
    let mut root = Prim::default();
    p.body(&mut root, 0)?;
    // Materials by path.
    let mut out = Imported::default();
    let mut mats: HashMap<String, usize> = HashMap::new();
    fn find_mats(pr: &Prim, path: &str, out: &mut Imported, mats: &mut HashMap<String, usize>) {
        for c in &pr.children {
            let cp = format!("{path}/{}", c.name);
            if c.kind == "Material" {
                let color = c.children.iter().find_map(|s| s.attrs.get("inputs:diffuseColor").and_then(Val::vec3)).unwrap_or(Vec3::splat(0.8));
                let rough = c.children.iter().find_map(|s| s.attrs.get("inputs:roughness").and_then(Val::num)).map(|x| x as f32);
                let metal = c.children.iter().find_map(|s| s.attrs.get("inputs:metallic").and_then(Val::num)).map(|x| x as f32);
                mats.insert(cp.clone(), out.materials.len());
                out.materials.push(ImportedMaterial { name: c.name.clone(), color: color.to_array(), roughness: rough, metalness: metal });
            }
            find_mats(c, &cp, out, mats);
        }
    }
    find_mats(&root, "", &mut out, &mut mats);
    // Z-up files turn to Y-up; units become centimetres.
    let axes = if up == "Z" { Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2) } else { Mat4::IDENTITY };
    let conv = axes * Mat4::from_scale(Vec3::splat((mpu * 100.0) as f32));
    let conv_inv = conv.inverse();
    let mut refs = false;
    fn walk(pr: &Prim, path: &str, parent: Option<usize>, ctx: &mut (Mat4, Mat4, &mut Imported, &HashMap<String, usize>, &mut bool)) {
        for c in &pr.children {
            let cp = format!("{path}/{}", c.name);
            if matches!(
                c.kind.as_str(),
                "Material"
                    | "Shader"
                    | "Camera"
                    | "DistantLight"
                    | "SphereLight"
                    | "RectLight"
                    | "DomeLight"
                    | "DiskLight"
                    | "Skeleton"
                    | "SkelAnimation"
                    | "GeomSubset"
            ) {
                continue;
            }
            *ctx.4 |= c.has_refs;
            let local = mat_to_rh(ctx.0 * prim_matrix(c) * ctx.1);
            let me = ctx.2.nodes.len();
            let mut node = Node { name: c.name.clone(), parent, local, mesh: None, material: None };
            if c.kind == "Mesh" {
                let pts: Vec<Vec3> = c.attrs.get("points").map(|v| v.list().iter().filter_map(Val::vec3).collect()).unwrap_or_default();
                let counts: Vec<usize> =
                    c.attrs.get("faceVertexCounts").map(|v| v.list().iter().filter_map(|x| x.num()).map(|x| x as usize).collect()).unwrap_or_default();
                let idx: Vec<u32> =
                    c.attrs.get("faceVertexIndices").map(|v| v.list().iter().filter_map(|x| x.num()).map(|x| x as u32).collect()).unwrap_or_default();
                let left = c.attrs.get("orientation").and_then(Val::text) == Some("leftHanded");
                let mut polys = Vec::with_capacity(counts.len());
                let mut k = 0;
                for n in counts {
                    if k + n > idx.len() {
                        break;
                    }
                    let poly = idx[k..k + n].to_vec();
                    polys.push(if left { reversed(&poly) } else { poly });
                    k += n;
                }
                let points: Vec<Vec3> = pts.into_iter().map(|q| ctx.0.transform_point3(q)).collect();
                node.mesh = Some(mesh_from_rh(points, polys));
                if let Some(Val::Path(m)) = c.attrs.get("material:binding") {
                    node.material = ctx.3.get(m).copied();
                }
                if node.material.is_none()
                    && let Some(col) = c.attrs.get("primvars:displayColor").and_then(|v| v.list().first().and_then(Val::vec3))
                {
                    node.material = Some(ctx.2.materials.len());
                    ctx.2.materials.push(ImportedMaterial { name: format!("{} Colour", c.name), color: col.to_array(), roughness: None, metalness: None });
                }
            }
            ctx.2.nodes.push(node);
            walk(c, &cp, Some(me), ctx);
        }
    }
    walk(&root, "", None, &mut (conv, conv_inv, &mut out, &mats, &mut refs));
    if refs {
        out.note("references and payloads to other files were not followed");
    }
    // Points were converted per mesh; transforms by conjugation, so the
    // translation already carries the unit change.
    let _ = EulerRot::XYZ;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::{Format, testing};
    use super::*;

    #[test]
    fn round_trips() {
        let back = testing::round_trip(Format::Usd);
        assert_eq!(back.mesh_count(), 2);
        assert!(back.materials.iter().any(|m| m.name == "Red" && (m.color[0] - 0.8).abs() < 1e-5));
    }

    #[test]
    fn reads_z_up_metres_and_xform_ops() {
        let s = r#"#usda 1.0
(
    upAxis = "Z"
    metersPerUnit = 1
)
def Xform "Root" (
    kind = "component"
)
{
    double3 xformOp:translate = (1, 2, 3)
    float3 xformOp:rotateXYZ = (0, 0, 90)
    uniform token[] xformOpOrder = ["xformOp:translate", "xformOp:rotateXYZ"]
    def Mesh "Tri"
    {
        int[] faceVertexCounts = [3]
        int[] faceVertexIndices = [0, 1, 2]
        point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]
        color3f[] primvars:displayColor = [(0.1, 0.2, 0.3)] (interpolation = "constant")
    }
}
"#;
        let i = read(s.as_bytes()).unwrap();
        assert_eq!(i.nodes.len(), 2);
        // Z up, metres: the root sits at (100, 300, 200) cm, mirrored to Forma3D's Z.
        let pos = i.nodes[0].local.w_axis.truncate();
        assert!((pos - Vec3::new(100.0, 300.0, 200.0)).length() < 1e-3, "{pos}");
        assert_eq!(i.materials[0].color, [0.1, 0.2, 0.3]);
        assert!(read(b"PXR-USDC....").is_err());
    }
}
