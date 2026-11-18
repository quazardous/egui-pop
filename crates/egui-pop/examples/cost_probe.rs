//! The application `tools/cost/cost.py` measures: what a build of
//! egui-pop costs (build time, dependencies, binary size) and how long it
//! takes to its first frame, per feature set. It builds with any set of
//! the facade's features, `--no-default-features` included.
//!
//! `cost_probe --headless [minimal|default]` builds the application over
//! `MinimalPlugins` or `DefaultPlugins` on an `egui::Context` of its own,
//! draws and tessellates its first two frames at 1280x800 without a
//! window, prints one JSON line of timings and exits. Without
//! `--headless`, with the `eframe` feature, it opens its window as an
//! application would (so the binary's size counts eframe and its
//! backends); never run so by automation.

use std::time::Instant;

use egui_pop::egui;
use egui_pop::model::panels::{Area, PanelSpec};
use egui_pop::prelude::*;

/// The probe's one plugin: a panel in the centre, as small as an
/// application gets.
#[derive(Debug)]
struct Probe;

impl Plugin for Probe {
    fn id(&self) -> PluginId {
        PluginId::from_static("probe")
    }

    fn title(&self) -> String {
        "Probe".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(
            PanelSpec::new(PanelId::from_static("probe.panel"), "Probe", Area::Center),
            |ui, _app| {
                ui.heading("Probe");
                ui.label("The first frame of an egui-pop application.");
            },
        )?;
        Ok(())
    }
}

fn builder(plugins: &str) -> AppBuilder {
    let b = App::builder();
    let b = match plugins {
        "minimal" => b.plugins(MinimalPlugins),
        _ => b.plugins(DefaultPlugins),
    };
    b.plugin(Probe)
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// One frame drawn and tessellated, as a host would before presenting it.
fn frame(ctx: &egui::Context, app: &mut App) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| app.show_wrapped(ui));
    let primitives = ctx.tessellate(out.shapes, out.pixels_per_point);
    std::hint::black_box(primitives);
}

fn headless(start: Instant, plugins: &str) {
    let ctx = egui::Context::default();
    let t = Instant::now();
    let mut app = match builder(plugins).build(&ctx) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("the application does not build: {e}");
            std::process::exit(1);
        }
    };
    let build_ms = ms(t);
    let t = Instant::now();
    frame(&ctx, &mut app);
    let first_frame_ms = ms(t);
    let to_first_frame_ms = ms(start);
    let t = Instant::now();
    frame(&ctx, &mut app);
    let second_frame_ms = ms(t);
    println!(
        "{{\"plugins\":\"{plugins}\",\"build_ms\":{build_ms:.3},\"first_frame_ms\":{first_frame_ms:.3},\"to_first_frame_ms\":{to_first_frame_ms:.3},\"second_frame_ms\":{second_frame_ms:.3}}}"
    );
}

fn main() {
    let start = Instant::now();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let plugins = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .map_or("default", String::as_str);
    if args.iter().any(|a| a == "--headless") {
        headless(start, plugins);
        return;
    }
    #[cfg(feature = "eframe")]
    if let Err(e) = builder(plugins).run_native("Probe", egui_pop::eframe::NativeOptions::default())
    {
        eprintln!("{e}");
        std::process::exit(1);
    }
    #[cfg(not(feature = "eframe"))]
    {
        eprintln!("without the eframe feature the probe runs only with --headless");
        std::process::exit(2);
    }
}
