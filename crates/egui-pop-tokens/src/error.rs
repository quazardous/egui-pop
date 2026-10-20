//! What can be wrong with a token file or a theme.

use std::fmt;

/// A problem in a token file or met while resolving a theme.
///
/// Reading a file returns one as an error. Resolving a theme never fails:
/// it reports its problems in [`crate::ResolvedTokens::problems`] and
/// falls back where it must.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// The file cannot be read from disk.
    Io { file: String, message: String },
    /// The file is not valid JSON.
    Json { file: String, message: String },
    /// The file is not valid YAML, or holds a value YAML misreads (an
    /// unquoted `#colour` or `{alias}`).
    Yaml { file: String, message: String },
    /// The file is valid JSON but not a token file.
    Malformed {
        file: String,
        path: String,
        reason: String,
    },
    /// A theme named by a spec or by `$extends` is not known.
    UnknownTheme { name: String },
    /// Themes extend each other in a circle.
    ExtendsCycle { chain: Vec<String> },
    /// An alias names a token that does not exist.
    UnknownAlias { path: String, target: String },
    /// Aliases lead back to where they started.
    AliasCycle { chain: Vec<String> },
    /// A token has no `$type`, on itself, its groups or what it aliases.
    MissingType { path: String },
    /// A token has another type than expected (by its alias target, or by
    /// the contract).
    TypeMismatch {
        path: String,
        expected: String,
        found: String,
    },
    /// A value that cannot be read as its type.
    InvalidValue {
        path: String,
        ty: String,
        reason: String,
    },
    /// A modifier in a token's `$extensions` that cannot apply (an alpha
    /// outside 0 to 1, or on a token that is not a colour).
    InvalidModifier { path: String, reason: String },
    /// A token of the contract is not given by the theme.
    Missing { path: String },
    /// An override names a token the theme does not have.
    UnknownOverride { path: String },
    /// An accent names no usable colour scale.
    UnknownAccent { name: String },
    /// A component is based on a component no token names
    /// (`<component>.based-on`): it keeps its own tokens only.
    UnknownBase { path: String, base: String },
    /// Components are based on each other in a circle: each keeps its own
    /// tokens only.
    BasedOnCycle { chain: Vec<String> },
}

impl Error {
    /// The token the problem is about, if it is about one.
    pub fn path(&self) -> Option<&str> {
        match self {
            Error::Malformed { path, .. }
            | Error::UnknownAlias { path, .. }
            | Error::MissingType { path }
            | Error::TypeMismatch { path, .. }
            | Error::InvalidValue { path, .. }
            | Error::InvalidModifier { path, .. }
            | Error::Missing { path }
            | Error::UnknownOverride { path }
            | Error::UnknownBase { path, .. } => Some(path),
            Error::AliasCycle { chain } => chain.first().map(String::as_str),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { file, message } => write!(f, "`{file}`: cannot be read: {message}"),
            Error::Json { file, message } => write!(f, "`{file}`: not valid JSON: {message}"),
            Error::Yaml { file, message } => write!(f, "`{file}`: not valid YAML: {message}"),
            Error::Malformed { file, path, reason } => {
                write!(f, "`{file}`: `{path}`: {reason}")
            }
            Error::UnknownTheme { name } => write!(f, "no theme named `{name}`"),
            Error::ExtendsCycle { chain } => {
                write!(
                    f,
                    "themes extend each other in a circle: {}",
                    chain.join(" → ")
                )
            }
            Error::UnknownAlias { path, target } => {
                write!(f, "`{path}` refers to `{{{target}}}`, which is not a token")
            }
            Error::AliasCycle { chain } => write!(f, "alias cycle: {}", chain.join(" → ")),
            Error::MissingType { path } => write!(f, "`{path}` has no $type"),
            Error::TypeMismatch {
                path,
                expected,
                found,
            } => write!(f, "`{path}` is a {found}, expected a {expected}"),
            Error::InvalidValue { path, ty, reason } => {
                write!(f, "`{path}` is not a valid {ty}: {reason}")
            }
            Error::InvalidModifier { path, reason } => write!(f, "`{path}`: {reason}"),
            Error::Missing { path } => write!(f, "`{path}` is missing"),
            Error::UnknownOverride { path } => {
                write!(f, "override `{path}`: the theme has no such token")
            }
            Error::UnknownBase { path, base } => write!(
                f,
                "`{path}`: no token of a component `{base}`; its own tokens only are used"
            ),
            Error::BasedOnCycle { chain } => write!(
                f,
                "components are based on each other in a circle: {}; each keeps its own tokens only",
                chain.join(" → ")
            ),
            Error::UnknownAccent { name } => write!(
                f,
                "accent `{name}`: no colour scale of that name with steps 3, 5, 8, 9, 10 and `contrast`"
            ),
        }
    }
}

impl std::error::Error for Error {}
