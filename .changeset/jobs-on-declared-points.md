---
egui-pop-model: major
egui-pop-api: major
egui-pop-test: major
egui-pop: major
---

The jobs centre follows declared points, not journal names. A job event is now published on `pop.job.event` (`JobEvents`), typed by a `JobEventKind` (started, progress, item failed, paused, resumed, cancelling, failed, finished, cancelled, orphaned, adopted, log, and a job command's dispatch, answer or failure). Before, the jobs model folded every journal entry whose name began with `job.`. Now no listener reads an entry's name: a journal entry named `job.started` that was not published on the point changes nothing. Each event is still recorded in the journal under its label (`job.started`…), so the journal view and its readers keep working.

The host publishes job events with a `JobPublisher`, which is `Send + Sync` and works from any thread. The application hands one to its host through the new `Host::set_job_publisher` (a no-op by default). `AppHandle::job_publisher` and `AppHandle::publish_job` give the same publisher.

`egui_pop_model::jobs::Incoming` carries a `kind: JobEventKind` instead of a `name`, and `Jobs::apply` folds by kind. The `JobEvents` input is a `JobEvent` (`seq`, `source`, `cause`, `kind`, `data`) instead of a journal `Event`. `egui_pop_test::jobs::FakeHost::emit` and `JobScript::event` take a `JobEventKind`, and `emit` no longer returns a sequence number.

egui-pop now requires bus-kernel 0.3.1, which fixes the conformance check `crossing.declared_requests_fail_closed_alter_never`. That check failed on any bus an application had declared points on. egui-pop's journal conformance test now runs the full suite again.

Migration for hosts: before, a host had to name its job facts `job.*` in the journal it gives egui-pop, and so carry egui-pop's names. Now it keeps the publisher from `Host::set_job_publisher` (or `AppHandle::job_publisher`) and publishes each of its own job facts as `publisher.publish(source, JobEventKind::Started, data)`, with the same data as before (see `egui_pop_model::jobs`). A host that renamed its facts to `job.*` for egui-pop can drop that renaming. An in-process runner (`JobsPlugin::run_locally`) answers with `app.publish_job(..)` instead of `app.publish(.., "job.cancelled", ..)`.
