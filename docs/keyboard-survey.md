# Keyboard survey: moving around without a mouse

This survey asks how far the keyboard alone gets you in egui-pop and in
its main user, the photo editor Helios: Tab order, moving between panels
and regions, menus, lists and trees, dialogs, the gallery's side list,
the node graph and the image viewport; what cannot be reached, where the
focus gets lost or cannot be seen, and what eats a key. It compares that
with VS Code, JetBrains IDEs, Blender, darktable, Lightroom, GIMP,
Photoshop, Qt, GTK and the WAI-ARIA Authoring Practices, and ends with a
ranked list of changes, one pull request each, split between egui-pop
and Helios.

Read on 2026-10-30, against egui 0.36.2 (and 0.37.0 where it differs),
egui_dock 0.21.1 and egui-pop's `main`.

**How much of this was run.** This study was first written without
building anything. Its walk with the keyboard in a headless harness has
since been run for egui-pop, on the gallery and a few small harnesses, on
2026-10-30, with real key presses
([the walk](#the-walk), checks 1 to 18); Helios's half, checks 19 to 26,
is **still owed**. Every finding about today's state says where it comes
from:

- **[walk]**: seen by a keyboard walk (`egui_pop_test::walk`), pinned or
  listed as a gap in `crates/egui-pop-gallery/tests/keyboard_walk.rs`.
- **[read]**: from reading the code or a document; not seen happening.
  Treat it as a strong hypothesis.
- **[test]**: an existing kittest test asserts it, named where it helps.

Many of those tests put the focus on a widget through AccessKit
(`.focus()`) and then press keys. They show what a widget does once it
has the focus, not that the keyboard can get there. Behaviour quoted from
forums and bug trackers is marked as such.

## Summary

- **F6 is built and reaches nothing.** `tab::scope` and F6 work
  ([walk] on the gallery's Inputs page), but no shell panel opens such a
  scope ([walk] in the gallery, [read] in Helios; `docs/inputs.md` says so
  under "Not yet"). In an application, Tab through everything in drawing
  order is the only way across. F6 between regions is the one thing
  VS Code, GTK and the ARIA splitter pattern share.
- **Two ideas of "focused panel" that do not talk.** The panel the keys
  obey (`focusedPanel`) follows clicks and `workbench.focus`; egui's
  focus follows Tab. `workbench.focus` promises "gives it the keyboard"
  and moves no widget focus; Tab into another panel does not change
  `focusedPanel` ([walk]: clicked in the Outline, then Tab to a button of
  the Notes, it still names the Outline). So in Helios, Tab from the image
  to the filmstrip should leave `C` adding a circle mask ([read], Helios's
  walk is owed).
- **The way in can be closed.** Where a palette floats and no widget has
  the focus, Tab hides the palettes instead of moving ([test]), and F6 is
  inert ([walk]). That is Helios's usual state ([read]). The habit is right for a
  photo editor (Lightroom, Photoshop, GIMP and darktable all do it), so
  the answer is F6, not giving Tab back.
- **The focus can land on nothing, or out of view.** A window frame's
  caption, a side panel's resize line, a flyout's edge and an open menu's
  frame are stops with no role and no name, and the first two change no
  pixel when focused ([walk]). A focused plain button is seen, in the
  accent's outline in `pop-dark`, but has the pressed button's pixels
  ([walk]). A widget scrolled out of view keeps the focus out of view
  ([walk]): 35 rows of the gallery's side list on its first screen, the end
  of the settings page. VS Code, Qt, GTK and ARIA
  all show an indicator, and GTK and Qt know whether the keyboard moved it.
- **The focus has little memory.** A popover gives it back ([walk]); a
  dialog or the command palette closed by Escape does not, against what
  reading the overlay's tests said ([walk]). After Escape, when the focused
  widget disappears, or when a button changes the panels, the next Tab
  starts from the top of the window ([walk]).
- **Enter opens a dialog and answers it.** Enter on a focused button that
  opens a dialog also accepts the dialog, unless it is a danger: the key is
  still there when the dialog is drawn in the same pass ([walk]). Space
  opens it and leaves it open. Reading did not see this.
- **Menus are the weakest common part, and egui 0.37 fixes most of it.**
  Alt and a letter opens a menu ([walk]); after that egui 0.36 has no
  arrow keys in it ([walk]). There is no F10, no underlined letter, and no key for
  a context menu ([read]). egui 0.37 brings arrow keys inside menus
  ([#8548](https://github.com/emilk/egui/pull/8548)) and a `Menu` key.
- **The data views are already right.** Tree, list, table and grid take
  one Tab stop and move with the arrows; tree and list add type-ahead
  ([test]). This matches ARIA, Qt and JetBrains. The gaps are the lists
  drawn by hand from `selectable_label` (the gallery's side list, Helios's
  Modules panel), where every row is a Tab stop.
- **Keys are complete, routes are not.** Every command runs from the
  palette (F1) and has rebindable keys, a resolver with a trace, and
  surfaces: ahead of most of these systems. The study's changes are about
  moving focus, not about the keymap. The one exception read here, a
  rebound key losing its `when` clause, has been fixed since ([walk]).

## egui today

From egui 0.36.2's source (`memory/mod.rs`, `context.rs`,
`containers/menu.rs`, `input_state/mod.rs`).

- **One focused widget per window.** `Focus` holds one id and a filter
  (`EventFilter`, set by `set_focus_lock_filter`) saying which of Tab, the
  arrows and Escape the widget keeps.
- **Tab is drawing order.** Each focusable widget registers as it is
  drawn; Tab gives the focus to the next one that registers, Shift+Tab to
  the one before, and both wrap across passes. Which layer or panel it is
  plays no part. A modal layer narrows it: below the top modal, widgets
  are not interested. There are no tab indices and no groups.
- **What is focusable.** `Sense::click()`, `Sense::drag()` and
  `Sense::click_and_drag()` all include `FOCUSABLE`; `Sense::DRAG`
  alone does not. So anything draggable made with the helper is a Tab
  stop, whether or not it answers a key or draws focus.
- **Arrows are a spatial guess.** With a plain arrow and no filter, egui
  looks through every focusable widget's rectangle in a 90° cone and takes
  the best score ("There is a lot of room for improvement here", its
  comment). It knows nothing of panels, and the rectangles include widgets
  not visible. With nothing focused, arrows do nothing.
- **Escape drops the focus** of any widget that has not claimed it, to
  nothing.
- **A widget not drawn loses the focus,** with no successor.
- **Enter and Space click a focused widget** that senses clicks.
  `input.key_pressed` ignores modifiers and counts key repeats, so
  Ctrl+Enter clicks too, and holding Space should click repeatedly.
- **`consume_key(NONE, k)` also matches with Alt or Shift held**
  (`matches_logically`), so the plain key consumed first takes `Alt+k`.
  This is why Alt+Up failed in the file picker.
- **Nothing scrolls to the focus.** egui's `Response::scroll_to_me` must
  be called by the widget; no widget or container calls it on gaining the
  focus (`sections.rs`, `files.rs`, `palette.rs` and `dialogs.rs` in
  egui-pop call it themselves).
- **No focus ring.** `Widgets::style` gives a focused widget the `active`
  look, the same as pressed. `debug.show_focused_widget` exists as a debug
  tool. There is no record of whether the keyboard or the pointer moved
  the focus, so nothing like `:focus-visible`.
- **Menus.** `containers/menu.rs` in 0.36.2 reads no key; a button opens
  on Enter or Space (a click); `Popup` closes on Escape. No focus moves
  into an open menu. There is no key for a context menu: 0.36.2's `Key`
  has no `Menu`.
- **What 0.37.0 changes** ([release](https://github.com/emilk/egui/releases/tag/0.37.0)):
  arrows inside menus ([#8548](https://github.com/emilk/egui/pull/8548):
  Up and Down in tab order, wrapping; Left to the opener, Right and Enter
  into a submenu; closes [#8327](https://github.com/emilk/egui/issues/8327),
  focus escaping to the buttons behind a menu), `Key::Menu`
  (in 0.37.0's source, which egui-winit maps from the context menu key;
  the notes list new keys under [#8373](https://github.com/emilk/egui/pull/8373)),
  unfocusable widgets out of the widget
  cache (#8653), a `Role::Splitter` on panel resize handles (#8578). Tab
  is the same as in 0.36 and Enter and Space still ignore modifiers (from
  the 0.37.0 source).
- **Downstream reports** (other projects' trackers, not egui's own):
  [parterre #387](https://github.com/aquamoth/parterre/issues/387) wants
  one stop per control and a visible focus, and reads egui as above;
  [ryucad #136](https://github.com/Seiraiyu/ryucad/issues/136): Tab over a
  viewport walks the menu bar and Enter then opens a menu;
  [egui-charts #277](https://github.com/userFRM/egui-charts/issues/277):
  custom widgets draw no ring.

## egui-pop today

From `crates/egui-pop-core` (`tab.rs`, `focus.rs`, `keys.rs`,
`overlay.rs`), `crates/egui-pop-api` and the docs named.

### What exists

- **Key resolution** (ADR 0007): each pass rebuilds the focus chain from
  the `focus::scope`s around the focused widget, or the place last
  clicked; among bindings whose `when` holds, the nearest the focus wins.
  Text inputs keep plain keys, Escape has a fixed order, every resolution
  has a trace. [test] `the_binding_nearest_the_focused_widget_wins`,
  `chords_wait_and_typing_keeps_plain_keys`.
- **Tab scopes** (`tab::scope`, `tab::stop`): reading order, indices,
  groups; modal scopes cycle; F6 and Shift+F6 go between panel scopes and
  back to the stop last focused. [test]
  `tab_puts_indices_first_skips_disabled_and_keeps_read_only`,
  `tab_follows_reading_order_and_groups_and_f6_moves_between_panels`,
  `tab_cycles_inside_a_modal`.
- **The palette** runs every command, and `<panel>.focus` and
  `<panel>.toggle` exist for every panel. [test]
  `panel_commands_show_hide_and_focus`.
- **Focus mode**: hover mode, where the pointer moves the keyboard between
  surfaces. [test]
  `in_hover_mode_the_pointer_moves_the_keyboard_between_surfaces`.

### Tab order

- **[read] Inside a `tab::scope`, only inputs and sliders are stops.** A
  plain button, list or
  tab is not. Inside a `tab::scope`, Tab from a stop goes to the next
  stop, so it jumps over anything that is not one (documented for modals,
  true of panels too). Wrapping shell panels in `tab::scope` as they
  stand would therefore lose their buttons: F6 cannot simply be turned on
  that way.
- **[walk] Application order is drawing order:** the menu bar, the top
  bar's own widgets, **the status bar**, the activity strip, the areas, the
  centre last. The gallery's first page is 72 stops: 57 of the side list,
  and its first eleven before any panel. The status bar comes before every
  panel. A popup's widgets come where the code that opens it runs.
- **[walk] Stops that show nothing and answer no key** (role `Unknown`, no
  name; the first two compared pixel by pixel, focused against not):
  - the caption of an egui-pop window frame
    (`Sense::click_and_drag()`, `frame.rs`);
  - a side or bottom area's resize line (egui's `Panel`,
    `Sense::click_and_drag()`, drawn only when hovered or dragged);
  - a flyout's edge (`Sense::drag()`, `workbench.rs`);
  - an open menu's frame, not read at first: Tab goes from File to it, then
    to the first item.

  Palette edges avoid it (`Sense::CLICK | Sense::DRAG`), and a
  dialog's grips are focusable only on the handle meant for the keyboard.
- **[walk] Dock separators** do answer, where an area is split: focused,
  they move 16 points with **Shift or Ctrl** and an arrow along their axis
  (egui_dock). A plain arrow leaves them. Nothing says so. They are role
  `Unknown` with no name too, and no default layout of the gallery has one.
- **[walk] Each dock tab is a stop.** No close button ever is: the workbench
  sets `show_close_buttons(false)`. **[read]** Enter or Space activates the
  tab. There is no key for the next tab of an area.

### Moving between regions

- **[walk] F6 does nothing in an application**: the plugin is installed
  by the first `tab::scope`, and only the gallery's Inputs page opens one.
- **[walk] `workbench.focus` is logical only.** `Change::Focus` shows and
  activates the panel and sets `Workbench::focused`; nothing requests the
  widget focus. The other writer of `focused` is a pointer press in the
  dock's `ui`. So the panel `focusedPanel` names and the panel holding the
  keyboard differ: after `gallery.notes.focus` nothing is focused, and after
  Tab from a panel to the next, `focusedPanel` has not moved.
- **[read] No panel has a default focus key**; an application binds them.
- **[walk] A terminal keeps F6, as `tab.rs` says.** Reading `begin_pass`
  predicted the opposite. In a `tab::scope`, with another scope beside it,
  F6 from a field goes to the other scope; from a focused terminal the focus
  stays and the program reads `ESC [ 1 7 ~`.

### Where the focus gets lost

- **[walk] A popover gives it back; a dialog and the command palette do
  not.** `an_overlay_closes_on_escape_and_gives_the_focus_back` and
  `escape_closes_only_the_top_overlay_and_focus_goes_back` test overlays
  that are not modal. Escape from a status popover returns to its status
  item. Escape from a confirm dialog opened from a focused button, or from
  the palette opened with F1, leaves nothing focused. Why is not settled;
  the overlay's stack does ask for the focus back (`settle`), and egui takes
  the focus off widgets under a modal.
- **[read] egui's menus are not overlays**, so nothing restores the focus
  when one closes.
- **[walk] After Escape on any widget** egui drops the focus. The focus
  chain keeps the place, so the surface's keys go on working
  ([test] `a_palette_takes_the_keyboard_only_when_clicked_and_escape_gives_it_back`),
  but Tab starts from the first widget of the window, and **[read]** F6 from
  the first panel.
- **[walk] When the focused widget disappears** (a tab closed, a windowed
  section's row scrolled out), the focus goes to nothing and Tab restarts
  the same way. ARIA's advice is to send it to the next logical item.
- **[walk] A button changing the panels loses the focus it holds**, not read
  at first: "Open the demonstration panels" and "Close them" on the
  workbench page, by Enter, leave nothing focused.
- **[walk] A focused widget out of view stays out of view** in a plain
  `ScrollArea`: the gallery's side list from its 23rd row, the settings page
  past "Log level". Sections, the picker, the palette and dialogs' choices
  handle it themselves ([test] `the_sections_page_answers_the_keyboard`
  for the boundaries, since PR #99).
- **[walk] Windowed rows end where they are drawn.** With buttons as a
  windowed section's rows, Tab goes through those drawn, 11 of 500, then
  leaves the section: nothing scrolls, so the others cannot be reached by
  Tab. The gallery's own windowed rows are labels, not stops.

### Where it cannot be seen

- **[read] egui-pop's own widgets paint a ring** when the kit's `focus`
  state gives one (`paint_ring`), and the views, tree, graph, viewport and
  sliders have their `focus` states.
- **[read] egui's widgets do not.** The kit's own loss report says it:
  "egui's widgets draw no focus ring", "egui's text fields draw no focus
  ring".
- **[walk] A focused plain button has the pressed button's pixels,** and
  those differ from rest: the accent's outline, in `pop-dark`, the only theme
  compared. So it can be seen, and is not told from a press.
- **[read] The ring has no idea who moved the focus**, so what a click
  focuses also shows it.

### What eats a key

- **[test] Tab, where palettes float**, with no focused widget and no
  overlay: `tab_hides_and_shows_the_palettes_but_not_a_pinned_one`. It is
  documented and rebindable. **[walk]** `overlayOpen` reads egui-pop's
  overlay stack, and egui's menus are not on it: with a menu open and
  nothing focused, Tab hides the palettes behind it.
- **[read] Key bindings beat focused widgets.** The dispatcher consumes a
  key that resolves before anything is drawn. A global plain key therefore
  beats Space on a focused button or an arrow on a focused slider.
- **[read] A broken chord swallows its last key**
  (`Ctrl+K` then a key that continues nothing). Standard, and bounded by a
  two-second timeout.
- **[walk] Enter and Space click whatever the modifiers:** Ctrl+Enter opens
  a side list's page, and a held Space clicks at every repeat (four presses,
  four clicks).
- **[walk] Enter reaches a dialog it has just opened,** which takes it as an
  answer: on a confirm, the page's command ran and the dialog never showed.
  A choice and a form close the same way; a danger stays open, on Cancel.
- **[test, known] Alt+Up in the picker** was taken by the plain Up
  consumed first; fixed, and the fix's test is in `tests/files.rs`. The
  trap stays for every `consume_key` in the wrong order (`widgets.rs`
  comments on it: "Shift first: the plain key takes it too").
- **[test, known] Tab in the picker's path field** stayed there until
  PR #100: `tab_reaches_the_splitter_and_the_arrows_move_it`,
  `a_typed_path_completes_with_tab_and_backspace_goes_up`.
- **[walk, fixed] A rebound key keeps its `when`,** since this study was
  read: the gallery's `gallery.notes.ask_clear`, rebound, does nothing
  outside the notes' window. Model test in `crates/egui-pop-model/tests/scopes.rs`.
- **[test] Hidden by design:** a terminal takes everything but
  Shift+Escape; an editor takes Tab and gives way on Escape.

### Menus

- **[test] Alt and a letter** opens a menu, inside "…" too:
  `alt_and_a_hidden_menus_key_opens_it_inside_the_more_menu`.
- **[read] The letter is not shown.** It is given to AccessKit, not drawn.
- **[walk] Then there is no way down by arrow.** No widget is focused, so
  arrows do nothing. Tab starts at the top of the window and goes through
  what is drawn before the popup, which is drawn right after its button:
  from Alt+F, three presses reach File's first item (File, the popup's
  frame, the item). Escape closes it and leaves nothing focused.
- **[walk] With a menu opened from a focused button (Enter),** arrows use
  egui's cone and leave the menu (egui's #8327): the second Down from File
  goes to a row of the side list behind it, the next comes back to "Quit".
- **[read] No F10, nothing on a bare Alt, no Shift+F10.**
  `context_menu_with` answers the secondary
  click only, so nothing on a list row, tree node or graph node opens by
  key. `Alt+Space` opens the system menu, and "at a point" exists for the
  application.

### Lists, trees, tables, the grid

- **[test] One stop, arrows inside**, cursor apart from selection:
  `the_keyboard_moves_opens_closes_and_searches` (tree: type-ahead, F2,
  Ctrl+A), `a_list_selects_with_the_pointer_and_the_keyboard`,
  `the_keyboard_moves_by_cells_rows_pages_and_ends` (grid and filmstrip).
- **[read]** A focused view paints its `list-view` outline and scrolls to
  its own cursor. Hand-drawn lists (`selectable_label`) have neither,
  though each row is a Tab stop.

### Dialogs

- **[read] First focus is chosen:** the field, the primary action, or
  Cancel when the action is dangerous (`dialogs.rs`), as ARIA and the
  Windows guidelines ask.
- **[test]** Alt+F7 and the arrows move a dialog
  (`the_keyboard_moves_a_dialog_enter_keeps_the_place_escape_puts_it_back`);
  the corner handle takes arrows
  (`a_resizable_modal_follows_its_grips_and_the_arrows`,
  `the_corner_handle_resizes_with_the_arrows`); the picker's keys
  (`the_keyboard_filters_enters_and_chooses`).
- **[walk]** Tab in a dialog is egui's order inside the modal layer: it
  wraps, in drawing order, and F6 stays in. No dialog uses
  `ScopeKind::Modal`.
- **[walk]** A status popover takes Tab inward. Past its last stop, Tab goes
  on to the status bar's next item behind it, and on through the window,
  while the popover stays open.

### The gallery's side list

**[walk]** `shell.rs` draws one `selectable_label` per story, 57, so 57 Tab
stops before the page, none scrolling into view. Enter opens a page and
leaves the focus on its row. Up and Down go through egui's cone, row by row;
Right leaves for the side area's line beside it, and Left from that line
lands on a row out of view. **[read]** No type-ahead, no scope. Everything the study praises
in `egui-pop-views` is absent from the catalogue that shows it off.

### The graph

**[test]** `the_keyboard_picks_two_ports_and_connects_them`: Enter on two
ports connects, Delete disconnects, arrows go through ports and to the
nearest node, Escape back to the node. The view follows the focus. The
keys are data (`GraphKeys`).

**[read]** Tab goes through the canvas and every node's chevron, ports and
sliders ([docs](node-graph.md#keyboard)). On a network of thirty modules
that is well over a hundred stops before leaving, where ARIA asks for one
stop per composite.

### The viewport

**[test]** `keys_walk_the_ladder_when_focused`. **[read]** Focus comes from
a click, a drag or Tab; focused, the arrows pan, and `+`, `-`, `0`, `1`,
`2` zoom. The before key and the loupe key also answer when hovered.
Overlay handles are pointer gestures.

## Helios today

From `crates/helios-gui` and `docs/gui.md`.

- **[read] No `tab::scope` and no F6.** Helios depends on egui-pop's
  shell for all of it.
- **[read] Keys are scoped by `focusedPanel`** (`keys.rs`): library,
  image, graph, culling. A test checks the default map has no conflict and
  no global letter: `the_default_map_has_no_conflict_and_no_global_letter`.
  The scopes are only as right as `focusedPanel`, see above.
- **[read] A wrong panel after Tab.** From the image, Tab to the filmstrip
  should leave `C`, `E`, `G`, `P` and `M` adding masks, and `0` to `5` not
  rating. The tests get to a panel by clicking
  (`a_click_elsewhere_ends_typing_and_takes_the_focus`) or by command
  (`typing_in_a_field_fires_no_shortcut_and_an_agent_does_not_take_the_focus`
  covers an agent not moving the focus).
- **[read] Palettes float by default** (`helios.masks`, `helios.scopes`,
  `helios.navigator`), so Tab with nothing focused hides them
  (`docs/gui.md` still says "`Tab` moves the focus" in its darktable
  table). After a start with no click, the keyboard's ways in are menus
  (see above), the palette, and commands that move no widget focus.
  `helios.vault.focus` and the like make keys scoped to that panel work,
  but the grid's own arrows need the widget focus.
- **[read] Culling's keys beat the focus on purpose** ("wherever the focus
  is"): digits, `P`, `X`, `U`, Left, Right, Space, Backspace, without
  `!widgetFocus`. So Space cannot press a focused button in culling, and
  Left and Right cannot step a focused slider.
- **[read] Hand-drawn lists:** Modules (a `selectable_label` per module
  and one for its bypass), the stack review, the decisions' options.
  Every row is a stop or two, with no arrows of their own or type-ahead.
  The mask tree and library use egui-pop's views.
- **[read] Parameters** are sliders in a `ScrollArea`: stops, but scrolled
  out of view when reached (see above). [test]
  `a_held_arrow_steps_a_slider_as_one_undo_step`.
- **[read] Masks by key:** `C`, `E`, `G` place a shape at the view's
  centre, `P` begins a path that needs clicks, `Delete` removes a point
  chosen by pointer. Shapes then move by pointer only.
- **[test] Typing wins:**
  `typing_in_a_field_fires_no_shortcut_and_an_agent_does_not_take_the_focus`;
  the palette gives the focus back:
  `the_palette_lets_go_with_escape_and_gives_the_focus_back`.
- **[read] Credo 10** says holding `H` "will show" the keys of the current
  context; it does not yet.

## The systems

### VS Code

- **Regions.** F6 and Shift+F6 run `focusNextPart` and `focusPreviousPart`
  ([source](https://github.com/microsoft/vscode/blob/main/src/vs/workbench/browser/actions/navigationActions.ts)),
  through Editor, Panel, Secondary Side Bar, Status Bar, Activity Bar, Side
  Bar, skipping what is hidden. The
  [accessibility docs](https://code.visualstudio.com/docs/configure/accessibility/accessibility)
  recommend them "for quick navigation across the workbench".
- **Direct keys.** Ctrl+1 focuses the first editor group, with
  Ctrl+K Ctrl+Left and Right for the neighbours
  ([defaults](https://code.visualstudio.com/docs/reference/default-keybindings)).
  Ctrl+0 for the side bar is widely quoted, but neither page read lists
  it: double-check.
- **Tab.** Moves between controls, with an indicator around each. Toolbars
  and tab lists are one stop: "Once a toolbar or a tab list has focus, you
  can use the arrow keys to navigate within them".
- **The trap.** Tab in an editor inserts; Ctrl+M toggles it, and a status
  item says "Tab moves focus". The terminal follows the same mode;
  read-only files never trap.
- **Lists.** Ctrl+Alt+F opens a find in any tree, and Down goes to the
  first match ([docs](https://code.visualstudio.com/docs/getstarted/userinterface)).
- **Keys.** A binding made from a UI action carries its `when`: "If the
  action has a `when` clause, it's automatically included"; the
  troubleshooting command logs what each key dispatched
  ([docs](https://code.visualstudio.com/docs/configure/keybindings)).

### JetBrains IDEs

- **Direct keys over cycling.** Alt+1 to Alt+0 open or focus numbered
  tool windows, Alt+F12 the terminal; **Escape returns to the editor at
  the last caret, F12 jumps back to the last tool window**, Shift+Escape
  hides the active one, Ctrl+Shift+F12 hides them all; Alt+Left and Right
  go through a window's tabs ([docs](https://www.jetbrains.com/help/idea/tool-windows.html)).
- **Sashes.** Ctrl+Alt+Shift and an arrow resizes the active tool window
  ([docs](https://www.jetbrains.com/help/idea/manipulating-the-tool-windows.html)).
- **Speed search.** In any tool window, tree, list or popup, start typing:
  a field appears over it, and the selection moves to the first match
  ([docs](https://www.jetbrains.com/help/idea/speed-search-in-the-tool-windows.html)).

### Blender

- **No focus, but the pointer.** Many shortcuts "depend on which editor
  the mouse cursor is hovering over"
  ([Areas](https://docs.blender.org/manual/en/3.3/interface/window_system/areas.html)),
  and property shortcuts work "while hovering"
  ([common shortcuts](https://docs.blender.org/manual/en/4.5/interface/keymap/introduction.html)).
  egui-pop's hover mode is this model, as an option.
- **Menus are the best here.** Arrows, an underlined letter per item,
  number keys by position, Space to search, Enter, Esc
  ([Menus](https://docs.blender.org/manual/en/latest/interface/controls/buttons/menus.html)).
- **Fields.** Tab and Shift+Tab go to the next and previous field while
  typing a value, Esc cancels
  ([Fields](https://docs.blender.org/manual/en/latest/interface/controls/buttons/fields.html)).
- There is no Tab order across the interface in what was read, and
  F3 searches the menus
  ([keymap](https://docs.blender.org/manual/en/latest/editors/preferences/keymap.html)).

### darktable

- **Tab hides the side panels**
  ([shortcuts](https://docs.darktable.org/usermanual/development/en/preferences-settings/shortcuts/)):
  the predefined `tab` runs `globals/panels/all`; Shift+Ctrl+L, R, T and B
  toggle one panel each
  ([layout](https://docs.darktable.org/usermanual/development/en/overview/user-interface/screen-layout/)).
- **Focus is a module's**, not a widget's: a key can "focus a module", and
  `<focused>` shortcuts drive that module's first, second and later
  sliders, buttons and tabs.
- **Forum:** asked about GTK-style navigation of panes, a reply says
  "dt has not implemented full accessibility access"; Tab cycles text
  fields, a focused slider or dropdown takes arrows, and Enter opens the
  numeric popup
  ([pixls.us](https://discuss.pixls.us/t/general-accessibility-shortcuts/53772)).
  A contributor proposed Up and Down in the module search to go through
  visible modules
  ([pixls.us](https://discuss.pixls.us/t/added-navigation-of-visible-modules-with-arrow-keys/54074));
  whether it shipped is unconfirmed.

### Lightroom Classic

Adobe's pages refused a direct read; this comes from search excerpts of
[keyboard shortcuts](https://helpx.adobe.com/lightroom-classic/help/keyboard-shortcuts.html)
and [workspace basics](https://helpx.adobe.com/lightroom-classic/help/workspace-basics.html).
Double-check.

- **Tab hides the side panels, Shift+Tab all panels;** F5 the module
  picker, F6 the filmstrip, F7 the left panels, F8 the right.
- **Panels by number:** Ctrl+0 to 9 open or close the right panels in
  Library and Develop, Ctrl+Shift+0 to 5 the left, top to bottom.
- These are open and close, not focus: the model is visibility and
  direct commands.

### Photoshop

From [Adobe's help](https://helpx.adobe.com/photoshop/desktop/get-started/learn-the-basics/hide-show-panels.html)
(through a search excerpt) and forums.

- **Tab hides or shows all panels,** Shift+Tab all but Tools and the
  Options bar; Auto-Show brings them back at the window's edge.
- **Forum:** Tab and Shift+Tab cannot be edited in its shortcut editor
  ([Adobe community](https://community.adobe.com/questions-712/how-to-reset-the-tab-and-shift-tab-shortcut-keys-in-photoshop-1161691)).
  egui-pop's is rebindable.

### GIMP

- **Tab hides the docks**, kept across restarts
  ([Windows menu](https://testing.docs.gimp.org/3.2/en/gimp-windows.html));
  Tab "jump to next widget" appears in the
  [quick reference](https://docs.gimp.org/quickreference/gimp-keys-en.pdf)
  too, and inside a dock Tab navigates the dock
  ([2.8 docs](https://docs.gimp.org/2.8/en/gimp-concepts-docks.html)).
- **The same collision as Helios's:** the manual warns that the shortcut
  can fail with the focus in a tool's dialog, and advises putting the focus
  on the image or using the menu
  ([2.10 guide, German](https://docs.gimp.org/2.10/de/gimp-using-getting-unstuck-gui.html)).

### Qt

- **[Tab](https://doc.qt.io/qt-6/focus.html)** follows construction order
  by default, `setTabOrder` changes it; circular, per window; text editors
  take Tab and Qt maps Ctrl+Tab to Tab for them. It advises click-to-focus
  for text entry and against it for most others, toolbar buttons among them.
- **[Focus policy](https://doc.qt.io/qt-6/qt.html)** is per widget:
  `TabFocus`, `ClickFocus`, `StrongFocus`, `WheelFocus`, `NoFocus`. egui
  has only "focusable or not".
- **Item views.** Arrows move and select, Ctrl+arrow moves only, Ctrl+Space
  toggles; `keyboardSearch` is type-ahead with `keyboardInputInterval`; Tab
  may move within by `tabKeyNavigation`
  ([QAbstractItemView](https://doc.qt.io/qt-6/qabstractitemview.html)).
  egui-pop's keys are these.
- **The ring knows.** `State_KeyboardFocusChange` says "if the focus was
  changed with the keyboard", and `PE_FrameFocusRect` is the "Generic focus
  indicator" ([QStyle](https://doc.qt.io/qt-6/qstyle.html)).
- **Menus:** `&File` gives Alt+F and an underline, on Windows often shown
  only while Alt is held ([QMenuBar](https://doc.qt.io/qt-6/qmenubar.html)).
- **Splitters:** [QSplitter](https://doc.qt.io/qt-6/qsplitter.html)'s page
  mentions drag only.

### GTK

- **Window bindings** ([gtkwindow.c](https://gitlab.gnome.org/GNOME/gtk/-/raw/main/gtk/gtkwindow.c)):
  Tab, Ctrl+Tab and their Shift forms and the arrows (with or without Ctrl)
  move the focus; F10 activates the menu bar.
- **Panes** ([gtkpaned.c](https://gitlab.gnome.org/GNOME/gtk/-/raw/main/gtk/gtkpaned.c)):
  F6 and Shift+F6 cycle the children, **F8 and Shift+F8 focus the handle**;
  then arrows, Ctrl+arrows, Page Up, Page Down, Home and End move it,
  Return or Space accepts, Escape cancels. This is the fullest splitter
  here.
- **Context menu:** "Shift+F10 or Menu" throughout
  ([FileChooserWidget](https://docs.gtk.org/gtk4/class.FileChooserWidget.html)),
  and a trigger for them
  ([ShortcutTrigger](https://docs.gtk.org/gtk4/class.ShortcutTrigger.html)).
- **The ring knows:** [`GtkWindow:focus-visible`](https://docs.gtk.org/gtk4/property.Window.focus-visible.html)
  is kept by GTK from input, and
  [`has_visible_focus`](https://docs.gtk.org/gtk4/method.Widget.has_visible_focus.html)
  takes it into account, with a state flag for CSS. Since 4.24 a
  [timeout setting](https://docs.gtk.org/gtk4/property.Settings.gtk-keyboard-focus-visible-timeout.html),
  marked unstable.
- **[GNOME's guidelines](https://developer.gnome.org/hig/reference/keyboard.html)**
  add F10 for the menu, F9 for a utility pane, Alt+arrows for back, forward
  and up; Alt+F7 and Alt+F8 move and resize a window, Alt+F6 switches the
  application's windows. So **Alt+F7 for a dialog** (egui-pop's) is that
  key's meaning, but a window manager may take it.

### Windows guidelines

The [Windows 7 UX guide](https://learn.microsoft.com/en-us/windows/win32/uxguide/inter-keyboard),
old but still published: "Always show the input focus indicator", with an
exception when it appears on any keyboard input; Tab order follows reading
order; a group has one tab stop, arrows move within and cannot leave it;
"Don't disable a control with input focus"; initial focus on the likely
control; "Keyboard navigation shouldn't change control values". Access keys
are underlined and documented on screen; Tab names use Ctrl+Tab.

### WAI-ARIA Authoring Practices

- **[The rule](https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/).**
  Tab moves between components, arrows within a composite: "the tab sequence
  should include only one focusable element of a composite UI component".
  Focus and selection are distinct and must look distinct. Roving tabindex
  or `aria-activedescendant`, the second keeping focus on the container
  (egui-pop's views are this). When the focused element is removed, send
  focus "to the next logical item". Shift+F10 opens a context menu.
- **[Menu and menu bar](https://www.w3.org/WAI/ARIA/apg/patterns/menubar/).**
  Focus goes to the first item on opening; Up and Down inside; Right and
  Left through submenus and to the next menu; Home, End; optional
  type-ahead; Escape returns focus to the opener. Tab leaves and closes.
- **[Tree](https://www.w3.org/WAI/ARIA/apg/patterns/treeview/)** and
  **[listbox](https://www.w3.org/WAI/ARIA/apg/patterns/listbox/).**
  Type-ahead "recommended", above all beyond seven items; Home and End.
  Selection following focus "can severely degrade accessibility" in some
  cases.
- **[Toolbar](https://www.w3.org/WAI/ARIA/apg/patterns/toolbar/).** One tab
  stop, arrows inside, optionally back to the control last focused.
- **[Tabs](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/).** One stop,
  arrows wrap; automatic activation only if the panel shows without latency;
  Delete optional; Shift+F10 for a tab's menu.
- **[Window splitter](https://www.w3.org/WAI/ARIA/apg/patterns/windowsplitter/).**
  Arrows move; Enter collapses or restores; Home, End optional; **F6 cycles
  the panes**, optional.
- **[Modal dialog](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/).**
  Tab wraps; Escape closes; initial focus by content (the least destructive
  action for an irreversible step); return to the opener.
- **[Landmarks](https://www.w3.org/WAI/ARIA/apg/practices/landmark-regions/)**
  give screen readers regions to go between; the page does not mention F6.
- **[WCAG 2.4.7 and 2.4.13](https://www.w3.org/WAI/WCAG22/Understanding/focus-appearance.html).**
  A visible indicator is required; the AAA level asks for an area of at
  least a 2-pixel border, with a 3:1 change.

## Comparison

"–" means not found in what was read, not proof of absence. egui-pop is as
walked where the sections above say so; Helios is as read here, not as run.

| | egui-pop | Helios | VS Code | JetBrains | Blender | darktable | Lightroom | Photoshop | GIMP | Qt | GTK | ARIA |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Regions by cycling | F6, only in a `tab::scope` | no | F6 | – | – | – | – | – | – | – | F6 in panes | F6, optional |
| Regions directly | commands, no default key | commands, no default key | Ctrl+1 | Alt+1..0 | pointer | module focus keys | F5–F8, toggles | Tab, toggles | Tab, toggles | Alt+letter | – | – |
| Back to the work | no | no | – | Escape, then F12 back | pointer | – | – | – | "focus on the image" | – | – | Escape to opener |
| Tab order | drawing, or scope | drawing | – | – | fields only | text fields (forum) | – | – | next widget | construction, editable | focus chain | author's |
| Tab's main job | moves, or hides palettes | moves, or hides palettes | moves | moves | fields | hides panels | hides panels | hides panels | hides docks | moves | moves | moves |
| Composite is one stop | views yes; toolbars, tabs, graph no | hand lists no | toolbars, tabs | – | – | – | – | – | – | – | – | yes |
| Type-ahead | tree, list | via views | find in tree | speed search | menus by Space | – | – | – | – | yes | – | recommended |
| Menus with arrows | egui 0.37 | same | – | – | yes | – | – | – | – | – | – | yes |
| Menu bar key | Alt+letter | same | – | – | – | – | – | – | – | Alt+letter | F10 | – |
| Letter shown | no | no | – | – | yes | – | – | – | – | yes | – | – |
| Context menu key | no (egui 0.37 has the key) | no | – | – | – | – | – | – | – | – | Shift+F10, Menu | Shift+F10 |
| Focus indicator | own widgets only | same | yes | – | – | – | – | – | – | yes | yes | required |
| Knows keyboard from pointer | no | no | – | – | – | – | – | – | – | yes | yes | (browser) |
| Sash by keyboard | sections, dialog, dock (hidden); not side panels | same | – | Ctrl+Alt+Shift+arrow | – | – | – | – | – | – | F8, then arrows | yes |
| Focus back from overlays | popovers, not modals | as read: yes | – | – | – | – | – | – | – | – | – | yes |
| Rebind keeps the clause | yes | yes | yes, from a UI action | – | – | – | – | – | – | – | – | – |

## What egui-pop and Helios should change

Ranked by what it gives against what it costs, as read before the walk.
Pure egui-pop items first. S is an afternoon, M a few days, L longer. Each is
one pull request. Numbers stay as they are, since the walk's tests name them;
[what the walk changes](#what-the-walk-changes) gives the order to follow now.

### egui-pop

1. **Done: a Tab walk in the test kit, and the first walks.** `egui_pop_test::walk`
   and `crates/egui-pop-gallery/tests/keyboard_walk.rs` ([testing](testing.md#keyboard-walks)).
2. **Done: a rebound key keeps its `when`,** in `Keymap::rebind` and
   `rebind_all`, before the walk ran.
3. **One focused panel.** `workbench.focus` hands over the widget focus (the
   widget last focused in the panel, else its first focusable), as its table
   says. When the widget focus moves into a panel by any means, `focused`
   follows it. Helios's `focusedPanel` is then right after Tab. It needs care
   for a panel without a focusable widget (only the logical focus moves), for
   palettes, and for detached windows. Size: M.
4. **A focus you can see.** The focus plugin paints a ring from the `focus.ring`
   token around the focused widget at the end of the pass, unless the widget
   paints its own, and only when the keyboard last moved the focus, as GTK's
   and Qt's flags do. Stays within the widget's clip. Size: M.
5. **F6 through the shell.** Regions: the activity bar, each visible area,
   the centre, floating palettes, an open flyout, the status bar. Built on the
   `focus::scope`s already there, not on `tab::scope`, so that Tab stays as it
   is. A region remembers its last focus. It lands on nothing that is not
   focusable, and does nothing behind a modal. A terminal keeps F6, as it
   does today in a `tab::scope`: keep that test passing. egui does not list focusable widgets
   publicly; a small focusable marker at a region's start, with egui's
   "next" from it, is the first thing to try. Size: M, after 3.
6. **Focus keeps its place.** The focus plugin remembers the last focused
   widget and rectangle. Tab after Escape, or after that widget vanished,
   continues from that place, not from the window's top; a removed row hands
   the focus on to its neighbour. Size: S to M.
7. **Focus scrolls into view.** A widget focused by the keyboard is brought
   into its scroll area. `scroll_to_me` works for the scroll area drawn around
   the caller, so it belongs in `Input`, the slider and the kit's widgets
   first; then an upstream issue for a general answer. Size: S to M.
8. **No stops that show nothing.** The frame's caption and a flyout's edge
   sense without `FOCUSABLE` (`Sense::CLICK | Sense::DRAG`, `Sense::DRAG`); the
   walk checks it stays so. An open menu's frame is one too, from egui: see
   whether 0.37 still has it. Size: S.
9. **Side areas' sashes are real splitters.** egui's resize line is replaced
   by egui-pop's own, as in `sections`: a stop with a ring, role and value,
   arrows, Shift for a larger step, Enter to reset (as sections, GTK, ARIA). Keep
   JetBrains' Ctrl+Alt+Shift+arrows out unless asked. Dock separators get a plain-arrow move if egui_dock takes it,
   else a hint. Size: M.
10. **The gallery's side list as a `ListView`.** One stop, arrows, Home, End,
    type-ahead; Enter opens the page and moves the focus into it. The catalogue
    should show the good pattern. Size: S. Done with the tree view rather than
    a list, its groups foldable; Enter opens the page and the focus stays in
    the list.
11. **egui 0.37.** Menus with arrows (#8548), the `Menu` key, several `Remove
    when` entries in the upstream register. It is its own migration and belongs
    in its own plan, whatever the keyboard says. Size: L.
12. **F10 and visible letters.** F10 opens the first menu; the access letter is
    underlined (while Alt is held, as Windows and Qt do), with menu items given
    letters too. After 11. Size: S.
13. **A key for context menus.** Shift+F10 and `Menu` open the focused widget's
    declared menu at its rectangle; a list, tree or grid opens for its cursor.
    After 11 for `Menu`; Shift+F10 can come first. Size: M.
14. **Modifiers on Enter and Space.** Enter or Space with Ctrl, Alt or Shift, or
    a key repeat, does not click a focused plain button unless bound. Done in
    the dispatcher's pass for widgets that only sense clicks; an upstream issue
    with it, from the reproduction first. Size: S.
15. **`overlayOpen` sees egui's popups.** Tab behind an open menu no longer
    hides the palettes. Size: S.
16. **Arrows stay in the region.** A plain arrow that would take the focus out
    of its `focus::scope` is undone, so that the cone cannot jump panels. Size:
    S to M; confirm with the walk first.
17. **A roving helper, then toolbars and strips.** `tab::roving`: one stop, arrows
    inside, back to the last. Then one PR each: toolbars (`toolbars.md` lists it),
    the activity bar, the status bar. Size: M, then S each.
18. **Tabs by key.** `workbench.tab.next` and `.previous` for the focused area
    (Ctrl+Page Down, Ctrl+Page Up, as VS Code). Close buttons are not shown, so
    they are no stops today; one added later should not be a stop of its own.
    Size: S to M.
19. **The graph as one stop.** A setting, on by default for large graphs: Tab
    enters and leaves the graph, arrows go between nodes, Enter goes into a
    node's parts, Escape back, as ports already do. Size: M.
20. **Enter on a button does not answer the dialog it opens.** New from the
    walk. A dialog ignores the Enter of the pass it opens in, as a key that was
    not meant for it. A confirmation that can be skipped by pressing its
    button's key defeats it, and egui-pop's own promise "Enter accepts" meets
    its own "Enter clicks a focused button". Size: S.
21. **A modal overlay gives the focus back.** New from the walk. Escape from a
    dialog or the palette returns to the widget that held the focus when it
    opened, as a popover already does and ARIA's modal dialog asks. Size: S.
22. **Tab past a popover's end.** New from the walk. It cycles inside or closes
    the popover (ARIA's menu closes on Tab), not walking the window behind it
    with the popover open. Size: S.

### Helios

1. **Say what Tab is, and F6 is the way in.** Keep Tab hiding the palettes where
   nothing has the focus; correct `docs/gui.md`; mention F6 in the tips. *Not
   decided.*
   Size: S, after egui-pop 5.
2. **Keys for panels.** A proposal: `Ctrl+1` the image, `Ctrl+2` the graph,
   `Ctrl+3` the vault, then the Parameters, Modules, Filmstrip, as VS Code and
   JetBrains give direct keys; free in the default map as read. *Not decided.*
   Size: S, after egui-pop 3.
3. **Culling's keys give way to a control that takes them.** Left, Right and
   Space with a slider, a list or a button focused; a context key from egui-pop
   for "the focused widget takes arrows", or `!widgetFocus` with the image
   exempt. Size: S.
4. **The Modules panel as a `ListView`,** with type-ahead; the bypass stays on
   `Ctrl+/`. Then one PR each for the stack review and the decisions. Size: S each.
5. **Hold `H`** for the keys of the context (credo 10). Size: M.

### What to skip, and why

- **Giving Tab back from the palettes in Helios.** Lightroom, Photoshop, GIMP
  and darktable teach that Tab hides panels; it is rebindable, and F6 gives a
  keyboard user the way in. GIMP's own warning shows the collision needs a
  rule: nothing focused, not "always".
- **Blender's pointer as the only focus.** egui-pop's hover mode exists as a
  choice. A keyboard user cannot use it.
- **Access keys on every dialog control** (the Windows rule). Plenty of work,
  and a burden on translations, for dialogs that are short and already
  start in the right place. Menus only.
- **A speed-search field on every list** (JetBrains). Type-ahead is there;
  add the field where a list is long and people ask.
- **Per-cell focus in the grid and per-row in the tree.** Keeping the focus on
  the view, as `aria-activedescendant` does, is correct, scales to tens of
  thousands of cells and already works.
- **Tab indices on shell widgets.** Reading order is right for most of it; the
  status bar's place is fixed by F6, not by renumbering.
- **Ctrl+M for Tab trapping** (VS Code). Escape already leaves an editor and
  Shift+Escape a terminal; Tab moves on after. One rule, no mode.
- **Dragging tabs by keyboard.** `workbench.move` and the palette do it.
- **Moving and reshaping masks by keyboard, for now.** A pointer task in all of
  these editors; no one asks. The commands that place a shape remain.
- **Arrow keys between regions, tmux-style.** F6 and direct keys are enough.

## The walk

Run on 2026-10-30 for egui-pop: 18 checks on the gallery at 1100 by 720 in
`pop-dark`, on egui 0.36.2, and on small harnesses where the gallery has no
such case, all by real key presses through kittest
(`crates/egui-pop-gallery/tests/keyboard_walk.rs`, `crates/egui-pop-test/tests/walk.rs`).
Each confirmed gap is an ignored test whose reason names its item:
`cargo test -p egui-pop-gallery --test keyboard_walk -- --ignored` lists them.
"Right" says whether reading predicted the result. None of it ran in a real
window; item 20 and check 9's pixels are worth seeing once in a nested
compositor.

**Gallery and shell**

| # | Check | Predicted | Result | Right? |
|---|---|---|---|---|
| 1 | Tab until the focus returns; where is the status bar? Any stop unseen or unnamed? | status bar before the panels | Status bar before the panels. 72 stops on the first page, looping to File; 35 out of view (side list); 2 unnamed, the side lines | yes |
| 2 | Caption, side line, flyout's edge are stops | yes, nothing shown | All three; caption and side lines change no pixel. **Also** an open menu's frame | yes, and one more |
| 3 | Dock separators are stops; Ctrl and an arrow moves them | yes | Yes, Ctrl or Shift, along their axis, 16 points. No default layout has one (tested on a split area) | yes |
| 4 | Focus a button, Escape, Tab | the window's first stop | File | yes |
| 5 | A tab's close button, Enter, Tab | the same | **No close button exists** (`show_close_buttons(false)`). A focused tab closed by its command: nothing focused, Tab goes to File | premise wrong, result as said |
| 6 | Inputs page F6, Shift+F6; other pages F6 | works there, nothing elsewhere | As said: Street, Tags, Street; on overview, workbench, menus, terminal, settings, nothing | yes |
| 7 | Up, Down in the side list; Left, Right | leaves for the page | Up and Down go row by row. Right goes to the side line, not the page; Left from there to a row out of view | nearly |
| 8 | Side list's stops | one per story | 57 for 57 | yes |
| 9 | Focused button against pressed; Ctrl+Enter; held Space | same pixels; clicks; repeats | Same pixels, and different from rest, so it is seen; Ctrl+Enter clicks; 4 presses, 4 clicks | yes, the look less bad than said |
| 10 | Long form, Tab past the fold | out of view | Settings page: out of view from "Language" | yes |
| 11 | A windowed row: focus it, scroll it away | (open) | The gallery's rows are not stops; with buttons as rows, focus goes to nothing, Tab restarts at the first stop. **Also** Tab cannot reach rows not drawn | new |

**Menus and overlays**

| # | Check | Predicted | Result | Right? |
|---|---|---|---|---|
| 12 | Alt and a letter, Down, Tab, Escape | Down nothing, Tab from the top, Escape closes | As said; File's first item is 3 Tabs away. Nothing focused afterwards | yes |
| 13 | Menu from its focused button, arrows | the focus can leave | Second Down goes to the side list behind the menu | yes |
| 14 | Tab with a menu open and a floating palette | the palette hides | It hides | yes |
| 15 | Status popover, Tab past its last stop | (open) | Goes on through the window behind, popover still open; Escape returns to the item | new (item 22) |
| 16 | Confirm dialog, danger on and off: first focus, Tab round | as `dialogs.rs` | Action first, Cancel for a danger, Tab wraps, F6 stays. **Also** Enter from the opening button answers a dialog at once (item 20), Escape leaves nothing focused (item 21) | yes, and two bugs |

**Keys**

| # | Check | Predicted | Result | Right? |
|---|---|---|---|---|
| 17 | Rebind a `when`-scoped command, press it elsewhere | fires | Does not fire: fixed since it was read | no longer |
| 18 | F6 with a focused terminal in a scope | the terminal does not receive it | **The terminal receives it and keeps the focus**, as `tab.rs` promises. F6 from a field beside it moves | **no** |

**Still owed: Helios**

It did not run, as this pull request stays in egui-pop. Each check confirms or
removes a finding above; the expected result is what reading predicts. The
answer to check 19's first half is already expected from egui-pop's own
result: `focusedPanel` does not follow Tab.

19. From the image (clicked), Tab until the filmstrip, press `C` (expected: a
    circle mask), press `3` (expected: no rating).
20. `helios.vault.focus` from the palette, then Up, Down, `3` (expected: `3` rates,
    Up and Down nothing until a click).
21. Fresh start, floating palettes, no click: Tab (expected: palettes hide, no
    focus). What is the shortest keyboard route to a slider?
22. Culling: focus a slider, press Left; focus a button, press Space (expected:
    the image changes, the control does not).
23. Modules panel: stops per module, arrows, type-ahead (expected: two, cone, none).
24. Parameters: Tab through a long module (expected: out of view).
25. Graph of thirty modules: stops, entry and exit, and the view following the
    focus.
26. Viewport: focus by Tab; arrows pan; `C`, then Enter and Escape; what reaches a
    placed shape.

### What the walk changes

The order to follow for egui-pop now, by the same measure:

1. **Item 20, Enter answers the dialog it opens.** A bug skipping a
   confirmation, small to fix. New, and first.
2. **Item 21, modal overlays give the focus back.** Small, and every dialog
   and palette use loses the place today. New.
3. **Item 3, one focused panel.** Confirmed; still what Helios's keys depend on.
   First of the larger ones.
4. **Item 7, focus scrolls into view,** up from seventh: it is the first thing
   a Tab user meets, 35 stops out of view on the gallery's first screen.
   It should say what to do about windowed rows, which Tab cannot reach.
5. **Item 10, the side list as a `ListView`.** Up from tenth, for the same
   reason: 57 stops before any page, and it removes the worst of the rows out
   of view.
6. **Item 6, focus keeps its place,** now also after a button changing the
   panels.
7. **Item 8, no stops that show nothing,** with the menu's frame.
8. **Item 5, F6 through the shell.** After 3 as planned. A terminal is not the
   risk it was read to be.
9. **Item 22, a popover's end,** and **item 15.** Small, confirmed.
10. **Item 4, a focus you can see,** down from fourth: a plain button does show
    the focus, the gap is telling it from a press and from the pointer's, and the
    stops that show nothing are item 8's.

Items 9, 11 to 14, 16 to 19 keep their order after these; 9, 11, 14 and 16 are
confirmed by the walk, 12, 13, 17 and 19 were not walked. Item 2 is done.

## Sources

The sources are linked where they are used. The main ones:
[VS Code's accessibility docs](https://code.visualstudio.com/docs/configure/accessibility/accessibility)
and [navigation actions](https://github.com/microsoft/vscode/blob/main/src/vs/workbench/browser/actions/navigationActions.ts),
[JetBrains' tool windows](https://www.jetbrains.com/help/idea/tool-windows.html),
[Blender's menus](https://docs.blender.org/manual/en/latest/interface/controls/buttons/menus.html),
[darktable's shortcuts](https://docs.darktable.org/usermanual/development/en/preferences-settings/shortcuts/),
[Qt's focus](https://doc.qt.io/qt-6/focus.html),
[GTK's panes](https://gitlab.gnome.org/GNOME/gtk/-/raw/main/gtk/gtkpaned.c),
the [ARIA practices on keyboards](https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/)
and egui's [#8548](https://github.com/emilk/egui/pull/8548) and
[0.37.0](https://github.com/emilk/egui/releases/tag/0.37.0).

To double-check: Adobe's pages (read through search excerpts only), VS Code's
Ctrl+0, and anything marked as a forum's.
