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
use egui_pop_model::clock::ManualClock;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{BlockId, CommandId, PluginId};
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

// Tips declared in files, chosen by the rules and shown beside their
// target.

const RATE_FILE: &str = "---
command: shots.rate
surface: shotsOpen
target: shots.stars
---
# Rate from the keyboard

Press {key} to rate the shot, **0** to clear it; {key:shots.compare} compares.
";

const COMPARE_FILE: &str = "---
command: shots.compare
---
# Compare two shots

Select two shots, then press {key}.
";

/// Declares two commands, their tips from files, and draws the stars
/// a tip points at.
struct Shots;

impl Plugin for Shots {
    fn id(&self) -> PluginId {
        PluginId::from_static("shots")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![tips::TIPS_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(CommandId::from_static("shots.rate"), "Rate", "Shots").key("R"),
            |_| Ok(json!(null)),
        )?
        .command(
            CommandSpec::new(CommandId::from_static("shots.compare"), "Compare", "Shots"),
            |_| Ok(json!(null)),
        )?;
        let tips = ctx.app().service(&TIPS).expect("required");
        let errors = tips.add_files(
            "shots",
            &[
                ("shots.rate.md", RATE_FILE),
                ("shots.compare.md", COMPARE_FILE),
            ],
        );
        assert!(errors.is_empty(), "{errors:?}");
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("shots.stars"), 0, |ui, _| {
                ui.add_space(200.0);
                tips::mark(ui.button("Stars"), "shots.stars");
            }),
        );
        Ok(())
    }
}

fn shots(clock: &ManualClock) -> App {
    let app = App::builder()
        .plugin(TipsPlugin)
        .plugin(Shots)
        .build(&egui::Context::default())
        .unwrap();
    app.handle()
        .service(&TIPS)
        .unwrap()
        .set_clock(clock.shared());
    app
}

#[test]
fn a_tip_from_a_file_shows_the_keys_in_force() {
    let clock = ManualClock::new(1_000_000.0);
    let app = shots(&clock);
    let h = app.handle();
    let tips = h.service(&TIPS).unwrap();
    let rate = tips.get("shots.rate").unwrap();
    assert_eq!(rate.origin_name(), "shots");
    assert_eq!(rate.target.as_deref(), Some("shots.stars"));
    // Bound: the key as code; unbound: the command's title in bold.
    assert_eq!(
        rate.text_shown(&h.egui(), h),
        "Press `R` to rate the shot, **0** to clear it; **Compare** compares."
    );
}

#[test]
fn the_rules_skip_used_commands_and_tips_shown_today() {
    let clock = ManualClock::new(1_000_000.0);
    let mut app = shots(&clock);
    let tips = app.handle().service(&TIPS).unwrap();
    let cx = egui_pop_model::when::WhenContext::new().flag("shotsOpen", true);
    let elsewhere = egui_pop_model::when::WhenContext::new();
    let picked = |cx: &egui_pop_model::when::WhenContext| {
        tips.pick(app.handle(), cx, |_| true).map(|t| t.id)
    };
    // Its surface holds: the first registered.
    assert_eq!(picked(&cx).as_deref(), Some("shots.rate"));
    // Elsewhere, only the tip without a surface.
    assert_eq!(picked(&elsewhere).as_deref(), Some("shots.compare"));
    // Shown: not again today, but tomorrow.
    tips.show(app.handle(), "shots.rate").unwrap();
    tips.not_now(app.handle());
    assert_eq!(picked(&cx).as_deref(), Some("shots.compare"));
    clock.advance(tips::pick::AGAIN_AFTER + 1.0);
    // A person ran the command: its tip is never picked again.
    let mut h = harness_of(app);
    h.state_mut()
        .run(
            &CommandId::from_static("shots.rate"),
            json!(null),
            Origin::User,
        )
        .now()
        .unwrap()
        .unwrap();
    h.run();
    assert!(tips.used(h.state().handle()).contains("shots.rate"));
    assert_eq!(
        tips.pick(h.state().handle(), &cx, |_| true)
            .map(|t| t.id)
            .as_deref(),
        Some("shots.compare")
    );
    // An agent running one does not count.
    h.state_mut()
        .run(
            &CommandId::from_static("shots.compare"),
            json!(null),
            Origin::Agent("test".into()),
        )
        .now()
        .unwrap()
        .unwrap();
    h.run();
    assert!(!tips.used(h.state().handle()).contains("shots.compare"));
    app = h.into_state();
    // Got it is final; tips off is off.
    tips.show(app.handle(), "shots.compare").unwrap();
    tips.got_it(app.handle()).unwrap();
    assert!(tips.is_dismissed(app.handle(), "shots.compare"));
    tips.turn_off(app.handle()).unwrap();
    assert!(tips.show_next(app.handle()).is_none());
}

fn harness_of(app: App) -> Harness<'static, App> {
    Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app)
}

#[test]
fn the_card_sits_beside_its_target_and_escape_closes_it() {
    let clock = ManualClock::new(1_000_000.0);
    let mut h = harness_of(shots(&clock));
    h.run();
    let tips = h.state().handle().service(&TIPS).unwrap();
    tips.show(h.state().handle(), "shots.rate").unwrap();
    h.run();
    assert!(h.query_by_label("Did you know?").is_some());
    assert!(h.query_by_label("Rate from the keyboard").is_some());
    let shown = h
        .state_mut()
        .run(
            &tips::CMD_TIPS_CURRENT,
            json!(null),
            Origin::Agent("test".into()),
        )
        .now()
        .unwrap()
        .unwrap();
    assert_eq!(shown["id"], "shots.rate");
    let rect = |v: &serde_json::Value| {
        let a: Vec<f32> = v
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap() as f32)
            .collect();
        egui::Rect::from_min_max(egui::pos2(a[0], a[1]), egui::pos2(a[2], a[3]))
    };
    let (card, target) = (rect(&shown["card"]), rect(&shown["targetRect"]));
    assert!(!card.intersects(target), "{card:?} covers {target:?}");
    // Next: the other tip, the first one recorded as shown.
    h.get_by_label("Next tip").click();
    h.run();
    assert!(h.query_by_label("Compare two shots").is_some());
    // Escape: not now.
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.query_by_label("Did you know?").is_none());
    assert!(tips.current().is_none());
}

#[test]
fn tips_show_by_themselves_once_the_start_has_passed() {
    let clock = ManualClock::new(1_000_000.0);
    let mut h = harness_of(shots(&clock));
    let tips = h.state().handle().service(&TIPS).unwrap();
    tips.set_auto(true);
    h.run();
    assert!(tips.current().is_none(), "not at once");
    // Steps of a quarter of a second, past the first tip's moment.
    for _ in 0..((tips::FIRST_AFTER + 1.0) * 4.0) as usize {
        h.step();
    }
    h.run();
    assert_eq!(
        tips.current().map(|t| t.id).as_deref(),
        Some("shots.compare")
    );
}
