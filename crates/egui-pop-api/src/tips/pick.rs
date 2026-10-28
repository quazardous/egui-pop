//! Which tip to show by itself: the rules, without egui.
//!
//! A tip is a candidate when
//! - it is not contextual (those show only where the application asks);
//! - it was not dismissed ("Got it");
//! - its plugin's tips are not turned off;
//! - its command was never used (a user who ran it knows it);
//! - its surface holds (where it makes sense);
//! - it was not shown in the last day ([`AGAIN_AFTER`]).
//!
//! Among the candidates, the one shown longest ago wins (never shown
//! first), then the first registered.

use std::collections::{BTreeMap, BTreeSet};

use super::Tip;

/// How long before a tip shown may show again, in seconds: a day.
pub const AGAIN_AFTER: f64 = 24.0 * 3600.0;

/// What the rules read.
pub struct Rules<'a> {
    /// The tips dismissed, by id.
    pub dismissed: &'a BTreeSet<String>,
    /// The commands used, by id.
    pub used: &'a BTreeSet<String>,
    /// When each tip was shown last, in seconds.
    pub seen: &'a BTreeMap<String, f64>,
    /// The origins whose tips are turned off.
    pub off: &'a BTreeSet<String>,
    /// Now, in the same seconds as `seen`.
    pub now: f64,
    /// Whether a surface (a `when` clause) holds; an empty one always
    /// does.
    pub holds: &'a dyn Fn(&str) -> bool,
}

impl std::fmt::Debug for Rules<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rules")
            .field("dismissed", self.dismissed)
            .field("used", self.used)
            .field("seen", self.seen)
            .field("off", self.off)
            .field("now", &self.now)
            .finish_non_exhaustive()
    }
}

impl Rules<'_> {
    /// Why a tip is not a candidate, or `None` when it is.
    pub fn refusal(&self, tip: &Tip) -> Option<&'static str> {
        if tip.contextual {
            return Some("contextual");
        }
        if self.dismissed.contains(&tip.id) {
            return Some("dismissed");
        }
        if self.off.contains(tip.origin_name()) {
            return Some("its plugin's tips are off");
        }
        if tip
            .command
            .as_ref()
            .is_some_and(|c| self.used.contains(c.as_str()))
        {
            return Some("its command was used");
        }
        if !tip.surface.is_empty() && !(self.holds)(&tip.surface) {
            return Some("its surface does not hold");
        }
        if self
            .seen
            .get(&tip.id)
            .is_some_and(|t| self.now - t < AGAIN_AFTER)
        {
            return Some("shown in the last day");
        }
        None
    }

    /// The tip to show among `tips`, those `keep` accepts.
    pub fn pick<'t>(&self, tips: &'t [Tip], keep: impl Fn(&Tip) -> bool) -> Option<&'t Tip> {
        tips.iter()
            .filter(|t| keep(t) && self.refusal(t).is_none())
            .min_by(|a, b| {
                let at = |t: &Tip| self.seen.get(&t.id).copied().unwrap_or(f64::NEG_INFINITY);
                // `min_by` keeps the first of equals: the first registered.
                at(a).total_cmp(&at(b))
            })
    }
}

/// `id@seconds` pairs, separated by commas, as the setting keeps them.
pub fn read_seen(text: &str) -> BTreeMap<String, f64> {
    text.split(',')
        .filter_map(|p| {
            let (id, t) = p.trim().rsplit_once('@')?;
            Some((id.to_string(), t.parse().ok()?))
        })
        .collect()
}

/// The pairs written back.
pub fn write_seen(seen: &BTreeMap<String, f64>) -> String {
    seen.iter()
        .map(|(id, t)| format!("{id}@{}", t.round()))
        .collect::<Vec<_>>()
        .join(",")
}

/// Ids separated by commas, as the settings keep them.
pub fn read_ids(text: &str) -> BTreeSet<String> {
    text.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// The ids written back.
pub fn write_ids(ids: &BTreeSet<String>) -> String {
    ids.iter().cloned().collect::<Vec<_>>().join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_pop_model::ids::CommandId;

    fn tips() -> Vec<Tip> {
        vec![
            Tip::new("p.rate", "Rate", "Press R.").command(CommandId::from_static("p.rate")),
            Tip::new("p.zoom", "Zoom", "Press Z.").surface("panel == 'viewer'"),
            Tip::new("p.compare", "Compare", "Press C."),
        ]
    }

    #[test]
    fn the_rules_choose_the_tip_shown_longest_ago_among_those_left() {
        let tips = tips();
        let none = BTreeSet::new();
        let mut seen = BTreeMap::new();
        let holds = |w: &str| w == "panel == 'viewer'";
        let rules = |used: &BTreeSet<String>, seen: &BTreeMap<String, f64>, now: f64| {
            let r = Rules {
                dismissed: &none,
                used,
                seen,
                off: &none,
                now,
                holds: &holds,
            };
            r.pick(&tips, |_| true).map(|t| t.id.clone())
        };
        // Never shown: the first registered.
        assert_eq!(rules(&none, &seen, 0.0).as_deref(), Some("p.rate"));
        // A command used: never its tip.
        let used: BTreeSet<String> = ["p.rate".to_string()].into();
        assert_eq!(rules(&used, &seen, 0.0).as_deref(), Some("p.zoom"));
        // Not twice in a day; then the one shown longest ago.
        seen.insert("p.zoom".into(), 0.0);
        seen.insert("p.compare".into(), 10.0);
        assert_eq!(rules(&used, &seen, 100.0), None);
        assert_eq!(
            rules(&used, &seen, AGAIN_AFTER + 100.0).as_deref(),
            Some("p.zoom")
        );
    }

    #[test]
    fn dismissed_off_and_elsewhere_are_never_chosen() {
        let tips = tips();
        let dismissed: BTreeSet<String> = ["p.rate".to_string()].into();
        let off: BTreeSet<String> = BTreeSet::new();
        let none = BTreeSet::new();
        let seen = BTreeMap::new();
        let nowhere = |_: &str| false;
        let r = Rules {
            dismissed: &dismissed,
            used: &none,
            seen: &seen,
            off: &off,
            now: 0.0,
            holds: &nowhere,
        };
        assert_eq!(r.refusal(&tips[0]), Some("dismissed"));
        assert_eq!(r.refusal(&tips[1]), Some("its surface does not hold"));
        assert_eq!(
            r.pick(&tips, |_| true).map(|t| t.id.as_str()),
            Some("p.compare")
        );
        let off: BTreeSet<String> = ["p".to_string()].into();
        let r = Rules { off: &off, ..r };
        assert_eq!(r.pick(&tips, |_| true), None);
    }

    #[test]
    fn the_settings_text_reads_back() {
        let seen = read_seen("a.b@12, c@3.4,broken,d@x");
        assert_eq!(seen.len(), 2);
        assert_eq!(write_seen(&seen), "a.b@12,c@3");
        assert_eq!(write_ids(&read_ids(" x, ,y ")), "x,y");
    }
}
