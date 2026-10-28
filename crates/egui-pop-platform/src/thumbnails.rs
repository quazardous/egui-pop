//! The system's thumbnails as a preview provider of the file picker
//! (feature `thumbnails`, off by default; [`ThumbnailsPlugin`], opt-in):
//! for what egui-pop's own providers do not preview (PDF, office
//! documents, videos, fonts…), the thumbnail the desktop made or can
//! make.
//!
//! - **Linux and the BSDs**: the freedesktop thumbnail cache
//!   (`$XDG_CACHE_HOME/thumbnails`, a thumbnail kept only while its
//!   `Thumb::MTime` is the file's), then the installed thumbnailers
//!   (`thumbnailers/*.thumbnailer` in the XDG data folders), run for the
//!   file's MIME type (the shared MIME database's globs) into a
//!   temporary file, stopped after a timeout or when the selection
//!   moves. The cache is read, not written.
//! - **Windows** (the shell's thumbnail cache) and **macOS** (QuickLook)
//!   are not done yet: the provider says so as its reason.
//! - **The web**: nothing; the provider passes.
//!
//! It runs at the lowest priority (-100), after every other provider,
//! and only on the system's file system (a [`MemoryFs`] has no paths
//! the desktop knows).
//!
//! ```
//! use egui_pop_api::files::FilesPlugin;
//! use egui_pop_api::App;
//! use egui_pop_platform::thumbnails::ThumbnailsPlugin;
//!
//! let app = App::builder()
//!     .plugin(FilesPlugin::in_app())
//!     .plugin(ThumbnailsPlugin::default())
//!     .build(&egui::Context::default())
//!     .unwrap();
//! let files = app.handle().service(&egui_pop_api::files::FILES).unwrap();
//! assert!(files.previews().providers().iter().any(|(id, _)| id == "egui_pop.system_thumbnails"));
//! ```
//!
//! [`MemoryFs`]: egui_pop_api::files::fs::MemoryFs

use std::sync::Arc;
use std::time::Duration;

use egui_pop_api::files::preview::{Handles, Preview, PreviewProvider, PreviewRequest};
use egui_pop_api::files::{self, FILES};
use egui_pop_api::{Ctx, Plugin, PluginError};
use egui_pop_model::ids::PluginId;

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.thumbnails");

/// The provider's id.
pub const PROVIDER: &str = "egui_pop.system_thumbnails";

/// The system's thumbnails, as a preview provider.
#[derive(Clone, Debug)]
pub struct SystemThumbnails {
    /// How long a thumbnailer may run.
    pub timeout: Duration,
}

impl Default for SystemThumbnails {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
        }
    }
}

/// Whether this platform has system thumbnails here: why not, if not.
pub fn available() -> Result<(), String> {
    if cfg!(target_arch = "wasm32") {
        Err("the web has no system thumbnails".into())
    } else if cfg!(target_os = "windows") {
        Err("system thumbnails are not available on Windows yet".into())
    } else if cfg!(target_os = "macos") {
        Err("system thumbnails (QuickLook) are not available on macOS yet".into())
    } else {
        Ok(())
    }
}

impl PreviewProvider for SystemThumbnails {
    fn id(&self) -> &str {
        PROVIDER
    }

    fn handles(&self) -> Handles {
        Handles::new().mime(&["*/*"])
    }

    fn priority(&self) -> i32 {
        -100
    }

    fn preview(&self, request: &PreviewRequest) -> Result<Option<Preview>, String> {
        if cfg!(target_arch = "wasm32") || !request.fs().is_native() {
            return Ok(None);
        }
        available()?;
        system(request, self.timeout)
    }
}

#[cfg(all(
    unix,
    not(target_os = "macos"),
    not(target_os = "ios"),
    not(target_os = "android")
))]
fn system(request: &PreviewRequest, timeout: Duration) -> Result<Option<Preview>, String> {
    freedesktop::preview(request, timeout)
}

#[cfg(not(all(
    unix,
    not(target_os = "macos"),
    not(target_os = "ios"),
    not(target_os = "android")
)))]
fn system(_request: &PreviewRequest, _timeout: Duration) -> Result<Option<Preview>, String> {
    Ok(None)
}

/// Registers [`SystemThumbnails`] with the files plugin.
#[derive(Clone, Debug, Default)]
pub struct ThumbnailsPlugin {
    provider: SystemThumbnails,
}

impl ThumbnailsPlugin {
    /// How long a thumbnailer may run (10 s by default).
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.provider.timeout = timeout;
        self
    }
}

impl Plugin for ThumbnailsPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "System thumbnails".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![files::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let files = ctx
            .app()
            .service(&FILES)
            .ok_or_else(|| PluginError::Failed("the files plugin is not connected".into()))?;
        files.previews().register(Arc::new(self.provider.clone()));
        Ok(())
    }
}

/// MD5 (RFC 1321), for the thumbnail cache's file names.
pub fn md5_hex(data: &[u8]) -> String {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    // floor(abs(sin(i + 1)) * 2^32)
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_le_bytes());
    let (mut a0, mut b0, mut c0, mut d0) = (
        0x6745_2301u32,
        0xefcd_ab89u32,
        0x98ba_dcfeu32,
        0x1032_5476u32,
    );
    for chunk in msg.chunks(64) {
        let m: Vec<u32> = chunk
            .chunks(4)
            .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
            .collect();
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(K[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    [a0, b0, c0, d0]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A path as a `file://` URI, escaped as GLib does
/// (`g_filename_to_uri`): what the thumbnail cache is keyed by.
pub fn file_uri(path: &std::path::Path) -> String {
    let mut out = String::from("file://");
    for &b in path.to_string_lossy().as_bytes() {
        let keep = b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'-' | b'_'
                    | b'.'
                    | b'~'
                    | b'/'
                    | b'!'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'('
                    | b')'
                    | b'*'
                    | b'+'
                    | b','
                    | b';'
                    | b'='
                    | b':'
                    | b'@'
            );
        if keep {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A PNG's text chunks (`tEXt`), keyword and text.
pub fn png_texts(png: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return out;
    }
    let mut at = 8;
    while at + 8 <= png.len() {
        let len = u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]) as usize;
        let kind = &png[at + 4..at + 8];
        let Some(data) = png.get(at + 8..at + 8 + len) else {
            break;
        };
        if kind == b"tEXt"
            && let Some(nul) = data.iter().position(|b| *b == 0)
        {
            out.push((
                String::from_utf8_lossy(&data[..nul]).into_owned(),
                String::from_utf8_lossy(&data[nul + 1..]).into_owned(),
            ));
        }
        if kind == b"IDAT" || kind == b"IEND" {
            break;
        }
        at += 12 + len;
    }
    out
}

#[cfg(all(
    unix,
    not(target_os = "macos"),
    not(target_os = "ios"),
    not(target_os = "android")
))]
mod freedesktop {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant, SystemTime};

    use egui_pop_api::files::preview::{self, Body, Preview, PreviewRequest};

    use super::{file_uri, md5_hex, png_texts};

    /// The cache's folders, the size asked first.
    fn sizes(max_side: u32) -> [&'static str; 4] {
        if max_side <= 128 {
            ["normal", "large", "x-large", "xx-large"]
        } else if max_side <= 256 {
            ["large", "x-large", "normal", "xx-large"]
        } else {
            ["x-large", "xx-large", "large", "normal"]
        }
    }

    fn home() -> Option<PathBuf> {
        std::env::var_os("HOME").map(PathBuf::from)
    }

    fn cache_dir() -> Option<PathBuf> {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home().map(|h| h.join(".cache")))
            .map(|c| c.join("thumbnails"))
    }

    /// The XDG data folders, the user's first.
    fn data_dirs() -> Vec<PathBuf> {
        let mut out = Vec::new();
        if let Some(d) = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home().map(|h| h.join(".local/share")))
        {
            out.push(d);
        }
        let dirs = std::env::var("XDG_DATA_DIRS")
            .ok()
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        out.extend(dirs.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
        out
    }

    fn mtime(t: Option<SystemTime>) -> Option<u64> {
        t?.duration_since(SystemTime::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs())
    }

    /// The image of a thumbnail, scaled to the size asked, with the
    /// facts its text chunks give.
    fn decode(bytes: &[u8], request: &PreviewRequest) -> Result<Preview, String> {
        let image = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
        let side = request.max_side;
        let image = if image.width().max(image.height()) > side {
            image.thumbnail(side, side)
        } else {
            image
        };
        let rgba = image.to_rgba8();
        let pixels = egui::ColorImage::from_rgba_unmultiplied(
            [rgba.width() as usize, rgba.height() as usize],
            rgba.as_raw(),
        );
        let mut p = Preview::new(Body::Image(Arc::new(pixels)));
        let texts = png_texts(bytes);
        let get = |k: &str| {
            texts
                .iter()
                .find(|(key, _)| key == k)
                .and_then(|(_, v)| v.parse::<u32>().ok())
        };
        if let (Some(w), Some(h)) = (get("Thumb::Image::Width"), get("Thumb::Image::Height")) {
            p = p.dimensions(w, h);
        }
        if let Some(n) = get("Thumb::Document::Pages") {
            p = p.pages(n);
        }
        Ok(p)
    }

    /// A thumbnail of the cache, if one is there and up to date.
    fn cached(path: &Path, request: &PreviewRequest) -> Option<Preview> {
        let dir = cache_dir()?;
        let name = format!("{}.png", md5_hex(file_uri(path).as_bytes()));
        let want = mtime(request.meta.modified)?;
        for size in sizes(request.max_side) {
            let Ok(bytes) = std::fs::read(dir.join(size).join(&name)) else {
                continue;
            };
            let fresh = png_texts(&bytes)
                .iter()
                .any(|(k, v)| k == "Thumb::MTime" && v.parse::<u64>().ok() == Some(want));
            if fresh && let Ok(p) = decode(&bytes, request) {
                return Some(p);
            }
        }
        None
    }

    /// A file's MIME type from the shared MIME database's globs (`*.ext`
    /// patterns), else egui-pop's own table.
    fn mime_type(path: &Path) -> Option<String> {
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        let mut best: Option<(u32, usize, String)> = None;
        for dir in data_dirs() {
            let Ok(text) = std::fs::read_to_string(dir.join("mime/globs2")) else {
                continue;
            };
            for line in text.lines().filter(|l| !l.starts_with('#')) {
                let mut parts = line.split(':');
                let (Some(weight), Some(mime), Some(glob)) =
                    (parts.next(), parts.next(), parts.next())
                else {
                    continue;
                };
                let Some(ext) = glob.strip_prefix('*') else {
                    continue;
                };
                if ext.contains(['*', '?', '[']) || !name.ends_with(&ext.to_lowercase()) {
                    continue;
                }
                let weight = weight.parse().unwrap_or(50);
                // The heaviest, then the longest pattern.
                let better = best
                    .as_ref()
                    .is_none_or(|(w, l, _)| (weight, ext.len()) > (*w, *l));
                if better {
                    best = Some((weight, ext.len(), mime.to_string()));
                }
            }
            if best.is_some() {
                break;
            }
        }
        best.map(|(_, _, m)| m)
            .or_else(|| preview::mime_of(path).map(str::to_string))
    }

    /// A program's path, from `PATH` when it is a bare name.
    fn which(program: &str) -> Option<PathBuf> {
        let p = Path::new(program);
        if p.is_absolute() {
            return p.is_file().then(|| p.to_path_buf());
        }
        std::env::split_paths(&std::env::var_os("PATH")?)
            .map(|d| d.join(program))
            .find(|c| c.is_file())
    }

    /// A thumbnailer's command for a MIME type: its `Exec` line.
    fn thumbnailer(mime: &str) -> Option<String> {
        for dir in data_dirs() {
            let Ok(read) = std::fs::read_dir(dir.join("thumbnailers")) else {
                continue;
            };
            let mut files: Vec<PathBuf> = read
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "thumbnailer"))
                .collect();
            files.sort();
            for f in files {
                let Ok(text) = std::fs::read_to_string(&f) else {
                    continue;
                };
                let field = |key: &str| {
                    text.lines()
                        .find_map(|l| l.strip_prefix(key)?.trim_start().strip_prefix('='))
                        .map(str::trim)
                };
                let handles = field("MimeType")
                    .is_some_and(|types| types.split(';').any(|t| t.trim() == mime));
                if !handles {
                    continue;
                }
                if let Some(t) = field("TryExec")
                    && which(t).is_none()
                {
                    continue;
                }
                if let Some(exec) = field("Exec") {
                    return Some(exec.to_string());
                }
            }
        }
        None
    }

    /// An `Exec` line as arguments: words split on spaces, quotes kept
    /// together, `%i`, `%u`, `%o`, `%s` and `%%` replaced.
    pub(super) fn arguments(exec: &str, input: &Path, output: &Path, size: u32) -> Vec<String> {
        let mut words = Vec::new();
        let mut word = String::new();
        let mut quote: Option<char> = None;
        let mut started = false;
        let mut chars = exec.chars().peekable();
        while let Some(c) = chars.next() {
            match (c, quote) {
                ('"' | '\'', None) => {
                    quote = Some(c);
                    started = true;
                }
                (c, Some(q)) if c == q => quote = None,
                ('\\', _) => {
                    if let Some(n) = chars.next() {
                        word.push(n);
                    }
                }
                (' ' | '\t', None) => {
                    if started || !word.is_empty() {
                        words.push(std::mem::take(&mut word));
                        started = false;
                    }
                }
                ('%', _) => match chars.next() {
                    Some('i') => word.push_str(&input.to_string_lossy()),
                    Some('u') => word.push_str(&file_uri(input)),
                    Some('o') => word.push_str(&output.to_string_lossy()),
                    Some('s') => word.push_str(&size.to_string()),
                    Some('%') => word.push('%'),
                    _ => {}
                },
                (c, _) => word.push(c),
            }
        }
        if started || !word.is_empty() {
            words.push(word);
        }
        words
    }

    /// Runs a thumbnailer into a temporary file and reads it.
    fn run(
        exec: &str,
        path: &Path,
        request: &PreviewRequest,
        timeout: Duration,
    ) -> Result<Vec<u8>, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let output = std::env::temp_dir().join(format!(
            "egui-pop-thumbnail-{}-{}.png",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let size = if request.max_side <= 128 {
            128
        } else if request.max_side <= 256 {
            256
        } else {
            512
        };
        let args = arguments(exec, path, &output, size);
        let (program, rest) = args.split_first().ok_or("the thumbnailer has no command")?;
        let mut child = Command::new(program)
            .args(rest)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("{program}: {e}"))?;
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break status;
            }
            if request.cancelled() || started.elapsed() > timeout {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&output);
                return Err(if request.cancelled() {
                    "cancelled".into()
                } else {
                    format!("{program} took longer than {} s", timeout.as_secs())
                });
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let bytes = std::fs::read(&output);
        let _ = std::fs::remove_file(&output);
        if !status.success() {
            return Err(format!("{program} failed ({status})"));
        }
        bytes.map_err(|e| format!("{program} made no thumbnail: {e}"))
    }

    pub(super) fn preview(
        request: &PreviewRequest,
        timeout: Duration,
    ) -> Result<Option<Preview>, String> {
        let path = std::fs::canonicalize(&request.path).map_err(|e| e.to_string())?;
        if let Some(p) = cached(&path, request) {
            return Ok(Some(p));
        }
        let Some(mime) = mime_type(&path) else {
            return Ok(None);
        };
        let Some(exec) = thumbnailer(&mime) else {
            return Ok(None);
        };
        let bytes = run(&exec, &path, request, timeout)?;
        decode(&bytes, request).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_matches_the_rfc() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            md5_hex(
                b"12345678901234567890123456789012345678901234567890123456789012345678901234567890"
            ),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
    }

    #[test]
    fn uris_are_escaped_as_glib_does() {
        assert_eq!(
            file_uri(std::path::Path::new("/home/a b/été (1).png")),
            "file:///home/a%20b/%C3%A9t%C3%A9%20(1).png"
        );
    }

    #[test]
    fn png_text_chunks_are_read() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        let data = b"Thumb::MTime\x001700000000";
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        png.extend_from_slice(b"tEXt");
        png.extend_from_slice(data);
        png.extend_from_slice(&[0, 0, 0, 0]);
        assert_eq!(
            png_texts(&png),
            [("Thumb::MTime".to_string(), "1700000000".to_string())]
        );
    }

    #[cfg(all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    ))]
    #[test]
    fn exec_lines_become_arguments() {
        use std::path::Path;
        assert_eq!(
            freedesktop::arguments(
                "thumb -s %s \"%u\" %o 100%%",
                Path::new("/a b.pdf"),
                Path::new("/tmp/o.png"),
                256
            ),
            [
                "thumb",
                "-s",
                "256",
                "file:///a%20b.pdf",
                "/tmp/o.png",
                "100%"
            ]
        );
    }
}
