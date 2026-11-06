//! What each panel draws: the blueprint, the selection, the time panel,
//! the top bar and the four views. The time series is egui-pop's plot;
//! plain egui painting over egui-pop's tokens where egui-pop has no
//! component (the time panel).

use egui::{
    Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, RichText, Role, Sense, Stroke,
    StrokeKind, Ui, WidgetInfo, pos2, vec2,
};
use egui_pop::api::AppHandle;
use egui_pop::core::icon::{Icon, IconWidget, Size};
use egui_pop::core::sections::{Section, Sections};
use egui_pop::core::ui::UiPop as _;
use egui_pop::prelude::*;
use egui_pop::views::Mode;
use egui_pop::views::plot::{Plot, Series};
use egui_pop::views::table::{Column, DataTable, TableRows};
use egui_pop::views::tree::{FilterField, Node, Sink, TreeSource, TreeView};

use super::data::{self, ENTITIES, FRAMES, Kind, Level, View};
use super::{Emphasis, Item, Model, entity_key, item_of_key};

/// Names a painted, interactive area for assistive technology.
fn named(response: &Response, name: &str) {
    let name = name.to_string();
    response.widget_info(move || WidgetInfo::labeled(Role::Group, true, &name));
}

/// The colour an entity is drawn in, with its emphasis.
fn stroke_for(color: Color32, emphasis: Emphasis) -> Stroke {
    match emphasis {
        Emphasis::Selected => Stroke::new(2.5, color),
        Emphasis::Hovered => Stroke::new(2.0, color),
        Emphasis::None => Stroke::new(1.25, color),
    }
}

/// The outline Rerun draws around what is selected or hovered.
fn outline(t: &Tokens, emphasis: Emphasis) -> Option<Stroke> {
    match emphasis {
        Emphasis::Selected => Some(Stroke::new(5.0, t.selection)),
        Emphasis::Hovered => Some(Stroke::new(4.5, t.text.strong.gamma_multiply(0.35))),
        Emphasis::None => None,
    }
}

// ------------------------------------------------------------- blueprint

/// The blueprint's tree: the views, each with its entities.
struct Blueprint;

impl TreeSource<String> for Blueprint {
    fn load(&self, parent: Option<&String>, sink: Sink<String>) {
        let nodes = match parent.and_then(|k| View::from_key(k)) {
            None => View::ALL
                .iter()
                .map(|v| Node::branch(v.key(), v.title()))
                .collect(),
            Some(view) => view
                .entities()
                .into_iter()
                .map(|e| Node::leaf(entity_key(view, e), ENTITIES[e].path))
                .collect(),
        };
        sink.deliver(nodes);
    }
}

pub fn blueprint(ui: &mut Ui, app: &AppHandle, m: &mut Model) {
    let t = Tokens::of(ui.ctx());
    let Model {
        tree,
        hidden,
        hovered,
        hover_now,
        ..
    } = m;
    ui.add_space(t.space.s);
    FilterField::new("rerun.blueprint").show(ui, tree);
    ui.add_space(t.space.xs);
    let hovered = *hovered;
    TreeView::new("rerun.blueprint")
        .label("Blueprint")
        .app(app)
        .icon(|node: &Node<String>| match item_of_key(&node.key) {
            Some(Item::View(_)) => Some(Icon::Layout.into()),
            Some(Item::Entity(e)) if ENTITIES[e].kind == Kind::Text => Some(Icon::File.into()),
            Some(Item::Entity(_)) => Some(Icon::Dot.into()),
            None => None,
        })
        .row_widgets(|ui, node| {
            let Some(Item::Entity(e)) = item_of_key(&node.key) else {
                return;
            };
            let row = ui.max_rect();
            let over = ui.rect_contains_pointer(row);
            if over {
                *hover_now = Some(e);
            } else if hovered == Some(e) {
                // Hovered in a view: the row shows it too.
                ui.painter().rect_filled(row, 0.0, t.surface.raised);
            }
            let shown = !hidden.contains(&e);
            if over || !shown {
                let (icon, name) = if shown {
                    (Icon::Visible, "Hide")
                } else {
                    (Icon::Hidden, "Show")
                };
                let r = ui.add(
                    IconWidget::new(icon)
                        .size(Size::Inline)
                        .label(&format!("{name} {}", ENTITIES[e].path))
                        .sense_click(),
                );
                if r.clicked() {
                    if shown {
                        hidden.insert(e);
                    } else {
                        hidden.remove(&e);
                    }
                }
            }
        })
        .show(ui, tree, &Blueprint);
}

// ------------------------------------------------------------- selection

/// A property row: a muted name, a value.
fn property(ui: &mut Ui, t: &Tokens, name: &str, value: impl Into<RichText>) {
    ui.label(RichText::new(name).color(t.text.muted));
    ui.label(value.into());
    ui.end_row();
}

fn properties(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    let t = Tokens::of(ui.ctx());
    egui::Grid::new(id)
        .num_columns(2)
        .spacing(vec2(t.space.l, t.space.s))
        .min_col_width(84.0)
        .show(ui, add);
}

fn swatch(ui: &mut Ui, color: Color32, text: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
        ui.painter().rect_filled(r, 2.0, color);
        ui.label(text);
    });
}

pub fn selection(ui: &mut Ui, m: &mut Model) {
    let t = Tokens::of(ui.ctx());
    let at = m.at();
    match m.selected {
        None => {
            ui.add_space(t.space.l);
            ui.horizontal(|ui| {
                ui.add_space(t.space.l);
                ui.vertical(|ui| {
                    ui.muted("Nothing selected");
                    ui.meta(
                        "Select an entity or a view in the blueprint, a view or the time panel.",
                    );
                });
            });
        }
        Some(Item::Entity(e)) => {
            let ent = &ENTITIES[e];
            header(ui, &t, ent.path, ent.kind.archetype());
            let shown = !m.hidden.contains(&e);
            Sections::new("rerun.selection.entity")
                .capitals(false)
                .show(ui, |stack| {
                    stack.section(Section::new("entity", "Entity"), |ui| {
                        properties(ui, "rerun.entity", |ui| {
                            property(ui, &t, "Path", RichText::new(ent.path).monospace());
                            property(ui, &t, "Archetype", ent.kind.archetype());
                            let views: Vec<&str> = ent.views.iter().map(|v| v.title()).collect();
                            property(ui, &t, "Views", views.join(", "));
                            property(ui, &t, "Visible", if shown { "yes" } else { "no" });
                            property(ui, &t, "Logged", format!("every {} frames", ent.every));
                        });
                    });
                    stack.section(
                        Section::new("latest", format!("Latest at frame {at}")),
                        |ui| {
                            properties(ui, "rerun.latest", |ui| match ent.kind {
                                Kind::Scalar => {
                                    property(
                                        ui,
                                        &t,
                                        "Scalar",
                                        RichText::new(format!(
                                            "{:.2} {}",
                                            data::scalar(e, at),
                                            ent.unit
                                        ))
                                        .monospace(),
                                    );
                                    property(
                                        ui,
                                        &t,
                                        "Frame",
                                        format!("{}", at / ent.every * ent.every),
                                    );
                                }
                                Kind::Image => {
                                    property(ui, &t, "Resolution", "256 x 160");
                                    property(ui, &t, "Format", "RGB8");
                                }
                                Kind::Boxes => {
                                    for (label, b) in data::detections(at) {
                                        property(
                                            ui,
                                            &t,
                                            label,
                                            RichText::new(format!("{:.2}, {:.2}", b[0], b[1]))
                                                .monospace(),
                                        );
                                    }
                                }
                                Kind::Text => {
                                    let last =
                                        m.log.iter().rev().find(|l| l.entity == e && l.frame <= at);
                                    match last {
                                        Some(l) => {
                                            property(ui, &t, "Level", l.level.name());
                                            property(ui, &t, "Text", l.text.as_str());
                                        }
                                        None => property(ui, &t, "Text", "none yet"),
                                    }
                                }
                            });
                        },
                    );
                    stack.section(Section::new("visualizer", "Visualizer"), |ui| {
                        properties(ui, "rerun.visualizer", |ui| {
                            ui.label(RichText::new("Color").color(t.text.muted));
                            swatch(
                                ui,
                                ent.color,
                                &format!(
                                    "#{:02x}{:02x}{:02x}",
                                    ent.color.r(),
                                    ent.color.g(),
                                    ent.color.b()
                                ),
                            );
                            ui.end_row();
                            property(
                                ui,
                                &t,
                                "Width",
                                if ent.kind == Kind::Scalar {
                                    "1.25 pt"
                                } else {
                                    "default"
                                },
                            );
                        });
                    });
                });
        }
        Some(Item::View(view)) => {
            header(ui, &t, view.title(), view.kind());
            let mut pick = None;
            Sections::new("rerun.selection.view")
                .capitals(false)
                .show(ui, |stack| {
                    stack.section(Section::new("view", "View"), |ui| {
                        properties(ui, "rerun.view", |ui| {
                            property(ui, &t, "Name", view.title());
                            property(ui, &t, "Class", view.kind());
                            property(ui, &t, "Entities", format!("{}", view.entities().len()));
                        });
                    });
                    stack.section(
                        Section::new("entities", "Entities").count(view.entities().len() as u64),
                        |ui| {
                            for e in view.entities() {
                                if ui.selectable_label(false, ENTITIES[e].path).clicked() {
                                    pick = Some(Item::Entity(e));
                                }
                            }
                        },
                    );
                });
            if pick.is_some() {
                m.select(pick);
            }
        }
    }
}

fn header(ui: &mut Ui, t: &Tokens, title: &str, kind: &str) {
    ui.add_space(t.space.m);
    ui.horizontal(|ui| {
        ui.add_space(t.space.l);
        ui.vertical(|ui| {
            ui.label(
                RichText::new(title)
                    .size(t.font.scale.l)
                    .color(t.text.strong)
                    .strong(),
            );
            ui.meta(kind);
        });
    });
    ui.add_space(t.space.m);
}

// ------------------------------------------------------------ transport

/// The transport buttons egui-pop has no icons for.
#[derive(Clone, Copy)]
enum Transport {
    Play,
    Pause,
    Back,
    Forward,
    Loop(bool),
}

fn transport(ui: &mut Ui, kind: Transport, name: &str) -> Response {
    let t = Tokens::of(ui.ctx());
    let side = t.size.control_height;
    let (rect, r) = ui.allocate_exact_size(vec2(side, side), Sense::click());
    named(&r, name);
    let r = r.on_hover_text(name);
    let p = ui.painter();
    if r.hovered() {
        p.rect_filled(rect, t.radius.widget, t.surface.hover);
    }
    let c = if r.hovered() {
        t.text.strong
    } else {
        t.text.default
    };
    let g = Rect::from_center_size(rect.center(), vec2(side * 0.4, side * 0.4));
    let tri = |p: &egui::Painter, r: Rect, right: bool| {
        let pts = if right {
            vec![r.left_top(), pos2(r.right(), r.center().y), r.left_bottom()]
        } else {
            vec![
                r.right_top(),
                pos2(r.left(), r.center().y),
                r.right_bottom(),
            ]
        };
        p.add(egui::Shape::convex_polygon(pts, c, Stroke::NONE));
    };
    match kind {
        Transport::Play => tri(p, g, true),
        Transport::Pause => {
            let w = g.width() * 0.32;
            p.rect_filled(Rect::from_min_size(g.min, vec2(w, g.height())), 1.0, c);
            p.rect_filled(
                Rect::from_min_size(pos2(g.right() - w, g.top()), vec2(w, g.height())),
                1.0,
                c,
            );
        }
        Transport::Back | Transport::Forward => {
            let right = matches!(kind, Transport::Forward);
            let bar = g.width() * 0.2;
            let (bar_x, tri_r) = if right {
                (
                    g.right() - bar,
                    Rect::from_min_max(g.min, pos2(g.right() - bar, g.bottom())),
                )
            } else {
                (
                    g.left(),
                    Rect::from_min_max(pos2(g.left() + bar, g.top()), g.max),
                )
            };
            p.rect_filled(
                Rect::from_min_size(pos2(bar_x, g.top()), vec2(bar, g.height())),
                0.0,
                c,
            );
            tri(p, tri_r.shrink2(vec2(1.0, 0.0)), right);
        }
        Transport::Loop(on) => {
            let c = if on { t.accent.default } else { c };
            Icon::Reset.paint(p, g.expand(2.0), c);
        }
    }
    r
}

fn play_controls(ui: &mut Ui, app: &AppHandle, m: &Model) {
    let (kind, name) = if m.playing {
        (Transport::Pause, "Pause")
    } else {
        (Transport::Play, "Play")
    };
    if transport(ui, kind, name).clicked() {
        app.run(super::CMD_PLAY, serde_json::Value::Null);
    }
}

fn readout(ui: &mut Ui, m: &Model) {
    let t = Tokens::of(ui.ctx());
    let at = m.at();
    ui.label(
        RichText::new(format!("frame {at:>3}"))
            .monospace()
            .color(t.text.strong),
    );
    ui.label(
        RichText::new(format!("{:.2} s", at as f32 / data::FPS))
            .monospace()
            .color(t.text.muted),
    );
}

pub fn top_bar(ui: &mut Ui, app: &AppHandle, m: &mut Model) {
    let t = Tokens::of(ui.ctx());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = t.space.s;
        ui.label(RichText::new("demo_recording").color(t.text.muted));
        ui.separator();
        play_controls(ui, app, m);
        readout(ui, m);
        ui.separator();
    });
}

// ------------------------------------------------------------ time panel

const NAMES_WIDTH: f32 = 170.0;
const RULER: f32 = 20.0;

pub fn time_panel(ui: &mut Ui, app: &AppHandle, m: &mut Model) {
    let t = Tokens::of(ui.ctx());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = t.space.s;
        ui.add_space(t.space.s);
        play_controls(ui, app, m);
        if transport(ui, Transport::Back, "Step back one frame").clicked() {
            app.run(super::CMD_STEP_BACK, serde_json::Value::Null);
        }
        if transport(ui, Transport::Forward, "Step forward one frame").clicked() {
            app.run(super::CMD_STEP_FORWARD, serde_json::Value::Null);
        }
        if transport(ui, Transport::Loop(m.looping), "Toggle looping").clicked() {
            app.run(super::CMD_LOOP, serde_json::Value::Null);
        }
        ui.separator();
        ui.label(RichText::new("timeline").color(t.text.muted));
        ui.label(RichText::new("frame").monospace());
        ui.separator();
        readout(ui, m);
    });
    ui.add_space(t.space.xs);
    let row_h = t.size.control_height - 4.0;
    let rows = ENTITIES.len();
    let height = (RULER + row_h * rows as f32).max(ui.available_height());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let names = Rect::from_min_max(rect.min, pos2(rect.left() + NAMES_WIDTH, rect.bottom()));
    let lane = Rect::from_min_max(
        pos2(names.right() + t.space.m, rect.top()),
        pos2(rect.right() - t.space.l, rect.bottom()),
    );
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, t.surface.canvas);
    p.line_segment(
        [
            pos2(names.right(), rect.top()),
            pos2(names.right(), rect.bottom()),
        ],
        Stroke::new(1.0, t.border.default),
    );
    let x_of = |f: f32| lane.left() + lane.width() * f / FRAMES as f32;
    // The ruler.
    let ruler = Rect::from_min_max(
        pos2(lane.left(), rect.top()),
        pos2(lane.right(), rect.top() + RULER),
    );
    let scrub = ui.interact(
        ruler.expand2(vec2(4.0, 0.0)),
        ui.scope_id().with("rerun.ruler"),
        Sense::click_and_drag(),
    );
    named(&scrub, "Time ruler");
    egui_pop::core::pointer::drag_cursor(&scrub, egui_pop::core::pointer::DragKind::Value);
    for f in (0..=FRAMES).step_by(10) {
        let x = x_of(f as f32);
        let major = f % 50 == 0;
        let h = if major { 8.0 } else { 4.0 };
        p.line_segment(
            [pos2(x, ruler.bottom() - h), pos2(x, ruler.bottom())],
            Stroke::new(1.0, t.text.muted),
        );
        if major {
            p.text(
                pos2(x + 3.0, ruler.top() + 2.0),
                Align2::LEFT_TOP,
                f.to_string(),
                FontId::monospace(t.font.scale.xs),
                t.text.muted,
            );
        }
    }
    p.line_segment(
        [ruler.left_bottom(), ruler.right_bottom()],
        Stroke::new(1.0, t.border.default),
    );
    // A row per entity: its name, then a tick where it has data.
    for (e, ent) in ENTITIES.iter().enumerate() {
        let top = rect.top() + RULER + row_h * e as f32;
        let row = Rect::from_min_max(pos2(rect.left(), top), pos2(rect.right(), top + row_h));
        let r = ui.interact(row, ui.scope_id().with(("rerun.stream", e)), Sense::click());
        named(&r, ent.path);
        if r.hovered() {
            m.hover(e);
        }
        if r.clicked() {
            m.select(Some(Item::Entity(e)));
        }
        let emphasis = m.emphasis(e);
        match emphasis {
            Emphasis::Selected => {
                p.rect_filled(row, 0.0, t.accent.subtle);
            }
            Emphasis::Hovered => {
                p.rect_filled(row, 0.0, t.surface.hover);
            }
            Emphasis::None => {}
        }
        let text = if emphasis == Emphasis::Selected {
            t.text.strong
        } else {
            t.text.default
        };
        p.circle_filled(
            pos2(names.left() + t.space.l, row.center().y),
            3.0,
            ent.color,
        );
        p.text(
            pos2(names.left() + t.space.l + 10.0, row.center().y),
            Align2::LEFT_CENTER,
            ent.path,
            FontId::proportional(t.font.scale.m),
            text,
        );
        let tick = if emphasis == Emphasis::Selected {
            t.accent.default
        } else {
            t.text.muted.gamma_multiply(0.7)
        };
        for f in (0..=FRAMES).filter(|f| data::logged_at(e, *f)) {
            let x = x_of(f as f32);
            p.line_segment(
                [pos2(x, row.top() + 4.0), pos2(x, row.bottom() - 4.0)],
                Stroke::new(1.0, tick),
            );
        }
    }
    // The cursor.
    if let Some(pos) = scrub.interact_pointer_pos() {
        m.frame = ((pos.x - lane.left()) / lane.width() * FRAMES as f32)
            .clamp(0.0, FRAMES as f32)
            .round();
        m.playing = false;
    }
    let x = x_of(m.frame);
    p.line_segment(
        [pos2(x, ruler.top() + 4.0), pos2(x, rect.bottom())],
        Stroke::new(1.5, t.text.strong),
    );
    p.add(egui::Shape::convex_polygon(
        vec![
            pos2(x - 5.0, ruler.top() + 2.0),
            pos2(x + 5.0, ruler.top() + 2.0),
            pos2(x, ruler.top() + 9.0),
        ],
        t.text.strong,
        Stroke::NONE,
    ));
}

// ------------------------------------------------------------ the views

/// A view's canvas: its rect, its response and a painter clipped to it.
fn canvas(ui: &mut Ui, name: &str) -> (Rect, Response) {
    let t = Tokens::of(ui.ctx());
    let (rect, r) = ui.allocate_exact_size(ui.available_size(), Sense::click());
    named(&r, name);
    ui.painter().rect_filled(rect, 0.0, t.surface.canvas);
    (rect, r)
}

pub fn time_series(ui: &mut Ui, m: &mut Model) {
    let shown: Vec<usize> = View::TimeSeries
        .entities()
        .into_iter()
        .filter(|e| !m.hidden.contains(e))
        .collect();
    // x in frames, as the time panel counts them; each entity in its own
    // colour, the one hovered or selected elsewhere drawn thicker.
    let series: Vec<Series> = shown
        .iter()
        .map(|&e| {
            let ent = &ENTITIES[e];
            Series::line(
                ent.path,
                (0..=FRAMES)
                    .step_by(ent.every as usize)
                    .map(|f| [f64::from(f), f64::from(data::scalar(e, f))]),
            )
            .color(ent.color)
        })
        .collect();
    let emphasized = shown.iter().position(|&e| m.emphasis(e) != Emphasis::None);
    let out = Plot::new("rerun.time_series", &series)
        .label("Time series plot")
        .time_cursor(Some(f64::from(m.at())))
        .drag_cursor(true)
        .values_tooltip(true)
        .emphasize(emphasized)
        .legend(true)
        .show(ui);
    if let Some(t) = out.cursor_dragged {
        m.frame = t.round().clamp(0.0, f64::from(FRAMES)) as f32;
        m.playing = false;
    }
    let target = out.hovered.as_ref().map(|h| shown[h.series]);
    if let Some(e) = target {
        m.hover(e);
    }
    if out.response.clicked() {
        m.select(Some(
            target.map_or(Item::View(View::TimeSeries), Item::Entity),
        ));
    }
}

pub fn camera(ui: &mut Ui, m: &mut Model, texture: &egui::TextureHandle) {
    let t = Tokens::of(ui.ctx());
    let (rect, r) = canvas(ui, "Camera view");
    let p = ui.painter_at(rect);
    let inner = rect.shrink(t.space.m);
    let aspect = 256.0 / 160.0;
    let size = if inner.width() / inner.height() > aspect {
        vec2(inner.height() * aspect, inner.height())
    } else {
        vec2(inner.width(), inner.width() / aspect)
    };
    let img = Rect::from_center_size(inner.center(), size);
    let (image, boxes) = (3, 4);
    let mut target = None;
    let pointer = r.hover_pos();
    if !m.hidden.contains(&image) {
        p.image(
            texture.id(),
            img,
            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        if let Some(o) = outline(&t, m.emphasis(image)) {
            p.rect_stroke(img, 0.0, Stroke::new(2.0, o.color), StrokeKind::Outside);
        }
        if pointer.is_some_and(|pp| img.contains(pp)) {
            target = Some(image);
        }
    } else {
        p.rect_stroke(
            img,
            0.0,
            Stroke::new(1.0, t.border.default),
            StrokeKind::Inside,
        );
    }
    if !m.hidden.contains(&boxes) {
        let emphasis = m.emphasis(boxes);
        let color = ENTITIES[boxes].color;
        for (label, b) in data::detections(m.at()) {
            let bx = Rect::from_min_size(
                img.min + vec2(b[0] * img.width(), b[1] * img.height()),
                vec2(b[2] * img.width(), b[3] * img.height()),
            );
            if pointer.is_some_and(|pp| bx.contains(pp)) {
                target = Some(boxes);
            }
            if let Some(o) = outline(&t, emphasis) {
                p.rect_stroke(bx, 0.0, o, StrokeKind::Middle);
            }
            p.rect_stroke(bx, 0.0, stroke_for(color, emphasis), StrokeKind::Middle);
            let galley = p.layout_no_wrap(
                label.to_string(),
                FontId::proportional(t.font.scale.xs),
                Color32::BLACK,
            );
            let lr = Rect::from_min_size(
                bx.left_top() - vec2(0.0, galley.size().y + 2.0),
                galley.size() + vec2(6.0, 2.0),
            );
            p.rect_filled(lr, CornerRadius::same(2), color);
            p.galley(lr.min + vec2(3.0, 1.0), galley, Color32::BLACK);
        }
    }
    if let Some(e) = target {
        m.hover(e);
    }
    p.text(
        rect.left_bottom() + vec2(t.space.m, -t.space.s),
        Align2::LEFT_BOTTOM,
        "/camera",
        FontId::proportional(t.font.scale.xs),
        t.text.muted,
    );
    if r.clicked() {
        m.select(Some(target.map_or(Item::View(View::Camera), Item::Entity)));
    }
}

/// The dataframe's rows: one per frame, a column per scalar.
struct Frames<'a> {
    t: &'a Tokens,
    at: u32,
    selected: Option<Item>,
    hovered: Option<usize>,
    hover_now: &'a mut Option<usize>,
}

const SCALARS: [usize; 3] = [0, 1, 2];

impl TableRows<u32> for Frames<'_> {
    fn len(&self) -> usize {
        FRAMES as usize + 1
    }

    fn key(&self, row: usize) -> u32 {
        row as u32
    }

    fn label(&self, row: usize) -> String {
        format!("frame {row}")
    }

    fn cell(&mut self, ui: &mut Ui, row: usize, column: usize, _selected: bool) {
        let t = self.t;
        let frame = row as u32;
        let cell = ui.max_rect();
        if frame == self.at {
            ui.painter().rect_filled(cell, 0.0, t.surface.raised);
        }
        let mono = |s: String| RichText::new(s).monospace();
        match column {
            0 => {
                ui.label(mono(frame.to_string()).color(t.text.muted));
            }
            1 => {
                ui.label(mono(format!("{:.2}", frame as f32 / data::FPS)).color(t.text.muted));
            }
            c => {
                let e = SCALARS[c - 2];
                if self.selected == Some(Item::Entity(e)) {
                    ui.painter().rect_filled(cell, 0.0, t.accent.subtle);
                } else if self.hovered == Some(e) {
                    ui.painter().rect_filled(cell, 0.0, t.surface.hover);
                }
                if ui.rect_contains_pointer(cell) {
                    *self.hover_now = Some(e);
                }
                let text = if data::logged_at(e, frame) {
                    mono(format!("{:.2}", data::scalar(e, frame)))
                } else {
                    mono("-".into()).color(t.text.muted)
                };
                ui.label(text);
            }
        }
    }
}

pub fn dataframe(ui: &mut Ui, m: &mut Model) {
    let t = Tokens::of(ui.ctx());
    let mut columns = vec![
        Column::new("frame", "frame").width(56.0),
        Column::new("time", "time (s)").width(64.0),
    ];
    for e in SCALARS {
        columns.push(Column::new(ENTITIES[e].path, ENTITIES[e].path).width(150.0));
    }
    let at = m.at();
    let Model {
        table,
        selected,
        hovered,
        hover_now,
        ..
    } = m;
    let mut rows = Frames {
        t: &t,
        at,
        selected: *selected,
        hovered: *hovered,
        hover_now,
    };
    DataTable::new("rerun.dataframe", &columns)
        .mode(Mode::Single)
        .show(ui, table, &mut rows);
}

pub fn logs(ui: &mut Ui, m: &mut Model) {
    let t = Tokens::of(ui.ctx());
    let at = m.at();
    let lines: Vec<data::LogLine> = m
        .log
        .iter()
        .filter(|l| l.frame <= at && !m.hidden.contains(&l.entity))
        .cloned()
        .collect();
    let row_h = t.size.control_height - 2.0;
    let mono = FontId::monospace(t.font.scale.s);
    let body = FontId::proportional(t.font.scale.m);
    let cols = [8.0, 56.0, 112.0, 236.0];
    // The header.
    let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::hover());
    let p = ui.painter();
    p.rect_filled(head, 0.0, t.surface.raised);
    for (x, name) in cols.iter().zip(["frame", "level", "entity", "body"]) {
        p.text(
            pos2(head.left() + x, head.center().y),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(t.font.scale.s),
            t.text.muted,
        );
    }
    let mut clicked = None;
    let area = egui::ScrollArea::vertical()
        .id_salt("rerun.logs")
        .auto_shrink([false, false])
        .stick_to_bottom(true);
    ui.scroll_area_rows(area, row_h, lines.len(), |ui, range| {
        for line in &lines[range] {
            let (row, r) =
                ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
            named(
                &r,
                &format!("{} {} {}", line.frame, line.level.name(), line.text),
            );
            if r.hovered() {
                m.hover(line.entity);
            }
            if r.clicked() {
                clicked = Some(line.entity);
            }
            let p = ui.painter();
            match m.emphasis(line.entity) {
                Emphasis::Selected => {
                    p.rect_filled(row, 0.0, t.accent.subtle);
                }
                Emphasis::Hovered => {
                    p.rect_filled(row, 0.0, t.surface.hover);
                }
                Emphasis::None => {}
            }
            let level = match line.level {
                Level::Trace => t.palette.log.trace,
                Level::Debug => t.palette.log.debug,
                Level::Info => t.palette.log.info,
                Level::Warn => t.palette.log.warn,
                Level::Error => t.palette.log.error,
            };
            let y = row.center().y;
            p.text(
                pos2(row.left() + cols[0], y),
                Align2::LEFT_CENTER,
                line.frame.to_string(),
                mono.clone(),
                t.text.muted,
            );
            p.text(
                pos2(row.left() + cols[1], y),
                Align2::LEFT_CENTER,
                line.level.name(),
                mono.clone(),
                level,
            );
            p.text(
                pos2(row.left() + cols[2], y),
                Align2::LEFT_CENTER,
                ENTITIES[line.entity].path,
                body.clone(),
                t.text.muted,
            );
            p.text(
                pos2(row.left() + cols[3], y),
                Align2::LEFT_CENTER,
                &line.text,
                body.clone(),
                t.text.default,
            );
        }
    });
    if let Some(e) = clicked {
        m.select(Some(Item::Entity(e)));
    }
}
