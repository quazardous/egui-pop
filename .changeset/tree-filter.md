---
egui-pop-views: minor
---

A tree can be filtered: `TreeState::set_filter`, typed in a `FilterField`
drawn above the tree. A node shows when every word of the query is in its
match text (its name, plus the words `TreeSource::match_text` adds), as a
substring, ignoring case and the accents of Latin letters. The matches show
with their ancestors, opened for them without touching the fold the tree
keeps; the rest is hidden, a "No match" row says when nothing is left, and
the matched part of each label is underlined in the row's accent tone. The
cursor goes to the first match when its row hides. The field shows "3 of
57" and announces it; Ctrl+F or `/` in the tree goes to it (the `tree.filter`
command of `ViewsPlugin`, rebindable), Escape clears it and then leaves it,
Down goes to the tree. Agents read each tree's filter and rows in the new
`trees` debug collector. New strings `egui-pop-views-tree-filter*`,
translated in French.
