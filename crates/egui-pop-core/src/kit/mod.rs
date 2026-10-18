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

use egui::{Color32, Context, Galley, Id, Painter, Pos2, Rect, Response, Vec2};
use egui_pop_tokens::Value;

use crate::theme::Tokens;

mod paint;
mod shapes;
mod tone;

pub use paint::{Corners, label_rect, paint_box, paint_indicator, paint_label, paint_ring};
pub use tone::Tone;

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
    /// Points; zero when the component has no fixed height.
    pub height: f32,
    /// Points; zero when the component has no fixed width.
    pub width: f32,
    pub indicator_side: Side,
    pub align: Align,
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
            align,
            padding_x: r
                .dimension(&format!("{component}.padding-x"))
                .unwrap_or(0.0),
            height: r.dimension(&format!("{component}.height")).unwrap_or(0.0),
            indicator_side,
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
                problems: Vec<String>,
            ) -> Kit {
                Kit {
                    $($field: component($name, $default),)*
                    problems,
                }
            }
        }
    };
}

components! {
    /// A tab of the workbench's tab bars.
    tab: "tab" => "box",
    /// The bar behind a row of tabs.
    tab_bar: "tab-bar" => "plain",
    /// A window's own title bar; its label is the window's title.
    title_bar: "title-bar" => "plain",
    /// Minimize and maximize (the glyph is drawn in the box's `text`).
    window_button: "window-button" => "plain",
    /// The close button.
    window_close: "window-close" => "plain",
}

type Handlers = BTreeMap<String, Arc<dyn Primitive<ComponentKit>>>;

/// The handlers a kit's `shape` tokens can name, per component.
#[derive(Clone)]
pub struct Shapes(BTreeMap<String, Handlers>);

impl Default for Shapes {
    /// The handlers egui-pop ships: `plain` for every component, and each
    /// handler module's own ([`shapes::built_in`]).
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
            let (shape, handler) = match handlers.get(&asked) {
                Some(h) => (asked, h.clone()),
                None => {
                    problems.push(format!(
                        "{name}.shape: no handler named `{asked}` (known: {}), `{default}` is drawn",
                        handlers.keys().cloned().collect::<Vec<_>>().join(", ")
                    ));
                    (default.to_string(), handlers[default].clone())
                }
            };
            Component {
                data,
                shape,
                handler,
            }
        };
        let mut kit = Kit::compile_components(component, Vec::new());
        kit.problems = problems;
        kit
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
    use egui::pos2;
    use egui_pop_tokens::{ThemeSpec, Themes};

    fn tokens_with(json: &str) -> Tokens {
        let mut themes = Themes::default();
        themes.add_json("kit-test", json).expect("test theme reads");
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
            r##"{
              "$extends": "pop-dark",
              "tab": {
                "rest": { "fill": {"$type": "color", "$value": "#101010"},
                          "text": {"$type": "color", "$value": "#808080"} },
                "selected": { "text": {"$type": "color", "$value": "#ffffff"} },
                "hover": { "fill": {"$type": "color", "$value": "#202020"} }
              }
            }"##,
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
            r#"{ "$extends": "pop-dark",
                 "title-bar": { "align": {"$type": "string", "$value": "start"} } }"#,
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
            r#"{ "$extends": "pop-dark",
                 "tab": { "focus": {
                   "ring-color": {"$type": "color", "$value": "{focus.ring}"},
                   "ring-width": {"$type": "dimension", "$value": {"value": 2, "unit": "px"}},
                   "ring-offset": {"$type": "dimension", "$value": {"value": 1, "unit": "px"}} } } }"#,
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
    fn tones_merge_after_rest_and_states_keep_what_they_give() {
        let t = tokens_with(
            r##"{
              "$extends": "pop-dark",
              "tab": {
                "rest": { "fill": {"$type": "color", "$value": "#101010"} },
                "hover": { "fill": {"$type": "color", "$value": "#202020"} },
                "disabled": { "text": {"$type": "color", "$value": "#303030"} },
                "tone": {
                  "danger": {
                    "fill": {"$type": "color", "$value": "#ff0000"},
                    "text": {"$type": "color", "$value": "#ffffff"},
                    "hover": { "fill": {"$type": "color", "$value": "#ff4040"} }
                  }
                }
              }
            }"##,
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
    }

    #[test]
    fn a_bad_align_is_reported() {
        let t = tokens_with(
            r#"{ "$extends": "pop-dark",
                 "title-bar": { "align": {"$type": "string", "$value": "middle"} } }"#,
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
            r#"{ "$extends": "pop-dark",
                 "tab": { "shape": {"$type": "string", "$value": "zigzag"} } }"#,
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
            r#"{ "$extends": "pop-dark",
                 "tab": { "shape": {"$type": "string", "$value": "dot"} } }"#,
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
