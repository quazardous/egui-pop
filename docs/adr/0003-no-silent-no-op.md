# 0003: No silent no-op

Date: 2026-10-11. Status: accepted.

## Context

Extension systems fail quietly: a block contributed to a zone nobody
declared never shows, a second command with the same id is dropped, a
"replace" aimed at a block that was renamed does nothing. Each costs an
afternoon to find, because nothing says it happened.

## Decision

A registration that would be ignored is either impossible by types, or
reported. Reporting means both:

- a `log::warn!` (or `error!`) naming the owner, the subject and **what
  happens instead** ("block `x` replaces `y` in zone `app.main`, which has
  no such block: it is added as a new block"), and
- an entry in a collected `Diagnostics` list (owner, code, subject,
  message), that tests assert on and the debug bridge shows.

Types come first: typed ids (ADR 0002), well-known zones as constants,
builders that cannot be half-filled. Validation runs at registration
(when-clauses parsed, ids checked for duplicates) or, for what depends on
other plugins (zones, replace targets, `requires`), when the app is
built, never per frame.

Refusals that change the outcome (a duplicate command id, a plugin whose
requirement is missing, a dependency cycle) are typed errors, not
warnings.

## Consequences

- Every registry method returns a `Result` or records a diagnostic; none
  returns `()` after doing nothing.
- Tests can assert that an app builds with an empty diagnostics list.
- The `log` facade is the only logging dependency; the application picks
  the logger.
