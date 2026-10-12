//! The file chooser: the in-app picker driven by the keyboard (filter,
//! arrows, Enter, a typed path with completion, bookmarks, Escape), its
//! result through a `Pending`, and the `file.choose` and `file.chosen`
//! steps.

use std::path::{Path, PathBuf};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
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
    assert!(h.query_by_label("todo.md").is_some());
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
