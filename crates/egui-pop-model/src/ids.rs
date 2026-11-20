//! Typed ids (ADR 0002): one newtype per namespace, sharing one syntax,
//! serialized as plain strings.
//!
//! ```
//! use egui_pop_model::ids::{CommandId, ZoneId};
//!
//! const SIDEBAR: ZoneId = ZoneId::from_static("app.sidebar");
//! let palette = CommandId::new("view.palette").unwrap();
//! assert_eq!(palette.as_str(), "view.palette");
//! assert_eq!(serde_json::to_string(&SIDEBAR).unwrap(), "\"app.sidebar\"");
//! assert!(CommandId::new("has space").is_err());
//! ```
//!
//! An invalid literal in `from_static` is a compile error when used in a
//! constant:
//!
//! ```compile_fail
//! use egui_pop_model::ids::ZoneId;
//! const BAD: ZoneId = ZoneId::from_static("no spaces allowed");
//! ```

use std::borrow::Cow;
use std::fmt;

/// The longest id, in bytes.
pub const MAX_ID_LEN: usize = 128;

/// Why a text is not an id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdError {
    /// The namespace (`CommandId`, `ZoneId`...).
    pub kind: &'static str,
    pub text: String,
    pub reason: &'static str,
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` is not a {}: {}", self.text, self.kind, self.reason)
    }
}

impl std::error::Error for IdError {}

/// Checks the shared syntax: ASCII letters, digits and `. _ - / :`, not
/// empty, at most [`MAX_ID_LEN`] bytes. `Err` holds the reason.
pub const fn check(text: &str) -> Result<(), &'static str> {
    let b = text.as_bytes();
    if b.is_empty() {
        return Err("it is empty");
    }
    if b.len() > MAX_ID_LEN {
        return Err("it is longer than 128 bytes");
    }
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let ok = c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-' | b'/' | b':');
        if !ok {
            return Err("only ASCII letters, digits and `. _ - / :` are allowed");
        }
        i += 1;
    }
    Ok(())
}

/// Checks a panel id: the shared syntax ([`check`]), and an instance's
/// `#<n>` after it (`app.gallery#2`, ADR 0002's panel instances), `n` a
/// number from 2 without leading zeros (the panel declared is instance 1).
pub const fn check_panel(text: &str) -> Result<(), &'static str> {
    let b = text.as_bytes();
    let mut hash = b.len();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'#' {
            hash = i;
            break;
        }
        i += 1;
    }
    if hash == b.len() {
        return check(text);
    }
    if b.len() > MAX_ID_LEN {
        return Err("it is longer than 128 bytes");
    }
    if hash == 0 {
        return Err("an instance's `#<n>` follows a panel's id");
    }
    let digits = b.len() - hash - 1;
    if digits == 0 || digits > 9 || b[hash + 1] == b'0' {
        return Err("an instance's `#<n>` is a number from 2, without leading zeros");
    }
    let mut j = hash + 1;
    while j < b.len() {
        if !b[j].is_ascii_digit() {
            return Err("an instance's `#<n>` is a number from 2, without leading zeros");
        }
        j += 1;
    }
    if digits == 1 && b[hash + 1] == b'1' {
        return Err("instance 1 is the panel itself: its id has no `#1`");
    }
    // The kind before `#`: the shared syntax.
    let mut k = 0;
    while k < hash {
        let c = b[k];
        let ok = c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-' | b'/' | b':');
        if !ok {
            return Err("only ASCII letters, digits and `. _ - / :` are allowed");
        }
        k += 1;
    }
    Ok(())
}

macro_rules! typed_id {
    ($(#[$doc:meta])* $name:ident) => {
        typed_id!($(#[$doc])* $name, check);
    };
    ($(#[$doc:meta])* $name:ident, $check:ident) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Cow<'static, str>);

        impl $name {
            /// From a literal, checked at compile time when used in a
            /// constant (and panicking otherwise).
            pub const fn from_static(text: &'static str) -> Self {
                match $check(text) {
                    Ok(()) => $name(Cow::Borrowed(text)),
                    Err(_) => panic!(concat!("invalid ", stringify!($name), " literal")),
                }
            }

            /// From any text, checked.
            pub fn new(text: impl Into<String>) -> Result<Self, IdError> {
                let text = text.into();
                match $check(&text) {
                    Ok(()) => Ok($name(Cow::Owned(text))),
                    Err(reason) => Err(IdError {
                        kind: stringify!($name),
                        text,
                        reason,
                    }),
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// This id with a suffix (`panel.focus` from `panel`),
            /// checked.
            pub fn join(&self, suffix: &str) -> Result<Self, IdError> {
                Self::new(format!("{}.{suffix}", self.0))
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({:?})", stringify!($name), &*self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl std::str::FromStr for $name {
            type Err = IdError;

            fn from_str(s: &str) -> Result<Self, IdError> {
                Self::new(s)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = IdError;

            fn try_from(s: &str) -> Result<Self, IdError> {
                Self::new(s)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdError;

            fn try_from(s: String) -> Result<Self, IdError> {
                Self::new(s)
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.0)
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = String::deserialize(d)?;
                Self::new(text).map_err(serde::de::Error::custom)
            }
        }
    };
}

typed_id!(
    /// A command: `view.palette`, `file.save`.
    CommandId
);
typed_id!(
    /// A menu: `app.main`, `panel.tab`.
    MenuId
);
typed_id!(
    /// A panel: `app.log`; an instance of a panel declared with several
    /// (`PanelSpec::instances`) adds `#<n>`: `app.gallery#2`.
    PanelId,
    check_panel
);

impl PanelId {
    /// The panel declared this id is an instance of: itself for the
    /// panel declared (instance 1), `app.gallery` for `app.gallery#2`.
    ///
    /// ```
    /// use egui_pop_model::ids::PanelId;
    /// let second = PanelId::new("app.gallery#2").unwrap();
    /// assert_eq!(second.kind().as_str(), "app.gallery");
    /// assert_eq!(second.instance_number(), 2);
    /// assert_eq!(second.kind().instance_number(), 1);
    /// assert_eq!(second.kind().instance(2).unwrap(), second);
    /// assert!(PanelId::new("app.gallery#1").is_err());
    /// assert!(PanelId::new("app.gallery#02").is_err());
    /// ```
    pub fn kind(&self) -> PanelId {
        match self.0.split_once('#') {
            Some((kind, _)) => PanelId(Cow::Owned(kind.to_string())),
            None => self.clone(),
        }
    }

    /// Its instance's number: 1 for the panel declared.
    pub fn instance_number(&self) -> u32 {
        self.0
            .split_once('#')
            .and_then(|(_, n)| n.parse().ok())
            .unwrap_or(1)
    }

    /// Whether it names an instance other than the panel declared
    /// (`#<n>`).
    pub fn is_instance(&self) -> bool {
        self.0.contains('#')
    }

    /// The id of this panel's instance `n` (its kind's): the kind itself
    /// for 1.
    pub fn instance(&self, n: u32) -> Result<PanelId, IdError> {
        let kind = self.kind();
        if n <= 1 {
            return Ok(kind);
        }
        PanelId::new(format!("{kind}#{n}"))
    }
}
typed_id!(
    /// A zone where blocks are drawn: `app.sidebar`.
    ZoneId
);
typed_id!(
    /// A block in a zone.
    BlockId
);
typed_id!(
    /// A plugin: `gallery.overview`.
    PluginId
);
typed_id!(
    /// Who registered something: a plugin's id, `framework`, `user`.
    OwnerId
);
typed_id!(
    /// A setting, also its path in the value store under `settings/`.
    SettingId
);

/// Where settings live in the value store: a setting `id` is at
/// `settings/<id>` ([`SettingId::path`]).
pub const SETTINGS_PREFIX: &str = "settings/";

impl SettingId {
    /// The setting's path in the value store: `settings/<id>`.
    ///
    /// ```
    /// use egui_pop_model::ids::SettingId;
    /// let id = SettingId::from_static("swatch.show_code");
    /// assert_eq!(id.path(), "settings/swatch.show_code");
    /// ```
    pub fn path(&self) -> String {
        format!("{SETTINGS_PREFIX}{}", self.0)
    }
}
typed_id!(
    /// A service plugins share.
    ServiceId
);
typed_id!(
    /// A hook point declarations are altered at.
    HookId
);
typed_id!(
    /// A gallery story.
    StoryId
);
typed_id!(
    /// A step of a feature's life that plugins may watch, veto, replace
    /// or extend: `frame.close`, `workbench.detach`.
    StepId
);

typed_id!(
    /// A frame of the context stack (ADR 0012): a place (`demo.album`)
    /// or a mode (`demo.orientation.culling`).
    FrameId
);
typed_id!(
    /// A group of modes, one active at a time (`orientation`): the
    /// context key `mode.<group>`, so letters, digits, `_` and `.`.
    GroupId
);
typed_id!(
    /// A history (ADR 0014): one undo line, a document's edit, a
    /// collection's marks, the settings window (`app`, `settings`).
    ScopeId
);

impl OwnerId {
    /// The framework itself.
    pub const FRAMEWORK: OwnerId = OwnerId::from_static("framework");
    /// The user (their keymap, their settings).
    pub const USER: OwnerId = OwnerId::from_static("user");
}

impl From<&PluginId> for OwnerId {
    fn from(p: &PluginId) -> OwnerId {
        OwnerId(p.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_check_their_syntax_and_round_trip_as_strings() {
        for good in ["a", "view.palette", "app/sidebar", "x:y-z_1"] {
            assert!(CommandId::new(good).is_ok(), "{good}");
        }
        for bad in ["", "a b", "é", &"x".repeat(129)] {
            let e = CommandId::new(bad).unwrap_err();
            assert_eq!(e.kind, "CommandId");
            assert!(e.to_string().contains("is not a CommandId"), "{e}");
        }
        const Z: ZoneId = ZoneId::from_static("app.main");
        let json = serde_json::to_string(&Z).unwrap();
        assert_eq!(json, "\"app.main\"");
        assert_eq!(serde_json::from_str::<ZoneId>(&json).unwrap(), Z);
        assert!(serde_json::from_str::<ZoneId>("\"a b\"").is_err());
        assert_eq!(format!("{Z:?}"), "ZoneId(\"app.main\")");
        assert_eq!(Z.join("focus").unwrap().as_str(), "app.main.focus");
        assert_eq!(
            OwnerId::from(&PluginId::from_static("p")),
            OwnerId::new("p").unwrap()
        );
    }
}
