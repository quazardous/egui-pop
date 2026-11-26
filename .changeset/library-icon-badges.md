---
egui-pop-views: major
egui-pop-i18n: major
egui-pop: major
---

A library badge can be an icon: `Badge::icon("phosphor:plugs", "offline")`
draws an icon of the registry on a pill of its tone, in the cell's bottom
right row with the text badges, its name as its tooltip. `Badge::named`
gives a text badge a name in words ("R+J" named "RAW+JPG"). A grid cell's
accessible name now says its badges after the image's name
(`IMG_0001.CR3 · offline · 3 versions`), as the file browser says its
entries' marks; a cell without badges keeps its bare name. An icon badge's
id for agents is `library.badge:<image>:<name>`. A badge as the file
browser's chip keeps its icon and name.

Breaking: `Badge` gains the public fields `icon` and `name`; a badge built
as a struct literal adds them (`..`-less literals no longer compile).
