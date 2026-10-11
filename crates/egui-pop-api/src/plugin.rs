//! Plugins (ADR 0004): an id, requirements, a priority, `enabled`,
//! `connect` and `disconnect`. Everything a plugin registers through
//! [`Ctx`] is its own, removed when it is disconnected.
//!
//! Not to be confused with `egui::Plugin`, egui's context hooks: an
//! egui-pop plugin is an application feature.

use std::fmt;

use egui_pop_model::ids::PluginId;
use egui_pop_model::order::OrderError;

use crate::ctx::{Ctx, CtxError};

/// An application feature.
pub trait Plugin: 'static {
    fn id(&self) -> PluginId;

    /// A human title (settings sections, the debug bridge); the id by
    /// default.
    fn title(&self) -> String {
        self.id().to_string()
    }

    /// The plugins it needs connected first.
    fn requires(&self) -> Vec<PluginId> {
        Vec::new()
    }

    /// Among plugins ready at the same time, higher connects first.
    fn priority(&self) -> i32 {
        0
    }

    /// Whether to connect it (a setting may switch it off).
    fn enabled(&self, _ctx: &Ctx<'_>) -> bool {
        true
    }

    /// Registers what it brings.
    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError>;

    /// Releases what it holds outside the framework; its registrations
    /// are removed by the framework.
    fn disconnect(&mut self, _ctx: &mut Ctx<'_>) {}
}

/// Why a plugin did not connect.
#[derive(Debug)]
pub enum PluginError {
    /// A registration was refused.
    Ctx(CtxError),
    /// The plugin's own reason.
    Failed(String),
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PluginError::Ctx(e) => write!(f, "{e}"),
            PluginError::Failed(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PluginError {}

impl From<CtxError> for PluginError {
    fn from(e: CtxError) -> Self {
        PluginError::Ctx(e)
    }
}

/// Why an application could not be built.
#[derive(Debug)]
pub enum BuildError {
    /// Missing requirement, cycle, duplicate id.
    Order(OrderError),
    /// A plugin failed to connect.
    Connect {
        plugin: PluginId,
        error: PluginError,
    },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::Order(e) => write!(f, "{e}"),
            BuildError::Connect { plugin, error } => {
                write!(f, "plugin `{plugin}` failed to connect: {error}")
            }
        }
    }
}

impl std::error::Error for BuildError {}

impl From<OrderError> for BuildError {
    fn from(e: OrderError) -> Self {
        BuildError::Order(e)
    }
}

/// A plugin as the application lists it.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct PluginInfo {
    pub id: PluginId,
    pub title: String,
    pub requires: Vec<PluginId>,
    pub priority: i32,
    pub connected: bool,
    /// Why it is not connected, if it is not.
    pub reason: Option<String>,
}
