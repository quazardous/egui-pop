---
egui-pop-core: patch
---

In sections sized by their content (`Sections::fill(false)`), a title or boundary drag moves the bands again when the open sections above are at their content's height: down, the section above grows past its rows, into the room under the last band; up, it shrinks to its least height. Content sizing only places the sections until a drag.
