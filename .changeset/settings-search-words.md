---
egui-pop-model: patch
egui-pop-api: patch
egui-pop: patch
---

The settings search no longer matches a word as letters scattered over a long description: labels and keywords are still matched fuzzily, but descriptions only by whole words or word prefixes, and each field on its own. Within a section, a setting found by its label comes before one found by a keyword, then by its description. `fuzzy::Fields` (fields of a tier: label, keyword, description, found best first) and `fuzzy::word_prefix` do this for any search field.
