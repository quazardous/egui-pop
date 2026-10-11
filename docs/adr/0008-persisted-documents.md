# 0008: Persisted documents: versions, migrations, atomic writes

Date: 2026-10-11. Status: accepted.

## Context

Layouts, keymaps and settings are written to disk and read back by later
versions of the application and of egui-pop. A crash during a write must
not lose them, and a format change must not reset them.

## Decision

Every persisted document is a JSON object with a `"version"` integer
(missing means 0). Each kind (`layout`, `keymap`, `settings`, or an
application's own) has a `Schema`: its current version and one migration
per older version, each a function from the JSON of version `n` to that of
`n + 1`. Loading applies the migrations in order, then deserializes; a
document newer than the schema is refused, not guessed at. Schemas and
migrations live in `egui-pop-model::doc` (no I/O).

The persistence driver (`egui-pop-core::persist`) writes atomically: the
new text to `name.json.tmp`, flushed and synced, the previous file renamed
to `name.json.bak` (one backup), then the temporary file renamed into
place. Loading falls back to the backup when the main file is missing or
unreadable, and says so.

No crate is added: `tempfile` and `atomicwrites` do the rename but not the
backup or the migrations, which are the substance here.

## Consequences

- Each format change ships with its migration and a test loading the
  previous version.
- On the web (no file system) the driver reports an error; storage there
  is a later concern.
