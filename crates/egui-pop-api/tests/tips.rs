//! Tips: the tip of the day turns, a tip dismissed is remembered in the
//! settings (and from them, in the next run), hints show beside what
//! they are about, and tips follow the interface's language.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::tips::{self, CMD_TIPS_LIST, CMD_TIPS_RESET, TIPS, Tip, TipsPlugin};
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_core::lock;
use egui_pop_core::strings::{self, MapStrings};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{BlockId, PluginId};
use serde_json::json;

/// Registers three tips and draws the tip of the day and a hint.
struct Photos;

impl Plugin for Photos {
    fn id(&self) -> PluginId {
        PluginId::from_static("photos")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![tips::TIPS_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let tips = ctx.app().service(&TIPS).expect("required");
        tips.add(Tip::new(
            "photos.rate",
            "Rate quickly",
            "Press 1 to 5 to rate the selected photos.",
        ));
        tips.add(Tip::new(
            "photos.compare",
            "Compare two photos",
            "Select two photos and press C.",
        ));
        tips.add(
            Tip::new(
                "photos.export-here",
                "Export from here",
                "This button exports the selection with the last settings.",
            )
            .contextual(),
        );
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("photos.tips"), 0, |ui, app| {
                tips::tip_of_the_day(ui, app);
                let _ = ui.button("Export");
                tips::hint(ui, app, "photos.export-here");
            }),
        );
        Ok(())
    }
}

fn app() -> App {
    App::builder()
        .plugin(TipsPlugin)
        .plugin(Photos)
        .build(&egui::Context::default())
        .unwrap()
}

fn harness(app: App) -> Harness<'static, App> {
    Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app)
}

#[test]
fn the_tip_of_the_day_turns_and_a_dismissed_tip_is_remembered() {
    let mut h = harness(app());
    h.run();
    assert!(h.query_by_label("Tip of the day").is_some());
    assert!(h.query_by_label("Rate quickly").is_some());
    assert!(h.query_by_label("1 of 2").is_some());
    h.get_by_label("Next tip").click();
    h.run();
    assert!(h.query_by_label("Compare two photos").is_some());

    // Dismissed for good: written to the settings, out of the turn.
    h.get_by_label("Don't show again").click();
    h.run();
    let store = h.state().handle().store();
    let dismissed = lock(&store)
        .get("settings/tips.dismissed")
        .and_then(|e| e.value.as_text().map(str::to_string));
    assert_eq!(dismissed.as_deref(), Some("photos.compare"));
    assert!(h.query_by_label("Compare two photos").is_none());
    assert!(h.query_by_label("Rate quickly").is_some());
    assert!(h.query_by_label("1 of 1").is_some());
    // Not an undoable change of the settings.
    assert!(!h.state().handle().can_undo());

    // The next run reads the same settings (as persistence restores
    // them): the tip stays dismissed.
    let doc = lock(&store).export("settings/");
    let next = app();
    let _ = lock(&next.handle().store()).import(&doc, &Origin::User);
    let tips = next.handle().service(&TIPS).unwrap();
    assert!(tips.is_dismissed(next.handle(), "photos.compare"));
    assert_eq!(
        tips.of_the_day(next.handle()).map(|t| t.id),
        Some("photos.rate".into())
    );
    let mut next = next;
    let listed = next
        .run(&CMD_TIPS_LIST, json!(null), Origin::Agent("test".into()))
        .now()
        .unwrap()
        .unwrap();
    let compare = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "photos.compare")
        .unwrap()
        .clone();
    assert_eq!(compare["dismissed"], true);
}

#[test]
fn a_hint_is_dismissed_with_got_it_and_reset_shows_it_again() {
    let mut h = harness(app());
    h.run();
    assert!(h.query_by_label("Export from here").is_some());
    h.get_by_label("Got it: do not show Export from here again")
        .click();
    h.run();
    assert!(h.query_by_label("Export from here").is_none());
    let out = h
        .state_mut()
        .run(&CMD_TIPS_RESET, json!(null), Origin::User)
        .now()
        .unwrap();
    assert!(out.is_ok());
    h.run();
    assert!(h.query_by_label("Export from here").is_some());
}

#[test]
fn closing_hides_the_tip_of_the_day_for_the_session_and_the_setting_turns_tips_off() {
    let mut h = harness(app());
    h.run();
    h.get_by_label("Close").click();
    h.run();
    assert!(h.query_by_label("Tip of the day").is_none());
    assert!(h.query_by_label("Export from here").is_some());
    let store = h.state().handle().store();
    let _ = lock(&store).set(egui_pop_model::values::Set::new(
        "settings/tips.show",
        egui_pop_model::values::Change::To(egui_pop_model::values::Value::Bool(false)),
        Origin::User,
    ));
    h.run();
    assert!(h.query_by_label("Export from here").is_none());
}

#[test]
fn tips_follow_the_interface_language() {
    let mut h = harness(app());
    strings::install(
        &h.ctx,
        std::sync::Arc::new(
            MapStrings::new("fr")
                .with("egui-pop-tips-of-the-day", "Astuce du jour")
                .with("tip-photos-rate.title", "Noter vite")
                .with(
                    "tip-photos-rate.text",
                    "Appuyez sur 1 à 5 pour noter les photos choisies.",
                ),
        ),
    );
    h.run();
    assert!(h.query_by_label("Astuce du jour").is_some());
    assert!(h.query_by_label("Noter vite").is_some());
    // What the catalogue lacks falls back to English.
    assert!(h.query_by_label("Next tip").is_some());
}
