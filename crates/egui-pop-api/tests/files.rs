//! The file chooser: the in-app picker driven by the keyboard (filter,
//! arrows, Enter, a typed path with completion, bookmarks, Escape), its
//! result through a `Pending`, and the `file.choose` and `file.chosen`
//! steps.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::files::{
    self, Chosen, FILES, FileRequest, FilesPlugin, STEP_CHOOSE, STEP_CHOSEN,
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

fn choose(h: &mut Harness<'static, App>, request: FileRequest) -> Pending {
    let app = h.state().handle().clone();
    let f = app.service(&FILES).unwrap();
    let p = f.choose(&app, request);
    // A modal is placed once its size is known: a few frames.
    for _ in 0..3 {
        h.run();
    }
    p
}

fn result(h: &mut Harness<'static, App>, p: &Pending) -> Result<Chosen, String> {
    for _ in 0..4 {
        h.run();
        if let Some(r) = p.take() {
            return files::chosen(&r);
        }
    }
    panic!("no result");
}

fn key(h: &mut Harness<'static, App>, key: Key, modifiers: Modifiers) {
    h.key_press_modifiers(modifiers, key);
    h.run();
}

fn type_in(h: &mut Harness<'static, App>, text: &str) {
    h.get_by_label("Filter, or type a path").type_text(text);
    h.run();
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
    h.run();
    assert!(h.query_by_label("Remove the bookmark").is_some());
    // The place takes back to the folder from elsewhere.
    key(&mut h, Key::ArrowUp, Modifiers::ALT);
    h.get_by_label("photos").click();
    h.run();
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
    h.run();
    let f = h.state().handle().service(&FILES).unwrap();
    assert_eq!(f.picker(f.open_ids()[0]).unwrap().highlight, 2);
    h.get_by_label("2026/").click();
    h.run();
    assert_eq!(
        f.picker(f.open_ids()[0]).unwrap().dir,
        root.join("photos/2026")
    );
    h.get_by_label("Up").click();
    h.run();
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
            h.run();
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
    h.run();
    h.drag_at(from);
    h.run();
    for i in 1..=4 {
        h.hover_at(from + by * (i as f32 / 4.0));
        h.run();
    }
    h.drop_at(from + by);
    h.run();
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
        h.run();
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
        h.run();
    }
    assert!(
        h.ctx.content_rect().contains_rect(dialog(&h)),
        "{:?}",
        dialog(&h)
    );
    assert_eq!(files::dialog_size(&app), egui::vec2(1100.0, 700.0));
    h.set_size(egui::vec2(1280.0, 800.0));
    for _ in 0..3 {
        h.run();
    }
    assert_eq!(dialog(&h).size(), big.size());
    // Opening in a window too small for it: back to the default.
    key(&mut h, Key::Escape, Modifiers::NONE);
    key(&mut h, Key::Escape, Modifiers::NONE);
    for _ in 0..3 {
        h.run();
    }
    h.set_size(egui::vec2(640.0, 480.0));
    h.run();
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
    h.run();
    assert_eq!(pane_value(&h), f64::from(files::preview::PANE_WIDTH));
    assert_eq!(files::pane_width(&app), files::preview::PANE_WIDTH);
    // Another width is there for the next picker.
    files::want_pane_width(&app, 300.0);
    key(&mut h, Key::Escape, Modifiers::NONE);
    key(&mut h, Key::Escape, Modifiers::NONE);
    for _ in 0..3 {
        h.run();
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
        h.run();
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
    h.run();
    key(&mut h, Key::ArrowRight, Modifiers::NONE);
    key(&mut h, Key::ArrowDown, Modifiers::NONE);
    h.run();
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
    h.run();
    assert!(!files::preview_wanted(&app));
    assert!(
        h.query_by_role_and_label(Role::Splitter, SPLITTER)
            .is_none()
    );
    assert_eq!(dialog(&h).size(), size);
    key(&mut h, Key::P, Modifiers::ALT);
    h.run();
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
