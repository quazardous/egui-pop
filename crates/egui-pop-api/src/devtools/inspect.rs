//! Devtools' inspect mode: click an element (or reach it with Tab and the
//! arrows, then Enter) and a dialog says what egui and egui-pop know of
//! it, with a Copy button, so that a misbehaving element can be named
//! precisely in a report.
//!
//! The command `devtools.inspect` (Ctrl+Shift+F12; View menu, the
//! palette, the picker button at the menu bar's end, the devtools panel)
//! enters the mode, a second press or Escape leaves it. While it is on,
//! the menu bar's button is pressed ([`CONTEXT_INSPECTING`]) and the
//! pointer is a crosshair ([`CURSOR`]), but over the inspect dialog. Not Ctrl+Shift+C: browsers take it for their own
//! element picker before a web build sees it. It works in every window, a detached
//! panel's included. The picking itself is `egui_pop_core::inspect`'s;
//! this side adds the window, the zone, block or panel and the plugin
//! owning it, the theme, its kit and density, and what the application
//! adds through [`DevtoolsPlugin::describe`](super::DevtoolsPlugin::describe)
//! (the gallery: its page and story).
//!
//! Copy puts a readable text block on the clipboard (the plain-text
//! fallback) with its JSON form (`application/x-egui-pop.inspect+json`),
//! through the clipboard's `clipboard.copy` step when its plugin is
//! connected. Report… (when the application declares `help.report`, see
//! [`crate::about`]) opens a new issue prefilled with the pick. The last
//! pick is the `inspect` collector's snapshot.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use egui::Ui;
use egui_pop_core::inspect::{self as picker, Pick};
use egui_pop_core::strings;
use egui_pop_model::clipboard::{ClipItem, FormatId, Repr, TEXT};
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{BlockId, CommandId};
use serde::Serialize;
use serde_json::{Value as Json, json};

use crate::app::{AppHandle, MENU_BAR_END, MENU_VIEW};
use crate::clipboard::{self, ClipSource, ClipType, CopyRequest};
use crate::dialogs::{Custom, DIALOGS, DialogSpec};
use crate::steps::Outcome;
use crate::strings as txt;
use crate::zones::{self, Block};
use crate::{Ctx, PluginError};

/// Enters or leaves the inspect mode.
pub const CMD_INSPECT: CommandId = CommandId::from_static("devtools.inspect");
/// The context key saying the mode is on (`true` or `false`): the
/// command's `checked` clause, which draws its menu bar button pressed.
pub const CONTEXT_INSPECTING: &str = "inspecting";
/// The pointer over the window while the mode is on (not over the
/// inspect dialog): a crosshair, as browsers' element pickers.
pub const CURSOR: egui::CursorIcon = egui::CursorIcon::Crosshair;
/// The clipboard endpoint Copy takes the last pick from.
pub const ENDPOINT: &str = "devtools.inspect";
/// The JSON form of a pick on the clipboard.
pub const FORMAT: FormatId = FormatId::from_static("application/x-egui-pop.inspect+json");

/// What an application adds to a pick: rows of a name and a value (the
/// gallery: its page and story).
#[derive(Clone)]
pub struct Describe(pub(crate) Rc<DescribeFn>);

/// The function behind [`Describe`].
pub type DescribeFn = dyn Fn(&AppHandle, &Pick) -> Vec<(String, String)>;

impl fmt::Debug for Describe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Describe(..)")
    }
}

/// A pick with what the application knows of it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Inspected {
    pub element: Pick,
    /// `main`, or the detached panel whose window it is in.
    pub window: String,
    /// The zone and block drawing it, the panel holding it and the plugin
    /// owning the innermost of them.
    pub zone: Option<String>,
    pub block: Option<String>,
    pub panel: Option<String>,
    pub plugin: Option<String>,
    pub theme: String,
    pub dark: bool,
    /// The kit the theme is (a kit added to the application), or
    /// `built-in`.
    pub kit: String,
    pub density: String,
    /// What the application adds.
    pub app: Vec<(String, String)>,
}

impl Inspected {
    fn new(app: &AppHandle, element: Pick, describe: Option<&Describe>) -> Inspected {
        let ctx = app.egui();
        let auxiliary = app.layout().auxiliary;
        // A detached panel's window: its own viewport, or embedded in the
        // main one by a backend without windows (kittest), its panel then
        // telling.
        let window = auxiliary
            .iter()
            .find(|a| {
                crate::workbench::window_id(&a.panel) == element.viewport
                    || element
                        .regions
                        .iter()
                        .any(|r| r.kind == "panel" && a.panels().any(|p| r.id == p.as_str()))
            })
            .map_or_else(
                || {
                    if element.viewport == egui::ViewportId::ROOT {
                        "main".to_string()
                    } else {
                        format!("{:?}", element.viewport)
                    }
                },
                |a| format!("panel {}", a.panel),
            );
        let block = element.regions.iter().rev().find(|r| r.kind == "block");
        let panel = element.regions.iter().rev().find(|r| r.kind == "panel");
        let spec = app.theme().spec();
        let kit = if app.theme().kits().iter().any(|k| k.name == spec.base) {
            spec.base.clone()
        } else {
            "built-in".to_string()
        };
        let mut out = Inspected {
            window,
            zone: block.and_then(|b| b.zone.clone()),
            block: block.map(|b| b.id.clone()),
            panel: panel.map(|p| p.id.clone()),
            plugin: element.regions.last().map(|r| r.owner.clone()),
            theme: spec.base.clone(),
            dark: egui_pop_core::theme::Tokens::of(&ctx).dark_mode(),
            kit,
            density: spec.density.name().to_string(),
            app: Vec::new(),
            element,
        };
        if let Some(d) = describe {
            out.app = (d.0)(app, &out.element);
        }
        out
    }

    /// The rows the dialog shows and the text block holds.
    pub fn rows(&self) -> Vec<(String, String)> {
        let e = &self.element;
        let mut rows: Vec<(String, String)> = Vec::new();
        let mut row = |k: &str, v: String| rows.push((k.to_string(), v));
        let what = match (&e.role, &e.label) {
            (Some(r), Some(l)) => format!("{r} {l:?}"),
            (Some(r), None) => r.clone(),
            (None, Some(l)) => format!("{l:?}"),
            (None, None) => "unnamed".to_string(),
        };
        row("Element", format!("{what} ({})", e.kind));
        if let Some(r) = &e.role {
            row("Role", r.clone());
        }
        if let Some(l) = &e.label {
            row("Name", l.clone());
        }
        if let Some(v) = &e.value {
            row("Value", v.clone());
        }
        if let Some(d) = &e.description {
            row("Description", d.clone());
        }
        if let Some(w) = &e.within {
            row("Within", w.clone());
        }
        row("egui id", e.id.clone());
        row(
            "Rect",
            format!(
                "x {:.0}, y {:.0}, {:.0} × {:.0} points",
                e.rect[0], e.rect[1], e.rect[2], e.rect[3]
            ),
        );
        row("Window", self.window.clone());
        row("Layer", e.layer.clone());
        if let Some(z) = &self.zone {
            row("Zone", z.clone());
        }
        if let Some(b) = &self.block {
            row("Block", b.clone());
        }
        if let Some(p) = &self.panel {
            row("Panel", p.clone());
        }
        if let Some(p) = &self.plugin {
            row("Plugin", p.clone());
        }
        row(
            "Theme",
            format!(
                "{} ({}), kit {}, density {}",
                self.theme,
                if self.dark { "dark" } else { "light" },
                self.kit,
                self.density
            ),
        );
        row(
            "Kit part",
            e.kit.as_ref().map_or_else(
                || "none painted through a kit handler".to_string(),
                |k| {
                    format!(
                        "{}{} ({}), state {}: fill {}, border {}, radius {}, text {}",
                        k.component,
                        if k.around { ", around it" } else { "" },
                        k.shape,
                        k.state,
                        k.fill,
                        k.border,
                        k.radius,
                        k.text
                    )
                },
            ),
        );
        row("Focused", yes(e.focused));
        row("Enabled", yes(e.enabled));
        row("Senses", e.sense.clone());
        for (k, v) in &self.app {
            row(k, v.clone());
        }
        row("Picked by", e.how.clone());
        rows
    }

    /// The readable text block: one `Name: value` line per row.
    pub fn text(&self) -> String {
        let mut s = String::new();
        for (k, v) in self.rows() {
            s.push_str(&k);
            s.push_str(": ");
            s.push_str(&v);
            s.push('\n');
        }
        s
    }

    /// The JSON form.
    pub fn json(&self) -> Json {
        let mut j = serde_json::to_value(self).unwrap_or(Json::Null);
        j["app"] = Json::Object(
            self.app
                .iter()
                .map(|(k, v)| (k.clone(), Json::String(v.clone())))
                .collect(),
        );
        j
    }
}

fn yes(b: bool) -> String {
    if b { "yes" } else { "no" }.to_string()
}

/// The application's side of the mode.
#[derive(Default)]
struct Shared {
    last: RefCell<Option<Inspected>>,
    dialog: RefCell<Option<u64>>,
    copied: RefCell<bool>,
    describe: RefCell<Option<Describe>>,
}

fn shared(app: &AppHandle) -> Rc<Shared> {
    app.local::<Shared>()
}

/// The last pick, with what the application knows of it.
pub fn last(app: &AppHandle) -> Option<Inspected> {
    shared(app).last.borrow().clone()
}

/// Whether the mode is on.
pub fn active(app: &AppHandle) -> bool {
    picker::active(&app.egui())
}

/// Enters the mode (when the devtools setting is on).
pub fn start(app: &AppHandle) -> Result<(), String> {
    if !super::enabled(app) {
        return Err("devtools are off (the `developer.devtools` setting)".into());
    }
    picker::start(&app.egui());
    app.publish("egui_pop", None, "inspect.started", Json::Null);
    Ok(())
}

/// Leaves the mode.
pub fn stop(app: &AppHandle) {
    picker::stop(&app.egui());
}

/// Copies the last pick: through the clipboard's `clipboard.copy` step
/// when its plugin is connected, else as text to the system clipboard.
pub fn copy(app: &AppHandle) -> Result<(), String> {
    let connected = app
        .plugins()
        .iter()
        .any(|p| p.id == clipboard::PLUGIN && p.connected);
    let out = if connected {
        match clipboard::copy(app, CopyRequest::new(ENDPOINT), &Origin::User) {
            Outcome::Done(r) => r.map(|_| ()),
            Outcome::Vetoed { by, reason } => Err(format!("vetoed by {by}: {reason}")),
        }
    } else {
        let text = last(app).ok_or("nothing was inspected")?.text();
        app.egui().copy_text(text);
        Ok(())
    };
    if out.is_ok() {
        *shared(app).copied.borrow_mut() = true;
    }
    out
}

struct Source;

impl ClipSource for Source {
    fn copy(&self, app: &AppHandle) -> Option<ClipItem> {
        let i = last(app)?;
        let text = i.text();
        let mut payload = i.json();
        payload["text"] = json!(text);
        Some(ClipItem::new(FORMAT, 1, payload, "Inspected element").fallback(TEXT, json!(text)))
    }

    fn can_copy(&self, app: &AppHandle) -> bool {
        last(app).is_some()
    }
}

/// Declares the mode's command, clipboard endpoint, collector and block.
pub(crate) fn connect(ctx: &mut Ctx<'_>, describe: Option<Describe>) -> Result<(), PluginError> {
    *shared(&ctx.app()).describe.borrow_mut() = describe;
    ctx.command(
        CommandSpec::new(CMD_INSPECT, "Inspect an element", "View")
            .description(
                "Enters or leaves the inspect mode: click an element (or reach it with Tab and the arrows, then Enter) to see what egui-pop knows of it (when the Devtools setting is on).",
            )
            .key("Ctrl+Shift+F12")
            .when("devtoolsEnabled")
            .icon("phosphor:cursor-click")
            .checked(CONTEXT_INSPECTING)
            .menu(MENU_VIEW, "8_devtools", 1)
            .menu(MENU_BAR_END, "9_devtools", 0)
            .no_agent(),
        |call| {
            if active(call.app) {
                stop(call.app);
                Ok(json!({ "inspecting": false }))
            } else {
                start(call.app)?;
                Ok(json!({ "inspecting": true }))
            }
        },
    )?;
    ctx.clip_type(
        ClipType::new(FORMAT, 1, "an inspected element")
            .fallbacks(|p| vec![Repr::new(TEXT, p["text"].clone())]),
    )?;
    ctx.clip_endpoint(ENDPOINT, Some(Rc::new(Source)), None);
    let app = ctx.app().downgrade();
    ctx.collector(
        egui_pop_core::debug::FnCollector::new(
            "inspect",
            "The inspect mode: whether it is on, and the last element picked (what the dialog shows, as JSON and as the text Copy gives).",
        )
        .snapshot(move |ctx| {
            let last = app.get().ok().and_then(|app| last(&app));
            json!({
                "active": picker::active(ctx),
                "last": last.map(|i| json!({ "json": i.json(), "text": i.text() })),
            })
        }),
    )?;
    // In every window: the hint while the mode is on; the pick made in
    // any of them opens the dialog.
    ctx.block(
        zones::WINDOW_OVERLAYS,
        Block::add(BlockId::from_static("egui_pop.devtools.inspect"), 95, pass),
    );
    Ok(())
}

/// Whether the mode is bound to the platform's copy chord with Shift
/// (Ctrl+Shift+C, a user's rebinding): egui-winit turns it into a copy
/// event, which `picker::arm` gives back as the key. Left alone
/// otherwise, so a text field still copies with it.
fn on_the_copy_chord(app: &AppHandle) -> bool {
    app.keys_for(&CMD_INSPECT).iter().any(|k| {
        egui_pop_model::keymap::Keys::parse(k).is_ok_and(|keys| match keys.0.as_slice() {
            [c] => c.key == "c" && c.shift && !c.alt && (c.ctrl || c.meta),
            _ => false,
        })
    })
}

/// Each pass, in each window.
fn pass(ui: &mut Ui, app: &AppHandle) {
    let ctx = ui.ctx().clone();
    let on = super::enabled(app);
    picker::arm(&ctx, on && on_the_copy_chord(app));
    if !on && picker::active(&ctx) {
        picker::stop(&ctx);
    }
    if picker::take_left(&ctx) {
        app.publish("egui_pop", None, "inspect.left", Json::Null);
    }
    if let Some(pick) = picker::take_picked(&ctx) {
        picked(app, pick);
    }
    let active = picker::active(&ctx);
    let key = if active { "true" } else { "false" };
    if app.context().get(CONTEXT_INSPECTING) != key {
        app.set_context(CONTEXT_INSPECTING, key);
        ctx.request_repaint();
    }
    if active {
        hint(ui);
        if !over_dialog(app, &ctx) {
            ctx.set_cursor_icon(CURSOR);
        }
    }
}

/// Whether the pointer is over the inspect dialog (it keeps its own
/// cursors).
fn over_dialog(app: &AppHandle, ctx: &egui::Context) -> bool {
    let Some(id) = *shared(app).dialog.borrow() else {
        return false;
    };
    let Some(pointer) = ctx.pointer_hover_pos() else {
        return false;
    };
    egui_pop_core::overlay::rect(ctx, crate::dialogs::overlay_id(id))
        .is_some_and(|r| r.contains(pointer))
}

fn picked(app: &AppHandle, pick: Pick) {
    let st = shared(app);
    let describe = st.describe.borrow().clone();
    let inspected = Inspected::new(app, pick, describe.as_ref());
    app.publish("egui_pop", None, "inspect.picked", inspected.json());
    *st.last.borrow_mut() = Some(inspected);
    *st.copied.borrow_mut() = false;
    let Some(dialogs) = app.service(&DIALOGS) else {
        return;
    };
    // A dialog already showing a pick shows the new one.
    if st.dialog.borrow().is_some_and(|id| dialogs.is_open(id)) {
        return;
    }
    let title = strings::tr(&app.egui(), &txt::INSPECT_TITLE);
    let spec = DialogSpec::new(Custom::closing(&title).size(560.0, 420.0)).closable(true);
    if let Ok(id) = dialogs.open_custom(app, spec, body) {
        *st.dialog.borrow_mut() = Some(id);
    }
}

fn body(ui: &mut Ui, app: &AppHandle, pass: &mut crate::dialogs::CustomPass) {
    let Some(i) = last(app) else {
        pass.cancel();
        return;
    };
    pass.describe(i.json());
    let ctx = ui.ctx().clone();
    ui.horizontal(|ui| {
        if ui.button(strings::tr(&ctx, &txt::INSPECT_COPY)).clicked()
            && let Err(e) = copy(app)
        {
            app.publish(
                "egui_pop",
                None,
                "inspect.copy_failed",
                json!({ "error": e }),
            );
        }
        // Report: a new issue with this pick, when the application says
        // where issues go (`help.report`).
        if app.command(&crate::about::CMD_REPORT).is_some()
            && ui
                .button(strings::tr(&ctx, &txt::INSPECT_REPORT))
                .on_hover_text(strings::tr(&ctx, &txt::INSPECT_REPORT_HOVER))
                .clicked()
        {
            app.run(crate::about::CMD_REPORT, Json::Null);
        }
        if ui
            .button(strings::tr(&ctx, &txt::INSPECT_PICK_ANOTHER))
            .clicked()
        {
            pass.cancel();
            let _ = start(app);
        }
        if *shared(app).copied.borrow() {
            ui.label(strings::tr(&ctx, &txt::INSPECT_COPIED));
        }
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .id_salt("egui_pop.inspect.rows")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("egui_pop.inspect.grid")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    for (k, v) in i.rows() {
                        ui.strong(k);
                        ui.add(egui::Label::new(egui::RichText::new(v).monospace()).wrap());
                        ui.end_row();
                    }
                });
        });
}

/// The hint at the top of the window.
fn hint(ui: &Ui) {
    let ctx = ui.ctx();
    let t = egui_pop_core::theme::Tokens::of(ctx);
    let text = strings::tr(ctx, &txt::INSPECT_HINT);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Debug,
        egui::Id::unique("egui_pop.inspect.hint"),
    ));
    let screen = ctx.content_rect();
    let galley = painter.layout(
        text,
        egui::FontId::proportional(t.font.small),
        t.text.on_accent,
        (screen.width() - 4.0 * t.space.m).max(80.0),
    );
    let pad = egui::vec2(t.space.s, t.space.xs);
    let size = galley.size() + 2.0 * pad;
    let rect = egui::Rect::from_min_size(
        egui::pos2(screen.center().x - size.x / 2.0, screen.top() + t.space.s),
        size,
    );
    painter.rect_filled(rect, t.radius.widget, t.accent.default);
    painter.galley(rect.min + pad, galley, t.text.on_accent);
}

/// The devtools panel's button.
pub(crate) fn button(ui: &mut Ui, app: &AppHandle) {
    let label = strings::tr(ui.ctx(), &txt::INSPECT_BUTTON);
    if egui_pop_core::widgets::selectable_label(ui, active(app), label).clicked() {
        app.run(CMD_INSPECT, Json::Null);
    }
}
