//! Gallery stories: a plugin contributes a component shown in its states,
//! with an optional playground of its options. A gallery application
//! lists and draws them; tests reuse them as fixtures.

use std::fmt;

use egui::Ui;
use egui_pop_model::ids::StoryId;
use serde::Serialize;

use crate::app::AppHandle;

/// Draws a story or its playground.
pub type StoryFn = Box<dyn FnMut(&mut Ui, &AppHandle)>;

/// A story as contributed.
pub struct Story {
    pub id: StoryId,
    pub title: String,
    pub description: String,
    /// Stories of a group are listed together.
    pub group: String,
    /// Lighter first within a group.
    pub weight: i32,
    pub show: StoryFn,
    pub playground: Option<StoryFn>,
}

impl fmt::Debug for Story {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Story")
            .field("id", &self.id)
            .field("title", &self.title)
            .finish()
    }
}

impl Story {
    pub fn new(id: StoryId, title: &str, show: impl FnMut(&mut Ui, &AppHandle) + 'static) -> Story {
        Story {
            id,
            title: title.into(),
            description: String::new(),
            group: String::new(),
            weight: 0,
            show: Box::new(show),
            playground: None,
        }
    }

    pub fn description(mut self, text: &str) -> Story {
        self.description = text.into();
        self
    }

    pub fn group(mut self, group: &str, weight: i32) -> Story {
        self.group = group.into();
        self.weight = weight;
        self
    }

    pub fn playground(mut self, f: impl FnMut(&mut Ui, &AppHandle) + 'static) -> Story {
        self.playground = Some(Box::new(f));
        self
    }
}

/// What the gallery lists.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StoryInfo {
    pub id: StoryId,
    pub title: String,
    pub description: String,
    pub group: String,
    pub weight: i32,
    pub has_playground: bool,
    pub owner: egui_pop_model::ids::OwnerId,
}
