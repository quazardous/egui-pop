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

## Light and dark together

A theme may name its counterpart, the other variant of it, light or
dark: its token file says so at the root
(`$extensions: {"org.egui-pop.variant-of": "pop-dark"}`, on one side or
both), or the spec does (`ThemeSpec::pair(dark, light)`,
`ThemeSpec::counterpart`). The spec's density, accent, reduced motion
and overrides apply to both. Every built-in theme with a light and a
dark version is paired with it; `pop-high-contrast` is alone.

`install` then installs both: each in its own egui style (dark or
light), with its own kit, and `Tokens::of` and `Kit::of` give the ones
of egui's theme of the moment, read at draw time. The theme named first
is shown, as for a theme alone; `theme::set_preference` (or egui's own
`ctx.set_theme`) switches to the other, or follows the system's light
or dark mode, without installing anything again:

```rust
use egui_pop::egui;
use egui_pop::theme::{self, ThemeSpec, Tokens};

let ctx = egui::Context::default();
let dark = theme::install(&ctx, &ThemeSpec::new("gruvbox-dark"));
assert!(theme::is_paired(&ctx));
theme::set_preference(&ctx, egui::ThemePreference::Light);
assert_eq!(Tokens::of(&ctx).resolved().spec().base, "gruvbox-light");
assert_ne!(Tokens::of(&ctx).surface.panel, dark.surface.panel);
// Follows the system; the theme named first when it says nothing.
theme::set_preference(&ctx, egui::ThemePreference::System);

// Any two themes, or one alone (its mode is then forced).
theme::install(&ctx, &ThemeSpec::pair("pop-high-contrast", "github-light"));
theme::install(&ctx, &ThemeSpec::new("pop-dark").alone());
assert!(!theme::is_paired(&ctx));
```

A theme alone keeps today's behaviour: both of egui's styles are its,
and its mode is forced whatever is asked. A widget reading `Tokens::of`
or `Kit::of` while drawing follows a switch; anything keeping what it
drew from them across frames keys on the kit's address
(`Arc::as_ptr(&Kit::of(ctx))`) or on the colours themselves. Fonts and
icon sizes come from the theme named first.

In an application, the theme service keeps the preference, the
`appearance.theme_preference` setting (`system`, `light`, `dark`; in
the settings' Appearance section): `ThemeService::preference`,
`set_preference`, `shown` (the theme drawn) and `toggle`, the
`view.theme.toggle_light_dark` command (View menu). Picking a theme
(`ThemeService::pick`) shows it: when the preference would show its
counterpart, the preference becomes the picked theme's mode. Without a
counterpart, the preference changes nothing.

## What can be chosen

- **Themes**: `pop-dark`, `pop-light` and `pop-high-contrast`, and twelve
  editor-inspired ones (GitHub, Catppuccin, One, Solarized and Gruvbox
  families, light and dark, and GitHub's high-contrast pair; sources and
  licences in [NOTICE](../NOTICE)).
- **Densities**: `comfortable` and `compact`.
- **Accents**: `gold`, `blue`, `green`, `amber` and `red`, a theme's
  own scales, or any colour: `ThemeSpec::accent_color(seed)` generates
  the accent's scale from it, for light and dark
  ([Design tokens](tokens.md#a-scale-from-one-colour)).
- **Reduced motion**.
- **Any token**, overridden by its path (`override_token`).

See [Design tokens](tokens.md) for the order a theme is resolved in and
how to write a theme of your own, and [the theme editor](theme-editor.md)
to edit one live and export it.

Every built-in theme is shown at full size [below](#the-built-in-themes).

## The built-in themes

Each one as the gallery's overview page shows it, at full size (a
1280x800 window, one pixel per point); click an image to open it alone.

### `pop-dark`

<a href="screenshots/themes/pop-dark.png"><img src="screenshots/themes/pop-dark.png" width="100%" alt="The gallery's overview in pop-dark"></a>

### `pop-light`

<a href="screenshots/themes/pop-light.png"><img src="screenshots/themes/pop-light.png" width="100%" alt="The gallery's overview in pop-light"></a>

### `pop-high-contrast`

<a href="screenshots/themes/pop-high-contrast.png"><img src="screenshots/themes/pop-high-contrast.png" width="100%" alt="The gallery's overview in pop-high-contrast"></a>

### `github-light`

<a href="screenshots/themes/github-light.png"><img src="screenshots/themes/github-light.png" width="100%" alt="The gallery's overview in github-light"></a>

### `github-dark`

<a href="screenshots/themes/github-dark.png"><img src="screenshots/themes/github-dark.png" width="100%" alt="The gallery's overview in github-dark"></a>

### `github-light-high-contrast`

<a href="screenshots/themes/github-light-high-contrast.png"><img src="screenshots/themes/github-light-high-contrast.png" width="100%" alt="The gallery's overview in github-light-high-contrast"></a>

### `github-dark-high-contrast`

<a href="screenshots/themes/github-dark-high-contrast.png"><img src="screenshots/themes/github-dark-high-contrast.png" width="100%" alt="The gallery's overview in github-dark-high-contrast"></a>

### `catppuccin-latte`

<a href="screenshots/themes/catppuccin-latte.png"><img src="screenshots/themes/catppuccin-latte.png" width="100%" alt="The gallery's overview in catppuccin-latte"></a>

### `catppuccin-mocha`

<a href="screenshots/themes/catppuccin-mocha.png"><img src="screenshots/themes/catppuccin-mocha.png" width="100%" alt="The gallery's overview in catppuccin-mocha"></a>

### `one-light`

<a href="screenshots/themes/one-light.png"><img src="screenshots/themes/one-light.png" width="100%" alt="The gallery's overview in one-light"></a>

### `one-dark-pro`

<a href="screenshots/themes/one-dark-pro.png"><img src="screenshots/themes/one-dark-pro.png" width="100%" alt="The gallery's overview in one-dark-pro"></a>

### `solarized-light`

<a href="screenshots/themes/solarized-light.png"><img src="screenshots/themes/solarized-light.png" width="100%" alt="The gallery's overview in solarized-light"></a>

### `solarized-dark`

<a href="screenshots/themes/solarized-dark.png"><img src="screenshots/themes/solarized-dark.png" width="100%" alt="The gallery's overview in solarized-dark"></a>

### `gruvbox-light`

<a href="screenshots/themes/gruvbox-light.png"><img src="screenshots/themes/gruvbox-light.png" width="100%" alt="The gallery's overview in gruvbox-light"></a>

### `gruvbox-dark`

<a href="screenshots/themes/gruvbox-dark.png"><img src="screenshots/themes/gruvbox-dark.png" width="100%" alt="The gallery's overview in gruvbox-dark"></a>

The images are refreshed with the others (`cargo xtask screenshots`, see
the [README](../README.md#refreshing-the-screenshots)).
