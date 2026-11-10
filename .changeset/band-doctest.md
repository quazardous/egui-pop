---
egui-pop-core: patch
---

`sections::band`'s example is compiled but not run: a bare context has
no fonts loaded, so running it panicked in the doctest.
