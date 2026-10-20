# Pointer gestures: a survey, and profiles instead of one rule

Status: **design note, study** (2026-10-20). Nothing here is decided;
section 7 lists the questions for the maintainer. It answers
[issue #47](https://github.com/quazardous/egui-pop/issues/47)
("Canvases: one rule for the mouse wheel and the middle button") and
its follow-up, which widened the scope to every pointer button, the
modifier keys, trackpads, and the keymap.

The issue starts from one inconsistency (the image viewport zooms with
the wheel, the node graph pans with it). The answer proposed here is
not one fixed rule but **profiles**: named presets a user picks
("darktable", "Adobe", "Document", "Blender / node editors",
"Trackpad"), stored as a layer of the keymap, with per-gesture
overrides on top. Helios defaults to the darktable profile, because
Helios users come from darktable.

Contents:

1. What egui-pop does today
2. How egui delivers the pointer
3. The survey: two comparison tables
4. darktable in detail
5. Conflicts and platform constraints
6. Proposed profiles, storage and overrides
7. Open questions
8. References

Conventions: "wheel" is a mouse wheel notch (`MouseWheelUnit::Line`);
"two-finger scroll" is a trackpad's smooth scroll (`Point`); "Ctrl"
means Cmd on macOS wherever egui's `Modifiers::command` applies
(section 5). Left/right are the *logical* primary/secondary buttons:
a left-handed setup swaps them in the OS and the app never sees it.

---

## 1. What egui-pop does today

Paths are relative to the repository; egui 0.36.2.

| Canvas / widget | Wheel | Shift + wheel | Ctrl + wheel / pinch | Left drag | Middle drag | Right click / drag | Double click |
|---|---|---|---|---|---|---|---|
| Image viewport (`viewport/src/widget.rs`, `Gestures`) | zooms at the pointer; stops once at 100 % | pans (egui moved the delta to x) | zooms (`zoom_delta` branch) | pans on free canvas, moves a handle on a handle | pans anywhere | nothing | fit (`DoubleClick::Fit`); resets a handle |
| Node graph (`viewport/src/graph/mod.rs`, egui `Scene`) | **pans** | pans horizontally | zooms at the pointer | rubber band (Shift/Ctrl add); moves nodes; draws wires | pans | click: menus (canvas, node, port); drag: **pans** | fits; collapses a node from its title |
| Graph minimap (`graph/minimap.rs`) | zooms (does not consume the scroll) | | | pans (any button) | pans | pans | fits |
| Image navigator (`viewport/src/navigator.rs`) | steps the zoom ladder (sign of the raw event) | | | moves the frame | | | fits |
| Library compare (`views/src/library/compare.rs`) | scrolls the parent | | zooms at the pointer | pans (any button) | pans | pans | resets |
| Library grid (`views/src/library/grid.rs`) | scrolls | | **thumbnail size** | rubber band | | | activates |
| Curve editor (`viewport/src/curve.rs`) | nothing | | | moves a point (Ctrl finer, Shift coarser); adds on the curve | | **deletes a point** | deletes a point; resets elsewhere |
| Colour wheels, histogram | nothing | | | relative drag (Ctrl finer, Shift coarser) | | | resets |
| Sliders (`core/src/slider.rs`, `slider::Gestures`) | **scrolls the panel**; steps the value only if focused or Alt held | | | relative drag (Ctrl finer, Shift coarser), any button | drags | drags | resets |
| Lists, tree, table (`views/src/select.rs`) | scrolls | | | drag and drop | | context menu (tree) | opens a branch / activates |

Also: Space + drag pans the viewport and the graph; Escape cancels a
drag; `Gestures` (viewport) and `slider::Gestures` live in context
slots meant for "input profiles", but nothing sets them, nothing
persists them, and the settings window has no input section. The
keymap (`model/src/keymap.rs`, ADR 0007) knows keys only: `Chord {
ctrl, alt, shift, meta, key }`. No code reads `MouseWheelUnit`, touch
events, pen pressure or tilt.

Inconsistencies a user meets:

- the plain wheel zooms (viewport, minimap), steps (navigator), pans
  (graph) or scrolls (library, sliders);
- the right button pans in the graph and the compare panes, deletes a
  point in the curve editor, opens a menu elsewhere, and does nothing
  on the viewport;
- some drags accept any button (minimap, compare, sliders), others one;
- the docs disagree: the graph page says Shift or Ctrl "adds", the
  code toggles; lists say Ctrl toggles and Shift extends a range;
- the viewport's "other wheel action" modifier is Shift while the rest
  of egui uses Ctrl for zoom.

## 2. How egui delivers the pointer

Read from egui 0.36.2 `input_state/mod.rs` and `wheel_state.rs`:

- `InputOptions` defaults: `zoom_modifier = COMMAND`,
  `horizontal_scroll_modifier = SHIFT`, `vertical_scroll_modifier =
  ALT`, `line_scroll_speed` 40 points native and 8 on the web,
  `scroll_zoom_speed` 1/200.
- Units are normalised early: `Line` × `line_scroll_speed`, `Page` ×
  the viewport height, `Point` as is. After that,
  `smooth_scroll_delta` no longer says whether a wheel or a trackpad
  produced it.
- With the zoom modifier held, the wheel goes into `zoom_delta` and
  `smooth_scroll_delta` stays zero. `zoom_delta()` returns the
  multi-touch pinch when there is one, else that factor: **a pinch and
  Ctrl + wheel look the same** at that level.
- Shift moves the whole delta to x; Alt forces it to y.
- `Scene` pans with `smooth_scroll_delta` and zooms with `zoom_delta`;
  its `drag_pan_buttons` default to all buttons.

Consequence: a profile cannot be implemented on top of
`smooth_scroll_delta`/`zoom_delta`. The canvases have to read the raw
`Event::MouseWheel { unit, delta, modifiers }` and `Event::Zoom`
(pinch) themselves, or the app sets `InputOptions` from the profile
(`zoom_modifier`, the two scroll modifiers) so egui's conversion agrees
with it. The first is more precise (it keeps the line/point split); the
second keeps plain egui widgets (`ScrollArea`) consistent. Probably both:
`InputOptions` follow the profile for generic scrolling, and canvases
read raw events.

On the web, browsers deliver a trackpad pinch as a wheel event with
`ctrlKey = true` (Chrome, Firefox; Safari also has its own gesture
events), so on the web a pinch and Ctrl + wheel are indistinguishable
even in raw events, except by unit (a pinch has pixel deltas, a wheel
notch usually line deltas, depending on the browser).

## 3. The survey

Sources are cited by key; the keys are in section 8. "—" means nothing
documented; "(s)" marks a secondary source (forum, blog, source code)
where official docs are silent. darktable cells cite source files, see
section 4.

### 3.1 Photo and painting applications

| Gesture | darktable | Lightroom Classic | Capture One | Photoshop | GIMP 3 | Krita (default profile) | Affinity Photo 2 | RawTherapee |
|---|---|---|---|---|---|---|---|---|
| Wheel on the image | zooms at the pointer, soft stops at fit/100 % [DT-zoom] | — (zoom is click/Space) [LR1] | zooms in steps (preference) [C1a] | scrolls vertically; zooms with "Zoom with Scroll Wheel" [PS1] | scrolls vertically [GI1] | zooms at the pointer [KR2] | scrolls vertically [AF1] | zooms at the pointer (s) [RT3] |
| Shift + wheel | on a mask: feather / hardness / compression [DT-masks] | — | — | — | scrolls horizontally [GI1] | — | scrolls horizontally [AF1] | — |
| Ctrl + wheel | zooms past the soft stops [DT-zoom]; on a mask: opacity | — | — | scrolls horizontally (s) [PS2] | zooms [GI2] | — | rotates the canvas (preference) [AF3] | resizes a colour picker (s) [RT3] |
| Alt + wheel | (colour zones: next channel) | — | scrolls the tool panel when wheel-on-slider is on [C1b] | zooms (s) [PS2] | tool opacity (Ctrl+Alt: size…) [GI3] | — | zooms [AF2] | — |
| Middle drag | — | — | — | — | pans [GI4] | pans; Ctrl: zoom; Shift: rotate [KR1] | — | — |
| Middle click | cycles 100 % → 200 % → fit at the pointer [DT-mid] | — | — | — | (GTK: paste) | — | — | — |
| Left drag on free canvas | pans [DT-drag] | pans once zoomed; hold-to-zoom [LR1] | pans with the Pan cursor [C1a] | tool | tool | tool | tool | pans with the hand tool [RT2] |
| Shift + left drag | crop: vertical only; path: Bezier one handle | Develop: scrubby zoom (s) [LR2] | — | adds to a selection; constrains [PS3] | adds (before); square (after) [GI5] | brush size [KR2] | constrains to 0/45/90° [AF1] | pans/scales the crop [RT2] |
| Ctrl + left drag | crop: horizontal only; ellipse/brush: rotates | zoom to a box (s) [LR2] | — | (Move: toggles auto-select) | subtracts (before); from centre (after) [GI5] | samples colour [KR2] | corner: scales from centre [AF1] | curves: fine (s) [RT2] |
| Alt + left drag | (ellipse: proportional feather on click) | brush: erase; crop: from centre [LR3] | — | moves a copy; subtracts a selection [PS3] | moves the outline [GI5] | — | subtracts a selection [AF1] | — |
| Space + drag | — | (Space toggles zoom) [LR1] | pans [C1a] | pans [PS3] | pans (preference) [GI6] | pans [KR1] | pans [AF4] | — |
| Double click on the image | back to the lighttable [DT-dbl] | Loupe ↔ Grid (s) [LR2] | Pan cursor: 100 % ↔ fit [C1a] | (on the Hand tool icon: fit) [PS3] | — | — | (on the View tool icon: fit) [AF2] | — |
| Right click on the image | straighten (draw the horizon) [DT-right] | context menu | floating navigator [C1a] | context menu [PS3] | image menu [GI1] | pop-up palette [KR1] | context menu [AF3] | back to the default cursor [RT2] |
| Two-finger scroll | pans [DT-tp] | — | pans (since 22, s) [C1c] | pans (macOS gestures) [PS1] | scrolls | pans (macOS, Wayland) [KR2] | pans [AF5] | — |
| Pinch | zooms [DT-tp] | — | zooms (s) [C1c] | zooms [PS1] | zooms or rotates (s) [GI7] | zooms and rotates (touch) [KR2] | zooms; two-finger double tap: 100 % [AF5] | — |
| Wheel over a slider | **changes the value** (Ctrl fine, Shift coarse); Ctrl+Alt scrolls the panel; preference swaps [DT-slider] | — | changes it if the preference is on; Alt scrolls [C1b] | steps a focused field (s) [PS4] | — | — | — | **scrolls the panel; Shift + wheel changes the value** [RT1] |
| Drag a slider | absolute; any modifier: relative, Ctrl fine, Shift coarse [DT-slider] | Shift finer (s) [LR4] | — | scrubby label: Shift ×10, Alt fine (s) [PS4] | Shift or right: fine; Ctrl: coarse [GI3] | Shift fine (relative), Ctrl snaps [KR3] | scrubby label [AF1] | hold still: fine [RT1] |
| Right click on a slider | fine-tune popup, type a value [DT-slider] | — | — | — | text edit, value selected [GI3] | text entry [KR3] | context menu | — |
| Double click on a slider | resets [DT-slider] | on the name: resets [LR3] | resets [C1d] | — | — | text entry [KR3] | — | (reset button) [RT2] |
| Thumbnail grid: Ctrl / Shift click | toggles / range [DT-lt] | toggles / range [LR3] | adds / range [C1e] | — | — | — | — | — |
| Thumbnail grid: Ctrl + wheel | thumbnails per row [DT-lt] | — | — | — | — | — | — | — |

### 3.2 Node editors, design tools and 3D

| Gesture | Blender 3D viewport | Blender node editors / 2D | Unreal Blueprint | Unity Shader Graph | ComfyUI (default "Drag Navigation") | Figma | Inkscape 1.x |
|---|---|---|---|---|---|---|---|
| Wheel | zooms [BL1] | zooms [BL2] | zooms [UE1] | zooms [UN1] | zooms ("Standard" mode: pans) [CU1] | pans vertically; zooms with "Use scroll wheel zoom" [FI1] | scrolls vertically [IN1] |
| Shift + wheel | pans vertically [BL1] | pans vertically (s) | — | — | — | pans horizontally [FI1] | scrolls horizontally [IN1] |
| Ctrl + wheel | — | pans horizontally (s); over a field: steps the value [BL5] | zooms past 1:1 [UE1] | — | — | zooms [FI1] | zooms [IN1] |
| Alt + wheel | — | — | — | — | — | — | Selector: cycles stacked objects [IN1] |
| Middle drag | orbits; Shift: pans; Ctrl: zooms [BL1] | pans; Ctrl: zooms [BL2] | — | — | — | — (feature request) | pans; Shift: zoom box; Ctrl: rotates [IN1] |
| Middle click | — | — | — | — | — | — | zooms in; Shift: out [IN1] |
| Left drag on empty canvas | select box (left-click select) [BL3] | box select; Ctrl+Alt: lasso [BL4] | marquee [UE1] | marquee [UN1] | **pans** ("Standard": selects) [CU1] | marquee [FI2] | rubber band [IN1] |
| Shift + left click / drag | extends the selection | adds [BL4] | adds [UE1] | — | adds [CU2] | toggles [FI2] | toggles / adds [IN1] |
| Ctrl + left click / drag | — | — | toggles / removes [UE1] | — | adds [CU2]; Ctrl+Shift drag: fast zoom [CU1] | deep select [FI2] | selects in groups; constrains a move [IN1] |
| Alt + left drag | (Industry Compatible: orbits) [BL6] | detaches links [BL4] | Alt+click a pin: breaks links [UE1] | **pans** [UN1] | — | duplicates (s) [FI3] | touch select [IN1] |
| Space + drag | — | — | — | (Space: create node) [UN2] | pans [CU2] | pans [FI1] | pans (preference) [IN1] |
| Right click | context menu (left-click select) [BL3] | context menu | context menu [UE1] | menu [UN1] | menu | context menu [FI2] | context menu; Ctrl: zoom in [IN1] |
| Right drag | — | Ctrl: cuts links; Ctrl+Alt: mutes [BL4] | **pans** [UE1] | — | — | pans (preference) [FI1] | Shift/Ctrl: pans [IN1] |
| Double click on canvas | — | — | goes to the node's target [UE1] | — | opens node search [CU2] | enters a child [FI2] | edits / enters a group [IN1] |
| Two-finger scroll | orbits; Shift: pans; Ctrl: zooms [BL7] | pans | — | — | pans [CU1] | pans [FI1] | scrolls |
| Pinch | — | zooms | — | — | zooms [CU1] | zooms [FI1] | zooms [IN2] |
| Wheel over a value | Ctrl + wheel steps [BL5] | same | — | — | — | — | only after a click into the field; Ctrl: larger steps [IN2] |
| Presets offered | keymaps: Blender, 2.7x, Industry Compatible; Emulate 3 Button Mouse [BL6] | same | — | Shortcuts Manager [UN2] | Navigation Mode: Standard / Drag / Custom [CU1] | two checkboxes [FI1] | one toggle (removed in 1.1) [IN3] |

### 3.3 Patterns

- **The plain wheel zooms** in darktable, Krita, Capture One,
  RawTherapee, Blender, Unreal, Unity and ComfyUI's default; **it
  scrolls** in Photoshop, Affinity, GIMP, Inkscape, Figma (document
  tools, which inherit the browser's page model). Those move zoom to a
  modifier: Ctrl (GIMP, Inkscape, Figma) or Alt (Photoshop, Affinity).
- **Almost every app has a toggle for it** (Photoshop, Affinity, Figma,
  Capture One, ComfyUI). The richer ones ship presets: Blender keymaps,
  Krita input profiles named after rival apps, ComfyUI navigation modes;
  GIMP 3 has a button × modifier table and maps every wheel × modifier
  pair to any action.
- **Space + drag pans** nearly everywhere except Blender, Unreal, Unity
  (Space opens menus there) and Lightroom (Space toggles zoom).
- **Middle drag pans** in GIMP, Krita, Inkscape, Blender (orbits in 3D)
  and egui-pop; the Adobe tools and Capture One do not document it. On
  Linux, GTK binds the middle click to paste.
- **The right button** opens a context menu almost everywhere; the
  exceptions are useful: Unreal pans with a right drag, Figma can, Krita
  opens a palette, darktable straightens the horizon.
- **A left drag on empty canvas** is a marquee in design and node tools
  and pans in photo viewers (darktable, Lightroom zoomed, ComfyUI's
  default). Shift adds almost everywhere; Ctrl toggles or subtracts; Alt
  duplicates in design tools.
- **Wheel over a value is guarded** in most apps (RawTherapee needs
  Shift, Inkscape a focus, Blender Ctrl, Capture One a preference).
  darktable is the exception that changes values on a plain wheel, with
  a preference to swap, and its users rely on it.
- **Fine and coarse**: Ctrl fine / Shift coarse (darktable, egui-pop
  today), Shift fine / Ctrl snap (Blender, Krita), Shift coarse / Alt
  fine (Photoshop scrubby labels). No consensus: it belongs to the
  profile.
- **Trackpads**: two fingers pan and a pinch zooms everywhere except
  Blender's 3D view (two fingers orbit).

## 4. darktable in detail

darktable is Helios's reference. Read from darktable master as of
2026-10-11 (commit 6271f5378; most files cited are unchanged in the
Helios fork). Line numbers drift; the function names are the stable
reference. Paths are under `src/`.

### 4.1 Mouse gestures in the shortcut system

`gui/accelerators.c` binds **mouse moves together with keys**: a
`dt_shortcut_t` (`accelerators.c:36`) holds a key and modifiers, a
`button` bit mask (left, middle, right), a `click` kind (long, double,
triple), a `move` (`dt_shortcut_move_t` in `accelerators.h:116`:
scroll, pan = horizontal wheel, horizontal / vertical / diagonal / skew
drag) with a direction, and a speed. In `dt_shortcut_dispatcher`
(`accelerators.c:4948`):

- with **no ordinary key held**, mouse events go straight to the widget
  under the pointer; Ctrl + wheel or Shift + drag alone are never
  shortcuts. Mouse shortcuts are "hold a key, then scroll, drag or
  click" (e.g. hold a key and scroll to change a slider anywhere);
- while a key is held, the wheel becomes `MOVE_SCROLL`/`MOVE_PAN` unit
  steps (`dt_gui_get_scroll_unit_deltas`), the pointer becomes a move
  every 10 px whose kind follows the angle, buttons become the bit mask
  with double/triple/long clicks; tablet buttons above 7 are a separate
  device.

**Fallbacks** give extra modifiers and buttons a default meaning for a
whole class of actions (`_action_fallbacks_value`, `accelerators.c:458`;
slider fallbacks `bauhaus.c:4516`): Ctrl ×0.1, Shift ×10, left double
click = reset, Ctrl+Shift = beyond the soft limits, right button +
vertical move = zoom the slider's range. They are off by default
(`accel/enable_fallbacks`).

**Speed**: `dt_accel_get_speed_multiplier` (`accelerators.c:5634`)
applies `darkroom/ui/scale_step_multiplier` (1),
`scale_rough_step_multiplier` (10, Shift) and
`scale_precise_step_multiplier` (0.1, Ctrl) to every wheel or drag step
on a widget, so **Ctrl is fine and Shift coarse across darktable**.

**Editing**: the visual mapping mode (`libs/tools/global_toolbox.c`,
`_main_do_event_keymap`) binds by doing: hover a widget, press a key or
key + button, or scroll while holding a key. The wheel over a widget in
that mode changes the shortcut's speed, a middle click reverses its
direction. Shortcuts are saved as text tokens (`left`, `middle`,
`right`, `double`, `long`, move names). **Conflicts** are resolved at
insertion (`_insert_shortcut`, `accelerators.c:1500`): the dialog lists
clashing shortcuts across the views the action belongs to and asks to
remove them; an identical one offers to remove or disable a default.

### 4.2 Sliders, comboboxes and the side panel (bauhaus)

- **Wheel** (`_widget_scroll`, `bauhaus/bauhaus.c:3427`): steps the
  slider (`_slider_add_step`: linear step × multiplier; log sliders
  multiply), toggles a toggle, moves a combobox to the next entry.
  Ctrl+Shift crosses the soft limits; in the outer 10 % of the slider,
  Ctrl+Shift+wheel zooms the slider's visible range.
- **Panel or value**: preference `darkroom/ui/sidebar_scroll_default`
  ("mouse wheel scrolls modules side panel by default", default
  **off**): off, the wheel changes the control under the pointer and
  **Ctrl+Alt + wheel scrolls the panel**; on, the reverse
  (`gui/gtk.c`, `_dt_gui_ignore_scroll` and `_scroll_sidebar`).
- **Left click / drag** (`_widget_button_press`, `_widget_motion`):
  a click in the bar jumps to the pointer and the drag is absolute; with
  any modifier the drag is relative and fine/coarse.
- **Left double click** resets to the default; Ctrl + double click
  reapplies the module's auto-presets.
- **Right click** opens the fine-tune popup: the further the pointer is
  from the line, the finer the drag; typing digits enters a value; the
  wheel zooms the range; a middle click resets the zoom.
- **Middle click** toggles the soft and the hard range.
- Notebook tabs: the wheel switches tab; a double click on a tab resets
  every slider on it. Module groups: the wheel cycles groups; right
  click opens the manager. Module header: left click expands (Shift
  inverts "single module"), Ctrl+click renames the instance, right click
  opens presets, a drag reorders the pipe.

### 4.3 Darkroom image

Events go first to the colour picker, then the masks, then the focused
module, and only then to the view (`views/darkroom.c`, `button_pressed`,
`mouse_moved`, `scrolled`). Holding **A** bypasses masks and modules
("force pan/zoom/rotate with mouse").

- **Wheel** zooms anchored on the pointer (`dt_dev_zoom_move(DT_ZOOM_SCROLL)`,
  anchoring in `develop/develop.c` `_dev_zoom_move`): ×1.1 per notch
  below 200 %, ×2 above; fit, 100 % and 200 % are stops; with
  `darkroom/ui/constrain_zoom` (on) the zoom stays between fit and
  100 %; **Ctrl + wheel lifts the stops and limits**. Shift and Alt have
  no zoom meaning (masks and modules see them first).
- **Left drag** on free image pans; while a mask or module drags, the
  view only auto-pans past the border.
- **Left double click** (not taken by a mask or module) **returns to
  the lighttable**.
- **Middle click** zooms at the pointer: 100 % → 200 % → fit with
  `darkroom/mouse/middle_button_cycle_zoom_to_200_percent` (on), fit ↔
  100 % otherwise; Ctrl + middle forces the 200 % step.
- **Right drag** draws a horizon line handed to the rotate and
  perspective module (`dev->proxy.rotate`); masks consume right clicks.
- **Touchpad** (`darkroom/ui/touchpad_gestures`, on;
  `dt_gui_scroll_should_pan` in `gui/gtk.c`): a smooth scroll from a
  touchpad pans instead of zooming, unless Ctrl is held; a pinch zooms
  (`GtkGestureZoom`); a touchscreen one-finger drag is a mouse move.
  Smooth deltas are attenuated (`_scroll_proxy_real`), wheel notches
  are not.
- **Pen**: pressure is read for pen devices and used by the brush mask
  (`pressure_sensitivity`, `brush_smoothing`); tilt is not used.
- No left-handed, natural-scrolling or inverted-wheel option: darktable
  leaves those to the OS, except `masks_scroll_down_increases`.
- **Navigation minimap** (`libs/navigation.c`): left or right drag
  centres the view, middle click runs the zoom cycle, the wheel zooms
  at the mapped point (Ctrl lifts the limits), touchpad pans, pinch
  zooms.

### 4.4 Masks and on-image tools

The wheel and its modifiers **change what is adjusted** (not only how
finely) on a hovered or selected shape (`develop/masks/*.c`, size step
×0.97):

| Shape | Wheel | Shift + wheel | Ctrl + wheel | Ctrl+Shift + wheel | Clicks and drags |
|---|---|---|---|---|---|
| Circle | size | feather | opacity | | drag moves; Shift(+Ctrl)+click while creating places the clone source; right click deletes |
| Ellipse | radii | feather | opacity | rotation | Alt+click proportional feather; Ctrl+drag rotates |
| Brush | size | hardness | opacity | | drag paints with pressure; Ctrl+drag rotates; Shift+click on a segment adds a node; right click deletes a node or the shape |
| Path | grows / shrinks | feather | opacity | resize around the centroid | click adds a node, Ctrl+click a sharp one; right click closes / deletes |
| Gradient | curvature | compression | opacity | | double click resets curvature; Shift+click linear ↔ sigmoid |
| Liquify | radius and strength | strength | strength direction | | Ctrl+click adds a node; Ctrl+Alt+click line ↔ curve; right click cancels |

Ctrl + click on a shape button keeps creating shapes of that kind.
Crop: Shift+drag moves vertically only, Ctrl+drag horizontally only,
Shift on an edge resizes around the centre, right click resets. Rotate
and perspective: right drag draws the horizon; Shift + left / right drag
selects / deselects lines in a box; the wheel scales the selection
radius.

### 4.5 Curves

Tone curve, base curve, RGB curve (`iop/tonecurve.c`, `rgbcurve.c`):
hovering selects the nearest node; **the wheel moves it vertically**
(0.001 × multiplier: Ctrl fine, Shift coarse); a drag moves or adds;
Ctrl+click adds at the pointer's x; a double click resets the curve; a
right click deletes a node (resets an endpoint). A + wheel zooms the
graph (RGB curve). Colour zones: Alt + wheel switches the channel tab;
in "edit by area" the wheel sets the brush radius; Ctrl + right click
sets a node to zero. RGB levels: the wheel moves the active handle.

### 4.6 Lighttable and culling

- File manager: the wheel scrolls one row (or pixels with
  `thumbtable_fractional_scrolling`); **Ctrl + wheel changes the
  thumbnails per row**; Ctrl/Cmd+click toggles, Shift+click selects a
  range, a click on empty space clears; selection happens on release;
  a double click opens the darkroom. No rubber band, nothing on middle
  or right click. Dragging thumbnails exports ids and URIs.
- Zoomable lighttable: the wheel zooms, a left drag pans, touchpad
  pans, pinch zooms.
- Filmstrip: the wheel moves one image, Shift + wheel a page, Ctrl +
  wheel resizes the bottom panel.
- Culling / preview (`dtgtk/culling.c`): the wheel goes to the next
  image; Ctrl + wheel zooms all images, Ctrl+Shift + wheel only the
  hovered one; a left drag pans all, Shift + drag only the hovered one;
  a middle click toggles 100 % (Shift: hovered only).
- Map view: the wheel zooms at the pointer; over a location shape,
  plain / Shift / Ctrl + wheel resize both / one / the other radius.

### 4.7 What darktable teaches

1. The wheel is overloaded on purpose and **follows the pointer**: over
   the image it zooms, over a shape it sizes it, over a curve node it
   moves it, over a slider it changes it, over a thumbnail grid it
   scrolls. Users expect the object under the pointer to take the wheel.
2. Modifiers on the wheel mean **fine/coarse** on values (Ctrl / Shift)
   but **another parameter** on shapes (Shift = feather, Ctrl =
   opacity). egui-pop's rule "a modifier never changes what a gesture
   adjusts, only how finely" (`gestures.rs`) does not hold there; the
   profile must allow both, per canvas kind.
3. Ctrl + wheel "lifts the limits" on zoom, and the ladder has stops at
   fit, 100 % and 200 %: the same idea as egui-pop's 100 % detent.
4. A guarded escape hatch exists for the panel: Ctrl+Alt + wheel, and a
   preference to swap the default.
5. Mouse shortcuts in the keymap are keyed (hold a key + scroll/drag);
   plain modifier + pointer gestures are hard-wired in the widgets.
   egui-pop can do better: bind modifier + pointer gestures in the
   keymap directly.

## 5. Conflicts and platform constraints

- **Linux window managers.** GNOME (mutter) uses **Super** + left drag
  to move a window, Super + middle to resize, Super + right for the
  window menu (`mouse-button-modifier`). KDE (KWin) now defaults to
  **Meta**, or Alt if chosen; it moved away from Alt partly because of
  conflicts with GIMP and Blender. Xfce still uses **Alt** + drag, and
  Alt + wheel zooms the whole desktop. So: no default binding should
  rely on Alt + drag or Super + anything; Alt + wheel is unreliable on
  Xfce. A profile that uses Alt (Adobe: Alt + wheel zooms) needs a
  warning in the editor on Linux.
- **macOS.** Cmd plays Ctrl's role; egui's `Modifiers::command` (and
  egui-pop's keymap, which maps Cmd to `ctrl`) already handles it.
  Ctrl + click is a secondary click, so "Ctrl + left click" must not be
  bound on macOS unless it means the context menu. A two-finger click or
  tap is the secondary click.
- **Browsers.** Ctrl + wheel zooms the page. A web build must take the
  wheel event with a non-passive listener and `preventDefault` before
  the browser (to verify: whether eframe's web runner already does this
  when egui wants the pointer). A trackpad pinch also arrives as Ctrl +
  wheel. Firefox's Shift + right click bypasses the page's context menu;
  on Windows a middle click starts autoscroll and on Linux pastes.
- **Trackpads.** A trackpad emits `Point` units; a wheel `Line` units.
  This split is the cheapest way to tell them apart, but it is not
  reliable: some mice (Apple Magic Mouse, high-resolution and
  free-spinning wheels on Linux with libinput, browsers that report
  pixels) emit `Point` units too. darktable uses GDK's source device
  (`GDK_SOURCE_TOUCHPAD`) instead, which egui does not expose. A
  per-device override ("this wheel is a mouse") is the escape hatch.
- **Pinch vs Ctrl + wheel.** egui merges them in `zoom_delta()`; Windows
  legacy apps and browsers deliver pinch as Ctrl + wheel. A profile
  cannot give Ctrl + wheel a different meaning from pinch on those
  platforms; the profiles below keep them compatible (Ctrl + wheel is
  some kind of zoom in every profile but Adobe's, where it scrolls
  horizontally, as in Photoshop).
- **Left-handed setups.** The OS swaps the buttons; egui receives the
  logical primary button. Nothing to do, except never naming the buttons
  "left/right" in the UI (say "primary/secondary", or follow the OS).
  Krita offers a Ctrl/Alt swap for keyboards without a right Alt.
- **Pen and touch.** egui gives touch events and a pinch, not pen
  pressure or tilt. Pen buttons arrive as mouse buttons. Out of scope
  here, but the gesture model should not assume a wheel exists: the
  Trackpad profile covers pens and touchscreens too.
- **Middle button absent** (trackpads, many laptops): every middle
  gesture needs an alternative (Space + drag, or Blender-style
  "emulate 3 buttons" with Alt + left, which hits the Linux WM).

## 6. Proposed profiles

### 6.1 The model

**Gesture actions** are commands like any other, with an argument
(the amount, the delta), grouped per canvas kind:

- view: `view.zoom` (at the pointer), `view.zoom_free` (no stops or
  limits), `view.pan`, `view.pan_x`, `view.pan_y`, `view.zoom_cycle`
  (fit → 100 % → 200 %), `view.fit`, `view.zoom_box`, `view.leave`
  (the app's "back" action: darktable's double click);
- selection: `select.replace`, `select.add`, `select.toggle`,
  `select.range`, `select.box`, `select.box_add`;
- objects: `object.move`, `object.move_constrained`, `object.duplicate`,
  `object.menu`;
- values: `value.step`, `value.step_fine`, `value.step_coarse`,
  `value.reset`, `value.edit` (typed entry / fine-tune popup),
  `value.beyond_limits`;
- handles (masks, nodes, points): `handle.size`, `handle.feather`,
  `handle.opacity`, `handle.rotate`, `handle.delete`, `handle.reset`;
- `panel.scroll`, `panel.scroll_x`.

**Pointer triggers** extend the keymap's `Chord`: the same
`ctrl/alt/shift/meta` modifiers, plus a pointer part instead of a key:

```
Wheel  WheelX  Scroll  ScrollX  Pinch          (Wheel = line units, Scroll = point units)
Primary  Secondary  Middle  Back  Forward      (+ Click, DoubleClick, Drag, LongPress)
Space+PrimaryDrag                               (a held key + a pointer gesture, as darktable)
```

e.g. `"Ctrl+Wheel"`, `"Shift+MiddleDrag"`, `"Space+PrimaryDrag"`,
`"SecondaryClick"`, `"Pinch"`. A trigger is bound **in a scope**, one
per canvas kind (`surface:canvas.image`, `canvas.graph`, `canvas.curve`,
`canvas.minimap`, `widget.value`, `widget.list`, `widget.grid`) and,
inside a canvas, on a **target**: empty canvas, a handle or node, a
port. The keymap's existing layers (default, workspace, user), `when`
clauses, resolve trace and `conflicts()` apply unchanged.

A **profile** is a named set of default-layer bindings for every scope
(as Blender's keymap presets and Krita's input profiles), plus the
`InputOptions` it implies (zoom and scroll modifiers). Choosing a
profile is one setting, `input.pointer_profile`. **Overrides** are
ordinary user-layer bindings on top, and **survive a profile change**
(ComfyUI instead flips to "Custom"; Blender copies the preset). The
shortcuts editor shows the profile name with "(2 changes)", a filter
"pointer gestures", and a "reset to profile" per binding.

Two global options sit beside the profile, because they cut across all
of them: **wheel device** (auto from units / always a mouse / always a
trackpad) and **zoom direction** (wheel up zooms in / out).

The canvases read the resolved bindings (a `Gestures` per canvas kind,
built from the keymap) instead of their own defaults; `Scene`'s input
handling is switched off for the graph and its transform driven from
the same resolution. The modifiers held change the cursor and, where a
status line exists, a hint ("drag: pan · Shift: add · Ctrl: toggle"),
using the cursor query that already carries the modifiers
([cursors.md](../cursors.md)).

### 6.2 The profiles

Cells name the action; "—" means unbound (it falls through to the
parent, e.g. the panel scrolls). In every profile: Escape cancels a
drag, two-finger scroll pans and pinch zooms on canvases (the Trackpad
profile only changes the rest), primary click / Shift / Ctrl on lists
and grids select / range / toggle (the OS convention, not a profile
matter), and a secondary click on an object opens its menu unless the
table says otherwise.

#### A. darktable (default for Helios)

Close to darktable as section 4 reads it; where darktable has no such
canvas (the node graph), it follows node editors.

| Trigger | Image viewport | Node graph | Curves | Minimap / navigator | Sliders / values | Lists / grids |
|---|---|---|---|---|---|---|
| Wheel | zoom at pointer, stops at fit/100/200 % | zoom at pointer | over a point: move it (value.step) | zoom at mapped point | value.step | scroll; grid: — |
| Shift + wheel | over a handle: feather; else pan x | pan x | coarse step | — | coarse step | scroll x |
| Ctrl + wheel | zoom free; over a handle: opacity | zoom free | fine step | zoom free | fine step | grid: thumbnails per row |
| Ctrl+Alt + wheel | panel.scroll | panel.scroll | panel.scroll | panel.scroll | panel.scroll | — |
| Alt + wheel | — | — | (colour-zones style: next channel) | — | — | — |
| Primary drag, empty | pan | box select | add a point / move | move the frame | absolute drag | rubber band |
| Shift + primary drag | handle: constrain / one handle | box add | coarse move | coarse | relative coarse | extend |
| Ctrl + primary drag | handle: rotate | box toggle | fine move | fine | relative fine | toggle |
| Space + primary drag | pan | pan | — | — | — | — |
| Middle drag | pan | pan | — | pan | — | — |
| Middle click | zoom cycle 100 → 200 → fit at pointer | fit | — | zoom cycle | soft ↔ hard range | — |
| Primary double click | view.leave (back to the browser) | fit; on a node title: collapse | reset curve; on a point: delete | fit | reset | activate |
| Secondary click | handle: delete; empty: — | menu | delete point | centre | value.edit (fine-tune, type) | menu |
| Secondary drag | tool secondary (straighten) | — | — | centre | — | — |
| Two-finger scroll | pan | pan | — | pan | panel.scroll | scroll |
| Pinch | zoom | zoom | — | zoom | — | grid: thumbnail size |

Preference "the wheel scrolls the side panel" (darktable's
`sidebar_scroll_default`) swaps `value.step` and `panel.scroll` on
widgets.

#### B. Adobe (Lightroom Classic, Photoshop, Capture One-ish)

| Trigger | Image viewport | Node graph | Curves | Minimap / navigator | Sliders / values | Lists / grids |
|---|---|---|---|---|---|---|
| Wheel | pan y ("Zoom with scroll wheel" toggles to zoom) | pan y | — | — | panel.scroll | scroll |
| Shift + wheel | pan x | pan x | — | — | panel.scroll | scroll x |
| Ctrl + wheel | pan x (Photoshop) — or zoom if pinch must equal Ctrl + wheel (open question) | zoom | — | — | panel.scroll | — |
| Alt + wheel | zoom at pointer | zoom | — | zoom | — | — |
| Primary click, empty | zoom toggle fit ↔ 100 % at pointer (Lightroom) | clear selection | add point | centre | jump | select |
| Primary drag, empty | pan when zoomed | box select | move point | move frame | absolute drag | rubber band |
| Shift + primary drag | constrain; scrubby zoom | box add | constrain | — | fine | extend |
| Ctrl + primary drag | zoom box | box toggle | — | — | — | toggle |
| Alt + primary drag | duplicate / from centre | duplicate selection | — | — | — | — |
| Space + primary drag | pan | pan | — | — | — | — |
| Space+Ctrl + click / Space+Alt + click | zoom in / zoom out | same | — | — | — | — |
| Middle drag / click | pan / — | pan / — | — | — | — | — |
| Primary double click | fit | fit | delete point | fit | on the label: reset | activate |
| Secondary click | menu | menu | menu | — | menu | menu |
| Two-finger scroll / pinch | pan / zoom | pan / zoom | — | pan / zoom | panel.scroll | scroll |

#### C. Document (GIMP, Inkscape, Figma, the web)

Everything scrolls like a page; zoom is Ctrl + wheel (the browser's
own convention, so it is also the natural web default).

| Trigger | Image viewport | Node graph | Curves | Minimap / navigator | Sliders / values | Lists / grids |
|---|---|---|---|---|---|---|
| Wheel | pan y | pan y | — | — | panel.scroll | scroll |
| Shift + wheel | pan x | pan x | — | — | panel.scroll | scroll x |
| Ctrl + wheel | zoom at pointer | zoom at pointer | — | zoom | when focused: coarse step | grid: thumbnail size |
| Primary drag, empty | tool / pan if no tool | box select | add / move | move frame | GIMP: absolute | rubber band |
| Shift + / Ctrl + primary drag | add / subtract (GIMP) | box add / toggle | coarse / fine | — | fine (Shift) / coarse (Ctrl) | extend / toggle |
| Space + primary drag | pan | pan | — | — | — | — |
| Middle drag | pan | pan | — | pan | value.edit (GIMP) | autoscroll / — |
| Shift + middle drag / Ctrl + middle drag | zoom box / zoom | zoom box / zoom | — | — | — | — |
| Primary double click | — | enter / fit | delete point | fit | — | activate |
| Secondary click | menu | menu | menu | — | value.edit | menu |
| Secondary drag | — | pan (Figma option) | — | — | fine drag (GIMP) | — |
| Two-finger scroll / pinch | pan / zoom | pan / zoom | — | pan / zoom | panel.scroll | scroll |

#### D. Blender / node editors (Blender, Unreal, Unity, ComfyUI, Houdini)

| Trigger | Image viewport | Node graph | Curves | Minimap / navigator | Sliders / values | Lists / grids |
|---|---|---|---|---|---|---|
| Wheel | zoom at pointer | zoom at pointer | — | zoom | panel.scroll | scroll |
| Shift + wheel | pan y | pan y | — | — | panel.scroll | — |
| Ctrl + wheel | pan x | pan x | step point | — | value.step | — |
| Primary drag, empty | box select / tool | box select | move point | move frame | relative drag | rubber band |
| Shift + primary drag | box add | box add | fine | — | fine | extend |
| Ctrl + primary drag | box remove (Unreal) | box remove | snap | — | snap | toggle |
| Ctrl+Alt + primary drag | lasso | lasso | — | — | — | — |
| Middle drag | pan | pan | — | pan | — | — |
| Ctrl + middle drag | zoom (scrubby) | zoom | — | — | — | — |
| Secondary click | menu | menu | menu | — | menu | menu |
| Secondary drag | — | Ctrl: cut links (Blender); plain: pan (Unreal) | — | — | — | — |
| Primary double click | fit | fit / open group | reset | fit | value.edit | activate |
| Alt + primary drag | (emulate middle, off by default) | detach links | — | — | — | — |
| Two-finger scroll / pinch | pan / zoom | pan / zoom | — | pan / zoom | panel.scroll | scroll |

Reset a value: Backspace over it (Blender), plus double click.

#### E. Trackpad (laptops, tablets, no middle button)

For devices with no wheel and no middle button. Never changes values on
scroll (accidental edits are the main complaint on trackpads).

| Trigger | Image viewport | Node graph | Curves | Minimap / navigator | Sliders / values | Lists / grids |
|---|---|---|---|---|---|---|
| Two-finger scroll | pan | pan | — | pan | panel.scroll | scroll |
| Pinch | zoom at pointer | zoom | — | zoom | — | grid: thumbnail size |
| Ctrl (Cmd) + two-finger scroll | zoom | zoom | fine step | zoom | value.step | — |
| Two-finger double tap (where the OS sends it) | zoom toggle fit ↔ 100 % | fit | — | fit | — | — |
| Primary drag, empty | pan (image) | box select | move | move frame | relative drag | rubber band |
| Space + primary drag | pan | pan | — | — | — | — |
| Shift / Ctrl + primary drag | coarse / fine on handles | box add / toggle | coarse / fine | — | coarse / fine | extend / toggle |
| Secondary click (two-finger click) | menu | menu | delete point | — | value.edit | menu |
| Primary double click | fit | fit | reset | fit | reset | activate |
| Long press | menu | menu | — | — | value.edit | menu |

### 6.3 Defaults

- **Helios**: the darktable profile, with the wheel device on "auto"
  (line units zoom, point units pan, as darktable's
  `touchpad_gestures`).
- **egui-pop's gallery** (and the library's built-in default): also the
  darktable profile for canvases. It is what issue #47 proposed (wheel
  zooms, middle pans, smooth scroll pans, pinch zooms) and what node
  editors do; only the sliders differ: the gallery keeps today's
  guarded wheel (panel scroll unless focused or a modifier is held),
  since a library default should not edit values by accident. On the
  web build, "Document" is the safer first choice if taking Ctrl +
  wheel from the browser proves unreliable (open question).
- A **profile picker** in the settings' input section ("Pointer: darktable
  / Adobe / Document / Blender / Trackpad"), with a one-line description
  and a link to the shortcuts editor filtered on pointer gestures.

### 6.4 Steps (once the questions are settled)

1. `Chord` gains a pointer part; the keymap schema version goes up;
   parsing and the editor's capture accept wheel, clicks and drags.
2. Gesture actions as commands; per-canvas `Gestures` resolved from the
   keymap and published in the context slots that exist today.
3. Canvases read raw wheel events (unit, modifiers) and `Event::Zoom`;
   the graph drives `Scene`'s transform itself; `InputOptions` follow
   the profile.
4. Profiles as default-layer documents shipped with the library;
   Helios selects darktable.
5. Cursor and status hints from the resolved bindings.
6. kittest tests per profile and canvas: line wheel, point scroll,
   pinch, middle drag, each modifier.
7. One gestures page in the docs, linked from the viewport, graph,
   slider and shortcuts pages.

## 7. Open questions

1. **Profiles at all**: is the profile set (darktable, Adobe, Document,
   Blender / node editors, Trackpad) right, too many, or missing one
   (Krita, Capture One)? Should Helios ship all five or only darktable
   plus Trackpad?
2. **Gallery default**: darktable for canvases with guarded sliders (as
   proposed), or Document on the web build only?
3. **Pointer gestures in the keymap document** (one editor, one
   conflict check, a schema bump) or in a sibling "pointer map"
   document shown in the same editor?
4. **Overrides across a profile change**: keep them (proposed) or reset
   them like ComfyUI's "Custom"?
5. **Wheel over sliders** in the darktable profile: change the value
   (darktable's default) with Ctrl+Alt + wheel for the panel, or
   egui-pop's guard (focus or a modifier)?
6. **Modifiers change what, or how finely**: darktable's masks use
   Shift + wheel for feather and Ctrl + wheel for opacity, against the
   rule in `gestures.rs`. Accept "per target" meanings in the profile?
7. **Primary double click on the image**: darktable leaves for the
   lighttable; the viewport today fits. Map it to `view.leave` in
   Helios's darktable profile (the app decides what "leave" means) or
   keep fit?
8. **Primary drag on an empty image**: pan (darktable, Lightroom) in
   every profile for the image viewport, and box select only on the
   graph?
9. **Secondary button**: context menu everywhere except where a profile
   says otherwise (darktable: slider fine-tune, straighten on the
   image; Unreal: pan)? The graph pans with a right drag today; keep it
   in the darktable profile?
10. **Fine/coarse modifiers**: Ctrl fine / Shift coarse (darktable,
    today) everywhere, or per profile (Blender and Krita: Shift fine)?
11. **Alt**: should any default use Alt (Adobe's Alt + wheel zoom), given
    Xfce and KWin-with-Alt? Warn in the editor on Linux, or avoid?
12. **Ctrl + wheel on the web**: take it from the browser (every profile
    zooms with it) or bind zoom elsewhere on the web?
13. **Device split**: is the line/point unit split enough, with a
    "wheel device" override, or do we need per-device settings (egui
    does not expose the device)?
14. **Scope of a profile**: do profiles also carry the keyboard
    navigation keys (Space, +/-, 0/1/2, arrows), like Blender's keymaps,
    or pointer gestures only?
15. **Held-key + pointer bindings** (darktable's "hold a key and scroll
    to change that slider"): in scope now, or later?

## 8. References

darktable: source files and functions cited in section 4, from
<https://github.com/darktable-org/darktable/tree/6271f5378/src>.
Short keys used in section 3.1: [DT-zoom] `views/darkroom.c` `scrolled`,
`develop/develop.c` `_dev_zoom_move`; [DT-masks] `develop/masks/*.c`;
[DT-mid] `develop/develop.c` (zoom cycle), `views/darkroom.c`
`button_pressed`; [DT-drag], [DT-dbl], [DT-right] `views/darkroom.c`
`button_pressed` / `mouse_moved`; [DT-tp] `gui/gtk.c`
`dt_gui_scroll_should_pan`, `views/darkroom.c` `gesture_pan`,
`gesture_pinch`; [DT-slider] `bauhaus/bauhaus.c` `_widget_scroll`,
`_widget_button_press`, `gui/gtk.c` `_dt_gui_ignore_scroll`;
[DT-lt] `dtgtk/thumbtable.c`.

Adobe and Capture One help pages were read through web.archive.org
snapshots (they refuse automated fetches); the URLs are the originals.

- [PS1] <https://helpx.adobe.com/photoshop/using/viewing-images.html>
- [PS2] (s) <https://community.adobe.com/t5/photoshop-ecosystem-discussions/how-can-i-change-zoom-and-scroll-modifiers-in-photoshop/td-p/14833929>
- [PS3] <https://helpx.adobe.com/photoshop/using/default-keyboard-shortcuts.html>
- [PS4] (s) <https://planetphotoshop.com/scrubby-slider-shift-click-trick.html>
- [LR1] <https://helpx.adobe.com/lightroom-classic/help/view-photos.html>
- [LR2] (s) <https://jkost.com/blog/2020/07/zooming-in-lightroom-classic.html>
- [LR3] <https://helpx.adobe.com/lightroom-classic/help/keyboard-shortcuts.html>
- [LR4] (s) <https://community.adobe.com/t5/lightroom-classic-discussions/sliders-in-the-develop-module/m-p/9359357>
- [C1a] <https://support.captureone.com/hc/en-us/articles/360002473237>
- [C1b] <https://support.captureone.com/hc/en-us/articles/360002484097>
- [C1c] (s) <https://support.captureone.com/hc/es/community/posts/4416440456721-Zoom-with-trackpad>
- [C1d] <https://www.captureone.com/blog/11-nifty-tips-and-tricks-for-capture-one>
- [C1e] <https://support.captureone.com/hc/en-us/articles/360002480877>
- [GI1] <https://docs.gimp.org/3.0/en/gimp-image-window.html>
- [GI2] <https://docs.gimp.org/3.0/en/gimp-tool-zoom.html>
- [GI3] <https://docs.gimp.org/3.0/en/gimp-tools.html>
- [GI4] <https://docs.gimp.org/3.0/en/gimp-prefs-canvas-modifiers.html>
- [GI5] <https://docs.gimp.org/3.0/en/gimp-tools-selection.html>
- [GI6] <https://docs.gimp.org/3.0/en/gimp-prefs-canvas-interaction.html>, wheel mapping: <https://docs.gimp.org/3.0/en/gimp-prefs-input-controllers.html>
- [GI7] (s) <https://gitlab.gnome.org/GNOME/gimp/-/merge_requests/401>
- [KR1] <https://docs.krita.org/en/user_manual/getting_started/navigation.html>
- [KR2] (s, source) <https://invent.kde.org/graphics/krita/-/raw/master/krita/data/input/kritadefault.profile> and the sibling profiles (Photoshop, SAI, Clip Studio compatible)
- [KR3] <https://docs.krita.org/en/reference_manual/special_widgets.html>
- [AF1] <https://affinity.help/photo2/en-US.lproj/pages/Workspace/shortcuts.html>
- [AF2] <https://affinity.help/photo2/en-US.lproj/pages/GetStarted/zoom.html>
- [AF3] <https://affinity.help/photo2/en-US.lproj/pages/Workspace/preferences.html>
- [AF4] <https://affinity.help/photo2/en-US.lproj/pages/GetStarted/pan.html>
- [AF5] <https://affinity.help/photo2/en-US.lproj/pages/Extras/trackpads.html>
- [RT1] <https://rawpedia.pixls.us/general_comments_about_some_toolbox_widgets/>
- [RT2] <https://rawpedia.pixls.us/keyboard_shortcuts/>
- [RT3] (s, source) <https://github.com/RawTherapee/RawTherapee/blob/dev/rtgui/cropwindow.cc>
- [BL1] <https://docs.blender.org/manual/en/latest/editors/3dview/navigate/navigation.html>
- [BL2] <https://docs.blender.org/manual/en/latest/interface/window_system/regions.html>
- [BL3] <https://docs.blender.org/manual/en/latest/editors/preferences/keymap.html>
- [BL4] <https://docs.blender.org/manual/en/latest/interface/controls/nodes/editing.html>
- [BL5] <https://docs.blender.org/manual/en/latest/interface/controls/buttons/fields.html>
- [BL6] <https://docs.blender.org/manual/en/latest/interface/keymap/industry_compatible.html>, <https://docs.blender.org/manual/en/latest/editors/preferences/input.html>
- [BL7] <https://docs.blender.org/manual/en/latest/getting_started/configuration/hardware.html>
- [UE1] <https://dev.epicgames.com/documentation/en-us/unreal-engine/blueprint-editor-cheat-sheet-in-unreal-engine>
- [UN1] <https://docs.unity3d.com/Packages/com.unity.shadergraph@17.1/manual/Shader-Graph-Window.html>
- [UN2] <https://docs.unity3d.com/Packages/com.unity.shadergraph@17.1/manual/Keyboard-shortcuts.html>
- [CU1] <https://docs.comfy.org/interface/settings/lite-graph>
- [CU2] <https://docs.comfy.org/interface/shortcuts>
- [FI1] <https://help.figma.com/hc/en-us/articles/1500004414582>
- [FI2] <https://help.figma.com/hc/en-us/articles/360040449873>
- [FI3] (s) <https://forum.figma.com/t/option-drag-a-locked-layer-results-in-duplicating-it/3804>
- [IN1] <https://inkscape.org/doc/keys.html>
- [IN2] <https://wiki.inkscape.org/wiki/Release_notes/1.0>
- [IN3] <https://wiki.inkscape.org/wiki/Release_notes/1.1>

Operating systems and browsers:

- macOS trackpad gestures <https://support.apple.com/en-us/102482>;
  secondary click <https://support.apple.com/guide/mac-help/right-click-mh35853/mac>
- Windows touchpad <https://support.microsoft.com/en-us/windows/touchpad-gestures-for-windows-a9d28305-4818-a5df-4e2b-e5590f850741>;
  pinch as Ctrl + wheel for legacy apps <https://learn.microsoft.com/en-us/windows/win32/wintouch/windows-touch-gestures-overview>
- GNOME `mouse-button-modifier` <https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas/-/raw/master/schemas/org.gnome.desktop.wm.preferences.gschema.xml.in>;
  left-handed mouse <https://help.gnome.org/users/gnome-help/stable/mouse-lefthanded.html.en>
- KWin window actions <https://docs.kde.org/stable_kf6/en/kwin/kcontrol/windowbehaviour/index.html>;
  the move from Alt to Meta <https://phabricator.kde.org/D20730>
- Xfce Alt + drag and Alt + wheel (s) <https://gitlab.xfce.org/xfce/xfwm4/-/issues/159>
- GTK middle-click paste <https://docs.gtk.org/gtk4/property.Settings.gtk-enable-primary-paste.html>
- Wheel events, `ctrlKey` for pinch, `deltaMode` <https://developer.mozilla.org/en-US/docs/Web/API/Element/wheel_event>,
  <https://developer.mozilla.org/en-US/docs/Web/API/WheelEvent/deltaMode>;
  passive listeners <https://developer.chrome.com/blog/scrolling-intervention-2>;
  Firefox pinch as Ctrl + wheel <https://bugzilla.mozilla.org/show_bug.cgi?id=1052253>;
  logical buttons <https://developer.mozilla.org/en-US/docs/Web/API/MouseEvent/button>
