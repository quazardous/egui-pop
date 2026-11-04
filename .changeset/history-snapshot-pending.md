---
egui-pop-model: major
egui-pop-api: minor
---

A history snapshot can be pending: `Snapshot::pending` says its owner keeps it
only at its next save, and loses it if the application quits first (an owner
whose history is saved, as an image editor's). The history panel shows such a
snapshot's name with "(not saved yet)" in italics, and its tooltip says when it
is kept. A snapshot read from an owner that does not set the flag is not
pending. `Snapshot` gains a field: an owner building one by its fields adds
`pending`.
