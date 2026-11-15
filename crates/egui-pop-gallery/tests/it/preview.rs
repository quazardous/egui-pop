//! The preview dialog (ADR 0013): while it is modal, the viewport
//! behind it keeps receiving and drawing tiles from its workers, as the
//! preview's own viewport does from its; a global binding is refused
//! under the modal; Escape cancels it.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use egui_kittest::Harness;
use egui_pop::api::dialogs::DIALOGS;
use egui_pop::api::zones::{self, Block};
use egui_pop::api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop::imaging::preview::{self, Preview, PreviewState};
use egui_pop::model::commands::CommandSpec;
use egui_pop::model::ids::{BlockId, CommandId, PluginId};
use egui_pop::model::keymap::Verdict;
use egui_pop::viewport::demo::Procedural;
use egui_pop::viewport::source::{TileRequest, TileSource};
use egui_pop::viewport::{View, Viewport, ViewportState};
use serde_json::json;

const IMAGE: [u32; 2] = [2048, 1536];
const EXPORT: CommandId = CommandId::from_static("doc.export");
const PING: CommandId = CommandId::from_static("doc.ping");

/// A procedural source shared with the test, which waits on it.
struct Shared(Arc<Procedural>);

impl TileSource for Shared {
    fn size(&self) -> Option<[u32; 2]> {
        self.0.size()
    }

    fn request(&self, requests: Vec<TileRequest>) {
        self.0.request(requests);
    }

    fn readout(&self, pixel: [i64; 2]) {
        self.0.readout(pixel);
    }
}

/// The viewport behind the dialog.
#[derive(Default)]
struct Behind {
    view: View,
    state: ViewportState,
    source: Option<Arc<Procedural>>,
}

struct Doc(Rc<RefCell<Behind>>, Rc<RefCell<u32>>);

impl Plugin for Doc {
    fn id(&self) -> PluginId {
        PluginId::from_static("doc")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(CommandSpec::new(EXPORT, "Export", "Doc"), |_| {
            Ok(json!(null))
        })?;
        let pings = self.1.clone();
        ctx.command(
            CommandSpec::new(PING, "Ping", "Doc").key("Ctrl+Alt+J"),
            move |_| {
                *pings.borrow_mut() += 1;
                Ok(json!(null))
            },
        )?;
        let behind = self.0.clone();
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("doc.image"), 0, move |ui, _| {
                let mut b = behind.borrow_mut();
                let Behind {
                    view,
                    state,
                    source,
                } = &mut *b;
                let shared = Shared(source.clone().expect("made before"));
                let _ = Viewport::new("behind")
                    .source(&shared)
                    .label("Image behind")
                    .show(ui, state, view);
            }),
        );
        Ok(())
    }
}

struct Scene {
    h: Harness<'static, App>,
    behind: Rc<RefCell<Behind>>,
    behind_source: Arc<Procedural>,
    pings: Rc<RefCell<u32>>,
}

fn scene() -> Scene {
    let behind = Rc::new(RefCell::new(Behind::default()));
    let mut state = ViewportState::new();
    state.set_settle(0.0);
    let source = Arc::new(Procedural::new(state.sink(), IMAGE, 2));
    behind.borrow_mut().state = state;
    behind.borrow_mut().source = Some(source.clone());
    let pings = Rc::new(RefCell::new(0));
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(Doc(behind.clone(), pings.clone()))
        .build(&egui::Context::default())
        .unwrap();
    let h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    Scene {
        h,
        behind,
        behind_source: source,
        pings,
    }
}

/// Runs until both viewports have every tile on screen, waiting on the
/// sources' own signal (never sleeping).
fn settle(s: &mut Scene, preview: &Rc<RefCell<PreviewState>>, preview_source: &Procedural) {
    for _ in 0..60 {
        let _ = s.h.run_ok();
        let done =
            s.behind.borrow().state.tiles_complete() && preview.borrow().state.tiles_complete();
        if done {
            return;
        }
        s.behind_source.wait_idle();
        preview_source.wait_idle();
    }
    panic!("the tiles never completed");
}

#[test]
fn the_viewport_behind_keeps_its_workers_while_the_preview_is_modal() {
    let mut s = scene();
    let _ = s.h.run_ok();
    let app = s.h.state().handle().clone();
    let mut kept = None;
    let (_, preview) = preview::open(
        &app,
        Preview::new("Export preview", "Export", EXPORT),
        |sink| {
            let source = Arc::new(Procedural::new(sink, IMAGE, 2));
            kept = Some(source.clone());
            Box::new(Shared(source))
        },
    )
    .unwrap();
    let preview_source = kept.expect("made by open");
    preview.borrow_mut().state.set_settle(0.0);
    settle(&mut s, &preview, &preview_source);
    assert!(egui_pop::core::overlay::modal_open(&s.h.ctx));
    let (served, _) = s.behind_source.counts();
    let (preview_served, _) = preview_source.counts();
    assert!(served > 0 && preview_served > 0);
    // The image behind changes while the dialog is open: its workers
    // deliver the new tiles and the viewport behind draws them.
    s.behind_source.set_shift(0.5);
    let _ = s.h.run_ok();
    s.behind_source.wait_idle();
    settle(&mut s, &preview, &preview_source);
    assert!(s.behind_source.counts().0 > served, "new tiles behind");
    assert!(egui_pop::core::overlay::modal_open(&s.h.ctx));
    let dialogs = app.service(&DIALOGS).unwrap();
    assert_eq!(dialogs.len(), 1, "still open");
    // A global binding is refused under the modal.
    s.h.key_press_modifiers(
        egui::Modifiers::COMMAND | egui::Modifiers::ALT,
        egui::Key::J,
    );
    let _ = s.h.run_ok();
    assert_eq!(*s.pings.borrow(), 0);
    let blocked = app.key_traces().iter().any(|t| {
        t.candidates
            .iter()
            .any(|c| c.verdict == Verdict::BlockedByModal)
    });
    assert!(blocked, "{:?}", app.key_traces());
    // The preview describes its viewport to the debug bridge.
    let current = dialogs.describe();
    assert_eq!(current["kind"], "custom");
    assert_eq!(current["body"]["complete"], true);
    // Escape cancels it; nothing is exported.
    s.h.key_press(egui::Key::Escape);
    let _ = s.h.run_ok();
    assert!(dialogs.is_empty());
    assert!(!egui_pop::core::overlay::modal_open(&s.h.ctx));
}
