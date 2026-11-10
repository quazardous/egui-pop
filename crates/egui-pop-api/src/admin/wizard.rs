//! The wizard kit's surface (`docs/design/admin-forms.md`, section 5.3):
//! a wizard declared in a manifest ([`egui_pop_model::admin::wizard`])
//! fills the Settings app's page while it runs: the stepper on the left,
//! the step's form, and a footer with Cancel, Back, Skip and Next (Finish
//! on the summary).
//!
//! - Next is greyed while the step has a blocking problem, its reason in
//!   the tooltip and beside it; each problem shows under its field.
//! - The stepper goes back to any step, and ahead to a step whose steps
//!   before it are valid; it says "Step 2 of 4, Destination" to
//!   assistive technology.
//! - Cancel (and Escape) asks first when anything was entered.
//! - Keys: Enter (when no widget takes it first: a text field, an open
//!   list, a focused control) and `Alt+N` go next or finish,
//!   `Alt+B` back, Escape cancels.
//! - Every gesture is a command an agent runs too: `admin.wizard.open`
//!   (`{"id", "step", "values"}`), `admin.wizard.set` (`{"key", "value"}`
//!   or `{"values"}`), `admin.wizard.next`, `admin.wizard.back`,
//!   `admin.wizard.skip`, `admin.wizard.go` (`{"step"}`),
//!   `admin.wizard.finish` and `admin.wizard.cancel` (`{"force"}`); each
//!   answers the flow's state, which the `admin` collector shows too.
//!
//! Finishing runs the wizard's command with the draft as its arguments,
//! then closes the wizard (and the Settings app, when the wizard opened
//! it).

use egui::accesskit::Role;
use egui::{Align, Key, Modifiers, Ui};
use egui_pop_core::a11y;
use egui_pop_core::indicator::Indicator;
use egui_pop_core::lock;
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;
use egui_pop_core::ui::UiPop as _;
use egui_pop_core::widgets;
use egui_pop_model::admin::hooks::{Problem, Values};
use egui_pop_model::admin::wizard::{Judge, SUMMARY};
use egui_pop_model::admin::{
    Check, Field, FieldKind, Flow, Item, Step, Tone, Wizard, cell_text, holds,
};
use egui_pop_model::ids::CommandId;
use serde_json::{Value as Json, json};

use super::{ADMIN, Admin, shell};
use crate::app::AppHandle;
use crate::strings as txt;

/// Opens a wizard (`{"id", "step", "values"}`).
pub const CMD_WIZARD_OPEN: CommandId = CommandId::from_static("admin.wizard.open");
/// Sets draft values (`{"key", "value"}` or `{"values": {...}}`).
pub const CMD_WIZARD_SET: CommandId = CommandId::from_static("admin.wizard.set");
/// Goes to the next step, if the step shown is valid.
pub const CMD_WIZARD_NEXT: CommandId = CommandId::from_static("admin.wizard.next");
/// Goes back a step.
pub const CMD_WIZARD_BACK: CommandId = CommandId::from_static("admin.wizard.back");
/// Skips an optional step.
pub const CMD_WIZARD_SKIP: CommandId = CommandId::from_static("admin.wizard.skip");
/// Goes to a step (`{"step"}`, `summary` for the summary).
pub const CMD_WIZARD_GO: CommandId = CommandId::from_static("admin.wizard.go");
/// Runs the wizard's command with the draft, and closes it.
pub const CMD_WIZARD_FINISH: CommandId = CommandId::from_static("admin.wizard.finish");
/// Closes the wizard; asks first when anything was entered, unless
/// `{"force": true}`.
pub const CMD_WIZARD_CANCEL: CommandId = CommandId::from_static("admin.wizard.cancel");

/// A wizard running.
#[derive(Debug, Clone)]
pub(super) struct Running {
    pub flow: Flow,
    /// The wizard opened the Settings app: its end closes it.
    pub opened: bool,
    /// Why the last Next, Go or Finish was refused; cleared by the next
    /// gesture.
    pub refused: Option<String>,
}

/// A field's own check, in words.
fn check_text(ctx: &egui::Context, f: &Field, c: Check) -> String {
    let field = [("field", f.meta.title.as_str())];
    match c {
        Check::Required => strings::tr_args(ctx, &txt::ADMIN_WIZARD_REQUIRED, &field),
        Check::Below(min) => strings::tr_args(
            ctx,
            &txt::ADMIN_WIZARD_BELOW,
            &[field[0], ("min", &min.to_string())],
        ),
        Check::Above(max) => strings::tr_args(
            ctx,
            &txt::ADMIN_WIZARD_ABOVE,
            &[field[0], ("max", &max.to_string())],
        ),
        Check::NotAChoice => strings::tr_args(ctx, &txt::ADMIN_WIZARD_NOT_A_CHOICE, &field),
    }
}

impl Admin {
    /// Runs `f` with a judge: the application's keys, the hooks, the
    /// checks' words.
    fn judging<R>(&self, app: &AppHandle, f: impl FnOnce(&Judge<'_>) -> R) -> R {
        let ctx = app.egui();
        let say = move |field: &Field, c: Check| check_text(&ctx, field, c);
        let hooks = self.hooks();
        let j = Judge {
            base: app.when_context(),
            hooks: &hooks,
            say: &say,
        };
        f(&j)
    }

    /// The wizard running, if any.
    pub fn wizard_flow(&self) -> Option<Flow> {
        lock(&self.wizard).as_ref().map(|r| r.flow.clone())
    }

    pub(super) fn running(&self) -> Option<Running> {
        lock(&self.wizard).clone()
    }

    /// Opens a wizard declared in a manifest, on its first step (or on
    /// `step`, if the steps before it are valid), its draft the defaults
    /// with `values` over them. The Settings app opens with it.
    pub fn open_wizard(
        &self,
        app: &AppHandle,
        id: &str,
        step: Option<&str>,
        values: Values,
    ) -> Result<Json, String> {
        let tree = self.tree(app);
        let w = tree.wizard(id).ok_or_else(|| format!("no wizard `{id}`"))?;
        let (flow, refused) = self.judging(app, |j| {
            let mut f = Flow::start(w, &j.base, values);
            let refused = step.and_then(|s| f.go(w, j, s).err());
            (f, refused)
        });
        let opened = !self.is_open();
        *lock(&self.wizard) = Some(Running {
            flow,
            opened,
            refused,
        });
        lock(&self.state).open = true;
        self.refresh();
        Ok(self.wizard_state(app))
    }

    /// Runs `f` on the flow running and its wizard; its refusal is kept
    /// (shown beside Next) and returned.
    fn on_flow<R>(
        &self,
        app: &AppHandle,
        f: impl FnOnce(&Wizard, &mut Flow, &Judge<'_>) -> Result<R, String>,
    ) -> Result<R, String> {
        let tree = self.tree(app);
        let mut guard = lock(&self.wizard);
        let run = guard.as_mut().ok_or("no wizard is open")?;
        let w = tree
            .wizard(&run.flow.wizard)
            .ok_or_else(|| format!("the wizard `{}` is gone", run.flow.wizard))?;
        let r = self.judging(app, |j| f(w, &mut run.flow, j));
        run.refused = r.as_ref().err().cloned();
        r
    }

    /// Sets draft values.
    pub fn wizard_set(&self, app: &AppHandle, values: Values) -> Result<Json, String> {
        self.on_flow(app, |w, f, _| {
            for (k, v) in values {
                f.set(w, &k, v)?;
            }
            Ok(())
        })?;
        Ok(self.wizard_state(app))
    }

    /// Next: refused with the step's first blocking problem.
    pub fn wizard_next(&self, app: &AppHandle) -> Result<Json, String> {
        self.on_flow(app, |w, f, j| f.next(w, j))?;
        Ok(self.wizard_state(app))
    }

    pub fn wizard_back(&self, app: &AppHandle) -> Result<Json, String> {
        self.on_flow(app, |w, f, j| f.back(w, &j.base))?;
        Ok(self.wizard_state(app))
    }

    pub fn wizard_skip(&self, app: &AppHandle) -> Result<Json, String> {
        self.on_flow(app, |w, f, j| f.skip(w, &j.base))?;
        Ok(self.wizard_state(app))
    }

    /// Goes to a step: lands on the first invalid one before it, if any,
    /// and says why.
    pub fn wizard_go(&self, app: &AppHandle, step: &str) -> Result<Json, String> {
        self.on_flow(app, |w, f, j| f.go(w, j, step))?;
        Ok(self.wizard_state(app))
    }

    /// Finishes: runs the wizard's command with the draft, then closes
    /// it. Refused (and landed on the step) while a step is not valid.
    pub fn wizard_finish(&self, app: &AppHandle) -> Result<Json, String> {
        let (wizard, (command, args)) = self.on_flow(app, |w, f, j| {
            f.finish(w, j)
                .map(|r| (w.meta.id.clone(), r))
                .map_err(|(step, reason)| {
                    f.at = step;
                    reason
                })
        })?;
        let id = CommandId::new(&command).map_err(|e| format!("{e:?}"))?;
        if app.command(&id).is_none() {
            return Err(format!("no command `{command}`"));
        }
        app.run(id, args.clone());
        self.end_wizard();
        let done = json!({ "finished": wizard, "command": command, "args": args });
        *lock(&self.finished) = Some(done.clone());
        Ok(done)
    }

    /// Cancels: asks first when anything was entered (unless `force`),
    /// through the dialogs; without them, refused.
    pub fn wizard_cancel(&self, app: &AppHandle, force: bool) -> Result<Json, String> {
        let Some(run) = self.running() else {
            return Err("no wizard is open".into());
        };
        if run.flow.dirty() && !force {
            let tree = self.tree(app);
            let ctx = app.egui();
            let title = tree
                .wizard(&run.flow.wizard)
                .map_or(run.flow.wizard.clone(), |w| w.meta.title.clone());
            let asked = tree.wizard(&run.flow.wizard).and_then(|w| w.cancel.clone());
            let (title, text, verb) = match asked {
                Some(c) => (c.title, c.text, c.verb),
                None => (
                    strings::tr_args(&ctx, &txt::ADMIN_WIZARD_CANCEL_TITLE, &[("wizard", &title)]),
                    strings::tr(&ctx, &txt::ADMIN_WIZARD_CANCEL_TEXT),
                    strings::tr(&ctx, &txt::ADMIN_WIZARD_CANCEL_VERB),
                ),
            };
            let dialogs = app
                .service(&crate::dialogs::DIALOGS)
                .ok_or("something was entered: cancel with `force` to lose it")?;
            dialogs.open(
                crate::dialogs::Confirm::new(&title, &verb, CMD_WIZARD_CANCEL)
                    .detail(&text)
                    .args(json!({ "force": true }))
                    .danger(),
            );
            return Ok(json!({ "asked": true }));
        }
        self.end_wizard();
        Ok(json!({ "cancelled": run.flow.wizard }))
    }

    /// Ends the wizard running; the Settings app closes when the wizard
    /// opened it.
    fn end_wizard(&self) {
        let run = lock(&self.wizard).take();
        if run.is_some_and(|r| r.opened) {
            self.close();
        }
    }

    /// The flow's state, for its commands' answers and the collector:
    /// the step, the steps and their state, the draft, the problems.
    pub fn wizard_state(&self, app: &AppHandle) -> Json {
        let Some(run) = self.running() else {
            return Json::Null;
        };
        let tree = self.tree(app);
        let Some(w) = tree.wizard(&run.flow.wizard) else {
            return Json::Null;
        };
        let f = &run.flow;
        self.judging(app, |j| {
            let shown = f.shown(w, &j.base);
            let index = f.index(w, &j.base);
            let blocked: Vec<bool> = shown.iter().map(|s| f.blocked(s, j).is_some()).collect();
            let steps: Vec<Json> = shown
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    json!({
                        "id": s.meta.id,
                        "title": s.meta.title,
                        "state": step_state(f, &s.meta.id, i, index, &blocked),
                        "optional": s.optional,
                    })
                })
                .collect();
            let here = w
                .step_of(&f.at)
                .map(|s| f.problems(s, j))
                .unwrap_or_default();
            let blocking = here.iter().find(|p| p.tone == Tone::Danger);
            json!({
                "wizard": w.meta.id,
                "title": w.meta.title,
                "step": f.at,
                "index": index + 1,
                "count": shown.len(),
                "summary": w.summary,
                "steps": steps,
                "values": f.values,
                "draft": f.draft(w, &j.base),
                "problems": here,
                "blocked": blocking.map(|p| p.message.clone()),
                "refused": run.refused,
                "dirty": f.dirty(),
                "canFinish": f.first_blocked(w, j).is_none(),
            })
        })
    }

    /// What the last finish ran (the wizard, the command, its arguments).
    pub fn last_finished(&self) -> Option<Json> {
        lock(&self.finished).clone()
    }
}

/// A step's state in the stepper: `current`, `done` (passed), `todo`
/// (reachable), `later` (a step before it is not valid).
fn step_state(f: &Flow, id: &str, i: usize, index: usize, blocked: &[bool]) -> &'static str {
    if i == index {
        "current"
    } else if blocked[..i].iter().any(|b| *b) && i > index {
        "later"
    } else if f.passed.iter().any(|p| p == id) {
        "done"
    } else {
        "todo"
    }
}

/// The values a step's clauses and actions read: the draft.
fn values(f: &Flow) -> Values {
    f.values.clone()
}

/// The wizard's page in the Settings app, while it runs.
pub(super) fn page(ui: &mut Ui, app: &AppHandle, admin: &Admin) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let tree = admin.tree(app);
    let Some(run) = admin.running() else {
        return;
    };
    let Some(w) = tree.wizard(&run.flow.wizard) else {
        admin.end_wizard();
        return;
    };
    let f = &run.flow;
    let base = app.when_context();
    let ctx2 = ctx.clone();
    let say = move |field: &Field, c: Check| check_text(&ctx2, field, c);
    let hooks = admin.hooks();
    let j = Judge {
        base: base.clone(),
        hooks: &hooks,
        say: &say,
    };
    let shown = f.shown(w, &base);
    let index = f.index(w, &base);
    let blocked: Vec<bool> = shown.iter().map(|s| f.blocked(s, &j).is_some()).collect();
    let step = w.step_of(&f.at);
    let problems = step.map(|s| f.problems(s, &j)).unwrap_or_default();
    let blocking = problems
        .iter()
        .find(|p| p.tone == Tone::Danger)
        .map(|p| p.message.clone());
    let can_finish = f.first_blocked(w, &j).is_none();
    drop(hooks);
    let last = !w.summary && index + 1 >= shown.len();
    let finishing = f.on_summary() || last;
    // The keys: with no dialog over the page and no key captured.
    let free = app
        .service(&crate::dialogs::DIALOGS)
        .is_none_or(|d| d.current().is_none())
        && !app.keys_captured();
    if free {
        // Enter is Next's (the default button, as in a dialog) unless a
        // text edit takes it or a list is open; Space presses a focused
        // button.
        let enter = !egui::Popup::is_any_open(&ctx)
            && ctx
                .memory(|m| m.focused())
                .is_none_or(|id| egui::text_edit::TextEditState::load(&ctx, id).is_none());
        let (cancel, next, back) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::Escape),
                i.consume_key(Modifiers::ALT, Key::N)
                    || (enter && i.consume_key(Modifiers::NONE, Key::Enter)),
                i.consume_key(Modifiers::ALT, Key::B),
            )
        });
        if cancel {
            app.run(CMD_WIZARD_CANCEL, json!({}));
        } else if next {
            app.run(
                if finishing {
                    CMD_WIZARD_FINISH
                } else {
                    CMD_WIZARD_NEXT
                },
                json!({}),
            );
        } else if back && index > 0 {
            app.run(CMD_WIZARD_BACK, json!({}));
        }
    }
    // The title, and where the flow stands.
    let count = shown.len() + usize::from(w.summary);
    let here = if f.on_summary() {
        strings::tr(&ctx, &txt::ADMIN_WIZARD_SUMMARY)
    } else {
        step.map_or_else(String::new, |s| s.meta.title.clone())
    };
    let position = strings::tr_args(
        &ctx,
        &txt::ADMIN_WIZARD_STEP_OF,
        &[
            ("n", &(index + 1).to_string()),
            ("count", &count.to_string()),
            ("title", &here),
        ],
    );
    ui.horizontal(|ui| {
        ui.heading(&w.meta.title);
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.weak(&position);
        });
    });
    if !w.meta.description.is_empty() {
        ui.weak(&w.meta.description);
    }
    ui.separator();
    let footer = ui.spacing().interact_size.y + t.space.m * 2.0;
    let body = (ui.available_height() - footer).max(0.0);
    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        egui::vec2(width, body),
        egui::Layout::left_to_right(Align::Min),
        |ui| {
            ui.set_min_height(body);
            // The stepper.
            let nav = ui
                .vertical(|ui| {
                    ui.set_width(220.0);
                    for (i, s) in shown.iter().enumerate() {
                        let state = step_state(f, &s.meta.id, i, index, &blocked);
                        stepper_row(ui, app, i, count, &s.meta.title, &s.meta.id, state);
                    }
                    if w.summary {
                        let state = if f.on_summary() {
                            "current"
                        } else if can_finish {
                            "todo"
                        } else {
                            "later"
                        };
                        let title = strings::tr(&ctx, &txt::ADMIN_WIZARD_SUMMARY);
                        stepper_row(ui, app, shown.len(), count, &title, SUMMARY, state);
                    }
                })
                .response;
            a11y::name(
                &nav,
                Role::List,
                &strings::tr(&ctx, &txt::ADMIN_WIZARD_STEPS),
            );
            ui.separator();
            let width = ui.available_width();
            egui::ScrollArea::vertical()
                .id_salt(("egui_pop.admin.wizard", &w.meta.id, &f.at))
                .auto_shrink([false, false])
                .max_height(body)
                .show(ui, |ui| {
                    let width = (width - ui.spacing().scroll.allocated_width()).max(0.0);
                    ui.set_max_width(width);
                    ui.vertical(|ui| {
                        ui.set_max_width(width);
                        match step {
                            Some(s) => step_page(ui, app, admin, w, f, s, &problems),
                            None => summary_page(ui, app, w, f, &base),
                        }
                    });
                });
        },
    );
    ui.separator();
    // The footer: Cancel on the left; Next or Finish, Skip, Back on the
    // right, the reason Next is greyed beside it. Toasts keep off it:
    // Finish sits in their default corner, and one over it takes its
    // click.
    let footer = ui.horizontal(|ui| {
        let r = ui.button(strings::tr(&ctx, &txt::ADMIN_WIZARD_CANCEL));
        if r.clicked() {
            app.run(CMD_WIZARD_CANCEL, json!({}));
        }
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            let (label, go, on) = if finishing {
                let verb = if w.finish.verb.is_empty() {
                    strings::tr(&ctx, &txt::ADMIN_WIZARD_FINISH)
                } else {
                    w.finish.verb.clone()
                };
                (verb, CMD_WIZARD_FINISH, can_finish)
            } else {
                (
                    strings::tr(&ctx, &txt::ADMIN_WIZARD_NEXT),
                    CMD_WIZARD_NEXT,
                    blocking.is_none(),
                )
            };
            let r = ui.add_enabled(on, egui::Button::new(&label).selected(true));
            if r.clicked() {
                app.run(go, json!({}));
            }
            let reason = blocking.clone().or_else(|| run.refused.clone());
            if !on && let Some(reason) = &reason {
                r.on_disabled_hover_text(reason);
            }
            if step.is_some_and(|s| s.optional) {
                let r = ui.button(strings::tr(&ctx, &txt::ADMIN_WIZARD_SKIP));
                if r.clicked() {
                    app.run(CMD_WIZARD_SKIP, json!({}));
                }
            }
            if index > 0 {
                let r = ui.button(strings::tr(&ctx, &txt::ADMIN_WIZARD_BACK));
                if r.clicked() {
                    app.run(CMD_WIZARD_BACK, json!({}));
                }
            }
            if let Some(reason) = reason {
                ui.add(Indicator::text(reason).tone(egui_pop_core::kit::Tone::Danger));
            }
        });
    });
    crate::toasts::keep_clear(&ctx, footer.response.rect);
}

/// A step in the stepper: its number and title, selected while shown,
/// greyed while a step before it is not valid; said "Step 2 of 4,
/// Destination, done".
fn stepper_row(
    ui: &mut Ui,
    app: &AppHandle,
    i: usize,
    count: usize,
    title: &str,
    id: &str,
    state: &str,
) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let text = format!("{}. {title}", i + 1);
    let rich = match state {
        "done" => egui::RichText::new(text).color(t.status.success),
        _ => egui::RichText::new(text),
    };
    let r = ui.add_enabled(
        state != "later",
        widgets::SelectableRow::new(state == "current", rich),
    );
    let mut name = strings::tr_args(
        &ctx,
        &txt::ADMIN_WIZARD_STEP_OF,
        &[
            ("n", &(i + 1).to_string()),
            ("count", &count.to_string()),
            ("title", title),
        ],
    );
    match state {
        "done" => name = format!("{name}, {}", strings::tr(&ctx, &txt::ADMIN_WIZARD_DONE)),
        "later" => name = format!("{name}, {}", strings::tr(&ctx, &txt::ADMIN_WIZARD_LATER)),
        _ => {}
    }
    let selected = state == "current";
    let enabled = state != "later";
    r.widget_info(move || {
        let mut info = egui::WidgetInfo::labeled(Role::ListItem, enabled, &name);
        info.selected = Some(selected);
        info
    });
    if r.clicked() && state != "current" {
        app.run(CMD_WIZARD_GO, json!({ "step": id }));
    }
}

/// A step's page: its title and description, its groups in inset boxes,
/// its draft fields with their problems under them, the step's own
/// problems at the end.
#[allow(clippy::too_many_arguments)]
fn step_page(
    ui: &mut Ui,
    app: &AppHandle,
    admin: &Admin,
    w: &Wizard,
    f: &Flow,
    s: &Step,
    problems: &[Problem],
) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    ui.horizontal(|ui| {
        ui.heading(&s.meta.title);
        if s.optional {
            ui.weak(strings::tr(&ctx, &txt::ADMIN_WIZARD_OPTIONAL));
        }
    });
    if !s.meta.description.is_empty() {
        ui.weak(&s.meta.description);
    }
    ui.add_space(t.space.s);
    let entries = app.settings();
    let values = values(f);
    let cx = f.context(&app.when_context());
    let mut changed = Values::new();
    for g in &s.groups {
        if !holds(g.meta.when.as_deref(), &cx) {
            continue;
        }
        if !g.meta.title.is_empty() {
            ui.strong(&g.meta.title);
        }
        shell::inset(ui, |ui| {
            for item in &g.items {
                let meta = item.meta();
                if !holds(meta.when.as_deref(), &cx) {
                    continue;
                }
                let on = holds(meta.enabled.as_deref(), &cx);
                let r = ui
                    .add_enabled_ui(on, |ui| match item {
                        Item::Field(field) if field.draft_key().is_some() => {
                            let key = field.draft_key().unwrap_or_default();
                            let v = f.values.get(key).cloned().unwrap_or(Json::Null);
                            if let Some(new) = draft_row(ui, w, field, &v) {
                                changed.insert(key.to_string(), new);
                            }
                            for p in problems.iter().filter(|p| p.field == key) {
                                problem(ui, p);
                            }
                        }
                        other => shell::item_row(ui, app, admin, &entries, other, &values),
                    })
                    .response;
                if !on && let Some(reason) = &meta.reason {
                    r.on_disabled_hover_text(reason);
                }
            }
        });
        ui.add_space(t.space.m);
    }
    // The step's own problems (from its hook, or on no field shown here).
    let keys: Vec<&str> = s.fields().filter_map(Field::draft_key).collect();
    for p in problems
        .iter()
        .filter(|p| !keys.contains(&p.field.as_str()))
    {
        problem(ui, p);
    }
    if !changed.is_empty() {
        let _ = admin.wizard_set(app, changed);
    }
}

/// A problem, under its field or at the end of the step.
fn problem(ui: &mut Ui, p: &Problem) {
    ui.add(Indicator::text(&p.message).tone(shell::kit_tone(p.tone)));
}

/// A draft field's row: its label and control (a toggle, a number, a
/// choice, a text), its help under it. The new value when it changed.
fn draft_row(ui: &mut Ui, w: &Wizard, f: &Field, value: &Json) -> Option<Json> {
    let title = f.meta.title.as_str();
    let id = egui::Id::unique(("egui_pop.admin.wizard.field", &w.meta.id, &f.meta.id));
    let kind = f.kind.unwrap_or(FieldKind::Text);
    let mut out = None;
    ui.horizontal_wrapped(|ui| match kind {
        FieldKind::Toggle => {
            let mut on = value.as_bool().unwrap_or(false);
            if widgets::checkbox(ui, &mut on, title).changed() {
                out = Some(Json::Bool(on));
            }
        }
        FieldKind::Number => {
            ui.label(title);
            let before = value.as_f64().unwrap_or(0.0);
            let mut n = before;
            // A value out of range stays as it is (its problem says so),
            // never clamped behind the person's back.
            let mut drag = egui::DragValue::new(&mut n).clamp_existing_to_range(false);
            if let (Some(min), Some(max)) = (f.input.min, f.input.max) {
                drag = drag.range(min..=max);
            }
            let r = ui.add(drag);
            a11y::name(&r, Role::SpinButton, title);
            if r.changed() && n != before {
                out = Some(if n.fract() == 0.0 && n.abs() < 1e15 {
                    json!(n as i64)
                } else {
                    json!(n)
                });
            }
        }
        FieldKind::Choice => {
            ui.label(title);
            let now = f.shown(value);
            let mut picked = None;
            let r = ui
                .combo_box(widgets::combo_box(id).selected_text(now), |ui| {
                    for c in &f.input.choices {
                        if ui
                            .add(widgets::SelectableRow::new(c.value == *value, &c.label))
                            .clicked()
                        {
                            picked = Some(c.value.clone());
                        }
                    }
                })
                .response;
            a11y::name(&r, Role::ComboBox, title);
            out = picked.filter(|p| p != value);
        }
        _ => {
            ui.label(title);
            let mut text = cell_text(value);
            let edit = if kind == FieldKind::LongText {
                egui::TextEdit::multiline(&mut text)
            } else {
                egui::TextEdit::singleline(&mut text)
            };
            let r = ui.add(edit.id(id).desired_width(260.0));
            a11y::name(&r, Role::TextInput, title);
            if r.changed() {
                out = Some(Json::String(text));
            }
        }
    });
    if !f.help.is_empty() {
        ui.weak(&f.help);
    }
    out
}

/// The summary: each step's fields with their values as shown, and
/// "Change" going back to the step.
fn summary_page(
    ui: &mut Ui,
    app: &AppHandle,
    w: &Wizard,
    f: &Flow,
    base: &egui_pop_model::when::WhenContext,
) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    ui.heading(strings::tr(&ctx, &txt::ADMIN_WIZARD_SUMMARY));
    ui.weak(strings::tr(&ctx, &txt::ADMIN_WIZARD_SUMMARY_ABOUT));
    ui.add_space(t.space.s);
    for (s, rows) in f.summary(w, base) {
        ui.horizontal(|ui| {
            ui.strong(&s.meta.title);
            let r = ui.small_button(strings::tr(&ctx, &txt::ADMIN_WIZARD_CHANGE));
            a11y::name(
                &r,
                Role::Button,
                &strings::tr_args(
                    &ctx,
                    &txt::ADMIN_WIZARD_CHANGE_NAME,
                    &[("step", &s.meta.title)],
                ),
            );
            if r.clicked() {
                app.run(CMD_WIZARD_GO, json!({ "step": s.meta.id }));
            }
        });
        shell::inset(ui, |ui| {
            if rows.is_empty() {
                ui.weak(strings::tr(&ctx, &txt::ADMIN_WIZARD_NOT_SET));
            }
            for (field, v) in rows {
                let shown = match v {
                    Json::Bool(true) => strings::tr(&ctx, &txt::ADMIN_WIZARD_YES),
                    Json::Bool(false) => strings::tr(&ctx, &txt::ADMIN_WIZARD_NO),
                    Json::Null => strings::tr(&ctx, &txt::ADMIN_WIZARD_NOT_SET),
                    Json::String(ref s) if s.trim().is_empty() => {
                        strings::tr(&ctx, &txt::ADMIN_WIZARD_NOT_SET)
                    }
                    ref other => field.shown(other),
                };
                let r = ui
                    .horizontal_wrapped(|ui| {
                        ui.label(&field.meta.title);
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            ui.strong(&shown);
                        });
                    })
                    .response;
                a11y::name(&r, Role::Group, &format!("{}: {shown}", field.meta.title));
            }
        });
        ui.add_space(t.space.m);
    }
}

/// The commands: open, set, next, back, skip, go, finish, cancel.
pub(super) fn commands(ctx: &mut crate::Ctx<'_>) -> Result<(), crate::PluginError> {
    use egui_pop_model::commands::CommandSpec;
    fn admin(call: &crate::app::Call<'_>) -> Result<std::sync::Arc<Admin>, String> {
        call.app.service(&ADMIN).ok_or_else(|| "no admin".into())
    }
    fn values_arg(args: &Json) -> Values {
        let mut v: Values = args
            .get("values")
            .and_then(Json::as_object)
            .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        if let Some(key) = args.get("key").and_then(Json::as_str) {
            v.insert(key.into(), args.get("value").cloned().unwrap_or(Json::Null));
        }
        v
    }
    ctx.command(
        CommandSpec::new(CMD_WIZARD_OPEN, "Open a wizard", "File")
            .hidden()
            .args(json!({ "id": "", "step": "", "values": {} })),
        |call| {
            let id = call
                .args
                .get("id")
                .and_then(Json::as_str)
                .ok_or("`id` is missing")?;
            let step = call
                .args
                .get("step")
                .and_then(Json::as_str)
                .filter(|s| !s.is_empty());
            admin(call)?.open_wizard(call.app, id, step, values_arg(call.args))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_WIZARD_SET, "Set a wizard's values", "File")
            .hidden()
            .args(json!({ "key": "", "value": null, "values": {} })),
        |call| admin(call)?.wizard_set(call.app, values_arg(call.args)),
    )?;
    ctx.command(
        CommandSpec::new(CMD_WIZARD_NEXT, "Wizard: next step", "File").hidden(),
        |call| admin(call)?.wizard_next(call.app),
    )?;
    ctx.command(
        CommandSpec::new(CMD_WIZARD_BACK, "Wizard: previous step", "File").hidden(),
        |call| admin(call)?.wizard_back(call.app),
    )?;
    ctx.command(
        CommandSpec::new(CMD_WIZARD_SKIP, "Wizard: skip this step", "File").hidden(),
        |call| admin(call)?.wizard_skip(call.app),
    )?;
    ctx.command(
        CommandSpec::new(CMD_WIZARD_GO, "Wizard: go to a step", "File")
            .hidden()
            .args(json!({ "step": "" })),
        |call| {
            let step = call
                .args
                .get("step")
                .and_then(Json::as_str)
                .ok_or("`step` is missing")?;
            admin(call)?.wizard_go(call.app, step)
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_WIZARD_FINISH, "Wizard: finish", "File").hidden(),
        |call| admin(call)?.wizard_finish(call.app),
    )?;
    ctx.command(
        CommandSpec::new(CMD_WIZARD_CANCEL, "Wizard: cancel", "File")
            .hidden()
            .args(json!({ "force": false })),
        |call| {
            let force = call
                .args
                .get("force")
                .and_then(Json::as_bool)
                .unwrap_or(false);
            admin(call)?.wizard_cancel(call.app, force)
        },
    )?;
    Ok(())
}
