---
egui-pop-api: major
egui-pop: major
egui-pop-agent: major
egui-pop-editors: major
egui-pop-i18n: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-views: major
---

Requests heard after the drawing, and every command run on the bus. `AppHandle::ask::<P>(input, value)` asks a request now and has it heard after the drawing (bus-contract's `Defer` capability; `None` on a bus without it), for code that cannot be re-entered while it draws, such as a panel with its state borrowed. The application drains the asks (`AppHandle::drain_asks`, what each did) once per pass after the main window's panels, its detached panels' windows and its zones are drawn, so an ask made while drawing is heard in the same pass, and before it quits (`App::exit`, before the `on_exit` functions and after them for what they asked); an ask made during a drain waits for the next one, a drain that ran something asks for a pass, and `AppHandle::asks_waiting` counts what waits. An application window's asks (a deferred viewport on the same thread) wait in the same queue for the main window's next pass, which is asked for; the bus's wakers now ask the main window's pass. Every command run, whatever asked it (a key, the palette, a menu, a toolbar, an agent), is asked first on the new request point `command.run` (`bus::CommandRun`, its input a `bus::CommandAsk`: command, arguments, origin, window), declared fail-closed and listed in `bus::COMMAND_POINTS`, `bus::POINTS` and `bus::CONTRACT`: a listener may refuse the run with a reason (`HookError::refuse`), and one that fails, panics or misses its deadline refuses it too. A refused run never reaches the host or its handler; the caller gets "refused by `listener`: reason" (or "refused, `listener` failed: reason") and the journal says `command.refused` with `by`, `reason` and `failed`. egui-pop registers no listener on it, so with nobody listening commands run as before. Breaking: with `layout.changed`, `panel.describe` and `command.run`, `bus::POINTS` grows from 20 entries in 0.12 to 23 (its type changes): code naming the array's length writes `bus::POINTS.len()` or the new length.
