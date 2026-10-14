# Micro-indicators: survey, places of use and the `indicator` module

Status: **design note, proposed** (2026-10-14). Nothing here is
implemented yet. Decisions marked "not decided" wait for the owner.

Micro-indicators are the small marks that say a state in very little
space, attached to something else: a dot or LED, a count badge, an
activity pip, an icon whose glyph follows a state, a flag on a row or a
tile, a colour swatch. Examples: the active colour in a menu entry, an
activity LED in a dialog, a modified dot on a tab, an error count on a
status bar item. Some are passive; some are buttons.

This note has four parts: what exists (section 1), where the marks
serve in egui-pop and in an application such as Helios (section 2),
the proposed `indicator` module (section 3), and what it depends on
(section 4). Open questions close it (section 5).

## 1. Survey

Verdicts follow egui-pop's usual vocabulary: **adopt** (depend on it as
it is), **wrap** (depend on it behind an egui-pop type), **own** (write
it in egui-pop, maybe after the idea).

### 1.1 egui itself (0.36)

| Piece | What it gives | What it misses | Verdict |
|---|---|---|---|
| `Painter::circle_filled`, `rect`, `text` | Every shape a mark needs | Everything else: tokens, a11y, placement | **Adopt** as the drawing layer |
| `Spinner` | An arc turning on `input.time`, size, colour, role `ProgressIndicator` | No label, no delay, no reduced motion, repaints every frame (already noted in [widgets.md](../widgets.md)) | Already **owned**: `egui_pop_core::progress::paint_spinner` and `motion::Busy` |
| `color_picker::show_color`, `Ui::color_edit_button_*` | A swatch rectangle with a checkerboard under alpha; a swatch button opening egui's picker | No token outline, no name for AccessKit, the button opens egui's picker only | **Wrap** `show_color` for the alpha checkerboard, **own** the swatch widget |
| `Atom` / `AtomLayout` (`Atom::custom`) | A reserved rectangle inside a button's layout, painted after layout; egui-pop's menus already put their tick and icon there | Nothing indicator-specific | **Adopt** as the inline anchor inside buttons and menu entries |
| `Response::widget_info`, `Context::accesskit_node_builder` | Role, name, value, `set_busy` on a node | Nothing for "decoration of another node" | **Adopt**: an indicator's state goes into its host's description, or into its own node when interactive |
| `egui::Image` with a tint | Dynamic icons from images | Nothing more | **Adopt** through `icon::Source` |

### 1.2 Crates around egui

Searched on crates.io and GitHub on 2026-10-14 ("egui badge",
"notification", "indicator", "led", "dot", "status", "spinner",
"swatch"). No crate is dedicated to LEDs or dots. The crates found are
either toasts, icon fonts, or design-system kits that include a badge.

| Crate | Version, egui | Licence | What it has for marks | Verdict |
|---|---|---|---|---|
| egui-phosphor | 0.14, egui 0.36 | MIT OR Apache-2.0 | The Phosphor icon font, already egui-pop's icon source | **Adopt** (already a dependency) as the glyph source of dynamic icons |
| egui_material_icons | 0.8, egui 0.36 | MIT (font Apache-2.0) | Material Symbols font | Not needed; an application can register it through the icon registry (section 4) |
| egui_dock | 0.21, egui 0.36 | MIT | `TabViewer::title`, `is_closeable`, `on_close` (can veto), `tab_style_override` | **No modified mark on tabs**; the dot that replaces the close button cannot be done without patching. **Own** the mark; an upstream issue for a tab "decoration" slot is worth filing (`docs/upstream.md`) |
| egui-notify, egui-toast | 0.23 and 0.22, egui 0.36 | MIT | Toasts with a spinner or a bar | Out of scope (egui-pop has its own notifications) |
| egui-elegance | 0.16, egui 0.36 | MIT OR Apache-2.0 | `Indicator` (a status light: On, Connecting, Off), `StatusPill` (lights with labels), `Badge` with tones, `Spinner`, `ProgressRing` | Fixed look, not token-driven, its own colour scheme. **Ideas only**: the light's three states and the pill of lights |
| egui-elements | 1.3, egui 0.36 | MIT | A corner `Badge` (a circle with optional text) anchored to one of four corners of a button | **Ideas only**: corner anchoring |
| egui-material3 | 0.0.9, egui 0.33 | MIT OR Apache-2.0 | Material 3 `Badge`: tones, sizes 16/20/24, positions | Behind on egui; **ideas only** |
| kylig-ui, kylig-fx | 0.3, egui 0.36 | MIT | `Badge`, `Severity`, `ProgressRing`; `prefers_reduced_motion(ctx)` as an application-set flag | **Ideas only**; egui-pop already has its reduced motion (`motion.duration` = 0) |
| shadcn-egui | 0.4, egui 0.34 | MIT | A shadcn `Badge` port | Ignore |

**Verdict: own.** No crate combines token theming, AccessKit names,
an interactive mode with button rules and test hooks, and each badge is
a few dozen lines on the painter. Adopting one would bring a second
theme system. egui-phosphor stays the glyph source.

### 1.3 What egui-pop already has

egui-pop has many marks already, each written for one component. None
is shared, and four `Tone` enums coexist.

| Where | What | Notes |
|---|---|---|
| `egui-pop-api::strips` | `Indicator` and `IndicatorState`: status bar items reduced from the host's journal (text, `Tone` Quiet/Active/Success/Warning/Error, `count`, `progress` line, `busy` spinner, `detail` tooltip, a command on click) | The count is drawn as `"text (n)"`, not a badge. The name `Indicator` is taken (section 5, question 1) |
| `egui-pop-api::jobs` | The status bar's jobs item: a ring (determinate or turning), "n running", "n failed" in the danger colour, a popover | A pip and a count, hand-drawn |
| `egui-pop-api::menus` | The menu entry's tick, icon, or a spinner in the icon's place while a command runs (`busy::busy_shown`), AccessKit `set_busy` | The first inline pip; it lives in an `Atom::custom` slot |
| `egui-pop-core::icon` | `Icon` by role (`Dot`, `Diamond`, `Warning`, `Visible`/`Hidden`, `Lock`/`Unlock`...), `Size`, `Tone` (Default, Muted, Strong, Accent, OnAccent, Danger, Warning, Success, Info), `.label()`, `.sense_click()` | The base of the dynamic icon kind |
| `egui-pop-core::progress` | `Tone` (Accent, Info, Success, Warning, Danger, Muted), `ProgressTokens` (`progress.ring`), `paint_spinner`, rings | The base of the pip kind |
| `egui-pop-core::motion` | The motion clock, `phase` static under reduced motion (`motion.duration` = 0), `animate` rate-limited, `Busy` (300 ms delay, 500 ms minimum) | The base of the pulse |
| `egui-pop-views::library` | `Flag` (pick, reject), `ColorLabel` (five labels, **each a shape of its own**: circle, triangle, square, diamond, hexagon), `RatingStars`, `ColorLabels`, `FlagToggle` (interactive, named), `Badge` (text + its own `Tone` Neutral/Info/Success/Warning/Danger) drawn as pills in tile corners, `library.*` tokens | The most complete marks; flags and label shapes are the model for "never colour alone" |
| `egui-pop-views::tree` | Row icon (`TreeView::icon`, a `Source`), row widgets at the right end (`row_widgets`), a failed row in the danger colour | No mark slot of its own: an application puts its marks in `row_widgets` |
| `egui-pop-viewport::graph` | Ports as circles (`PORT_R` 5.0, hit square `PORT_HIT` 16.0), coloured by type token, a hover ring | Literal sizes; a 16 px hit square is under the 24 px target used below |
| `egui-pop-editors::markers` | Gutter markers: diagnostics, breakpoints, diff marks | Gutter-specific; out of scope, but they could reuse the dot and the tones |
| `egui-pop-api::theme_editor`, gallery `pages::swatch`/`chip` | Two private swatch functions (outline from `border.strong` or `border.default`, no checkerboard, `Sense::hover`) | Duplicates; the swatch kind replaces both |
| `docs/widgets.md` | The component survey already lists "Badge and counter: **own** (small), never colour only" | This note details it |

**Verdict:** own the module, built from what is here (motion clock,
spinner, icons, library shapes, tokens), and move the existing marks
onto it.

### 1.4 Prior art

Behaviour is described from public documentation; no code is taken.

- **VS Code.**
  - [Status bar items](https://code.visualstudio.com/api/references/vscode-api#StatusBarItem):
    text with `$(icon)` codicons (`$(sync~spin)` turns), a tooltip, a
    command, `accessibilityInformation` (label and role). The background
    is a closed set: only `statusBarItem.errorBackground` and
    `statusBarItem.warningBackground`.
  - [View badges](https://code.visualstudio.com/api/references/vscode-api#ViewBadge):
    `{ value, tooltip }` on a view and its activity bar icon, coloured by
    `activityBarBadge.background` and `.foreground`.
  - Editor tabs: the **modified dot takes the close button's place** and
    turns back into the close button on hover;
    `workbench.editor.highlightModifiedTabs` adds a top border.
  - [File decorations](https://code.visualstudio.com/api/references/vscode-api#FileDecoration):
    a badge of at most two characters, a theme colour, a tooltip, and
    `propagate` to parent folders (a count rolls up the tree).
  - The Problems item: an icon and a count per severity; a click opens
    the panel.
- **GTK 4 and libadwaita.**
  - [`AdwViewStackPage`](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/class.ViewStackPage.html):
    `needs-attention` (a dot) and `badge-number`, combinable.
  - [`AdwTabPage`](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/class.TabPage.html):
    `indicator-icon`, `indicator-activatable` (the tab view emits
    `indicator-activated`, for example to mute a tab playing sound),
    `indicator-tooltip`, `needs-attention`, `loading` (a spinner replaces
    the icon). This is the clearest model of a mark that is **passive by
    default and can be made interactive**, with its own tooltip.
  - `AdwSpinner`, `GtkLevelBar` (continuous or discrete, named offsets
    mapped to style classes). Animations follow
    `gtk-enable-animations`.
- **Qt and KDE.** Qt has no badge widget: applications use
  `QTabBar::setTabButton`, `setTabIcon`, a `QToolButton` with a painted
  overlay, `QStatusBar::addPermanentWidget`.
  `KStatusNotifierItem` has a status (Passive, Active, NeedsAttention)
  with an attention icon. Launcher counts go through the LauncherEntry
  D-Bus interface (`count`, `progress`, `urgent`).
- **macOS.** `NSDockTile.badgeLabel` (free text); SwiftUI `.badge()` on
  list rows and tab items, **zero hides it**;
  `NSWindow.isDocumentEdited` puts a dot in the close button (the dot
  replacing the close button again); `NSColorWell` (an interactive
  swatch); Finder tags (coloured dots, several per item, each named).
- **Fluent.** [`Badge`, `CounterBadge`,
  `PresenceBadge`](https://fluent2.microsoft.design/components/web/react/core/badge/usage):
  six sizes from tiny to extra-large, appearances (filled, ghost,
  outline, tint), shapes, a closed colour set (brand, danger,
  important, informative, severe, subtle, success, warning).
  `CounterBadge`: `overflowCount` 99 gives "99+", `showZero` false by
  default, a `dot` mode. `PresenceBadge` gives each presence its own
  icon, not only a colour. Badges are **not interactive**: the meaning
  goes into the host's accessible name ("Inbox, 3 unread"). WinUI's
  [`InfoBadge`](https://learn.microsoft.com/windows/apps/design/controls/info-badge)
  has three forms (dot, icon, number).
- **Material 3.** [Badges](https://m3.material.io/components/badges/specs):
  a small 6 dp dot without text, a large 16 dp badge with a number of at
  most four characters ("999+"), anchored on the top-end of an icon.
- **Photo applications.** Lightroom Classic: pick, reject, unflagged;
  colour labels; stars 0 to 5. darktable: stars, reject, five colour
  labels that combine, clickable overlays on thumbnails. egui-pop's
  library already follows both.
- **Accessibility rules** (WCAG 2.2):
  [1.4.1](https://www.w3.org/WAI/WCAG22/Understanding/use-of-color.html)
  colour is never the only carrier; 1.4.11 non-text contrast 3:1 for
  marks that carry a state;
  [2.5.8](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html)
  targets of 24 by 24 px, or spaced so a 24 px circle on each does not
  overlap a neighbour's; 2.2.2 moving content lasting over 5 s can be
  stopped; reduced motion swaps a pulse for a still shape.

**Takeaways.**

1. Sizes: dot 6 to 8 px; a numeric badge 16 px high, at least as wide,
   a pill as it grows; a 24 px target for anything clickable.
2. Overflow: `cap+`, the cap a parameter (Fluent 99, Material 999); zero
   hidden by default; a dot mode; the real count in the accessible name
   even when capped.
3. Placement: a badge on the top-end corner of its host, overlapping
   the edge; inline marks after the label.
4. The modified dot takes the close button's slot (VS Code, macOS).
5. Passive by default, with the meaning in the host's name; opt-in
   activatable marks with a tooltip and a name of their own (libadwaita,
   darktable, `NSColorWell`, VS Code's status items).
6. Severity is a closed set of tokens; free colours only for data
   (swatches, colour labels), and those still get a name.
7. A loading state replaces the icon (libadwaita, egui-pop's menus
   already do it).

## 2. Places of use

Each place gives: which kinds serve there, passive or interactive, the
anchor, and a rule.

### 2.1 egui-pop

- **Menus.** A swatch for the active colour (passive: the entry is the
  button), a dot for an on/off state without a tick, a badge for a
  count ("Recent files 12"), the pip already drawn while a command
  runs. Anchor: the icon gutter (an `Atom`), or the trailing column
  before the shortcut. Rule: the mark never takes a click of its own
  inside an entry; the entry's AccessKit name carries the state
  ("Accent colour: blue").
- **Tabs and panel headers.** VS Code's dirty dot: a modified dot that
  becomes the close button on hover or focus; an error badge on a
  panel's tab or activity bar icon. Anchor: the close slot (dot) or the
  icon's top-end corner (badge). Rule: a dot that turns into a close
  button follows button rules in its close state only.
- **Tree rows.** Flags (modified, pinned, error), a pip while a lazy
  node loads, a count of children. Anchor: inline before the row
  widgets, or the icon's corner. Rule: passive in the row (the row is
  the target; Space and Enter are the tree's); an interactive mark is a
  row widget with its own name and stays outside the selection.
- **Status bar.** Today's `strips::Indicator` becomes a composition of
  kinds: an LED for a connection, a badge for a count (not `"(n)"`), a
  pip for work, a dynamic icon. Anchor: inline. Rule: the item is the
  button, with the full button rules; the marks inside it are passive.
- **Dialogs.** An activity LED (a probe, a preview rendering), a field
  state flag (modified, required, invalid), a swatch for the current
  value. Anchor: inline after a label, or the field's corner. Rule:
  passive; a field's state goes into the field's description.
- **Buttons and toolbars.** A badge on an icon button ("3 filters"), a
  dot for "something new". Anchor: the icon's top-end corner,
  overlapping the button's frame. Rule: the badge never grows the
  button's hit rect; the button's name includes the count.
- **Node graph ports.** A port is already an interactive dot; a node
  header gets a busy pip and an error badge. Anchor: the port centre,
  the header's end. Rule: an interactive port keeps a hit target of the
  interactive size even when drawn at 10 px (section 3.2).

### 2.2 Helios

Current ad hoc marks, found in the Helios GUI crate:

- **Status bar.** A text glyph "● REC" in the error colour while a
  recording runs: no tooltip, no name beyond its text, and "error"
  borrowed to mean "recording". The jobs item is egui-pop's. Proposed:
  an LED kind `Recording` (its own token, a solid dot plus the word),
  an LED for node and AI provider connections, the jobs pip, a badge
  for notifications.
- **Library thumbnails.** Flags, colour labels, stars, stack counts
  and the `RAW` badge all come from egui-pop's library views. Helios
  only gives data, which is the target state. The `RAW` text is a
  literal (it is a format name, so not necessarily translated).
  Proposed: unchanged for Helios; inside egui-pop the library's pills
  and label shapes become the badge and flag kinds.
- **Masks tree.** Kind icons are Unicode glyphs and emoji (the emoji
  depend on egui's emoji font); the eye and lock toggles are egui-pop
  `Icon`s with a tone, a label and a tooltip, the cleanest marks in the
  tree. Proposed: the kind icons move to the icon registry (section 4);
  the eye and lock become dynamic icons, interactive; a mask in error
  gets a flag.
- **Parameters panel.** Three painted marks with literal sizes (a 3 px
  dot in a 10x18 slot, a 4 px dot or 4.5 px diamond in a 12x18 slot):
  "changed from default" (interactive: a click resets, tooltip only,
  no AccessKit name), "held by another origin" (warning colour) and
  "moved by another origin" (a diamond that fades). Their tooltips are
  not translated. A documented "outside the soft range" mark is not
  drawn. Proposed: the dot and the flag kinds, with the reset dot as an
  interactive dot (button rules), its name "Reset to default".
- **Stack review.** A "★" glyph prefixes the proposed best member;
  screen readers read "star". Proposed: a flag kind `Best` with a name.
- **Connection states.** None: a lost node link shows only in a job's
  log. Proposed: an LED per node link and per AI provider in the status
  bar (off, connecting with a pulse, on, lost), each with a tooltip and
  a click opening its detail.
- **Jobs.** Everything is egui-pop's jobs item; it would gain the pip
  kind and a failure badge instead of coloured text.

Inconsistencies to remove on the way: glyph marks next to painted
marks (the parameters panel paints because fonts may lack glyphs, the
status bar and stack review use glyphs), sizes as literals, colour
meanings borrowed (error for recording, the selection colour for two
marks), tooltips without AccessKit names.

## 3. The proposed `indicator` module

### 3.1 Where it lives

`egui-pop-core::indicator` (L2): it needs the theme, the motion clock,
icons and AccessKit, nothing from the application API. The status bar
item (`strips`), the jobs item, the library and the tree use it from
their own layers. Its tokens join the contract in `egui-pop-tokens`.

### 3.2 Six kinds

| Kind | Shape | States or content | Default size |
|---|---|---|---|
| **Dot / LED** | A filled circle; an LED adds a ring and a glow token | `Off` (ring only), `On`, `Busy` (pulse), `Success`, `Warning`, `Error`, `Attention`; each state has its own shape cue (off hollow, warning a triangle notch, error a square or cross) | `indicator.dot.size` 8 px |
| **Badge** | A pill with a count or up to three characters | A number (capped: `99+` by default, the cap a parameter), a short text (`!`, `NEW`), or a dot when the count is shown as presence only; tone | `indicator.badge.height` 16 px, min width = height |
| **Activity pip** | The existing spinner, or a determinate ring | Indeterminate, a fraction, done (a tick for `indicator.pip.linger`), failed; the busy delay and minimum of `motion::Busy` | `progress.ring` (shared) |
| **Dynamic icon** | An `Icon` (or a registry name) chosen by a state | A map from state to glyph and tone: eye and eye-off, link and unlink, a level (battery-like, stepped), lock and unlock | `size.icon.inline` |
| **Flag** | A corner triangle, a stripe, a chip or a label shape | Modified, pinned, picked, rejected, required, best, invalid; colour labels as their shapes | `indicator.flag.size` 6 px (triangle), stripe 3 px |
| **Swatch** | A rounded square with an outline; a checkerboard under alpha | A colour (a token or a value), live; optional "none" (a slash) and "mixed" | `indicator.swatch.size` = `size.icon.inline` |

**Passive or interactive**, per instance (`.interactive(...)`, off by
default):

- **Passive**: `Sense::hover()` at most (for the tooltip), no focus, no
  AccessKit node of its own. Its state is written into its **host's**
  description: the host widget's name gets a suffix ("Masks, 3
  errors"), or its AccessKit description is set. Fluent's and
  Material's guidance is the same: a badge is not a control.
- **Interactive**: the full button rules of egui-pop:
  - a hit target of at least `indicator.target` (24 px, WCAG 2.5.8),
    centred on the mark, never drawn; when the target would overlap a
    neighbour's, the indicator is laid out with its target, not its
    mark;
  - focus: in the tab order, the theme's focus ring around the target;
  - keyboard: Space and Enter activate;
  - a tooltip (required: an interactive mark has no visible text);
  - an AccessKit node of its own, role `Button` (or `CheckBox` for a
    dynamic icon that toggles, with its toggled state), a name that
    says the action and the state ("Hide mask", "Reset exposure to its
    default");
  - disabled with a reason in the tooltip, like every egui-pop button;
  - its activation can run a command (`CommandId`), so it is a step and
    reachable by agents.

**Never colour alone**: every state has a shape, glyph or text, and a
name. Colour labels keep the library's shapes.

### 3.3 Tokens

Component tokens aliasing system tokens, as `progress.*` and
`library.*` do:

- sizes: `indicator.dot.size`, `indicator.led.ring`,
  `indicator.badge.height`, `indicator.badge.font` (alias
  `font.size.small` scaled), `indicator.badge.padding`,
  `indicator.flag.size`, `indicator.swatch.size`,
  `indicator.swatch.radius`, `indicator.target` (24 px), and the corner
  offset `indicator.offset`;
- colours by **one shared tone set**: `neutral`, `accent`, `info`,
  `success`, `warning`, `danger`, plus `recording` and `attention`
  (alias `status.danger` and `accent.default` by default, but separate
  so a theme can tell recording from an error);
  `indicator.badge.<tone>` (fill) and `indicator.badge.<tone>-text`;
  `indicator.led.off`; `indicator.outline` (the ring that separates a
  badge or swatch from what it sits on, alias of the surface);
- motion: `indicator.pulse.period`, `indicator.pulse.count` (an
  attention pulse runs a few times, then stays still), reusing
  `motion.spinner.*` for pips;
- contrast: pairs in `contrast::PAIRS`: each badge text on its fill
  (4.5:1), each dot and flag tone against `surface.panel`,
  `surface.raised` and the menu surface (3:1, interface parts), the
  outline against the badge fill.

The tone set is the occasion to settle the four existing `Tone`
enums (section 5, question 2).

### 3.4 Anchoring

An indicator is a **decoration**, never a host (ADR 0013): it does not
change its host's size or layout unless placed inline.

- **Inline**: `ui.add(indicator)` allocates its size (or its target
  when interactive) in the flow, like a label; inside a button or menu
  entry it goes in an `Atom::custom` slot.
- **On a rect**: `indicator.paint_at(ui, host_rect, Anchor::TopEnd)`
  after the host is drawn. Anchors: the four corners, `Start`, `End`
  (vertically centred), `Center`; an `offset` in points; `inset` or
  `overhang` (a badge overlaps the corner of an icon by half its height,
  as Material and Fluent do). `TopEnd` follows the layout direction
  (right in left-to-right, left in right-to-left).
- **On a response**: `response.with_indicator(indicator, Anchor)`
  (an extension trait), which also merges a passive indicator's state
  into the response's AccessKit description.
- The painter layer is the host's, so clipping follows the host; an
  overhanging badge is drawn on the host's layer with clipping widened
  by `indicator.offset` (not decided: a separate foreground layer is the
  alternative).

### 3.5 Pulse and reduced motion

- A pulse (LED busy, attention on a change to error) is an opacity and
  scale oscillation on `motion::phase`, so a frozen test clock gives
  the same frame every run.
- Under reduced motion (`motion.duration` = 0) there is no pulse: the
  mark shows its "busy" or "attention" **shape** still (a ring around
  the dot, a thicker outline), so the state is not lost.
- An attention pulse runs `indicator.pulse.count` times after the state
  changes, then stops; continuous pulses are only for busy.
- Frames are asked through `motion::animate` (rate-limited, not when
  hidden, not under reduced motion), only while the indicator is drawn.
- Screen readers: a change of state is announced politely once (the
  AccessKit live region of the host, or egui-pop's announcement
  channel), not on every count update; counts coalesce.
- A user can silence a noisy indicator (not decided: a per-indicator
  setting in the value store, or only for status bar items).

### 3.6 State visible to agents and tests

Following egui-pop's conventions (the debug bridge's collectors,
`describe()` returning JSON, the test kit's AccessKit dump):

- Each indicator has an optional **id** (`Id`) and a `describe()`:
  `{ "kind": "badge", "id": "...", "host": "...", "state": "count",
  "value": 3, "text": "3", "tone": "danger", "interactive": false,
  "pulsing": false }`.
- Indicators drawn in a frame with an id are recorded in a per-frame
  registry in the context's temp data; a core collector
  **`indicators`** snapshots it (what the user sees now), so an agent
  calls `indicators` instead of reading pixels. The status bar's
  `strips::Indicator` states join the same snapshot.
- Interactive indicators run commands, so they are already steps;
  passive ones produce no steps.
- Test hooks in `egui-pop-test`: `indicators(&harness)` (the frame's
  records), `assert_indicator(&harness, id, state)`, and the existing
  `assert_all_named` covering interactive ones. The AccessKit dump
  shows a passive indicator in its host's description.

### 3.7 API sketch

```rust
use egui_pop_core::indicator::{Anchor, Badge, Dot, DynIcon, Flag, Indicator, Pip, Swatch, Tone};

// Passive, inline.
ui.add(Dot::led(LedState::On).name("Node connected"));
ui.add(Pip::busy(job.started).name("Exporting"));
ui.add(Swatch::color(current).name("Accent colour"));

// On a host's corner, passive: the state joins the host's name.
let r = ui.add(Icon::Bell.size(Size::Toolbar).label("Notifications").sense_click());
r.with_indicator(Badge::count(unread).tone(Tone::Danger), Anchor::TopEnd);

// Interactive: button rules, a command.
if ui
    .add(Dot::new(Tone::Accent).interactive("Reset exposure to its default"))
    .clicked()
{ /* reset */ }

// A dynamic icon that toggles.
let shown = ui.add(
    DynIcon::toggle(mask.visible, Icon::Visible, Icon::Hidden)
        .names("Hide mask", "Show mask")
        .interactive(),
);

// A flag in a tile's corner.
Flag::modified().paint_at(ui, tile_rect, Anchor::TopStart);

// Badge content.
Badge::count(n)            // "0" hidden unless .show_zero(), "99+" over the cap
    .cap(999)
    .tone(Tone::Danger);
Badge::text("NEW");
```

Common builder methods: `.id(..)`, `.name(..)` (AccessKit, required for
interactive), `.tooltip(..)`, `.tone(..)`, `.size(..)` (a size step,
not points), `.interactive(name)`, `.command(CommandId)`,
`.pulse(Pulse::Attention)`, `.disabled(reason)`.

`strips::IndicatorState` gains `marks: Vec<Mark>` (or keeps its fields
and draws them with the new kinds; not decided). The library's `Badge`
and its pills move onto `indicator::Badge`, keeping the library's
names as re-exports.

### 3.8 Tests

- **Unit** (no rendering): badge text and cap (`0`, `9`, `99`, `100`
  gives `99+`, a custom cap, negative refused), anchor geometry in
  left-to-right and right-to-left, the hit target never under
  `indicator.target` when interactive, the pulse phase static under
  reduced motion, `describe()` JSON.
- **kittest interaction**: an interactive dot found by role and name,
  activated by click, Space and Enter, focus ring shown on Tab; a
  passive badge adds no node and changes its host's name; a disabled
  interactive mark says why; a toggle reports its toggled state.
- **AccessKit**: `assert_all_named` on the gallery page; announcements
  once per change, not per frame.
- **Agents**: the `indicators` collector lists what the page draws,
  with ids and states.
- **Tokens**: every new contrast pair passes in every built-in theme
  and density.
- **Snapshots** per theme of the gallery page, at rest and frozen
  mid-pulse, and with reduced motion.

### 3.9 Gallery page

An "Indicators" page:

- one row per kind, every state side by side, passive then
  interactive, at rest, hover, focus and disabled;
- the anchors on a sample icon button, tab, tree row, tile and menu
  entry (corners, inline, overhang), in left-to-right and right-to-left;
- a playground: kind, state, count, tone, size, anchor, interactive,
  pulse, reduced motion;
- a live line: a fake status bar with an LED cycling connection states,
  a jobs pip, a badge counting fake notifications;
- the page's indicators visible in the debug page through the
  `indicators` collector.

## 4. Dependencies

- **The mount contract (ADR 0013, accepted, on the `wip/mount-contract`
  branch, not yet on main).** Indicators are decorations: they declare
  no `Mountable` of their own and need no host offer. What they need
  from it: the vocabulary for "decoration on a host" (so the generated
  host matrix does not list them as components), and the status bar's
  offer (one row), which bounds the badge and LED heights. Inline and
  on-rect anchoring do not wait for it. The gallery page and the
  `indicators` collector do not either. **Waits**: moving
  `strips::Indicator` onto the new kinds, since the status bar's offer
  and its items are reworked by that branch.
- **The icon registry (pending: icons declared once on commands and
  object kinds, shown in menus, the palette, toolbars and the status
  bar).** The dynamic icon kind maps states to icons. With today's
  `Icon` enum it can ship for the built-in roles (eye, lock, link,
  check, warning). **Waits for the registry**: dynamic icons naming a
  plugin's own icons, a command's "on" icon (the registry's second
  icon for the on state), icons resolved by name in `describe()` and
  from agents, the status bar items built from a command's icon, and
  the Helios masks kind icons. The menu gutter (the registry's aligned
  icon column) is where menu swatches and dots sit, so menu placement
  also follows the registry.
- **Field states** (the forms work: modified, required, invalid): the
  flag kind provides the marks; the forms decide where they go.
- No new dependency; the module is egui and egui-pop only.

Order of work proposed: tokens and the five kinds that need nothing
(dot, badge, pip, flag, swatch), the gallery page and tests; then the
library and tree moved onto them; then, after the mount contract, the
status bar items; then, after the registry, the dynamic icon's named
icons and the menu placement. Helios adopts in the same order.

## 5. Open questions for the owner

1. **The name.** `strips::Indicator` already names a status bar item.
   Keep `indicator` for the new module and rename the status bar's to
   `StatusItem`, or call the new module `marks` (which the library
   already uses for rating, labels and flags)? Proposed: rename the
   status bar's type, `indicator` for the module.
2. **One tone enum.** egui-pop has four (`icon::Tone`,
   `progress::Tone`, `strips::Tone`, `library::grid::Tone`). Merge them
   into one shared tone in core (breaking, before 0.1 is published), or
   map each to the indicator tones?
3. **Badge cap**: `99+` by default (VS Code, Fluent, Material small) or
   `999+` (Material large)?
4. **Recording** as its own tone, or `danger`?
5. **Overhanging badges**: clipped to the host's layer widened, or a
   foreground layer of their own?
6. **Silencing**: may the user mute any indicator, or only status bar
   items?
7. **Dirty dot on tabs**: does the workbench adopt VS Code's dot that
   becomes the close button, given that tabs hide their close buttons
   today?
