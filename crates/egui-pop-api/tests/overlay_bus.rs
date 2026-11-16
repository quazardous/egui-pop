//! Popups, menus and the palette on the bus (ADR 0029, slice 6): the
//! overlay stack's opens and closes are `popup.open` / `popup.close`, with
//! their owner, their window and how they closed; a window closing under
//! an open menu closes the menu first (`parent-closed`), no invariant
//! broken.

use std::path::Path;

use egui::{Key, Modifiers, ViewportId};
use egui_pop_api::app::MENU_VIEW;
use egui_pop_api::bus::{WindowKind, WindowRecord};
use egui_pop_api::host::Event;
use egui_pop_api::windows::WindowSpec;
use egui_pop_api::{App, AppHandle, DefaultPlugins, menus, palette};
use egui_pop_core::overlay::{self, Kind, OverlaySpec};
use egui_pop_test::windows::MultiWindow;

const IMPORT: &str = "import";

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

/// Where the last event named `name` about `window` is in the journal.
fn said_at(app: &AppHandle, name: &str, window: &str) -> Option<usize> {
    app.events_since(Default::default())
        .iter()
        .rposition(|e| e.name == name && e.data["window"] == window)
}

fn popups(app: &AppHandle) -> Vec<WindowRecord> {
    app.windows_list()
        .into_iter()
        .filter(|w| w.kind == WindowKind::Popup)
        .collect()
}

fn no_invariant_broken(app: &AppHandle) {
    assert!(
        app.broken_invariants().is_empty(),
        "{:?}",
        app.broken_invariants()
    );
}

/// Opens the import window, its content showing a popover while `shown`
/// says so; the window's viewport.
fn import(m: &mut MultiWindow, shown: std::rc::Rc<std::cell::Cell<bool>>) -> ViewportId {
    m.app()
        .open_window(WindowSpec::new(IMPORT, "Import"), move |ui, _| {
            ui.label("import");
            let mut open = shown.get();
            let spec = OverlaySpec::new("demo.import.popover", "Import options", Kind::Popover);
            overlay::show(ui.ctx(), &spec, &mut open, None, |ui| {
                ui.label("options");
            });
            shown.set(open);
        });
    m.root_pass();
    let v = m.app().window_viewport(&IMPORT.into()).unwrap();
    m.window_pass(v);
    m.window_pass(v);
    v
}

#[test]
fn the_palette_opened_and_closed_by_escape_is_a_popup_of_its_window() {
    let mut m = multi();
    let app = m.app().clone();
    app.service(&palette::PALETTE)
        .unwrap()
        .open(app.when_context());
    m.root_pass();
    m.root_pass();
    let open = popups(&app);
    assert_eq!(open.len(), 1, "{open:?}");
    assert_eq!(open[0].id, "palette@main");
    assert_eq!(open[0].owner, "egui_pop.palette");
    assert_eq!(open[0].parent.as_deref(), Some("main"));
    m.press(ViewportId::ROOT, Key::Escape, Modifiers::NONE);
    m.root_pass();
    m.root_pass();
    assert!(!palette::is_open(&app));
    assert!(popups(&app).is_empty(), "{:?}", popups(&app));
    let closed = events(&app, "popup.closed");
    let e = closed.last().expect("popup.closed");
    assert_eq!(e.data["window"], "palette@main");
    assert_eq!(e.data["reason"], "user");
    assert_eq!(e.data["how"], "escape");
    no_invariant_broken(&app);
}

#[test]
fn a_popup_menu_opened_and_closed_is_a_popup_of_its_window() {
    let mut m = multi();
    let app = m.app().clone();
    assert!(menus::popup_at(&app, &MENU_VIEW, egui::pos2(40.0, 40.0)));
    m.root_pass();
    let open = popups(&app);
    assert_eq!(open.len(), 1, "{open:?}");
    assert_eq!(open[0].id, "menu@main");
    assert_eq!(open[0].owner, "egui_pop.menus");
    m.press(ViewportId::ROOT, Key::Escape, Modifiers::NONE);
    m.root_pass();
    m.root_pass();
    assert_eq!(menus::popup_open(&app), None);
    assert!(popups(&app).is_empty(), "{:?}", popups(&app));
    let closed = events(&app, "popup.closed");
    assert_eq!(closed.last().unwrap().data["window"], "menu@main");
    assert_eq!(closed.last().unwrap().data["reason"], "user");
    // Closed by code: its owner's request.
    assert!(menus::popup_at(&app, &MENU_VIEW, egui::pos2(40.0, 40.0)));
    m.root_pass();
    menus::close_popup(&app);
    assert!(popups(&app).is_empty());
    assert_eq!(
        events(&app, "popup.closed").last().unwrap().data["reason"],
        "command"
    );
    no_invariant_broken(&app);
}

/// A window closed under an open menu and a popover: both closed first,
/// by their `popup.close` (`parent-closed`), before `window.closed`; the
/// menu's owner closed it in that call; no invariant broken.
#[test]
fn a_window_closing_under_an_open_menu_closes_it_first() {
    let mut m = multi();
    let app = m.app().clone();
    let shown = std::rc::Rc::new(std::cell::Cell::new(true));
    let v = import(&mut m, shown.clone());
    let popover = popups(&app);
    assert_eq!(popover.len(), 1, "the popover: {popover:?}");
    assert_eq!(popover[0].parent.as_deref(), Some(IMPORT));
    // The person works in the import window: the menu opens there.
    m.focus(v);
    m.click(v, egui::pos2(300.0, 300.0));
    m.window_pass(v);
    m.root_pass();
    assert_eq!(app.active_window(), v);
    shown.set(true);
    m.window_pass(v);
    assert!(menus::popup_at(&app, &MENU_VIEW, egui::pos2(40.0, 40.0)));
    m.window_pass(v);
    let menu = format!("menu@{IMPORT}");
    let open: Vec<String> = popups(&app).into_iter().map(|w| w.id).collect();
    assert!(open.contains(&menu), "{open:?}");
    assert_eq!(open.len(), 2, "{open:?}");
    app.close_window(&IMPORT.into());
    m.root_pass();
    for popup in &open {
        let closed = events(&app, "popup.closed");
        let e = closed
            .iter()
            .rfind(|e| e.data["window"] == *popup)
            .unwrap_or_else(|| panic!("no popup.closed for {popup}: {closed:?}"));
        assert_eq!(e.data["reason"], "parent-closed");
        assert_eq!(e.data["parent"], IMPORT);
        assert!(
            said_at(&app, "popup.closed", popup) < said_at(&app, "window.closed", IMPORT),
            "{popup} closed before its window"
        );
    }
    assert_eq!(menus::popup_open(&app), None, "closed with its window");
    assert!(popups(&app).is_empty(), "{:?}", popups(&app));
    no_invariant_broken(&app);
}

/// Overlays are pushed and popped by the stack and reported by its watch;
/// egui-pop's own code closes them by their popup's request, never by a
/// direct call, but for the owners that ask their own requests.
#[test]
fn overlays_close_on_the_bus_only() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    // The dialogs' and file choices' popups are their own: their
    // `popup.close` listeners close their overlay.
    let own = ["dialogs.rs", "files.rs", "bus"];
    let mut found = Vec::new();
    for entry in walk(&src) {
        let rel = entry.strip_prefix(&src).unwrap();
        if own.iter().any(|o| rel.starts_with(o)) {
            continue;
        }
        let text = std::fs::read_to_string(&entry).unwrap();
        for (n, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            if code.contains("overlay::close(") || code.contains("overlay::close_anywhere(") {
                found.push(format!("{}:{}: {code}", rel.display(), n + 1));
            }
        }
    }
    assert!(
        found.is_empty(),
        "close an overlay by its popup (`AppHandle::close_overlay`, a `popup.close`): {found:#?}"
    );
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
    out
}
