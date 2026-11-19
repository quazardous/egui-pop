//! Passes over a point's budget ([`bus_contract_timings::Timings`]): heard from
//! the bus's budget sink on whatever thread ran them, said on the
//! interface thread as the `bus.overBudget` fact and one warning in the
//! log, at most once a minute per point (the passes not said are
//! counted in the next one). A budget is not a deadline: nothing fails.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use bus_contract_timings::{
    BusTimings, ListenerTimings, OVER_BUDGET_FACT, OverBudget, PointTimings, TimingStats,
};
use serde_json::{Value as Json, json};
use web_time::Instant;

use crate::app::AppHandle;

/// How often a point's passes over budget are said, at most.
pub(crate) const SAY_EVERY: Duration = Duration::from_secs(60);

/// Passes heard and not said yet are dropped past this many (a point
/// over budget on every emit, between two passes of the interface).
const QUEUE: usize = 256;

/// What the bus's budget sink heard, and when each point was last said.
#[derive(Default)]
pub(crate) struct Heard {
    queue: Arc<Mutex<Vec<OverBudget>>>,
    /// Per point: when it was last said and the passes over budget not
    /// said since.
    said: RefCell<HashMap<&'static str, (Instant, u64)>>,
}

/// Listens to the bus's passes over budget, when the bus offers
/// [`bus_contract_timings::Timings`].
pub(crate) fn install(kernel: &dyn bus_contract::Bus) -> Heard {
    let heard = Heard::default();
    if let Some(timings) = kernel.timings() {
        let queue = heard.queue.clone();
        timings.on_over_budget(move |over| {
            let mut q = queue.lock().unwrap_or_else(PoisonError::into_inner);
            if q.len() < QUEUE {
                q.push(over.clone());
            }
        });
    }
    heard
}

/// A time in whole microseconds, for the fact's data.
pub(crate) fn us(d: Duration) -> u64 {
    u64::try_from(d.as_micros()).unwrap_or(u64::MAX)
}

/// Timing stats as the debug collector writes them, in µs.
pub(crate) fn stats(s: &TimingStats) -> Json {
    json!({
        "count": s.count,
        "total_us": us(s.total),
        "p50_us": us(s.p50),
        "p95_us": us(s.p95),
        "max_us": us(s.max),
    })
}

/// Every listener timed, the slowest first by p95 then max: "who costs
/// what", across points.
pub(crate) fn slowest(points: &[PointTimings]) -> Json {
    let mut all: Vec<(&'static str, &ListenerTimings)> = points
        .iter()
        .flat_map(|p| p.listeners.iter().map(move |l| (p.point, l)))
        .collect();
    all.sort_by(|a, b| {
        (b.1.stats.p95, b.1.stats.max)
            .cmp(&(a.1.stats.p95, a.1.stats.max))
            .then_with(|| (a.0, &a.1.id).cmp(&(b.0, &b.1.id)))
    });
    Json::Array(
        all.into_iter()
            .map(|(point, l)| {
                let mut j = stats(&l.stats);
                j["point"] = json!(point);
                j["id"] = json!(l.id);
                j["owner"] = json!(l.owner);
                j
            })
            .collect(),
    )
}

/// The data of a `bus.overBudget` fact.
pub(crate) fn data(over: &OverBudget, unsaid: u64) -> Json {
    let slowest = over.slowest();
    json!({
        "point": over.point,
        "took_us": us(over.took),
        "budget_us": us(over.budget),
        "slowest": slowest.map(|(id, _)| id),
        "slowest_us": slowest.map(|(_, t)| us(t)),
        "listeners": over
            .listeners
            .iter()
            .map(|(id, t)| json!({ "id": id, "us": us(*t) }))
            .collect::<Vec<_>>(),
        "unsaid": unsaid,
    })
}

impl AppHandle {
    /// Says the passes over a point's budget heard since the last call:
    /// each point at most once a minute ([`SAY_EVERY`]), as the
    /// `bus.overBudget` fact (point, its time and budget, each listener's
    /// time, the slowest, and `unsaid`: the passes over budget not said
    /// since the last one) and one warning in the log. The application
    /// calls it once per pass.
    pub(crate) fn say_over_budget(&self) {
        let heard = &self.0.bus.over_budget;
        let taken =
            std::mem::take(&mut *heard.queue.lock().unwrap_or_else(PoisonError::into_inner));
        if taken.is_empty() {
            return;
        }
        let now = Instant::now();
        for over in taken {
            let unsaid = {
                let mut said = heard.said.borrow_mut();
                match said.get_mut(over.point) {
                    Some((at, skipped)) if now.duration_since(*at) < SAY_EVERY => {
                        *skipped += 1;
                        continue;
                    }
                    Some((at, skipped)) => {
                        *at = now;
                        std::mem::take(skipped)
                    }
                    None => {
                        said.insert(over.point, (now, 0));
                        0
                    }
                }
            };
            log::warn!(target: "egui_pop", "{over}");
            self.publish("egui_pop", None, OVER_BUDGET_FACT, data(&over, unsaid));
        }
    }
}
