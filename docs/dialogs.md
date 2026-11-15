# Dialogs and the settings window

The contract of the family "Standard windows and dialogs"
([ADR 0027](adr/0027-component-families.md)): the standard modal
dialogs, the settings form and window, and the pages plugins add to it.
About has its own page ([About the application](about.md)).

| Module | What it does |
|---|---|
| `egui_pop_api::dialogs` | `DialogsPlugin`: confirm, form, choose, info and custom dialogs, queued, one shown at a time per window |
| `egui_pop_api::settings` | The settings form: every declared setting by section, applied live, each with its reset |
| `egui_pop_api::settings_window` | `SettingsPlugin`: the settings window (search, tree, form), the shortcuts editor, the gestures and the Fonts page |
| `egui_pop_api::settings_pages` | Custom pages a plugin draws in the settings window, mounted through the mount contract (ADR 0013) |

Both plugins are in `DefaultPlugins`.

**Owns**: the dialogs' rules (what the primary button says, Enter and
Escape, the queue, the backdrop, moving), the settings window generated
from declared settings, custom settings pages and their resets.

**Refuses**: wizards spanning several applications; a form builder for
arbitrary data (that is [admin forms](design/admin-forms.md)).

## Dialogs

A dialog is data, queued on the `DIALOGS` service; accepting runs its
command through the host with its arguments, the form's values and the
choice merged in.

```rust
use egui_pop_api::dialogs::{self, Choose, Confirm, DialogSpec};
use egui_pop_api::App;
use egui_pop_model::ids::CommandId;

let app = App::builder()
    .plugin(dialogs::DialogsPlugin)
    .build(&egui::Context::default())
    .unwrap();
let d = app.handle().service(&dialogs::DIALOGS).unwrap();
d.open(
    Confirm::new("Delete 3 images?", "Delete 3 images", CommandId::from_static("doc.delete"))
        .detail("They go to the trash.")
        .danger(),
);
d.open(DialogSpec::new(Choose::new("Export as", &["JPEG", "PNG"], CommandId::from_static("doc.export"))).movable(true));
assert_eq!(d.len(), 2);
```

- **Kinds** (`Dialog`): `Confirm` (a question before an action), `Form`
  (fields typed by value descriptors; a path field has Browse), `Choose`
  (options under a filter field, each answering a value of its own),
  `Info` (a notice, or `Info::error` with its details shown), `Custom` (a
  body of the application's, `Dialogs::open_custom`).
- **Rules**: the primary button says what it does ("Delete 3 images",
  never "OK"); a dangerous one is styled so and the focus starts on
  Cancel; Enter accepts (except a danger), Escape cancels; a click on the
  backdrop never closes. A notice and a confirmation have Copy (title,
  text and `Details`, for a bug report).
- **Presentation** (`DialogSpec`): `closable` (a ✕ that is Escape),
  `movable` (a [header band](overlays.md#header-bands), `dialogs.move`
  with Alt+F7 for the keyboard), `backdrop` (`Backdrop::Dim` or `Clear`,
  modal either way), `remember_position`. The settings `dialogs.closable`
  and `dialogs.movable` apply them to every dialog.
- **Windows** (ADR 0025): each application window has its own queue and
  a dialog blocks its own window group only; a dialog whose window
  closes is cancelled with it.
- **Owners** (ADR 0028): a dialog belongs to the plugin of its command,
  and a custom one also to the plugin that opened it. When that plugin
  is disconnected the dialog is cancelled at once, whatever the hooks:
  `dialog.cancelled` from `egui_pop` with a `reason` ("its plugin `x` is
  disconnected") and an info line; the next queued dialog shows. It does
  not come back when the plugin reconnects.
- **Custom bodies** are hosts of the mount contract: drawn every frame
  in a `dialog` focus scope and a [fault boundary](containment.md); keys
  the body claims go to it first (when it claims Enter, Ctrl+Enter
  accepts); its values (`CustomPass::value`) join the command's
  arguments.
- **Steps** (ADR 0011): `dialog.show` (a veto cancels), `dialog.answer`
  (a veto keeps it open with the reason, a redirect changes the answer),
  `dialog.move`. The journal says `dialog.answered`, then
  `dialog.chosen` or `dialog.cancelled`, and `dialog.moved`.
- **Agents**: the `dialogs` collector shows the dialog and answers it as
  its caller (`Answer::Accept`, `Choose`, `Value`, `Cancel`); an answer
  that would run a command not offered to agents is refused to anyone
  but the user (`dialog.answer.refused`), and the dialog stays open.

## The settings window

`Ctrl+,` (`app.settings`) opens the Settings app when the application
has it (`DefaultPlugins` do), else the settings window; the window
itself opens from the Settings app's General › Advanced page
(`app.settings.window`). It has a search field over a tree of sections,
the form (`egui_pop_api::settings::form`), the shortcuts editor (keys
captured by pressing them, conflicts said, each change reset on its
own), the pointer gestures and the Fonts page. Its writes go to the
store's `SETTINGS` history: while the focus is in the window, `Ctrl+Z`
undoes the settings' changes.

A plugin adds a page with `Ctx::settings_page`:

```rust
use egui_pop_api::settings_pages::SettingsPage;
use egui_pop_api::{Ctx, Plugin, PluginError};
use egui_pop_model::ids::PluginId;

struct Compute;

impl Plugin for Compute {
    fn id(&self) -> PluginId {
        PluginId::from_static("compute")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.settings_page(
            SettingsPage::new("compute.devices", "Compute").keywords(&["GPU", "threads"]).order(10),
            |ui, _app| {
                ui.label("The devices found, and which one runs what.");
            },
        );
        Ok(())
    }
}
```

A page has a place in the tree (`under`, `order`), keywords for the
search, and the settings it edits (`settings`): those leave the
generated sections and its reset takes them back as one undo step, with
`reset_with` for what is not a setting. Agents open a page with
`app.settings.page` (`{"page": "<id>"}`) and read the pages in the
`settings_window` collector.

## States and failure

- A dialog vetoed by `dialog.show` is cancelled before it shows; one
  whose answer is vetoed stays open with the reason.
- `Dialogs::open_custom` refuses a spec that is not a `Custom`; a
  custom page or body whose component needs what the host does not offer
  is refused at mount (ADR 0003): reported, nothing added.
- A body or page that panics shows a fault card in its place; the
  dialog keeps Cancel and Escape.
- A body is drawn only while its page is shown; the settings window
  moves by commands, applied before the next frame.

## Accessibility

Every dialog is modal with its first focus inside and the focus given
back on close ([overlays](overlays.md)); every editor of the settings
form is labelled by its setting's label; the ✕ is named "Close"; a
movable dialog moves by keys. Labels, sections and choices are in the
interface's language.

## Tier

From [API stability](stability.md): dialogs and the settings window are
**Supported** ("The components: the command palette, dialogs, toasts,
the settings window and shortcuts editor, … Their behaviour and commands
are kept; their drawing follows the theme and may be refined"). Custom
settings pages follow the settings window.

## Tests

`crates/egui-pop-api/tests/`: `dialogs.rs` (kinds, rules, queue,
windows, moving), `custom_dialog.rs` (custom bodies, claimed keys),
`dialog_copy.rs` (Copy and details), `settings_window.rs`,
`settings_pages.rs`, and the settings' history in `history.rs` and
`undo.rs`; dialogs opened by other families in `about.rs`,
`clipboard.rs`, `files.rs`, `toasts.rs`.
