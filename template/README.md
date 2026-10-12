# {{project-name}}

An [egui-pop](https://github.com/quazardous/egui-pop) application: one
plugin with a panel, a setting and a command, over the default plugins
(command palette `Ctrl+Shift+P`, settings `Ctrl+,`, dialogs, toasts, the
window's place kept).

```sh
cargo run
```

Grow it plugin by plugin: each registers its commands, panels, settings
and blocks through `Ctx`, and loses them when it is disconnected.
