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
    LibraryItems, LibraryState, Signal, Stack, Thumb, ThumbGrid, ThumbSource,
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
                let mut grid = ThumbGrid::new("grid").keymap(l.keymap.clone());
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
    h.query_all_by_label("Rate 2 of 5")
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
