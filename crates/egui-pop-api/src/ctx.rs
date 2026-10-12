//! `Ctx`: what a plugin registers through, at connect time. One fluent
//! facade: `command`, `keybinding`, `menu_item`, `panel`, `zone`,
//! `block`, `setting`, `service`, `on`, `hook`, `story`. Everything is
//! owned by the plugin.

use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use egui::Ui;
use egui_pop_model::commands::{CommandSpec, RegisterError};
use egui_pop_model::ids::{CommandId, MenuId, OwnerId, PanelId, PluginId, SettingId, ZoneId};
use egui_pop_model::keymap::{Action, BindOptions, Layer};
use egui_pop_model::menus::MenuItem;
use egui_pop_model::panels::PanelSpec;
use egui_pop_model::values::{DeclareError, Descriptor};
use serde_json::Value as Json;

use crate::app::{AppHandle, Call, CommandFn, Registries};
use crate::hooks::{Hook, LifecycleFn, On};
use crate::services::ServiceToken;
use crate::stories::Story;
use crate::zones::Block;

/// A registration refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtxError {
    Command(RegisterError),
    /// A key binding that does not parse.
    Keys {
        keys: String,
        error: String,
    },
    Menu(String),
    Panel(String),
    /// The zone is declared already.
    Zone {
        zone: ZoneId,
        by: OwnerId,
    },
    Setting(DeclareError),
    Service {
        id: String,
        by: OwnerId,
    },
    Story {
        id: String,
        by: OwnerId,
    },
    Hook {
        id: String,
        by: OwnerId,
    },
    /// The step is declared already.
    Step {
        id: String,
        by: OwnerId,
    },
    /// A debug bridge collector of that name exists.
    Collector(String),
}

impl fmt::Display for CtxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CtxError::Command(e) => write!(f, "{e}"),
            CtxError::Keys { keys, error } => write!(f, "key binding `{keys}`: {error}"),
            CtxError::Menu(e) | CtxError::Panel(e) => write!(f, "{e}"),
            CtxError::Zone { zone, by } => write!(f, "zone `{zone}` is already declared by `{by}`"),
            CtxError::Setting(e) => write!(f, "{e}"),
            CtxError::Collector(e) => write!(f, "{e}"),
            CtxError::Service { id, by } => {
                write!(f, "service `{id}` is already provided by `{by}`")
            }
            CtxError::Story { id, by } => {
                write!(f, "story `{id}` is already contributed by `{by}`")
            }
            CtxError::Hook { id, by } => write!(f, "hook `{id}` is already registered by `{by}`"),
            CtxError::Step { id, by } => write!(f, "step `{id}` is already declared by `{by}`"),
        }
    }
}

impl std::error::Error for CtxError {}

/// What a plugin registers through.
pub struct Ctx<'a> {
    pub(crate) owner: OwnerId,
    pub(crate) plugin: PluginId,
    pub(crate) rank: usize,
    pub(crate) reg: &'a mut Registries,
    pub(crate) app: &'a AppHandle,
    pub(crate) connected: &'a [PluginId],
}

impl fmt::Debug for Ctx<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ctx").field("plugin", &self.plugin).finish()
    }
}

impl<'a> Ctx<'a> {
    /// The plugin registering.
    pub fn plugin(&self) -> &PluginId {
        &self.plugin
    }

    /// The owner its registrations are tagged with.
    pub fn owner(&self) -> &OwnerId {
        &self.owner
    }

    /// The application, to keep in closures (blocks, handlers).
    pub fn app(&self) -> AppHandle {
        self.app.clone()
    }

    /// Whether another plugin is connected.
    pub fn is_connected(&self, plugin: &PluginId) -> bool {
        self.connected.contains(plugin)
    }

    /// Declares a command and everything derived from it (its default
    /// key bindings, its menu entries, its palette entry and its agent
    /// surface), with an in-process handler: the default `LocalHost`
    /// runs it on the interface thread.
    pub fn command(
        &mut self,
        spec: CommandSpec,
        handler: impl Fn(&Call<'_>) -> Result<Json, String> + 'static,
    ) -> Result<&mut Self, CtxError> {
        let handler: CommandFn = Rc::new(handler);
        self.add_command(spec, Some(handler))
    }

    /// Declares a command the application's host runs (`AppBuilder::host`):
    /// keys, menus, palette and traces as for [`Ctx::command`], no
    /// handler here.
    pub fn declare(&mut self, spec: CommandSpec) -> Result<&mut Self, CtxError> {
        self.add_command(spec, None)
    }

    fn add_command(
        &mut self,
        spec: CommandSpec,
        handler: Option<CommandFn>,
    ) -> Result<&mut Self, CtxError> {
        for k in &spec.keys {
            egui_pop_model::keymap::Keys::parse(&k.keys).map_err(|error| CtxError::Keys {
                keys: k.keys.clone(),
                error,
            })?;
        }
        let keys = spec.keys.clone();
        let menus = spec.menus.clone();
        let id = spec.id.clone();
        let when = spec.when.clone();
        let checked = spec.checked.clone();
        self.reg
            .commands
            .register(&self.owner, spec, handler)
            .map_err(CtxError::Command)?;
        for k in keys {
            self.reg
                .keymap
                .bind_with(
                    &self.owner,
                    Layer::Default,
                    &k.keys,
                    Action::Run(id.clone()),
                    &when,
                    k.options,
                )
                .map_err(|error| CtxError::Keys {
                    keys: k.keys,
                    error,
                })?;
        }
        for m in menus {
            self.reg
                .menus
                .contribute(
                    &self.owner,
                    MenuItem::command(m.menu, &m.group, m.order, id.clone())
                        .when(&when, true)
                        .checked(&checked),
                )
                .map_err(CtxError::Menu)?;
        }
        Ok(self)
    }

    /// Binds keys to a command (`layer` defaults: use `Layer::Default`
    /// for a plugin's own, `Layer::Suggestion` for a soft one).
    pub fn keybinding(
        &mut self,
        keys: &str,
        command: CommandId,
        when: &str,
        layer: Layer,
    ) -> Result<&mut Self, CtxError> {
        self.reg
            .keymap
            .bind(&self.owner, layer, keys, Action::Run(command), when)
            .map_err(|error| CtxError::Keys {
                keys: keys.into(),
                error,
            })?;
        Ok(self)
    }

    /// Binds keys to a command, listening as `options` say (on a
    /// surface, while typing, through modals).
    pub fn keybinding_with(
        &mut self,
        keys: &str,
        command: CommandId,
        when: &str,
        layer: Layer,
        options: BindOptions,
    ) -> Result<&mut Self, CtxError> {
        self.reg
            .keymap
            .bind_with(
                &self.owner,
                layer,
                keys,
                Action::Run(command),
                when,
                options,
            )
            .map_err(|error| CtxError::Keys {
                keys: keys.into(),
                error,
            })?;
        Ok(self)
    }

    /// Unbinds keys (in the layers below `layer`).
    pub fn unbind(&mut self, keys: &str, layer: Layer) -> Result<&mut Self, CtxError> {
        self.reg
            .keymap
            .bind(&self.owner, layer, keys, Action::Unbind, "")
            .map_err(|error| CtxError::Keys {
                keys: keys.into(),
                error,
            })?;
        Ok(self)
    }

    /// Contributes a menu item.
    pub fn menu_item(&mut self, item: MenuItem) -> Result<&mut Self, CtxError> {
        self.reg
            .menus
            .contribute(&self.owner, item)
            .map_err(CtxError::Menu)?;
        Ok(self)
    }

    /// Declares a submenu under a menu (`app.menu.view`).
    pub fn submenu(
        &mut self,
        parent: MenuId,
        group: &str,
        order: i32,
        menu: MenuId,
        label: &str,
    ) -> Result<&mut Self, CtxError> {
        self.menu_item(MenuItem::submenu(parent, group, order, menu, label))
    }

    /// Declares a panel and how to draw it; the workbench places it in
    /// its area (see `workbench`), inside a fault boundary and a focus
    /// scope, and its commands `<id>.focus` and `<id>.toggle` are
    /// declared with it.
    pub fn panel(
        &mut self,
        spec: PanelSpec,
        draw: impl FnMut(&mut Ui, &AppHandle) + 'static,
    ) -> Result<&mut Self, CtxError> {
        let id: PanelId = spec.id.clone();
        let title = spec.title.clone();
        self.reg
            .panels
            .declare(&self.owner, spec)
            .map_err(CtxError::Panel)?;
        self.app
            .add_panel_view(&self.owner, id.clone(), Box::new(draw));
        let command = |suffix: &str| {
            CommandId::new(format!("{id}.{suffix}")).map_err(|e| CtxError::Panel(e.to_string()))
        };
        let (focus, toggle) = (id.clone(), id.clone());
        self.command(
            CommandSpec::new(command("focus")?, &format!("Focus {title}"), "View")
                .description(&format!("Shows {title} and gives it the keyboard.")),
            move |call| {
                call.app.focus_panel(&focus);
                Ok(Json::Null)
            },
        )?
        .command(
            CommandSpec::new(command("toggle")?, &format!("Show or hide {title}"), "View")
                .description(&format!("Opens {title} where it was, or closes it.")),
            move |call| {
                let shown = call.app.panel_visible(&toggle);
                call.app.set_panel_visible(&toggle, !shown);
                Ok(serde_json::json!({ "shown": !shown }))
            },
        )?;
        Ok(self)
    }

    /// Shows a zone as a panel (the shell's side bar, main and inspector
    /// zones): no commands, drawn bare when alone in its area.
    pub(crate) fn zone_panel(
        &mut self,
        spec: PanelSpec,
        zone: ZoneId,
        scope: &'static str,
    ) -> Result<&mut Self, CtxError> {
        let id = spec.id.clone();
        self.reg
            .panels
            .declare(&self.owner, spec.bare())
            .map_err(CtxError::Panel)?;
        self.app
            .workbench_mut()
            .zone_of
            .insert(id.clone(), zone.clone());
        self.app.add_panel_view(
            &self.owner,
            id,
            Box::new(move |ui, app| {
                egui_pop_core::focus::scope(ui, scope, &[], |ui| app.show_zone(ui, &zone));
            }),
        );
        Ok(self)
    }

    /// Declares a zone other plugins may contribute blocks to.
    pub fn zone(&mut self, zone: ZoneId, title: &str) -> Result<&mut Self, CtxError> {
        self.app
            .zones_mut()
            .declare(&self.owner, zone.clone(), title)
            .map_err(|by| CtxError::Zone { zone, by })?;
        Ok(self)
    }

    /// Contributes a block to a zone: add, replace, extend or wrap. What
    /// cannot apply (unknown zone, missing target, duplicate id) is
    /// reported when the application resolves zones.
    pub fn block(&mut self, zone: ZoneId, block: Block) -> &mut Self {
        self.app
            .zones_mut()
            .contribute(&self.owner, self.rank, zone, block);
        self
    }

    /// Declares a setting at `settings/<id>`, applied live, with its
    /// reset; shown in the settings form under the plugin's title.
    pub fn setting(
        &mut self,
        id: SettingId,
        descriptor: Descriptor,
    ) -> Result<&mut Self, CtxError> {
        let path = format!("settings/{id}");
        egui_pop_core::lock(&self.app.store())
            .declare(&self.owner, &path, descriptor)
            .map_err(CtxError::Setting)?;
        self.reg
            .settings
            .insert(&self.owner, id, path)
            .map_err(|t| {
                CtxError::Setting(DeclareError::Taken {
                    path: format!("settings/{}", t.id),
                    by: t.by,
                })
            })?;
        Ok(self)
    }

    /// Adds a collector to the debug bridge (removed with the plugin).
    pub fn collector(
        &mut self,
        collector: egui_pop_core::debug::FnCollector,
    ) -> Result<&mut Self, CtxError> {
        self.app.add_collector(&self.owner, collector)?;
        Ok(self)
    }

    /// Provides a service.
    pub fn service<T: Send + Sync + 'static>(
        &mut self,
        token: &ServiceToken<T>,
        value: T,
    ) -> Result<&mut Self, CtxError> {
        self.app
            .services_mut()
            .provide(&self.owner, token, Arc::new(value))
            .map_err(|by| CtxError::Service {
                id: token.id().to_string(),
                by,
            })?;
        Ok(self)
    }

    /// Calls `f` at a moment of the application's life.
    pub fn on(
        &mut self,
        when: On,
        f: impl FnMut(&AppHandle, &crate::hooks::Lifecycle) + 'static,
    ) -> &mut Self {
        let f: LifecycleFn = Box::new(f);
        self.reg.lifecycle.push(&self.owner, (when, f));
        self
    }

    /// Calls `f` after each change of a store path under `prefix`.
    pub fn on_change(
        &mut self,
        prefix: &str,
        f: impl FnMut(&egui_pop_model::values::Changed) + Send + 'static,
    ) -> &mut Self {
        egui_pop_core::lock(&self.app.store()).listen(&self.owner, prefix, f);
        self
    }

    /// Registers an alter hook.
    pub fn hook(&mut self, hook: Hook) -> Result<&mut Self, CtxError> {
        if let Some(s) = self.reg.hooks.iter().find(|s| s.item.id == hook.id) {
            return Err(CtxError::Hook {
                id: hook.id.to_string(),
                by: s.owner.clone(),
            });
        }
        self.reg.hooks.push(&self.owner, hook);
        Ok(self)
    }

    /// Declares a step of this plugin's features that other plugins may
    /// watch, veto, replace or extend (ADR 0011).
    pub fn step(&mut self, spec: crate::steps::StepSpec) -> Result<&mut Self, CtxError> {
        self.app.declare_step(&self.owner, spec)?;
        Ok(self)
    }

    /// Runs before a step: lets it go, vetoes it or changes its
    /// arguments.
    pub fn before(
        &mut self,
        step: egui_pop_model::ids::StepId,
        f: impl Fn(&AppHandle, &Json) -> crate::steps::Before + 'static,
    ) -> &mut Self {
        self.app
            .add_step_hook(&self.owner, step, crate::steps::HookFn::Before(Rc::new(f)));
        self
    }

    /// Does a step instead of its default (the last plugin to replace a
    /// step wins).
    pub fn replace(
        &mut self,
        step: egui_pop_model::ids::StepId,
        f: impl Fn(&AppHandle, &Json) -> Result<Json, String> + 'static,
    ) -> &mut Self {
        self.app
            .add_step_hook(&self.owner, step, crate::steps::HookFn::Run(Rc::new(f)));
        self
    }

    /// Runs after a step, with its arguments and result.
    pub fn after(
        &mut self,
        step: egui_pop_model::ids::StepId,
        f: impl Fn(&AppHandle, &Json, &Result<Json, String>) + 'static,
    ) -> &mut Self {
        self.app
            .add_step_hook(&self.owner, step, crate::steps::HookFn::After(Rc::new(f)));
        self
    }

    /// Adds an indicator to the status bar, fed by the journal's events
    /// (shown by `strips::StripsPlugin`).
    pub fn indicator(&mut self, indicator: crate::strips::Indicator) -> &mut Self {
        self.app.add_indicator(&self.owner, indicator);
        self
    }

    /// Contributes a gallery story.
    pub fn story(&mut self, story: Story) -> Result<&mut Self, CtxError> {
        self.app.add_story(&self.owner, story)?;
        Ok(self)
    }
}
