//! Links (ADR 0017): one service opens them, through one policy and one
//! step, with a replaceable opener.
//!
//! - **Requests** ([`Request`], from `egui_pop_model::links`) name the
//!   target, how (`Auto`, `Outside`, `Inside`, `Reveal`), the source
//!   (declared by the application, found in content, asked by an agent),
//!   the text the link was shown under and who asks. [`open`] runs them
//!   through the `link.open` step: a before-hook vetoes or redirects, a
//!   run-hook opens its own way, the journal says `link.opened`,
//!   `link.failed`, `link.asked` or `link.cancelled`.
//! - **The policy** ([`Policy`]) classifies the target and decides how it
//!   opens and whether the user confirms first: declared web and mail
//!   links open at once; a content link asks while its host is not
//!   trusted (the confirmation offers to trust it for the session); a
//!   local file is always confirmed and a program refused, its folder
//!   offered; an agent's request is always confirmed, one at a time per
//!   agent, five more queued, then refused. [`Links`] (the `app.links`
//!   service) holds it, with the opener.
//! - **Openers** ([`Opener`]) do the opening: [`EguiOpener`] (the
//!   default: web pages and mail through egui's `OpenUrl`, eframe's
//!   `links` feature on native, a new tab on the web build),
//!   [`RecordingOpener`] (tests: records each call, answers as
//!   scripted), [`NullOpener`] (a headless node: refuses with the reason,
//!   or forwards). `egui_pop_platform::open::SystemOpener` adds local
//!   files, folders and Reveal.
//! - **egui's own `OpenUrl`** (`ui.hyperlink`, a third-party widget,
//!   `ctx.open_url`) is taken out of each frame's output and opened
//!   through the same step, so no link bypasses the policy.
//! - **The widget** ([`link`], [`Link`]): the theme's link colour, the
//!   real target on hover (an internationalised host with its punycode),
//!   a context menu (Open, Open outside, Show in folder, Copy link),
//!   Enter and Space, Ctrl+click and middle click for outside, the role
//!   `link` with the target as its description.
//! - **Commands**: `links.open` (`target`, `how`, `dry_run`: a dry run
//!   says the kind, how, whether it would ask and the verdict, opening
//!   nothing) and `links.check` for agents, `files.reveal` (`path`), and
//!   the `links` collector; [`lint`] checks the links an application
//!   declares.
//!
//! ```
//! use std::sync::Arc;
//! use egui_pop_api::links::{self, LinksPlugin, RecordingOpener, Request};
//! use egui_pop_api::App;
//!
//! let rec = RecordingOpener::default();
//! let app = App::builder()
//!     .plugin(LinksPlugin::default().opener(Arc::new(rec.clone())))
//!     .build(&egui::Context::default())
//!     .unwrap();
//! let out = links::open(app.handle(), Request::new("https://example.org/"));
//! assert!(out.ok());
//! assert_eq!(rec.records()[0].target, "https://example.org/");
//! ```

pub mod lint;

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use egui::{Response, RichText, Ui};
use egui_pop_core::debug::FnCollector;
use egui_pop_core::lock;
use egui_pop_core::strings;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, OwnerId, PluginId, StepId};
use serde::Serialize;
use serde_json::{Value as Json, json};

use crate::app::AppHandle;
use crate::clipboard::{self, ClipSource, CopyRequest};
use crate::dialogs::{self, Custom, DIALOGS};
use crate::hooks::{Lifecycle, On};
use crate::host::{Pending, source_of};
use crate::services::ServiceToken;
use crate::steps::{Before, Outcome, StepSpec};
use crate::strings as txt;
use crate::toasts::{TOASTS, Toast};
use crate::{Ctx, Plugin, PluginError};

pub use egui_pop_model::links::{
    Caps, Decision, Facts, How, Kind, Policy, Reason, Request, Source, Target,
};

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.links");

/// The links service: the policy and the opener.
pub const LINKS: ServiceToken<Links> = ServiceToken::new("app.links");

/// Opening a link (`{ target, how, source, label, origin, dry_run }`): a
/// before-hook vetoes it or redirects it (another target, another way);
/// a run-hook opens it its own way; its default decides through the
/// policy, asks the user when it must, and has the opener open it. Its
/// result: the decision and the status (`opened`, `opening`, `asked`,
/// `queued`, or a dry run's verdict).
pub const STEP_OPEN: StepId = StepId::from_static("link.open");

/// Opens a link (`target`, `how`, `label`, `dry_run`); an agent's
/// request is always confirmed by the user.
pub const CMD_OPEN: CommandId = CommandId::from_static("links.open");
/// What a link is and how it would open (`target`, `how`, `source`),
/// opening nothing.
pub const CMD_CHECK: CommandId = CommandId::from_static("links.check");
/// Copies a link (`target`), through the clipboard.
pub const CMD_COPY: CommandId = CommandId::from_static("links.copy");
/// Shows a file in its folder (`path`).
pub const CMD_REVEAL: CommandId = CommandId::from_static("files.reveal");
/// The user's answer to a confirmation (`id`, `trust`): kept from
/// agents and the palette.
pub const CMD_CONFIRMED: CommandId = CommandId::from_static("links.confirmed");

/// The clipboard endpoint a link is copied from.
pub const ENDPOINT: &str = "egui_pop.links";

/// How many requests of one agent wait behind its confirmation.
pub const AGENT_QUEUE: usize = 5;
/// How many requests the `links` collector keeps.
pub const RECENT: usize = 32;

const NO_PLUGIN: &str = "no links service: the LinksPlugin is not connected";

/// What an opener answers.
#[derive(Debug)]
pub enum Opening {
    /// Done, or refused with the reason.
    Done(Result<(), String>),
    /// The answer comes later (a portal, a thread).
    Later(Pending),
}

/// One call to an opener: what to open and how, from where, for whom.
#[derive(Clone, Copy, Debug)]
pub struct OpenCall<'a> {
    pub target: &'a Target,
    /// `Outside` or `Reveal` (`Inside` is the application's).
    pub how: How,
    pub kind: Kind,
    pub source: Source,
    pub origin: &'a Origin,
    /// The context of the window it is asked from.
    pub egui: &'a egui::Context,
}

/// What opens a link outside the application, replaceable by the host
/// (ADR 0010): a kiosk opening nothing, a host opening in its own
/// browser, a headless node forwarding to an attached interface.
pub trait Opener: Send + Sync {
    /// Its name, as the `links` collector shows it.
    fn name(&self) -> &str;

    /// What it can do here: the policy refuses the rest with the reason.
    fn caps(&self) -> Caps;

    /// Opens a target the policy approved (and the user confirmed when
    /// asked).
    fn open(&self, call: &OpenCall<'_>) -> Opening;
}

/// The default opener: web pages and `mailto:` through egui's `OpenUrl`
/// (eframe's `links` feature, webbrowser, on native; a new tab on the web
/// build, `mailto:` in the same tab, which hands it to the mail client).
/// Local files, folders and Reveal need the platform's
/// `SystemOpener`.
#[derive(Clone, Copy, Debug, Default)]
pub struct EguiOpener;

impl Opener for EguiOpener {
    fn name(&self) -> &str {
        "egui"
    }

    fn caps(&self) -> Caps {
        Caps::WEB
    }

    fn open(&self, call: &OpenCall<'_>) -> Opening {
        if call.how != How::Outside || call.target.scheme() == "file" {
            return Opening::Done(Err("egui's opener opens web pages and mail only".into()));
        }
        let new_tab = call.target.scheme() != "mailto";
        emit_approved(call.egui, call.target.as_str(), new_tab);
        Opening::Done(Ok(()))
    }
}

/// Emits egui's `OpenUrl` for a link the policy approved: the
/// interceptor lets it through to eframe.
pub fn emit_approved(ctx: &egui::Context, url: &str, new_tab: bool) {
    ctx.with_plugin(|i: &mut Interceptor| {
        i.approved.push(url.to_string());
        if i.approved.len() > 16 {
            i.approved.remove(0);
        }
    });
    ctx.open_url(egui::OpenUrl {
        url: url.to_string(),
        new_tab,
    });
}

/// One call a [`RecordingOpener`] received.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Record {
    pub target: String,
    pub how: How,
    pub kind: Kind,
    pub source: Source,
    /// Who asked (`user`, `agent:<name>`).
    pub origin: String,
}

#[derive(Debug)]
struct Recording {
    records: Vec<Record>,
    answer: Result<(), String>,
    caps: Caps,
}

/// An opener for tests: records each call (target, how, kind, source,
/// origin) and answers as scripted (success by default), with every
/// capability. Clones share the record.
#[derive(Clone, Debug)]
pub struct RecordingOpener(Arc<Mutex<Recording>>);

impl Default for RecordingOpener {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Recording {
            records: Vec::new(),
            answer: Ok(()),
            caps: Caps::ALL,
        })))
    }
}

impl RecordingOpener {
    /// Answers every later call with this failure (`None`: success).
    pub fn fail(&self, reason: Option<&str>) {
        lock(&self.0).answer = reason.map_or(Ok(()), |r| Err(r.to_string()));
    }

    /// What it says it can do.
    pub fn set_caps(&self, caps: Caps) {
        lock(&self.0).caps = caps;
    }

    /// The calls so far, oldest first.
    pub fn records(&self) -> Vec<Record> {
        lock(&self.0).records.clone()
    }

    pub fn clear(&self) {
        lock(&self.0).records.clear();
    }
}

impl Opener for RecordingOpener {
    fn name(&self) -> &str {
        "recording"
    }

    fn caps(&self) -> Caps {
        lock(&self.0).caps
    }

    fn open(&self, call: &OpenCall<'_>) -> Opening {
        let mut r = lock(&self.0);
        r.records.push(Record {
            target: call.target.as_str().to_string(),
            how: call.how,
            kind: call.kind,
            source: call.source,
            origin: source_of(call.origin),
        });
        Opening::Done(r.answer.clone())
    }
}

type ForwardFn = Arc<dyn Fn(&OpenCall<'_>) -> Opening + Send + Sync>;

/// An opener for a node without a screen: every request is refused with
/// the reason (its `link.failed` in the journal), or forwarded to an
/// attached interface when the host offers one.
#[derive(Clone)]
pub struct NullOpener {
    reason: String,
    forward: Option<ForwardFn>,
}

impl std::fmt::Debug for NullOpener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NullOpener")
            .field("reason", &self.reason)
            .field("forwards", &self.forward.is_some())
            .finish()
    }
}

impl Default for NullOpener {
    fn default() -> Self {
        Self {
            reason: "no interface to open it in".into(),
            forward: None,
        }
    }
}

impl NullOpener {
    /// Refuses with this reason.
    pub fn new(reason: &str) -> Self {
        Self {
            reason: reason.into(),
            forward: None,
        }
    }

    /// Forwards each request (to an attached interface, which asks its
    /// user).
    pub fn forward(f: impl Fn(&OpenCall<'_>) -> Opening + Send + Sync + 'static) -> Self {
        Self {
            reason: String::new(),
            forward: Some(Arc::new(f)),
        }
    }
}

impl Opener for NullOpener {
    fn name(&self) -> &str {
        if self.forward.is_some() {
            "forward"
        } else {
            "null"
        }
    }

    fn caps(&self) -> Caps {
        // The policy decides as anywhere; the answer is the refusal.
        Caps {
            inside: false,
            ..Caps::ALL
        }
    }

    fn open(&self, call: &OpenCall<'_>) -> Opening {
        match &self.forward {
            Some(f) => f(call),
            None => Opening::Done(Err(self.reason.clone())),
        }
    }
}

/// The links service: the policy and the opener, shared by the
/// application's windows.
pub struct Links {
    opener: Mutex<Arc<dyn Opener>>,
    policy: Mutex<Policy>,
}

impl std::fmt::Debug for Links {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Links")
            .field("opener", &self.opener().name())
            .field("policy", &self.policy())
            .finish()
    }
}

impl Links {
    pub fn new(opener: Arc<dyn Opener>, policy: Policy) -> Self {
        Self {
            opener: Mutex::new(opener),
            policy: Mutex::new(policy),
        }
    }

    /// The opener in use.
    pub fn opener(&self) -> Arc<dyn Opener> {
        lock(&self.opener).clone()
    }

    /// Replaces the opener (the platform's, a test's, a host's).
    pub fn set_opener(&self, opener: Arc<dyn Opener>) {
        *lock(&self.opener) = opener;
    }

    /// The policy, as it is now.
    pub fn policy(&self) -> Policy {
        lock(&self.policy).clone()
    }

    /// Changes the policy (schemes, documentation hosts, the
    /// application's scheme).
    pub fn update_policy(&self, f: impl FnOnce(&mut Policy)) {
        f(&mut lock(&self.policy));
    }

    /// Trusts a host for the session: content links to it open without
    /// asking.
    pub fn trust(&self, host: &str) {
        lock(&self.policy).trust(host);
    }
}

/// egui's plugin taking `OpenUrl` out of each frame's output (every
/// window's): the commands the service approved go on to eframe, the
/// others are opened through the service on the next pass.
#[derive(Debug, Default)]
pub struct Interceptor {
    active: bool,
    approved: Vec<String>,
    taken: Vec<egui::OpenUrl>,
}

impl egui::Plugin for Interceptor {
    fn debug_name(&self) -> &'static str {
        "egui_pop.links"
    }

    fn output_hook(&mut self, ctx: &egui::Context, output: &mut egui::FullOutput) {
        if !self.active {
            return;
        }
        let (approved, taken) = (&mut self.approved, &mut self.taken);
        let before = taken.len();
        output.platform_output.commands.retain(|c| match c {
            egui::OutputCommand::OpenUrl(u) => {
                if let Some(i) = approved.iter().position(|a| *a == u.url) {
                    approved.remove(i);
                    true
                } else {
                    taken.push(u.clone());
                    false
                }
            }
            _ => true,
        });
        if taken.len() > before {
            ctx.request_repaint();
        }
    }
}

/// A request waiting for the user's answer.
#[derive(Clone, Debug)]
struct Asking {
    request: Request,
    decision: Decision,
    /// The dialog showing it; none while queued behind another of the
    /// same agent.
    dialog: Option<u64>,
    /// Answered (accepted): its command runs next.
    answered: bool,
}

/// An open whose answer comes later.
struct Waiting {
    summary: Json,
    pending: Pending,
    toast: bool,
}

#[derive(Default)]
struct State {
    connected: Cell<bool>,
    next: Cell<u64>,
    asking: RefCell<BTreeMap<u64, Asking>>,
    /// Requests the user accepted: the next run opens them.
    accepted: RefCell<Vec<u64>>,
    /// Per agent session, the requests queued behind its confirmation.
    queued: RefCell<BTreeMap<String, VecDeque<u64>>>,
    opening: RefCell<Vec<Waiting>>,
    recent: RefCell<VecDeque<Json>>,
    /// Links declared by plugins: owner, place, target.
    declared: RefCell<Vec<(OwnerId, String, String)>>,
    /// Declared links drawn so far: target, label.
    seen: RefCell<BTreeMap<String, String>>,
    /// The link a copy takes.
    copying: RefCell<String>,
}

fn state(app: &AppHandle) -> std::rc::Rc<State> {
    app.local::<State>()
}

/// The links service, when the plugin is connected.
pub fn links(app: &AppHandle) -> Option<Arc<Links>> {
    app.service(&LINKS)
}

/// What is known of a local target on disk (nothing on the web build).
pub fn facts_of(target: &Target) -> Option<Facts> {
    let path = target.local_path()?;
    read_facts(&path)
}

#[cfg(not(target_arch = "wasm32"))]
fn read_facts(path: &str) -> Option<Facts> {
    Some(match std::fs::metadata(path) {
        Ok(m) => Facts {
            exists: true,
            dir: m.is_dir(),
            executable: m.is_file() && executable_bit(&m) && !has_extension(path),
        },
        Err(_) => Facts::default(),
    })
}

#[cfg(target_arch = "wasm32")]
fn read_facts(_: &str) -> Option<Facts> {
    None
}

/// Many file systems mark every file executable (FAT, network shares):
/// the bit counts only for a file without an extension, which the
/// extension list cannot judge.
#[cfg(not(target_arch = "wasm32"))]
fn has_extension(path: &str) -> bool {
    std::path::Path::new(path).extension().is_some()
}

#[cfg(unix)]
fn executable_bit(m: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    m.permissions().mode() & 0o111 != 0
}

#[cfg(all(not(unix), not(target_arch = "wasm32")))]
fn executable_bit(_: &std::fs::Metadata) -> bool {
    false
}

/// What the policy decides for a request now, with the opener's
/// capabilities and what the disk says (nothing is opened).
pub fn decide(app: &AppHandle, request: &Request) -> Result<Decision, String> {
    let links = links(app).ok_or(NO_PLUGIN)?;
    let facts = Target::parse(&request.target)
        .ok()
        .and_then(|t| facts_of(&t));
    Ok(links
        .policy()
        .decide(request, &links.opener().caps(), facts.as_ref()))
}

/// Opens a link through the [`STEP_OPEN`] step. A refusal or a failure
/// asked by the user shows in a toast with Copy link (or Show in folder
/// for a program); a request needing a confirmation opens it and ends
/// `asked` (or `queued` behind an agent's earlier one).
pub fn open(app: &AppHandle, request: Request) -> Outcome {
    run(app, request, false, None)
}

/// What opening a link would do (the kind, how, whether it asks, the
/// verdict) through the [`STEP_OPEN`] step, opening nothing.
pub fn dry_run(app: &AppHandle, request: Request) -> Outcome {
    run(app, request, true, None)
}

fn run(app: &AppHandle, request: Request, dry_run: bool, confirmed: Option<u64>) -> Outcome {
    let origin = request.origin.clone();
    let mut args = serde_json::to_value(&request).unwrap_or(Json::Null);
    args["origin"] = json!(source_of(&origin));
    args["dry_run"] = json!(dry_run);
    if let Some(id) = confirmed {
        args["confirmed"] = json!(id);
    }
    let who = origin.clone();
    let out = app.step(&STEP_OPEN, args.clone(), move |app, args| {
        let mut req: Request =
            serde_json::from_value(args.clone()).map_err(|e| format!("a link: {e}"))?;
        req.origin = who.clone();
        let dry = args["dry_run"].as_bool().unwrap_or(false);
        let confirmed = args["confirmed"].as_u64();
        run_default(app, req, dry, confirmed)
    });
    let toast = origin == Origin::User || confirmed.is_some();
    if let Outcome::Vetoed { reason, .. } = &out
        && toast
        && !dry_run
    {
        fail_toast(app, &request.target, reason, false);
    }
    remember(app, &args, &out);
    out
}

fn run_default(
    app: &AppHandle,
    req: Request,
    dry: bool,
    confirmed: Option<u64>,
) -> Result<Json, String> {
    let links = links(app).ok_or(NO_PLUGIN)?;
    let opener = links.opener();
    let caps = opener.caps();
    let target = Target::parse(&req.target).ok();
    let facts = target.as_ref().and_then(facts_of);
    let policy = links.policy();
    let d = policy.decide(&req, &caps, facts.as_ref());
    let mut summary = json!({
        "target": d.target,
        "kind": d.kind,
        "how": d.how,
        "source": req.source,
        "origin": source_of(&req.origin),
        "confirm": d.confirm,
        "why": d.why,
        "refused": d.refused.as_ref().map(ToString::to_string),
        "reveal_instead": d.reveal_instead,
        "host": d.host,
        "disguised": d.disguised,
        "opener": opener.name(),
    });
    if dry {
        summary["dry_run"] = json!(true);
        summary["status"] = json!(match (&d.refused, d.confirm) {
            (Some(_), _) => "refused",
            (None, true) => "would_ask",
            (None, false) => "would_open",
        });
        return Ok(summary);
    }
    let toast = req.origin == Origin::User || confirmed.is_some();
    if let Some(reason) = &d.refused {
        let reason = reason.to_string();
        app.publish(
            &source_of(&req.origin),
            None,
            "link.refused",
            json!({ "target": d.target, "reason": reason }),
        );
        if toast {
            fail_toast(app, &req.target, &reason, d.reveal_instead);
        }
        return Err(reason);
    }
    let st = state(app);
    let was_accepted = confirmed.is_some_and(|id| {
        let mut acc = st.accepted.borrow_mut();
        match acc.iter().position(|a| *a == id) {
            Some(i) => {
                acc.remove(i);
                true
            }
            None => false,
        }
    });
    if d.confirm && !was_accepted {
        return ask(app, req, d, summary);
    }
    let target = target.ok_or("a link that does not parse")?;
    perform(
        app,
        &req,
        &d,
        &target,
        &policy,
        opener.as_ref(),
        summary,
        toast,
    )
}

#[allow(clippy::too_many_arguments)]
fn perform(
    app: &AppHandle,
    req: &Request,
    d: &Decision,
    target: &Target,
    policy: &Policy,
    opener: &dyn Opener,
    mut summary: Json,
    toast: bool,
) -> Result<Json, String> {
    let who = source_of(&req.origin);
    if d.how == How::Inside {
        let (command, args) = match d.kind {
            Kind::AppLink => (
                CommandId::from_static("deeplink.open"),
                json!({ "url": target.as_str() }),
            ),
            Kind::Command => {
                let (id, args) = target.command().ok_or("a command link names no command")?;
                let id = CommandId::new(id.clone()).map_err(|e| format!("`{id}`: {e}"))?;
                (id, args)
            }
            _ => return Err("no viewer inside the application for this link".into()),
        };
        if !app.commands().iter().any(|(s, _)| s.id == command) {
            let e = format!("nothing handles it: no command `{command}`");
            if toast {
                fail_toast(app, &req.target, &e, false);
            }
            return Err(e);
        }
        app.run_as(command, args, req.origin.clone());
        app.publish(&who, None, "link.opened", summary.clone());
        summary["status"] = json!("opened");
        return Ok(summary);
    }
    // A `docs:` page without the viewer: its online copy.
    let online;
    let target = match (d.kind, target.doc_page()) {
        (Kind::Doc, Some(page)) => {
            let url = policy
                .docs_url(&page)
                .ok_or("this page has no online copy")?;
            online = Target::parse(&url).map_err(|e| e.to_string())?;
            &online
        }
        _ => target,
    };
    let ctx = app.egui();
    let call = OpenCall {
        target,
        how: d.how,
        kind: d.kind,
        source: req.source,
        origin: &req.origin,
        egui: &ctx,
    };
    match opener.open(&call) {
        Opening::Done(Ok(())) => {
            app.publish(&who, None, "link.opened", summary.clone());
            summary["status"] = json!("opened");
            Ok(summary)
        }
        Opening::Done(Err(e)) => {
            failed(app, &summary, &e, toast);
            Err(e)
        }
        Opening::Later(pending) => {
            state(app).opening.borrow_mut().push(Waiting {
                summary: summary.clone(),
                pending,
                toast,
            });
            summary["status"] = json!("opening");
            Ok(summary)
        }
    }
}

fn failed(app: &AppHandle, summary: &Json, error: &str, toast: bool) {
    let mut data = summary.clone();
    data["error"] = json!(error);
    let who = summary["origin"].as_str().unwrap_or("user").to_string();
    app.publish(&who, None, "link.failed", data);
    if toast {
        let target = summary["target"].as_str().unwrap_or_default().to_string();
        fail_toast(app, &target, error, false);
    }
}

/// A warning toast saying why a link did not open, with Copy link, or
/// Show in folder for a program.
fn fail_toast(app: &AppHandle, target: &str, reason: &str, reveal: bool) {
    let Some(t) = app.service(&TOASTS) else {
        return;
    };
    let ctx = app.egui();
    let text = strings::tr_args(&ctx, &txt::LINKS_NOT_OPENED, &[("reason", reason)]);
    let toast = Toast::warning(&text);
    t.notify(if reveal {
        toast.action(
            &strings::tr(&ctx, &txt::LINKS_REVEAL),
            CMD_REVEAL,
            json!({ "path": target }),
        )
    } else {
        toast.action(
            &strings::tr(&ctx, &txt::LINKS_COPY),
            CMD_COPY,
            json!({ "target": target }),
        )
    });
}

/// The session an origin asks for (`agent:<name>`), when it is not the
/// user.
fn session(origin: &Origin) -> Option<String> {
    (*origin != Origin::User).then(|| source_of(origin))
}

fn ask(app: &AppHandle, req: Request, d: Decision, mut summary: Json) -> Result<Json, String> {
    let st = state(app);
    let id = st.next.get() + 1;
    st.next.set(id);
    let who = source_of(&req.origin);
    // One confirmation per agent at a time; five more wait, then no.
    if let Some(s) = session(&req.origin) {
        let busy = st
            .asking
            .borrow()
            .values()
            .any(|a| a.dialog.is_some() && session(&a.request.origin).as_deref() == Some(&s));
        if busy {
            let mut queued = st.queued.borrow_mut();
            let q = queued.entry(s.clone()).or_default();
            if q.len() >= AGENT_QUEUE {
                return Err(format!(
                    "`{s}` has {AGENT_QUEUE} link requests waiting already: answer them first"
                ));
            }
            q.push_back(id);
            let position = q.len();
            drop(queued);
            st.asking.borrow_mut().insert(
                id,
                Asking {
                    request: req,
                    decision: d,
                    dialog: None,
                    answered: false,
                },
            );
            summary["status"] = json!("queued");
            summary["id"] = json!(id);
            summary["position"] = json!(position);
            app.publish(&who, None, "link.asked", summary.clone());
            return Ok(summary);
        }
    }
    let dialog = show_confirm(app, id, &req, &d)?;
    st.asking.borrow_mut().insert(
        id,
        Asking {
            request: req,
            decision: d,
            dialog: Some(dialog),
            answered: false,
        },
    );
    summary["status"] = json!("asked");
    summary["id"] = json!(id);
    app.publish(&who, None, "link.asked", summary.clone());
    Ok(summary)
}

/// The confirmation: the full target, the host (its punycode beside an
/// internationalised one), why it asks, Copy link, Open, Cancel, and for
/// a content link "Trust <host> for this session".
fn show_confirm(app: &AppHandle, id: u64, req: &Request, d: &Decision) -> Result<u64, String> {
    let dialogs = app
        .service(&DIALOGS)
        .ok_or("this link needs a confirmation, and the dialogs plugin is not connected")?;
    let ctx = app.egui();
    let target = Target::parse(&req.target).map_err(|e| e.to_string())?;
    let shown = target.shown();
    let title = match (&req.origin, d.how, d.kind) {
        (Origin::Agent(name), _, _) => {
            strings::tr_args(&ctx, &txt::LINKS_CONFIRM_AGENT, &[("agent", name)])
        }
        (o, _, _) if *o != Origin::User => {
            strings::tr_args(&ctx, &txt::LINKS_CONFIRM_AGENT, &[("agent", &source_of(o))])
        }
        (_, How::Reveal, _) => strings::tr(&ctx, &txt::LINKS_CONFIRM_REVEAL),
        (_, _, Kind::File | Kind::Image | Kind::Folder) if target.scheme() == "file" => {
            strings::tr(&ctx, &txt::LINKS_CONFIRM_FILE)
        }
        _ => strings::tr(&ctx, &txt::LINKS_CONFIRM_TITLE),
    };
    let verb = if d.how == How::Reveal {
        strings::tr(&ctx, &txt::LINKS_REVEAL)
    } else {
        strings::tr(&ctx, &txt::LINKS_OPEN)
    };
    let host = d.host.clone();
    let trust_host = (req.source == Source::Content && d.why.as_deref() == Some("untrusted host"))
        .then(|| d.host_ascii.clone())
        .flatten();
    let disguised = d.disguised.then(|| req.label.clone().unwrap_or_default());
    let content = req.source == Source::Content;
    let mut trust = false;
    let spec = Custom::new(&title, &verb, CMD_CONFIRMED)
        .args(json!({ "id": id }))
        .size(480.0, 132.0);
    let copy_target = req.target.clone();
    let description = json!({
        "target": shown,
        "host": host,
        "why": d.why,
        "disguised": disguised,
        "trust": trust_host,
    });
    dialogs.open_custom(app, spec, move |ui, app, pass| {
        ui.label(RichText::new(&shown).monospace());
        if let Some(h) = &host {
            ui.label(strings::tr_args(ui.ctx(), &txt::LINKS_HOST, &[("host", h)]));
        }
        if let Some(label) = &disguised {
            ui.label(strings::tr_args(
                ui.ctx(),
                &txt::LINKS_CONFIRM_DISGUISED,
                &[("label", label)],
            ));
        } else if content {
            ui.weak(strings::tr(ui.ctx(), &txt::LINKS_CONFIRM_CONTENT));
        }
        ui.horizontal(|ui| {
            if let Some(h) = &trust_host {
                let text = strings::tr_args(ui.ctx(), &txt::LINKS_TRUST, &[("host", h)]);
                if ui.checkbox(&mut trust, text).changed() {
                    pass.value("trust", json!(trust));
                }
            }
            if ui.button(strings::tr(ui.ctx(), &txt::LINKS_COPY)).clicked() {
                copy_link(app, &copy_target, &Origin::User);
            }
        });
        pass.describe(description.clone());
    })
}

/// The user's answer to a confirmation: open it (trusting its host when
/// asked), then the agent's next request.
fn confirmed(app: &AppHandle, args: &Json, origin: &Origin) -> Result<Json, String> {
    if *origin != Origin::User {
        return Err("only the user answers a link's confirmation".into());
    }
    let id = args["id"].as_u64().ok_or("`id` is missing")?;
    let st = state(app);
    let asking = st
        .asking
        .borrow_mut()
        .remove(&id)
        .ok_or_else(|| format!("no link waits for an answer under {id}"))?;
    if args["trust"].as_bool() == Some(true)
        && let (Some(links), Some(host)) = (links(app), &asking.decision.host_ascii)
    {
        links.trust(host);
    }
    st.accepted.borrow_mut().push(id);
    let out = run(app, asking.request.clone(), false, Some(id));
    next_of(app, &asking.request.origin);
    match out {
        Outcome::Done(r) => r,
        Outcome::Vetoed { by, reason } => Err(format!("vetoed by {by}: {reason}")),
    }
}

/// A confirmation cancelled (or never shown): the agent hears it, its
/// next request comes up.
fn cancelled(app: &AppHandle, id: u64) {
    let st = state(app);
    let Some(asking) = st.asking.borrow_mut().remove(&id) else {
        return;
    };
    let who = source_of(&asking.request.origin);
    app.publish(
        &who,
        None,
        "link.cancelled",
        json!({ "id": id, "target": asking.decision.target, "origin": who }),
    );
    remember(
        app,
        &json!({ "target": asking.request.target, "origin": who, "how": asking.request.how, "source": asking.request.source }),
        &Outcome::Done(Ok(json!({ "status": "cancelled" }))),
    );
    next_of(app, &asking.request.origin);
}

/// Shows the next queued request of an agent, if any.
fn next_of(app: &AppHandle, origin: &Origin) {
    let Some(s) = session(origin) else {
        return;
    };
    let st = state(app);
    loop {
        let next = st
            .queued
            .borrow_mut()
            .get_mut(&s)
            .and_then(VecDeque::pop_front);
        let Some(id) = next else {
            return;
        };
        let item = st.asking.borrow().get(&id).cloned();
        let Some(item) = item else {
            continue;
        };
        match show_confirm(app, id, &item.request, &item.decision) {
            Ok(dialog) => {
                if let Some(a) = st.asking.borrow_mut().get_mut(&id) {
                    a.dialog = Some(dialog);
                }
                return;
            }
            Err(_) => {
                st.asking.borrow_mut().remove(&id);
            }
        }
    }
}

/// Copies a link: through the clipboard (the `clipboard.copy` step, a
/// `text/uri-list` item with its text) when its plugin is connected,
/// else as text.
pub fn copy_link(app: &AppHandle, target: &str, origin: &Origin) {
    let st = state(app);
    *st.copying.borrow_mut() = target.to_string();
    if clipboard::with_owner(app, |_| ()).is_some() {
        clipboard::copy(app, CopyRequest::new(ENDPOINT), origin);
    } else {
        app.egui().copy_text(target.to_string());
    }
}

/// What a copy of a link takes.
struct LinkSource;

impl ClipSource for LinkSource {
    fn copy(&self, app: &AppHandle) -> Option<clipboard::ClipItem> {
        let target = state(app).copying.borrow().clone();
        (!target.is_empty()).then(|| {
            clipboard::ClipItem::new(clipboard::URI_LIST, 1, json!(target), "a link")
                .fallback(clipboard::TEXT, json!(target))
        })
    }
}

fn remember(app: &AppHandle, args: &Json, out: &Outcome) {
    let (status, error) = match out {
        Outcome::Done(Ok(v)) => (v["status"].as_str().unwrap_or("done").to_string(), None),
        Outcome::Done(Err(e)) => ("failed".to_string(), Some(e.clone())),
        Outcome::Vetoed { by, reason } => ("vetoed".to_string(), Some(format!("{by}: {reason}"))),
    };
    let decision = match out {
        Outcome::Done(Ok(v)) => {
            json!({ "kind": v["kind"], "how": v["how"], "confirm": v["confirm"], "why": v["why"] })
        }
        _ => Json::Null,
    };
    let entry = json!({
        "target": args["target"],
        "how": args["how"],
        "source": args["source"],
        "origin": args["origin"],
        "dry_run": args["dry_run"],
        "status": status,
        "error": error,
        "decision": decision,
    });
    let st = state(app);
    let mut recent = st.recent.borrow_mut();
    recent.push_back(entry);
    while recent.len() > RECENT {
        recent.pop_front();
    }
}

/// The requests seen lately, oldest first, as the `links` collector
/// shows them.
pub fn recent(app: &AppHandle) -> Vec<Json> {
    state(app).recent.borrow().iter().cloned().collect()
}

/// Declares a link the application shows (About, help, a tip): checked
/// by [`lint`] (the plugin-owned form is [`Ctx::link`]).
pub fn declare(app: &AppHandle, place: &str, target: &str) {
    state(app).declared.borrow_mut().push((
        OwnerId::from_static("app"),
        place.into(),
        target.into(),
    ));
}

/// The links declared (place, target) and those drawn as declared so far
/// (`drawn`, the label).
pub fn declared(app: &AppHandle) -> Vec<(String, String)> {
    let st = state(app);
    let mut out: Vec<(String, String)> = st
        .declared
        .borrow()
        .iter()
        .map(|(_, p, t)| (p.clone(), t.clone()))
        .collect();
    out.extend(
        st.seen
            .borrow()
            .iter()
            .map(|(t, l)| (format!("drawn: {l}"), t.clone())),
    );
    out
}

/// Each pass: egui's own `OpenUrl` taken from the last frame opened
/// through the service, later answers collected, confirmations closed
/// without an answer cancelled.
fn pass(app: &AppHandle) {
    if !state(app).connected.get() {
        return;
    }
    let ctx = app.egui();
    if ctx.with_plugin(|_: &mut Interceptor| ()).is_none() {
        ctx.add_plugin(Interceptor::default());
    }
    let taken = ctx
        .with_plugin(|i: &mut Interceptor| {
            i.active = true;
            std::mem::take(&mut i.taken)
        })
        .unwrap_or_default();
    for u in taken {
        // What an application's own hyperlink is: declared for web and
        // mail, content otherwise.
        let declared = ["http:", "https:", "mailto:"]
            .iter()
            .any(|s| u.url.to_ascii_lowercase().starts_with(s));
        let source = if declared {
            Source::Declared
        } else {
            Source::Content
        };
        open(app, Request::new(&u.url).source(source));
    }
    let st = state(app);
    let done: Vec<(Json, Result<Json, String>, bool)> = {
        let mut opening = st.opening.borrow_mut();
        let mut out = Vec::new();
        opening.retain(|o| match o.pending.take() {
            Some(r) => {
                out.push((o.summary.clone(), r, o.toast));
                false
            }
            None => true,
        });
        out
    };
    for (summary, result, toast) in done {
        match result {
            Ok(_) => {
                let who = summary["origin"].as_str().unwrap_or("user").to_string();
                app.publish(&who, None, "link.opened", summary);
            }
            Err(e) => failed(app, &summary, &e, toast),
        }
    }
    // A confirmation gone without an answer (its `dialog.show` vetoed).
    if let Some(d) = app.service(&DIALOGS) {
        let gone: Vec<u64> = st
            .asking
            .borrow()
            .iter()
            .filter(|(_, a)| !a.answered && a.dialog.is_some_and(|id| !d.is_open(id)))
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            cancelled(app, id);
        }
    }
}

/// The collector's snapshot.
fn describe(app: &AppHandle) -> Json {
    let st = state(app);
    let (opener, caps, policy) = match links(app) {
        Some(l) => {
            let o = l.opener();
            (json!(o.name()), json!(o.caps()), json!(l.policy()))
        }
        None => (Json::Null, Json::Null, Json::Null),
    };
    let asking: Vec<Json> = st
        .asking
        .borrow()
        .iter()
        .map(|(id, a)| {
            json!({
                "id": id,
                "target": a.decision.target,
                "kind": a.decision.kind,
                "how": a.decision.how,
                "why": a.decision.why,
                "origin": source_of(&a.request.origin),
                "dialog": a.dialog,
                "queued": a.dialog.is_none(),
            })
        })
        .collect();
    json!({
        "opener": opener,
        "caps": caps,
        "policy": policy,
        "asking": asking,
        "opening": st.opening.borrow().len(),
        "recent": recent(app),
        "declared": declared(app).into_iter().map(|(p, t)| json!({ "place": p, "target": t })).collect::<Vec<_>>(),
    })
}

/// The link widget: the theme's link colour and a pointing hand, the
/// real target on hover, a context menu (Open, Open outside when the
/// default is inside, Show in folder for a local file, Copy link), Enter
/// and Space to open, Ctrl+click and middle click to open outside, role
/// `link` with the label as its name and the target as its description.
#[derive(Clone, Debug)]
pub struct Link {
    pub label: String,
    pub target: String,
    pub source: Source,
}

impl Link {
    /// A link the application declares.
    pub fn new(label: &str, target: &str) -> Self {
        Self {
            label: label.into(),
            target: target.into(),
            source: Source::Declared,
        }
    }

    /// A link found in content (a document, a program's output).
    pub fn content(label: &str, target: &str) -> Self {
        Self::new(label, target).source(Source::Content)
    }

    pub fn source(mut self, source: Source) -> Self {
        self.source = source;
        self
    }

    fn request(&self, how: How) -> Request {
        Request::new(&self.target)
            .how(how)
            .source(self.source)
            .label(&self.label)
    }

    /// Draws it; a click opens it through [`open`].
    pub fn show(self, ui: &mut Ui, app: &AppHandle) -> Response {
        let parsed = Target::parse(&self.target).ok();
        let response = ui.add(egui::Link::new(&self.label));
        let shown = parsed
            .as_ref()
            .map_or_else(|| self.target.clone(), Target::shown);
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_description(shown.clone());
        });
        if self.source == Source::Declared {
            state(app)
                .seen
                .borrow_mut()
                .insert(self.target.clone(), self.label.clone());
        }
        let response = response.on_hover_ui(|ui| hover(ui, &self.target, parsed.as_ref()));
        if response.clicked_with_open_in_background() || response.middle_clicked() {
            open(app, self.request(How::Outside));
        } else if response.clicked() {
            open(app, self.request(How::Auto));
        }
        egui_pop_core::pointer::claim::context_menu(&response, |ui| {
            let d = decide(app, &self.request(How::Auto)).ok();
            if ui.button(strings::tr(ui.ctx(), &txt::LINKS_OPEN)).clicked() {
                open(app, self.request(How::Auto));
                ui.close();
            }
            if d.as_ref().is_some_and(|d| d.how == How::Inside)
                && ui
                    .button(strings::tr(ui.ctx(), &txt::LINKS_OPEN_OUTSIDE))
                    .clicked()
            {
                open(app, self.request(How::Outside));
                ui.close();
            }
            let local = parsed.as_ref().is_some_and(|t| t.scheme() == "file");
            if local
                && ui
                    .button(strings::tr(ui.ctx(), &txt::LINKS_REVEAL))
                    .clicked()
            {
                open(app, self.request(How::Reveal));
                ui.close();
            }
            if ui.button(strings::tr(ui.ctx(), &txt::LINKS_COPY)).clicked() {
                copy_link(app, &self.target, &Origin::User);
                ui.close();
            }
        });
        response
    }
}

/// Draws a link the application declares; see [`Link`].
pub fn link(ui: &mut Ui, app: &AppHandle, label: &str, target: &str) -> Response {
    Link::new(label, target).show(ui, app)
}

/// A link's hover: the full target, then its host (an internationalised
/// one with its punycode).
fn hover(ui: &mut Ui, raw: &str, target: Option<&Target>) {
    ui.set_max_width(420.0);
    let shown = target.map_or_else(|| raw.to_string(), Target::shown);
    ui.label(RichText::new(shown).monospace());
    if let Some(host) = target.and_then(Target::host_shown) {
        ui.label(
            RichText::new(strings::tr_args(
                ui.ctx(),
                &txt::LINKS_HOST,
                &[("host", &host)],
            ))
            .strong(),
        );
    }
}

/// The links service as a plugin (in `DefaultPlugins`): the policy and
/// the opener ([`EguiOpener`] unless given), the `link.open` step, the
/// interceptor of egui's `OpenUrl`, the confirmation, the commands
/// (`links.open`, `links.check`, `links.copy`, `files.reveal`) and the
/// `links` collector.
pub struct LinksPlugin {
    policy: Policy,
    opener: Arc<dyn Opener>,
}

impl std::fmt::Debug for LinksPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinksPlugin")
            .field("policy", &self.policy)
            .field("opener", &self.opener.name())
            .finish()
    }
}

impl Default for LinksPlugin {
    fn default() -> Self {
        Self {
            policy: Policy::default(),
            opener: Arc::new(EguiOpener),
        }
    }
}

impl LinksPlugin {
    /// The policy to start from (schemes, the application's scheme,
    /// documentation hosts).
    pub fn policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    /// The opener to use.
    pub fn opener(mut self, opener: Arc<dyn Opener>) -> Self {
        self.opener = opener;
        self
    }
}

fn outcome_json(o: Outcome) -> Result<Json, String> {
    match o {
        Outcome::Done(r) => r,
        Outcome::Vetoed { by, reason } => Err(format!("vetoed by {by}: {reason}")),
    }
}

/// A request from a command's arguments: an agent's (or a script's) is
/// always the agent's source, whatever it says.
fn request_of(args: &Json, origin: &Origin, key: &str) -> Result<Request, String> {
    let target = args[key]
        .as_str()
        .ok_or_else(|| format!("`{key}` is missing"))?;
    let mut r = Request::new(target).origin(origin.clone());
    if let Some(how) = args.get("how").filter(|h| !h.is_null()) {
        r.how = serde_json::from_value(how.clone()).map_err(|e| format!("`how`: {e}"))?;
    }
    if let Some(label) = args["label"].as_str() {
        r.label = Some(label.into());
    }
    r.source = if *origin == Origin::User {
        match args.get("source").filter(|s| !s.is_null()) {
            Some(s) => serde_json::from_value(s.clone()).map_err(|e| format!("`source`: {e}"))?,
            None => Source::Declared,
        }
    } else {
        Source::Agent
    };
    Ok(r)
}

impl Plugin for LinksPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Links".into()
    }

    /// After the dialogs plugin, whose answers it watches.
    fn priority(&self) -> i32 {
        -1
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let app = ctx.app();
        state(&app).connected.set(true);
        ctx.service(&LINKS, Links::new(self.opener.clone(), self.policy.clone()))?;
        let egui_ctx = app.egui();
        egui_ctx.add_plugin(Interceptor::default());
        egui_ctx.with_plugin(|i: &mut Interceptor| i.active = true);
        ctx.step(
            StepSpec::new(
                STEP_OPEN,
                "Opening a link; a veto keeps it closed, a redirect opens another target or another way, a run-hook opens it its own way.",
            )
            .args(json!({ "target": "https://example.org/", "how": "auto", "source": "declared", "origin": "user", "dry_run": false })),
        )?;
        let category = "Links";
        ctx.command(
            CommandSpec::new(CMD_OPEN, "Open a link", category)
                .description("Opens a link (`target`) its kind's way or `how` (auto, outside, inside, reveal); an agent's request is always confirmed by the user; `dry_run` says the kind, how, whether it would ask and the verdict, opening nothing.")
                .args(json!({ "target": "https://example.org/", "how": "auto", "dry_run": true })),
            |call| {
                let req = request_of(call.args, call.origin, "target")?;
                if call.args["dry_run"].as_bool() == Some(true) {
                    outcome_json(dry_run(call.app, req))
                } else {
                    outcome_json(open(call.app, req))
                }
            },
        )?
        .command(
            CommandSpec::new(CMD_CHECK, "Check a link", category)
                .description("What a link (`target`) is and how it would open (`how`, `source`): its kind, the way, whether the user would be asked, the policy's verdict and the opener's capabilities; nothing is opened and no step runs.")
                .args(json!({ "target": "https://example.org/", "how": "auto" })),
            |call| {
                let req = request_of(call.args, call.origin, "target")?;
                let d = decide(call.app, &req)?;
                let caps = links(call.app).map(|l| l.opener().caps());
                let mut out = serde_json::to_value(&d).map_err(|e| e.to_string())?;
                out["caps"] = json!(caps);
                out["refused"] = json!(d.refused.as_ref().map(ToString::to_string));
                Ok(out)
            },
        )?
        .command(
            CommandSpec::new(CMD_COPY, "Copy a link", category)
                .description("Copies a link (`target`) through the clipboard.")
                .args(json!({ "target": "https://example.org/" }))
                .hidden(),
            |call| {
                let target = call.args["target"].as_str().ok_or("`target` is missing")?;
                copy_link(call.app, target, call.origin);
                Ok(json!({ "copied": target }))
            },
        )?
        .command(
            CommandSpec::new(CMD_REVEAL, "Show in folder", "File")
                .description("Shows a file (`path`) selected in the file manager; an agent's request is confirmed by the user.")
                .args(json!({ "path": "/home/me/export.png" })),
            |call| {
                let req = request_of(call.args, call.origin, "path")?.how(How::Reveal);
                outcome_json(open(call.app, req))
            },
        )?
        .command(
            CommandSpec::new(CMD_CONFIRMED, "Open the confirmed link", category)
                .description("The user's answer to a link's confirmation (`id`, `trust`).")
                .args(json!({ "id": 1, "trust": false }))
                .hidden()
                .no_agent(),
            |call| confirmed(call.app, call.args, call.origin),
        )?;
        // Only the user answers a confirmation; an agent may cancel it.
        // (Without the dialogs plugin nothing is confirmed: a request
        // that must be is refused with the reason.)
        if ctx.is_connected(&dialogs::PLUGIN) {
            hook_answers(ctx);
        }
        ctx.clip_endpoint(ENDPOINT, Some(std::rc::Rc::new(LinkSource)), None);
        let weak = app.downgrade();
        ctx.collector(
            FnCollector::new(
                "links",
                "Links: the opener and its capabilities, the policy (schemes, trusted hosts, documentation hosts), the requests waiting for the user's answer or queued per agent, the recent requests with their decision and outcome, the links declared.",
            )
            .snapshot(move |_| match weak.get() {
                Ok(app) => describe(&app),
                Err(_) => Json::Null,
            }),
        )?;
        ctx.on(On::Pass, |app, _| pass(app))
            .on(On::Disconnected, |app, ev| {
                if let Lifecycle::Disconnected(p) = ev {
                    let owner = OwnerId::from(p);
                    state(app)
                        .declared
                        .borrow_mut()
                        .retain(|(o, _, _)| *o != owner);
                }
            });
        Ok(())
    }

    fn disconnect(&mut self, ctx: &mut Ctx<'_>) {
        let app = ctx.app();
        state(&app).connected.set(false);
        app.egui()
            .with_plugin(|i: &mut Interceptor| i.active = false);
    }
}

/// The confirmation's answers: only the user's accepts one; a cancel
/// tells the agent and brings its next request up.
fn hook_answers(ctx: &mut Ctx<'_>) {
    ctx.before(dialogs::STEP_ANSWER, |_, args| {
        let ours = args["dialog"]["command"].as_str() == Some(CMD_CONFIRMED.as_str());
        let cancel = args["answer"]["answer"].as_str() == Some("cancel");
        if ours && !cancel && args["origin"].as_str() != Some("user") {
            Before::Veto("only the user answers a link's confirmation".into())
        } else {
            Before::Go
        }
    })
    .after(dialogs::STEP_ANSWER, |app, args, result| {
        if args["dialog"]["command"].as_str() != Some(CMD_CONFIRMED.as_str()) || result.is_err() {
            return;
        }
        let Some(id) = args["dialog"]["args"]["id"].as_u64() else {
            return;
        };
        if args["answer"]["answer"].as_str() == Some("cancel") {
            cancelled(app, id);
        } else if let Some(a) = state(app).asking.borrow_mut().get_mut(&id) {
            a.answered = true;
        }
    });
}

impl Ctx<'_> {
    /// Declares a link this plugin shows (its About, a help entry, a
    /// tip): [`lint`] checks it; it goes with the plugin.
    pub fn link(&mut self, place: &str, target: &str) -> &mut Self {
        state(self.app).declared.borrow_mut().push((
            self.owner.clone(),
            place.into(),
            target.into(),
        ));
        self
    }
}
