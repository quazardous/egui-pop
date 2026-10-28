//! Tips declared in files: `tips/<id>.md`, a header between `---` lines,
//! then a `# Title` heading and the text.
//!
//! ```text
//! ---
//! id: photos.rate
//! surface: focusedPanel == 'photos.grid'
//! command: photos.rate
//! target: photos.rating
//! ---
//! # Rate quickly
//!
//! Press {key} to rate the selected photos, **0** to clear the rating.
//! ```
//!
//! | Header | What | Default |
//! |---|---|---|
//! | `id` | the tip's id, prefixed by its plugin's | the file's name |
//! | `surface` | a `when` clause: where the tip makes sense | anywhere |
//! | `command` | the command it teaches; once used, the tip is not picked | none |
//! | `target` | the name of what it is about ([`super::target`]) | none: a corner |
//! | `contextual` | `true`: shown only where the application asks ([`super::hint`]) | `false` |
//!
//! The text keeps paragraphs (a blank line), `**bold**`, `` `code` ``,
//! `{key}` (the command's key binding) and `{key:<command>}` (another
//! command's).

use std::fmt;
use std::path::Path;

use egui_pop_model::ids::CommandId;

use super::Tip;

/// What is wrong with a tip's file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TipError {
    /// The file's name (`photos.rate.md`).
    pub file: String,
    /// The line, from 1 (0: the whole file).
    pub line: usize,
    pub message: String,
}

impl fmt::Display for TipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}: {}", self.file, self.message)
        } else {
            write!(f, "{}:{}: {}", self.file, self.line, self.message)
        }
    }
}

impl std::error::Error for TipError {}

/// The header's keys.
pub const HEADER_KEYS: [&str; 5] = ["id", "surface", "command", "target", "contextual"];

/// Reads a tip from a file's text; `file` is its name (`photos.rate.md`),
/// whose stem is the id when the header has none.
pub fn parse(file: &str, source: &str) -> Result<Tip, TipError> {
    let err = |line: usize, message: String| TipError {
        file: file.to_string(),
        line,
        message,
    };
    let stem = file.strip_suffix(".md").unwrap_or(file);
    let lines: Vec<&str> = source.lines().collect();
    let mut at = 0;
    while at < lines.len() && lines[at].trim().is_empty() {
        at += 1;
    }
    let mut header: Vec<(usize, &str, &str)> = Vec::new();
    if lines.get(at).map(|l| l.trim()) == Some("---") {
        at += 1;
        loop {
            let Some(line) = lines.get(at) else {
                return Err(err(at, "the header has no closing `---`".into()));
            };
            at += 1;
            let line = line.trim();
            if line == "---" {
                break;
            }
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once(':') else {
                return Err(err(at, format!("`{line}` is not `key: value`")));
            };
            let (k, v) = (k.trim(), v.trim());
            if !HEADER_KEYS.contains(&k) {
                return Err(err(
                    at,
                    format!("unknown key `{k}` (known: {})", HEADER_KEYS.join(", ")),
                ));
            }
            if header.iter().any(|(_, h, _)| *h == k) {
                return Err(err(at, format!("`{k}` is given twice")));
            }
            header.push((at, k, v));
        }
    }
    let get = |k: &str| {
        header
            .iter()
            .find(|(_, h, _)| *h == k)
            .map(|(_, _, v)| unquote(v))
    };
    let id = get("id").unwrap_or(stem).to_string();
    if id.is_empty() || id.chars().any(char::is_whitespace) {
        return Err(err(0, format!("`{id}` is not a tip id")));
    }
    while at < lines.len() && lines[at].trim().is_empty() {
        at += 1;
    }
    let Some(title) = lines
        .get(at)
        .and_then(|l| l.trim().strip_prefix("# "))
        .map(str::trim)
        .filter(|t| !t.is_empty())
    else {
        return Err(err(
            at + 1,
            "the text starts with a `# Title` heading".into(),
        ));
    };
    at += 1;
    let text = paragraphs(&lines[at..]);
    if text.is_empty() {
        return Err(err(0, "the tip has no text".into()));
    }
    let contextual = match get("contextual") {
        None | Some("false") => false,
        Some("true") => true,
        Some(v) => {
            return Err(err(
                0,
                format!("`contextual` is `true` or `false`, not `{v}`"),
            ));
        }
    };
    let mut tip = Tip::new(&id, title, &text);
    tip.contextual = contextual;
    tip.surface = get("surface").unwrap_or_default().to_string();
    tip.command = match get("command").filter(|c| !c.is_empty()) {
        None => None,
        Some(c) => Some(CommandId::new(c).map_err(|e| err(0, e.to_string()))?),
    };
    tip.target = get("target").filter(|t| !t.is_empty()).map(str::to_string);
    Ok(tip)
}

/// A value without its quotes, when it has a pair.
fn unquote(v: &str) -> &str {
    for q in ['"', '\''] {
        if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
            return &v[1..v.len() - 1];
        }
    }
    v
}

/// The lines as paragraphs: a paragraph's lines joined by a space,
/// paragraphs by a line break.
fn paragraphs(lines: &[&str]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for l in lines {
        let l = l.trim();
        if l.is_empty() {
            if !cur.is_empty() {
                out.push(cur.join(" "));
                cur.clear();
            }
        } else {
            cur.push(l);
        }
    }
    if !cur.is_empty() {
        out.push(cur.join(" "));
    }
    out.join("\n")
}

/// Reads every `*.md` file of a folder, in the order of their names:
/// the tips read, and the files that could not be.
pub fn read_dir(dir: &Path) -> (Vec<Tip>, Vec<TipError>) {
    let mut tips = Vec::new();
    let mut errors = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            errors.push(TipError {
                file: dir.display().to_string(),
                line: 0,
                message: e.to_string(),
            });
            return (tips, errors);
        }
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    files.sort();
    for path in files {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        match std::fs::read_to_string(&path) {
            Ok(text) => match parse(&name, &text) {
                Ok(t) => tips.push(t),
                Err(e) => errors.push(e),
            },
            Err(e) => errors.push(TipError {
                file: name,
                line: 0,
                message: e.to_string(),
            }),
        }
    }
    (tips, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_gives_its_header_title_and_text() {
        let t = parse(
            "photos.rate.md",
            "---\nsurface: focusedPanel == 'photos.grid'\ncommand: photos.rate\ntarget: \"photos.rating\"\n---\n\n# Rate quickly\n\nPress {key} to rate\nthe photos.\n\nPress **0** to clear.\n",
        )
        .unwrap();
        assert_eq!(t.id, "photos.rate");
        assert_eq!(t.title, "Rate quickly");
        assert_eq!(
            t.text,
            "Press {key} to rate the photos.\nPress **0** to clear."
        );
        assert_eq!(t.surface, "focusedPanel == 'photos.grid'");
        assert_eq!(t.command.as_ref().map(|c| c.as_str()), Some("photos.rate"));
        assert_eq!(t.target.as_deref(), Some("photos.rating"));
        assert!(!t.contextual);
    }

    #[test]
    fn a_header_is_optional_and_mistakes_say_where() {
        let t = parse("a.b.md", "# Title\nText.").unwrap();
        assert_eq!((t.id.as_str(), t.surface.as_str()), ("a.b", ""));
        let e = parse("x.md", "---\nwhen: x\n---\n# T\nText").unwrap_err();
        assert_eq!(e.line, 2);
        assert!(e.message.contains("unknown key `when`"), "{e}");
        assert!(parse("x.md", "---\nid: x\n# T\nText").is_err());
        assert!(parse("x.md", "Text without a title").is_err());
        assert!(parse("x.md", "# Title only").is_err());
        assert!(parse("x.md", "---\ncontextual: maybe\n---\n# T\nText").is_err());
    }
}
