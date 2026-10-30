//! The viewer's panel: the toolbar (back, forward, reload, the address
//! bar, Open outside, Copy link, the zoom), the search bar, the bar of
//! images waiting for the user, then the page, its error, or nothing.
//! What the user asks for is collected while drawing and done after, the
//! viewer's lock released.

use egui::{Align, Response, RichText, Ui};
use egui_commonmark::CommonMarkViewer;
use egui_pop_api::AppHandle;
use egui_pop_api::strings as api_txt;
use egui_pop_core::focus;
use egui_pop_core::icon::{Icon, Size, Tone};
use egui_pop_core::lock;
use egui_pop_core::strings::{self, RETRY, tr, tr_args};
use egui_pop_core::theme::Tokens;
use egui_pop_model::commands::Origin;

use super::page::{Markdown, escape, resolve};
use super::{Content, Image, Status, Viewer};
use crate::strings as txt;

/// What the user asked for while the panel was drawn.
enum Action {
    Back,
    Forward,
    Reload,
    Go(String),
    Follow { dest: String, outside: bool },
    OpenOutside,
    Copy,
    LoadImages(String),
    FindStep(bool),
    FindClose,
    Find(String),
    Zoom(i32),
}

/// Draws the viewer's panel.
pub(super) fn show(ui: &mut Ui, app: &AppHandle, v: &Viewer) {
    v.poll(app);
    let mut actions = Vec::new();
    focus::scope(ui, super::SURFACE, &[], |ui| {
        toolbar(ui, v, &mut actions);
        find_bar(ui, v, &mut actions);
        images_bar(ui, v, &mut actions);
        simplified_bar(ui, v);
        ui.separator();
        body(ui, v, &mut actions);
    });
    for a in actions {
        match a {
            Action::Back => {
                v.back(app);
            }
            Action::Forward => {
                v.forward(app);
            }
            Action::Reload => {
                v.reload(app);
            }
            Action::Go(t) => v.go(app, &t),
            Action::Follow { dest, outside } => v.follow(app, &dest, outside),
            Action::OpenOutside => {
                let _ = v.open_outside(app, &Origin::User);
            }
            Action::Copy => {
                let _ = v.copy_link(app, &Origin::User);
            }
            Action::LoadImages(h) => {
                if let Err(e) = v.load_images(app, &h) {
                    log::warn!("viewer: {e}");
                }
            }
            Action::FindStep(next) => {
                v.find_step(next);
            }
            Action::FindClose => v.close_find(),
            Action::Find(q) => {
                v.find(Some(&q));
            }
            Action::Zoom(by) => {
                v.zoom_step(by);
            }
        }
        ui.ctx().request_repaint();
    }
}

fn icon_button(ui: &mut Ui, icon: Icon, name: &str, enabled: bool) -> Response {
    ui.add_enabled(enabled, icon.size(Size::Toolbar).label(name).sense_click())
        .on_hover_text(name)
}

fn toolbar(ui: &mut Ui, v: &Viewer, actions: &mut Vec<Action>) {
    let ctx = ui.ctx().clone();
    let mut i = lock(&v.inner);
    let has_page = i.current.is_some();
    ui.horizontal(|ui| {
        if icon_button(
            ui,
            Icon::ChevronLeft,
            &tr(&ctx, &txt::VIEWER_BACK),
            !i.back.is_empty(),
        )
        .clicked()
        {
            actions.push(Action::Back);
        }
        if icon_button(
            ui,
            Icon::ChevronRight,
            &tr(&ctx, &txt::VIEWER_FORWARD),
            !i.forward.is_empty(),
        )
        .clicked()
        {
            actions.push(Action::Forward);
        }
        if icon_button(ui, Icon::Reset, &tr(&ctx, &txt::VIEWER_RELOAD), has_page).clicked() {
            actions.push(Action::Reload);
        }
        let zoom = format!("{}%", (i.zoom * 100.0).round() as i32);
        let zoom_name = tr_args(&ctx, &txt::VIEWER_ZOOM, &[("zoom", &zoom)]);
        let copy = tr(&ctx, &api_txt::LINKS_COPY);
        let outside = tr(&ctx, &api_txt::LINKS_OPEN_OUTSIDE);
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(has_page, egui::Button::new(&outside))
                .clicked()
            {
                actions.push(Action::OpenOutside);
            }
            if icon_button(ui, Icon::Copy, &copy, has_page).clicked() {
                actions.push(Action::Copy);
            }
            let z = ui
                .add(egui::Button::new(&zoom).small())
                .on_hover_text(&zoom_name);
            egui_pop_core::a11y::name(&z, egui::WidgetType::Button, &zoom_name);
            if z.clicked() {
                actions.push(Action::Zoom(0));
            }
            let address = tr(&ctx, &txt::VIEWER_ADDRESS);
            let edit = ui.add(
                egui::TextEdit::singleline(&mut i.address)
                    .desired_width(ui.available_width())
                    .hint_text(&address),
            );
            egui_pop_core::a11y::name(&edit, egui::WidgetType::TextEdit, &address);
            if edit.lost_focus() && ui.input(|inp| inp.key_pressed(egui::Key::Enter)) {
                actions.push(Action::Go(i.address.clone()));
            } else if !edit.has_focus()
                && let Some(t) = i.current.as_ref().map(|e| e.target.clone())
            {
                // Shows the real target while not edited.
                i.address = t;
            }
        });
    });
}

fn find_bar(ui: &mut Ui, v: &Viewer, actions: &mut Vec<Action>) {
    let ctx = ui.ctx().clone();
    let mut i = lock(&v.inner);
    if !i.find.open {
        return;
    }
    ui.horizontal(|ui| {
        let name = tr(&ctx, &txt::VIEWER_FIND);
        let mut query = i.find.query.clone();
        let edit = ui.add(
            egui::TextEdit::singleline(&mut query)
                .desired_width(180.0)
                .hint_text(&name),
        );
        egui_pop_core::a11y::name(&edit, egui::WidgetType::TextEdit, &name);
        if std::mem::take(&mut i.find.focus) {
            edit.request_focus();
        }
        if edit.changed() {
            i.find.query = query.clone();
            actions.push(Action::Find(query));
        }
        if edit.lost_focus() && ui.input(|inp| inp.key_pressed(egui::Key::Enter)) {
            let back = ui.input(|inp| inp.modifiers.shift);
            actions.push(Action::FindStep(!back));
            i.find.focus = true;
        }
        if edit.has_focus() && ui.input(|inp| inp.key_pressed(egui::Key::Escape)) {
            actions.push(Action::FindClose);
        }
        let n = i.find.matches.len();
        if i.find.query.is_empty() {
        } else if n == 0 {
            ui.weak(tr(&ctx, &txt::VIEWER_FIND_NONE));
        } else {
            ui.weak(tr_args(
                &ctx,
                &txt::VIEWER_FIND_COUNT,
                &[
                    ("at", &(i.find.at + 1).to_string()),
                    ("count", &n.to_string()),
                ],
            ));
        }
        if icon_button(
            ui,
            Icon::ChevronUp,
            &tr(&ctx, &txt::VIEWER_FIND_PREVIOUS),
            n > 0,
        )
        .clicked()
        {
            actions.push(Action::FindStep(false));
        }
        if icon_button(
            ui,
            Icon::ChevronDown,
            &tr(&ctx, &txt::VIEWER_FIND_NEXT),
            n > 0,
        )
        .clicked()
        {
            actions.push(Action::FindStep(true));
        }
        if icon_button(ui, Icon::Close, &tr(&ctx, &txt::VIEWER_FIND_CLOSE), true).clicked() {
            actions.push(Action::FindClose);
        }
    });
}

/// One bar per host whose images wait for the user, when the viewer can
/// fetch them at all.
fn images_bar(ui: &mut Ui, v: &Viewer, actions: &mut Vec<Action>) {
    if v.fetch.is_none() {
        return;
    }
    let hosts: std::collections::BTreeSet<String> = lock(&v.inner)
        .images
        .values()
        .filter_map(|s| match s {
            Image::Blocked(h) => Some(h.clone()),
            _ => None,
        })
        .collect();
    let ctx = ui.ctx().clone();
    for host in hosts {
        ui.horizontal_wrapped(|ui| {
            ui.add(Icon::Info.size(Size::Inline).tone(Tone::Info));
            ui.label(tr_args(
                &ctx,
                &txt::VIEWER_IMAGES_BLOCKED,
                &[("host", &host)],
            ));
            if ui
                .button(tr_args(&ctx, &txt::VIEWER_IMAGES_LOAD, &[("host", &host)]))
                .clicked()
            {
                actions.push(Action::LoadImages(host.clone()));
            }
        });
    }
}

/// What a page does not show, said out loud.
fn simplified_bar(ui: &mut Ui, v: &Viewer) {
    let Status::Shown(page) = v.status() else {
        return;
    };
    let dropped = &page.dropped;
    if dropped.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.add(Icon::Info.size(Size::Inline).tone(Tone::Info));
        ui.label(tr_args(
            ui.ctx(),
            &txt::VIEWER_SIMPLIFIED,
            &[("what", &dropped.join(", "))],
        ));
    });
}

fn body(ui: &mut Ui, v: &Viewer, actions: &mut Vec<Action>) {
    let ctx = ui.ctx().clone();
    let status = v.status();
    match status {
        Status::Empty => {
            ui.weak(tr(&ctx, &txt::VIEWER_EMPTY));
        }
        Status::Loading(target) => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak(tr_args(&ctx, &txt::VIEWER_LOADING, &[("target", &target)]));
            });
        }
        Status::Failed {
            target,
            failure,
            detail,
        } => {
            let t = Tokens::of(&ctx);
            ui.add_space(t.space.m);
            ui.horizontal(|ui| {
                ui.add(Icon::Warning.size(Size::Inline).tone(Tone::Warning));
                ui.strong(tr(&ctx, failure.title()));
            });
            ui.weak(&detail);
            ui.label(RichText::new(&target).monospace());
            ui.add_space(t.space.s);
            ui.horizontal(|ui| {
                if ui.button(tr(&ctx, &RETRY)).clicked() {
                    actions.push(Action::Reload);
                }
                if ui.button(tr(&ctx, &api_txt::LINKS_OPEN_OUTSIDE)).clicked() {
                    actions.push(Action::OpenOutside);
                }
                if ui.button(tr(&ctx, &api_txt::LINKS_COPY)).clicked() {
                    actions.push(Action::Copy);
                }
            });
        }
        Status::Shown(page) => {
            let (zoom, restore) = {
                let mut i = lock(&v.inner);
                (i.zoom, i.restore.take())
            };
            let mut area = egui::ScrollArea::both()
                .id_salt("egui_pop.viewer.page")
                .auto_shrink([false, false]);
            if let Some(y) = restore {
                area = area.vertical_scroll_offset(y);
            }
            let out = area.show(ui, |ui| {
                let name = tr(&ctx, &txt::VIEWER_PAGE);
                ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
                    node.set_role(egui::accesskit::Role::Document);
                    node.set_label(name);
                });
                for (_, font) in ui.style_mut().text_styles.iter_mut() {
                    font.size *= zoom;
                }
                match &page.content {
                    Content::Markdown(md) => markdown(ui, v, &page.target, md, actions),
                    Content::Image(uri) => {
                        ui.add(egui::Image::from_uri(uri).fit_to_original_size(zoom));
                    }
                }
            });
            lock(&v.inner).scroll = out.state.offset.y;
        }
    }
}

/// The page's blocks, each drawn alone: the one holding the current
/// match marked, the one asked for brought into view, the links clicked
/// collected.
fn markdown(ui: &mut Ui, v: &Viewer, base: &str, md: &Markdown, actions: &mut Vec<Action>) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let (current, scroll_to, images) = {
        let mut i = lock(&v.inner);
        let current = i
            .find
            .matches
            .get(i.find.at)
            .copied()
            .filter(|_| i.find.open);
        (current, i.scroll_to.take(), i.images.clone())
    };
    let width = ui.available_width();
    let before = ctx.viewport(|vp| vp.this_pass.widgets.get_layer(ui.layer_id()).count());
    let mut cache = lock(&v.cache);
    for (n, block) in md.blocks.iter().enumerate() {
        let source = block_source(&ctx, base, block, &md.references, &images);
        let draw = |ui: &mut Ui, cache: &mut egui_commonmark::CommonMarkCache| {
            ui.push_id(n, |ui| {
                CommonMarkViewer::new()
                    .explicit_image_uri_scheme(true)
                    .enable_scroll_to_heading(true)
                    .max_image_width(Some(width.max(1.0) as usize))
                    .show(ui, cache, &source);
            })
            .response
        };
        let rect = if current == Some(n) {
            egui::Frame::new()
                .fill(t.selection.gamma_multiply(0.35))
                .corner_radius(t.radius.widget)
                .show(ui, |ui| draw(ui, &mut cache))
                .response
                .rect
        } else {
            draw(ui, &mut cache).rect
        };
        if scroll_to == Some(n) {
            ui.scroll_to_rect(rect, Some(Align::TOP));
        }
        for dest in &block.links {
            if cache.get_link_hook(dest) == Some(true) {
                let outside = ui.input(|i| {
                    i.modifiers.command || i.pointer.button_released(egui::PointerButton::Middle)
                });
                actions.push(Action::Follow {
                    dest: dest.clone(),
                    outside,
                });
            }
        }
    }
    // upstream: egui_commonmark draws a code block as a read-only `TextEdit`
    // without a name and keeps its id to itself (see docs/upstream.md).
    name_code_blocks(ui, before);
}

/// Names the unnamed multi-line fields among the widgets drawn since `before`.
fn name_code_blocks(ui: &Ui, before: usize) {
    use egui::accesskit::Role;
    let ctx = ui.ctx();
    let drawn: Vec<egui::Id> = ctx.viewport(|vp| {
        let widgets = vp.this_pass.widgets.get_layer(ui.layer_id());
        widgets.skip(before).map(|w| w.id).collect()
    });
    let name = tr(ctx, &txt::VIEWER_CODE_BLOCK);
    for id in drawn {
        ctx.accesskit_node_builder(id, |node| {
            if node.role() == Role::MultilineTextInput && node.label().is_none() {
                node.set_label(name.as_str());
            }
        });
    }
}

/// A block's markdown as drawn: each image either at the address its
/// bytes were given under, or its alt text when it is not shown; the
/// page's reference definitions after it.
fn block_source(
    ctx: &egui::Context,
    base: &str,
    block: &super::Block,
    references: &str,
    images: &std::collections::BTreeMap<String, Image>,
) -> String {
    let mut out = String::with_capacity(block.source.len() + references.len());
    let mut at = 0;
    for img in &block.images {
        out.push_str(&block.source[at..img.range.start]);
        let uri = resolve(base, &img.dest);
        if images.get(&uri) == Some(&Image::Ready) {
            out.push_str(&format!("![{}](<{uri}>)", escape(&img.alt)));
        } else {
            let alt = if img.alt.is_empty() {
                img.dest.clone()
            } else {
                img.alt.clone()
            };
            let text = strings::tr_args(ctx, &txt::VIEWER_IMAGE_MISSING, &[("alt", &alt)]);
            out.push_str(&format!("*{}*", escape(&text)));
        }
        at = img.range.end;
    }
    out.push_str(&block.source[at..]);
    if !references.is_empty() && block.source.contains('[') {
        out.push_str("\n\n");
        out.push_str(references);
    }
    out
}
