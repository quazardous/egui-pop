//! The file's text, line by line: where its sections, items and keys
//! are (block style only), the splices an edit makes, and how a part
//! is written when it has to be.
//!
//! Nothing here has to be exact: every splice is read back and compared
//! with the graph it should give, and a part is written again whole
//! when the text does not match (see `super::YamlGraph::apply`).

use serde_json::Value as Json;

use super::entries::{EdgeEntry, Field, NodeEntry, ParamEntry, PortEntry};

/// A line: where it starts, where its content ends (before `\r\n` or
/// `\n`), and where the next one starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Line {
    pub start: usize,
    pub end: usize,
    pub next: usize,
}

/// The text cut into lines (a byte order mark is not part of the first).
#[derive(Debug)]
pub(crate) struct Text<'a> {
    pub src: &'a str,
    pub lines: Vec<Line>,
}

impl<'a> Text<'a> {
    pub fn new(src: &'a str) -> Text<'a> {
        let mut lines = Vec::new();
        let mut start = if src.starts_with('\u{feff}') { 3 } else { 0 };
        while start < src.len() {
            let (end, next) = match src[start..].find('\n') {
                Some(i) => (start + i, start + i + 1),
                None => (src.len(), src.len()),
            };
            let end = if end > start && src.as_bytes()[end - 1] == b'\r' {
                end - 1
            } else {
                end
            };
            lines.push(Line { start, end, next });
            start = next;
        }
        Text { src, lines }
    }

    /// The text's line ending: `\r\n` when it uses it.
    pub fn newline(&self) -> &'static str {
        if self.src.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        }
    }

    pub fn content(&self, i: usize) -> &'a str {
        let l = self.lines[i];
        &self.src[l.start..l.end]
    }

    pub fn indent(&self, i: usize) -> usize {
        let c = self.content(i);
        c.len() - c.trim_start_matches(' ').len()
    }

    /// A blank line or a comment.
    pub fn is_gap(&self, i: usize) -> bool {
        let t = self.content(i).trim_start();
        t.is_empty() || t.starts_with('#')
    }

    fn is_comment(&self, i: usize) -> bool {
        self.content(i).trim_start().starts_with('#')
    }

    /// Where a line starts (the end of the text past the last).
    pub fn pos(&self, i: usize) -> usize {
        self.lines.get(i).map_or(self.src.len(), |l| l.start)
    }

    /// The last line that is not a gap in `from..to`, plus one.
    pub fn content_end(&self, from: usize, to: usize) -> Option<usize> {
        (from..to).rev().find(|&i| !self.is_gap(i)).map(|i| i + 1)
    }

    /// A line's value after its key at `col`, without its comment.
    pub fn inline_value(&self, i: usize, col: usize) -> Option<&'a str> {
        let c = self.content(i);
        let (_, rest) = parse_key(c.get(col..)?)?;
        let rest = &c[col + rest..];
        let cut = comment_start(rest).unwrap_or(rest.len());
        Some(rest[..cut].trim())
    }
}

/// A key at the start of `s`: the key and where its value starts (after
/// the colon). Plain, single-quoted or double-quoted keys.
pub(crate) fn parse_key(s: &str) -> Option<(String, usize)> {
    let b = s.as_bytes();
    let colon_ok = |i: usize| {
        i < b.len() && b[i] == b':' && (i + 1 == b.len() || b[i + 1] == b' ' || b[i + 1] == b'\t')
    };
    match b.first()? {
        b'"' => {
            let mut i = 1;
            while i < b.len() && b[i] != b'"' {
                i += if b[i] == b'\\' { 2 } else { 1 };
            }
            if i >= b.len() {
                return None;
            }
            let key =
                serde_json::from_str::<String>(&s[..=i]).unwrap_or_else(|_| s[1..i].to_string());
            let after = i + 1 + (s[i + 1..].len() - s[i + 1..].trim_start().len());
            colon_ok(after).then(|| (key, after + 1))
        }
        b'\'' => {
            let mut i = 1;
            loop {
                if i >= b.len() {
                    return None;
                }
                if b[i] == b'\'' {
                    if b.get(i + 1) == Some(&b'\'') {
                        i += 2;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            let key = s[1..i].replace("''", "'");
            let after = i + 1 + (s[i + 1..].len() - s[i + 1..].trim_start().len());
            colon_ok(after).then(|| (key, after + 1))
        }
        b'-' if b.len() == 1 || b[1] == b' ' => None,
        b'#' | b'[' | b'{' | b'?' | b'&' | b'*' | b'!' | b'|' | b'>' | b'%' | b'@' | b'`' => None,
        _ => {
            let i = (0..b.len()).find(|&i| colon_ok(i))?;
            let key = s[..i].trim_end();
            (!key.contains(" #")).then(|| (key.to_string(), i + 1))
        }
    }
}

/// Where a comment starts in a value (a `#` after a space, outside
/// quotes).
pub(crate) fn comment_start(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut quote = None;
    let mut i = 0;
    while i < b.len() {
        match (quote, b[i]) {
            (None, b'"') | (None, b'\'') => quote = Some(b[i]),
            (Some(b'"'), b'\\') => i += 1,
            (Some(q), c) if c == q => quote = None,
            (None, b'#') if i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t' => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// A line with its value after the key at `col` replaced, the comment
/// after it kept.
pub(crate) fn replace_value(line: &str, col: usize, value: &str) -> Option<String> {
    let (_, rest) = parse_key(line.get(col..)?)?;
    let head = &line[..col + rest];
    let rest = &line[col + rest..];
    let cut = comment_start(rest).unwrap_or(rest.len());
    let value_end = rest[..cut].trim_end().len();
    let lead = rest.len() - rest.trim_start().len();
    let space = if lead == 0 || lead > value_end {
        " "
    } else {
        &rest[..lead]
    };
    let tail = &rest[value_end.max(lead.min(value_end))..];
    let tail = if value_end == 0 && !tail.is_empty() && !tail.starts_with(' ') {
        format!(" {tail}")
    } else {
        tail.to_string()
    };
    Some(format!("{head}{space}{value}{tail}"))
}

/// A top-level key and its lines: the key's line, up to the next one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Section {
    pub key: String,
    pub header: usize,
    pub end: usize,
}

/// The top level's keys, or the first line that is not one of them nor
/// inside one (the top level is not a block mapping).
pub(crate) fn sections(t: &Text<'_>) -> Result<Vec<Section>, usize> {
    let mut out: Vec<Section> = Vec::new();
    let mut open = false;
    for i in 0..t.lines.len() {
        if t.is_gap(i) {
            continue;
        }
        let c = t.content(i);
        if t.indent(i) > 0 || c == "-" || c.starts_with("- ") {
            if !open {
                return Err(i);
            }
            continue;
        }
        if c.starts_with("---") || c.starts_with("...") || c.starts_with('%') {
            if let Some(s) = out.last_mut().filter(|_| open) {
                s.end = i;
            }
            open = false;
            continue;
        }
        let Some((key, _)) = parse_key(c) else {
            return Err(i);
        };
        if let Some(s) = out.last_mut().filter(|_| open) {
            s.end = i;
        }
        out.push(Section {
            key,
            header: i,
            end: t.lines.len(),
        });
        open = true;
    }
    Ok(out)
}

/// An entry of a block: a sequence item or a key. `lead` is the first
/// of the comments right above it (they go with it), `end` the line
/// after its last line that is not a gap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub lead: usize,
    pub start: usize,
    pub end: usize,
}

fn entries(t: &Text<'_>, starts: &[usize], from: usize, to: usize) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::with_capacity(starts.len());
    for (k, &start) in starts.iter().enumerate() {
        let next = starts.get(k + 1).copied().unwrap_or(to);
        let end = t.content_end(start, next).unwrap_or(start + 1);
        let floor = out.last().map_or(from, |e| e.end);
        let mut lead = start;
        while lead > floor && t.is_comment(lead - 1) {
            lead -= 1;
        }
        out.push(Entry { lead, start, end });
    }
    out
}

fn is_item(c: &str) -> bool {
    c == "-" || c.starts_with("- ")
}

/// A section's sequence items (`- ...` lines), `None` when its value is
/// not a block sequence (a flow one, `[...]`, or something else).
pub(crate) fn seq_items(t: &Text<'_>, s: &Section) -> Option<Vec<Entry>> {
    let inline = t.inline_value(s.header, 0)?;
    let first = (s.header + 1..s.end).find(|&i| !t.is_gap(i));
    match (inline, first) {
        ("" | "[]" | "~" | "null", None) => return Some(Vec::new()),
        ("", Some(_)) => {}
        _ => return None,
    }
    let first = first?;
    let indent = t.indent(first);
    let mut starts = Vec::new();
    for i in first..s.end {
        if t.is_gap(i) {
            continue;
        }
        let n = t.indent(i);
        if n < indent {
            return None;
        }
        if n == indent {
            if !is_item(&t.content(i)[n..]) {
                return None;
            }
            starts.push(i);
        }
    }
    let end = t.content_end(s.header + 1, s.end).unwrap_or(s.header + 1);
    Some(entries(t, &starts, s.header + 1, end))
}

/// A key of a block mapping and its lines; `col` is the key's column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KeyEntry {
    pub key: String,
    pub col: usize,
    pub entry: Entry,
}

/// The keys of a block mapping in `from..to`, at `col` or at the first
/// key's column; `None` when the lines are not one.
fn block_keys(
    t: &Text<'_>,
    mut starts: Vec<usize>,
    from: usize,
    to: usize,
    col: Option<usize>,
) -> Option<(usize, Vec<KeyEntry>)> {
    let first = (from..to).find(|&i| !t.is_gap(i));
    let col = match (col, first) {
        (Some(c), _) => c,
        (None, Some(f)) => t.indent(f),
        (None, None) => return Some((0, Vec::new())),
    };
    for i in from..to {
        if t.is_gap(i) {
            continue;
        }
        let n = t.indent(i);
        if n < col {
            return None;
        }
        if n == col {
            parse_key(&t.content(i)[col..])?;
            starts.push(i);
        }
    }
    let region = starts.first().copied().unwrap_or(from);
    let keys = entries(t, &starts, region, to)
        .into_iter()
        .map(|entry| {
            let (key, _) = parse_key(&t.content(entry.start)[col..]).unwrap_or_default();
            KeyEntry { key, col, entry }
        })
        .collect();
    Some((col, keys))
}

/// A sequence item's keys (`- id: a` and the lines below it), `None`
/// when the item is not a block mapping (a flow one, a scalar).
pub(crate) fn item_keys(t: &Text<'_>, item: &Entry) -> Option<Vec<KeyEntry>> {
    let c = t.content(item.start);
    let n = t.indent(item.start);
    let after = &c[n + 1..];
    let rest = after.trim_start();
    if rest.is_empty() || rest.starts_with('#') {
        return block_keys(t, Vec::new(), item.start + 1, item.end, None).map(|(_, k)| k);
    }
    let col = n + 1 + (after.len() - rest.len());
    parse_key(rest)?;
    block_keys(t, vec![item.start], item.start + 1, item.end, Some(col)).map(|(_, k)| k)
}

/// The keys of the block mapping under a key (`params:` and the lines
/// below it): their column (`None` when there are none) and entries;
/// `None` when the value is not a block mapping.
pub(crate) fn sub_keys(t: &Text<'_>, key: &KeyEntry) -> Option<(Option<usize>, Vec<KeyEntry>)> {
    if !t.inline_value(key.entry.start, key.col)?.is_empty() {
        return None;
    }
    let (col, keys) = block_keys(t, Vec::new(), key.entry.start + 1, key.entry.end, None)?;
    if keys.is_empty() {
        return Some((None, keys));
    }
    (col > key.col).then_some((Some(col), keys))
}

/// Text replacements: `start..end` in bytes, and what goes there.
#[derive(Debug, Default)]
pub(crate) struct Splices(Vec<(usize, usize, String)>);

impl Splices {
    /// Lines `a..b` replaced by `block` (lines ending with `\n`).
    pub fn lines(&mut self, t: &Text<'_>, a: usize, b: usize, block: &str) {
        let block = block.replace('\n', t.newline());
        let start = t.pos(a);
        let pad = if b >= t.lines.len()
            && !t.src.is_empty()
            && !t.src.ends_with('\n')
            && start == t.src.len()
        {
            t.newline()
        } else {
            ""
        };
        let end = if b >= t.lines.len() {
            t.src.len()
        } else {
            t.pos(b)
        };
        // The last line without a line ending keeps none.
        let block = if b >= t.lines.len() && a < b && !t.src.ends_with('\n') {
            block
                .strip_suffix(t.newline())
                .unwrap_or(&block)
                .to_string()
        } else {
            block
        };
        self.0.push((start, end, format!("{pad}{block}")));
    }

    /// `block` inserted before line `at`.
    pub fn insert(&mut self, t: &Text<'_>, at: usize, block: &str) {
        let block = block.replace('\n', t.newline());
        let pos = t.pos(at);
        let pad = if pos == t.src.len() && !t.src.is_empty() && !t.src.ends_with('\n') {
            t.newline()
        } else {
            ""
        };
        self.0.push((pos, pos, format!("{pad}{block}")));
    }

    /// Line `i`'s content replaced (its line ending kept).
    pub fn line(&mut self, t: &Text<'_>, i: usize, content: String) {
        let l = t.lines[i];
        self.0.push((l.start, l.end, content));
    }

    pub fn apply(mut self, src: &str) -> String {
        self.0.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
        let mut out = src.to_string();
        for (start, end, with) in self.0 {
            out.replace_range(start..end, &with);
        }
        out
    }
}

/// A number as written: `240`, `0.5`, `-3`.
pub(crate) fn num(v: f64) -> String {
    if v.is_nan() {
        ".nan".into()
    } else if v.is_infinite() {
        if v > 0.0 {
            ".inf".into()
        } else {
            "-.inf".into()
        }
    } else if v == 0.0 {
        "0".into()
    } else {
        format!("{v}")
    }
}

/// A position: `[240, 0]`.
pub(crate) fn pos(p: [f32; 2]) -> String {
    let one = |v: f32| {
        if v == 0.0 {
            "0".to_string()
        } else {
            format!("{v}")
        }
    };
    format!("[{}, {}]", one(p[0]), one(p[1]))
}

/// A string as written: plain when YAML reads it back as the same
/// string, else double-quoted.
pub(crate) fn string(s: &str) -> String {
    let reserved = matches!(
        s.to_ascii_lowercase().as_str(),
        "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "y" | "n" | "~"
    );
    let plain = !reserved
        && s.chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        && !s.ends_with(' ')
        && s.chars().all(|c| {
            c.is_alphanumeric()
                || matches!(
                    c,
                    ' ' | '_' | '-' | '.' | '/' | '>' | '+' | '(' | ')' | '$' | '='
                )
        });
    if plain {
        s.to_string()
    } else {
        serde_json::to_string(s).unwrap_or_default()
    }
}

/// A JSON value on one line, in YAML's flow style.
pub(crate) fn json(v: &Json) -> String {
    match v {
        Json::String(s) => string(s),
        Json::Array(a) => format!("[{}]", a.iter().map(json).collect::<Vec<_>>().join(", ")),
        Json::Object(o) => fields_inline(o.iter()),
        other => other.to_string(),
    }
}

fn fields_inline<'a>(fields: impl Iterator<Item = (&'a String, &'a Json)>) -> String {
    let inner: Vec<String> = fields
        .map(|(k, v)| format!("{}: {}", string(k), json(v)))
        .collect();
    format!("{{{}}}", inner.join(", "))
}

fn port_inline(p: &PortEntry) -> String {
    match p {
        PortEntry::Type(t) => string(t),
        PortEntry::Full(f) => fields_inline(f.iter().map(|(k, v)| (k, v))),
    }
}

/// A parameter on one line: its value, or its fields in flow style.
pub(crate) fn param_inline(p: &ParamEntry) -> String {
    match p {
        ParamEntry::Value(v) => num(*v),
        ParamEntry::Full(f) => fields_inline(f.iter().map(|(k, v)| (k, v))),
    }
}

/// A node's lines, without the item's `- ` nor any indent: keys in their
/// order, keys egui-pop does not know as flow values.
pub(crate) fn node_lines(n: &NodeEntry) -> Vec<String> {
    let mut out = Vec::new();
    let map = |out: &mut Vec<String>, key: &str, items: Vec<(String, String)>| {
        if items.is_empty() {
            out.push(format!("{key}: {{}}"));
        } else {
            out.push(format!("{key}:"));
            out.extend(
                items
                    .into_iter()
                    .map(|(k, v)| format!("  {}: {v}", string(&k))),
            );
        }
    };
    for (key, field) in &n.fields {
        let k = string(key);
        match field {
            Field::Id(s) | Field::Kind(s) | Field::Title(s) | Field::Group(s) => {
                out.push(format!("{k}: {}", string(s)));
            }
            Field::Pos(p) => out.push(format!("{k}: {}", pos(*p))),
            Field::Inputs(ports) | Field::Outputs(ports) => map(
                &mut out,
                &k,
                ports
                    .iter()
                    .map(|(id, p)| (id.clone(), port_inline(p)))
                    .collect(),
            ),
            Field::Params(params) => map(
                &mut out,
                &k,
                params
                    .iter()
                    .map(|(id, p)| (id.clone(), param_inline(p)))
                    .collect(),
            ),
            Field::Other(v) => out.push(format!("{k}: {}", json(v))),
        }
    }
    out
}

/// A wire's value as written in its item.
pub(crate) fn edge_inline(e: &EdgeEntry) -> String {
    match e {
        EdgeEntry::Text(s) => string(s),
        EdgeEntry::Map(f) => fields_inline(f.iter().map(|(k, v)| (k, v))),
    }
}

/// Sequence items at `indent`: `- ` before each item's first line, its
/// other lines under it.
pub(crate) fn items_block(indent: usize, items: &[Vec<String>]) -> String {
    let pad = " ".repeat(indent);
    let mut out = String::new();
    for lines in items {
        for (i, l) in lines.iter().enumerate() {
            out.push_str(&pad);
            out.push_str(if i == 0 { "- " } else { "  " });
            out.push_str(l);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_plain_and_quoted() {
        assert_eq!(parse_key("pos: [1, 2]"), Some(("pos".into(), 4)));
        assert_eq!(parse_key("nodes:"), Some(("nodes".into(), 6)));
        assert_eq!(parse_key("\"a b\": 1"), Some(("a b".into(), 6)));
        assert_eq!(parse_key("'it''s': 1"), Some(("it's".into(), 8)));
        assert_eq!(parse_key("$extensions:"), Some(("$extensions".into(), 12)));
        assert_eq!(parse_key("- id: a"), None);
        assert_eq!(parse_key("{a: 1}"), None);
        assert_eq!(parse_key("a:b"), None);
    }

    #[test]
    fn values_are_replaced_and_comments_kept() {
        assert_eq!(
            replace_value("    pos: [0, 0]   # here", 4, "[10, 20]").as_deref(),
            Some("    pos: [10, 20]   # here")
        );
        assert_eq!(
            replace_value("  - pos: [0, 0]", 4, "[1, 2]").as_deref(),
            Some("  - pos: [1, 2]")
        );
        assert_eq!(
            replace_value("radius: 2", 0, "3.5").as_deref(),
            Some("radius: 3.5")
        );
        assert_eq!(
            replace_value("radius:", 0, "3").as_deref(),
            Some("radius: 3")
        );
        assert_eq!(
            replace_value("a: \"x # y\" # c", 0, "1").as_deref(),
            Some("a: 1 # c")
        );
    }

    #[test]
    fn sections_items_and_keys() {
        let src = "# head\nnodes:\n  # the source\n  - id: a\n    pos: [0, 0]\n\n  - id: b # second\n    params:\n      r: 1\nedges: []\n";
        let t = Text::new(src);
        let s = sections(&t).unwrap();
        assert_eq!(
            s.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(),
            ["nodes", "edges"]
        );
        let items = seq_items(&t, &s[0]).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0],
            Entry {
                lead: 2,
                start: 3,
                end: 5
            }
        );
        assert_eq!(
            items[1],
            Entry {
                lead: 6,
                start: 6,
                end: 9
            }
        );
        let keys = item_keys(&t, &items[1]).unwrap();
        assert_eq!(
            keys.iter().map(|k| k.key.as_str()).collect::<Vec<_>>(),
            ["id", "params"]
        );
        assert_eq!(keys[0].col, 4);
        let (col, sub) = sub_keys(&t, &keys[1]).unwrap();
        assert_eq!(col, Some(6));
        assert_eq!(sub[0].key, "r");
        assert_eq!(seq_items(&t, &s[1]), Some(Vec::new()));
        assert!(sections(&Text::new("{a: 1}\n")).is_err());
    }

    #[test]
    fn strings_are_quoted_when_they_must_be() {
        assert_eq!(string("blur-1"), "blur-1");
        assert_eq!(
            string("load.image -> blur.image"),
            "load.image -> blur.image"
        );
        assert_eq!(string("Tone curve"), "Tone curve");
        assert_eq!(string("true"), "\"true\"");
        assert_eq!(string("12"), "\"12\"");
        assert_eq!(string("a: b"), "\"a: b\"");
        assert_eq!(string(""), "\"\"");
        assert_eq!(num(2.0), "2");
        assert_eq!(num(-0.0), "0");
        assert_eq!(num(0.1), "0.1");
        assert_eq!(pos([240.0, -12.5]), "[240, -12.5]");
    }

    #[test]
    fn splices_apply_from_the_end() {
        let t = Text::new("a: 1\nb: 2\nc: 3");
        let mut s = Splices::default();
        s.lines(&t, 1, 2, "");
        s.insert(&t, 3, "d: 4\n");
        s.line(&t, 0, "a: 9".into());
        assert_eq!(s.apply(t.src), "a: 9\nc: 3\nd: 4\n");
    }
}
