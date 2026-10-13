# An egui-pop extension, from its template

A minimal third-party crate that extends
[egui-pop](https://github.com/quazardous/egui-pop): a **tally**, a
counter with a name, brought by one plugin. Every way an extension plugs
into egui-pop is here, small enough to read in one sitting:

| What | Where | egui-pop's contract |
|---|---|---|
| A plugin owning all it registers | `TallyPlugin` in `src/lib.rs` | [ADR 0004](https://github.com/quazardous/egui-pop/blob/main/docs/adr/0004-plugins.md) |
| A component drawn from tokens | `Tally` in `src/widget.rs`, `TallyTokens` in `src/tokens.rs` | [Design tokens](https://github.com/quazardous/egui-pop/blob/main/docs/tokens.md) |
| Its strings, English and French | `src/strings.rs`, `locales/*/tally.ftl` | [Translations](https://github.com/quazardous/egui-pop/blob/main/docs/i18n.md) |
| A gallery story with a playground | `STORY` | [The gallery](https://github.com/quazardous/egui-pop/blob/main/docs/gallery.md) |
| A step others may veto, replace or follow | `STEP_CHANGE` (`tally.change`) | [ADR 0011](https://github.com/quazardous/egui-pop/blob/main/docs/adr/0011-lifecycle-steps.md) |
| A command for people, one for agents | `CMD_INCREMENT`, `CMD_SET` | [Plugins, commands and steps](https://github.com/quazardous/egui-pop/blob/main/docs/plugins.md) |
| Headless tests | `src/tests.rs` (egui_kittest, no GPU) | [Testing](https://github.com/quazardous/egui-pop/blob/main/docs/testing.md) |

In the egui-pop repository this directory is a workspace member: every
commit builds it, lints it and runs its tests against the egui-pop of
that commit, so the template never falls behind the API.

## Start your extension from it

With [cargo-generate](https://github.com/cargo-generate/cargo-generate):

```sh
cargo generate --git https://github.com/quazardous/egui-pop templates/extension
```

It asks for the crate's name and the egui-pop and egui versions, and
writes `Cargo.toml` from `Cargo.toml.liquid` (the repository's own
`Cargo.toml` stays behind). Or copy the directory by hand and replace
`Cargo.toml` with `Cargo.toml.liquid`, filling its three placeholders.

Then make it yours:

1. Pick a plugin id that names your extension (`acme-tally`, not
   `tally`), and change `PLUGIN` and every id built on it: the step, the
   commands, the story, the service, the token paths (`acme-tally.*`)
   and the Fluent keys (`acme-tally-…`, `command-acme-tally-…`,
   `plugin-acme-tally`). egui-pop refuses a catalogue message that does
   not carry its plugin's id, so a forgotten one fails a test.
2. Replace the tally with your component; keep its tests' shape: a
   harness built on its own context, widgets found by role and label,
   `assert_all_named`, the step vetoed by another plugin, the French
   strings, the contrast of its tokens in every built-in theme.
3. Pick your licence in `Cargo.toml` (the template's files are MIT, like
   egui-pop; your crate may use any licence its dependencies allow).
4. `cargo test`, then list it in egui-pop's
   [registry of extensions](https://github.com/quazardous/egui-pop/blob/main/docs/extensions.md) with a pull
   request.

## Use it in an application

```rust,ignore
App::builder()
    .plugins(DefaultPlugins)
    .plugin(egui_pop::i18n::I18nPlugin::new()) // optional: translations
    .plugin(TallyPlugin)
    .run_native("Tallies", eframe::NativeOptions::default())
```

In a panel or a block: `Tally::new("apples", "Apples").show(ui, app)`.
An agent sets a tally through the debug bridge with the `tally.set`
command (`{"tally": "apples", "value": 3}`); a plugin keeps tallies
positive with a before-hook on `tally.change`.

## The rules an extension follows

- Colours, sizes and durations come from tokens: a component token read
  by path (`tally.background`) with a system token as its fallback, never
  a literal; text over its background keeps 4.5:1 (7:1 in high-contrast
  themes), a control's boundary 3:1.
- Every interactive widget has a role and a name a screen reader says;
  the keyboard reaches it.
- Every text shown has a key and an English fallback; the catalogues
  carry the plugin's id.
- What changes state runs as a step, so applications can take part.
- Nothing fails silently (ADR 0003): a component without its plugin
  says so.

The full list, and how egui-pop treats extensions across versions, is
in [GOVERNANCE.md](https://github.com/quazardous/egui-pop/blob/main/GOVERNANCE.md) and
[CONTRIBUTING.md](https://github.com/quazardous/egui-pop/blob/main/CONTRIBUTING.md#extensions).
