//! The Layer, Take and Structure Managers, and the panels of the Track, UV
//! Edit and Sculpt workspaces.

use crate::icons::icon_s;
use crate::objects::rgb;
use crate::state::app;
use forma3d_core::component::{EditMode, SelectOp, mesh_of, mesh_of_mut};
use forma3d_core::layers::{LayerFlag, LayerId};
use leptos::prelude::*;

/// A small menu in a panel's header bar.
#[component]
fn BarMenu(id: &'static str, label: &'static str, children: ChildrenFn) -> impl IntoView {
    let app = app();
    let open = move || app.menu.get() == Some(id);
    view! {
        <div style="position:relative">
            <button
                class="menu-btn"
                class:open=open
                on:mousedown=move |ev| {
                    ev.stop_propagation();
                    app.menu.update(|m| *m = if *m == Some(id) { None } else { Some(id) });
                }
            >
                {label}
            </button>
            <Show when=open>
                <div class="dropdown" on:mousedown=|ev| ev.stop_propagation() on:click=move |_| app.menu.set(None)>
                    {children()}
                </div>
            </Show>
        </div>
    }
}

/// A plain menu row.
fn item(label: &'static str, enabled: Signal<bool>, run: impl Fn() + 'static) -> impl IntoView {
    view! {
        <div class="mi" class:off=move || !enabled.get() on:click=move |_| {
            if enabled.get_untracked() {
                run()
            }
        }>
            <span class="chk"></span>
            <span class="ico"></span>
            {label}
        </div>
    }
}

fn hex(c: [f32; 3]) -> String {
    crate::fields::rgb_hex(c)
}

// ---------------------------------------------------------------------------
// Layers
// ---------------------------------------------------------------------------

#[component]
pub fn Layers() -> impl IntoView {
    let app = app();
    let selected = RwSignal::new(None::<LayerId>);
    let renaming = RwSignal::new(None::<LayerId>);
    let drop_on = RwSignal::new(None::<LayerId>);
    let ctx = RwSignal::new(None::<(LayerId, i32, i32)>);
    let has_sel = Signal::derive(move || {
        selected.get().is_some()
            && app.read(|e| selected.get().is_some_and(|l| e.scene.layer(l).is_some()))
    });
    let always = Signal::derive(|| true);
    let any_solo = Signal::derive(move || app.read(|e| e.scene.layers().iter().any(|l| l.solo)));
    let new_layer = move || {
        let id = app.act(|e| {
            let id = e.scene.add_layer("Layer");
            e.label("New Layer");
            id
        });
        selected.set(Some(id));
    };
    let set_all = move |f: LayerFlag, on: bool, label: &'static str| {
        app.act(|e| {
            let ids: Vec<LayerId> = e.scene.layers().iter().map(|l| l.id).collect();
            for id in ids {
                if let Some(l) = e.scene.layer_mut(id) {
                    f.set(l, on);
                }
            }
            e.label(label);
        })
    };
    let rows = move || {
        app.read(|e| {
            e.scene
                .layers()
                .iter()
                .map(|l| (l.clone(), e.scene.layer_members(l.id).len()))
                .collect::<Vec<_>>()
        })
    };
    view! {
        <div class="bar">
            <BarMenu id="layer-menu" label="Layer">
                {item("New Layer", always, new_layer)}
                <crate::chrome::MenuEntries entries=vec![crate::chrome::Entry::C(crate::commands::Cmd::NewLayerFromSelection)] />
                <div class="msep"></div>
                {item("Add Selected Objects to Layer", has_sel, move || if let Some(l) = selected.get_untracked() {
                    app.act(|e| {
                        let sel = e.selection.clone();
                        e.scene.set_layer(&sel, Some(l));
                        e.label("Set Layer");
                    })
                })}
                {item("Select Objects on Layer", has_sel, move || if let Some(l) = selected.get_untracked() {
                    app.act(|e| e.selection = e.scene.layer_members(l))
                })}
                <div class="msep"></div>
                {item("Rename Layer", has_sel, move || renaming.set(selected.get_untracked()))}
                {item("Delete Layer", has_sel, move || if let Some(l) = selected.get_untracked() {
                    selected.set(None);
                    app.act(|e| {
                        e.scene.remove_layer(l);
                        e.label("Delete Layer");
                    })
                })}
            </BarMenu>
            <BarMenu id="layer-edit" label="Edit">
                {item("Solo Off (all layers)", any_solo, move || set_all(LayerFlag::Solo, false, "Solo Off"))}
                {item("View On (all layers)", always, move || set_all(LayerFlag::View, true, "Layer Switches"))}
                {item("Render On (all layers)", always, move || set_all(LayerFlag::Render, true, "Layer Switches"))}
                {item("Manager On (all layers)", always, move || set_all(LayerFlag::Manager, true, "Layer Switches"))}
                {item("Unlock All", always, move || set_all(LayerFlag::Locked, false, "Unlock Layers"))}
            </BarMenu>
        </div>
        <div class="lhead">
            <span class="lname">"Name"</span>
            {LayerFlag::ALL.map(|f| view! { <span class="lflag" title=f.tip()>{f.letter()}</span> }).collect_view()}
            <span class="lcount">"#"</span>
        </div>
        <div class="content" on:click=move |ev: web_sys::MouseEvent| {
            if ev.target() == ev.current_target() {
                selected.set(None)
            }
        }>
            {move || {
                let rows = rows();
                if rows.is_empty() {
                    return view! { <div class="empty">"No layers yet." <br /> "Layer > New Layer, or select objects and use Add to New Layer." <br /> "Drag objects from the Object Manager onto a layer to put them on it."</div> }.into_any();
                }
                rows.into_iter()
                    .map(|(l, count)| {
                        let lid = l.id;
                        let name = l.name.clone();
                        view! {
                            <div
                                class="lrow"
                                class:sel=move || selected.get() == Some(lid)
                                class:drop=move || drop_on.get() == Some(lid)
                                on:click=move |_| selected.set(Some(lid))
                                on:dblclick=move |_| renaming.set(Some(lid))
                                on:contextmenu=move |ev: web_sys::MouseEvent| {
                                    ev.prevent_default();
                                    ev.stop_propagation();
                                    selected.set(Some(lid));
                                    ctx.set(Some((lid, ev.client_x(), ev.client_y())));
                                }
                                on:dragover=move |ev: web_sys::DragEvent| {
                                    if ev.data_transfer().is_some_and(|dt| dt.types().includes(&"application/x-forma3d-object".into(), 0)) {
                                        ev.prevent_default();
                                        drop_on.set(Some(lid));
                                    }
                                }
                                on:dragleave=move |_| drop_on.set(None)
                                on:drop=move |ev: web_sys::DragEvent| {
                                    ev.prevent_default();
                                    drop_on.set(None);
                                    if let Some(obj) = ev.data_transfer().and_then(|dt| dt.get_data("application/x-forma3d-object").ok()).and_then(|s| s.parse::<u64>().ok()) {
                                        app.act(|e| {
                                            let targets = if e.is_selected(obj) { e.selection.clone() } else { vec![obj] };
                                            e.scene.set_layer(&targets, Some(lid));
                                            e.label("Set Layer");
                                        });
                                    }
                                }
                            >
                                <input
                                    type="color"
                                    class="color lswatch"
                                    title="Layer colour"
                                    value=hex(l.color)
                                    on:click=|ev| ev.stop_propagation()
                                    on:input=move |ev| {
                                        if let Some(c) = crate::fields::parse_hex(&event_target_value(&ev)) {
                                            app.gesture(|e| if let Some(l) = e.scene.layer_mut(lid) { l.color = c });
                                        }
                                    }
                                    on:change=move |_| app.act(|e| e.label("Layer Colour"))
                                />
                                {move || {
                                    if renaming.get() == Some(lid) {
                                        let commit = move |v: String| {
                                            renaming.set(None);
                                            if !v.trim().is_empty() {
                                                app.act(|e| {
                                                    if let Some(l) = e.scene.layer_mut(lid) {
                                                        l.name = v.trim().to_owned();
                                                    }
                                                    e.label("Rename Layer");
                                                });
                                            }
                                        };
                                        view! {
                                            <input
                                                class="lname-in"
                                                value=name.clone()
                                                autofocus
                                                on:click=|ev| ev.stop_propagation()
                                                on:keydown=move |ev: web_sys::KeyboardEvent| {
                                                    ev.stop_propagation();
                                                    match ev.key().as_str() {
                                                        "Enter" => commit(event_target_value(&ev)),
                                                        "Escape" => renaming.set(None),
                                                        _ => {}
                                                    }
                                                }
                                                on:blur=move |ev| commit(event_target_value(&ev))
                                            />
                                        }
                                        .into_any()
                                    } else {
                                        view! { <span class="lname">{name.clone()}</span> }.into_any()
                                    }
                                }}
                                {LayerFlag::ALL
                                    .map(|f| {
                                        let on = f.get(&l);
                                        let style = match (f, on) {
                                            (LayerFlag::Locked, true) => "background:#c85a46;color:#fff".to_owned(),
                                            (LayerFlag::Solo, true) => "background:var(--accent);color:#000".to_owned(),
                                            (_, true) => format!("background:{};color:#000", rgb(l.color)),
                                            (_, false) => String::new(),
                                        };
                                        view! {
                                            <span class="lflag">
                                                <i
                                                    class="lbox"
                                                    style=style
                                                    title=f.tip()
                                                    on:click=move |ev| {
                                                        ev.stop_propagation();
                                                        app.act(|e| {
                                                            if let Some(l) = e.scene.layer_mut(lid) {
                                                                f.set(l, !on);
                                                            }
                                                            e.label(format!("Layer {}", f.label()));
                                                        });
                                                    }
                                                >
                                                    {f.letter()}
                                                </i>
                                            </span>
                                        }
                                    })
                                    .collect_view()}
                                <span class="lcount">{count}</span>
                            </div>
                        }
                    })
                    .collect_view()
                    .into_any()
            }}
        </div>
        {move || ctx.get().map(|(lid, x, y)| {
            let close = move || ctx.set(None);
            view! {
                <div class="scrim" on:mousedown=move |_| close()></div>
                <div class="dropdown fixed" style=format!("left:{x}px;top:{y}px") on:mousedown=|ev| ev.stop_propagation() on:click=move |_| close()>
                    {item("Add Selected Objects", always, move || app.act(|e| {
                        let sel = e.selection.clone();
                        e.scene.set_layer(&sel, Some(lid));
                        e.label("Set Layer");
                    }))}
                    {item("Select Objects", always, move || app.act(|e| e.selection = e.scene.layer_members(lid)))}
                    {item("Rename", always, move || renaming.set(Some(lid)))}
                    <div class="msep"></div>
                    {item("Move Up", always, move || app.act(|e| {
                        e.scene.move_layer(lid, true);
                        e.label("Move Layer");
                    }))}
                    {item("Move Down", always, move || app.act(|e| {
                        e.scene.move_layer(lid, false);
                        e.label("Move Layer");
                    }))}
                    <div class="msep"></div>
                    {item("Delete Layer", always, move || {
                        selected.set(None);
                        app.act(|e| {
                            e.scene.remove_layer(lid);
                            e.label("Delete Layer");
                        })
                    })}
                </div>
            }
        })}
    }
}

// ---------------------------------------------------------------------------
// Takes
// ---------------------------------------------------------------------------

#[component]
pub fn Takes() -> impl IntoView {
    let app = app();
    let renaming = RwSignal::new(None::<usize>);
    let ctx = RwSignal::new(None::<(usize, i32, i32)>);
    let active = move || app.read(|e| e.scene.active_take());
    let not_main = Signal::derive(move || active() > 0);
    let can_clear = Signal::derive(move || active() > 0 && app.read(|e| !e.selection.is_empty()));
    let always = Signal::derive(|| true);
    let takes = move || {
        app.read(|e| {
            e.scene
                .takes()
                .takes
                .iter()
                .enumerate()
                .map(|(i, t)| (i, t.name.clone(), e.scene.take_override_count(i)))
                .collect::<Vec<_>>()
        })
    };
    let overrides = move || {
        app.read(|e| {
            let a = e.scene.active_take();
            e.scene
                .take_overrides(a)
                .into_iter()
                .filter_map(|(id, ov)| {
                    Some((
                        id,
                        e.scene.get(id)?.name.clone(),
                        ov.fields().join(", "),
                        e.is_selected(id),
                    ))
                })
                .collect::<Vec<_>>()
        })
    };
    view! {
        <div class="bar">
            <BarMenu id="take-menu" label="Take">
                <crate::chrome::MenuEntries entries=vec![crate::chrome::Entry::C(crate::commands::Cmd::NewTake)] />
                {item("Clone Take", not_main, move || app.act(|e| {
                    let a = e.scene.active_take();
                    e.scene.clone_take(a);
                    e.label("Clone Take");
                }))}
                {item("Rename Take", not_main, move || renaming.set(Some(app.peek(|e| e.scene.active_take()))))}
                <div class="msep"></div>
                {item("Delete Take", not_main, move || app.act(|e| {
                    let a = e.scene.active_take();
                    e.scene.remove_take(a);
                    e.label("Delete Take");
                }))}
            </BarMenu>
            <BarMenu id="take-overrides" label="Overrides">
                {item("Remove Overrides from Selected Objects", can_clear, move || app.act(|e| {
                    let sel = e.selection.clone();
                    e.scene.clear_take_overrides(&sel);
                    e.label("Remove Overrides");
                }))}
                {item("Remove All Overrides of this Take", not_main, move || app.act(|e| {
                    let a = e.scene.active_take();
                    let ids: Vec<u64> = e.scene.take_overrides(a).into_iter().map(|(id, _)| id).collect();
                    e.scene.clear_take_overrides(&ids);
                    e.label("Remove Overrides");
                }))}
            </BarMenu>
        </div>
        <div class="content">
            {move || {
                let a = active();
                takes()
                    .into_iter()
                    .map(|(i, name, count)| {
                        let is_active = i == a;
                        let label = match count {
                            _ if i == 0 => String::new(),
                            0 => "no overrides".to_owned(),
                            1 => "1 override".to_owned(),
                            n => format!("{n} overrides"),
                        };
                        let name2 = name.clone();
                        view! {
                            <div
                                class="trow"
                                class:sel=is_active
                                style=format!("padding-left:{}px", if i == 0 { 8 } else { 24 })
                                on:click=move |_| {
                                    if !is_active {
                                        let n = name2.clone();
                                        app.act(|e| {
                                            e.scene.set_active_take(i);
                                            e.label("Switch Take");
                                            e.status = format!("Take \"{n}\" active");
                                        })
                                    }
                                }
                                on:dblclick=move |_| {
                                    if i > 0 {
                                        renaming.set(Some(i));
                                    }
                                }
                                on:contextmenu=move |ev: web_sys::MouseEvent| {
                                    ev.prevent_default();
                                    ctx.set(Some((i, ev.client_x(), ev.client_y())));
                                }
                            >
                                <i class="tmark" class:on=is_active></i>
                                {icon_s("takes")}
                                {move || {
                                    if renaming.get() == Some(i) && i > 0 {
                                        let commit = move |v: String| {
                                            renaming.set(None);
                                            if !v.trim().is_empty() {
                                                app.act(|e| {
                                                    e.scene.rename_take(i, v.trim());
                                                    e.label("Rename Take");
                                                });
                                            }
                                        };
                                        view! {
                                            <input
                                                class="lname-in"
                                                value=name.clone()
                                                autofocus
                                                on:click=|ev| ev.stop_propagation()
                                                on:keydown=move |ev: web_sys::KeyboardEvent| {
                                                    ev.stop_propagation();
                                                    match ev.key().as_str() {
                                                        "Enter" => commit(event_target_value(&ev)),
                                                        "Escape" => renaming.set(None),
                                                        _ => {}
                                                    }
                                                }
                                                on:blur=move |ev| commit(event_target_value(&ev))
                                            />
                                        }
                                        .into_any()
                                    } else {
                                        view! { <span class="tname">{name.clone()}</span> }.into_any()
                                    }
                                }}
                                <span class="tcount">{label}</span>
                            </div>
                        }
                    })
                    .collect_view()
            }}
            {move || {
                if active() == 0 {
                    view! { <p class="note">"Main is the scene itself: changes here apply to every take that doesn't override them. Add a take (Take > New Take), then change objects to record its overrides."</p> }.into_any()
                } else {
                    let list = overrides();
                    view! {
                        <h4 class="thead">"Overrides of the active take"</h4>
                        {if list.is_empty() {
                            view! { <p class="note">"None yet: move, hide or recolour objects to record overrides for this take."</p> }.into_any()
                        } else {
                            list.into_iter()
                                .map(|(id, name, fields, sel)| view! {
                                    <div class="orow" class:sel=sel on:click=move |_| app.act(|e| e.select_only(id))>
                                        <b>{name}</b>
                                        <span class="hint">{fields}</span>
                                    </div>
                                })
                                .collect_view()
                                .into_any()
                        }}
                    }
                    .into_any()
                }
            }}
        </div>
        {move || ctx.get().map(|(i, x, y)| {
            let close = move || ctx.set(None);
            let main = Signal::derive(move || i > 0);
            view! {
                <div class="scrim" on:mousedown=move |_| close()></div>
                <div class="dropdown fixed" style=format!("left:{x}px;top:{y}px") on:mousedown=|ev| ev.stop_propagation() on:click=move |_| close()>
                    {item("Make Active", always, move || app.act(|e| {
                        e.scene.set_active_take(i);
                        e.label("Switch Take");
                    }))}
                    {item("Rename", main, move || renaming.set(Some(i)))}
                    {item("Clone", main, move || app.act(|e| {
                        e.scene.clone_take(i);
                        e.label("Clone Take");
                    }))}
                    <div class="msep"></div>
                    {item("Delete", main, move || app.act(|e| {
                        e.scene.remove_take(i);
                        e.label("Delete Take");
                    }))}
                </div>
            }
        })}
    }
}

// ---------------------------------------------------------------------------
// Structure
// ---------------------------------------------------------------------------

const ROW_H: f64 = 24.0;

#[component]
pub fn Structure() -> impl IntoView {
    let app = app();
    let polys = RwSignal::new(app.peek(|e| e.mode == EditMode::Polygons));
    let scroll = RwSignal::new(0.0f64);
    let height = RwSignal::new(400.0f64);
    let target = move || app.read(|e| e.edit_target());
    let info = move || {
        app.read(|e| {
            let id = e.edit_target()?;
            let m = mesh_of(&e.scene, id)?;
            Some((
                e.scene.get(id).map(|o| o.name.clone()).unwrap_or_default(),
                m.points.len(),
                m.polys.len(),
            ))
        })
    };
    let rows = move || {
        let Some((_, np, npoly)) = info() else {
            return Vec::new();
        };
        let n = if polys.get() { npoly } else { np };
        let first = (scroll.get() / ROW_H).floor().max(0.0) as usize;
        let count = (height.get() / ROW_H).ceil() as usize + 4;
        (first..(first + count).min(n)).collect::<Vec<_>>()
    };
    let total = move || info().map_or(0, |(_, np, npoly)| if polys.get() { npoly } else { np });
    let select = move |i: u32, ev: &web_sys::MouseEvent| {
        let op = if ev.ctrl_key() || ev.meta_key() {
            SelectOp::Remove
        } else if ev.shift_key() {
            SelectOp::Add
        } else {
            SelectOp::Replace
        };
        let poly = polys.get_untracked();
        app.act(|e| {
            if poly {
                if e.mode != EditMode::Polygons {
                    e.set_mode(EditMode::Polygons);
                }
                e.comp.apply_polys([i], op);
            } else {
                if e.mode != EditMode::Points {
                    e.set_mode(EditMode::Points);
                }
                e.comp.apply_points([i], op);
            }
        });
    };
    view! {
        <div class="bar">
            <div class="seg">
                <span class:on=move || !polys.get() on:click=move |_| polys.set(false)>"Points"</span>
                <span class:on=move || polys.get() on:click=move |_| polys.set(true)>"Polygons"</span>
            </div>
            <span class="hint" style="margin-left:auto;align-self:center">
                {move || info().map(|(n, a, b)| format!("{n}: {a} points, {b} polygons")).unwrap_or_default()}
            </span>
        </div>
        {move || match target() {
            None => view! { <div class="content"><div class="empty">"Select a polygon object to see its points and polygons." <br /> "Press C to make a primitive editable."</div></div> }.into_any(),
            Some(id) => view! {
                <div class="shead">
                    <span class="sidx">"Index"</span>
                    {move || (if polys.get() { vec!["A", "B", "C", "D"] } else { vec!["X", "Y", "Z"] }).into_iter().map(|h| view! { <span class="scol">{h}</span> }).collect_view()}
                </div>
                <div
                    class="content"
                    on:scroll=move |ev| {
                        let el: web_sys::HtmlElement = event_target(&ev);
                        scroll.set(el.scroll_top() as f64);
                        height.set(el.client_height() as f64);
                    }
                >
                    <div style=move || format!("height:{}px;position:relative", total() as f64 * ROW_H)>
                        {move || {
                            let poly = polys.get();
                            rows()
                                .into_iter()
                                .map(|i| {
                                    let sel = app.read(|e| if poly { e.comp.polys.contains(&(i as u32)) } else { e.comp.points.contains(&(i as u32)) });
                                    let cells = app.read(|e| {
                                        let m = mesh_of(&e.scene, id)?;
                                        Some(if poly {
                                            m.polys.get(i).map(|p| p.iter().map(|v| *v as f32).collect::<Vec<_>>()).unwrap_or_default()
                                        } else {
                                            m.points.get(i).map(|p| vec![p.x, p.y, p.z]).unwrap_or_default()
                                        })
                                    }).unwrap_or_default();
                                    let n = cells.len();
                                    let shown = if poly { 4 } else { 3 };
                                    view! {
                                        <div class="srow" style=format!("top:{}px", i as f64 * ROW_H)>
                                            <span class="sidx" class:sel=sel on:click=move |ev| select(i as u32, &ev)>{i}</span>
                                            {(0..shown)
                                                .map(|k| {
                                                    if k >= n {
                                                        return view! { <span class="scol dim">"–"</span> }.into_any();
                                                    }
                                                    let v = cells[k];
                                                    let text = if poly { format!("{}", v as u32) } else { format!("{v:.3}") };
                                                    view! {
                                                        <span class="scol">
                                                            <input
                                                                class="text-in cell"
                                                                value=text
                                                                on:keydown=|ev: web_sys::KeyboardEvent| {
                                                                    ev.stop_propagation();
                                                                    if ev.key() == "Enter" {
                                                                        let el: web_sys::HtmlInputElement = event_target(&ev);
                                                                        let _ = el.blur();
                                                                    }
                                                                }
                                                                on:change=move |ev| {
                                                                    let Ok(val) = event_target_value(&ev).trim().trim_end_matches("cm").trim().parse::<f32>() else { return };
                                                                    app.act(|e| {
                                                                        let Some(m) = mesh_of_mut(&mut e.scene, id) else { return };
                                                                        if poly {
                                                                            let max = m.points.len().saturating_sub(1) as u32;
                                                                            let Some(p) = m.polys.get_mut(i) else { return };
                                                                            let v = (val.max(0.0) as u32).min(max);
                                                                            // A polygon can't use the same point twice.
                                                                            if p.iter().enumerate().any(|(j, x)| j != k && *x == v) {
                                                                                e.status = "A polygon can't use the same point twice".to_owned();
                                                                                return;
                                                                            }
                                                                            p[k] = v;
                                                                            e.label("Edit Polygon");
                                                                        } else if let Some(p) = m.points.get_mut(i) {
                                                                            p[k] = val;
                                                                            e.label("Edit Point");
                                                                        }
                                                                    });
                                                                }
                                                            />
                                                        </span>
                                                    }
                                                    .into_any()
                                                })
                                                .collect_view()}
                                            {(poly && n > 4).then(|| view! { <span class="hint">{format!("+{}", n - 4)}</span> })}
                                        </div>
                                    }
                                })
                                .collect_view()
                        }}
                    </div>
                </div>
            }
            .into_any(),
        }}
    }
}

// ---------------------------------------------------------------------------
// Workspace panels
// ---------------------------------------------------------------------------

/// The UV Editor: the 0-1 UV square, ready for UV islands.
#[component]
pub fn UvEditor() -> impl IntoView {
    let app = app();
    let name = move || {
        app.read(|e| {
            e.selection
                .first()
                .and_then(|&id| e.scene.get(id))
                .map(|o| o.name.clone())
        })
    };
    let grid: String = (0..=10)
        .map(|i| {
            let t = i as f32 * 10.0;
            let c = if i == 0 || i == 5 || i == 10 {
                "#555a63"
            } else {
                "#3a3e45"
            };
            format!(r#"<path d="M{t} 0V100M0 {t}H100" stroke="{c}" stroke-width="0.35"/>"#)
        })
        .collect();
    view! {
        <div class="content uv">
            <div class="uvbox">
                <svg viewBox="-8 -4 112 112" inner_html=format!(
                    r##"<rect x="0" y="0" width="100" height="100" fill="#3a3c41"/>{grid}<rect x="0" y="0" width="100" height="100" fill="none" stroke="#8b909a" stroke-width="0.5"/><text x="-3" y="103" font-size="4" fill="#8b909a" text-anchor="end">0</text><text x="100" y="105" font-size="4" fill="#8b909a" text-anchor="middle">1</text><text x="-3" y="2" font-size="4" fill="#8b909a" text-anchor="end">1</text><text x="50" y="106" font-size="4.5" fill="#e5534b" text-anchor="middle">U</text><text x="-5" y="51" font-size="4.5" fill="#57ab5a" text-anchor="middle">V</text>"##
                )></svg>
                <div class="note card">
                    <b>"UV Editor"</b>
                    <br />
                    {move || match name() {
                        Some(n) => format!("\"{n}\" has no UV map: Forma3D meshes don't carry UV coordinates yet. When UV unwrapping is added, its UV islands will appear on this square to edit."),
                        None => "Forma3D meshes don't carry UV coordinates yet. When UV unwrapping is added, the selected object's UV islands will appear on this square to edit.".to_owned(),
                    }}
                </div>
            </div>
        </div>
    }
}

/// The Footage panel for camera tracking.
#[component]
pub fn Footage() -> impl IntoView {
    view! {
        <div class="content footage">
            <div class="film">
                <div class="note card">
                    <b>"No footage"</b>
                    <br />
                    "Camera tracking isn't built yet. This panel will hold the video clip, its tracking points and the camera solve; the solved camera then drives the viewport."
                </div>
            </div>
            <div class="ftools">
                <button class="btn" disabled>"Load Footage..."</button>
                <button class="btn" disabled>"Add Track"</button>
                <button class="btn" disabled>"Solve Camera"</button>
            </div>
        </div>
    }
}

/// Sculpt brushes and their settings (sculpting itself isn't built yet).
#[component]
pub fn Brushes() -> impl IntoView {
    view! {
        <div class="content brushes">
            <p class="note">"Sculpting isn't built yet. These are the brushes it will have; they turn on when sculpting is added."</p>
            <div class="bgrid">
                {["Draw", "Smooth", "Grab", "Inflate", "Pinch", "Flatten", "Crease", "Mask"].map(|b| view! { <button class="btn" disabled>{b}</button> }).collect_view()}
            </div>
            <div class="prow"><span class="plabel">"Radius"</span><input type="range" disabled value="50" /></div>
            <div class="prow"><span class="plabel">"Strength"</span><input type="range" disabled value="50" /></div>
            <label class="check"><input type="checkbox" disabled checked />"X Symmetry"</label>
        </div>
    }
}
