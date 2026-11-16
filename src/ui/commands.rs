//! The command registry: every menu entry, palette button and keyboard
//! shortcut is a `Cmd`. Menus, palettes, the Commander search box and the
//! keyboard all go through this one table, so a command has one name, one
//! icon and one set of shortcuts everywhere, and shortcuts can be rebound.

use crate::camera::ViewKind;
use crate::component::EditMode;
use crate::deformer::DeformerKind;
use crate::editor::{Editor, MeshTool, ViewLayout};
use crate::generator::GeneratorKind;
use crate::gizmo::{CoordSystem, Tool};
use crate::icons::{self, Icon};
use crate::primitive::PrimitiveKind;
use crate::light::LightKind;
use crate::pen::PenKind;
use crate::renderer::{DisplayMode, ViewFx};
use crate::scene::ObjectKind;
use crate::snap::{SnapKind, SnapMode};
use eframe::egui::{Key, Modifiers};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

/// Workspaces: each arranges the panels for one kind of job. They are the
/// tabs of the Workspace bar at the top right. Every workspace remembers how
/// you rearrange it; Custom starts with every panel for you to arrange.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Workspace {
    Model,
    Track,
    UvEdit,
    Animate,
    Sculpt,
    Custom,
}

impl Workspace {
    pub const ALL: [Workspace; 6] =
        [Workspace::Model, Workspace::Track, Workspace::UvEdit, Workspace::Animate, Workspace::Sculpt, Workspace::Custom];

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
            Workspace::Model => "Modelling: viewport, Object and Attribute Managers, materials and coordinates",
            Workspace::Track => "Camera tracking: viewport with a Footage panel and the timeline",
            Workspace::UvEdit => "UV editing: the UV Editor beside the viewport",
            Workspace::Animate => "Animation: viewport, timeline, coordinates and attributes",
            Workspace::Sculpt => "Sculpting: a large viewport with the Brushes panel",
            Workspace::Custom => "Your own arrangement: drag panels anywhere; it is remembered",
        }
    }

    fn keywords(self) -> &'static str {
        match self {
            Workspace::Model => "workspace layout modeling standard",
            Workspace::Track => "workspace layout motion camera tracking footage video",
            Workspace::UvEdit => "workspace layout uv unwrap texture",
            Workspace::Animate => "workspace layout animation timeline keyframe",
            Workspace::Sculpt => "workspace layout sculpting brush",
            Workspace::Custom => "workspace layout user own",
        }
    }

    /// Whether the timeline shows in this workspace until you change it.
    pub fn shows_timeline(self) -> bool {
        !matches!(self, Workspace::UvEdit | Workspace::Sculpt)
    }
}

/// The dockable panels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Panel {
    Viewport,
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
}

impl Panel {
    /// Panels that can be opened from Window > Panels (the viewport is always there).
    pub const OPENABLE: [Panel; 12] = [
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
    ];

    pub fn icon(self) -> Option<Icon> {
        match self {
            Panel::Layers => Some(Icon::Layers),
            Panel::Takes => Some(Icon::Takes),
            Panel::Structure => Some(Icon::Structure),
            Panel::Assets => Some(Icon::Assets),
            Panel::Console => Some(Icon::Console),
            _ => None,
        }
    }

    fn keywords(self) -> &'static str {
        match self {
            Panel::Layers => "panel manager layer solo lock hide visibility",
            Panel::Takes => "panel manager take variation override auto take",
            Panel::Structure => "panel manager structure points polygons table coordinates vertex",
            Panel::Assets => "panel manager asset browser library content preset model material",
            Panel::Console => "panel manager console log messages script",
            _ => "panel manager window show open",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Panel::Viewport => "Viewport",
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
        }
    }

    fn id(self) -> &'static str {
        match self {
            Panel::Viewport => "viewport",
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
        }
    }
}

/// Object types the viewport Filter menu can hide.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ViewFilter {
    Primitives,
    Polygons,
    Generators,
    Deformers,
    Nulls,
    Splines,
    Lights,
}

impl ViewFilter {
    pub const ALL: [ViewFilter; 7] =
        [ViewFilter::Primitives, ViewFilter::Polygons, ViewFilter::Splines, ViewFilter::Generators, ViewFilter::Deformers, ViewFilter::Lights, ViewFilter::Nulls];

    pub fn label(self) -> &'static str {
        match self {
            ViewFilter::Primitives => "Primitives",
            ViewFilter::Polygons => "Polygon Objects",
            ViewFilter::Generators => "Generators",
            ViewFilter::Deformers => "Deformers",
            ViewFilter::Nulls => "Nulls",
            ViewFilter::Splines => "Splines",
            ViewFilter::Lights => "Lights",
        }
    }

    fn flag(self, o: &mut crate::editor::ViewOptions) -> &mut bool {
        match self {
            ViewFilter::Primitives => &mut o.primitives,
            ViewFilter::Polygons => &mut o.polygons,
            ViewFilter::Generators => &mut o.generators,
            ViewFilter::Deformers => &mut o.deformers,
            ViewFilter::Nulls => &mut o.nulls,
            ViewFilter::Splines => &mut o.splines,
            ViewFilter::Lights => &mut o.lights,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cmd {
    // File
    New,
    Open,
    Close,
    Revert,
    Save,
    SaveAs,
    Merge,
    Quit,
    // Edit
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Preferences,
    Duplicate,
    Group,
    Delete,
    Cancel,
    // Create
    Primitive(PrimitiveKind),
    Null,
    Generator(GeneratorKind),
    Deformer(DeformerKind),
    NewMaterial,
    // Modes
    Mode(EditMode),
    MakeEditable,
    ToggleAxis(u8),
    ToggleCoordSystem,
    /// Enable Axis: the tools move the axis, not the geometry.
    ToggleAxisMode,
    CenterAxis,
    ToggleTweak,
    ToggleSnap,
    SnapMode(SnapMode),
    SnapTo(SnapKind),
    ToggleQuantize,
    /// Align the workplane to world X (0), Y (1) or Z (2).
    AlignWorkplane(u8),
    AlignWorkplaneToSelection,
    AlignSelectionToWorkplane,
    /// Put the selection on a new layer.
    NewLayerFromSelection,
    NewTake,
    SaveAsset,
    // Select
    SelectAll,
    DeselectAll,
    InvertSelection,
    SelectChildren,
    LoopSelect,
    // Objects (Object Manager)
    Rename,
    ToggleEnabled,
    ExpandAll,
    CollapseAll,
    ResetTransform,
    /// Put this material on the selected objects (one command per material).
    AssignMaterial(u64),
    RemoveMaterial,
    // Materials (Material Manager)
    ApplyMaterial,
    DeleteMaterial,
    OpenMaterialEditor,
    // Tools
    Tool(Tool),
    MeshTool(MeshTool),
    // Mesh
    Bridge,
    Weld,
    Optimize,
    SubdivideSmooth,
    SubdivideLinear,
    ReverseNormals,
    // View
    FrameSelected,
    FrameAll,
    Camera(ViewKind),
    FourViews,
    FrameDefault,
    Filter(ViewFilter),
    Display(DisplayMode),
    ToggleGrid,
    ToggleCompass,
    ToggleSolo,
    /// A viewport render option (Options menu).
    ViewFx(ViewFx),
    /// A light (the Light menu).
    Light(LightKind),
    /// The Spline Pen or Polygon Pen.
    Pen(PenKind),
    ToggleIconBar,
    IconLibrary,
    // Animation
    PlayPause,
    PlayBackward,
    NextFrame,
    PrevFrame,
    ToStart,
    ToEnd,
    // Window / Help
    Workspace(Workspace),
    NextWorkspace,
    PrevWorkspace,
    ResetLayout,
    ShowPanel(Panel),
    ToggleTimeline,
    Commander,
    CustomizeCommands,
    About,
}

/// Commands the editor can't carry out on its own (files, windows, layout).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppAction {
    New,
    Open,
    Close,
    Revert,
    Save,
    SaveAs,
    Merge,
    Quit,
    Cut,
    Copy,
    Paste,
    Preferences,
    Workspace(Workspace),
    CycleWorkspace(i32),
    ResetLayout,
    ShowPanel(Panel),
    Commander,
    CustomizeCommands,
    IconLibrary,
    About,
    OpenMaterialEditor(u64),
    SaveAsset,
}

/// Name, icon, menu category and search words of a command.
#[derive(Clone, Debug)]
pub struct Info {
    pub cmd: Cmd,
    pub id: String,
    pub name: String,
    pub category: &'static str,
    pub icon: Option<Icon>,
    pub keywords: &'static str,
}

/// Every command, in menu order.
pub fn all() -> Vec<Info> {
    let mut v = Vec::new();
    let mut add = |cmd: Cmd, id: &str, name: &str, category: &'static str, icon: Option<Icon>, keywords: &'static str| {
        v.push(Info { cmd, id: id.to_owned(), name: name.to_owned(), category, icon, keywords });
    };
    add(Cmd::New, "file.new", "New Project", "File", None, "scene clear document");
    add(Cmd::Open, "file.open", "Open...", "File", None, "load scene project");
    add(Cmd::Close, "file.close", "Close", "File", None, "close project document");
    add(Cmd::Revert, "file.revert", "Revert to Saved", "File", None, "reload discard changes");
    add(Cmd::Save, "file.save", "Save", "File", None, "");
    add(Cmd::SaveAs, "file.save_as", "Save As...", "File", None, "");
    add(Cmd::Merge, "file.merge", "Merge Project...", "File", None, "import add scene objects");
    add(Cmd::Cut, "edit.cut", "Cut", "Edit", None, "clipboard");
    add(Cmd::Copy, "edit.copy", "Copy", "Edit", None, "clipboard");
    add(Cmd::Paste, "edit.paste", "Paste", "Edit", None, "clipboard");
    add(Cmd::Preferences, "edit.preferences", "Preferences...", "Edit", None, "settings options autosave undo scale");
    add(Cmd::Quit, "file.quit", "Quit", "File", None, "exit close");
    add(Cmd::Undo, "edit.undo", "Undo", "Edit", Some(Icon::Undo), "");
    add(Cmd::Redo, "edit.redo", "Redo", "Edit", Some(Icon::Redo), "");
    add(Cmd::Duplicate, "edit.duplicate", "Duplicate", "Edit", Some(Icon::Duplicate), "copy clone");
    add(Cmd::Group, "edit.group", "Group Objects", "Edit", Some(Icon::Group), "null parent");
    add(Cmd::Delete, "edit.delete", "Delete", "Edit", Some(Icon::Delete), "remove dissolve");
    add(Cmd::Cancel, "edit.cancel", "Cancel / Deselect", "Edit", None, "escape");
    for k in PrimitiveKind::ALL {
        let keywords = match k {
            PrimitiveKind::Cube => "primitive shape square box block",
            PrimitiveKind::Sphere => "primitive shape ball globe",
            PrimitiveKind::Cylinder => "primitive shape column can",
            PrimitiveKind::Cone => "primitive shape point",
            PrimitiveKind::Pyramid => "primitive shape triangle point",
            PrimitiveKind::Tube => "primitive shape pipe hollow ring",
            PrimitiveKind::Capsule => "primitive shape pill rounded",
            PrimitiveKind::Landscape => "primitive shape terrain mountain hills fractal ground",
            PrimitiveKind::Figure => "primitive shape human person dummy character body",
            PrimitiveKind::Torus => "primitive shape ring donut",
            PrimitiveKind::Plane => "primitive shape floor ground flat",
        };
        add(Cmd::Primitive(k), &format!("create.{}", k.name().to_lowercase()), k.name(), "Create", Some(icons::for_primitive(k)), keywords);
    }
    add(Cmd::Null, "create.null", "Null", "Create", Some(Icon::Null), "empty group");
    for k in LightKind::ALL {
        add(Cmd::Light(k), &format!("light.{}", k.id()), k.name(), "Light", Some(icons::for_light(k)), k.keywords());
    }
    add(Cmd::Pen(PenKind::Spline), "create.spline_pen", "Spline Pen", "Create", Some(Icon::SplinePen), "spline curve draw bezier path line");
    add(Cmd::Pen(PenKind::Polygon), "mesh.polygon_pen", "Polygon Pen", "Mesh", Some(Icon::PolygonPen), "polygon draw retopology create face quad");
    for k in GeneratorKind::ALL {
        let icon = icons::for_kind(&ObjectKind::Generator(k.default_generator()));
        let id = format!("create.{}", k.name().to_lowercase().replace(' ', "_"));
        let keywords = match k {
            GeneratorKind::Subdivision => "generator sds smooth subd hypernurbs",
            GeneratorKind::Array => "generator repeat copies clone",
            GeneratorKind::Symmetry => "generator mirror",
            GeneratorKind::Boolean => "generator boole csg subtract union intersect",
        };
        add(Cmd::Generator(k), &id, k.name(), "Create", Some(icon), keywords);
    }
    for k in DeformerKind::ALL {
        let icon = icons::for_kind(&ObjectKind::Deformer(k.default_deformer()));
        let keywords = match k {
            DeformerKind::Bend => "deformer curve",
            DeformerKind::Twist => "deformer rotate spiral",
            DeformerKind::Taper => "deformer narrow scale cone pinch",
            DeformerKind::Shear => "deformer lean slant skew offset",
        };
        add(Cmd::Deformer(k), &format!("create.{}", k.name().to_lowercase()), k.name(), "Create", Some(icon), keywords);
    }
    add(Cmd::NewMaterial, "create.material", "New Material", "Create", Some(Icon::Material), "colour color");
    for (m, icon, keywords) in [
        (EditMode::Model, Icon::Model, "object resize geometry"),
        (EditMode::Object, Icon::ObjectMode, "object scale axes children"),
        (EditMode::Points, Icon::Points, "component vertex"),
        (EditMode::Edges, Icon::Edges, "component"),
        (EditMode::Polygons, Icon::Polygons, "component face"),
        (EditMode::Workplane, Icon::Workplane, "construction plane grid"),
    ] {
        add(Cmd::Mode(m), &format!("mode.{}", m.label().to_lowercase()), &format!("{} Mode", m.label()), "Modes", Some(icon), keywords);
    }
    add(Cmd::ToggleAxisMode, "mode.enable_axis", "Enable Axis", "Modes", Some(Icon::AxisMode), "axis pivot center move origin");
    add(Cmd::CenterAxis, "mode.center_axis", "Center Axis to Geometry", "Modes", Some(Icon::AxisMode), "axis pivot origin centre");
    add(Cmd::ToggleTweak, "mode.tweak", "Tweak Mode", "Modes", Some(Icon::Tweak), "drag direct move under cursor");
    add(Cmd::ToggleSnap, "snap.enable", "Enable Snap", "Snap", Some(Icon::Snap), "snapping magnet");
    for m in SnapMode::ALL {
        let id = match m {
            SnapMode::Snap3D => "snap.mode_3d",
            SnapMode::Snap25D => "snap.mode_25d",
            SnapMode::Snap2D => "snap.mode_2d",
        };
        add(Cmd::SnapMode(m), id, m.label(), "Snap", Some(Icon::Snap), "snapping");
    }
    for k in SnapKind::ALL {
        let id = format!("snap.to_{}", k.label().to_lowercase().replace(' ', "_"));
        add(Cmd::SnapTo(k), &id, &format!("Snap to {}", k.label()), "Snap", Some(Icon::Snap), "snapping target");
    }
    add(Cmd::ToggleQuantize, "snap.quantize", "Enable Quantizing", "Snap", Some(Icon::Quantize), "step increment grid round");
    for (i, a) in ["X", "Y", "Z"].into_iter().enumerate() {
        add(Cmd::AlignWorkplane(i as u8), &format!("workplane.align_{}", a.to_lowercase()), &format!("Align Workplane to {a}"), "Workplane", Some(Icon::Workplane), "construction plane");
    }
    add(Cmd::AlignWorkplaneToSelection, "workplane.align_selection", "Align Workplane to Selection", "Workplane", Some(Icon::Workplane), "construction plane object");
    add(Cmd::AlignSelectionToWorkplane, "workplane.align_objects", "Align Selection to Workplane", "Workplane", Some(Icon::Workplane), "construction plane object move");
    add(Cmd::NewLayerFromSelection, "layer.new_from_selection", "Add to New Layer", "Layer", Some(Icon::Layers), "layer manager selection");
    add(Cmd::NewTake, "take.new", "New Take", "Take", Some(Icon::Takes), "take variation override");
    add(Cmd::SaveAsset, "asset.save_selection", "Save Selection to Asset Browser", "Asset", Some(Icon::Assets), "asset library preset save");
    add(Cmd::MakeEditable, "mode.make_editable", "Make Editable", "Modes", Some(Icon::MakeEditable), "convert polygon");
    for (i, a) in ["X", "Y", "Z"].into_iter().enumerate() {
        add(Cmd::ToggleAxis(i as u8), &format!("mode.axis_{}", a.to_lowercase()), &format!("{a}-Axis Lock"), "Modes", None, "axis lock");
    }
    add(Cmd::ToggleCoordSystem, "mode.coord_system", "World / Object Coordinate System", "Modes", Some(Icon::WorldCoords), "axes");
    add(Cmd::SelectAll, "select.all", "Select All", "Select", None, "");
    add(Cmd::DeselectAll, "select.none", "Deselect All", "Select", None, "clear");
    add(Cmd::InvertSelection, "select.invert", "Invert Selection", "Select", None, "");
    add(Cmd::SelectChildren, "select.children", "Select Children", "Select", None, "hierarchy");
    add(Cmd::LoopSelect, "select.loop", "Loop Selection", "Select", None, "edge ring");
    add(Cmd::Rename, "object.rename", "Rename", "Object", None, "name");
    add(Cmd::ToggleEnabled, "object.toggle_enabled", "Enable / Disable", "Object", None, "switch on off generator deformer check");
    add(Cmd::ExpandAll, "object.expand_all", "Expand All", "Object", None, "hierarchy tree");
    add(Cmd::CollapseAll, "object.collapse_all", "Collapse All", "Object", None, "hierarchy tree fold");
    add(Cmd::ResetTransform, "object.reset_psr", "Reset PSR (Reset Transform)", "Object", None, "position scale rotation zero");
    add(Cmd::RemoveMaterial, "tags.remove_material", "Remove Material Tag", "Tags", None, "material");
    add(Cmd::ApplyMaterial, "material.apply", "Apply Material to Selected Objects", "Material", Some(Icon::Material), "assign tag");
    add(Cmd::DeleteMaterial, "material.delete", "Delete Material", "Material", Some(Icon::Delete), "remove");
    add(Cmd::OpenMaterialEditor, "material.editor", "Open Material Editor", "Material", Some(Icon::Material), "edit colour color");
    for (t, icon, name) in [
        (Tool::Select, Icon::Select, "Select"),
        (Tool::Move, Icon::Move, "Move"),
        (Tool::Scale, Icon::Scale, "Scale"),
        (Tool::Rotate, Icon::Rotate, "Rotate"),
    ] {
        add(Cmd::Tool(t), &format!("tool.{}", name.to_lowercase()), name, "Tools", Some(icon), "gizmo transform");
    }
    for (t, icon) in [
        (MeshTool::Extrude, Icon::Extrude),
        (MeshTool::Inset, Icon::Inset),
        (MeshTool::LoopCut, Icon::LoopCut),
        (MeshTool::Bevel, Icon::Bevel),
        (MeshTool::Knife, Icon::Knife),
    ] {
        add(Cmd::MeshTool(t), &format!("tool.{}", t.label().to_lowercase().replace(' ', "_")), t.label(), "Tools", Some(icon), "mesh modeling");
    }
    add(Cmd::Bridge, "mesh.bridge", "Bridge", "Mesh", None, "connect tunnel");
    add(Cmd::Weld, "mesh.weld", "Weld", "Mesh", None, "merge collapse");
    add(Cmd::Optimize, "mesh.optimize", "Optimize (merge close points)", "Mesh", None, "merge duplicate");
    add(Cmd::SubdivideSmooth, "mesh.subdivide_smooth", "Subdivide (smooth, whole object)", "Mesh", None, "catmull clark");
    add(Cmd::SubdivideLinear, "mesh.subdivide_linear", "Subdivide (linear, selected polygons)", "Mesh", None, "split");
    add(Cmd::ReverseNormals, "mesh.reverse_normals", "Reverse Normals", "Mesh", None, "flip");
    add(Cmd::FrameSelected, "view.frame_selected", "Frame Selected", "View", Some(Icon::Frame), "focus zoom");
    add(Cmd::FrameAll, "view.frame_all", "Frame All", "View", Some(Icon::FrameAll), "focus zoom scene");
    for k in ViewKind::ALL {
        add(Cmd::Camera(k), &format!("view.camera_{}", k.label().to_lowercase()), &format!("{} View", k.label()), "View", Some(Icon::SingleView), "camera");
    }
    add(Cmd::FourViews, "view.four_views", "4 Views", "View", Some(Icon::QuadView), "quad panel");
    add(Cmd::FrameDefault, "view.frame_default", "Frame Default (reset view)", "View", Some(Icon::Frame), "camera home");
    for f in ViewFilter::ALL {
        let id = format!("view.filter_{}", f.label().to_lowercase().replace(' ', "_"));
        add(Cmd::Filter(f), &id, &format!("Filter: Show {}", f.label()), "View", None, "hide visibility");
    }
    for m in DisplayMode::ALL {
        let id = format!("view.display_{}", m.id());
        let icon = match m {
            DisplayMode::Gouraud => Icon::Gouraud,
            DisplayMode::GouraudLines => Icon::GouraudLines,
            DisplayMode::Wireframe => Icon::Wireframe,
            DisplayMode::Quick => Icon::Quick,
            DisplayMode::QuickLines => Icon::QuickLines,
        };
        add(Cmd::Display(m), &id, m.label(), "View", Some(icon), "shading display wireframe lines");
    }
    add(Cmd::ToggleGrid, "view.grid", "Show Grid", "View", Some(Icon::Grid), "floor");
    add(Cmd::ToggleCompass, "view.compass", "Show Axis Compass", "View", Some(Icon::Compass), "");
    add(Cmd::ToggleSolo, "view.solo", "Viewport Solo", "View", Some(Icon::Solo), "isolate");
    for fx in ViewFx::ALL {
        add(Cmd::ViewFx(fx), &format!("view.{}", fx.id()), fx.label(), "View", Some(icons::for_view_fx(fx)), fx.keywords());
    }
    add(Cmd::PlayPause, "anim.play", "Play / Pause", "Animation", Some(Icon::Play), "timeline");
    add(Cmd::PlayBackward, "anim.play_backward", "Play Backwards", "Animation", Some(Icon::PlayBackward), "reverse");
    add(Cmd::NextFrame, "anim.next_frame", "Next Frame", "Animation", Some(Icon::NextFrame), "");
    add(Cmd::PrevFrame, "anim.prev_frame", "Previous Frame", "Animation", Some(Icon::PrevFrame), "");
    add(Cmd::ToStart, "anim.to_start", "Go to Start", "Animation", Some(Icon::ToStart), "");
    add(Cmd::ToEnd, "anim.to_end", "Go to End", "Animation", Some(Icon::ToEnd), "");
    for w in Workspace::ALL {
        add(Cmd::Workspace(w), &format!("window.workspace_{}", w.id()), &format!("Workspace: {}", w.label()), "Window", Some(Icon::Layout), w.keywords());
    }
    add(Cmd::NextWorkspace, "window.next_workspace", "Next Workspace", "Window", Some(Icon::Layout), "workspace tab");
    add(Cmd::PrevWorkspace, "window.prev_workspace", "Previous Workspace", "Window", Some(Icon::Layout), "workspace tab");
    add(Cmd::ResetLayout, "window.reset_workspace", "Reset Workspace", "Window", Some(Icon::Layout), "layout default panels");
    for p in Panel::OPENABLE {
        add(Cmd::ShowPanel(p), &format!("window.panel_{}", p.id()), &format!("Panel: {}", p.title()), "Window", p.icon(), p.keywords());
    }
    add(Cmd::ToggleIconBar, "window.icon_bar", "Viewport Icon Bar", "Window", Some(Icon::Shelf), "toolbar shelf buttons");
    add(Cmd::IconLibrary, "window.icon_library", "Icon Library (customize icon bar)...", "Window", Some(Icon::Shelf), "toolbar shelf buttons customize drag");
    add(Cmd::ToggleTimeline, "window.timeline", "Timeline", "Window", Some(Icon::Timeline), "animation");
    add(Cmd::Commander, "window.commander", "Commander", "Window", Some(Icon::Commander), "search find command");
    add(Cmd::CustomizeCommands, "window.customize_commands", "Customize Commands...", "Window", None, "shortcuts keys keyboard hotkeys");
    add(Cmd::About, "help.about", "About Forma3D", "Help", None, "version");
    v
}

/// Commands that depend on the scene: one "Material Tag" per material.
/// They appear in the Commander but can't be given shortcuts.
pub fn dynamic(ed: &Editor) -> Vec<Info> {
    ed.scene
        .materials()
        .iter()
        .map(|m| Info {
            cmd: Cmd::AssignMaterial(m.id),
            id: format!("tags.material.{}", m.id),
            name: format!("Material Tag: {}", m.name),
            category: "Tags",
            icon: Some(Icon::Material),
            keywords: "assign apply material tag",
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Shortcuts
// ---------------------------------------------------------------------------

/// One key with exact modifiers ("Ctrl" means Cmd on macOS).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: Key,
}

impl Shortcut {
    pub const fn key(key: Key) -> Self {
        Shortcut { ctrl: false, shift: false, alt: false, key }
    }
    pub const fn ctrl(key: Key) -> Self {
        Shortcut { ctrl: true, shift: false, alt: false, key }
    }
    pub const fn shift(key: Key) -> Self {
        Shortcut { ctrl: false, shift: true, alt: false, key }
    }
    pub const fn alt(key: Key) -> Self {
        Shortcut { ctrl: false, shift: false, alt: true, key }
    }
    pub const fn ctrl_shift(key: Key) -> Self {
        Shortcut { ctrl: true, shift: true, alt: false, key }
    }

    /// Exact match: Shift+B does not trigger B and vice versa.
    pub fn matches(&self, key: Key, m: Modifiers) -> bool {
        self.key == key && self.ctrl == m.command && self.shift == m.shift && self.alt == m.alt
    }

    pub fn from_event(key: Key, m: Modifiers) -> Self {
        Shortcut { ctrl: m.command, shift: m.shift, alt: m.alt, key }
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
        s.push_str(key_name(self.key));
        s
    }

    pub fn parse(text: &str) -> Option<Shortcut> {
        let mut sc = Shortcut::key(Key::A);
        let parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let (key, mods) = parts.split_last()?;
        for m in mods {
            match m.to_ascii_lowercase().as_str() {
                "ctrl" | "cmd" | "command" => sc.ctrl = true,
                "shift" => sc.shift = true,
                "alt" | "option" => sc.alt = true,
                _ => return None,
            }
        }
        sc.key = parse_key(key)?;
        Some(sc)
    }
}

fn key_name(k: Key) -> &'static str {
    match k {
        Key::Num0 => "0",
        Key::Num1 => "1",
        Key::Num2 => "2",
        Key::Num3 => "3",
        Key::Num4 => "4",
        Key::Num5 => "5",
        Key::Num6 => "6",
        Key::Num7 => "7",
        Key::Num8 => "8",
        Key::Num9 => "9",
        Key::Delete => "Del",
        Key::Escape => "Esc",
        Key::Backspace => "Backspace",
        other => other.name(),
    }
}

fn parse_key(s: &str) -> Option<Key> {
    match s {
        "Del" => Some(Key::Delete),
        "Esc" => Some(Key::Escape),
        _ if s.len() == 1 && s.chars().all(|c| c.is_ascii_digit()) => Key::from_name(&format!("Num{s}")).or_else(|| Key::from_name(s)),
        _ => Key::from_name(s),
    }
}

/// Keys that would break text entry are not bindable.
pub fn bindable(key: Key) -> bool {
    !matches!(key, Key::Enter | Key::Tab)
}

/// A shortcut: one key, or a Cinema 4D-style two-key sequence such as M~S
/// (press M, release, then press S).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Binding {
    pub first: Shortcut,
    pub then: Option<Shortcut>,
}

impl Binding {
    pub const fn single(s: Shortcut) -> Self {
        Binding { first: s, then: None }
    }

    /// Two plain keys, e.g. `seq(Key::M, Key::S)` for M~S.
    pub const fn seq(a: Key, b: Key) -> Self {
        Binding { first: Shortcut::key(a), then: Some(Shortcut::key(b)) }
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
            Some((a, b)) => Some(Binding { first: Shortcut::parse(a)?, then: Some(Shortcut::parse(b)?) }),
        }
    }
}

/// Why a shortcut can't be given to a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conflict {
    /// The key is the first key of these commands' sequences (e.g. M of M~S).
    StartsSequences(Vec<Cmd>),
    /// The sequence's first key is already a shortcut of this command on its own.
    FirstKeyIsShortcut(Cmd),
}

/// Built-in shortcuts. Several shortcuts can run the same command. The
/// two-key sequences follow Cinema 4D's (M~ mesh tools, K~ knives, U~ commands).
pub fn default_shortcuts() -> Vec<(Cmd, Binding)> {
    use Shortcut as S;
    let one = Binding::single;
    let mut v = vec![
        (Cmd::New, one(S::ctrl(Key::N))),
        // N~ shading modes, as in Cinema 4D.
        (Cmd::Display(DisplayMode::Gouraud), Binding::seq(Key::N, Key::A)),
        (Cmd::Display(DisplayMode::GouraudLines), Binding::seq(Key::N, Key::B)),
        (Cmd::Display(DisplayMode::Quick), Binding::seq(Key::N, Key::C)),
        (Cmd::Display(DisplayMode::QuickLines), Binding::seq(Key::N, Key::D)),
        (Cmd::Display(DisplayMode::Wireframe), Binding::seq(Key::N, Key::H)),
        (Cmd::NextWorkspace, one(S::ctrl(Key::PageDown))),
        (Cmd::PrevWorkspace, one(S::ctrl(Key::PageUp))),
        (Cmd::Open, one(S::ctrl(Key::O))),
        (Cmd::Save, one(S::ctrl(Key::S))),
        (Cmd::Undo, one(S::ctrl(Key::Z))),
        (Cmd::Redo, one(S::ctrl_shift(Key::Z))),
        (Cmd::SaveAs, one(S::ctrl_shift(Key::S))),
        (Cmd::Merge, one(S::ctrl_shift(Key::O))),
        (Cmd::Close, one(S::ctrl(Key::F4))),
        (Cmd::Close, one(S::ctrl(Key::W))),
        (Cmd::Cut, one(S::ctrl(Key::X))),
        (Cmd::Copy, one(S::ctrl(Key::C))),
        (Cmd::Paste, one(S::ctrl(Key::V))),
        (Cmd::Preferences, one(S::ctrl(Key::E))),
        (Cmd::Redo, one(S::ctrl(Key::Y))),
        (Cmd::Duplicate, one(S::ctrl(Key::D))),
        (Cmd::Group, one(S::alt(Key::G))),
        (Cmd::Delete, one(S::key(Key::Delete))),
        (Cmd::Delete, one(S::key(Key::Backspace))),
        (Cmd::Cancel, one(S::key(Key::Escape))),
        (Cmd::SelectAll, one(S::ctrl(Key::A))),
        (Cmd::Tool(Tool::Select), one(S::key(Key::Num9))),
        (Cmd::Tool(Tool::Move), one(S::key(Key::E))),
        (Cmd::Tool(Tool::Rotate), one(S::key(Key::R))),
        (Cmd::Tool(Tool::Scale), one(S::key(Key::T))),
        (Cmd::Mode(EditMode::Model), one(S::key(Key::Num1))),
        (Cmd::Mode(EditMode::Points), one(S::key(Key::Num2))),
        (Cmd::Mode(EditMode::Edges), one(S::key(Key::Num3))),
        (Cmd::Mode(EditMode::Polygons), one(S::key(Key::Num4))),
        (Cmd::MakeEditable, one(S::key(Key::C))),
        (Cmd::MeshTool(MeshTool::Extrude), one(S::key(Key::D))),
        (Cmd::MeshTool(MeshTool::Extrude), Binding::seq(Key::M, Key::T)),
        (Cmd::MeshTool(MeshTool::Inset), one(S::key(Key::I))),
        (Cmd::MeshTool(MeshTool::Inset), Binding::seq(Key::M, Key::W)),
        (Cmd::MeshTool(MeshTool::LoopCut), Binding::seq(Key::K, Key::L)),
        (Cmd::MeshTool(MeshTool::Bevel), one(S::key(Key::B))),
        (Cmd::MeshTool(MeshTool::Bevel), Binding::seq(Key::M, Key::S)),
        (Cmd::MeshTool(MeshTool::Knife), one(S::key(Key::J))),
        (Cmd::MeshTool(MeshTool::Knife), Binding::seq(Key::K, Key::K)),
        (Cmd::Bridge, one(S::shift(Key::B))),
        (Cmd::Bridge, Binding::seq(Key::M, Key::B)),
        (Cmd::Pen(PenKind::Polygon), Binding::seq(Key::M, Key::E)),
        (Cmd::Weld, one(S::key(Key::W))),
        (Cmd::Weld, Binding::seq(Key::M, Key::Q)),
        (Cmd::Optimize, Binding::seq(Key::U, Key::O)),
        (Cmd::InvertSelection, Binding::seq(Key::U, Key::I)),
        (Cmd::ReverseNormals, Binding::seq(Key::U, Key::R)),
        (Cmd::SubdivideSmooth, Binding::seq(Key::U, Key::S)),
        (Cmd::LoopSelect, Binding::seq(Key::U, Key::L)),
        (Cmd::ToggleAxis(0), one(S::key(Key::X))),
        (Cmd::ToggleAxis(1), one(S::key(Key::Y))),
        (Cmd::ToggleAxis(2), one(S::key(Key::Z))),
        (Cmd::FrameSelected, one(S::key(Key::S))),
        (Cmd::FrameAll, one(S::key(Key::H))),
        (Cmd::FourViews, one(S::key(Key::F5))),
        (Cmd::Commander, one(S::shift(Key::C))),
        (Cmd::ToggleAxisMode, one(S::key(Key::L))),
        (Cmd::ToggleSnap, one(S::shift(Key::S))),
        (Cmd::CustomizeCommands, one(S::shift(Key::F12))),
    ];
    for (k, key) in ViewKind::ALL.into_iter().zip([Key::F1, Key::F2, Key::F3, Key::F4]) {
        v.push((Cmd::Camera(k), one(S::key(key))));
    }
    v
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
            None => default_shortcuts().into_iter().filter(|(c, _)| *c == cmd).map(|(_, s)| s).collect(),
        }
    }

    /// Text for menus and tooltips ("Ctrl+Z", "M~S", or "" when unbound).
    pub fn label(&self, cmd: Cmd) -> String {
        self.shortcuts(cmd).first().map(Binding::label).unwrap_or_default()
    }

    /// Every (shortcut, command) pair in force.
    pub fn bindings(&self) -> Vec<(Binding, Cmd)> {
        let mut out: Vec<(Binding, Cmd)> =
            default_shortcuts().into_iter().filter(|(c, _)| !self.overrides.contains_key(c)).map(|(c, s)| (s, c)).collect();
        for (c, list) in &self.overrides {
            out.extend(list.iter().map(|s| (*s, *c)));
        }
        out
    }

    /// A single-key shortcut.
    pub fn command_for(&self, key: Key, m: Modifiers) -> Option<Cmd> {
        self.bindings().into_iter().find(|(b, _)| b.then.is_none() && b.first.matches(key, m)).map(|(_, c)| c)
    }

    /// True when `first` begins one or more two-key sequences.
    pub fn is_prefix(&self, first: Shortcut) -> bool {
        self.bindings().iter().any(|(b, _)| b.then.is_some() && b.first == first)
    }

    /// The second key of a sequence that started with `first`.
    pub fn command_for_seq(&self, first: Shortcut, key: Key, m: Modifiers) -> Option<Cmd> {
        self.bindings()
            .into_iter()
            .find(|(b, _)| b.first == first && b.then.is_some_and(|t| t.matches(key, m)))
            .map(|(_, c)| c)
    }

    /// What can follow `first` (for the status-bar hint), sorted by key.
    pub fn completions(&self, first: Shortcut) -> Vec<(Shortcut, Cmd)> {
        let mut v: Vec<(Shortcut, Cmd)> =
            self.bindings().into_iter().filter(|(b, _)| b.first == first).filter_map(|(b, c)| Some((b.then?, c))).collect();
        v.sort_by_key(|(s, _)| s.label());
        v
    }

    /// Binds `binding` to `cmd` alone. A shortcut already used by another
    /// command moves over (that command is returned). Refuses bindings that
    /// would make a single key and a sequence start with the same key.
    pub fn assign(&mut self, cmd: Cmd, binding: Binding) -> Result<Option<Cmd>, Conflict> {
        let others: Vec<(Binding, Cmd)> = self.bindings().into_iter().filter(|(_, c)| *c != cmd).collect();
        match binding.then {
            None => {
                let starts: Vec<Cmd> =
                    others.iter().filter(|(b, _)| b.then.is_some() && b.first == binding.first).map(|(_, c)| *c).collect();
                if !starts.is_empty() {
                    return Err(Conflict::StartsSequences(starts));
                }
            }
            Some(_) => {
                if let Some((_, c)) = others.iter().find(|(b, _)| b.then.is_none() && b.first == binding.first) {
                    return Err(Conflict::FirstKeyIsShortcut(*c));
                }
            }
        }
        let previous = others.iter().find(|(b, _)| *b == binding).map(|(_, c)| *c);
        if let Some(other) = previous {
            let rest: Vec<Binding> = self.shortcuts(other).into_iter().filter(|b| *b != binding).collect();
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

    /// Drops overrides that equal the defaults.
    fn tidy(&mut self) {
        let defaults = default_shortcuts();
        self.overrides.retain(|c, list| {
            let d: Vec<Binding> = defaults.iter().filter(|(dc, _)| dc == c).map(|(_, s)| *s).collect();
            *list != d
        });
    }

    /// `{ "command id": ["Ctrl+Z", "M~S", ...] }`, overrides only.
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
                let parsed: Vec<Binding> = list.iter().filter_map(|s| Binding::parse(s)).collect();
                km.overrides.insert(info.cmd, parsed);
            }
        }
        km.tidy();
        km
    }
}

/// Where user shortcuts are kept (`~/.config/forma3d/keymap.json` and the
/// platform equivalents).
pub fn keymap_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| {
            std::env::var_os("HOME").map(|h| {
                let h = PathBuf::from(h);
                if cfg!(target_os = "macos") { h.join("Library/Application Support") } else { h.join(".config") }
            })
        })?;
    Some(base.join("forma3d").join("keymap.json"))
}

pub fn save_keymap(km: &Keymap, infos: &[Info], path: &std::path::Path) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, km.to_json(infos)).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn load_keymap(infos: &[Info], path: &std::path::Path) -> Keymap {
    std::fs::read_to_string(path).map(|t| Keymap::from_json(&t, infos)).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Search (Commander)
// ---------------------------------------------------------------------------

/// Fuzzy match score of `query` against a command (higher is better).
/// Every query word must match the name, category or keywords.
pub fn score(info: &Info, query: &str) -> Option<i32> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Some(0);
    }
    let name = info.name.to_lowercase();
    let extra = format!("{} {}", info.category.to_lowercase(), info.keywords);
    let mut total = 0;
    for word in q.split_whitespace() {
        // The better of a name match and a (weaker) keyword match, so an exact
        // keyword like "sds" beats loose letters-in-order hits in other names.
        let in_name = word_score(&name, word).map(|s| s + 20);
        let in_extra = extra.split_whitespace().filter_map(|w| word_score(w, word)).max().map(|s| s / 2);
        let s = in_name.max(in_extra)?;
        total += s;
    }
    // Shorter names win ties ("Cube" before "Cube Object ...").
    Some(total * 10 - name.len() as i32)
}

fn word_score(text: &str, word: &str) -> Option<i32> {
    if text.starts_with(word) {
        return Some(100);
    }
    if text.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(word)) {
        return Some(80);
    }
    if text.contains(word) {
        return Some(50);
    }
    // Letters in order, with gaps.
    let mut it = text.chars();
    let mut gaps = 0;
    for ch in word.chars() {
        let mut found = false;
        for c in it.by_ref() {
            if c == ch {
                found = true;
                break;
            }
            gaps += 1;
        }
        if !found {
            return None;
        }
    }
    Some((20 - gaps).max(1))
}

pub fn search<'a>(infos: &'a [Info], query: &str) -> Vec<&'a Info> {
    let mut hits: Vec<(&Info, i32)> = infos.iter().filter_map(|i| Some((i, score(i, query)?))).collect();
    hits.sort_by_key(|h| std::cmp::Reverse(h.1));
    hits.into_iter().map(|(i, _)| i).collect()
}

// ---------------------------------------------------------------------------
// State and execution
// ---------------------------------------------------------------------------

/// Whether a command can run now, and its tick state if it is a toggle.
pub fn state(ed: &Editor, cmd: Cmd, layout: Workspace) -> (bool, Option<bool>) {
    let editable = ed.edit_target().is_some();
    let has_sel = !ed.selection.is_empty();
    match cmd {
        Cmd::Undo => (ed.history.undo_label().is_some(), None),
        Cmd::Redo => (ed.history.redo_label().is_some(), None),
        Cmd::Duplicate | Cmd::Group | Cmd::Cut | Cmd::Copy => (has_sel, None),
        Cmd::Revert => (ed.file_path.is_some(), None),
        Cmd::Delete => (has_sel || (ed.mode.is_component() && editable), None),
        Cmd::MakeEditable => (ed.active().is_some(), None),
        Cmd::SelectChildren => (has_sel && !ed.mode.is_component(), None),
        Cmd::LoopSelect => (ed.mode == EditMode::Edges && !ed.comp.edges.is_empty(), None),
        Cmd::Rename | Cmd::ResetTransform | Cmd::RemoveMaterial | Cmd::AssignMaterial(_) => (has_sel, None),
        Cmd::ToggleEnabled => (ed.selection.iter().any(|&s| ed.scene.can_disable(s)), None),
        Cmd::ApplyMaterial => (ed.selected_material.is_some() && has_sel, None),
        Cmd::DeleteMaterial | Cmd::OpenMaterialEditor => (ed.selected_material.is_some(), None),
        Cmd::Filter(f) => {
            let mut o = ed.view_opts;
            (true, Some(*f.flag(&mut o)))
        }
        Cmd::MeshTool(t) => (editable, Some(ed.mesh_tool == Some(t))),
        Cmd::Bridge | Cmd::Weld | Cmd::Optimize | Cmd::SubdivideSmooth | Cmd::SubdivideLinear | Cmd::ReverseNormals => {
            (editable, None)
        }
        Cmd::Mode(m) => (true, Some(ed.mode == m)),
        Cmd::Tool(t) => (true, Some(ed.tool == t && ed.mesh_tool.is_none() && ed.pen.is_none())),
        Cmd::Pen(k) => (true, Some(ed.pen.as_ref().is_some_and(|p| p.kind == k))),
        Cmd::ToggleAxis(i) => (true, Some(ed.axes.0[i as usize % 3])),
        Cmd::ToggleCoordSystem => (true, Some(ed.coord == CoordSystem::World)),
        Cmd::ToggleAxisMode => (true, Some(ed.axis_mode)),
        Cmd::CenterAxis => (ed.active().is_some(), None),
        Cmd::ToggleTweak => (true, Some(ed.tweak)),
        Cmd::ToggleSnap => (true, Some(ed.snap.enabled)),
        Cmd::SnapMode(m) => (true, Some(ed.snap.mode == m)),
        Cmd::SnapTo(k) => (true, Some(ed.snap.get(k))),
        Cmd::ToggleQuantize => (true, Some(ed.quantize.enabled)),
        Cmd::AlignWorkplaneToSelection | Cmd::NewLayerFromSelection | Cmd::SaveAsset => (ed.active().is_some(), None),
        Cmd::AlignSelectionToWorkplane => (has_sel, None),
        Cmd::Camera(k) => (true, Some(ed.layout == ViewLayout::Single && ed.single_view == k)),
        Cmd::FourViews => (true, Some(ed.layout == ViewLayout::Quad)),
        Cmd::Display(m) => (true, Some(ed.display == m)),
        Cmd::ToggleGrid => (true, Some(ed.view_opts.grid)),
        Cmd::ToggleCompass => (true, Some(ed.view_opts.compass)),
        Cmd::ViewFx(fx) => (true, Some(ed.view_opts.fx.get(fx))),
        Cmd::ToggleSolo => (true, Some(ed.view_opts.solo)),
        Cmd::ToggleTimeline => (true, Some(ed.show_timeline)),
        Cmd::ToggleIconBar => (true, Some(ed.show_icon_bar)),
        Cmd::PlayPause => (true, Some(ed.timeline.playing && !ed.timeline.reverse)),
        Cmd::PlayBackward => (true, Some(ed.timeline.playing && ed.timeline.reverse)),
        Cmd::Workspace(w) => (true, Some(layout == w)),
        _ => (true, None),
    }
}

/// Runs a command on the editor. Returns what the app itself must do for
/// file, window and layout commands.
pub fn execute(ed: &mut Editor, cmd: Cmd) -> Option<AppAction> {
    match cmd {
        Cmd::New => return Some(AppAction::New),
        Cmd::Open => return Some(AppAction::Open),
        Cmd::Save => return Some(AppAction::Save),
        Cmd::SaveAs => return Some(AppAction::SaveAs),
        Cmd::Quit => return Some(AppAction::Quit),
        Cmd::Close => return Some(AppAction::Close),
        Cmd::Revert => return Some(AppAction::Revert),
        Cmd::Merge => return Some(AppAction::Merge),
        Cmd::Cut => return Some(AppAction::Cut),
        Cmd::Copy => return Some(AppAction::Copy),
        Cmd::Paste => return Some(AppAction::Paste),
        Cmd::Preferences => return Some(AppAction::Preferences),
        Cmd::Workspace(w) => return Some(AppAction::Workspace(w)),
        Cmd::NextWorkspace => return Some(AppAction::CycleWorkspace(1)),
        Cmd::PrevWorkspace => return Some(AppAction::CycleWorkspace(-1)),
        Cmd::ShowPanel(p) => return Some(AppAction::ShowPanel(p)),
        Cmd::ResetLayout => return Some(AppAction::ResetLayout),
        Cmd::Commander => return Some(AppAction::Commander),
        Cmd::CustomizeCommands => return Some(AppAction::CustomizeCommands),
        Cmd::IconLibrary => return Some(AppAction::IconLibrary),
        Cmd::About => return Some(AppAction::About),
        Cmd::SaveAsset => return Some(AppAction::SaveAsset),
        Cmd::ToggleAxisMode => {
            ed.axis_mode = !ed.axis_mode;
            ed.status = if ed.axis_mode { "Axis mode on: Move and Rotate change the axis, not the geometry" } else { "Axis mode off" }.to_owned();
        }
        Cmd::CenterAxis => ed.center_axis(),
        Cmd::ToggleTweak => {
            ed.tweak = !ed.tweak;
            ed.status = if ed.tweak { "Tweak mode on: drag an object (or a point, edge, polygon) to move it directly" } else { "Tweak mode off" }.to_owned();
        }
        Cmd::ToggleSnap => {
            ed.snap.enabled = !ed.snap.enabled;
            ed.status = if ed.snap.enabled { "Snapping on" } else { "Snapping off" }.to_owned();
        }
        Cmd::SnapMode(m) => {
            ed.snap.mode = m;
            ed.snap.enabled = true;
        }
        Cmd::SnapTo(k) => ed.snap.toggle(k),
        Cmd::ToggleQuantize => {
            ed.quantize.enabled = !ed.quantize.enabled;
            let q = ed.quantize;
            ed.status = if q.enabled {
                format!("Quantizing on: {} cm, {}°, {} %", q.move_step, q.rotate_step, q.scale_step)
            } else {
                "Quantizing off".to_owned()
            };
        }
        Cmd::AlignWorkplane(a) => ed.align_workplane_to_axis(a as usize % 3),
        Cmd::AlignWorkplaneToSelection => ed.align_workplane_to_selection(),
        Cmd::AlignSelectionToWorkplane => ed.align_selection_to_workplane(),
        Cmd::NewLayerFromSelection => {
            let id = ed.scene.add_layer("Layer");
            let sel = ed.selection.clone();
            ed.scene.set_layer(&sel, Some(id));
            ed.label("New Layer");
        }
        Cmd::NewTake => {
            ed.scene.add_take("Take");
            ed.label("New Take");
            ed.status = "New take: changes you make now are recorded as its overrides".to_owned();
        }

        Cmd::Undo => ed.undo(),
        Cmd::Redo => ed.redo(),
        Cmd::Duplicate => ed.duplicate_selection(),
        Cmd::Group => ed.group_selection(),
        Cmd::Delete => {
            if ed.mode.is_component() {
                ed.delete_components();
            } else {
                ed.delete_selection();
            }
        }
        Cmd::Cancel => {
            if let Some(pen) = ed.pen.as_mut() {
                // First Esc drops unfinished polygon corners, the next leaves the pen.
                if !pen.pending.is_empty() {
                    pen.pending.clear();
                } else {
                    ed.stop_pen();
                    ed.status = "Pen finished".to_owned();
                }
            } else if ed.knife_start.is_some() {
                ed.knife_start = None;
            } else if ed.mesh_tool.is_some() {
                ed.mesh_tool = None;
                ed.tool_drag = None;
            } else if ed.mode.is_component() {
                ed.comp.clear();
            } else {
                ed.clear_selection();
            }
        }
        Cmd::Primitive(k) => {
            ed.create_primitive(k);
        }
        Cmd::Null => {
            ed.create_null();
        }
        Cmd::Generator(k) => {
            ed.create_generator(k);
        }
        Cmd::Deformer(k) => {
            ed.create_deformer(k);
        }
        Cmd::NewMaterial => {
            ed.new_material();
        }
        Cmd::Mode(m) => ed.set_mode(m),
        Cmd::MakeEditable => {
            ed.make_editable();
        }
        Cmd::ToggleAxis(i) => {
            let i = i as usize % 3;
            ed.axes.0[i] = !ed.axes.0[i];
        }
        Cmd::ToggleCoordSystem => {
            ed.coord = if ed.coord == CoordSystem::World { CoordSystem::Object } else { CoordSystem::World };
        }
        Cmd::SelectAll => {
            if ed.mode.is_component() {
                ed.comp_select_all();
            } else {
                ed.select_all();
            }
        }
        Cmd::DeselectAll => {
            if ed.mode.is_component() {
                ed.comp.clear();
            } else {
                ed.clear_selection();
            }
        }
        Cmd::InvertSelection => ed.invert_selection(),
        Cmd::SelectChildren => ed.select_children(),
        Cmd::LoopSelect => ed.loop_select(),
        Cmd::Rename => ed.renaming = ed.active(),
        Cmd::ToggleEnabled => ed.toggle_enabled(),
        Cmd::ExpandAll => {
            let mut all = Vec::new();
            ed.scene.walk(|o, _| all.push(o.id));
            ed.expanded.extend(all);
        }
        Cmd::CollapseAll => ed.expanded.clear(),
        Cmd::ResetTransform => ed.reset_transform(),
        Cmd::AssignMaterial(m) => {
            if ed.scene.material(m).is_some() {
                let sel = ed.selection.clone();
                ed.assign_material(&sel, Some(m));
            }
        }
        Cmd::RemoveMaterial => {
            let sel = ed.selection.clone();
            ed.assign_material(&sel, None);
        }
        Cmd::ApplyMaterial => {
            let sel = ed.selection.clone();
            ed.assign_material(&sel, ed.selected_material);
        }
        Cmd::DeleteMaterial => {
            if let Some(m) = ed.selected_material.take() {
                ed.scene.remove_material(m);
                ed.label("Delete Material");
            }
        }
        Cmd::OpenMaterialEditor => {
            if let Some(m) = ed.selected_material {
                return Some(AppAction::OpenMaterialEditor(m));
            }
        }
        Cmd::FrameDefault => {
            let v = ed.active_view;
            ed.cameras[v.index()] = crate::camera::Camera::new(v);
        }
        Cmd::Filter(f) => {
            let flag = f.flag(&mut ed.view_opts);
            *flag = !*flag;
        }
        Cmd::Tool(t) => {
            ed.tool = t;
            ed.mesh_tool = None;
            ed.pen = None;
        }
        Cmd::Light(k) => {
            ed.create_light(k);
        }
        Cmd::Pen(k) => {
            if ed.pen.as_ref().is_some_and(|p| p.kind == k) {
                ed.stop_pen();
            } else {
                ed.start_pen(k);
            }
        }
        Cmd::MeshTool(t) => {
            if ed.edit_target().is_some() {
                ed.set_mesh_tool(t);
            } else {
                ed.status = format!("{}: select a polygon object first (C makes the selected object editable)", t.label());
            }
        }
        Cmd::Bridge => ed.bridge(),
        Cmd::Weld => ed.weld(),
        Cmd::Optimize => ed.optimize(),
        Cmd::SubdivideSmooth => ed.subdivide(true),
        Cmd::SubdivideLinear => ed.subdivide(false),
        Cmd::ReverseNormals => ed.reverse_normals(),
        Cmd::FrameSelected => ed.frame_selected(ed.active_view),
        Cmd::FrameAll => ed.frame_all(ed.active_view),
        Cmd::Camera(k) => {
            ed.layout = ViewLayout::Single;
            ed.single_view = k;
            ed.active_view = k;
        }
        Cmd::FourViews => ed.layout = ViewLayout::Quad,
        Cmd::Display(m) => ed.display = m,
        Cmd::ToggleGrid => ed.view_opts.grid = !ed.view_opts.grid,
        Cmd::ToggleCompass => ed.view_opts.compass = !ed.view_opts.compass,
        Cmd::ViewFx(fx) => ed.view_opts.fx.toggle(fx),
        Cmd::ToggleSolo => ed.view_opts.solo = !ed.view_opts.solo,
        Cmd::ToggleTimeline => ed.show_timeline = !ed.show_timeline,
        Cmd::ToggleIconBar => ed.show_icon_bar = !ed.show_icon_bar,
        Cmd::PlayPause => {
            let t = &mut ed.timeline;
            let fwd = t.playing && !t.reverse;
            t.playing = !fwd;
            t.reverse = false;
        }
        Cmd::PlayBackward => {
            let t = &mut ed.timeline;
            let back = t.playing && t.reverse;
            t.playing = !back;
            t.reverse = true;
        }
        Cmd::NextFrame => ed.timeline.frame = (ed.timeline.frame + 1).min(ed.timeline.max),
        Cmd::PrevFrame => ed.timeline.frame = (ed.timeline.frame - 1).max(ed.timeline.min),
        Cmd::ToStart => ed.timeline.frame = ed.timeline.min,
        Cmd::ToEnd => ed.timeline.frame = ed.timeline.max,
    }
    None
}

// ---------------------------------------------------------------------------
// UI glue: any panel can draw a command and queue it; the app runs the queue.
// ---------------------------------------------------------------------------

/// What panels need to draw commands, published by the app every frame.
#[derive(Clone)]
pub struct Shared {
    pub infos: std::sync::Arc<Vec<Info>>,
    pub keymap: std::sync::Arc<Keymap>,
    pub layout: Workspace,
}

fn shared_id() -> eframe::egui::Id {
    eframe::egui::Id::new("forma3d-commands")
}

fn queue_id() -> eframe::egui::Id {
    eframe::egui::Id::new("forma3d-command-queue")
}

pub fn publish(ctx: &eframe::egui::Context, shared: Shared) {
    ctx.data_mut(|d| d.insert_temp(shared_id(), shared));
}

pub fn shared(ctx: &eframe::egui::Context) -> Option<Shared> {
    ctx.data(|d| d.get_temp::<Shared>(shared_id()))
}

pub fn queue(ctx: &eframe::egui::Context, cmd: Cmd) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Vec<Cmd>>(queue_id()).push(cmd));
}

pub fn take_queue(ctx: &eframe::egui::Context) -> Vec<Cmd> {
    ctx.data_mut(|d| std::mem::take(d.get_temp_mut_or_default::<Vec<Cmd>>(queue_id())))
}

impl Shared {
    pub fn info(&self, cmd: Cmd) -> Option<&Info> {
        self.infos.iter().find(|i| i.cmd == cmd)
    }

    /// "Name (Shortcut)" for tooltips.
    pub fn tooltip(&self, cmd: Cmd) -> String {
        let name = self.info(cmd).map_or_else(|| format!("{cmd:?}"), |i| i.name.clone());
        match self.keymap.label(cmd) {
            k if k.is_empty() => name,
            k => format!("{name} ({k})"),
        }
    }
}

/// The current shortcut text of `cmd` ("" when unbound), for menus that
/// act in their own context but share a command's key.
pub fn key_label(ctx: &eframe::egui::Context, cmd: Cmd) -> String {
    shared(ctx).map(|s| s.keymap.label(cmd)).unwrap_or_default()
}

/// A menu row for `cmd`: registry name, icon and shortcut, greyed when it
/// can't run, ticked when it's an active toggle. Clicking queues the command.
pub fn menu_item(ui: &mut eframe::egui::Ui, ed: &Editor, cmd: Cmd) -> bool {
    let Some(sh) = shared(ui.ctx()) else { return false };
    let Some(info) = sh.info(cmd) else { return false };
    let (enabled, checked) = state(ed, cmd, sh.layout);
    let name = match cmd {
        Cmd::Undo => ed.history.undo_label().map_or_else(|| "Undo".to_owned(), |l| format!("Undo {l}")),
        Cmd::Redo => ed.history.redo_label().map_or_else(|| "Redo".to_owned(), |l| format!("Redo {l}")),
        _ => info.name.clone(),
    };
    let clicked = super::widgets::menu_row(ui, info.icon, checked, &name, &sh.keymap.label(cmd), enabled).clicked();
    if clicked {
        queue(ui.ctx(), cmd);
    }
    clicked
}

/// A palette icon button for `cmd` (lit when it is the active toggle).
pub fn palette_button(ui: &mut eframe::egui::Ui, ed: &Editor, cmd: Cmd) -> bool {
    let Some(sh) = shared(ui.ctx()) else { return false };
    let Some(info) = sh.info(cmd) else { return false };
    let (enabled, checked) = state(ed, cmd, sh.layout);
    let icon = info.icon.unwrap_or(Icon::Select);
    let clicked =
        super::widgets::palette_button_enabled(ui, icon, checked == Some(true), enabled, &sh.tooltip(cmd)).clicked();
    if clicked {
        queue(ui.ctx(), cmd);
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;
    use std::collections::HashSet;

    #[test]
    fn ids_and_names_are_unique() {
        let infos = all();
        let ids: HashSet<&str> = infos.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids.len(), infos.len(), "duplicate command id");
        let cmds: HashSet<Cmd> = infos.iter().map(|i| i.cmd).collect();
        assert_eq!(cmds.len(), infos.len(), "a command is listed twice");
        // Every default shortcut belongs to a listed command.
        for (c, _) in default_shortcuts() {
            assert!(cmds.contains(&c), "{c:?} has a shortcut but no entry");
        }
    }

    #[test]
    fn default_shortcuts_do_not_collide_and_round_trip() {
        let mut seen = HashMap::new();
        let defaults = default_shortcuts();
        for (c, b) in &defaults {
            if let Some(other) = seen.insert(*b, *c) {
                panic!("{} is bound to both {other:?} and {c:?}", b.label());
            }
            assert_eq!(Binding::parse(&b.label()), Some(*b), "{}", b.label());
        }
        // A key can't be both a shortcut and the start of a sequence.
        for (_, b) in defaults.iter().filter(|(_, b)| b.then.is_some()) {
            assert!(
                !defaults.iter().any(|(_, s)| s.then.is_none() && s.first == b.first),
                "{} starts a sequence but is also a shortcut",
                b.first.label()
            );
        }
    }

    #[test]
    fn modifiers_match_exactly() {
        let km = Keymap::default();
        let none = Modifiers::NONE;
        assert_eq!(km.command_for(Key::B, none), Some(Cmd::MeshTool(MeshTool::Bevel)));
        assert_eq!(km.command_for(Key::B, Modifiers::SHIFT), Some(Cmd::Bridge));
        assert_eq!(km.command_for(Key::C, Modifiers::SHIFT), Some(Cmd::Commander));
        assert_eq!(km.command_for(Key::Z, Modifiers::COMMAND), Some(Cmd::Undo));
        assert_eq!(km.command_for(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT), Some(Cmd::Redo));
        assert_eq!(km.command_for(Key::Z, none), Some(Cmd::ToggleAxis(2)));
        assert_eq!(km.command_for(Key::Q, Modifiers::ALT), None);
    }

    #[test]
    fn sequences() {
        let km = Keymap::default();
        let m = Shortcut::key(Key::M);
        assert!(km.is_prefix(m) && km.is_prefix(Shortcut::key(Key::K)) && km.is_prefix(Shortcut::key(Key::U)));
        assert!(!km.is_prefix(Shortcut::key(Key::E)));
        assert_eq!(km.command_for(Key::M, Modifiers::NONE), None, "M alone does nothing");
        assert_eq!(km.command_for_seq(m, Key::S, Modifiers::NONE), Some(Cmd::MeshTool(MeshTool::Bevel)));
        assert_eq!(km.command_for_seq(Shortcut::key(Key::K), Key::L, Modifiers::NONE), Some(Cmd::MeshTool(MeshTool::LoopCut)));
        assert_eq!(km.command_for_seq(m, Key::S, Modifiers::SHIFT), None, "exact modifiers on the second key too");
        let next: Vec<String> = km.completions(m).iter().map(|(s, _)| s.label()).collect();
        assert_eq!(next, ["B", "E", "Q", "S", "T", "W"]);
        assert_eq!(km.label(Cmd::MeshTool(MeshTool::LoopCut)), "K~L");
        assert_eq!(Binding::parse("Ctrl+M~Shift+S").unwrap().label(), "Ctrl+M~Shift+S");
        assert_eq!(Binding::parse("M~"), None);
    }

    #[test]
    fn rebinding_steals_clears_resets_and_saves() {
        let infos = all();
        let mut km = Keymap::default();
        // Give Weld the Bevel key: Bevel loses it.
        let lost = km.assign(Cmd::Weld, Binding::single(Shortcut::key(Key::B))).unwrap();
        assert_eq!(lost, Some(Cmd::MeshTool(MeshTool::Bevel)));
        assert_eq!(km.command_for(Key::B, Modifiers::NONE), Some(Cmd::Weld));
        assert_eq!(km.label(Cmd::MeshTool(MeshTool::Bevel)), "M~S", "Bevel keeps its sequence");
        assert_eq!(km.command_for(Key::W, Modifiers::NONE), None, "Weld's old key is free now");
        // Sequences can be assigned and moved too.
        let lost = km.assign(Cmd::Optimize, Binding::seq(Key::K, Key::K)).unwrap();
        assert_eq!(lost, Some(Cmd::MeshTool(MeshTool::Knife)));
        km.clear(Cmd::Undo);
        assert!(km.shortcuts(Cmd::Undo).is_empty());

        let dir = std::env::temp_dir().join(format!("forma3d-keymap-{}", std::process::id()));
        let path = dir.join("keymap.json");
        save_keymap(&km, &infos, &path).unwrap();
        let back = load_keymap(&infos, &path);
        assert_eq!(back, km);
        std::fs::remove_dir_all(&dir).ok();

        km.reset(Cmd::Undo);
        assert_eq!(km.label(Cmd::Undo), "Ctrl+Z");
        km.reset_all();
        assert_eq!(km, Keymap::default());
        km.assign(Cmd::Save, Binding::single(Shortcut::ctrl(Key::S))).unwrap();
        assert!(!km.is_custom(Cmd::Save), "rebinding to the default leaves no override");
    }

    #[test]
    fn sequence_conflicts_are_refused() {
        let mut km = Keymap::default();
        // M starts M~S, M~B...: it can't also be a shortcut on its own.
        match km.assign(Cmd::Undo, Binding::single(Shortcut::key(Key::M))) {
            Err(Conflict::StartsSequences(cmds)) => assert!(cmds.contains(&Cmd::Bridge)),
            other => panic!("{other:?}"),
        }
        // E is Move: it can't start a sequence.
        assert_eq!(km.assign(Cmd::Undo, Binding::seq(Key::E, Key::X)), Err(Conflict::FirstKeyIsShortcut(Cmd::Tool(Tool::Move))));
        assert_eq!(km, Keymap::default(), "refused changes leave the keymap alone");
        // A command may turn its own key into the start of a sequence.
        assert!(km.assign(Cmd::Weld, Binding::seq(Key::W, Key::W)).is_ok(), "W was Weld's own key");
    }

    #[test]
    fn broken_keymap_files_are_ignored() {
        let infos = all();
        let km = Keymap::from_json(r#"{"no.such.command": ["Ctrl+Q"], "mesh.weld": ["Hyper+Q", "Shift+W", "M~Z"]}"#, &infos);
        assert_eq!(km.shortcuts(Cmd::Weld), vec![Binding::single(Shortcut::shift(Key::W)), Binding::seq(Key::M, Key::Z)]);
        assert_eq!(Keymap::from_json("not json", &infos), Keymap::default());
    }

    #[test]
    fn context_commands() {
        let mut ed = Editor::new(Scene::demo());
        let infos = dynamic(&ed);
        assert_eq!(infos.len(), 3, "one Material Tag command per material");
        let roots = ed.scene.roots().to_vec();
        ed.select_only(roots[0]);
        let steel = ed.scene.materials()[0].id;
        let sage = ed.scene.materials()[2].id;
        assert!(state(&ed, Cmd::AssignMaterial(sage), Workspace::Model).0);
        execute(&mut ed, Cmd::AssignMaterial(sage));
        assert_eq!(ed.scene.get(roots[0]).unwrap().material, Some(sage));
        execute(&mut ed, Cmd::RemoveMaterial);
        assert_eq!(ed.scene.get(roots[0]).unwrap().material, None);
        ed.selected_material = Some(steel);
        execute(&mut ed, Cmd::ApplyMaterial);
        assert_eq!(ed.scene.get(roots[0]).unwrap().material, Some(steel));
        assert_eq!(execute(&mut ed, Cmd::OpenMaterialEditor), Some(AppAction::OpenMaterialEditor(steel)));
        // Filters and Frame Default act on the viewport.
        execute(&mut ed, Cmd::Filter(ViewFilter::Nulls));
        assert!(!ed.view_opts.nulls);
        assert_eq!(state(&ed, Cmd::Filter(ViewFilter::Nulls), Workspace::Model).1, Some(false));
        ed.cameras[0].distance = 12345.0;
        execute(&mut ed, Cmd::FrameDefault);
        assert!(ed.cameras[0].distance < 12000.0);
        // Reset PSR, Enable / Disable, Rename.
        ed.scene.get_mut(roots[0]).unwrap().pos.x = 77.0;
        execute(&mut ed, Cmd::ResetTransform);
        assert_eq!(ed.scene.get(roots[0]).unwrap().pos, glam::Vec3::ZERO);
        let g = ed.create_generator(GeneratorKind::Subdivision);
        execute(&mut ed, Cmd::ToggleEnabled);
        assert!(!ed.scene.get(g).unwrap().enabled);
        execute(&mut ed, Cmd::Rename);
        assert_eq!(ed.renaming, Some(g));
        execute(&mut ed, Cmd::CollapseAll);
        assert!(ed.expanded.is_empty());
        execute(&mut ed, Cmd::ExpandAll);
        assert!(ed.expanded.contains(&g));
    }

    #[test]
    fn loop_selection_command() {
        let mut ed = Editor::new(Scene::new());
        let id = ed.create_primitive(PrimitiveKind::Cube);
        if let Some(ObjectKind::Primitive(crate::primitive::Primitive::Cube { segments, .. })) = ed.scene.get_mut(id).map(|o| &mut o.kind) {
            *segments = [2, 2, 2];
        }
        execute(&mut ed, Cmd::MakeEditable);
        execute(&mut ed, Cmd::Mode(EditMode::Edges));
        ed.prune_selection();
        let mesh = crate::component::mesh_of(&ed.scene, id).unwrap().clone();
        let seed = mesh.edges().into_iter().find(|&(a, b)| mesh.points[a as usize].y.abs() < 1e-3 && mesh.points[b as usize].y.abs() < 1e-3).unwrap();
        ed.comp.edges.insert(seed);
        assert!(state(&ed, Cmd::LoopSelect, Workspace::Model).0);
        execute(&mut ed, Cmd::LoopSelect);
        assert_eq!(ed.comp.edges.len(), 8);
    }

    #[test]
    fn search_ranks_sensibly() {
        let infos = all();
        let first = |q: &str| search(&infos, q).first().map(|i| i.cmd);
        assert_eq!(first("cube"), Some(Cmd::Primitive(PrimitiveKind::Cube)));
        assert_eq!(first("extr"), Some(Cmd::MeshTool(MeshTool::Extrude)));
        assert_eq!(first("sub smooth"), Some(Cmd::SubdivideSmooth));
        assert_eq!(first("sds"), Some(Cmd::Generator(GeneratorKind::Subdivision)), "keywords count");
        assert_eq!(first("shortcuts"), Some(Cmd::CustomizeCommands));
        assert_eq!(first("brdg"), Some(Cmd::Bridge), "letters in order");
        assert!(search(&infos, "zzzz").is_empty());
        assert_eq!(search(&infos, "").len(), infos.len());
    }

    #[test]
    fn execute_and_state() {
        let mut ed = Editor::new(Scene::new());
        assert_eq!(execute(&mut ed, Cmd::Save), Some(AppAction::Save));
        assert!(!state(&ed, Cmd::MakeEditable, Workspace::Model).0);
        execute(&mut ed, Cmd::Primitive(PrimitiveKind::Cube));
        assert_eq!(ed.scene.len(), 1);
        assert!(state(&ed, Cmd::MakeEditable, Workspace::Model).0);
        execute(&mut ed, Cmd::MakeEditable);
        execute(&mut ed, Cmd::Mode(EditMode::Polygons));
        assert_eq!(state(&ed, Cmd::Mode(EditMode::Polygons), Workspace::Model), (true, Some(true)));
        ed.prune_selection();
        execute(&mut ed, Cmd::SelectAll);
        assert_eq!(ed.comp.polys.len(), 6, "Select All acts on components in component modes");
        execute(&mut ed, Cmd::Cancel);
        assert!(ed.comp.polys.is_empty());
        execute(&mut ed, Cmd::ToggleAxis(0));
        assert_eq!(state(&ed, Cmd::ToggleAxis(0), Workspace::Model).1, Some(false));
        assert_eq!(state(&ed, Cmd::Workspace(Workspace::Sculpt), Workspace::Sculpt).1, Some(true));
        assert_eq!(state(&ed, Cmd::Workspace(Workspace::Model), Workspace::Sculpt).1, Some(false));
        // Every command runs without panicking on an empty scene and on the demo.
        for scene in [Scene::new(), Scene::demo()] {
            let mut e = Editor::new(scene);
            let mut cmds: Vec<Cmd> = all().into_iter().map(|i| i.cmd).collect();
            cmds.extend(dynamic(&e).into_iter().map(|i| i.cmd));
            cmds.push(Cmd::AssignMaterial(999_999));
            for c in cmds {
                execute(&mut e, c);
            }
        }
    }
}
