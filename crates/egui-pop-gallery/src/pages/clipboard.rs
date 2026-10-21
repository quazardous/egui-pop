//! The clipboard page: two palettes copying a typed colour between them
//! (Ctrl+C, Ctrl+X, Ctrl+V, through the Edit menu too), a grey palette
//! taking a colour only through a lossy transform, paste special's ways
//! with their previews, and a text field keeping egui's own copy and
//! paste.

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Color32, CornerRadius, Sense, Stroke, Ui, vec2};
use egui_pop::api::clipboard::{
    self, ClipItem, ClipSource, ClipTarget, ClipType, Cut, FormatId, Paste, Repr, Slot, TEXT,
    Transform, TransformSpec, Transformed,
};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::model::commands::Origin;
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::theme::Tokens;
use serde_json::{Value as Json, json};

use super::section;
use crate::shell::SHELL;

pub const CLIPBOARD_PAGE: PluginId = PluginId::from_static("gallery.clipboard");

/// The gallery's colour: a private type of its own.
pub const COLOR: FormatId = FormatId::from_static("application/x-egui-pop-gallery.color+json");
/// A grey level, which only a lossy transform makes from a colour.
pub const GRAY: FormatId = FormatId::from_static("application/x-egui-pop-gallery.gray+json");

/// The endpoints, by name.
pub const PALETTE_A: &str = "gallery.clipboard.a";
pub const PALETTE_B: &str = "gallery.clipboard.b";
pub const GREYS: &str = "gallery.clipboard.greys";

/// A palette: colours as `#rrggbb`, the last one selected.
struct Palette {
    colors: RefCell<Vec<String>>,
    accepts: Vec<FormatId>,
    copies: bool,
}

impl Palette {
    fn new(colors: &[&str], accepts: Vec<FormatId>, copies: bool) -> Rc<Self> {
        Rc::new(Self {
            colors: RefCell::new(colors.iter().map(|c| c.to_string()).collect()),
            accepts,
            copies,
        })
    }
}

impl ClipSource for Palette {
    fn copy(&self, _: &AppHandle) -> Option<ClipItem> {
        let hex = self.colors.borrow().last().cloned()?;
        Some(ClipItem::new(COLOR, 1, json!({ "hex": hex }), "a colour").fallback(TEXT, json!(hex)))
    }

    fn can_copy(&self, _: &AppHandle) -> bool {
        self.copies && !self.colors.borrow().is_empty()
    }

    fn can_cut(&self, _: &AppHandle) -> bool {
        self.copies
    }

    fn delete(&self, _: &AppHandle, _: &Cut<'_>) -> Result<(), String> {
        let mut colors = self.colors.borrow_mut();
        if colors.len() < 2 {
            return Err("a palette keeps one colour at least".into());
        }
        colors.pop();
        Ok(())
    }
}

impl ClipTarget for Palette {
    fn accepts(&self, _: &AppHandle) -> Vec<FormatId> {
        self.accepts.clone()
    }

    fn paste(&self, _: &AppHandle, p: &Paste<'_>) -> Result<Json, String> {
        let hex = p.repr.data["hex"]
            .as_str()
            .or(p.repr.text())
            .ok_or("no colour")?
            .to_string();
        let n = self.colors.borrow().len();
        if !p.dry_run {
            self.colors.borrow_mut().push(hex.clone());
        }
        Ok(json!(format!("adds {hex} as colour {}", n + 1)))
    }
}

fn parse_hex(text: &str) -> Option<Color32> {
    let t = text.trim().strip_prefix('#')?;
    if t.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(t, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// The page, its palettes, its colour type and transforms.
#[derive(Debug, Default)]
pub struct ClipboardPage;

impl Plugin for ClipboardPage {
    fn id(&self) -> PluginId {
        CLIPBOARD_PAGE
    }

    fn title(&self) -> String {
        "Clipboard".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, clipboard::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let a = Palette::new(&["#d9a441", "#3a7bd5"], vec![COLOR, TEXT], true);
        let b = Palette::new(&["#2e8b57"], vec![COLOR, TEXT], true);
        let greys = Palette::new(&["#808080"], vec![GRAY], false);
        ctx.clip_endpoint(PALETTE_A, Some(a.clone()), Some(a.clone()))
            .clip_endpoint(PALETTE_B, Some(b.clone()), Some(b.clone()))
            .clip_endpoint(GREYS, None, Some(greys.clone()));
        ctx.clip_type(
            ClipType::new(COLOR, 1, "a colour")
                .fallbacks(|p| vec![Repr::new(TEXT, p["hex"].clone())]),
        )?
        .clip_transform(Transform::new(
            TransformSpec::new("gallery.parse-colour", TEXT, COLOR),
            |r| {
                let text = r.text().unwrap_or_default().trim();
                parse_hex(text)
                    .map(|_| Transformed::exact(json!({ "hex": text })))
                    .ok_or_else(|| format!("`{text}` is not a colour"))
            },
        ))?
        .clip_transform(Transform::new(
            TransformSpec::new("gallery.to-grey", COLOR, GRAY).lossy(),
            |r| {
                let c = r.data["hex"]
                    .as_str()
                    .and_then(parse_hex)
                    .ok_or("not a colour")?;
                let l = (0.2126 * f32::from(c.r())
                    + 0.7152 * f32::from(c.g())
                    + 0.0722 * f32::from(c.b()))
                .round() as u8;
                Ok(Transformed::lossy(
                    json!({ "hex": format!("#{l:02x}{l:02x}{l:02x}") }),
                    "the hue",
                ))
            },
        ))?;
        ctx.story(
            Story::new(StoryId::from_static("clipboard"), "Clipboard", move |ui, app| {
                show(ui, app, &a, &b, &greys);
            })
            .description("Typed items copied between widgets: Ctrl+C, Ctrl+X and Ctrl+V, the Edit menu, paste special's previews, a lossy transform, and text fields keeping their own.")
            .group("Framework", 104),
        )?;
        Ok(())
    }
}

/// A palette drawn as a row of swatches, focusable, bound to its
/// endpoint.
fn palette(ui: &mut Ui, app: &AppHandle, name: &str, label: &str, p: &Palette) {
    let t = Tokens::of(ui.ctx());
    let h = ui.spacing().interact_size.y;
    let colors = p.colors.borrow().clone();
    let w = (colors.len().max(4) as f32) * (h + t.space.xs) + t.space.s * 2.0;
    let (rect, response) = ui.allocate_exact_size(vec2(w, h + t.space.s * 2.0), Sense::click());
    if response.clicked() {
        response.request_focus();
    }
    let stroke = if response.has_focus() {
        Stroke::new(2.0, t.accent.default)
    } else {
        Stroke::new(1.0, t.border.default)
    };
    let radius = CornerRadius::same(t.radius.widget.round() as u8);
    ui.painter().rect(
        rect,
        radius,
        t.surface.sunken,
        stroke,
        egui::StrokeKind::Inside,
    );
    for (i, hex) in colors.iter().enumerate() {
        let fill = parse_hex(hex).unwrap_or(Color32::TRANSPARENT);
        let min = rect.min + vec2(t.space.s + i as f32 * (h + t.space.xs), t.space.s);
        let r = egui::Rect::from_min_size(min, vec2(h, h));
        let edge = if i + 1 == colors.len() {
            Stroke::new(2.0, t.text.default)
        } else {
            Stroke::new(1.0, t.border.default)
        };
        ui.painter()
            .rect(r, radius, fill, edge, egui::StrokeKind::Inside);
    }
    let value = colors.join(", ");
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label);
        info.current_text_value = Some(value.clone());
        info
    });
    clipboard::bind(app, name, response.id);
}

fn show(ui: &mut Ui, app: &AppHandle, a: &Palette, b: &Palette, greys: &Palette) {
    ui.label("Click a palette, then Ctrl+C, Ctrl+X or Ctrl+V (or the Edit menu): a colour goes as a typed item, and as #rrggbb text to other applications. Ctrl+Shift+V chooses how to paste, each way with its preview.");
    section(ui, "Two palettes and a grey one");
    ui.horizontal_wrapped(|ui| {
        palette(ui, app, PALETTE_A, "Palette A", a);
        palette(ui, app, PALETTE_B, "Palette B", b);
        palette(ui, app, GREYS, "Greys", greys);
    });
    ui.weak("The selected colour is the last one; Greys takes a colour only through a lossy transform, and says what it loses.");
    section(ui, "On the clipboard");
    match clipboard::with_owner(app, |c| (c.get(&Slot::User), c.rev())) {
        Some((Some(item), rev)) => {
            let offers: Vec<String> = item.offers().iter().map(ToString::to_string).collect();
            ui.label(format!(
                "{} (revision {rev}): {}",
                item.title,
                offers.join(", ")
            ));
            if !item.stripped.is_empty() {
                ui.weak(format!("{} left out on the way out", item.stripped.len()));
            }
        }
        Some((None, _)) => {
            ui.weak("Nothing copied yet.");
        }
        None => {
            ui.weak("The clipboard plugin is not connected.");
        }
    }
    section(ui, "Paste special into Palette B");
    match clipboard::ways(app, PALETTE_B, None, None, &Origin::User) {
        Ok(ways) => {
            for w in ways {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(&w.label);
                    if let Some(p) = &w.preview {
                        ui.weak(p);
                    }
                });
            }
        }
        Err(e) => {
            ui.weak(e);
        }
    }
    section(ui, "A text field keeps its own copy and paste");
    let id = egui::Id::new("gallery.clipboard.note");
    let mut note = ui
        .data(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| "Select me and copy".into());
    let label = ui.label("Note");
    ui.add(egui::TextEdit::singleline(&mut note))
        .labelled_by(label.id);
    ui.data_mut(|d| d.insert_temp(id, note));
}
