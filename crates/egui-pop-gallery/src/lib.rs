//! The egui-pop gallery: a catalogue application with no purpose of its
//! own, built as an egui-pop plugin application. Every page is a plugin
//! contributing a story; the shell plugin lists them in the side bar,
//! draws the page in the main zone and its playground in the inspector,
//! and puts the theme switchers in the top bar.
//!
//! Pages: the theme overview, the token table and egui's built-in
//! widgets (plain functions reused by the snapshot tests); zones and
//! blocks (one plugin replacing, extending and wrapping another's
//! blocks); settings applied live; translations; the platform services
//! (simulated unless `--platform`); the debug bridge.

use std::io::Write as _;

use egui::Ui;
use egui_pop::api::{App, AppBuilder, BuildError};
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::theme::ThemeSpec;

mod options;
pub mod pages;
pub mod shell;

pub use options::{Options, USAGE};

use pages::story::PagePlugin;

/// The window title, also printed in the start-up line.
pub const TITLE: &str = "egui-pop gallery";

/// The pages, as named on the command line.
pub const PAGES: [&str; 27] = [
    "overview",
    "tokens",
    "builtins",
    "icons",
    "values",
    "sliders",
    "menus",
    "dialogs",
    "files",
    "toasts",
    "viewport",
    "images",
    "display",
    "tree",
    "table",
    "text-editor",
    "code-editor",
    "zones",
    "workbench",
    "arrangements",
    "strips",
    "frame",
    "scale",
    "platform",
    "settings",
    "i18n",
    "debug",
];

/// The gallery's plugins.
pub fn builder(options: &Options) -> AppBuilder {
    let mut spec = ThemeSpec::new(options.theme.clone())
        .density(options.density)
        .reduced_motion(options.reduced_motion);
    spec.accent = options.accent.clone();
    let builder = App::builder()
        .name("egui-pop gallery")
        .theme(spec)
        .plugins(egui_pop::api::DefaultPlugins)
        .plugin(egui_pop::api::frame::FramePlugin::system())
        .plugin(shell::Shell {
            first: StoryId::new(options.page.as_str()).unwrap_or(StoryId::from_static("overview")),
        })
        .plugin(PagePlugin {
            plugin: PluginId::from_static("gallery.overview"),
            story: StoryId::from_static("overview"),
            title: "Theme overview",
            description: "Surfaces, text, accents, status colours, spacing and the contrast checks of the current theme.",
            group: "Theme",
            weight: 0,
            show: pages::overview::show,
            playground: Some(pages::overview::playground),
        })
        .plugin(PagePlugin {
            plugin: PluginId::from_static("gallery.tokens"),
            story: StoryId::from_static("tokens"),
            title: "Tokens table",
            description: "Every token of the current theme: palette, roles and components.",
            group: "Theme",
            weight: 10,
            show: pages::tokens::show,
            playground: Some(pages::tokens::playground),
        })
        .plugin(PagePlugin {
            plugin: PluginId::from_static("gallery.builtins"),
            story: StoryId::from_static("builtins"),
            title: "egui built-ins",
            description: "egui's own widgets, styled from the theme's tokens with no per-widget code.",
            group: "Theme",
            weight: 20,
            show: pages::builtins::show,
            playground: Some(pages::builtins::playground),
        })
        .plugin(PagePlugin {
            plugin: PluginId::from_static("gallery.icons"),
            story: StoryId::from_static("icons"),
            title: "Icons",
            description: "Icons by role, never by codepoint: drawn from the icon font, or painted.",
            group: "Components",
            weight: 30,
            show: pages::icons::show,
            playground: Some(pages::icons::playground),
        })
        .plugin(pages::values::ValuesPage)
        .plugin(pages::sliders::SlidersPage)
        .plugin(pages::menus::MenusPage)
        .plugin(pages::dialogs::DialogsPage)
        .plugin(pages::files::FilesPage)
        .plugin(pages::toasts::ToastsPage)
        .plugin(pages::viewport::ViewportPage)
        .plugin(PagePlugin {
            plugin: PluginId::from_static("gallery.images"),
            story: StoryId::from_static("images"),
            title: "Image widgets",
            description: "Thumbnails in a virtualised grid (an image, or its average colour until it is there) and a histogram from supplied bins.",
            group: "Components",
            weight: 75,
            show: pages::images::show,
            playground: Some(pages::images::playground),
        })
        .plugin(PagePlugin {
            plugin: PluginId::from_static("gallery.display"),
            story: StoryId::from_static("display"),
            title: "Wide gamut and HDR",
            description: "What a window draws for when the application wants Display P3 or HDR, what each surface carries, and the PQ and HLG curves.",
            group: "Components",
            weight: 77,
            show: pages::display::show,
            playground: None,
        })
        .plugin(egui_pop::views::ViewsPlugin)
        .plugin(pages::tree::TreePage)
        .plugin(pages::table::TablePage)
        .plugin(egui_pop::editors::EditorsPlugin)
        .plugin(pages::editors::TextPage)
        .plugin(pages::editors::CodeEditorPage)
        .plugin(pages::zones::ZonesPage)
        .plugin(pages::zones::Extender)
        .plugin(pages::workbench::WorkbenchPage)
        .plugin(pages::arrangements::ArrangementsPage::default())
        .plugin(egui_pop::api::strips::StripsPlugin::default())
        .plugin(pages::strips::StripsPage)
        .plugin(egui_pop::api::devtools::DevtoolsPlugin::default())
        .plugin(pages::frame::FramePage)
        .plugin(pages::scale::ScalePage)
        .plugin(pages::settings::SettingsPage)
        .plugin(
            egui_pop::i18n::I18nPlugin::new()
                .catalogues(pages::i18n::CATALOGUES)
                .language(&options.lang)
                .env("EGUI_POP_LANG"),
        )
        .plugin(egui_pop::i18n::ShowKeys)
        .plugin(pages::i18n::I18nPage)
        .plugin(pages::debug::DebugPage);
    pages::platform::add(builder, options.platform, take_instance())
}

static INSTANCE: std::sync::Mutex<Option<egui_pop::platform::instance::Instance>> =
    std::sync::Mutex::new(None);

/// Gives the gallery the single instance `main` made (with
/// `--platform`); the next build takes it.
pub fn set_instance(instance: egui_pop::platform::instance::Instance) {
    *INSTANCE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(instance);
}

fn take_instance() -> Option<egui_pop::platform::instance::Instance> {
    INSTANCE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
}

/// The gallery application: the egui-pop app, and the options of a
/// scripted run.
#[derive(Debug)]
pub struct Gallery {
    pub app: App,
    frames: u64,
    quit_after: Option<u64>,
    announce: bool,
    dump_debug: bool,
}

impl Gallery {
    /// The gallery from eframe's creation context: [`Gallery::new`], and
    /// the image viewport's GPU path on eframe's wgpu device.
    pub fn from_creation_context(
        cc: &eframe::CreationContext<'_>,
        options: Options,
    ) -> Result<Gallery, BuildError> {
        if let Some(rs) = cc.wgpu_render_state.as_ref() {
            egui_pop::viewport::gpu::install(&cc.egui_ctx, rs);
        }
        Gallery::new(&cc.egui_ctx, options)
    }

    /// The gallery on a context, its theme installed.
    pub fn new(ctx: &egui::Context, options: Options) -> Result<Gallery, BuildError> {
        let app = builder(&options).build(ctx)?;
        if options.announce {
            let spec = app.handle().theme().spec();
            announce(&serde_json::json!({
                "event": "ready",
                "app": "egui-pop-gallery",
                "version": env!("CARGO_PKG_VERSION"),
                "title": TITLE,
                "theme": spec.base,
                "density": spec.density.name(),
                "accent": spec.accent,
                "reduced_motion": spec.reduced_motion,
                "page": options.page,
                "quit_after": options.quit_after,
                "plugins": app.handle().plugins().iter().map(|p| p.id.to_string()).collect::<Vec<_>>(),
                "diagnostics": app.handle().diagnostics(),
            }));
        }
        Ok(Gallery {
            app,
            frames: 0,
            quit_after: options.quit_after,
            announce: options.announce,
            dump_debug: options.dump_debug,
        })
    }

    /// The page shown.
    pub fn page(&self) -> StoryId {
        self.app
            .handle()
            .service(&shell::NAV)
            .map(|n| n.page())
            .unwrap_or(StoryId::from_static("overview"))
    }

    /// Draws the whole gallery in a `Ui` covering the window.
    pub fn show(&mut self, ui: &mut Ui) {
        self.app.show(ui);
    }

    /// Between frames: the app's logic, the frame count, quitting.
    pub fn logic(&mut self, ctx: &egui::Context) {
        self.app.logic(ctx);
        self.frames += 1;
        if let Some(limit) = self.quit_after {
            if self.frames >= limit {
                if self.dump_debug {
                    announce(&serde_json::json!({
                        "event": "debug",
                        "snapshot": self.app.handle().debug_snapshot(),
                    }));
                }
                if self.announce {
                    announce(&serde_json::json!({ "event": "quit", "frames": self.frames }));
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.request_repaint();
            }
        }
    }
}

fn announce(line: &serde_json::Value) {
    let mut out = std::io::stdout().lock();
    // A closed stdout is no reason to stop the gallery.
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

impl eframe::App for Gallery {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        Gallery::logic(self, ctx);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }

    #[cfg(target_arch = "wasm32")]
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(&mut *self)
    }
}
