# Forma3D

A 3D modelling app in Rust with a Cinema 4D-style workflow: Object / Attribute / Material / Coordinates
managers, a 3D viewport with move / rotate / scale gizmos, primitives, generators, deformers, lights, booleans,
editable meshes, and undo and redo.

This is original code. It does not use any Blender source, and it uses no Cinema 4D code, icons or branding.

## Project layout

| Folder | What it is |
|---|---|
| `core/` | **forma3d-core**: the engine. Scene, meshes and their tools, editor state with undo, and the wgpu viewport renderer. No interface code. |
| `app/` | **forma3d-app**: the interface, written with [Leptos](https://leptos.dev) and compiled to WebAssembly together with the engine. The 3D view draws through WebGL 2 (add `?webgpu` to the address to try WebGPU). |
| `src-tauri/` | **forma3d**: the [Tauri](https://tauri.app) desktop shell. It opens the window and does what a web page can't: native Open / Save dialogs and file access. |
| `classic/` | **forma3d-classic**: the original egui interface. It still has everything the new interface hasn't caught up with yet (see below). |

## Run

The desktop app (Tauri + Leptos) needs the WebAssembly target, [Trunk](https://trunkrs.dev) and the Tauri CLI:

    rustup target add wasm32-unknown-unknown
    cargo install trunk
    npx @tauri-apps/cli@2 dev        # live-reloading window
    npx @tauri-apps/cli@2 build      # target/release/forma3d.exe and an installer

The interface also runs in a browser with WebGPU or WebGL 2 (Chrome, Edge, Firefox): `cd app && trunk serve`,
then open http://localhost:1420. There, Open and Save use the browser's upload and download.

The classic egui app:

    cargo run --release -p forma3d-classic

Tests (engine and classic interface):

    cargo test        # 224 unit tests (scene, camera, gizmos, picking, history, generators, deformers, booleans, mesh editing, ...)

## Windows .exe

Every push to `main` is built and tested on GitHub's Windows machines (`.github/workflows/windows.yml`) and
published as the repository's latest release:

- https://github.com/Scottrh30/forma3d/releases/latest/download/Forma3D.exe: the app. It runs as is; it
  uses Microsoft Edge WebView2, which Windows 10 and 11 include.
- https://github.com/Scottrh30/forma3d/releases/latest/download/Forma3D-setup.exe: installs it (and
  WebView2, if a PC lacks it) with a Start menu entry.
- https://github.com/Scottrh30/forma3d/releases/latest/download/Forma3D-classic.exe: the egui interface.

A second job builds the web version, clicks through it in Chromium and publishes the screenshots and browser
console as the "preview" release, so the interface can be checked without Windows.

On a Windows PC without GitHub: install Rust, then run `build-windows.bat` to build the classic app.

## The Tauri + Leptos interface

The layout follows the classic one: menus in a dark custom title bar (with the command search, Shift+C), the
toolbar, a mode palette on the left, the 3D view with its timeline, the Object Manager and Attribute Manager on
the right, and Materials / Asset Browser / Console beside Coordinates at the bottom.

- **3D view:** right-drag (or Alt+left-drag) orbits, middle-drag pans, Alt+right-drag and the wheel zoom.
  Click to select (Shift or Ctrl to add); with Move, Rotate or Scale, drag a gizmo handle, or anywhere to move
  freely. Camera views (F1-F4), Frame All (H) and Frame Selected (S), five shading modes and the grid are in the
  view's bar and menus.
- **Menus:** File (New, Open, Save, Save As, Revert, Merge, Quit), Edit (Undo, Redo, Cut, Copy, Paste, Delete,
  Duplicate, Group), Create (11 shapes, Null, generators, deformers, materials), Light (7 kinds), Modes, Select,
  Tools, View, Help. Closing, New and Open ask before discarding unsaved changes.
- **Object Manager:** the hierarchy with expand / collapse, click and Shift / Ctrl selection, double-click to
  rename, drag onto an object to parent it (or onto empty space to unparent), editor / renderer visibility dots,
  the on / off check, and material tags. Drag a material from the Materials panel onto an object to assign it.
- **Attribute Manager:** Basic (name, visibility, enabled, material), Coord. (position, scale, rotation) and
  Object, which shows every setting of the shape, generator, deformer or light. Drag a field's letter to scrub
  its value (Shift for fine steps), or type a number.
- **Materials:** new, rename (double-click), recolour, assign to the selection, delete. **Asset Browser:** the
  built-in objects and materials, with search. **Console:** the log, and a line that runs commands by name.
  **Coordinates:** world position, bounding-box size and rotation of the active object.
- Most shortcuts match the classic app: Ctrl+N / O / S / Shift+S, Ctrl+Z / Y, Ctrl+X / C / V, Del, Ctrl+D,
  Alt+G, Ctrl+A, E / R / T / 9 for the tools, X / Y / Z axis locks, W world / object, C Make Editable.

Not in the new interface yet (use Forma3D-classic.exe meanwhile): point, edge and polygon editing and the mesh
tools, the Spline and Polygon Pens, snapping, quantizing, the workplane and Axis modes, layers, takes, the
Structure Manager, workspaces and dockable panels, four-view layout, multiple open projects, preferences and
shortcut customization, and the viewport Options effects menu.

## What the engine and classic interface do

- A Cinema 4D-style interface (Forma3D's own icons, no Cinema 4D branding):
  - Menu bar (File, Edit, Create, Modes, Select, Tools, Mesh, Window, Help) with icons and shortcuts.
  - **Workspace** tabs at the top right switch the whole window between jobs. Each workspace
    remembers how you rearrange it (and whether its timeline shows), across launches, in `workspaces.json`:
    - **Model**: viewport, Object and Attribute Managers, Materials and Coordinates.
    - **Track**: viewport and timeline above a **Footage** panel for camera tracking.
    - **UV Edit**: the **UV Editor** (the 0-1 UV square) beside the viewport.
    - **Animate**: viewport with the timeline, Coordinates and the managers.
    - **Sculpt**: a large viewport with the **Brushes** panel.
    - **Custom**: starts like Model; arrange it however you like.
    Ctrl+Page Down / Page Up step through them; Window > Workspace lists them too. Panels close with their
    ×, and Window > Panels opens any panel in the current workspace. Window > Reset Workspace restores
    the current workspace's default arrangement.
  - **Command palette**: Undo/Redo, Select/Move/Scale/Rotate, X/Y/Z locks, coordinate system, then object
    groups. Groups with a corner triangle are **flyouts**: click runs the last used command, hold (or
    right-click, or click the triangle) opens the whole group. Viewport Solo at the end.
  - **Mode palette** on the left: Make Editable, Model / Points / Edges / Polygons, and the mesh tools in
    component modes.
  - **Viewport**: each view has its own View / Cameras / Display / Shading / Options / Filter / Panel menus and
    drag-to-navigate icons (drag the cross to pan, the arrows to zoom, the circle to orbit; the last icon
    toggles this view / four views). Gradient background, optional grid and axis compass, object-type filters.
  - **Icon bar** above the viewport: a row of command buttons you arrange yourself. Drag an icon along
    the bar to move it (an orange line shows where it lands), drag it off the bar to remove it, and drag
    new ones in from the **Icon Library** (the + at the end of the bar, or Window > Icon Library), or
    click one there to add it at the end. Any command can go on the bar; those without an icon show their
    initials. Right-click for Remove, Add Separator, Reset Icon Bar and Hide Icon Bar
    (Window > Viewport Icon Bar brings it back). The layout is saved to `icon_bar.json` next to
    `keymap.json`. The **Shapes** button at the front is a dropdown: click it for the list of parametric
    shapes (it shows the last one you made). The default bar has the Shapes dropdown, Spline Pen, Polygon Pen and
    Subdivision Surface, then framing, display modes, the render options, grid / solo / 4 views, Make Editable,
    Duplicate and Delete.
  - **Viewport Options** (each view's Options menu, also in the Commander). All are off by default and
    apply to every view:
    - **Post Effects**: FXAA anti-aliasing, a gentle contrast curve and saturation, and a vignette.
    - **Tessellation**: Phong tessellation at display time (each triangle split into 9) so curved
      objects get rounder silhouettes. Flat faces stay flat and hard edges stay sharp and crack-free.
      The objects themselves are not changed.
    - **Shadows**: a 2048-pixel shadow map from the key light (which follows the camera), soft-edged
      (3x3 PCF), with objects shadowing each other and the floor (in Perspective and Top, when the
      grid is shown).
    - **Reflections**: a Fresnel-weighted reflection of a studio surround (sky, floor and two softboxes
      fixed in the world, so highlights slide across objects as you orbit).
    - **SSAO**: screen-space ambient occlusion (16 samples, depth-aware blur), including contact
      occlusion between objects and the floor.
    - **Backface Culling**: hides polygons facing away from the camera, which shows flipped normals.
  - **Timeline** under the viewport: frame ruler with the current-frame marker, first/current/last frame
    fields, go to start/end, previous/next frame, play backwards/forwards.
  - **Object Manager** with its own menus (Create, Edit, View, Object, Tags): selected names in orange, a
    green tick / red cross to switch generators and deformers on and off, stacked editor/render visibility
    dots, and a tags column showing material tags.
  - **Attribute Manager**: Mode/Edit menus, "Cube Object [Cube]" title, Basic / Coord. / Object tabs, section
    bars and dotted parameter labels; the Coord. tab uses the P / S / R grid.
  - **Coordinates Manager**: Position / Size / Rotation columns, Object (Rel) or World, Size or Scale, and
    **Apply** (or Enter) to commit, as in Cinema 4D. In component modes it shows and edits the selection's
    centre and size.
  - **Material Manager**: shaded material balls with Create / Edit menus; double-click opens a small
    Material Editor.
  - Panels dock: drag manager tabs to rearrange; Window > Reset Workspace restores the workspace's default.
- **Commands and shortcuts**: every menu entry, palette button and key goes through one command
  registry (`src/ui/commands.rs`), so a command has the same name, icon and shortcut everywhere.
  - **Commander** (`Shift+C`, or "Search commands" at the top right): type part of a name ("sub",
    "extr", "brdg"), Up/Down to choose, Enter to run. Recent commands come first; commands that can't
    run right now are greyed out.
  - **Customize Commands** (`Shift+F12`, Edit or Window menu): click a shortcut and press new keys.
    Esc cancels, Backspace removes it, and a key already in use moves to the new command (the window
    says which command lost it). Reset one command or all. Shortcuts are saved to
    `~/.config/forma3d/keymap.json` (`%APPDATA%\forma3d` on Windows, `~/Library/Application Support/forma3d`
    on macOS) and only your changes are stored.
  - Shortcuts match modifiers exactly, so `B` (Bevel) and `Shift+B` (Bridge) never trigger each other.
  - **Two-key shortcuts** as in Cinema 4D: press the first key, then the second (M~S = press M, then S).
    After the first key the status bar lists what can follow; Esc or a few seconds' pause cancels.
    Defaults: `M~T` Extrude, `M~W` Extrude Inner, `M~S` Bevel, `M~B` Bridge, `M~Q` Weld, `K~L` Loop Cut,
    `K~K` Knife, `U~L` Loop Selection, `U~I` Invert Selection, `U~O` Optimize, `U~R` Reverse Normals,
    `U~S` Subdivide. The single-key shortcuts (D, I, B, J, W, Shift+B) still work; Loop Cut moved from
    K to K~L because K now starts the knife sequences.
  - In Customize Commands, press one key then Enter for a single shortcut, or two keys for a sequence. A
    key can't be both a shortcut and the start of a sequence; the window says which commands are in the way.
  - **Panel commands are in the Commander too**: Rename, Enable / Disable, Expand / Collapse All,
    Reset PSR, Loop Selection, Frame Default, the viewport filters, Apply / Delete Material, Open Material
    Editor, Remove Material Tag, and one "Material Tag: <name>" per material (assigns it to the selection).
    The panel menus use the same commands, so they show the same shortcuts.
- Viewport: perspective plus Top / Right / Front orthographic views, single or four-view layout.
  Shading menu (per view header, also View menu, Commander and `N~` keys as in Cinema 4D): **Gouraud Shading**
  (`N~A`), **Gouraud Shading (Lines)** (`N~B`), **Lines** (`N~H`), **Quick Shading** (`N~C`) and **Quick Shading
  (Lines)** (`N~D`). Quick shading lights from the camera only, with no highlight, shadows or reflections, for
  speed. The Display menu holds Grid, Axis Compass, Solo, Timeline and Icon Bar; the Options menu the render
  effects.
- Parametric shapes: Sphere, Cube, Cone, Pyramid, Tube, Cylinder, Capsule, Plane, Torus, Landscape and Figure, plus
  Null objects. Their
  parameters (sizes, radii, segments, caps) stay editable in the Attribute Manager until you Make Editable.
  Pyramid has a base size, height and segments; Tube has inner and outer radius, height, and rotation and
  height segments; Capsule has radius, overall height (never less than the diameter), and rotation and height segments.
  **Landscape** is fractal terrain: width, height, depth, segments, Roughness, Scale (hill size), Seed (another
  set of hills) and Borders at Sea Level. **Figure** is a standing human dummy (feet at the origin) with Height and
  Segments.
- **Lights** (the **Light** menu in the menu bar, also Create > Light and the Commander): **Area**, **Point**,
  **Spot**, **Omni**, **Infinite**, **Dome** and **Sun**. A light shines along its +Z axis and is drawn as its own
  outline (rectangle, bulb, cone, star, parallel arrows, wire dome, sun). New lights are placed as a key light above
  and to the left of the current view, aimed at what the camera looks at (Frame All brings them into view). As in
  Cinema 4D, once the scene has a visible light the viewport is lit by the scene's lights (up to 8 direct lights
  plus Dome ambient) instead of the default camera light. Settings: Color, Intensity, Shadows; Falloff and Radius
  (Area, Point, Spot, Omni); Inner and Outer Angle (Spot); Size X/Y (Area). Point lights fade with distance by
  default, Omni lights don't. The Dome lights everything evenly from the sky; the Sun's colour follows its height
  (white at noon, orange low down, gone below the horizon) and it brings a soft blue sky. With Options > Shadows
  on, the brightest light that casts shadows casts them. Lights have their own viewport Filter.
- **Splines** (Create > Spline > Spline Pen, the pen button on the top palette, or the icon bar): click to place
  corner points, click-drag to pull out Bézier tangents, click the first point to close, click on the curve to insert
  a point (the shape is kept), drag a point or a handle to move it (Shift breaks the handle pair), Backspace removes
  the last point, Enter or Esc finishes. Points land on the floor in Perspective and Top, on the XY plane in Front
  and the ZY plane in Right. Selecting a spline and starting the pen continues it. The Attribute Manager has Type
  (Linear / Bezier), Close Spline and Intermediate Points. Splines are picked along their line and have their own
  viewport Filter.
- Generators (non-destructive, Cinema 4D style): **Subdivision Surface** (Catmull-Clark, 0-4 levels),
  **Array** (linear offset / rotation / scale per copy, or radial), **Symmetry** (mirror across YZ, XZ or XY),
  **Boolean** (A subtract / union / intersect B; A is the first child, B the second, so select A first,
  Ctrl+click B, then create the Boolean).
  Child objects are the input and are not drawn on their own; generators can be nested and
  inherit their first input's material. Creating a generator with objects selected puts them inside it;
  otherwise drag objects onto it in the Object Manager.
- Deformers, each Limited (the effect stops at the top of its box and carries on unchanged above it) or
  Unlimited (it keeps going):
  - **Bend**: curls the object over (Strength in degrees, Angle for the direction).
  - **Twist**: rotates each slice around Y, more towards the top (Angle).
  - **Taper**: narrows the object towards the top (Strength in %, 100 % pinches to a point, negative
    widens) with Curvature (0-100 %) for a rounded profile.
  - **Shear**: slides each slice sideways (Strength in % of the box height, 100 % = 45°; Angle for the
    direction) with Curvature for a curved lean.
  As in Cinema 4D a deformer goes *under* the object it deforms; creating one with an object selected
  puts it there and fits its box to the object (Attributes > Object > Fit to Parent does it again).
  Under a Null or a generator it deforms everything inside. Deformers are shown as a purple cage that
  bends along with the geometry, and they feed into generators (e.g. Twist, then Subdivision Surface).
- Object Manager: hierarchy, rename (double-click), visibility dots, drag-and-drop reparenting
  (top / middle / bottom of a row = before / into / after), group, duplicate, delete.
- Attribute Manager (Basic / Coord. / Object tabs), Coordinates Manager (relative or world), Material Manager
  (drag a swatch onto an object in the viewport or the Object Manager).
- Gizmos: Move, Rotate, Scale with X/Y/Z lock toggles and Object / World axes.
- Undo / redo (up to 200 steps), save / open as JSON (type a path in the dialog).
- Cinema 4D conventions: left-handed Y-up, centimetres, HPB rotation angles.

## Files, projects and the clipboard

- **File menu:** New Project, Open, Close, Revert to Saved, Save, Save As, Merge Project, Quit.
- **Several projects can be open at once**, as in Cinema 4D. New Project opens an empty one alongside the others;
  Open opens the file as another project (or in place of an untouched Untitled one); the end of the **Window**
  menu lists every open project (an asterisk marks unsaved changes) and switches between them. Close closes the
  current project (closing the last leaves an empty Untitled). Quit asks first if any project is unsaved.
- **Revert to Saved** reloads the project from its file (asks first; greyed out until the project has been saved).
- **Merge Project** adds another scene file's objects and materials to the current project; materials identical to
  ones already there (same name and colour) are shared. It is one undo step.
- **Edit menu:** Cut, Copy, Paste and Delete work on objects (whole hierarchies, with their materials). Pasted
  objects keep their place in the world, go to the top level and are selected. The clipboard is shared by all open
  projects, and copied objects also go on the system clipboard as scene text, so another Forma3D window can paste
  them.
- **Preferences** (Edit menu, `Ctrl+E`), saved as `prefs.json` beside the shortcuts: Interface Scale; what opens on
  startup (starter or empty scene); Autosave every N minutes (changed projects are copied to an `autosave` folder
  beside the settings, never over your files); whether to ask before closing or reverting unsaved work; shading and
  grid for new projects; Undo Steps; a button for Customize Commands; Reset to Defaults.

## Modes, snapping and the workplane

- **Model and Object modes** both act on whole objects. In **Model** mode (`1`) scaling resizes the geometry:
  a primitive's parameters change (a cube's size, a cylinder's radius and height) and a polygon object's or
  spline's points move, so the object's own scale stays at 1; children keep their size but move with it. Shapes
  that can't be squashed that way (a sphere scaled along one axis, say) keep the scale instead. In **Object** mode
  scaling changes the object's scale, so everything under it is scaled too.
- **Enable Axis** (`L`): Move and Rotate move the object's axis (pivot) while its points and children stay where
  they are. It works on polygon objects, splines, nulls and generators; press C first to make a primitive
  editable. **Center Axis to Geometry** puts the axis in the middle of the points (or of the children).
- **Tweak Mode**: drag an object directly to move it, without grabbing a handle (with the Select tool too).
  In Points / Edges / Polygons mode the point, edge or polygon under the cursor is grabbed and moved.
- **Snapping** (`Shift+S`, Modes > Snapping, or the magnet in the left palette): while moving objects or
  components, or clicking with the Spline and Polygon Pens, what is near the cursor attracts: vertices, edges,
  edge midpoints, polygons (the surface under the cursor), object axes and grid points, each switchable, with a
  radius in pixels. 3D snaps anywhere, 2.5D drops the result onto the workplane, 2D works in the orthographic views
  and keeps the depth. A cyan ring names what was hit.
- **Quantizing** (Modes > Quantizing, or the staircase in the palette): moves, rotations and scales go in steps
  (10 cm, 10°, 10 % unless changed in the same menu), for objects and components.
- **Workplane**: a movable construction plane. Workplane Mode puts the Move and Rotate gizmos on it; Modes >
  Workplane aligns it to X, Y or Z, to the selected object, or moves the selection onto it. Once moved, the
  Perspective view shows its grid, and the pens draw on it there. It is saved with the scene and undoable.
- The view's corner shows the mode and which of Axis, Tweak, Snap and Quantize are on.

## Layers, takes and the other managers

All open from Window > Panels; the Model workspace has them as tabs.

- **Layers** (beside Attributes): make one with Layer > New Layer, or select objects and use Add to New Layer
  (also in the Object Manager's Object menu and its right-click menu, and as a field in the Basic tab). Putting an
  object on a layer puts everything under it there too. Drag objects from the Object Manager onto a layer to move
  them to it. Each layer has a colour (shown as a strip in the Object Manager) and switches: **Solo** (only soloed
  layers show), **View**, **Render**, **Manager** (list its objects in the Object Manager) and **Locked** (its objects
  can't be picked or moved in the viewport). Right-click a layer to select its objects, rename, reorder or delete it.
- **Takes** (beside Objects): variations of one scene. Take > New Take (or the New Take command) adds one and makes
  it active; while it is active, whatever you change about an object's position, rotation, scale, visibility,
  material, layer or on/off switch becomes that take's override (as with Cinema 4D's Auto Take). Click Main or
  another take to switch; the list shows each take's override count and the active take's overrides, object by
  object. Clone, rename, delete, and remove overrides from the selected objects are in the menus. Takes are saved
  with the scene.
- **Structure** (Structure Manager): the active polygon object's points (X, Y, Z) or polygons (A, B, C, D) as a
  table to type or drag values into; clicking a row's number selects that point or polygon (Shift adds, Ctrl
  removes) and switches to that mode.
- **Asset Browser**: ready-made objects (table, chair, shelf, tree, rock, snowman, stairs, column, doorway,
  pedestal, studio backdrop) and fourteen materials, with search and categories. Double-click a tile to add it; a
  material also goes onto the selected objects. Save > Selected Objects as Asset (or the Save Selection to Asset
  Browser command) and Save > Selected Material as Asset keep your own in the `assets` folder beside the settings;
  right-click to delete them.
- **Console**: every status message with its time and project, filterable; and a command line that runs any
  command by name or id (`cube`, `frame all`, `enable snap`, `view.frame_all`). `list <words>` lists matching
  commands, `help` explains, Up / Down recall earlier lines.

## Editable meshes (component modelling)

- **Make Editable** (`C`, or the Mesh menu) turns a primitive or generator into a Polygon object.
  Generators bake their inputs; deformers under the object stay live.
- **Modes** (left palette, or `1` Model, `2` Points, `3` Edges, `4` Polygons). The active Polygon
  object is edited; its selection is kept per mode.
- **Selecting:** click, Shift+click to add, Ctrl+click to remove; drag with the Select tool (`9`) for a
  box selection; double-click an edge to select its loop; `Ctrl+A` selects all.
- **Transforming:** the Move / Rotate / Scale gizmos act on the selected components (pivot at their centre,
  object or world axes, X/Y/Z locks). The Coordinates manager shows and edits the selection's centre.
- **Tools:** Extrude (`D`, drag left/right for the offset; connected polygons stay together),
  Extrude Inner (`I`, drag right for the inset), Loop Cut (`K`, hover an edge to preview, click to cut),
  Delete (`Del`: removes points / polygons, dissolves edges), Reverse Normals.
- **Bevel** (`B`, Edges mode): drag right to chamfer the selected edges. Edges meeting at a corner are
  mitred; a corner where three bevelled edges meet gets a cap.
- **Polygon Pen** (`M~E`, Mesh > Polygon Pen): click corners, then click the first one (or press Enter) to make
  a polygon. Corners snap to the target's existing points and land on its surface when a click hits it, otherwise
  on the drawing plane. A new polygon follows the winding of any neighbour it shares an edge with, or faces the
  camera. It draws into the selected polygon object, or makes a new one. Backspace drops the last corner; Esc drops
  unfinished corners, then leaves the tool.
- **Knife** (`J`): click two points; the object is cut along the plane through that line and your view
  (the whole object, or only the selected polygons in Polygons mode). Esc cancels the first click.
- **Bridge** (`Shift+B`): in Edges mode joins two open (border) edges, chains or loops with the same
  number of edges; in Polygons mode removes two polygons with the same point count and joins them
  with a tube (a hole through the object).
- **Weld** (`W`) merges the selected points (or the points of selected edges / polygons) into one at
  their centre. **Optimize** merges points closer than 0.01 cm (e.g. after bridging two halves).
- **Subdivide**: smooth (Catmull-Clark) for the whole object, or linear for the selected polygons
  (neighbours get the new edge points, so no cracks).
- Editing a Polygon object inside a Subdivision Surface shows its cage, so you can box-model and see the
  smooth result live.
- Shading: neighbouring polygons more than 60 degrees apart get a hard edge, otherwise they are smoothed.

## Controls

| Input | Action |
|---|---|
| 9 / E / R / T | Select / Move / Rotate / Scale |
| C | make editable |
| 1 / 2 / 3 / 4 | Model / Points / Edges / Polygons mode |
| L / Shift+S | Enable Axis / Enable Snap |
| D / I / K~L | Extrude / Extrude Inner / Loop Cut |
| M~ / K~ / U~ | two-key shortcuts (see Commands and shortcuts) |
| B / J | Bevel / Knife |
| Shift+B / W | Bridge / Weld |
| X / Y / Z | toggle axis for the tools |
| F1-F4, F5 | Perspective, Top, Right, Front, four views |
| S / H | frame selected / frame all |
| Alt + LMB / MMB / RMB drag | orbit / pan / zoom |
| MMB drag, RMB drag, scroll | pan, orbit, zoom |
| Ctrl + click | add to selection |
| Ctrl+Z, Ctrl+Shift+Z (or Ctrl+Y) | undo, redo |
| Ctrl+D, Alt+G, Del, Ctrl+A | duplicate, group, delete, select all |
| Ctrl+N / O / S, Ctrl+Shift+S | new project / open / save / save as |
| Ctrl+W (or Ctrl+F4), Ctrl+Shift+O | close project, merge project |
| Ctrl+X / C / V | cut / copy / paste objects |
| Ctrl+E | Preferences |
| Shift+C | Commander (search and run any command) |
| Shift+F12 | Customize Commands (rebind shortcuts) |

## Known limitations

- Not built yet from the Interface section of the command reference, because the systems they work on don't exist
  yet: UV Points / UV Polygons and Texture modes (meshes have no UV coordinates), Point Level Animation (no
  keyframes), Picture Viewer and Render Queue / Team Render (no final renderer), Node Editor and XPresso (no node
  system), Python scripting and extensions, the Particle Property Manager, and the legacy Content Browser (the Asset
  Browser replaces it).
- Asset Browser tiles are drawn from the objects' icons and colours, not rendered previews.

- Generator children can't be clicked in the viewport (select them in the Object Manager).
- Splines don't feed any generator yet (no Extrude, Lathe, Loft or Sweep), and can't be edited in Points mode;
  edit them with the Spline Pen. The Polygon Pen has no edge-drag extrude or quad-strip mode.
- Esc doesn't close the icon bar's Shapes dropdown; click anywhere else.
- Only one light casts shadows (the brightest shadow caster), and point, spot and area shadows are approximated
  with a directional shadow map. Area lights are shaded as soft point lights; the Dome has no HDR image.
- Symmetry doesn't weld the seam, so a mesh touching the mirror plane keeps a crease there.
- Subdivision has no crease / edge weighting; non-primitive input (e.g. after Symmetry) is subdivided as given.
- A deformer under a primitive deforms that primitive only, not other objects nested under it
  (put them under a Null with the deformer instead).
- Smooth bends need segments on the object: the default cube has one segment per side.
- No Bulge, FFD or other deformers beyond Bend, Twist, Taper and Shear yet.
- Boolean inputs must be closed solids (a Plane gives odd results). The result has T-junctions, so it
  isn't watertight for further subdivision, and the whole object takes A's colour (B's cut faces are
  not coloured separately). The wireframe can show a few short stray segments near the cut.
  Booleans with more than 60,000 input polygons are skipped and show A unchanged.
- Component selection only ignores back-facing polygons; a front-facing point hidden behind another
  part of the same mesh can still be picked. Selections are not part of undo.
- Extrude Inner works on each polygon separately; Extrude works on polygons only (not edges/points).
- Bevel is a single-segment chamfer (no rounded multi-segment bevels yet) and works on edges only.
  Bevel width is capped at a bit under half of the shortest edge it touches.
- Knife cuts along an infinite line across the whole object (no segment-only or visible-only cut), and
  splits a polygon only when the line crosses it exactly twice (some concave n-gons are skipped).
- Bridge chooses how loops line up by shortest total distance; it doesn't offer a manual twist control.
- Undo still snapshots the whole scene, which gets heavy with very large meshes.
- No import/export formats other than the JSON scene file.
- The timeline only moves a time cursor. There are no keyframes yet.
- "Visible in Renderer" is stored but there is no renderer.
- Viewport Reflections show an environment, not other objects (no screen-space reflections yet).
  Shadows come from the single camera-attached key light only; scene lights don't exist yet.
- Viewport Options are global, not per view, and are not saved with the scene.
- Tessellation skips meshes above 150,000 triangles.
- The gizmo acts on the active (last selected) object only.
- Dragging with a transform tool on empty space moves / rotates / scales the active object (as in Cinema 4D),
  so use click, not drag, to deselect.
- Undo stores whole-scene snapshots. That is fine for this scene size but will need to become command-based later.
- Re-parenting under a non-uniformly scaled and rotated parent keeps the world position exactly but the
  orientation only approximately.
- Open / Save As / Merge Project use a typed path (no native file dialog). Copy and paste work on objects only,
  not on points, edges or polygons.
- Interface pieces that would need missing features are left out rather than shown disabled: there is no
  render group in the command palette, no Takes / Layers / Content Browser tabs, no keyframe record buttons
  in the timeline, and the Material Editor only has the colour channel.
- Camera tracking, UV unwrapping and sculpting are not built yet. Their workspaces and panels (Footage,
  UV Editor, Brushes) are in place and say so; the controls in them are greyed out until the tools exist.
- Two-key sequences are limited to two keys. Material Tag commands change with the scene, so they can be
  run from the Commander but not given shortcuts.
- Tested by unit tests and by driving the app under a virtual display with a software GL driver, not on a real GPU.
