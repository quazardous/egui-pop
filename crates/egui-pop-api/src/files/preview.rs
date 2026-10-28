//! Previews of files, from pluggable providers.
//!
//! A [`PreviewProvider`] says which files it handles ([`Handles`]: file
//! extensions, MIME types, magic bytes, folders) and with what priority;
//! plugins register theirs with [`Previews::register`] (the service is
//! [`Files::previews`](super::Files::previews)). For a path, the
//! providers that handle it are tried from the highest priority down:
//! the first to give a preview wins, one may pass (`Ok(None)`) to the
//! next, and a failure (an error, or a panic) is shown with its reason.
//!
//! - **Off the interface thread**: [`Previews::get`] answers from the
//!   cache, else hands the path to a worker thread of its own and says
//!   [`Shown::Loading`]; the frame never waits. Asking for another path
//!   cancels the one asked before ([`PreviewRequest::cancelled`]). On
//!   the web, which has no threads, the preview is made in the frame.
//! - **Cached** by path, size and modification time, up to a size in
//!   bytes ([`Previews::set_cache_bytes`], 64 MiB by default; the oldest
//!   go first). Providers read at most [`PreviewRequest::max_bytes`].
//!
//! ```
//! use std::path::Path;
//! use std::sync::Arc;
//! use egui_pop_api::files::fs::{MemoryFs, SharedFs};
//! use egui_pop_api::files::preview::{
//!     Body, Handles, Preview, PreviewProvider, PreviewRequest, Previews, Shown,
//! };
//!
//! #[derive(Debug)]
//! struct Shout;
//! impl PreviewProvider for Shout {
//!     fn id(&self) -> &str {
//!         "shout"
//!     }
//!     fn handles(&self) -> Handles {
//!         Handles::new().extensions(&["txt"])
//!     }
//!     fn preview(&self, request: &PreviewRequest) -> Result<Option<Preview>, String> {
//!         let text = String::from_utf8_lossy(&request.read()?).to_uppercase();
//!         Ok(Some(Preview::new(Body::Text { text, truncated: false })))
//!     }
//! }
//!
//! let previews = Previews::default();
//! previews.register(Arc::new(Shout));
//! let fs: SharedFs = Arc::new(MemoryFs::new().file("/a.txt", b"hi".to_vec()));
//! let Shown::Ready(ready) = previews.compute(&fs, Path::new("/a.txt")) else { panic!() };
//! assert!(matches!(&ready.preview.body, Body::Text { text, .. } if text == "HI"));
//! assert_eq!(ready.provider, "shout");
//! ```

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::SystemTime;

use egui::{ColorImage, RichText, TextureHandle, TextureOptions, Ui};
use egui_pop_core::lock;
use egui_pop_core::strings::tr;
use egui_pop_core::theme::Tokens;
use serde_json::{Value as Json, json};

use super::fs::{Meta, SharedFs};
use crate::strings as txt;

/// The longest side of a preview's image, by default, in pixels.
pub const MAX_SIDE: u32 = 256;
/// The most bytes a provider reads, by default.
pub const MAX_BYTES: usize = 64 * 1024 * 1024;
/// The cache's size, by default, in bytes.
pub const CACHE_BYTES: usize = 64 * 1024 * 1024;
/// How many bytes of a file the magic numbers are read from.
const HEAD: usize = 64;

/// Which files a provider handles: any of its extensions, MIME types or
/// magic numbers, and folders when it says so.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Handles {
    /// Extensions, lower case, without the dot.
    pub extensions: Vec<String>,
    /// MIME types (`image/png`), a whole family (`text/*`), or any file
    /// (`*/*`); a file's type comes from its extension ([`mime_of`]).
    pub mime: Vec<String>,
    /// Bytes a file starts with.
    pub magic: Vec<Vec<u8>>,
    /// Folders.
    pub folders: bool,
}

impl Handles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn extensions(mut self, extensions: &[&str]) -> Self {
        self.extensions
            .extend(extensions.iter().map(|e| e.to_lowercase()));
        self
    }

    pub fn mime(mut self, types: &[&str]) -> Self {
        self.mime.extend(types.iter().map(|t| t.to_string()));
        self
    }

    pub fn magic(mut self, bytes: &[u8]) -> Self {
        self.magic.push(bytes.to_vec());
        self
    }

    pub fn folders(mut self) -> Self {
        self.folders = true;
        self
    }

    /// Whether a path is handled.
    pub fn matches(&self, probe: &Probe<'_>) -> bool {
        if probe.meta.is_dir {
            return self.folders;
        }
        let ext = extension(probe.path);
        ext.as_deref()
            .is_some_and(|e| self.extensions.iter().any(|x| x == e))
            || probe.mime.is_some_and(|m| {
                self.mime.iter().any(|x| {
                    x == "*/*"
                        || x == m
                        || x.strip_suffix("/*")
                            .is_some_and(|family| m.split('/').next() == Some(family))
                })
            })
            || self.magic.iter().any(|m| probe.head.starts_with(m))
    }
}

/// What a provider is asked whether it handles: the path, its facts, its
/// MIME type from the extension, and its first bytes.
#[derive(Clone, Copy, Debug)]
pub struct Probe<'a> {
    pub path: &'a Path,
    pub meta: &'a Meta,
    pub mime: Option<&'static str>,
    /// The first bytes of a file (none for a folder).
    pub head: &'a [u8],
}

/// A path's extension, lower case.
pub fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
}

/// A file's MIME type, from its extension (the common ones).
pub fn mime_of(path: &Path) -> Option<&'static str> {
    Some(match extension(path)?.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" | "jpe" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "heic" | "heif" => "image/heif",
        "txt" | "text" | "log" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "xml" => "application/xml",
        "json" => "application/json",
        "toml" => "application/toml",
        "yaml" | "yml" => "application/yaml",
        "rs" => "text/rust",
        "py" => "text/x-python",
        "js" | "mjs" => "text/javascript",
        "ts" => "text/typescript",
        "c" | "h" => "text/x-c",
        "cpp" | "cc" | "hpp" => "text/x-c++",
        "sh" => "application/x-shellscript",
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "odt" => "application/vnd.oasis.opendocument.text",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "zip" => "application/zip",
        _ => return None,
    })
}

/// A preview: what to draw and the facts to say.
#[derive(Clone, Debug, Default)]
pub struct Preview {
    pub body: Body,
    /// An image's or a page's width and height, in pixels.
    pub dimensions: Option<[u32; 2]>,
    /// A document's pages.
    pub pages: Option<u32>,
    /// Further facts, a label and a value each, in the interface's
    /// language.
    pub facts: Vec<(String, String)>,
}

impl Preview {
    pub fn new(body: Body) -> Self {
        Self {
            body,
            ..Self::default()
        }
    }

    pub fn dimensions(mut self, width: u32, height: u32) -> Self {
        self.dimensions = Some([width, height]);
        self
    }

    pub fn pages(mut self, pages: u32) -> Self {
        self.pages = Some(pages);
        self
    }

    pub fn fact(mut self, label: impl Into<String>, value: impl Into<String>) -> Self {
        self.facts.push((label.into(), value.into()));
        self
    }

    /// About what it holds in memory.
    pub fn bytes(&self) -> usize {
        let image = |i: &ColorImage| i.pixels.len() * 4;
        let facts: usize = self.facts.iter().map(|(a, b)| a.len() + b.len()).sum();
        facts
            + match &self.body {
                Body::Empty => 0,
                Body::Image(i) => image(i),
                Body::Text { text, .. } => text.len(),
                Body::Markdown(blocks) => blocks.iter().map(|b| b.text().len() + 8).sum(),
                Body::Folder { thumbnails, .. } => thumbnails.iter().map(|i| image(i)).sum(),
            }
    }

    /// The metadata, as agents read it (no pixels, no text).
    pub fn describe(&self) -> Json {
        let mut d = json!({ "kind": self.body.kind() });
        if let Some([w, h]) = self.dimensions {
            d["dimensions"] = json!([w, h]);
        }
        if let Some(p) = self.pages {
            d["pages"] = json!(p);
        }
        if let Body::Folder {
            folders,
            files,
            thumbnails,
        } = &self.body
        {
            d["folders"] = json!(folders);
            d["files"] = json!(files);
            d["thumbnails"] = json!(thumbnails.len());
        }
        if let Body::Text { truncated, .. } = &self.body {
            d["truncated"] = json!(truncated);
        }
        if !self.facts.is_empty() {
            d["facts"] = self
                .facts
                .iter()
                .map(|(k, v)| (k.clone(), json!(v)))
                .collect::<serde_json::Map<_, _>>()
                .into();
        }
        d
    }
}

/// What a preview draws.
#[derive(Clone, Debug, Default)]
pub enum Body {
    /// Nothing to draw: only facts.
    #[default]
    Empty,
    /// An image, already scaled down.
    Image(Arc<ColorImage>),
    /// The first lines of a text, in a monospace font.
    Text { text: String, truncated: bool },
    /// A document drawn simply.
    Markdown(Vec<Block>),
    /// A folder: what it holds, and the first images in it.
    Folder {
        folders: usize,
        files: usize,
        thumbnails: Vec<Arc<ColorImage>>,
    },
}

impl Body {
    pub fn kind(&self) -> &'static str {
        match self {
            Body::Empty => "empty",
            Body::Image(_) => "image",
            Body::Text { .. } => "text",
            Body::Markdown(_) => "markdown",
            Body::Folder { .. } => "folder",
        }
    }
}

/// A block of a document drawn simply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    /// A heading, of level 1 to 6.
    Heading(u8, String),
    Paragraph(String),
    /// An item of a list.
    Item(String),
    /// A line of code.
    Code(String),
    Quote(String),
    /// A thematic break.
    Rule,
}

impl Block {
    pub fn text(&self) -> &str {
        match self {
            Block::Heading(_, t)
            | Block::Paragraph(t)
            | Block::Item(t)
            | Block::Code(t)
            | Block::Quote(t) => t,
            Block::Rule => "",
        }
    }
}

/// A provider of previews.
pub trait PreviewProvider: Send + Sync + fmt::Debug {
    /// Its id, for the bridge and the messages (`images`, `my_app.raw`).
    fn id(&self) -> &str;

    /// Which files it handles (read once, when it is registered).
    fn handles(&self) -> Handles;

    /// Higher first. The built-in providers are at 0 (text at -10), the
    /// system's thumbnails at -100.
    fn priority(&self) -> i32 {
        0
    }

    /// Whether it takes a path its [`Handles`] match; all of them, by
    /// default.
    fn accepts(&self, probe: &Probe<'_>) -> bool {
        let _ = probe;
        true
    }

    /// The preview (on a worker thread): `Ok(None)` passes to the next
    /// provider, an error is shown with its reason.
    fn preview(&self, request: &PreviewRequest) -> Result<Option<Preview>, String>;
}

#[derive(Clone, Debug)]
struct Registered {
    provider: Arc<dyn PreviewProvider>,
    handles: Handles,
    priority: i32,
}

/// What a provider is asked for.
#[derive(Clone, Debug)]
pub struct PreviewRequest {
    pub path: PathBuf,
    pub meta: Meta,
    pub mime: Option<&'static str>,
    /// The longest side of an image, in pixels.
    pub max_side: u32,
    /// The most bytes to read.
    pub max_bytes: usize,
    fs: SharedFs,
    cancel: Arc<AtomicBool>,
    chain: Arc<[Registered]>,
}

impl PreviewRequest {
    /// Where to read.
    pub fn fs(&self) -> &SharedFs {
        &self.fs
    }

    /// Whether the selection moved on: a long provider may stop.
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// The file's bytes, refused past [`PreviewRequest::max_bytes`].
    pub fn read(&self) -> Result<Vec<u8>, String> {
        if self.meta.len > self.max_bytes as u64 {
            return Err(format!(
                "too large to preview ({}, at most {})",
                size_text(self.meta.len),
                size_text(self.max_bytes as u64)
            ));
        }
        self.read_head(self.max_bytes)
    }

    /// At most `limit` bytes from the start of the file.
    pub fn read_head(&self, limit: usize) -> Result<Vec<u8>, String> {
        self.fs
            .read(&self.path, limit.min(self.max_bytes))
            .map_err(|e| e.to_string())
    }

    /// The file's extension, lower case.
    pub fn extension(&self) -> Option<String> {
        extension(&self.path)
    }

    /// Another file's preview from the same providers, at most
    /// `max_side` pixels (a folder's thumbnails); folders are skipped.
    pub fn preview_of(&self, path: &Path, max_side: u32) -> Option<Preview> {
        let meta = self.fs.metadata(path).ok().filter(|m| !m.is_dir)?;
        let request = PreviewRequest {
            path: path.to_path_buf(),
            meta,
            mime: mime_of(path),
            max_side,
            max_bytes: self.max_bytes,
            fs: self.fs.clone(),
            cancel: self.cancel.clone(),
            chain: self.chain.clone(),
        };
        match run(&request) {
            Shown::Ready(r) => Some(r.preview.clone()),
            _ => None,
        }
    }
}

/// A preview made, with the facts of its file and its textures once
/// drawn.
pub struct Ready {
    pub preview: Preview,
    pub meta: Meta,
    /// The provider that made it.
    pub provider: String,
    textures: Mutex<HashMap<usize, TextureHandle>>,
}

impl fmt::Debug for Ready {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ready")
            .field("preview", &self.preview)
            .field("meta", &self.meta)
            .field("provider", &self.provider)
            .field("textures", &lock(&self.textures).len())
            .finish()
    }
}

impl Ready {
    pub fn new(preview: Preview, meta: Meta, provider: &str) -> Self {
        Self {
            preview,
            meta,
            provider: provider.into(),
            textures: Mutex::default(),
        }
    }

    /// The texture of its image (`0`), or of a folder's `n`th
    /// thumbnail, made on the first call.
    pub fn texture(&self, ctx: &egui::Context, n: usize) -> Option<TextureHandle> {
        let image = match &self.preview.body {
            Body::Image(i) if n == 0 => i.clone(),
            Body::Folder { thumbnails, .. } => thumbnails.get(n)?.clone(),
            _ => return None,
        };
        let mut t = lock(&self.textures);
        Some(
            t.entry(n)
                .or_insert_with(|| {
                    ctx.load_texture(
                        format!("egui_pop.files.preview.{n}"),
                        (*image).clone(),
                        TextureOptions::LINEAR,
                    )
                })
                .clone(),
        )
    }
}

/// Where a path's preview is.
#[derive(Clone, Debug)]
pub enum Shown {
    /// No provider handles it.
    Nothing,
    /// A worker is making it.
    Loading,
    Ready(Arc<Ready>),
    /// It could not be made: why.
    Failed(String),
}

impl Shown {
    pub fn state(&self) -> &'static str {
        match self {
            Shown::Nothing => "none",
            Shown::Loading => "loading",
            Shown::Ready(_) => "ready",
            Shown::Failed(_) => "failed",
        }
    }

    /// As agents read it: the state, the provider and the preview's
    /// metadata.
    pub fn describe(&self) -> Json {
        let mut d = json!({ "state": self.state() });
        match self {
            Shown::Ready(r) => {
                d["provider"] = json!(r.provider);
                if let Json::Object(m) = r.preview.describe() {
                    for (k, v) in m {
                        d[k] = v;
                    }
                }
            }
            Shown::Failed(e) => d["reason"] = json!(e),
            _ => {}
        }
        d
    }

    fn bytes(&self) -> usize {
        match self {
            Shown::Ready(r) => r.preview.bytes() + 256,
            Shown::Failed(e) => e.len() + 64,
            _ => 64,
        }
    }
}

/// A preview's cache key: the path, its size and modification time, the
/// size asked.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
    max_side: u32,
}

#[derive(Debug)]
struct Job {
    key: Key,
    request: PreviewRequest,
}

#[derive(Debug)]
struct State {
    providers: Vec<Registered>,
    cache: HashMap<Key, Shown>,
    /// Oldest first.
    order: VecDeque<Key>,
    bytes: usize,
    cap: usize,
    max_side: u32,
    max_bytes: usize,
    /// Waiting for the worker (the last asked: those before are dropped).
    job: Option<Job>,
    /// What the worker makes, and its cancel flag.
    running: Option<(Key, Arc<AtomicBool>)>,
    worker: bool,
    repaint: Option<egui::Context>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            providers: Vec::new(),
            cache: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            cap: CACHE_BYTES,
            max_side: MAX_SIDE,
            max_bytes: MAX_BYTES,
            job: None,
            running: None,
            worker: false,
            repaint: None,
        }
    }
}

impl State {
    fn put(&mut self, key: Key, shown: Shown) {
        if let Some(old) = self.cache.remove(&key) {
            self.bytes = self.bytes.saturating_sub(old.bytes());
            self.order.retain(|k| *k != key);
        }
        let size = shown.bytes();
        if size > self.cap {
            return;
        }
        while self.bytes + size > self.cap {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(old) = self.cache.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(old.bytes());
            }
        }
        self.bytes += size;
        self.order.push_back(key.clone());
        self.cache.insert(key, shown);
    }
}

#[derive(Debug, Default)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

/// The preview providers, their cache and their worker, as a service of
/// [`Files`](super::Files).
#[derive(Clone, Debug, Default)]
pub struct Previews(Arc<Shared>);

impl Previews {
    /// Adds a provider.
    pub fn register(&self, provider: Arc<dyn PreviewProvider>) {
        let r = Registered {
            handles: provider.handles(),
            priority: provider.priority(),
            provider,
        };
        let mut s = lock(&self.0.state);
        s.providers.retain(|p| p.provider.id() != r.provider.id());
        // Stable: among equals, the first registered first.
        let at = s
            .providers
            .iter()
            .position(|p| p.priority < r.priority)
            .unwrap_or(s.providers.len());
        s.providers.insert(at, r);
        // What was made before may now come from another provider.
        s.cache.clear();
        s.order.clear();
        s.bytes = 0;
    }

    /// The providers, highest priority first: id and priority.
    pub fn providers(&self) -> Vec<(String, i32)> {
        lock(&self.0.state)
            .providers
            .iter()
            .map(|p| (p.provider.id().to_string(), p.priority))
            .collect()
    }

    /// The cache's size in bytes; the oldest previews go first past it.
    pub fn set_cache_bytes(&self, bytes: usize) {
        let mut s = lock(&self.0.state);
        s.cap = bytes;
        while s.bytes > s.cap {
            let Some(oldest) = s.order.pop_front() else {
                break;
            };
            if let Some(old) = s.cache.remove(&oldest) {
                s.bytes = s.bytes.saturating_sub(old.bytes());
            }
        }
    }

    /// The longest side of a preview's image, in pixels.
    pub fn set_max_side(&self, pixels: u32) {
        lock(&self.0.state).max_side = pixels.max(16);
    }

    /// The most bytes a provider reads.
    pub fn set_max_bytes(&self, bytes: usize) {
        lock(&self.0.state).max_bytes = bytes;
    }

    fn job(&self, fs: &SharedFs, path: &Path) -> Result<(Key, PreviewRequest), Shown> {
        let meta = fs
            .metadata(path)
            .map_err(|e| Shown::Failed(e.to_string()))?;
        let s = lock(&self.0.state);
        let key = Key {
            path: path.to_path_buf(),
            len: meta.len,
            modified: meta.modified,
            max_side: s.max_side,
        };
        let request = PreviewRequest {
            path: path.to_path_buf(),
            meta,
            mime: mime_of(path),
            max_side: s.max_side,
            max_bytes: s.max_bytes,
            fs: fs.clone(),
            cancel: Arc::default(),
            chain: s.providers.clone().into(),
        };
        Ok((key, request))
    }

    /// A path's preview: from the cache, else asked of the worker
    /// ([`Shown::Loading`]; the path asked before, unless it is done, is
    /// cancelled). The context is woken when it is ready.
    pub fn get(&self, ctx: &egui::Context, fs: &SharedFs, path: &Path) -> Shown {
        let (key, request) = match self.job(fs, path) {
            Ok(j) => j,
            Err(shown) => return shown,
        };
        let mut s = lock(&self.0.state);
        if let Some(shown) = s.cache.get(&key) {
            return shown.clone();
        }
        if s.running.as_ref().is_some_and(|(k, _)| *k == key)
            || s.job.as_ref().is_some_and(|j| j.key == key)
        {
            return Shown::Loading;
        }
        if s.providers.is_empty() {
            return Shown::Nothing;
        }
        // Another path was asked before: it is not wanted any more.
        if let Some((_, cancel)) = &s.running {
            cancel.store(true, Ordering::Relaxed);
        }
        s.repaint = Some(ctx.clone());
        if cfg!(target_arch = "wasm32") {
            drop(s);
            let shown = run(&request);
            lock(&self.0.state).put(key, shown.clone());
            return shown;
        }
        s.job = Some(Job { key, request });
        if !s.worker {
            s.worker = true;
            drop(s);
            self.spawn();
        } else {
            drop(s);
            self.0.wake.notify_all();
        }
        Shown::Loading
    }

    /// What is known of a path's preview without asking for it.
    pub fn peek(&self, fs: &SharedFs, path: &Path) -> Shown {
        let (key, _) = match self.job(fs, path) {
            Ok(j) => j,
            Err(shown) => return shown,
        };
        let s = lock(&self.0.state);
        if let Some(shown) = s.cache.get(&key) {
            return shown.clone();
        }
        if s.running.as_ref().is_some_and(|(k, _)| *k == key)
            || s.job.as_ref().is_some_and(|j| j.key == key)
        {
            return Shown::Loading;
        }
        if s.providers.is_empty() {
            Shown::Nothing
        } else {
            Shown::Loading
        }
    }

    /// A path's preview made now, on this thread (agents and tests), or
    /// from the cache; kept in the cache.
    pub fn compute(&self, fs: &SharedFs, path: &Path) -> Shown {
        let (key, request) = match self.job(fs, path) {
            Ok(j) => j,
            Err(shown) => return shown,
        };
        if let Some(shown) = lock(&self.0.state).cache.get(&key) {
            return shown.clone();
        }
        let shown = run(&request);
        lock(&self.0.state).put(key, shown.clone());
        shown
    }

    /// Nothing is wanted any more (the picker closed): the preview in
    /// the making is cancelled.
    pub fn cancel(&self) {
        let mut s = lock(&self.0.state);
        s.job = None;
        if let Some((_, cancel)) = &s.running {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// The providers and the cache, as the bridge shows them.
    pub fn describe(&self) -> Json {
        let s = lock(&self.0.state);
        json!({
            "providers": s.providers.iter().map(|p| json!({
                "id": p.provider.id(),
                "priority": p.priority,
                "extensions": p.handles.extensions,
                "mime": p.handles.mime,
                "folders": p.handles.folders,
            })).collect::<Vec<_>>(),
            "cached": s.cache.len(),
            "cache_bytes": s.bytes,
            "cache_cap": s.cap,
            "running": s.running.as_ref().map(|(k, _)| k.path.display().to_string()),
        })
    }

    fn spawn(&self) {
        let weak = Arc::downgrade(&self.0);
        let spawned = std::thread::Builder::new()
            .name("egui-pop-previews".into())
            .spawn(move || worker(&weak));
        if let Err(e) = spawned {
            log::warn!(target: "egui_pop", "files: no thread for previews: {e}");
            // Made in the frame, then.
            let job = {
                let mut s = lock(&self.0.state);
                s.worker = false;
                s.job.take()
            };
            if let Some(job) = job {
                let shown = run(&job.request);
                lock(&self.0.state).put(job.key, shown);
            }
        }
    }
}

/// The worker: makes the last preview asked, until the service is gone.
fn worker(weak: &std::sync::Weak<Shared>) {
    loop {
        let Some(shared) = weak.upgrade() else {
            return;
        };
        let job = {
            let mut s = lock(&shared.state);
            if s.job.is_none() {
                s = match shared
                    .wake
                    .wait_timeout(s, std::time::Duration::from_millis(500))
                {
                    Ok((s, _)) => s,
                    Err(e) => e.into_inner().0,
                };
            }
            let job = s.job.take();
            if let Some(j) = &job {
                s.running = Some((j.key.clone(), j.request.cancel.clone()));
            }
            job
        };
        // Not held while waiting for the next one: the service may go.
        let Some(job) = job else {
            continue;
        };
        let shown = run(&job.request);
        let repaint = {
            let mut s = lock(&shared.state);
            s.running = None;
            if !job.request.cancelled() || matches!(shown, Shown::Ready(_)) {
                s.put(job.key, shown);
            }
            s.repaint.clone()
        };
        if let Some(ctx) = repaint {
            ctx.request_repaint();
        }
    }
}

/// The providers tried in turn for a request.
fn run(request: &PreviewRequest) -> Shown {
    let head = if request.meta.is_dir {
        Vec::new()
    } else {
        request.fs.read(&request.path, HEAD).unwrap_or_default()
    };
    let probe = Probe {
        path: &request.path,
        meta: &request.meta,
        mime: request.mime,
        head: &head,
    };
    for r in request.chain.iter() {
        if request.cancelled() {
            return Shown::Failed("cancelled".into());
        }
        if !r.handles.matches(&probe) || !r.provider.accepts(&probe) {
            continue;
        }
        match catch_unwind(AssertUnwindSafe(|| r.provider.preview(request))) {
            Ok(Ok(Some(p))) => {
                return Shown::Ready(Arc::new(Ready::new(p, request.meta, r.provider.id())));
            }
            Ok(Ok(None)) => {}
            Ok(Err(e)) => return Shown::Failed(e),
            Err(_) => {
                return Shown::Failed(format!("the `{}` previewer failed", r.provider.id()));
            }
        }
    }
    Shown::Nothing
}

/// The width of the picker's preview pane.
pub const PANE_WIDTH: f32 = 240.0;
/// The height of a preview's drawing in the pane.
const PANE_HEIGHT: f32 = 180.0;

/// Draws a preview pane: the entry's name, its preview and its facts
/// (a file's size and modification time, a folder's items, dimensions,
/// pages, the provider's).
pub fn pane(ui: &mut Ui, name: &str, meta: Option<&Meta>, shown: &Shown) {
    let t = Tokens::of(ui.ctx());
    ui.add(egui::Label::new(RichText::new(name).strong()).truncate());
    ui.add_space(t.space.xs);
    let ready = match shown {
        Shown::Loading => {
            ui.weak(tr(ui.ctx(), &txt::FILES_PREVIEW_LOADING));
            None
        }
        Shown::Nothing => {
            ui.weak(tr(ui.ctx(), &txt::FILES_PREVIEW_NONE));
            None
        }
        Shown::Failed(reason) => {
            ui.colored_label(t.status.danger, tr(ui.ctx(), &txt::FILES_PREVIEW_FAILED));
            ui.add(egui::Label::new(RichText::new(reason).weak()).wrap());
            None
        }
        Shown::Ready(r) => {
            body(ui, r, name);
            Some(r)
        }
    };
    ui.add_space(t.space.s);
    let meta = ready.map(|r| &r.meta).or(meta);
    let mut rows: Vec<(String, String)> = Vec::new();
    if let Some(m) = meta.filter(|m| !m.is_dir) {
        rows.push((tr(ui.ctx(), &txt::FILES_SIZE), size_text(m.len)));
    }
    if let Some(p) = ready.map(|r| &r.preview) {
        if let Body::Folder { folders, files, .. } = &p.body {
            rows.push((
                tr(ui.ctx(), &txt::FILES_ITEMS),
                (folders + files).to_string(),
            ));
        }
        if let Some([w, h]) = p.dimensions {
            rows.push((tr(ui.ctx(), &txt::FILES_DIMENSIONS), format!("{w} × {h}")));
        }
        if let Some(n) = p.pages {
            rows.push((tr(ui.ctx(), &txt::FILES_PAGES), n.to_string()));
        }
    }
    // A file's: a folder's changes with what is put in it.
    if let Some(at) = meta.filter(|m| !m.is_dir).and_then(|m| m.modified) {
        rows.push((tr(ui.ctx(), &txt::FILES_MODIFIED), time_text(at)));
    }
    if let Some(p) = ready.map(|r| &r.preview) {
        rows.extend(p.facts.iter().cloned());
    }
    egui::Grid::new("egui_pop.files.preview.facts")
        .num_columns(2)
        .spacing([t.space.m, t.space.xs])
        .show(ui, |ui| {
            for (label, value) in rows {
                ui.weak(label);
                ui.label(value);
                ui.end_row();
            }
        });
}

/// A size that fits `image` in `area`, keeping its proportions, never
/// larger than it is.
fn fit(image: [usize; 2], area: egui::Vec2) -> egui::Vec2 {
    let (w, h) = (image[0].max(1) as f32, image[1].max(1) as f32);
    let k = (area.x / w).min(area.y / h).min(1.0);
    egui::vec2(w * k, h * k)
}

fn body(ui: &mut Ui, ready: &Ready, name: &str) {
    let t = Tokens::of(ui.ctx());
    let area = egui::vec2(ui.available_width(), PANE_HEIGHT);
    match &ready.preview.body {
        Body::Empty => {}
        Body::Image(image) => {
            if let Some(tex) = ready.texture(ui.ctx(), 0) {
                let r = ui.image((tex.id(), fit(image.size, area)));
                egui_pop_core::a11y::name(&r, egui::WidgetType::Image, name);
            }
        }
        Body::Text { text, truncated } => {
            egui::ScrollArea::vertical()
                .id_salt("egui_pop.files.preview.text")
                .max_height(area.y)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(text).monospace().small()).wrap());
                    if *truncated {
                        ui.weak("…");
                    }
                });
        }
        Body::Markdown(blocks) => {
            egui::ScrollArea::vertical()
                .id_salt("egui_pop.files.preview.document")
                .max_height(area.y)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for b in blocks {
                        match b {
                            Block::Heading(1, s) => {
                                ui.label(RichText::new(s).heading());
                            }
                            Block::Heading(_, s) => {
                                ui.label(RichText::new(s).strong());
                            }
                            Block::Paragraph(s) => {
                                ui.add(egui::Label::new(s.as_str()).wrap());
                            }
                            Block::Item(s) => {
                                ui.add(egui::Label::new(format!("• {s}")).wrap());
                            }
                            Block::Code(s) => {
                                ui.label(RichText::new(s).monospace().small());
                            }
                            Block::Quote(s) => {
                                ui.add(egui::Label::new(RichText::new(s).italics().weak()).wrap());
                            }
                            Block::Rule => {
                                ui.separator();
                            }
                        }
                    }
                });
        }
        Body::Folder { thumbnails, .. } => {
            let side = ((area.x - t.space.xs) / 2.0).floor();
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(t.space.xs, t.space.xs);
                for (n, image) in thumbnails.iter().enumerate() {
                    if let Some(tex) = ready.texture(ui.ctx(), n) {
                        let size = fit(image.size, egui::vec2(side, side));
                        let (rect, r) =
                            ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
                        let at = egui::Rect::from_center_size(rect.center(), size);
                        ui.painter().image(
                            tex.id(),
                            at,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                        egui_pop_core::a11y::name(&r, egui::WidgetType::Image, name);
                    }
                }
            });
        }
    }
}

/// A size in bytes, as people read it (powers of 1000).
pub fn size_text(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64 / 1000.0;
    let mut unit = 0;
    while v >= 1000.0 && unit < UNITS.len() - 1 {
        v /= 1000.0;
        unit += 1;
    }
    if v < 10.0 {
        format!("{v:.1} {}", UNITS[unit])
    } else {
        format!("{v:.0} {}", UNITS[unit])
    }
}

/// A time as `2026-01-15 10:04 UTC`.
pub fn time_text(t: SystemTime) -> String {
    let secs = match t.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    };
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rest / 3600,
        rest % 3600 / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::fs::MemoryFs;
    use std::time::{Duration, Instant};

    /// A provider that says what it was asked, or fails as told.
    #[derive(Debug)]
    struct Fake {
        id: &'static str,
        handles: Handles,
        priority: i32,
        answer: fn(&PreviewRequest) -> Result<Option<Preview>, String>,
    }

    impl PreviewProvider for Fake {
        fn id(&self) -> &str {
            self.id
        }
        fn handles(&self) -> Handles {
            self.handles.clone()
        }
        fn priority(&self) -> i32 {
            self.priority
        }
        fn preview(&self, request: &PreviewRequest) -> Result<Option<Preview>, String> {
            (self.answer)(request)
        }
    }

    fn fake(
        id: &'static str,
        handles: Handles,
        priority: i32,
        answer: fn(&PreviewRequest) -> Result<Option<Preview>, String>,
    ) -> Arc<dyn PreviewProvider> {
        Arc::new(Fake {
            id,
            handles,
            priority,
            answer,
        })
    }

    fn named(r: &PreviewRequest) -> Result<Option<Preview>, String> {
        let name = r.path.file_name().unwrap().to_string_lossy().into_owned();
        Ok(Some(Preview::new(Body::Text {
            text: name,
            truncated: false,
        })))
    }

    fn fs() -> SharedFs {
        Arc::new(
            MemoryFs::new()
                .file("/d/a.png", b"\x89PNG\r\n\x1a\nrest".to_vec())
                .file("/d/b.txt", b"words".to_vec())
                .file("/d/c.bin", b"\x00\x01".to_vec())
                .file("/d/d.dat", b"MAGIC and more".to_vec())
                .dir("/d/sub"),
        )
    }

    fn provider_of(shown: &Shown) -> &str {
        match shown {
            Shown::Ready(r) => &r.provider,
            other => panic!("not ready: {other:?}"),
        }
    }

    #[test]
    fn providers_are_chosen_by_extension_type_magic_and_priority() {
        let p = Previews::default();
        let fs = fs();
        p.register(fake("low", Handles::new().mime(&["*/*"]), -100, named));
        p.register(fake("text", Handles::new().mime(&["text/*"]), 0, named));
        p.register(fake("magic", Handles::new().magic(b"MAGIC"), 0, named));
        p.register(fake(
            "png",
            Handles::new().extensions(&["PNG"]),
            5,
            |_| Ok(None), // passes to the next
        ));
        p.register(fake("folders", Handles::new().folders(), 0, named));
        assert_eq!(p.providers()[0], ("png".into(), 5));
        assert_eq!(provider_of(&p.compute(&fs, Path::new("/d/b.txt"))), "text");
        assert_eq!(provider_of(&p.compute(&fs, Path::new("/d/d.dat"))), "magic");
        assert_eq!(provider_of(&p.compute(&fs, Path::new("/d/a.png"))), "low");
        assert_eq!(provider_of(&p.compute(&fs, Path::new("/d/sub"))), "folders");
        let other = Previews::default();
        other.register(fake("png", Handles::new().extensions(&["png"]), 0, named));
        assert!(matches!(
            other.compute(&fs, Path::new("/d/c.bin")),
            Shown::Nothing
        ));
        assert!(matches!(
            other.compute(&fs, Path::new("/d/missing")),
            Shown::Failed(_)
        ));
    }

    #[test]
    fn a_failure_or_a_panic_is_a_reason_not_a_crash() {
        let p = Previews::default();
        let fs = fs();
        p.register(fake("bad", Handles::new().extensions(&["txt"]), 0, |_| {
            Err("broken header".into())
        }));
        p.register(fake(
            "panics",
            Handles::new().extensions(&["png"]),
            0,
            |_| panic!("boom"),
        ));
        let Shown::Failed(e) = p.compute(&fs, Path::new("/d/b.txt")) else {
            panic!()
        };
        assert_eq!(e, "broken header");
        let Shown::Failed(e) = p.compute(&fs, Path::new("/d/a.png")) else {
            panic!()
        };
        assert!(e.contains("panics"), "{e}");
    }

    #[test]
    fn previews_come_from_a_worker_and_are_cached_until_the_file_changes() {
        let p = Previews::default();
        p.register(fake("text", Handles::new().mime(&["text/*"]), 0, named));
        let ctx = egui::Context::default();
        let fs = fs();
        let path = Path::new("/d/b.txt");
        assert!(matches!(p.get(&ctx, &fs, path), Shown::Loading));
        let deadline = Instant::now() + Duration::from_secs(10);
        let ready = loop {
            match p.get(&ctx, &fs, path) {
                Shown::Ready(r) => break r,
                Shown::Loading => {
                    assert!(Instant::now() < deadline, "the worker did not answer");
                    std::thread::sleep(Duration::from_millis(5));
                }
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(ready.provider, "text");
        // From the cache now.
        let Shown::Ready(again) = p.get(&ctx, &fs, path) else {
            panic!()
        };
        assert!(Arc::ptr_eq(&ready, &again));
        assert_eq!(p.describe()["cached"], 1);
        // The same path, changed: made again.
        let changed: SharedFs = Arc::new(MemoryFs::new().file(path, b"other words".to_vec()));
        assert!(matches!(p.peek(&changed, path), Shown::Loading));
        // The cache's cap drops the oldest.
        p.set_cache_bytes(0);
        assert_eq!(p.describe()["cached"], 0);
    }

    #[test]
    fn asking_for_another_path_cancels_the_one_before() {
        let p = Previews::default();
        p.register(fake("slow", Handles::new().mime(&["*/*"]), 0, |r| {
            // Waits until cancelled (or a while).
            let until = Instant::now() + Duration::from_secs(10);
            while !r.cancelled() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(Some(
                Preview::new(Body::Empty).fact("cancelled", r.cancelled().to_string()),
            ))
        }));
        let ctx = egui::Context::default();
        let fs = fs();
        assert!(matches!(
            p.get(&ctx, &fs, Path::new("/d/b.txt")),
            Shown::Loading
        ));
        // Wait until the worker holds it.
        let deadline = Instant::now() + Duration::from_secs(10);
        while p.describe()["running"].is_null() {
            assert!(Instant::now() < deadline, "the worker did not start");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(matches!(
            p.get(&ctx, &fs, Path::new("/d/c.bin")),
            Shown::Loading
        ));
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Shown::Ready(r) = p.peek(&fs, Path::new("/d/b.txt")) {
                assert_eq!(r.preview.facts, [("cancelled".into(), "true".into())]);
                break;
            }
            assert!(Instant::now() < deadline, "the cancel was not seen");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn sizes_and_times_read_as_people_read_them() {
        assert_eq!(size_text(999), "999 B");
        assert_eq!(size_text(1500), "1.5 KB");
        assert_eq!(size_text(25_000_000), "25 MB");
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_768_471_440);
        assert_eq!(time_text(t), "2026-01-15 10:04 UTC");
    }
}
