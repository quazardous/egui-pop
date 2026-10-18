# 0015, appendix: the analysis behind the clipboard

The analysis that led to [ADR 0015](0015-clipboard.md), kept for the
reasons it gives. Where it differs from the ADR, the ADR wins. Helios,
the application egui-pop is built for, has its own note on the kernel
side (its `docs/design/clipboard.md`); this appendix summarises what it
needs.

## What exists today

Read from the code on `main` (2026-10-18), egui, eframe and egui-winit
0.36.2.

### In egui-pop

Two widgets touch the clipboard themselves, the code editor and the
terminal; both handle plain text only, both through egui
(`ctx.copy_text` out, `Event::Copy`, `Event::Cut`, `Event::Paste` in).
There is no clipboard command, no Edit menu entry for it, no clipboard
step, no `Host` part and no internal buffer. Nothing uses `arboard`
directly.

| Site | Copies or pastes | Format | How | Step | Gaps |
|---|---|---|---|---|---|
| Code editor, copy (`egui-pop-editors/src/view.rs:1065`, `copy_text` at :1115) | the selection, or the cursor's line and a newline when nothing is selected | plain text | `ctx.copy_text` | none | no step, no signal, nothing can see or veto it |
| Code editor, cut (`view.rs:1069`, `cut_span` at :1129) | copies as above, then deletes through `edit(.., EditKind::Cut)` | plain text | `copy_text`, then the `edit.change` step | the delete is `edit.change {kind: cut}`, undoable | **bug:** the clipboard is written before the step runs, so a vetoed `edit.change`, a refused edit or a read-only editor (`edit()` returns false at :994) still copies, and the user sees a cut that deleted nothing |
| Code editor, paste (`view.rs:1082`) | replaces the selection, `\r\n` becomes `\n` | plain text | `Event::Paste` → `edit(.., EditKind::Paste)` | `edit.change {kind: paste}`, vetoable, redirectable, undoable | fine; but a multi-character `Event::Text` is also classed as a paste (`view.rs:1035`), so some input methods are labelled "paste" |
| Terminal, paste (`terminal/view.rs:294`) | the text, newlines as `\r` | plain text | `Event::Paste` → `terminal.input {kind: paste}` | vetoable, rewritable | **bug:** `write_default` (`terminal/state.rs:679`) wraps a paste in `ESC[200~ … ESC[201~` when the program asked for bracketed paste, but does not remove an `ESC[201~` (or any escape) inside the text: pasted text can end the bracket early and type commands |
| Terminal, copy (`terminal/view.rs:298`, `copy()` at :430) | the selection; with none, sends `^C` | plain text | `ctx.copy_text`, `TerminalSignal::Copied` | copy: none; `^C`: `terminal.input` | **bug:** Ctrl+Shift+C is meant to copy only (`view.rs:309`), but under winit it arrives as `Event::Copy` (see below), so with no selection it interrupts the program; only kittest, which sends key events, reaches the `Key::C` branch (`tests/terminal.rs:137`) |
| Terminal, cut (`terminal/view.rs:302`) | sends `^X` | n/a | `terminal.input` | yes | intended |
| Terminal, `terminal.copy` command (`terminal/steps.rs:44`, :321) | the selection | plain text | `AppHandle::egui().copy_text` | a command, no key, no menu | the only clipboard verb an agent has; there is no paste command |
| Terminal docs (`docs/editors.md:447`) | "Ctrl+V, Shift+Insert" paste | | | | **doc bug:** egui-winit maps Shift+Insert to paste on Windows only |
| Node graph (`egui-pop-viewport/src/graph/mod.rs:1950`) | no copy or paste; Duplicate (Ctrl+D) emits `graph.add` with `copy_of` (`egui-pop-imaging/src/graph.rs:11`) | n/a | | `graph.add` | nodes cannot be copied to another graph, window or application |
| Theme editor (`egui-pop-api/src/theme_editor.rs:453`) | import from text pasted into a text field, then `theme.import` / `theme.load` | JSON text | egui's text field | import is a step | no "copy token", no "copy colour" |
| Colour picker, swatches, wheels (`egui-pop-core/src/param.rs`, `egui-pop-viewport/src/wheel.rs`) | nothing | | | | no copy of a colour, no paste of one |
| Table, list, tree (`egui-pop-views`) | nothing; the library binds X to Reject (`views/src/library/marks.rs:231`) | | | | no copy of rows or cells |
| egui text fields (about 25 `TextEdit` uses: palette, files, dialogs, settings, find bar, tags, filters…) | egui's own copy, cut, paste | plain text | egui | none | fine as they are: text inputs keep their own, as for undo |
| Edit menu (`egui-pop-api/src/app.rs:2232`) | `edit.undo`, `edit.redo` only, group `0_undo` | | | | no `edit.cut`, `edit.copy`, `edit.paste` |
| Debug bridge, collectors | nothing | | | | an agent cannot see what is on the clipboard |
| `tools/wbox/config.yaml:10` | `clipboard_bridge: false` | | | | scripted runs in the nested compositor cannot test the real clipboard |
| `Icon::Copy` (`egui-pop-core/src/icon.rs:178`) | exists | | | | used by no command |

### How the keys reach egui-pop

egui-winit (`src/lib.rs:1016` in 0.36.2) turns a chord into
`Event::Cut`, `Event::Copy` or `Event::Paste(text)` and **returns
without sending the key**: copy is the Copy key, Command+C (whatever
Shift does) or Ctrl+Insert on Windows; paste is the Paste key,
Command+V or Shift+Insert on Windows; cut is the Cut key, Command+X or
Shift+Delete on Windows. Three consequences:

1. A command bound to Ctrl+C, Ctrl+X or Ctrl+V (or Ctrl+Shift+V) in
   egui-pop's keymap never fires under winit: no key event arrives. The
   clipboard commands must be dispatched from the three events, with
   the modifiers held at that moment.
2. A paste reads the system clipboard **synchronously, as text only**,
   inside egui-winit, before any application code: a plugin cannot
   choose a richer format, and a slow clipboard owner (another
   application on Wayland) stalls the frame.
3. Copy and cut carry no payload: the focused widget decides what to
   copy, and nothing else can.

### What eframe brings

eframe 0.36 always enables egui-winit's `clipboard` feature off the
web: `arboard` 3.6.1 (with `image-data`; X11 through x11rb, Windows,
macOS) and `smithay-clipboard` 0.7.3 on Wayland (`egui-winit
src/clipboard.rs`). On Wayland, text goes through smithay-clipboard
with the window's display; an image (`ctx.copy_image`) goes through
arboard, which without its `wayland-data-control` feature (off) talks
to XWayland, so it works only when XWayland runs. When neither backend
starts, a string inside the process stands in. Nothing reads an image,
HTML or a file list on paste, and nothing reads or writes the primary
selection (middle-click paste). `deny.toml:37` already accepts
`clipboard-win` (BSL-1.0), arboard's Windows backend.

### In Helios (summary)

From its `docs/design/clipboard.md`:

- Helios copies two strings today (a parameter's path and its value
  as JSON, from the parameter's context menu) and pastes nothing. What
  a user would copy is typed and rich: a module's parameters (all or some), mask
  shapes and groups, a sub-network, a colour, library images (as
  references or as files), styles and presets, workspace parts.
- darktable, its engine, has a history copy and a selective paste
  (append or overwrite), styles, and a path that upgrades old module
  parameters; Helios will reuse that path as a paste transform.
- Its workspace parts import is already a copy with a review (decided
  2026-10-13: importing is copying, with no provenance), and its "Save
  as template" already strips absolute paths, secrets and embedded data:
  the same rules a copy leaving the vault needs.
- Its agents and its headless node edit through the same commands as
  the user and need copy and paste without a desktop clipboard.

### What the inventory says

1. The only rich content egui-pop could put on a clipboard (nodes,
   colours, tokens, rows) has no copy at all; what is copied is text.
2. The key path is egui-winit's, not the keymap's: Ctrl+C, Ctrl+X and
   Ctrl+V cannot be commands as Ctrl+Z is, without dispatching from
   egui's events.
3. Nothing between the widget and the system clipboard can inspect,
   strip, transform, veto or log a copy or a paste.
4. Three small bugs (the editor's cut before its veto, the terminal's
   unsanitised bracketed paste, Ctrl+Shift+C interrupting) and one doc
   error.

## Survey

### egui and eframe

- **Text** both ways, through `ctx.copy_text` and `Event::Paste`;
  **images** out only (`ctx.copy_image`, egui 0.30 and later), through
  arboard, which on Wayland needs XWayland in eframe's configuration.
- **No formats:** no HTML, no file list, no custom type, no way to
  offer several representations of one copy.
- **No primary selection** on X11 or Wayland (selecting text does not
  set it, middle-click does not paste it).
- **Paste is pushed, not pulled:** egui-winit reads text on the key,
  synchronously; a widget cannot ask for another format.
- Verdict: **keep** for text inputs (they keep their own copy and
  paste, as they keep their own undo), **wrap** for the rest: egui-pop
  dispatches its commands from the three events and reads and writes
  the richer formats itself. The missing pieces (rich paste, a pull
  model, primary selection) go to `docs/upstream.md`, with issues
  filed upstream after a search for existing ones.

### Crates

| Crate | Licence | Does | Lacks | Verdict |
|---|---|---|---|---|
| arboard 3.6 (already in the tree through egui-winit) | MIT or Apache-2.0 | text, HTML with a text alternative, RGBA images, file lists (read and write); X11 primary and secondary selections; `exclude_from_history` for clipboard managers; Windows, macOS, X11; Wayland with `wayland-data-control` | arbitrary (private) types; Wayland without the data-control protocol, which not every compositor offers; on Linux the data dies with the process unless a clipboard manager keeps it | **adopt**, behind egui-pop's system backend, for every standard format |
| smithay-clipboard 0.7 (already in the tree) | MIT | Wayland clipboard and primary selection through the window's own connection (no extra protocol) | text only (`text/plain;charset=utf-8`, `UTF8_STRING`, `text/plain`) | **adopt** for text and the primary selection on Wayland, as egui-winit does |
| wl-clipboard-rs | MIT or Apache-2.0 | any MIME type on Wayland, reading and serving | needs the data-control protocol (the same gap as arboard's feature), runs outside the window | **not now**; the custom-type backend may use it where the protocol exists |
| copypasta (Alacritty's) | MIT or Apache-2.0 | text, X11 and Wayland, primary selection | text only; nothing over arboard and smithay-clipboard | **reject** |
| x11rb, the Wayland data-device protocol, Windows `RegisterClipboardFormat`, macOS `NSPasteboard` | permissive | private types per platform | a backend to write per platform | **own**, later and small, only for private types across processes (step 5) |

No crate offers the model itself (an internal typed clipboard, several
representations, negotiation, transforms, hooks); **own** it, as the
context stack and undo are owned.

### Prior art

**VS Code.** A copy in the editor puts plain text, HTML (the text with
its colours) and a private `vscode-editor-data` entry (the language,
whether it was a whole line, the multi-cursor split) on the clipboard;
a paste into VS Code reads the private entry and restores what text
lost (a whole-line copy pastes above the line; three cursors receive
one line each). Extensions register paste providers per MIME type:
pasting an image into Markdown can write the file and insert a link,
pasting a URL can make a link; after a paste a small widget offers the
other ways ("Paste as…"), and a preference picks the default per kind.
Providers also run on copy to stash metadata for the paste.

**Browsers.** A copy is a `ClipboardItem` carrying several types
(`text/plain`, `text/html`, `image/png`, and "web custom formats" for
private types, which the browser keeps apart from the system's own
names); a paste reads the item and picks the type it can use. Pasted
HTML is sanitised (scripts, event handlers and foreign styles removed)
before a page sees it. Reading the clipboard without a user gesture
asks for permission. Copy and paste are events a page can intercept
and rewrite.

**GIMP.** Copy, Copy Visible, Cut; Paste (a floating selection to
anchor), Paste In Place, Paste Into Selection, Paste as New Layer, as
New Image; **named buffers** keep several copies at once. It exchanges
images with other applications as PNG and other types; a paste is one
undo group ("Paste").

**Krita.** Copies layers in its own format besides a flat image; on a
paste whose clipboard holds both Krita's own data and an image from
elsewhere, a preference (ask, the web image, the local data) decides.
Paste at cursor, Paste into active layer, Paste as new layer; Copy
Merged; layer styles copy and paste separately.

**Blender.** Ctrl+C and Ctrl+V over any field copy and paste its value
as text (a colour as a list, a vector), and paste can fill a whole
array; Ctrl+Alt+C copies a whole vector or colour. Objects copy through
a file in the temporary directory, so a paste works in another
Blender process; node selections copy inside the session. Pose copy
has **Paste X-Flipped**: a paste that transforms the content for its
target. Alt over a field edits it on every selected object.

**Lightroom Classic.** Copy Settings opens a dialog of checkboxes
(white balance, exposure, crop, masks, each group) whose last choice
is remembered; Paste Settings applies it; Paste from Previous takes
the previous photo's; **Sync** applies the chosen settings to every
selected photo, as one undo step. Masks synced to other photos are
recomputed there (a subject mask finds that photo's subject); a crop
synced keeps its aspect ratio; white balance "as shot" stays relative.
These are transforms on paste in all but name.

**darktable.** In the darkroom and the lighttable: copy the history
stack (Ctrl+C), copy parts of it (Ctrl+Shift+C, a dialog of modules,
some unticked by default because they belong to one image), paste
(Ctrl+V) or paste parts (Ctrl+Shift+V), in **append** or
**overwrite** mode (a preference, also in the dialog); one source,
many targets. Masks travel with their modules in normalised
coordinates. Styles are saved partial histories applied the same way.
A module's old parameters are upgraded to its current version when
read. Copy and paste reach the history stack only, never the library
fields or the GUI.

### What carries over

- **One copy, several representations, the target picks** (VS Code,
  browsers, Krita): a rich private one, readable fallbacks for
  everyone else.
- **Paste special is a choice of representation or of parts**
  (VS Code's "Paste as", GIMP's paste variants, darktable and
  Lightroom's selective paste).
- **Transforms on paste exist everywhere under other names**
  (Lightroom's recomputed masks, Blender's X-flip, darktable's
  parameter upgrade, VS Code's paste providers).
- **Sanitising is a boundary rule** (browsers on paste, Helios's "Save
  as template" on export): content crossing out of a trust zone is
  stripped.
- **A paste is one undo step** (GIMP's "Paste" group, Lightroom's
  sync).
- **Cross-process works by a shared store** (Blender's file in the
  temporary directory) or by private types on the system clipboard
  (VS Code, browsers, Krita).
- **Several buffers** (GIMP's named buffers) are rare and expert; a
  short history of recent copies is what most users meet (clipboard
  managers).

## The questions, answered

### 1. Internal and system clipboards

**Recommendation:** two layers. The **internal clipboard** holds the
last copy as a typed item (its type id, its version, the payload as
structured data, its source: origin, window, place, and the kernel
it came from), owned by the `Clipboard` owner and shared by every
window of that owner. The **system clipboard** receives, on each copy,
the item's readable fallbacks (text, and HTML or an image when the
type renders one) and, where the platform allows it, the item itself
under a private type. A paste first asks whether the system clipboard
still holds what the last internal copy put there (a change counter
where the platform has one, else a hash of the fallback text written):
if so the internal item is pasted, otherwise the system's content is
external and pasted by its own formats.

Why not internal only (Blender's node copy): a colour or a parameter
set copied should land in a chat or a text file as readable text.
Why not system only: private types are not portable today
(smithay-clipboard is text only, arboard has none), and rich items
carry ids that must never leave the kernel.

### 2. Formats and negotiation

- A **format** is a MIME-like id: private types are
  `application/x-<app>.<type>+json` (egui-pop's own are
  `application/x-egui-pop.<type>+json`), with a version inside the
  payload; standard ones are `text/plain`, `text/html`, `image/png`,
  `text/uri-list`.
- A copy produces an **offer**: the type, then its fallbacks in order,
  each rendered lazily (an image is made only if asked).
- A **target** declares the formats it accepts, richest first. A paste
  takes the first format of the target's list that the offer has, or
  that a declared transform can make from one it has (one transform
  by default; chains are not searched). Nothing found: the paste is
  refused with the reason ("A colour cannot be pasted here"), never a
  silent no-op (ADR 0003).
- Paste special lists every format and transform that would work, with
  a preview where the target gives one, and the parts when the type
  has parts (a parameter set's modules).

### 3. Hooks

- **On copy**, the `clipboard.copy` step: its before-hooks may veto
  ("this field is a secret") or redirect (copy another selection), and
  **sanitisers** run on each representation by the **boundary** it
  crosses: inside the same owner, nothing is removed; to the system
  clipboard, local paths, secrets and owner-local ids are removed from
  the fallbacks and from the private payload. A sanitiser says what it
  removed; the toast says it ("Copied; 2 local paths left out").
- **On paste**, the `clipboard.paste` step: before-hooks may veto or
  redirect (another target, another format); **transforms** adapt the
  chosen representation to the target (a parameter set to the
  target's module version, a shape to the target's size, a colour to
  the target's space, a name made unique); external content is
  **sanitised** first (HTML stripped to its text or allowed tags,
  control characters out of text pasted into a terminal).
- **Plugins declare** types (id, version, schema, the fallbacks they
  render, a migration from older versions) and transforms (from, to,
  when they apply, lossy or not) in their registry, and own them (ADR
  0004).
- A paste is **one undo step**, made under a history group (ADR 0014)
  labelled by the type ("Paste 3 nodes"); a cut is the copy plus a
  delete, one step, and copies only when the delete is accepted. A
  copy is never an undo step: it changes no document.

### 4. Commands and keys

- `edit.cut`, `edit.copy`, `edit.paste`, `edit.pasteSpecial` in the
  Edit menu (group `1_clipboard`, under undo), with `when`
  `!textInputFocus` so text inputs keep their own; they are dispatched
  from egui's `Event::Cut`, `Event::Copy`, `Event::Paste` (Ctrl+Shift+V
  read from the modifiers held then), and from the keymap for every
  other binding.
- The **focused component** answers what a copy takes and where a
  paste goes (a `ClipSource` and a `ClipTarget` it registers, as a
  local undo scope is registered); with nothing focused that answers,
  the commands are disabled (`canCopy`, `canPaste` keys, as `canUndo`).

### 5. Across windows and processes

- Windows of one application share one owner, so one internal
  clipboard.
- Two processes of one application share it when their owner is shared
  (a kernel serving both: the item is stored by the kernel, the system
  clipboard carries its id as a private type or as a marker beside the
  fallback). Without a shared owner, the private type on the system
  clipboard is the only way, when the platform backend supports it.
- Another application only ever sees the fallbacks.

### 6. Agents

- The same steps with the same refusals: `clipboard.copy {source,
  selection?, dry_run}`, `clipboard.paste {target, format?, transform?,
  parts?, if_rev, dry_run}`, `clipboard.offers`, `clipboard.read`,
  `clipboard.write`.
- `dry_run` answers the plan: the format chosen, the transforms that
  would run, what a sanitiser would remove, and the changes the paste
  would make, without making them.
- An agent's session has its **own slot** in the internal clipboard by
  default: it never reads what the user copied (which may be a
  password from another application) and never overwrites it, unless
  the host lets it (`clipboard.read {slot: "user"}` refused by
  default, with the reason).

### 7. Primary selection

X11 and Wayland users expect select-to-copy and middle-click paste in
text. egui has neither. **Recommended:** opt in, text inputs and the
code editor only, through the system backend (arboard's primary kind
on X11, smithay-clipboard's on Wayland), and an upstream issue for
egui to carry it.

## The contract

Sketch (names to settle when built):

```rust
// egui-pop-model::clipboard (pure, no egui)
pub struct FormatId(String);                     // "application/x-egui-pop.color+json", "text/plain"
pub struct ClipItem {
    pub kind: FormatId,                          // the private type
    pub version: u32,
    pub payload: serde_json::Value,
    pub source: ClipSourceInfo,                  // origin, window, place (ADR 0012), owner id
    pub stripped: Vec<Stripped>,                 // what sanitisers removed, and why
}
pub enum Boundary { Owner, System }              // where a representation goes
pub struct TransformSpec { pub from: FormatId, pub to: FormatId, pub lossy: bool }
pub fn negotiate(offer: &[FormatId], accepts: &[FormatId], transforms: &[TransformSpec])
    -> Result<Plan, Refusal>;                    // the format, the transform, or why not

// egui-pop-api::clipboard
pub trait Clipboard {
    fn put(&mut self, slot: &Slot, item: ClipItem, origin: &Origin) -> Answer;
    fn get(&self, slot: &Slot) -> Option<ClipItem>;
    fn offers(&self, slot: &Slot) -> Vec<FormatId>;   // internal, or what the system holds
    fn rev(&self) -> u64;
}
pub trait ClipSource { fn copy(&mut self, ctx: &CopyCtx) -> Option<ClipItem>; fn cut(&mut self) -> Option<Delete>; }
pub trait ClipTarget { fn accepts(&self) -> Vec<FormatId>; fn paste(&mut self, item: Pasted, plan: &Plan) -> Answer; }

// Host (ADR 0010)
fn clipboard(&mut self) -> Option<&mut dyn Clipboard> { None }  // None: LocalClipboard
```

`LocalClipboard` keeps one item per slot in memory and writes the
fallbacks through the system backend. `Answer` is the same as the
context stack's and history's: done, refused with a reason, or later.
The owner publishes `clipboard.changed {slot, kind, rev}` on the
journal (never the payload).

## Flows

**O** is the owner, **UI** egui-pop's projection.

### A colour copied to a chat, and back

```
user : focus on a colour swatch, Ctrl+C (Event::Copy)
UI   : edit.copy → clipboard.copy step → the swatch's ClipSource
O    : item color {srgb, space, value}; system: "#d9a441" as text
user : pastes in a chat → "#d9a441"
user : copies "rgb(12, 40, 200)" from a web page, focus on a swatch, Ctrl+V
UI   : the system holds text not written by O → external
O    : the swatch accepts [color, text/plain]; transform text→color parses
       it; one undo step "Paste colour"
```

### Nodes copied from one window to another

```
user : selects three nodes in window A, Ctrl+C
O    : item graph.nodes {nodes, edges between them, values}; system:
       a readable summary as text ("3 nodes: blur, mask, mix")
user : window B, Ctrl+V on its graph
O    : the system still holds O's text → the internal item
UI   : clipboard.paste step; transform "rename on clash"; one step
       "Paste 3 nodes"; Ctrl+Z removes them together
```

### A cut that is vetoed

```
user : selects a read-only node, Ctrl+X
UI   : edit.cut → the graph's delete is vetoed ("locked")
O    : nothing copied, nothing deleted; toast with the reason
```

### Paste special

```
user : Ctrl+Shift+V on a parameter panel holding a parameter-set item
UI   : the dialog lists the parts (modules) and the formats; the
       target's preview on the viewport
user : unticks two modules, confirms → clipboard.paste {parts}; one step
```

### An agent pasting with a dry run

```
agent: clipboard.copy {source: "graph", selection: [n1, n2]}  → its slot
agent: clipboard.paste {target: "graph@B", dry_run: true}
O    : plan: format graph.nodes, transform rename (n1 → n1_2), 2 adds
agent: clipboard.paste {target: "graph@B", if_rev: 41} → one step,
       origin agent
```

## Open questions, with the defaults proposed

1. **System clipboard on every copy, or on demand?** Default: every
   copy writes the fallbacks; a type may say "internal only" (a
   secret, a huge item).
2. **Private type on the system clipboard** (step 5 needs a backend of
   our own), or a marker only? Default: a marker (change counter or
   hash) first; the private type later, where the platform allows.
3. **Clipboard history** (several recent items, GIMP's buffers)?
   Default: no; one item per slot. A history panel can come later.
4. **Agents reading the user's clipboard**: default refused; the host
   may allow it per agent.
5. **External HTML on paste**: default reduced to text unless the
   target accepts `text/html`, and then sanitised.
6. **Primary selection**: default opt-in, text only.
7. **Transforms that lose information** (a colour out of gamut, a
   parameter dropped by a newer module): default applied with a toast
   naming the loss; paste special shows it before.
8. **Paste with nothing focused that accepts it**: default refused with
   a reason, the command greyed out; no guess at a target.

## Steps

1. Fixes first: the editor's cut copies after its delete is accepted;
   the terminal strips escapes from a bracketed paste; Ctrl+Shift+C in
   the terminal never interrupts; `docs/editors.md` says Shift+Insert
   is Windows only. An entry in `docs/upstream.md` for egui-winit
   swallowing the keys.
2. `egui-pop-model::clipboard` (formats, items, negotiation, boundary
   rules) and the `Clipboard` contract with `LocalClipboard`;
   `Host::clipboard()`.
3. `edit.cut`, `edit.copy`, `edit.paste`, `edit.pasteSpecial`, the
   `clipboard.*` steps, dispatch from egui's events, `ClipSource` and
   `ClipTarget`, the undo group per paste.
4. First types: colour, graph nodes, theme tokens, table rows; the
   sanitisers; agent commands and a `clipboard` collector on the debug
   bridge.
5. The system backend for rich formats (images, HTML, files) and,
   where possible, private types; primary selection; wbox's clipboard
   bridge for tests.
