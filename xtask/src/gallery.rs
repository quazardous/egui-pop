//! `cargo xtask gallery`: runs the gallery with the optional media
//! features the system can build.
//!
//! Cargo features are static, so the gallery's `video` (GStreamer) and
//! `webview` (WebKitGTK) pages stay compiled out unless asked for. This
//! launcher asks pkg-config for the libraries their `*-sys` crates link
//! (the names in their `system-deps` metadata), turns on each feature
//! whose libraries are all there, says why the others are off and what
//! to install, then runs `cargo run -p egui-pop-gallery` with them.
//!
//! With `--features inspection` it also sets `EGUI_INSPECTION=1` for the
//! gallery when the environment does not set it, so that eframe opens the
//! `egui_inspection` endpoint `egui-mcp` attaches to (docs/inspection.md).

use std::process::{Command, ExitCode, Stdio};

pub const USAGE: &str = "\
cargo xtask gallery [--release] [--no-detect] [--check] [--features LIST] [-- <gallery args>]
cargo gallery ...       (the same, through the alias)

Runs the gallery with the optional media features the system can build:
`video` (GStreamer) and `webview` (WebKitGTK, Linux only), found through
pkg-config. Prints one line per feature, on or off with the reason and
the packages to install.

  --release              build in release mode
  --no-detect            probe nothing: only the features given
  --check                print the decision and the command, run nothing
  --features LIST        more features, comma or space separated, kept
                         whatever the probe says (repeatable);
                         `inspection` opens the egui_inspection endpoint
                         for egui-mcp (EGUI_INSPECTION=1 unless set; no
                         authentication, see docs/inspection.md);
                         `native-dialogs` builds the system's file
                         chooser, a fallback used only with
                         `-- --system-file-chooser`
  -- <args>              passed to the gallery (`-- --help` lists them)";

/// What the launcher asks of the system, behind a trait so the tests
/// can fake it.
pub trait Probe {
    /// Whether pkg-config itself runs.
    fn has_pkg_config(&self) -> bool;
    /// Whether pkg-config knows `module` (`pkg-config --exists`).
    fn exists(&self, module: &str) -> bool;
}

/// The real system, through `$PKG_CONFIG` or `pkg-config`, as the
/// `*-sys` crates' build scripts find it.
pub struct PkgConfig {
    program: String,
}

impl PkgConfig {
    pub fn new() -> Self {
        let program = std::env::var("PKG_CONFIG").unwrap_or_else(|_| "pkg-config".into());
        Self { program }
    }

    fn status(&self, args: &[&str]) -> Option<bool> {
        Command::new(&self.program)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .ok()
            .map(|s| s.success())
    }
}

impl Probe for PkgConfig {
    fn has_pkg_config(&self) -> bool {
        self.status(&["--version"]).is_some()
    }

    fn exists(&self, module: &str) -> bool {
        self.status(&["--exists", module]).unwrap_or(false)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Linux,
    Mac,
    Windows,
    Other,
}

impl Os {
    pub fn current() -> Self {
        match std::env::consts::OS {
            "linux" => Os::Linux,
            "macos" => Os::Mac,
            "windows" => Os::Windows,
            _ => Os::Other,
        }
    }
}

/// A pkg-config module and the development packages that ship it.
struct Module {
    pc: &'static str,
    fedora: &'static str,
    debian: &'static str,
}

/// An optional media feature of the gallery.
struct Media {
    name: &'static str,
    /// What the feature shows, for the printed line.
    what: &'static str,
    /// The modules its `*-sys` crates link.
    modules: &'static [Module],
    /// Whether the feature does anything on that system; `None` when it
    /// does, else why not.
    unsupported: fn(Os) -> Option<&'static str>,
    /// The install hint off Linux.
    elsewhere: &'static str,
}

/// gstreamer-rs 0.25: gstreamer-sys, gstreamer-base-sys (pulled by
/// gstreamer-app), gstreamer-app-sys and gstreamer-video-sys.
const GSTREAMER: &[Module] = &[
    Module {
        pc: "gstreamer-1.0",
        fedora: "gstreamer1-devel",
        debian: "libgstreamer1.0-dev",
    },
    Module {
        pc: "gstreamer-base-1.0",
        fedora: "gstreamer1-devel",
        debian: "libgstreamer1.0-dev",
    },
    Module {
        pc: "gstreamer-app-1.0",
        fedora: "gstreamer1-plugins-base-devel",
        debian: "libgstreamer-plugins-base1.0-dev",
    },
    Module {
        pc: "gstreamer-video-1.0",
        fedora: "gstreamer1-plugins-base-devel",
        debian: "libgstreamer-plugins-base1.0-dev",
    },
];

/// wry 0.57 on Linux: webkit2gtk-sys 2.0 and gtk-sys 0.18 (WebKitGTK's
/// development package pulls GTK's, libsoup's and JavaScriptCore's).
const WEBKIT: &[Module] = &[
    Module {
        pc: "webkit2gtk-4.1",
        fedora: "webkit2gtk4.1-devel",
        debian: "libwebkit2gtk-4.1-dev",
    },
    Module {
        pc: "gtk+-3.0",
        fedora: "gtk3-devel",
        debian: "libgtk-3-dev",
    },
];

const MEDIA: &[Media] = &[
    Media {
        name: "video",
        what: "the viewer's MP4 page, GStreamer",
        modules: GSTREAMER,
        unsupported: |_| None,
        elsewhere: "install GStreamer with its development files and pkg-config \
                    (gstreamer.freedesktop.org; on macOS `brew install gstreamer pkgconf`)",
    },
    Media {
        name: "webview",
        what: "the viewer's YouTube page, WebKitGTK",
        modules: WEBKIT,
        unsupported: |os| (os != Os::Linux).then_some("the web view is Linux-only for now"),
        elsewhere: "",
    },
];

/// Whether a feature is on, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Found on the system.
    Found,
    /// Given with `--features`.
    Asked,
    /// Off, with the reason and, when there is one, what to install.
    Off {
        reason: String,
        hint: Option<String>,
    },
}

/// The launcher's decision: a line per media feature, and the features
/// passed to cargo.
#[derive(Debug, PartialEq, Eq)]
pub struct Decision {
    pub lines: Vec<(&'static str, &'static str, Verdict)>,
    pub features: Vec<String>,
}

impl Decision {
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (name, what, verdict) in &self.lines {
            let line = match verdict {
                Verdict::Found => format!("{name}: on ({what})"),
                Verdict::Asked => format!("{name}: on (--features)"),
                Verdict::Off { reason, hint } => match hint {
                    Some(h) => format!("{name}: off, {reason}; {h}"),
                    None => format!("{name}: off, {reason}"),
                },
            };
            out.push_str(&line);
            out.push('\n');
        }
        out
    }
}

/// Decides the features: the explicit ones always, plus, when `detect`,
/// each media feature whose modules are all found.
pub fn decide(os: Os, probe: &dyn Probe, explicit: &[String], detect: bool) -> Decision {
    let mut features: Vec<String> = Vec::new();
    for f in explicit {
        if !features.contains(f) {
            features.push(f.clone());
        }
    }
    let mut lines = Vec::new();
    let pkg_config = detect && probe.has_pkg_config();
    for media in MEDIA {
        let verdict = if explicit.iter().any(|f| f == media.name) {
            Verdict::Asked
        } else if !detect {
            Verdict::Off {
                reason: "not detected (--no-detect)".into(),
                hint: Some(format!("add --features {}", media.name)),
            }
        } else if let Some(why) = (media.unsupported)(os) {
            Verdict::Off {
                reason: why.into(),
                hint: None,
            }
        } else if !pkg_config {
            Verdict::Off {
                reason: "pkg-config not found".into(),
                hint: Some(match os {
                    Os::Linux => {
                        "install pkgconf-pkg-config (Fedora) or pkg-config (Debian/Ubuntu)".into()
                    }
                    _ => media.elsewhere.into(),
                }),
            }
        } else {
            let missing: Vec<&Module> = media
                .modules
                .iter()
                .filter(|m| !probe.exists(m.pc))
                .collect();
            if missing.is_empty() {
                features.push(media.name.into());
                Verdict::Found
            } else {
                let names: Vec<&str> = missing.iter().map(|m| m.pc).collect();
                Verdict::Off {
                    reason: format!("pkg-config lacks {}", names.join(", ")),
                    hint: Some(match os {
                        Os::Linux => linux_hint(&missing),
                        _ => media.elsewhere.into(),
                    }),
                }
            }
        };
        lines.push((media.name, media.what, verdict));
    }
    Decision { lines, features }
}

/// The packages to install for the missing modules, each once.
fn linux_hint(missing: &[&Module]) -> String {
    let mut fedora: Vec<&str> = Vec::new();
    let mut debian: Vec<&str> = Vec::new();
    for m in missing {
        if !fedora.contains(&m.fedora) {
            fedora.push(m.fedora);
        }
        if !debian.contains(&m.debian) {
            debian.push(m.debian);
        }
    }
    format!(
        "install {} (Fedora) or {} (Debian/Ubuntu)",
        fedora.join(" "),
        debian.join(" ")
    )
}

/// The environment variable eframe's `inspection` reads
/// (`egui_inspection::INSPECTION_ENV_VAR`).
pub const INSPECTION_ENV: &str = "EGUI_INSPECTION";

/// With the `inspection` feature: the line printed about the endpoint
/// and the value to give [`INSPECTION_ENV`] (`None` when the
/// environment already sets it, and the gallery reads that). `None`
/// without the feature.
pub fn inspection(
    features: &[String],
    env: Option<&str>,
) -> Option<(String, Option<&'static str>)> {
    if !features.iter().any(|f| f == "inspection") {
        return None;
    }
    const WARNING: &str = "no authentication, any local process can drive the gallery";
    Some(match env {
        None => (
            format!("inspection: on, 127.0.0.1:5719 ({INSPECTION_ENV}=1); {WARNING}"),
            Some("1"),
        ),
        Some(value) => (
            format!("inspection: on, {INSPECTION_ENV}={value} from the environment; {WARNING}"),
            None,
        ),
    })
}

/// The parsed command line.
#[derive(Debug, Default, PartialEq, Eq)]
struct Args {
    release: bool,
    detect: bool,
    check: bool,
    features: Vec<String>,
    rest: Vec<String>,
}

fn parse(args: &[String]) -> Result<Option<Args>, String> {
    let mut out = Args {
        detect: true,
        ..Args::default()
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--release" => out.release = true,
            "--no-detect" => out.detect = false,
            "--check" | "--no-run" => out.check = true,
            "--features" | "-F" => {
                let list = it.next().ok_or("--features needs a list")?;
                push_features(&mut out.features, list);
            }
            "-h" | "--help" => return Ok(None),
            "--" => {
                out.rest = it.cloned().collect();
                break;
            }
            a if a.starts_with("--features=") => {
                push_features(&mut out.features, &a["--features=".len()..]);
            }
            a => return Err(format!("unknown argument {a:?}\n\n{USAGE}")),
        }
    }
    Ok(Some(out))
}

fn push_features(into: &mut Vec<String>, list: &str) {
    into.extend(
        list.split([',', ' '])
            .filter(|f| !f.is_empty())
            .map(String::from),
    );
}

/// `cargo run`'s arguments for a decision.
fn cargo_args(args: &Args, features: &[String]) -> Vec<String> {
    let mut out: Vec<String> = ["run", "-p", "egui-pop-gallery"].map(String::from).into();
    if args.release {
        out.push("--release".into());
    }
    if !features.is_empty() {
        out.push("--features".into());
        out.push(features.join(","));
    }
    if !args.rest.is_empty() {
        out.push("--".into());
        out.extend(args.rest.iter().cloned());
    }
    out
}

/// Runs the task; the exit code is the gallery's.
pub fn gallery(args: &[String]) -> Result<ExitCode, String> {
    let Some(args) = parse(args)? else {
        println!("{USAGE}");
        return Ok(ExitCode::SUCCESS);
    };
    let decision = decide(
        Os::current(),
        &PkgConfig::new(),
        &args.features,
        args.detect,
    );
    print!("{}", decision.render());
    let env = std::env::var(INSPECTION_ENV).ok();
    let inspection = inspection(&decision.features, env.as_deref());
    if let Some((line, _)) = &inspection {
        println!("{line}");
    }
    let cargo_args = cargo_args(&args, &decision.features);
    println!("cargo {}", cargo_args.join(" "));
    if args.check {
        return Ok(ExitCode::SUCCESS);
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut command = Command::new(cargo);
    if let Some((_, Some(value))) = inspection {
        command.env(INSPECTION_ENV, value);
    }
    let status = command
        .args(&cargo_args)
        .current_dir(crate::root())
        .status()
        .map_err(|e| format!("cargo: {e}"))?;
    Ok(match status.code() {
        Some(0) => ExitCode::SUCCESS,
        Some(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        None => ExitCode::FAILURE,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A system with pkg-config (or not) and the given modules.
    struct Fake {
        pkg_config: bool,
        modules: &'static [&'static str],
    }

    impl Probe for Fake {
        fn has_pkg_config(&self) -> bool {
            self.pkg_config
        }
        fn exists(&self, module: &str) -> bool {
            self.modules.contains(&module)
        }
    }

    const ALL: &[&str] = &[
        "gstreamer-1.0",
        "gstreamer-base-1.0",
        "gstreamer-app-1.0",
        "gstreamer-video-1.0",
        "webkit2gtk-4.1",
        "gtk+-3.0",
    ];

    fn fake(modules: &'static [&'static str]) -> Fake {
        Fake {
            pkg_config: true,
            modules,
        }
    }

    fn verdict<'a>(d: &'a Decision, name: &str) -> &'a Verdict {
        &d.lines.iter().find(|l| l.0 == name).unwrap().2
    }

    #[test]
    fn everything_found_turns_both_on() {
        let d = decide(Os::Linux, &fake(ALL), &[], true);
        assert_eq!(d.features, ["video", "webview"]);
        assert_eq!(verdict(&d, "video"), &Verdict::Found);
    }

    #[test]
    fn nothing_found_turns_both_off() {
        let d = decide(Os::Linux, &fake(&[]), &[], true);
        assert!(d.features.is_empty());
        assert!(matches!(verdict(&d, "webview"), Verdict::Off { .. }));
    }

    #[test]
    fn one_missing_module_turns_the_feature_off() {
        let d = decide(
            Os::Linux,
            &fake(&["gstreamer-1.0", "gstreamer-base-1.0", "gstreamer-app-1.0"]),
            &[],
            true,
        );
        assert!(d.features.is_empty());
        let Verdict::Off { reason, hint } = verdict(&d, "video") else {
            panic!("video should be off");
        };
        assert!(reason.contains("gstreamer-video-1.0"), "{reason}");
        let hint = hint.as_deref().unwrap();
        assert!(hint.contains("gstreamer1-plugins-base-devel"), "{hint}");
        assert!(hint.contains("libgstreamer-plugins-base1.0-dev"), "{hint}");
        assert!(
            !hint.contains("gstreamer1-devel"),
            "only the missing: {hint}"
        );
        assert!(
            !hint.contains("libgstreamer1.0-dev"),
            "only the missing: {hint}"
        );
    }

    #[test]
    fn the_printed_hint_names_the_packages() {
        let out = decide(Os::Linux, &fake(&[]), &[], true).render();
        for package in [
            "gstreamer1-devel",
            "gstreamer1-plugins-base-devel",
            "libgstreamer1.0-dev",
            "libgstreamer-plugins-base1.0-dev",
            "webkit2gtk4.1-devel",
            "libwebkit2gtk-4.1-dev",
        ] {
            assert!(out.contains(package), "{package} missing from:\n{out}");
        }
        assert_eq!(out.lines().count(), 2, "{out}");
        assert!(out.lines().all(|l| l.contains(": off, ")), "{out}");
        // Each package once, even when two modules share it.
        assert_eq!(out.matches("gstreamer1-devel").count(), 1, "{out}");
    }

    #[test]
    fn without_pkg_config_says_so() {
        let probe = Fake {
            pkg_config: false,
            modules: ALL,
        };
        let d = decide(Os::Linux, &probe, &[], true);
        assert!(d.features.is_empty());
        let out = d.render();
        assert!(out.contains("pkg-config not found"), "{out}");
        assert!(out.contains("pkgconf-pkg-config"), "{out}");
    }

    #[test]
    fn webview_is_off_away_from_linux() {
        for os in [Os::Mac, Os::Windows, Os::Other] {
            let d = decide(os, &fake(ALL), &[], true);
            assert_eq!(d.features, ["video"], "{os:?}");
            assert!(
                d.render()
                    .contains("webview: off, the web view is Linux-only")
            );
        }
    }

    #[test]
    fn video_off_away_from_linux_hints_at_gstreamer() {
        let out = decide(Os::Mac, &fake(&[]), &[], true).render();
        assert!(out.contains("brew install gstreamer"), "{out}");
    }

    #[test]
    fn no_detect_keeps_only_the_explicit_features() {
        let explicit = vec!["video".to_string(), "trace".to_string()];
        let d = decide(Os::Linux, &fake(ALL), &explicit, false);
        assert_eq!(d.features, ["video", "trace"]);
        assert_eq!(verdict(&d, "video"), &Verdict::Asked);
        assert!(matches!(verdict(&d, "webview"), Verdict::Off { .. }));
    }

    #[test]
    fn explicit_features_win_over_the_probe() {
        let explicit = vec!["webview".to_string()];
        let d = decide(Os::Linux, &fake(&[]), &explicit, true);
        assert_eq!(d.features, ["webview"]);
        assert_eq!(verdict(&d, "webview"), &Verdict::Asked);
    }

    #[test]
    fn explicit_and_found_are_not_repeated() {
        let explicit = vec!["video".to_string(), "video".to_string()];
        let d = decide(Os::Linux, &fake(ALL), &explicit, true);
        assert_eq!(d.features, ["video", "webview"]);
    }

    #[test]
    fn inspection_sets_the_variable_unless_the_environment_does() {
        assert_eq!(inspection(&strings(&["video"]), None), None);
        let (line, value) = inspection(&strings(&["inspection"]), None).unwrap();
        assert_eq!(value, Some("1"));
        assert!(line.contains("127.0.0.1:5719"), "{line}");
        assert!(line.contains("no authentication"), "{line}");
        let (line, value) = inspection(&strings(&["inspection"]), Some("127.0.0.1:5800")).unwrap();
        assert_eq!(value, None, "the environment's value is kept");
        assert!(line.contains("EGUI_INSPECTION=127.0.0.1:5800"), "{line}");
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_flags_features_and_the_rest() {
        let a = parse(&strings(&[
            "--release",
            "--features",
            "trace,native-dialogs",
            "--features=video",
            "--check",
            "--",
            "--page",
            "viewer",
        ]))
        .unwrap()
        .unwrap();
        assert!(a.release && a.detect && a.check);
        assert_eq!(a.features, ["trace", "native-dialogs", "video"]);
        assert_eq!(a.rest, ["--page", "viewer"]);
        assert!(parse(&strings(&["--bogus"])).is_err());
        assert!(parse(&strings(&["--help"])).unwrap().is_none());
    }

    #[test]
    fn the_cargo_line_carries_the_features_and_the_rest() {
        let a = parse(&strings(&["--release", "--", "--theme", "pop-light"]))
            .unwrap()
            .unwrap();
        let line = cargo_args(&a, &strings(&["video", "webview"])).join(" ");
        assert_eq!(
            line,
            "run -p egui-pop-gallery --release --features video,webview -- --theme pop-light"
        );
        let plain = parse(&[]).unwrap().unwrap();
        assert_eq!(cargo_args(&plain, &[]).join(" "), "run -p egui-pop-gallery");
    }
}
