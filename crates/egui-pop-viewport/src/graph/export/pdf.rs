//! svg2pdf's output made byte-stable.
//!
//! svg2pdf writes a resource dictionary's entries and its fonts' objects
//! in the order of a `HashMap`, which changes from one run to the next.
//! This rewrites its PDF in a canonical form: the entries of a
//! dictionary whose values are references sorted by name, the objects
//! numbered and written in the order they are reached from the
//! catalogue, and fixed metadata (the title, `egui-pop` as producer, no
//! date and no file identifier). It reads only what pdf-writer writes
//! (one dictionary entry per line, streams with their `/Length`), and
//! gives `None` on anything else.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// The producer written in the metadata.
const PRODUCER: &str = "egui-pop";

/// An object of the file: its text (a dictionary, an array) and the
/// bytes of its stream, if it has one.
struct Object<'a> {
    head: Vec<u8>,
    stream: Option<&'a [u8]>,
}

/// The canonical form of a PDF svg2pdf wrote, titled `title`.
pub(super) fn canonical(pdf: &[u8], title: &str) -> Option<Vec<u8>> {
    let (objects, root, info) = parse(pdf)?;

    // The order the objects are reached in from the catalogue, the
    // references of each in the order of its (sorted) text.
    let mut order: Vec<u32> = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if order.contains(&id) || Some(id) == info {
            continue;
        }
        let object = objects.get(&id)?;
        order.push(id);
        let refs: Vec<u32> = refs(&object.head).into_iter().map(|(_, _, r)| r).collect();
        stack.extend(refs.into_iter().rev());
    }
    // What nothing references (never seen; kept rather than lost).
    for &id in objects.keys() {
        if !order.contains(&id) && Some(id) != info {
            order.push(id);
        }
    }
    let new_id: BTreeMap<u32, u32> = order
        .iter()
        .enumerate()
        .map(|(k, &id)| (id, k as u32 + 1))
        .collect();

    let mut out: Vec<u8> = b"%PDF-1.7\n%\x80\x80\x80\x80\n\n".to_vec();
    let mut offsets = Vec::with_capacity(order.len() + 1);
    for &id in &order {
        let object = &objects[&id];
        offsets.push(out.len());
        let _ = writeln!(Text(&mut out), "{} 0 obj", new_id[&id]);
        out.extend(renumber(&object.head, &new_id)?);
        if let Some(stream) = object.stream {
            out.extend_from_slice(b"\nstream\n");
            out.extend_from_slice(stream);
            out.extend_from_slice(b"\nendstream");
        }
        out.extend_from_slice(b"\nendobj\n\n");
    }
    let info_id = order.len() as u32 + 1;
    offsets.push(out.len());
    let _ = write!(
        Text(&mut out),
        "{info_id} 0 obj\n<<\n  /Title {}\n  /Producer ({PRODUCER})\n>>\nendobj\n\n",
        text_string(title)
    );

    let xref = out.len();
    let size = offsets.len() + 1;
    let _ = write!(Text(&mut out), "xref\n0 {size}\n0000000000 65535 f\r\n");
    for offset in offsets {
        let _ = write!(Text(&mut out), "{offset:010} 00000 n\r\n");
    }
    let _ = write!(
        Text(&mut out),
        "trailer\n<<\n  /Size {size}\n  /Root {} 0 R\n  /Info {info_id} 0 R\n>>\nstartxref\n{xref}\n%%EOF",
        new_id[&root]
    );
    Some(out)
}

/// The objects by number, the catalogue's number and the metadata's.
#[allow(clippy::type_complexity)]
fn parse(pdf: &[u8]) -> Option<(BTreeMap<u32, Object<'_>>, u32, Option<u32>)> {
    let mut objects = BTreeMap::new();
    let mut at = find(pdf, b"\n\n", 0)? + 2;
    loop {
        while pdf.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        if pdf[at..].starts_with(b"xref") {
            break;
        }
        let (id, len) = number(&pdf[at..])?;
        at += len;
        if !pdf[at..].starts_with(b" 0 obj\n") {
            return None;
        }
        at += b" 0 obj\n".len();
        let end = find(pdf, b"\nendobj", at)?;
        let object = match find(pdf, b">>\nstream\n", at).filter(|&s| s < end) {
            Some(s) => {
                let head = &pdf[at..s + 2];
                let len = stream_length(head)?;
                let data = s + b">>\nstream\n".len();
                let stream = pdf.get(data..data + len)?;
                let tail = data + len;
                if !pdf[tail..].starts_with(b"\nendstream\nendobj") {
                    return None;
                }
                at = tail + b"\nendstream\nendobj".len();
                Object {
                    head: sorted(head),
                    stream: Some(stream),
                }
            }
            None => {
                let head = &pdf[at..end];
                at = end + b"\nendobj".len();
                Object {
                    head: sorted(head),
                    stream: None,
                }
            }
        };
        objects.insert(id, object);
    }
    let trailer = &pdf[find(pdf, b"trailer", at)?..];
    let entry = |key: &[u8]| -> Option<u32> {
        let k = find(trailer, key, 0)? + key.len();
        number(&trailer[k..]).map(|(n, _)| n)
    };
    let root = entry(b"/Root ")?;
    Some((objects, root, entry(b"/Info ")))
}

/// A stream's `/Length`, from its dictionary.
fn stream_length(head: &[u8]) -> Option<usize> {
    let k = find(head, b"/Length ", 0)? + b"/Length ".len();
    number(&head[k..]).map(|(n, _)| n as usize)
}

/// An object's text with each run of dictionary entries whose values
/// are references (`/Name 12 0 R`, one per line at one indent) sorted
/// by name: a dictionary's order means nothing, and svg2pdf's changes.
fn sorted(head: &[u8]) -> Vec<u8> {
    let lines: Vec<&[u8]> = head.split(|&b| b == b'\n').collect();
    let entry = |line: &[u8]| -> Option<(usize, Vec<u8>)> {
        let indent = line.iter().take_while(|&&b| b == b' ').count();
        let rest = &line[indent..];
        let name_end = rest.iter().position(|&b| b == b' ')?;
        if !rest.starts_with(b"/") {
            return None;
        }
        let value = &rest[name_end + 1..];
        let (_, len) = number(value)?;
        (&value[len..] == b" 0 R").then(|| (indent, rest[..name_end].to_vec()))
    };
    let mut out: Vec<&[u8]> = Vec::with_capacity(lines.len());
    let mut k = 0;
    while k < lines.len() {
        let Some((indent, _)) = entry(lines[k]) else {
            out.push(lines[k]);
            k += 1;
            continue;
        };
        let mut run: Vec<(Vec<u8>, &[u8])> = Vec::new();
        while let Some(line) = lines.get(k)
            && let Some((i, name)) = entry(line)
            && i == indent
        {
            run.push((name, line));
            k += 1;
        }
        run.sort_by(|a, b| a.0.cmp(&b.0));
        out.extend(run.into_iter().map(|(_, line)| line));
    }
    out.join(&b'\n')
}

/// The references in an object's text: where each starts, its length
/// and the object it names.
fn refs(head: &[u8]) -> Vec<(usize, usize, u32)> {
    let mut out = Vec::new();
    let mut k = 0;
    while k < head.len() {
        let starts =
            head[k].is_ascii_digit() && (k == 0 || matches!(head[k - 1], b' ' | b'[' | b'\n'));
        if !starts {
            k += 1;
            continue;
        }
        let Some((id, len)) = number(&head[k..]) else {
            k += 1;
            continue;
        };
        let after = k + len;
        let is_ref = head[after..].starts_with(b" 0 R")
            && head
                .get(after + 4)
                .is_none_or(|b| !b.is_ascii_alphanumeric());
        if is_ref {
            out.push((k, len + 4, id));
            k = after + 4;
        } else {
            k = after;
        }
    }
    out
}

/// An object's text with its references to the new numbers.
fn renumber(head: &[u8], new_id: &BTreeMap<u32, u32>) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(head.len());
    let mut from = 0;
    for (at, len, id) in refs(head) {
        out.extend_from_slice(&head[from..at]);
        let _ = write!(Text(&mut out), "{} 0 R", new_id.get(&id)?);
        from = at + len;
    }
    out.extend_from_slice(&head[from..]);
    Some(out)
}

/// The unsigned number a text starts with, and its length.
fn number(s: &[u8]) -> Option<(u32, usize)> {
    let len = s.iter().take_while(|b| b.is_ascii_digit()).count();
    let n = std::str::from_utf8(&s[..len]).ok()?.parse().ok()?;
    Some((n, len))
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// A PDF text string: UTF-16BE with its byte order mark, in hex.
fn text_string(s: &str) -> String {
    let mut out = String::from("<FEFF");
    for unit in s.encode_utf16() {
        let _ = write!(out, "{unit:04X}");
    }
    out.push('>');
    out
}

/// `write!` into bytes.
struct Text<'a>(&'a mut Vec<u8>);

impl std::fmt::Write for Text<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_entries_are_sorted_by_name() {
        let head = b"<<\n  /ExtGState <<\n    /g1 7 0 R\n    /g0 5 0 R\n  >>\n  /Font <<\n    /f0 9 0 R\n  >>\n>>";
        assert_eq!(
            sorted(head),
            b"<<\n  /ExtGState <<\n    /g0 5 0 R\n    /g1 7 0 R\n  >>\n  /Font <<\n    /f0 9 0 R\n  >>\n>>"
        );
    }

    #[test]
    fn references_are_found_and_renumbered() {
        let head = b"<<\n  /Kids [3 0 R 12 0 R]\n  /W [0 [500 0] 3 [250]]\n>>";
        let found: Vec<u32> = refs(head).into_iter().map(|r| r.2).collect();
        assert_eq!(found, [3, 12]);
        let map = BTreeMap::from([(3, 1), (12, 2)]);
        assert_eq!(
            renumber(head, &map).unwrap(),
            b"<<\n  /Kids [1 0 R 2 0 R]\n  /W [0 [500 0] 3 [250]]\n>>"
        );
    }

    #[test]
    fn titles_are_utf16_in_hex() {
        assert_eq!(text_string("Né"), "<FEFF004E00E9>");
    }
}
