//! A tray icon (a status icon in the panel, the notification area, the
//! menu bar) with a declared menu.
//!
//! The tray's menu is an egui-pop menu, [`MENU_TRAY`]: plugins fill it as
//! any other (`CommandSpec::menu(MENU_TRAY, ...)`, `Ctx::menu_item`,
//! submenus, lists), its items' titles, `when` clauses, ticks and
//! greying come from the commands, and a pick runs the command through
//! the host. The plugin keeps the system's menu current at each pass.
//!
//! Its step (ADR 0011):
//!
//! - `tray.activate`: the icon was clicked, `{ "button" }` (`left`,
//!   `middle`, `double`; the right button opens the menu). By default the
//!   main window comes to the front ([`crate::window::raise`]). Replace it
//!   to toggle a quick panel, veto it to do nothing.
//!
//! [`System`] is the system's tray through tray-icon (StatusNotifierItem
//! over D-Bus on Linux and BSD, through ksni, with no GTK; the
//! notification area on Windows; the menu bar on macOS). [`Simulated`]
//! records the menu and clicks as a user would, for tests and the
//! gallery.
//!
//! ```
//! use egui_pop_api::{App, Ctx, Plugin, PluginError};
//! use egui_pop_model::commands::CommandSpec;
//! use egui_pop_model::ids::{CommandId, PluginId};
//! use egui_pop_platform::tray::{Entry, MENU_TRAY, Simulated, TrayPlugin};
//!
//! struct Sync;
//!
//! impl Plugin for Sync {
//!     fn id(&self) -> PluginId { PluginId::from_static("sync") }
//!     fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
//!         ctx.command(
//!             CommandSpec::new(CommandId::from_static("sync.now"), "Sync now", "Sync")
//!                 .menu(MENU_TRAY, "1_sync", 0),
//!             |_| Ok(serde_json::Value::Null),
//!         )?;
//!         Ok(())
//!     }
//! }
//!
//! let tray = Simulated::default();
//! let ctx = egui::Context::default();
//! let mut app = App::builder()
//!     .plugin(Sync)
//!     .plugin(TrayPlugin::new("Demo").backend(tray.clone()))
//!     .build(&ctx)
//!     .unwrap();
//! app.logic(&ctx);
//! assert!(matches!(&tray.menu()[0], Entry::Item { label, .. } if label == "Sync now"));
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use egui_pop_api::hooks::On;
use egui_pop_api::services::ServiceToken;
use egui_pop_api::steps::StepSpec;
use egui_pop_api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop_core::lock;
use egui_pop_core::strings;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{CommandId, MenuId, PluginId, StepId};
use egui_pop_model::menus::{Item, Shown, show_items};
use egui_pop_model::when::{When, WhenContext};
use serde::Serialize;
use serde_json::{Value as Json, json};

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.tray");
/// The tray icon's menu.
pub const MENU_TRAY: MenuId = MenuId::from_static("app.tray");
/// The icon was clicked: `{ "button" }`.
pub const STEP_ACTIVATE: StepId = StepId::from_static("tray.activate");
/// The tray, as a service: what it shows, whether it is there.
pub const TRAY: ServiceToken<Tray> = ServiceToken::new("platform.tray");
/// How deep submenus go.
const MAX_DEPTH: u32 = 4;

/// A menu entry as the tray shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Entry {
    /// A command: `id` is the entry's own (a pick names it).
    Item {
        id: String,
        label: String,
        enabled: bool,
        /// Ticked or not when it is a toggle; none otherwise.
        checked: Option<bool>,
    },
    Separator,
    Submenu {
        label: String,
        enabled: bool,
        entries: Vec<Entry>,
    },
}

/// The icon's picture: RGBA pixels, row by row.
#[derive(Clone, Debug, PartialEq)]
pub struct Icon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Icon {
    /// A plain disc of `rgb` on transparency, `size` pixels square:
    /// the icon when the application gives none.
    pub fn disc(size: u32, rgb: [u8; 3]) -> Icon {
        let mut rgba = Vec::with_capacity((size * size * 4) as usize);
        let c = (size as f32 - 1.0) / 2.0;
        let r = size as f32 / 2.0 - 1.0;
        for y in 0..size {
            for x in 0..size {
                let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
                let alpha = (r - d + 0.5).clamp(0.0, 1.0);
                rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], (alpha * 255.0) as u8]);
            }
        }
        Icon {
            rgba,
            width: size,
            height: size,
        }
    }
}

/// What the tray shows besides its menu.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    /// The application's id (`org.example.App`), for the system.
    pub id: String,
    pub title: String,
    /// The text over the icon.
    pub tooltip: String,
    pub icon: Icon,
}

/// What the tray says back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The icon was clicked with this button (`left`, `middle`,
    /// `double`).
    Click(String),
    /// A menu entry was picked, by its id.
    Pick(String),
}

/// Hands events to the plugin from any thread, waking the interface.
#[derive(Clone, Debug)]
pub struct Events {
    tx: Sender<Event>,
    waker: egui::Context,
}

impl Events {
    pub fn send(&self, e: Event) {
        let _ = self.tx.send(e);
        self.waker.request_repaint();
    }
}

/// What shows the tray icon. It lives on the interface thread (the
/// system's tray objects are tied to it on Windows and macOS).
pub trait Backend: 'static {
    /// A short name: `sni`, `windows`, `macos`, `simulated`.
    fn name(&self) -> String;
    /// Shows the icon.
    fn start(&mut self, spec: &Spec, events: Events) -> Result<(), String>;
    /// Shows this menu from now on.
    fn set_menu(&mut self, entries: &[Entry]) -> Result<(), String>;
    /// Takes the icon away.
    fn stop(&mut self);
}

/// The system's tray, through tray-icon (see the module).
#[derive(Default)]
pub struct System {
    #[cfg(not(target_arch = "wasm32"))]
    icon: Option<tray_icon::TrayIcon>,
}

impl std::fmt::Debug for System {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("System").finish_non_exhaustive()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn muda_menu(entries: &[Entry]) -> Result<tray_icon::menu::Menu, String> {
    use tray_icon::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};

    fn fill(
        entries: &[Entry],
        append: &mut dyn FnMut(&dyn IsMenuItem) -> Result<(), String>,
    ) -> Result<(), String> {
        for e in entries {
            match e {
                Entry::Item {
                    id,
                    label,
                    enabled,
                    checked: None,
                } => append(&MenuItem::with_id(id.as_str(), label, *enabled, None))?,
                Entry::Item {
                    id,
                    label,
                    enabled,
                    checked: Some(on),
                } => append(&CheckMenuItem::with_id(
                    id.as_str(),
                    label,
                    *enabled,
                    *on,
                    None,
                ))?,
                Entry::Separator => append(&PredefinedMenuItem::separator())?,
                Entry::Submenu {
                    label,
                    enabled,
                    entries,
                } => {
                    let sub = Submenu::new(label, *enabled);
                    fill(entries, &mut |i| sub.append(i).map_err(|e| e.to_string()))?;
                    append(&sub)?;
                }
            }
        }
        Ok(())
    }

    let menu = Menu::new();
    fill(entries, &mut |i| menu.append(i).map_err(|e| e.to_string()))?;
    Ok(menu)
}

impl Backend for System {
    fn name(&self) -> String {
        if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_arch = "wasm32") {
            "none"
        } else {
            "sni"
        }
        .into()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start(&mut self, spec: &Spec, events: Events) -> Result<(), String> {
        use tray_icon::menu::MenuEvent;
        use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

        let icon =
            tray_icon::Icon::from_rgba(spec.icon.rgba.clone(), spec.icon.width, spec.icon.height)
                .map_err(|e| e.to_string())?;
        let tray = TrayIconBuilder::new()
            .with_id(spec.id.as_str())
            .with_title(&spec.title)
            .with_tooltip(&spec.tooltip)
            .with_icon(icon)
            .with_menu(Box::new(muda_menu(&[])?))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| e.to_string())?;
        let clicks = events.clone();
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            let button = match e {
                TrayIconEvent::Click {
                    button,
                    button_state: MouseButtonState::Up,
                    ..
                } => match button {
                    MouseButton::Left => Some("left"),
                    MouseButton::Middle => Some("middle"),
                    MouseButton::Right => None,
                },
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => Some("double"),
                _ => None,
            };
            if let Some(b) = button {
                clicks.send(Event::Click(b.into()));
            }
        }));
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            events.send(Event::Pick(e.id().0.clone()));
        }));
        self.icon = Some(tray);
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    fn start(&mut self, _: &Spec, _: Events) -> Result<(), String> {
        Err("no tray on the web".into())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn set_menu(&mut self, entries: &[Entry]) -> Result<(), String> {
        let menu = muda_menu(entries)?;
        if let Some(icon) = &self.icon {
            icon.set_menu(Some(Box::new(menu)));
        }
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    fn set_menu(&mut self, _: &[Entry]) -> Result<(), String> {
        Ok(())
    }

    fn stop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use tray_icon::TrayIconEvent;
            use tray_icon::menu::MenuEvent;
            TrayIconEvent::set_event_handler(None::<fn(TrayIconEvent)>);
            MenuEvent::set_event_handler(None::<fn(MenuEvent)>);
            self.icon = None;
        }
    }
}

#[derive(Debug, Default)]
struct SimState {
    started: Option<Spec>,
    menu: Vec<Entry>,
    events: Option<Events>,
}

/// A tray that records what it shows and clicks as a user would. For
/// tests and the gallery.
#[derive(Clone, Debug, Default)]
pub struct Simulated(Arc<Mutex<SimState>>);

impl Simulated {
    /// What it shows, while started.
    pub fn spec(&self) -> Option<Spec> {
        lock(&self.0).started.clone()
    }

    /// The menu it shows.
    pub fn menu(&self) -> Vec<Entry> {
        lock(&self.0).menu.clone()
    }

    /// The user clicks the icon (`left`, `middle`, `double`).
    pub fn click(&self, button: &str) {
        if let Some(e) = lock(&self.0).events.clone() {
            e.send(Event::Click(button.into()));
        }
    }

    /// The user picks the menu entry labelled `label` (in submenus too);
    /// false when there is none, or it is greyed.
    pub fn pick(&self, label: &str) -> bool {
        fn find(entries: &[Entry], label: &str) -> Option<String> {
            entries.iter().find_map(|e| match e {
                Entry::Item {
                    id,
                    label: l,
                    enabled: true,
                    ..
                } if l == label => Some(id.clone()),
                Entry::Submenu {
                    entries,
                    enabled: true,
                    ..
                } => find(entries, label),
                _ => None,
            })
        }
        let s = lock(&self.0);
        match (find(&s.menu, label), s.events.clone()) {
            (Some(id), Some(e)) => {
                drop(s);
                e.send(Event::Pick(id));
                true
            }
            _ => false,
        }
    }
}

impl Backend for Simulated {
    fn name(&self) -> String {
        "simulated".into()
    }

    fn start(&mut self, spec: &Spec, events: Events) -> Result<(), String> {
        let mut s = lock(&self.0);
        s.started = Some(spec.clone());
        s.events = Some(events);
        Ok(())
    }

    fn set_menu(&mut self, entries: &[Entry]) -> Result<(), String> {
        lock(&self.0).menu = entries.to_vec();
        Ok(())
    }

    fn stop(&mut self) {
        let mut s = lock(&self.0);
        s.started = None;
        s.events = None;
        s.menu.clear();
    }
}

#[derive(Debug, Default)]
struct TrayState {
    backend: String,
    /// Whether the icon shows, or why not.
    status: Option<Result<(), String>>,
    menu: Vec<Entry>,
}

/// The tray's state, as a service.
#[derive(Clone, Debug, Default)]
pub struct Tray(Arc<Mutex<TrayState>>);

impl Tray {
    /// The backend's name.
    pub fn backend(&self) -> String {
        lock(&self.0).backend.clone()
    }

    /// Whether the icon shows, or why not; none before the first pass.
    pub fn status(&self) -> Option<Result<(), String>> {
        lock(&self.0).status.clone()
    }

    /// The menu it shows.
    pub fn menu(&self) -> Vec<Entry> {
        lock(&self.0).menu.clone()
    }
}

/// What each entry runs, by the entry's id.
pub type Targets = BTreeMap<String, (CommandId, Json)>;

/// The tray's menu now, and what each entry runs.
pub fn entries(app: &AppHandle) -> (Vec<Entry>, Targets) {
    let mut targets = Targets::new();
    let cx = app.when_context();
    let mut next = 0;
    let out = menu_entries(app, &MENU_TRAY, &cx, &mut targets, &mut next, 0);
    (out, targets)
}

fn menu_entries(
    app: &AppHandle,
    menu: &MenuId,
    cx: &WhenContext,
    targets: &mut Targets,
    next: &mut u32,
    depth: u32,
) -> Vec<Entry> {
    let ctx = app.egui();
    let mut out = Vec::new();
    if depth > MAX_DEPTH {
        return out;
    }
    let add = |targets: &mut Targets,
               next: &mut u32,
               label: String,
               enabled: bool,
               checked: Option<bool>,
               target: (CommandId, Json)| {
        *next += 1;
        let id = format!("egui_pop.tray.{next}");
        targets.insert(id.clone(), target);
        Entry::Item {
            id,
            label,
            enabled,
            checked,
        }
    };
    let mut items = Vec::new();
    for shown in show_items(&app.menu(menu), cx) {
        match shown {
            Shown::Separator => items.push(None),
            Shown::Item {
                item,
                enabled,
                checked,
                checkable,
            } => items.push(Some((item, enabled, checked, checkable))),
        }
    }
    for i in items {
        let Some((item, enabled, checked, checkable)) = i else {
            out.push(Entry::Separator);
            continue;
        };
        match item {
            Item::Command { id, args, label } => {
                let Some(spec) = app.command(&id) else {
                    continue;
                };
                let applies =
                    spec.when.is_empty() || When::parse(&spec.when).is_ok_and(|w| w.eval(cx));
                let ticked = checkable || !spec.checked.is_empty();
                let on = checked
                    || (!spec.checked.is_empty()
                        && When::parse(&spec.checked).is_ok_and(|w| w.eval(cx)));
                let label = label.unwrap_or_else(|| app.command_menu_title(&spec));
                let enabled = enabled && applies && !app.running(&id);
                out.push(add(
                    targets,
                    next,
                    label,
                    enabled,
                    ticked.then_some(on),
                    (id, args),
                ));
            }
            Item::Submenu { menu: sub, label } => {
                let label = strings::menu_label(&ctx, sub.as_str(), &label);
                let entries = menu_entries(app, &sub, cx, targets, next, depth + 1);
                out.push(Entry::Submenu {
                    label,
                    enabled,
                    entries,
                });
            }
            Item::List { name } => {
                for l in app.list_items(&name, cx) {
                    let checked = l.checked.then_some(true);
                    out.push(add(
                        targets,
                        next,
                        l.label,
                        enabled && l.enabled,
                        checked,
                        (l.command, l.args),
                    ));
                }
            }
        }
    }
    out
}

/// The backend while the plugin is connected.
type Running = Rc<RefCell<Option<Box<dyn Backend>>>>;

/// The tray icon, as a plugin (see the module).
pub struct TrayPlugin {
    spec: Spec,
    backend: Option<Box<dyn Backend>>,
    /// The backend while connected, to stop it at disconnect.
    running: Option<Running>,
}

impl std::fmt::Debug for TrayPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrayPlugin")
            .field("spec", &self.spec.title)
            .finish_non_exhaustive()
    }
}

impl TrayPlugin {
    /// The system's tray, titled `title`.
    pub fn new(title: &str) -> Self {
        Self {
            spec: Spec {
                id: title.into(),
                title: title.into(),
                tooltip: title.into(),
                icon: Icon::disc(32, [0x35, 0x84, 0xe4]),
            },
            backend: Some(Box::new(System::default())),
            running: None,
        }
    }

    /// The application's id for the system (`org.example.App`).
    pub fn app_id(mut self, id: &str) -> Self {
        self.spec.id = id.into();
        self
    }

    pub fn tooltip(mut self, text: &str) -> Self {
        self.spec.tooltip = text.into();
        self
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.spec.icon = icon;
        self
    }

    /// Another backend ([`Simulated`] in tests).
    pub fn backend(mut self, backend: impl Backend) -> Self {
        self.backend = Some(Box::new(backend));
        self
    }
}

impl Plugin for TrayPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Tray icon".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.step(
            StepSpec::new(
                STEP_ACTIVATE,
                "The tray icon was clicked (`button`: left, middle, double); by default the window comes to the front.",
            )
            .args(json!({ "button": "left" })),
        )?;
        let mut backend = self
            .backend
            .take()
            .unwrap_or_else(|| Box::new(System::default()));
        let tray = Tray::default();
        lock(&tray.0).backend = backend.name();
        ctx.service(&TRAY, tray.clone())?;
        let (tx, rx): (Sender<Event>, Receiver<Event>) = channel();
        let events = Events {
            tx,
            waker: ctx.app().egui(),
        };
        let started = backend.start(&self.spec, events);
        if let Err(e) = &started {
            log::warn!(target: "egui_pop", "tray icon: {e}");
        }
        lock(&tray.0).status = Some(started.clone());
        let shown: Arc<Mutex<(Vec<Entry>, Targets)>> = Arc::default();
        let backend = Rc::new(RefCell::new(Some(backend)));
        self.running = Some(backend.clone());
        ctx.on(On::Pass, move |app, _| {
            let (menu, targets) = entries(app);
            {
                let mut s = lock(&shown);
                if started.is_ok() && s.0 != menu {
                    if let Some(b) = backend.borrow_mut().as_mut()
                        && let Err(e) = b.set_menu(&menu)
                    {
                        log::warn!(target: "egui_pop", "tray menu: {e}");
                    }
                    lock(&tray.0).menu = menu.clone();
                    s.0 = menu;
                }
                s.1 = targets;
            }
            for e in rx.try_iter() {
                match e {
                    Event::Click(button) => {
                        app.step(&STEP_ACTIVATE, json!({ "button": button }), |app, args| {
                            crate::window::raise(&app.egui());
                            Ok(json!({ "raised": true, "button": args["button"] }))
                        });
                    }
                    Event::Pick(id) => {
                        let target = lock(&shown).1.get(&id).cloned();
                        if let Some((command, args)) = target {
                            app.run_as(command, args, Origin::User);
                        }
                    }
                }
            }
        });
        Ok(())
    }

    fn disconnect(&mut self, _ctx: &mut Ctx<'_>) {
        if let Some(b) = self.running.take()
            && let Some(mut b) = b.borrow_mut().take()
        {
            b.stop();
            self.backend = Some(b);
        }
    }
}
