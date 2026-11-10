//! The kit paints egui_stack's docks: its tabs (`tab`), tab bars
//! (`tab-bar`), the borders of the panels' bodies (`dock-body`),
//! separators (`dock-separator`) and drop zones (`dock-preview`:
//! the region a dragged tab would take, bordered and tinted, on the node
//! itself; the buttons of egui_stack's former cross overlay, when an
//! application asks for it, in `button`); and its section stacks
//! ([`crate::sections`]): their bands (`section-band`, the mark
//! `section-disclosure`, the count `section-count`) and the focus ring of
//! their boundaries; and its toolbar rows ([`egui_stack::toolbar`]): their
//! tools (`toolbar-button`, a row of their More menu `menu-item`), More
//! button (`toolbar-button`, the More glyph) and separators; and the
//! shell's layout ([`egui_stack::layout`]): the lines between its areas,
//! the folded areas' strips (`area-strip`). egui_stack lays them out and
//! takes their input.

use egui::{
    Color32, CornerRadius, FontFamily, FontId, Painter, Rect, Shadow, Stroke, StrokeKind, Vec2,
    pos2, vec2,
};
use egui_stack::layout::{FlyoutPaint, LayoutPainter, SashPaint, Shown, StripPaint};
use egui_stack::toolbar::ToolKind;
use egui_stack::{
    DockPainter, DropButtonPaint, DropPaint, DropTargetPaint, PartState, SectionBandPaint,
    SectionBoundaryPaint, SeparatorAxis, SeparatorPaint, Split, TabBarPaint, TabBarTailPaint,
    TabBodyPaint, TabMeasure, TabPaint, ToolItemPaint, ToolMorePaint, ToolSeparatorPaint,
};

use super::{
    BoxStyle, Corners, Glyph, Kit, KitCx, Part, Side, State, Tone, paint_box, paint_indicator,
    paint_ring,
};
use crate::icon::Size;
use crate::indicator;
use crate::progress::paint_spinner_on;
use crate::theme::{TextRole, Tokens};

/// The kit's state for egui_stack's flags (`DRAGGED` and `LEAF_FOCUSED`
/// have no kit state).
fn state(flags: PartState) -> State {
    State::REST
        .with(State::SELECTED, flags.contains(PartState::SELECTED))
        .with(State::HOVER, flags.contains(PartState::HOVERED))
        .with(State::ACTIVE, flags.contains(PartState::PRESSED))
        .with(State::FOCUS, flags.contains(PartState::FOCUSED))
        .with(State::DISABLED, flags.contains(PartState::DISABLED))
}

/// A tool's state: a disabled tool is neither hovered nor pressed (egui
/// says so of a disabled widget), and `selected` only when `selected`
/// is kept (a toggle on, not a menu open: the toolbar frames what is on).
fn tool_state(flags: PartState, selected: bool) -> State {
    let disabled = flags.contains(PartState::DISABLED);
    State::REST
        .with(
            State::SELECTED,
            selected && flags.contains(PartState::SELECTED),
        )
        .with(
            State::HOVER,
            !disabled && flags.contains(PartState::HOVERED),
        )
        .with(
            State::ACTIVE,
            !disabled && flags.contains(PartState::PRESSED),
        )
        .with(State::FOCUS, flags.contains(PartState::FOCUSED))
        .with(State::DISABLED, disabled)
}

/// A panel body's border in a dock, the kit's `dock-body` in the body's
/// state (`hover` under the pointer, `focus` for the dock's focused
/// leaf): drawn inside the body, its top edge hidden under the tab bar as
/// egui_stack's own border is, its free corners rounded by the state's
/// `radius` (those joining the tab bar stay square), then its glow (the
/// state's ring). Nothing when the state has neither.
fn paint_body(kit: &Kit, painter: &Painter, body: &TabBodyPaint<'_>) {
    let flags = body.state;
    let state = State::REST
        .with(State::HOVER, flags.contains(PartState::HOVERED))
        .with(
            State::FOCUS,
            flags.contains(PartState::FOCUSED) || flags.contains(PartState::LEAF_FOCUSED),
        );
    let look = kit.dock_body.style(state);
    let ppp = painter.pixels_per_point();
    let r = look.radius.round().clamp(0.0, 255.0) as u8;
    // The top corners join the tab bar where egui_stack squares them.
    let corners = if body.corner_radius.nw == 0 && body.corner_radius.ne == 0 {
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: r,
            se: r,
        }
    } else {
        CornerRadius::same(r)
    };
    if look.border_width > 0.0 && look.border_color.a() > 0 {
        let width = crate::scale::snap(look.border_width, ppp);
        // As egui_stack draws it: the top edge goes up behind the clip
        // rect, out of sight, by an even number of points, so that an
        // odd width leaves no anti-aliased line.
        let lift = (width / 2.0).ceil() * 2.0;
        let rect = Rect::from_min_max(body.rect.min - vec2(0.0, lift), body.rect.max);
        painter.rect_stroke(
            rect.expand(-(width / 2.0).ceil()),
            corners,
            Stroke::new(width, look.border_color),
            StrokeKind::Inside,
        );
    }
    if look.ring_width > 0.0 && look.ring_color.a() > 0 {
        // The glow keeps the border's centre, as a focus ring does:
        // larger corners outside the body, smaller inside.
        let ring = (look.radius + look.ring_offset).round().clamp(0.0, 255.0) as u8;
        let joined = corners.nw == 0;
        painter.rect_stroke(
            body.rect.expand(look.ring_offset),
            CornerRadius {
                nw: if joined { 0 } else { ring },
                ne: if joined { 0 } else { ring },
                sw: ring,
                se: ring,
            },
            Stroke::new(crate::scale::snap(look.ring_width, ppp), look.ring_color),
            StrokeKind::Outside,
        );
    }
}

fn cx(painter: &Painter) -> KitCx<'_> {
    KitCx {
        painter,
        ppp: painter.pixels_per_point(),
    }
}

impl Kit {
    /// The breadth an area keeps for the line on its side facing the
    /// centre, as an egui side panel reserved it in its frame's outer
    /// margin: the resting line's width when the kit's `side-area` has a
    /// border (the line shows at rest), else none.
    pub fn area_line_room(&self, style: &egui::Style) -> f32 {
        if self.side_area.style(State::REST).border_width > 0.0 {
            style.visuals.widgets.noninteractive.bg_stroke.width.round()
        } else {
            0.0
        }
    }

    /// The line between two areas (an egui side panel's separator): shown
    /// at rest only when the kit's `side-area` has a border, highlighted
    /// while hovered, focused or dragged. `inward` points from the
    /// boundary into the area the line belongs to (zero: centred on the
    /// boundary); the line then lies in the room the area keeps for it
    /// ([`Kit::area_line_room`]), where egui drew a panel's separator.
    pub fn paint_area_line(&self, painter: &Painter, sash: &SashPaint<'_>, inward: Vec2) {
        let style = painter.ctx().global_style();
        let widgets = &style.visuals.widgets;
        let flags = sash.state;
        let stroke = if flags.contains(PartState::DRAGGED) {
            widgets.active.fg_stroke
        } else if flags.contains(PartState::HOVERED) || flags.contains(PartState::FOCUSED) {
            widgets.hovered.fg_stroke
        } else if self.side_area.style(State::REST).border_width > 0.0 {
            widgets.noninteractive.bg_stroke
        } else {
            return;
        };
        let room = self.area_line_room(&style);
        let c = sash.rect.center() + inward * (room - 0.5 * stroke.width);
        match sash.axis {
            SeparatorAxis::Vertical => {
                painter.vline(c.x, sash.rect.y_range(), stroke);
            }
            SeparatorAxis::Horizontal => {
                painter.hline(sash.rect.x_range(), c.y, stroke);
            }
        }
    }

    /// A folded area's strip (`area-strip`): its box, the line on the side
    /// facing the centre (`inner`), and the drag-handle glyph near its top.
    pub fn paint_area_strip(&self, painter: &Painter, rect: Rect, state: State, inner: Side) {
        let component = &self.area_strip;
        let grip = component.data.width.max(1.0);
        let b = component.style(state);
        let cx = cx(painter);
        paint_box(
            &cx,
            rect,
            &BoxStyle {
                border_width: 0.0,
                ..*b
            },
            Corners::None,
            inner,
        );
        let line = BoxStyle {
            indicator_color: b.border_color,
            indicator_size: b.border_width,
            ..BoxStyle::default()
        };
        paint_indicator(&cx, rect, &line, inner);
        let at = Rect::from_center_size(
            pos2(rect.center().x, rect.top() + grip * 1.25),
            vec2(grip, grip),
        );
        self.glyph(Glyph::DragHandle).paint(painter, at, b.text);
    }
}

/// The kit paints egui_stack's layout: the line between two areas (as an
/// egui side panel's separator, shown at rest only when the kit's
/// `side-area` has a border), the folded areas' strips (`area-strip`) and
/// the frame of what lies over the centre (`side-area`, its shadow).
impl LayoutPainter for Kit {
    fn sash(&self, painter: &Painter, sash: &SashPaint<'_>) {
        self.paint_area_line(painter, sash, Vec2::ZERO);
    }

    fn strip(&self, painter: &Painter, strip: &StripPaint<'_>) {
        // The side facing the rest of the window, from where the strip is.
        let inner = if strip.rect.center().x < painter.clip_rect().center().x {
            Side::Right
        } else {
            Side::Left
        };
        let state = state(strip.grip_state).with(State::SELECTED, strip.shown == Shown::Over);
        self.paint_area_strip(painter, strip.rect, state, inner);
    }

    fn flyout_frame(&self, painter: &Painter, frame: &FlyoutPaint<'_>) {
        let shadow = self.side_area.data.shadow;
        if shadow != Shadow::NONE {
            painter.add(shadow.as_shape(frame.rect, CornerRadius::ZERO));
        }
        let fill = painter.ctx().global_style().visuals.panel_fill;
        painter.rect_filled(frame.rect, CornerRadius::ZERO, fill);
    }
}

impl DockPainter for Kit {
    fn tab_width(&self, tab: &TabMeasure<'_>) -> f32 {
        // The kit's padding, not egui_stack's: a kit may make tabs narrow.
        self.tab.measure(tab.title.size()).x + tab.close_width
    }

    fn tab_bar(&self, painter: &Painter, bar: &TabBarPaint<'_>) {
        self.tab_bar.paint(
            &cx(painter),
            &Part {
                rect: bar.rect,
                room: bar.rect,
                state: State::REST,
                label: None,
            },
        );
    }

    fn tab(&self, painter: &Painter, tab: &TabPaint<'_>) {
        let room = match tab.close {
            Some(close) => {
                Rect::from_x_y_ranges(tab.rect.left()..=close.rect.left(), tab.rect.y_range())
            }
            None => tab.rect,
        };
        self.tab.paint_part(
            &cx(painter),
            &Part {
                rect: tab.rect,
                room,
                state: state(tab.state),
                label: Some(tab.title),
            },
        );
    }

    fn tab_bar_tail(&self, _painter: &Painter, _tail: &TabBarTailPaint<'_>) {
        // The `tab-bar` part already spans the whole bar.
    }

    fn tab_body(&self, painter: &Painter, body: &TabBodyPaint<'_>) {
        paint_body(self, painter, body);
    }

    fn separator(&self, painter: &Painter, separator: &SeparatorPaint<'_>) {
        // The kit's line, as thick as the kit says, along the separator;
        // `active` while pressed or dragged.
        let rect = separator.rect;
        let look = &self.dock_separator;
        let width = look.data.width;
        let line = if width <= 0.0 {
            rect
        } else {
            match separator.axis {
                SeparatorAxis::Vertical => {
                    Rect::from_center_size(rect.center(), vec2(width, rect.height()))
                }
                SeparatorAxis::Horizontal => {
                    Rect::from_center_size(rect.center(), vec2(rect.width(), width))
                }
            }
        };
        let flags = separator.state;
        look.paint(
            &cx(painter),
            &Part {
                rect: line,
                room: line,
                state: state(flags).with(State::ACTIVE, flags.contains(PartState::DRAGGED)),
                label: None,
            },
        );
    }

    fn drop_target(&self, _painter: &Painter, _target: &DropTargetPaint<'_>) {
        // The preview says where the tab goes; the node under it needs no
        // frame of its own.
    }

    fn drop_button(&self, painter: &Painter, button: &DropButtonPaint<'_>) {
        let cx = cx(painter);
        let state = State::REST.with(State::HOVER, button.hovered);
        self.button.paint(
            &cx,
            &Part {
                rect: button.rect,
                room: button.rect,
                state,
                label: None,
            },
        );
        // The part of the button the tab would take: all of it, or the
        // half a split gives.
        let inner = button.rect.shrink(button.rect.width() * 0.2);
        let center = inner.center();
        let taken = match button.split {
            None => inner,
            Some(Split::Left) => inner.intersect(Rect::everything_left_of(center.x)),
            Some(Split::Right) => inner.intersect(Rect::everything_right_of(center.x)),
            Some(Split::Above) => inner.intersect(Rect::everything_above(center.y)),
            Some(Split::Below) => inner.intersect(Rect::everything_below(center.y)),
        };
        self.dock_preview.paint(
            &cx,
            &Part {
                rect: taken,
                room: taken,
                state,
                label: None,
            },
        );
    }

    fn drop_preview(&self, painter: &Painter, drop: &DropPaint<'_>) {
        // The drop zone, as between areas: the `dock-preview` at rest
        // (its border and its fill at its opacity, a light tint) over the
        // region the tab would take, disabled with its label saying why
        // where it may not go.
        let refusal = drop.refusal;
        let label = refusal.and_then(|r| r.reason.as_ref());
        if drop.from_buttons && refusal.is_none() {
            // The button already shows it.
            return;
        }
        self.dock_preview.paint_part(
            &cx(painter),
            &Part {
                rect: drop.rect,
                room: drop.rect,
                state: State::REST.with(State::DISABLED, refusal.is_some()),
                label,
            },
        );
    }

    fn section_band(&self, painter: &Painter, band: &SectionBandPaint<'_>) {
        self.paint_section_band(
            painter,
            &Band {
                rect: band.rect,
                id: band.id,
                title: band.title,
                count: band.count,
                open: band.open,
                state: band.state,
            },
        );
    }

    fn section_boundary(&self, painter: &Painter, boundary: &SectionBoundaryPaint<'_>) {
        // Nothing at rest nor under the pointer: the bands' own lines part
        // the sections. The focus shows as the band's ring, over the line.
        if !boundary.state.contains(PartState::FOCUSED) {
            return;
        }
        crate::focus::own_ring(painter.ctx(), boundary.id);
        let focus = self.section_band.data.states.get(State::FOCUS);
        paint_ring(&cx(painter), boundary.rect, focus, Corners::All);
    }

    fn tool_item(&self, painter: &Painter, item: &ToolItemPaint<'_>) {
        let cx = cx(painter);
        let ctx = painter.ctx();
        if item.state.contains(PartState::FOCUSED) {
            // The tool shows its focus itself, as any kit part does.
            crate::ring::own(ctx, item.id);
        }
        if item.in_menu {
            let on = item.state.contains(PartState::SELECTED) && item.kind != ToolKind::Menu;
            self.tool_menu_row(painter, item, on);
            return;
        }
        let look = &self.toolbar_button;
        let state = tool_state(item.state, item.kind != ToolKind::Menu);
        look.paint(
            &cx,
            &Part {
                rect: item.rect,
                room: item.rect,
                state,
                label: None,
            },
        );
        let color = look.style(state).text;
        let enabled = !state.contains(State::DISABLED);
        if item.busy {
            paint_spinner_on(painter, item.icon_rect, color);
        } else if let Some(icon) = item.icon {
            icon.paint(painter, item.icon_rect, color, enabled);
        }
        if let Some(label) = item.label {
            painter.galley(item.label_pos, label.clone(), color);
        }
        if item.kind == ToolKind::Menu {
            // The drop-down mark at the end, as wide as 0.6 icon, a
            // padding off the edge (the icon's room starts a padding in).
            let side = item.icon_rect.width() * 0.6;
            let pad = item.icon_rect.left() - item.rect.left();
            let at = Rect::from_center_size(
                pos2(item.rect.right() - pad - side / 2.0, item.rect.center().y),
                vec2(side, side),
            );
            self.glyph(Glyph::Dropdown).paint(painter, at, color);
        }
    }

    fn tool_more(&self, painter: &Painter, more: &ToolMorePaint<'_>) {
        let ctx = painter.ctx();
        if more.state.contains(PartState::FOCUSED) {
            crate::ring::own(ctx, more.id);
        }
        // Not framed while its menu is open: framed says "on".
        let state = tool_state(more.state, false);
        let look = &self.toolbar_button;
        look.paint(
            &cx(painter),
            &Part {
                rect: more.rect,
                room: more.rect,
                state,
                label: None,
            },
        );
        let side = Size::Toolbar.points(&Tokens::of(ctx));
        let at = Rect::from_center_size(more.rect.center(), vec2(side, side));
        self.glyph(Glyph::More)
            .paint(painter, at, look.style(state).text);
    }

    fn tool_separator(&self, painter: &Painter, separator: &ToolSeparatorPaint<'_>) {
        // A line across the separator's room, in the border's colour.
        let rect = separator.rect;
        let stroke = egui::Stroke::new(1.0, Tokens::of(painter.ctx()).border.default);
        let line = if separator.vertical {
            [rect.left_center(), rect.right_center()]
        } else {
            [rect.center_top(), rect.center_bottom()]
        };
        painter.line_segment(line, stroke);
    }
}

/// What a section band shows.
pub(crate) struct Band<'a> {
    pub rect: Rect,
    pub id: egui::Id,
    pub title: &'a str,
    pub count: Option<u64>,
    pub open: bool,
    pub state: PartState,
}

impl Kit {
    /// A section's band: the box, the disclosure mark, the title and the
    /// count (the stack's, and a band standing alone,
    /// [`crate::sections::band`]).
    pub(crate) fn paint_section_band(&self, painter: &Painter, band: &Band<'_>) {
        let cx = cx(painter);
        let ctx = painter.ctx();
        let tokens = Tokens::of(ctx);
        if band.state.contains(PartState::FOCUSED) {
            // The band shows its focus itself, as any kit part does.
            crate::ring::own(ctx, band.id);
        }
        let state = state(band.state);
        let rect = band.rect;
        let data = &self.section_band.data;
        let gap = tokens.space.inline;
        self.section_band.paint(
            &cx,
            &Part {
                rect,
                room: rect,
                state,
                label: None,
            },
        );

        // The disclosure mark at the start.
        let mark = &self.section_disclosure;
        let side = if mark.data.width > 0.0 {
            mark.data.width
        } else {
            10.0
        };
        let mark_rect = Rect::from_center_size(
            pos2(rect.left() + data.padding_x + side / 2.0, rect.center().y),
            vec2(side, side),
        );
        let mark_part = Part {
            rect: mark_rect,
            room: mark_rect,
            state,
            label: None,
        };
        mark.paint(&cx, &mark_part);
        let glyph = if band.open {
            Glyph::DisclosureOpen
        } else {
            Glyph::DisclosureClosed
        };
        self.glyph(glyph)
            .paint(painter, mark_rect, mark.style(state).text);

        // The count, measured first: at the end, or after the title.
        let count = band.count.map(|n| {
            let count = &self.section_count;
            let size = if count.data.font_size > 0.0 {
                count.data.font_size
            } else {
                tokens.font.small
            };
            let galley = painter.layout_no_wrap(
                indicator::badge_text(n, indicator::CAP),
                FontId::proportional(size),
                Color32::PLACEHOLDER,
            );
            let measured = count.measure(galley.size());
            (galley, measured)
        });

        // The title, after the mark.
        let role = tokens.text_role.get(TextRole::Section);
        let size = if data.font_size > 0.0 {
            data.font_size
        } else {
            role.size
        };
        let family = crate::ui::role_family(ctx, role.weight).unwrap_or(FontFamily::Proportional);
        let galley = painter.layout_no_wrap(
            band.title.to_owned(),
            FontId::new(size, family),
            Color32::PLACEHOLDER,
        );
        let title_left = mark_rect.right() + gap;

        // Where the title's room ends, and where the count goes.
        let mut end = rect.right() - data.padding_x;
        if let Some((count_galley, measured)) = &count {
            end -= measured.x + gap;
            let left = match self.sections.count_placement {
                super::CountPlacement::End => end + gap,
                super::CountPlacement::AfterTitle => {
                    // After what shows of the title.
                    let shown = galley.size().x.min(end - title_left).max(0.0);
                    title_left + shown + gap
                }
            };
            let pill =
                Rect::from_min_size(pos2(left, rect.center().y - measured.y / 2.0), *measured);
            self.section_count.paint_part(
                &cx,
                &Part {
                    rect: pill,
                    room: pill,
                    state: State::REST,
                    label: Some(count_galley),
                },
            );
        }

        // The label's room, less the padding `paint_label` takes off.
        let start = mark_rect.right() + gap - data.padding_x;
        let room = Rect::from_x_y_ranges(start..=(end + data.padding_x).max(start), rect.y_range());
        let clipped = painter.with_clip_rect(
            Rect::from_x_y_ranges(
                mark_rect.right()..=end.max(mark_rect.right()),
                rect.y_range(),
            )
            .intersect(painter.clip_rect()),
        );
        self.section_band.paint_label(
            &KitCx {
                painter: &clipped,
                ppp: cx.ppp,
            },
            &Part {
                rect,
                room,
                state,
                label: Some(&galley),
            },
        );
    }

    /// A tool as a row of a toolbar's More menu, in the look of a menu's
    /// items: the `menu-item` box, the icon in the gutter (framed by
    /// `menu-item-gutter` while on, else a tick or a dot there), the key
    /// in the `muted` tone.
    fn tool_menu_row(&self, painter: &Painter, item: &ToolItemPaint<'_>, on: bool) {
        let cx = cx(painter);
        let look = &self.menu_item;
        let state = tool_state(item.state, false);
        let disabled = state.contains(State::DISABLED);
        look.paint(
            &cx,
            &Part {
                rect: item.rect,
                room: item.rect,
                state,
                label: None,
            },
        );
        let color = look.style(state).text;
        // The gutter's mark keeps the item's resting colour, as a menu's.
        let mark = look.style(State::REST.with(State::DISABLED, disabled)).text;
        let square = item.icon_rect;
        if on && item.icon.is_some() && !item.busy {
            let pad = square.left() - item.rect.left();
            let frame = Rect::from_center_size(
                square.center(),
                vec2(square.width() + 2.0 * pad, item.rect.height()),
            );
            self.menu_item_gutter.paint(
                &cx,
                &Part {
                    rect: frame,
                    room: frame,
                    state: State::SELECTED.with(State::DISABLED, disabled),
                    label: None,
                },
            );
        }
        if item.busy {
            paint_spinner_on(painter, square, mark);
        } else if let Some(icon) = item.icon {
            icon.paint(painter, square, mark, !disabled);
        } else if on {
            let glyph = if item.kind == ToolKind::Radio {
                Glyph::Radio
            } else {
                Glyph::Check
            };
            self.glyph(glyph).paint(painter, square, mark);
        }
        if let Some(label) = item.label {
            painter.galley(item.label_pos, label.clone(), color);
        }
        if let Some(keys) = item.shortcut {
            let muted = look.style_toned(Tone::Muted, state).text;
            painter.galley(item.shortcut_pos, keys.clone(), muted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_map_onto_the_kit_states() {
        let flags = PartState::SELECTED | PartState::HOVERED | PartState::FOCUSED;
        assert_eq!(state(flags), State::SELECTED | State::HOVER | State::FOCUS);
        assert_eq!(state(PartState::PRESSED), State::ACTIVE);
        assert_eq!(
            state(PartState::DRAGGED | PartState::LEAF_FOCUSED),
            State::REST
        );
    }
}
