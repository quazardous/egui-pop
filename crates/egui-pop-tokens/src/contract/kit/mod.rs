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
mod glyph;
mod job_log;
mod list_row;
mod list_view;
mod meter;
mod meter_tick;
mod palette_window;
mod popover;
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
mod terminal_frame;
mod tip_card;
mod title_bar;
mod toast;
mod tree_drop_marker;
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
    glyph::CONTRACT,
    job_log::CONTRACT,
    list_row::CONTRACT,
    list_view::CONTRACT,
    meter::CONTRACT,
    meter_tick::CONTRACT,
    palette_window::CONTRACT,
    popover::CONTRACT,
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
    terminal_frame::CONTRACT,
    tip_card::CONTRACT,
    title_bar::CONTRACT,
    toast::CONTRACT,
    tree_drop_marker::CONTRACT,
    window::CONTRACT,
    window_button::CONTRACT,
    window_close::CONTRACT,
];
