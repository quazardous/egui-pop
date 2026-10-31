# File dialogs survey: what makes a good file picker

This survey feeds the in-app file picker of egui-pop
([Choosing files](files.md)). It compares the system dialogs (GTK and the
desktop portal, KDE, Windows, macOS) and the applications that draw their
own browser (Blender, darktable, Lightroom, JetBrains IDEs, VS Code,
Total Commander, file managers), asks what each does about layout,
preview, search, places, keyboard, saving, selection and remembered
state, and ends with a ranked list of changes for egui-pop's picker.
egui-pop's main user is a photo editor, so photo workflows weigh in the
ranking.

Read on 2026-10-29. Behaviour quoted from bug trackers and forums is
marked as such; it may have changed since.

## Summary

- **A dialog keeps its size; panes share it.** Every system that has a
  preview pane (KDE's F11 pane, the Windows Explorer preview pane,
  Finder's preview column, Dolphin's information panel) puts it inside
  the window and takes the room from the file view, behind a splitter.
  None grows the window when the pane appears. KDE's long trail of bugs
  about a preview pane that "makes itself bigger" each time the dialog
  opens shows what people think of a pane that changes the dialog's
  size. egui-pop's picker widens its modal by the pane's width when the
  preview is on: it should keep its width and narrow the list.
- **The dialog is sized by the person, and remembered.** GTK stores the
  dialog's size, the sidebar's width, the view, the sort and hidden
  files in its settings; Windows remembers the last folder per purpose
  (`SetClientGuid`), macOS per panel `identifier`. egui-pop's picker is
  a fixed 640-point modal with a list 280 points high.
- **Two ways in, one field.** All good dialogs let the person either
  browse (click, arrows) or type (a path, a name). Where they fail is
  when typing has three meanings: GTK turned off type-ahead in 3.16
  because it was "the third kind of navigation entry ... after the
  location entry and search, and it interacts badly with both"; Nautilus
  replaced type-ahead with recursive search and people have patched it
  back for ten years. egui-pop's single field (a fuzzy filter that
  becomes a path with `/` or `~`, `Tab` completes) is already the
  pattern that avoids this; keep it.
- **Photo pickers are grids.** GTK waited 18 years (2004 to GTK 4.10) for
  a thumbnail grid in its chooser; that wait is the most quoted
  complaint about it. Lightroom's and darktable's import dialogs,
  Blender's browser and every file manager offer thumbnails with a size
  control. egui-pop's picker shows names only, with one preview at a
  time.
- **Saving has rules everybody follows**: a proposed name with its stem
  selected, the extension following the chosen type, a confirmation
  before replacing a file, a button to create a folder. egui-pop's
  picker has none of the last three.
- **Photo specifics**: RAW and JPEG (or RAW and XMP sidecar) shown as one
  item (Lightroom's default, darktable's sidecars, digiKam's grouping),
  ratings and EXIF in the preview, "only new" in an import. The picker
  can carry the first two through hooks; the third belongs to the
  application's import, not to a picker.

## egui-pop today

From the code on `main` (`crates/egui-pop-api/src/files.rs` and
`files/`):

- A modal (`OverlaySpec::width`) 640 points wide, plus
  `preview::PANE_WIDTH` (240) when the preview is on; it is not
  resizable and its list is 280 points high.
- Top: an Up button, a Bookmark button and the folder as monospace text.
  Then one field: a fuzzy filter, or a path when it holds `/` or starts
  with `~`; `Tab` completes, completions show as small buttons.
- Left (150 points): Home, Recent places, Bookmarks.
- Middle: one name per row, folders first then files, sorted by name
  (case-insensitive, not natural). A single click on a folder enters
  it; on a file it highlights, toggles (choosing several) or copies the
  name into the field (saving).
- Right: the preview pane (providers off the interface thread, cached),
  `Alt+P` or a check box, remembered in `files.preview`.
- Bottom: the type filter (combo), Hidden files, Preview, the verb and
  Cancel.
- Keys: arrows, Enter (enter a folder or choose), Backspace in an empty
  field or `Alt+Up` (parent), `Ctrl+Enter` (accept), `Ctrl+Space`
  (select, several), `Ctrl+D` (bookmark), Escape.
- Saving: Enter in the name field chooses `folder/name` at once; no
  extension is added, an existing file is replaced without a question,
  and there is no way to create a folder.

## The systems

### GTK 4 and the desktop portal

- **Layout.** Places sidebar; a path bar of buttons that becomes a
  location entry with `Ctrl+L`, or when `/` or `~` is typed (the
  `location-popup` signal's default bindings,
  [docs](https://docs.gtk.org/gtk4/signal.FileChooserWidget.location-popup.html)).
  A list with columns (name, size, type, modified; a sortable Type
  column since 2019), and since GTK 4.10 a grid with thumbnails
  ([GTK blog, "A grid for the file chooser"](https://blog.gtk.org/2022/12/15/a-grid-for-the-file-chooser/);
  the request was
  [bug 141154](https://bugzilla.gnome.org/show_bug.cgi?id=141154),
  opened in 2004).
- **Preview.** GTK 4 removed the application preview widget of GTK 3,
  because a sandboxed (portal) chooser cannot draw the application's
  widget; the advice is to embed a `GtkFileChooserWidget` and put one's
  own preview beside it
  ([GNOME Discourse](https://discourse.gnome.org/t/file-chooser-gtk-4-image-preview/11510)).
- **Typing.** Typing in the list starts a search (recursive, through the
  indexer when present). Type-ahead was turned off in 3.16
  ([commit](https://lists.gnome.org/archives/commits-list/2015-January/msg05538.html));
  the complaints and the distribution patches restoring it ran for
  years ([Launchpad 1592177](https://bugs.launchpad.net/ubuntu-gnome/+bug/1592177),
  [patch thread](https://lists.gnome.org/archives/gtk-devel-list/2015-August/msg00034.html)),
  and the GTK 4 entry had its own bugs where `~` searched instead of
  opening the location entry
  ([issue 4911](https://gitlab.gnome.org/GNOME/gtk/-/issues/4911)).
- **Remembered.** The `org.gtk.gtk4.Settings.FileChooser` schema keeps
  `window-size`, `sidebar-width`, `sort-column`, `sort-order`,
  `view-type` (list or grid), `show-hidden`, `sort-directories-first`
  ([schema](https://forge.a-lec.org/jean/Learning_GTK4_tree/src/branch/master/gtk/org.gtk.gtk4.Settings.FileChooser.gschema.xml)).
  `Ctrl+H` toggles hidden files.
- **Saving.** `GtkFileDialog` takes `initial-name`, `initial-folder`,
  `initial-file`, filters and an accept label
  ([docs](https://docs.gtk.org/gtk4/class.FileDialog.html)); GTK 4's
  chooser always confirms an overwrite.
- **Portal.** `org.freedesktop.portal.FileChooser` passes filters, a
  current filter, `choices` (combos and check boxes the dialog shows for
  the application), `current_name`, `current_folder` (which "the portal
  implementation is free to ignore"), `current_file`, `directory`,
  `multiple`; `SaveFiles` saves several names into one folder
  ([spec](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html)).
  No preview, no per-purpose memory.

### KDE (KFileWidget, Dolphin)

- **Layout.** Places panel; a breadcrumb navigator that becomes an
  editable path (`Ctrl+L`); icon, compact and detailed views with a zoom
  slider; a filter combo and a name field at the bottom.
- **Preview.** An aside preview pane toggled with F11 (inline thumbnails
  with F12), inside the dialog behind a splitter
  ([D10314](https://phabricator.kde.org/R241:0792ed68923b3c3eac6fd3b9769e83ed2de9842b)).
  Its size has been a recurring bug: the pane grew each time the dialog
  opened ([bug 185764](https://bugs.kde.org/show_bug.cgi?id=185764),
  [bug 160400](https://bugs.kde.org/show_bug.cgi?id=160400)), the places
  panel crept by a pixel per opening
  ([D12592](https://phabricator.kde.org/D12592)), and in 2026 people
  still report the pane opening at nearly the whole window and its size
  not kept ([KDE Discuss](https://discuss.kde.org/t/file-dialog-will-not-remember-preview-f11-size/48327)).
  The lesson is twofold: the pane belongs inside a fixed window, and its
  width must be stored and restored exactly.
- **Dolphin** has the same Information panel (F11): preview, then
  metadata (size, dates, rating, tags, image dimensions), as a panel
  that narrows the view.

### Windows (Common Item Dialog, Explorer)

- **Layout.** Navigation pane, a breadcrumb address bar that becomes
  editable when clicked, Explorer's views (details, list, icons of
  several sizes), a name field and a type combo at the bottom. The
  dialog hosts Explorer's view, so sorting, columns and natural sort
  (`StrCmpLogicalW`: "3.txt" before "10.txt") are Explorer's.
- **Remembered.** The dialog opens in the last folder used; since Vista
  per purpose with `SetClientGuid`, because "a video editing application
  would present the same folder in the Render As dialog as it would in
  the Import Media dialog". Microsoft advises `SetDefaultFolder` (first
  time only) over `SetFolder` (always), which hides the last location
  ([Common Item Dialog](https://learn.microsoft.com/en-us/windows/win32/shell/common-file-dialog)).
- **Saving.** `FOS_OVERWRITEPROMPT` is on by default; the default
  extension is appended to a typed name and follows the chosen type
  ("If the user chooses a different file type, the extension
  automatically updates"). `SetSaveAsItem` opens in the edited
  document's folder with its name.
- **Application controls.** `IFileDialogCustomize` adds check boxes,
  combos, radio lists and choices on the Open button ("Open as
  read-only"); `OnFileOk` lets the application refuse a choice and keep
  the dialog open.
- **Preview pane.** In Explorer, `Alt+P` shows a preview pane on the
  right that narrows the file view
  ([NinjaOne](https://www.ninjaone.com/blog/show-or-hide-the-preview-pane-in-file-explorer-in-windows-11/));
  the window keeps its size.
- **Typing in the list.** By default typing selects the first item whose
  name starts with the letters; a Folder Options setting sends them to
  the search box instead
  ([NinjaOne](https://www.ninjaone.com/blog/how-to-change-when-typing-into-list-view-action/)).

### macOS (NSOpenPanel, NSSavePanel, Finder)

- **Layout.** Sidebar; icon, list, column and gallery views. In column
  view the last column previews the selected file; Space opens Quick
  Look from an open panel as from Finder
  ([Apple Community](https://discussions.apple.com/thread/2631679)).
  Typing `/` or `~` opens "Go to Folder", where Tab completes;
  `Cmd+Shift+.` shows hidden files
  ([Macworld](https://www.macworld.com/article/628623/how-to-jump-to-folder-mac.html)).
- **Saving.** The save panel opens collapsed (name, tags, location
  popup) and expands to a full browser (`isExpanded`); it has a New
  Folder button (`canCreateDirectories`), a "Hide extension" box
  (`canSelectHiddenExtension`, `isExtensionHidden`), a format popup
  (`showsContentTypes`), `allowsOtherFileTypes`, an `accessoryView` for
  the application's options, and an `identifier` under which the panel
  keeps its state
  ([NSSavePanel](https://developer.apple.com/documentation/appkit/nssavepanel)).
- **HIG.** "Use the default file browser unless you have an important
  reason to create a custom one"; a custom one should let people reach
  the whole file system even if it opens on the most relevant place,
  offer recent items, filters and multiple selection, name the Open
  button after the task ("Insert"), and give the save interface a name,
  a format and a location; extensions are hidden by default but people
  can show them
  ([Human Interface Guidelines, File management](https://developer.apple.com/design/human-interface-guidelines/file-management)).

### Blender's file browser

- **Layout** ([manual](https://docs.blender.org/manual/en/4.1/editors/file_browser.html)):
  a path region on top (back, forward, parent, refresh, new folder,
  path, search, display mode and settings), a source list on the left
  (Volumes, System, Bookmarks, Recent), the files, an execution region
  at the bottom (name, Cancel, the verb), and on the right the calling
  operator's options (an import's settings live in the browser).
- **Views.** Vertical list, horizontal list, thumbnails, with a Display
  Size; sort by name, extension, date or size; recursion of one to three
  levels shown flat; filters by type, hidden files.
- **Keys.** `Ctrl+F` search (wildcards), Backspace or `Alt+Left` back,
  `P` or `Alt+Up` parent, `Ctrl+L` edit the path with Tab completion,
  Tab completion in the name field, `+` and `-` change the trailing
  number of the file name (saving numbered versions).

### darktable's import dialog

- Three panes: places (home, pictures, mounted devices; `+` and `-`),
  a folder tree from the place's root, and the folder's files, all
  selected at first. Thumbnails on demand (an eye icon per file).
  "Select only new" (not yet in the library), "recursive directory",
  "ignore non-raw images", a "from XMP" column, rating and metadata to
  apply. The last place and folder come back; Enter imports
  ([manual](https://docs.darktable.org/usermanual/development/en/module-reference/utility-modules/lighttable/import/)).
- Forum reports ask for faster listing and for choosing several folders
  at once ([pixls.us](https://discuss.pixls.us/t/import-module-is-too-slow-and-needs-multiple-selection-for-folders/41384)).

### Lightroom Classic's import window

- Source panel (devices, folders, "include subfolders"), a grid of
  thumbnails with a check box each and a Thumbnails size slider, a
  loupe on one image, sort, and on the right the destination and
  options; a compact window without thumbnails
  ([Adobe help](https://helpx.adobe.com/lightroom/help/import-photos-video-catalog.html)).
- Files already in the catalogue show greyed out when "Don't import
  suspected duplicates" is on
  ([Adobe Community](https://community.adobe.com/t5/lightroom-classic-discussions/duplicate-lightroom-imports/m-p/9006705)).
- RAW and JPEG of the same name become one photo, the JPEG a sidecar,
  unless "Treat JPEG files next to raw files as separate photos" is set
  ([Adobe help](https://helpx.adobe.com/lightroom-classic/desktop/import-photos/file-import-formats-settings.html)).
  digiKam groups such pairs on demand ("Group Selected by Filename") and
  hides declared sidecar extensions as part of the main file
  ([digiKam docs](https://docs.digikam.org/setup_application/metadata_settings.html)).

### JetBrains IDEs, VS Code, Sublime Text

- **JetBrains' chooser**: a tree with a path field above it (`Ctrl+P`
  hides or shows it), a toolbar toggle for hidden files, and dropping a
  file from the system's file manager onto it navigates there
  ([IntelliJ IDEA 2017.2 help](https://www.jetbrains.com/help/idea/2017.2/select-path-dialog.html)).
- **VS Code's simple dialog** (`files.simpleDialog.enable`, and the
  dialog of remote workspaces): a quick pick where one types a path,
  segment by segment, with completion; a missing folder can be created
  ("Would you like to create it?"); it was turned on for automation
  because "native macOS dialogs cannot be driven" by tools
  ([PR 318488](https://github.com/microsoft/vscode/pull/318488)). This is
  egui-pop's field, and its reason (an agent answers it) is egui-pop's.
- **Sublime's and VS Code's Quick Open** (`Ctrl+P`): fuzzy matching over
  the whole project, not one folder. It suits a known project; a picker
  starting in a folder of photos does not need it.

### Total Commander, Far, Nautilus

- **Total Commander** separates quick search (`Ctrl+Alt` and letters:
  jump to the next match) from quick filter (`Ctrl+S`: hide what does
  not match), and lets the person switch and remember which
  ([forum](https://www.ghisler.ch/board/viewtopic.php?p=204322)). Far
  Manager works the same way (`Alt` and letters).
- **Nautilus** removed type-ahead in 2013 for a recursive search; the
  request to bring it back ([issue 466](https://gitlab.gnome.org/GNOME/nautilus/-/issues/466),
  [issue 1371](https://gitlab.gnome.org/GNOME/nautilus/-/issues/1371))
  and patched builds
  ([Linux Uprising](https://www.linuxuprising.com/2020/06/how-to-install-nautilus-with-type-ahead.html))
  never went away.

## Compared

| | GTK 4 / portal | KDE | Windows | macOS | Blender | darktable import | Lightroom import | egui-pop today |
|---|---|---|---|---|---|---|---|---|
| Views | list, grid (4.10) | icons, compact, details | details, list, icons | icons, list, columns, gallery | lists, thumbnails | files list + on-demand thumbnail | grid, loupe | names only |
| Thumbnail size | no | zoom slider | icon sizes | slider | Display Size | no | slider | no |
| Preview | none (app widget removed) | F11 aside pane, splitter | Explorer `Alt+P` pane | column view's last column, Quick Look | thumbnails | eye icon | loupe | aside pane, `Alt+P` |
| Preview changes window size | no | no (bugs where the pane grew) | no | no | no | no | no | **yes, +240 points** |
| Path | path bar, `Ctrl+L`, `/`, `~` | breadcrumbs, editable | breadcrumbs, editable | Go to Folder on `/`, `~` | path field, `Ctrl+L` | folder tree | source panel | text label + field |
| Typing in the list | recursive search | jump | jump (or search, a setting) | jump | `Ctrl+F` filter | no | no | fuzzy filter in one field |
| Places | sidebar, bookmarks, recent | places panel | navigation pane, `AddPlace` | sidebar | bookmarks, system, volumes, recent | places + tree | sources | home, recent, bookmarks |
| Sort | columns, folders first (setting) | columns | columns, natural | columns | name, extension, date, size | name | capture time, name… | name, folders first |
| Save: extension | follows filter | auto-extension option | appended, follows type | hide extension box | n/a (operator) | n/a | n/a | not added |
| Save: overwrite question | always | yes | `FOS_OVERWRITEPROMPT` | yes | yes | n/a | n/a | **no** |
| New folder | yes | yes | yes | yes | yes | no | yes (destination) | **no** |
| App options in the dialog | portal `choices` | custom widget | `IFileDialogCustomize` | `accessoryView` | operator options | the import's own | options panels | inline mode only |
| Remembered | size, sidebar, view, sort, hidden | size, view, pane width | last folder per GUID | state per identifier | per operator | last place and folder | last source | preview on/off, recent, bookmarks |
| Sidecars as one | no | no | no | no | no | XMP read on import | RAW+JPEG one photo | no |

## What egui-pop's picker should take, ranked

Sizes: S is under a day, M a few days, L a week or more. The layering
note matters for several items: the picker lives in `egui-pop-api` (L3),
the data table in `egui-pop-views` (L4) and the thumbnail grid in
`egui-pop-core` (L2, moved down from `egui-pop-viewport` for item 6);
reuse means moving shared drawing down or the picker up, which is a
decision of its own (item 7 moved the header's drawing down, not the table).

1. **Keep the dialog's size when the preview is toggled; the preview
   takes its room from the list.** The modal's width stops depending on
   `show_preview`; the row beside the places splits its width between
   the list and the pane, and the pane's width is a share (a default
   such as 35 %, with a minimum list width under which the pane is
   hidden rather than the list crushed). Every system in this survey
   does it this way; KDE's bugs show the cost of not doing it. Size: S.
2. **A resizable dialog whose size, pane widths and view are
   remembered.** The list's 280-point height and the 640-point width
   become the default of a size the person drags, stored with the
   settings (as GTK's `window-size` and `sidebar-width`), restored
   exactly, and clamped to the window. A splitter between list and
   preview, its share remembered. Needs a resizable modal in the overlay
   layer (`OverlaySpec` has a width only). Size: M.
3. **Saving done right.** A confirmation before replacing an existing
   file (on by default, as `FOS_OVERWRITEPROMPT` and GTK 4; an
   application may turn it off when it asks itself); the active
   filter's first extension appended to a name typed without one, and
   changed when the filter changes (Windows' rule); the proposed name's
   stem selected when the dialog opens; a New Folder button (and
   `Ctrl+Shift+N`) in save and folder modes. Size: M.
4. **Last folder per purpose.** `FileRequest::purpose("import")` (or an
   id) under which the picker keeps its last folder, so an import and an
   export do not share one, as `SetClientGuid` and the panel
   `identifier` do; the request's `directory` becomes the first-time
   default (`SetDefaultFolder`), with an explicit way to force it.
   Recent places stay global. Size: S.
5. **Single click selects, double click or Enter opens.** A single click
   entering a folder is a file-manager habit of some Linux settings, not
   of any dialog here; it makes a mis-click a navigation and breaks
   multiple selection. With it, the usual selection keys for choosing
   several: `Ctrl`+click toggles, `Shift`+click and `Shift`+arrows
   extend, `Ctrl+A` selects all, and the footer says how many are
   selected (`Ctrl+Space` stays). Size: S.
6. **A thumbnail grid with a size control, for photos.** A list / grid
   switch (remembered), the grid drawing the preview providers' images
   as thumbnails (the cache and the off-thread worker exist), a size
   slider and `Ctrl`+wheel or `Ctrl` `+`/`-`, virtualised for folders of
   thousands of raws, a placeholder until a thumbnail is there. This is
   the feature GTK users waited 18 years for, and the one a photo
   editor's open and import dialogs need most. Decided: `thumbnail_grid`
   moved down to `egui-pop-core`, for the picker and the viewport crate
   alike. Size: L.
7. **Columns in the list: size, modified, type, sortable.** Clicking a
   header sorts, the sort is remembered, folders stay first (a setting,
   as GTK's `sort-directories-first`), and names sort naturally
   ("IMG_9" before "IMG_10", as Explorer). Photo folders are read by
   date as often as by name. Decided: the data table stays out, for three
   reasons. It lives a layer above the picker; it brings its own selection,
   which is not the picker's highlight and selection; and its headers cycle
   through no order at all, which a listing does not have. What they share went
   down to `egui-pop-core`: the header's drawing (`widgets::SortHeader`). The
   picker's rows use the grid's virtualisation (`ThumbnailGrid`, one cell to a
   row), so a row now reads its facts only when in view. A click sorts the
   largest and the newest first, as Finder and Nautilus; the headers stay above
   the grid, which has no other way to change the order. Natural order is our
   own 30 lines: `natord`'s last release is from 2015, and `alphanumeric-sort`
   compares by Unicode scalar values, which leaves case apart. Size: M.
8. **Breadcrumbs for the folder, editable on demand.** The monospace
   path becomes clickable segments (each opens its folder; a segment's
   menu lists its subfolders, as Windows' address bar); `Ctrl+L`, or
   typing `/` or `~` when the list has the focus, puts the path into the
   field. GTK and macOS both open path entry on `/` and `~`; egui-pop's
   field already understands both, so this is the entry point, not a
   second field. **Done:** the segments sit beside the row's controls and
   collapse from the start behind a `…` menu, so the deepest stay; a chevron
   after each is a stop of its own, which costs two Tab stops per segment
   against one with a key on the segment, for a menu anyone can find, by the
   pointer too. Its menu reads the folder as it opens, not each frame, and a
   press on a chevron or on List or Grid hands the keyboard back to the field.
   `Ctrl+L` is the command `file.edit_path`. A crates.io search (2026-10-31) found
   no egui breadcrumb widget to adopt, and `TextEdit` stays the field. Size: M.
9. **Grouped files: RAW with its sidecars as one entry.** A grouping
   hook on the files service (an application declares that `x.xmp`,
   `x.cr3.xmp` or `x.jpg` belong to `x.cr3`), the entry showing a badge
   ("+XMP", "+JPEG") and choosing it giving the main file (with the
   group available to the application). egui-pop provides the
   mechanism, the application the rules; Lightroom's default and
   darktable's sidecars show the need. Size: M.
10. **Application options inside the dialog.** A slot where the
    application draws its options next to the buttons (Windows'
    customize interface, the portal's `choices`, `accessoryView`,
    Blender's operator options), first as the portal's shape (combos and
    check boxes, given back with the paths) so that it works with the
    system's chooser too, then as free drawing for the in-app picker.
    Helios's import ("recursive", "only new") and export (format) need
    it. Size: M.
11. **Richer places.** The system's folders (Pictures, Desktop,
    Downloads from the XDG user dirs, Known Folders, the macOS
    standard directories), mounted volumes and removable cards (a
    camera card is the first place of a photo import), bookmarks
    reordered and renamed, a folder dropped on the sidebar bookmarked.
    Size: M.
12. **Preview facts for photos.** The preview pane already shows
    provider facts; make the fact list ordered and typed (dimensions,
    capture date, camera, lens, exposure, rating as stars) so that a raw
    provider from the application shows what Dolphin's panel and
    Lightroom's loupe show, the same way for every provider. Size: S.
13. **Small keyboard parity.** `Ctrl+H` (GTK, KDE) and `Cmd+Shift+.`
    (macOS) for hidden files, `F5` to reload, `Alt+Left` and `Alt+Right`
    back and forward (Blender, Windows), `Ctrl+L` to the field, the
    `Alt+P` already there; listed in the dialog's tooltips and in the
    keymap so they can be rebound. Size: S.
14. **Accessibility.** The list exposed as a list box (or a grid, in grid
    view) with its selected and current items, the folder change and the
    count of entries announced, the preview pane named, every action
    reachable by keyboard (already mostly true), and target sizes kept
    in the grid. Ongoing; size S per step.

### What to skip, and why

- **Recursive search as what typing does** (GTK, Nautilus): the source
  of the longest-running complaints in this survey, and slow on a
  photo archive. Keep the field filtering the folder; a recursive search
  can come later as an explicit command, never as the default meaning
  of typing.
- **A separate type-ahead next to the filter** (Total Commander's two
  modes, GTK 3's three entries): one field with one meaning per prefix
  is clearer; the fuzzy filter already jumps the highlight to the best
  match.
- **Column (Miller) view.** macOS's column view is loved by some, but it
  needs width the dialog does not have and adds little over breadcrumbs
  plus a list for picking one file.
- **Full-screen Quick Look on Space.** Space is taken (`Ctrl+Space`
  selects), and the preview pane plus a larger grid covers the need;
  revisit if people ask.
- **Import features** (copy and rename on import, duplicates greyed out,
  "only new"): they belong to Helios's import, which can draw them
  through the options slot (item 10) and the grouping hook (item 9),
  not to a generic picker.
- **Hiding extensions** (macOS's default): a photo editor's users need to
  see `.cr3` from `.jpg`; show extensions always.

## Sources

The sources are linked where they are used above. The main ones:
GTK's [`location-popup`](https://docs.gtk.org/gtk4/signal.FileChooserWidget.location-popup.html)
and [`GtkFileDialog`](https://docs.gtk.org/gtk4/class.FileDialog.html)
docs, the [portal's FileChooser](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html),
Microsoft's [Common Item Dialog](https://learn.microsoft.com/en-us/windows/win32/shell/common-file-dialog),
Apple's [NSSavePanel](https://developer.apple.com/documentation/appkit/nssavepanel)
and [HIG](https://developer.apple.com/design/human-interface-guidelines/file-management),
[Blender's manual](https://docs.blender.org/manual/en/4.1/editors/file_browser.html),
[darktable's manual](https://docs.darktable.org/usermanual/development/en/module-reference/utility-modules/lighttable/import/),
and the KDE, GNOME and Launchpad bug reports cited in each section.
