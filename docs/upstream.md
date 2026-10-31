# Upstream workarounds

The register of what egui-pop compensates for in its dependencies. Each
entry says what is compensated, where it is tracked upstream, how
egui-pop works around it (the code site carries an `// upstream:`
comment pointing here) and when the workaround can go.

Reviewed at every egui bump: remove what upstream fixed, update the rest.

## How a limit is handled

1. **Work around it in egui-pop**, at the narrowest place, with an
   `// upstream:` comment at the code site. An application or an
   extension never has to carry the workaround itself.
2. **Search upstream** (egui's issues and pull requests, or the
   dependency's) for the same limit. If none exists, **file a focused
   issue** with a minimal reproduction, written against upstream's code
   alone; a fix, when it is small and clear, goes with it as a pull
   request.
3. **Record it here**: what is compensated, where it is tracked
   upstream ("not filed yet" says it must be), the workaround, and when
   it can go.
4. **At every egui bump**, review the register: remove the workarounds
   upstream fixed (and their entries), update the rest.

An extension author who meets a limit egui-pop could compensate for
everyone opens an issue here, or a pull request adding the entry and
its workaround. Workarounds are not part of the extension contract
([GOVERNANCE.md](../GOVERNANCE.md)): they go when upstream fixes the
limit.

## egui: a link with selectable labels is announced as a label

- **Compensated**: `egui::Link` (and `Hyperlink`) sets its AccessKit
  role to `Link`, then draws its text through the label text selection
  when `selectable_labels` is on (egui's default), whose AccessKit
  update (`update_accesskit_for_text_widget`, with `Role::Label`)
  overwrites the role: screen readers, and tests looking for a link by
  its role, see a plain label. On egui 0.36.2, `ui.link` and
  `ui.hyperlink_to` report `Label` with selectable labels on and `Link`
  with them off. A link scrolled out of view keeps `Link`, as its text
  is not drawn. Links drawn by egui_commonmark in the viewer's pages
  keep the limit too.
- **Upstream**: fixed by <https://github.com/emilk/egui/pull/8574>,
  released in egui 0.37.0; nothing to file.
- **Workaround**: `egui_pop::api::links::Link` sets the role back to
  `Link` on the node after drawing it (`crates/egui-pop-api/src/links.rs`).
  egui's own `Hyperlink` on the built-in widgets page keeps the
  limit.
- **Remove when**: the bump to egui 0.37 (the text selection keeps the
  role the widget gave).

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

## egui: a scroll area cannot keep the wheel at its end

- **What is compensated:** a `ScrollArea` at the end of its range leaves
  the wheel's delta in the input, so the next scroll area out moves
  instead, with no option to keep it (the web's `overscroll-behavior:
  contain`). And it clamps its offset after drawing and asks for no
  repaint, so a body that grew, or lost rows, shows blank room under its
  last row until something else repaints.
- **Tracked upstream:** not filed yet.
- **Workaround:** `core::sections`: after a body's scroll area, the pointer
  over it, `contain_wheel` clears the vertical delta from the input, as
  `ScrollArea` itself does when it takes a wheel; `scrolled` asks for a
  repaint when the offset changed since the last pass.
- **When it can go:** when `ScrollArea` has such an option and repaints
  after clamping.

## egui: a scroll area reads no key

- **What is compensated:** a `ScrollArea` reads no key, says neither which ways
  it may scroll nor whether it has something to scroll on them, and egui's
  widgets read keys without taking them from the input (`Slider`'s arrows, a
  `TextEdit`'s Home and End), so nothing after them can tell a key was used.
- **Tracked upstream:** not filed yet.
- **Workaround:** `core::scroll_keys`, behind `UiPop::scroll_area`: after the
  content, it reads which ways there are from the bars' widgets and writes the
  offset to the area's `State`, for the next pass. A focused widget keeps the
  keys if it is a text input, senses drags or says so (`focus::takes_keys`).
- **When it can go:** when `ScrollArea` has keys of its own, and a widget's use
  of a key can be seen.

## egui: a selectable button changes size under the pointer

- **Compensated**: `Button::selectable` draws no frame at rest (neither
  selected nor hovered), and its frameless margin is the button padding
  plus the state's expansion minus its stroke width, where the framed
  margin comes to the padding alone. A theme with a stroke at rest
  (egui-pop's 1 px border) makes the button 2 px smaller at rest than
  hovered: a list of them changes height under the pointer, and a
  wrapped label can wrap differently.
- **Upstream**: <https://github.com/emilk/egui/issues/8125> (open). Fixed
  on egui's main and in 0.37.0: the default style hides the inactive frame
  with `Frame::invisible`, keeping its stroke and margins
  (`theme/default_style.rs`).
- **Workaround**: egui-pop's selectable rows (`widgets::SelectableRow`)
  keep egui's sizes, and `SelectableRow::steady` keeps the framed size
  in every state: the rows of a `Choose` dialog are steady
  (`dialogs::choice_row`, `// upstream:`).
- **Remove when**: egui-pop moves to egui 0.37 (check that a selectable
  row keeps its size under the pointer).

## egui: input in the pass of an AccessKit focus is lost

- **Compensated**: an AccessKit `Focus` action on a text field (from a
  screen reader, an agent, or a test) is only noted when egui reads the
  pass's input (`Focus::begin_pass`); the focus is handed over when the
  field registers its interest in it, as it is allocated. A `TextEdit`
  reads its events earlier, while its text is laid out, so the keys and
  text that arrive in the same pass as the action are dropped: the field
  gets the keyboard, not what was typed with it. Later passes are fine,
  which is why a kittest test (one queued event per step) does not see
  it; a platform batching the action with the keys does.
- **Upstream**: fixed in egui 0.37.0: `Focus::begin_pass` focuses the
  widget the action names at once, looked up among the focusable widgets
  of the previous pass (`memory/mod.rs`), before anything reads the
  input. No issue to file.
- **Workaround**: the focus plugin (`egui_pop_core::focus::install`,
  which an `App` installs) holds back the keyboard input that follows an
  AccessKit `Focus` action in the same pass and replays it, in order, at
  the start of the next pass, when the field has the focus
  (`focus::hold_after_accesskit_focus`, `// upstream:`). It covers every
  text field in the context, egui-pop's and an application's own.
- **Remove when**: egui-pop moves to egui 0.37 (check that text typed in
  the pass of an AccessKit focus reaches the field without the plugin).

## egui: a plugin's input hook runs in another window

- **Compensated**: `Context::begin_pass` calls `Plugin::input_hook` before the
  pass begins: the viewport stack and `Memory::viewport_id` are only set after
  it. For an immediate viewport's pass they are still the parent's, and for a
  deferred viewport's pass the context says the root and the memory says the
  last window that ran. So `Memory::focused()` and `surrender_focus` in a hook
  act on another window's focus, and no public method reads a given window's:
  only `RawInput::viewport_id` tells which window the input is for. In a real
  second window, Escape made the main window's text field let go and lost the
  press, and a field of that window never got step 1 of the Escape order.
- **Upstream**: not filed. egui's main branch has the same order (2026-10-30).
- **Workaround**: the focus plugin notes at the end of each window's pass what
  has the focus, the hook reads that for `RawInput::viewport_id`, and the focus
  is let go in `on_begin_pass`, where the memory is that window's
  (`focus::escape`, `// upstream:`). A host running passes with
  `Context::begin_pass` and `end_pass` by hand has no `on_end_pass`, so no step
  1 of the Escape order, and a terminal gets no keys.
- **Also compensated**: the terminal's key routing, which had the same cause.
  In real windows, keys typed in a detached window went to a terminal focused
  in the main window, past that window's own text field, and a terminal in a
  detached window got none of its keys. It reads the same note
  (`focus::focused_in_window`) and lets go on Shift+Escape in `on_begin_pass`
  (`terminal::keys`, `// upstream:`).
- **Remove when**: egui runs the hook with the window's memory, or offers a
  window's focus by id.

## egui: popups built from the context's style, not the opening `Ui`'s

- **Compensated**: egui builds the `Ui` of every `Area` from the
  context's style (`Ui::new` falls back to `ctx.global_style()`; `Area`
  has no style of its own), and every popup is an `Area`: a combo box's
  list, a tooltip (`on_hover_text`, `on_hover_ui`), a context menu, a
  menu (`menu_button`, `MenuBar`), egui's `Popup` and `Modal`, windows.
  A style set on one `Ui` (a theme drawn in one part of a window, egui-pop's
  `ScopedTheme`) does not reach the popups opened inside it. The hooks
  that exist (`Popup::style`, `ComboBox::popup_style`,
  `MenuConfig::style`, all a `StyleModifier` applied over the context's
  style) only help at call sites one owns: `Response::on_hover_text`
  and `Response::context_menu` take none, and a widget of another crate
  opening a popup cannot be reached. The same in egui 0.37.0 (`ui.rs`,
  `containers/popup.rs`); its experimental style providers see a
  popup's `UiStack`, which does not lead back to the `Ui` that opened it.
- **Upstream**: <https://github.com/emilk/egui/issues/4692> (tooltips
  should inherit the parent's style, open; `ComboBox::popup_style`,
  <https://github.com/emilk/egui/pull/7360>, covers combo boxes at the
  call site only). A comment widening it to every popup, with a
  proposed API, is not posted yet (draft:
  [upstream-drafts/egui-popups-inherit-ui-style.md](upstream-drafts/egui-popups-inherit-ui-style.md)).
- **Workaround**: while a `ScopedTheme` draws, egui's context styles
  (dark and light) are the scoped theme's, made from the installed
  style and kept per scope, and the installed ones are put back when
  the scope ends, even after a panic (`ScopedTheme::within`,
  `// upstream:` in `theme.rs`). egui 0.36 draws its popups while the
  widget that opens them is drawn, so they follow the scope. A popup
  egui-pop draws later, out of the scope (`menus::popup_at`, drawn by
  the shell), records the scope it was opened in and is drawn within
  it. Limits: egui's light or dark mode (`ctx.theme()`) stays the
  installed one, and a change to the context's style made while a
  scope draws is undone when it ends (themes are installed between
  frames).
- **Remove when**: egui's popups take the style of the `Ui` they are
  opened from; `ScopedTheme::within` then only sets egui-pop's tokens
  and kit (still needed for a popup drawn after its scope).

## egui: scroll bar colours come from the widget visuals

- **Compensated**: egui's `ScrollStyle` carries a scroll bar's sizes
  and opacities but no colour. egui 0.36 draws the thumb in the
  colours of the widget states (`visuals.widgets.inactive`, `hovered`,
  `active`): their `fg_stroke` colour with `foreground_color`, else
  their `bg_fill`, and their `corner_radius`; these are the button's
  colours and corners. It draws the track in `extreme_bg_color`, which
  text fields, canvases and progress bars also use. A theme cannot give
  the scroll bars colours of their own without changing every button
  and field. egui 0.37.0 is the same: its `ScrollStyle` has no colour
  either (`style.rs`, `containers/scroll_area.rs`).
- **Upstream**: not filed yet (draft:
  [upstream-drafts/egui-scroll-style-colours.md](upstream-drafts/egui-scroll-style-colours.md)).
- **Workaround**: the kit's `scroll-bar` goes into `ScrollStyle` with
  the theme for what it can carry (width, margin, placement,
  visibility, no track), in every scroll area. Its colours come from a
  `ScrollBar` scope: `UiPop::scroll_area` shows the area in a `Ui`
  whose widget states' foreground colours and corners are the thumb's,
  and whose `extreme_bg_color` is the track's, and gives the content
  the style from outside the scope again (`// upstream:` in `ui.rs`).
  An `egui::ScrollArea` shown directly keeps the button's colours, and
  the kit reports it in its problems.
- **Remove when**: egui's `ScrollStyle` (or its visuals) has its own
  colours for the thumb in each state and for the track, and its own
  corner radius; the kit then writes them with the theme,
  `Stock::ScrollBar` and the problem go, and `UiPop::scroll_area`
  becomes a plain `ScrollArea::show`.

## egui: AccessKit bounds ignore layer transforms

- **Compensated**: a widget in a transformed layer (egui's `Scene`)
  reports its rectangle to AccessKit in the layer's coordinates, before
  the transform: screen readers (and kittest, which clicks at a node's
  bounds) place it where it is not.
- **Upstream**: not filed yet (draft:
  [upstream-drafts/egui-accesskit-bounds-layer-transform.md](upstream-drafts/egui-accesskit-bounds-layer-transform.md)).
- **Workaround**: the node graph editor sets the bounds of its nodes,
  ports, chevrons, groups, stacks and sliders to their rectangles on
  screen (`graph::Cx::bounds`, `// upstream:`).
- **Remove when**: egui applies the layer's transform to the bounds it
  reports.

## egui: the focus cannot be given back as a modal closes

- **Compensated**: a widget drawn under the top modal layer gives up the
  focus as it is drawn (`create_widget`, `allows_interaction`), and that
  layer is the previous pass's (`top_modal_layer`). So `request_focus` on the
  widget behind a modal is undone in the pass the modal closes, and in the
  next when the modal was still drawn in that pass: on egui 0.36.2, the focus
  ends on nothing.
- **Upstream**: not filed yet.
- **Workaround**: the overlay stack keeps the place and asks for the focus
  once `allows_interaction` says the widget's layer can hold it (`// upstream:`
  at `give_focus_back`, `crates/egui-pop-core/src/overlay.rs`).
- **Remove when**: egui takes a modal layer off in the pass it stops being
  drawn, or keeps a focus requested for a widget below it.

## egui: a scroll target stops at the innermost scroll area

- **Compensated**: `Response::scroll_to_me` and `Ui::scroll_to_rect` set a target
  that the innermost `ScrollArea` takes whole, on both axes, even when it cannot
  move on that axis or does not overflow, so the areas around it never see it
  (`Prepared::end`). Reproduced on egui 0.36.2 with plain egui, a button in an
  area nested in a scrolled area: `scroll_to_me` leaves it out of view
  (`egui_scroll_target_stops_at_the_innermost_area`,
  `crates/egui-pop-test/tests/focus_into_view.rs`). And nothing says which areas
  hold a widget, or lets a central hook set a target for them:
  `Action::ScrollIntoView`, which a hook could inject, stops at the same place
  and always centres.
- **Upstream**: not filed yet. The nearest is [#985](https://github.com/emilk/egui/issues/985), from 2023, asking for a way to name the area: it saw every area scroll, the opposite of 0.36.2.
- **Workaround**: the focus plugin writes the offsets of the areas itself, from
  the previous pass (`// upstream:` in `crates/egui-pop-core/src/reveal.rs`): the
  ids in `PassState::used_ids` holding a `scroll_area::State` are the areas; the
  widgets `id.with(axis)` are their bars, giving their reach; the widgets between
  the area's content `Ui` and its bars are what it holds. These are egui's
  internals read through public items: the test file above fails if a bump
  changes them. An area with hidden bars is not found.
- **Remove when**: a scroll target bubbles to the outer areas as far as it needs
  to, or a widget's enclosing areas can be read.

## egui: a focus dropped leaves no place

- **Compensated**: when egui drops the focus (Escape, a widget no longer drawn) it
  keeps no trace of it: Tab starts at the window's first stop, Shift+Tab at its
  last, and a widget removed has no successor. Reproduced on egui 0.36.2 with the
  plugin off (`crates/egui-pop-test/tests/focus_place.rs` fails).
- **Upstream**: not filed yet.
- **Workaround**: the focus plugin remembers the place
  (`// upstream:` in `crates/egui-pop-core/src/place.rs`), holds back a Tab pressed
  with nothing focused for the pass, so that a key binding can take it, then gives
  the widget the focus back at the start of the next pass and tells egui to move
  on from it, and hands a removed widget's focus on before egui's own end of pass.
- **Remove when**: egui's focus has a notion of the last place, or a successor.

## egui: a popup's frame is a Tab stop

- **Compensated**: `Popup` senses `Sense::click()` by default, so the frame of
  every popup (`MenuButton`, `SubMenu`, `Popup::menu`, a context menu, a combo
  box's list) is a focusable widget with no role and no name, showing nothing:
  Tab goes from a menu's button to its frame, then to its first item. Only a
  `Popup` built by hand can set another sense; `MenuButton`, `ComboBox` and
  `Response::context_menu` have nowhere to say it. Reproduced on egui 0.36.2 with
  the plugin off (`crates/egui-pop-test/tests/focus_place.rs` fails).
- **Upstream**: fixed in egui 0.37.0, whose `Popup` senses `Sense::CLICK`.
- **Workaround**: when the focus lands on a popup's frame, the focus plugin
  moves it on in the next pass, the way Tab was going
  (`// upstream:` in `crates/egui-pop-core/src/place.rs`).
- **Remove when**: egui-pop moves to egui 0.37.

## egui: Tab wraps only in a modal layer

- **Compensated**: Tab goes by drawing order through the whole window and wraps
  only inside `Memory::set_modal_layer`, which also blocks the pointer below, so a
  popover closed by a click outside cannot use it. A focused widget's
  `EventFilter` says whether it takes Tab, but neither the filter nor the direction
  egui read from the key can be asked, so whether a press will move the focus is
  unknown before the pass. Read in egui 0.36.2 (`Focus::begin_pass`,
  `interested_in_focus`).
- **Upstream**: no issue filed yet.
- **Workaround**: `focus::keep_tab`. egui moves the focus by its own rule, so a
  widget taking Tab keeps it, and the focus plugin corrects where it landed: a
  hidden stop ends the popover's order, for Tab from the last stop to land on, and
  the next pass moves the focus from there, or from a widget behind, to the
  popover's first or last stop (`// upstream:` in
  `crates/egui-pop-core/src/place.rs`).
- **Remove when**: egui can wrap Tab in a layer without blocking the pointer, or
  says where a press of Tab is going.

## egui: no side-effect free read of a widget's response

- **Compensated**: `Context::read_response` runs the "clicked
  elsewhere" rule (`get_response`): on a frame with a click that did
  not land on the widget, a focused widget loses the focus. Read every
  pass for the focused widget, to find its layer or place, it drops the
  focus a widget took when the click landed on one of its children (a
  tree focused through its clicked row). The pass's widget lists, which
  hold the layer and rectangles, are not public.
- **Upstream**: not filed yet.
- **Workaround**: `focus::peek_response` reads the response with
  `surrender_focus_on` set to `Never` for the call (`// upstream:` in
  `focus.rs`); the focus chain, hover mode, F6 and the palettes'
  Escape check use it.
- **Remove when**: egui offers a read of a widget's rectangle and layer
  (or a response) without interaction side effects.

## egui: text blurs in a Scene zoomed past 100 %

- **Compensated**: egui draws text at its size and scales the layer, so
  a `Scene` zoomed in shows blurred glyphs.
- **Upstream**: <https://github.com/emilk/egui/issues/4813>
- **Workaround**: the node graph editor stops at 150 % by default
  (`GraphEditor::zoom_range`), and its overview draws titles at a few
  larger sizes rather than scaling small ones.
- **Remove when**: egui lays text out at the layer's scale.

## eframe: no custom cursor on the web or in immediate viewports

- **Compensated**: `Context::set_cursor_image` reaches winit only where
  egui-winit has the event loop (eframe's root and deferred viewports);
  immediate viewports and the web drop the bitmap.
- **Upstream**: web: <https://github.com/emilk/egui/issues/3848> (custom
  pointer icons, open; the web integration carries a TODO for CSS
  `url(...)` cursors). Immediate viewports: a known limitation of
  <https://github.com/emilk/egui/pull/8155>, not filed yet (draft:
  [upstream-drafts/egui-cursor-image-immediate-viewports.md](upstream-drafts/egui-cursor-image-immediate-viewports.md)).
- **Workaround**: every bitmap cursor is shown with a system cursor as
  its fallback (`cursor::apply`, `// upstream:`): the tool cursor's
  nearest system cursor, or a registered image's own fallback.
- **Remove when**: the bitmap reaches the system everywhere.

## winit: no system cursor size

- **Compensated**: winit does not say how large the system's cursors
  are, so tool cursors could not match them.
- **Upstream**: <https://github.com/rust-windowing/winit/issues/3788>
  (open); a comment with this use case is not posted yet (draft:
  [upstream-drafts/winit-cursor-size-comment.md](upstream-drafts/winit-cursor-size-comment.md)).
- **Workaround**: `cursor::system_size` reads `XCURSOR_SIZE` (X11 and
  Wayland desktops), else 24 points; `CursorSettings::size` overrides
  it.
- **Remove when**: winit reports the cursor size (Windows'
  `SM_CXCURSOR`, the desktop's cursor theme size).

## winit and egui: no owner window for a viewport

- **Not built**: a palette or toolbox as a native utility window (U3 in
  [hosts.md](hosts.md)). Without an owner, such a window floats above
  every application instead of only above its own, and is listed in the
  taskbar and the window switcher.
- **Upstream**: winit has an owner only on Windows
  (`WindowAttributesExtWindows::with_owner_window`). The cross-platform
  request is <https://github.com/rust-windowing/winit/issues/4256>
  (open; it names X11's `XSetTransientForHint`, macOS child windows and,
  in a comment, Wayland's `xdg_toplevel.set_parent`). A comment with
  this use case is not posted yet (draft:
  [upstream-drafts/winit-owner-window-comment.md](upstream-drafts/winit-owner-window-comment.md)).
  egui's `ViewportBuilder` has no owner either; an issue is drafted for
  when winit has one
  ([upstream-drafts/egui-viewport-owner-issue.md](upstream-drafts/egui-viewport-owner-issue.md)).
- **Workaround**: palettes stay inside the main window; a detached
  panel is an ordinary top-level window, not a utility window.
- **Remove when**: winit gives an owner on every desktop platform and
  egui's `ViewportBuilder` passes it through.

## egui_table: unnamed resize handles, rows drawn per region

- **Compensated**: the column resize handles are interactive widgets
  without an accessible name; `TableDelegate::row_ui` and
  `header_cell_ui` are called once per region (sticky and scrolled
  columns), also for the sticky region when there are no sticky columns;
  `TableDelegate::prepare` gets a reversed `visible_rows` when the table is
  clipped away, scrolled out of a scroll area around it.
- **Upstream**: egui_table lives in rerun-io/egui_crates now. Not filed
  yet (drafts:
  [upstream-drafts/egui_crates-table-resize-handle-name.md](upstream-drafts/egui_crates-table-resize-handle-name.md)
  and
  [upstream-drafts/egui_crates-table-row-ui-empty-sticky-region.md](upstream-drafts/egui_crates-table-row-ui-empty-sticky-region.md)).
  On egui_table's main the header is drawn once, so only `row_ui` is still
  called for the empty region.
- **Workaround**: `egui-pop-views`' data table names each handle after
  its column once the table is drawn, and skips the rows and header
  cells of an empty region (`table.rs`, `// upstream:`).
- **Remove when**: egui_table names its handles and skips empty regions.

## egui_commonmark: a code block is an unnamed text field

- **Compensated**: `egui_commonmark_backend::elements::code_block` draws
  a code block as a read-only `egui::TextEdit::multiline` with no hint and
  no label, so AccessKit reports a `MultilineTextInput` without a name; it
  returns nothing, and the field's id comes from `ui.next_auto_id()`, so a
  caller cannot name it. Seen on 0.25.0 (egui 0.36, the version in use);
  0.26.0 (egui 0.37, the current release) and the `master` branch still draw
  it the same way. The block's copy button is named by its glyph (`🗐`, then
  `✔`).
- **Reproduction**: `CommonMarkViewer::new().show(ui, &mut cache,
  "```text\na\n```")` in an `egui_kittest` harness: the tree holds a node
  of role `MultilineTextInput` with no label.
- **Upstream**: not filed yet.
- **Workaround**: `egui-pop-views`' viewer names the unnamed multi-line
  fields among the widgets a page just drew, on their AccessKit nodes
  (`viewer/view.rs`, `// upstream:`). The copy button keeps its glyph.
- **Remove when**: egui_commonmark names its code blocks, or takes a name
  for them.

## egui_kittest: no way to move a window to a screen of another scale

- **Compensated**: `Harness::set_pixels_per_point` changes egui's zoom
  factor, not the window's native scale, so a test cannot simulate a
  window moved to a screen of another scale (winit's scale change).
- **Upstream**: not filed yet (draft:
  [upstream-drafts/egui-kittest-native-pixels-per-point.md](upstream-drafts/egui-kittest-native-pixels-per-point.md),
  proposing `Harness::set_native_pixels_per_point`).
- **Workaround**: `egui_pop_test::set_native_scale` writes the root
  viewport's `native_pixels_per_point` in the harness's input before the
  next step, as egui-winit does on a scale change.
- **Remove when**: kittest has such a setter.

## egui: keyboard zoom on by default

- **Compensated**: `Options::zoom_with_keyboard` is on by default: Ctrl
  and `+`, `-`, `0` change the zoom factor of every window, the keys an
  application's content zoom (an image viewer's) uses too.
- **Upstream**: a default, not a defect; nothing to file.
- **Workaround**: egui-pop's application turns it off and owns the
  interface zoom (`egui_pop_api::zoom`): a setting in fixed steps, and
  commands bound by default to `Ctrl+Alt+=`, `Ctrl+Alt+-` and
  `Ctrl+Alt+0`, rebindable like any command.
- **Remove when**: never; this is egui-pop's design.

## egui_dock: no per-tab, per-destination drop veto

- **Compensated**: egui_dock has no hook to refuse dropping a given tab
  in a given place (only `allowed_in_windows` and the global
  `AllowedSplits`), while egui-pop panels declare the areas they may live
  in.
- **Upstream**: <https://github.com/anhosh/egui_dock/issues/351> (related
  to #163 there).
- **Workaround**: each area is a dock of its own, its tabs never allowed
  into egui_dock's windows; a tab dropped over another area is moved
  there by the workbench (`egui_pop_api::workbench`) when the panel's
  rules allow it, and refused otherwise, with the drop hint saying why
  and the refusal reported (no silent no-op, ADR 0003). `Layout::repair`
  moves panels found in an area they may not be in.
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
  maximize, close or a resize edge). On Windows, `FramePlugin` (when the
  application draws the frame) subclasses the main window through
  `egui-pop-win32` (`SetWindowSubclass`, `unsafe` in one module,
  with no egui dependency) and answers `WM_NCHITTEST` from them:
  `HTCAPTION` (Windows moves the window, with Aero Snap), `HTMINBUTTON`,
  `HTMAXBUTTON` (Snap Layouts) and `HTCLOSE`, the resize codes, else
  `HTCLIENT`. The buttons' non-client clicks never reach winit, so the
  layer swallows them and hands them back with hover, as are a double
  click and a right click on the caption: the `frame.*` steps still run.
  The layer is installed and removed through the `frame.hit_test` step.
  Limits: only the main window (its handle comes from eframe's creation
  context; auxiliary windows keep egui's own hit handling), and a child
  window covering the client area would take the hit tests first (the
  2026 comment above).
- **Remove when**: winit can answer a hit test for a point of the window.

## egui-wgpu: no surface format or colour space to choose

- **Compensated**: wgpu 30 configures surfaces in any colour space the
  platform offers (`SurfaceConfiguration::color_space`: extended sRGB,
  scRGB, Display P3, BT.2100 PQ and HLG) and reports them per format
  (`SurfaceCapabilities::color_spaces`) with the display's headroom
  (`Surface::display_hdr_info`). eframe 0.36 does not let an application
  use any of it: egui-wgpu picks the format itself
  (`preferred_framebuffer_format`: `Rgba8Unorm` or `Bgra8Unorm`), and
  configures the surface with wgpu's default colour space
  (`configure_surface` builds on `get_default_config`); `WgpuConfiguration`
  has no field for either. Its renderer also writes sRGB-encoded values
  to every format that is not `*Srgb` (`fs_main_gamma_framebuffer`), so
  the interface is right only on sRGB and extended sRGB surfaces.
- **Upstream**: not filed yet. What would plug in: a preferred format
  and colour space in `egui_wgpu::WgpuConfiguration` (used by
  `RenderState::create` and `configure_surface`), and the renderer's
  fragment entry chosen by the colour space's transfer (linear for scRGB,
  an encoding pass for PQ and HLG) rather than by `is_srgb()`.
- **Workaround**: none possible inside eframe for the window's own
  surface. The viewport's display stage implements every transfer and
  the P3 and BT.2020 primaries (CPU and shader); it reports truthfully
  what an eframe window draws (sRGB, with the reason,
  `display::Resolution`); a host with its own surface configures it with
  `egui_pop_viewport::gpu::choose_surface` and declares it
  (`gpu::install_for_surface`, `display::set_target`); on Wayland the
  image gets a surface of its own (next entry). The issue text is
  drafted below, under the Wayland entry.
- **Remove when**: egui-wgpu takes a surface format and colour space;
  then `gpu::install` reads them from the render state.

## egui-winit: copy, cut and paste arrive as events, their keys swallowed

- **Compensated**: egui-winit turns the platform's copy, cut and paste
  chords (Command or Ctrl with C, X, V; Ctrl+Insert, Shift+Insert,
  Shift+Delete on Windows) into `Event::Copy`, `Event::Cut` and
  `Event::Paste(text)` and sends no key event: a command bound to them
  in the keymap never runs. A paste is read synchronously, as text
  only, and nothing reads the primary selection. When the system
  clipboard holds no text (a file manager's copy of files, a browser's
  "Copy image"), the paste chord makes no event at all: its press is
  dropped, only its release reaches egui as a key event.
- **Upstream**: egui 0.37 answers most of it: `NativeOptions::clipboard_shortcuts`
  (egui-winit's `State::set_clipboard_shortcuts`), when off, sends the
  chords as plain key events, and `Event::PasteImage` (emilk/egui#8472)
  carries an image when the clipboard holds no text. A clipboard
  holding only files still makes no event with the shortcuts on
  (emilk/egui#1167 asks for file paste events).
- **Workaround**: while a clipboard endpoint has the focus (and no text
  field does), the clipboard plugin takes the three events and runs
  `edit.copy`, `edit.cut`, `edit.paste` (Shift held: paste special)
  with the pasted text (`clipboard::pass`, `// upstream:`); the keymap
  binds the same keys for every other backend (kittest, the web). The
  paste chord with no text is recognised by its release: a release of
  the paste key whose press egui never saw, with no paste event since
  and the command modifier held meanwhile, runs `edit.paste` with
  `no_text`, and the system backend reads what the clipboard holds
  (files, an image, HTML); with the default backend, which reads
  nothing, that paste is refused when the owner's text is no longer
  there. It fires on the release, not the press, and a key repeat
  pastes once.
- **Remove when**: egui-pop moves to egui 0.37: turn
  `clipboard_shortcuts` off in the native options, bind Ctrl+C, Ctrl+X
  and Ctrl+V (and Windows' Insert and Delete chords) in the keymap for
  every backend, and give text fields their copy, cut and paste events
  from the system backend (egui's `TextEdit` reads `Event::Copy`,
  `Event::Cut` and `Event::Paste`), so the key is seen on its press
  whatever the clipboard holds.

## Wayland: no colour management through winit

- **Compensated**: on Wayland, colour management is the
  `wp_color_management_v1` protocol (staging, in wayland-protocols since
  1.41): a surface gets an image description (primaries, transfer,
  luminances), and the compositor gives each surface's preferred one.
  winit 0.30 binds none of it (no colour-management object, no event for
  the preferred description), eframe configures every window 8-bit sRGB
  (above), and wgpu's Vulkan path relies on the driver: Mesa's Wayland
  window system integration (25.1 and later) maps some
  `VkColorSpaceKHR` to the protocol, none of them BT.2020 with a 2.2
  power, and holds the colour-management object of every swapchain's
  surface when the compositor announces the `srgb` transfer to it
  (research: [wayland-colour.md](wayland-colour.md)).
- **Upstream**: winit's
  <https://github.com/rust-windowing/winit/issues/4131> (colour
  management getters and events, open); the wgpu, egui-wgpu and Mesa
  requests below are not filed yet.
- **Workaround**: the image gets a surface of its own
  (`egui_pop_viewport::image_surface`, `// upstream:` in
  `image_surface.rs`): `egui-pop-wayland` joins winit's connection from
  the window's raw handles, makes a subsurface below the window, tags it
  through the protocol (or lets the driver declare a colour space where
  it holds the surface), and reads the preferred description through
  `get_surface_feedback`; the viewport renders into it with wgpu and
  punches a hole in the interface. It needs a transparent window and
  serves the main window only.
- **Remove when**: egui-wgpu takes a surface colour space and wgpu
  offers one for the wanted encoding (or a pass-through one an
  application can tag), and winit exposes the preferred description;
  then the window's own surface carries the image, and the interface is
  encoded for it.

### Drafted requests (not filed)

**wgpu: a pass-through surface colour space, and BT.2020 linear.**

> Title: `SurfaceColorSpace::PassThrough` and `Bt2020Linear` on Vulkan
>
> wgpu 30 configures a surface's colour space (`SurfaceConfiguration::color_space`,
> #9658), mapping `SRGB_NONLINEAR`, `EXTENDED_SRGB_LINEAR`,
> `EXTENDED_SRGB_NONLINEAR`, `DISPLAY_P3_NONLINEAR`, `HDR10_ST2084` and
> `HDR10_HLG` (`wgpu-hal/src/vulkan/conv.rs`); the others a driver
> reports are dropped (`unknown_color_spaces_are_dropped`). Two of them
> matter on Wayland:
>
> - `VK_COLOR_SPACE_PASS_THROUGH_EXT`: Mesa's Wayland WSI then leaves
>   the surface's `wp_color_management_surface_v1` alone. With any other
>   colour space, Mesa creates that object (one is allowed per
>   `wl_surface`) on compositors announcing the `srgb` transfer, even for
>   plain sRGB, so an application that describes its surface itself
>   through the protocol (an ICC profile, BT.2020 with a 2.2 power,
>   custom primaries) cannot use a wgpu swapchain there: tagging it is a
>   fatal `surface_exists` error.
> - `VK_COLOR_SPACE_BT2020_LINEAR_EXT`: Mesa maps it to `bt2020` +
>   `ext_linear` wherever the compositor announces both (GNOME and sway
>   included, which offer no scRGB), a wide SDR encoding.
>
> Proposal: `SurfaceColorSpace::PassThrough` (documented as "the
> presentation engine applies no colour management; the application
> describes the surface by other means") and `Bt2020Linear`, with their
> `SurfaceColorSpaces` flags, mapped on Vulkan only and reported from
> `vkGetPhysicalDeviceSurfaceFormatsKHR` as the others are.

PR idea: two variants in `wgpu-types/src/surface.rs` and their flags,
the two arms in `wgpu-hal/src/vulkan/conv.rs` both ways, a line in the
`03_hdr_surface` example's list.

**egui-wgpu / eframe: a surface colour space** (see also the entry
above).

> Title: Let `WgpuConfiguration` choose the surface format and colour space
>
> egui-wgpu picks the surface format (`preferred_framebuffer_format`) and
> configures wgpu's default colour space (`configure_surface` builds on
> `get_default_config`); an application cannot ask for a wide-gamut or
> HDR surface, which wgpu 30 supports (`SurfaceConfiguration::color_space`,
> `SurfaceCapabilities::format_capabilities`). The renderer also picks
> its output encoding from `format.is_srgb()` (`fs_main_linear_framebuffer`
> or `fs_main_gamma_framebuffer`), so on a linear half-float surface
> (scRGB) the interface would be written gamma-encoded.
>
> Proposal: `WgpuConfiguration::surface_format` and `surface_color_space`
> as callbacks over the surface's capabilities (`Fn(&SurfaceCapabilities)
> -> Option<(TextureFormat, SurfaceColorSpace)>`, default: today's
> choice), used by `RenderState::create` and `configure_surface`; the
> chosen colour space exposed in `RenderState`; the renderer's output
> transfer chosen from it (linear for scRGB and the linear spaces,
> sRGB-encoded otherwise), with a hook for a matrix and transfer
> (Display P3, PQ) later.

PR idea: the two fields and their default in `egui-wgpu/src/lib.rs`,
their use in `winit.rs` (`configure_surface`) and `RenderState::create`,
and a `Renderer` option for the output transfer replacing the
`is_srgb()` test; eframe passes them through `NativeOptions::wgpu_options`.

**winit: the preferred image description** (a comment on
rust-windowing/winit#4131 rather than a new issue).

> On Wayland, `wp_color_manager_v1.get_surface_feedback` gives a
> window's preferred image description (primaries, transfer,
> luminances, the display's target volume) and `preferred_changed` its
> changes, which is what an application needs to choose an encoding and
> an HDR headroom per window. A `Window::preferred_color_description()`
> (a plain struct of those values) and a `WindowEvent::ColorDescriptionChanged`
> would cover it; binding the manager in winit's Wayland backend and one
> feedback object per window is enough. Today an application binds it on
> winit's connection through the raw handles (as egui-pop does).

**Mesa (Wayland WSI): leave sRGB surfaces untouched** (for the record;
Mesa's call).

> `needs_color_surface()` creates a `wp_color_management_surface_v1`
> for `VK_COLOR_SPACE_SRGB_NONLINEAR_KHR` when the compositor announces
> the `srgb` primaries and transfer, but `wsi_wl_swapchain_update_colorspace`
> never sets a description for it. The object only takes the surface's
> single slot, so a client describing the surface itself (a subsurface
> with an ICC profile, for one) cannot. Creating it lazily, when a
> description is set, would leave sRGB swapchains untouched. Separately,
> `SRGB_NONLINEAR` and `DISPLAY_P3_NONLINEAR` map to the `srgb` transfer,
> deprecated in version 2 and not announced by KWin, wlroots or Weston,
> so Display P3 is offered only on GNOME and Hyprland; `gamma22` (or
> `compound_power_2_4` on version 2) would offer it everywhere.
