//! Layers (Cinema 4D's Layer Manager): named, coloured groups of objects
//! with switches for showing them in the viewport, the final render and the
//! Object Manager, for locking them and for soloing them.

use serde::{Deserialize, Serialize};

pub type LayerId = u64;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub color: [f32; 3],
    /// While any layer is soloed, only soloed layers show in the viewport.
    pub solo: bool,
    /// Shown in the viewport.
    pub view: bool,
    /// Included in the final render.
    pub render: bool,
    /// Listed in the Object Manager.
    pub manager: bool,
    /// Can't be selected or changed in the viewport.
    pub locked: bool,
}

impl Default for Layer {
    fn default() -> Self {
        Layer { id: 0, name: "Layer".to_owned(), color: PALETTE[0], solo: false, view: true, render: true, manager: true, locked: false }
    }
}

/// Colours given to new layers in turn.
pub const PALETTE: [[f32; 3]; 8] = [
    [0.86, 0.36, 0.33],
    [0.35, 0.62, 0.90],
    [0.45, 0.76, 0.40],
    [0.93, 0.74, 0.28],
    [0.68, 0.46, 0.86],
    [0.30, 0.78, 0.76],
    [0.92, 0.52, 0.22],
    [0.84, 0.48, 0.66],
];

/// One of a layer's switches, as the Layer Manager's columns show them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerFlag {
    Solo,
    View,
    Render,
    Manager,
    Locked,
}

impl LayerFlag {
    pub const ALL: [LayerFlag; 5] = [LayerFlag::Solo, LayerFlag::View, LayerFlag::Render, LayerFlag::Manager, LayerFlag::Locked];

    pub fn label(self) -> &'static str {
        match self {
            LayerFlag::Solo => "Solo",
            LayerFlag::View => "View",
            LayerFlag::Render => "Render",
            LayerFlag::Manager => "Manager",
            LayerFlag::Locked => "Locked",
        }
    }

    /// One-letter column heading.
    pub fn letter(self) -> &'static str {
        match self {
            LayerFlag::Solo => "S",
            LayerFlag::View => "V",
            LayerFlag::Render => "R",
            LayerFlag::Manager => "M",
            LayerFlag::Locked => "L",
        }
    }

    pub fn tip(self) -> &'static str {
        match self {
            LayerFlag::Solo => "Solo: while any layer is soloed, only soloed layers show in the viewport",
            LayerFlag::View => "View: show this layer's objects in the viewport",
            LayerFlag::Render => "Render: include this layer's objects in the final render",
            LayerFlag::Manager => "Manager: list this layer's objects in the Object Manager",
            LayerFlag::Locked => "Locked: this layer's objects can't be selected or changed in the viewport",
        }
    }

    pub fn get(self, l: &Layer) -> bool {
        match self {
            LayerFlag::Solo => l.solo,
            LayerFlag::View => l.view,
            LayerFlag::Render => l.render,
            LayerFlag::Manager => l.manager,
            LayerFlag::Locked => l.locked,
        }
    }

    pub fn set(self, l: &mut Layer, on: bool) {
        match self {
            LayerFlag::Solo => l.solo = on,
            LayerFlag::View => l.view = on,
            LayerFlag::Render => l.render = on,
            LayerFlag::Manager => l.manager = on,
            LayerFlag::Locked => l.locked = on,
        }
    }
}
