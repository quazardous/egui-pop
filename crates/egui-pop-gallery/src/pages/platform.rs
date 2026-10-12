//! The platform services page: a single instance and deep links,
//! notifications, the tray, global shortcuts, logs, crash reports and
//! the focus mode. By default the desktop is simulated (nothing reaches
//! the machine's, and pictures stay the same); `--platform` uses the
//! real one, with buttons playing the user only where it is simulated.

use std::path::PathBuf;

use egui::Ui;
use egui_pop::api::app::FOCUS_MODE;
use egui_pop::api::menus::ListItem;
use egui_pop::api::services::ServiceToken;
use egui_pop::api::stories::Story;
use egui_pop::api::system_keys::{KeyStatus, SYSTEM_KEYS, Support};
use egui_pop::api::{AppBuilder, AppHandle, Ctx, Plugin, PluginError};
use egui_pop::model::commands::CommandSpec;
use egui_pop::model::ids::{CommandId, MenuId, PluginId, StoryId};
use egui_pop::model::keymap::BindOptions;
use egui_pop::model::menus::MenuItem;
use egui_pop::platform::crash::{self, CRASHES, CrashPlugin, Report};
use egui_pop::platform::focus::FocusModePlugin;
use egui_pop::platform::instance::{Instance, InstancePlugin, LAUNCHES, Launch};
use egui_pop::platform::keys::{self, SystemKeysPlugin};
use egui_pop::platform::logs::{LOGS, Level, Logs, LogsPlugin};
use egui_pop::platform::notify::{self, Desktop, NOTIFIER, Notification, NotifyPlugin};
use egui_pop::platform::tray::{self, Entry, MENU_TRAY, TRAY, TrayPlugin};
use serde_json::{Value as Json, json};

use super::section;
use crate::shell::{CMD_NEXT, CMD_PAGE, CMD_THEME, SHELL};

pub const PLATFORM_PAGE: PluginId = PluginId::from_static("gallery.platform");
/// The gallery's id with the desktop.
pub const APP_ID: &str = "org.egui_pop.Gallery";
/// Opens a `gallery://page/<name>` link.
pub const CMD_OPEN_LINK: CommandId = CommandId::from_static("gallery.open_link");
/// Counts its runs: bound to Ctrl+Alt+G in the system scope.
pub const CMD_PING: CommandId = CommandId::from_static("gallery.ping");
/// What a notification's action runs: keeps its arguments.
pub const CMD_NOTIFIED: CommandId = CommandId::from_static("gallery.notified");
/// The tray's submenu of pages.
const MENU_PAGES: MenuId = MenuId::from_static("gallery.tray.pages");

/// The simulated desktop, when the gallery uses one.
#[derive(Clone, Debug)]
pub struct Sim {
    pub notify: notify::Simulated,
    pub tray: tray::Simulated,
    pub keys: keys::Simulated,
    /// Where the logs and crash reports go.
    pub scratch: PathBuf,
}

/// The simulated desktop, as a service (absent with `--platform`).
pub const SIM: ServiceToken<Sim> = ServiceToken::new("gallery.platform.sim");

/// The platform services, simulated or real, and this page.
pub fn add(builder: AppBuilder, real: bool, first: Option<Instance>) -> AppBuilder {
    let first = first.unwrap_or_else(|| Instance::detached(Launch::default()));
    let builder = builder.plugin(
        InstancePlugin::new(first)
            .scheme("gallery")
            .links(CMD_OPEN_LINK),
    );
    if real {
        builder
            .plugin(
                NotifyPlugin::new("egui-pop gallery")
                    .backend(Desktop::new("egui-pop gallery").app_id(APP_ID)),
            )
            .plugin(TrayPlugin::new("egui-pop gallery").app_id(APP_ID))
            .plugin(SystemKeysPlugin::new(APP_ID))
            .plugin(LogsPlugin::new("egui-pop-gallery"))
            .plugin(CrashPlugin::new(
                "egui-pop-gallery",
                env!("CARGO_PKG_VERSION"),
            ))
            .plugin(FocusModePlugin::new())
            .plugin(PlatformPage { sim: None })
    } else {
        let sim = Sim {
            notify: notify::Simulated::default(),
            tray: tray::Simulated::default(),
            keys: keys::Simulated::default(),
            scratch: std::env::temp_dir().join(format!("egui-pop-gallery-{}", std::process::id())),
        };
        builder
            .plugin(NotifyPlugin::new("egui-pop gallery").backend(sim.notify.clone()))
            .plugin(TrayPlugin::new("egui-pop gallery").backend(sim.tray.clone()))
            .plugin(SystemKeysPlugin::new(APP_ID).backend(sim.keys.clone()))
            .plugin(LogsPlugin::new("egui-pop-gallery").logs(Logs::new(&sim.scratch, "gallery")))
            .plugin(
                CrashPlugin::new("egui-pop gallery", env!("CARGO_PKG_VERSION"))
                    .dir(sim.scratch.join("crashes"))
                    .hook(false),
            )
            .plugin(FocusModePlugin::fixed(Default::default()))
            .plugin(PlatformPage { sim: Some(sim) })
    }
}

/// The platform services page.
#[derive(Debug)]
pub struct PlatformPage {
    sim: Option<Sim>,
}

impl Plugin for PlatformPage {
    fn id(&self) -> PluginId {
        PLATFORM_PAGE
    }

    fn title(&self) -> String {
        "Platform services".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        if let Some(sim) = &self.sim {
            ctx.service(&SIM, sim.clone())?;
        }
        ctx.command(
            CommandSpec::new(CMD_OPEN_LINK, "Open a gallery link", "Gallery")
                .description("Opens a `gallery://page/<name>` link (`url`).")
                .args(json!({ "url": "gallery://page/tokens" }))
                .hidden(),
            |call| {
                let url = call.args["url"].as_str().ok_or("`url` is missing")?;
                let page = url
                    .split_once("://")
                    .and_then(|(_, rest)| rest.strip_prefix("page/"))
                    .ok_or_else(|| format!("`{url}` names no page"))?;
                call.app.run(CMD_PAGE, json!({ "page": page }));
                Ok(json!({ "page": page }))
            },
        )?
        .command(
            CommandSpec::new(CMD_PING, "Ping the gallery", "Gallery")
                .description("Counts its runs: a global shortcut, also outside the window.")
                .key_with("Ctrl+Alt+G", BindOptions::system())
                .menu(MENU_TRAY, "2_gallery", 0),
            |call| {
                let n = call
                    .app
                    .context()
                    .get("galleryPings")
                    .parse::<u64>()
                    .unwrap_or(0)
                    + 1;
                call.app.set_context("galleryPings", &n.to_string());
                Ok(json!(n))
            },
        )?
        .command(
            CommandSpec::new(CMD_NOTIFIED, "Show what a notification ran", "Gallery")
                .hidden()
                .no_agent(),
            |call| {
                call.app
                    .set_context("galleryNotified", &call.args.to_string());
                Ok(Json::Null)
            },
        )?;
        ctx.menu_item(MenuItem::command(MENU_TRAY, "1_pages", 0, CMD_NEXT))?
            .menu_item(MenuItem::command(MENU_TRAY, "1_pages", 1, CMD_THEME))?
            .submenu(MENU_TRAY, "1_pages", 2, MENU_PAGES, "Pages")?
            .menu_item(MenuItem::list(MENU_PAGES, "1", 0, "gallery.tray.pages"))?;
        ctx.list_provider("gallery.tray.pages", |app, _| {
            crate::shell::ordered(app)
                .into_iter()
                .map(|s| ListItem::new(&s.title, CMD_PAGE, json!({ "page": s.id })))
                .collect()
        });
        ctx.story(
            Story::new(StoryId::from_static("platform"), "Platform services", show)
                .description("A single instance and deep links, notifications, the tray, global shortcuts, logs, crash reports and the focus mode: each an opt-in plugin, its moments steps.")
                .group("Framework", 108),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    let sim = app.service(&SIM);
    ui.label(if sim.is_some() {
        "The desktop is simulated: nothing reaches this machine's. The buttons below play the user. Run with --platform to use the real one."
    } else {
        "The desktop's own services (--platform)."
    });
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            instance_section(ui, app);
            notify_section(ui, app, sim.as_deref());
            tray_section(ui, app, sim.as_deref());
            keys_section(ui, app, sim.as_deref());
            logs_section(ui, app);
            crash_section(ui, app, sim.as_deref());
            focus_section(ui, app);
        });
}

fn instance_section(ui: &mut Ui, app: &AppHandle) {
    section(ui, "Single instance and deep links");
    let Some(launches) = app.service(&LAUNCHES) else {
        ui.weak("The instance plugin is not connected.");
        return;
    };
    ui.label("A second launch hands its arguments to the running one; a gallery://page/<name> link shows that page.");
    if ui.button("Simulate a second launch with a link").clicked() {
        // The link shows the tokens page.
        launches.deliver(Launch {
            args: vec!["gallery://page/tokens".into()],
            ..Launch::default()
        });
    }
    for l in launches.history().iter().rev().take(3) {
        ui.monospace(l.args.join(" "));
    }
}

fn notify_section(ui: &mut Ui, app: &AppHandle, sim: Option<&Sim>) {
    section(ui, "Notifications");
    let Some(notifier) = app.service(&NOTIFIER) else {
        ui.weak("The notifications plugin is not connected.");
        return;
    };
    ui.label(match notifier.desktop() {
        None => "Asking the desktop…".to_string(),
        Some(Ok(server)) => {
            format!("Desktop notifications through {server}; toasts when the setting says so.")
        }
        Some(Err(why)) => format!("Toasts only: {why}."),
    });
    ui.horizontal_wrapped(|ui| {
        if ui.button("Notify with an action").clicked() {
            notify::show(
                app,
                Notification::new("Export done", "3 images in Pictures")
                    .level(notify::Level::Success)
                    .action(
                        "show",
                        "Show",
                        CMD_NOTIFIED,
                        json!({ "what": "show the export" }),
                    ),
            );
        }
        if ui.button("Notify from a thread").clicked() {
            let n = (*notifier).clone();
            std::thread::spawn(move || n.post(Notification::new("Import done", "120 images")));
        }
    });
    for shown in notifier.history().iter().rev().take(3) {
        ui.horizontal_wrapped(|ui| {
            ui.monospace(format!(
                "#{} {:?}: {}",
                shown.id, shown.via, shown.notification.title
            ));
            if let Some(sim) = sim
                && shown.via == notify::Via::Desktop
                && let Some(a) = shown.notification.actions.first()
                && shown.picked.is_empty()
                && ui.button(format!("Click {}", a.label)).clicked()
            {
                sim.notify.click(shown.id, &a.id);
            }
        });
    }
    let sent = app.context().get("galleryNotified").to_string();
    if !sent.is_empty() {
        ui.monospace(format!("Ran: {sent}"));
    }
}

fn entries_text(entries: &[Entry], depth: usize, out: &mut Vec<String>) {
    for e in entries {
        let pad = "  ".repeat(depth);
        match e {
            Entry::Item {
                label,
                enabled,
                checked,
                ..
            } => out.push(format!(
                "{pad}{}{label}{}",
                match checked {
                    Some(true) => "[x] ",
                    Some(false) => "[ ] ",
                    None => "",
                },
                if *enabled { "" } else { " (greyed)" }
            )),
            Entry::Separator => out.push(format!("{pad}—")),
            Entry::Submenu { label, entries, .. } => {
                out.push(format!("{pad}{label} ›"));
                if depth == 0 {
                    entries_text(&entries[..entries.len().min(3)], depth + 1, out);
                }
            }
        }
    }
}

fn tray_section(ui: &mut Ui, app: &AppHandle, sim: Option<&Sim>) {
    section(ui, "Tray icon");
    let Some(tray) = app.service(&TRAY) else {
        ui.weak("The tray plugin is not connected.");
        return;
    };
    ui.label(match tray.status() {
        Some(Ok(())) => format!(
            "Shown through {}; its menu is the declared menu app.tray.",
            tray.backend()
        ),
        Some(Err(why)) => format!("Not shown: {why}."),
        None => "Starting…".into(),
    });
    let mut lines = Vec::new();
    entries_text(&tray.menu(), 0, &mut lines);
    ui.monospace(lines.join("\n"));
    if let Some(sim) = sim {
        ui.horizontal_wrapped(|ui| {
            if ui.button("Click the icon").clicked() {
                sim.tray.click("left");
            }
            if ui.button("Pick Ping the gallery").clicked() {
                sim.tray.pick("Ping the gallery");
            }
        });
    }
}

fn keys_section(ui: &mut Ui, app: &AppHandle, sim: Option<&Sim>) {
    section(ui, "Global shortcuts");
    let Some(keys) = app.service(&SYSTEM_KEYS) else {
        ui.weak("The global shortcuts plugin is not connected.");
        return;
    };
    ui.label(match keys.support() {
        Support::Unknown => "Asking the desktop…".to_string(),
        Support::Available { backend } => {
            format!("The desktop takes global shortcuts ({backend}).")
        }
        Support::Unavailable { reason } => format!("Only in the window: {reason}."),
    });
    for (k, status) in keys.statuses() {
        ui.monospace(match status {
            KeyStatus::Registered => format!("{k}: registered"),
            KeyStatus::Pending => format!("{k}: waiting for the desktop"),
            KeyStatus::Failed(why) => format!("{k}: refused, {why}"),
        });
    }
    let pings = app.context().get("galleryPings").to_string();
    ui.horizontal_wrapped(|ui| {
        ui.label(format!(
            "Ctrl+Alt+G pinged {} time(s).",
            if pings.is_empty() { "0" } else { &pings }
        ));
        if let Some(sim) = sim
            && ui.button("Press Ctrl+Alt+G outside the window").clicked()
        {
            sim.keys.press("Ctrl+Alt+G");
        }
    });
}

fn logs_section(ui: &mut Ui, app: &AppHandle) {
    section(ui, "Logs");
    let Some(logs) = app.service(&LOGS) else {
        ui.weak("The logs plugin is not connected.");
        return;
    };
    ui.label(format!(
        "Level {} (the developer.log_level setting), rotated by size.",
        logs.level().to_string().to_lowercase()
    ));
    if ui.button("Write a line").clicked() {
        logs.write(Level::Info, "gallery", "a line from the platform page");
    }
    let tail = logs.tail();
    let shown: Vec<String> = tail
        .iter()
        .rev()
        .take(3)
        .rev()
        // The time differs at each run.
        .map(|l| {
            l.split_once(' ')
                .map_or(l.clone(), |(_, rest)| rest.to_string())
        })
        .collect();
    ui.monospace(shown.join("\n"));
}

fn crash_section(ui: &mut Ui, app: &AppHandle, sim: Option<&Sim>) {
    section(ui, "Crash reports");
    let Some(crashes) = app.service(&CRASHES) else {
        ui.weak("The crash reports plugin is not connected.");
        return;
    };
    ui.label("A panic writes a report; the next start offers it. Nothing is sent.");
    if let Some(sim) = sim
        && ui.button("Simulate a crash in a past session").clicked()
    {
        let dir = sim.scratch.join("crashes");
        let report = Report::panic(
            "egui-pop gallery",
            env!("CARGO_PKG_VERSION"),
            "index out of bounds",
        );
        if crash::simulate(&dir, 4_190_000, &report).is_ok() {
            crashes.scan();
            app.egui().request_repaint();
        }
    }
    for path in crashes.reports().iter().rev().take(3) {
        ui.horizontal_wrapped(|ui| {
            let summary = crashes.read(path).map(|r| r.summary()).unwrap_or_default();
            ui.monospace(summary);
            if ui.button("Dismiss").clicked() {
                let _ = crashes.dismiss(path);
            }
        });
    }
}

fn focus_section(ui: &mut Ui, app: &AppHandle) {
    section(ui, "Focus mode");
    let Some(modes) = app.service(&FOCUS_MODE) else {
        return;
    };
    let system = modes.system();
    ui.label(format!(
        "The desktop says {:?}{}; read in-process.",
        system.mode,
        system.from.map(|f| format!(" ({f})")).unwrap_or_default()
    ));
}
