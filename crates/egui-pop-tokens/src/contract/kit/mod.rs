//! The contract of the default kit's components (`docs/kit.md`), one
//! module each. One line per component in [`GROUPS`], in alphabetical
//! order.

use super::Required;

mod meter;
mod meter_tick;
mod progress_bar;
mod progress_caption;
mod progress_fill;
mod progress_line;
mod progress_ring;
mod spinner;
mod status_indicator;
mod tab;
mod tab_bar;
mod title_bar;
mod window;
mod window_button;
mod window_close;

/// Each component's contract.
pub(super) const GROUPS: &[&[Required]] = &[
    meter::CONTRACT,
    meter_tick::CONTRACT,
    progress_bar::CONTRACT,
    progress_caption::CONTRACT,
    progress_fill::CONTRACT,
    progress_line::CONTRACT,
    progress_ring::CONTRACT,
    spinner::CONTRACT,
    status_indicator::CONTRACT,
    tab::CONTRACT,
    tab_bar::CONTRACT,
    title_bar::CONTRACT,
    window::CONTRACT,
    window_button::CONTRACT,
    window_close::CONTRACT,
];
