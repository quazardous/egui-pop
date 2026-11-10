---
egui-pop-core: major
egui-pop-i18n: patch
egui-pop-api: patch
egui-pop-viewport: patch
egui-pop-views: patch
egui-pop-editors: patch
---

Text placed in a translated string stays as it is written: a folder
named "2026" no longer shows as "2,026", nor "007", "1e3" or "3.14" as
numbers. `strings::tr_args` and `lookup_args` now place their text
arguments verbatim; a count or a size to write as the language writes
numbers goes through the new `strings::tr_with` (`lookup_with`,
`fill_with`), whose arguments are typed `strings::Arg` values,
`Arg::Str` or `Arg::Num`, built with `into()` from text or a number.
`Strings::get_args` takes `Arg`s: a provider formats `Arg::Num` only.
Every count egui-pop shows (jobs, errors, selections, tree and log
filters, graph and curve values, wizard steps and the rest) is passed as
a number and still formatted per locale.
