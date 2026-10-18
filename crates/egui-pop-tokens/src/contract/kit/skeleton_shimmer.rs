//! The contract of the default kit's `skeleton-shimmer` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "skeleton-shimmer.shape",
        "string",
        "the handler drawing the shimmer crossing a skeleton",
    ),
    comp(
        "skeleton-shimmer.rest.fill",
        "color",
        "the shimmer crossing a skeleton",
    ),
    comp(
        "skeleton-shimmer.rest.radius",
        "dimension",
        "the shimmer's corner radius",
    ),
];
