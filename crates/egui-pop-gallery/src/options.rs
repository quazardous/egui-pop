//! Command-line options, for scripted runs (tests, headless compositors).

use egui_pop::theme::{Density, FALLBACK_THEME};

use crate::Page;

/// The usage text printed by `--help`.
pub const USAGE: &str = "\
egui-pop-gallery: every egui-pop component in every variant and state

Options:
  --theme <name>         base theme (pop-dark, pop-light, pop-high-contrast)
  --density <name>       comfortable or compact
  --accent <scale>       accent colour scale (gold, blue, green, amber, red)
  --reduced-motion       turn animations off
  --page <name>          page shown first (overview, tokens, builtins)
  --quit-after <frames>  exit by itself after this many frames
  --help                 this text

At start-up one JSON line is printed on stdout (event \"ready\"), and one
more when quitting by --quit-after (event \"quit\").";

/// How the gallery starts.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub theme: String,
    pub density: Density,
    pub accent: Option<String>,
    pub reduced_motion: bool,
    pub page: Page,
    /// Close the window after this many frames.
    pub quit_after: Option<u64>,
    /// Print the JSON start and quit lines on stdout.
    pub announce: bool,
    pub help: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            theme: FALLBACK_THEME.to_string(),
            density: Density::default(),
            accent: None,
            reduced_motion: false,
            page: Page::default(),
            quit_after: None,
            announce: false,
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
                "--page" => o.page = value("--page")?.parse()?,
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
        ])
        .unwrap();
        assert_eq!(o.theme, "pop-light");
        assert_eq!(o.density, Density::Compact);
        assert_eq!(o.page, Page::Tokens);
        assert_eq!(o.accent.as_deref(), Some("blue"));
        assert_eq!(o.quit_after, Some(30));
        assert!(o.reduced_motion);
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
        assert!(Options::parse(["--bogus"]).unwrap_err().contains("--bogus"));
    }
}
