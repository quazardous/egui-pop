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
configuration folder, in a folder named after the crate.
