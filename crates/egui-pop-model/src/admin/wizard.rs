//! The wizard kit (`docs/design/admin-forms.md`, section 5.3): a
//! multi-step flow declared as data, each step a form built with the same
//! groups and items as a section's page, its values a **draft** handed to
//! a command on finish.
//!
//! - **Steps**: declared in order; a step whose `when` does not hold
//!   (read against the draft as `form.<key>`) does not exist for now, and
//!   its values are not handed on.
//! - **Validation per step**: each field's own checks ([`Field::check`]:
//!   required, range, choices), its `validate` hook, then the step's; a
//!   problem of the danger tone blocks Next with its message, a warning
//!   is shown and lets through.
//! - **Navigation**: Next, Back, Skip (an optional step: its values go
//!   back to their defaults), and going to any step whose steps before it
//!   are valid; one that is not lands on the first invalid one, with its
//!   reason.
//! - **Summary**: after the last step (unless declared without), every
//!   step's fields with their values as shown.
//! - **Finish**: the command, with the template filled from the draft
//!   (or the draft itself when the wizard declares no arguments).
//!
//! [`Flow`] is the state of a wizard being filled: pure, no egui, so an
//! agent and a test drive it as the shell does.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};

use super::hooks::{Hooks, Problem, Values};
use super::{
    Confirmation, Field, Group, Item, Meta, Section, SectionBuilder, Tone, bad_clauses,
    form_context, holds, template,
};
use crate::diagnostics::Diagnostics;
use crate::when::{When, WhenContext};

/// Where a flow stands after its last step: the summary.
pub const SUMMARY: &str = "summary";

/// What a field's own checks found ([`Field::check`]); the shell words
/// it in the interface's language.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Check {
    /// Required and left empty.
    Required,
    /// Below its minimum.
    Below(f64),
    /// Above its maximum.
    Above(f64),
    /// Not one of its choices.
    NotAChoice,
}

/// A step: a form (groups of fields, statuses, callouts, records), its
/// `validate` hook, whether it may be skipped.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Step {
    #[serde(flatten)]
    pub meta: Meta,
    #[serde(default)]
    pub groups: Vec<Group>,
    /// A `validate` hook run over the draft when the step is left.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validate: Option<String>,
    /// Skip is offered.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
}

impl Step {
    /// Its fields, every group's.
    pub fn fields(&self) -> impl Iterator<Item = &Field> {
        self.groups
            .iter()
            .flat_map(|g| &g.items)
            .filter_map(|i| match i {
                Item::Field(f) => Some(f),
                _ => None,
            })
    }

    /// Its fields shown for these values: in a group and with a `when`
    /// that hold.
    pub fn shown_fields(&self, cx: &WhenContext) -> Vec<&Field> {
        self.groups
            .iter()
            .filter(|g| holds(g.meta.when.as_deref(), cx))
            .flat_map(|g| &g.items)
            .filter_map(|i| match i {
                Item::Field(f) if holds(f.meta.when.as_deref(), cx) => Some(f),
                _ => None,
            })
            .collect()
    }
}

/// What finishing runs: a command, its arguments' template (`${key}`;
/// `null`: the draft itself, as an object), and the button's verb.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Finish {
    pub command: String,
    #[serde(default, skip_serializing_if = "Json::is_null")]
    pub args: Json,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub verb: String,
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

/// A wizard: its steps, its summary, what finishing runs, and what
/// cancelling asks when something was entered.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wizard {
    #[serde(flatten)]
    pub meta: Meta,
    pub steps: Vec<Step>,
    /// A summary step after the last one.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub summary: bool,
    pub finish: Finish,
    /// What cancelling asks once something was entered (the shell's own
    /// words otherwise).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel: Option<Confirmation>,
}

impl Wizard {
    /// A wizard whose finish runs `command`.
    pub fn new(id: &str, title: &str, command: &str) -> Wizard {
        Wizard {
            meta: Meta::new(id, title),
            steps: Vec::new(),
            summary: true,
            finish: Finish {
                command: command.into(),
                args: Json::Null,
                verb: String::new(),
            },
            cancel: None,
        }
    }

    pub fn description(mut self, text: &str) -> Wizard {
        self.meta.description = text.into();
        self
    }

    pub fn icon(mut self, icon: &str) -> Wizard {
        self.meta.icon = Some(icon.into());
        self
    }

    /// A step, built as a section is (groups, fields, statuses...): its
    /// draft fields are [`Field::draft`]s of this wizard.
    pub fn step(
        mut self,
        id: &str,
        title: &str,
        build: impl FnOnce(&mut SectionBuilder<'_>),
    ) -> Wizard {
        let mut section = Section {
            meta: Meta {
                order: self.steps.len() as i32,
                ..Meta::new(id, title)
            },
            scope: self.meta.id.clone(),
            parent: None,
            groups: Vec::new(),
        };
        build(&mut SectionBuilder {
            section: &mut section,
        });
        self.steps.push(Step {
            meta: section.meta,
            groups: section.groups,
            validate: None,
            optional: false,
        });
        self
    }

    /// The last step's `validate` hook.
    pub fn validate(mut self, hook: &str) -> Wizard {
        if let Some(s) = self.steps.last_mut() {
            s.validate = Some(hook.into());
        }
        self
    }

    /// The last step exists only when the clause holds.
    pub fn when(mut self, clause: &str) -> Wizard {
        if let Some(s) = self.steps.last_mut() {
            s.meta.when = Some(clause.into());
        }
        self
    }

    /// The last step may be skipped.
    pub fn optional(mut self) -> Wizard {
        if let Some(s) = self.steps.last_mut() {
            s.optional = true;
        }
        self
    }

    /// The finish's arguments: a template over the draft (`${key}`).
    pub fn args(mut self, args: Json) -> Wizard {
        self.finish.args = args;
        self
    }

    /// The finish button's verb ("Set up backups").
    pub fn verb(mut self, verb: &str) -> Wizard {
        self.finish.verb = verb.into();
        self
    }

    /// No summary: the last step finishes.
    pub fn no_summary(mut self) -> Wizard {
        self.summary = false;
        self
    }

    /// What cancelling asks once something was entered.
    pub fn cancel(mut self, confirmation: Confirmation) -> Wizard {
        self.cancel = Some(confirmation);
        self
    }

    pub fn step_of(&self, id: &str) -> Option<&Step> {
        self.steps.iter().find(|s| s.meta.id == id)
    }

    /// Every draft field, by key, with its step.
    pub fn fields(&self) -> impl Iterator<Item = (&Step, &Field)> {
        self.steps
            .iter()
            .flat_map(|s| s.fields().map(move |f| (s, f)))
            .filter(|(_, f)| f.draft_key().is_some())
    }

    /// The draft at the start: each draft field's default.
    pub fn defaults(&self) -> Values {
        self.fields()
            .filter_map(|(_, f)| {
                f.draft_key()
                    .map(|k| (k.to_string(), f.input.default.clone()))
            })
            .collect()
    }

    /// Checks it: steps there, their ids unique, their clauses parse, the
    /// settings its fields bind exist. What fails is reported and left
    /// out.
    pub(crate) fn check(&mut self, d: &mut Diagnostics, setting_exists: &dyn Fn(&str) -> bool) {
        let at = format!("wizard/{}", self.meta.id);
        let mut ids: Vec<String> = Vec::new();
        self.steps.retain_mut(|s| {
            if s.meta.id == SUMMARY || ids.contains(&s.meta.id) {
                d.warn(
                    "admin-duplicate",
                    None,
                    format!("{at}/{}", s.meta.id),
                    "a step with this id is already declared (or is `summary`): left out",
                );
                return false;
            }
            ids.push(s.meta.id.clone());
            bad_clauses(d, &at, &mut s.meta);
            for g in &mut s.groups {
                bad_clauses(d, &at, &mut g.meta);
                g.items.retain(|item| {
                    for clause in [&item.meta().when, &item.meta().enabled]
                        .into_iter()
                        .flatten()
                    {
                        if let Err(e) = When::parse(clause) {
                            d.warn(
                                "admin-bad-clause",
                                None,
                                format!("{at}/{}", s.meta.id),
                                format!("`{clause}` does not parse ({e}): it always holds"),
                            );
                        }
                    }
                    if let Item::Field(f) = item
                        && let Some(setting) = f.setting_id()
                        && !setting_exists(setting)
                    {
                        d.warn(
                            "admin-no-setting",
                            None,
                            format!("{at}/{}", s.meta.id),
                            format!("no setting `{setting}`: left out"),
                        );
                        return false;
                    }
                    true
                });
            }
            true
        });
        if self.steps.is_empty() {
            d.warn("admin-wizard-empty", None, &at, "a wizard with no step");
        }
    }
}

/// What a flow needs to judge its steps: the application's context keys,
/// the hooks, and the words of a field's own checks.
pub struct Judge<'a> {
    pub base: WhenContext,
    pub hooks: &'a Hooks,
    pub say: &'a dyn Fn(&Field, Check) -> String,
}

impl std::fmt::Debug for Judge<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Judge")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

/// A wizard being filled: the step shown, the draft, the steps passed.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Flow {
    pub wizard: String,
    /// A step's id, or [`SUMMARY`].
    pub at: String,
    pub values: Values,
    /// The draft at the start: what Cancel compares with.
    pub initial: Values,
    /// The steps left by Next or Skip (the stepper marks them done).
    pub passed: Vec<String>,
}

impl Flow {
    /// A flow at its first step, its draft the defaults with `values`
    /// over them.
    pub fn start(w: &Wizard, base: &WhenContext, values: Values) -> Flow {
        let mut initial = w.defaults();
        initial.extend(values);
        let mut f = Flow {
            wizard: w.meta.id.clone(),
            at: String::new(),
            values: initial.clone(),
            initial,
            passed: Vec::new(),
        };
        f.at = f
            .shown(w, base)
            .first()
            .map_or_else(|| SUMMARY.to_string(), |s| s.meta.id.clone());
        f
    }

    /// The context its clauses read: the application's, and the draft as
    /// `form.<key>`.
    pub fn context(&self, base: &WhenContext) -> WhenContext {
        form_context(base.clone(), &self.values)
    }

    /// The steps that exist for the draft as it stands.
    pub fn shown<'a>(&self, w: &'a Wizard, base: &WhenContext) -> Vec<&'a Step> {
        let cx = self.context(base);
        w.steps
            .iter()
            .filter(|s| holds(s.meta.when.as_deref(), &cx))
            .collect()
    }

    /// Where it stands among the steps shown (the summary after them).
    pub fn index(&self, w: &Wizard, base: &WhenContext) -> usize {
        let shown = self.shown(w, base);
        shown
            .iter()
            .position(|s| s.meta.id == self.at)
            .unwrap_or(shown.len())
    }

    pub fn on_summary(&self) -> bool {
        self.at == SUMMARY
    }

    /// Whether anything was entered.
    pub fn dirty(&self) -> bool {
        self.values != self.initial
    }

    /// Sets a draft value; refused for a key no field of the wizard has.
    pub fn set(&mut self, w: &Wizard, key: &str, value: Json) -> Result<(), String> {
        if !w.fields().any(|(_, f)| f.draft_key() == Some(key)) {
            return Err(format!("no field `{key}` in the wizard `{}`", w.meta.id));
        }
        self.values.insert(key.into(), value);
        Ok(())
    }

    /// A step's problems for the draft as it stands: its fields' own
    /// checks and hooks, then the step's hook.
    pub fn problems(&self, step: &Step, j: &Judge<'_>) -> Vec<Problem> {
        let cx = self.context(&j.base);
        let mut out = Vec::new();
        for f in step.shown_fields(&cx) {
            let Some(key) = f.draft_key() else {
                continue;
            };
            let v = self.values.get(key).cloned().unwrap_or(Json::Null);
            if let Some(c) = f.check(&v) {
                out.push(Problem {
                    field: key.into(),
                    tone: Tone::Danger,
                    message: (j.say)(f, c),
                });
            }
            if let Some(h) = &f.hooks.validate {
                out.extend(j.hooks.validate(h, &self.values));
            }
        }
        if let Some(h) = &step.validate {
            out.extend(j.hooks.validate(h, &self.values));
        }
        out
    }

    /// The first problem that blocks a step (danger), if any.
    pub fn blocked(&self, step: &Step, j: &Judge<'_>) -> Option<Problem> {
        self.problems(step, j)
            .into_iter()
            .find(|p| p.tone == Tone::Danger)
    }

    fn pass(&mut self, id: &str) {
        if !self.passed.iter().any(|p| p == id) {
            self.passed.push(id.into());
        }
    }

    /// The step after the one shown, or the summary; `None` past the last
    /// step of a wizard with no summary.
    fn after(&self, w: &Wizard, base: &WhenContext) -> Option<String> {
        let shown = self.shown(w, base);
        let i = shown.iter().position(|s| s.meta.id == self.at)?;
        match shown.get(i + 1) {
            Some(s) => Some(s.meta.id.clone()),
            None => w.summary.then(|| SUMMARY.to_string()),
        }
    }

    /// Next: the step shown checked, then the one after it. Refused with
    /// the first blocking problem's message, or on the last step.
    pub fn next(&mut self, w: &Wizard, j: &Judge<'_>) -> Result<(), String> {
        if self.on_summary() {
            return Err("this is the summary: finish".into());
        }
        let step = w
            .step_of(&self.at)
            .ok_or_else(|| format!("no step `{}`", self.at))?;
        if let Some(p) = self.blocked(step, j) {
            return Err(p.message);
        }
        let to = self
            .after(w, &j.base)
            .ok_or("this is the last step: finish")?;
        let at = self.at.clone();
        self.pass(&at);
        self.at = to;
        Ok(())
    }

    /// Skip: an optional step's values back to their defaults, then the
    /// step after it.
    pub fn skip(&mut self, w: &Wizard, base: &WhenContext) -> Result<(), String> {
        let step = w
            .step_of(&self.at)
            .filter(|s| s.optional)
            .ok_or("this step may not be skipped")?;
        let defaults = w.defaults();
        for f in step.fields() {
            if let Some(k) = f.draft_key() {
                match self.initial.get(k).or_else(|| defaults.get(k)) {
                    Some(v) => self.values.insert(k.into(), v.clone()),
                    None => self.values.remove(k),
                };
            }
        }
        let to = self.after(w, base).ok_or("this is the last step: finish")?;
        let at = self.at.clone();
        self.pass(&at);
        self.at = to;
        Ok(())
    }

    /// Back: the step before (the last one from the summary).
    pub fn back(&mut self, w: &Wizard, base: &WhenContext) -> Result<(), String> {
        let shown = self.shown(w, base);
        let i = self.index(w, base);
        if i == 0 {
            return Err("this is the first step".into());
        }
        self.at = shown[i - 1].meta.id.clone();
        Ok(())
    }

    /// Goes to a step (or the summary): every step before it must be
    /// valid, else it lands on the first one that is not and says why.
    pub fn go(&mut self, w: &Wizard, j: &Judge<'_>, to: &str) -> Result<(), String> {
        let shown = self.shown(w, &j.base);
        let end = if to == SUMMARY {
            if !w.summary {
                return Err("this wizard has no summary".into());
            }
            shown.len()
        } else {
            shown
                .iter()
                .position(|s| s.meta.id == to)
                .ok_or_else(|| format!("no step `{to}` now"))?
        };
        for s in &shown[..end] {
            if let Some(p) = self.blocked(s, j) {
                self.at = s.meta.id.clone();
                return Err(p.message);
            }
        }
        for s in &shown[..end] {
            self.pass(&s.meta.id);
        }
        self.at = to.into();
        Ok(())
    }

    /// Whether every step shown is valid: the summary may be reached and
    /// the wizard finished.
    pub fn first_blocked(&self, w: &Wizard, j: &Judge<'_>) -> Option<(String, Problem)> {
        self.shown(w, &j.base)
            .into_iter()
            .find_map(|s| self.blocked(s, j).map(|p| (s.meta.id.clone(), p)))
    }

    /// The draft handed on: the values of the fields shown in the steps
    /// shown (a step or field whose `when` does not hold hands nothing).
    pub fn draft(&self, w: &Wizard, base: &WhenContext) -> Values {
        let cx = self.context(base);
        self.shown(w, base)
            .into_iter()
            .flat_map(|s| s.shown_fields(&cx))
            .filter_map(|f| f.draft_key())
            .filter_map(|k| self.values.get(k).map(|v| (k.to_string(), v.clone())))
            .collect()
    }

    /// Finishing: the command and its arguments, once every step shown is
    /// valid; else the first invalid step and its reason. In the
    /// arguments' template, a field of a step not shown is `null`.
    pub fn finish(&self, w: &Wizard, j: &Judge<'_>) -> Result<(String, Json), (String, String)> {
        if let Some((step, p)) = self.first_blocked(w, j) {
            return Err((step, p.message));
        }
        let draft = self.draft(w, &j.base);
        let args = if w.finish.args.is_null() {
            Json::Object(draft.into_iter().collect::<Map<String, Json>>())
        } else {
            // A field of a step not shown is null, not its `${key}`.
            let mut all = draft;
            for (_, f) in w.fields() {
                if let Some(k) = f.draft_key() {
                    all.entry(k.to_string()).or_insert(Json::Null);
                }
            }
            template::expand(&w.finish.args, &all).0
        };
        Ok((w.finish.command.clone(), args))
    }

    /// The summary: each step shown, its fields shown and their values.
    pub fn summary<'a>(
        &self,
        w: &'a Wizard,
        base: &WhenContext,
    ) -> Vec<(&'a Step, Vec<(&'a Field, Json)>)> {
        let cx = self.context(base);
        self.shown(w, base)
            .into_iter()
            .map(|s| {
                let rows = s
                    .shown_fields(&cx)
                    .into_iter()
                    .filter_map(|f| {
                        f.draft_key()
                            .map(|k| (f, self.values.get(k).cloned().unwrap_or(Json::Null)))
                    })
                    .collect();
                (s, rows)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::hooks::Hook;
    use super::super::{FORMAT, FieldKind, Manifest};
    use super::*;
    use serde_json::json;

    fn trip() -> Wizard {
        Wizard::new("trip", "Plan a trip", "trip.book")
            .step("where", "Where", |s| {
                s.field(
                    Field::draft("trip", "city", "City", FieldKind::Text)
                        .required()
                        .default_value(json!("")),
                )
                .field(
                    Field::draft("trip", "abroad", "Abroad", FieldKind::Toggle)
                        .default_value(json!(false)),
                );
            })
            .step("passport", "Passport", |s| {
                s.field(
                    Field::draft("trip", "passport", "Passport number", FieldKind::Text).required(),
                );
            })
            .when("form.abroad")
            .step("nights", "Nights", |s| {
                s.field(
                    Field::draft("trip", "nights", "Nights", FieldKind::Number)
                        .range(1.0, 30.0)
                        .default_value(json!(2)),
                );
            })
            .validate("trip.long")
            .step("extras", "Extras", |s| {
                s.field(
                    Field::draft("trip", "room", "Room", FieldKind::Choice)
                        .choice(json!("single"), "Single room")
                        .choice(json!("double"), "Double room")
                        .default_value(json!("single")),
                );
            })
            .optional()
            .args(json!({ "to": "${city}", "nights": "${nights}", "room": "${room}" }))
            .verb("Book")
    }

    fn hooks() -> Hooks {
        let mut h = Hooks::new();
        h.register(
            "trip.long",
            Hook::Validate(Box::new(|v| {
                let n = v.get("nights").and_then(Json::as_f64).unwrap_or(0.0);
                if n > 14.0 {
                    vec![Problem {
                        field: "nights".into(),
                        tone: Tone::Warning,
                        message: "A long trip.".into(),
                    }]
                } else {
                    Vec::new()
                }
            })),
        );
        h
    }

    fn say(f: &Field, c: Check) -> String {
        format!("{}: {c:?}", f.meta.title)
    }

    #[test]
    fn a_wizard_round_trips_through_a_manifest() {
        let m = Manifest::new(FORMAT).wizard(trip());
        let json = m.to_json();
        assert_eq!(json["wizards"][0]["steps"][1]["when"], "form.abroad");
        assert_eq!(json["wizards"][0]["steps"][3]["optional"], true);
        assert_eq!(Manifest::from_json(&json).unwrap(), m);
    }

    #[test]
    fn next_is_blocked_with_the_reason_and_conditional_steps_come_and_go() {
        let w = trip();
        let h = hooks();
        let j = Judge {
            base: WhenContext::new(),
            hooks: &h,
            say: &say,
        };
        let mut f = Flow::start(&w, &j.base, Values::new());
        assert_eq!(f.at, "where");
        assert!(!f.dirty());
        // The city is required: Next is refused with its reason.
        assert_eq!(f.next(&w, &j).unwrap_err(), "City: Required");
        f.set(&w, "city", json!("Lyon")).unwrap();
        assert!(f.set(&w, "nowhere", json!(1)).is_err());
        assert!(f.dirty());
        // Not abroad: no passport step.
        assert_eq!(f.shown(&w, &j.base).len(), 3);
        f.next(&w, &j).unwrap();
        assert_eq!(f.at, "nights");
        f.back(&w, &j.base).unwrap();
        f.set(&w, "abroad", json!(true)).unwrap();
        assert_eq!(f.shown(&w, &j.base).len(), 4);
        f.next(&w, &j).unwrap();
        assert_eq!(f.at, "passport");
        // Going ahead lands on the first invalid step, with its reason.
        let e = f.go(&w, &j, SUMMARY).unwrap_err();
        assert_eq!(
            (f.at.as_str(), e.as_str()),
            ("passport", "Passport number: Required")
        );
        f.set(&w, "passport", json!("X1")).unwrap();
        f.next(&w, &j).unwrap();
        // A range, then a warning that lets through.
        f.set(&w, "nights", json!(40)).unwrap();
        assert_eq!(f.next(&w, &j).unwrap_err(), "Nights: Above(30.0)");
        f.set(&w, "nights", json!(20)).unwrap();
        let s = w.step_of("nights").unwrap();
        assert_eq!(f.problems(s, &j)[0].tone, Tone::Warning);
        f.next(&w, &j).unwrap();
        assert_eq!(f.at, "extras");
        // Skipping an optional step takes its values back.
        f.set(&w, "room", json!("double")).unwrap();
        f.skip(&w, &j.base).unwrap();
        assert!(f.on_summary());
        assert_eq!(f.values["room"], json!("single"));
        let summary = f.summary(&w, &j.base);
        assert_eq!(summary.len(), 4);
        assert_eq!(summary[3].1[0].0.shown(&summary[3].1[0].1), "Single room");
        let (cmd, args) = f.finish(&w, &j).unwrap();
        assert_eq!(cmd, "trip.book");
        assert_eq!(
            args,
            json!({ "to": "Lyon", "nights": 20, "room": "single" })
        );
        // Back from the summary: the last step.
        f.back(&w, &j.base).unwrap();
        assert_eq!(f.at, "extras");
        assert_eq!(f.passed, ["where", "passport", "nights", "extras"]);
    }

    #[test]
    fn a_hidden_step_hands_nothing_on_and_the_draft_is_the_default_args() {
        let mut w = trip();
        w.finish.args = Json::Null;
        let h = Hooks::new();
        let j = Judge {
            base: WhenContext::new(),
            hooks: &h,
            say: &say,
        };
        let mut f = Flow::start(&w, &j.base, [("city".to_string(), json!("Oslo"))].into());
        assert!(!f.dirty(), "values given at the start are not an entry");
        f.values.insert("passport".into(), json!("kept out"));
        let (_, args) = f.finish(&w, &j).unwrap();
        assert_eq!(
            args,
            json!({ "city": "Oslo", "abroad": false, "nights": 2, "room": "single" })
        );
        w.finish.args = json!({ "passport": "${passport}" });
        let (_, args) = f.finish(&w, &j).unwrap();
        assert_eq!(args, json!({ "passport": null }), "its step is not shown");
    }

    #[test]
    fn the_check_reports_bad_steps() {
        let mut m = Manifest::new(FORMAT)
            .wizard(
                Wizard::new("w", "W", "x")
                    .step("a", "A", |s| {
                        s.field(Field::setting("missing"));
                    })
                    .step("a", "Again", |_| {})
                    .step("b", "B", |_| {})
                    .when("(("),
            )
            .wizard(Wizard::new("w", "Twice", "x"));
        let d = m.check(&[], &|_| false);
        let codes: Vec<&str> = d.all().iter().map(|x| x.code).collect();
        assert_eq!(
            codes,
            [
                "admin-no-setting",
                "admin-duplicate",
                "admin-bad-clause",
                "admin-duplicate"
            ]
        );
        assert_eq!(m.wizards.len(), 1);
        assert_eq!(m.wizards[0].steps.len(), 2);
        assert!(m.wizards[0].steps[1].meta.when.is_none());
    }
}
