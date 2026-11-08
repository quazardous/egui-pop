# Administration as forms: one declarative surface for settings, tools and tasks

Status: **accepted** (2026-11-08), with the recommendations of section 8
(Q1 to Q10). Slice 1 is built: the model (`egui_pop_model::admin`:
manifests, value scopes, hooks, argument templates, the settings'
generated pages) and the shell (`egui_pop_api::admin`, `AdminPlugin`:
overlay and pop out, scope tabs, sections, search, rows with a modified
bar, an origin tag, reset and "Set for", deep links, commands `admin.*`,
the `admin` collector), shown in the gallery's Settings page.

Slice 2 adds the record bricks: `list` and `table` items (`Records`)
read from a `read_only` command (an array at a JSON pointer) or a
`records` hook, with columns, a tone key, row actions and an empty
state; statuses read from `read_only` commands; `when` and `enabled`
evaluated (the context keys plus the section's values as
`form.<field id>`; a clause that does not parse is reported). A read is
asked as a command run before the next frame and its answer taken from
the runs; it is kept until a command that changes something runs or an
event a node names in `refresh` arrives. Tables draw as a striped grid,
not yet through `egui-pop-views`' data table; row fields, add and
remove, selection, bulk actions and group-by are still to come. Rows lay
their right-hand controls from the page's right edge and wrap their help,
so nothing passes the page.

Slices 3 and 4 are on the application's side (Helios): its scopes and
its settings placed through a bridge (layers: machine, library, person,
command line), then its Storage and Backup pages declared as manifests.
The volume table and the strategy statuses read the application's own
`read_only` commands, with row actions over its commands; the scope and
section titles go through the application's catalogues before they are
declared, since declared titles are shown as given. Its hand-drawn
Storage window went with slice 5, once row fields, confirmations and
the strategy as a form existed.

Slice 5 adds the editing bricks. **Row fields** (`RowField`): a
record's value shown as a toggle, a text (written when left or on
Enter) or a choice (declared choices, and choices the record holds at a
key), written through a command whose arguments name the record
(`${row.<key>}`) and the value (`${value}`); in a table one takes the
place of its key's column or adds a column, in a list it shows under
the record's title. **Confirmations**: an action declares one (title,
text, verb, danger; its texts may name the row) and the dialogs'
Confirm asks it before the command runs; a `confirm` hook is asked the
same way. A **meter** column (`CellKind::Meter`): a share from 0 to 1
drawn as a bar with the words of another key. Row fields and row
actions read their `when` and `enabled` against the record
(`row.<key>`). A source answering an object at its pointer is one
record, so a single record (a backup strategy) is edited as a list of
one. A deep link `scope/section/<record key>` marks that record and
scrolls to it. Helios's Storage and Backup pages are built on these.

Slice 1 leaves for later: `Ctrl+,`, File > Settings and the aliases
`app.settings` and `app.settings.page` (the settings window keeps them
until the shortcuts editor, gestures and fonts become widget kinds and
the window goes); the kit components of section 4.4 (the shell draws
with the existing selectable rows, buttons, combo boxes and indicators);
icons on the scope tabs and their overflow; the scopes' order and the
search's filters (`@modified`, `@scope:`); statuses read from commands
(done in slice 2); `when` and `enabled` clauses on nodes (done in
slice 2); draft fields, record lists and tables (lists and tables done
in slice 2), tasks, the wizard kit; the alter hook
over the tree (`Hook::admin`); the last place kept across runs (only the
mode is); the application's link scheme; Helios's scopes and its
`helios-config` bridge.

The request: an application's non-image side (its settings, its tools,
its import and export, its backups, its storage, its health check, its
plugins, its keys, its jobs) is **one application within the
application**, built by one framework. It is pure forms, like a DOM, and
has no image surfaces. Photo work (viewports, the graph, the darkroom,
culling grids) stays in the workbench.

The owner's rules, all binding:

1. One administration app for every non-photo surface.
2. Two navigation axes: **horizontal** for the scope or family (tabs with
   icons), **vertical** for the sections and names (a left panel).
3. **Full page**: an overlay of the main window, or a separate system
   window that can go full screen.
4. **Components never draw their own widgets.** They declare a contract,
   a manifest: options, actions, status read-outs, record lists and
   tables, tasks. The framework renders and manages the panels. Custom
   hooks are allowed; registered widget kinds are declared exceptions.
5. **Ultra-standard building blocks**: a small, finite set of bricks
   composes every surface.
6. **Import and Export use the same framework** without living inside the
   settings universe: standalone task surfaces with their own entry
   points, built from the same manifests and bricks.
7. **A multi-step wizard kit**: declared steps (each a form), validation
   per step, next, back and skip, a summary, a run step with progress and
   results, resumable, cancellable where safe, every step deep-linkable.

Helios is the first consumer and drives the inventory, but the framework
is egui-pop's: any egui-pop application declares into it, and egui-pop's
own settings window becomes its first surface.

Out of scope: the image side (viewports, graph, darkroom, grids), the
layout of the workbench (egui_stack), the file browser's internals (see
[One file browser, many views](file-browser.md); here it is a registered
widget kind).

## 1. What exists

Read on 2026-11-08: egui-pop's `next`, Helios's `main`.

### 1.1 egui-pop

| Piece | Where | What it already covers | What it lacks for this note |
|---|---|---|---|
| Value descriptors | `egui_pop_model::values::Descriptor` | A typed value: `Float`, `Int`, `Bool`, `Choice`, `Floats`, `Ints`, `Text`; label, description, default; hard and soft ranges, step, coarse step, unit, display scale, precision; choices; list count; text capacity; a path hint (directory or file with extensions, shown with Browse); who may write it (`Writers`). Serialisable. | No `when` (shown or enabled when), no validation hook, no dynamic choices, no group or order, no scope beyond the store path, no help link, no "needs restart". |
| Settings | `Ctx::setting(SettingId, Descriptor)` | A setting at `settings/<id>`, owned by its plugin (ADR 0004, 0016), applied live, with its reset; read with `AppHandle::setting`. | One scope: the user's. No library or workspace layer for settings (the keymap has one, below). |
| The settings form's entries | `hooks::SettingEntry`, `Hook::settings` | Every setting as the form shows it: id, path, owner, a section title (the owner's, unless an alter hook changes it). Alter hooks reorder, regroup, remove. | The section is a flat title; no scopes, no groups inside a page. |
| The settings window | `settings_window::SettingsPlugin` | A modal overlay (`OverlaySpec ... Kind::Modal`, `Ctrl+,`, File > Settings): a search field over a tree of sections, the generated form, a reset per setting; its own undo history (`SETTINGS`, ADR 0014); a shortcuts editor (keys captured, conflicts said, user or workspace layer, per-key reset); the pointer gestures; a Fonts page. Commands `app.settings`, `app.settings.page`. A `settings_window` collector for agents. | A modal of fixed size, one axis only (the tree), not a full page, no separate window, no tasks, lists or statuses. |
| Custom settings pages | `settings_pages::SettingsPage`, `Ctx::settings_page` | A page a plugin draws, placed in the tree (`under`, `order`), found by search (`keywords`, its settings), its reset (the settings it edits, plus `reset_with`), mounted through the mount contract (ADR 0013) in a focus scope and a fault boundary. Helios's theme editor is one. | It draws its own widgets: the exception this note keeps, but declared as such. |
| `setting_row` | `settings_window::setting_row` | One setting drawn as the form draws it, for a custom page. | The seed of the row brick. |
| Dialogs | `dialogs::{Confirm, Form, Choose}` | A **Form** is fields typed by descriptors and the command they go to (`Form::new(title, verb, command)`, `field`, `field_at`); a **Confirm** names its verb and has a danger style; a **Choose** is a filtered list. Every open dialog is data for agents. | One page; no steps, no validation per field beyond the descriptor, no preview. |
| Commands | `egui_pop_model::commands::CommandSpec` | Id, title, category, description, `when`, `checked`, keys, menus, palette, agent, `read_only`, an `args` description (free JSON or a schema), icon. Every command runs through the host (ADR 0023). | Exactly what an action brick binds to; `read_only` commands are what a status read-out reads. |
| Jobs | `docs/progress.md` | A jobs model (progress, "n of m", time left, cancel, pause, resume, retry, the `jobs.estimate` step), the jobs centre, busy controls, skeletons and empty states. | The run step of a task binds to it as it is. |
| Data views | `egui-pop-views`, `docs/table.md`, `docs/list-items.md` | A virtualised list, a data table (on `egui_table`), a tree; list items and property rows; their kit (`list-row`, `table-row`, `list-item`). | The list and table bricks render through them. |
| Collapsible sections | `docs/sections.md` | Sections in egui_stack's stack, egui-pop's look, named and observable. | The group brick. |
| Inputs and Tab order | `docs/inputs.md` | The decorated input, field states (error, warning, disabled), Tab order, F6 through regions, roving groups. | Every field brick uses them. |
| Deep links | `docs/instance.md` | A single instance and `scheme://` links handed to the running one. | `admin.open` gets a link form. |
| The kit | `docs/kit.md` | Components drawn by kit handlers from `component.state.term` tokens; the default kit is a kit. | New components for the shell, rows and the wizard (section 4.4). |
| `developer.*` settings | `egui-pop-api` devtools, `egui-pop-agent`, `egui-pop-i18n`, platform logs | `developer.devtools`, `developer.agentSocket`, `developer.show_keys`, `developer.log_level`, each a plain setting under its owner's section. | A Developer scope gathers them. |

egui-pop's own settings, by prefix: `appearance.*` (accent colour, focus,
fonts, interface zoom, language, scroll bars, text size, theme preference,
toast corner), `dialogs.*` (closable, movable), `files.*` (folders first,
native chooser, preview), `input.*` (pointer profile, wheel device and
speed, slider wheel, image double click), `menus.icons`,
`notifications.desktop`, `palettes.*`, `tips.*`, `viewer.image_hosts`,
`window.*` (frame, bar in full screen), `workbench.*` (panel alignment,
side bar position), `developer.*`.

**What the existing pieces already say.** Descriptors and commands cover
most of a manifest: a field is a descriptor bound to a store path, an
action is a command with arguments, a status is a read-only command, a
form is a list of fields and a command. What is missing is the **tree**
(scopes, sections, groups), **records** (lists and tables with row
actions), **tasks** and **steps**, the **hooks** a descriptor cannot
express (validation across fields, dynamic choices, previews), and a
**shell** that is a full page with two axes.

### 1.2 Helios

| Surface | How it is drawn today | Entry | What it holds |
|---|---|---|---|
| Settings window | egui-pop's, generated | `Ctrl+,`, the system menu | egui-pop's settings plus Helios's three: `display.wide-gamut-preview`, `notifications.jobs`, `health.destinationDays`; the shortcuts editor with the workspace layer; the theme editor (a custom page). |
| `helios-config` settings | **Not shown anywhere in the GUI** | TOML files, `--set KEY=VALUE`, `helios inspect describe config` | Declared once with a concern (machine, project, user) and layers (default, global, project, user, run): `system.cache.budget`, `system.cache.share`, `system.cache.thumbnails`, `system.map.exclude`, `system.map.defaults`, `system.map.record`, `system.map.limit`, `system.map.keepMissing`, `system.scheduler.workers`, `system.ai.calls`, `system.events.journal`, `system.workflow.keep_days`, `project.decisions.mode`, `interface.panels.queue_when_empty`, `interface.graph.zoom`, `interface.experimental.workshops`. `docs/architecture.md` says pages come from the `page.group.name` naming; that page generator was never wired into the desktop. |
| Storage page | Hand-drawn `egui::Window` (620 by 480, centred), `DragValue`, `ComboBox`, frames | Library > Storage… (`library.storage`, `{"volume": id}`) | The backup strategy (none, or copies, destinations, off site, when) and "Back up now" (`library.backupNow`); every volume (name edited in place, kind, capacity, free space, role toggles, off-site and RAID flags, node, last seen, photo count, "Mark this disk…" with a confirmation). Saved at once. |
| Health check | Hand-drawn page | Help > Health check (`help.health`), the status bar's badge | Findings by level (problem, warning, fine), each with detail, "Fix…" (a command), "Ignore"; "Ignored (n)" with "Show again"; "Check now" (`health.checkNow`). Checks: free space, catalogue integrity, the library, the backup strategy, photos backed up, backup disks seen lately, verified copies, cards safe to format. |
| Import window | Hand-drawn `egui::Window`, three columns, about 1,800 lines with its choices | File > Import… (`Ctrl+Shift+I`, `library.importWindow`), the empty library, a card's toast, drop and paste | Sources (cards, recent folders, favourites, "Choose a folder…"), contents by capture day with ticks and "New only", choices (copy or add, into and layout with an example path, name pattern with an example, duplicates, darktable edits, "More"), presets (Default, saved, Save as, Delete), totals, Cancel and Import; `library.importFiles` with the choices as arguments. |
| Import dialog (scripts) | egui-pop Form | `library.importSource`, `library.importFiles` without arguments | Referenced or copied, "Remember for this workspace", darktable edits. |
| Importer plugins | Menu entries, then a file or folder chooser | File > Import from (`library.importWith`) | An importer a plugin declares. |
| Export | egui-pop Form | File > Export… (`Ctrl+Shift+E`, `library.export`) | The workspace's export settings, "Remember for this workspace"; runs as a parent job with an export per image. |
| Keyboard shortcuts | egui-pop's shortcuts editor | Settings, the system menu | Keys per command, user or workspace layer. Keymap packages (`keymap.package`, `keymap.package.remove`) are commands only. |
| Themes and profiles | System menu lists; the theme editor is a custom settings page | `theme.select`, `profile.use`, `profile.import`, `profile.export`, `theme.exportMerged` | A theme and its overrides; profiles saved per workspace. |
| Plugins | CLI and commands only | `plugin.list`, `plugin.enable`, `plugin.disable`, `plugin.grant`, `plugin.revoke` | No GUI. |
| GPU and compute | CLI and commands only | `compute.list`, `compute.probe`, `compute.benchmark`, `compute.enable`, `compute.disable`, `compute.role`, `compute.route`, `compute.budget`, `compute.remote` | No GUI; the calibration reports as a job once the desktop opens a compute manager. |
| Jobs | egui-pop's jobs centre (a panel and the status bar's popover), job consoles | The status bar | Live jobs, Cancel, logs. |
| Library and workspaces | Commands, Forms, the workspace editor (`workspace.edit`) | File menu, system menu | `library.default`, `library.db`, `vault.integrity`, `workspace.new`, `workspace.open`, `workspace.saveAsTemplate`, `workspace.importParts`, `workspace.reset`. |
| Developer | Plain settings in egui-pop's window | Settings | `developer.devtools`, `developer.agentSocket`, `developer.log_level`, `developer.show_keys`; `logs.reveal` in the palette. |
| Catalogue snapshots | Research only (`docs/storage-backup-research.md`) | None yet | Snapshots on close and daily, copied to backup volumes; restoring one is a health fix's suggestion. |

**Count.** Three hand-drawn windows (Storage, Health, Import), about
3,000 lines of egui calls between them; sixteen declared settings with no
GUI; five families of commands (plugins, compute, keymap packages,
profiles, snapshots to come) with no GUI at all. Everything else is
already declared and generated.

## 2. Survey

From the products' documentation and use; the links are in section 9.
"Unconfirmed" marks what we did not check against a source.

### 2.1 Settings and preferences

| App | Navigation axes | Search | Scopes | Contributed settings | Deep links | Reset and modified | Modal or page |
|---|---|---|---|---|---|---|---|
| Blender Preferences | One vertical list of sections on the left (Interface, Themes, Viewport, Editing, Animation, Add-ons, Input, Navigation, Keymap, System, Save & Load, File Paths); one long page with collapsible panels | Inside Keymap, Add-ons and Themes only | App only; per-file settings live elsewhere (the Properties editor) | Add-ons draw their own preferences under their row (`AddonPreferences.draw`), a counter-example | The operator `screen.userpref_show(section=...)` | Right click "Reset to Default Value"; no modified marker (unconfirmed for 4.x) | A separate window; saved automatically or by a button |
| darktable | Tabs on top (general, import, lighttable, darkroom, processing, security, storage, miscellaneous, shortcuts, presets, Lua options), then headed groups | In the shortcuts tab | App only | Lua scripts register preferences (`darktable.preferences.register`), drawn by the host in the Lua options tab: a declarative precedent | None | A double click on a label resets; a changed value is marked (unconfirmed which mark) | Modal dialog |
| Lightroom Classic | Tabs on top (General, Presets, External Editing, File Handling, Interface, Performance, Display, Network…) | None | **Two dialogs**: Preferences (app) and Catalog Settings (the library) | Plug-ins add sections to the Plug-in Manager and to Export through the SDK's view factory (`LrView`: rows, fields bound to property keys): the host's vocabulary, the plug-in's layout | None | "Restore defaults" per area | Modal |
| Photoshop | A list of panes on the left (General, Interface, Workspace, Tools, History Log, File Handling, Export, Performance, Scratch Disks, Cursors, Transparency, Units & Rulers, Guides, Plug-ins, Type, Technology Previews), Prev and Next buttons | None (the Discover panel searches features, not preferences) | App only | None | None | "Reset Preferences on Quit"; some panes "need a restart" | Modal |
| Capture One | Tabs with icons on top of the Preferences window (General, Appearance, Image, Color, Exposure, Warnings, Capture, Plug-ins…, unconfirmed list) | None | App; catalogue or session for import and output | Plug-ins in their own tab | None | Unconfirmed | Window |
| VS Code | **Search first**; a scope axis as tabs (User, Remote, Workspace, Folder); a table of contents on the left (Commonly Used, Text Editor, Workbench, Window, Features, Application, Extensions > each one) | Full text over ids, titles and descriptions; filters `@modified`, `@ext:`, `@id:`, `@feature:`, `@lang:`, `@tag:` | `application`, `machine`, `machine-overridable`, `window`, `resource`, `language-overridable` per setting | **Declared**: an extension's `package.json` `contributes.configuration` is a JSON Schema per key (type, default, enum and its descriptions, markdown description, scope, order, deprecation, edit presentation); the host renders every one | `vscode://settings/<id>`; the command `workbench.action.openSettings` with a query | A bar at the left of a modified row; a gear menu (Reset Setting, Copy Setting ID, Copy as JSON); "Also modified in Workspace" | A page (an editor tab), with a JSON view of the same values |
| JetBrains IDEs | A tree on the left (Appearance & Behavior, Keymap, Editor, Plugins, Version Control, Build, Languages, Tools, Advanced Settings) | Above the tree: filters the tree and highlights matching labels inside pages; options indexed at build time | IDE or project, a marker on project-level pages | Plug-ins declare configurable pages in `plugin.xml` (`applicationConfigurable`, `projectConfigurable`: parent id, id, name); the configurable builds its UI, usually with the Kotlin UI DSL bound to properties | `ShowSettingsUtil.showSettingsDialog(project, id)` | "Reset" per page; modified pages marked in the tree (unconfirmed); Apply, OK, Cancel | Modal |
| GNOME Settings | A sidebar of panels (Network, Bluetooth, Displays, Sound, Power, Multitasking, Appearance, Apps, Notifications, Search, Online Accounts, Sharing, Mouse & Touchpad, Keyboard, Color, Printers, Accessibility, Privacy & Security, System), sub-pages pushed inside a panel | At the top of the sidebar: panels by keywords | System or user | Panels are built in; the libadwaita bricks are finite: preferences page, group, action row, switch row, combo row, spin row, entry row, expander row | `gnome-control-center <panel>` | None per row | A window |
| macOS System Settings | A sidebar with search, a pane of grouped inset lists, sub-pages pushed inside | Top of the sidebar, highlights the matching pane | System or user | Third-party panes (legacy `.prefPane`) | `x-apple.systempreferences:<pane>?<anchor>` | None per row | A window |
| Windows Settings | A left list of categories (System, Bluetooth & devices, Network, Personalization, Apps, Accounts, Time & language, Gaming, Accessibility, Privacy & security, Windows Update); pages with a breadcrumb and sub-pages; setting cards and expanders | Top of the left list | System or user | Built in | `ms-settings:<page>` | None per row | A window |

### 2.2 Tasks and wizards

| App | Shape | Steps | Validation | Summary | Run | Resume, cancel |
|---|---|---|---|---|---|---|
| Lightroom Classic import | One full page: a top strip read left to right (source, then Copy as DNG, Copy, Move or Add, then destination), sources on the left, the grid in the middle, panels on the right (File Handling, File Renaming, Apply During Import, Destination), presets at the bottom; a compact mode | Implied by the strip; one page | Inline (no photos ticked: Import disabled) | The bottom bar's counts | Background, a bar at the top left | Cancel the background task |
| Lightroom Classic export | A dialog: presets on the left (Lightroom's, the user's, plug-ins'), collapsible sections on the right (Export Location, File Naming, File Settings, Image Sizing, Output Sharpening, Metadata, Watermarking, Post-Processing); plug-ins add sections | One page | Inline | None | Background | Cancel |
| Capture One import | One page: a source browser, thumbnails, panels (Import To, **Backup To**, Naming, Metadata, Adjustments) | One page | Inline | None | Background | Cancel |
| macOS Migration Assistant | A wizard: how to transfer, from which source, a code to compare, what to transfer (a checklist with sizes), progress with time left, done | Linear, with Back | Per step (the source must answer) | The checklist with sizes | Long, progress and time left | Cancellable; restart from the start (unconfirmed) |
| GNOME Initial Setup | A wizard: language, keyboard, privacy, time zone, online accounts, about you, password, ready | Linear, Back and Next in the header bar, some skippable | Per step | None, "Ready to go" | Short | Not resumable |
| Windows installers (MSI, WiX, Inno Setup) | A wizard: welcome, licence, destination, components, **Ready to install** (Inno's memo of every choice), installing, finish | Linear, Back, Next, Cancel | Per step (disk space, a path) | A dedicated step | Progress, then a finish page with "Launch" | Cancel rolls back (MSI) |
| VS Code walkthroughs | Declared (`contributes.walkthroughs`): steps with media, each completed by an event (`onCommand`, `onSettingChanged`, `onView`) | Non-linear, a checklist | By the completion events | None | Commands | Progress kept per user |
| JetBrains New Project | A list of generators on the left, a form on the right, more steps for some | Next, Previous | Per step | None | Short | No |
| JSON Forms (EclipseSource) | A UI schema over a JSON Schema: layouts (vertical, horizontal, group) and **Categorization**, rendered as tabs **or as a stepper** (`variant: stepper`) from the same schema; rules show, hide, enable, disable on a condition | The categories | The schema's | None | None | None |
| react-jsonschema-form | A JSON Schema plus a UI schema (`ui:widget`, `ui:order`, `ui:help`, `ui:options`); a registry of widgets, fields and templates; custom widgets by name; validation by a pluggable validator; conditional parts by `dependencies` and `if`/`then` | One page | Schema plus custom validation | None | None | None |
| Qt property browser, KConfigXT | Qt's: property managers (the model), editor factories per type (widget kinds), browsers (tree, group box, button: the layout). KDE's: a `.kcfg` file declares entries, a dialog binds widgets named `kcfg_<name>`, Defaults and modified state come for free | One page | Per type | None | None | None |

### 2.3 Patterns

- **Two axes are common, and the horizontal one is never the same
  thing.** darktable, Lightroom and Capture One put families on top tabs
  and groups under them; VS Code puts the **value scope** (User,
  Workspace) on top and families on the left. The owner's direction is
  the first: families on top. The value scope then needs another place
  (section 3.4).
- **Settings in the OS and in the big IDEs are search-first**: a search
  field at the top of the navigation (GNOME, macOS, Windows, JetBrains) or
  of the page (VS Code). Photo tools have none. Every search shows the
  results in place, not as a list of links.
- **Library settings apart from app settings.** Lightroom's two dialogs,
  Capture One's catalogue and session, JetBrains' IDE and project. Two
  dialogs is the known complaint; one surface with a scope marker is the
  modern answer (VS Code, JetBrains' marker).
- **The best extension models are declarative.** VS Code renders every
  extension's settings from JSON Schema; darktable renders Lua's;
  Lightroom's SDK gives a host vocabulary. Blender and JetBrains let
  plug-ins draw, and their preferences look like a dozen apps glued
  together.
- **A finite brick set is a known, good thing.** libadwaita (action,
  switch, combo, spin, entry, expander rows inside groups inside pages)
  and WinUI's setting cards and expanders build all of GNOME's and
  Windows' settings from about eight bricks.
- **Deep links exist wherever settings are a page** (VS Code, GNOME,
  macOS, Windows), and are the way a health finding or an error message
  sends the person to the fix.
- **Modified markers and per-row reset are rare** outside VS Code and
  darktable, and loved where they exist. egui-pop already has per-row
  reset.
- **Import and export in photo tools are one page, never a wizard**
  (Lightroom, Capture One), because people import daily. Wizards are for
  what happens once (first launch, migration, an installer), and their
  common shape is fixed: steps on the side or as a header, Back and Next,
  a "Ready" summary, a run with progress, a finish page with follow-ups.
- **One schema, two renderings.** JSON Forms renders the same categories
  as tabs or as a stepper. That is the answer to "wizard or single page"
  for import: the steps are declared once; the shell chooses.

## 3. The model: a declarative form tree

### 3.1 The tree

Every node has an `id` (stable, never translated, unique among its
siblings), a `title` and optional `description` (translated through the
strings catalogue, as settings are today), `keywords` for search, an
`icon` (from the icon vocabulary), an `order`, a `when` clause (shown when
it holds; the existing `when` language) and an `enabled` clause (greyed
with its reason otherwise). A node's **path** is its ids joined by `/`.

| Kind | Holds | Renders as | Notes |
|---|---|---|---|
| **app** | scopes | the administration shell (section 5.1) | One per application; plugins add to it. |
| **scope** | sections | a tab with an icon on the horizontal axis | Settings, Storage, Backup… (section 6). |
| **section** | groups and nodes | an entry of the left panel, and the page shown | One level of nesting below (sub-sections), no deeper. |
| **group** | nodes | a heading and a block on the page, collapsible | Listed under the active section in the left panel as anchors. |
| **field** | a value | a row: label, control, help, modified marker, reset | Its kind is one of the bricks (section 4.1). Bound to the store or to a task's draft. |
| **action** | a command and its arguments | a button (primary, secondary or danger) or a link | Confirmation optional; busy while its job runs. |
| **status** | a read-out | a row: label, value, tone, last updated | Read from a read-only command or a provider; refreshed on events. |
| **list** and **table** | records | a list of record rows, or a data table | Row actions, row fields, add and remove, selection, an empty state (section 3.3). |
| **task** | inputs, a run command, results | a page with a footer (presets, totals, Cancel, the run verb) | The unit Import and Export are made of. |
| **wizard** and **step** | steps, each a form | a stepper and a page per step, or all steps on one page | Section 5.3. |
| **widget** | a registered kind and its options | whatever the kind draws, in a row or a block | The declared exception: the file browser, the key-chord editor (section 4.3). |
| **page** | a draw function | the plugin's own drawing, in a focus scope and a fault boundary | Today's `SettingsPage`, kept as a declared exception, counted and listed by the inspector. |

A section's page is generated from its children in order. Nothing in the
tree says how many columns or pixels; the framework lays out, the kit
paints.

### 3.2 Bindings

**To the store.** A field binds to a value path. A setting keeps its
path, `settings/<id>`, and its descriptor; the field adds what the
descriptor lacks:

```text
field  id: "destinationDays"
       bind: setting "health.destinationDays"    # descriptor from the declaration
       scopes: [app]                              # where it may be set (3.4)
       kind: number                               # default: from the descriptor's type
       help: "A backup disk not seen for longer is a warning."
       restart: false
```

A field may declare **its own** descriptor instead (a task's input, a
record's row field). Its value then lives in the task's draft (3.5) or
the record (3.3), not in the settings.

**To commands.** An action is a command id and an argument template.
`${field}` takes a field's value from the same form, `${row.<key>}` a
record's, `${selection}` the selected records' ids. The command's
`CommandSpec` gives the title, icon, keys and `when` when the action does
not set them, so an action and the palette entry are the same thing.

```text
action id: "backupNow"  command: "library.backupNow"  style: primary
       confirm: none     busy: job "backup"
```

A **status** binds to a `read_only` command and a JSON pointer into its
answer, or to a provider registered in Rust; `refresh` names the event
kinds after which it is read again (`job.finished`, `vault.*`,
`storage.volume.*`) and a period. Statuses are never polled per frame.

### 3.3 Records: lists and tables

A list or table names a **record source**: a read-only command returning
an array of objects (`compute.list`, `plugin.list`), or a provider
registered in Rust (the volume registry). It declares:

- the **key** of a record (its id) and its **title** and **subtitle**
  fields (a list) or **columns** (a table: key, title, kind, width hint,
  sortable);
- **row fields**: fields bound to a record's key, written through a
  command (`storage.volume.set {"volume": ${row.id}, "name": ...}`), shown
  inline or in a details pane under the row (an expander row);
- **row actions** (each an action with `${row.*}`), a **primary** one run
  by Enter or a double click, and **bulk actions** on the selection;
- **add** (a command, often with a form) and **remove** (a command, with
  a confirmation), when the source allows them;
- a **tone** per row from a field (a finding's level, a volume offline);
- an **empty state** (a message and an action: "No backup disk yet. Add
  one…");
- **group by** a field (findings by level, volumes online then offline).

Rendering goes through `egui-pop-views` (list, table) and list items, so
virtualisation, keyboard and the kit come for free.

### 3.4 Value scopes: app, library, workspace

Every field says where its value may be set: `app` (this person on this
machine), `library` (the open library, shared by everyone using it) and
`workspace` (the open workspace's own files). A run override (a command
line `--set`) is a fourth origin, shown, never edited.

| Here | egui-pop today | Helios today (`helios-config`) |
|---|---|---|
| `app` | the settings store (`settings/<id>`) | `global` (and `user`, which follows the person on every machine) |
| `library` | none | `project` |
| `workspace` | the keymap's workspace layer | the workspace's files (import source, export settings, keys) |
| run, shown only | none | `run` |

Helios's concerns map: machine to `app` only, project to `app` and
`library`, user to `app` (and `workspace` where it makes sense). egui-pop
needs a layered store for settings (a library and a workspace layer above
the user's), which the keymap already has: that is a model change, the
first slice's.

A row shows **where its value comes from** (a small origin tag:
`library`, `workspace`, `run`; nothing for the default) and its reset
takes back the value **of the layer shown**. A field settable in several
scopes has a menu in its row, "Set for: this app, this library, this
workspace" (JetBrains' marker and VS Code's "Also modified in", without
VS Code's top tabs, which the families took).

### 3.5 Tasks

A task is a form whose values are a **draft**, not settings: its fields'
values live under `tasks/<task>/draft` in the store (per library, so
"the last used" is remembered where it makes sense) until it runs. It
declares:

- its **inputs** (fields, lists, widgets);
- its **presets**: named drafts (Default, the user's, a plugin's), with
  Save as, Rename, Delete; the brick is the task's footer menu;
- its **run**: a command with the draft as arguments (`library.importFiles`
  with `paths` and the choices) and a **verb** ("Import 248 photos");
- its **totals**: statuses computed from the draft (count, size, space
  left on the destination);
- its **results**: what the job's end returns (counts, a record list of
  what failed or was skipped) and **follow-ups** (actions: "Show in
  library", "Back up now", "Eject the card", "Open folder").

The draft is the command's arguments: an agent runs the same task by the
command alone, with `dry_run` where the command has it. The form is only
a way to fill the arguments.

### 3.6 Hooks

Hooks are Rust functions registered by name (`HookId`, as the alter
hooks) and named by the manifest. They compute; they never draw.

| Hook | Signature, in words | Used for |
|---|---|---|
| `validate` | (the form's values) to diagnostics per field and for the form | A pattern that does not parse; the destination not writable; Next disabled with the reason. Field-level checks the descriptor can say (range, capacity, path kind) need no hook. |
| `transform` | stored value to shown value, and back | A byte budget shown in GB; a list of globs stored as one text. |
| `choices` | (the form's values) to choices with labels and hints | Destinations among the volumes with a backup role; presets; GPU devices. |
| `preview` | (the form's values) to a text or a small record | The example path and file name from the first ticked photo; "Copies to: Backup A, Backup B". |
| `confirm` | (the action and the form) to a confirmation or none | "Forget this volume? 1,204 photos lose their location." Danger style, Cancel focused. |
| `status` | (none) to a value and a tone | What a read-only command cannot give cheaply. |
| `records` | (none) to records | A source with no command. |

The `when` and `enabled` clauses cover "shown or enabled when" without a
hook: they read the form's values as context keys (`form.copy == true`).

### 3.7 Manifests

A manifest is what one owner (the app or a plugin) contributes: nodes,
each placed by its parent's path, plus the hooks and widget kinds it
registers. It is **data with names**: Rust builders produce it, and it
serialises to JSON so an out-of-process plugin can send it and an agent
can read it.

```rust
// Rust builders: the same shape as SettingsPage and Form today.
ctx.admin(
    Manifest::new(1)
        .section("backup/strategy", "Strategy", |s| {
            s.field(Field::setting(STRATEGY_COPIES))
             .field(Field::setting(STRATEGY_OFFSITE))
             .status(Status::command("Last backup", "backup.status", "/last"))
             .action(Action::command("library.backupNow").primary())
        })
        .section("storage/volumes", "Volumes", |s| {
            s.table(Table::provider("volumes", VOLUMES)
                .column("name", "Name").column("free", "Free")
                .row_field(Field::text("name", "Name").write("storage.volume.set"))
                .row_action(Action::command("storage.volume.forget").danger()))
        }),
)?;
```

- **Versioned**: `Manifest::new(1)`, the format's version; a manifest of a
  newer version than the framework is refused and reported (ADR 0003),
  one of an older version is read through a migration (ADR 0008's way).
- **Owned** (ADR 0004): a plugin's nodes leave with it; a node placed
  under a path that does not exist lands in the plugin's own section of
  the Plugins scope, with a diagnostic.
- **Altered** by alter hooks, as the settings entries are today
  (`Hook::admin`): reorder, regroup, hide.
- **Checked** when the application resolves its declarations: every
  bound setting exists, every command exists, every hook and widget kind
  is registered, every id is unique among its siblings. A failure is a
  diagnostic and the node is left out, never a panic.
- **Seen by agents**: an `admin` collector gives the resolved tree, and
  the inspector counts the custom pages and widgets (the exceptions).

## 4. The standard bricks

The list is closed: adding a brick is a decision in this note, not in a
plugin.

### 4.1 Field kinds

| Kind | Value | Renders like | From today's descriptor |
|---|---|---|---|
| `toggle` | bool | a switch at the row's end | `Bool` |
| `number` | int or float | a slider and a field when a soft range is given, a spin field otherwise; unit, step, coarse step | `Float`, `Int` |
| `choice` | one of n | a segmented control (up to 3, icons allowed), radio rows (4 or 5, with a hint each), a drop-down (6 and more), decided by the framework | `Choice` |
| `multi` | some of n | check rows, or chips past 6 | new (a list of choices) |
| `text` | text | a single-line field; a pattern with a preview hook; capacity | `Text` |
| `long-text` | text | a multi-line field | `Text` with `lines` |
| `path` | a path | a field and Browse (egui-pop's folder or file chooser) | `Text` with a path hint |
| `list` | values | an editable list of entries (add, remove, reorder): globs, folders | `Floats`, `Ints`, a list of texts (new) |
| `colour` | a colour | a swatch and a picker popover | new |
| `keys` | a key chord | the chord, captured by pressing it; conflicts said | the shortcuts editor's capture, as a field |

Ten kinds. A date, a secret and a slider over a curve are not in the
list until something needs them.

### 4.2 Layout and display bricks

| Brick | Renders like |
|---|---|
| `page` | A section's page: its title, its description, its groups; scrolls; the modified count and "Reset section" in its header. |
| `group` | A heading, an optional description, its rows in an inset box; collapsible. |
| `row` | Label and help on the start side, the control on the end side; the modified marker before the label (a bar on the start side, as VS Code); the origin tag; the reset button on hover and focus; the field's diagnostics under it. Every field, status and action is a row. |
| `callout` | A message with a tone (info, success, warning, danger) and an optional action, at the top of a page or group. |
| `status` | A row whose control is a value with a tone, a badge, a meter (used of total) or a relative time ("2 days ago"). |
| `action` | A button: primary (one per page at most), secondary, danger; or a link in a row. |
| `record list` | List items with a title, subtitle, trailing value, tone, row actions; an expander for row fields. |
| `record table` | egui-pop's data table with sorting, selection, row actions, a details pane. |
| `stepper` | The wizard's steps: on the left panel in the admin shell, as a header strip in a narrow window. |
| `footer` | A task's bar: presets menu, totals, Cancel, the run verb. |
| `progress` | A job's bar, "n of m", the item, time left, Cancel, the log toggle (the jobs centre's row). |
| `results` | Counts with tones, a record list of what failed or was skipped, follow-up actions. |
| `empty` | An empty state: a message, an illustration-free icon, an action. |

### 4.3 Registered widget kinds

A widget kind is registered in Rust (`ctx.widget_kind(id, kind)`) with
its value type and its options schema; the manifest places it like any
node. It must take part in Tab order, accessibility, search (by its
keywords) and reset. The ones the inventory needs:

| Kind | For |
|---|---|
| `file-browser` | Import's contents: [One file browser, many views](file-browser.md) as a widget kind (its view modes, file source and thumbnail provider), its value the ticked entries. |
| `source-list` | Import's sources: cards and cameras mounted, recent folders, favourites, "Choose a folder…". Could be a record list with a provider; kept as a candidate kind until the first slice shows it cannot. |
| `shortcuts` | The shortcuts editor: every command with its keys, capture, conflicts, layer. |
| `gestures` | The pointer gestures and their pad. |
| `theme-editor` | Today's theme editor page. |
| `font-picker` | The family picker with its live preview. |

Everything else in section 6 fits the bricks.

### 4.4 Kit components

New kit components, in the kit's vocabulary (`component.state.term`),
each based on an existing one where it can be:

- `admin-tab`, based on `tab`: the scope tabs (icon above or before the
  label), `selected` for the current scope;
- `admin-nav-item`, based on `list-item`: the left panel's entries,
  `selected` for the page shown, a muted count of modified values;
- `form-group`: the inset box of a group (`fill`, `border-*`, `radius`,
  `padding-*`);
- `form-row`: a row (`padding-*`, `hover`, `focus`), with
  `indicator-color`, `indicator-size` and `indicator-side: left` as the
  modified marker (state `selected` while modified, so a kit restyles it
  with data);
- `form-origin`: the origin tag, based on the indicators' pill;
- `callout`, with the four tones;
- `wizard-step`, based on `admin-nav-item`: `rest` (to do), `selected`
  (current), `active` (done), `disabled` (not reachable yet), and a tone
  for an invalid step.

The bricks never set a colour or a size of their own: what is not a kit
token comes from the theme's tokens.

## 5. Surfaces

### 5.1 The administration app

```text
+--------------------------------------------------------------------------+
| [gear] General  [brush] Interface  [kbd] Input  [lib] Library  [disk]    |
|  Storage  [shield] Backup  [chip] Hardware  [puzzle] Plugins  [heart]    |
|  Health  [list] Jobs  [code] Developer                  [search......]   |
+------------------+-------------------------------------------------------+
| Strategy      2  | Strategy                               Reset section  |
|   Copies         | +---------------------------------------------------+ |
|   Destinations   | | | Verified copies        [ 2 ]                     | |
| Status           | |   Off site required      [on ]                     | |
| Cards            | |   When                   [After each import  v]    | |
| Snapshots        | +---------------------------------------------------+ |
|                  |   Last backup           2 days ago     (warning)      |
|                  |                                 [ Back up now ]       |
+------------------+-------------------------------------------------------+
```

- **Horizontal axis**: the scopes as tabs with an icon and a label; past
  the window's width the labels go first (icons only, the label as a
  tooltip and accessible name), then an overflow menu.
- **Vertical axis**: the left panel lists the scope's sections; under the
  page shown, its groups as anchors; a muted count of modified values per
  entry.
- **The page**: generated from the section's nodes.
- **Full page, two modes** (Q1):
  - **overlay**: a layer over the whole main window (above the
    workbench, below the toasts), the workbench kept as it was; Escape or
    the close button returns; not a modal dialog of fixed size;
  - **window**: a separate system window (an egui viewport) with the
    frame's title bar, which goes full screen with F11 as the main one;
  - a "Pop out" button in the overlay moves it to a window, keeping the
    place; the mode used last is remembered.
- **Applying**: settings apply live, as today, with the settings' undo
  history (`Ctrl+Z` while the focus is in the app) and per-row reset;
  tasks apply only when run.

### 5.2 Standalone task surfaces: Import and Export

Import and Export are **task surfaces**: the same shell without the
scope tabs, opened by their own entry points (`Ctrl+Shift+I`,
`Ctrl+Shift+E`, a card's toast, a drop), as an overlay or a window like
the app. Their left panel is the task's steps; their footer is the task's
footer. Their defaults and presets also appear in the app's "Import and
export" scope, as plain sections (the same manifest nodes, placed twice:
a node may be **mounted** in more than one place, its path being the
first).

### 5.3 The wizard kit

**The contract.**

```text
wizard  id, title, icon
        draft: per library | per app | none
        steps: [step...]
        summary: auto | custom nodes
        run: command, verb, cancellable: never | until <phase> | always
        results: nodes, follow-ups
        rendering: steps | page | ask                        (Q4)

step    id, title, description
        nodes: fields, lists, widgets, statuses, callouts
        validate: hook (optional; descriptors' own checks always run)
        when: clause (the step exists only when it holds)
        optional: bool (Skip is offered)
        done_when: clause (for a checklist-like wizard)
```

**Behaviour.**

- **Navigation**: Back, Next, Skip (optional steps), and the stepper on
  the left: any step already reached can be gone back to by a click;
  later steps are greyed until the ones before are valid. Next is greyed
  with the first reason when the step is not valid (the diagnostic under
  the field, and in Next's tooltip).
- **Validation per step**: descriptors' checks, then the step's hook,
  run when a value changes (debounced) and when Next is pressed.
- **Summary**: generated from the steps' fields (label and value as
  shown, grouped by step), each group with "Change" going back to its
  step; or custom nodes. The run verb is on this step ("Import 248
  photos").
- **Run**: the command runs as a job; the step shows the job's progress
  brick and its log. Cancel is offered only while the declared
  `cancellable` phase lasts (an import cancels between files; a vault
  migration only before it writes); leaving the surface does not cancel,
  the job stays in the jobs centre and the surface can be reopened on
  it.
- **Results**: counts, the records that failed or were skipped, and the
  follow-ups.
- **Resumable**: the draft is kept as it changes (per library by
  default); reopening a wizard with a draft offers "Continue where you
  left" or "Start again". A run that was interrupted resumes if its
  command does (import skips what it already imported).
- **Deep links**: `admin.open {"path": "import/choices"}` opens the task
  on that step; a step that is not reachable yet opens the first invalid
  one before it, with its reason.
- **Searchable**: steps and their fields are in the search, as sections
  and fields are.
- **Two renderings of one wizard**: `steps` (one step per page, the
  stepper on the left) and `page` (every step as a column or a group on
  one page, Lightroom's import). The run, summary and results are the
  same.

**Import onto the kit.**

| Step | Nodes | Today |
|---|---|---|
| Sources | `source-list` (cards, cameras, recent, favourites, Choose a folder) | the left column |
| Contents | `file-browser` (by capture day, ticks, "New only", RAW+JPEG as one), status: count and size ticked | the middle column |
| Copy or add | choice: Copy into the library, Add where they are | top of the choices |
| Destination (when copying) | path (Into), choice (layout by date), preview hook (example path) | Into |
| Names (when copying) | choice of pattern presets, text (custom pattern), preview hook (example name) | Name |
| Options | choice (duplicates: skip, keep, ask), toggle (darktable edits), group "More" (metadata, time offset) | Duplicates, darktable edits, More |
| Backup (when a strategy exists; optional) | choice (back up after import), status (the destinations plugged in) | new, from the backup strategy and Capture One's "Backup To" |
| Summary | auto; totals (photos, size, space left on the destination) | the footer's totals |
| Run | `library.importFiles` with the draft; progress; cancellable between files | the import job |
| Results | imported, duplicates, skipped, warnings; follow-ups: Show in library, Back up now, Eject the card | a toast, the Duplicates and Log panels |

Presets are the task's: Default, saved ones, Save as, Delete, as the
footer has today.

**Export onto the kit.**

| Step | Nodes |
|---|---|
| Photos | status (the selection, else the images shown, and their count); choice (which) |
| Destination | path, choice (subfolder by date or none), choice (existing files: skip, overwrite, number) |
| File | choice (format), number (quality), choice (bit depth), choice (colour space) |
| Size | choice (original, long edge, megapixels), number |
| Names | pattern, preview hook |
| Metadata | multi (what to keep), toggle (remove location) |
| After | choice (nothing, open folder) |
| Summary, Run, Results | a parent job and one per image; failed ones as records with their reason; follow-up Open folder |

Plugins add export steps or groups (a format plugin adds its options to
File) by placing nodes under `export/file`.

**Other wizards.**

- **Backup setup**: copies, destinations (a record list of volumes with a
  backup role, "Mark this disk…"), off site, when; summary; run: the
  first backup.
- **First launch**: language, theme, library location (new or existing),
  "Bring a darktable library" (optional, leads to the migration), backup
  strategy (optional), summary.
- **darktable migration**: find `library.db` (path, detected candidates
  as choices), what to bring (multi: film rolls, tags, ratings and
  labels, edits as versions, styles), a dry run as a status, summary,
  run with progress (cancellable until it writes), results with what was
  left out and why.
- **Health fixes**: a finding whose fix needs choices opens a short
  wizard ("Plug in Backup A, then back up": wait for the disk as a step
  whose `done_when` is the disk mounted; run `library.backupNow`).

### 5.4 Search, deep links, keyboard, accessibility, the last place

- **Search** covers every scope at once: node titles, descriptions,
  keywords, setting ids, current values of choices, record titles of
  small sources (volumes, plugins), steps and their fields. Typing
  replaces the left panel with the hits grouped by scope and section, and
  the page with the matching rows from every section, each under a
  breadcrumb (`Backup › Strategy`), editable in place (VS Code's way).
  Filters: `@modified`, `@scope:library`, `@id:`, `@plugin:`. Every
  section, field and task is also a palette entry ("Settings: Verified
  copies", "Open: Storage › Volumes").
- **Deep links**: `admin.open {"path": "backup/strategy/copies"}`; a
  setting id alone (`{"key": "health.destinationDays"}`) resolves to its
  first place; a record (`{"path": "storage/volumes", "record": "<id>"}`)
  scrolls to and selects it; `{"mode": "window"}` picks the mode. The
  application's link scheme takes the same path (`helios://admin/backup/strategy`).
  `app.settings` and `app.settings.page` become aliases. Health fixes,
  toasts and error messages send the person to a place this way.
- **Keyboard**: `Ctrl+,` opens the app on the last place; `Ctrl+F` (or
  typing anywhere outside a field) goes to search; `Ctrl+PageDown` and
  `Ctrl+PageUp` move between scopes; `Ctrl+Tab` and `Ctrl+Shift+Tab` too;
  F6 cycles the regions (tabs, left panel, page, footer); arrows inside
  the left panel; `Alt+Left` and `Alt+Right` go back and forward through
  places; Escape clears the search, then closes the overlay. A wizard:
  `Alt+N` Next, `Alt+B` Back, Enter on the summary runs.
- **Accessibility**: the scopes are a tab list, the left panel a tree,
  each row a labelled control whose description carries its help, its
  origin and "modified"; statuses are live regions polite; a wizard step
  says "Step 3 of 7, Names". The test kit's "every interactive node has a
  name" covers every generated page for free.
- **The last place**: the scope, section, scroll and search per surface,
  kept with the interface's state; a task's draft per library; the mode
  (overlay or window) and the window's place.

## 6. Information architecture for Helios

Scopes in order, their icon, their sections, and what goes there. Value
scopes in brackets where not `app`. "(w)" is a registered widget kind,
"(page)" a custom page, "(new)" a GUI for something with none today.

1. **General** (gear)
   - Language and region: `appearance.language`.
   - Startup: `library.default` (the library opened), open the last
     workspace.
   - Notifications: `notifications.desktop`, `notifications.jobs`,
     `appearance.toast_corner`.
   - Tips: `tips.off`, the tips list with a switch per plugin.
   - About: version, licences, `logs.reveal`.
2. **Interface** (brush)
   - Theme: `appearance.theme_preference`, `theme.select` (choice),
     `appearance.accent_color`, Theme editor (w), `theme.exportMerged`.
   - Text and size: `appearance.text_size`, `appearance.interface_zoom`,
     `appearance.font_ui`, `appearance.font_mono` (w: font picker).
   - Window: `window.frame`, `window.bar_in_fullscreen`, `menus.icons`,
     `dialogs.movable`, `dialogs.closable`, `appearance.focus`,
     `appearance.scroll_bars`.
   - Workbench: `workbench.panel_alignment`,
     `workbench.side_bar_position`, `interface.panels.queue_when_empty`,
     `interface.graph.zoom` (new: no GUI today).
   - Profiles: a record list (`profile.use`, `profile.import`,
     `profile.export`) [workspace].
   - Experimental: `interface.experimental.workshops` (needs a restart).
3. **Input** (keyboard)
   - Shortcuts (w: shortcuts) [app, workspace].
   - Keymap packages: a record list (`keymap.package`,
     `keymap.package.remove`) (new).
   - Pointer and wheel: `input.pointer_profile`, `input.wheel_device`,
     `input.wheel_speed`, `input.slider_wheel`,
     `input.image_double_click`, Gestures (w).
4. **Library** (books)
   - Library: status (path, size, photo count), `library.openFolder`,
     `library.default`, `library.db`, catalogue integrity (status and
     `vault.integrity`).
   - Workspaces: a record list of recent workspaces (`workspace.open`,
     `workspace.new`), templates (`workspace.saveAsTemplate`),
     `workspace.importParts`, `workspace.reset` [workspace]. Editing the
     network stays in the workbench.
   - Image map: `system.map.exclude`, `system.map.defaults`,
     `system.map.record`, `system.map.limit`, `system.map.keepMissing`
     (new).
   - Decisions: `project.decisions.mode` [library] (new).
   - History: `system.workflow.keep_days` [library] (new).
5. **Storage** (disk)
   - Volumes: a record table from the volume registry (name, kind,
     capacity, free, roles, off site, RAID, node, last seen, photos), row
     fields, "Mark this disk…" (confirm hook), `library.storage`'s
     `volume` as a deep link to a record.
   - Caches: `system.cache.budget`, `system.cache.share`,
     `system.cache.thumbnails` (new).
6. **Backup** (shield)
   - Strategy: copies, destinations (choices hook over the volumes),
     off site, when [library]; "Set up backups…" (the backup wizard).
   - Status: photos short of copies, unverified copies, last backup,
     `library.backupNow`.
   - Destinations: `health.destinationDays`, the destination volumes
     with last seen.
   - Cards: cards safe to format.
   - Catalogue snapshots (new, when built): a record list (date, size,
     where copied), "Snapshot now", "Restore…" (confirm), how many kept.
7. **Import and export** (arrow in and out)
   - Import: open Import (the task), default copy or add, the import
     source [workspace], darktable edits.
   - Import presets: a record list (rename, delete, set default).
   - Import sources: favourites and recent folders.
   - Importers: the plugins' importers (`library.importWith`).
   - Export: open Export (the task), the workspace's export settings
     [workspace].
   - Export presets: a record list.
   - Migration: "Bring a darktable library…" (the wizard).
8. **Hardware** (chip)
   - GPU and compute: a record table (`compute.list`: device, role,
     budget, speed, enabled), row actions (`compute.enable`,
     `compute.disable`, `compute.role`, `compute.budget`),
     `compute.probe`, `compute.benchmark` (a job), routing
     (`compute.route`), remote nodes (`compute.remote`) (new).
   - Display: `display.wide-gamut-preview`, its status (what the window
     got and why).
   - Workers: `system.scheduler.workers`, `system.ai.calls`,
     `system.events.journal` (new).
9. **Plugins** (puzzle)
   - Installed: a record table (`plugin.list`: name, version, enabled,
     permissions), row actions (`plugin.enable`, `plugin.disable`), row
     fields (permissions: `plugin.grant`, `plugin.revoke`) (new).
   - Each plugin: a section of its own for the nodes it places nowhere
     else.
10. **Health** (heart)
    - Findings: a record list grouped by level, row actions "Fix…" (its
      command or wizard) and "Ignore"; "Check now".
    - Ignored: a record list with "Show again".
    - Checks: the registered checks and their last run.
11. **Jobs** (list) (Q3)
    - Running and recent: the jobs centre as a record table with Cancel,
      Retry, the log.
    - Journal: `system.events.journal`, retention.
12. **Developer** (code)
    - `developer.devtools`, `developer.agentSocket`, `developer.log_level`,
      `developer.show_keys`, `logs.reveal`; the debug bridge's state as
      statuses. Shown when devtools are on, or by search.

Standalone task surfaces: **Import** and **Export**. Wizards: backup
setup, first launch, darktable migration, health fixes.

What stays in the workbench: the viewport, the graph and its node
editing, the darkroom, the library grid and culling (ratings, flags,
labels, stacks: `vault.*`, `library.stacks.*`), the Parameters, Modules
and Masks panels, macros and workshops (`macro.*`, `workshop.*`), the
Queue panel, the live Jobs panel.

## 7. Migration

Each slice ships on its own, with its gallery page and snapshots, and
removes what it replaces.

1. **The model and the shell, on today's settings.**
   - egui-pop: the tree, bindings and manifest types in `egui-pop-model`
     (no egui); the shell, the bricks and their kit components in
     `egui-pop-api` (Q8), replacing the settings window. The generated
     sections become nodes under scopes (a default placement from the id's
     prefix, `appearance.*` to Interface, `input.*` to Input…), alter hooks
     keep working. `SettingsPage` becomes the `page` node unchanged; the
     shortcuts editor, gestures and fonts become widget kinds. `app.settings`
     and `app.settings.page` become aliases of `admin.open`. Overlay and
     window modes. Search, deep links, the last place.
   - The layered settings store (library and workspace layers), behind the
     same `AppHandle::setting`.
   - Helios: declares its scopes, and the sixteen `helios-config` settings
     through a bridge (concern to value scopes, the `run` layer as an
     origin), so they get a GUI for the first time.
2. **Storage and Health.** The volume table with row fields, the strategy
   form, the statuses and "Back up now"; the findings list with Fix and
   Ignore; their hand-drawn windows removed. `library.storage` and
   `help.health` open their sections. The status bar badge's click opens
   Health.
3. **The wizard kit, then Export, then Import.** The kit with the
   summary, run and results; Export first (small, already a Form, no new
   widget kind); then Import, which needs the file browser as a widget
   kind (the file browser design's slices first) and the source list. The
   import window's code maps onto the steps table in 5.3; its presets and
   remembered choices become the task's draft and presets.
4. **The rest.** Plugins, Hardware, keymap packages, profiles, Jobs,
   Developer; then the backup setup, the first launch and the darktable
   migration wizards; catalogue snapshots when the vault has them.

The **file browser** relates in one way only: it is a registered widget
kind (`file-browser`), its value the entries ticked, its options the
view modes allowed, the file source and the thumbnail provider. The
import task gives it a source over the ticked sources and Helios's
thumbnail provider; nothing in the file browser knows about forms.

## 8. Open questions

**Q1. Overlay or window by default.**
- A. Overlay of the main window, with "Pop out" to a window; the mode
  used last remembered.
- B. A separate window by default (Blender, GNOME, macOS, Windows).
- C. Overlay for the settings app, window for the task surfaces (an
  import runs beside the library).
- Recommended: **A**. It works the same on every platform and in the web
  build, keeps one window for people on a laptop, and the window is one
  click away; remembering the last mode serves people with two screens.

**Q2. Tabs on top or a sidebar for the scopes.**
- A. Tabs with icons on top, sections in the left panel (the owner's two
  axes; darktable, Lightroom, Capture One).
- B. One sidebar, scopes as headings and sections under them (macOS,
  GNOME, Windows).
- C. An icon rail on the far left for the scopes, the sections panel
  beside it (VS Code's activity bar).
- Recommended: **A**, as directed; twelve scopes fit as icon and label
  on a 1280-point window, icons alone below that.

**Q3. Where jobs live.**
- A. Live jobs stay in the workbench (the Jobs panel and the status
  bar's popover); the app's Jobs scope holds history, retention and the
  journal.
- B. Only the workbench; no Jobs scope.
- C. Only the app; the status bar's item opens it.
- Recommended: **A**. A running export is followed beside the photos;
  what ran last week is administration.

**Q4. Import as a wizard or a single page by default.**
- A. One page (Lightroom, Capture One: today's three columns, the steps
  as columns), "Guided" switching to steps, remembered.
- B. Steps by default.
- C. Steps for the first import of a library, one page after.
- Recommended: **A**. Photographers import daily and every photo tool
  they know is one page; the same manifest gives the guided mode for
  free, and the first launch wizard leads into it.

**Q5. The manifest's format.**
- A. Rust builders only.
- B. A data file (TOML or RON) per owner, hooks named.
- C. Rust builders producing a serialisable manifest; out-of-process
  plugins send the same manifest as JSON (or TOML in their package).
- Recommended: **C**. In-process code keeps types and compile errors;
  Helios's plugins are out-of-process binaries and must send data; agents
  read the same JSON.

**Q6. Where search sits.**
- A. At the top of the left panel (GNOME, macOS, Windows, JetBrains).
- B. In the scope tab row, at its end, searching every scope (always
  visible).
- C. At the top of the page (VS Code).
- Recommended: **B**. The search covers every scope, so it belongs above
  the scopes, not inside one; the palette also lists every section and
  field.

**Q7. When values apply.**
- A. Live, with undo and per-row reset, everywhere (today's settings).
- B. Apply, OK and Cancel per page (JetBrains, Lightroom).
- C. Live for settings and records; only on run for tasks and wizards.
- Recommended: **C**. It is what egui-pop does today for settings, and a
  task's choices are its arguments.

**Q8. Where the framework lives.**
- A. Model in `egui-pop-model`, shell and bricks in `egui-pop-api`,
  replacing `settings_window` and `settings_pages`.
- B. A new L4 crate, `egui-pop-forms`, on the API, with the settings
  window moved into it.
- C. As A, with the record bricks in `egui-pop-views`.
- Recommended: **A**. The settings window, the dialogs' Form and the
  pages are in the API crate today; a new crate would move every
  `egui_pop::api::settings_*` path for no gain. The record bricks use
  `egui-pop-views` through the facade, as the file browser does.

**Q9. How a field set in several value scopes shows it.**
- A. An origin tag on the row and a "Set for" menu (JetBrains' marker).
- B. Scope tabs inside the page (VS Code's User and Workspace).
- C. Separate sections per value scope ("This library", "This
  workspace").
- Recommended: **A**. The top tabs are the families; few fields have more
  than one value scope.

**Q10. The command namespace and the app's name.**
- A. `admin.*` (`admin.open`), titled "Settings" in menus with
  `Ctrl+,`.
- B. `settings.*`, titled "Settings".
- C. `admin.*`, titled "Settings and tools".
- Recommended: **A**. People look for "Settings" (every OS calls it so);
  the id says it holds more than settings.

## 9. Sources

- Blender: [Preferences](https://docs.blender.org/manual/en/latest/editors/preferences/index.html),
  [Add-on preferences](https://docs.blender.org/api/current/bpy.types.AddonPreferences.html).
- darktable: [Preferences](https://docs.darktable.org/usermanual/development/en/preferences-settings/),
  [Lua preferences](https://docs.darktable.org/lua/stable/lua.api.manual/darktable/darktable.preferences/),
  [Import](https://docs.darktable.org/usermanual/development/en/module-reference/utility-modules/lighttable/import/).
- Lightroom Classic: [Preferences](https://helpx.adobe.com/lightroom-classic/help/setting-preferences-working-lightroom.html),
  [Catalog settings](https://helpx.adobe.com/lightroom-classic/help/create-catalogs.html),
  [Import photos](https://helpx.adobe.com/lightroom-classic/help/import-photos-video-catalog.html),
  [Export](https://helpx.adobe.com/lightroom-classic/help/exporting-photos-basic-workflow.html),
  [SDK](https://developer.adobe.com/lightroom-classic/).
- Photoshop: [Preferences](https://helpx.adobe.com/photoshop/using/preferences.html).
- Capture One: [Preferences](https://support.captureone.com/hc/en-us/sections/360004127657-Preferences),
  [Importing images](https://support.captureone.com/hc/en-us/articles/360002556698-Importing-images).
- VS Code: [User and workspace settings](https://code.visualstudio.com/docs/getstarted/settings),
  [Contribution points: configuration, walkthroughs](https://code.visualstudio.com/api/references/contribution-points).
- JetBrains: [Settings](https://www.jetbrains.com/help/idea/settings-preferences-dialog.html),
  [Settings guide for plugins](https://plugins.jetbrains.com/docs/intellij/settings-guide.html).
- GNOME: [Settings](https://help.gnome.org/users/gnome-help/stable/),
  [libadwaita preferences widgets](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/),
  [Initial Setup](https://gitlab.gnome.org/GNOME/gnome-initial-setup).
- macOS: [System Settings](https://support.apple.com/guide/mac-help/change-system-settings-mh15217/mac),
  [Migration Assistant](https://support.apple.com/en-us/102613).
- Windows: [Launch the Settings app (`ms-settings:`)](https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-settings-app),
  [Settings cards (Windows Community Toolkit)](https://learn.microsoft.com/en-us/dotnet/communitytoolkit/windows/settingscontrols/settingscard),
  [Inno Setup wizard pages](https://jrsoftware.org/ishelp/),
  [WiX dialogs](https://wixtoolset.org/docs/tools/wixext/wixui/).
- Declarative forms: [JSON Forms, UI schema and categorization](https://jsonforms.io/docs/uischema/),
  [react-jsonschema-form](https://rjsf-team.github.io/react-jsonschema-form/docs/),
  [Qt property browser](https://doc.qt.io/archives/qq/qq18-propertybrowser.html),
  [KConfigXT](https://develop.kde.org/docs/features/configuration/kconfig_xt/).
