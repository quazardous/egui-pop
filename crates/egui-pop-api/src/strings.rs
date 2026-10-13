//! The text egui-pop's application interface shows itself (the palette's
//! prompt, the settings window's buttons, the shortcuts editor, dialogs'
//! buttons, toasts, devtools, the workbench's tab menu...), by key with
//! English fallbacks, read through `egui_pop_core::strings` so that the
//! installed provider translates it. The OK, Cancel, Close, Reset and
//! Retry buttons use core's keys ([`egui_pop_core::strings::ALL`]).
//!
//! What plugins declare (command titles, categories, panel titles,
//! submenu labels, settings, plugin titles) is not here: it is
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

// The shortcuts editor.

pub const SHORTCUTS_SEARCH_BY_KEY: StrKey =
    StrKey::new("egui-pop-shortcuts-search-by-key", "Search by key");
pub const SHORTCUTS_PRESS_TO_SEARCH: StrKey = StrKey::new(
    "egui-pop-shortcuts-press-to-search",
    "Press the keys to search for. Escape: cancel.",
);
pub const SHORTCUTS_BOUND_TO: StrKey =
    StrKey::new("egui-pop-shortcuts-bound-to", "Bound to { $keys }");
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

// Palettes.

pub const PALETTE_ROLL_UP: StrKey = StrKey::new("egui-pop-palette-roll-up", "Roll up { $palette }");
pub const PALETTE_ROLL_DOWN: StrKey =
    StrKey::new("egui-pop-palette-roll-down", "Roll down { $palette }");
pub const PALETTE_CLOSE: StrKey = StrKey::new("egui-pop-palette-close", "Close { $palette }");
pub const PALETTE_PIN: StrKey = StrKey::new("egui-pop-palette-pin", "Pin { $palette }");
pub const PALETTE_UNPIN: StrKey = StrKey::new("egui-pop-palette-unpin", "Unpin { $palette }");
/// A detached panel's window title, with the application's.
pub const WORKBENCH_WINDOW_TITLE: StrKey = StrKey::new(
    "egui-pop-workbench-window-title",
    "{ $panel } — { $application }",
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

/// Every string of this module, for catalogues and tests.
pub const ALL: &[StrKey] = &[
    COMMAND_LABEL,
    COMMAND_WITH_CHANGE,
    NOTHING_MATCHES,
    MENU_TOO_DEEP,
    MENU_EMPTY,
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
    SHORTCUTS_SEARCH_BY_KEY,
    SHORTCUTS_PRESS_TO_SEARCH,
    SHORTCUTS_BOUND_TO,
    SHORTCUTS_SHOW_ALL,
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
    TOAST_INFO,
    TOAST_SUCCESS,
    TOAST_WARNING,
    TOAST_ERROR,
    TOAST_CLOSE,
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
    PALETTE_CLOSE,
    PALETTE_ROLL_UP,
    PALETTE_ROLL_DOWN,
    PALETTE_PIN,
    PALETTE_UNPIN,
    WORKBENCH_WINDOW_TITLE,
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
