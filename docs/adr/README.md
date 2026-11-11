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
| [0014](0014-undo.md) | Undo: histories owned by the kernel, routed by focus, projected by the interface ([analysis](0014-undo-analysis.md)) | 2026-10-14 |
| [0015](0015-clipboard.md) | The clipboard: typed items owned by the kernel, offered in several formats, hooked at each boundary ([analysis](0015-clipboard-analysis.md)) | 2026-10-18 |
| [0016](0016-one-writer-per-value.md) | One writer per value: a path's writers are declared, a write names its scope | 2026-10-20 |
| [0017](0017-links-and-outside-content.md) | Links and outside content: one service opens them, an egui-native viewer shows what is safe, the rest goes outside ([analysis](0017-links-and-outside-content-analysis.md)) | 2026-10-28 |
| [0018](0018-agent-collector-actions.md) | Collector actions say whether agents may call them, through one step | 2026-11-03 |
| [0019](0019-actions-act-as-their-caller.md) | Collector actions and value writes act as their caller | 2026-11-03 |
| [0020](0020-answers-carry-their-caller.md) | Answers carry their caller, and a dialog keeps its command's flag | 2026-11-03 |
| [0021](0021-origin-along-the-chain.md) | The origin goes along the chain of commands; deep links are remote | 2026-11-03 |
| [0022](0022-sections-and-toolbars-in-egui-stack.md) | Sections and a toolbar row move into egui_stack as plain-egui cores; egui-pop wraps them (accepted, [analysis](../egui-stack-directions.md)) | 2026-11-03 |
| [0023](0023-every-command-through-the-host.md) | Every command goes through the host's dispatch; history walks are held to the writers | 2026-11-03 |
| [0024](0024-bridge-modes-and-read-only-commands.md) | The debug bridge has a mode; commands say when they only read | 2026-11-03 |
| [0025](0025-application-windows.md) | Application windows are deferred viewports with an `Rc` shim; the main window is always the first | 2026-11-11 |
| [0026](0026-layer-ranks.md) | Layers have ranks, pinned under base layers at the bottom of the foreground | 2026-11-11 |

Template: copy any of them; keep it to half a page.
