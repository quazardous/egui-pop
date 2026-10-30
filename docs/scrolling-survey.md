# Scrolling survey: when a scroll bar shows, and what the wheel does

This survey settles how egui-pop's scroll bars and wheel should behave. It
starts from a page list in the gallery that ran past the window's bottom
with nothing to say so. It reads what egui gives and what egui-pop does with
it, compares the desktops (GNOME and GTK, macOS, Windows 11) and the
applications (VS Code, JetBrains IDEs, Blender, darktable, Lightroom Classic,
Photoshop, Qt), and ends with a recommended default, the tokens and settings
it needs, and a ranked list of changes. egui-pop's main user is a photo
editor, whose panels are long and full of sliders, so that weighs in the
ranking.

Read on 2026-10-30, on egui 0.36.2 (with a look at 0.37.0). Nothing was built
or run for this page: what it says of egui-pop comes from reading the code,
and the first change confirms it in a test. Claims from forums and bug
trackers are marked **(forum)**; the rest come from vendors' documents or
source code.

## Summary

- **Bars should be visible at rest by default, when the content overflows.**
  Thin and quiet, widening under the pointer, not egui's "nothing until
  hovered". Nothing in today's default tells someone who has not moved the
  pointer over a list that it continues.
- **The hidden bar is the industry's default, and its most lasting
  complaint.** GNOME, macOS with a trackpad and Windows 11 all hide bars at
  rest, and all three had to add a way back. Applications that draw their own
  interface moved the other way: Blender went from hidden (2018) to "always
  visible, but thin and dim" (2022); darktable shows its panels' bars by
  default.
- **Follow the system one way only.** A person who turned on "always show" in
  the system gets full bars. A system left at its default of hiding does not
  hide ours, since most people never chose it. A setting in the application
  covers the rest.
- **The evidence is thinner than the opinions.** Nielsen Norman Group's
  guidance is clear ("offer a scrollbar if an area has scrolling content")
  but dates from 2005 and is not an experiment on overlay bars. No controlled
  study of hidden or overlay bars was found. The case rests on guidance,
  vendors' own reversals and years of complaints.
- **Width at rest and hit area are two things.** egui already senses the bar's
  full width while drawing it thin, which is what the complaints about thin
  bars ask for. The complaints that remain are about what lies next to the
  bar: darktable's and Lightroom's panel-hiding controls, right beside it.
- **The gallery's list has a second problem under the first.** As the code
  reads, a shell panel alone in its area is drawn bare, with no scroll area, and
  the gallery's page list adds none: it is cut at the window's bottom and does
  not scroll at all. Visible bars would not have helped it. Most of the
  gallery's pages are in the same case.
- **The other gaps are small and separate:** plain scroll areas take no
  keyboard at all, the wheel's containment exists only in sections, its speed
  has no setting, and the system's reduced motion is not read.

## egui-pop today

### What egui 0.36 gives

From `containers/scroll_area.rs`, `style.rs` and
`input_state/wheel_state.rs`:

| Subject | Behaviour |
|---|---|
| Presets | `ScrollStyle::floating()`, the default: no bar at rest (opacity 0), `floating_width` 2, `bar_width` 10, the thumb at 0.6 under the pointer and 1.0 over the bar. `solid()`: takes room, 6 wide, 4 from the content, opaque. `thin()`: floating with 6 points of room kept for it, opaque at rest, translucent once widened |
| When a bar exists | `ScrollBarVisibility`: `VisibleWhenNeeded` (the default), `AlwaysVisible`, `AlwaysHidden`, per scroll area |
| Hit area | The whole `bar_width`, whatever is drawn: "We always sense interaction with the full width" |
| Thumb | `handle_min_length` 12 |
| Click on the track | Jumps there, the thumb centred on the pointer, and goes on as a drag. No page-by-page option |
| Keyboard | None: `scroll_area.rs` reads no key, and egui's sources handle Home and End only in text. `scroll_to_*` is for code |
| Hints | `ScrollFadeStyle`: a fade at each cut edge, on by default (`strength` 0.5, `size` 20), in the colour of the frames around (`ui.stack().bg_color()`) |
| Wheel | Taken only with the pointer over the area, and cleared from the input only if the area moved. So at its end it goes to the area around: nothing asks to keep it (0.37 is the same for a mouse wheel) |
| Speed | `line_scroll_speed`, 40 points a line on native; `MouseWheelUnit::Point` as given. `ScrollArea::wheel_scroll_multiplier` per area |
| Smoothing | A wheel's lines go in over several frames (90 % in 0.1 s, a constant, "TODO: parameterize"); a trackpad's small deltas do not |
| Sideways | `horizontal_scroll_modifier` Shift, `vertical_scroll_modifier` Alt, `zoom_modifier` Ctrl. `always_scroll_the_only_direction` is off: a horizontal-only area needs Shift |
| Inertia | None in 0.36 beyond what the system sends. 0.37 adds `KineticScrollStyle` (`decay_time` 0.5 s, `rubber_band` on, trackpads and touch) and `MouseWheelSource` |

A theme can set all of `ScrollStyle`. It cannot set the thumb's or track's
colours in it: those are the widget states' (see
[the upstream draft](upstream-drafts/egui-scroll-style-colours.md)), which
`UiPop::scroll_area` works around with a scope. egui has no "while scrolling"
state either; the wheel needs the pointer over the area anyway, so it would
differ from "hover" only for keys and code.

### What egui-pop does with it

- **`ScrollBarKit`** (`crates/egui-pop-core/src/kit/scroll.rs`, the kit's
  `scroll-bar`) defaults to egui's `floating()`, and a test holds every
  built-in theme to that. `scroll-bar.visibility` is `hover`, `always` or
  `scroll` (drawn as `hover`, and reported).
- **`always` is loud.** It draws the bar at full width with the thumb at
  opacity 1, in `text.default` by default, about 13:1 on `pop-dark`'s panel,
  over an opaque `surface.sunken` track. Switching a theme to `always` is
  available, but it is not a default anyone would keep. There is no value for
  "thin and quiet at rest", and `floating_allocated_width` is forced to 0, so
  a bar at rest always lies over the content.
- **Sections** force the bar on when the whole stack overflows, and only
  then. A wheel over a body stops there (`contain_wheel`, see
  [Collapsible sections](sections.md)). This is the only place with
  containment and the only forced bar.
- **Keys.** The data views (`views::nav`, the list, the tree, the library's
  grid), the terminal and the editor read Page Up/Down and Home/End. A plain
  scroll area does not.
- **Profiles** ([Pointer gestures](pointer-gestures.md)) set the three
  modifiers through `InputOptions`, not the speed. `input.wheel_device` sorts
  mice from trackpads for canvases.
- **Reduced motion** sets `animation_time` to 0 and `scroll_animation` to none
  (`theme.rs`). It covers bars' widening and `scroll_to_*`, not the wheel's
  smoothing, which is egui's constant. Nothing in egui-pop reads the system's
  preference: `ThemeSpec::reduced_motion` is up to the application.
- **Colours reach few areas.** Two call sites use `UiPop::scroll_area`
  (sections, the gallery's `scoped` page); about 25 library scroll areas call
  `egui::ScrollArea` directly and keep the button's colours.

### Where the gallery and Helios scroll, and whether anything says so

| Where | Scrolled by | Says so at rest? |
|---|---|---|
| Gallery, page list (side bar) | **Nothing, as the code reads:** a bare panel (below), and `pages` adds no scroll area | No bar and no scroll: cut at the window's bottom |
| Gallery, pages | Read before item 2, which gave them a scroll area: the 12 page files with their own `ScrollArea`; the other 46 have none, or none around the whole page (`table.rs`, for one) | No. Those without are cut if the window is short |
| Gallery, Lists and tables | `DataTable` (egui_table's `SplitScroll`), `ListView` (`views::rows`) | No; for the table, rows cut at its edge |
| Gallery, Sections | Each body, and the stack when too short | Only the stack's bar |
| Gallery, `scoped` page | A demonstration of a kit with `always` | Yes |
| egui-pop's panels and dialogs: file picker, settings, palette, jobs, history, theme editor, tips | Plain `ScrollArea` | No |
| Library grid, thumbnails, viewer, editors | Plain `ScrollArea`, drawn by the view | No |
| Helios: parameters, modules, queue, decisions, duplicates, stacks | Plain `ScrollArea` (six in `helios-gui`) | No |
| Helios: library, mask tree | egui-pop's grid and tree | No |

**Bare panels.** The shell's panels (`PANEL_SIDE_BAR`, `PANEL_MAIN`,
`PANEL_INSPECTOR`) go through `zone_panel`, which declares them `bare`. Alone in
their area, `draw_area` calls `show_panel` directly: no dock, and no scroll area
from anyone (`show_zone`, `show_story` and `show_panel` add none). Beside another
panel, the same content is in a dock tab, and egui_dock wraps a tab's body in a
`ScrollArea` on both axes, as the workbench's `Viewer` does not override
`scroll_bars`. So whether a block overflowing a panel scrolls depends on whether
a second panel shares the area. The same goes for an application's blocks in
those zones.

Helios's three themes extend `pop-*` and set no `scroll-bar` token, so it has
the default everywhere. A list or a panel says nothing until the pointer comes
over it. And in its darktable profile, where the wheel over a slider changes
the value, a parameter panel's bar is the plain way to move it.

## The systems

### GNOME and GTK

- **What it does.** Scrollbars are "scrolling indicators" that fade out and
  grow when approached: "the size of the scrolling indicator increases to
  present a larger target" (Emmanuele Bassi, GTK developer, on
  [GNOME Discourse](https://discourse.gnome.org/t/whats-with-the-trend-of-tiny-and-hidden-scrollbars/18696))
  **(forum)**. GTK's source fades them after 2 seconds
  (`INDICATOR_FADE_OUT_DELAY`,
  [`gtkscrolledwindow.c`](https://github.com/GNOME/gtk/blob/main/gtk/gtkscrolledwindow.c)).
- **The way back.**
  [`gtk-overlay-scrolling`](https://docs.gtk.org/gtk4/property.Settings.gtk-overlay-scrolling.html):
  "If this is set to FALSE, scrolled windows will have permanent scrollbars".
  An application can opt out for one window
  ([`set_overlay_scrolling`](https://docs.gtk.org/gtk4/method.ScrolledWindow.set_overlay_scrolling.html)).
  For users the key is `org.gnome.desktop.interface overlay-scrolling`, hidden
  from GNOME 3.34 ([Linux Uprising](https://www.linuxuprising.com/2019/09/how-to-disable-gnomes-overlay.html))
  **(forum)**, then an accessibility switch, "Always show scrollbars"
  ([libadwaita issue 859](https://gitlab.gnome.org/GNOME/libadwaita/-/issues/859)),
  said to be in Accessibility › Seeing **(forum)**.
- **Complaints.** [GTK issue 1907](https://gitlab.gnome.org/GNOME/gtk/-/issues/1907),
  "Option to always show scrollbars": "some users don't feel comfortable with
  this behaviour". The Discourse thread above: "they should also ALWAYS be
  visible". [GIMP issue 1796](https://gitlab.gnome.org/GNOME/gimp/-/issues/1796):
  "Overlay effect on scrolling bars impedes to slider controls".
  [Launchpad 782754](https://bugs.launchpad.net/overlay-scrollbar/+bug/782754)
  (Ubuntu's overlay bars, 2011), "unusable to people with color blindness/bad
  eye sight" **(all forum)**.
- **Track.** [`gtk-primary-button-warps-slider`](https://docs.gtk.org/gtk4/property.Settings.gtk-primary-button-warps-slider.html)
  chooses between jumping there and moving by a page.
- **Wheel.** A notch moves `pow(page_size, 2/3)`, so a taller view steps
  further; Shift swaps the axes; a wheel event is stopped unless it scrolled in
  a direction the area cannot, or overshoot is under way
  ([source](https://github.com/GNOME/gtk/blob/main/gtk/gtkscrolledwindow.c)).

### macOS

- **Setting.** [Appearance settings](https://support.apple.com/guide/mac-help/change-appearance-settings-on-mac-mchlp1225/mac),
  "Show scroll bars": "Automatically based on mouse or trackpad", "When
  scrolling", "Always". So a mouse gets bars and a trackpad does not by
  default: the only system here that decides by the device.
- **Track.** Same page, "Click in the scroll bar to": "Jump to the next page"
  or "Jump to the spot that's clicked".
- **HIG.** [Scroll views](https://developer.apple.com/design/human-interface-guidelines/scroll-views):
  "Make it apparent when content is scrollable. Because scroll indicators
  aren't always visible [...] displaying partial content at the edge of a view
  indicates that there's more content". "Avoid putting a scroll view inside
  another scroll view with the same orientation". "Support default scrolling
  gestures and keyboard shortcuts". Page-by-page scrolling should keep "a unit
  of overlap". "Use small or mini scroll bars in a panel". An older edition,
  still in mirrors, added "Avoid moving window content when transient scroll
  bars appear"; today's page no longer says it.
- **Scroll edge effects** are not a hint: "Only use a scroll edge effect when
  a scroll view is behind floating interface elements."

### Windows 11

- **Look.** A 2-pixel panning indicator that morphs into the scroll bar when
  the mouse or pen comes over it ([Scroll viewer controls](https://learn.microsoft.com/en-us/windows/apps/design/controls/scroll-controls)).
  "When the scrollbar is visible it is overlaid as 16px on top of the content
  [...] leave 16px of padding on the edge of the viewport". Even `Visible`
  "appears only when the mouse cursor is over it unless the viewport cannot
  display all of the content".
- **Setting.** The API is
  [`UISettings.AutoHideScrollBars`](https://learn.microsoft.com/en-us/uwp/api/windows.ui.viewmanagement.uisettings.autohidescrollbars),
  "intended for handling mouse cursor proximity in custom UI frameworks and
  custom scroll bar implementations", with a change event. Its documentation
  names Ease of Access › Display › "Automatically hide scroll bars in
  Windows". On Windows 11 the switch is Accessibility › Visual effects ›
  "Always show scrollbars"
  ([Windows Central](https://windowscentral.com/how-always-show-scrollbars-windows-11))
  **(press; no Microsoft page found)**.
- **Complaints.** "[Why is the scroll bar in Windows 11 so narrow? In what
  world is this an improvement?](https://learn.microsoft.com/en-us/answers/questions/4018586/why-is-the-scroll-bar-in-windows-11-so-narrow-in-w)"
  and "[Is there a fix yet for the narrow scrollbars](https://learn.microsoft.com/en-au/answers/questions/5732037/is-there-a-fix-yet-for-the-narrow-scrollbars-in-wi)";
  the switch reportedly does nothing for
  [older applications](https://learn.microsoft.com/en-us/answers/questions/678559/how-to-force-scroll-bars-without-focus-to-show-in) **(forum)**.
- **Wheel.** `WHEEL_DELTA` is 120 and each one should give "one increment"
  ([`WM_MOUSEWHEEL`](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-mousewheel));
  the person sets the lines per notch, 3 by default
  ([`SPI_GETWHEELSCROLLLINES`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfow)).
  winit divides by `WHEEL_DELTA` and does not read that setting, so egui-pop
  ignores it today.

### VS Code

From its sources, which are the reference for these defaults:

- **Bars.** `editor.scrollbar.vertical` and `horizontal`: `auto`, `visible`,
  `hidden`; `verticalScrollbarSize` 14, `horizontalScrollbarSize` 12; arrows
  off; `useShadows` on, a shadow along a scrolled edge
  ([`editorOptions.ts`](https://github.com/microsoft/vscode/blob/main/src/vs/editor/common/config/editorOptions.ts)).
  `auto` hides it 500 ms after the pointer leaves, unless dragged
  (`HIDE_TIMEOUT`, [`scrollableElement.ts`](https://github.com/microsoft/vscode/blob/main/src/vs/base/browser/ui/scrollbar/scrollableElement.ts)).
- **Track.** `scrollByPage` off: a click jumps; on, it pages. The same for
  lists (`workbench.list.scrollByPage`,
  [`listService.ts`](https://github.com/microsoft/vscode/blob/main/src/vs/platform/list/browser/listService.ts)).
- **Wheel.** `alwaysConsumeMouseWheel` is on for the editor and for lists, even
  with nothing to scroll ([`listView.ts`](https://github.com/microsoft/vscode/blob/main/src/vs/base/browser/ui/list/listView.ts)).
  `mouseWheelScrollSensitivity` 1, `fastScrollSensitivity` 5 with Alt,
  `smoothScrolling` off, `scrollPredominantAxis` on, so a trackpad's drift does
  not move the other axis. Shift turns the wheel sideways, except on macOS,
  which does it itself.

### JetBrains IDEs

[Scrollbar guidelines](https://plugins.jetbrains.com/docs/intellij/scrollbar.html),
the clearest rule found: "Always show the scrollbar when it is important to
understand which part of the content is currently shown", else "Respect the OS
settings when showing the scrollbar". "Reserve the space for a vertical
scrollbar if there is a column on the right side". On a Mac, people missing a
bar are told to set macOS to "Always", and object to changing the whole system
for one program
([JetBrains community](https://intellij-support.jetbrains.com/hc/en-us/community/posts/203361390/comments/203903644))
**(forum)**.

### Blender

Draws its own interface, so its path is the closest to egui-pop's:

- [2018](https://developer.blender.org/rB51efeb683451d896531baac949de65043edff0b6):
  "Hide & shrink scroll-bars based on cursor position".
- [2022](https://archive.blender.org/lists/bf-blender-cvs/2022-June/174991.html):
  reversed, "always visible, but thin and dim, and widen and become more
  visible as your mouse approaches".
- People on [its design thread](https://projects.blender.org/blender/blender/issues/69391)
  found the thin bars hard to catch **(forum, seen only in search results:
  the page refused the fetch)**.
- Wheel: "[if the region has no zoom level](https://docs.blender.org/manual/en/latest/interface/window_system/regions.html),
  it can also be scrolled by using the Wheel"; middle drag is the main way.
  Ctrl and the wheel cycle a menu's value or a tab.

### darktable

- [Preferences › miscellaneous](https://docs.darktable.org/usermanual/development/en/preferences-settings/miscellaneous/):
  "always show panels' scrollbars", **on by default**, restart needed. So a
  photo editor's panels show their bars. A separate option covers the centre
  view (off).
- "Mouse wheel scrolls modules side panel by default" is off: the wheel
  changes values, Ctrl+Alt and the wheel scroll the panel. With the wheel on
  values, the bar matters more.
- Complaints are about the bar's width and neighbours, not its presence:
  [2026, version 5.6](https://discuss.pixls.us/t/question-about-darktable-scrollbars/59466),
  hard to hit, so people "accidentally hit on the area which hides the
  modules", fixed by a CSS tweak; one user pushes the pointer against the
  screen's edge. [2025](https://discuss.pixls.us/t/darktable-side-panel-mouse-scroll/48449):
  "The scroll bar is very narrow", and what people liked was going to the edge
  and using the wheel **(forum)**.

### Lightroom Classic and Photoshop

- **Lightroom.** No Adobe document found. The same complaint as darktable's,
  [open since 2015](https://community.adobe.com/feature-requests-676/p-lightroom-classic-panel-scroll-bar-close-annoyance-664748):
  "The panel scroll bar and close panel bar arrow section are annoyingly close
  together", thin on 4K, 118 replies, the last in October 2026: "11 years and
  this annoyance still exists". The bar also comes and goes with the panel's
  fit ([thread](https://community.adobe.com/t5/lightroom-classic-discussions/lightroom-scroll-bar-indicator-disappears/m-p/13868329))
  **(forum)**.
- **Photoshop.** Nothing was found on its panels' bars, and this page makes no
  claim on them. The "Zoom with Scroll Wheel" preference decides what the wheel
  does on the canvas, Alt reversing it
  ([Adobe Community](https://community.adobe.com/t5/photoshop-ecosystem-discussions/zoom-in-out/m-p/13703633))
  **(forum)**.

### Qt

- [`QAbstractScrollArea`](https://doc.qt.io/qt-6/qabstractscrollarea.html):
  "The default policy is Qt::ScrollBarAsNeeded", per area.
- [`QStyle`](https://doc.qt.io/qt-6/qstyle.html): `SH_ScrollBar_Transient`
  (transient bars are the style's choice), `SH_ScrollBar_LeftClickAbsolutePosition`
  (jump or page), `PM_ScrollBarExtent` (width). A theme decides what egui-pop's
  kit decides.
- [`QApplication::wheelScrollLines`](https://doc.qt.io/qt-6/qapplication.html):
  3 by default.
- On a Mac, `ScrollBarAlwaysOn` alone reportedly did not help, and a proxy
  style returning false for the hint did
  ([Qt interest list](https://lists.qt-project.org/pipermail/interest/2015-April/016332.html))
  **(forum)**.

### The evidence

- **Nielsen Norman Group.** [Scrolling and Scrollbars](https://www.nngroup.com/articles/scrolling-and-scrollbars/)
  (Nielsen, 2005): "Offer a scrollbar if an area has scrolling content", "Hide
  scrollbars if all content is visible", "use scrollbars that look like
  scrollbars". [The Illusion of Completeness](https://www.nngroup.com/articles/illusion-of-completeness/)
  (Flaherty, 2016): when "the visible content on the screen appears to be
  complete", people stop; show content bleeding off the edge. It does not
  mention fades. [Scrolling and Attention](https://www.nngroup.com/articles/scrolling-and-attention/):
  "Include signifiers (such as cut-off text)", "Beware of false floors".
- **No controlled study of hidden or overlay bars was found.** A search of the
  open web gave opinions and bug reports; the ACM Digital Library was not
  searched.
- **WCAG 2.2.** [Non-text contrast](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html)
  asks 3:1 for what is "required to identify" a component, and exempts what the
  user agent draws. It does not name scroll bars: that it applies to a thumb is
  this survey's reading. [Target size (minimum)](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html)
  asks 24 by 24 CSS pixels with exceptions, and gives a browser's scrollbars as
  the exception for what the user agent draws. Bars egui-pop draws are not the
  user agent's, and these criteria are written for the web: a target to aim at,
  not a legal bar on a desktop application.
- **MDN.** [`scrollbar-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/scrollbar-width):
  `thin` or `none` "can make content hard or impossible to scroll"; "Avoid
  using `none`". [`scrollbar-gutter`](https://developer.mozilla.org/en-US/docs/Web/CSS/scrollbar-gutter):
  `stable` keeps the room so that the bar's arrival does not shift the layout.

### Hints without a bar

- **Partly visible rows.** Recommended by Apple's HIG and NN/g above, and by
  [Jared Spool's "cut-off look"](https://archive.uie.com/brainsparks/2006/08/02/utilizing-the-cut-off-look-to-encourage-users-to-scroll).
  But a list of same-height rows ends on a whole row by chance, and a sized
  panel may too: it cannot be a guarantee. A list that wanted one would have
  to pick its height for it, which layouts do not allow.
- **Fades and shadows.** VS Code has `useShadows`; egui's fades are on by
  default. No usability evidence was found for or against them. They say that
  there is more without saying how much, and they do not help find the end. A
  complement, not a substitute.

### The wheel

- **Chaining or containment.** The web chains by default: "once the dialog
  box's scroll boundary is reached, the underlying page will then start to
  scroll". [`overscroll-behavior`](https://developer.mozilla.org/en-US/docs/Web/CSS/overscroll-behavior)
  `contain` stops it. VS Code contains by default. GTK contains unless a
  direction cannot scroll. egui chains and offers no option, 0.37 included.
  The harm is greatest in a nested list: a notch too many moves the whole
  panel under the pointer, and the next notch lands on another widget.
- **Smooth or stepped.** VS Code: stepped by default, smooth in a setting.
  egui: smoothed and constant. Scrolling a list of sliders with smoothing means
  the row under the pointer changes for 0.1 s after the notch; Helios's
  guarded wheel keeps that harmless.
- **Sensitivity.** Windows and Qt: lines per notch, 3. VS Code: a multiplier.
  GTK: by the view's height. egui-pop: fixed at 40 points a line.
- **Shift for sideways.** Common ground (GTK, VS Code, Windows applications;
  macOS does it in the system). egui-pop's profiles keep it and may change it.
- **Trackpad inertia.** The system's on macOS, as egui's sources say of its
  momentum phase; 0.37 adds kinetics and a rubber band in egui. Not checked on
  Linux or Windows.
- **Reduced motion.** [`prefers-reduced-motion`](https://developer.mozilla.org/en-US/docs/Web/CSS/@media/prefers-reduced-motion)
  maps to a setting on every system. The portal has a standard key
  ([`org.freedesktop.appearance reduced-motion`](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html)),
  and Windows `SPI_GETCLIENTAREAANIMATION`.
  [WCAG 2.3.3](https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html)
  (AAA) treats the movement that brings content into view as essential to
  scrolling; only what is added to it must be possible to turn off.
- **Keyboard.** [MDN on `overflow`](https://developer.mozilla.org/en-US/docs/Web/CSS/overflow):
  "In some browsers, scrolling content areas are not keyboard-focusable, so they
  cannot be scrolled by a keyboard-only user". axe flags it as
  [WCAG 2.1.1](https://dequeuniversity.com/rules/axe/4.11/scrollable-region-focusable).
  Chrome and Firefox now make scrollers focusable themselves (version 127 or
  130 for Chrome: the [sources](https://github.com/adobe/react-spectrum/issues/10395)
  [disagree](https://docs.rs/crate/auditmysite/latest/source/src/wcag/rules/scrollable_region.rs)).

## Compared

| | GNOME / GTK | macOS | Windows 11 | VS Code | JetBrains | Blender | darktable | Qt | egui-pop today | Recommended |
|---|---|---|---|---|---|---|---|---|---|---|
| At rest | hidden | hidden on a trackpad, shown with a mouse | 2 px or hidden | hidden | follows the OS; editor shown | thin, dim (since 2022) | shown | by the style | **hidden** | **thin, quiet** |
| Setting | system switch | system, 3 values | system switch | per view | OS | none found | preference | per area, style | theme only | application + system's "always" |
| Under the pointer | grows | as the setting | morphs to a bar | shows | not found | widens | not found | by the style | fades in thin, widens over the bar | widens over the bar |
| Takes room | no | no (transient) | no: "leave 16px" | no | "reserve" by a column | not found | not found | by the style | no | a little, kept for it |
| Width | grows on approach | small, mini in panels | 2 to 6 px | 14 / 12 | not given | not given | narrow, a CSS tweak | `PM_ScrollBarExtent` | 2 to 10 | 4 to 10 |
| Click on the track | a setting | a setting | not checked | jump, or `scrollByPage` | not found | not found | not found | style hint | jump | **jump** |
| Wheel at the end | stops (see above) | not checked | not checked | **stops** | not found | not found | not found | not checked | chains; stops in sections | **stops in nested views and popups** |
| Wheel speed | by view height | system | system: 3 lines | multiplier | not found | not found | not found | 3 lines | fixed | a setting |
| Keyboard on a plain area | not checked | HIG: expected | documented input | not checked | not found | not found | not found | not checked | **none** | Page Up/Down, Home/End |

"Not found" means the search did not give it, not that it lacks it.

## The recommended default

**Should bars be visible at rest by default? Yes.** A bar is drawn at rest
whenever the content overflows, in every built-in theme:

- 4 points wide at rest, 10 under the pointer, as it is today: the hit area
  stays 10 at all times, and the pointer against the window's edge reaches it.
- Dim, but 3:1 against the panel. In the thumb's default colour, `text.default`:

  | Theme and surface | 0.4 | 0.45 | 0.5 | 0.55 | 0.6 | 0.65 |
  |---|---|---|---|---|---|---|
  | `pop-dark`, panel `#1b1916` | 3.12 | **3.62** | 4.17 | | 5.44 | |
  | `pop-light`, panel `#f2f0ec` | | | 2.73 | 3.06 | **3.46** | 3.98 |

  So 0.45 and 0.6, each theme's own, held by a test computing the ratio through
  the same blending as the painter (these figures use a plain alpha blend, so
  they are approximate). For comparison, the thumb in
  [the kit's example](kit.md#scroll-bars), `#4b4b4b`, is 2.0:1 on `pop-dark`'s
  panel.
- No track at rest. Over the bar, the track as egui draws it today.
- 6 points kept for it (egui's `floating_allocated_width`, as its `thin()`),
  so that text and a slider's end do not lie under the thumb, and widening
  covers the content by 4 points without moving it. Content narrows only when
  it starts to overflow; for a list, that is once.
- A thumb at least 24 points long (egui's 12 is small on a long list).
- No bar when the content fits, as egui does (`VisibleWhenNeeded`, and NN/g).

**Is it a setting? Yes,** `appearance.scroll_bars`:

- `auto` (default): what the theme says, raised to `always` when the system
  asks for it.
- `always`: full width, opaque thumb, a track, for those who want the old bar
  or a better target.
- `hover`: egui's own, today's behaviour, for those who prefer a clean panel.

**Does it follow the system? One way only.** The system's "always show"
(GNOME's switch, Windows' switch, macOS's "Always") raises `auto` to `always`:
someone asked for that, often for a reason of sight. The system's default of
hiding does not lower it: GNOME's, Windows' and a Mac trackpad's hide by default
and nobody chose that, so following them gives this very bug on every fresh
install. JetBrains respects the OS in both directions and keeps the bar in the
editor because "it is important to understand which part of the content is
currently shown"; for a photo editor's panels and grids, that is always.

A kit decides for itself: a theme with `visibility: hover` keeps its look and
its contract. `pop-high-contrast` takes `always`, 14 wide, its thumb at opacity
1. Pointer profiles do not touch the bars: a profile is gestures, not a look.

**The wheel:** contained in nested views, popups and dialogs, chained in plain
panels (next section's item 7), its speed a setting. **The track:** a click
jumps, as today, as on GTK's and VS Code's defaults. **Keys:** Page Up/Down and
Home/End on every plain scroll area.

## Tokens and settings needed

| Name | Meaning | Default |
|---|---|---|
| `scroll-bar.visibility` | adds `rest`: thin and quiet at rest, wide over the bar | `rest` in the built-in themes (`hover` today) |
| `scroll-bar.width` | as today: the bar at its widest, and its hit area | 10 |
| `scroll-bar.width-rest` | the bar's width at rest, under `rest` (egui's `floating_width`) | 4 |
| `scroll-bar.opacity-rest` | the thumb's opacity at rest, in every scroll area (egui's `dormant_handle_opacity`) | 0.45 dark, 0.6 light |
| `scroll-bar.room` | room kept for a floating bar (`floating_allocated_width`) | 6 |
| `scroll-bar.min-length` | the thumb's least length (`handle_min_length`) | 24 |
| `scroll-bar.fade-strength`, `fade-size` | the cut edge's fade, late and optional (`ScrollFadeStyle`) | egui's |
| `appearance.scroll_bars` (setting) | `auto`, `always`, `hover` | `auto` |
| `input.wheel_speed` (setting) | a multiplier on `line_scroll_speed`; a trackpad's points are left alone | 1 |

## What egui-pop should take, ranked

Sizes: S is under a day, M a few days. One pull request each.

1. **The gallery's page list scrolls, and says so.** First a test at a short
   window, to confirm what this page only read: the list is cut and does not
   scroll. `pages` then gets a scroll area of its own, filling the side bar, its
   bar forced on as `Sections` does, with a snapshot at a short size. Item 4
   makes the forcing needless and removes it; the scroll area and the snapshot
   stay. Size: S.
2. **Done: nothing in the gallery is cut without a way to it, and panels say what
   they give.** The gallery's `page` and `playground` wrap a page in a scroll area
   unless it stays inside the zone itself, by a list in the gallery's shell
   (`shell::FILLS_THE_ZONE`) rather than a flag on the story, so the library does
   not change. Read as a whole, few pages fill the zone: most canvases, stacks and
   editors stand among text and controls, at sizes of their own, and scroll with
   the page, keeping the wheel over themselves. A test walks every page at a short
   window. [Workbench](workbench.md#panels-in-areas) says that a panel alone in its
   area has no scroll area and one in tabs has. Whether the shell's panels should
   always scroll is a question for that page, not settled here: a viewport in
   `MAIN` must not.
3. **Done: the kit learns "thin and quiet at rest".** `visibility: rest` and the
   tokens above in `ScrollBarKit`, no theme using them yet, so the pictures that
   change are those of the forced bars, which take the quiet look: the sections'
   stack, and the gallery's list and pages. `ScrollBarKit::seen_at_rest` is that
   forcing, and leaves an `always` bar as it is.
4. **Done: visible at rest in the built-in themes.** The themes set it, the test
   that held them to `floating()` is replaced by a 3:1 test per theme on every
   surface, through egui's blending, which raises `opacity-rest` where a theme's
   text is close to its surfaces. The high contrast themes take `always`, `beside`
   the content so that it covers nothing. The gallery's list loses its forcing.
   The snapshots are regenerated.
5. **Done: the `appearance.scroll_bars` setting.** `auto`, `always`, `hover`, on
   the settings page, written into the theme's spec and taking over
   `scroll-bar.visibility` and `placement` when the theme resolves, so that it
   reaches the style with the theme and keeps across a change of theme. `always`
   goes `beside` the content, as a full bar over it would hide it, and brings a track
   where a theme has none; `hover` goes `over` it, as egui always shows a bar
   beside the content. The rule that a system only raises `auto` is in, through
   `AppBuilder::system_always_shows_scroll_bars`; item 10 is what reads the system
   and gives it.
6. **Keys on plain scroll areas.** Page Up/Down (the view's height less a row,
   Apple's "unit of overlap"), Home/End, the arrows by a line, on the area under
   the pointer or holding the focus, unless a widget inside takes the key; bound
   through the keymap's scope, so a binding wins. In `UiPop::scroll_area`, and
   item 9 brings the others. The focus question (is a scroll area a Tab stop?)
   is the hard part: a stop only when it has nothing focusable inside, as
   browsers do. Size: M.
7. **Containment as an option, used by what nests.** `contain_wheel` moves out
   of sections into `UiPop::scroll_area` (`.contain_wheel(true)`), used by the
   views (`views::rows`, the grid, the table), popups and dialogs, the terminal
   and the editor. A plain panel with no scroll around it has nothing to chain
   to. Plain bodies keep chaining: a sizeable text in a panel should not trap
   the wheel. Size: S.
8. **Wheel speed.** `input.wheel_speed`, in Settings › Input, multiplying
   `line_scroll_speed`, and read from `SPI_GETWHEELSCROLLLINES` as its default on
   Windows (3 lines being today's 40 points). Canvases read it too, through
   `wheel_inputs`. Size: S.
9. **The library's scroll areas go through `UiPop::scroll_area`.** About 25 in
   `egui-pop-api`, `-views`, `-viewport`, `-editors`: mechanical, and it gives a
   kit's thumb colours and items 6 and 7 a single way in. A view that draws in
   `show_viewport` needs that form added. Size: M.
10. **The system's "always show".** In `egui-pop-platform`, alongside
   `text_scale`, with the same sources: on GNOME the portal's
   `org.gnome.desktop.interface` `overlay-scrolling` (and its change signal), on
   Windows `UISettings.AutoHideScrollBars`, or the registry as `text_scale`
   does, on macOS `NSScroller`'s preferred style. Feeds `auto`, through
   `AppBuilder::system_always_shows_scroll_bars`. Size: M.
11. **The system's reduced motion.** The portal's `reduced-motion`,
    `SPI_GETCLIENTAREAANIMATION`, macOS's accessibility flag, into
    `ThemeSpec::reduced_motion` until the user sets it. Wider than scrolling, and
    worth doing for itself. Size: S.
12. **Only one direction, any wheel.** `always_scroll_the_only_direction` on in
    the theme's style, so that a horizontal strip takes a plain wheel; egui's
    default wants Shift, and nobody finds that on a strip with no bar. Sections
    already read this flag. Size: S.
13. **Upstream, as drafts for a person to post.** A `ScrollArea` option to keep
    the wheel at its end (`docs/upstream.md` has "not filed yet"), and a way to
    turn off the wheel's smoothing, for reduced motion. Size: S.
14. **At the bump to egui 0.37.** `ScrollStyle::kinetic`: `rubber_band` and
    `decay_time` off under reduced motion, and the bounce decided for panels of
    sliders (a bounce there has no use); `contain_wheel` retested against
    `has_scrolling_ancestor`. Part of the bump's pull request. Size: S.
15. **Fade tokens.** Only if a theme asks. Also check that a fade's colour is right
    on a list drawn over another fill than the frames', as egui takes the frames'.
    Size: S.

### What to skip, and why

- **`visibility: scroll` made real.** The wheel needs the pointer over the area,
  so `hover` already shows then; with `rest` as the default, it is a fourth look
  nobody asked for. Keep it reported as `hover`.
- **Page-by-page on a track click, Shift+click.** Jump is GTK's way, VS Code's
  default and egui's own; paging needs an egui change for a habit few have. If
  asked, it is one setting, as `scrollByPage`.
- **Arrows on the bar.** VS Code has them off by default. They take room for a
  use the wheel and the keys cover.
- **Hiding on a timer** (VS Code's 500 ms, GTK's 2 s). A cost in repaints, for a
  look the `hover` mode already gives.
- **A choice between smooth and stepped.** egui's constant cannot be set from
  outside, and a canvas already has the raw wheel. Item 13 asks for what is needed.
- **Fast scrolling with Alt** (VS Code's 5 times). Alt is the vertical modifier
  and steps a slider in the default profile. A person who wants speed has item 8.
- **Device-based bars** (macOS's "automatically"). winit does not tell a mouse
  from a trackpad, which is why `input.wheel_device` exists. It would hide bars on
  laptops, the very bug, for a small gain.
- **Copying each app's bar in its pointer profile.** A profile is gestures; bars
  are the kit's. Lightroom's habits do not need Lightroom's bar.
- **A guaranteed partly visible row.** It would mean sizing lists by rows; layouts
  decide their height. The bar does the job.
- **A minimap or a scrub bar with dates.** Each is a widget of its own, for a
  view that asks for it.

### What belongs to Helios

- **Nothing to change for the default.** Its themes extend `pop-*` and get it
  with the update; its snapshots are regenerated then.
- **Blocks in shell zones.** Anything it puts in `SIDE_BAR`, `MAIN` or `INSPECTOR`
  has no scroll area when alone there: to check against item 2.
- **Its six plain `ScrollArea`** (`params`, `modules`, `queue`, `decisions`,
  `duplicates`, `stack_review`) move to `ui.scroll_area` once item 9 has set the
  way, so that keys and colours reach them.
- **The parameter panel's edge.** The complaints of darktable and Lightroom are
  about this very place: a thin bar next to something that hides the panel. Helios
  should keep the panel's grip and any hide control away from the bar: its
  `docs/dpi.md` already notes that a bar at the window's edge loses 5 points to the
  grip. The pointer against the edge should land on the bar, as darktable's users
  like. egui-pop gives the room token; the layout is Helios's.
- **The wheel over a slider** keeps its rule (decided 2026-10-12): the guarded
  wheel scrolls the panel, the darktable profile changes the value. Under that
  profile the bar is the plain way to scroll, which argues for it the more.
- **The library grid** at 100,000 images is where the position matters most, and
  where a minimum-length thumb stops being proportional. A larger grid control
  (a date beside the thumb) would be a decision of its own, not decided.
- **Settings.** Its Settings › Input should list `input.wheel_speed`, its
  Appearance `appearance.scroll_bars`, if it does not take egui-pop's pages as they
  are.

## Sources

The sources are linked where they are used above. The main ones:
[Apple's HIG](https://developer.apple.com/design/human-interface-guidelines/scroll-views)
and [Mac settings](https://support.apple.com/guide/mac-help/change-appearance-settings-on-mac-mchlp1225/mac),
Microsoft's [scroll viewer](https://learn.microsoft.com/en-us/windows/apps/design/controls/scroll-controls)
and [`AutoHideScrollBars`](https://learn.microsoft.com/en-us/uwp/api/windows.ui.viewmanagement.uisettings.autohidescrollbars),
[GTK's settings](https://docs.gtk.org/gtk4/property.Settings.gtk-overlay-scrolling.html)
and [source](https://github.com/GNOME/gtk/blob/main/gtk/gtkscrolledwindow.c),
[VS Code's source](https://github.com/microsoft/vscode),
[JetBrains' guidelines](https://plugins.jetbrains.com/docs/intellij/scrollbar.html),
[Blender's 2022 commit](https://archive.blender.org/lists/bf-blender-cvs/2022-June/174991.html),
[darktable's manual](https://docs.darktable.org/usermanual/development/en/preferences-settings/miscellaneous/),
[Qt's `QStyle`](https://doc.qt.io/qt-6/qstyle.html),
[NN/g](https://www.nngroup.com/articles/scrolling-and-scrollbars/),
[WCAG 2.2](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html),
and egui 0.36.2's and 0.37.0's own sources.
