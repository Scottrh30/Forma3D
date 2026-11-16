//! The asset library behind the Asset Browser: ready-made objects and
//! materials that ship with Forma3D, plus your own, saved as files in the
//! `assets` folder beside the settings.

use crate::primitive::Primitive;
use crate::scene::{ObjectId, ObjectKind, Scene};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetKind {
    Object,
    Material,
}

#[derive(Clone, Debug)]
pub enum Content {
    Scene(Scene),
    Material([f32; 3]),
}

#[derive(Clone, Debug)]
pub struct Asset {
    pub name: String,
    pub category: String,
    /// Ships with Forma3D (can't be deleted).
    pub builtin: bool,
    /// Where a saved asset lives.
    pub path: Option<PathBuf>,
    pub content: Content,
}

impl Asset {
    pub fn kind(&self) -> AssetKind {
        match self.content {
            Content::Scene(_) => AssetKind::Object,
            Content::Material(_) => AssetKind::Material,
        }
    }

    /// Text the search box matches against.
    pub fn haystack(&self) -> String {
        format!("{} {} {}", self.name, self.category, if self.kind() == AssetKind::Object { "object model" } else { "material colour color" })
            .to_lowercase()
    }
}

#[derive(Serialize, Deserialize)]
struct AssetFile {
    format: String,
    version: u32,
    name: String,
    category: String,
    kind: AssetKind,
    /// A whole Forma3D scene file, for objects.
    #[serde(default)]
    scene: Option<String>,
    #[serde(default)]
    color: Option<[f32; 3]>,
}

const FORMAT: &str = "forma3d-asset";

#[derive(Default)]
pub struct Library {
    pub assets: Vec<Asset>,
    pub dir: Option<PathBuf>,
    /// Files that couldn't be read, with the reason.
    pub errors: Vec<String>,
}

impl Library {
    /// The built-in assets plus the ones saved in `dir`.
    pub fn load(dir: Option<PathBuf>) -> Library {
        let mut lib = Library { assets: builtin(), dir, errors: Vec::new() };
        lib.reload_user();
        lib
    }

    pub fn reload_user(&mut self) {
        self.assets.retain(|a| a.builtin);
        self.errors.clear();
        let Some(dir) = self.dir.clone() else { return };
        let Ok(entries) = std::fs::read_dir(&dir) else { return };
        let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
        paths.sort();
        for p in paths {
            match read(&p) {
                Ok(a) => self.assets.push(a),
                Err(e) => self.errors.push(format!("{}: {e}", p.display())),
            }
        }
    }

    /// Saves objects as a new asset file and adds it to the library.
    pub fn save_scene(&mut self, name: &str, scene: Scene) -> Result<usize, String> {
        let text = scene.to_json().map_err(|e| e.to_string())?;
        let file = AssetFile { format: FORMAT.into(), version: 1, name: name.into(), category: "My Assets".into(), kind: AssetKind::Object, scene: Some(text), color: None };
        self.write(file, Content::Scene(scene))
    }

    pub fn save_material(&mut self, name: &str, color: [f32; 3]) -> Result<usize, String> {
        let file = AssetFile { format: FORMAT.into(), version: 1, name: name.into(), category: "My Assets".into(), kind: AssetKind::Material, scene: None, color: Some(color) };
        self.write(file, Content::Material(color))
    }

    fn write(&mut self, file: AssetFile, content: Content) -> Result<usize, String> {
        let dir = self.dir.clone().ok_or("no folder for assets")?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let stem: String = file.name.chars().map(|c| if c.is_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '_' }).collect();
        let stem = if stem.trim_matches('_').is_empty() { "asset".to_owned() } else { stem };
        let path = (0..).map(|i| if i == 0 { dir.join(format!("{stem}.json")) } else { dir.join(format!("{stem}-{i}.json")) }).find(|p| !p.exists()).expect("unbounded");
        let text = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        self.assets.push(Asset { name: file.name, category: file.category, builtin: false, path: Some(path), content });
        Ok(self.assets.len() - 1)
    }

    /// Deletes a saved asset (its file too). Built-in assets stay.
    pub fn delete(&mut self, index: usize) -> Result<(), String> {
        let a = self.assets.get(index).ok_or("no such asset")?;
        if a.builtin {
            return Err("built-in assets can't be deleted".into());
        }
        if let Some(p) = &a.path {
            std::fs::remove_file(p).map_err(|e| e.to_string())?;
        }
        self.assets.remove(index);
        Ok(())
    }

    pub fn categories(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for a in &self.assets {
            if !v.contains(&a.category) {
                v.push(a.category.clone());
            }
        }
        v
    }
}

fn read(path: &Path) -> Result<Asset, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let file: AssetFile = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if file.format != FORMAT {
        return Err("not a Forma3D asset".into());
    }
    let content = match file.kind {
        AssetKind::Object => Content::Scene(Scene::from_json(file.scene.as_deref().ok_or("asset has no objects")?)?),
        AssetKind::Material => Content::Material(file.color.ok_or("material asset has no colour")?),
    };
    Ok(Asset { name: file.name, category: file.category, builtin: false, path: Some(path.to_path_buf()), content })
}

// ---------------------------------------------------------------------------
// Built-in assets
// ---------------------------------------------------------------------------

fn part(s: &mut Scene, parent: ObjectId, name: &str, p: Primitive, pos: Vec3, material: Option<u64>) -> ObjectId {
    let id = s.add_object(name, ObjectKind::Primitive(p), Some(parent));
    if let Some(o) = s.get_mut(id) {
        o.pos = pos;
        o.material = material;
    }
    id
}

fn cube(x: f32, y: f32, z: f32) -> Primitive {
    Primitive::Cube { size: Vec3::new(x, y, z), segments: [1, 1, 1] }
}

fn cylinder(r: f32, h: f32) -> Primitive {
    Primitive::Cylinder { radius: r, height: h, segments: 24, hseg: 1, caps: true }
}

fn sphere(r: f32) -> Primitive {
    Primitive::Sphere { radius: r, segments: 24 }
}

fn group(name: &str) -> (Scene, ObjectId) {
    let mut s = Scene::new();
    let g = s.add_null(None);
    if let Some(o) = s.get_mut(g) {
        o.name = name.to_owned();
    }
    (s, g)
}

fn object(name: &str, category: &str, scene: Scene) -> Asset {
    Asset { name: name.into(), category: category.into(), builtin: true, path: None, content: Content::Scene(scene) }
}

fn material(name: &str, color: [f32; 3]) -> Asset {
    Asset { name: name.into(), category: "Materials".into(), builtin: true, path: None, content: Content::Material(color) }
}

pub fn builtin() -> Vec<Asset> {
    let mut v = Vec::new();

    // Furniture.
    let (mut s, g) = group("Table");
    let wood = Some(s.add_material("Wood", [0.62, 0.43, 0.27]));
    part(&mut s, g, "Top", cube(200.0, 8.0, 120.0), Vec3::new(0.0, 75.0, 0.0), wood);
    for (i, (x, z)) in [(-90.0, -50.0), (90.0, -50.0), (-90.0, 50.0), (90.0, 50.0)].into_iter().enumerate() {
        part(&mut s, g, &format!("Leg {}", i + 1), cylinder(4.0, 71.0), Vec3::new(x, 35.5, z), wood);
    }
    v.push(object("Table", "Furniture", s));

    let (mut s, g) = group("Chair");
    let wood = Some(s.add_material("Wood", [0.62, 0.43, 0.27]));
    part(&mut s, g, "Seat", cube(46.0, 5.0, 46.0), Vec3::new(0.0, 45.0, 0.0), wood);
    part(&mut s, g, "Back", cube(46.0, 46.0, 4.0), Vec3::new(0.0, 70.5, -21.0), wood);
    for (i, (x, z)) in [(-20.0, -20.0), (20.0, -20.0), (-20.0, 20.0), (20.0, 20.0)].into_iter().enumerate() {
        part(&mut s, g, &format!("Leg {}", i + 1), cube(4.0, 42.5, 4.0), Vec3::new(x, 21.25, z), wood);
    }
    v.push(object("Chair", "Furniture", s));

    let (mut s, g) = group("Shelf");
    let white = Some(s.add_material("Ivory", [0.92, 0.90, 0.84]));
    for i in 0..5 {
        part(&mut s, g, &format!("Board {}", i + 1), cube(100.0, 3.0, 30.0), Vec3::new(0.0, 1.5 + i as f32 * 45.0, 0.0), white);
    }
    for (i, x) in [-51.5, 51.5].into_iter().enumerate() {
        part(&mut s, g, &format!("Side {}", i + 1), cube(3.0, 183.0, 30.0), Vec3::new(x, 91.5, 0.0), white);
    }
    v.push(object("Shelf", "Furniture", s));

    // Nature.
    let (mut s, g) = group("Tree");
    let bark = Some(s.add_material("Bark", [0.40, 0.27, 0.17]));
    let leaves = Some(s.add_material("Leaves", [0.30, 0.58, 0.28]));
    part(&mut s, g, "Trunk", cylinder(12.0, 110.0), Vec3::new(0.0, 55.0, 0.0), bark);
    part(&mut s, g, "Crown", Primitive::Cone { radius: 70.0, height: 170.0, segments: 24, hseg: 1, caps: true }, Vec3::new(0.0, 185.0, 0.0), leaves);
    v.push(object("Tree", "Nature", s));

    let (mut s, g) = group("Rock");
    let stone = Some(s.add_material("Stone", [0.52, 0.52, 0.50]));
    let rock = part(&mut s, g, "Rock", Primitive::Sphere { radius: 40.0, segments: 8 }, Vec3::new(0.0, 22.0, 0.0), stone);
    if let Some(o) = s.get_mut(rock) {
        o.scale = Vec3::new(1.4, 0.7, 1.0);
        o.rot = Vec3::new(20.0, 0.0, 8.0);
    }
    v.push(object("Rock", "Nature", s));

    let (mut s, g) = group("Snowman");
    let snow = Some(s.add_material("Snow", [0.95, 0.96, 0.98]));
    let carrot = Some(s.add_material("Carrot", [0.95, 0.50, 0.15]));
    part(&mut s, g, "Base", sphere(50.0), Vec3::new(0.0, 50.0, 0.0), snow);
    part(&mut s, g, "Body", sphere(35.0), Vec3::new(0.0, 128.0, 0.0), snow);
    part(&mut s, g, "Head", sphere(23.0), Vec3::new(0.0, 182.0, 0.0), snow);
    let nose = part(&mut s, g, "Nose", Primitive::Cone { radius: 4.0, height: 18.0, segments: 12, hseg: 1, caps: true }, Vec3::new(0.0, 182.0, -30.0), carrot);
    if let Some(o) = s.get_mut(nose) {
        o.rot = Vec3::new(0.0, -90.0, 0.0);
    }
    v.push(object("Snowman", "Nature", s));

    // Architecture.
    let (mut s, g) = group("Stairs");
    let concrete = Some(s.add_material("Concrete", [0.66, 0.65, 0.62]));
    for i in 0..8 {
        let h = 18.0 * (i + 1) as f32;
        part(&mut s, g, &format!("Step {}", i + 1), cube(100.0, h, 28.0), Vec3::new(0.0, h * 0.5, i as f32 * 28.0), concrete);
    }
    v.push(object("Stairs", "Architecture", s));

    let (mut s, g) = group("Column");
    let marble = Some(s.add_material("Marble", [0.90, 0.89, 0.86]));
    part(&mut s, g, "Base", cube(70.0, 15.0, 70.0), Vec3::new(0.0, 7.5, 0.0), marble);
    part(&mut s, g, "Shaft", cylinder(25.0, 270.0), Vec3::new(0.0, 150.0, 0.0), marble);
    part(&mut s, g, "Capital", cube(70.0, 15.0, 70.0), Vec3::new(0.0, 292.5, 0.0), marble);
    v.push(object("Column", "Architecture", s));

    let (mut s, g) = group("Arch Doorway");
    let brick = Some(s.add_material("Brick", [0.70, 0.33, 0.24]));
    part(&mut s, g, "Left Wall", cube(60.0, 220.0, 30.0), Vec3::new(-80.0, 110.0, 0.0), brick);
    part(&mut s, g, "Right Wall", cube(60.0, 220.0, 30.0), Vec3::new(80.0, 110.0, 0.0), brick);
    part(&mut s, g, "Lintel", cube(220.0, 40.0, 30.0), Vec3::new(0.0, 240.0, 0.0), brick);
    v.push(object("Arch Doorway", "Architecture", s));

    // Basics: a few ready shapes at a common size.
    let (mut s, g) = group("Pedestal");
    let grey = Some(s.add_material("Studio Grey", [0.55, 0.55, 0.57]));
    part(&mut s, g, "Pedestal", cylinder(60.0, 90.0), Vec3::new(0.0, 45.0, 0.0), grey);
    v.push(object("Pedestal", "Studio", s));

    let (mut s, g) = group("Backdrop");
    let grey = Some(s.add_material("Studio White", [0.88, 0.88, 0.90]));
    part(&mut s, g, "Floor", Primitive::Plane { width: 800.0, height: 600.0, wseg: 1, hseg: 1 }, Vec3::ZERO, grey);
    part(&mut s, g, "Wall", cube(800.0, 400.0, 2.0), Vec3::new(0.0, 200.0, 300.0), grey);
    v.push(object("Backdrop", "Studio", s));

    for (name, c) in [
        ("Gold", [0.95, 0.74, 0.30]),
        ("Copper", [0.82, 0.48, 0.32]),
        ("Chrome", [0.80, 0.81, 0.84]),
        ("Brick", [0.70, 0.33, 0.24]),
        ("Concrete", [0.66, 0.65, 0.62]),
        ("Oak", [0.67, 0.50, 0.31]),
        ("Walnut", [0.40, 0.27, 0.18]),
        ("Grass", [0.36, 0.62, 0.30]),
        ("Ocean", [0.18, 0.42, 0.66]),
        ("Plastic Red", [0.85, 0.18, 0.16]),
        ("Ivory", [0.92, 0.90, 0.84]),
        ("Charcoal", [0.16, 0.16, 0.17]),
        ("Lavender", [0.66, 0.58, 0.84]),
        ("Mint", [0.58, 0.86, 0.72]),
    ] {
        v.push(material(name, c));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_valid_scenes_and_materials() {
        let lib = Library::load(None);
        assert!(lib.assets.iter().filter(|a| a.kind() == AssetKind::Object).count() >= 8);
        assert!(lib.assets.iter().filter(|a| a.kind() == AssetKind::Material).count() >= 10);
        for a in &lib.assets {
            if let Content::Scene(s) = &a.content {
                // Each is one group, and survives a save / load round trip.
                assert_eq!(s.roots().len(), 1, "{}", a.name);
                Scene::from_json(&s.to_json().unwrap()).unwrap();
            }
        }
    }

    #[test]
    fn saving_loading_and_deleting_user_assets() {
        let dir = std::env::temp_dir().join(format!("forma3d-assets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut lib = Library::load(Some(dir.clone()));
        let builtins = lib.assets.len();
        let i = lib.save_scene("My Cube!", Scene::demo()).unwrap();
        lib.save_material("Teal", [0.1, 0.6, 0.6]).unwrap();
        assert!(lib.assets[i].path.as_ref().unwrap().exists());
        // A fresh library finds both files.
        let again = Library::load(Some(dir.clone()));
        assert_eq!(again.assets.len(), builtins + 2);
        assert!(again.errors.is_empty());
        let names: Vec<&str> = again.assets.iter().filter(|a| !a.builtin).map(|a| a.name.as_str()).collect();
        assert!(names.contains(&"My Cube!") && names.contains(&"Teal"));
        // Built-ins can't be deleted; saved ones can.
        assert!(lib.delete(0).is_err());
        lib.delete(i).unwrap();
        assert_eq!(Library::load(Some(dir.clone())).assets.len(), builtins + 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
