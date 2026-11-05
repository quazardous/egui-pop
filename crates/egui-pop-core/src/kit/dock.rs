//! The kit paints egui_stack's docks: its tabs (`tab`), tab bars
//! (`tab-bar`), separators (`splitter`) and drop overlay
//! (`dock-preview`, its buttons `button`); and its section stacks
//! ([`crate::sections`]): their bands (`section-band`, the mark
//! `section-disclosure`, the count `section-count`) and the focus ring of
//! their boundaries; and its toolbar rows ([`egui_stack::toolbar`]): their
//! tools (`toolbar-button`, a row of their More menu `menu-item`), More
//! button (`toolbar-button`, the More glyph) and separators. egui_stack
//! lays them out and takes their input.

use egui::{Color32, FontFamily, FontId, Painter, Rect, pos2, vec2};
use egui_stack::toolbar::ToolKind;
use egui_stack::{
    DockPainter, DropButtonPaint, DropPaint, DropTargetPaint, PartState, SectionBandPaint,
    SectionBoundaryPaint, SeparatorAxis, SeparatorPaint, Split, TabBarPaint, TabBarTailPaint,
    TabMeasure, TabPaint, ToolItemPaint, ToolMorePaint, ToolSeparatorPaint,
};

use super::{Corners, Glyph, Kit, KitCx, Part, State, Tone, paint_ring};
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

fn cx(painter: &Painter) -> KitCx<'_> {
    KitCx {
        painter,
        ppp: painter.pixels_per_point(),
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

    fn separator(&self, painter: &Painter, separator: &SeparatorPaint<'_>) {
        // The kit's line, as thick as the kit says, along the separator.
        let rect = separator.rect;
        let width = self.splitter.data.width;
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
        self.splitter.paint(
            &cx(painter),
            &Part {
                rect: line,
                room: line,
                state: state(separator.state),
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
        // As between areas: the `dock-preview` at rest where the tab may
        // go, disabled with its label saying why where it may not.
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

        // The count at the end.
        let mut end = rect.right() - data.padding_x;
        if let Some(n) = band.count {
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
            let pill = Rect::from_min_size(
                pos2(end - measured.x, rect.center().y - measured.y / 2.0),
                measured,
            );
            count.paint_part(
                &cx,
                &Part {
                    rect: pill,
                    room: pill,
                    state: State::REST,
                    label: Some(&galley),
                },
            );
            end = pill.left() - gap;
        }

        // The title between them, clipped to its room.
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

impl Kit {
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
