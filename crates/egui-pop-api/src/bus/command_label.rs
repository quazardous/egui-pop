//! A command's live text, on the bus (ADR 0029): `pop.command.label` asks
//! the application for a command's title and tooltip each time a menu
//! item, a toolbar button or a shortcut tooltip naming it is drawn
//! ([`AppHandle::command_title`], [`AppHandle::command_tooltip`]).
//!
//! egui-pop has no listener of its own: with nobody answering, a command
//! shows its declared title and description, translated, as before. An
//! application answers for the commands whose text depends on its state
//! ("Zen: Compare" for the panel zen would show), leaving the others'
//! [`CommandText`] as it gets it. What it answers is shown as given: it
//! is already in the interface's language.
//!
//! [`AppHandle::command_title`]: crate::AppHandle::command_title
//! [`AppHandle::command_tooltip`]: crate::AppHandle::command_tooltip

use bus_contract::{Point, PointId, PointKind};
use egui_pop_model::ids::CommandId;

/// A command's text as an application says it now; `None` keeps the
/// declared one.
///
/// ```
/// use egui_pop_api::bus::CommandText;
///
/// let label = CommandText::default().title("Zen: Compare");
/// assert_eq!(label.title.as_deref(), Some("Zen: Compare"));
/// assert_eq!(label.tooltip, None);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct CommandText {
    /// The title menus, toolbars and shortcut tooltips show.
    pub title: Option<String>,
    /// The tooltip, in place of the command's description.
    pub tooltip: Option<String>,
}

impl CommandText {
    /// The title shown.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The tooltip shown.
    #[must_use]
    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
}

/// A command's live title and tooltip (`pop.command.label`): the input is
/// the command, the value what the application says of it, empty while no
/// listener answers.
#[derive(Debug)]
pub struct CommandLabel;
impl Point for CommandLabel {
    const KIND: PointKind = PointKind::Alter;
    const ID: PointId = PointId::new("pop.command.label");
    type Input = CommandId;
    type Value = CommandText;
}

/// The command text points, in the order [`super::order_dump`] prints
/// them.
pub const COMMAND_LABEL_POINTS: [&str; 1] = [CommandLabel::ID.as_str()];
