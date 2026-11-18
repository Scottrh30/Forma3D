//! Forma3D's engine: the scene, meshes and their tools, the editor state with
//! undo, and the wgpu viewport renderer. The interfaces (the Tauri + Leptos
//! app) is built on top of it.

// A few small helpers exist for tests and upcoming features.
#![allow(dead_code)]

pub mod assets;
pub mod camera;
pub mod component;
pub mod csg;
pub mod deformer;
pub mod editmesh;
pub mod editor;
pub mod generator;
pub mod gizmo;
pub mod history;
pub mod layers;
pub mod light;
pub mod mesh;
pub mod meshops;
pub mod pen;
pub mod picking;
pub mod primitive;
pub mod renderer;
pub mod render;
pub mod scene;
pub mod scene_camera;
pub mod sky;
pub mod snap;
pub mod spline;
pub mod takes;
pub mod texture;
pub mod tessellate;
