//! The app's shared state: the open projects (each an engine `Editor`), the
//! settings, and signals that tell the interface when to redraw.

use crate::commands::Cmd;
use crate::keymap::{Keymap, Shortcut};
use crate::prefs::Prefs;
use crate::workspace::{Layout, Panel, Saved, Workspace};
use forma3d_core::editor::Editor;
use forma3d_core::scene::Scene;
use leptos::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttrTab {
    Basic,
    Coord,
    Object,
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub time: f64,
    pub text: String,
    pub error: bool,
}

/// A question shown in a dialog (e.g. "discard unsaved changes?").
#[derive(Clone, Debug)]
pub struct Ask {
    pub title: String,
    pub text: String,
    pub ok: String,
    pub then: Cmd,
}

/// Handles to everything; `Copy`, so closures can take it freely.
#[derive(Clone, Copy)]
pub struct App {
    /// The current project.
    pub ed: StoredValue<Editor, LocalStorage>,
    /// The other open projects, in Window menu order; `ed` sits at `slot`.
    pub others: StoredValue<Vec<Editor>, LocalStorage>,
    pub slot: RwSignal<usize>,
    /// Bumped after every change to the editor.
    pub rev: RwSignal<u64>,
    /// Bumped when only the view changed (camera moves, hover), to redraw.
    pub view_rev: RwSignal<u64>,
    pub menu: RwSignal<Option<&'static str>>,
    pub commander: RwSignal<bool>,
    pub attr_tab: RwSignal<AttrTab>,
    pub log: RwSignal<Vec<LogEntry>>,
    pub ask: RwSignal<Option<Ask>>,
    pub about: RwSignal<bool>,
    pub prefs_open: RwSignal<bool>,
    pub customize_open: RwSignal<bool>,
    pub clipboard: StoredValue<Option<Scene>, LocalStorage>,
    pub renaming: RwSignal<Option<u64>>,
    pub gpu_name: RwSignal<String>,
    pub keymap: RwSignal<Keymap>,
    /// First key of a two-key sequence (M of M~S) and when it was pressed.
    pub prefix: RwSignal<Option<(Shortcut, f64)>>,
    /// Most recently run commands, newest first (the Commander shows them first).
    pub recent: RwSignal<Vec<Cmd>>,
    pub prefs: RwSignal<Prefs>,
    pub workspace: RwSignal<Workspace>,
    /// The current workspace's arrangement.
    pub layout: RwSignal<Layout>,
    /// The other workspaces' arrangements.
    pub layouts: StoredValue<HashMap<Workspace, Layout>, LocalStorage>,
    /// Settings have been read (until then nothing is written back).
    pub loaded: RwSignal<bool>,
    /// Customize Commands: the command waiting for keys, and the first key
    /// typed so far (a second key makes a sequence).
    pub capture: RwSignal<Option<(Cmd, Option<Shortcut>)>>,
    pub capture_msg: RwSignal<String>,
}

impl App {
    pub fn new() -> Self {
        let mut ed = Editor::new(Scene::demo());
        ed.status = "Ready".to_owned();
        App {
            ed: StoredValue::new_local(ed),
            others: StoredValue::new_local(Vec::new()),
            slot: RwSignal::new(0),
            rev: RwSignal::new(0),
            view_rev: RwSignal::new(0),
            menu: RwSignal::new(None),
            commander: RwSignal::new(false),
            attr_tab: RwSignal::new(AttrTab::Coord),
            log: RwSignal::new(Vec::new()),
            ask: RwSignal::new(None),
            about: RwSignal::new(false),
            prefs_open: RwSignal::new(false),
            customize_open: RwSignal::new(false),
            clipboard: StoredValue::new_local(None),
            renaming: RwSignal::new(None),
            gpu_name: RwSignal::new(String::new()),
            keymap: RwSignal::new(Keymap::default()),
            prefix: RwSignal::new(None),
            recent: RwSignal::new(Vec::new()),
            prefs: RwSignal::new(Prefs::default()),
            workspace: RwSignal::new(Workspace::Model),
            layout: RwSignal::new(Workspace::Model.default_layout()),
            layouts: StoredValue::new_local(HashMap::new()),
            loaded: RwSignal::new(false),
            capture: RwSignal::new(None),
            capture_msg: RwSignal::new(String::new()),
        }
    }

    /// Reads the editor and re-runs the caller when it changes.
    pub fn read<R>(&self, f: impl FnOnce(&Editor) -> R) -> R {
        self.rev.track();
        self.ed.with_value(f)
    }

    /// Reads the editor without subscribing.
    pub fn peek<R>(&self, f: impl FnOnce(&Editor) -> R) -> R {
        self.ed.with_value(f)
    }

    /// Changes the editor as one undo step and refreshes the interface.
    pub fn act<R>(&self, f: impl FnOnce(&mut Editor) -> R) -> R {
        let (r, status) = self
            .ed
            .try_update_value(|e| {
                let before = e.status.clone();
                let r = f(e);
                e.prune_selection();
                e.commit_history(false);
                (r, (e.status != before).then(|| e.status.clone()))
            })
            .expect("editor is alive");
        if let Some(s) = status {
            self.log_line(s, false);
        }
        self.rev.update(|r| *r += 1);
        r
    }

    /// Changes the editor during a drag (undo commits when it ends).
    pub fn gesture(&self, f: impl FnOnce(&mut Editor)) {
        self.ed.update_value(|e| {
            f(e);
            e.commit_history(true);
        });
        self.rev.update(|r| *r += 1);
    }

    /// Changes something only the view shows (the camera): redraw, no undo.
    pub fn view<R>(&self, f: impl FnOnce(&mut Editor) -> R) -> R {
        let r = self.ed.try_update_value(f).expect("editor is alive");
        self.view_rev.update(|r| *r += 1);
        r
    }

    pub fn log_line(&self, text: impl Into<String>, error: bool) {
        let text = text.into();
        if text.trim().is_empty() {
            return;
        }
        let time = window()
            .performance()
            .map(|p| p.now() / 1000.0)
            .unwrap_or(0.0);
        self.log.update(|l| {
            if l.last().is_some_and(|x| x.text == text) {
                return;
            }
            l.push(LogEntry { time, text, error });
            if l.len() > 1000 {
                l.drain(..l.len() - 1000);
            }
        });
    }

    pub fn status(&self, text: impl Into<String>) {
        let text = text.into();
        self.ed.update_value(|e| e.status = text.clone());
        self.log_line(text, false);
        self.rev.update(|r| *r += 1);
    }

    pub fn error(&self, text: impl Into<String>) {
        let text = text.into();
        self.ed.update_value(|e| e.status = text.clone());
        self.log_line(text, true);
        self.rev.update(|r| *r += 1);
    }

    // ---- projects ----------------------------------------------------------

    /// A fresh, empty project set up from Preferences.
    pub fn blank_project(&self) -> Editor {
        let mut ed = Editor::new(Scene::new());
        self.prefs.with_untracked(|p| p.apply_to_new(&mut ed));
        ed
    }

    pub fn project_count(&self) -> usize {
        self.others.with_value(|o| o.len()) + 1
    }

    /// Makes `ed` the current project; the old one stays open behind it.
    pub fn add_project(&self, ed: Editor) {
        let old = self
            .ed
            .try_update_value(|cur| std::mem::replace(cur, ed))
            .expect("editor is alive");
        let slot = self.slot.get_untracked();
        self.others
            .update_value(|o| o.insert(slot.min(o.len()), old));
        self.slot.set(slot + 1);
        self.rev.update(|r| *r += 1);
    }

    /// Puts `ed` in place of the current project (an untouched Untitled one).
    pub fn replace_project(&self, ed: Editor) {
        self.ed.set_value(ed);
        self.rev.update(|r| *r += 1);
    }

    /// Closes the current project; closing the last one leaves an empty Untitled.
    pub fn close_project(&self) {
        let name = self.peek(|e| e.project_name());
        let next = if self.others.with_value(|o| o.is_empty()) {
            self.blank_project()
        } else {
            let slot = self.slot.get_untracked();
            let i = slot.min(self.others.with_value(|o| o.len()) - 1);
            self.slot.set(i);
            self.others
                .try_update_value(|o| o.remove(i))
                .expect("projects are alive")
        };
        self.ed.set_value(next);
        self.status(format!("Closed {name}"));
    }

    /// Brings project `index` (Window menu order) to the front.
    pub fn switch_project(&self, index: usize) {
        let slot = self.slot.get_untracked();
        let n = self.project_count();
        if index == slot || index >= n {
            return;
        }
        let target = if index < slot { index } else { index - 1 };
        let ed = self
            .others
            .try_update_value(|o| o.remove(target))
            .expect("projects are alive");
        let old = self
            .ed
            .try_update_value(|cur| std::mem::replace(cur, ed))
            .expect("editor is alive");
        let at = if index < slot { slot - 1 } else { slot };
        self.others.update_value(|o| o.insert(at.min(o.len()), old));
        self.slot.set(index);
        self.rev.update(|r| *r += 1);
        self.view_rev.update(|r| *r += 1);
    }

    /// Names of all open projects in Window menu order, marking unsaved ones.
    pub fn project_names(&self) -> Vec<String> {
        self.rev.track();
        let slot = self.slot.get();
        let mut names: Vec<String> = self.others.with_value(|o| {
            o.iter()
                .map(|e| {
                    format!(
                        "{}{}",
                        e.project_name(),
                        if e.is_dirty() { " •" } else { "" }
                    )
                })
                .collect()
        });
        let cur = self.peek(|e| {
            format!(
                "{}{}",
                e.project_name(),
                if e.is_dirty() { " •" } else { "" }
            )
        });
        names.insert(slot.min(names.len()), cur);
        names
    }

    pub fn unsaved_projects(&self) -> usize {
        self.others
            .with_value(|o| o.iter().filter(|e| e.is_dirty()).count())
            + usize::from(self.peek(|e| e.is_dirty()))
    }

    /// Runs `f` on every open project (for settings that apply to all).
    pub fn each_project(&self, f: impl Fn(&mut Editor)) {
        self.ed.update_value(&f);
        self.others.update_value(|o| o.iter_mut().for_each(&f));
    }

    // ---- workspaces --------------------------------------------------------

    pub fn timeline_shown(&self) -> bool {
        self.layout.with(|l| l.timeline)
    }

    pub fn timeline_shown_untracked(&self) -> bool {
        self.layout.with_untracked(|l| l.timeline)
    }

    pub fn edit_layout(&self, f: impl FnOnce(&mut Layout)) {
        self.layout.update(f);
        crate::settings::save_workspaces(*self);
    }

    pub fn toggle_timeline(&self) {
        self.edit_layout(|l| l.timeline = !l.timeline);
    }

    pub fn switch_workspace(&self, to: Workspace) {
        let from = self.workspace.get_untracked();
        if to == from {
            return;
        }
        let current = self.layout.get_untracked();
        let next = self.layouts.try_update_value(|m| {
            m.insert(from, current);
            m.remove(&to)
        });
        self.layout
            .set(next.flatten().unwrap_or_else(|| to.default_layout()));
        self.workspace.set(to);
        self.status(format!("Workspace: {}", to.label()));
        crate::settings::save_workspaces(*self);
    }

    pub fn reset_workspace(&self) {
        let w = self.workspace.get_untracked();
        self.edit_layout(|l| *l = w.default_layout());
        self.status(format!("Reset the {} workspace", w.label()));
    }

    pub fn show_panel(&self, p: Panel) {
        self.edit_layout(|l| l.show(p));
    }

    pub fn saved_workspaces(&self) -> Saved {
        let mut layouts = self.layouts.get_value();
        layouts.insert(self.workspace.get_untracked(), self.layout.get_untracked());
        Saved {
            active: Some(self.workspace.get_untracked()),
            layouts,
        }
    }

    pub fn load_workspaces(&self, saved: Saved) {
        let mut layouts = saved.layouts;
        for l in layouts.values_mut() {
            l.sanitize();
        }
        let active = saved.active.unwrap_or(Workspace::Model);
        let layout = layouts
            .remove(&active)
            .unwrap_or_else(|| active.default_layout());
        self.layouts.set_value(layouts);
        self.workspace.set(active);
        self.layout.set(layout);
    }
}

pub fn app() -> App {
    use_context::<App>().expect("App context")
}
