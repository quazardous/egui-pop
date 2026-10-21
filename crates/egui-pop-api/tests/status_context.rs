//! The status bar and the context stack together
//! (`docs/status-and-context.md`): indicators that follow the context.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::contexts::{self, ContextsPlugin, FrameSpec};
use egui_pop_api::strips::{Indicator, StripsPlugin};
use egui_pop_api::{App, AppBuilder, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::ids::{FrameId, PluginId};
use serde_json::{Value as Json, json};

const ALBUM: FrameId = FrameId::from_static("demo.album");
const PHOTO: FrameId = FrameId::from_static("demo.photo");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![contexts::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let album = FrameSpec::place(ALBUM, "Album");
        ctx.frame(album.clone())?
            .frame(FrameSpec::place(PHOTO, "Photo"))?;
        // Counts images added, shown only in the album.
        ctx.indicator(
            Indicator::new("demo.images", "No image", "image.")
                .during(&album)
                .reduce(|state, e| {
                    if e.name == "image.added" {
                        let n = state.count.unwrap_or(0) + 1;
                        state.count = Some(n);
                        state.text = "Images".into();
                    }
                }),
        )
        .indicator(Indicator::new("demo.always", "Everywhere", "always."))
        .indicator(Indicator::new("demo.broken", "Broken clause", "broken.").when("place =="));
        Ok(())
    }
}

fn builder() -> AppBuilder {
    App::builder()
        .plugins(DefaultPlugins)
        .plugin(ContextsPlugin::default())
        .plugin(StripsPlugin::status_only())
        .plugin(Demo)
}

fn harness(builder: AppBuilder, width: f32) -> Harness<'static, App> {
    let app = builder.build(&egui::Context::default()).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(width, 480.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

#[test]
fn an_indicator_with_a_clause_shows_only_where_it_holds_and_keeps_reducing() {
    let mut h = harness(builder(), 1000.0);
    let app = h.state().handle().clone();
    // At the root: the indicator without a clause only.
    assert!(h.query_by_label("Everywhere").is_some());
    assert!(h.query_by_label_contains("Image").is_none());
    // Hidden, it still follows the events.
    app.publish("host", None, "image.added", json!({}));
    app.publish("host", None, "image.added", json!({}));
    h.run();
    assert!(h.query_by_label_contains("Image").is_none());
    assert!(contexts::enter(&app, ALBUM, Json::Null).ok());
    h.run();
    assert!(h.query_by_label("Images (2)").is_some());
    // A place under the album is another place: hidden again.
    assert!(contexts::enter(&app, PHOTO, json!(1)).ok());
    h.run();
    assert!(h.query_by_label_contains("Image").is_none());
    // A clause that does not parse keeps its indicator hidden, reported.
    assert!(h.query_by_label("Broken clause").is_none());
    assert!(
        app.diagnostics()
            .iter()
            .any(|d| d.code == "indicator-when" && d.subject == "demo.broken")
    );
}
