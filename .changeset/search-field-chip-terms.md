---
egui-pop-views: minor
egui-pop: minor
---

An application chip of `SearchField` says the part of the search text it
stands for (`AppChip::term`): with `whole_text(true)`, the filter's text is
the chips' terms, then the text typed, so the `library.filter` step, its
hooks and the filter the registry publishes see the whole search, not only
what is typed. A filter set whole (the `library.filter` command, a hook's
redirect or veto) is typed whole and reported as `SearchEvent::FilterSet`:
the application drops its chips.
