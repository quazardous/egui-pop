//! Every page drawn with egui's own fonts: Ubuntu Light and Hack, plus
//! egui-pop's icon font, and no emoji or symbol font (egui 0.37's
//! `monochrome_emoji_fonts` and the like stay off in this build). The
//! harness panics on a character no font has, so a glyph that needs
//! one of those fonts (✕, ★, ⚠, 🔥) fails here rather than drawing a
//! box in an application that leaves them off.

use egui_kittest::Harness;
use egui_pop::core::fonts;
use egui_pop::theme::Density;
use egui_pop_gallery::{Gallery, Options, PAGES};
use egui_pop_test::SharedGpu as _;
use serde_json::json;

/// Pages left out, each for what it shows on purpose.
const SKIPPED: [&str; 1] = [
    // A specimen per script (CJK, Arabic, Vietnamese...): egui's pair
    // has no face for most of them, and the page says so.
    "fonts",
];

fn harness(page: &str) -> Harness<'static, Gallery> {
    let options = Options {
        theme: "pop-dark".into(),
        density: Density::Comfortable,
        page: page.into(),
        ..Options::default()
    };
    Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .with_max_steps(8)
        .shared_wgpu()
        .build_eframe(move |cc| {
            egui_pop::core::motion::freeze(&cc.egui_ctx, 0.0);
            let gallery = Gallery::from_creation_context(cc, options).expect("the gallery builds");
            // egui's bundled faces by name, in place of egui-pop's pair.
            gallery.app.handle().theme().update(|spec| {
                spec.overrides
                    .insert("font.family.ui".into(), json!(["Ubuntu"]));
                spec.overrides
                    .insert("font.family.mono".into(), json!(["Hack"]));
            });
            gallery
        })
}

#[test]
fn every_page_draws_with_eguis_own_fonts() {
    for page in PAGES.iter().filter(|p| !SKIPPED.contains(p)) {
        let mut h = harness(page);
        let _ = h.run_ok();
        let (ui, mono) = fonts::installed_families(&h.ctx).expect("fonts installed");
        assert_eq!((ui, mono), (vec!["Ubuntu".into()], vec!["Hack".into()]));
        // A page whose content keeps asking for frames (tiles, a
        // preview) gets a few more.
        for _ in 0..4 {
            let _ = h.run_ok();
        }
    }
}
