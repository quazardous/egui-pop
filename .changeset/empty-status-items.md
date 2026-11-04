---
egui-pop-api: patch
---

A status item with neither text nor icon (nor spinner nor counts) is no
longer drawn as an empty button without an accessible name: it takes no
room until its reducer gives it something to show, and agents still list
it.
