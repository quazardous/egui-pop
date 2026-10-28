//! The clipboard (ADR 0015): a typed item copied from one widget and
//! pasted into another by the keys, the menu, egui's events and agents;
//! the paste negotiated, transformed, sanitised, one undo step; a cut
//! copying only once its delete is accepted; hooks vetoing and
//! redirecting; text fields keeping egui's own copy and paste.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::clipboard::{
    self, Boundary, ClipItem, ClipSource, ClipTarget, ClipType, ClipboardPlugin, CopyRequest, Cut,
    FormatId, LocalClipboard, Part, Paste, PasteRequest, STEP_COPY, STEP_PASTE, Sanitiser, Slot,
    TEXT, Transform, TransformSpec, Transformed,
};
use egui_pop_api::dialogs::{self, DIALOGS, Dialog};
use egui_pop_api::host::{Dispatch, Host, Journal, Reply, Waker};
use egui_pop_api::steps::{Before, Outcome};
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{
    App, AppBuilder, AppHandle, Ctx, CtxError, DefaultPlugins, Plugin, PluginError,
};
use egui_pop_core::binding::SharedStore;
use egui_pop_core::lock;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{BlockId, OwnerId, PluginId};
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use serde_json::{Value as Json, json};

const COLOR: FormatId = FormatId::from_static("application/x-demo.color+json");
const GRAY: FormatId = FormatId::from_static("application/x-demo.gray+json");

/// A swatch kept in the store at `path`: copied as a colour (text as
/// its fallback), pasted from a colour, cut unless locked.
struct Swatch {
    path: &'static str,
    store: SharedStore,
    locked: Rc<Cell<bool>>,
    accepts: Vec<FormatId>,
}

impl Swatch {
    fn hex(&self) -> String {
        hex(&self.store, self.path)
    }

    fn write(
        &self,
        value: &str,
        origin: &Origin,
        group: Option<egui_pop_model::values::GestureId>,
    ) {
        let mut set = Set::new(
            self.path,
            Change::To(Value::Text(value.into())),
            origin.clone(),
        )
        .by(&OwnerId::USER);
        if let Some(g) = group {
            set = set.gesture(g);
        }
        lock(&self.store).set(set).unwrap();
    }
}

fn hex(store: &SharedStore, path: &str) -> String {
    lock(store)
        .get(path)
        .and_then(|e| e.value.as_text().map(str::to_string))
        .unwrap_or_default()
}

impl ClipSource for Swatch {
    fn copy(&self, _: &AppHandle) -> Option<ClipItem> {
        let hex = self.hex();
        (!hex.is_empty()).then(|| {
            ClipItem::new(
                COLOR,
                2,
                json!({ "hex": hex, "note": "/home/me/secret.txt" }),
                "a colour",
            )
            .fallback(TEXT, json!(format!("{hex} /home/me/secret.txt")))
        })
    }

    fn can_cut(&self, _: &AppHandle) -> bool {
        true
    }

    fn delete(&self, _: &AppHandle, cut: &Cut<'_>) -> Result<(), String> {
        if self.locked.get() {
            return Err("the swatch is locked".into());
        }
        self.write("", cut.origin, cut.group);
        Ok(())
    }
}

impl ClipTarget for Swatch {
    fn accepts(&self, _: &AppHandle) -> Vec<FormatId> {
        self.accepts.clone()
    }

    fn paste(&self, _: &AppHandle, p: &Paste<'_>) -> Result<Json, String> {
        let hex = match p.repr.format.as_str() {
            "text/plain" => p.repr.text().unwrap_or("").to_string(),
            _ => p.repr.data["hex"].as_str().unwrap_or("").to_string(),
        };
        let hex = if p.wants("hex") { hex } else { String::new() };
        if !p.dry_run {
            self.write(&hex, p.origin, p.group);
        }
        Ok(json!(format!("{} becomes {hex}", self.path)))
    }
}

/// The demo: swatches A, B and a grey one, a name field; the colour
/// type (version 2, migrating version 1), parsing text into a colour,
/// a colour to grey (lossy), a sanitiser of local paths.
struct Demo {
    locked: Rc<Cell<bool>>,
}

const A: &str = "demo/a";
const B: &str = "demo/b";
const G: &str = "demo/g";

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![clipboard::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let app = ctx.app();
        let store = app.store();
        for (path, value) in [(A, "#d9a441"), (B, "#000000"), (G, "")] {
            lock(&store)
                .declare(&OwnerId::USER, path, Descriptor::text(path, value))
                .unwrap();
        }
        let swatch = |path, accepts: Vec<FormatId>| {
            Rc::new(Swatch {
                path,
                store: store.clone(),
                locked: self.locked.clone(),
                accepts,
            })
        };
        let (a, b, g) = (
            swatch(A, vec![COLOR, TEXT]),
            swatch(B, vec![COLOR, TEXT]),
            swatch(G, vec![GRAY]),
        );
        ctx.clip_endpoint("a", Some(a.clone()), Some(a))
            .clip_endpoint("b", Some(b.clone()), Some(b))
            .clip_endpoint("g", None, Some(g));
        ctx.clip_type(
            ClipType::new(COLOR, 2, "a colour")
                .parts(|_| vec![Part::new("hex", "the hex code")])
                .migrate(|from, old| match from {
                    1 => Ok(json!({ "hex": old["rgb"] })),
                    v => Err(format!("no colour of version {v}")),
                }),
        )?
        .clip_transform(Transform::new(
            TransformSpec::new("demo.parse", TEXT, COLOR),
            |r| {
                let t = r.text().unwrap_or("").trim();
                if t.len() == 7 && t.starts_with('#') {
                    Ok(Transformed::exact(json!({ "hex": t })))
                } else {
                    Err(format!("`{t}` is not a colour"))
                }
            },
        ))?
        .clip_transform(Transform::new(
            TransformSpec::new("demo.to-gray", COLOR, GRAY).lossy(),
            |r| {
                Ok(Transformed::lossy(
                    json!({ "hex": r.data["hex"] }),
                    "the hue",
                ))
            },
        ))?
        .clip_sanitiser(
            Sanitiser::new("demo.paths", Boundary::System, |data| {
                let Some(t) = data.as_str() else {
                    return Vec::new();
                };
                if !t.contains("/home/") {
                    return Vec::new();
                }
                let kept: Vec<&str> = t.split(' ').filter(|w| !w.starts_with("/home/")).collect();
                *data = json!(kept.join(" "));
                vec!["a local path".into()]
            })
            .format(TEXT),
        )?;
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("demo.swatches"), 0, |ui, app| {
                let a = ui.button("Swatch A");
                clipboard::bind(app, "a", a.id);
                let b = ui.button("Swatch B");
                clipboard::bind(app, "b", b.id);
                let g = ui.button("Grey swatch");
                clipboard::bind(app, "g", g.id);
                let mut name = ui
                    .data(|d| d.get_temp::<String>(egui::Id::new("demo.name")))
                    .unwrap_or_default();
                ui.add(egui::TextEdit::singleline(&mut name).hint_text("Name"))
                    .labelled_by(ui.label("Name").id);
                ui.data_mut(|d| d.insert_temp(egui::Id::new("demo.name"), name));
            }),
        );
        Ok(())
    }
}

fn builder() -> (AppBuilder, Rc<Cell<bool>>) {
    let locked = Rc::new(Cell::new(false));
    let b = App::builder()
        .plugins(DefaultPlugins)
        .plugin(ClipboardPlugin)
        .plugin(Demo {
            locked: locked.clone(),
        });
    (b, locked)
}

fn harness(builder: AppBuilder) -> Harness<'static, App> {
    let app = builder.build(&egui::Context::default()).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 500.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn store_hex(app: &AppHandle, path: &str) -> String {
    hex(&app.store(), path)
}

fn key(app: &AppHandle, k: &str) -> String {
    app.when_context().get(k).to_string()
}

fn user_item(app: &AppHandle) -> Option<ClipItem> {
    clipboard::with_owner(app, |c| c.get(&Slot::User)).flatten()
}

fn rev(app: &AppHandle) -> u64 {
    clipboard::with_owner(app, |c| c.rev()).unwrap()
}

#[test]
fn ctrl_c_and_ctrl_v_copy_a_typed_item_between_two_widgets() {
    let (b, _) = builder();
    let mut h = harness(b);
    let app = h.state().handle().clone();
    h.get_by_label("Swatch A").focus();
    h.run();
    assert_eq!(clipboard::focused(&app).as_deref(), Some("a"));
    h.key_press_modifiers(Modifiers::COMMAND, Key::C);
    h.run();
    let item = user_item(&app).expect("copied");
    assert_eq!(item.kind, COLOR);
    assert_eq!(item.payload["hex"], "#d9a441");
    assert_eq!(item.origin.source, "a");
    // The fallback leaves the owner without its local path; the item
    // inside keeps everything and records what went.
    assert_eq!(item.payload["note"], "/home/me/secret.txt");
    assert_eq!(item.stripped.len(), 1);
    assert_eq!(item.stripped[0].what, "a local path");
    let copy = app
        .step_traces()
        .into_iter()
        .rev()
        .find(|t| t.step == STEP_COPY)
        .unwrap();
    assert_eq!(copy.result.unwrap().unwrap()["system"], true);
    h.get_by_label("Swatch B").focus();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::V);
    h.run();
    assert_eq!(store_hex(&app, B), "#d9a441");
    // The journal says what changed, never the payload.
    let changed = app
        .events_since(0)
        .into_iter()
        .find(|e| e.name == "clipboard.changed")
        .unwrap();
    assert_eq!(changed.data["kind"], COLOR.as_str());
    assert!(changed.data.get("payload").is_none());
    assert!(app.diagnostics().is_empty(), "{:?}", app.diagnostics());
}

#[test]
fn the_paste_key_pastes_once_a_pass() {
    // As under eframe: its `logic` hook, then `App::show`, which calls
    // the logic again; and between passes, the logic ticked alone (a
    // hidden window) over the last pass's input.
    let (b, _) = builder();
    let app = b.build(&egui::Context::default()).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 500.0))
        .build_ui_state(
            |ui, app: &mut App| {
                app.logic(ui.ctx());
                app.show(ui);
            },
            app,
        );
    h.run();
    let app = h.state().handle().clone();
    let pastes = |app: &AppHandle| {
        app.step_traces()
            .into_iter()
            .filter(|t| t.step == STEP_PASTE)
            .count()
    };
    // The paste key's events one pass each, as egui-winit sends them;
    // right after the pass holding its release, the logic ticks alone
    // over that pass's input.
    let paste_key = |h: &mut Harness<'_, App>, press: bool| {
        let key = |pressed| egui::Event::Key {
            key: Key::V,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        };
        h.event(egui::Event::ModifiersChanged(Modifiers::COMMAND));
        h.step();
        if press {
            h.event(key(true));
            h.step();
        }
        h.event(key(false));
        h.step();
        let ctx = h.ctx.clone();
        for _ in 0..3 {
            h.state_mut().logic(&ctx);
        }
        h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
        h.run();
    };
    h.get_by_label("Swatch A").focus();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::C);
    h.run();
    h.get_by_label("Swatch B").focus();
    h.run();
    // The paste key as a key: its press, then its release.
    paste_key(&mut h, true);
    assert_eq!(store_hex(&app, B), "#d9a441");
    assert_eq!(pastes(&app), 1, "the release is no second paste");
    // As egui-winit sends it with text on the system clipboard: the
    // paste event, then the key's release alone.
    h.event(egui::Event::Paste("#102030".into()));
    h.run();
    paste_key(&mut h, false);
    assert_eq!(store_hex(&app, B), "#102030");
    assert_eq!(pastes(&app), 2, "the release is no second paste");
}

#[test]
fn the_edit_commands_sit_in_the_edit_menu_greyed_when_nothing_focused_answers() {
    let (b, _) = builder();
    let mut h = harness(b);
    let app = h.state().handle().clone();
    let items = app.menu(&egui_pop_api::app::MENU_EDIT);
    for id in ["edit.cut", "edit.copy", "edit.paste", "edit.pasteSpecial"] {
        assert!(
            items
                .iter()
                .any(|i| format!("{:?}", i.item).contains(id) && i.group == clipboard::MENU_GROUP),
            "{id} in {items:?}"
        );
    }
    // Nothing focused: nothing applies.
    assert_eq!(key(&app, "canCopy"), "false");
    assert_eq!(key(&app, "canPaste"), "false");
    // The grey swatch takes greys only: a colour fits through the
    // transform, so it can paste; it cannot be copied from.
    h.get_by_label("Grey swatch").focus();
    h.run();
    assert_eq!(key(&app, "canCopy"), "false");
    assert!(clipboard::copy(&app, CopyRequest::new("a"), &Origin::User).ok());
    assert_eq!(key(&app, "canPaste"), "true");
    // Swatch A: copy and cut.
    h.get_by_label("Swatch A").focus();
    h.run();
    assert_eq!(key(&app, "canCopy"), "true");
    assert_eq!(key(&app, "canCut"), "true");
    assert_eq!(
        app.keys_for(&clipboard::CMD_PASTE_SPECIAL),
        ["Ctrl+Shift+V"]
    );
}

#[test]
fn egui_s_copy_and_paste_events_reach_the_commands_and_external_text_is_parsed() {
    let (b, _) = builder();
    let mut h = harness(b);
    let app = h.state().handle().clone();
    // Text from another application, with an escape sequence.
    h.get_by_label("Swatch B").focus();
    h.run();
    h.event(egui::Event::Paste("\u{1b}[31m#102030".into()));
    h.run();
    assert_eq!(store_hex(&app, B), "#102030");
    let paste = app
        .step_traces()
        .into_iter()
        .rev()
        .find(|t| t.step == STEP_PASTE)
        .unwrap();
    let r = paste.result.unwrap().unwrap();
    assert_eq!(r["internal"], false);
    assert_eq!(r["transform"], "demo.parse");
    assert_eq!(r["stripped"][0]["by"], clipboard::SANITISE_CONTROLS);
    // egui-winit's copy event on A, then a paste bringing back the text
    // written: the owner's own item, pasted as a colour.
    h.get_by_label("Swatch A").focus();
    h.run();
    h.event(egui::Event::Copy);
    h.run();
    assert_eq!(user_item(&app).unwrap().payload["hex"], "#d9a441");
    h.get_by_label("Swatch B").focus();
    h.run();
    h.event(egui::Event::Paste("#d9a441".into()));
    h.run();
    let r = app
        .step_traces()
        .into_iter()
        .rev()
        .find(|t| t.step == STEP_PASTE)
        .unwrap()
        .result
        .unwrap()
        .unwrap();
    assert_eq!(r["internal"], true, "{r}");
    assert_eq!(r["format"], COLOR.as_str());
    assert_eq!(store_hex(&app, B), "#d9a441");
}

#[test]
fn text_fields_keep_egui_s_own_copy_and_paste() {
    let (b, _) = builder();
    let mut h = harness(b);
    let app = h.state().handle().clone();
    h.query_all_by_label("Name")
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::TextInput)
        .expect("the name field")
        .focus();
    h.run();
    assert_eq!(key(&app, "textInputFocus"), "true");
    let before = rev(&app);
    h.event(egui::Event::Paste("Ada".into()));
    h.run();
    assert_eq!(rev(&app), before);
    let name = h
        .ctx
        .data(|d| d.get_temp::<String>(egui::Id::new("demo.name")));
    assert_eq!(name.as_deref(), Some("Ada"));
}

#[test]
fn a_paste_is_one_undo_step_and_a_cut_copies_only_once_its_delete_is_accepted() {
    let (b, locked) = builder();
    let mut h = harness(b);
    let app = h.state().handle().clone();
    assert!(clipboard::copy(&app, CopyRequest::new("a"), &Origin::User).ok());
    assert!(clipboard::paste(&app, PasteRequest::new("b"), &Origin::User).ok());
    assert_eq!(store_hex(&app, B), "#d9a441");
    assert_eq!(app.undo_label().as_deref(), Some("Paste a colour"));
    assert!(app.undo(&Origin::User).ok());
    assert_eq!(store_hex(&app, B), "#000000");
    // A locked swatch refuses its delete: nothing is cut, nothing copied.
    locked.set(true);
    let before = rev(&app);
    let out = clipboard::copy(&app, CopyRequest::new("a").cut(), &Origin::User);
    assert!(
        matches!(&out, Outcome::Done(Err(e)) if e.contains("locked")),
        "{out:?}"
    );
    assert_eq!(rev(&app), before);
    assert_eq!(store_hex(&app, A), "#d9a441");
    h.run();
    let toasts = app.service(&egui_pop_api::toasts::TOASTS).unwrap();
    assert!(
        toasts
            .shown()
            .iter()
            .any(|t| t.toast.text.contains("locked")),
        "{:?}",
        toasts.shown()
    );
    // Unlocked: copied and deleted, one step undone together.
    locked.set(false);
    assert!(clipboard::copy(&app, CopyRequest::new("a").cut(), &Origin::User).ok());
    assert_eq!(rev(&app), before + 1);
    assert_eq!(store_hex(&app, A), "");
    assert_eq!(app.undo_label().as_deref(), Some("Cut a colour"));
    assert!(app.undo(&Origin::User).ok());
    assert_eq!(store_hex(&app, A), "#d9a441");
    // A paste the target refuses leaves no step.
    let out = clipboard::paste(&app, PasteRequest::new("g").format(TEXT), &Origin::User);
    assert!(out.result().is_some_and(Result::is_err), "{out:?}");
}

#[test]
fn hooks_veto_a_copy_and_redirect_a_paste() {
    let (b, _) = builder();
    let b = b.plugin(Hooks);
    let h = harness(b);
    let app = h.state().handle().clone();
    // Copying from B is a secret.
    let out = clipboard::copy(&app, CopyRequest::new("b"), &Origin::User);
    assert_eq!(out.vetoed(), Some("B is a secret"));
    assert!(user_item(&app).is_none());
    // A paste aimed at A lands in B.
    assert!(clipboard::copy(&app, CopyRequest::new("a"), &Origin::User).ok());
    clipboard::with_owner(&app, |c| {
        let mut item = c.get(&Slot::User).unwrap();
        item.payload["hex"] = json!("#abcdef");
        c.put(&Slot::User, item, &Origin::User)
    });
    assert!(clipboard::paste(&app, PasteRequest::new("a"), &Origin::User).ok());
    assert_eq!(store_hex(&app, A), "#d9a441");
    assert_eq!(store_hex(&app, B), "#abcdef");
}

struct Hooks;

impl Plugin for Hooks {
    fn id(&self) -> PluginId {
        PluginId::from_static("hooks")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![clipboard::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_COPY, |_, args| {
            if args["source"] == "b" {
                Before::Veto("B is a secret".into())
            } else {
                Before::Go
            }
        })
        .before(STEP_PASTE, |_, args| {
            if args["target"] == "a" {
                let mut to = args.clone();
                to["target"] = json!("b");
                Before::Redirect(to)
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

#[test]
fn paste_special_lists_every_way_with_its_preview_and_pastes_the_choice() {
    let (b, _) = builder();
    let mut h = harness(b);
    let app = h.state().handle().clone();
    assert!(clipboard::copy(&app, CopyRequest::new("a"), &Origin::User).ok());
    let ways = clipboard::ways(&app, "b", None, None, &Origin::User).unwrap();
    let labels: Vec<&str> = ways.iter().map(|w| w.label.as_str()).collect();
    assert_eq!(labels, ["As a colour", "As text", "Only the hex code"]);
    assert_eq!(ways[0].preview.as_deref(), Some("demo/b becomes #d9a441"));
    // The grey swatch: through the lossy transform, which says so.
    let ways = clipboard::ways(&app, "g", None, None, &Origin::User).unwrap();
    assert_eq!(ways[0].plan.transform.as_deref(), Some("demo.to-gray"));
    assert!(ways[0].label.contains("loses the hue"), "{}", ways[0].label);
    // Ctrl+Shift+V on B opens the choice; choosing text pastes the
    // fallback as it left the owner.
    h.get_by_label("Swatch B").focus();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::V);
    h.run();
    let dialogs = app.service(&DIALOGS).unwrap();
    let Some((_, Dialog::Choose(c))) = dialogs.current() else {
        panic!("paste special is open");
    };
    assert_eq!(c.options.len(), 3);
    assert_eq!(
        c.options[1].hint,
        "demo/b becomes #d9a441 /home/me/secret.txt"
    );
    dialogs
        .answer(dialogs::Answer::Choose { index: 2 }, Origin::User)
        .unwrap();
    h.run();
    assert_eq!(store_hex(&app, B), "#d9a441");
    // A lossy paste is applied, and a toast names the loss.
    assert!(clipboard::paste(&app, PasteRequest::new("g"), &Origin::User).ok());
    h.run();
    let toasts = app.service(&egui_pop_api::toasts::TOASTS).unwrap();
    assert!(
        toasts
            .shown()
            .iter()
            .any(|t| t.toast.text.contains("the hue")),
        "{:?}",
        toasts.shown()
    );
}

#[test]
fn an_older_item_is_migrated_before_it_is_pasted() {
    let (b, _) = builder();
    let h = harness(b);
    let app = h.state().handle().clone();
    clipboard::with_owner(&app, |c| {
        c.put(
            &Slot::User,
            ClipItem::new(COLOR, 1, json!({ "rgb": "#0a0b0c" }), "a colour"),
            &Origin::User,
        )
    });
    assert!(clipboard::paste(&app, PasteRequest::new("b"), &Origin::User).ok());
    assert_eq!(store_hex(&app, B), "#0a0b0c");
}

#[test]
fn agents_copy_and_paste_in_their_own_slot_with_a_dry_run() {
    let (b, _) = builder();
    let mut app = b.build(&egui::Context::default()).unwrap();
    let h = app.handle().clone();
    let agent = Origin::Agent("claude".into());
    let run = |app: &mut App, cmd, args: Json, origin: &Origin| {
        app.run(&cmd, args, origin.clone()).now().unwrap()
    };
    // The user copies A; the agent copies B into its own slot.
    assert!(clipboard::copy(&h, CopyRequest::new("a"), &Origin::User).ok());
    run(
        &mut app,
        clipboard::CMD_CLIP_COPY,
        json!({ "source": "b" }),
        &agent,
    )
    .unwrap();
    assert_eq!(user_item(&h).unwrap().payload["hex"], "#d9a441");
    let own = run(&mut app, clipboard::CMD_READ, json!({}), &agent).unwrap();
    assert_eq!(own["slot"], "agent:claude");
    assert_eq!(own["item"]["payload"]["hex"], "#000000");
    // The user's slot is refused to it.
    let refused = run(
        &mut app,
        clipboard::CMD_READ,
        json!({ "slot": "user" }),
        &agent,
    );
    assert!(refused.unwrap_err().contains("may not use"));
    // A dry run: the plan and the changes, nothing changed.
    let rev = rev(&h);
    let plan = run(
        &mut app,
        clipboard::CMD_CLIP_PASTE,
        json!({ "target": "g", "dry_run": true }),
        &agent,
    )
    .unwrap();
    assert_eq!(plan["transform"], "demo.to-gray");
    assert_eq!(plan["changes"], "demo/g becomes #000000");
    assert_eq!(store_hex(&h, G), "");
    // Checked against the revision.
    let stale = run(
        &mut app,
        clipboard::CMD_CLIP_PASTE,
        json!({ "target": "g", "if_rev": rev + 7 }),
        &agent,
    );
    assert!(stale.unwrap_err().contains("revision"));
    run(
        &mut app,
        clipboard::CMD_CLIP_PASTE,
        json!({ "target": "g", "if_rev": rev }),
        &agent,
    )
    .unwrap();
    assert_eq!(store_hex(&h, G), "#000000");
    // Offers: what the slot holds and every way it fits.
    let offers = run(
        &mut app,
        clipboard::CMD_OFFERS,
        json!({ "target": "b" }),
        &agent,
    )
    .unwrap();
    assert_eq!(offers["item"]["kind"], COLOR.as_str());
    assert!(
        offers["ways"].as_array().is_some_and(|w| !w.is_empty()),
        "{offers}"
    );
    // Written by the agent, typed by its format.
    run(
        &mut app,
        clipboard::CMD_WRITE,
        json!({ "format": COLOR.as_str(), "data": { "hex": "#111111" } }),
        &agent,
    )
    .unwrap();
    let own = run(&mut app, clipboard::CMD_READ, json!({}), &agent).unwrap();
    assert_eq!(own["item"]["title"], "a colour");
    assert_eq!(own["item"]["version"], 2);
    // Allowed by the host: it reads the user's.
    clipboard::allow_session(&h, "agent:claude");
    let user = run(
        &mut app,
        clipboard::CMD_READ,
        json!({ "slot": "user" }),
        &agent,
    )
    .unwrap();
    assert_eq!(user["item"]["payload"]["hex"], "#d9a441");
}

/// A kernel owning the clipboard.
#[derive(Default)]
struct Kernel {
    journal: Journal,
    store: SharedStore,
    clipboard: LocalClipboard,
    puts: Rc<RefCell<u32>>,
}

impl Host for Kernel {
    fn dispatch(&mut self, d: &Dispatch<'_>) -> Reply {
        Reply::Done(d.run_local().unwrap_or(Err("no handler".into())))
    }
    fn journal(&self) -> Option<&Journal> {
        Some(&self.journal)
    }
    fn journal_mut(&mut self) -> Option<&mut Journal> {
        Some(&mut self.journal)
    }
    fn store(&self) -> SharedStore {
        self.store.clone()
    }
    fn set_waker(&mut self, _: Waker) {}
    fn clipboard(&mut self) -> Option<&mut dyn clipboard::Clipboard> {
        *self.puts.borrow_mut() += 1;
        Some(&mut self.clipboard)
    }
}

#[test]
fn a_host_may_own_the_clipboard() {
    let kernel = Kernel::default();
    let asked = kernel.puts.clone();
    let locked = Rc::new(Cell::new(false));
    let app = App::builder()
        .host(kernel)
        .plugins(DefaultPlugins)
        .plugin(ClipboardPlugin)
        .plugin(Demo { locked })
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle();
    let before = *asked.borrow();
    assert!(clipboard::copy(h, CopyRequest::new("a"), &Origin::User).ok());
    assert!(*asked.borrow() > before, "the host's owner was asked");
    assert!(clipboard::paste(h, PasteRequest::new("b"), &Origin::User).ok());
    assert_eq!(store_hex(h, B), "#d9a441");
    assert_eq!(rev(h), 1);
}

#[test]
fn a_type_is_declared_once_and_goes_with_its_plugin() {
    struct Twice;
    impl Plugin for Twice {
        fn id(&self) -> PluginId {
            PluginId::from_static("twice")
        }
        fn requires(&self) -> Vec<PluginId> {
            vec![PluginId::from_static("demo")]
        }
        fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
            let e = ctx
                .clip_type(ClipType::new(COLOR, 1, "a colour"))
                .unwrap_err();
            assert!(
                matches!(e, CtxError::Clipboard(ref m) if m.contains("`demo`")),
                "{e}"
            );
            Ok(())
        }
    }
    let (b, _) = builder();
    let mut h = harness(b.plugin(Twice));
    let app = h.state().handle().clone();
    assert!(clipboard::clip_type(&app, &COLOR).is_some());
    app.set_plugin_enabled(PluginId::from_static("twice"), false);
    app.set_plugin_enabled(PluginId::from_static("demo"), false);
    h.run();
    h.run();
    assert!(clipboard::clip_type(&app, &COLOR).is_none());
    assert!(!clipboard::endpoints(&app).contains(&"a".to_string()));
}

/// A system clipboard in memory: what the last copy wrote there, and
/// what a paste reads (`None`: it cannot read).
#[derive(Clone, Default)]
struct FakeSystem {
    written: Rc<RefCell<Vec<clipboard::Repr>>>,
    held: Rc<RefCell<Option<Vec<clipboard::Repr>>>>,
}

impl clipboard::SystemBackend for FakeSystem {
    fn name(&self) -> String {
        "fake".into()
    }

    fn writes(&self, format: &FormatId) -> bool {
        *format == TEXT || *format == COLOR
    }

    fn write(&mut self, _: &egui::Context, reprs: &[clipboard::Repr]) -> Result<(), String> {
        *self.written.borrow_mut() = reprs.to_vec();
        *self.held.borrow_mut() = Some(reprs.to_vec());
        Ok(())
    }

    fn read(&mut self, _: &egui::Context) -> Option<Vec<clipboard::Repr>> {
        self.held.borrow().clone()
    }

    fn readable(&self) -> Vec<FormatId> {
        vec![clipboard::URI_LIST]
    }
}

fn done(out: Outcome) -> Result<Json, String> {
    match out {
        Outcome::Done(r) => r,
        Outcome::Vetoed { reason, .. } => Err(reason),
    }
}

#[test]
fn a_system_backend_gets_every_format_it_writes_and_gives_back_what_others_left() {
    let (b, _) = builder();
    let h = harness(b);
    let app = h.state().handle().clone();
    let system = FakeSystem::default();
    clipboard::set_backend(&app, Box::new(system.clone()));
    done(clipboard::copy(&app, CopyRequest::new("a"), &Origin::User)).unwrap();
    // The private type crosses where the backend writes it, the text
    // sanitised for the system boundary.
    let written = system.written.borrow().clone();
    assert_eq!(written[0].format, COLOR);
    let text = written.iter().find(|r| r.format == TEXT).unwrap();
    assert_eq!(text.data, json!("#d9a441"));
    // Still there: the owner's item is pasted.
    let v = done(clipboard::paste(
        &app,
        PasteRequest::new("b"),
        &Origin::User,
    ))
    .unwrap();
    assert_eq!(v["internal"], true);
    assert_eq!(store_hex(&app, B), "#d9a441");
    // Another application's files and their text: one external item,
    // negotiated like any other.
    *system.held.borrow_mut() = Some(vec![
        clipboard::Repr::new(clipboard::URI_LIST, json!("file:///tmp/%23123456")),
        clipboard::Repr::new(TEXT, json!("#123456")),
    ]);
    let v = done(clipboard::paste(
        &app,
        PasteRequest::new("b"),
        &Origin::User,
    ))
    .unwrap();
    assert_eq!(v["internal"], false);
    assert_eq!(v["kind"], "text/uri-list");
    assert_eq!(v["format"], "text/plain");
    assert_eq!(store_hex(&app, B), "#123456");
}

#[test]
fn the_paste_key_with_no_text_is_refused_when_nothing_reads_the_rest() {
    let (b, _) = builder();
    let h = harness(b);
    let app = h.state().handle().clone();
    done(clipboard::copy(&app, CopyRequest::new("a"), &Origin::User)).unwrap();
    let req = PasteRequest {
        no_text: true,
        ..PasteRequest::new("b")
    };
    let e = done(clipboard::paste(&app, req, &Origin::User)).unwrap_err();
    assert!(e.contains("no text"), "{e}");
    assert_eq!(store_hex(&app, B), "#000000");
}

const COUNT: FormatId = FormatId::from_static("application/x-demo.count+json");

/// A target taking counts, saying its limit, keeping what it was given.
#[derive(Default)]
struct Counter {
    got: RefCell<Option<(Json, Option<String>)>>,
    later: RefCell<Option<egui_pop_api::host::Resolver>>,
    deferred: bool,
}

impl ClipTarget for Counter {
    fn accepts(&self, _: &AppHandle) -> Vec<FormatId> {
        vec![COUNT]
    }

    fn paste(&self, _: &AppHandle, p: &Paste<'_>) -> Result<Json, String> {
        if !p.dry_run {
            *self.got.borrow_mut() = Some((p.repr.data.clone(), p.element.map(str::to_string)));
        }
        Ok(json!("counts"))
    }

    fn apply(&self, app: &AppHandle, p: &Paste<'_>) -> clipboard::Pasted {
        if !self.deferred {
            return clipboard::Pasted::Done(self.paste(app, p));
        }
        let (pending, resolver) = egui_pop_api::host::pending();
        *self.later.borrow_mut() = Some(resolver);
        clipboard::Pasted::Later(pending)
    }

    fn context(&self, _: &AppHandle, element: Option<&str>) -> Json {
        json!({ "max": 3, "element": element })
    }
}

/// Declares text to counts, clamped to the target's limit.
struct Counting {
    counter: Rc<Counter>,
}

impl Plugin for Counting {
    fn id(&self) -> PluginId {
        PluginId::from_static("counting")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![clipboard::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.clip_endpoint("count", None, Some(self.counter.clone()));
        ctx.clip_transform(Transform::for_target(
            TransformSpec::new("counting.length", TEXT, COUNT).lossy(),
            |r, d| {
                let n = r.text().unwrap_or("").chars().count() as u64;
                let max = d.context["max"].as_u64().unwrap_or(u64::MAX);
                Ok(Transformed::lossy(
                    json!({ "n": n.min(max), "in": d.target, "at": d.context["element"] }),
                    "the count above the limit",
                ))
            },
        ))?;
        Ok(())
    }
}

fn put_text(app: &AppHandle, text: &str) {
    clipboard::with_owner(app, |c| {
        c.put(
            &Slot::User,
            ClipItem::new(TEXT, 0, json!(text), "text"),
            &Origin::User,
        )
    })
    .unwrap();
}

#[test]
fn a_transform_sees_the_target_and_its_element() {
    let counter = Rc::new(Counter::default());
    let (b, _) = builder();
    let h = harness(b.plugin(Counting {
        counter: counter.clone(),
    }));
    let app = h.state().handle().clone();
    put_text(&app, "hello");
    let v = done(clipboard::paste(
        &app,
        PasteRequest::new("count").element("row-2"),
        &Origin::User,
    ))
    .unwrap();
    assert_eq!(v["lost"], "the count above the limit");
    assert_eq!(v["element"], "row-2");
    let (data, element) = counter.got.borrow().clone().unwrap();
    assert_eq!(data, json!({ "n": 3, "in": "count", "at": "row-2" }));
    assert_eq!(element.as_deref(), Some("row-2"));
}

#[test]
fn a_deferred_paste_answers_pending_then_publishes_its_result() {
    let counter = Rc::new(Counter {
        deferred: true,
        ..Counter::default()
    });
    let (b, _) = builder();
    let mut h = harness(b.plugin(Counting {
        counter: counter.clone(),
    }));
    let app = h.state().handle().clone();
    put_text(&app, "hi");
    let v = done(clipboard::paste(
        &app,
        PasteRequest::new("count"),
        &Origin::User,
    ))
    .unwrap();
    assert_eq!(v["pending"], true);
    assert_eq!(clipboard::pending_pastes(&app), 1);
    counter
        .later
        .borrow_mut()
        .take()
        .unwrap()
        .resolve(Ok(json!("two counted")));
    h.run();
    assert_eq!(clipboard::pending_pastes(&app), 0);
    let pasted = app
        .events_since(0)
        .into_iter()
        .find(|e| e.name == "clipboard.pasted")
        .expect("the result is published");
    assert_eq!(pasted.data["changes"], "two counted");
    assert_eq!(pasted.data["pending"], false);
}
