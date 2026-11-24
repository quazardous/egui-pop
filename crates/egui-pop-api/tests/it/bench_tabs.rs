//! Frame time of the workbench with many tabs: the interface built and
//! tessellated (no GPU), the pointer sweeping the window so tabs change
//! state. Ignored by default; run in release:
//!
//! ```text
//! cargo test --release -p egui-pop-api --test bench_tabs -- --ignored --nocapture
//! ```

use std::time::Instant;

use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};

const PER_AREA: usize = 24;

struct Many;

impl Plugin for Many {
    fn id(&self) -> PluginId {
        PluginId::from_static("bench")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        for (area, name) in [
            (Area::Bottom, "bottom"),
            (Area::StartSide, "start"),
            (Area::EndSide, "end"),
        ] {
            for i in 0..PER_AREA {
                let id = PanelId::new(format!("bench.{name}-{i}")).expect("a valid id");
                ctx.panel(
                    PanelSpec::new(id, &format!("Panel {name} {i}"), area),
                    |ui, _| {
                        ui.label("body");
                    },
                )?;
            }
        }
        Ok(())
    }
}

fn frame(ctx: &egui::Context, app: &mut App, pos: egui::Pos2) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1600.0, 900.0),
        )),
        events: vec![egui::Event::PointerMoved(pos)],
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| app.show(ui));
    let prims = ctx.tessellate(std::mem::take(&mut out.shapes), out.pixels_per_point);
    std::hint::black_box(prims);
    out.textures_delta.clear();
}

#[test]
#[ignore = "a benchmark: run in release with --ignored --nocapture"]
fn workbench_frame_time_with_many_tabs() {
    let ctx = egui::Context::default();
    let mut app = App::builder()
        .plugin(Many)
        .build(&ctx)
        .expect("the app builds");
    let pos = |i: usize| egui::pos2((i * 37 % 1600) as f32, (i * 53 % 900) as f32);
    for i in 0..100 {
        frame(&ctx, &mut app, pos(i));
    }
    let mut times = Vec::new();
    for i in 0..2000 {
        let t = Instant::now();
        frame(&ctx, &mut app, pos(i));
        times.push(t.elapsed().as_secs_f64() * 1e6);
    }
    // Every panel is open: its tab is drawn in its area's tab bar.
    let handle = app.handle().clone();
    for name in ["bottom", "start", "end"] {
        let id = PanelId::new(format!("bench.{name}-{}", PER_AREA - 1)).expect("a valid id");
        assert!(handle.panel_visible(&id), "{id:?} is not shown");
    }
    times.sort_by(f64::total_cmp);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    println!(
        "frame: mean {mean:.0} us, median {:.0} us, p95 {:.0} us ({} tabs)",
        times[times.len() / 2],
        times[times.len() * 95 / 100],
        3 * PER_AREA
    );
}
