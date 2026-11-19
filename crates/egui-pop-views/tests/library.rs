//! The library's widgets driven as a user would (kittest), and their
//! steps and commands through an application.

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::steps::Before;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::PluginId;
use egui_pop_views::library::filter::{SearchField, SearchState};
use egui_pop_views::library::grid::{
    Badge, LibraryItems, LibraryState, Signal, Stack, Thumb, ThumbGrid, ThumbSource,
};
use egui_pop_views::library::marks::{ColorLabel, CullKeymap, Flag, Marks};
use egui_pop_views::library::plugin::{
    CMD_RATE, CMD_SELECT, INBOX, LIBRARIES, LIBRARY_FLAG, LIBRARY_RATE, LibraryPlugin,
};
use egui_pop_views::library::tags::{TagInput, TagInputState};
use serde_json::json;

/// `n` images; images 3, 4 and 5 form a stack topped by 3.
struct Shoot {
    n: u32,
    stacked: bool,
}

impl LibraryItems<u32> for Shoot {
    fn len(&self) -> usize {
        self.n as usize
    }
    fn key(&self, i: usize) -> u32 {
        i as u32
    }
    fn label(&self, i: usize) -> String {
        format!("IMG_{i:05}")
    }
    fn marks(&self, i: usize) -> Marks {
        Marks {
            rating: (i % 6) as u8,
            ..Marks::default()
        }
    }
    fn stack(&self, i: usize) -> Option<Stack<u32>> {
        (self.stacked && (3..=5).contains(&i)).then_some(Stack { top: 3, count: 3 })
    }
    fn badges(&self, i: usize) -> Vec<Badge> {
        if i == 1 {
            vec![
                Badge::new("RAW"),
                Badge::new("HDR").tone(egui_pop_core::kit::Tone::Info),
            ]
        } else {
            Vec::new()
        }
    }
}

/// Thumbnails never ready: placeholders.
struct Thumbs;

impl ThumbSource<u32> for Thumbs {
    fn thumb(&self, key: &u32, _: u32) -> Thumb {
        Thumb::Pending {
            average: key
                .is_multiple_of(2)
                .then_some(egui::Color32::from_gray(80)),
        }
    }
}

/// Caps ratings at 4 and refuses rejects.
struct Guard;

impl Plugin for Guard {
    fn id(&self) -> PluginId {
        PluginId::from_static("guard")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(LIBRARY_RATE, |_, args| {
            if args["rating"].as_u64() > Some(4) {
                let mut a = args.clone();
                a["rating"] = json!(4);
                Before::Redirect(a)
            } else {
                Before::Go
            }
        });
        ctx.before(LIBRARY_FLAG, |_, args| {
            if args["flag"] == "reject" {
                Before::Veto("no rejects here".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

struct Lib {
    app: Option<App>,
    with_app: bool,
    state: LibraryState<u32>,
    shoot: Shoot,
    keymap: CullKeymap,
    signals: Vec<Signal<u32>>,
    built: usize,
    columns: usize,
    drag_out: bool,
}

fn harness(n: u32, stacked: bool, with_app: bool, keymap: CullKeymap) -> Harness<'static, Lib> {
    let lib = Lib {
        app: None,
        with_app,
        state: LibraryState::new("shoot"),
        shoot: Shoot { n, stacked },
        keymap,
        signals: Vec::new(),
        built: 0,
        columns: 0,
        drag_out: false,
    };
    Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui_state(
            |ui, l: &mut Lib| {
                if l.with_app && l.app.is_none() {
                    l.app = Some(
                        App::builder()
                            .plugin(LibraryPlugin)
                            .plugin(Guard)
                            .build(ui.ctx())
                            .unwrap(),
                    );
                }
                let handle = l.app.as_ref().map(|a| a.handle().clone());
                let mut grid = ThumbGrid::new("grid")
                    .keymap(l.keymap.clone())
                    .drag_out(l.drag_out);
                if let Some(h) = &handle {
                    grid = grid.app(h);
                }
                let out = grid.show(ui, &mut l.state, &l.shoot, &Thumbs);
                l.built = out.built;
                l.columns = out.columns;
                l.signals.extend(out.signals);
            },
            lib,
        )
}

#[test]
fn a_grid_keeps_the_wheel_at_its_end_in_a_panel_that_scrolls() {
    struct InPanel {
        state: LibraryState<u32>,
        shoot: Shoot,
        panel: f32,
    }
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui_state(
            |ui, p: &mut InPanel| {
                let out = egui::ScrollArea::vertical()
                    .id_salt("panel")
                    .show(ui, |ui| {
                        ThumbGrid::new("grid").height(200.0).show(
                            ui,
                            &mut p.state,
                            &p.shoot,
                            &Thumbs,
                        );
                        ui.allocate_space(egui::vec2(10.0, 2000.0));
                    });
                p.panel = out.state.clamped_offset().y;
            },
            InPanel {
                state: LibraryState::new("shoot"),
                shoot: Shoot {
                    n: 60,
                    stacked: false,
                },
                panel: 0.0,
            },
        );
    h.run();
    let event = |delta, phase| egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, delta),
        modifiers: Modifiers::NONE,
        phase,
        source: egui::MouseWheelSource::Wheel,
    };
    h.hover_at(egui::pos2(100.0, 100.0));
    h.run();
    // In one gesture, as a touchpad's, so that egui does not smooth it.
    for _ in 0..12 {
        h.event(event(0.0, egui::TouchPhase::Start));
        h.event(event(-8.0, egui::TouchPhase::Move));
        h.run();
        h.event(event(0.0, egui::TouchPhase::End));
        h.run();
    }
    assert_eq!(h.state().panel, 0.0);
}

fn cursor(h: &Harness<'_, Lib>) -> Option<u32> {
    h.state().state.selection().cursor().copied()
}

/// A cell by its image's number (the label leaks: kittest borrows it
/// for as long as the node).
fn cell<'h>(h: &'h Harness<'_, Lib>, i: u32) -> egui_kittest::Node<'h> {
    let label: &'static str = Box::leak(format!("IMG_{i:05}").into_boxed_str());
    h.get_by_label(label)
}

#[test]
fn the_keyboard_moves_by_cells_rows_pages_and_ends() {
    let mut h = harness(100, false, false, CullKeymap::lightroom());
    h.run();
    let cols = h.state().columns as u32;
    assert!(cols > 1, "{cols} columns");
    cell(&h, 0).click();
    h.run();
    assert_eq!(cursor(&h), Some(0));
    h.key_press(Key::ArrowRight);
    h.run();
    assert_eq!(cursor(&h), Some(1));
    h.key_press(Key::ArrowDown);
    h.run();
    assert_eq!(cursor(&h), Some(1 + cols));
    h.key_press(Key::ArrowUp);
    h.run();
    assert_eq!(cursor(&h), Some(1));
    h.key_press(Key::PageDown);
    h.run();
    let c = cursor(&h).unwrap();
    assert!(
        c > 1 && (c - 1).is_multiple_of(cols),
        "a page down keeps the column: {c}"
    );
    h.key_press(Key::End);
    h.run();
    assert_eq!(cursor(&h), Some(99));
    assert!(
        h.query_by_label("IMG_00099").is_some(),
        "scrolled to the end"
    );
    h.key_press(Key::Home);
    h.run();
    assert_eq!(cursor(&h), Some(0));
    h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowRight);
    h.run();
    assert_eq!(h.state().state.selection().keys(), &[0, 1]);
    h.key_press(Key::Enter);
    h.run();
    assert!(h.state().signals.contains(&Signal::Activate { key: 1 }));
}

#[test]
fn clicks_shift_ctrl_and_a_rubber_band_select() {
    let mut h = harness(100, false, false, CullKeymap::lightroom());
    h.run();
    cell(&h, 2).click();
    h.run();
    cell(&h, 5).click_modifiers(Modifiers::SHIFT);
    h.run();
    assert_eq!(
        h.state().state.selection().keys(),
        &[2, 3, 4, 5],
        "{:?}",
        h.state().signals
    );
    cell(&h, 3).click_modifiers(Modifiers::COMMAND);
    h.run();
    assert_eq!(h.state().state.selection().keys(), &[2, 4, 5]);
    // A band from the first cell to the second: those two.
    let a = cell(&h, 0).rect().center();
    let b = cell(&h, 1).rect().center();
    h.drag_at(a);
    h.run();
    for i in 1..=4 {
        h.hover_at(a + (b - a) * (i as f32 / 4.0));
        h.run();
    }
    h.drop_at(b);
    h.run();
    assert_eq!(h.state().state.selection().keys(), &[0, 1]);
    assert_eq!(cursor(&h), Some(1));
}

/// Drags from `a` to `b` in a few moves, as a hand does.
fn drag(h: &mut Harness<'_, Lib>, a: egui::Pos2, b: egui::Pos2) {
    h.drag_at(a);
    h.run();
    for i in 1..=4 {
        h.hover_at(a + (b - a) * (i as f32 / 4.0));
        h.run();
    }
    h.drop_at(b);
    h.run();
}

#[test]
fn a_drag_out_carries_the_selection_or_the_image_under_the_pointer() {
    let mut h = harness(100, false, false, CullKeymap::lightroom());
    h.state_mut().drag_out = true;
    h.run();
    cell(&h, 2).click();
    h.run();
    cell(&h, 4).click_modifiers(Modifiers::SHIFT);
    h.run();
    assert_eq!(h.state().state.selection().keys(), &[2, 3, 4]);
    // From a selected cell: the selection goes, and stays selected.
    h.state_mut().signals.clear();
    let (a, b) = (cell(&h, 3).rect().center(), cell(&h, 9).rect().center());
    drag(&mut h, a, b);
    assert!(
        h.state().signals.contains(&Signal::DragOut {
            keys: vec![2, 3, 4]
        }),
        "{:?}",
        h.state().signals
    );
    assert_eq!(h.state().state.selection().keys(), &[2, 3, 4], "no band");
    // From a cell not selected: that image alone, the selection kept.
    h.state_mut().signals.clear();
    let (a, b) = (cell(&h, 7).rect().center(), cell(&h, 0).rect().center());
    drag(&mut h, a, b);
    assert!(
        h.state()
            .signals
            .contains(&Signal::DragOut { keys: vec![7] }),
        "{:?}",
        h.state().signals
    );
    assert_eq!(h.state().state.selection().keys(), &[2, 3, 4]);
}

#[test]
fn rating_keys_mark_the_selection_and_auto_advance_moves_on() {
    let mut h = harness(
        100,
        false,
        false,
        CullKeymap::lightroom().auto_advance(true),
    );
    h.run();
    cell(&h, 0).click();
    h.run();
    h.state_mut().signals.clear();
    h.key_press(Key::Num3);
    h.run();
    assert_eq!(
        h.state().signals[0],
        Signal::Rate {
            keys: vec![0],
            rating: 3
        }
    );
    assert_eq!(cursor(&h), Some(1), "advanced");
    h.key_press(Key::P);
    h.run();
    assert!(h.state().signals.contains(&Signal::Flag {
        keys: vec![1],
        flag: Flag::Pick
    }));
    assert_eq!(cursor(&h), Some(2));
    h.key_press(Key::Num6);
    h.run();
    assert!(h.state().signals.contains(&Signal::Label {
        keys: vec![2],
        label: ColorLabel::Red,
        on: true
    }));
    // Without auto-advance the cursor stays.
    h.state_mut().keymap = CullKeymap::lightroom();
    h.key_press(Key::Num0);
    h.run();
    assert_eq!(cursor(&h), Some(3));
    // The stars of a cell rate it.
    h.state_mut().signals.clear();
    h.query_all_by_label("Set 2 of 5")
        .next()
        .expect("stars on the cells")
        .click();
    h.run();
    assert!(
        h.state()
            .signals
            .iter()
            .any(|s| matches!(s, Signal::Rate { rating: 2, .. }))
    );
    // The star of a cell's rating clears it.
    h.state_mut().signals.clear();
    h.query_all_by_label("Clear rating")
        .next()
        .expect("a rated cell")
        .click();
    h.run();
    assert!(
        h.state()
            .signals
            .iter()
            .any(|s| matches!(s, Signal::Rate { rating: 0, .. }))
    );
}

#[test]
fn hooks_redirect_and_veto_marks_and_commands_reach_the_grid() {
    let mut h = harness(100, false, true, CullKeymap::lightroom());
    h.run();
    cell(&h, 7).click();
    h.run();
    h.state_mut().signals.clear();
    h.key_press(Key::Num5);
    h.run();
    assert_eq!(
        h.state().signals[0],
        Signal::Rate {
            keys: vec![7],
            rating: 4
        },
        "capped by the hook"
    );
    h.key_press(Key::X);
    h.run();
    assert!(
        h.state()
            .signals
            .iter()
            .any(|s| matches!(s, Signal::Vetoed { reason, .. } if reason == "no rejects here"))
    );
    // An agent's commands.
    h.state_mut().signals.clear();
    let app = h.state_mut().app.as_mut().unwrap();
    app.run(
        &CMD_SELECT,
        json!({ "keys": [10, 11] }),
        Origin::Agent("t".into()),
    )
    .now()
    .unwrap()
    .unwrap();
    app.run(&CMD_RATE, json!({ "rating": 2 }), Origin::Agent("t".into()))
        .now()
        .unwrap()
        .unwrap();
    h.run();
    assert_eq!(h.state().state.selection().keys(), &[10, 11]);
    assert!(h.state().signals.contains(&Signal::Rate {
        keys: vec![10, 11],
        rating: 2
    }));
    let traces = h.state().app.as_ref().unwrap().handle().steps();
    let declared: Vec<String> = traces.iter().map(|s| s.spec.id.to_string()).collect();
    for s in [
        "library.select",
        "library.rate",
        "library.label",
        "library.flag",
        "library.tag",
        "library.stack",
        "library.filter",
    ] {
        assert!(declared.iter().any(|d| d == s), "{s}");
    }
    assert!(
        h.state()
            .app
            .as_ref()
            .unwrap()
            .handle()
            .diagnostics()
            .is_empty()
    );
}

#[test]
fn stacks_show_their_top_until_expanded() {
    let mut h = harness(20, true, false, CullKeymap::lightroom());
    h.run();
    assert!(h.query_by_label("IMG_00003").is_some());
    assert!(h.query_by_label("IMG_00004").is_none(), "collapsed");
    assert_eq!(h.state().state.shown(), 18);
    h.get_by_label("Expand the stack of 3").click();
    h.run();
    h.run();
    assert!(h.query_by_label("IMG_00004").is_some());
    assert!(h.state().signals.contains(&Signal::Stack {
        top: 3,
        expanded: true
    }));
    // S on the top image collapses it again.
    cell(&h, 3).click();
    h.run();
    h.key_press(Key::S);
    h.run();
    h.run();
    assert!(h.query_by_label("IMG_00005").is_none());
    assert!(h.query_by_label("Expand the stack of 3").is_some());
}

#[test]
fn a_hundred_thousand_images_build_a_screenful() {
    let mut h = harness(100_000, false, false, CullKeymap::lightroom());
    h.run();
    let built = h.state().built;
    assert!(built > 10 && built < 200, "{built} cells built");
    let named = h.query_all_by_label_contains("IMG_").count();
    assert_eq!(named, built, "only the cells built are in the tree");
    cell(&h, 0).click();
    h.run();
    h.key_press(Key::End);
    h.run();
    assert!(h.query_by_label("IMG_99999").is_some());
    assert!(h.state().built < 200);
    let start = std::time::Instant::now();
    for _ in 0..10 {
        h.step();
    }
    let per = start.elapsed() / 10;
    assert!(
        per < std::time::Duration::from_millis(100),
        "{per:?} a pass"
    );
}

#[test]
fn the_search_field_turns_criteria_into_removable_chips() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(700.0, 200.0))
        .build_ui_state(
            |ui, (state, last): &mut (SearchState, Vec<serde_json::Value>)| {
                if let Some(f) = SearchField::new("shoot").show(ui, state).changed {
                    last.push(serde_json::to_value(f).unwrap());
                }
            },
            (SearchState::new(), Vec::new()),
        );
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Search").click();
    h.run();
    h.event(egui::Event::Text("beach rating>=3 label:red".into()));
    h.run();
    assert_eq!(
        h.state().1.last().unwrap(),
        &json!({ "text": "beach", "min_rating": 3, "labels": ["red"] }),
        "the filter follows the typing"
    );
    h.key_press(Key::Enter);
    h.run();
    assert!(h.query_by_label("Rating ≥ 3").is_some());
    assert!(h.query_by_label("Label: Red").is_some());
    assert_eq!(h.state().0.text(), "beach");
    h.get_by_label("Remove Rating ≥ 3").click();
    h.run();
    assert_eq!(
        h.state().1.last().unwrap(),
        &json!({ "text": "beach", "labels": ["red"] })
    );
    h.get_by_label("Clear the search").click();
    h.run();
    assert_eq!(h.state().1.last().unwrap(), &json!({}));
}

#[test]
fn the_tag_input_completes_adds_and_removes() {
    let known: Vec<String> = ["beach", "bench", "sea", "sunset"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut h = Harness::builder()
        .with_size(egui::vec2(600.0, 300.0))
        .build_ui_state(
            move |ui, (state, tags): &mut (TagInputState, Vec<String>)| {
                let out = TagInput::new("tags").source(&known).show(ui, state, tags);
                tags.retain(|t| !out.removed.contains(t));
                tags.extend(out.added);
            },
            (TagInputState::default(), vec!["sea".to_string()]),
        );
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Add a tag")
        .click();
    h.run();
    h.event(egui::Event::Text("sun".into()));
    h.run();
    assert!(h.query_by_label("sunset").is_some(), "a completion");
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(h.state().1, ["sea", "sunset"]);
    // A comma ends a new tag.
    h.event(egui::Event::Text("dunes,".into()));
    h.run();
    assert_eq!(h.state().1, ["sea", "sunset", "dunes"]);
    h.get_by_label("Remove sea").click();
    h.run();
    assert_eq!(h.state().1, ["sunset", "dunes"]);
}

#[test]
fn a_library_not_drawn_keeps_its_latest_requests_and_counts_the_others() {
    let mut h = harness(40, false, true, CullKeymap::lightroom());
    h.run();
    // Its host stops drawing it; an agent keeps rating.
    let app = h.state_mut().app.as_mut().unwrap();
    for i in 0..INBOX + 36 {
        app.run(
            &CMD_RATE,
            json!({ "library": "shoot", "keys": [1], "rating": i % 6 }),
            Origin::Agent("t".into()),
        )
        .now()
        .unwrap()
        .unwrap();
    }
    let reg = app.handle().service(&LIBRARIES).unwrap();
    let shoot = &reg.describe()["libraries"]["shoot"];
    assert_eq!(shoot["waiting"], INBOX);
    assert_eq!(shoot["dropped"], 36);
    // Drawn again, it takes in what waited.
    h.run();
    let shoot = &reg.describe()["libraries"]["shoot"];
    assert_eq!(shoot["waiting"], 0);
    assert_eq!(shoot["dropped"], 36);
}

#[test]
fn the_registry_says_a_librarys_selection_and_cursor() {
    let mut h = harness(40, false, true, CullKeymap::lightroom());
    h.run();
    let app = h.state_mut().app.as_mut().unwrap();
    let reg = app.handle().service(&LIBRARIES).unwrap();
    assert_eq!(reg.cursor("shoot"), None);
    app.run(
        &CMD_SELECT,
        json!({ "library": "shoot", "keys": [3, 4], "cursor": 4 }),
        Origin::Agent("t".into()),
    )
    .now()
    .unwrap()
    .unwrap();
    h.run();
    assert_eq!(reg.selection("shoot"), Some(vec![json!(3), json!(4)]));
    assert_eq!(reg.cursor("shoot"), Some(json!(4)));
    assert_eq!(reg.cursor("elsewhere"), None);
}

/// A widget's description: its tooltip's text.
fn described<T>(h: &Harness<'_, T>, role: Role, label: &str) -> Option<String> {
    use egui_kittest::kittest::NodeT as _;
    h.get_by_role_and_label(role, label)
        .accesskit_node()
        .description()
}

/// The text painted with exactly `text`, if any (an open tooltip's).
fn painted<T>(h: &Harness<'_, T>, text: &str) -> bool {
    h.output().shapes.iter().any(|c| match &c.shape {
        egui::Shape::Text(s) => s.galley.job.text == text,
        _ => false,
    })
}

fn hover<T>(h: &mut Harness<'_, T>, at: egui::Pos2) {
    h.event(egui::Event::PointerMoved(at));
    for _ in 0..4 {
        h.run();
    }
}

#[test]
fn mark_buttons_say_what_they_do_and_their_keys_in_tooltips() {
    use egui_pop_views::library::marks::{ColorLabels, FlagToggle, Labels};
    let keys = CullKeymap::lightroom();
    let mut h = Harness::builder()
        .with_size(egui::vec2(400.0, 200.0))
        .build_ui(move |ui| {
            let _ = ColorLabels::new(Labels::NONE.with(ColorLabel::Red, true))
                .keys(&keys)
                .tooltip(ColorLabel::Purple, "Client's choice")
                .show(ui);
            let _ = FlagToggle::new(Flag::Pick).keys(&keys).show(ui);
            // Without keys: the name alone.
            ui.push_id("bare", |ui| {
                let _ = FlagToggle::new(Flag::None).show(ui);
            });
        });
    h.ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
    h.run();
    let tip = |h: &Harness<'_>, label: &str| described(h, Role::CheckBox, label);
    assert_eq!(tip(&h, "Red label").as_deref(), Some("Red label (6)"));
    assert_eq!(tip(&h, "Blue label").as_deref(), Some("Blue label (9)"));
    // Purple has no key by default, and its own tooltip here.
    assert_eq!(tip(&h, "Purple label").as_deref(), Some("Client's choice"));
    let flags = h.get_all_by_role_and_label(Role::CheckBox, "Pick").count();
    assert_eq!(flags, 2, "two flag toggles");
    let mut picks = h
        .get_all_by_role_and_label(Role::CheckBox, "Pick")
        .map(|n| {
            use egui_kittest::kittest::NodeT as _;
            n.accesskit_node().description()
        })
        .collect::<Vec<_>>();
    picks.sort();
    assert_eq!(picks, [Some("Pick".into()), Some("Pick (P)".into())]);
    let red = h.get_by_role_and_label(Role::CheckBox, "Red label").rect();
    assert!(!painted(&h, "Red label (6)"));
    hover(&mut h, red.center());
    assert!(painted(&h, "Red label (6)"), "the tooltip shows on hover");
}

#[test]
fn the_search_field_s_buttons_have_names_and_tooltips() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(700.0, 200.0))
        .build_ui_state(
            |ui, state: &mut SearchState| {
                let _ = SearchField::new("shoot").show(ui, state);
            },
            SearchState::new(),
        );
    h.ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Search").click();
    h.run();
    h.event(egui::Event::Text("rating>=3".into()));
    h.run();
    h.key_press(Key::Enter);
    h.run();
    for name in ["Add a filter", "Clear the search", "Remove Rating ≥ 3"] {
        assert_eq!(
            described(&h, Role::Button, name).as_deref(),
            Some(name),
            "{name}"
        );
    }
    let clear = h
        .get_by_role_and_label(Role::Button, "Clear the search")
        .rect();
    hover(&mut h, clear.center());
    assert!(
        painted(&h, "Clear the search"),
        "the tooltip shows on hover"
    );
}

#[test]
fn colour_labels_are_swatches_unless_the_app_or_theme_asks_for_shapes() {
    use egui_pop_core::theme::{self, ThemeSpec, Themes};
    use egui_pop_views::library::LabelStyle;
    let ctx = egui::Context::default();
    assert_eq!(LabelStyle::of(&ctx), LabelStyle::Swatch);
    LabelStyle::set(&ctx, Some(LabelStyle::Shape));
    assert_eq!(LabelStyle::of(&ctx), LabelStyle::Shape);
    LabelStyle::set(&ctx, None);
    assert_eq!(LabelStyle::of(&ctx), LabelStyle::Swatch);
    let mut themes = Themes::default();
    themes
        .add_text(
            "shapes",
            "$extends: \"pop-dark\"\nlibrary:\n  label-style: {$type: \"string\", $value: \"shape\"}\n",
        )
        .expect("the test theme reads");
    theme::install_with(&ctx, &themes, &ThemeSpec::new("shapes"));
    assert_eq!(LabelStyle::of(&ctx), LabelStyle::Shape);
    // The application's choice wins over the theme's.
    LabelStyle::set(&ctx, Some(LabelStyle::Swatch));
    assert_eq!(LabelStyle::of(&ctx), LabelStyle::Swatch);
}

/// The commands that ran, by id.
type Ran = std::rc::Rc<std::cell::RefCell<Vec<String>>>;

/// A global Right and Home, and a Left bound in the window `import`.
struct Globals(Ran);

impl Plugin for Globals {
    fn id(&self) -> PluginId {
        PluginId::from_static("globals")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        use egui_pop_model::commands::CommandSpec;
        use egui_pop_model::ids::CommandId;
        use egui_pop_model::keymap::BindOptions;
        for (id, keys, options) in [
            ("demo.next", "Right", BindOptions::global()),
            ("demo.first", "Home", BindOptions::global()),
            ("demo.import_left", "Left", BindOptions::window("import")),
        ] {
            let ran = self.0.clone();
            ctx.command(
                CommandSpec::new(CommandId::from_static(id), id, "Demo").key_with(keys, options),
                move |_| {
                    ran.borrow_mut().push(id.to_string());
                    Ok(serde_json::Value::Null)
                },
            )?;
        }
        Ok(())
    }
}

/// The widget before the keymap: in an application window, a focused grid
/// reads its own arrows and ends before a global binding of theirs, which
/// still runs where no widget reads them; a binding scoped to the window
/// still goes first.
#[test]
fn a_focused_grid_gets_its_arrows_before_a_global_binding() {
    use egui_pop_api::DefaultPlugins;
    use egui_pop_api::windows::WindowSpec;
    use egui_pop_test::windows::MultiWindow;
    use std::cell::RefCell;
    use std::rc::Rc;

    let ran = Ran::default();
    let r = ran.clone();
    let mut m = MultiWindow::new(move |ctx| {
        App::builder()
            .name("Import")
            .plugins(DefaultPlugins)
            .plugin(Globals(r))
            .build(ctx)
            .unwrap()
    });
    m.root_pass();
    let state = Rc::new(RefCell::new(LibraryState::<u32>::new("import")));
    let s = state.clone();
    m.app()
        .open_window(WindowSpec::new("import", "Import"), move |ui, _| {
            let shoot = Shoot {
                n: 20,
                stacked: false,
            };
            ThumbGrid::new("grid").show(ui, &mut s.borrow_mut(), &shoot, &Thumbs);
        });
    m.root_pass();
    let import = m.app().window_viewport(&"import".into()).unwrap();
    m.window_pass(import);
    m.window_pass(import);
    let cursor = || state.borrow().selection().cursor().copied();
    let take = || std::mem::take(&mut *ran.borrow_mut());
    let press = |m: &mut MultiWindow, v: egui::ViewportId, key: Key| {
        m.focus(v);
        m.press(v, key, Modifiers::NONE);
        if v == egui::ViewportId::ROOT {
            m.root_pass();
        } else {
            m.window_pass(v);
            m.window_pass(v);
        }
        m.root_pass();
        m.root_pass();
    };

    // Nothing focused in the window: the global binding runs.
    press(&mut m, import, Key::ArrowRight);
    assert_eq!(take(), ["demo.next"]);
    assert_eq!(cursor(), None);

    let (rect, _) = m.node(import, "IMG_00000").expect("the first cell");
    m.focus(import);
    m.click(import, rect.center());
    m.window_pass(import);
    m.window_pass(import);
    m.root_pass();
    assert_eq!(cursor(), Some(0));

    // The grid focused: its Right and End are its own.
    press(&mut m, import, Key::ArrowRight);
    assert_eq!(
        cursor(),
        Some(1),
        "Right in the focused grid ran {:?} instead",
        ran.borrow()
    );
    assert!(take().is_empty(), "a global binding took the grid's key");
    press(&mut m, import, Key::End);
    assert_eq!(cursor(), Some(19));
    press(&mut m, import, Key::Home);
    assert_eq!(cursor(), Some(0));
    assert!(take().is_empty());

    // A binding scoped to the window goes first.
    press(&mut m, import, Key::ArrowLeft);
    assert_eq!(take(), ["demo.import_left"]);

    // In the main window, where no widget reads Right, the global runs.
    press(&mut m, egui::ViewportId::ROOT, Key::ArrowRight);
    assert_eq!(take(), ["demo.next"]);
}

#[test]
fn badges_are_indicators_in_the_bottom_right_corner_of_their_cell() {
    let mut h = harness(12, false, false, CullKeymap::lightroom());
    h.run();
    let drawn: Vec<_> = egui_pop_core::indicator::drawn(&h.ctx)
        .into_iter()
        .filter(|r| r.id.starts_with("library.badge:"))
        .collect();
    let ids: Vec<&str> = drawn.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        ["library.badge:IMG_00001:HDR", "library.badge:IMG_00001:RAW"]
    );
    assert_eq!(drawn[0].tone, egui_pop_core::kit::Tone::Info);
    let cell = cell(&h, 1).rect();
    let (hdr, raw) = (drawn[0].rect, drawn[1].rect);
    // Inside the cell, the last badge at its right end, the first before it.
    assert!(hdr[0] + hdr[2] <= cell.right(), "{hdr:?} in {cell:?}");
    assert!(raw[0] + raw[2] <= hdr[0] + 0.5, "{raw:?} before {hdr:?}");
    assert!(
        raw[1] > cell.center().y,
        "{raw:?} in the bottom half of {cell:?}"
    );
}

// ---------------------------------------------------------------------------
// The library grid as a file browser's mode
// ---------------------------------------------------------------------------

mod library_grid_mode {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use egui::Key;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT as _, Queryable as _};
    use egui_pop_api::files::browser::{FileBrowser, FsSource, ViewModes};
    use egui_pop_api::files::fs::MemoryFs;
    use egui_pop_views::library::grid::Signal;
    use egui_pop_views::library::marks::{ColorLabel, CullKeymap, Flag, Marks};
    use egui_pop_views::library::mode::{LibraryGridMode, LibrarySignals, MODE_LIBRARY};

    type Shared = Arc<Mutex<HashMap<PathBuf, Marks>>>;

    /// A folder of images, one named so that type-ahead on "3" would
    /// jump to it, in a browser showing the library grid.
    fn browser(
        marks: &Shared,
        keymap: CullKeymap,
    ) -> (Harness<'static, FileBrowser>, LibrarySignals) {
        let mut fs = MemoryFs::new().dir("/p");
        for name in ["IMG_001.png", "IMG_002.png", "IMG_003.png", "3rd.png"] {
            fs = fs.file(format!("/p/{name}"), vec![0; 8]);
        }
        let source = Arc::new(FsSource::new(Arc::new(fs)));
        let modes = ViewModes::default();
        let signals = LibrarySignals::default();
        let shared = marks.clone();
        LibraryGridMode::new(move |e| {
            shared
                .lock()
                .unwrap()
                .get(&e.path)
                .copied()
                .unwrap_or_default()
        })
        .keymap(keymap)
        .signals(signals.clone())
        .register(&modes)
        .unwrap();
        let mut b = FileBrowser::new("lib-mode", source, "/p").with_modes(modes);
        b.set_mode(MODE_LIBRARY);
        let mut h = Harness::builder()
            .with_size(egui::vec2(640.0, 480.0))
            .build_ui_state(
                |ui, b: &mut FileBrowser| {
                    b.show(ui);
                },
                b,
            );
        egui_pop_core::icon::install(&h.ctx);
        h.run();
        h.run();
        (h, signals)
    }

    fn p(name: &str) -> PathBuf {
        Path::new("/p").join(name)
    }

    #[test]
    fn it_is_a_mode_of_the_switcher() {
        let (h, _) = browser(&Shared::default(), CullKeymap::lightroom());
        assert_eq!(h.state().mode(), MODE_LIBRARY);
        assert!(h.query_by_label_contains("Library grid").is_some());
    }

    #[test]
    fn culling_keys_rate_flag_and_label_without_typing_ahead() {
        let marks = Shared::default();
        let (mut h, signals) = browser(&marks, CullKeymap::lightroom());
        h.get_by_label("IMG_002.png").click();
        h.run();
        signals.drain();
        h.key_press(Key::Num3);
        h.event(egui::Event::Text("3".into()));
        h.run();
        assert_eq!(
            signals.drain(),
            [Signal::Rate {
                keys: vec![p("IMG_002.png")],
                rating: 3
            }]
        );
        // The "3" did not jump to 3rd.png.
        assert_eq!(h.state().focus(), Some(p("IMG_002.png").as_path()));
        h.key_press(Key::P);
        h.run();
        assert_eq!(
            signals.drain(),
            [Signal::Flag {
                keys: vec![p("IMG_002.png")],
                flag: Flag::Pick
            }]
        );
        // A label toggles from the focused file's marks.
        marks.lock().unwrap().insert(
            p("IMG_002.png"),
            Marks {
                labels: egui_pop_views::library::marks::Labels::default()
                    .with(ColorLabel::Red, true),
                ..Marks::default()
            },
        );
        h.key_press(Key::Num6);
        h.run();
        assert_eq!(
            signals.drain(),
            [Signal::Label {
                keys: vec![p("IMG_002.png")],
                label: ColorLabel::Red,
                on: false
            }]
        );
        // The selection, every file, in the browser's order.
        h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
        h.run();
        h.key_press(Key::Num1);
        h.run();
        let Some(Signal::Rate { keys, rating: 1 }) = signals.drain().pop() else {
            panic!("a rating of the selection");
        };
        assert_eq!(keys.len(), 4);
    }

    #[test]
    fn auto_advance_moves_the_focus_to_the_next_file() {
        let (mut h, signals) = browser(
            &Shared::default(),
            CullKeymap::lightroom().auto_advance(true),
        );
        h.get_by_label("IMG_001.png").click();
        h.run();
        h.key_press(Key::Num2);
        h.run();
        h.run();
        assert_eq!(signals.len(), 1);
        assert_eq!(h.state().focus(), Some(p("IMG_002.png").as_path()));
        assert!(h.state().selection().contains(&p("IMG_002.png")));
    }

    #[test]
    fn a_cell_says_its_marks() {
        let marks = Shared::default();
        marks.lock().unwrap().insert(
            p("IMG_001.png"),
            Marks {
                rating: 4,
                flag: Flag::Pick,
                ..Marks::default()
            },
        );
        let (h, _) = browser(&marks, CullKeymap::lightroom());
        let said = h
            .get_by_label("IMG_001.png")
            .accesskit_node()
            .description()
            .unwrap_or_default();
        assert_eq!(said, "Rated 4 of 5, Pick");
        assert_eq!(
            h.get_by_label("IMG_002.png").accesskit_node().description(),
            None
        );
    }
}
