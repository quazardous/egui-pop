//! A plugin contributing one gallery page: a story over a state, with a
//! playground.

use std::cell::RefCell;
use std::rc::Rc;

use egui::Ui;
use egui_pop::api::stories::Story;
use egui_pop::api::{Ctx, Plugin, PluginError};
use egui_pop::model::ids::{PluginId, StoryId};

use crate::shell::SHELL;

/// A page as a plugin.
pub struct PagePlugin<S> {
    pub plugin: PluginId,
    pub story: StoryId,
    pub title: &'static str,
    pub description: &'static str,
    pub group: &'static str,
    pub weight: i32,
    pub show: fn(&mut Ui, &mut S),
    pub playground: Option<fn(&mut Ui, &mut S)>,
}

impl<S> std::fmt::Debug for PagePlugin<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PagePlugin")
            .field("plugin", &self.plugin)
            .finish()
    }
}

impl<S: Default + 'static> Plugin for PagePlugin<S> {
    fn id(&self) -> PluginId {
        self.plugin.clone()
    }

    fn title(&self) -> String {
        self.title.into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let state = Rc::new(RefCell::new(S::default()));
        let show = self.show;
        let s = state.clone();
        let mut story = Story::new(self.story.clone(), self.title, move |ui, _| {
            show(ui, &mut s.borrow_mut())
        })
        .description(self.description)
        .group(self.group, self.weight);
        if let Some(play) = self.playground {
            story = story.playground(move |ui, _| play(ui, &mut state.borrow_mut()));
        }
        ctx.story(story)?;
        Ok(())
    }
}
