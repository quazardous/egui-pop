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
`--quit-after <frames>` and `--dump-debug` for scripted runs, and
`--platform` (the desktop's own platform services: a single instance
answering `gallery://page/<name>` links, notifications, a tray icon, the
global shortcut Ctrl+Alt+G, logs and crash reports in the user's state
directory; without it they are simulated and nothing reaches the
desktop).

Pages: `overview` (the theme overview), `tokens`, `builtins` (egui's
built-ins), `icons`, `values`, `menus`, `dialogs`, `toasts`, `viewport`
(the image viewport), `images`, `tree` (the live tree), `table` (lists
and tables), `zones` (zones and blocks), `workbench`,
`arrangements`, `strips`, `frame`, `scale`, `platform` (the platform
services), `settings`, `i18n` (translations) and `debug` (the debug
bridge). `Alt+Down` and `Alt+Up` go to the next and previous page,
`Ctrl+Shift+T` to the next theme.

It also builds for the web (`wasm32-unknown-unknown`, with Trunk from
`crates/egui-pop-gallery`), and runs offscreen in a nested compositor
for scripted runs and screenshots ([tools/wbox](../tools/wbox)). Its
pages are also the snapshot tests (see [Testing](testing.md)).

## Screenshots

Every snapshot the gallery's tests keep, page by page. They are
regenerated with the tests (see [Testing](testing.md)), so they show
the current state. Click one for its full size.

### overview

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/overview-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-pop-dark.png" alt="overview-pop-dark" width="100%"></a>

`pop-dark@2x`

<a href="../crates/egui-pop-gallery/tests/snapshots/overview-pop-dark@2x.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-pop-dark@2x.png" alt="overview-pop-dark@2x" width="100%"></a>

`pop-high-contrast`

<a href="../crates/egui-pop-gallery/tests/snapshots/overview-pop-high-contrast.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-pop-high-contrast.png" alt="overview-pop-high-contrast" width="100%"></a>

`pop-light`

<a href="../crates/egui-pop-gallery/tests/snapshots/overview-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-pop-light.png" alt="overview-pop-light" width="100%"></a>

<details><summary>The other built-in themes</summary>

`catppuccin-latte`

<a href="../crates/egui-pop-gallery/tests/snapshots/overview-catppuccin-latte.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-catppuccin-latte.png" alt="overview-catppuccin-latte" width="100%"></a>

`catppuccin-mocha`

<a href="../crates/egui-pop-gallery/tests/snapshots/overview-catppuccin-mocha.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-catppuccin-mocha.png" alt="overview-catppuccin-mocha" width="100%"></a>

`github-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/overview-github-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-github-dark.png" alt="overview-github-dark" width="100%"></a>

`github-dark-high-contrast`

<a href="../crates/egui-pop-gallery/tests/snapshots/overview-github-dark-high-contrast.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-github-dark-high-contrast.png" alt="overview-github-dark-high-contrast" width="100%"></a>

`github-light`

<a href="../crates/egui-pop-gallery/tests/snapshots/overview-github-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-github-light.png" alt="overview-github-light" width="100%"></a>

`github-light-high-contrast`

<a href="../crates/egui-pop-gallery/tests/snapshots/overview-github-light-high-contrast.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-github-light-high-contrast.png" alt="overview-github-light-high-contrast" width="100%"></a>

`gruvbox-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/overview-gruvbox-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-gruvbox-dark.png" alt="overview-gruvbox-dark" width="100%"></a>

`gruvbox-light`

<a href="../crates/egui-pop-gallery/tests/snapshots/overview-gruvbox-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-gruvbox-light.png" alt="overview-gruvbox-light" width="100%"></a>

`one-dark-pro`

<a href="../crates/egui-pop-gallery/tests/snapshots/overview-one-dark-pro.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-one-dark-pro.png" alt="overview-one-dark-pro" width="100%"></a>

`one-light`

<a href="../crates/egui-pop-gallery/tests/snapshots/overview-one-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-one-light.png" alt="overview-one-light" width="100%"></a>

`solarized-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/overview-solarized-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-solarized-dark.png" alt="overview-solarized-dark" width="100%"></a>

`solarized-light`

<a href="../crates/egui-pop-gallery/tests/snapshots/overview-solarized-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-solarized-light.png" alt="overview-solarized-light" width="100%"></a>

</details>

### tokens

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/tokens-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/tokens-pop-dark.png" alt="tokens-pop-dark" width="100%"></a>

### builtins

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/builtins-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/builtins-pop-dark.png" alt="builtins-pop-dark" width="100%"></a>

`pop-dark-compact`

<a href="../crates/egui-pop-gallery/tests/snapshots/builtins-pop-dark-compact.png"><img src="../crates/egui-pop-gallery/tests/snapshots/builtins-pop-dark-compact.png" alt="builtins-pop-dark-compact" width="100%"></a>

`pop-high-contrast`

<a href="../crates/egui-pop-gallery/tests/snapshots/builtins-pop-high-contrast.png"><img src="../crates/egui-pop-gallery/tests/snapshots/builtins-pop-high-contrast.png" alt="builtins-pop-high-contrast" width="100%"></a>

`pop-light`

<a href="../crates/egui-pop-gallery/tests/snapshots/builtins-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/builtins-pop-light.png" alt="builtins-pop-light" width="100%"></a>

### icons

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/icons-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/icons-pop-dark.png" alt="icons-pop-dark" width="100%"></a>

`pop-high-contrast`

<a href="../crates/egui-pop-gallery/tests/snapshots/icons-pop-high-contrast.png"><img src="../crates/egui-pop-gallery/tests/snapshots/icons-pop-high-contrast.png" alt="icons-pop-high-contrast" width="100%"></a>

`pop-light`

<a href="../crates/egui-pop-gallery/tests/snapshots/icons-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/icons-pop-light.png" alt="icons-pop-light" width="100%"></a>

`pop-light-compact`

<a href="../crates/egui-pop-gallery/tests/snapshots/icons-pop-light-compact.png"><img src="../crates/egui-pop-gallery/tests/snapshots/icons-pop-light-compact.png" alt="icons-pop-light-compact" width="100%"></a>

### values

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/values-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/values-pop-dark.png" alt="values-pop-dark" width="100%"></a>

`pop-light`

<a href="../crates/egui-pop-gallery/tests/snapshots/values-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/values-pop-light.png" alt="values-pop-light" width="100%"></a>

### menus

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/menus-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/menus-pop-dark.png" alt="menus-pop-dark" width="100%"></a>

`pop-light`

<a href="../crates/egui-pop-gallery/tests/snapshots/menus-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/menus-pop-light.png" alt="menus-pop-light" width="100%"></a>

### dialogs

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/dialogs-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/dialogs-pop-dark.png" alt="dialogs-pop-dark" width="100%"></a>

`pop-light`

<a href="../crates/egui-pop-gallery/tests/snapshots/dialogs-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/dialogs-pop-light.png" alt="dialogs-pop-light" width="100%"></a>

### files

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/files-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/files-pop-dark.png" alt="files-pop-dark" width="100%"></a>

### toasts

`long-pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/toasts-long-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/toasts-long-pop-dark.png" alt="toasts-long-pop-dark" width="100%"></a>

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/toasts-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/toasts-pop-dark.png" alt="toasts-pop-dark" width="100%"></a>

`pop-light`

<a href="../crates/egui-pop-gallery/tests/snapshots/toasts-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/toasts-pop-light.png" alt="toasts-pop-light" width="100%"></a>

### viewport

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/viewport-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/viewport-pop-dark.png" alt="viewport-pop-dark" width="100%"></a>

### images

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/images-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/images-pop-dark.png" alt="images-pop-dark" width="100%"></a>

### display

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/display-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/display-pop-dark.png" alt="display-pop-dark" width="100%"></a>

### tree

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/tree-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/tree-pop-dark.png" alt="tree-pop-dark" width="100%"></a>

### table

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/table-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/table-pop-dark.png" alt="table-pop-dark" width="100%"></a>

### zones

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/zones-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/zones-pop-dark.png" alt="zones-pop-dark" width="100%"></a>

### workbench

`detached-pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/workbench-detached-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/workbench-detached-pop-dark.png" alt="workbench-detached-pop-dark" width="100%"></a>

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/workbench-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/workbench-pop-dark.png" alt="workbench-pop-dark" width="100%"></a>

### arrangements

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/arrangements-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/arrangements-pop-dark.png" alt="arrangements-pop-dark" width="100%"></a>

### strips

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/strips-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/strips-pop-dark.png" alt="strips-pop-dark" width="100%"></a>

### frame

`own-pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/frame-own-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/frame-own-pop-dark.png" alt="frame-own-pop-dark" width="100%"></a>

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/frame-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/frame-pop-dark.png" alt="frame-pop-dark" width="100%"></a>

### scale

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/scale-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/scale-pop-dark.png" alt="scale-pop-dark" width="100%"></a>

### platform

No snapshot yet.

### settings

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/settings-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/settings-pop-dark.png" alt="settings-pop-dark" width="100%"></a>

`window-pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/settings-window-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/settings-window-pop-dark.png" alt="settings-window-pop-dark" width="100%"></a>

### i18n

No snapshot yet.

### debug

`pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/debug-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/debug-pop-dark.png" alt="debug-pop-dark" width="100%"></a>

### Other windows

`devtools-pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/devtools-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/devtools-pop-dark.png" alt="devtools-pop-dark" width="100%"></a>

`shortcuts-pop-dark`

<a href="../crates/egui-pop-gallery/tests/snapshots/shortcuts-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/shortcuts-pop-dark.png" alt="shortcuts-pop-dark" width="100%"></a>
