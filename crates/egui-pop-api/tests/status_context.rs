//! The status bar and the context stack together
//! (`docs/status-and-context.md`): indicators that follow the context,
//! the breadcrumb and the mode chips at the start of the bar, the
//! user's choices kept per place.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::contexts::{self, ContextsPlugin, FrameSpec, Verdict};
use egui_pop_api::host::Reply;
use egui_pop_api::strips::{self, CMD_HIDE_ITEM, Indicator, StripsPlugin};
use egui_pop_api::{App, AppBuilder, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{FrameId, GroupId, PluginId};
use serde_json::{Value as Json, json};

const ALBUM: FrameId = FrameId::from_static("demo.album");
const PHOTO: FrameId = FrameId::from_static("demo.photo");
const CULLING: FrameId = FrameId::from_static("demo.culling");
const COMPARE: FrameId = FrameId::from_static("demo.compare");
const PLUMBING: FrameId = FrameId::from_static("demo.plumbing");
const LOCKED: FrameId = FrameId::from_static("demo.locked");

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
            .frame(FrameSpec::place(PHOTO, "Photo"))?
            .frame(
                FrameSpec::mode(CULLING, GroupId::from_static("orientation"), "Culling").visible(),
            )?
            .frame(
                FrameSpec::mode(COMPARE, GroupId::from_static("compare"), "Compare")
                    .visible()
                    .tone("info"),
            )?
            // Plumbing: never in the bar.
            .frame(FrameSpec::mode(
                PLUMBING,
                GroupId::from_static("plumbing"),
                "Plumbing",
            ))?
            .frame(
                FrameSpec::mode(LOCKED, GroupId::from_static("batch"), "Batch")
                    .visible()
                    .can_leave(|_| Verdict::Refuse("the batch runs".into())),
            )?;
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

fn key(h: &Harness<'_, App>, k: &str) -> String {
    h.state().handle().context().get(k).to_string()
}

#[test]
fn the_status_bar_starts_with_the_breadcrumb_and_the_chips() {
    let mut h = harness(builder(), 1000.0);
    let app = h.state().handle().clone();
    assert!(h.query_by_label("You are in Home").is_some());
    assert!(contexts::enter(&app, ALBUM, json!("Holidays")).ok());
    assert!(contexts::enter(&app, PHOTO, json!(3)).ok());
    h.run();
    assert!(h.query_by_label("Go to Home").is_some());
    assert!(h.query_by_label("Go to Album Holidays").is_some());
    assert!(h.query_by_label("You are in Photo 3").is_some());
    // The current crumb does nothing.
    h.get_by_label("You are in Photo 3").click();
    h.run();
    assert_eq!(key(&h, "place"), "demo.photo");
    // A crumb goes back to its place.
    h.get_by_label("Go to Album Holidays").click();
    h.run();
    assert_eq!(key(&h, "place"), "demo.album");
    // A chip per mode of a visible group; plumbing stays out.
    assert!(contexts::set_mode(&app, CULLING).ok());
    assert!(contexts::set_mode(&app, PLUMBING).ok());
    assert!(contexts::push_mode(&app, COMPARE).ok());
    h.run();
    assert!(h.query_by_label("Leave Culling").is_some());
    assert!(h.query_by_label_contains("Plumbing").is_none());
    // The pushed compare: a back mark, a click goes back.
    assert!(h.query_by_label("Back from Compare").is_some());
    h.get_by_label("Back from Compare").click();
    h.run();
    assert_eq!(key(&h, "mode.compare"), "");
    assert_eq!(key(&h, "mode.orientation"), "demo.culling");
    // A set mode's chip turns it off.
    h.get_by_label("Leave Culling").click();
    h.run();
    assert_eq!(key(&h, "mode.orientation"), "");
    assert_eq!(key(&h, "mode.plumbing"), "demo.plumbing");
    // A guard refuses: the chip stays and says why.
    assert!(contexts::set_mode(&app, LOCKED).ok());
    h.run();
    let chip = contexts::chips(&app)
        .into_iter()
        .find(|c| c.frame == LOCKED)
        .unwrap();
    assert_eq!(
        contexts::why_not(&app, chip.leave()).as_deref(),
        Some("the batch runs")
    );
    h.get_by_label("Leave Batch").click();
    h.run();
    assert_eq!(key(&h, "mode.batch"), "demo.locked");
    assert!(h.query_by_label("Leave Batch").is_some());
}

#[test]
fn an_application_keeps_the_breadcrumb_out_of_the_bar() {
    let h = harness(
        App::builder()
            .plugins(DefaultPlugins)
            .plugin(ContextsPlugin::default())
            .plugin(StripsPlugin::status_only().breadcrumb(false))
            .plugin(Demo),
        1000.0,
    );
    assert!(h.query_by_label("You are in Home").is_none());
    assert!(h.query_by_label("Everywhere").is_some());
}

#[test]
fn on_a_narrow_bar_the_crumbs_collapse_then_indicators_give_way_never_chips() {
    let mut h = harness(builder(), 300.0);
    let app = h.state().handle().clone();
    assert!(contexts::enter(&app, ALBUM, json!("Holidays")).ok());
    assert!(contexts::enter(&app, PHOTO, json!(3)).ok());
    assert!(contexts::set_mode(&app, CULLING).ok());
    h.run();
    // The middle crumb is in `…`; the chip stays.
    assert!(h.query_by_label("Go to Album Holidays").is_none());
    assert!(h.query_by_label("You are in Photo 3").is_some());
    assert!(h.query_by_label("Leave Culling").is_some());
    h.get_by_label("Places above").click();
    h.run();
    h.get_by_label("Go to Album Holidays").click();
    h.run();
    assert_eq!(key(&h, "place"), "demo.album");
    // Narrower still: the indicator gives way, the chip stays.
    h.set_size(egui::vec2(170.0, 480.0));
    assert!(contexts::enter(&app, PHOTO, json!(3)).ok());
    h.run();
    assert!(h.query_by_label("Everywhere").is_none());
    assert!(h.query_by_label("Leave Culling").is_some());
}

#[test]
fn the_bars_choices_are_kept_per_place_falling_back_to_the_roots() {
    let dir = std::env::temp_dir().join(format!("egui-pop-status-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    {
        let mut h = harness(builder().persist(&dir), 1000.0);
        let app = h.state().handle().clone();
        // Hidden at the root, by its context menu.
        h.get_by_label("Everywhere").click_secondary();
        h.run();
        h.get_by_label("Hide Everywhere").click();
        h.run();
        assert!(h.query_by_label("Everywhere").is_none());
        assert_eq!(strips::choices(&app).hidden, vec!["demo.always"]);
        // The album has no choices of its own: the root's.
        assert!(contexts::enter(&app, ALBUM, Json::Null).ok());
        h.run();
        assert!(h.query_by_label("Everywhere").is_none());
        // Shown again in the album (an agent's command), ordered there.
        let reply = h.state_mut().run(
            &CMD_HIDE_ITEM,
            json!({ "id": "demo.always", "hidden": false }),
            Origin::Agent("test".into()),
        );
        assert!(matches!(reply, Reply::Done(Ok(_))));
        h.run();
        assert!(h.query_by_label("Everywhere").is_some());
        strips::move_item(&app, "demo.always", true);
        h.run();
        assert_eq!(strips::choices(&app).order, ["demo.always", "demo.images"]);
        // The root kept its own.
        assert!(contexts::leave(&app).ok());
        h.run();
        assert!(h.query_by_label("Everywhere").is_none());
        h.state().save().unwrap();
    }
    // After a restart, each place as it was.
    let mut h = harness(builder().persist(&dir), 1000.0);
    let app = h.state().handle().clone();
    assert!(h.query_by_label("Everywhere").is_none());
    assert!(contexts::enter(&app, ALBUM, Json::Null).ok());
    h.run();
    assert!(h.query_by_label("Everywhere").is_some());
    assert_eq!(strips::choices(&app).order, ["demo.always", "demo.images"]);
    let _ = std::fs::remove_dir_all(&dir);
}
