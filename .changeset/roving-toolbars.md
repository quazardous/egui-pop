---
egui-pop-core: minor
egui-pop-api: minor
---

A toolbar is one Tab stop: Tab from any tool leaves it, Tab coming back lands
on the tool focused last, and the arrows along it go from tool to tool, round
from one end to the other, Home and End to its ends, a greyed tool passed
over. The helper is `tab::roving` (with `tab::Axis` and `tab::rovings`), for
any row or column of widgets, with the focus plugin.
