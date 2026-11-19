//! egui-pop's journal on the bus's (ADR 0030, slice 3): one journal per
//! process.
//!
//! When the application's bus offers the contract's
//! [`Journal`](bus_contract::Journal) capability, egui-pop's journal
//! (`AppHandle::publish`, `AppHandle::events_since`,
//! `AppHandle::last_event`, so agents' `wait` and the debug bridge's
//! journal readers) is that journal, unless the host keeps a journal of
//! its own. egui-pop's events are entries of one journaled point,
//! [`HostEvents`] (`pop.journal`), and read back as host events with
//! their own names (`command.done`, `job.*`); the bus's other entries
//! (the application's journaled facts) are read back as events named by
//! their point. One numbered sequence carries both.

use std::sync::Arc;

use bus_contract::{AnyValue, Point, PointDecl, PointId, PointKey, PointKind};
use bus_contract_journal::{JOURNALED, Journal, JournalEntry};
use serde_json::Value as Json;

use crate::host::{Event, Seq};

/// An event egui-pop publishes, as the bus's journal keeps it: who did
/// it, its name and its data (its number and cause are the entry's).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct HostEvent {
    /// Who did it: `user`, `agent:<name>`, a plugin, the host.
    pub source: String,
    /// What happened: `command.done`, `job.started`...
    pub name: String,
    pub data: Json,
}

impl HostEvent {
    pub fn new(source: impl Into<String>, name: impl Into<String>, data: Json) -> Self {
        Self {
            source: source.into(),
            name: name.into(),
            data,
        }
    }
}

/// egui-pop's events on the bus's journal (`pop.journal`): each one the
/// host journal records when it is the bus's, delivered to this point's
/// listeners by the journal's pump, after the publisher moved on.
#[derive(Debug)]
pub struct HostEvents;
impl Point for HostEvents {
    const KIND: PointKind = PointKind::Event;
    const ID: PointId = PointId::new("pop.journal");
    type Input = HostEvent;
    type Value = ();
}

/// The declaration of [`HostEvents`]: journaled, so declared only on a
/// bus offering the journal capability (it is not in
/// [`CONTRACT`](super::CONTRACT), which every bus takes).
pub const HOST_JOURNAL: PointDecl = PointDecl::of::<HostEvents>(
    "egui-pop's journal on the bus's: each event egui-pop publishes (a command dispatched, done or failed, a job's progress, a link, an agent's session), recorded with a number shared with the application's journaled facts.",
)
.facets(&[JOURNALED]);

/// Records an event on the bus's journal; its number, or why not.
pub(crate) fn publish(
    journal: &dyn Journal,
    source: &str,
    cause: Option<Seq>,
    name: &str,
    data: Json,
) -> Result<Seq, bus_contract::Refused> {
    journal.publish_any(
        PointKey::of::<HostEvents>(),
        Arc::new(HostEvent::new(source, name, data)),
        fresh,
        cause,
    )
}

/// The value each listener of [`HostEvents`] fills: none.
fn fresh() -> AnyValue {
    Box::new(())
}

/// An entry of the bus's journal, as egui-pop's journal shows it: an
/// event egui-pop published keeps its source, name and data; any other
/// entry is named by its point (its kind), its source the process it was
/// relayed from or `bus`, its data its text read as JSON (a string when
/// it is not JSON, null when it has none).
pub(crate) fn event_of(entry: &JournalEntry) -> Event {
    if let Some(e) = entry.read::<HostEvents>() {
        return Event {
            seq: entry.seq,
            source: e.source.clone(),
            cause: entry.cause,
            name: e.name.clone(),
            data: e.data.clone(),
        };
    }
    Event {
        seq: entry.seq,
        source: entry
            .origin
            .as_ref()
            .map_or_else(|| "bus".into(), |o| o.process.clone()),
        cause: entry.cause,
        name: entry.kind.clone(),
        data: entry.text.as_deref().map_or(Json::Null, |t| {
            serde_json::from_str(t).unwrap_or_else(|_| Json::String(t.into()))
        }),
    }
}
