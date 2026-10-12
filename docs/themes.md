# Themes

How to pick a theme, adjust it, and read it when painting. How tokens
are written, resolved and checked is in [Design tokens](tokens.md).

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

In an application, give the spec to `AppBuilder::theme`; the theme
service (`AppHandle::theme`) switches it while running, installed before
the next frame:

```rust
use egui_pop::egui;
use egui_pop::prelude::*;

let app = App::builder()
    .theme(ThemeSpec::new("pop-light").reduced_motion(true))
    .build(&egui::Context::default())
    .unwrap();
app.handle().theme().set(ThemeSpec::new("gruvbox-dark").accent("amber"));
```

## What can be chosen

- **Themes**: `pop-dark`, `pop-light` and `pop-high-contrast`, and twelve
  editor-inspired ones (GitHub, Catppuccin, One, Solarized and Gruvbox
  families, light and dark, and GitHub's high-contrast pair; sources and
  licences in [NOTICE](../NOTICE)).
- **Densities**: `comfortable` and `compact`.
- **Accents**: `gold`, `blue`, `green`, `amber` and `red`.
- **Reduced motion**.
- **Any token**, overridden by its path (`override_token`).

See [Design tokens](tokens.md) for the order a theme is resolved in and
how to write a theme of your own.

| `pop-dark` | `pop-light` | `pop-high-contrast` |
|---|---|---|
| ![pop-dark](screenshots/overview-pop-dark.png) | ![pop-light](screenshots/overview-pop-light.png) | ![pop-high-contrast](screenshots/overview-pop-high-contrast.png) |

## The built-in themes

Each one as the gallery's overview page shows it, reduced.

<table>
  <tr>
    <td align="center"><img src="screenshots/themes/pop-dark.png" width="200" alt="pop-dark"><br><code>pop-dark</code></td>
    <td align="center"><img src="screenshots/themes/pop-light.png" width="200" alt="pop-light"><br><code>pop-light</code></td>
    <td align="center"><img src="screenshots/themes/pop-high-contrast.png" width="200" alt="pop-high-contrast"><br><code>pop-high-contrast</code></td>
  </tr>
  <tr>
    <td align="center"><img src="screenshots/themes/github-light.png" width="200" alt="github-light"><br><code>github-light</code></td>
    <td align="center"><img src="screenshots/themes/github-dark.png" width="200" alt="github-dark"><br><code>github-dark</code></td>
    <td align="center"><img src="screenshots/themes/github-light-high-contrast.png" width="200" alt="github-light-high-contrast"><br><code>github-light-high-contrast</code></td>
  </tr>
  <tr>
    <td align="center"><img src="screenshots/themes/github-dark-high-contrast.png" width="200" alt="github-dark-high-contrast"><br><code>github-dark-high-contrast</code></td>
    <td align="center"><img src="screenshots/themes/catppuccin-latte.png" width="200" alt="catppuccin-latte"><br><code>catppuccin-latte</code></td>
    <td align="center"><img src="screenshots/themes/catppuccin-mocha.png" width="200" alt="catppuccin-mocha"><br><code>catppuccin-mocha</code></td>
  </tr>
  <tr>
    <td align="center"><img src="screenshots/themes/one-light.png" width="200" alt="one-light"><br><code>one-light</code></td>
    <td align="center"><img src="screenshots/themes/one-dark-pro.png" width="200" alt="one-dark-pro"><br><code>one-dark-pro</code></td>
    <td align="center"><img src="screenshots/themes/solarized-light.png" width="200" alt="solarized-light"><br><code>solarized-light</code></td>
  </tr>
  <tr>
    <td align="center"><img src="screenshots/themes/solarized-dark.png" width="200" alt="solarized-dark"><br><code>solarized-dark</code></td>
    <td align="center"><img src="screenshots/themes/gruvbox-light.png" width="200" alt="gruvbox-light"><br><code>gruvbox-light</code></td>
    <td align="center"><img src="screenshots/themes/gruvbox-dark.png" width="200" alt="gruvbox-dark"><br><code>gruvbox-dark</code></td>
  </tr>
</table>

The images are refreshed with the others (`cargo xtask screenshots`, see
the [README](../README.md#refreshing-the-screenshots)).
