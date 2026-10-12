# The gallery

The gallery (`crates/egui-pop-gallery`, not published) is the catalogue
of egui-pop, itself a plugin application: each page is a story a plugin
registers.

```sh
cargo run -p egui-pop-gallery -- --theme pop-light --page zones
cargo run -p egui-pop-gallery -- --help
```

Options: `--theme`, `--density` (`comfortable` or `compact`),
`--accent`, `--reduced-motion`, `--page` (the page shown first),
`--lang` (the language setting's default: `en`, `fr` or `system`),
`--quit-after <frames>` and `--dump-debug` for scripted runs.

Pages: `overview` (the theme overview), `tokens`, `builtins` (egui's
built-ins), `icons`, `values`, `menus`, `dialogs`, `toasts`, `viewport`
(the image viewport), `images`, `zones` (zones and blocks), `workbench`,
`arrangements`, `strips`, `frame`, `scale`, `settings`, `i18n`
(translations) and `debug` (the debug bridge). `Alt+Down` and `Alt+Up` go to the next and previous page,
`Ctrl+Shift+T` to the next theme.

It also builds for the web (`wasm32-unknown-unknown`, with Trunk from
`crates/egui-pop-gallery`), and runs offscreen in a nested compositor
for scripted runs and screenshots ([tools/wbox](../tools/wbox)). Its
pages are also the snapshot tests (see [Testing](testing.md)).
