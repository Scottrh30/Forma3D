//! Your own assets: objects and materials saved to the Asset Browser (Save
//! Selection to Asset Browser), kept in the settings as `assets.json`.

use forma3d_core::assets::{Asset, AssetKind, Content};
use forma3d_core::scene::Scene;
use serde::{Deserialize, Serialize};

pub const FILE: &str = "assets.json";
pub const CATEGORY: &str = "My Assets";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub name: String,
    pub kind: AssetKind,
    /// A Forma3D scene file holding the objects.
    #[serde(default)]
    pub scene: Option<String>,
    #[serde(default)]
    pub color: Option<[f32; 3]>,
    /// A saved material's full settings (reflection, picture, glow...).
    #[serde(default)]
    pub look: Option<forma3d_core::scene::Material>,
}

impl Saved {
    pub fn objects(name: &str, scene: &Scene) -> Result<Saved, String> {
        Ok(Saved { name: name.to_owned(), kind: AssetKind::Object, scene: Some(scene.to_json().map_err(|e| e.to_string())?), color: None, look: None })
    }

    pub fn material(name: &str, look: &forma3d_core::scene::Material) -> Saved {
        Saved { name: name.to_owned(), kind: AssetKind::Material, scene: None, color: Some(look.color), look: Some(look.clone()) }
    }

    /// As an Asset Browser entry (None if it can't be read).
    pub fn asset(&self) -> Option<Asset> {
        let content = match self.kind {
            AssetKind::Object => Content::Scene(Scene::from_json(self.scene.as_deref()?).ok()?),
            AssetKind::Material => Content::Material(self.color?),
        };
        Some(Asset { name: self.name.clone(), category: CATEGORY.to_owned(), builtin: false, path: None, content })
    }
}

/// A name not used by another saved asset ("Cube", "Cube 2", ...).
pub fn unique_name(base: &str, taken: &[Saved]) -> String {
    let base = if base.trim().is_empty() { "Asset" } else { base.trim() };
    (1..)
        .map(|i| if i == 1 { base.to_owned() } else { format!("{base} {i}") })
        .find(|n| !taken.iter().any(|s| s.name == *n))
        .expect("unbounded")
}

#[cfg(test)]
mod tests {
    use super::*;
    use forma3d_core::primitive::PrimitiveKind;

    #[test]
    fn round_trips_objects_and_materials() {
        let mut scene = Scene::new();
        scene.add_primitive(PrimitiveKind::Cube, None);
        let s = Saved::objects("Box", &scene).unwrap();
        let back: Vec<Saved> = serde_json::from_str(&serde_json::to_string(&vec![s.clone(), Saved::material("Red", &forma3d_core::scene::Material::new(1, "Red", [1.0, 0.0, 0.0]))]).unwrap()).unwrap();
        assert_eq!(back[0], s);
        match back[0].asset().unwrap().content {
            Content::Scene(sc) => assert_eq!(sc.len(), scene.len()),
            Content::Material(_) => panic!("objects expected"),
        }
        assert!(matches!(back[1].asset().unwrap().content, Content::Material(c) if c == [1.0, 0.0, 0.0]));
        assert_eq!(unique_name("Box", &back), "Box 2");
        assert_eq!(unique_name(" ", &[]), "Asset");
    }
}
