//! The contract of the default kit's `skeleton` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "skeleton.shape",
        "string",
        "the handler drawing a skeleton's shapes",
    ),
    comp(
        "skeleton.rest.fill",
        "color",
        "the grey shapes standing in for loading content",
    ),
    comp(
        "skeleton.rest.radius",
        "dimension",
        "a skeleton shape's corner radius",
    ),
];
