//! The file browser drawn: modes, keyboard, thumbnails' order.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::files::browser::{
    self, DetailsMode, FileBrowser, FileSource, FsSource, Priority, RecordingThumbnails,
    ThumbnailProvider, ViewModes,
};
use egui_pop_api::files::fs::MemoryFs;

fn source(n: usize) -> Arc<dyn FileSource> {
    let mut fs = MemoryFs::new().dir("/p");
    for i in 1..=n {
        fs = fs.file(format!("/p/IMG_{i:03}.png"), vec![0; i]);
    }
    Arc::new(FsSource::new(Arc::new(fs)))
}

fn harness(browser: FileBrowser) -> Harness<'static, FileBrowser> {
    harness_sized(browser, 400.0)
}

fn harness_sized(browser: FileBrowser, height: f32) -> Harness<'static, FileBrowser> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(640.0, height))
        .build_ui_state(
            |ui, b: &mut FileBrowser| {
                b.show(ui);
            },
            browser,
        );
    // egui-pop's icon font, as an application has it: the switcher's icons.
    egui_pop_core::icon::install(&h.ctx);
    h.run();
    h
}

#[test]
fn the_switcher_without_the_icon_font_shows_its_labels() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(640.0, 400.0))
        .build_ui_state(
            |ui, b: &mut FileBrowser| {
                b.show(ui);
            },
            FileBrowser::new("bare", source(3), "/p"),
        );
    h.run();
    assert!(h.query_by_label("Details").is_some());
    assert!(h.query_by_label("Thumbnails").is_some());
}

#[test]
fn switching_modes_by_the_toolbar_keeps_selection_and_focus() {
    let mut h = harness(FileBrowser::new("switch", source(20), "/p"));
    h.run();
    h.get_by_label("IMG_003.png").click();
    h.run();
    h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowDown);
    h.run();
    let sel = h.state().selection().clone();
    assert_eq!(sel.len(), 2);
    h.get_by_label_contains("Thumbnails").click();
    h.run();
    assert_eq!(h.state().mode(), "thumbnails");
    assert_eq!(h.state().selection(), &sel);
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_004.png")));
    h.get_by_label_contains("Details").click();
    h.run();
    assert_eq!(h.state().selection(), &sel);
}

#[test]
fn the_keyboard_follows_each_modes_layout() {
    let mut h = harness(FileBrowser::new("keys", source(20), "/p"));
    h.run();
    h.get_by_label("IMG_001.png").click();
    h.run();
    h.key_press(Key::ArrowDown);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_002.png")));
    h.state_mut().set_mode("thumbnails");
    h.run();
    h.get_by_label("IMG_001.png").click();
    h.run();
    h.key_press(Key::ArrowRight);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_002.png")));
    // Down goes one row of the grid, not one entry.
    h.key_press(Key::ArrowDown);
    h.run();
    let focus = h.state().focus().unwrap().to_path_buf();
    assert_ne!(focus, PathBuf::from("/p/IMG_003.png"));
    h.key_press(Key::End);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_020.png")));
}

#[test]
fn type_ahead_moves_the_focus() {
    let mut h = harness(FileBrowser::new("type", source(20), "/p"));
    h.run();
    h.get_by_label("IMG_001.png").click();
    h.run();
    h.event(egui::Event::Text("img_01".into()));
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_010.png")));
}

#[test]
fn thumbnails_ask_for_what_is_visible_first_and_cancel_what_scrolled_away() {
    let rec = Arc::new(RecordingThumbnails::default());
    let provider: Arc<dyn ThumbnailProvider> = rec.clone();
    let mut b = FileBrowser::new("thumbs", source(200), "/p").with_thumbnails(provider);
    b.set_mode("thumbnails");
    let mut h = harness(b);
    h.run();
    {
        let r = rec.record.lock().unwrap();
        let first = r.frames.last().unwrap();
        assert!(!first.is_empty());
        assert_eq!(
            first[0],
            (PathBuf::from("/p/IMG_001.png"), Priority::Visible)
        );
        // Fewer than all: only what is in view, and the next row.
        assert!(first.len() < 200);
        let near = first.iter().position(|(_, p)| *p == Priority::Near);
        if let Some(n) = near {
            assert!(first[..n].iter().all(|(_, p)| *p == Priority::Visible));
        }
    }
    h.get_by_label("IMG_001.png").click();
    h.run();
    h.key_press(Key::End);
    h.run();
    h.run();
    let r = rec.record.lock().unwrap();
    let last = r.frames.last().unwrap();
    assert!(last.iter().any(|(p, _)| p == Path::new("/p/IMG_200.png")));
    assert!(!last.iter().any(|(p, _)| p == Path::new("/p/IMG_001.png")));
    assert!(
        r.cancelled
            .iter()
            .flatten()
            .any(|p| p == Path::new("/p/IMG_001.png")),
        "the first thumbnail, scrolled away, is cancelled"
    );
}

#[test]
fn ctrl_and_a_digit_switch_modes_while_the_entries_hold_the_keyboard() {
    let mut h = harness(FileBrowser::new("digits", source(20), "/p"));
    h.run();
    // Not before: the keys are the entries'.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Num2);
    h.run();
    assert_eq!(h.state().mode(), "details");
    h.get_by_label("IMG_003.png").click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Num2);
    h.run();
    assert_eq!(h.state().mode(), "thumbnails");
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_003.png")));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Num1);
    h.run();
    assert_eq!(h.state().mode(), "details");
    // A mode the switcher does not have changes nothing.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Num9);
    h.run();
    assert_eq!(h.state().mode(), "details");
    // As the keymap's command does it.
    browser::request_mode(&h.ctx, 2);
    h.run();
    assert_eq!(h.state().mode(), "thumbnails");
}

#[test]
fn the_switcher_is_segmented_up_to_four_modes_then_a_menu() {
    let many = ViewModes::default();
    for id in ["a", "b", "c"] {
        many.register(id, Arc::new(|| Box::new(DetailsMode::default())))
            .unwrap();
    }
    let mut h = harness(FileBrowser::new("many", source(3), "/p").with_modes(many.clone()));
    h.run();
    assert!(h.query_by_label("Thumbnails").is_none());
    assert!(h.query_by_label("View mode").is_some());
    many.unregister("c");
    let mut h = harness(FileBrowser::new("four", source(3), "/p").with_modes(many));
    h.run();
    assert!(h.query_by_label("Thumbnails").is_some());
    assert!(h.query_by_label("View mode").is_none());
}

/// `/r` holding `a` and `b`; `a` holding `sub` (two files) and images.
fn tree() -> Arc<dyn FileSource> {
    let mut fs = MemoryFs::new()
        .dir("/r")
        .dir("/r/a")
        .dir("/r/b")
        .dir("/r/a/sub")
        .file("/r/a/sub/in_1.png", vec![0; 3])
        .file("/r/a/sub/in_2.png", vec![0; 3]);
    for i in 1..=6 {
        fs = fs.file(format!("/r/a/IMG_{i:03}.png"), vec![0; i]);
    }
    Arc::new(FsSource::new(Arc::new(fs)))
}

fn columns(id: &str) -> FileBrowser {
    let mut b = FileBrowser::new(id.to_owned(), tree(), "/r/a")
        .with_root("/r")
        .with_modes(ViewModes::folders());
    assert!(b.set_mode("columns"));
    b
}

#[test]
fn the_folder_chooser_offers_details_and_columns() {
    assert_eq!(ViewModes::folders().ids(), ["details", "columns"]);
    let mut h = harness(columns("set"));
    h.run();
    assert!(h.query_by_label("Columns").is_some());
    assert!(h.query_by_label("Thumbnails").is_none());
    // The root's column, its open entry the place.
    let up = h.state().ancestors();
    assert_eq!(up.len(), 1);
    assert_eq!(up[0].place, Path::new("/r"));
    assert_eq!(
        up[0].open.map(|i| up[0].entries[i].path.clone()),
        Some(PathBuf::from("/r/a"))
    );
}

#[test]
fn columns_move_up_and_down_in_a_column_and_left_and_right_between_them() {
    let mut h = harness(columns("keys"));
    h.run();
    h.get_by_label("sub").click();
    h.run();
    h.key_press(Key::ArrowDown);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/r/a/IMG_001.png")));
    // Right on a file stays.
    h.key_press(Key::ArrowRight);
    h.run();
    assert_eq!(h.state().place(), Path::new("/r/a"));
    h.key_press(Key::ArrowUp);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/r/a/sub")));
    // The focused folder is looked into: its entries in the next column.
    assert!(h.query_by_label("in_1.png").is_some());
    // Right enters it, its first entry focused.
    h.key_press(Key::ArrowRight);
    h.run();
    assert_eq!(h.state().place(), Path::new("/r/a/sub"));
    assert_eq!(h.state().focus(), Some(Path::new("/r/a/sub/in_1.png")));
    assert_eq!(h.state().ancestors().len(), 2);
    // Left goes back, the folder left focused.
    h.key_press(Key::ArrowLeft);
    h.run();
    assert_eq!(h.state().place(), Path::new("/r/a"));
    assert_eq!(h.state().focus(), Some(Path::new("/r/a/sub")));
    h.key_press(Key::ArrowLeft);
    h.run();
    assert_eq!(h.state().place(), Path::new("/r"));
    assert_eq!(h.state().focus(), Some(Path::new("/r/a")));
    // Not above the root.
    h.key_press(Key::ArrowLeft);
    h.run();
    assert_eq!(h.state().place(), Path::new("/r"));
}

#[test]
fn columns_a_click_in_an_ancestors_column_goes_there() {
    let mut h = harness(columns("click"));
    h.run();
    h.get_by_label("b").click();
    h.run();
    assert_eq!(h.state().place(), Path::new("/r"));
    assert_eq!(h.state().focus(), Some(Path::new("/r/b")));
    assert!(h.state().selection().contains(Path::new("/r/b")));
}

#[test]
fn the_selection_is_kept_from_details_to_columns_and_back() {
    let mut h = harness(columns("keep"));
    h.state_mut().set_mode("details");
    h.run();
    h.get_by_label("IMG_002.png").click();
    h.run();
    h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowDown);
    h.run();
    let sel = h.state().selection().clone();
    assert_eq!(sel.len(), 2);
    h.get_by_label_contains("Columns").click();
    h.run();
    assert_eq!(h.state().mode(), "columns");
    assert_eq!(h.state().selection(), &sel);
    assert_eq!(h.state().focus(), Some(Path::new("/r/a/IMG_003.png")));
    h.get_by_label_contains("Details").click();
    h.run();
    assert_eq!(h.state().selection(), &sel);
}

#[test]
fn columns_ask_for_the_focused_files_thumbnail_alone() {
    let rec = Arc::new(RecordingThumbnails::default());
    let provider: Arc<dyn ThumbnailProvider> = rec.clone();
    let mut h = harness(columns("thumb").with_thumbnails(provider));
    h.run();
    h.get_by_label("IMG_004.png").click();
    h.run();
    h.run();
    let r = rec.record.lock().unwrap();
    assert_eq!(
        r.frames.last().unwrap(),
        &[(PathBuf::from("/r/a/IMG_004.png"), Priority::Visible)]
    );
}

/// Picks a mode in the switcher's menu (more than four modes), as a
/// person does: the menu opened, then its entry.
fn pick(h: &mut Harness<'static, FileBrowser>, label: &str) {
    h.get_by_label("View mode").click();
    h.run();
    h.get_all_by_label(label).last().unwrap().click();
    h.run();
}

fn filmstrip(id: &str, n: usize) -> FileBrowser {
    let mut b = FileBrowser::new(id.to_owned(), source(n), "/p").with_modes(ViewModes::built_in());
    assert!(b.set_mode("filmstrip"));
    b
}

#[test]
fn every_built_in_mode_is_offered_in_order() {
    assert_eq!(
        ViewModes::built_in().ids(),
        [
            "details",
            "thumbnails",
            "filmstrip",
            "columns",
            "carousel",
            "agenda"
        ]
    );
}

#[test]
fn the_filmstrip_moves_left_and_right_and_shows_the_focused_entry_large() {
    let mut h = harness(filmstrip("film-keys", 30));
    h.run();
    h.get_by_label("IMG_003.png").click();
    h.run();
    assert!(h.query_by_label("Preview of IMG_003.png").is_some());
    h.key_press(Key::ArrowRight);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_004.png")));
    assert!(h.query_by_label("Preview of IMG_004.png").is_some());
    // Up and Down stay: one row.
    h.key_press(Key::ArrowDown);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_004.png")));
    h.key_press(Key::ArrowLeft);
    h.key_press(Key::ArrowLeft);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_002.png")));
    h.key_press(Key::End);
    h.run();
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_030.png")));
    assert!(
        h.query_by_label("IMG_030.png").is_some(),
        "the strip scrolled to it"
    );
}

/// A click on a thumbnail at the strip's edge: its centre (where kittest's
/// `click` presses) lies past the window, so that press reaches nothing,
/// as a press off the window would. A press on its visible part, or a
/// screen reader's click, takes it, and it scrolls whole into view.
#[test]
fn the_filmstrip_takes_a_click_on_a_thumbnail_partly_in_view() {
    let width = 640.0;
    let mut h = harness(filmstrip("film-click", 200));
    h.run();
    let rect = h.get_by_label("IMG_005.png").rect();
    assert!(
        rect.left() < width && rect.center().x > width,
        "the fifth cell is cut by the window's edge: {rect:?}"
    );
    // On its visible part.
    let at = egui::pos2((rect.left() + width) / 2.0, rect.center().y);
    h.event(egui::Event::PointerMoved(at));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    h.run();
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_005.png")));
    assert!(h.state().selection().contains(Path::new("/p/IMG_005.png")));
    let rect = h.get_by_label("IMG_005.png").rect();
    assert!(rect.right() <= width, "scrolled whole into view: {rect:?}");
    // Through accessibility, the next one.
    h.get_by_label("IMG_006.png").click_accesskit();
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_006.png")));
    assert!(h.state().selection().contains(Path::new("/p/IMG_006.png")));
}

#[test]
fn the_filmstrip_asks_for_the_large_view_first_then_the_strip_in_order() {
    let rec = Arc::new(RecordingThumbnails::default());
    let provider: Arc<dyn ThumbnailProvider> = rec.clone();
    let mut h = harness(filmstrip("film-thumbs", 200).with_thumbnails(provider));
    h.state_mut().focus_on(Path::new("/p/IMG_005.png"), false);
    h.run();
    h.run();
    let r = rec.record.lock().unwrap();
    let last = r.frames.last().unwrap();
    // The large view's entry first, then the strip from its start.
    assert_eq!(
        last[0],
        (PathBuf::from("/p/IMG_005.png"), Priority::Visible)
    );
    assert_eq!(
        last[1],
        (PathBuf::from("/p/IMG_001.png"), Priority::Visible)
    );
    let near = last
        .iter()
        .position(|(_, p)| *p == Priority::Near)
        .expect("the next screen asked");
    assert!(last[..near].iter().all(|(_, p)| *p == Priority::Visible));
    assert!(last.len() < 200);
}

#[test]
fn the_selection_is_kept_across_every_built_in_mode() {
    let mut h = harness(filmstrip("film-keep", 20));
    h.state_mut().set_mode("details");
    h.run();
    h.get_by_label("IMG_002.png").click();
    h.run();
    h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowDown);
    h.run();
    let sel = h.state().selection().clone();
    assert_eq!(sel.len(), 2);
    for (mode, label) in [
        ("thumbnails", "Thumbnails"),
        ("filmstrip", "Filmstrip"),
        ("carousel", "Carousel"),
        ("agenda", "Agenda"),
        ("details", "Details"),
    ] {
        pick(&mut h, label);
        assert_eq!(h.state().mode(), mode);
        assert_eq!(h.state().selection(), &sel, "{mode}");
        assert_eq!(
            h.state().focus(),
            Some(Path::new("/p/IMG_003.png")),
            "{mode}"
        );
    }
}

#[test]
fn quick_look_opens_with_ctrl_y_or_space_moves_with_the_arrows_and_closes() {
    let mut h = harness(FileBrowser::new("look", source(20), "/p"));
    h.run();
    h.get_by_label("IMG_003.png").click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Y);
    h.run();
    assert!(h.state().quick_look());
    assert!(h.query_by_label("Quick look: IMG_003.png").is_some());
    h.key_press(Key::ArrowRight);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_004.png")));
    assert!(h.query_by_label("Quick look: IMG_004.png").is_some());
    h.key_press(Key::ArrowUp);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_003.png")));
    h.key_press(Key::Escape);
    h.run();
    assert!(!h.state().quick_look());
    assert!(h.query_by_label("Quick look: IMG_003.png").is_none());
    // Space, the entries holding the keyboard, opens and closes it.
    h.get_by_label("IMG_005.png").click();
    h.run();
    h.key_press(Key::Space);
    h.run();
    assert!(h.state().quick_look());
    h.key_press(Key::Space);
    h.run();
    assert!(!h.state().quick_look());
    // Ctrl+Y closes it too.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Y);
    h.run();
    assert!(h.state().quick_look());
    h.key_press_modifiers(Modifiers::COMMAND, Key::Y);
    h.run();
    assert!(!h.state().quick_look());
}

#[test]
fn quick_look_asks_for_its_thumbnail_as_visible() {
    let rec = Arc::new(RecordingThumbnails::default());
    let provider: Arc<dyn ThumbnailProvider> = rec.clone();
    let mut h = harness(FileBrowser::new("look-thumbs", source(5), "/p").with_thumbnails(provider));
    h.run();
    h.get_by_label("IMG_002.png").click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Y);
    h.run();
    h.run();
    let r = rec.record.lock().unwrap();
    assert!(
        r.frames
            .last()
            .unwrap()
            .iter()
            .any(|(p, pr)| p == Path::new("/p/IMG_002.png") && *pr == Priority::Visible)
    );
}

fn carousel(id: &str, n: usize) -> FileBrowser {
    let mut b = FileBrowser::new(id.to_owned(), source(n), "/p").with_modes(ViewModes::built_in());
    assert!(b.set_mode("carousel"));
    b
}

#[test]
fn the_carousel_shows_the_focused_entry_large_in_the_middle_its_neighbours_smaller() {
    let mut h = harness(carousel("car-show", 30));
    h.state_mut().focus_on(Path::new("/p/IMG_010.png"), false);
    h.run();
    h.run();
    egui_pop_test::assert_all_named(&h);
    let mid = h.get_by_label("IMG_010.png").rect();
    assert!(
        (mid.center().x - 320.0).abs() < 2.0,
        "in the middle: {mid:?}"
    );
    let next = h.get_by_label("IMG_011.png").rect();
    let previous = h.get_by_label("IMG_009.png").rect();
    assert!(next.width() < mid.width() && previous.width() < mid.width());
    assert!(next.left() > mid.right() && previous.right() < mid.left());
    let farther = h.get_by_label("IMG_012.png").rect();
    assert!(farther.width() < next.width());
    // Only what fits: not every entry.
    assert!(h.query_by_label("IMG_030.png").is_none());
    assert!(h.query_by_label("IMG_001.png").is_none());
}

#[test]
fn the_carousel_moves_with_left_and_right() {
    let mut h = harness(carousel("car-keys", 30));
    h.run();
    h.get_by_label("IMG_002.png").click();
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_002.png")));
    h.key_press(Key::ArrowRight);
    h.key_press(Key::ArrowRight);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_004.png")));
    let mid = h.get_by_label("IMG_004.png").rect();
    assert!((mid.center().x - 320.0).abs() < 2.0, "{mid:?}");
    h.key_press(Key::ArrowLeft);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_003.png")));
    // Up and Down stay: one row.
    h.key_press(Key::ArrowUp);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_003.png")));
    h.key_press(Key::End);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_030.png")));
    assert!(h.query_by_label("IMG_030.png").is_some());
    h.key_press(Key::Home);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_001.png")));
}

#[test]
fn the_carousel_brings_a_clicked_neighbour_to_the_middle() {
    let mut h = harness(carousel("car-click", 30));
    h.state_mut().focus_on(Path::new("/p/IMG_010.png"), false);
    h.run();
    h.get_by_label("IMG_011.png").click();
    h.run();
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_011.png")));
    assert!(h.state().selection().contains(Path::new("/p/IMG_011.png")));
    let mid = h.get_by_label("IMG_011.png").rect();
    assert!((mid.center().x - 320.0).abs() < 2.0, "{mid:?}");
    // Through accessibility too.
    h.get_by_label("IMG_009.png").click_accesskit();
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/p/IMG_009.png")));
}

#[test]
fn the_carousel_asks_for_the_middle_first_then_outwards() {
    let rec = Arc::new(RecordingThumbnails::default());
    let provider: Arc<dyn ThumbnailProvider> = rec.clone();
    let mut h = harness(carousel("car-thumbs", 30).with_thumbnails(provider));
    h.state_mut().focus_on(Path::new("/p/IMG_010.png"), false);
    h.run();
    h.run();
    let r = rec.record.lock().unwrap();
    let last = r.frames.last().unwrap();
    let paths: Vec<_> = last.iter().map(|(p, _)| p.clone()).collect();
    assert_eq!(
        paths[..3],
        [
            PathBuf::from("/p/IMG_010.png"),
            PathBuf::from("/p/IMG_011.png"),
            PathBuf::from("/p/IMG_009.png"),
        ]
    );
    let near = last
        .iter()
        .position(|(_, p)| *p == Priority::Near)
        .expect("the next one beyond each side");
    assert!(last[..near].iter().all(|(_, p)| *p == Priority::Visible));
    assert_eq!(last.len() - near, 2);
}

/// `/d` holding files changed on three days and a folder without a time.
fn days() -> Arc<dyn FileSource> {
    let at = |day: u64, hour: u64| {
        Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(day * 86_400 + hour * 3600))
    };
    // 2025-11-01 is day 20 393.
    let fs = MemoryFs::new()
        .dir("/d")
        .dir("/d/sub")
        .file_at("/d/a_old.png", vec![0; 1], at(20_390, 9))
        .file_at("/d/b_mid1.png", vec![0; 1], at(20_393, 9))
        .file_at("/d/c_mid2.png", vec![0; 1], at(20_393, 15))
        .file_at("/d/d_new.png", vec![0; 1], at(20_395, 12));
    Arc::new(FsSource::new(Arc::new(fs)))
}

/// An agenda tall enough for its four days.
fn agenda(id: &str) -> Harness<'static, FileBrowser> {
    let mut b = FileBrowser::new(id.to_owned(), days(), "/d").with_modes(ViewModes::built_in());
    assert!(b.set_mode("agenda"));
    b.set_thumbnail_size(64.0);
    harness_sized(b, 800.0)
}

#[test]
fn the_agenda_shows_one_heading_per_day_the_newest_first() {
    let mut h = agenda("ag-show");
    h.run();
    h.run();
    egui_pop_test::assert_all_named(&h);
    let heading = |h: &Harness<'static, FileBrowser>, name: &str| {
        h.get_by_role_and_label(egui::accesskit::Role::Heading, name)
            .rect()
    };
    let newest = heading(&h, "2025-11-03");
    let mid = heading(&h, "2025-11-01");
    let old = heading(&h, "2025-10-29");
    let undated = heading(&h, "No date");
    assert!(newest.top() < mid.top() && mid.top() < old.top() && old.top() < undated.top());
    // Each entry under its day's heading, the newest of a day first.
    let y = |h: &Harness<'static, FileBrowser>, name: &str| h.get_by_label(name).rect().top();
    assert!(newest.top() < y(&h, "d_new.png") && y(&h, "d_new.png") < mid.top());
    assert!(mid.top() < y(&h, "c_mid2.png") && y(&h, "b_mid1.png") < old.top());
    let (c, b) = (
        h.get_by_label("c_mid2.png").rect(),
        h.get_by_label("b_mid1.png").rect(),
    );
    assert!(c.left() < b.left(), "the newest of the day first");
    assert!(old.top() < y(&h, "a_old.png") && y(&h, "a_old.png") < undated.top());
    assert!(undated.top() < y(&h, "sub"));
}

#[test]
fn the_agenda_moves_through_its_days_with_the_arrows() {
    let mut h = agenda("ag-keys");
    h.run();
    h.get_by_label("d_new.png").click();
    h.run();
    h.key_press(Key::ArrowRight);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/d/c_mid2.png")));
    h.key_press(Key::ArrowRight);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/d/b_mid1.png")));
    // Down goes to the next day's row, its column as near as it goes.
    h.key_press(Key::ArrowDown);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/d/a_old.png")));
    h.key_press(Key::ArrowUp);
    h.key_press(Key::ArrowUp);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/d/d_new.png")));
    h.key_press(Key::End);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/d/sub")));
    h.key_press(Key::Home);
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/d/d_new.png")));
}

#[test]
fn the_agenda_selects_a_clicked_entry_and_asks_for_its_thumbnails() {
    let rec = Arc::new(RecordingThumbnails::default());
    let provider: Arc<dyn ThumbnailProvider> = rec.clone();
    let mut b = FileBrowser::new("ag-click", days(), "/d")
        .with_modes(ViewModes::built_in())
        .with_thumbnails(provider);
    assert!(b.set_mode("agenda"));
    b.set_thumbnail_size(64.0);
    let mut h = harness_sized(b, 800.0);
    h.run();
    h.get_by_label("b_mid1.png").click();
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/d/b_mid1.png")));
    assert!(h.state().selection().contains(Path::new("/d/b_mid1.png")));
    h.get_by_label("a_old.png").click_accesskit();
    h.run();
    assert_eq!(h.state().focus(), Some(Path::new("/d/a_old.png")));
    let r = rec.record.lock().unwrap();
    let paths: Vec<_> = r
        .frames
        .last()
        .unwrap()
        .iter()
        .map(|(p, _)| p.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        paths,
        [
            "/d/d_new.png",
            "/d/c_mid2.png",
            "/d/b_mid1.png",
            "/d/a_old.png",
            "/d/sub"
        ]
    );
}

#[test]
fn the_switcher_offers_the_carousel_and_the_agenda() {
    let mut h = harness(carousel("car-switch", 5));
    h.run();
    pick(&mut h, "Agenda");
    assert_eq!(h.state().mode(), "agenda");
    pick(&mut h, "Carousel");
    assert_eq!(h.state().mode(), "carousel");
    // Ctrl and a digit, in the switcher's order.
    h.get_by_label("IMG_001.png").click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Num6);
    h.run();
    assert_eq!(h.state().mode(), "agenda");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Num5);
    h.run();
    assert_eq!(h.state().mode(), "carousel");
}
