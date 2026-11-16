//! The Object Manager: the scene hierarchy. Click to select (Shift / Ctrl to
//! add), double-click to rename, drag onto another object to parent it.

use crate::commands::{self, Cmd};
use crate::icons::{icon_s, path};
use crate::state::app;
use forma3d_core::scene::{DropTarget, ObjectId, Visibility};
use leptos::prelude::*;

#[derive(Clone, PartialEq)]
struct Row {
    id: ObjectId,
    depth: usize,
    name: String,
    icon: &'static str,
    has_kids: bool,
    open: bool,
    selected: bool,
    active: bool,
    enabled: bool,
    vis: (Visibility, Visibility),
    dimmed: bool,
    material: Option<[f32; 3]>,
}

pub fn rgb(c: [f32; 3]) -> String {
    format!("rgb({},{},{})", (c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)
}

fn vis_class(v: Visibility) -> &'static str {
    match v {
        Visibility::Default => "dot",
        Visibility::On => "dot on",
        Visibility::Off => "dot off",
    }
}

#[component]
pub fn ObjectManager() -> impl IntoView {
    let app = app();
    let drag_id = StoredValue::new(None::<ObjectId>);
    let drop_on = RwSignal::new(None::<ObjectId>);

    let rows = move || {
        app.read(|ed| {
            let mut out = Vec::new();
            fn walk(ed: &forma3d_core::editor::Editor, parent: Option<ObjectId>, depth: usize, out: &mut Vec<Row>) {
                for &id in ed.scene.children_of(parent) {
                    if !ed.scene.in_manager(id) {
                        continue;
                    }
                    let Some(o) = ed.scene.get(id) else { continue };
                    let kids = !ed.scene.children_of(Some(id)).is_empty();
                    let open = ed.expanded.contains(&id);
                    out.push(Row {
                        id,
                        depth,
                        name: o.name.clone(),
                        icon: crate::icons::for_kind(&o.kind),
                        has_kids: kids,
                        open,
                        selected: ed.is_selected(id),
                        active: ed.active() == Some(id),
                        enabled: o.enabled,
                        vis: (o.vis_editor, o.vis_render),
                        dimmed: !ed.scene.editor_visible(id),
                        material: o.material.and_then(|m| ed.scene.material(m)).map(|m| m.color),
                    });
                    if kids && open {
                        walk(ed, Some(id), depth + 1, out);
                    }
                }
            }
            walk(ed, None, 0, &mut out);
            out
        })
    };

    let row_view = move |r: Row| {
        let id = r.id;
        let renaming = move || app.renaming.get() == Some(id);
        let name = r.name.clone();
        view! {
            <div
                class="row"
                class:sel=r.selected
                class:active=r.active
                class:drop=move || drop_on.get() == Some(id)
                style=format!("padding-left:{}px", 4 + r.depth * 16)
                draggable="true"
                on:click=move |ev: web_sys::MouseEvent| {
                    let add = ev.shift_key() || ev.ctrl_key() || ev.meta_key();
                    app.act(|e| if add { e.toggle_selection(id) } else { e.select_only(id) });
                }
                on:dblclick=move |_| app.renaming.set(Some(id))
                on:dragstart=move |ev: web_sys::DragEvent| {
                    drag_id.set_value(Some(id));
                    if let Some(dt) = ev.data_transfer() {
                        let _ = dt.set_data("text/plain", &id.to_string());
                        dt.set_effect_allowed("move");
                    }
                }
                on:dragover=move |ev: web_sys::DragEvent| {
                    let material = ev.data_transfer().is_some_and(|dt| dt.types().includes(&"application/x-forma3d-material".into(), 0));
                    if material || drag_id.get_value().is_some_and(|d| d != id) {
                        ev.prevent_default();
                        drop_on.set(Some(id));
                    }
                }
                on:dragleave=move |_| drop_on.set(None)
                on:drop=move |ev: web_sys::DragEvent| {
                    ev.prevent_default();
                    ev.stop_propagation();
                    drop_on.set(None);
                    // A material from the Material Manager goes onto the object.
                    if let Some(m) = ev
                        .data_transfer()
                        .and_then(|dt| dt.get_data("application/x-forma3d-material").ok())
                        .and_then(|s| s.parse::<u64>().ok())
                    {
                        app.act(|e| {
                            let targets = if e.is_selected(id) { e.selection.clone() } else { vec![id] };
                            e.assign_material(&targets, Some(m));
                        });
                        return;
                    }
                    if let Some(src) = drag_id.get_value() {
                        drag_id.set_value(None);
                        app.act(|e| {
                            let ids = if e.is_selected(src) { e.scene.top_level(&e.selection) } else { vec![src] };
                            let mut moved = 0;
                            for s in ids {
                                if e.scene.move_object(s, DropTarget::Into(id)) {
                                    moved += 1;
                                }
                            }
                            if moved > 0 {
                                e.expanded.insert(id);
                                e.label("Move in Hierarchy");
                            }
                        });
                    }
                }
            >
                <span
                    class="twist"
                    on:click=move |ev| {
                        ev.stop_propagation();
                        app.view(|e| {
                            if !e.expanded.remove(&id) {
                                e.expanded.insert(id);
                            }
                        });
                        app.rev.update(|r| *r += 1);
                    }
                >
                    {r.has_kids.then(|| view! {
                        <svg viewBox="0 0 24 24" inner_html=path(if r.open { "chev-down" } else { "chev-right" })></svg>
                    })}
                </span>
                <span class:dim=r.dimmed>{icon_s(r.icon)}</span>
                <span class="nm" class:dim=r.dimmed>
                    {move || {
                        let name = name.clone();
                        if renaming() {
                            let node = NodeRef::<leptos::html::Input>::new();
                            Effect::new(move |_| {
                                if let Some(i) = node.get() {
                                    let _ = i.focus();
                                    i.select();
                                }
                            });
                            let commit = move |v: String| {
                                app.renaming.set(None);
                                if !v.trim().is_empty() {
                                    app.act(|e| {
                                        if let Some(o) = e.scene.get_mut(id)
                                            && o.name != v.trim()
                                        {
                                            o.name = v.trim().to_owned();
                                            e.label("Rename");
                                        }
                                    });
                                }
                            };
                            view! {
                                <input
                                    node_ref=node
                                    value=name
                                    on:click=|ev| ev.stop_propagation()
                                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                                        ev.stop_propagation();
                                        match ev.key().as_str() {
                                            "Enter" => commit(event_target_value(&ev)),
                                            "Escape" => app.renaming.set(None),
                                            _ => {}
                                        }
                                    }
                                    on:blur=move |ev| commit(event_target_value(&ev))
                                />
                            }
                            .into_any()
                        } else {
                            view! { <span>{name}</span> }.into_any()
                        }
                    }}
                </span>
                {r.material.map(|c| view! {
                    <span class="mtag" title="Material" style=format!("background:radial-gradient(circle at 35% 30%, #fff8 0, {} 45%)", rgb(c))></span>
                })}
                <span class="dots" title="Visible in editor / renderer: grey default, green on, red off (click to cycle)">
                    <i
                        class=vis_class(r.vis.0)
                        on:click=move |ev| {
                            ev.stop_propagation();
                            app.act(|e| {
                                if let Some(o) = e.scene.get_mut(id) {
                                    o.vis_editor = o.vis_editor.cycled();
                                }
                                e.label("Change Visibility");
                            });
                        }
                    ></i>
                    <i
                        class=vis_class(r.vis.1)
                        on:click=move |ev| {
                            ev.stop_propagation();
                            app.act(|e| {
                                if let Some(o) = e.scene.get_mut(id) {
                                    o.vis_render = o.vis_render.cycled();
                                }
                                e.label("Change Visibility");
                            });
                        }
                    ></i>
                </span>
                <span
                    class="chk"
                    class:x=!r.enabled
                    title=if r.enabled { "Enabled (click to switch off)" } else { "Switched off (click to enable)" }
                    on:click=move |ev| {
                        ev.stop_propagation();
                        app.act(|e| {
                            if let Some(o) = e.scene.get_mut(id) {
                                o.enabled = !o.enabled;
                            }
                            e.label("Enable / Disable");
                        });
                    }
                >
                    {if r.enabled { "✓" } else { "✕" }}
                </span>
            </div>
        }
    };

    view! {
        <div class="bar">
            <button class="tb" style="width:26px;height:24px" title="New Null" on:click=move |_| commands::run(app, Cmd::Null)>{icon_s("null")}</button>
            <button class="tb" style="width:26px;height:24px" title="Group Objects (Alt+G)" on:click=move |_| commands::run(app, Cmd::Group)>{icon_s("group")}</button>
            <button class="tb" style="width:26px;height:24px" title="Duplicate (Ctrl+D)" on:click=move |_| commands::run(app, Cmd::Duplicate)>{icon_s("duplicate")}</button>
            <button class="tb" style="width:26px;height:24px" title="Delete (Del)" on:click=move |_| commands::run(app, Cmd::Delete)>{icon_s("trash")}</button>
        </div>
        <div
            class="content tree"
            on:click=move |ev: web_sys::MouseEvent| {
                if ev.target() == ev.current_target() {
                    app.act(|e| e.clear_selection());
                }
            }
            on:dragover=move |ev: web_sys::DragEvent| ev.prevent_default()
            on:drop=move |ev: web_sys::DragEvent| {
                // Dropped on empty space: back to the top level.
                ev.prevent_default();
                if let Some(src) = drag_id.get_value() {
                    drag_id.set_value(None);
                    app.act(|e| {
                        let ids = if e.is_selected(src) { e.scene.top_level(&e.selection) } else { vec![src] };
                        for s in ids {
                            e.scene.move_object(s, DropTarget::Root);
                        }
                        e.label("Move in Hierarchy");
                    });
                }
            }
        >
            {move || {
                let rows = rows();
                if rows.is_empty() {
                    view! { <div class="empty">"The scene is empty." <br /> "Add a shape from the toolbar or the Create menu."</div> }.into_any()
                } else {
                    rows.into_iter().map(row_view).collect_view().into_any()
                }
            }}
        </div>
    }
}
