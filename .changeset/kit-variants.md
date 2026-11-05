---
egui-pop-tokens: minor
egui-pop-api: minor
---

A kit may hold both its variants, light and dark: its manifest's
`counterpart` (in `org.egui-pop.kit`) gives the other variant's name,
label and tokens. It becomes a theme of its own, each variant naming the
other as its counterpart, so the light, dark or system setting switches
between them with one kit folder. The kit's other token files and its
faces serve both variants. `KitData::counterpart` holds it,
`Themes::add_kit` and `AppBuilder::kit` add both themes, and
`KitData::infos` (and so `ThemeService::kits`) lists both; a counterpart
that cannot be used is a manifest problem, the kit keeping one variant.
