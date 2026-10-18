//! The search bar above an editor: find (case, whole word, regular
//! expression), replace one or all, or go to a line.

use egui::{Id, Key, Ui};
use egui_pop_api::AppHandle;
use egui_pop_core::icon::Icon;
use egui_pop_core::kit::{Glyph, Kit, KitCx, Part, State};
use egui_pop_core::strings::{tr, tr_args};
use egui_pop_core::theme::Tokens;

use crate::model::EditorState;
use crate::steps;
use crate::strings as s;
use crate::view::Signal;

/// Names a field for assistive technology (egui's hint is no name).
fn name(ui: &Ui, id: Id, label: &str) {
    let label = label.to_string();
    ui.ctx().accesskit_node_builder(id, |n| n.set_label(label));
}

/// A small toggle button, named.
fn toggle(ui: &mut Ui, on: &mut bool, text: &str, label: &str) -> bool {
    let r = ui
        .selectable_label(*on, egui::RichText::new(text).monospace())
        .on_hover_text(label);
    let lab = label.to_string();
    let value = *on;
    r.widget_info(move || {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, value, &lab)
    });
    if r.clicked() {
        *on = !*on;
        return true;
    }
    false
}

/// A toggle showing a kit glyph (the replace row's disclosure) in a
/// one-character monospace cell, like the text toggles next to it.
fn glyph_toggle(ui: &mut Ui, on: &mut bool, glyph: Icon, label: &str) -> bool {
    let r = ui
        .selectable_label(*on, egui::RichText::new(" ").monospace())
        .on_hover_text(label);
    let side = r.rect.height() * 0.6;
    let at = egui::Rect::from_center_size(r.rect.center(), egui::vec2(side, side));
    let color = ui.style().interact_selectable(&r, *on).text_color();
    glyph.paint(ui.painter(), at, color);
    let lab = label.to_string();
    let value = *on;
    r.widget_info(move || {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, value, &lab)
    });
    if r.clicked() {
        *on = !*on;
        return true;
    }
    false
}

fn icon_button(ui: &mut Ui, icon: Icon, label: &str) -> bool {
    let size = ui.spacing().interact_size.y;
    let (rect, r) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let kit = Kit::of(ui.ctx());
        let button = &kit.find_bar_button;
        let part = Part {
            rect,
            room: rect,
            state: State::from_response(&r),
            label: None,
        };
        let cx = KitCx {
            painter: ui.painter(),
            ppp: ui.ctx().pixels_per_point(),
        };
        button.paint(&cx, &part);
        icon.paint(
            ui.painter(),
            rect.shrink(size * 0.22),
            button.style(part.state).text,
        );
    }
    egui_pop_core::a11y::name(&r, egui::WidgetType::Button, label);
    r.on_hover_text(label).clicked()
}

/// Draws the bar.
pub(crate) fn show(
    ui: &mut Ui,
    state: &EditorState,
    app: Option<&AppHandle>,
    editor: Id,
    signals: &mut Vec<Signal>,
) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let origin = "user";
    let (goto, focus) = {
        let mut m = state.lock();
        (m.find.goto.clone(), std::mem::take(&mut m.find.focus))
    };
    let field_w = 220.0;
    Kit::of(&ctx)
        .find_bar
        .show(ui, State::REST, egui::Shadow::NONE, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(t.space.xs, t.space.xs);
            if let Some(mut text) = goto {
                ui.horizontal(|ui| {
                    let id = editor.with("goto");
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut text)
                            .id(id)
                            .hint_text(tr(&ctx, &s::GOTO))
                            .desired_width(field_w),
                    );
                    name(ui, id, &tr(&ctx, &s::GOTO));
                    if focus {
                        r.request_focus();
                    }
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    let close =
                        icon_button(ui, Kit::of(&ctx).glyph(Glyph::Close), &tr(&ctx, &s::CLOSE));
                    if enter && let Ok(l) = text.trim().parse::<usize>() {
                        steps::goto(state, l);
                        close_bar(state);
                    } else if close {
                        close_bar(state);
                        state.request_focus();
                    } else {
                        state.lock().find.goto = Some(text);
                    }
                });
                return;
            }
            let (mut query, mut replacement, replace_open) = {
                let m = state.lock();
                (
                    m.find.query.clone(),
                    m.find.replacement.clone(),
                    m.find.replace,
                )
            };
            let mut changed = false;
            let mut next = None;
            let mut replace_one = false;
            let mut replace_all = false;
            let mut close = false;
            let mut show_replace = replace_open;
            ui.horizontal(|ui| {
                let kit = egui_pop_core::kit::Kit::of(&ctx);
                let glyph = kit.glyph(if show_replace {
                    egui_pop_core::kit::Glyph::DisclosureOpen
                } else {
                    egui_pop_core::kit::Glyph::DisclosureClosed
                });
                if glyph_toggle(ui, &mut show_replace, glyph, &tr(&ctx, &s::TOGGLE_REPLACE)) {
                    state.lock().find.replace = show_replace;
                }
                let id = editor.with("find");
                let r = ui.add(
                    egui::TextEdit::singleline(&mut query.text)
                        .id(id)
                        .hint_text(tr(&ctx, &s::FIND))
                        .desired_width(field_w),
                );
                name(ui, id, &tr(&ctx, &s::FIND));
                if focus {
                    r.request_focus();
                }
                changed |= r.changed();
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    next = Some(!ui.input(|i| i.modifiers.shift));
                    r.request_focus();
                }
                changed |= toggle(
                    ui,
                    &mut query.case_sensitive,
                    "Aa",
                    &tr(&ctx, &s::MATCH_CASE),
                );
                changed |= toggle(ui, &mut query.whole_word, "ab", &tr(&ctx, &s::WHOLE_WORD));
                changed |= toggle(ui, &mut query.regex, ".*", &tr(&ctx, &s::REGEX));
                let (count, current, error) = {
                    let mut m = state.lock();
                    if changed {
                        m.find.query = query.clone();
                    }
                    m.refresh_matches();
                    (m.find.matches.len(), m.find.current, m.find.error.clone())
                };
                let text = match (error, count) {
                    (Some(e), _) => e,
                    (None, 0) => tr(&ctx, &s::NO_RESULTS),
                    (None, n) => tr_args(
                        &ctx,
                        &s::COUNT,
                        &[
                            ("current", &current.map_or(0, |c| c + 1).to_string()),
                            ("total", &n.to_string()),
                        ],
                    ),
                };
                // A fixed width, so the buttons after it never move.
                let h = ui.spacing().interact_size.y;
                ui.add_sized(
                    [96.0, h],
                    egui::Label::new(egui::RichText::new(text).weak()).truncate(),
                );
                if icon_button(ui, Icon::ChevronUp, &tr(&ctx, &s::PREVIOUS)) {
                    next = Some(false);
                }
                if icon_button(ui, Icon::ChevronDown, &tr(&ctx, &s::NEXT)) {
                    next = Some(true);
                }
                close = icon_button(ui, Kit::of(&ctx).glyph(Glyph::Close), &tr(&ctx, &s::CLOSE));
            });
            if show_replace {
                ui.horizontal(|ui| {
                    ui.add_space(ui.spacing().interact_size.y + t.space.xs);
                    let id = editor.with("replace");
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut replacement)
                            .id(id)
                            .hint_text(tr(&ctx, &s::REPLACE))
                            .desired_width(field_w),
                    );
                    name(ui, id, &tr(&ctx, &s::REPLACE));
                    if r.changed() {
                        state.lock().find.replacement = replacement.clone();
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        replace_one = true;
                        r.request_focus();
                    }
                    replace_one |= ui.button(tr(&ctx, &s::REPLACE_ONE)).clicked();
                    replace_all = ui.button(tr(&ctx, &s::REPLACE_ALL)).clicked();
                });
            }
            if changed {
                // Typing a query jumps to the first match from the cursor.
                let mut m = state.lock();
                let from = m.selection.span().start;
                m.refresh_matches();
                if let Some(i) = crate::search::next_from(&m.find.matches, from) {
                    let hit = m.find.matches[i];
                    m.find.current = Some(i);
                    m.select(crate::selection::Selection::new(hit.start, hit.end));
                }
            }
            if let Some(forward) = next {
                steps::find_step(state, forward);
            }
            if replace_one {
                match steps::replace_one(state, app, origin) {
                    Ok(true) => signals.push(Signal::Changed {
                        version: state.version(),
                        kind: "replace".into(),
                    }),
                    Ok(false) => {}
                    Err(reason) => signals.push(Signal::Refused {
                        step: steps::EDIT_CHANGE,
                        reason,
                    }),
                }
            }
            if replace_all {
                let q = state.lock().find.query.clone();
                match state.replace_all(app, &q, &replacement, origin) {
                    Ok(n) if n > 0 => signals.push(Signal::Changed {
                        version: state.version(),
                        kind: "replace".into(),
                    }),
                    Ok(_) => {}
                    Err(reason) => signals.push(Signal::Refused {
                        step: steps::EDIT_CHANGE,
                        reason,
                    }),
                }
            }
            if close {
                close_bar(state);
                state.request_focus();
            }
        });
    // Escape in the bar (once the field let go of it) closes the bar.
    let bar_has_focus = [
        editor.with("find"),
        editor.with("replace"),
        editor.with("goto"),
    ]
    .iter()
    .any(|id| ctx.memory(|m| m.has_focus(*id)));
    let escape = ctx.input(|i| i.key_pressed(Key::Escape));
    let editor_focused = ctx.memory(|m| m.has_focus(editor));
    if escape && !bar_has_focus && !editor_focused && state.lock().find.open {
        close_bar(state);
        state.request_focus();
    }
}

fn close_bar(state: &EditorState) {
    let mut m = state.lock();
    m.find.open = false;
    m.find.goto = None;
}
