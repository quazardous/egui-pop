//! The contract of the default kit's components (`docs/kit.md`), one
//! module each. One line per component in [`GROUPS`], in alphabetical
//! order.

use super::Required;

mod tab;
mod tab_bar;
mod title_bar;
mod window;
mod window_button;
mod window_close;

/// Each component's contract.
pub(super) const GROUPS: &[&[Required]] = &[
    tab::CONTRACT,
    tab_bar::CONTRACT,
    title_bar::CONTRACT,
    window::CONTRACT,
    window_button::CONTRACT,
    window_close::CONTRACT,
];
