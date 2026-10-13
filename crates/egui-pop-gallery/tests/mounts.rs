//! Mount tests (ADR 0013): every component's gallery story mounted in a
//! mock host built from each host's offer that accepts it (nothing
//! faults, every interactive node has a name, no diagnostic), and every
//! refused mount reported with its rule, nothing mounted. These are the
//! evidence `docs/hosts.md` cites for its `yes` and `if` cells.

use std::rc::Rc;

use egui_kittest::Harness;
use egui_pop::api::AppHandle;
use egui_pop::model::ids::StoryId;
use egui_pop::model::mount::{self, Fit};
use egui_pop_gallery::{Gallery, Options};
use egui_pop_test::hosts::{Draw, MockHost};

/// Each component's fixture: the gallery story that draws it.
const FIXTURES: &[(&str, &str)] = &[
    ("viewport", "viewport"),
    ("navigator", "navigator"),
    ("node-graph", "node-graph"),
    ("curves", "grading"),
    ("wheels", "grading"),
    ("histogram", "images"),
    ("thumbnail", "images"),
    ("library-grid", "library"),
    ("filmstrip", "library"),
    ("compare", "library"),
    ("tree", "tree"),
    ("table", "table"),
    ("text-editor", "text-editor"),
    ("code-editor", "code-editor"),
    ("terminal", "terminal"),
    ("sliders", "sliders"),
    ("forms", "values"),
    ("progress", "progress"),
    ("jobs-centre", "jobs"),
    ("theme-editor", "theme-editor"),
    ("tips", "tips"),
];

struct Scene {
    gallery: Gallery,
    host: Option<MockHost>,
}

fn harness() -> Harness<'static, Scene> {
    let ctx = egui::Context::default();
    // The overview behind: no component of the matrix on it.
    let gallery = Gallery::new(&ctx, Options::default()).expect("the gallery builds");
    Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .build_ui_state(
            |ui, s: &mut Scene| {
                s.gallery.show(ui);
                let app = s.gallery.app.handle().clone();
                if let Some(host) = &mut s.host {
                    host.show(ui.ctx(), &app);
                }
            },
            Scene {
                gallery,
                host: None,
            },
        )
}

fn app(h: &Harness<'_, Scene>) -> AppHandle {
    h.state().gallery.app.handle().clone()
}

fn settle(h: &mut Harness<'_, Scene>) {
    for _ in 0..6 {
        let _ = h.run_ok();
    }
}

#[test]
fn every_component_has_a_fixture() {
    for m in egui_pop::mounts::components() {
        assert!(
            FIXTURES.iter().any(|(c, _)| *c == m.id),
            "no fixture for `{}`",
            m.id
        );
    }
    for (_, story) in FIXTURES {
        assert!(
            egui_pop_gallery::PAGES.contains(story),
            "`{story}` is not a gallery page"
        );
    }
}

#[test]
fn every_component_mounts_in_every_host_that_accepts_it() {
    let hosts = egui_pop::mounts::hosts();
    for m in egui_pop::mounts::components() {
        let story = FIXTURES
            .iter()
            .find(|(c, _)| *c == m.id)
            .map(|(_, s)| StoryId::new(*s).unwrap())
            .expect("a fixture");
        let mut h = harness();
        settle(&mut h);
        for offer in hosts.iter().filter(|o| MockHost::supported(o)) {
            let fit = mount::check(&m, offer);
            if !fit.fits() {
                continue;
            }
            let where_ = format!("{} in {}", m.id, offer.id);
            let app = app(&h);
            assert_eq!(app.mount(&m.id, &offer.id).as_ref(), Ok(&fit), "{where_}");
            let shown = story.clone();
            let draw: Draw = Rc::new(move |ui: &mut egui::Ui, app: &AppHandle| {
                app.show_story(ui, &shown);
            });
            h.state_mut().host = Some(MockHost::new(offer.clone(), draw));
            settle(&mut h);
            let faults = egui_pop::core::fault::faults(&h.ctx);
            assert!(faults.is_empty(), "{where_}: {faults:?}");
            assert!(
                app.diagnostics().is_empty(),
                "{where_}: {:?}",
                app.diagnostics()
            );
            let unnamed = egui_pop_test::unnamed_interactive(&h);
            assert!(unnamed.is_empty(), "{where_}: unnamed {unnamed:?}");
            if let Some(mut host) = h.state_mut().host.take() {
                host.close(&app);
            }
            assert!(app.unmount(&m.id, &offer.id), "{where_}");
            settle(&mut h);
        }
    }
}

#[test]
fn every_refused_mount_is_reported_with_its_rule_and_mounts_nothing() {
    let h = harness();
    let app = app(&h);
    let mut refused = 0;
    for m in egui_pop::mounts::components() {
        for offer in egui_pop::mounts::hosts() {
            let Fit::No { refusal, .. } = mount::check(&m, &offer) else {
                continue;
            };
            refused += 1;
            let before = app.diagnostics().len();
            let why = app.mount(&m.id, &offer.id).unwrap_err();
            assert!(why.contains(refusal.rule.number()), "{why}");
            assert!(why.contains(refusal.code), "{why}");
            let d = app.diagnostics();
            assert_eq!(d.len(), before + 1, "{} in {}", m.id, offer.id);
            assert_eq!(d[before].code, "mount-refused");
            assert!(app.mounts().is_empty());
        }
    }
    assert!(refused > 0);
}

#[test]
fn agents_ask_where_a_component_can_go() {
    let mut h = harness();
    settle(&mut h);
    let f = h.state_mut().gallery.app.debug_call(
        "mounts",
        "where_can",
        &serde_json::json!({ "component": "terminal" }),
    );
    assert!(f.ok, "{}", f.message);
    let verdicts = f.data.as_array().unwrap();
    let popover = verdicts.iter().find(|v| v["host"] == "popover").unwrap();
    assert_eq!(popover["fit"]["fit"], "no");
    assert_eq!(popover["fit"]["refusal"]["code"], "D3");
    let custom = verdicts
        .iter()
        .find(|v| v["host"] == "dialog.custom")
        .unwrap();
    assert_eq!(custom["fit"]["fit"], "under");
}
