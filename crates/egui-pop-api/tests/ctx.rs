//! `Ctx`'s contract (docs/plugins.md): what a plugin registers through
//! at connect time. Its refusals name what was refused and who holds it
//! already, and stop the build with the plugin named; its writer writes
//! as the plugin; a panel brings its focus and toggle commands.

use std::cell::RefCell;
use std::rc::Rc;

use egui_pop_api::hooks::Hook;
use egui_pop_api::services::ServiceToken;
use egui_pop_api::{App, BuildError, Ctx, CtxError, Plugin, PluginError, ValueWriter};
use egui_pop_model::commands::{CommandSpec, Origin, RegisterError};
use egui_pop_model::ids::{CommandId, HookId, MenuId, OwnerId, PanelId, PluginId, ZoneId};
use egui_pop_model::keymap::Layer;
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_model::values::{Change, DeclareError, Descriptor, Refused, Set, Value};
use serde_json::{Value as Json, json};

type Connect = Box<dyn FnMut(&mut Ctx<'_>) -> Result<(), PluginError>>;

/// A plugin doing what its closure says.
struct P {
    id: &'static str,
    requires: Vec<PluginId>,
    connect: Connect,
}

fn p(id: &'static str, f: impl FnMut(&mut Ctx<'_>) -> Result<(), PluginError> + 'static) -> P {
    P {
        id,
        requires: Vec::new(),
        connect: Box::new(f),
    }
}

impl Plugin for P {
    fn id(&self) -> PluginId {
        PluginId::from_static(self.id)
    }

    fn requires(&self) -> Vec<PluginId> {
        self.requires.clone()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        (self.connect)(ctx)
    }
}

/// The build's error: the plugin that failed and its `CtxError`.
fn refused(a: P, b: P) -> (PluginId, CtxError, String) {
    let e = App::builder()
        .plugin(a)
        .plugin(b)
        .build(&egui::Context::default())
        .expect_err("the second registration is refused");
    let text = e.to_string();
    match e {
        BuildError::Connect {
            plugin,
            error: PluginError::Ctx(c),
        } => (plugin, c, text),
        other => panic!("not a registration refused: {other}"),
    }
}

const PING: CommandId = CommandId::from_static("probe.ping");

fn ping(ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
    ctx.command(CommandSpec::new(PING, "Ping", "Probe"), |_| Ok(Json::Null))?;
    Ok(())
}

fn first() -> OwnerId {
    OwnerId::from_static("first")
}

#[test]
fn a_command_declared_twice_is_refused_naming_its_owner() {
    let (plugin, e, text) = refused(p("first", ping), p("second", ping));
    assert_eq!(plugin, PluginId::from_static("second"));
    assert_eq!(
        e,
        CtxError::Command(RegisterError::Duplicate {
            id: PING,
            by: first(),
        })
    );
    assert!(
        text.starts_with("plugin `second` failed to connect: "),
        "{text}"
    );
}

#[test]
fn keys_that_do_not_parse_are_refused_with_the_keys() {
    let e = App::builder()
        .plugin(p("first", |ctx| {
            ping(ctx)?;
            ctx.keybinding("Ctrl+Nope+", PING, "", Layer::Default)?;
            Ok(())
        }))
        .build(&egui::Context::default())
        .expect_err("bad keys");
    let BuildError::Connect {
        error: PluginError::Ctx(CtxError::Keys { keys, error }),
        ..
    } = &e
    else {
        panic!("{e}");
    };
    assert_eq!(keys, "Ctrl+Nope+");
    assert!(!error.is_empty());
    assert!(e.to_string().contains("key binding `Ctrl+Nope+`: "), "{e}");

    // A command's own keys are checked as it is declared: nothing of it
    // is registered.
    let e = App::builder()
        .plugin(p("first", |ctx| {
            ctx.command(
                CommandSpec::new(PING, "Ping", "Probe").key("Ctrl+Nope+"),
                |_| Ok(Json::Null),
            )?;
            Ok(())
        }))
        .build(&egui::Context::default())
        .expect_err("bad keys");
    assert!(
        matches!(
            e,
            BuildError::Connect {
                error: PluginError::Ctx(CtxError::Keys { .. }),
                ..
            }
        ),
        "{e}"
    );
}

#[test]
fn a_zone_a_service_a_hook_and_a_value_are_each_refused_a_second_time() {
    const ZONE: ZoneId = ZoneId::from_static("probe.zone");
    let (_, e, _) = refused(
        p("first", |ctx| {
            ctx.zone(ZONE, "Zone")?;
            Ok(())
        }),
        p("second", |ctx| {
            ctx.zone(ZONE, "Zone again")?;
            Ok(())
        }),
    );
    assert_eq!(
        e,
        CtxError::Zone {
            zone: ZONE,
            by: first()
        }
    );
    assert_eq!(
        e.to_string(),
        "zone `probe.zone` is already declared by `first`"
    );

    const TOKEN: ServiceToken<u32> = ServiceToken::new("probe.count");
    let (_, e, _) = refused(
        p("first", |ctx| {
            ctx.service(&TOKEN, 1)?;
            Ok(())
        }),
        p("second", |ctx| {
            ctx.service(&TOKEN, 2)?;
            Ok(())
        }),
    );
    assert_eq!(
        e,
        CtxError::Service {
            id: "probe.count".into(),
            by: first()
        }
    );
    assert_eq!(
        e.to_string(),
        "service `probe.count` is already provided by `first`"
    );

    let hook = || {
        Hook::menu(
            HookId::from_static("probe.hook"),
            MenuId::from_static("app.menu.view"),
            |_| {},
        )
    };
    let (_, e, _) = refused(
        p("first", move |ctx| {
            ctx.hook(hook())?;
            Ok(())
        }),
        p("second", move |ctx| {
            ctx.hook(hook())?;
            Ok(())
        }),
    );
    assert_eq!(
        e,
        CtxError::Hook {
            id: "probe.hook".into(),
            by: first()
        }
    );
    assert_eq!(
        e.to_string(),
        "hook `probe.hook` is already registered by `first`"
    );

    let hue = || Descriptor::float("Hue", 0.0, 1.0, 0.5);
    let (_, e, _) = refused(
        p("first", move |ctx| {
            ctx.value("probe/hue", hue())?;
            Ok(())
        }),
        p("second", move |ctx| {
            ctx.value("probe/hue", hue())?;
            Ok(())
        }),
    );
    assert!(
        matches!(&e, CtxError::Setting(DeclareError::Taken { path, by }) if path == "probe/hue" && *by == first()),
        "{e:?}"
    );
}

#[test]
fn the_writer_writes_as_its_plugin_and_is_refused_elsewhere() {
    let kept: Rc<RefCell<Option<ValueWriter>>> = Rc::default();
    let keep = kept.clone();
    let app = App::builder()
        .plugin(p("first", |ctx| {
            ctx.value("first/hue", Descriptor::float("Hue", 0.0, 1.0, 0.5))?;
            Ok(())
        }))
        .plugin(p("second", move |ctx| {
            assert_eq!(ctx.plugin(), &PluginId::from_static("second"));
            assert_eq!(ctx.owner(), &OwnerId::from_static("second"));
            ctx.value("second/hue", Descriptor::float("Hue", 0.0, 1.0, 0.5))?;
            *keep.borrow_mut() = Some(ctx.writer());
            Ok(())
        }))
        .build(&egui::Context::default())
        .unwrap();
    let w = kept.borrow_mut().take().unwrap();
    assert_eq!(w.scope(), &OwnerId::from_static("second"));
    let to = || Change::To(Value::Float(0.25));
    // Its own path: written. Another plugin's: refused, naming both.
    w.set(Set::new("second/hue", to(), Origin::User)).unwrap();
    assert_eq!(app.handle().value::<f64>("second/hue"), Some(0.25));
    let e = w
        .set(Set::new("first/hue", to(), Origin::User))
        .expect_err("not a writer of another plugin's path");
    assert!(
        matches!(&e, Refused::NotWriter { path, by, owner, .. }
            if path == "first/hue" && *by == OwnerId::from_static("second") && *owner == first()),
        "{e:?}"
    );
    assert_eq!(app.handle().value::<f64>("first/hue"), Some(0.5));
}

#[test]
fn a_plugin_sees_which_plugins_are_connected_before_it() {
    let seen: Rc<RefCell<Vec<bool>>> = Rc::default();
    let s = seen.clone();
    let mut second = p("second", move |ctx| {
        s.borrow_mut().extend([
            ctx.is_connected(&PluginId::from_static("first")),
            ctx.is_connected(&PluginId::from_static("third")),
        ]);
        Ok(())
    });
    second.requires = vec![PluginId::from_static("first")];
    App::builder()
        .plugin(second)
        .plugin(p("first", |_| Ok(())))
        .plugin(p("third", |_| Ok(())))
        .build(&egui::Context::default())
        .unwrap();
    // `first` is required, so connected before; `third` is not yet.
    assert_eq!(*seen.borrow(), [true, false]);
}

#[test]
fn a_panel_brings_its_focus_and_toggle_commands() {
    const PANEL: PanelId = PanelId::from_static("probe.panel");
    let mut app = App::builder()
        .plugin(p("first", |ctx| {
            ctx.panel(PanelSpec::new(PANEL, "Probe", Area::EndSide), |_, _| {})?;
            Ok(())
        }))
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle().clone();
    let commands: Vec<_> = h
        .commands()
        .into_iter()
        .filter(|(s, _)| s.id.as_str().starts_with("probe.panel."))
        .map(|(s, owner)| (s.id.to_string(), s.title, owner))
        .collect();
    assert_eq!(
        commands.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(),
        ["probe.panel.focus", "probe.panel.toggle"]
    );
    assert!(commands.iter().all(|c| c.2 == first()), "{commands:?}");
    assert!(
        commands.iter().all(|c| c.1.contains("Probe")),
        "{commands:?}"
    );

    // The toggle answers what it did.
    let toggle = CommandId::from_static("probe.panel.toggle");
    let shown = h.panel_visible(&PANEL);
    let r = app.run(&toggle, Json::Null, Origin::User).now();
    assert_eq!(r, Some(Ok(json!({ "shown": !shown }))));
    assert_eq!(h.panel_visible(&PANEL), !shown);
    let r = app.run(&toggle, Json::Null, Origin::User).now();
    assert_eq!(r, Some(Ok(json!({ "shown": shown }))));

    // A second panel with the same id is refused.
    let (_, e, _) = refused(
        p("first", |ctx| {
            ctx.panel(PanelSpec::new(PANEL, "Probe", Area::EndSide), |_, _| {})?;
            Ok(())
        }),
        p("second", |ctx| {
            ctx.panel(PanelSpec::new(PANEL, "Again", Area::EndSide), |_, _| {})?;
            Ok(())
        }),
    );
    assert!(matches!(e, CtxError::Panel(_)), "{e:?}");
}
