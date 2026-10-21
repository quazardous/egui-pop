//! The egui-pop gallery: a catalogue application with no purpose of its
//! own, built as an egui-pop plugin application. Every page is a plugin
//! contributing a story; the shell plugin lists them in the side bar,
//! draws the page in the main zone and its playground in the inspector,
//! and puts the theme switchers in the top bar.
//!
//! Pages: the theme overview, the token table, the theme editor and
//! egui's built-in widgets (plain functions reused by the snapshot tests); zones and
//! blocks (one plugin replacing, extending and wrapping another's
//! blocks); settings applied live; translations; the platform services
//! (simulated unless `--platform`); the debug bridge.

use std::io::Write as _;

use egui::Ui;
use egui_pop::api::{App, AppBuilder, BuildError};
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::theme::ThemeSpec;

pub mod host;
mod options;
pub mod pages;
pub mod shell;

pub use options::{Options, USAGE};

use pages::story::PagePlugin;

/// The window title, also printed in the start-up line.
pub const TITLE: &str = "egui-pop gallery";

/// The pages, as named on the command line.
pub const PAGES: [&str; 49] = [
    "overview",
    "tokens",
    "builtins",
    "scoped",
    "theme-editor",
    "fonts",
    "icons",
    "indicators",
    "motion",
    "values",
    "sliders",
    "inputs",
    "menus",
    "dialogs",
    "files",
    "toasts",
    "tips",
    "progress",
    "placeholders",
    "viewport",
    "navigator",
    "images",
    "grading",
    "cursors",
    "node-graph",
    "graph-file",
    "display",
    "preview",
    "tree",
    "table",
    "library",
    "text-editor",
    "code-editor",
    "terminal",
    "zones",
    "workbench",
    "palettes",
    "arrangements",
    "contexts",
    "strips",
    "jobs",
    "frame",
    "scale",
    "pointer",
    "platform",
    "settings",
    "i18n",
    "debug",
    "roles",
];

/// The gallery's plugins.
pub fn builder(options: &Options) -> AppBuilder {
    let mut spec = ThemeSpec::new(options.theme.clone())
        .density(options.density)
        .reduced_motion(options.reduced_motion);
    spec.accent = options.accent.clone();
    let jobs_sim = pages::jobs::Sim::default();
    let waiting = host::Waiting::default();
    let builder = App::builder()
        .host(host::GalleryHost::new(waiting.clone()))
        .name("egui-pop gallery")
        .theme(spec)
        .plugins(egui_pop::api::DefaultPlugins)
        .plugin(egui_pop::mounts::MountsPlugin)
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
            plugin: PluginId::from_static("gallery.scoped"),
            story: StoryId::from_static("scoped"),
            title: "egui widgets by kit",
            description: "egui's own buttons, combo boxes and text fields, drawn by egui with a kit component's states set in a scope.",
            group: "Theme",
            weight: 25,
            show: pages::scoped::show,
            playground: None,
        })
        .plugin(PagePlugin {
            plugin: PluginId::from_static("gallery.roles"),
            story: StoryId::from_static("roles"),
            title: "Layout by role",
            description: "The UiPop helpers: text roles, spacing by intent, cards and buttons by tone, each read from the theme.",
            group: "Layout",
            weight: 200,
            show: pages::roles::show,
            playground: None,
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
        .plugin(PagePlugin {
            plugin: PluginId::from_static("gallery.indicators"),
            story: StoryId::from_static("indicators"),
            title: "Indicators",
            description: "Micro-indicators: dots and LEDs, badges, activity pips, flags, swatches and icons by state, passive or interactive, inline or on a widget, drawn by the kit.",
            group: "Components",
            weight: 31,
            show: pages::indicators::show,
            playground: Some(pages::indicators::playground),
        })
        .plugin(PagePlugin {
            plugin: PluginId::from_static("gallery.motion"),
            story: StoryId::from_static("motion"),
            title: "Motion",
            description: "The motion clock (frozen in tests, still under reduced motion), the motion tokens and the busy delay.",
            group: "Progress and jobs",
            weight: 131,
            show: pages::motion::show,
            playground: None,
        })
        .plugin(pages::values::ValuesPage)
        .plugin(pages::sliders::SlidersPage)
        .plugin(pages::inputs::InputsPage)
        .plugin(pages::menus::MenusPage)
        .plugin(pages::dialogs::DialogsPage)
        .plugin(pages::files::FilesPage)
        .plugin(pages::toasts::ToastsPage)
        .plugin(egui_pop::api::theme_editor::ThemeEditorPlugin)
        .plugin(pages::theme_editor::ThemeEditorPage)
        .plugin(pages::fonts::FontsPage)
        .plugin(egui_pop::api::tips::TipsPlugin)
        .plugin(pages::tips::TipsPage)
        .plugin(PagePlugin {
            plugin: PluginId::from_static("gallery.placeholders"),
            story: StoryId::from_static("placeholders"),
            title: "Placeholders",
            description: "Skeletons standing in for loading content (no shimmer in tests nor under reduced motion), and empty states with their actions.",
            group: "Progress and jobs",
            weight: 133,
            show: pages::placeholders::show,
            playground: None,
        })
        .plugin(pages::progress::ProgressPage {
            waiting: waiting.clone(),
        })
        .plugin(pages::viewport::ViewportPage)
        .plugin(egui_pop::imaging::ImagingPlugin)
        .plugin(pages::navigator::NavigatorPage)
        .plugin(pages::grading::GradingPage)
        .plugin(pages::cursors::CursorsPage)
        .plugin(pages::node_graph::NodeGraphPage)
        .plugin(pages::graph_file::GraphFilePage)
        .plugin(PagePlugin {
            plugin: PluginId::from_static("gallery.images"),
            story: StoryId::from_static("images"),
            title: "Image widgets",
            description: "Thumbnails in a virtualised grid (an image, or its average colour until it is there) and a histogram from supplied bins, with clipping markers and a drag adjusting an exposure.",
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
        .plugin(pages::preview::PreviewPage)
        .plugin(egui_pop::views::ViewsPlugin)
        .plugin(pages::tree::TreePage)
        .plugin(pages::table::TablePage)
        .plugin(egui_pop::views::library::LibraryPlugin)
        .plugin(pages::library::LibraryPage)
        .plugin(egui_pop::editors::EditorsPlugin)
        .plugin(pages::editors::TextPage)
        .plugin(pages::editors::CodeEditorPage)
        .plugin(egui_pop::editors::TerminalPlugin)
        .plugin(egui_pop::editors::JobConsolesPlugin)
        .plugin(pages::terminal::TerminalPage)
        .plugin(pages::zones::ZonesPage)
        .plugin(pages::zones::Extender)
        .plugin(pages::workbench::WorkbenchPage)
        .plugin(pages::palettes::PalettesPage)
        .plugin(pages::arrangements::ArrangementsPage::default())
        .plugin(egui_pop::api::contexts::ContextsPlugin::default().root("All photos"))
        .plugin(pages::contexts::ContextsPage::default())
        .plugin(egui_pop::api::strips::StripsPlugin::default())
        .plugin(pages::strips::StripsPage)
        .plugin(jobs_plugin(&jobs_sim))
        .plugin(pages::jobs::JobsPage { sim: jobs_sim })
        .plugin(egui_pop::api::devtools::DevtoolsPlugin::default())
        .plugin(pages::frame::FramePage)
        .plugin(pages::scale::ScalePage)
        .plugin(pages::pointer::PointerPage)
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

/// The jobs plugin, its commands answered by the jobs page's pretend
/// host.
fn jobs_plugin(sim: &pages::jobs::Sim) -> egui_pop::api::jobs::JobsPlugin {
    let sim = sim.clone();
    egui_pop::api::jobs::JobsPlugin::default()
        .run_locally(move |app, request, id| sim.answer(app, request, id))
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
            #[cfg(not(target_arch = "wasm32"))]
            if options.wide_gamut {
                use egui_pop::viewport::{display, image_surface};
                let offer = image_surface::install(
                    &cc.egui_ctx,
                    rs,
                    cc,
                    image_surface::Options {
                        transparent_window: true,
                    },
                );
                if let Some(why) = offer.unavailable {
                    eprintln!("wide gamut: no image surface: {why}");
                }
                display::want(
                    &cc.egui_ctx,
                    egui::ViewportId::ROOT,
                    pages::display::Wanted::Rec2020.target(),
                );
            }
        }
        Gallery::new(&cc.egui_ctx, options)
    }

    /// The gallery on a context, its theme installed.
    pub fn new(ctx: &egui::Context, options: Options) -> Result<Gallery, BuildError> {
        let app = builder(&options).build(ctx)?;
        // The theme asked for is shown, whatever the system's light or
        // dark mode: the switcher's "Mode" offers to follow it.
        let service = app.handle().theme();
        service.set_preference(if service.is_dark(&options.theme) {
            egui::ThemePreference::Dark
        } else {
            egui::ThemePreference::Light
        });
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
        // What the web smoke test waits for (tools/web-smoke.mjs).
        #[cfg(target_arch = "wasm32")]
        if self.frames == 1 {
            eframe::web_sys::console::info_1(&"egui-pop gallery started".into());
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(&mut *self)
    }
}
