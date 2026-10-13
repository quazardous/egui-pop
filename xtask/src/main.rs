//! Repository tasks, run as `cargo xtask <task>`.
//!
//! `screenshots [filter]` refreshes every image of the documentation:
//! it builds the gallery in release mode, captures each shot headless
//! through wbox (`tools/wbox/shoot.py`, one nested compositor per shot),
//! crops each capture to the window and writes `docs/screenshots/`, the
//! README's thumbnails (`thumbs/`), the themes gallery (`themes/`, at
//! full size in a larger window) and the editor-inspired themes' sheet
//! (`themes.png`).

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead as _, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use ab_glyph::{Font as _, FontRef, PxScale, ScaleFont as _};
use image::imageops::{self, FilterType as Resize};
use image::{Rgb, RgbImage, RgbaImage};
use serde_json::json;

const USAGE: &str = "\
cargo xtask <task>

Tasks:
  screenshots [filter]   refresh the documentation's images, headless
                         through wbox; with a filter, only the shots whose
                         name contains it (`--list` prints the names)

Environment:
  WBOX_PYTHON            the Python with the wbox library (default: the
                         interpreter of `wbox-mcp` found on PATH)";

/// The README's thumbnails: 400 px wide, shown at 200 so they stay
/// crisp on a high-density screen.
const THUMB_WIDTH: u32 = 400;

/// The compositor's output and the gallery's window (inner size, in
/// points; one point is one pixel) for most shots: the coordinates of
/// their steps are measured in it.
const SCREEN: (i32, i32) = (1280, 800);
const WINDOW: (i32, i32) = (1100, 720);

/// The themes gallery's window, larger so its shots are read at full
/// size, never reduced; its steps need none of the measured coordinates.
const THEME_SCREEN: (i32, i32) = (1440, 900);
const THEME_WINDOW: (i32, i32) = (1280, 800);

/// What the compositor adds around the window's inner size: the border,
/// and the title bar above.
const DECORATION: (i32, i32) = (2, 28);

/// A cell of the themes sheet, as `docs/tokens.md` shows it.
const SHEET_CELL: u32 = 360;
const SHEET_GAP: u32 = 8;
const SHEET_LABEL: u32 = 22;
const SHEET_COLUMNS: u32 = 4;

/// Where the focusing click goes: the inspector's empty area, inert on
/// every page (a click on the desktop would open the compositor's
/// menu), this far from the window's bottom right corner. In the
/// default 1280x800 output, at 1070,700.
const FOCUS_FROM_CORNER: (i32, i32) = (121, 74);

/// The focusing click, for a window of `window` points centred in a
/// `screen` output.
fn focus(screen: (i32, i32), window: (i32, i32)) -> Step {
    let right = (screen.0 + window.0 + DECORATION.0) / 2;
    let bottom = (screen.1 + window.1 + DECORATION.1) / 2;
    Step::Click(right - FOCUS_FROM_CORNER.0, bottom - FOCUS_FROM_CORNER.1)
}

/// Where the pointer rests while the shot is taken: on the desktop, off
/// the window, so nothing is hovered. grim leaves the cursor out.
fn away(screen: (i32, i32)) -> Step {
    Step::Move(screen.0 - 5, screen.1 - 5)
}

/// One input played before the capture, in output coordinates (the
/// window is centred: its top-left corner, title bar included, is at
/// 89,26).
#[derive(Clone, Copy, Debug)]
enum Step {
    Click(i32, i32),
    Move(i32, i32),
    Key(&'static str),
    Wait(f32),
}

impl Step {
    fn to_json(self) -> serde_json::Value {
        match self {
            Step::Click(x, y) => json!({ "click": [x, y] }),
            Step::Move(x, y) => json!({ "move": [x, y] }),
            Step::Key(k) => json!({ "key": k }),
            Step::Wait(s) => json!({ "wait": s }),
        }
    }
}

#[derive(Clone, Debug)]
struct Shot {
    name: String,
    page: &'static str,
    theme: String,
    density: &'static str,
    steps: Vec<Step>,
    /// `docs/screenshots/<name>.png` is followed by its thumbnail.
    thumb: bool,
    screen: (i32, i32),
    window: (i32, i32),
}

fn shot(name: &str, page: &'static str, theme: &str, steps: &[Step], thumb: bool) -> Shot {
    Shot {
        name: name.into(),
        page,
        theme: theme.into(),
        density: "comfortable",
        steps: steps.to_vec(),
        thumb,
        screen: SCREEN,
        window: WINDOW,
    }
}

/// A shot of the themes gallery: the overview, full size, in a larger
/// window.
fn theme_shot(theme: &str) -> Shot {
    Shot {
        screen: THEME_SCREEN,
        window: THEME_WINDOW,
        ..shot(&format!("themes/{theme}"), "overview", theme, &[], false)
    }
}

/// Every shot of the documentation.
fn shots() -> Vec<Shot> {
    use Step::*;
    let settings_window = [Key("ctrl+comma"), Wait(0.5)];
    let mut all = vec![
        shot("overview-pop-dark", "overview", "pop-dark", &[], true),
        shot("overview-pop-light", "overview", "pop-light", &[], false),
        shot(
            "overview-pop-high-contrast",
            "overview",
            "pop-high-contrast",
            &[],
            false,
        ),
        Shot {
            density: "compact",
            ..shot(
                "builtins-pop-dark-compact",
                "builtins",
                "pop-dark",
                &[],
                false,
            )
        },
        shot(
            "palette-pop-dark",
            "overview",
            "pop-dark",
            &[Key("ctrl+shift+p"), Wait(0.5)],
            true,
        ),
        shot(
            "settings-window-pop-dark",
            "settings",
            "pop-dark",
            &settings_window,
            true,
        ),
        shot(
            "shortcuts-pop-dark",
            "settings",
            "pop-dark",
            &[settings_window[0], settings_window[1], Click(271, 388)],
            true,
        ),
        // "Export settings".
        shot(
            "dialogs-pop-dark",
            "dialogs",
            "pop-dark",
            &[Click(609, 224), Wait(0.5)],
            true,
        ),
        // Warning, Error, then "Success with an action": the success
        // toast lasts 4 s, the capture comes before.
        shot(
            "toasts-pop-light",
            "toasts",
            "pop-light",
            &[Click(564, 209), Click(623, 209), Click(456, 209)],
            false,
        ),
        shot("values-pop-dark", "values", "pop-dark", &[], false),
        shot("zones-pop-dark", "zones", "pop-dark", &[], true),
        // The extender plugin switched off.
        shot(
            "zones-pop-light-off",
            "zones",
            "pop-light",
            &[Key("ctrl+e"), Wait(0.5)],
            false,
        ),
        // Its tiles come from a worker: let them in.
        shot(
            "viewport-pop-dark",
            "viewport",
            "pop-dark",
            &[Wait(2.0)],
            true,
        ),
        // Two key resolutions for the bridge to show.
        shot(
            "debug-pop-dark",
            "debug",
            "pop-dark",
            &[Key("alt+up"), Wait(0.5), Key("alt+down"), Wait(0.5)],
            true,
        ),
        // "Open the demonstration panels".
        shot(
            "workbench-pop-dark",
            "workbench",
            "pop-dark",
            &[Click(396, 172), Wait(0.5)],
            false,
        ),
    ];
    all.extend(
        egui_pop_tokens::BUILT_IN_THEMES
            .iter()
            .map(|t| theme_shot(t)),
    );
    all
}

/// The editor-inspired themes, the ones `themes.png` shows.
fn sheet_themes() -> Vec<&'static str> {
    egui_pop_tokens::BUILT_IN_THEMES
        .iter()
        .copied()
        .filter(|t| !egui_pop_tokens::POP_THEMES.contains(t))
        .collect()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("screenshots") => screenshots(&args[1..]),
        Some("-h" | "--help") => {
            println!("{USAGE}");
            Ok(())
        }
        _ => Err(format!("unknown or missing task\n\n{USAGE}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits in the workspace")
        .to_path_buf()
}

fn screenshots(args: &[String]) -> Result<(), String> {
    let mut filter = None;
    let mut list = false;
    for arg in args {
        match arg.as_str() {
            "--list" => list = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            a if a.starts_with('-') => return Err(format!("unknown option `{a}`")),
            a if filter.is_none() => filter = Some(a.to_string()),
            a => return Err(format!("one filter only (`{a}`)")),
        }
    }
    let selected: Vec<Shot> = shots()
        .into_iter()
        .filter(|s| filter.as_deref().is_none_or(|f| s.name.contains(f)))
        .collect();
    if list {
        for s in &selected {
            println!("{}", s.name);
        }
        return Ok(());
    }
    if selected.is_empty() {
        return Err(format!(
            "no shot matches `{}` (see --list)",
            filter.unwrap_or_default()
        ));
    }

    let root = root();
    let python = wbox_python()?;
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    eprintln!("building the gallery (release)");
    run(Command::new(&cargo).current_dir(&root).args([
        "build",
        "--release",
        "-p",
        "egui-pop-gallery",
    ]))?;

    let work = root.join("target/screenshots");
    let raw = work.join("raw");
    fs::create_dir_all(&raw).map_err(|e| format!("{}: {e}", raw.display()))?;
    let plan = json!({
        "config": root.join("tools/wbox/config.yaml"),
        "settle": 1.5,
        "shots": selected.iter().map(|s| {
            let mut steps = vec![focus(s.screen, s.window).to_json()];
            steps.extend(s.steps.iter().map(|st| st.to_json()));
            steps.push(away(s.screen).to_json());
            json!({
                "name": s.name,
                "env": {
                    "GALLERY_THEME": s.theme,
                    "GALLERY_DENSITY": s.density,
                    "GALLERY_PAGE": s.page,
                    "GALLERY_SIZE": format!("{}x{}", s.window.0, s.window.1),
                    "GALLERY_REDUCED_MOTION": "1",
                },
                "steps": steps,
                "screen": format!("{}x{}", s.screen.0, s.screen.1),
                "out": raw.join(format!("{}.png", s.name.replace('/', "-"))),
            })
        }).collect::<Vec<_>>(),
    });
    let plan_path = work.join("plan.json");
    fs::write(&plan_path, plan.to_string()).map_err(|e| e.to_string())?;

    eprintln!("capturing {} shots through wbox", selected.len());
    let mut child = Command::new(&python)
        .arg(root.join("tools/wbox/shoot.py"))
        .arg(&plan_path)
        .current_dir(&root)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{}: {e}", python.display()))?;
    let mut failed = Vec::new();
    let docs = root.join("docs/screenshots");
    let mut written = Vec::new();
    let mut sheet_dirty = false;
    let stdout = child.stdout.take().expect("piped");
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|e| e.to_string())?;
        let report: serde_json::Value =
            serde_json::from_str(&line).map_err(|e| format!("driver said `{line}`: {e}"))?;
        let name = report["name"].as_str().unwrap_or_default();
        if let Some(e) = report["error"].as_str() {
            eprintln!("  {name}: FAILED: {e}");
            failed.push(name.to_string());
            continue;
        }
        let Some(s) = selected.iter().find(|s| s.name == name) else {
            continue;
        };
        let capture = raw.join(format!("{}.png", name.replace('/', "-")));
        let window = crop_to_window(&capture)?;
        written.push(save(&window, &docs.join(format!("{name}.png")))?);
        if s.thumb {
            let small = reduce(&window, THUMB_WIDTH);
            written.push(save(&small, &docs.join(format!("thumbs/{name}.png")))?);
        }
        if sheet_themes().iter().any(|t| name == format!("themes/{t}")) {
            sheet_dirty = true;
        }
        eprintln!("  {name}: done");
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if sheet_dirty {
        written.push(sheet(&docs)?);
    }

    let mut total = 0;
    for (path, size) in &written {
        total += size;
        println!(
            "{:>7.1} KB  {}",
            *size as f64 / 1024.0,
            path.strip_prefix(&root).unwrap_or(path).display()
        );
    }
    println!(
        "{} files, {:.1} KB written; docs/screenshots holds {:.1} KB",
        written.len(),
        total as f64 / 1024.0,
        dir_size(&docs) as f64 / 1024.0
    );
    if !failed.is_empty() || !status.success() {
        return Err(format!("failed shots: {}", failed.join(", ")));
    }
    Ok(())
}

/// The Python that imports wbox: `WBOX_PYTHON`, else the interpreter
/// of the `wbox-mcp` launcher found on PATH.
fn wbox_python() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("WBOX_PYTHON") {
        return Ok(p.into());
    }
    let launcher = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|d| d.join("wbox-mcp"))
        .find(|p| p.is_file())
        .ok_or("wbox not found: install wbox-mcp, or set WBOX_PYTHON")?;
    let text = fs::read_to_string(&launcher).map_err(|e| format!("{}: {e}", launcher.display()))?;
    text.lines()
        .next()
        .and_then(|l| l.strip_prefix("#!"))
        .map(|l| PathBuf::from(l.split_whitespace().next().unwrap_or_default()))
        .ok_or_else(|| {
            format!(
                "{} has no interpreter line: set WBOX_PYTHON",
                launcher.display()
            )
        })
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let status = cmd.status().map_err(|e| format!("{cmd:?}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{cmd:?} failed: {status}"))
    }
}

/// The capture cropped to the window, title bar included: the bounding
/// box of what differs from the desktop's colour (its top-left pixel).
fn crop_to_window(path: &Path) -> Result<RgbImage, String> {
    let img: RgbaImage = image::open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .to_rgba8();
    let desktop = *img.get_pixel(0, 0);
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in img.enumerate_pixels() {
        if *p != desktop {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    if x0 > x1 {
        return Err(format!("{}: no window in the capture", path.display()));
    }
    let window = imageops::crop_imm(&img, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image();
    Ok(image::DynamicImage::ImageRgba8(window).to_rgb8())
}

fn reduce(img: &RgbImage, width: u32) -> RgbImage {
    let height = (img.height() as f64 * width as f64 / img.width() as f64).round() as u32;
    imageops::resize(img, width, height, Resize::Lanczos3)
}

/// Above this, a PNG is reduced to 256 colours (only the viewport's
/// gradients come near it).
const PNG_BUDGET: usize = 200 * 1024;

/// Writes a PNG as small as it goes without a visible change: indexed
/// when the image has 256 colours or fewer (lossless), else RGB, else
/// quantised to 256 colours if the RGB file is over [`PNG_BUDGET`].
/// Returns its path and size.
fn save(img: &RgbImage, path: &Path) -> Result<(PathBuf, u64), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let fail = |e: png::EncodingError| format!("{}: {e}", path.display());
    let bytes = match exact_palette(img) {
        Some((palette, indices)) => indexed(img, &palette, &indices).map_err(fail)?,
        None => {
            let rgb = encode(
                img,
                png::ColorType::Rgb,
                None,
                img.as_raw(),
                png::Filter::Adaptive,
            )
            .map_err(fail)?;
            if rgb.len() <= PNG_BUDGET {
                rgb
            } else {
                let rgba: Vec<u8> = img.pixels().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
                let quant = color_quant::NeuQuant::new(10, 256, &rgba);
                let indices: Vec<u8> = rgba
                    .chunks_exact(4)
                    .map(|p| quant.index_of(p) as u8)
                    .collect();
                indexed(img, &quant.color_map_rgb(), &indices).map_err(fail)?
            }
        }
    };
    fs::write(path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((path.to_path_buf(), bytes.len() as u64))
}

/// The image's own colours, if 256 or fewer, and each pixel's index.
fn exact_palette(img: &RgbImage) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut colours: HashMap<[u8; 3], u8> = HashMap::new();
    let mut palette = Vec::new();
    let mut indices = Vec::with_capacity(img.len() / 3);
    for p in img.pixels() {
        let next = colours.len();
        let index = *colours.entry(p.0).or_insert_with(|| {
            palette.extend_from_slice(&p.0);
            next.min(255) as u8
        });
        if colours.len() > 256 {
            return None;
        }
        indices.push(index);
    }
    Some((palette, indices))
}

/// An indexed PNG, with whichever filtering compresses it better.
fn indexed(img: &RgbImage, palette: &[u8], indices: &[u8]) -> Result<Vec<u8>, png::EncodingError> {
    let plain = encode(
        img,
        png::ColorType::Indexed,
        Some(palette),
        indices,
        png::Filter::NoFilter,
    )?;
    let filtered = encode(
        img,
        png::ColorType::Indexed,
        Some(palette),
        indices,
        png::Filter::Adaptive,
    )?;
    Ok(if filtered.len() < plain.len() {
        filtered
    } else {
        plain
    })
}

fn encode(
    img: &RgbImage,
    colour: png::ColorType,
    palette: Option<&[u8]>,
    data: &[u8],
    filter: png::Filter,
) -> Result<Vec<u8>, png::EncodingError> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, img.width(), img.height());
    encoder.set_color(colour);
    encoder.set_depth(png::BitDepth::Eight);
    if let Some(palette) = palette {
        encoder.set_palette(palette);
    }
    encoder.set_deflate_compression(png::DeflateCompression::Level(9));
    encoder.set_filter(filter);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(data)?;
    writer.finish()?;
    Ok(bytes)
}

/// `themes.png`: the editor-inspired themes' overviews reduced, four a
/// row, each named under its picture.
fn sheet(docs: &Path) -> Result<(PathBuf, u64), String> {
    let themes = sheet_themes();
    let cells: Vec<RgbImage> = themes
        .iter()
        .map(|t| {
            let path = docs.join(format!("themes/{t}.png"));
            image::open(&path)
                .map(|i| reduce(&i.to_rgb8(), SHEET_CELL))
                .map_err(|e| format!("{}: {e} (capture `themes` first)", path.display()))
        })
        .collect::<Result<_, _>>()?;
    let cell_h = cells.iter().map(RgbImage::height).max().unwrap_or(0);
    let rows = (cells.len() as u32).div_ceil(SHEET_COLUMNS);
    let width = SHEET_COLUMNS * SHEET_CELL + (SHEET_COLUMNS + 1) * SHEET_GAP;
    let height = SHEET_GAP + rows * (cell_h + SHEET_LABEL);
    let mut out = RgbImage::from_pixel(width, height, Rgb([0x22, 0x22, 0x22]));
    let font = FontRef::try_from_slice(epaint_default_fonts::HACK_REGULAR)
        .map_err(|e| format!("the label font: {e}"))?;
    for (i, (cell, theme)) in cells.iter().zip(&themes).enumerate() {
        let (col, row) = (i as u32 % SHEET_COLUMNS, i as u32 / SHEET_COLUMNS);
        let x = SHEET_GAP + col * (SHEET_CELL + SHEET_GAP);
        let y = SHEET_GAP + row * (cell_h + SHEET_LABEL);
        imageops::replace(&mut out, cell, x.into(), y.into());
        label(&mut out, &font, theme, x, y + cell_h + 2);
    }
    save(&out, &docs.join("themes.png"))
}

/// Draws `text` in light grey, its top at `y`.
fn label(img: &mut RgbImage, font: &FontRef<'_>, text: &str, x: u32, y: u32) {
    let scale = PxScale::from(15.0);
    let scaled = font.as_scaled(scale);
    let mut caret = x as f32;
    let baseline = y as f32 + scaled.ascent();
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        let glyph = id.with_scale_and_position(scale, ab_glyph::point(caret, baseline));
        caret += scaled.h_advance(id);
        let Some(outlined) = font.outline_glyph(glyph) else {
            continue;
        };
        let bounds = outlined.px_bounds();
        outlined.draw(|gx, gy, coverage| {
            let px = bounds.min.x as i64 + gx as i64;
            let py = bounds.min.y as i64 + gy as i64;
            if px < 0 || py < 0 || px >= img.width() as i64 || py >= img.height() as i64 {
                return;
            }
            let p = img.get_pixel_mut(px as u32, py as u32);
            for ch in 0..3 {
                let bg = p.0[ch] as f32;
                p.0[ch] = (bg + (230.0 - bg) * coverage).round() as u8;
            }
        });
    }
}

fn dir_size(dir: &Path) -> u64 {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| {
            let p = e.path();
            if p.is_dir() {
                dir_size(&p)
            } else {
                e.metadata().map(|m| m.len()).unwrap_or(0)
            }
        })
        .sum()
}
