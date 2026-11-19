//! Pairing: an agent names itself on the socket (`hello`), the user
//! approves it once, and its calls run as `Origin::Agent("<id>")` from
//! then on.
//!
//! - A `hello` with a `name` and no `token` asks the user, through a
//!   confirmation dialog naming the agent; the request is held until the
//!   user answers (or its deadline passes). Approved, a pairing is
//!   stored and its token handed to the agent, once: the pairings keep
//!   only its SHA-256.
//! - A `hello` with a `token` is paired at once when the token is
//!   known, refused otherwise.
//! - A connection that sends no `hello` stays
//!   `Origin::Agent("agent-socket")`, with the same rights.
//!
//! Only the user answers the dialog: its answer command
//! ([`CMD_CONFIRM`]) is kept from agents, and an agent's accept through
//! `dialogs.answer` is vetoed (its cancel goes through).
//!
//! The token says which agent calls, for the journal and the histories;
//! it does not isolate agents from one another: any process of the user
//! may read what another one keeps.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use egui_pop_api::dialogs::{self, Confirm, DIALOGS};
use egui_pop_api::steps::Before;
use egui_pop_api::{AppHandle, Ctx, PluginError};
use egui_pop_core::debug::BridgeMode;
use egui_pop_core::strings;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::CommandId;
use serde_json::{Value as Json, json};
use sha2::{Digest as _, Sha256};

use crate::Reply;
use crate::rpc::RpcError;
use crate::strings as txt;

/// The user's answer to an agent's request: a pairing (`id`) or a
/// destructive call ([`crate::confirm`], `run`); kept from agents and the
/// palette.
pub const CMD_CONFIRM: CommandId = CommandId::from_static("pop.agent.confirm");
/// The document the pairings are kept in, in the application's
/// persistence directory (`AppBuilder::persist`); without one, they
/// last as long as the application runs.
pub const PAIRINGS_DOCUMENT: &str = "agent-pairings";
/// On the journal: the user paired an agent (`agent`, `name`).
pub const EVENT_PAIRED: &str = "agent.paired";
/// On the journal: the user declined to pair an agent (`name`), or the
/// request was dropped unanswered.
pub const EVENT_DECLINED: &str = "agent.declined";
/// On the journal: a connection said `hello` as a paired agent
/// (`agent`, `name`).
pub const EVENT_CONNECTED: &str = "agent.connected";
/// On the journal: a paired agent's connection closed (`agent`).
pub const EVENT_DISCONNECTED: &str = "agent.disconnected";
/// On the journal: the user revoked a pairing (`agent`, `name`, and
/// `closed`, how many of its connections closed).
pub const EVENT_REVOKED: &str = "agent.revoked";
/// The paired agents and the defaults, with their grants, for the
/// Agents page: `{ "agents": [...], "defaults": [...] }` (read-only).
pub const CMD_LIST: CommandId = CommandId::from_static("pop.agent.list");
/// Sets a grant to a preset (`read-only`, `editor`, `full`): a paired
/// agent's (`{ "agent": id, "preset" }`) or a default's (`{ "default":
/// "unpaired" | "paired", "preset" }`); the user's only.
pub const CMD_GRANT: CommandId = CommandId::from_static("pop.agent.grant");
/// Revokes a pairing (`{ "agent": id }`): its token stops pairing and
/// its connections close; the user's only.
pub const CMD_REVOKE: CommandId = CommandId::from_static("pop.agent.revoke");
/// The longest agent name, in characters.
pub const MAX_NAME_CHARS: usize = 64;
/// How long a `hello` waits for the user's answer when it gives no
/// `deadline_ms`, and the longest it may wait.
pub const HELLO_DEADLINE: Duration = Duration::from_secs(120);

/// The ids an agent may not take: the socket's own, and words that
/// would read as someone else.
const RESERVED: &[&str] = &[crate::AGENT, "anonymous", "user", "debug-bridge"];

/// A paired agent.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Pairing {
    /// Its id, in its calls' origin: `Origin::Agent(id)`.
    pub id: String,
    /// The name it gave.
    pub name: String,
    /// When the user paired it, in seconds since the Unix epoch.
    pub paired_at: u64,
    /// When it last asked something, in seconds since the Unix epoch;
    /// 0 when never since it was paired.
    pub last_seen: u64,
}

/// A pairing as kept: its token's SHA-256, never the token.
#[derive(Clone, Debug)]
struct Kept {
    pairing: Pairing,
    token_sha256: String,
}

/// A `hello` waiting for the user's answer.
struct Asking {
    name: String,
    deadline: Instant,
    reply: Reply,
    dialog: u64,
    /// Accepted: its command runs next.
    answered: bool,
}

#[derive(Default)]
struct State {
    loaded: Cell<bool>,
    kept: RefCell<Vec<Kept>>,
    next: Cell<u64>,
    asking: RefCell<BTreeMap<u64, Asking>>,
}

fn state(app: &AppHandle) -> Rc<State> {
    app.local::<State>()
}

/// The agents paired so far, oldest first.
pub fn pairings(app: &AppHandle) -> Vec<Pairing> {
    load(app);
    state(app)
        .kept
        .borrow()
        .iter()
        .map(|k| k.pairing.clone())
        .collect()
}

/// Reads the pairings once, from the persistence directory.
fn load(app: &AppHandle) {
    let st = state(app);
    if st.loaded.replace(true) {
        return;
    }
    let Some(persist) = app.persist() else {
        return;
    };
    let path = persist.path(PAIRINGS_DOCUMENT);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            log::error!(target: "egui_pop_agent", "agent pairings not read from {}: {e}", path.display());
            return;
        }
    };
    match parse(&text) {
        Ok(kept) => *st.kept.borrow_mut() = kept,
        Err(e) => {
            log::error!(target: "egui_pop_agent", "agent pairings not read from {}: {e}", path.display());
        }
    }
}

/// The document's pairings.
fn parse(text: &str) -> Result<Vec<Kept>, String> {
    let doc: Json = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let list = doc["pairings"].as_array().ok_or("no `pairings` list")?;
    list.iter()
        .map(|p| {
            let s = |k: &str| {
                p[k].as_str()
                    .map(String::from)
                    .ok_or_else(|| format!("a pairing without `{k}`"))
            };
            Ok(Kept {
                pairing: Pairing {
                    id: s("id")?,
                    name: s("name")?,
                    paired_at: p["paired_at"].as_u64().unwrap_or(0),
                    last_seen: p["last_seen"].as_u64().unwrap_or(0),
                },
                token_sha256: s("token_sha256")?,
            })
        })
        .collect()
}

/// The document's text.
fn render(kept: &[Kept]) -> String {
    let list: Vec<Json> = kept
        .iter()
        .map(|k| {
            json!({
                "id": k.pairing.id,
                "name": k.pairing.name,
                "paired_at": k.pairing.paired_at,
                "last_seen": k.pairing.last_seen,
                "token_sha256": k.token_sha256,
            })
        })
        .collect();
    serde_json::to_string_pretty(&json!({ "version": 1, "pairings": list })).unwrap_or_default()
}

/// Writes the pairings now (a pairing is rare and must survive a crash).
fn save(app: &AppHandle) {
    let Some(persist) = app.persist() else {
        return;
    };
    let text = render(&state(app).kept.borrow());
    if let Err(e) = persist.save_text(PAIRINGS_DOCUMENT, &text) {
        log::error!(target: "egui_pop_agent", "agent pairings not saved: {e}");
    }
}

/// A new token: 32 random bytes from the system, in hex, prefixed.
fn new_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| format!("no randomness for a token: {e}"))?;
    Ok(format!("egp_{}", hex(&bytes)))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn sha256(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

/// An id from an agent's name: lower case letters, digits, `.`, `-` and
/// `_`, at most 32 characters, not a reserved one, not one taken.
fn id_for(name: &str, taken: &[Kept]) -> String {
    let mut base: String = name
        .trim()
        .chars()
        .map(|c| c.to_ascii_lowercase())
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect();
    base = base.trim_matches('-').chars().take(32).collect();
    if base.is_empty() {
        base = "agent".into();
    }
    let free = |id: &str| !RESERVED.contains(&id) && !taken.iter().any(|k| k.pairing.id == id);
    if free(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|id| free(id))
        .unwrap_or(base)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The journal's source for an agent.
fn source(id: &str) -> String {
    Origin::Agent(id.into()).to_string()
}

/// What `hello` answers once paired.
fn hello_result(p: &Pairing, token: Option<&str>) -> Json {
    let mut r = json!({
        "agent": p.id,
        "name": p.name,
        "origin": source(&p.id),
        "paired": true,
    });
    if let Some(t) = token {
        r["token"] = json!(t);
    }
    r
}

/// A `hello`, on the UI thread: answered at once for a known token,
/// held until the user answers for a new agent.
pub(crate) fn hello(
    app: &AppHandle,
    name: String,
    token: Option<String>,
    deadline: Instant,
    reply: Reply,
) {
    if app.debug_bridge_mode() == BridgeMode::Off {
        let _ = reply.send(Err(RpcError::off()));
        return;
    }
    load(app);
    let st = state(app);
    if let Some(token) = token {
        let hash = sha256(&token);
        let found = st
            .kept
            .borrow()
            .iter()
            .find(|k| k.token_sha256 == hash)
            .map(|k| k.pairing.clone());
        let _ = reply.send(match found {
            Some(p) => {
                seen(app, &Origin::Agent(p.id.clone()));
                app.publish(
                    &source(&p.id),
                    None,
                    EVENT_CONNECTED,
                    json!({ "agent": p.id, "name": p.name }),
                );
                Ok(hello_result(&p, None))
            }
            None => Err(RpcError::new(
                RpcError::REFUSED,
                "unknown pairing token: say `hello` without a token to be paired again",
            )),
        });
        return;
    }
    let Some(dialogs) = app.service(&DIALOGS) else {
        let _ = reply.send(Err(RpcError::new(
            RpcError::REFUSED,
            "pairing needs the user's approval, and the dialogs plugin is not connected",
        )));
        return;
    };
    let n = st.next.get() + 1;
    st.next.set(n);
    let id = id_for(&name, &st.kept.borrow());
    let ctx = app.egui();
    let title = strings::tr_args(&ctx, &txt::PAIR_TITLE, &[("name", &name)]);
    let detail = strings::tr_args(&ctx, &txt::PAIR_DETAIL, &[("id", &id)]);
    let question = Confirm::new(&title, &strings::tr(&ctx, &txt::PAIR_VERB), CMD_CONFIRM)
        .detail(&detail)
        .args(json!({ "id": n }));
    let dialog = dialogs.open(question);
    st.asking.borrow_mut().insert(
        n,
        Asking {
            name,
            deadline,
            reply,
            dialog,
            answered: false,
        },
    );
    app.egui().request_repaint();
}

/// The user's answer: the agent is paired, its token handed to it.
fn confirm(app: &AppHandle, args: &Json, origin: &Origin) -> Result<Json, String> {
    if *origin != Origin::User {
        return Err("only the user answers an agent's request".into());
    }
    if let Some(n) = args["run"].as_u64() {
        return crate::confirm::accept(app, n);
    }
    let n = args["id"].as_u64().ok_or("`id` is missing")?;
    let st = state(app);
    let asking = st
        .asking
        .borrow_mut()
        .remove(&n)
        .ok_or_else(|| format!("no agent waits to be paired under {n}"))?;
    load(app);
    let token = match new_token() {
        Ok(t) => t,
        Err(e) => {
            let _ = asking
                .reply
                .send(Err(RpcError::new(RpcError::REFUSED, e.clone())));
            return Err(e);
        }
    };
    let now = now_secs();
    let pairing = Pairing {
        id: id_for(&asking.name, &st.kept.borrow()),
        name: asking.name,
        paired_at: now,
        last_seen: now,
    };
    st.kept.borrow_mut().push(Kept {
        pairing: pairing.clone(),
        token_sha256: sha256(&token),
    });
    save(app);
    let data = json!({ "agent": pairing.id, "name": pairing.name });
    app.publish(&source(&pairing.id), None, EVENT_PAIRED, data.clone());
    app.publish(&source(&pairing.id), None, EVENT_CONNECTED, data.clone());
    let _ = asking.reply.send(Ok(hello_result(&pairing, Some(&token))));
    // The token goes to the agent only: not in the command's result,
    // which the journal shows.
    Ok(data)
}

/// A request declined (or dropped unanswered): the agent hears why, and
/// its dialog, if still open, is cancelled.
fn declined(app: &AppHandle, n: u64, error: RpcError) {
    let Some(asking) = state(app).asking.borrow_mut().remove(&n) else {
        return;
    };
    crate::confirm::close(app, asking.dialog, Origin::Agent(crate::AGENT.into()));
    app.publish(
        "user",
        None,
        EVENT_DECLINED,
        json!({ "name": asking.name, "reason": error.message }),
    );
    let _ = asking.reply.send(Err(error));
}

/// Each pass: requests past their deadline end, and those whose dialog
/// closed without an answer are declined.
pub(crate) fn pass(app: &AppHandle) {
    let st = state(app);
    if st.asking.borrow().is_empty() {
        return;
    }
    let now = Instant::now();
    let open = app.service(&DIALOGS);
    let ended: Vec<(u64, RpcError)> = st
        .asking
        .borrow()
        .iter()
        .filter_map(|(n, a)| {
            if now >= a.deadline {
                Some((
                    *n,
                    RpcError::new(
                        RpcError::NO_ANSWER,
                        "the user did not answer the pairing request in time",
                    ),
                ))
            } else if !a.answered && open.as_ref().is_some_and(|d| !d.is_open(a.dialog)) {
                Some((*n, declined_error(&a.name)))
            } else {
                None
            }
        })
        .collect();
    for (n, e) in ended {
        declined(app, n, e);
    }
}

fn declined_error(name: &str) -> RpcError {
    RpcError::new(
        RpcError::REFUSED,
        format!("the user declined to pair “{name}”"),
    )
}

/// Whether a pairing request waits for the user's answer, its dialog
/// open.
pub(crate) fn asking(app: &AppHandle, dialogs: &dialogs::Dialogs) -> bool {
    state(app)
        .asking
        .borrow()
        .values()
        .any(|a| !a.answered && dialogs.is_open(a.dialog))
}

/// The socket is off: the requests held end.
pub(crate) fn refuse_all(app: &AppHandle) {
    let held: Vec<u64> = state(app).asking.borrow().keys().copied().collect();
    for n in held {
        declined(app, n, RpcError::stopped());
    }
}

/// A paired agent's connection closed: when it was last seen is kept.
pub(crate) fn gone(app: &AppHandle, agent: &str) {
    if seen(app, &Origin::Agent(agent.into())) {
        save(app);
    }
    app.publish(
        &source(agent),
        None,
        EVENT_DISCONNECTED,
        json!({ "agent": agent }),
    );
}

/// A paired agent asked something now; whether it is one.
pub(crate) fn seen(app: &AppHandle, origin: &Origin) -> bool {
    let Origin::Agent(id) = origin else {
        return false;
    };
    let st = state(app);
    let mut kept = st.kept.borrow_mut();
    match kept.iter_mut().find(|k| &k.pairing.id == id) {
        Some(k) => {
            k.pairing.last_seen = now_secs();
            true
        }
        None => false,
    }
}

/// How many connections the paired agent `id` has open now.
pub fn connected(app: &AppHandle, id: &str) -> usize {
    crate::live::of(app).count(id)
}

/// Revokes the pairing of `id`, as the user: its stored token hash is
/// removed (the token no longer pairs), its own grant dropped, its open
/// connections closed, and the journal says so ([`EVENT_REVOKED`]).
/// How many connections closed.
pub fn revoke(app: &AppHandle, id: &str) -> Result<usize, String> {
    load(app);
    let st = state(app);
    let gone = {
        let mut kept = st.kept.borrow_mut();
        let i = kept
            .iter()
            .position(|k| k.pairing.id == id)
            .ok_or_else(|| format!("no agent is paired as `{id}`"))?;
        kept.remove(i).pairing
    };
    save(app);
    let mut g = crate::grants::grants(app);
    if g.agents.remove(id).is_some() {
        crate::grants::set_grants(app, g);
    }
    let closed = crate::live::of(app).revoke(id);
    app.publish(
        "user",
        None,
        EVENT_REVOKED,
        json!({ "agent": gone.id, "name": gone.name, "closed": closed }),
    );
    Ok(closed)
}

/// The answer command, and the dialog's answers held to the user (the
/// pairing requests' and the destructive calls').
pub(crate) fn register(ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
    ctx.command(
        CommandSpec::new(CMD_CONFIRM, "Answer an agent's request", "Agents")
            .description(
                "The user's answer to an agent's request: to be paired (`id`), or to run a destructive call (`run`).",
            )
            .args(json!({ "id": 1 }))
            .hidden()
            .no_agent(),
        |call| confirm(call.app, call.args, call.origin),
    )?;
    if !ctx.is_connected(&dialogs::PLUGIN) {
        return Ok(());
    }
    ctx.before(dialogs::STEP_ANSWER, |_, args| {
        let ours = args["dialog"]["command"].as_str() == Some(CMD_CONFIRM.as_str());
        let cancel = args["answer"]["answer"].as_str() == Some("cancel");
        if ours && !cancel && args["origin"].as_str() != Some("user") {
            Before::Veto("only the user answers an agent's request".into())
        } else {
            Before::Go
        }
    })
    .after(dialogs::STEP_ANSWER, |app, args, result| {
        if args["dialog"]["command"].as_str() != Some(CMD_CONFIRM.as_str()) || result.is_err() {
            return;
        }
        let cancel = args["answer"]["answer"].as_str() == Some("cancel");
        if let Some(n) = args["dialog"]["args"]["run"].as_u64() {
            crate::confirm::answered(app, n, cancel);
            return;
        }
        let Some(n) = args["dialog"]["args"]["id"].as_u64() else {
            return;
        };
        if cancel {
            let name = state(app)
                .asking
                .borrow()
                .get(&n)
                .map(|a| a.name.clone())
                .unwrap_or_default();
            declined(app, n, declined_error(&name));
        } else if let Some(a) = state(app).asking.borrow_mut().get_mut(&n) {
            a.answered = true;
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(id: &str) -> Kept {
        Kept {
            pairing: Pairing {
                id: id.into(),
                name: id.into(),
                paired_at: 0,
                last_seen: 0,
            },
            token_sha256: String::new(),
        }
    }

    #[test]
    fn ids_are_tidy_free_and_not_reserved() {
        assert_eq!(id_for("Claude Code", &[]), "claude-code");
        assert_eq!(id_for("  ", &[]), "agent");
        assert_eq!(id_for("agent-socket", &[]), "agent-socket-2");
        assert_eq!(id_for("User", &[]), "user-2");
        assert_eq!(id_for("bot", &[kept("bot"), kept("bot-2")]), "bot-3");
        assert_eq!(id_for(&"x".repeat(80), &[]).len(), 32);
    }

    #[test]
    fn tokens_are_random_and_kept_hashed() {
        let a = new_token().unwrap();
        let b = new_token().unwrap();
        assert_ne!(a, b);
        assert!(a.starts_with("egp_") && a.len() == 4 + 64, "{a}");
        let mut k = kept("bot");
        k.token_sha256 = sha256(&a);
        let text = render(&[k]);
        assert!(!text.contains(&a), "the token is never written");
        let back = parse(&text).unwrap();
        assert_eq!(back[0].pairing.id, "bot");
        assert_eq!(back[0].token_sha256, sha256(&a));
    }
}
