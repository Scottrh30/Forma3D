#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// A few small helpers exist for tests and upcoming features.
#![allow(dead_code)]

mod assets;
mod camera;
mod component;
mod csg;
mod deformer;
mod editmesh;
mod editor;
mod generator;
mod gizmo;
mod history;
mod icons;
mod mesh;
mod meshops;
mod picking;
mod primitive;
mod renderer;
mod scene;
mod spline;
mod light;
mod layers;
mod snap;
mod takes;
mod pen;
mod tessellate;
mod theme;
mod ui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1480.0, 900.0])
            .with_min_inner_size([900.0, 560.0])
            .with_title("Forma3D"),
        renderer: eframe::Renderer::Wgpu,
        depth_buffer: 32,
        ..Default::default()
    };
    eframe::run_native("Forma3D", options, Box::new(|cc| Ok(Box::new(ui::App::new(cc)))))
}
