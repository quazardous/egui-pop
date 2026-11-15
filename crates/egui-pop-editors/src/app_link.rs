//! The editors' link to the application, whether there is one.
//!
//! With the `api` feature (on by default) these are egui-pop-api's
//! `AppHandle` and step `Outcome`. Without it an editor is a plain egui
//! widget: [`AppHandle`] is a type with no value, so every
//! `Option<&AppHandle>` an editor takes is `None` and its moments run their
//! defaults directly, the same code path as an editor given no handle.

#[cfg(feature = "api")]
pub use egui_pop_api::AppHandle;
#[cfg(feature = "api")]
pub(crate) use egui_pop_api::steps::Outcome;

#[cfg(not(feature = "api"))]
pub use stand_in::AppHandle;
#[cfg(not(feature = "api"))]
pub(crate) use stand_in::Outcome;

#[cfg(not(feature = "api"))]
mod stand_in {
    use serde_json::Value as Json;

    /// Without the `api` feature there is no application: no value of
    /// this type exists, and the methods taking `Option<&AppHandle>` are
    /// given `None`.
    #[derive(Debug)]
    pub enum AppHandle {}

    impl AppHandle {
        pub(crate) fn egui(&self) -> egui::Context {
            match *self {}
        }

        pub(crate) fn publish(&self, _: &str, _: Option<u64>, _: &str, _: Json) {
            match *self {}
        }
    }

    /// A step's outcome: without an application, its default ran.
    #[allow(dead_code)]
    pub(crate) enum Outcome {
        Done(Result<Json, String>),
        Vetoed { by: String, reason: String },
    }
}
