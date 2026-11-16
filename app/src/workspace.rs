//! Workspaces: each arranges the panels for one kind of job (the tabs at the
//! top right). Panels sit in five tab groups around the viewport; every
//! workspace remembers how you rearrange it (closing a tab, moving a tab to
//! another group, opening one from Window > Panels), saved as
//! `workspaces.json`.

use crate::icons::icon_s;
use crate::state::app;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Workspace {
    Model,
    Track,
    UvEdit,
    Animate,
    Sculpt,
    Custom,
}

impl Workspace {
    pub const ALL: [Workspace; 6] = [
        Workspace::Model,
        Workspace::Track,
        Workspace::UvEdit,
        Workspace::Animate,
        Workspace::Sculpt,
        Workspace::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Workspace::Model => "Model",
            Workspace::Track => "Track",
            Workspace::UvEdit => "UV Edit",
            Workspace::Animate => "Animate",
            Workspace::Sculpt => "Sculpt",
            Workspace::Custom => "Custom",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Workspace::Model => "model",
            Workspace::Track => "track",
            Workspace::UvEdit => "uv_edit",
            Workspace::Animate => "animate",
            Workspace::Sculpt => "sculpt",
            Workspace::Custom => "custom",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Workspace::Model => {
                "Modelling: viewport, Object and Attribute Managers, materials and coordinates"
            }
            Workspace::Track => "Camera tracking: viewport with a Footage panel and the timeline",
            Workspace::UvEdit => "UV editing: the UV Editor beside the viewport",
            Workspace::Animate => "Animation: viewport, timeline, coordinates and attributes",
            Workspace::Sculpt => "Sculpting: a large viewport with the Brushes panel",
            Workspace::Custom => "Your own arrangement: move and close panels; it is remembered",
        }
    }

    pub fn keywords(self) -> &'static str {
        match self {
            Workspace::Model => "workspace layout modeling standard",
            Workspace::Track => "workspace layout motion camera tracking footage video",
            Workspace::UvEdit => "workspace layout uv unwrap texture",
            Workspace::Animate => "workspace layout animation timeline keyframe",
            Workspace::Sculpt => "workspace layout sculpting brush",
            Workspace::Custom => "workspace layout user own",
        }
    }

    /// The default arrangement.
    pub fn default_layout(self) -> Layout {
        use Panel::*;
        let g = |panels: &[Panel]| Group {
            panels: panels.to_vec(),
            active: 0,
        };
        let (rt, rb, bl, br, side, wide) = match self {
            Workspace::Model | Workspace::Custom => (
                g(&[Objects, Takes]),
                g(&[Attributes, Layers]),
                g(&[Materials, Assets, Structure, Console]),
                g(&[Coordinates]),
                g(&[]),
                false,
            ),
            Workspace::Track => (
                g(&[Objects]),
                g(&[Attributes]),
                g(&[Footage]),
                g(&[]),
                g(&[]),
                false,
            ),
            Workspace::UvEdit => (
                g(&[Objects]),
                g(&[Attributes]),
                g(&[]),
                g(&[]),
                g(&[UvEditor]),
                true,
            ),
            Workspace::Animate => (
                g(&[Objects]),
                g(&[Attributes]),
                g(&[Coordinates]),
                g(&[]),
                g(&[]),
                false,
            ),
            Workspace::Sculpt => (
                g(&[Attributes, Objects]),
                g(&[]),
                g(&[]),
                g(&[]),
                g(&[Brushes]),
                false,
            ),
        };
        Layout {
            groups: [rt, rb, bl, br, side],
            timeline: !matches!(self, Workspace::UvEdit | Workspace::Sculpt),
            side_wide: wide,
        }
    }
}

/// The dockable panels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Panel {
    Objects,
    Attributes,
    Materials,
    Coordinates,
    UvEditor,
    Footage,
    Brushes,
    Layers,
    Takes,
    Structure,
    Assets,
    Console,
    PictureViewer,
    RenderQueue,
    LightLister,
}

impl Panel {
    pub const OPENABLE: [Panel; 15] = [
        Panel::Objects,
        Panel::Attributes,
        Panel::Materials,
        Panel::Coordinates,
        Panel::Layers,
        Panel::Takes,
        Panel::Structure,
        Panel::Assets,
        Panel::Console,
        Panel::UvEditor,
        Panel::Footage,
        Panel::Brushes,
        Panel::PictureViewer,
        Panel::RenderQueue,
        Panel::LightLister,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Panel::Objects => "Objects",
            Panel::Attributes => "Attributes",
            Panel::Materials => "Materials",
            Panel::Coordinates => "Coordinates",
            Panel::UvEditor => "UV Editor",
            Panel::Footage => "Footage",
            Panel::Brushes => "Brushes",
            Panel::Layers => "Layers",
            Panel::Takes => "Takes",
            Panel::Structure => "Structure",
            Panel::Assets => "Asset Browser",
            Panel::Console => "Console",
            Panel::PictureViewer => "Picture Viewer",
            Panel::RenderQueue => "Render Queue",
            Panel::LightLister => "Light Lister",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Panel::Objects => "objects",
            Panel::Attributes => "attributes",
            Panel::Materials => "materials",
            Panel::Coordinates => "coordinates",
            Panel::UvEditor => "uv_editor",
            Panel::Footage => "footage",
            Panel::Brushes => "brushes",
            Panel::Layers => "layers",
            Panel::Takes => "takes",
            Panel::Structure => "structure",
            Panel::Assets => "assets",
            Panel::Console => "console",
            Panel::PictureViewer => "picture_viewer",
            Panel::RenderQueue => "render_queue",
            Panel::LightLister => "light_lister",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Panel::Objects => "tree",
            Panel::Attributes => "sliders",
            Panel::Materials => "material",
            Panel::Coordinates => "move",
            Panel::UvEditor => "uv",
            Panel::Footage => "film",
            Panel::Brushes => "brush",
            Panel::Layers => "layers",
            Panel::Takes => "takes",
            Panel::Structure => "structure",
            Panel::Assets => "assets",
            Panel::Console => "console",
            Panel::PictureViewer => "picture",
            Panel::RenderQueue => "render",
            Panel::LightLister => "light",
        }
    }

    pub fn keywords(self) -> &'static str {
        match self {
            Panel::Layers => "panel manager layer solo lock hide visibility",
            Panel::Takes => "panel manager take variation override auto take",
            Panel::Structure => "panel manager structure points polygons table coordinates vertex",
            Panel::Assets => "panel manager asset browser library content preset model material",
            Panel::Console => "panel manager console log messages script",
            Panel::PictureViewer => "panel picture viewer render history image compare save",
            Panel::RenderQueue => "panel render queue batch jobs",
            Panel::LightLister => "panel light lister lights table intensity shadows",
            _ => "panel manager window show open",
        }
    }
}

/// Where a tab group sits.
pub const SLOT_NAMES: [&str; 5] = [
    "Right, top",
    "Right, bottom",
    "Bottom, left",
    "Bottom, right",
    "Left of the viewport",
];

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub panels: Vec<Panel>,
    pub active: usize,
}

impl Group {
    pub fn current(&self) -> Option<Panel> {
        self.panels
            .get(self.active.min(self.panels.len().saturating_sub(1)))
            .copied()
    }
}

/// One workspace's arrangement: groups right-top, right-bottom, bottom-left,
/// bottom-right and beside the viewport.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub groups: [Group; 5],
    pub timeline: bool,
    /// The group beside the viewport takes half the width (the UV Editor).
    pub side_wide: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Workspace::Model.default_layout()
    }
}

impl Layout {
    pub fn find(&self, p: Panel) -> Option<(usize, usize)> {
        self.groups
            .iter()
            .enumerate()
            .find_map(|(g, grp)| grp.panels.iter().position(|x| *x == p).map(|i| (g, i)))
    }

    pub fn close(&mut self, p: Panel) {
        if let Some((g, i)) = self.find(p) {
            let grp = &mut self.groups[g];
            grp.panels.remove(i);
            if grp.active >= grp.panels.len() {
                grp.active = grp.panels.len().saturating_sub(1);
            }
        }
    }

    /// Brings a panel to the front, opening it if it was closed (next to the
    /// Attribute Manager, or wherever there is room).
    pub fn show(&mut self, p: Panel) {
        if let Some((g, i)) = self.find(p) {
            self.groups[g].active = i;
            return;
        }
        let host = [
            Panel::Attributes,
            Panel::Objects,
            Panel::Coordinates,
            Panel::Materials,
        ]
        .iter()
        .find_map(|h| self.find(*h).map(|(g, _)| g))
        .unwrap_or(1);
        let grp = &mut self.groups[host];
        grp.panels.push(p);
        grp.active = grp.panels.len() - 1;
    }

    pub fn move_to(&mut self, p: Panel, to: usize) {
        if self.find(p).is_some_and(|(g, _)| g == to) || to >= self.groups.len() {
            return;
        }
        self.close(p);
        let grp = &mut self.groups[to];
        grp.panels.push(p);
        grp.active = grp.panels.len() - 1;
    }

    /// Repairs a loaded layout: no duplicates, active tabs in range.
    pub fn sanitize(&mut self) {
        let mut seen = std::collections::HashSet::new();
        for g in &mut self.groups {
            g.panels.retain(|p| seen.insert(*p));
            g.active = g.active.min(g.panels.len().saturating_sub(1));
        }
    }
}

/// What `workspaces.json` holds.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Saved {
    pub active: Option<Workspace>,
    pub layouts: HashMap<Workspace, Layout>,
}

/// One tab group with its tabs. Right-click a tab to move it to another
/// group; × closes it (Window > Panels opens it again).
#[component]
pub fn TabGroup(group: usize, #[prop(optional)] extra: &'static str) -> impl IntoView {
    let app = app();
    let current = move || app.layout.with(|l| l.groups[group].clone());
    let menu_for = RwSignal::new(None::<(Panel, i32, i32)>);
    view! {
        <div class=format!("panel {extra}")>
            <div class="tabs">
                {move || {
                    let g = current();
                    let cur = g.current();
                    g.panels
                        .iter()
                        .enumerate()
                        .map(|(i, &p)| {
                            view! {
                                <div
                                    class="tab"
                                    class:on=Some(p) == cur
                                    title="Click to show · right-click to move · × to close"
                                    on:click=move |_| app.edit_layout(|l| l.groups[group].active = i)
                                    on:contextmenu=move |ev: web_sys::MouseEvent| {
                                        ev.prevent_default();
                                        ev.stop_propagation();
                                        menu_for.set(Some((p, ev.client_x(), ev.client_y())));
                                    }
                                >
                                    {icon_s(p.icon())}
                                    {p.title()}
                                    <span
                                        class="tabx"
                                        title="Close (Window > Panels opens it again)"
                                        on:click=move |ev| {
                                            ev.stop_propagation();
                                            app.edit_layout(|l| l.close(p));
                                        }
                                    >
                                        "×"
                                    </span>
                                </div>
                            }
                        })
                        .collect_view()
                }}
            </div>
            {move || match current().current() {
                Some(p) => panel_view(p),
                None => view! { <div class="content"></div> }.into_any(),
            }}
            {move || menu_for.get().map(|(p, x, y)| {
                view! {
                    <div class="scrim" on:mousedown=move |_| menu_for.set(None)></div>
                    <div class="dropdown fixed" style=format!("left:{x}px;top:{y}px") on:mousedown=|ev| ev.stop_propagation()>
                        <div class="mlabel">{format!("Move \"{}\" to", p.title())}</div>
                        {SLOT_NAMES
                            .iter()
                            .enumerate()
                            .map(|(to, name)| {
                                let here = to == group;
                                view! {
                                    <div class="mi" class:off=here on:click=move |_| {
                                        menu_for.set(None);
                                        app.edit_layout(|l| l.move_to(p, to));
                                    }>
                                        <span class="chk">{if here { "✓" } else { "" }}</span>
                                        <span class="ico"></span>
                                        {*name}
                                    </div>
                                }
                            })
                            .collect_view()}
                        <div class="msep"></div>
                        <div class="mi" on:click=move |_| {
                            menu_for.set(None);
                            app.edit_layout(|l| l.close(p));
                        }>
                            <span class="chk"></span>
                            <span class="ico"></span>
                            "Close Panel"
                        </div>
                    </div>
                }
            })}
        </div>
    }
}

/// The contents of a panel.
pub fn panel_view(p: Panel) -> AnyView {
    match p {
        Panel::Objects => view! { <crate::objects::ObjectManager /> }.into_any(),
        Panel::Attributes => view! { <crate::attributes::Attributes /> }.into_any(),
        Panel::Materials => view! { <crate::bottom::Materials /> }.into_any(),
        Panel::Assets => view! { <crate::bottom::Assets /> }.into_any(),
        Panel::Console => view! { <crate::bottom::Console /> }.into_any(),
        Panel::PictureViewer => view! { <crate::render::PictureViewer /> }.into_any(),
        Panel::RenderQueue => view! { <crate::render::RenderQueue /> }.into_any(),
        Panel::LightLister => view! { <crate::render::LightLister /> }.into_any(),
        Panel::Coordinates => view! { <crate::bottom::Coordinates /> }.into_any(),
        Panel::Layers => view! { <crate::managers::Layers /> }.into_any(),
        Panel::Takes => view! { <crate::managers::Takes /> }.into_any(),
        Panel::Structure => view! { <crate::managers::Structure /> }.into_any(),
        Panel::UvEditor => view! { <crate::managers::UvEditor /> }.into_any(),
        Panel::Footage => view! { <crate::managers::Footage /> }.into_any(),
        Panel::Brushes => view! { <crate::managers::Brushes /> }.into_any(),
    }
}

/// The workspace tabs at the top right of the title bar.
#[component]
pub fn WorkspaceTabs() -> impl IntoView {
    let app = app();
    view! {
        <div class="ws">
            {Workspace::ALL
                .map(|w| {
                    let key = move || app.keymap.with(|k| k.label(crate::commands::Cmd::Workspace(w)));
                    view! {
                        <span
                            class:on=move || app.workspace.get() == w
                            title=move || {
                                let k = key();
                                if k.is_empty() { w.description().to_owned() } else { format!("{} ({k})", w.description()) }
                            }
                            on:click=move |_| app.switch_workspace(w)
                        >
                            {w.label()}
                        </span>
                    }
                })
                .collect_view()}
        </div>
    }
}

/// Everything between the toolbar and the status bar, arranged by the
/// current workspace.
#[component]
pub fn Body() -> impl IntoView {
    let app = app();
    let has = move |g: usize| app.layout.with(|l| !l.groups[g].panels.is_empty());
    let side_class = move || {
        if app.layout.with(|l| l.side_wide) {
            "side wide"
        } else {
            "side"
        }
    };
    view! {
        <div class="body">
            <crate::chrome::Palette />
            <Show when=move || has(4)>
                <div class=side_class><TabGroup group=4 extra="fill" /></div>
            </Show>
            <div class="center">
                <crate::viewport::Viewport />
                <Show when=move || has(2) || has(3)>
                    <div class="bottom">
                        <Show when=move || has(2)><TabGroup group=2 extra="fill" /></Show>
                        <Show when=move || has(3)><TabGroup group=3 extra="fill narrow" /></Show>
                    </div>
                </Show>
            </div>
            <Show when=move || has(0) || has(1)>
                <div class="right">
                    <Show when=move || has(0)><TabGroup group=0 extra="fill" /></Show>
                    <Show when=move || has(1)><TabGroup group=1 extra="fill tall" /></Show>
                </div>
            </Show>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_workspace_has_attributes_and_its_own_panels() {
        for w in Workspace::ALL {
            let l = w.default_layout();
            assert!(l.find(Panel::Attributes).is_some(), "{w:?}");
        }
        assert!(
            Workspace::Track
                .default_layout()
                .find(Panel::Footage)
                .is_some()
        );
        assert!(
            Workspace::UvEdit
                .default_layout()
                .find(Panel::UvEditor)
                .is_some()
        );
        assert!(
            Workspace::Sculpt
                .default_layout()
                .find(Panel::Brushes)
                .is_some()
        );
    }

    #[test]
    fn show_close_move_and_round_trip() {
        let mut l = Workspace::Track.default_layout();
        l.show(Panel::Layers);
        assert_eq!(
            l.find(Panel::Layers).map(|x| x.0),
            l.find(Panel::Attributes).map(|x| x.0)
        );
        l.move_to(Panel::Layers, 2);
        assert_eq!(l.find(Panel::Layers).map(|x| x.0), Some(2));
        l.close(Panel::Layers);
        assert!(l.find(Panel::Layers).is_none());
        let saved = Saved {
            active: Some(Workspace::Track),
            layouts: HashMap::from([(Workspace::Track, l.clone())]),
        };
        let back: Saved = serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        assert_eq!(back.layouts[&Workspace::Track], l);
        let mut dup = l.clone();
        dup.groups[0].panels.push(Panel::Attributes);
        dup.sanitize();
        assert_eq!(
            dup.groups
                .iter()
                .flat_map(|g| &g.panels)
                .filter(|p| **p == Panel::Attributes)
                .count(),
            1
        );
    }
}
