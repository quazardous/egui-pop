# egui: colours of their own for scroll bars in `ScrollStyle`

> **Before posting (delete this block).** Repository: emilk/egui, as a
> new issue (no existing issue found asking for this; search again
> before posting). egui's CONTRIBUTING ("Agents & LLM policy"): the
> human posting is responsible for it; no AI disclosure is required.
> Checked against egui 0.36.2 and 0.37.0.

Title: Scroll bars: thumb and track colours in `ScrollStyle`

`ScrollStyle` describes a scroll bar's sizes, margins and opacities, but
not its colours. `ScrollArea` draws the thumb in the colours of the
widget states (`crates/egui/src/containers/scroll_area.rs`):

```rust
let handle_color = if scroll_style.foreground_color {
    visuals.fg_stroke.color
} else {
    visuals.bg_fill
};
```

where `visuals` is `widgets.inactive`, `hovered` or `active`. The thumb
and the track use that state's `corner_radius`, and the track is drawn
in `visuals.extreme_bg_color`.

So the scroll bars share their colours with every button (text colour or
fill, corners) and their track with text fields, canvases and progress
bars. A theme cannot give scroll bars a look of their own, for example
a thin, fully rounded thumb in a dim grey with no track, without changing
those widgets too. Today we work around it by showing each `ScrollArea`
in a `ui.scope` whose widget visuals are the scroll bar's, then setting
the content's style back inside `show`. That only reaches scroll areas we
create ourselves, not the ones in egui's own widgets (a combo box's list,
a menu) or in other crates.

**Proposal.** Optional colours in `ScrollStyle`, used when set and
falling back to today's behaviour when `None`, so nothing changes by
default:

```rust
pub struct ScrollStyle {
    // ...
    /// The thumb's colour at rest, under the pointer, and while dragged.
    /// `None`: the widget states' colours, as today.
    pub handle_color: Option<[Color32; 3]>,
    /// The track's colour. `None`: `extreme_bg_color`, as today.
    pub background_color: Option<Color32>,
    /// The thumb's and track's corners. `None`: the widget states'.
    pub corner_radius: Option<CornerRadius>,
}
```

(A small struct with `inactive`, `hovered` and `active` fields would
also work in place of the array.) The opacities keep applying on top of
these colours, so floating bars still fade as they do now.

We can open a PR if this is welcome.
