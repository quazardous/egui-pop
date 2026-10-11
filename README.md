# egui-pop

Themable, accessible, agent-drivable components for
[egui](https://github.com/emilk/egui). Not an official egui crate.

**Status: phase 1, tokens and theme.** Design tokens, three built-in
themes, densities and accents, generated into egui's own style, with
contrast checks and a gallery. No components yet: they come next.

## What is there

| Crate | What it holds |
|---|---|
| [`egui-pop-tokens`](crates/egui-pop-tokens) | DTCG token files: parser, resolver (aliases, `$extends`, cycle and type errors, fallbacks), themes, densities, accents, the WCAG contrast checks. No egui dependency. |
| [`egui-pop`](crates/egui-pop) | The crate to depend on. `theme`: resolved tokens into egui's `Visuals` and `Style`, installed in a `Context`, with a typed `Tokens` set for custom widgets. Re-exports `egui`. |
| [`egui-pop-gallery`](crates/egui-pop-gallery) | A catalogue application (not published): every page under every theme, density and accent. Its pages are the snapshot fixtures. |

Built-in themes: `pop-dark` (warm greys, a gold accent), `pop-light` and
`pop-high-contrast`; densities `comfortable` and `compact`; accents `gold`,
`blue`, `green`, `amber` and `red`; reduced motion. See
[docs/tokens.md](docs/tokens.md) for the tiers, the naming rules and how to
add a theme.

```rust
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
```

## The gallery

```sh
cargo run -p egui-pop-gallery -- --theme pop-light --page builtins
cargo run -p egui-pop-gallery -- --help
```

It also builds for the web (`wasm32-unknown-unknown`, with Trunk from
`crates/egui-pop-gallery`), and runs offscreen in a nested compositor for
scripted runs ([tools/wbox](tools/wbox)).

## Where it is going

- **Components**: a slider with a soft and a hard range, parameter
  widgets, confirmations, forms, pickers, toasts, menus built from
  declared commands, a panel strip with indicators, a node graph view.
- **Plain or bound**: every widget works on a `&mut` value like egui's
  own, or bound to an observable source of values.
- **Accessible and agent-drivable**: roles, names, values and actions on
  every widget, keyboard patterns, contrast checked per theme.
- **Easy to debug**: an overlay with ids, bindings and resolved tokens.
- **Tested headless** with egui_kittest, snapshots per theme and state.

## Versions

One egui-pop minor per egui minor, released within days of it.

| egui-pop | egui | Rust |
|---|---|---|
| 0.0.x | 0.36 | 1.95 |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and [CHANGELOG.md](CHANGELOG.md).

## License

MIT, see [LICENSE](LICENSE).
