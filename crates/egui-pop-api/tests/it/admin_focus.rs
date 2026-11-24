//! The focus the Settings edit (`admin.focus`): opened from a window, the
//! Settings edit its focus, a header names it once there are several, the
//! switch picks another and the fields show its values; a setting with no
//! host level is the same in every focus.

use std::sync::{Arc, Mutex};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::admin::focus::{CMD_FOCUS, SettingsFoci, SettingsFocus};
use egui_pop_api::admin::model::{Field, Manifest};
use egui_pop_api::admin::store::MemoryStore;
use egui_pop_api::admin::{ADMIN, AdminPlugin, CMD_OPEN, ExtraScope};
use egui_pop_api::app::AppHandle;
use egui_pop_api::windows::{WindowId, WindowSpec, WindowsPlugin};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_core::strings::StrKey;
use egui_pop_model::admin::ValueScope;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{PluginId, SettingId};
use egui_pop_model::values::Descriptor;
use egui_pop_test::windows::MultiWindow;
use serde_json::{Value as Json, json};

const DARK: SettingId = SettingId::from_static("look.dark");
const SIZE: SettingId = SettingId::from_static("look.size");

const WORKSPACE: ExtraScope = ExtraScope::new(
    "workspace",
    StrKey::new("test-workspace", "This workspace"),
    StrKey::new("test-workspace-about", "while this workspace is open"),
);

struct Look;

impl Plugin for Look {
    fn id(&self) -> PluginId {
        PluginId::from_static("look")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.setting(DARK, Descriptor::bool("Dark look", false))?;
        ctx.setting(SIZE, Descriptor::int("Reading size", 8, 32, 12))?;
        let admin = ctx.app().service(&ADMIN).unwrap();
        admin.declare_scope(WORKSPACE);
        admin.manifest(Manifest::new(1).scope("look", "Look", "settings").section(
            "look/main",
            "Main",
            |s| {
                s.field(Field::setting("look.dark"));
                s.field(Field::setting("look.size").scopes(&[WORKSPACE.scope]));
            },
        ));
        Ok(())
    }
}

/// Two foci: the main window's, its workspace's size 20; the import
/// window's (while open), its size 24. Each keeps its values in a store
/// of its own.
#[derive(Clone)]
struct Foci {
    main: MemoryStore,
    import: MemoryStore,
    edits: Arc<Mutex<Vec<String>>>,
}

impl Foci {
    fn new() -> Foci {
        let store = |size| {
            MemoryStore::new(&[ValueScope::App, WORKSPACE.scope])
                .with(WORKSPACE.scope, [("look.size".into(), json!(size))].into())
        };
        Foci {
            main: store(20),
            import: store(24),
            edits: Arc::default(),
        }
    }
}

impl SettingsFoci for Foci {
    fn foci(&self, app: &AppHandle) -> Vec<SettingsFocus> {
        let mut out = vec![SettingsFocus::new(
            "main",
            "Main window",
            &[WindowId::main()],
        )];
        if app.window_viewport(&WindowId::new("import")).is_some() {
            out.push(SettingsFocus::new(
                "import",
                "Import window",
                &[WindowId::new("import")],
            ));
        }
        out
    }

    fn edit(&self, app: &AppHandle, focus: &str) -> Result<(), String> {
        let admin = app.service(&ADMIN).ok_or("no admin")?;
        let store = if focus == "import" {
            &self.import
        } else {
            &self.main
        };
        admin.offer(WORKSPACE.scope, true);
        admin.attach_store(store.clone());
        self.edits.lock().unwrap().push(focus.into());
        Ok(())
    }
}

fn build(ctx: &egui::Context, foci: &Foci) -> App {
    let app = App::builder()
        .name("Demo")
        .plugins(DefaultPlugins)
        .plugin(WindowsPlugin)
        .plugin(AdminPlugin)
        .plugin(Look)
        .build(ctx)
        .unwrap();
    let admin = app.handle().service(&ADMIN).unwrap();
    admin.set_foci(foci.clone());
    foci.edit(app.handle(), "main").unwrap();
    app
}

fn harness(foci: &Foci) -> Harness<'static, App> {
    let foci = foci.clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), {
            let ctx = egui::Context::default();
            build(&ctx, &foci)
        });
    h.run();
    h
}

fn run(h: &mut Harness<'_, App>, cmd: &egui_pop_model::ids::CommandId, args: Json) -> Json {
    let out = h.state_mut().run(cmd, args, Origin::User).now().unwrap();
    h.run();
    h.run();
    out.unwrap()
}

fn until<R>(what: &str, mut step: impl FnMut() -> R, done: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !done() {
        assert!(std::time::Instant::now() < deadline, "timed out: {what}");
        std::thread::sleep(std::time::Duration::from_millis(10));
        let _ = step();
    }
}

fn size(app: &AppHandle) -> i64 {
    app.setting::<i64>(&SIZE).unwrap()
}

#[test]
fn opened_from_the_main_window_it_edits_the_main_focus_without_a_header_alone() {
    let foci = Foci::new();
    let mut h = harness(&foci);
    run(&mut h, &CMD_OPEN, json!({}));
    let app = h.state().handle().clone();
    let a = app.service(&ADMIN).unwrap();
    assert_eq!(a.edited_focus().as_deref(), Some("main"));
    until("the main focus's values", || h.run(), || size(&app) == 20);
    // One focus: no header.
    assert!(h.query_by_label("Settings for").is_none());
}

#[test]
fn opened_from_another_window_it_edits_that_focus_and_the_switch_changes_it() {
    let foci = Foci::new();
    let mut m = MultiWindow::new(|ctx| build(ctx, &foci));
    m.app()
        .open_window(WindowSpec::new("import", "Import"), |ui, _| {
            ui.label("IMPORT");
        });
    m.root_pass();
    let vi = m.app().window_viewport(&"import".into()).unwrap();
    m.window_pass(vi);
    m.focus(vi);
    // Settings asked in the import window: its focus.
    m.press(vi, Key::Comma, Modifiers::COMMAND);
    m.window_pass(vi);
    m.root_pass();
    let app = m.app().clone();
    let a = app.service(&ADMIN).unwrap();
    assert!(a.is_open());
    assert_eq!(a.edited_focus().as_deref(), Some("import"));
    let mut pass = || {
        m.root_pass();
    };
    until("the import focus's values", &mut pass, || size(&app) == 24);
    // The app's setting, with no workspace level, is the same whatever
    // the focus.
    let dark = app.setting::<bool>(&DARK).unwrap();
    // The switch: back to the main focus, its values follow.
    a.edit_focus(&app, "main").unwrap();
    until("the main focus's values", &mut pass, || size(&app) == 20);
    assert_eq!(app.setting::<bool>(&DARK).unwrap(), dark);
    assert_eq!(*foci.edits.lock().unwrap(), ["main", "import", "main"]);
    // An unknown focus is refused.
    assert!(a.edit_focus(&app, "elsewhere").is_err());
}

#[test]
fn the_header_names_the_focus_and_its_switch_picks_another() {
    let foci = Foci::new();
    let mut h = harness(&foci);
    let app = h.state().handle().clone();
    // Embedded (kittest), the import window is drawn in the main one.
    app.open_window(WindowSpec::new("import", "Import"), |ui, _| {
        ui.label("IMPORT");
    });
    h.run();
    run(&mut h, &CMD_OPEN, json!({}));
    h.run();
    let out = run(&mut h, &CMD_FOCUS, json!({}));
    assert_eq!(out["edited"], json!("main"));
    assert_eq!(out["foci"].as_array().unwrap().len(), 2);
    let combo = h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Settings for");
    assert_eq!(combo.value().as_deref(), Some("Main window"));
    combo.click();
    h.run();
    h.get_by_label("Import window").click();
    h.run();
    h.run();
    let a = app.service(&ADMIN).unwrap();
    assert_eq!(a.edited_focus().as_deref(), Some("import"));
    until("the import focus's values", || h.run(), || size(&app) == 24);
    egui_pop_test::assert_all_named(&h);
}

#[test]
fn a_focus_closed_gives_way_to_the_first() {
    let foci = Foci::new();
    let mut h = harness(&foci);
    let app = h.state().handle().clone();
    app.open_window(WindowSpec::new("import", "Import"), |ui, _| {
        ui.label("IMPORT");
    });
    h.run();
    let a = app.service(&ADMIN).unwrap();
    a.edit_focus(&app, "import").unwrap();
    until("the import focus's values", || h.run(), || size(&app) == 24);
    app.close_window(&WindowId::new("import"));
    until(
        "the main focus again",
        || h.run(),
        || a.edited_focus().as_deref() == Some("main"),
    );
    until("the main focus's values", || h.run(), || size(&app) == 20);
}
