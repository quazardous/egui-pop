# Where an application writes

The folders an application writes to, read from the environment as each
platform names them. Two modules, in two families
([ADR 0027](adr/0027-component-families.md)): the configuration folder
is part of "Foundations" (`AppBuilder::persist_in_config` saves there),
the state, logs, crash reports and caches part of "Platform services".

| Module | What it gives |
|---|---|
| `egui_pop_api::dirs` | `config_dir`: where settings, the layout and the window's place are kept |
| `egui_pop_platform::dirs` | `state_dir`, `log_dir`, `crash_dir`, `cache_dir`, `temp_dir`, `pid`, `reveal`; re-exports `config_dir` |

| | Configuration | State | Logs |
|---|---|---|---|
| Linux and BSD | `$XDG_CONFIG_HOME/<app>` (`~/.config/<app>`) | `$XDG_STATE_HOME/<app>` (`~/.local/state/<app>`) | `<state>/logs` |
| macOS | `~/Library/Application Support/<app>` | `~/Library/Application Support/<app>` | `~/Library/Logs/<app>` |
| Windows | `%APPDATA%\<app>` | `%LOCALAPPDATA%\<app>` | `<state>\logs` |

Crash reports go to `<state>/crashes`; compiled files (a theme's cache)
to the platform's cache directory. Inside a Flatpak sandbox the `XDG_*`
variables point into the application's own directory, so the same code
finds the sandbox's place.

**Owns**: the names of these places per platform.

**Refuses**: creating or cleaning them (the writer creates what it
writes); data folders the user chooses (the application asks with
[choosing files](files.md)); a general directories crate (`dirs` and
`directories` were surveyed: three variables, and neither has a state
folder on macOS and Windows).

## The API

```rust
let config = egui_pop_api::dirs::config_dir("demo");
let logs = egui_pop_platform::dirs::log_dir("demo");
let _ = (config, logs);

// The same, from an environment of the caller's (tests, a sandbox):
let dir = egui_pop_platform::dirs::state_dir_with("demo", |name| match name {
    "XDG_STATE_HOME" => Some("/state".into()),
    _ => None,
});
if cfg!(all(unix, not(target_os = "macos"))) {
    assert_eq!(dir, Some("/state/demo".into()));
}
```

`egui_pop_platform::dirs::reveal` opens a file or folder with what the
system opens it with (the log folder from a crash dialog).

## Failure behaviour

Each function answers `None` when the environment names no home (no
`HOME`, no `APPDATA`); a relative value of a variable is ignored, as the
XDG specification says. The caller then keeps the data in memory or
uses `temp_dir`, the last resort. On the web there is no file system:
the functions answer `None`, `pid` is 0 and `reveal` refuses. `reveal`
returns the opener's error as text; it does not wait for the opener.

## Accessibility

None: no UI.

## Tier

From [API stability](stability.md): `egui_pop_api::dirs` is
**Supported** (decided 2026-11-15: the places users' settings live are
kept; moving them would need a migration of where they are read).
`egui_pop_platform::dirs` follows the platform services:
**Experimental** ("their backends, services … may move while the first
applications ship them").

## Tests

Unit tests in `crates/egui-pop-api/src/dirs.rs`; doc tests of both
modules; `crates/egui-pop-platform/tests/dirs.rs` (the state directory
per platform from the environment given, a relative place skipped, the
logs, crash reports and cache under their places, the temporary
directory and the process id). `reveal` starts the system's opener and
is not run in tests.
