//! The contract of the default kit's `job-log` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("job-log.shape", "string", "the handler drawing a job log"),
    comp(
        "job-log.padding-x",
        "dimension",
        "between a job log's content and its sides",
    ),
    comp(
        "job-log.padding-y",
        "dimension",
        "between a job log's content and its top and bottom",
    ),
    comp("job-log.rest.fill", "color", "a job log's fill at rest"),
    comp(
        "job-log.rest.border-color",
        "color",
        "a job log's border at rest",
    ),
    comp(
        "job-log.rest.border-width",
        "dimension",
        "a job log's border width (0: none)",
    ),
    comp(
        "job-log.rest.radius",
        "dimension",
        "a job log's corner radius",
    ),
];
