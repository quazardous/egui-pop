//! What a tab drag carries across windows.
//!
//! A tab or a whole tab group (all its tabs, dragged by the empty part of
//! its tab bar, its shown tab kept) is the payload of a drag session
//! (`bus::drag`), the one every drag across windows goes through: the
//! detached windows are its drop targets.

use egui_pop_model::ids::PanelId;

/// What a drag in the main window carries: one tab, or a whole tab
/// group.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Dragged {
    /// One tab.
    Tab(PanelId),
    /// A tab group, all its tabs in order and the one it shows.
    Group {
        panels: Vec<PanelId>,
        shown: PanelId,
    },
}

impl Dragged {
    /// The panel that names the drag: the tab, or the group's shown one.
    pub(crate) fn lead(&self) -> &PanelId {
        match self {
            Dragged::Tab(p) => p,
            Dragged::Group { shown, .. } => shown,
        }
    }

    /// The panels it moves, in order.
    pub(crate) fn panels(&self) -> &[PanelId] {
        match self {
            Dragged::Tab(p) => std::slice::from_ref(p),
            Dragged::Group { panels, .. } => panels,
        }
    }

    /// Whether it moves a whole group.
    pub(crate) fn is_group(&self) -> bool {
        matches!(self, Dragged::Group { .. })
    }

    /// Whether it moves `panel`.
    pub(crate) fn holds(&self, panel: &PanelId) -> bool {
        self.panels().contains(panel)
    }
}
