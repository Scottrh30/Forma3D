//! The Attribute Manager: the active object's Basic, Coord. and Object
//! settings. Object settings are read straight from the engine's data, so
//! every primitive, generator, deformer and light gets its fields.

use crate::fields::{Choice, NumField, Toggle, rgb_hex, parse_hex};
use crate::icons::icon_s;
use crate::state::{AttrTab, app};
use forma3d_core::scene::{ObjectId, ObjectKind, Visibility};
use glam::Vec3;
use leptos::prelude::*;
use serde_json::Value;

fn kind_label(k: &ObjectKind) -> String {
    match k {
        ObjectKind::Null => "Null".into(),
        ObjectKind::Primitive(p) => format!("{} Object", p.kind().name()),
        ObjectKind::Generator(g) => g.kind().name().into(),
        ObjectKind::Deformer(d) => d.kind().name().into(),
        ObjectKind::Polygon(_) => "Polygon Object".into(),
        ObjectKind::Spline(_) => "Spline".into(),
        ObjectKind::Light(l) => format!("{} Light", l.kind.name()),
    }
}

fn pretty(name: &str) -> String {
    name.split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn unit_for(name: &str) -> &'static str {
    let n = name.to_lowercase();
    if ["radius", "size", "height", "width", "length", "offset", "distance"].iter().any(|k| n.contains(k)) {
        "cm"
    } else if ["angle", "direction", "rotation", "inner", "outer", "twist"].iter().any(|k| n.contains(k)) {
        "°"
    } else if ["strength", "curvature", "intensity", "roughness"].iter().any(|k| n.contains(k)) {
        "%"
    } else {
        ""
    }
}

/// The choices of the engine's small enums, as their serialized names.
fn options_for(v: &str) -> Option<Vec<String>> {
    use forma3d_core::csg::BoolMode;
    use forma3d_core::generator::{ArrayMode, MirrorPlane};
    use forma3d_core::light::LightKind;
    use forma3d_core::spline::SplineType;
    let names = |vals: Vec<Value>| vals.into_iter().filter_map(|v| v.as_str().map(str::to_owned)).collect::<Vec<_>>();
    let sets = [
        names(vec![serde_json::to_value(ArrayMode::Linear).ok()?, serde_json::to_value(ArrayMode::Radial).ok()?]),
        names(MirrorPlane::ALL.iter().filter_map(|m| serde_json::to_value(m).ok()).collect()),
        names(BoolMode::ALL.iter().filter_map(|m| serde_json::to_value(m).ok()).collect()),
        names(LightKind::ALL.iter().filter_map(|m| serde_json::to_value(m).ok()).collect()),
        names(SplineType::ALL.iter().filter_map(|m| serde_json::to_value(m).ok()).collect()),
    ];
    sets.into_iter().find(|s| s.iter().any(|x| x == v))
}

/// The object's own data as JSON fields, plus a way to write a field back.
fn object_json(k: &ObjectKind) -> Option<Value> {
    match k {
        ObjectKind::Primitive(p) => serde_json::to_value(p).ok(),
        ObjectKind::Generator(g) => serde_json::to_value(g).ok(),
        ObjectKind::Deformer(d) => serde_json::to_value(d).ok(),
        ObjectKind::Light(l) => serde_json::to_value(l).ok(),
        _ => None,
    }
}

fn write_json(k: &mut ObjectKind, v: Value) -> bool {
    match k {
        ObjectKind::Primitive(p) => serde_json::from_value::<forma3d_core::primitive::Primitive>(v).map(|n| *p = n.sanitized()).is_ok(),
        ObjectKind::Generator(g) => serde_json::from_value::<forma3d_core::generator::Generator>(v).map(|n| *g = n.sanitized()).is_ok(),
        ObjectKind::Deformer(d) => serde_json::from_value::<forma3d_core::deformer::Deformer>(v).map(|n| *d = n.sanitized()).is_ok(),
        ObjectKind::Light(l) => serde_json::from_value::<forma3d_core::light::Light>(v).map(|n| *l = n.sanitized()).is_ok(),
        _ => false,
    }
}

/// The map of fields inside the value: `{"Cube": {...}}` or a plain struct.
fn fields_of(v: &mut Value) -> Option<&mut serde_json::Map<String, Value>> {
    let obj = v.as_object_mut()?;
    if obj.len() == 1 && obj.values().next().is_some_and(|x| x.is_object()) {
        return obj.values_mut().next()?.as_object_mut();
    }
    Some(obj)
}

/// Sets one field (index `slot` for 3-vectors) and writes it back.
fn set_field(id: ObjectId, key: String, slot: Option<usize>, new: Value) {
    let app = app();
    app.act(|e| {
        let Some(o) = e.scene.get_mut(id) else { return };
        let Some(mut v) = object_json(&o.kind) else { return };
        let Some(f) = fields_of(&mut v) else { return };
        match (slot, f.get_mut(&key)) {
            (Some(i), Some(Value::Array(a))) if i < a.len() => a[i] = new,
            (None, Some(x)) => *x = new,
            _ => return,
        }
        if write_json(&mut o.kind, v) {
            e.label(format!("Change {}", pretty(&key)));
        }
    });
}

fn num_value(old: &Value, x: f64) -> Value {
    if old.is_u64() {
        Value::from(x.round().max(0.0) as u64)
    } else if old.is_i64() {
        Value::from(x.round() as i64)
    } else {
        serde_json::Number::from_f64(x).map(Value::Number).unwrap_or(Value::Null)
    }
}

#[component]
fn ObjectFields(id: ObjectId) -> impl IntoView {
    let app = app();
    move || {
        let json = app.read(|e| e.scene.get(id).and_then(|o| object_json(&o.kind)));
        let extra = app.read(|e| match e.scene.get(id).map(|o| &o.kind) {
            Some(ObjectKind::Polygon(m)) => Some(format!("{} points, {} polygons. Edit them in the classic app's Points, Edges and Polygons modes (coming to this interface next).", m.points.len(), m.polys.len())),
            Some(ObjectKind::Spline(s)) => Some(format!("{} points{}.", s.points.len(), if s.closed { ", closed" } else { "" })),
            Some(ObjectKind::Null) => Some("A null has no settings of its own: use it to group and move objects together.".into()),
            _ => None,
        });
        let mut json = json;
        let rows = json
            .as_mut()
            .and_then(fields_of)
            .map(|f| {
                f.iter()
                    .map(|(k, v)| {
                        let key = k.clone();
                        let label = pretty(k);
                        let unit = unit_for(k);
                        match v {
                            Value::Number(n) => {
                                let x = n.as_f64().unwrap_or(0.0) as f32;
                                let int = !v.is_f64();
                                let old = v.clone();
                                view! {
                                    <div class="lbl">
                                        <span>{label}</span>
                                        <NumField
                                            value=Signal::derive(move || x)
                                            unit=unit
                                            step=1.0
                                            decimals=if int { 0 } else { 2 }
                                            on_change=Callback::new(move |nv: f32| set_field(id, key.clone(), None, num_value(&old, nv as f64)))
                                        />
                                    </div>
                                }
                                .into_any()
                            }
                            Value::Bool(b) => {
                                let b = *b;
                                view! {
                                    <div class="lbl">
                                        <span>{label}</span>
                                        <Toggle on=Signal::derive(move || b) on_change=Callback::new(move |nb: bool| set_field(id, key.clone(), None, Value::Bool(nb))) />
                                    </div>
                                }
                                .into_any()
                            }
                            Value::Array(a) if a.len() == 3 && a.iter().all(|x| x.is_number()) => {
                                if k == "color" {
                                    let c = [a[0].as_f64().unwrap_or(1.0) as f32, a[1].as_f64().unwrap_or(1.0) as f32, a[2].as_f64().unwrap_or(1.0) as f32];
                                    view! {
                                        <div class="lbl">
                                            <span>{label}</span>
                                            <input
                                                type="color"
                                                class="color"
                                                value=rgb_hex(c)
                                                on:change=move |ev| {
                                                    if let Some(c) = parse_hex(&event_target_value(&ev)) {
                                                        let v: Value = serde_json::json!([c[0], c[1], c[2]]);
                                                        set_field(id, key.clone(), None, v);
                                                    }
                                                }
                                            />
                                        </div>
                                    }
                                    .into_any()
                                } else {
                                    let names = ["X", "Y", "Z"];
                                    let cells = (0..3)
                                        .map(|i| {
                                            let old = a[i].clone();
                                            let x = old.as_f64().unwrap_or(0.0) as f32;
                                            let key = key.clone();
                                            let int = !old.is_f64();
                                            view! {
                                                <NumField
                                                    value=Signal::derive(move || x)
                                                    label=names[i]
                                                    color=["var(--x)", "var(--y)", "var(--z)"][i]
                                                    decimals=if int { 0 } else { 1 }
                                                    on_change=Callback::new(move |nv: f32| set_field(id, key.clone(), Some(i), num_value(&old, nv as f64)))
                                                />
                                            }
                                        })
                                        .collect_view();
                                    let head = if unit.is_empty() { label } else { format!("{label} ({unit})") };
                                    view! {
                                        <div class="sec">{head}</div>
                                        <div class="grid3">{cells}</div>
                                    }
                                    .into_any()
                                }
                            }
                            Value::String(s) => {
                                let opts = options_for(s).unwrap_or_else(|| vec![s.clone()]);
                                let choices: Vec<(String, String)> = opts.iter().map(|o| (o.clone(), pretty(o))).collect();
                                let cur = s.clone();
                                view! {
                                    <div class="lbl">
                                        <span>{label}</span>
                                        <Choice
                                            options=choices
                                            value=Signal::derive(move || cur.clone())
                                            on_change=Callback::new(move |nv: String| set_field(id, key.clone(), None, Value::String(nv)))
                                        />
                                    </div>
                                }
                                .into_any()
                            }
                            _ => ().into_any(),
                        }
                    })
                    .collect_view()
            });
        view! {
            {rows}
            {extra.map(|t| view! { <p class="hint">{t}</p> })}
        }
    }
}

#[component]
fn Coord(id: ObjectId) -> impl IntoView {
    let app = app();
    let get = move |which: usize, i: usize| {
        Signal::derive(move || {
            app.read(|e| {
                e.scene.get(id).map_or(0.0, |o| {
                    let v = match which {
                        0 => o.pos,
                        1 => o.scale,
                        _ => o.rot,
                    };
                    v[i]
                })
            })
        })
    };
    let set = move |which: usize, i: usize| {
        Callback::new(move |x: f32| {
            app.act(|e| {
                if let Some(o) = e.scene.get_mut(id) {
                    let v: &mut Vec3 = match which {
                        0 => &mut o.pos,
                        1 => &mut o.scale,
                        _ => &mut o.rot,
                    };
                    v[i] = x;
                }
                e.label(["Move", "Scale", "Rotate"][which]);
            })
        })
    };
    let names = [["X", "Y", "Z"], ["X", "Y", "Z"], ["H", "P", "B"]];
    let colors = ["var(--x)", "var(--y)", "var(--z)"];
    let heads = ["Position (cm)", "Scale", "Rotation (°)"];
    let decimals = [1usize, 3, 1];
    let steps = [1.0f32, 0.01, 0.5];
    view! {
        {(0..3)
            .map(|which| {
                view! {
                    <div class="sec">{heads[which]}</div>
                    <div class="grid3">
                        {(0..3)
                            .map(|i| view! {
                                <NumField
                                    value=get(which, i)
                                    label=names[which][i]
                                    color=if which == 2 { "var(--text)" } else { colors[i] }
                                    decimals=decimals[which]
                                    step=steps[which]
                                    on_change=set(which, i)
                                />
                            })
                            .collect_view()}
                    </div>
                }
            })
            .collect_view()}
        <div style="margin-top:4px">
            <button class="btn" on:click=move |_| crate::commands::run(app, crate::commands::Cmd::ResetTransform)>"Reset Transform"</button>
        </div>
    }
}

#[component]
fn Basic(id: ObjectId) -> impl IntoView {
    let app = app();
    let name = move || app.read(|e| e.scene.get(id).map(|o| o.name.clone()).unwrap_or_default());
    let vis = |editor: bool| {
        Signal::derive(move || {
            app.read(|e| {
                let v = e.scene.get(id).map(|o| if editor { o.vis_editor } else { o.vis_render }).unwrap_or_default();
                format!("{v:?}")
            })
        })
    };
    let set_vis = move |editor: bool| {
        Callback::new(move |v: String| {
            let v = match v.as_str() {
                "On" => Visibility::On,
                "Off" => Visibility::Off,
                _ => Visibility::Default,
            };
            app.act(|e| {
                if let Some(o) = e.scene.get_mut(id) {
                    if editor {
                        o.vis_editor = v;
                    } else {
                        o.vis_render = v;
                    }
                }
                e.label("Change Visibility");
            });
        })
    };
    let vis_opts = || vec![("Default".to_owned(), "Default".to_owned()), ("On".to_owned(), "On".to_owned()), ("Off".to_owned(), "Off".to_owned())];
    let mats = move || {
        app.read(|e| {
            let mut v = vec![("none".to_owned(), "None".to_owned())];
            v.extend(e.scene.materials().iter().map(|m| (m.id.to_string(), m.name.clone())));
            v
        })
    };
    let mat = Signal::derive(move || app.read(|e| e.scene.get(id).and_then(|o| o.material).map_or("none".to_owned(), |m| m.to_string())));
    let enabled = Signal::derive(move || app.read(|e| e.scene.get(id).is_some_and(|o| o.enabled)));
    view! {
        <div class="lbl">
            <span>"Name"</span>
            <input
                class="text-in"
                prop:value=name
                on:keydown=|ev: web_sys::KeyboardEvent| ev.stop_propagation()
                on:change=move |ev| {
                    let v = event_target_value(&ev);
                    if !v.trim().is_empty() {
                        app.act(|e| {
                            if let Some(o) = e.scene.get_mut(id) {
                                o.name = v.trim().to_owned();
                            }
                            e.label("Rename");
                        });
                    }
                }
            />
        </div>
        <div class="lbl"><span>"Visible in Editor"</span><Choice options=vis_opts() value=vis(true) on_change=set_vis(true) /></div>
        <div class="lbl"><span>"Visible in Renderer"</span><Choice options=vis_opts() value=vis(false) on_change=set_vis(false) /></div>
        <div class="lbl">
            <span>"Enabled"</span>
            <Toggle on=enabled on_change=Callback::new(move |b: bool| app.act(|e| {
                if let Some(o) = e.scene.get_mut(id) {
                    o.enabled = b;
                }
                e.label("Enable / Disable");
            })) />
        </div>
        <div class="lbl">
            <span>"Material"</span>
            {move || view! {
                <Choice
                    options=mats()
                    value=mat
                    on_change=Callback::new(move |v: String| {
                        let m = v.parse::<u64>().ok();
                        app.act(|e| e.assign_material(&[id], m));
                    })
                />
            }}
        </div>
    }
}

#[component]
pub fn Attributes() -> impl IntoView {
    let app = app();
    let active = move || app.read(|e| e.active());
    view! {
        <div class="content attr">
            {move || match active() {
                None => view! { <div class="empty">"No object selected." <br /> "Select an object to see its settings."</div> }.into_any(),
                Some(id) => {
                    let head = app.read(|e| e.scene.get(id).map(|o| (o.name.clone(), kind_label(&o.kind), crate::icons::for_kind(&o.kind))));
                    let Some((name, kind, ic)) = head else { return ().into_any() };
                    let tab = app.attr_tab;
                    view! {
                        <h3>{icon_s(ic)}{name}<small>{kind}</small></h3>
                        <div class="seg">
                            <span class:on=move || tab.get() == AttrTab::Basic on:click=move |_| tab.set(AttrTab::Basic)>"Basic"</span>
                            <span class:on=move || tab.get() == AttrTab::Coord on:click=move |_| tab.set(AttrTab::Coord)>"Coord."</span>
                            <span class:on=move || tab.get() == AttrTab::Object on:click=move |_| tab.set(AttrTab::Object)>"Object"</span>
                        </div>
                        {move || match tab.get() {
                            AttrTab::Basic => view! { <Basic id=id /> }.into_any(),
                            AttrTab::Coord => view! { <Coord id=id /> }.into_any(),
                            AttrTab::Object => view! { <ObjectFields id=id /> }.into_any(),
                        }}
                    }
                    .into_any()
                }
            }}
        </div>
    }
}
