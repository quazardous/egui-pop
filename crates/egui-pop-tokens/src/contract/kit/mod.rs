//! The contract of the default kit's components (`docs/kit.md`), one
//! module each. One line per component in [`GROUPS`], in alphabetical
//! order.

use super::Required;

mod glyph;
mod list_row;
mod list_view;
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
mod table_header_cell;
mod table_stripe;
mod title_bar;
mod tree_drop_marker;
mod window;
mod window_button;
mod window_close;

/// Each component's contract.
pub(super) const GROUPS: &[&[Required]] = &[
    glyph::CONTRACT,
    list_row::CONTRACT,
    list_view::CONTRACT,
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
    table_header_cell::CONTRACT,
    table_stripe::CONTRACT,
    title_bar::CONTRACT,
    tree_drop_marker::CONTRACT,
    window::CONTRACT,
    window_button::CONTRACT,
    window_close::CONTRACT,
];
