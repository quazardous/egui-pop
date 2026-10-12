# egui-pop

An application framework and component library for
[egui](https://github.com/emilk/egui): themable, accessible, drivable by
agents, easy to debug. Not an official egui crate.

![The gallery, pop-dark: the theme overview](docs/screenshots/overview-pop-dark.png)

**Status: phase 2, the framework.** Design tokens and themes (phase 1);
plugins that own what they register, commands declared once, layered
keymaps resolved by focus, zones and blocks, overlays, a value store with
undo, persistence, a debug bridge and a test kit. Components come next.

## What is there

| Crate | Layer | What it holds |
|---|---|---|
| [`egui-pop-tokens`](crates/egui-pop-tokens) | L0 | DTCG token files: parser, resolver, themes, densities, accents, contrast checks. No egui. |
| [`egui-pop-model`](crates/egui-pop-model) | L1 | The logic without egui: typed ids, diagnostics, `when` clauses, commands, keymap (layers, chords, focus depth, traces), menus, panels, layouts, the value store, versioned documents, plugin ordering. |
| [`egui-pop-core`](crates/egui-pop-core) | L2 | The egui layer: theme, focus scopes, key dispatcher, overlay stack, placement, accessibility, strings, icons, persistence, fault boundaries, value binding, the debug bridge. |
| [`egui-pop-api`](crates/egui-pop-api) | L3 | The application API (its own semver): `Plugin`, `Ctx`, `AppBuilder` over eframe or headless, zones and blocks, hooks, services, stories. |
| [`egui-pop-viewport`](crates/egui-pop-viewport) | L3 | A generic image viewport (zoom in physical pixels, tiles from an asynchronous source, a display-transform hook, overlays with handles, before/after, a pixel readout), thumbnails and a histogram (feature `viewport` of the facade). |
| [`egui-pop-test`](crates/egui-pop-test) | L4 | Test kit: harnesses, "every interactive node has a name", accessibility tree dump. |
| [`egui-pop`](crates/egui-pop) | L5 | The crate to depend on: re-exports the layers, `egui`, and a `prelude`. |
| [`egui-pop-gallery`](crates/egui-pop-gallery) | app | The catalogue, itself a plugin application (not published). |

Layers only depend downwards; a test reading `cargo metadata` keeps it so
([ADR 0001](docs/adr/0001-layering.md)). Decisions are in
[docs/adr](docs/adr), principles in [CREDO.md](CREDO.md), what each
release promises in [docs/stability.md](docs/stability.md); the survey of
advanced widgets and how egui-pop gets them is
[docs/widgets.md](docs/widgets.md), and the plan for fonts
[docs/fonts.md](docs/fonts.md).

## A theme

```rust
use egui_pop::egui;
use egui_pop::theme::{self, Density, ThemeSpec, Tokens};

fn setup(ctx: &egui::Context) {
    let tokens = theme::install(
        ctx,
        &ThemeSpec::new("pop-dark").density(Density::Compact).accent("blue"),
    );
    for problem in tokens.resolved().problems() {
        eprintln!("theme: {problem}");
    }
}

fn draw(ui: &mut egui::Ui) {
    // egui's widgets follow the theme; custom painting reads the tokens.
    let tokens = Tokens::of(ui.ctx());
    ui.colored_label(tokens.status.success, "Saved");
}

setup(&egui::Context::default());
```

| `pop-dark` | `pop-light` | `pop-high-contrast` |
|---|---|---|
| ![pop-dark](docs/screenshots/overview-pop-dark.png) | ![pop-light](docs/screenshots/overview-pop-light.png) | ![pop-high-contrast](docs/screenshots/overview-pop-high-contrast.png) |

Built-in themes: `pop-dark`, `pop-light` and `pop-high-contrast`, and
twelve editor-inspired ones (GitHub, Catppuccin, One, Solarized and
Gruvbox families, light and dark, and GitHub's high-contrast pair;
sources and licences in [NOTICE](NOTICE));
densities `comfortable` and `compact`; accents `gold`, `blue`, `green`,
`amber` and `red`; reduced motion. See [docs/tokens.md](docs/tokens.md).

## A plugin application

A plugin registers through `Ctx`; whatever it registers is removed when
it is disconnected. A command is declared once: its key binding, menu
entry, palette entry and agent surface are derived from it.

```rust
use egui_pop::api::app::MENU_VIEW;
use egui_pop::egui;
use egui_pop::prelude::*;

struct Counter;

const COUNT: SettingId = SettingId::from_static("counter.count");

impl Plugin for Counter {
    fn id(&self) -> PluginId {
        PluginId::from_static("counter")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.setting(COUNT, Descriptor::int("Count", 0, 100, 0))?;
        ctx.command(
            CommandSpec::new(CommandId::from_static("counter.add"), "Add one", "Counter")
                .key("Ctrl+Up")
                .menu(MENU_VIEW, "5_counter", 0),
            |call| {
                let store = call.app.store();
                let mut store = store.lock().unwrap();
                let set = egui_pop::model::values::Set::new(
                    "settings/counter.count",
                    egui_pop::model::values::Change::Step(1),
                    call.origin.clone(),
                );
                store.set(set).map(|a| a.value.to_json()).map_err(|e| e.to_string())
            },
        )?;
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("counter.view"), 0, |ui, app| {
                let store = app.store();
                let n: i64 = egui_pop::core::binding::read(&store, "settings/counter.count").unwrap_or(0);
                ui.heading(format!("Count: {n}"));
                if ui.button("Add one").clicked() {
                    app.run(CommandId::from_static("counter.add"), serde_json::Value::Null);
                }
            }),
        );
        Ok(())
    }
}

// Headless, as in tests; `.run_native("Counter", options)` opens a window.
let ctx = egui::Context::default();
let mut app = App::builder().plugin(Counter).build(&ctx).unwrap();
assert!(app.handle().diagnostics().is_empty());
let out = app.run(&CommandId::from_static("counter.add"), serde_json::Value::Null, Origin::User);
// Run by the default host, in-process: the result is there now.
assert_eq!(out.now(), Some(Ok(serde_json::json!(1))));
```

Zones and blocks let a plugin add to, replace, extend or wrap what
another drew ([ADR 0005](docs/adr/0005-zones-and-blocks.md)); what cannot
apply is reported, never silently dropped
([ADR 0003](docs/adr/0003-no-silent-no-op.md)). In the gallery, one
plugin replaces, extends and wraps another's blocks; switched off, the
card is back as its owner made it:

| Extender connected | Extender switched off (`pop-light`) |
|---|---|
| ![zones, extended](docs/screenshots/zones-pop-dark.png) | ![zones, as declared](docs/screenshots/zones-pop-light-off.png) |

Every command is in the palette (`Ctrl+Shift+P` or `F1`, a plugin in
`DefaultPlugins`), ranked by a fuzzy match with the recent ones first,
with the keys the keymap really binds; a command taking arguments asks
for them:

![the command palette](docs/screenshots/palette-pop-dark.png)

The components of the paved road, each a plugin in `DefaultPlugins`:
dialogs (here a form from value descriptors), toasts with actions, the
settings window with its shortcuts editor, and the parameter widgets:

| Dialogs | Toasts (`pop-light`) |
|---|---|
| ![a form dialog](docs/screenshots/dialogs-pop-dark.png) | ![toasts](docs/screenshots/toasts-pop-light.png) |

| Settings | Shortcuts |
|---|---|
| ![the settings window](docs/screenshots/settings-window-pop-dark.png) | ![the shortcuts editor](docs/screenshots/shortcuts-pop-dark.png) |

![sliders and parameter widgets](docs/screenshots/values-pop-dark.png)

## An image viewport

`egui-pop-viewport` (the facade's `viewport` feature) shows an image the
application computes, in tiles: pan and zoom in physical pixels (100 %
is one image pixel per screen pixel, in every window), a proxy first and
exact tiles after, a display transform where colour management plugs in,
overlays with handles, before and after, a pixel readout. The
application implements the tile source; the viewport asks, never waits,
and says what changed with signals.

```rust
use egui_pop::egui;
use egui_pop::viewport::source::{Pixels, Tile, TileRequest, TileSink, TileSource};
use egui_pop::viewport::{Signal, View, Viewport, ViewportState};

/// A flat mid-grey image, answered at once (a real source works on a
/// thread and delivers through the sink when done).
struct Grey {
    sink: TileSink,
}

impl TileSource for Grey {
    fn size(&self) -> Option<[u32; 2]> {
        Some([6000, 4000])
    }

    fn request(&self, requests: Vec<TileRequest>) {
        for r in requests {
            let n = r.size[0] as usize * r.size[1] as usize;
            // Scene-referred floats: the display transform runs on them.
            self.sink.deliver(Tile::answer(&r, Pixels::Scene(vec![0.18; n * 4])));
        }
    }
}

fn draw(ui: &mut egui::Ui, state: &mut ViewportState, view: &mut View, source: &Grey) {
    let out = Viewport::new("main").source(source).readout(true).show(ui, state, view);
    for signal in out.signals {
        if let Signal::View { view, .. } = signal {
            // Keep the view in the document, one undo step per gesture...
            let _ = view;
        }
    }
}

let state = ViewportState::new();
let source = Grey { sink: state.sink() };
let _ = (draw, source);
```

![the viewport page: before and after, overlays](docs/screenshots/viewport-pop-dark.png)

## Start an application

A [cargo-generate](https://github.com/cargo-generate/cargo-generate)
template makes a new application: one plugin with a panel, a setting and
a command, over `DefaultPlugins`.

```sh
cargo generate --git https://github.com/quazardous/egui-pop template
```

It is the `minimal` example (`cargo run -p egui-pop --example minimal`),
kept identical to it by a test.

## Debugging

Every registry describes itself to the **debug bridge**: collectors for
commands, key resolution traces, focus, overlays, zones, plugins, the
store and diagnostics, each with a JSON snapshot and actions flagged
`mutates`; every call returns a feedback block. The gallery's Debug page
shows them, with the last key resolutions:

![the debug page](docs/screenshots/debug-pop-dark.png)

```rust
use egui_pop::egui;
use egui_pop::prelude::*;

let ctx = egui::Context::default();
let mut app = App::builder().plugins(DefaultPlugins).build(&ctx).unwrap();
let snapshot = app.handle().debug_snapshot();
assert!(snapshot["commands"]["commands"].is_array());
let feedback = app.debug_call("commands", "run", &serde_json::json!({ "id": "app.palette" }));
assert!(feedback.ok && feedback.mutates);
```

## The gallery

```sh
cargo run -p egui-pop-gallery -- --theme pop-light --page zones
cargo run -p egui-pop-gallery -- --help
```

Pages: theme overview, tokens, egui's built-ins, icons, values, menus, dialogs,
toasts, the image viewport, zones and blocks, settings, the debug bridge. It also builds for the web
(`wasm32-unknown-unknown`, with Trunk from `crates/egui-pop-gallery`), and
runs offscreen in a nested compositor for scripted runs
([tools/wbox](tools/wbox)).

## Versions

egui-pop has its own 0.x version; one egui-pop minor per egui minor,
released within days of it.

| egui-pop | egui | Rust |
|---|---|---|
| 0.0.x | 0.36 | 1.95 |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md), [AGENTS.md](AGENTS.md) and
[CHANGELOG.md](CHANGELOG.md).

## License

MIT, see [LICENSE](LICENSE).
