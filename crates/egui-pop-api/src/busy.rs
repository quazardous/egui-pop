//! Controls bound to a command show its busy state: a run waiting for
//! the host's result (`Reply::Later`, ADR 0010) greys the control and,
//! after `motion.spinner.delay`, a spinner takes its icon's place for at
//! least `motion.spinner.min`. The control keeps its width and its name
//! (AccessKit's `busy` flag is set while the run waits); a failed run
//! says why on hover. Menu items do the same (`menus`), and the status
//! bar's indicators show a delayed spinner while busy (`strips`).
//!
//! ```
//! use egui_pop_api::AppHandle;
//! use egui_pop_api::busy::CommandButton;
//! use egui_pop_model::ids::CommandId;
//!
//! fn toolbar(ui: &mut egui::Ui, app: &AppHandle) {
//!     ui.add(CommandButton::new(app, CommandId::from_static("doc.export")));
//! }
//! ```

use egui::{Atom, Button, Color32, Id, Rect, Response, RichText, Shape, Stroke, Ui, Widget, vec2};
use egui_pop_core::icon::Icon;
use egui_pop_core::kit::{Kit, Part, State};
use egui_pop_core::motion::Busy;
use egui_pop_core::progress::paint_spinner;
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;
use egui_pop_model::ids::CommandId;
use serde_json::Value as Json;

use crate::app::AppHandle;
use crate::strings as txt;

/// Whether the busy indicator of a command shows in a control (`id`
/// names the control): after the delay while a run waits, then for the
/// minimum.
pub fn busy_shown(app: &AppHandle, ctx: &egui::Context, id: Id, command: &CommandId) -> bool {
    let running = app.running(command);
    let since = app.running_since(command).unwrap_or(0.0);
    Busy::new(id, since).visible(ctx, running)
}

/// Paints a control's icon slot: a spinner while busy, else its icon.
pub fn paint_slot(ui: &Ui, rect: Rect, busy: bool, icon: Option<Icon>, enabled: bool) {
    let t = Tokens::of(ui.ctx());
    let color = if enabled {
        t.text.default
    } else {
        t.text.muted
    };
    if busy {
        paint_spinner(ui, rect, color);
    } else if let Some(i) = icon {
        i.paint(ui.painter(), rect, color);
    }
}

/// A button running a command, with its title, its icon slot and its
/// busy state.
#[derive(Debug)]
#[must_use = "add it with `ui.add`"]
pub struct CommandButton<'a> {
    app: &'a AppHandle,
    command: CommandId,
    args: Json,
    label: Option<String>,
    icon: Option<Icon>,
}

impl<'a> CommandButton<'a> {
    pub fn new(app: &'a AppHandle, command: CommandId) -> Self {
        Self {
            app,
            command,
            args: Json::Null,
            label: None,
            icon: None,
        }
    }

    /// The arguments it runs the command with.
    pub fn args(mut self, args: Json) -> Self {
        self.args = args;
        self
    }

    /// Its text (the command's title by default).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The icon before its text, which a spinner replaces while busy.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
}

impl Widget for CommandButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let ctx = ui.ctx().clone();
        let t = Tokens::of(&ctx);
        let app = self.app;
        let id = &self.command;
        let spec = app.command(id);
        let title = self.label.clone().unwrap_or_else(|| {
            spec.as_ref()
                .map_or_else(|| id.to_string(), |s| app.command_title(s))
        });
        let running = app.running(id);
        let applies = spec.is_some() && app.applies(id, &app.when_context());
        let enabled = applies && !running;
        let slot = Id::new(("egui_pop_command_button", id.as_str(), ui.id()));
        let side = t.size.icon_inline;
        // egui lays the button out and takes its clicks; the kit's
        // `command-button` paints its box under it (`under`), in the
        // state of the last pass as egui's own frame would. egui greys
        // it when disabled.
        let kit = Kit::of(&ctx);
        let look = &kit.command_button;
        let r = ui
            .add_enabled_ui(enabled, |ui| {
                let under = ui.painter().add(Shape::Noop);
                let state = ui
                    .ctx()
                    .read_response(ui.next_auto_id())
                    .map_or(State::REST, |r| State::from_response(&r))
                    .with(State::DISABLED, !ui.is_enabled());
                let label = RichText::new(title.as_str()).color(look.style(state).text);
                // egui's margin keeps room for its own border: kept, unseen.
                let border = ui.visuals().widgets.inactive.bg_stroke.width;
                let r = Button::new((Atom::custom(slot, vec2(side, side)), label))
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::new(border, Color32::TRANSPARENT))
                    .atom_ui(ui);
                let rect = r.response.rect;
                let part = Part {
                    rect,
                    room: rect,
                    state,
                    label: None,
                };
                look.paint_under(ui, under, &part, None);
                r
            })
            .inner;
        let busy = busy_shown(app, &ctx, slot, id);
        if let Some(rect) = r.rect(slot) {
            paint_slot(ui, rect, busy, self.icon, enabled);
        }
        let mut response = r.response;
        if running {
            ctx.accesskit_node_builder(response.id, |node| node.set_busy());
            response = response.on_disabled_hover_text(strings::tr(&ctx, &txt::MENU_RUNNING));
        } else if let Some(e) = app
            .history()
            .iter()
            .rev()
            .find(|run| run.id == *id)
            .and_then(|run| run.result.clone().err())
        {
            response = response.on_hover_text(strings::tr_args(
                &ctx,
                &txt::MENU_LAST_RUN_FAILED,
                &[("error", &e)],
            ));
        }
        if response.clicked() {
            app.run(id.clone(), self.args.clone());
        }
        response
    }
}
