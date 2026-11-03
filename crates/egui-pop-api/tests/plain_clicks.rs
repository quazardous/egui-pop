//! Enter and Space on a focused plain button: alone and once, they click
//! it; with a modifier or repeated while held, they do not, unless a binding
//! takes them.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_core::theme::ThemeSpec;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{BlockId, CommandId, PluginId};
use egui_pop_test::walk::{self, Press, TAB};
use serde_json::Value as Json;

#[derive(Default)]
struct Counts {
    clicks: AtomicUsize,
    bound: AtomicUsize,
}

struct Demo(Arc<Counts>);

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let counts = self.0.clone();
        ctx.command(
            CommandSpec::new(CommandId::from_static("demo.bound"), "Bound", "Demo")
                .key("Ctrl+Space"),
            move |_| {
                counts.bound.fetch_add(1, Ordering::Relaxed);
                Ok(Json::Null)
            },
        )?;
        let counts = self.0.clone();
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("demo.main"), 0, move |ui, _| {
                if ui.button("Go").clicked() {
                    counts.clicks.fetch_add(1, Ordering::Relaxed);
                }
            }),
        );
        Ok(())
    }
}

fn harness() -> (Harness<'static, App>, Arc<Counts>) {
    let counts = Arc::new(Counts::default());
    let app = App::builder()
        .theme(ThemeSpec::new("pop-dark"))
        .plugins(DefaultPlugins)
        .plugin(Demo(counts.clone()))
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    walk::reach(&mut h, TAB, "Go", 60).expect("Tab reaches the button");
    (h, counts)
}

fn key(h: &mut Harness<'static, App>, key: Key, modifiers: Modifiers, repeat: bool) {
    h.event(egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat,
        modifiers,
    });
    let _ = h.run_ok();
}

fn release(h: &mut Harness<'static, App>, key: Key) {
    h.event(egui::Event::Key {
        key,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    let _ = h.run_ok();
}

fn clicks(c: &Counts) -> usize {
    c.clicks.load(Ordering::Relaxed)
}

#[test]
fn enter_and_space_alone_click_a_plain_button() {
    let (mut h, counts) = harness();
    walk::press(&mut h, Press::key(Key::Enter));
    assert_eq!(clicks(&counts), 1);
    walk::press(&mut h, Press::key(Key::Space));
    assert_eq!(clicks(&counts), 2);
}

#[test]
fn enter_or_space_with_a_modifier_does_not_click_it() {
    let (mut h, counts) = harness();
    for m in [Modifiers::CTRL, Modifiers::ALT, Modifiers::SHIFT] {
        for k in [Key::Enter, Key::Space] {
            key(&mut h, k, m, false);
            release(&mut h, k);
        }
    }
    assert_eq!(clicks(&counts), 0);
}

#[test]
fn a_held_space_clicks_once() {
    let (mut h, counts) = harness();
    for repeat in [false, true, true, true] {
        key(&mut h, Key::Space, Modifiers::NONE, repeat);
    }
    release(&mut h, Key::Space);
    assert_eq!(clicks(&counts), 1);
}

#[test]
fn a_binding_on_the_key_still_runs() {
    let (mut h, counts) = harness();
    key(&mut h, Key::Space, Modifiers::CTRL, false);
    release(&mut h, Key::Space);
    assert_eq!(counts.bound.load(Ordering::Relaxed), 1);
    assert_eq!(clicks(&counts), 0);
}
