//! Configure View (Shift+V): the active panel's own settings — projection,
//! shading, render options, guides and its background image.

use crate::fields::{Choice, NumField, Toggle};
use crate::host;
use crate::state::{App, app};
use forma3d_core::camera::ViewKind;
use forma3d_core::editor::{Editor, PanelOptions};
use forma3d_core::renderer::{DisplayMode, ViewFx};
use forma3d_core::views::{Background, BackgroundFit};
use leptos::prelude::*;

/// Changes the active panel (a view change: no undo step) and refreshes.
fn change(app: App, f: impl FnOnce(&mut PanelOptions)) {
    app.view(|ed| f(ed.active_panel_mut()));
    app.rev.update(|r| *r += 1);
}

fn change_bg(app: App, f: impl FnOnce(&mut Background)) {
    change(app, |p| {
        if let Some(b) = p.background.as_mut() {
            f(b);
            *b = b.sanitized();
        }
    });
}

/// Picks a picture for the active panel's background.
pub fn load_background(app: App) {
    leptos::task::spawn_local(async move {
        match host::pick_bytes("image/png,image/jpeg,.png,.jpg,.jpeg").await {
            Ok(Some((name, bytes))) => match Background::from_file(&name, &bytes) {
                Ok(mut bg) => {
                    let (slot, ortho) = app.peek(|e| (e.active_view, e.cameras[e.active_view.index()].kind.is_ortho()));
                    // Reference images in parallel views usually sit in the scene.
                    if ortho {
                        bg.fit = BackgroundFit::Scene;
                    }
                    let label = app.peek(|e| e.cameras[slot.index()].kind.label());
                    app.view(|ed| ed.panels[slot.index()].background = Some(bg));
                    app.rev.update(|r| *r += 1);
                    app.status(format!("{name} is the {label} view's background"));
                }
                Err(e) => app.error(format!("Could not use {name}: {e}")),
            },
            Ok(None) => {}
            Err(e) => app.error(format!("Could not read the picture: {e}")),
        }
    });
}

fn row(label: &'static str, body: impl IntoView) -> impl IntoView {
    view! {
        <div class="lbl">
            <span>{label}</span>
            {body}
        </div>
    }
}

fn toggle_row(app: App, label: &'static str, get: fn(&PanelOptions) -> bool, set: fn(&mut PanelOptions, bool)) -> impl IntoView {
    row(
        label,
        view! { <Toggle on=Signal::derive(move || app.read(|e| get(e.active_panel()))) on_change=Callback::new(move |v: bool| change(app, |p| set(p, v)))/> },
    )
}

fn title(ed: &Editor) -> String {
    let slot = ed.active_view;
    let kind = ed.cameras[slot.index()].kind;
    if kind == slot {
        format!("Configure View: {}", kind.label())
    } else {
        format!("Configure View: {} (the {} panel)", kind.label(), slot.label())
    }
}

#[component]
pub fn ConfigureView() -> impl IntoView {
    let app = app();
    let close = move || app.view_settings.set(false);
    let projections: Vec<(String, String)> = ViewKind::PROJECTIONS.iter().map(|k| (k.id().to_owned(), k.label().to_owned())).collect();
    let shadings: Vec<(String, String)> = DisplayMode::ALL.iter().map(|m| (m.id().to_owned(), m.label().to_owned())).collect();
    let fits: Vec<(String, String)> = BackgroundFit::ALL.iter().map(|f| (f.label().to_owned(), f.label().to_owned())).collect();

    let general = move || {
        let projections = projections.clone();
        let shadings = shadings.clone();
        view! {
            <h3>"View"</h3>
            {row("Projection", view! {
                <Choice
                    options=projections
                    value=Signal::derive(move || app.read(|e| e.cameras[e.active_view.index()].kind.id().to_owned()))
                    on_change=Callback::new(move |id: String| {
                        if let Some(k) = ViewKind::PROJECTIONS.into_iter().find(|k| k.id() == id) {
                            app.view(|ed| ed.set_projection(k));
                            app.rev.update(|r| *r += 1);
                        }
                    })
                />
            })}
            {row("Shading", view! {
                <Choice
                    options=shadings
                    value=Signal::derive(move || app.read(|e| e.active_panel().display.id().to_owned()))
                    on_change=Callback::new(move |id: String| {
                        if let Some(m) = DisplayMode::ALL.into_iter().find(|m| m.id() == id) {
                            change(app, |p| p.display = m);
                        }
                    })
                />
            })}
            {toggle_row(app, "Grid", |p| p.grid, |p, v| p.grid = v)}
            {toggle_row(app, "Axis Compass", |p| p.compass, |p, v| p.compass = v)}
            {toggle_row(app, "Safe Frames", |p| p.safe_frames, |p, v| p.safe_frames = v)}
        }
    };

    let options = move || {
        ViewFx::ALL
            .into_iter()
            .filter(|f| *f != ViewFx::Tessellation)
            .map(|fx| {
                row(
                    fx.label(),
                    view! {
                        <Toggle
                            on=Signal::derive(move || app.read(|e| e.active_panel().fx.get(fx)))
                            on_change=Callback::new(move |_| change(app, |p| p.fx.toggle(fx)))
                        />
                    },
                )
            })
            .collect_view()
    };

    let background = move || {
        let fits = fits.clone();
        let bg = app.read(|e| e.active_panel().background.clone());
        match bg {
            None => view! {
                <div class="lbl">
                    <span>"Picture"</span>
                    <button class="btn" on:click=move |_| load_background(app)>"Load..."</button>
                </div>
                <p class="hint">"A PNG or JPEG behind this view: a blueprint to model from, or a photo to match."</p>
            }
            .into_any(),
            Some(b) => {
                let scene = b.fit == BackgroundFit::Scene;
                let name = b.name.clone();
                let tip = b.name.clone();
                view! {
                    <div class="lbl">
                        <span>"Picture"</span>
                        <span class="bg-name" title=tip>{name}</span>
                        <button class="btn" on:click=move |_| load_background(app)>"Replace..."</button>
                        <button class="btn" on:click=move |_| change(app, |p| p.background = None)>"Remove"</button>
                    </div>
                    {row("Show", view! {
                        <Toggle on=Signal::derive(move || app.read(|e| e.active_panel().background.as_ref().is_some_and(|b| b.show))) on_change=Callback::new(move |v: bool| change_bg(app, |b| b.show = v))/>
                    })}
                    {row("Opacity", view! {
                        <NumField
                            value=Signal::derive(move || app.read(|e| e.active_panel().background.as_ref().map_or(0.0, |b| b.opacity * 100.0)))
                            unit="%"
                            decimals=0
                            step=1.0
                            on_change=Callback::new(move |v: f32| change_bg(app, |b| b.opacity = v / 100.0))
                        />
                    })}
                    {row("Placement", view! {
                        <Choice
                            options=fits
                            value=Signal::derive(move || app.read(|e| e.active_panel().background.as_ref().map(|b| b.fit.label().to_owned()).unwrap_or_default()))
                            on_change=Callback::new(move |l: String| {
                                if let Some(f) = BackgroundFit::ALL.into_iter().find(|f| f.label() == l) {
                                    change_bg(app, |b| b.fit = f);
                                }
                            })
                        />
                    })}
                    {scene.then(|| view! {
                        {row("Width", view! {
                            <NumField
                                value=Signal::derive(move || app.read(|e| e.active_panel().background.as_ref().map_or(0.0, |b| b.size)))
                                unit="cm"
                                decimals=1
                                step=1.0
                                on_change=Callback::new(move |v: f32| change_bg(app, |b| b.size = v))
                            />
                        })}
                        {row("Offset X", view! {
                            <NumField
                                value=Signal::derive(move || app.read(|e| e.active_panel().background.as_ref().map_or(0.0, |b| b.offset[0])))
                                unit="cm"
                                decimals=1
                                step=1.0
                                on_change=Callback::new(move |v: f32| change_bg(app, |b| b.offset[0] = v))
                            />
                        })}
                        {row("Offset Y", view! {
                            <NumField
                                value=Signal::derive(move || app.read(|e| e.active_panel().background.as_ref().map_or(0.0, |b| b.offset[1])))
                                unit="cm"
                                decimals=1
                                step=1.0
                                on_change=Callback::new(move |v: f32| change_bg(app, |b| b.offset[1] = v))
                            />
                        })}
                        <p class="hint">"In Scene: the picture is fixed in the world and moves and zooms with this parallel view. Perspective views fit it to the view."</p>
                    })}
                }
                .into_any()
            }
        }
    };

    let copy_to_all = move |_| {
        app.view(|ed| {
            let src = ed.active_panel().clone();
            for p in &mut ed.panels {
                let bg = p.background.take();
                *p = PanelOptions { background: bg, ..src.clone() };
            }
        });
        app.rev.update(|r| *r += 1);
        app.status("Every view now has these settings (backgrounds stay per view)");
    };

    view! {
        <Show when=move || app.view_settings.get()>
            <div class="modal-bg" on:mousedown=move |_| close()>
                <div class="dialog prefs rsettings viewcfg" on:mousedown=|ev| ev.stop_propagation()>
                    <h2>{move || app.read(title)}</h2>
                    <div class="rs-body">
                        {general.clone()}
                        <h3>"Options"</h3>
                        {options}
                        <p class="hint">"Tessellation applies to every view. These settings are saved with the scene."</p>
                        <h3>"Background"</h3>
                        {background.clone()}
                    </div>
                    <div class="btns">
                        <button class="btn" on:click=copy_to_all>"Use in All Views"</button>
                        <button class="btn primary" on:click=move |_| close()>"Close"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
