//! The `appearance.scroll_bars` setting: `auto` leaves the bars to the theme,
//! `always` and `hover` decide over it. The system's "always show" raises
//! `auto` to `always` (`AppBuilder::system_always_shows_scroll_bars`), and
//! nothing a system says lowers it. Written into the theme's spec
//! ([`ThemeSpec::scroll_bars`]), so that it reaches egui's style with the
//! theme, in every scroll area, and stays when the theme changes.

use egui_pop_core::lock;
use egui_pop_core::theme::{ScrollBars, ThemeSpec};
use egui_pop_model::ids::SettingId;
use egui_pop_model::values::Descriptor;

use crate::app::AppHandle;
use crate::ctx::{Ctx, CtxError};

/// The setting: `auto`, `always`, `hover`.
pub const SETTING_SCROLL_BARS: SettingId = SettingId::from_static("appearance.scroll_bars");

/// What the setting says now.
pub fn scroll_bars(app: &AppHandle) -> ScrollBars {
    lock(&app.store())
        .get(&format!("settings/{SETTING_SCROLL_BARS}"))
        .and_then(|e| e.value.as_choice().and_then(ScrollBars::parse))
        .unwrap_or_default()
}

/// What the setting asks of the theme: the system's "always show" raises
/// `auto`, and never touches a choice the user made.
fn asked(setting: ScrollBars, system_always: bool) -> ScrollBars {
    match setting {
        ScrollBars::Auto if system_always => ScrollBars::Always,
        chosen => chosen,
    }
}

/// Writes what the setting asks into the spec. A choice stays there, even
/// through an application's new spec; `auto` leaves the application's spec
/// alone unless it comes back from another choice.
pub(crate) fn apply(app: &AppHandle, seen: &mut Option<ScrollBars>, spec: &mut ThemeSpec) -> bool {
    let wanted = asked(scroll_bars(app), app.system_always_shows_scroll_bars());
    let moved = seen.is_some_and(|before| before != wanted);
    *seen = Some(wanted);
    if (wanted != ScrollBars::Auto || moved) && spec.scroll_bars != wanted {
        spec.scroll_bars = wanted;
        return true;
    }
    false
}

pub(crate) fn register(ctx: &mut Ctx<'_>) -> Result<(), CtxError> {
    ctx.setting(
        SETTING_SCROLL_BARS,
        Descriptor::choice(
            "Scroll bars",
            &ScrollBars::CHOICES,
            ScrollBars::Auto.as_str(),
        )
        .description(
            "When content overflows: bars as the theme draws them (auto), full bars beside the content (always), or none until the pointer is over it (hover). Auto gives full bars too when the application reports that the system always shows them.",
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_raises_auto_and_changes_no_choice() {
        assert_eq!(asked(ScrollBars::Auto, false), ScrollBars::Auto);
        assert_eq!(asked(ScrollBars::Auto, true), ScrollBars::Always);
        assert_eq!(asked(ScrollBars::Hover, true), ScrollBars::Hover);
        assert_eq!(asked(ScrollBars::Always, false), ScrollBars::Always);
    }
}
