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
| [0011](0011-lifecycle-steps.md) | Every feature's life is reachable: observe, veto, replace, extend | 2026-10-12 |
| [0012](0012-context-stack.md) | The context stack: owned by the kernel, projected by the interface ([analysis](0012-context-stack-analysis.md)) | 2026-10-13 |
| [0013](0013-mount-contract.md) | The mount contract: components declare needs, hosts declare offers ([analysis](0013-mount-contract-analysis.md)) | 2026-10-14 |
| [0015](0015-clipboard.md) | The clipboard: typed items owned by the kernel, offered in several formats, hooked at each boundary (proposed; [analysis](0015-clipboard-analysis.md)) | 2026-10-18 |
| [0016](0016-one-writer-per-value.md) | One writer per value: a path's writers are declared, a write names its scope | 2026-10-20 |

Template: copy any of them; keep it to half a page.
