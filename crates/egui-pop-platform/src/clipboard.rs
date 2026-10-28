//! The system clipboard over arboard (feature `clipboard`): the
//! clipboard's [`SystemBackend`] for files, images, HTML and text, and
//! the primary selection on X11 and Wayland.
//!
//! egui-pop's default backend writes text through egui and reads
//! nothing. [`SystemClipboardPlugin`] installs [`Arboard`] instead
//! (`egui_pop_api::clipboard::set_backend`): a copy writes its richest
//! standard representation, a paste reads every format the clipboard
//! holds as one external item, and the paste key works when the
//! clipboard holds no text (a file manager's files, a browser's image).
//!
//! | | Writes (one per copy, the first it has) | Reads |
//! |---|---|---|
//! | files | `text/uri-list` (`file://` URIs) | yes |
//! | an image | `image/png` | yes |
//! | HTML | `text/html`, with the text as its alternative | yes |
//! | text | `text/plain` | yes |
//!
//! arboard writes one format per copy (HTML takes its text along):
//! files win over an image, an image over HTML, HTML over text. Private
//! types stay in the owner; a kernel wanting them on the system
//! clipboard gives its own backend (`Clipboard::system`).
//!
//! **The platform's path** ([`Arboard::path`], in the `clipboard`
//! collector):
//!
//! - Windows (the Win32 clipboard), macOS (`NSPasteboard`) and X11
//!   (selections, through x11rb) as arboard reaches them.
//! - Wayland: through XWayland by default. A copy then reaches native
//!   Wayland applications only when the compositor hands XWayland's
//!   selection over, which some do only while an X11 window has the
//!   focus. The `clipboard-wayland` feature adds the data-control
//!   protocol (arboard's `wayland-data-control`, through wl-clipboard-rs):
//!   used where the compositor offers it (wlroots compositors, KDE
//!   Plasma), XWayland elsewhere. The core protocol, which needs the
//!   window's input serial, is what egui-winit's own text clipboard
//!   uses (smithay-clipboard); arboard does not.
//! - The web: nothing (the module is not built for `wasm32`).
//!
//! **The primary selection** (select to copy, middle-click to paste) is
//! read and written on X11 and Wayland (`clipboard::primary_text`,
//! `clipboard::set_primary_text`); elsewhere there is none. egui-pop's
//! own widgets do not use it yet (ADR 0015: opt-in, for text inputs and
//! the code editor).
//!
//! The clipboard is opened on first use, not when the plugin connects,
//! so connecting it touches nothing (tests, catalogues).
//!
//! ```
//! use egui_pop_platform::clipboard::{file_uris, paths_of};
//!
//! let list = file_uris(&[std::path::PathBuf::from("/tmp/a b.png")]);
//! assert_eq!(list, "file:///tmp/a%20b.png");
//! assert_eq!(paths_of(&list), [std::path::PathBuf::from("/tmp/a b.png")]);
//! ```

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use egui_pop_api::clipboard::{self, FormatId, HTML, PNG, Repr, SystemBackend, TEXT, URI_LIST};
use egui_pop_api::{Ctx, Plugin, PluginError};
use egui_pop_model::ids::PluginId;
use serde_json::{Value as Json, json};

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.system_clipboard");

/// The formats [`Arboard`] writes and reads, richest first.
const FORMATS: [FormatId; 4] = [URI_LIST, PNG, HTML, TEXT];

/// The system clipboard through arboard, opened on first use.
#[derive(Default)]
pub struct Arboard {
    clipboard: Option<arboard::Clipboard>,
    /// Why it could not be opened, once tried.
    failed: Option<String>,
}

impl std::fmt::Debug for Arboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Arboard")
            .field("open", &self.clipboard.is_some())
            .field("failed", &self.failed)
            .finish()
    }
}

impl Arboard {
    pub fn new() -> Self {
        Self::default()
    }

    /// The clipboard, opened on first use (one attempt).
    fn open(&mut self) -> Result<&mut arboard::Clipboard, String> {
        if self.clipboard.is_none() && self.failed.is_none() {
            match arboard::Clipboard::new() {
                Ok(c) => self.clipboard = Some(c),
                Err(e) => {
                    log::warn!("the system clipboard could not be opened: {e}");
                    self.failed = Some(e.to_string());
                }
            }
        }
        match (&mut self.clipboard, &self.failed) {
            (Some(c), _) => Ok(c),
            (None, Some(e)) => Err(format!("the system clipboard could not be opened: {e}")),
            (None, None) => Err("the system clipboard is not open".into()),
        }
    }

    /// The way it reaches the system clipboard on this platform and
    /// session.
    pub fn path() -> &'static str {
        if cfg!(target_os = "windows") {
            "win32"
        } else if cfg!(target_os = "macos") {
            "nspasteboard"
        } else if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            if cfg!(feature = "clipboard-wayland") {
                "wayland: data-control where the compositor offers it, else xwayland"
            } else {
                "wayland: xwayland"
            }
        } else {
            "x11"
        }
    }
}

impl SystemBackend for Arboard {
    fn name(&self) -> String {
        "arboard".into()
    }

    fn writes(&self, format: &FormatId) -> bool {
        FORMATS.contains(format)
    }

    fn write(&mut self, _ctx: &egui::Context, reprs: &[Repr]) -> Result<(), String> {
        let find = |f: &FormatId| reprs.iter().find(|r| r.format == *f);
        let text = find(&TEXT).and_then(Repr::text).map(str::to_string);
        let cb = self.open()?;
        let files = find(&URI_LIST)
            .and_then(Repr::text)
            .map(paths_of)
            .unwrap_or_default();
        let done = if !files.is_empty() {
            cb.set().file_list(&files)
        } else if let Some(png) = find(&PNG).and_then(Repr::text) {
            let (width, height, rgba) = decode_png(png)?;
            cb.set().image(arboard::ImageData {
                width,
                height,
                bytes: Cow::Owned(rgba),
            })
        } else if let Some(html) = find(&HTML).and_then(Repr::text) {
            cb.set().html(html.to_string(), text)
        } else if let Some(text) = text {
            cb.set().text(text)
        } else {
            return Ok(());
        };
        done.map_err(|e| e.to_string())
    }

    fn read(&mut self, _ctx: &egui::Context) -> Option<Vec<Repr>> {
        let cb = self.open().ok()?;
        let mut out = Vec::new();
        if let Ok(files) = cb.get().file_list()
            && !files.is_empty()
        {
            out.push(Repr::new(URI_LIST, Json::String(file_uris(&files))));
        }
        if let Ok(image) = cb.get().image() {
            match encode_png(&image) {
                Ok(png) => out.push(Repr::new(PNG, Json::String(png))),
                Err(e) => log::warn!("the clipboard's image was not read: {e}"),
            }
        }
        if let Ok(html) = cb.get().html()
            && !html.is_empty()
        {
            out.push(Repr::new(HTML, Json::String(html)));
        }
        if let Ok(text) = cb.get().text()
            && !text.is_empty()
        {
            out.push(Repr::new(TEXT, Json::String(text.replace("\r\n", "\n"))));
        }
        Some(out)
    }

    fn readable(&self) -> Vec<FormatId> {
        vec![URI_LIST, PNG, HTML]
    }

    fn read_primary(&mut self) -> Option<String> {
        #[cfg(all(
            unix,
            not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
        ))]
        {
            use arboard::{GetExtLinux as _, LinuxClipboardKind};
            let cb = self.open().ok()?;
            cb.get().clipboard(LinuxClipboardKind::Primary).text().ok()
        }
        #[cfg(not(all(
            unix,
            not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
        )))]
        {
            None
        }
    }

    fn write_primary(&mut self, text: &str) -> Result<(), String> {
        #[cfg(all(
            unix,
            not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
        ))]
        {
            use arboard::{LinuxClipboardKind, SetExtLinux as _};
            let cb = self.open()?;
            cb.set()
                .clipboard(LinuxClipboardKind::Primary)
                .text(text.to_string())
                .map_err(|e| e.to_string())
        }
        #[cfg(not(all(
            unix,
            not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
        )))]
        {
            let _ = text;
            Err("this platform has no primary selection".into())
        }
    }

    fn describe(&self) -> Json {
        json!({
            "path": Self::path(),
            "writes": "one per copy: files, else an image, else HTML with its text, else text",
            "reads": FORMATS,
            "primary": cfg!(all(
                unix,
                not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
            )),
            "failed": self.failed,
        })
    }
}

/// Files as a URI list: one `file://` URI per line, CRLF between, each
/// byte outside the unreserved set and `/` percent-encoded.
pub fn file_uris(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| file_uri(p))
        .collect::<Vec<_>>()
        .join("\r\n")
}

fn file_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    if !text.starts_with('/') {
        // A Windows path: `file:///C:/dir`.
        out.push('/');
    }
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/:".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The local files of a URI list (`file://` URIs; comments, other
/// schemes and other hosts left out).
pub fn paths_of(list: &str) -> Vec<PathBuf> {
    list.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let rest = l.strip_prefix("file://")?;
            let rest = rest.strip_prefix("localhost").unwrap_or(rest);
            if !rest.starts_with('/') {
                return None;
            }
            let bytes = percent_decode(rest);
            let text = String::from_utf8(bytes).ok()?;
            // `/C:/dir` is a Windows path.
            let b = text.as_bytes();
            let text = if b.len() > 2 && b[2] == b':' && b[1].is_ascii_alphabetic() {
                text[1..].to_string()
            } else {
                text
            };
            Some(PathBuf::from(text))
        })
        .collect()
}

fn percent_decode(text: &str) -> Vec<u8> {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| char::from(c).to_digit(16);
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

/// An image as a PNG file in standard base64.
fn encode_png(image: &arboard::ImageData<'_>) -> Result<String, String> {
    let width = u32::try_from(image.width).map_err(|e| e.to_string())?;
    let height = u32::try_from(image.height).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(&image.bytes)
        .map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// A PNG file in standard base64 as RGBA pixels: width, height, bytes.
fn decode_png(data: &str) -> Result<(usize, usize, Vec<u8>), String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.trim())
        .map_err(|e| format!("an image that is not base64: {e}"))?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8() | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let size = reader
        .output_buffer_size()
        .ok_or("an image too large to read")?;
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::GrayscaleAlpha => buf
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        other => return Err(format!("an image in {other:?}, not RGBA")),
    };
    Ok((info.width as usize, info.height as usize, rgba))
}

/// Installs [`Arboard`] as the clipboard's system backend when it
/// connects, and egui's text again when it disconnects. Needs the
/// clipboard plugin (`egui_pop_api::clipboard::ClipboardPlugin`).
#[derive(Debug, Default)]
pub struct SystemClipboardPlugin;

impl SystemClipboardPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Plugin for SystemClipboardPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "System clipboard".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![clipboard::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        clipboard::set_backend(&ctx.app(), Box::new(Arboard::new()));
        Ok(())
    }

    fn disconnect(&mut self, ctx: &mut Ctx<'_>) {
        clipboard::set_backend(&ctx.app(), Box::new(clipboard::EguiText));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_lists_round_trip() {
        let paths = vec![
            PathBuf::from("/home/me/a b.png"),
            PathBuf::from("/tmp/été#1.txt"),
        ];
        let list = file_uris(&paths);
        assert_eq!(
            list,
            "file:///home/me/a%20b.png\r\nfile:///tmp/%C3%A9t%C3%A9%231.txt"
        );
        assert_eq!(paths_of(&list), paths);
        assert_eq!(
            paths_of("# a comment\nhttps://example.org/x\nfile://localhost/x\nfile://host/y\n"),
            [PathBuf::from("/x")]
        );
        assert_eq!(
            paths_of("file:///C:/dir/f.txt"),
            [PathBuf::from("C:/dir/f.txt")]
        );
    }

    #[test]
    fn images_round_trip_through_png() {
        let image = arboard::ImageData {
            width: 2,
            height: 1,
            bytes: Cow::Owned(vec![255, 0, 0, 255, 0, 0, 255, 128]),
        };
        let png = encode_png(&image).unwrap();
        let (w, h, rgba) = decode_png(&png).unwrap();
        assert_eq!((w, h), (2, 1));
        assert_eq!(rgba, image.bytes.as_ref());
    }
}
