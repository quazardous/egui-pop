//! Global and surface key bindings: precedence, typing, modals, traces.

use egui_pop_model::ids::{CommandId, OwnerId};
use egui_pop_model::keymap::{
    Action, BindOptions, KeyScope, Keymap, KeymapDoc, Keys, Layer, Resolution, Typing, Verdict,
};
use egui_pop_model::when::WhenContext;

const FW: OwnerId = OwnerId::FRAMEWORK;

fn keys(k: &str) -> Keys {
    Keys::parse(k).unwrap()
}

fn run(id: &'static str) -> Resolution {
    Resolution::Run(CommandId::from_static(id))
}

/// A keymap with `N` bound globally, on the outer `list` surface and on
/// the inner `cell` surface.
fn layered() -> Keymap {
    let mut k = Keymap::new();
    k.bind(&FW, Layer::Default, "N", Action::run("global.n"), "")
        .unwrap();
    k.bind_with(
        &FW,
        Layer::Default,
        "N",
        Action::run("list.n"),
        "",
        BindOptions::surface("list"),
    )
    .unwrap();
    k.bind_with(
        &FW,
        Layer::Default,
        "N",
        Action::run("cell.n"),
        "",
        BindOptions::surface("cell"),
    )
    .unwrap();
    k
}

/// The focus chain's context: `<name>Focus` at each depth.
fn chain(names: &[&str]) -> WhenContext {
    let mut cx = WhenContext::new();
    for (i, n) in names.iter().enumerate() {
        cx.flag_at(&format!("{n}Focus"), true, i as u32 + 1);
    }
    cx
}

#[test]
fn the_inner_focused_surface_wins_then_the_outer_then_global() {
    let k = layered();
    assert_eq!(k.resolve(&keys("N"), &chain(&[])), run("global.n"));
    assert_eq!(k.resolve(&keys("N"), &chain(&["list"])), run("list.n"));
    assert_eq!(
        k.resolve(&keys("N"), &chain(&["list", "cell"])),
        run("cell.n")
    );
    let (_, trace) = k.resolve_traced(&keys("N"), &chain(&["list", "cell"]));
    assert_eq!(trace.scope.as_deref(), Some("surface:cell"));
    let verdicts: Vec<(String, Verdict)> = trace
        .candidates
        .iter()
        .map(|c| (c.binding.options.scope.to_string(), c.verdict.clone()))
        .collect();
    assert!(
        matches!(&verdicts[0], (s, Verdict::Lost { because: "nearer the focus", .. }) if s == "global")
    );
    assert!(matches!(&verdicts[2], (s, Verdict::Won) if s == "surface:cell"));
    // Away from the list, its binding says why it did not fire.
    let (_, trace) = k.resolve_traced(&keys("N"), &chain(&[]));
    assert_eq!(trace.scope.as_deref(), Some("global"));
    assert_eq!(
        trace.candidates[1].verdict,
        Verdict::SurfaceNotFocused {
            surface: "list".into()
        }
    );
    assert!(trace.to_string().contains("N: runs global.n (global)"));
}

#[test]
fn a_text_input_keeps_plain_keys_but_not_chorded_or_always_ones() {
    let mut k = layered();
    k.bind(&FW, Layer::Default, "Ctrl+S", Action::run("save"), "")
        .unwrap();
    k.bind(&FW, Layer::Default, "F2", Action::run("rename"), "")
        .unwrap();
    k.bind_with(
        &FW,
        Layer::Default,
        "Tab",
        Action::run("next"),
        "",
        BindOptions::global().typing(Typing::Always),
    )
    .unwrap();
    let typing = chain(&["list"]).flag("textInputFocus", true);
    assert_eq!(k.resolve(&keys("N"), &typing), Resolution::Nothing);
    let (_, trace) = k.resolve_traced(&keys("N"), &typing);
    assert!(
        trace
            .candidates
            .iter()
            .filter(|c| c.binding.options.scope != KeyScope::surface("cell"))
            .all(|c| c.verdict == Verdict::LeftToTyping)
    );
    assert_eq!(k.resolve(&keys("Ctrl+S"), &typing), run("save"));
    assert_eq!(k.resolve(&keys("F2"), &typing), run("rename"));
    assert_eq!(k.resolve(&keys("Tab"), &typing), run("next"));
    assert_eq!(Typing::Auto.effective(&keys("Alt+X")), Typing::Always);
    assert_eq!(
        Typing::Auto.effective(&keys("Shift+X")),
        Typing::UnlessTyping
    );
}

#[test]
fn a_modal_blocks_global_bindings_unless_they_go_through() {
    let mut k = layered();
    k.bind_with(
        &FW,
        Layer::Default,
        "Ctrl+Shift+P",
        Action::run("palette"),
        "",
        BindOptions::global().through_modal(),
    )
    .unwrap();
    k.bind(&FW, Layer::Default, "Ctrl+Q", Action::run("quit"), "")
        .unwrap();
    let modal = chain(&["dialog"]).flag("modalOpen", true);
    assert_eq!(k.resolve(&keys("Ctrl+Q"), &modal), Resolution::Nothing);
    let (_, trace) = k.resolve_traced(&keys("Ctrl+Q"), &modal);
    assert_eq!(trace.candidates[0].verdict, Verdict::BlockedByModal);
    assert_eq!(k.resolve(&keys("Ctrl+Shift+P"), &modal), run("palette"));
    // A surface inside the modal still has its keys.
    let in_list = chain(&["dialog", "list"]).flag("modalOpen", true);
    assert_eq!(k.resolve(&keys("N"), &in_list), run("list.n"));
}

#[test]
fn scopes_survive_a_saved_keymap() {
    let mut k = Keymap::new();
    let user = OwnerId::USER;
    k.bind_with(
        &user,
        Layer::User,
        "Delete",
        Action::run("list.remove"),
        "",
        BindOptions::surface("list").typing(Typing::UnlessTyping),
    )
    .unwrap();
    k.bind_with(
        &user,
        Layer::User,
        "Ctrl+Space",
        Action::run("assist"),
        "",
        BindOptions::global().through_modal(),
    )
    .unwrap();
    let doc = k.export(&user, Layer::User);
    let json = serde_json::to_value(&doc).unwrap();
    assert_eq!(json["bindings"][0]["scope"], "surface:list");
    assert_eq!(json["bindings"][0]["typing"], "unless-typing");
    assert_eq!(json["bindings"][1]["through_modal"], true);
    assert!(json["bindings"][1].get("scope").is_none());
    let back: KeymapDoc = serde_json::from_value(json).unwrap();
    let mut again = Keymap::new();
    assert!(again.load(&user, Layer::User, &back).is_empty());
    assert_eq!(again.export(&user, Layer::User), doc);
    // A scope that does not parse is reported, not guessed.
    let bad: KeymapDoc = serde_json::from_value(serde_json::json!({
        "bindings": [{ "keys": "A", "command": "x", "scope": "panel" }]
    }))
    .unwrap();
    assert_eq!(again.load(&user, Layer::Workspace, &bad).len(), 1);
}

#[test]
fn bindings_of_different_scopes_do_not_conflict() {
    let k = layered();
    assert!(k.conflicts().is_empty());
    let mut k = layered();
    k.bind(&FW, Layer::User, "N", Action::run("other"), "")
        .unwrap();
    assert_eq!(k.conflicts().len(), 1);
}

#[test]
fn a_rebinding_replaces_the_default_keys_and_a_reset_brings_them_back() {
    let mut k = Keymap::new();
    let user = OwnerId::USER;
    let save = CommandId::from_static("doc.save");
    k.bind(&FW, Layer::Default, "Ctrl+S", Action::run("doc.save"), "")
        .unwrap();
    k.bind(
        &FW,
        Layer::Default,
        "Ctrl+Shift+S",
        Action::run("doc.save_as"),
        "",
    )
    .unwrap();
    k.rebind(&user, Layer::User, &save, Some("Ctrl+Alt+S"))
        .unwrap();
    assert_eq!(k.keys_for(&save), [keys("Ctrl+Alt+S")]);
    assert_eq!(k.resolve(&keys("Ctrl+S"), &chain(&[])), Resolution::Nothing);
    assert!(k.rebound(&user, Layer::User, &save));
    // Again: the earlier rebinding goes.
    k.rebind(&user, Layer::User, &save, Some("F2")).unwrap();
    assert_eq!(k.keys_for(&save), [keys("F2")]);
    // Onto keys another command has: a conflict to show.
    k.rebind(&user, Layer::User, &save, Some("Ctrl+Shift+S"))
        .unwrap();
    assert_eq!(k.conflicts().len(), 1);
    assert!(k.rebind(&user, Layer::User, &save, Some("Ctrl+")).is_err());
    // None: no key at all.
    k.rebind(&user, Layer::User, &save, None).unwrap();
    assert!(k.keys_for(&save).is_empty());
    assert_eq!(k.reset(&user, Layer::User, &save), 1);
    assert_eq!(k.keys_for(&save), [keys("Ctrl+S")]);
    // It survives a saved keymap.
    k.rebind(&user, Layer::User, &save, Some("F3")).unwrap();
    let doc = k.export(&user, Layer::User);
    let mut again = Keymap::new();
    again
        .bind(&FW, Layer::Default, "Ctrl+S", Action::run("doc.save"), "")
        .unwrap();
    assert!(again.load(&user, Layer::User, &doc).is_empty());
    assert_eq!(again.keys_for(&save), [keys("F3")]);
}

#[test]
fn system_bindings_fire_in_the_window_and_keep_their_scope_when_rebound() {
    let mut k = Keymap::new();
    k.bind_with(
        &FW,
        Layer::Default,
        "Ctrl+Alt+P",
        Action::run("player.toggle"),
        "",
        BindOptions::system(),
    )
    .unwrap();
    k.bind(&FW, Layer::Default, "Ctrl+S", Action::run("file.save"), "")
        .unwrap();
    assert_eq!(KeyScope::parse("system"), Ok(KeyScope::System));
    assert_eq!(KeyScope::System.to_string(), "system");
    assert!(KeyScope::System.is_global() && KeyScope::System.is_system());
    // Inside the window it is a global binding.
    let cx = WhenContext::new();
    assert_eq!(k.resolve(&keys("Ctrl+Alt+P"), &cx), run("player.toggle"));
    let system = k.system_bindings();
    assert_eq!(system.len(), 1);
    assert_eq!(system[0].keys, keys("Ctrl+Alt+P"));
    // The user's new keys stay in the system scope.
    let user = OwnerId::USER;
    let id = CommandId::from_static("player.toggle");
    k.rebind(&user, Layer::User, &id, Some("Ctrl+Alt+O"))
        .unwrap();
    let system = k.system_bindings();
    assert_eq!(system.len(), 1);
    assert_eq!(system[0].keys, keys("Ctrl+Alt+O"));
    assert_eq!(system[0].layer, Layer::User);
    // Saved with its scope, read back the same.
    let doc = k.export_layer(Layer::User);
    assert!(doc.bindings.iter().any(|b| b.scope == "system"));
    // A global binding of the same keys conflicts with it.
    k.bind(&FW, Layer::Default, "Ctrl+Alt+O", Action::run("other"), "")
        .unwrap();
    assert_eq!(k.conflicts().len(), 1);
    // No keys: nothing to register.
    k.rebind(&user, Layer::User, &id, None).unwrap();
    assert!(k.system_bindings().is_empty());
    let _ = KeymapDoc::default();
}

#[test]
fn bindings_whose_clauses_never_hold_together_do_not_conflict() {
    let mut k = Keymap::new();
    k.bind(
        &FW,
        Layer::Default,
        "Delete",
        Action::run("mask.delete"),
        "view == 'masks'",
    )
    .unwrap();
    k.bind(
        &FW,
        Layer::Default,
        "Delete",
        Action::run("image.reject"),
        "view == 'lighttable'",
    )
    .unwrap();
    assert!(k.conflicts().is_empty());
    // One that may hold with either: a conflict again.
    k.bind(&FW, Layer::User, "Delete", Action::run("other"), "editing")
        .unwrap();
    assert_eq!(k.conflicts().len(), 2);
}

#[test]
fn a_rebound_key_listens_as_the_declared_one_does() {
    let mut k = Keymap::new();
    k.bind_with(
        &FW,
        Layer::Default,
        "Ctrl+Shift+N",
        Action::run("files.new"),
        "",
        BindOptions::global().through_modal(),
    )
    .unwrap();
    k.rebind(
        &OwnerId::USER,
        Layer::User,
        &CommandId::from_static("files.new"),
        Some("Ctrl+Shift+F"),
    )
    .unwrap();
    let modal = chain(&["dialog"]).flag("modalOpen", true);
    assert_eq!(k.resolve(&keys("Ctrl+Shift+F"), &modal), run("files.new"));
    assert_eq!(
        k.resolve(&keys("Ctrl+Shift+N"), &modal),
        Resolution::Nothing
    );
}

#[test]
fn a_rebound_key_keeps_the_declared_clause() {
    let mut k = Keymap::new();
    k.bind(
        &FW,
        Layer::Default,
        "Ctrl+Shift+N",
        Action::run("files.new"),
        "pickerOpen",
    )
    .unwrap();
    k.rebind(
        &OwnerId::USER,
        Layer::User,
        &CommandId::from_static("files.new"),
        Some("Ctrl+Shift+F"),
    )
    .unwrap();
    assert_eq!(
        k.resolve(&keys("Ctrl+Shift+F"), &chain(&[])),
        Resolution::Nothing
    );
    let open = chain(&[]).flag("pickerOpen", true);
    assert_eq!(k.resolve(&keys("Ctrl+Shift+F"), &open), run("files.new"));
    // Several keys at once, as a pointer gesture's triggers.
    k.rebind_all(
        &OwnerId::USER,
        Layer::User,
        &CommandId::from_static("files.new"),
        &["Ctrl+Shift+G"],
    )
    .unwrap();
    assert_eq!(
        k.resolve(&keys("Ctrl+Shift+G"), &chain(&[])),
        Resolution::Nothing
    );
    assert_eq!(k.resolve(&keys("Ctrl+Shift+G"), &open), run("files.new"));
}

/// The context of a window: its `appWindow` and its own `focusedPanel`.
fn in_window(window: &str, panel: &str) -> WhenContext {
    WhenContext::new()
        .with("appWindow", window)
        .with("focusedPanel", panel)
}

/// `Right` bound in each scope of the bubble order.
fn bubbling() -> Keymap {
    let mut k = Keymap::new();
    let bind = |k: &mut Keymap, cmd: &'static str, scope: KeyScope| {
        let options = BindOptions {
            scope,
            ..BindOptions::default()
        };
        k.bind_with(&FW, Layer::Default, "Right", Action::run(cmd), "", options)
            .unwrap();
    };
    bind(&mut k, "app.right", KeyScope::Global);
    bind(&mut k, "main.right", KeyScope::window("main"));
    bind(&mut k, "grid.right", KeyScope::panel("app.grid"));
    bind(&mut k, "cell.right", KeyScope::surface("cell"));
    k
}

#[test]
fn keys_bubble_from_the_surface_to_the_panel_the_window_then_global() {
    let k = bubbling();
    let mut cx = in_window("main", "app.grid");
    cx.flag_at("cellFocus", true, 1);
    assert_eq!(k.resolve(&keys("Right"), &cx), run("cell.right"));
    assert_eq!(
        k.resolve(&keys("Right"), &in_window("main", "app.grid")),
        run("grid.right")
    );
    assert_eq!(
        k.resolve(&keys("Right"), &in_window("main", "app.other")),
        run("main.right")
    );
    assert_eq!(
        k.resolve(&keys("Right"), &in_window("import", "")),
        run("app.right")
    );
}

#[test]
fn a_panel_s_binding_fires_only_while_its_window_has_it_focused() {
    let mut k = Keymap::new();
    k.bind_with(
        &FW,
        Layer::Default,
        "Delete",
        Action::run("grid.remove"),
        "",
        BindOptions {
            scope: KeyScope::panel("app.grid"),
            ..BindOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        k.resolve(&keys("Delete"), &in_window("main", "app.grid")),
        run("grid.remove")
    );
    // Another window: its own focused panel is not the grid.
    let (r, trace) = k.resolve_traced(&keys("Delete"), &in_window("import", ""));
    assert_eq!(r, Resolution::Nothing);
    assert_eq!(
        trace.candidates[0].verdict,
        Verdict::PanelNotFocused {
            panel: "app.grid".into()
        }
    );
    let (r, trace) = {
        let mut w = Keymap::new();
        w.bind_with(
            &FW,
            Layer::Default,
            "Delete",
            Action::run("main.remove"),
            "",
            BindOptions {
                scope: KeyScope::window("main"),
                ..BindOptions::default()
            },
        )
        .unwrap();
        w.resolve_traced(&keys("Delete"), &in_window("import", ""))
    };
    assert_eq!(r, Resolution::Nothing);
    assert_eq!(
        trace.candidates[0].verdict,
        Verdict::OtherWindow {
            window: "main".into()
        }
    );
}

#[test]
fn a_modal_blocks_window_and_panel_bindings_as_global_ones() {
    let k = bubbling();
    let mut cx = in_window("main", "app.grid");
    cx.set("modalOpen", "true");
    assert_eq!(k.resolve(&keys("Right"), &cx), Resolution::Nothing);
}

#[test]
fn a_binding_without_a_scope_takes_its_panel_from_its_clause() {
    assert_eq!(
        KeyScope::infer_from("focusedPanel == 'app.grid' && !busy"),
        KeyScope::panel("app.grid")
    );
    assert_eq!(
        KeyScope::infer_from("focusedPanel == 'app.grid'"),
        KeyScope::panel("app.grid")
    );
    // An alternative of panels, or none, stays global.
    assert_eq!(
        KeyScope::infer_from("focusedPanel == 'a' || focusedPanel == 'b'"),
        KeyScope::Global
    );
    assert_eq!(KeyScope::infer_from("canUndo"), KeyScope::Global);
    assert_eq!(KeyScope::infer_from(""), KeyScope::Global);
    // A keymap written before scopes: its panel binding is the panel's.
    let doc: KeymapDoc = serde_json::from_value(serde_json::json!({
        "bindings": [
            { "keys": "Delete", "command": "grid.remove", "when": "focusedPanel == 'app.grid'" },
            { "keys": "Ctrl+Z", "command": "app.undo" },
            { "keys": "N", "command": "cell.n", "scope": "surface:cell" },
            { "keys": "W", "command": "main.w", "scope": "window:main" },
            { "keys": "G", "command": "any.g", "scope": "global", "when": "canUndo" }
        ]
    }))
    .unwrap();
    let mut k = Keymap::new();
    assert!(k.load(&FW, Layer::User, &doc).is_empty());
    let scope = |cmd: &str| {
        k.all()
            .find(|b| b.action.to_file().as_deref() == Some(cmd))
            .map(|b| b.options.scope.clone())
            .unwrap()
    };
    assert_eq!(scope("grid.remove"), KeyScope::panel("app.grid"));
    assert_eq!(scope("app.undo"), KeyScope::Global);
    assert_eq!(scope("cell.n"), KeyScope::surface("cell"));
    assert_eq!(scope("main.w"), KeyScope::window("main"));
    assert_eq!(scope("any.g"), KeyScope::Global);
    // Saved back as it was read, inferred scopes left out (`global`
    // written out is the same as none).
    let saved = k.export(&FW, Layer::User);
    assert_eq!(saved.bindings[0].scope, "");
    assert_eq!(saved.bindings[3].scope, "window:main");
    assert_eq!(saved.bindings[4].scope, "");
    // The plain `bind` infers too.
    let mut b = Keymap::new();
    b.bind(
        &FW,
        Layer::Default,
        "Delete",
        Action::run("grid.remove"),
        "focusedPanel == 'app.grid'",
    )
    .unwrap();
    assert_eq!(
        b.all().next().unwrap().options.scope,
        KeyScope::panel("app.grid")
    );
}

#[test]
fn scopes_parse_and_print_as_in_files() {
    for text in [
        "global",
        "system",
        "surface:cell",
        "window:main",
        "panel:app.grid",
    ] {
        assert_eq!(KeyScope::parse(text).unwrap().to_string(), text);
    }
    assert!(KeyScope::parse("window:").is_err());
    assert!(KeyScope::parse("pane:x").is_err());
}
