//! The files page: the file chooser's kinds of choice, each opened by a
//! button, the result as it arrives, the recent places and bookmarks.
//! The choices start in a demo folder the page makes, so that the
//! in-app picker shows the same entries everywhere, with previews: its
//! images, text, document and table are drawn and written by code at
//! start (the repository holds no binary file), on the disk on the
//! desktop and in a folder held in memory on the web.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};

use egui::Ui;
use egui_pop::api::files::fs::MemoryFs;
use egui_pop::api::files::groups::SidecarRule;
use egui_pop::api::files::{self, FILES, FileRequest};
use egui_pop::api::host::Pending;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::state::Slot;
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::theme::Tokens;

use super::section;
use crate::shell::SHELL;

pub const FILES_PAGE: PluginId = PluginId::from_static("gallery.files");

/// The page.
#[derive(Debug, Default)]
pub struct FilesPage;

impl Plugin for FilesPage {
    fn id(&self) -> PluginId {
        FILES_PAGE
    }

    fn title(&self) -> String {
        "Files".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, files::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // The web has no file system: the picker reads the demo folder
        // from memory there.
        if cfg!(target_arch = "wasm32")
            && let Some(f) = ctx.app().service(&FILES)
        {
            f.set_file_system(Arc::new(demo_memory_folder()));
        }
        // A photo application's rule, as this page's own: egui-pop has none.
        if let Some(f) = ctx.app().service(&FILES) {
            let photos =
                SidecarRule::new("gallery.photos", &["cr3", "jpg", "png"], &["xmp", "jpg"])
                    .badge("jpg", "JPEG");
            f.groupings().register(Arc::new(photos));
        }
        ctx.story(
            Story::new(StoryId::from_static("files"), "Choosing files", show)
                .description("The system's file chooser off the interface thread, or the in-app picker: keyboard, typed paths with completion, filters, recent places and bookmarks, a preview pane (images, text, documents, folders) from pluggable providers, a dialog you resize and a splitter beside the pane, a list with columns to sort by or a grid of thumbnails with a size control, all remembered.")
                .group("Components", 52),
        )?;
        Ok(())
    }
}

/// The demo folder's folders.
const DEMO_DIRS: [&str; 4] = ["Photos/2025", "Photos/2026", "Documents", "Exports"];

/// When the demo files were last changed, for every one of them: the
/// same facts on every machine (2026-01-15 10:04 UTC).
pub fn demo_time() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_768_471_440)
}

/// Where the demo folder is: in the temporary folder on the desktop, at
/// `/demo` in the web build's memory.
pub fn demo_root() -> PathBuf {
    if cfg!(target_arch = "wasm32") {
        PathBuf::from("/demo")
    } else {
        egui_pop::platform::dirs::temp_dir().join("egui-pop-gallery-files")
    }
}

/// The demo folder on the disk: the same files on every machine,
/// rewritten when they differ.
pub fn demo_folder() -> PathBuf {
    let root = demo_root();
    if cfg!(target_arch = "wasm32") {
        return root;
    }
    for d in DEMO_DIRS {
        let _ = std::fs::create_dir_all(root.join(d));
    }
    for (name, bytes) in demo_files() {
        let p = root.join(name);
        if std::fs::read(&p).ok().as_ref() != Some(bytes) {
            let _ = std::fs::write(&p, bytes);
        }
        if let Ok(f) = std::fs::File::options().write(true).open(&p) {
            let _ = f.set_modified(demo_time());
        }
    }
    // Last, for what was made above to count for nothing: the picker's list
    // shows when a folder changed. Windows opens no folder this way, and keeps
    // the real time.
    for d in DEMO_DIRS {
        let mut at = root.join(d);
        while at != root {
            if let Ok(f) = std::fs::File::open(&at) {
                let _ = f.set_modified(demo_time());
            }
            at.pop();
        }
    }
    root
}

/// The demo folder held in memory (the web build's).
pub fn demo_memory_folder() -> MemoryFs {
    let root = demo_root();
    let mut fs = MemoryFs::new();
    for d in DEMO_DIRS {
        fs = fs.dir(root.join(d));
    }
    for (name, bytes) in demo_files() {
        fs = fs.file_at(root.join(name), bytes.clone(), Some(demo_time()));
    }
    fs
}

/// The demo files, by path in the folder, made once.
pub fn demo_files() -> &'static [(&'static str, Vec<u8>)] {
    static FILES: OnceLock<Vec<(&'static str, Vec<u8>)>> = OnceLock::new();
    FILES.get_or_init(|| {
        vec![
            ("Photos/harbour.png", png(&harbour())),
            ("Photos/lighthouse.jpg", jpeg(&lighthouse())),
            ("Photos/2025/dunes.png", png(&dunes())),
            ("Photos/2026/meadow.jpg", jpeg(&meadow())),
            ("Photos/2026/meadow.xmp", XMP.as_bytes().to_vec()),
            // No provider reads this RAW: its JPEG stands in, in the grid and
            // in the pane.
            ("Photos/2026/reef.cr3", RAW.to_vec()),
            ("Photos/2026/reef.cr3.xmp", XMP.as_bytes().to_vec()),
            ("Photos/2026/reef.jpg", jpeg(&dunes())),
            ("Photos/notes.txt", NOTES.as_bytes().to_vec()),
            // What "Save as…" proposes: saving asks before replacing it.
            ("Exports/harbour-edit.png", png(&harbour())),
            ("Documents/plan.md", PLAN.as_bytes().to_vec()),
            ("Documents/budget.csv", BUDGET.as_bytes().to_vec()),
        ]
    })
}

/// Not a RAW, and no text either: only the name says what it would be.
const RAW: &[u8] = &[0xff, 0x00, 0xfe, 0x01, 0xfd, 0x02, 0xfc, 0x03];

const XMP: &str = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"/>\n";

const NOTES: &str = "Harbour walk, 14 March

- Light from the west after four.
- The keeper says the beam turns every twelve seconds.
- Try the long lens from the end of the pier.
- Ask about the boat that leaves at dawn.
";

const PLAN: &str = "# Spring exhibition

Twelve prints from the **coast** series, framed by the end of April.

## Steps

1. Pick the prints
2. Proof them on matte paper
3. Order the frames

> Keep the captions short.

---

```
size: 40 x 50 cm
```
";

const BUDGET: &str = "item,quantity,unit price,total
Prints,12,18.50,222.00
Frames,12,24.00,288.00
Mats,12,6.00,72.00
Total,,,582.00
";

type Rgb = image::RgbImage;

fn png(image: &Rgb) -> Vec<u8> {
    let mut out = Vec::new();
    let _ = image.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png);
    out
}

fn jpeg(image: &Rgb) -> Vec<u8> {
    let mut out = Vec::new();
    let _ = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 88).encode_image(image);
    out
}

/// Between two colours, `t` from 0 to 1.
fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> image::Rgb<u8> {
    let t = t.clamp(0.0, 1.0);
    image::Rgb(std::array::from_fn(|i| {
        (f32::from(a[i]) + (f32::from(b[i]) - f32::from(a[i])) * t).round() as u8
    }))
}

/// A boat in a harbour at the end of the day.
fn harbour() -> Rgb {
    let (w, h) = (240u32, 160u32);
    let horizon = 100.0;
    Rgb::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32, y as f32);
        // The sun, low on the left.
        let sun = ((fx - 60.0).powi(2) + (fy - 70.0).powi(2)).sqrt();
        // The boat: a hull and a sail.
        let hull = (104.0..=114.0).contains(&fy) && (fx - 150.0).abs() <= 34.0 - (fy - 104.0) * 1.2;
        let sail =
            (50.0..104.0).contains(&fy) && (152.0..=152.0 + (fy - 50.0) * 0.55).contains(&fx);
        let mast = (46.0..104.0).contains(&fy) && (148.0..151.0).contains(&fx);
        if hull {
            image::Rgb([92, 52, 36])
        } else if sail {
            image::Rgb([244, 238, 226])
        } else if mast {
            image::Rgb([60, 44, 36])
        } else if fy < horizon {
            if sun < 16.0 {
                image::Rgb([255, 214, 120])
            } else {
                mix([96, 150, 210], [250, 190, 140], fy / horizon)
            }
        } else {
            // The sea, the sun's light on it, and waves.
            let wave = ((fx * 0.35 + fy * 1.7).sin() * 0.5 + 0.5) * 0.12;
            let glint = (fx - 60.0).abs() < 10.0 - (fy - horizon) * 0.08;
            if glint && wave > 0.06 {
                image::Rgb([255, 220, 160])
            } else {
                mix([40, 96, 140], [16, 48, 84], (fy - horizon) / 60.0 + wave)
            }
        }
    })
}

/// A lighthouse on a rock at night.
fn lighthouse() -> Rgb {
    let (w, h) = (160u32, 240u32);
    Rgb::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32, y as f32);
        let tower_half = 9.0 + (fy - 70.0) * 0.08;
        let tower = (70.0..196.0).contains(&fy) && (fx - 80.0).abs() <= tower_half;
        let lamp = (52.0..70.0).contains(&fy) && (fx - 80.0).abs() <= 8.0;
        let roof = (40.0..52.0).contains(&fy) && (fx - 80.0).abs() <= (fy - 40.0) * 0.9;
        let rock = fy >= 190.0 - ((fx - 80.0) / 30.0).cos() * 10.0 && (fx - 80.0).abs() < 60.0;
        // The beam, to the left.
        let beam = fx < 72.0 && (fy - 61.0).abs() <= (72.0 - fx) * 0.18;
        // A few stars, always the same.
        let star =
            (x.wrapping_mul(7919) ^ y.wrapping_mul(104_729)).is_multiple_of(211) && fy < 150.0;
        if lamp {
            image::Rgb([255, 236, 150])
        } else if roof {
            image::Rgb([40, 40, 48])
        } else if tower {
            // Red and white bands.
            if (((fy - 70.0) / 21.0) as u32).is_multiple_of(2) {
                image::Rgb([236, 236, 230])
            } else {
                image::Rgb([196, 48, 44])
            }
        } else if rock {
            mix([70, 64, 60], [36, 32, 30], (fy - 180.0) / 60.0)
        } else if fy >= 200.0 {
            mix([20, 40, 72], [8, 20, 40], (fy - 200.0) / 40.0)
        } else if beam {
            mix([250, 236, 170], [60, 70, 110], (72.0 - fx) / 72.0)
        } else if star {
            image::Rgb([230, 230, 255])
        } else {
            mix([12, 18, 48], [44, 60, 110], fy / 200.0)
        }
    })
}

/// Dunes under an orange sky.
fn dunes() -> Rgb {
    Rgb::from_fn(200, 120, |x, y| {
        let (fx, fy) = (x as f32, y as f32);
        let near = 84.0 + (fx * 0.045).sin() * 14.0;
        let far = 60.0 + (fx * 0.03 + 1.3).sin() * 10.0;
        if fy > near {
            mix([226, 160, 92], [184, 112, 60], (fy - near) / 30.0)
        } else if fy > far {
            mix([208, 140, 84], [236, 178, 110], (fy - far) / 30.0)
        } else {
            mix([250, 150, 90], [255, 214, 150], fy / 60.0)
        }
    })
}

/// A meadow with flowers under a blue sky.
fn meadow() -> Rgb {
    Rgb::from_fn(200, 120, |x, y| {
        let (fx, fy) = (x as f32, y as f32);
        let hill = 62.0 + (fx * 0.025).cos() * 12.0;
        let flower = fy > hill + 6.0 && (x * 31 + y * 17).is_multiple_of(97);
        if flower {
            image::Rgb([250, 230, 90])
        } else if fy > hill {
            mix([110, 176, 72], [52, 112, 44], (fy - hill) / 50.0)
        } else {
            mix([90, 150, 225], [200, 226, 248], fy / hill)
        }
    })
}

/// The choice asked and not answered yet.
const PENDING: Slot<Option<Pending>> = Slot::new("gallery.files.pending");

/// What the last choice gave.
const LAST: Slot<String> = Slot::new("gallery.files.last");

fn show(ui: &mut Ui, app: &AppHandle) {
    let t = Tokens::of(ui.ctx());
    let Some(f) = app.service(&FILES) else {
        ui.weak("The files plugin is not connected.");
        return;
    };
    let ctx = ui.ctx().clone();
    ui.label(if cfg!(target_arch = "wasm32") {
        "On the web, the in-app picker reads the demo folder from memory."
    } else if files::native_available() {
        "The system's chooser is available (setting `files.native`); off, the in-app picker below answers."
    } else {
        "No system chooser here (the `native-dialogs` feature is off, or no portal): the in-app picker answers."
    });
    section(ui, "Choose");
    let root = demo_folder();
    let mut asked = None;
    ui.horizontal_wrapped(|ui| {
        if ui.button("Open an image…").clicked() {
            asked = Some(
                FileRequest::open("Open an image")
                    .purpose("gallery.open_image")
                    .directory(root.join("Photos"))
                    .filter("Images", &["png", "jpg"]),
            );
        }
        if ui.button("Open several…").clicked() {
            asked = Some(FileRequest::open_many("Open files").directory(&root));
        }
        if ui.button("Save as…").clicked() {
            asked = Some(
                FileRequest::save("Export the image", "harbour-edit.png")
                    .purpose("gallery.export_image")
                    .directory(root.join("Exports"))
                    .filter("PNG", &["png"]),
            );
        }
        if ui.button("Choose a folder…").clicked() {
            asked = Some(FileRequest::folder("Choose a folder").directory(&root));
        }
    });
    if let Some(request) = asked {
        let p = f.choose(app, request);
        PENDING.set(&ctx, Some(p));
    }
    let pending = PENDING.get(&ctx).flatten();
    if let Some(r) = pending.as_ref().and_then(Pending::take) {
        let text = match files::chosen(&r) {
            Ok(files::Chosen::Paths(paths)) => paths
                .iter()
                .map(|p| p.strip_prefix(&root).unwrap_or(p).display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
            Ok(files::Chosen::Cancelled) => "cancelled".into(),
            Err(e) => format!("refused: {e}"),
        };
        PENDING.set(&ctx, None);
        LAST.set(&ctx, text);
    }
    ui.add_space(t.space.s);
    let last = LAST.get(&ctx).unwrap_or_else(|| "nothing yet".into());
    ui.label(format!("Last choice: {last}"));
    section(ui, "Places");
    let name = |p: &PathBuf| {
        p.strip_prefix(&root)
            .map(|r| format!("demo/{}", r.display()))
            .unwrap_or_else(|_| p.display().to_string())
    };
    let recent = files::recent(app);
    let marks = files::bookmarks(app);
    ui.label(format!(
        "Recent: {}",
        if recent.is_empty() {
            "none".into()
        } else {
            recent.iter().map(name).collect::<Vec<_>>().join(", ")
        }
    ));
    ui.label(format!(
        "Bookmarks: {}",
        if marks.is_empty() {
            "none".into()
        } else {
            marks.iter().map(name).collect::<Vec<_>>().join(", ")
        }
    ));
    section(ui, "Keys in the in-app picker");
    egui::Grid::new("gallery.files.keys")
        .num_columns(2)
        .spacing([t.space.l, t.space.xs])
        .show(ui, |ui| {
            for (k, what) in [
                (
                    "Typing",
                    "filters the folder (fuzzy), or takes a path with / or ~",
                ),
                (
                    "Tab",
                    "completes the typed path; else the next widget (the splitter, past the places)",
                ),
                (
                    "/, ~",
                    "on an entry: start a path in the field, from the root or from home",
                ),
                (
                    "Enter, Space",
                    "on a segment of the path: open that folder; on its chevron: list its subfolders",
                ),
                (
                    "Up, Down, Tab, Escape",
                    "in a segment's menu: move, or close it and go back to the chevron",
                ),
                (
                    "Left, Right",
                    "on the splitter: the pane's width (Enter: its default)",
                ),
                (
                    "Arrows",
                    "on the dialog's corner handle: the dialog's size",
                ),
                (
                    "Up, Down, Enter",
                    "move, then enter a folder or choose a file",
                ),
                ("Shift+Up, Shift+Down", "extend the selection (several)"),
                ("Left, Right, Home, End", "in the grid: move, too"),
                ("Ctrl+Plus, Ctrl+Minus", "in the grid: the thumbnails' size"),
                ("Backspace, Alt+Up", "the parent folder"),
                ("Ctrl+Enter", "choose the folder, or the selection"),
                ("Ctrl+D", "bookmark the folder"),
                ("Alt+P", "show or hide the preview pane"),
                ("Escape", "cancel (from the field: twice)"),
            ] {
                ui.monospace(k);
                ui.label(what);
                ui.end_row();
            }
            // Declared, so it says what it is bound to now.
            ui.monospace(app.keys_for(&files::CMD_NEW_FOLDER).join(", "));
            ui.label("a new folder (saving, or choosing a folder)");
            ui.end_row();
            ui.monospace(app.keys_for(&files::CMD_EDIT_PATH).join(", "));
            ui.label("put the folder into the field, as a path to edit");
            ui.end_row();
            ui.monospace(app.keys_for(&files::CMD_TOGGLE_SELECTION).join(", "));
            ui.label("select or unselect the entry (several, or a folder)");
            ui.end_row();
            ui.monospace(app.keys_for(&files::CMD_SELECT_ALL).join(", "));
            ui.label("select every file shown (several)");
            ui.end_row();
        });
}
