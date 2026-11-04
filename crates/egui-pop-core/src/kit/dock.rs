//! The kit paints egui_stack's docks: its tabs (`tab`), tab bars
//! (`tab-bar`), separators (`splitter`) and drop overlay
//! (`dock-preview`, its buttons `button`). egui_stack lays them out and
//! takes their input.

use egui::{Painter, Rect, vec2};
use egui_stack::{
    DockPainter, DropButtonPaint, DropPaint, DropTargetPaint, PartState, SeparatorAxis,
    SeparatorPaint, Split, TabBarPaint, TabBarTailPaint, TabMeasure, TabPaint,
};

use super::{Kit, KitCx, Part, State};

/// The kit's state for egui_stack's flags (`DRAGGED` and `LEAF_FOCUSED`
/// have no kit state).
fn state(flags: PartState) -> State {
    State::REST
        .with(State::SELECTED, flags.contains(PartState::SELECTED))
        .with(State::HOVER, flags.contains(PartState::HOVERED))
        .with(State::ACTIVE, flags.contains(PartState::PRESSED))
        .with(State::FOCUS, flags.contains(PartState::FOCUSED))
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
