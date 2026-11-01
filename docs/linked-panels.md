# Linked panels

A selection in one view drives a preview in another panel: a list and
its details, a grid and a loupe, a graph and its parameters. egui-pop
has no global "the selection". Several independent selections coexist,
each one a value under a path the application names, because only the
application knows what a selection means.

| Who | Owns |
|---|---|
| egui-pop | How a view says what is selected, how a value reaches another panel, what happens when a panel moves, closes or is detached, the slow load's worker |
| The application | The path, the value's form (a name, an id, a list), which view writes it, what a preview shows |

The `linked-panels` page of the [gallery](gallery.md) is this recipe
running, and its test covers each case below.

## The recipe

1. The application declares one value per selection.
2. The source view writes it, after drawing, when the view's selection
   differs from what it last wrote. It writes with `untracked()`: a
   selection is not an undo step. It asks for a repaint, since the panel
   may have been drawn earlier in the frame.
3. The following panel reads the value in `ui()`.

```rust
use egui_pop::api::ValueWriter;
use egui_pop::core::latest::{Latest, Loaded};
use egui_pop::core::lock;
use egui_pop::egui;
use egui_pop::model::panels::{Area, PanelSpec};
use egui_pop::model::values::{Change, Set, Value};
use egui_pop::prelude::*;
use egui_pop::views::ViewsPlugin;
use egui_pop::views::list::{ListRows, ListState, ListView};

const PREVIEW: PanelId = PanelId::from_static("album.preview");
const SELECTED: &str = "album/selected";
const VIEWS: PluginId = egui_pop::views::steps::VIEWS;

struct Photos(Vec<String>);

impl ListRows<String> for Photos {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn key(&self, row: usize) -> String {
        self.0[row].clone()
    }
    fn label(&self, row: usize) -> String {
        self.0[row].clone()
    }
}

struct Album;

impl Plugin for Album {
    fn id(&self) -> PluginId {
        PluginId::from_static("album")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![VIEWS]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        lock(&ctx.app().store())
            .declare(ctx.owner(), SELECTED, Descriptor::text("The photo selected", ""))
            .map_err(|e| PluginError::Failed(e.to_string()))?;

        let writer = ctx.writer();
        let mut state = ListState::new();
        let mut rows = Photos(vec!["a.raw".into(), "b.raw".into()]);
        let mut written = String::new();
        ctx.story(Story::new(StoryId::from_static("album"), "Album", move |ui, app| {
            list(ui, app, &writer, &mut state, &mut rows, &mut written)
        }))?;

        let mut latest = Latest::new();
        ctx.panel(
            PanelSpec::new(PREVIEW, "Preview", Area::EndSide),
            move |ui, app| preview(ui, app, &mut latest),
        )?;
        Ok(())
    }
}

fn list(
    ui: &mut egui::Ui,
    app: &AppHandle,
    writer: &ValueWriter,
    state: &mut ListState<String>,
    rows: &mut Photos,
    written: &mut String,
) {
    let _ = ListView::new("album.photos").label("Photos").app(app).show(ui, state, rows);
    // After drawing, from the state: the list also loses rows that are gone.
    let now = state.selection().keys().first().cloned().unwrap_or_default();
    if now != *written {
        let write = Set::new(SELECTED, Change::To(Value::Text(now.clone())), Origin::User);
        let _ = writer.set(write.untracked());
        *written = now;
        ui.ctx().request_repaint();
    }
}

fn preview(ui: &mut egui::Ui, app: &AppHandle, latest: &mut Latest<String, String>) {
    let photo = lock(&app.store())
        .get(SELECTED)
        .and_then(|e| e.value.as_text())
        .unwrap_or_default()
        .to_string();
    let key = (!photo.is_empty()).then_some(&photo);
    let shown = latest.want(ui.ctx(), key, |name, _cancel| format!("The preview of {name}"));
    match shown {
        Loaded::Nothing => ui.weak("Nothing selected."),
        Loaded::Loading { .. } => ui.weak("Loading…"),
        Loaded::Ready(text) => ui.label(text.as_str()),
        Loaded::Failed(reason) => ui.label(reason),
    };
}

App::builder()
    .plugin(ViewsPlugin)
    .plugin(Album)
    .build(&egui::Context::default())
    .unwrap();
```

For a selection that also moves from elsewhere (another panel, an
agent), the source view reads the value before drawing and, when it
differs from what it last wrote, calls `set_selection`: the page in the
gallery does both. Several plugins writing one value need the owner to
name them ([who writes a value](state-or-event.md#who-writes-a-value)).

### Why compare in `ui()` here

[State or event](state-or-event.md) says to react to a value with
`Ctx::on_change` rather than by comparing with last frame's value. A
preview is the exception, on purpose: a listener runs whether the panel
is shown or not, so it would compute for a hidden panel; a comparison
in `ui()` runs only when the panel is drawn. Use `on_change` for what
must happen even with no panel on screen.

## When panels move

Nothing is needed from the application, because the panel holds no
copy of the selection: it reads the value when drawn.

| The panel is | What happens |
|---|---|
| Moved to another area | Its `ui()` runs from there; nothing changes |
| Detached into a window | Its `ui()` runs in that window's pass; `Latest` wakes the window drawing it; a page's `request_repaint` reaches it too, the window being an immediate viewport of the main one |
| Hidden behind another tab, closed | Its `ui()` does not run, so nothing is read or computed |
| Shown again | It reads the current value, and loads that alone |

A load running when its panel closes goes on to its end, since nothing
tells it: `ui()` is what notices. If the same key is still selected
when the panel comes back, it shows that result without a second load.

## Which views say what

| View | Where its selection lives | How it says it changed | How another panel reads it |
|---|---|---|---|
| `ListView` | `ListState` (the application's) | `list.select` step, `Signal::Select` | `state.selection()`, or the value the application writes |
| `DataTable` | `TableState` (the application's) | `table.select` step, `Signal::Select` | the same |
| `TreeView` | `TreeState` (the application's) | `tree.select` step, `Signal::Select` | the same; `pending_selection` for what waits for nodes to load |
| `ThumbGrid`, filmstrip | `LibraryState` (the application's) | `library.select` step, `Signal::Select` | the same, or from any plugin `Libraries::selection(id)` and `Libraries::cursor(id)` (the `LIBRARIES` service) |
| `GraphEditor` | `GraphState` (the application's) | `GraphEvent::Selection`; on the journal `graph.selection_changed` when drawn through `imaging::show_graph` | `state.selection`, or the value the application writes |
| `FilePicker` | The picker's `highlight` and `selected` | Nothing: no step, no signal | Its own `highlighted()` and `selected`; held by the `Files` service, `Files::highlighted(id)` |
| `Thumbnail`, `thumbnail_grid` | The caller's variable (plain widgets) | The response's click | The caller's variable |

The list, the table, the tree and the library share `select::Selection`,
which keeps keys, not row numbers. The keyboard code (`nav`) holds none
of its own. A text editor's or a terminal's selection is a range of
text, not this.

**Write from the state, not from the signals.** A step and a signal mean
someone asked for a selection. Three things change a view's selection
without either: rows that are gone leave it, `set_selection` runs no
step, and a tree takes a queued selection as its nodes load. A source
that writes only on `Signal::Select`, or from an after-hook on the step,
keeps previewing a row that was deleted. The library's registry has
this right already: it publishes from the state, whatever changed it.

Other ways to follow, for a panel that cannot reach the source's state:
an after-hook on the step (`Ctx::after`), or the journal
(`AppHandle::events_since`). Each sees only what ran as a step, as said
above.

## Slow previews

A preview that decodes a file cannot run in the frame, and the
selection moves faster than it loads. Three things go wrong by hand:
a slow early load arriving after a fast later one overwrites it, one
thread per row of a held arrow key, and work for a panel nobody sees.

`egui_pop::core::latest::Latest` handles them:

- `want(ctx, key, load)`, called each time the panel draws, starts
  `load` on a worker only when `key` changed, so the comparison with
  the last value is in it, and the panel keeps none of its own.
- One load runs at a time. A key asked while one runs waits; a later
  key takes its place, so it never starts. The running load is told
  through `Cancel::cancelled`, and its result is dropped.
- `Loaded::Loading { previous }` gives the last value, to show dimmed
  rather than a blank.
- A load that panics gives `Loaded::Failed` with the message, and the
  next key loads as usual.
- On the web there are no threads: the load runs in the frame that asks.

It holds one value, with no cache: keep one in `load` when rows are
visited again. For files, the picker's `Previews` service does all of
this with providers and a cache ([choosing files](files.md)).

## How others do it

| Practice | What it does | Here |
|---|---|---|
| [List/details](https://learn.microsoft.com/en-us/windows/apps/design/controls/list-details) (Windows; also the "master-detail" of other platforms) | Selecting in the list updates the details pane; side by side or stacked by window width | **Adopt** the pattern: the page and the test are this |
| [Qt's shared selection model](https://doc.qt.io/qt-6/model-view-programming.html) ("Sharing selections among views") | Two views given one `QItemSelectionModel` show one selection; it is a third object with its own signals | **Own, after it**: the shared object is a value in the store, not a model class. Our views keep their own `Selection`, since most links are one way, to a view that shows something else altogether |
| [Eclipse's selection service](https://www.eclipse.org/articles/Article-WorkbenchSelections/article.html) | Each window tracks the selection of the active part; listeners follow the active part or one part by id | **Rejected**: following "whatever part is active" is the global selection this design avoids. Its listeners by part id are what a named path gives |
| [VS Code's context keys](https://code.visualstudio.com/api/references/when-clause-contexts) | Named keys in a global table (`listFocus`, `listMultiSelection`, your own through `setContext`), read by `when` clauses | **Already here**: `AppHandle::set_context` and `when` clauses, for what enables a command. A key saying a selection exists fits; the selection's content belongs in the store |
| [tokio's `watch` channel](https://docs.rs/tokio/latest/tokio/sync/watch/index.html) | Keeps only the last value; each receiver knows whether it saw it | **Same idea**: the store's last value is the truth, and `Latest` is the receiver for a load |
| [poll-promise](https://docs.rs/poll-promise/latest/poll_promise/) (MIT or Apache 2.0) | A result polled each frame, made for immediate mode | **Own instead of wrap**: it holds one result per promise and documents neither cancelling nor replacing one, so the latest-wins part, the single worker and the panic's message would be ours anyway, for a dependency |

## See also

- [State or event](state-or-event.md): which mechanism for which need.
- [Lists and tables](table.md), [the tree](tree.md),
  [the photo library](library.md), [node graph editor](node-graph.md)
  and [choosing files](files.md): each view's steps.
- [The workbench](workbench.md): areas, tabs and detached windows.
