# Image tools

The image tools of `egui-pop-viewport` (the facade's `viewport`
feature) are the controls of a photo editor's develop panels: a
histogram, a curve editor, colour wheels, a zoom control and a
navigator. They follow one rule: **the application computes, egui-pop
draws and handles the interaction**. The histogram's bins, the curve's
maths, what a wheel's value means and the image itself are the
application's; the widgets draw them, take the gestures and hand back
what changed.

![the curves and wheels page](../crates/egui-pop-gallery/tests/snapshots/grading-pop-dark.png)

Every tool works like the [sliders](sliders.md):

- each change is a `Change` (`from`, `to`, the `Cause`, and whether the
  gesture is `done`), seen by the tool's `on_change` hook before it is
  written; the hook answers `Verdict::Allow`, `Veto` or `Redirect`;
- a drag's frames come with `done: false`, and its end once more, from
  the value before the drag, with `done: true`; a veto then puts the
  value back, and Escape during a drag puts it back with no trace;
- Ctrl (Cmd) makes a drag or an arrow ten times finer, Shift ten times
  coarser (the sliders' `Gestures`);
- a click never makes a relative control jump;
- `show(ui)` returns what was written in the frame.

## Histogram

```rust
use egui_pop::egui;
use egui_pop::viewport::histogram::{Channel, Clipping, Histogram};

fn histogram(ui: &mut egui::Ui, red: &[f32], green: &[f32], blue: &[f32], exposure: &mut f64) {
    let out = Histogram::new([
        Channel::new("Red", red, egui::Color32::from_rgb(220, 70, 60)),
        Channel::new("Green", green, egui::Color32::from_rgb(80, 190, 90)),
        Channel::new("Blue", blue, egui::Color32::from_rgb(70, 120, 230)),
    ])
    .log(true)
    // The application measured them: the share of pixels clipped.
    .clipping(Clipping { shadows: 0.0, highlights: 0.004 })
    .draggable(true)
    .show(ui);
    // Four stops per width of the histogram.
    *exposure += f64::from(out.moved()) * 4.0;
}
```

- **Bins** per channel, any scale, normalised together; linear or
  logarithmic heights.
- **Clipping markers**: a triangle in the bottom-left corner (shadows,
  `histogram.clip-shadows`) and one in the bottom-right corner
  (highlights, `histogram.clip-highlights`), lit when the fraction is
  above zero; the accessible value says how much.
- **Drag**: darktable's drag of the exposure on the histogram. The
  widget owns no value; each change is a `Drag` (where the gesture
  began, `at`, from 0 on the left, and how far it moved, `delta`, in
  widths). The output's changes say how far the gesture has moved, from
  `from` to `to`: adding `to.delta - from.delta` (`moved()`) to the
  value keeps it in step, including after a veto or Escape (a change
  back to 0 comes then). Where it began (`at`) lets the application
  drag the black level from the left and the exposure from the right.
  Focused, the left and right arrows move it by a step.

## Curve editor

```rust
use egui_pop::egui;
use egui_pop::viewport::curve::{CurveEditor, Linear, Pick, PickAction, Spline};

/// The application's curve maths: here straight segments, eased.
struct Eased;

impl Spline for Eased {
    fn eval(&self, points: &[[f32; 2]], x: f32) -> f32 {
        let y = Linear.eval(points, x);
        y * y * (3.0 - 2.0 * y)
    }
}

fn tone(ui: &mut egui::Ui, points: &mut Vec<[f32; 2]>, bins: &[f32], under_pointer: Option<f32>) {
    CurveEditor::new(points, "Tone curve")
        .spline(&Eased)
        .histogram(bins)
        // The value under the pointer in the image: marked, its point lit.
        .pick(under_pointer.map(|x| Pick { x, action: PickAction::Hover }))
        .show(ui);
}
```

Points are `[x, y]` in 0..1, sorted by `x`, at least two. The spline is
the application's, through the `Spline` trait; `MonotoneCubic` (the
default: smooth, through every point, never overshooting) and `Linear`
are supplied.

| Gesture | What it does |
|---|---|
| Press on the curve | Adds a point there; a drag moves it at once |
| Drag a point | Moves it between its neighbours (Ctrl finer, Shift coarser; Escape puts it back) |
| Double click or right click on a point | Deletes it (two points stay) |
| Double click elsewhere | Back to the default points (`default_points`, the identity unless given) |
| Page Down, Page Up | The next, the previous point (when focused) |
| Arrows | Move the selected point by a step |
| Insert | Adds a point halfway to the next one |
| Delete, Backspace | Deletes the selected point |
| R | Resets the curve |

The keys are data (`curve::CurveKeys`, `curve::set_keys`).

**The targeted adjustment.** In a photo editor, the user points at the
image and drags up or down to adjust the tone under the pointer. The
application reads the value under the pointer (the pixel's input on the
curve, 0..1) and passes it with what the pointer does:
`PickAction::Hover` marks it and lights the nearest point, `Press`
selects that point or adds one on the curve, `Move(dy)` moves its
output, `Release` ends the gesture (one done change, cause `pick`), and
`Cancel` puts the points back. The gallery's Curves and wheels page
does it on a grey ramp standing in for the image.

## Colour wheels

```rust
use egui_pop::egui;
use egui_pop::viewport::wheel::{ColorWheel, WheelValue};

fn grading(ui: &mut egui::Ui, wheels: &mut [WheelValue; 3], colour: &mut WheelValue) {
    ui.horizontal_top(|ui| {
        for (value, label) in wheels.iter_mut().zip(["Lift", "Gamma", "Gain"]) {
            ColorWheel::offset(value, label).size(130.0).show(ui);
        }
        ColorWheel::picker(colour, "Colour").size(130.0).show(ui);
    });
}
```

A wheel's value is a `WheelValue`: a hue in degrees (0 to the right,
counter-clockwise), a chroma (0 at the centre, 1 at the rim) and a
luminance; what they mean (an offset, a power, a gain, a colour) is the
application's.

- `ColorWheel::offset`: the grading wheels. Neutral at the centre; a
  drag moves the puck relatively, so a press never makes it jump; a
  double click resets; the luminance slider is signed, from -1 to 1.
- `ColorWheel::picker`: hue and saturation. A press takes the colour
  under the pointer and a drag follows it; the luminance runs from 0 to
  1.
- The luminance slider is one of egui-pop's sliders, named after the
  wheel ("Lift luminance"); its changes go through the wheel's hook as
  changes of the whole value. `.luminance(None)` leaves it out.
- Focused, the arrows move the puck by a step.

The disc's colours are content (the hues around a neutral centre); its
ring and puck are tokens (`wheel.ring`, `wheel.puck`,
`wheel.puck-outline`).

## Zoom control and navigator

```rust
use egui_pop::egui;
use egui_pop::viewport::navigator::{Navigator, ZoomControl};
use egui_pop::viewport::{View, Viewport, ViewportState};

fn darkroom(ui: &mut egui::Ui, state: &mut ViewportState, view: &mut View, proxy: egui::TextureId) {
    ZoomControl::new(state, view).show(ui);
    ui.horizontal_top(|ui| {
        Viewport::new("main")
            .image_size([6000, 4000])
            .size(egui::vec2(640.0, 420.0))
            .show(ui, state, view);
        Navigator::new(state, view).texture(proxy).show(ui);
    });
}
```

Both share the viewport's own state: what they change goes through the
viewport's `before_view` hook, like its own gestures, and comes out as
its `Signal::View` with the next frame.

- `ZoomControl`: zoom out and zoom in (one step of the zoom ladder),
  the zoom in percent, fit and 100 %.
- `Navigator`: the whole image (a texture the application gives, its
  proxy) with a frame around the part in view and a veil over the rest.
  A drag of the frame pans (relative, Escape puts the view back), a
  press elsewhere centres the view there, the wheel zooms, a double
  click fits; focused, the arrows pan, `+` and `-` zoom, `0` fits.

## Steps

`egui-pop-imaging` (the facade's `imaging` feature) turns each tool's
hook into a step of the application (ADR 0011). Each finished change
runs the tool's step; a drag's frames stay out of it, and its release
decides.

| Step | Arguments | Default |
|---|---|---|
| `histogram.drag` | `histogram`, `from`, `to` (`{at, delta}`), `cause` | writes `to` |
| `curve.change` | `curve`, `from`, `to` (`[[x, y], ...]`), `cause` (`drag`, `add`, `delete`, `reset`, `key`, `pick`, `access`) | writes `to` |
| `wheel.change` | `wheel`, `from`, `to` (`{hue, chroma, luminance}`), `cause` | writes `to` |
| `zoom.change` | `viewport`, `from`, `to` (`{zoom: "fit" or a number, center}`), `cause` | writes `to` |

A before-hook vetoes the change (the tool keeps, or goes back to,
`from`) or redirects it; a run-hook gives the value to write; an
after-hook follows it, and the journal records every run.
`ImagingPlugin` declares the steps.

```rust
use egui_pop::egui;
use egui_pop::api::steps::Before;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::imaging::{self, STEP_CURVE};
use egui_pop::model::ids::PluginId;
use egui_pop::viewport::curve::CurveEditor;

/// Refuses curves of more than eight points.
struct EightPoints;

impl Plugin for EightPoints {
    fn id(&self) -> PluginId {
        PluginId::from_static("eight_points")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![imaging::IMAGING_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_CURVE, |_, args| {
            if args["to"].as_array().map_or(0, Vec::len) > 8 {
                Before::Veto("eight points at most".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

fn tone(ui: &mut egui::Ui, app: &AppHandle, points: &mut Vec<[f32; 2]>) {
    CurveEditor::new(points, "Tone curve")
        .on_change(imaging::curve(app, "tone"))
        .show(ui);
}

let _ = (EightPoints, tone);
```

## Accessibility

Every tool is an AccessKit node named by its label, with a value
screen readers say:

| Tool | Role | Value | Actions |
|---|---|---|---|
| Histogram | slider when draggable | its channels and bins, what is clipped | increment and decrement move the drag |
| Curve editor | slider | the selected point and its input and output | increment, decrement and set-value change that point's output |
| Colour wheel | slider (and the luminance slider) | hue, chroma and luminance | increment and decrement change the chroma |
| Zoom control | buttons | | |
| Navigator | scroll view | the zoom and the centre in view | |

## Tokens

`histogram.*` (with `clip-shadows`, `clip-highlights`), `curve.*`
(`background`, `grid`, `identity`, `line`, `point`, `point-selected`,
`pick`), `wheel.*` (`ring`, `puck`, `puck-outline`) and `navigator.*`
(`background`, `frame`, `veil`), each an alias of a system role; the
curve and its points are contrast-checked against the editor's
background. See [Design tokens](tokens.md).

## Why its own

egui_plot draws histograms and lines but brings its own axes, pan and
zoom; no crate checked has a curve editor with an application's spline,
colour grading wheels or an image navigator for egui 0.36. See
[the survey](widgets.md#image-tools-what-was-checked-2026-10-13).
