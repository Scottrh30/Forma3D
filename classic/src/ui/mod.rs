//! The application shell: menus, toolbars, dockable panels, shortcuts and dialogs.

mod assets;
mod attributes;
mod commander;
pub(crate) mod commands;
mod components;
mod console;
mod coords;
mod icon_bar;
mod layers;
mod materials;
mod objects;
mod preferences;
mod pen;
mod structure;
mod takes;
mod timeline;
mod viewport;
mod widgets;
mod workspace_panels;

use crate::component::EditMode;
use crate::deformer::DeformerKind;
use crate::editor::{Dialog, Editor, MeshTool, Pending};
use crate::generator::GeneratorKind;
use crate::gizmo::{CoordSystem, Tool};
use crate::icons::{self, Icon};
use crate::primitive::PrimitiveKind;
use crate::renderer::GpuResources;
use crate::scene::Scene;
use crate::theme;
use commander::{Commander, Customize};
use commands::{AppAction, Cmd, Info, Keymap, Panel as Tab, Shared, Workspace};
use eframe::egui::{self, Event, Key, Ui};
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use widgets::{axis_toggle, flyout, palette_separator};

const MESH_TOOLS: [MeshTool; 5] = [MeshTool::Extrude, MeshTool::Inset, MeshTool::LoopCut, MeshTool::Bevel, MeshTool::Knife];

/// Viewport in the middle, Objects over Attributes on the right, and (except
/// in the Model layout) Materials and Coordinates under the viewport.
/// The default panel arrangement for a workspace.
fn build_dock(workspace: Workspace) -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Viewport]);
    let tree = dock.main_surface_mut();
    match workspace {
        Workspace::Model | Workspace::Custom => {
            // As in Cinema 4D: Objects / Takes over Attributes / Layers.
            let [left, right] = tree.split_right(NodeIndex::root(), 0.76, vec![Tab::Objects, Tab::Takes]);
            tree.split_below(right, 0.48, vec![Tab::Attributes, Tab::Layers]);
            let [_view, bottom] = tree.split_below(left, 0.75, vec![Tab::Materials, Tab::Assets, Tab::Structure, Tab::Console]);
            tree.split_right(bottom, 0.45, vec![Tab::Coordinates]);
        }
        Workspace::Track => {
            let [left, right] = tree.split_right(NodeIndex::root(), 0.76, vec![Tab::Objects]);
            tree.split_below(right, 0.48, vec![Tab::Attributes]);
            tree.split_below(left, 0.58, vec![Tab::Footage]);
        }
        Workspace::UvEdit => {
            let [left, right] = tree.split_right(NodeIndex::root(), 0.78, vec![Tab::Objects]);
            tree.split_below(right, 0.48, vec![Tab::Attributes]);
            tree.split_left(left, 0.46, vec![Tab::UvEditor]);
        }
        Workspace::Animate => {
            let [left, right] = tree.split_right(NodeIndex::root(), 0.76, vec![Tab::Objects]);
            tree.split_below(right, 0.48, vec![Tab::Attributes]);
            tree.split_below(left, 0.78, vec![Tab::Coordinates]);
        }
        Workspace::Sculpt => {
            let [left, _right] = tree.split_right(NodeIndex::root(), 0.80, vec![Tab::Attributes, Tab::Objects]);
            tree.split_left(left, 0.17, vec![Tab::Brushes]);
        }
    }
    hide_viewport_tab_bar(&mut dock);
    dock
}

/// The viewport has its own menu strip instead of a tab, unless other panels
/// were docked into the same place.
fn hide_viewport_tab_bar(dock: &mut DockState<Tab>) {
    if let Some(path) = dock.find_tab(&Tab::Viewport) {
        let alone = dock[path.node_path()].tabs().is_some_and(|t| t.len() == 1);
        dock[path.node_path()].set_tab_bar_hidden(alone);
    }
}

/// Saved workspaces: the active one and every arrangement.
#[derive(serde::Serialize, serde::Deserialize)]
struct SavedWorkspaces {
    active: Workspace,
    docks: HashMap<Workspace, DockState<Tab>>,
    /// Whether each workspace shows the timeline.
    #[serde(default)]
    timelines: HashMap<Workspace, bool>,
}

/// Encodes saved workspaces. Panels never drawn have NaN rectangles, which
/// JSON writes as null and cannot read back; they are laid out again on
/// screen anyway, so they are stored as zero.
fn workspaces_to_json(saved: &SavedWorkspaces) -> Option<String> {
    // Every position in a dock layout is an {x, y} pair.
    fn fix(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Object(map) => {
                for (k, child) in map.iter_mut() {
                    if child.is_null() && (k == "x" || k == "y") {
                        *child = serde_json::json!(0.0);
                    } else {
                        fix(child);
                    }
                }
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(fix),
            _ => {}
        }
    }
    let mut value = serde_json::to_value(saved).ok()?;
    fix(&mut value);
    serde_json::to_string(&value).ok()
}

fn workspaces_path() -> Option<PathBuf> {
    commands::keymap_path().map(|p| p.with_file_name("workspaces.json"))
}

fn load_workspaces(path: Option<&Path>) -> Option<SavedWorkspaces> {
    let text = std::fs::read_to_string(path?).ok()?;
    let mut saved: SavedWorkspaces = serde_json::from_str(&text).ok()?;
    // A layout that lost the viewport is no use; fall back to the default.
    saved.docks.retain(|_, d| d.find_tab(&Tab::Viewport).is_some());
    Some(saved)
}

struct Viewer<'a> {
    ed: &'a mut Editor,
    icon_bar: &'a mut icon_bar::IconBar,
    srgb: bool,
    assets: &'a mut crate::assets::Library,
    console: &'a mut console::Console,
    time: f64,
}

impl TabViewer for Viewer<'_> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        tab.title().into()
    }

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(("tab", tab.title()))
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Tab) {
        match tab {
            Tab::Viewport => {
                ui.spacing_mut().item_spacing.y = 0.0;
                if self.ed.show_icon_bar {
                    self.icon_bar.show(ui, self.ed);
                }
                viewport::show(ui, self.ed, self.srgb);
            }
            Tab::Objects => objects::show(ui, self.ed),
            Tab::Attributes => attributes::show(ui, self.ed),
            Tab::Materials => materials::show(ui, self.ed),
            Tab::Coordinates => coords::show(ui, self.ed),
            Tab::UvEditor => workspace_panels::uv_editor(ui, self.ed),
            Tab::Footage => workspace_panels::footage(ui, self.ed),
            Tab::Brushes => workspace_panels::brushes(ui, self.ed),
            Tab::Layers => layers::show(ui, self.ed),
            Tab::Takes => takes::show(ui, self.ed),
            Tab::Structure => structure::show(ui, self.ed),
            Tab::Assets => assets::show(ui, self.ed, self.assets),
            Tab::Console => {
                let name = self.ed.project_name();
                console::show(ui, self.console, self.time, &name);
            }
        }
    }

    /// Every panel but the viewport can be closed (Window > Panels reopens it).
    fn closeable(&mut self, tab: &mut Tab) -> bool {
        *tab != Tab::Viewport
    }

    fn allowed_in_windows(&self, tab: &mut Tab) -> bool {
        *tab != Tab::Viewport
    }

    fn scroll_bars(&self, _tab: &Tab) -> [bool; 2] {
        [false, false]
    }
}

pub struct App {
    ed: Editor,
    dock: DockState<Tab>,
    workspace: Workspace,
    /// Arrangements of the workspaces not on screen (each keeps its changes).
    docks: HashMap<Workspace, DockState<Tab>>,
    /// Timeline on/off for each workspace, as last left.
    timelines: HashMap<Workspace, bool>,
    workspaces_path: Option<PathBuf>,
    /// What was last written to `workspaces.json`.
    saved_workspaces: String,
    srgb: bool,
    allow_close: bool,
    last_title: String,
    /// The command registry and the shortcuts in force.
    infos: Arc<Vec<Info>>,
    keymap: Keymap,
    keymap_path: Option<PathBuf>,
    commander: Commander,
    customize: Customize,
    /// Most recently run commands, newest first (shown first in the Commander).
    recent: Vec<Cmd>,
    /// First key of a two-key sequence (M of M~S) and when it was pressed.
    prefix: Option<(commands::Shortcut, f64)>,
    /// Material to open in the Material Editor next frame.
    open_material: Option<u64>,
    /// The customizable icon bar above the viewport.
    icon_bar: icon_bar::IconBar,
    /// The other open projects (the current one is `ed`), in Window menu order;
    /// `ed` sits at position `slot` of that list.
    others: Vec<Editor>,
    slot: usize,
    /// Objects copied with Copy or Cut, shared by all projects.
    clipboard: Option<Scene>,
    /// Text to put on the system clipboard next frame (copied objects).
    clipboard_out: Option<String>,
    prefs: preferences::Prefs,
    prefs_path: Option<PathBuf>,
    prefs_open: bool,
    /// The interface scale last applied.
    applied_scale: f32,
    /// When autosave last ran (seconds of app time).
    last_autosave: f64,
    /// The Asset Browser's library.
    assets: crate::assets::Library,
    console: console::Console,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let rs = cc.wgpu_render_state.as_ref().expect("Forma3D needs the wgpu renderer");
        let gpu = GpuResources::new(&rs.device, rs.target_format);
        let srgb = gpu.target_is_srgb;
        rs.renderer.write().callback_resources.insert(gpu);
        let infos = Arc::new(commands::all());
        let keymap_path = commands::keymap_path();
        let keymap = keymap_path.as_deref().map(|p| commands::load_keymap(&infos, p)).unwrap_or_default();
        let icon_bar = icon_bar::IconBar::load(&infos);
        let workspaces_path = workspaces_path();
        let (workspace, mut docks, timelines) = match load_workspaces(workspaces_path.as_deref()) {
            Some(saved) => (saved.active, saved.docks, saved.timelines),
            None => (Workspace::Model, HashMap::new(), HashMap::new()),
        };
        let mut dock = docks.remove(&workspace).unwrap_or_else(|| build_dock(workspace));
        hide_viewport_tab_bar(&mut dock);
        let prefs_path = preferences::Prefs::path();
        let prefs = preferences::Prefs::load(prefs_path.as_deref());
        let mut ed = match prefs.startup {
            preferences::Startup::Demo => Editor::new(Scene::demo()),
            preferences::Startup::Empty => Editor::new(Scene::new()),
        };
        prefs.apply_to(&mut ed);
        ed.show_timeline = timelines.get(&workspace).copied().unwrap_or(workspace.shows_timeline());
        App {
            ed,
            dock,
            workspace,
            docks,
            timelines,
            workspaces_path,
            saved_workspaces: String::new(),
            srgb,
            allow_close: false,
            last_title: String::new(),
            infos,
            keymap,
            keymap_path,
            commander: Commander::default(),
            customize: Customize::default(),
            recent: Vec::new(),
            prefix: None,
            open_material: None,
            icon_bar,
            others: Vec::new(),
            slot: 0,
            clipboard: None,
            clipboard_out: None,
            prefs,
            prefs_path,
            prefs_open: false,
            applied_scale: 0.0,
            last_autosave: 0.0,
            assets: crate::assets::Library::load(commands::keymap_path().map(|p| p.with_file_name("assets"))),
            console: console::Console::default(),
        }
    }

    // ---- file actions ------------------------------------------------------

    /// Runs `pending` now, or asks first when it would throw away unsaved work.
    fn guard(&mut self, pending: Pending) {
        let at_risk = match pending {
            Pending::Quit => self.unsaved_projects() > 0,
            Pending::Close | Pending::Revert => self.ed.is_dirty(),
            Pending::New | Pending::Open => false,
        };
        let ask = at_risk && (self.prefs.confirm_discard || pending == Pending::Quit);
        if ask {
            self.ed.dialog = Some(Dialog::ConfirmDiscard(pending));
        } else {
            self.perform(pending);
        }
    }

    fn perform(&mut self, pending: Pending) {
        match pending {
            Pending::New => self.new_project(),
            Pending::Open => {
                self.ed.dialog = Some(Dialog::Open { path: self.ed.default_path(), error: None });
            }
            Pending::Quit => {
                self.allow_close = true;
                self.ed.quit_requested = true;
            }
            Pending::Close => self.close_project(),
            Pending::Revert => {
                if let Err(e) = self.ed.revert() {
                    self.ed.status = format!("Revert failed: {e}");
                }
            }
        }
    }

    // ---- projects ------------------------------------------------------------

    fn unsaved_projects(&self) -> usize {
        self.others.iter().chain(std::iter::once(&self.ed)).filter(|e| e.is_dirty()).count()
    }

    /// A fresh, empty project set up from Preferences.
    fn blank_project(&self) -> Editor {
        let mut ed = Editor::new(Scene::new());
        self.prefs.apply_to_new(&mut ed);
        ed.show_timeline = self.ed.show_timeline;
        ed
    }

    /// Makes `ed` the current project; the old one stays open behind it.
    fn add_project(&mut self, ed: Editor) {
        let old = std::mem::replace(&mut self.ed, ed);
        self.others.insert(self.slot, old);
        self.slot += 1;
    }

    fn new_project(&mut self) {
        let ed = self.blank_project();
        self.add_project(ed);
        self.ed.status = format!("New project ({} open)", self.others.len() + 1);
    }

    /// Opens a file as a new project, or in place of an untouched Untitled one.
    fn open_project(&mut self, path: &Path) -> Result<(), String> {
        let mut ed = self.blank_project();
        ed.open_from(path)?;
        if self.ed.is_pristine() {
            self.ed = ed;
        } else {
            self.add_project(ed);
        }
        Ok(())
    }

    /// Closes the current project; closing the last one leaves an empty Untitled.
    fn close_project(&mut self) {
        let name = self.ed.project_name();
        if self.others.is_empty() {
            self.ed = self.blank_project();
        } else {
            let next = self.slot.min(self.others.len() - 1);
            self.ed = self.others.remove(next);
            self.slot = next;
        }
        self.ed.status = format!("Closed {name}");
    }

    /// Brings project `index` (Window menu order) to the front.
    fn switch_project(&mut self, index: usize) {
        if index == self.slot || index > self.others.len() {
            return;
        }
        let target = if index < self.slot { index } else { index - 1 };
        let ed = self.others.remove(target);
        let old = std::mem::replace(&mut self.ed, ed);
        self.others.insert(if index < self.slot { self.slot - 1 } else { self.slot }, old);
        self.slot = index;
    }

    /// Names of all open projects in Window menu order, marking unsaved ones.
    fn project_names(&self) -> Vec<String> {
        let mut all: Vec<&Editor> = self.others.iter().collect();
        all.insert(self.slot, &self.ed);
        all.iter().map(|e| format!("{}{}", e.project_name(), if e.is_dirty() { " *" } else { "" })).collect()
    }

    fn copy(&mut self, cut: bool) {
        let clip = if cut { self.ed.cut_selection() } else { self.ed.copy_selection() };
        if let Some(clip) = clip {
            // Also as text, so Ctrl+V works and other Forma3D windows can paste it.
            self.clipboard_out = clip.to_json().ok();
            self.clipboard = Some(clip);
        }
    }

    fn paste(&mut self) {
        match self.clipboard.clone() {
            Some(clip) => {
                self.ed.paste(&clip);
            }
            None => self.ed.status = "Nothing to paste: copy some objects first".to_owned(),
        }
    }

    /// Autosaves changed projects into the autosave folder (never over the
    /// project's own file).
    fn autosave(&mut self, now: f64) {
        let minutes = self.prefs.autosave_minutes;
        if minutes == 0 {
            self.last_autosave = now;
            return;
        }
        if now - self.last_autosave < minutes as f64 * 60.0 {
            return;
        }
        self.last_autosave = now;
        let Some(dir) = preferences::Prefs::autosave_dir() else { return };
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let mut saved = 0;
        let mut all: Vec<&Editor> = self.others.iter().collect();
        all.insert(self.slot, &self.ed);
        for (i, ed) in all.iter().enumerate() {
            if !ed.is_dirty() {
                continue;
            }
            let stem = ed.project_name().replace(['/', '\\', ':'], "_");
            let path = dir.join(format!("{}-{}.forma3d.json", stem.trim_end_matches(".json"), i + 1));
            if let Ok(text) = ed.scene.to_json()
                && std::fs::write(&path, text).is_ok()
            {
                saved += 1;
            }
        }
        if saved > 0 {
            self.ed.status = format!("Autosaved {saved} project(s) to {}", dir.display());
        }
    }

    fn save(&mut self) {
        match self.ed.file_path.clone() {
            Some(path) => {
                if let Err(e) = self.ed.save_to(&path) {
                    self.ed.status = format!("Save failed: {e}");
                }
            }
            None => self.save_as(),
        }
    }

    fn save_as(&mut self) {
        self.ed.dialog = Some(Dialog::SaveAs { path: self.ed.default_path(), error: None });
    }

    // ---- workspaces ---------------------------------------------------------

    /// Switches workspace, keeping the current one's arrangement for later.
    fn switch_workspace(&mut self, to: Workspace) {
        if to == self.workspace {
            return;
        }
        let next = self.docks.remove(&to).unwrap_or_else(|| build_dock(to));
        let previous = std::mem::replace(&mut self.dock, next);
        self.docks.insert(self.workspace, previous);
        self.timelines.insert(self.workspace, self.ed.show_timeline);
        self.workspace = to;
        hide_viewport_tab_bar(&mut self.dock);
        self.ed.show_timeline = self.timelines.get(&to).copied().unwrap_or(to.shows_timeline());
        self.ed.status = format!("Workspace: {}", to.label());
        self.save_workspaces();
    }

    fn cycle_workspace(&mut self, step: i32) {
        let all = Workspace::ALL;
        let i = all.iter().position(|w| *w == self.workspace).unwrap_or(0) as i32;
        let n = all.len() as i32;
        self.switch_workspace(all[(i + step).rem_euclid(n) as usize]);
    }

    /// Puts the current workspace back to its default arrangement.
    fn reset_workspace(&mut self) {
        self.dock = build_dock(self.workspace);
        self.ed.show_timeline = self.workspace.shows_timeline();
        self.ed.status = format!("Reset the {} workspace", self.workspace.label());
    }

    /// Brings a panel to the front, opening it if it was closed.
    fn show_panel(&mut self, panel: Tab) {
        if let Some(path) = self.dock.find_tab(&panel) {
            let _ = self.dock.set_active_tab(path);
            return;
        }
        // Dock it with the Attribute or Object Manager if either is open,
        // otherwise in a new column on the right.
        let host = [Tab::Attributes, Tab::Objects, Tab::Coordinates, Tab::Materials]
            .iter()
            .find_map(|t| self.dock.find_tab(t));
        match host {
            Some(path) => {
                self.dock.set_focused_node_and_surface(path.node_path());
                self.dock.push_to_focused_leaf(panel);
            }
            None => {
                self.dock.main_surface_mut().split_right(NodeIndex::root(), 0.75, vec![panel]);
            }
        }
        hide_viewport_tab_bar(&mut self.dock);
        self.save_workspaces();
    }

    /// Writes the workspaces file when an arrangement has changed.
    fn save_workspaces(&mut self) {
        let Some(path) = self.workspaces_path.clone() else { return };
        let mut docks = self.docks.clone();
        docks.insert(self.workspace, self.dock.clone());
        let mut timelines = self.timelines.clone();
        timelines.insert(self.workspace, self.ed.show_timeline);
        let saved = SavedWorkspaces { active: self.workspace, docks, timelines };
        let Some(text) = workspaces_to_json(&saved) else { return };
        if text == self.saved_workspaces {
            return;
        }
        let write = || -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = path.with_extension("json.tmp");
            std::fs::write(&tmp, &text)?;
            std::fs::rename(&tmp, &path)
        };
        match write() {
            Ok(()) => self.saved_workspaces = text,
            Err(e) => self.ed.status = format!("Could not save workspaces: {e}"),
        }
    }

    /// The Workspace tabs, at the right of the menu bar.
    fn workspace_tabs(ui: &mut Ui, current: Workspace) {
        // Right-to-left layout: draw the tabs last to first.
        for w in Workspace::ALL.iter().rev() {
            let active = *w == current;
            let text = egui::RichText::new(w.label()).color(if active { egui::Color32::WHITE } else { theme::TEXT });
            let button = egui::Button::new(text)
                .fill(if active { theme::PALETTE_ACTIVE } else { egui::Color32::TRANSPARENT })
                .corner_radius(egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 })
                .min_size(egui::vec2(0.0, 22.0));
            let shortcut = commands::key_label(ui.ctx(), Cmd::Workspace(*w));
            let tip = if shortcut.is_empty() { w.description().to_owned() } else { format!("{} ({shortcut})", w.description()) };
            if ui.add(button).on_hover_text(tip).clicked() {
                commands::queue(ui.ctx(), Cmd::Workspace(*w));
            }
        }
        ui.colored_label(theme::TEXT_DIM, "Workspace");
    }

    fn menu_bar(&mut self, ui: &mut Ui) {
        use commands::menu_item as item;
        let workspace = self.workspace;
        let project_names = self.project_names();
        let ed = &mut self.ed;
        let slot = self.slot;
        let mut switch_to: Option<usize> = None;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                for c in [Cmd::New, Cmd::Open, Cmd::Close, Cmd::Revert] {
                    item(ui, ed, c);
                }
                ui.separator();
                for c in [Cmd::Save, Cmd::SaveAs] {
                    item(ui, ed, c);
                }
                ui.separator();
                item(ui, ed, Cmd::Merge);
                ui.separator();
                item(ui, ed, Cmd::Quit);
            });
            ui.menu_button("Edit", |ui| {
                item(ui, ed, Cmd::Undo);
                item(ui, ed, Cmd::Redo);
                ui.separator();
                for c in [Cmd::Cut, Cmd::Copy, Cmd::Paste, Cmd::Delete] {
                    item(ui, ed, c);
                }
                ui.separator();
                for c in [Cmd::Duplicate, Cmd::Group] {
                    item(ui, ed, c);
                }
                ui.separator();
                item(ui, ed, Cmd::Preferences);
                item(ui, ed, Cmd::CustomizeCommands);
            });
            ui.menu_button("Create", |ui| {
                objects::create_menu(ui, ed);
                ui.separator();
                ui.menu_button("Material", |ui| {
                    item(ui, ed, Cmd::NewMaterial);
                });
            });
            ui.menu_button("Light", |ui| objects::light_items(ui, ed));
            ui.menu_button("Modes", |ui| {
                for m in EditMode::ALL {
                    item(ui, ed, Cmd::Mode(m));
                }
                ui.separator();
                item(ui, ed, Cmd::ToggleAxisMode);
                item(ui, ed, Cmd::CenterAxis);
                item(ui, ed, Cmd::ToggleTweak);
                ui.separator();
                ui.menu_button("Snapping", |ui| snap_menu(ui, ed));
                ui.menu_button("Quantizing", |ui| quantize_menu(ui, ed));
                ui.menu_button("Workplane", |ui| workplane_menu(ui, ed));
                ui.separator();
                item(ui, ed, Cmd::MakeEditable);
                ui.separator();
                item(ui, ed, Cmd::ToggleCoordSystem);
                for i in 0..3 {
                    item(ui, ed, Cmd::ToggleAxis(i));
                }
            });
            ui.menu_button("Select", |ui| {
                for c in [Cmd::SelectAll, Cmd::DeselectAll, Cmd::InvertSelection] {
                    item(ui, ed, c);
                }
                ui.separator();
                item(ui, ed, Cmd::SelectChildren);
                widgets::menu_item_enabled(ui, false, None, "Loop Selection: double-click an edge", "");
            });
            ui.menu_button("Tools", |ui| {
                for t in [Tool::Select, Tool::Move, Tool::Scale, Tool::Rotate] {
                    item(ui, ed, Cmd::Tool(t));
                }
                ui.separator();
                item(ui, ed, Cmd::Pen(crate::pen::PenKind::Spline));
                item(ui, ed, Cmd::Pen(crate::pen::PenKind::Polygon));
                ui.separator();
                for t in MESH_TOOLS {
                    item(ui, ed, Cmd::MeshTool(t));
                }
            });
            ui.menu_button("Mesh", |ui| mesh_menu(ui, ed));
            ui.menu_button("View", |ui| view_menu(ui, ed));
            ui.menu_button("Window", |ui| {
                item(ui, ed, Cmd::Commander);
                item(ui, ed, Cmd::CustomizeCommands);
                ui.separator();
                ui.menu_button("Workspace", |ui| {
                    for w in Workspace::ALL {
                        item(ui, ed, Cmd::Workspace(w));
                    }
                    ui.separator();
                    item(ui, ed, Cmd::NextWorkspace);
                    item(ui, ed, Cmd::PrevWorkspace);
                });
                item(ui, ed, Cmd::ResetLayout);
                ui.menu_button("Panels", |ui| {
                    for p in Tab::OPENABLE {
                        item(ui, ed, Cmd::ShowPanel(p));
                    }
                });
                ui.separator();
                item(ui, ed, Cmd::ToggleTimeline);
                item(ui, ed, Cmd::ToggleIconBar);
                item(ui, ed, Cmd::IconLibrary);
                // The open projects, as in Cinema 4D's Window menu.
                ui.separator();
                for (i, name) in project_names.iter().enumerate() {
                    let current = i == slot;
                    if ui.add(egui::Button::new(name.as_str()).selected(current)).clicked() {
                        switch_to = Some(i);
                        ui.close();
                    }
                }
            });
            ui.menu_button("Help", |ui| {
                item(ui, ed, Cmd::CustomizeCommands);
                item(ui, ed, Cmd::About);
            });

            // Workspace tabs, top right.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(4.0);
                Self::workspace_tabs(ui, workspace);
                ui.add_space(10.0);
                // Commander shortcut hint, like a search field.
                let sh = commands::shared(ui.ctx());
                let hint = sh.map(|s| s.keymap.label(Cmd::Commander)).unwrap_or_default();
                let label = if hint.is_empty() { "Search commands".to_owned() } else { format!("Search commands  {hint}") };
                if ui.add(egui::Button::new(egui::RichText::new(label).color(theme::TEXT_DIM)).min_size(egui::vec2(170.0, 0.0))).clicked() {
                    commands::queue(ui.ctx(), Cmd::Commander);
                }
                ui.add_space(12.0);
            });
        });
        if let Some(i) = switch_to {
            self.switch_project(i);
        }
    }

    /// The command palette under the menu bar.
    fn command_palette(&mut self, ui: &mut Ui) {
        use commands::palette_button as button;
        let ed = &mut self.ed;
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            ui.add_space(4.0);
            button(ui, ed, Cmd::Undo);
            button(ui, ed, Cmd::Redo);
            palette_separator(ui);
            for t in [Tool::Select, Tool::Move, Tool::Scale, Tool::Rotate] {
                button(ui, ed, Cmd::Tool(t));
            }
            palette_separator(ui);
            for (i, name) in ["X", "Y", "Z"].into_iter().enumerate() {
                axis_toggle(ui, name, theme::axis_color(i), &mut ed.axes.0[i]);
            }
            // The coordinate system button shows the current system.
            let world = ed.coord == CoordSystem::World;
            let icon = if world { Icon::WorldCoords } else { Icon::ObjectCoords };
            let tip = if world { "Coordinate system: World (click for Object)" } else { "Coordinate system: Object (click for World)" };
            if widgets::palette_button(ui, icon, false, tip).clicked() {
                commands::queue(ui.ctx(), Cmd::ToggleCoordSystem);
            }
            palette_separator(ui);

            // Object groups: click runs the last used, hold opens the group.
            let prims: Vec<(Icon, &str)> = PrimitiveKind::ALL.iter().map(|&k| (icons::for_primitive(k), k.name())).collect();
            if let Some(i) = flyout(ui, "primitives", &prims, "Primitive objects") {
                commands::queue(ui.ctx(), Cmd::Primitive(PrimitiveKind::ALL[i]));
            }
            // Pens: click uses the last one, hold for both.
            let pens = [(Icon::SplinePen, "Spline Pen"), (Icon::PolygonPen, "Polygon Pen")];
            if let Some(i) = flyout(ui, "pens", &pens, "Spline Pen and Polygon Pen") {
                let k = [crate::pen::PenKind::Spline, crate::pen::PenKind::Polygon][i];
                commands::queue(ui.ctx(), Cmd::Pen(k));
            }
            button(ui, ed, Cmd::Generator(GeneratorKind::Subdivision));
            let modeling = [(Icon::Array, "Array"), (Icon::Symmetry, "Symmetry"), (Icon::Boolean, "Boolean")];
            if let Some(i) = flyout(ui, "modeling", &modeling, "Modeling generators") {
                let k = [GeneratorKind::Array, GeneratorKind::Symmetry, GeneratorKind::Boolean][i];
                commands::queue(ui.ctx(), Cmd::Generator(k));
            }
            let deformers: Vec<(Icon, &str)> = DeformerKind::ALL
                .iter()
                .map(|&k| (icons::for_kind(&crate::scene::ObjectKind::Deformer(k.default_deformer())), k.name()))
                .collect();
            if let Some(i) = flyout(ui, "deformers", &deformers, "Deformers (added under the selected object)") {
                commands::queue(ui.ctx(), Cmd::Deformer(DeformerKind::ALL[i]));
            }
            button(ui, ed, Cmd::Null);
            palette_separator(ui);
            button(ui, ed, Cmd::ToggleSolo);
        });
    }

    /// The vertical mode palette on the left.
    fn mode_palette(&mut self, ui: &mut Ui) {
        use commands::palette_button as button;
        let ed = &self.ed;
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.add_space(4.0);
            button(ui, ed, Cmd::MakeEditable);
            ui.add_space(4.0);
            for m in EditMode::ALL {
                button(ui, ed, Cmd::Mode(m));
            }
            ui.add_space(6.0);
            for c in [Cmd::ToggleAxisMode, Cmd::ToggleTweak, Cmd::ToggleSnap, Cmd::ToggleQuantize] {
                button(ui, ed, c);
            }
            if ed.mode.is_component() {
                ui.add_space(6.0);
                ui.separator();
                for t in MESH_TOOLS {
                    button(ui, ed, Cmd::MeshTool(t));
                }
            }
        });
    }

    /// Runs one command, remembering it for the Commander.
    fn run(&mut self, cmd: Cmd) {
        if !matches!(cmd, Cmd::Commander | Cmd::Cancel) {
            self.recent.retain(|c| *c != cmd);
            self.recent.insert(0, cmd);
            self.recent.truncate(8);
        }
        let Some(action) = commands::execute(&mut self.ed, cmd) else { return };
        match action {
            AppAction::New => self.guard(Pending::New),
            AppAction::Open => self.guard(Pending::Open),
            AppAction::Close => self.guard(Pending::Close),
            AppAction::Revert => {
                if self.ed.file_path.is_some() {
                    self.guard(Pending::Revert);
                } else {
                    self.ed.status = "Revert: this project has never been saved".to_owned();
                }
            }
            AppAction::Merge => {
                let path = self.ed.file_path.as_ref().and_then(|p| p.parent()).map_or_else(
                    || self.ed.default_path(),
                    |d| d.join("scene.forma3d.json").display().to_string(),
                );
                self.ed.dialog = Some(Dialog::Merge { path, error: None });
            }
            AppAction::Cut => self.copy(true),
            AppAction::Copy => self.copy(false),
            AppAction::Paste => self.paste(),
            AppAction::Preferences => self.prefs_open = true,
            AppAction::Save => self.save(),
            AppAction::SaveAs => self.save_as(),
            AppAction::Quit => self.guard(Pending::Quit),
            AppAction::Workspace(w) => self.switch_workspace(w),
            AppAction::CycleWorkspace(step) => self.cycle_workspace(step),
            AppAction::ResetLayout => self.reset_workspace(),
            AppAction::ShowPanel(p) => self.show_panel(p),
            AppAction::Commander => self.commander.open(),
            AppAction::CustomizeCommands => self.customize.open(),
            AppAction::IconLibrary => {
                self.ed.show_icon_bar = true;
                self.icon_bar.library_open = true;
            }
            AppAction::About => self.ed.dialog = Some(Dialog::About),
            AppAction::OpenMaterialEditor(m) => self.open_material = Some(m),
            AppAction::SaveAsset => {
                let Some(active) = self.ed.active() else { return };
                let name = self.ed.scene.get(active).map(|o| o.name.clone()).unwrap_or_else(|| "Asset".to_owned());
                let clip = self.ed.scene.extract(&self.ed.selection);
                self.ed.status = match self.assets.save_scene(&name, clip) {
                    Ok(_) => format!("Saved \"{name}\" to the Asset Browser (My Assets)"),
                    Err(e) => format!("Could not save the asset: {e}"),
                };
            }
        }
    }

    fn save_keymap(&mut self) {
        if let Some(path) = &self.keymap_path
            && let Err(e) = commands::save_keymap(&self.keymap, &self.infos, path) {
                self.ed.status = format!("Could not save shortcuts: {e}");
            }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(6.0);
            if let Some((first, _)) = self.prefix {
                // Waiting for the second key: list what can follow, like a key guide.
                let names: Vec<String> = self
                    .keymap
                    .completions(first)
                    .into_iter()
                    .map(|(k, c)| {
                        let name = self.infos.iter().find(|i| i.cmd == c).map_or("?", |i| i.name.as_str());
                        format!("{} {}", k.label(), name)
                    })
                    .collect();
                ui.colored_label(theme::ACCENT, format!("{}~", first.label()));
                ui.colored_label(theme::TEXT, names.join("   ·   "));
            } else {
                ui.colored_label(theme::TEXT_DIM, &self.ed.status);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                ui.colored_label(
                    theme::TEXT_DIM,
                    if self.ed.mode.is_component() && self.ed.edit_target().is_some() {
                        format!("{} mode | {} selected", self.ed.mode.label(), self.ed.comp.count(self.ed.mode))
                    } else {
                        format!("{} objects | {} selected", self.ed.scene.len(), self.ed.selection.len())
                    },
                );
            });
        });
    }

    /// Keyboard shortcuts from the keymap (exact modifiers), including
    /// two-key sequences: the first key of a sequence waits (up to a few
    /// seconds) for the second. Matched key presses are removed so menus and
    /// popups don't also react to them.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        const SEQUENCE_TIMEOUT: f64 = 3.0;
        let now = ctx.input(|i| i.time);
        if self.prefix.is_some_and(|(_, t)| now - t > SEQUENCE_TIMEOUT) {
            self.prefix = None;
        }
        if ctx.egui_wants_keyboard_input() || self.customize.capturing.is_some() || self.commander.open {
            self.prefix = None;
            return;
        }
        let keymap = &self.keymap;
        let mut prefix = self.prefix;
        let mut hits = Vec::new();
        let mut missed = None;
        let mut pasted_text: Option<String> = None;
        ctx.input_mut(|i| {
            i.events.retain(|e| {
                // egui turns Ctrl+C / X / V into clipboard events instead of
                // key presses: run whatever those keys are bound to.
                let clip_key = match e {
                    Event::Copy => Some(Key::C),
                    Event::Cut => Some(Key::X),
                    Event::Paste(text) => {
                        pasted_text = Some(text.clone());
                        Some(Key::V)
                    }
                    _ => None,
                };
                if let Some(k) = clip_key {
                    if let Some(cmd) = keymap.command_for(k, egui::Modifiers::COMMAND) {
                        hits.push(cmd);
                    }
                    return false;
                }
                let Event::Key { key, pressed: true, repeat: false, modifiers, .. } = e else { return true };
                if let Some((first, _)) = prefix.take() {
                    // Second key of a sequence; anything else ends it.
                    match keymap.command_for_seq(first, *key, *modifiers) {
                        Some(cmd) => hits.push(cmd),
                        None if *key == Key::Escape => {}
                        None => missed = Some(format!("{}~{} is not a shortcut", first.label(), commands::Shortcut::from_event(*key, *modifiers).label())),
                    }
                    return false;
                }
                let sc = commands::Shortcut::from_event(*key, *modifiers);
                if keymap.is_prefix(sc) {
                    prefix = Some((sc, now));
                    return false;
                }
                if let Some(cmd) = keymap.command_for(*key, *modifiers) {
                    hits.push(cmd);
                    return false;
                }
                true
            });
        });
        self.prefix = prefix;
        // Objects copied in another Forma3D window arrive as scene text.
        if let Some(text) = pasted_text
            && let Ok(scene) = Scene::from_json(&text)
        {
            self.clipboard = Some(scene);
        }
        if let Some(m) = missed {
            self.ed.status = m;
        }
        if self.prefix.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        for cmd in hits {
            commands::queue(ctx, cmd);
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.ed.dialog.take() else { return };
        let mut keep = Some(dialog.clone());
        match dialog {
            Dialog::Open { mut path, error } => {
                let mut err = error;
                let mut open = true;
                egui::Window::new("Open Scene").collapsible(false).open(&mut open).show(ctx, |ui| {
                    ui.label("Path of a .json scene file:");
                    let r = ui.add(egui::TextEdit::singleline(&mut path).desired_width(420.0));
                    if let Some(e) = &err {
                        ui.colored_label(egui::Color32::from_rgb(230, 90, 80), e);
                    }
                    ui.horizontal(|ui| {
                        let go = ui.button("Open").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
                        if go {
                            match self.open_project(&PathBuf::from(path.trim())) {
                                Ok(()) => keep = None,
                                Err(e) => err = Some(e),
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            keep = None;
                        }
                    });
                });
                if !open {
                    keep = None;
                }
                if keep.is_some() {
                    keep = Some(Dialog::Open { path, error: err });
                }
            }
            Dialog::SaveAs { mut path, error } => {
                let mut err = error;
                let mut open = true;
                egui::Window::new("Save Scene As").collapsible(false).open(&mut open).show(ctx, |ui| {
                    ui.label("Where to save the scene:");
                    let r = ui.add(egui::TextEdit::singleline(&mut path).desired_width(420.0));
                    if let Some(e) = &err {
                        ui.colored_label(egui::Color32::from_rgb(230, 90, 80), e);
                    }
                    ui.horizontal(|ui| {
                        let go = ui.button("Save").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
                        if go {
                            match self.ed.save_to(&PathBuf::from(path.trim())) {
                                Ok(()) => keep = None,
                                Err(e) => err = Some(e),
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            keep = None;
                        }
                    });
                });
                if !open {
                    keep = None;
                }
                if keep.is_some() {
                    keep = Some(Dialog::SaveAs { path, error: err });
                }
            }
            Dialog::Merge { mut path, error } => {
                let mut err = error;
                let mut open = true;
                egui::Window::new("Merge Project").collapsible(false).open(&mut open).show(ctx, |ui| {
                    ui.label("Scene file whose objects and materials to add to this project:");
                    let r = ui.add(egui::TextEdit::singleline(&mut path).desired_width(420.0));
                    if let Some(e) = &err {
                        ui.colored_label(egui::Color32::from_rgb(230, 90, 80), e);
                    }
                    ui.horizontal(|ui| {
                        let go = ui.button("Merge").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
                        if go {
                            match self.ed.merge_from(&PathBuf::from(path.trim())) {
                                Ok(_) => keep = None,
                                Err(e) => err = Some(e),
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            keep = None;
                        }
                    });
                });
                if !open {
                    keep = None;
                }
                if keep.is_some() {
                    keep = Some(Dialog::Merge { path, error: err });
                }
            }
            Dialog::ConfirmDiscard(pending) => {
                let message = match pending {
                    Pending::Quit if self.unsaved_projects() > 1 => {
                        format!("{} open projects have changes that have not been saved.", self.unsaved_projects())
                    }
                    Pending::Revert => format!("Revert {} to its saved version? Your changes since then will be lost.", self.ed.project_name()),
                    _ => format!("{} has changes that have not been saved.", self.ed.project_name()),
                };
                egui::Window::new("Unsaved changes")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                    .show(ctx, |ui| {
                        ui.label(message);
                        ui.horizontal(|ui| {
                            if ui.button("Discard changes").clicked() {
                                keep = None;
                                self.perform(pending);
                            }
                            if ui.button("Cancel").clicked() {
                                keep = None;
                            }
                        });
                    });
            }
            Dialog::Shortcuts => {
                keep = None;
                self.customize.open();
            }
            Dialog::About => {
                let mut open = true;
                egui::Window::new("About Forma3D").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                    ui.heading("Forma3D");
                    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.label("A 3D modelling prototype in Rust with a Cinema 4D-style workflow.");
                    ui.label("Original code; no Blender or Cinema 4D source, assets or branding.");
                });
                if !open {
                    keep = None;
                }
            }
        }
        // A handler may have opened a different dialog meanwhile; that one wins.
        if self.ed.dialog.is_none() {
            self.ed.dialog = keep;
        }
    }
}

/// Modelling commands, shared by the Mesh menu and the viewport context menu.
/// Modelling commands, shared by the Mesh menu and the viewport context menu.
/// Modes > Snapping.
pub(crate) fn snap_menu(ui: &mut Ui, ed: &mut Editor) {
    use commands::menu_item as item;
    item(ui, ed, Cmd::ToggleSnap);
    ui.separator();
    for m in crate::snap::SnapMode::ALL {
        item(ui, ed, Cmd::SnapMode(m));
    }
    ui.separator();
    for k in crate::snap::SnapKind::ALL {
        item(ui, ed, Cmd::SnapTo(k));
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Radius");
        ui.add(egui::DragValue::new(&mut ed.snap.radius).range(2.0..=60.0).speed(0.5).suffix(" px"));
    });
}

/// Modes > Quantizing: the switch and the step sizes.
pub(crate) fn quantize_menu(ui: &mut Ui, ed: &mut Editor) {
    commands::menu_item(ui, ed, Cmd::ToggleQuantize);
    ui.separator();
    egui::Grid::new("quantize-steps").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
        let q = &mut ed.quantize;
        ui.label("Move");
        ui.add(egui::DragValue::new(&mut q.move_step).range(0.01..=10000.0).speed(0.5).suffix(" cm"));
        ui.end_row();
        ui.label("Rotate");
        ui.add(egui::DragValue::new(&mut q.rotate_step).range(0.1..=180.0).speed(0.5).suffix(" °"));
        ui.end_row();
        ui.label("Scale");
        ui.add(egui::DragValue::new(&mut q.scale_step).range(0.1..=500.0).speed(0.5).suffix(" %"));
        ui.end_row();
    });
}

/// Modes > Workplane.
pub(crate) fn workplane_menu(ui: &mut Ui, ed: &mut Editor) {
    use commands::menu_item as item;
    item(ui, ed, Cmd::Mode(EditMode::Workplane));
    ui.separator();
    for a in 0..3 {
        item(ui, ed, Cmd::AlignWorkplane(a));
    }
    item(ui, ed, Cmd::AlignWorkplaneToSelection);
    item(ui, ed, Cmd::AlignSelectionToWorkplane);
}

pub(crate) fn mesh_menu(ui: &mut Ui, ed: &Editor) {
    use commands::menu_item as item;
    item(ui, ed, Cmd::MakeEditable);
    ui.separator();
    item(ui, ed, Cmd::Pen(crate::pen::PenKind::Polygon));
    for t in MESH_TOOLS {
        item(ui, ed, Cmd::MeshTool(t));
    }
    ui.separator();
    for c in [Cmd::Bridge, Cmd::Weld, Cmd::Optimize, Cmd::SubdivideSmooth, Cmd::SubdivideLinear] {
        item(ui, ed, c);
    }
    ui.separator();
    for c in [Cmd::Delete, Cmd::SelectAll, Cmd::ReverseNormals] {
        item(ui, ed, c);
    }
}

/// View commands (main menu; the viewport strips have their own menus too).
pub(crate) fn view_menu(ui: &mut Ui, ed: &Editor) {
    use commands::menu_item as item;
    item(ui, ed, Cmd::FrameSelected);
    item(ui, ed, Cmd::FrameAll);
    ui.separator();
    for k in crate::camera::ViewKind::ALL {
        item(ui, ed, Cmd::Camera(k));
    }
    item(ui, ed, Cmd::FourViews);
    ui.separator();
    for m in crate::renderer::DisplayMode::ALL {
        item(ui, ed, Cmd::Display(m));
    }
    ui.separator();
    for c in [Cmd::ToggleGrid, Cmd::ToggleCompass, Cmd::ToggleSolo] {
        item(ui, ed, c);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Closing with unsaved changes asks first.
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.unsaved_projects() > 0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.ed.dialog = Some(Dialog::ConfirmDiscard(Pending::Quit));
        }
        if self.ed.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        commands::publish(
            &ctx,
            Shared { infos: self.infos.clone(), keymap: Arc::new(self.keymap.clone()), layout: self.workspace },
        );
        self.shortcuts(&ctx);

        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("toolbar").exact_size(38.0).show(ui, |ui| self.command_palette(ui));
        egui::Panel::bottom("status").exact_size(22.0).show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("palette").exact_size(40.0).resizable(false).show(ui, |ui| self.mode_palette(ui));

        let style = theme::dock_style(ui.style());
        let time = ctx.input(|i| i.time);
        let mut viewer = Viewer {
            ed: &mut self.ed,
            icon_bar: &mut self.icon_bar,
            srgb: self.srgb,
            assets: &mut self.assets,
            console: &mut self.console,
            time,
        };
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                DockArea::new(&mut self.dock)
                    .style(style)
                    .show_close_buttons(true)
                    .show_add_buttons(false)
                    .show_leaf_close_all_buttons(false)
                    .hidable_tab_bars(true)
                    .show_leaf_collapse_buttons(false)
                    .show_inside(ui, &mut viewer);
            });

        self.dialogs(&ctx);
        self.icon_bar.library(&ctx, &mut self.ed);
        let saved_to = self.prefs_path.as_ref().map_or("(nowhere: no config folder)".to_owned(), |p| p.display().to_string());
        let pref = preferences::window(&ctx, &mut self.prefs_open, &mut self.prefs, &saved_to);
        if pref.changed {
            for ed in self.others.iter_mut().chain(std::iter::once(&mut self.ed)) {
                self.prefs.apply_to(ed);
            }
            if let Some(path) = &self.prefs_path
                && let Err(e) = self.prefs.save(path)
            {
                self.ed.status = format!("Could not save preferences: {e}");
            }
        }
        if pref.open_shortcuts {
            self.customize.open();
        }
        if self.applied_scale != self.prefs.ui_scale {
            ctx.set_zoom_factor(self.prefs.ui_scale / 100.0);
            self.applied_scale = self.prefs.ui_scale;
        }
        if let Some(text) = self.clipboard_out.take() {
            ctx.copy_text(text);
        }
        self.autosave(ctx.input(|i| i.time));
        // Save rearranged workspaces once a drag or click has finished.
        if ctx.input(|i| i.pointer.any_released()) || self.saved_workspaces.is_empty() {
            self.save_workspaces();
        }

        // Commander and Customize Commands float above everything.
        if let Some(sh) = commands::shared(&ctx) {
            let extra = if self.commander.open { commands::dynamic(&self.ed) } else { Vec::new() };
            if let Some(cmd) = self.commander.show(&ctx, &self.ed, &sh, &extra, &self.recent) {
                commands::queue(&ctx, cmd);
            }
        }
        if let Some(m) = self.open_material.take() {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("material-editor"), Some(m)));
        }
        let saved_to = self.keymap_path.as_ref().map_or("(nowhere: no config folder)".to_owned(), |p| p.display().to_string());
        if self.customize.show(&ctx, &self.infos, &mut self.keymap, &saved_to) {
            self.save_keymap();
        }

        // Every queued command (menus, palettes, keys, Commander) runs here.
        for cmd in commands::take_queue(&ctx) {
            self.run(cmd);
        }

        if let Some(e) = self.icon_bar.save_if_changed(&self.infos) {
            self.ed.status = format!("Could not save the icon bar: {e}");
        }

        // The Console logs every status message.
        let name = self.ed.project_name();
        self.console.watch_status(ctx.input(|i| i.time), &name, &self.ed.status);

        // End of frame bookkeeping.
        let gesture = ctx.input(|i| i.pointer.any_down()) || ctx.egui_wants_keyboard_input();
        self.ed.prune_selection();
        self.ed.commit_history(gesture);
        self.ed.meshes.end_frame();

        let title = self.ed.title();
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }
}

#[cfg(test)]
mod workspace_tests {
    use super::*;

    fn panels(dock: &DockState<Tab>) -> Vec<Tab> {
        dock.iter_all_tabs().map(|(_, t)| *t).collect()
    }

    #[test]
    fn every_workspace_has_the_viewport_and_its_own_panels() {
        for w in Workspace::ALL {
            let p = panels(&build_dock(w));
            assert_eq!(p.iter().filter(|t| **t == Tab::Viewport).count(), 1, "{w:?}");
            assert!(p.contains(&Tab::Attributes), "{w:?} has no Attribute Manager");
        }
        assert!(panels(&build_dock(Workspace::Track)).contains(&Tab::Footage));
        assert!(panels(&build_dock(Workspace::UvEdit)).contains(&Tab::UvEditor));
        assert!(panels(&build_dock(Workspace::Sculpt)).contains(&Tab::Brushes));
        assert!(panels(&build_dock(Workspace::Model)).contains(&Tab::Materials));
    }

    #[test]
    fn saved_workspaces_round_trip() {
        let mut docks = HashMap::new();
        for w in Workspace::ALL {
            docks.insert(w, build_dock(w));
        }
        let timelines = HashMap::from([(Workspace::Custom, false)]);
        let saved = SavedWorkspaces { active: Workspace::Sculpt, docks, timelines };
        let text = workspaces_to_json(&saved).unwrap();
        let dir = std::env::temp_dir().join(format!("forma3d-ws-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("workspaces.json");
        std::fs::write(&path, &text).unwrap();
        let back = load_workspaces(Some(&path)).unwrap();
        assert_eq!(back.active, Workspace::Sculpt);
        assert_eq!(back.timelines.get(&Workspace::Custom), Some(&false));
        assert_eq!(panels(&back.docks[&Workspace::Track]), panels(&build_dock(Workspace::Track)));
        // Garbage gives None (the defaults are used).
        std::fs::write(&path, "not json").unwrap();
        assert!(load_workspaces(Some(&path)).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workspace_commands_are_registered() {
        let infos = commands::all();
        for w in Workspace::ALL {
            assert!(infos.iter().any(|i| i.cmd == Cmd::Workspace(w)), "{w:?}");
        }
        for p in Tab::OPENABLE {
            assert!(infos.iter().any(|i| i.cmd == Cmd::ShowPanel(p)), "{p:?}");
        }
    }
}
