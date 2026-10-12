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
