//! The editors' component tokens (`editor.*` and `syntax.*` in the theme
//! files), each an alias of a system token.

use egui::{Color32, Context};
use egui_pop_core::theme::Tokens;

use crate::highlight::Kind;

/// The editors' colours, from the installed theme.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorTokens {
    pub background: Color32,
    pub gutter: Color32,
    pub line_number: Color32,
    pub line_number_active: Color32,
    pub current_line: Color32,
    pub selection: Color32,
    pub cursor: Color32,
    pub indent_guide: Color32,
    pub bracket: Color32,
    pub search_match: Color32,
    pub match_current: Color32,
    pub fold: Color32,
    pub breakpoint: Color32,
    pub error: Color32,
    pub warning: Color32,
    pub info: Color32,
    pub diff_added: Color32,
    pub diff_modified: Color32,
    pub diff_removed: Color32,
    /// `syntax.*`, by [`Kind`] in [`Kind::ALL`] order.
    pub syntax: [Color32; 14],
}

impl EditorTokens {
    /// The tokens of the theme installed in `ctx`.
    pub fn of(ctx: &Context) -> EditorTokens {
        let t = Tokens::of(ctx);
        let c = |path: &str, fallback: Color32| t.color(path).unwrap_or(fallback);
        let syntax = Kind::ALL.map(|k| c(&format!("syntax.{}", k.token()), t.text.default));
        EditorTokens {
            background: c("editor.background", t.surface.panel),
            gutter: c("editor.gutter", t.surface.panel),
            line_number: c("editor.line-number", t.text.muted),
            line_number_active: c("editor.line-number-active", t.text.strong),
            current_line: c("editor.current-line", t.surface.hover),
            selection: c("editor.selection", t.selection),
            cursor: c("editor.cursor", t.text.strong),
            indent_guide: c("editor.indent-guide", t.border.default),
            bracket: c("editor.bracket", t.border.strong),
            search_match: c("editor.match", t.accent.subtle),
            match_current: c("editor.match-current", t.accent.default),
            fold: c("editor.fold", t.text.muted),
            breakpoint: c("editor.breakpoint", t.status.danger),
            error: c("editor.error", t.status.danger),
            warning: c("editor.warning", t.status.warning),
            info: c("editor.info", t.status.info),
            diff_added: c("editor.diff-added", t.status.success),
            diff_modified: c("editor.diff-modified", t.status.info),
            diff_removed: c("editor.diff-removed", t.status.danger),
            syntax,
        }
    }

    /// The colour of a kind of code.
    pub fn kind(&self, k: Kind) -> Color32 {
        let i = Kind::ALL.iter().position(|x| *x == k).unwrap_or(0);
        self.syntax[i]
    }
}
