# Example kits

Kit folders (`docs/kit-packages.md`), read by any application with
`AppBuilder::kit`, or checked with:

```sh
cargo run -p egui-pop-tokens --example kit-check -- examples/kits/underline-tabs
```

- `underline-tabs/`: data only, the gallery's `workbench-kit-underline`
  snapshot is drawn from it.
- `flavour-macos/`: data only, the window buttons as macOS draws them
  (`traffic-light` circles at the start of the title bar, glyphs shown
  while the group is hovered); the gallery's `frame-kit-macos` snapshot
  is drawn from it.
