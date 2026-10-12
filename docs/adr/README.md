# Architecture decision records

One page per decision that shapes the code: what was known (context), what
was decided, what follows (consequences). An ADR is dated and never edited
after the fact except to mark it superseded by a later one.

| # | Decision | Date |
|---|---|---|
| [0001](0001-layering.md) | Crates in layers, no back-edges, checked in CI | 2026-10-11 |
| [0002](0002-typed-ids.md) | Typed ids that serialize to strings | 2026-10-11 |
| [0003](0003-no-silent-no-op.md) | No silent no-op | 2026-10-11 |
| [0004](0004-plugins.md) | Plugins own what they register | 2026-10-11 |
| [0005](0005-zones-and-blocks.md) | Zones and blocks | 2026-10-11 |
| [0006](0006-theme-in-core.md) | The theme moves to `egui-pop-core`, `egui-pop` becomes the facade | 2026-10-11 |
| [0007](0007-key-resolution.md) | Key resolution: focus depth, layers, removal | 2026-10-11 |
| [0008](0008-persisted-documents.md) | Persisted documents: versions, migrations, atomic writes | 2026-10-11 |
| [0009](0009-value-binding.md) | Three ways to bind a value | 2026-10-11 |
| [0010](0010-host.md) | egui-pop wraps the application's kernel through a host | 2026-10-12 |

Template: copy any of them; keep it to half a page.
