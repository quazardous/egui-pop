//! Plugins, Ctx, zones and blocks, hooks, services, settings: ordering,
//! errors, removal on disconnect, rendering order (headless).

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::hooks::{Hook, Lifecycle, On};
use egui_pop_api::services::ServiceToken;
use egui_pop_api::stories::Story;
use egui_pop_api::zones::{self, Block, Place};
use egui_pop_api::{App, AppBuilder, BuildError, Ctx, Plugin, PluginError};
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{
    BlockId, CommandId, HookId, MenuId, PanelId, PluginId, SettingId, StoryId, ZoneId,
};
use egui_pop_model::menus::{Item, MenuItem};
use egui_pop_model::order::OrderError;
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_model::values::Descriptor;
use serde_json::{Value as Json, json};

type Connect = Box<dyn FnMut(&mut Ctx<'_>) -> Result<(), PluginError>>;

/// A plugin made of closures.
struct P {
    id: &'static str,
    requires: Vec<&'static str>,
    priority: i32,
    enabled: bool,
    connect: Connect,
    log: Rc<RefCell<Vec<String>>>,
}

impl P {
    fn new(
        id: &'static str,
        log: &Rc<RefCell<Vec<String>>>,
        connect: impl FnMut(&mut Ctx<'_>) -> Result<(), PluginError> + 'static,
    ) -> P {
        P {
            id,
            requires: Vec::new(),
            priority: 0,
            enabled: true,
            connect: Box::new(connect),
            log: log.clone(),
        }
    }

    fn requires(mut self, r: &'static str) -> P {
        self.requires.push(r);
        self
    }

    fn priority(mut self, p: i32) -> P {
        self.priority = p;
        self
    }
}

impl Plugin for P {
    fn id(&self) -> PluginId {
        PluginId::from_static(self.id)
    }

    fn requires(&self) -> Vec<PluginId> {
        self.requires
            .iter()
            .map(|r| PluginId::from_static(r))
            .collect()
    }

    fn priority(&self) -> i32 {
        self.priority
    }

    fn enabled(&self, _ctx: &Ctx<'_>) -> bool {
        self.enabled
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        self.log.borrow_mut().push(format!("connect {}", self.id));
        (self.connect)(ctx)
    }

    fn disconnect(&mut self, _ctx: &mut Ctx<'_>) {
        self.log
            .borrow_mut()
            .push(format!("disconnect {}", self.id));
    }
}

fn log() -> Rc<RefCell<Vec<String>>> {
    Rc::new(RefCell::new(Vec::new()))
}

fn nothing(_: &mut Ctx<'_>) -> Result<(), PluginError> {
    Ok(())
}

fn build(b: AppBuilder) -> App {
    b.build(&egui::Context::default()).unwrap()
}

#[test]
fn plugins_connect_in_order_and_errors_are_typed() {
    let l = log();
    let _app = build(
        App::builder()
            .plugin(P::new("pages", &l, nothing).requires("shell"))
            .plugin(P::new("shell", &l, nothing))
            .plugin(P::new("theme", &l, nothing).priority(10)),
    );
    assert_eq!(
        *l.borrow(),
        ["connect theme", "connect shell", "connect pages"]
    );

    let err = App::builder()
        .plugin(P::new("a", &l, nothing).requires("ghost"))
        .build(&egui::Context::default())
        .unwrap_err();
    assert!(
        matches!(err, BuildError::Order(OrderError::Missing { .. })),
        "{err}"
    );
    let err = App::builder()
        .plugin(P::new("a", &l, nothing).requires("b"))
        .plugin(P::new("b", &l, nothing).requires("a"))
        .build(&egui::Context::default())
        .unwrap_err();
    assert_eq!(err.to_string(), "plugins require each other: a -> b -> a");
    let err = App::builder()
        .plugin(P::new("bad", &l, |ctx| {
            ctx.command(
                CommandSpec::new(CommandId::from_static("bad.x"), "X", "Bad"),
                |_| Ok(Json::Null),
            )?;
            Err(PluginError::Failed("no device".into()))
        }))
        .build(&egui::Context::default())
        .unwrap_err();
    assert_eq!(err.to_string(), "plugin `bad` failed to connect: no device");
}

#[test]
fn a_disabled_plugin_and_its_dependents_are_skipped_and_say_why() {
    let l = log();
    let mut off = P::new("off", &l, nothing);
    off.enabled = false;
    let app = build(
        App::builder()
            .plugin(off)
            .plugin(P::new("needs-off", &l, nothing).requires("off")),
    );
    let infos = app.handle().plugins();
    assert!(infos.iter().all(|p| !p.connected));
    assert_eq!(infos[0].reason.as_deref(), Some("it is disabled"));
    assert_eq!(
        infos[1].reason.as_deref(),
        Some("its requirement `off` is not connected")
    );
    let codes: Vec<&str> = app.handle().diagnostics().iter().map(|d| d.code).collect();
    assert_eq!(codes, ["plugin-skipped"]);
}

const SERVICE: ServiceToken<String> = ServiceToken::new("demo.greeting");
const ZONE: ZoneId = ZoneId::from_static("demo.zone");

/// Registers one of everything.
fn everything(ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
    ctx.command(
        CommandSpec::new(CommandId::from_static("demo.go"), "Go", "Demo")
            .key("Ctrl+G")
            .menu(egui_pop_api::app::MENU_VIEW, "5_demo", 0),
        |_| Ok(json!("went")),
    )?
    .menu_item(MenuItem::command(
        egui_pop_api::app::MENU_FILE,
        "5_demo",
        0,
        CommandId::from_static("demo.go"),
    ))?
    .panel(
        PanelSpec::new(PanelId::from_static("demo.panel"), "Demo", Area::Bottom),
        |ui, _| {
            ui.label("panel");
        },
    )?
    .zone(ZONE, "Demo zone")?
    .setting(
        SettingId::from_static("demo.level"),
        Descriptor::int("Level", 0, 10, 3),
    )?
    .service(&SERVICE, "hello".to_string())?
    .hook(Hook::palette(HookId::from_static("demo.palette"), |_| {}))?
    .story(Story::new(
        StoryId::from_static("demo.story"),
        "Story",
        |ui, _| {
            ui.label("story");
        },
    ))?
    .step(egui_pop_api::steps::StepSpec::new(
        egui_pop_model::ids::StepId::from_static("demo.step"),
        "A demo step.",
    ))?
    .after(
        egui_pop_model::ids::StepId::from_static("demo.step"),
        |_, _, _| {},
    )
    .indicator(egui_pop_api::strips::Indicator::new(
        "demo.indicator",
        "Demo",
        "demo.",
    ))
    .list_provider("demo.list", |_, _| Vec::new())
    .on(On::Rebuilt, |_, _| {})
    .on_change("settings/", |_| {})
    .block(
        ZONE,
        Block::add(BlockId::from_static("demo.block"), 0, |ui, _| {
            ui.label("block");
        }),
    )
    .block(
        zones::MAIN,
        Block::add(BlockId::from_static("demo.main"), 0, |ui, _| {
            ui.label("main");
        }),
    );
    Ok(())
}

#[test]
fn everything_a_plugin_registers_goes_away_with_it() {
    let l = log();
    let mut app = build(
        App::builder()
            .plugin(P::new("base", &l, nothing))
            .plugin(P::new("demo", &l, everything)),
    );
    let with = app.census();
    assert!(
        app.handle().diagnostics().is_empty(),
        "{:?}",
        app.handle().diagnostics()
    );
    assert_eq!(
        app.handle()
            .service(&SERVICE)
            .as_deref()
            .map(String::as_str),
        Some("hello")
    );
    assert_eq!(
        app.disconnect(&PluginId::from_static("demo")),
        [PluginId::from_static("demo")]
    );
    app.rebuild();
    let without = app.census();
    for (k, v) in &without {
        assert!(
            with[k] > *v,
            "the plugin registered {k}, and they went: {with:?} {without:?}"
        );
    }
    // Each kind went back to what the framework alone has.
    let bare = build(App::builder().plugin(P::new("base", &l, nothing))).census();
    assert_eq!(without, bare);
    assert!(app.handle().service(&SERVICE).is_none());
    assert!(
        app.handle()
            .palette()
            .iter()
            .all(|e| e.command.as_str() != "demo.go")
    );
    // Connected again: everything is back.
    app.connect(&PluginId::from_static("demo")).unwrap();
    app.rebuild();
    assert_eq!(app.census(), with);
    assert_eq!(
        *l.borrow(),
        [
            "connect base",
            "connect demo",
            "disconnect demo",
            "connect base",
            "connect demo"
        ]
    );
}

#[test]
fn disconnecting_a_plugin_disconnects_those_requiring_it_first() {
    let l = log();
    let mut app = build(
        App::builder()
            .plugin(P::new("a", &l, nothing))
            .plugin(P::new("b", &l, nothing).requires("a"))
            .plugin(P::new("c", &l, nothing).requires("b")),
    );
    let gone = app.disconnect(&PluginId::from_static("a"));
    assert_eq!(gone, ["c", "b", "a"].map(PluginId::from_static));
    app.connect(&PluginId::from_static("c")).unwrap();
    assert!(app.handle().plugins().iter().all(|p| p.connected));
}

#[test]
fn a_command_declared_once_yields_its_key_menu_palette_and_agent_entries() {
    let l = log();
    let mut app = build(App::builder().plugin(P::new("demo", &l, everything)));
    let go = CommandId::from_static("demo.go");
    assert_eq!(app.keymap().keys_for(&go)[0].to_string(), "Ctrl+G");
    let view = app.handle().menu(&egui_pop_api::app::MENU_VIEW);
    assert!(
        view.iter()
            .any(|i| matches!(&i.item, Item::Command { id, .. } if *id == go))
    );
    let palette = app.handle().palette();
    let entry = palette.iter().find(|e| e.command == go).unwrap();
    assert_eq!(entry.keys, ["Ctrl+G"]);
    assert!(
        app.handle()
            .commands()
            .iter()
            .any(|(s, o)| s.id == go && s.agent && o.as_str() == "demo")
    );
    assert_eq!(
        app.run(&go, Json::Null, Origin::Agent("t".into())).now(),
        Some(Ok(json!("went")))
    );
    assert_eq!(
        app.handle().history().last().unwrap().origin,
        Origin::Agent("t".into())
    );
}

fn order_of(h: &Harness<'_, App>, labels: &[&str]) -> Vec<String> {
    let mut found: Vec<(f32, String)> = labels
        .iter()
        .filter_map(|l| h.query_by_label(l).map(|n| (n.rect().top(), l.to_string())))
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, l)| l).collect()
}

fn label_block(id: &'static str, weight: i32) -> Block {
    Block::add(BlockId::from_static(id), weight, move |ui, _| {
        ui.label(id);
    })
}

#[test]
fn blocks_are_added_replaced_extended_and_wrapped_in_order() {
    let l = log();
    let owner = P::new("owner", &l, |ctx| {
        ctx.block(zones::MAIN, label_block("b.two", 20))
            .block(zones::MAIN, label_block("b.one", 10))
            .block(zones::MAIN, label_block("b.three", 30));
        Ok(())
    });
    let other = P::new("other", &l, |ctx| {
        ctx.block(
            zones::MAIN,
            Block::replace(
                BlockId::from_static("b.two"),
                BlockId::from_static("x.two"),
                |ui, _| {
                    ui.label("x.two");
                },
            ),
        )
        .block(
            zones::MAIN,
            Block::extend(
                Place::After(BlockId::from_static("b.one")),
                BlockId::from_static("x.after-one"),
                |ui, _| {
                    ui.label("x.after-one");
                },
            ),
        )
        .block(
            zones::MAIN,
            Block::wrap(
                BlockId::from_static("b.three"),
                BlockId::from_static("x.wrap"),
                |ui, _, inner| {
                    ui.label("x.before-three");
                    inner(ui);
                    ui.label("x.after-three");
                },
            ),
        );
        Ok(())
    })
    .requires("owner");
    let mut h = Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui_state(
            |ui, app: &mut App| app.show(ui),
            build(App::builder().plugin(owner).plugin(other)),
        );
    h.run();
    let all = [
        "b.one",
        "b.two",
        "x.two",
        "x.after-one",
        "x.before-three",
        "b.three",
        "x.after-three",
    ];
    assert_eq!(
        order_of(&h, &all),
        [
            "b.one",
            "x.after-one",
            "x.two",
            "x.before-three",
            "b.three",
            "x.after-three"
        ]
    );
    assert!(h.state().handle().diagnostics().is_empty());
    let main = h
        .state()
        .handle()
        .zones()
        .into_iter()
        .find(|z| z.id == zones::MAIN)
        .unwrap();
    assert_eq!(main.blocks[2].notes, ["replaced `b.two` (owner)"]);
    // The other plugin goes: the zone is as its owner made it.
    h.state_mut().disconnect(&PluginId::from_static("other"));
    h.run();
    assert_eq!(order_of(&h, &all), ["b.one", "b.two", "b.three"]);
}

#[test]
fn what_cannot_apply_is_reported_with_what_happens_instead() {
    let l = log();
    let p = P::new("p", &l, |ctx| {
        let nowhere = ZoneId::from_static("nowhere");
        let ghost = BlockId::from_static("ghost");
        ctx.block(nowhere, label_block("p.lost", 0))
            .block(zones::MAIN, label_block("p.a", 0))
            .block(zones::MAIN, label_block("p.a", 1))
            .block(
                zones::MAIN,
                Block::replace(ghost.clone(), BlockId::from_static("p.r"), |_, _| {}),
            )
            .block(
                zones::MAIN,
                Block::extend(
                    Place::Before(ghost.clone()),
                    BlockId::from_static("p.e"),
                    |_, _| {},
                ),
            )
            .block(
                zones::MAIN,
                Block::wrap(ghost, BlockId::from_static("p.w"), |_, _, _| {}),
            );
        ctx.hook(
            Hook::palette(HookId::from_static("p.hook"), |_| {})
                .after(HookId::from_static("p.missing")),
        )?;
        ctx.menu_item(MenuItem::command(
            egui_pop_api::app::MENU_FILE,
            "1",
            0,
            CommandId::from_static("p.undeclared"),
        ))?;
        Ok(())
    });
    let app = build(App::builder().plugin(p));
    let mut codes: Vec<&str> = app.handle().diagnostics().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    assert_eq!(
        codes,
        [
            "dangling-after",
            "dangling-extend",
            "dangling-replace",
            "dangling-wrap",
            "duplicate-block",
            "unknown-command",
            "unknown-zone"
        ]
    );
    let replace = app
        .handle()
        .diagnostics()
        .into_iter()
        .find(|d| d.code == "dangling-replace")
        .unwrap();
    assert!(
        replace.message.contains("it is added as a new block"),
        "{}",
        replace.message
    );
    assert_eq!(replace.owner.unwrap().as_str(), "p");
    // A duplicate registration that changes the outcome is an error.
    let dup = App::builder()
        .plugin(P::new("d", &l, |ctx| {
            ctx.zone(zones::MAIN, "again")?;
            Ok(())
        }))
        .build(&egui::Context::default())
        .unwrap_err();
    assert!(
        dup.to_string()
            .contains("zone `app.main` is already declared by `framework`"),
        "{dup}"
    );
}

#[test]
fn alter_hooks_run_by_priority_and_after_at_build_time() {
    let l = log();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let c = calls.clone();
    let p = P::new("p", &l, move |ctx| {
        let file = egui_pop_api::app::MENU_FILE;
        let (c1, c2, c3) = (c.clone(), c.clone(), c.clone());
        ctx.hook(Hook::menu(
            HookId::from_static("h.low"),
            file.clone(),
            move |items| {
                c1.borrow_mut().push("low");
                items.retain(
                    |i| !matches!(&i.item, Item::Command { id, .. } if id.as_str() == "app.quit"),
                );
            },
        ))?
        .hook(
            Hook::menu(HookId::from_static("h.high"), file.clone(), move |_| {
                c2.borrow_mut().push("high")
            })
            .priority(10),
        )?
        .hook(
            Hook::menu(HookId::from_static("h.after"), file, move |_| {
                c3.borrow_mut().push("after-low")
            })
            .priority(99)
            .after(HookId::from_static("h.low")),
        )?;
        Ok(())
    });
    let app = build(App::builder().plugin(p));
    assert_eq!(*calls.borrow(), ["high", "low", "after-low"]);
    let file = app.handle().menu(&egui_pop_api::app::MENU_FILE);
    assert!(
        file.iter()
            .all(|i| !matches!(&i.item, Item::Command { id, .. } if id.as_str() == "app.quit"))
    );
    // The quit command is still there, by key and palette; only the menu
    // entry is gone.
    assert!(
        !app.keymap()
            .keys_for(&CommandId::from_static("app.quit"))
            .is_empty()
    );
}

#[test]
fn lifecycle_handlers_hear_start_and_departures() {
    let l = log();
    let heard = Rc::new(RefCell::new(Vec::new()));
    let h2 = heard.clone();
    let watcher = P::new("watcher", &l, move |ctx| {
        let h = h2.clone();
        ctx.on(On::Started, {
            let h = h.clone();
            move |_, e| h.borrow_mut().push(e.kind().to_string())
        })
        .on(On::Disconnected, move |_, e| {
            if let Lifecycle::Disconnected(p) = e {
                h.borrow_mut().push(format!("gone {p}"));
            }
        });
        Ok(())
    });
    let mut app = build(
        App::builder()
            .plugin(watcher)
            .plugin(P::new("leaf", &l, nothing)),
    );
    app.disconnect(&PluginId::from_static("leaf"));
    assert_eq!(*heard.borrow(), ["started", "gone leaf"]);
}

#[test]
fn keys_run_commands_through_the_shell_and_the_palette_lists_them() {
    let l = log();
    let ran = Rc::new(RefCell::new(0));
    let r = ran.clone();
    let p = P::new("p", &l, move |ctx| {
        let r = r.clone();
        ctx.command(
            CommandSpec::new(CommandId::from_static("p.count"), "Count", "Demo").key("Ctrl+J"),
            move |_| {
                *r.borrow_mut() += 1;
                Ok(Json::Null)
            },
        )?;
        Ok(())
    });
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 500.0))
        .build_ui_state(
            |ui, app: &mut App| app.show(ui),
            build(
                App::builder()
                    .plugins(egui_pop_api::DefaultPlugins)
                    .plugin(p),
            ),
        );
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::J);
    h.run();
    assert_eq!(*ran.borrow(), 1);
    // The palette opens on its key, filters, and Enter runs the command.
    h.key_press_modifiers(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::P,
    );
    h.run();
    assert!(egui_pop_api::palette::is_open(h.state().handle()));
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("count");
    h.run();
    assert!(h.query_by_label_contains("Demo: Count").is_some());
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(*ran.borrow(), 2);
    assert!(!egui_pop_api::palette::is_open(h.state().handle()));
    let trace = h.state().dispatcher().traces().next().unwrap().clone();
    assert_eq!(trace.keys, "Ctrl+J");
    // The menu bar shows the framework's menus.
    assert!(h.query_by_label("File").is_some());
    let _ = MenuId::from_static("x");
}

#[test]
fn settings_persist_and_come_back_with_their_plugin() {
    let dir = std::env::temp_dir().join(format!("egui-pop-api-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let l = log();
    let make = |l: &Rc<RefCell<Vec<String>>>| {
        P::new("prefs", l, |ctx| {
            ctx.setting(
                SettingId::from_static("prefs.size"),
                Descriptor::int("Size", 8, 32, 12),
            )?;
            Ok(())
        })
    };
    let app = build(App::builder().persist(&dir).plugin(make(&l)));
    // The framework's own (the focus mode, the interface zoom) come
    // first.
    let settings = app.handle().settings();
    assert_eq!(settings.len(), 3);
    assert_eq!(settings[0].id, egui_pop_api::SETTING_FOCUS);
    assert_eq!(settings[1].id, egui_pop_api::zoom::SETTING_ZOOM);
    assert_eq!(settings[2].section, "prefs");
    egui_pop_core::lock(&app.handle().store())
        .set(egui_pop_model::values::Set::new(
            "settings/prefs.size",
            egui_pop_model::values::Change::To(egui_pop_model::values::Value::Int(20)),
            Origin::User,
        ))
        .unwrap();
    app.save().unwrap();
    let again = build(App::builder().persist(&dir).plugin(make(&l)));
    let v = egui_pop_core::lock(&again.handle().store())
        .get("settings/prefs.size")
        .unwrap()
        .value
        .clone();
    assert_eq!(v, egui_pop_model::values::Value::Int(20));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_focus_mode_follows_the_system_unless_the_setting_says_otherwise() {
    use egui_pop_api::core::focus_mode::{self, FocusMode, SystemFocus};
    use egui_pop_model::values::{Change, Set, Value};
    let app = App::builder()
        .system_focus(SystemFocus {
            mode: FocusMode::Hover,
            from: Some("test"),
        })
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    assert_eq!(focus_mode::mode(&h.ctx), FocusMode::Hover);
    let modes = h
        .state()
        .handle()
        .service(&egui_pop_api::FOCUS_MODE)
        .unwrap();
    assert_eq!(modes.report().from, Some("test"));
    let setting = h
        .state()
        .handle()
        .settings()
        .into_iter()
        .find(|s| s.id == egui_pop_api::SETTING_FOCUS)
        .unwrap();
    assert_eq!(setting.section, "Appearance");
    assert!(setting.descriptor.description.contains("(test: hover)"));
    egui_pop_api::core::lock(&h.state().handle().store())
        .set(Set::new(
            &setting.path,
            Change::To(Value::Choice("click".into())),
            Origin::User,
        ))
        .unwrap();
    h.run();
    assert_eq!(focus_mode::mode(&h.ctx), FocusMode::Click);
    let snapshot = h.state().handle().debug_snapshot();
    assert_eq!(snapshot["focus"]["mode"]["mode"], "click", "{snapshot}");
    assert_eq!(snapshot["focus"]["mode"]["system"], "hover");
}

#[test]
fn the_palette_asks_for_arguments_and_escape_steps_back() {
    use egui_pop_api::palette;
    let got: Rc<RefCell<Vec<Json>>> = Rc::default();
    let g = got.clone();
    let l = log();
    let p = P::new("zoom", &l, move |ctx| {
        let g = g.clone();
        ctx.command(
            CommandSpec::new(CommandId::from_static("zoom.set"), "Set zoom", "View")
                .args(json!({ "percent": 100 })),
            move |call| {
                g.borrow_mut().push(call.args.clone());
                Ok(Json::Null)
            },
        )?;
        Ok(())
    });
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 500.0))
        .build_ui_state(
            |ui, app: &mut App| app.show(ui),
            build(
                App::builder()
                    .plugins(egui_pop_api::DefaultPlugins)
                    .plugin(p),
            ),
        );
    let open = |h: &mut Harness<'_, App>| {
        h.key_press_modifiers(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::P,
        );
        h.run();
    };
    h.run();
    open(&mut h);
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("szoom");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    // It asks for the argument; Escape goes back to the list.
    assert!(h.query_by_label_contains("Set zoom: percent").is_some());
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(palette::is_open(h.state().handle()));
    assert!(h.query_by_label_contains("Set zoom: percent").is_none());
    h.key_press(egui::Key::Enter);
    h.run();
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("lots");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(h.query_by_label_contains("is not a number").is_some());
    for _ in 0..4 {
        h.key_press(egui::Key::Backspace);
    }
    h.run();
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("50");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    h.run();
    assert_eq!(*got.borrow(), [json!({ "percent": 50 })]);
    assert!(!palette::is_open(h.state().handle()));
    // Run lately: first in the list, marked recent; Escape closes.
    open(&mut h);
    assert!(palette::is_open(h.state().handle()));
    let zoom = h.get_by_label_contains("View: Set zoom").rect();
    let quit = h.get_by_label_contains("File: Quit").rect();
    assert!(zoom.top() < quit.top(), "the recent command comes first");
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(!palette::is_open(h.state().handle()));
}

#[test]
fn pass_handlers_run_at_every_logic_pass_and_go_with_their_plugin() {
    let l = log();
    let seen = Rc::new(RefCell::new(0));
    let s = seen.clone();
    let ctx = egui::Context::default();
    let mut app = App::builder()
        .plugin(P::new("ticker", &l, move |ctx| {
            let s = s.clone();
            ctx.on(On::Pass, move |_, e| {
                assert_eq!(e, &Lifecycle::Pass);
                *s.borrow_mut() += 1;
            });
            Ok(())
        }))
        .build(&ctx)
        .unwrap();
    // A hidden window runs the logic alone.
    app.logic(&ctx);
    app.logic(&ctx);
    assert_eq!(*seen.borrow(), 2);
    app.disconnect(&PluginId::from_static("ticker"));
    app.logic(&ctx);
    assert_eq!(*seen.borrow(), 2);
}
