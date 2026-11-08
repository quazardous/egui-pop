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
//!   `view.hand`. Their keys ([`NavKeys`], after the pointer profile in
//!   force) work on the widget alone; in an application the same
//!   commands are in the keymap and the palette, and reach the canvas in
//!   use through `egui_pop_core::canvas`.
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
    /// Every command, in the palette's order.
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

    /// The command's id (`view.fit`).
    pub fn id(self) -> &'static str {
        match self {
            NavCommand::Fit => "view.fit",
            NavCommand::FrameSelection => "view.frame_selection",
            NavCommand::ZoomIn => "view.zoom_in",
            NavCommand::ZoomOut => "view.zoom_out",
            NavCommand::Zoom100 => "view.zoom_100",
            NavCommand::Pan(PanDir::Left) => "view.pan_left",
            NavCommand::Pan(PanDir::Right) => "view.pan_right",
            NavCommand::Pan(PanDir::Up) => "view.pan_up",
            NavCommand::Pan(PanDir::Down) => "view.pan_down",
            NavCommand::Hand => "view.hand",
        }
    }

    /// A command by its id.
    pub fn from_id(id: &str) -> Option<NavCommand> {
        NavCommand::ALL.into_iter().find(|c| c.id() == id)
    }
}

/// A key and its modifiers.
pub type Chord = (Modifiers, Key);

/// The navigation keys, per pointer profile ([`NavKeys::for_profile`]).
#[derive(Clone, Debug, PartialEq)]
pub struct NavKeys {
    pub fit: Vec<Chord>,
    pub frame_selection: Vec<Chord>,
    pub zoom_in: Vec<Chord>,
    pub zoom_out: Vec<Chord>,
    pub zoom_100: Vec<Chord>,
    /// Held with the arrows: pans.
    pub pan: Modifiers,
    pub hand: Vec<Chord>,
}

impl Default for NavKeys {
    /// The Document profile's.
    fn default() -> Self {
        NavKeys::for_profile(egui_pop_core::model::pointer::DEFAULT_PROFILE)
    }
}

impl NavKeys {
    /// The keys every profile shares: Home fits, F frames the selection,
    /// Ctrl and `+` / `-` / `0` zoom, Ctrl and the arrows pan, H toggles
    /// hand mode.
    pub fn common() -> NavKeys {
        let none = Modifiers::NONE;
        let cmd = Modifiers::COMMAND;
        NavKeys {
            fit: vec![(none, Key::Home)],
            frame_selection: vec![(none, Key::F)],
            zoom_in: vec![(cmd, Key::Plus), (cmd, Key::Equals)],
            zoom_out: vec![(cmd, Key::Minus)],
            zoom_100: vec![(cmd, Key::Num0)],
            pan: cmd,
            hand: vec![(none, Key::H)],
        }
    }

    /// A profile's keys: the common ones, plus Blender's `.` (frame
    /// selected) and Document's Shift+1 / Shift+2 (Figma's fit all, fit
    /// selection).
    pub fn for_profile(id: &str) -> NavKeys {
        let mut k = NavKeys::common();
        match egui_pop_core::model::pointer::canonical_id(id) {
            "blender" => k.frame_selection.push((Modifiers::NONE, Key::Period)),
            "document" => {
                k.fit.push((Modifiers::SHIFT, Key::Num1));
                k.frame_selection.push((Modifiers::SHIFT, Key::Num2));
            }
            _ => {}
        }
        k
    }

    /// The keymap's spelling of a command's keys (`Ctrl+Plus`), for
    /// tooltips and the palette.
    pub fn keys_of(&self, command: NavCommand) -> Vec<String> {
        let chords: Vec<Chord> = match command {
            NavCommand::Fit => self.fit.clone(),
            NavCommand::FrameSelection => self.frame_selection.clone(),
            NavCommand::ZoomIn => self.zoom_in.clone(),
            NavCommand::ZoomOut => self.zoom_out.clone(),
            NavCommand::Zoom100 => self.zoom_100.clone(),
            NavCommand::Hand => self.hand.clone(),
            NavCommand::Pan(d) => vec![(
                self.pan,
                match d {
                    PanDir::Left => Key::ArrowLeft,
                    PanDir::Right => Key::ArrowRight,
                    PanDir::Up => Key::ArrowUp,
                    PanDir::Down => Key::ArrowDown,
                },
            )],
        };
        chords
            .into_iter()
            .map(|(m, k)| egui_pop_core::keys::chord_of(k, m).to_string())
            .collect()
    }

    /// The commands whose keys were pressed, the keys consumed.
    fn read(&self, ctx: &Context) -> Vec<NavCommand> {
        let mut out = Vec::new();
        ctx.input_mut(|i| {
            let mut any = |cmd: NavCommand, chords: &[Chord]| {
                if chords.iter().any(|&(m, k)| i.consume_key(m, k)) {
                    out.push(cmd);
                }
            };
            any(NavCommand::Fit, &self.fit);
            any(NavCommand::FrameSelection, &self.frame_selection);
            any(NavCommand::ZoomIn, &self.zoom_in);
            any(NavCommand::ZoomOut, &self.zoom_out);
            any(NavCommand::Zoom100, &self.zoom_100);
            any(NavCommand::Hand, &self.hand);
            for d in [PanDir::Left, PanDir::Right, PanDir::Up, PanDir::Down] {
                let key = match d {
                    PanDir::Left => Key::ArrowLeft,
                    PanDir::Right => Key::ArrowRight,
                    PanDir::Up => Key::ArrowUp,
                    PanDir::Down => Key::ArrowDown,
                };
                any(NavCommand::Pan(d), &[(self.pan, key)]);
            }
        });
        out
    }
}

const KEYS: Slot<NavKeys> = Slot::new("viewport.navigation.keys");

/// The navigation keys in force: set with [`set_keys`], else the pointer
/// profile's.
pub fn keys(ctx: &Context) -> NavKeys {
    KEYS.get(ctx)
        .unwrap_or_else(|| NavKeys::for_profile(&egui_pop_core::pointer::map(ctx).profile))
}

/// Sets the navigation keys outright.
pub fn set_keys(ctx: &Context, keys: NavKeys) {
    KEYS.set(ctx, keys);
}

const HAND: Slot<bool> = Slot::new("viewport.navigation.hand");

/// Whether hand mode is on (a primary drag pans).
pub fn hand(ctx: &Context) -> bool {
    HAND.get_or_default(ctx)
}

/// Turns hand mode on or off.
pub fn set_hand(ctx: &Context, on: bool) {
    HAND.set(ctx, on);
    ctx.request_repaint();
}

/// Whether a primary drag pans now: hand mode, or Space held.
pub fn primary_pans(ctx: &Context) -> bool {
    hand(ctx) || ctx.input(|i| i.key_down(Key::Space))
}

/// A canvas's navigation for this frame: marks it active when the
/// pointer is over it or it has the keyboard, reads its keys then (not
/// while a text input owns the keyboard), and gives what was asked, the
/// application's commands included. Hand mode is toggled here and not
/// returned.
pub fn commands(ctx: &Context, id: Id, hovered: bool, focused: bool) -> Vec<NavCommand> {
    let mut out = Vec::new();
    if hovered || focused {
        canvas::mark_active(ctx, id);
        if !egui_pop_core::focus::text_input_owns_keyboard(ctx) && !ctx.text_edit_focused() {
            out = keys(ctx).read(ctx);
        }
    }
    out.extend(
        canvas::take(ctx, id)
            .iter()
            .filter_map(|c| NavCommand::from_id(c)),
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
    // In an area above the canvas (and above the scene's own layer, where
    // a graph's nodes are), so it takes no room in the canvas's `Ui`.
    egui::Area::new(ui.scope_id().with(("egui_pop.navigation.overlay", text)))
        .order(egui::Order::Foreground)
        .fixed_pos(r.min)
        .constrain(false)
        .show(ui.ctx(), |ui| {
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
