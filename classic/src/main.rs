#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// A few small helpers exist for tests and upcoming features.
#![allow(dead_code)]

// The engine lives in forma3d-core; re-exporting its modules here keeps the
// interface's `crate::scene::...` paths working.
pub use forma3d_core::*;

mod icons;
mod theme;
mod ui;
mod view_callback;

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
