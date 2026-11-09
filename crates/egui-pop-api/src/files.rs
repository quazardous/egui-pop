//! Choosing files, as a plugin ([`FilesPlugin`], in `DefaultPlugins`):
//! egui-pop's in-app picker, or, as a fallback an application allows, the
//! system's chooser off the interface thread.
//!
//! [`Files::choose`] returns a [`Pending`] (the host contract's, ADR
//! 0010): the frame never waits. In-app pickers open in a modal of their
//! own, or inside the caller's dialog ([`Files::choose_inline`], as a
//! form's path field does). The result is `{ "paths": [...] }`, or
//! `null` when the person cancelled; [`chosen`] reads it.
//!
//! - **The system's chooser** (feature `native-dialogs`, through `rfd`):
//!   a fallback kept in case an application cannot do with the in-app
//!   picker, off at build time and at run time. Used only when the feature
//!   is compiled in *and* the application allows it
//!   (`AppBuilder::system_file_chooser`) *and* the person's `files.native`
//!   setting is on *and* a portal answers; never under tests
//!   ([`system_chooser_refused`]). The xdg desktop portal on Linux, the
//!   common dialogs on Windows, the open and save panels on macOS. It is
//!   created on the interface thread and awaited on a thread of its own.
//! - **The in-app picker** ([`FilePicker`]), in every other case: a
//!   click selects and a double click or Enter opens, keyboard navigation
//!   (arrows, Enter, Backspace or `Alt+Up` for the parent folder,
//!   `Ctrl+Enter` to choose the folder or the selection,
//!   `Ctrl+D` to bookmark), several files selected with `Ctrl` or `Shift`
//!   and a click, `Shift` and the arrows, `Ctrl+Space` and `Ctrl+A`
//!   ([`CMD_TOGGLE_SELECTION`], [`CMD_SELECT_ALL`]), a typed path with
//!   completion (`Tab`), the fuzzy filter of the choice dialog,
//!   the request's filters, the system's folders and volumes ([`places`]),
//!   recent places and bookmarks (renamed and moved from their menu) kept in
//!   the host store (`settings/files.recent`, `settings/files.bookmarks`), and the
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
//! - **The in-app picker's order**: a click on a column's header sorts by it
//!   ([`sort`]), in the list and the grid alike; it is kept in the host store
//!   ([`SORT`]), and [`SETTING_FOLDERS_FIRST`] keeps folders first.
//! - **The in-app picker's view**, with previews: a list, or a grid of
//!   thumbnails from the same providers ([`PickerView`]), whose size is a
//!   slider, `Ctrl` and the wheel, or `Ctrl` and `+` or `-`; only the cells
//!   in view ask for a thumbnail ([`Previews::want`]), a placeholder stands
//!   in until it is made, and the arrows move in two dimensions. Both are
//!   kept in the host store ([`VIEW`], [`THUMBNAIL_SIZE`]).
//! - **The in-app picker's folder** reads as breadcrumbs: each segment opens its
//!   folder, the chevron after it lists its subfolders, and with too little room
//!   the start collapses behind a menu, the deepest segments staying.
//!   [`CMD_EDIT_PATH`] (Ctrl+L), or typing `/` or `~` on an entry, puts a path
//!   into the field, which takes both.
//! - **Grouped files** ([`groups`]): with a grouping declared by the application
//!   ([`FilesPlugin::grouping`], [`Files::groupings`]), files that go together are
//!   one entry of the in-app picker, a RAW with its sidecars, with a badge
//!   (`+XMP`). Choosing it gives the main file, and [`chosen_groups`] the others.
//! - **The application's options** ([`FileRequest::choice`],
//!   [`FileRequest::check`]): combo boxes and check boxes above the in-app
//!   picker's buttons, in the desktop portal's shape; the choice gives their
//!   values back ([`chosen_choices`]). `rfd` cannot show them, so a request
//!   with choices opens the in-app picker.
//!
//! Steps (ADR 0011):
//!
//! | Step | Arguments | Default |
//! |---|---|---|
//! | `file.choose` | `{ "id", "request" }` | opens the system's chooser or the in-app picker |
//! | `file.chosen` | `{ "id", "request", "paths", "origin" }`, `"groups"` when some are, `"choices"` when the request has some | remembers the places, gives the paths, their groups and the choices |
//!
//! A run-hook on `file.choose` replaces the whole chooser: it answers
//! `{ "paths": [...] }` or `{ "cancelled": true }` at once, or anything
//! else and later [`Files::deliver`]. A before-hook on `file.chosen` may
//! veto the choice (the result is the reason, as an error) or redirect
//! it to other paths. The journal says `file.cancelled` when the choice
//! is cancelled, from whoever cancelled it. The debug bridge's `files`
//! collector shows the choices open and answers them (`answer`, the
//! user's unless offered to agents), as its caller: the result carries
//! the origin ([`chosen_origin`]), and so does `file.chosen` (`origin`,
//! as the journal writes it), so a hook may refuse a choice no person
//! made. The `file.preview`
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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use egui::{AtomExt as _, EventFilter, Key, Modifiers, Sense, Ui, pos2, vec2};
use egui_pop_core::a11y::{self, Politeness};
use egui_pop_core::debug::FnCollector;
use egui_pop_core::focus;
use egui_pop_core::icon::Icon;
use egui_pop_core::indicator::{Anchor, Indicator};
use egui_pop_core::kit::{Kit, Part, State as PartState, Tone};
use egui_pop_core::lock;
use egui_pop_core::overlay::{self, Kind, OverlaySpec};
use egui_pop_core::placeholder::SkeletonTokens;
use egui_pop_core::slider::Slider;
use egui_pop_core::strings::{self, CANCEL};
use egui_pop_core::theme::Tokens;
use egui_pop_core::thumbnail::{self, Thumbnail};
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

use self::sort::{PickerSort, SortColumn};

pub mod browser;
pub mod fs;
pub mod groups;
pub mod places;
pub mod preview;
pub mod providers;
pub mod sort;

use browser::ViewMode as _;
use fs::{Meta, SharedFs};
use groups::{FileGroup, FileGrouping, Groupings};
use places::SystemPlaces;
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
/// The open in-app picker's folder goes into its field, as a path to edit
/// (Ctrl+L). Saving, the name stays at its end.
pub const CMD_EDIT_PATH: CommandId = CommandId::from_static("file.edit_path");
/// The open in-app picker shows or hides hidden files (Ctrl+H, as GTK and KDE).
pub const CMD_TOGGLE_HIDDEN: CommandId = CommandId::from_static("file.toggle_hidden");
/// The open in-app picker reads its folder again (F5).
pub const CMD_RELOAD: CommandId = CommandId::from_static("file.reload");
/// The open in-app picker goes back to the folder it showed before (Alt+Left).
pub const CMD_BACK: CommandId = CommandId::from_static("file.back");
/// The open in-app picker goes forward again after going back (Alt+Right).
pub const CMD_FORWARD: CommandId = CommandId::from_static("file.forward");
/// The in-app picker's preview pane is shown or hidden (Alt+P, as the Windows
/// file explorer's), and kept so.
pub const CMD_TOGGLE_PREVIEW: CommandId = CommandId::from_static("file.toggle_preview");

/// The context key: an open in-app picker can be asked for a new folder
/// (saving, or choosing a folder).
const WHEN_NEW_FOLDER: &str = "filePickerNewFolder";
/// The context key: an open in-app picker selects all its files on request.
const WHEN_SELECT_ALL: &str = "filePickerSelectAll";
/// The context key: an open in-app picker selects apart from its highlight.
const WHEN_SELECTS: &str = "filePickerSelects";
/// The context key: an in-app picker is open, and not waiting for an answer.
const WHEN_EDIT_PATH: &str = "filePickerEditPath";
/// That, in a modal (as the picker's own and a dialog are): where `Alt+Left` is
/// not the context stack's back (`nav.back`).
const WHEN_IN_MODAL: &str = "modalOpen && filePickerEditPath";
/// A file browser holds the keyboard (an in-app picker with previews open,
/// or a [`browser::FileBrowser`]'s entries): the mode keys work.
pub const WHEN_BROWSER: &str = "fileBrowserFocused";
/// The prefix of the mode keys' commands: `file.view.mode1` (`Ctrl+1`) to
/// `file.view.mode9`.
pub const CMD_VIEW_MODE: &str = "file.view.mode";

/// The setting: use the system's chooser when there is one. Declared only
/// when the application allows that chooser
/// (`AppBuilder::system_file_chooser`); on by default there.
pub const SETTING_NATIVE: SettingId = SettingId::from_static("files.native");
/// The setting: the in-app picker shows its preview pane.
pub const SETTING_PREVIEW: SettingId = SettingId::from_static("files.preview");
/// Whether the in-app picker keeps folders before files in every order.
pub const SETTING_FOLDERS_FIRST: SettingId = SettingId::from_static("files.folders_first");

/// Where recent places are kept (a JSON list of folders, newest first).
pub const RECENT: &str = "settings/files.recent";
/// Where bookmarks are kept (a JSON list of folders).
pub const BOOKMARKS: &str = "settings/files.bookmarks";
/// Where the names given to bookmarks are kept (a JSON list of
/// `[folder, name]` pairs). See [`rename_bookmark`].
pub const BOOKMARK_NAMES: &str = "settings/files.bookmark_names";

/// Where the in-app picker's size is kept (width and height of its
/// content, in points).
pub const DIALOG_SIZE: &str = "settings/files.dialog_size";
/// Where the width of the in-app picker's preview pane is kept.
pub const PANE_WIDTH: &str = "settings/files.pane_width";

/// Where the in-app picker's view last used is kept: a mode id of
/// [`browser::ViewModes`] (`details`, `thumbnails`); a dialog never seen
/// opens in it.
pub const VIEW: &str = "settings/files.view";
/// The mode each dialog was last shown in, by dialog (a request's purpose,
/// else its kind): a JSON list of pairs ([`view_for`]).
pub const VIEWS: &str = "settings/files.views";
/// How many dialogs' modes are kept.
pub const VIEWS_KEPT: usize = 32;
/// Where the size of the in-app picker's thumbnails is kept, in points.
pub const THUMBNAIL_SIZE: &str = "settings/files.thumbnail_size";
/// Where the in-app picker's order is kept: `name`, `-name` (descending), and
/// `size`, `modified` or `type` the same way ([`sort::PickerSort`]).
pub const SORT: &str = "settings/files.sort";
/// Where it is kept whether the in-app picker groups files, with groupings
/// declared ([`groups`]). Not a setting of the settings window: without a
/// grouping it would do nothing.
pub const GROUPED: &str = "settings/files.grouped";

/// Where the last folder of each purpose is kept (a JSON list of
/// `[purpose, folder]` pairs, newest first). See [`FileRequest::purpose`].
pub const LAST_FOLDERS: &str = "settings/files.last_folders";

/// How many folders the in-app picker goes back through.
const HISTORY_KEPT: usize = 64;
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

/// An option of the application's, shown by the in-app picker beside its
/// buttons and given back with the paths ([`chosen_choices`]): a combo box of
/// values, or a check box. Its shape is the desktop portal's `choices`: an id,
/// a label, the values offered as `(id, label)`, and the value it starts with;
/// no values make a check box, whose value is `"true"` or `"false"`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChoice {
    pub id: String,
    /// What the picker says beside it, translated by the application.
    pub label: String,
    /// The values offered, `(id, label)`; none: a check box.
    #[serde(default)]
    pub options: Vec<(String, String)>,
    /// The value it starts with: an option's id, or `"true"` or `"false"`.
    pub default: String,
}

impl FileChoice {
    /// A check box.
    pub fn is_check(&self) -> bool {
        self.options.is_empty()
    }

    /// Whether `value` is one this choice can take.
    pub fn accepts(&self, value: &str) -> bool {
        if self.is_check() {
            value == "true" || value == "false"
        } else {
            self.options.iter().any(|(id, _)| id == value)
        }
    }
}

/// The application's choices as a choice ends with them: each id with its
/// value ([`chosen_choices`]).
pub type ChoiceValues = BTreeMap<String, String>;

/// What a choice asks for. Built with its constructors ([`FileRequest::open`]
/// and the others) and their builders: a field added later breaks nobody.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
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
    /// Opening files, the application's groupings apply (on by default). See
    /// [`FileRequest::ungrouped`].
    #[serde(default = "asks")]
    pub group_files: bool,
    /// The application's options, shown beside the in-app picker's buttons.
    /// See [`FileRequest::choice`].
    #[serde(default)]
    pub choices: Vec<FileChoice>,
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
            group_files: true,
            choices: Vec::new(),
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

    /// Every file is an entry of its own and a choice gives no groups, whatever
    /// the application's groupings ([`groups`]): for a choice that is after a
    /// sidecar as much as a RAW. Saving and choosing a folder never group: a name
    /// to save under must see every file it could replace.
    pub fn ungrouped(mut self) -> Self {
        self.group_files = false;
        self
    }

    /// An option of the application's, a combo box offering `options`
    /// (`(id, label)`) and starting on `default`: the in-app picker shows it
    /// beside its buttons, and the choice gives back the value picked
    /// ([`chosen_choices`]), as Windows' customize interface, macOS's accessory
    /// view and the desktop portal's `choices` do; an import's "Only new
    /// photos", an export's format. A request with choices opens the in-app
    /// picker even where the system's chooser is used: `rfd` cannot show them.
    pub fn choice(
        mut self,
        id: &str,
        label: &str,
        options: &[(&str, &str)],
        default: &str,
    ) -> Self {
        self.choices.push(FileChoice {
            id: id.into(),
            label: label.into(),
            options: options
                .iter()
                .map(|(id, label)| ((*id).into(), (*label).into()))
                .collect(),
            default: default.into(),
        });
        self
    }

    /// An option of the application's as a check box, on or off at first; its
    /// value is `"true"` or `"false"`. See [`FileRequest::choice`].
    pub fn check(mut self, id: &str, label: &str, default: bool) -> Self {
        self.choices.push(FileChoice {
            id: id.into(),
            label: label.into(),
            options: Vec::new(),
            default: default.to_string(),
        });
        self
    }

    /// Each choice with the value it starts with.
    pub fn choice_defaults(&self) -> ChoiceValues {
        self.choices
            .iter()
            .map(|c| (c.id.clone(), c.default.clone()))
            .collect()
    }

    /// `values` over the defaults, each checked: an unknown id or a value its
    /// choice cannot take is refused.
    pub fn choice_values(&self, values: &ChoiceValues) -> Result<ChoiceValues, String> {
        let mut out = self.choice_defaults();
        for (id, value) in values {
            let c = self
                .choices
                .iter()
                .find(|c| &c.id == id)
                .ok_or_else(|| format!("no choice `{id}` in this request"))?;
            if !c.accepts(value) {
                return Err(format!("`{value}` is not a value of the choice `{id}`"));
            }
            out.insert(id.clone(), value.clone());
        }
        Ok(out)
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

/// Who answered a choice, from its result: the user (the picker, the
/// system's chooser, a plugin's) unless an agent or a script answered it
/// through the debug bridge. A consumer that should act only on a
/// person's choice checks it.
pub fn chosen_origin(result: &Result<Json, String>) -> Origin {
    result
        .as_ref()
        .ok()
        .and_then(|j| j.get("origin"))
        .and_then(|o| serde_json::from_value(o.clone()).ok())
        .unwrap_or(Origin::User)
}

/// The groups a choice's result holds ([`groups`]): one for each path chosen
/// that heads a group, none otherwise. [`chosen`] gives the paths, each group's
/// main among them; the members come from here.
pub fn chosen_groups(result: &Result<Json, String>) -> Vec<FileGroup> {
    result
        .as_ref()
        .map_or_else(|_| Vec::new(), groups::from_json)
}

/// The application's choices a choice ended with ([`FileRequest::choice`]):
/// each id with its value, empty when cancelled or without choices.
pub fn chosen_choices(result: &Result<Json, String>) -> ChoiceValues {
    result
        .as_ref()
        .ok()
        .and_then(|j| j.get("choices"))
        .map(choices_of)
        .unwrap_or_default()
}

fn choices_of(json: &Json) -> ChoiceValues {
    json.as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default()
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
    /// Who answered it: the user (the picker, a chooser) unless
    /// [`Files::deliver_as`] says otherwise.
    by: Origin,
    /// The application's choices as whoever answered set them
    /// ([`Files::set_choices`]), over the in-app picker's.
    choices: Option<ChoiceValues>,
}

#[derive(Debug, Default)]
struct State {
    next: u64,
    open: Vec<Open>,
    /// The last choices: id, request title, outcome.
    last: VecDeque<Json>,
    /// The system's chooser is used when available (the setting).
    native: bool,
    /// The application allows the system's chooser
    /// (`AppBuilder::system_file_chooser`).
    allowed: bool,
    /// Why the system's chooser is refused, as of the last frame.
    refused: Option<&'static str>,
    /// Where the in-app picker reads (none: the system's file system).
    fs: Option<SharedFs>,
    previews: Previews,
    groupings: Groupings,
    /// The system's places, as read when a picker last opened.
    system: Option<SystemPlaces>,
    /// Where they come from (none: the system, [`places::system_places`]).
    system_source: Option<PlaceSource>,
}

/// Where the system's places come from ([`Files::set_system_places`]).
#[derive(Clone)]
struct PlaceSource(Arc<dyn Fn() -> SystemPlaces + Send + Sync>);

impl std::fmt::Debug for PlaceSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PlaceSource")
    }
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

/// The environment variable that refuses the system's chooser to a process,
/// whatever its value: for test runs that drive an application without
/// `egui_pop_test` (egui-pop's CI sets it).
pub const TEST_ENV: &str = "EGUI_POP_TEST";

/// Refused in this process ([`refuse_system_chooser`]).
static REFUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Refuses the system's chooser in this process from now on, whatever the
/// feature, the application and the setting say: the in-app picker answers
/// every choice. One way. `egui_pop_test`'s harnesses call it, so that an
/// application's own tests never open a dialog on the desktop.
pub fn refuse_system_chooser() {
    REFUSED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Whether the system's chooser is refused here, whatever the feature, the
/// application and the setting say: under tests, or with no window.
/// [`system_chooser_refusal`] says why.
pub fn system_chooser_refused(ctx: &egui::Context) -> bool {
    system_chooser_refusal(ctx).is_some()
}

/// Why the system's chooser is refused here, the first reason found, or
/// `None`:
///
/// - `"cfg(test)"`: this crate's own unit tests;
/// - `"test harness"`: [`refuse_system_chooser`] was called (each of
///   `egui_pop_test`'s harnesses does);
/// - `"EGUI_POP_TEST"`: [`TEST_ENV`] is set;
/// - `"nextest"`: the process is a test of cargo-nextest (`NEXTEST`);
/// - `"no window"`: the frames come with no window's focus, as with
///   egui_kittest or a bare `Context::run`. A window of eframe or
///   egui-winit always says whether it has the focus.
pub fn system_chooser_refusal(ctx: &egui::Context) -> Option<&'static str> {
    if cfg!(test) {
        Some("cfg(test)")
    } else if REFUSED.load(std::sync::atomic::Ordering::Relaxed) {
        Some("test harness")
    } else if std::env::var_os(TEST_ENV).is_some() {
        Some(TEST_ENV)
    } else if std::env::var_os("NEXTEST").is_some() {
        Some("nextest")
    } else if ctx.input(|i| i.viewport().focused.is_none()) {
        Some("no window")
    } else {
        None
    }
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

    /// The groupings of files: a plugin declares its own here
    /// ([`Groupings::register`]).
    pub fn groupings(&self) -> Groupings {
        lock(&self.0).groupings.clone()
    }

    /// The groups the paths of a finished choice head: none saving, choosing a
    /// folder, for a request that turned them off, or with the person's box off.
    fn groups_of(
        &self,
        app: &AppHandle,
        request: &FileRequest,
        paths: &[PathBuf],
    ) -> Vec<FileGroup> {
        let groups = matches!(request.kind, ChooseKind::Open | ChooseKind::OpenMany)
            && request.group_files
            && grouped(app);
        if !groups {
            return Vec::new();
        }
        self.groupings().of_paths(&self.file_system(), paths)
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
                by: Origin::User,
                choices: None,
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
                if let Some(c) = v.get("choices")
                    && let Err(e) = self.set_choices(id, &choices_of(c))
                {
                    log::warn!(target: "egui_pop", "file.choose: {e}");
                }
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
        let out = picker.show(ui, &places(app, self));
        keep_view(app, &picker);
        settle(app, id, &mut picker, &answer, out, true);
        self.keep(id, picker);
        true
    }

    /// The entry highlighted in the in-app picker of a choice (the one its
    /// preview is of, where the arrows are): what another panel follows
    /// (`docs/linked-panels.md`). `None` without a picker or an entry.
    pub fn highlighted(&self, id: u64) -> Option<Entry> {
        let mut s = lock(&self.0);
        let picker = s.open.iter_mut().find(|o| o.id == id)?.picker.as_mut()?;
        picker.highlighted()
    }

    fn keep(&self, id: u64, picker: FilePicker) {
        let mut s = lock(&self.0);
        if let Some(o) = s.open.iter_mut().find(|o| o.id == id) {
            o.picker = Some(picker);
        }
    }

    /// The default of `file.choose`: the in-app picker, or the system's
    /// chooser when the application allows it, the person wants it, it is
    /// there and it is not refused (tests).
    fn start(&self, app: &AppHandle, id: u64, request: FileRequest) -> Result<Json, String> {
        // `rfd` cannot show the application's choices: the in-app picker does.
        let native = {
            let s = lock(&self.0);
            s.allowed && s.native
        } && request.choices.is_empty()
            && native_available()
            && !system_chooser_refused(&app.egui());
        let fs = self.file_system();
        let previews = self.previews();
        let groupings = self.groupings();
        let show_preview = preview_wanted(app);
        let start = start_folder(app, &request, &fs);
        if !native {
            // Volumes come and go: read when a picker opens, not each frame.
            self.refresh_system_places();
        }
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
        let mut picker = FilePicker::new_in(&at, fs)
            .with_previews(previews)
            .with_grouping(groupings);
        picker.show_preview = show_preview;
        o.picker = Some(picker);
        Ok(json!({ "via": "in_app" }))
    }

    /// Answers a choice (applied on the next frame): paths, or `None`
    /// for cancelled, as the user. For a run-hook replacing the chooser.
    pub fn deliver(&self, id: u64, answer: Option<Vec<PathBuf>>) -> Result<(), String> {
        self.deliver_as(id, answer, Origin::User)
    }

    /// As [`Files::deliver`], answered by `origin` (an agent through the
    /// debug bridge): the origin goes with the choice, to the
    /// `file.chosen` step (`origin`), the journal and the result
    /// ([`chosen_origin`]).
    pub fn deliver_as(
        &self,
        id: u64,
        answer: Option<Vec<PathBuf>>,
        origin: Origin,
    ) -> Result<(), String> {
        let mut s = lock(&self.0);
        let o = s
            .open
            .iter_mut()
            .find(|o| o.id == id)
            .ok_or_else(|| format!("no choice {id} is open"))?;
        o.by = origin;
        *lock(&o.answer) = Some(answer);
        Ok(())
    }

    /// Sets the application's choices of an open choice ([`FileRequest::choice`])
    /// as its answer goes: for a run-hook replacing the chooser, with
    /// [`Files::deliver`]. The values are checked against the request; those
    /// not given keep the in-app picker's, else their defaults.
    pub fn set_choices(&self, id: u64, values: &ChoiceValues) -> Result<(), String> {
        let mut s = lock(&self.0);
        let o = s
            .open
            .iter_mut()
            .find(|o| o.id == id)
            .ok_or_else(|| format!("no choice {id} is open"))?;
        let mut base = o
            .choices
            .clone()
            .or_else(|| o.picker.as_ref().map(|p| p.choice_values.clone()))
            .unwrap_or_else(|| o.request.choice_defaults());
        base.extend(
            o.request
                .choice_values(values)?
                .into_iter()
                .filter(|(k, _)| values.contains_key(k)),
        );
        o.choices = Some(base);
        Ok(())
    }

    /// Gives the system's places the in-app picker lists (its folders and the
    /// volumes mounted) from `source` rather than from the system
    /// ([`places::system_places`]): a demo's, a sandbox's, a test's. It is
    /// asked when a picker opens.
    pub fn set_system_places(&self, source: Arc<dyn Fn() -> SystemPlaces + Send + Sync>) {
        let mut s = lock(&self.0);
        s.system_source = Some(PlaceSource(source));
        s.system = None;
    }

    /// The system's places as last read, read now if never.
    fn system_places(&self) -> SystemPlaces {
        if let Some(p) = lock(&self.0).system.clone() {
            return p;
        }
        self.refresh_system_places()
    }

    /// Reads the system's places again: when a picker opens.
    fn refresh_system_places(&self) -> SystemPlaces {
        let source = lock(&self.0).system_source.clone();
        let read = match source {
            Some(PlaceSource(f)) => f(),
            None if cfg!(target_arch = "wasm32") => SystemPlaces::default(),
            None => places::system_places(),
        };
        lock(&self.0).system = Some(read.clone());
        read
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
            "native_allowed": s.allowed,
            "native_available": native_available(),
            "native_refused": s.refused,
            "open": s.open.iter().map(|o| {
                let mut d = json!({ "id": o.id, "request": o.request.to_json(), "via": o.via });
                if let Some(p) = &o.picker {
                    d["picker"] = p.clone().describe();
                }
                d
            }).collect::<Vec<_>>(),
            "last": s.last.iter().cloned().collect::<Vec<_>>(),
            "previews": s.previews.describe(),
            "groupings": s.groupings.ids(),
        })
    }

    /// `file.select_all`, `file.toggle_selection` and `file.edit_path`: on the open in-app
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

    fn note(&self, id: u64, title: &str, outcome: Json, by: &str) {
        let mut s = lock(&self.0);
        if s.last.len() == 16 {
            s.last.pop_front();
        }
        s.last
            .push_back(json!({ "id": id, "title": title, "outcome": outcome, "by": by }));
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

/// Bookmarks a folder, or removes its bookmark (and the name it was given).
pub fn set_bookmark(app: &AppHandle, dir: &Path, on: bool) {
    let mut list = bookmarks(app);
    list.retain(|p| p != dir);
    if on {
        list.push(dir.to_path_buf());
    } else {
        rename_bookmark(app, dir, "");
    }
    write_list(app, BOOKMARKS, &list);
}

/// The names given to bookmarks, by folder.
pub fn bookmark_names(app: &AppHandle) -> Vec<(PathBuf, String)> {
    lock(&app.store())
        .get(BOOKMARK_NAMES)
        .and_then(|e| e.value.as_text().map(str::to_string))
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Gives a bookmark a name of its own in the picker's side bar; an empty name
/// (or only spaces) gives it back its folder's.
pub fn rename_bookmark(app: &AppHandle, dir: &Path, name: &str) {
    let mut names = bookmark_names(app);
    let before = names.len();
    names.retain(|(d, _)| d != dir);
    let name = name.trim();
    if name.is_empty() && names.len() == before {
        return;
    }
    if !name.is_empty() {
        names.push((dir.to_path_buf(), name.to_owned()));
    }
    // A folder whose name is not Unicode cannot be kept: the names stay as they were.
    if let Ok(text) = serde_json::to_string(&names) {
        write_text(app, BOOKMARK_NAMES, text);
    }
}

/// Moves a bookmark to `to` in the list (clamped to its end).
pub fn move_bookmark(app: &AppHandle, dir: &Path, to: usize) {
    let mut list = bookmarks(app);
    let Some(at) = list.iter().position(|p| p == dir) else {
        return;
    };
    let d = list.remove(at);
    list.insert(to.min(list.len()), d);
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
    fs: Option<SharedFs>,
    builtin_previews: bool,
    groupings: Vec<Arc<dyn FileGrouping>>,
}

impl Default for FilesPlugin {
    /// The in-app picker; the system's chooser only where the application
    /// allows it (`AppBuilder::system_file_chooser`).
    fn default() -> Self {
        Self {
            fs: None,
            builtin_previews: true,
            groupings: Vec::new(),
        }
    }
}

impl FilesPlugin {
    /// The in-app picker: the same as [`FilesPlugin::default`], which no
    /// longer turns to the system's chooser by itself.
    pub fn in_app() -> Self {
        Self::default()
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

    /// Files that go together are one entry of the in-app picker, by this
    /// grouping of the application's ([`groups`]). egui-pop declares none of its
    /// own. Several apply in the order given.
    pub fn grouping(mut self, grouping: Arc<dyn FileGrouping>) -> Self {
        self.groupings.push(grouping);
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
        let allowed = ctx.app().system_file_chooser();
        lock(&files.0).allowed = allowed;
        if let Some(fs) = self.fs.clone() {
            files.set_file_system(fs);
        }
        if self.builtin_previews {
            for p in providers::builtin() {
                files.previews().register(p);
            }
        }
        for grouping in &self.groupings {
            files.groupings().register(grouping.clone());
        }
        // Only where the application allows the system's chooser: elsewhere
        // the setting would change nothing.
        if allowed {
            ctx.setting(
                SETTING_NATIVE,
                Descriptor::bool("Use the system's file chooser", true).description(
                    "Choose files with the system's chooser when there is one; off, with egui-pop's own.",
                ),
            )?;
        }
        ctx.setting(
            SETTING_PREVIEW,
            Descriptor::bool("Previews in the file picker", true).description(
                "egui-pop's file picker shows the selected file's preview and facts beside the list; Alt+P shows or hides it.",
            ),
        )?;
        ctx.setting(
            SETTING_FOLDERS_FIRST,
            Descriptor::bool("Folders first in the file picker", true).description(
                "egui-pop's file picker lists folders before files in every order; off, they sort among the files.",
            ),
        )?;
        {
            let store = ctx.app().store();
            let mut store = lock(&store);
            for (path, label) in [
                (RECENT, "Recent places"),
                (BOOKMARKS, "Bookmarks"),
                (BOOKMARK_NAMES, "Bookmark names"),
                (LAST_FOLDERS, "Last folders by purpose"),
                (VIEWS, "File views by dialog"),
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
                (
                    VIEW,
                    Descriptor::choice(
                        "File picker view",
                        &[
                            PickerView::List.as_str(),
                            PickerView::Grid.as_str(),
                            PickerView::Columns.as_str(),
                        ],
                        browser::MODE_DETAILS,
                    ),
                ),
                (
                    THUMBNAIL_SIZE,
                    Descriptor::float(
                        "File picker thumbnail size",
                        f64::from(THUMBNAIL_MIN_SIZE),
                        f64::from(THUMBNAIL_MAX_SIZE),
                        f64::from(THUMBNAIL_DEFAULT_SIZE),
                    ),
                ),
                (
                    SORT,
                    Descriptor::text("File picker order", &PickerSort::default().as_text()),
                ),
                (GROUPED, Descriptor::bool("File picker groups files", true)),
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
                "A choice of files starts; a run-hook replaces the chooser (answer `paths` or `cancelled`, and `choices` for the request's, or later through `Files::deliver`).",
            )
            .args(json!({ "id": 1, "request": FileRequest::open("Open").to_json() })),
        )?;
        ctx.step(
            StepSpec::new(
                STEP_CHOSEN,
                "Paths were chosen, with the request's `choices` when it has some; a veto refuses them, a redirect changes them.",
            )
            .args(json!({ "id": 1, "request": FileRequest::open("Open").to_json(), "paths": ["/tmp/a.png"] })),
        )?;
        let f = files.clone();
        ctx.command(
            CommandSpec::new(CMD_PREVIEW, "Preview a file", "File")
                .description("A file's or a folder's preview metadata (`path`), as the in-app picker shows it: whether it is a folder, its size and modification time, the provider, the kind of preview, dimensions, pages, a folder's counts, the provider's facts, or why there is none; no pixels and no text.")
                .args(json!({ "path": "/tmp/a.png" }))
                .hidden()
                .read_only(),
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
            CommandSpec::new(CMD_EDIT_PATH, "Edit the folder's path", "File")
                .description("Puts the folder of the open in-app file picker into its field, as a path to edit; saving, the name stays at its end. A path already typed stays as it is.")
                .when(WHEN_EDIT_PATH)
                .key_with("Ctrl+L", BindOptions::global().through_modal()),
            move |call| {
                f.select_on(call.app, |p| {
                    p.edit_path();
                    Ok(())
                })
            },
        )?;
        // Literal titles and descriptions: the catalogue's extractor reads them.
        let keyed = |spec: CommandSpec, keys: &str, id: CommandId| {
            let f = files.clone();
            let spec = spec.key_with(keys, BindOptions::global().through_modal());
            (spec, move |call: &crate::app::Call<'_>| {
                f.select_on(call.app, |p| {
                    p.run_key(&id);
                    Ok(())
                })
            })
        };
        let (spec, run) = keyed(
            CommandSpec::new(CMD_TOGGLE_HIDDEN, "Show or hide hidden files", "File")
                .description("Shows or hides the hidden files of the open in-app file picker.")
                .when(WHEN_EDIT_PATH),
            "Ctrl+H",
            CMD_TOGGLE_HIDDEN,
        );
        ctx.command(spec, run)?;
        let (spec, run) = keyed(
            CommandSpec::new(CMD_RELOAD, "Reload the folder", "File")
                .description("Reads the folder of the open in-app file picker again.")
                .when(WHEN_EDIT_PATH),
            "F5",
            CMD_RELOAD,
        );
        ctx.command(spec, run)?;
        let (spec, run) = keyed(
            CommandSpec::new(CMD_BACK, "Back", "File").description(
                "The open in-app file picker goes back to the folder it showed before; nothing when there is none.",
            )
            // In a modal only: outside one, Alt+Left is the context stack's back.
            .when(WHEN_IN_MODAL),
            "Alt+Left",
            CMD_BACK,
        );
        ctx.command(spec, run)?;
        let (spec, run) = keyed(
            CommandSpec::new(CMD_FORWARD, "Forward", "File").description(
                "The open in-app file picker goes forward to the folder it went back from; nothing when there is none.",
            )
            .when(WHEN_IN_MODAL),
            "Alt+Right",
            CMD_FORWARD,
        );
        ctx.command(spec, run)?;
        for n in 1..=browser::MODE_KEYS {
            let f = files.clone();
            let id = CommandId::new(format!("{CMD_VIEW_MODE}{n}"))
                .map_err(|e| PluginError::Failed(format!("{CMD_VIEW_MODE}{n}: {e:?}")))?;
            ctx.command(
                CommandSpec::new(id, &format!("View mode {n}"), "File")
                    .description(&format!("Shows the entries of the file browser holding the keyboard (the open in-app file picker first) in its mode number {n}, in the order of its switcher; nothing when it has fewer modes."))
                    .when(WHEN_BROWSER)
                    .key_with(&format!("Ctrl+{n}"), BindOptions::global().through_modal()),
                move |call| {
                    // The picker reads its view from the store each frame:
                    // the change goes there.
                    let dialog = lock(&f.0)
                        .active_picker()
                        .map(|p| (p.dialog.clone(), p.views()))
                        .filter(|(_, views)| views.len() > 1);
                    match dialog {
                        Some((dialog, views)) => {
                            if let Some(view) = n.checked_sub(1).and_then(|i| views.get(i)) {
                                want_view_for(call.app, &dialog, view.as_str());
                            }
                        }
                        None => browser::request_mode(&call.app.egui(), n),
                    }
                    call.app.egui().request_repaint();
                    Ok(Json::Null)
                },
            )?;
        }
        ctx.command(
            CommandSpec::new(CMD_TOGGLE_PREVIEW, "Show or hide the preview", "File")
                .description("Shows or hides the preview pane of the in-app file picker, kept for the next one (the `files.preview` setting).")
                .when(WHEN_EDIT_PATH)
                .key_with("Alt+P", BindOptions::global().through_modal()),
            move |call| {
                want_preview(call.app, !preview_wanted(call.app));
                call.app.egui().request_repaint();
                Ok(Json::Null)
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
                "The file choices open (request, chooser, the in-app picker's folder and entries) and the last ones (`by`: who answered); `answer` answers one as its caller.",
            )
            .snapshot(move |_| a.describe())
            .action_as(
                "answer",
                "Answers a choice as its caller (the origin goes with the result): `id`, and `paths` (a list) or `cancel: true`; `choices` (id to value) sets the application's options, checked against the request.",
                true,
                move |_, args, origin| {
                    let id = args.get("id").and_then(Json::as_u64).ok_or("`id` is missing")?;
                    let answer = if args.get("cancel").and_then(Json::as_bool) == Some(true) {
                        None
                    } else {
                        Some(paths_of(args))
                    };
                    if let Some(c) = args.get("choices") {
                        b.set_choices(id, &choices_of(c))?;
                    }
                    b.deliver_as(id, answer, origin.clone())?;
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
    let refused = system_chooser_refusal(ui.ctx());
    let browsing = browser::take_focused(ui.ctx());
    let (new_folder, select_all, selects, edit_path, picker_modes) = {
        let mut s = lock(&files.0);
        s.native = native;
        s.refused = refused;
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
            any(&|_| true),
            any(&|p| p.previews.is_some()),
        )
    };
    let say = |on: bool| if on { "true" } else { "false" };
    app.set_context(WHEN_NEW_FOLDER, say(new_folder));
    app.set_context(WHEN_SELECT_ALL, say(select_all));
    app.set_context(WHEN_SELECTS, say(selects));
    app.set_context(WHEN_EDIT_PATH, say(edit_path));
    app.set_context(WHEN_BROWSER, say(picker_modes || browsing));
    let done: Vec<(
        u64,
        FileRequest,
        Answer,
        ChoiceValues,
        Option<Resolver>,
        Origin,
    )> = {
        let mut s = lock(&files.0);
        let mut done = Vec::new();
        let mut i = 0;
        while i < s.open.len() {
            let answer = lock(&s.open[i].answer).take();
            match answer {
                Some(a) => {
                    let o = s.open.remove(i);
                    let choices = o
                        .choices
                        .or_else(|| o.picker.map(|p| p.choice_values))
                        .unwrap_or_else(|| o.request.choice_defaults());
                    done.push((o.id, o.request, a, choices, o.resolver, o.by));
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
    for (id, request, answer, choices, resolver, by) in done {
        overlay::close(&ctx, overlay_id(id));
        let answer = answer.map(|paths| (paths, choices));
        finish(app, &files, id, &request, answer, resolver, &by);
    }
    picker_overlay(&ctx, app, &files);
}

fn overlay_id(id: u64) -> egui::Id {
    egui::Id::unique(("egui_pop.files", id))
}

/// Ends a choice: `file.chosen` for paths (and the application's choices),
/// the journal for a cancel.
fn finish(
    app: &AppHandle,
    files: &Files,
    id: u64,
    request: &FileRequest,
    answer: Option<(Vec<PathBuf>, ChoiceValues)>,
    resolver: Option<Resolver>,
    by: &Origin,
) {
    let source = crate::host::source_of(by);
    let result = match answer {
        None => {
            app.publish(&source, None, "file.cancelled", json!({ "id": id }));
            files.note(id, &request.title, json!("cancelled"), &source);
            if *by == Origin::User {
                Ok(Json::Null)
            } else {
                Ok(json!({ "cancelled": true, "origin": by }))
            }
        }
        Some((paths, choices)) => {
            let found = files.groups_of(app, request, &paths);
            let mut args = json!({
                "id": id,
                "request": request.to_json(),
                "paths": paths_json(&paths),
                "origin": source,
            });
            if !found.is_empty() {
                args["groups"] = groups::to_json(&found);
            }
            if !request.choices.is_empty() {
                args["choices"] = json!(choices);
            }
            let out = app.step(&STEP_CHOSEN, args, |app, args| {
                let chosen = paths_of(args);
                remember(app, &chosen);
                remember_folder(app, request, &chosen);
                let mut out = json!({ "paths": paths_json(&chosen), "origin": by });
                // A before-hook may have changed them, as the paths.
                if let Some(c) = args.get("choices") {
                    out["choices"] = c.clone();
                }
                // A hook may have redirected the choice: its groups, then.
                let last = if chosen == paths {
                    found.clone()
                } else {
                    files.groups_of(app, request, &chosen)
                };
                if !last.is_empty() {
                    out["groups"] = groups::to_json(&last);
                }
                Ok(out)
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
                &source,
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
        Some(PickerAnswer::RenameBookmark(dir, name)) => rename_bookmark(app, &dir, &name),
        Some(PickerAnswer::MoveBookmark(dir, to)) => move_bookmark(app, &dir, to),
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

fn places(app: &AppHandle, files: &Files) -> Places {
    Places {
        home: home(),
        system: files.system_places(),
        recent: recent(app),
        bookmarks: bookmarks(app),
        bookmark_names: bookmark_names(app),
    }
}

/// Whether a click came from the pointer: its press left nothing focused, where
/// Enter or Space leave the focus on the widget.
fn by_pointer(r: &egui::Response) -> bool {
    r.clicked_by(egui::PointerButton::Primary)
}

/// The width of the in-app picker's modal at first, the preview pane
/// shown or not: wide enough for the places, the list with all its columns
/// and the full pane.
pub const PICKER_WIDTH: f32 = 960.0;
/// The height of the in-app picker's modal at first: the list has its
/// usual height ([`LIST_HEIGHT`]).
pub const PICKER_HEIGHT: f32 = 398.0;
/// The least size the in-app picker's modal is dragged to: the row of
/// its filters and buttons fits, and a few entries.
pub const PICKER_MIN_SIZE: egui::Vec2 = egui::vec2(640.0, 280.0);
/// The list's height when the picker is not given a height to fill.
pub const LIST_HEIGHT: f32 = 280.0;
/// The width of the in-app picker's side bar of places, in points.
const PLACES_WIDTH: f32 = 150.0;
/// The narrowest the picker's list gets for the preview pane: under
/// it, the pane shrinks instead.
pub const LIST_MIN_WIDTH: f32 = 240.0;
/// The narrowest preview pane drawn: with less room, it is not shown.
pub const PANE_MIN_WIDTH: f32 = 120.0;
/// The narrowest the name's column gets: with less room, the other columns
/// go, the type first and the modification time last.
pub const NAME_MIN_WIDTH: f32 = 140.0;

/// The smallest thumbnails of the picker's grid, in points.
pub const THUMBNAIL_MIN_SIZE: f32 = 64.0;
/// The largest thumbnails of the picker's grid, in points: the side
/// previews are made at by default ([`preview::MAX_SIDE`]).
pub const THUMBNAIL_MAX_SIZE: f32 = 256.0;
/// The size of the picker's thumbnails at first, in points.
pub const THUMBNAIL_DEFAULT_SIZE: f32 = 128.0;
/// `Ctrl` and `+` or `-` multiply or divide the thumbnails' size by this.
const THUMBNAIL_STEP: f32 = 1.25;
/// The width of the slider of the thumbnails' size.
const THUMBNAIL_SLIDER_WIDTH: f32 = 170.0;

/// With less room than this beside the controls, the breadcrumbs take a row of
/// their own: the deepest segment would hardly show.
const CRUMBS_MIN_WIDTH: f32 = 160.0;
/// How wide a breadcrumb's menu gets: a longer name is cut.
const CRUMB_MENU_WIDTH: f32 = 320.0;
/// How high a breadcrumb's menu gets: it scrolls past that.
const CRUMB_MENU_HEIGHT: f32 = 320.0;

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
    let places = places(app, files);
    load_view(app, &mut picker);
    let modal = overlay_id(id);
    // One size, the preview on or off: the pane takes its width from
    // the list's, so toggling it does not resize the dialog. The size at
    // opening is what the person left, read once per dialog.
    let size = overlay::modal_size(ctx, modal).unwrap_or_else(|| opening_size(ctx, app));
    let mut spec = OverlaySpec::new(("egui_pop.files", id), &title, Kind::Modal)
        .resizable(size, PICKER_MIN_SIZE);
    debug_assert_eq!(spec.id, modal);
    if picker.naming.is_some() || picker.renaming.is_some() {
        // Escape closes the row asking a new folder's name, or a bookmark's,
        // not the picker.
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
    picker.hidden_keys = app.keys_for(&CMD_TOGGLE_HIDDEN);
    picker.preview_keys = app.keys_for(&CMD_TOGGLE_PREVIEW);
    picker.declared_keys = true;
    picker.show_preview = preview_wanted(app);
    if !picker.pane_dragging {
        picker.pane_width = pane_width(app);
    }
    let view = view_for(app, &picker.dialog)
        .and_then(|m| PickerView::parse(&m))
        .unwrap_or_else(|| view(app));
    // The columns, last used choosing a folder, are not a file dialog's.
    picker.view = if picker.views().contains(&view) || view == PickerView::Grid {
        view
    } else {
        PickerView::List
    };
    if !picker.thumbnail_dragging {
        picker.thumbnail_size = thumbnail_size(app);
    }
    picker.set_sort(sort_order(app));
    picker.set_folders_first(folders_first(app));
    picker.set_grouped(grouped(app));
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
    if view_for(app, &picker.dialog).and_then(|m| PickerView::parse(&m)) != Some(picker.view) {
        want_view_for(app, &picker.dialog, picker.view.as_str());
    }
    if !picker.thumbnail_dragging && picker.thumbnail_size != thumbnail_size(app) {
        want_thumbnail_size(app, picker.thumbnail_size);
    }
    if picker.sort != sort_order(app) {
        want_sort_order(app, picker.sort);
    }
    if picker.folders_first != folders_first(app) {
        want_folders_first(app, picker.folders_first);
    }
    if picker.grouped != grouped(app) {
        want_grouped(app, picker.grouped);
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

/// Writes a remembered value of the picker, outside any undo history.
fn keep_value(app: &AppHandle, path: &str, value: Value) {
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
    keep_value(app, DIALOG_SIZE, value);
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
    keep_value(app, PANE_WIDTH, Value::Float(f64::from(width)));
}

/// The in-app picker's view last used, as kept.
pub fn view(app: &AppHandle) -> PickerView {
    lock(&app.store())
        .get(VIEW)
        .and_then(|e| e.value.as_text().and_then(PickerView::parse))
        .unwrap_or_default()
}

/// Keeps the in-app picker's view last used.
pub fn want_view(app: &AppHandle, view: PickerView) {
    keep_value(app, VIEW, Value::Choice(view.as_str().into()));
}

fn views(app: &AppHandle) -> Vec<(String, String)> {
    lock(&app.store())
        .get(VIEWS)
        .and_then(|e| e.value.as_text().map(str::to_string))
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// The mode a dialog was last shown in (a mode id: `details`,
/// `thumbnails`), kept in the host's store; `None` for a dialog never
/// shown.
pub fn view_for(app: &AppHandle, dialog: &str) -> Option<String> {
    views(app)
        .into_iter()
        .find_map(|(d, mode)| (d == dialog).then_some(mode))
}

/// Keeps the mode a dialog is shown in, and as the view last used.
pub fn want_view_for(app: &AppHandle, dialog: &str, mode: &str) {
    let mut list = views(app);
    list.retain(|(d, _)| d != dialog);
    list.insert(0, (dialog.to_string(), mode.to_string()));
    list.truncate(VIEWS_KEPT);
    if let Ok(text) = serde_json::to_string(&list) {
        write_text(app, VIEWS, text);
    }
    if let Some(view) = PickerView::parse(mode) {
        want_view(app, view);
    }
}

/// The dialog a request's view is kept under: its purpose, else its kind.
pub fn dialog_of(request: &FileRequest) -> String {
    request.purpose.clone().unwrap_or_else(|| {
        match request.kind {
            ChooseKind::Open => "open",
            ChooseKind::OpenMany => "open_many",
            ChooseKind::Save => "save",
            ChooseKind::Folder => "folder",
        }
        .to_string()
    })
}

/// The size of the in-app picker's thumbnails, as kept.
pub fn thumbnail_size(app: &AppHandle) -> f32 {
    lock(&app.store())
        .get(THUMBNAIL_SIZE)
        .and_then(|e| e.value.as_f64())
        .map_or(THUMBNAIL_DEFAULT_SIZE, |s| s as f32)
}

/// Keeps the size of the in-app picker's thumbnails.
pub fn want_thumbnail_size(app: &AppHandle, size: f32) {
    let size = size.clamp(THUMBNAIL_MIN_SIZE, THUMBNAIL_MAX_SIZE);
    keep_value(app, THUMBNAIL_SIZE, Value::Float(f64::from(size)));
}

/// The in-app picker's order, as kept.
pub fn sort_order(app: &AppHandle) -> PickerSort {
    lock(&app.store())
        .get(SORT)
        .and_then(|e| e.value.as_text().and_then(PickerSort::parse))
        .unwrap_or_default()
}

/// Keeps the in-app picker's order.
pub fn want_sort_order(app: &AppHandle, sort: PickerSort) {
    keep_value(app, SORT, Value::Text(sort.as_text()));
}

/// Whether the in-app picker keeps folders first (the setting).
pub fn folders_first(app: &AppHandle) -> bool {
    lock(&app.store())
        .get(&format!("settings/{SETTING_FOLDERS_FIRST}"))
        .and_then(|e| e.value.as_bool())
        .unwrap_or(true)
}

/// Keeps folders first in the in-app picker, or not (the setting).
pub fn want_folders_first(app: &AppHandle, on: bool) {
    let path = format!("settings/{SETTING_FOLDERS_FIRST}");
    keep_value(app, &path, Value::Bool(on));
}

/// Whether the in-app picker groups files, with groupings declared.
pub fn grouped(app: &AppHandle) -> bool {
    lock(&app.store())
        .get(GROUPED)
        .and_then(|e| e.value.as_bool())
        .unwrap_or(true)
}

/// Keeps whether the in-app picker groups files.
pub fn want_grouped(app: &AppHandle, on: bool) {
    keep_value(app, GROUPED, Value::Bool(on));
}

/// The home folder, if known.
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|d| d.is_dir())
}

/// Places the picker lists beside the folder: home, the system's folders,
/// the volumes mounted, recent places and bookmarks. Made with
/// `Places::default()` and its fields.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Places {
    pub home: Option<PathBuf>,
    /// The system's folders and volumes ([`places::system_places`]).
    pub system: SystemPlaces,
    pub recent: Vec<PathBuf>,
    pub bookmarks: Vec<PathBuf>,
    /// The names given to bookmarks ([`rename_bookmark`]).
    pub bookmark_names: Vec<(PathBuf, String)>,
}

impl Places {
    /// A bookmark's name: the one given, else its folder's.
    pub fn bookmark_name(&self, dir: &Path) -> String {
        self.bookmark_names
            .iter()
            .find_map(|(d, n)| (d == dir).then(|| n.clone()))
            .unwrap_or_else(|| folder_name(dir))
    }
}

fn folder_name(dir: &Path) -> String {
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.display().to_string())
}

/// How the in-app picker shows a folder's entries: the file browser's
/// modes it draws ([`browser::MODE_DETAILS`], [`browser::MODE_THUMBNAILS`],
/// [`browser::MODE_COLUMNS`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum PickerView {
    /// The details mode: one entry per row, with its facts in columns.
    #[default]
    List,
    /// The thumbnails mode: thumbnails from the preview providers (only
    /// with previews, [`FilePicker::with_previews`]: details without them).
    Grid,
    /// The columns mode, Finder's: the path's folders side by side, the
    /// highlighted folder's subfolders after them. Only when choosing a
    /// folder: details elsewhere.
    Columns,
}

impl PickerView {
    /// Every view, in the switcher's order; a picker offers those that fit
    /// it ([`FilePicker::views`]).
    pub const ALL: [PickerView; 3] = [PickerView::List, PickerView::Grid, PickerView::Columns];

    /// Its mode's id, its name in the store.
    pub fn as_str(self) -> &'static str {
        match self {
            PickerView::List => browser::MODE_DETAILS,
            PickerView::Grid => browser::MODE_THUMBNAILS,
            PickerView::Columns => browser::MODE_COLUMNS,
        }
    }

    /// The view of a mode's id; the names kept before the modes (`list`,
    /// `grid`) too.
    pub fn parse(name: &str) -> Option<PickerView> {
        match name {
            browser::MODE_DETAILS | "list" => Some(PickerView::List),
            browser::MODE_THUMBNAILS | "grid" => Some(PickerView::Grid),
            browser::MODE_COLUMNS => Some(PickerView::Columns),
            _ => None,
        }
    }
}

/// A column of the picker's list, as it fits the width it has.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ListColumn {
    column: SortColumn,
    /// Where its text starts, from the row's left.
    left: f32,
    width: f32,
}

impl SortColumn {
    /// Its title.
    fn title(self) -> &'static strings::StrKey {
        match self {
            SortColumn::Name => &txt::FILES_COLUMN_NAME,
            SortColumn::Size => &txt::FILES_COLUMN_SIZE,
            SortColumn::Modified => &txt::FILES_COLUMN_MODIFIED,
            SortColumn::Type => &txt::FILES_COLUMN_TYPE,
        }
    }
}

/// A row's height in the picker's list, and its headers': a selectable row's.
fn row_height(ui: &Ui) -> f32 {
    let text = ui.text_style_height(&egui::TextStyle::Body);
    ui.spacing()
        .interact_size
        .y
        .max(text + 2.0 * ui.spacing().button_padding.y)
}

/// Lays columns out in a row `room` wide: `wanted` holds the width of each but
/// the name, in the order they are given up, the last first, while the name has
/// less than [`NAME_MIN_WIDTH`]. Their order on screen is the name, then
/// `wanted`'s.
fn fit_columns(room: f32, gap: f32, padding: f32, wanted: &[(SortColumn, f32)]) -> Vec<ListColumn> {
    let mut kept = wanted.len();
    let name_width = loop {
        let others: f32 = wanted[..kept].iter().map(|(_, w)| w + gap).sum();
        let name = room - 2.0 * padding - others;
        if name >= NAME_MIN_WIDTH || kept == 0 {
            break name.max(0.0);
        }
        kept -= 1;
    };
    let mut out = vec![ListColumn {
        column: SortColumn::Name,
        left: padding,
        width: name_width,
    }];
    let mut left = padding + name_width + gap;
    for &(column, width) in &wanted[..kept] {
        out.push(ListColumn {
            column,
            left,
            width,
        });
        left += width + gap;
    }
    out
}

/// The columns of a list `width` wide: wide enough for their title with its
/// arrow, and for what they say. Choosing a folder lists only folders, which
/// have no size and all one type.
fn list_columns(ui: &Ui, width: f32, kind: ChooseKind) -> Vec<ListColumn> {
    let ctx = ui.ctx();
    let font = egui::TextStyle::Body.resolve(ui.style());
    let measure = |text: String| {
        ui.painter()
            .layout_no_wrap(text, font.clone(), egui::Color32::PLACEHOLDER)
            .size()
    };
    let inset = header_inset(ui, &Kit::of(ctx), &font);
    let wanted = |column: SortColumn, sample: &str| {
        let title = measure(strings::tr(ctx, column.title()));
        // The arrow goes after the title, a little apart, as high as the text.
        let header = title.x + title.y * 1.25 + inset;
        (column, measure(sample.to_string()).x.max(header).ceil())
    };
    let folder = strings::tr(ctx, &txt::FILES_KIND_FOLDER);
    let mut columns = vec![wanted(SortColumn::Modified, "0000-00-00 00:00 UTC")];
    if kind != ChooseKind::Folder {
        columns.push(wanted(SortColumn::Size, "999 MB"));
        columns.push(wanted(SortColumn::Type, &folder));
    }
    let gap = Tokens::of(ctx).space.m;
    fit_columns(width, gap, ui.spacing().button_padding.x, &columns)
}

/// How far a header's title starts from the left of its cell.
fn header_inset(ui: &Ui, kit: &Kit, font: &egui::FontId) -> f32 {
    let galley =
        ui.painter()
            .layout_no_wrap("A".to_string(), font.clone(), egui::Color32::PLACEHOLDER);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(100.0, 40.0));
    let part = Part {
        rect,
        room: rect,
        state: PartState::REST,
        label: Some(&galley),
    };
    kit.table_header_cell
        .label_rect(&part, galley.size())
        .left()
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
#[non_exhaustive]
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
    /// Give this bookmark this name (empty: its folder's).
    RenameBookmark(PathBuf, String),
    /// Move this bookmark to this place in the list.
    MoveBookmark(PathBuf, usize),
}

/// The keys a tip says: `Ctrl+H`, or `Ctrl+H, F8` when bound twice.
fn keys_tip(keys: &[String]) -> String {
    keys.join(", ")
}

/// An entry of the list or the grid, as assistive technology reads it: an
/// option of the list box, selected as its row says.
fn as_option(ui: &Ui, r: &egui::Response) {
    ui.ctx().accesskit_node_builder(r.id, |node| {
        node.set_role(egui::accesskit::Role::ListBoxOption);
    });
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

/// A folder on the breadcrumbs, or in one of their menus.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Crumb {
    name: String,
    path: PathBuf,
}

/// A path without `.`, and without `..` where the folder it leaves is named: a
/// typed `../` would otherwise stay in the path and read as a folder.
fn without_dots(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => out.push(part),
            },
            _ => out.push(part),
        }
    }
    out
}

/// The folders a folder lies in and itself, the root first.
fn crumbs_of(dir: &Path) -> Vec<Crumb> {
    let dir = without_dots(dir);
    let mut out: Vec<Crumb> = dir
        .ancestors()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| Crumb {
            name: p.file_name().map_or_else(
                || p.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            ),
            path: p.to_path_buf(),
        })
        .collect();
    out.reverse();
    out
}

/// The first crumb shown, given each one's width: all of them when they fit,
/// else as many of the deepest as fit beside the button standing for the others
/// (`collapsed` wide). The deepest stays whatever the room.
fn first_crumb(widths: &[f32], room: f32, collapsed: f32) -> usize {
    if widths.iter().sum::<f32>() <= room {
        return 0;
    }
    let room = room - collapsed;
    let mut used = 0.0;
    let mut first = widths.len();
    for (n, width) in widths.iter().enumerate().rev() {
        used += width;
        if used > room && first < widths.len() {
            break;
        }
        first = n;
    }
    first
}

/// A folder as last read, and in which order its entries lie.
#[derive(Clone, Debug)]
struct Listing {
    dir: PathBuf,
    hidden: bool,
    order: (PickerSort, bool),
    entries: Arc<Vec<Entry>>,
    /// Groups were looked for.
    grouped: bool,
    /// The folder's groups, by main file.
    groups: Arc<HashMap<PathBuf, FileGroup>>,
    /// The main file of each member.
    members: Arc<HashMap<PathBuf, PathBuf>>,
}

/// The subfolders the picker's columns list, by folder and whether hidden
/// ones are shown.
type ColumnCache = Arc<Mutex<browser::ColumnListings>>;

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
    /// How the entries are shown.
    pub view: PickerView,
    /// The dialog its view is kept under ([`dialog_of`]).
    pub dialog: String,
    /// The thumbnails' size in the grid, in points.
    pub thumbnail_size: f32,
    /// The order of the entries, in the list and in the grid.
    pub sort: PickerSort,
    /// Folders come before files in every order.
    pub folders_first: bool,
    /// Files that go together are one entry, with groupings declared
    /// ([`FilePicker::with_grouping`]) and when opening files.
    pub grouped: bool,
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
    /// The keys of the hidden files and preview commands, as their check
    /// boxes' tips say them.
    hidden_keys: Vec<String>,
    preview_keys: Vec<String>,
    /// The folders shown before, and those gone back from.
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
    /// The selection's keys are commands ([`Files`] draws this picker);
    /// drawn on its own, it has no keymap and reads them itself.
    declared_keys: bool,
    /// Where `Shift` extends the selection from.
    anchor: Option<PathBuf>,
    /// What the selection line said last: a change is announced.
    said: Option<String>,
    /// The folder last announced: another one is.
    said_dir: Option<PathBuf>,
    /// The list's rows as last drawn: holding the focus, they leave the
    /// picker's keys to the picker.
    row_ids: Vec<egui::Id>,
    /// The name proposed when saving: back when a typed name was a folder.
    proposed: String,
    /// Asks before replacing a file.
    confirm_overwrite: bool,
    /// The bookmark being renamed, and the name typed.
    renaming: Option<(PathBuf, String)>,
    /// That field takes the focus.
    renaming_focus: bool,
    /// The dialog asking whether to replace a file: the picker waits.
    asking: Option<u64>,
    /// The question closed: the field takes the focus back, which it lost
    /// while disabled.
    refocus: bool,
    /// The name's stem is selected in the field at the next frame.
    select_stem: bool,
    /// The field's cursor goes to its end at the next frame.
    cursor_to_end: bool,
    /// What the open breadcrumb menu lists, read as it opened (none: the folder
    /// could not be read). Shared: the picker is copied every frame it is drawn.
    crumb_menu: Option<Arc<Option<Vec<Crumb>>>>,
    /// The keyboard opened that menu: its first item takes the focus, and the
    /// focus goes back to its button as it closes.
    crumb_menu_keys: bool,
    /// That first item is still to take the focus.
    crumb_menu_focus: bool,
    /// Where text is translated, once drawn.
    ctx: Option<egui::Context>,
    focused: bool,
    /// The splitter is being dragged.
    pane_dragging: bool,
    /// The height of what comes below the list, as last drawn.
    /// The cells in a row of the grid, as last drawn.
    columns: usize,
    /// The modes drawing the list and the grid.
    details: browser::DetailsMode,
    thumbs: browser::ThumbnailsMode,
    cols: browser::ColumnsMode,
    /// The listings of the columns beside the folder shown (its ancestors'
    /// subfolders, the highlighted folder's), each off the interface thread
    /// on the system's file system. Shared: the picker is copied every
    /// frame.
    column_cache: ColumnCache,
    /// The grid brings the highlighted cell into view at the next frame.
    reveal_cell: bool,
    /// The highlighted cell takes the focus when it is next drawn: the
    /// keyboard is moving from cell to cell.
    focus_cell: bool,
    /// The slider of the thumbnails' size is being dragged.
    thumbnail_dragging: bool,
    /// The grid asked for thumbnails at the last frame.
    asked_thumbnails: bool,
    /// The facts of the files the grid has shown, read once per listing:
    /// the cells in view would read them every frame otherwise.
    metas: Arc<Mutex<HashMap<PathBuf, Option<Meta>>>>,
    /// Shared: the picker is copied every frame it is drawn.
    listing: Option<Listing>,
    fs: SharedFs,
    previews: Option<Previews>,
    groupings: Option<Groupings>,
    /// The request leaves groups on ([`FileRequest::ungrouped`]).
    request_groups: bool,
    /// The application's options ([`FileRequest::choice`]), and their values.
    choices: Vec<FileChoice>,
    choice_values: ChoiceValues,
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
            view: PickerView::List,
            dialog: dialog_of(request),
            thumbnail_size: THUMBNAIL_DEFAULT_SIZE,
            sort: PickerSort::default(),
            folders_first: true,
            grouped: true,
            fill: false,
            error: None,
            naming: None,
            naming_focus: false,
            new_folder_keys: Vec::new(),
            hidden_keys: vec!["Ctrl+H".into()],
            preview_keys: vec!["Alt+P".into()],
            back: Vec::new(),
            forward: Vec::new(),
            declared_keys: false,
            anchor: None,
            said: None,
            said_dir: None,
            row_ids: Vec::new(),
            proposed,
            confirm_overwrite: request.confirm_overwrite,
            renaming: None,
            renaming_focus: false,
            asking: None,
            refocus: false,
            select_stem: request.kind == ChooseKind::Save,
            cursor_to_end: false,
            crumb_menu: None,
            crumb_menu_keys: false,
            crumb_menu_focus: false,
            ctx: None,
            focused: false,
            pane_dragging: false,
            columns: 1,
            details: browser::DetailsMode::default(),
            thumbs: browser::ThumbnailsMode::default(),
            cols: browser::ColumnsMode::default(),
            column_cache: Arc::new(Mutex::new(
                browser::ColumnListings::default().folders_only(true),
            )),
            reveal_cell: false,
            focus_cell: false,
            thumbnail_dragging: false,
            asked_thumbnails: false,
            metas: Arc::default(),
            listing: None,
            fs,
            previews: None,
            groupings: None,
            request_groups: request.group_files,
            choices: request.choices.clone(),
            choice_values: request.choice_defaults(),
        }
    }

    /// The application's choices ([`FileRequest::choice`]) as they stand: each
    /// id with its value.
    pub fn choices(&self) -> &ChoiceValues {
        &self.choice_values
    }

    /// Sets one of the application's choices; refused for an id the request
    /// does not have or a value it cannot take.
    pub fn set_choice(&mut self, id: &str, value: &str) -> Result<(), String> {
        let c = self
            .choices
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| format!("no choice `{id}` in this request"))?;
        if !c.accepts(value) {
            return Err(format!("`{value}` is not a value of the choice `{id}`"));
        }
        self.choice_values.insert(id.into(), value.into());
        Ok(())
    }

    /// Shows the selected entry's preview in a pane beside the list,
    /// from these providers (`Alt+P` hides it).
    pub fn with_previews(mut self, previews: Previews) -> Self {
        self.previews = Some(previews);
        self.show_preview = true;
        self
    }

    /// Files that go together are one entry, by these groupings ([`groups`]).
    /// Drawn on its own, the picker answers with the main files: its caller
    /// reads the others with [`group_of`](Self::group_of).
    pub fn with_grouping(mut self, groupings: Groupings) -> Self {
        self.groupings = Some(groupings);
        self
    }

    /// Whether this choice can group files: opening files, the request not
    /// against it, with a grouping declared.
    fn can_group(&self) -> bool {
        matches!(self.kind, ChooseKind::Open | ChooseKind::OpenMany)
            && self.request_groups
            && self.groupings.as_ref().is_some_and(|g| !g.is_empty())
    }

    /// Whether it groups now.
    fn grouping_on(&self) -> bool {
        self.grouped && self.can_group()
    }

    /// The group a file heads, in the folder as last listed.
    pub fn group_of(&self, path: &Path) -> Option<&FileGroup> {
        self.listing.as_ref()?.groups.get(path)
    }

    /// Whether a file is left out of the entries, for its group's: the filter
    /// has to accept the main file, or the member stays an entry of its own.
    fn absorbed(&self, listing: &Listing, path: &Path) -> bool {
        listing
            .members
            .get(path)
            .is_some_and(|main| self.accepts(main))
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
        let dir = without_dots(&dir);
        if dir != self.dir {
            self.back
                .push(std::mem::replace(&mut self.dir, dir.clone()));
            if self.back.len() > HISTORY_KEPT {
                self.back.remove(0);
            }
            self.forward.clear();
        }
        self.show_dir(dir);
    }

    /// Shows `dir`, the history left as it is.
    fn show_dir(&mut self, dir: PathBuf) {
        self.dir = dir;
        self.highlight = 0;
        self.reveal_cell = true;
        self.error = None;
        self.selected.clear();
        self.anchor = None;
        if self.kind != ChooseKind::Save {
            self.text.clear();
        }
    }

    /// Goes back to the folder shown before; says whether there was one.
    pub fn go_back(&mut self) -> bool {
        let Some(dir) = self.back.pop() else {
            return false;
        };
        self.forward
            .push(std::mem::replace(&mut self.dir, dir.clone()));
        self.show_dir(dir);
        true
    }

    /// Goes forward to the folder gone back from; says whether there was one.
    pub fn go_forward(&mut self) -> bool {
        let Some(dir) = self.forward.pop() else {
            return false;
        };
        self.back
            .push(std::mem::replace(&mut self.dir, dir.clone()));
        self.show_dir(dir);
        true
    }

    /// Reads the folder again, and the facts of its files.
    pub fn reload(&mut self) {
        self.listing = None;
        // A file changed is previewed again: its time is in the cache's key.
        lock(&self.metas).clear();
        lock(&self.column_cache).clear();
    }

    /// What a key of the picker's commands does to it.
    fn run_key(&mut self, id: &CommandId) {
        if *id == CMD_TOGGLE_HIDDEN {
            self.show_hidden = !self.show_hidden;
        } else if *id == CMD_RELOAD {
            self.reload();
        } else if *id == CMD_BACK {
            self.go_back();
        } else if *id == CMD_FORWARD {
            self.go_forward();
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

    /// The folder's entries in the picker's order, those the filter accepts
    /// (folders whatever the filter; none but folders when choosing a folder);
    /// hidden ones only when asked. The folder is read once, and sorting by
    /// size or by date reads every entry's facts once, off that listing.
    pub fn entries(&mut self) -> Vec<Entry> {
        let order = (self.sort, self.folders_first);
        let read = self.listing.as_ref().is_none_or(|l| {
            l.dir != self.dir || l.hidden != self.show_hidden || l.grouped != self.grouping_on()
        });
        if read {
            let all: Vec<Entry> = self
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
            lock(&self.metas).clear();
            let (groups, members) = self.groups_in(&all);
            let all = sort::sort_entries(all, order.0, order.1, |e| self.meta_of(&e.path));
            self.listing = Some(Listing {
                dir: self.dir.clone(),
                hidden: self.show_hidden,
                order,
                entries: Arc::new(all),
                grouped: self.grouping_on(),
                groups: Arc::new(groups),
                members: Arc::new(members),
            });
        } else if let Some(l) = self.listing.as_ref().filter(|l| l.order != order) {
            let all = sort::sort_entries(l.entries.to_vec(), order.0, order.1, |e| {
                self.meta_of(&e.path)
            });
            if let Some(l) = &mut self.listing {
                l.order = order;
                l.entries = Arc::new(all);
            }
        }
        self.listing
            .as_ref()
            .map(|l| {
                l.entries
                    .iter()
                    .filter(|x| x.is_dir || self.accepts(&x.path))
                    .filter(|x| !self.absorbed(l, &x.path))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The groups among a folder's entries, by main file, and each member's main
    /// file. A hidden file joins no group, shown or not.
    #[allow(clippy::type_complexity)]
    fn groups_in(&self, all: &[Entry]) -> (HashMap<PathBuf, FileGroup>, HashMap<PathBuf, PathBuf>) {
        let Some(groupings) = self.groupings.as_ref().filter(|_| self.grouping_on()) else {
            return Default::default();
        };
        let names: Vec<&str> = all
            .iter()
            .filter(|e| !e.is_dir && !e.name.starts_with('.'))
            .map(|e| e.name.as_str())
            .collect();
        let mut groups = HashMap::new();
        let mut members = HashMap::new();
        for group in groupings.in_folder(&self.dir, &names) {
            for member in &group.members {
                members.insert(member.clone(), group.main.clone());
            }
            groups.insert(group.main.clone(), group);
        }
        (groups, members)
    }

    /// Groups files or leaves each its own entry: the highlight stays on its
    /// entry, or goes to the group that takes it in, and a member selected
    /// leaves the selection for what its group's entry stands for.
    pub fn set_grouped(&mut self, on: bool) {
        if on == self.grouped {
            return;
        }
        self.reorder(|p| p.grouped = on);
        self.entries();
        let Some(listing) = self.listing.clone() else {
            return;
        };
        let taken: Vec<PathBuf> = (self.selected.iter())
            .filter(|p| self.absorbed(&listing, p))
            .cloned()
            .collect();
        for path in taken {
            self.selected.remove(&path);
        }
    }

    /// Takes another order; the highlighted entry stays highlighted, wherever
    /// it lands, and comes into view.
    pub fn set_sort(&mut self, sort: PickerSort) {
        if sort != self.sort {
            self.reorder(|p| p.sort = sort);
        }
    }

    /// Keeps folders first, or sorts them among the files; the highlighted
    /// entry stays highlighted, wherever it lands, and comes into view.
    pub fn set_folders_first(&mut self, on: bool) {
        if on != self.folders_first {
            self.reorder(|p| p.folders_first = on);
        }
    }

    fn reorder(&mut self, change: impl FnOnce(&mut FilePicker)) {
        let at = self.highlighted().map(|e| e.path);
        change(self);
        if let Some(path) = at {
            let shown = self.visible();
            let main = self.listing.as_ref().and_then(|l| l.members.get(&path));
            let path = main.unwrap_or(&path);
            if let Some(n) = shown.iter().position(|e| e.path == *path) {
                self.highlight = n;
            }
        }
        self.reveal_cell = true;
        if let Some(ctx) = &self.ctx {
            ctx.request_repaint();
        }
    }

    /// Whether the field's text puts the entries in order of its best
    /// matches, whatever the order asked.
    fn ranks(&self) -> bool {
        !self.is_path() && !self.text.trim().is_empty() && self.kind != ChooseKind::Save
    }

    /// Ctrl+L: the folder goes into the field, as a path to edit, and the field
    /// takes the focus; a path already typed stays. Saving, the name stays at the
    /// path's end with its stem selected, and comes back if the path leads to
    /// another folder instead.
    pub fn edit_path(&mut self) {
        self.refocus = true;
        self.select_stem = self.kind == ChooseKind::Save;
        self.cursor_to_end = self.kind != ChooseKind::Save;
        if self.is_path() {
            return;
        }
        let dir = self.dir.display().to_string();
        let separator = if dir.ends_with(['/', std::path::MAIN_SEPARATOR]) {
            String::new()
        } else {
            std::path::MAIN_SEPARATOR.to_string()
        };
        let name = self.keep_name();
        self.text = format!("{dir}{separator}{name}");
        self.text_changed();
    }

    /// `/` or `~` typed on an entry: the field takes it, and the focus.
    fn start_path(&mut self, typed: &str) {
        self.keep_name();
        self.text = typed.to_string();
        self.text_changed();
        self.refocus = true;
        self.cursor_to_end = true;
    }

    /// Saving, the name in the field, as a path is about to take its place: it
    /// is the one to come back once in a folder.
    fn keep_name(&mut self) -> String {
        if self.kind != ChooseKind::Save || self.is_path() {
            return String::new();
        }
        self.proposed = self.text.clone();
        self.text.clone()
    }

    /// The field says something else now: a name, or a filter's first match.
    /// Files selected stay, and the line under the list counts them.
    fn text_changed(&mut self) {
        self.highlight = 0;
        self.reveal_cell = true;
        self.error = None;
        if self.kind != ChooseKind::OpenMany {
            self.selected.clear();
        }
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
    /// first, equals in the picker's order); with a typed path, those of its folder its last part
    /// starts.
    pub fn visible(&mut self) -> Vec<Entry> {
        let entries = self.entries();
        if !self.ranks() {
            return entries;
        }
        // A group answers to its members' names too.
        let mut names = Vec::with_capacity(entries.len());
        for (n, e) in entries.iter().enumerate() {
            names.push((n, e.name.clone()));
            if let Some(group) = self.group_of(&e.path) {
                names.extend(group.member_names().into_iter().map(|m| (n, m)));
            }
        }
        let mut seen = HashSet::new();
        fuzzy::rank(&names, self.text.trim(), |(_, name)| name.clone())
            .into_iter()
            .filter(|(i, _)| seen.insert(names[*i].0))
            .map(|(i, _)| entries[names[i].0].clone())
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
                let paths: HashSet<&PathBuf> = shown.iter().map(|e| &e.path).collect();
                let hidden = self.selected.iter().filter(|p| !paths.contains(p)).count();
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
            "view": self.view.as_str(),
            "thumbnail_size": self.thumbnail_size,
            "columns": self.columns,
            "sort": self.sort.as_text(),
            "folders_first": self.folders_first,
            "choices": self.choice_values,
            "grouped": self.grouping_on(),
            "groups": shown.iter().take(64).filter_map(|e| {
                self.group_of(&e.path).map(|g| json!({
                    "main": e.name,
                    "members": g.member_names(),
                    "badges": g.badges,
                }))
            }).collect::<Vec<_>>(),
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
        let (shown, stand_in) = self.preview_of(&e, |path| previews.peek(&self.fs, path));
        let mut d = shown.describe();
        d["path"] = json!(e.path.display().to_string());
        if let Some(member) = stand_in {
            d["stand_in"] = json!(member.display().to_string());
        }
        d
    }

    /// The preview of an entry, by `ask`: its own, else, for a group's main file
    /// without one, the image of the first member that has one, and that member.
    /// A member is asked only then, in turn, and a sidecar's text never stands in
    /// for a photo.
    fn preview_of(
        &self,
        e: &Entry,
        mut ask: impl FnMut(&Path) -> preview::Shown,
    ) -> (preview::Shown, Option<PathBuf>) {
        let own = ask(&e.path);
        if !matches!(own, preview::Shown::Nothing | preview::Shown::Failed(_)) {
            return (own, None);
        }
        for member in self.group_of(&e.path).map_or(&[][..], |g| &g.members) {
            let shown = ask(member);
            match &shown {
                preview::Shown::Loading => return (shown, None),
                preview::Shown::Ready(ready)
                    if matches!(ready.preview.body, preview::Body::Image(_)) =>
                {
                    return (shown, Some(member.clone()));
                }
                _ => {}
            }
        }
        (own, None)
    }

    /// Draws the picker; its answer when given. Drawn to fill its room
    /// (`fill`, a dialog), its footer keeps its whole height at the
    /// bottom and what is above it scrolls when the room runs short
    /// ([`overlay::DialogLayout`]).
    pub fn show(&mut self, ui: &mut Ui, places: &Places) -> Option<PickerAnswer> {
        let mut answer = None;
        if self.fill {
            let layout = overlay::DialogLayout::new(ui, "egui_pop.files.picker").fill(true);
            layout.body(ui, |ui, height| {
                self.show_body(ui, places, Some(height), &mut answer);
            });
            layout.footer(ui, |ui| self.show_footer(ui, &mut answer));
        } else {
            self.show_body(ui, places, None, &mut answer);
            self.show_footer(ui, &mut answer);
        }
        answer
    }

    /// The picker above its footer: the toolbar, the path field, the
    /// places, the list and the preview; the list takes what `height`
    /// leaves (none: `LIST_HEIGHT`).
    fn show_body(
        &mut self,
        ui: &mut Ui,
        places: &Places,
        height: Option<f32>,
        answer: &mut Option<PickerAnswer>,
    ) {
        let top = ui.cursor().min.y;
        let t = Tokens::of(ui.ctx());
        self.ctx = Some(ui.ctx().clone());
        let first = !self.focused;
        self.focused = true;
        // Under its question: drawn, but neither the pointer nor the keys
        // are its own.
        let waiting = self.asking.is_some();
        if waiting {
            ui.disable();
        }
        let field_id = ui.scope_id().with("egui_pop.files.field");
        if !waiting {
            self.path_keys(ui);
        }
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
        // The grid's moves in two dimensions. The field keeps the keys that
        // move its cursor while it holds text and the focus.
        let grid = self.grid_on();
        let in_cell = self.focus_cell
            || ui
                .memory(|m| m.focused())
                .is_some_and(|f| self.row_ids.contains(&f));
        let editing = !empty && ui.memory(|m| m.has_focus(field_id));
        let (grid_move, grid_extend) = if grid && !waiting && !elsewhere && !editing {
            ui.input_mut(|i| {
                let mut moved = (None, false);
                for (key, to) in [
                    (Key::ArrowLeft, browser::ModeKey::Left),
                    (Key::ArrowRight, browser::ModeKey::Right),
                    (Key::Home, browser::ModeKey::Home),
                    (Key::End, browser::ModeKey::End),
                ] {
                    // Shift first: egui matches a key asked without it with
                    // it held too.
                    if i.consume_key(Modifiers::SHIFT, key) {
                        moved = (Some(to), true);
                    } else if i.consume_key(Modifiers::NONE, key) {
                        moved = (Some(to), false);
                    }
                }
                moved
            })
        } else {
            (None, false)
        };
        // Left and Right leave the folder in the columns, as the mode says.
        if self.columns_on() && !waiting && !elsewhere && !editing {
            let cols = &self.cols;
            let travel = ui.input_mut(|i| {
                [
                    (Key::ArrowLeft, browser::ModeKey::Left),
                    (Key::ArrowRight, browser::ModeKey::Right),
                ]
                .into_iter()
                .filter_map(|(key, mk)| cols.travel(mk).map(|t| (key, t)))
                .find(|(key, _)| i.consume_key(Modifiers::NONE, *key))
                .map(|(_, t)| t)
            });
            if let Some(t) = travel {
                self.travel(t);
            }
        }
        if grid && !waiting {
            let (bigger, smaller) = ui.input_mut(|i| {
                (
                    i.consume_key(Modifiers::COMMAND, Key::Plus)
                        || i.consume_key(Modifiers::COMMAND, Key::Equals),
                    i.consume_key(Modifiers::COMMAND, Key::Minus),
                )
            });
            if bigger {
                self.resize_thumbnails(self.thumbnail_size * THUMBNAIL_STEP);
            }
            if smaller {
                self.resize_thumbnails(self.thumbnail_size / THUMBNAIL_STEP);
            }
        }
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
        // Drawn on its own, the picker reads its commands' keys itself.
        if !waiting
            && !self.declared_keys
            && self.previews.is_some()
            && let Some(n) = browser::mode_key(ui)
        {
            self.set_mode_index(n);
        }
        if !waiting && !self.declared_keys {
            // Alt+P, as the preview pane of the Windows file explorer.
            if self.previews.is_some() && ui.input_mut(|i| i.consume_key(Modifiers::ALT, Key::P)) {
                self.show_preview = !self.show_preview;
            }
            for (modifiers, key, id) in [
                (Modifiers::COMMAND, Key::H, CMD_TOGGLE_HIDDEN),
                (Modifiers::NONE, Key::F5, CMD_RELOAD),
                (Modifiers::ALT, Key::ArrowLeft, CMD_BACK),
                (Modifiers::ALT, Key::ArrowRight, CMD_FORWARD),
            ] {
                if ui.input_mut(|i| i.consume_key(modifiers, key)) {
                    self.run_key(&id);
                }
            }
        }
        let preview_on = self.show_preview && self.previews.is_some();
        let bookmarked = places.bookmarks.contains(&self.dir);
        let mut crumbs_below = false;
        ui.horizontal(|ui| {
            let r = ui
                .button(strings::tr(ui.ctx(), &txt::PICKER_UP))
                .on_hover_text(keys_tip(&["Alt+Up".into()]));
            if r.clicked() {
                self.up();
                self.refocus |= by_pointer(&r);
            }
            let label = if bookmarked {
                &txt::FILES_UNBOOKMARK
            } else {
                &txt::FILES_BOOKMARK
            };
            let r = ui.button(strings::tr(ui.ctx(), label));
            if r.clicked() {
                *answer = Some(PickerAnswer::Bookmark(self.dir.clone(), !bookmarked));
                self.refocus |= by_pointer(&r);
            }
            if self.makes_folders() {
                self.new_folder_button(ui);
            }
            if self.views().len() > 1 {
                self.view_controls(ui);
            }
            if self.can_group() {
                self.group_control(ui);
            }
            crumbs_below = ui.available_width() < CRUMBS_MIN_WIDTH;
            if !crumbs_below {
                self.breadcrumbs(ui);
            }
        });
        if crumbs_below {
            ui.horizontal(|ui| self.breadcrumbs(ui));
        }
        let mut back_to_field = false;
        if self.naming.is_some() {
            back_to_field = self.naming_row(ui, answer);
        }
        if bookmark {
            *answer = Some(PickerAnswer::Bookmark(self.dir.clone(), !bookmarked));
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
        egui_pop_core::a11y::name(&field, egui::Role::TextInput, &strings::tr(ui.ctx(), hint));
        let refocus = std::mem::take(&mut self.refocus);
        // In the grid, the arrows leave the focus where it is: on a cell, it
        // follows the highlight.
        let cell_stays = grid && in_cell;
        let arrows = down || up;
        if first || tab || parent || (arrows && !cell_stays) || enter || back_to_field || refocus {
            field.request_focus();
            self.focus_cell = false;
        } else if (arrows || grid_move.is_some()) && cell_stays {
            self.focus_cell = true;
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
            self.text_changed();
        }
        let completed = tab && self.complete();
        if completed {
            self.highlight = 0;
        }
        if std::mem::take(&mut self.cursor_to_end) || completed {
            let mut state = egui::TextEdit::load_state(ui.ctx(), field.id).unwrap_or_default();
            let end = egui::text::CCursor::new(self.text.chars().count());
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(end)));
            state.store(ui.ctx(), field.id);
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
        let key = grid_move.or(if down {
            Some(browser::ModeKey::Down)
        } else if up {
            Some(browser::ModeKey::Up)
        } else {
            None
        });
        if let Some(key) = key {
            // Choosing a folder the list opens with nothing selected: the first
            // press selects the entry highlighted, rather than the one after it.
            // Otherwise the mode shown says where the key goes, by its layout.
            let to = if self.kind == ChooseKind::Folder && self.selected.is_empty() {
                self.highlight
            } else {
                let mode: &dyn browser::ViewMode = if grid { &self.thumbs } else { &self.details };
                mode.step(self.highlight, shown.len(), key)
            };
            self.move_highlight(to, &shown, extend || grid_extend);
            self.reveal_cell = true;
        }
        ui.add_space(t.space.s);
        // The list takes what the room leaves under what is above it (the
        // footer is kept apart, see `show`); at its least, the body scrolls.
        let list_height = match height {
            Some(h) => (h - (ui.cursor().min.y - top)).max(ui.spacing().interact_size.y),
            None => LIST_HEIGHT,
        };
        ui.horizontal_top(|ui| {
            // Places.
            ui.vertical(|ui| {
                ui.set_width(PLACES_WIDTH);
                ui.scroll_area(
                    egui::ScrollArea::vertical()
                        .id_salt("egui_pop.files.places")
                        .max_height(list_height)
                        .auto_shrink([false, true]),
                    |ui| {
                        if let Some(a) = self.side_bar(ui, places) {
                            *answer = Some(a);
                        }
                    },
                );
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
                let id = ui.scope_id().with("egui_pop.files.splitter");
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
                let spacing = ui.spacing().item_spacing.y;
                let row = row_height(ui);
                let height = (list_height - row - spacing).max(0.0);
                // The scroll bar takes its room only when there is one.
                let scrolls = !grid && shown.len() as f32 * (row + spacing) > height;
                let bar = ui.spacing().scroll.allocated_width();
                let row_width = list_width - if scrolls { bar } else { 0.0 };
                let columns = list_columns(ui, row_width, self.kind);
                self.entries_view(ui, shown, &columns, row_width, list_width, height, answer);
            });
            if let Some(pane_width) = pane_width {
                ui.add_space(t.space.m);
                ui.vertical(|ui| {
                    ui.set_width(pane_width);
                    // As high as the list, whatever the preview's facts.
                    ui.scroll_area(
                        egui::ScrollArea::vertical()
                            .id_salt("egui_pop.files.pane")
                            .max_height(list_height)
                            .auto_shrink([false, true]),
                        |ui| {
                            // Named, for a screen reader to find it.
                            let name = strings::tr(ui.ctx(), &txt::FILES_PREVIEW);
                            ui.scope_builder(
                                egui::UiBuilder::new()
                                    .accessibility_role(egui::accesskit::Role::Group)
                                    .accessibility_label(name),
                                |ui| self.preview_pane(ui),
                            );
                        },
                    );
                });
            }
        });
        if !grid && self.asked_thumbnails {
            self.want_thumbnails(ui.ctx(), &[]);
        }
        if toggle {
            self.toggle();
        }
        if select_all {
            // A refusal is the picker's error, shown below the list.
            let _ = self.select_all();
        }
        if enter && answer.is_none() {
            *answer = self.activate();
        }
        if accept && answer.is_none() {
            *answer = self.accept();
        }
    }

    /// The picker's footer: an error, the selection's line, the choices,
    /// the filters, the toggles and the buttons.
    fn show_footer(&mut self, ui: &mut Ui, answer: &mut Option<PickerAnswer>) {
        let t = Tokens::of(ui.ctx());
        if let Some(e) = &self.error {
            ui.colored_label(t.status.danger, e);
        }
        // A folder shown anew is said, with how many entries it shows.
        if self.said_dir.as_ref() != Some(&self.dir) {
            if self.said_dir.is_some() {
                let count = self.visible().len().to_string();
                let said = strings::tr_args(
                    ui.ctx(),
                    &txt::FILES_FOLDER_OPENED,
                    &[("folder", &folder_name(&self.dir)), ("count", &count)],
                );
                a11y::announce(ui.ctx(), said, Politeness::Polite);
            }
            self.said_dir = Some(self.dir.clone());
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
        if !self.choices.is_empty() {
            ui.add_space(t.space.s);
            self.choice_row(ui);
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
            )
            .on_hover_text(keys_tip(&self.hidden_keys));
            let mut folders_first = self.folders_first;
            widgets::checkbox(
                ui,
                &mut folders_first,
                strings::tr(ui.ctx(), &txt::FILES_FOLDERS_FIRST),
            );
            self.set_folders_first(folders_first);
            if self.previews.is_some() {
                widgets::checkbox(
                    ui,
                    &mut self.show_preview,
                    strings::tr(ui.ctx(), &txt::FILES_PREVIEW),
                )
                .on_hover_text(keys_tip(&self.preview_keys));
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
                    *answer = answer.take().or_else(|| self.accept());
                }
                if ui.button(strings::tr(ui.ctx(), &CANCEL)).clicked() {
                    *answer = Some(PickerAnswer::Cancel);
                }
            });
        });
    }
}

impl FilePicker {
    /// Whether the entries are drawn as a grid: it needs the providers.
    fn grid_on(&self) -> bool {
        self.view == PickerView::Grid && self.previews.is_some()
    }

    /// The columns are shown: choosing a folder, in that view.
    fn columns_on(&self) -> bool {
        self.view == PickerView::Columns && self.kind == ChooseKind::Folder
    }

    /// A new size for the thumbnails; the highlighted cell stays in view.
    fn resize_thumbnails(&mut self, size: f32) {
        self.thumbnail_size = size.clamp(THUMBNAIL_MIN_SIZE, THUMBNAIL_MAX_SIZE);
        self.reveal_cell = true;
    }

    /// The application's choices, on a line of their own above the buttons:
    /// a check box, or a label and its combo box.
    fn choice_row(&mut self, ui: &mut Ui) {
        let values = &mut self.choice_values;
        ui.horizontal_wrapped(|ui| {
            for c in &self.choices {
                let value = values
                    .entry(c.id.clone())
                    .or_insert_with(|| c.default.clone());
                if c.is_check() {
                    let mut on = value == "true";
                    if widgets::checkbox(ui, &mut on, c.label.as_str()).changed() {
                        *value = on.to_string();
                    }
                    continue;
                }
                ui.label(c.label.as_str());
                let shown = c
                    .options
                    .iter()
                    .find(|(id, _)| id == value)
                    .map_or(value.as_str(), |(_, label)| label.as_str())
                    .to_owned();
                let combo = widgets::combo_box(("egui_pop.files.choice", c.id.as_str()))
                    .selected_text(shown);
                let list = ui.combo_box(combo, |ui| {
                    for (id, label) in &c.options {
                        widgets::selectable_value(ui, value, id.clone(), label.as_str());
                    }
                });
                // The label beside it is not its name for a screen reader.
                let name = c.label.clone();
                ui.ctx()
                    .accesskit_node_builder(list.response.id, |node| node.set_label(name));
            }
        });
    }

    /// A place of the side bar: a click opens its folder.
    fn place(&mut self, ui: &mut Ui, name: &str, dir: &Path) -> egui::Response {
        let r = widgets::selectable_label(ui, self.dir == dir, name)
            .on_hover_text(dir.display().to_string());
        if r.clicked() {
            self.go(dir.to_path_buf());
        }
        r
    }

    /// The side bar: home, the system's folders, the volumes, recent places
    /// and bookmarks, each bookmark with a menu to rename, move or remove it.
    fn side_bar(&mut self, ui: &mut Ui, places: &Places) -> Option<PickerAnswer> {
        let t = Tokens::of(ui.ctx());
        let ctx = ui.ctx().clone();
        let mut out = None;
        if let Some(h) = &places.home {
            self.place(ui, &strings::tr(&ctx, &txt::FILES_HOME), h);
        }
        for (folder, dir) in &places.system.folders {
            self.place(ui, &strings::tr(&ctx, folder.title()), dir);
        }
        if !places.system.volumes.is_empty() {
            ui.add_space(t.space.s);
            ui.weak(strings::tr(&ctx, &txt::FILES_DEVICES));
            for v in &places.system.volumes {
                self.place(ui, &v.name, &v.path);
            }
        }
        if !places.recent.is_empty() {
            ui.add_space(t.space.s);
            ui.weak(strings::tr(&ctx, &txt::FILES_RECENT));
            for d in &places.recent {
                self.place(ui, &folder_name(d), d);
            }
        }
        if places.bookmarks.is_empty() {
            return None;
        }
        ui.add_space(t.space.s);
        ui.weak(strings::tr(&ctx, &txt::FILES_BOOKMARKS));
        let last = places.bookmarks.len() - 1;
        for (i, d) in places.bookmarks.iter().enumerate() {
            if self.renaming.as_ref().is_some_and(|(at, _)| at == d) {
                if let Some(a) = self.rename_field(ui, d) {
                    out = Some(a);
                }
                continue;
            }
            let name = places.bookmark_name(d);
            let r = self.place(ui, &name, d);
            egui_pop_core::pointer::claim::context_menu(&r, |ui| {
                if ui
                    .button(strings::tr(&ctx, &txt::FILES_RENAME_BOOKMARK))
                    .clicked()
                {
                    self.renaming = Some((d.clone(), name.clone()));
                    self.renaming_focus = true;
                    ui.close();
                }
                if i > 0 && ui.button(strings::tr(&ctx, &txt::FILES_MOVE_UP)).clicked() {
                    out = Some(PickerAnswer::MoveBookmark(d.clone(), i - 1));
                    ui.close();
                }
                if i < last
                    && ui
                        .button(strings::tr(&ctx, &txt::FILES_MOVE_DOWN))
                        .clicked()
                {
                    out = Some(PickerAnswer::MoveBookmark(d.clone(), i + 1));
                    ui.close();
                }
                if ui
                    .button(strings::tr(&ctx, &txt::FILES_UNBOOKMARK))
                    .clicked()
                {
                    out = Some(PickerAnswer::Bookmark(d.clone(), false));
                    ui.close();
                }
            });
        }
        out
    }

    /// The field renaming a bookmark, in its place: Enter, or leaving it,
    /// keeps the name; Escape keeps the old one.
    fn rename_field(&mut self, ui: &mut Ui, dir: &Path) -> Option<PickerAnswer> {
        let ctx = ui.ctx().clone();
        let id = ui.make_persistent_id(("egui_pop.files.rename", dir));
        let label = strings::tr(&ctx, &txt::FILES_BOOKMARK_NAME);
        let escaped = focus::escape_left(&ctx, id)
            || (ui.memory(|m| m.has_focus(id))
                && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)));
        let (_, text) = self.renaming.as_mut()?;
        let edit = ui.add(
            egui::TextEdit::singleline(text)
                .id(id)
                .desired_width(f32::INFINITY),
        );
        egui_pop_core::a11y::name(&edit, egui::Role::TextInput, &label);
        if std::mem::take(&mut self.renaming_focus) {
            edit.request_focus();
            // Its text selected, to type the new name over it.
            let mut state = egui::text_edit::TextEditState::load(&ctx, id).unwrap_or_default();
            let all = egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(text.chars().count()),
            );
            state.cursor.set_char_range(Some(all));
            state.store(&ctx, id);
            return None;
        }
        if escaped {
            self.renaming = None;
            return None;
        }
        if edit.lost_focus() {
            let (dir, name) = self.renaming.take()?;
            return Some(PickerAnswer::RenameBookmark(dir, name));
        }
        None
    }

    /// The views it offers, in the switcher's order: details; thumbnails
    /// with previews; columns when choosing a folder.
    pub fn views(&self) -> Vec<PickerView> {
        PickerView::ALL
            .into_iter()
            .filter(|v| match v {
                PickerView::List => true,
                PickerView::Grid => self.previews.is_some(),
                PickerView::Columns => self.kind == ChooseKind::Folder,
            })
            .collect()
    }

    /// The modes it offers, in the switcher's order ([`Self::views`]).
    fn mode_infos(&self) -> Vec<browser::ModeInfo> {
        let modes = browser::ViewModes::built_in();
        self.views()
            .iter()
            .filter_map(|v| modes.make(v.as_str()))
            .map(|m| m.describe())
            .collect()
    }

    /// Shows the `n`-th mode (from 1, among [`Self::views`]), as `Ctrl+n`
    /// does; `false` without such a mode, or with one view alone.
    pub fn set_mode_index(&mut self, n: usize) -> bool {
        let views = self.views();
        let Some(view) = n.checked_sub(1).and_then(|i| views.get(i).copied()) else {
            return false;
        };
        if views.len() < 2 || self.view == view {
            return false;
        }
        self.view = view;
        self.reveal_cell = true;
        true
    }

    /// The mode switcher, and the thumbnails' size with the thumbnails.
    fn view_controls(&mut self, ui: &mut Ui) {
        let id = ui.scope_id().with("egui_pop.files.modes");
        let modes = self.mode_infos();
        if let Some((mode, r)) = browser::mode_switcher(ui, id, &modes, self.view.as_str()) {
            if let Some(view) = PickerView::parse(&mode)
                && view != self.view
            {
                self.view = view;
                self.reveal_cell = true;
            }
            // A press leaves nothing focused: typing goes on filtering.
            self.refocus |= by_pointer(&r);
        }
        self.thumbnail_dragging = false;
        if self.view != PickerView::Grid {
            return;
        }
        let name = strings::tr(ui.ctx(), &txt::FILES_THUMBNAIL_SIZE);
        let height = ui.spacing().interact_size.y;
        ui.allocate_ui(vec2(THUMBNAIL_SLIDER_WIDTH, height), |ui| {
            let mut size = f64::from(self.thumbnail_size);
            let range = f64::from(THUMBNAIL_MIN_SIZE)..=f64::from(THUMBNAIL_MAX_SIZE);
            let out = Slider::new(&mut size, &name, range)
                .step(1.0)
                .default(f64::from(THUMBNAIL_DEFAULT_SIZE))
                .show(ui);
            if out.response.changed() {
                self.resize_thumbnails(size as f32);
            }
            self.thumbnail_dragging = out.response.dragged();
        });
    }

    /// The switch that groups files or lets each be an entry of its own, the way
    /// to a member with the pointer. Only with a grouping to switch.
    fn group_control(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let name = strings::tr(&ctx, &txt::FILES_GROUP);
        let r = widgets::selectable_label(ui, self.grouped, name)
            .on_hover_text(strings::tr(&ctx, &txt::FILES_GROUP_TIP));
        if r.clicked() {
            self.set_grouped(!self.grouped);
            // A press leaves nothing focused: typing goes on filtering.
            self.refocus |= by_pointer(&r);
        }
    }

    /// The keys that put a path into the field, read before anything is drawn:
    /// Ctrl+L, and `/` or `~` typed while an entry has the focus (by what they
    /// type: the keys making them differ from layout to layout).
    fn path_keys(&mut self, ui: &mut Ui) {
        if !self.declared_keys && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::L)) {
            self.edit_path();
        }
        if !ui
            .memory(|m| m.focused())
            .is_some_and(|f| self.row_ids.contains(&f))
        {
            return;
        }
        let typed = ui.input_mut(|i| {
            let at = i
                .events
                .iter()
                .position(|e| matches!(e, egui::Event::Text(t) if t == "/" || t == "~"))?;
            match i.events.remove(at) {
                egui::Event::Text(t) => Some(t),
                _ => None,
            }
        });
        if let Some(typed) = typed {
            self.start_path(&typed);
            ui.ctx().request_repaint();
        }
    }

    /// The folder as breadcrumbs, in the room left in the row: each segment opens
    /// its folder, the chevron after it lists its subfolders. What does not fit
    /// is the start of the path, behind a button listing it; the deepest segment
    /// is cut only if it does not fit alone.
    fn breadcrumbs(&mut self, ui: &mut Ui) {
        let t = Tokens::of(ui.ctx());
        let ctx = ui.ctx().clone();
        let crumbs = crumbs_of(&self.dir);
        ui.spacing_mut().item_spacing.x = 0.0;
        let room = ui.available_width();
        let padding = 2.0 * ui.spacing().button_padding.x;
        let font = egui::TextStyle::Button.resolve(ui.style());
        let text_width = |text: &str| {
            ui.painter()
                .layout_no_wrap(text.to_string(), font.clone(), egui::Color32::PLACEHOLDER)
                .size()
                .x
        };
        let chevron = vec2(
            t.size.icon_inline + t.space.xs,
            ui.spacing().interact_size.y,
        );
        // Each as wide as it will be drawn, its chevron with it; the deepest is
        // strong.
        let name_widths: Vec<f32> = crumbs
            .iter()
            .enumerate()
            .map(|(n, c)| {
                let width = if n + 1 == crumbs.len() {
                    let strong = egui::RichText::new(&c.name).strong();
                    egui::WidgetText::from(strong)
                        .into_galley(
                            ui,
                            Some(egui::TextWrapMode::Extend),
                            f32::INFINITY,
                            font.clone(),
                        )
                        .size()
                        .x
                } else {
                    text_width(&c.name)
                };
                // Rounded up: a fraction over would push the row out.
                (width + padding).ceil()
            })
            .collect();
        let widths: Vec<f32> = name_widths.iter().map(|w| w + chevron.x).collect();
        let collapsed_width = (text_width("…") + padding).ceil();
        let first = first_crumb(&widths, room, collapsed_width);
        let mut chosen = None;
        if first > 0 {
            let name = strings::tr(&ctx, &txt::FILES_CRUMB_COLLAPSED);
            let r = ui
                .add(egui::Button::new("…").frame_when_inactive(false))
                .on_hover_text(&name);
            a11y::name(&r, egui::Role::Button, &name);
            let above: Vec<Crumb> = crumbs[..first].iter().rev().cloned().collect();
            chosen = self.crumb_menu(ui, &r, move |_| Some(above));
        }
        let left = if first > 0 {
            room - collapsed_width
        } else {
            room
        };
        for (n, crumb) in crumbs.iter().enumerate().skip(first) {
            let current = n + 1 == crumbs.len();
            let mut text = egui::RichText::new(&crumb.name);
            if current {
                text = text.strong();
            }
            let button = egui::Button::new(text).frame_when_inactive(false);
            let fits = (left - chevron.x).max(chevron.x);
            // By its path: the focus stays on it as the start collapses.
            let r = ui
                .push_id((&crumb.path, "open"), |ui| {
                    if current && name_widths[n] > fits {
                        ui.allocate_ui(vec2(fits, chevron.y), |ui| ui.add(button.truncate()))
                            .inner
                    } else {
                        ui.add(button)
                    }
                })
                .inner;
            let name = strings::tr_args(&ctx, &txt::FILES_CRUMB_OPEN, &[("name", &crumb.name)]);
            a11y::name(&r, egui::Role::Button, &name);
            if current {
                let shown = strings::tr(&ctx, &txt::FILES_CRUMB_CURRENT);
                ctx.accesskit_node_builder(r.id, |node| node.set_description(shown));
            }
            let r = r.on_hover_text(crumb.path.display().to_string());
            if r.clicked() {
                chosen = Some(crumb.path.clone());
            }
            let name =
                strings::tr_args(&ctx, &txt::FILES_CRUMB_SUBFOLDERS, &[("name", &crumb.name)]);
            let r = ui
                .push_id(&crumb.path, |ui| {
                    ui.add(
                        egui::Button::new("")
                            .min_size(chevron)
                            .frame_when_inactive(false),
                    )
                })
                .inner
                .on_hover_text(&name);
            a11y::name(&r, egui::Role::Button, &name);
            let dir = crumb.path.clone();
            let popup = egui::Popup::default_response_id(&r);
            let color = ui.style().interact(&r).text_color();
            let icon = if egui::Popup::is_id_open(&ctx, popup) {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            };
            let side = t.size.icon_inline.min(r.rect.height());
            icon.paint(
                ui.painter(),
                egui::Rect::from_center_size(r.rect.center(), vec2(side, side)),
                color,
            );
            chosen = self
                .crumb_menu(ui, &r, move |picker| picker.subfolders(&dir))
                .or(chosen);
        }
        if let Some(dir) = chosen {
            // The folder shown, asked again: it is read again.
            if dir == self.dir {
                self.listing = None;
            }
            self.go(dir);
            self.refocus = true;
            ctx.request_repaint();
        }
    }

    /// A folder's subfolders by name, hidden ones only when the list shows them.
    /// None when it cannot be read.
    fn subfolders(&self, dir: &Path) -> Option<Vec<Crumb>> {
        let mut names: Vec<String> = self
            .fs
            .read_dir(dir)
            .ok()?
            .into_iter()
            .filter(|e| e.is_dir && (self.show_hidden || !e.name.starts_with('.')))
            .map(|e| e.name)
            .collect();
        names.sort_by(|a, b| sort::natural_cmp(a, b));
        Some(
            names
                .into_iter()
                .map(|name| Crumb {
                    path: dir.join(&name),
                    name,
                })
                .collect(),
        )
    }

    /// A key the columns' mode reads as leaving the folder shown: into the
    /// highlighted folder (the selection left behind, so the folder entered
    /// is the choice), or back to the parent, the folder left selected.
    fn travel(&mut self, travel: browser::Travel) {
        match travel {
            browser::Travel::Into => {
                if let Some(e) = self.highlighted().filter(|e| e.is_dir) {
                    self.go(e.path);
                }
            }
            browser::Travel::Out => {
                if let Some(parent) = self.dir.parent().map(Path::to_path_buf) {
                    let left = self.dir.clone();
                    self.reach(parent, Some(left));
                }
            }
        }
    }

    /// Shows `place`, `focus` highlighted and selected there: a click in
    /// another column than the folder shown's.
    fn reach(&mut self, place: PathBuf, focus: Option<PathBuf>) {
        self.go(place);
        let Some(focus) = focus else {
            return;
        };
        if let Some(at) = self.visible().iter().position(|e| e.path == focus) {
            self.highlight = at;
            self.selected.clear();
            self.selected.insert(focus);
        }
    }

    /// The menu under a breadcrumb's `button`, listing what `read` gives as it
    /// opens: the folder chosen. By the keyboard it is walked with the arrows,
    /// Tab or Escape close it, and the focus goes back to the button; by the
    /// pointer, the focus goes back to the field, for typing to go on.
    fn crumb_menu(
        &mut self,
        ui: &Ui,
        button: &egui::Response,
        read: impl FnOnce(&Self) -> Option<Vec<Crumb>>,
    ) -> Option<PathBuf> {
        let ctx = ui.ctx().clone();
        let popup =
            egui::Popup::menu(button).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
        let id = popup.get_id();
        let was_open = egui::Popup::is_id_open(&ctx, id);
        if button.clicked() && !was_open {
            let items = read(self);
            self.crumb_menu_keys = !by_pointer(button);
            self.crumb_menu_focus = self.crumb_menu_keys;
            // Nothing to walk: what the menu says is read out.
            let empty = match &items {
                None => Some(&txt::FILES_CRUMB_UNREADABLE),
                Some(list) if list.is_empty() => Some(&txt::FILES_CRUMB_NONE),
                Some(_) => None,
            };
            if let Some(empty) = empty {
                a11y::announce(&ctx, strings::tr(&ctx, empty), Politeness::Polite);
            }
            self.crumb_menu = Some(Arc::new(items));
        }
        ctx.accesskit_node_builder(button.id, |node| {
            node.set_has_popup(egui::accesskit::HasPopup::Menu);
            node.set_expanded(was_open);
        });
        let items = self.crumb_menu.clone();
        let mut chosen = None;
        let mut leave = false;
        popup.show(|ui| {
            let Some(items) = items.as_deref() else {
                return;
            };
            let Some(list) = items.as_ref() else {
                ui.weak(strings::tr(ui.ctx(), &txt::FILES_CRUMB_UNREADABLE));
                return;
            };
            if list.is_empty() {
                ui.weak(strings::tr(ui.ctx(), &txt::FILES_CRUMB_NONE));
                return;
            }
            // As wide as its longest name, up to a limit: its items fill the menu.
            let font = egui::TextStyle::Button.resolve(ui.style());
            let widest = list.iter().fold(0.0_f32, |widest, item| {
                let name = item.name.clone();
                let galley =
                    ui.painter()
                        .layout_no_wrap(name, font.clone(), egui::Color32::PLACEHOLDER);
                widest.max(galley.size().x)
            });
            let room = widest
                + 2.0 * ui.spacing().button_padding.x
                + ui.spacing().scroll.allocated_width();
            ui.set_width(room.ceil().min(CRUMB_MENU_WIDTH));
            let on_path = list.iter().position(|c| self.dir.starts_with(&c.path));
            let mut ids = Vec::with_capacity(list.len());
            ui.scroll_area(
                egui::ScrollArea::vertical()
                    .id_salt("egui_pop.files.crumb_menu")
                    .max_height(CRUMB_MENU_HEIGHT),
                |ui| {
                    for (n, item) in list.iter().enumerate() {
                        let r = ui.add(
                            egui::Button::new(&item.name)
                                .selected(on_path == Some(n))
                                .truncate(),
                        );
                        a11y::name(&r, egui::Role::Button, &item.name);
                        if r.clicked() {
                            chosen = Some(item.path.clone());
                        }
                        ids.push(r.id);
                    }
                },
            );
            if chosen.is_some() {
                ui.close();
            }
            // The sizing pass draws nothing to focus.
            if self.crumb_menu_focus && !ui.is_sizing_pass() {
                self.crumb_menu_focus = false;
                ui.memory_mut(|m| m.request_focus(ids[on_path.unwrap_or(0)]));
                ui.ctx().request_repaint();
            }
            if let Some(at) = ui
                .memory(|m| m.focused())
                .and_then(|f| ids.iter().position(|i| *i == f))
            {
                // The arrows stay in the menu, and Tab does not walk on under it.
                ui.memory_mut(|m| {
                    m.set_focus_lock_filter(
                        ids[at],
                        EventFilter {
                            tab: true,
                            vertical_arrows: true,
                            ..Default::default()
                        },
                    )
                });
                let (down, up, tab) = ui.input_mut(|i| {
                    (
                        i.consume_key(Modifiers::NONE, Key::ArrowDown),
                        i.consume_key(Modifiers::NONE, Key::ArrowUp),
                        i.consume_key(Modifiers::SHIFT, Key::Tab)
                            || i.consume_key(Modifiers::NONE, Key::Tab),
                    )
                });
                let to = if down {
                    Some((at + 1) % ids.len())
                } else if up {
                    Some((at + ids.len() - 1) % ids.len())
                } else {
                    None
                };
                if let Some(to) = to {
                    ui.memory_mut(|m| m.request_focus(ids[to]));
                    ui.ctx().request_repaint();
                }
                if tab {
                    leave = true;
                    ui.close();
                }
            }
        });
        if was_open && !egui::Popup::is_id_open(&ctx, id) {
            self.crumb_menu = None;
            if chosen.is_none() {
                if self.crumb_menu_keys || leave {
                    button.request_focus();
                } else {
                    self.refocus = true;
                }
                ctx.request_repaint();
            }
        }
        chosen
    }

    /// The headers above the entries, in the list and the grid alike: a click
    /// sorts, and the arrow says by what. While the field's filter ranks the
    /// entries, the order is not theirs to give, and they say so.
    fn column_headers(&mut self, ui: &mut Ui, columns: &[ListColumn], width: f32) {
        let ctx = ui.ctx().clone();
        let height = row_height(ui);
        let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        let ranks = self.ranks();
        let kit = Kit::of(&ctx);
        let font = egui::TextStyle::Body.resolve(ui.style());
        let pointer = ui.input(|i| i.pointer.any_down() || i.pointer.any_released());
        let inset = header_inset(ui, &kit, &font);
        ui.add_enabled_ui(!ranks, |ui| {
            for (n, c) in columns.iter().enumerate() {
                let left = (rect.left() + c.left - inset).max(rect.left());
                let right = columns
                    .get(n + 1)
                    .map_or(rect.right(), |next| rect.left() + next.left - inset);
                let at =
                    egui::Rect::from_min_max(pos2(left, rect.top()), pos2(right, rect.bottom()));
                let title = strings::tr(&ctx, c.column.title());
                let descending =
                    (!ranks && self.sort.column == c.column).then_some(self.sort.descending);
                let name = match descending {
                    Some(true) => {
                        strings::tr_args(&ctx, &txt::FILES_SORTED_DOWN, &[("column", &title)])
                    }
                    Some(false) => {
                        strings::tr_args(&ctx, &txt::FILES_SORTED_UP, &[("column", &title)])
                    }
                    None => title.clone(),
                };
                let id = ui
                    .scope_id()
                    .with("egui_pop.files.header")
                    .with(c.column.as_str());
                let r = widgets::SortHeader {
                    title: &title,
                    name: &name,
                    descending,
                    sortable: true,
                }
                .show(ui, &kit, at, id)
                .on_disabled_hover_text(strings::tr(&ctx, &txt::FILES_SORT_RANKED));
                if r.clicked() {
                    self.set_sort(self.sort.click(c.column));
                    // By the pointer: the keys go on as before.
                    self.refocus |= pointer;
                }
            }
        });
    }

    /// The entries, drawn by the mode shown (`details` or `thumbnails`)
    /// through the picker's drawer: its column headers, its rows and cells,
    /// the groups' badges, the highlight the field drives, and its clicks.
    #[allow(clippy::too_many_arguments)]
    fn entries_view(
        &mut self,
        ui: &mut Ui,
        shown: Vec<Entry>,
        columns: &[ListColumn],
        row_width: f32,
        list_width: f32,
        height: f32,
        answer: &mut Option<PickerAnswer>,
    ) {
        let grid = self.grid_on();
        if grid {
            // `Ctrl` and the wheel over the grid change the thumbnails' size.
            let area =
                egui::Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), height));
            if ui.rect_contains_pointer(area) {
                let zoom = ui.input(|i| i.zoom_delta());
                if (zoom - 1.0).abs() > f32::EPSILON {
                    self.resize_thumbnails(self.thumbnail_size * zoom);
                }
            }
        }
        let reveal = std::mem::take(&mut self.reveal_cell) && !shown.is_empty();
        let highlight = self.highlight.min(shown.len().saturating_sub(1));
        let entries: Vec<browser::SourceEntry> = shown
            .iter()
            .map(|e| browser::SourceEntry {
                path: e.path.clone(),
                name: e.name.clone(),
                kind: if e.is_dir {
                    browser::EntryKind::Folder
                } else {
                    browser::EntryKind::File
                },
                size: None,
                modified: None,
                taken: None,
                app_id: None,
            })
            .collect();
        let selection = browser::Selection {
            paths: self.selected.clone(),
            anchor: self.anchor.clone(),
        };
        let mut settings = browser::ModeSettings {
            thumbnail_size: self.thumbnail_size,
            ..browser::ModeSettings::default()
        };
        let name = strings::tr_args(
            ui.ctx(),
            &txt::FILES_ENTRIES_OF,
            &[("folder", &folder_name(&self.dir))],
        );
        let many = self.kind == ChooseKind::OpenMany;
        self.row_ids.clear();
        let mut details = std::mem::take(&mut self.details);
        let mut thumbs = std::mem::take(&mut self.thumbs);
        let mut cols = std::mem::take(&mut self.cols);
        let place = self.dir.clone();
        let columns_on = self.columns_on();
        // The columns beside the folder shown list off the interface
        // thread: their entries stream in, a busy row while they do.
        let column_cache = self.column_cache.clone();
        let mut side = lock(&column_cache);
        if columns_on {
            let mut wanted: Vec<PathBuf> = self
                .dir
                .ancestors()
                .skip(1)
                .map(Path::to_path_buf)
                .collect();
            wanted.reverse();
            let peek = entries
                .get(highlight)
                .filter(|e| e.is_dir())
                .map(|e| e.path.clone());
            let source = browser::FsSource::new(self.fs.clone());
            let options = browser::ListOptions {
                hidden: self.show_hidden,
            };
            if side.sync(&source, &options, &wanted, &self.dir, peek.as_deref()) {
                ui.ctx().request_repaint();
            }
        } else {
            side.clear();
        }
        let (ancestors, peek) = if columns_on {
            (side.ancestors(), side.peek())
        } else {
            (&[][..], None)
        };
        // A column holds the names alone.
        let name_only;
        let (columns, row_width) = if columns_on {
            let width = browser::ColumnsMode::COLUMN_WIDTH - ui.spacing().scroll.allocated_width();
            name_only = fit_columns(
                width,
                Tokens::of(ui.ctx()).space.m,
                ui.spacing().button_padding.x,
                &[],
            );
            (name_only.as_slice(), width)
        } else {
            (columns, row_width)
        };
        let mut drawer = PickerDrawer {
            modifiers: ui.input(|i| i.modifiers),
            pointer: ui.input(|i| i.pointer.any_down() || i.pointer.any_released()),
            picker: self,
            shown,
            columns,
            row_width,
            list_width,
            answer,
            in_view: Vec::new(),
        };
        let cx = browser::ModeContext {
            entries: &entries,
            selection: &selection,
            focus: (!entries.is_empty()).then_some(highlight),
            thumbnails: &browser::NoThumbnails,
            settings: &mut settings,
            id: egui::Id::unique("egui_pop.files.entries"),
            reveal_focus: reveal,
            max_height: Some(height),
            drawer: Some(&mut drawer),
            name: Some(name),
            many,
            place: &place,
            ancestors,
            peek,
            loading: false,
            generation: 0,
        };
        let out = if columns_on {
            cols.show(ui, cx)
        } else if grid {
            thumbs.show(ui, cx)
        } else {
            details.show(ui, cx)
        };
        drop(side);
        let in_view = std::mem::take(&mut drawer.in_view);
        self.details = details;
        self.thumbs = thumbs;
        self.cols = cols;
        self.columns = out.columns;
        if let Some(r) = out.reach {
            self.reach(r.place, r.focus);
            self.refocus = true;
            ui.ctx().request_repaint();
        }
        if grid && (!in_view.is_empty() || self.asked_thumbnails) {
            self.want_thumbnails(ui.ctx(), &in_view);
        }
    }

    /// Draws row `n`: a selectable row, named by the entry's name, its other
    /// columns said as its description.
    fn list_row(
        &mut self,
        ui: &mut Ui,
        e: &Entry,
        n: usize,
        columns: &[ListColumn],
    ) -> egui::Response {
        let t = Tokens::of(ui.ctx());
        let label = if e.is_dir {
            format!("{}/", e.name)
        } else {
            e.name.clone()
        };
        // Where a selection exists apart from the highlight, a ring says where
        // the highlight is.
        let on = self.selected.contains(&e.path) || (n == self.highlight && !self.selects_apart());
        // Inside the row's padding.
        let height = row_height(ui) - 2.0 * ui.spacing().button_padding.y;
        let mut atoms = egui::Atoms::default();
        let mut facts = Vec::new();
        let group = self.group_of(&e.path).cloned();
        let name_column = columns.first().map_or(0.0, |c| c.width);
        // The badges keep to the end of the name's column, and at most half of it.
        let marks = group
            .as_ref()
            .map_or_else(Vec::new, |g| group_marks(ui, g, name_column / 2.0));
        let marks_room = marks_width(ui, &marks);
        let reserved = if marks.is_empty() {
            0.0
        } else {
            marks_room + t.space.m
        };
        if let Some(group) = &group {
            facts.push(group_said(ui.ctx(), group));
        }
        for c in columns {
            let (text, align) = match c.column {
                SortColumn::Name => {
                    let width = (c.width - reserved).max(0.0);
                    atoms.push_right(
                        label
                            .clone()
                            .atom_max_width(width)
                            .atom_size(vec2(width, height))
                            .atom_align(egui::Align2::LEFT_CENTER),
                    );
                    if !marks.is_empty() {
                        // In the place of the badges, painted over it: the
                        // columns after it stay under their headers.
                        atoms.push_right(String::new().atom_size(vec2(marks_room, height)));
                    }
                    continue;
                }
                column => {
                    let fact = self.fact_text(ui.ctx(), e, column);
                    if !fact.is_empty() {
                        facts.push(fact.clone());
                    }
                    let align = if column == SortColumn::Size {
                        egui::Align2::RIGHT_CENTER
                    } else {
                        egui::Align2::LEFT_CENTER
                    };
                    (fact, align)
                }
            };
            atoms.push_right(
                text.atom_max_width(c.width)
                    .atom_size(vec2(c.width, height))
                    .atom_align(align),
            );
        }
        let name_width = name_column - reserved;
        let font = egui::TextStyle::Body.resolve(ui.style());
        let cut = ui
            .painter()
            .layout_no_wrap(label.clone(), font, egui::Color32::PLACEHOLDER)
            .size()
            .x
            > name_width;
        // By its path: the focus stays on it as the rows scroll.
        let r = ui
            .push_id(&e.path, |ui| {
                ui.spacing_mut().icon_spacing = t.space.m;
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                    ui.add(widgets::SelectableRow::new(on, atoms).name(label.clone()))
                })
                .inner
            })
            .inner;
        if !facts.is_empty() {
            ui.ctx().accesskit_node_builder(r.id, |node| {
                node.set_description(facts.join(", "));
            });
        }
        if let (Some(group), Some(c)) = (&group, columns.first()) {
            let left = r.rect.left() + c.left;
            let host = egui::Rect::from_x_y_ranges(left..=left + c.width, r.rect.y_range());
            paint_marks(ui, host, Anchor::End, &marks, group);
        }
        if cut { r.on_hover_text(label) } else { r }
    }

    /// What a column says of an entry: empty for a folder's size, or a fact
    /// the file system does not give.
    fn fact_text(&self, ctx: &egui::Context, e: &Entry, column: SortColumn) -> String {
        match column {
            SortColumn::Name => e.name.clone(),
            SortColumn::Size if e.is_dir => String::new(),
            SortColumn::Size => self
                .meta_of(&e.path)
                .map_or_else(String::new, |m| preview::size_text(m.len)),
            SortColumn::Modified => self
                .meta_of(&e.path)
                .and_then(|m| m.modified)
                .map_or_else(String::new, preview::time_text),
            SortColumn::Type if e.is_dir => strings::tr(ctx, &txt::FILES_KIND_FOLDER),
            SortColumn::Type => match extension_dot(&e.name) {
                Some(dot) => e.name[dot + 1..].to_uppercase(),
                None => strings::tr(ctx, &txt::FILES_KIND_FILE),
            },
        }
    }

    /// What the row just drawn does with the pointer and the keys.
    #[allow(clippy::too_many_arguments)]
    fn row_input(
        &mut self,
        ui: &mut Ui,
        r: &egui::Response,
        n: usize,
        shown: &[Entry],
        modifiers: Modifiers,
        pointer: bool,
        answer: &mut Option<PickerAnswer>,
    ) {
        if self.selects_apart() && n == self.highlight {
            egui_pop_core::focus::own_ring(ui.ctx(), r.id);
            let t = Tokens::of(ui.ctx());
            ui.painter().rect_stroke(
                r.rect,
                t.radius.widget,
                egui::Stroke::new(1.0, t.focus_ring),
                egui::StrokeKind::Inside,
            );
        }
        self.row_ids.push(r.id);
        // Tab through the rows moves the highlight too.
        if r.gained_focus() && !pointer {
            self.move_highlight(n, shown, false);
            r.scroll_to_me(None);
        }
        self.entry_clicks(r, n, shown, modifiers, pointer, answer);
    }

    /// Tells the providers which files the grid has in view.
    fn want_thumbnails(&mut self, ctx: &egui::Context, in_view: &[(PathBuf, Meta)]) {
        if let Some(previews) = &self.previews {
            previews.want(ctx, &self.fs, in_view);
        }
        self.asked_thumbnails = !in_view.is_empty();
    }

    /// Draws cell `n`, in the `Ui` the grid gave it: its thumbnail, or what
    /// stands in for it. A cell in view that has no thumbnail yet asks for
    /// one.
    fn grid_cell(
        &mut self,
        ui: &mut Ui,
        n: usize,
        shown: &[Entry],
        cell: egui::Vec2,
        in_view: &mut Vec<(PathBuf, Meta)>,
    ) -> egui::Response {
        let e = &shown[n];
        let selected = self.selected.contains(&e.path);
        let highlighted = n == self.highlight;
        let apart = self.selects_apart();
        let label = if e.is_dir {
            format!("{}/", e.name)
        } else {
            e.name.clone()
        };
        let mut cell_widget = Thumbnail::new(label)
            .caption(e.name.as_str())
            .size(cell)
            .selected(selected || (highlighted && !apart));
        let visible = ui.is_rect_visible(egui::Rect::from_min_size(ui.cursor().min, cell));
        let mut failed = None;
        if e.is_dir {
            cell_widget = cell_widget.icon(Icon::Folder);
        } else if !visible {
            // In a row built around the view: nothing is asked for it.
            cell_widget = cell_widget.placeholder(SkeletonTokens::of(ui.ctx()).fill);
        } else {
            match self.meta_of(&e.path) {
                None => cell_widget = cell_widget.icon(Icon::Warning),
                Some(meta) => {
                    // Whatever it has now, for the cache to keep it.
                    in_view.push((e.path.clone(), meta));
                    let made = match &self.previews {
                        Some(previews) => {
                            let (made, _) = self.preview_of(e, |path| {
                                if path == e.path {
                                    return previews.thumbnail(path, &meta);
                                }
                                match self.meta_of(path) {
                                    Some(meta) => {
                                        in_view.push((path.to_path_buf(), meta));
                                        previews.thumbnail(path, &meta)
                                    }
                                    None => preview::Shown::Nothing,
                                }
                            });
                            made
                        }
                        None => preview::Shown::Nothing,
                    };
                    cell_widget = match &made {
                        preview::Shown::Loading => {
                            cell_widget.placeholder(SkeletonTokens::of(ui.ctx()).fill)
                        }
                        preview::Shown::Ready(ready) => {
                            let image = match &ready.preview.body {
                                preview::Body::Image(image) => Some(image.size),
                                _ => None,
                            };
                            match (image, ready.texture(ui.ctx(), 0)) {
                                (Some([w, h]), Some(texture)) => {
                                    cell_widget.image(texture.id(), [w as u32, h as u32])
                                }
                                _ => cell_widget.icon(Icon::File),
                            }
                        }
                        preview::Shown::Nothing => cell_widget.icon(Icon::File),
                        preview::Shown::Failed(reason) => {
                            failed = Some(reason.clone());
                            cell_widget.icon(Icon::Warning)
                        }
                    };
                }
            }
        }
        // By its path: the focus stays on it as the rows scroll.
        let r = ui.push_id(&e.path, |ui| ui.add(cell_widget)).inner;
        let r = match failed {
            Some(reason) => r.on_hover_text(reason),
            None => r,
        };
        if let Some(group) = self.group_of(&e.path) {
            let said = group_said(ui.ctx(), group);
            ui.ctx().accesskit_node_builder(r.id, |node| {
                node.set_description(said);
            });
            let host = r.rect.shrink(Tokens::of(ui.ctx()).space.xs);
            let marks = group_marks(ui, group, host.width());
            paint_marks(ui, host, Anchor::TopEnd, &marks, group);
        }
        r
    }

    /// A file's facts, read once per listing.
    fn meta_of(&self, path: &Path) -> Option<Meta> {
        *lock(&self.metas)
            .entry(path.to_path_buf())
            .or_insert_with(|| self.fs.metadata(path).ok())
    }

    /// What the cell just drawn does with the pointer and the keys.
    #[allow(clippy::too_many_arguments)]
    fn cell_input(
        &mut self,
        ui: &mut Ui,
        r: &egui::Response,
        n: usize,
        shown: &[Entry],
        modifiers: Modifiers,
        pointer: bool,
        answer: &mut Option<PickerAnswer>,
    ) {
        if self.selects_apart() && n == self.highlight {
            egui_pop_core::focus::own_ring(ui.ctx(), r.id);
            let t = Tokens::of(ui.ctx());
            ui.painter().rect_stroke(
                r.rect,
                t.radius.widget,
                egui::Stroke::new(1.0, t.focus_ring),
                egui::StrokeKind::Inside,
            );
        }
        self.row_ids.push(r.id);
        // The grid's own arrows, not egui's moves from widget to widget.
        if r.has_focus() {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    r.id,
                    EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
        }
        // Tab through the cells moves the highlight too.
        if r.gained_focus() && !pointer && n != self.highlight {
            self.move_highlight(n, shown, false);
            r.scroll_to_me(None);
        }
        if self.focus_cell && n == self.highlight {
            self.focus_cell = false;
            r.request_focus();
        }
        self.entry_clicks(r, n, shown, modifiers, pointer, answer);
        // A filter holds text: the arrows must reach the grid, so the cell
        // keeps the focus. Saving, the field holds the name just taken.
        if r.clicked() && self.kind != ChooseKind::Save && !self.text.is_empty() {
            self.refocus = false;
            r.request_focus();
        }
    }

    /// What a click does on an entry, the same in the list and the grid. It
    /// selects; a double click opens.
    fn entry_clicks(
        &mut self,
        r: &egui::Response,
        n: usize,
        shown: &[Entry],
        modifiers: Modifiers,
        pointer: bool,
        answer: &mut Option<PickerAnswer>,
    ) {
        if r.double_clicked() && !modifiers.command && !modifiers.shift {
            self.click_entry(n, shown, Modifiers::NONE);
            *answer = answer.take().or_else(|| self.activate());
        } else if r.clicked() {
            self.click_entry(n, shown, modifiers);
            // By the pointer: the keys go on as before.
            self.refocus |= pointer;
        }
    }

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
        let id = ui.scope_id().with("egui_pop.files.new_folder");
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
            egui_pop_core::a11y::name(&edit, egui::Role::TextInput, &label);
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
                let ctx = ui.ctx().clone();
                let (shown, stand_in) =
                    self.preview_of(&e, |path| previews.get(&ctx, &self.fs, path));
                let meta = self.fs.metadata(&e.path).ok();
                let stand_in = stand_in
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy());
                preview::pane_for(ui, &e.name, meta.as_ref(), &shown, stand_in.as_deref());
                if let Some(group) = self.group_of(&e.path) {
                    let t = Tokens::of(&ctx);
                    ui.add_space(t.space.s);
                    ui.weak(strings::tr(&ctx, &txt::FILES_GROUP_MEMBERS));
                    for name in group.member_names() {
                        ui.add(egui::Label::new(name).truncate());
                    }
                }
            }
            None => {
                ui.weak(strings::tr(ui.ctx(), &txt::FILES_PREVIEW_NONE));
            }
        }
    }
}

/// The picker's [`browser::EntryDrawer`]: the layer between the modes and
/// the picker. The modes lay the entries out; it draws the picker's rows
/// with their columns and its thumbnail cells, with the groups' badges and
/// the highlight, and runs the picker's own rules on a click.
struct PickerDrawer<'a> {
    picker: &'a mut FilePicker,
    shown: Vec<Entry>,
    columns: &'a [ListColumn],
    row_width: f32,
    list_width: f32,
    modifiers: Modifiers,
    pointer: bool,
    answer: &'a mut Option<PickerAnswer>,
    /// The files the cells in view showed, for the providers.
    in_view: Vec<(PathBuf, Meta)>,
}

impl browser::EntryDrawer for PickerDrawer<'_> {
    fn headers(&mut self, ui: &mut Ui, _width: f32) {
        let order = self.picker.sort;
        self.picker
            .column_headers(ui, self.columns, self.list_width);
        // A click on a header shows in this frame.
        if self.picker.sort != order {
            self.shown = self.picker.visible();
        }
    }

    fn empty(&mut self, ui: &mut Ui) {
        ui.weak(strings::tr(ui.ctx(), &txt::NOTHING_MATCHES));
    }

    fn row_size(&self, ui: &Ui, _width: f32) -> egui::Vec2 {
        vec2(self.row_width, row_height(ui))
    }

    fn row(&mut self, ui: &mut Ui, n: usize, _size: egui::Vec2) -> egui::Response {
        let r = self.picker.list_row(ui, &self.shown[n], n, self.columns);
        as_option(ui, &r);
        self.picker.row_input(
            ui,
            &r,
            n,
            &self.shown,
            self.modifiers,
            self.pointer,
            self.answer,
        );
        r
    }

    fn cell_size(&self, ui: &Ui, _side: f32) -> egui::Vec2 {
        let side = self.picker.thumbnail_size;
        vec2(side, side + thumbnail::caption_height(ui))
    }

    fn cell(
        &mut self,
        ui: &mut Ui,
        n: usize,
        size: egui::Vec2,
        _thumbnail: browser::ThumbState,
    ) -> egui::Response {
        let r = self
            .picker
            .grid_cell(ui, n, &self.shown, size, &mut self.in_view);
        as_option(ui, &r);
        self.picker.cell_input(
            ui,
            &r,
            n,
            &self.shown,
            self.modifiers,
            self.pointer,
            self.answer,
        );
        r
    }
}

/// What a group's entry says to a screen reader and in its badges' tip.
fn group_said(ctx: &egui::Context, group: &FileGroup) -> String {
    let members = group.member_names().join(", ");
    strings::tr_args(ctx, &txt::FILES_GROUP_WITH, &[("members", &members)])
}

/// A badge of a group's entry.
fn group_mark(text: &str) -> Indicator {
    Indicator::text(text).tone(Tone::Neutral).overhang(false)
}

/// The room between a group's badges.
fn marks_gap(ui: &Ui) -> f32 {
    Tokens::of(ui.ctx()).space.xs
}

/// How wide these badges are, side by side.
fn marks_width(ui: &Ui, marks: &[String]) -> f32 {
    let gap = marks_gap(ui);
    marks
        .iter()
        .map(|m| group_mark(m).measure(ui).x + gap)
        .sum()
}

/// A group's badges, `+XMP` and `+JPEG`, in `room`; with too little, `+2`.
fn group_marks(ui: &Ui, group: &FileGroup, room: f32) -> Vec<String> {
    let all: Vec<String> = group.badges.iter().map(|b| format!("+{b}")).collect();
    if !all.is_empty() && marks_width(ui, &all) <= room {
        all
    } else {
        vec![format!("+{}", group.members.len())]
    }
}

/// Paints badges from the end of `host`, the last at the end; their tip names
/// the members. They take no click: the entry under them does.
fn paint_marks(ui: &mut Ui, host: egui::Rect, anchor: Anchor, marks: &[String], group: &FileGroup) {
    let said = group_said(ui.ctx(), group);
    let gap = marks_gap(ui);
    let mut host = host;
    for mark in marks.iter().rev() {
        // By an id: agents read them in the `indicators` collector.
        let id = format!("files.badge:{}:{mark}", group.main.display());
        let mark = group_mark(mark).id(id).tooltip(said.clone());
        let width = mark.measure(ui).x;
        mark.paint_at(ui, host, anchor);
        host.set_right(host.right() - width - gap);
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
    fn breadcrumbs_read_from_the_root_without_dots() {
        let names = |path: &str| -> Vec<String> {
            crumbs_of(Path::new(path))
                .into_iter()
                .map(|c| c.name)
                .collect()
        };
        assert_eq!(names("/home/me/photos"), ["/", "home", "me", "photos"]);
        assert_eq!(names("/"), ["/"]);
        assert_eq!(names("/home/me/../you/./a/"), ["/", "home", "you", "a"]);
        assert_eq!(names("/../a"), ["/", "a"]);
        let crumbs = crumbs_of(Path::new("/home/me"));
        assert_eq!(crumbs[1].path, Path::new("/home"));
        assert_eq!(without_dots(Path::new("../a/./b/..")), Path::new("../a"));
    }

    #[test]
    fn the_start_of_a_path_collapses_and_the_deepest_stays() {
        let widths = [10.0, 20.0, 30.0, 40.0];
        // All of them fit: no button for the others.
        assert_eq!(first_crumb(&widths, 100.0, 5.0), 0);
        // The button takes its room from what is left.
        assert_eq!(first_crumb(&widths, 99.0, 5.0), 1);
        assert_eq!(first_crumb(&widths, 75.0, 5.0), 2);
        assert_eq!(first_crumb(&widths, 74.0, 5.0), 3);
        // Too narrow even for the deepest: it stays, and is cut.
        assert_eq!(first_crumb(&widths, 10.0, 5.0), 3);
        assert_eq!(first_crumb(&[50.0], 10.0, 5.0), 0);
    }

    #[test]
    fn the_columns_give_way_to_the_name_the_type_first() {
        let wanted = [
            (SortColumn::Modified, 150.0),
            (SortColumn::Size, 60.0),
            (SortColumn::Type, 50.0),
        ];
        let fit = |room| {
            fit_columns(room, 10.0, 4.0, &wanted)
                .iter()
                .map(|c| c.column)
                .collect::<Vec<_>>()
        };
        use SortColumn::*;
        let all = 8.0 + NAME_MIN_WIDTH + 150.0 + 60.0 + 50.0 + 30.0;
        assert_eq!(fit(all), [Name, Modified, Size, Type]);
        assert_eq!(fit(all - 1.0), [Name, Modified, Size]);
        assert_eq!(fit(all - 61.0), [Name, Modified]);
        assert_eq!(fit(all - 131.0), [Name]);
        // Too narrow even for the name: it is cut, and nothing goes negative.
        assert_eq!(fit_columns(4.0, 10.0, 4.0, &wanted)[0].width, 0.0);
        // Each starts where the one before ends, and the name takes what is left.
        let columns = fit_columns(all + 40.0, 10.0, 4.0, &wanted);
        assert_eq!(columns[0].left, 4.0);
        assert_eq!(columns[0].width, NAME_MIN_WIDTH + 40.0);
        assert_eq!(columns[1].left, 4.0 + NAME_MIN_WIDTH + 40.0 + 10.0);
        assert_eq!(columns[2].left, columns[1].left + 160.0);
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
    fn a_picker_drawn_in_a_panel_says_what_it_highlights() {
        let root = PathBuf::from("/demo");
        let fs: SharedFs = Arc::new(fs::MemoryFs::new().dir(root.join("sub")));
        let app = crate::App::builder()
            .plugin(FilesPlugin::in_app().file_system(fs))
            .build(&egui::Context::default())
            .unwrap();
        let h = app.handle().clone();
        let files = h.service(&FILES).unwrap();
        let (id, _pending) = files.choose_inline(&h, FileRequest::open("Open").directory(&root));
        assert_eq!(files.highlighted(id).unwrap().name, "sub");
        assert!(files.highlighted(id + 1).is_none());
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
