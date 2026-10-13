# 0013: The mount contract: components declare needs, hosts declare offers

Date: 2026-10-14. Status: **proposed**.

The analysis behind it (what egui-pop has, the contract in detail, the
survey, the free-content dialog, the steps, the risks and the open
questions) is in the [appendix](0013-mount-contract-analysis.md).
Today's matrix, written by hand from the code, is
[hosts.md](../hosts.md).

## Context

Whether a component can be put in a place is answered nowhere. A dialog
cannot hold a viewport (its four kinds take no free content), although
its `Clear` backdrop was made for a dialog previewing its effect on the
image. A panel detached into an auxiliary window loses the keymap
without saying so. A palette cannot be resized, so a viewport in it has
no size to fill. Each answer is buried in one module, and the only
declaration that says where something may go, `PanelSpec` (areas,
`min_size`, `detachable`, `floatable`, `palette`, `refuses(area)`),
covers panels in areas and nothing else.

What is missing is a low-level connector shared by every place and
every component, like the studs of building bricks: a small, common
vocabulary in which a component says what it needs and a place says
what it offers, so that "can this go there, and if not, why not" is
computed, tested and documented from one source.

## Decision (proposed)

**Components declare needs** (`Mountable`, in `egui-pop-model`, no
egui): keyboard (none, while hovered, when focused, or exclusive) and
the keys it claims (Enter, Escape, Space, Tab); pointer (drags inside,
drag out, wheel, bitmap cursors); workers (a producer the application
runs, drained when drawn) and whether they must keep running while
another surface is modal; surface (egui textures, a GPU paint callback,
the image's own surface) and colour (sRGB, wide gamut wanted, wide
gamut required); size (minimum, preferred, maximum, fills or not,
aspect); instances (single, through a service, or several, through the
caller's state); a view-state key; its commands and keymap surface; an
accessibility role; whether it needs an `AppHandle`; whether it
**asks a question** (must be answered before going on); whether it
holds a **session** (an edit, a running program) that must not end by
accident; and a non-interactive preview mode.

**Hosts declare offers** (`HostOffer`): size (fills a rect, bounded
width with content height, one row, none), resizable by the user; focus
policy (shared, click-into, modal-exclusive, none) and the keys the host
owns (Enter, Escape, a click outside); lifetime (persistent or
transient); window (main, auxiliary, follows its panel); whether the
keymap reaches it; participation in the context stack's projection
(ADR 0012); what it keeps (place, view state); whether it gives a `Ui`
and an `AppHandle`.

**Mounting is matching**, a pure function in `egui-pop-model`:
`check(needs, offer) -> Verdict`, one of **fits**, **fits under
conditions** (each condition named and explained), or **does not fit**
with a reason of one kind: **by design** (a named rule and what to do
instead), **an egui-pop limit** (with its gap), or **an upstream
limit** (with its entry in `docs/upstream.md`). "Not tried" is not a
verdict: a component or host without declarations shows as a gap. The
rules are named and few (size, keyboard and claimed keys, lifetime and
sessions, questions, display, instances, application handle, view
state); each rule's text is the reason shown.

**The matrix is generated.** `docs/hosts.md` is produced from the
declarations (an xtask writes it; a test fails when the committed page
is stale), so the "why" in each cell is the unmet need, never a
sentence written apart from the code.

**Each component has one mount test** against mock hosts built from
each host's offer (`egui-pop-test`): it mounts the component's
fixture in every host its verdict accepts, checks the conditions that
can be checked headless (size kept, every interactive node named, a
global binding refused under a modal with the trace's verdict), and
checks that a refused mount is reported with its rule (ADR 0003). A new
host or a new component is covered by the same loop.

**The free-content dialog is a host.** A `Custom` dialog kind takes a
body and keeps every dialog rule (a primary button that says what it
does, Enter and Escape, steps, agents through the collector, movable,
closable, a remembered position, the backdrop); its offer is modal,
transient, bounded, keyboard exclusive with claims. Its first use is a
preview dialog with an image viewport and the `Clear` backdrop.

**Existing declarations become offers and needs.** `PanelSpec`'s
`only_in`, `min_size`, `detachable` and `floatable` stay, and gain the
content's `Mountable`; `refuses(area)` generalises to `refuses(host)`.
Zones, the workbench's areas, palettes, auxiliary windows, the overlay
stack's modals and popovers, the status bar and the gallery's stories
each declare an offer.

## Consequences

- "Where can this go?" has one answer for people (the generated page),
  for code (`check`), for agents (a `mounts` collector and an
  `AppHandle` query), and for tests.
- A refused mount (detaching a panel whose content requires the main
  window, putting an edit session in a popover) is reported with its
  rule instead of failing silently.
- Every component crate carries one declaration per component and one
  mount test; a plain widget declares `Mountable::plain()` and changes
  nothing else.
- The gaps found while writing the first matrix (no free-content
  dialog, palettes not resizable, no keymap in auxiliary windows,
  overlays in the main window only, no custom settings page) become
  tickets, each a rule whose verdict changes from "egui-pop limit" to
  "fits".
- Declarations can lie; the mount tests are what keeps them honest.
