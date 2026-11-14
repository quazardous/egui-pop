---
egui-pop-agent: patch
---

A `ui` request on another window than the main one is answered while the main window is idle: the agent wakes the main window by name (a wake during another window's frame no longer goes to that window), and keeps asking for the main window's frames while a request waits on another window's tree or its queued input, which also gives an immediate viewport its frames.
