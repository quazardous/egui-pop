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
mod card;
mod checkbox;
mod combo_arrow;
mod combo_box;
mod command_button;
mod command_palette_row;
mod compare_pane;
mod dialog;
mod dialog_grip;
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
mod indicator_badge;
mod indicator_button;
mod indicator_dot;
mod indicator_flag;
mod indicator_outline;
mod indicator_swatch;
mod job_log;
mod list_row;
mod list_view;
mod mark_toggle;
mod menu_item;
mod menu_item_gutter;
mod meter;
mod meter_tick;
mod palette_window;
mod pill;
mod popover;
mod progress_bar;
mod progress_caption;
mod progress_fill;
mod progress_line;
mod progress_ring;
mod radio;
mod readout;
mod scroll_bar;
mod search_field;
mod section_band;
mod section_count;
mod section_disclosure;
mod selectable_row;
mod side_area;
mod skeleton;
mod skeleton_shimmer;
mod slider;
mod slider_fill;
mod slider_thumb;
mod slider_toggle;
mod slider_value;
mod spinner;
mod splitter;
mod status_indicator;
mod tab;
mod tab_bar;
mod tab_preview;
mod table_header_cell;
mod table_stripe;
mod tag_chip;
mod terminal_frame;
mod thumb_cell;
mod thumb_layer;
mod thumb_overlay_badge;
mod thumbnail;
mod tip_card;
mod title_bar;
mod toast;
mod toggle_button;
mod toggle_chip;
mod toolbar_button;
mod tree_drop_marker;
mod viewport_focus;
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
    card::CONTRACT,
    checkbox::CONTRACT,
    combo_arrow::CONTRACT,
    combo_box::CONTRACT,
    command_button::CONTRACT,
    command_palette_row::CONTRACT,
    compare_pane::CONTRACT,
    dialog::CONTRACT,
    dialog_grip::CONTRACT,
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
    indicator_badge::CONTRACT,
    indicator_button::CONTRACT,
    indicator_dot::CONTRACT,
    indicator_flag::CONTRACT,
    indicator_outline::CONTRACT,
    indicator_swatch::CONTRACT,
    job_log::CONTRACT,
    list_row::CONTRACT,
    list_view::CONTRACT,
    mark_toggle::CONTRACT,
    menu_item::CONTRACT,
    menu_item_gutter::CONTRACT,
    meter::CONTRACT,
    meter_tick::CONTRACT,
    palette_window::CONTRACT,
    pill::CONTRACT,
    popover::CONTRACT,
    progress_bar::CONTRACT,
    progress_caption::CONTRACT,
    progress_fill::CONTRACT,
    progress_line::CONTRACT,
    progress_ring::CONTRACT,
    radio::CONTRACT,
    readout::CONTRACT,
    scroll_bar::CONTRACT,
    search_field::CONTRACT,
    section_band::CONTRACT,
    section_count::CONTRACT,
    section_disclosure::CONTRACT,
    selectable_row::CONTRACT,
    side_area::CONTRACT,
    skeleton::CONTRACT,
    skeleton_shimmer::CONTRACT,
    slider::CONTRACT,
    slider_fill::CONTRACT,
    slider_thumb::CONTRACT,
    slider_toggle::CONTRACT,
    slider_value::CONTRACT,
    spinner::CONTRACT,
    splitter::CONTRACT,
    status_indicator::CONTRACT,
    tab::CONTRACT,
    tab_bar::CONTRACT,
    tab_preview::CONTRACT,
    table_header_cell::CONTRACT,
    table_stripe::CONTRACT,
    tag_chip::CONTRACT,
    terminal_frame::CONTRACT,
    thumb_cell::CONTRACT,
    thumb_layer::CONTRACT,
    thumb_overlay_badge::CONTRACT,
    thumbnail::CONTRACT,
    tip_card::CONTRACT,
    title_bar::CONTRACT,
    toast::CONTRACT,
    toggle_button::CONTRACT,
    toggle_chip::CONTRACT,
    toolbar_button::CONTRACT,
    tree_drop_marker::CONTRACT,
    viewport_focus::CONTRACT,
    window::CONTRACT,
    window_button::CONTRACT,
    window_close::CONTRACT,
];
