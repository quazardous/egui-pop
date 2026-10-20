//! Visual kits (`docs/kit.md`): what a component looks like, as data from
//! the theme and drawing handlers, compiled once when the theme is
//! installed.
//!
//! egui-pop's widgets keep the behaviour (clicks, drags, focus, the
//! keyboard, accessibility) and compute a [`State`]; the kit measures and
//! paints. Every component is described with the same words: per state
//! (`rest`, `hover`, `active`, `focus`, `selected`, `disabled`) a box
//! ([`BoxStyle`]: fill, border, radius, text colour, indicator), plus a
//! `shape`, the name of the handler that draws it ([`Primitive`]). A
//! token is always `component.state.term` (`tab.hover.fill`), or
//! `component.term` for what does not change with the state
//! (`tab.shape`, `tab.padding-x`).
//!
//! Nothing is resolved while drawing: [`install`] compiles the theme into
//! a [`Kit`] of plain values, with every combination of states merged
//! ahead (32 boxes per component), and the handlers picked by name.
//! Drawing reads fields and makes one call per part.
//!
//! The default look is a kit like any other: the `tab.*` tokens of the
//! built-in `pop-dark` theme, which every theme extends, drawn by the
//! handlers shipped here (`box`, `underline`). An application adds its
//! own handlers with [`register_tab_shape`] before installing a theme
//! that names them.

use std::collections::BTreeMap;
use std::sync::Arc;

use egui::epaint::{Shadow, Shape};
use egui::layers::ShapeIdx;
use egui::{
    Color32, Context, Frame, Galley, Id, InnerResponse, Margin, Painter, Pos2, Rect, Response,
    Stroke, Ui, Vec2,
};
use egui_pop_tokens::Value;

use crate::theme::Tokens;

mod glyph;
mod paint;
mod shapes;
mod source;
mod tone;
mod window;

pub use glyph::{Glyph, Glyphs};

pub use paint::{Corners, label_rect, paint_box, paint_indicator, paint_label, paint_ring};
pub use source::KitSource;
pub use tone::Tone;
pub use window::{ButtonsHover, ButtonsSide, WindowCorners, WindowKit};

/// The states a part can be in, combined (a selected tab under the
/// pointer is `SELECTED | HOVER`). No flag is the resting state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct State(u8);

impl State {
    pub const REST: State = State(0);
    pub const HOVER: State = State(1);
    pub const ACTIVE: State = State(1 << 1);
    pub const FOCUS: State = State(1 << 2);
    pub const SELECTED: State = State(1 << 3);
    pub const DISABLED: State = State(1 << 4);

    /// The flags, in the order their boxes apply (a later one wins):
    /// selected, hover, focus, active, disabled. The names are the
    /// tokens' state names.
    pub const ORDER: [(State, &'static str); 5] = [
        (State::SELECTED, "selected"),
        (State::HOVER, "hover"),
        (State::FOCUS, "focus"),
        (State::ACTIVE, "active"),
        (State::DISABLED, "disabled"),
    ];

    pub fn contains(self, other: State) -> bool {
        self.0 & other.0 == other.0
    }

    /// The state egui sees on a response: `HOVER` under the pointer,
    /// `ACTIVE` while pressed, `FOCUS` with the keyboard focus, `DISABLED`
    /// when not enabled. What only the widget knows (`SELECTED`) it adds
    /// with [`State::with`].
    pub fn from_response(r: &Response) -> State {
        State::REST
            .with(State::HOVER, r.hovered())
            .with(State::ACTIVE, r.is_pointer_button_down_on())
            .with(State::FOCUS, r.has_focus())
            .with(State::DISABLED, !r.enabled())
    }

    /// `self` with `other` added when `on`.
    #[must_use]
    pub fn with(self, other: State, on: bool) -> State {
        if on { State(self.0 | other.0) } else { self }
    }
}

impl std::ops::BitOr for State {
    type Output = State;
    fn bitor(self, rhs: State) -> State {
        State(self.0 | rhs.0)
    }
}

/// A side of a box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

/// Where a label goes along its part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    /// After the start padding.
    Start,
    /// On the part's centre, moved into its room when it does not fit
    /// there.
    Centre,
    /// Before the end padding.
    End,
}

impl Align {
    fn parse(s: &str) -> Option<Align> {
        Some(match s {
            "start" => Align::Start,
            "centre" | "center" => Align::Centre,
            "end" => Align::End,
            _ => return None,
        })
    }
}

impl Side {
    fn parse(s: &str) -> Option<Side> {
        Some(match s {
            "top" => Side::Top,
            "bottom" => Side::Bottom,
            "left" => Side::Left,
            "right" => Side::Right,
            _ => return None,
        })
    }
}

/// What a part looks like in one state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoxStyle {
    pub fill: Color32,
    pub border_color: Color32,
    /// Points; snapped to whole physical pixels when painted.
    pub border_width: f32,
    pub radius: f32,
    pub text: Color32,
    /// The bar on one side of the box ([`ComponentKit::indicator_side`]);
    /// none when its size is zero or its colour transparent.
    pub indicator_color: Color32,
    pub indicator_size: f32,
    /// The focus ring (usually given by the `focus` state only): an
    /// outline `ring_offset` points outside the box (inside when
    /// negative), none when its width is zero.
    pub ring_color: Color32,
    pub ring_width: f32,
    pub ring_offset: f32,
}

/// The terms of a box, as a state gives them: only what differs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct BoxPatch {
    fill: Option<Color32>,
    border_color: Option<Color32>,
    border_width: Option<f32>,
    radius: Option<f32>,
    text: Option<Color32>,
    indicator_color: Option<Color32>,
    indicator_size: Option<f32>,
    ring_color: Option<Color32>,
    ring_width: Option<f32>,
    ring_offset: Option<f32>,
}

impl BoxPatch {
    fn read(t: &Tokens, prefix: &str) -> BoxPatch {
        let r = t.resolved();
        let c = |term: &str| t.color(&format!("{prefix}.{term}"));
        let d = |term: &str| r.dimension(&format!("{prefix}.{term}"));
        BoxPatch {
            fill: c("fill"),
            border_color: c("border-color"),
            border_width: d("border-width"),
            radius: d("radius"),
            text: c("text"),
            indicator_color: c("indicator-color"),
            indicator_size: d("indicator-size"),
            ring_color: c("ring-color"),
            ring_width: d("ring-width"),
            ring_offset: d("ring-offset"),
        }
    }

    fn apply(&self, b: &mut BoxStyle) {
        let BoxPatch {
            fill,
            border_color,
            border_width,
            radius,
            text,
            indicator_color,
            indicator_size,
            ring_color,
            ring_width,
            ring_offset,
        } = *self;
        b.fill = fill.unwrap_or(b.fill);
        b.border_color = border_color.unwrap_or(b.border_color);
        b.border_width = border_width.unwrap_or(b.border_width);
        b.radius = radius.unwrap_or(b.radius);
        b.text = text.unwrap_or(b.text);
        b.indicator_color = indicator_color.unwrap_or(b.indicator_color);
        b.indicator_size = indicator_size.unwrap_or(b.indicator_size);
        b.ring_color = ring_color.unwrap_or(b.ring_color);
        b.ring_width = ring_width.unwrap_or(b.ring_width);
        b.ring_offset = ring_offset.unwrap_or(b.ring_offset);
    }
}

/// A component's boxes for every combination of states, merged when the
/// kit is compiled, and per tone when its tokens give tones
/// ([`Tone`], `kit/tone.rs`).
#[derive(Clone, Debug, PartialEq)]
pub struct States([BoxStyle; 32], Option<Box<tone::Tones>>);

impl States {
    fn compile(t: &Tokens, component: &str) -> States {
        let mut rest = BoxStyle::default();
        BoxPatch::read(t, &format!("{component}.rest")).apply(&mut rest);
        let patches: Vec<(State, BoxPatch)> = State::ORDER
            .iter()
            .map(|(s, name)| (*s, BoxPatch::read(t, &format!("{component}.{name}"))))
            .collect();
        let boxes = std::array::from_fn(|bits| {
            let state = State(bits as u8);
            let mut b = rest;
            for (s, patch) in &patches {
                if state.contains(*s) {
                    patch.apply(&mut b);
                }
            }
            b
        });
        let tones = tone::compile(t, component, &rest, &patches);
        States(boxes, tones)
    }

    /// The box of a state, in the tone it carries ([`State::toned`]).
    pub fn get(&self, state: State) -> &BoxStyle {
        tone::get(&self.0, self.1.as_deref(), state)
    }

    /// Whether the component's tokens give tones.
    pub fn toned(&self) -> bool {
        self.1.is_some()
    }
}

/// What a part is asked to paint.
#[derive(Debug)]
pub struct Part<'a> {
    pub rect: Rect,
    /// Where its label may go: the part's rectangle, or less when the
    /// widget puts other things on it (a title bar's items).
    pub room: Rect,
    pub state: State,
    /// Its label, laid out without a colour (the box's `text` is given at
    /// paint).
    pub label: Option<&'a Arc<Galley>>,
}

/// What a handler paints with.
#[allow(missing_debug_implementations)] // egui's Painter has no Debug.
pub struct KitCx<'a> {
    pub painter: &'a Painter,
    /// Physical pixels per point: strokes snap to whole pixels.
    pub ppp: f32,
}

/// A component's drawing handler. Every component's handlers have this
/// shape; `D` is the component's data.
pub trait Primitive<D>: Send + Sync {
    /// The size of a part around a label of size `label`.
    fn measure(&self, data: &D, label: Vec2) -> Vec2;
    /// Paints a part's box (everything but its label).
    fn paint(&self, cx: &KitCx<'_>, data: &D, part: &Part<'_>);
    /// Paints a part's label, after the widget's own content.
    fn paint_label(&self, cx: &KitCx<'_>, data: &D, part: &Part<'_>);
    /// Whether `pos` hits the part (a part that is not a rectangle).
    fn hit(&self, _data: &D, rect: Rect, pos: Pos2) -> bool {
        rect.contains(pos)
    }
}

/// The data of a component: its boxes per state, and what does not
/// change with the state.
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentKit {
    pub states: States,
    /// Points between the label and the part's side edges.
    pub padding_x: f32,
    /// Points between the content and the part's top and bottom edges
    /// (a frame around content, [`Component::show`]).
    pub padding_y: f32,
    /// Points; zero when the component has no fixed height.
    pub height: f32,
    /// Points; zero when the component has no fixed width.
    pub width: f32,
    /// Points: the size of its text (`font-size`); zero when not given,
    /// the widget's own size then.
    pub font_size: f32,
    /// Points: how far the part sits from the edge it is drawn on,
    /// towards the outside (a side area's grip, off its area's edge).
    pub offset: f32,
    pub indicator_side: Side,
    pub align: Align,
    /// How opaque the whole part is, 0 to 1 (1 when not given): a
    /// backdrop's strength.
    pub opacity: f32,
    /// The shadow it casts (`shadow-offset-x`, `shadow-offset-y`,
    /// `shadow-blur`, `shadow-spread`, `shadow-color`); none when not
    /// given.
    pub shadow: Shadow,
}

impl ComponentKit {
    fn compile(t: &Tokens, component: &str, problems: &mut Vec<String>) -> ComponentKit {
        let r = t.resolved();
        let side = string(t, &format!("{component}.indicator-side"));
        let indicator_side = match side.as_deref().map(Side::parse) {
            None => Side::Bottom,
            Some(Some(s)) => s,
            Some(None) => {
                problems.push(format!(
                    "{component}.indicator-side: `{}` is not top, bottom, left or right",
                    side.unwrap_or_default()
                ));
                Side::Bottom
            }
        };
        let align_asked = string(t, &format!("{component}.align"));
        let align = match align_asked.as_deref().map(Align::parse) {
            None => Align::Centre,
            Some(Some(a)) => a,
            Some(None) => {
                problems.push(format!(
                    "{component}.align: `{}` is not start, centre or end",
                    align_asked.unwrap_or_default()
                ));
                Align::Centre
            }
        };
        ComponentKit {
            states: States::compile(t, component),
            width: r.dimension(&format!("{component}.width")).unwrap_or(0.0),
            font_size: r
                .dimension(&format!("{component}.font-size"))
                .unwrap_or(0.0)
                .max(0.0),
            offset: r.dimension(&format!("{component}.offset")).unwrap_or(0.0),
            align,
            padding_x: r
                .dimension(&format!("{component}.padding-x"))
                .unwrap_or(0.0),
            padding_y: r
                .dimension(&format!("{component}.padding-y"))
                .unwrap_or(0.0),
            height: r.dimension(&format!("{component}.height")).unwrap_or(0.0),
            indicator_side,
            opacity: r
                .dimension(&format!("{component}.opacity"))
                .map_or(1.0, |o| o.clamp(0.0, 1.0)),
            shadow: crate::theme::shadow(r, &format!("{component}.shadow-"))
                .unwrap_or(Shadow::NONE),
        }
    }
}

/// A component's data and the handler that draws it.
#[derive(Clone)]
pub struct Component {
    pub data: ComponentKit,
    /// The handler's name (`tab.shape`).
    pub shape: String,
    handler: Arc<dyn Primitive<ComponentKit>>,
    /// Whether the handler is the built-in `card`, which egui's own
    /// [`Frame`] draws exactly ([`Component::show`]).
    egui_frame: bool,
}

impl Component {
    pub fn measure(&self, label: Vec2) -> Vec2 {
        self.handler.measure(&self.data, label)
    }

    /// Paints a part's box.
    pub fn paint(&self, cx: &KitCx<'_>, part: &Part<'_>) {
        self.handler.paint(cx, &self.data, part);
    }

    /// Paints a part's label.
    pub fn paint_label(&self, cx: &KitCx<'_>, part: &Part<'_>) {
        self.handler.paint_label(cx, &self.data, part);
    }

    /// Paints a part, box then label (a widget with nothing of its own
    /// between them).
    pub fn paint_part(&self, cx: &KitCx<'_>, part: &Part<'_>) {
        self.paint(cx, part);
        self.paint_label(cx, part);
    }

    /// Where the default label painting puts a label of `size`
    /// ([`label_rect`]): a widget gives it to assistive technology.
    pub fn label_rect(&self, part: &Part<'_>, size: Vec2) -> Rect {
        label_rect(&self.data, part, size)
    }

    /// The box of a state (in the tone it carries).
    pub fn style(&self, state: State) -> &BoxStyle {
        self.data.states.get(state)
    }

    /// The box of a state in a tone.
    pub fn style_toned(&self, tone: Tone, state: State) -> &BoxStyle {
        self.data.states.get(state.toned(tone))
    }

    pub fn hit(&self, rect: Rect, pos: Pos2) -> bool {
        self.handler.hit(&self.data, rect, pos)
    }

    /// The box of a state as an egui [`Frame`] at `ppp` physical pixels
    /// per point: its fill, its border (its width snapped to whole
    /// physical pixels, as the kit's own painting does) and corner
    /// radius, the padding (`padding-x`, `padding-y`) as its inner
    /// margin. No shadow: egui's own, from `Visuals`, is the caller's to
    /// add. For a container that takes a frame; a part egui lays out
    /// around its content is better shown with [`Component::show`],
    /// which also paints the focus ring and honours a handler of the
    /// theme's.
    pub fn frame(&self, state: State, ppp: f32) -> Frame {
        let b = self.style(state);
        let m = |p: f32| p.round().clamp(-128.0, 127.0) as i8;
        let stroke = if b.border_width > 0.0 {
            Stroke::new(crate::scale::snap(b.border_width, ppp), b.border_color)
        } else {
            Stroke::NONE
        };
        Frame::new()
            .fill(b.fill)
            .stroke(stroke)
            .corner_radius(b.radius)
            .inner_margin(Margin::symmetric(
                m(self.data.padding_x),
                m(self.data.padding_y),
            ))
    }

    /// Shows `add` inside the component's box in `state`, laid out by egui
    /// (the part is as large as its content and padding), with egui's
    /// `shadow` under it ([`Shadow::NONE`] for none).
    ///
    /// The built-in `card` handler is egui's [`Frame`] of the box, then
    /// the state's focus ring. Another handler paints once the content is
    /// laid out, its shapes moved under the content: what it draws needs
    /// one allocation per part and frame, which `card` does not.
    pub fn show<R>(
        &self,
        ui: &mut Ui,
        state: State,
        shadow: Shadow,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        let ppp = ui.ctx().pixels_per_point();
        let b = *self.style(state);
        if self.egui_frame {
            let r = self.frame(state, ppp).shadow(shadow).show(ui, add);
            let cx = KitCx {
                painter: ui.painter(),
                ppp,
            };
            paint_ring(&cx, r.response.rect, &b, Corners::All);
            return r;
        }
        let slot = ui.painter().add(Shape::Noop);
        // The box's room kept (padding and border), nothing painted.
        let frame = self.frame(state, ppp);
        let r = frame
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(frame.stroke.width, Color32::TRANSPARENT))
            .show(ui, add);
        let rect = r.response.rect;
        let shadow = (shadow != Shadow::NONE).then(|| Shape::from(shadow.as_shape(rect, b.radius)));
        self.paint_under(
            ui,
            slot,
            &Part {
                rect,
                room: rect,
                state,
                label: None,
            },
            shadow,
        );
        r
    }

    /// Paints a part's box into `slot`, a shape reserved (`Shape::Noop`)
    /// before the widget drew its content: the box ends up under the
    /// content although it is painted after it, once the part's
    /// rectangle and state are known. `first` goes under the box (a
    /// shadow). One allocation per part and frame. The `ui`'s opacity (a
    /// disabled or fading `ui`) applies once, when the box is set into
    /// its slot.
    pub fn paint_under(&self, ui: &Ui, slot: ShapeIdx, part: &Part<'_>, first: Option<Shape>) {
        let layer = ui.layer_id();
        let start = ui.ctx().graphics_mut(|g| g.entry(layer).next_idx());
        let mut painter = ui.painter().clone();
        painter.set_opacity(1.0);
        self.paint(
            &KitCx {
                painter: &painter,
                ppp: ui.ctx().pixels_per_point(),
            },
            part,
        );
        let mut shapes: Vec<Shape> = first.into_iter().collect();
        ui.ctx().graphics_mut(|g| {
            let list = g.entry(layer);
            let end = list.next_idx().0;
            for i in start.0..end {
                list.mutate_shape(ShapeIdx(i), |c| {
                    shapes.push(std::mem::replace(&mut c.shape, Shape::Noop));
                });
            }
        });
        ui.painter().set(slot, Shape::Vec(shapes));
    }
}

impl std::fmt::Debug for Component {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Component")
            .field("shape", &self.shape)
            .field("data", &self.data)
            .finish_non_exhaustive()
    }
}

/// Declares the kit's components: the `Kit` struct's fields, the
/// [`COMPONENTS`] list (token name and default handler) and their
/// compilation. One line per component, in alphabetical order, so that
/// changes adding different components do not touch the same lines.
macro_rules! components {
    ($($(#[$doc:meta])* $field:ident: $name:literal => $default:literal,)*) => {
        /// A compiled kit: every component egui-pop draws through one.
        #[derive(Clone, Debug)]
        pub struct Kit {
            $($(#[$doc])* pub $field: Component,)*
            /// The icon of each glyph role (`glyph.*`).
            pub glyphs: Glyphs,
            /// The window's own terms (`title-bar.buttons-side`,
            /// `title-bar.buttons-hover`, `window.corners`).
            pub window: WindowKit,
            /// What could not be compiled as written (an unknown shape, a
            /// bad side): fallbacks were used.
            pub problems: Vec<String>,
        }

        /// The components a kit has (their token name), with the handler
        /// drawn when a theme names none it knows.
        pub const COMPONENTS: &[(&str, &str)] = &[$(($name, $default),)*];

        impl Kit {
            fn compile_components(
                mut component: impl FnMut(&str, &str) -> Component,
                glyphs: Glyphs,
                problems: Vec<String>,
            ) -> Kit {
                Kit {
                    $($field: component($name, $default),)*
                    glyphs,
                    window: WindowKit::default(),
                    problems,
                }
            }
        }
    };
}

components! {
    /// A row of a tree, a list or a table: `selected`, `hover`, and
    /// `focus` for the cursor of a focused view.
    list_row: "list-row" => "box",
    /// A data view as a whole (tree, list, table, library grid): its
    /// outline while it has the keyboard focus, painted over its rows.
    list_view: "list-view" => "plain",
    /// An item of a menu (command, check item, submenu): egui lays it out
    /// and takes its clicks, the kit paints its box under it; `selected`
    /// while its submenu is open, its shortcut in the `muted` tone.
    menu_item: "menu-item" => "box",
    /// A meter's track (its fill is a `progress-fill`).
    meter: "meter" => "track",
    /// A meter's threshold, a line across its track.
    meter_tick: "meter-tick" => "tick",
    /// A progress bar's track (its fill and sections are `progress-fill`).
    progress_bar: "progress-bar" => "track",
    /// A bar's caption: its name, and its value text in the `muted` tone.
    progress_caption: "progress-caption" => "plain",
    /// A stretch of a track, by tone: a bar's fill and sections, a
    /// meter's fill, an indeterminate bar's segment.
    progress_fill: "progress-fill" => "span",
    /// A thin progress line's track (under a title, along a viewport).
    progress_line: "progress-line" => "track",
    /// A progress ring, colours only (its arc is drawn by egui-pop): the
    /// track in `fill`, the arc in `indicator-color`, by tone.
    progress_ring: "progress-ring" => "plain",
    /// A side area of the workbench: egui draws it and its separator line
    /// (none when `border-width` is 0).
    side_area: "side-area" => "plain",
    /// A shape of a skeleton standing in for loading content.
    skeleton: "skeleton" => "box",
    /// The band crossing a skeleton's shapes, painted where they meet.
    skeleton_shimmer: "skeleton-shimmer" => "box",
    /// A slider's bar: its track and its label; `hover`, `active` while
    /// dragged, `focus`. A range slider's bar is in the states of both
    /// its thumbs.
    slider: "slider" => "box",
    /// A slider's fill, a strip along the bar's bottom (`height`, else
    /// 15 % of the bar); not drawn on a gradient track.
    slider_fill: "slider-fill" => "track",
    /// A slider's thumb, a line across the bar, in its own thumb's state.
    slider_thumb: "slider-thumb" => "tick",
    /// The square button at a slider's end, its icon in `text`;
    /// `selected` while its toggle is on.
    slider_toggle: "slider-toggle" => "box",
    /// A slider's value text, in the `warning` tone past the soft range.
    slider_value: "slider-value" => "plain",
    /// A spinner, colours only like `progress-ring`; its label in `text`.
    spinner: "spinner" => "plain",
    /// A status bar indicator, colours only: its text by tone, its
    /// progress line (`border-color` the track, `indicator-*` the fill).
    status_indicator: "status-indicator" => "plain",
    /// An icon of the activity bar, its icon in `text`: `selected` while
    /// its panel is shown (never `hover` then).
    activity_bar_item: "activity-bar-item" => "box",
    /// A side area's resize grip at the middle of its edge (`width` its
    /// thickness, `height` its length); egui takes the drag.
    area_grip: "area-grip" => "box",
    /// A folded side area's strip (`width`); a click unfolds the area.
    area_strip: "area-strip" => "plain",
    /// The frame of an editor's code assist popups (completions,
    /// actions, hover cards).
    assist_card: "assist-card" => "card",
    /// A push button. egui's own widgets take their states from it:
    /// rest, hover, active, and selected for an open menu or combo box.
    button: "button" => "box",
    /// A button running a command: egui lays it out and takes its clicks,
    /// the kit paints its box; egui greys it when disabled.
    command_button: "command-button" => "box",
    /// A command in the command palette: egui lays it out, the kit paints
    /// its box under it; `selected` for the one Enter runs, its matches in
    /// the `accent` tone, its keys in the `muted` tone.
    command_palette_row: "command-palette-row" => "box",
    /// What dims the interface behind a modal dialog: its `fill` at
    /// `opacity` (egui paints it).
    backdrop: "backdrop" => "plain",
    /// An icon button on a header band (a palette's close, roll and pin, a
    /// dialog's close), its icon in `text`; `selected` for a pinned
    /// palette.
    band_icon_button: "band-icon-button" => "box",
    /// A pane of the library's comparison: `selected` for the best
    /// image, its ring painted again over the image.
    compare_pane: "compare-pane" => "box",
    /// A dialog's frame.
    dialog: "dialog" => "card",
    /// A dialog's title row, colours only: its title in `text`, on the
    /// header band of a movable dialog (`padding-y` below it).
    dialog_header: "dialog-header" => "plain",
    /// Where a dragged tab or palette would dock (its fill at `opacity`);
    /// `disabled` when refused, its label saying why.
    dock_preview: "dock-preview" => "box",
    /// The frame around a text or code editor; `focus` while it has the
    /// keyboard.
    editor_frame: "editor-frame" => "card",
    /// An empty place: its box (none in the default kit) around the
    /// icon, title, sentence and action; the icon and the sentence in
    /// `text`, the icon in the `danger` tone when something went wrong.
    empty_state: "empty-state" => "card",
    /// A text field. egui's text fields take their background from it.
    field: "field" => "box",
    /// The card standing in for a part that panicked.
    fault_card: "fault-card" => "card",
    /// An editor's find and replace bar.
    find_bar: "find-bar" => "card",
    /// An icon button of the find bar (previous, next, close).
    find_bar_button: "find-bar-button" => "box",
    /// The strip at the top of a full screen window, its way out.
    fullscreen_exit: "fullscreen-exit" => "card",
    /// The band at the top of a movable dialog and of a palette, dragged to
    /// move it: `hover` under the pointer, `active` while dragged.
    header_band: "header-band" => "band",
    /// A button made of an icon alone (`IconWidget::sense_click`), its
    /// icon in `text`, by tone; `selected` while it is on.
    icon_button: "icon-button" => "box",
    /// A count or short text badge (a micro-indicator): a pill by tone.
    indicator_badge: "indicator-badge" => "pill",
    /// The target of an interactive micro-indicator: `hover`, `active`,
    /// `focus`, `disabled`, `selected` for a toggle that is on; its
    /// `text` is an icon mark's colour.
    indicator_button: "indicator-button" => "box",
    /// A dot or an LED, colours only: its colour in `fill` by tone, an
    /// LED that is off a ring in `border-color`.
    indicator_dot: "indicator-dot" => "plain",
    /// A flag, colours only: its colour in `fill` by tone.
    indicator_flag: "indicator-flag" => "plain",
    /// The ring separating an overhanging mark from its host, colours
    /// only (`ring-color`, `ring-width`).
    indicator_outline: "indicator-outline" => "plain",
    /// A colour swatch's box: its outline and corners over the colour;
    /// `text` the slash of no colour and the dash of mixed ones.
    indicator_swatch: "indicator-swatch" => "box",
    /// A job's log console, as plain lines.
    job_log: "job-log" => "card",
    /// A pick, reject or colour label toggle: `selected` while on.
    mark_toggle: "mark-toggle" => "box",
    /// A floating palette's window.
    palette_window: "palette-window" => "card",
    /// A pill over a thumbnail: a badge (by tone) or a stack's count.
    pill: "pill" => "pill",
    /// A popover's or a menu's surface.
    popover: "popover" => "card",
    /// A viewport's pixel readout; its label is the readout's text.
    readout: "readout" => "card",
    /// The library's search field: `focus` while its text has the
    /// keyboard.
    search_field: "search-field" => "card",
    /// A tab of the workbench's tab bars.
    tab: "tab" => "box",
    /// The bar behind a row of tabs.
    tab_bar: "tab-bar" => "plain",
    /// A column's header in a data table: `hover` when the column sorts,
    /// `selected` when the table is sorted by it.
    table_header_cell: "table-header-cell" => "box",
    /// Behind every other row of a data table.
    table_stripe: "table-stripe" => "plain",
    /// A tag or filter chip: selected, hover, focus.
    tag_chip: "tag-chip" => "pill",
    /// A cell of the library's grid: `selected`, `hover` (never while
    /// selected), `focus` for the cursor of a focused grid.
    thumb_cell: "thumb-cell" => "box",
    /// What a library cell lays over its image, by tone: `muted` a
    /// rejected image's veil, `accent` a stack member's bar.
    thumb_layer: "thumb-layer" => "box",
    /// The box behind a cell's flag, labels and stars.
    thumb_overlay_badge: "thumb-overlay-badge" => "box",
    /// A thumbnail of the viewport crate: selected, hover, focus.
    thumbnail: "thumbnail" => "box",
    /// A window's own title bar; its label is the window's title.
    title_bar: "title-bar" => "plain",
    /// Where a drag over a tree drops: around the target row, or a line
    /// between rows (a part with no height); `disabled` when refused.
    tree_drop_marker: "tree-drop-marker" => "outline",
    /// The frame around a terminal's grid; `focus` while it has the
    /// keyboard. Its fill is the grid's default background.
    terminal_frame: "terminal-frame" => "card",
    /// A tip's card.
    tip_card: "tip-card" => "card",
    /// A toast's frame (its level's tone is its icon's); its `text` is
    /// the message's.
    toast: "toast" => "card",
    /// An image viewport's outline while it has the keyboard focus.
    viewport_focus: "viewport-focus" => "plain",
    /// Minimize and maximize (the glyph is drawn in the box's `text`),
    /// in the tones `warning` and `success`; `traffic-light` draws
    /// circles.
    window_button: "window-button" => "plain",
    /// The close button, in the tone `danger`.
    window_close: "window-close" => "plain",
}

type Handlers = BTreeMap<String, Arc<dyn Primitive<ComponentKit>>>;

/// The handlers a kit's `shape` tokens can name, per component.
#[derive(Clone)]
pub struct Shapes(BTreeMap<String, Handlers>);

impl Default for Shapes {
    /// The handlers egui-pop ships: `plain` for every component, and each
    /// handler module's own (`shapes::built_in`).
    fn default() -> Self {
        let mut shapes = Shapes(BTreeMap::new());
        for (component, _) in COMPONENTS {
            shapes.insert(component, "plain", Arc::new(shapes::Plain));
        }
        shapes::built_in(&mut shapes);
        shapes
    }
}

impl Shapes {
    /// The handler a component's `shape` names: its own, else one its
    /// base has (`<component>.based-on`, followed along a chain), so that
    /// a component based on another is drawn as it is.
    fn find(
        &self,
        t: &Tokens,
        component: &str,
        name: &str,
    ) -> Option<Arc<dyn Primitive<ComponentKit>>> {
        let mut at = component.to_string();
        let mut seen = Vec::new();
        loop {
            if let Some(h) = self.0.get(&at).and_then(|h| h.get(name)) {
                return Some(h.clone());
            }
            seen.push(at.clone());
            match string(t, &format!("{at}.{}", egui_pop_tokens::BASED_ON)) {
                Some(base) if !seen.contains(&base) => at = base,
                _ => return None,
            }
        }
    }

    /// Adds a handler for a component under a name.
    pub fn insert(
        &mut self,
        component: &str,
        name: &str,
        handler: Arc<dyn Primitive<ComponentKit>>,
    ) {
        self.0
            .entry(component.into())
            .or_default()
            .insert(name.into(), handler);
    }
}

impl std::fmt::Debug for Shapes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut m = f.debug_map();
        for (c, h) in &self.0 {
            m.entry(c, &h.keys().collect::<Vec<_>>());
        }
        m.finish()
    }
}

fn shapes_id() -> Id {
    Id::new("egui_pop::kit::Shapes")
}

fn kit_id() -> Id {
    Id::new("egui_pop::kit::Kit")
}

/// Registers a handler for a component (one of [`COMPONENTS`]) under a
/// name its `shape` token can give. In use from the next theme install.
pub fn register_shape(
    ctx: &Context,
    component: &str,
    name: &str,
    handler: Arc<dyn Primitive<ComponentKit>>,
) {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_insert_with(shapes_id(), Shapes::default)
            .insert(component, name, handler);
    });
}

/// Registers a tab handler ([`register_shape`] for `tab`).
pub fn register_tab_shape(ctx: &Context, name: &str, handler: Arc<dyn Primitive<ComponentKit>>) {
    register_shape(ctx, "tab", name, handler);
}

fn string(t: &Tokens, path: &str) -> Option<String> {
    match t.resolved().value(path) {
        Some(Value::Other(serde_json::Value::String(s))) => Some(s.clone()),
        _ => None,
    }
}

impl Kit {
    /// Compiles the kit a theme describes, with the handlers given.
    pub fn compile(t: &Tokens, shapes: &Shapes) -> Kit {
        let mut problems = Vec::new();
        let component = |name: &str, default: &str| {
            let handlers = &shapes.0[name];
            let data = ComponentKit::compile(t, name, &mut problems);
            let asked = string(t, &format!("{name}.shape")).unwrap_or_else(|| default.into());
            let (shape, handler) = match shapes.find(t, name, &asked) {
                Some(h) => (asked, h),
                None => {
                    problems.push(format!(
                        "{name}.shape: no handler named `{asked}` (known: {}), `{default}` is drawn",
                        handlers.keys().cloned().collect::<Vec<_>>().join(", ")
                    ));
                    (default.to_string(), handlers[default].clone())
                }
            };
            let egui_frame = Arc::ptr_eq(&handler, &shapes::CARD);
            Component {
                data,
                shape,
                handler,
                egui_frame,
            }
        };
        let mut kit = Kit::compile_components(component, Glyphs::default(), Vec::new());
        kit.glyphs = Glyphs::compile(t, &mut problems);
        kit.window = WindowKit::compile(t, &mut problems);
        problems.extend(egui_losses(t, &kit));
        kit.problems = problems;
        kit
    }

    /// The icon a kit draws for a glyph role.
    pub fn glyph(&self, glyph: Glyph) -> crate::icon::Icon {
        self.glyphs.get(glyph)
    }

    /// The kit installed in a context, or the one of the installed
    /// tokens, compiled on first use.
    pub fn of(ctx: &Context) -> Arc<Kit> {
        if let Some(k) = ctx.data(|d| d.get_temp::<Arc<Kit>>(kit_id())) {
            return k;
        }
        install(ctx, &Tokens::of(ctx))
    }
}

/// What egui's own widgets, drawn from `button` and `field`
/// (`theme::visuals_with`), and egui's separator of a side area cannot
/// show of them: a kit giving it is told, rather than seeing it dropped
/// in silence.
fn egui_losses(t: &Tokens, kit: &Kit) -> Vec<String> {
    let mut lost = Vec::new();
    for (state, name) in [
        (State::REST, "rest"),
        (State::HOVER, "hover"),
        (State::ACTIVE, "active"),
        (State::SELECTED, "selected"),
    ] {
        let b = kit.button.style(state);
        if b.indicator_size > 0.0 && b.indicator_color.a() > 0 {
            lost.push(format!(
                "button.{name}: egui's widgets draw no indicator (kept for egui-pop's own buttons)"
            ));
        }
        if b.ring_width > 0.0 && b.ring_color.a() > 0 {
            lost.push(format!(
                "button.{name}: egui's widgets draw no focus ring (kept for egui-pop's own buttons)"
            ));
        }
    }
    let (f, b) = (kit.field.style(State::REST), kit.button.style(State::REST));
    if (f.border_color, f.border_width, f.radius) != (b.border_color, b.border_width, b.radius) {
        lost.push(
            "field.rest: egui's text fields take their outline from `button` (egui shares widget states)"
                .into(),
        );
    }
    if f.text != b.text {
        lost.push("field.rest.text: egui's text fields take the button's text colour".into());
    }
    // egui draws a side area's separator with its non-interactive
    // widgets' stroke: the button's width at rest, `border.default`.
    let side = kit.side_area.style(State::REST);
    if side.border_width > 0.0 && side.border_width != b.border_width {
        lost.push(format!(
            "side-area.rest.border-width: egui draws the separator line {} wide (the button's at rest); only 0 hides it",
            b.border_width
        ));
    }
    if side.border_width > 0.0 && side.border_color.a() > 0 && side.border_color != t.border.default
    {
        lost.push(
            "side-area.rest.border-color: egui draws the separator line in `border.default`".into(),
        );
    }
    lost
}

/// Compiles the kit of `tokens` with the registered handlers and
/// installs it. Called by `theme::install_tokens`, between frames.
pub fn install(ctx: &Context, tokens: &Tokens) -> Arc<Kit> {
    let shapes = ctx.data(|d| d.get_temp::<Shapes>(shapes_id()).unwrap_or_default());
    let kit = Arc::new(Kit::compile(tokens, &shapes));
    for p in &kit.problems {
        log::warn!(target: "egui_pop", "kit: {p}");
    }
    ctx.data_mut(|d| d.insert_temp(kit_id(), kit.clone()));
    kit
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icon::Icon;
    use egui::pos2;
    use egui_pop_tokens::{ThemeSpec, Themes};

    fn tokens_with(json: &str) -> Tokens {
        let mut themes = Themes::default();
        themes.add_text("kit-test", json).expect("test theme reads");
        Tokens::new(themes.resolve(&ThemeSpec::new("kit-test")))
    }

    #[test]
    fn the_default_kit_compiles_without_problems() {
        let kit = Kit::compile(&Tokens::default(), &Shapes::default());
        assert!(kit.problems.is_empty(), "{:?}", kit.problems);
        assert_eq!(kit.tab.shape, "box");
        assert!(kit.tab_bar.data.height > 0.0);
    }

    #[test]
    fn shadows_come_from_tokens_with_egui_s_own_look_by_default() {
        let themes = Themes::default();
        for (name, dark) in [("pop-dark", true), ("pop-light", false)] {
            let t = Tokens::new(themes.resolve(&ThemeSpec::new(name)));
            let egui = if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            };
            let v = crate::theme::visuals(&t);
            assert_eq!(v.window_shadow, egui.window_shadow, "{name}");
            assert_eq!(v.popup_shadow, egui.popup_shadow, "{name}");
            let kit = Kit::compile(&t, &Shapes::default());
            assert_eq!(kit.side_area.data.shadow, t.shadow.side, "{name}");
            assert_ne!(kit.side_area.data.shadow, Shadow::NONE, "{name}");
            // A component without shadow terms casts none.
            assert_eq!(kit.tab.data.shadow, Shadow::NONE);
        }
        let t = tokens_with(
            r##"{
              "$extends": "pop-dark",
              "side-area": {
                "shadow-offset-x": {"$type": "dimension", "$value": {"value": 3, "unit": "px"}},
                "shadow-spread": {"$type": "dimension", "$value": {"value": -2, "unit": "px"}},
                "shadow-color": {"$type": "color", "$value": "#ff000080"}
              }
            }"##,
        );
        let s = Kit::compile(&t, &Shapes::default()).side_area.data.shadow;
        assert_eq!(s.offset, [3, 0]);
        assert_eq!(s.spread, 0, "egui's spread is never negative");
        assert_eq!(s.blur, 32);
        assert_eq!(s.color, Color32::from_rgba_unmultiplied(255, 0, 0, 128));
    }

    #[test]
    fn every_built_in_theme_compiles_its_kit() {
        let themes = Themes::default();
        for name in themes.names() {
            let t = Tokens::new(themes.resolve(&ThemeSpec::new(name)));
            let kit = Kit::compile(&t, &Shapes::default());
            assert!(kit.problems.is_empty(), "{name}: {:?}", kit.problems);
        }
    }

    #[test]
    fn states_merge_in_order_and_only_what_they_give() {
        let t = tokens_with(
            r##"
            $extends: "pop-dark"
            tab:
              rest:
                fill: {$type: "color", $value: "#101010"}
                text: {$type: "color", $value: "#808080"}
              selected:
                text: {$type: "color", $value: "#ffffff"}
              hover:
                fill: {$type: "color", $value: "#202020"}
            "##,
        );
        let kit = Kit::compile(&t, &Shapes::default());
        let s = &kit.tab.data.states;
        let hex = |c: Color32| format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
        assert_eq!(hex(s.get(State::REST).fill), "#101010");
        assert_eq!(hex(s.get(State::SELECTED).text), "#ffffff");
        // What `selected` does not give stays the rest's.
        assert_eq!(s.get(State::SELECTED).radius, s.get(State::REST).radius);
        // Hover applies after selected: its fill (this theme's) and its
        // text (pop-dark's) win; selected's text shows when not hovered.
        let both = s.get(State::SELECTED | State::HOVER);
        assert_eq!(hex(both.fill), "#202020");
        assert_eq!(both.text, s.get(State::HOVER).text);
    }

    #[test]
    fn a_label_goes_where_align_says_inside_the_room() {
        let t = tokens_with(
            r#"
            $extends: "pop-dark"
            title-bar:
              align: {$type: "string", $value: "start"}
            "#,
        );
        let kit = Kit::compile(&t, &Shapes::default());
        assert_eq!(kit.title_bar.data.align, Align::Start);
        let bar = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 32.0));
        let room = Rect::from_min_max(pos2(100.0, 0.0), pos2(600.0, 32.0));
        let part = Part {
            rect: bar,
            room,
            state: State::REST,
            label: None,
        };
        let pad = kit.title_bar.data.padding_x;
        let size = Vec2::new(120.0, 16.0);
        // Start: right after the leading items and the padding.
        assert_eq!(kit.title_bar.label_rect(&part, size).left(), 100.0 + pad);
        // Centre (the default kit): on the bar's centre, which fits here...
        let centred = Kit::compile(&Tokens::default(), &Shapes::default());
        assert_eq!(centred.title_bar.label_rect(&part, size).center().x, 400.0);
        // ...and moved into the room when it does not.
        let narrow = Part {
            room: Rect::from_min_max(pos2(500.0, 0.0), pos2(780.0, 32.0)),
            ..part
        };
        assert_eq!(
            centred.title_bar.label_rect(&narrow, size).left(),
            500.0 + pad
        );
    }

    #[test]
    fn a_focus_ring_is_data_of_the_focus_state() {
        let t = tokens_with(
            r#"
            $extends: "pop-dark"
            tab:
              focus:
                ring-color: {$type: "color", $value: "{focus.ring}"}
                ring-width: {$type: "dimension", $value: {value: 2, unit: "px"}}
                ring-offset: {$type: "dimension", $value: {value: 1, unit: "px"}}
            "#,
        );
        let kit = Kit::compile(&t, &Shapes::default());
        assert!(kit.problems.is_empty(), "{:?}", kit.problems);
        let s = &kit.tab.data.states;
        assert_eq!(s.get(State::REST).ring_width, 0.0);
        let focus = s.get(State::FOCUS | State::HOVER);
        assert_eq!((focus.ring_width, focus.ring_offset), (2.0, 1.0));
        assert_eq!(focus.ring_color, t.focus_ring);
    }

    #[test]
    fn every_glyph_role_has_its_default_icon_in_the_default_kit() {
        let kit = Kit::compile(&Tokens::default(), &Shapes::default());
        for g in Glyph::ALL {
            assert_eq!(kit.glyph(*g), g.default_icon(), "{}", g.name());
        }
        assert_eq!(kit.glyph(Glyph::DisclosureClosed), Icon::TriangleRight);
    }

    #[test]
    fn a_kit_remaps_a_glyph_and_an_unknown_icon_is_reported() {
        let t = tokens_with(
            r#"
            $extends: "pop-dark"
            glyph:
              disclosure-closed: {$type: "string", $value: "chevron-right"}
              check: {$type: "string", $value: "tick-mark"}
            "#,
        );
        let kit = Kit::compile(&t, &Shapes::default());
        assert_eq!(kit.glyph(Glyph::DisclosureClosed), Icon::ChevronRight);
        assert_eq!(kit.glyph(Glyph::Check), Icon::Check);
        assert_eq!(kit.problems.len(), 1, "{:?}", kit.problems);
        assert!(kit.problems[0].starts_with("glyph.check: no icon named `tick-mark`"));
    }

    #[test]
    fn the_led_and_flag_shapes_are_glyphs_a_kit_remaps_but_never_merges() {
        // The default shapes differ where they must.
        let kit = Kit::compile(&Tokens::default(), &Shapes::default());
        for (a, b) in egui_pop_tokens::kit::DISTINCT_GLYPHS {
            let role = |n: &str| *Glyph::ALL.iter().find(|g| g.name() == n).unwrap();
            assert_ne!(kit.glyph(role(a)), kit.glyph(role(b)), "{a} and {b}");
        }
        // A kit gives the LEDs other shapes.
        let t = tokens_with(
            r#"
            $extends: "pop-dark"
            glyph:
              led-error: {$type: "string", $value: "close"}
              flag-pinned: {$type: "string", $value: "lock"}
            "#,
        );
        let kit = Kit::compile(&t, &Shapes::default());
        assert_eq!(kit.glyph(Glyph::LedError), Icon::Close);
        assert_eq!(kit.glyph(Glyph::FlagPinned), Icon::Lock);
        assert!(kit.problems.is_empty(), "{:?}", kit.problems);
        // Two states that would differ by colour alone go back to
        // egui-pop's shapes, and the kit is told.
        let t = tokens_with(
            r#"
            $extends: "pop-dark"
            glyph:
              led-warning: {$type: "string", $value: "square"}
            "#,
        );
        let kit = Kit::compile(&t, &Shapes::default());
        assert_eq!(kit.glyph(Glyph::LedWarning), Icon::Triangle);
        assert_eq!(kit.glyph(Glyph::LedError), Icon::Square);
        assert_eq!(kit.problems.len(), 1, "{:?}", kit.problems);
        assert!(
            kit.problems[0].starts_with("glyph.led-warning and glyph.led-error both draw"),
            "{:?}",
            kit.problems
        );
        // A busy LED may share the on LED's shape: it pulses.
        assert_eq!(kit.glyph(Glyph::LedBusy), kit.glyph(Glyph::LedOn));
    }

    #[test]
    fn the_default_rows_alias_the_system_tokens() {
        let t = Tokens::default();
        let kit = Kit::compile(&t, &Shapes::default());
        let row = &kit.list_row;
        assert_eq!(row.shape, "box");
        assert_eq!(row.style(State::REST).fill, Color32::TRANSPARENT);
        assert_eq!(row.style(State::REST).text, t.text.default);
        let selected = row.style(State::SELECTED);
        assert_eq!((selected.fill, selected.text), (t.selection, t.text.strong));
        assert_eq!(row.style(State::HOVER).fill, t.surface.hover);
        // The cursor: a ring one pixel inside the row's edge.
        let cursor = row.style(State::SELECTED | State::FOCUS);
        assert_eq!(cursor.fill, t.selection);
        assert_eq!(
            (cursor.ring_color, cursor.ring_width, cursor.ring_offset),
            (t.focus_ring, 1.0, -2.0)
        );
        let view = kit.list_view.style(State::FOCUS);
        assert_eq!((view.ring_width, view.ring_offset), (1.0, -1.0));
        assert_eq!(kit.list_view.style(State::REST).ring_width, 0.0);
        assert_eq!(kit.tree_drop_marker.shape, "outline");
        assert_eq!(
            kit.tree_drop_marker.style(State::DISABLED).border_color,
            t.status.danger
        );
        assert_eq!(kit.table_stripe.style(State::REST).fill, t.surface.raised);
        assert_eq!(kit.table_header_cell.data.align, Align::Start);
    }

    #[test]
    fn the_default_sliders_alias_the_system_tokens() {
        let t = Tokens::default();
        let kit = Kit::compile(&t, &Shapes::default());
        let bar = &kit.slider;
        assert_eq!(bar.shape, "box");
        assert_eq!(bar.style(State::REST).fill, t.border.default);
        assert_eq!(bar.style(State::REST).border_width, 0.0);
        assert_eq!(bar.data.align, Align::Start);
        assert_eq!(bar.data.padding_x, t.space.s);
        let hover = bar.style(State::HOVER);
        assert_eq!(
            (hover.border_color, hover.border_width),
            (t.border.strong, 1.0)
        );
        // Focused: the ring inside the bar, no hover border under it.
        let focus = bar.style(State::FOCUS | State::HOVER);
        assert_eq!(
            (focus.ring_color, focus.ring_width, focus.ring_offset),
            (t.focus_ring, 2.0, -2.0)
        );
        assert_eq!(focus.border_width, 0.0);
        assert_eq!(kit.slider_fill.style(State::REST).fill, t.accent.default);
        assert_eq!(kit.slider_thumb.style(State::REST).fill, t.text.strong);
        assert_eq!(kit.slider_thumb.data.width, 2.0);
        let value = &kit.slider_value;
        assert_eq!(value.data.align, Align::End);
        assert_eq!(value.style(State::REST).text, t.text.default);
        assert_eq!(
            value.style_toned(Tone::Warning, State::FOCUS).text,
            t.status.warning
        );
        let toggle = &kit.slider_toggle;
        assert_eq!(toggle.style(State::REST).text, t.text.default);
        assert_eq!(toggle.style(State::SELECTED).text, t.accent.default);
        assert_eq!(kit.slider.style(State::REST).fill, t.border.default);
    }

    #[test]
    fn an_icon_button_s_states_win_over_its_tone() {
        let t = Tokens::default();
        let kit = Kit::compile(&t, &Shapes::default());
        let b = &kit.icon_button;
        assert_eq!(b.style(State::REST).fill, Color32::TRANSPARENT);
        assert_eq!(b.style(State::REST).text, t.text.default);
        assert_eq!(
            b.style_toned(Tone::Danger, State::REST).text,
            t.status.danger
        );
        assert_eq!(
            b.style_toned(Tone::Danger, State::HOVER).text,
            t.text.strong
        );
        assert_eq!(
            b.style_toned(Tone::Accent, State::DISABLED).text,
            t.text.muted
        );
        assert_eq!(b.style(State::SELECTED).text, t.accent.default);
        let focus = b.style(State::FOCUS);
        assert_eq!(
            (focus.ring_color, focus.ring_width, focus.ring_offset),
            (t.focus_ring, 1.5, 1.0)
        );
    }

    #[test]
    fn the_default_placeholders_alias_the_system_tokens() {
        let t = Tokens::default();
        let kit = Kit::compile(&t, &Shapes::default());
        assert_eq!(kit.skeleton.style(State::REST).fill, t.surface.active);
        assert_eq!(kit.skeleton.style(State::REST).radius, t.radius.widget);
        assert_eq!(
            kit.skeleton_shimmer.style(State::REST).fill,
            t.surface.hover
        );
        let empty = &kit.empty_state;
        assert_eq!(empty.shape, "card");
        assert_eq!(empty.data.padding_y, t.space.l);
        assert_eq!(empty.style(State::REST).fill, Color32::TRANSPARENT);
        assert_eq!(empty.style(State::REST).text, t.text.muted);
        assert_eq!(
            empty.style_toned(Tone::Danger, State::REST).text,
            t.status.danger
        );
    }

    #[test]
    fn tones_merge_after_rest_and_states_keep_what_they_give() {
        let t = tokens_with(
            r##"
            $extends: "pop-dark"
            tab:
              rest:
                fill: {$type: "color", $value: "#101010"}
              hover:
                fill: {$type: "color", $value: "#202020"}
              disabled:
                text: {$type: "color", $value: "#303030"}
              tone:
                danger:
                  fill: {$type: "color", $value: "#ff0000"}
                  text: {$type: "color", $value: "#ffffff"}
                  hover:
                    fill: {$type: "color", $value: "#ff4040"}
            "##,
        );
        let kit = Kit::compile(&t, &Shapes::default());
        let hex = |c: Color32| format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
        let tab = &kit.tab;
        assert!(tab.data.states.toned());
        let danger = |s: State| tab.style_toned(Tone::Danger, s);
        assert_eq!(hex(danger(State::REST).fill), "#ff0000");
        // The tone's own hover wins over the component's hover.
        assert_eq!(hex(danger(State::HOVER).fill), "#ff4040");
        // A state the tone does not give still applies over it.
        assert_eq!(hex(danger(State::DISABLED).text), "#303030");
        assert_eq!(hex(danger(State::DISABLED).fill), "#ff0000");
        // A tone the component does not give is its own box.
        assert_eq!(
            tab.style_toned(Tone::Info, State::HOVER),
            tab.style(State::HOVER)
        );
        assert_eq!(
            tab.style_toned(Tone::Neutral, State::REST),
            tab.style(State::REST)
        );
        // The tone travels in the state: handlers honour it unchanged.
        let s = State::HOVER.toned(Tone::Danger);
        assert_eq!(s.tone(), Tone::Danger);
        assert!(s.contains(State::HOVER));
        assert_eq!(tab.style(s), danger(State::HOVER));
        assert_eq!(s.toned(Tone::Neutral), State::HOVER);
    }

    #[test]
    fn a_component_without_tones_has_no_table() {
        let kit = Kit::compile(&Tokens::default(), &Shapes::default());
        assert!(!kit.tab.data.states.toned());
        assert_eq!(
            kit.tab.style_toned(Tone::Danger, State::HOVER),
            kit.tab.style(State::HOVER)
        );
        // The default kit's toned components alias the status tokens.
        let t = Tokens::default();
        let fill = |tone| kit.progress_fill.style_toned(tone, State::REST).fill;
        assert_eq!(fill(Tone::Neutral), t.accent.default);
        assert_eq!(fill(Tone::Accent), t.accent.default);
        assert_eq!(fill(Tone::Info), t.status.info);
        assert_eq!(fill(Tone::Success), t.status.success);
        assert_eq!(fill(Tone::Warning), t.status.warning);
        assert_eq!(fill(Tone::Danger), t.status.danger);
        assert_eq!(fill(Tone::Muted), t.text.muted);
    }

    #[test]
    fn the_default_chrome_aliases_the_system_tokens() {
        let t = Tokens::default();
        let kit = Kit::compile(&t, &Shapes::default());
        assert_eq!(kit.header_band.shape, "band");
        assert_eq!(kit.header_band.style(State::REST).fill, t.surface.raised);
        assert_eq!(kit.header_band.data.padding_y, t.space.xs);
        assert_eq!(kit.dialog_header.data.padding_y, t.space.m);
        let pinned = kit.band_icon_button.style(State::SELECTED);
        assert_eq!(
            (pinned.fill, pinned.text),
            (t.surface.raised, t.text.strong)
        );
        let focus = kit.band_icon_button.style(State::FOCUS);
        assert_eq!((focus.ring_color, focus.ring_width), (t.focus_ring, 2.0));
        // A command button looks like egui's buttons, which `button` draws.
        for s in [State::REST, State::HOVER, State::ACTIVE] {
            let (c, b) = (kit.command_button.style(s), kit.button.style(s));
            assert_eq!(
                (c.fill, c.border_color, c.text),
                (b.fill, b.border_color, b.text)
            );
        }
        let row = &kit.command_palette_row;
        assert_eq!(row.style(State::SELECTED).fill, t.selection);
        assert_eq!(row.style_toned(Tone::Muted, State::REST).text, t.text.muted);
        assert_eq!(
            kit.activity_bar_item.style(State::SELECTED).fill,
            t.accent.subtle
        );
        let refused = kit.dock_preview.style(State::DISABLED);
        assert_eq!(refused.border_color, t.status.danger);
        assert_eq!(kit.dock_preview.data.opacity, 0.5);
    }

    #[test]
    fn egui_s_widgets_take_their_states_from_the_kit_s_button() {
        let t = tokens_with(
            r##"
            $extends: "pop-dark"
            button:
              hover:
                fill: {$type: "color", $value: "#123456"}
              rest:
                radius: {$type: "dimension", $value: {value: 9, unit: "px"}}
            field:
              rest:
                fill: {$type: "color", $value: "#0a0b0c"}
            "##,
        );
        let ctx = Context::default();
        crate::theme::install_tokens(&ctx, t);
        let v = ctx.global_style().visuals.clone();
        assert_eq!(
            v.widgets.hovered.bg_fill,
            Color32::from_rgb(0x12, 0x34, 0x56)
        );
        assert_eq!(
            v.widgets.inactive.corner_radius,
            egui::CornerRadius::same(9)
        );
        assert_eq!(
            v.text_edit_bg_color,
            Some(Color32::from_rgb(0x0a, 0x0b, 0x0c))
        );
        // What egui cannot draw: the field's radius differs from the button's.
        let kit = Kit::of(&ctx);
        assert!(
            kit.problems
                .iter()
                .any(|p| p.starts_with("field.rest: egui's text fields")),
            "{:?}",
            kit.problems
        );
    }

    #[test]
    fn a_button_indicator_is_reported_as_beyond_egui_s_widgets() {
        let t = tokens_with(
            r#"
            $extends: "pop-dark"
            button:
              selected:
                indicator-color: {$type: "color", $value: "{accent.default}"}
                indicator-size: {$type: "dimension", $value: {value: 2, unit: "px"}}
            "#,
        );
        let kit = Kit::compile(&t, &Shapes::default());
        assert_eq!(kit.problems.len(), 1, "{:?}", kit.problems);
        assert!(kit.problems[0].starts_with("button.selected: egui's widgets draw no indicator"));
    }

    #[test]
    fn a_side_area_separator_egui_cannot_draw_is_reported() {
        let t = tokens_with(
            r##"
            $extends: "pop-dark"
            side-area:
              rest:
                border-width: {$type: "dimension", $value: {value: 3, unit: "px"}}
                border-color: {$type: "color", $value: "#ff0000"}
            "##,
        );
        let kit = Kit::compile(&t, &Shapes::default());
        assert_eq!(kit.problems.len(), 2, "{:?}", kit.problems);
        assert!(kit.problems[0].starts_with("side-area.rest.border-width"));
        assert!(kit.problems[1].starts_with("side-area.rest.border-color"));
        // Hiding the line is what egui honours: nothing to report.
        let t = tokens_with(
            r##"
            $extends: "pop-dark"
            side-area:
              rest:
                border-width: {$type: "dimension", $value: {value: 0, unit: "px"}}
            "##,
        );
        assert!(Kit::compile(&t, &Shapes::default()).problems.is_empty());
    }

    #[test]
    fn a_bad_align_is_reported() {
        let t = tokens_with(
            r#"
            $extends: "pop-dark"
            title-bar:
              align: {$type: "string", $value: "middle"}
            "#,
        );
        let kit = Kit::compile(&t, &Shapes::default());
        assert_eq!(kit.title_bar.data.align, Align::Centre);
        assert!(
            kit.problems[0].starts_with("title-bar.align: `middle`"),
            "{:?}",
            kit.problems
        );
    }

    #[test]
    fn an_unknown_shape_is_reported_and_the_default_drawn() {
        let t = tokens_with(
            r#"
            $extends: "pop-dark"
            tab:
              shape: {$type: "string", $value: "zigzag"}
            "#,
        );
        let kit = Kit::compile(&t, &Shapes::default());
        assert_eq!(kit.tab.shape, "box");
        assert!(
            kit.problems[0].starts_with("tab.shape: no handler named `zigzag`"),
            "{:?}",
            kit.problems
        );
    }

    #[test]
    fn a_frame_is_the_box_as_egui_draws_it() {
        let t = Tokens::default();
        let kit = Kit::compile(&t, &Shapes::default());
        assert_eq!(kit.dialog.shape, "card");
        let f = kit.dialog.frame(State::REST, 1.0);
        assert_eq!(f.fill, t.surface.overlay);
        assert_eq!(f.stroke, Stroke::new(1.0, t.border.default));
        // Its border is whole physical pixels: one at 1.25, not 1.25.
        assert_eq!(kit.dialog.frame(State::REST, 1.25).stroke.width, 0.8);
        assert_eq!(f.corner_radius, egui::CornerRadius::from(t.radius.window));
        assert_eq!(f.inner_margin, Margin::from(t.space.l));
        assert_eq!(kit.dialog.data.width, t.size.dialog_width);
        assert!((kit.backdrop.data.opacity - 0.392).abs() < 1e-6);
    }

    #[test]
    fn the_editor_and_terminal_frames_show_the_focus() {
        let t = Tokens::default();
        let kit = Kit::compile(&t, &Shapes::default());
        let editor = &kit.editor_frame.data.states;
        assert_eq!(editor.get(State::REST).ring_width, 0.0);
        let focus = editor.get(State::FOCUS);
        assert_eq!(
            (focus.ring_color, focus.ring_width, focus.ring_offset),
            (t.focus_ring, 1.0, -1.0)
        );
        let terminal = &kit.terminal_frame.data.states;
        assert_eq!(terminal.get(State::REST).border_color, t.border.default);
        assert_eq!(terminal.get(State::FOCUS).border_color, t.focus_ring);
        assert_eq!(terminal.get(State::REST).fill, t.surface.sunken);
    }

    #[test]
    fn a_frame_handler_of_its_own_paints_under_the_content() {
        struct Blot;
        impl Primitive<ComponentKit> for Blot {
            fn measure(&self, _: &ComponentKit, label: Vec2) -> Vec2 {
                label
            }
            fn paint(&self, cx: &KitCx<'_>, _: &ComponentKit, part: &Part<'_>) {
                cx.painter.rect_filled(part.rect, 0.0, Color32::RED);
            }
            fn paint_label(&self, _: &KitCx<'_>, _: &ComponentKit, _: &Part<'_>) {}
        }
        let ctx = Context::default();
        register_shape(&ctx, "dialog", "blot", Arc::new(Blot));
        let t = tokens_with(
            r#"
            $extends: "pop-dark"
            dialog:
              shape: {$type: "string", $value: "blot"}
            "#,
        );
        let kit = install(&ctx, &t);
        assert!(kit.problems.is_empty(), "{:?}", kit.problems);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            kit.dialog.show(ui, State::REST, egui::Shadow::NONE, |ui| {
                ui.label("content")
            });
        });
        let kinds: Vec<&str> = out
            .shapes
            .iter()
            .filter_map(|c| match &c.shape {
                Shape::Vec(v) if !v.is_empty() => Some("box"),
                Shape::Text(_) => Some("text"),
                Shape::Rect(r) if r.fill == Color32::RED => Some("box drawn over"),
                _ => None,
            })
            .collect();
        assert_eq!(kinds, ["box", "text"]);
        out.textures_delta.clear();
    }

    #[test]
    fn a_box_painted_under_fades_once_with_its_ui() {
        let ctx = Context::default();
        let kit = install(&ctx, &Tokens::default());
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.multiply_opacity(0.5);
            let slot = ui.painter().add(Shape::Noop);
            let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(20.0));
            let part = Part {
                rect,
                room: rect,
                state: State::REST,
                label: None,
            };
            kit.button.paint_under(ui, slot, &part, None);
        });
        let fill = out
            .shapes
            .iter()
            .find_map(|c| match &c.shape {
                Shape::Vec(v) => v.iter().find_map(|s| match s {
                    Shape::Rect(r) if r.fill.a() > 0 => Some(r.fill),
                    _ => None,
                }),
                _ => None,
            })
            .expect("the box is painted");
        let rest = kit.button.style(State::REST).fill;
        assert_eq!(fill, rest.gamma_multiply(0.5));
        out.textures_delta.clear();
    }

    #[test]
    fn a_registered_handler_can_be_named_by_a_theme() {
        struct Dot;
        impl Primitive<ComponentKit> for Dot {
            fn measure(&self, _: &ComponentKit, label: Vec2) -> Vec2 {
                label
            }
            fn paint(&self, _: &KitCx<'_>, _: &ComponentKit, _: &Part<'_>) {}
            fn paint_label(&self, _: &KitCx<'_>, _: &ComponentKit, _: &Part<'_>) {}
        }
        let ctx = Context::default();
        register_tab_shape(&ctx, "dot", Arc::new(Dot));
        let t = tokens_with(
            r#"
            $extends: "pop-dark"
            tab:
              shape: {$type: "string", $value: "dot"}
            "#,
        );
        let kit = install(&ctx, &t);
        assert_eq!(kit.tab.shape, "dot");
        assert!(kit.problems.is_empty());
        assert_eq!(
            kit.tab.measure(Vec2::new(30.0, 10.0)),
            Vec2::new(30.0, 10.0)
        );
    }
}
