//! Translations for egui-pop applications, over Fluent.
//!
//! - [`I18nPlugin`]: the plugin. It loads catalogues (egui-pop's own,
//!   the application's, an override directory), declares the
//!   `appearance.language` setting, publishes `locale.changed`, declares
//!   the `i18n.resolve` step, adds the `i18n` debug collector, and
//!   installs the interface's strings provider so egui-pop's own text and
//!   what plugins declare are translated (see [`plugin`]).
//! - [`I18n`]: the catalogues and the locale, shared; [`t`], [`text`]
//!   and [`tr!`] look text up. A lookup tries the locale, its parents
//!   (`fr-CA`, then `fr`), then English; a key missing everywhere shows
//!   the English written in the code, else the key.
//! - [`format`](mod@format): numbers, dates and plurals for the locale,
//!   in Rust and in catalogues (`NUMBER`, `DATETIME`).
//! - [`locale`]: tags, the locale to use, fallback chains.
//! - [`extract`] and [`check`]: for tools, the keys the code uses, the
//!   English catalogue written from them, and per locale the keys
//!   missing, unused or changed since translated.
//!
//! ```
//! use egui_pop_api::App;
//! use egui_pop_i18n::{Catalogue, I18nPlugin, args, plugin, t};
//!
//! static CATALOGUES: &[Catalogue] = &[
//!     Catalogue::new("en", "app.ftl", "hello = Hello, { $name }!\n"),
//!     Catalogue::new("fr", "app.ftl", "hello = Bonjour, { $name } !\n"),
//! ];
//!
//! let ctx = egui::Context::default();
//! let mut app = App::builder()
//!     .plugin(I18nPlugin::new().catalogues(CATALOGUES))
//!     .build(&ctx)
//!     .unwrap();
//! plugin::set_language(app.handle(), "fr").unwrap();
//! let mut said = String::new();
//! let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
//!     app.show(ui);
//!     said = t(ui.ctx(), "hello", &args![name = "Ada"]);
//! });
//! out.textures_delta.clear();
//! assert_eq!(said, "Bonjour, Ada !");
//! ```

pub mod catalog;
pub mod check;
pub mod extract;
pub mod format;
mod i18n;
pub mod locale;
pub mod plugin;

pub use fluent_bundle::{FluentArgs, FluentValue};
pub use i18n::{Args, Catalogue, EGUI_POP, I18n, Provider};
pub use plugin::{I18nPlugin, ShowKeys, global, t, text};

/// The text of a key from the catalogues of the connected
/// [`I18nPlugin`] ([`global`]), with named arguments; the key itself
/// when no catalogue has it. The extractor finds its literal keys.
///
/// ```
/// use egui_pop_i18n::tr;
///
/// assert_eq!(tr!("some-key"), "some-key");
/// assert_eq!(tr!("files", count = 3), "files");
/// ```
#[macro_export]
macro_rules! tr {
    ($key:literal $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::global().t($key, &$crate::args![$($name = $value),*])
    };
}
