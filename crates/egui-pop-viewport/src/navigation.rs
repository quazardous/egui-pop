//! One navigation model for every canvas larger than its view (the node
//! graph, the image viewport, plots): what `docs/design/navigation.md`
//! proposes, in one place so each canvas behaves the same.
//!
//! - **Hand mode** ([`hand`]): H toggles it, Space held is the same for
//!   the moment; a primary drag then pans from anywhere on the canvas,
//!   for a pen, a touch screen or a mouse without a middle button. The
//!   canvas shows it with the grab cursor and an indicator
//!   ([`indicator`]).
//! - **Navigation commands** ([`NavCommand`]): `view.fit`,
//!   `view.frame_selection`, `view.zoom_in`, `view.zoom_out`,
//!   `view.zoom_100`, `view.pan_left` (`_right`, `_up`, `_down`) and
//!   `view.hand`. Their keys ([`NavKeys`]) are the pointer profile's
//!   (its `view_keys`): on the widget alone it reads them; in an
//!   application they are keymap bindings (shown and rebound in the
//!   shortcuts editor), and the commands reach the canvas in use through
//!   `egui_pop_core::canvas`.
//! - **The zoom control** ([`CanvasZoom`]): zoom out, the percentage
//!   with a menu of fit, 100 % and steps, zoom in and hand mode, over
//!   any canvas, through the same commands.
//! - **Touch screens**: two fingers pan ([`touch_pan`]) and a pinch
//!   zooms every canvas.
//! - **The overview** shows when the content is much larger than the
//!   view ([`overview_needed`]).
//! - **The view kept** across sessions for a canvas given an id
//!   ([`load_view`], [`save_view`]), in egui's persisted memory.
//! - **A one-time hint** on a canvas kind's first use ([`hint`]),
//!   dismissed by the first pan or a click, remembered.

use egui::{
    Align2, Context, CursorIcon, Id, Key, Modifiers, Rect, Response, Sense, Ui, Vec2, vec2,
};
use egui_pop_core::canvas;
use egui_pop_core::state::Slot;
use egui_pop_core::strings::tr;

use crate::strings as s;

/// A navigation command, the same on every canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NavCommand {
    /// Frames all the content.
    Fit,
    /// Frames the selection (all the content when nothing is selected).
    FrameSelection,
    ZoomIn,
    ZoomOut,
    /// One content unit to one point (an image's pixel to a pixel).
    Zoom100,
    /// A zoom in percent, around the view's centre (a zoom control's
    /// step); sent as `view.zoom_to:<percent>`, it has no key.
    ZoomTo(u32),
    /// Pans a step that way.
    Pan(PanDir),
    /// Toggles hand mode.
    Hand,
}

/// A direction to pan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PanDir {
    Left,
    Right,
    Up,
    Down,
}

impl PanDir {
    /// Which way the view moves, as a unit vector.
    pub fn vec(self) -> Vec2 {
        match self {
            PanDir::Left => vec2(-1.0, 0.0),
            PanDir::Right => vec2(1.0, 0.0),
            PanDir::Up => vec2(0.0, -1.0),
            PanDir::Down => vec2(0.0, 1.0),
        }
    }
}

impl NavCommand {
    /// Every command with keys, in the palette's order.
    pub const ALL: [NavCommand; 10] = [
        NavCommand::Fit,
        NavCommand::FrameSelection,
        NavCommand::ZoomIn,
        NavCommand::ZoomOut,
        NavCommand::Zoom100,
        NavCommand::Pan(PanDir::Left),
        NavCommand::Pan(PanDir::Right),
        NavCommand::Pan(PanDir::Up),
        NavCommand::Pan(PanDir::Down),
        NavCommand::Hand,
    ];

    /// The command's id (`view.fit`; `view.zoom_to` for any zoom to a
    /// percent).
    pub fn id(self) -> &'static str {
        match self {
            NavCommand::Fit => "view.fit",
            NavCommand::FrameSelection => "view.frame_selection",
            NavCommand::ZoomIn => "view.zoom_in",
            NavCommand::ZoomOut => "view.zoom_out",
            NavCommand::Zoom100 => "view.zoom_100",
            NavCommand::ZoomTo(_) => "view.zoom_to",
            NavCommand::Pan(PanDir::Left) => "view.pan_left",
            NavCommand::Pan(PanDir::Right) => "view.pan_right",
            NavCommand::Pan(PanDir::Up) => "view.pan_up",
            NavCommand::Pan(PanDir::Down) => "view.pan_down",
            NavCommand::Hand => "view.hand",
        }
    }

    /// A command by its id (one of [`NavCommand::ALL`]).
    pub fn from_id(id: &str) -> Option<NavCommand> {
        NavCommand::ALL.into_iter().find(|c| c.id() == id)
    }

    /// The command as sent to a canvas (`egui_pop_core::canvas::request`):
    /// its id, or `view.zoom_to:<percent>`.
    pub fn request(self) -> String {
        match self {
            NavCommand::ZoomTo(p) => format!("view.zoom_to:{p}"),
            c => c.id().to_string(),
        }
    }

    /// A command as sent to a canvas ([`NavCommand::request`]).
    pub fn parse(text: &str) -> Option<NavCommand> {
        if let Some(p) = text.strip_prefix("view.zoom_to:") {
            return p
                .trim()
                .parse()
                .ok()
                .filter(|p| *p > 0)
                .map(NavCommand::ZoomTo);
        }
        NavCommand::from_id(text)
    }
}

/// A key and its modifiers.
pub type Chord = (Modifiers, Key);

/// The navigation keys: each command's chords. A pointer profile gives
/// them ([`egui_pop_core::model::pointer::Profile::view_keys`]), through
/// the keymap in an application, where the user rebinds them in the
/// shortcuts editor.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NavKeys {
    pub bindings: Vec<(NavCommand, Chord)>,
}

impl NavKeys {
    /// The keys of a pointer map ([`PointerMap::view_keys`]).
    ///
    /// [`PointerMap::view_keys`]: egui_pop_core::model::pointer::PointerMap::view_keys
    pub fn from_map(map: &egui_pop_core::model::pointer::PointerMap) -> NavKeys {
        let mut bindings = Vec::new();
        for c in NavCommand::ALL {
            for chord in map.view_keys(c.id()) {
                if let Some((m, k)) = egui_pop_core::keys::key_of(chord) {
                    // Ctrl is the command key (Cmd on macOS), as egui
                    // matches it logically.
                    let m = Modifiers {
                        ctrl: false,
                        command: m.ctrl,
                        mac_cmd: false,
                        ..m
                    };
                    bindings.push((c, (m, k)));
                }
            }
        }
        // More modifiers first: Shift+2 is not read as a plain 2.
        bindings.sort_by_key(|(_, (m, _))| {
            std::cmp::Reverse(u8::from(m.command) + u8::from(m.shift) + u8::from(m.alt))
        });
        NavKeys { bindings }
    }

    /// A shipped profile's keys (the default profile's for an unknown
    /// id).
    pub fn for_profile(id: &str) -> NavKeys {
        use egui_pop_core::model::pointer;
        let p = pointer::profile(id).unwrap_or_else(pointer::default_profile);
        NavKeys::from_map(&pointer::PointerMap::from_profile(p))
    }

    /// The chords of a command.
    pub fn chords(&self, command: NavCommand) -> impl Iterator<Item = Chord> + '_ {
        self.bindings
            .iter()
            .filter(move |(c, _)| *c == command)
            .map(|(_, k)| *k)
    }

    /// The keymap's spelling of a command's keys (`Ctrl+Plus`), for
    /// tooltips and the palette.
    pub fn keys_of(&self, command: NavCommand) -> Vec<String> {
        self.chords(command)
            .map(|(m, k)| egui_pop_core::keys::chord_of(k, m).to_string())
            .collect()
    }

    /// The commands whose keys were pressed, the keys consumed.
    fn read(&self, ctx: &Context) -> Vec<NavCommand> {
        let mut out = Vec::new();
        ctx.input_mut(|i| {
            for &(c, (m, k)) in &self.bindings {
                if i.consume_key(m, k) && !out.contains(&c) {
                    out.push(c);
                }
            }
        });
        out
    }
}

const KEYS: Slot<NavKeys> = Slot::new("viewport.navigation.keys");

/// The navigation keys in force: set with [`set_keys`], else the pointer
/// map's (the profile's, with the user's changes in an application).
pub fn keys(ctx: &Context) -> NavKeys {
    KEYS.get(ctx)
        .unwrap_or_else(|| NavKeys::from_map(&egui_pop_core::pointer::map(ctx)))
}

/// Sets the navigation keys outright; a canvas then reads them itself,
/// in an application too.
pub fn set_keys(ctx: &Context, keys: NavKeys) {
    KEYS.set(ctx, keys);
}

/// Whether hand mode is on (a primary drag pans).
pub fn hand(ctx: &Context) -> bool {
    canvas::hand(ctx)
}

/// Turns hand mode on or off.
pub fn set_hand(ctx: &Context, on: bool) {
    canvas::set_hand(ctx, on);
}

/// Whether a primary drag pans now: hand mode, or Space held.
pub fn primary_pans(ctx: &Context) -> bool {
    hand(ctx) || ctx.input(|i| i.key_down(Key::Space))
}

/// A canvas's navigation for this frame: marks it active when the
/// pointer is over it or it has the keyboard, reads its keys then (not
/// while a text input owns the keyboard, nor in an application, whose
/// keymap reads them and sends the commands), and gives what was asked,
/// the application's commands included. Hand mode is toggled here and
/// not returned.
pub fn commands(ctx: &Context, id: Id, hovered: bool, focused: bool) -> Vec<NavCommand> {
    let mut out = Vec::new();
    if hovered || focused {
        canvas::mark_active(ctx, id);
        let own = KEYS.get(ctx);
        let reads = own.is_some() || !egui_pop_core::pointer::map(ctx).keys_dispatched;
        if reads && !egui_pop_core::focus::text_input_owns_keyboard(ctx) && !ctx.text_edit_focused()
        {
            out = own.unwrap_or_else(|| keys(ctx)).read(ctx);
        }
    }
    out.extend(
        canvas::take(ctx, id)
            .iter()
            .filter_map(|c| NavCommand::parse(c)),
    );
    out.retain(|c| {
        if *c == NavCommand::Hand {
            set_hand(ctx, !hand(ctx));
            false
        } else {
            true
        }
    });
    out
}

/// Whether two fingers or more are on a touch screen now (egui's
/// multi-touch): a canvas then pans and zooms with them, and leaves the
/// first finger's drag alone.
pub fn touching(ctx: &Context) -> bool {
    ctx.input(|i| i.multi_touch().is_some())
}

/// How far two fingers or more moved this frame on a touch screen, in
/// points, when they started over `rect`: the content follows them. A
/// pinch's zoom comes with the wheel's inputs
/// (`egui_pop_core::pointer::wheel_inputs`).
pub fn touch_pan(ctx: &Context, rect: Rect) -> Vec2 {
    ctx.input(|i| {
        i.multi_touch()
            .filter(|t| rect.contains(t.start_pos))
            .map_or(Vec2::ZERO, |t| t.translation_delta)
    })
}

/// How much larger than the view the content must be, on one axis, for
/// the overview to show.
pub const OVERVIEW_RATIO: f32 = 2.0;

/// When a canvas shows its overview (a minimap in a corner).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overview {
    /// When the content is much larger than the view ([`overview_needed`]).
    #[default]
    WhenNeeded,
    Always,
    Never,
}

impl Overview {
    /// Whether it shows, for this content and view (both in content
    /// units).
    pub fn shows(self, content: Rect, view: Rect) -> bool {
        match self {
            Overview::WhenNeeded => overview_needed(content, view),
            Overview::Always => true,
            Overview::Never => false,
        }
    }
}

/// Whether the content is much larger than the view: on one axis, more
/// than [`OVERVIEW_RATIO`] times the view.
pub fn overview_needed(content: Rect, view: Rect) -> bool {
    if !content.is_positive() || !view.is_positive() {
        return false;
    }
    content.width() > view.width() * OVERVIEW_RATIO
        || content.height() > view.height() * OVERVIEW_RATIO
}

/// A canvas's view kept in egui's persisted memory under `id`.
pub fn load_view(ctx: &Context, id: Id) -> Option<Rect> {
    let v = ctx.data_mut(|d| d.get_persisted::<[f32; 4]>(view_id(id)))?;
    let r = Rect::from_min_max(egui::pos2(v[0], v[1]), egui::pos2(v[2], v[3]));
    (r.is_finite() && r.is_positive()).then_some(r)
}

/// Keeps a canvas's view under `id` (written when it changed).
pub fn save_view(ctx: &Context, id: Id, view: Rect) {
    if !view.is_finite() || !view.is_positive() {
        return;
    }
    let v = [view.min.x, view.min.y, view.max.x, view.max.y];
    let key = view_id(id);
    if ctx.data(|d| d.get_temp::<[f32; 4]>(key)) != Some(v) {
        ctx.data_mut(|d| d.insert_persisted(key, v));
    }
}

fn view_id(id: Id) -> Id {
    Id::unique("egui_pop.navigation.view").with(id)
}

fn hint_id(kind: &str) -> Id {
    Id::unique("egui_pop.navigation.hint_seen").with(kind)
}

/// Whether a canvas kind's hint was seen (dismissed) on this
/// installation.
pub fn hint_seen(ctx: &Context, kind: &str) -> bool {
    ctx.data_mut(|d| d.get_persisted::<bool>(hint_id(kind)))
        .unwrap_or(false)
}

/// Marks a canvas kind's hint as seen.
pub fn dismiss_hint(ctx: &Context, kind: &str) {
    ctx.data_mut(|d| d.insert_persisted(hint_id(kind), true));
}

/// The hint showing on a canvas, by its id.
const HINT_SHOWN: Slot<bool> = Slot::new("viewport.navigation.hint_shown");

/// The one-time hint on a canvas kind's first use: shown from the first
/// time the pointer is over the canvas until the first pan (`panned`) or
/// a click on the canvas or on the hint; then never again for that kind.
/// Whether it shows.
pub fn hint(
    ui: &mut Ui,
    id: Id,
    rect: Rect,
    kind: &str,
    response: &Response,
    panned: bool,
) -> bool {
    let ctx = ui.ctx().clone();
    if hint_seen(&ctx, kind) {
        return false;
    }
    let slot = HINT_SHOWN.keyed(id);
    let mut shown = slot.get_or_default(&ctx);
    if !shown && response.hovered() {
        shown = true;
        slot.set(&ctx, true);
    }
    if !shown {
        return false;
    }
    if panned || response.clicked() || response.drag_started() {
        dismiss_hint(&ctx, kind);
        slot.set(&ctx, false);
        return false;
    }
    let text = tr(&ctx, &s::NAV_HINT);
    let at = rect.center_top() + vec2(0.0, 8.0);
    let clicked = overlay_label(ui, rect, at, Align2::CENTER_TOP, &text, Sense::click()).clicked();
    if clicked {
        dismiss_hint(&ctx, kind);
        slot.set(&ctx, false);
        return false;
    }
    true
}

/// The hand mode indicator in a canvas's top-left corner, and the grab
/// cursor over the canvas, while hand mode is on. Whether it shows.
pub fn indicator(ui: &mut Ui, rect: Rect, response: &Response) -> bool {
    let ctx = ui.ctx().clone();
    if !hand(&ctx) {
        return false;
    }
    if response.hovered() && !ctx.input(|i| i.pointer.any_down()) {
        ctx.set_cursor_icon(CursorIcon::Grab);
    }
    let text = tr(&ctx, &s::NAV_HAND);
    overlay_label(
        ui,
        rect,
        rect.left_top() + vec2(8.0, 8.0),
        Align2::LEFT_TOP,
        &text,
        Sense::hover(),
    );
    true
}

/// A small framed label over a canvas.
fn overlay_label(
    ui: &mut Ui,
    clip: Rect,
    at: egui::Pos2,
    align: Align2,
    text: &str,
    sense: Sense,
) -> Response {
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::TextStyle::Small.resolve(ui.style()),
        ui.visuals().strong_text_color(),
    );
    let size = galley.size() + vec2(16.0, 8.0);
    let min = match align {
        Align2::CENTER_TOP => egui::pos2(at.x - size.x / 2.0, at.y),
        _ => at,
    };
    let r = Rect::from_min_size(min, size).intersect(clip);
    // In an area just above the canvas (and above the scene's own layer,
    // where a graph's nodes are), so it takes no room in the canvas's
    // `Ui`, and a window over the canvas stays over it.
    let id = ui.scope_id().with(("egui_pop.navigation.overlay", text));
    canvas_overlay(ui, id, r.min, |ui| {
        ui.set_clip_rect(clip);
        let (r, resp) = ui.allocate_exact_size(r.size(), sense);
        let fill = ui.visuals().window_fill.gamma_multiply(0.92);
        let stroke = ui.visuals().window_stroke;
        ui.painter()
            .rect(r, 4.0, fill, stroke, egui::StrokeKind::Inside);
        let label = text.to_string();
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &label));
        ui.painter().galley(
            r.center() - galley.size() / 2.0,
            galley,
            ui.visuals().strong_text_color(),
        );
        resp
    })
    .inner
}

/// An overlay over a canvas drawn in `ui`, at `at`: an area in the
/// canvas layer's order, drawn just above that layer (a sublayer of it),
/// not in the foreground order, where it would lie over every window,
/// popup or dialog that covers the canvas.
pub(crate) fn canvas_overlay<R>(
    ui: &Ui,
    id: Id,
    at: egui::Pos2,
    add: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    let parent = ui.layer_id();
    let shown = egui::Area::new(id)
        .order(parent.order)
        .fixed_pos(at)
        .constrain(false)
        .show(ui.ctx(), add);
    let layer = shown.response.layer_id;
    ui.ctx().set_sublayer(parent, layer);
    shown
}

/// The zoom steps a [`CanvasZoom`]'s menu offers by default, in percent
/// (100 % is always offered).
pub const ZOOM_STEPS: [u32; 5] = [25, 50, 200, 400, 800];

/// A zoom control for any canvas: zoom out, the zoom in percent (a menu
/// of fit, frame the selection, 100 % and the steps), zoom in, and hand
/// mode. It sends the canvas commands (`view.zoom_out`, `view.fit`,
/// `view.zoom_to:200`...) to the canvas it drives through
/// `egui_pop_core::canvas`, and shows the zoom the canvas published;
/// each button's tooltip says its keys.
///
/// ```no_run
/// # fn page(ui: &mut egui::Ui, graph: &dyn egui_pop_viewport::graph::GraphModel, state: &mut egui_pop_viewport::graph::GraphState) {
/// use egui_pop_viewport::graph::GraphEditor;
/// use egui_pop_viewport::navigation::CanvasZoom;
///
/// let out = GraphEditor::new(graph, state).show(ui);
/// CanvasZoom::new(out.canvas).show(ui);
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct CanvasZoom {
    canvas: Option<Id>,
    steps: Vec<u32>,
    hand: bool,
    frame_selection: bool,
}

impl CanvasZoom {
    /// Drives a canvas by its id (`GraphOutput::canvas`,
    /// `ViewportOutput::canvas`).
    pub fn new(canvas: Id) -> CanvasZoom {
        CanvasZoom {
            canvas: Some(canvas),
            steps: ZOOM_STEPS.to_vec(),
            hand: true,
            frame_selection: true,
        }
    }

    /// Drives the canvas in use, else the one used last (a toolbar's
    /// control for every canvas of a window).
    pub fn active() -> CanvasZoom {
        CanvasZoom {
            canvas: None,
            ..CanvasZoom::new(Id::NULL)
        }
    }

    /// The menu's steps, in percent.
    pub fn steps(mut self, steps: impl IntoIterator<Item = u32>) -> CanvasZoom {
        self.steps = steps.into_iter().filter(|p| *p > 0).collect();
        self
    }

    /// Shows the hand mode button (default: on).
    pub fn hand_button(mut self, on: bool) -> CanvasZoom {
        self.hand = on;
        self
    }

    /// Offers "frame the selection" in the menu (default: on; off for a
    /// canvas without a selection, an image).
    pub fn frame_selection(mut self, on: bool) -> CanvasZoom {
        self.frame_selection = on;
        self
    }

    /// Draws the row. Its response is clicked when a command was sent.
    pub fn show(self, ui: &mut Ui) -> Response {
        use egui_pop_core::icon::{Icon, Size};
        use egui_pop_core::ui::{Stock, Tone, UiPop as _};
        let ctx = ui.ctx().clone();
        let target = self
            .canvas
            .or_else(|| canvas::active(&ctx).or_else(|| canvas::last(&ctx)));
        let keys = keys(&ctx);
        let tip = |title: &str, c: NavCommand| {
            let k = keys.keys_of(c);
            if k.is_empty() {
                title.to_string()
            } else {
                format!("{title} ({})", k.join(", "))
            }
        };
        let mut sent: Vec<NavCommand> = Vec::new();
        let inner = ui.horizontal(|ui| {
            ui.add_enabled_ui(target.is_some(), |ui| {
                let out = ui
                    .add(
                        Icon::Minus
                            .size(Size::Toolbar)
                            .label(&tr(&ctx, &s::ZOOM_OUT))
                            .sense_click(),
                    )
                    .on_hover_text(tip(&tr(&ctx, &s::ZOOM_OUT), NavCommand::ZoomOut));
                if out.clicked() {
                    sent.push(NavCommand::ZoomOut);
                }
                let zoom = target.and_then(|id| canvas::zoom(&ctx, id));
                let text = zoom.map_or_else(
                    || "– %".to_string(),
                    |z| {
                        format!(
                            "{} %",
                            crate::navigator::percent_text(&ctx, f64::from(z) * 100.0)
                        )
                    },
                );
                let level = tr(&ctx, &s::ZOOM_LEVEL);
                let menu = ui.menu_button(text.clone(), |ui| {
                    let mut item = |ui: &mut Ui, label: String, c: NavCommand| {
                        let mut b = ui.button(label);
                        let k = keys.keys_of(c);
                        if !k.is_empty() {
                            b = b.on_hover_text(k.join(", "));
                        }
                        if b.clicked() {
                            sent.push(c);
                            ui.close();
                        }
                    };
                    item(ui, tr(&ctx, &s::ZOOM_FIT), NavCommand::Fit);
                    if self.frame_selection {
                        item(
                            ui,
                            tr(&ctx, &s::ZOOM_FRAME_SELECTION),
                            NavCommand::FrameSelection,
                        );
                    }
                    item(ui, tr(&ctx, &s::ZOOM_ACTUAL), NavCommand::Zoom100);
                    if !self.steps.is_empty() {
                        ui.separator();
                    }
                    for &p in self.steps.iter().filter(|p| **p != 100) {
                        item(
                            ui,
                            format!("{} %", crate::navigator::percent_text(&ctx, f64::from(p))),
                            NavCommand::ZoomTo(p),
                        );
                    }
                });
                let r = menu.response.on_hover_text(level.clone());
                ctx.accesskit_node_builder(r.id, |node| {
                    node.set_label(level.clone());
                    node.set_value(text.clone());
                });
                let into = ui
                    .add(
                        Icon::Plus
                            .size(Size::Toolbar)
                            .label(&tr(&ctx, &s::ZOOM_IN))
                            .sense_click(),
                    )
                    .on_hover_text(tip(&tr(&ctx, &s::ZOOM_IN), NavCommand::ZoomIn));
                if into.clicked() {
                    sent.push(NavCommand::ZoomIn);
                }
                if self.hand {
                    let on = hand(&ctx);
                    let label = tr(&ctx, &s::NAV_HAND_BUTTON);
                    let b = ui
                        .scoped(Stock::ToggleButton, Tone::Neutral, |ui| {
                            ui.add(egui::Button::new(label.clone()).selected(on))
                        })
                        .inner
                        .on_hover_text(tip(&label, NavCommand::Hand));
                    if b.clicked() {
                        set_hand(&ctx, !on);
                    }
                }
            });
        });
        let mut response = inner.response;
        if let Some(id) = target {
            for c in &sent {
                canvas::request_to(&ctx, id, &c.request());
            }
        }
        if !sent.is_empty() {
            response.mark_changed();
        }
        response
    }
}
