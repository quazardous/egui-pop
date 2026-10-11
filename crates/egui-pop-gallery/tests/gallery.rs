//! The gallery renders every page under every theme, and its pages are the
//! snapshot fixtures (`tests/snapshots/`, thresholds in `kittest.toml`).
//! Regenerate with `UPDATE_SNAPSHOTS=1 cargo test -p egui-pop-gallery`.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop::theme::{BUILT_IN_THEMES, Density, Tokens};
use egui_pop_gallery::{Gallery, Options, Page};

fn harness(options: Options) -> Harness<'static, Gallery> {
    Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .wgpu()
        .build_eframe(move |cc| Gallery::new(&cc.egui_ctx, options))
}

fn options(theme: &str, density: Density, page: Page) -> Options {
    Options {
        theme: theme.to_string(),
        density,
        page,
        ..Options::default()
    }
}

#[test]
fn every_page_renders_under_every_theme_and_density() {
    for theme in BUILT_IN_THEMES {
        for density in Density::ALL {
            for page in Page::ALL {
                let mut h = harness(options(theme, density, page));
                h.run();
                // In the sidebar and as the page's heading.
                let titles = h.query_all_by_label(page.title()).count();
                assert_eq!(titles, 2, "{theme} {page:?}");
                let tokens = Tokens::of(&h.ctx);
                assert!(tokens.resolved().problems().is_empty());
                assert_eq!(tokens.resolved().spec().base, *theme);
                h.render().expect("render");
            }
        }
    }
}

#[test]
fn snapshots_per_theme() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in BUILT_IN_THEMES {
        for page in [Page::Overview, Page::Builtins] {
            let mut h = harness(options(theme, Density::Comfortable, page));
            h.run();
            results.add(h.try_snapshot(format!("{}-{theme}", page.name())));
        }
    }
    let mut h = harness(options("pop-dark", Density::Compact, Page::Builtins));
    h.run();
    results.add(h.try_snapshot("builtins-pop-dark-compact"));
    let mut h = harness(options("pop-dark", Density::Comfortable, Page::Tokens));
    h.run();
    results.add(h.try_snapshot("tokens-pop-dark"));
}

#[test]
fn switching_theme_and_page_from_the_ui() {
    let mut h = harness(Options::default());
    h.run();
    h.get_by_label(Page::Tokens.title()).click();
    h.run();
    assert_eq!(h.state().page, Page::Tokens);
    // The switchers change the spec; the theme is installed between frames.
    h.state_mut().spec.base = "pop-light".into();
    h.run();
    let tokens = Tokens::of(&h.ctx);
    assert_eq!(tokens.resolved().spec().base, "pop-light");
    assert!(!h.ctx.global_style().visuals.dark_mode);
}

#[test]
fn quit_after_closes_the_window() {
    let mut h = harness(Options {
        quit_after: Some(3),
        ..Options::default()
    });
    for _ in 0..3 {
        h.step();
    }
    let commands = &h.output().viewport_output[&egui::ViewportId::ROOT].commands;
    assert!(
        commands.contains(&egui::ViewportCommand::Close),
        "{commands:?}"
    );
}
