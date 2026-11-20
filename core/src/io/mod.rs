//! Import and export of other 3D formats: OBJ, STL, glTF (GLB), FBX, USD
//! (text) and Alembic.
//!
//! Every format goes through the same two shapes: [`Imported`] (a tree of
//! named nodes, with polygon meshes and simple materials) on the way in and
//! a list of [`ExportItem`]s (the scene's surfaces) on the way out. Points
//! are in Forma3D's space (left-handed, Y up, centimetres); each format
//! converts to its own convention.

pub mod abc;
pub mod fbx;
pub mod gltf;
pub mod obj;
pub mod stl;
pub mod usd;

use crate::editmesh::EditMesh;
use crate::editor::Editor;
use crate::generator::Geo;
use crate::scene::{ObjectId, ObjectKind, Scene};
use glam::{Mat4, Vec3};

/// The formats Forma3D reads and writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Obj,
    Stl,
    Gltf,
    Fbx,
    Usd,
    Alembic,
}

impl Format {
    pub const ALL: [Format; 6] = [Format::Obj, Format::Stl, Format::Gltf, Format::Fbx, Format::Usd, Format::Alembic];

    pub fn label(self) -> &'static str {
        match self {
            Format::Obj => "Wavefront OBJ",
            Format::Stl => "STL",
            Format::Gltf => "glTF Binary (GLB)",
            Format::Fbx => "FBX",
            Format::Usd => "USD (text)",
            Format::Alembic => "Alembic",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Format::Obj => "obj",
            Format::Stl => "stl",
            Format::Gltf => "gltf",
            Format::Fbx => "fbx",
            Format::Usd => "usd",
            Format::Alembic => "abc",
        }
    }

    /// The file extension written on export.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Obj => "obj",
            Format::Stl => "stl",
            Format::Gltf => "glb",
            Format::Fbx => "fbx",
            Format::Usd => "usda",
            Format::Alembic => "abc",
        }
    }

    /// Extensions this format reads.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Format::Obj => &["obj"],
            Format::Stl => &["stl"],
            Format::Gltf => &["glb", "gltf"],
            Format::Fbx => &["fbx"],
            Format::Usd => &["usda", "usd"],
            Format::Alembic => &["abc"],
        }
    }

    pub fn from_extension(ext: &str) -> Option<Format> {
        let ext = ext.trim_start_matches('.').to_ascii_lowercase();
        Format::ALL.into_iter().find(|f| f.extensions().contains(&ext.as_str()))
    }

    pub fn from_file_name(name: &str) -> Option<Format> {
        name.rsplit_once('.').and_then(|(_, ext)| Format::from_extension(ext))
    }

    /// File units per Forma3D centimetre by default: OBJ, FBX, USD and
    /// Alembic in centimetres, STL in millimetres, glTF in metres.
    pub fn default_scale(self) -> f32 {
        match self {
            Format::Stl => 10.0,
            Format::Gltf => 0.01,
            _ => 1.0,
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Format::Gltf => "model/gltf-binary",
            Format::Stl => "model/stl",
            Format::Obj => "model/obj",
            Format::Usd => "model/vnd.usda",
            _ => "application/octet-stream",
        }
    }
}

/// A material as other formats see it.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedMaterial {
    pub name: String,
    /// Linear RGB.
    pub color: [f32; 3],
    pub roughness: Option<f32>,
    pub metalness: Option<f32>,
}

/// One node of an imported file: a transform, maybe with a mesh.
#[derive(Clone, Debug, Default)]
pub struct Node {
    pub name: String,
    pub parent: Option<usize>,
    /// Relative to the parent, in Forma3D space.
    pub local: Mat4,
    pub mesh: Option<EditMesh>,
    pub material: Option<usize>,
}

/// What a file holds, in Forma3D space. Parents come before their children.
#[derive(Clone, Debug, Default)]
pub struct Imported {
    pub nodes: Vec<Node>,
    pub materials: Vec<ImportedMaterial>,
    /// Things the file had that Forma3D left out (shown to the user).
    pub notes: Vec<String>,
}

impl Imported {
    pub fn polygon_count(&self) -> usize {
        self.nodes.iter().filter_map(|n| n.mesh.as_ref()).map(|m| m.polys.len()).sum()
    }

    pub fn mesh_count(&self) -> usize {
        self.nodes.iter().filter(|n| n.mesh.is_some()).count()
    }

    fn note(&mut self, s: impl Into<String>) {
        let s = s.into();
        if !self.notes.contains(&s) {
            self.notes.push(s);
        }
    }
}

/// Reads a file of any supported format (chosen by its name).
pub fn import(name: &str, bytes: &[u8]) -> Result<Imported, String> {
    let format = Format::from_file_name(name).ok_or_else(|| format!("{name}: not a format Forma3D can import"))?;
    import_as(format, bytes, format.default_scale())
}

/// Reads a file; `scale` is file units per centimetre.
pub fn import_as(format: Format, bytes: &[u8], scale: f32) -> Result<Imported, String> {
    let out = match format {
        Format::Obj => obj::read(bytes)?,
        Format::Stl => stl::read(bytes)?,
        Format::Gltf => gltf::read(bytes)?,
        Format::Fbx => fbx::read(bytes)?,
        Format::Usd => usd::read(bytes)?,
        Format::Alembic => abc::read(bytes)?,
    };
    finish(out, scale)
}

fn finish(mut out: Imported, scale: f32) -> Result<Imported, String> {
    let to_cm = 1.0 / if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    // File units to centimetres: scale translations and points.
    for n in &mut out.nodes {
        n.local.w_axis = (n.local.w_axis.truncate() * to_cm).extend(1.0);
        if let Some(m) = &mut n.mesh {
            for p in &mut m.points {
                *p *= to_cm;
            }
        }
    }
    clean(&mut out);
    if out.mesh_count() == 0 {
        return Err("the file has no polygon meshes".to_owned());
    }
    Ok(out)
}

/// Drops degenerate polygons and repeated points so imported meshes pass
/// [`EditMesh::validate`].
fn clean(out: &mut Imported) {
    let mut dropped = 0usize;
    for n in &mut out.nodes {
        let Some(m) = &mut n.mesh else { continue };
        let count = m.points.len() as u32;
        let before = m.polys.len();
        m.polys = std::mem::take(&mut m.polys)
            .into_iter()
            .filter_map(|mut p| {
                p.dedup();
                while p.len() > 1 && p.first() == p.last() {
                    p.pop();
                }
                let unique: std::collections::HashSet<u32> = p.iter().copied().collect();
                (p.len() >= 3 && unique.len() == p.len() && p.iter().all(|&i| i < count)).then_some(p)
            })
            .collect();
        dropped += before - m.polys.len();
        if m.points.iter().any(|p| !p.is_finite()) {
            for p in &mut m.points {
                if !p.is_finite() {
                    *p = Vec3::ZERO;
                }
            }
        }
        m.remove_unused_points();
        if m.polys.is_empty() {
            n.mesh = None;
        }
    }
    if dropped > 0 {
        out.note(format!("{dropped} broken polygon(s) were left out"));
    }
}

/// Adds an imported file to the scene under a Null named `name`; returns it.
pub fn add_to_scene(scene: &mut Scene, imported: &Imported, name: &str) -> ObjectId {
    let mats: Vec<_> = imported
        .materials
        .iter()
        .map(|m| {
            let id = scene.add_material(&m.name, m.color);
            if let Some(mat) = scene.material_mut(id) {
                if let Some(r) = m.roughness {
                    mat.roughness = r.clamp(0.0, 1.0);
                }
                if let Some(mt) = m.metalness {
                    mat.metalness = mt.clamp(0.0, 1.0);
                }
            }
            id
        })
        .collect();
    let root = scene.add_null(None);
    if let Some(o) = scene.get_mut(root) {
        o.name = name.to_owned();
    }
    let mut ids: Vec<ObjectId> = Vec::with_capacity(imported.nodes.len());
    for n in &imported.nodes {
        let parent = n.parent.and_then(|p| ids.get(p).copied()).unwrap_or(root);
        let kind = match &n.mesh {
            Some(m) => ObjectKind::Polygon(m.clone()),
            None => ObjectKind::Null,
        };
        let id = scene.add_object(if n.name.is_empty() { "Object" } else { &n.name }, kind, Some(parent));
        let (scale, rot, pos) = n.local.to_scale_rotation_translation();
        if let Some(o) = scene.get_mut(id) {
            o.pos = if pos.is_finite() { pos } else { Vec3::ZERO };
            o.rot = crate::scene::quat_to_hpb_near(rot, Vec3::ZERO);
            o.scale = if scale.is_finite() { scale } else { Vec3::ONE };
            o.material = n.material.and_then(|m| mats.get(m).copied());
        }
        ids.push(id);
    }
    root
}

/// One surface to export: its mesh (in its own space) and where it is.
#[derive(Clone, Debug)]
pub struct ExportItem {
    pub name: String,
    /// Object space to world, Forma3D space.
    pub world: Mat4,
    pub mesh: EditMesh,
    /// Linear RGB.
    pub color: [f32; 3],
    pub material: String,
    pub roughness: f32,
    pub metalness: f32,
}

/// The scene's surfaces as a final render shows them (generators,
/// deformers and primitives evaluated), or only the selected ones.
pub fn gather(ed: &mut Editor, selected_only: bool) -> Vec<ExportItem> {
    let selected: std::collections::HashSet<ObjectId> =
        if selected_only { ed.selection.iter().flat_map(|&id| ed.scene.subtree(id)).collect() } else { Default::default() };
    let objects = ed.final_objects();
    let mut used = std::collections::HashMap::<String, usize>::new();
    let mut out = Vec::new();
    for o in objects {
        if selected_only && !selected.contains(&o.id) {
            continue;
        }
        let mesh = EditMesh::from_geo(&Geo::from_mesh(&o.mesh));
        if mesh.polys.is_empty() {
            continue;
        }
        let base = ed.scene.get(o.id).map_or("Object".to_owned(), |x| x.name.clone());
        let n = used.entry(base.clone()).or_insert(0);
        *n += 1;
        let name = if *n == 1 { base } else { format!("{base} {n}") };
        let mat = o.material.and_then(|m| ed.scene.material(m));
        out.push(ExportItem {
            name,
            world: o.world,
            mesh,
            color: mat.map_or(o.color, |m| m.color),
            material: mat.map_or_else(|| "Default".to_owned(), |m| m.name.clone()),
            roughness: mat.map_or(0.5, |m| m.roughness),
            metalness: mat.map_or(0.0, |m| m.metalness),
        });
    }
    out
}

/// Writes items in a format; `scale` is file units per centimetre.
pub fn export(format: Format, items: &[ExportItem], scale: f32) -> Result<Vec<u8>, String> {
    if items.is_empty() {
        return Err("there is nothing to export".to_owned());
    }
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { format.default_scale() };
    Ok(match format {
        Format::Obj => obj::write(items, scale),
        Format::Stl => stl::write(items, scale),
        Format::Gltf => gltf::write(items, scale),
        Format::Fbx => fbx::write(items, scale),
        Format::Usd => usd::write(items, scale),
        Format::Alembic => abc::write(items, scale),
    })
}

/// Writes items as the files a format needs, named from `base` (a file
/// name without its extension): OBJ also writes its MTL materials file.
pub fn export_files(format: Format, items: &[ExportItem], scale: f32, base: &str) -> Result<Vec<(String, Vec<u8>)>, String> {
    if format == Format::Obj {
        if items.is_empty() {
            return Err("there is nothing to export".to_owned());
        }
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { format.default_scale() };
        let mtl = format!("{base}.mtl");
        let (obj, mtl_text) = obj::write_with_mtl(items, scale, &mtl);
        return Ok(vec![(format!("{base}.obj"), obj.into_bytes()), (mtl, mtl_text.into_bytes())]);
    }
    Ok(vec![(format!("{base}.{}", format.extension()), export(format, items, scale)?)])
}

/// The materials file an OBJ names (`mtllib`), if any.
pub fn obj_material_library(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(&bytes[..bytes.len().min(1 << 20)])
        .lines()
        .find_map(|l| l.trim().strip_prefix("mtllib ").map(|n| n.trim().rsplit(['/', '\\']).next().unwrap_or("").to_owned()))
        .filter(|n| !n.is_empty())
}

/// Reads a file with its companion files (an OBJ's MTL), matched by name.
pub fn import_with(name: &str, bytes: &[u8], extra: &[(String, Vec<u8>)], scale: f32) -> Result<Imported, String> {
    let format = Format::from_file_name(name).ok_or_else(|| format!("{name}: not a format Forma3D can import"))?;
    if format == Format::Obj {
        let want = obj_material_library(bytes);
        let mtl = extra
            .iter()
            .find(|(n, _)| want.as_deref().is_some_and(|w| n.eq_ignore_ascii_case(w)))
            .or_else(|| extra.iter().find(|(n, _)| n.to_ascii_lowercase().ends_with(".mtl")));
        let mut out = obj::read_with_mtl(bytes, mtl.map(|(_, b)| String::from_utf8_lossy(b).into_owned()).as_deref())?;
        if mtl.is_none()
            && let Some(w) = want
        {
            out.note(format!("its materials file {w} wasn't chosen, so colours are plain"));
        }
        return finish(out, scale);
    }
    import_as(format, bytes, scale)
}

// --- shared helpers ----------------------------------------------------------

/// Forma3D (left-handed) to a right-handed Y-up space: mirror Z.
pub(crate) fn to_rh(p: Vec3) -> Vec3 {
    Vec3::new(p.x, p.y, -p.z)
}

/// The same mirror for a transform: S * m * S.
pub(crate) fn mat_to_rh(m: Mat4) -> Mat4 {
    let s = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0));
    s * m * s
}

/// Mirroring flips the winding: polygons are reversed both ways.
pub(crate) fn reversed(poly: &[u32]) -> Vec<u32> {
    let mut p = poly.to_vec();
    p.reverse();
    p
}

/// A mesh from right-handed points and polygons (counter-clockwise from
/// outside, the usual right-handed convention).
pub(crate) fn mesh_from_rh(points: Vec<Vec3>, polys: Vec<Vec<u32>>) -> EditMesh {
    EditMesh { points: points.into_iter().map(to_rh).collect(), polys: polys.iter().map(|p| reversed(p)).collect() }
}

/// sRGB (0..1) to linear.
pub(crate) fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

pub(crate) fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// A name safe for formats that allow only letters, digits and underscores.
pub(crate) fn identifier(name: &str) -> String {
    let mut s: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    if s.is_empty() || s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert(0, '_');
    }
    s
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use crate::primitive::PrimitiveKind;

    /// Signed volume: positive when polygons face outwards.
    pub fn volume(m: &EditMesh) -> f32 {
        let g = m.to_geo();
        g.tris
            .chunks_exact(3)
            .map(|t| {
                let p = |i: u32| Vec3::from(g.verts[i as usize].pos);
                p(t[0]).dot(p(t[1]).cross(p(t[2]))) / 6.0
            })
            .sum()
    }

    /// A scene with a cube (moved, turned, coloured) and a sphere.
    pub fn items() -> Vec<ExportItem> {
        let mut ed = Editor::new(Scene::new());
        let cube = ed.scene.add_primitive(PrimitiveKind::Cube, None);
        let red = ed.scene.add_material("Red", [0.8, 0.1, 0.05]);
        if let Some(o) = ed.scene.get_mut(cube) {
            o.pos = Vec3::new(100.0, 50.0, -30.0);
            o.rot = Vec3::new(30.0, 0.0, 0.0);
            o.material = Some(red);
        }
        let sphere = ed.scene.add_primitive(PrimitiveKind::Sphere, None);
        if let Some(o) = ed.scene.get_mut(sphere) {
            o.pos = Vec3::new(-150.0, 0.0, 40.0);
        }
        gather(&mut ed, false)
    }

    /// World-space bounds of everything in an import.
    pub fn bounds(i: &Imported) -> (Vec3, Vec3) {
        let mut world = vec![Mat4::IDENTITY; i.nodes.len()];
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for (k, n) in i.nodes.iter().enumerate() {
            world[k] = n.parent.map_or(Mat4::IDENTITY, |p| world[p]) * n.local;
            if let Some(m) = &n.mesh {
                for p in &m.points {
                    let q = world[k].transform_point3(*p);
                    lo = lo.min(q);
                    hi = hi.max(q);
                }
            }
        }
        (lo, hi)
    }

    pub fn item_bounds(items: &[ExportItem]) -> (Vec3, Vec3) {
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for it in items {
            for p in &it.mesh.points {
                let q = it.world.transform_point3(*p);
                lo = lo.min(q);
                hi = hi.max(q);
            }
        }
        (lo, hi)
    }

    /// Exports, imports again and checks the shapes survived.
    pub fn round_trip(format: Format) -> Imported {
        let items = items();
        let bytes = export(format, &items, format.default_scale()).unwrap();
        let back = import_as(format, &bytes, format.default_scale()).unwrap_or_else(|e| panic!("{format:?}: {e}"));
        let (lo, hi) = item_bounds(&items);
        let (blo, bhi) = bounds(&back);
        assert!((lo - blo).abs().max_element() < 0.05 && (hi - bhi).abs().max_element() < 0.05, "{format:?}: {lo} {hi} vs {blo} {bhi}");
        // Every mesh still faces outwards.
        let mut world = vec![Mat4::IDENTITY; back.nodes.len()];
        for (k, n) in back.nodes.iter().enumerate() {
            world[k] = n.parent.map_or(Mat4::IDENTITY, |p| world[p]) * n.local;
            if let Some(m) = &n.mesh {
                let flipped = world[k].determinant() < 0.0;
                let v = volume(m) * if flipped { -1.0 } else { 1.0 };
                assert!(v > 0.0, "{format:?}: {} faces inwards", n.name);
            }
        }
        back
    }

    #[test]
    fn forma3d_meshes_face_outwards() {
        for it in items() {
            assert!(volume(&it.mesh) > 0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obj_exports_and_reads_its_materials_file() {
        let items = testing::items();
        let files = export_files(Format::Obj, &items, 1.0, "scene").unwrap();
        assert_eq!(files.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), ["scene.obj", "scene.mtl"]);
        assert_eq!(obj_material_library(&files[0].1).as_deref(), Some("scene.mtl"));
        let back = import_with("scene.obj", &files[0].1, &files[1..], 1.0).unwrap();
        let red = back.materials.iter().find(|m| m.name == "Red").unwrap();
        assert!((red.color[0] - 0.8).abs() < 0.01, "{:?}", red.color);
        let plain = import_with("scene.obj", &files[0].1, &[], 1.0).unwrap();
        assert!(plain.notes.iter().any(|n| n.contains("scene.mtl")));
        // Other formats are one file.
        assert_eq!(export_files(Format::Gltf, &items, 0.01, "scene").unwrap()[0].0, "scene.glb");
    }
}

#[cfg(test)]
mod scene_tests {
    use super::*;

    #[test]
    fn an_imported_obj_keeps_its_size_in_the_scene() {
        let obj = b"mtllib p.mtl\no Pyramid\nv -100 0 -100\nv 100 0 -100\nv 100 0 100\nv -100 0 100\nv 0 150 0\nusemtl Brick\nf 1 2 3 4\nf 1 5 2\nf 2 5 3\nf 3 5 4\nf 4 5 1\n";
        let imported = import_with("p.obj", obj, &[("p.mtl".to_owned(), b"newmtl Brick\nKd 0.8 0.2 0.1\n".to_vec())], 1.0).unwrap();
        let mut ed = Editor::new(Scene::new());
        let root = add_to_scene(&mut ed.scene, &imported, "p");
        let objs = ed.final_objects();
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        let mut tris = 0;
        for o in objs.iter().filter(|o| o.id != root) {
            for v in &o.mesh.vertices {
                let p = o.world.transform_point3(Vec3::from(v.pos));
                lo = lo.min(p);
                hi = hi.max(p);
            }
            tris += o.mesh.indices.len() / 3;
        }
        assert_eq!(tris, 6);
        assert!((hi - lo - Vec3::new(200.0, 150.0, 200.0)).length() < 0.01, "{lo} {hi}");
    }
}
