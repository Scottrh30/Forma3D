//! The icon bar at the top of the viewport: a row of command buttons you arrange
//! yourself.
//!
//! - Drag an icon along the bar to move it; a blue line shows where it lands.
//! - Drag an icon off the bar and let go to remove it.
//! - The "+" at the end opens the Icon Library: click a command to add it at
//!   the end, or drag it onto the bar.
//! - Right-click for Remove, Add Separator, Icon Library and Reset.
//! - Window > Viewport Icon Bar shows or hides it.
//!
//! The layout is saved in the settings as `icon_bar.json`.

use crate::chrome::DropButton;
use crate::commands::{self, Cmd};
use crate::icons::{icon, icon_s};
use crate::state::{App, app};
use forma3d_core::generator::GeneratorKind;
use forma3d_core::pen::PenKind;
use forma3d_core::primitive::PrimitiveKind;
use forma3d_core::renderer::{DisplayMode, ViewFx};
use leptos::prelude::*;

pub const FILE: &str = "icon_bar.json";
const SHAPES_ID: &str = "group.shapes";
const VERSION: u64 = 1;

/// One place on the bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Command(Cmd),
    Separator,
    /// A dropdown of the parametric shapes.
    Shapes,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IconBar {
    pub entries: Vec<Entry>,
    pub shown: bool,
}

impl Default for IconBar {
    fn default() -> Self {
        IconBar { entries: default_entries(), shown: true }
    }
}

/// The starting set: the views, framing, display modes, grid and solo,
/// view effects, then shapes, pens and rendering.
pub fn default_entries() -> Vec<Entry> {
    use Entry::{Command as C, Separator as S};
    let mut v: Vec<Entry> = forma3d_core::camera::ViewKind::ALL.iter().map(|&k| C(Cmd::Camera(k))).collect();
    v.extend([C(Cmd::FourViews), S, C(Cmd::FrameAll), C(Cmd::FrameSelected), S]);
    v.extend(DisplayMode::ALL.iter().map(|&m| C(Cmd::Display(m))));
    v.extend([S, C(Cmd::ToggleGrid), C(Cmd::ToggleSolo), S]);
    v.extend(ViewFx::ALL.iter().map(|&f| C(Cmd::ViewFx(f))));
    v.extend([
        S,
        Entry::Shapes,
        C(Cmd::Pen(PenKind::Spline)),
        C(Cmd::Pen(PenKind::Polygon)),
        C(Cmd::Generator(GeneratorKind::Subdivision)),
        S,
        C(Cmd::RenderView),
        C(Cmd::RenderToViewer),
    ]);
    v
}

impl IconBar {
    /// Command ids, "|" for separators.
    pub fn to_json(&self) -> String {
        let ids: Vec<String> = self
            .entries
            .iter()
            .filter_map(|e| match e {
                Entry::Separator => Some("|".to_owned()),
                Entry::Shapes => Some(SHAPES_ID.to_owned()),
                Entry::Command(c) => commands::infos().iter().find(|i| i.cmd == *c).map(|i| i.id.clone()),
            })
            .collect();
        serde_json::json!({ "version": VERSION, "shown": self.shown, "items": ids }).to_string()
    }

    /// Reads a saved bar; unknown ids are skipped, and anything unreadable
    /// gives the default bar.
    pub fn from_json(text: &str) -> IconBar {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return IconBar::default() };
        let Some(items) = v.get("items").and_then(|i| i.as_array()) else { return IconBar::default() };
        let entries = items
            .iter()
            .filter_map(|i| i.as_str())
            .filter_map(|id| match id {
                "|" => Some(Entry::Separator),
                SHAPES_ID => Some(Entry::Shapes),
                id => commands::infos().iter().find(|i| i.id == id).map(|i| Entry::Command(i.cmd)),
            })
            .collect();
        IconBar { entries, shown: v.get("shown").and_then(|s| s.as_bool()).unwrap_or(true) }
    }

    /// Moves the entry at `from` to before position `to` (positions as they
    /// were before the move).
    pub fn move_entry(&mut self, from: usize, to: usize) {
        if from >= self.entries.len() {
            return;
        }
        let e = self.entries.remove(from);
        let to = if to > from { to - 1 } else { to };
        self.entries.insert(to.min(self.entries.len()), e);
    }

    pub fn insert(&mut self, at: usize, e: Entry) {
        let at = at.min(self.entries.len());
        self.entries.insert(at, e);
    }

    pub fn remove(&mut self, at: usize) {
        if at < self.entries.len() {
            self.entries.remove(at);
        }
    }
}

fn save(app: App) {
    let text = app.icon_bar.with_untracked(|b| b.to_json());
    crate::settings::write_named(app, FILE, text);
}

pub fn edit(app: App, f: impl FnOnce(&mut IconBar)) {
    app.icon_bar.update(f);
    save(app);
}

/// Something being dragged onto or along the bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarDrag {
    pub entry: Entry,
    /// Where it sits on the bar, when it came from the bar.
    pub from: Option<usize>,
    pub start: (i32, i32),
    pub at: (i32, i32),
    pub moving: bool,
    /// Insert position under the pointer, or None when off the bar.
    pub target: Option<usize>,
}

/// The insert position under a point: before the icon under it (after it
/// past its middle), or the end of the bar; None when off the bar.
fn bar_target(x: i32, y: i32) -> Option<usize> {
    let doc = web_sys::window()?.document()?;
    let bar = doc.query_selector(".iconbar").ok()??;
    let r = bar.get_bounding_client_rect();
    let (x, y) = (x as f64, y as f64);
    if x < r.left() || x > r.right() || y < r.top() - 12.0 || y > r.bottom() + 12.0 {
        return None;
    }
    let items = bar.query_selector_all("[data-ib]").ok()?;
    let mut n = 0;
    for k in 0..items.length() {
        let Some(el) = items.item(k).and_then(|n| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(n).ok()) else { continue };
        let i: usize = el.get_attribute("data-ib")?.parse().ok()?;
        n = n.max(i + 1);
        let er = el.get_bounding_client_rect();
        if x < er.left() + er.width() / 2.0 {
            return Some(i);
        }
    }
    Some(n)
}

pub fn install(app: App) {
    let _ = window_event_listener(leptos::ev::pointermove, move |ev| {
        let Some(mut d) = app.bar_drag.get_untracked() else { return };
        let (x, y) = (ev.client_x(), ev.client_y());
        d.at = (x, y);
        if !d.moving && ((x - d.start.0).pow(2) + (y - d.start.1).pow(2)) > 25 {
            d.moving = true;
        }
        if d.moving {
            d.target = bar_target(x, y);
        }
        app.bar_drag.set(Some(d));
    });
    let _ = window_event_listener(leptos::ev::pointerup, move |ev| {
        let Some(d) = app.bar_drag.get_untracked() else { return };
        app.bar_drag.set(None);
        if !d.moving {
            return;
        }
        // The click that follows a drag must not run the command.
        SUPPRESS_CLICK.with(|s| s.set(true));
        match (bar_target(ev.client_x(), ev.client_y()), d.from) {
            (Some(to), Some(from)) => edit(app, |b| b.move_entry(from, to)),
            (Some(to), None) => edit(app, |b| b.insert(to, d.entry)),
            (None, Some(from)) => {
                edit(app, |b| b.remove(from));
                app.status("Removed from the icon bar");
            }
            (None, None) => {}
        }
    });
    let _ = window_event_listener(leptos::ev::keydown, move |ev| {
        if ev.key() == "Escape" {
            if app.bar_drag.get_untracked().is_some() {
                app.bar_drag.set(None);
            }
            if app.icon_library.get_untracked() {
                app.icon_library.set(false);
            }
        }
    });
}

thread_local! {
    static SUPPRESS_CLICK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn take_suppressed() -> bool {
    SUPPRESS_CLICK.with(|s| s.replace(false))
}

fn start_drag(app: App, entry: Entry, from: Option<usize>, ev: &web_sys::PointerEvent) {
    if ev.button() != 0 {
        return;
    }
    let at = (ev.client_x(), ev.client_y());
    SUPPRESS_CLICK.with(|s| s.set(false));
    app.bar_drag.set(Some(BarDrag { entry, from, start: at, at, moving: false, target: None }));
}

fn tooltip(app: App, cmd: Cmd) -> String {
    let name = commands::info(cmd).name;
    let k = app.keymap.with_untracked(|k| k.label(cmd));
    if k.is_empty() { name } else { format!("{name}  ({k})") }
}

/// One icon on the bar.
#[component]
fn BarIcon(index: usize, entry: Entry) -> impl IntoView {
    let app = app();
    let dragging = move || app.bar_drag.with(|d| d.as_ref().is_some_and(|d| d.moving && d.from == Some(index)));
    let mark = move || app.bar_drag.with(|d| d.as_ref().filter(|d| d.moving).and_then(|d| d.target) == Some(index));
    let menu = move |ev: web_sys::MouseEvent| {
        ev.prevent_default();
        ev.stop_propagation();
        app.bar_menu.set(Some((Some(index), ev.client_x(), ev.client_y())));
    };
    let inner = match entry {
        Entry::Separator => view! { <div class="ib-sep"></div> }.into_any(),
        Entry::Shapes => view! {
            <DropButton id="ib-shapes" icon_name="sphere" title="Shapes" items=PrimitiveKind::ALL.map(Cmd::Primitive).to_vec() />
        }
        .into_any(),
        Entry::Command(cmd) => {
            let st = move || app.read(|ed| commands::state(cmd, ed, &app));
            view! {
                <button
                    class="tb"
                    class:on=move || st().1 == Some(true)
                    class:off=move || !st().0
                    title=move || tooltip(app, cmd)
                    on:click=move |_| {
                        if !take_suppressed() {
                            commands::run(app, cmd);
                        }
                    }
                >
                    {icon(commands::info(cmd).icon)}
                </button>
            }
            .into_any()
        }
    };
    view! {
        <div
            class="ib-item"
            class:dragging=dragging
            class:mark=mark
            data-ib=index.to_string()
            on:pointerdown=move |ev: web_sys::PointerEvent| start_drag(app, entry, Some(index), &ev)
            on:contextmenu=menu
        >
            {inner}
        </div>
    }
}

/// The bar itself (shown above the viewport).
#[component]
pub fn IconBarView() -> impl IntoView {
    let app = app();
    let end_mark = move || {
        let n = app.icon_bar.with(|b| b.entries.len());
        app.bar_drag.with(|d| d.as_ref().filter(|d| d.moving).and_then(|d| d.target) == Some(n))
    };
    view! {
        <Show when=move || app.icon_bar.with(|b| b.shown)>
            <div
                class="iconbar"
                class:target=move || app.bar_drag.with(|d| d.as_ref().is_some_and(|d| d.moving && d.target.is_some()))
                on:contextmenu=move |ev: web_sys::MouseEvent| {
                    ev.prevent_default();
                    app.bar_menu.set(Some((None, ev.client_x(), ev.client_y())));
                }
            >
                {move || {
                    app.icon_bar
                        .with(|b| b.entries.clone())
                        .into_iter()
                        .enumerate()
                        .map(|(i, e)| view! { <BarIcon index=i entry=e /> })
                        .collect_view()
                }}
                <div class="ib-end" class:mark=end_mark></div>
                <button class="tb ib-add" title="Icon Library: add icons to the bar" on:click=move |_| app.icon_library.update(|v| *v = !*v)>
                    {icon_s("plus")}
                </button>
            </div>
        </Show>
        <BarMenu />
        <IconLibrary />
        {move || app.bar_drag.get().filter(|d| d.moving && d.from.is_none()).map(|d| {
            let ic = match d.entry {
                Entry::Command(c) => commands::info(c).icon,
                Entry::Shapes => "sphere",
                Entry::Separator => "minus",
            };
            view! { <div class="ib-ghost" style=format!("left:{}px;top:{}px", d.at.0 - 14, d.at.1 - 14)>{icon(ic)}</div> }
        })}
    }
}

/// Right-click menu of the bar.
#[component]
fn BarMenu() -> impl IntoView {
    let app = app();
    let close = move || app.bar_menu.set(None);
    move || {
        let (index, x, y) = app.bar_menu.get()?;
        let item = |label: &'static str, f: Box<dyn Fn()>| {
            view! {
                <div class="mi" on:click=move |_| { close(); f(); }>
                    <span class="chk"></span>
                    <span class="ico"></span>
                    {label}
                </div>
            }
        };
        let remove = index.map(|i| item("Remove", Box::new(move || edit(app, |b| b.remove(i)))));
        let sep_at = index.map_or_else(|| app.icon_bar.with_untracked(|b| b.entries.len()), |i| i + 1);
        Some(view! {
            <div class="scrim" on:mousedown=move |_| close()></div>
            <div class="dropdown fixed" style=format!("left:{x}px;top:{y}px") on:mousedown=|ev| ev.stop_propagation()>
                {remove}
                {item("Add Separator", Box::new(move || edit(app, |b| b.insert(sep_at, Entry::Separator))))}
                {item("Icon Library...", Box::new(move || app.icon_library.set(true)))}
                <div class="msep"></div>
                {item("Reset Icon Bar", Box::new(move || edit(app, |b| b.entries = default_entries())))}
                {item("Hide Icon Bar", Box::new(move || edit(app, |b| b.shown = false)))}
            </div>
        })
    }
}

/// Every command with an icon: click to add at the end, or drag onto the bar.
#[component]
fn IconLibrary() -> impl IntoView {
    let app = app();
    let search = RwSignal::new(String::new());
    let list = move || {
        let q = search.get().to_lowercase();
        let mut groups: Vec<(&'static str, Vec<Cmd>)> = Vec::new();
        for i in commands::infos() {
            if !q.is_empty() && !format!("{} {} {}", i.name, i.category, i.keywords).to_lowercase().contains(&q) {
                continue;
            }
            match groups.iter_mut().find(|g| g.0 == i.category) {
                Some(g) => g.1.push(i.cmd),
                None => groups.push((i.category, vec![i.cmd])),
            }
        }
        groups
            .into_iter()
            .map(|(cat, cmds)| {
                view! {
                    <div class="il-cat">{cat}</div>
                    <div class="il-grid">
                        {cmds
                            .into_iter()
                            .map(|c| {
                                let info = commands::info(c);
                                view! {
                                    <button
                                        class="tb"
                                        title=info.name.clone()
                                        on:pointerdown=move |ev: web_sys::PointerEvent| start_drag(app, Entry::Command(c), None, &ev)
                                        on:click=move |_| {
                                            if !take_suppressed() {
                                                edit(app, |b| b.entries.push(Entry::Command(c)));
                                                app.status(format!("Added {} to the icon bar", commands::info(c).name));
                                            }
                                        }
                                    >
                                        {icon(info.icon)}
                                    </button>
                                }
                            })
                            .collect_view()}
                    </div>
                }
            })
            .collect_view()
    };
    view! {
        <Show when=move || app.icon_library.get()>
            <div class="iconlib" on:mousedown=|ev| ev.stop_propagation()>
                <div class="il-head">
                    <b>"Icon Library"</b>
                    <span class="hint">"Click to add · drag onto the bar"</span>
                    <button class="tb" title="Close" on:click=move |_| app.icon_library.set(false)>{icon_s("close")}</button>
                </div>
                <input
                    class="searchin"
                    placeholder="Search commands"
                    on:keydown=|ev: web_sys::KeyboardEvent| ev.stop_propagation()
                    on:input=move |ev| search.set(event_target_value(&ev))
                />
                <div class="il-body">
                    <div class="il-cat">"Bar items"</div>
                    <div class="il-grid">
                        <button class="tb" title="Shapes (a dropdown)" on:pointerdown=move |ev: web_sys::PointerEvent| start_drag(app, Entry::Shapes, None, &ev) on:click=move |_| if !take_suppressed() { edit(app, |b| b.entries.push(Entry::Shapes)) }>
                            {icon("sphere")}
                        </button>
                        <button class="tb" title="Separator" on:pointerdown=move |ev: web_sys::PointerEvent| start_drag(app, Entry::Separator, None, &ev) on:click=move |_| if !take_suppressed() { edit(app, |b| b.entries.push(Entry::Separator)) }>
                            {icon("minus")}
                        </button>
                    </div>
                    {list}
                </div>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_moves_and_skips_unknown_ids() {
        let mut b = IconBar::default();
        let text = b.to_json();
        assert_eq!(IconBar::from_json(&text), b);
        let n = b.entries.len();
        let first = b.entries[0];
        b.move_entry(0, n);
        assert_eq!(b.entries[n - 1], first);
        b.move_entry(n - 1, 0);
        assert_eq!(b.entries[0], first);
        b.remove(0);
        assert_eq!(b.entries.len(), n - 1);
        b.shown = false;
        let back = IconBar::from_json(&b.to_json());
        assert!(!back.shown);
        let odd = IconBar::from_json(r#"{"version":1,"items":["|","no.such.command","group.shapes"]}"#);
        assert_eq!(odd.entries, vec![Entry::Separator, Entry::Shapes]);
        assert_eq!(IconBar::from_json("nonsense"), IconBar::default());
    }
}
