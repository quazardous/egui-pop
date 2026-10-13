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
and tables), `text-editor`, `code-editor`, `zones` (zones and blocks), `workbench`,
`arrangements`, `strips`, `frame`, `scale`, `platform` (the platform
services), `settings`, `i18n` (translations) and `debug` (the debug
bridge). `Alt+Down` and `Alt+Up` go to the next and previous page,
`Ctrl+Shift+T` to the next theme.

It also builds for the web (`wasm32-unknown-unknown`, with Trunk from
`crates/egui-pop-gallery`), and runs offscreen in a nested compositor
for scripted runs and screenshots ([tools/wbox](../tools/wbox)). Its
pages are also the snapshot tests (see [Testing](testing.md)).

## Screenshots

Every snapshot the gallery's tests keep, by kind, four to a row. They
are regenerated with the tests (see [Testing](testing.md)), so they show
the current state. Click one for its full size.

### Look and feel

Themes, design tokens, icons and scaling.

<table>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/overview-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-pop-dark.png" width="200" alt="overview-pop-dark"></a><br>overview-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/overview-pop-dark@2x.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-pop-dark@2x.png" width="200" alt="overview-pop-dark@2x"></a><br>overview-pop-dark@2x</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/overview-pop-high-contrast.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-pop-high-contrast.png" width="200" alt="overview-pop-high-contrast"></a><br>overview-pop-high-contrast</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/overview-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-pop-light.png" width="200" alt="overview-pop-light"></a><br>overview-pop-light</td>
  </tr>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/tokens-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/tokens-pop-dark.png" width="200" alt="tokens-pop-dark"></a><br>tokens-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/icons-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/icons-pop-dark.png" width="200" alt="icons-pop-dark"></a><br>icons-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/icons-pop-high-contrast.png"><img src="../crates/egui-pop-gallery/tests/snapshots/icons-pop-high-contrast.png" width="200" alt="icons-pop-high-contrast"></a><br>icons-pop-high-contrast</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/icons-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/icons-pop-light.png" width="200" alt="icons-pop-light"></a><br>icons-pop-light</td>
  </tr>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/icons-pop-light-compact.png"><img src="../crates/egui-pop-gallery/tests/snapshots/icons-pop-light-compact.png" width="200" alt="icons-pop-light-compact"></a><br>icons-pop-light-compact</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/scale-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/scale-pop-dark.png" width="200" alt="scale-pop-dark"></a><br>scale-pop-dark</td>
  </tr>
</table>

<details><summary>The overview in the other built-in themes</summary>

<table>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/overview-catppuccin-latte.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-catppuccin-latte.png" width="200" alt="overview-catppuccin-latte"></a><br>overview-catppuccin-latte</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/overview-catppuccin-mocha.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-catppuccin-mocha.png" width="200" alt="overview-catppuccin-mocha"></a><br>overview-catppuccin-mocha</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/overview-github-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-github-dark.png" width="200" alt="overview-github-dark"></a><br>overview-github-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/overview-github-dark-high-contrast.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-github-dark-high-contrast.png" width="200" alt="overview-github-dark-high-contrast"></a><br>overview-github-dark-high-contrast</td>
  </tr>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/overview-github-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-github-light.png" width="200" alt="overview-github-light"></a><br>overview-github-light</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/overview-github-light-high-contrast.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-github-light-high-contrast.png" width="200" alt="overview-github-light-high-contrast"></a><br>overview-github-light-high-contrast</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/overview-gruvbox-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-gruvbox-dark.png" width="200" alt="overview-gruvbox-dark"></a><br>overview-gruvbox-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/overview-gruvbox-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-gruvbox-light.png" width="200" alt="overview-gruvbox-light"></a><br>overview-gruvbox-light</td>
  </tr>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/overview-one-dark-pro.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-one-dark-pro.png" width="200" alt="overview-one-dark-pro"></a><br>overview-one-dark-pro</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/overview-one-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-one-light.png" width="200" alt="overview-one-light"></a><br>overview-one-light</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/overview-solarized-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-solarized-dark.png" width="200" alt="overview-solarized-dark"></a><br>overview-solarized-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/overview-solarized-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/overview-solarized-light.png" width="200" alt="overview-solarized-light"></a><br>overview-solarized-light</td>
  </tr>
</table>

</details>

### Widgets

Egui's built-ins in egui-pop's style, values, menus, dialogs, file choosing and toasts.

<table>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/builtins-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/builtins-pop-dark.png" width="200" alt="builtins-pop-dark"></a><br>builtins-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/builtins-pop-dark-compact.png"><img src="../crates/egui-pop-gallery/tests/snapshots/builtins-pop-dark-compact.png" width="200" alt="builtins-pop-dark-compact"></a><br>builtins-pop-dark-compact</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/builtins-pop-high-contrast.png"><img src="../crates/egui-pop-gallery/tests/snapshots/builtins-pop-high-contrast.png" width="200" alt="builtins-pop-high-contrast"></a><br>builtins-pop-high-contrast</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/builtins-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/builtins-pop-light.png" width="200" alt="builtins-pop-light"></a><br>builtins-pop-light</td>
  </tr>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/values-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/values-pop-dark.png" width="200" alt="values-pop-dark"></a><br>values-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/values-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/values-pop-light.png" width="200" alt="values-pop-light"></a><br>values-pop-light</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/menus-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/menus-pop-dark.png" width="200" alt="menus-pop-dark"></a><br>menus-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/menus-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/menus-pop-light.png" width="200" alt="menus-pop-light"></a><br>menus-pop-light</td>
  </tr>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/dialogs-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/dialogs-pop-dark.png" width="200" alt="dialogs-pop-dark"></a><br>dialogs-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/dialogs-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/dialogs-pop-light.png" width="200" alt="dialogs-pop-light"></a><br>dialogs-pop-light</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/dialogs-closable-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/dialogs-closable-pop-dark.png" width="200" alt="dialogs-closable-pop-dark"></a><br>dialogs-closable-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/dialogs-closable-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/dialogs-closable-pop-light.png" width="200" alt="dialogs-closable-pop-light"></a><br>dialogs-closable-pop-light</td>
  </tr>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/files-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/files-pop-dark.png" width="200" alt="files-pop-dark"></a><br>files-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/toasts-long-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/toasts-long-pop-dark.png" width="200" alt="toasts-long-pop-dark"></a><br>toasts-long-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/toasts-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/toasts-pop-dark.png" width="200" alt="toasts-pop-dark"></a><br>toasts-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/toasts-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/toasts-pop-light.png" width="200" alt="toasts-pop-light"></a><br>toasts-pop-light</td>
  </tr>
</table>

### Data views

The live tree, lists and tables.

<table>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/tree-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/tree-pop-dark.png" width="200" alt="tree-pop-dark"></a><br>tree-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/table-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/table-pop-dark.png" width="200" alt="table-pop-dark"></a><br>table-pop-dark</td>
  </tr>
</table>

### Editors

The text editor and the code editor.

<table>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/text-editor-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/text-editor-pop-dark.png" width="200" alt="text-editor-pop-dark"></a><br>text-editor-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/code-editor-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/code-editor-pop-dark.png" width="200" alt="code-editor-pop-dark"></a><br>code-editor-pop-dark</td>
  </tr>
</table>

### Images

The tiled image viewport, images and the display.

<table>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/viewport-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/viewport-pop-dark.png" width="200" alt="viewport-pop-dark"></a><br>viewport-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/images-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/images-pop-dark.png" width="200" alt="images-pop-dark"></a><br>images-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/display-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/display-pop-dark.png" width="200" alt="display-pop-dark"></a><br>display-pop-dark</td>
  </tr>
</table>

### Layout

Zones and blocks, the workbench, arrangements, strips and the window frame.

<table>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/zones-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/zones-pop-dark.png" width="200" alt="zones-pop-dark"></a><br>zones-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/workbench-detached-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/workbench-detached-pop-dark.png" width="200" alt="workbench-detached-pop-dark"></a><br>workbench-detached-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/workbench-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/workbench-pop-dark.png" width="200" alt="workbench-pop-dark"></a><br>workbench-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/arrangements-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/arrangements-pop-dark.png" width="200" alt="arrangements-pop-dark"></a><br>arrangements-pop-dark</td>
  </tr>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/strips-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/strips-pop-dark.png" width="200" alt="strips-pop-dark"></a><br>strips-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/frame-own-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/frame-own-pop-dark.png" width="200" alt="frame-own-pop-dark"></a><br>frame-own-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/frame-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/frame-pop-dark.png" width="200" alt="frame-pop-dark"></a><br>frame-pop-dark</td>
  </tr>
</table>

### Application services

Settings, shortcuts, platform services, translations and the developer tools.

<table>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/settings-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/settings-pop-dark.png" width="200" alt="settings-pop-dark"></a><br>settings-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/settings-window-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/settings-window-pop-dark.png" width="200" alt="settings-window-pop-dark"></a><br>settings-window-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/shortcuts-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/shortcuts-pop-dark.png" width="200" alt="shortcuts-pop-dark"></a><br>shortcuts-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/debug-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/debug-pop-dark.png" width="200" alt="debug-pop-dark"></a><br>debug-pop-dark</td>
  </tr>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/devtools-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/devtools-pop-dark.png" width="200" alt="devtools-pop-dark"></a><br>devtools-pop-dark</td>
  </tr>
</table>

No snapshot yet: `platform`, `i18n`.
