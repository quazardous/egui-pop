---
egui-pop-views: minor
egui-pop-i18n: minor
egui-pop: minor
---

A text log view on the data table, after the Rerun viewer's text log view (its design, none of its code): `egui_pop::views::text_log`.

- **Columns**: the time in a timeline's scale (`TextLog::time_scale`, or a bound timeline's), else a plain number; the entity path, tinted where it is highlighted elsewhere (`TextLog::highlight_entity`); the level as a chip in its `palette.log.*` colour (`Level`, six levels from trace to critical); the body, its first line, a multi-line body unfolding to all its lines.
- **Filters**: a toggle chip per level with its count, and a text filter over the bodies and entity paths, case aside; `TextLog::keep` adds the application's own.
- **Follow**: the last line stays in view as lines come in, until the user scrolls up; back at the end it follows again (`TextLogOutput::following`).
- **The time cursor**: the rows at the latest time not after the cursor (`TextLog::time_cursor`, or a timeline's playhead with `TextLog::timeline`) are the table's selection, drawn by the kit's `table-row`, and come into view when the cursor moves; a click or the arrow keys move the cursor to a row's time (`TextLogOutput::cursor_moved`, `picked`).
- **Large logs**: only the rows in view are built, and the filter scans only the entries added since the last pass (`TextLogState::refilter` starts it over).
- Its texts are translated (English and French).
