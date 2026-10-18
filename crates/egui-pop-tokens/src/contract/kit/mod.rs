//! The contract of the default kit's components (`docs/kit.md`), one
//! module each. One line per component in [`GROUPS`], in alphabetical
//! order.

use super::Required;

mod list_row;
mod list_view;
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
    list_row::CONTRACT,
    list_view::CONTRACT,
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
