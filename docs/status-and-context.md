# The status bar and the context stack

**Status: decided (2026-10-21), built.** This note settles how the status
bar and the context stack ([contexts.md](contexts.md), ADR 0012) meet.
The owner accepted every recommended default below, the open questions
included. How to use it is in [workbench.md](workbench.md#slim-strips);
"As built" at the end says where the code differs in its details.

## Why together

The context stack says where the user is (the place, a drill-down shown as
a breadcrumb) and what is active (modes, shown as chips). The status bar
(`strips::Indicator`, fed by the host's events) says what is going on.
Both answer "where am I, and what is active?". Today they are two separate
widgets an application places on its own. Putting the context in the
status bar gives that answer one home, always in the same spot.

## 1. The place at the start of the status bar

The breadcrumb (`library › darkroom › IMG_0042`) sits at the start of the
status bar, before the indicators. A crumb is a button: a click goes back
to that place (`context.go`), as `nav.up` does one level at a time. On a
narrow window the middle crumbs collapse into `…`, which opens a menu.

**Default:** on, when the context plugin and the status bar are both
installed. An application can keep the breadcrumb elsewhere and turn this
off (`StripsPlugin::breadcrumb(false)`).

## 2. Modes as status items

Each active mode shows as a chip after the breadcrumb: its title, coloured
with a shared tone (the micro-indicators' `Tone`), and an icon when it has
one.
- A click on a chip leaves the mode (`context.set_mode` to the group's
  default).
- A pushed mode (one that remembers what it replaced) shows a back mark;
  a click returns to what it replaced, as `nav.back` does.
- A mode that refuses to be left (a guard) shows its reason in the
  tooltip and keeps the chip.

**Default:** only modes whose group is declared `visible` show. A group
that is pure plumbing stays out of the bar.

## 3. Status items that follow the context

An indicator gains a `when` clause, with the same grammar as commands, over
the context keys (`place`, `placeDepth`, `modePushed`, `mode.<group>`, and
each frame's own keys). The bar shows an indicator only where its clause
holds. Examples:
- the image count shows in `place == 'library'`;
- zoom and the history step show in `place == 'darkroom'`;
- mask tool hints show in `mode.edit == 'masks'`.

An indicator with no `when` shows everywhere, as today.

**Default:** a hidden indicator keeps reducing events, so its state is
right the moment it reappears. Only its drawing is skipped.

## 4. One saved state

The context stack already saves the place, the modes and each projection's
view state, and restores them on reopen. The status bar's user choices go
into the same document, per place, so a bar arranged for the darkroom
comes back as it was:
- which items the user hid (only status items can be muted, per the
  micro-indicators decision);
- their order.

**Default:** the choices are per place, falling back to the root place's
choices where a place has none.

## 5. What agents read

`context.read` returns, beside the place and modes, what the status bar
shows: each visible indicator's id, text, tone, count and progress. An
agent then sees the same summary as the user, in one call. The `strips`
debug collector shows the same thing, with the `when` that hid each
hidden item.

**Default:** `context.read` gains a `status` field, and nothing else
changes in the command.

## Open questions

1. Should a click on the place's last crumb (the current place) open a
   quick switch to sibling places, or do nothing? *Default: nothing.*
2. When the window is too narrow for the breadcrumb, the chips and the
   indicators, which gives way first? *Default: the breadcrumb collapses
   first, then low-priority indicators, then chips; chips never hide,
   since they show active states.*
3. Do frames contribute their own status items (declared with the frame,
   live only while it is entered)? *Default: yes, as a convenience over
   writing the `when` by hand.*

## Build order

The status bar's next pass (a rich hover and popover per item) was an
ordering preference, not a dependency: these steps were built before it.
1. `when` on indicators. *Built.*
2. The breadcrumb and chips in the bar. *Built.*
3. The saved choices per place. *Built.*
4. The `status` field in `context.read`. *Built.*
5. Gallery page, tests (*built*), and an adoption by an application
   (library, darkroom, culling), still to come.

## As built

- `Indicator::when(clause)`; `Indicator::during(&frame)` answers open
  question 3: it writes the frame's clause (`FrameSpec::clause`: the top
  place, or the mode on in its group), so a frame's status items need no
  clause by hand. A clause that does not parse keeps the item hidden and
  is a diagnostic (`indicator-when`).
- Groups have no declaration of their own: a mode declared
  `FrameSpec::visible()` makes its group visible. A chip's tone is
  `FrameSpec::tone(name)` (a kit tone name, `accent` by default); a chip
  is the micro-indicators' text badge, interactive.
- A group has no default mode: a chip's click turns its group off
  (`ClearMode`), or pops the last pushed mode (`PopMode`, back to what
  it replaced). A crumb goes back with `context.jump` to its depth, the
  same transition as `context.go` to its path.
- The guard's reason is asked as a dry run of the stack's own owner
  (`contexts::why_not`) only while the chip is under the pointer; a host
  owning the stack (an answer that may come later) gives no reason
  there, its refusal still shows in a toast on a click.
- "Low-priority indicators" are the heaviest (`weight`): the lightest
  sit at the bar's end and stay longest.
- The choices are a value per place in the `view` document
  (`contexts::place_value`, keyed by the place's frame id, `root` at the
  root), under `egui_pop.status`; without the context stack they last
  the session. They are changed from an item's context menu, or by
  `status.item.hide` and `status.item.move`.
- The root crumb shows at the root too ("You are in Home" for a screen
  reader), so the bar always says where the user is.
