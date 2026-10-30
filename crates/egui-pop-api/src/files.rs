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
//!   off: a click selects and a double click or Enter opens, keyboard
//!   navigation (arrows, Enter, Backspace or `Alt+Up` for the parent
//!   folder, `Ctrl+Enter` to choose the folder or the selection,
//!   `Ctrl+D` to bookmark), several files selected with `Ctrl` or `Shift`
//!   and a click, `Shift` and the arrows, `Ctrl+Space` and `Ctrl+A`
//!   ([`CMD_TOGGLE_SELECTION`], [`CMD_SELECT_ALL`]), a typed path with
//!   completion (`Tab`), the fuzzy filter of the choice dialog,
//!   the request's filters, recent places and bookmarks kept in the host
//!   store (`settings/files.recent`, `settings/files.bookmarks`), and the
//!   last folder per purpose (`settings/files.last_folders`).
//!   Saving, it appends the filter's extension to a name without one and
//!   follows the filter when it changes, selects the name's stem, asks
//!   before replacing a file ([`FileRequest::confirm_overwrite`], through
//!   the dialogs plugin), and makes folders ([`CMD_NEW_FOLDER`]).
//!   It reads through a [`fs::FileSystem`]: the system's by default, or a
//!   folder held in memory ([`fs::MemoryFs`]; the web build has no file
//!   system), given with [`FilesPlugin::file_system`] or
//!   [`Files::set_file_system`].
//! - **Previews** come from pluggable providers ([`preview`]): plugins
//!   register theirs with [`Files::previews`]; they are made off the
//!   interface thread, cancelled when the highlight moves, and cached.
//!   The in-app picker shows the highlighted entry's preview and facts in
//!   a pane beside the list (`Alt+P` shows or hides it; the
//!   `files.preview` setting remembers). Built in ([`providers`]):
//!   images (PNG and JPEG, feature `image-previews`), text and code,
//!   Markdown drawn simply, folders with their first images.
//! - **The in-app picker's size** is the person's: its modal is resized
//!   by its edges and corners, a splitter sets the pane's width, and
//!   both are kept in the host store ([`DIALOG_SIZE`], [`PANE_WIDTH`]).
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
//! and answers them (`answer`), as a person would. The `file.preview`
//! command gives agents a path's preview metadata ([`preview_metadata`]).
//!
//! The question before replacing a file is the in-app picker's own: the
//! system's choosers ask theirs, a run-hook on `file.choose` and
//! [`Files::deliver`] answer for themselves, and none of them is asked
//! again.
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

use egui::{EventFilter, Key, Modifiers, Ui, pos2, vec2};
use egui_pop_core::a11y::{self, Politeness};
use egui_pop_core::debug::FnCollector;
use egui_pop_core::focus;
use egui_pop_core::lock;
use egui_pop_core::overlay::{self, Kind, OverlaySpec};
use egui_pop_core::strings::{self, CANCEL};
use egui_pop_core::theme::Tokens;
use egui_pop_core::ui::UiPop as _;
use egui_pop_core::widgets;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::fuzzy;
use egui_pop_model::ids::{BlockId, CommandId, OwnerId, PluginId, SettingId, StepId};
use egui_pop_model::keymap::BindOptions;
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use crate::app::AppHandle;
use crate::dialogs::{Confirm, DIALOGS};
use crate::host::{self, Pending, Resolver};
use crate::services::ServiceToken;
use crate::steps::{Outcome, StepSpec};
use crate::strings as txt;
use crate::zones::{self, Block};
use crate::{Ctx, Plugin, PluginError};

pub mod fs;
pub mod preview;
pub mod providers;

use fs::SharedFs;
use preview::Previews;

/// The file chooser, as a service.
pub const FILES: ServiceToken<Files> = ServiceToken::new("app.files");

/// A choice starts: the system's chooser or the in-app picker opens.
pub const STEP_CHOOSE: StepId = StepId::from_static("file.choose");
/// Paths were chosen.
pub const STEP_CHOSEN: StepId = StepId::from_static("file.chosen");

/// A path's preview metadata, for agents (`{ "path" }`).
pub const CMD_PREVIEW: CommandId = CommandId::from_static("file.preview");
/// A new folder in the open in-app picker (Ctrl+Shift+N): asks its name,
/// or makes it at once with `{ "name" }`.
pub const CMD_NEW_FOLDER: CommandId = CommandId::from_static("file.new_folder");
/// The answer to the question before replacing a file: the choice ends
/// with that file (`{ "id", "path" }`).
pub const CMD_REPLACE: CommandId = CommandId::from_static("file.replace");
/// Every file shown in the open in-app picker is selected (Ctrl+A, choosing
/// several files, unless the field holds a path: the key is then the field's).
pub const CMD_SELECT_ALL: CommandId = CommandId::from_static("file.select_all");
/// The highlighted entry of the open in-app picker is selected or unselected
/// (Ctrl+Space, choosing several files or a folder).
pub const CMD_TOGGLE_SELECTION: CommandId = CommandId::from_static("file.toggle_selection");

/// The context key: an open in-app picker can be asked for a new folder
/// (saving, or choosing a folder).
const WHEN_NEW_FOLDER: &str = "filePickerNewFolder";
/// The context key: an open in-app picker selects all its files on request.
const WHEN_SELECT_ALL: &str = "filePickerSelectAll";
/// The context key: an open in-app picker selects apart from its highlight.
const WHEN_SELECTS: &str = "filePickerSelects";

/// The setting: use the system's chooser when there is one.
pub const SETTING_NATIVE: SettingId = SettingId::from_static("files.native");
/// The setting: the in-app picker shows its preview pane.
pub const SETTING_PREVIEW: SettingId = SettingId::from_static("files.preview");

/// Where recent places are kept (a JSON list of folders, newest first).
pub const RECENT: &str = "settings/files.recent";
/// Where bookmarks are kept (a JSON list of folders).
pub const BOOKMARKS: &str = "settings/files.bookmarks";

/// Where the in-app picker's size is kept (width and height of its
/// content, in points).
pub const DIALOG_SIZE: &str = "settings/files.dialog_size";
/// Where the width of the in-app picker's preview pane is kept.
pub const PANE_WIDTH: &str = "settings/files.pane_width";

/// Where the last folder of each purpose is kept (a JSON list of
/// `[purpose, folder]` pairs, newest first). See [`FileRequest::purpose`].
pub const LAST_FOLDERS: &str = "settings/files.last_folders";

/// How many recent places are kept.
pub const RECENT_KEPT: usize = 8;
/// How many purposes keep a last folder: the one chosen in the longest ago
/// goes first.
pub const LAST_FOLDERS_KEPT: usize = 32;

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
    /// With a [`purpose`](Self::purpose), only until that purpose has a
    /// last folder, unless [`force_directory`](Self::force_directory) said
    /// otherwise.
    #[serde(default)]
    pub directory: Option<PathBuf>,
    /// The key the last folder is remembered under. See
    /// [`FileRequest::purpose`].
    #[serde(default)]
    pub purpose: Option<String>,
    /// `directory` wins over the purpose's last folder. See
    /// [`FileRequest::force_directory`].
    #[serde(default)]
    pub force_directory: bool,
    /// The name proposed when saving.
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub filters: Vec<FileFilter>,
    /// Saving: the in-app picker asks before replacing an existing file
    /// (on by default). See [`FileRequest::confirm_overwrite`].
    #[serde(default = "asks")]
    pub confirm_overwrite: bool,
}

fn asks() -> bool {
    true
}

impl FileRequest {
    fn new(kind: ChooseKind, title: &str) -> Self {
        Self {
            kind,
            title: title.into(),
            directory: None,
            purpose: None,
            force_directory: false,
            file_name: None,
            filters: Vec::new(),
            confirm_overwrite: true,
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

    /// The choice's purpose: its last folder is remembered under this key,
    /// so an import and an export do not share one (Windows' `SetClientGuid`,
    /// the `identifier` of a macOS panel). The next choice of that purpose
    /// starts there: [`directory`](Self::directory) is then the first-time
    /// default (Windows' `SetDefaultFolder`), and the start when the last
    /// folder is gone, before home. Recent places stay global, and a purpose
    /// does not start in them. Without a purpose nothing is remembered per
    /// choice and `directory` always wins.
    pub fn purpose(mut self, key: &str) -> Self {
        self.purpose = Some(key.into());
        self
    }

    /// Always starts in `dir` (Windows' `SetFolder`), over the purpose's last
    /// folder, which the choice still updates: for a request that names the
    /// folder that matters this time, as a document's own. When `dir` is gone,
    /// the last folder, then home.
    pub fn force_directory(mut self, dir: impl Into<PathBuf>) -> Self {
        self.directory = Some(dir.into());
        self.force_directory = true;
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

    /// Whether the in-app picker asks before replacing an existing file
    /// (on by default, as Windows' `FOS_OVERWRITEPROMPT` and GTK 4): off
    /// for an application that asks itself. It holds for the in-app
    /// picker only: the system's choosers ask on their own, and `rfd`
    /// has no way to tell them not to.
    pub fn confirm_overwrite(mut self, ask: bool) -> Self {
        self.confirm_overwrite = ask;
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
    /// Where the in-app picker reads (none: the system's file system).
    fs: Option<SharedFs>,
    previews: Previews,
}

impl State {
    /// The in-app picker commands speak to: the one in a modal first.
    fn active_picker(&mut self) -> Option<&mut FilePicker> {
        self.open
            .iter_mut()
            .filter(|o| o.picker.is_some())
            .min_by_key(|o| o.inline)
            .and_then(|o| o.picker.as_mut())
    }
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
    /// Where the in-app picker reads.
    pub fn file_system(&self) -> SharedFs {
        lock(&self.0).fs.clone().unwrap_or_else(fs::native)
    }

    /// Reads from `fs` in the in-app pickers opened from now on.
    pub fn set_file_system(&self, fs: SharedFs) {
        lock(&self.0).fs = Some(fs);
    }

    /// The preview providers, their cache and their worker: a plugin
    /// registers its providers here ([`Previews::register`]).
    pub fn previews(&self) -> Previews {
        lock(&self.0).previews.clone()
    }

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
        load_view(app, &mut picker);
        let out = picker.show(ui, &places(app));
        keep_view(app, &picker);
        settle(app, id, &mut picker, &answer, out, true);
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
        let fs = self.file_system();
        let previews = self.previews();
        let show_preview = preview_wanted(app);
        let start = start_folder(app, &request, &fs);
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
            native::start(&app.egui(), &request, start, answer);
            return Ok(json!({ "via": "native" }));
        }
        o.via = Via::InApp;
        let at = FileRequest {
            directory: start,
            ..request
        };
        let mut picker = FilePicker::new_in(&at, fs).with_previews(previews);
        picker.show_preview = show_preview;
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
            "previews": s.previews.describe(),
        })
    }

    /// `file.select_all` and `file.toggle_selection`: on the open in-app
    /// picker (the one in a modal first); gives how many are selected.
    fn select_on(
        &self,
        app: &AppHandle,
        select: impl FnOnce(&mut FilePicker) -> Result<(), String>,
    ) -> Result<Json, String> {
        let selected = {
            let mut s = lock(&self.0);
            let picker = s.active_picker().ok_or("no in-app file picker is open")?;
            select(picker)?;
            picker.selected.len()
        };
        app.egui().request_repaint();
        Ok(json!({ "selected": selected }))
    }

    /// `file.new_folder`: in the open in-app picker (the one in a modal
    /// first), asks the new folder's name, or makes it with `name`.
    fn new_folder(&self, app: &AppHandle, name: Option<&str>) -> Result<Json, String> {
        let made = {
            let mut s = lock(&self.0);
            let picker = s.active_picker().ok_or("no in-app file picker is open")?;
            if picker.asking.is_some() {
                return Err("the picker waits for its question's answer".into());
            }
            match name {
                Some(name) => Some(picker.create_folder(name)?),
                None => {
                    picker.begin_new_folder()?;
                    None
                }
            }
        };
        app.egui().request_repaint();
        Ok(match made {
            Some(path) => {
                folder_created(app, &path);
                json!({ "path": path.display().to_string() })
            }
            None => json!({ "naming": true }),
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
    write_text(app, path, text);
}

fn write_text(app: &AppHandle, path: &str, text: String) {
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

/// The last folders by purpose, newest first.
fn last_folders(app: &AppHandle) -> Vec<(String, PathBuf)> {
    lock(&app.store())
        .get(LAST_FOLDERS)
        .and_then(|e| e.value.as_text().map(str::to_string))
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// The last folder of a purpose ([`FileRequest::purpose`]), whether or not it
/// still exists.
pub fn last_folder(app: &AppHandle, purpose: &str) -> Option<PathBuf> {
    last_folders(app)
        .into_iter()
        .find_map(|(p, dir)| (p == purpose).then_some(dir))
}

/// Keeps the folder of a finished choice under its purpose, if it has one: the
/// folder chosen, or the first path's parent.
fn remember_folder(app: &AppHandle, request: &FileRequest, paths: &[PathBuf]) {
    let (Some(purpose), Some(first)) = (&request.purpose, paths.first()) else {
        return;
    };
    let dir = if request.kind == ChooseKind::Folder {
        Some(first.as_path())
    } else {
        first.parent()
    };
    let Some(dir) = dir.filter(|d| !d.as_os_str().is_empty()) else {
        return;
    };
    let mut list = last_folders(app);
    list.retain(|(p, _)| p != purpose);
    list.insert(0, (purpose.clone(), dir.to_path_buf()));
    list.truncate(LAST_FOLDERS_KEPT);
    // A folder whose name is not Unicode cannot be kept: the list stays as it was.
    if let Ok(text) = serde_json::to_string(&list) {
        write_text(app, LAST_FOLDERS, text);
    }
}

/// Where a choice starts. Without a purpose, the request's folder else the
/// last recent place, as given (the in-app picker checks it, the system's
/// chooser does its own). With one, the first that exists: its last folder,
/// the request's folder (the other way round when forced), home.
fn start_folder(app: &AppHandle, request: &FileRequest, fs: &SharedFs) -> Option<PathBuf> {
    let Some(purpose) = &request.purpose else {
        return request
            .directory
            .clone()
            .or_else(|| recent(app).first().cloned());
    };
    let existing = |d: Option<PathBuf>| d.filter(|d| fs.is_dir(d));
    let last = existing(last_folder(app, purpose));
    let asked = existing(request.directory.clone());
    let (first, second) = if request.force_directory {
        (asked, last)
    } else {
        (last, asked)
    };
    first.or(second).or_else(|| existing(home()))
}

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.files");

/// The plugin.
#[derive(Clone, Debug)]
pub struct FilesPlugin {
    native: bool,
    fs: Option<SharedFs>,
    builtin_previews: bool,
}

impl Default for FilesPlugin {
    /// The system's chooser when there is one.
    fn default() -> Self {
        Self {
            native: true,
            fs: None,
            builtin_previews: true,
        }
    }
}

impl FilesPlugin {
    /// The in-app picker by default (the setting may still switch).
    pub fn in_app() -> Self {
        Self {
            native: false,
            fs: None,
            builtin_previews: true,
        }
    }

    /// The in-app picker reads from `fs` (a [`fs::MemoryFs`], say), not
    /// from the system's file system.
    pub fn file_system(mut self, fs: SharedFs) -> Self {
        self.fs = Some(fs);
        self
    }

    /// Without the built-in preview providers ([`providers::builtin`]):
    /// only those plugins register.
    pub fn without_builtin_previews(mut self) -> Self {
        self.builtin_previews = false;
        self
    }
}

impl Plugin for FilesPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Files".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let files = Files::default();
        if let Some(fs) = self.fs.clone() {
            files.set_file_system(fs);
        }
        if self.builtin_previews {
            for p in providers::builtin() {
                files.previews().register(p);
            }
        }
        ctx.setting(
            SETTING_NATIVE,
            Descriptor::bool("Use the system's file chooser", self.native).description(
                "Choose files with the system's chooser when there is one; off, with egui-pop's own.",
            ),
        )?;
        ctx.setting(
            SETTING_PREVIEW,
            Descriptor::bool("Previews in the file picker", true).description(
                "egui-pop's file picker shows the selected file's preview and facts beside the list; Alt+P shows or hides it.",
            ),
        )?;
        {
            let store = ctx.app().store();
            let mut store = lock(&store);
            for (path, label) in [
                (RECENT, "Recent places"),
                (BOOKMARKS, "Bookmarks"),
                (LAST_FOLDERS, "Last folders by purpose"),
            ] {
                store
                    .declare(
                        ctx.owner(),
                        path,
                        Descriptor::text(label, "[]").capacity(16_384),
                    )
                    .map_err(|e| PluginError::Failed(format!("{path}: {e:?}")))?;
            }
            // No settings of their own in the settings window: dragged,
            // not typed.
            let size = [f64::from(PICKER_WIDTH), f64::from(PICKER_HEIGHT)];
            for (path, descriptor) in [
                (
                    DIALOG_SIZE,
                    Descriptor::floats("File picker size", 2, 0.0, MAX_LENGTH, &size),
                ),
                (
                    PANE_WIDTH,
                    Descriptor::float(
                        "File picker preview width",
                        0.0,
                        MAX_LENGTH,
                        f64::from(preview::PANE_WIDTH),
                    ),
                ),
            ] {
                store
                    .declare(ctx.owner(), path, descriptor)
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
        let f = files.clone();
        ctx.command(
            CommandSpec::new(CMD_PREVIEW, "Preview a file", "File")
                .description("A file's or a folder's preview metadata (`path`), as the in-app picker shows it: whether it is a folder, its size and modification time, the provider, the kind of preview, dimensions, pages, a folder's counts, the provider's facts, or why there is none; no pixels and no text.")
                .args(json!({ "path": "/tmp/a.png" }))
                .hidden(),
            move |call| {
                let path = call
                    .args
                    .get("path")
                    .and_then(Json::as_str)
                    .ok_or("`path` is missing")?;
                preview_metadata(&f, Path::new(path))
            },
        )?;
        let f = files.clone();
        ctx.command(
            CommandSpec::new(CMD_NEW_FOLDER, "New folder", "File")
                .description("A new folder in the open in-app file picker, which then shows it: asks its name, or makes it at once with `name`. Refused on a file system that cannot make one (the web build's, in memory), the reason shown in the picker.")
                .args(json!({ "name": "Exports" }))
                .when(WHEN_NEW_FOLDER)
                .key_with("Ctrl+Shift+N", BindOptions::global().through_modal()),
            move |call| {
                let name = call.args.get("name").and_then(Json::as_str);
                f.new_folder(call.app, name)
            },
        )?;
        let f = files.clone();
        ctx.command(
            CommandSpec::new(CMD_SELECT_ALL, "Select all files", "File")
                .description("Selects every file shown in the open in-app file picker, choosing several. Refused, the reason shown in the picker, when none is shown.")
                .when(WHEN_SELECT_ALL)
                .key_with("Ctrl+A", BindOptions::global().through_modal()),
            move |call| f.select_on(call.app, FilePicker::select_all),
        )?;
        let f = files.clone();
        ctx.command(
            CommandSpec::new(CMD_TOGGLE_SELECTION, "Select or unselect the entry", "File")
                .description("Selects or unselects the highlighted entry of the open in-app file picker, choosing several files or a folder. A folder among files is not selected: the picker says so.")
                .when(WHEN_SELECTS)
                .key_with("Ctrl+Space", BindOptions::global().through_modal()),
            move |call| {
                f.select_on(call.app, |p| {
                    p.toggle();
                    Ok(())
                })
            },
        )?;
        let f = files.clone();
        ctx.command(
            CommandSpec::new(CMD_REPLACE, "Replace the file", "File")
                .description("The answer to the question before replacing a file: ends the choice `id` with `path`.")
                .args(json!({ "id": 1, "path": "/tmp/a.png" }))
                .hidden(),
            move |call| {
                let id = call.args.get("id").and_then(Json::as_u64).ok_or("`id` is missing")?;
                let path = call.args.get("path").and_then(Json::as_str).ok_or("`path` is missing")?;
                f.deliver(id, Some(vec![PathBuf::from(path)]))?;
                call.app.egui().request_repaint();
                Ok(Json::Null)
            },
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
    let (new_folder, select_all, selects) = {
        let mut s = lock(&files.0);
        s.native = native;
        let any = |test: &dyn Fn(&FilePicker) -> bool| {
            s.open
                .iter()
                .filter_map(|o| o.picker.as_ref())
                .any(|p| p.asking.is_none() && test(p))
        };
        (
            any(&FilePicker::makes_folders),
            any(&|p| p.kind == ChooseKind::OpenMany && !p.is_path()),
            any(&FilePicker::selects_apart),
        )
    };
    let say = |on: bool| if on { "true" } else { "false" };
    app.set_context(WHEN_NEW_FOLDER, say(new_folder));
    app.set_context(WHEN_SELECT_ALL, say(select_all));
    app.set_context(WHEN_SELECTS, say(selects));
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
    if !done.is_empty() {
        // Nothing to preview for them any more.
        files.previews().cancel();
    }
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
                remember_folder(app, request, &paths);
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

/// What the picker answered, applied: the choice's answer, the question
/// before replacing a file, a bookmark, a new folder in the journal.
fn settle(
    app: &AppHandle,
    id: u64,
    picker: &mut FilePicker,
    answer: &Mutex<Option<Answer>>,
    out: Option<PickerAnswer>,
    open: bool,
) {
    match out {
        Some(PickerAnswer::Chosen(paths)) => *lock(answer) = Some(Some(paths)),
        Some(PickerAnswer::Cancel) => *lock(answer) = Some(None),
        Some(PickerAnswer::Bookmark(dir, on)) => set_bookmark(app, &dir, on),
        Some(PickerAnswer::Replace(path)) => picker.ask_replace(app, id, &path),
        Some(PickerAnswer::FolderCreated(path)) => folder_created(app, &path),
        None if !open => *lock(answer) = Some(None),
        None => {}
    }
}

/// A folder was made: the journal says so.
fn folder_created(app: &AppHandle, path: &Path) {
    app.publish(
        "user",
        None,
        "file.folder_created",
        json!({ "path": path.display().to_string() }),
    );
}

fn places(app: &AppHandle) -> Places {
    Places {
        home: home(),
        recent: recent(app),
        bookmarks: bookmarks(app),
    }
}

/// The width of the in-app picker's modal at first, the preview pane
/// shown or not: wide enough for the places, the list and the full pane.
pub const PICKER_WIDTH: f32 = 880.0;
/// The height of the in-app picker's modal at first: the list has its
/// usual height ([`LIST_HEIGHT`]).
pub const PICKER_HEIGHT: f32 = 398.0;
/// The least size the in-app picker's modal is dragged to: the row of
/// its filters and buttons fits, and a few entries.
pub const PICKER_MIN_SIZE: egui::Vec2 = egui::vec2(520.0, 280.0);
/// The list's height when the picker is not given a height to fill.
pub const LIST_HEIGHT: f32 = 280.0;
/// The narrowest the picker's list gets for the preview pane: under
/// it, the pane shrinks instead.
pub const LIST_MIN_WIDTH: f32 = 240.0;
/// The narrowest preview pane drawn: with less room, it is not shown.
pub const PANE_MIN_WIDTH: f32 = 120.0;

/// More than any window: what the store accepts for a length.
const MAX_LENGTH: f64 = 65_536.0;

/// Splits the room beside the places between the list and the preview
/// pane (when it is on, with `gap` between them): the pane takes the
/// width `wanted` from the list's while the list keeps `LIST_MIN_WIDTH`,
/// then shrinks, down to `PANE_MIN_WIDTH`, under which it is hidden.
fn split_list(available: f32, gap: f32, preview: bool, wanted: f32) -> (f32, Option<f32>) {
    if !preview {
        return (available, None);
    }
    let pane = (available - gap - LIST_MIN_WIDTH).min(wanted.max(PANE_MIN_WIDTH));
    if pane < PANE_MIN_WIDTH {
        (available, None)
    } else {
        (available - gap - pane, Some(pane))
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
    load_view(app, &mut picker);
    let modal = overlay_id(id);
    // One size, the preview on or off: the pane takes its width from
    // the list's, so toggling it does not resize the dialog. The size at
    // opening is what the person left, read once per dialog.
    let size = overlay::modal_size(ctx, modal).unwrap_or_else(|| opening_size(ctx, app));
    let mut spec = OverlaySpec::new(("egui_pop.files", id), &title, Kind::Modal)
        .resizable(size, PICKER_MIN_SIZE);
    debug_assert_eq!(spec.id, modal);
    if picker.naming.is_some() {
        // Escape closes the row asking a new folder's name, not the picker.
        spec = spec.keep_on_escape();
    }
    picker.fill = true;
    let mut open = true;
    let mut out = None;
    overlay::show(ctx, &spec, &mut open, None, |ui| {
        ui.heading(&title);
        out = picker.show(ui, &places);
    });
    keep_view(app, &picker);
    // What the person left, not what the window holds it to; a drag is
    // kept once it is over.
    if open
        && !overlay::modal_resizing(ctx, modal)
        && let Some(size) = overlay::modal_size(ctx, modal)
        && size != dialog_size(app)
    {
        want_dialog_size(app, size);
    }
    settle(app, id, &mut picker, &answer, out, open);
    files.keep(id, picker);
}

/// The size the in-app picker opens at: the one kept, or back to the
/// default when it no longer fits the window (or makes no sense).
fn opening_size(ctx: &egui::Context, app: &AppHandle) -> egui::Vec2 {
    let size = dialog_size(app);
    let max = overlay::modal_max_size(ctx);
    let fits = size.x >= PICKER_MIN_SIZE.x
        && size.y >= PICKER_MIN_SIZE.y
        && size.x <= max.x
        && size.y <= max.y;
    if fits {
        return size;
    }
    let set = Set::new(DIALOG_SIZE, Change::Reset, Origin::User)
        .by(&OwnerId::FRAMEWORK)
        .untracked();
    if let Err(e) = lock(&app.store()).set(set) {
        log::warn!(target: "egui_pop", "{DIALOG_SIZE}: {e:?}");
    }
    dialog_size(app)
}

/// What the kept view says, for the picker to show: the preview pane on
/// or off and its width (not during a drag of its splitter), the keys of
/// its new folder, and whether its question is still open.
fn load_view(app: &AppHandle, picker: &mut FilePicker) {
    if let Some(dialog) = picker.asking
        && !app.service(&DIALOGS).is_some_and(|d| d.is_open(dialog))
    {
        picker.asking = None;
        picker.refocus = true;
    }
    picker.new_folder_keys = app.keys_for(&CMD_NEW_FOLDER);
    picker.declared_keys = true;
    picker.show_preview = preview_wanted(app);
    if !picker.pane_dragging {
        picker.pane_width = pane_width(app);
    }
}

/// Keeps what the person changed of the view; a drag of the splitter
/// once it is over.
fn keep_view(app: &AppHandle, picker: &FilePicker) {
    if picker.show_preview != preview_wanted(app) {
        want_preview(app, picker.show_preview);
    }
    if !picker.pane_dragging && picker.pane_width != pane_width(app) {
        want_pane_width(app, picker.pane_width);
    }
}

/// A path's preview metadata, as `file.preview` gives it: made now
/// from the providers if it is not in the cache. Metadata only: no
/// pixels and no text.
pub fn preview_metadata(files: &Files, path: &Path) -> Result<Json, String> {
    let fs = files.file_system();
    let meta = fs
        .metadata(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut d = files.previews().compute(&fs, path).describe();
    d["path"] = json!(path.display().to_string());
    d["is_dir"] = json!(meta.is_dir);
    if !meta.is_dir {
        d["size"] = json!(meta.len);
    }
    if let Some(t) = meta.modified {
        if let Ok(since) = t.duration_since(std::time::SystemTime::UNIX_EPOCH) {
            d["modified"] = json!(since.as_secs());
        }
        d["modified_text"] = json!(preview::time_text(t));
    }
    Ok(d)
}

/// Whether the in-app picker shows its preview pane (the setting).
pub fn preview_wanted(app: &AppHandle) -> bool {
    lock(&app.store())
        .get(&format!("settings/{SETTING_PREVIEW}"))
        .and_then(|e| e.value.as_bool())
        .unwrap_or(true)
}

/// Shows or hides the in-app picker's preview pane (the setting, kept).
pub fn want_preview(app: &AppHandle, on: bool) {
    let set = Set::new(
        &format!("settings/{SETTING_PREVIEW}"),
        Change::To(Value::Bool(on)),
        Origin::User,
    )
    .by(&OwnerId::FRAMEWORK);
    if let Err(e) = lock(&app.store()).set(set) {
        log::warn!(target: "egui_pop", "{SETTING_PREVIEW}: {e:?}");
    }
}

/// Writes a remembered length of the picker, outside any undo history.
fn keep_lengths(app: &AppHandle, path: &str, value: Value) {
    let set = Set::new(path, Change::To(value), Origin::User)
        .by(&OwnerId::FRAMEWORK)
        .untracked();
    if let Err(e) = lock(&app.store()).set(set) {
        log::warn!(target: "egui_pop", "{path}: {e:?}");
    }
}

/// The in-app picker's size, as kept (it may not fit the window now).
pub fn dialog_size(app: &AppHandle) -> egui::Vec2 {
    let store = app.store();
    let store = lock(&store);
    match store.get(DIALOG_SIZE).map(|e| &e.value) {
        Some(Value::Floats(f)) if f.len() == 2 => vec2(f[0] as f32, f[1] as f32),
        _ => vec2(PICKER_WIDTH, PICKER_HEIGHT),
    }
}

/// Keeps the in-app picker's size.
pub fn want_dialog_size(app: &AppHandle, size: egui::Vec2) {
    let value = Value::Floats(vec![f64::from(size.x), f64::from(size.y)]);
    keep_lengths(app, DIALOG_SIZE, value);
}

/// The width of the in-app picker's preview pane, as kept (the room in
/// the dialog may hold it smaller).
pub fn pane_width(app: &AppHandle) -> f32 {
    lock(&app.store())
        .get(PANE_WIDTH)
        .and_then(|e| e.value.as_f64())
        .map_or(preview::PANE_WIDTH, |w| w as f32)
}

/// Keeps the width of the in-app picker's preview pane.
pub fn want_pane_width(app: &AppHandle, width: f32) {
    keep_lengths(app, PANE_WIDTH, Value::Float(f64::from(width)));
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
    /// This file was chosen and exists: ask before replacing it (only with
    /// [`FileRequest::confirm_overwrite`]). [`Files`] asks through the
    /// dialogs plugin; a caller drawing a [`FilePicker`] on its own asks
    /// its way, and takes the path as chosen on a yes.
    Replace(PathBuf),
    /// This folder was made, and the picker shows it.
    FolderCreated(PathBuf),
}

/// Where the file's own name starts in a name or a typed path.
fn file_part(text: &str) -> usize {
    text.rfind(['/', std::path::MAIN_SEPARATOR])
        .map_or(0, |i| i + 1)
}

/// Where the dot of a name's extension is (bytes), if it has one. A
/// leading dot makes a hidden file rather than an extension, a trailing
/// one has nothing after it, and what follows a dot in `Mr. Smith` holds
/// a space: none of them is an extension.
fn extension_dot(text: &str) -> Option<usize> {
    let start = file_part(text);
    let dot = text[start..].rfind('.').filter(|d| *d > 0)? + start;
    let extension = &text[dot + 1..];
    (!extension.is_empty() && !extension.contains(char::is_whitespace)).then_some(dot)
}

/// A name with `extension` added when it has none; a hidden file's name
/// (`.config`) stays as it is.
fn with_extension(text: &str, extension: &str) -> String {
    let file = &text[file_part(text)..];
    if extension_dot(text).is_some() || file.is_empty() || file.starts_with('.') {
        return text.to_string();
    }
    format!("{}.{extension}", text.trim_end_matches('.'))
}

/// A name's stem, in characters: what typing replaces.
fn stem_chars(text: &str) -> (usize, usize) {
    let start = file_part(text);
    let end = extension_dot(text).unwrap_or(text.len());
    (text[..start].chars().count(), text[..end].chars().count())
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
    /// The entry highlighted among those shown: where the arrows are, what
    /// the preview is of, and what Enter acts on.
    pub highlight: usize,
    /// What is selected, in the folder shown: files (choosing several), or
    /// one folder (choosing a folder, or saving: Enter goes into it).
    pub selected: BTreeSet<PathBuf>,
    pub show_hidden: bool,
    /// The preview pane is shown (with previews, [`FilePicker::with_previews`]).
    pub show_preview: bool,
    /// The preview pane's width, as wanted: the room in the dialog may
    /// hold it smaller.
    pub pane_width: f32,
    /// The list takes the height left in the `Ui` it is drawn in (a
    /// dialog of a given size), rather than [`LIST_HEIGHT`].
    pub fill: bool,
    /// Why the last Enter did nothing.
    pub error: Option<String>,
    /// The new folder's name being typed.
    naming: Option<String>,
    /// That field takes the focus.
    naming_focus: bool,
    /// The keys of the new folder command, as the button's tip says them.
    new_folder_keys: Vec<String>,
    /// The selection's keys are commands ([`Files`] draws this picker);
    /// drawn on its own, it has no keymap and reads them itself.
    declared_keys: bool,
    /// Where `Shift` extends the selection from.
    anchor: Option<PathBuf>,
    /// What the selection line said last: a change is announced.
    said: Option<String>,
    /// The list's rows as last drawn: holding the focus, they leave the
    /// picker's keys to the picker.
    row_ids: Vec<egui::Id>,
    /// The name proposed when saving: back when a typed name was a folder.
    proposed: String,
    /// Asks before replacing a file.
    confirm_overwrite: bool,
    /// The dialog asking whether to replace a file: the picker waits.
    asking: Option<u64>,
    /// The question closed: the field takes the focus back, which it lost
    /// while disabled.
    refocus: bool,
    /// The name's stem is selected in the field at the next frame.
    select_stem: bool,
    /// Where text is translated, once drawn.
    ctx: Option<egui::Context>,
    focused: bool,
    /// The splitter is being dragged.
    pane_dragging: bool,
    /// The height of what comes below the list, as last drawn.
    footer_height: Option<f32>,
    listing: Option<(PathBuf, bool, Vec<Entry>)>,
    fs: SharedFs,
    previews: Option<Previews>,
}

impl FilePicker {
    /// A picker for a request, in its folder (else home, else the root),
    /// on the system's file system.
    pub fn new(request: &FileRequest) -> Self {
        Self::new_in(request, fs::native())
    }

    /// A picker for a request reading from `fs`, in the request's folder
    /// (else home, else the root).
    pub fn new_in(request: &FileRequest, fs: SharedFs) -> Self {
        let dir = request
            .directory
            .clone()
            .filter(|d| fs.is_dir(d))
            .or_else(|| home().filter(|h| fs.is_dir(h)))
            .unwrap_or_else(|| PathBuf::from("/"));
        let proposed = if request.kind == ChooseKind::Save {
            request.file_name.clone().unwrap_or_default()
        } else {
            String::new()
        };
        Self {
            kind: request.kind,
            dir,
            filters: request.filters.clone(),
            filter_index: 0,
            text: proposed.clone(),
            highlight: 0,
            selected: BTreeSet::new(),
            show_hidden: false,
            show_preview: false,
            pane_width: preview::PANE_WIDTH,
            fill: false,
            error: None,
            naming: None,
            naming_focus: false,
            new_folder_keys: Vec::new(),
            declared_keys: false,
            anchor: None,
            said: None,
            row_ids: Vec::new(),
            proposed,
            confirm_overwrite: request.confirm_overwrite,
            asking: None,
            refocus: false,
            select_stem: request.kind == ChooseKind::Save,
            ctx: None,
            focused: false,
            pane_dragging: false,
            footer_height: None,
            listing: None,
            fs,
            previews: None,
        }
    }

    /// Shows the selected entry's preview in a pane beside the list,
    /// from these providers (`Alt+P` hides it).
    pub fn with_previews(mut self, previews: Previews) -> Self {
        self.previews = Some(previews);
        self.show_preview = true;
        self
    }

    /// The entry the preview is of: the highlighted one.
    pub fn highlighted(&mut self) -> Option<Entry> {
        let at = self.highlight;
        self.visible().into_iter().nth(at)
    }

    /// Where it reads.
    pub fn file_system(&self) -> &SharedFs {
        &self.fs
    }

    /// Shows another folder; the selection stays behind.
    pub fn go(&mut self, dir: PathBuf) {
        self.dir = dir;
        self.highlight = 0;
        self.error = None;
        self.selected.clear();
        self.anchor = None;
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
        let mut all: Vec<Entry> = self
            .fs
            .read_dir(&self.dir)
            .map(|r| {
                r.into_iter()
                    .map(|e| Entry {
                        path: self.dir.join(&e.name),
                        name: e.name,
                        is_dir: e.is_dir,
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
        let Ok(read) = self.fs.read_dir(&head_path) else {
            return Vec::new();
        };
        let lower = part.to_lowercase();
        let mut out: Vec<String> = read
            .into_iter()
            .filter_map(|e| {
                let (name, is_dir) = (e.name, e.is_dir);
                let shown = (self.show_hidden || !name.starts_with('.') || part.starts_with('.'))
                    && name.to_lowercase().starts_with(&lower)
                    && (is_dir
                        || (self.kind != ChooseKind::Folder
                            && self.accepts(&head_path.join(&name))));
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

    /// Enter: into a folder, or the choice. Saving, the folder selected
    /// goes first: otherwise the field's name is what Enter takes.
    pub fn activate(&mut self) -> Option<PickerAnswer> {
        self.error = None;
        if self.kind == ChooseKind::Save {
            if let Some(dir) = self.selected.first().cloned() {
                self.go(dir);
                self.select_stem = true;
                return None;
            }
            return self.save();
        }
        if self.is_path() {
            let p = self.typed_path();
            if self.fs.is_dir(&p) {
                self.go(p);
                return None;
            }
            return match self.kind {
                ChooseKind::Open | ChooseKind::OpenMany if self.fs.is_file(&p) => {
                    Some(PickerAnswer::Chosen(vec![p]))
                }
                _ => {
                    self.error = Some(format!("{} does not exist", p.display()));
                    None
                }
            };
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

    /// Enter, saving: into a folder, else the field's name with the
    /// filter's extension when it has none, and the question when it
    /// exists.
    fn save(&mut self) -> Option<PickerAnswer> {
        let typed = self.text.trim().to_string();
        if typed.is_empty() {
            return None;
        }
        if self.enters(self.typed_path()) {
            return None;
        }
        let parent = self.typed_path().parent().map(Path::to_path_buf);
        if file_part(&typed) == typed.len() || !parent.is_some_and(|d| self.fs.is_dir(&d)) {
            self.error = Some(format!("{} does not exist", self.typed_path().display()));
            return None;
        }
        if let Some(extension) = self.filter().and_then(|f| f.extensions.first()) {
            self.text = with_extension(&typed, extension);
        }
        let p = self.typed_path();
        if self.enters(p.clone()) {
            return None;
        }
        if self.confirm_overwrite && self.fs.is_file(&p) {
            return Some(PickerAnswer::Replace(p));
        }
        Some(PickerAnswer::Chosen(vec![p]))
    }

    /// Goes into `p` if it is a folder, the proposed name back in the
    /// field in place of the folder's; says whether it did.
    fn enters(&mut self, p: PathBuf) -> bool {
        if !self.fs.is_dir(&p) {
            return false;
        }
        self.go(p);
        self.text = self.proposed.clone();
        self.select_stem = true;
        true
    }

    /// Takes another filter (`filters.len()`: all files). Saving, a name
    /// whose extension one of the request's filters names takes the new
    /// filter's first, unless the new filter accepts it too; an extension
    /// no filter names is the person's and stays, and so does the name
    /// under a filter without extensions.
    pub fn set_filter(&mut self, index: usize) {
        self.filter_index = index;
        self.highlight = 0;
        if self.kind != ChooseKind::Save {
            return;
        }
        let Some(new) = self.filter().and_then(|f| f.extensions.first()).cloned() else {
            return;
        };
        let Some(dot) = extension_dot(&self.text) else {
            return;
        };
        let typed = self.text[dot + 1..].to_string();
        let named = |f: &FileFilter| f.extensions.iter().any(|e| e.eq_ignore_ascii_case(&typed));
        if self.filters.iter().any(named) && !self.filter().is_some_and(named) {
            self.text.truncate(dot + 1);
            self.text.push_str(&new);
            self.select_stem = true;
        }
    }

    /// Whether this kind of choice offers a new folder.
    fn makes_folders(&self) -> bool {
        matches!(self.kind, ChooseKind::Save | ChooseKind::Folder)
    }

    /// The text of `key` as the picker's language says it.
    fn say(&self, key: &strings::StrKey, args: &[(&str, &str)]) -> String {
        match &self.ctx {
            Some(ctx) => strings::tr_args(ctx, key, args),
            None => strings::fill(key.fallback, args),
        }
    }

    /// Asks the new folder's name, in a row under the folder: Enter makes
    /// it, Escape closes the row. Without a file system that can make one,
    /// the reason is the picker's error, and the result.
    pub fn begin_new_folder(&mut self) -> Result<(), String> {
        self.error = None;
        if !self.makes_folders() {
            return Err("a new folder is offered when saving or choosing a folder".into());
        }
        if !self.fs.can_create_dirs() {
            let reason = self.say(&txt::FILES_FOLDER_READ_ONLY, &[]);
            self.error = Some(reason.clone());
            return Err(reason);
        }
        self.naming.get_or_insert_default();
        self.naming_focus = true;
        Ok(())
    }

    /// Makes a folder in the one shown, and goes into it. A refusal is the
    /// picker's error, and the result.
    pub fn create_folder(&mut self, name: &str) -> Result<PathBuf, String> {
        match self.make_folder(name.trim()) {
            Ok(path) => {
                self.naming = None;
                self.go(path.clone());
                Ok(path)
            }
            Err(reason) => {
                self.error = Some(reason.clone());
                Err(reason)
            }
        }
    }

    fn make_folder(&self, name: &str) -> Result<PathBuf, String> {
        if !self.makes_folders() {
            return Err("a new folder is offered when saving or choosing a folder".into());
        }
        if !self.fs.can_create_dirs() {
            return Err(self.say(&txt::FILES_FOLDER_READ_ONLY, &[]));
        }
        if name.is_empty() {
            return Err(self.say(&txt::FILES_FOLDER_NAME_EMPTY, &[]));
        }
        if matches!(name, "." | "..") || file_part(name) != 0 {
            return Err(self.say(&txt::FILES_FOLDER_NAME_INVALID, &[("name", name)]));
        }
        let path = self.dir.join(name);
        if self.fs.metadata(&path).is_ok() {
            return Err(self.say(&txt::FILES_FOLDER_EXISTS, &[("name", name)]));
        }
        self.fs.create_dir(&path).map_err(|e| {
            let reason = e.to_string();
            self.say(
                &txt::FILES_FOLDER_FAILED,
                &[("name", name), ("reason", &reason)],
            )
        })?;
        Ok(path)
    }

    /// Opens the question before replacing `path` over the picker, which
    /// waits for the dialog; the yes ends the choice ([`CMD_REPLACE`]).
    /// Without the dialogs plugin nobody can ask: nothing is replaced, and
    /// the picker says so.
    fn ask_replace(&mut self, app: &AppHandle, id: u64, path: &Path) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let ctx = app.egui();
        let Some(dialogs) = app.service(&DIALOGS) else {
            log::warn!(target: "egui_pop", "files: no dialogs plugin to ask before replacing {}", path.display());
            self.error = Some(strings::tr_args(
                &ctx,
                &txt::FILES_REPLACE_NO_DIALOGS,
                &[("name", &name)],
            ));
            return;
        };
        let folder = path
            .parent()
            .map(|d| d.display().to_string())
            .unwrap_or_default();
        let question = Confirm::new(
            &strings::tr_args(&ctx, &txt::FILES_REPLACE_TITLE, &[("name", &name)]),
            &strings::tr(&ctx, &txt::FILES_REPLACE),
            CMD_REPLACE,
        )
        .detail(&strings::tr_args(
            &ctx,
            &txt::FILES_REPLACE_DETAIL,
            &[("folder", &folder)],
        ))
        .danger()
        .args(json!({ "id": id, "path": path.display().to_string() }));
        self.asking = Some(dialogs.open_now(question));
        ctx.request_repaint();
    }

    /// Ctrl+Enter, and the primary button: the folder (choosing a
    /// folder), the selection, or what Enter would choose.
    pub fn accept(&mut self) -> Option<PickerAnswer> {
        match self.kind {
            ChooseKind::Folder => Some(PickerAnswer::Chosen(vec![self.folder_choice()])),
            ChooseKind::OpenMany if !self.selected.is_empty() => Some(PickerAnswer::Chosen(
                self.selected.iter().cloned().collect(),
            )),
            _ => self.activate(),
        }
    }

    /// What choosing a folder takes now: a typed folder, the folder selected,
    /// the entry the filter leaves highlighted, else the folder shown.
    fn folder_choice(&mut self) -> PathBuf {
        if self.is_path() && self.fs.is_dir(&self.typed_path()) {
            return self.typed_path();
        }
        if let Some(selected) = self.selected.first() {
            return selected.clone();
        }
        if !self.text.trim().is_empty()
            && let Some(e) = self.visible().get(self.highlight)
        {
            return e.path.clone();
        }
        self.dir.clone()
    }

    /// Selects or unselects the highlighted entry: a file choosing several (a
    /// folder is refused, the reason in [`error`](Self::error)), a folder
    /// choosing one. Does nothing in the other kinds, which have no key for it.
    pub fn toggle(&mut self) {
        self.error = None;
        let shown = self.visible();
        if let Some(e) = shown.get(self.highlight) {
            self.toggle_entry(e);
        }
    }

    /// See [`toggle`](Self::toggle).
    fn toggle_entry(&mut self, e: &Entry) {
        match self.kind {
            ChooseKind::OpenMany if e.is_dir => {
                self.error = Some(self.say(&txt::FILES_SELECT_FOLDER, &[("name", &e.name)]));
            }
            ChooseKind::OpenMany => {
                if !self.selected.remove(&e.path) {
                    self.selected.insert(e.path.clone());
                }
                self.anchor = Some(e.path.clone());
            }
            ChooseKind::Folder => {
                let was = self.selected.contains(&e.path);
                self.selected.clear();
                if !was {
                    self.selected.insert(e.path.clone());
                }
            }
            ChooseKind::Open | ChooseKind::Save => {}
        }
    }

    /// Selects every file shown (choosing several). None to select is a
    /// refusal: the reason is the picker's error, and the result.
    pub fn select_all(&mut self) -> Result<(), String> {
        self.error = None;
        if self.kind != ChooseKind::OpenMany {
            return Err("selecting all is offered when choosing several files".into());
        }
        let files: Vec<PathBuf> = self
            .visible()
            .into_iter()
            .filter(|e| !e.is_dir)
            .map(|e| e.path)
            .collect();
        if files.is_empty() {
            let reason = self.say(&txt::FILES_SELECT_ALL_NONE, &[]);
            self.error = Some(reason.clone());
            return Err(reason);
        }
        self.selected.extend(files);
        Ok(())
    }

    /// Whether a selection exists apart from the highlight: several files, or
    /// the folder to choose, where the list opens with nothing selected.
    fn selects_apart(&self) -> bool {
        matches!(self.kind, ChooseKind::OpenMany | ChooseKind::Folder)
    }

    /// The highlight goes to the `to`th entry shown, from the arrows or Tab:
    /// choosing several, it extends the selection with `extend`; choosing a
    /// folder, or saving, it selects the folder it lands on.
    fn move_highlight(&mut self, to: usize, shown: &[Entry], extend: bool) {
        let Some(e) = shown.get(to) else {
            return;
        };
        self.error = None;
        let from = std::mem::replace(&mut self.highlight, to);
        match self.kind {
            ChooseKind::OpenMany if extend => self.extend_selection(from, to, shown),
            ChooseKind::OpenMany => self.anchor = Some(e.path.clone()),
            ChooseKind::Folder | ChooseKind::Save => {
                self.selected.clear();
                if e.is_dir {
                    self.selected.insert(e.path.clone());
                }
            }
            ChooseKind::Open => {}
        }
    }

    /// The files between the anchor (else `from`) and `to`, as the selection.
    fn extend_selection(&mut self, from: usize, to: usize, shown: &[Entry]) {
        let anchor = self
            .anchor
            .as_ref()
            .and_then(|a| shown.iter().position(|e| &e.path == a))
            .unwrap_or(from);
        self.anchor = shown.get(anchor).map(|e| e.path.clone());
        self.selected = shown[anchor.min(to)..=anchor.max(to)]
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| e.path.clone())
            .collect();
    }

    /// A click on the `n`th entry shown, with these modifiers. It selects; it
    /// does not open.
    fn click_entry(&mut self, n: usize, shown: &[Entry], modifiers: Modifiers) {
        let Some(e) = shown.get(n) else {
            return;
        };
        self.error = None;
        let from = std::mem::replace(&mut self.highlight, n);
        match self.kind {
            ChooseKind::OpenMany if modifiers.shift => self.extend_selection(from, n, shown),
            ChooseKind::Folder | ChooseKind::OpenMany if modifiers.command => self.toggle_entry(e),
            ChooseKind::OpenMany => {
                self.selected.clear();
                if !e.is_dir {
                    self.selected.insert(e.path.clone());
                }
                self.anchor = Some(e.path.clone());
            }
            ChooseKind::Folder => {
                self.selected.clear();
                self.selected.insert(e.path.clone());
            }
            ChooseKind::Save => {
                self.selected.clear();
                if e.is_dir {
                    self.selected.insert(e.path.clone());
                } else {
                    self.text = e.name.clone();
                    self.select_stem = true;
                }
            }
            ChooseKind::Open => {}
        }
    }

    /// The line under the list, and what is announced when it changes: the
    /// count of files selected, or what the folder selected means.
    fn selection_line(&mut self) -> Option<String> {
        match self.kind {
            ChooseKind::OpenMany => {
                let shown = self.visible();
                let total = self.selected.len();
                let hidden = self
                    .selected
                    .iter()
                    .filter(|p| !shown.iter().any(|e| &&e.path == p))
                    .count();
                let (count, not_shown) = (total.to_string(), hidden.to_string());
                let args = [("count", count.as_str()), ("hidden", not_shown.as_str())];
                Some(match (total, hidden) {
                    (0, _) => self.say(&txt::FILES_SELECTED_NONE, &[]),
                    (_, 0) => self.say(&txt::FILES_SELECTED, &args),
                    _ => self.say(&txt::FILES_SELECTED_NOT_SHOWN, &args),
                })
            }
            ChooseKind::Folder => {
                let choice = self.folder_choice();
                let name = choice.file_name().map_or_else(
                    || choice.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                let key = if choice == self.dir {
                    &txt::FILES_FOLDER_SHOWN
                } else {
                    &txt::FILES_FOLDER_SELECTED
                };
                Some(self.say(key, &[("name", &name)]))
            }
            ChooseKind::Open | ChooseKind::Save => None,
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
            "new_folder": self.naming,
            "asking": self.asking,
            "preview": self.preview_state(),
            "pane_width": self.pane_width,
        })
    }

    /// The highlighted entry's preview as agents read it (`null` without
    /// the pane).
    fn preview_state(&mut self) -> Json {
        let Some(previews) = self.previews.clone().filter(|_| self.show_preview) else {
            return Json::Null;
        };
        let Some(e) = self.highlighted() else {
            return Json::Null;
        };
        let mut d = previews.peek(&self.fs, &e.path).describe();
        d["path"] = json!(e.path.display().to_string());
        d
    }

    /// Draws the picker; its answer when given.
    pub fn show(&mut self, ui: &mut Ui, places: &Places) -> Option<PickerAnswer> {
        let t = Tokens::of(ui.ctx());
        self.ctx = Some(ui.ctx().clone());
        let first = !self.focused;
        self.focused = true;
        let mut answer = None;
        // Under its question: drawn, but neither the pointer nor the keys
        // are its own.
        let waiting = self.asking.is_some();
        if waiting {
            ui.disable();
        }
        let field_id = ui.id().with("egui_pop.files.field");
        // The plain keys go to whichever widget Tab took the focus to
        // (the splitter's arrows, a button's Enter); else they are the
        // field's and the list's.
        let elsewhere = !first
            && ui
                .memory(|m| m.focused())
                .is_some_and(|f| f != field_id && !self.row_ids.contains(&f));
        // Keys taken before the field sees them. Tab completes a path;
        // out of a path, it goes to the next widget.
        let empty = self.text.is_empty();
        let path = self.is_path();
        let keys = |i: &mut egui::InputState| {
            // First: egui matches a key asked without Alt with Alt held too,
            // so the plain Up below would take it.
            let alt_up = i.consume_key(Modifiers::ALT, Key::ArrowUp);
            // Shift too, for the same reason.
            let shift_down = !elsewhere && i.consume_key(Modifiers::SHIFT, Key::ArrowDown);
            let shift_up = !elsewhere && i.consume_key(Modifiers::SHIFT, Key::ArrowUp);
            let mut plain = |key| !elsewhere && i.consume_key(Modifiers::NONE, key);
            let (down, up, enter) = (
                shift_down || plain(Key::ArrowDown),
                shift_up || plain(Key::ArrowUp),
                plain(Key::Enter),
            );
            let tab = path && plain(Key::Tab);
            let backspace = empty && plain(Key::Backspace);
            (
                down,
                up,
                shift_down || shift_up,
                enter,
                i.consume_key(Modifiers::COMMAND, Key::Enter),
                tab,
                alt_up || backspace,
                i.consume_key(Modifiers::COMMAND, Key::D),
            )
        };
        let (down, up, extend, enter, accept, tab, parent, bookmark) = if waiting {
            Default::default()
        } else {
            ui.input_mut(keys)
        };
        // Drawn on its own, the picker has no keymap for these.
        let (toggle, select_all) = if waiting || self.declared_keys {
            (false, false)
        } else {
            let (selects, all) = (
                self.selects_apart(),
                self.kind == ChooseKind::OpenMany && !path,
            );
            ui.input_mut(|i| {
                (
                    selects && i.consume_key(Modifiers::COMMAND, Key::Space),
                    all && i.consume_key(Modifiers::COMMAND, Key::A),
                )
            })
        };
        // Alt+P, as the preview pane of the Windows file explorer.
        if !waiting
            && self.previews.is_some()
            && ui.input_mut(|i| i.consume_key(Modifiers::ALT, Key::P))
        {
            self.show_preview = !self.show_preview;
        }
        let preview_on = self.show_preview && self.previews.is_some();
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
            if self.makes_folders() {
                self.new_folder_button(ui);
            }
            ui.monospace(self.dir.display().to_string());
        });
        let mut back_to_field = false;
        if self.naming.is_some() {
            back_to_field = self.naming_row(ui, &mut answer);
        }
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
                .id(field_id)
                .hint_text(strings::tr(ui.ctx(), hint))
                .desired_width(f32::INFINITY),
        );
        // In a path, Tab stays in the field: it completes.
        if self.is_path() {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    field_id,
                    EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
        }
        egui_pop_core::a11y::name(
            &field,
            egui::WidgetType::TextEdit,
            &strings::tr(ui.ctx(), hint),
        );
        let refocus = std::mem::take(&mut self.refocus);
        if first || tab || parent || down || up || enter || back_to_field || refocus {
            field.request_focus();
        }
        // The stem when the field gets the focus, not under a click placing
        // the cursor: typing replaces the name and keeps its extension.
        let clicking = ui.input(|i| i.pointer.any_down() || i.pointer.any_released());
        if self.kind == ChooseKind::Save
            && !waiting
            && (self.select_stem || (field.gained_focus() && !clicking))
        {
            self.select_stem = false;
            let (start, end) = stem_chars(&self.text);
            let mut state = egui::TextEdit::load_state(ui.ctx(), field.id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(start),
                    egui::text::CCursor::new(end),
                )));
            state.store(ui.ctx(), field.id);
        }
        if field.changed() {
            self.highlight = 0;
            self.error = None;
            // Typing says what is meant now: a name, or a filter's first match.
            // Files selected stay, and the line under the list counts them.
            if self.kind != ChooseKind::OpenMany {
                self.selected.clear();
            }
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
        if down || up {
            // Choosing a folder the list opens with nothing selected: the first
            // press selects the entry highlighted, rather than the one after it.
            let to = if self.kind == ChooseKind::Folder && self.selected.is_empty() {
                self.highlight
            } else if down {
                (self.highlight + 1).min(shown.len().saturating_sub(1))
            } else {
                self.highlight.saturating_sub(1)
            };
            self.move_highlight(to, &shown, extend);
        }
        ui.add_space(t.space.s);
        let list_height = if self.fill {
            // What comes below the list keeps its room: its height as
            // last drawn, or the row of buttons and the space above it.
            let footer = self.footer_height.unwrap_or_else(|| {
                ui.spacing().interact_size.y + t.space.m + ui.spacing().item_spacing.y
            });
            (ui.available_height() - footer).max(ui.spacing().interact_size.y)
        } else {
            LIST_HEIGHT
        };
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
            // The gap: the space added below, and the row's own spacing
            // after the list.
            let gap = t.space.m + ui.spacing().item_spacing.x;
            let room = ui.available_width();
            let (list_width, pane_width) = split_list(room, gap, preview_on, self.pane_width);
            self.pane_dragging = false;
            if let Some(mut width) = pane_width {
                // Laid out before the entries, for Tab to reach it
                // without going through them all.
                let at = ui.cursor().min;
                let rect = egui::Rect::from_min_size(
                    pos2(at.x + list_width, at.y),
                    vec2(gap, list_height),
                );
                let name = strings::tr(ui.ctx(), &txt::FILES_PREVIEW_RESIZE);
                let id = ui.id().with("egui_pop.files.splitter");
                let max = room - gap - LIST_MIN_WIDTH;
                let r = widgets::Splitter::new(id, &name, PANE_MIN_WIDTH, max, preview::PANE_WIDTH)
                    .after()
                    .show(ui, rect, &mut width);
                if r.changed() {
                    self.pane_width = width;
                }
                self.pane_dragging = r.dragged();
            }
            ui.vertical(|ui| {
                ui.set_width(list_width);
                egui::ScrollArea::vertical()
                    .id_salt("egui_pop.files.entries")
                    .max_height(list_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // One entry per row (the row beside the places is
                        // laid out left to right).
                        ui.set_width(ui.available_width());
                        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                            let apart = self.selects_apart();
                            let pointer =
                                ui.input(|i| i.pointer.any_down() || i.pointer.any_released());
                            let modifiers = ui.input(|i| i.modifiers);
                            self.row_ids.clear();
                            for (n, e) in shown.iter().enumerate() {
                                let label = if e.is_dir {
                                    format!("{}/", e.name)
                                } else {
                                    e.name.clone()
                                };
                                // Where a selection exists apart from the
                                // highlight, a ring says where the highlight is.
                                let selected = self.selected.contains(&e.path);
                                let highlighted = n == self.highlight;
                                let on = selected || (highlighted && !apart);
                                let r = widgets::selectable_label(ui, on, label);
                                if apart && highlighted {
                                    ui.painter().rect_stroke(
                                        r.rect,
                                        t.radius.widget,
                                        egui::Stroke::new(1.0, t.focus_ring),
                                        egui::StrokeKind::Inside,
                                    );
                                }
                                self.row_ids.push(r.id);
                                // Tab through the rows moves the highlight too.
                                let tabbed = r.gained_focus() && !pointer;
                                if tabbed {
                                    self.move_highlight(n, &shown, false);
                                }
                                if n == self.highlight && (down || up || tabbed) {
                                    r.scroll_to_me(None);
                                }
                                if r.double_clicked() && !modifiers.command && !modifiers.shift {
                                    self.click_entry(n, &shown, Modifiers::NONE);
                                    answer = answer.take().or_else(|| self.activate());
                                } else if r.clicked() {
                                    self.click_entry(n, &shown, modifiers);
                                    // By the pointer: the keys go on as before.
                                    self.refocus |= pointer;
                                }
                            }
                            if shown.is_empty() {
                                ui.weak(strings::tr(ui.ctx(), &txt::NOTHING_MATCHES));
                            }
                        });
                    });
            });
            if let Some(pane_width) = pane_width {
                ui.add_space(t.space.m);
                ui.vertical(|ui| {
                    ui.set_width(pane_width);
                    // As high as the list, whatever the preview's facts.
                    egui::ScrollArea::vertical()
                        .id_salt("egui_pop.files.pane")
                        .max_height(list_height)
                        .auto_shrink([false, true])
                        .show(ui, |ui| self.preview_pane(ui));
                });
            }
        });
        if toggle {
            self.toggle();
        }
        if select_all {
            // A refusal is the picker's error, shown below the list.
            let _ = self.select_all();
        }
        if enter && answer.is_none() {
            answer = self.activate();
        }
        if accept && answer.is_none() {
            answer = self.accept();
        }
        let footer_top = ui.cursor().min.y;
        if let Some(e) = &self.error {
            ui.colored_label(t.status.danger, e);
        }
        let line = self.selection_line();
        if let Some(line) = &line {
            ui.label(line);
        }
        if self.said != line {
            // Not what it says on opening: that is no change.
            if let (Some(_), Some(line)) = (&self.said, &line) {
                a11y::announce(ui.ctx(), line, Politeness::Polite);
            }
            self.said = line;
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
                let list = ui.combo_box(combo, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        widgets::selectable_value(ui, &mut at, i, n);
                    }
                });
                // No label beside it: a screen reader still needs its name.
                let name = strings::tr(ui.ctx(), &txt::FILES_TYPE);
                ui.ctx()
                    .accesskit_node_builder(list.response.id, |node| node.set_label(name));
                if at != self.filter_index {
                    self.set_filter(at);
                }
            }
            widgets::checkbox(
                ui,
                &mut self.show_hidden,
                strings::tr(ui.ctx(), &txt::FILES_HIDDEN),
            );
            if self.previews.is_some() {
                widgets::checkbox(
                    ui,
                    &mut self.show_preview,
                    strings::tr(ui.ctx(), &txt::FILES_PREVIEW),
                )
                .on_hover_text("Alt+P");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let verb = match self.kind {
                    ChooseKind::Open | ChooseKind::OpenMany => &txt::FILES_OPEN,
                    // What it does now: it goes into the folder selected.
                    ChooseKind::Save if !self.selected.is_empty() => &txt::FILES_OPEN,
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
        let footer = ui.cursor().min.y - footer_top;
        if self.footer_height != Some(footer) {
            self.footer_height = Some(footer);
            ui.ctx().request_repaint();
        }
        answer
    }
}

impl FilePicker {
    /// The New folder button: disabled, with the reason, on a file system
    /// that cannot make one (the key says the same in the picker).
    fn new_folder_button(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let can = self.fs.can_create_dirs();
        let button = egui::Button::new(strings::tr(&ctx, &txt::FILES_NEW_FOLDER));
        let r = ui.add_enabled(can, button);
        let r = if can {
            match self.new_folder_keys.first() {
                Some(keys) => r.on_hover_text(keys),
                None => r,
            }
        } else {
            r.on_disabled_hover_text(strings::tr(&ctx, &txt::FILES_FOLDER_READ_ONLY))
        };
        if r.clicked() {
            // A refusal is the picker's error, shown below the list.
            let _ = self.begin_new_folder();
        }
    }

    /// The row asking a new folder's name. Says whether the focus goes
    /// back to the picker's field: the row closed.
    fn naming_row(&mut self, ui: &mut Ui, answer: &mut Option<PickerAnswer>) -> bool {
        let ctx = ui.ctx().clone();
        let id = ui.id().with("egui_pop.files.new_folder");
        // Escape takes the focus from a field before anything else sees
        // it; from the row's buttons, it comes this way.
        let mut close = focus::escape_left(&ctx, id)
            || ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        let focused = ui.memory(|m| m.has_focus(id));
        let mut create = focused && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
        let mut name = self.naming.take().unwrap_or_default();
        let label = strings::tr(&ctx, &txt::FILES_FOLDER_NAME);
        ui.horizontal(|ui| {
            ui.label(&label);
            let edit = ui.add(
                egui::TextEdit::singleline(&mut name)
                    .id(id)
                    .desired_width(ui.available_width() / 2.0),
            );
            egui_pop_core::a11y::name(&edit, egui::WidgetType::TextEdit, &label);
            if std::mem::take(&mut self.naming_focus) {
                edit.request_focus();
            }
            if edit.changed() {
                self.error = None;
            }
            create |= ui.button(strings::tr(&ctx, &txt::FILES_CREATE)).clicked();
            close |= ui.button(strings::tr(&ctx, &CANCEL)).clicked();
        });
        if close {
            self.error = None;
            return true;
        }
        self.naming = Some(name.clone());
        if !create {
            return false;
        }
        match self.create_folder(&name) {
            Ok(path) => {
                *answer = Some(PickerAnswer::FolderCreated(path));
                true
            }
            Err(_) => {
                // The reason shows below the list; the name stays to fix.
                self.naming_focus = true;
                ctx.request_repaint();
                false
            }
        }
    }

    /// The pane beside the list: the highlighted entry's preview, asked
    /// of the providers (the one asked before is cancelled).
    fn preview_pane(&mut self, ui: &mut Ui) {
        let Some(previews) = self.previews.clone() else {
            return;
        };
        match self.highlighted() {
            Some(e) => {
                let shown = previews.get(ui.ctx(), &self.fs, &e.path);
                let meta = self.fs.metadata(&e.path).ok();
                preview::pane(ui, &e.name, meta.as_ref(), &shown);
            }
            None => {
                ui.weak(strings::tr(ui.ctx(), &txt::FILES_PREVIEW_NONE));
            }
        }
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
        start: Option<PathBuf>,
        answer: Arc<Mutex<Option<Answer>>>,
    ) {
        use std::future::Future;
        use std::pin::Pin;

        use super::ChooseKind;

        let mut d = rfd::AsyncFileDialog::new().set_title(&request.title);
        if let Some(dir) = start {
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
        _start: Option<PathBuf>,
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
    fn the_preview_pane_takes_the_list_s_width_and_shrinks_first() {
        // Off: all the room is the list's.
        let wanted = preview::PANE_WIDTH;
        assert_eq!(split_list(700.0, 8.0, false, wanted), (700.0, None));
        // Room for both: the full pane, from the list's width.
        let (list, pane) = split_list(700.0, 8.0, true, wanted);
        assert_eq!(pane, Some(preview::PANE_WIDTH));
        assert_eq!(list, 700.0 - 8.0 - preview::PANE_WIDTH);
        // Less room: the pane shrinks, the list keeps its minimum.
        let (list, pane) = split_list(LIST_MIN_WIDTH + 8.0 + 150.0, 8.0, true, wanted);
        assert_eq!((list, pane), (LIST_MIN_WIDTH, Some(150.0)));
        // Too little: no pane at all.
        assert_eq!(split_list(300.0, 8.0, true, wanted), (300.0, None));
        // The width the person gave, within the limits.
        let (list, pane) = split_list(700.0, 8.0, true, 300.0);
        assert_eq!((list, pane), (392.0, Some(300.0)));
        assert_eq!(split_list(700.0, 8.0, true, 10.0).1, Some(PANE_MIN_WIDTH));
        assert_eq!(split_list(700.0, 8.0, true, 900.0).0, LIST_MIN_WIDTH);
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
    fn the_picker_reads_a_memory_folder() {
        let root = PathBuf::from("/demo");
        let fs: SharedFs = Arc::new(
            fs::MemoryFs::new()
                .file(root.join("Photos/a.png"), b"x".to_vec())
                .file(root.join("notes.txt"), b"y".to_vec()),
        );
        let mut p = FilePicker::new_in(&FileRequest::open("Open").directory(&root), fs);
        assert_eq!(p.dir, root);
        let names =
            |p: &mut FilePicker| p.visible().into_iter().map(|e| e.name).collect::<Vec<_>>();
        assert_eq!(names(&mut p), vec!["Photos", "notes.txt"]);
        // Enter goes into the folder, then chooses its file.
        assert_eq!(p.activate(), None);
        assert_eq!(p.dir, root.join("Photos"));
        assert_eq!(
            p.activate(),
            Some(PickerAnswer::Chosen(vec![root.join("Photos/a.png")]))
        );
    }

    /// A provider that names what it was asked for, at the size asked.
    #[derive(Debug)]
    struct Fake;

    impl preview::PreviewProvider for Fake {
        fn id(&self) -> &str {
            "fake"
        }
        fn handles(&self) -> preview::Handles {
            preview::Handles::new().extensions(&["fake"]).folders()
        }
        fn priority(&self) -> i32 {
            50
        }
        fn preview(
            &self,
            request: &preview::PreviewRequest,
        ) -> Result<Option<preview::Preview>, String> {
            if request.meta.is_dir {
                return Err("folders are not faked".into());
            }
            Ok(Some(
                preview::Preview::new(preview::Body::Empty)
                    .dimensions(request.max_side, 7)
                    .pages(3)
                    .fact("Name", request.path.display().to_string()),
            ))
        }
    }

    #[test]
    fn agents_read_a_preview_and_the_picker_says_it() {
        let root = PathBuf::from("/demo");
        let fs: SharedFs = Arc::new(
            fs::MemoryFs::new()
                .file_at(
                    root.join("a.fake"),
                    b"x".to_vec(),
                    Some(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(60)),
                )
                .dir(root.join("sub")),
        );
        let mut app = crate::App::builder()
            .plugin(FilesPlugin::in_app().file_system(fs.clone()))
            .build(&egui::Context::default())
            .unwrap();
        let files = app.handle().service(&FILES).unwrap();
        files.previews().register(Arc::new(Fake));
        assert_eq!(files.previews().providers()[0], ("fake".into(), 50));
        let out = app
            .run(
                &CMD_PREVIEW,
                json!({ "path": "/demo/a.fake" }),
                Origin::Agent("test".into()),
            )
            .now()
            .unwrap()
            .unwrap();
        assert_eq!(out["state"], "ready");
        assert_eq!(out["provider"], "fake");
        assert_eq!(out["dimensions"], json!([preview::MAX_SIDE, 7]));
        assert_eq!(out["pages"], 3);
        assert_eq!(out["size"], 1);
        assert_eq!(out["modified"], 60);
        assert_eq!(out["modified_text"], "1970-01-01 00:01 UTC");
        // A failure is a reason.
        let out = preview_metadata(&files, &root.join("sub")).unwrap();
        assert_eq!(out["state"], "failed");
        assert_eq!(out["reason"], "folders are not faked");
        assert!(preview_metadata(&files, &root.join("missing")).is_err());
        // The picker's pane asks the same providers; the bridge says what
        // it shows.
        let mut p = FilePicker::new_in(&FileRequest::open("Open").directory(&root), fs)
            .with_previews(files.previews());
        assert_eq!(p.highlighted().unwrap().name, "sub");
        p.highlight = 1;
        let d = p.describe();
        assert_eq!(d["preview"]["state"], "ready", "{d}");
        assert_eq!(d["preview"]["path"], "/demo/a.fake");
        p.show_preview = false;
        assert!(p.describe()["preview"].is_null());
    }

    #[test]
    fn a_name_s_extension_is_read_added_and_its_stem_found() {
        assert_eq!(extension_dot("a.png"), Some(1));
        assert_eq!(extension_dot("a.tar.gz"), Some(5));
        assert_eq!(extension_dot("/x.y/photo"), None);
        assert_eq!(extension_dot(".config"), None);
        assert_eq!(extension_dot("photo."), None);
        assert_eq!(extension_dot("Mr. Smith"), None);
        assert_eq!(with_extension("a", "png"), "a.png");
        assert_eq!(with_extension("a.", "png"), "a.png");
        assert_eq!(with_extension("Mr. Smith", "png"), "Mr. Smith.png");
        assert_eq!(with_extension("a.jpg", "png"), "a.jpg");
        assert_eq!(with_extension(".config", "png"), ".config");
        assert_eq!(with_extension("/x.y/a", "png"), "/x.y/a.png");
        assert_eq!(stem_chars("été.png"), (0, 3));
        assert_eq!(stem_chars("/é/a.png"), (3, 4));
        assert_eq!(stem_chars("a.tar.gz"), (0, 5));
        assert_eq!(stem_chars(".config"), (0, 7));
        assert_eq!(stem_chars(""), (0, 0));
    }

    #[test]
    fn a_filter_changes_only_the_extensions_filters_name() {
        let fs: SharedFs = Arc::new(fs::MemoryFs::new().dir("/demo"));
        let request = FileRequest::save("Save", "a.png")
            .directory("/demo")
            .filter("PNG", &["png"])
            .filter("Images", &["jpg", "png"])
            .filter("Any", &[]);
        let mut p = FilePicker::new_in(&request, fs);
        // The new filter accepts it too.
        p.set_filter(1);
        assert_eq!(p.text, "a.png");
        p.set_filter(0);
        p.text = "a.jpg".into();
        p.set_filter(1);
        assert_eq!(p.text, "a.jpg");
        p.set_filter(0);
        assert_eq!(p.text, "a.png");
        // No extension to follow with: a filter without any, or all files.
        p.set_filter(2);
        assert_eq!(p.text, "a.png");
        p.set_filter(3);
        assert_eq!(p.text, "a.png");
        // Back from all files, an extension a filter names follows.
        p.text = "a.jpg".into();
        p.set_filter(0);
        assert_eq!(p.text, "a.png");
        // One no filter names is the person's; no extension waits for Enter.
        p.text = "a.tiff".into();
        p.set_filter(1);
        assert_eq!(p.text, "a.tiff");
        p.text = "a".into();
        p.set_filter(0);
        assert_eq!(p.text, "a");
        // Not when opening: the field filters there.
        let mut p = FilePicker::new_in(
            &FileRequest::open("Open")
                .directory("/demo")
                .filter("PNG", &["png"])
                .filter("JPEG", &["jpg"]),
            Arc::new(fs::MemoryFs::new().dir("/demo")),
        );
        p.text = "a.png".into();
        p.set_filter(1);
        assert_eq!(p.text, "a.png");
    }

    #[test]
    fn a_typed_folder_is_entered_and_the_proposed_name_comes_back() {
        let root = tree("save-folder");
        let mut p = FilePicker::new(&FileRequest::save("Save", "new.png").directory(&root));
        p.text = "photos".into();
        assert_eq!(p.activate(), None);
        assert_eq!(p.dir, root.join("photos"));
        assert_eq!(p.text, "new.png");
        // A missing folder is said, rather than made into a file's name.
        p.text = "nowhere/".into();
        assert_eq!(p.activate(), None);
        assert!(p.error.is_some());
        p.text = "nowhere/x.png".into();
        assert_eq!(p.activate(), None);
        assert!(p.error.is_some());
        // On its own, the picker hands the question to its caller.
        p.text = "a.png".into();
        assert_eq!(
            p.activate(),
            Some(PickerAnswer::Replace(root.join("photos/a.png")))
        );
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
