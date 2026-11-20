//! The journal a problem report attaches: a time window of what the
//! application did, redacted, and the bundle `help.report` shows before
//! anything leaves the machine.
//!
//! A [`Recorder`] follows the journal (`AppHandle::events_since`) once a
//! pass and keeps the entries of the last few minutes
//! ([`RecorderConfig::window`], 5 by default): commands and their
//! outcomes, refusals, facts, the application's summary events. It keeps
//! its own copy rather than reading the shared journal at report time:
//! that journal is a ring of a fixed count, where a burst of frequent
//! entries pushes the rare ones out; a time window keeps a rare entry as
//! long as it is recent. Kinds that come at every pass or at a high rate
//! are skipped ([`RecorderConfig::skip`], patterns as the journal reads
//! them: `pop.pointer.*`, `*`).
//!
//! Each entry is redacted as it is kept ([`Redactor`]): a private
//! command's arguments, result and error read `<private>`
//! (`host::redact_private`); every string has the home folder written
//! `~`, every other absolute path reduced to its root and its extension
//! (`/…/*.raw`), and any secret of the environment blanked.
//!
//! ```
//! use std::time::Duration;
//! use egui_pop_api::about::journal::{Recorder, RecorderConfig, Redactor};
//! use egui_pop_api::host::Event;
//! use serde_json::json;
//! use web_time::Instant;
//!
//! let redactor = Redactor::new(Some("/home/ann"), &[]);
//! let mut rec = Recorder::new(RecorderConfig::default().skip("view.frame"));
//! let t0 = Instant::now();
//! let event = |seq, name: &str, data| Event {
//!     seq, source: "user".into(), cause: None, name: name.into(), data,
//! };
//! rec.record(t0, &[
//!     event(1, "command.dispatched", json!({ "command": "file.open", "args": { "path": "/home/ann/Photos/a.raw" } })),
//!     event(2, "view.frame", json!(null)),
//! ], &redactor);
//! let kept = rec.window(t0 + Duration::from_secs(1));
//! assert_eq!(kept.len(), 1);
//! assert_eq!(kept[0].data["args"]["path"], "~/….raw");
//! // Past the window, it goes.
//! assert!(rec.window(t0 + Duration::from_secs(301)).is_empty());
//! ```

use std::collections::VecDeque;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value as Json;
use web_time::Instant;

use crate::host::{Event, Seq, redact_private};

/// How long a report's journal reaches back by default.
pub const WINDOW: Duration = Duration::from_secs(5 * 60);

/// The most entries a recorder keeps, whatever their age: a bound on its
/// memory, far above what a few minutes of skipped-free use give.
pub const CAP: usize = 20_000;

/// What stands for a secret in a report.
pub const SECRET: &str = "[redacted]";

/// What a recorder keeps and for how long.
#[derive(Clone, Debug, PartialEq)]
pub struct RecorderConfig {
    /// How far back the window reaches.
    pub window: Duration,
    /// Kinds never kept: per-render or high-rate ones (journal patterns:
    /// an id, a prefix ending `.*`, or `*`).
    pub skip: Vec<String>,
    /// The most entries kept ([`CAP`]).
    pub cap: usize,
}

impl Default for RecorderConfig {
    fn default() -> Self {
        Self {
            window: WINDOW,
            skip: Vec::new(),
            cap: CAP,
        }
    }
}

impl RecorderConfig {
    /// How far back the window reaches (at least a second).
    #[must_use]
    pub fn window(mut self, window: Duration) -> Self {
        self.window = window.max(Duration::from_secs(1));
        self
    }

    /// A kind, or a pattern of kinds, never kept.
    #[must_use]
    pub fn skip(mut self, pattern: &str) -> Self {
        self.skip.push(pattern.into());
        self
    }

    /// The most entries kept (at least one).
    #[must_use]
    pub fn cap(mut self, cap: usize) -> Self {
        self.cap = cap.max(1);
        self
    }

    /// Whether entries of `kind` are skipped.
    pub fn skips(&self, kind: &str) -> bool {
        self.skip
            .iter()
            .any(|p| crate::bus::bus_contract_journal::kind_matches(p, kind))
    }
}

/// An entry of a report's journal: a journal event, redacted, with its
/// age when the report was made.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Recorded {
    pub seq: Seq,
    /// Milliseconds before the report.
    pub ago_ms: u64,
    pub source: String,
    pub cause: Option<Seq>,
    pub name: String,
    pub data: Json,
}

/// The journal's recent entries, kept by time.
#[derive(Debug)]
pub struct Recorder {
    config: RecorderConfig,
    entries: VecDeque<(Instant, Event)>,
    /// The last sequence number seen.
    last: Seq,
    /// Entries the recorder never saw: the journal dropped them between
    /// two reads.
    missed: u64,
    /// Entries let go for the cap, not for their age.
    capped: u64,
}

impl Recorder {
    pub fn new(config: RecorderConfig) -> Self {
        Self {
            config,
            entries: VecDeque::new(),
            last: 0,
            missed: 0,
            capped: 0,
        }
    }

    pub fn config(&self) -> &RecorderConfig {
        &self.config
    }

    /// The last sequence number seen: the next read starts after it.
    pub fn last(&self) -> Seq {
        self.last
    }

    /// Entries the recorder never saw (the journal had dropped them
    /// before it read) and entries let go for the cap.
    pub fn lost(&self) -> (u64, u64) {
        (self.missed, self.capped)
    }

    /// Keeps the events read at `now` (oldest first) that are not
    /// skipped, redacted, and lets go of those past the window.
    pub fn record(&mut self, now: Instant, events: &[Event], redactor: &Redactor) {
        for e in events {
            if e.seq <= self.last {
                continue;
            }
            // The first read takes what the journal still has; after it,
            // a hole in the numbers is entries dropped before this read.
            if self.last > 0 && e.seq > self.last + 1 {
                self.missed += e.seq - self.last - 1;
            }
            self.last = e.seq;
            if self.config.skips(&e.name) {
                continue;
            }
            if self.entries.len() >= self.config.cap {
                self.entries.pop_front();
                self.capped += 1;
            }
            self.entries.push_back((now, redactor.event(e)));
        }
        self.prune(now);
    }

    /// Lets go of the entries past the window.
    pub fn prune(&mut self, now: Instant) {
        let window = self.config.window;
        while let Some((at, _)) = self.entries.front() {
            if now.saturating_duration_since(*at) > window {
                self.entries.pop_front();
            } else {
                break;
            }
        }
    }

    /// The entries of the window at `now`, oldest first.
    pub fn window(&self, now: Instant) -> Vec<Recorded> {
        let window = self.config.window;
        self.entries
            .iter()
            .filter(|(at, _)| now.saturating_duration_since(*at) <= window)
            .map(|(at, e)| Recorded {
                seq: e.seq,
                ago_ms: u64::try_from(now.saturating_duration_since(*at).as_millis())
                    .unwrap_or(u64::MAX),
                source: e.source.clone(),
                cause: e.cause,
                name: e.name.clone(),
                data: e.data.clone(),
            })
            .collect()
    }
}

/// Hides what a report must not carry: private commands' data, the home
/// folder's name, absolute paths, the environment's secrets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Redactor {
    /// Written `~`.
    home: Option<String>,
    /// Values never written, wherever they appear.
    secrets: Vec<String>,
}

impl Redactor {
    /// A redactor writing `home` as `~` and blanking `secrets` (values
    /// shorter than 8 characters are too common to blank and are left).
    pub fn new(home: Option<&str>, secrets: &[&str]) -> Self {
        let home = home
            .map(|h| h.trim_end_matches(['/', '\\']).to_string())
            .filter(|h| !h.is_empty());
        let mut secrets: Vec<String> = secrets
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| s.len() >= 8)
            .collect();
        // The longest first, so one inside another is blanked whole.
        secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
        Self { home, secrets }
    }

    /// From an environment: its home folder, and the values of its
    /// variables named like a secret (a token, a key, a password, a
    /// client id or secret, a credential).
    pub fn from_env(vars: &[(String, String)], home: Option<&str>) -> Self {
        let secrets: Vec<&str> = vars
            .iter()
            .filter(|(n, _)| secret_name(n))
            .map(|(_, v)| v.as_str())
            .collect();
        Self::new(home, &secrets)
    }

    /// This process's environment (no home and no variables on the web).
    pub fn current() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let vars: Vec<(String, String)> = std::env::vars().collect();
            let home = std::env::home_dir();
            Self::from_env(&vars, home.as_deref().and_then(|h| h.to_str()))
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self::default()
        }
    }

    /// A journal event, redacted: a private command's data hidden, then
    /// every string of its data and its source scrubbed.
    pub fn event(&self, e: &Event) -> Event {
        let mut v = serde_json::json!({ "seq": e.seq, "name": e.name, "data": e.data });
        redact_private(&mut v);
        let mut data = v.get_mut("data").map(Json::take).unwrap_or(Json::Null);
        self.json(&mut data);
        Event {
            seq: e.seq,
            source: self.text(&e.source),
            cause: e.cause,
            name: e.name.clone(),
            data,
        }
    }

    /// Scrubs every string of `v`, keys included.
    pub fn json(&self, v: &mut Json) {
        match v {
            Json::String(s) => *s = self.value(s),
            Json::Array(items) => items.iter_mut().for_each(|x| self.json(x)),
            Json::Object(o) => {
                let taken = std::mem::take(o);
                for (k, mut x) in taken {
                    self.json(&mut x);
                    o.insert(self.text(&k), x);
                }
            }
            _ => {}
        }
    }

    /// A string value: reduced whole when it is one path alone (spaces
    /// in its names do not end it), else as [`Redactor::text`].
    pub fn value(&self, value: &str) -> String {
        let v = value.trim();
        let b = v.as_bytes();
        let drive = b.len() > 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && (b[2] == b'\\' || b[2] == b'/');
        let rooted = v.starts_with("file:")
            || v.starts_with("~/")
            || v.starts_with("~\\")
            || (v.starts_with('/') && v.len() > 1)
            || drive;
        if rooted && !v.contains(['\n', '"']) && !v.contains(": ") {
            let masked = self.unsecret(v).replace(' ', "\u{1}");
            return scrub_paths(&masked).replace('\u{1}', " ");
        }
        self.text(value)
    }

    /// `text` with no secret and the home folder written `~`.
    fn unsecret(&self, text: &str) -> String {
        let mut out = text.to_string();
        for s in &self.secrets {
            if out.contains(s.as_str()) {
                out = out.replace(s.as_str(), SECRET);
            }
        }
        match &self.home {
            Some(home) => home_to_tilde(&out, home),
            None => out,
        }
    }

    /// `text` with no secret, the home folder written `~` and every
    /// absolute path reduced to its root and its extension.
    pub fn text(&self, text: &str) -> String {
        scrub_paths(&self.unsecret(text))
    }
}

/// A variable whose value is a secret, or close enough to one never to be
/// copied.
fn secret_name(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    [
        "TOKEN",
        "SECRET",
        "CLIENT_ID",
        "PASSWORD",
        "PASSWD",
        "KEY",
        "CREDENTIAL",
        "AUTH",
    ]
    .iter()
    .any(|s| n.contains(s))
}

/// `home` written `~`, only as a whole folder (`/home/ann` in
/// `/home/ann/x`, not in `/home/anna`).
fn home_to_tilde(text: &str, home: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(home) {
        let after = &rest[at + home.len()..];
        out.push_str(&rest[..at]);
        if after.is_empty()
            || after.starts_with(['/', '\\', ' ', '"', '\'', ')', ',', ':', ';', '\n'])
        {
            out.push('~');
        } else {
            out.push_str(home);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Where a path may start: the text's start or after one of these.
fn boundary(c: Option<char>) -> bool {
    c.is_none_or(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '(' | '[' | '=' | '<' | ','))
}

/// Where a path ends: whitespace, a quote, a closing bracket, a comma, or
/// a colon or a semicolon before whitespace or the end.
fn path_end(s: &str) -> usize {
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let next = chars.peek().map(|(_, c)| *c);
        let stop = c.is_whitespace()
            || matches!(c, '"' | '\'' | ')' | ']' | '>' | ',')
            || (matches!(c, ':' | ';') && next.is_none_or(char::is_whitespace));
        if stop {
            return i;
        }
    }
    s.len()
}

/// A path reduced to its root and its file's extension: `~/….raw`,
/// `/…`, `C:/…​.jpg`.
fn reduced(root: &str, path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or("");
    let ext = file
        .rsplit_once('.')
        .filter(|(stem, ext)| !stem.is_empty() && !ext.is_empty() && ext.len() <= 5)
        .map(|(_, ext)| format!(".{ext}"))
        .unwrap_or_default();
    format!("{root}/…{ext}")
}

/// Every absolute path of `text` reduced ([`reduced`]): `/…`, `~/…`,
/// `file:` URLs, Windows drives (`C:\…`); a bare `/` or `~` is left.
fn scrub_paths(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev: Option<char> = None;
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let c = rest.chars().next().unwrap_or(' ');
        if boundary(prev) {
            // `file:` URLs, then the path they hold.
            let (prefix, body) = match rest.strip_prefix("file://") {
                Some(b) => ("file://", b),
                None => match rest.strip_prefix("file:") {
                    Some(b) => ("file:", b),
                    None => ("", rest),
                },
            };
            let root = if body.starts_with("~/") || body.starts_with("~\\") {
                Some(("~", 1))
            } else if body.starts_with('/')
                && body[1..].chars().next().is_some_and(|c| !c.is_whitespace())
            {
                Some(("", 0))
            } else {
                let b = body.as_bytes();
                (b.len() > 3
                    && b[0].is_ascii_alphabetic()
                    && b[1] == b':'
                    && (b[2] == b'\\' || b[2] == b'/'))
                    .then(|| (&body[..2], 2))
            };
            if let Some((root, skip)) = root {
                let end = path_end(body);
                if end > skip + 1 {
                    out.push_str(prefix);
                    out.push_str(&reduced(root, &body[..end]));
                    i += prefix.len() + end;
                    prev = Some('…');
                    continue;
                }
            }
        }
        out.push(c);
        prev = Some(c);
        i += c.len_utf8();
    }
    out
}

/// Says whether a token (a run of letters and digits) is an id to alias.
pub type AliasFn = dyn Fn(&str) -> bool;

/// Ids replaced by aliases numbered in order of appearance (`photo-1`,
/// `photo-2`…): the same id gets the same alias across a report, so
/// entries still tie together, and an id that is a fingerprint (a
/// content hash, an account id) never leaves the machine.
pub struct Aliases<'a> {
    prefix: &'a str,
    matches: &'a AliasFn,
    seen: std::collections::HashMap<String, usize>,
}

impl std::fmt::Debug for Aliases<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Aliases")
            .field("prefix", &self.prefix)
            .field("seen", &self.seen.len())
            .finish_non_exhaustive()
    }
}

impl<'a> Aliases<'a> {
    pub fn new(prefix: &'a str, matches: &'a AliasFn) -> Self {
        Self {
            prefix,
            matches,
            seen: std::collections::HashMap::new(),
        }
    }

    /// How many ids were aliased.
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }

    /// `text` with each matching token replaced by its alias.
    pub fn text(&mut self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut token = String::new();
        let flush = |token: &mut String, out: &mut String, me: &mut Self| {
            if token.is_empty() {
                return;
            }
            if (me.matches)(token) {
                let n = me.seen.len() + 1;
                let n = *me.seen.entry(std::mem::take(token)).or_insert(n);
                out.push_str(&format!("{}-{n}", me.prefix));
            } else {
                out.push_str(token);
                token.clear();
            }
        };
        for c in text.chars() {
            if c.is_ascii_alphanumeric() {
                token.push(c);
            } else {
                flush(&mut token, &mut out, self);
                out.push(c);
            }
        }
        flush(&mut token, &mut out, self);
        out
    }

    /// Every string of `v`, keys included, aliased.
    pub fn json(&mut self, v: &mut Json) {
        match v {
            Json::String(s) => *s = self.text(s),
            Json::Array(items) => items.iter_mut().for_each(|x| self.json(x)),
            Json::Object(o) => {
                let taken = std::mem::take(o);
                for (k, mut x) in taken {
                    let k = self.text(&k);
                    self.json(&mut x);
                    o.insert(k, x);
                }
            }
            _ => {}
        }
    }
}

/// What a problem report attaches: shown to the user before anything
/// leaves the machine ([`Bundle::text`]).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Bundle {
    /// The version info, without its platform line.
    pub version: String,
    pub platform: String,
    /// The page or view shown, when the application says.
    pub page: Option<String>,
    /// How far back the journal reaches, in seconds.
    pub window_s: u64,
    /// Entries the recorder never saw (dropped by the journal between
    /// two reads) and entries let go for its cap: a report with either
    /// may lack the cause.
    pub missed: u64,
    pub capped: u64,
    /// The journal's window, oldest first, redacted.
    pub journal: Vec<Recorded>,
    /// The application's context, by name, redacted.
    pub context: std::collections::BTreeMap<String, Json>,
    /// The bus's points and their listeners' order.
    pub bus: Json,
}

impl Bundle {
    /// The report as the user reads and pastes it: a fenced JSON block.
    pub fn text(&self) -> String {
        let json = serde_json::to_string_pretty(self).unwrap_or_default();
        format!(
            "<details><summary>Diagnostic report</summary>\n\n```json\n{json}\n```\n</details>\n"
        )
    }
}

/// The bus's points and the order of each one's listeners, for a report
/// (ids only: no input and no value).
pub fn bus_description(bus: &dyn bus_contract::Bus) -> Json {
    let mut points: Vec<_> = bus.points();
    points.sort_by_key(|p| p.id);
    let mut out = serde_json::Map::new();
    for p in points {
        let listeners: Vec<String> = bus
            .listeners(p.id)
            .unwrap_or_default()
            .into_iter()
            .map(|l| l.id)
            .collect();
        out.insert(
            p.id.to_string(),
            serde_json::json!({ "kind": format!("{:?}", p.kind), "listeners": listeners }),
        );
    }
    Json::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ann() -> Redactor {
        Redactor::new(Some("/home/ann"), &["tok-56789012"])
    }

    fn ev(seq: Seq, name: &str, data: Json) -> Event {
        Event {
            seq,
            source: "user".into(),
            cause: None,
            name: name.into(),
            data,
        }
    }

    #[test]
    fn paths_keep_their_root_and_extension_only() {
        let r = ann();
        // In free text a space ends a path; a value that is one path is
        // reduced whole.
        assert_eq!(
            r.text("/home/ann/Photos/Jane wedding.raw"),
            "~/… wedding.raw"
        );
        assert_eq!(r.value("/home/ann/Photos/Jane wedding.raw"), "~/….raw");
        assert_eq!(r.value("not a path: /home/ann/x y"), "not a path: ~/… y");
        assert_eq!(
            r.text("/home/ann/Photos/a.raw: unreadable"),
            "~/….raw: unreadable"
        );
        assert_eq!(r.text("/home/anna/b.jpg"), "/….jpg");
        assert_eq!(r.text("file:/home/ann/x"), "file:~/…");
        assert_eq!(r.text("file:///mnt/card/DCIM"), "file:///…");
        assert_eq!(r.text("C:\\Users\\Ann\\c.NEF"), "C:/….NEF");
        assert_eq!(
            r.text("open \"/run/media/ann/CARD\" now"),
            "open \"/…\" now"
        );
        // Not paths: a URL's slashes, a ratio, a lone slash, a relative path.
        assert_eq!(r.text("https://github.com/o/r"), "https://github.com/o/r");
        assert_eq!(r.text("1/2 and a / b"), "1/2 and a / b");
        assert_eq!(r.text("photos/2024"), "photos/2024");
        assert_eq!(r.text("~"), "~");
    }

    #[test]
    fn secrets_are_blanked_and_short_values_left() {
        let r = Redactor::from_env(
            &[
                ("SOME_TOKEN".into(), "tok-56789012".into()),
                ("API_KEY".into(), "short".into()),
                ("LANG".into(), "fr_FR.UTF-8".into()),
            ],
            None,
        );
        assert_eq!(r.text("sent tok-56789012 to x"), "sent [redacted] to x");
        assert_eq!(r.text("short fr_FR.UTF-8"), "short fr_FR.UTF-8");
    }

    #[test]
    fn a_private_command_reads_private_and_strings_are_scrubbed() {
        let r = ann();
        let e = r.event(&ev(
            4,
            "command.done",
            json!({ "command": "accounts.connect", "result": "ann@example.org", "private": true }),
        ));
        assert_eq!(e.data["result"], crate::host::REDACTED);
        let e = r.event(&ev(
            5,
            "image.refused",
            json!({ "why": "locked: /home/ann/a.raw", "/home/ann/k": [ "/srv/x.tif" ] }),
        ));
        assert_eq!(e.data["why"], "locked: ~/….raw");
        assert_eq!(e.data["~/…"][0], "/….tif");
    }

    #[test]
    fn the_window_is_by_time_and_skipped_kinds_never_enter() {
        let mut rec = Recorder::new(
            RecorderConfig::default()
                .window(Duration::from_secs(60))
                .skip("view.*"),
        );
        let t0 = Instant::now();
        let r = Redactor::default();
        rec.record(t0, &[ev(1, "engine.gpu_lost", json!(null))], &r);
        // A burst of frequent kinds does not push the rare one out.
        let burst: Vec<Event> = (2..3000).map(|n| ev(n, "view.frame", json!(n))).collect();
        rec.record(t0 + Duration::from_secs(10), &burst, &r);
        let w = rec.window(t0 + Duration::from_secs(30));
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].name, "engine.gpu_lost");
        assert_eq!(w[0].ago_ms, 30_000);
        assert_eq!(rec.last(), 2999);
        // Past the window.
        rec.prune(t0 + Duration::from_secs(61));
        assert!(rec.window(t0 + Duration::from_secs(61)).is_empty());
    }

    #[test]
    fn ids_get_one_alias_each_in_order_of_appearance() {
        let hex = |t: &str| t.len() == 8 && t.chars().all(|c| c.is_ascii_hexdigit());
        let mut a = Aliases::new("photo", &hex);
        let mut v = json!([
            { "image": "deadbeef" },
            { "image": "0badf00d", "text": "after deadbeef, cafe" },
            { "deadbeef": 1 },
        ]);
        a.json(&mut v);
        assert_eq!(v[0]["image"], "photo-1");
        assert_eq!(v[1]["image"], "photo-2");
        assert_eq!(v[1]["text"], "after photo-1, cafe");
        assert_eq!(v[2]["photo-1"], 1);
        assert_eq!(a.len(), 2);
    }

    #[test]
    fn entries_dropped_between_reads_are_counted() {
        let mut rec = Recorder::new(RecorderConfig::default().cap(2));
        let t = Instant::now();
        let r = Redactor::default();
        rec.record(t, &[ev(10, "a", json!(1))], &r);
        rec.record(t, &[ev(14, "b", json!(1)), ev(15, "c", json!(1))], &r);
        assert_eq!(rec.lost(), (3, 1));
        let names: Vec<_> = rec.window(t).into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["b", "c"]);
    }
}
