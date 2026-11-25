# Moving a floating panel vs docking it

Status: **design note, decided and built** (study and decision
2026-11-17; built in egui-pop 0.13, [workbench.md](../workbench.md)). A
plain drag of a floating panel only moves it; the dock modifier (Ctrl,
Cmd on macOS) shows the drop targets and docks (option A, section 4).
A is egui-pop's default, not a fixed rule: the mode is a declared
option an application picks the default of, and a user setting.

The problem: while a floating panel (a palette, alone or in a group)
is dragged, every side or bottom area under the pointer, every closed
area's edge and every other palette is a drop target. A user who only
wants to move the panel cannot avoid them: let go over a target, the
panel docks or joins a group.

Contents:

1. How other applications tell moving from docking
2. Modifier keys per platform, and conflicts
3. What egui-pop and egui_stack do today
4. Options, and the decision
5. The setting, the gesture, the bus
6. Discoverability: the hint while dragging
7. Implementation
8. References

## 1. How other applications tell moving from docking

| Application | Plain drag of a floating window | Docking | Modifier |
|---|---|---|---|
| Visual Studio | shows the guide diamond; let go over an arrow, it docks [VS1] | plain drag | **Ctrl** held: moves without docking [VS1] (same since VS 6 [VS2]) |
| JetBrains IDEs | a floating tool window moves as a window | the View Mode menu (Dock Pinned, Undock, Float, Window) [JB1]; the tool window buttons drag between the bars [JB2] | none |
| Photoshop (and Bridge, the same panel system) | drop zones highlight while a panel is dragged; let go over one, it docks [AD1] | plain drag | **Ctrl** (Windows) / **Cmd** (macOS) held: no docking, the panel floats where let go [AD1] [AD2] |
| Lightroom Classic | no floating panels: the panels sit in fixed side groups | none by drag | none |
| Blender | no floating editors; areas split, join and swap from a corner drag [BL1] | joining and replacing by dragging a corner into another area | Ctrl: swap two areas; Shift: duplicate into a new window [BL1] |
| GNOME | a window moves; dragged to the top edge it maximizes, to a side it tiles [GN1] | (edge tiling) | Super + drag moves a window from anywhere in it [GN1] |
| KDE Plasma | a window moves; the screen edges quick-tile | custom tiles only with **Shift** held while dragging [KD1] | users ask for the reverse too (Shift to suppress) [KD2] |
| Windows 11 | a window moves; dragged to the top, the snap layouts show [MS1] | snap | a setting turns the snap layouts on drag off [MS1] |
| egui_dock / egui_stack | a floating window (an `egui::Window`) moves by its frame, no targets | only a dragged **tab** (or a tab bar) shows the drop overlay [ED1] | none; the overlay's feel has timings (`max_preference_time`) but no dwell to arm it [ED1] |

Two families:

- **Plain drag docks, a modifier suppresses** (option B below): Visual
  Studio, Photoshop and the Adobe panel system.
- **Plain drag moves, docking is a separate act** (option A): KDE's
  custom tiles (Shift to snap), JetBrains (a menu), egui_dock (a
  different handle: the tab, not the window frame).

Nobody arms docking by resting on a target (option C); window managers
use an edge and a short delay for tiling, not for docking.

## 2. Modifier keys per platform, and conflicts

- **Ctrl on Windows and Linux, Cmd on macOS.** egui's
  `Modifiers::COMMAND` and egui-pop's keymap (which reads Cmd as `Ctrl`
  on macOS) already give this. On macOS, Ctrl + click is the secondary
  click: Ctrl must not be the dock key there; Cmd is (Photoshop's
  choice too).
- **Alt is taken on Linux**: Xfce moves windows with Alt + drag, KWin
  could (it defaults to Meta now), and GNOME uses Super
  (`docs/design/pointer-gestures.md`, section 5). Alt + drag would move
  the whole application window instead of the panel. Super and Meta are
  the window manager's. So Alt, Super and Meta are out.
- **Shift** is free on a palette's band, but KDE uses Shift + drag of a
  window for its tiles; using it inside the application would teach the
  opposite of the desktop's own Shift.
- **Ctrl inside egui-pop**: no binding uses Ctrl with a drag on a
  palette's band or a tab. Ctrl + drag is bound elsewhere, on other
  surfaces: `gesture.sections.pull` (Ctrl + drag on a section boundary,
  Cmd on macOS), the timeline's pan, the Photoshop profile's zoom box on
  a canvas, finer steps on sliders and scrub fields. None of them is a
  palette band, so the dock key on a band conflicts with none. Keyboard
  chords starting with Ctrl (Ctrl+K and others) are key presses, not a
  drag: no conflict either.
- **GNOME's "Locate pointer"** (an accessibility option) answers a
  lone Ctrl press and release; held during a drag, it does nothing.
- **The web**: Ctrl + drag in a browser drags a link or an image; the
  canvas takes the pointer first, as it does for every drag.

The modifier is read **live, every pass of the drag**, as Visual Studio
and Photoshop do: pressed or released mid-drag, the targets show or go.

## 3. What egui-pop and egui_stack do today

**A floating panel** (`crate::palettes`, in egui-pop-api). A palette or
a group is dragged by its band (`follow`), a member by its own band or
a tab (`follow_member`). Each pass of the drag:

- the panel moves live, snapped to the workbench's edges and to the
  other palettes within the `palette.magnet` token, clamped inside;
- `join_target` looks for another palette under the pointer (top
  quarter: above; bottom quarter: below; middle: as a tab) and keeps a
  `GroupHint`;
- `paint_hints` draws the group hint, and `publish_hint` (workbench)
  draws the dock hint, with the ghost of the panel's tab, when the
  workbench's `dock_at` finds a side or bottom area under the pointer,
  or a closed area within the magnet's width of the edges.

Let go, the order is fixed: a group hint joins (`palette.group`), else
an area docks (`palette.dock`, the whole group as tabs), else the panel
moves (`palette.move`) or comes out of its group (`palette.ungroup`).
No modifier is read. A palette snapped to the window's edge sits within
the magnet's width of a closed area's edge, which is where `dock_at`
docks: the very spot the snap brings it to. The test
`a_palette_is_dragged_by_its_band_snaps_and_docks_on_an_area`
(`crates/egui-pop-api/tests/it/palettes.rs`) asserts that a plain drag
over the bottom area docks.

**A docked tab** (egui_stack's `DockArea` in the workbench's
`DockSpace`). Dragging a tab always shows the zones (`OverlayType::Zones`
painted by egui_stack, the panel's refusal from `DropVerdict`); let go
in the main window, the areas take it (`workbench.move`). A tab drag
has no "move only" meaning: it is the docking gesture.

**A whole tab group** (#474, dragged by the empty part of its tab
bar). The same as a tab: zones in the main window; let go over a
detached window it joins it; let go outside every window it opens a
window of its own (`workbench.detach` with `group`).

**A detached OS window** (`workbench.detach`). The system moves it:
by its decorations, or by egui-pop's own title bar through
`ViewportCommand::StartDrag`, which hands the move to the window
manager. egui-pop sees none of it, so moving it never docks. Its panels
dock back with `workbench.attach`, on closing the window, or by
dragging their tabs out of it. A tab or a group dragged across windows
goes through the drag session of `bus::drag` (`pop.drag.start`,
`pop.drag.drop`, `drag.dropped`, `drag.refused`); a detached window is a
drop target of it.

**egui_stack's own floating windows** are off:
`TabViewer::allowed_in_windows` returns `false` in the workbench
(detached panels get OS windows instead).

So the problem is the floating panel alone: it is the only drag whose
two meanings, moving and docking, share one gesture.

## 4. Options, and the decision

- **A. A plain drag only moves; the dock modifier shows the targets and
  docks.** Decided (owner, 2026-11-17), egui-pop's default.
- **B. A plain drag docks; the modifier suppresses docking.** Visual
  Studio's and Photoshop's way; kept as a mode for applications and
  profiles that follow them.
- **C. Docking arms only after the pointer rests on a target for a
  delay** (around half a second). Kept as a mode for touch and pen,
  where no modifier is at hand.

Why A by default: moving a palette is the frequent act and docking the
rare one, so the frequent act should be the one that cannot go wrong;
the targets never flash during a move; a palette snapped to an edge
stays floating; and it is the KDE desktop's own model (a plain drag
moves, a held key snaps into tiles). B keeps the trap for whoever does
not know the key. C slows docking and still docks a user who pauses
over a target.

What the modes cover: the drag of a floating panel's band, a member's
band and a floating tab, for both kinds of target (an area to dock in,
another palette's group to join). Snapping to edges and palettes and
anchoring stay on a plain drag: they are part of moving. Docked tabs,
tab groups and detached windows keep their behaviour (section 3): their
drag has one meaning.

### Per profile

A profile named after an application behaves like it
(`docs/design/pointer-gestures.md`, section 6):

| Pointer profile | Mode | Why |
|---|---|---|
| Document (the default) | A | egui-pop's default |
| darktable | A | darktable has no floating panels |
| Photoshop | B | Photoshop docks on a plain drag, Ctrl / Cmd keeps it floating |
| Lightroom Classic | A | no floating panels; A is the safe default |
| Blender / node editors | A | Blender has no floating editors |
| Trackpad | A | the modifier key is at hand on a laptop; C is for touch |

An application such as a Visual Studio-like editor picks B as its
default; a touch-first application picks C.

## 5. The setting, the gesture, the bus

**The setting**: `input.dock_drag`, a choice in egui-pop's value
store, beside the other pointer settings:

| Value | Meaning |
|---|---|
| `profile` | the pointer profile's mode (the default value) |
| `modifier-docks` | A: a plain drag moves, the dock keys dock |
| `modifier-floats` | B: a plain drag docks, the dock keys keep it floating |
| `dwell` | C: docking arms after resting on a target |

The pointer profile document gains `options.dock_drag` (the three
modes), as it has `slider_wheel`; a profile without it reads A. The
application picks its default with a builder option (`dock_drag` on the
app builder, as `pointer_profile` is), which sets the setting's
default; the user changes it in the settings, like any other. A change
is said once on the bus as `pop.settings.changed`, and the palettes read
the mode in force from the setting, as the pointer options are read.

**The gesture**: `gesture.workbench.dock`, a pointer gesture in the
keymap's default layer (as `gesture.sections.pull` is), bound to
`Ctrl+PrimaryDrag` (Cmd on macOS through the keymap's mapping). Its
keys are the dock keys in A and the keys that keep a panel floating in
B; C does not read it. The user rebinds it in the shortcuts editor; a
user binding survives a profile change, as for the other gestures.

**The bus**: nothing per pass (the drag's painting is ADR 0029's
per-frame exception). What lands is already on the bus and stays so:
`palette.move`, `palette.dock`, `palette.group`, `palette.ungroup`
steps and their `palette.moved`, `palette.docked`, `palette.grouped`,
`palette.ungrouped` events, which hooks can veto as today. The drop
hint an agent reads (`AppHandle::drop_hint`, `palettes::group_hint`,
the `workbench` collector, `overlay_rects`) gains whether the targets
are armed, so a test or an agent sees "over the bottom area, not
armed" without a pixel.

## 6. Discoverability: the hint while dragging

While a floating panel is dragged, a one-line hint shows in the
status bar, or under the pointer when the application has no status
bar, worded after the mode and the keys bound to the gesture (a rebind
changes the text):

| Mode | Keys not held | Keys held |
|---|---|---|
| A | "Hold Ctrl to dock" (only while a target is under the pointer) | "Release Ctrl to move only" |
| B | "Hold Ctrl to move without docking" (only while a target is under the pointer) | none |
| C | "Rest here to dock" (over a target, while the delay runs) | none |

The text names the keys as the keymap prints them on the platform
(Cmd on macOS). Under A the targets stay hidden until the keys are
held, so the hint is the only sign that docking exists: it shows
whenever the pointer is over a place the panel could dock in or join,
armed or not. The band's menu gains **Dock in** (left, right, bottom)
for a pointer with no keyboard, running `palette.dock`.

## 7. Implementation

One ticket in egui-pop, title: **Floating windows: a plain drag moves,
Ctrl docks**.

1. Model (egui-pop-model, `pointer`): a `DockDrag` choice
   (`modifier-docks`, `modifier-floats`, `dwell`) in the profile's
   `options` (A when absent; Photoshop's profile gets B), the
   `gesture.workbench.dock` gesture spec bound to `Ctrl+PrimaryDrag` in
   every shipped profile.
2. Setting (egui-pop-api, `pointer`): `input.dock_drag` (`profile`,
   then the three modes), its default from a new builder option
   `dock_drag`, read with the other pointer options.
3. Palettes (`palettes.rs`, `follow` and `follow_member`; workbench's
   `publish_hint`): each pass, compute **armed** from the mode, the
   gesture's keys held now and, for C, how long the pointer has rested
   on the same target (a dwell of 500 ms, a constant). Not armed: no
   `GroupHint`, no dock hint drawn, and the drop is a move
   (`palette.move` or `palette.ungroup`). Armed: today's behaviour.
   Snapping and anchoring are unchanged.
4. Hint: the status bar line of section 6 (strings in egui-pop's
   catalogue), and `armed` in `DropHint` and `GroupHint`.
5. The band's menu: Dock in Left / Right / Bottom (`palette.dock`).
6. Docs: `docs/workbench.md` and the palettes module's docs; a
   changeset (`egui-pop-api` and `egui-pop-model`, minor: the default
   drag behaviour changes).

**Headless test** (kittest, `crates/egui-pop-api/tests/it/palettes.rs`,
default features):

- mode A, default: a palette's band dragged over the bottom area and
  let go stays floating (`palette.moved`, no `palette.docked`), and
  `drop_hint` reports the area not armed during the drag; the same drag
  with Ctrl held (the harness's modifiers) docks it (`palette.docked`);
  Ctrl pressed only mid-drag, before the release, docks too; Ctrl
  released before the release does not;
- a palette dropped on another palette's middle joins it only with
  Ctrl; snapped to the start edge with a plain drag, it stays floating;
- mode B (`input.dock_drag` = `modifier-floats`): the plain drag docks,
  the drag with Ctrl stays floating;
- mode C (`dwell`): a drag passing over the area without stopping stays
  floating; one resting on it past the delay (the harness's clock)
  docks;
- the setting changed is said once as `pop.settings.changed`; the Photoshop
  profile with the setting at `profile` gives B;
- the existing `a_palette_is_dragged_by_its_band_snaps_and_docks_on_an_area`
  and the grouping tests hold Ctrl where they dock or join.

## 8. References

- [VS1] Microsoft, "Customize window layouts in Visual Studio", Dock
  windows: <https://learn.microsoft.com/en-us/visualstudio/ide/customizing-window-layouts-in-visual-studio>
- [VS2] Microsoft, Visual Studio 6.0, "Working with Floating or
  Docking Windows": <https://learn.microsoft.com/en-us/previous-versions/visualstudio/visual-studio-6.0/aa266154(v=vs.60)>
- [JB1] JetBrains, IntelliJ IDEA, "Tool window view modes":
  <https://www.jetbrains.com/help/idea/viewing-modes.html>
- [JB2] JetBrains, IntelliJ IDEA, "Tool windows":
  <https://www.jetbrains.com/help/idea/tool-windows.html>
- [AD1] Adobe, Bridge, "Adjust panels" (the panel system Photoshop
  shares): <https://helpx.adobe.com/uk/bridge/desktop/workspaces-and-panels/adjust-panels.html>
- [AD2] Adobe Community, Photoshop, "Docking toolbars" (Cmd / Ctrl
  keeps a panel from docking):
  <https://community.adobe.com/t5/photoshop-ecosystem-discussions/docking-toolbars/m-p/10427950>
- [BL1] Blender Manual, "Areas":
  <https://docs.blender.org/manual/en/latest/interface/window_system/areas.html>
- [GN1] GNOME Help, "Maximize and unmaximize a window":
  <https://help.gnome.org/users/gnome-help/stable/shell-windows-maximize.html.en>
- [KD1] KDE bug 464810, "Windows should only snap to custom tiling when
  Shift is pressed, not regular tiles": <https://bugs.kde.org/show_bug.cgi?id=464810>
- [KD2] KDE bug 491909, "Swap tiling manager shift key so that it
  disables using tiling manager when pressed":
  <https://bugs.kde.org/show_bug.cgi?id=491909>
- [MS1] Microsoft Support, "Snap your windows":
  <https://support.microsoft.com/en-us/windows/snap-your-windows-885a9b1e-a983-a3b1-16cd-c531795e6241>
- [ED1] egui_stack 0.25 (egui_dock's continuation), `OverlayFeel` and
  `TabViewer::allowed_in_windows`: <https://docs.rs/egui_stack/0.25.12/egui_stack/>
