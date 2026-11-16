//! The app's shared state: the engine's `Editor`, plus signals that tell the
//! interface when to redraw.

use forma3d_core::editor::Editor;
use forma3d_core::scene::Scene;
use leptos::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BottomTab {
    Materials,
    Assets,
    Console,
}

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
    pub then: crate::commands::Cmd,
}

/// Handles to everything; `Copy`, so closures can take it freely.
#[derive(Clone, Copy)]
pub struct App {
    pub ed: StoredValue<Editor, LocalStorage>,
    /// Bumped after every change to the editor.
    pub rev: RwSignal<u64>,
    /// Bumped when only the view changed (camera moves, hover), to redraw.
    pub view_rev: RwSignal<u64>,
    pub menu: RwSignal<Option<&'static str>>,
    pub commander: RwSignal<bool>,
    pub bottom: RwSignal<BottomTab>,
    pub attr_tab: RwSignal<AttrTab>,
    pub log: RwSignal<Vec<LogEntry>>,
    pub ask: RwSignal<Option<Ask>>,
    pub about: RwSignal<bool>,
    pub clipboard: StoredValue<Option<Scene>, LocalStorage>,
    pub renaming: RwSignal<Option<u64>>,
    pub gpu_name: RwSignal<String>,
}

impl App {
    pub fn new() -> Self {
        let mut ed = Editor::new(Scene::demo());
        ed.status = "Ready".to_owned();
        App {
            ed: StoredValue::new_local(ed),
            rev: RwSignal::new(0),
            view_rev: RwSignal::new(0),
            menu: RwSignal::new(None),
            commander: RwSignal::new(false),
            bottom: RwSignal::new(BottomTab::Materials),
            attr_tab: RwSignal::new(AttrTab::Coord),
            log: RwSignal::new(Vec::new()),
            ask: RwSignal::new(None),
            about: RwSignal::new(false),
            clipboard: StoredValue::new_local(None),
            renaming: RwSignal::new(None),
            gpu_name: RwSignal::new(String::new()),
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
    pub fn view(&self, f: impl FnOnce(&mut Editor)) {
        self.ed.update_value(f);
        self.view_rev.update(|r| *r += 1);
    }

    pub fn log_line(&self, text: impl Into<String>, error: bool) {
        let text = text.into();
        if text.trim().is_empty() {
            return;
        }
        let time = window().performance().map(|p| p.now() / 1000.0).unwrap_or(0.0);
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
}

pub fn app() -> App {
    use_context::<App>().expect("App context")
}
