# Pointer gestures and profiles

What the wheel, a trackpad's two fingers, a pinch and each mouse button
do on a canvas is not one fixed rule: photo applications zoom with the
wheel, document tools scroll with it, node editors pan with the middle
button. egui-pop ships five **pointer profiles** and lets the user pick
one, then change any gesture in the shortcuts editor, as a keyboard
shortcut is changed. The study behind it, with the survey of fifteen
applications, is [design/pointer-gestures.md](design/pointer-gestures.md).

## Gestures are keymap bindings

A chord's key may be a pointer gesture, with the usual modifiers
(`Ctrl` is Cmd on macOS):

| Gesture | What it is |
|---|---|
| `Wheel` | a mouse wheel's notches (line units) |
| `Scroll` | a smooth scroll in points: a trackpad's two fingers, a free-spinning wheel |
| `Pinch` | two fingers apart or together (a browser sends a trackpad's pinch the same way) |
| `PrimaryClick`, `MiddleClick`... | a button pressed and released without moving |
| `PrimaryDoubleClick`... | a double click |
| `PrimaryDrag`, `MiddleDrag`, `SecondaryDrag`... | a button held while the pointer moves |

Buttons are `Primary`, `Secondary`, `Middle`, `Back` and `Forward`: the
logical buttons (a left-handed setup swaps them in the system). A
pointer chord stands alone, never in a sequence: `Ctrl+Wheel`,
`Shift+MiddleDrag`, `Pinch`. It is bound to a **gesture command** in the
scope of a canvas kind:

| Scope | Commands |
|---|---|
| `surface:canvas.image` (the image viewport) | `gesture.image.zoom` (holding at fit and 100 %), `zoom_free`, `pan`, `pan_x`, `pan_y`, `zoom_cycle` (100 %, 200 %, fit), `fit`, `zoom_toggle` (fit or 100 %) |
| `surface:canvas.graph` (the node graph) | `gesture.graph.zoom`, `pan`, `pan_x`, `pan_y`, `fit` |
| `surface:widget.value` (sliders) | `gesture.value.step` (its wheel's modifiers step a slider without the focus), `fine` and `coarse` (their drag's modifiers: ten times finer or coarser, for drags, arrows and the wheel), `panel` (its wheel's modifiers leave the wheel to the panel) |

The keymap's layers, removals, conflicts and resolution trace apply
unchanged ([plugins.md](plugins.md)); the key dispatcher never sees a
pointer chord: canvases read the resolved table instead.

## The six profiles

A profile is a `pointer-profile` document (in
`crates/egui-pop-model/profiles/`): bindings for the default layer, the
options it gives its default for, the modifiers egui's own scrolling
follows (`egui::InputOptions`), so plain scroll areas agree, and, where
they differ from the drag's, the modifiers of a value's arrow keys
(`value_keys`). Each profile named after an application follows that
application; where it has no equivalent the gesture is left unbound.

| | Document (default) | darktable | Photoshop | Lightroom Classic | Blender / node editors | Trackpad |
|---|---|---|---|---|---|---|
| Wheel on the image | pans | zooms (Ctrl: no stops) | pans | — | zooms | pans |
| Ctrl + wheel on the image | zooms | zooms without stops | pans sideways | zooms | pans sideways | zooms |
| Shift + wheel on the image | pans sideways | pans sideways | — | — | pans up and down | — |
| Alt + wheel on the image | — | — | zooms | — | — | — |
| Two fingers / a pinch | pan / zoom | pan / zoom | pan / zoom | pan / zoom | pan / zoom | pan / zoom |
| Primary drag on the image | pans | pans | pans | pans | pans | pans |
| Middle drag | pans | pans | — | — | pans (graph: also a secondary drag) | — |
| Middle click on the image | — | 100 %, 200 %, fit | — | — | — | — |
| Click on the image | — | — | — | fit or 100 % | — | — |
| Double click on the image | fits | leaves the view | — | leaves the view | fits | fits |
| Wheel on the graph | pans | zooms | pans | pans | zooms | pans |
| Wheel over a slider | scrolls the panel | changes the value | scrolls the panel | scrolls the panel | scrolls the panel | scrolls the panel |
| Steps a slider without the focus | Alt + wheel | the wheel; Ctrl+Alt + wheel scrolls the panel | — | — | Ctrl + wheel | Ctrl + wheel |
| Finer / coarser drag | Ctrl / Shift | Ctrl / Shift | Alt / Shift | Shift / — | Shift / Ctrl | Ctrl / Shift |
| Finer / coarser arrows | as the drag | as the drag | as the drag | Alt / Shift | as the drag | as the drag |

In every profile a drag with Space held pans the image and the graph,
Escape cancels a drag, a primary drag on a handle moves it and the wheel
over a handle adjusts it. The finer and coarser modifiers apply to a
slider's drag, its wheel and, unless the profile gives them their own,
its arrow keys: Lightroom Classic makes a Shift drag finer but Shift
with the arrows coarser, and has no coarser drag. Neither Adobe
application has a node graph: both profiles take Photoshop's canvas
wheel there. Lightroom's click on the image toggles fit and 100 %, and
a double click leaves the view, as its Loupe goes back to the Grid: its
first click toggles the zoom, its second press only leaves. The id `adobe`, from before the split into Photoshop
and Lightroom Classic, still works and names `lightroom`; pickers do
not list it. "Leaves the view" is a signal
(`Signal::Leave`): the application decides what it means (darktable
goes back to the lighttable).

An application picks its profile with `AppBuilder::pointer_profile`
(`"darktable"`); the user changes it in the settings
(`input.pointer_profile`). Two options follow the profile unless set:
`input.slider_wheel` (`guarded`: the wheel scrolls the panel and steps a
slider with the focus or the step modifiers; `value`: it changes the
slider under the pointer) and `input.image_double_click` (`fit`,
`leave`, `nothing`). `input.wheel_device` settles what sends wheel
events: `auto` takes lines for a mouse wheel and points for a
trackpad, `mouse` and `trackpad` say it outright (some mice send
points).

```rust
use egui_pop::api::{App, DefaultPlugins};

let app = App::builder()
    .plugins(DefaultPlugins)
    .pointer_profile("darktable")
    .build(&egui_pop::egui::Context::default())
    .unwrap();
assert_eq!(egui_pop::api::pointer::profile_id(app.handle()), "darktable");
```

## Changing a gesture

The shortcuts editor (`Ctrl+,`, then *Keyboard shortcuts*) lists every
gesture command under *Pointer*, says which profile is in force and how
many of its gestures the user changed, and has a filter showing the
gestures alone. Each trigger of a gesture has its own removal; *Add a
gesture* opens a pad on which the gesture is done (the wheel, a pinch,
a click, a double click or a drag, holding its keys), then used. The
reset takes the profile's triggers back. Conflicts are reported per
canvas scope, as for keys. On Linux, Alt with a drag or the wheel comes
with a warning: some window managers take it.

The user's changes are user-layer bindings, saved in the keymap
document (version 3), and stay over the profile. When the profile
changes and some exist, a dialog asks whether to keep them on the new
profile or drop them (`pointer.overrides`, `{ "value": "keep" | "drop" }`).
`AppHandle::rebind_gestures` changes a gesture's triggers from code.

## What canvases read

The application publishes the table in force to its context after each
change (`egui_pop::core::pointer::map`); without an application, canvases
follow the default profile. A canvas of an application's own reads the
wheel raw with `egui_pop::core::pointer::wheel_inputs` (unit and
modifiers kept: egui merges a pinch with Ctrl and the wheel once it
smooths the delta) and looks each gesture up in its scope
(`PointerMap::command`). The image viewport's `Gestures` and the
graph's take their pointer part from it (`viewport::gestures::ViewGestures`),
sliders their modifiers (`core::slider::Gestures::from_map`);
`set_gestures` still sets either outright.

The image viewport and the graph give a **hint** while the pointer is
over them: what the pointer does with the keys held now
(`Wheel: pan · PrimaryDrag: pan`, with Ctrl `Wheel: zoom`), in their
output (`ViewportOutput::hint`, `GraphOutput::hint`) and for a status
line (`egui_pop::core::pointer::hint_now`). The viewport shows the hand
when the keys held make a drag pan where a plain drag would not.

## A widget's own secondary button

The secondary button opens a context menu everywhere by default, and a
canvas may bind a secondary drag in its profile (Blender / node editors
pan the graph with it). A widget that needs the right click for
something else (a curve editor deleting a point, a swatch resetting its
colour, a canvas of the application's binding a right drag) **claims**
it on its rect, each pass it is drawn
(`egui_pop::core::pointer::claim`):

```rust
use egui_pop::core::pointer::claim::{self, SecondaryClaim};
use egui_pop::egui;

fn swatch(ui: &mut egui::Ui) {
    let r = ui.allocate_response(egui::vec2(24.0, 24.0), egui::Sense::click());
    claim::claim_secondary(&r, SecondaryClaim::new("Swatch").click("Resets the colour"));
    if r.secondary_clicked() {
        // reset the colour
    }
}
```

`SecondaryClaim::click` takes the click, `drag` the drag; a widget may
take both. Where the button was pressed over a claim:

- no context menu opens, neither one declared on a parent
  (`menus::context_menu`) nor egui-pop's own (the graph's, the status
  bar's, a tree's, the window's system menu on the title bar); a menu
  drawn with egui's `Response::context_menu` should go through
  `claim::context_menu` to obey claims, and code reading the click
  itself asks `claim::opens_context_menu`;
- the canvas's profile binding for that gesture does not fire (the
  image viewport's secondary clicks and drags, the graph's secondary
  drag), and a canvas of the application's asks `claim::claimed` the
  same way.

A claimed click leaves the secondary drag to the canvas, and a claimed
drag leaves the click to the menus. A claim counts for the pass it was
made in and the next, so a parent that reads its response before its
children are drawn sees them; a window or popup drawn above it hides
it. The curve editor claims the secondary click, which deletes the
point under the pointer.

What a claim says it does is kept as the widget's **use**: the
shortcuts editor lists the uses seen so far under *Widgets' own
gestures* (they are the widget's, so not changed there), and
`claim::declare_use` lists one before its widget is first drawn. The
debug bridge's `pointer` collector shows the profile in force, the hint,
the claims in force (widget, rect, click and drag with what they do)
and the uses.

## The web

Ctrl and the wheel zoom a page, and a trackpad's pinch arrives the same
way: a web build takes them from the browser. Pass
`egui_pop::core::pointer::web_prevent_default` as eframe's
`WebOptions::should_prevent_default`, and give the canvas
`touch-action: none` in its page, as the gallery does.
