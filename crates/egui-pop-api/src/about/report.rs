//! Report a problem: the new-issue URL `help.report` opens, prefilled
//! with what a maintainer asks first (the version info, the platform, the
//! page shown, the last inspect pick).
//!
//! The URL follows GitHub's query parameters: `title` and `body`, or,
//! with an issue form (`template=bug.yml`), one parameter per form field,
//! named by the field's `id` ([`FIELD_VERSION`], [`FIELD_PLATFORM`],
//! [`FIELD_PAGE`], [`FIELD_PICK`]). It is kept under [`URL_LIMIT`]
//! bytes: the pick is trimmed first, then the page, each ending
//! `(trimmed)`.
//!
//! ```
//! use egui_pop_api::about::report::{Report, URL_LIMIT};
//!
//! let report = Report {
//!     title: "Problem in Files".into(),
//!     version: "Darkroom 1.2.0\negui-pop 0.3.0\n".into(),
//!     platform: "linux x86_64".into(),
//!     page: Some("Files".into()),
//!     pick: None,
//! };
//! let url = report.url("https://github.com/o/r/issues", Some("bug.yml"));
//! assert!(url.url.starts_with("https://github.com/o/r/issues/new?template=bug.yml&title=Problem%20in%20Files&"));
//! assert!(url.url.len() <= URL_LIMIT && !url.trimmed);
//! ```

use serde::Serialize;

/// The longest URL built, in bytes: GitHub answers 414 a little over
/// 8 KB.
pub const URL_LIMIT: usize = 8000;

/// The issue form fields the URL fills, by `id` (the repository's form
/// declares them; `.github/ISSUE_TEMPLATE/bug.yml` in egui-pop's).
pub const FIELD_VERSION: &str = "version";
pub const FIELD_PLATFORM: &str = "platform";
pub const FIELD_PAGE: &str = "page";
pub const FIELD_PICK: &str = "pick";

/// What ends a trimmed field.
pub const TRIMMED: &str = "\n(trimmed)";

/// What a report says before the reporter writes a word.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Report {
    pub title: String,
    /// The version info, without its platform line.
    pub version: String,
    /// Where the application runs (`AppInfo::platform`).
    pub platform: String,
    /// The page or view shown, when the application can say.
    pub page: Option<String>,
    /// The last inspect pick, as its text block.
    pub pick: Option<String>,
}

/// A new-issue URL, and whether a field was trimmed to fit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IssueUrl {
    pub url: String,
    pub trimmed: bool,
}

impl Report {
    /// The new-issue URL under `issues` (the issue list,
    /// `https://github.com/owner/repo/issues`; `…/issues/new` is taken as
    /// is). `form`: the issue form's file name (`bug.yml`), its fields
    /// then filled one by one; without it, everything goes in the body.
    pub fn url(&self, issues: &str, form: Option<&str>) -> IssueUrl {
        let base = issues.trim_end_matches('/');
        let base = if base.ends_with("/new") {
            base.to_string()
        } else {
            format!("{base}/new")
        };
        let build = |page: Option<&str>, pick: Option<&str>| -> String {
            let mut q: Vec<(&str, String)> = Vec::new();
            if let Some(f) = form {
                q.push(("template", f.to_string()));
            }
            q.push(("title", self.title.clone()));
            match form {
                Some(_) => {
                    q.push((FIELD_VERSION, self.version.clone()));
                    q.push((FIELD_PLATFORM, self.platform.clone()));
                    if let Some(p) = page {
                        q.push((FIELD_PAGE, p.to_string()));
                    }
                    if let Some(p) = pick {
                        q.push((FIELD_PICK, p.to_string()));
                    }
                }
                None => q.push(("body", self.body(page, pick))),
            }
            let query: Vec<String> = q
                .iter()
                .map(|(k, v)| format!("{k}={}", encode(v)))
                .collect();
            format!("{base}?{}", query.join("&"))
        };
        let page = self.page.as_deref();
        let full = build(page, self.pick.as_deref());
        if full.len() <= URL_LIMIT {
            return IssueUrl {
                url: full,
                trimmed: false,
            };
        }
        // The pick first, then the page.
        if let Some(pick) = &self.pick
            && let Some(url) = fit(pick, |p| build(page, Some(p)))
        {
            return IssueUrl { url, trimmed: true };
        }
        let url = match page {
            Some(p) => fit(p, |p| build(Some(p), None)).unwrap_or_else(|| build(None, None)),
            None => build(None, None),
        };
        IssueUrl { url, trimmed: true }
    }

    /// The body without an issue form: a section per field, the
    /// reporter's three first.
    pub fn body(&self, page: Option<&str>, pick: Option<&str>) -> String {
        let mut s = String::from(
            "### What happened\n\n\n\n### What you expected\n\n\n\n### Steps to reproduce\n\n1. \n\n",
        );
        s.push_str("### Version info\n\n```text\n");
        s.push_str(self.version.trim_end());
        s.push_str("\n```\n\n### Platform\n\n");
        s.push_str(&self.platform);
        s.push('\n');
        if let Some(p) = page {
            s.push_str("\n### Page\n\n");
            s.push_str(p);
            s.push('\n');
        }
        if let Some(p) = pick {
            s.push_str("\n### Inspected element\n\n```text\n");
            s.push_str(p.trim_end());
            s.push_str("\n```\n");
        }
        s
    }
}

/// The longest start of `text`, ended with [`TRIMMED`], whose URL
/// (`build`) fits; `None` when not even the mark alone fits.
fn fit(text: &str, build: impl Fn(&str) -> String) -> Option<String> {
    let cuts: Vec<usize> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain([text.len()])
        .collect();
    let at = |n: usize| format!("{}{TRIMMED}", text[..cuts[n]].trim_end());
    if build(&at(0)).len() > URL_LIMIT {
        return None;
    }
    // The largest number of characters kept that fits.
    let (mut lo, mut hi) = (0, cuts.len() - 1);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if build(&at(mid)).len() <= URL_LIMIT {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    Some(build(&at(lo)))
}

/// Percent-encodes a query value: everything but the unreserved
/// characters (RFC 3986), as UTF-8 bytes.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// A browser and its operating system, read from a user agent string
/// (`Firefox 132 on Linux`); the string itself when neither is known.
pub fn describe_user_agent(ua: &str) -> String {
    let version = |marker: &str| -> Option<String> {
        let rest = &ua[ua.find(marker)? + marker.len()..];
        let v: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let major = v.split('.').next().unwrap_or("").to_string();
        (!major.is_empty()).then_some(major)
    };
    let named = |name: &str, marker: &str| version(marker).map(|v| format!("{name} {v}"));
    let browser = named("Edge", "Edg/")
        .or_else(|| named("Opera", "OPR/"))
        .or_else(|| named("Samsung Internet", "SamsungBrowser/"))
        .or_else(|| named("Firefox", "Firefox/"))
        .or_else(|| named("Firefox", "FxiOS/"))
        .or_else(|| named("Chrome", "CriOS/"))
        .or_else(|| named("Chrome", "Chrome/"))
        .or_else(|| {
            ua.contains("Safari/")
                .then(|| named("Safari", "Version/"))
                .flatten()
        });
    let os = if ua.contains("Windows") {
        Some("Windows")
    } else if ua.contains("Android") {
        Some("Android")
    } else if ua.contains("iPhone") || ua.contains("iPad") {
        Some("iOS")
    } else if ua.contains("Mac OS X") || ua.contains("Macintosh") {
        Some("macOS")
    } else if ua.contains("CrOS") {
        Some("ChromeOS")
    } else if ua.contains("Linux") {
        Some("Linux")
    } else {
        None
    };
    match (browser, os) {
        (Some(b), Some(o)) => format!("{b} on {o}"),
        (Some(b), None) => b,
        (None, Some(o)) => format!("an unknown browser on {o}: {ua}"),
        (None, None) => ua.to_string(),
    }
}
