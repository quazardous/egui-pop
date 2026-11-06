# {{project-name}}

An [egui-pop](https://github.com/quazardous/egui-pop) application: a
colour swatch with two parameters. It is the end state of egui-pop's
guide [Your first application](https://github.com/quazardous/egui-pop/blob/main/docs/first-app.md),
and each step of the guide is a part of it:

| Step | Where |
|---|---|
| 1. A window | `main` and `builder` in `src/main.rs` |
| 2. A command with a key and a menu entry | `RESET` in `src/swatch.rs` |
| 3. Panels in the workbench | `VIEW` and `PARAMETERS`, drawn by `view` and `parameters` |
| 4. A setting | `SHOW_CODE`, read by `view` |
| 5. A kit of its own | `kit/kit.yaml`, added by `kit` in `src/main.rs` |
| 6. Undo | the values `HUE` and `SIZE`, and `write`: one step per action |
| 7. A command for agents | `SET`, its arguments described |
| 8. Tests | `src/tests.rs`, and its picture in `tests/snapshots/` |

```sh
cargo run      # the window: Ctrl+Shift+P for the palette, Ctrl+, for the settings
cargo test     # headless; the picture is rendered with wgpu
```

After changing what the window shows, write the picture again with
`UPDATE_SNAPSHOTS=1 cargo test` and look at it.

### The picture and the GPU

The snapshot test (`the_window_looks_as_it_did`) renders the window
with wgpu and compares it with `tests/snapshots/swatch.png`, which was
drawn by **lavapipe**, Mesa's software Vulkan (as egui-pop's CI draws
its pictures). A GPU draws edges a little differently, past the
tolerances of `kittest.toml`, so the test compares only on lavapipe:
without it (no adapter, or a GPU) it prints why it skipped and passes.

- To compare here, install lavapipe: `mesa-vulkan-drivers` (Debian,
  Ubuntu, Fedora), `vulkan-swrast` (Arch); wgpu picks it over a GPU.
- To compare on your GPU anyway, set `EGUI_POP_SNAPSHOT_ANY_GPU=1`,
  after writing the picture there (`UPDATE_SNAPSHOTS=1`).
- Keep the picture your CI checks drawn by the same adapter as the CI.

## Make it yours

1. Pick your plugin's id (`PLUGIN`) and change every id built on it:
   the values, the setting, the commands and the panels.
2. Replace the swatch with your application's state and views; keep
   the shape: state in the store (or in your own kernel behind a host),
   actions as commands, panels drawing from the state.
3. Give the kit your product's look, or drop it and pick a built-in
   theme (`ThemeSpec::new("pop-light")`).
4. Pick your licence.

The settings, the layout and the window's place are kept in your
configuration folder, in a folder named after the crate
(`AppBuilder::persist_in_config`: `~/.config/<crate>` on Linux,
`~/Library/Application Support/<crate>` on macOS, `%APPDATA%\<crate>`
on Windows).
