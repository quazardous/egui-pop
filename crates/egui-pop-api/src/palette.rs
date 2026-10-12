//! The command palette, as a plugin ([`PalettePlugin`], in
//! `DefaultPlugins`): every palette command that applies where the
//! palette opened, ranked by a fuzzy match of `Category: Title` (and the
//! command's id), the commands run lately first. A command declaring
//! arguments (an example object, `{"percent": 100}`) asks for each in
//! turn, the example's value as the default; Enter runs it in the
//! context the palette opened in.
//!
//! Keys: `Ctrl+Shift+P` and `F1` open and close it (through modals), the
//! arrows move the selection, Enter runs, Escape closes (or leaves an
//! argument prompt).
//!
//! ```
//! use egui_pop_api::{App, palette};
//!
//! let app = App::builder()
//!     .plugin(palette::PalettePlugin)
//!     .build(&egui::Context::default())
//!     .unwrap();
//! assert!(!palette::is_open(app.handle()));
//! ```

use std::sync::{Arc, Mutex};

use egui::text::LayoutJob;
use egui::{Color32, FontId, Key, TextFormat, Ui};
use egui_pop_core::lock;
use egui_pop_core::overlay::{self, Kind, OverlaySpec};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::fuzzy::{self, Match};
use egui_pop_model::ids::{BlockId, CommandId, PluginId};
use egui_pop_model::keymap::BindOptions;
use egui_pop_model::when::WhenContext;
use serde_json::{Map, Value as Json, json};

use crate::app::{AppHandle, MENU_VIEW};
use crate::hooks::PaletteEntry;
use crate::services::ServiceToken;
use crate::zones::{self, Block};
use crate::{Ctx, Plugin, PluginError};

/// Opens and closes the palette.
pub const CMD_PALETTE: CommandId = CommandId::from_static("app.palette");

/// The palette's state, as a service.
pub const PALETTE: ServiceToken<Palette> = ServiceToken::new("app.palette");

/// How many recent commands come first.
pub const RECENT: usize = 8;

/// The plugin.
#[derive(Debug, Default)]
pub struct PalettePlugin;

impl Plugin for PalettePlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("egui_pop.palette")
    }

    fn title(&self) -> String {
        "Command palette".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.service(&PALETTE, Palette::default())?;
        ctx.command(
            CommandSpec::new(CMD_PALETTE, "Command palette", "View")
                .description("Search and run any command.")
                // Through modals: the palette is one, and closes on its key.
                .key_with("Ctrl+Shift+P", BindOptions::global().through_modal())
                .key_with("F1", BindOptions::global().through_modal())
                .menu(MENU_VIEW, "0_palette", 0)
                .hidden(),
            |call| {
                let p = call.app.service(&PALETTE).ok_or("no palette")?;
                if p.is_open() {
                    p.close();
                } else {
                    p.open(call.app.when_context());
                }
                Ok(json!({ "open": p.is_open() }))
            },
        )?;
        ctx.block(
            zones::OVERLAYS,
            Block::add(BlockId::from_static("egui_pop.palette"), 0, show),
        );
        Ok(())
    }
}

/// An argument being asked for.
#[derive(Clone, Debug)]
struct Prompt {
    command: CommandId,
    title: String,
    /// Each argument's name and example.
    args: Vec<(String, Json)>,
    at: usize,
    values: Map<String, Json>,
    input: String,
    error: Option<String>,
}

#[derive(Debug, Default)]
struct State {
    open: bool,
    /// The context when the palette opened: its commands are offered and
    /// run there.
    context: WhenContext,
    query: String,
    selected: usize,
    prompt: Option<Prompt>,
}

/// The palette's state: open or not, the query.
#[derive(Debug, Default)]
pub struct Palette(Arc<Mutex<State>>);

impl Palette {
    pub fn is_open(&self) -> bool {
        lock(&self.0).open
    }

    /// Opens it, offering what applies in `context`.
    pub fn open(&self, context: WhenContext) {
        let mut s = lock(&self.0);
        *s = State {
            open: true,
            context,
            ..State::default()
        };
    }

    pub fn close(&self) {
        *lock(&self.0) = State::default();
    }

    /// The query typed.
    pub fn query(&self) -> String {
        lock(&self.0).query.clone()
    }
}

/// Whether the application's palette is open.
pub fn is_open(app: &AppHandle) -> bool {
    app.service(&PALETTE).is_some_and(|p| p.is_open())
}

/// The commands run lately, most recent first, each once.
fn recent(app: &AppHandle) -> Vec<CommandId> {
    let mut out: Vec<CommandId> = Vec::new();
    for r in app.history().iter().rev() {
        if r.id != CMD_PALETTE && r.result.is_ok() && !out.contains(&r.id) {
            out.push(r.id.clone());
            if out.len() == RECENT {
                break;
            }
        }
    }
    out
}

/// One line of the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Ranked {
    pub entry: PaletteEntry,
    /// `Category: Title`, as shown.
    pub label: String,
    /// The matched characters of `label`.
    pub positions: Vec<usize>,
    pub recent: bool,
}

/// The palette's list for a query: the entries that apply, best first.
/// Without a query, the recent ones first, then by category and title;
/// with one, by fuzzy score (`Category: Title`, or the id a little
/// behind), a recent command winning ties and close calls.
pub fn rank(
    entries: &[PaletteEntry],
    query: &str,
    recent: &[CommandId],
    applies: impl Fn(&CommandId) -> bool,
) -> Vec<Ranked> {
    let mut out: Vec<(i32, usize, Ranked)> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        if !applies(&e.command) {
            continue;
        }
        let label = format!("{}: {}", e.category, e.title);
        let by_label = fuzzy::score(query, &label);
        let by_id = fuzzy::score(query, e.command.as_str()).map(|m| Match {
            score: m.score - 16,
            positions: Vec::new(),
        });
        let best = match (by_label, by_id) {
            (Some(a), Some(b)) => Some(if b.score > a.score { b } else { a }),
            (a, b) => a.or(b),
        };
        let Some(m) = best else { continue };
        let at = recent.iter().position(|r| *r == e.command);
        let bonus = at.map_or(0, |n| (RECENT - n) as i32 * 4);
        let score = if query.trim().is_empty() {
            at.map_or(0, |n| 1000 - n as i32)
        } else {
            m.score + bonus
        };
        out.push((
            score,
            i,
            Ranked {
                entry: e.clone(),
                label,
                positions: m.positions,
                recent: at.is_some(),
            },
        ));
    }
    out.sort_by_key(|(s, i, _)| (std::cmp::Reverse(*s), *i));
    out.into_iter().map(|(_, _, r)| r).collect()
}

/// The arguments a command asks for: the keys of its example object.
fn arguments(spec: &CommandSpec) -> Vec<(String, Json)> {
    match &spec.args {
        Json::Object(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => Vec::new(),
    }
}

/// A typed answer, read as the example's type; empty: the example.
fn parse_arg(input: &str, example: &Json) -> Result<Json, String> {
    let t = input.trim();
    if t.is_empty() {
        return Ok(example.clone());
    }
    match example {
        Json::Bool(_) => match t {
            "true" | "yes" | "on" => Ok(Json::Bool(true)),
            "false" | "no" | "off" => Ok(Json::Bool(false)),
            _ => Err(format!("`{t}` is not true or false")),
        },
        Json::Number(n) if n.is_f64() => t
            .parse::<f64>()
            .map(|v| json!(v))
            .map_err(|_| format!("`{t}` is not a number")),
        Json::Number(_) => t
            .parse::<i64>()
            .map(|v| json!(v))
            .or_else(|_| t.parse::<f64>().map(|v| json!(v)))
            .map_err(|_| format!("`{t}` is not a number")),
        Json::String(_) => Ok(Json::String(t.into())),
        _ => serde_json::from_str(t).map_err(|e| format!("not JSON: {e}")),
    }
}

/// The label with its matched characters in the strong text colour.
fn highlighted(ui: &Ui, label: &str, positions: &[usize]) -> LayoutJob {
    let font = FontId::default();
    let normal = ui.visuals().text_color();
    let strong = ui.visuals().strong_text_color();
    let mut job = LayoutJob::default();
    for (i, c) in label.chars().enumerate() {
        let hit = positions.contains(&i);
        job.append(
            c.encode_utf8(&mut [0; 4]),
            0.0,
            TextFormat {
                font_id: font.clone(),
                color: if hit { strong } else { normal },
                underline: if hit {
                    egui::Stroke::new(1.0, strong)
                } else {
                    egui::Stroke::NONE
                },
                background: Color32::TRANSPARENT,
                ..TextFormat::default()
            },
        );
    }
    job
}

const SPEC_ID: &str = "egui_pop.palette";

/// Draws the palette while it is open.
fn show(ui: &mut Ui, app: &AppHandle) {
    let Some(p) = app.service(&PALETTE) else {
        return;
    };
    let mut open = p.is_open();
    if !open {
        return;
    }
    let ctx = ui.ctx().clone();
    // The palette handles Escape itself: back from a prompt, else close.
    let spec = OverlaySpec::new(SPEC_ID, "Command palette", Kind::Modal).keep_on_escape();
    let entries = app.palette();
    let recent = recent(app);
    let mut run: Option<(CommandId, Json, WhenContext)> = None;
    let mut close = false;
    overlay::show(&ctx, &spec, &mut open, None, |ui| {
        let mut s = lock(&p.0);
        let cx = s.context.clone();
        if let Some(prompt) = &mut s.prompt {
            if let Some(done) = ask(ui, prompt) {
                match done {
                    Some(args) => run = Some((prompt.command.clone(), args, cx)),
                    None => s.prompt = None,
                }
            }
            return;
        }
        let title = ui.strong("Command palette");
        let edit = ui
            .add(
                egui::TextEdit::singleline(&mut s.query)
                    .hint_text("Type a command")
                    .desired_width(f32::INFINITY),
            )
            .labelled_by(title.id);
        edit.request_focus();
        egui_pop_core::focus::escape_passes(ui.ctx(), edit.id);
        if edit.changed() {
            s.selected = 0;
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            close = true;
            return;
        }
        let hits = rank(&entries, &s.query, &recent, |c| app.applies(c, &cx));
        if ui.input(|i| i.key_pressed(Key::ArrowDown)) {
            s.selected = (s.selected + 1).min(hits.len().saturating_sub(1));
        }
        if ui.input(|i| i.key_pressed(Key::ArrowUp)) {
            s.selected = s.selected.saturating_sub(1);
        }
        let selected = s.selected.min(hits.len().saturating_sub(1));
        let mut chosen = None;
        if ui.input(|i| i.key_pressed(Key::Enter)) {
            chosen = hits.get(selected).map(|h| h.entry.command.clone());
        }
        ui.separator();
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                // Justified, left-aligned: a line without keys keeps its
                // label at the start, as the others.
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                    for (i, h) in hits.iter().enumerate() {
                        let mut b = egui::Button::new(highlighted(ui, &h.label, &h.positions))
                            .selected(i == selected)
                            .frame_when_inactive(i == selected);
                        let mut keys = h.entry.keys.first().cloned().unwrap_or_default();
                        if h.recent && s.query.trim().is_empty() {
                            keys = if keys.is_empty() {
                                "recent".into()
                            } else {
                                format!("recent  {keys}")
                            };
                        }
                        if !keys.is_empty() {
                            b = b.shortcut_text(keys);
                        }
                        let r = ui.add(b);
                        if i == selected && edit.changed() {
                            r.scroll_to_me(None);
                        }
                        if r.clicked() {
                            chosen = Some(h.entry.command.clone());
                        }
                    }
                    if hits.is_empty() {
                        ui.weak("No command matches.");
                    }
                });
            });
        if let Some(c) = chosen {
            let spec = app.command(&c);
            let args = spec.as_ref().map(arguments).unwrap_or_default();
            if args.is_empty() {
                run = Some((c, Json::Null, cx));
            } else {
                s.prompt = Some(Prompt {
                    command: c,
                    title: spec.map(|s| s.title).unwrap_or_default(),
                    args,
                    at: 0,
                    values: Map::new(),
                    input: String::new(),
                    error: None,
                });
            }
        }
    });
    if let Some((command, args, context)) = run {
        p.close();
        overlay::close(&ctx, spec.id);
        app.request_run_in(command, args, context);
    } else if close {
        p.close();
        overlay::close(&ctx, spec.id);
    } else if !open {
        p.close();
    }
}

/// Asks for the prompt's current argument. `Some(Some(args))`: all
/// given; `Some(None)`: given up (Escape); `None`: still asking.
#[allow(clippy::option_option)]
fn ask(ui: &mut Ui, prompt: &mut Prompt) -> Option<Option<Json>> {
    let (name, example) = prompt.args[prompt.at].clone();
    let title = ui.strong(format!("{}: {name}", prompt.title));
    let edit = ui
        .add(
            egui::TextEdit::singleline(&mut prompt.input)
                .hint_text(example.to_string())
                .desired_width(f32::INFINITY),
        )
        .labelled_by(title.id);
    edit.request_focus();
    egui_pop_core::focus::escape_passes(ui.ctx(), edit.id);
    ui.weak(format!(
        "Argument {} of {}. Enter: next; empty: {example}; Escape: back.",
        prompt.at + 1,
        prompt.args.len()
    ));
    if let Some(e) = &prompt.error {
        ui.colored_label(ui.visuals().error_fg_color, e);
    }
    // Escape here goes back to the list instead of closing the palette.
    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
        return Some(None);
    }
    if !ui.input(|i| i.key_pressed(Key::Enter)) {
        return None;
    }
    match parse_arg(&prompt.input, &example) {
        Ok(v) => {
            prompt.values.insert(name, v);
            prompt.input.clear();
            prompt.error = None;
            prompt.at += 1;
            (prompt.at == prompt.args.len()).then(|| Some(Json::Object(prompt.values.clone())))
        }
        Err(e) => {
            prompt.error = Some(e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &'static str, category: &str, title: &str) -> PaletteEntry {
        PaletteEntry {
            command: CommandId::from_static(id),
            title: title.into(),
            category: category.into(),
            keys: Vec::new(),
        }
    }

    #[test]
    fn recent_commands_come_first_and_the_query_ranks_the_rest() {
        let entries = [
            entry("file.open", "File", "Open file"),
            entry("view.zoom", "View", "Zoom in"),
            entry("edit.undo", "Edit", "Undo"),
            entry("edit.hidden", "Edit", "Not here"),
        ];
        let recent = [CommandId::from_static("edit.undo")];
        let here = |c: &CommandId| c.as_str() != "edit.hidden";
        let ids = |r: Vec<Ranked>| {
            r.into_iter()
                .map(|r| r.entry.command.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(rank(&entries, "", &recent, here)),
            ["edit.undo", "file.open", "view.zoom"]
        );
        assert_eq!(ids(rank(&entries, "zi", &recent, here)), ["view.zoom"]);
        // The id matches too, behind the label.
        assert_eq!(ids(rank(&entries, "fileopen", &[], here)), ["file.open"]);
        let r = rank(&entries, "of", &[], here);
        assert_eq!(r[0].label, "File: Open file");
        assert_eq!(r[0].positions, [6, 11]);
    }

    #[test]
    fn arguments_are_read_as_their_example() {
        assert_eq!(parse_arg("", &json!(100)), Ok(json!(100)));
        assert_eq!(parse_arg("50", &json!(100)), Ok(json!(50)));
        assert_eq!(parse_arg("0.5", &json!(1.5)), Ok(json!(0.5)));
        assert!(parse_arg("lots", &json!(100)).is_err());
        assert_eq!(parse_arg("yes", &json!(false)), Ok(json!(true)));
        assert_eq!(parse_arg(" a b ", &json!("x")), Ok(json!("a b")));
        assert_eq!(parse_arg("[1, 2]", &json!([0])), Ok(json!([1, 2])));
    }
}
