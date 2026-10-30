//! The viewer page: the gallery's own help pages (`docs:`) opened in the
//! viewer panel, an HTML page in its safe subset, a page that is not
//! there, an anchor, a remote image waiting for the user, and short texts
//! in the inline subset.

use std::sync::Arc;

use egui::Ui;
use egui_pop::api::links::{self, Inline};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::views::viewer::{self, Docs, Fetch, Fetched, ViewerPlugin};

use super::section;
use crate::shell::SHELL;

pub const VIEWER_PAGE: PluginId = PluginId::from_static("gallery.viewer");

/// The image a page shows from another host: the gallery pretends to
/// fetch it, once its host is allowed.
pub const REMOTE_IMAGE: &str = "https://images.example.org/banner.png";

/// An HTML page on a documentation host: the gallery pretends to fetch
/// it, for the tests that declare the host.
pub const REMOTE_PAGE: &str = "https://docs.example.org/page.html";

const GRADIENT: &[u8] = include_bytes!("viewer_docs/gradient.png");
const REMOTE_HTML: &str = "<h1>A page from the network</h1>\
    <p><a href=\"https://example.org/more\">More</a> and \
    <a href=\"command:gallery.nothing\">a command it may not run</a>.</p>\
    <p><img src=\"file:///etc/hostname\" alt=\"A local file\"></p>\
    <p><img src=\"https://images.example.org/banner.png\" alt=\"A remote image\"></p>";

/// The gallery's help pages.
pub fn docs() -> Docs {
    Docs::new()
        .page("index", include_str!("viewer_docs/index.md"))
        .page("guide", include_str!("viewer_docs/guide.md"))
        .page("about.html", include_str!("viewer_docs/about.html"))
        .file("gradient.png", GRADIENT)
}

/// A pretend network: the remote image answers with the gradient,
/// anything else is unreachable. Nothing leaves the machine.
#[derive(Debug, Default)]
pub struct PretendFetch;

impl Fetch for PretendFetch {
    fn fetch(&self, url: &str, done: Box<dyn FnOnce(Fetched) + Send>) {
        if url == REMOTE_IMAGE {
            done(Ok(GRADIENT.to_vec()));
        } else if url == REMOTE_PAGE {
            done(Ok(REMOTE_HTML.as_bytes().to_vec()));
        } else {
            done(Err("the gallery does not reach the network".into()));
        }
    }
}

/// The viewer plugin as the gallery connects it.
pub fn plugin() -> ViewerPlugin {
    ViewerPlugin::default()
        .docs(docs())
        .fetch(Arc::new(PretendFetch))
}

/// The page.
#[derive(Debug, Default)]
pub struct ViewerPage;

impl Plugin for ViewerPage {
    fn id(&self) -> PluginId {
        VIEWER_PAGE
    }

    fn title(&self) -> String {
        "Viewer".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, links::PLUGIN, viewer::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.link("gallery.viewer: the help", "docs:index")
            .link("gallery.viewer: the guide", "docs:guide")
            .link("gallery.viewer: an HTML page", "docs:about");
        ctx.story(
            Story::new(StoryId::from_static("viewer"), "In-app viewer", show)
                .description("The in-app viewer: the application's help pages, markdown, a safe subset of HTML and images in a panel, with back and forward, an address bar, find in page, zoom, remote images loaded only once allowed, and an error page; short texts with emphasis, code and links.")
                .group("Framework", 106),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    ui.label("Help pages open in the viewer, a panel of the end side bar that can be detached into its own window. Inside it: Ctrl+F to search, Ctrl+plus and Ctrl+minus to zoom, Alt+Left to go back.");
    section(ui, "Pages shipped with the application");
    ui.horizontal_wrapped(|ui| {
        links::link(ui, app, "The help", "docs:index");
        links::link(ui, app, "The guide", "docs:guide");
        links::link(
            ui,
            app,
            "The guide, at its search section",
            "docs:guide#find-in-page",
        );
        links::link(ui, app, "The gradient image", "docs:gradient.png");
        links::link(ui, app, "An HTML page", "docs:about");
    });
    section(ui, "Short texts");
    Inline::new("Tips, toasts and settings descriptions read a **small** part of markdown: *emphasis*, `code` and [links](docs:guide), or a bare https://github.com/quazardous/egui-pop.").show(ui, app);
    Inline::new("Delimiters without a pair stay as written: 2 * 3, snake_case_name.")
        .weak()
        .show(ui, app);
    section(ui, "What the viewer holds");
    if let Some(v) = app.service(&viewer::VIEWER) {
        let s = v.state();
        ui.monospace(format!(
            "{}  {}  zoom {}  back {}  forward {}",
            s["status"].as_str().unwrap_or_default(),
            s["target"].as_str().unwrap_or("-"),
            s["zoom"],
            s["back"].as_array().map_or(0, Vec::len),
            s["forward"].as_array().map_or(0, Vec::len),
        ));
    }
}
