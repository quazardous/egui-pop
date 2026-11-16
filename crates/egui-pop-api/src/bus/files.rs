//! Files dropped from another program, on the bus (ADR 0029).
//!
//! Every window's pass (the main window, an application window, a
//! detached panel's window) reads the files the system says over it or
//! dropped on it **once, here**: nothing else in egui-pop reads egui's
//! `hovered_files` or `dropped_files`. Files dropped on a window are its
//! `files.drop` request ([`FilesDrop`], a [`FileDrop`] naming the window),
//! asked at the end of that window's pass; whoever takes them is one of its
//! listeners and says so in the value. Taken, `files.dropped` is said;
//! taken by nobody, `files.refused` (ADR 0003: never lost without a word).
//!
//! Widgets drawn in the pass see the drop through [`system_drag`]. One that
//! takes it ([`take`], the file browser over one of its targets) is said by
//! egui-pop's listener [`BROWSER`], first: an application's listener
//! declared `after` it knows the drop is already taken. While files hover a
//! window and no widget shows where they would go, the window shows
//! whether anything takes files ([`end_pass`]).

use std::path::PathBuf;

use bus_contract::{Bus, ListenOpts, Point, PointKind};
use egui::{Context, Id, Ui};
use serde::Serialize;
use serde_json::json;

use super::{OWNER, emit_with};
use crate::app::AppHandle;
use egui_pop_core::strings;

/// Files dropped on a window: which window, the files, where the pointer
/// was.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct FileDrop {
    /// The window's id in the registry (`main`, an application window's
    /// id, `panel:<id>`).
    pub window: String,
    pub paths: Vec<PathBuf>,
    /// The pointer in the window, in points, when the system says it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<[f32; 2]>,
    /// What widgets of the window's pass took it ([`take`]): said by
    /// [`BROWSER`].
    #[serde(skip)]
    taken: Vec<String>,
}

impl FileDrop {
    pub fn new(window: impl Into<String>, paths: Vec<PathBuf>) -> Self {
        Self {
            window: window.into(),
            paths,
            at: None,
            taken: Vec::new(),
        }
    }

    /// Where the pointer was, in the window's points.
    pub fn at(mut self, at: [f32; 2]) -> Self {
        self.at = Some(at);
        self
    }
}

/// Files are dropped on a window (`files.drop`): each listener that takes
/// them says what it did, one line, in the value. Nobody taking them is a
/// refusal.
#[derive(Debug)]
pub struct FilesDrop;
impl Point for FilesDrop {
    const KIND: PointKind = PointKind::Request;
    const ID: &'static str = "files.drop";
    type Input = FileDrop;
    type Value = Vec<String>;
}

/// The files points, in the order [`super::order_dump`] prints them.
pub const FILES_POINTS: [&str; 1] = [FilesDrop::ID];

/// egui-pop's listener of `files.drop`, first: what the widgets of the
/// window's pass took (the file browser over one of its targets).
pub const BROWSER: &str = "files.browser";

/// What a window's pass knows of the files from another program.
#[derive(Clone, Debug, Default)]
struct Pass {
    /// The window, when its pass began here (else a widget drawn outside
    /// an egui-pop window: it reads egui's input itself).
    window: Option<String>,
    hovered: Vec<PathBuf>,
    /// Files hover, whether or not the system says their paths.
    hovering: bool,
    dropped: Vec<PathBuf>,
    at: Option<egui::Pos2>,
    /// What widgets took the drop.
    taken: Vec<String>,
    /// A widget showed where the hovering files would go.
    shown: bool,
}

fn pass_id(ctx: &Context) -> Id {
    Id::unique(("egui_pop.files.drop", ctx.viewport_id()))
}

/// Reads egui's input: the one place that does.
fn read(ctx: &Context) -> Pass {
    ctx.input(|i| Pass {
        window: None,
        hovered: i
            .raw
            .hovered_files
            .iter()
            .filter_map(|f| f.path.clone())
            .collect(),
        hovering: !i.raw.hovered_files.is_empty(),
        dropped: i
            .raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .filter(|p| !p.as_os_str().is_empty())
            .collect(),
        at: i.pointer.latest_pos(),
        taken: Vec::new(),
        shown: false,
    })
}

/// A window's pass begins: what the system says of files over it, or
/// dropped on it, read once.
pub(crate) fn begin_pass(ctx: &Context, window: &str) {
    let mut pass = read(ctx);
    pass.window = Some(window.to_string());
    let id = pass_id(ctx);
    ctx.data_mut(|d| d.insert_temp(id, pass));
}

/// Files from another program in this pass, for a widget: their paths and
/// whether they are dropped now (else they hover; their paths may be empty
/// while they do, some systems say them only on the drop).
pub(crate) fn system_drag(ctx: &Context) -> Option<(Vec<PathBuf>, bool)> {
    let id = pass_id(ctx);
    let pass = ctx
        .data(|d| d.get_temp::<Pass>(id))
        .filter(|p| p.window.is_some())
        .unwrap_or_else(|| read(ctx));
    if !pass.dropped.is_empty() {
        Some((pass.dropped, true))
    } else if pass.hovering {
        Some((pass.hovered, false))
    } else {
        None
    }
}

/// A widget of this pass took the files dropped: `what` says how, as
/// [`BROWSER`] says it on the request.
pub(crate) fn take(ctx: &Context, what: String) {
    in_pass(ctx, |p| p.taken.push(what));
}

/// A widget of this pass showed where the hovering files would go: the
/// window shows nothing more.
pub(crate) fn shown(ctx: &Context) {
    in_pass(ctx, |p| p.shown = true);
}

/// Changes the window pass's record; outside an egui-pop window's pass
/// (a widget in plain egui) there is none, and nothing to say.
fn in_pass(ctx: &Context, change: impl FnOnce(&mut Pass)) {
    let id = pass_id(ctx);
    ctx.data_mut(|d| {
        if let Some(mut p) = d.get_temp::<Pass>(id).filter(|p| p.window.is_some()) {
            change(&mut p);
            d.insert_temp(id, p);
        }
    });
}

/// A window's pass ends: files dropped on it are its `files.drop`; files
/// hovering it that no widget placed show whether anything takes files.
pub(crate) fn end_pass(app: &AppHandle, ui: &Ui) {
    let ctx = ui.ctx();
    let id = pass_id(ctx);
    let Some(pass) = ctx.data_mut(|d| d.remove_temp::<Pass>(id)) else {
        return;
    };
    let Some(window) = pass.window else { return };
    if !pass.dropped.is_empty() {
        let mut drop = FileDrop::new(window, pass.dropped);
        drop.at = pass.at.map(|p| [p.x, p.y]);
        drop.taken = pass.taken;
        app.request_files_drop(drop);
    } else if pass.hovering && !pass.shown {
        paint_hover(app, ui);
        // egui repaints while files hover; the drop's pass comes.
        ctx.request_repaint();
    }
}

/// Over the whole window: whether its files go somewhere.
fn paint_hover(app: &AppHandle, ui: &Ui) {
    let ctx = ui.ctx();
    let takes = takers(app) > 0;
    let visuals = ui.visuals();
    let color = if takes {
        visuals.selection.stroke.color
    } else {
        visuals.error_fg_color
    };
    let rect = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        Id::unique("egui_pop.files.hover"),
    ));
    painter.rect_stroke(
        rect.shrink(2.0),
        4.0,
        egui::Stroke::new(3.0, color),
        egui::StrokeKind::Inside,
    );
    let text = strings::tr(
        ctx,
        if takes {
            &crate::strings::FILES_DROP_HERE
        } else {
            &crate::strings::FILES_DROP_NOWHERE
        },
    );
    let font = egui::TextStyle::Body.resolve(ui.style());
    let galley = painter.layout_no_wrap(text, font, visuals.strong_text_color());
    let size = galley.size() + egui::vec2(16.0, 8.0);
    let caption = egui::Rect::from_center_size(rect.center(), size);
    painter.rect_filled(caption, 4.0, visuals.window_fill);
    painter.rect_stroke(
        caption,
        4.0,
        egui::Stroke::new(1.0, color),
        egui::StrokeKind::Inside,
    );
    painter.galley(caption.center() - galley.size() / 2.0, galley, color);
    ctx.set_cursor_icon(if takes {
        egui::CursorIcon::Copy
    } else {
        egui::CursorIcon::NoDrop
    });
}

/// The listeners of `files.drop` but egui-pop's own.
fn takers(app: &AppHandle) -> usize {
    app.bus()
        .listeners(FilesDrop::ID)
        .unwrap_or_default()
        .iter()
        .filter(|l| l.id != BROWSER)
        .count()
}

/// Registers egui-pop's listener of `files.drop`.
pub(super) fn install(kernel: &dyn Bus) {
    let listened = kernel.listen::<FilesDrop>(
        BROWSER,
        ListenOpts::new().owner(OWNER).priority(1000),
        |drop, did| {
            for t in &drop.taken {
                did.push(format!("{BROWSER}: {t}"));
            }
            Ok(())
        },
    );
    debug_assert!(listened.is_ok(), "{listened:?}");
}

impl AppHandle {
    /// Asks for files dropped on a window (`files.drop`): its listeners run
    /// now, in their declared order, each that takes them saying so; then
    /// `files.dropped` is said with what they did, or `files.refused` when
    /// none took them or the window is not open. What they did, one line
    /// each.
    ///
    /// Every window's pass asks it for the files the system dropped on it;
    /// an application or a test may ask it too.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn request_files_drop(&self, drop: FileDrop) -> Vec<String> {
        let data = |extra: serde_json::Value| {
            let mut d = json!({ "window": drop.window, "paths": drop.paths });
            if let Some(at) = drop.at {
                d["at"] = json!(at);
            }
            if let (Some(d), Some(extra)) = (d.as_object_mut(), extra.as_object()) {
                d.extend(extra.clone());
            }
            d
        };
        let open = self.windows_list().iter().any(|w| w.id == drop.window);
        let did = if open {
            emit_with::<FilesDrop>(self, drop.clone(), Vec::new())
        } else {
            Vec::new()
        };
        if did.is_empty() {
            let reason = if open {
                "nothing takes files dropped on this window"
            } else {
                "no open window of this id"
            };
            log::warn!(
                target: "egui_pop",
                "{} files dropped on `{}` refused: {reason}",
                drop.paths.len(),
                drop.window
            );
            self.publish(
                "egui_pop",
                None,
                "files.refused",
                data(json!({ "request": FilesDrop::ID, "reason": reason })),
            );
        } else {
            self.publish(
                "egui_pop",
                None,
                "files.dropped",
                data(json!({ "did": did })),
            );
        }
        did
    }
}
