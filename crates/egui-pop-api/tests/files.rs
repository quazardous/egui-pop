//! The file chooser: the in-app picker driven by the keyboard (filter,
//! arrows, Enter, a typed path with completion, bookmarks, Escape), its
//! result through a `Pending`, and the `file.choose` and `file.chosen`
//! steps.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::dialogs::{self, DIALOGS};
use egui_pop_api::files::{
    self, CMD_NEW_FOLDER, Chosen, FILES, FileRequest, FilesPlugin, STEP_CHOOSE, STEP_CHOSEN,
};
use egui_pop_api::host::Pending;
use egui_pop_api::steps::Before;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::ids::PluginId;
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
    h.get_by_label("2026/").click();
    settle(&mut h);
    assert_eq!(
        f.picker(f.open_ids()[0]).unwrap().dir,
        root.join("photos/2026")
    );
    h.get_by_label("Up").click();
    settle(&mut h);
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
