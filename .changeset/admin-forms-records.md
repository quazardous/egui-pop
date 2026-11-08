---
egui-pop-model: minor
egui-pop-api: minor
---

Administration app, slice 2. Record lists and tables (`Item::List`,
`Item::Table`, `Records`): rows read from a `read_only` command or a
`records` hook, with columns, a tone per row, row actions (`${row.<key>}`)
and an empty state. Statuses read `read_only` commands, once, and again
after a command that changes something or an event they name. `when`
hides a node and `enabled` greys it with its reason (`Clauses`), read
against the context keys and the section's values (`form.<id>`). Rows
keep their controls inside the page and wrap their help, in search too.
