//! A preview dialog: an image viewport in a custom dialog (ADR 0013),
//! the first use of `Dialog::Custom`. An export or a crop shows its
//! effect before it runs: the viewport draws from a tile source of its
//! own, the interface behind stays as it is (a `Clear` backdrop) and
//! keeps drawing, its own viewport included, while the dialog is modal.
//!
//! The dialog keeps every dialog rule: the primary button says what it
//! does ("Export"), Enter runs it, Escape cancels it, except during a
//! drag of the viewport (Escape then cancels the drag: the viewport
//! claims Escape). It is movable and comes back where it was left. The
//! viewport draws sRGB first (decided 2026-10-14): two overlapping
//! wide-gamut image surfaces have not been tried.
//!
//! ```no_run
//! use egui_pop_api::AppHandle;
//! use egui_pop_imaging::preview::{self, Preview};
//! use egui_pop_model::ids::CommandId;
//! use egui_pop_viewport::demo::Procedural;
//!
//! fn export(app: &AppHandle) {
//!     let preview = Preview::new("Export preview", "Export", CommandId::from_static("doc.export"));
//!     let opened = preview::open(app, preview, |sink| Box::new(Procedural::new(sink, [4096, 3072], 2)));
//!     if let Err(why) = opened {
//!         eprintln!("no preview: {why}");
//!     }
//! }
//! ```

use std::cell::RefCell;
use std::rc::Rc;

use egui_pop_api::AppHandle;
use egui_pop_api::dialogs::{Backdrop, Custom, DIALOGS, DialogSpec};
use egui_pop_model::ids::CommandId;
use egui_pop_viewport::source::{TileSink, TileSource};
use egui_pop_viewport::{View, Viewport, ViewportState};
use serde_json::{Value as Json, json};

/// A preview dialog, as declared.
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    pub title: String,
    /// The primary button: what accepting does ("Export 3 images").
    pub verb: String,
    pub command: CommandId,
    /// Arguments the view is merged into (`view`: the zoom and centre
    /// the preview was left at).
    pub args: Json,
    /// The share of the window the viewport takes.
    pub share: f32,
    /// The key its place is kept under.
    pub remember: String,
}

impl Preview {
    /// A preview whose primary button says what it does and runs
    /// `command`; 70 % of the window, kept under `export.preview`.
    pub fn new(title: &str, verb: &str, command: CommandId) -> Self {
        Self {
            title: title.into(),
            verb: verb.into(),
            command,
            args: Json::Null,
            share: Custom::SHARE,
            remember: "export.preview".into(),
        }
    }

    pub fn args(mut self, args: Json) -> Self {
        self.args = args;
        self
    }

    /// The share of the window the viewport takes.
    pub fn share(mut self, share: f32) -> Self {
        self.share = share;
        self
    }

    /// The key its place is kept under.
    pub fn remember(mut self, key: &str) -> Self {
        self.remember = key.into();
        self
    }
}

/// The preview's viewport: its view, its state, its source.
pub struct PreviewState {
    pub view: View,
    pub state: ViewportState,
    pub source: Box<dyn TileSource>,
}

impl std::fmt::Debug for PreviewState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreviewState")
            .field("view", &self.view)
            .field("complete", &self.state.tiles_complete())
            .finish_non_exhaustive()
    }
}

/// Opens a preview: `source` makes the tile source from the viewport's
/// sink. Returns the dialog's id and the viewport's state, shared with
/// the dialog's body; an error when the dialogs plugin is missing or
/// the viewport does not fit a custom dialog.
pub fn open(
    app: &AppHandle,
    preview: Preview,
    source: impl FnOnce(TileSink) -> Box<dyn TileSource>,
) -> Result<(u64, Rc<RefCell<PreviewState>>), String> {
    let dialogs = app
        .service(&DIALOGS)
        .ok_or("the dialogs plugin is not connected")?;
    let state = ViewportState::new();
    let source = source(state.sink());
    let shared = Rc::new(RefCell::new(PreviewState {
        view: View::fit(),
        state,
        source,
    }));
    let custom = Custom::new(&preview.title, &preview.verb, preview.command.clone())
        .args(preview.args.clone())
        .share(preview.share)
        .needs(egui_pop_viewport::mounts::viewport());
    let spec = DialogSpec::new(custom)
        .backdrop(Backdrop::Clear)
        .movable(true)
        .remember_position(&preview.remember);
    let body = shared.clone();
    let name = format!("preview.{}", preview.remember);
    let id = dialogs.open_custom(app, spec, move |ui, _, pass| {
        let mut p = body.borrow_mut();
        let PreviewState {
            view,
            state,
            source,
        } = &mut *p;
        // A drag of the view: this Escape cancels it, not the dialog.
        if state.in_gesture() {
            pass.claim_escape();
        }
        let _ = Viewport::new(name.as_str())
            .source(source.as_ref())
            .show(ui, state, view);
        let center = [view.center[0], view.center[1]];
        pass.value("view", json!({ "center": center }));
        pass.describe(json!({
            "center": center,
            "complete": state.tiles_complete(),
            "outstanding": state.outstanding(),
        }));
    })?;
    Ok((id, shared))
}
