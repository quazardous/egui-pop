//! The gallery's scenarios (`tests/scenarios/*.ron`), replayed headless
//! with the invariants checked after every step; the broken ones
//! (`tests/scenarios/broken/`) fail on purpose, with a clear report.

use egui_pop_gallery::Options;
use egui_pop_test::scenario::Runner;

fn runner() -> Runner {
    Runner::new(|| {
        egui_pop_gallery::builder(&Options {
            no_persistence: true,
            ..Options::default()
        })
    })
}

fn dir(sub: &str) -> String {
    format!("{}/tests/scenarios{sub}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn scenario_the_gallery_ones_pass() {
    let passed = runner().assert_dir(dir(""));
    let names: Vec<&str> = passed.iter().map(|p| p.scenario.as_str()).collect();
    assert_eq!(names, ["about", "describe", "toasts"]);
    // Without a renderer a shot keeps the accessibility tree.
    assert!(
        passed[0]
            .trees
            .iter()
            .any(|(n, t)| n == "about-settled" && !t.is_empty())
    );
}

#[test]
fn scenario_a_failing_command_breaks_its_step_with_a_report() {
    let f = runner()
        .run_file(dir("/broken/unknown-page.ron"))
        .expect_err("the page does not exist");
    let report = f.to_string();
    eprintln!("{report}");
    assert_eq!(f.step, Some(2), "{report}");
    assert_eq!(f.broken, "invariant NoFailedCommand", "{report}");
    assert!(f.message.contains("no-such-page"), "{report}");
    assert!(report.contains("step 2: Call(\"gallery.page\""), "{report}");
    assert!(
        f.journal.iter().any(|e| e.contains("command.failed")),
        "{report}"
    );
    assert!(!f.tree.is_empty(), "{report}");
}

#[test]
fn scenario_an_unknown_capability_breaks_no_unknown_command() {
    let f = runner()
        .run_file(dir("/broken/unknown-command.ron"))
        .expect_err("nobody registered it");
    assert_eq!(f.step, Some(1), "{f}");
    assert_eq!(f.broken, "invariant NoUnknownCommand", "{f}");
    assert!(f.message.contains("gallery.does.not.exist"), "{f}");
}
