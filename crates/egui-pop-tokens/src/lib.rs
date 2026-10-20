//! Design tokens for egui-pop, in the DTCG format, with no egui dependency.
//!
//! A token file is a tree of groups and tokens in the format of the W3C
//! Design Tokens Community Group (DTCG 2025.10). A token is an object with
//! a `$value` and a `$type` given on the token or inherited from its group;
//! its name is its dotted path (`surface.panel`). A value can be a
//! whole-value alias of another token, `"{gray.2}"`. `$description` and
//! `$extensions` are kept. At the root, `$extends` names the theme a file
//! builds on: every token not given comes from it.
//!
//! Tokens come in three tiers ([`Tier`]): the **reference** palette (colour
//! scales such as `gray.0`..`gray.12`), the **system** roles widgets read
//! (`surface.panel`, `text.muted`) and **component** tokens
//! (`slider-fill.rest.fill`), each tier aliasing the one below. Every theme answers
//! the same [`contract()`]; a theme missing a token, or giving it the wrong
//! type, gets the built-in `pop-dark` value for it and a reported problem.
//! Resolving never panics.
//!
//! A theme is composed from a [`ThemeSpec`]: a base theme (mode and
//! contrast), a [`Density`] overlay (space and size tokens only), an accent
//! (a colour scale whose steps become the accent roles), reduced motion and
//! the user's overrides.
//!
//! ```
//! use egui_pop_tokens::{ThemeSpec, Themes, contrast};
//!
//! let themes = Themes::default();
//! let tokens = themes.resolve(&ThemeSpec::new("pop-dark").accent("blue"));
//! assert!(tokens.problems().is_empty());
//! let text = tokens.color("text.default").unwrap();
//! let panel = tokens.color("surface.panel").unwrap();
//! assert!(contrast::contrast_ratio(text, panel) >= 4.5);
//! ```
//!
//! A theme being edited is a [`ThemeDraft`]: a base theme, the tokens
//! changed over it and their history, saved as a token file that
//! `$extends` the base.
//!
//! A file may carry a format stamp ([`FormatStamp`]): the format and
//! contract versions it was written for. Loading a theme over its base
//! never fails on a token: what is read otherwise than written comes
//! back as [`Warning`]s.
//!
//! A colour scale can be generated from one seed colour
//! ([`palette::ramp`]): twelve steps for a light or a dark theme, the
//! text on its solid step chosen with APCA ([`apca`]).
//!
//! A kit's tokens, fonts and licence come as a folder or embedded in the
//! binary ([`kit`](mod@kit), [`kit!`]).
//!
//! Types read: `color` (DTCG colour objects in `srgb`, `srgb-linear`,
//! `oklch` or `oklab`, any other space through its `hex` fallback, and
//! `#rrggbb[aa]` strings), `dimension` (`px` or `rem`), `number`,
//! `duration` (`ms` or `s`) and `fontFamily`. Other types are kept as raw
//! JSON ([`Value::Other`]).

pub mod apca;
pub mod cache;
mod color;
pub mod contract;
pub mod contrast;
mod draft;
mod error;
mod file;
mod format;
pub mod kit;
pub mod palette;
mod theme;
mod value;
mod yaml;
mod yaml_trap;

pub use color::Color;
pub use contract::{CONTRACT_VERSION, RENAMED, Renamed, Required, Tier, contract};
#[allow(deprecated)]
pub use draft::{DRAFT_EXTENSION, DRAFT_VERSION, ThemeDraft, TokenEdit};
pub use error::Error;
pub use file::{RawToken, TOKENS_COMPILER, TokenFile};
pub use format::{FORMAT_EXTENSION, FORMAT_VERSION, FormatStamp, Warning};
pub use theme::{
    ALPHA_EXTENSION, BASED_ON, BUILT_IN_ACCENTS, BUILT_IN_THEMES, Counterpart, Density,
    FALLBACK_THEME, POP_THEMES, ResolvedTokens, SEED_SCALE, ThemeSpec, Themes, Token,
    VARIANT_OF_EXTENSION, theme_label,
};
pub use value::{PX_PER_REM, Value};
