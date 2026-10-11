# Running the gallery in wbox

wbox runs an application inside a nested, headless Wayland compositor
(labwc here), so the gallery can be started, screenshot, clicked and typed
into by a script or an agent without a window ever opening on the desktop.

- `config.yaml`: the workspace (`egui-pop-gallery-wbox`, labwc, 1280x800,
  headless). Screenshots go to `screenshots/`, compositor logs to `log/`.
- `app.sh`: what the compositor launches, the release build of the gallery
  with options taken from `GALLERY_*` environment variables (see the
  script).

A wbox driver starts it with this config: the `wbox-mcp` server (its tools
take the config path), or a small script calling the wbox library directly
(up, screenshot, click, key, down). No driver is shipped here.

```sh
cargo build --release -p egui-pop-gallery
GALLERY_THEME=pop-light GALLERY_PAGE=builtins GALLERY_QUIT_AFTER=600 \
  <driver> up tools/wbox/config.yaml
```

The gallery tells a driver it is up by printing one JSON line on stdout
(`out/gallery.jsonl` through `app.sh`):

```json
{"event":"ready","app":"egui-pop-gallery","title":"egui-pop gallery","theme":"pop-light","density":"comfortable","accent":null,"page":"builtins","quit_after":600,...}
```

With `--quit-after <frames>` it closes itself after that many frames and
prints `{"event":"quit","frames":...}` first. The window title is always
`egui-pop gallery`. Run `egui-pop-gallery --help` for every option.

`out/`, `log/` and `screenshots/` are not committed.
