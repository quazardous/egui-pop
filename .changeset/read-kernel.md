---
egui-pop-api: minor
---

A read kernel (`reads::ReadKernel`): a host gives a `ReadTransport` and per-command policies (how long a value stays fresh, which events make it out of date), and the Settings app reads the commands that transport serves through it (`Admin::attach_reads`): away from the UI thread, so a page draws its labels at once and fills its values in as they come; reads are kept by command and arguments, shared while in flight, and a value is never blanked while it is read again.
