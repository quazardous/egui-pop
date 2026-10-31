//! The file chooser: the in-app picker driven by the keyboard (filter,
//! arrows, Enter, a typed path with completion, bookmarks, Escape), its
//! result through a `Pending`, and the `file.choose` and `file.chosen`
//! steps.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use egui::accesskit::{Role, Toggled};
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::dialogs::{self, DIALOGS};
use egui_pop_api::files::fs::{DirItem, FileSystem, MemoryFs, Meta, SharedFs};
use egui_pop_api::files::preview::{
    Body, Handles, Preview, PreviewProvider, PreviewRequest, Shown,
};
use egui_pop_api::files::{
    self, CMD_NEW_FOLDER, CMD_SELECT_ALL, CMD_TOGGLE_SELECTION, Chosen, FILES, FileRequest,
    FilesPlugin, PickerView, STEP_CHOOSE, STEP_CHOSEN,
};
use egui_pop_api::host::Pending;
use egui_pop_api::steps::Before;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_core::a11y;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{OwnerId, PluginId};
use egui_pop_model::values::{Change, Set, Value};
use egui_pop_test::walk::{self, Press};
use serde_json::json;

/// A folder tree of its own per test.
fn tree(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("egui-pop-files-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for d in ["photos/2026", "notes"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    for f in [
        "photos/a.png",
        "photos/b.jpg",
        "photos/c.txt",
        "notes/todo.md",
    ] {
        std::fs::write(root.join(f), b"x").unwrap();
    }
    root
}

fn harness(extra: Option<Box<dyn Plugin>>) -> Harness<'static, App> {
    let mut b = App::builder().plugin(FilesPlugin::in_app());
    if let Some(p) = extra {
        b = b.boxed(p);
    }
    let app = b.build(&egui::Context::default()).unwrap();
    Harness::builder()
        .with_size(egui::vec2(900.0, 640.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app)
}

/// Frames until the picker has settled: no frame asks for another at once,
/// the open dialog (placed once its size is known) has stopped moving, and
/// the highlighted entry's preview is done. The preview's worker repaints on
/// its own thread's time, and that stacks on the repaints of the frames around
/// it (the opening's, a text cursor's blink), so `Harness::run` and its cap on
/// the steps of one run would fail or not depending on the machine's load.
fn settle(h: &mut Harness<'static, App>) {
    let rect = |h: &Harness<'static, App>| {
        let app = h.state().handle().clone();
        let id = *app.service(&FILES)?.open_ids().first()?;
        let modal = egui::Id::new(("egui_pop.files", id));
        h.ctx.memory(|m| m.area_rect(modal))
    };
    let mut placed = rect(h);
    for _ in 0..500 {
        let quiet = h.try_run().is_ok();
        let now = rect(h);
        let snapshot = h.state().handle().debug_snapshot();
        let loading = snapshot["files"]["open"][0]["picker"]["preview"]["state"] == "loading";
        if quiet && now == placed && !loading {
            return;
        }
        placed = now;
        if loading {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    panic!("the picker did not settle");
}

fn choose(h: &mut Harness<'static, App>, request: FileRequest) -> Pending {
    let app = h.state().handle().clone();
    let f = app.service(&FILES).unwrap();
    let p = f.choose(&app, request);
    settle(h);
    p
}

fn result(h: &mut Harness<'static, App>, p: &Pending) -> Result<Chosen, String> {
    for _ in 0..4 {
        settle(h);
        if let Some(r) = p.take() {
            return files::chosen(&r);
        }
    }
    panic!("no result");
}

fn key(h: &mut Harness<'static, App>, key: Key, modifiers: Modifiers) {
    h.key_press_modifiers(modifiers, key);
    settle(h);
}

fn type_in(h: &mut Harness<'static, App>, text: &str) {
    h.get_by_label("Filter, or type a path").type_text(text);
    settle(h);
}

#[test]
fn the_keyboard_filters_enters_and_chooses() {
    let root = tree("keys");
    let mut h = harness(None);
    let p = choose(
        &mut h,
        FileRequest::open("Open an image")
            .directory(&root)
            .filter("Images", &["png", "jpg"]),
    );
    assert!(h.query_by_label("Open an image").is_some());
    assert!(h.query_by_label("photos/").is_some());
    // The fuzzy filter, then Enter into the folder.
    type_in(&mut h, "pho");
    assert!(h.query_by_label("notes/").is_none());
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert!(h.query_by_label("a.png").is_some());
    assert!(h.query_by_label("c.txt").is_none(), "the filter hides it");
    // Folders first: 2026, a.png, b.jpg; down twice is b.jpg.
    key(&mut h, Key::ArrowDown, Modifiers::NONE);
    key(&mut h, Key::ArrowDown, Modifiers::NONE);
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![root.join("photos/b.jpg")]))
    );
    // The folder is a recent place now, in the store.
    assert_eq!(files::recent(h.state().handle()), vec![root.join("photos")]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_typed_path_completes_with_tab_and_backspace_goes_up() {
    let root = tree("typed");
    let mut h = harness(None);
    let p = choose(&mut h, FileRequest::open("Open").directory(&root));
    type_in(&mut h, &format!("{}/no", root.display()));
    key(&mut h, Key::Tab, Modifiers::NONE);
    let id = h.state().handle().service(&FILES).unwrap().open_ids()[0];
    let picker = |h: &Harness<'static, App>| {
        h.state()
            .handle()
            .service(&FILES)
            .unwrap()
            .picker(id)
            .unwrap()
    };
    assert_eq!(picker(&h).text, format!("{}/notes/", root.display()));
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(picker(&h).dir, root.join("notes"));
    // The list's row, and the preview pane's title once it is selected.
    assert!(h.query_all_by_label("todo.md").next().is_some());
    // Backspace in an empty field: the parent folder.
    key(&mut h, Key::Backspace, Modifiers::NONE);
    assert_eq!(picker(&h).dir, root);
    // Escape: the field lets go of the focus, then the picker cancels.
    key(&mut h, Key::Escape, Modifiers::NONE);
    assert!(p.take().is_none());
    key(&mut h, Key::Escape, Modifiers::NONE);
    assert_eq!(result(&mut h, &p), Ok(Chosen::Cancelled));
    let journal = h.state().handle().events_since(0);
    assert!(journal.iter().any(|e| e.name == "file.cancelled"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn bookmarks_are_kept_and_listed() {
    let root = tree("marks");
    let mut h = harness(None);
    let _p = choose(
        &mut h,
        FileRequest::folder("Folder").directory(root.join("photos")),
    );
    key(&mut h, Key::D, Modifiers::COMMAND);
    assert_eq!(
        files::bookmarks(h.state().handle()),
        vec![root.join("photos")]
    );
    settle(&mut h);
    assert!(h.query_by_label("Remove the bookmark").is_some());
    // The place takes back to the folder from elsewhere.
    key(&mut h, Key::ArrowUp, Modifiers::ALT);
    h.get_by_label("photos").click();
    settle(&mut h);
    let f = h.state().handle().service(&FILES).unwrap();
    assert_eq!(f.picker(f.open_ids()[0]).unwrap().dir, root.join("photos"));
    let _ = std::fs::remove_dir_all(root);
}

/// Replaces the chooser and refuses text files.
struct Hooks;

impl Plugin for Hooks {
    fn id(&self) -> PluginId {
        PluginId::from_static("hooks")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.replace(STEP_CHOOSE, |_, args| {
            let name = args["request"]["file_name"].as_str().unwrap_or("x");
            Ok(json!({ "paths": [format!("/chosen/{name}")] }))
        });
        ctx.before(STEP_CHOSEN, |_, args| {
            let txt = args["paths"][0]
                .as_str()
                .is_some_and(|p| Path::new(p).extension().is_some_and(|e| e == "txt"));
            if txt {
                Before::Veto("no text files".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

#[test]
fn a_plugin_replaces_the_chooser_and_vetoes_a_choice() {
    let mut h = harness(Some(Box::new(Hooks)));
    let p = choose(&mut h, FileRequest::save("Save", "a.png"));
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![PathBuf::from("/chosen/a.png")]))
    );
    let p = choose(&mut h, FileRequest::save("Save", "a.txt"));
    assert_eq!(result(&mut h, &p), Err("no text files".into()));
}

#[test]
fn an_agent_answers_through_the_bridge() {
    let root = tree("agent");
    let mut h = harness(None);
    let p = choose(&mut h, FileRequest::open("Open").directory(&root));
    let snap = h.state().handle().debug_snapshot();
    let open = &snap["files"]["open"][0];
    assert_eq!(open["via"], "in_app");
    assert_eq!(open["picker"]["shown"], json!(["notes/", "photos/"]));
    let id = open["id"].as_u64().unwrap();
    let feedback = h.state_mut().debug_call(
        "files",
        "answer",
        &json!({ "id": id, "paths": [root.join("notes/todo.md")] }),
    );
    assert!(feedback.ok, "{}", feedback.message);
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![root.join("notes/todo.md")]))
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_click_highlights_a_double_click_chooses() {
    let root = tree("mouse");
    let mut h = harness(None);
    let p = choose(
        &mut h,
        FileRequest::open("Open").directory(root.join("photos")),
    );
    h.get_by_label("b.jpg").click();
    settle(&mut h);
    let f = h.state().handle().service(&FILES).unwrap();
    assert_eq!(f.picker(f.open_ids()[0]).unwrap().highlight, 2);
    double_click(&mut h, "a.png");
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![root.join("photos/a.png")]))
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_preview_pane_keeps_the_dialog_s_size() {
    let root = tree("preview-size");
    let mut h = harness(None);
    let app = h.state().handle().clone();
    let _p = choose(&mut h, FileRequest::open("Open").directory(&root));
    let id = app.service(&FILES).unwrap().open_ids()[0];
    let modal = egui::Id::new(("egui_pop.files", id));
    // The dialog's width and a list row's (rows fill the list's width).
    let measure = |h: &mut Harness<'static, App>, on: bool| {
        files::want_preview(&app, on);
        for _ in 0..3 {
            settle(h);
        }
        let dialog = h.ctx.memory(|m| m.area_rect(modal)).unwrap().width();
        let row = h.get_by_label("photos/").rect().width();
        (dialog, row)
    };
    let (dialog_on, row_on) = measure(&mut h, true);
    let (dialog_off, row_off) = measure(&mut h, false);
    assert_eq!(dialog_on, dialog_off);
    assert!(
        row_on + files::preview::PANE_WIDTH <= row_off,
        "the list gives the pane its width: {row_on} with it, {row_off} without"
    );
    let _ = std::fs::remove_dir_all(root);
}

const SPLITTER: &str = "Resize the preview";

/// A harness of a size of its own, large enough for the default dialog.
fn sized_harness(size: egui::Vec2) -> Harness<'static, App> {
    let app = App::builder()
        .plugin(FilesPlugin::in_app())
        .build(&egui::Context::default())
        .unwrap();
    Harness::builder()
        .with_size(size)
        .build_ui_state(|ui, app: &mut App| app.show(ui), app)
}

fn large_harness() -> Harness<'static, App> {
    sized_harness(egui::vec2(1280.0, 800.0))
}

/// The dialog of the only choice open, as drawn.
fn dialog(h: &Harness<'static, App>) -> egui::Rect {
    let app = h.state().handle().clone();
    let id = app.service(&FILES).unwrap().open_ids()[0];
    let modal = egui::Id::new(("egui_pop.files", id));
    h.ctx.memory(|m| m.area_rect(modal)).unwrap()
}

/// Drags from a point by so much, and lets go.
fn drag(h: &mut Harness<'static, App>, from: egui::Pos2, by: egui::Vec2) {
    h.hover_at(from);
    settle(h);
    h.drag_at(from);
    settle(h);
    for i in 1..=4 {
        h.hover_at(from + by * (i as f32 / 4.0));
        settle(h);
    }
    h.drop_at(from + by);
    settle(h);
}

fn pane_value(h: &Harness<'static, App>) -> f64 {
    h.get_by_role_and_label(Role::Splitter, SPLITTER)
        .accesskit_node()
        .numeric_value()
        .unwrap()
}

#[test]
fn a_resized_dialog_is_kept_for_the_next_picker() {
    let root = tree("resize");
    let mut h = large_harness();
    let app = h.state().handle().clone();
    let _p = choose(&mut h, FileRequest::open("Open").directory(&root));
    let before = dialog(&h);
    // The right edge: the left one stays.
    drag(
        &mut h,
        egui::pos2(before.right(), before.center().y),
        egui::vec2(100.0, 0.0),
    );
    let wider = dialog(&h);
    assert!(
        (wider.width() - before.width() - 100.0).abs() < 1.5,
        "{wider:?}"
    );
    assert!((wider.left() - before.left()).abs() < 1.5, "{wider:?}");
    // The bottom right corner, both ways.
    drag(
        &mut h,
        wider.max - egui::vec2(2.0, 2.0),
        egui::vec2(-60.0, 40.0),
    );
    let moved = dialog(&h);
    assert!(
        (moved.width() - wider.width() + 60.0).abs() < 1.5,
        "{moved:?}"
    );
    assert!(
        (moved.height() - wider.height() - 40.0).abs() < 1.5,
        "{moved:?}"
    );
    let kept = files::dialog_size(&app);
    assert_ne!(kept, egui::vec2(files::PICKER_WIDTH, files::PICKER_HEIGHT));
    // Another choice opens at that size.
    key(&mut h, Key::Escape, Modifiers::NONE);
    key(&mut h, Key::Escape, Modifiers::NONE);
    for _ in 0..3 {
        settle(&mut h);
    }
    assert!(app.service(&FILES).unwrap().open_ids().is_empty());
    let _p = choose(&mut h, FileRequest::open("Open").directory(&root));
    assert_eq!(dialog(&h).size(), moved.size());
    assert_eq!(files::dialog_size(&app), kept);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_dialog_stops_at_its_least_size_and_at_the_window() {
    let root = tree("resize-limits");
    let mut h = large_harness();
    let _p = choose(&mut h, FileRequest::open("Open").directory(&root));
    let at = dialog(&h);
    drag(
        &mut h,
        at.max - egui::vec2(2.0, 2.0),
        egui::vec2(-2000.0, -2000.0),
    );
    let least = dialog(&h);
    assert!(least.width() >= files::PICKER_MIN_SIZE.x, "{least:?}");
    assert!(least.height() >= files::PICKER_MIN_SIZE.y, "{least:?}");
    assert!(least.width() < at.width() && least.height() < at.height());
    drag(
        &mut h,
        least.max - egui::vec2(2.0, 2.0),
        egui::vec2(3000.0, 3000.0),
    );
    let window = h.ctx.content_rect();
    assert!(window.contains_rect(dialog(&h)), "{:?}", dialog(&h));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_window_holds_the_dialog_and_a_size_that_no_longer_fits_is_reset() {
    let root = tree("resize-window");
    let mut h = large_harness();
    let app = h.state().handle().clone();
    files::want_dialog_size(&app, egui::vec2(1100.0, 700.0));
    let _p = choose(&mut h, FileRequest::open("Open").directory(&root));
    let big = dialog(&h);
    assert!(big.width() > 1100.0, "{big:?}");
    // The window gets smaller while it is open: held, and the size asked
    // is not forgotten.
    h.set_size(egui::vec2(640.0, 480.0));
    for _ in 0..3 {
        settle(&mut h);
    }
    assert!(
        h.ctx.content_rect().contains_rect(dialog(&h)),
        "{:?}",
        dialog(&h)
    );
    assert_eq!(files::dialog_size(&app), egui::vec2(1100.0, 700.0));
    h.set_size(egui::vec2(1280.0, 800.0));
    for _ in 0..3 {
        settle(&mut h);
    }
    assert_eq!(dialog(&h).size(), big.size());
    // Opening in a window too small for it: back to the default.
    key(&mut h, Key::Escape, Modifiers::NONE);
    key(&mut h, Key::Escape, Modifiers::NONE);
    for _ in 0..3 {
        settle(&mut h);
    }
    h.set_size(egui::vec2(640.0, 480.0));
    settle(&mut h);
    let _p = choose(&mut h, FileRequest::open("Open").directory(&root));
    assert!(
        h.ctx.content_rect().contains_rect(dialog(&h)),
        "{:?}",
        dialog(&h)
    );
    assert_eq!(
        files::dialog_size(&app),
        egui::vec2(files::PICKER_WIDTH, files::PICKER_HEIGHT)
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_splitter_moves_within_its_limits_and_is_kept() {
    let root = tree("splitter");
    let mut h = large_harness();
    let app = h.state().handle().clone();
    let _p = choose(&mut h, FileRequest::open("Open").directory(&root));
    assert_eq!(f64::from(files::preview::PANE_WIDTH), pane_value(&h));
    // To the left, the pane takes the list's width, down to the list's
    // least width.
    let line = h.get_by_role_and_label(Role::Splitter, SPLITTER).rect();
    drag(&mut h, line.center(), egui::vec2(-2000.0, 0.0));
    let widest = pane_value(&h);
    assert!(widest > f64::from(files::preview::PANE_WIDTH));
    assert_eq!(f64::from(files::pane_width(&app)), widest);
    let row = h.get_by_label("photos/").rect().width();
    assert!(
        (row - files::LIST_MIN_WIDTH).abs() < 1.0,
        "the list keeps its least width: {row}"
    );
    // To the right, down to the pane's least width.
    let line = h.get_by_role_and_label(Role::Splitter, SPLITTER).rect();
    drag(&mut h, line.center(), egui::vec2(3000.0, 0.0));
    assert_eq!(pane_value(&h), f64::from(files::PANE_MIN_WIDTH));
    // A double click gives the default back.
    let line = h.get_by_role_and_label(Role::Splitter, SPLITTER).rect();
    let pos = line.center();
    {
        let events = &mut h.input_mut().events;
        events.push(egui::Event::PointerMoved(pos));
        for _ in 0..2 {
            for pressed in [true, false] {
                events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                });
            }
        }
    }
    settle(&mut h);
    assert_eq!(pane_value(&h), f64::from(files::preview::PANE_WIDTH));
    assert_eq!(files::pane_width(&app), files::preview::PANE_WIDTH);
    // Another width is there for the next picker.
    files::want_pane_width(&app, 300.0);
    key(&mut h, Key::Escape, Modifiers::NONE);
    key(&mut h, Key::Escape, Modifiers::NONE);
    for _ in 0..3 {
        settle(&mut h);
    }
    let _p = choose(&mut h, FileRequest::open("Open").directory(&root));
    assert_eq!(pane_value(&h), 300.0);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn tab_reaches_the_splitter_and_the_arrows_move_it() {
    let root = tree("splitter-keys");
    let mut h = large_harness();
    let app = h.state().handle().clone();
    let _p = choose(&mut h, FileRequest::open("Open").directory(&root));
    let focused = |h: &Harness<'static, App>| {
        h.get_by_role_and_label(Role::Splitter, SPLITTER)
            .is_focused()
    };
    // From the field, past the places, not through the entries.
    let mut presses = 0;
    while !focused(&h) && presses < 6 {
        key(&mut h, Key::Tab, Modifiers::NONE);
        settle(&mut h);
        presses += 1;
    }
    assert!(focused(&h), "not reached in {presses} presses");
    let start = pane_value(&h);
    key(&mut h, Key::ArrowLeft, Modifiers::NONE);
    let wider = pane_value(&h);
    assert!(wider > start, "left widens the pane after the line");
    key(&mut h, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(pane_value(&h), start);
    key(&mut h, Key::ArrowLeft, Modifiers::SHIFT);
    assert_eq!(pane_value(&h) - start, (wider - start) * 4.0);
    assert_eq!(f64::from(files::pane_width(&app)), pane_value(&h));
    // The list's keys are not taken from it, and it keeps the focus.
    let highlight =
        app.service(&FILES).unwrap().describe()["open"][0]["picker"]["highlight"].clone();
    key(&mut h, Key::ArrowDown, Modifiers::NONE);
    assert!(focused(&h));
    assert_eq!(
        app.service(&FILES).unwrap().describe()["open"][0]["picker"]["highlight"],
        highlight
    );
    // Enter gives the default back, and chooses nothing.
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(pane_value(&h), f64::from(files::preview::PANE_WIDTH));
    assert_eq!(app.service(&FILES).unwrap().open_ids().len(), 1);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_corner_handle_resizes_with_the_arrows() {
    let root = tree("handle-keys");
    let mut h = large_harness();
    let app = h.state().handle().clone();
    let _p = choose(&mut h, FileRequest::open("Open").directory(&root));
    let before = files::dialog_size(&app);
    h.get_by_role_and_label(Role::Splitter, "Resize Open")
        .focus();
    settle(&mut h);
    key(&mut h, Key::ArrowRight, Modifiers::NONE);
    key(&mut h, Key::ArrowDown, Modifiers::NONE);
    settle(&mut h);
    let after = files::dialog_size(&app);
    assert!(
        after.x > before.x && after.y > before.y,
        "{before:?} {after:?}"
    );
    let value = h
        .get_by_role_and_label(Role::Splitter, "Resize Open")
        .accesskit_node()
        .value();
    assert_eq!(value, Some(format!("{} × {}", after.x, after.y)));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn toggling_the_preview_keeps_a_resized_dialog() {
    let root = tree("resize-preview");
    let mut h = large_harness();
    let app = h.state().handle().clone();
    let _p = choose(&mut h, FileRequest::open("Open").directory(&root));
    let at = dialog(&h);
    drag(
        &mut h,
        egui::pos2(at.right(), at.center().y),
        egui::vec2(80.0, 0.0),
    );
    let size = dialog(&h).size();
    key(&mut h, Key::P, Modifiers::ALT);
    settle(&mut h);
    assert!(!files::preview_wanted(&app));
    assert!(
        h.query_by_role_and_label(Role::Splitter, SPLITTER)
            .is_none()
    );
    assert_eq!(dialog(&h).size(), size);
    key(&mut h, Key::P, Modifiers::ALT);
    settle(&mut h);
    assert!(
        h.query_by_role_and_label(Role::Splitter, SPLITTER)
            .is_some()
    );
    assert_eq!(dialog(&h).size(), size);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_list_takes_the_dialog_s_height() {
    let root = tree("resize-height");
    let mut h = large_harness();
    let _p = choose(&mut h, FileRequest::open("Open").directory(&root));
    let top = h.get_by_role_and_label(Role::Button, "Open").rect().top();
    let at = dialog(&h);
    assert!(top < at.bottom(), "the buttons are inside");
    drag(
        &mut h,
        egui::pos2(at.center().x, at.bottom()),
        egui::vec2(0.0, 120.0),
    );
    let taller = dialog(&h);
    assert!((taller.height() - at.height() - 120.0).abs() < 1.5);
    // The buttons follow the bottom, inside the frame.
    let button = h.get_by_role_and_label(Role::Button, "Open").rect();
    assert!(
        (button.top() - top - 120.0).abs() < 1.5,
        "{button:?} from {top}"
    );
    assert!(button.bottom() < taller.bottom());
    let _ = std::fs::remove_dir_all(root);
}

/// A double click in one frame (the test kit's helper, which this crate
/// cannot depend on; see docs/upstream.md).
fn double_click(h: &mut Harness<'static, App>, label: &str) {
    let pos = h.get_by_label(label).rect().center();
    let events = &mut h.input_mut().events;
    events.push(egui::Event::PointerMoved(pos));
    for _ in 0..2 {
        for pressed in [true, false] {
            events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
    }
    h.step();
}

/// With the dialogs plugin, which asks before replacing a file.
fn saving_harness(fs: Option<files::fs::SharedFs>) -> Harness<'static, App> {
    let mut files = FilesPlugin::in_app();
    if let Some(fs) = fs {
        files = files.file_system(fs);
    }
    let app = App::builder()
        .plugin(dialogs::DialogsPlugin)
        .plugin(files)
        .build(&egui::Context::default())
        .unwrap();
    Harness::builder()
        .with_size(egui::vec2(900.0, 640.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app)
}

/// The field's text.
fn field(h: &Harness<'static, App>) -> String {
    let snapshot = h.state().handle().debug_snapshot();
    snapshot["files"]["open"][0]["picker"]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn picker_state(h: &Harness<'static, App>, key: &str) -> serde_json::Value {
    h.state().handle().debug_snapshot()["files"]["open"][0]["picker"][key].clone()
}

/// Enter in the field, the field holding a new name typed over the stem.
fn save_as(h: &mut Harness<'static, App>, name: &str) {
    let label = "File name, or a path";
    h.get_by_label(label).type_text(name);
    settle(h);
    key(h, Key::Enter, Modifiers::NONE);
}

#[test]
fn saving_over_a_file_asks_first_and_the_answer_decides() {
    let root = tree("replace");
    let mut h = saving_harness(None);
    let p = choose(
        &mut h,
        FileRequest::save("Export", "a.png").directory(root.join("photos")),
    );
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert!(p.take().is_none(), "nothing chosen yet");
    assert!(h.query_by_label("Replace a.png?").is_some());
    // The dangerous answer is not the default: the focus is on Cancel, and
    // the Enter that reached the question did not accept it.
    assert!(
        h.query_all_by_role_and_label(Role::Button, "Cancel")
            .any(|b| b.is_focused())
    );
    // Escape closes the question only: the picker is still there, the
    // stem selected again for another name.
    key(&mut h, Key::Escape, Modifiers::NONE);
    assert!(h.query_by_label("Replace a.png?").is_none());
    assert!(p.take().is_none());
    assert_eq!(field(&h), "a.png");
    assert!(picker_state(&h, "asking").is_null());
    assert!(h.get_by_label("File name, or a path").is_focused());
    // Again, and Replace by the keyboard: from Cancel to the verb.
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert!(h.query_by_label("Replace a.png?").is_some());
    h.get_by_role_and_label(Role::Button, "Replace").focus();
    settle(&mut h);
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![root.join("photos/a.png")]))
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_picker_waits_under_its_question() {
    let root = tree("waits");
    let mut h = saving_harness(None);
    let p = choose(
        &mut h,
        FileRequest::save("Export", "a.png").directory(root.join("photos")),
    );
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert!(picker_state(&h, "asking").is_u64());
    // The picker's own keys do nothing meanwhile.
    key(&mut h, Key::ArrowUp, Modifiers::ALT);
    key(&mut h, Key::N, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(picker_state(&h, "dir"), json!(root.join("photos")));
    assert!(picker_state(&h, "new_folder").is_null());
    assert_eq!(h.state().handle().service(&DIALOGS).unwrap().len(), 1);
    assert!(p.take().is_none());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_new_name_asks_nothing_and_the_opt_out_neither() {
    let root = tree("no-question");
    let mut h = saving_harness(None);
    let dialogs = h.state().handle().service(&DIALOGS).unwrap();
    let p = choose(
        &mut h,
        FileRequest::save("Export", "new.png").directory(root.join("photos")),
    );
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![root.join("photos/new.png")]))
    );
    let p = choose(
        &mut h,
        FileRequest::save("Export", "a.png")
            .directory(root.join("photos"))
            .confirm_overwrite(false),
    );
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![root.join("photos/a.png")]))
    );
    assert!(dialogs.is_empty());
    assert!(dialogs.answered().is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn who_answers_for_the_picker_is_not_asked_again() {
    // An answer from outside the picker (the system's chooser ends this
    // way, as a run-hook and an agent do) goes straight to `file.chosen`.
    let root = tree("no-twice");
    let mut h = saving_harness(None);
    let dialogs = h.state().handle().service(&DIALOGS).unwrap();
    let p = choose(
        &mut h,
        FileRequest::save("Export", "a.png").directory(root.join("photos")),
    );
    let f = h.state().handle().service(&FILES).unwrap();
    f.deliver(f.open_ids()[0], Some(vec![root.join("photos/a.png")]))
        .unwrap();
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![root.join("photos/a.png")]))
    );
    assert!(dialogs.is_empty());
    assert!(dialogs.answered().is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn without_the_dialogs_plugin_nothing_is_replaced_and_the_picker_says_so() {
    let root = tree("no-dialogs");
    let mut h = harness(None);
    let p = choose(
        &mut h,
        FileRequest::save("Export", "a.png").directory(root.join("photos")),
    );
    key(&mut h, Key::Enter, Modifiers::NONE);
    settle(&mut h);
    assert!(p.take().is_none());
    assert!(
        h.query_by_label_contains("no dialog can ask").is_some(),
        "{}",
        picker_state(&h, "error")
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_inline_picker_asks_over_the_dialog_it_is_in() {
    let root = tree("inline");
    let mut h = saving_harness(None);
    let app = h.state().handle().clone();
    let f = app.service(&FILES).unwrap();
    let (id, p) = f.choose_inline(
        &app,
        FileRequest::save("Export", "a.png").directory(root.join("photos")),
    );
    let dialogs = app.service(&DIALOGS).unwrap();
    let f2 = f.clone();
    dialogs
        .open_custom(
            &app,
            dialogs::Custom::closing("Export dialog"),
            move |ui, app, _pass| {
                f2.show_inline(ui, app, id);
            },
        )
        .unwrap();
    settle(&mut h);
    assert!(h.query_by_label("Export dialog").is_some());
    h.get_by_role_and_label(Role::Button, "Save").click();
    settle(&mut h);
    // The question is shown at once, not queued behind the dialog.
    assert!(h.query_by_label("Replace a.png?").is_some());
    assert!(h.query_by_label("Export dialog").is_none());
    key(&mut h, Key::Escape, Modifiers::NONE);
    settle(&mut h);
    // Back to the dialog and its picker, which works again.
    assert!(h.query_by_label("Export dialog").is_some());
    assert!(p.take().is_none());
    h.get_by_role_and_label(Role::Button, "Save").click();
    settle(&mut h);
    h.get_by_role_and_label(Role::Button, "Replace").click();
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![root.join("photos/a.png")]))
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_filter_s_extension_is_added_and_follows_the_filter() {
    let root = tree("extension");
    let mut h = saving_harness(None);
    // Typing replaces the stem only: the extension is kept.
    let p = choose(
        &mut h,
        FileRequest::save("Export", "photo.png")
            .directory(&root)
            .filter("PNG", &["png"])
            .filter("JPEG", &["jpg", "jpeg"]),
    );
    h.get_by_label("File name, or a path").type_text("dawn");
    settle(&mut h);
    assert_eq!(field(&h), "dawn.png");
    // Another filter: the extension follows, the stem selected again.
    h.get_by_label("File type").click();
    settle(&mut h);
    h.get_by_label_contains("JPEG").click();
    settle(&mut h);
    assert_eq!(field(&h), "dawn.jpg");
    // All files: the name as it is.
    h.get_by_label("File type").click();
    settle(&mut h);
    h.get_by_label("All files").click();
    settle(&mut h);
    assert_eq!(field(&h), "dawn.jpg");
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![root.join("dawn.jpg")]))
    );
    // A name without an extension gets the filter's on the way out.
    let p = choose(
        &mut h,
        FileRequest::save("Export", "")
            .directory(&root)
            .filter("PNG", &["png"]),
    );
    save_as(&mut h, "plain");
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![root.join("plain.png")]))
    );
    // An extension the person typed stays.
    let p = choose(
        &mut h,
        FileRequest::save("Export", "")
            .directory(&root)
            .filter("PNG", &["png"]),
    );
    save_as(&mut h, "plain.tiff");
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![root.join("plain.tiff")]))
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_added_extension_that_exists_asks_too() {
    let root = tree("extension-asks");
    let mut h = saving_harness(None);
    let p = choose(
        &mut h,
        FileRequest::save("Export", "")
            .directory(root.join("photos"))
            .filter("PNG", &["png"]),
    );
    save_as(&mut h, "a");
    assert!(h.query_by_label("Replace a.png?").is_some());
    assert!(p.take().is_none());
    assert_eq!(field(&h), "a.png");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_new_folder_by_the_key_is_named_made_and_entered() {
    let root = tree("new-folder");
    let mut h = saving_harness(None);
    let p = choose(
        &mut h,
        FileRequest::save("Export", "a.png").directory(&root),
    );
    key(&mut h, Key::N, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(picker_state(&h, "new_folder"), json!(""));
    // Its name's field has the focus: typing goes there, by role and label.
    assert!(
        h.get_by_role_and_label(Role::TextInput, "Folder name")
            .is_focused()
    );
    // No name: said, and the row stays.
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert!(h.query_by_label("Type a name for the folder").is_some());
    // A folder that exists: said, the name there to fix.
    h.get_by_role_and_label(Role::TextInput, "Folder name")
        .type_text("notes");
    settle(&mut h);
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert!(h.query_by_label("notes already exists").is_some());
    assert_eq!(picker_state(&h, "new_folder"), json!("notes"));
    // A path is no name.
    for _ in 0..5 {
        key(&mut h, Key::Backspace, Modifiers::NONE);
    }
    h.get_by_role_and_label(Role::TextInput, "Folder name")
        .type_text("a/b");
    settle(&mut h);
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert!(h.query_by_label_contains("cannot name a folder").is_some());
    assert!(!root.join("a").exists());
    for _ in 0..3 {
        key(&mut h, Key::Backspace, Modifiers::NONE);
    }
    h.get_by_role_and_label(Role::TextInput, "Folder name")
        .type_text("Prints");
    settle(&mut h);
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert!(root.join("Prints").is_dir());
    // Entered; the row gone; the name to save under still in the field,
    // and the keyboard back there.
    assert_eq!(picker_state(&h, "dir"), json!(root.join("Prints")));
    assert!(picker_state(&h, "new_folder").is_null());
    assert_eq!(field(&h), "a.png");
    assert!(
        h.get_by_label("File name, or a path").is_focused(),
        "the field has the keyboard"
    );
    let journal = h.state().handle().events_since(0);
    assert!(journal.iter().any(|e| e.name == "file.folder_created"));
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![root.join("Prints/a.png")]))
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn escape_closes_the_new_folder_row_not_the_picker() {
    let root = tree("new-folder-escape");
    let mut h = saving_harness(None);
    let p = choose(&mut h, FileRequest::folder("Folder").directory(&root));
    h.get_by_role_and_label(Role::Button, "New folder").click();
    settle(&mut h);
    assert_eq!(picker_state(&h, "new_folder"), json!(""));
    h.get_by_role_and_label(Role::TextInput, "Folder name")
        .type_text("Never");
    settle(&mut h);
    key(&mut h, Key::Escape, Modifiers::NONE);
    assert!(picker_state(&h, "new_folder").is_null());
    assert!(p.take().is_none());
    assert!(h.query_by_label("Folder").is_some(), "the picker stays");
    assert!(!root.join("Never").exists());
    // From the row's buttons too.
    h.get_by_role_and_label(Role::Button, "New folder").click();
    settle(&mut h);
    h.get_by_role_and_label(Role::Button, "Create").focus();
    settle(&mut h);
    key(&mut h, Key::Escape, Modifiers::NONE);
    assert!(picker_state(&h, "new_folder").is_null());
    assert!(p.take().is_none());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_new_folder_key_is_a_command_and_rebinds() {
    let root = tree("new-folder-key");
    let mut h = saving_harness(None);
    let app = h.state().handle().clone();
    assert_eq!(app.keys_for(&CMD_NEW_FOLDER), vec!["Ctrl+Shift+N"]);
    app.rebind(CMD_NEW_FOLDER, Some("Ctrl+Shift+F".into()));
    settle(&mut h);
    // No picker open: the rebound key keeps the command's clause, so it is
    // not taken from whoever else wants it.
    key(&mut h, Key::F, Modifiers::COMMAND | Modifiers::SHIFT);
    let traces = app.debug_snapshot()["keys"]["traces"].clone();
    assert_eq!(traces[0]["resolution"]["kind"], "nothing", "{traces}");
    let _p = choose(&mut h, FileRequest::folder("Folder").directory(&root));
    key(&mut h, Key::N, Modifiers::COMMAND | Modifiers::SHIFT);
    assert!(picker_state(&h, "new_folder").is_null(), "no longer this");
    key(&mut h, Key::F, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(picker_state(&h, "new_folder"), json!(""));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_agent_makes_a_folder_and_opening_offers_none() {
    use egui_pop_model::commands::Origin;
    let root = tree("new-folder-agent");
    let mut h = saving_harness(None);
    let run = |h: &mut Harness<'static, App>, name: &str| {
        h.state_mut()
            .run(
                &CMD_NEW_FOLDER,
                json!({ "name": name }),
                Origin::Agent("test".into()),
            )
            .now()
            .unwrap()
    };
    assert!(run(&mut h, "Early").is_err(), "no picker is open");
    let _p = choose(&mut h, FileRequest::folder("Folder").directory(&root));
    let out = run(&mut h, "Prints").unwrap();
    assert_eq!(out["path"], json!(root.join("Prints")));
    assert!(root.join("Prints").is_dir());
    settle(&mut h);
    assert_eq!(picker_state(&h, "dir"), json!(root.join("Prints")));
    assert!(run(&mut h, "../out").is_err());
    let f = h.state().handle().service(&FILES).unwrap();
    f.deliver(f.open_ids()[0], None).unwrap();
    settle(&mut h);
    // Opening a file: no button, and the key does nothing to the picker.
    let _p = choose(&mut h, FileRequest::open("Open").directory(&root));
    assert!(h.query_by_label("New folder").is_none());
    key(&mut h, Key::N, Modifiers::COMMAND | Modifiers::SHIFT);
    assert!(picker_state(&h, "new_folder").is_null());
    assert!(run(&mut h, "Nope").is_err());
    assert!(!root.join("Nope").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_read_only_file_system_refuses_a_new_folder_out_loud() {
    let fs: files::fs::SharedFs =
        std::sync::Arc::new(files::fs::MemoryFs::new().file("/demo/a.png", b"x".to_vec()));
    let mut h = saving_harness(Some(fs));
    let _p = choose(
        &mut h,
        FileRequest::save("Export", "a.png").directory("/demo"),
    );
    // The button is there, disabled.
    let button = h.get_by_role_and_label(Role::Button, "New folder");
    assert!(button.accesskit_node().is_disabled());
    // The key says why, in the dialog.
    key(&mut h, Key::N, Modifiers::COMMAND | Modifiers::SHIFT);
    assert!(h.query_by_label_contains("read-only").is_some());
    assert!(picker_state(&h, "new_folder").is_null());
    // Replacing still asks: the question does not need to write.
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert!(h.query_by_label("Replace a.png?").is_some());
}

#[test]
fn saving_kinds_round_trip_the_opt_out() {
    let r = FileRequest::save("Export", "a.png");
    assert!(r.confirm_overwrite, "on by default");
    let off = r.clone().confirm_overwrite(false);
    assert_eq!(FileRequest::from_json(&off.to_json()).unwrap(), off);
    // A request written before the option asks.
    let mut old = r.to_json();
    old.as_object_mut().unwrap().remove("confirm_overwrite");
    assert!(FileRequest::from_json(&old).unwrap().confirm_overwrite);
}

#[test]
fn no_key_accepts_the_question_from_where_its_focus_starts() {
    // egui clicks a focused button on Enter or Space whatever the modifiers
    // (`Context::create_widget`). The question's focus starts on Cancel, so
    // each of these cancels it, and none reaches the picker below.
    let root = tree("danger-keys");
    for (k, modifiers) in [
        (Key::Enter, Modifiers::NONE),
        (Key::Enter, Modifiers::COMMAND),
        (Key::Enter, Modifiers::ALT),
        (Key::Enter, Modifiers::SHIFT),
        (Key::Space, Modifiers::NONE),
        (Key::Space, Modifiers::COMMAND),
    ] {
        let mut h = saving_harness(None);
        let dialogs = h.state().handle().service(&DIALOGS).unwrap();
        let p = choose(
            &mut h,
            FileRequest::save("Export", "a.png").directory(root.join("photos")),
        );
        key(&mut h, Key::Enter, Modifiers::NONE);
        assert!(h.query_by_label("Replace a.png?").is_some());
        key(&mut h, k, modifiers);
        assert_eq!(
            dialogs.answered().last().map(|(_, a, _)| a.clone()),
            Some(dialogs::Answer::Cancel),
            "{k:?} {modifiers:?}"
        );
        assert!(p.take().is_none(), "{k:?} {modifiers:?}: nothing chosen");
        assert!(h.get_by_label("File name, or a path").is_focused());
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn alt_up_goes_to_the_parent_folder() {
    let root = tree("alt-up");
    let mut h = harness(None);
    let _p = choose(
        &mut h,
        FileRequest::open("Open").directory(root.join("photos")),
    );
    key(&mut h, Key::ArrowUp, Modifiers::ALT);
    assert_eq!(picker_state(&h, "dir"), json!(root));
    let _ = std::fs::remove_dir_all(root);
}

/// Answers the open choice as the chooser would, and waits for its result.
fn answer(h: &mut Harness<'static, App>, p: &Pending, path: PathBuf) {
    let f = h.state().handle().service(&FILES).unwrap();
    f.deliver(f.open_ids()[0], Some(vec![path.clone()]))
        .unwrap();
    assert_eq!(result(h, p), Ok(Chosen::Paths(vec![path])));
}

/// Cancels the open choice with the keyboard: the field lets go of the
/// focus, then the picker cancels.
fn cancel(h: &mut Harness<'static, App>, p: &Pending) {
    key(h, Key::Escape, Modifiers::NONE);
    key(h, Key::Escape, Modifiers::NONE);
    assert_eq!(result(h, p), Ok(Chosen::Cancelled));
}

fn shown(h: &Harness<'static, App>, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

/// Home as the picker falls back on it.
fn home_or_root() -> PathBuf {
    files::home().unwrap_or_else(|| PathBuf::from("/"))
}

#[test]
fn a_second_opening_starts_in_the_purposes_last_folder() {
    let root = tree("purpose-again");
    let mut h = harness(None);
    let request = || {
        FileRequest::open("Import")
            .purpose("import")
            .directory(&root)
    };
    // The first time, the request's folder.
    let p = choose(&mut h, request());
    assert!(shown(&h, "photos/"));
    assert!(!shown(&h, "a.png"));
    // Chosen with the keyboard, in a folder below.
    type_in(&mut h, "pho");
    key(&mut h, Key::Enter, Modifiers::NONE);
    key(&mut h, Key::ArrowDown, Modifiers::NONE);
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![root.join("photos/a.png")]))
    );
    assert_eq!(
        files::last_folder(h.state().handle(), "import"),
        Some(root.join("photos"))
    );
    // The same request starts there now, its `directory` a first-time default.
    let p = choose(&mut h, request());
    assert!(shown(&h, "a.png"));
    assert!(!shown(&h, "photos/"));
    // A cancel keeps nothing: wandering off does not move the last folder.
    key(&mut h, Key::ArrowUp, Modifiers::ALT);
    assert!(shown(&h, "notes/"));
    cancel(&mut h, &p);
    let p = choose(&mut h, request());
    assert!(shown(&h, "a.png"));
    assert!(!shown(&h, "notes/"));
    cancel(&mut h, &p);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn two_purposes_keep_their_own_folders() {
    let root = tree("purpose-two");
    let mut h = harness(None);
    let import = || FileRequest::open("Import").purpose("import");
    let export = || FileRequest::save("Export", "new.md").purpose("export");

    let p = choose(&mut h, import().directory(&root));
    answer(&mut h, &p, root.join("photos/a.png"));
    // Another purpose does not start in it, with a folder of its own...
    let p = choose(&mut h, export().directory(root.join("notes")));
    assert!(shown(&h, "todo.md"));
    assert!(!shown(&h, "a.png"));
    answer(&mut h, &p, root.join("notes/new.md"));
    // ... or without one, though it is the last recent place too: home.
    let p = choose(&mut h, FileRequest::open("Other").purpose("other"));
    assert_eq!(picker_state(&h, "dir"), json!(home_or_root()));
    cancel(&mut h, &p);

    // Each comes back to its own.
    let p = choose(&mut h, import());
    assert!(shown(&h, "a.png"));
    assert!(!shown(&h, "todo.md"));
    cancel(&mut h, &p);
    let p = choose(&mut h, export());
    assert!(shown(&h, "todo.md"));
    assert!(!shown(&h, "a.png"));
    cancel(&mut h, &p);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_gone_last_folder_falls_back_to_the_request_then_home() {
    let root = tree("purpose-gone");
    let gone = root.join("photos/2026");
    let mut h = harness(None);
    let request = || FileRequest::folder("Folder").purpose("folder");
    // Choosing a folder keeps that folder, not its parent.
    let p = choose(&mut h, request().directory(&root));
    answer(&mut h, &p, gone.clone());
    assert_eq!(
        files::last_folder(h.state().handle(), "folder"),
        Some(gone.clone())
    );
    let p = choose(&mut h, request().directory(root.join("notes")));
    assert_eq!(picker_state(&h, "dir"), json!(gone));
    cancel(&mut h, &p);

    std::fs::remove_dir_all(&gone).unwrap();
    let p = choose(&mut h, request().directory(root.join("notes")));
    assert_eq!(picker_state(&h, "dir"), json!(root.join("notes")));
    cancel(&mut h, &p);
    let p = choose(&mut h, request());
    assert_eq!(picker_state(&h, "dir"), json!(home_or_root()));
    cancel(&mut h, &p);
    let p = choose(&mut h, request().directory(root.join("nowhere")));
    assert_eq!(picker_state(&h, "dir"), json!(home_or_root()));
    cancel(&mut h, &p);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_forced_directory_beats_the_last_folder_and_still_updates_it() {
    let root = tree("purpose-forced");
    let mut h = harness(None);
    let request = || FileRequest::open("Import").purpose("import");
    let p = choose(&mut h, request().directory(&root));
    answer(&mut h, &p, root.join("photos/a.png"));

    let p = choose(&mut h, request().force_directory(root.join("notes")));
    assert!(shown(&h, "todo.md"));
    answer(&mut h, &p, root.join("notes/todo.md"));
    assert_eq!(
        files::last_folder(h.state().handle(), "import"),
        Some(root.join("notes"))
    );
    // A forced folder that is gone gives way to the last one.
    let p = choose(&mut h, request().force_directory(root.join("nowhere")));
    assert_eq!(picker_state(&h, "dir"), json!(root.join("notes")));
    cancel(&mut h, &p);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn without_a_purpose_the_request_and_the_recent_places_decide() {
    let root = tree("purpose-none");
    let mut h = harness(None);
    let p = choose(&mut h, FileRequest::open("Open").directory(&root));
    answer(&mut h, &p, root.join("photos/a.png"));
    // The request's folder wins, on every opening.
    let p = choose(&mut h, FileRequest::open("Open").directory(&root));
    assert!(shown(&h, "photos/"));
    assert!(!shown(&h, "a.png"));
    cancel(&mut h, &p);
    // Without one, the last recent place, as ever.
    let p = choose(&mut h, FileRequest::open("Open"));
    assert!(shown(&h, "a.png"));
    cancel(&mut h, &p);
    let _ = std::fs::remove_dir_all(root);
}

// Selecting: a click selects, a double click or Enter opens.

/// A row of the list: the preview pane may repeat its name, as a label.
fn row<'a>(h: &'a Harness<'static, App>, name: &'a str) -> egui_kittest::Node<'a> {
    h.get_by_role_and_label(Role::Button, name)
}

fn click(h: &mut Harness<'static, App>, name: &str, modifiers: Modifiers) {
    row(h, name).click_modifiers(modifiers);
    settle(h);
}

/// A key through the keyboard walk's own press, then the picker settled.
fn press(h: &mut Harness<'static, App>, key: Key, modifiers: Modifiers) {
    walk::press(h, Press { modifiers, key });
    settle(h);
}

fn selected(h: &Harness<'static, App>) -> Vec<String> {
    let names = picker_state(h, "selected");
    let names = names.as_array().unwrap().iter();
    names
        .map(|p| {
            let name = Path::new(p.as_str().unwrap()).file_name().unwrap();
            name.to_string_lossy().into_owned()
        })
        .collect()
}

/// The line under the list, or an error: the live region may repeat it.
fn line(h: &Harness<'static, App>, text: &str) -> bool {
    h.query_by_role_and_label(Role::Label, text).is_some()
}

fn said(h: &Harness<'static, App>, text: &str) -> bool {
    a11y::announcements(&h.ctx).iter().any(|a| a.text == text)
}

fn highlighted(h: &Harness<'static, App>) -> String {
    let at = picker_state(h, "highlight").as_u64().unwrap() as usize;
    picker_state(h, "shown")[at].as_str().unwrap().to_string()
}

#[test]
fn a_click_selects_a_folder_and_enter_or_a_double_click_goes_in() {
    let root = tree("click-folder");
    let mut h = harness(None);
    let _p = choose(&mut h, FileRequest::open("Open").directory(&root));
    let dir = |h: &Harness<'static, App>| PathBuf::from(picker_state(h, "dir").as_str().unwrap());
    click(&mut h, "photos/", Modifiers::NONE);
    assert_eq!(dir(&h), root, "a click does not go in");
    assert_eq!(highlighted(&h), "photos/");
    press(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(dir(&h), root.join("photos"));
    press(&mut h, Key::ArrowUp, Modifiers::ALT);
    assert_eq!(dir(&h), root);
    double_click(&mut h, "notes/");
    settle(&mut h);
    assert_eq!(dir(&h), root.join("notes"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn choosing_one_file_has_no_selection_and_no_count() {
    let root = tree("one-file");
    let mut h = harness(None);
    let p = choose(
        &mut h,
        FileRequest::open("Open").directory(root.join("photos")),
    );
    assert!(h.query_all_by_label_contains("selected").next().is_none());
    // Modifiers change nothing: the highlight goes there, one at a time.
    click(&mut h, "a.png", Modifiers::COMMAND);
    click(&mut h, "c.txt", Modifiers::SHIFT);
    assert_eq!(highlighted(&h), "c.txt");
    press(&mut h, Key::ArrowUp, Modifiers::SHIFT);
    assert_eq!(highlighted(&h), "b.jpg");
    assert_eq!(selected(&h), Vec::<String>::new());
    // No such keys here: Ctrl+A stays the field's.
    type_in(&mut h, "jp");
    press(&mut h, Key::A, Modifiers::COMMAND);
    type_in(&mut h, "b");
    assert_eq!(field(&h), "b");
    press(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![root.join("photos/b.jpg")]))
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn ctrl_and_shift_clicks_select_several_and_the_count_says_it() {
    let root = tree("many-clicks");
    let mut h = harness(None);
    let p = choose(
        &mut h,
        FileRequest::open_many("Many").directory(root.join("photos")),
    );
    assert!(
        h.query_all_by_label_contains("Nothing selected")
            .next()
            .is_some()
    );
    // A click selects that one.
    click(&mut h, "a.png", Modifiers::NONE);
    assert!(line(&h, "Selected: 1"));
    assert!(said(&h, "Selected: 1"), "the count is announced");
    // Ctrl adds and takes away.
    click(&mut h, "c.txt", Modifiers::COMMAND);
    assert_eq!(selected(&h), ["a.png", "c.txt"]);
    assert!(line(&h, "Selected: 2"));
    let on = |h: &Harness<'static, App>, name| row(h, name).accesskit_node().toggled();
    assert_eq!(on(&h, "c.txt"), Some(Toggled::True));
    assert_eq!(on(&h, "b.jpg"), Some(Toggled::False));
    click(&mut h, "a.png", Modifiers::COMMAND);
    assert_eq!(selected(&h), ["c.txt"]);
    // Shift: from the last one clicked, the folders left out.
    click(&mut h, "c.txt", Modifiers::SHIFT);
    assert_eq!(selected(&h), ["a.png", "b.jpg", "c.txt"]);
    assert!(said(&h, "Selected: 3"));
    click(&mut h, "2026/", Modifiers::SHIFT);
    assert_eq!(selected(&h), ["a.png"], "the range is from the same anchor");
    // A plain click starts again; a folder cannot join, and says so.
    click(&mut h, "b.jpg", Modifiers::NONE);
    assert_eq!(selected(&h), ["b.jpg"]);
    click(&mut h, "c.txt", Modifiers::COMMAND);
    click(&mut h, "2026/", Modifiers::COMMAND);
    assert_eq!(selected(&h), ["b.jpg", "c.txt"]);
    assert!(h.query_by_label_contains("2026 is a folder").is_some());
    // The highlight is on the folder: Enter would go in, Ctrl+Enter chooses.
    press(&mut h, Key::Enter, Modifiers::COMMAND);
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![
            root.join("photos/b.jpg"),
            root.join("photos/c.txt")
        ]))
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_double_click_opens_that_file_alone() {
    let root = tree("many-double");
    let mut h = harness(None);
    let p = choose(
        &mut h,
        FileRequest::open_many("Many").directory(root.join("photos")),
    );
    click(&mut h, "a.png", Modifiers::NONE);
    double_click(&mut h, "c.txt");
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![root.join("photos/c.txt")]))
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_arrows_move_the_highlight_shift_extends_and_the_preview_follows() {
    let root = tree("many-keys");
    let mut h = harness(None);
    let p = choose(
        &mut h,
        FileRequest::open_many("Many").directory(root.join("photos")),
    );
    assert_eq!(highlighted(&h), "2026/");
    press(&mut h, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(highlighted(&h), "a.png");
    assert_eq!(selected(&h), Vec::<String>::new());
    press(&mut h, Key::ArrowDown, Modifiers::SHIFT);
    assert_eq!(selected(&h), ["a.png", "b.jpg"]);
    assert!(line(&h, "Selected: 2"));
    let preview = picker_state(&h, "preview");
    assert_eq!(
        preview["path"],
        root.join("photos/b.jpg").display().to_string()
    );
    // Plain arrows keep the selection, for Ctrl+Space further on.
    press(&mut h, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(highlighted(&h), "c.txt");
    assert_eq!(selected(&h), ["a.png", "b.jpg"]);
    press(&mut h, Key::Space, Modifiers::COMMAND);
    assert_eq!(selected(&h), ["a.png", "b.jpg", "c.txt"]);
    press(&mut h, Key::Space, Modifiers::COMMAND);
    assert_eq!(selected(&h), ["a.png", "b.jpg"]);
    // Shift, back up through the anchor Ctrl+Space set.
    press(&mut h, Key::ArrowUp, Modifiers::SHIFT);
    assert_eq!(selected(&h), ["b.jpg", "c.txt"]);
    press(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![
            root.join("photos/b.jpg"),
            root.join("photos/c.txt")
        ]))
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn ctrl_a_selects_what_is_shown_a_typed_path_keeps_the_key() {
    let root = tree("many-all");
    let mut h = harness(None);
    let _p = choose(
        &mut h,
        FileRequest::open_many("Many").directory(root.join("photos")),
    );
    let app = h.state().handle().clone();
    assert_eq!(app.keys_for(&CMD_SELECT_ALL), vec!["Ctrl+A"]);
    assert_eq!(app.keys_for(&CMD_TOGGLE_SELECTION), vec!["Ctrl+Space"]);
    // With the filter: what it leaves, and the filter's text is not selected.
    type_in(&mut h, "pn");
    press(&mut h, Key::A, Modifiers::COMMAND);
    assert_eq!(selected(&h), ["a.png"]);
    type_in(&mut h, "g");
    assert_eq!(field(&h), "png");
    // What the filter hides stays selected, and the count says so.
    press(&mut h, Key::Backspace, Modifiers::NONE);
    press(&mut h, Key::Backspace, Modifiers::NONE);
    press(&mut h, Key::Backspace, Modifiers::NONE);
    press(&mut h, Key::A, Modifiers::COMMAND);
    assert_eq!(selected(&h), ["a.png", "b.jpg", "c.txt"]);
    assert!(line(&h, "Selected: 3"));
    type_in(&mut h, "jp");
    assert!(line(&h, "Selected: 3 (2 not shown)"));
    // In a typed path, the key is the field's.
    let path = root.join("photos").display().to_string();
    h.get_by_label("Filter, or type a path").type_text("/");
    settle(&mut h);
    press(&mut h, Key::A, Modifiers::COMMAND);
    type_in(&mut h, &path);
    assert_eq!(
        field(&h),
        path,
        "the field's text was selected and replaced"
    );
    // Another folder: the selection stays behind; nothing to select is said.
    press(&mut h, Key::Enter, Modifiers::NONE);
    press(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(selected(&h), Vec::<String>::new());
    assert_eq!(
        picker_state(&h, "dir"),
        root.join("photos/2026").display().to_string()
    );
    press(&mut h, Key::A, Modifiers::COMMAND);
    assert!(line(&h, "No file is shown to select"));
    assert!(picker_state(&h, "error").is_string());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_selection_keys_are_commands_and_rebind() {
    let root = tree("many-rebind");
    let mut h = harness(None);
    let app = h.state().handle().clone();
    app.rebind(CMD_SELECT_ALL, Some("Ctrl+Shift+F".into()));
    settle(&mut h);
    // No picker open: the key is not taken from whoever else wants it.
    key(&mut h, Key::F, Modifiers::COMMAND | Modifiers::SHIFT);
    let traces = app.debug_snapshot()["keys"]["traces"].clone();
    assert_eq!(traces[0]["resolution"]["kind"], "nothing", "{traces}");
    let _p = choose(
        &mut h,
        FileRequest::open_many("Many").directory(root.join("photos")),
    );
    press(&mut h, Key::A, Modifiers::COMMAND);
    assert_eq!(selected(&h), Vec::<String>::new(), "no longer this");
    press(&mut h, Key::F, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(selected(&h).len(), 3);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn choosing_a_folder_says_what_selecting_means() {
    let root = tree("folder-select");
    let mut h = harness(None);
    let p = choose(&mut h, FileRequest::folder("Folder").directory(&root));
    let name = root.file_name().unwrap().to_string_lossy().into_owned();
    let shown = format!("Nothing selected: the button chooses the folder shown, {name}");
    assert!(line(&h, &shown));
    const PHOTOS: &str = "Selected: photos. The button chooses it, Enter goes into it";
    // A click selects, and the line says so; Ctrl+Space takes it back.
    click(&mut h, "photos/", Modifiers::NONE);
    assert_eq!(selected(&h), ["photos"]);
    assert!(line(&h, PHOTOS));
    assert!(said(&h, PHOTOS), "what it says is announced");
    press(&mut h, Key::Space, Modifiers::COMMAND);
    assert_eq!(selected(&h), Vec::<String>::new());
    assert!(line(&h, &shown));
    // The arrows select too, from the entry highlighted.
    press(&mut h, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(selected(&h), ["photos"]);
    // Enter goes in: the selection stays behind, and this folder is the choice.
    press(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(selected(&h), Vec::<String>::new());
    assert_eq!(
        picker_state(&h, "dir"),
        root.join("photos").display().to_string()
    );
    press(&mut h, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(selected(&h), ["2026"]);
    press(&mut h, Key::Enter, Modifiers::COMMAND);
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![root.join("photos/2026")]))
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn saving_a_selected_folder_goes_in_and_the_name_stays() {
    let root = tree("save-select");
    let mut h = saving_harness(None);
    let p = choose(
        &mut h,
        FileRequest::save("Save", "new.png").directory(&root),
    );
    // Selected, the button says what it does; a file's name fills the field.
    click(&mut h, "photos/", Modifiers::NONE);
    assert!(h.query_by_role_and_label(Role::Button, "Save").is_none());
    assert_eq!(field(&h), "new.png");
    h.get_by_role_and_label(Role::Button, "Open").click();
    settle(&mut h);
    assert_eq!(
        picker_state(&h, "dir"),
        root.join("photos").display().to_string()
    );
    assert_eq!(field(&h), "new.png");
    assert!(h.query_by_role_and_label(Role::Button, "Save").is_some());
    // The stem is selected again: typing replaces it.
    click(&mut h, "2026/", Modifiers::NONE);
    press(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(
        picker_state(&h, "dir"),
        root.join("photos/2026").display().to_string()
    );
    h.get_by_label("File name, or a path").type_text("dawn");
    settle(&mut h);
    assert_eq!(field(&h), "dawn.png");
    press(&mut h, Key::ArrowUp, Modifiers::ALT);
    click(&mut h, "c.txt", Modifiers::NONE);
    assert_eq!(field(&h), "c.txt");
    // An arrow onto a folder selects it; typing a name takes it back.
    press(&mut h, Key::ArrowUp, Modifiers::NONE);
    press(&mut h, Key::ArrowUp, Modifiers::NONE);
    press(&mut h, Key::ArrowUp, Modifiers::NONE);
    assert_eq!(selected(&h), ["2026"]);
    h.get_by_label("File name, or a path").type_text("x");
    settle(&mut h);
    assert_eq!(selected(&h), Vec::<String>::new());
    press(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(
        result(&mut h, &p),
        Ok(Chosen::Paths(vec![root.join("photos/x.txt")]))
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn tab_through_the_rows_moves_the_highlight_and_the_keys_answer_there() {
    let root = tree("rows-walk");
    let mut h = harness(None);
    let _p = choose(
        &mut h,
        FileRequest::open_many("Many").directory(root.join("photos")),
    );
    let stop = walk::reach(&mut h, walk::TAB, "a.png", 40).expect("Tab reaches the rows");
    assert_eq!(stop.role, Role::Button);
    settle(&mut h);
    assert_eq!(highlighted(&h), "a.png");
    // Space clicks the row: it selects, and the focus stays.
    press(&mut h, Key::Space, Modifiers::NONE);
    assert_eq!(selected(&h), ["a.png"]);
    assert_eq!(walk::focused(&h).unwrap().name, "a.png");
    // The arrows and Shift are the picker's from there, in the field again.
    press(&mut h, Key::ArrowDown, Modifiers::SHIFT);
    assert_eq!(selected(&h), ["a.png", "b.jpg"]);
    assert!(h.get_by_label("Filter, or type a path").is_focused());
    // Back on a folder's row, Enter goes in.
    let stop = walk::reach(&mut h, walk::SHIFT_TAB, "2026/", 40);
    assert!(stop.is_some(), "Shift+Tab reaches the folder");
    settle(&mut h);
    assert_eq!(highlighted(&h), "2026/");
    press(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(
        picker_state(&h, "dir"),
        root.join("photos/2026").display().to_string()
    );
    let _ = std::fs::remove_dir_all(root);
}

// The grid.

/// A provider of images that notes what it is asked for, and waits while
/// `hold` is set.
#[derive(Debug, Default)]
struct Raws {
    asked: std::sync::Mutex<Vec<PathBuf>>,
    hold: std::sync::atomic::AtomicBool,
}

impl PreviewProvider for Raws {
    fn id(&self) -> &str {
        "raws"
    }
    fn handles(&self) -> Handles {
        Handles::new().extensions(&["cr3"])
    }
    fn preview(&self, request: &PreviewRequest) -> Result<Option<Preview>, String> {
        self.asked.lock().unwrap().push(request.path.clone());
        while self.hold.load(Ordering::Relaxed) && !request.cancelled() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let image = egui::ColorImage::filled([12, 8], egui::Color32::GRAY);
        Ok(Some(Preview::new(Body::Image(Arc::new(image)))))
    }
}

/// `IMG_00000.cr3` and on, in `/raws` in memory, with a folder first.
fn raws(count: usize) -> SharedFs {
    let mut fs = MemoryFs::new().dir("/raws/Selects");
    for i in 0..count {
        fs = fs.file(format!("/raws/IMG_{i:05}.cr3"), vec![0; 16]);
    }
    Arc::new(fs)
}

/// A picker over `count` raws, in the view kept, its pane off.
fn grid_harness(count: usize, view: PickerView) -> (Harness<'static, App>, Arc<Raws>) {
    let app = App::builder()
        .plugin(FilesPlugin::in_app().file_system(raws(count)))
        .build(&egui::Context::default())
        .unwrap();
    let handle = app.handle().clone();
    let provider = Arc::new(Raws::default());
    let shared: Arc<dyn PreviewProvider> = provider.clone();
    handle.service(&FILES).unwrap().previews().register(shared);
    files::want_preview(&handle, false);
    files::want_view(&handle, view);
    let h = Harness::builder()
        .with_size(egui::vec2(900.0, 640.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    (h, provider)
}

/// Frames until the worker has made all that the grid has in view.
fn settle_thumbnails(h: &mut Harness<'static, App>) {
    for _ in 0..1000 {
        let quiet = h.try_run().is_ok();
        let previews = h.state().handle().debug_snapshot()["files"]["previews"].clone();
        if quiet && previews["queued"] == 0 && previews["running"].is_null() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("the thumbnails did not settle");
}

fn state(h: &Harness<'static, App>, what: &str) -> serde_json::Value {
    h.state().handle().debug_snapshot()["files"]["open"][0]["picker"][what].clone()
}

fn at(h: &Harness<'static, App>) -> u64 {
    state(h, "highlight").as_u64().unwrap()
}

#[test]
fn the_list_stays_the_view_at_first_and_has_no_grid_without_previews() {
    let (mut h, _) = grid_harness(3, PickerView::List);
    let app = h.state().handle().clone();
    assert_eq!(files::view(&app), PickerView::List);
    let _p = choose(&mut h, FileRequest::open("Open").directory("/raws"));
    assert_eq!(state(&h, "view"), "list");
    assert!(h.query_by_label("Thumbnail size").is_none());
    assert!(h.query_by_label("IMG_00000.cr3").is_some());
    // Left and Right are the field's alone in the list.
    h.get_by_label("Filter, or type a path").type_text("IMG");
    settle(&mut h);
    press(&mut h, Key::ArrowDown, Modifiers::NONE);
    let before = at(&h);
    press(&mut h, Key::ArrowLeft, Modifiers::NONE);
    press(&mut h, Key::Home, Modifiers::NONE);
    assert_eq!(at(&h), before);

    // A picker without previews offers no switch, and shows a list.
    let request = FileRequest::open("Open").directory("/raws");
    let mut picker = files::FilePicker::new_in(&request, raws(3));
    picker.view = PickerView::Grid;
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 640.0))
        .build_ui_state(
            |ui, picker: &mut files::FilePicker| {
                picker.show(ui, &files::Places::default());
            },
            picker,
        );
    h.run();
    assert!(h.query_by_label("Grid").is_none());
    assert!(h.query_by_label("IMG_00000.cr3").is_some());
}

#[test]
fn only_the_cells_in_view_ask_for_a_thumbnail_among_thousands() {
    let (mut h, raws) = grid_harness(5000, PickerView::Grid);
    let _p = choose(&mut h, FileRequest::open("Open").directory("/raws"));
    settle_thumbnails(&mut h);
    let asked = |raws: &Raws| raws.asked.lock().unwrap().clone();
    let first = asked(&raws);
    let columns = state(&h, "columns").as_u64().unwrap() as usize;
    assert!(columns > 1, "{columns} columns");
    assert!(!first.is_empty());
    // A dialog's worth, however many there are.
    assert!(first.len() <= 8 * columns, "{} asked", first.len());
    // What was asked is what is drawn; a cell far away is not.
    for path in &first {
        let name = path.file_name().unwrap().to_str().unwrap();
        assert!(h.query_by_label(name).is_some(), "{name} is not drawn");
    }
    assert!(h.query_by_label("IMG_04999.cr3").is_none());
    // Nothing more is asked while nothing moves.
    for _ in 0..5 {
        h.step();
    }
    settle_thumbnails(&mut h);
    assert_eq!(asked(&raws).len(), first.len());
    // Jumping to the end asks for the end, not for what lies between.
    press(&mut h, Key::End, Modifiers::NONE);
    settle_thumbnails(&mut h);
    let now = asked(&raws);
    assert!(h.query_by_label("IMG_04999.cr3").is_some());
    assert!(now.iter().any(|p| p.ends_with("IMG_04999.cr3")));
    assert!(
        now.len() <= first.len() * 2 + 8 * columns,
        "{} asked",
        now.len()
    );
    // In the same folder, the first cells come from the cache going back.
    press(&mut h, Key::Home, Modifiers::NONE);
    settle_thumbnails(&mut h);
    assert_eq!(
        asked(&raws).iter().filter(|p| **p == first[0]).count(),
        1,
        "already made"
    );
}

#[test]
fn a_placeholder_stands_in_and_the_frames_go_on_until_a_thumbnail_is_made() {
    let (mut h, raws) = grid_harness(40, PickerView::Grid);
    raws.hold.store(true, Ordering::Relaxed);
    let _p = choose(&mut h, FileRequest::open("Open").directory("/raws"));
    // The worker holds the first; every cell is drawn all the same, and
    // the keyboard answers.
    h.try_run().ok();
    assert!(h.query_by_label("IMG_00000.cr3").is_some());
    assert!(h.query_by_label("IMG_00001.cr3").is_some());
    press(&mut h, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(at(&h), 1);
    let app = h.state().handle().clone();
    let files_service = app.service(&FILES).unwrap();
    let path = Path::new("/raws/IMG_00000.cr3");
    let meta = files_service.file_system().metadata(path).unwrap();
    let previews = files_service.previews();
    assert!(matches!(previews.thumbnail(path, &meta), Shown::Loading));
    assert!(previews.describe()["queued"].as_u64() > Some(0));
    // Made once it is let go.
    raws.hold.store(false, Ordering::Relaxed);
    settle_thumbnails(&mut h);
    assert!(matches!(previews.thumbnail(path, &meta), Shown::Ready(_)));
}

#[test]
fn the_arrows_move_in_two_dimensions_home_and_end_and_enter_as_in_the_list() {
    let (mut h, _) = grid_harness(30, PickerView::Grid);
    let p = choose(&mut h, FileRequest::open("Open").directory("/raws"));
    settle_thumbnails(&mut h);
    let columns = state(&h, "columns").as_u64().unwrap();
    assert!(columns > 1);
    assert_eq!(at(&h), 0);
    // The folder comes first, then the files: 31 entries.
    press(&mut h, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(at(&h), 1);
    press(&mut h, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(at(&h), 1 + columns);
    press(&mut h, Key::ArrowLeft, Modifiers::NONE);
    assert_eq!(at(&h), columns);
    press(&mut h, Key::ArrowUp, Modifiers::NONE);
    assert_eq!(at(&h), 0);
    press(&mut h, Key::End, Modifiers::NONE);
    assert_eq!(at(&h), 30);
    assert_eq!(highlighted(&h), "IMG_00029.cr3");
    press(&mut h, Key::Home, Modifiers::NONE);
    assert_eq!(at(&h), 0);
    // Enter goes into a folder...
    assert_eq!(highlighted(&h), "Selects/");
    press(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(state(&h, "dir"), "/raws/Selects");
    press(&mut h, Key::Backspace, Modifiers::NONE);
    settle_thumbnails(&mut h);
    // ...and chooses a file.
    press(&mut h, Key::End, Modifiers::NONE);
    press(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(
        result(&mut h, &p).unwrap(),
        Chosen::Paths(vec![PathBuf::from("/raws/IMG_00029.cr3")])
    );
}

#[test]
fn shift_extends_the_selection_in_the_grid() {
    let (mut h, _) = grid_harness(20, PickerView::Grid);
    let _p = choose(&mut h, FileRequest::open_many("Many").directory("/raws"));
    settle_thumbnails(&mut h);
    press(&mut h, Key::ArrowRight, Modifiers::NONE);
    press(&mut h, Key::ArrowRight, Modifiers::SHIFT);
    assert_eq!(selected(&h), ["IMG_00000.cr3", "IMG_00001.cr3"]);
    let columns = state(&h, "columns").as_u64().unwrap() as usize;
    press(&mut h, Key::ArrowDown, Modifiers::SHIFT);
    assert_eq!(selected(&h).len(), columns + 2);
    press(&mut h, Key::End, Modifiers::SHIFT);
    assert_eq!(selected(&h).len(), 20);
}

#[test]
fn the_field_keeps_the_keys_that_edit_its_text() {
    let (mut h, _) = grid_harness(20, PickerView::Grid);
    let _p = choose(&mut h, FileRequest::open("Open").directory("/raws"));
    settle_thumbnails(&mut h);
    type_in(&mut h, "IMG");
    assert_eq!(at(&h), 0);
    press(&mut h, Key::ArrowRight, Modifiers::NONE);
    press(&mut h, Key::End, Modifiers::NONE);
    assert_eq!(at(&h), 0, "the cursor in the text moved, not the highlight");
    // Up and Down have no use there: they move the highlight, as in the list.
    press(&mut h, Key::ArrowDown, Modifiers::NONE);
    assert!(at(&h) > 0);
}

#[test]
fn tab_reaches_a_cell_and_the_focus_follows_the_arrows_through_a_long_grid() {
    let (mut h, _) = grid_harness(2000, PickerView::Grid);
    let _p = choose(&mut h, FileRequest::open("Open").directory("/raws"));
    settle_thumbnails(&mut h);
    let stop = walk::reach(&mut h, walk::TAB, "IMG_00000.cr3", 60).expect("Tab reaches the cells");
    assert_eq!(stop.role, Role::Button);
    settle_thumbnails(&mut h);
    assert_eq!(highlighted(&h), "IMG_00000.cr3");
    press(&mut h, Key::ArrowRight, Modifiers::NONE);
    settle_thumbnails(&mut h);
    assert_eq!(highlighted(&h), "IMG_00001.cr3");
    assert_eq!(walk::focused(&h).unwrap().name, "IMG_00001.cr3");
    // Far away: the cell is not drawn when the key is pressed, and has the
    // focus once it is, in view.
    press(&mut h, Key::End, Modifiers::NONE);
    settle_thumbnails(&mut h);
    let stop = walk::focused(&h).expect("the focus followed");
    assert_eq!(stop.name, "IMG_01999.cr3");
    assert!(stop.visible, "{stop}");
    // Enter there, as in the list.
    press(&mut h, Key::Home, Modifiers::NONE);
    settle_thumbnails(&mut h);
    assert_eq!(highlighted(&h), "Selects/");
    assert_eq!(walk::focused(&h).unwrap().name, "Selects/");
    press(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(state(&h, "dir"), "/raws/Selects");
    assert!(h.get_by_label("Filter, or type a path").is_focused());
}

#[test]
fn a_click_selects_a_double_click_opens_in_the_grid() {
    let (mut h, _) = grid_harness(6, PickerView::Grid);
    let p = choose(&mut h, FileRequest::open("Open").directory("/raws"));
    settle_thumbnails(&mut h);
    h.get_by_label("IMG_00002.cr3").click();
    settle_thumbnails(&mut h);
    assert_eq!(highlighted(&h), "IMG_00002.cr3");
    assert!(p.take().is_none(), "a click does not open");
    // Back to the field, whose keys go on.
    assert!(h.get_by_label("Filter, or type a path").is_focused());
    let pos = h.get_by_label("IMG_00003.cr3").rect().center();
    {
        let events = &mut h.input_mut().events;
        events.push(egui::Event::PointerMoved(pos));
        for _ in 0..2 {
            for pressed in [true, false] {
                events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                });
            }
        }
    }
    assert_eq!(
        result(&mut h, &p).unwrap(),
        Chosen::Paths(vec![PathBuf::from("/raws/IMG_00003.cr3")])
    );
}

#[test]
fn the_view_and_the_size_are_kept_for_the_next_picker() {
    let (mut h, _) = grid_harness(40, PickerView::List);
    let app = h.state().handle().clone();
    let _p = choose(&mut h, FileRequest::open("Open").directory("/raws"));
    h.get_by_label("Grid").click();
    settle_thumbnails(&mut h);
    assert_eq!(state(&h, "view"), "grid");
    assert_eq!(files::view(&app), PickerView::Grid);
    assert_eq!(files::thumbnail_size(&app), files::THUMBNAIL_DEFAULT_SIZE);
    let before = state(&h, "columns").as_u64().unwrap();
    // Ctrl and + or -, and Ctrl and the wheel (which egui reads as a zoom).
    press(&mut h, Key::Plus, Modifiers::COMMAND);
    settle_thumbnails(&mut h);
    let bigger = files::thumbnail_size(&app);
    assert!(bigger > files::THUMBNAIL_DEFAULT_SIZE);
    assert!(state(&h, "columns").as_u64().unwrap() < before);
    press(&mut h, Key::Minus, Modifiers::COMMAND);
    settle_thumbnails(&mut h);
    assert!((files::thumbnail_size(&app) - files::THUMBNAIL_DEFAULT_SIZE).abs() < 0.01);
    let over = h.get_by_label("IMG_00000.cr3").rect().center();
    h.hover_at(over);
    h.input_mut().events.push(egui::Event::Zoom(0.8));
    settle_thumbnails(&mut h);
    assert!(files::thumbnail_size(&app) < files::THUMBNAIL_DEFAULT_SIZE);
    // A slider says it too, and the size stays within its limits.
    let slider = h.get_by_role_and_label(Role::Slider, "Thumbnail size");
    assert!(slider.rect().width() > 0.0);
    for _ in 0..40 {
        press(&mut h, Key::Minus, Modifiers::COMMAND);
    }
    settle_thumbnails(&mut h);
    assert_eq!(files::thumbnail_size(&app), files::THUMBNAIL_MIN_SIZE);
    for _ in 0..40 {
        press(&mut h, Key::Plus, Modifiers::COMMAND);
    }
    settle_thumbnails(&mut h);
    assert_eq!(files::thumbnail_size(&app), files::THUMBNAIL_MAX_SIZE);
    // A second picker opens as the first was left.
    key(&mut h, Key::Escape, Modifiers::NONE);
    key(&mut h, Key::Escape, Modifiers::NONE);
    settle(&mut h);
    assert!(app.service(&FILES).unwrap().open_ids().is_empty());
    let _p = choose(&mut h, FileRequest::open("Open").directory("/raws"));
    assert_eq!(state(&h, "view"), "grid");
    assert_eq!(
        state(&h, "thumbnail_size").as_f64(),
        Some(f64::from(files::THUMBNAIL_MAX_SIZE))
    );
    // Back to the list: the grid's queue is dropped.
    h.get_by_label("List").click();
    settle_thumbnails(&mut h);
    assert_eq!(files::view(&app), PickerView::List);
    assert!(h.query_by_label("Thumbnail size").is_none());
    assert_eq!(
        h.state().handle().debug_snapshot()["files"]["previews"]["queued"],
        0
    );
}

// The columns and the order.

/// A folder of files whose size and date disagree with their names.
fn sortable() -> SharedFs {
    let at = |s| Some(UNIX_EPOCH + Duration::from_secs(s));
    Arc::new(
        MemoryFs::new()
            .file_at("/d/IMG_10.jpg", vec![0; 3000], at(100))
            .file_at("/d/IMG_9.jpg", vec![0; 10], at(300))
            .file_at("/d/notes.txt", vec![0; 500], at(200))
            .file("/d/README", vec![0; 1])
            .dir("/d/Trips")
            .dir("/d/Albums"),
    )
}

fn sort_harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugin(FilesPlugin::in_app().file_system(sortable()))
        .build(&egui::Context::default())
        .unwrap();
    // The window holds the dialog: without the pane, all the columns fit.
    files::want_preview(app.handle(), false);
    Harness::builder()
        .with_size(egui::vec2(900.0, 640.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app)
}

fn shown_names(h: &Harness<'static, App>) -> Vec<String> {
    let names = picker_state(h, "shown");
    let names = names.as_array().unwrap().iter();
    names.map(|n| n.as_str().unwrap().to_string()).collect()
}

fn header_click(h: &mut Harness<'static, App>, name: &str) {
    row(h, name).click();
    settle(h);
}

#[test]
fn names_read_naturally_with_folders_first() {
    let mut h = sort_harness();
    let _p = choose(&mut h, FileRequest::open("Open").directory("/d"));
    assert_eq!(
        shown_names(&h),
        [
            "Albums/",
            "Trips/",
            "IMG_9.jpg",
            "IMG_10.jpg",
            "notes.txt",
            "README"
        ]
    );
    assert_eq!(picker_state(&h, "sort"), "name");
    assert_eq!(picker_state(&h, "folders_first"), true);
    // Ascending by name at first: the arrow's name says so.
    assert!(
        h.query_by_role_and_label(Role::Button, "Name, sorted ascending")
            .is_some()
    );
    egui_pop_test::assert_all_named(&h);
}

#[test]
fn a_header_sorts_turns_and_is_kept_for_the_next_picker() {
    let mut h = sort_harness();
    let app = h.state().handle().clone();
    let p = choose(&mut h, FileRequest::open("Open").directory("/d"));
    // The newest first, folders still first (they have no time here).
    header_click(&mut h, "Modified");
    assert_eq!(
        shown_names(&h),
        [
            "Albums/",
            "Trips/",
            "IMG_9.jpg",
            "notes.txt",
            "IMG_10.jpg",
            "README"
        ]
    );
    assert!(
        h.query_by_role_and_label(Role::Button, "Modified, sorted descending")
            .is_some()
    );
    assert_eq!(files::sort_order(&app).as_text(), "-modified");
    // The same header again turns it; what has no time stays last.
    header_click(&mut h, "Modified, sorted descending");
    assert_eq!(
        shown_names(&h)[2..],
        ["IMG_10.jpg", "notes.txt", "IMG_9.jpg", "README"]
    );
    assert_eq!(files::sort_order(&app).as_text(), "modified");
    // The largest first.
    header_click(&mut h, "Size");
    assert_eq!(
        shown_names(&h)[2..],
        ["IMG_10.jpg", "notes.txt", "IMG_9.jpg", "README"]
    );
    header_click(&mut h, "Size, sorted descending");
    assert_eq!(
        shown_names(&h)[2..],
        ["README", "IMG_9.jpg", "notes.txt", "IMG_10.jpg"]
    );
    // By extension, those without one first.
    header_click(&mut h, "Type");
    assert_eq!(
        shown_names(&h)[2..],
        ["README", "IMG_9.jpg", "IMG_10.jpg", "notes.txt"]
    );
    assert_eq!(files::sort_order(&app).as_text(), "type");
    // A click on a header leaves the keys where they were.
    assert!(h.get_by_label("Filter, or type a path").is_focused());

    cancel(&mut h, &p);
    let _p = choose(&mut h, FileRequest::open("Open").directory("/d"));
    assert_eq!(picker_state(&h, "sort"), "type");
    assert!(
        h.query_by_role_and_label(Role::Button, "Type, sorted ascending")
            .is_some()
    );
}

#[test]
fn nonsense_in_the_store_is_the_order_by_name() {
    let mut h = sort_harness();
    let app = h.state().handle().clone();
    assert_eq!(files::sort_order(&app).as_text(), "name");
    let store = app.store();
    egui_pop_core::lock(&store)
        .set(
            Set::new(
                files::SORT,
                Change::To(Value::Text("colour".into())),
                Origin::User,
            )
            .by(&OwnerId::FRAMEWORK),
        )
        .unwrap();
    let _p = choose(&mut h, FileRequest::open("Open").directory("/d"));
    assert_eq!(picker_state(&h, "sort"), "name");
}

#[test]
fn the_highlight_stays_on_its_entry_through_a_new_order() {
    let mut h = sort_harness();
    let _p = choose(&mut h, FileRequest::open("Open").directory("/d"));
    click(&mut h, "notes.txt", Modifiers::NONE);
    assert_eq!(highlighted(&h), "notes.txt");
    let before = picker_state(&h, "highlight");
    header_click(&mut h, "Name, sorted ascending");
    assert_eq!(highlighted(&h), "notes.txt");
    assert_ne!(picker_state(&h, "highlight"), before);
}

#[test]
fn folders_first_is_a_setting_the_box_turns() {
    let mut h = sort_harness();
    let app = h.state().handle().clone();
    assert!(files::folders_first(&app));
    let p = choose(&mut h, FileRequest::open("Open").directory("/d"));
    h.get_by_role_and_label(Role::CheckBox, "Folders first")
        .click();
    settle(&mut h);
    assert!(!files::folders_first(&app));
    assert_eq!(
        shown_names(&h),
        [
            "Albums/",
            "IMG_9.jpg",
            "IMG_10.jpg",
            "notes.txt",
            "README",
            "Trips/"
        ]
    );
    cancel(&mut h, &p);
    // Kept, and the setting itself moves an open picker.
    let _p = choose(&mut h, FileRequest::open("Open").directory("/d"));
    assert_eq!(picker_state(&h, "folders_first"), false);
    files::want_folders_first(&app, true);
    settle(&mut h);
    assert_eq!(shown_names(&h)[..2], ["Albums/", "Trips/"]);
}

#[test]
fn a_row_says_its_facts_and_a_folder_has_no_size() {
    let mut h = sort_harness();
    let _p = choose(&mut h, FileRequest::open("Open").directory("/d"));
    let description =
        |h: &Harness<'static, App>, name: &str| row(h, name).accesskit_node().description();
    assert_eq!(
        description(&h, "IMG_10.jpg").as_deref(),
        Some("1970-01-01 00:01 UTC, 3.0 KB, JPG")
    );
    assert_eq!(description(&h, "README").as_deref(), Some("1 B, File"));
    assert_eq!(description(&h, "Albums/").as_deref(), Some("Folder"));
}

#[test]
fn choosing_a_folder_has_no_columns_that_would_say_nothing() {
    let mut h = sort_harness();
    let _p = choose(&mut h, FileRequest::folder("Folder").directory("/d"));
    assert!(
        h.query_by_role_and_label(Role::Button, "Modified")
            .is_some()
    );
    assert!(h.query_by_role_and_label(Role::Button, "Size").is_none());
    assert!(h.query_by_role_and_label(Role::Button, "Type").is_none());
}

#[test]
fn the_grid_follows_the_same_order_and_its_headers_sort() {
    let app = App::builder()
        .plugin(FilesPlugin::in_app().file_system(sortable()))
        .build(&egui::Context::default())
        .unwrap();
    let handle = app.handle().clone();
    files::want_view(&handle, PickerView::Grid);
    files::want_preview(&handle, false);
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 640.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    let _p = choose(&mut h, FileRequest::open("Open").directory("/d"));
    assert_eq!(picker_state(&h, "view"), "grid");
    header_click(&mut h, "Size");
    assert_eq!(
        shown_names(&h)[2..],
        ["IMG_10.jpg", "notes.txt", "IMG_9.jpg", "README"]
    );
    // The cells lie in that order.
    let x = |h: &Harness<'static, App>, name: &str| row(h, name).rect().left();
    assert!(x(&h, "IMG_10.jpg") < x(&h, "notes.txt"));
    assert!(x(&h, "notes.txt") < x(&h, "IMG_9.jpg"));
    assert_eq!(files::sort_order(&handle).as_text(), "-size");
    // The list's keeps it.
    h.get_by_label("List").click();
    settle(&mut h);
    assert_eq!(picker_state(&h, "view"), "list");
    assert!(row(&h, "IMG_10.jpg").rect().top() < row(&h, "notes.txt").rect().top());
}

#[test]
fn while_the_filter_ranks_the_headers_say_why_they_do_not_sort() {
    let mut h = sort_harness();
    let _p = choose(&mut h, FileRequest::open("Open").directory("/d"));
    type_in(&mut h, "img");
    assert!(row(&h, "Size").accesskit_node().is_disabled());
    // Not the order's arrow: the entries are not in that order now.
    assert!(row(&h, "Name").accesskit_node().is_disabled());
    row(&h, "Size").click();
    settle(&mut h);
    assert_eq!(picker_state(&h, "sort"), "name");
    // Emptied, they sort again.
    h.get_by_label("Filter, or type a path").focus();
    settle(&mut h);
    for _ in 0..3 {
        key(&mut h, Key::Backspace, Modifiers::NONE);
    }
    assert_eq!(field(&h), "");
    header_click(&mut h, "Size");
    assert_eq!(picker_state(&h, "sort"), "-size");
}

/// A memory folder that counts what is read of it.
#[derive(Debug)]
struct Counted {
    fs: MemoryFs,
    reads: AtomicUsize,
}

impl FileSystem for Counted {
    fn read_dir(&self, dir: &Path) -> std::io::Result<Vec<DirItem>> {
        self.fs.read_dir(dir)
    }
    fn metadata(&self, path: &Path) -> std::io::Result<Meta> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        self.fs.metadata(path)
    }
    fn read(&self, path: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
        self.fs.read(path, limit)
    }
}

#[test]
fn a_long_folder_reads_the_facts_it_shows_and_each_once() {
    let mut fs = MemoryFs::new().dir("/raws");
    for i in 0..2000 {
        fs = fs.file(format!("/raws/IMG_{i:05}.cr3"), vec![0; i % 7]);
    }
    let counted = Arc::new(Counted {
        fs,
        reads: AtomicUsize::new(0),
    });
    let reads = counted.clone();
    let request = FileRequest::open("Open").directory("/raws");
    let picker = files::FilePicker::new_in(&request, counted);
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 640.0))
        .with_max_steps(64)
        .build_ui_state(
            |ui, picker: &mut files::FilePicker| {
                picker.show(ui, &files::Places::default());
            },
            picker,
        );
    h.run();
    let read = || reads.reads.load(Ordering::Relaxed);
    // Only the rows in view are drawn, and only they read.
    let (first, rows) = (read(), h.get_all_by_role(Role::Button).count());
    assert!(rows < 100, "{rows} widgets for 2000 rows");
    assert!(
        first > 0 && first < 100,
        "{first} reads for the rows in view"
    );
    assert!(h.query_by_label("IMG_00000.cr3").is_some());
    assert!(h.query_by_label("IMG_01999.cr3").is_none());
    // More frames read nothing more.
    h.run();
    assert_eq!(read(), first);
    // Sorting by size reads the rest, once.
    let size = files::sort::SortColumn::Size;
    h.state_mut()
        .set_sort(files::sort::PickerSort::default().click(size));
    h.run();
    let sorted = read();
    assert!((2000..2000 + first).contains(&sorted), "{sorted} reads");
    // Turning it reads nothing: the facts are known, the folder too.
    let turned = h.state().sort.click(size);
    h.state_mut().set_sort(turned);
    h.run();
    assert_eq!(read(), sorted);
}

#[test]
fn the_arrows_bring_a_row_that_was_not_drawn_into_view() {
    let (mut h, _) = grid_harness(2000, PickerView::List);
    let _p = choose(&mut h, FileRequest::open("Open").directory("/raws"));
    assert!(h.query_by_label("IMG_00200.cr3").is_none());
    for _ in 0..200 {
        h.key_press(Key::ArrowDown);
    }
    settle(&mut h);
    assert_eq!(at(&h), 200);
    let at = row(&h, "IMG_00200.cr3").rect();
    let top = row(&h, "Name, sorted ascending").rect().bottom();
    assert!(at.top() >= top - 1.0, "{at:?} under the headers at {top}");
    assert!(h.query_by_label("IMG_00000.cr3").is_none());
    // Back up, the same way.
    for _ in 0..200 {
        h.key_press(Key::ArrowUp);
    }
    settle(&mut h);
    assert!(h.query_by_label("IMG_00000.cr3").is_some());
}

#[test]
fn at_the_least_size_the_row_of_buttons_keeps_its_boxes() {
    let root = tree("least-footer");
    let mut h = large_harness();
    let _p = choose(
        &mut h,
        FileRequest::open("Open")
            .directory(&root)
            .filter("Images", &["png", "jpg"]),
    );
    let at = dialog(&h);
    drag(
        &mut h,
        at.max - egui::vec2(2.0, 2.0),
        egui::vec2(-2000.0, -2000.0),
    );
    let least = dialog(&h);
    assert!(least.width() < at.width(), "{least:?}");
    let boxes = h.get_by_role_and_label(Role::CheckBox, "Preview").rect();
    let cancel = h.get_by_role_and_label(Role::Button, "Cancel").rect();
    assert!(
        boxes.right() <= cancel.left(),
        "{boxes:?} runs into {cancel:?}"
    );
    let open = h.get_by_role_and_label(Role::Button, "Open").rect();
    assert!(open.right() <= least.right(), "{open:?} out of {least:?}");
    let _ = std::fs::remove_dir_all(root);
}

const FIELD: &str = "Filter, or type a path";

fn focused_name(h: &Harness<'static, App>) -> String {
    walk::focused(h).map(|s| s.name).unwrap_or_default()
}

#[test]
fn a_press_on_the_view_gives_the_keyboard_back_to_the_field() {
    let (mut h, _) = grid_harness(3, PickerView::List);
    let _p = choose(&mut h, FileRequest::open("Open").directory("/raws"));
    click(&mut h, "Grid", Modifiers::NONE);
    assert_eq!(state(&h, "view"), "grid");
    assert!(h.get_by_label(FIELD).is_focused());
    h.event(egui::Event::Text("img".into()));
    settle(&mut h);
    assert_eq!(field(&h), "img");
    // A press on what is already on does the same.
    click(&mut h, "Grid", Modifiers::NONE);
    assert!(h.get_by_label(FIELD).is_focused());
    click(&mut h, "List", Modifiers::NONE);
    assert!(h.get_by_label(FIELD).is_focused());
    // By the keyboard, the focus stays where it is, to go on from there.
    walk::reach(&mut h, walk::SHIFT_TAB, "Grid", 60).expect("Shift+Tab reaches the switch");
    press(&mut h, Key::Space, Modifiers::NONE);
    assert_eq!(state(&h, "view"), "grid");
    assert_eq!(focused_name(&h), "Grid");
}

#[test]
fn a_press_on_up_gives_the_keyboard_back_to_the_field() {
    let (mut h, _) = grid_harness(3, PickerView::List);
    let _p = choose(&mut h, FileRequest::open("Open").directory("/raws"));
    click(&mut h, "Up", Modifiers::NONE);
    assert_eq!(picker_state(&h, "dir"), "/");
    assert!(h.get_by_label(FIELD).is_focused());
}
