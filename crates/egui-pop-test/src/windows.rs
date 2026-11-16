//! A headless harness for applications with several windows
//! (`AppHandle::open_window`).
//!
//! kittest has one window and embeds the others in it. [`MultiWindow`]
//! runs an egui context with real viewports instead, as eframe does,
//! without a GPU: [`MultiWindow::root_pass`] runs the main window's pass
//! (`App::show`), which declares the application's windows as deferred
//! viewports; [`MultiWindow::window_pass`] then runs one window's callback
//! in a pass of its own, with that window's input. Each window keeps its
//! own queued events, focus, size and close request; the commands each
//! pass sent to the system (`ViewportCommand`) are kept for the test.
//!
//! Immediate viewports (a detached panel's window) are real too: the
//! harness is the context's immediate renderer, so each runs its pass of
//! its own, with its input, inside the main window's pass that shows it,
//! as eframe runs them; [`MultiWindow::root_pass`] takes their output as
//! a window's. A window shown that way is listed from the next pass on,
//! and leaves the list at the first main pass that no longer shows it.
//!
//! It is the backend of the window invariants (ADR 0029): a window the
//! main pass declares is listed in the input of every later pass, as a
//! system window that exists. After each pass, a window invariant broken
//! fails the test (a panic naming the rule and the window), unless the
//! test allows that rule ([`MultiWindow::allow_broken`]) because it
//! breaks it on purpose.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use egui::{
    Context, DeferredViewportUiCallback, Event, HoveredFile, Id, ImmediateViewport, Key, Modifiers,
    Pos2, RawInput, Rect, Vec2, ViewportBuilder, ViewportCommand, ViewportEvent, ViewportId,
    ViewportInfo,
};
use egui_pop_api::{App, AppHandle};

/// Each window's size at first.
const SIZE: Vec2 = Vec2::new(640.0, 480.0);

/// The immediate windows' side of a harness: the input each takes in its
/// next pass, and the output of the passes run inside the last main pass.
#[derive(Default)]
struct Immediate {
    inputs: BTreeMap<ViewportId, RawInput>,
    outputs: Vec<(ViewportId, egui::FullOutput)>,
    /// The builders the windows shown were given.
    builders: Vec<(ViewportId, ViewportBuilder)>,
}

thread_local! {
    /// The harnesses of this thread, by the token their context keeps:
    /// egui's immediate renderer is one per thread.
    static HARNESSES: RefCell<BTreeMap<u64, Weak<RefCell<Immediate>>>> =
        const { RefCell::new(BTreeMap::new()) };
    static NEXT_TOKEN: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Where a harness's context keeps its token.
fn token_id() -> Id {
    Id::unique("egui_pop_test.multi_window")
}

/// The immediate renderer: runs an immediate window's pass in the context
/// that shows it, with the input its harness prepared, and keeps the
/// output for it. A context without a harness gets a plain input.
fn render_immediate(ctx: &Context, viewport: ImmediateViewport<'_>) {
    let ImmediateViewport {
        ids,
        builder,
        mut viewport_ui_cb,
    } = viewport;
    let token = ctx.data(|d| d.get_temp::<u64>(token_id()));
    let harness = token.and_then(|t| HARNESSES.with_borrow(|h| h.get(&t).and_then(Weak::upgrade)));
    let input = harness
        .as_ref()
        .and_then(|h| h.borrow_mut().inputs.remove(&ids.this))
        .unwrap_or_else(|| RawInput {
            viewport_id: ids.this,
            // A window shown for the first time: the backend makes it now.
            viewports: std::iter::once((ids.this, MultiWindow::fresh_info())).collect(),
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            ..Default::default()
        });
    let out = ctx.run_ui(input, |ui| viewport_ui_cb(ui));
    if let Some(h) = harness {
        let mut h = h.borrow_mut();
        h.outputs.push((ids.this, out));
        h.builders.push((ids.this, builder));
    }
}

/// An application driven pass by pass, each window on its own.
pub struct MultiWindow {
    ctx: Context,
    app: App,
    /// The immediate windows' inputs and outputs, shared with the
    /// renderer.
    immediate: Rc<RefCell<Immediate>>,
    token: u64,
    /// The immediate windows the last main pass showed.
    shown_immediate: BTreeSet<ViewportId>,
    /// The deferred windows the last main pass declared, with their
    /// callbacks.
    declared: BTreeMap<ViewportId, Arc<DeferredViewportUiCallback>>,
    info: BTreeMap<ViewportId, ViewportInfo>,
    queued: BTreeMap<ViewportId, Vec<Event>>,
    /// Files from another program over each window (hovering) or
    /// dropped on it, for its next pass.
    files: BTreeMap<ViewportId, Files>,
    /// The commands each window's last pass sent.
    commands: BTreeMap<ViewportId, Vec<ViewportCommand>>,
    /// The builder each window was last given by the main window's pass.
    builders: BTreeMap<ViewportId, ViewportBuilder>,
    /// The accessible names each window's last pass gave.
    names: BTreeMap<ViewportId, Vec<String>>,
    /// Each named node's rectangle (in points) and whether it is
    /// disabled, per window, from its last pass.
    nodes: BTreeMap<ViewportId, Vec<(String, Rect, bool)>>,
    /// Windows the backend keeps listing once no longer declared.
    held: BTreeSet<ViewportId>,
    /// Whether each window's last pass painted content (a text, an
    /// image, a drawing; not only backgrounds and lines).
    painted: BTreeMap<ViewportId, bool>,
    /// The shapes each window's last pass painted.
    shapes: BTreeMap<ViewportId, Vec<egui::epaint::ClippedShape>>,
    /// Every pass's texture changes, in order (the windows share their
    /// textures, the context's), for a picture of a window.
    textures: egui::TexturesDelta,
    /// The window invariants a test breaks on purpose.
    allowed: BTreeSet<String>,
    /// The system tells no window's position (Wayland): no inner or
    /// outer rectangle in the windows' input.
    positions_hidden: bool,
    /// The broken invariants already looked at.
    seen_broken: usize,
    time: f64,
}

impl std::fmt::Debug for MultiWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MultiWindow")
            .field("declared", &self.declared.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl MultiWindow {
    /// Builds the application in a context whose viewports are real.
    pub fn new(build: impl FnOnce(&Context) -> App) -> Self {
        let ctx = Context::default();
        ctx.set_embed_viewports(false);
        ctx.enable_accesskit();
        let immediate = Rc::new(RefCell::new(Immediate::default()));
        let token = NEXT_TOKEN.with(|n| {
            let t = n.get();
            n.set(t + 1);
            t
        });
        HARNESSES.with_borrow_mut(|h| h.insert(token, Rc::downgrade(&immediate)));
        ctx.data_mut(|d| d.insert_temp(token_id(), token));
        Context::set_immediate_viewport_renderer(render_immediate);
        let app = build(&ctx);
        let mut info = BTreeMap::new();
        info.insert(ViewportId::ROOT, Self::fresh_info());
        Self {
            ctx,
            app,
            immediate,
            token,
            shown_immediate: BTreeSet::new(),
            declared: BTreeMap::new(),
            info,
            queued: BTreeMap::new(),
            files: BTreeMap::new(),
            commands: BTreeMap::new(),
            builders: BTreeMap::new(),
            names: BTreeMap::new(),
            nodes: BTreeMap::new(),
            held: BTreeSet::new(),
            painted: BTreeMap::new(),
            shapes: BTreeMap::new(),
            textures: egui::TexturesDelta::default(),
            allowed: BTreeSet::new(),
            positions_hidden: false,
            seen_broken: 0,
            time: 0.0,
        }
    }

    fn fresh_info() -> ViewportInfo {
        ViewportInfo {
            inner_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            focused: Some(false),
            ..Default::default()
        }
    }

    pub fn app(&self) -> &AppHandle {
        self.app.handle()
    }

    pub fn ctx(&self) -> &Context {
        &self.ctx
    }

    /// Lets the window invariant `rule` break without failing the test
    /// (`egui_pop_api::window_invariants::RULES`): a test that breaks it
    /// on purpose, and asserts its `window.invariant` itself.
    pub fn allow_broken(&mut self, rule: &str) {
        self.allowed.insert(rule.to_string());
    }

    /// Panics when a window invariant broke since the last look and the
    /// test did not allow its rule.
    fn assert_invariants(&mut self) {
        let broken = self.app.handle().broken_invariants();
        let new = broken.get(self.seen_broken..).unwrap_or_default().to_vec();
        self.seen_broken = broken.len();
        let failing: Vec<String> = new
            .iter()
            .filter(|b| !self.allowed.contains(&b.rule))
            .map(|b| {
                format!(
                    "`{}` broken by `{}` (owner {:?}): {}",
                    b.rule, b.window, b.owner, b.state
                )
            })
            .collect();
        assert!(failing.is_empty(), "window invariants broken: {failing:#?}");
    }

    /// The deferred windows the last main pass declared.
    pub fn declared(&self) -> BTreeSet<ViewportId> {
        self.declared.keys().copied().collect()
    }

    /// The commands sent to a window by the last pass that sent it any
    /// (its own pass always counts, even with none).
    pub fn commands(&self, viewport: ViewportId) -> Vec<ViewportCommand> {
        self.commands.get(&viewport).cloned().unwrap_or_default()
    }

    /// The accessible names of what a window's last pass drew (a button's
    /// label, a text's words), in no particular order.
    pub fn names(&self, viewport: ViewportId) -> Vec<String> {
        self.names.get(&viewport).cloned().unwrap_or_default()
    }

    /// Where a window's last pass drew the first node named `name` (in
    /// points), and whether it is disabled.
    pub fn node(&self, viewport: ViewportId, name: &str) -> Option<(Rect, bool)> {
        self.nodes
            .get(&viewport)?
            .iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, r, d)| (*r, *d))
    }

    /// Gives a window the focus (the others lose it).
    pub fn focus(&mut self, viewport: ViewportId) {
        for (v, info) in &mut self.info {
            info.focused = Some(*v == viewport);
        }
        self.info
            .entry(viewport)
            .or_insert_with(Self::fresh_info)
            .focused = Some(true);
    }

    /// The builder the main window's last pass gave a window (deferred or
    /// immediate): the size, position, maximized and full-screen states it
    /// asks its system window to open with.
    pub fn builder(&self, viewport: ViewportId) -> Option<ViewportBuilder> {
        self.builders.get(&viewport).cloned()
    }

    /// Maximizes or restores a window, as the system would: its input says
    /// so from its next pass (its size is the test's: [`Self::set_inner_size`]).
    pub fn set_maximized(&mut self, viewport: ViewportId, on: bool) {
        let info = self.info.entry(viewport).or_insert_with(Self::fresh_info);
        info.maximized = Some(on);
    }

    /// Puts a window in full screen or out of it, as the system would.
    pub fn set_fullscreen(&mut self, viewport: ViewportId, on: bool) {
        let info = self.info.entry(viewport).or_insert_with(Self::fresh_info);
        info.fullscreen = Some(on);
    }

    /// The screens the system says, in points, for this thread's
    /// application (`egui_pop_api::window::screens`): a window kept off
    /// every one of them is brought back onto the first. Forgotten when
    /// the harness is dropped.
    pub fn set_screens(&mut self, screens: &[Rect]) {
        egui_pop_api::window::set_test_screens(Some(screens.to_vec()));
    }

    /// Sets a window's inner size, as the system would after a resize; it
    /// keeps its place.
    pub fn set_inner_size(&mut self, viewport: ViewportId, size: [f32; 2]) {
        let info = self.info.entry(viewport).or_insert_with(Self::fresh_info);
        let at = info.inner_rect.map_or(Pos2::ZERO, |r| r.min);
        info.inner_rect = Some(Rect::from_min_size(at, Vec2::from(size)));
    }

    /// Sets a window's position on the screen, as a platform telling
    /// positions would (Wayland tells none): its outer and inner
    /// rectangles both start there, as a window without the system's
    /// decorations.
    pub fn set_outer_position(&mut self, viewport: ViewportId, at: [f32; 2]) {
        let info = self.info.entry(viewport).or_insert_with(Self::fresh_info);
        let size = info.inner_rect.map_or(SIZE, |r| r.size());
        let rect = Rect::from_min_size(Pos2::from(at), size);
        info.outer_rect = Some(rect);
        info.inner_rect = Some(rect);
    }

    /// The system tells no window's position from now on, as Wayland:
    /// each window knows its size only (no inner or outer rectangle in
    /// its input).
    pub fn hide_positions(&mut self) {
        self.positions_hidden = true;
    }

    /// Queues an event (a pointer's move or button) for a window's next
    /// pass.
    pub fn queue(&mut self, viewport: ViewportId, event: Event) {
        self.queued.entry(viewport).or_default().push(event);
    }

    /// Files from another program dragged over a window, as the system
    /// says them: its next pass sees them hovering (`hovered_files`).
    pub fn hover_files(&mut self, viewport: ViewportId, paths: &[&str]) {
        self.files.entry(viewport).or_default().hovered = paths.iter().map(PathBuf::from).collect();
    }

    /// Files from another program dropped on a window: its next pass sees
    /// them dropped (`dropped_files`), and no longer hovering.
    pub fn drop_files(&mut self, viewport: ViewportId, paths: &[&str]) {
        let files = self.files.entry(viewport).or_default();
        files.hovered.clear();
        files.dropped = paths.iter().map(PathBuf::from).collect();
    }

    /// Clicks a point of a window (in points): the pointer moves there and
    /// presses in one pass, releases in the next.
    pub fn click(&mut self, viewport: ViewportId, at: Pos2) {
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.queue(viewport, Event::PointerMoved(at));
        self.queue(viewport, button(true));
        self.run_pass(viewport);
        self.queue(viewport, button(false));
        self.run_pass(viewport);
    }

    fn run_pass(&mut self, viewport: ViewportId) {
        // An immediate window's pass runs inside the main window's.
        if viewport == ViewportId::ROOT || self.shown_immediate.contains(&viewport) {
            self.root_pass();
        } else {
            self.window_pass(viewport);
        }
    }

    /// The backend keeps listing a window (its system window) after the
    /// main window's pass stopped declaring it, as a backend that failed
    /// to destroy it would: the input of every later pass still has it.
    pub fn hold_listed(&mut self, viewport: ViewportId) {
        self.held.insert(viewport);
        self.info.entry(viewport).or_insert_with(Self::fresh_info);
    }

    /// The user asks to close a window (its close button), seen in its
    /// next pass.
    pub fn request_close(&mut self, viewport: ViewportId) {
        self.info
            .entry(viewport)
            .or_insert_with(Self::fresh_info)
            .events
            .push(ViewportEvent::Close);
    }

    /// Queues a key press and release for a window's next pass.
    pub fn press(&mut self, viewport: ViewportId, key: Key, modifiers: Modifiers) {
        let events = self.queued.entry(viewport).or_default();
        for pressed in [true, false] {
            events.push(Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            });
        }
    }

    fn input(&mut self, viewport: ViewportId) -> RawInput {
        self.time += 1.0 / 60.0;
        let info = self.info.entry(viewport).or_insert_with(Self::fresh_info);
        let rect = info
            .inner_rect
            .map(|r| Rect::from_min_size(Pos2::ZERO, r.size()))
            .unwrap_or(Rect::from_min_size(Pos2::ZERO, SIZE));
        let hidden = self.positions_hidden;
        let viewports = self
            .info
            .iter()
            .map(|(v, i)| {
                let mut i = i.clone();
                if hidden {
                    i.inner_rect = None;
                    i.outer_rect = None;
                }
                (*v, i)
            })
            .collect();
        // Dropped files are said once; hovering ones while they hover.
        let (hovered, dropped) = match self.files.get_mut(&viewport) {
            Some(f) => (f.hovered.clone(), std::mem::take(&mut f.dropped)),
            None => (Vec::new(), Vec::new()),
        };
        RawInput {
            viewport_id: viewport,
            viewports,
            hovered_files: hovered
                .into_iter()
                .map(|path| HoveredFile {
                    path: Some(path),
                    ..Default::default()
                })
                .collect(),
            dropped_files: dropped
                .into_iter()
                .map(|p| Arc::new(SystemFile(p)) as egui::DroppedFileHandle)
                .collect(),
            screen_rect: Some(rect),
            time: Some(self.time),
            focused: self
                .info
                .get(&viewport)
                .and_then(|i| i.focused)
                .unwrap_or(false),
            events: self.queued.remove(&viewport).unwrap_or_default(),
            ..Default::default()
        }
    }

    /// Whether a window's last pass painted content: a text, an image or
    /// a drawing, not only plain backgrounds and lines. A pass that
    /// paints nothing shows the clear colour alone on a real backend (a
    /// black window without its title bar).
    pub fn painted(&self, viewport: ViewportId) -> bool {
        self.painted.get(&viewport).copied().unwrap_or(false)
    }

    /// The shapes a window's last pass painted, in paint order.
    pub fn shapes(&self, viewport: ViewportId) -> &[egui::epaint::ClippedShape] {
        self.shapes.get(&viewport).map_or(&[], Vec::as_slice)
    }

    /// A picture of what a window's last pass painted (feature `wgpu`),
    /// drawn with the process's wgpu device, the size of the last pass's
    /// window: take it right after a pass of that window (an immediate
    /// window's pass runs inside the main one's, so the windows are best
    /// the same size). For a snapshot, compare it with
    /// `egui_kittest::try_image_snapshot_options`.
    #[cfg(feature = "wgpu")]
    pub fn render(&self, viewport: ViewportId) -> Result<image::RgbaImage, String> {
        use egui_kittest::TestRenderer as _;
        let mut renderer = crate::gpu::shared_renderer();
        renderer.handle_delta(&mut self.textures.clone());
        let output = egui::FullOutput {
            shapes: self.shapes(viewport).to_vec(),
            pixels_per_point: self.ctx.pixels_per_point(),
            ..Default::default()
        };
        renderer.render(&self.ctx, &output)
    }

    fn take_output(&mut self, viewport: ViewportId, mut out: egui::FullOutput) {
        self.painted
            .insert(viewport, out.shapes.iter().any(|s| is_content(&s.shape)));
        self.shapes
            .insert(viewport, std::mem::take(&mut out.shapes));
        // No painter: the textures are kept for a picture.
        self.textures
            .append(std::mem::take(&mut out.textures_delta));
        // A close request is seen by one pass only.
        if let Some(info) = self.info.get_mut(&viewport) {
            info.events.clear();
        }
        if let Some(update) = &out.platform_output.accesskit_update {
            let names = update
                .nodes
                .iter()
                .filter_map(|(_, n)| n.label().or_else(|| n.value()).map(str::to_string))
                .collect();
            self.names.insert(viewport, names);
            let ppp = self.ctx.pixels_per_point();
            let nodes = update
                .nodes
                .iter()
                .filter_map(|(_, n)| {
                    let name = n.label().or_else(|| n.value())?.to_string();
                    let b = n.bounds()?;
                    let rect = Rect::from_min_max(
                        Pos2::new(b.x0 as f32 / ppp, b.y0 as f32 / ppp),
                        Pos2::new(b.x1 as f32 / ppp, b.y1 as f32 / ppp),
                    );
                    Some((name, rect, n.is_disabled()))
                })
                .collect();
            self.nodes.insert(viewport, nodes);
        }
        // The commands this pass sent: to its own window, and to others.
        self.commands.insert(viewport, Vec::new());
        for (v, o) in &out.viewport_output {
            if *v == viewport || !o.commands.is_empty() {
                self.commands.insert(*v, o.commands.clone());
            }
        }
        if viewport == ViewportId::ROOT {
            for (v, o) in &out.viewport_output {
                if *v != ViewportId::ROOT && o.viewport_ui_cb.is_some() {
                    self.builders.insert(*v, o.builder.clone());
                }
            }
            self.declared = out
                .viewport_output
                .into_iter()
                .filter(|(v, _)| *v != ViewportId::ROOT)
                .filter_map(|(v, o)| o.viewport_ui_cb.map(|cb| (v, cb)))
                .collect();
            let declared = self.declared.clone();
            // The backend makes a system window for each declared one:
            // listed from the next pass on.
            for v in declared.keys() {
                self.info.entry(*v).or_insert_with(Self::fresh_info);
            }
            let held = &self.held;
            let immediate = &self.shown_immediate;
            for v in immediate {
                self.info.entry(*v).or_insert_with(Self::fresh_info);
            }
            self.info.retain(|v, _| {
                *v == ViewportId::ROOT
                    || declared.contains_key(v)
                    || immediate.contains(v)
                    || held.contains(v)
            });
            // A window gone: the builder it was given goes with it.
            let info = &self.info;
            self.builders.retain(|v, _| info.contains_key(v));
        }
    }

    /// Runs the main window's pass (`App::show`), and the passes of the
    /// immediate windows it shows, inside it.
    pub fn root_pass(&mut self) {
        let input = self.input(ViewportId::ROOT);
        // Each immediate window listed takes its input in the pass that
        // shows it.
        let listed: Vec<ViewportId> = self
            .info
            .keys()
            .copied()
            .filter(|v| *v != ViewportId::ROOT && !self.declared.contains_key(v))
            .collect();
        let inputs: BTreeMap<ViewportId, RawInput> =
            listed.into_iter().map(|v| (v, self.input(v))).collect();
        self.immediate.borrow_mut().inputs = inputs;
        let app = &mut self.app;
        let out = self.ctx.run_ui(input, |ui| app.show(ui));
        let shown = {
            let mut im = self.immediate.borrow_mut();
            im.inputs.clear();
            for (v, b) in std::mem::take(&mut im.builders) {
                self.builders.insert(v, b);
            }
            std::mem::take(&mut im.outputs)
        };
        self.shown_immediate = shown.iter().map(|(v, _)| *v).collect();
        // Theirs first: the main pass's commands to them add to theirs.
        for (v, out) in shown {
            self.take_output(v, out);
        }
        self.take_output(ViewportId::ROOT, out);
        self.assert_invariants();
    }

    /// The immediate windows (a detached panel's) the last main pass
    /// showed, each in a pass of its own.
    pub fn shown_immediate(&self) -> BTreeSet<ViewportId> {
        self.shown_immediate.clone()
    }

    /// Runs a declared window's pass; panics when the last main pass did
    /// not declare it (an immediate window's pass runs inside the main
    /// window's: [`MultiWindow::root_pass`]).
    pub fn window_pass(&mut self, viewport: ViewportId) {
        let cb = self
            .declared
            .get(&viewport)
            .cloned()
            .expect("the window is declared by the main window's last pass");
        let input = self.input(viewport);
        let out = self.ctx.run_ui(input, |ui| cb(ui));
        self.take_output(viewport, out);
        self.assert_invariants();
    }
}

impl Drop for MultiWindow {
    fn drop(&mut self) {
        HARNESSES.with_borrow_mut(|h| h.remove(&self.token));
        egui_pop_api::window::set_test_screens(None);
        // Kept for pictures only: never uploaded, nothing to free.
        self.textures.clear();
    }
}

/// The files over a window, or dropped on it.
#[derive(Debug, Default)]
struct Files {
    hovered: Vec<PathBuf>,
    dropped: Vec<PathBuf>,
}

/// A file dropped from another program, as an integration gives it: its
/// path; its content is empty.
#[derive(Debug)]
struct SystemFile(PathBuf);

impl egui::DroppedFile for SystemFile {
    fn path(&self) -> &std::path::Path {
        &self.0
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn bytes(&self) -> Result<Vec<u8>, String> {
        Ok(Vec::new())
    }

    #[cfg(target_arch = "wasm32")]
    fn bytes_async(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, String>> + '_>> {
        Box::pin(async { Ok(Vec::new()) })
    }
}

/// Whether a shape is content: a text, an image, a drawing; plain
/// rectangles without an image and lines are what an empty window still
/// paints.
fn is_content(shape: &egui::Shape) -> bool {
    match shape {
        egui::Shape::Noop | egui::Shape::LineSegment { .. } => false,
        egui::Shape::Rect(r) => r.brush.is_some(),
        egui::Shape::Vec(v) => v.iter().any(is_content),
        _ => true,
    }
}
