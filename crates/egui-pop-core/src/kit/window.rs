//! What a window drawn by the application takes from the kit beside its
//! components' boxes: where its title bar puts the window buttons, which
//! of them show the hover state, the corners the system is asked for and
//! the outline drawn around it.
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

/// The corners asked of the system for a window drawn by the application
/// (`window.corners`), where it can round them (Windows 11).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowCorners {
    /// The system's rounded corners. The default.
    #[default]
    Round,
    /// The system's small rounded corners.
    Small,
    /// Square corners.
    Square,
}

impl WindowCorners {
    /// The token's value.
    pub fn name(self) -> &'static str {
        match self {
            WindowCorners::Round => "round",
            WindowCorners::Small => "small",
            WindowCorners::Square => "square",
        }
    }
}

/// The window's terms of a kit.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WindowKit {
    pub buttons_side: ButtonsSide,
    pub buttons_hover: ButtonsHover,
    pub corners: WindowCorners,
    /// The line around a window drawn by the application, inside its
    /// edges (`window.rest.border-width` and `border-color`); none when
    /// the width is 0, the default.
    pub outline: egui::Stroke,
}

/// Reads a string token among `choices`; what is not one of them is a
/// problem and gives the default.
pub(super) fn choice<T: Copy + Default>(
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
            corners: choice(
                t,
                "window.corners",
                &[
                    ("round", WindowCorners::Round),
                    ("small", WindowCorners::Small),
                    ("square", WindowCorners::Square),
                ],
                problems,
            ),
            outline: match t.resolved().dimension("window.rest.border-width") {
                Some(w) if w > 0.0 => egui::Stroke::new(
                    w,
                    t.color("window.rest.border-color")
                        .unwrap_or(egui::Color32::TRANSPARENT),
                ),
                _ => egui::Stroke::NONE,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_pop_tokens::{ThemeSpec, Themes};

    fn kit(json: &str) -> crate::kit::Kit {
        let mut themes = Themes::default();
        themes
            .add_text("window-test", json)
            .expect("test theme reads");
        let t = Tokens::new(themes.resolve(&ThemeSpec::new("window-test")));
        crate::kit::Kit::compile(&t, &crate::kit::Shapes::default())
    }

    #[test]
    fn corners_are_round_by_default_and_read_from_the_theme() {
        let default = kit(r#"
                          $extends: "pop-dark"
                          "#);
        assert_eq!(default.window, WindowKit::default());
        assert_eq!(default.window.corners, WindowCorners::Round);
        let square = kit(r#"
                         $extends: "pop-dark"
                         window:
                           corners: {$type: "string", $value: "square"}
                         "#);
        assert_eq!(square.window.corners, WindowCorners::Square);
        assert!(square.problems.is_empty(), "{:?}", square.problems);
        let bad = kit(r#"
                      $extends: "pop-dark"
                      window:
                        corners: {$type: "string", $value: "oval"}
                      "#);
        assert_eq!(bad.window.corners, WindowCorners::Round);
        assert_eq!(
            bad.problems,
            ["window.corners: `oval` is not round, small or square"]
        );
    }
}
