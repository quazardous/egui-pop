//! The file browser drawn: modes, keyboard, thumbnails' order.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::files::browser::{
    FileBrowser, FileSource, FsSource, Priority, RecordingThumbnails, ThumbnailProvider,
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
    Harness::builder()
        .with_size(egui::vec2(640.0, 400.0))
        .build_ui_state(
            |ui, b: &mut FileBrowser| {
                b.show(ui);
            },
            browser,
        )
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
