//! A folded side area's strip: an icon per panel, a click opening the
//! panel alone as a flyout over the centre, a double click or the pin
//! unfolding the area, the flyout's width kept per panel, and a panel's
//! indicator on its icon while the panel is not shown.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::steps::Outcome;
use egui_pop_api::strips::{Indicator, StripsPlugin, Tone};
use egui_pop_api::workbench::{self, STEP_FLYOUT};
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, AppBuilder, Ctx, Plugin, PluginError};
use egui_pop_model::ids::{BlockId, CommandId, PanelId, PluginId};
use egui_pop_model::keymap::Layer;
use egui_pop_model::panels::{Area, PanelSpec};
use serde_json::json;

const FILES: PanelId = PanelId::from_static("demo.files");
const SEARCH: PanelId = PanelId::from_static("demo.search");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(
            PanelSpec::new(FILES, "Files", Area::StartSide).icon("folder"),
            |ui, _| {
                ui.label("files body");
            },
        )?
        // No icon: its initial stands for it.
        .panel(
            PanelSpec::new(SEARCH, "Search", Area::StartSide).min_size(160.0),
            |ui, _| {
                ui.label("search body");
            },
        )?;
        ctx.keybinding(
            "Ctrl+Shift+E",
            CommandId::new("demo.files.focus").unwrap(),
            "",
            Layer::Default,
        )?;
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("demo.main"), 0, |ui, _| {
                ui.label("main body");
            }),
        );
        // The search panel's hits, on its icon while it is not shown.
        ctx.indicator(
            Indicator::new("demo.search.hits", "No hits", "search.")
                .panel(&SEARCH)
                .reduce(|state, e| {
                    let n = e.data["hits"].as_u64().unwrap_or(0);
                    state.count = Some(n);
                    state.text = format!("{n} hits");
                    state.detail = "Search results waiting".into();
                    state.tone = Tone::Accent;
                }),
        );
        Ok(())
    }
}

fn builder() -> AppBuilder {
    App::builder()
        .plugin(StripsPlugin::status_only())
        .plugin(Demo)
        .folded(&[Area::StartSide])
}

fn harness(builder: AppBuilder) -> Harness<'static, App> {
    let app = builder.build(&egui::Context::default()).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h.run();
    h
}

/// A strip icon (a button named by its panel's title; the flyout's
/// band shows the title as a label).
fn icon_button<'h>(h: &'h Harness<'static, App>, title: &'h str) -> egui_kittest::Node<'h> {
    h.get_by_role_and_label(egui::accesskit::Role::Button, title)
}

/// A press and release at a point.
fn click_at(h: &mut Harness<'static, App>, at: egui::Pos2) {
    h.drag_at(at);
    h.run();
    h.drop_at(at);
    h.run();
    h.run();
}

fn icon<'a>(icons: &'a [workbench::StripIcon], panel: &PanelId) -> &'a workbench::StripIcon {
    icons
        .iter()
        .find(|i| i.panel == *panel)
        .expect("the panel has an icon in the strip")
}

#[test]
fn a_folded_strip_shows_an_icon_per_panel_in_tab_order() {
    let h = harness(builder());
    let app = h.state().handle().clone();
    let icons = workbench::strip(&app);
    let panels: Vec<&PanelId> = icons.iter().map(|i| &i.panel).collect();
    assert_eq!(panels, [&FILES, &SEARCH]);
    let files = icon(&icons, &FILES);
    assert_eq!(files.area, "start_side");
    assert_eq!(files.icon.as_deref(), Some("folder"));
    assert!(
        files.key.as_deref().is_some_and(|k| k.contains('E')),
        "{files:?}"
    );
    assert!(!files.shown);
    // Without an icon: its initial is drawn, no key.
    let search = icon(&icons, &SEARCH);
    assert_eq!(search.icon, None);
    assert_eq!(search.key, None);
    // Named for assistive technology by the panel's title.
    assert!(h.query_by_label("Files").is_some());
    assert!(h.query_by_label("files body").is_none());
    // The strip is wide enough for its icons.
    let strip = h.get_by_label("Unfold the start side bar").rect();
    assert!(strip.width() >= 16.0, "{strip:?}");
}

#[test]
fn a_click_opens_the_panel_as_a_flyout_over_the_centre_and_escape_closes_it() {
    let mut h = harness(builder());
    let app = h.state().handle().clone();
    let centre = app.area_rect(Area::Center).unwrap();
    icon_button(&h, "Files").click();
    h.run();
    h.run();
    let (panel, rect) = app.flyout().expect("the flyout is open");
    assert_eq!(panel, FILES);
    let rect = rect.expect("drawn");
    assert!(h.query_by_label("files body").is_some());
    assert!(h.query_by_label("Pin Files").is_some());
    // Over the centre, which does not move; the area stays folded.
    assert!(rect.intersects(centre), "{rect:?} {centre:?}");
    assert_eq!(app.area_rect(Area::Center).unwrap(), centre);
    assert_eq!(app.layout().folded, [Area::StartSide]);
    // An overlay of the stack.
    assert!(
        egui_pop_core::overlay::stack(&h.ctx)
            .iter()
            .any(|o| o.label == "Files")
    );
    let icons = workbench::strip(&app);
    assert!(icon(&icons, &FILES).shown && icon(&icons, &FILES).flyout);
    // Escape closes it.
    h.key_press(egui::Key::Escape);
    h.run();
    h.run();
    assert!(app.flyout().is_none());
    assert!(h.query_by_label("files body").is_none());
    // A click on another icon switches; one on the centre closes it.
    icon_button(&h, "Files").click();
    h.run();
    icon_button(&h, "Search").click();
    h.run();
    h.run();
    assert_eq!(app.flyout().map(|f| f.0), Some(SEARCH));
    assert!(h.query_by_label("search body").is_some());
    assert!(h.query_by_label("files body").is_none());
    // Away from the flyout, over the centre.
    click_at(&mut h, egui::pos2(850.0, 300.0));
    assert!(app.flyout().is_none());
    assert_eq!(app.layout().folded, [Area::StartSide]);
}

#[test]
fn a_double_click_or_the_pin_unfolds_the_area_with_the_panel_shown() {
    let mut h = harness(builder());
    let app = h.state().handle().clone();
    // The pin.
    icon_button(&h, "Search").click();
    h.run();
    h.run();
    h.get_by_label("Pin Search").click();
    h.run();
    h.run();
    assert!(app.layout().folded.is_empty());
    assert!(app.flyout().is_none());
    assert!(h.query_by_label("search body").is_some());
    // Folded again, a double click on an icon. kittest steps one frame
    // per queued event (a quarter of a second each), so two `click()`s
    // are two single clicks; both clicks go into one frame's input.
    app.fold_area(Area::StartSide, true);
    h.run();
    h.run();
    let at = icon_button(&h, "Files").rect().center();
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    };
    h.input_mut().events.extend([
        egui::Event::PointerMoved(at),
        button(true),
        button(false),
        button(true),
        button(false),
    ]);
    h.run();
    h.run();
    assert!(app.layout().folded.is_empty());
    assert!(app.flyout().is_none());
    assert!(h.query_by_label("files body").is_some());
}

#[test]
fn the_flyout_is_resized_by_its_free_edge_and_its_width_is_saved() {
    let dir = std::env::temp_dir().join(format!("egui-pop-flyout-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    {
        let mut h = harness(builder().persist(&dir));
        let app = h.state().handle().clone();
        icon_button(&h, "Search").click();
        h.run();
        h.run();
        let before = app.flyout().and_then(|f| f.1).expect("drawn");
        let from = egui::pos2(before.right(), before.center().y);
        h.drag_at(from);
        h.run();
        for i in 1..=4 {
            h.hover_at(from + egui::vec2(10.0 * i as f32, 0.0));
            h.run();
        }
        h.drop_at(from + egui::vec2(40.0, 0.0));
        h.run();
        h.run();
        let width = *app
            .layout()
            .flyouts
            .get(&SEARCH)
            .expect("the width is kept");
        assert!((width - (before.width() + 40.0)).abs() < 2.0, "{width}");
        // Still open: the drag was on its edge, not outside.
        let after = app.flyout().and_then(|f| f.1).expect("still open");
        assert!((after.width() - width).abs() < 1.0, "{after:?}");
        h.state().save().unwrap();
    }
    let mut h = harness(builder().persist(&dir));
    let app = h.state().handle().clone();
    let width = *app.layout().flyouts.get(&SEARCH).expect("saved");
    assert!(app.open_flyout(&SEARCH, true).unwrap().ok());
    h.run();
    h.run();
    let rect = app.flyout().and_then(|f| f.1).expect("drawn");
    assert!((rect.width() - width).abs() < 1.0, "{rect:?} {width}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn only_a_folded_side_areas_panel_flies_out_and_the_centre_never_folds() {
    let mut h = harness(App::builder().plugin(Demo));
    let app = h.state().handle().clone();
    // Not folded: refused.
    let out = app.open_flyout(&FILES, true).unwrap();
    assert!(
        matches!(out, Outcome::Done(Err(ref e)) if e.contains("folded side area")),
        "{out:?}"
    );
    // Once folded, through the step.
    app.fold_area(Area::StartSide, true);
    h.run();
    assert!(app.open_flyout(&FILES, true).unwrap().ok());
    h.run();
    let snap = app.debug_snapshot();
    let runs = snap["steps"]["runs"].to_string();
    assert!(runs.contains(STEP_FLYOUT.as_str()), "{runs}");
    assert_eq!(snap["workbench"]["flyout"]["panel"], "demo.files");
    assert_eq!(app.flyout().map(|f| f.0), Some(FILES));
    // Unfolding the area closes the flyout.
    app.fold_area(Area::StartSide, false);
    h.run();
    assert!(app.flyout().is_none());
    // The centre.
    let out = app.fold_area(Area::Center, true).unwrap();
    assert!(
        matches!(out, Outcome::Done(Err(ref e)) if e.contains("never folds")),
        "{out:?}"
    );
    assert!(!app.layout().folded.contains(&Area::Center));
}

#[test]
fn a_panels_indicator_shows_on_its_icon_while_the_panel_is_not_shown() {
    let mut h = harness(builder());
    let app = h.state().handle().clone();
    app.publish("host", None, "search.done", json!({ "hits": 3 }));
    h.run();
    h.run();
    let icons = workbench::strip(&app);
    let search = icon(&icons, &SEARCH);
    assert!(search.marked, "{search:?}");
    let state = search.indicator.as_ref().expect("its indicator");
    assert_eq!(state.count, Some(3));
    assert_eq!(state.text, "3 hits");
    assert!(!icon(&icons, &FILES).marked);
    // Drawn on the icon, read by agents as a mark.
    let mark = egui_pop_core::indicator::find(&h.ctx, "strip:demo.search.hits")
        .expect("the mark is drawn");
    assert_eq!(mark.text.as_deref(), Some("3"));
    assert!(mark.shown);
    // The workbench collector lists it.
    let snap = app.debug_snapshot();
    let strips = snap["workbench"]["strips"].as_array().expect("strips");
    let listed = strips
        .iter()
        .find(|i| i["panel"] == "demo.search")
        .expect("listed");
    assert_eq!(listed["marked"], true);
    assert_eq!(listed["indicator"]["count"], 3);
    // Not a status bar item.
    assert!(
        egui_pop_api::strips::status(&app)
            .iter()
            .all(|i| i.id != "demo.search.hits")
    );
    // Shown as the flyout: no mark.
    assert!(app.open_flyout(&SEARCH, true).unwrap().ok());
    h.run();
    h.run();
    assert!(!icon(&workbench::strip(&app), &SEARCH).marked);
    assert!(egui_pop_core::indicator::find(&h.ctx, "strip:demo.search.hits").is_none());
}

const FORM: PanelId = PanelId::from_static("demo.form");

/// Entries long enough to reach past a narrow flyout's edge: a click on
/// their end lands outside the flyout's rect.
const FINE: &str = "Fine, an entry long enough to reach well past the flyout's edge";
const MARK: &str = "Mark it, from a menu entry long enough to pass the flyout's edge";

/// A narrow panel with a combo box and a button with a context menu.
struct Form {
    mode: std::sync::Arc<std::sync::Mutex<String>>,
    marked: std::sync::Arc<std::sync::Mutex<bool>>,
}

impl Plugin for Form {
    fn id(&self) -> PluginId {
        PluginId::from_static("form")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let mode = self.mode.clone();
        let marked = self.marked.clone();
        ctx.panel(
            PanelSpec::new(FORM, "Form", Area::StartSide)
                .min_size(120.0)
                .default_size(120.0),
            move |ui, _| {
                let mut m = mode.lock().unwrap();
                egui::ComboBox::from_id_salt("demo.form.mode")
                    .selected_text(m.clone())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut *m, "Fast".to_owned(), "Fast");
                        ui.selectable_value(&mut *m, FINE.to_owned(), FINE);
                    });
                ui.button("Options").context_menu(|ui| {
                    if ui.button(MARK).clicked() {
                        *marked.lock().unwrap() = true;
                    }
                });
            },
        )?;
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("form.main"), 0, |ui, _| {
                ui.label("main body");
            }),
        );
        Ok(())
    }
}

struct FormHarness {
    h: Harness<'static, App>,
    app: egui_pop_api::AppHandle,
    mode: std::sync::Arc<std::sync::Mutex<String>>,
    marked: std::sync::Arc<std::sync::Mutex<bool>>,
}

/// The form panel open as a flyout.
fn form_flyout() -> FormHarness {
    let mode = std::sync::Arc::new(std::sync::Mutex::new("Fast".to_owned()));
    let marked = std::sync::Arc::new(std::sync::Mutex::new(false));
    let builder = App::builder()
        .plugin(StripsPlugin::status_only())
        .plugin(Form {
            mode: mode.clone(),
            marked: marked.clone(),
        })
        .folded(&[Area::StartSide]);
    let mut h = harness(builder);
    let app = h.state().handle().clone();
    icon_button(&h, "Form").click();
    h.run();
    h.run();
    assert_eq!(app.flyout().map(|f| f.0), Some(FORM));
    FormHarness {
        h,
        app,
        mode,
        marked,
    }
}

impl FormHarness {
    fn flyout_rect(&self) -> egui::Rect {
        self.app
            .flyout()
            .and_then(|f| f.1)
            .expect("the flyout is open and drawn")
    }

    /// Opens the combo box with a click on it.
    fn open_combo(&mut self) {
        self.h.get_by_role(egui::accesskit::Role::ComboBox).click();
        self.h.run();
        self.h.run();
        assert!(
            self.h.query_by_label(FINE).is_some(),
            "the combo box's list is open"
        );
    }

    /// A click near the end of an entry, outside the flyout's rect.
    fn click_entry_end(&mut self, label: &str) {
        let entry = self.h.get_by_label(label).rect();
        let at = egui::pos2(entry.right() - 4.0, entry.center().y);
        let flyout = self.flyout_rect();
        assert!(
            !flyout.expand(8.0).contains(at),
            "the entry passes the flyout's edge: {at:?} {flyout:?}"
        );
        click_at(&mut self.h, at);
    }
}

#[test]
fn a_pick_in_a_combo_box_opened_in_a_flyout_keeps_the_flyout_open() {
    let mut f = form_flyout();
    f.open_combo();
    f.click_entry_end(FINE);
    assert_eq!(*f.mode.lock().unwrap(), FINE, "the value changed");
    assert_eq!(
        f.app.flyout().map(|o| o.0),
        Some(FORM),
        "the flyout stays open"
    );
    assert!(!egui::Popup::is_any_open(&f.h.ctx), "the list closed");
}

#[test]
fn a_pick_in_a_context_menu_opened_in_a_flyout_keeps_the_flyout_open() {
    let mut f = form_flyout();
    f.h.get_by_label("Options").click_secondary();
    f.h.run();
    f.h.run();
    assert!(
        f.h.query_by_label(MARK).is_some(),
        "the context menu is open"
    );
    f.click_entry_end(MARK);
    assert!(*f.marked.lock().unwrap(), "the entry ran");
    assert_eq!(
        f.app.flyout().map(|o| o.0),
        Some(FORM),
        "the flyout stays open"
    );
    assert!(f.h.query_by_label(MARK).is_none(), "the menu closed");
}

#[test]
fn a_click_truly_outside_closes_the_flyout_and_its_combo_box() {
    let mut f = form_flyout();
    f.open_combo();
    click_at(&mut f.h, egui::pos2(850.0, 300.0));
    assert!(f.app.flyout().is_none(), "the flyout closed");
    assert!(f.h.query_by_label(FINE).is_none(), "the list closed");
    assert_eq!(*f.mode.lock().unwrap(), "Fast", "nothing was picked");
}

#[test]
fn escape_closes_the_combo_box_first_then_the_flyout() {
    let mut f = form_flyout();
    f.open_combo();
    f.h.key_press(egui::Key::Escape);
    f.h.run();
    f.h.run();
    assert!(f.h.query_by_label(FINE).is_none(), "the list closed first");
    assert_eq!(
        f.app.flyout().map(|o| o.0),
        Some(FORM),
        "the flyout is still open"
    );
    f.h.key_press(egui::Key::Escape);
    f.h.run();
    f.h.run();
    assert!(f.app.flyout().is_none(), "then the flyout closed");
}
