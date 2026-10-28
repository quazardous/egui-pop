//! The clipboard's pure parts (ADR 0015): formats, items, slots, the
//! boundaries content crosses, and the negotiation between what a copy
//! offers and what a paste target accepts. No egui: the contract, its
//! default owner and the commands are `egui_pop_api::clipboard`'s.
//!
//! - A **format** ([`FormatId`]) is a MIME-like id: an application's
//!   private types are `application/x-<app>.<type>+json` (egui-pop's
//!   own `application/x-egui-pop.<type>+json`), the standard ones
//!   `text/plain`, `text/html`, `image/png`, `text/uri-list`.
//! - An **item** ([`ClipItem`]) is what one copy made: its private type,
//!   its version, a structured payload, a title ("3 nodes"), readable
//!   **fallbacks** for the system clipboard, where it came from and what
//!   sanitisers left out.
//! - A **target** declares the formats it accepts, richest first;
//!   [`negotiate`] takes the first the item offers, or one a declared
//!   transform makes from what it offers (one transform, never a chain).
//!   Nothing fits: a [`Refusal`] saying why (ADR 0003).
//!
//! ```
//! use egui_pop_model::clipboard::{self, ClipItem, FormatId, TransformSpec, TEXT};
//!
//! const COLOR: FormatId = FormatId::from_static("application/x-demo.color+json");
//! const GRAY: FormatId = FormatId::from_static("application/x-demo.gray+json");
//!
//! let item = ClipItem::new(COLOR, 1, serde_json::json!({ "hex": "#d9a441" }), "a colour")
//!     .fallback(TEXT, serde_json::json!("#d9a441"));
//! assert_eq!(item.offers(), [COLOR, TEXT]);
//!
//! let to_gray = TransformSpec::new("demo.color-to-gray", COLOR, GRAY).lossy();
//! // A target taking colours takes the item as it is.
//! let plan = clipboard::negotiate(&item.offers(), &[COLOR, TEXT], &[to_gray.clone()]).unwrap();
//! assert_eq!((plan.format, plan.transform), (COLOR, None));
//! // A target taking greys only gets it through the transform.
//! let plan = clipboard::negotiate(&item.offers(), &[GRAY], &[to_gray]).unwrap();
//! assert_eq!(plan.transform.as_deref(), Some("demo.color-to-gray"));
//! assert!(plan.lossy);
//! // A target taking images refuses it, saying why.
//! assert!(clipboard::negotiate(&item.offers(), &[clipboard::PNG], &[]).is_err());
//! ```

use std::borrow::Cow;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value as Json;

use crate::commands::Origin;

/// A format: a MIME-like type, `type/subtype` with optional parameters.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FormatId(Cow<'static, str>);

/// Plain text: every item renders one, and external text arrives as it.
pub const TEXT: FormatId = FormatId::from_static("text/plain");
/// HTML, with a text alternative.
pub const HTML: FormatId = FormatId::from_static("text/html");
/// An image.
pub const PNG: FormatId = FormatId::from_static("image/png");
/// A list of files or links.
pub const URI_LIST: FormatId = FormatId::from_static("text/uri-list");

const fn valid(text: &str) -> Result<(), &'static str> {
    let b = text.as_bytes();
    if b.is_empty() {
        return Err("it is empty");
    }
    let mut slash = 0;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'/' {
            slash += 1;
        } else if !(c.is_ascii_alphanumeric()
            || matches!(c, b'.' | b'_' | b'-' | b'+' | b';' | b'=' | b' '))
        {
            return Err("only ASCII letters, digits and `/ . _ - + ; =` are allowed");
        }
        i += 1;
    }
    if slash != 1 || b[0] == b'/' || b[b.len() - 1] == b'/' {
        return Err("it is not `type/subtype`");
    }
    Ok(())
}

impl FormatId {
    /// A format from a literal; an invalid one panics (a compile error in
    /// a constant).
    pub const fn from_static(text: &'static str) -> Self {
        match valid(text) {
            Ok(()) => Self(Cow::Borrowed(text)),
            Err(_) => panic!("not a format: expected `type/subtype`"),
        }
    }

    /// A format from a text, or why it is not one.
    pub fn new(text: &str) -> Result<Self, String> {
        valid(text)
            .map(|()| Self(Cow::Owned(text.to_string())))
            .map_err(|e| format!("`{text}` is not a format: {e}"))
    }

    /// An application's private type: `application/x-<app>.<type>+json`.
    pub fn private(app: &str, kind: &str) -> Result<Self, String> {
        Self::new(&format!("application/x-{app}.{kind}+json"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether it is a private type (`application/x-…+json`).
    pub fn is_private(&self) -> bool {
        self.0.starts_with("application/x-") && self.0.ends_with("+json")
    }

    /// Whether its data is text (`text/…`): a JSON string.
    pub fn is_text(&self) -> bool {
        self.0.starts_with("text/")
    }
}

impl fmt::Display for FormatId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for FormatId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl Serialize for FormatId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for FormatId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        FormatId::new(&text).map_err(serde::de::Error::custom)
    }
}

/// Where an item is kept in the owner: the user's slot, or a session's
/// own (an agent's, a script's), so that an agent neither reads nor
/// overwrites what the user copied unless the owner allows it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    User,
    /// A session, by its journal name (`agent:<name>`, `script:<name>`).
    Session(String),
}

impl Slot {
    /// The slot a run by `origin` uses unless it names another.
    pub fn of(origin: &Origin) -> Slot {
        match origin {
            Origin::User => Slot::User,
            Origin::Agent(n) => Slot::Session(format!("agent:{n}")),
            Origin::Script(n) => Slot::Session(format!("script:{n}")),
            Origin::Device(n) => Slot::Session(format!("device:{n}")),
            Origin::Remote(n) => Slot::Session(format!("remote:{n}")),
        }
    }

    /// `user`, or the session's name.
    pub fn name(&self) -> &str {
        match self {
            Slot::User => "user",
            Slot::Session(s) => s,
        }
    }

    /// The slot named `user`, or a session's.
    pub fn parse(text: &str) -> Slot {
        if text == "user" {
            Slot::User
        } else {
            Slot::Session(text.to_string())
        }
    }
}

impl fmt::Display for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl Serialize for Slot {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for Slot {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Slot::parse(&String::deserialize(d)?))
    }
}

/// The boundary a representation crosses, which sanitisers run on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Boundary {
    /// Kept inside the owner: nothing is removed by default.
    Owner,
    /// Written to the system clipboard, where other applications read
    /// it: local paths, secrets and owner-local ids go.
    System,
    /// Coming in from outside (another application's content): reduced
    /// before a target sees it.
    External,
}

/// One representation of an item: a format and its data (a JSON string
/// for a text format).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Repr {
    pub format: FormatId,
    pub data: Json,
}

impl Repr {
    pub fn new(format: FormatId, data: Json) -> Self {
        Self { format, data }
    }

    /// Its data as text, when it is a string.
    pub fn text(&self) -> Option<&str> {
        self.data.as_str()
    }
}

/// Where an item came from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ClipOrigin {
    /// Who copied it, as the journal names them (`user`, `agent:<name>`).
    #[serde(default)]
    pub by: String,
    /// The source it was copied from (its registered name).
    #[serde(default)]
    pub source: String,
    /// The window it was copied in, when there are several.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
    /// The place of the context stack it was copied in (ADR 0012).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,
    /// The owner (kernel) that keeps it, when it names itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

/// What a sanitiser removed, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stripped {
    /// The sanitiser.
    pub by: String,
    /// What went ("a local path", "3 control characters").
    pub what: String,
    pub boundary: Boundary,
    /// The representation it went from.
    pub format: FormatId,
}

/// A title as a catalogue message: its key and its arguments.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitleKey {
    pub key: String,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub args: std::collections::BTreeMap<String, String>,
}

/// What one copy made: a typed item with its readable fallbacks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipItem {
    /// Its type: a private type, or a standard format for content that
    /// came from outside.
    pub kind: FormatId,
    /// The version of its type it was made at.
    pub version: u32,
    pub payload: Json,
    /// What it is, for menus and toasts ("a colour", "3 nodes"), in
    /// English.
    pub title: String,
    /// Its title as a catalogue message, shown in the interface's
    /// language (`title` is its English).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_key: Option<TitleKey>,
    /// Its other representations, richest first (text last).
    #[serde(default)]
    pub fallbacks: Vec<Repr>,
    #[serde(default)]
    pub origin: ClipOrigin,
    /// What sanitisers removed on the boundaries it crossed.
    #[serde(default)]
    pub stripped: Vec<Stripped>,
    /// Whether it came from outside the owner (the system clipboard).
    #[serde(default)]
    pub external: bool,
}

impl ClipItem {
    pub fn new(kind: FormatId, version: u32, payload: Json, title: &str) -> Self {
        Self {
            kind,
            version,
            payload,
            title: title.into(),
            title_key: None,
            fallbacks: Vec::new(),
            origin: ClipOrigin::default(),
            stripped: Vec::new(),
            external: false,
        }
    }

    /// Text from outside the owner (another application's).
    pub fn external_text(text: &str) -> Self {
        let mut item = Self::new(TEXT, 0, Json::String(text.into()), "text");
        item.external = true;
        item
    }

    /// Its title as a catalogue message with its arguments (`{ $count }`
    /// nodes): the interface shows it in its language, the English
    /// `title` where the catalogue has no translation.
    pub fn title_key(mut self, key: &str, args: &[(&str, &str)]) -> Self {
        self.title_key = Some(TitleKey {
            key: key.into(),
            args: args
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        });
        self
    }

    /// Adds a fallback representation.
    pub fn fallback(mut self, format: FormatId, data: Json) -> Self {
        self.fallbacks.push(Repr::new(format, data));
        self
    }

    /// The formats it offers: its type, then its fallbacks, each once.
    pub fn offers(&self) -> Vec<FormatId> {
        let mut out = vec![self.kind.clone()];
        for r in &self.fallbacks {
            if !out.contains(&r.format) {
                out.push(r.format.clone());
            }
        }
        out
    }

    /// Its representation in a format, when it offers it.
    pub fn repr(&self, format: &FormatId) -> Option<Repr> {
        if *format == self.kind {
            return Some(Repr::new(self.kind.clone(), self.payload.clone()));
        }
        self.fallbacks.iter().find(|r| r.format == *format).cloned()
    }

    /// Its plain text fallback (or its payload, when it is text).
    pub fn text(&self) -> Option<&str> {
        if self.kind == TEXT {
            return self.payload.as_str();
        }
        self.fallbacks
            .iter()
            .find(|r| r.format == TEXT)
            .and_then(Repr::text)
    }
}

/// A transform a plugin declares: it makes `to` from `from`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransformSpec {
    pub id: String,
    pub from: FormatId,
    pub to: FormatId,
    /// Whether it loses information (a colour out of gamut, a dropped
    /// parameter): applied with a toast naming the loss.
    #[serde(default)]
    pub lossy: bool,
}

impl TransformSpec {
    pub fn new(id: &str, from: FormatId, to: FormatId) -> Self {
        Self {
            id: id.into(),
            from,
            to,
            lossy: false,
        }
    }

    pub fn lossy(mut self) -> Self {
        self.lossy = true;
        self
    }
}

/// How a paste goes: the representation taken from the offer, what the
/// target receives, and the transform between them, if any.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// The representation taken from the offer.
    pub format: FormatId,
    /// The format the target receives.
    pub to: FormatId,
    /// The transform from `format` to `to`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<String>,
    #[serde(default)]
    pub lossy: bool,
}

/// Why nothing on the clipboard fits a target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refusal {
    pub reason: String,
    pub offered: Vec<FormatId>,
    pub accepted: Vec<FormatId>,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}

/// Every way an offer fits a target, the best first: per accepted format
/// (richest first), the offer's own representation, or else each
/// transform making it from what is offered (from the richest offered).
pub fn candidates(
    offer: &[FormatId],
    accepts: &[FormatId],
    transforms: &[TransformSpec],
) -> Vec<Plan> {
    let mut out = Vec::new();
    for to in accepts {
        if offer.contains(to) {
            out.push(Plan {
                format: to.clone(),
                to: to.clone(),
                transform: None,
                lossy: false,
            });
        }
        if offer.contains(to) {
            // Offered as it is: no transform needed.
            continue;
        }
        for from in offer {
            for t in transforms.iter().filter(|t| t.to == *to && t.from == *from) {
                out.push(Plan {
                    format: from.clone(),
                    to: to.clone(),
                    transform: Some(t.id.clone()),
                    lossy: t.lossy,
                });
            }
        }
    }
    out
}

/// The plan of a paste: the first accepted format the offer has, or one
/// a transform makes from it; nothing: why not.
pub fn negotiate(
    offer: &[FormatId],
    accepts: &[FormatId],
    transforms: &[TransformSpec],
) -> Result<Plan, Refusal> {
    choose(offer, accepts, transforms, None, None)
}

/// [`negotiate`], held to the format the target should receive
/// (`format`) and the transform to use (`transform`), when given: what
/// paste special and an agent's paste ask for.
pub fn choose(
    offer: &[FormatId],
    accepts: &[FormatId],
    transforms: &[TransformSpec],
    format: Option<&FormatId>,
    transform: Option<&str>,
) -> Result<Plan, Refusal> {
    candidates(offer, accepts, transforms)
        .into_iter()
        .find(|p| {
            format.is_none_or(|f| p.to == *f)
                && transform.is_none_or(|t| p.transform.as_deref() == Some(t))
        })
        .ok_or_else(|| {
            let reason = if offer.is_empty() {
                "the clipboard is empty".to_string()
            } else if let Some(t) = transform {
                format!("the transform `{t}` does not apply here")
            } else if let Some(f) = format {
                format!("`{f}` cannot be made from what the clipboard holds")
            } else {
                format!(
                    "nothing on the clipboard fits here: it holds {}, this takes {}",
                    list(offer),
                    list(accepts)
                )
            };
            Refusal {
                reason,
                offered: offer.to_vec(),
                accepted: accepts.to_vec(),
            }
        })
}

fn list(formats: &[FormatId]) -> String {
    if formats.is_empty() {
        return "nothing".into();
    }
    formats
        .iter()
        .map(|f| format!("`{f}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The marker of a text written to the system clipboard: 64-bit FNV-1a.
/// A paste whose text has the marker of the owner's last write is the
/// owner's own item, not external content.
pub fn marker(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Text without control characters: escape sequences (`ESC [ … final`,
/// `ESC ] … BEL`) and every C0 and C1 control but tab, line feed and
/// carriage return. The text and how many characters went.
pub fn strip_controls(text: &str) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut removed = 0;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            removed += 1;
            match chars.peek() {
                Some('[') => {
                    // CSI: parameters, then one final byte in @..~.
                    chars.next();
                    removed += 1;
                    for n in chars.by_ref() {
                        removed += 1;
                        if ('@'..='~').contains(&n) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    // OSC: up to BEL or ESC \.
                    chars.next();
                    removed += 1;
                    while let Some(n) = chars.next() {
                        removed += 1;
                        if n == '\u{7}' {
                            break;
                        }
                        if n == '\u{1b}' && chars.peek() == Some(&'\\') {
                            chars.next();
                            removed += 1;
                            break;
                        }
                    }
                }
                Some(_) => {
                    chars.next();
                    removed += 1;
                }
                None => {}
            }
            continue;
        }
        if c.is_control() && !matches!(c, '\t' | '\n' | '\r') {
            removed += 1;
            continue;
        }
        out.push(c);
    }
    (out, removed)
}

/// HTML reduced to its text: tags removed (with the content of `script`
/// and `style`), block ends as line breaks, the common entities decoded.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let lower = html.to_ascii_lowercase();
    let mut i = 0;
    let b = html.as_bytes();
    while i < b.len() {
        if b[i] == b'<' {
            let end = html[i..].find('>').map_or(b.len(), |e| i + e + 1);
            let tag = &lower[i..end];
            let name: String = tag
                .trim_start_matches(['<', '/'])
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if !tag.starts_with("</") && (name == "script" || name == "style") {
                let close = format!("</{name}");
                i = lower[end..].find(&close).map_or(b.len(), |c| {
                    let at = end + c;
                    lower[at..].find('>').map_or(b.len(), |e| at + e + 1)
                });
                continue;
            }
            if matches!(
                name.as_str(),
                "br" | "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
            ) && (tag.starts_with("</") || name == "br")
                && !out.ends_with('\n')
            {
                out.push('\n');
            }
            i = end;
            continue;
        }
        let next = html[i..].find('<').map_or(b.len(), |n| i + n);
        out.push_str(&decode_entities(&html[i..next]));
        i = next;
    }
    out.trim_end_matches('\n').to_string()
}

fn decode_entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const COLOR: FormatId = FormatId::from_static("application/x-demo.color+json");
    const GRAY: FormatId = FormatId::from_static("application/x-demo.gray+json");

    #[test]
    fn formats_are_mime_like_and_private_types_are_recognised() {
        assert!(COLOR.is_private());
        assert!(!TEXT.is_private());
        assert!(TEXT.is_text());
        assert_eq!(
            FormatId::private("egui-pop", "color").unwrap().as_str(),
            "application/x-egui-pop.color+json"
        );
        assert!(FormatId::new("no-slash").is_err());
        assert!(FormatId::new("text/plain;charset=utf-8").is_ok());
        assert_eq!(serde_json::to_value(&TEXT).unwrap(), json!("text/plain"));
        let back: FormatId = serde_json::from_value(json!("text/html")).unwrap();
        assert_eq!(back, HTML);
    }

    #[test]
    fn slots_follow_the_origin() {
        assert_eq!(Slot::of(&Origin::User), Slot::User);
        let agent = Slot::of(&Origin::Agent("a".into()));
        assert_eq!(agent.name(), "agent:a");
        assert_eq!(Slot::parse("agent:a"), agent);
        assert_eq!(serde_json::to_value(&Slot::User).unwrap(), json!("user"));
    }

    #[test]
    fn an_item_offers_its_type_then_its_fallbacks() {
        let item = ClipItem::new(COLOR, 2, json!({ "hex": "#102030" }), "a colour")
            .fallback(HTML, json!("<b>#102030</b>"))
            .fallback(TEXT, json!("#102030"));
        assert_eq!(item.offers(), [COLOR, HTML, TEXT]);
        assert_eq!(item.text(), Some("#102030"));
        assert_eq!(item.repr(&COLOR).unwrap().data["hex"], "#102030");
        assert_eq!(item.repr(&PNG), None);
        let ext = ClipItem::external_text("hello");
        assert!(ext.external);
        assert_eq!(ext.offers(), [TEXT]);
        assert_eq!(ext.text(), Some("hello"));
    }

    #[test]
    fn negotiation_takes_the_richest_accepted_then_a_transform() {
        let offer = [COLOR, TEXT];
        let to_gray = TransformSpec::new("gray", COLOR, GRAY).lossy();
        let parse = TransformSpec::new("parse", TEXT, COLOR);
        // Direct beats a transform for the same accepted format.
        let p = negotiate(&offer, &[COLOR], std::slice::from_ref(&parse)).unwrap();
        assert_eq!(p.transform, None);
        // The target's order wins over the offer's.
        let p = negotiate(&offer, &[TEXT, COLOR], &[]).unwrap();
        assert_eq!(p.to, TEXT);
        // Only through a transform; one, never a chain.
        let p = negotiate(&offer, &[GRAY], std::slice::from_ref(&to_gray)).unwrap();
        assert_eq!((p.format, p.to, p.lossy), (COLOR, GRAY, true));
        let chain = negotiate(&[TEXT], &[GRAY], &[parse.clone(), to_gray.clone()]);
        assert!(chain.is_err());
        // Text from outside parsed into a colour.
        let p = negotiate(&[TEXT], &[COLOR], std::slice::from_ref(&parse)).unwrap();
        assert_eq!(p.transform.as_deref(), Some("parse"));
    }

    #[test]
    fn a_refusal_says_what_was_offered_and_accepted() {
        let r = negotiate(&[COLOR], &[PNG], &[]).unwrap_err();
        assert!(r.reason.contains("application/x-demo.color+json"), "{r}");
        assert!(r.reason.contains("image/png"), "{r}");
        assert_eq!(r.offered, [COLOR]);
        let empty = negotiate(&[], &[TEXT], &[]).unwrap_err();
        assert_eq!(empty.reason, "the clipboard is empty");
    }

    #[test]
    fn paste_special_lists_every_way_and_can_be_held_to_one() {
        let offer = [COLOR, TEXT];
        let to_gray = TransformSpec::new("gray", COLOR, GRAY).lossy();
        let all = candidates(&offer, &[COLOR, GRAY, TEXT], std::slice::from_ref(&to_gray));
        assert_eq!(all.len(), 3);
        let p = choose(
            &offer,
            &[COLOR, GRAY, TEXT],
            std::slice::from_ref(&to_gray),
            Some(&TEXT),
            None,
        )
        .unwrap();
        assert_eq!(p.to, TEXT);
        let p = choose(
            &offer,
            &[COLOR, GRAY],
            std::slice::from_ref(&to_gray),
            None,
            Some("gray"),
        )
        .unwrap();
        assert_eq!(p.to, GRAY);
        assert!(choose(&offer, &[COLOR], &[to_gray], None, Some("gray")).is_err());
    }

    #[test]
    fn control_characters_and_escapes_are_stripped() {
        let (t, n) = strip_controls("ls\u{1b}[201~; rm -rf /\u{7}\n\tok\u{1b}]0;title\u{7}!");
        assert_eq!(t, "ls; rm -rf /\n\tok!");
        assert!(n > 0);
        assert_eq!(strip_controls("plain\r\n"), ("plain\r\n".to_string(), 0));
    }

    #[test]
    fn html_is_reduced_to_its_text() {
        let t = html_to_text(
            "<p>A &amp; B</p><script>alert(1)</script><style>p{}</style><div>C<br>D</div>",
        );
        assert_eq!(t, "A & B\nC\nD");
    }

    #[test]
    fn the_marker_is_stable() {
        assert_eq!(marker(""), 0xcbf2_9ce4_8422_2325);
        assert_ne!(marker("a"), marker("b"));
    }
}
