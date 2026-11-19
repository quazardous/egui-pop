//! Grants: what each agent on the socket may do, set by the application.
//!
//! A [`Grant`] says which commands an agent may run (patterns), whether
//! it may only read, which values it may write through `value.set` and
//! `value.reset`, whether `ui` may act (click, type) for it, and whether
//! it reads private data (places, people, file paths). Three presets:
//! [`Grant::read_only`], [`Grant::editor`], [`Grant::full`], each with
//! private data shown ([`Grant::private`] turns it off).
//!
//! [`AgentGrants`] gives one to each agent:
//!
//! - a connection that said no `hello` (`Origin::Agent("agent-socket")`)
//!   gets [`AgentGrants::unpaired`], read-only by default;
//! - a paired agent gets its own ([`AgentGrants::agent`]), or
//!   [`AgentGrants::paired`], the editor preset by default;
//! - any other origin (the user, an in-process caller, a script) is not
//!   the socket's, and grants say nothing about it.
//!
//! The bridge's mode stays the ceiling: under `BridgeMode::ReadOnly` no
//! grant makes an agent write.
//!
//! **Enforced** in two places, fail-closed:
//!
//! - the socket checks each `call` and `ui` action before it runs or is
//!   held for the user's confirmation, so the agent hears the refusal at
//!   once;
//! - a listener named [`GRANTS`] on `pop.command.run` refuses any run of
//!   a command for an agent its grant does not allow, whatever path the
//!   run took. It is placed before the application's listeners (priority
//!   [`PRIORITY`]), so an application policy such as `helios.policy`
//!   sees only what the grants let through.
//!
//! A refusal reaches the journal as `command.refused` (a command) or
//! `debug.call.refused` (another action, `ui` too) with `by`
//! `pop.agent.grants`.
//!
//! **Escape hatch.** Until scenario runners and the MCP binary say
//! `hello`, an application that lets unpaired clients write as before
//! says so: [`AgentGrants::allow_unpaired_writes`].

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use egui_pop_api::bus::bus_contract::{HookError, ListenOpts, Point as _};
use egui_pop_api::bus::{self, CommandAsk, CommandRun};
use egui_pop_api::{AppBuilder, AppHandle, Ctx, Plugin, PluginError};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{CommandId, PluginId};
use serde_json::{Value as Json, json};

use crate::Reply;
use crate::rpc::RpcError;

/// The grants' listener on `pop.command.run`, and the `by` of their
/// refusals on the journal.
pub const GRANTS: &str = "pop.agent.grants";
/// The listener's priority on `pop.command.run`: above the default (0),
/// so it runs before the application's listeners that set none. An
/// application listener that must run first says
/// `ListenOpts::before(GRANTS)`.
pub const PRIORITY: i32 = 10_000;
/// The plugin [`AgentGrantsExt::agent_grants`] adds.
pub const GRANTS_PLUGIN: PluginId = PluginId::from_static("egui_pop.agent_grants");

/// The commands that write a value, held to [`Grant::values`].
const VALUE_WRITES: [&str; 2] = ["value.set", "value.reset"];

/// What one agent may do.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Grant {
    /// The commands it may run: `*` for all, `files.*` for those under
    /// a prefix, or an id. `value.set` and `value.reset` are held to
    /// [`Grant::values`] instead.
    pub commands: Vec<String>,
    /// Only what reads: read-only commands and bridge calls that change
    /// nothing; no value written, no `ui` action.
    pub read_only: bool,
    /// The value paths it may write (`value.set`, `value.reset`): `*`,
    /// a prefix (`settings/*`) or a path.
    pub values: Vec<String>,
    /// `ui` may act for it (click, type, press keys, scroll); reading
    /// the tree is always allowed.
    pub ui: bool,
    /// It reads private data: places (GPS), people (faces, names),
    /// absolute file paths. Without it, a command declared
    /// `CommandSpec::private` is refused to it, and a command whose
    /// result only partly is private leaves those parts out
    /// ([`reads_private`]). On in every preset.
    pub private: bool,
}

impl Grant {
    /// Reads everything, changes nothing.
    pub fn read_only() -> Self {
        Self {
            commands: vec!["*".into()],
            read_only: true,
            values: Vec::new(),
            ui: false,
            private: true,
        }
    }

    /// Runs every command offered to agents (destructive ones still
    /// confirmed by the user); writes no value directly and does not act
    /// through `ui`.
    pub fn editor() -> Self {
        Self {
            read_only: false,
            ..Self::read_only()
        }
    }

    /// Everything the bridge's mode allows: every command, every value,
    /// `ui` actions.
    pub fn full() -> Self {
        Self {
            commands: vec!["*".into()],
            read_only: false,
            values: vec!["*".into()],
            ui: true,
            private: true,
        }
    }

    /// Only these commands (patterns, see [`Grant::commands`]).
    #[must_use]
    pub fn commands<S: Into<String>>(mut self, patterns: impl IntoIterator<Item = S>) -> Self {
        self.commands = patterns.into_iter().map(Into::into).collect();
        self
    }

    /// These value paths writable (patterns, see [`Grant::values`]).
    #[must_use]
    pub fn values<S: Into<String>>(mut self, patterns: impl IntoIterator<Item = S>) -> Self {
        self.values = patterns.into_iter().map(Into::into).collect();
        self
    }

    /// `ui` actions allowed or not.
    #[must_use]
    pub fn ui(mut self, on: bool) -> Self {
        self.ui = on;
        self
    }

    /// Private data shown or withheld ([`Grant::private`]).
    #[must_use]
    pub fn private(mut self, on: bool) -> Self {
        self.private = on;
        self
    }

    /// Why this grant refuses a command whose result carries private
    /// data (`CommandSpec::private`); `None` when it allows it.
    pub fn private_refusal(&self, command: &str, private: bool) -> Option<String> {
        (private && !self.private).then(|| {
            format!("the agent does not read private data: `{command}` gives places, people or file paths")
        })
    }

    /// Why this grant refuses running `command` (whose spec says
    /// whether it only reads) with `args`; `None` when it allows it.
    pub fn refusal(&self, command: &str, read_only: bool, args: &Json) -> Option<String> {
        if VALUE_WRITES.contains(&command) {
            let path = args.get("path").and_then(Json::as_str).unwrap_or_default();
            if self.read_only {
                return Some(format!("the agent is read-only: it may not write `{path}`"));
            }
            if !matches_any(&self.values, path) {
                return Some(format!("the agent may not write the value `{path}`"));
            }
            return None;
        }
        if !matches_any(&self.commands, command) {
            return Some(format!("the agent may not run `{command}`"));
        }
        if self.read_only && !read_only {
            return Some(format!(
                "the agent is read-only: `{command}` changes the application"
            ));
        }
        None
    }
}

/// Whether `id` matches one of `patterns`: `*`, `prefix*`, or itself.
fn matches_any(patterns: &[String], id: &str) -> bool {
    patterns.iter().any(|p| match p.strip_suffix('*') {
        Some(prefix) => id.starts_with(prefix),
        None => p == id,
    })
}

/// The grants of the socket's agents.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AgentGrants {
    /// A connection that said no `hello`: [`Grant::read_only`] by
    /// default.
    pub unpaired: Grant,
    /// A paired agent with no grant of its own: [`Grant::editor`] by
    /// default.
    pub paired: Grant,
    /// Paired agents' own grants, by id.
    pub agents: BTreeMap<String, Grant>,
}

impl Default for AgentGrants {
    fn default() -> Self {
        Self {
            unpaired: Grant::read_only(),
            paired: Grant::editor(),
            agents: BTreeMap::new(),
        }
    }
}

impl AgentGrants {
    /// The defaults: unpaired read-only, paired editor.
    pub fn new() -> Self {
        Self::default()
    }

    /// What a connection that said no `hello` may do.
    #[must_use]
    pub fn unpaired(mut self, grant: Grant) -> Self {
        self.unpaired = grant;
        self
    }

    /// What a paired agent with no grant of its own may do.
    #[must_use]
    pub fn paired(mut self, grant: Grant) -> Self {
        self.paired = grant;
        self
    }

    /// What the paired agent `id` may do.
    #[must_use]
    pub fn agent(mut self, id: impl Into<String>, grant: Grant) -> Self {
        self.agents.insert(id.into(), grant);
        self
    }

    /// Unpaired clients get [`Grant::full`], as before grants: for
    /// scenario runs on a throwaway profile, and clients that do not say
    /// `hello` yet (the MCP binary, `egui_pop_test::scenario::live`).
    /// The bridge's mode still applies.
    #[must_use]
    pub fn allow_unpaired_writes(self) -> Self {
        self.unpaired(Grant::full())
    }
}

#[derive(Default)]
struct State {
    grants: RefCell<AgentGrants>,
}

fn state(app: &AppHandle) -> Rc<State> {
    app.local::<State>()
}

/// The application's grants now.
pub fn grants(app: &AppHandle) -> AgentGrants {
    state(app).grants.borrow().clone()
}

/// Sets the application's grants; they apply from the next call.
pub fn set_grants(app: &AppHandle, grants: AgentGrants) {
    *state(app).grants.borrow_mut() = grants;
}

/// The grant `origin` is held to: `None` when it is not one of the
/// socket's agents.
pub fn grant_of(app: &AppHandle, origin: &Origin) -> Option<Grant> {
    let Origin::Agent(id) = origin else {
        return None;
    };
    let g = state(app).grants.borrow().clone();
    if id == crate::AGENT {
        return Some(g.unpaired);
    }
    if let Some(own) = g.agents.get(id) {
        return Some(own.clone());
    }
    crate::pairing::pairings(app)
        .iter()
        .any(|p| &p.id == id)
        .then_some(g.paired)
}

/// Whether `origin` reads private data (places, people, file paths):
/// `false` only for one of the socket's agents whose grant withholds it
/// ([`Grant::private`]). A handler whose result only partly is private
/// leaves those parts out when it is `false` (a photo list without its
/// places, say), rather than being refused whole as a command declared
/// `CommandSpec::private` is.
pub fn reads_private(app: &AppHandle, origin: &Origin) -> bool {
    grant_of(app, origin).is_none_or(|g| g.private)
}

/// `answer` as `origin` may read it: what private commands carry (their
/// arguments, results and errors in the journal, a `wait`'s event and
/// the `commands` collector's run history) is redacted
/// (`egui_pop_api::host::redact_private`) when `origin` does not read
/// private data ([`reads_private`]), and left whole otherwise. The socket
/// passes every `snapshot`, `call` and `wait` answer through it.
pub fn redacted_for(app: &AppHandle, origin: &Origin, mut answer: Json) -> Json {
    if !reads_private(app, origin) {
        egui_pop_api::host::redact_private(&mut answer);
    }
    answer
}

/// Why `origin`'s grant refuses running `command` with `args`.
fn command_refusal(app: &AppHandle, origin: &Origin, command: &str, args: &Json) -> Option<String> {
    let grant = grant_of(app, origin)?;
    let spec = CommandId::new(command).ok().and_then(|id| app.command(&id));
    let read_only = spec.as_ref().is_some_and(|s| s.read_only);
    let private = spec.as_ref().is_some_and(|s| s.private);
    grant
        .refusal(command, read_only, args)
        .or_else(|| grant.private_refusal(command, private))
}

/// Why `origin`'s grant refuses this bridge call: `commands.run` judged
/// as its command, another action by whether it changes anything.
pub(crate) fn call_refusal(
    app: &AppHandle,
    origin: &Origin,
    collector: &str,
    action: &str,
    args: &Json,
) -> Option<String> {
    if collector == "commands" && action == "run" {
        let id = args.get("id").and_then(Json::as_str)?;
        let inner = args.get("args").cloned().unwrap_or(Json::Null);
        return command_refusal(app, origin, id, &inner);
    }
    let grant = grant_of(app, origin)?;
    (grant.read_only && app.debug_mutates(collector, action, args))
        .then(|| format!("the agent is read-only: `{collector}.{action}` changes the application"))
}

/// Why `origin`'s grant refuses a `ui` action.
pub(crate) fn ui_refusal(app: &AppHandle, origin: &Origin) -> Option<String> {
    let grant = grant_of(app, origin)?;
    (!grant.ui).then(|| "the agent may not act through `ui` (click, type, keys)".into())
}

/// A refusal's message, as `pop.command.run` words its own.
fn worded(reason: &str) -> String {
    format!("refused by `{GRANTS}`: {reason}")
}

/// A bridge call refused by the grants: the agent hears why, the journal
/// says it, from the agent.
pub(crate) fn refuse_call(
    app: &AppHandle,
    origin: &Origin,
    collector: &str,
    action: &str,
    args: &Json,
    reason: &str,
    reply: Reply,
) {
    let source = origin.to_string();
    let message = worded(reason);
    match (collector, action, args.get("id").and_then(Json::as_str)) {
        ("commands", "run", Some(id)) => app.publish(
            &source,
            None,
            "command.refused",
            json!({ "command": id, "origin": source, "reason": message, "by": GRANTS, "failed": false }),
        ),
        _ => app.publish(
            &source,
            None,
            "debug.call.refused",
            json!({
                "collector": collector,
                "action": action,
                "args": args,
                "reason": message,
                "by": GRANTS,
            }),
        ),
    }
    let _ = reply.send(Err(RpcError::new(RpcError::REFUSED, message)));
}

/// The listener on `pop.command.run`.
pub(crate) fn listen(app: &AppHandle) {
    let bus = app.bus();
    bus.unlisten(&CommandRun::ID, GRANTS);
    let r = bus.listen::<CommandRun>(
        GRANTS,
        ListenOpts::new().owner(GRANTS).priority(PRIORITY),
        |ask: &CommandAsk, did| {
            let Some(app) = bus::asking() else {
                return Ok(());
            };
            match command_refusal(&app, &ask.origin, ask.command.as_str(), &ask.args) {
                Some(reason) => Err(HookError::refuse(reason)),
                None => {
                    if grant_of(&app, &ask.origin).is_some() {
                        did.push(format!("{GRANTS}: allowed"));
                    }
                    Ok(())
                }
            }
        },
    );
    if let Err(e) = r {
        log::error!(target: "egui_pop_agent", "agent grants not listening: {e:?}");
    }
}

/// Setting the grants from the application's builder.
pub trait AgentGrantsExt: Sized {
    /// The socket's agents are held to `grants` (once; later changes
    /// through [`set_grants`]).
    fn agent_grants(self, grants: AgentGrants) -> Self;
}

impl AgentGrantsExt for AppBuilder {
    fn agent_grants(self, grants: AgentGrants) -> Self {
        self.plugin(GrantsPlugin(grants))
    }
}

/// Sets the grants when the application connects its plugins.
#[derive(Debug)]
struct GrantsPlugin(AgentGrants);

impl Plugin for GrantsPlugin {
    fn id(&self) -> PluginId {
        GRANTS_PLUGIN
    }

    fn title(&self) -> String {
        "Agent grants".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        set_grants(&ctx.app(), self.0.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets() {
        let ro = Grant::read_only();
        assert_eq!(ro.refusal("about.read", true, &Json::Null), None);
        assert!(ro.refusal("test.write", false, &Json::Null).is_some());
        assert!(
            ro.refusal("value.set", false, &json!({ "path": "settings/x" }))
                .is_some()
        );
        assert!(!ro.ui);

        let ed = Grant::editor();
        assert_eq!(ed.refusal("test.write", false, &Json::Null), None);
        assert!(
            ed.refusal("value.set", false, &json!({ "path": "settings/x" }))
                .unwrap()
                .contains("settings/x")
        );
        assert!(!ed.ui);

        let full = Grant::full();
        assert_eq!(full.refusal("test.write", false, &Json::Null), None);
        assert_eq!(
            full.refusal("value.reset", false, &json!({ "path": "a/b" })),
            None
        );
        assert!(full.ui);
    }

    #[test]
    fn patterns() {
        let g = Grant::editor()
            .commands(["files.*", "about.read"])
            .values(["settings/*"]);
        assert_eq!(g.refusal("files.trash", false, &Json::Null), None);
        assert_eq!(g.refusal("about.read", true, &Json::Null), None);
        assert!(g.refusal("library.rate", false, &Json::Null).is_some());
        assert!(g.refusal("filesx", false, &Json::Null).is_some());
        assert_eq!(
            g.refusal("value.set", false, &json!({ "path": "settings/x" })),
            None
        );
        assert!(
            g.refusal("value.set", false, &json!({ "path": "layout/x" }))
                .is_some()
        );
    }

    #[test]
    fn private_data() {
        for g in [Grant::read_only(), Grant::editor(), Grant::full()] {
            assert!(g.private);
            assert_eq!(g.private_refusal("photo.places", true), None);
        }
        let g = Grant::editor().private(false);
        assert!(
            g.private_refusal("photo.places", true)
                .unwrap()
                .contains("photo.places")
        );
        assert_eq!(g.private_refusal("about.read", false), None);
        assert_eq!(g.refusal("photo.places", true, &Json::Null), None);
    }

    #[test]
    fn defaults_and_the_escape_hatch() {
        let d = AgentGrants::new();
        assert_eq!(d.unpaired, Grant::read_only());
        assert_eq!(d.paired, Grant::editor());
        assert_eq!(
            AgentGrants::new().allow_unpaired_writes().unpaired,
            Grant::full()
        );
    }
}
