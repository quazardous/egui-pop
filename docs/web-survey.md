# Web survey: the gallery in a browser, and reporting from it

This survey lists every problem egui-pop has, or can have, in its web
build (`wasm32-unknown-unknown`, eframe's web runner), and ranks them by
one goal: the gallery on the web
([quazardous.github.io/egui-pop/gallery](https://quazardous.github.io/egui-pop/gallery/))
should be an easy way for anyone to report a bug or suggest an
improvement. A problem that stops a visitor from reaching the inspect
picker, copying what they see, or opening an issue weighs more than one
that only makes a page differ from the desktop.

It covers the reporting path itself, keys, platform services, storage,
rendering and performance, media and embeds, accessibility, and a walk
through every gallery page, and ends with proposed follow-ups ranked by
value for that goal and by cost.

Read and run on 2026-11-04, against egui-pop's `main` (a6e6ed24), egui
and eframe 0.37.0, wgpu 30.

**How much of this was run.** The gallery was built for the web from
`main` (release, `wasm32-unknown-unknown`, wasm-bindgen 0.2.129: the same
crate and features as the CI job "Gallery for the web", without Trunk's
packaging), served locally and driven headless with Playwright in three
browsers on Linux: Chromium 140 (Playwright's headless shell, WebGL 2
through SwiftShader), Chrome 157 (headless, WebGPU on SwiftShader) and
Firefox 141 (Playwright's build, WebGL 2). The deployed site was loaded
once more to time its start. Safari, Windows and macOS were **not run**;
what is said of them is read. Every finding says where it comes from:

- **[browser]**: reproduced in the headless browsers above (the browsers
  named when they differ).
- **[read]**: from egui-pop's or eframe's source, or a document; not seen
  happening. Treat it as a strong hypothesis.
- **[upstream]**: documented by egui, eframe, a browser or a standard,
  linked.

Headless browsers inject keys into the page, below the browser's own
shortcuts: a run shows what the page receives and whether eframe cancels
the browser's default action (`defaultPrevented`, read by a capture
listener on `window`), never what the browser itself then does with a
key it keeps. That last part is read from the browsers' documentation.

**Already in progress** (a separate change, not repeated here): the
browsers take Ctrl+Shift+C (inspect) and F12 (devtools) for themselves;
the gallery turns devtools on by default; and on the web the user's
keymap and settings are lost on reload, because `App::builder().persist(dir)`
needs a folder and only the layout goes to eframe's storage. Rows that
touch them say "in progress".

**Already known**: the application knows it is on the web only at
compile time (`cfg!(target_arch = "wasm32")`); keymap `when` conditions
have no `isWeb`; egui knows the operating system under the browser
(`Context::os`), but nothing knows which browser.

## Summary

- **The gallery runs everywhere it was tried.** It starts in under 3 s
  locally and on the deployed site (Chromium, a fast link), and all 58
  pages draw in Chromium and Firefox with no panic and no error, only
  warnings ([browser]).
- **The reporting path exists but is hidden and split.** Inspect,
  Copy, and About > Copy version info all work on the web ([browser]),
  but the inspect chord belongs to the browser and devtools are off (both
  in progress), the inspect text has no version or platform, the version
  info says `web (wasm32)` without the browser or the OS, "Report a
  problem" opens the bare issue list with nothing filled in, the Help
  menu has no entry for it, and the repository has no issue template.
- **Most bound keys also reach the browser.** eframe cancels the
  browser's action only for a fixed list (Ctrl with S, O, P or Comma;
  Space; Tab; Backspace; the arrows; text). Its `should_prevent_default`
  option is never asked for key presses ([browser], [read]). So Ctrl+F
  opens the find bar as well as the editor's search, F6 sends the focus
  to the address bar, F5 reloads the page and loses the session,
  Ctrl+Plus zooms the page, Ctrl+D offers to bookmark, Ctrl+Shift+Delete
  opens "clear browsing data" ([read], from each browser's shortcut list).
- **A reporter cannot point at a page.** There is no URL per page: a
  link always opens the overview, and a reload goes back to it
  ([browser], [read]).
- **The first load is heavy.** The deployed module is 33.6 MB (13.3 MB
  gzipped on the wire); nothing shows while it loads ([read], measured).
- **No screen reader can read the canvas.** eframe's web runner drops
  AccessKit's updates ([read], [upstream]).
- **Platform services degrade quietly, mostly well.** Copy and paste of
  text work ([browser]); notifications fall back to toasts; files come
  from an in-memory demo folder; there is no file drop, upload or
  download; full screen, quit and window commands do nothing on the web
  but stay enabled ([browser]).

## The reporting path today

What a visitor does now to report a bug from the web gallery, step by
step, with what each step costs. Measured in headless Chromium unless
said otherwise.

1. **Know that the tools exist.** Nothing on the landing page or in the
   gallery says how to report: the Help menu has "egui-pop on GitHub",
   "Documentation", "Tips..." and "About egui-pop gallery"; the "Report a
   problem" link is inside the About dialog and the About page only
   ([browser]). The Debug bridge page is the one place that says "Turn
   on Settings > Developer > Devtools to inspect" ([browser]).
2. **Turn devtools on.** Settings (Ctrl+Comma) > Developer > Devtools.
   Until then View > Devtools (F12) and View > Inspect an element
   (Ctrl+Shift+C) are greyed with no reason given, and the chords do
   nothing at all, not even a toast ([browser]). On by default: in
   progress. The setting is lost on reload (in progress).
3. **Enter the inspect mode.** Ctrl+Shift+C works in a headless browser
   ([browser]); in a real Chrome or Firefox the browser opens its own
   element picker first (in progress). The View menu entry works.
4. **Pick and copy.** A click picks; the dialog's Copy puts a text block
   on the clipboard ([browser]): element, role, egui id, rectangle,
   window, layer, zone, block, panel, plugin, theme, kit part, focus,
   page, story and how it was picked. It has **no version, platform,
   browser or renderer**. A swatch on the overview is named only as its
   container (`GenericContainer (container)`), as the debugging guide
   says of custom-painted elements.
5. **Copy the version.** A second copy, from the About page or dialog:
   `egui-pop gallery 0.3.0 (a6e6ed24)`, egui-pop and egui versions,
   `Platform: web (wasm32)`, `Renderer: wgpu: Gl, ANGLE (...)`
   ([browser]). The browser, its version and the operating system are
   missing; the commit is there.
6. **Open an issue.** "Report a problem" opens
   `https://github.com/quazardous/egui-pop/issues` in a new tab
   ([browser]: the page called `window.open` with that URL and `_blank`).
   No title, no body, no labels; the visitor must paste both copies and
   write the steps by hand.
7. **Attach a screenshot.** Only with the operating system's or the
   browser's own tool (Firefox: Take Screenshot; Chrome: devtools'
   "Capture screenshot"). The gallery has no screenshot command, though
   eframe's web runner does honour `ViewportCommand::Screenshot`
   ([read]: `handle_viewport_commands` keeps that one).
8. **Say where.** The page cannot be given as a link (no URL per page),
   so the steps must say "click X in the side list".

What a reporter must know beforehand, today: that devtools exist and
where their setting is; that the browser's Ctrl+Shift+C must be avoided
(use the View menu); that two separate copies are needed; that "Report a
problem" is in the About dialog; and that the page is not in the URL.

### Reporting path problems

| # | Problem | What it breaks for a reporter | Evidence | Fix (size) |
|---|---|---|---|---|
| R1 | Ctrl+Shift+C and F12 belong to the browser | The documented inspect chord opens the browser's inspector instead | in progress | in progress |
| R2 | Devtools off by default, the menu entries greyed with no reason, the chords silent | A reporter never finds inspect | [browser] | in progress (on by default); also a toast or tooltip saying which setting enables them (S) |
| R3 | The devtools setting is lost on reload | Each visit starts over | [browser]: on after the toggle, Ctrl+Shift+C did nothing after a reload | in progress |
| R4 | "Report a problem" opens the bare issue list | The reporter writes everything by hand; most will not | [browser] | done (2026-11-04): `help.report` opens `issues/new` with the issue form `bug.yml` filled field by field (`template=`, then `title`, `version`, `platform`, `page`, `pick`), or a body without a form, under 8 KB with the pick trimmed first |
| R5 | No "Report a problem" in the Help menu, the palette or the inspect dialog | The entry point is two clicks deep in About | [browser] | done (2026-11-04): `help.report` in Help and the palette, a Report… button beside Copy in the inspect dialog, and the About dialog's link through the same command |
| R6 | The inspect copy has no version, platform, browser or renderer | Two copies to paste; half the reports will miss one | [browser] | Append the version info to the inspect text, or make the report one block (S) |
| R7 | The version info says `web (wasm32)`: no browser, no browser version, no OS | Browser-specific bugs (keys, clipboard, WebGL vs WebGPU) cannot be told apart | [read] `AppInfo::platform`, [browser] | done for the browser and the OS (2026-11-04): the platform line reads `navigator.userAgent` (`web (wasm32), Firefox 132 on Linux`) |
| R8 | No URL per page | Steps cannot link the page; a reload loses it | [browser]: reload shows the overview; [read]: the web `main` uses `Options::default()` | Read and write the page in the URL's hash (`#page=files`), as `--page` does on the desktop (S) |
| R9 | No screenshot from the app | The reporter needs an outside tool; many reports come without one | [read] | A "Copy screenshot" or "Save screenshot" command through `ViewportCommand::Screenshot` (supported by eframe's web runner) and `Context::copy_image` or a download (M) |
| R10 | No issue template in the repository | A prefilled URL has nothing to aim at; free-form reports | [read] `.github/` holds only the pull request template | done (2026-11-04): `.github/ISSUE_TEMPLATE/bug.yml` (what happened, expected, steps, version info, platform, page, inspected element), a light `feature.yml` and `config.yml` |
| R11 | Keys lost after a click outside the canvas | After the address bar or the browser's devtools, chords do nothing until the canvas is clicked; a reporter thinks the key is broken | [browser]: after a blur, Alt+Down changed nothing; a click on the canvas restored it | Focus the canvas when the pointer enters it, or show a "click to use the keyboard" hint while it has no focus (S) |
| R12 | A panic shows a message but no way to report it | The one report that matters most is the hardest | [read] `main.rs` `failed` | Add a prefilled issue link with the panic text to the failure message (S) |
| R13 | Console noise a reporter will paste | `logging to egui-pop-gallery-0/gallery.log` (no file on the web), `crash reports: ... operation not supported on this platform`, `Unhandled egui viewport command: Decorations(true)` at every start | [browser], all three browsers | Skip the log file line, the crash plugin's warning and the decorations command on the web (S) |
| R14 | Commands that do nothing on the web stay enabled | View > Full screen (F11), Zen mode, File > Quit (Ctrl+Q) look broken; a reporter files them | [browser]: `Fullscreen(true)` and `Close` are logged as unhandled | Disable them on the web with a reason, or implement full screen with the Fullscreen API (S) |
| R15 | The landing page and the gallery do not say how to report | The goal is invisible | [read] `tools/site/index.html` | A line and a link on the landing page and in the gallery's first page (S) |

## Keys

### Every default binding, in a browser

The bindings below are every key given by `CommandSpec::key` and
`CommandSpec::key_with` in egui-pop's crates and the gallery (the
gallery's pages and the minimal example included), plus the file
picker's own keys. egui-pop's `Ctrl` is the Command key on macOS too
(`egui_pop_core::keys::chord_of`).

Column **Prevented** is measured ([browser], identical in Chromium 140,
Chrome 157 and Firefox 141): whether eframe cancelled the browser's
default action. Column **The browser's own use** is read from the
browsers' shortcut lists (Linux and Windows first, macOS with Cmd after
the semicolon); **reserved** means the page never sees the key, or sees
it but cannot cancel it.

| Chord | egui-pop command | Prevented | The browser's or the system's own use | Class |
|---|---|---|---|---|
| Ctrl+B | toggle primary side bar | no | Firefox: bookmarks sidebar; Safari (Cmd+Shift+B: favourites bar) | **leaks** in Firefox |
| Ctrl+J | toggle panel | no | Chrome, Firefox on Windows: downloads; Safari Cmd+J: jump to selection | **leaks** |
| Ctrl+Alt+B | toggle secondary side bar | no | Windows: AltGr+B on some layouts; Chrome macOS Cmd+Option+B: bookmark manager | leaks on macOS |
| Ctrl+G | palette's go to / editor go to line | no | find next (all) | **leaks** |
| Ctrl+E | zones page's extender (gallery) | no | Chrome, Firefox: focus the search box; Safari: use selection for find | **leaks** |
| Ctrl+F | editor and viewer find | no | find in page (all) | **leaks** |
| F3, Shift+F3 | find next, previous | no | find next, previous | **leaks** |
| Ctrl+H | editor replace | no | Chrome: history; Firefox: history sidebar; macOS Cmd+H hides the browser | **leaks**; reserved on macOS |
| Ctrl+S | editor save | yes | save page | fine |
| Ctrl+L | file picker: path field | no | focus the address bar | **leaks**, takes the focus away |
| Ctrl+D | file picker: bookmark folder | no | bookmark this page | **leaks** |
| Ctrl+A | file picker: select all | no | select all (nothing selectable but the canvas) | harmless |
| Ctrl+Shift+N | file picker: new folder | no | Chrome: incognito window; Firefox: reopen closed window; Safari: private window | **reserved** |
| Ctrl+Space | file picker and editor: toggle, complete | yes | Windows: IME toggle (CJK); macOS (Cmd+Space): Spotlight | reserved on macOS by the system |
| Ctrl+Enter | file picker: choose | no | none | fine |
| Alt+P | file picker: preview pane | yes (as text) | none | fine |
| Ctrl+Comma | settings | yes | macOS Cmd+Comma: browser preferences | fine |
| Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y | undo, redo | no | none outside a text field | harmless |
| Ctrl+C, Ctrl+X, Ctrl+V, Ctrl+Insert, Shift+Insert, Shift+Delete | copy, cut, paste | no (on purpose: eframe needs the browser's copy, cut and paste events) | copy, cut, paste | fine |
| Ctrl+Shift+V | paste special | no | Chrome, Firefox: paste as plain text | harmless |
| Ctrl+Shift+C | inspect | no | devtools' element picker (Chrome, Firefox; Chrome macOS Cmd+Shift+C) | in progress |
| F12 | devtools | no | browser devtools | in progress |
| Ctrl+Shift+P, F1 | command palette | Ctrl+Shift+P yes, F1 no | Ctrl+Shift+P: Firefox private window (reserved); F1: Chrome help page | **reserved** in Firefox; F1 **leaks** in Chrome |
| Ctrl+Shift+T | next theme (gallery) | no | reopen closed tab | **reserved** |
| Ctrl+Shift+G | show grid (gallery menus page) | no | find previous | **leaks** |
| Ctrl+Shift+K | terminal clear, minimal example | no | Firefox: web console | **leaks** in Firefox |
| Ctrl+Shift+Delete | workbench reset (gallery) | no | clear browsing data dialog | **leaks** |
| Ctrl+Shift+[, Ctrl+Shift+] | editor fold, unfold | no | macOS Cmd+Shift+[ ]: previous, next tab | reserved on macOS |
| Ctrl+Period | editor code action | no | Linux (IBus, GNOME): emoji picker; macOS Cmd+Period: stop | leaks on Linux |
| F9 | editor breakpoint | no | none | fine |
| Ctrl+PageDown, Ctrl+PageUp | next, previous document | no | next, previous tab | **reserved** in Chrome |
| Ctrl+Alt+Equals, Ctrl+Alt+Plus, Ctrl+Alt+Minus, Ctrl+Alt+0 | interface zoom | no | Windows: AltGr with 0, =, - types characters on many layouts (`}` `@` on AZERTY, `}` on German) | collides with AltGr |
| Ctrl+Plus, Ctrl+Equals, Ctrl+Minus, Ctrl+0 | viewer zoom | no | page zoom (all) | **leaks**: page and viewer zoom together |
| Ctrl+Alt+S | toolbar save (gallery) | yes | none | fine |
| Ctrl+Alt+G | global shortcut demo (gallery) | no | none (no global shortcuts on the web) | fine |
| Ctrl+Q | quit | no | Firefox Linux and Windows: quit (reserved); macOS Cmd+Q: quit the browser | **reserved**; does nothing on the web anyway |
| Alt+Left, Alt+Right | back, forward (context stack, viewer) | yes | back, forward; macOS uses Cmd+Left | fine (eframe cancels arrows) |
| Alt+Up, Alt+Down | previous, next page (gallery) | yes | none | fine |
| Alt+Shift+Up | context stack up | yes | none | fine |
| Alt+Space | window menu | yes | Windows and GNOME: the window menu (system) | reserved by the system |
| Alt+F7 | dialog move | no | GNOME: move the window (system) | reserved on GNOME |
| F5 | viewer refresh | no (on purpose) | reload the page | **leaks**: the session is lost |
| F6, Shift+F6 | next, previous region | no | focus the address bar, cycle browser panes | **leaks**, takes the focus away |
| F11 | full screen | no | browser full screen | works by accident; the app's command does nothing |
| Shift+F11 | zen mode | no | none | fine |
| Tab, Space, Backspace, arrows | focus and widgets | yes | | fine |

Reserved keys come from the browsers, not from egui: Chrome keeps
Ctrl+N, Ctrl+T, Ctrl+W, their Shift forms, Ctrl+Tab and the tab-switching
keys from pages ([Chromium's browser keys note](https://www.chromium.org/developers/design-documents/extensions/proposed-changes/apis-under-development/browser-keys/)),
and Firefox similarly reserves new window, new tab, close, quit and the
private window. The only way around them is the
[Keyboard Lock API](https://wicg.github.io/keyboard-lock/), Chromium only,
and only in full screen.

### Keys: problems

| # | Problem | What it breaks | Evidence | Fix (size) |
|---|---|---|---|---|
| K1 | eframe cancels the browser's action only for its own list; `WebOptions::should_prevent_default` is consulted for text, paste, pointer and wheel events, never for key presses | Every bound chord outside that list runs twice: in egui-pop and in the browser (find bar, address bar, bookmark dialog, page zoom, reload) | [browser]: the table above; [read]: [`should_prevent_default_for_key`](https://docs.rs/crate/eframe/0.37.0/source/src/web/events.rs) | egui-pop's web glue adds its own `keydown` listener on the canvas that cancels a key when the keymap resolves it (the dispatcher already knows), or upstream: let `should_prevent_default` see `Event::Key` (M; the upstream request S) |
| K2 | Reserved chords bound by default: Ctrl+Shift+N, Ctrl+Shift+T, Ctrl+Shift+P (Firefox), Ctrl+PageUp/PageDown (Chrome), Ctrl+Q | The command never runs from its key on the web; the browser opens a window or switches tabs | [read], [upstream] (links above) | `isWeb` alternatives in the default keymap (palette: F1 or Ctrl+K style; next theme: none), and a check that warns when a default web binding is reserved (S) |
| K3 | F5 bound to the viewer's refresh | The page reloads and the session is gone | [read]; eframe never cancels F5 on purpose | Bind the viewer's refresh to something else on the web (S) |
| K4 | F6 regions and Ctrl+L | The browser takes the focus to its address bar; keyboard navigation between regions is gone and the canvas loses the focus (see R11) | [read] | Cancel them through K1; check by hand that each browser lets a page cancel F6 (with K1) |
| K5 | Ctrl+Plus, Ctrl+Minus, Ctrl+0 bound to the viewer's zoom | The page zooms with the viewer: the whole interface changes size | [read] | Cancel through K1 (with K1) |
| K6 | Shortcuts shown as `Ctrl` on macOS | A Mac user reads Ctrl+B and presses Ctrl (which works, as egui-pop maps both) but every Mac convention says Cmd; reports will say "Ctrl+B does nothing" when the browser took Cmd+B | [read]: no Cmd label anywhere | Show `Cmd` (or ⌘) when `Context::os` is macOS, on the web too (S) |
| K7 | Ctrl+Alt chords on Windows collide with AltGr | Typing `}` or `@` on French or German layouts may reset the interface zoom; on the web as on the desktop | [read] | Leave Ctrl+Alt+digit and symbol chords alone while a text input owns the keyboard, or move the interface zoom off AltGr chords (S) |
| K8 | Dead keys and IME | Composition goes through eframe's hidden text agent: an IME commit (`にほん` to `日本`) reached a text field in Chromium; dead keys and AltGr were not run | [browser] (IME, Chromium only); [read] the rest | Run the keyboard walk with a dead key and AltGr in the browser harness (S) |
| K9 | Keys before the first click | In all three browsers the canvas had the focus at load and Alt+Down worked before any click ([browser]); after a click elsewhere it does not (R11). In an iframe or an embedding page it would not have it | [browser] | See R11 |
| K10 | No `isWeb` or `isMac` in `when` conditions | A keymap cannot give the web or macOS another key | known | Add `isWeb` (compile time) and `isMac` (`Context::os`, on the web the browser's OS) to `AppHandle::when_context`; cheap, and K2, K3 need `isWeb` (S) |

**Are `isWeb` and `isMac` worth adding?** Yes, both, and cheaply:
`when_context` already sets base keys each pass. `isWeb` is what K2 and
K3 need (a web-only default without changing the desktop's). `isMac` is
less pressing, since `Ctrl` already means Command, but it lets a keymap
avoid macOS's own chords (Cmd+H, Cmd+Q, Cmd+Space, Cmd+Shift+[ ]) and it
is what K6's labels read. A browser name (`isFirefox`) is not worth a
condition: K1 removes most of the differences.

## Platform services

| # | Service | What it breaks | Evidence | Fix (size) |
|---|---|---|---|---|
| P1 | Clipboard, text | Copy (Copy version info, inspect's Copy, a text field's Ctrl+C) and paste (Ctrl+V into a text field) work in Chromium. Needs a secure context (the deployed site is HTTPS; a local `http://` address other than localhost is not) | [browser] Chromium; [upstream] [MDN, secure contexts](https://developer.mozilla.org/en-US/docs/Web/Security/Secure_Contexts) | none |
| P2 | Clipboard in Firefox and Safari | Not read back in Firefox (the harness cannot grant clipboard reading there); Safari only writes inside a user gesture, which eframe handles since [egui#3621](https://github.com/emilk/egui/pull/3621) ([issue #3480](https://github.com/emilk/egui/issues/3480)) | untested | Check by hand once in each (S) |
| P3 | Clipboard: custom formats and reading | No backend reads the system clipboard on the web: a paste knows only the text the browser hands over; egui-pop's typed formats (inspect JSON, colours, nodes) stay inside the page. Reading needs the async Clipboard API and a permission prompt | [read] `docs/clipboard.md`, `EguiText` | A web `SystemBackend` over `navigator.clipboard.read`/`write` with `text/plain` and `web `-prefixed custom formats (M) |
| P4 | Files: no file system | The picker reads an in-memory demo folder; Save writes nowhere; a visitor cannot open their own file to reproduce a bug | [browser] files page; [read] `MemoryFs` | An upload (`<input type=file>`) into the memory file system and a download for Save (M) |
| P5 | File drag and drop | eframe delivers dropped files (bytes on the web), but nothing in egui-pop reads `dropped_files` or `hovered_files` | [read] | Read drops into the memory file system and the viewer (S to M) |
| P6 | Open URL | Links open a new tab ([browser]: `window.open(url, "_blank")`); `mailto:` in the same tab; local files and "Show in folder" are refused with a reason | [browser], [read] `docs/links.md` | none |
| P7 | Notifications | Toasts only; the platform page shows the raw error ("no thread to show desktop notifications: operation not supported on this platform") | [browser] | Optional: the Notifications API behind a permission (M); reword the page (S) |
| P8 | Multiple viewports | Detached panels are embedded as egui windows inside the canvas (egui's default when the backend has no viewports) | [read] eframe sets `embed_viewports` only on native | Check the detach paths draw well embedded (S) |
| P9 | Full screen, window commands | `Fullscreen`, `Close`, `Decorations`, `Minimized`, `Maximized`, `StartDrag`, `RequestUserAttention` are logged as unhandled and ignored | [browser] (`Fullscreen(true)`, `Close`, `Decorations(true)`); [read] `handle_viewport_commands` | Full screen through `Element.requestFullscreen` in egui-pop's web glue; others disabled on the web (S, see R14) |
| P10 | Cursor and hiding it | System cursors map to CSS; `CursorIcon::None` hides it; bitmap cursors are dropped, so painted tool cursors fall back | [read], [upstream] [egui#3848](https://github.com/emilk/egui/issues/3848) (in `docs/upstream.md`) | upstream |
| P11 | Pointer lock | Not used by egui-pop; nothing to break today | [read] | none |
| P12 | Single instance, deep links, tray, global shortcuts, focus mode, system text scale, system thumbnails, terminal shell | None on the web; each says so (tray and global shortcuts simulated on the platform page; terminal's shell section absent) | [browser] platform and terminal pages; [read] | none; R14 for wording |
| P13 | Logs and crash reports | The log file and crash reports need a folder: on the web only the log tail in memory; a panic replaces the canvas with a message | [browser] console; [read] | R12, R13 |

## Storage

What each part keeps, and where it goes on the web.

| Part | Desktop | Web today | Evidence |
|---|---|---|---|
| egui's memory (folds, panel sizes, scroll offsets) | eframe's file | `localStorage["egui_memory_ron"]`, about 8 KB | [browser] |
| Workbench layout | eframe's storage | `localStorage["gallery.layout"]`, about 0.6 KB | [browser] |
| Settings (theme, density, devtools, picker's recent places and bookmarks `settings/files.*`) | `persist(dir)` documents | lost (in progress) | [read] `AppBuilder::persist`, `save_later` |
| User keymap | `persist(dir)` | lost (in progress) | [read] |
| Contexts' view document | `persist(dir)` | lost: `save_view` returns without a directory | [read] `contexts.rs` |
| User kits, token cache | folders | none | [read] |
| Journal, history | memory | memory | [read] |
| Log file, crash reports | folders | none | [browser] console |

| # | Problem | What it breaks | Evidence | Fix (size) |
|---|---|---|---|---|
| S1 | `persist(dir)` writes files; nothing writes eframe's storage but the layout | Settings, keymap, bookmarks, recent places, the context view lost on reload | [read] | in progress |
| S2 | Is a library-level backend over `eframe::Storage` right? | | | **Yes**: make the persistence writer a trait with a folder implementation and an `eframe::Storage` one (one key per document, `egui_pop.<name>`), chosen by `run_web`/`run_native`; the debounced writer, the documents and their schemas stay as they are. A host with its own store gives its own. Keeps the gallery out of it (M; overlaps the in-progress change) |
| S3 | `localStorage` is shared by the whole origin | The gallery lives on `quazardous.github.io`: every project page of that account shares the origin, and egui's key `egui_memory_ron` is the same in every eframe app, so another eframe app published there would read and overwrite the gallery's memory | [read] eframe `storage.rs`; [upstream] [MDN, storage quotas](https://developer.mozilla.org/en-US/docs/Web/API/Storage_API/Storage_quotas_and_eviction_criteria) | Prefix egui-pop's own keys with the application id; for egui's key, an upstream option for its name (S, upstream S) |
| S4 | Limits | About 5 MB per origin for `localStorage`, synchronous; a large keymap or kit fits, a document library would not; private windows clear it at the end | [upstream] same | IndexedDB if documents ever grow (L, not needed now) |
| S5 | No reset on the web | A broken stored layout or setting can only be cleared through the browser's site data; a reporter cannot say "it works after a reset" | [read] | A "Reset everything stored" command (S) |
| S6 | Saved on blur and every 30 s | A tab closed within 30 s of a change keeps the old state; Chrome 157 had nothing stored after the short probe while Chromium 140 and Firefox had both keys | [browser], [read] eframe's `auto_save_interval` | Save on `pagehide`/`visibilitychange` too (eframe already listens for `visibilitychange`); verify (S) |

## Rendering and performance

| # | Problem | What it breaks | Evidence | Fix (size) |
|---|---|---|---|---|
| G1 | WebGPU or WebGL 2 | eframe's wgpu painter takes WebGPU where the browser has it (Chrome 157: `BrowserWebGpu`) and WebGL 2 otherwise (Chromium 140 headless, Firefox 141: `Gl`); both drew every page. The viewport's buffer device tiles need fragment storage that WebGL 2 lacks and fall back, as documented | [browser]; [read] `docs/viewport.md`; [upstream] [WebGPU status](https://github.com/gpuweb/gpuweb/wiki/Implementation-Status) | none; the renderer line in the version info tells them apart |
| G2 | Software rendering | On a machine without a GPU the browser uses SwiftShader; egui-wgpu warns "Software rasterizer detected"; slow but correct | [browser] | none |
| G3 | Module size | 33.6 MB deployed (13.3 MB gzipped on the wire), 38.6 MB in this build; no `opt-level = "z"`, no LTO, no `wasm-opt`; fonts are only about 2 MB of it | [read] measured with `curl` and `ls` | A `[profile.release]` for the web build (`lto`, `codegen-units = 1`, `opt-level = "s"`), Trunk's `data-wasm-opt`, and check `panic = "abort"` size; measure each (M) |
| G4 | Load time and nothing to see while loading | 2.6 s to start from the deployed site in Chromium on a fast link; a slow link takes several times that over a dark empty page | [browser] | A loading message and progress in `index.html` (S) |
| G5 | Caching | GitHub Pages serves `max-age=600`; Trunk's hashed file names make a new deploy load fresh; a reporter may run a build up to 10 minutes old, and the version info's commit says which | [read] response headers | none |
| G6 | No `std::thread` | Every thread path in the gallery's web build is gated (`cfg!(target_arch = "wasm32")`): previews, `latest`, tree and editor demos, the viewport's workers (0 on the web). A spawn would panic at run time, not at build time; nothing catches a new ungated one | [read] all spawns listed; [browser] no panic on any page | A CI check: run the web smoke test over every page, as the walk here did (S) |
| G7 | Blocking calls | `std::time::Instant::now` panics on `wasm32-unknown-unknown`; runtime code uses `web-time`, the remaining `Instant::now` are in tests | [read] | Lint: `clippy::disallowed_methods` for `std::time::Instant::now` and `SystemTime::now` (S) |
| G8 | Work runs in the frame | Previews, the slow linked-panel computation and tile production run on the interface thread, one per frame; a heavy page stutters where the desktop does not | [read] `docs/linked-panels.md`, `files/preview.rs` | Accept; or web workers later (L) |
| G9 | Timers in background tabs | eframe paints on `requestAnimationFrame`, which browsers pause in hidden tabs: toasts, jobs, the motion clock and debounced saves wait until the tab is back | [read]; [upstream] [MDN, requestAnimationFrame](https://developer.mozilla.org/en-US/docs/Web/API/Window/requestAnimationFrame) | none needed; note it for job pages (S) |
| G10 | Glyphs missing from the bundled fonts | Drawn by eframe's canvas fallback (`canvas_glyphs`): Japanese typed in a field showed ([browser]); the look differs from the desktop's system fallback | [browser], [read] | none |

## Media and embeds

| # | Problem | What it breaks | Evidence | Fix (size) |
|---|---|---|---|---|
| M1 | Video viewer (GStreamer) | Not built for the web: a video link opens outside, in a new tab | [read] `viewer/mod.rs` gates `gst_player` on non-wasm; `docs/links.md` | An HTML `<video>` element over the canvas, positioned on the panel's rectangle (M to L) |
| M2 | Web view (wry, WebKitGTK) | Not on the web: HTML pages beyond the safe subset open in a new tab | [read] `docs/links.md` | An `<iframe>` over the canvas (same technique as M1) (M) |
| M3 | YouTube as an iframe over the canvas | Feasible: position an absolutely placed `<iframe>` on the panel's rectangle each frame, hide it when the panel is covered (menus and popups are drawn on the canvas, so an overlay on top of the iframe is impossible: it must hide while one is open); keyboard and focus go to the iframe while inside it | [read] | M, after M1's overlay helper |

## Accessibility

| # | Problem | What it breaks | Evidence | Fix (size) |
|---|---|---|---|---|
| A1 | AccessKit on the web | eframe's web runner ignores `accesskit_update` ("not currently implemented"): screen readers see one canvas; egui's built-in speech (`screen_reader` option, Web Speech API) is the only voice | [read] [`app_runner.rs`](https://docs.rs/crate/eframe/0.37.0/source/src/web/app_runner.rs); [upstream] [AccessKit](https://github.com/AccessKit/accesskit) lists a web adapter as planned | upstream (L); meanwhile say so on the landing page (S) |
| A2 | The inspect picker still works | egui builds the AccessKit tree in every backend (always on in egui 0.37), so the inspect dialog has roles and names on the web too | [browser] | none |
| A3 | IME text input | Through eframe's hidden `<input>`: an IME commit reached a text field in Chromium; the candidate window's position follows `ime` output | [browser] Chromium; [read] | Run in Firefox and with a real IME (S) |
| A4 | Browser zoom and text size | Page zoom changes the device pixel ratio and egui follows; the browser's minimum font size and text-only zoom do nothing to a canvas | [read] | none |

## Gallery pages on the web

All 58 pages were walked with Alt+Down in Chromium 140 and Firefox 141,
a screenshot and the console of each ([browser]). None panicked or
logged an error; the only page warning was a Canvas2D readback hint on
the font pairs page (Chromium), probably from eframe's canvas fallback for glyphs. Differences from the desktop:

| Page | On the web | Evidence |
|---|---|---|
| overview, tokens, builtins, scoped, icons, indicators, sections, motion, values, history, sliders, inputs, menus, toolbars, dialogs, toasts, tips, about, progress, placeholders, roles | as on the desktop | [browser] |
| theme-editor | Export as a token file writes nowhere (no folder) | [read] |
| fonts | as on the desktop; a Canvas2D readback warning in Chromium | [browser] |
| files | the in-app picker over an in-memory demo folder; "Recent: none, Bookmarks: none" and nothing kept; no system chooser, no thumbnails, no upload or download | [browser] |
| viewport, navigator, images, grading, cursors, node-graph, graph-file, preview | draw; procedural tiles made in the frame (no workers); painted tool cursors only where egui-pop paints them | [browser], [read] |
| display | the wide gamut and HDR surfaces are not offered (desktop only); the page reports sRGB | [browser], [read] `lib.rs` |
| tree, table, library, linked-panels | answers come in the same frame (no threads); the linked panel's slow preview blocks the frame instead of sleeping | [read] |
| text-editor, code-editor | as on the desktop; Ctrl+F, Ctrl+H, Ctrl+G, F3 also reach the browser (K1) | [browser], [read] |
| terminal | the build log and echo demos; the shell section is absent (no pseudo-terminal) | [browser] |
| zones, layout, arrangements, workbench, contexts, strips, clipboard, palettes, links | draw; detaching a panel embeds it in the canvas (not tried) | [browser], [read] |
| frame | the window frame preview draws; no real window to frame | [browser] |
| i18n | as on the desktop | [browser] |
| viewer | help pages, markdown and safe HTML; no video, no web view; F5 reloads the page (K3) | [browser], [read] |
| scale | as on the desktop; Ctrl+Alt+Plus and Minus do not zoom the page | [browser] |
| pointer | as on the desktop; Ctrl+wheel goes to egui-pop (the gallery cancels every wheel event) | [read] `web_prevent_default` |
| platform | says "Run with --platform to use the real one" and shows a `gallery://page/<name>` deep link, neither of which exists on the web; notifications show the raw error; logs warn "another logger is installed" | [browser] |
| settings | as on the desktop; nothing survives a reload (S1) | [browser] |
| debug | the collectors draw; the only place that says how to enable inspect | [browser] |
| jobs | timers pause in a background tab (G9) | [read] |

## What could not be tested

- Safari (any platform), and every browser on Windows and macOS: the
  key table's browser columns for them are read, not run.
- What a real browser does with a key it keeps (reserved keys, F5, F6,
  Ctrl+Shift+C): headless runs show only what the page receives.
- Reading the clipboard in Firefox (no permission in the headless
  harness); Safari's clipboard.
- Dead keys, AltGr and a real IME's candidate window.
- Screen readers.
- A real GPU: every run used SwiftShader.
- Detached panels, file drop and background tabs.
- The CI job's Trunk packaging itself: Trunk did not build on this
  machine, so the module was packed with wasm-bindgen directly (same
  crate, features and profile).

## Proposed follow-ups

One per fix worth doing, ordered by value for the reporting goal first,
then by cost. The three items in progress are left out.

1. **Prefilled issue from the app** (R4, R5, R10; done 2026-11-04, R7's
   browser line with it): a `help.report`
   command in Help, the palette and the inspect dialog that opens
   `issues/new` with a title, the version info, the browser, the page and
   the last pick, plus a bug issue form in the repository. S.
2. **One report block** (R6, R7): the inspect copy carries the version
   info; the platform line names the browser, its version and the OS on
   the web. S.
3. **Page in the URL** (R8): the page read from and written to the
   URL's hash on the web. S.
4. **Cancel every bound key** (K1, K4, K5): a canvas `keydown` listener
   that cancels a key the keymap resolves, and an upstream request for
   `should_prevent_default` on key events. M (upstream request S).
5. **Web-safe default keys** (K2, K3, K10): `isWeb` and `isMac` in
   `when` conditions, web alternatives for reserved chords and F5, and a
   test listing reserved chords in the default keymap. S.
6. **Keyboard focus hint** (R11, K9): focus the canvas on pointer entry,
   or show a "click to use the keyboard" hint while it has no focus. S.
7. **Say why devtools are greyed** (R2): the disabled entries and chords
   say which setting enables them. S.
8. **Screenshot command** (R9): copy or download a screenshot through
   `ViewportCommand::Screenshot`. M.
9. **Panic page with a report link** (R12). S.
10. **Quiet the web console** (R13): no log-file line, crash-plugin
    warning or `Decorations` command on the web. S.
11. **Disable or implement desktop-only commands** (R14, P9): full
    screen through the Fullscreen API; quit and window commands
    disabled with a reason; platform page wording. S.
12. **Say how to report on the landing page** (R15), with a note on
    screen readers (A1). S.
13. **Persistence backend over `eframe::Storage`** (S2, S5, S6): the
    writer as a trait, an `eframe::Storage` implementation, a reset
    command, a save on page hide. M (after the in-progress change).
14. **Smaller, announced first load** (G3, G4): a web release profile,
    `wasm-opt`, and a loading message. M.
15. **Origin-safe storage keys** (S3): egui-pop's keys prefixed with
    the application id; an upstream option for egui's memory key. S.
16. **Mac labels** (K6): `Cmd` in shortcut labels on macOS. S.
17. **AltGr-safe interface zoom** (K7). S.
18. **Every page in CI's web smoke test** (G6) and a lint for
    `Instant::now` (G7). S.
19. **Upload, download and drop** (P4, P5): a visitor's own file in the
    picker and the viewer. M.
20. **Web clipboard backend** (P3): typed formats through the async
    Clipboard API. M.
21. **Video and pages over the canvas** (M1, M2, M3): an overlay helper
    for `<video>` and `<iframe>` on a panel's rectangle. M to L.
22. **Manual checks** (P2, K8, A3): Safari, Windows and macOS, dead keys
    and an IME, once by hand. S.
23. **AccessKit on the web** (A1): upstream, follow AccessKit's web
    adapter. L.
