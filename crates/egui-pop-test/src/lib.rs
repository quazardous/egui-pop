//! A test kit for egui-pop applications (layer L4, a dev-dependency).
//!
//! - [`app`] and [`themed`]: egui_kittest harnesses with a theme
//!   installed before the first frame; [`harness_for`]: a plugin in an
//!   application with the default plugins;
//! - [`keymap_problems`] and [`assert_keymap_clean`]: no two commands
//!   claim the same keys, no binding names a command that does not
//!   exist;
//! - [`unnamed_interactive`] and [`assert_all_named`]: every interactive
//!   node (button, check box, slider, text input...) has a name a screen
//!   reader can say;
//! - [`a11y_tree`]: the accessibility tree as indented text, for
//!   assertions and for reading in a failure;
//! - [`indicators`] and [`assert_indicator`]: the micro-indicators with
//!   an id drawn in the last pass, and the state of one;
//! - [`double_click`]: a double click that registers (see
//!   `docs/upstream.md`);
//! - `SharedGpu::shared_wgpu` and `gpu_lock` (feature `wgpu`): one wgpu
//!   device per test process (see `docs/testing.md`);
//! - [`SCALES`], [`app_at`], [`set_native_scale`], [`at_scales`],
//!   [`rects_of`]: tests at several scales (1, 1.25, 1.5, 2), a window
//!   moved to a screen of another scale, rectangles in points to compare
//!   across scales;
//! - [`jobs::FakeHost`]: a host whose jobs a test scripts, event by
//!   event, answering the job commands now, later or never;
//! - `viewport::TestSource` (feature `viewport`): a tile source for the
//!   image viewport, every pixel its own colour, served when the test
//!   says.
//!
//! ```
//! use egui_pop_test::{assert_all_named, themed};
//! use egui_pop_core::theme::ThemeSpec;
//!
//! let mut h = themed(&ThemeSpec::new("pop-light"), egui::vec2(200.0, 100.0), |ui| {
//!     let _ = ui.button("Save");
//! });
//! h.run();
//! assert_all_named(&h);
//! ```

#[cfg(feature = "wgpu")]
mod gpu;
#[cfg(feature = "wgpu")]
pub use gpu::{SharedGpu, gpu_lock, shared_renderer, shared_setup};

pub mod jobs;
#[cfg(feature = "viewport")]
pub mod viewport;

use egui::accesskit::Role;
use egui::{Event, Modifiers, PointerButton, Pos2, Vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable as _};
use egui_pop_api::{App, AppBuilder};
use egui_pop_core::theme::{self, ThemeSpec};

/// A harness drawing a whole application. The application is built on a
/// context of its own, then attaches to the harness's on the first frame
/// (its theme installed before anything is drawn).
pub fn app(builder: AppBuilder, size: Vec2) -> Harness<'static, App> {
    let app = builder
        .build(&egui::Context::default())
        .unwrap_or_else(|e| panic!("the application does not build: {e}"));
    Harness::builder()
        .with_size(size)
        .build_ui_state(|ui, app: &mut App| app.show(ui), app)
}

/// The scales tests run at: whole and fractional (1.25 and 1.5 are where
/// lines blur and rounding shows).
pub const SCALES: [f32; 4] = [1.0, 1.25, 1.5, 2.0];

/// As [`app`], on a screen of this native scale (physical pixels per
/// point).
pub fn app_at(builder: AppBuilder, size: Vec2, scale: f32) -> Harness<'static, App> {
    let app = builder
        .build(&egui::Context::default())
        .unwrap_or_else(|e| panic!("the application does not build: {e}"));
    Harness::builder()
        .with_size(size)
        .with_pixels_per_point(scale)
        .build_ui_state(|ui, app: &mut App| app.show(ui), app)
}

/// Moves the harness's window to a screen of another native scale, as
/// winit's scale change does (the interface zoom is left alone, unlike
/// `Harness::set_pixels_per_point`, which changes the zoom). Takes effect
/// at the next step.
// upstream: kittest has no way to change the native scale mid-run (see
// docs/upstream.md).
pub fn set_native_scale<S>(h: &mut Harness<'_, S>, scale: f32) {
    h.input_mut()
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .native_pixels_per_point = Some(scale);
}

/// Runs `f` at each of [`SCALES`], saying which one failed.
#[track_caller]
pub fn at_scales(mut f: impl FnMut(f32)) {
    for s in SCALES {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(s)));
        if let Err(e) = r {
            eprintln!("failed at scale {s}");
            std::panic::resume_unwind(e);
        }
    }
}

/// The rectangle, in points, of each node with this label (scale
/// independent checks compare them across scales).
pub fn rects_of<S>(h: &Harness<'_, S>, label: &str) -> Vec<egui::Rect> {
    h.query_all_by_label(label).map(|n| n.rect()).collect()
}

/// A harness drawing a plugin in an application with the default
/// plugins (palette, dialogs, toasts, settings, window), at a common
/// window size.
pub fn harness_for(plugin: impl egui_pop_api::Plugin) -> Harness<'static, App> {
    app(
        App::builder()
            .plugins(egui_pop_api::DefaultPlugins)
            .plugin(plugin),
        Vec2::new(1024.0, 768.0),
    )
}

/// What is wrong with an application's keymap: keys claimed by two
/// commands in the same scope (one loses), bindings running a command
/// nobody declared.
pub fn keymap_problems(app: &App) -> Vec<String> {
    use egui_pop_core::model::keymap::Action;
    let handle = app.handle();
    let mut out: Vec<String> = handle
        .key_conflicts()
        .iter()
        .map(|(lose, win)| {
            format!(
                "`{}` ({}): {} loses to {}",
                lose.keys, lose.options.scope, lose.action, win.action
            )
        })
        .collect();
    for b in app.keymap().all() {
        if let Action::Run(c) = &b.action
            && handle.command(c).is_none()
        {
            out.push(format!("`{}` runs `{c}`, which no plugin declares", b.keys));
        }
    }
    out
}

/// Fails with the keymap's problems, if any.
#[track_caller]
pub fn assert_keymap_clean(app: &App) {
    let problems = keymap_problems(app);
    assert!(
        problems.is_empty(),
        "keymap problems:\n{}",
        problems.join("\n")
    );
}

/// As [`app`], rendering with wgpu (for snapshots).
#[cfg(feature = "wgpu")]
pub fn app_wgpu(builder: AppBuilder, size: Vec2) -> Harness<'static, App> {
    let app = builder
        .build(&egui::Context::default())
        .unwrap_or_else(|e| panic!("the application does not build: {e}"));
    Harness::builder()
        .with_size(size)
        .shared_wgpu()
        .build_ui_state(|ui, app: &mut App| app.show(ui), app)
}

/// A harness drawing `ui` with a theme installed before the first frame.
pub fn themed<'a>(spec: &ThemeSpec, size: Vec2, ui: impl FnMut(&mut egui::Ui) + 'a) -> Harness<'a> {
    let h = Harness::builder().with_size(size).build_ui(ui);
    theme::install(&h.ctx, spec);
    h
}

/// Roles a user acts on, which must have a name.
pub const INTERACTIVE: &[Role] = &[
    Role::Button,
    Role::DefaultButton,
    Role::CheckBox,
    Role::RadioButton,
    Role::Switch,
    Role::Slider,
    Role::SpinButton,
    Role::TextInput,
    Role::MultilineTextInput,
    Role::SearchInput,
    Role::ComboBox,
    Role::Link,
    Role::MenuItem,
    Role::MenuItemCheckBox,
    Role::MenuItemRadio,
    Role::Tab,
    Role::ListBoxOption,
    Role::ColorWell,
];

fn describe<'t>(n: &impl NodeT<'t>) -> String {
    let a = n.accesskit_node();
    let mut s = format!("{:?}", a.role());
    if let Some(l) = a.label().filter(|l| !l.is_empty()) {
        s.push_str(&format!(" {l:?}"));
    }
    if let Some(v) = a.value().filter(|v| !v.is_empty()) {
        s.push_str(&format!(" value={v:?}"));
    }
    if let Some(v) = a.numeric_value() {
        s.push_str(&format!(" value={v}"));
    }
    if a.is_disabled() {
        s.push_str(" disabled");
    }
    if a.is_focused() {
        s.push_str(" focused");
    }
    s
}

fn named<'t>(n: &impl NodeT<'t>) -> bool {
    let a = n.accesskit_node();
    a.label().is_some_and(|l| !l.trim().is_empty()) || !a.data().labelled_by().is_empty()
}

/// The interactive nodes without a name, described with their path in
/// the tree.
pub fn unnamed_interactive<S>(h: &Harness<'_, S>) -> Vec<String> {
    let mut out = Vec::new();
    for n in h.root().children_recursive() {
        let role = n.accesskit_node().role();
        if INTERACTIVE.contains(&role) && !named(&n) {
            let mut path = vec![describe(&n)];
            let mut at = n.accesskit_node().parent();
            while let Some(p) = at {
                if let Some(l) = p.label().filter(|l| !l.is_empty()) {
                    path.push(format!("{:?} {l:?}", p.role()));
                }
                at = p.parent();
            }
            path.reverse();
            out.push(path.join(" > "));
        }
    }
    out
}

/// Fails, listing them, when an interactive node has no name.
#[track_caller]
pub fn assert_all_named<S>(h: &Harness<'_, S>) {
    let unnamed = unnamed_interactive(h);
    assert!(
        unnamed.is_empty(),
        "interactive nodes without a name:\n  {}\n\naccessibility tree:\n{}",
        unnamed.join("\n  "),
        a11y_tree(h)
    );
}

/// The accessibility tree as indented text: role, name, value, state.
/// Generic containers without a name are skipped (their children are
/// kept), so the dump reads as the screen reader would.
pub fn a11y_tree<S>(h: &Harness<'_, S>) -> String {
    fn walk<'t, N: NodeT<'t> + 't>(n: N, depth: usize, out: &mut String) {
        let a = n.accesskit_node();
        if a.role() == Role::TextRun {
            return;
        }
        let skip = matches!(a.role(), Role::GenericContainer | Role::Unknown)
            && a.label().is_none_or(|l| l.is_empty());
        let next = if skip {
            depth
        } else {
            out.push_str(&"  ".repeat(depth));
            out.push_str(&describe(&n));
            out.push('\n');
            depth + 1
        };
        for c in a.children() {
            walk(n.new_related(c), next, out);
        }
    }
    let mut out = String::new();
    walk(h.root(), 0, &mut out);
    out
}

/// The micro-indicators with an id drawn in the harness's last pass
/// (what the debug bridge's `indicators` collector shows).
pub fn indicators<S>(h: &Harness<'_, S>) -> Vec<egui_pop_core::indicator::Record> {
    egui_pop_core::indicator::drawn(&h.ctx)
}

/// Fails unless the indicator `id` was drawn in the last pass in
/// `state` (an LED's `on`, a badge's `count`, a flag's `modified`...).
#[track_caller]
pub fn assert_indicator<S>(h: &Harness<'_, S>, id: &str, state: &str) {
    let all = indicators(h);
    match all.iter().find(|r| r.id == id) {
        Some(r) => assert_eq!(r.state, state, "indicator `{id}`: {r:?}"),
        None => panic!(
            "no indicator `{id}` drawn; there are: {:?}",
            all.iter().map(|r| r.id.as_str()).collect::<Vec<_>>()
        ),
    }
}

/// Double-clicks at a point, so that egui sees a double click.
// upstream: kittest queues events one step apart, beyond egui's
// double-click delay (see docs/upstream.md); the four button events go
// in one frame instead.
pub fn double_click_at<S>(h: &mut Harness<'_, S>, pos: Pos2) {
    let events = &mut h.input_mut().events;
    events.push(Event::PointerMoved(pos));
    for _ in 0..2 {
        for pressed in [true, false] {
            events.push(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
    }
    h.step();
}

/// Double-clicks the centre of the node labelled `label`.
#[track_caller]
pub fn double_click<S>(h: &mut Harness<'_, S>, label: &str) {
    let pos = h.get_by_label(label).rect().center();
    double_click_at(h, pos);
}
