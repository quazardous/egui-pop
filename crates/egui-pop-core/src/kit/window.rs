//! What a window drawn by the application takes from the kit beside its
//! components' boxes: where its title bar puts the window buttons and
//! which of them show the hover state.
//! Compiled with the kit, read as plain fields.

use super::{Tokens, string};

/// Where the window buttons go on the title bar (`title-bar.buttons-side`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonsSide {
    /// Before the leading items, close at the start edge, then minimize
    /// and maximize (macOS).
    Start,
    /// After the trailing items, minimize, maximize, then close at the
    /// end edge (Windows, most Linux desktops). The default.
    #[default]
    End,
}

/// Which window buttons are drawn `hover` (`title-bar.buttons-hover`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonsHover {
    /// The one under the pointer. The default.
    #[default]
    Own,
    /// All three while the pointer is over any of them (macOS shows every
    /// glyph then).
    Group,
}

/// The window's terms of a kit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowKit {
    pub buttons_side: ButtonsSide,
    pub buttons_hover: ButtonsHover,
}

/// Reads a string token among `choices`; what is not one of them is a
/// problem and gives the default.
fn choice<T: Copy + Default>(
    t: &Tokens,
    path: &str,
    choices: &[(&str, T)],
    problems: &mut Vec<String>,
) -> T {
    let Some(asked) = string(t, path) else {
        return T::default();
    };
    match choices.iter().find(|(name, _)| *name == asked) {
        Some((_, v)) => *v,
        None => {
            let names: Vec<&str> = choices.iter().map(|(n, _)| *n).collect();
            let (last, first) = names.split_last().unwrap_or((&"", &[]));
            problems.push(format!(
                "{path}: `{asked}` is not {} or {last}",
                first.join(", ")
            ));
            T::default()
        }
    }
}

impl WindowKit {
    pub(super) fn compile(t: &Tokens, problems: &mut Vec<String>) -> WindowKit {
        WindowKit {
            buttons_side: choice(
                t,
                "title-bar.buttons-side",
                &[("start", ButtonsSide::Start), ("end", ButtonsSide::End)],
                problems,
            ),
            buttons_hover: choice(
                t,
                "title-bar.buttons-hover",
                &[("own", ButtonsHover::Own), ("group", ButtonsHover::Group)],
                problems,
            ),
        }
    }
}
