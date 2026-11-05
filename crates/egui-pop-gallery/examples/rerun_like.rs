//! A Rerun-like viewer on egui-pop (`docs/rerun-like.md`): the Rerun
//! viewer's look and layout, from egui-pop's workbench, kit and
//! components, over generated data.
//!
//! ```sh
//! cargo run -p egui-pop-gallery --example rerun_like
//! ```

use egui_pop::eframe;
use egui_pop_gallery::rerun_like::{self, RerunLike};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui_pop::egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    rerun_like::builder(RerunLike::default()).run_native_with(
        "Rerun-like viewer",
        options,
        |app, _| {
            rerun_like::start(&app);
            Ok(Box::new(app))
        },
    )
}
