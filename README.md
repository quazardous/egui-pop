# egui-pop

Themable, accessible, agent-drivable components for
[egui](https://github.com/emilk/egui). Not an official egui crate.

**Status: nothing usable yet.** The first components are being extracted
from the application they were written for.

What it is meant to be:

- **Design tokens and themes** (the DTCG format): palette, roles,
  components; dark, light and high-contrast modes, densities, accents;
  generated into egui's own style so egui's widgets match.
- **Components**: a slider with a soft and a hard range, parameter
  widgets, confirmations, forms, pickers, toasts, menus built from
  declared commands, a panel strip with indicators, a node graph view.
- **Plain or bound**: every widget works on a `&mut` value like egui's
  own, or bound to an observable source of values (it then listens to
  outside changes and shows who made them).
- **Accessible and agent-drivable**: roles, names, values and actions on
  every widget, keyboard patterns, contrast checked per theme.
- **Easy to debug**: an overlay with ids, bindings, origins and resolved
  tokens; repaint reasons; a change log; checks in debug builds.
- **Tested headless** with egui_kittest, snapshots per theme and state,
  and a gallery application.

One release per egui minor version.

## License

MIT, see [LICENSE](LICENSE).
