//! The contract of the default kit's components (`docs/kit.md`), one
//! module each. One line per component in [`GROUPS`], in alphabetical
//! order.

use super::Required;

mod backdrop;
mod dialog;
mod fault_card;
mod fullscreen_exit;
mod job_log;
mod palette_window;
mod popover;
mod tab;
mod tab_bar;
mod tip_card;
mod title_bar;
mod toast;
mod window;
mod window_button;
mod window_close;

/// Each component's contract.
pub(super) const GROUPS: &[&[Required]] = &[
    backdrop::CONTRACT,
    dialog::CONTRACT,
    fault_card::CONTRACT,
    fullscreen_exit::CONTRACT,
    job_log::CONTRACT,
    palette_window::CONTRACT,
    popover::CONTRACT,
    tab::CONTRACT,
    tab_bar::CONTRACT,
    tip_card::CONTRACT,
    title_bar::CONTRACT,
    toast::CONTRACT,
    window::CONTRACT,
    window_button::CONTRACT,
    window_close::CONTRACT,
];
