//! Command-line options, for scripted runs (tests, headless compositors).

use egui_pop::theme::{Density, FALLBACK_THEME};

use crate::PAGES;

/// The usage text printed by `--help`.
pub const USAGE: &str = "\
egui-pop-gallery: every egui-pop component in every variant and state

Options:
  --theme <name>         base theme (pop-dark, pop-light, pop-high-contrast)
  --density <name>       comfortable or compact
  --accent <scale>       accent colour scale (gold, blue, green, amber, red)
  --reduced-motion       turn animations off
  --page <name>          page shown first (overview, tokens, builtins,
                         icons, menus, zones, settings, i18n,
                         platform, debug)
  --lang <locale>        the language setting's default: en (the
                         default), fr, or system (EGUI_POP_LANG, then
                         the system's language)
  --platform             the desktop's own services: a single instance,
                         notifications, a tray icon, global shortcuts,
                         logs and crash reports in the user's state
                         directory (simulated otherwise)
  --size <W>x<H>         the window's inner size in points (default:
                         1100x720)
  --wide-gamut           a transparent window whose image viewports get
                         a surface of their own on Wayland, BT.2020 with
                         a 2.2 power wanted (see the display page)
  --quit-after <frames>  exit by itself after this many frames
  --dump-debug           with --quit-after: print the debug bridge's
                         snapshot (event \"debug\") before quitting
  --help                 this text

At start-up one JSON line is printed on stdout (event \"ready\"), and one
more when quitting by --quit-after (event \"quit\").";

/// The window's inner size when `--size` is not given, in points.
pub const DEFAULT_SIZE: [f32; 2] = [1100.0, 720.0];

/// How the gallery starts.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub theme: String,
    pub density: Density,
    pub accent: Option<String>,
    pub reduced_motion: bool,
    pub page: String,
    /// The language setting's default (`en`, `fr`, `system`).
    pub lang: String,
    /// The window's inner size in points.
    pub size: [f32; 2],
    /// Close the window after this many frames.
    pub quit_after: Option<u64>,
    /// Print the JSON start and quit lines on stdout.
    pub announce: bool,
    /// Print the debug bridge's snapshot when quitting.
    pub dump_debug: bool,
    /// The desktop's own platform services instead of simulated ones.
    pub platform: bool,
    /// A transparent window, image surfaces installed and BT.2020 with a
    /// 2.2 power wanted.
    pub wide_gamut: bool,
    pub help: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            theme: FALLBACK_THEME.to_string(),
            density: Density::default(),
            accent: None,
            reduced_motion: false,
            page: "overview".into(),
            lang: "en".into(),
            size: DEFAULT_SIZE,
            quit_after: None,
            announce: false,
            dump_debug: false,
            platform: false,
            wide_gamut: false,
            help: false,
        }
    }
}

impl Options {
    /// Reads the arguments (without the program name).
    pub fn parse<I, S>(args: I) -> Result<Options, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut o = Options::default();
        let mut args = args.into_iter().map(Into::into);
        while let Some(arg) = args.next() {
            let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
            match arg.as_str() {
                "--theme" => o.theme = value("--theme")?,
                "--density" => o.density = value("--density")?.parse()?,
                "--accent" => o.accent = Some(value("--accent")?),
                "--reduced-motion" => o.reduced_motion = true,
                "--page" => {
                    let page = value("--page")?;
                    if !PAGES.contains(&page.as_str()) {
                        return Err(format!("`{page}` is not a page ({})", PAGES.join(", ")));
                    }
                    o.page = page;
                }
                "--lang" => o.lang = value("--lang")?,
                "--size" => o.size = parse_size(&value("--size")?)?,
                "--dump-debug" => o.dump_debug = true,
                "--platform" => o.platform = true,
                "--wide-gamut" => o.wide_gamut = true,
                "--quit-after" => {
                    let v = value("--quit-after")?;
                    o.quit_after = Some(
                        v.parse()
                            .map_err(|_| format!("--quit-after: `{v}` is not a frame count"))?,
                    );
                }
                "--help" | "-h" => o.help = true,
                other => return Err(format!("unknown option `{other}` (see --help)")),
            }
        }
        Ok(o)
    }
}

/// `<W>x<H>`, both positive, in points.
fn parse_size(v: &str) -> Result<[f32; 2], String> {
    let bad = || format!("--size: `{v}` is not <width>x<height>");
    let (w, h) = v.split_once('x').ok_or_else(bad)?;
    let w: u32 = w.parse().map_err(|_| bad())?;
    let h: u32 = h.parse().map_err(|_| bad())?;
    if w == 0 || h == 0 {
        return Err(bad());
    }
    Ok([w as f32, h as f32])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_parse() {
        let o = Options::parse([
            "--theme",
            "pop-light",
            "--density",
            "compact",
            "--page",
            "tokens",
            "--accent",
            "blue",
            "--quit-after",
            "30",
            "--reduced-motion",
            "--lang",
            "fr",
            "--size",
            "1280x800",
            "--wide-gamut",
        ])
        .unwrap();
        assert_eq!(o.theme, "pop-light");
        assert_eq!(o.density, Density::Compact);
        assert_eq!(o.page, "tokens");
        assert_eq!(o.accent.as_deref(), Some("blue"));
        assert_eq!(o.quit_after, Some(30));
        assert!(o.reduced_motion);
        assert_eq!(o.lang, "fr");
        assert_eq!(o.size, [1280.0, 800.0]);
        assert!(o.wide_gamut);
        assert_eq!(
            Options::parse(Vec::<String>::new()).unwrap(),
            Options::default()
        );
    }

    #[test]
    fn bad_options_say_why() {
        assert!(
            Options::parse(["--density", "roomy"])
                .unwrap_err()
                .contains("roomy")
        );
        assert!(
            Options::parse(["--page"])
                .unwrap_err()
                .contains("needs a value")
        );
        assert!(Options::parse(["--quit-after", "x"]).is_err());
        for size in ["1280", "0x800", "1280x", "wide"] {
            assert!(Options::parse(["--size", size]).unwrap_err().contains(size));
        }
        assert!(Options::parse(["--bogus"]).unwrap_err().contains("--bogus"));
        assert!(
            Options::parse(["--page", "nope"])
                .unwrap_err()
                .contains("debug")
        );
    }
}
