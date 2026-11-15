//! The views' link to the application, whether there is one.
//!
//! With the `api` feature (on by default) these are egui-pop-api's
//! [`AppHandle`] and step [`Outcome`]. Without it a view is a plain egui
//! widget: the handle is a type with no value, so a view's
//! `Option<&AppHandle>` is always `None` and its moments run their
//! defaults directly, the same code path as a view given no handle.

#[cfg(feature = "api")]
pub(crate) use egui_pop_api::AppHandle;
#[cfg(feature = "api")]
pub(crate) use egui_pop_api::steps::Outcome;

#[cfg(not(feature = "api"))]
pub(crate) use stand_in::{AppHandle, Outcome};

#[cfg(not(feature = "api"))]
mod stand_in {
    use egui_pop_core::binding::SharedStore;
    use egui_pop_model::ids::CommandId;
    use serde_json::Value as Json;

    /// No application: no value of this type exists.
    #[derive(Debug)]
    pub(crate) enum AppHandle {}

    impl AppHandle {
        pub(crate) fn publish(&self, _: &str, _: Option<u64>, _: &str, _: Json) {
            match *self {}
        }

        pub(crate) fn store(&self) -> SharedStore {
            match *self {}
        }

        pub(crate) fn run(&self, _: CommandId, _: Json) {
            match *self {}
        }
    }

    /// A step's outcome: without an application, its default ran.
    #[allow(dead_code)]
    pub(crate) enum Outcome {
        Done(Result<Json, String>),
        Vetoed { by: String, reason: String },
    }

    impl Outcome {
        pub(crate) fn vetoed(&self) -> Option<&str> {
            match self {
                Outcome::Vetoed { reason, .. } => Some(reason),
                Outcome::Done(_) => None,
            }
        }
    }
}
