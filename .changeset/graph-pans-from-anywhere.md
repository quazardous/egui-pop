---
egui-pop-viewport: patch
---

The node graph pans with a middle drag, or a drag with Space held,
wherever the drag starts on it: over a node, a folded stack, a group's
title, a port or a wire too. Before, those took the drag (a Space drag
moved the node), so on a dense graph the drags seemed not to pan at all.
