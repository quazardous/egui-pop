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
    let mut h = Harness::builder()
        .with_size(egui::vec2(640.0, 400.0))
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
fn a_click_in_an_ancestors_column_goes_there() {
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
