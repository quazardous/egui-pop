# The clipboard

egui-pop copies and pastes typed content, not only text: a colour, a
set of nodes, a theme token, an application's own types. The decision
and its reasons are [ADR 0015](adr/0015-clipboard.md); this page is how
to use it. Everything below is `egui_pop::api::clipboard` (the
contract, the plugin) and `egui_pop::model::clipboard` (formats, items,
negotiation, no egui).

## The model

- **The owner** keeps the items: one per **slot**, the user's and each
  agent session's own. It is the application's kernel when the host
  says so (`Host::clipboard`), else egui-pop's `LocalClipboard`, kept in
  memory by `ClipboardPlugin` and shared by the application's windows.
  Each `put` raises its revision and publishes `clipboard.changed` (the
  slot, the type, the revision, never the payload).
- **An item** (`ClipItem`) has a type (a private format,
  `application/x-<app>.<type>+json`), a version, a JSON payload, a
  title ("a colour", "3 nodes"), readable **fallbacks** (text always,
  HTML when the type has one), where it came from, and what sanitisers
  left out.
- **The system clipboard** receives the user's copies' fallbacks
  (text), so a colour lands in a chat as `#d9a441`. A paste whose text
  is still what the owner wrote there pastes the owner's item; other
  text is external and pasted as text. A type declared internal
  (`ClipType::internal`) never reaches the system clipboard.
- **A target** accepts formats, richest first. A paste takes the first
  the item offers, or one a declared transform makes from what it
  offers (one transform, never a chain); nothing fits: the paste is
  refused with the reason, and Edit > Paste is greyed.

## A component: a source and a target

A component registers an **endpoint** by name with a `ClipSource`
(what a copy takes; a cut's delete) and a `ClipTarget` (the formats it
accepts; how it applies a paste), then says each pass which widget
draws it. While that widget has the focus, the Edit commands use the
endpoint.

```rust,ignore
use egui_pop::api::clipboard::{self, ClipItem, ClipSource, ClipTarget, Cut, Paste, TEXT};

impl ClipSource for Swatches {
    fn copy(&self, _: &AppHandle) -> Option<ClipItem> {
        let c = self.selected()?;
        Some(ClipItem::new(COLOR, 1, json!({ "hex": c }), "a colour")
            .fallback(TEXT, json!(c)))
    }
    fn can_cut(&self, _: &AppHandle) -> bool { !self.locked() }
    fn delete(&self, _: &AppHandle, cut: &Cut<'_>) -> Result<(), String> {
        self.remove_selected(cut.group) // its writes carry the group
    }
}

impl ClipTarget for Swatches {
    fn accepts(&self, _: &AppHandle) -> Vec<FormatId> { vec![COLOR, TEXT] }
    fn paste(&self, _: &AppHandle, p: &Paste<'_>) -> Result<Json, String> {
        let hex = p.repr.data["hex"].as_str().unwrap_or_default();
        if !p.dry_run {
            self.add(hex, p.group);
        }
        Ok(json!(format!("adds {hex}"))) // paste special's preview
    }
}

// In `connect`: owned by the plugin, gone with it.
ctx.clip_endpoint("demo.swatches", Some(swatches.clone()), Some(swatches.clone()));
// Each pass, where it is drawn:
let response = draw_swatches(ui);
clipboard::bind(app, "demo.swatches", response.id);
```

In a node graph, a tree or a list of sliders, the focus sits on each
node, row or slider, not on the panel. Such a component binds its
elements, or an area, instead of one widget:

```rust,ignore
// Each element, with its key: the source and the target receive it.
for node in &graph.nodes {
    let response = draw_node(ui, node);
    clipboard::bind_element(app, "demo.graph", response.id, &node.key);
}
// Or any focused widget whose centre lies in the canvas.
clipboard::bind_area(app, "demo.graph", canvas.rect);
```

`ClipSource::copy_element`, `Cut::element` and `Paste::element` carry
the focused element's key (`clipboard::element` says it); an agent
names one with `element` in `clipboard.copy` and `clipboard.paste`. A
widget bound alone wins over an area, and of two areas holding the
focused widget the smaller one. The focused widget's rectangle is read
from egui's lists of widgets: `Context::read_response` is never used
for it, as building a response for a widget that is not hovered during
a press makes egui surrender that widget's focus (a host finding the
focused row that way steals the tree's focus on the click that gave
it).

A paste is **one undo step** ("Paste a colour"): egui-pop opens a
history group (`Host::begin_group`, the store's by default) and the
target's writes carry it (`Set::gesture(paste.group)`). A cut is the
copy and the delete in one step, and **copies only once its delete is
accepted**: a refused delete copies nothing. A dry run (`p.dry_run`)
says what would change and changes nothing: paste special shows it as
the preview, an agent reads it as the plan.

Writes a source or a target makes are its plugin's own (the scope of
the plugin that registered the endpoint).

## Types, transforms and sanitisers

Plugins declare them in `connect`, and they go with the plugin:

```rust,ignore
ctx.clip_type(
    ClipType::new(COLOR, 2, "a colour")
        .fallbacks(|p| vec![Repr::new(TEXT, p["hex"].clone())])
        .parts(|_| vec![Part::new("alpha", "the opacity")])
        .migrate(|from, old| match from {
            1 => Ok(json!({ "hex": old["rgb"] })),
            v => Err(format!("no colour of version {v}")),
        }),
)?
.clip_transform(Transform::new(
    TransformSpec::new("demo.parse-colour", TEXT, COLOR),
    |r| parse(r.text().unwrap_or_default()).map(Transformed::exact),
))?
.clip_sanitiser(
    Sanitiser::new("demo.paths", Boundary::System, |data| strip_paths(data))
        .format(TEXT),
)?;
```

- A **type**'s title ("a colour") is a declaration, translated under
  `clip-<format>.title` (`clipboard::type_title_key`) and listed by the
  key extractor; an item's own title ("3 nodes") is translated when its
  source gives it as a message, `ClipItem::title_key("demo-nodes",
  &[("count", "3")])`, its English `title` the fallback. The undo
  step's label ("Paste a colour") and paste special's formats use them.
- A **type** renders its fallbacks for items whose source gave none,
  lists its parts (paste special offers each alone), and migrates a
  payload made at an older version before it is pasted.
- A **transform** makes one format from another; `TransformSpec::lossy`
  says it loses information: it is applied, and a toast names the loss
  (`Transformed::lossy(data, "the hue")`); paste special shows it
  beforehand. `Transform::when` limits it to some representations or
  targets. `Transform::for_target` adapts to the target: it receives
  the target's name, the element pasted into and what the target says
  of itself (`ClipTarget::context`: the image a mask lands on, the
  range a value is clamped to, the names already taken), so a mask
  scaled to the image, a value clamped or a name made unique is a
  transform, its loss shown in the toast and paste special's preview,
  rather than an adaptation hidden in `ClipTarget::paste`.
- A **sanitiser** runs on each representation crossing its boundary:
  `Boundary::System` (what leaves for other applications: local paths,
  secrets, owner-local ids), `Boundary::External` (what comes in),
  `Boundary::Owner` (inside, nothing by default). It returns what it
  removed; the item records it and the copy's toast says it ("Copied;
  left out: a local path").

egui-pop declares two of its own: `egui_pop.controls` removes control
characters and escape sequences from external text, and
`egui_pop.html-to-text` (lossy) reduces HTML to text for a target that
does not take HTML.

## Steps

Copy and paste are steps (ADR 0011), observed, vetoed, redirected or
replaced like any other:

| Step | Arguments | A before-hook may |
|---|---|---|
| `clipboard.copy` | `source`, `slot`, `cut`, `dry_run`, `origin` | veto (a secret field), redirect (another selection, another slot) |
| `clipboard.paste` | `target`, `slot`, `format`, `transform`, `parts`, `if_rev`, `dry_run`, `text`, `origin` | veto, redirect (another target, another format) |

Their results say the type, the plan (`format`, `to`, `transform`,
`lossy`), what was `lost` and `stripped`, and the target's `changes`.
`clipboard::copy` and `clipboard::paste` run them from code; a refusal
or a veto asked by the user shows in a toast.

## Commands and keys

| Command | Keys | What |
|---|---|---|
| `edit.cut` | Ctrl+X, Shift+Delete | cut the focused endpoint's selection |
| `edit.copy` | Ctrl+C, Ctrl+Insert | copy it |
| `edit.paste` | Ctrl+V, Shift+Insert | paste in the richest format it accepts |
| `edit.pasteSpecial` | Ctrl+Shift+V | choose the format, transform or part, each with its preview |

They sit in the Edit menu under undo (group `1_clipboard`), with
`when` `!textInputFocus` and the keys `canCut`, `canCopy`, `canPaste`:
greyed when nothing focused answers. Ctrl is Command on macOS. Text
fields keep egui's own copy and paste, as they keep their own undo; the
code editor and the terminal keep theirs too.

egui-winit turns the platform's copy, cut and paste chords into events
and swallows the keys (see [upstream](upstream.md)); while an endpoint
has the focus, those events run the commands (Shift held on a paste:
paste special), so they work under winit as under the keymap.

## Agents

Agents go through the same steps, with the user's refusals:

- `clipboard.copy {source, cut, dry_run}`;
- `clipboard.paste {target, format, transform, parts, if_rev,
  dry_run}`: a dry run answers the plan, the transform, what would be
  stripped and lost, and the target's changes, without making them;
- `clipboard.offers {target}`: what the slot holds and every way it
  fits the target, each with its preview;
- `clipboard.read`, `clipboard.write {format, data, title}`.

Each works in the session's own slot (`agent:<name>`): an agent neither
reads nor overwrites what the user copied, unless the owner allows it
(`Clipboard::may_use`; with `LocalClipboard`,
`clipboard::allow_session`). The `clipboard` collector of the debug
bridge shows the owner, its revision, each slot's item (without its
payload), the endpoints and what plugins declared.

## Not yet

- What the system clipboard holds is read only when the paste key
  brings it (egui reads it as text, on the key): Edit > Paste from the
  menu pastes the owner's item.
- Rich formats from other applications (HTML, images, files), private
  types across processes without a shared owner, and the **primary
  selection** (select to copy, middle-click to paste; opt-in, for text
  inputs and the code editor) wait for egui-pop's own system backend
  over arboard and smithay-clipboard (ADR 0015, step 5).
- The first types (colours, graph nodes, theme tokens, table rows) and
  their sanitisers come next (ADR 0015, step 4).
