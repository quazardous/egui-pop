---
egui-pop-api: patch
---

The in-app file picker on Windows: a folder given with a verbatim prefix
(`\\?\C:\…`, as `std::fs::canonicalize` returns) shows its root on the
breadcrumbs as the drive or the share (`C:\`, `\\server\share`) rather
than the prefix, and a path typed with `/` in such a folder is read with
Windows' own separator, which a verbatim path requires.
