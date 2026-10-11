//! The egui layer, headless (no renderer needed): focus scopes and key
//! dispatch, overlays, fault boundaries, binding, persistence, strings,
//! accessibility.

use std::sync::{Arc, Mutex};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::binding::{self, Binding};
use egui_pop_core::keys::{KeyDispatcher, chord_of, key_of};
use egui_pop_core::overlay::{self, Anchor, Kind, OverlaySpec};
use egui_pop_core::persist::{Persist, Source};
use egui_pop_core::placement::Side;
use egui_pop_core::{a11y, fault, focus, strings};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::OwnerId;
use egui_pop_model::keymap::{Action, Keymap, Layer, Verdict};
use egui_pop_model::layout::{self, Layout};
use egui_pop_model::values::{Descriptor, Store, Value};
use egui_pop_model::when::WhenContext;

struct Keys {
    keymap: Keymap,
    dispatcher: KeyDispatcher,
    ran: Vec<String>,
    text: String,
}

fn keys_app() -> Harness<'static, Keys> {
    let mut keymap = Keymap::new();
    let fw = OwnerId::FRAMEWORK;
    for (keys, cmd, when) in [
        ("Escape", "app.cancel", ""),
        ("Escape", "outer.close", "outerFocus"),
        ("Escape", "inner.clear", "innerFocus"),
        ("Ctrl+K Ctrl+S", "keys.editor", ""),
        ("3", "rate.3", "!textInputFocus"),
    ] {
        keymap
            .bind(
                &fw,
                Layer::Default,
                keys,
                Action::Run(cmd.parse().unwrap()),
                when,
            )
            .unwrap();
    }
    Harness::new_ui_state(
        |ui, s: &mut Keys| {
            let cx = focus::when_context(ui.ctx(), &WhenContext::new());
            for d in s.dispatcher.process(ui.ctx(), &s.keymap, &cx) {
                s.ran.push(d.command.to_string());
            }
            focus::scope(ui, "outer", &[], |ui| {
                let _ = ui.button("Outer button");
                focus::scope(ui, "inner", &[("mode", "edit")], |ui| {
                    ui.text_edit_singleline(&mut s.text);
                    let _ = ui.button("Inner button");
                });
            });
        },
        Keys {
            keymap,
            dispatcher: KeyDispatcher::new(),
            ran: Vec::new(),
            text: String::new(),
        },
    )
}

#[test]
fn the_binding_nearest_the_focused_widget_wins() {
    let mut h = keys_app();
    h.run();
    h.press_key(Key::Escape);
    h.run();
    assert_eq!(h.state().ran, ["app.cancel"], "nothing focused");
    h.get_by_label("Inner button").focus();
    h.run();
    assert_eq!(focus::chain_names(&h.ctx), ["outer", "inner"]);
    h.key_press(Key::Escape);
    h.run();
    h.get_by_label("Outer button").focus();
    h.run();
    assert_eq!(focus::chain_names(&h.ctx), ["outer"]);
    h.key_press(Key::Escape);
    h.run();
    assert_eq!(h.state().ran, ["app.cancel", "inner.clear", "outer.close"]);
    let trace = h.state().dispatcher.traces().last().unwrap().clone();
    assert!(
        trace
            .candidates
            .iter()
            .any(|c| c.verdict == Verdict::WhenFalse)
    );
    // egui lets go of the focus on Escape: focus the button again.
    h.get_by_label("Outer button").focus();
    h.run();
    let cx = focus::when_context(&h.ctx, &WhenContext::new());
    assert_eq!(cx.depth_of("outerFocus"), Some(1));
    assert_eq!(cx.depth_of("widgetFocus"), Some(2));
}

trait PressKey {
    fn press_key(&mut self, key: Key);
}

impl PressKey for Harness<'_, Keys> {
    fn press_key(&mut self, key: Key) {
        self.key_press(key);
    }
}

#[test]
fn chords_wait_and_typing_keeps_plain_keys() {
    let mut h = keys_app();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::K);
    h.run();
    assert!(h.state().dispatcher.pending().is_some());
    assert!(h.state().ran.is_empty());
    h.key_press_modifiers(Modifiers::COMMAND, Key::S);
    h.run();
    assert_eq!(h.state().ran, ["keys.editor"]);
    assert!(h.state().dispatcher.pending().is_none());
    // With a button focused, `3` runs; in the text field, it does not.
    h.get_by_label("Outer button").focus();
    h.run();
    h.key_press(Key::Num3);
    h.run();
    assert_eq!(h.state().ran.last().unwrap(), "rate.3");
    h.get_by_role(egui::accesskit::Role::TextInput).focus();
    h.run();
    let cx = focus::when_context(&h.ctx, &WhenContext::new());
    assert_eq!(cx.get("textInputFocus"), "true");
    assert_eq!(cx.get("mode"), "edit");
    h.key_press(Key::Num3);
    h.run();
    assert_eq!(h.state().ran.len(), 2);
}

#[test]
fn chords_convert_both_ways() {
    let c = chord_of(Key::K, Modifiers::COMMAND);
    assert_eq!(c.to_string(), "Ctrl+K");
    let (m, k) = key_of(&c).unwrap();
    assert_eq!(k, Key::K);
    assert!(m.command);
    assert_eq!(
        chord_of(Key::ArrowDown, Modifiers::SHIFT).to_string(),
        "Shift+Down"
    );
}

struct Pop {
    open: bool,
    anchor: egui::Rect,
}

#[test]
fn an_overlay_closes_on_escape_and_gives_the_focus_back() {
    let mut h = Harness::new_ui_state(
        |ui, s: &mut Pop| {
            overlay::begin_pass(ui.ctx());
            let r = ui.button("Open");
            if r.clicked() {
                s.open = true;
            }
            s.anchor = r.rect;
            let spec = OverlaySpec::new("pop", "Options", Kind::Popover);
            overlay::show(
                ui.ctx(),
                &spec,
                &mut s.open,
                Some(Anchor {
                    rect: s.anchor,
                    side: Side::Bottom,
                }),
                |ui| {
                    let _ = ui.button("Inside");
                },
            );
        },
        Pop {
            open: false,
            anchor: egui::Rect::NOTHING,
        },
    );
    h.run();
    h.get_by_label("Open").focus();
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert!(h.state().open);
    assert_eq!(overlay::stack(&h.ctx).len(), 1);
    let inside = h.get_by_label("Inside").rect();
    assert!(
        inside.top() >= h.state().anchor.bottom(),
        "placed below its anchor"
    );
    h.get_by_label("Inside").focus();
    h.run();
    h.key_press(Key::Escape);
    h.run();
    assert!(!h.state().open);
    assert!(overlay::stack(&h.ctx).is_empty());
    assert!(h.get_by_label("Open").is_focused(), "focus restored");
    assert_eq!(
        overlay::recently_closed(&h.ctx)[0].reason,
        overlay::Reason::Escape
    );
}

#[test]
fn a_panic_in_a_boundary_shows_a_card_until_retried() {
    let mut h = Harness::new_ui_state(
        |ui, boom: &mut bool| {
            let fail = *boom;
            fault::boundary(ui, "inspector", "Inspector", |ui| {
                if fail {
                    panic!("index out of range");
                }
                ui.label("All good");
            });
            ui.label("Below the boundary");
        },
        true,
    );
    h.run();
    assert!(h.query_by_label("Inspector stopped working").is_some());
    assert!(h.query_by_label("index out of range").is_some());
    assert!(
        h.query_by_label("Below the boundary").is_some(),
        "the rest is drawn"
    );
    assert_eq!(fault::faults(&h.ctx).len(), 1);
    *h.state_mut() = false;
    h.get_by_label("Retry").click();
    h.run();
    assert!(h.query_by_label("All good").is_some());
    assert!(fault::faults(&h.ctx).is_empty());
}

fn store() -> binding::SharedStore {
    let store = Arc::new(Mutex::new(Store::new()));
    store
        .lock()
        .unwrap()
        .declare(
            &OwnerId::FRAMEWORK,
            "settings/volume",
            Descriptor::float("Volume", 0.0, 100.0, 50.0),
        )
        .unwrap();
    store
}

#[test]
fn bound_widgets_write_the_store_and_a_drag_is_one_undo_step() {
    let store = store();
    let seen = Arc::new(Mutex::new(0));
    let s2 = seen.clone();
    store
        .lock()
        .unwrap()
        .listen(&OwnerId::FRAMEWORK, "settings/", move |_| {
            *s2.lock().unwrap() += 1
        });
    let st = store.clone();
    let mut h = Harness::new_ui_state(
        move |ui, local: &mut f64| {
            binding::edit(ui, Binding::plain(local), |ui, v| {
                ui.add(egui::DragValue::new(v).prefix("plain "))
            });
            binding::edit(
                ui,
                Binding::bound(&st, "settings/volume"),
                |ui, v: &mut f64| ui.add(egui::DragValue::new(v).prefix("bound ")),
            );
            binding::edit(
                ui,
                Binding::watch(&st, "settings/volume"),
                |ui, v: &mut f64| ui.add(egui::DragValue::new(v).prefix("watch ")),
            );
        },
        1.0,
    );
    h.run();
    // A drag over the bound value: many writes, one undo step.
    let node = h.get_by_value("bound 50");
    let start = node.rect().center();
    h.drag_at(start);
    h.run();
    for dx in [5.0, 10.0, 20.0] {
        h.hover_at(start + egui::vec2(dx, 0.0));
        h.run();
    }
    h.drop_at(start + egui::vec2(20.0, 0.0));
    h.run();
    let s = store.lock().unwrap();
    let v = s.get("settings/volume").unwrap();
    assert!(v.value.as_f64().unwrap() > 50.0, "{:?}", v.value);
    assert_eq!(v.origin, Some(Origin::User));
    assert_eq!(s.undo_steps().len(), 1, "{:?}", s.undo_steps());
    assert!(*seen.lock().unwrap() >= 1);
    drop(s);
    // The watcher shows the new value and cannot be changed.
    let shown = format!(
        "watch {}",
        binding::read::<f64>(&store, "settings/volume").unwrap()
    );
    let watch = h.get_by_value(&shown);
    assert!(watch.accesskit_node().is_disabled());
}

#[test]
fn documents_are_written_atomically_with_a_backup_and_migrated_on_load() {
    let dir = std::env::temp_dir().join(format!("egui-pop-core-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let persist = Persist::new(&dir);
    assert!(
        persist
            .load::<Layout>("layout", &layout::schema())
            .unwrap()
            .is_none()
    );
    // A version 1 layout written by an older release.
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        persist.path("layout"),
        r#"{"version": 1, "areas": [["editors", {"kind": "tabs", "panels": ["app.a"], "active": 0}]]}"#,
    )
    .unwrap();
    let read = persist
        .load::<Layout>("layout", &layout::schema())
        .unwrap()
        .unwrap();
    assert_eq!(read.from_version, 1);
    assert_eq!(read.notes, ["migrated 1 -> 2"]);
    // Saved twice: the first save becomes the backup.
    persist
        .save("layout", &layout::schema(), &read.value)
        .unwrap();
    let mut second = read.value.clone();
    second.hidden.push("app.b".parse().unwrap());
    persist.save("layout", &layout::schema(), &second).unwrap();
    assert!(dir.join("layout.json.bak").exists());
    assert!(!dir.join("layout.json.tmp").exists());
    // The main file is damaged: the backup is used, and said.
    std::fs::write(persist.path("layout"), "{ not json").unwrap();
    let read = persist
        .load::<Layout>("layout", &layout::schema())
        .unwrap()
        .unwrap();
    assert_eq!(read.source, Source::Backup);
    assert!(read.value.hidden.is_empty());
    assert!(read.notes[0].contains("unreadable"), "{:?}", read.notes);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn strings_fall_back_to_english_and_fill_placeholders() {
    let ctx = egui::Context::default();
    assert_eq!(
        strings::tr_args(&ctx, &strings::FAULT_TITLE, &[("name", "Log")]),
        "Log stopped working"
    );
    strings::install(
        &ctx,
        Arc::new(
            strings::MapStrings::new("fr")
                .with(strings::FAULT_TITLE.key, "{name} a cessé de fonctionner"),
        ),
    );
    assert_eq!(
        strings::tr_args(&ctx, &strings::FAULT_TITLE, &[("name", "Log")]),
        "Log a cessé de fonctionner"
    );
    assert_eq!(strings::provider(&ctx).language(), "fr");
    assert_eq!(strings::tr(&ctx, &strings::CANCEL), "Cancel");
    let keys: std::collections::BTreeSet<&str> = strings::ALL.iter().map(|k| k.key).collect();
    assert_eq!(keys.len(), strings::ALL.len(), "keys are unique");
}

#[test]
fn announcements_reach_a_live_region() {
    let mut h = Harness::new_ui(|ui| {
        a11y::live_regions(ui);
        if ui.button("Save").clicked() {
            a11y::announce(ui.ctx(), "Saved", a11y::Politeness::Polite);
        }
    });
    h.run();
    h.get_by_label("Save").click();
    h.run();
    let labels: Vec<Option<String>> = h
        .query_all_by_role(egui::accesskit::Role::Status)
        .map(|n| n.accesskit_node().label())
        .collect();
    assert_eq!(labels, [Some("Saved".to_string()), Some(String::new())]);
    assert_eq!(a11y::announcements(&h.ctx).len(), 1);
}

#[test]
fn icons_are_named_and_sized_by_role() {
    use egui_pop_core::icon::{Icon, Size};
    let mut h = Harness::new_ui(|ui| {
        ui.add(Icon::Close.size(Size::Toolbar).label("Close").sense_click());
        ui.add(Icon::Check.label("Done"));
    });
    h.run();
    let t = egui_pop_core::theme::Tokens::of(&h.ctx);
    let close = h.get_by_label("Close");
    assert_eq!(close.rect().width(), t.size.icon_toolbar);
    assert_eq!(close.accesskit_node().role(), egui::accesskit::Role::Button);
    assert_eq!(h.get_by_label("Done").rect().width(), t.size.icon_inline);
    let _ = Value::Bool(true);
}
