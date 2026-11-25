//! Dialogs and file choices on the bus (ADR 0029, slice 5): a dialog
//! coming up is its `pop.popup.open`, a file choice started its `pop.popup.open`
//! (or a native window's `pop.window.open` for the system's chooser), each
//! ending by its close. A window closed with a dialog and a file choice
//! open closes both first, each by its own `pop.popup.close` (reason
//! `parent-closed`), their owners cancelling them in that call: no child
//! is left in `windows.list`.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use egui::ViewportId;
use egui_pop_api::bus::{Reason, WindowKind, WindowRequest};
use egui_pop_api::dialogs::{self, Confirm, DIALOGS, Info};
use egui_pop_api::files::{self, FILES, FileRequest};
use egui_pop_api::host::{Event, Pending};
use egui_pop_api::windows::{WindowId, WindowSpec};
use egui_pop_api::{App, AppHandle, DefaultPlugins};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::CommandId;
use egui_pop_test::windows::MultiWindow;

const IMPORT: &str = "import";
const DONE: CommandId = CommandId::from_static("demo.done");

fn multi() -> MultiWindow {
    let mut m = MultiWindow::new(|ctx| {
        App::builder()
            .name("Demo")
            .plugins(DefaultPlugins)
            .build(ctx)
            .unwrap()
    });
    m.root_pass();
    m
}

fn events(app: &AppHandle, name: &str) -> Vec<Event> {
    app.events_since(Default::default())
        .into_iter()
        .filter(|e| e.name == name)
        .collect()
}

/// Where the first event named `name` about `window` is in the journal.
fn said_at(app: &AppHandle, name: &str, window: &str) -> Option<usize> {
    app.events_since(Default::default())
        .iter()
        .position(|e| e.name == name && e.data["window"] == window)
}

fn listed(app: &AppHandle) -> Vec<(String, WindowKind, Option<String>)> {
    app.windows_list()
        .into_iter()
        .map(|w| (w.id, w.kind, w.parent))
        .collect()
}

/// What the import window asked in its first pass.
#[derive(Default)]
struct Asked {
    dialog: Option<u64>,
    /// The dialogs opened after the first: they wait behind it.
    waiting: Vec<u64>,
    choice: Option<Pending>,
}

/// Opens the import window, whose first pass opens a dialog and starts a
/// file choice (`dialog`, `choice`), and draws it until both are on
/// screen: the import window's viewport and what it asked.
fn import_with(
    m: &mut MultiWindow,
    dialog: bool,
    choice: bool,
) -> (ViewportId, Rc<RefCell<Asked>>) {
    import_with_dialogs(m, usize::from(dialog), choice)
}

/// As [`import_with`], with `dialogs` dialogs opened, one after another.
fn import_with_dialogs(
    m: &mut MultiWindow,
    dialogs: usize,
    choice: bool,
) -> (ViewportId, Rc<RefCell<Asked>>) {
    let asked: Rc<RefCell<Asked>> = Rc::default();
    let a = asked.clone();
    let app = m.app().clone();
    let mut first = true;
    m.app()
        .open_window(WindowSpec::new(IMPORT, "Import"), move |ui, _| {
            ui.label("import");
            if !std::mem::take(&mut first) {
                return;
            }
            let mut a = a.borrow_mut();
            let d = app.service(&DIALOGS).unwrap();
            for i in 0..dialogs {
                if i == 0 {
                    a.dialog =
                        Some(d.open(Confirm::new("Import 3 photos?", "Import 3 photos", DONE)));
                } else {
                    a.waiting
                        .push(d.open(Info::new("Done", "3 photos imported")));
                }
            }
            if choice {
                let f = app.service(&FILES).unwrap();
                a.choice = Some(f.choose(&app, FileRequest::folder("Choose a folder…")));
            }
        });
    m.root_pass();
    let v = m.app().window_viewport(&IMPORT.into()).unwrap();
    m.window_pass(v);
    m.window_pass(v);
    m.root_pass();
    (v, asked)
}

fn dialog_popup(id: u64) -> String {
    format!("{}{id}", dialogs::POPUP_PREFIX)
}

fn choice_popup(app: &AppHandle) -> String {
    let id = app.service(&FILES).unwrap().open_ids()[0];
    format!("{}{id}", files::POPUP_PREFIX)
}

/// The import window closed: its dialog and its choice each closed first
/// by their `pop.popup.close` (`parent-closed`), cancelled, gone from the
/// registry, before `window.closed`; no invariant broken.
fn assert_children_closed_first(
    app: &AppHandle,
    dialog: &str,
    choice: &str,
    asked: &Rc<RefCell<Asked>>,
) {
    for popup in [dialog, choice] {
        let closed = events(app, "popup.closed");
        let e = closed
            .iter()
            .find(|e| e.data["window"] == popup)
            .unwrap_or_else(|| panic!("no popup.closed for {popup}: {closed:?}"));
        assert_eq!(e.data["reason"], "parent-closed");
        assert_eq!(e.data["parent"], IMPORT);
        assert!(
            said_at(app, "popup.closed", popup) < said_at(app, "window.closed", IMPORT),
            "{popup} closed before its window"
        );
    }
    assert_eq!(
        listed(app),
        [("main".to_string(), WindowKind::Main, None)],
        "no child left"
    );
    // Their owners cancelled them in the same call.
    let d = app.service(&DIALOGS).unwrap();
    assert!(d.is_empty());
    assert!(
        events(app, "dialog.cancelled")
            .iter()
            .any(|e| Some(e.data["id"].as_u64().unwrap()) == asked.borrow().dialog)
    );
    let r = asked
        .borrow()
        .choice
        .as_ref()
        .unwrap()
        .take()
        .expect("the choice ended in the close's call");
    assert_eq!(files::chosen(&r).unwrap(), files::Chosen::Cancelled);
    assert!(app.service(&FILES).unwrap().open_ids().is_empty());
    assert!(
        app.broken_invariants().is_empty(),
        "{:?}",
        app.broken_invariants()
    );
}

#[test]
fn a_dialog_and_a_file_choice_are_popups_of_their_window() {
    let mut m = multi();
    let (_, asked) = import_with(&mut m, true, true);
    let dialog = dialog_popup(asked.borrow().dialog.unwrap());
    let choice = choice_popup(m.app());
    let list = listed(m.app());
    for popup in [&dialog, &choice] {
        assert!(
            list.contains(&(popup.clone(), WindowKind::Popup, Some(IMPORT.into()))),
            "{popup} in {list:?}"
        );
        let opened = events(m.app(), "popup.opened");
        assert!(
            opened.iter().any(|e| e.data["window"] == **popup),
            "{opened:?}"
        );
    }
    let mut owners: Vec<String> = m
        .app()
        .windows_list()
        .into_iter()
        .filter(|w| w.kind == WindowKind::Popup)
        .map(|w| w.owner)
        .collect();
    owners.sort();
    assert_eq!(owners, ["egui_pop.dialogs", "egui_pop.files"]);
    assert!(m.app().broken_invariants().is_empty());
}

/// The ADR's acceptance: closed by request, with a dialog and a file
/// choice open.
#[test]
fn a_window_closed_by_request_closes_its_dialog_and_file_choice_first() {
    let mut m = multi();
    let (_, asked) = import_with(&mut m, true, true);
    let dialog = dialog_popup(asked.borrow().dialog.unwrap());
    let choice = choice_popup(m.app());
    m.app().close_window(&WindowId::new(IMPORT));
    // In the close's call: no pass in between.
    assert_children_closed_first(m.app(), &dialog, &choice, &asked);
}

/// The same, closed by the system (its title bar's close).
#[test]
fn a_window_closed_by_the_system_closes_its_dialog_and_file_choice_first() {
    let mut m = multi();
    let (v, asked) = import_with(&mut m, true, true);
    let dialog = dialog_popup(asked.borrow().dialog.unwrap());
    let choice = choice_popup(m.app());
    m.request_close(v);
    m.window_pass(v);
    assert_children_closed_first(m.app(), &dialog, &choice, &asked);
    m.root_pass();
    m.root_pass();
    assert!(m.app().broken_invariants().is_empty());
}

/// The same, asked as `pop.window.close` by anyone.
#[test]
fn a_window_close_asked_on_the_bus_closes_its_dialog_and_file_choice_first() {
    let mut m = multi();
    let (_, asked) = import_with(&mut m, true, true);
    let dialog = dialog_popup(asked.borrow().dialog.unwrap());
    let choice = choice_popup(m.app());
    assert!(m.app().request_window_close(WindowRequest::new(
        IMPORT,
        WindowKind::App,
        "app",
        Reason::Command
    )));
    assert_children_closed_first(m.app(), &dialog, &choice, &asked);
}

#[test]
fn a_dialog_answered_closes_its_popup() {
    let mut m = multi();
    let (v, asked) = import_with(&mut m, true, false);
    let dialog = dialog_popup(asked.borrow().dialog.unwrap());
    let d = m.app().service(&DIALOGS).unwrap();
    let f = m.app().debug_call_in(
        Some(IMPORT),
        "dialogs",
        "answer",
        &serde_json::json!({ "answer": "cancel" }),
        Origin::User,
    );
    assert!(f.ok, "{}", f.message);
    m.window_pass(v);
    assert!(d.is_empty());
    let closed = events(m.app(), "popup.closed");
    let e = closed
        .iter()
        .find(|e| e.data["window"] == dialog)
        .unwrap_or_else(|| panic!("{closed:?}"));
    assert_eq!(e.data["reason"], "user");
    assert_eq!(listed(m.app()).len(), 2, "the main and import windows");
    assert!(m.app().broken_invariants().is_empty());
}

#[test]
fn a_file_choice_answered_closes_its_popup() {
    let mut m = multi();
    let (v, asked) = import_with(&mut m, false, true);
    let choice = choice_popup(m.app());
    let f = m.app().service(&FILES).unwrap();
    let id = f.open_ids()[0];
    f.deliver(id, Some(vec!["/tmp/photos".into()])).unwrap();
    m.window_pass(v);
    m.window_pass(v);
    let r = asked
        .borrow()
        .choice
        .as_ref()
        .unwrap()
        .take()
        .expect("the choice ended");
    assert_eq!(
        files::chosen(&r).unwrap(),
        files::Chosen::Paths(vec!["/tmp/photos".into()])
    );
    let closed = events(m.app(), "popup.closed");
    let e = closed
        .iter()
        .find(|e| e.data["window"] == choice)
        .unwrap_or_else(|| panic!("{closed:?}"));
    assert_eq!(e.data["reason"], "user");
    assert!(
        !listed(m.app()).iter().any(|(id, ..)| *id == choice),
        "{:?}",
        listed(m.app())
    );
    assert!(m.app().broken_invariants().is_empty());
}

/// A dialog waiting behind another in the closing window never came up:
/// it has no popup, and `pop.dialogs.cancel-on-close` cancels it in the close.
#[test]
fn a_waiting_dialog_is_cancelled_with_its_window() {
    let mut m = multi();
    let (_, asked) = import_with_dialogs(&mut m, 2, false);
    let d = m.app().service(&DIALOGS).unwrap();
    // The second, opened from the import window too, waits.
    let second = asked.borrow().waiting[0];
    assert_eq!(d.len(), 2);
    assert!(
        !listed(m.app())
            .iter()
            .any(|(id, ..)| *id == dialog_popup(second)),
        "a waiting dialog is not on screen"
    );
    m.app().close_window(&WindowId::new(IMPORT));
    assert!(d.is_empty());
    let cancelled = events(m.app(), "dialog.cancelled");
    for id in [asked.borrow().dialog.unwrap(), second] {
        assert!(
            cancelled.iter().any(|e| e.data["id"] == id),
            "{id}: {cancelled:?}"
        );
    }
    // What `pop.window.close`'s listeners did, as its step's event says.
    let step = events(m.app(), "window.close");
    let did = step.last().unwrap().data["result"]["did"].to_string();
    assert!(did.contains(dialogs::CANCEL_ON_CLOSE), "{did}");
    assert_eq!(listed(m.app()).len(), 1);
}

/// A native window (the system's chooser, a web view's own window) is a
/// child of its window: closed with it by its own `pop.window.close`.
#[test]
fn a_native_child_closes_with_its_window() {
    let mut m = multi();
    let (_, _) = import_with(&mut m, false, false);
    let native = || WindowRequest::new("chooser", WindowKind::Native, "test", Reason::Command);
    // Refused without an open parent, or without an owner.
    assert!(!m.app().request_native_open(native()));
    assert!(!m.app().request_native_open(native().parent("nowhere")));
    assert!(!m.app().request_native_open(
        WindowRequest::new("chooser", WindowKind::Native, "", Reason::Command).parent(IMPORT)
    ));
    assert_eq!(events(m.app(), "window.refused").len(), 3);
    assert!(m.app().request_native_open(native().parent(IMPORT)));
    assert!(listed(m.app()).contains(&(
        "chooser".to_string(),
        WindowKind::Native,
        Some(IMPORT.into())
    )));
    m.app().close_window(&WindowId::new(IMPORT));
    let closed = events(m.app(), "window.closed");
    let e = closed
        .iter()
        .find(|e| e.data["window"] == "chooser")
        .unwrap_or_else(|| panic!("{closed:?}"));
    assert_eq!(e.data["reason"], "parent-closed");
    assert!(
        said_at(m.app(), "window.closed", "chooser") < said_at(m.app(), "window.closed", IMPORT)
    );
    assert_eq!(listed(m.app()).len(), 1);
    assert!(m.app().broken_invariants().is_empty());
}

/// A popup asked without its parent open is refused.
#[test]
fn a_popup_without_its_parent_is_refused() {
    let m = multi();
    let popup = WindowRequest::new("menu", WindowKind::Popup, "test", Reason::User);
    assert!(!m.app().request_popup_open(popup.clone()));
    assert!(!m.app().request_popup_open(popup.clone().parent("nowhere")));
    assert!(!m.app().request_popup_close(popup.clone()));
    assert_eq!(events(m.app(), "popup.refused").len(), 3);
    assert!(m.app().request_popup_open(popup.clone().parent("main")));
    assert!(m.app().request_popup_close(popup));
    assert_eq!(listed(m.app()).len(), 1);
}

/// The main window closed while the import window holds a dialog: the
/// import window moves into it (`pop.window.reparent`), its dialog with it,
/// now a popup of the main window, its parent never gone.
#[test]
fn a_window_moved_into_the_main_window_takes_its_popups() {
    let mut m = multi();
    let (_, asked) = import_with(&mut m, true, false);
    let dialog = dialog_popup(asked.borrow().dialog.unwrap());
    m.request_close(ViewportId::ROOT);
    m.root_pass();
    m.root_pass();
    assert_eq!(
        m.app().window_viewport(&IMPORT.into()),
        Some(ViewportId::ROOT)
    );
    assert!(
        listed(m.app()).contains(&(dialog, WindowKind::Popup, Some("main".into()))),
        "{:?}",
        listed(m.app())
    );
    assert!(!m.app().service(&DIALOGS).unwrap().is_empty());
    assert!(
        m.app().broken_invariants().is_empty(),
        "{:?}",
        m.app().broken_invariants()
    );
}

/// No dialog or file choice ends but on the bus: the old ways a closed
/// window's dialogs and choices were cancelled at a later pass are gone.
#[test]
fn dialogs_and_choices_have_no_private_close_path() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let read = |f: &str| std::fs::read_to_string(src.join(f)).unwrap();
    let dialogs = read("dialogs.rs");
    for old in ["fn window_closed", "orphaned"] {
        assert!(
            !dialogs.contains(old),
            "dialogs.rs has `{old}`: a closing window closes its dialogs' popups"
        );
    }
    let files = read("files.rs");
    assert!(
        !files.contains("still_open"),
        "files.rs follows its window's close from a pass: listen to `pop.popup.close`"
    );
    let bus = read("bus.rs");
    assert!(
        !bus.contains("dialogs::window_closed"),
        "`pop.window.children` names a neighbour: it asks each child's close"
    );
}
