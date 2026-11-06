//! Dense list items: one row look for lists, trees and panels.
//!
//! A [`ListItem`] is a row of the theme's row height (`size.row.height`,
//! apart from the controls' `size.control.height`): an optional drag
//! handle, indent guides down its levels, a collapsing arrow, an icon,
//! its label, weak text on its right ([`ListItem::trailing`]) and action
//! buttons ([`ItemAction`]: a visibility eye, remove) shown while the row
//! is hovered, focused or selected, or always. The tree and the list draw
//! their rows with the same pieces, so a row looks the same everywhere.
//! A [`PropertyRow`](crate::property::PropertyRow) is the same row with
//! a name column and a value.
//!
//! Everything is painted through the kit: the row's box is the
//! `list-item` component (based on `list-row`, so a kit restyling rows
//! restyles items), its guides `list-guide`, its action buttons
//! `icon-button`, its arrow the kit's disclosure glyphs.
//!
//! ```
//! use egui_pop_core::icon::Icon;
//! use egui_pop_views::item::{ItemAction, ListItem};
//!
//! fn layer(ui: &mut egui::Ui, shown: &mut bool, selected: bool) -> bool {
//!     let eye = if *shown { Icon::Visible } else { Icon::Hidden };
//!     let out = ListItem::new("Ground")
//!         .icon(Icon::File)
//!         .trailing("12 points")
//!         .selected(selected)
//!         .action(ItemAction::new("visibility", eye, "Hide Ground").pinned(!*shown))
//!         .show(ui);
//!     if out.action() == Some("visibility") {
//!         *shown = !*shown;
//!     }
//!     out.response.clicked()
//! }
//! # let _ = layer;
//! ```
//!
//! # Keyboard
//!
//! A standalone item takes the focus (Tab); Enter or Space clicks it,
//! Right and Left open and close a collapsing one. Its action buttons
//! follow it in the Tab order: they show while the item or one of them
//! has the focus, so Tab walks from the item through its buttons to the
//! next widget, and Shift+Tab walks back. In the tree, the cursor's row
//! shows its actions while the tree has the focus, and they come right
//! after the tree in the Tab order: Tab from the tree reaches the
//! cursor's buttons, Shift+Tab goes back to the tree. Screen readers also
//! find each action on the row itself, as a custom action named by its
//! label.
//!
//! A label that does not fit draws on one line, elided with "…" before
//! the row's trailing text and buttons, which stay whole; hovering the row
//! then shows the full label. The row's accessible name is always the
//! full label.

use egui::accesskit::{self, Role};
use egui::text::{LayoutJob, TextWrapping};
use egui::{
    AsIdSalt, Color32, EventFilter, FontSelection, Id, IdSalt, Key, Modifiers, Rect, Response,
    Sense, TextStyle, Ui, UiBuilder, WidgetInfo, pos2, vec2,
};
use egui_pop_core::icon::{Icon, Size, Source};
use egui_pop_core::kit::{Glyph, Kit, KitCx, Part, State, Tone};
use egui_pop_core::pointer::{self, DragKind};
use egui_pop_core::strings::tr_args;
use egui_pop_core::theme::Tokens;

use crate::strings as s;

/// A row's height: the theme's `size.row.height`, kept apart from the
/// controls' height, and never under the body text.
///
/// It is egui's interact height moved by what the theme's row height
/// differs from its control height: the token's value once a theme is
/// installed (which sets egui's interact height to the control height),
/// egui's interact height in a bare context.
pub fn row_height(ui: &Ui) -> f32 {
    let t = Tokens::of(ui.ctx());
    let shift = t.size.row_height - t.size.control_height;
    (ui.spacing().interact_size.y + shift).max(ui.text_style_height(&TextStyle::Body) + 4.0)
}

/// The column on a row's left that holds its collapsing arrow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Disclosure {
    /// No column: the content starts after a small padding.
    #[default]
    None,
    /// The column, empty: a leaf among branches, aligned with them.
    Leaf,
    /// The column with the open arrow.
    Open,
    /// The column with the closed arrow.
    Closed,
}

impl Disclosure {
    /// A branch's arrow, open or closed.
    pub fn branch(open: bool) -> Self {
        if open {
            Disclosure::Open
        } else {
            Disclosure::Closed
        }
    }

    /// Whether it has the column.
    pub fn has_column(self) -> bool {
        self != Disclosure::None
    }

    /// Whether it draws an arrow (a branch).
    pub fn is_branch(self) -> bool {
        matches!(self, Disclosure::Open | Disclosure::Closed)
    }
}

/// When a row's action buttons show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reveal {
    /// While the row is hovered, focused (or one of its buttons is) or
    /// selected; pinned actions always.
    #[default]
    Hover,
    /// Always.
    Always,
}

/// An action button at a row's right end: an icon, and its label (its
/// accessible name and tooltip, and the row's custom action for screen
/// readers).
#[derive(Clone, Debug)]
pub struct ItemAction {
    id: String,
    icon: Source,
    label: String,
    pinned: bool,
}

impl ItemAction {
    /// An action named `id` in the output (and the tree's `tree.action`
    /// step), drawn as `icon`, named `label`.
    pub fn new(id: impl Into<String>, icon: impl Into<Source>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            icon: icon.into(),
            label: label.into(),
            pinned: false,
        }
    }

    /// Shown even when the row's actions are hidden: a state worth seeing
    /// at a glance (a hidden layer's closed eye).
    pub fn pinned(mut self, pinned: bool) -> Self {
        self.pinned = pinned;
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn icon(&self) -> &Source {
        &self.icon
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned
    }
}

/// What [`ListItem::show`] returns.
#[derive(Debug)]
pub struct ItemResponse {
    /// The row: clicked, double clicked, right clicked, hovered. A click
    /// on the arrow clicks the row too: look at `toggled` first.
    pub response: Response,
    /// The arrow was clicked (or Right, Left, or a screen reader's expand
    /// or collapse asked for it): the application opens or closes it.
    pub toggled: bool,
    /// The action clicked this pass (its id).
    pub action: Option<String>,
    /// The drag handle, with [`ListItem::drag_handle`]: drag from it
    /// (`Response::drag_started`, `dnd_set_drag_payload`).
    pub handle: Option<Response>,
}

impl ItemResponse {
    /// The action clicked this pass.
    pub fn action(&self) -> Option<&str> {
        self.action.as_deref()
    }
}

/// A dense row: see the [module](self).
#[derive(Clone, Debug)]
#[must_use]
pub struct ListItem {
    label: String,
    icon: Option<Source>,
    trailing: Option<String>,
    depth: usize,
    guides: bool,
    disclosure: Disclosure,
    handle: bool,
    selected: bool,
    active: bool,
    disabled: bool,
    actions: Vec<ItemAction>,
    reveal: Reveal,
    height: Option<f32>,
    id_salt: Option<IdSalt>,
    role: Role,
}

impl ListItem {
    /// A row showing `label` (also its accessible name).
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            trailing: None,
            depth: 0,
            guides: false,
            disclosure: Disclosure::None,
            handle: false,
            selected: false,
            active: false,
            disabled: false,
            actions: Vec::new(),
            reveal: Reveal::Hover,
            height: None,
            id_salt: None,
            role: Role::ListItem,
        }
    }

    /// An icon before the label, in the row's text colour at the inline
    /// icon size.
    pub fn icon(mut self, icon: impl Into<Source>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Weak text at the right end, before the actions (a count, a type).
    pub fn trailing(mut self, text: impl Into<String>) -> Self {
        self.trailing = Some(text.into());
        self
    }

    /// Its nesting level: one indent (`ui.spacing().indent`) per level.
    pub fn depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    /// Draws a guide down the row for each of its levels (`list-guide`).
    pub fn guides(mut self, on: bool) -> Self {
        self.guides = on;
        self
    }

    /// Its arrow column ([`Disclosure`]).
    pub fn disclosure(mut self, disclosure: Disclosure) -> Self {
        self.disclosure = disclosure;
        self
    }

    /// A collapsing row, open or closed: a click on its arrow, Right or
    /// Left say [`ItemResponse::toggled`].
    pub fn collapsing(self, open: bool) -> Self {
        self.disclosure(Disclosure::branch(open))
    }

    /// A drag handle at its left end, shown with its actions
    /// ([`ItemResponse::handle`]).
    pub fn drag_handle(mut self, on: bool) -> Self {
        self.handle = on;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Drawn pressed (`active`) though the pointer is not on it: a row
    /// being dragged, the target of what happens elsewhere.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Greyed and not interactive.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// An action button at the right end (added right to left: the first
    /// is outermost).
    pub fn action(mut self, action: ItemAction) -> Self {
        self.actions.push(action);
        self
    }

    /// Several actions ([`ListItem::action`]).
    pub fn actions(mut self, actions: impl IntoIterator<Item = ItemAction>) -> Self {
        self.actions.extend(actions);
        self
    }

    /// When the actions show (default: [`Reveal::Hover`]).
    pub fn reveal(mut self, reveal: Reveal) -> Self {
        self.reveal = reveal;
        self
    }

    /// Its height (default: [`row_height`]).
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Its id, stable when rows come and go (default: the next id of the
    /// `Ui`).
    pub fn id_salt(mut self, salt: impl AsIdSalt) -> Self {
        self.id_salt = Some(IdSalt::new(salt));
        self
    }

    /// Its accessible role (default: `ListItem`; `TreeItem` in a tree).
    pub fn role(mut self, role: Role) -> Self {
        self.role = role;
        self
    }

    /// Draws the row across the width available.
    pub fn show(self, ui: &mut Ui) -> ItemResponse {
        let h = self.height.unwrap_or_else(|| row_height(ui));
        let (auto, rect) = ui.allocate_space(vec2(ui.available_width(), h));
        let id = self
            .id_salt
            .map_or(auto, |salt| ui.scope_id().with_salt(salt));
        let mut row = ui.new_child(UiBuilder::new().max_rect(rect).id_salt(id));
        if self.disabled {
            row.disable();
        }
        self.draw(&mut row, id, rect)
    }

    fn draw(self, ui: &mut Ui, id: Id, rect: Rect) -> ItemResponse {
        let kit = Kit::of(ui.ctx());
        let enabled = ui.is_enabled();
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        // What has the focus before the row takes its place in the Tab
        // order (which may move it on this pass).
        let ids: Vec<Id> = (0..self.actions.len()).map(|i| action_id(id, i)).collect();
        let before = ui.memory(|m| m.focused());
        let action_focused = before.is_some_and(|f| ids.contains(&f));
        let row_focused = before == Some(id);
        let response = ui.interact(rect, id, sense);
        let hovered = enabled && response.contains_pointer();
        let state = State::REST
            .with(State::SELECTED, self.selected)
            .with(State::HOVER, hovered && !self.selected)
            .with(State::FOCUS, response.has_focus())
            .with(
                State::ACTIVE,
                self.active || response.is_pointer_button_down_on(),
            )
            .with(State::DISABLED, !enabled);
        let revealed = self.reveal == Reveal::Always
            || hovered
            || self.selected
            || row_focused
            || response.has_focus()
            || action_focused;

        // The keyboard: Right and Left open and close a collapsing row.
        let mut toggled = false;
        let branch = self.disclosure.is_branch();
        if branch && response.has_focus() {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    id,
                    EventFilter {
                        horizontal_arrows: true,
                        ..Default::default()
                    },
                )
            });
            let open = self.disclosure == Disclosure::Open;
            let key = if open {
                Key::ArrowLeft
            } else {
                Key::ArrowRight
            };
            toggled |= ui.input_mut(|i| i.consume_key(Modifiers::NONE, key));
        }
        if branch {
            let asked = if self.disclosure == Disclosure::Open {
                accesskit::Action::Collapse
            } else {
                accesskit::Action::Expand
            };
            toggled |= ui.input(|i| i.has_accesskit_action_request(id, asked));
        }

        let frame = Frame {
            kit: &kit,
            state,
            depth: self.depth,
            guides: self.guides,
            disclosure: self.disclosure,
            handle: self.handle,
            icon: self.icon.as_ref(),
        };
        let placed = frame.paint(ui, rect, revealed);
        if branch
            && response.clicked()
            && let (Some(toggle), Some(p)) = (placed.toggle, response.interact_pointer_pos())
            && toggle.contains(p)
        {
            toggled = true;
        }
        if let Some(name) = self.icon.as_ref().and_then(Source::icon_name) {
            egui_pop_core::icon::registry::record(ui.ctx(), "list-item", &self.label, name);
        }

        // The right end: actions, then the trailing text.
        let shown: Vec<usize> = (0..self.actions.len())
            .filter(|&i| revealed || self.actions[i].pinned)
            .collect();
        let rects = action_rects(ui, rect, rect.right(), shown.len());
        let mut right = rects.last().map_or(rect.right(), |r| r.left());
        if let Some(text) = &self.trailing {
            right = paint_trailing(ui, &kit, state, rect, placed.left, right, text);
        } else if rects.is_empty() {
            right -= ui.spacing().item_spacing.x;
        }
        let elided = paint_label(ui, rect, placed.left, right, &self.label, placed.color);

        // The row's accessible node, before its buttons'.
        let label = self.label.clone();
        let (selected, role) = (self.selected, self.role);
        response.widget_info(move || WidgetInfo::selected(role, enabled, selected, &label));
        let response = if elided {
            full_label_tip(response, &self.label)
        } else {
            response
        };
        let open = self.disclosure == Disclosure::Open;
        let depth = self.depth;
        let labels: Vec<String> = self.actions.iter().map(|a| a.label.clone()).collect();
        ui.ctx().accesskit_node_builder(id, |node| {
            if branch {
                node.set_expanded(open);
                node.add_action(if open {
                    accesskit::Action::Collapse
                } else {
                    accesskit::Action::Expand
                });
            }
            if depth > 0 {
                node.set_level(depth + 1);
            }
        });
        custom_actions(ui, id, &labels);
        let mut action = requested_action(ui, id)
            .and_then(|i| self.actions.get(i))
            .map(|a| a.id.clone());

        let handle = placed.handle.map(|r| {
            let h = ui.interact(r, id.with("handle"), Sense::DRAG);
            pointer::drag_cursor(&h, DragKind::Move);
            let name = tr_args(ui.ctx(), &s::ITEM_MOVE, &[("name", &self.label)]);
            h.widget_info(move || WidgetInfo::labeled(Role::Button, enabled, &name));
            h
        });

        for (&i, r) in shown.iter().zip(&rects) {
            let a = &self.actions[i];
            if action_button(ui, &kit, ids[i], *r, a, state).clicked() {
                action = Some(a.id.clone());
            }
        }
        if toggled || action.is_some() {
            ui.ctx().request_repaint();
        }
        ItemResponse {
            response,
            toggled,
            action,
            handle,
        }
    }
}

/// The id of a row's action `i`.
pub(crate) fn action_id(row: Id, i: usize) -> Id {
    row.with(("item_action", i))
}

/// The start of a row, as the tree, the list and the items share it.
pub(crate) struct Frame<'a> {
    pub kit: &'a Kit,
    pub state: State,
    pub depth: usize,
    pub guides: bool,
    pub disclosure: Disclosure,
    pub handle: bool,
    pub icon: Option<&'a Source>,
}

/// Where a row's pieces went.
pub(crate) struct Placed {
    /// The arrow's column.
    pub toggle: Option<Rect>,
    /// The drag handle's column.
    pub handle: Option<Rect>,
    /// Where the label starts.
    pub left: f32,
    /// The row's text colour.
    pub color: Color32,
}

impl Frame<'_> {
    /// Paints the box (`list-item`), the guides, the handle (when
    /// `revealed`), the arrow and the icon, left to right.
    pub fn paint(&self, ui: &Ui, rect: Rect, revealed: bool) -> Placed {
        let cx = KitCx {
            painter: ui.painter(),
            ppp: ui.ctx().pixels_per_point(),
        };
        let item = &self.kit.list_item;
        item.paint(
            &cx,
            &Part {
                rect,
                room: rect,
                state: self.state,
                label: None,
            },
        );
        let color = item.style(self.state).text;
        let indent = ui.spacing().indent;
        let mut x = rect.left();
        let handle = self.handle.then(|| {
            let r = Rect::from_min_size(pos2(x, rect.top()), vec2(indent, rect.height()));
            x += indent;
            r
        });
        if let Some(r) = handle
            && revealed
        {
            let side = Size::Inline.points(&Tokens::of(ui.ctx()));
            let muted = item.style_toned(Tone::Muted, self.state).text;
            Icon::Grip.paint(
                ui.painter(),
                Rect::from_center_size(r.center(), vec2(side, side)),
                muted,
            );
        }
        if self.guides {
            for level in 0..self.depth {
                let gx = x + level as f32 * indent + indent / 2.0;
                let at = Rect::from_min_max(pos2(gx, rect.top()), pos2(gx, rect.bottom()));
                self.kit.list_guide.paint(
                    &cx,
                    &Part {
                        rect: at,
                        room: at,
                        state: State::REST,
                        label: None,
                    },
                );
            }
        }
        x += self.depth as f32 * indent;
        let toggle = self
            .disclosure
            .has_column()
            .then(|| Rect::from_min_size(pos2(x, rect.top()), vec2(indent, rect.height())));
        let mut left = match toggle {
            Some(t) => {
                if self.disclosure.is_branch() {
                    let glyph = if self.disclosure == Disclosure::Open {
                        Glyph::DisclosureOpen
                    } else {
                        Glyph::DisclosureClosed
                    };
                    self.kit.glyph(glyph).paint(ui.painter(), t, color);
                }
                t.right()
            }
            None => x + ui.spacing().item_spacing.x,
        };
        if let Some(source) = self.icon {
            let side = Size::Inline.points(&Tokens::of(ui.ctx()));
            let at =
                Rect::from_center_size(pos2(left + side / 2.0, rect.center().y), vec2(side, side));
            source.paint(ui, at, color);
            left += side + ui.spacing().icon_spacing;
        }
        Placed {
            toggle,
            handle,
            left,
            color,
        }
    }
}

/// The side of a row's action button.
fn action_side(ui: &Ui, rect: Rect) -> f32 {
    let t = Tokens::of(ui.ctx());
    (t.size.icon_inline + 2.0 * t.space.xs).min(rect.height())
}

/// The squares of `n` action buttons, right to left from `right`.
pub(crate) fn action_rects(ui: &Ui, rect: Rect, right: f32, n: usize) -> Vec<Rect> {
    let side = action_side(ui, rect);
    (0..n)
        .map(|k| {
            Rect::from_center_size(
                pos2(right - side * (k as f32 + 0.5), rect.center().y),
                vec2(side, side),
            )
        })
        .collect()
}

/// An action button: `icon-button`'s box, the icon in the row's muted
/// text until hovered or focused.
pub(crate) fn action_button(
    ui: &Ui,
    kit: &Kit,
    id: Id,
    rect: Rect,
    action: &ItemAction,
    row: State,
) -> Response {
    let r = ui.interact(rect, id, Sense::click());
    let state = State::from_response(&r);
    let cx = KitCx {
        painter: ui.painter(),
        ppp: ui.ctx().pixels_per_point(),
    };
    let button = &kit.icon_button;
    button.paint(
        &cx,
        &Part {
            rect,
            room: rect,
            state,
            label: None,
        },
    );
    let lit = state.contains(State::HOVER) || state.contains(State::FOCUS);
    let color = if lit {
        button.style(state).text
    } else {
        kit.list_item.style_toned(Tone::Muted, row).text
    };
    let side = Size::Inline
        .points(&Tokens::of(ui.ctx()))
        .min(rect.height());
    action.icon.paint(
        ui,
        Rect::from_center_size(rect.center(), vec2(side, side)),
        color,
    );
    let label = action.label.clone();
    let enabled = ui.is_enabled();
    r.widget_info(move || WidgetInfo::labeled(Role::Button, enabled, &label));
    r.on_hover_text(&action.label)
}

/// Paints a row's trailing text right-aligned before `right`; returns
/// where the label must stop.
pub(crate) fn paint_trailing(
    ui: &Ui,
    kit: &Kit,
    state: State,
    rect: Rect,
    left: f32,
    right: f32,
    text: &str,
) -> f32 {
    let pad = ui.spacing().item_spacing.x;
    let color = kit.list_item.style_toned(Tone::Muted, state).text;
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        FontSelection::Style(TextStyle::Body).resolve(ui.style()),
        color,
    );
    let end = right - pad;
    let start = (end - galley.size().x).max(left);
    let clip = Rect::from_min_max(pos2(start, rect.top()), pos2(end, rect.bottom()));
    ui.painter()
        .with_clip_rect(clip.intersect(ui.clip_rect()))
        .galley(
            pos2(start, rect.center().y - galley.size().y / 2.0),
            galley,
            color,
        );
    start - pad
}

/// Paints a row's label from `left` on one line, elided with "…" when it
/// does not fit before `right`; returns whether it was elided (the row
/// then shows the full label on hover, [`full_label_tip`]).
pub(crate) fn paint_label(
    ui: &Ui,
    rect: Rect,
    left: f32,
    right: f32,
    text: &str,
    color: Color32,
) -> bool {
    let job = LayoutJob::simple(
        text.to_owned(),
        TextStyle::Body.resolve(ui.style()),
        color,
        f32::INFINITY,
    );
    paint_label_job(ui, rect, left, right, job, color)
}

/// [`paint_label`] for a label already laid out in sections (a filter's
/// matches, highlighted).
pub(crate) fn paint_label_job(
    ui: &Ui,
    rect: Rect,
    left: f32,
    right: f32,
    mut job: LayoutJob,
    color: Color32,
) -> bool {
    let width = (right - left).max(0.0);
    job.wrap = TextWrapping::truncate_at_width(width);
    let galley = ui.painter().layout_job(job);
    let elided = galley.elided;
    let clip = Rect::from_min_max(pos2(left, rect.top()), pos2(right.max(left), rect.bottom()));
    ui.painter()
        .with_clip_rect(clip.intersect(ui.clip_rect()))
        .galley(
            pos2(left, rect.center().y - galley.size().y / 2.0),
            galley,
            color,
        );
    elided
}

/// Shows a row's full label as its tooltip, for a label drawn elided. The
/// row's accessible node is left as it is (its name is already the full
/// label; egui's `on_hover_text` would add it again as a description).
pub(crate) fn full_label_tip(response: Response, label: &str) -> Response {
    let label = label.to_owned();
    let tip = move |ui: &mut Ui| {
        ui.set_max_width(ui.spacing().tooltip_width);
        ui.add(egui::Label::new(label));
    };
    if response.enabled() {
        response.on_hover_ui(tip)
    } else {
        response.on_disabled_hover_ui(tip)
    }
}

/// Where a list's default row drawing leaves "its label was elided" for
/// the list (keyed by the row's `Ui`).
const ELIDED: &str = "egui_pop_row_label_elided";

/// Draws a row's label in `ui`'s whole rect (a list's default row), elided
/// as [`paint_label`] does; the list reads [`take_elided`] afterwards.
pub(crate) fn row_label(ui: &Ui, text: &str, color: Color32) {
    let rect = ui.max_rect();
    if paint_label(ui, rect, rect.left(), rect.right(), text, color) {
        ui.data_mut(|d| d.insert_temp(ui.scope_id().with(ELIDED), true));
    }
}

/// Whether [`row_label`] elided the label it drew in `ui` (and forgets
/// it).
pub(crate) fn take_elided(ui: &Ui) -> bool {
    ui.data_mut(|d| d.remove_temp::<bool>(ui.scope_id().with(ELIDED)))
        .unwrap_or(false)
}

/// Lists a row's actions on its accessible node, as custom actions
/// numbered in order.
pub(crate) fn custom_actions(ui: &Ui, row: Id, labels: &[String]) {
    if labels.is_empty() {
        return;
    }
    ui.ctx().accesskit_node_builder(row, |node| {
        for (i, label) in labels.iter().enumerate() {
            node.push_custom_action(accesskit::CustomAction {
                id: i as i32,
                description: label.clone(),
            });
        }
    });
}

/// The custom action a screen reader asked of a row this pass.
pub(crate) fn requested_action(ui: &Ui, row: Id) -> Option<usize> {
    ui.input(|i| {
        i.accesskit_action_requests(row, accesskit::Action::CustomAction)
            .find_map(|r| match r.data {
                Some(accesskit::ActionData::CustomAction(n)) => usize::try_from(n).ok(),
                _ => None,
            })
    })
}
