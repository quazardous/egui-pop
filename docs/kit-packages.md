# Kits as packages

How a kit ([Visual kits](kit.md)) is shipped and added to an application:
a folder of data, embedded in the binary or read at run time, plus code
when the kit draws shapes of its own.

## The folder

```text
underline-tabs/
  kit.yaml          a DTCG token file, like a theme (kit.json is read too)
  *.tokens.yaml     optional: more token files (one per component;
                    .tokens.json is read too)
  fonts/            optional: .ttf, .otf or .ttc faces
  LICENSE           optional: the kit's licence and its fonts'
  icons/            reserved for SVG icons, not read yet (reported)
```

`kit.yaml` is a theme file, in YAML or JSON ([tokens.md](tokens.md#yaml-or-json)): `$description`, `$extends` (`pop-dark` when
not given) and tokens in the vocabulary of [kit.md](kit.md)
(`tab.shape`, `tab.hover.fill`), plus any system or palette token a
theme can give. Other `*.tokens.yaml` (or `.json`) files of the folder are read in
name order and merged into it; a token given twice keeps its first
value, and only `kit.yaml` says what the kit extends.

The manifest lives in `kit.yaml`'s root `$extensions`, so the file stays
plain DTCG:

```yaml
$description: "Our product's look."
$extends: "pop-dark"
$extensions:
  org.egui-pop.kit:
    name: "brand"
    label: "Brand"
    fonts:
      - {file: "JetBrainsMono-Regular.ttf", family: "JetBrains Mono", weight: 400}
font:
  family:
    mono:
      $type: "fontFamily"
      $value: ["JetBrains Mono", "monospace"]
```

Every key is optional. `name` is the kit's theme name (the folder's name
otherwise; a built-in theme's name gets `-kit` added); `label` is its
name for people.

**Faces.** A face's family and weight come from its manifest entry, or
else from its file name: `Family-Weight.ext`, the weight a CSS name
(`Thin`, `ExtraLight`, `Light`, `Regular`, `Medium`, `SemiBold`, `Bold`,
`ExtraBold`, `Black` and their usual synonyms) or a number
(`Brand-700.otf`); `Family.ext` is 400; a variable font's axes in
brackets are dropped (`Inter[wght].ttf` is `Inter` at 400, its axis
serving every weight, see [fonts.md](fonts.md)). A family whose name has
spaces needs the manifest. The kit's tokens name the families
(`font.family.ui`); its faces are registered through
`egui_pop_core::fonts`, so a family is found in the kit before the
system.

## Reading a kit

`egui_pop_tokens::kit` reads a kit; nothing in it panics:

| | |
|---|---|
| `KitData::load(dir)` | a folder at run time, its YAML through the default cache (`KitData::load_with(dir, &cache)` names one) |
| `kit!("dir", tokens = [..], fonts = [..], license = "LICENSE")` | embedded in the binary (`include_str!`/`include_bytes!`, the path relative to the calling file); only `kit.yaml` is implied (`kit = "kit.json"` names another), the other files are listed |
| `KitData::from_text(name, text)` | a data-only kit from its `kit.yaml` (or JSON) text |
| `kit::discover(dir)` | each sub-folder holding a `kit.yaml`, `kit.yml` or `kit.json` (`discover_with(dir, &cache)`) |
| `kit::check(&kit)` | every problem: reading it, and resolving its theme |

What is wrong is in `KitData::problems()`, and the kit is usable anyway
(ADR 0003):

- `kit.yaml` unreadable, not YAML or JSON, or holding an unquoted
  `#colour` or `{alias}`: the kit is its base theme under its
  name; another token file that cannot be read is left out;
- a token of a kit component outside the vocabulary (a misspelt term or
  state, a per-state term without its state, a term of the wrong type),
  or shaped `component.state.term` for a component no kit has: kept,
  read by nothing, reported;
- a face named by the manifest and missing, or a file that is not a
  font: left out (the families naming it fall back);
- an `icons/` folder: not read yet;
- a token file whose compiled form cannot be kept in the cache
  (`NotCached`): read uncached, parsed again next time.

**Compile-time validation.** `kit!` is a `macro_rules` macro: it embeds,
it does not check (that needs a proc-macro). An application checks its
embedded kits in a test, which also finds the faces its manifest names
that the macro was not given:

```rust,ignore
#[test]
fn our_kit_is_clean() {
    let kit = egui_pop::tokens::kit!("../kits/brand", fonts = ["Inter-Regular.ttf"]);
    assert!(egui_pop::tokens::kit::check(&kit).is_empty());
}
```

From the command line:

```sh
cargo run -p egui-pop-tokens --example kit-check -- path/to/kit
```

prints the kit's name, base, faces, licence and problems, and exits with
1 when there are problems.

## Adding a kit to an application

One line: `AppBuilder::kit` takes a folder read at run time, a `kit!`,
or a kit crate's `KitSource`. The kit's theme joins the themes under
its name; its faces and handlers are registered before the theme is
installed (at start-up, never per frame). Pick it as any theme:

```rust
use egui_pop::egui;
use egui_pop::prelude::*;

let kit = egui_pop::tokens::kit::KitData::from_text(
    "underline",
    r#"{ "tab": { "shape": { "$type": "string", "$value": "underline" } } }"#,
);
let ctx = egui::Context::default();
let app = App::builder()
    .kit(kit)
    .theme(ThemeSpec::new("underline"))
    .build(&ctx)
    .unwrap();
assert_eq!(egui_pop::core::kit::Kit::of(&ctx).tab.shape, "underline");
assert_eq!(app.handle().theme().kits()[0].label, "underline");
```

A kit's problems are logged and in the diagnostics (`kit-problem`, the
kit's name as subject); the application builds anyway.
`ThemeService::kits()` lists the kits added (name, label, description,
origin) for a theme picker; switching to one is `theme().set(...)`.

## A kit with code

A kit whose `shape` tokens name handlers of its own is a crate exposing
one function:

```rust,ignore
use std::sync::Arc;
use egui_pop::core::kit::KitSource;

pub fn kit() -> KitSource {
    KitSource::new(egui_pop::tokens::kit!("../kit"))
        .shape("tab", "notch", Arc::new(Notch))
}
```

`Notch` implements `kit::Primitive<ComponentKit>` ([kit.md](kit.md)).
The application adds it with `.kit(brand_kit::kit())`.
`KitSource::register(ctx)` registers its faces (`fonts::register`) and
handlers (`kit::register_shape`); a handler for a component that does
not exist is reported and not registered. Without `AppBuilder`,
`register`, `Themes::add_kit` and `theme::install_with` are the three
steps.

## User kits

`AppBuilder::user_kits(true)` also reads the data-only kit folders the
user put under the persistence directory (`AppBuilder::persist`):
`<persist>/kits/<kit>/kit.yaml`. They come after the application's own
kits (a user kit of the same name replaces it) and are listed by
`ThemeService::kits()` with their folder as origin. A user kit is data
only: code comes only from crates the application links.

### The cache of compiled files

A kit read at run time is YAML, which costs several times JSON to parse.
Its files are compiled once into a cache folder, never next to the kit:
read again unchanged, a file is read from its compiled JSON (the default
kit's 76 files as a folder: 6.8 ms parsed, 2.8 ms from the cache, release
build). An entry is keyed by the file's path, a hash of its contents and
the compiler's version, so an edited file, or a new egui-pop, compiles it
again. The checks run on what is read either way, so a kit's problems
are the same on a hit.

The cache lives in the platform's cache directory under the
application's id (`~/.cache/<id>/egui-pop` on Linux,
`~/Library/Caches/<id>/egui-pop` on macOS,
`%LOCALAPPDATA%\<id>\cache\egui-pop` on Windows). egui-pop does not know
the id, so it is off until the application gives one, for every reader
at once or for the application's user kits:

```rust
use egui_pop::tokens::cache::{self, Cache};

cache::set_default(Cache::for_app("org.example.App"));
# cache::set_default(Cache::off());
let _builder = egui_pop::prelude::App::builder()
    .user_kits(true)
    .cache(Cache::for_app("org.example.App"));
```

`Cache::at(dir)` puts it elsewhere (a test's temporary folder),
`Cache::off()` turns it off. A cache that cannot be written is not an
error: the kit is read uncached, with a `NotCached` problem. The store
is not specific to tokens (`Cache::get`, `Cache::put` with a
`Compiler`'s name and version): token files are its first user.

## The default kit

egui-pop's own look is a kit folder too: `crates/egui-pop-tokens/themes/kit/`
(a `kit.yaml` and one token file per component), read by the same code
as any embedded kit and layered into `pop-dark`, which every built-in
theme extends. It is not a theme of its own, because it is the base all
others fall back to. A test reads that folder with `KitData::load` and
checks it holds exactly the built-in kit's tokens.

## Example

[`examples/kits/underline-tabs`](../examples/kits/underline-tabs) is a
data-only kit: underlined tabs on a sunken bar. The gallery's
`workbench-kit-underline` snapshot is drawn from it.
