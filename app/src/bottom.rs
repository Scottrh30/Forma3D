//! The lower panels: Materials, Asset Browser, Console and Coordinates.

use crate::commands::{self, Cmd};
use crate::fields::{NumField, parse_hex, rgb_hex};
use crate::icons::icon_s;
use crate::objects::rgb;
use crate::state::app;
use forma3d_core::assets::{Content, builtin};
use forma3d_core::scene::MaterialId;
use leptos::prelude::*;

fn ball(c: [f32; 3]) -> String {
    let hi = [
        (c[0] + 0.55).min(1.0),
        (c[1] + 0.55).min(1.0),
        (c[2] + 0.55).min(1.0),
    ];
    let lo = [c[0] * 0.45, c[1] * 0.45, c[2] * 0.45];
    let dk = [c[0] * 0.18, c[1] * 0.18, c[2] * 0.18];
    format!(
        "background:radial-gradient(circle at 34% 30%, {} 0, {} 24%, {} 64%, {} 100%)",
        rgb(hi),
        rgb(c),
        rgb(lo),
        rgb(dk)
    )
}

#[component]
pub fn Materials() -> impl IntoView {
    let app = app();
    let renaming = RwSignal::new(None::<MaterialId>);
    let mats = move || {
        app.read(|e| {
            e.scene
                .materials()
                .iter()
                .map(|m| (m.id, m.name.clone(), m.color))
                .collect::<Vec<_>>()
        })
    };
    let selected = move || app.read(|e| e.selected_material);
    let sel_color = move || {
        app.read(|e| {
            e.selected_material
                .and_then(|m| e.scene.material(m))
                .map(|m| m.color)
        })
    };
    view! {
        <div class="content">
            <div class="mats">
                {move || {
                    mats()
                        .into_iter()
                        .map(|(id, name, c)| {
                            view! {
                                <div
                                    class="mat"
                                    class:on=move || selected() == Some(id)
                                    title="Click to select · double-click to rename · drag onto an object"
                                    draggable="true"
                                    on:dragstart=move |ev: web_sys::DragEvent| {
                                        if let Some(dt) = ev.data_transfer() {
                                            let _ = dt.set_data("application/x-forma3d-material", &id.to_string());
                                            let _ = dt.set_data("text/plain", &format!("material:{id}"));
                                        }
                                    }
                                    on:click=move |_| app.act(|e| e.selected_material = Some(id))
                                    on:dblclick=move |_| renaming.set(Some(id))
                                >
                                    <div class="ball" style=ball(c)></div>
                                    {move || {
                                        let name = name.clone();
                                        if renaming.get() == Some(id) {
                                            view! {
                                                <input
                                                    value=name
                                                    autofocus
                                                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                                                        ev.stop_propagation();
                                                        if ev.key() == "Enter" || ev.key() == "Escape" {
                                                            let v = event_target_value(&ev);
                                                            renaming.set(None);
                                                            if ev.key() == "Enter" && !v.trim().is_empty() {
                                                                app.act(|e| {
                                                                    if let Some(m) = e.scene.material_mut(id) {
                                                                        m.name = v.trim().to_owned();
                                                                    }
                                                                    e.label("Rename Material");
                                                                });
                                                            }
                                                        }
                                                    }
                                                    on:blur=move |_| renaming.set(None)
                                                />
                                            }
                                            .into_any()
                                        } else {
                                            view! { <small>{name}</small> }.into_any()
                                        }
                                    }}
                                </div>
                            }
                        })
                        .collect_view()
                }}
                <div class="mat" title="New material (goes on the selected objects)" on:click=move |_| commands::run(app, Cmd::NewMaterial)>
                    <div class="add">"+"</div>
                    <small style="color:var(--dim)">"New"</small>
                </div>
            </div>
            {move || sel_color().map(|c| {
                view! {
                    <div class="matedit">
                        "Colour"
                        <input
                            type="color"
                            class="color"
                            value=rgb_hex(c)
                            on:input=move |ev| {
                                if let Some(c) = parse_hex(&event_target_value(&ev)) {
                                    app.gesture(|e| {
                                        if let Some(m) = e.selected_material.and_then(|m| e.scene.material_mut(m)) {
                                            m.color = c;
                                        }
                                    });
                                }
                            }
                            on:change=move |_| app.act(|e| e.label("Material Colour"))
                        />
                        <button class="btn" on:click=move |_| app.act(|e| {
                            let sel = e.selection.clone();
                            let m = e.selected_material;
                            e.assign_material(&sel, m);
                        })>"Assign to Selection"</button>
                        <button class="btn" on:click=move |_| app.act(|e| {
                            if let Some(m) = e.selected_material.take() {
                                e.scene.remove_material(m);
                                e.label("Delete Material");
                            }
                        })>{icon_s("trash")}"Delete"</button>
                    </div>
                }
            })}
        </div>
    }
}

#[component]
pub fn Assets() -> impl IntoView {
    let app = app();
    let builtins = StoredValue::new_local(builtin());
    let search = RwSignal::new(String::new());
    // Your own assets first, then the built-in ones; (user index, asset).
    let all = move || {
        let mut v: Vec<(Option<usize>, forma3d_core::assets::Asset)> = app
            .user_assets
            .with(|u| u.iter().enumerate().filter_map(|(i, s)| s.asset().map(|a| (Some(i), a))).collect());
        builtins.with_value(|b| v.extend(b.iter().cloned().map(|a| (None, a))));
        v
    };
    let insert = move |a: forma3d_core::assets::Asset| {
        let name = a.name.clone();
        app.act(|e| match a.content {
            Content::Scene(s) => {
                let ids = e.paste(&s);
                e.status = format!("Added {name} ({} object(s))", ids.len());
            }
            Content::Material(c) => {
                let m = e.scene.add_material(&name, c);
                e.selected_material = Some(m);
                if !e.selection.is_empty() {
                    let sel = e.selection.clone();
                    e.assign_material(&sel, Some(m));
                }
                e.status = format!("Added material {name}");
            }
        });
    };
    let tile_icon = |a: &forma3d_core::assets::Asset| match &a.content {
        Content::Material(c) => (Some(*c), "material"),
        Content::Scene(s) => {
            // The icon of the first real part.
            let mut ic = "group";
            let mut found = false;
            s.walk(|o, _| {
                if !found && !matches!(o.kind, forma3d_core::scene::ObjectKind::Null) {
                    ic = crate::icons::for_kind(&o.kind);
                    found = true;
                }
            });
            (None, ic)
        }
    };
    let tiles = move || {
        let q = search.get().to_lowercase();
        let items: Vec<_> = all().into_iter().filter(|(_, a)| q.is_empty() || a.haystack().contains(&q)).collect();
        let mut out = Vec::new();
        let mut last_mine = None;
        for (user, a) in items {
            let mine = user.is_some();
            if last_mine != Some(mine) {
                last_mine = Some(mine);
                out.push(view! { <div class="asset-head">{if mine { "My Assets" } else { "Built-in" }}</div> }.into_any());
            }
            let (mat, ic) = tile_icon(&a);
            let name = a.name.clone();
            let tip = format!("Add {name}");
            let del = user.map(|i| {
                view! {
                    <span
                        class="asset-x"
                        title="Delete from My Assets"
                        on:click=move |ev| {
                            ev.stop_propagation();
                            let gone = app.user_assets.with_untracked(|u| u.get(i).map(|s| s.name.clone()));
                            app.user_assets.update(|u| {
                                if i < u.len() {
                                    u.remove(i);
                                }
                            });
                            crate::settings::save_assets(app);
                            if let Some(n) = gone {
                                app.status(format!("Deleted {n} from My Assets"));
                            }
                        }
                    >
                        "×"
                    </span>
                }
            });
            out.push(
                view! {
                    <div class="asset" class:mine=mine title=tip on:click=move |_| insert(a.clone())>
                        {match mat {
                            Some(c) => view! { <div class="sw" style=ball(c)></div> }.into_any(),
                            None => crate::icons::icon(ic).into_any(),
                        }}
                        <small>{name}</small>
                        {del}
                    </div>
                }
                .into_any(),
            );
        }
        out
    };
    view! {
        <div class="bar">
            <button class="btn nowrap" title="Save the selected objects as an asset (click a tile to add it)" on:click=move |_| crate::commands::run(app, crate::commands::Cmd::SaveAsset)>
                "Save Selection"
            </button>
            <button class="btn nowrap" title="Save the selected material as an asset" on:click=move |_| crate::commands::run(app, crate::commands::Cmd::SaveMaterialAsset)>
                "Save Material"
            </button>
            <input class="searchin" placeholder="Search assets" on:keydown=|ev: web_sys::KeyboardEvent| ev.stop_propagation() on:input=move |ev| search.set(event_target_value(&ev)) />
        </div>
        <div class="content">
            <div class="assets">{tiles}</div>
        </div>
    }
}

#[component]
pub fn Console() -> impl IntoView {
    let app = app();
    let line = RwSignal::new(String::new());
    let run_line = move |text: String| {
        let t = text.trim().to_lowercase();
        if t.is_empty() {
            return;
        }
        app.log_line(format!("> {}", text.trim()), false);
        if t == "clear" {
            app.log.set(Vec::new());
            return;
        }
        if t == "help" {
            app.log_line("Type a command's name to run it (\"cube\", \"frame all\", \"undo\"); \"clear\" empties the log.", false);
            return;
        }
        let all = commands::infos();
        let exact = all.iter().find(|i| i.name.to_lowercase() == t);
        let hits = commands::search(all, &t);
        match (exact, hits.len()) {
            (Some(i), _) => {
                app.log_line(format!("running {}", i.name), false);
                commands::run(app, i.cmd);
            }
            (None, 0) => app.log_line(format!("no command matches \"{}\"", text.trim()), true),
            (None, _) => {
                let best = hits[0];
                app.log_line(format!("running {}", best.name), false);
                commands::run(app, best.cmd);
            }
        }
    };
    view! {
        <div class="content log">
            {move || {
                app.log
                    .get()
                    .into_iter()
                    .rev()
                    .take(300)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .map(|e| {
                        let s = e.time as u64;
                        view! {
                            <div class:err=e.error>
                                <span class="t">{format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)}</span>
                                {e.text}
                            </div>
                        }
                    })
                    .collect_view()
            }}
        </div>
        <div class="cmdline">
            <span style="color:var(--accent)">">"</span>
            <input
                placeholder="Type a command (cube, frame all, help) and press Enter"
                prop:value=move || line.get()
                on:input=move |ev| line.set(event_target_value(&ev))
                on:keydown=move |ev: web_sys::KeyboardEvent| {
                    ev.stop_propagation();
                    if ev.key() == "Enter" {
                        run_line(line.get_untracked());
                        line.set(String::new());
                    }
                }
            />
        </div>
    }
}

#[component]
pub fn Coordinates() -> impl IntoView {
    let app = app();
    let active = move || app.read(|e| e.active());
    view! {
        <div class="content">
            {move || match active() {
                None => view! { <div class="empty">"Select an object"</div> }.into_any(),
                Some(id) => {
                    let pos = move |i: usize| Signal::derive(move || app.read(|e| e.scene.world_position(id)[i]));
                    let size = move |i: usize| Signal::derive(move || app.ed.try_update_value(|e| { app.rev.track(); e.object_size(id).map_or(0.0, |s| s[i]) }).unwrap_or(0.0));
                    let rot = move |i: usize| Signal::derive(move || app.read(|e| e.scene.get(id).map_or(0.0, |o| o.rot[i])));
                    let set_pos = move |i: usize| Callback::new(move |v: f32| app.act(|e| {
                        let mut p = e.scene.world_position(id);
                        p[i] = v;
                        e.scene.set_world_position(id, p);
                        e.label("Move");
                    }));
                    let set_size = move |i: usize| Callback::new(move |v: f32| app.act(|e| {
                        if let Some(mut s) = e.object_size(id) {
                            s[i] = v.max(0.0);
                            e.set_object_size(id, s);
                            e.label("Size");
                        }
                    }));
                    let set_rot = move |i: usize| Callback::new(move |v: f32| app.act(|e| {
                        if let Some(o) = e.scene.get_mut(id) {
                            o.rot[i] = v;
                        }
                        e.label("Rotate");
                    }));
                    let c = ["var(--x)", "var(--y)", "var(--z)"];
                    view! {
                        <div class="coords">
                            <h4>"Position"</h4><h4>"Size"</h4><h4>"Rotation"</h4>
                            {(0..3).map(|i| view! {
                                <NumField value=pos(i) label=["X", "Y", "Z"][i] color=c[i] unit="cm" decimals=1 on_change=set_pos(i) />
                                <NumField value=size(i) label=["X", "Y", "Z"][i] color=c[i] unit="cm" decimals=1 on_change=set_size(i) />
                                <NumField value=rot(i) label=["H", "P", "B"][i] unit="°" decimals=1 step=0.5 on_change=set_rot(i) />
                            }).collect_view()}
                            <div class="coordsfoot"><span class="hint">"Position is in world space; size is the object's bounding box."</span></div>
                        </div>
                    }
                    .into_any()
                }
            }}
        </div>
    }
}
