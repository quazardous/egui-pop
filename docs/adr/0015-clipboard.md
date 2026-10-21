# 0015: The clipboard: typed items owned by the kernel, offered in several formats, hooked at each boundary

Date: 2026-10-18. Status: **accepted** (2026-10-21).

The analysis behind it (what exists today, how egui and eframe reach
the system clipboard, a survey of crates and of seven applications, the
questions answered, the flows) is in the
[appendix](0015-clipboard-analysis.md). Where they differ, this page
wins.

## Context

egui-pop copies text and nothing else. The code editor and the terminal
handle egui's `Event::Copy`, `Event::Cut` and `Event::Paste` themselves;
text fields keep egui's own handling; the node graph, colours, theme
tokens and table rows cannot be copied at all. There is no clipboard
command, no Edit menu entry, no step to observe or veto a copy or a
paste, and no way for an application to strip what must not leave it
or to adapt what comes in. egui-winit turns Ctrl+C, Ctrl+X and Ctrl+V
into those three events and swallows the keys, so the keymap cannot
bind them; it reads a paste synchronously, as text only, and knows no
primary selection.

An application with a kernel (Helios) copies typed, rich content
(module parameters, mask shapes, sub-networks, colours in a colour
space, library images, workspace parts) that holds ids and paths only
its kernel understands, between windows and between processes, and
lets agents copy and paste through the same commands as the user.
Like the context stack (ADR 0012) and undo (ADR 0014), the clipboard's
content belongs to the kernel; the interface routes the keys to it and
shows it.

## Decision

**A typed contract, implemented by the kernel, with a local default.**
The owner implements `Clipboard` (in `egui-pop-api`, reached through
`Host::clipboard()`, like `Host::contexts()` and `Host::history()`):
slots holding one **item** each, `put`, `get`, `offers`, a revision.
egui-pop's default owner, `LocalClipboard`, keeps the items in memory,
shared by the application's windows. The pure parts (`FormatId`,
`ClipItem`, the boundary rules, `negotiate`) live in
`egui-pop-model::clipboard`.

**An item is typed; the system clipboard gets its fallbacks.** A copy
makes a `ClipItem`: a private type (`application/x-<app>.<type>+json`)
with its version, a structured payload, and its source (origin, window,
place in the context stack, owner). The owner keeps the item (the
**internal clipboard**) and writes its readable **fallbacks** to the
**system clipboard**: text always, HTML or an image when the type
renders one, and the private type itself where the platform backend
allows it. A paste uses the internal item when the system clipboard
still holds what the owner last wrote there (a change counter, else a
hash of the fallback); otherwise the content is external and is pasted
from its own formats.

**The target picks the richest format it accepts.** A target declares
the formats it accepts, richest first; `negotiate` takes the first one
the offer has, or one a declared transform makes from what the offer
has (one transform, no chains). Nothing fits: the paste is refused
with the reason (ADR 0003). **Paste special** lists every format and
transform that fits, the item's parts when its type has parts (a
parameter set's modules), and the target's preview.

**Hooks at each boundary.** Copy and paste are steps (ADR 0011):

- `clipboard.copy`: before-hooks veto (a secret field) or redirect
  (another selection); **sanitisers** run per representation on the
  boundary it crosses. Inside the owner nothing is removed; towards
  the system clipboard local paths, secrets and owner-local ids are
  removed from the fallbacks and from the private payload, and the
  item records what went (`stripped`), shown in the toast.
- `clipboard.paste`: before-hooks veto or redirect (another target,
  another format); external content is sanitised first (HTML reduced,
  control characters removed from text bound for a terminal); then
  **transforms** adapt the representation to the target (a version
  upgrade, a size, a colour space, a unique name). A lossy transform
  says what it lost.

**Plugins declare types and transforms** in their registry and own
them (ADR 0004): a type has an id, a version, a schema, the fallbacks
it renders and a migration from its older versions; a transform has a
source and a target format, a condition, and whether it is lossy.

**A paste is one undo step; a copy is none.** The paste runs under a
history group (ADR 0014) labelled by the type ("Paste 3 nodes"); a cut
is the copy and the delete in one step, and copies only once the delete
is accepted.

**Sources and targets are the focused component's.** A component
registers a `ClipSource` (what a copy takes, what a cut deletes) and a
`ClipTarget` (the formats it accepts, how it applies a paste), as it
registers a local undo scope. The commands `edit.cut`, `edit.copy`,
`edit.paste` and `edit.pasteSpecial` sit in the Edit menu (group
`1_clipboard`) with the keys `canCopy`, `canCut`, `canPaste`, and
`when` `!textInputFocus`: text inputs keep egui's own copy and paste.
Ctrl+C, Ctrl+X and Ctrl+V are dispatched from egui's three events (the
modifiers held then give Ctrl+Shift+V), every other binding from the
keymap.

**Across windows and processes.** One owner, one internal clipboard,
for all its windows. Two processes share an item when they share an
owner (the kernel stores it; the system clipboard carries a marker),
or through the private type on the system clipboard where the backend
supports it. Other applications see only the fallbacks.

**Agents** use the same steps: `clipboard.copy`, `clipboard.paste`
(with `format`, `transform`, `parts`, `if_rev`, `dry_run`; a dry run
answers the format chosen, the transforms, what would be stripped and
the changes, without making them), `clipboard.offers`, `clipboard.read`,
`clipboard.write`, and a `clipboard` collector on the debug bridge. An
agent's session has its own slot: it neither reads nor overwrites the
user's unless the host allows it.

**Survey.** egui and eframe carry text both ways and images out, no
formats, no primary selection, and a paste pushed as text on the key:
**keep** them for text inputs, **wrap** the rest. arboard (MIT or
Apache-2.0, already in the tree through egui-winit: text, HTML, images,
file lists, X11's primary selection, no private types) and
smithay-clipboard (MIT, already in the tree: Wayland text and primary
selection) are **adopted** behind egui-pop's system backend.
wl-clipboard-rs (any type, but only with the data-control protocol)
waits; copypasta (text only) is rejected. No crate has the model
(typed items, negotiation, transforms, boundaries): **own** it, and
own a small private-type backend later.

## Consequences

- Copy and paste of rich content (nodes, colours, tokens, rows, and an
  application's own types) with one rule for every component; text
  fields unchanged.
- What leaves the owner is stripped by rules the application declares,
  and the user is told what was left out.
- A host adapts pasted content to its target in one place (transforms)
  instead of in every widget.
- Agents copy and paste with the user's refusals and a dry run, without
  touching the user's clipboard.
- The keymap cannot rebind Ctrl+C, Ctrl+X, Ctrl+V while egui-winit
  swallows them; that limit goes to `docs/upstream.md`.
- Private types across processes without a shared owner wait for a
  backend per platform; until then such a paste falls back to text.

## Answers to the open questions (2026-10-21)

The owner accepted the proposed defaults (the analysis gives the
reasons):

1. **System clipboard on every copy:** yes, the fallbacks are written
   on every copy; a type may declare itself internal only.
2. **Private type or marker:** a marker first (a change counter, else
   a hash of the fallback); the private type later, where the platform
   allows it.
3. **Clipboard history:** none; one item per slot.
4. **Agents reading the user's clipboard:** refused, unless the host
   allows it per agent.
5. **External HTML on paste:** reduced to text, unless the target
   accepts HTML; then sanitised.
6. **Primary selection:** opt-in, for text inputs and the code editor
   only.
7. **Lossy transforms:** applied, with a toast naming the loss; paste
   special shows it beforehand.
8. **Paste with no target that accepts it:** refused with the reason,
   the command greyed out; no guessed target.

## Done ahead of the steps

The fixes of step 1 found while reading the code: a cut in the code
editor copies only once its delete is accepted; the terminal removes
the bracket's markers from a bracketed paste; Ctrl+Shift+C in the
terminal never interrupts; `docs/editors.md` says Shift+Insert pastes
on Windows only. The `Clipboard` contract itself is step 2.

## Built (2026-10-21)

Steps 2 and 3: the model in `egui-pop-model::clipboard`, the
`Clipboard` contract with `LocalClipboard` behind `Host::clipboard`,
history groups (`Host::begin_group`, `Host::end_group`), endpoints
(`ClipSource`, `ClipTarget`), plugin-declared types, transforms and
sanitisers, the `clipboard.copy` and `clipboard.paste` steps, the four
Edit commands (dispatched from egui's events too), the agents'
`clipboard.*` with a dry run, and the `clipboard` collector; a gallery
page. The guide is [docs/clipboard.md](../clipboard.md). Left: the
first types and their sanitisers (step 4), and the system backend for
rich formats, private types and the primary selection (step 5).
