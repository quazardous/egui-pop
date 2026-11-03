//! The test kit on itself and on a small application.

use egui_kittest::kittest::Queryable as _;
use egui_pop_core::theme::ThemeSpec;
use egui_pop_test::{a11y_tree, assert_all_named, double_click, themed, unnamed_interactive};

#[test]
fn unnamed_interactive_nodes_are_found() {
    // kittest's own check (egui 0.37) would fail the run on the unnamed
    // button this test is about: off here.
    let mut h = egui_kittest::Harness::builder()
        .with_size(egui::vec2(300.0, 120.0))
        .with_accessibility_check(false)
        .build_ui(|ui| {
            let _ = ui.button("Named");
            // A button with an empty label says nothing to a screen reader.
            let _ = ui.button("");
            ui.label("Text is not interactive");
        });
    egui_pop_core::theme::install(&h.ctx, &ThemeSpec::default());
    h.run();
    let unnamed = unnamed_interactive(&h);
    assert_eq!(unnamed.len(), 1, "{unnamed:?}");
    assert!(
        unnamed[0].starts_with("Button") || unnamed[0].contains("Button"),
        "{unnamed:?}"
    );
    let tree = a11y_tree(&h);
    assert!(tree.contains("Button \"Named\""), "{tree}");
    assert!(
        tree.contains("Label value=\"Text is not interactive\""),
        "{tree}"
    );
    assert!(!tree.contains("TextRun"), "{tree}");
}

#[test]
fn a_double_click_registers() {
    let mut count = 0;
    {
        let mut h = themed(
            &ThemeSpec::new("pop-light"),
            egui::vec2(200.0, 80.0),
            |ui| {
                if ui
                    .add(egui::Label::new("Open").sense(egui::Sense::click()))
                    .double_clicked()
                {
                    count += 1;
                }
            },
        );
        h.run();
        double_click(&mut h, "Open");
        h.run();
        assert_all_named(&h);
        assert!(h.query_by_label("Open").is_some());
    }
    assert_eq!(count, 1);
}

#[test]
fn an_app_harness_runs_an_application_on_its_context() {
    let mut h = egui_pop_test::app(
        egui_pop_api::App::builder().theme(ThemeSpec::new("pop-high-contrast")),
        egui::vec2(800.0, 500.0),
    );
    h.run();
    let tokens = egui_pop_core::theme::Tokens::of(&h.ctx);
    assert_eq!(tokens.resolved().spec().base, "pop-high-contrast");
    assert!(h.query_by_label("File").is_some());
    let snapshot = h.state().handle().debug_snapshot();
    assert_eq!(snapshot["theme"]["spec"]["base"], "pop-high-contrast");
    let _ = egui_pop_model::ids::OwnerId::FRAMEWORK;
}

#[test]
fn the_shell_and_its_palette_name_every_interactive_node() {
    let mut h = egui_pop_test::app(
        egui_pop_api::App::builder().plugins(egui_pop_api::DefaultPlugins),
        egui::vec2(800.0, 500.0),
    );
    h.run();
    assert_all_named(&h);
    h.key_press_modifiers(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::P,
    );
    h.run();
    assert!(egui_pop_api::palette::is_open(h.state().handle()));
    assert_all_named(&h);
}

struct Clash;

impl egui_pop_api::Plugin for Clash {
    fn id(&self) -> egui_pop_model::ids::PluginId {
        egui_pop_model::ids::PluginId::from_static("clash")
    }

    fn connect(
        &mut self,
        ctx: &mut egui_pop_api::Ctx<'_>,
    ) -> Result<(), egui_pop_api::PluginError> {
        use egui_pop_model::commands::CommandSpec;
        use egui_pop_model::ids::CommandId;
        ctx.command(
            CommandSpec::new(CommandId::from_static("clash.quit"), "Also quit", "Clash")
                .key("Ctrl+Q"),
            |_| Ok(serde_json::Value::Null),
        )?;
        ctx.keybinding(
            "Ctrl+Alt+N",
            CommandId::from_static("nobody.declares"),
            "",
            egui_pop_model::keymap::Layer::Default,
        )?;
        Ok(())
    }
}

#[test]
fn a_plugin_runs_with_the_defaults_and_its_keymap_is_checked() {
    let mut h = egui_pop_test::harness_for(Clash);
    h.run();
    egui_pop_test::assert_all_named(&h);
    let problems = egui_pop_test::keymap_problems(h.state());
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(problems[0].contains("Ctrl+Q"), "{problems:?}");
    assert!(problems[1].contains("nobody.declares"), "{problems:?}");
    let clean = egui_pop_test::app(
        egui_pop_api::App::builder().plugins(egui_pop_api::DefaultPlugins),
        egui::vec2(800.0, 500.0),
    );
    egui_pop_test::assert_keymap_clean(clean.state());
}

/// Two frames of the context stack that can be on together, each
/// binding Z.
struct FrameClash;

impl egui_pop_api::Plugin for FrameClash {
    fn id(&self) -> egui_pop_model::ids::PluginId {
        egui_pop_model::ids::PluginId::from_static("frame_clash")
    }

    fn requires(&self) -> Vec<egui_pop_model::ids::PluginId> {
        vec![egui_pop_api::contexts::PLUGIN]
    }

    fn connect(
        &mut self,
        ctx: &mut egui_pop_api::Ctx<'_>,
    ) -> Result<(), egui_pop_api::PluginError> {
        use egui_pop_api::contexts::FrameSpec;
        use egui_pop_model::commands::CommandSpec;
        use egui_pop_model::ids::{CommandId, FrameId, GroupId};
        let culling = FrameSpec::mode(
            FrameId::from_static("clash.culling"),
            GroupId::from_static("orientation"),
            "Culling",
        );
        let review = FrameSpec::mode(
            FrameId::from_static("clash.review"),
            GroupId::from_static("orientation"),
            "Review",
        );
        let compare = FrameSpec::mode(
            FrameId::from_static("clash.compare"),
            GroupId::from_static("compare"),
            "Compare",
        );
        for (id, title) in [
            ("clash.pick", "Pick"),
            ("clash.approve", "Approve"),
            ("clash.zoom", "Zoom"),
        ] {
            ctx.command(
                CommandSpec::new(CommandId::from_static(id), title, "Clash"),
                |_| Ok(serde_json::Value::Null),
            )?;
        }
        // Culling and review are one group: their Z never compete. Compare
        // is another group: on with either, its Z takes theirs.
        ctx.frame_keybinding(&culling, "Z", CommandId::from_static("clash.pick"))?
            .frame_keybinding(&review, "Z", CommandId::from_static("clash.approve"))?
            .frame_keybinding(&compare, "Z", CommandId::from_static("clash.zoom"))?;
        ctx.frame(culling)?.frame(review)?.frame(compare)?;
        Ok(())
    }
}

#[test]
fn frames_binding_one_key_while_both_on_are_a_keymap_problem() {
    let mut h = egui_pop_test::app(
        egui_pop_api::App::builder()
            .plugins(egui_pop_api::DefaultPlugins)
            .plugin(egui_pop_api::contexts::ContextsPlugin::default())
            .plugin(FrameClash),
        egui::vec2(800.0, 500.0),
    );
    h.run();
    let problems = egui_pop_test::keymap_problems(h.state());
    assert_eq!(problems.len(), 2, "{problems:?}");
    for (mode, command) in [
        ("clash.culling", "clash.pick"),
        ("clash.review", "clash.approve"),
    ] {
        assert!(
            problems.iter().any(|p| p.contains(mode)
                && p.contains("clash.compare")
                && p.contains(command)
                && p.contains("can be on together")),
            "{problems:?}"
        );
    }
    // Said at start too, as a diagnostic, without waiting for a test.
    let said = h
        .state()
        .handle()
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "context-frame-key-clash")
        .count();
    assert_eq!(said, 2);
}

/// `a` declares a value, `b` writes it from a command.
fn stray_app(check: egui_pop_core::model::values::WriterCheck) -> egui_pop_api::App {
    use egui_pop_core::model::commands::{CommandSpec, Origin};
    use egui_pop_core::model::ids::{CommandId, PluginId};
    use egui_pop_core::model::values::{Change, Descriptor, Set, Value};

    struct A;
    impl egui_pop_api::Plugin for A {
        fn id(&self) -> PluginId {
            PluginId::from_static("a")
        }
        fn connect(
            &mut self,
            ctx: &mut egui_pop_api::Ctx<'_>,
        ) -> Result<(), egui_pop_api::PluginError> {
            egui_pop_core::lock(&ctx.app().store())
                .declare(ctx.owner(), "a/v", Descriptor::int("V", 0, 9, 0))
                .unwrap();
            Ok(())
        }
    }
    struct B;
    impl egui_pop_api::Plugin for B {
        fn id(&self) -> PluginId {
            PluginId::from_static("b")
        }
        fn connect(
            &mut self,
            ctx: &mut egui_pop_api::Ctx<'_>,
        ) -> Result<(), egui_pop_api::PluginError> {
            let writer = ctx.writer();
            ctx.command(
                CommandSpec::new(CommandId::from_static("b.write"), "Write", "Test"),
                move |_| {
                    let _ = writer.set(Set::new("a/v", Change::To(Value::Int(1)), Origin::User));
                    Ok(serde_json::Value::Null)
                },
            )?;
            Ok(())
        }
    }
    let mut app = egui_pop_api::App::builder()
        .plugin(A)
        .plugin(B)
        .writer_check(check)
        .build(&egui::Context::default())
        .unwrap();
    egui_pop_test::assert_writes_declared(&app);
    app.run(
        &CommandId::from_static("b.write"),
        serde_json::Value::Null,
        Origin::User,
    );
    app
}

#[test]
fn undeclared_writes_are_listed() {
    use egui_pop_core::model::values::WriterCheck;
    let app = stray_app(WriterCheck::Refuse);
    assert_eq!(
        egui_pop_test::undeclared_writes(&app),
        ["`b` wrote `a/v`, owned by `a` (refused)"]
    );
    let app = stray_app(WriterCheck::Warn);
    assert_eq!(
        egui_pop_test::undeclared_writes(&app),
        ["`b` wrote `a/v`, owned by `a`"]
    );
}

#[test]
#[should_panic(expected = "writes not declared")]
fn undeclared_writes_fail_the_assertion() {
    egui_pop_test::assert_writes_declared(&stray_app(
        egui_pop_core::model::values::WriterCheck::Warn,
    ));
}
