# 0022: Sections and a toolbar row move into egui_stack as plain-egui cores; egui-pop wraps them

Date: 2026-11-03. Status: accepted (2026-11-03, by the lead maintainer).

## Context

egui_stack continues egui_dock (MIT) on egui 0.37 and means to hold more
of an application's panel management: stackable sections and toolbars.
egui-pop has both today ([sections](../sections.md),
[toolbars](../toolbars.md)). Sections are a widget in `egui-pop-core`
over egui-free arithmetic in `egui-pop-model`; toolbars are menus drawn
as buttons in `egui-pop-api`. What ties them to egui-pop is narrow: the
kit paints them, the strings provider names them, the gesture map gives
the pull's keys, the debug bridge reads them; for toolbars, the command
world (menus, `when`, keys, arguments, providers, the icon registry).
The analysis is in [egui_stack directions](../egui-stack-directions.md),
section 5.

## Decision

- **egui_stack holds the plain cores**, egui and nothing else:
  - `sections::plan`: the arithmetic as it is (`fit`, `plan_drag`,
    `keep_title`, `pin_plan`, `rows_least`), without egui types;
  - `sections::Sections`, `Section`, `Stack`: the widget with its three
    body kinds, its behaviour, keyboard and accessibility, its persisted
    `Layout` by key and `laid_out`;
  - `toolbar::ToolRow`: a row or column of caller-given items (icon,
    label, tooltip, enabled, checked, busy, separator, submenu) with the
    overflow into a More button, returning what was clicked and what was
    laid out.
- **Three seams, no egui-pop type:** a painter trait per widget
  (`SectionsPainter`, `ToolRowPainter`), defaulting to egui's `Visuals`;
  a labels struct with English defaults; configuration values (the
  pull's modifiers, sizes).
- **egui-pop wraps them:** `egui_pop::core::sections` and
  `egui_pop::api::toolbars` keep their API, implementing the painters
  with the kit, the labels with the strings provider, the pull's keys
  from the gesture map; commands, `when`, keys, arguments, list
  providers, the icon registry, the value store, the debug collectors
  and the focus plugin stay egui-pop's. egui_stack never depends on
  egui-pop.
- **Sections move first**, once their behaviour has settled; the
  toolbar row follows.
- **Layering** (ADR 0001): egui_stack is an external dependency of
  `egui-pop-core` (L2). `egui-pop-model` loses `sections`; its path is
  kept as a deprecated re-export from `egui-pop-core` for one release.

## Consequences

- The widgets serve any egui application and get their testers; egui-pop
  keeps less code and its tests unchanged, run against the wrapper.
- A change of behaviour lands in egui_stack first, then in egui-pop on a
  bump: two repositories in step while sections still evolve.
- Sections can later become a container kind of egui_stack's tree
  (direction N of the analysis), so a panel could move between tabs
  and a section stack.
- Alternatives set aside: keeping both in egui-pop (no reuse, egui_stack
  without its stated growth); a third crate for sections alone (one more
  repository for a widget meant to meet the dock); moving the whole
  toolbars module (it is the command world, which egui_stack must not
  know).

## Progress

- **Sections: done (2026-11-04).** egui_stack 0.22.2 has
  `sections` (`Sections`, `Section`, `Stack`, `Layout`, `laid_out`, the
  arithmetic in `sections::plan`); the painter seam is the existing
  `DockPainter` (`section_band`, `section_boundary`) rather than a
  `SectionsPainter` of its own, the labels are `Labels::resize_section`,
  and the pull is configured by a `pull_when` closure. `egui_pop::core::sections`
  wraps it with its API kept; the memory key became `egui_stack.sections`,
  and a layout stored under egui-pop's old key moves there on first load.
  `egui-pop-model` cannot re-export the arithmetic, as egui_stack depends
  on egui and the model must not (ADR 0001, its layering test): its
  `sections` stays as a deprecated, unused copy for one release, and the
  arithmetic is re-exported as `egui_pop::core::sections::plan`.
- **Toolbar row:** not started.
