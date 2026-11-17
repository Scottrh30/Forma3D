//! The command registry: every menu entry, palette button and keyboard
//! shortcut is a `Cmd`. Menus, palettes, the Commander and the keyboard all
//! go through this one table, so a command has one name, one icon and one set
//! of shortcuts everywhere (see `keymap` for the shortcuts).

use crate::host;
use crate::keymap::{Binding, Shortcut};
use crate::state::{App, Ask};
use crate::workspace::{Panel, Workspace};
use forma3d_core::camera::ViewKind;
use forma3d_core::component::EditMode;
use forma3d_core::deformer::DeformerKind;
use forma3d_core::editor::{Editor, MeshTool, ViewLayout};
use forma3d_core::generator::GeneratorKind;
use forma3d_core::gizmo::{CoordSystem, Tool};
use forma3d_core::light::LightKind;
use forma3d_core::pen::PenKind;
use forma3d_core::primitive::PrimitiveKind;
use forma3d_core::renderer::{DisplayMode, ViewFx};
use forma3d_core::scene::ObjectKind;
use forma3d_core::scene_camera::CameraKind;
use forma3d_core::sky::SkyKind;
use forma3d_core::snap::{SnapKind, SnapMode};
use leptos::prelude::*;
use std::sync::OnceLock;

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
    pub const ALL: [ViewFilter; 7] = [
        ViewFilter::Primitives,
        ViewFilter::Polygons,
        ViewFilter::Splines,
        ViewFilter::Generators,
        ViewFilter::Deformers,
        ViewFilter::Lights,
        ViewFilter::Nulls,
    ];

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

    fn flag(self, o: &mut forma3d_core::editor::ViewOptions) -> &mut bool {
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

pub const MESH_TOOLS: [MeshTool; 5] = [
    MeshTool::Extrude,
    MeshTool::Inset,
    MeshTool::LoopCut,
    MeshTool::Bevel,
    MeshTool::Knife,
];

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
    Light(LightKind),
    TargetLight,
    Sky(SkyKind),
    SceneCamera(CameraKind),
    Pen(PenKind),
    // Modes
    Mode(EditMode),
    MakeEditable,
    ToggleAxis(u8),
    ToggleCoordSystem,
    ToggleAxisMode,
    CenterAxis,
    ToggleTweak,
    ToggleSnap,
    SnapMode(SnapMode),
    SnapTo(SnapKind),
    ToggleQuantize,
    AlignWorkplane(u8),
    AlignWorkplaneToSelection,
    AlignSelectionToWorkplane,
    NewLayerFromSelection,
    NewTake,
    // Select
    SelectAll,
    DeselectAll,
    InvertSelection,
    SelectChildren,
    LoopSelect,
    // Objects
    Rename,
    ToggleEnabled,
    ExpandAll,
    CollapseAll,
    ResetTransform,
    AssignMaterial(u64),
    RemoveMaterial,
    ApplyMaterial,
    DeleteMaterial,
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
    FrameDefault,
    Camera(ViewKind),
    FourViews,
    Filter(ViewFilter),
    Display(DisplayMode),
    ToggleGrid,
    ToggleCompass,
    ToggleSolo,
    ViewFx(ViewFx),
    // Cameras
    LookThrough,
    EditorCamera,
    UseRenderCamera,
    FocusOnSelection,
    // Render
    RenderView,
    RenderToViewer,
    RenderRegion,
    StopRender,
    RenderSettings,
    AddToQueue,
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
    Minimize,
    ToggleMaximize,
    ToggleFullscreen,
    Commander,
    CustomizeCommands,
    About,
}

/// Name, icon, menu category and search words of a command.
#[derive(Clone, Debug)]
pub struct Info {
    pub cmd: Cmd,
    pub id: String,
    pub name: String,
    pub category: &'static str,
    pub icon: &'static str,
    pub keywords: &'static str,
}

/// Every command, in menu order (built once).
pub fn infos() -> &'static [Info] {
    static ALL: OnceLock<Vec<Info>> = OnceLock::new();
    ALL.get_or_init(all)
}

pub fn info(cmd: Cmd) -> Info {
    infos()
        .iter()
        .find(|i| i.cmd == cmd)
        .cloned()
        .unwrap_or_else(|| Info {
            cmd,
            id: format!("{cmd:?}"),
            name: match cmd {
                Cmd::AssignMaterial(_) => "Material Tag".to_owned(),
                _ => format!("{cmd:?}"),
            },
            category: "Tags",
            icon: "material",
            keywords: "",
        })
}

fn tool_icon(t: Tool) -> &'static str {
    match t {
        Tool::Select => "select",
        Tool::Move => "move",
        Tool::Rotate => "rotate",
        Tool::Scale => "scale",
    }
}

pub fn mesh_tool_icon(t: MeshTool) -> &'static str {
    match t {
        MeshTool::Extrude => "extrude",
        MeshTool::Inset => "inset",
        MeshTool::LoopCut => "loopcut",
        MeshTool::Bevel => "bevel",
        MeshTool::Knife => "knife",
    }
}

pub fn fx_icon(fx: ViewFx) -> &'static str {
    match fx {
        ViewFx::PostEffects => "fx-post",
        ViewFx::Tessellation => "fx-tess",
        ViewFx::Shadows => "fx-shadow",
        ViewFx::Reflections => "fx-reflect",
        ViewFx::Ssao => "fx-ssao",
        ViewFx::BackfaceCulling => "fx-backface",
    }
}

pub fn all() -> Vec<Info> {
    let mut v = Vec::new();
    let mut add = |cmd: Cmd,
                   id: &str,
                   name: &str,
                   category: &'static str,
                   icon: &'static str,
                   keywords: &'static str| {
        v.push(Info {
            cmd,
            id: id.to_owned(),
            name: name.to_owned(),
            category,
            icon,
            keywords,
        });
    };
    add(
        Cmd::New,
        "file.new",
        "New Project",
        "File",
        "new",
        "scene clear document",
    );
    add(
        Cmd::Open,
        "file.open",
        "Open...",
        "File",
        "open",
        "load scene project",
    );
    add(
        Cmd::Close,
        "file.close",
        "Close",
        "File",
        "close",
        "close project document",
    );
    add(
        Cmd::Revert,
        "file.revert",
        "Revert to Saved",
        "File",
        "undo",
        "reload discard changes",
    );
    add(Cmd::Save, "file.save", "Save", "File", "save", "");
    add(
        Cmd::SaveAs,
        "file.save_as",
        "Save As...",
        "File",
        "save",
        "",
    );
    add(
        Cmd::Merge,
        "file.merge",
        "Merge Project...",
        "File",
        "open",
        "import add scene objects",
    );
    add(
        Cmd::Quit,
        "file.quit",
        "Quit",
        "File",
        "power",
        "exit close",
    );
    add(Cmd::Undo, "edit.undo", "Undo", "Edit", "undo", "");
    add(Cmd::Redo, "edit.redo", "Redo", "Edit", "redo", "");
    add(Cmd::Cut, "edit.cut", "Cut", "Edit", "cut", "clipboard");
    add(Cmd::Copy, "edit.copy", "Copy", "Edit", "copy", "clipboard");
    add(
        Cmd::Paste,
        "edit.paste",
        "Paste",
        "Edit",
        "paste",
        "clipboard",
    );
    add(
        Cmd::Preferences,
        "edit.preferences",
        "Preferences...",
        "Edit",
        "settings",
        "settings options autosave undo scale",
    );
    add(
        Cmd::Duplicate,
        "edit.duplicate",
        "Duplicate",
        "Edit",
        "duplicate",
        "copy clone",
    );
    add(
        Cmd::Group,
        "edit.group",
        "Group Objects",
        "Edit",
        "group",
        "null parent",
    );
    add(
        Cmd::Delete,
        "edit.delete",
        "Delete",
        "Edit",
        "trash",
        "remove dissolve",
    );
    add(
        Cmd::Cancel,
        "edit.cancel",
        "Cancel / Deselect",
        "Edit",
        "close",
        "escape",
    );
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
        add(
            Cmd::Primitive(k),
            &format!("create.{}", k.name().to_lowercase()),
            k.name(),
            "Create",
            crate::icons::for_primitive(k),
            keywords,
        );
    }
    add(
        Cmd::Null,
        "create.null",
        "Null",
        "Create",
        "null",
        "empty group",
    );
    for k in LightKind::ALL {
        add(
            Cmd::Light(k),
            &format!("light.{}", k.id()),
            k.name(),
            "Light",
            "light",
            k.keywords(),
        );
    }
    add(Cmd::TargetLight, "light.target", "Target Light", "Light", "light", "spot aim look at target null");
    for k in SkyKind::ALL {
        add(Cmd::Sky(k), &format!("light.{}", k.id()), k.name(), "Light", "sky", k.keywords());
    }
    for k in CameraKind::ALL {
        add(Cmd::SceneCamera(k), &format!("create.camera.{}", k.id()), k.name(), "Create", "camera", k.keywords());
    }
    add(Cmd::LookThrough, "cameras.look_through", "Look Through Camera", "Cameras", "camera", "use camera view active object set");
    add(Cmd::EditorCamera, "cameras.editor", "Editor Camera", "Cameras", "camera", "stop looking through default view");
    add(Cmd::UseRenderCamera, "cameras.render_camera", "Use as Render Camera", "Cameras", "camera", "render camera active final");
    add(Cmd::FocusOnSelection, "cameras.focus", "Focus on Selection", "Cameras", "camera", "depth of field focus distance dof pick");
    add(Cmd::RenderView, "render.view", "Render View", "Render", "render", "render active view preview");
    add(Cmd::RenderToViewer, "render.picture_viewer", "Render to Picture Viewer", "Render", "render", "render final picture image save frames");
    add(Cmd::RenderRegion, "render.region", "Interactive Render Region", "Render", "render", "ipr interactive region live preview");
    add(Cmd::StopRender, "render.stop", "Stop Rendering", "Render", "close", "cancel abort render");
    add(Cmd::RenderSettings, "render.settings", "Edit Render Settings...", "Render", "settings", "render settings output resolution samples passes color management");
    add(Cmd::AddToQueue, "render.queue_add", "Add to Render Queue", "Render", "render", "batch render queue");
    add(
        Cmd::Pen(PenKind::Spline),
        "create.spline_pen",
        "Spline Pen",
        "Create",
        "pen",
        "spline curve draw bezier path line",
    );
    add(
        Cmd::Pen(PenKind::Polygon),
        "mesh.polygon_pen",
        "Polygon Pen",
        "Mesh",
        "polygon",
        "polygon draw retopology create face quad",
    );
    for k in GeneratorKind::ALL {
        let keywords = match k {
            GeneratorKind::Subdivision => "generator sds smooth subd hypernurbs",
            GeneratorKind::Array => "generator repeat copies clone",
            GeneratorKind::Symmetry => "generator mirror",
            GeneratorKind::Boolean => "generator boole csg subtract union intersect",
        };
        let icon = crate::icons::for_kind(&ObjectKind::Generator(k.default_generator()));
        add(
            Cmd::Generator(k),
            &format!("create.{}", k.name().to_lowercase().replace(' ', "_")),
            k.name(),
            "Create",
            icon,
            keywords,
        );
    }
    for k in DeformerKind::ALL {
        let keywords = match k {
            DeformerKind::Bend => "deformer curve",
            DeformerKind::Twist => "deformer rotate spiral",
            DeformerKind::Taper => "deformer narrow scale cone pinch",
            DeformerKind::Shear => "deformer lean slant skew offset",
        };
        add(
            Cmd::Deformer(k),
            &format!("create.{}", k.name().to_lowercase()),
            k.name(),
            "Create",
            "deformer",
            keywords,
        );
    }
    add(
        Cmd::NewMaterial,
        "create.material",
        "New Material",
        "Create",
        "material",
        "colour color",
    );
    for (m, icon, keywords) in [
        (EditMode::Model, "model", "object resize geometry"),
        (EditMode::Object, "object", "object scale axes children"),
        (EditMode::Points, "points", "component vertex"),
        (EditMode::Edges, "edges", "component"),
        (EditMode::Polygons, "polys", "component face"),
        (EditMode::Workplane, "workplane", "construction plane grid"),
    ] {
        add(
            Cmd::Mode(m),
            &format!("mode.{}", m.label().to_lowercase()),
            &format!("{} Mode", m.label()),
            "Modes",
            icon,
            keywords,
        );
    }
    add(
        Cmd::ToggleAxisMode,
        "mode.enable_axis",
        "Enable Axis",
        "Modes",
        "axis",
        "axis pivot center move origin",
    );
    add(
        Cmd::CenterAxis,
        "mode.center_axis",
        "Center Axis to Geometry",
        "Modes",
        "axis",
        "axis pivot origin centre",
    );
    add(
        Cmd::ToggleTweak,
        "mode.tweak",
        "Tweak Mode",
        "Modes",
        "tweak",
        "drag direct move under cursor",
    );
    add(
        Cmd::ToggleSnap,
        "snap.enable",
        "Enable Snap",
        "Snap",
        "snap",
        "snapping magnet",
    );
    for m in SnapMode::ALL {
        let id = match m {
            SnapMode::Snap3D => "snap.mode_3d",
            SnapMode::Snap25D => "snap.mode_25d",
            SnapMode::Snap2D => "snap.mode_2d",
        };
        add(Cmd::SnapMode(m), id, m.label(), "Snap", "snap", "snapping");
    }
    for k in SnapKind::ALL {
        let id = format!("snap.to_{}", k.label().to_lowercase().replace(' ', "_"));
        add(
            Cmd::SnapTo(k),
            &id,
            &format!("Snap to {}", k.label()),
            "Snap",
            "snap",
            "snapping target",
        );
    }
    add(
        Cmd::ToggleQuantize,
        "snap.quantize",
        "Enable Quantizing",
        "Snap",
        "quantize",
        "step increment grid round",
    );
    for (i, a) in ["X", "Y", "Z"].into_iter().enumerate() {
        add(
            Cmd::AlignWorkplane(i as u8),
            &format!("workplane.align_{}", a.to_lowercase()),
            &format!("Align Workplane to {a}"),
            "Workplane",
            "workplane",
            "construction plane",
        );
    }
    add(
        Cmd::AlignWorkplaneToSelection,
        "workplane.align_selection",
        "Align Workplane to Selection",
        "Workplane",
        "workplane",
        "construction plane object",
    );
    add(
        Cmd::AlignSelectionToWorkplane,
        "workplane.align_objects",
        "Align Selection to Workplane",
        "Workplane",
        "workplane",
        "construction plane object move",
    );
    add(
        Cmd::NewLayerFromSelection,
        "layer.new_from_selection",
        "Add to New Layer",
        "Layer",
        "layers",
        "layer manager selection",
    );
    add(
        Cmd::NewTake,
        "take.new",
        "New Take",
        "Take",
        "takes",
        "take variation override",
    );
    add(
        Cmd::MakeEditable,
        "mode.make_editable",
        "Make Editable",
        "Modes",
        "editable",
        "convert polygon",
    );
    for (i, a) in ["X", "Y", "Z"].into_iter().enumerate() {
        add(
            Cmd::ToggleAxis(i as u8),
            &format!("mode.axis_{}", a.to_lowercase()),
            &format!("{a}-Axis Lock"),
            "Modes",
            "move",
            "axis lock",
        );
    }
    add(
        Cmd::ToggleCoordSystem,
        "mode.coord_system",
        "World / Object Coordinate System",
        "Modes",
        "world",
        "axes",
    );
    add(
        Cmd::SelectAll,
        "select.all",
        "Select All",
        "Select",
        "select",
        "",
    );
    add(
        Cmd::DeselectAll,
        "select.none",
        "Deselect All",
        "Select",
        "select",
        "clear",
    );
    add(
        Cmd::InvertSelection,
        "select.invert",
        "Invert Selection",
        "Select",
        "select",
        "",
    );
    add(
        Cmd::SelectChildren,
        "select.children",
        "Select Children",
        "Select",
        "select",
        "hierarchy",
    );
    add(
        Cmd::LoopSelect,
        "select.loop",
        "Loop Selection",
        "Select",
        "loopsel",
        "edge ring",
    );
    add(
        Cmd::Rename,
        "object.rename",
        "Rename",
        "Object",
        "rename",
        "name",
    );
    add(
        Cmd::ToggleEnabled,
        "object.toggle_enabled",
        "Enable / Disable",
        "Object",
        "power",
        "switch on off generator deformer check",
    );
    add(
        Cmd::ExpandAll,
        "object.expand_all",
        "Expand All",
        "Object",
        "chev-down",
        "hierarchy tree",
    );
    add(
        Cmd::CollapseAll,
        "object.collapse_all",
        "Collapse All",
        "Object",
        "chev-right",
        "hierarchy tree fold",
    );
    add(
        Cmd::ResetTransform,
        "object.reset_psr",
        "Reset PSR (Reset Transform)",
        "Object",
        "undo",
        "position scale rotation zero",
    );
    add(
        Cmd::RemoveMaterial,
        "tags.remove_material",
        "Remove Material Tag",
        "Tags",
        "material",
        "material",
    );
    add(
        Cmd::ApplyMaterial,
        "material.apply",
        "Apply Material to Selected Objects",
        "Material",
        "material",
        "assign tag",
    );
    add(
        Cmd::DeleteMaterial,
        "material.delete",
        "Delete Material",
        "Material",
        "trash",
        "remove",
    );
    for (t, name) in [
        (Tool::Select, "Select"),
        (Tool::Move, "Move"),
        (Tool::Scale, "Scale"),
        (Tool::Rotate, "Rotate"),
    ] {
        add(
            Cmd::Tool(t),
            &format!("tool.{}", name.to_lowercase()),
            name,
            "Tools",
            tool_icon(t),
            "gizmo transform",
        );
    }
    for t in MESH_TOOLS {
        add(
            Cmd::MeshTool(t),
            &format!("tool.{}", t.label().to_lowercase().replace(' ', "_")),
            t.label(),
            "Tools",
            mesh_tool_icon(t),
            "mesh modeling",
        );
    }
    add(
        Cmd::Bridge,
        "mesh.bridge",
        "Bridge",
        "Mesh",
        "bridge",
        "connect tunnel",
    );
    add(
        Cmd::Weld,
        "mesh.weld",
        "Weld",
        "Mesh",
        "weld",
        "merge collapse",
    );
    add(
        Cmd::Optimize,
        "mesh.optimize",
        "Optimize (merge close points)",
        "Mesh",
        "optimize",
        "merge duplicate",
    );
    add(
        Cmd::SubdivideSmooth,
        "mesh.subdivide_smooth",
        "Subdivide (smooth, whole object)",
        "Mesh",
        "subdivide",
        "catmull clark",
    );
    add(
        Cmd::SubdivideLinear,
        "mesh.subdivide_linear",
        "Subdivide (linear, selected polygons)",
        "Mesh",
        "subdivide",
        "split",
    );
    add(
        Cmd::ReverseNormals,
        "mesh.reverse_normals",
        "Reverse Normals",
        "Mesh",
        "normals",
        "flip",
    );
    add(
        Cmd::FrameSelected,
        "view.frame_selected",
        "Frame Selected",
        "View",
        "frame-sel",
        "focus zoom",
    );
    add(
        Cmd::FrameAll,
        "view.frame_all",
        "Frame All",
        "View",
        "frame",
        "focus zoom scene",
    );
    for k in ViewKind::ALL {
        add(
            Cmd::Camera(k),
            &format!("view.camera_{}", k.label().to_lowercase()),
            &format!("{} View", k.label()),
            "View",
            "single",
            "camera",
        );
    }
    add(
        Cmd::FourViews,
        "view.four_views",
        "4 Views",
        "View",
        "quad",
        "quad panel",
    );
    add(
        Cmd::FrameDefault,
        "view.frame_default",
        "Frame Default (reset view)",
        "View",
        "frame",
        "camera home",
    );
    for f in ViewFilter::ALL {
        let id = format!("view.filter_{}", f.label().to_lowercase().replace(' ', "_"));
        add(
            Cmd::Filter(f),
            &id,
            &format!("Filter: Show {}", f.label()),
            "View",
            "eye",
            "hide visibility",
        );
    }
    for m in DisplayMode::ALL {
        let icon = match m {
            DisplayMode::Gouraud => "shade",
            DisplayMode::GouraudLines => "shade-lines",
            DisplayMode::Wireframe => "wire",
            DisplayMode::Quick => "quick",
            DisplayMode::QuickLines => "quick-lines",
        };
        add(
            Cmd::Display(m),
            &format!("view.display_{}", m.id()),
            m.label(),
            "View",
            icon,
            "shading display wireframe lines",
        );
    }
    add(
        Cmd::ToggleGrid,
        "view.grid",
        "Show Grid",
        "View",
        "grid",
        "floor",
    );
    add(
        Cmd::ToggleCompass,
        "view.compass",
        "Show Axis Compass",
        "View",
        "compass",
        "",
    );
    add(
        Cmd::ToggleSolo,
        "view.solo",
        "Viewport Solo",
        "View",
        "solo",
        "isolate",
    );
    for fx in ViewFx::ALL {
        add(
            Cmd::ViewFx(fx),
            &format!("view.{}", fx.id()),
            fx.label(),
            "View",
            fx_icon(fx),
            fx.keywords(),
        );
    }
    add(
        Cmd::PlayPause,
        "anim.play",
        "Play / Pause",
        "Animation",
        "play",
        "timeline",
    );
    add(
        Cmd::PlayBackward,
        "anim.play_backward",
        "Play Backwards",
        "Animation",
        "back",
        "reverse",
    );
    add(
        Cmd::NextFrame,
        "anim.next_frame",
        "Next Frame",
        "Animation",
        "next",
        "",
    );
    add(
        Cmd::PrevFrame,
        "anim.prev_frame",
        "Previous Frame",
        "Animation",
        "prev",
        "",
    );
    add(
        Cmd::ToStart,
        "anim.to_start",
        "Go to Start",
        "Animation",
        "first",
        "",
    );
    add(
        Cmd::ToEnd,
        "anim.to_end",
        "Go to End",
        "Animation",
        "last",
        "",
    );
    for w in Workspace::ALL {
        add(
            Cmd::Workspace(w),
            &format!("window.workspace_{}", w.id()),
            &format!("Workspace: {}", w.label()),
            "Window",
            "layout",
            w.keywords(),
        );
    }
    add(
        Cmd::NextWorkspace,
        "window.next_workspace",
        "Next Workspace",
        "Window",
        "layout",
        "workspace tab",
    );
    add(
        Cmd::PrevWorkspace,
        "window.prev_workspace",
        "Previous Workspace",
        "Window",
        "layout",
        "workspace tab",
    );
    add(
        Cmd::ResetLayout,
        "window.reset_workspace",
        "Reset Workspace",
        "Window",
        "layout",
        "layout default panels",
    );
    for p in Panel::OPENABLE {
        add(
            Cmd::ShowPanel(p),
            &format!("window.panel_{}", p.id()),
            &format!("Panel: {}", p.title()),
            "Window",
            p.icon(),
            p.keywords(),
        );
    }
    add(
        Cmd::ToggleTimeline,
        "window.timeline",
        "Timeline",
        "Window",
        "timeline",
        "animation",
    );
    add(Cmd::Minimize, "window.minimize", "Minimize", "Window", "minimize", "hide window taskbar");
    add(Cmd::ToggleMaximize, "window.maximize", "Maximize / Restore", "Window", "maximize", "window size restore zoom");
    add(Cmd::ToggleFullscreen, "window.fullscreen", "Full Screen", "Window", "fullscreen", "full screen kiosk hide taskbar f11");
    add(
        Cmd::Commander,
        "window.commander",
        "Commander",
        "Window",
        "search",
        "search find command",
    );
    add(
        Cmd::CustomizeCommands,
        "window.customize_commands",
        "Customize Commands...",
        "Window",
        "keyboard",
        "shortcuts keys keyboard hotkeys",
    );
    add(
        Cmd::About,
        "help.about",
        "About Forma3D",
        "Help",
        "info",
        "version",
    );
    v
}

/// Commands that depend on the scene: one "Material Tag" per material.
pub fn dynamic(ed: &Editor) -> Vec<Info> {
    ed.scene
        .materials()
        .iter()
        .map(|m| Info {
            cmd: Cmd::AssignMaterial(m.id),
            id: format!("tags.material.{}", m.id),
            name: format!("Material Tag: {}", m.name),
            category: "Tags",
            icon: "material",
            keywords: "assign apply material tag",
        })
        .collect()
}

/// Built-in shortcuts. The two-key sequences follow Cinema 4D's (M~ mesh
/// tools, K~ knives, U~ commands, N~ shading).
pub fn default_shortcuts() -> Vec<(Cmd, Binding)> {
    use Shortcut as S;
    let one = Binding::single;
    let mut v = vec![
        (Cmd::New, one(S::ctrl("N"))),
        (Cmd::Display(DisplayMode::Gouraud), Binding::seq("N", "A")),
        (
            Cmd::Display(DisplayMode::GouraudLines),
            Binding::seq("N", "B"),
        ),
        (Cmd::Display(DisplayMode::Quick), Binding::seq("N", "C")),
        (
            Cmd::Display(DisplayMode::QuickLines),
            Binding::seq("N", "D"),
        ),
        (Cmd::Display(DisplayMode::Wireframe), Binding::seq("N", "H")),
        (Cmd::NextWorkspace, one(S::ctrl("PageDown"))),
        (Cmd::PrevWorkspace, one(S::ctrl("PageUp"))),
        (Cmd::Open, one(S::ctrl("O"))),
        (Cmd::Save, one(S::ctrl("S"))),
        (Cmd::Undo, one(S::ctrl("Z"))),
        (Cmd::Redo, one(S::ctrl_shift("Z"))),
        (Cmd::SaveAs, one(S::ctrl_shift("S"))),
        (Cmd::Merge, one(S::ctrl_shift("O"))),
        (Cmd::Close, one(S::ctrl("F4"))),
        (Cmd::Close, one(S::ctrl("W"))),
        (Cmd::Cut, one(S::ctrl("X"))),
        (Cmd::Copy, one(S::ctrl("C"))),
        (Cmd::Paste, one(S::ctrl("V"))),
        (Cmd::Preferences, one(S::ctrl("E"))),
        (Cmd::Redo, one(S::ctrl("Y"))),
        (Cmd::Duplicate, one(S::ctrl("D"))),
        (Cmd::Group, one(S::alt("G"))),
        (Cmd::Delete, one(S::key("Del"))),
        (Cmd::Delete, one(S::key("Backspace"))),
        (Cmd::Cancel, one(S::key("Esc"))),
        (Cmd::SelectAll, one(S::ctrl("A"))),
        (Cmd::DeselectAll, one(S::ctrl_shift("A"))),
        (Cmd::Tool(Tool::Select), one(S::key("9"))),
        (Cmd::Tool(Tool::Move), one(S::key("E"))),
        (Cmd::Tool(Tool::Rotate), one(S::key("R"))),
        (Cmd::Tool(Tool::Scale), one(S::key("T"))),
        (Cmd::Mode(EditMode::Model), one(S::key("1"))),
        (Cmd::Mode(EditMode::Points), one(S::key("2"))),
        (Cmd::Mode(EditMode::Edges), one(S::key("3"))),
        (Cmd::Mode(EditMode::Polygons), one(S::key("4"))),
        (Cmd::MakeEditable, one(S::key("C"))),
        (Cmd::MeshTool(MeshTool::Extrude), one(S::key("D"))),
        (Cmd::MeshTool(MeshTool::Extrude), Binding::seq("M", "T")),
        (Cmd::MeshTool(MeshTool::Inset), one(S::key("I"))),
        (Cmd::MeshTool(MeshTool::Inset), Binding::seq("M", "W")),
        (Cmd::MeshTool(MeshTool::LoopCut), Binding::seq("K", "L")),
        (Cmd::MeshTool(MeshTool::Bevel), one(S::key("B"))),
        (Cmd::MeshTool(MeshTool::Bevel), Binding::seq("M", "S")),
        (Cmd::MeshTool(MeshTool::Knife), one(S::key("J"))),
        (Cmd::MeshTool(MeshTool::Knife), Binding::seq("K", "K")),
        (Cmd::Bridge, one(S::shift("B"))),
        (Cmd::Bridge, Binding::seq("M", "B")),
        (Cmd::Pen(PenKind::Polygon), Binding::seq("M", "E")),
        (Cmd::Weld, one(S::key("W"))),
        (Cmd::Weld, Binding::seq("M", "Q")),
        (Cmd::Optimize, Binding::seq("U", "O")),
        (Cmd::InvertSelection, Binding::seq("U", "I")),
        (Cmd::ReverseNormals, Binding::seq("U", "R")),
        (Cmd::SubdivideSmooth, Binding::seq("U", "S")),
        (Cmd::LoopSelect, Binding::seq("U", "L")),
        (Cmd::ToggleAxis(0), one(S::key("X"))),
        (Cmd::ToggleAxis(1), one(S::key("Y"))),
        (Cmd::ToggleAxis(2), one(S::key("Z"))),
        (Cmd::FrameSelected, one(S::key("S"))),
        (Cmd::FrameAll, one(S::key("H"))),
        (Cmd::FourViews, one(S::key("F5"))),
        (Cmd::Commander, one(S::shift("C"))),
        (Cmd::ToggleAxisMode, one(S::key("L"))),
        (Cmd::ToggleSnap, one(S::shift("S"))),
        (Cmd::CustomizeCommands, one(S::shift("F12"))),
        (Cmd::PlayPause, one(S::key("F8"))),
        (Cmd::ToggleFullscreen, one(S::key("F11"))),
        (Cmd::RenderView, one(S::ctrl("R"))),
        (Cmd::RenderToViewer, one(S::shift("R"))),
        (Cmd::RenderSettings, one(S::ctrl("B"))),
        (Cmd::RenderRegion, one(S::alt("R"))),
    ];
    for (k, key) in ViewKind::ALL.into_iter().zip(["F1", "F2", "F3", "F4"]) {
        v.push((Cmd::Camera(k), one(S::key(key))));
    }
    v
}

// ---------------------------------------------------------------------------
// Search (Commander)
// ---------------------------------------------------------------------------

/// Fuzzy match score of `query` against a command (higher is better). Every
/// query word must match the name, category or keywords.
pub fn score(info: &Info, query: &str) -> Option<i32> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Some(0);
    }
    let name = info.name.to_lowercase();
    let extra = format!("{} {}", info.category.to_lowercase(), info.keywords);
    let mut total = 0;
    for word in q.split_whitespace() {
        let in_name = word_score(&name, word).map(|s| s + 20);
        let in_extra = extra
            .split_whitespace()
            .filter_map(|w| word_score(w, word))
            .max()
            .map(|s| s / 2);
        total += in_name.max(in_extra)?;
    }
    Some(total * 10 - name.len() as i32)
}

fn word_score(text: &str, word: &str) -> Option<i32> {
    if text.starts_with(word) {
        return Some(100);
    }
    if text
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| w.starts_with(word))
    {
        return Some(80);
    }
    if text.contains(word) {
        return Some(50);
    }
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
    let mut hits: Vec<(&Info, i32)> = infos
        .iter()
        .filter_map(|i| Some((i, score(i, query)?)))
        .collect();
    hits.sort_by_key(|h| std::cmp::Reverse(h.1));
    hits.into_iter().map(|(i, _)| i).collect()
}

// ---------------------------------------------------------------------------
// State and execution
// ---------------------------------------------------------------------------

/// Whether a command can run now, and its tick state if it is a toggle.
pub fn state_of(
    ed: &Editor,
    cmd: Cmd,
    workspace: Workspace,
    has_clip: bool,
    timeline: bool,
) -> (bool, Option<bool>) {
    let editable = ed.edit_target().is_some();
    let has_sel = !ed.selection.is_empty();
    match cmd {
        Cmd::Undo => (ed.history.undo_label().is_some(), None),
        Cmd::Redo => (ed.history.redo_label().is_some(), None),
        Cmd::Duplicate | Cmd::Group | Cmd::Cut | Cmd::Copy => (has_sel, None),
        Cmd::Paste => (has_clip, None),
        Cmd::Revert => (ed.file_path.is_some(), None),
        Cmd::Delete => (has_sel || (ed.mode.is_component() && editable), None),
        Cmd::MakeEditable => (ed.active().is_some(), None),
        Cmd::SelectChildren => (has_sel && !ed.mode.is_component(), None),
        Cmd::LoopSelect => (
            ed.mode == EditMode::Edges && !ed.comp.edges.is_empty(),
            None,
        ),
        Cmd::Rename | Cmd::ResetTransform | Cmd::RemoveMaterial | Cmd::AssignMaterial(_) => {
            (has_sel, None)
        }
        Cmd::ToggleEnabled => (ed.selection.iter().any(|&s| ed.scene.can_disable(s)), None),
        Cmd::ApplyMaterial => (ed.selected_material.is_some() && has_sel, None),
        Cmd::DeleteMaterial => (ed.selected_material.is_some(), None),
        Cmd::FrameSelected => (has_sel || ed.mode.is_component(), None),
        Cmd::Filter(f) => {
            let mut o = ed.view_opts;
            (true, Some(*f.flag(&mut o)))
        }
        Cmd::MeshTool(t) => (editable, Some(ed.mesh_tool == Some(t))),
        Cmd::Bridge
        | Cmd::Weld
        | Cmd::Optimize
        | Cmd::SubdivideSmooth
        | Cmd::SubdivideLinear
        | Cmd::ReverseNormals => (editable, None),
        Cmd::Mode(m) => (true, Some(ed.mode == m)),
        Cmd::Tool(t) => (
            true,
            Some(ed.tool == t && ed.mesh_tool.is_none() && ed.pen.is_none()),
        ),
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
        Cmd::AlignWorkplaneToSelection | Cmd::NewLayerFromSelection => {
            (ed.active().is_some(), None)
        }
        Cmd::AlignSelectionToWorkplane => (has_sel, None),
        Cmd::Camera(k) => (
            true,
            Some(ed.layout == ViewLayout::Single && ed.single_view == k),
        ),
        Cmd::FourViews => (true, Some(ed.layout == ViewLayout::Quad)),
        Cmd::Display(m) => (true, Some(ed.display == m)),
        Cmd::ToggleGrid => (true, Some(ed.view_opts.grid)),
        Cmd::ToggleCompass => (true, Some(ed.view_opts.compass)),
        Cmd::ViewFx(fx) => (true, Some(ed.view_opts.fx.get(fx))),
        Cmd::ToggleSolo => (true, Some(ed.view_opts.solo)),
        Cmd::ToggleTimeline => (true, Some(timeline)),
        Cmd::Minimize | Cmd::ToggleMaximize => (crate::host::in_tauri(), None),
        Cmd::PlayPause => (true, Some(ed.timeline.playing && !ed.timeline.reverse)),
        Cmd::PlayBackward => (true, Some(ed.timeline.playing && ed.timeline.reverse)),
        Cmd::Workspace(w) => (true, Some(workspace == w)),
        Cmd::EditorCamera => (true, Some(ed.look_through.is_none())),
        Cmd::LookThrough => (true, Some(ed.look_through.is_some())),
        Cmd::UseRenderCamera => (ed.active().is_some_and(|id| matches!(ed.scene.get(id).map(|o| &o.kind), Some(ObjectKind::Camera(_)))), None),
        Cmd::StopRender => (crate::render::busy(), None),
        Cmd::RenderRegion => (true, Some(crate::render::region_on())),
        _ => (true, None),
    }
}

/// The state as the interface shows it (subscribes to changes).
pub fn state(c: Cmd, ed: &Editor, app: &App) -> (bool, Option<bool>) {
    let ws = app.workspace.get();
    let clip = app.clipboard.with_value(|c| c.is_some());
    let tl = app.timeline_shown();
    state_of(ed, c, ws, clip, tl)
}

/// What the app itself must do for file, window and layout commands.
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
    Rename,
    Workspace(Workspace),
    CycleWorkspace(i32),
    ResetLayout,
    ShowPanel(Panel),
    ToggleTimeline,
    Minimize,
    ToggleMaximize,
    ToggleFullscreen,
    Commander,
    CustomizeCommands,
    About,
    Render(crate::render::Target),
    RenderRegion,
    StopRender,
    RenderSettings,
    AddToQueue,
}

/// Runs a command on the editor alone. Returns what the app must do for
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
        Cmd::Rename => return Some(AppAction::Rename),
        Cmd::Workspace(w) => return Some(AppAction::Workspace(w)),
        Cmd::NextWorkspace => return Some(AppAction::CycleWorkspace(1)),
        Cmd::PrevWorkspace => return Some(AppAction::CycleWorkspace(-1)),
        Cmd::ShowPanel(p) => return Some(AppAction::ShowPanel(p)),
        Cmd::ResetLayout => return Some(AppAction::ResetLayout),
        Cmd::ToggleTimeline => return Some(AppAction::ToggleTimeline),
        Cmd::Minimize => return Some(AppAction::Minimize),
        Cmd::ToggleMaximize => return Some(AppAction::ToggleMaximize),
        Cmd::ToggleFullscreen => return Some(AppAction::ToggleFullscreen),
        Cmd::Commander => return Some(AppAction::Commander),
        Cmd::CustomizeCommands => return Some(AppAction::CustomizeCommands),
        Cmd::About => return Some(AppAction::About),
        Cmd::ToggleAxisMode => {
            ed.axis_mode = !ed.axis_mode;
            ed.status = if ed.axis_mode {
                "Axis mode on: Move and Rotate change the axis, not the geometry"
            } else {
                "Axis mode off"
            }
            .to_owned();
        }
        Cmd::CenterAxis => ed.center_axis(),
        Cmd::ToggleTweak => {
            ed.tweak = !ed.tweak;
            ed.status = if ed.tweak {
                "Tweak mode on: drag an object (or a point, edge, polygon) to move it directly"
            } else {
                "Tweak mode off"
            }
            .to_owned();
        }
        Cmd::ToggleSnap => {
            ed.snap.enabled = !ed.snap.enabled;
            ed.status = if ed.snap.enabled {
                "Snapping on"
            } else {
                "Snapping off"
            }
            .to_owned();
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
                format!(
                    "Quantizing on: {} cm, {}°, {} %",
                    q.move_step, q.rotate_step, q.scale_step
                )
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
            let m = ed.new_material();
            ed.selected_material = Some(m);
            if !ed.selection.is_empty() {
                let sel = ed.selection.clone();
                ed.assign_material(&sel, Some(m));
            }
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
            ed.coord = if ed.coord == CoordSystem::World {
                CoordSystem::Object
            } else {
                CoordSystem::World
            };
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
        Cmd::FrameDefault => {
            let v = ed.active_view;
            ed.cameras[v.index()] = forma3d_core::camera::Camera::new(v);
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
        Cmd::TargetLight => {
            ed.create_target_light();
        }
        Cmd::Sky(k) => {
            ed.create_sky(k);
        }
        Cmd::SceneCamera(k) => {
            ed.create_camera(k);
        }
        Cmd::LookThrough => {
            let cam = ed.active().filter(|id| matches!(ed.scene.get(*id).map(|o| &o.kind), Some(ObjectKind::Camera(_)))).or(ed.scene.render_camera);
            match cam {
                Some(id) => {
                    ed.look_through = Some(id);
                    ed.layout = ViewLayout::Single;
                    ed.single_view = ViewKind::Perspective;
                    ed.active_view = ViewKind::Perspective;
                    let name = ed.scene.get(id).map(|o| o.name.clone()).unwrap_or_default();
                    ed.status = format!("Looking through {name}: navigating moves the camera");
                }
                None => ed.status = "Look Through: select a camera first".to_owned(),
            }
        }
        Cmd::EditorCamera => {
            ed.look_through = None;
            ed.status = "Editor camera".to_owned();
        }
        Cmd::UseRenderCamera => {
            let cam = ed.active().filter(|id| matches!(ed.scene.get(*id).map(|o| &o.kind), Some(ObjectKind::Camera(_))));
            ed.scene.render_camera = cam;
            ed.status = match cam.and_then(|c| ed.scene.get(c)) {
                Some(o) => format!("Renders now look through {}", o.name),
                None => "Renders now look through the active view".to_owned(),
            };
            ed.label("Set Render Camera");
        }
        Cmd::FocusOnSelection => {
            let cam = ed.look_through.or(ed.scene.render_camera);
            let target = ed.selection.iter().copied().find(|s| Some(*s) != cam);
            match (cam, target) {
                (Some(c), Some(t)) => {
                    let eye = forma3d_core::scene_camera::pose(&ed.scene, c, ed.time()).map(|p| (p.eye, p.forward));
                    let at = ed.scene.world_position(t);
                    if let (Some((eye, fwd)), Some(ObjectKind::Camera(sc))) = (eye, ed.scene.get_mut(c).map(|o| &mut o.kind)) {
                        sc.focus_distance = (at - eye).dot(fwd).max(1.0);
                        sc.depth_of_field = true;
                    }
                    ed.label("Focus on Selection");
                    ed.status = "Focus distance set to the selection".to_owned();
                }
                _ => ed.status = "Focus on Selection: look through or render with a camera, and select what should be sharp".to_owned(),
            }
        }
        Cmd::RenderView => return Some(AppAction::Render(crate::render::Target::View)),
        Cmd::RenderToViewer => return Some(AppAction::Render(crate::render::Target::Viewer)),
        Cmd::RenderRegion => return Some(AppAction::RenderRegion),
        Cmd::StopRender => return Some(AppAction::StopRender),
        Cmd::RenderSettings => return Some(AppAction::RenderSettings),
        Cmd::AddToQueue => return Some(AppAction::AddToQueue),
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
                ed.status = format!(
                    "{}: select a polygon object first (C makes the selected object editable)",
                    t.label()
                );
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

/// Commands that only change what the views show: no undo step.
fn view_only(c: Cmd) -> bool {
    matches!(
        c,
        Cmd::FrameAll
            | Cmd::FrameSelected
            | Cmd::FrameDefault
            | Cmd::Camera(_)
            | Cmd::FourViews
            | Cmd::Filter(_)
            | Cmd::Display(_)
            | Cmd::ToggleGrid
            | Cmd::ToggleCompass
            | Cmd::ToggleSolo
            | Cmd::ViewFx(_)
    )
}

pub fn run(app: App, c: Cmd) {
    exec(app, c, false);
}

/// Runs a command the user confirmed in the discard dialog.
pub fn run_confirmed(app: App, c: Cmd) {
    app.ask.set(None);
    exec(app, c, true);
}

fn exec(app: App, c: Cmd, confirmed: bool) {
    app.menu.set(None);
    let (enabled, _) = app.peek(|ed| {
        state_of(
            ed,
            c,
            app.workspace.get_untracked(),
            app.clipboard.with_value(|c| c.is_some()),
            app.timeline_shown_untracked(),
        )
    });
    if !enabled {
        return;
    }
    if !matches!(c, Cmd::Commander | Cmd::Cancel) {
        app.recent.update(|r| {
            r.retain(|x| *x != c);
            r.insert(0, c);
            r.truncate(8);
        });
    }
    let action = if view_only(c) {
        app.view(|e| execute(e, c));
        app.rev.update(|r| *r += 1);
        None
    } else {
        app.act(|e| execute(e, c))
    };
    let Some(action) = action else { return };
    match action {
        AppAction::New => {
            let ed = app.blank_project();
            app.add_project(ed);
            app.status(format!("New project ({} open)", app.project_count()));
        }
        AppAction::Open => open(app),
        AppAction::Close => guard(
            app,
            c,
            confirmed,
            "Close",
            app.peek(|e| e.is_dirty()),
            move || app.close_project(),
        ),
        AppAction::Revert => guard(
            app,
            c,
            confirmed,
            "Revert",
            app.peek(|e| e.is_dirty()),
            move || {
                app.act(|e| {
                    if let Err(err) = e.revert() {
                        e.status = format!("Could not revert: {err}");
                    }
                })
            },
        ),
        AppAction::Quit => {
            let unsaved = app.unsaved_projects();
            // Quitting with unsaved work always asks, whatever Preferences say.
            if !confirmed && unsaved > 0 {
                ask(
                    app,
                    c,
                    "Quit: discard changes?",
                    if unsaved > 1 {
                        format!("{unsaved} open projects have changes that have not been saved.")
                    } else {
                        format!(
                            "\"{}\" has changes that have not been saved.",
                            app.peek(|e| e.project_name())
                        )
                    },
                );
            } else {
                host::quit();
            }
        }
        AppAction::Merge => merge(app),
        AppAction::Save => save(app, false),
        AppAction::SaveAs => save(app, true),
        AppAction::Cut => {
            let clip = app.act(|e| e.cut_selection());
            if let Some(clip) = clip {
                host::copy_text(&clip.to_json().unwrap_or_default());
                app.clipboard.set_value(Some(clip));
            }
        }
        AppAction::Copy => {
            let clip = app.ed.try_update_value(|e| e.copy_selection()).flatten();
            if let Some(clip) = clip {
                let n = clip.roots().len();
                host::copy_text(&clip.to_json().unwrap_or_default());
                app.clipboard.set_value(Some(clip));
                app.status(format!("Copied {n} object(s)"));
            }
        }
        AppAction::Paste => match app.clipboard.get_value() {
            Some(clip) => app.act(|e| {
                let ids = e.paste(&clip);
                e.status = format!("Pasted {} object(s)", ids.len());
            }),
            None => app.status("Nothing to paste: copy some objects first"),
        },
        AppAction::Preferences => app.prefs_open.set(true),
        AppAction::Rename => {
            let a = app.peek(|e| e.active());
            app.renaming.set(a);
        }
        AppAction::Workspace(w) => app.switch_workspace(w),
        AppAction::CycleWorkspace(step) => {
            let all = Workspace::ALL;
            let i = all
                .iter()
                .position(|w| *w == app.workspace.get_untracked())
                .unwrap_or(0) as i32;
            app.switch_workspace(all[(i + step).rem_euclid(all.len() as i32) as usize]);
        }
        AppAction::ResetLayout => app.reset_workspace(),
        AppAction::ShowPanel(p) => app.show_panel(p),
        AppAction::ToggleTimeline => app.toggle_timeline(),
        AppAction::Minimize => crate::host::window_call("minimize"),
        AppAction::ToggleMaximize => crate::host::window_call("toggleMaximize"),
        AppAction::ToggleFullscreen => leptos::task::spawn_local(crate::host::toggle_fullscreen()),
        AppAction::Commander => app.commander.set(true),
        AppAction::CustomizeCommands => app.customize_open.set(true),
        AppAction::About => app.about.set(true),
        AppAction::Render(t) => crate::render::start(app, t),
        AppAction::RenderRegion => crate::render::toggle_region(app),
        AppAction::StopRender => crate::render::stop(app),
        AppAction::RenderSettings => crate::render::open_settings(app),
        AppAction::AddToQueue => crate::render::add_to_queue(app),
    }
}

fn ask(app: App, c: Cmd, title: &str, text: String) {
    app.ask.set(Some(Ask {
        title: title.to_owned(),
        text,
        ok: "Discard Changes".to_owned(),
        then: c,
    }));
}

/// Asks before throwing away unsaved changes (when Preferences say to).
fn guard(app: App, c: Cmd, confirmed: bool, title: &str, at_risk: bool, then: impl FnOnce()) {
    if !confirmed && at_risk && app.prefs.with_untracked(|p| p.confirm_discard) {
        let name = app.peek(|e| e.project_name());
        let text = if c == Cmd::Revert {
            format!("Revert \"{name}\" to its saved version? Your changes since then will be lost.")
        } else {
            format!("\"{name}\" has unsaved changes. They will be lost.")
        };
        ask(app, c, &format!("{title}: discard changes?"), text);
        return;
    }
    then();
}

fn open(app: App) {
    leptos::task::spawn_local(async move {
        let picked = if host::in_tauri() {
            host::open_scene().await
        } else {
            host::upload().await
        };
        match picked {
            Ok(Some(f)) => {
                let mut ed = app.blank_project();
                match ed.open_json(&f.text, f.path.clone().into()) {
                    Ok(()) => {
                        ed.frame_all(ed.active_view);
                        ed.status = format!("Opened {}", f.path);
                        // Replaces an untouched Untitled project; otherwise opens beside it.
                        if app.peek(|e| e.is_pristine()) {
                            app.replace_project(ed);
                        } else {
                            app.add_project(ed);
                        }
                        app.log_line(format!("Opened {}", f.path), false);
                    }
                    Err(err) => app.error(format!("Could not open {}: {err}", f.path)),
                }
            }
            Ok(None) => {}
            Err(e) => app.error(format!("Could not open: {e}")),
        }
    });
}

fn merge(app: App) {
    leptos::task::spawn_local(async move {
        let picked = if host::in_tauri() {
            host::open_scene().await
        } else {
            host::upload().await
        };
        match picked {
            Ok(Some(f)) => match forma3d_core::scene::Scene::from_json(&f.text) {
                Ok(other) => app.act(|e| {
                    let ids = e.scene.merge(&other);
                    e.selection = ids.clone();
                    e.label("Merge Project");
                    e.status = format!("Merged {} object(s) from {}", ids.len(), f.path);
                }),
                Err(err) => app.error(format!("Could not merge {}: {err}", f.path)),
            },
            Ok(None) => {}
            Err(e) => app.error(format!("Could not merge: {e}")),
        }
    });
}

fn save(app: App, save_as: bool) {
    leptos::task::spawn_local(async move {
        let (path, text, name) = app.peek(|e| {
            let path = (!save_as)
                .then(|| e.file_path.as_ref().map(|p| p.display().to_string()))
                .flatten();
            let name = e
                .file_path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled.forma3d.json".to_owned());
            (path, e.scene.to_json().unwrap_or_default(), name)
        });
        if host::in_tauri() {
            match host::save_scene(path, &name, text).await {
                Ok(Some(written)) => app.act(|e| {
                    if let Err(err) = e.save_json(written.clone().into()) {
                        e.status = format!("Could not save: {err}");
                    }
                }),
                Ok(None) => {}
                Err(e) => app.error(format!("Could not save: {e}")),
            }
        } else {
            match host::download(&name, &text) {
                Ok(()) => app.act(|e| {
                    let _ = e.save_json(name.clone().into());
                }),
                Err(e) => app.error(format!("Could not save: {e}")),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use forma3d_core::scene::Scene;
    use std::collections::HashSet;

    #[test]
    fn ids_and_commands_are_unique() {
        let infos = all();
        let ids: HashSet<&str> = infos.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids.len(), infos.len(), "duplicate command id");
        let cmds: HashSet<Cmd> = infos.iter().map(|i| i.cmd).collect();
        assert_eq!(cmds.len(), infos.len(), "a command is listed twice");
    }

    #[test]
    fn search_ranks_sensibly() {
        let infos = all();
        let first = |q: &str| search(&infos, q).first().map(|i| i.cmd);
        assert_eq!(first("cube"), Some(Cmd::Primitive(PrimitiveKind::Cube)));
        assert_eq!(first("extr"), Some(Cmd::MeshTool(MeshTool::Extrude)));
        assert_eq!(
            first("sds"),
            Some(Cmd::Generator(GeneratorKind::Subdivision))
        );
        assert_eq!(first("brdg"), Some(Cmd::Bridge));
        assert!(search(&infos, "zzzz").is_empty());
    }

    #[test]
    fn every_command_runs_on_the_engine() {
        for scene in [Scene::new(), Scene::demo()] {
            let mut e = Editor::new(scene);
            let mut cmds: Vec<Cmd> = all().into_iter().map(|i| i.cmd).collect();
            cmds.extend(dynamic(&e).into_iter().map(|i| i.cmd));
            for c in cmds {
                execute(&mut e, c);
            }
        }
    }

    #[test]
    fn component_commands() {
        let mut ed = Editor::new(Scene::new());
        execute(&mut ed, Cmd::Primitive(PrimitiveKind::Cube));
        execute(&mut ed, Cmd::MakeEditable);
        execute(&mut ed, Cmd::Mode(EditMode::Polygons));
        ed.prune_selection();
        execute(&mut ed, Cmd::SelectAll);
        assert_eq!(
            ed.comp.polys.len(),
            6,
            "Select All acts on components in component modes"
        );
        assert!(
            state_of(
                &ed,
                Cmd::MeshTool(MeshTool::Extrude),
                Workspace::Model,
                false,
                true
            )
            .0
        );
        execute(&mut ed, Cmd::Cancel);
        assert!(ed.comp.polys.is_empty());
    }
}
