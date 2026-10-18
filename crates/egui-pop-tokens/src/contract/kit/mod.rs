//! The contract of the default kit's components (`docs/kit.md`), one
//! module each. One line per component in [`GROUPS`], in alphabetical
//! order.

use super::Required;

mod activity_bar_item;
mod area_grip;
mod area_strip;
mod assist_card;
mod backdrop;
mod band_icon_button;
mod button;
mod command_button;
mod command_palette_row;
mod dialog;
mod dialog_header;
mod dock_preview;
mod editor_frame;
mod empty_state;
mod fault_card;
mod field;
mod find_bar;
mod find_bar_button;
mod fullscreen_exit;
mod glyph;
mod header_band;
mod icon_button;
mod job_log;
mod list_row;
mod list_view;
mod menu_item;
mod meter;
mod meter_tick;
mod palette_window;
mod popover;
mod progress_bar;
mod progress_caption;
mod progress_fill;
mod progress_line;
mod progress_ring;
mod side_area;
mod skeleton;
mod skeleton_shimmer;
mod slider;
mod slider_fill;
mod slider_thumb;
mod slider_toggle;
mod slider_value;
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
    activity_bar_item::CONTRACT,
    area_grip::CONTRACT,
    area_strip::CONTRACT,
    assist_card::CONTRACT,
    backdrop::CONTRACT,
    band_icon_button::CONTRACT,
    button::CONTRACT,
    command_button::CONTRACT,
    command_palette_row::CONTRACT,
    dialog::CONTRACT,
    dialog_header::CONTRACT,
    dock_preview::CONTRACT,
    editor_frame::CONTRACT,
    empty_state::CONTRACT,
    fault_card::CONTRACT,
    field::CONTRACT,
    find_bar::CONTRACT,
    find_bar_button::CONTRACT,
    fullscreen_exit::CONTRACT,
    glyph::CONTRACT,
    header_band::CONTRACT,
    icon_button::CONTRACT,
    job_log::CONTRACT,
    list_row::CONTRACT,
    list_view::CONTRACT,
    menu_item::CONTRACT,
    meter::CONTRACT,
    meter_tick::CONTRACT,
    palette_window::CONTRACT,
    popover::CONTRACT,
    progress_bar::CONTRACT,
    progress_caption::CONTRACT,
    progress_fill::CONTRACT,
    progress_line::CONTRACT,
    progress_ring::CONTRACT,
    side_area::CONTRACT,
    skeleton::CONTRACT,
    skeleton_shimmer::CONTRACT,
    slider::CONTRACT,
    slider_fill::CONTRACT,
    slider_thumb::CONTRACT,
    slider_toggle::CONTRACT,
    slider_value::CONTRACT,
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
