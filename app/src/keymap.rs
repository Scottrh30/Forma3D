//! Keyboard shortcuts: single keys with exact modifiers, Cinema 4D-style
//! two-key sequences (M~S: press M, then S), the defaults, and the user's
//! changes (saved as `keymap.json`, `{ "command id": ["Ctrl+Z", "M~S"] }`).

use crate::commands::{Cmd, Info};
use std::collections::{BTreeMap, HashMap};

/// Every key that can be part of a shortcut, by name. A `Key` is an index
/// into this table, so shortcuts stay `Copy` and hashable.
const NAMES: &[&str] = &[
    "A",
    "B",
    "C",
    "D",
    "E",
    "F",
    "G",
    "H",
    "I",
    "J",
    "K",
    "L",
    "M",
    "N",
    "O",
    "P",
    "Q",
    "R",
    "S",
    "T",
    "U",
    "V",
    "W",
    "X",
    "Y",
    "Z",
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "F1",
    "F2",
    "F3",
    "F4",
    "F5",
    "F6",
    "F7",
    "F8",
    "F9",
    "F10",
    "F11",
    "F12",
    "Del",
    "Esc",
    "Backspace",
    "Enter",
    "Tab",
    "Space",
    "PageUp",
    "PageDown",
    "Home",
    "End",
    "Insert",
    "Up",
    "Down",
    "Left",
    "Right",
    "Minus",
    "Equal",
    "Comma",
    "Period",
    "Slash",
    "Semicolon",
    "Quote",
    "BracketLeft",
    "BracketRight",
    "Backslash",
    "Backquote",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key(u8);

impl Key {
    pub fn named(name: &str) -> Option<Key> {
        NAMES
            .iter()
            .position(|n| n.eq_ignore_ascii_case(name))
            .map(|i| Key(i as u8))
    }

    /// `Key::of("M")`; panics on an unknown name (for the built-in tables).
    pub fn of(name: &str) -> Key {
        Key::named(name).unwrap_or_else(|| panic!("unknown key {name}"))
    }

    pub fn name(self) -> &'static str {
        NAMES[self.0 as usize]
    }

    /// The key of a browser key event, by its physical position for letters,
    /// digits and punctuation (so Shift+1 is still "1").
    pub fn from_event(ev: &web_sys::KeyboardEvent) -> Option<Key> {
        let code = ev.code();
        if let Some(l) = code.strip_prefix("Key") {
            return Key::named(l);
        }
        if let Some(d) = code
            .strip_prefix("Digit")
            .or_else(|| code.strip_prefix("Numpad"))
            .filter(|d| d.len() == 1)
        {
            return Key::named(d);
        }
        if let Some(k) = Key::named(&code) {
            return Some(k);
        }
        let key = ev.key();
        Key::named(match key.as_str() {
            "Delete" => "Del",
            "Escape" => "Esc",
            " " => "Space",
            "ArrowUp" => "Up",
            "ArrowDown" => "Down",
            "ArrowLeft" => "Left",
            "ArrowRight" => "Right",
            k => k,
        })
    }
}

/// Keys that would break text entry are not bindable.
pub fn bindable(k: Key) -> bool {
    !matches!(k.name(), "Enter" | "Tab")
}

/// One key with exact modifiers ("Ctrl" also means Cmd on macOS).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: Key,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Mods {
    pub fn of(ev: &web_sys::KeyboardEvent) -> Mods {
        Mods {
            ctrl: ev.ctrl_key() || ev.meta_key(),
            shift: ev.shift_key(),
            alt: ev.alt_key(),
        }
    }
    pub fn none(self) -> bool {
        !self.ctrl && !self.shift && !self.alt
    }
}

impl Shortcut {
    pub fn key(k: &str) -> Self {
        Shortcut {
            ctrl: false,
            shift: false,
            alt: false,
            key: Key::of(k),
        }
    }
    pub fn ctrl(k: &str) -> Self {
        Shortcut {
            ctrl: true,
            ..Shortcut::key(k)
        }
    }
    pub fn shift(k: &str) -> Self {
        Shortcut {
            shift: true,
            ..Shortcut::key(k)
        }
    }
    pub fn alt(k: &str) -> Self {
        Shortcut {
            alt: true,
            ..Shortcut::key(k)
        }
    }
    pub fn ctrl_shift(k: &str) -> Self {
        Shortcut {
            ctrl: true,
            shift: true,
            ..Shortcut::key(k)
        }
    }

    pub fn from_event(key: Key, m: Mods) -> Self {
        Shortcut {
            ctrl: m.ctrl,
            shift: m.shift,
            alt: m.alt,
            key,
        }
    }

    pub fn matches(&self, key: Key, m: Mods) -> bool {
        self.key == key && self.ctrl == m.ctrl && self.shift == m.shift && self.alt == m.alt
    }

    pub fn label(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        s.push_str(self.key.name());
        s
    }

    pub fn parse(text: &str) -> Option<Shortcut> {
        let parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let (key, mods) = parts.split_last()?;
        let mut sc = Shortcut {
            ctrl: false,
            shift: false,
            alt: false,
            key: Key::named(key)?,
        };
        for m in mods {
            match m.to_ascii_lowercase().as_str() {
                "ctrl" | "cmd" | "command" => sc.ctrl = true,
                "shift" => sc.shift = true,
                "alt" | "option" => sc.alt = true,
                _ => return None,
            }
        }
        Some(sc)
    }
}

/// A shortcut: one key, or a two-key sequence such as M~S.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Binding {
    pub first: Shortcut,
    pub then: Option<Shortcut>,
}

impl Binding {
    pub fn single(s: Shortcut) -> Self {
        Binding {
            first: s,
            then: None,
        }
    }

    pub fn seq(a: &str, b: &str) -> Self {
        Binding {
            first: Shortcut::key(a),
            then: Some(Shortcut::key(b)),
        }
    }

    pub fn label(&self) -> String {
        match self.then {
            None => self.first.label(),
            Some(t) => format!("{}~{}", self.first.label(), t.label()),
        }
    }

    pub fn parse(text: &str) -> Option<Binding> {
        match text.split_once('~') {
            None => Some(Binding::single(Shortcut::parse(text)?)),
            Some((a, b)) => Some(Binding {
                first: Shortcut::parse(a)?,
                then: Some(Shortcut::parse(b)?),
            }),
        }
    }
}

/// Why a shortcut can't be given to a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conflict {
    /// The key is the first key of these commands' sequences (M of M~S).
    StartsSequences(Vec<Cmd>),
    /// The sequence's first key is already a shortcut of this command on its own.
    FirstKeyIsShortcut(Cmd),
}

/// The active shortcuts: defaults plus the user's changes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keymap {
    /// Commands whose shortcuts the user changed (an empty list = none).
    overrides: HashMap<Cmd, Vec<Binding>>,
}

impl Keymap {
    pub fn shortcuts(&self, cmd: Cmd) -> Vec<Binding> {
        match self.overrides.get(&cmd) {
            Some(v) => v.clone(),
            None => crate::commands::default_shortcuts()
                .into_iter()
                .filter(|(c, _)| *c == cmd)
                .map(|(_, s)| s)
                .collect(),
        }
    }

    /// "Ctrl+Z", "M~S", or "" when unbound.
    pub fn label(&self, cmd: Cmd) -> String {
        self.shortcuts(cmd)
            .first()
            .map(Binding::label)
            .unwrap_or_default()
    }

    pub fn bindings(&self) -> Vec<(Binding, Cmd)> {
        let mut out: Vec<(Binding, Cmd)> = crate::commands::default_shortcuts()
            .into_iter()
            .filter(|(c, _)| !self.overrides.contains_key(c))
            .map(|(c, s)| (s, c))
            .collect();
        for (c, list) in &self.overrides {
            out.extend(list.iter().map(|s| (*s, *c)));
        }
        out
    }

    pub fn command_for(&self, key: Key, m: Mods) -> Option<Cmd> {
        self.bindings()
            .into_iter()
            .find(|(b, _)| b.then.is_none() && b.first.matches(key, m))
            .map(|(_, c)| c)
    }

    pub fn is_prefix(&self, first: Shortcut) -> bool {
        self.bindings()
            .iter()
            .any(|(b, _)| b.then.is_some() && b.first == first)
    }

    pub fn command_for_seq(&self, first: Shortcut, key: Key, m: Mods) -> Option<Cmd> {
        self.bindings()
            .into_iter()
            .find(|(b, _)| b.first == first && b.then.is_some_and(|t| t.matches(key, m)))
            .map(|(_, c)| c)
    }

    /// What can follow `first` (for the status bar hint), sorted by key.
    pub fn completions(&self, first: Shortcut) -> Vec<(Shortcut, Cmd)> {
        let mut v: Vec<(Shortcut, Cmd)> = self
            .bindings()
            .into_iter()
            .filter(|(b, _)| b.first == first)
            .filter_map(|(b, c)| Some((b.then?, c)))
            .collect();
        v.sort_by_key(|(s, _)| s.label());
        v
    }

    /// Binds `binding` to `cmd` alone. A shortcut another command had moves
    /// over (that command is returned). Refuses a single key that starts
    /// sequences, and a sequence whose first key is a shortcut on its own.
    pub fn assign(&mut self, cmd: Cmd, binding: Binding) -> Result<Option<Cmd>, Conflict> {
        let others: Vec<(Binding, Cmd)> = self
            .bindings()
            .into_iter()
            .filter(|(_, c)| *c != cmd)
            .collect();
        match binding.then {
            None => {
                let starts: Vec<Cmd> = others
                    .iter()
                    .filter(|(b, _)| b.then.is_some() && b.first == binding.first)
                    .map(|(_, c)| *c)
                    .collect();
                if !starts.is_empty() {
                    return Err(Conflict::StartsSequences(starts));
                }
            }
            Some(_) => {
                if let Some((_, c)) = others
                    .iter()
                    .find(|(b, _)| b.then.is_none() && b.first == binding.first)
                {
                    return Err(Conflict::FirstKeyIsShortcut(*c));
                }
            }
        }
        let previous = others.iter().find(|(b, _)| *b == binding).map(|(_, c)| *c);
        if let Some(other) = previous {
            let rest: Vec<Binding> = self
                .shortcuts(other)
                .into_iter()
                .filter(|b| *b != binding)
                .collect();
            self.overrides.insert(other, rest);
        }
        self.overrides.insert(cmd, vec![binding]);
        self.tidy();
        Ok(previous)
    }

    pub fn clear(&mut self, cmd: Cmd) {
        self.overrides.insert(cmd, Vec::new());
        self.tidy();
    }

    pub fn reset(&mut self, cmd: Cmd) {
        self.overrides.remove(&cmd);
    }

    pub fn reset_all(&mut self) {
        self.overrides.clear();
    }

    pub fn is_custom(&self, cmd: Cmd) -> bool {
        self.overrides.contains_key(&cmd)
    }

    fn tidy(&mut self) {
        let defaults = crate::commands::default_shortcuts();
        self.overrides.retain(|c, list| {
            let d: Vec<Binding> = defaults
                .iter()
                .filter(|(dc, _)| dc == c)
                .map(|(_, s)| *s)
                .collect();
            *list != d
        });
    }

    pub fn to_json(&self, infos: &[Info]) -> String {
        let map: BTreeMap<String, Vec<String>> = self
            .overrides
            .iter()
            .filter_map(|(c, list)| {
                let id = infos.iter().find(|i| i.cmd == *c)?.id.clone();
                Some((id, list.iter().map(Binding::label).collect()))
            })
            .collect();
        serde_json::to_string_pretty(&map).unwrap_or_else(|_| "{}".to_owned())
    }

    /// Unknown commands and unreadable shortcuts are skipped, not fatal.
    pub fn from_json(text: &str, infos: &[Info]) -> Keymap {
        let map: BTreeMap<String, Vec<String>> = serde_json::from_str(text).unwrap_or_default();
        let mut km = Keymap::default();
        for (id, list) in map {
            if let Some(info) = infos.iter().find(|i| i.id == id) {
                km.overrides.insert(
                    info.cmd,
                    list.iter().filter_map(|s| Binding::parse(s)).collect(),
                );
            }
        }
        km.tidy();
        km
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{all, default_shortcuts};
    use forma3d_core::editor::MeshTool;
    use forma3d_core::gizmo::Tool;

    const NONE: Mods = Mods {
        ctrl: false,
        shift: false,
        alt: false,
    };
    const SHIFT: Mods = Mods {
        ctrl: false,
        shift: true,
        alt: false,
    };
    const CTRL: Mods = Mods {
        ctrl: true,
        shift: false,
        alt: false,
    };

    #[test]
    fn defaults_are_consistent() {
        let infos = all();
        let mut seen = HashMap::new();
        for (c, b) in default_shortcuts() {
            assert!(
                infos.iter().any(|i| i.cmd == c),
                "{c:?} has a shortcut but no entry"
            );
            if let Some(other) = seen.insert(b, c) {
                panic!("{} is bound to both {other:?} and {c:?}", b.label());
            }
            assert_eq!(Binding::parse(&b.label()), Some(b), "{}", b.label());
        }
        let d = default_shortcuts();
        for (_, b) in d.iter().filter(|(_, b)| b.then.is_some()) {
            assert!(
                !d.iter()
                    .any(|(_, s)| s.then.is_none() && s.first == b.first),
                "{} starts a sequence and is a shortcut",
                b.first.label()
            );
        }
    }

    #[test]
    fn lookups_sequences_and_rebinding() {
        let infos = all();
        let mut km = Keymap::default();
        assert_eq!(
            km.command_for(Key::of("B"), NONE),
            Some(Cmd::MeshTool(MeshTool::Bevel))
        );
        assert_eq!(km.command_for(Key::of("B"), SHIFT), Some(Cmd::Bridge));
        assert_eq!(km.command_for(Key::of("Z"), CTRL), Some(Cmd::Undo));
        let m = Shortcut::key("M");
        assert!(km.is_prefix(m));
        assert_eq!(
            km.command_for_seq(m, Key::of("S"), NONE),
            Some(Cmd::MeshTool(MeshTool::Bevel))
        );
        assert_eq!(
            km.assign(Cmd::Weld, Binding::single(Shortcut::key("B"))),
            Ok(Some(Cmd::MeshTool(MeshTool::Bevel)))
        );
        assert_eq!(km.command_for(Key::of("B"), NONE), Some(Cmd::Weld));
        assert!(matches!(
            km.assign(Cmd::Undo, Binding::single(Shortcut::key("M"))),
            Err(Conflict::StartsSequences(_))
        ));
        assert_eq!(
            km.assign(Cmd::Undo, Binding::seq("E", "X")),
            Err(Conflict::FirstKeyIsShortcut(Cmd::Tool(Tool::Move)))
        );
        let back = Keymap::from_json(&km.to_json(&infos), &infos);
        assert_eq!(back, km);
        km.reset_all();
        assert_eq!(km, Keymap::default());
        assert_eq!(Keymap::from_json("not json", &infos), Keymap::default());
    }
}
