---
egui-pop-hooks: minor
egui-pop-api: minor
---

Local listeners on the bus (bus-contract 0.2.0): `Kernel::listen_local` registers a listener that is not `Send` (an interface's `Rc<RefCell<…>>`), bound to the thread that registered it and ordered with the other listeners. An emit on that thread runs it inline; an emit from another thread runs the others and queues it (`Altered::deferred`, `Collected::deferred`), calls the `on_pending` wakers, and the thread runs the queue in order with `run_pending()`. `unlisten` and `forget` remove local listeners from any thread, queued runs included; `ListenerInfo::local` and the order dump mark them. egui-pop's application runs the queue once per pass, before the `On::Pass` handlers (`AppHandle::run_pending`), and asks for a pass when an emit queues work. An application supplying a bus of its own implements bus-contract 0.2.0's `Bus`, which includes these three methods (`listen_local_any`, `run_pending`, `add_pending_waker`).
