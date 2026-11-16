//! The lower panels: Materials, Asset Browser, Console and Coordinates.

use crate::commands::{self, Cmd};
use crate::fields::{NumField, parse_hex, rgb_hex};
use crate::icons::icon_s;
use crate::objects::rgb;
use crate::state::{BottomTab, app};
use forma3d_core::assets::{Content, builtin};
use forma3d_core::scene::MaterialId;
use leptos::prelude::*;

fn ball(c: [f32; 3]) -> String {
    let hi = [(c[0] + 0.55).min(1.0), (c[1] + 0.55).min(1.0), (c[2] + 0.55).min(1.0)];
    let lo = [c[0] * 0.45, c[1] * 0.45, c[2] * 0.45];
    let dk = [c[0] * 0.18, c[1] * 0.18, c[2] * 0.18];
    format!("background:radial-gradient(circle at 34% 30%, {} 0, {} 24%, {} 64%, {} 100%)", rgb(hi), rgb(c), rgb(lo), rgb(dk))
}

#[component]
fn Materials() -> impl IntoView {
    let app = app();
    let renaming = RwSignal::new(None::<MaterialId>);
    let mats = move || app.read(|e| e.scene.materials().iter().map(|m| (m.id, m.name.clone(), m.color)).collect::<Vec<_>>());
    let selected = move || app.read(|e| e.selected_material);
    let sel_color = move || app.read(|e| e.selected_material.and_then(|m| e.scene.material(m)).map(|m| m.color));
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
fn Assets() -> impl IntoView {
    let app = app();
    let lib = StoredValue::new_local(builtin());
    let search = RwSignal::new(String::new());
    let shown = move || {
        let q = search.get().to_lowercase();
        lib.with_value(|l| {
            l.iter()
                .enumerate()
                .filter(|(_, a)| q.is_empty() || a.haystack().contains(&q))
                .map(|(i, a)| {
                    let (mat, ic) = match &a.content {
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
                    (i, a.name.clone(), mat, ic)
                })
                .collect::<Vec<_>>()
        })
    };
    let insert = move |i: usize| {
        let content = lib.with_value(|l| l.get(i).map(|a| (a.name.clone(), a.content.clone())));
        let Some((name, content)) = content else { return };
        app.act(|e| match content {
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
    view! {
        <div class="bar">
            <span style="color:var(--dim);align-self:center">"Built-in objects and materials · click to add"</span>
            <input class="searchin" placeholder="Search assets" on:keydown=|ev: web_sys::KeyboardEvent| ev.stop_propagation() on:input=move |ev| search.set(event_target_value(&ev)) />
        </div>
        <div class="content">
            <div class="assets">
                {move || {
                    shown()
                        .into_iter()
                        .map(|(i, name, mat, ic)| {
                            let tip = format!("Add {name}");
                            view! {
                                <div class="asset" title=tip on:click=move |_| insert(i)>
                                    {match mat {
                                        Some(c) => view! { <div class="sw" style=ball(c)></div> }.into_any(),
                                        None => crate::icons::icon(ic).into_any(),
                                    }}
                                    <small>{name}</small>
                                </div>
                            }
                        })
                        .collect_view()
                }}
            </div>
        </div>
    }
}

#[component]
fn Console() -> impl IntoView {
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
        let words: Vec<&str> = t.split_whitespace().collect();
        let hits: Vec<Cmd> = commands::all()
            .into_iter()
            .filter(|c| {
                let l = commands::info(*c).label.to_lowercase();
                l == t || words.iter().all(|w| l.contains(w))
            })
            .collect();
        let exact = hits.iter().copied().find(|c| commands::info(*c).label.to_lowercase() == t);
        match (exact, hits.len()) {
            (Some(_), _) | (None, 1) => {
                let c = exact.unwrap_or(hits[0]);
                app.log_line(format!("running {}", commands::info(c).label), false);
                commands::run(app, c);
            }
            (None, 0) => app.log_line(format!("no command matches \"{}\"", text.trim()), true),
            (None, n) => app.log_line(
                format!("{n} commands match: {}", hits.iter().take(6).map(|c| commands::info(*c).label).collect::<Vec<_>>().join(", ")),
                true,
            ),
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
fn Coordinates() -> impl IntoView {
    let app = app();
    let active = move || app.read(|e| e.active());
    view! {
        <div class="tabs"><div class="tab on">"Coordinates"</div></div>
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

#[component]
pub fn Bottom() -> impl IntoView {
    let app = app();
    let tab = app.bottom;
    let t = move |which: BottomTab, label: &'static str| {
        view! { <div class="tab" class:on=move || tab.get() == which on:click=move |_| tab.set(which)>{label}</div> }
    };
    view! {
        <div class="bottom">
            <div class="panel">
                <div class="tabs">
                    {t(BottomTab::Materials, "Materials")}
                    {t(BottomTab::Assets, "Asset Browser")}
                    {t(BottomTab::Console, "Console")}
                </div>
                {move || match tab.get() {
                    BottomTab::Materials => view! { <Materials /> }.into_any(),
                    BottomTab::Assets => view! { <Assets /> }.into_any(),
                    BottomTab::Console => view! { <Console /> }.into_any(),
                }}
            </div>
            <div class="panel">
                <Coordinates />
            </div>
        </div>
    }
}

