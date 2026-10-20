//! Choosing files, as a plugin ([`FilesPlugin`], in `DefaultPlugins`):
//! the system's chooser off the interface thread, or an in-app picker
//! when the system's is unavailable or switched off.
//!
//! [`Files::choose`] returns a [`Pending`] (the host contract's, ADR
//! 0010): the frame never waits. In-app pickers open in a modal of their
//! own, or inside the caller's dialog ([`Files::choose_inline`], as a
//! form's path field does). The result is `{ "paths": [...] }`, or
//! `null` when the person cancelled; [`chosen`] reads it.
//!
//! - **The system's chooser** (feature `native-dialogs`, through `rfd`):
//!   the xdg desktop portal on Linux, the common dialogs on Windows, the
//!   open and save panels on macOS. It is created on the interface
//!   thread and awaited on a thread of its own.
//! - **The in-app picker** ([`FilePicker`]), when the feature is off, no
//!   portal answers (no session bus), or the `files.native` setting is
//!   off: keyboard navigation (arrows, Enter, Backspace or `Alt+Up` for
//!   the parent folder, `Ctrl+Enter` to choose the folder or the
//!   selection, `Ctrl+Space` to select, `Ctrl+D` to bookmark), a typed
//!   path with completion (`Tab`), the fuzzy filter of the choice dialog,
//!   the request's filters, recent places and bookmarks kept in the host
//!   store (`settings/files.recent`, `settings/files.bookmarks`).
//!
//! Steps (ADR 0011):
//!
//! | Step | Arguments | Default |
//! |---|---|---|
//! | `file.choose` | `{ "id", "request" }` | opens the system's chooser or the in-app picker |
//! | `file.chosen` | `{ "id", "request", "paths" }` | remembers the places, gives the paths |
//!
//! A run-hook on `file.choose` replaces the whole chooser: it answers
//! `{ "paths": [...] }` or `{ "cancelled": true }` at once, or anything
//! else and later [`Files::deliver`]. A before-hook on `file.chosen` may
//! veto the choice (the result is the reason, as an error) or redirect
//! it to other paths. The journal says `file.cancelled` when the person
//! cancels. The debug bridge's `files` collector shows the choices open
//! and answers them (`answer`), as a person would.
//!
//! ```
//! use egui_pop_api::files::{self, FileRequest, FilesPlugin, FILES};
//! use egui_pop_api::App;
//!
//! let app = App::builder().plugin(FilesPlugin::in_app()).build(&egui::Context::default()).unwrap();
//! let h = app.handle();
//! let f = h.service(&FILES).unwrap();
//! let pending = f.choose(&h, FileRequest::open("Open an image").filter("Images", &["png", "jpg"]));
//! // Nothing yet: the picker is open, the frame goes on.
//! assert!(pending.take().is_none());
//! assert_eq!(f.describe()["open"][0]["via"], "in_app");
//! ```

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use egui::{Key, Modifiers, Ui};
use egui_pop_core::debug::FnCollector;
use egui_pop_core::lock;
use egui_pop_core::overlay::{self, Kind, OverlaySpec};
use egui_pop_core::strings::{self, CANCEL};
use egui_pop_core::theme::Tokens;
use egui_pop_core::ui::UiPop as _;
use egui_pop_core::widgets;
use egui_pop_model::commands::Origin;
use egui_pop_model::fuzzy;
use egui_pop_model::ids::{BlockId, OwnerId, PluginId, SettingId, StepId};
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use crate::app::AppHandle;
use crate::host::{self, Pending, Resolver};
use crate::services::ServiceToken;
use crate::steps::{Outcome, StepSpec};
use crate::strings as txt;
use crate::zones::{self, Block};
use crate::{Ctx, Plugin, PluginError};

/// The file chooser, as a service.
pub const FILES: ServiceToken<Files> = ServiceToken::new("app.files");

/// A choice starts: the system's chooser or the in-app picker opens.
pub const STEP_CHOOSE: StepId = StepId::from_static("file.choose");
/// Paths were chosen.
pub const STEP_CHOSEN: StepId = StepId::from_static("file.chosen");

/// The setting: use the system's chooser when there is one.
pub const SETTING_NATIVE: SettingId = SettingId::from_static("files.native");

/// Where recent places are kept (a JSON list of folders, newest first).
pub const RECENT: &str = "settings/files.recent";
/// Where bookmarks are kept (a JSON list of folders).
pub const BOOKMARKS: &str = "settings/files.bookmarks";

/// How many recent places are kept.
pub const RECENT_KEPT: usize = 8;

/// What is chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChooseKind {
    /// One existing file.
    Open,
    /// Existing files.
    OpenMany,
    /// A file to write (it may not exist yet).
    Save,
    /// A folder.
    Folder,
}

/// Files a filter shows: a name and extensions (without the dot).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

impl FileFilter {
    pub fn new(name: &str, extensions: &[&str]) -> Self {
        Self {
            name: name.into(),
            extensions: extensions.iter().map(|e| e.to_lowercase()).collect(),
        }
    }

    /// Whether a file's name has one of the extensions (any, without
    /// extensions).
    pub fn accepts(&self, path: &Path) -> bool {
        self.extensions.is_empty()
            || path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| self.extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
    }
}

/// What a choice asks for.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileRequest {
    pub kind: ChooseKind,
    /// The chooser's title.
    pub title: String,
    /// Where it starts (the last recent place, else home, by default).
    #[serde(default)]
    pub directory: Option<PathBuf>,
    /// The name proposed when saving.
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub filters: Vec<FileFilter>,
}

impl FileRequest {
    fn new(kind: ChooseKind, title: &str) -> Self {
        Self {
            kind,
            title: title.into(),
            directory: None,
            file_name: None,
            filters: Vec::new(),
        }
    }

    /// One existing file.
    pub fn open(title: &str) -> Self {
        Self::new(ChooseKind::Open, title)
    }

    /// Existing files.
    pub fn open_many(title: &str) -> Self {
        Self::new(ChooseKind::OpenMany, title)
    }

    /// A file to write, `name` proposed.
    pub fn save(title: &str, name: &str) -> Self {
        Self::new(ChooseKind::Save, title).file_name(name)
    }

    /// A folder.
    pub fn folder(title: &str) -> Self {
        Self::new(ChooseKind::Folder, title)
    }

    pub fn directory(mut self, dir: impl Into<PathBuf>) -> Self {
        self.directory = Some(dir.into());
        self
    }

    pub fn file_name(mut self, name: &str) -> Self {
        self.file_name = Some(name.into());
        self
    }

    /// Adds a filter (the first is the one shown first).
    pub fn filter(mut self, name: &str, extensions: &[&str]) -> Self {
        self.filters.push(FileFilter::new(name, extensions));
        self
    }

    pub fn to_json(&self) -> Json {
        serde_json::to_value(self).unwrap_or(Json::Null)
    }

    pub fn from_json(json: &Json) -> Result<FileRequest, String> {
        serde_json::from_value(json.clone()).map_err(|e| format!("not a file request: {e}"))
    }
}

/// How a choice ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chosen {
    Paths(Vec<PathBuf>),
    Cancelled,
}

/// Reads a choice's result (what its [`Pending`] gives).
pub fn chosen(result: &Result<Json, String>) -> Result<Chosen, String> {
    let json = result.as_ref().map_err(Clone::clone)?;
    if json.is_null() || json.get("cancelled").and_then(Json::as_bool) == Some(true) {
        return Ok(Chosen::Cancelled);
    }
    Ok(Chosen::Paths(paths_of(json)))
}

fn paths_of(json: &Json) -> Vec<PathBuf> {
    json.get("paths")
        .and_then(Json::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Json::as_str)
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

fn paths_json(paths: &[PathBuf]) -> Json {
    json!(
        paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
    )
}

/// Who answers a choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Via {
    Native,
    InApp,
    /// A plugin's run-hook on `file.choose`.
    Hook,
}

/// What a choice ends with: paths, or `None` (cancelled).
type Answer = Option<Vec<PathBuf>>;

#[derive(Debug)]
struct Open {
    id: u64,
    request: FileRequest,
    via: Via,
    resolver: Option<Resolver>,
    answer: Arc<Mutex<Option<Answer>>>,
    picker: Option<FilePicker>,
    /// Its in-app picker is drawn by the caller ([`Files::show_inline`]),
    /// not in a modal of its own.
    inline: bool,
}

#[derive(Debug, Default)]
struct State {
    next: u64,
    open: Vec<Open>,
    /// The last choices: id, request title, outcome.
    last: VecDeque<Json>,
    /// The system's chooser is used when available (the setting).
    native: bool,
}

/// The choices open, as a service.
#[derive(Clone, Debug, Default)]
pub struct Files(Arc<Mutex<State>>);

/// Whether the system's chooser can be used here: the feature is on and,
/// on Linux and the BSDs, a session bus is there for the portal.
pub fn native_available() -> bool {
    #[cfg(all(feature = "native-dialogs", not(target_arch = "wasm32")))]
    {
        if cfg!(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        )) {
            return std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some()
                || std::env::var_os("XDG_RUNTIME_DIR")
                    .is_some_and(|d| Path::new(&d).join("bus").exists());
        }
        true
    }
    #[cfg(not(all(feature = "native-dialogs", not(target_arch = "wasm32"))))]
    false
}

impl Files {
    /// Starts a choice; its result comes later through the [`Pending`].
    pub fn choose(&self, app: &AppHandle, request: FileRequest) -> Pending {
        self.start_choice(app, request, false).1
    }

    /// Starts a choice whose in-app picker the caller draws itself, in a
    /// dialog of its own ([`Files::show_inline`]); the system's chooser,
    /// or a plugin's, is used the same way. Gives the choice's id.
    pub fn choose_inline(&self, app: &AppHandle, request: FileRequest) -> (u64, Pending) {
        self.start_choice(app, request, true)
    }

    fn start_choice(&self, app: &AppHandle, request: FileRequest, inline: bool) -> (u64, Pending) {
        let (pending, resolver) = host::pending();
        let id = {
            let mut s = lock(&self.0);
            s.next += 1;
            let id = s.next;
            s.open.push(Open {
                id,
                request: request.clone(),
                via: Via::Hook,
                resolver: Some(resolver),
                answer: Arc::default(),
                picker: None,
                inline,
            });
            id
        };
        let me = self.clone();
        let args = json!({ "id": id, "request": request.to_json() });
        let out = app.step(&STEP_CHOOSE, args, move |app, args| {
            let request = FileRequest::from_json(&args["request"])?;
            me.start(app, id, request)
        });
        let immediate = match out {
            Outcome::Vetoed { reason, .. } => Some(Err(reason)),
            Outcome::Done(Err(e)) => Some(Err(e)),
            Outcome::Done(Ok(v)) => {
                if v.get("paths").is_some() {
                    let _ = self.deliver(id, Some(paths_of(&v)));
                } else if v.get("cancelled").and_then(Json::as_bool) == Some(true) {
                    let _ = self.deliver(id, None);
                }
                None
            }
        };
        if let Some(r) = immediate {
            let mut s = lock(&self.0);
            if let Some(at) = s.open.iter().position(|o| o.id == id)
                && let Some(resolver) = s.open.remove(at).resolver
            {
                resolver.resolve(r);
            }
        }
        app.egui().request_repaint();
        (id, pending)
    }

    /// Draws the in-app picker of a choice started with
    /// [`Files::choose_inline`], if it has one; says whether it did.
    pub fn show_inline(&self, ui: &mut Ui, app: &AppHandle, id: u64) -> bool {
        let Some((mut picker, answer)) = ({
            let s = lock(&self.0);
            s.open
                .iter()
                .find(|o| o.id == id)
                .and_then(|o| o.picker.clone().map(|p| (p, o.answer.clone())))
        }) else {
            return false;
        };
        let out = picker.show(ui, &places(app));
        settle(app, &answer, out, true);
        self.keep(id, picker);
        true
    }

    fn keep(&self, id: u64, picker: FilePicker) {
        let mut s = lock(&self.0);
        if let Some(o) = s.open.iter_mut().find(|o| o.id == id) {
            o.picker = Some(picker);
        }
    }

    /// The default of `file.choose`: the system's chooser when it is
    /// available and wanted, else the in-app picker.
    fn start(&self, app: &AppHandle, id: u64, request: FileRequest) -> Result<Json, String> {
        let native = lock(&self.0).native && native_available();
        let recent = recent(app);
        let mut s = lock(&self.0);
        let o = s
            .open
            .iter_mut()
            .find(|o| o.id == id)
            .ok_or("the choice is gone")?;
        o.request = request.clone();
        if native {
            o.via = Via::Native;
            let answer = o.answer.clone();
            drop(s);
            native::start(&app.egui(), &request, recent.first().cloned(), answer);
            return Ok(json!({ "via": "native" }));
        }
        o.via = Via::InApp;
        let mut picker = FilePicker::new(&request);
        if request.directory.is_none()
            && let Some(dir) = recent.first().filter(|d| d.is_dir())
        {
            picker.go(dir.clone());
        }
        o.picker = Some(picker);
        Ok(json!({ "via": "in_app" }))
    }

    /// Answers a choice (applied on the next frame): paths, or `None`
    /// for cancelled. For a run-hook replacing the chooser, and agents.
    pub fn deliver(&self, id: u64, answer: Option<Vec<PathBuf>>) -> Result<(), String> {
        let s = lock(&self.0);
        let o = s
            .open
            .iter()
            .find(|o| o.id == id)
            .ok_or_else(|| format!("no choice {id} is open"))?;
        *lock(&o.answer) = Some(answer);
        Ok(())
    }

    /// The choices open, by id.
    pub fn open_ids(&self) -> Vec<u64> {
        lock(&self.0).open.iter().map(|o| o.id).collect()
    }

    /// The choices open and the last ones, as the bridge shows them.
    pub fn describe(&self) -> Json {
        let s = lock(&self.0);
        json!({
            "native": s.native,
            "native_available": native_available(),
            "open": s.open.iter().map(|o| {
                let mut d = json!({ "id": o.id, "request": o.request.to_json(), "via": o.via });
                if let Some(p) = &o.picker {
                    d["picker"] = p.clone().describe();
                }
                d
            }).collect::<Vec<_>>(),
            "last": s.last.iter().cloned().collect::<Vec<_>>(),
        })
    }

    /// The in-app picker of an open choice (tests and agents drive it).
    pub fn picker(&self, id: u64) -> Option<FilePicker> {
        lock(&self.0)
            .open
            .iter()
            .find(|o| o.id == id)
            .and_then(|o| o.picker.clone())
    }

    fn note(&self, id: u64, title: &str, outcome: Json) {
        let mut s = lock(&self.0);
        if s.last.len() == 16 {
            s.last.pop_front();
        }
        s.last
            .push_back(json!({ "id": id, "title": title, "outcome": outcome }));
    }
}

/// A list of paths kept in the store as JSON text.
fn read_list(app: &AppHandle, path: &str) -> Vec<PathBuf> {
    lock(&app.store())
        .get(path)
        .and_then(|e| e.value.as_text().map(str::to_string))
        .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

fn write_list(app: &AppHandle, path: &str, list: &[PathBuf]) {
    let text = serde_json::to_string(
        &list
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>(),
    )
    .unwrap_or_default();
    let set = Set::new(path, Change::To(Value::Text(text)), Origin::User)
        .by(&OwnerId::FRAMEWORK)
        .untracked();
    if let Err(e) = lock(&app.store()).set(set) {
        log::warn!(target: "egui_pop", "files: keeping {path}: {e:?}");
    }
}

/// The recent places, newest first.
pub fn recent(app: &AppHandle) -> Vec<PathBuf> {
    read_list(app, RECENT)
}

/// The bookmarked folders.
pub fn bookmarks(app: &AppHandle) -> Vec<PathBuf> {
    read_list(app, BOOKMARKS)
}

/// Bookmarks a folder, or removes its bookmark.
pub fn set_bookmark(app: &AppHandle, dir: &Path, on: bool) {
    let mut list = bookmarks(app);
    list.retain(|p| p != dir);
    if on {
        list.push(dir.to_path_buf());
    }
    write_list(app, BOOKMARKS, &list);
}

/// Remembers the folders of chosen paths as recent places.
pub fn remember(app: &AppHandle, paths: &[PathBuf]) {
    let mut list = recent(app);
    for p in paths.iter().rev() {
        let dir = if p.is_dir() {
            p.clone()
        } else {
            match p.parent() {
                Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
                _ => continue,
            }
        };
        list.retain(|x| *x != dir);
        list.insert(0, dir);
    }
    list.truncate(RECENT_KEPT);
    write_list(app, RECENT, &list);
}

/// The plugin.
#[derive(Clone, Debug)]
pub struct FilesPlugin {
    native: bool,
}

impl Default for FilesPlugin {
    /// The system's chooser when there is one.
    fn default() -> Self {
        Self { native: true }
    }
}

impl FilesPlugin {
    /// The in-app picker by default (the setting may still switch).
    pub fn in_app() -> Self {
        Self { native: false }
    }
}

impl Plugin for FilesPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("egui_pop.files")
    }

    fn title(&self) -> String {
        "Files".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let files = Files::default();
        ctx.setting(
            SETTING_NATIVE,
            Descriptor::bool("Use the system's file chooser", self.native).description(
                "Choose files with the system's chooser when there is one; off, with egui-pop's own.",
            ),
        )?;
        {
            let store = ctx.app().store();
            let mut store = lock(&store);
            for (path, label) in [(RECENT, "Recent places"), (BOOKMARKS, "Bookmarks")] {
                store
                    .declare(
                        ctx.owner(),
                        path,
                        Descriptor::text(label, "[]").capacity(16_384),
                    )
                    .map_err(|e| PluginError::Failed(format!("{path}: {e:?}")))?;
            }
        }
        ctx.service(&FILES, files.clone())?;
        ctx.step(
            StepSpec::new(
                STEP_CHOOSE,
                "A choice of files starts; a run-hook replaces the chooser (answer `paths` or `cancelled`, or later through `Files::deliver`).",
            )
            .args(json!({ "id": 1, "request": FileRequest::open("Open").to_json() })),
        )?;
        ctx.step(
            StepSpec::new(
                STEP_CHOSEN,
                "Paths were chosen; a veto refuses them, a redirect changes them.",
            )
            .args(json!({ "id": 1, "request": FileRequest::open("Open").to_json(), "paths": ["/tmp/a.png"] })),
        )?;
        let (a, b) = (files.clone(), files.clone());
        ctx.collector(
            FnCollector::new(
                "files",
                "The file choices open (request, chooser, the in-app picker's folder and entries) and the last ones; `answer` answers one as an agent.",
            )
            .snapshot(move |_| a.describe())
            .action(
                "answer",
                "Answers a choice: `id`, and `paths` (a list) or `cancel: true`.",
                true,
                move |_, args| {
                    let id = args.get("id").and_then(Json::as_u64).ok_or("`id` is missing")?;
                    let answer = if args.get("cancel").and_then(Json::as_bool) == Some(true) {
                        None
                    } else {
                        Some(paths_of(args))
                    };
                    b.deliver(id, answer)?;
                    Ok(json!({ "queued": true }))
                },
            )
            .args(json!({ "id": 1, "paths": ["/tmp/a.png"] })),
        )?;
        ctx.block(
            zones::OVERLAYS,
            Block::add(BlockId::from_static("egui_pop.files"), 12, frame),
        );
        Ok(())
    }
}

/// Polls the choices, finishes the answered ones, draws the in-app
/// picker of the first one open.
fn frame(ui: &mut Ui, app: &AppHandle) {
    let Some(files) = app.service(&FILES) else {
        return;
    };
    let native = lock(&app.store())
        .get(&format!("settings/{SETTING_NATIVE}"))
        .and_then(|e| e.value.as_bool())
        .unwrap_or(false);
    lock(&files.0).native = native;
    let done: Vec<(u64, FileRequest, Answer, Option<Resolver>)> = {
        let mut s = lock(&files.0);
        let mut done = Vec::new();
        let mut i = 0;
        while i < s.open.len() {
            let answer = lock(&s.open[i].answer).take();
            match answer {
                Some(a) => {
                    let o = s.open.remove(i);
                    done.push((o.id, o.request, a, o.resolver));
                }
                None => i += 1,
            }
        }
        done
    };
    let ctx = ui.ctx().clone();
    for (id, request, answer, resolver) in done {
        overlay::close(&ctx, overlay_id(id));
        finish(app, &files, id, &request, answer, resolver);
    }
    picker_overlay(&ctx, app, &files);
}

fn overlay_id(id: u64) -> egui::Id {
    egui::Id::new(("egui_pop.files", id))
}

/// Ends a choice: `file.chosen` for paths, the journal for a cancel.
fn finish(
    app: &AppHandle,
    files: &Files,
    id: u64,
    request: &FileRequest,
    answer: Answer,
    resolver: Option<Resolver>,
) {
    let result = match answer {
        None => {
            app.publish("user", None, "file.cancelled", json!({ "id": id }));
            files.note(id, &request.title, json!("cancelled"));
            Ok(Json::Null)
        }
        Some(paths) => {
            let args =
                json!({ "id": id, "request": request.to_json(), "paths": paths_json(&paths) });
            let out = app.step(&STEP_CHOSEN, args, |app, args| {
                let paths = paths_of(args);
                remember(app, &paths);
                Ok(json!({ "paths": paths_json(&paths) }))
            });
            let r = match out {
                Outcome::Done(r) => r,
                Outcome::Vetoed { reason, .. } => Err(reason),
            };
            files.note(
                id,
                &request.title,
                match &r {
                    Ok(v) => v.clone(),
                    Err(e) => json!({ "refused": e }),
                },
            );
            r
        }
    };
    if let Some(r) = resolver {
        r.resolve(result);
    }
}

/// What the picker answered, applied: the choice's answer, or a
/// bookmark.
fn settle(app: &AppHandle, answer: &Mutex<Option<Answer>>, out: Option<PickerAnswer>, open: bool) {
    match out {
        Some(PickerAnswer::Chosen(paths)) => *lock(answer) = Some(Some(paths)),
        Some(PickerAnswer::Cancel) => *lock(answer) = Some(None),
        Some(PickerAnswer::Bookmark(dir, on)) => set_bookmark(app, &dir, on),
        None if !open => *lock(answer) = Some(None),
        None => {}
    }
}

fn places(app: &AppHandle) -> Places {
    Places {
        home: home(),
        recent: recent(app),
        bookmarks: bookmarks(app),
    }
}

/// The in-app picker of the first choice that has one (and is not drawn
/// by its caller), in a modal.
fn picker_overlay(ctx: &egui::Context, app: &AppHandle, files: &Files) {
    let Some((id, title, mut picker, answer)) = ({
        let s = lock(&files.0);
        s.open.iter().filter(|o| !o.inline).find_map(|o| {
            o.picker
                .clone()
                .map(|p| (o.id, o.request.title.clone(), p, o.answer.clone()))
        })
    }) else {
        return;
    };
    let places = places(app);
    let spec = OverlaySpec::new(("egui_pop.files", id), &title, Kind::Modal).width(640.0);
    debug_assert_eq!(spec.id, overlay_id(id));
    let mut open = true;
    let mut out = None;
    overlay::show(ctx, &spec, &mut open, None, |ui| {
        ui.heading(&title);
        out = picker.show(ui, &places);
    });
    settle(app, &answer, out, open);
    files.keep(id, picker);
}

/// The home folder, if known.
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|d| d.is_dir())
}

/// Places the picker lists beside the folder.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Places {
    pub home: Option<PathBuf>,
    pub recent: Vec<PathBuf>,
    pub bookmarks: Vec<PathBuf>,
}

/// One entry of a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
}

/// What the picker answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickerAnswer {
    Chosen(Vec<PathBuf>),
    Cancel,
    /// Bookmark this folder (`true`) or remove its bookmark.
    Bookmark(PathBuf, bool),
}

/// The in-app picker: a folder's entries, a field that filters them
/// (fuzzy) or takes a typed path (with completion), places, filters.
/// Usable on its own: [`FilePicker::show`] in any `Ui`.
#[derive(Clone, Debug)]
pub struct FilePicker {
    pub kind: ChooseKind,
    /// The folder shown.
    pub dir: PathBuf,
    pub filters: Vec<FileFilter>,
    /// The filter in use (`filters.len()`: all files).
    pub filter_index: usize,
    /// The field: a filter, or a path when it holds a separator or starts
    /// with `~`.
    pub text: String,
    /// The entry highlighted among those shown.
    pub highlight: usize,
    /// Files selected (choosing several).
    pub selected: BTreeSet<PathBuf>,
    pub show_hidden: bool,
    /// Why the last Enter did nothing.
    pub error: Option<String>,
    focused: bool,
    listing: Option<(PathBuf, bool, Vec<Entry>)>,
}

impl FilePicker {
    /// A picker for a request, in its folder (else home, else the root).
    pub fn new(request: &FileRequest) -> Self {
        let dir = request
            .directory
            .clone()
            .filter(|d| d.is_dir())
            .or_else(home)
            .unwrap_or_else(|| PathBuf::from("/"));
        Self {
            kind: request.kind,
            dir,
            filters: request.filters.clone(),
            filter_index: 0,
            text: if request.kind == ChooseKind::Save {
                request.file_name.clone().unwrap_or_default()
            } else {
                String::new()
            },
            highlight: 0,
            selected: BTreeSet::new(),
            show_hidden: false,
            error: None,
            focused: false,
            listing: None,
        }
    }

    /// Shows another folder.
    pub fn go(&mut self, dir: PathBuf) {
        self.dir = dir;
        self.highlight = 0;
        self.error = None;
        if self.kind != ChooseKind::Save {
            self.text.clear();
        }
    }

    /// Shows the parent folder.
    pub fn up(&mut self) {
        if let Some(p) = self.dir.parent() {
            self.go(p.to_path_buf());
        }
    }

    fn filter(&self) -> Option<&FileFilter> {
        self.filters.get(self.filter_index)
    }

    fn accepts(&self, path: &Path) -> bool {
        self.filter().is_none_or(|f| f.accepts(path))
    }

    /// The folder's entries: folders, then the files the filter accepts
    /// (none when choosing a folder), by name; hidden ones only when
    /// asked.
    pub fn entries(&mut self) -> Vec<Entry> {
        let key = (self.dir.clone(), self.show_hidden);
        if let Some((d, h, e)) = &self.listing
            && (d, *h) == (&key.0, key.1)
        {
            return e
                .iter()
                .filter(|x| x.is_dir || self.accepts(&x.path))
                .cloned()
                .collect();
        }
        let mut all: Vec<Entry> = std::fs::read_dir(&self.dir)
            .map(|r| {
                r.filter_map(Result::ok)
                    .map(|e| {
                        let path = e.path();
                        Entry {
                            name: e.file_name().to_string_lossy().into_owned(),
                            is_dir: path.is_dir(),
                            path,
                        }
                    })
                    .filter(|e| self.show_hidden || !e.name.starts_with('.'))
                    .filter(|e| e.is_dir || self.kind != ChooseKind::Folder)
                    .collect()
            })
            .unwrap_or_default();
        all.sort_by(|a, b| {
            (!a.is_dir, a.name.to_lowercase()).cmp(&(!b.is_dir, b.name.to_lowercase()))
        });
        self.listing = Some((key.0, key.1, all));
        self.entries()
    }

    /// Whether the field holds a path rather than a filter.
    pub fn is_path(&self) -> bool {
        self.text.contains('/')
            || self.text.contains(std::path::MAIN_SEPARATOR)
            || self.text.starts_with('~')
    }

    /// The field as a path: `~` is home, a relative path is in the
    /// folder.
    pub fn typed_path(&self) -> PathBuf {
        let t = self.text.trim();
        let expanded = match t.strip_prefix('~') {
            Some(rest) => {
                let rest = rest.trim_start_matches(['/', std::path::MAIN_SEPARATOR]);
                home().unwrap_or_default().join(rest)
            }
            None => PathBuf::from(t),
        };
        if expanded.is_absolute() {
            expanded
        } else {
            self.dir.join(expanded)
        }
    }

    /// The entries shown: all of them, or those the filter matches (best
    /// first); with a typed path, those of its folder its last part
    /// starts.
    pub fn visible(&mut self) -> Vec<Entry> {
        let entries = self.entries();
        if self.is_path() || self.text.trim().is_empty() || self.kind == ChooseKind::Save {
            return entries;
        }
        fuzzy::rank(&entries, self.text.trim(), |e| e.name.clone())
            .into_iter()
            .map(|(i, _)| entries[i].clone())
            .collect()
    }

    /// What a typed path may complete to: the names in its folder its
    /// last part starts (folders end with a separator).
    pub fn completions(&self) -> Vec<String> {
        if !self.is_path() {
            return Vec::new();
        }
        let text = self.text.as_str();
        let cut = text
            .rfind(['/', std::path::MAIN_SEPARATOR])
            .map_or(0, |i| i + 1);
        let (head, part) = text.split_at(cut);
        let head_path = if head.is_empty() {
            self.dir.clone()
        } else {
            let mut p = self.clone();
            p.text = head.to_string();
            p.typed_path()
        };
        let Ok(read) = std::fs::read_dir(&head_path) else {
            return Vec::new();
        };
        let lower = part.to_lowercase();
        let mut out: Vec<String> = read
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let is_dir = e.path().is_dir();
                let shown = (self.show_hidden || !name.starts_with('.') || part.starts_with('.'))
                    && name.to_lowercase().starts_with(&lower)
                    && (is_dir || (self.kind != ChooseKind::Folder && self.accepts(&e.path())));
                shown.then(|| format!("{head}{name}{}", if is_dir { "/" } else { "" }))
            })
            .collect();
        out.sort();
        out
    }

    /// Completes the typed path (Tab): to the only completion, else to
    /// what every completion shares. Says whether the text changed.
    pub fn complete(&mut self) -> bool {
        let all = self.completions();
        let Some(first) = all.first() else {
            return false;
        };
        let mut common = first.clone();
        for c in &all[1..] {
            let n = common
                .chars()
                .zip(c.chars())
                .take_while(|(a, b)| a.to_lowercase().eq(b.to_lowercase()))
                .count();
            common = common.chars().take(n).collect();
        }
        if common.chars().count() > self.text.chars().count() {
            self.text = common;
            true
        } else {
            false
        }
    }

    /// Enter: into a folder, or the choice.
    pub fn activate(&mut self) -> Option<PickerAnswer> {
        self.error = None;
        if self.is_path() {
            let p = self.typed_path();
            if p.is_dir() {
                self.go(p);
                return None;
            }
            return match self.kind {
                ChooseKind::Save if p.parent().is_some_and(Path::is_dir) => {
                    Some(PickerAnswer::Chosen(vec![p]))
                }
                ChooseKind::Open | ChooseKind::OpenMany if p.is_file() => {
                    Some(PickerAnswer::Chosen(vec![p]))
                }
                _ => {
                    self.error = Some(format!("{} does not exist", p.display()));
                    None
                }
            };
        }
        if self.kind == ChooseKind::Save {
            let name = self.text.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let p = self.dir.join(&name);
            if p.is_dir() {
                self.go(p);
                return None;
            }
            return Some(PickerAnswer::Chosen(vec![p]));
        }
        let shown = self.visible();
        let e = shown.get(self.highlight)?.clone();
        if e.is_dir {
            self.go(e.path);
            return None;
        }
        if self.kind == ChooseKind::OpenMany && !self.selected.is_empty() {
            return Some(PickerAnswer::Chosen(
                self.selected.iter().cloned().collect(),
            ));
        }
        Some(PickerAnswer::Chosen(vec![e.path]))
    }

    /// Ctrl+Enter, and the primary button: the folder (choosing a
    /// folder), the selection, or what Enter would choose.
    pub fn accept(&mut self) -> Option<PickerAnswer> {
        match self.kind {
            ChooseKind::Folder => {
                let dir = if self.is_path() && self.typed_path().is_dir() {
                    self.typed_path()
                } else {
                    let shown = self.visible();
                    match shown.get(self.highlight) {
                        Some(e) if !self.text.trim().is_empty() => e.path.clone(),
                        _ => self.dir.clone(),
                    }
                };
                Some(PickerAnswer::Chosen(vec![dir]))
            }
            ChooseKind::OpenMany if !self.selected.is_empty() => Some(PickerAnswer::Chosen(
                self.selected.iter().cloned().collect(),
            )),
            _ => self.activate(),
        }
    }

    /// Selects or unselects the highlighted file (choosing several).
    pub fn toggle(&mut self) {
        if self.kind != ChooseKind::OpenMany {
            return;
        }
        let shown = self.visible();
        if let Some(e) = shown.get(self.highlight).filter(|e| !e.is_dir)
            && !self.selected.remove(&e.path)
        {
            self.selected.insert(e.path.clone());
        }
    }

    /// The picker as the bridge shows it.
    pub fn describe(&mut self) -> Json {
        let shown = self.visible();
        json!({
            "dir": self.dir.display().to_string(),
            "text": self.text,
            "highlight": self.highlight,
            "shown": shown.iter().take(64).map(|e| if e.is_dir { format!("{}/", e.name) } else { e.name.clone() }).collect::<Vec<_>>(),
            "selected": self.selected.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "filter": self.filter().map(|f| f.name.clone()),
            "completions": self.completions(),
            "error": self.error,
        })
    }

    /// Draws the picker; its answer when given.
    pub fn show(&mut self, ui: &mut Ui, places: &Places) -> Option<PickerAnswer> {
        let t = Tokens::of(ui.ctx());
        let first = !self.focused;
        self.focused = true;
        let mut answer = None;
        // Keys taken before the field sees them.
        let empty = self.text.is_empty();
        let (down, up, enter, accept, tab, parent, toggle, bookmark) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::COMMAND, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Tab),
                i.consume_key(Modifiers::ALT, Key::ArrowUp)
                    || (empty && i.consume_key(Modifiers::NONE, Key::Backspace)),
                i.consume_key(Modifiers::COMMAND, Key::Space),
                i.consume_key(Modifiers::COMMAND, Key::D),
            )
        });
        let bookmarked = places.bookmarks.contains(&self.dir);
        ui.horizontal(|ui| {
            if ui.button(strings::tr(ui.ctx(), &txt::PICKER_UP)).clicked() {
                self.up();
            }
            let label = if bookmarked {
                &txt::FILES_UNBOOKMARK
            } else {
                &txt::FILES_BOOKMARK
            };
            if ui.button(strings::tr(ui.ctx(), label)).clicked() {
                answer = Some(PickerAnswer::Bookmark(self.dir.clone(), !bookmarked));
            }
            ui.monospace(self.dir.display().to_string());
        });
        if bookmark {
            answer = Some(PickerAnswer::Bookmark(self.dir.clone(), !bookmarked));
        }
        if parent {
            self.up();
        }
        let hint = if self.kind == ChooseKind::Save {
            &txt::FILES_NAME
        } else {
            &txt::FILES_FILTER
        };
        let field = ui.add(
            egui::TextEdit::singleline(&mut self.text)
                .hint_text(strings::tr(ui.ctx(), hint))
                .desired_width(f32::INFINITY),
        );
        egui_pop_core::a11y::name(
            &field,
            egui::WidgetType::TextEdit,
            &strings::tr(ui.ctx(), hint),
        );
        if first || tab || parent || down || up || enter {
            field.request_focus();
        }
        if field.changed() {
            self.highlight = 0;
            self.error = None;
        }
        if tab && self.complete() {
            self.highlight = 0;
            if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), field.id) {
                let end = egui::text::CCursor::new(self.text.chars().count());
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::one(end)));
                state.store(ui.ctx(), field.id);
            }
        }
        let completions = self.completions();
        if !completions.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for c in completions.iter().take(8) {
                    if ui.small_button(c).clicked() {
                        self.text = c.clone();
                    }
                }
            });
        }
        let shown = self.visible();
        if down {
            self.highlight = (self.highlight + 1).min(shown.len().saturating_sub(1));
        }
        if up {
            self.highlight = self.highlight.saturating_sub(1);
        }
        ui.add_space(t.space.s);
        ui.horizontal_top(|ui| {
            // Places.
            ui.vertical(|ui| {
                ui.set_width(150.0);
                let mut place = |ui: &mut Ui, name: &str, dir: &Path| {
                    if widgets::selectable_label(ui, self.dir == dir, name)
                        .on_hover_text(dir.display().to_string())
                        .clicked()
                    {
                        self.go(dir.to_path_buf());
                    }
                };
                if let Some(h) = &places.home {
                    place(ui, &strings::tr(ui.ctx(), &txt::FILES_HOME), h);
                }
                for (title, list) in [
                    (&txt::FILES_RECENT, &places.recent),
                    (&txt::FILES_BOOKMARKS, &places.bookmarks),
                ] {
                    if list.is_empty() {
                        continue;
                    }
                    ui.add_space(t.space.s);
                    ui.weak(strings::tr(ui.ctx(), title));
                    for d in list {
                        let name = d
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| d.display().to_string());
                        place(ui, &name, d);
                    }
                }
            });
            // Not a separator: in a row it takes the row's height, which
            // grows with it in a modal.
            ui.add_space(t.space.m);
            egui::ScrollArea::vertical()
                .id_salt("egui_pop.files.entries")
                .max_height(280.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // One entry per row (the row beside the places is
                    // laid out left to right).
                    ui.set_width(ui.available_width());
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                        for (n, e) in shown.iter().enumerate() {
                            let label = if e.is_dir {
                                format!("{}/", e.name)
                            } else {
                                e.name.clone()
                            };
                            // Selected files (choosing several) show as the
                            // highlighted one does.
                            let on = n == self.highlight || self.selected.contains(&e.path);
                            let r = widgets::selectable_label(ui, on, label);
                            if n == self.highlight && (down || up) {
                                r.scroll_to_me(None);
                            }
                            if r.double_clicked() {
                                self.highlight = n;
                                answer = answer.take().or_else(|| self.activate());
                            } else if r.clicked() {
                                self.highlight = n;
                                if e.is_dir {
                                    self.go(e.path.clone());
                                } else if self.kind == ChooseKind::OpenMany {
                                    self.toggle();
                                } else if self.kind == ChooseKind::Save {
                                    self.text = e.name.clone();
                                }
                            }
                        }
                        if shown.is_empty() {
                            ui.weak(strings::tr(ui.ctx(), &txt::NOTHING_MATCHES));
                        }
                    });
                });
        });
        if toggle {
            self.toggle();
        }
        if enter && answer.is_none() {
            answer = self.activate();
        }
        if accept && answer.is_none() {
            answer = self.accept();
        }
        if let Some(e) = &self.error {
            ui.colored_label(t.status.danger, e);
        }
        ui.add_space(t.space.m);
        ui.horizontal(|ui| {
            if !self.filters.is_empty() {
                let names: Vec<String> = self
                    .filters
                    .iter()
                    .map(|f| {
                        if f.extensions.is_empty() {
                            f.name.clone()
                        } else {
                            format!("{} (*.{})", f.name, f.extensions.join(", *."))
                        }
                    })
                    .chain([strings::tr(ui.ctx(), &txt::FILES_ALL)])
                    .collect();
                let mut at = self.filter_index.min(names.len() - 1);
                let combo =
                    widgets::combo_box("egui_pop.files.filter").selected_text(names[at].as_str());
                ui.combo_box(combo, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        widgets::selectable_value(ui, &mut at, i, n);
                    }
                });
                if at != self.filter_index {
                    self.filter_index = at;
                    self.highlight = 0;
                }
            }
            widgets::checkbox(
                ui,
                &mut self.show_hidden,
                strings::tr(ui.ctx(), &txt::FILES_HIDDEN),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let verb = match self.kind {
                    ChooseKind::Open | ChooseKind::OpenMany => &txt::FILES_OPEN,
                    ChooseKind::Save => &txt::FILES_SAVE,
                    ChooseKind::Folder => &txt::PICKER_CHOOSE_FOLDER,
                };
                if ui.button(strings::tr(ui.ctx(), verb)).clicked() {
                    answer = answer.take().or_else(|| self.accept());
                }
                if ui.button(strings::tr(ui.ctx(), &CANCEL)).clicked() {
                    answer = Some(PickerAnswer::Cancel);
                }
            });
        });
        answer
    }
}

/// The system's chooser, awaited on a thread of its own.
mod native {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use super::{Answer, FileRequest};

    #[cfg(all(feature = "native-dialogs", not(target_arch = "wasm32")))]
    pub(super) fn start(
        ctx: &egui::Context,
        request: &FileRequest,
        recent: Option<PathBuf>,
        answer: Arc<Mutex<Option<Answer>>>,
    ) {
        use std::future::Future;
        use std::pin::Pin;

        use super::ChooseKind;

        let mut d = rfd::AsyncFileDialog::new().set_title(&request.title);
        if let Some(dir) = request.directory.clone().or(recent) {
            d = d.set_directory(dir);
        }
        if let Some(name) = &request.file_name {
            d = d.set_file_name(name);
        }
        for f in &request.filters {
            d = d.add_filter(&f.name, &f.extensions);
        }
        // Made here, on the interface thread (macOS wants its panels made
        // on the main thread); awaited elsewhere.
        type Fut = Pin<Box<dyn Future<Output = Answer> + Send>>;
        let one = |p: Option<rfd::FileHandle>| p.map(|h| vec![h.path().to_path_buf()]);
        let fut: Fut = match request.kind {
            ChooseKind::Open => {
                let f = d.pick_file();
                Box::pin(async move { one(f.await) })
            }
            ChooseKind::OpenMany => {
                let f = d.pick_files();
                Box::pin(async move {
                    f.await
                        .map(|v| v.iter().map(|h| h.path().to_path_buf()).collect())
                })
            }
            ChooseKind::Save => {
                let f = d.save_file();
                Box::pin(async move { one(f.await) })
            }
            ChooseKind::Folder => {
                let f = d.pick_folder();
                Box::pin(async move { one(f.await) })
            }
        };
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("egui-pop-file-chooser".into())
            .spawn(move || {
                let r = block_on(fut);
                *egui_pop_core::lock(&answer) = Some(r);
                ctx.request_repaint();
            });
        if let Err(e) = spawned {
            log::warn!(target: "egui_pop", "files: no thread for the chooser: {e}");
        }
    }

    #[cfg(not(all(feature = "native-dialogs", not(target_arch = "wasm32"))))]
    pub(super) fn start(
        _ctx: &egui::Context,
        _request: &FileRequest,
        _recent: Option<PathBuf>,
        answer: Arc<Mutex<Option<Answer>>>,
    ) {
        // Never reached: `native_available` is false without the feature.
        *egui_pop_core::lock(&answer) = Some(None);
    }

    /// Runs a future to its end on this thread.
    #[cfg(all(feature = "native-dialogs", not(target_arch = "wasm32")))]
    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        use std::task::{Context, Poll, Wake, Waker};

        struct Unpark(std::thread::Thread);
        impl Wake for Unpark {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
        let mut cx = Context::from_waker(&waker);
        let mut f = std::pin::pin!(f);
        loop {
            if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
                return v;
            }
            std::thread::park();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("egui-pop-files-unit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for d in ["photos/2026", "photos/2025", "notes", ".hidden"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        for f in [
            "photos/a.png",
            "photos/b.jpg",
            "photos/readme.txt",
            "notes/todo.md",
        ] {
            std::fs::write(root.join(f), b"x").unwrap();
        }
        root
    }

    #[test]
    fn requests_round_trip_and_results_read() {
        let r = FileRequest::save("Export", "out.png")
            .directory("/tmp")
            .filter("PNG", &["PNG"]);
        assert_eq!(FileRequest::from_json(&r.to_json()).unwrap(), r);
        assert_eq!(r.filters[0].extensions, vec!["png"]);
        assert!(r.filters[0].accepts(Path::new("a.PNG")));
        assert_eq!(chosen(&Ok(Json::Null)), Ok(Chosen::Cancelled));
        assert_eq!(
            chosen(&Ok(json!({ "paths": ["/a", "/b"] }))),
            Ok(Chosen::Paths(vec!["/a".into(), "/b".into()]))
        );
        assert_eq!(chosen(&Err("no".into())), Err("no".into()));
    }

    #[test]
    fn the_picker_filters_completes_and_navigates() {
        let root = tree("navigate");
        let mut p = FilePicker::new(
            &FileRequest::open("Open")
                .directory(&root)
                .filter("Images", &["png", "jpg"]),
        );
        let names =
            |p: &mut FilePicker| p.visible().into_iter().map(|e| e.name).collect::<Vec<_>>();
        assert_eq!(names(&mut p), vec!["notes", "photos"]);
        p.go(root.join("photos"));
        // Folders first, then the files the filter accepts.
        assert_eq!(names(&mut p), vec!["2025", "2026", "a.png", "b.jpg"]);
        p.filter_index = 1; // all files
        assert_eq!(names(&mut p).len(), 5);
        // The fuzzy filter.
        p.text = "rdm".into();
        assert_eq!(names(&mut p), vec!["readme.txt"]);
        assert_eq!(
            p.activate(),
            Some(PickerAnswer::Chosen(vec![root.join("photos/readme.txt")]))
        );
        // A typed path completes, then Enter goes in.
        p.text = format!("{}/no", root.display());
        assert!(p.complete());
        assert_eq!(p.text, format!("{}/notes/", root.display()));
        assert_eq!(p.activate(), None);
        assert_eq!(p.dir, root.join("notes"));
        p.text = "../ph".into();
        assert!(p.complete());
        assert_eq!(p.text, "../photos/");
        p.text = "../photos/20".into();
        // 2025 and 2026 share 202.
        assert!(p.complete());
        assert_eq!(p.text, "../photos/202");
        assert!(!p.complete());
        assert_eq!(p.completions().len(), 2);
        // Hidden folders only when asked.
        p.go(root.clone());
        assert!(!names(&mut p).contains(&".hidden".to_string()));
        p.show_hidden = true;
        assert!(names(&mut p).contains(&".hidden".to_string()));
        // A missing file is said.
        p.text = "~/surely/not/here.png".into();
        assert_eq!(p.activate(), None);
        assert!(p.error.is_some());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn folders_saves_and_several_files() {
        let root = tree("kinds");
        let mut p = FilePicker::new(&FileRequest::folder("Folder").directory(&root));
        assert!(p.visible().iter().all(|e| e.is_dir));
        assert_eq!(p.accept(), Some(PickerAnswer::Chosen(vec![root.clone()])));
        let mut p = FilePicker::new(&FileRequest::save("Save", "new.png").directory(&root));
        assert_eq!(p.text, "new.png");
        assert_eq!(
            p.activate(),
            Some(PickerAnswer::Chosen(vec![root.join("new.png")]))
        );
        let mut p = FilePicker::new(&FileRequest::open_many("Many").directory(root.join("photos")));
        p.highlight = 2;
        p.toggle();
        p.highlight = 3;
        p.toggle();
        assert_eq!(
            p.accept(),
            Some(PickerAnswer::Chosen(vec![
                root.join("photos/a.png"),
                root.join("photos/b.jpg")
            ]))
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
