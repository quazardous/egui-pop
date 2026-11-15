---
egui-pop-api: patch
egui-pop-i18n: patch
---

A settings page read again after a change (a command run, a refresh event) keeps showing its last rows and statuses until the new answer comes, with a small "Refreshing…" mark beside the list, instead of blanking while it reads; `Admin::refreshing` says whether a read shows its last answer.
