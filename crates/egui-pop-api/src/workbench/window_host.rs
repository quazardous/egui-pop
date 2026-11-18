//! The detached panels' windows as egui_stack hosts native floats: a
//! [`ViewportHost`] over the workbench's windows (`layout.auxiliary`).
//!
//! egui_stack never creates a window: what hosts its native floats says
//! which viewport each one is, how its window is built and how it is
//! shown. Here that is egui-pop's own window for a detached panel:
//!
//! - **Viewport ids stay egui-pop's** ([`super::window_id`], one per
//!   window, from the panel naming it): an application pairs things with
//!   them (an image surface, a display route), and they never change
//!   while the window is open.
//! - **The window is built as egui-pop builds it:** titled after its
//!   shown panel and the application, at the place it opened with
//!   (#5241), decorated by the system unless the application draws its
//!   own frame, transparent where a surface below may show through.
//! - **It is shown as a window of the application in full:** its keys,
//!   files, drags and overlays, the application's title bar, its place
//!   kept in the layout, its close asked through `workbench.close_window`.
//! - **Its screens are egui-pop's** ([`crate::window::set_screens`]), in
//!   egui_stack's terms ([`stack_screens`]: physical pixels, named), which
//!   places the floats; egui-pop brings its windows back onto them itself,
//!   so the host tells egui_stack none and its repair stays off.
//!
//! [`float_of`] gives a detached window as egui_stack's model sees a
//! float: the layout document stays the saved form, and the float is read
//! from it.

use egui::{Context, Rect, Ui, ViewportBuilder, ViewportClass, ViewportId};
use egui_pop_core::strings;
use egui_pop_model::ids::PanelId;
use egui_pop_model::layout::Auxiliary;
use egui_stack::ViewportHost;
use egui_stack::layout::{Float, FloatHost, Place, Screen};
use serde_json::json;

use crate::app::AppHandle;
use crate::strings as txt;

use super::{Change, window_id};

/// The name of the dock a detached window's tabs are in, in egui_stack's
/// terms: one per window, after the panel naming it.
pub(crate) fn window_dock(panel: &PanelId) -> String {
    format!("window:{}", panel.as_str())
}

/// The panel naming the window of a float's key (the key is the panel's
/// id).
fn panel_of_key(key: &str) -> Option<PanelId> {
    PanelId::new(key).ok()
}

/// egui-pop's screens in egui_stack's terms: named (a screen the system
/// names none of is named after its rank, the first
/// [`crate::window::UNNAMED_SCREEN`]), in desktop physical pixels.
pub(crate) fn stack_screens(screens: &[crate::window::Screen]) -> Vec<Screen> {
    screens
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let name = s.name.clone().unwrap_or_else(|| match i {
                0 => crate::window::UNNAMED_SCREEN.to_owned(),
                _ => format!("{}-{i}", crate::window::UNNAMED_SCREEN),
            });
            let scale = sane(s.scale);
            let rect = Rect::from_min_max(
                (s.rect.min.to_vec2() * scale).to_pos2(),
                (s.rect.max.to_vec2() * scale).to_pos2(),
            );
            Screen::new(name, rect, scale)
        })
        .collect()
}

/// A scale factor as egui-pop's screens give it: positive, else 1.0.
fn sane(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

/// A detached window as an egui_stack float: keyed by the panel naming
/// it, showing its dock ([`window_dock`]), native, at its place. The
/// place is read on the screen it names, else on the screen holding it;
/// on none of them (no screen told, or off them all), it keeps the
/// desktop rect with no screen, as a layout from before screens were
/// named.
pub(crate) fn float_of(aux: &Auxiliary, screens: &[Screen]) -> Float {
    let [x, y, w, h] = aux.rect;
    let rect = Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h));
    let named = screens.iter().find(|s| s.name == aux.screen);
    let place = match named {
        Some(screen) => {
            // The window's corner from the screen's, in physical pixels
            // over the screen's scale: the screen's own points.
            let scale = sane(screen.scale);
            let origin = (screen.rect.min.to_vec2() / scale).to_pos2();
            Place::on(
                screen,
                Rect::from_min_size((rect.min - origin).to_pos2(), rect.size()),
            )
        }
        None => {
            let physical = screens.iter().find_map(|s| {
                let scale = sane(s.scale);
                let r = Rect::from_min_max(
                    (rect.min.to_vec2() * scale).to_pos2(),
                    (rect.max.to_vec2() * scale).to_pos2(),
                );
                Place::from_physical(r, std::slice::from_ref(s))
                    .filter(|_| s.rect.contains(r.center()))
            });
            physical.unwrap_or_else(|| Place::new(rect))
        }
    };
    let mut place = place;
    place.maximized = aux.maximized;
    place.fullscreen = aux.fullscreen;
    Float::dock(
        aux.panel.as_str(),
        window_dock(&aux.panel),
        FloatHost::Native,
        place,
    )
}

/// How a detached window is made transparent, and why (the application
/// asks it of its panels' windows, or builds them opaque).
fn transparency(app: &AppHandle) -> egui_pop_core::window_surface::Transparency {
    if app.transparent_panel_windows() {
        crate::window::surface_below_transparency(crate::window::windowing())
    } else {
        egui_pop_core::window_surface::Transparency::Opaque(
            "the application builds its panels' windows opaque \
             (AppBuilder::transparent_panel_windows)"
                .into(),
        )
    }
}

/// The detached panels' windows, hosting egui_stack's native floats.
pub(crate) struct PanelWindows<'a> {
    app: &'a AppHandle,
    /// The main window's context, where the windows are declared.
    ctx: &'a Context,
    /// The application's name, in each window's title.
    app_name: &'a str,
    /// Dispatches a window's keys in its own context.
    keys: &'a mut dyn FnMut(&Context),
}

impl<'a> PanelWindows<'a> {
    pub(crate) fn new(
        app: &'a AppHandle,
        ctx: &'a Context,
        app_name: &'a str,
        keys: &'a mut dyn FnMut(&Context),
    ) -> Self {
        Self {
            app,
            ctx,
            app_name,
            keys,
        }
    }

    /// The window of viewport `id`, as the layout holds it.
    fn window(&self, id: ViewportId) -> Option<Auxiliary> {
        let wb = self.app.workbench_mut();
        (wb.layout.auxiliary.iter())
            .find(|a| window_id(&a.panel) == id)
            .cloned()
    }

    /// A window's title: its shown panel's, with the application's name;
    /// `None` when that panel is not declared.
    fn title(&self, ctx: &Context, aux: &Auxiliary) -> Option<String> {
        let spec = self.app.workbench_mut().spec(aux.shown()).cloned()?;
        let panel_title = strings::panel_title(ctx, spec.id.as_str(), &spec.title);
        Some(if self.app_name.is_empty() {
            panel_title
        } else {
            strings::tr_args(
                ctx,
                &txt::WORKBENCH_WINDOW_TITLE,
                &[("panel", &panel_title), ("application", self.app_name)],
            )
        })
    }
}

impl PanelWindows<'_> {
    /// What a window's builder says of it beside its place, as it stands
    /// this pass: its title (its shown panel's), its decorations (the
    /// system's unless the application draws its own frame), and whether
    /// it is transparent (a surface below may show through, or rounded
    /// corners). The backend follows a change of them; its place stays the
    /// one it opened with.
    fn dressed(&self, builder: ViewportBuilder, aux: &Auxiliary) -> ViewportBuilder {
        let ctx = self.ctx;
        let title = self
            .title(ctx, aux)
            .unwrap_or_else(|| aux.shown().as_str().to_owned());
        let own = crate::frame::own_frame(self.app);
        let builder = builder.with_title(title).with_decorations(!own);
        let rounded = own && egui_pop_core::frame::rounding(ctx);
        if transparency(self.app).is_transparent() || rounded {
            builder.with_transparent(true)
        } else {
            builder
        }
    }
}

impl ViewportHost for PanelWindows<'_> {
    fn viewport_id(&self, key: &str) -> ViewportId {
        super::panel_viewport(key)
    }

    /// Built at the place the window opened with (#5241: egui_stack makes
    /// it once and gives it again unchanged for the window's life, so the
    /// backend asks the system nothing more), titled, decorated by the
    /// system unless the application draws its own frame, transparent
    /// where a surface below may show through or for rounded corners
    /// ([`Self::dressed`], again each pass).
    fn builder(&mut self, float: &Float) -> ViewportBuilder {
        let Some(aux) =
            panel_of_key(&float.key).and_then(|p| self.window(super::panel_viewport(p.as_str())))
        else {
            return ViewportBuilder::default().with_title(float.key.clone());
        };
        // egui_stack makes it once and gives it again unchanged (#5241).
        let opened = Some(super::repaired_place(
            (aux.rect, aux.maximized, aux.fullscreen),
            &crate::window::screens(),
        ));
        let mut builder = self.dressed(ViewportBuilder::default(), &aux);
        if let Some(state) = opened {
            builder = builder.with_inner_size(state.size);
            if let Some(p) = state.position {
                builder = builder.with_position(p);
            }
            if state.maximized {
                builder = builder.with_maximized(true);
            }
            if state.fullscreen {
                builder = builder.with_fullscreen(true);
            }
        }
        builder
    }

    /// Shown as a window of the application in full: its keys in its own
    /// context, its files and drags, a modal open elsewhere blocking it,
    /// the application's title bar and edges when it draws its own frame,
    /// then `body`, then the overlays opened from it; its close asked
    /// through `workbench.close_window`, its place kept in the layout.
    fn show(
        &mut self,
        ctx: &Context,
        id: ViewportId,
        builder: ViewportBuilder,
        body: &mut dyn FnMut(&mut Ui),
    ) {
        let Some(aux) = self.window(id) else {
            ctx.show_viewport_immediate(id, builder, |ui, _| body(ui));
            return;
        };
        // Its title follows the panel it shows; its place is the one it
        // opened with.
        let builder = self.dressed(builder, &aux);
        let app = self.app;
        let panel = aux.panel.clone();
        let title = self
            .title(ctx, &aux)
            .unwrap_or_else(|| aux.shown().as_str().to_owned());
        let own = crate::frame::own_frame(app);
        // The window says how it was built and why.
        egui_pop_core::window_surface::set_transparency(ctx, id, Some(transparency(app)));
        let keys = &mut *self.keys;
        crate::bus::panels::show_window(ctx, id, builder, |ui, class| {
            let vctx = ui.ctx().clone();
            // Embedded in the main window (a backend without windows of
            // its own, kittest): the main window's pass does all this.
            let own_window = class != ViewportClass::EmbeddedWindow;
            if own_window {
                app.note_window(&vctx);
                let window = crate::bus::panels::window_of_panel(&panel);
                crate::bus::files::begin_pass(&vctx, &window);
                crate::bus::drag::begin_pass(&vctx, &window);
                app.block_window(&vctx, &|v| !crate::windows::is_app_window(app, v));
                egui_pop_core::overlay::begin_pass(&vctx);
                keys(&vctx);
            }
            if let Some((from, to)) = egui_pop_core::scale::changed(&vctx) {
                app.publish(
                    "egui_pop",
                    None,
                    "scale.changed",
                    json!({ "viewport": panel, "from": from, "to": to }),
                );
            }
            // In full screen, the title bar as the main window's: kept
            // unless the setting says otherwise.
            let full_screen = egui_pop_core::frame::full_screen(&vctx);
            if own && (!full_screen || crate::frame::bar_in_fullscreen(app)) {
                egui_pop_core::frame::resize_edges(&vctx);
                // Its edge: a line that tells it from the window under it
                // (embedded in the main window, that window's is drawn).
                if own_window {
                    egui_pop_core::frame::paint_window_edge(
                        &vctx,
                        egui_pop_core::frame::window_edge(&vctx, &crate::window::screens()),
                    );
                }
                let out =
                    egui::Panel::top(egui::Id::unique(("egui_pop.aux.title", panel.as_str())))
                        .frame(egui::Frame::NONE)
                        .show(ui, |ui| {
                            egui_pop_core::frame::TitleBar::new(&title)
                                .button(egui_pop_core::frame::Button::Minimize, false)
                                .show(ui, |_| {}, |_| {})
                        })
                        .inner;
                match out.request {
                    Some(egui_pop_core::frame::FrameRequest::Close) => {
                        app.change_workbench(Change::CloseWindow(panel.clone()));
                    }
                    Some(r) => crate::frame::request(app, &vctx, r),
                    None => {}
                }
            }
            body(ui);
            if own_window {
                app.show_zone(ui, &crate::zones::WINDOW_OVERLAYS);
                // Files dropped on this window: its `files.drop`.
                crate::bus::files::end_pass(app, ui);
                crate::bus::drag::end_pass(app, &vctx);
                crate::menus::draw_popup(&vctx, app);
            }
            // In a window of its own, egui_stack asks the close of the
            // workbench's viewer (`close_policy`), which runs the step.
            if !own_window && vctx.input(|i| i.viewport().close_requested()) {
                // The step decides, then the window's `window.close`: the
                // default docks its panels back. Refused, it stays.
                app.change_workbench(Change::CloseWindow(panel.clone()));
                let kept = (app.workbench_mut().layout.auxiliary.iter()).any(|a| a.panel == panel);
                if kept {
                    crate::bus::panels::keep_open(&vctx);
                }
            }
            // Its place, kept in the layout.
            if own_window {
                super::keep_window_place(app, &vctx, &panel);
                // A tab moved among its tabs, or another gesture its dock
                // made: read back and said if it changed (`layout.changed`).
                if vctx.input(|i| {
                    i.pointer.any_released()
                        || i.events
                            .iter()
                            .any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))
                }) {
                    app.note_layout("input");
                }
            }
        });
    }

    /// None: egui-pop brings its windows back onto the screens itself
    /// (`window_place`, `screen.removed` docking a window back), so
    /// egui_stack's own repair is left off. The floats' places are read on
    /// [`stack_screens`].
    fn screens(&self) -> Vec<Screen> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aux(screen: &str, rect: [f32; 4]) -> Auxiliary {
        Auxiliary::window(PanelId::from_static("app.log"), screen, rect)
    }

    fn screen(
        name: Option<&str>,
        at: (f32, f32),
        size: (f32, f32),
        scale: f32,
    ) -> crate::window::Screen {
        crate::window::Screen {
            name: name.map(str::to_owned),
            rect: Rect::from_min_size(egui::pos2(at.0, at.1), egui::vec2(size.0, size.1)),
            scale,
        }
    }

    #[test]
    fn screens_are_named_and_in_physical_pixels() {
        let screens = stack_screens(&[
            screen(Some("DP-1"), (0.0, 0.0), (960.0, 540.0), 2.0),
            screen(None, (960.0, 0.0), (1280.0, 1024.0), 1.0),
            screen(None, (2240.0, 0.0), (800.0, 600.0), 0.0),
        ]);
        assert_eq!(screens[0].name, "DP-1");
        assert_eq!(
            screens[0].rect,
            Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1920.0, 1080.0))
        );
        assert_eq!(screens[0].scale, 2.0);
        assert_eq!(screens[1].name, "primary-1");
        assert_eq!(screens[1].rect.min, egui::pos2(960.0, 0.0));
        assert_eq!(screens[2].name, "primary-2");
        assert_eq!(screens[2].scale, 1.0);
    }

    #[test]
    fn a_window_is_a_native_float_keyed_by_its_panel_on_its_dock() {
        let float = float_of(&aux("", [10.0, 20.0, 640.0, 480.0]), &[]);
        assert_eq!(float.key, "app.log");
        assert_eq!(float.dock_name(), Some("window:app.log"));
        assert_eq!(float.host, FloatHost::Native);
        // No screen told: the desktop rect, on no screen.
        assert_eq!(float.place.screen, None);
        assert_eq!(
            float.place.rect,
            Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(640.0, 480.0))
        );
    }

    #[test]
    fn a_window_s_place_is_read_on_the_screen_it_names() {
        let screens = stack_screens(&[
            screen(Some("left"), (0.0, 0.0), (1920.0, 1080.0), 1.0),
            screen(Some("right"), (960.0, 0.0), (1280.0, 720.0), 2.0),
        ]);
        let mut a = aux("right", [1000.0, 100.0, 400.0, 300.0]);
        a.maximized = true;
        let float = float_of(&a, &screens);
        assert_eq!(float.place.screen.as_deref(), Some("right"));
        assert_eq!(float.place.scale, 2.0);
        assert_eq!(
            float.place.rect,
            Rect::from_min_size(egui::pos2(40.0, 100.0), egui::vec2(400.0, 300.0))
        );
        assert!(float.place.maximized);
        // On the desktop, in physical pixels: where egui-pop has it.
        let physical = float.place.physical(&screens[1]);
        assert_eq!(physical.min, egui::pos2(2000.0, 200.0));
        assert_eq!(physical.size(), egui::vec2(800.0, 600.0));
    }

    #[test]
    fn a_window_naming_no_known_screen_is_read_on_the_one_holding_it() {
        let screens = stack_screens(&[screen(Some("only"), (0.0, 0.0), (1920.0, 1080.0), 1.0)]);
        let float = float_of(&aux("gone", [100.0, 50.0, 400.0, 300.0]), &screens);
        assert_eq!(float.place.screen.as_deref(), Some("only"));
        assert_eq!(float.place.rect.min, egui::pos2(100.0, 50.0));
        // Off every screen: kept as it is, on none.
        let float = float_of(&aux("gone", [5000.0, 50.0, 400.0, 300.0]), &screens);
        assert_eq!(float.place.screen, None);
        assert_eq!(float.place.rect.min, egui::pos2(5000.0, 50.0));
    }
}
