//! What this crate's components need, in the mount contract's terms
//! (ADR 0013): `docs/hosts.md` and `AppHandle::where_can` are made from
//! these.

use egui_pop_model::mount::{ClaimKey, Keyboard, Mountable};

/// The text editor.
pub const TEXT_EDITOR: &str = "text-editor";
/// The code editor.
pub const CODE_EDITOR: &str = "code-editor";
/// The terminal.
pub const TERMINAL: &str = "terminal";

/// The text editor: unsaved text is a session.
pub fn text_editor() -> Mountable {
    Mountable::new(TEXT_EDITOR, "Text editor")
        .crate_name("egui-pop-editors")
        .claims(&[ClaimKey::Enter, ClaimKey::Tab, ClaimKey::Escape])
        .surface("editor")
        .drag()
        .fills()
        .session()
        .role("text")
}

/// The code editor: the text editor with assist popups and providers
/// answering from any thread.
pub fn code_editor() -> Mountable {
    Mountable::new(CODE_EDITOR, "Code editor")
        .crate_name("egui-pop-editors")
        .claims(&[ClaimKey::Enter, ClaimKey::Tab, ClaimKey::Escape])
        .surface("editor")
        .drag()
        .workers()
        .fills()
        .session()
        .popups()
        .role("text")
}

/// The terminal: every key while focused (Shift+Escape leaves), a
/// program running behind it.
pub fn terminal() -> Mountable {
    Mountable::new(TERMINAL, "Terminal")
        .crate_name("egui-pop-editors")
        .keyboard(Keyboard::Exclusive)
        .claims(&[ClaimKey::Enter, ClaimKey::Tab, ClaimKey::Escape])
        .surface("terminal")
        .drag()
        .workers()
        .min(32.0, 32.0)
        .fills()
        .session()
        .role("terminal")
}

/// This crate's components.
pub fn components() -> Vec<Mountable> {
    vec![text_editor(), code_editor(), terminal()]
}
