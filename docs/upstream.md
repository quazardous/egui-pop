# Upstream workarounds

The register of what egui-pop compensates for in its dependencies. Each
entry says what is compensated, where it is tracked upstream, how
egui-pop works around it (the code site carries an `// upstream:`
comment pointing here) and when the workaround can go.

Reviewed at every egui bump: remove what upstream fixed, update the rest.

## egui_kittest: no double click from queued events

- **Compensated**: a double click made of queued events never registers,
  because each queued event advances the clock by `step_dt` (0.25 s),
  beyond egui's 0.3 s double-click delay.
- **Upstream**: <https://github.com/emilk/egui/issues/8697>
- **Workaround**: `egui_pop_test::double_click(harness, label)` (and
  `double_click_at`) put the four button events in one frame, so egui
  sees both clicks within its delay instead of `step_dt` apart.
- **Remove when**: kittest ships a `double_click` helper or a per-step
  time.

## egui_dock: no per-tab, per-destination drop veto

- **Compensated**: egui_dock has no hook to refuse dropping a given tab
  in a given place (only `allowed_in_windows` and the global
  `AllowedSplits`), while egui-pop panels declare the areas they may live
  in.
- **Upstream**: <https://github.com/anhosh/egui_dock/issues/351> (related
  to #163 there).
- **Workaround** (for the future workbench): validate the layout after a
  drop and move the tab back to an allowed area, with a warning naming
  the panel and the area (no silent no-op, ADR 0003). The model side
  already exists: `Layout::repair` moves panels found in an area they may
  not be in.
- **Remove when**: the hook lands in egui_dock.

## egui: symbols missing from the default fonts

- **Compensated**: symbols such as ✓ ▸ ◆ ● render as missing glyphs with
  egui's default fonts.
- **Upstream**: fixed by <https://github.com/emilk/egui/pull/8491>
  ("Render any glyph on native", merged, expected in egui 0.37).
- **Workaround**: icons are named by role (`egui_pop_core::icon::Icon`)
  and drawn from the Phosphor icon font (`phosphor` feature), installed
  in a family of its own, or painted with lines and fills; widgets never
  put symbols in text. egui's `SubMenuButton` arrow (`⏵`) is replaced
  by a painted chevron in egui-pop's menus.
- **Remove when**: egui-pop moves to egui 0.37.

## eframe: no list of screens when a window is placed

- **Compensated**: a window's place is restored before the window
  exists, when eframe gives no list of the screens (only the current
  monitor's size afterwards, with no origin), so a place on a screen that
  is gone cannot be told from one on a second screen.
- **Upstream**: related to <https://github.com/emilk/egui/issues/4073>
  (choose the monitor at launch); a request for the screens' rectangles
  in `ViewportInfo` is not filed yet.
- **Workaround**: `egui_pop_api::window::WindowState::repair` takes the
  screens when they are known (tested with sample layouts); at start-up
  only the size is bounded and a position far away is dropped. Sizes are
  kept under 8192 points per side, past which the surface cannot be
  configured (<https://github.com/emilk/egui/issues/8361>).
- **Remove when**: eframe tells the screens before the window is placed.

## winit: no hit-test hook for a custom title bar on Windows

- **Compensated**: Windows 11 shows its Snap Layouts flyout only when the
  maximize button's area answers `HTMAXBUTTON` to `WM_NCHITTEST`; winit
  has no hook to answer it, so a title bar drawn by the application loses
  Snap Layouts.
- **Upstream**: <https://github.com/rust-windowing/winit/issues/3884>
  (open since 2024), <https://github.com/rust-windowing/winit/issues/2413>
  (a `HitTest` event) and its pull request
  <https://github.com/rust-windowing/winit/pull/3207>, not updated since
  2024. Maintainers point to subclassing the window outside winit; a 2026
  comment on the issue notes that subclassing does not help when a child
  window covers the client area.
- **Workaround**: egui-pop's frame (`egui_pop_core::frame`) publishes
  each window's hit regions every frame, in physical client pixels
  (`HitTable`, `HitRegions::hit` mapping a point to caption, minimize,
  maximize, close or a resize edge), so that a small Windows-only layer
  outside egui-pop (which forbids `unsafe`) can subclass the window and
  answer `WM_NCHITTEST` itself. Until that layer exists, applications
  drawing their own frame keep the system's title bar on Windows.
- **Remove when**: winit can answer a hit test for a point of the window.
