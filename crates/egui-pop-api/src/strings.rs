//! The text egui-pop's application interface shows itself (the palette's
//! prompt, the settings window's buttons, the shortcuts editor, dialogs'
//! buttons, toasts, devtools, the workbench's tab menu...), by key with
//! English fallbacks, read through `egui_pop_core::strings` so that the
//! installed provider translates it. The OK, Cancel, Close, Reset and
//! Retry buttons use core's keys ([`egui_pop_core::strings::ALL`]).
//!
//! What plugins declare (command titles, categories, panel titles,
//! submenu labels, settings, plugin titles, frame titles) is not here: it is
//! translated where it is drawn, through keys derived from its id
//! (`egui_pop_core::strings::command_title` and its siblings).
//!
//! Keys are Fluent message ids, `egui-pop-<area>-<what>`; placeholders
//! are written `{ $name }` and filled by `strings::tr_args`. [`ALL`]
//! lists every key, for catalogues and tests.

use egui_pop_core::strings::StrKey;

// Shared.

/// A command as lists show it (the palette, the shortcuts editor).
pub const COMMAND_LABEL: StrKey =
    StrKey::new("egui-pop-command-label", "{ $category }: { $title }");
/// Undo and redo naming what they would do ("Undo exposure change").
pub const COMMAND_WITH_CHANGE: StrKey =
    StrKey::new("egui-pop-command-with-change", "{ $title } { $change }");
/// A search or a filter found nothing.
pub const NOTHING_MATCHES: StrKey = StrKey::new("egui-pop-nothing-matches", "Nothing matches.");

// Menus.

pub const MENU_TOO_DEEP: StrKey =
    StrKey::new("egui-pop-menu-too-deep", "`{ $menu }` nests too deep");
pub const MENU_EMPTY: StrKey = StrKey::new("egui-pop-menu-empty", "(empty)");
pub const MENU_MORE: StrKey = StrKey::new("egui-pop-menu-more", "More menus");
pub const MENU_MAIN: StrKey = StrKey::new("egui-pop-menu-main", "Main menu");
/// A toolbar's button holding the tools that do not fit.
pub const TOOLBAR_MORE: StrKey = StrKey::new("egui-pop-toolbar-more", "More tools");
/// A toolbar button's tooltip: its command's title and its key.
pub const TOOLBAR_WITH_KEY: StrKey =
    StrKey::new("egui-pop-toolbar-with-key", "{ $action } ({ $keys })");
pub const MENU_DOES_NOT_APPLY: StrKey = StrKey::new(
    "egui-pop-menu-does-not-apply",
    "Does not apply here ({ $when })",
);
pub const MENU_NOT_AVAILABLE: StrKey =
    StrKey::new("egui-pop-menu-not-available", "Not available here");
pub const MENU_NO_COMMAND: StrKey = StrKey::new("egui-pop-menu-no-command", "No command `{ $id }`");
pub const MENU_RUNNING: StrKey = StrKey::new("egui-pop-menu-running", "Running…");
pub const MENU_LAST_RUN_FAILED: StrKey = StrKey::new(
    "egui-pop-menu-last-run-failed",
    "Last run failed: { $error }",
);

// The command palette.

pub const PALETTE_TITLE: StrKey = StrKey::new("egui-pop-palette-title", "Command palette");
pub const PALETTE_HINT: StrKey = StrKey::new("egui-pop-palette-hint", "Type a command");
pub const PALETTE_RECENT: StrKey = StrKey::new("egui-pop-palette-recent", "recent");
pub const PALETTE_RECENT_KEYS: StrKey =
    StrKey::new("egui-pop-palette-recent-keys", "recent  { $keys }");
pub const PALETTE_NO_MATCH: StrKey =
    StrKey::new("egui-pop-palette-no-match", "No command matches.");
/// An argument's prompt: the command's title and the argument's name.
pub const PALETTE_ARGUMENT_TITLE: StrKey = StrKey::new(
    "egui-pop-palette-argument-title",
    "{ $command }: { $argument }",
);
pub const PALETTE_ARGUMENT_HELP: StrKey = StrKey::new(
    "egui-pop-palette-argument-help",
    "Argument { $at } of { $count }. Enter: next; empty: { $example }; Escape: back.",
);
pub const PALETTE_NOT_BOOL: StrKey = StrKey::new(
    "egui-pop-palette-not-bool",
    "`{ $input }` is not true or false",
);
pub const PALETTE_NOT_NUMBER: StrKey = StrKey::new(
    "egui-pop-palette-not-number",
    "`{ $input }` is not a number",
);
pub const PALETTE_NOT_JSON: StrKey =
    StrKey::new("egui-pop-palette-not-json", "not JSON: { $error }");

// The settings window and form.

pub const SETTINGS_TITLE: StrKey = StrKey::new("egui-pop-settings-title", "Settings");
pub const SETTINGS_SEARCH_HINT: StrKey = StrKey::new(
    "egui-pop-settings-search-hint",
    "Search settings and shortcuts",
);
/// The section holding the shortcuts editor.
pub const SETTINGS_SHORTCUTS: StrKey =
    StrKey::new("egui-pop-settings-shortcuts", "Keyboard shortcuts");
pub const SETTINGS_FONTS: StrKey = StrKey::new("egui-pop-settings-fonts", "Fonts");
pub const FONTS_THEMES: StrKey = StrKey::new("egui-pop-fonts-themes", "The theme's");
pub const FONTS_PREVIEW: StrKey = StrKey::new("egui-pop-fonts-preview", "Preview");
pub const FONTS_NUMBERS: StrKey =
    StrKey::new("egui-pop-fonts-numbers", "Values line their digits up:");
pub const FONTS_PICKER_HINT: StrKey = StrKey::new(
    "egui-pop-fonts-picker-hint",
    "Each family is drawn in its own face, but the system's; rest the pointer on one to preview it below.",
);
pub const FONTS_ORIGIN_BUNDLED: StrKey =
    StrKey::new("egui-pop-fonts-origin-bundled", "Shipped with egui");
pub const FONTS_ORIGIN_REGISTERED: StrKey = StrKey::new(
    "egui-pop-fonts-origin-registered",
    "Shipped with the application",
);
pub const FONTS_ORIGIN_FILE: StrKey =
    StrKey::new("egui-pop-fonts-origin-file", "One of your font files");
pub const FONTS_ORIGIN_SYSTEM: StrKey =
    StrKey::new("egui-pop-fonts-origin-system", "Installed on the system");
pub const SETTINGS_UNDO: StrKey = StrKey::new("egui-pop-settings-undo", "Undo");
pub const SETTINGS_REDO: StrKey = StrKey::new("egui-pop-settings-redo", "Redo");
pub const SETTINGS_UNDO_STEP: StrKey = StrKey::new(
    "egui-pop-settings-undo-step",
    "Undo { $change } (the settings only)",
);
pub const SETTINGS_REDO_STEP: StrKey = StrKey::new(
    "egui-pop-settings-redo-step",
    "Redo { $change } (the settings only)",
);
pub const SETTINGS_NOTHING_TO_UNDO: StrKey = StrKey::new(
    "egui-pop-settings-nothing-to-undo",
    "Nothing to undo in the settings",
);
pub const SETTINGS_NOTHING_TO_REDO: StrKey = StrKey::new(
    "egui-pop-settings-nothing-to-redo",
    "Nothing to redo in the settings",
);
pub const SETTINGS_UNDO_NAME: StrKey =
    StrKey::new("egui-pop-settings-undo-name", "Undo in the settings");
pub const SETTINGS_REDO_NAME: StrKey =
    StrKey::new("egui-pop-settings-redo-name", "Redo in the settings");
/// A setting's reset button, as the screen reader says it.
pub const SETTINGS_RESET_SETTING: StrKey =
    StrKey::new("egui-pop-settings-reset-setting", "Reset { $setting }");
pub const SETTINGS_AT_DEFAULT: StrKey = StrKey::new(
    "egui-pop-settings-at-default",
    "{ $setting } is at its default",
);
/// A custom page's reset.
pub const SETTINGS_RESET_PAGE: StrKey =
    StrKey::new("egui-pop-settings-reset-page", "Reset the page");
/// A custom page's reset, as the screen reader says it.
pub const SETTINGS_RESET_PAGE_NAME: StrKey =
    StrKey::new("egui-pop-settings-reset-page-name", "Reset { $page }");
pub const SETTINGS_PAGE_AT_DEFAULT: StrKey = StrKey::new(
    "egui-pop-settings-page-at-default",
    "{ $page } is at its defaults",
);
/// The button opening a custom page found by the search.
pub const SETTINGS_OPEN_PAGE: StrKey = StrKey::new("egui-pop-settings-open-page", "Open");
pub const SETTINGS_OPEN_PAGE_NAME: StrKey = StrKey::new(
    "egui-pop-settings-open-page-name",
    "Open the { $page } page",
);

// The shortcuts editor.

pub const SHORTCUTS_SEARCH_BY_KEY: StrKey =
    StrKey::new("egui-pop-shortcuts-search-by-key", "Search by key");
pub const SHORTCUTS_PRESS_TO_SEARCH: StrKey = StrKey::new(
    "egui-pop-shortcuts-press-to-search",
    "Press the keys to search for. Escape: cancel.",
);
pub const SHORTCUTS_BOUND_TO: StrKey =
    StrKey::new("egui-pop-shortcuts-bound-to", "Bound to { $keys }");
/// The shortcuts editor's pointer gestures: the profile in force.
pub const SHORTCUTS_POINTER_PROFILE: StrKey = StrKey::new(
    "egui-pop-shortcuts-pointer-profile",
    "Pointer profile: { $profile }",
);
/// The same, when the user changed some of its gestures.
pub const SHORTCUTS_POINTER_PROFILE_CHANGED: StrKey = StrKey::new(
    "egui-pop-shortcuts-pointer-profile-changed",
    "Pointer profile: { $profile } ({ $count } changed)",
);
pub const SHORTCUTS_POINTER_ONLY: StrKey =
    StrKey::new("egui-pop-shortcuts-pointer-only", "Pointer gestures only");
pub const SHORTCUTS_POINTER_ADD: StrKey =
    StrKey::new("egui-pop-shortcuts-pointer-add", "Add a gesture");
pub const SHORTCUTS_POINTER_ADD_NAME: StrKey = StrKey::new(
    "egui-pop-shortcuts-pointer-add-name",
    "Add a gesture to { $command }",
);
pub const SHORTCUTS_POINTER_REMOVE_NAME: StrKey = StrKey::new(
    "egui-pop-shortcuts-pointer-remove-name",
    "Remove { $keys } from { $command }",
);
/// The pad a gesture is done on to capture it.
pub const SHORTCUTS_POINTER_PAD: StrKey = StrKey::new(
    "egui-pop-shortcuts-pointer-pad",
    "Do the gesture here for { $command }: the wheel, a pinch, a click, a double click or a drag, holding its keys. Escape: cancel.",
);
pub const SHORTCUTS_POINTER_PAD_NAME: StrKey =
    StrKey::new("egui-pop-shortcuts-pointer-pad-name", "Gesture pad");
pub const SHORTCUTS_POINTER_USE: StrKey =
    StrKey::new("egui-pop-shortcuts-pointer-use", "Use { $keys }");
/// Alt with the pointer on Linux.
pub const SHORTCUTS_POINTER_ALT: StrKey = StrKey::new(
    "egui-pop-shortcuts-pointer-alt",
    "Alt with a drag or the wheel may go to the window manager on some Linux desktops.",
);
/// The heading of the widgets' own gestures (a widget claiming the
/// secondary button).
pub const SHORTCUTS_WIDGET_USES: StrKey =
    StrKey::new("egui-pop-shortcuts-widget-uses", "Widgets' own gestures");
/// Under that heading.
pub const SHORTCUTS_WIDGET_USES_NOTE: StrKey = StrKey::new(
    "egui-pop-shortcuts-widget-uses-note",
    "These widgets keep these gestures for themselves: context menus and the pointer profile leave them alone there.",
);
/// The dialog after a change of pointer profile, when the user changed
/// gestures: its title.
pub const POINTER_OVERRIDES_TITLE: StrKey = StrKey::new(
    "egui-pop-pointer-overrides-title",
    "Pointer profile: { $profile }",
);
pub const POINTER_OVERRIDES_DETAIL: StrKey = StrKey::new(
    "egui-pop-pointer-overrides-detail",
    "You changed { $count } pointer gestures. Keep your changes on top of this profile, or drop them?",
);
pub const POINTER_KEEP: StrKey = StrKey::new("egui-pop-pointer-keep", "Keep my changes");
pub const POINTER_DROP: StrKey = StrKey::new("egui-pop-pointer-drop", "Drop my changes");
pub const SHORTCUTS_SHOW_ALL: StrKey =
    StrKey::new("egui-pop-shortcuts-show-all", "Show every command");
pub const SHORTCUTS_CONFLICTS: StrKey = StrKey::new("egui-pop-shortcuts-conflicts", "Conflicts");
pub const SHORTCUTS_CONFLICT_IN_SCOPE: StrKey = StrKey::new(
    "egui-pop-shortcuts-conflict-in-scope",
    "{ $keys } in { $scope }: { $loser } goes to { $winner }",
);
pub const SHORTCUTS_PRESS_NEW_KEYS: StrKey = StrKey::new(
    "egui-pop-shortcuts-press-new-keys",
    "Press the new keys for { $command }. Escape: cancel; Backspace: no key.",
);
pub const SHORTCUTS_GOES_TO: StrKey = StrKey::new(
    "egui-pop-shortcuts-goes-to",
    "{ $keys } goes to { $command }",
);
pub const SHORTCUTS_ALSO_BOUND: StrKey = StrKey::new(
    "egui-pop-shortcuts-also-bound",
    "{ $keys } also bound to { $command }",
);
/// A command whose keys also work outside the window, registered.
pub const SHORTCUTS_SYSTEM_ON: StrKey = StrKey::new(
    "egui-pop-shortcuts-system-on",
    "{ $keys } also works outside the window",
);
/// Registration asked, not answered yet.
pub const SHORTCUTS_SYSTEM_PENDING: StrKey = StrKey::new(
    "egui-pop-shortcuts-system-pending",
    "{ $keys } outside the window: waiting for the desktop",
);
/// The desktop refused one binding.
pub const SHORTCUTS_SYSTEM_FAILED: StrKey = StrKey::new(
    "egui-pop-shortcuts-system-failed",
    "{ $keys } does not work outside the window: { $reason }",
);
/// The desktop takes no global shortcuts.
pub const SHORTCUTS_SYSTEM_UNAVAILABLE: StrKey = StrKey::new(
    "egui-pop-shortcuts-system-unavailable",
    "{ $keys } works only in the window: { $reason }",
);
/// Nothing registers system shortcuts.
pub const SHORTCUTS_SYSTEM_NO_PLUGIN: StrKey = StrKey::new(
    "egui-pop-shortcuts-system-no-plugin",
    "{ $keys } works only in the window: no plugin registers global shortcuts",
);
pub const SHORTCUTS_CHANGE: StrKey = StrKey::new("egui-pop-shortcuts-change", "Change");
pub const SHORTCUTS_CHANGE_NAME: StrKey = StrKey::new(
    "egui-pop-shortcuts-change-name",
    "Change the keys of { $command }",
);
pub const SHORTCUTS_NO_KEY: StrKey = StrKey::new("egui-pop-shortcuts-no-key", "No key");
pub const SHORTCUTS_NO_KEY_NAME: StrKey = StrKey::new(
    "egui-pop-shortcuts-no-key-name",
    "Remove every key of { $command }",
);
pub const SHORTCUTS_DEFAULT_NAME: StrKey = StrKey::new(
    "egui-pop-shortcuts-default-name",
    "Default keys of { $command }",
);
/// Where a binding comes from, after its keys.
pub const SHORTCUTS_FROM_WORKSPACE: StrKey =
    StrKey::new("egui-pop-shortcuts-from-workspace", "workspace");
pub const SHORTCUTS_FROM_USER: StrKey = StrKey::new("egui-pop-shortcuts-from-user", "user");
/// A key and where it comes from.
pub const SHORTCUTS_KEY_FROM: StrKey =
    StrKey::new("egui-pop-shortcuts-key-from", "{ $keys } ({ $source })");
/// The layer the keys changed go to.
pub const SHORTCUTS_TARGET: StrKey =
    StrKey::new("egui-pop-shortcuts-target", "Keys changed here apply to");
pub const SHORTCUTS_TARGET_USER: StrKey =
    StrKey::new("egui-pop-shortcuts-target-user", "every workspace (yours)");
pub const SHORTCUTS_TARGET_WORKSPACE: StrKey =
    StrKey::new("egui-pop-shortcuts-target-workspace", "this workspace");

// Dialogs.

pub const DIALOG_FILTER: StrKey = StrKey::new("egui-pop-dialog-filter", "Filter");
pub const DIALOG_BROWSE: StrKey = StrKey::new("egui-pop-dialog-browse", "Browse…");
pub const PICKER_UP: StrKey = StrKey::new("egui-pop-picker-up", "Up");
pub const PICKER_CHOOSE_FOLDER: StrKey =
    StrKey::new("egui-pop-picker-choose-folder", "Choose this folder");
pub const PICKER_CLOSE: StrKey = StrKey::new("egui-pop-picker-close", "Close the picker");

// Files.

pub const FILES_FILTER: StrKey = StrKey::new("egui-pop-files-filter", "Filter, or type a path");
pub const FILES_NAME: StrKey = StrKey::new("egui-pop-files-name", "File name, or a path");
pub const FILES_BOOKMARK: StrKey = StrKey::new("egui-pop-files-bookmark", "Bookmark this folder");
pub const FILES_UNBOOKMARK: StrKey =
    StrKey::new("egui-pop-files-unbookmark", "Remove the bookmark");
pub const FILES_HOME: StrKey = StrKey::new("egui-pop-files-home", "Home");
pub const FILES_RECENT: StrKey = StrKey::new("egui-pop-files-recent", "Recent");
pub const FILES_BOOKMARKS: StrKey = StrKey::new("egui-pop-files-bookmarks", "Bookmarks");
pub const FILES_ALL: StrKey = StrKey::new("egui-pop-files-all", "All files");
pub const FILES_HIDDEN: StrKey = StrKey::new("egui-pop-files-hidden", "Hidden files");
pub const FILES_OPEN: StrKey = StrKey::new("egui-pop-files-open", "Open");
pub const FILES_SAVE: StrKey = StrKey::new("egui-pop-files-save", "Save");
pub const FILES_PREVIEW: StrKey = StrKey::new("egui-pop-files-preview", "Preview");
pub const FILES_PREVIEW_LOADING: StrKey =
    StrKey::new("egui-pop-files-preview-loading", "Loading the preview…");
pub const FILES_PREVIEW_NONE: StrKey = StrKey::new("egui-pop-files-preview-none", "No preview");
pub const FILES_PREVIEW_FAILED: StrKey =
    StrKey::new("egui-pop-files-preview-failed", "The preview failed");
pub const FILES_SIZE: StrKey = StrKey::new("egui-pop-files-size", "Size");
pub const FILES_ITEMS: StrKey = StrKey::new("egui-pop-files-items", "Items");
pub const FILES_DIMENSIONS: StrKey = StrKey::new("egui-pop-files-dimensions", "Dimensions");
pub const FILES_PAGES: StrKey = StrKey::new("egui-pop-files-pages", "Pages");
pub const FILES_MODIFIED: StrKey = StrKey::new("egui-pop-files-modified", "Modified");

// Toasts.

pub const TOAST_INFO: StrKey = StrKey::new("egui-pop-toast-info", "Information");
pub const TOAST_SUCCESS: StrKey = StrKey::new("egui-pop-toast-success", "Done");
pub const TOAST_WARNING: StrKey = StrKey::new("egui-pop-toast-warning", "Warning");
pub const TOAST_ERROR: StrKey = StrKey::new("egui-pop-toast-error", "Error");
/// A toast's close button, as the screen reader says it.
pub const TOAST_CLOSE: StrKey = StrKey::new("egui-pop-toast-close", "Close: { $text }");

// The window and its frame.

pub const FRAME_WINDOW_MENU: StrKey = StrKey::new("egui-pop-frame-window-menu", "Window menu");
pub const WINDOW_EXIT_FULLSCREEN: StrKey =
    StrKey::new("egui-pop-window-exit-fullscreen", "Exit full screen (F11)");

// The workbench.

pub const AREA_CENTER: StrKey = StrKey::new("egui-pop-area-center", "the centre");
pub const AREA_START_SIDE: StrKey = StrKey::new("egui-pop-area-start-side", "the start side bar");
pub const AREA_END_SIDE: StrKey = StrKey::new("egui-pop-area-end-side", "the end side bar");
pub const AREA_BOTTOM: StrKey = StrKey::new("egui-pop-area-bottom", "the bottom panel");
pub const WORKBENCH_MOVE_TO: StrKey =
    StrKey::new("egui-pop-workbench-move-to", "Move to { $area }");
pub const WORKBENCH_STAYS_OUT: StrKey = StrKey::new(
    "egui-pop-workbench-stays-out",
    "{ $panel } stays out of { $area }",
);
pub const WORKBENCH_DETACH: StrKey =
    StrKey::new("egui-pop-workbench-detach", "Detach into a window");
pub const WORKBENCH_CLOSE_PANEL: StrKey =
    StrKey::new("egui-pop-workbench-close-panel", "Close { $panel }");
pub const WORKBENCH_FLOAT: StrKey = StrKey::new("egui-pop-workbench-float", "Float");
pub const WORKBENCH_UNFOLD: StrKey = StrKey::new("egui-pop-workbench-unfold", "Unfold { $area }");
pub const WORKBENCH_FOLD: StrKey = StrKey::new("egui-pop-workbench-fold", "Fold { $area }");
/// The button of a flyout (a folded area's panel shown over the centre)
/// unfolding its area with the panel shown.
pub const WORKBENCH_FLYOUT_PIN: StrKey =
    StrKey::new("egui-pop-workbench-flyout-pin", "Pin { $panel }");

// Palettes.

pub const PALETTE_ROLL_UP: StrKey = StrKey::new("egui-pop-palette-roll-up", "Roll up { $palette }");
pub const PALETTE_ROLL_DOWN: StrKey =
    StrKey::new("egui-pop-palette-roll-down", "Roll down { $palette }");
pub const PALETTE_CLOSE: StrKey = StrKey::new("egui-pop-palette-close", "Close { $palette }");
/// The menu of a group of palettes, on its band (named with its title).
pub const PALETTE_GROUP_MENU: StrKey =
    StrKey::new("egui-pop-palette-group-menu", "{ $palette } menu");
/// The entry of a group's menu closing every palette of the group.
pub const PALETTE_CLOSE_GROUP: StrKey = StrKey::new("egui-pop-palette-close-group", "Close group");
pub const PALETTE_PIN: StrKey = StrKey::new("egui-pop-palette-pin", "Pin { $palette }");
pub const PALETTE_UNPIN: StrKey = StrKey::new("egui-pop-palette-unpin", "Unpin { $palette }");
/// A detached panel's window title, with the application's.
pub const WORKBENCH_WINDOW_TITLE: StrKey = StrKey::new(
    "egui-pop-workbench-window-title",
    "{ $panel } — { $application }",
);
/// Zen mode's way out: its hint, with the key that leaves it.
pub const ZEN_HINT: StrKey = StrKey::new("egui-pop-zen-hint", "Zen mode: { $keys } to leave");
/// Zen mode's hint when no key leaves it.
pub const ZEN_HINT_NO_KEY: StrKey = StrKey::new(
    "egui-pop-zen-hint-no-key",
    "Zen mode: the top edge shows the way out",
);
/// The commands every panel gets (`<panel>.focus`, `<panel>.toggle`):
/// one text for every panel, its title filled in.
pub const PANEL_FOCUS_TITLE: StrKey = StrKey::new("egui-pop-panel-focus-title", "Focus { $panel }");
pub const PANEL_FOCUS_DESCRIPTION: StrKey = StrKey::new(
    "egui-pop-panel-focus-description",
    "Shows { $panel } and gives it the keyboard.",
);
pub const PANEL_TOGGLE_TITLE: StrKey =
    StrKey::new("egui-pop-panel-toggle-title", "Show or hide { $panel }");
pub const PANEL_TOGGLE_DESCRIPTION: StrKey = StrKey::new(
    "egui-pop-panel-toggle-description",
    "Opens { $panel } where it was, or closes it.",
);

// The layout controls.

/// The title bar's button opening the layout's choices.
pub const LAYOUT_CUSTOMIZE: StrKey = StrKey::new("egui-pop-layout-customize", "Customize layout");
/// An area toggle's tooltip: its action and its key.
pub const LAYOUT_WITH_KEY: StrKey =
    StrKey::new("egui-pop-layout-with-key", "{ $action } ({ $keys })");
/// The layout menu's item saving the current arrangement.
pub const LAYOUT_SAVE_AS: StrKey = StrKey::new(
    "egui-pop-layout-save-as",
    "Save the current arrangement as…",
);
pub const LAYOUT_SIDE_BAR_LEFT: StrKey = StrKey::new(
    "egui-pop-layout-side-bar-left",
    "Primary side bar on the left",
);
pub const LAYOUT_SIDE_BAR_RIGHT: StrKey = StrKey::new(
    "egui-pop-layout-side-bar-right",
    "Primary side bar on the right",
);
pub const LAYOUT_PANEL_CENTER: StrKey =
    StrKey::new("egui-pop-layout-panel-center", "Panel under the editors");
pub const LAYOUT_PANEL_JUSTIFY: StrKey = StrKey::new(
    "egui-pop-layout-panel-justify",
    "Panel across the full width",
);
/// The dialog naming an arrangement to save.
pub const ARRANGEMENT_SAVE_TITLE: StrKey = StrKey::new(
    "egui-pop-arrangement-save-title",
    "Save the current arrangement",
);
pub const ARRANGEMENT_SAVE_VERB: StrKey =
    StrKey::new("egui-pop-arrangement-save-verb", "Save the arrangement");
pub const ARRANGEMENT_NAME: StrKey = StrKey::new("egui-pop-arrangement-name", "Name");
/// The name the dialog suggests (`n`: one more than the saved ones).
pub const ARRANGEMENT_SUGGESTED: StrKey =
    StrKey::new("egui-pop-arrangement-suggested", "Arrangement { $n }");

// Devtools.

pub const DEVTOOLS_OFF: StrKey = StrKey::new(
    "egui-pop-devtools-off",
    "Devtools are off (Settings > Developer > Devtools).",
);
pub const DEVTOOLS_TAB_KEYS: StrKey = StrKey::new("egui-pop-devtools-tab-keys", "Keys");
pub const DEVTOOLS_TAB_JOURNAL: StrKey = StrKey::new("egui-pop-devtools-tab-journal", "Journal");
pub const DEVTOOLS_TAB_STEPS: StrKey = StrKey::new("egui-pop-devtools-tab-steps", "Steps");
pub const DEVTOOLS_TAB_FOCUS: StrKey = StrKey::new("egui-pop-devtools-tab-focus", "Focus");
pub const DEVTOOLS_TAB_SCALE: StrKey = StrKey::new("egui-pop-devtools-tab-scale", "Scale");
pub const DEVTOOLS_TAB_COLLECTORS: StrKey =
    StrKey::new("egui-pop-devtools-tab-collectors", "Collectors");
/// The devtools' tabs, in the order of `devtools::TABS`.
pub const DEVTOOLS_TABS: [StrKey; 6] = [
    DEVTOOLS_TAB_KEYS,
    DEVTOOLS_TAB_JOURNAL,
    DEVTOOLS_TAB_STEPS,
    DEVTOOLS_TAB_FOCUS,
    DEVTOOLS_TAB_SCALE,
    DEVTOOLS_TAB_COLLECTORS,
];
pub const DEVTOOLS_WAITING: StrKey = StrKey::new(
    "egui-pop-devtools-waiting",
    "Waiting for the next chord after { $keys }",
);
pub const DEVTOOLS_PRESS_A_KEY: StrKey = StrKey::new(
    "egui-pop-devtools-press-a-key",
    "Press a key bound to a command: its resolution shows here.",
);
pub const DEVTOOLS_NO_HOOKS: StrKey = StrKey::new("egui-pop-devtools-no-hooks", "no hooks");
pub const DEVTOOLS_HOOK_BY: StrKey =
    StrKey::new("egui-pop-devtools-hook-by", "{ $kind } by { $owner }");
pub const DEVTOOLS_FOCUS_CHAIN: StrKey =
    StrKey::new("egui-pop-devtools-focus-chain", "Focus chain");
pub const DEVTOOLS_OVERLAYS: StrKey = StrKey::new("egui-pop-devtools-overlays", "Overlays");
pub const DEVTOOLS_CONTEXT_KEYS: StrKey =
    StrKey::new("egui-pop-devtools-context-keys", "Context keys");
pub const DEVTOOLS_ARGUMENTS: StrKey = StrKey::new("egui-pop-devtools-arguments", "Arguments");
pub const DEVTOOLS_MUTATES: StrKey =
    StrKey::new("egui-pop-devtools-mutates", "{ $action } (mutates)");

// Jobs.

/// The status bar's jobs item: how many go on.
pub const JOBS_ACTIVE: StrKey = StrKey::new("egui-pop-jobs-active", "Jobs: { $count }");
/// ... and their overall estimate.
pub const JOBS_ACTIVE_PERCENT: StrKey = StrKey::new(
    "egui-pop-jobs-active-percent",
    "Jobs: { $count } ({ $percent }%)",
);
/// How many failed.
pub const JOBS_FAILED: StrKey = StrKey::new("egui-pop-jobs-failed", "Failed: { $count }");

/// The jobs centre's title.
pub const JOBS_TITLE: StrKey = StrKey::new("egui-pop-jobs-title", "Jobs");
pub const JOBS_NONE: StrKey = StrKey::new("egui-pop-jobs-none", "No jobs.");
/// Finished jobs.
pub const JOBS_HISTORY: StrKey = StrKey::new("egui-pop-jobs-history", "History");
pub const JOBS_CLEAR: StrKey = StrKey::new("egui-pop-jobs-clear", "Clear finished");
/// From the popover to the jobs panel.
pub const JOBS_OPEN_PANEL: StrKey = StrKey::new("egui-pop-jobs-open-panel", "Open the panel");
/// A job's state.
pub const JOB_RUNNING: StrKey = StrKey::new("egui-pop-job-running", "running");
pub const JOB_PAUSED: StrKey = StrKey::new("egui-pop-job-paused", "paused");
pub const JOB_CANCELLING: StrKey = StrKey::new("egui-pop-job-cancelling", "cancelling");
pub const JOB_DONE: StrKey = StrKey::new("egui-pop-job-done", "done");
pub const JOB_FAILED: StrKey = StrKey::new("egui-pop-job-failed", "failed");
pub const JOB_CANCELLED: StrKey = StrKey::new("egui-pop-job-cancelled", "cancelled");
/// A request sent, not answered yet.
pub const JOB_ASKED: StrKey = StrKey::new("egui-pop-job-asked", "{ $state }, waiting for the host");
/// Items done of all.
pub const JOB_OF: StrKey = StrKey::new("egui-pop-job-of", "{ $done } of { $total }");
/// Time left.
pub const JOB_ETA_SECONDS: StrKey = StrKey::new("egui-pop-job-eta-seconds", "about { $n } s left");
pub const JOB_ETA_MINUTES: StrKey =
    StrKey::new("egui-pop-job-eta-minutes", "about { $n } min left");
/// Buttons asking the host (Cancel and Retry are core's).
pub const JOB_PAUSE: StrKey = StrKey::new("egui-pop-job-pause", "Pause");
pub const JOB_RESUME: StrKey = StrKey::new("egui-pop-job-resume", "Resume");
pub const JOB_DISMISS: StrKey = StrKey::new("egui-pop-job-dismiss", "Dismiss");
/// A job's button as assistive technologies name it.
pub const JOB_ACTION_NAME: StrKey =
    StrKey::new("egui-pop-job-action-name", "{ $action }: { $job }");
/// The chevron opening a job's log console.
pub const JOB_SHOW_LOG: StrKey = StrKey::new("egui-pop-job-show-log", "Show the log of { $job }");
pub const JOB_HIDE_LOG: StrKey = StrKey::new("egui-pop-job-hide-log", "Hide the log of { $job }");
/// Why the host refused a request.
pub const JOB_REFUSED: StrKey = StrKey::new("egui-pop-job-refused", "Refused: { $error }");
pub const JOB_NO_LOG: StrKey = StrKey::new("egui-pop-job-no-log", "No log yet.");

// The theme editor.

pub const THEME_EDITOR_MISSING: StrKey = StrKey::new(
    "egui-pop-theme-editor-missing",
    "The theme editor plugin is not connected.",
);
pub const THEME_EDITOR_START: StrKey = StrKey::new("egui-pop-theme-editor-start", "Start from");
pub const THEME_EDITOR_UNDO: StrKey = StrKey::new("egui-pop-theme-editor-undo", "Undo");
pub const THEME_EDITOR_REDO: StrKey = StrKey::new("egui-pop-theme-editor-redo", "Redo");
pub const THEME_EDITOR_CHANGE_OF: StrKey = StrKey::new(
    "egui-pop-theme-editor-change-of",
    "The change of { $token }",
);
pub const THEME_EDITOR_CHANGED_COUNT: StrKey = StrKey::new(
    "egui-pop-theme-editor-changed-count",
    "Tokens changed: { $count }",
);
pub const THEME_EDITOR_FILTER: StrKey =
    StrKey::new("egui-pop-theme-editor-filter", "Filter tokens");
pub const THEME_EDITOR_SURFACES: StrKey = StrKey::new("egui-pop-theme-editor-surfaces", "Surfaces");
pub const THEME_EDITOR_TEXT: StrKey = StrKey::new("egui-pop-theme-editor-text", "Text");
pub const THEME_EDITOR_ACCENT: StrKey =
    StrKey::new("egui-pop-theme-editor-accent", "Accent and focus");
pub const THEME_EDITOR_STATES: StrKey = StrKey::new("egui-pop-theme-editor-states", "States");
pub const THEME_EDITOR_BORDERS: StrKey = StrKey::new("egui-pop-theme-editor-borders", "Borders");
pub const THEME_EDITOR_SHAPE: StrKey = StrKey::new(
    "egui-pop-theme-editor-shape",
    "Shape, space, type and motion",
);
pub const THEME_EDITOR_COMPONENTS: StrKey =
    StrKey::new("egui-pop-theme-editor-components", "Components");
pub const THEME_EDITOR_PALETTE: StrKey = StrKey::new("egui-pop-theme-editor-palette", "Palette");
pub const THEME_EDITOR_EDITED: StrKey = StrKey::new("egui-pop-theme-editor-edited", "changed");
pub const THEME_EDITOR_PICK: StrKey =
    StrKey::new("egui-pop-theme-editor-pick", "Pick a token in the list.");
/// A step of the theme editor's history (ADR 0014): one token set.
pub const THEME_EDITOR_STEP: StrKey = StrKey::new("egui-pop-theme-editor-step", "Set { $token }");
/// A step of the theme editor's history: tokens set together (a scale).
pub const THEME_EDITOR_STEP_MANY: StrKey = StrKey::new(
    "egui-pop-theme-editor-step-many",
    "Set { $token } and { $more } more",
);
pub const THEME_EDITOR_TYPE: StrKey = StrKey::new("egui-pop-theme-editor-type", "Type: { $type }");
pub const THEME_EDITOR_OWN: StrKey =
    StrKey::new("egui-pop-theme-editor-own", "Changed in this theme");
pub const THEME_EDITOR_FROM: StrKey = StrKey::new("egui-pop-theme-editor-from", "As in { $theme }");
pub const THEME_EDITOR_ALIAS_OF: StrKey =
    StrKey::new("egui-pop-theme-editor-alias-of", "Alias of { $token }");
pub const THEME_EDITOR_VALUE: StrKey = StrKey::new("egui-pop-theme-editor-value", "Value");
pub const THEME_EDITOR_VALUE_OF: StrKey =
    StrKey::new("egui-pop-theme-editor-value-of", "Value of { $token }");
pub const THEME_EDITOR_ADJUST_OF: StrKey =
    StrKey::new("egui-pop-theme-editor-adjust-of", "Adjust { $token }");
pub const THEME_EDITOR_NOT_A_VALUE: StrKey = StrKey::new(
    "egui-pop-theme-editor-not-a-value",
    "Not a { $type } value, nor an alias of a token",
);
pub const THEME_EDITOR_RESET: StrKey =
    StrKey::new("egui-pop-theme-editor-reset", "Reset to { $theme }");
pub const THEME_EDITOR_CONTRAST: StrKey = StrKey::new("egui-pop-theme-editor-contrast", "Contrast");
pub const THEME_EDITOR_NO_PAIRS: StrKey = StrKey::new(
    "egui-pop-theme-editor-no-pairs",
    "No contrast pair uses this token.",
);
pub const THEME_EDITOR_PAIR: StrKey = StrKey::new("egui-pop-theme-editor-pair", "Pair");
pub const THEME_EDITOR_PAIR_ON: StrKey = StrKey::new(
    "egui-pop-theme-editor-pair-on",
    "{ $foreground } on { $background }",
);
pub const THEME_EDITOR_RATIO: StrKey = StrKey::new("egui-pop-theme-editor-ratio", "Ratio");
pub const THEME_EDITOR_THEME_NEEDS: StrKey =
    StrKey::new("egui-pop-theme-editor-theme-needs", "This theme needs");
pub const THEME_EDITOR_PASS: StrKey = StrKey::new("egui-pop-theme-editor-pass", "pass");
pub const THEME_EDITOR_FAIL: StrKey = StrKey::new("egui-pop-theme-editor-fail", "fail");
pub const THEME_EDITOR_NOT_APPLICABLE: StrKey =
    StrKey::new("egui-pop-theme-editor-not-applicable", "n/a");
pub const THEME_EDITOR_GENERATE_TITLE: StrKey = StrKey::new(
    "egui-pop-theme-editor-generate-title",
    "Generate this scale from a colour",
);
pub const THEME_EDITOR_SEED_OF: StrKey =
    StrKey::new("egui-pop-theme-editor-seed-of", "Seed colour of { $scale }");
pub const THEME_EDITOR_SEED_HEX_OF: StrKey = StrKey::new(
    "egui-pop-theme-editor-seed-hex-of",
    "Hex of the seed colour of { $scale }",
);
pub const THEME_EDITOR_GENERATE: StrKey =
    StrKey::new("egui-pop-theme-editor-generate", "Generate { $scale }");
pub const THEME_EDITOR_GENERATED: StrKey = StrKey::new(
    "egui-pop-theme-editor-generated",
    "{ $count } tokens of { $scale } generated; one undo takes them back.",
);
pub const THEME_EDITOR_SEED_CLAMPED: StrKey = StrKey::new(
    "egui-pop-theme-editor-seed-clamped",
    "Too dark, too light or too vivid for a ramp: { $color } is used.",
);
pub const THEME_EDITOR_GENERATE_VARIANT: StrKey = StrKey::new(
    "egui-pop-theme-editor-generate-variant",
    "Written in the variant edited only; the other keeps its own scale (switch to it, or generate the other variant).",
);
pub const THEME_EDITOR_VARIANT: StrKey =
    StrKey::new("egui-pop-theme-editor-variant", "Variant edited");
pub const THEME_EDITOR_VARIANT_HINT: StrKey = StrKey::new(
    "egui-pop-theme-editor-variant-hint",
    "The light and dark variants are edited as a pair, each with its own changes and history: switching shows and edits the other one.",
);
pub const THEME_EDITOR_DARK: StrKey = StrKey::new("egui-pop-theme-editor-dark", "Dark");
pub const THEME_EDITOR_LIGHT: StrKey = StrKey::new("egui-pop-theme-editor-light", "Light");
pub const THEME_EDITOR_OTHER: StrKey =
    StrKey::new("egui-pop-theme-editor-other", "Generate the other variant");
pub const THEME_EDITOR_OTHER_HINT: StrKey = StrKey::new(
    "egui-pop-theme-editor-other-hint",
    "Proposes { $theme } from this variant: colours mirrored (the same hue and saturation, the lightness inverted), aliases and other values as they are. Nothing changes until applied.",
);
pub const THEME_EDITOR_OTHER_TITLE: StrKey = StrKey::new(
    "egui-pop-theme-editor-other-title",
    "Proposed for { $theme }",
);
pub const THEME_EDITOR_OTHER_PREVIEWED: StrKey = StrKey::new(
    "egui-pop-theme-editor-other-previewed",
    "The preview pane shows the proposal.",
);
pub const THEME_EDITOR_OTHER_NONE: StrKey = StrKey::new(
    "egui-pop-theme-editor-other-none",
    "Nothing to change: { $theme } already matches this variant.",
);
pub const THEME_EDITOR_OTHER_APPLY: StrKey =
    StrKey::new("egui-pop-theme-editor-other-apply", "Apply to { $theme }");
pub const THEME_EDITOR_OTHER_CANCEL: StrKey =
    StrKey::new("egui-pop-theme-editor-other-cancel", "Cancel");
pub const THEME_EDITOR_OTHER_WRITTEN: StrKey = StrKey::new(
    "egui-pop-theme-editor-other-written",
    "{ $count } tokens of { $theme } written; one undo there takes them back.",
);
pub const THEME_EDITOR_PREVIEW_ONLY: StrKey =
    StrKey::new("egui-pop-theme-editor-preview-only", "Preview only");
pub const THEME_EDITOR_PREVIEW_ONLY_HINT: StrKey = StrKey::new(
    "egui-pop-theme-editor-preview-only-hint",
    "Changes stay in the preview pane until applied; the rest of the window keeps the theme installed.",
);
pub const THEME_EDITOR_APPLY: StrKey = StrKey::new("egui-pop-theme-editor-apply", "Apply");
pub const THEME_EDITOR_DISCARD: StrKey = StrKey::new("egui-pop-theme-editor-discard", "Discard");
pub const THEME_EDITOR_PREVIEW: StrKey = StrKey::new("egui-pop-theme-editor-preview", "Preview");
pub const THEME_EDITOR_NOT_APPLIED: StrKey = StrKey::new(
    "egui-pop-theme-editor-not-applied",
    "Not applied yet: only this pane shows the draft.",
);
pub const THEME_EDITOR_HEX_OF: StrKey =
    StrKey::new("egui-pop-theme-editor-hex-of", "Hex of { $token }");
pub const THEME_EDITOR_OPEN_FILE: StrKey =
    StrKey::new("egui-pop-theme-editor-open-file", "Open a file");
pub const THEME_EDITOR_SAVE_FILE: StrKey =
    StrKey::new("egui-pop-theme-editor-save-file", "Save as a file");
pub const THEME_EDITOR_SAVED: StrKey =
    StrKey::new("egui-pop-theme-editor-saved", "Saved to { $path }.");
pub const THEME_EDITOR_SAMPLE_HEADING: StrKey =
    StrKey::new("egui-pop-theme-editor-sample-heading", "A heading");
pub const THEME_EDITOR_SAMPLE_TEXT: StrKey = StrKey::new(
    "egui-pop-theme-editor-sample-text",
    "Body text, as most of an interface reads.",
);
pub const THEME_EDITOR_SAMPLE_MUTED: StrKey = StrKey::new(
    "egui-pop-theme-editor-sample-muted",
    "Muted text, for hints and details.",
);
pub const THEME_EDITOR_SAMPLE_BUTTON: StrKey =
    StrKey::new("egui-pop-theme-editor-sample-button", "A button");
pub const THEME_EDITOR_SAMPLE_PRIMARY: StrKey =
    StrKey::new("egui-pop-theme-editor-sample-primary", "Primary");
pub const THEME_EDITOR_SAMPLE_DANGER: StrKey =
    StrKey::new("egui-pop-theme-editor-sample-danger", "Delete");
pub const THEME_EDITOR_SAMPLE_DISABLED: StrKey =
    StrKey::new("egui-pop-theme-editor-sample-disabled", "Disabled");
pub const THEME_EDITOR_SAMPLE_CHECKBOX: StrKey =
    StrKey::new("egui-pop-theme-editor-sample-checkbox", "A checkbox");
pub const THEME_EDITOR_SAMPLE_OPTION_A: StrKey =
    StrKey::new("egui-pop-theme-editor-sample-option-a", "One option");
pub const THEME_EDITOR_SAMPLE_OPTION_B: StrKey =
    StrKey::new("egui-pop-theme-editor-sample-option-b", "Another");
pub const THEME_EDITOR_SAMPLE_SLIDER: StrKey =
    StrKey::new("egui-pop-theme-editor-sample-slider", "A slider");
pub const THEME_EDITOR_SAMPLE_FIELD: StrKey =
    StrKey::new("egui-pop-theme-editor-sample-field", "A field");
pub const THEME_EDITOR_SAMPLE_SELECTED: StrKey =
    StrKey::new("egui-pop-theme-editor-sample-selected", "A selected row");
pub const THEME_EDITOR_TAB_TOKENS: StrKey =
    StrKey::new("egui-pop-theme-editor-tab-tokens", "Tokens");
pub const THEME_EDITOR_TAB_FONTS: StrKey = StrKey::new("egui-pop-theme-editor-tab-fonts", "Fonts");
pub const THEME_EDITOR_FAMILIES: StrKey = StrKey::new("egui-pop-theme-editor-families", "Families");
pub const THEME_EDITOR_FAMILIES_HINT: StrKey = StrKey::new(
    "egui-pop-theme-editor-families-hint",
    "In order of priority: the first one found draws, the others are its fallbacks.",
);
pub const THEME_EDITOR_FAMILY_UI: StrKey = StrKey::new(
    "egui-pop-theme-editor-family-ui",
    "Interface text (font.family.ui)",
);
pub const THEME_EDITOR_FAMILY_MONO: StrKey = StrKey::new(
    "egui-pop-theme-editor-family-mono",
    "Values and code (font.family.mono)",
);
pub const THEME_EDITOR_MOVE_UP: StrKey =
    StrKey::new("egui-pop-theme-editor-move-up", "Move { $family } up");
pub const THEME_EDITOR_MOVE_DOWN: StrKey =
    StrKey::new("egui-pop-theme-editor-move-down", "Move { $family } down");
pub const THEME_EDITOR_REMOVE_FAMILY: StrKey =
    StrKey::new("egui-pop-theme-editor-remove-family", "Remove { $family }");
pub const THEME_EDITOR_ADD_FAMILY_TO: StrKey = StrKey::new(
    "egui-pop-theme-editor-add-family-to",
    "A family to add to { $role }",
);
pub const THEME_EDITOR_ADD: StrKey = StrKey::new("egui-pop-theme-editor-add", "Add");
pub const THEME_EDITOR_FONT_FILES: StrKey =
    StrKey::new("egui-pop-theme-editor-font-files", "Font files");
pub const THEME_EDITOR_NO_FONT_FILES: StrKey =
    StrKey::new("egui-pop-theme-editor-no-font-files", "No font file added.");
pub const THEME_EDITOR_REMOVE_FONT_FILE: StrKey = StrKey::new(
    "egui-pop-theme-editor-remove-font-file",
    "Remove the file of { $family }",
);
pub const THEME_EDITOR_ADD_FONT_FILE: StrKey =
    StrKey::new("egui-pop-theme-editor-add-font-file", "Add a font file");
pub const THEME_EDITOR_FONT_ADDED: StrKey = StrKey::new(
    "egui-pop-theme-editor-font-added",
    "{ $family } { $weight } added; apply to use it.",
);
pub const THEME_EDITOR_FONT_SAMPLE: StrKey =
    StrKey::new("egui-pop-theme-editor-font-sample", "Sample");
pub const THEME_EDITOR_FONT_SAMPLE_TEXT: StrKey = StrKey::new(
    "egui-pop-theme-editor-font-sample-text",
    "Sphinx of black quartz, judge my vow. 0123456789",
);
pub const THEME_EDITOR_FONT_SAMPLE_HINT: StrKey = StrKey::new(
    "egui-pop-theme-editor-font-sample-hint",
    "Drawn with the fonts edited, before they are applied; rest the pointer on a family to preview it first.",
);
pub const THEME_EDITOR_APPLY_FONTS: StrKey =
    StrKey::new("egui-pop-theme-editor-apply-fonts", "Apply the fonts");
pub const THEME_EDITOR_FAMILY_MISSING: StrKey = StrKey::new(
    "egui-pop-theme-editor-family-missing",
    "Not installed here: the next family draws",
);
pub const THEME_EDITOR_INSTALLED: StrKey =
    StrKey::new("egui-pop-theme-editor-installed", "Families installed");
pub const THEME_EDITOR_INSTALLED_HINT: StrKey = StrKey::new(
    "egui-pop-theme-editor-installed-hint",
    "Rest the pointer on a family to preview it; pick one to put it first. A theme names it by its family name: where it is missing, the next family of its list draws.",
);
pub const THEME_EDITOR_FOR_UI: StrKey =
    StrKey::new("egui-pop-theme-editor-for-ui", "For the interface");
pub const THEME_EDITOR_FOR_MONO: StrKey =
    StrKey::new("egui-pop-theme-editor-for-mono", "For monospace");
pub const THEME_EDITOR_SEARCH_FAMILIES: StrKey = StrKey::new(
    "egui-pop-theme-editor-search-families",
    "Search the families",
);
pub const THEME_EDITOR_NO_FAMILY_MATCHES: StrKey = StrKey::new(
    "egui-pop-theme-editor-no-family-matches",
    "No family matches.",
);
pub const THEME_EDITOR_CANCEL_FONTS: StrKey = StrKey::new(
    "egui-pop-theme-editor-cancel-fonts",
    "Cancel the font edits",
);
pub const THEME_EDITOR_APPLY_FONTS_HINT: StrKey = StrKey::new(
    "egui-pop-theme-editor-apply-fonts-hint",
    "Rebuilds the font atlas once, and applies the theme being edited.",
);
pub const THEME_EDITOR_LC_HINT: StrKey = StrKey::new(
    "egui-pop-theme-editor-lc-hint",
    "APCA lightness contrast, for information: 60 for ordinary text, 75 for body text; the theme is checked on WCAG.",
);
pub const THEME_EDITOR_FILE: StrKey =
    StrKey::new("egui-pop-theme-editor-file", "Export and import");
pub const THEME_EDITOR_NAME: StrKey = StrKey::new("egui-pop-theme-editor-name", "Theme name");
pub const THEME_EDITOR_EXPORT: StrKey = StrKey::new("egui-pop-theme-editor-export", "Export");
pub const THEME_EDITOR_IMPORT: StrKey = StrKey::new("egui-pop-theme-editor-import", "Import");
pub const THEME_EDITOR_FILE_TEXT: StrKey = StrKey::new(
    "egui-pop-theme-editor-file-text",
    "Theme file (.tokens.json)",
);
pub const THEME_EDITOR_IMPORTED: StrKey = StrKey::new(
    "egui-pop-theme-editor-imported",
    "Imported over { $theme }.",
);
pub const THEME_EDITOR_SKIPPED: StrKey =
    StrKey::new("egui-pop-theme-editor-skipped", "Left out: { $tokens }.");
pub const THEME_EDITOR_WARNINGS: StrKey = StrKey::new(
    "egui-pop-theme-editor-warnings",
    "Read with warnings: { $warnings }.",
);
pub const THEME_EDITOR_PROBLEMS: StrKey =
    StrKey::new("egui-pop-theme-editor-problems", "Problems in this theme");

// Tips.

pub const TIPS_OF_THE_DAY: StrKey = StrKey::new("egui-pop-tips-of-the-day", "Tip of the day");
pub const TIPS_HINT: StrKey = StrKey::new("egui-pop-tips-hint", "Tip");
pub const TIPS_PREVIOUS: StrKey = StrKey::new("egui-pop-tips-previous", "Previous tip");
pub const TIPS_NEXT: StrKey = StrKey::new("egui-pop-tips-next", "Next tip");
pub const TIPS_AT: StrKey = StrKey::new("egui-pop-tips-at", "{ $at } of { $count }");
pub const TIPS_DONT_SHOW: StrKey = StrKey::new("egui-pop-tips-dont-show", "Don't show again");
pub const TIPS_GOT_IT: StrKey = StrKey::new("egui-pop-tips-got-it", "Got it");
pub const TIPS_GOT_IT_NAME: StrKey = StrKey::new(
    "egui-pop-tips-got-it-name",
    "Got it: do not show { $tip } again",
);
pub const TIPS_DID_YOU_KNOW: StrKey = StrKey::new("egui-pop-tips-did-you-know", "Did you know?");
pub const TIPS_NOT_NOW: StrKey = StrKey::new("egui-pop-tips-not-now", "Not now");
pub const TIPS_NO_MORE: StrKey = StrKey::new("egui-pop-tips-no-more", "No more tips");
pub const TIPS_BROWSER: StrKey = StrKey::new("egui-pop-tips-browser", "Tips");
pub const TIPS_NONE: StrKey = StrKey::new("egui-pop-tips-none", "No tips are registered.");
pub const TIPS_ORIGIN_ON: StrKey = StrKey::new("egui-pop-tips-origin-on", "Show these tips");
pub const TIPS_ORIGIN_ON_NAME: StrKey = StrKey::new(
    "egui-pop-tips-origin-on-name",
    "Show the tips of { $origin }",
);
pub const TIPS_STATE_DISMISSED: StrKey = StrKey::new("egui-pop-tips-state-dismissed", "Dismissed");
pub const TIPS_STATE_USED: StrKey = StrKey::new("egui-pop-tips-state-used", "Its command was used");
pub const TIPS_SHOW: StrKey = StrKey::new("egui-pop-tips-show", "Show");
pub const TIPS_SHOW_NAME: StrKey = StrKey::new("egui-pop-tips-show-name", "Show the tip { $tip }");

// About the application.

/// The About dialog's title.
pub const ABOUT_TITLE: StrKey = StrKey::new("egui-pop-about-title", "About { $application }");
pub const ABOUT_VERSION: StrKey = StrKey::new("egui-pop-about-version", "Version { $version }");
pub const ABOUT_LICENCE: StrKey = StrKey::new("egui-pop-about-licence", "Licence");
pub const ABOUT_AUTHORS: StrKey = StrKey::new("egui-pop-about-authors", "Authors");
pub const ABOUT_REPOSITORY: StrKey = StrKey::new("egui-pop-about-repository", "Source code");
pub const ABOUT_WEBSITE: StrKey = StrKey::new("egui-pop-about-website", "Website");
pub const ABOUT_DOCS: StrKey = StrKey::new("egui-pop-about-docs", "Documentation");
pub const ABOUT_ISSUES: StrKey = StrKey::new("egui-pop-about-issues", "Report a problem");
pub const ABOUT_COPY: StrKey = StrKey::new("egui-pop-about-copy", "Copy version info");
pub const ABOUT_COPIED: StrKey = StrKey::new("egui-pop-about-copied", "Copied.");

// The context stack.

/// The breadcrumb's first crumb, the root.
pub const CONTEXT_ROOT: StrKey = StrKey::new("egui-pop-context-root", "Home");
/// A mode chip's button turning the mode off.
pub const CONTEXT_LEAVE_MODE: StrKey =
    StrKey::new("egui-pop-context-leave-mode", "Leave { $mode }");
/// A mode switch's choice of no mode of the group.
pub const CONTEXT_NO_MODE: StrKey = StrKey::new("egui-pop-context-no-mode", "None");
/// A transition refused, as a toast says it.
pub const CONTEXT_REFUSED: StrKey = StrKey::new("egui-pop-context-refused", "Not now: { $reason }");
/// What screen readers hear after a transition.
pub const CONTEXT_NOW_IN: StrKey = StrKey::new("egui-pop-context-now-in", "Now in { $place }");
/// An undo made in another place than the one shown.
pub const CONTEXT_UNDONE_IN: StrKey =
    StrKey::new("egui-pop-context-undone-in", "Undone in { $place }");
/// A redo made in another place than the one shown.
pub const CONTEXT_REDONE_IN: StrKey =
    StrKey::new("egui-pop-context-redone-in", "Redone in { $place }");
/// The toast's action going to where an undo or a redo happened.
pub const CONTEXT_GO_THERE: StrKey = StrKey::new("egui-pop-context-go-there", "Go there");
/// The history panel without any history.
pub const HISTORY_NONE: StrKey = StrKey::new("egui-pop-history-none", "No history");
/// The history panel's first row: before the first step.
pub const HISTORY_START: StrKey = StrKey::new("egui-pop-history-start", "Start");
/// A step of the history panel made by an agent, a script or another
/// process.
pub const HISTORY_BY: StrKey = StrKey::new("egui-pop-history-by", "{ $label } ({ $origin })");
/// What a row of the history panel does.
pub const HISTORY_JUMP: StrKey =
    StrKey::new("egui-pop-history-jump", "Go back or forward to this step");
/// The history panel's snapshots, as a heading.
pub const HISTORY_SNAPSHOTS: StrKey = StrKey::new("egui-pop-history-snapshots", "Snapshots");
/// The history panel's button pinning the current state.
pub const HISTORY_TAKE_SNAPSHOT: StrKey =
    StrKey::new("egui-pop-history-take-snapshot", "Take a snapshot");
/// A new snapshot's name, numbered.
pub const HISTORY_SNAPSHOT_NAME: StrKey =
    StrKey::new("egui-pop-history-snapshot-name", "Snapshot { $n }");
/// A snapshot row's action, and its context menu's first item.
pub const HISTORY_RESTORE: StrKey = StrKey::new("egui-pop-history-restore", "Restore");
/// What a snapshot row does.
pub const HISTORY_RESTORE_HINT: StrKey = StrKey::new(
    "egui-pop-history-restore-hint",
    "Go back to this snapshot (a new step)",
);
/// A snapshot row's context menu: delete it.
pub const HISTORY_DELETE_SNAPSHOT: StrKey =
    StrKey::new("egui-pop-history-delete-snapshot", "Delete the snapshot");
/// A step row's context menu: undo that one step, out of order.
pub const HISTORY_UNDO_THIS: StrKey = StrKey::new("egui-pop-history-undo-this", "Undo this step");
/// The history panel's toggle showing the steps new steps replaced.
pub const HISTORY_SHOW_REPLACED: StrKey =
    StrKey::new("egui-pop-history-show-replaced", "Show replaced steps");
/// A step a new step dropped from the redo list.
pub const HISTORY_REPLACED: StrKey =
    StrKey::new("egui-pop-history-replaced", "{ $label } (replaced)");
/// One line of a row's preview: a value there, and now.
pub const HISTORY_PREVIEW_CHANGE: StrKey = StrKey::new(
    "egui-pop-history-preview-change",
    "{ $what }: { $then } (now { $now })",
);
/// A crumb of the status bar's breadcrumb, going back to its place.
pub const CONTEXT_GO_TO: StrKey = StrKey::new("egui-pop-context-go-to", "Go to { $place }");
/// The status bar's last crumb, where the user is.
pub const CONTEXT_HERE: StrKey = StrKey::new("egui-pop-context-here", "You are in { $place }");
/// The status bar's `…` crumb, a menu of the places collapsed in it.
pub const CONTEXT_MORE_PLACES: StrKey = StrKey::new("egui-pop-context-more-places", "Places above");
/// A pushed mode's chip in the status bar, going back to what it replaced.
pub const CONTEXT_BACK_FROM_MODE: StrKey =
    StrKey::new("egui-pop-context-back-from-mode", "Back from { $mode }");
/// A chip whose mode refuses to be left now, with the guard's reason.
pub const CONTEXT_CANNOT_LEAVE: StrKey = StrKey::new(
    "egui-pop-context-cannot-leave",
    "{ $mode } stays on: { $reason }",
);

// The status bar.

/// A status item's context menu: hides it here.
pub const STATUS_HIDE: StrKey = StrKey::new("egui-pop-status-hide", "Hide { $item }");
/// The status bar's context menu: shows a hidden item again.
pub const STATUS_SHOW: StrKey = StrKey::new("egui-pop-status-show", "Show { $item }");
/// A status item's context menu: moves it one step towards the bar's start.
pub const STATUS_MOVE_START: StrKey =
    StrKey::new("egui-pop-status-move-start", "Move towards the start");
/// A status item's context menu: moves it one step towards the bar's end.
pub const STATUS_MOVE_END: StrKey = StrKey::new("egui-pop-status-move-end", "Move towards the end");
/// The status bar's overflow button: the items that gave way on a
/// narrow bar.
pub const STATUS_MORE: StrKey = StrKey::new("egui-pop-status-more", "More status items");
/// A status item's error count, as a screen reader says it.
pub const STATUS_ERRORS: StrKey = StrKey::new("egui-pop-status-errors", "Errors: { $count }");
/// A status item's warning count, as a screen reader says it.
pub const STATUS_WARNINGS: StrKey = StrKey::new("egui-pop-status-warnings", "Warnings: { $count }");
/// Announced once when a status item turns to errors.
pub const STATUS_ANNOUNCE: StrKey =
    StrKey::new("egui-pop-status-announce", "{ $item }: { $state }");
/// Frames of the saved stack that could not come back at start.
pub const CONTEXT_DROPPED: StrKey =
    StrKey::new("egui-pop-context-dropped", "Not restored: { $frames }");

// The clipboard.

/// A copy made with parts left out at the boundary, as a toast says it.
pub const CLIPBOARD_COPIED_STRIPPED: StrKey = StrKey::new(
    "egui-pop-clipboard-copied-stripped",
    "Copied; left out: { $what }",
);
/// A paste through a lossy transform, as a toast says it.
pub const CLIPBOARD_PASTED_LOSSY: StrKey =
    StrKey::new("egui-pop-clipboard-pasted-lossy", "Pasted; lost: { $what }");
/// A copy refused or vetoed, as a toast says it.
pub const CLIPBOARD_NOT_COPIED: StrKey =
    StrKey::new("egui-pop-clipboard-not-copied", "Not copied: { $reason }");
/// A paste refused or vetoed, as a toast says it.
pub const CLIPBOARD_NOT_PASTED: StrKey =
    StrKey::new("egui-pop-clipboard-not-pasted", "Not pasted: { $reason }");
/// Paste special's dialog title.
pub const CLIPBOARD_PASTE_SPECIAL: StrKey =
    StrKey::new("egui-pop-clipboard-paste-special", "Paste special");
/// A way to paste: in a format.
pub const CLIPBOARD_AS: StrKey = StrKey::new("egui-pop-clipboard-as", "As { $format }");
/// A way to paste: in a format, through a transform.
pub const CLIPBOARD_AS_THROUGH: StrKey = StrKey::new(
    "egui-pop-clipboard-as-through",
    "As { $format }, through { $transform }",
);
/// A way to paste: one part of the item alone.
pub const CLIPBOARD_ONLY_PART: StrKey =
    StrKey::new("egui-pop-clipboard-only-part", "Only { $part }");
/// What a lossy way to paste loses.
pub const CLIPBOARD_LOSES: StrKey = StrKey::new("egui-pop-clipboard-loses", "loses { $what }");
/// A paste's undo step, by the item's title.
pub const CLIPBOARD_PASTE_STEP: StrKey =
    StrKey::new("egui-pop-clipboard-paste-step", "Paste { $title }");
/// A cut's undo step, by the item's title.
pub const CLIPBOARD_CUT_STEP: StrKey = StrKey::new("egui-pop-clipboard-cut-step", "Cut { $title }");
/// Plain text, as a format and as the title of text from outside.
pub const CLIPBOARD_TEXT: StrKey = StrKey::new("egui-pop-clipboard-text", "text");

/// Every string of this module, for catalogues and tests.
pub const ALL: &[StrKey] = &[
    COMMAND_LABEL,
    COMMAND_WITH_CHANGE,
    NOTHING_MATCHES,
    MENU_TOO_DEEP,
    MENU_EMPTY,
    MENU_MORE,
    MENU_MAIN,
    TOOLBAR_MORE,
    TOOLBAR_WITH_KEY,
    MENU_DOES_NOT_APPLY,
    MENU_NOT_AVAILABLE,
    MENU_NO_COMMAND,
    MENU_RUNNING,
    MENU_LAST_RUN_FAILED,
    PALETTE_TITLE,
    PALETTE_HINT,
    PALETTE_RECENT,
    PALETTE_RECENT_KEYS,
    PALETTE_NO_MATCH,
    PALETTE_ARGUMENT_TITLE,
    PALETTE_ARGUMENT_HELP,
    PALETTE_NOT_BOOL,
    PALETTE_NOT_NUMBER,
    PALETTE_NOT_JSON,
    SETTINGS_TITLE,
    SETTINGS_SEARCH_HINT,
    SETTINGS_SHORTCUTS,
    SETTINGS_FONTS,
    FONTS_THEMES,
    FONTS_PREVIEW,
    FONTS_NUMBERS,
    FONTS_PICKER_HINT,
    FONTS_ORIGIN_BUNDLED,
    FONTS_ORIGIN_REGISTERED,
    FONTS_ORIGIN_FILE,
    FONTS_ORIGIN_SYSTEM,
    SETTINGS_UNDO,
    SETTINGS_REDO,
    SETTINGS_UNDO_STEP,
    SETTINGS_REDO_STEP,
    SETTINGS_NOTHING_TO_UNDO,
    SETTINGS_NOTHING_TO_REDO,
    SETTINGS_UNDO_NAME,
    SETTINGS_REDO_NAME,
    SETTINGS_RESET_SETTING,
    SETTINGS_AT_DEFAULT,
    SETTINGS_RESET_PAGE,
    SETTINGS_RESET_PAGE_NAME,
    SETTINGS_PAGE_AT_DEFAULT,
    SETTINGS_OPEN_PAGE,
    SETTINGS_OPEN_PAGE_NAME,
    SHORTCUTS_SEARCH_BY_KEY,
    SHORTCUTS_PRESS_TO_SEARCH,
    SHORTCUTS_BOUND_TO,
    SHORTCUTS_SHOW_ALL,
    SHORTCUTS_POINTER_PROFILE,
    SHORTCUTS_POINTER_PROFILE_CHANGED,
    SHORTCUTS_POINTER_ONLY,
    SHORTCUTS_POINTER_ADD,
    SHORTCUTS_POINTER_ADD_NAME,
    SHORTCUTS_POINTER_REMOVE_NAME,
    SHORTCUTS_POINTER_PAD,
    SHORTCUTS_POINTER_PAD_NAME,
    SHORTCUTS_POINTER_USE,
    SHORTCUTS_POINTER_ALT,
    SHORTCUTS_WIDGET_USES,
    SHORTCUTS_WIDGET_USES_NOTE,
    POINTER_OVERRIDES_TITLE,
    POINTER_OVERRIDES_DETAIL,
    POINTER_KEEP,
    POINTER_DROP,
    SHORTCUTS_CONFLICTS,
    SHORTCUTS_CONFLICT_IN_SCOPE,
    SHORTCUTS_PRESS_NEW_KEYS,
    SHORTCUTS_GOES_TO,
    SHORTCUTS_ALSO_BOUND,
    SHORTCUTS_SYSTEM_ON,
    SHORTCUTS_SYSTEM_PENDING,
    SHORTCUTS_SYSTEM_FAILED,
    SHORTCUTS_SYSTEM_UNAVAILABLE,
    SHORTCUTS_SYSTEM_NO_PLUGIN,
    SHORTCUTS_CHANGE,
    SHORTCUTS_CHANGE_NAME,
    SHORTCUTS_NO_KEY,
    SHORTCUTS_NO_KEY_NAME,
    SHORTCUTS_DEFAULT_NAME,
    SHORTCUTS_FROM_WORKSPACE,
    SHORTCUTS_FROM_USER,
    SHORTCUTS_KEY_FROM,
    SHORTCUTS_TARGET,
    SHORTCUTS_TARGET_USER,
    SHORTCUTS_TARGET_WORKSPACE,
    DIALOG_FILTER,
    DIALOG_BROWSE,
    PICKER_UP,
    PICKER_CHOOSE_FOLDER,
    PICKER_CLOSE,
    FILES_FILTER,
    FILES_NAME,
    FILES_BOOKMARK,
    FILES_UNBOOKMARK,
    FILES_HOME,
    FILES_RECENT,
    FILES_BOOKMARKS,
    FILES_ALL,
    FILES_HIDDEN,
    FILES_OPEN,
    FILES_SAVE,
    FILES_PREVIEW,
    FILES_PREVIEW_LOADING,
    FILES_PREVIEW_NONE,
    FILES_PREVIEW_FAILED,
    FILES_SIZE,
    FILES_ITEMS,
    FILES_DIMENSIONS,
    FILES_PAGES,
    FILES_MODIFIED,
    TOAST_INFO,
    TOAST_SUCCESS,
    TOAST_WARNING,
    TOAST_ERROR,
    TOAST_CLOSE,
    JOBS_ACTIVE,
    JOBS_ACTIVE_PERCENT,
    JOBS_FAILED,
    JOBS_TITLE,
    JOBS_NONE,
    JOBS_HISTORY,
    JOBS_CLEAR,
    JOBS_OPEN_PANEL,
    JOB_RUNNING,
    JOB_PAUSED,
    JOB_CANCELLING,
    JOB_DONE,
    JOB_FAILED,
    JOB_CANCELLED,
    JOB_ASKED,
    JOB_OF,
    JOB_ETA_SECONDS,
    JOB_ETA_MINUTES,
    JOB_PAUSE,
    JOB_RESUME,
    JOB_DISMISS,
    JOB_ACTION_NAME,
    JOB_SHOW_LOG,
    JOB_HIDE_LOG,
    JOB_REFUSED,
    JOB_NO_LOG,
    FRAME_WINDOW_MENU,
    WINDOW_EXIT_FULLSCREEN,
    AREA_CENTER,
    AREA_START_SIDE,
    AREA_END_SIDE,
    AREA_BOTTOM,
    WORKBENCH_MOVE_TO,
    WORKBENCH_STAYS_OUT,
    WORKBENCH_DETACH,
    WORKBENCH_CLOSE_PANEL,
    WORKBENCH_FLOAT,
    WORKBENCH_UNFOLD,
    WORKBENCH_FOLD,
    WORKBENCH_FLYOUT_PIN,
    PALETTE_CLOSE,
    PALETTE_CLOSE_GROUP,
    PALETTE_GROUP_MENU,
    PALETTE_ROLL_UP,
    PALETTE_ROLL_DOWN,
    PALETTE_PIN,
    PALETTE_UNPIN,
    WORKBENCH_WINDOW_TITLE,
    ZEN_HINT,
    ZEN_HINT_NO_KEY,
    LAYOUT_CUSTOMIZE,
    LAYOUT_WITH_KEY,
    LAYOUT_SAVE_AS,
    LAYOUT_SIDE_BAR_LEFT,
    LAYOUT_SIDE_BAR_RIGHT,
    LAYOUT_PANEL_CENTER,
    LAYOUT_PANEL_JUSTIFY,
    ARRANGEMENT_SAVE_TITLE,
    ARRANGEMENT_SAVE_VERB,
    ARRANGEMENT_NAME,
    ARRANGEMENT_SUGGESTED,
    PANEL_FOCUS_TITLE,
    PANEL_FOCUS_DESCRIPTION,
    PANEL_TOGGLE_TITLE,
    PANEL_TOGGLE_DESCRIPTION,
    DEVTOOLS_OFF,
    DEVTOOLS_TAB_KEYS,
    DEVTOOLS_TAB_JOURNAL,
    DEVTOOLS_TAB_STEPS,
    DEVTOOLS_TAB_FOCUS,
    DEVTOOLS_TAB_SCALE,
    DEVTOOLS_TAB_COLLECTORS,
    DEVTOOLS_WAITING,
    DEVTOOLS_PRESS_A_KEY,
    DEVTOOLS_NO_HOOKS,
    DEVTOOLS_HOOK_BY,
    DEVTOOLS_FOCUS_CHAIN,
    DEVTOOLS_OVERLAYS,
    DEVTOOLS_CONTEXT_KEYS,
    DEVTOOLS_ARGUMENTS,
    DEVTOOLS_MUTATES,
    THEME_EDITOR_MISSING,
    THEME_EDITOR_START,
    THEME_EDITOR_UNDO,
    THEME_EDITOR_REDO,
    THEME_EDITOR_CHANGE_OF,
    THEME_EDITOR_CHANGED_COUNT,
    THEME_EDITOR_FILTER,
    THEME_EDITOR_SURFACES,
    THEME_EDITOR_TEXT,
    THEME_EDITOR_ACCENT,
    THEME_EDITOR_STATES,
    THEME_EDITOR_BORDERS,
    THEME_EDITOR_SHAPE,
    THEME_EDITOR_COMPONENTS,
    THEME_EDITOR_PALETTE,
    THEME_EDITOR_EDITED,
    THEME_EDITOR_PICK,
    THEME_EDITOR_STEP,
    THEME_EDITOR_STEP_MANY,
    THEME_EDITOR_TYPE,
    THEME_EDITOR_OWN,
    THEME_EDITOR_FROM,
    THEME_EDITOR_ALIAS_OF,
    THEME_EDITOR_VALUE,
    THEME_EDITOR_VALUE_OF,
    THEME_EDITOR_ADJUST_OF,
    THEME_EDITOR_NOT_A_VALUE,
    THEME_EDITOR_RESET,
    THEME_EDITOR_CONTRAST,
    THEME_EDITOR_NO_PAIRS,
    THEME_EDITOR_PAIR,
    THEME_EDITOR_PAIR_ON,
    THEME_EDITOR_RATIO,
    THEME_EDITOR_THEME_NEEDS,
    THEME_EDITOR_PASS,
    THEME_EDITOR_FAIL,
    THEME_EDITOR_NOT_APPLICABLE,
    THEME_EDITOR_FILE,
    THEME_EDITOR_NAME,
    THEME_EDITOR_EXPORT,
    THEME_EDITOR_IMPORT,
    THEME_EDITOR_FILE_TEXT,
    THEME_EDITOR_GENERATE_TITLE,
    THEME_EDITOR_SEED_OF,
    THEME_EDITOR_SEED_HEX_OF,
    THEME_EDITOR_GENERATE,
    THEME_EDITOR_GENERATED,
    THEME_EDITOR_SEED_CLAMPED,
    THEME_EDITOR_GENERATE_VARIANT,
    THEME_EDITOR_VARIANT,
    THEME_EDITOR_VARIANT_HINT,
    THEME_EDITOR_DARK,
    THEME_EDITOR_LIGHT,
    THEME_EDITOR_OTHER,
    THEME_EDITOR_OTHER_HINT,
    THEME_EDITOR_OTHER_TITLE,
    THEME_EDITOR_OTHER_PREVIEWED,
    THEME_EDITOR_OTHER_NONE,
    THEME_EDITOR_OTHER_APPLY,
    THEME_EDITOR_OTHER_CANCEL,
    THEME_EDITOR_OTHER_WRITTEN,
    THEME_EDITOR_LC_HINT,
    THEME_EDITOR_TAB_TOKENS,
    THEME_EDITOR_TAB_FONTS,
    THEME_EDITOR_FAMILIES,
    THEME_EDITOR_FAMILIES_HINT,
    THEME_EDITOR_FAMILY_UI,
    THEME_EDITOR_FAMILY_MONO,
    THEME_EDITOR_MOVE_UP,
    THEME_EDITOR_MOVE_DOWN,
    THEME_EDITOR_REMOVE_FAMILY,
    THEME_EDITOR_ADD_FAMILY_TO,
    THEME_EDITOR_ADD,
    THEME_EDITOR_FONT_FILES,
    THEME_EDITOR_NO_FONT_FILES,
    THEME_EDITOR_REMOVE_FONT_FILE,
    THEME_EDITOR_ADD_FONT_FILE,
    THEME_EDITOR_FONT_ADDED,
    THEME_EDITOR_FONT_SAMPLE,
    THEME_EDITOR_FONT_SAMPLE_TEXT,
    THEME_EDITOR_FONT_SAMPLE_HINT,
    THEME_EDITOR_APPLY_FONTS,
    THEME_EDITOR_CANCEL_FONTS,
    THEME_EDITOR_FAMILY_MISSING,
    THEME_EDITOR_INSTALLED,
    THEME_EDITOR_INSTALLED_HINT,
    THEME_EDITOR_FOR_UI,
    THEME_EDITOR_FOR_MONO,
    THEME_EDITOR_SEARCH_FAMILIES,
    THEME_EDITOR_NO_FAMILY_MATCHES,
    THEME_EDITOR_APPLY_FONTS_HINT,
    THEME_EDITOR_PREVIEW_ONLY,
    THEME_EDITOR_PREVIEW_ONLY_HINT,
    THEME_EDITOR_APPLY,
    THEME_EDITOR_DISCARD,
    THEME_EDITOR_PREVIEW,
    THEME_EDITOR_NOT_APPLIED,
    THEME_EDITOR_HEX_OF,
    THEME_EDITOR_OPEN_FILE,
    THEME_EDITOR_SAVE_FILE,
    THEME_EDITOR_SAVED,
    THEME_EDITOR_SAMPLE_HEADING,
    THEME_EDITOR_SAMPLE_TEXT,
    THEME_EDITOR_SAMPLE_MUTED,
    THEME_EDITOR_SAMPLE_BUTTON,
    THEME_EDITOR_SAMPLE_PRIMARY,
    THEME_EDITOR_SAMPLE_DANGER,
    THEME_EDITOR_SAMPLE_DISABLED,
    THEME_EDITOR_SAMPLE_CHECKBOX,
    THEME_EDITOR_SAMPLE_OPTION_A,
    THEME_EDITOR_SAMPLE_OPTION_B,
    THEME_EDITOR_SAMPLE_SLIDER,
    THEME_EDITOR_SAMPLE_FIELD,
    THEME_EDITOR_SAMPLE_SELECTED,
    THEME_EDITOR_IMPORTED,
    THEME_EDITOR_SKIPPED,
    THEME_EDITOR_WARNINGS,
    THEME_EDITOR_PROBLEMS,
    TIPS_OF_THE_DAY,
    TIPS_HINT,
    TIPS_PREVIOUS,
    TIPS_NEXT,
    TIPS_AT,
    TIPS_DONT_SHOW,
    TIPS_GOT_IT,
    TIPS_GOT_IT_NAME,
    TIPS_DID_YOU_KNOW,
    TIPS_NOT_NOW,
    TIPS_NO_MORE,
    TIPS_BROWSER,
    TIPS_NONE,
    TIPS_ORIGIN_ON,
    TIPS_ORIGIN_ON_NAME,
    TIPS_STATE_DISMISSED,
    TIPS_STATE_USED,
    TIPS_SHOW,
    TIPS_SHOW_NAME,
    ABOUT_TITLE,
    ABOUT_VERSION,
    ABOUT_LICENCE,
    ABOUT_AUTHORS,
    ABOUT_REPOSITORY,
    ABOUT_WEBSITE,
    ABOUT_DOCS,
    ABOUT_ISSUES,
    ABOUT_COPY,
    ABOUT_COPIED,
    CONTEXT_ROOT,
    CONTEXT_LEAVE_MODE,
    CONTEXT_NO_MODE,
    CONTEXT_REFUSED,
    CONTEXT_NOW_IN,
    CONTEXT_UNDONE_IN,
    CONTEXT_REDONE_IN,
    CONTEXT_GO_THERE,
    HISTORY_NONE,
    HISTORY_START,
    HISTORY_BY,
    HISTORY_JUMP,
    HISTORY_SNAPSHOTS,
    HISTORY_TAKE_SNAPSHOT,
    HISTORY_SNAPSHOT_NAME,
    HISTORY_RESTORE,
    HISTORY_RESTORE_HINT,
    HISTORY_DELETE_SNAPSHOT,
    HISTORY_UNDO_THIS,
    HISTORY_SHOW_REPLACED,
    HISTORY_REPLACED,
    HISTORY_PREVIEW_CHANGE,
    CONTEXT_GO_TO,
    CONTEXT_HERE,
    CONTEXT_MORE_PLACES,
    CONTEXT_BACK_FROM_MODE,
    CONTEXT_CANNOT_LEAVE,
    CONTEXT_DROPPED,
    STATUS_HIDE,
    STATUS_SHOW,
    STATUS_MOVE_START,
    STATUS_MOVE_END,
    CLIPBOARD_COPIED_STRIPPED,
    CLIPBOARD_PASTED_LOSSY,
    CLIPBOARD_NOT_COPIED,
    CLIPBOARD_NOT_PASTED,
    CLIPBOARD_PASTE_SPECIAL,
    CLIPBOARD_AS,
    CLIPBOARD_AS_THROUGH,
    CLIPBOARD_ONLY_PART,
    CLIPBOARD_LOSES,
    CLIPBOARD_PASTE_STEP,
    CLIPBOARD_CUT_STEP,
    CLIPBOARD_TEXT,
    STATUS_MORE,
    STATUS_ERRORS,
    STATUS_WARNINGS,
    STATUS_ANNOUNCE,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn is_fluent_id(key: &str) -> bool {
        key.strip_prefix("egui-pop-").is_some_and(|rest| {
            !rest.is_empty()
                && rest
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
    }

    /// The `{ $name }` placeholders of a text; `Err` on a brace that
    /// opens none.
    fn placeholders(text: &str) -> Result<Vec<&str>, String> {
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(at) = rest.find(['{', '}']) {
            let tail = &rest[at..];
            if tail.starts_with('}') {
                return Err(format!("a lone `}}` in {text:?}"));
            }
            let end = tail
                .find('}')
                .ok_or_else(|| format!("an unclosed `{{` in {text:?}"))?;
            let inner = &tail[1..end];
            let name = inner
                .strip_prefix(" $")
                .and_then(|n| n.strip_suffix(' '))
                .filter(|n| {
                    n.starts_with(|c: char| c.is_ascii_lowercase())
                        && n.chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                })
                .ok_or_else(|| format!("`{{{inner}}}` is not `{{ $name }}` in {text:?}"))?;
            out.push(name);
            rest = &tail[end + 1..];
        }
        Ok(out)
    }

    #[test]
    fn keys_are_unique_fluent_ids_with_well_formed_placeholders() {
        let mut seen = std::collections::BTreeSet::new();
        for k in ALL {
            assert!(seen.insert(k.key), "`{}` listed twice", k.key);
            assert!(is_fluent_id(k.key), "`{}` is not a Fluent id", k.key);
            assert!(!k.fallback.is_empty(), "`{}` has no English", k.key);
            if let Err(e) = placeholders(k.fallback) {
                panic!("`{}`: {e}", k.key);
            }
        }
        for k in egui_pop_core::strings::ALL {
            assert!(!seen.contains(k.key), "`{}` is core's", k.key);
        }
        for k in DEVTOOLS_TABS {
            assert!(ALL.contains(&k));
        }
    }

    #[test]
    fn placeholders_are_checked() {
        assert_eq!(placeholders("Close { $panel }"), Ok(vec!["panel"]));
        assert!(placeholders("Close {panel}").is_err());
        assert!(placeholders("Close { $panel").is_err());
        assert!(placeholders("Close }").is_err());
    }
}
