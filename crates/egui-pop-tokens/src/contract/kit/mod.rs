//! The contract of the default kit's components (`docs/kit.md`), one
//! module each. One line per component in [`GROUPS`], in alphabetical
//! order.

use super::Required;

mod assist_card;
mod backdrop;
mod dialog;
mod editor_frame;
mod fault_card;
mod find_bar;
mod fullscreen_exit;
mod job_log;
mod palette_window;
mod popover;
mod tab;
mod tab_bar;
mod terminal_frame;
mod tip_card;
mod title_bar;
mod toast;
mod window;
mod window_button;
mod window_close;

/// Each component's contract.
pub(super) const GROUPS: &[&[Required]] = &[
    assist_card::CONTRACT,
    backdrop::CONTRACT,
    dialog::CONTRACT,
    editor_frame::CONTRACT,
    fault_card::CONTRACT,
    find_bar::CONTRACT,
    fullscreen_exit::CONTRACT,
    job_log::CONTRACT,
    palette_window::CONTRACT,
    popover::CONTRACT,
    tab::CONTRACT,
    tab_bar::CONTRACT,
    terminal_frame::CONTRACT,
    tip_card::CONTRACT,
    title_bar::CONTRACT,
    toast::CONTRACT,
    window::CONTRACT,
    window_button::CONTRACT,
    window_close::CONTRACT,
];
