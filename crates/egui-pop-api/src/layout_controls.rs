//! The workbench's layout, as VS Code arranges it: where the primary
//! side bar lies (`workbench.side_bar_position`, on the left by
//! default, the secondary one across from it).

use egui_pop_core::lock;
use egui_pop_model::ids::SettingId;

use crate::app::AppHandle;

/// The setting: where the primary side bar (the start side area) lies,
/// `left` (the default) or `right`; the secondary one lies across from
/// it, and the activity bar beside it.
pub const SETTING_SIDE_BAR_POSITION: SettingId =
    SettingId::from_static("workbench.side_bar_position");

/// Where the primary side bar lies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SideBarPosition {
    /// On the left, the secondary side bar on the right (the default).
    #[default]
    Left,
    /// On the right, the secondary side bar on the left.
    Right,
}

impl SideBarPosition {
    /// The setting's choices, in order.
    pub const CHOICES: [&'static str; 2] = ["left", "right"];

    pub fn as_str(self) -> &'static str {
        match self {
            SideBarPosition::Left => "left",
            SideBarPosition::Right => "right",
        }
    }

    pub fn parse(s: &str) -> Option<SideBarPosition> {
        match s {
            "left" => Some(SideBarPosition::Left),
            "right" => Some(SideBarPosition::Right),
            _ => None,
        }
    }

    /// The other side.
    pub fn other(self) -> SideBarPosition {
        match self {
            SideBarPosition::Left => SideBarPosition::Right,
            SideBarPosition::Right => SideBarPosition::Left,
        }
    }

    /// Whether the side areas are drawn the other way round from their
    /// names (the start one on the right).
    pub(crate) fn flipped(self) -> bool {
        self == SideBarPosition::Right
    }
}

/// The primary side bar's position, as the setting says now.
pub fn side_bar_position(app: &AppHandle) -> SideBarPosition {
    lock(&app.store())
        .get(&format!("settings/{SETTING_SIDE_BAR_POSITION}"))
        .and_then(|e| e.value.as_choice().and_then(SideBarPosition::parse))
        .unwrap_or_default()
}

/// Declares the setting (the framework's own plugin does).
pub(crate) fn declare(ctx: &mut crate::Ctx<'_>) -> Result<(), crate::CtxError> {
    ctx.setting(
        SETTING_SIDE_BAR_POSITION,
        egui_pop_model::values::Descriptor::choice(
            "Primary side bar position",
            &SideBarPosition::CHOICES,
            SideBarPosition::Left.as_str(),
        )
        .description(
            "Where the primary side bar lies: on the left, the secondary one on the right (left), or the other way round (right).",
        ),
    )?;
    Ok(())
}
