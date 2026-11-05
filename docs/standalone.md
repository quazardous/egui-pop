# Using parts without App

egui-pop is a framework, but most of it is a library first: the design
tokens, the theme, the visual kits and most components are plain egui
code that works in any egui or eframe application, with no `App`,
`AppBuilder`, `Ctx` or plugin. This page says what works alone, its
entry point, and what needs the framework and why.

Every example below is compiled and run as a test against the
`egui-pop` crate (`cargo test -p consumer-smoke --doc`), most of them
drawing one frame in a bare `egui::Context`.

## What works alone

| Part | Crate (facade feature) | Entry point without `App` |
|---|---|---|
| Design tokens, themes, contrast, colour ramps | `egui-pop-tokens` (no egui) | `Themes::default().resolve(&ThemeSpec)` |
| The theme in egui's style | `egui-pop-core` | `theme::install(&ctx, &spec)`; `theme::style(&tokens, &base)`; `ScopedTheme` |
| Visual kits | `egui-pop-tokens`, `egui-pop-core` | `Themes::add_kit`, `KitSource::register`, `theme::install_with` |
| Layout by role (`UiPop`), egui's widgets drawn by the kit | `egui-pop-core` | the `UiPop` trait on `egui::Ui`; `core::widgets` |
| Sliders, inputs, sections, indicators, progress, icons, hover cards, placeholders, thumbnails, fault boundaries | `egui-pop-core` | each widget's `new(..)`, then `ui.add` or `.show(ui)` |
| Value binding and the value store | `egui-pop-core`, `egui-pop-model` | `binding::edit`, `Binding::{plain, bound, watch}`, `values::Store` |
| Persistence | `egui-pop-core` | `persist::Persist::new(dir)` |
| Logic without egui: fuzzy matching, keymap, `when` clauses, documents | `egui-pop-model` | each module's types |
| Image viewport, histogram, curves, colour wheels, zoom control, navigator, node graph | `egui-pop-viewport` (`viewport`) | `Viewport::new(..).show(ui, ..)`; each tool's `new(..).show(ui)`; `gpu::install` for the GPU path |
| Tree, list, data table | `egui-pop-views` (`views`) | `TreeView`, `ListView`, `DataTable` with their states |
| Text and code editors, terminal | `egui-pop-editors` (`editors`) | `TextSurface`, `CodeEditor`, `Terminal` over an `EditorState` or `TerminalState` |
| Translations | `egui-pop-i18n` (`i18n`) | an `I18n` installed with `i18n::install(&ctx, &i18n)`: `t(ctx, ..)`, `tr!` and egui-pop's own text |
| Platform readers | `egui-pop-platform` (`platform-*`) | `text_scale::read`, `focus::read`, `wheel_lines::read`, `dirs`, `window::raise` |
| A single instance | `egui-pop-platform` (`platform-instance`) | `instance::single`, then `Launches::set_waker` and `Launches::take` each frame |
| Orthogonal wire routing | `ortho-router` (no egui) | its own crate |
| The test kit's checks on any harness | `egui-pop-test` (dev-dependency) | `themed`, `assert_all_named`, `a11y_tree` |

Each component above that has a `.app(&AppHandle)` option works without
it: with an application its moments run as
[steps](plugins.md) and its keys come from the keymap (rebindable);
without one, it reports what happened in its output and binds its own
default keys.

## The tokens, without egui

`egui-pop-tokens` has no egui dependency: resolve a theme, read its
colours and sizes, check a contrast, for a renderer of your own or a
style sheet.

```rust
use egui_pop::tokens::{ThemeSpec, Themes, contrast};

let tokens = Themes::default().resolve(&ThemeSpec::new("catppuccin-mocha").accent("blue"));
assert!(tokens.problems().is_empty());
let text = tokens.color("text.default").unwrap();
let panel = tokens.color("surface.panel").unwrap();
assert!(contrast::contrast_ratio(text, panel) >= 4.5);
let radius = tokens.dimension("radius.widget");
assert!(radius.is_some());
```

## A theme in a plain eframe application

`theme::install` resolves a theme, writes it into egui's style (both
egui's dark and light styles, fonts, icon sizes, the kit) and makes the
typed tokens readable from the context. Call it once at start-up, and
again when the user picks another theme, between frames.

```rust,no_run
use egui_pop::eframe;
use egui_pop::egui;
use egui_pop::theme::{self, Density, ThemeSpec, Tokens};

struct MyApp;

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // egui's widgets follow the theme; custom painting reads the tokens.
        let tokens = Tokens::of(ui.ctx());
        ui.colored_label(tokens.status.success, "Saved");
        let _ = ui.button("A themed button");
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "Themed",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            theme::install(&cc.egui_ctx, &ThemeSpec::new("pop-light").density(Density::Compact));
            Ok(Box::new(MyApp))
        }),
    )
}
```

To keep control of egui's style yourself, `theme::style` builds a
`Style` from tokens without installing anything; `ScopedTheme` draws one
part of the interface in another theme.

```rust
use egui_pop::egui;
use egui_pop::theme::{self, ScopedTheme, ThemeSpec, Tokens};
use egui_pop::tokens::Themes;

let ctx = egui::Context::default();
let tokens = Tokens::new(Themes::default().resolve(&ThemeSpec::new("nord")));
let style = theme::style(&tokens, &ctx.global_style());
ctx.set_global_style(style);

// A light inspector inside a dark window.
let light = ScopedTheme::new(&ctx, Tokens::new(Themes::default().resolve(&ThemeSpec::new("pop-light"))));
let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
    light.show(ui, |ui| {
        assert!(!Tokens::of(ui.ctx()).dark_mode());
        ui.label("Inspector");
    });
});
out.textures_delta.clear();
```

## A visual kit

`AppBuilder::kit` does three things, each callable alone: add the kit's
theme to the themes you resolve, register its faces and handlers in the
context, install the theme.

```rust
use egui_pop::core::kit::{Kit, KitSource};
use egui_pop::egui;
use egui_pop::theme::{self, ThemeSpec};
use egui_pop::tokens::Themes;
use egui_pop::tokens::kit::KitData;

let kit = KitData::from_text(
    "underline",
    r#"
    tab:
      shape: {$type: "string", $value: "underline"}
    "#,
);
let ctx = egui::Context::default();
let mut themes = Themes::default();
themes.add_kit(&kit);
KitSource::new(kit).register(&ctx);
theme::install_with(&ctx, &themes, &ThemeSpec::new("underline"));
assert_eq!(Kit::of(&ctx).tab.shape, "underline");
```

## Layout by role and egui's widgets

The `UiPop` helpers ([getting started](getting-started.md#laying-out-by-role))
and the kit's drawing of egui's check box, radio button and selectable
row (`egui_pop::core::widgets`) need only an installed theme; without
one they use `pop-dark`.

```rust
use egui_pop::core::widgets::{self, ToggleChip};
use egui_pop::egui;
use egui_pop::prelude::{Space, Tone, UiPop};

fn panel(ui: &mut egui::Ui, raw: &mut bool, flagged: &mut bool) {
    ui.section_title("Filters");
    ui.muted("Shown in the grid.");
    ui.space_role(Space::Group);
    widgets::checkbox(ui, raw, "Raw files only");
    ui.add(ToggleChip::new(flagged, "Flagged"));
    if ui.button_tone(Tone::Accent, "Apply").clicked() { /* ... */ }
}

let ctx = egui::Context::default();
let (mut raw, mut flagged) = (true, false);
let mut out = ctx.run_ui(egui::RawInput::default(), |ui| panel(ui, &mut raw, &mut flagged));
out.textures_delta.clear();
```

## Core components

Sliders, inputs, sections, indicators, progress, icons, hover cards and
placeholders are egui widgets: build, add, read the response. A slider's
`on_change` takes a plain closure (with an application,
`api::sliders::hook` runs each change as the `slider.change` step).

```rust
use egui_pop::core::hover_card::{Card, HoverCard};
use egui_pop::core::icon::Icon;
use egui_pop::core::indicator::{Indicator, Led};
use egui_pop::core::input::Input;
use egui_pop::core::placeholder::EmptyState;
use egui_pop::core::progress::ProgressBar;
use egui_pop::core::sections::{Section, Sections};
use egui_pop::core::slider::{Slider, Verdict};
use egui_pop::egui;

struct Panel {
    exposure: f64,
    name: String,
    photos: Vec<String>,
}

fn panel(ui: &mut egui::Ui, p: &mut Panel) {
    Sections::new("develop").show(ui, |stack| {
        stack.section(Section::new("basic", "Basic"), |ui| {
            ui.add(
                Slider::new(&mut p.exposure, "Exposure", -3.0..=4.0)
                    .unit("EV")
                    .default(0.0)
                    // Refuse the far end of the range.
                    .on_change(|c| if c.to > 3.5 { Verdict::Redirect(3.5) } else { Verdict::Go }),
            );
            Input::text(&mut p.name, "Export name").placeholder("File name").clearable(true).show(ui);
        });
        stack.section(Section::new("photos", "Photos").count(p.photos.len() as u64), |ui| {
            if p.photos.is_empty() {
                EmptyState::new(Icon::Folder, "No photo").text("Import a folder.").show(ui);
            }
        });
    });
    ui.add(ProgressBar::counts(340, 1204).label("Importing photos"));
    let led = ui.add(Indicator::led(Led::On).id("link").tooltip("The render node's link"));
    HoverCard::new("link-card").show(ui, &led, &Card::new().text("Connected").row("Latency", "3 ms"));
}

let ctx = egui::Context::default();
let mut p = Panel { exposure: 0.0, name: String::new(), photos: Vec::new() };
let mut out = ctx.run_ui(egui::RawInput::default(), |ui| panel(ui, &mut p));
out.textures_delta.clear();
```

A thumbnail grid and a fault boundary (an error card instead of a
panic taking the window down) are plain functions too:

```rust
use egui_pop::core::fault;
use egui_pop::core::thumbnail::ThumbnailGrid;
use egui_pop::egui;

fn grid(ui: &mut egui::Ui, names: &[&str]) {
    fault::boundary(ui, "grid", "The grid", |ui| {
        ThumbnailGrid::new(names.len(), egui::vec2(96.0, 96.0)).show(ui, |ui, i| {
            ui.label(names[i]);
        });
    });
}

let ctx = egui::Context::default();
let mut out = ctx.run_ui(egui::RawInput::default(), |ui| grid(ui, &["a.jpg", "b.jpg"]));
out.textures_delta.clear();
```

## A value store, without the application

The value store (`egui-pop-model`) and the bindings (`egui-pop-core`)
work on their own: a widget bound to a store writes with an origin, a
drag is one undo step, listeners are told
([ADR 0009](adr/0009-value-binding.md)).

```rust
use std::sync::{Arc, Mutex};

use egui_pop::core::binding::{self, Binding};
use egui_pop::egui;
use egui_pop::model::ids::OwnerId;
use egui_pop::model::values::{Descriptor, Store};

let store = Arc::new(Mutex::new(Store::new()));
store
    .lock()
    .unwrap()
    .declare(&OwnerId::FRAMEWORK, "view/zoom", Descriptor::float("Zoom", 0.1, 8.0, 1.0))
    .unwrap();

let ctx = egui::Context::default();
let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
    binding::edit(ui, Binding::bound(&store, "view/zoom"), |ui, v: &mut f64| {
        ui.add(egui::Slider::new(v, 0.1..=8.0))
    });
});
out.textures_delta.clear();
assert_eq!(binding::read::<f64>(&store, "view/zoom"), Some(1.0));
```

The rest of `egui-pop-model` is logic without egui: fuzzy matching
(`fuzzy::rank`), the keymap and its resolution trace, `when` clauses,
versioned documents.

```rust
use egui_pop::model::fuzzy;

let items = ["Export as JPEG", "Open folder", "Export settings"];
let ranked = fuzzy::rank(&items, "exp", |s| s.to_string());
assert_eq!(ranked.len(), 2);
```

## The image viewport and the image tools

With the `viewport` feature. The viewport draws tiles from a source you
implement ([the image viewport](viewport.md)); in an eframe application
on wgpu, `gpu::install` turns its GPU path on (the viewport's
`examples/viewer.rs` is a whole eframe application without `App`).

```rust
use egui_pop::egui;
use egui_pop::viewport::source::{Pixels, Tile, TileRequest, TileSink, TileSource};
use egui_pop::viewport::{View, Viewport, ViewportState};

struct Grey(TileSink);

impl TileSource for Grey {
    fn size(&self) -> Option<[u32; 2]> { Some([600, 400]) }

    fn request(&self, requests: Vec<TileRequest>) {
        for r in requests {
            let n = r.size[0] as usize * r.size[1] as usize;
            self.0.deliver(Tile::answer(&r, Pixels::Scene(vec![0.18; n * 4])));
        }
    }
}

// In eframe's creation closure, on wgpu:
// egui_pop::viewport::gpu::install(&cc.egui_ctx, cc.wgpu_render_state.as_ref().unwrap());
let mut state = ViewportState::new();
let source = Grey(state.sink());
let mut view = View::fit();
let ctx = egui::Context::default();
let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
    Viewport::new("main").source(&source).show(ui, &mut state, &mut view);
});
out.textures_delta.clear();
```

The histogram, the curve editor, the colour wheels, the zoom control,
the navigator and the node graph are widgets of the same crate; their
`on_change` takes a plain closure. The `imaging` crate only turns their
changes into steps of an application.

```rust
use egui_pop::egui;
use egui_pop::viewport::change::Verdict;
use egui_pop::viewport::curve::CurveEditor;
use egui_pop::viewport::histogram::{Channel, Histogram};
use egui_pop::viewport::wheel::{ColorWheel, WheelValue};

fn tools(ui: &mut egui::Ui, bins: &[f32], points: &mut Vec<[f32; 2]>, lift: &mut WheelValue) {
    Histogram::new([Channel::new("Luma", bins, egui::Color32::GRAY)]).show(ui);
    CurveEditor::new(points, "Tone curve")
        // At most eight points.
        .on_change(|c| if c.to.len() > 8 { Verdict::Veto } else { Verdict::Allow })
        .show(ui);
    ColorWheel::offset(lift, "Lift").size(120.0).show(ui);
}

let ctx = egui::Context::default();
let bins = vec![0.5; 64];
let mut points = vec![[0.0, 0.0], [1.0, 1.0]];
let mut lift = WheelValue::default();
let mut out = ctx.run_ui(egui::RawInput::default(), |ui| tools(ui, &bins, &mut points, &mut lift));
out.textures_delta.clear();
```

## The tree, the list and the data table

With the `views` feature. Without `.app(..)` they report selection,
activation and edits in their output; the table keeps its sort and
widths in its `TableState` (`.store(..)` keeps them in a value store
instead). The [tree](tree.md) and [table](table.md) pages have more.

```rust
use egui_pop::egui;
use egui_pop::views::table::{Column, DataTable, TableRows, TableState};
use egui_pop::views::tree::{Node, Sink, TreeSource, TreeState, TreeView};

struct Folders;

impl TreeSource<String> for Folders {
    fn load(&self, _parent: Option<&String>, sink: Sink<String>) {
        sink.deliver(vec![Node::leaf("a".into(), "a.txt"), Node::leaf("b".into(), "b.txt")]);
    }
}

struct Files(Vec<(u64, String)>);

impl TableRows<u64> for Files {
    fn len(&self) -> usize { self.0.len() }
    fn key(&self, row: usize) -> u64 { self.0[row].0 }
    fn label(&self, row: usize) -> String { self.0[row].1.clone() }
    fn cell(&mut self, ui: &mut egui::Ui, row: usize, _column: usize, _selected: bool) {
        ui.label(&self.0[row].1);
    }
}

let ctx = egui::Context::default();
let mut tree = TreeState::<String>::new();
let mut table = TableState::new();
let mut files = Files(vec![(1, "a.txt".into()), (2, "b.txt".into())]);
let columns = [Column::new("name", "Name")];
let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
    TreeView::new("folders").label("Folders").show(ui, &mut tree, &Folders);
    let out = DataTable::new("files", &columns).label("Files").show(ui, &mut table, &mut files);
    if out.sorted { /* sort `files` by `table.sort()` */ }
});
out.textures_delta.clear();
```

## The text and code editors, the terminal

With the `editors` feature. The `EditorState` is a handle shared with
the application; without `.app(..)` the editors bind their own default
keys (Ctrl+F, Ctrl+Z, Ctrl+S...) and report changes and saves in their
output ([text and code editors](editors.md)). A terminal shows a
`Console` (logs, ANSI colours) or a program in a pseudo-terminal
(`editors-pty`).

```rust
use egui_pop::editors::terminal::Console;
use egui_pop::editors::{CodeEditor, EditorState, Signal, Terminal, TerminalState};
use egui_pop::egui;

let code = EditorState::new("main.rs", "fn main() {}\n").language("rs");
let console = Console::new();
console.line("\x1b[32mbuild finished\x1b[0m");
let log = TerminalState::new("log", console);

let ctx = egui::Context::default();
let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
    let out = CodeEditor::new().label("main.rs").height(200.0).show(ui, &code);
    for s in out.signals {
        if let Signal::Saved { .. } = s { /* write `code.text()` */ }
    }
    Terminal::new().label("Build log").show(ui, &log);
});
out.textures_delta.clear();
```

## Translations

With the `i18n` feature. An `I18n` holds Fluent catalogues and looks
text up on its own; installed as the context's strings provider, it
translates egui-pop's own text (field states, error cards, the editors'
find bar...). `i18n::install` puts it in an egui context: `t(ctx, ..)`,
`text(ctx, ..)` and `tr!` read it, and egui-pop's own text is
translated through its strings provider.

```rust
use egui_pop::egui;
use egui_pop::i18n::{I18n, args, install, t, tr};

let i18n = I18n::with_egui_pop("fr");
i18n.add("fr", "hello = Bonjour, { $name } !\n").unwrap();

let ctx = egui::Context::default();
install(&ctx, &i18n);
assert_eq!(t(&ctx, "hello", &args![name = "Ada"]), "Bonjour, Ada !");
assert_eq!(tr!("hello", name = "Ada"), "Bonjour, Ada !");
let said = egui_pop::core::strings::lookup(&ctx, "egui-pop-field-modified-saved", "Modified since saved");
assert_eq!(said, "Modifié depuis l’enregistrement");
```

The `I18n` is shared: the application switches the language with
`i18n.set_locale(..)`, seen at the next lookup. The language setting
and the `locale.changed` signal are `I18nPlugin`'s.

## Platform readers

With a `platform-*` feature. The services are plugins, but what they
read from the system is a function:

```rust
use egui_pop::platform::{dirs, focus, text_scale, wheel_lines};

let scale = text_scale::read();   // the desktop's text scaling, 1.0 when unknown
assert!(scale.factor > 0.0);
let _mode = focus::read().mode;   // click to focus, or focus follows the pointer
assert!(wheel_lines::read() >= 1);
let _logs = dirs::log_dir("org.example.app");
```

## A single instance

With the `platform-instance` feature. `instance::single` at the top of
`main` makes a later launch hand its arguments to the running one; the
running one takes them in its frame, woken when one arrives:

```rust,no_run
use egui_pop::eframe;
use egui_pop::egui;
use egui_pop::platform::instance::{self, Launches, Single};

struct Viewer {
    launches: Launches,
}

impl eframe::App for Viewer {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        for launch in self.launches.take() {
            egui_pop::platform::window::raise(ui.ctx());
            // open what `launch.args` names, from `launch.cwd`
            let _ = launch;
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let first = match instance::single("org.example.Viewer")? {
        Single::Handed => return Ok(()), // the running instance took over
        Single::First(first) => first,
    };
    let launches = first.launches();
    eframe::run_native(
        "Viewer",
        eframe::NativeOptions::default(),
        Box::new(move |cc| {
            // A launch handed over repaints the window.
            launches.set_waker(Some(cc.egui_ctx.clone()));
            Ok(Box::new(Viewer { launches }))
        }),
    )?;
    Ok(())
}
```

`InstancePlugin` adds the `instance.second_launch` and `deeplink.open`
steps and the scheme's links on top: those need the application.

## Without eframe

The facade's default features include eframe. An application on another
integration (bevy_egui, egui-winit with its own renderer) turns them off
and keeps the rest:

```toml
egui-pop = { version = "0.2", default-features = false, features = ["phosphor", "bundled-fonts"] }
```

## What needs the application

These parts are the framework itself: they read registries that only
an `App` builds (commands, menus, the keymap, settings, panels, steps),
or they are plugins.

| Part | Why it needs `App` |
|---|---|
| Commands, the keymap at run time, menus, toolbars, the command palette | all read the command registry and the keymap a plugin fills through `Ctx` |
| Dialogs, toasts, the settings window and shortcuts editor, tips, About | plugins drawing in the application's overlay and settings |
| Zones and blocks, the workbench, arrangements, the window's own frame, the status bar, strips | the application lays the window out |
| Steps and hooks (`before`, `replace`, `after`), `.app(..)` on a component, `egui-pop-imaging` | steps are the application's (ADR 0011); without it a component reports in its output |
| The host, history, the clipboard, links, jobs, the context stack, choosing files | services behind the host and the application's store |
| The debug bridge's commands and devtools | they describe and drive the application's registries |
| `I18nPlugin` and the language setting | a plugin with a setting; without it the application calls `I18n::set_locale` |
| Platform services: notifications, the tray, global shortcuts, logs, crash reports | plugins routing to commands and steps |
| A single instance's steps and deep links | `InstancePlugin` runs them as steps and commands; without it the application takes the launches (above) |
