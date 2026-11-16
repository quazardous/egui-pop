---
egui-pop-hooks: major
egui-pop-api: major
---

Local listeners on the bus (bus-contract 0.2.0): `Kernel::listen_local` registers a listener that is not `Send` (an interface's `Rc<RefCell<…>>`), bound to the thread that registered it and ordered with the other listeners. An emit on that thread runs it inline; an emit from another thread runs the others and queues it (`Altered::deferred`, `Collected::deferred`), calls the `on_pending` wakers, and the thread runs the queue in order with `run_pending()`. `unlisten` and `forget` remove local listeners from any thread, queued runs included; `ListenerInfo::local` and the order dump mark them. egui-pop's application runs the queue once per pass, before the `On::Pass` handlers (`AppHandle::run_pending`), and asks for a pass when an emit queues work. A bus written by an application implements the three new `Bus` methods (`listen_local_any`, `run_pending`, `add_pending_waker`).
