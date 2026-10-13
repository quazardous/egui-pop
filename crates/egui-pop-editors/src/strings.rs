//! The strings the editors show, by key with English fallbacks,
//! translated through the provider installed with
//! `egui_pop_core::strings::install`.

use egui_pop_core::strings::StrKey;

/// A text editor's accessible name, unless the application gives one.
pub const TEXT: StrKey = StrKey::new("egui-pop-editors-text", "Text editor");
/// A code editor's accessible name, unless the application gives one.
pub const CODE: StrKey = StrKey::new("egui-pop-editors-code", "Code editor");
/// The search field.
pub const FIND: StrKey = StrKey::new("egui-pop-editors-find", "Find");
/// The replacement field.
pub const REPLACE: StrKey = StrKey::new("egui-pop-editors-replace", "Replace with");
/// Replaces the current match.
pub const REPLACE_ONE: StrKey = StrKey::new("egui-pop-editors-replace-one", "Replace");
/// Replaces every match.
pub const REPLACE_ALL: StrKey = StrKey::new("egui-pop-editors-replace-all", "Replace all");
/// Shows or hides the replacement field.
pub const TOGGLE_REPLACE: StrKey = StrKey::new("egui-pop-editors-toggle-replace", "Show replace");
pub const MATCH_CASE: StrKey = StrKey::new("egui-pop-editors-match-case", "Match case");
pub const WHOLE_WORD: StrKey = StrKey::new("egui-pop-editors-whole-word", "Whole word");
pub const REGEX: StrKey = StrKey::new("egui-pop-editors-regex", "Regular expression");
pub const PREVIOUS: StrKey = StrKey::new("egui-pop-editors-previous", "Previous match");
pub const NEXT: StrKey = StrKey::new("egui-pop-editors-next", "Next match");
/// Closes the search bar.
pub const CLOSE: StrKey = StrKey::new("egui-pop-editors-close", "Close the search");
/// The current match among all.
pub const COUNT: StrKey = StrKey::new("egui-pop-editors-count", "{ $current } of { $total }");
pub const NO_RESULTS: StrKey = StrKey::new("egui-pop-editors-no-results", "No results");
/// The go-to-line field.
pub const GOTO: StrKey = StrKey::new("egui-pop-editors-goto", "Go to line");
/// The cursor's place, under an editor.
pub const POSITION: StrKey = StrKey::new(
    "egui-pop-editors-position",
    "Line { $line }, column { $col }",
);

/// A terminal's accessible name, unless the application gives one.
pub const TERMINAL: StrKey = StrKey::new("egui-pop-editors-terminal", "Terminal");
/// The terminal's notice when its program ended with a code.
pub const TERMINAL_EXITED: StrKey = StrKey::new(
    "egui-pop-editors-terminal-exited",
    "Process exited with code { $code }",
);
/// The terminal's notice when its program ended without a code.
pub const TERMINAL_ENDED: StrKey = StrKey::new("egui-pop-editors-terminal-ended", "Process ended");
/// The terminal's notice when its session was closed.
pub const TERMINAL_CLOSED: StrKey =
    StrKey::new("egui-pop-editors-terminal-closed", "Session closed");
/// The terminal's notice when its program was not started.
pub const TERMINAL_REFUSED: StrKey = StrKey::new(
    "egui-pop-editors-terminal-refused",
    "Not started: { $reason }",
);

/// A job's log console's accessible name.
pub const JOB_LOG: StrKey = StrKey::new("egui-pop-editors-job-log", "Log of { $job }");

/// Every string the editors show, for catalogues and tests.
pub const ALL: &[StrKey] = &[
    TEXT,
    CODE,
    FIND,
    REPLACE,
    REPLACE_ONE,
    REPLACE_ALL,
    TOGGLE_REPLACE,
    MATCH_CASE,
    WHOLE_WORD,
    REGEX,
    PREVIOUS,
    NEXT,
    CLOSE,
    COUNT,
    NO_RESULTS,
    GOTO,
    POSITION,
    TERMINAL,
    TERMINAL_EXITED,
    TERMINAL_ENDED,
    TERMINAL_CLOSED,
    TERMINAL_REFUSED,
    JOB_LOG,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_fluent_ids() {
        let mut seen = std::collections::BTreeSet::new();
        for k in ALL {
            assert!(seen.insert(k.key), "`{}` twice", k.key);
            assert!(k.key.starts_with("egui-pop-editors-"), "{}", k.key);
        }
    }
}
