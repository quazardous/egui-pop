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

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use egui::{
    Context, DeferredViewportUiCallback, Event, Key, Modifiers, Pos2, RawInput, Rect, Vec2,
    ViewportCommand, ViewportEvent, ViewportId, ViewportInfo,
};
use egui_pop_api::{App, AppHandle};

/// Each window's size at first.
const SIZE: Vec2 = Vec2::new(640.0, 480.0);

/// An application driven pass by pass, each window on its own.
pub struct MultiWindow {
    ctx: Context,
    app: App,
    /// The deferred windows the last main pass declared, with their
    /// callbacks.
    declared: BTreeMap<ViewportId, Arc<DeferredViewportUiCallback>>,
    info: BTreeMap<ViewportId, ViewportInfo>,
    queued: BTreeMap<ViewportId, Vec<Event>>,
    /// The commands each window's last pass sent.
    commands: BTreeMap<ViewportId, Vec<ViewportCommand>>,
    /// The accessible names each window's last pass gave.
    names: BTreeMap<ViewportId, Vec<String>>,
    /// Each named node's rectangle (in points) and whether it is
    /// disabled, per window, from its last pass.
    nodes: BTreeMap<ViewportId, Vec<(String, Rect, bool)>>,
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
        let app = build(&ctx);
        let mut info = BTreeMap::new();
        info.insert(ViewportId::ROOT, Self::fresh_info());
        Self {
            ctx,
            app,
            declared: BTreeMap::new(),
            info,
            queued: BTreeMap::new(),
            commands: BTreeMap::new(),
            names: BTreeMap::new(),
            nodes: BTreeMap::new(),
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

    /// Sets a window's inner size, as the system would after a resize.
    pub fn set_inner_size(&mut self, viewport: ViewportId, size: [f32; 2]) {
        let info = self.info.entry(viewport).or_insert_with(Self::fresh_info);
        info.inner_rect = Some(Rect::from_min_size(Pos2::ZERO, Vec2::from(size)));
    }

    /// Sets a window's outer position, as a platform telling positions
    /// would (Wayland tells none).
    pub fn set_outer_position(&mut self, viewport: ViewportId, at: [f32; 2]) {
        let info = self.info.entry(viewport).or_insert_with(Self::fresh_info);
        let size = info.inner_rect.map_or(SIZE, |r| r.size());
        info.outer_rect = Some(Rect::from_min_size(Pos2::from(at), size));
    }

    /// Queues an event (a pointer's move or button) for a window's next
    /// pass.
    pub fn queue(&mut self, viewport: ViewportId, event: Event) {
        self.queued.entry(viewport).or_default().push(event);
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
        if viewport == ViewportId::ROOT {
            self.root_pass();
        } else {
            self.window_pass(viewport);
        }
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
        let viewports = self.info.iter().map(|(v, i)| (*v, i.clone())).collect();
        RawInput {
            viewport_id: viewport,
            viewports,
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

    fn take_output(&mut self, viewport: ViewportId, mut out: egui::FullOutput) {
        // No painter: the textures are not uploaded anywhere.
        out.textures_delta.clear();
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
            self.declared = out
                .viewport_output
                .into_iter()
                .filter(|(v, _)| *v != ViewportId::ROOT)
                .filter_map(|(v, o)| o.viewport_ui_cb.map(|cb| (v, cb)))
                .collect();
            let declared = self.declared.clone();
            self.info
                .retain(|v, _| *v == ViewportId::ROOT || declared.contains_key(v));
        }
    }

    /// Runs the main window's pass (`App::show`).
    pub fn root_pass(&mut self) {
        let input = self.input(ViewportId::ROOT);
        let app = &mut self.app;
        let out = self.ctx.run_ui(input, |ui| app.show(ui));
        self.take_output(ViewportId::ROOT, out);
    }

    /// Runs a declared window's pass; panics when the last main pass did
    /// not declare it.
    pub fn window_pass(&mut self, viewport: ViewportId) {
        let cb = self
            .declared
            .get(&viewport)
            .cloned()
            .expect("the window is declared by the main window's last pass");
        let input = self.input(viewport);
        let out = self.ctx.run_ui(input, |ui| cb(ui));
        self.take_output(viewport, out);
    }
}
