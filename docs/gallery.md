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
built-ins), `theme-editor` (the theme editor), `icons`, `motion` (the motion clock), `values`, `sliders`,
`menus`, `dialogs`, `files` (the file chooser), `toasts`, `tips` (tips of the day and
hints), `progress`,
`placeholders` (skeletons and empty states), `viewport` (the image
viewport), `navigator` (the zoom control and the navigator), `images`,
`grading` (curves and colour wheels), `cursors` (tool cursors, brush
outlines and the loupe), `node-graph` (the node graph editor), `display` (wide gamut and HDR surfaces), `tree`
(the live tree), `table` (lists and tables), `library` (the photo
library), `text-editor`,
`code-editor`, `terminal`, `zones` (zones and blocks), `workbench`,
`palettes`, `arrangements`, `contexts` (the context stack: places and modes), `strips`, `jobs` (a pretend host's
background jobs), `frame`, `scale`, `platform` (the platform services),
`settings`, `i18n` (translations) and `debug` (the debug
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

Themes, design tokens, the theme editor, icons and scaling.

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
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/theme-editor-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/theme-editor-pop-dark.png" width="200" alt="theme-editor-pop-dark"></a><br>theme-editor-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/theme-editor-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/theme-editor-pop-light.png" width="200" alt="theme-editor-pop-light"></a><br>theme-editor-pop-light</td>
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

Egui's built-ins in egui-pop's style, values, sliders, menus, dialogs (movable ones, a clear backdrop), file choosing, toasts, and progress (bars, rings, meters, spinners; still under reduced motion).

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
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/dialogs-movable-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/dialogs-movable-pop-dark.png" width="200" alt="dialogs-movable-pop-dark"></a><br>dialogs-movable-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/dialogs-movable-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/dialogs-movable-pop-light.png" width="200" alt="dialogs-movable-pop-light"></a><br>dialogs-movable-pop-light</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/dialogs-clear-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/dialogs-clear-pop-dark.png" width="200" alt="dialogs-clear-pop-dark"></a><br>dialogs-clear-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/files-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/files-pop-dark.png" width="200" alt="files-pop-dark"></a><br>files-pop-dark</td>
  </tr>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/toasts-long-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/toasts-long-pop-dark.png" width="200" alt="toasts-long-pop-dark"></a><br>toasts-long-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/toasts-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/toasts-pop-dark.png" width="200" alt="toasts-pop-dark"></a><br>toasts-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/toasts-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/toasts-pop-light.png" width="200" alt="toasts-pop-light"></a><br>toasts-pop-light</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/sliders-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/sliders-pop-dark.png" width="200" alt="sliders-pop-dark"></a><br>sliders-pop-dark</td>
  </tr>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/sliders-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/sliders-pop-light.png" width="200" alt="sliders-pop-light"></a><br>sliders-pop-light</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/progress-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/progress-pop-dark.png" width="200" alt="progress-pop-dark"></a><br>progress-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/progress-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/progress-pop-light.png" width="200" alt="progress-pop-light"></a><br>progress-pop-light</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/progress-reduced-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/progress-reduced-pop-dark.png" width="200" alt="progress-reduced-pop-dark"></a><br>progress-reduced-pop-dark</td>
  </tr>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/progress-busy-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/progress-busy-pop-dark.png" width="200" alt="progress-busy-pop-dark"></a><br>progress-busy-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/placeholders-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/placeholders-pop-dark.png" width="200" alt="placeholders-pop-dark"></a><br>placeholders-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/dialogs-kit-square.png"><img src="../crates/egui-pop-gallery/tests/snapshots/dialogs-kit-square.png" width="200" alt="dialogs-kit-square"></a><br>dialogs-kit-square</td>
  </tr>
</table>

### Data views

The live tree, lists and tables, and the photo library (a thumbnail grid
and filmstrip, marks, filter chips, a comparison).

<table>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/tree-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/tree-pop-dark.png" width="200" alt="tree-pop-dark"></a><br>tree-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/table-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/table-pop-dark.png" width="200" alt="table-pop-dark"></a><br>table-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/library-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/library-pop-dark.png" width="200" alt="library-pop-dark"></a><br>library-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/library-compare-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/library-compare-pop-dark.png" width="200" alt="library-compare-pop-dark"></a><br>library-compare-pop-dark</td>
  </tr>
</table>

### Editors

The text editor, the code editor and the terminal.

<table>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/text-editor-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/text-editor-pop-dark.png" width="200" alt="text-editor-pop-dark"></a><br>text-editor-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/code-editor-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/code-editor-pop-dark.png" width="200" alt="code-editor-pop-dark"></a><br>code-editor-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/terminal-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/terminal-pop-dark.png" width="200" alt="terminal-pop-dark"></a><br>terminal-pop-dark</td>
  </tr>
</table>

### Images

The tiled image viewport, its zoom control and navigator, images,
curves and colour wheels, cursors and the loupe, the node graph editor,
and the display.

<table>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/viewport-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/viewport-pop-dark.png" width="200" alt="viewport-pop-dark"></a><br>viewport-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/navigator-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/navigator-pop-dark.png" width="200" alt="navigator-pop-dark"></a><br>navigator-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/images-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/images-pop-dark.png" width="200" alt="images-pop-dark"></a><br>images-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/display-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/display-pop-dark.png" width="200" alt="display-pop-dark"></a><br>display-pop-dark</td>
  </tr>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/grading-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/grading-pop-dark.png" width="200" alt="grading-pop-dark"></a><br>grading-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/grading-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/grading-pop-light.png" width="200" alt="grading-pop-light"></a><br>grading-pop-light</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/cursors-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/cursors-pop-dark.png" width="200" alt="cursors-pop-dark"></a><br>cursors-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/cursors-pop-light.png"><img src="../crates/egui-pop-gallery/tests/snapshots/cursors-pop-light.png" width="200" alt="cursors-pop-light"></a><br>cursors-pop-light</td>
  </tr>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/node-graph-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/node-graph-pop-dark.png" width="200" alt="node-graph-pop-dark"></a><br>node-graph-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/node-graph-detail-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/node-graph-detail-pop-dark.png" width="200" alt="node-graph-detail-pop-dark"></a><br>node-graph-detail-pop-dark</td>
  </tr>
</table>

### Layout

Zones and blocks, the workbench, palettes, arrangements, the context stack, strips and the window frame.

<table>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/zones-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/zones-pop-dark.png" width="200" alt="zones-pop-dark"></a><br>zones-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/workbench-detached-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/workbench-detached-pop-dark.png" width="200" alt="workbench-detached-pop-dark"></a><br>workbench-detached-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/workbench-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/workbench-pop-dark.png" width="200" alt="workbench-pop-dark"></a><br>workbench-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/workbench-kit-underline.png"><img src="../crates/egui-pop-gallery/tests/snapshots/workbench-kit-underline.png" width="200" alt="workbench-kit-underline"></a><br>workbench-kit-underline</td>
  </tr>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/arrangements-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/arrangements-pop-dark.png" width="200" alt="arrangements-pop-dark"></a><br>arrangements-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/strips-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/strips-pop-dark.png" width="200" alt="strips-pop-dark"></a><br>strips-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/frame-own-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/frame-own-pop-dark.png" width="200" alt="frame-own-pop-dark"></a><br>frame-own-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/frame-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/frame-pop-dark.png" width="200" alt="frame-pop-dark"></a><br>frame-pop-dark</td>
  </tr>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/frame-kit-title-start.png"><img src="../crates/egui-pop-gallery/tests/snapshots/frame-kit-title-start.png" width="200" alt="frame-kit-title-start"></a><br>frame-kit-title-start</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/palettes-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/palettes-pop-dark.png" width="200" alt="palettes-pop-dark"></a><br>palettes-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/contexts-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/contexts-pop-dark.png" width="200" alt="contexts-pop-dark"></a><br>contexts-pop-dark</td>
  </tr>
</table>

### Application services

Settings, shortcuts, tips, platform services, translations, the developer tools, and the jobs centre (a job's log console open, in the page and in the status bar's popover).

<table>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/jobs-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/jobs-pop-dark.png" width="200" alt="jobs-pop-dark"></a><br>jobs-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/jobs-popover-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/jobs-popover-pop-dark.png" width="200" alt="jobs-popover-pop-dark"></a><br>jobs-popover-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/settings-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/settings-pop-dark.png" width="200" alt="settings-pop-dark"></a><br>settings-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/settings-window-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/settings-window-pop-dark.png" width="200" alt="settings-window-pop-dark"></a><br>settings-window-pop-dark</td>
  </tr>
  <tr>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/shortcuts-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/shortcuts-pop-dark.png" width="200" alt="shortcuts-pop-dark"></a><br>shortcuts-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/debug-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/debug-pop-dark.png" width="200" alt="debug-pop-dark"></a><br>debug-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/devtools-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/devtools-pop-dark.png" width="200" alt="devtools-pop-dark"></a><br>devtools-pop-dark</td>
    <td align="center"><a href="../crates/egui-pop-gallery/tests/snapshots/tips-pop-dark.png"><img src="../crates/egui-pop-gallery/tests/snapshots/tips-pop-dark.png" width="200" alt="tips-pop-dark"></a><br>tips-pop-dark</td>
  </tr>
</table>

No snapshot yet: `platform`, `i18n`.
