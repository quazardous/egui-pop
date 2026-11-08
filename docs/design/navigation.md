# Navigation on canvases larger than their view: a study

Status: **accepted, under way** (2026-11-08). The maintainer chose B
for Q1 (hand mode: H, and Space held), Q2 (the profile's key plus F
everywhere), Q3 (the overview when needed), Q4 (the view kept by id)
and Q6 (a one-time hint, dismissed by the first pan or a click); Q5
stays A. Built so far, in `egui_pop::viewport::navigation` and on the
node graph: hand mode, the `view.*` commands and their keys, fit
selection, the overview's rule, the kept view and the hint. Still to
do: the image viewport and plots on the same model (hand mode, fit,
100 % and the commands at least), the zoom control on the graph.

A report started it: in Helios and in the gallery "you cannot move
around in the diagrams". The quick check (section 1) found a real fault
and fixed it; it also showed that egui-pop never asked what moving
around a large surface needs, widget by widget and device by device.
This note asks it.

**Scope.** Every widget whose surface is larger than its view:

- the node graph (`egui_pop::viewport::graph`), drawn in egui's `Scene`,
  and any diagram drawn the same way;
- the image viewport (`egui_pop::viewport`), as Helios uses it in the
  loupe and in Compare;
- plots ([plot.md](../plot.md), over egui_plot);
- canvases to come (a timeline zoomed in, a map, a mask editor).

Scroll areas (lists, pages, editors) are out of scope: the wheel and
the scrollbar settle them, and [scrolling-survey.md](../scrolling-survey.md)
covers them.

## 1. The quick check (2026-11-08)

The documented gestures ([node-graph.md](../node-graph.md), "Pan and
zoom"; [pointer-gestures.md](../pointer-gestures.md)) are the wheel,
two fingers, Ctrl and the wheel or a pinch, a middle drag and a drag
with Space held. Kittests now send each one over the gallery's node
graph page and over the editor alone, from the empty canvas and from a
node, and assert that the view moved
(`the_node_graph_page_moves_with_every_navigation_gesture` in the
gallery; `a_middle_drag_pans_the_graph_even_from_a_node`,
`a_drag_with_space_held_pans_even_from_a_node_and_leaves_it_in_place`,
`two_fingers_pan_the_graph_and_a_pinch_zooms_it_in_every_profile` in
`egui-pop-viewport/tests/pointer.rs`).

**What was broken.** The wheel, two fingers, the pinch and Ctrl and
the wheel worked. The drags did not, as soon as they started on
anything but the bare canvas: the pan read the drag of the `Scene`'s
own response, and a node, a folded stack, a group's band, a port or a
wire on top took the drag first. A middle drag over a node did
nothing; a drag with Space held over a node moved the node. Fitted, the
gallery's fifty-node overview and Helios's network leave little bare
canvas between nodes, so the drags seemed not to work at all. Without
a middle button (a laptop), Space and the drag was the only way to pan
by dragging, and it moved nodes instead.

**The fix.** The editor decides, before its widgets, whether the drag
under way pans: a button pressed on the graph (not on a window above
it), moving, that the profile pans with, or a primary drag with Space
held. While it pans, no node, stack, band or port starts its own drag,
and the view follows the pointer (the point under it stays under it).
Helios uses the same editor without a wrapper of its own: it gets the
fix with the release.

**What the check does not cover**: whether people find the gestures
(section 2.3: no app relies on the user guessing), and the image
viewport's and plots' gestures, which have their own tests in
`egui-pop-viewport/tests/pointer.rs` and `plot.md`.

## 2. Survey of established applications

Sources are listed in section 9. "Unconfirmed" marks what no source we
read states; touch and pen are almost nowhere documented (section 2.2).

### 2.1 Mouse

| App | Pan | Zoom | Left drag on empty space | Fit all / fit selection / 100 % | Keyboard | Overview |
|---|---|---|---|---|---|---|
| Blender, node editor | Middle drag; Ctrl / Shift + wheel (unconfirmed) | Wheel; Ctrl + middle drag | Box select | Home / numpad `.` (frames, does not zoom in) / — | Numpad +/- | Scrollbars; no minimap |
| Nuke, node graph | Middle drag, Alt + drag | +/- at the pointer; Alt + middle drag | Marquee (unconfirmed) | F (selection, or all when none) / F / — | Bookmarks: Ctrl+F7..F10 set, Shift+F7..F10 go; J jumps to a node | Navigator map at the bottom right, only when the script outgrows the view |
| DaVinci Fusion, node editor | Middle drag; Shift+Cmd + drag | Ctrl / Cmd + wheel | Marquee (unconfirmed) | Menu "scale to fit" (unconfirmed) / — / Ctrl+1 resets the scale | — | Node Navigator at the top right, V toggles |
| Houdini, network editor | Middle drag; Space + left drag | Wheel; Space + right drag | Box select | H / G / — | Enter and U go in and out; PgUp/PgDn along the chain; quick marks Ctrl+1..5 set, 1..5 go; Alt+arrows walk the view history | Overview in a corner, O toggles; "when needed" by default |
| Unreal, Blueprints | Right drag | Wheel; left+right drag; Ctrl to go past 1:1 | Marquee (Shift adds, Ctrl removes) | API zoom to window / Home / — | PgDn / PgUp into and out of a sub-graph | None; a zoom readout and breadcrumbs |
| ComfyUI (frontend 1.25 and later) | Standard: wheel; legacy: left drag on empty space; Space + drag in both | Standard: Ctrl + wheel; legacy: wheel; Alt+= / Alt+- | Standard: select; legacy: pan | `.` / — / a % field in the zoom popover | V select tool, H hand tool | Minimap, Alt+Shift+M; shown on wide windows |
| Figma | Space + drag, H hand tool; wheel (Shift: sideways); optional right drag | Ctrl / Cmd + wheel; optional "scroll wheel zoom"; Shift+= / Shift+- | Marquee | Shift+1 / Shift+2 / Shift+0 or Ctrl+0 (sources disagree) | — | None; a "recenter" button when far away |
| Miro | Space + drag, H; Mouse mode: right or middle drag | Mouse mode: wheel; Ctrl / Cmd and +/- | Select | Alt+1 / Alt+2 / Ctrl+0 | Arrows move items | Minimap, M toggles |
| darktable, darkroom | Drag the image; drag in the navigation module | Wheel; middle click cycles fit, 100 %, 200 % | Pans the zoomed image | Middle-click cycle; zoom combobox under the preview | — | Navigation module (Ctrl+Shift+N) |
| Lightroom Classic, Loupe | Drag when zoomed; drag in the Navigator | Ctrl / Cmd + = / -; Z and Space toggle; a click toggles | Click toggles fit and the chosen zoom; drag pans | Navigator presets Fit / Fill / 1:1 / a ratio | — | Navigator panel, always there |

### 2.2 Other devices

| Device | What the apps do |
|---|---|
| Touchpad | Two fingers pan and a pinch zooms where the app says it (Figma, ComfyUI standard, Fusion, darktable with its touchpad option, Blender's "multi-touch gestures"). The wheel-zooming node editors make a touchpad zoom when two fingers move, which users report as the main annoyance; Miro and ComfyUI add a **navigation mode** for it (Auto / Mouse / Trackpad; Standard / Legacy / Custom) and Miro's users complain that Auto guesses wrong on laptops with both. |
| Mouse without a middle button | Blender's "emulate 3 button mouse" (Alt + left = middle, at the cost of Alt + left's own uses); everywhere else Space + drag or a hand tool (H). |
| Pen | Blender lowers its drag thresholds for tablets and turns off continuous grab; nothing else is documented. Pens send a primary button, often a barrel button as middle or secondary; Space + drag and a hand tool are what pen users rely on. |
| Touch screen | Only Miro documents it (detected apart from the mouse and trackpad modes): one finger acts, two fingers pan and pinch. |

### 2.3 Discoverability

No application expects the user to guess. What they use:

- **menus that show the key**: Blender's View menu, Houdini's right
  click on the background ("viewing controls"), Unreal's;
- **a zoom control** with fit and a percentage: ComfyUI's popover,
  darktable's combobox, Lightroom's Navigator presets, Figma's zoom menu;
- **a shortcut sheet**: darktable (H held), Miro (?);
- **a way back** when lost: Figma's "recenter" button, Nuke's
  navigator appearing as soon as the script outgrows the view;
- **status bar hints**: Blender shows what each mouse button does now
  (egui-pop's hint line does the same, `pointer::show_hint`).

### 2.4 Patterns

- **Space + drag pans everywhere**: Figma, Miro, ComfyUI, Houdini,
  Photoshop. It is the one gesture that works on every device with a
  keyboard.
- **The wheel splits two families**: node and 3D tools zoom with it
  (Blender, Houdini, Unreal, Nuke, ComfyUI legacy); design tools pan with
  it and zoom with Ctrl (Figma, ComfyUI standard). Miro picks by device.
- **A left drag on empty space selects** almost everywhere (a marquee);
  only ComfyUI legacy, the hand tools and the photo viewers pan with it.
- **Two framing commands**, "all" and "selection": Home / numpad `.`,
  H / G, F (selection, else all), Shift+1 / Shift+2, Alt+1 / Alt+2. A
  100 % key belongs to photo and design tools, not to node editors.
- **An overview** that appears when needed (Nuke, Houdini) or toggles
  with one key (Houdini O, Fusion V, Miro M, ComfyUI Alt+Shift+M).
- **Zoom limits** are stated: ComfyUI 1 % to 1000 %, Lightroom 6 % to
  1600 %, darktable fit to 100 % unless Ctrl, Unreal 1:1 unless Ctrl.
- **Places**: Nuke's bookmarks and Houdini's quick marks keep views to
  come back to; Houdini also keeps a view history.

## 3. Devices onto egui-pop's profiles

egui-pop's six profiles ([pointer-gestures.md](../pointer-gestures.md))
already cover the mouse families and the trackpad:

| Device | Profile today | Fits? |
|---|---|---|
| Mouse with a middle button, node habits | Blender / node editors | Yes: wheel zooms, middle drag pans, secondary drag pans on the graph (Unreal's right drag) |
| Mouse, document habits | Document (default) | Yes: Figma's and ComfyUI standard's rules |
| Mouse, photo habits | darktable, Photoshop, Lightroom Classic | Yes on the image; on the graph each takes the nearest rule (section 6.2 of the study) |
| Mouse without a middle button | any | Space + drag only; no hand tool, no emulation |
| Touchpad | Trackpad | Yes: two fingers pan, a pinch and Ctrl + two fingers zoom, the plain wheel never zooms |
| Pen | none | A pen is a pointer with one button: today it pans only with Space held, and a pen user rarely has a hand on the keyboard |
| Touch screen | none (egui turns one finger into a primary pointer) | One finger draws a rubber band on the graph and pans the image; two fingers send a pinch (egui's `MultiTouch`) |

Gaps, and what would fill them:

- **A hand mode, not a profile.** A pen, a touch screen and a mouse
  without a middle button lack the same thing: a way to make the
  primary drag pan. Figma, Miro and ComfyUI use a hand tool (H) or
  a held key; Blender an emulation. A profile is the wrong place: the
  same person switches between a pen and a mouse in one session. The
  proposal (section 6) is a **hand mode** toggled by a key and a
  button, and a held Space for the moment.
- **A touch profile is not needed.** Under the rule "a profile named
  after an app behaves like it", there is no app to name a touch
  profile after; touch is Document plus egui's multi-touch, and one
  finger on empty canvas should pan rather than select (Miro, every
  map), which is a device fact egui gives us (`Event::Touch`), not a
  preference.
- **A pen profile is not needed** either: a pen is the primary button
  plus a hand mode; its barrel button, when the driver sends it as a
  middle button, already pans in every profile that pans with the
  middle drag.
- **A ComfyUI profile** could be named after an app and behave like
  it (legacy: the wheel zooms, a left drag on empty space pans, Ctrl
  + drag selects). Asked in section 7.

## 4. Needs per widget

| Need | Node graph | Image viewport | Plots | Future canvases |
|---|---|---|---|---|
| Reach any area | Wheel, two fingers, drags (fixed in section 1), the minimap (`GraphMinimap`, the gallery shows one); missing: a pan from the keyboard | Wheel, drags, the navigator; arrows pan (`viewport.md`) | egui_plot's drag and wheel | Same model |
| Framing | Fit all: double click, Home, `graph.view {fit}`; missing: fit selection, a key for a zoom level | Fit, fill, 100 %, 200 %, the zoom control | Double click resets the bounds | Same model |
| Jump to the selection | The view follows the focus (a node brought into focus is revealed); missing: "frame selection" | n/a (the image is the selection); a mask or a point could be | n/a | A timeline's playhead |
| Zoom limits and levels | 5 % to 150 % (text blurs above 100 %), three levels of detail | Fit to a maximum, stops at fit and 100 % | egui_plot's own | Each states its range |
| Keep the place across sessions | Not kept: the view refits on open | Helios keeps the zoom per image (`keep_image_open`); the viewport's state is the application's | Not kept | Same model |

Remarks:

- **Fit selection** is the one framing command every node editor has
  and egui-pop's graph lacks (Blender numpad `.`, Nuke F, Houdini G,
  Unreal Home, Figma Shift+2).
- **Keeping the place** matters on a graph: a refit on every open
  throws away where the user was. The view is a `Rect` in graph units,
  already in `GraphState`; keeping it is a persistence slot.
- **A pan from the keyboard** is missing on the graph: the arrows move
  the focus between nodes and the view follows, which reaches every
  node, but not the empty space around them.

## 5. Accessibility

- **Keyboard only.** Every canvas must be navigable without a pointer:
  move the focus (done on the graph), pan (Ctrl + arrows, proposed),
  zoom (Ctrl + `+` / `-` / `0`, proposed), fit all, fit selection. The
  keys are data (`GraphKeys`) and follow the keymap, so a screen reader
  user's keys can be remapped.
- **Announce the view**: after a fit or a zoom by key, announce the
  zoom and the level politely (the graph already announces its level
  as the canvas's value).
- **Reduced motion.** A fit or a frame may animate the view
  (Figma, Blender's smooth view); with reduced motion
  (`egui_pop_core::motion`) it jumps. Kinetic panning, if ever added,
  stops at once under reduced motion.
- **Targets**: the minimap and the zoom control are focusable widgets
  with names (the minimap already is, "Graph overview").

## 6. Proposal: one navigation model

One model shared by the graph, the image viewport and future canvases;
plots join where egui_plot lets them.

1. **Gestures by profile**, as today (`surface:canvas.*` scopes), with
   the rule that a panning drag pans from anywhere on the canvas (the
   section 1 fix, to apply to every canvas).
2. **Hand mode**: a toggle (H by default, and a button on the zoom
   control) that makes the primary drag pan, for a pen, a touch screen
   or a mouse without a middle button; Space held is the same for the
   moment. On a touch screen, one finger on empty canvas pans.
3. **Navigation commands**, one set for every canvas kind, in the
   keymap and in a menu: `view.fit` (all), `view.frame_selection`,
   `view.zoom_in`, `view.zoom_out`, `view.zoom_100`, `view.pan_*` (Ctrl
   + arrows). The graph's Home stays fit all; the defaults per profile
   follow the apps (Blender: Home / numpad `.`; Document: Shift+1 /
   Shift+2 as Figma; darktable: its own). Agents get them as today's
   `graph.view`.
4. **A zoom control** (the viewport crate's, reused): a percentage, fit,
   fit selection, 100 %, the hand toggle; it shows the keys in its
   tooltips. This is the discoverability path every app has.
5. **An optional overview**: the minimap (`GraphMinimap`, the image
   navigator), shown in a corner **when the content outgrows the
   view**, toggled by a key, its policy an option (when needed /
   always / never, as Houdini).
6. **A hint overlay**: the hint line (`pointer::show_hint`) already
   says what the keys held change; add a one-time hint on a canvas's
   first hover in a session ("Wheel: pan · Ctrl+wheel: zoom · Space +
   drag: pan · Home: fit"), and a "back to the content" button when the
   view holds none of it (Figma's recenter).
7. **Keeping the place**: the view saved with the application's state
   per document, an option on the widget (`keep_view(id)`).

**Testing.** Kittests per profile: for each profile and each canvas
kind, a table of gestures (the wheel, two fingers, a pinch, each drag,
from the canvas and from an item on it) and the expected effect (pans,
zooms, nothing), run over the editor alone and over one gallery page
per canvas. The gallery's page tests catch what the widget tests
cannot: a parent scroll area, a layout, a panel taking the input. The
navigation commands get one test each per canvas, reduced motion one
test that a fit jumps.

## 7. Open questions

**Q1. Hand mode.** How does a user without a middle button pan with a
drag?
- A. Space held only (today).
- B. A hand mode toggled by H and a button, plus Space held.
- C. A hand mode chosen in the settings for the device ("pen or touch").
- Recommended: **B**. It is Figma's and Miro's, and works for a pen.

**Q2. Fit selection's key.**
- A. Per profile, after the apps (Blender numpad `.`, Document
  Shift+2, darktable none).
- B. F everywhere (Nuke: selection, else all).
- C. Shift+Home everywhere.
- Recommended: **A**, with F as a second binding in Document and
  Blender, since F is free on the graph.

**Q3. The overview's policy.**
- A. Never by default; the application places it (today).
- B. When needed: a corner minimap inside the canvas once the content
  outgrows the view, toggled by a key.
- C. Always.
- Recommended: **B**, as Nuke and Houdini.

**Q4. Keeping the view across sessions.**
- A. Never: refit on open (today).
- B. Kept per document by the widget when given an id.
- C. Left to the application.
- Recommended: **B**, as an option, on in the gallery and Helios.

**Q5. A ComfyUI profile** (the wheel zooms, a left drag on empty space
pans, Ctrl + drag selects)?
- A. No: Blender / node editors is close enough.
- B. Yes, named after ComfyUI's legacy mode.
- Recommended: **A** for now; revisit when a user asks, since a left
  drag that pans conflicts with the rubber band every other profile has.

**Q6. A one-time hint overlay** on a canvas's first hover.
- A. No; the hint line and the zoom control's tooltips are enough.
- B. Yes, once per canvas kind per installation, dismissed by any
  gesture.
- Recommended: **B**: the report that started this note is a
  discoverability report as much as a bug.

## 8. Steps (once the questions are settled)

1. The panning-drag-from-anywhere rule on the image viewport and the
   plots (the graph has it, section 1).
2. The navigation commands and their keys; fit selection on the graph.
3. Hand mode; one finger pans on a touch screen.
4. The zoom control on the graph; the overview policy.
5. Keeping the view; the hint overlay.

## 9. Sources (read 2026-11-08)

- Blender: [input preferences](https://docs.blender.org/manual/en/latest/editors/preferences/input.html),
  [3D navigation](https://docs.blender.org/manual/en/latest/editors/3dview/navigate/navigation.html),
  [regions](https://docs.blender.org/manual/en/latest/interface/window_system/regions.html),
  [navigation preferences](https://docs.blender.org/manual/en/latest/editors/preferences/navigation.html),
  [node editor navigation (2.79)](https://docs.blender.org/manual/ja/2.79/editors/node_editor/navigate.html).
- Nuke: [navigating the node graph](https://learn.foundry.com/nuke/17.0/content/getting_started/using_interface/navigating_node_graph.html),
  [navigating inside windows](https://learn.foundry.com/nuke/8.0/content/getting_started/tutorial1/navigating_inside_windows.html).
- DaVinci Fusion: [Fusion 18 manual, node editor](https://www.steakunderwater.com/VFXPedia/__man/Fusion18-6/Fusion18_Manual_files/part248.htm).
- Houdini: [navigating networks](https://www.sidefx.com/docs/houdini/network/navigate.html).
- Unreal: [Blueprint editor cheat sheet](https://dev.epicgames.com/documentation/en-us/unreal-engine/blueprint-editor-cheat-sheet-in-unreal-engine),
  [graph editor](https://dev.epicgames.com/documentation/unreal-engine/graph-editor-for-the-blueprints-visual-scripting-editor-in-unreal-engine).
- ComfyUI: [canvas controls](https://docs.comfy.org/interface/canvas-controls),
  [canvas navigation settings](https://github.com/Comfy-Org/ComfyUI_frontend/blob/main/packages/settings-core/src/canvasNavigation.ts),
  [core settings](https://github.com/Comfy-Org/ComfyUI_frontend/blob/main/src/platform/settings/constants/coreSettings.ts).
- Figma: [zoom and view options](https://help.figma.com/hc/en-us/articles/1500004414582),
  [forum: zoom without Ctrl](https://forum.figma.com/suggest-a-feature-11/zoom-without-ctrl-35244/index3.html).
- Miro: [shortcuts](https://miro.com/shortcuts/),
  [navigation mode](https://help.miro.com/hc/en-us/articles/360017731053),
  [community: navigation modes](https://community.miro.com/ask-the-community-45/anyone-else-finding-the-navigation-modes-confusing-3944).
- darktable: [darkroom](https://docs.darktable.org/usermanual/development/en/darkroom/overview/),
  [navigation module](https://docs.darktable.org/usermanual/4.6/en/module-reference/utility-modules/darkroom/navigation/),
  [darkroom preferences](https://docs.darktable.org/usermanual/4.4/lo/preferences-settings/darkroom/).
- Lightroom Classic: [keyboard shortcuts](https://helpx.adobe.com/lightroom-classic/desktop/introduction-to-lightroom-classic/keyboard-shortcuts.html),
  [zooming in Lightroom Classic](https://jkost.com/blog/2020/07/zooming-in-lightroom-classic.html).
