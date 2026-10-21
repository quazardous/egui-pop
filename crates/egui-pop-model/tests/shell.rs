//! Commands, keys, menus, panels and layouts, headless.

use egui_pop_model::commands::{CommandSpec, Commands, Origin};
use egui_pop_model::ids::{CommandId, MenuId, OwnerId, PanelId};
use egui_pop_model::keymap::{
    Action, BindOptions, DocBinding, Keymap, KeymapDoc, Keys, Layer, Resolution, Typing, Verdict,
};
use egui_pop_model::layout::{
    Anchor, AnchorEdge, Arrangement, Auxiliary, FloatingGroup, Join, Layout, Node, Placement,
};
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
        folded: vec![],
        over: vec![],
        flyouts: Default::default(),
        floating: vec![],
        docked_groups: vec![],
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

#[egui_pop_test_macros::critical]
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

#[test]
fn layout_operations_place_hide_activate_and_fold() {
    let mut p = Panels::new();
    for (id, area) in [("a.one", Area::Bottom), ("a.two", Area::Bottom)] {
        p.declare(&FW, PanelSpec::new(pid(id), id, area)).unwrap();
    }
    p.declare(
        &FW,
        PanelSpec::new(pid("a.late"), "Late", Area::StartSide).closed(),
    )
    .unwrap();
    let mut l = Layout::default_for(&p);
    assert!(l.hidden.contains(&pid("a.late")), "closed at first");
    assert_eq!(l.active_in(Area::Bottom), [&pid("a.one")]);
    assert!(l.activate(&pid("a.two")));
    assert_eq!(l.active_in(Area::Bottom), [&pid("a.two")]);
    l.place(&pid("a.late"), Area::Bottom);
    assert!(l.shows(&pid("a.late")) && !l.hidden.contains(&pid("a.late")));
    assert_eq!(l.active_in(Area::Bottom), [&pid("a.late")]);
    l.hide(&pid("a.one"));
    assert!(!l.shows(&pid("a.one")));
    assert!(l.remove(&pid("a.two")));
    assert!(!l.remove(&pid("a.two")));
    l.folded.push(Area::Bottom);
    let (back, _) = Layout::from_json(&l.to_json()).unwrap().repair(&p, &[]);
    assert_eq!(back.folded, [Area::Bottom]);
    // A side area lying over the centre is saved; only side areas may.
    assert_eq!(l.placement(Area::EndSide), Placement::Beside);
    l.set_placement(Area::EndSide, Placement::Over);
    l.over.push(Area::Bottom);
    let text = l.to_json();
    assert!(text.contains("\"over\""), "{text}");
    let (back, _) = Layout::from_json(&text).unwrap().repair(&p, &[]);
    assert_eq!(back.placement(Area::EndSide), Placement::Over);
    assert_eq!(back.over, [Area::EndSide]);
    let mut back = back;
    back.set_placement(Area::EndSide, Placement::Beside);
    assert!(back.over.is_empty());
    assert!(!back.to_json().contains("\"over\""));
    assert_eq!(Placement::parse("over"), Some(Placement::Over));
    assert_eq!(Placement::parse("under"), None);
    // A closed panel stays closed through a repair; a new one opens.
    assert!(back.hidden.contains(&pid("a.one")));
    // A flyout's width is kept per panel; a panel gone loses it, and the
    // centre never stays folded.
    back.flyouts.insert(pid("a.two"), 260.0);
    back.flyouts.insert(pid("a.gone"), 300.0);
    back.folded.push(Area::Center);
    let text = back.to_json();
    assert!(text.contains("\"flyouts\""), "{text}");
    let (again, _) = Layout::from_json(&text).unwrap().repair(&p, &[]);
    assert_eq!(again.flyouts.get(&pid("a.two")), Some(&260.0));
    assert!(!again.flyouts.contains_key(&pid("a.gone")));
    assert!(!again.folded.contains(&Area::Center));
}

#[test]
fn a_palette_floats_from_the_start_and_is_kept_floating() {
    let mut p = Panels::new();
    p.declare(&FW, PanelSpec::new(pid("app.log"), "Log", Area::Bottom))
        .unwrap();
    p.declare(
        &FW,
        PanelSpec::new(pid("app.tools"), "Tools", Area::StartSide).palette(),
    )
    .unwrap();
    let layout = Layout::default_for(&p);
    assert_eq!(layout.area_of(&pid("app.log")), Some(Area::Bottom));
    let tools = layout.floating_of(&pid("app.tools")).unwrap();
    assert_eq!(tools.pos, FloatingGroup::cascade(0));
    assert!(layout.shows(&pid("app.tools")));
    assert_eq!(layout.area_of(&pid("app.tools")), None);
    // Saved and read back, floating still.
    let read = Layout::from_json(&layout.to_json()).unwrap();
    assert_eq!(read, layout);
    let (same, notes) = read.repair(&p, &[]);
    assert_eq!((same, notes.len()), (layout.clone(), 0));
    // Docked, then floated again: one model, two placements.
    let mut l = layout;
    l.place(&pid("app.tools"), Area::StartSide);
    assert!(l.floating_of(&pid("app.tools")).is_none());
    assert_eq!(l.area_of(&pid("app.tools")), Some(Area::StartSide));
    l.float(&pid("app.tools"), [200.0, 100.0]);
    assert_eq!(l.area_of(&pid("app.tools")), None);
    assert_eq!(
        l.floating_of(&pid("app.tools")).unwrap().pos,
        [200.0, 100.0]
    );
    l.floating_mut(&pid("app.tools"))
        .unwrap()
        .member_mut(&pid("app.tools"))
        .unwrap()
        .rolled = true;
    l.float(&pid("app.tools"), [10.0, 20.0]);
    assert!(
        l.floating_of(&pid("app.tools"))
            .unwrap()
            .rolled(&pid("app.tools")),
        "kept"
    );
    // Closed: no longer placed, and kept closed.
    l.hide(&pid("app.tools"));
    assert!(!l.shows(&pid("app.tools")));
    assert!(l.floating.is_empty());
}

#[test]
fn a_floating_panel_that_may_no_longer_float_is_docked_back() {
    let mut p = Panels::new();
    p.declare(&FW, PanelSpec::new(pid("app.log"), "Log", Area::Bottom))
        .unwrap();
    let saved = Layout {
        floating: vec![
            FloatingGroup {
                pinned: true,
                ..FloatingGroup::lone(pid("app.log"), [40.0, 40.0])
            },
            FloatingGroup::lone(pid("app.gone"), [0.0, 0.0]),
        ],
        ..Layout::default()
    };
    let (fixed, notes) = saved.repair(&p, &[]);
    assert!(fixed.floating.is_empty());
    assert_eq!(fixed.area_of(&pid("app.log")), Some(Area::Bottom));
    assert_eq!(notes.len(), 2, "{notes:?}");
}

#[test]
fn a_layout_saved_before_anchors_loads_unanchored_and_anchors_round_trip() {
    // Written before floating panels had an anchor.
    let old = r#"{"version": 2, "kind": "layout", "areas": [],
        "floating": [{"panel": "app.tools", "pos": [12.0, 34.0], "pinned": true}]}"#;
    let l = Layout::from_json(old).unwrap();
    let f = l.floating_of(&pid("app.tools")).unwrap();
    assert_eq!(f.pos, [12.0, 34.0]);
    assert!(f.pinned);
    assert_eq!(f.anchor, Anchor::default());
    assert!(!l.to_json().contains("anchor"), "none is not written");
    // Anchored: kept through a save.
    let mut l = l;
    l.floating_mut(&pid("app.tools")).unwrap().anchor = Anchor {
        x: AnchorEdge::End,
        y: AnchorEdge::None,
        offset: [8.0, 0.0],
    };
    let text = l.to_json();
    assert!(text.contains("\"end\""), "{text}");
    let back = Layout::from_json(&text).unwrap();
    assert_eq!(back, l);
}

#[test]
fn a_palette_size_is_saved_and_a_palette_out_of_its_group_is_sized_by_its_content() {
    // Written before palettes could be resized: sized by their content.
    let old = r#"{"version": 2, "kind": "layout", "areas": [],
        "floating": [{"panel": "app.tools", "pos": [12.0, 34.0]}]}"#;
    let mut l = Layout::from_json(old).unwrap();
    assert_eq!(l.floating_of(&pid("app.tools")).unwrap().size, None);
    assert!(!l.to_json().contains("size"), "none is not written");
    l.floating_mut(&pid("app.tools")).unwrap().size = Some([240.0, 300.0]);
    let back = Layout::from_json(&l.to_json()).unwrap();
    assert_eq!(back, l);
    assert_eq!(
        back.floating_of(&pid("app.tools")).unwrap().size,
        Some([240.0, 300.0])
    );
    // A group keeps the size of the palette joined; one coming out of it
    // is sized by its content again.
    let mut l = Layout::default_for(&palettes());
    l.floating_mut(&pid("app.tools")).unwrap().size = Some([240.0, 300.0]);
    l.floating_mut(&pid("app.colours")).unwrap().size = Some([100.0, 100.0]);
    l.group(&[pid("app.colours")], &pid("app.tools"), Join::Below)
        .unwrap();
    assert_eq!(
        l.floating_of(&pid("app.colours")).unwrap().size,
        Some([240.0, 300.0])
    );
    l.ungroup(&pid("app.colours"), [400.0, 40.0]).unwrap();
    assert_eq!(l.floating_of(&pid("app.colours")).unwrap().size, None);
    assert_eq!(
        l.floating_of(&pid("app.tools")).unwrap().size,
        Some([240.0, 300.0])
    );
}

/// Three palettes and a docked panel.
fn palettes() -> Panels {
    let mut p = Panels::new();
    p.declare(&FW, PanelSpec::new(pid("app.log"), "Log", Area::Bottom))
        .unwrap();
    for id in ["app.tools", "app.colours", "app.nav"] {
        p.declare(&FW, PanelSpec::new(pid(id), id, Area::StartSide).palette())
            .unwrap();
    }
    p
}

fn members(l: &Layout, panel: &str) -> Vec<String> {
    l.floating_of(&pid(panel))
        .unwrap()
        .panels()
        .map(|p| p.to_string())
        .collect()
}

#[test]
fn palettes_stack_into_one_group_and_come_out_of_it() {
    let mut l = Layout::default_for(&palettes());
    assert_eq!(l.floating.len(), 3, "each alone: a group of one");
    l.floating_mut(&pid("app.colours"))
        .unwrap()
        .member_mut(&pid("app.colours"))
        .unwrap()
        .rolled = true;
    l.group(&[pid("app.colours")], &pid("app.tools"), Join::Below)
        .unwrap();
    l.group(&[pid("app.nav")], &pid("app.tools"), Join::Above)
        .unwrap();
    assert_eq!(l.floating.len(), 1);
    assert_eq!(
        members(&l, "app.tools"),
        ["app.nav", "app.tools", "app.colours"]
    );
    let g = l.floating_of(&pid("app.tools")).unwrap();
    assert_eq!(g.arrangement, Arrangement::Stack);
    assert!(
        g.rolled(&pid("app.colours")),
        "each member keeps its roll-up"
    );
    assert!(!g.rolled(&pid("app.tools")));
    assert_eq!(g.lead(), &pid("app.nav"));
    // One unit: floating any member moves the group.
    l.float(&pid("app.colours"), [300.0, 40.0]);
    assert_eq!(l.floating.len(), 1);
    assert_eq!(l.floating[0].pos, [300.0, 40.0]);
    // Saved and read back.
    let back = Layout::from_json(&l.to_json()).unwrap();
    assert_eq!(back, l);
    // Out of the group: alone where it is dropped, rolled up still.
    l.ungroup(&pid("app.colours"), [10.0, 400.0]).unwrap();
    assert_eq!(members(&l, "app.tools"), ["app.nav", "app.tools"]);
    let c = l.floating_of(&pid("app.colours")).unwrap();
    assert!(c.is_lone() && c.rolled(&pid("app.colours")));
    assert_eq!(c.pos, [10.0, 400.0]);
    assert!(l.ungroup(&pid("app.colours"), [0.0, 0.0]).is_err(), "alone");
    assert!(
        l.group(&[pid("app.log")], &pid("app.tools"), Join::Below)
            .is_err()
    );
    assert!(
        l.group(&[pid("app.tools")], &pid("app.tools"), Join::Below)
            .is_err()
    );
    // Closed: out of its group, the others stay together.
    l.hide(&pid("app.nav"));
    assert!(l.floating_of(&pid("app.tools")).unwrap().is_lone());
}

#[test]
fn a_tab_joins_a_group_shown_and_the_group_turns_to_tabs() {
    let mut l = Layout::default_for(&palettes());
    l.group(&[pid("app.colours")], &pid("app.tools"), Join::Below)
        .unwrap();
    l.group(&[pid("app.nav")], &pid("app.tools"), Join::Tab)
        .unwrap();
    let g = l.floating_of(&pid("app.tools")).unwrap();
    assert_eq!(g.arrangement, Arrangement::Tabs);
    assert_eq!(g.shown(), &pid("app.nav"));
    assert_eq!(
        members(&l, "app.tools"),
        ["app.tools", "app.nav", "app.colours"]
    );
    let text = l.to_json();
    assert!(text.contains("\"tabs\""), "{text}");
    assert_eq!(Layout::from_json(&text).unwrap(), l);
    // The shown tab out: the one before it is shown.
    l.ungroup(&pid("app.nav"), [0.0, 0.0]).unwrap();
    assert_eq!(
        l.floating_of(&pid("app.tools")).unwrap().shown(),
        &pid("app.tools")
    );
    // Down to one: a plain palette again.
    l.ungroup(&pid("app.colours"), [0.0, 0.0]).unwrap();
    let g = l.floating_of(&pid("app.tools")).unwrap();
    assert_eq!((g.arrangement, g.active), (Arrangement::Stack, 0));
    assert_eq!(Join::parse("tab"), Some(Join::Tab));
    assert_eq!(Join::parse("aside"), None);
}

#[test]
fn a_group_docks_whole_as_tabs_and_floats_back_as_a_group() {
    let mut l = Layout::default_for(&palettes());
    l.group(&[pid("app.colours")], &pid("app.tools"), Join::Below)
        .unwrap();
    l.float(&pid("app.tools"), [120.0, 80.0]);
    let docked = l.dock_group(&pid("app.colours"), Area::EndSide);
    assert_eq!(docked, [pid("app.tools"), pid("app.colours")]);
    assert!(l.floating_of(&pid("app.tools")).is_none());
    assert_eq!(l.area_of(&pid("app.tools")), Some(Area::EndSide));
    assert_eq!(l.area_of(&pid("app.colours")), Some(Area::EndSide));
    assert_eq!(l.active_in(Area::EndSide), [&pid("app.tools")]);
    assert_eq!(l.docked_groups.len(), 1);
    let back = Layout::from_json(&l.to_json()).unwrap();
    assert_eq!(back, l, "the docked group is kept with the layout");
    // Floating one of them gives the group back, where it is floated.
    l.float(&pid("app.colours"), [40.0, 40.0]);
    assert_eq!(members(&l, "app.tools"), ["app.tools", "app.colours"]);
    assert_eq!(l.floating_of(&pid("app.tools")).unwrap().pos, [40.0, 40.0]);
    assert!(l.docked_groups.is_empty());
    assert_eq!(l.area_of(&pid("app.tools")), None);
    // A member moved elsewhere in between stays docked.
    l.dock_group(&pid("app.tools"), Area::EndSide);
    l.place(&pid("app.tools"), Area::Bottom);
    l.float(&pid("app.colours"), [40.0, 40.0]);
    assert!(l.floating_of(&pid("app.colours")).unwrap().is_lone());
    assert_eq!(l.area_of(&pid("app.tools")), Some(Area::Bottom));
}

#[test]
fn a_group_is_repaired_member_by_member_and_old_applications_read_its_first() {
    let mut l = Layout::default_for(&palettes());
    l.group(&[pid("app.colours")], &pid("app.tools"), Join::Below)
        .unwrap();
    // Saved as a floating panel with the others under `group`.
    let text = l.to_json();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let first = &json["floating"][0];
    assert_eq!(first["panel"], "app.tools");
    assert_eq!(first["group"][0]["panel"], "app.colours");
    // A member no longer declared goes, the group stays.
    let mut fewer = Panels::new();
    fewer
        .declare(&FW, PanelSpec::new(pid("app.log"), "Log", Area::Bottom))
        .unwrap();
    for id in ["app.tools", "app.nav"] {
        fewer
            .declare(&FW, PanelSpec::new(pid(id), id, Area::StartSide).palette())
            .unwrap();
    }
    let (fixed, notes) = l.repair(&fewer, &[]);
    assert_eq!(members(&fixed, "app.tools"), ["app.tools"]);
    assert!(fixed.floating_of(&pid("app.tools")).unwrap().is_lone());
    assert_eq!(notes.len(), 1, "{notes:?}");
}
