//! Slow meshes (Booleans, deep subdivisions, anything that took longer than
//! a frame) are built on the desktop backend's worker threads: the object
//! keeps showing its previous shape until the new one arrives, so dragging
//! a slider stays smooth. In a browser they are still built here.

use crate::state::App;
use forma3d_core::editor::{Editor, MeshJob};
use forma3d_core::mesh::{Mesh, Vertex};
use leptos::prelude::*;

fn now_ms() -> f64 {
    crate::host::now_ms()
}

/// Lets this project hand its slow meshes to the backend.
pub fn install(app: App, ed: &mut Editor) {
    ed.meshes.set_offload(Box::new(move |job: MeshJob| send(app, job)), now_ms);
}

fn send(app: App, job: MeshJob) {
    let MeshJob { key, object, scene } = job;
    leptos::task::spawn_local(async move {
        let json = match scene.to_json() {
            Ok(j) => j,
            Err(e) => return give_back(app, key, object, None, 0.0, Some(e.to_string())),
        };
        match crate::host::build_mesh(json, object).await {
            Ok(built) => {
                let ms = built.as_ref().map_or(0.0, |b| b.ms);
                give_back(app, key, object, built.map(to_mesh), ms, None);
            }
            Err(e) => give_back(app, key, object, None, 0.0, Some(e)),
        }
    });
}

fn to_mesh(b: crate::host::BuiltMesh) -> Mesh {
    let vertices = b
        .positions
        .chunks_exact(3)
        .zip(b.normals.chunks_exact(3))
        .map(|(p, n)| Vertex { pos: [p[0], p[1], p[2]], normal: [n[0], n[1], n[2]] })
        .collect();
    Mesh::from_parts(vertices, b.indices, b.edges)
}

fn give_back(app: App, key: u64, object: u64, mesh: Option<Mesh>, ms: f64, error: Option<String>) {
    if let Some(e) = error {
        app.log_line(format!("A mesh couldn't be built in the background: {e}"), true);
    }
    let mut shown = false;
    let mut mesh = mesh;
    app.each_project_mut(|e| {
        if !shown && e.meshes.waiting_for(object, key) {
            shown = e.meshes.finish(key, object, mesh.take(), ms);
        }
    });
    if shown {
        // Redraw with the new shape (and pick up a newer state if the
        // object changed meanwhile).
        app.view_rev.update(|r| *r += 1);
    }
}
