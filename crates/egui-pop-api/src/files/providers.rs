//! The built-in preview providers, registered by the files plugin
//! unless it is told not to ([`FilesPlugin::without_builtin_previews`]):
//!
//! - [`Images`] (feature `image-previews`, on by default): PNG and JPEG
//!   decoded with the `image` crate, scaled down, with their dimensions;
//! - [`Markdown`]: a Markdown document drawn simply (headings, items,
//!   quotes, code, rules; emphasis and links as plain text): no Markdown
//!   renderer is in the tree, this is a small one of its own;
//! - [`Text`]: the first lines of a text or code file, in a monospace
//!   font; any file that reads as UTF-8 text, at the lowest priority;
//! - [`Folders`]: what a folder holds and its first images, as the
//!   providers preview them.
//!
//! [`FilesPlugin::without_builtin_previews`]: super::FilesPlugin::without_builtin_previews

use std::path::Path;
use std::sync::Arc;

use super::preview::{Block, Body, Handles, Preview, PreviewProvider, PreviewRequest, Probe};

/// How much of a text is read.
const TEXT_BYTES: usize = 64 * 1024;
/// How many lines of a text are shown.
const TEXT_LINES: usize = 40;
/// How long a line of a text is shown.
const LINE_CHARS: usize = 160;
/// How many blocks of a document are shown.
const BLOCKS: usize = 60;
/// How many images of a folder are shown, from how many files.
const THUMBNAILS: usize = 4;
const THUMBNAIL_TRIES: usize = 16;
/// The side of a folder's thumbnails, in pixels.
const THUMBNAIL_SIDE: u32 = 112;

/// The built-in providers, highest priority first.
pub fn builtin() -> Vec<Arc<dyn PreviewProvider>> {
    vec![
        #[cfg(feature = "image-previews")]
        Arc::new(Images),
        Arc::new(Markdown),
        Arc::new(Folders),
        Arc::new(Text),
    ]
}

/// PNG and JPEG images, decoded with the `image` crate.
#[cfg(feature = "image-previews")]
#[derive(Clone, Copy, Debug, Default)]
pub struct Images;

#[cfg(feature = "image-previews")]
impl PreviewProvider for Images {
    fn id(&self) -> &str {
        "egui_pop.images"
    }

    fn handles(&self) -> Handles {
        Handles::new()
            .extensions(&["png", "jpg", "jpeg", "jpe"])
            .mime(&["image/png", "image/jpeg"])
            .magic(b"\x89PNG\r\n\x1a\n")
            .magic(b"\xff\xd8\xff")
    }

    fn preview(&self, request: &PreviewRequest) -> Result<Option<Preview>, String> {
        let bytes = request.read()?;
        let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(512 * 1024 * 1024);
        reader.limits(limits);
        let image = reader.decode().map_err(|e| e.to_string())?;
        if request.cancelled() {
            return Err("cancelled".into());
        }
        let (w, h) = (image.width(), image.height());
        let side = request.max_side;
        let small = if w.max(h) > side {
            image.thumbnail(side, side)
        } else {
            image
        };
        let rgba = small.to_rgba8();
        let pixels = egui::ColorImage::from_rgba_unmultiplied(
            [rgba.width() as usize, rgba.height() as usize],
            rgba.as_raw(),
        );
        Ok(Some(
            Preview::new(Body::Image(Arc::new(pixels))).dimensions(w, h),
        ))
    }
}

/// Text and code: the first lines.
#[derive(Clone, Copy, Debug, Default)]
pub struct Text;

/// Whether bytes read as text: UTF-8 (a character cut at the end
/// allowed), no NUL.
fn is_text(bytes: &[u8]) -> bool {
    if bytes.contains(&0) {
        return false;
    }
    match std::str::from_utf8(bytes) {
        Ok(_) => true,
        Err(e) => e.error_len().is_none(),
    }
}

/// A text's first bytes as a string (a character cut at the end
/// dropped).
fn text_of(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(e) => String::from_utf8_lossy(&bytes[..e.valid_up_to()]).into_owned(),
    }
}

impl PreviewProvider for Text {
    fn id(&self) -> &str {
        "egui_pop.text"
    }

    fn handles(&self) -> Handles {
        // Any file: `accepts` keeps those that read as text.
        Handles::new().mime(&["*/*"])
    }

    fn priority(&self) -> i32 {
        -10
    }

    fn accepts(&self, probe: &Probe<'_>) -> bool {
        probe.mime.is_some_and(|m| m.starts_with("text/"))
            || probe.meta.len == 0
            || (!probe.head.is_empty() && is_text(probe.head))
    }

    fn preview(&self, request: &PreviewRequest) -> Result<Option<Preview>, String> {
        let bytes = request.read_head(TEXT_BYTES)?;
        if !is_text(&bytes) {
            return Ok(None);
        }
        let all = text_of(&bytes);
        let mut truncated = (bytes.len() as u64) < request.meta.len;
        let mut text = String::new();
        for (n, line) in all.lines().enumerate() {
            if n == TEXT_LINES {
                truncated = true;
                break;
            }
            let line = line.trim_end_matches('\r');
            match line.char_indices().nth(LINE_CHARS) {
                Some((cut, _)) => {
                    text.push_str(&line[..cut]);
                    text.push('…');
                }
                None => text.push_str(line),
            }
            text.push('\n');
        }
        let text = text.trim_end_matches('\n').replace('\t', "    ");
        Ok(Some(Preview::new(Body::Text { text, truncated })))
    }
}

/// Markdown, drawn simply.
#[derive(Clone, Copy, Debug, Default)]
pub struct Markdown;

impl PreviewProvider for Markdown {
    fn id(&self) -> &str {
        "egui_pop.markdown"
    }

    fn handles(&self) -> Handles {
        Handles::new()
            .extensions(&["md", "markdown"])
            .mime(&["text/markdown"])
    }

    fn preview(&self, request: &PreviewRequest) -> Result<Option<Preview>, String> {
        let bytes = request.read_head(TEXT_BYTES)?;
        if !is_text(&bytes) {
            return Ok(None);
        }
        Ok(Some(Preview::new(Body::Markdown(markdown(&text_of(
            &bytes,
        ))))))
    }
}

/// Emphasis, code marks and links as plain text.
fn plain(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find("](").and_then(|mid| {
            let close = after[mid + 2..].find(')')?;
            Some((mid, mid + 2 + close))
        }) {
            Some((mid, close)) => {
                out.push_str(&after[..mid]);
                rest = &after[close + 1..];
            }
            None => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out.replace("**", "").replace("__", "").replace('`', "")
}

/// A Markdown text as blocks: headings, paragraphs, items, quotes,
/// code lines and rules.
pub fn markdown(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let mut code = false;
    let flush = |paragraph: &mut Vec<&str>, out: &mut Vec<Block>| {
        if !paragraph.is_empty() {
            out.push(Block::Paragraph(plain(&paragraph.join(" "))));
            paragraph.clear();
        }
    };
    for line in text.lines() {
        if out.len() >= BLOCKS {
            break;
        }
        let line = line.trim_end();
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            flush(&mut paragraph, &mut out);
            code = !code;
            continue;
        }
        if code {
            out.push(Block::Code(line.replace('\t', "    ")));
            continue;
        }
        if t.is_empty() {
            flush(&mut paragraph, &mut out);
            continue;
        }
        let hashes = t.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
            flush(&mut paragraph, &mut out);
            let title = t[hashes..].trim().trim_end_matches('#').trim();
            out.push(Block::Heading(hashes as u8, plain(title)));
            continue;
        }
        let bare: String = t.chars().filter(|c| !c.is_whitespace()).collect();
        if bare.len() >= 3
            && (bare.chars().all(|c| c == '-')
                || bare.chars().all(|c| c == '*')
                || bare.chars().all(|c| c == '_'))
        {
            flush(&mut paragraph, &mut out);
            out.push(Block::Rule);
            continue;
        }
        let digits = t.chars().take_while(char::is_ascii_digit).count();
        let item = ["- ", "* ", "+ "]
            .iter()
            .find_map(|m| t.strip_prefix(m))
            .or_else(|| {
                (digits > 0)
                    .then(|| t[digits..].strip_prefix(". "))
                    .flatten()
            });
        if let Some(item) = item {
            flush(&mut paragraph, &mut out);
            out.push(Block::Item(plain(item.trim())));
            continue;
        }
        if let Some(q) = t.strip_prefix('>') {
            flush(&mut paragraph, &mut out);
            out.push(Block::Quote(plain(q.trim())));
            continue;
        }
        paragraph.push(t);
    }
    if out.len() < BLOCKS {
        flush(&mut paragraph, &mut out);
    }
    out
}

/// Folders: what they hold, and their first images.
#[derive(Clone, Copy, Debug, Default)]
pub struct Folders;

impl PreviewProvider for Folders {
    fn id(&self) -> &str {
        "egui_pop.folders"
    }

    fn handles(&self) -> Handles {
        Handles::new().folders()
    }

    fn preview(&self, request: &PreviewRequest) -> Result<Option<Preview>, String> {
        let items = request
            .fs()
            .read_dir(&request.path)
            .map_err(|e| e.to_string())?;
        let items: Vec<_> = items
            .into_iter()
            .filter(|e| !e.name.starts_with('.'))
            .collect();
        let folders = items.iter().filter(|e| e.is_dir).count();
        let mut files: Vec<&str> = items
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| e.name.as_str())
            .collect();
        files.sort_by_key(|n| n.to_lowercase());
        let mut thumbnails = Vec::new();
        // Images, and files of no known type (an application's own).
        let candidates = files.iter().filter(|n| {
            super::preview::mime_of(Path::new(n)).is_none_or(|m| m.starts_with("image/"))
        });
        for name in candidates.take(THUMBNAIL_TRIES) {
            if thumbnails.len() == THUMBNAILS || request.cancelled() {
                break;
            }
            if let Some(Preview {
                body: Body::Image(image),
                ..
            }) = request.preview_of(&request.path.join(name), THUMBNAIL_SIDE)
            {
                thumbnails.push(image);
            }
        }
        Ok(Some(Preview::new(Body::Folder {
            folders,
            files: files.len(),
            thumbnails,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::fs::{MemoryFs, SharedFs};
    use crate::files::preview::{Previews, Shown};

    fn previews() -> Previews {
        let p = Previews::default();
        for provider in builtin() {
            p.register(provider);
        }
        p
    }

    fn ready(shown: Shown) -> Arc<crate::files::preview::Ready> {
        match shown {
            Shown::Ready(r) => r,
            other => panic!("not ready: {other:?}"),
        }
    }

    #[test]
    fn markdown_reads_as_blocks() {
        let blocks = markdown(
            "# Plan\n\nSome **bold** text\nand a [link](http://x).\n\n- one\n2. two\n> said\n\n---\n```\nlet a = 1;\n```\n",
        );
        assert_eq!(
            blocks,
            vec![
                Block::Heading(1, "Plan".into()),
                Block::Paragraph("Some bold text and a link.".into()),
                Block::Item("one".into()),
                Block::Item("two".into()),
                Block::Quote("said".into()),
                Block::Rule,
                Block::Code("let a = 1;".into()),
            ]
        );
    }

    #[test]
    fn texts_documents_and_folders_preview() {
        let long: String = (0..100).map(|n| format!("line {n}\n")).collect();
        let fs: SharedFs = Arc::new(
            MemoryFs::new()
                .file("/d/notes.txt", long.into_bytes())
                .file("/d/README", b"plain words".to_vec())
                .file("/d/plan.md", b"# Plan\n\nWords.".to_vec())
                .file("/d/blob.bin", vec![0, 1, 2, 3])
                .dir("/d/sub"),
        );
        let p = previews();
        let r = ready(p.compute(&fs, Path::new("/d/notes.txt")));
        assert_eq!(r.provider, "egui_pop.text");
        let Body::Text { text, truncated } = &r.preview.body else {
            panic!()
        };
        assert_eq!(text.lines().count(), TEXT_LINES);
        assert!(truncated);
        // No extension, but text.
        assert_eq!(
            ready(p.compute(&fs, Path::new("/d/README"))).provider,
            "egui_pop.text"
        );
        let r = ready(p.compute(&fs, Path::new("/d/plan.md")));
        assert_eq!(r.provider, "egui_pop.markdown");
        assert!(matches!(&r.preview.body, Body::Markdown(b) if b.len() == 2));
        // Binary: no provider.
        assert!(matches!(
            p.compute(&fs, Path::new("/d/blob.bin")),
            Shown::Nothing
        ));
        let r = ready(p.compute(&fs, Path::new("/d")));
        assert!(matches!(
            r.preview.body,
            Body::Folder {
                folders: 1,
                files: 4,
                ..
            }
        ));
    }

    #[cfg(feature = "image-previews")]
    #[test]
    fn images_are_scaled_down_with_their_dimensions() {
        let mut png = Vec::new();
        image::RgbImage::from_pixel(600, 300, image::Rgb([200, 40, 40]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let fs: SharedFs = Arc::new(
            MemoryFs::new()
                .file("/d/red.png", png.clone())
                // A PNG by its bytes, whatever its name.
                .file("/d/red.data", png)
                .file("/d/broken.png", b"\x89PNG\r\n\x1a\nnot really".to_vec()),
        );
        let p = previews();
        let r = ready(p.compute(&fs, Path::new("/d/red.png")));
        assert_eq!(r.preview.dimensions, Some([600, 300]));
        let Body::Image(i) = &r.preview.body else {
            panic!()
        };
        assert_eq!(i.size, [256, 128]);
        assert_eq!(
            ready(p.compute(&fs, Path::new("/d/red.data"))).provider,
            "egui_pop.images"
        );
        assert!(matches!(
            p.compute(&fs, Path::new("/d/broken.png")),
            Shown::Failed(_)
        ));
        // A folder's first images.
        let r = ready(p.compute(&fs, Path::new("/d")));
        let Body::Folder { thumbnails, .. } = &r.preview.body else {
            panic!()
        };
        assert_eq!(thumbnails.len(), 2);
        assert_eq!(thumbnails[0].size[0], THUMBNAIL_SIDE as usize);
    }
}
