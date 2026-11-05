# Driving the gallery with egui-mcp

The gallery has an opt-in Cargo feature, `inspection`, that turns on
eframe's `inspection` feature. With it, and with the `EGUI_INSPECTION`
environment variable set, the gallery listens for the
[`egui_inspection`](https://crates.io/crates/egui_inspection) protocol,
and Rerun's [`egui-mcp`](https://crates.io/crates/egui_mcp) server
connects to it. An agent behind `egui-mcp` reads the gallery's
accessibility tree, clicks and types by role or label, takes
screenshots and waits for a widget to appear, in a live window.

This is a tool for working on egui-pop, not a part of it: no published
crate turns the feature on, and an application that wants it turns on
eframe's `inspection` itself.

## What it opens, and the risk

- **Off unless asked twice.** The feature is off by default, so no
  build has it unless it says `--features inspection`, release builds
  included. A build with it opens nothing until `EGUI_INSPECTION` is set
  (`1` or `true`: `127.0.0.1:5719`; a `host:port`: that address; unset,
  empty, `0` or `false`: nothing).
- **No authentication.** Whoever reaches the port drives the gallery
  and reads its screen: on loopback, any process of any user on the
  machine. Never bind it to another address than loopback on a machine
  you share; for a remote session, keep loopback and tunnel it (SSH).
  The gallery says so on stderr when it starts.
- **Native only.** On the web the feature compiles and opens nothing.
- **One port.** `127.0.0.1:5719` is fixed: two galleries started with
  `EGUI_INSPECTION=1` collide, the second one logs that it cannot bind.
  Give each its own address (`EGUI_INSPECTION=127.0.0.1:5720`).

## Run the gallery with it

```sh
cargo gallery --features inspection
```

`cargo gallery` (the xtask) passes the feature on and, when the
environment does not set `EGUI_INSPECTION`, sets it to `1` for the
gallery, so the endpoint is on `127.0.0.1:5719`. It prints a line saying
so. Set the variable yourself for another address:

```sh
EGUI_INSPECTION=127.0.0.1:5720 cargo gallery --features inspection
# or without the xtask
EGUI_INSPECTION=1 cargo run -p egui-pop-gallery --features inspection
```

The gallery then prints:

```text
inspection: the egui_inspection endpoint listens on 127.0.0.1:5719, without authentication: ...
```

From automation, run the gallery headless, never on a desktop session:
in wbox's nested compositor ([tools/wbox](../tools/wbox)), with
`EGUI_INSPECTION` in the environment of the command it starts. The tree,
clicks and keys work while the window is hidden; a screenshot needs a
rendered frame, which a headless compositor gives.

## Install and start egui-mcp

`egui-mcp` is an MCP server on stdio (MIT OR Apache-2.0):

```sh
cargo install egui_mcp --locked      # the `egui-mcp` binary, 0.3 at the time of writing
```

Register it with the agent, for example Claude Code:

```sh
claude mcp add egui egui-mcp
```

or in an `.mcp.json`:

```json
{ "mcpServers": { "egui": { "command": "egui-mcp" } } }
```

The agent then calls `attach` (default `127.0.0.1:5719`, or
`{"port": 5720}`), and the other tools work on the gallery:

| Tool | What it does |
|---|---|
| `attach`, `disconnect`, `status` | connect to the endpoint (retrying for a while), leave, say what is connected |
| `widget_tree`, `get_widget` | the accessibility tree, filtered by `role`, `label_contains`, `value_contains`, `content_contains`; one widget in full |
| `click`, `hover`, `drag`, `scroll` | at a widget found by id, role or text, or at a position in points |
| `type_text`, `press_key` | text into the focused widget (or one found first), a key with modifiers |
| `wait_for` | until widgets matching a query are there, or a number of frames ran |
| `screenshot`, `resize` | a PNG of the window, its size in points |
| `batch` | several of the above in one call |

## An example session

The tool calls an agent makes, with what the gallery answered (trimmed);
the gallery started with `cargo gallery --features inspection`.

```text
attach {}
  {"attached":{"label":"egui-pop gallery","protocol_version":1,"transport":"127.0.0.1:5719"},"ok":true}

widget_tree {"role": "Button", "limit": 40}
  {"nodes":[{"id":"e6b2a9759ad95ec2","label":"File","role":"Button"},
            {"id":"41914c2377eb539e","label":"Edit","role":"Button"},
            {"id":"c1a99ae429a37eda","label":"View","role":"Button"},
            {"id":"52e44f5028f53c93","label":"Help","role":"Button"}, ...]}

click {"label_contains": "Help"}
  {"button":"primary","clicked_id":"52e44f5028f53c93","count":1,"ok":true,"pos":[135.0,14.0]}

wait_for {"role": "Button", "label_contains": "Report a problem", "timeout_secs": 5}
  {"matched":[{"id":"edf89024e9b57bbf","label":"Report a problem","role":"Button"}],"ok":true}

screenshot {}
  {"height":692,"width":1100} and the PNG: the Help menu, open

click {"role": "Button", "label_contains": "About egui-pop gallery"}
  {"clicked_id":"e5325c61cbb50a26","ok":true,"pos":[214.8,217.0]}

wait_for {"role": "Button", "label_contains": "Copy version info", "timeout_secs": 5}
  {"matched":[{"label":"Copy version info","role":"Button", ...}],"ok":true,"steps_waited":6}

screenshot {}
  the About dialog over the overview page

click {"label_contains": "Close"}
```

`click {"role": "Button", "label_contains": "Report a problem"}` is the
same call, but
that entry opens a new issue in the system's browser rather than a
dialog in the gallery: from an agent's session it opens a browser on
whatever desktop the session reaches. Help > About is the dialog to
practise on.

A widget is found by its accessible name, so a click by label works
for every widget the test kit already requires to have a name
([Testing](testing.md)). Several matches are an error that lists them:
a dialog's title is a `Label` and a text run with the same text as the
menu entry that opened it, and the About dialog has its own "Report a
problem" link. Add a `role`, or pass an id from `widget_tree`.

## How it relates to the debug bridge

Two ways in, for different things:

| | `egui_inspection` / `egui-mcp` | egui-pop's debug bridge |
|---|---|---|
| Reaches | egui: the AccessKit tree, raw input events, the framebuffer | the `App`: collectors, commands, steps, the journal ([Agents](agents.md)) |
| Acts as | a person at the keyboard and mouse | the agent (`debug-bridge` origin), held by `agent` flags, `when` clauses and hooks |
| From another process | yes, TCP on loopback | no, Rust calls only; the transport is the application's |
| Wait | `wait_for` on the tree or frames | polling (`journal.since`, a collector's `idle`) |
| Screenshots | yes | no (wbox, kittest) |
| Built in | the gallery with `--features inspection` only | every `App` |

`egui_inspection` cannot carry the bridge's calls: its protocol has a
fixed set of requests (the tree, input events, a screenshot, a resize,
a wait), no extension point, and egui-pop adds none. A command reached
through `egui-mcp` is reached the way a person reaches it, through its
menu, its button or its shortcut; it runs as the user, and the bridge's
`agent` flags and its mode do not apply. To run a command as an agent,
read a collector or call a collector action, use the bridge
([Agents](agents.md)); devtools shows the same collectors in the window
([Debugging](debugging.md)), and through `egui-mcp` an agent can open
that panel and read it like any other widget.

## Tests

`crates/egui-pop-gallery/tests/inspection.rs` attaches the plugin eframe
attaches to a headless gallery (kittest, no window), serves it on a free
loopback port and asks for the info and the tree over TCP: the gallery
must answer with its Help menu in the tree. CI also builds and lints
the gallery with `--features inspection`.
