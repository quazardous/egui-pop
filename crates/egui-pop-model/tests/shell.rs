//! Commands, keys, menus, panels and layouts, headless.

use egui_pop_model::commands::{CommandSpec, Commands, Origin};
use egui_pop_model::ids::{CommandId, MenuId, OwnerId, PanelId};
use egui_pop_model::keymap::{
    Action, BindOptions, DocBinding, Keymap, KeymapDoc, Keys, Layer, Resolution, Typing, Verdict,
};
use egui_pop_model::layout::{Auxiliary, Layout, Node};
use egui_pop_model::panels::{Area, PanelSpec, Panels};
use egui_pop_model::when::WhenContext;
use serde_json::{Value, json};

fn cmd(id: &'static str) -> CommandId {
    CommandId::from_static(id)
}

const FW: OwnerId = OwnerId::FRAMEWORK;

#[test]
fn commands_run_by_id_from_any_origin_and_are_recorded() {
    let mut r = Commands::new();
    r.register_fn(
        &FW,
        CommandSpec::new(cmd("list.rate"), "Rate", "List").when("listFocus"),
        |args, _| Ok(args["stars"].clone()),
    )
    .unwrap();
    let dup = r
        .register_fn(
            &OwnerId::USER,
            CommandSpec::new(cmd("list.rate"), "Again", "x"),
            |_, _| Ok(Value::Null),
        )
        .unwrap_err();
    assert!(
        dup.to_string().contains("already declared by `framework`"),
        "{dup}"
    );
    assert!(
        r.register_fn(
            &FW,
            CommandSpec::new(cmd("bad"), "Bad", "x").when("a &&"),
            |_, _| Ok(Value::Null)
        )
        .is_err()
    );
    let list = WhenContext::new().flag("listFocus", true);
    assert_eq!(
        r.run(&cmd("list.rate"), json!({"stars": 3}), Origin::User, &list),
        Ok(json!(3))
    );
    let agent = Origin::Agent("session-1".into());
    let err = r
        .run(
            &cmd("list.rate"),
            json!({}),
            agent.clone(),
            &WhenContext::new(),
        )
        .unwrap_err();
    assert!(err.contains("does not apply"), "{err}");
    assert!(
        r.run(&cmd("nope"), Value::Null, agent.clone(), &list)
            .is_err()
    );
    assert_eq!(r.history().count(), 2, "unknown commands are not runs");
    assert_eq!(r.history().nth(1).unwrap().origin, agent);
    assert_eq!(r.search("rate li", &list).len(), 1);
    assert!(
        r.search("rate", &WhenContext::new()).is_empty(),
        "not offered where it does not apply"
    );
}

#[test]
fn search_ranks_title_prefixes_first_and_skips_hidden_commands() {
    let mut r = Commands::new();
    for (id, title) in [
        ("a.one", "Toggle sidebar"),
        ("a.two", "Open settings"),
        ("a.three", "Show the settings of the sidebar"),
    ] {
        r.register_fn(
            &FW,
            CommandSpec::new(CommandId::new(id).unwrap(), title, "View"),
            |_, _| Ok(Value::Null),
        )
        .unwrap();
    }
    r.register_fn(
        &FW,
        CommandSpec::new(cmd("a.hidden"), "Settings dump", "View").hidden(),
        |_, _| Ok(Value::Null),
    )
    .unwrap();
    let titles: Vec<&str> = r
        .search("settings", &WhenContext::new())
        .iter()
        .map(|s| s.title.as_str())
        .collect();
    assert_eq!(
        titles,
        ["Open settings", "Show the settings of the sidebar"]
    );
    let titles: Vec<&str> = r
        .search("tog", &WhenContext::new())
        .iter()
        .map(|s| s.title.as_str())
        .collect();
    assert_eq!(titles, ["Toggle sidebar"]);
}

#[test]
fn removing_an_owner_removes_its_commands() {
    let mut r = Commands::new();
    let plugin = OwnerId::from_static("plugin.a");
    r.register_fn(&plugin, CommandSpec::new(cmd("a.x"), "X", "A"), |_, _| {
        Ok(Value::Null)
    })
    .unwrap();
    r.register_fn(&FW, CommandSpec::new(cmd("f.y"), "Y", "F"), |_, _| {
        Ok(Value::Null)
    })
    .unwrap();
    assert_eq!(r.remove_owner(&plugin), [cmd("a.x")]);
    assert_eq!(r.len(), 1);
    // The id is free again.
    r.register_fn(&FW, CommandSpec::new(cmd("a.x"), "X", "A"), |_, _| {
        Ok(Value::Null)
    })
    .unwrap();
}

fn keys(k: &str) -> Keys {
    Keys::parse(k).unwrap()
}

fn run(id: &'static str) -> Resolution {
    Resolution::Run(cmd(id))
}

#[test]
fn layers_contexts_and_removal_decide_what_a_key_does() {
    let mut k = Keymap::new();
    let editor = OwnerId::from_static("editor");
    let field = OwnerId::from_static("field");
    k.bind(
        &FW,
        Layer::Default,
        "Ctrl+G",
        Action::run("item.group"),
        "listFocus",
    )
    .unwrap();
    k.bind(
        &FW,
        Layer::Default,
        "Ctrl+G",
        Action::run("node.group"),
        "canvasFocus",
    )
    .unwrap();
    k.bind(
        &editor,
        Layer::Suggestion,
        "Ctrl+G",
        Action::run("grid.toggle"),
        "",
    )
    .unwrap();
    k.bind(
        &field,
        Layer::Suggestion,
        "E",
        Action::run("field.edit"),
        "!textInputFocus",
    )
    .unwrap();
    let canvas = WhenContext::new().flag("canvasFocus", true);
    let list = WhenContext::new().flag("listFocus", true);
    let elsewhere = WhenContext::new();
    // The context picks among defaults; a suggestion only fills the gap.
    assert_eq!(k.resolve(&keys("Ctrl+G"), &canvas), run("node.group"));
    assert_eq!(k.resolve(&keys("Ctrl+G"), &list), run("item.group"));
    assert_eq!(k.resolve(&keys("Ctrl+G"), &elsewhere), run("grid.toggle"));
    // A single letter never fires while typing.
    let typing = WhenContext::new().flag("textInputFocus", true);
    assert_eq!(k.resolve(&keys("E"), &typing), Resolution::Nothing);
    // The user rebinds, and removes a default for good.
    k.bind(
        &OwnerId::USER,
        Layer::User,
        "Ctrl+G",
        Action::run("my.macro"),
        "",
    )
    .unwrap();
    assert_eq!(k.resolve(&keys("Ctrl+G"), &canvas), run("my.macro"));
    k.bind(
        &OwnerId::USER,
        Layer::User,
        "Ctrl+G",
        Action::Remove(cmd("item.group")),
        "",
    )
    .unwrap();
    assert!(k.keys_for(&cmd("item.group")).is_empty());
    // Conflicts are listed, losing suggestions included.
    let lost: Vec<String> = k
        .conflicts()
        .iter()
        .map(|(lose, _)| lose.action.to_file().unwrap())
        .collect();
    assert!(lost.contains(&"grid.toggle".to_string()), "{lost:?}");
}

#[test]
fn chords_wait_for_their_second_key_and_keys_are_logical() {
    let mut k = Keymap::new();
    k.bind(
        &FW,
        Layer::Default,
        "Ctrl+K Ctrl+S",
        Action::run("keys.editor"),
        "",
    )
    .unwrap();
    k.bind(&FW, Layer::Default, "3", Action::run("list.rate.3"), "")
        .unwrap();
    let cx = WhenContext::new();
    assert_eq!(k.resolve(&keys("Ctrl+K"), &cx), Resolution::Pending);
    assert_eq!(k.resolve(&keys("Ctrl+K Ctrl+S"), &cx), run("keys.editor"));
    assert_eq!(k.resolve(&keys("Ctrl+K Ctrl+X"), &cx), Resolution::Nothing);
    assert_eq!(k.resolve(&keys("Numpad3"), &cx), run("list.rate.3"));
    assert_eq!(keys("ctrl+shift+p").to_string(), "Ctrl+Shift+P");
    assert_eq!(keys("Esc").to_string(), "Escape");
    assert_eq!(keys("Ctrl++").to_string(), "Ctrl++");
    assert!(Keys::parse("Hyper+P").is_err());
}

#[test]
fn the_binding_nearest_the_focus_wins_and_the_trace_says_why() {
    let mut k = Keymap::new();
    let dialog = OwnerId::from_static("dialog");
    let field = OwnerId::from_static("field");
    // Bindings that fire while typing too (a plain key is otherwise
    // left to a text input, see `scopes.rs`).
    let always = || BindOptions::global().typing(Typing::Always);
    k.bind_with(
        &FW,
        Layer::Default,
        "Escape",
        Action::run("app.cancel"),
        "",
        always(),
    )
    .unwrap();
    k.bind_with(
        &dialog,
        Layer::Default,
        "Escape",
        Action::run("dialog.close"),
        "dialogFocus",
        always(),
    )
    .unwrap();
    k.bind(
        &field,
        Layer::Suggestion,
        "Escape",
        Action::run("field.clear"),
        "textInputFocus",
    )
    .unwrap();
    let mut cx = WhenContext::new();
    cx.flag_at("dialogFocus", true, 1);
    assert_eq!(k.resolve(&keys("Escape"), &cx), run("dialog.close"));
    // The text field inside the dialog is deeper: its suggestion wins
    // over the dialog's default.
    cx.flag_at("textInputFocus", true, 2);
    let (r, trace) = k.resolve_traced(&keys("Escape"), &cx);
    assert_eq!(r, run("field.clear"));
    let verdicts: Vec<(&str, &Verdict)> = trace
        .candidates
        .iter()
        .map(|c| (c.binding.owner.as_str(), &c.verdict))
        .collect();
    assert_eq!(
        verdicts,
        [
            (
                "framework",
                &Verdict::Lost {
                    to: "field.clear".into(),
                    because: "nearer the focus"
                }
            ),
            (
                "dialog",
                &Verdict::Lost {
                    to: "field.clear".into(),
                    because: "nearer the focus"
                }
            ),
            ("field", &Verdict::Won),
        ]
    );
    let text = trace.to_string();
    assert!(
        text.starts_with("Escape: runs field.clear (global)\n"),
        "{text}"
    );
    assert!(
        text.contains("lost to field.clear (nearer the focus)"),
        "{text}"
    );
    let json = serde_json::to_value(&trace).unwrap();
    assert_eq!(
        json["resolution"],
        json!({"kind": "run", "command": "field.clear"})
    );
}

#[test]
fn null_unbinds_keys_and_minus_removes_a_command_from_the_layers_below() {
    let mut k = Keymap::new();
    k.bind(&FW, Layer::Default, "Ctrl+P", Action::run("app.print"), "")
        .unwrap();
    k.bind(
        &FW,
        Layer::Default,
        "Ctrl+Shift+P",
        Action::run("app.palette"),
        "",
    )
    .unwrap();
    k.bind(&FW, Layer::Default, "F1", Action::run("app.palette"), "")
        .unwrap();
    let doc = KeymapDoc {
        bindings: vec![
            DocBinding {
                keys: "Ctrl+P".into(),
                command: None,
                when: String::new(),
                ..DocBinding::default()
            },
            DocBinding {
                keys: "Ctrl+K".into(),
                command: Some("-app.palette".into()),
                when: String::new(),
                ..DocBinding::default()
            },
            DocBinding {
                keys: "Ctrl+Q".into(),
                command: Some("not an id".into()),
                when: String::new(),
                ..DocBinding::default()
            },
        ],
    };
    let d = k.load(&OwnerId::USER, Layer::User, &doc);
    assert_eq!(d.codes(), ["bad-binding"]);
    let cx = WhenContext::new();
    assert_eq!(k.resolve(&keys("Ctrl+P"), &cx), Resolution::Nothing);
    assert!(k.keys_for(&cmd("app.palette")).is_empty());
    let (_, trace) = k.resolve_traced(&keys("Ctrl+P"), &cx);
    assert!(matches!(
        trace.candidates[0].verdict,
        Verdict::UnboundBy {
            layer: Layer::User,
            ..
        }
    ));
    // Saved and read back, through the document's schema.
    let saved = egui_pop_model::keymap_schema()
        .save(&k.export(&OwnerId::USER, Layer::User))
        .unwrap();
    assert!(saved.contains("\"command\": null"), "{saved}");
    let back: KeymapDoc = egui_pop_model::keymap_schema().load(&saved).unwrap().value;
    assert_eq!(back.bindings.len(), 2);
    // Removing the user's layer brings the defaults back.
    assert_eq!(k.remove_owner(&OwnerId::USER), 2);
    assert_eq!(k.resolve(&keys("Ctrl+P"), &cx), run("app.print"));
}

fn pid(id: &str) -> PanelId {
    PanelId::new(id).unwrap()
}

fn panels() -> Panels {
    let mut p = Panels::new();
    for (id, area) in [
        ("app.editor", Area::Center),
        ("app.log", Area::Bottom),
        ("app.inspector", Area::EndSide),
        ("app.files", Area::StartSide),
    ] {
        p.declare(
            &FW,
            PanelSpec::new(pid(id), id.trim_start_matches("app."), area),
        )
        .unwrap();
    }
    p
}

#[test]
fn a_layout_round_trips_and_is_repaired_against_panels_and_screens() {
    let mut panels = panels();
    assert_eq!(
        panels.shell_commands().len(),
        8,
        "focus and toggle per panel"
    );
    assert!(
        panels
            .declare(&FW, PanelSpec::new(pid("app.log"), "again", Area::Bottom))
            .is_err()
    );
    let mut layout = Layout::default_for(&panels);
    assert_eq!(layout.placed().len(), 4);
    // The user detaches the log to a second screen.
    for (_, node) in &mut layout.areas {
        if let Node::Tabs { panels, .. } = node {
            panels.retain(|p| p.as_str() != "app.log");
        }
    }
    layout
        .areas
        .retain(|(_, n)| !matches!(n, Node::Tabs { panels, .. } if panels.is_empty()));
    layout.auxiliary.push(Auxiliary {
        panel: pid("app.log"),
        screen: "HDMI-1".into(),
        rect: [0.0, 0.0, 800.0, 600.0],
    });
    let saved = Layout::from_json(&layout.to_json()).unwrap();
    assert_eq!(saved, layout);
    // Same screens: unchanged.
    let (same, notes) = saved.clone().repair(&panels, &["eDP-1", "HDMI-1"]);
    assert_eq!((same, notes.len()), (layout.clone(), 0));
    // The second screen is gone, a panel disappeared, a new one appeared.
    let mut next = Panels::new();
    for p in panels
        .all()
        .into_iter()
        .filter(|p| p.id.as_str() != "app.files")
    {
        next.declare(&FW, p.clone()).unwrap();
    }
    next.declare(&FW, PanelSpec::new(pid("app.queue"), "queue", Area::Bottom))
        .unwrap();
    let (fixed, notes) = saved.repair(&next, &["eDP-1"]);
    let placed: Vec<&str> = fixed.placed().iter().map(|p| p.as_str()).collect();
    assert!(placed.contains(&"app.log") && placed.contains(&"app.queue"));
    assert!(!placed.contains(&"app.files"));
    assert!(fixed.auxiliary.is_empty());
    assert_eq!(notes.len(), 3, "{notes:?}");
}

#[test]
fn a_closed_panel_stays_closed_when_the_layout_is_repaired() {
    let mut layout = Layout::default_for(&panels());
    let first = layout.placed()[0].clone();
    for (_, node) in &mut layout.areas {
        if let Node::Tabs { panels, .. } = node {
            panels.retain(|p| *p != first);
        }
    }
    layout
        .areas
        .retain(|(_, n)| !matches!(n, Node::Tabs { panels, .. } if panels.is_empty()));
    layout.hidden.push(first.clone());
    let (repaired, notes) = Layout::from_json(&layout.to_json())
        .unwrap()
        .repair(&panels(), &["primary"]);
    assert!(!repaired.placed().contains(&&first), "{notes:?}");
    assert_eq!(repaired.hidden, [first]);
}

#[test]
fn a_panel_found_where_it_may_not_be_goes_back_to_its_default_area() {
    let mut p = Panels::new();
    p.declare(
        &FW,
        PanelSpec::new(pid("app.editor"), "Editor", Area::Center).only_in(&[Area::Center]),
    )
    .unwrap();
    p.declare(&FW, PanelSpec::new(pid("app.log"), "Log", Area::Bottom))
        .unwrap();
    assert!(p.get("app.log").unwrap().allows(Area::Center));
    assert!(!p.get("app.editor").unwrap().allows(Area::StartSide));
    let saved = Layout {
        areas: vec![(
            Area::StartSide,
            Node::Tabs {
                panels: vec![pid("app.editor"), pid("app.log")],
                active: 0,
            },
        )],
        auxiliary: vec![],
        hidden: vec![],
    };
    let (fixed, notes) = saved.repair(&p, &[]);
    assert_eq!(
        fixed.areas,
        [
            (Area::Center, Node::tab(pid("app.editor"))),
            (Area::StartSide, Node::tab(pid("app.log"))),
        ]
    );
    assert_eq!(
        notes,
        ["`app.editor` cannot be in the start side bar: moved to its default area"]
    );
}

#[test]
fn a_version_1_layout_is_migrated_to_start_and_end_areas() {
    let v1 = r#"{
        "version": 1,
        "areas": [
            ["editors", {"kind": "tabs", "panels": ["app.editor"], "active": 0}],
            ["primary_side_bar", {"kind": "tabs", "panels": ["app.files"], "active": 0}]
        ],
        "auxiliary": []
    }"#;
    let layout = Layout::from_json(v1).unwrap();
    assert_eq!(layout.areas[0].0, Area::Center);
    assert_eq!(layout.areas[1].0, Area::StartSide);
    // Unversioned (version 0) documents read as version 1.
    let v0 = v1.replace("\"version\": 1,", "");
    assert_eq!(Layout::from_json(&v0).unwrap(), layout);
    let saved: Value = serde_json::from_str(&layout.to_json()).unwrap();
    assert_eq!(saved["version"], json!(2));
    assert_eq!(saved["kind"], json!("layout"));
    assert!(
        Layout::from_json(r#"{"version": 7, "areas": []}"#)
            .unwrap_err()
            .contains("newer")
    );
}

#[test]
fn menus_are_declared_grouped_and_follow_the_context() {
    use egui_pop_model::menus::{Item, MenuItem, Menus, Shown};
    let m_id = MenuId::from_static("app.main");
    let mut m = Menus::new();
    let plugin = OwnerId::from_static("plugin");
    m.contribute(
        &FW,
        MenuItem::command(m_id.clone(), "9_quit", 0, cmd("window.close")),
    )
    .unwrap();
    m.contribute(
        &FW,
        MenuItem::command(m_id.clone(), "1_open", 2, cmd("workspace.recent")),
    )
    .unwrap();
    m.contribute(
        &FW,
        MenuItem::command(m_id.clone(), "1_open", 1, cmd("workspace.open")),
    )
    .unwrap();
    m.contribute(
        &FW,
        MenuItem::command(m_id.clone(), "1_open", 3, cmd("palette.only"))
            .when("paletteVisible", false),
    )
    .unwrap();
    m.contribute(
        &plugin,
        MenuItem::command(m_id.clone(), "5_view", 0, cmd("window.fullscreen"))
            .when("paletteVisible", true)
            .checked("fullScreen"),
    )
    .unwrap();
    assert!(
        m.contribute(
            &FW,
            MenuItem::command(m_id.clone(), "x", 0, cmd("y")).when("a &&", true)
        )
        .is_err()
    );
    let id = |s: &Shown| match s {
        Shown::Item {
            item: Item::Command { id, .. },
            ..
        } => id.to_string(),
        Shown::Separator => "-".into(),
        _ => "?".into(),
    };
    let cx = WhenContext::new().flag("fullScreen", true);
    let shown = m.show(&m_id, &cx);
    let ids: Vec<String> = shown.iter().map(id).collect();
    // The palette-only item is hidden; full screen greyed but ticked.
    assert_eq!(
        ids,
        [
            "workspace.open",
            "workspace.recent",
            "-",
            "window.fullscreen",
            "-",
            "window.close"
        ]
    );
    assert!(matches!(
        shown[3],
        Shown::Item {
            enabled: false,
            checked: true,
            ..
        }
    ));
    assert_eq!(m.ids(), std::slice::from_ref(&m_id));
    assert_eq!(m.remove_owner(&plugin), 1);
    assert_eq!(m.show(&m_id, &cx).len(), 4);
}
