//! Links' pure parts (ADR 0017): a target parsed and classified, the
//! request to open it, and the one policy deciding how it opens and
//! whether the user confirms first. No egui and no system call: the
//! service, the openers, the widget and the confirmation are
//! `egui_pop_api::links`'.
//!
//! - A **target** ([`Target`]) is a URL (`https:`, `mailto:`, `file:`,
//!   `docs:`, `command:`, the application's own scheme) or an absolute
//!   local path, parsed once; its host is kept in punycode and shown in
//!   Unicode.
//! - A **request** ([`Request`]) names the target, **how** ([`How`]:
//!   `Auto`, `Outside`, `Inside`, `Reveal`), its **source** ([`Source`]:
//!   declared by the application, found in content, asked by an agent),
//!   the text it was shown under, and who asks.
//! - The **policy** ([`Policy::decide`]) classifies the target
//!   ([`Kind`]), resolves `Auto` against what the opener can do
//!   ([`Caps`]) and says whether to confirm, or why it is refused.
//!
//! ```
//! use egui_pop_model::links::{Caps, Facts, How, Kind, Policy, Request, Source};
//!
//! let policy = Policy::default();
//! let about = Request::new("https://github.com/quazardous/egui-pop");
//! let d = policy.decide(&about, &Caps::ALL, None);
//! assert_eq!((d.kind, d.how, d.confirm), (Kind::Web, How::Outside, false));
//!
//! // The same page found in a document asks first, until its host is
//! // trusted for the session.
//! let found = about.clone().source(Source::Content);
//! assert!(policy.decide(&found, &Caps::ALL, None).confirm);
//!
//! // A local executable is never opened: Reveal is offered instead.
//! let tool = Request::new("/opt/tool/run.sh");
//! let d = policy.decide(&tool, &Caps::ALL, Some(&Facts::file()));
//! assert!(d.refused.is_some() && d.reveal_instead);
//! ```

pub mod inline;

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use url::Url;

use crate::commands::Origin;

/// How a link opens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum How {
    /// What the policy picks for its kind (a plain click).
    #[default]
    Auto,
    /// Outside the application: the browser, the mail client, the
    /// system's default application, the file manager (Ctrl+click,
    /// middle click).
    Outside,
    /// Inside the application: its viewer, its own scheme, a command.
    Inside,
    /// The file manager, with the file selected.
    Reveal,
}

impl How {
    pub fn as_str(self) -> &'static str {
        match self {
            How::Auto => "auto",
            How::Outside => "outside",
            How::Inside => "inside",
            How::Reveal => "reveal",
        }
    }
}

/// Where a link comes from: what the policy trusts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Declared by the application: its About, menus, tips, toasts,
    /// `docs:` pages.
    #[default]
    Declared,
    /// Found in content: a document, a remote page, a terminal's output,
    /// a paste.
    Content,
    /// Asked by an agent (or a script, a remote process).
    Agent,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Declared => "declared",
            Source::Content => "content",
            Source::Agent => "agent",
        }
    }
}

/// What a target is, from its scheme, host and extension (never by
/// fetching it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A web page (`https:`, `http:`).
    Web,
    /// A video: a video host's page, or a remote video file.
    Video,
    /// A page the application ships (`docs:`).
    Doc,
    /// A remote markdown file.
    Markdown,
    /// A remote HTML file.
    Html,
    /// An image, remote or local.
    Image,
    /// A local file.
    File,
    /// A local folder.
    Folder,
    /// An address (`mailto:`).
    Mail,
    /// The application's own scheme (`<app>://`).
    AppLink,
    /// A command (`command:<id>?<json>`).
    Command,
    /// Nothing that may be opened (see the decision's reason).
    Blocked,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Web => "web",
            Kind::Video => "video",
            Kind::Doc => "doc",
            Kind::Markdown => "markdown",
            Kind::Html => "html",
            Kind::Image => "image",
            Kind::File => "file",
            Kind::Folder => "folder",
            Kind::Mail => "mail",
            Kind::AppLink => "app_link",
            Kind::Command => "command",
            Kind::Blocked => "blocked",
        }
    }
}

/// Why a link is refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum Reason {
    /// Nothing to open.
    Empty,
    /// Neither a URL nor an absolute path.
    Unparsable(String),
    /// A scheme the policy does not allow (`javascript:`, `data:`...).
    Scheme(String),
    /// A device (`/dev/...`, `\\.\...`).
    Device,
    /// An executable or a script: never opened, Reveal offered.
    Executable,
    /// A file that is not there.
    Missing,
    /// A `command:` link outside text the application declared, or asked
    /// by an agent.
    UndeclaredCommand,
    /// Content opens only on a click.
    NoGesture,
    /// The way asked does not apply to this kind (Reveal of a web page).
    NotApplicable(String),
    /// The opener cannot do it here (local files on the web build, no
    /// viewer).
    Unsupported(String),
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reason::Empty => write!(f, "the link is empty"),
            Reason::Unparsable(t) => write!(f, "`{t}` is neither an address nor a full path"),
            Reason::Scheme(s) => write!(f, "`{s}:` links are blocked"),
            Reason::Device => write!(f, "a device cannot be opened"),
            Reason::Executable => {
                write!(f, "a program or a script is never opened from a link")
            }
            Reason::Missing => write!(f, "the file is not there"),
            Reason::UndeclaredCommand => {
                write!(
                    f,
                    "a command link runs only from the application's own text"
                )
            }
            Reason::NoGesture => write!(f, "a link in content opens only on a click"),
            Reason::NotApplicable(w) | Reason::Unsupported(w) => write!(f, "{w}"),
        }
    }
}

/// A parsed target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    url: Url,
    /// It was given as a local path.
    path_given: bool,
}

/// Extensions of programs and scripts: never opened from a link.
pub const EXECUTABLE: &[&str] = &[
    "exe", "com", "bat", "cmd", "msi", "msp", "scr", "pif", "cpl", "ps1", "psm1", "vbs", "vbe",
    "js", "jse", "wsf", "wsh", "hta", "jar", "reg", "lnk", "sh", "bash", "zsh", "csh", "fish",
    "ksh", "command", "run", "bin", "desktop", "app", "appimage", "deb", "rpm", "pkg", "dmg", "py",
    "pyw", "pl", "rb", "php",
];

const IMAGES: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "avif", "bmp", "svg", "tif", "tiff", "heic", "jxl",
];
const VIDEOS: &[&str] = &["mp4", "webm", "mov", "mkv", "ogv", "m4v", "avi"];

impl Target {
    /// Parses a URL, or an absolute local path (`/home/a.png`,
    /// `C:\a.png`, `\\server\share\a.png`) made a `file:` URL.
    pub fn parse(text: &str) -> Result<Target, Reason> {
        let t = text.trim();
        if t.is_empty() {
            return Err(Reason::Empty);
        }
        if is_path(t) {
            return path_url(t)
                .map(|url| Target {
                    url,
                    path_given: true,
                })
                .ok_or_else(|| Reason::Unparsable(t.into()));
        }
        Url::parse(t)
            .map(|url| Target {
                url,
                path_given: false,
            })
            .map_err(|_| Reason::Unparsable(t.into()))
    }

    /// The target as a URL, canonical (`https://example.org/`).
    pub fn as_str(&self) -> &str {
        self.url.as_str()
    }

    pub fn url(&self) -> &Url {
        &self.url
    }

    /// The scheme, lowercase (`https`).
    pub fn scheme(&self) -> &str {
        self.url.scheme()
    }

    /// The host as the address carries it (punycode for an
    /// internationalised one).
    pub fn host_ascii(&self) -> Option<&str> {
        self.url.host_str().filter(|h| !h.is_empty())
    }

    /// The host as a person reads it (Unicode).
    pub fn host(&self) -> Option<String> {
        let ascii = self.host_ascii()?;
        if !ascii.contains("xn--") {
            return Some(ascii.to_string());
        }
        let (unicode, ok) = idna::domain_to_unicode(ascii);
        Some(if ok.is_ok() { unicode } else { ascii.into() })
    }

    /// Whether the host is internationalised: its Unicode form and its
    /// punycode differ, and both are shown.
    pub fn is_idn(&self) -> bool {
        self.host_ascii().is_some_and(|h| h.contains("xn--"))
    }

    /// The host as shown to the user: Unicode, with its punycode beside
    /// it when they differ (`bücher.example (xn--bcher-kva.example)`).
    pub fn host_shown(&self) -> Option<String> {
        let ascii = self.host_ascii()?;
        let unicode = self.host()?;
        Some(if unicode == ascii {
            unicode
        } else {
            format!("{unicode} ({ascii})")
        })
    }

    /// The local path of a `file:` target.
    pub fn local_path(&self) -> Option<String> {
        if self.scheme() != "file" {
            return None;
        }
        file_path(&self.url)
    }

    /// The text to show for it: the path when one was given, else the
    /// URL.
    pub fn shown(&self) -> String {
        match (self.path_given, self.local_path()) {
            (true, Some(p)) => p,
            _ => self.url.as_str().to_string(),
        }
    }

    /// The extension of its last path segment, lowercase.
    pub fn extension(&self) -> Option<String> {
        let last = self.url.path_segments()?.next_back()?;
        let (_, ext) = last.rsplit_once('.')?;
        (!ext.is_empty()).then(|| ext.to_ascii_lowercase())
    }

    /// Whether its extension is a program's or a script's.
    pub fn looks_executable(&self) -> bool {
        self.extension()
            .is_some_and(|e| EXECUTABLE.contains(&e.as_str()))
    }

    /// A `command:` target's command and arguments (`command:view.zoom?{"to":2}`,
    /// the JSON percent-encoded or not).
    pub fn command(&self) -> Option<(String, Json)> {
        if self.scheme() != "command" {
            return None;
        }
        let id = percent_decode(self.url.path());
        if id.is_empty() {
            return None;
        }
        let args = match self.url.query() {
            Some(q) if !q.is_empty() => serde_json::from_str(&percent_decode(q)).ok()?,
            _ => Json::Null,
        };
        Some((id, args))
    }

    /// A `docs:` target's page (`docs:guide/start#install` gives
    /// `guide/start`).
    pub fn doc_page(&self) -> Option<String> {
        (self.scheme() == "docs").then(|| percent_decode(self.url.path()))
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.shown())
    }
}

fn is_path(t: &str) -> bool {
    let b = t.as_bytes();
    t.starts_with('/')
        || t.starts_with("\\\\")
        || (b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && matches!(b[2], b'\\' | b'/'))
}

#[cfg(any(unix, windows))]
fn path_url(p: &str) -> Option<Url> {
    Url::from_file_path(p).ok()
}

#[cfg(not(any(unix, windows)))]
fn path_url(_: &str) -> Option<Url> {
    None
}

#[cfg(any(unix, windows))]
fn file_path(url: &Url) -> Option<String> {
    url.to_file_path().ok().map(|p| p.display().to_string())
}

#[cfg(not(any(unix, windows)))]
fn file_path(url: &Url) -> Option<String> {
    Some(percent_decode(url.path()))
}

/// Decodes `%xx` sequences (invalid ones are kept as they are).
pub fn percent_decode(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Whether a text looks like an address (`https://…`, `www.…`,
/// `example.org/…`, `mailto:…`): a link shown under such a text must go
/// there, or it is confirmed with both shown.
pub fn looks_like_url(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() || t.contains(char::is_whitespace) {
        return false;
    }
    let lower = t.to_lowercase();
    if [
        "http://", "https://", "www.", "mailto:", "file://", "ftp://",
    ]
    .iter()
    .any(|p| lower.starts_with(p))
    {
        return true;
    }
    let host = lower.split(['/', '?', '#']).next().unwrap_or_default();
    let parts: Vec<&str> = host.split('.').collect();
    let Some(tld) = parts.last() else {
        return false;
    };
    parts.len() >= 2
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_alphanumeric() || c == '-'))
        && tld.chars().count() >= 2
        && tld.chars().all(char::is_alphabetic)
}

/// Whether a link's text looks like an address other than its target's
/// (another host, or another page on it).
pub fn disguised(label: &str, target: &Target) -> bool {
    if !looks_like_url(label) {
        return false;
    }
    let l = label.trim();
    let lower = l.to_lowercase();
    if let Some(addr) = lower.strip_prefix("mailto:") {
        return target.scheme() != "mailto" || target.url.path().to_lowercase() != addr;
    }
    let shown = if lower.contains("://") {
        l.to_string()
    } else {
        format!("https://{l}")
    };
    let Ok(said) = Url::parse(&shown) else {
        return true;
    };
    let host = |u: &Url| {
        u.host_str()
            .map(|h| h.trim_start_matches("www.").to_string())
    };
    if host(&said) != host(&target.url) {
        return true;
    }
    let path = |u: &Url| u.path().trim_end_matches('/').to_string();
    // A text naming only the host stands for any page of it.
    !(path(&said).is_empty() || path(&said) == path(&target.url))
}

/// What the opener can do here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caps {
    /// Web pages and `mailto:` outside (the browser, the mail client).
    pub web: bool,
    /// Local files and folders outside (the default application, the
    /// file manager).
    pub files: bool,
    /// The file manager with a file selected.
    pub reveal: bool,
    /// A viewer inside the application for pages and images.
    pub inside: bool,
}

impl Caps {
    /// Everything (tests, a forwarding opener).
    pub const ALL: Caps = Caps {
        web: true,
        files: true,
        reveal: true,
        inside: true,
    };
    /// Web pages and mail only (egui's own opener).
    pub const WEB: Caps = Caps {
        web: true,
        files: false,
        reveal: false,
        inside: false,
    };
    /// Nothing.
    pub const NONE: Caps = Caps {
        web: false,
        files: false,
        reveal: false,
        inside: false,
    };
}

/// What is known of a local target, read by the caller (the policy never
/// touches the disk).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facts {
    pub exists: bool,
    pub dir: bool,
    /// The executable bit (Unix), beside the extensions.
    pub executable: bool,
}

impl Facts {
    /// A plain file that exists.
    pub fn file() -> Facts {
        Facts {
            exists: true,
            dir: false,
            executable: false,
        }
    }

    /// A folder that exists.
    pub fn dir() -> Facts {
        Facts {
            exists: true,
            dir: true,
            executable: false,
        }
    }
}

/// A request to open a link.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// The target, as written (a URL or an absolute path).
    pub target: String,
    #[serde(default)]
    pub how: How,
    #[serde(default)]
    pub source: Source,
    /// The text the link was shown under, when it had one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Who asks.
    #[serde(skip, default = "user")]
    pub origin: Origin,
}

fn user() -> Origin {
    Origin::User
}

impl Request {
    /// A declared link, opened its kind's way, by the user.
    pub fn new(target: &str) -> Request {
        Request {
            target: target.into(),
            how: How::Auto,
            source: Source::Declared,
            label: None,
            origin: Origin::User,
        }
    }

    pub fn how(mut self, how: How) -> Request {
        self.how = how;
        self
    }

    pub fn source(mut self, source: Source) -> Request {
        self.source = source;
        self
    }

    /// The text it is shown under.
    pub fn label(mut self, label: &str) -> Request {
        self.label = Some(label.into());
        self
    }

    pub fn origin(mut self, origin: Origin) -> Request {
        self.origin = origin;
        self
    }
}

/// What the policy decided.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    /// The target as parsed (canonical), or as given when it does not
    /// parse.
    pub target: String,
    pub kind: Kind,
    /// How it opens: never `Auto`.
    pub how: How,
    /// The user confirms first.
    pub confirm: bool,
    /// Why it is confirmed (`agent`, `local file`, `untrusted host`,
    /// `disguised`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// Why it is refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<Reason>,
    /// Refused, but showing it in its folder is offered instead.
    #[serde(default)]
    pub reveal_instead: bool,
    /// The host, as shown (Unicode, punycode beside it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// The host as the address carries it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_ascii: Option<String>,
    /// The text it was shown under looks like another address.
    #[serde(default)]
    pub disguised: bool,
}

impl Decision {
    fn refused(target: &str, reason: Reason) -> Decision {
        Decision {
            target: target.into(),
            kind: Kind::Blocked,
            how: How::Outside,
            confirm: false,
            why: None,
            refused: Some(reason),
            reveal_instead: false,
            host: None,
            host_ascii: None,
            disguised: false,
        }
    }

    /// Whether it may go on (after a confirmation when `confirm`).
    pub fn allowed(&self) -> bool {
        self.refused.is_none()
    }
}

/// The schemes allowed by default.
pub const SCHEMES: &[&str] = &["https", "http", "mailto", "file", "docs", "command"];

/// Hosts whose pages are videos (a link there never shows inside).
pub const VIDEO_HOSTS: &[&str] = &[
    "youtube.com",
    "youtu.be",
    "youtube-nocookie.com",
    "vimeo.com",
    "dailymotion.com",
    "peertube.tv",
];

/// The rules links follow: the schemes allowed, the hosts trusted for
/// the session, the hosts holding the application's documentation, the
/// application's own scheme.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Policy {
    /// Schemes allowed; everything else is blocked.
    pub schemes: BTreeSet<String>,
    /// The application's own schemes (`helios`), opened inside.
    #[serde(default)]
    pub app_schemes: BTreeSet<String>,
    /// Hosts trusted for this session: content links to them open
    /// without a confirmation (a host and its subdomains).
    pub trusted: BTreeSet<String>,
    /// Hosts holding the application's documentation: their markdown and
    /// HTML pages open inside when a viewer is there; content links to
    /// them are trusted.
    pub docs_hosts: BTreeSet<String>,
    /// Hosts whose pages are videos.
    pub video_hosts: BTreeSet<String>,
    /// Where the `docs:` pages are online (`https://example.org/docs/`):
    /// without a viewer, `docs:<page>` opens `<base><page>` there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs_online: Option<String>,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            schemes: SCHEMES.iter().map(|s| s.to_string()).collect(),
            app_schemes: BTreeSet::new(),
            trusted: BTreeSet::new(),
            docs_hosts: BTreeSet::new(),
            video_hosts: VIDEO_HOSTS.iter().map(|s| s.to_string()).collect(),
            docs_online: None,
        }
    }
}

/// Whether `host` is `listed` or one of its subdomains.
fn under(host: &str, listed: &BTreeSet<String>) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    listed.iter().any(|l| {
        let l = l.trim_end_matches('.').to_ascii_lowercase();
        host == l || host.ends_with(&format!(".{l}"))
    })
}

impl Policy {
    /// One of the application's own schemes, opened inside (allowed
    /// too).
    pub fn app_scheme(mut self, scheme: &str) -> Policy {
        let s = scheme.trim_end_matches("://").to_ascii_lowercase();
        self.schemes.insert(s.clone());
        self.app_schemes.insert(s);
        self
    }

    /// A host holding the application's documentation.
    pub fn docs_host(mut self, host: &str) -> Policy {
        self.docs_hosts.insert(host.to_ascii_lowercase());
        self
    }

    /// Trusts a host (and its subdomains) for the session.
    pub fn trust(&mut self, host: &str) {
        self.trusted.insert(host.to_ascii_lowercase());
    }

    /// Whether content links to a host open without asking.
    pub fn is_trusted(&self, host: &str) -> bool {
        under(host, &self.trusted) || under(host, &self.docs_hosts)
    }

    /// What a target is, from its scheme, host and extension, and what
    /// is known of it on disk.
    pub fn classify(&self, t: &Target, facts: Option<&Facts>) -> Result<Kind, Reason> {
        let scheme = t.scheme();
        if !self.schemes.contains(scheme) {
            return Err(Reason::Scheme(scheme.into()));
        }
        if self.app_schemes.contains(scheme) {
            return Ok(Kind::AppLink);
        }
        Ok(match scheme {
            "http" | "https" => {
                let ext = t.extension().unwrap_or_default();
                let host = t.host_ascii().unwrap_or_default();
                if under(host, &self.video_hosts) || VIDEOS.contains(&ext.as_str()) {
                    Kind::Video
                } else if IMAGES.contains(&ext.as_str()) {
                    Kind::Image
                } else if matches!(ext.as_str(), "md" | "markdown") {
                    Kind::Markdown
                } else if matches!(ext.as_str(), "html" | "htm" | "xhtml") {
                    Kind::Html
                } else {
                    Kind::Web
                }
            }
            "mailto" => Kind::Mail,
            "docs" => Kind::Doc,
            "command" => {
                if t.command().is_none() {
                    return Err(Reason::Unparsable(t.as_str().into()));
                }
                Kind::Command
            }
            "file" => {
                let path = t.local_path().unwrap_or_default();
                if path.starts_with("/dev/")
                    || path.starts_with("\\\\.\\")
                    || path.starts_with("/proc/")
                {
                    return Err(Reason::Device);
                }
                match facts {
                    Some(f) if f.dir => Kind::Folder,
                    _ if t.url.path().ends_with('/') => Kind::Folder,
                    _ if IMAGES.contains(&t.extension().unwrap_or_default().as_str()) => {
                        Kind::Image
                    }
                    _ => Kind::File,
                }
            }
            // An allowed scheme egui-pop knows nothing of: the system's
            // handler.
            _ => Kind::Web,
        })
    }

    /// Decides how a request opens and whether the user confirms it
    /// first: the one rule every link follows (ADR 0017).
    pub fn decide(&self, r: &Request, caps: &Caps, facts: Option<&Facts>) -> Decision {
        let t = match Target::parse(&r.target) {
            Ok(t) => t,
            Err(e) => return Decision::refused(r.target.trim(), e),
        };
        let mut d = Decision::refused(t.as_str(), Reason::Empty);
        d.refused = None;
        d.host = t.host_shown();
        d.host_ascii = t.host_ascii().map(str::to_string);
        d.disguised = r.label.as_deref().is_some_and(|l| disguised(l, &t));
        let kind = match self.classify(&t, facts) {
            Ok(k) => k,
            Err(e) => {
                d.refused = Some(e);
                return d;
            }
        };
        d.kind = kind;
        let local = t.scheme() == "file";
        let refuse = |mut d: Decision, why: Reason| {
            d.refused = Some(why);
            d.confirm = false;
            d.why = None;
            d
        };
        // Content opens on a click only: a person asks for it.
        if r.source == Source::Content && r.origin != Origin::User {
            return refuse(d, Reason::NoGesture);
        }
        // Command links run from the application's own text only.
        if kind == Kind::Command && r.source != Source::Declared {
            return refuse(d, Reason::UndeclaredCommand);
        }
        let viewer = |k: Kind| match k {
            Kind::Doc => caps.inside,
            Kind::Markdown | Kind::Html => {
                caps.inside && t.host_ascii().is_some_and(|h| under(h, &self.docs_hosts))
            }
            Kind::Image => caps.inside && local,
            _ => false,
        };
        d.how = match (r.how, kind) {
            (How::Reveal, Kind::File | Kind::Folder | Kind::Image) if local => How::Reveal,
            (How::Reveal, _) => {
                return refuse(
                    d,
                    Reason::NotApplicable(
                        "only a local file or folder can be shown in its folder".into(),
                    ),
                );
            }
            (_, Kind::AppLink) => How::Inside,
            (How::Outside, Kind::Command) => {
                return refuse(
                    d,
                    Reason::NotApplicable("a command link runs inside the application".into()),
                );
            }
            (_, Kind::Command) => How::Inside,
            (How::Inside, k) if viewer(k) => How::Inside,
            (How::Auto, k) if viewer(k) => How::Inside,
            _ => How::Outside,
        };
        if d.how == How::Outside && kind == Kind::Doc {
            match &self.docs_online {
                Some(_) => {}
                None => {
                    return refuse(
                        d,
                        Reason::Unsupported(
                            "this page needs the documentation viewer, and has no online copy"
                                .into(),
                        ),
                    );
                }
            }
        }
        // What the opener cannot do.
        let unsupported = match d.how {
            How::Reveal if !caps.reveal => {
                Some("showing a file in its folder is not available here")
            }
            How::Outside if local && !caps.files => Some("local files cannot be opened here"),
            How::Outside if !local && !caps.web => Some("links cannot be opened here"),
            _ => None,
        };
        if let Some(w) = unsupported {
            return refuse(d, Reason::Unsupported(w.into()));
        }
        if local {
            let program = t.looks_executable() || facts.is_some_and(|f| f.executable && !f.dir);
            if program && d.how == How::Outside {
                d.reveal_instead = caps.reveal && facts.is_none_or(|f| f.exists);
                return refuse(d, Reason::Executable);
            }
            if facts.is_some_and(|f| !f.exists) {
                return refuse(d, Reason::Missing);
            }
        }
        // Whether the user confirms first.
        let why = if r.source == Source::Agent || matches!(r.origin, Origin::Agent(_)) {
            Some("agent")
        } else if d.disguised {
            Some("disguised")
        } else {
            match (r.source, kind, d.how) {
                (_, _, How::Inside) if kind != Kind::AppLink => None,
                (Source::Declared, _, How::Reveal) => None,
                (Source::Declared, Kind::Folder, _) => None,
                (_, Kind::File | Kind::Image, How::Outside) if local => Some("local file"),
                (Source::Declared, _, _) => None,
                (_, Kind::Folder, _) | (_, _, How::Reveal) => Some("local file"),
                (_, Kind::AppLink | Kind::Mail, _) => Some("content"),
                _ => {
                    let trusted = t.host_ascii().is_some_and(|h| self.is_trusted(h));
                    (!trusted).then_some("untrusted host")
                }
            }
        };
        d.confirm = why.is_some();
        d.why = why.map(str::to_string);
        d
    }

    /// The address `docs:<page>` opens outside, when the documentation
    /// is online.
    pub fn docs_url(&self, page: &str) -> Option<String> {
        let base = self.docs_online.as_ref()?;
        Some(format!("{base}{}", page.trim_start_matches('/')))
    }
}

/// A link found in a short text: its label (a markdown link's) and its
/// target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Found {
    pub label: Option<String>,
    pub target: String,
}

/// The links in a short text: markdown links (`[label](target)`),
/// autolinks (`<https://…>`) and bare `http(s)://` addresses.
pub fn find(text: &str) -> Vec<Found> {
    let mut out = Vec::new();
    let mut rest = text;
    // Markdown links first; their targets are not found again as bare.
    let mut plain = String::new();
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find("](") else {
            break;
        };
        let target_start = &after[close + 2..];
        let Some(end) = target_start.find(')') else {
            break;
        };
        plain.push_str(&rest[..open]);
        let label = &after[..close];
        let target = target_start[..end].trim();
        let target = target.split_whitespace().next().unwrap_or_default();
        out.push(Found {
            label: Some(label.to_string()),
            target: target.trim_matches(['<', '>']).to_string(),
        });
        plain.push_str(label);
        rest = &target_start[end + 1..];
    }
    plain.push_str(rest);
    for word in plain.split(char::is_whitespace) {
        let w = word.trim_start_matches(['<', '(', '"', '\'']);
        let lower = w.to_ascii_lowercase();
        if lower.starts_with("https://") || lower.starts_with("http://") {
            let w = w.trim_end_matches(['>', ')', '.', ',', ';', ':', '!', '?', '"', '\'']);
            out.push(Found {
                label: None,
                target: w.to_string(),
            });
        }
    }
    out
}

/// What is wrong with a link the application declares: it must parse,
/// use an allowed scheme, and a `command:` link must name a declared
/// command (`commands`). `docs` lists the `docs:` pages the application
/// ships, when it says (`None`: not checked).
pub fn check_declared(
    policy: &Policy,
    target: &str,
    commands: &[String],
    docs: Option<&[String]>,
) -> Result<(), String> {
    let t = Target::parse(target).map_err(|e| e.to_string())?;
    let kind = policy.classify(&t, None).map_err(|e| e.to_string())?;
    match kind {
        Kind::Command => {
            let (id, _) = t
                .command()
                .ok_or_else(|| format!("`{target}` names no command"))?;
            if !commands.contains(&id) {
                return Err(format!("`{target}` names `{id}`, which is not declared"));
            }
        }
        Kind::Doc => {
            let page = t.doc_page().unwrap_or_default();
            if let Some(pages) = docs
                && !pages.contains(&page)
            {
                return Err(format!("`{target}`: no `docs:` page `{page}`"));
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decide(r: Request) -> Decision {
        Policy::default().decide(&r, &Caps::ALL, None)
    }

    #[test]
    fn declared_web_and_mail_links_open_outside_without_asking() {
        for t in ["https://example.org/a", "mailto:someone@example.org"] {
            let d = decide(Request::new(t));
            assert_eq!(d.how, How::Outside, "{t}");
            assert!(!d.confirm && d.allowed(), "{t}");
        }
    }

    #[test]
    fn content_links_ask_until_their_host_is_trusted() {
        let r = Request::new("https://example.org/report").source(Source::Content);
        let mut p = Policy::default();
        let d = p.decide(&r, &Caps::ALL, None);
        assert_eq!(
            (d.confirm, d.why.as_deref()),
            (true, Some("untrusted host"))
        );
        p.trust("example.org");
        assert!(!p.decide(&r, &Caps::ALL, None).confirm);
        // A subdomain is trusted with its host; another host is not.
        let sub = Request::new("https://docs.example.org/").source(Source::Content);
        assert!(!p.decide(&sub, &Caps::ALL, None).confirm);
        let other = Request::new("https://example.com/").source(Source::Content);
        assert!(p.decide(&other, &Caps::ALL, None).confirm);
    }

    #[test]
    fn content_needs_a_click_and_agents_always_confirm() {
        let r = Request::new("https://example.org/")
            .source(Source::Content)
            .origin(Origin::Script("s".into()));
        assert_eq!(decide(r).refused, Some(Reason::NoGesture));
        let r = Request::new("https://example.org/")
            .source(Source::Agent)
            .origin(Origin::Agent("a".into()));
        let d = decide(r);
        assert_eq!((d.confirm, d.why.as_deref()), (true, Some("agent")));
    }

    #[test]
    fn blocked_schemes_say_which() {
        for (t, s) in [
            ("javascript:alert(1)", "javascript"),
            ("data:text/html,x", "data"),
        ] {
            assert_eq!(
                decide(Request::new(t)).refused,
                Some(Reason::Scheme(s.into()))
            );
        }
        assert!(matches!(
            decide(Request::new("not a link")).refused,
            Some(Reason::Unparsable(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn local_files_are_confirmed_and_programs_refused_with_reveal_offered() {
        let p = Policy::default();
        let r = Request::new("/home/a/photo.txt");
        let d = p.decide(&r, &Caps::ALL, Some(&Facts::file()));
        assert_eq!((d.kind, d.confirm), (Kind::File, true));
        let d = p.decide(
            &Request::new("/home/a/run.sh"),
            &Caps::ALL,
            Some(&Facts::file()),
        );
        assert_eq!(d.refused, Some(Reason::Executable));
        assert!(d.reveal_instead);
        // Revealing a declared path asks nothing; a folder opens.
        let d = p.decide(
            &r.clone().how(How::Reveal),
            &Caps::ALL,
            Some(&Facts::file()),
        );
        assert_eq!((d.how, d.confirm), (How::Reveal, false));
        let d = p.decide(&Request::new("/home/a"), &Caps::ALL, Some(&Facts::dir()));
        assert_eq!((d.kind, d.confirm), (Kind::Folder, false));
        // Without the system opener, local files are refused.
        let d = p.decide(&r, &Caps::WEB, Some(&Facts::file()));
        assert!(matches!(d.refused, Some(Reason::Unsupported(_))));
        assert_eq!(
            p.decide(&Request::new("/dev/sda"), &Caps::ALL, None)
                .refused,
            Some(Reason::Device)
        );
    }

    #[test]
    fn videos_open_outside_and_docs_inside_when_a_viewer_is_there() {
        let d = decide(Request::new("https://www.youtube.com/watch?v=x"));
        assert_eq!((d.kind, d.how), (Kind::Video, How::Outside));
        let d = decide(Request::new("docs:guide/start"));
        assert_eq!((d.kind, d.how), (Kind::Doc, How::Inside));
        let p = Policy::default();
        let d = p.decide(&Request::new("docs:guide/start"), &Caps::WEB, None);
        assert!(matches!(d.refused, Some(Reason::Unsupported(_))));
    }

    #[test]
    fn command_links_run_from_declared_text_only() {
        let t = "command:view.zoom?%7B%22to%22%3A2%7D";
        let target = Target::parse(t).unwrap();
        assert_eq!(
            target.command(),
            Some(("view.zoom".into(), serde_json::json!({ "to": 2 })))
        );
        assert_eq!(decide(Request::new(t)).how, How::Inside);
        let d = decide(Request::new(t).source(Source::Content));
        assert_eq!(d.refused, Some(Reason::UndeclaredCommand));
    }

    #[test]
    fn a_text_showing_another_address_is_disguised() {
        let t = Target::parse("https://evil.example/login").unwrap();
        assert!(disguised("https://bank.example", &t));
        assert!(disguised("bank.example/login", &t));
        assert!(!disguised("evil.example", &t));
        assert!(!disguised("Log in", &t));
        let d = decide(Request::new("https://evil.example/login").label("bank.example"));
        assert_eq!((d.confirm, d.disguised), (true, true));
    }

    #[test]
    fn internationalised_hosts_show_their_punycode() {
        let t = Target::parse("https://bücher.example/").unwrap();
        assert!(t.is_idn());
        assert_eq!(t.host_ascii(), Some("xn--bcher-kva.example"));
        assert_eq!(
            t.host_shown().as_deref(),
            Some("bücher.example (xn--bcher-kva.example)")
        );
    }

    #[test]
    fn links_are_found_in_short_texts() {
        let found =
            find("See [the guide](docs:guide) or https://example.org/a, and <https://b.example>.");
        let targets: Vec<&str> = found.iter().map(|f| f.target.as_str()).collect();
        assert_eq!(
            targets,
            ["docs:guide", "https://example.org/a", "https://b.example"]
        );
        assert_eq!(found[0].label.as_deref(), Some("the guide"));
    }

    #[test]
    fn declared_links_are_checked() {
        let p = Policy::default();
        let cmds = vec!["view.zoom".to_string()];
        assert!(check_declared(&p, "https://example.org", &cmds, None).is_ok());
        assert!(check_declared(&p, "command:view.zoom", &cmds, None).is_ok());
        assert!(check_declared(&p, "command:view.nope", &cmds, None).is_err());
        assert!(check_declared(&p, "javascript:x", &cmds, None).is_err());
        let pages = vec!["intro".to_string()];
        assert!(check_declared(&p, "docs:intro", &cmds, Some(&pages)).is_ok());
        assert!(check_declared(&p, "docs:outro", &cmds, Some(&pages)).is_err());
    }
}
