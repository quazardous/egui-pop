//! The indicators page: every kind of micro-indicator in its states
//! (dots and LEDs, badges, pips, flags, swatches, icons by state),
//! passive and interactive, on widgets (an icon button, a tab, a row, a
//! tile, a menu entry), a pretend status bar, and a playground. Motion
//! is frozen in the tests, so the pulsing ones are fixtures too.

use egui::{Color32, CornerRadius, Sense, Ui, vec2};
use egui_pop::core::icon::Icon;
use egui_pop::core::indicator::{
    self, Anchor, Direction, Flag, FlagShape, Indicator, IndicatorExt as _, Led, Pip, Size, Swatch,
    Tone,
};
use egui_pop::core::motion;
use egui_pop::core::widgets;
use egui_pop::theme::Tokens;

use super::section;

/// What the playground shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    Dot,
    #[default]
    Led,
    Badge,
    Pip,
    Flag,
    Swatch,
    Icon,
}

const KINDS: [(Kind, &str); 7] = [
    (Kind::Dot, "Dot"),
    (Kind::Led, "LED"),
    (Kind::Badge, "Badge"),
    (Kind::Pip, "Pip"),
    (Kind::Flag, "Flag"),
    (Kind::Swatch, "Swatch"),
    (Kind::Icon, "Icon"),
];

/// The page's state and the playground's options.
#[derive(Clone, Debug)]
pub struct State {
    pub kind: Kind,
    pub led: Led,
    pub count: u64,
    pub tone: Tone,
    pub size: Size,
    pub anchor: Anchor,
    pub interactive: bool,
    pub right_to_left: bool,
    /// When the playground's mark was told to call attention (motion
    /// clock seconds).
    pub attention: Option<f64>,
    /// The demo toggles and how often the reset dot was pressed.
    pub visible: bool,
    pub locked: bool,
    pub resets: u32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            kind: Kind::Led,
            led: Led::Busy,
            count: 7,
            tone: Tone::Accent,
            size: Size::Regular,
            anchor: Anchor::TopEnd,
            interactive: false,
            right_to_left: false,
            attention: None,
            visible: true,
            locked: false,
            resets: 0,
        }
    }
}

fn tone_name(tone: Tone) -> &'static str {
    match tone {
        Tone::Neutral => "Neutral",
        Tone::Muted => "Muted",
        Tone::Accent => "Accent",
        Tone::Info => "Info",
        Tone::Success => "Success",
        Tone::Warning => "Warning",
        Tone::Danger => "Danger",
        Tone::Record => "Record",
    }
}

fn led_name(led: Led) -> &'static str {
    match led {
        Led::Off => "Off",
        Led::On => "On",
        Led::Busy => "Busy",
        Led::Warning => "Warning",
        Led::Error => "Error",
    }
}

fn anchor_name(anchor: Anchor) -> &'static str {
    match anchor {
        Anchor::TopStart => "Top start",
        Anchor::TopEnd => "Top end",
        Anchor::BottomStart => "Bottom start",
        Anchor::BottomEnd => "Bottom end",
        Anchor::Start => "Start",
        Anchor::End => "End",
        Anchor::Center => "Centre",
    }
}

fn size_name(size: Size) -> &'static str {
    match size {
        Size::Small => "Small",
        Size::Regular => "Regular",
        Size::Large => "Large",
    }
}

pub fn playground(ui: &mut Ui, state: &mut State) {
    let ctx = ui.ctx().clone();
    combo(ui, "Kind", "indicators.kind", &mut state.kind, &KINDS);
    let leds: Vec<(Led, &str)> = Led::ALL.iter().map(|&l| (l, led_name(l))).collect();
    combo(ui, "LED state", "indicators.led", &mut state.led, &leds);
    let label = ui.label("Count");
    ui.add(egui::DragValue::new(&mut state.count).range(0..=5000))
        .labelled_by(label.id);
    let tones: Vec<(Tone, &str)> = std::iter::once(Tone::Neutral)
        .chain(Tone::NAMED.iter().map(|(t, _)| *t))
        .map(|t| (t, tone_name(t)))
        .collect();
    combo(ui, "Tone", "indicators.tone", &mut state.tone, &tones);
    let sizes: Vec<(Size, &str)> = Size::ALL.iter().map(|&s| (s, size_name(s))).collect();
    combo(ui, "Size", "indicators.size", &mut state.size, &sizes);
    let anchors: Vec<(Anchor, &str)> = Anchor::ALL.iter().map(|&a| (a, anchor_name(a))).collect();
    combo(
        ui,
        "Anchor",
        "indicators.anchor",
        &mut state.anchor,
        &anchors,
    );
    widgets::checkbox(ui, &mut state.interactive, "Interactive");
    widgets::checkbox(ui, &mut state.right_to_left, "Right to left");
    if ui.button("Call attention").clicked() {
        state.attention = Some(motion::now(&ctx));
    }
    ui.weak("Reduced motion (the top bar's switch) turns the pulse into a still ring.");
}

fn combo<T: Copy + PartialEq>(
    ui: &mut Ui,
    label: &str,
    salt: &str,
    value: &mut T,
    choices: &[(T, &str)],
) {
    let l = ui.label(label);
    widgets::combo_box(salt)
        .selected_text(choices.iter().find(|c| c.0 == *value).map_or("", |c| c.1))
        .show_ui(ui, |ui| {
            for &(v, name) in choices {
                widgets::selectable_value(ui, value, v, name);
            }
        })
        .response
        .labelled_by(l.id);
}

/// The playground's mark.
fn chosen(state: &State) -> Indicator {
    let mut i = match state.kind {
        Kind::Dot => Indicator::dot(state.tone).name("Something changed"),
        Kind::Led => Indicator::led(state.led),
        Kind::Badge => Indicator::count(state.count),
        Kind::Pip => Indicator::pip(Pip::Progress(0.6)),
        Kind::Flag => Indicator::flag(Flag::Modified),
        Kind::Swatch => Indicator::swatch(Color32::from_rgb(0x3d, 0x8b, 0xd9)),
        Kind::Icon => Indicator::toggle(state.visible, Icon::Visible, Icon::Hidden),
    }
    .size(state.size)
    .id("playground");
    // An LED keeps its state's tone.
    if state.kind != Kind::Led {
        i = i.tone(state.tone);
    }
    if let Some(since) = state.attention {
        i = i.attention(since);
    }
    if state.interactive {
        i = i.interactive("Playground mark");
    }
    i
}

pub fn show(ui: &mut Ui, state: &mut State) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    ui.label("Small marks that say a state beside or on another widget. A state is never said by colour alone: each has a shape and a name. Passive marks add their meaning to their host's accessible name; interactive ones are buttons with a 24-point target.");

    section(ui, "Dots and LEDs");
    ui.horizontal_wrapped(|ui| {
        for led in Led::ALL {
            ui.add(Indicator::led(led).id(format!("led.{}", led.name())));
            ui.label(led_name(led));
            ui.add_space(t.space.m);
        }
    });
    ui.horizontal_wrapped(|ui| {
        for tone in [
            Tone::Neutral,
            Tone::Accent,
            Tone::Info,
            Tone::Success,
            Tone::Warning,
            Tone::Danger,
            Tone::Record,
        ] {
            ui.add(Indicator::dot(tone).name(tone_name(tone)));
            ui.weak(tone_name(tone));
            ui.add_space(t.space.s);
        }
    });

    section(ui, "Badges");
    ui.horizontal_wrapped(|ui| {
        ui.add(Indicator::count(0).show_zero(true).tone(Tone::Neutral));
        for n in [3, 42, 99, 100] {
            ui.add(
                Indicator::count(n)
                    .tone(Tone::Danger)
                    .id(format!("badge.{n}")),
            );
        }
        ui.add(Indicator::count(1234).cap(999).tone(Tone::Info));
        ui.add(Indicator::text("NEW").tone(Tone::Success));
        ui.add(Indicator::text("!").tone(Tone::Warning));
        ui.add(Indicator::text("REC").tone(Tone::Record));
        ui.add(Indicator::count(5).tone(Tone::Muted));
        ui.weak("Zero is hidden unless asked; past the cap, 99+ (the name keeps the count).");
    });

    section(ui, "Activity pips");
    ui.horizontal_wrapped(|ui| {
        for (pip, label) in [
            (Pip::Busy, "Busy"),
            (Pip::Progress(0.25), "A quarter"),
            (Pip::Progress(0.75), "Three quarters"),
            (Pip::Done, "Done"),
            (Pip::Failed, "Failed"),
        ] {
            ui.add(Indicator::pip(pip));
            ui.weak(label);
            ui.add_space(t.space.s);
        }
    });

    section(ui, "Flags");
    ui.horizontal_wrapped(|ui| {
        for flag in Flag::ALL {
            ui.add(Indicator::flag(flag).id(format!("flag.{}", flag.name())));
            ui.weak(flag.name());
            ui.add_space(t.space.s);
        }
        for (shape, name) in [(FlagShape::Stripe, "Stripe"), (FlagShape::Square, "Square")] {
            ui.add(Indicator::flag_shape(shape).tone(Tone::Info).name(name));
            ui.weak(name);
            ui.add_space(t.space.s);
        }
    });

    section(ui, "Swatches");
    ui.horizontal_wrapped(|ui| {
        for (color, label) in [
            (Color32::from_rgb(0xd9, 0x48, 0x3d), "Red"),
            (Color32::from_rgb(0x3d, 0x8b, 0xd9), "Blue"),
            (
                Color32::from_rgba_unmultiplied(0x3d, 0xd9, 0x6a, 110),
                "Translucent green",
            ),
        ] {
            ui.add(Indicator::swatch(color).name(label));
            ui.weak(label);
            ui.add_space(t.space.s);
        }
        ui.add(Indicator::swatch_of(Swatch::None));
        ui.weak("None");
        ui.add(Indicator::swatch_of(Swatch::Mixed));
        ui.weak("Mixed");
    });

    section(ui, "Interactive");
    ui.horizontal_wrapped(|ui| {
        if ui
            .add(
                Indicator::dot(Tone::Accent)
                    .name("Changed")
                    .interactive("Reset exposure")
                    .tooltip("Reset the exposure to its default")
                    .id("reset"),
            )
            .clicked()
        {
            state.resets += 1;
        }
        ui.label(format!("Exposure resets: {}", state.resets));
        ui.add(
            Indicator::dot(Tone::Accent)
                .name("Changed")
                .interactive("Reset contrast")
                .disabled("the contrast is held by a preset"),
        );
        ui.weak("Disabled");
        if ui
            .add(
                Indicator::toggle(state.visible, Icon::Visible, Icon::Hidden)
                    .interactive("Mask visible")
                    .tooltip("Show or hide the mask")
                    .id("mask.visible"),
            )
            .clicked()
        {
            state.visible = !state.visible;
        }
        if ui
            .add(
                Indicator::toggle(state.locked, Icon::Lock, Icon::Unlock)
                    .tone(if state.locked {
                        Tone::Warning
                    } else {
                        Tone::Muted
                    })
                    .interactive("Mask locked")
                    .tooltip("Lock or unlock the mask")
                    .id("mask.locked"),
            )
            .clicked()
        {
            state.locked = !state.locked;
        }
        ui.weak("Toggles are check boxes.");
    });

    section(ui, "On widgets");
    indicator::set_direction(
        &ctx,
        if state.right_to_left {
            Direction::RightToLeft
        } else {
            Direction::LeftToRight
        },
    );
    ui.horizontal_wrapped(|ui| {
        ui.add(
            Icon::Notification
                .size(egui_pop::core::icon::Size::Toolbar)
                .label("Notifications")
                .sense_click(),
        )
        .with_indicator(
            ui,
            Indicator::count(12)
                .tone(Tone::Danger)
                .id("on.notifications"),
            Anchor::TopEnd,
        );
        ui.add_space(t.space.l);
        ui.button("Inbox").with_indicator(
            ui,
            Indicator::dot(Tone::Accent).name("new mail"),
            Anchor::TopEnd,
        );
        ui.add_space(t.space.l);
        widgets::selectable_label(ui, true, "Masks").with_indicator(
            ui,
            Indicator::count(2).tone(Tone::Warning).name("2 warnings"),
            Anchor::TopEnd,
        );
        ui.add_space(t.space.l);
        // A row: a stripe at its start, the modified flag in its corner.
        let row = widgets::selectable_label(ui, false, "    exposure.tif    ");
        let row = row.with_indicator(
            ui,
            Indicator::flag_shape(FlagShape::Stripe)
                .tone(Tone::Accent)
                .name("in the stack"),
            Anchor::Start,
        );
        row.with_indicator(ui, Indicator::flag(Flag::Modified), Anchor::TopEnd);
        ui.add_space(t.space.l);
        // A tile.
        let (tile, r) = ui.allocate_exact_size(vec2(96.0, 64.0), Sense::click());
        ui.painter().rect_filled(
            tile,
            CornerRadius::same(t.radius.widget.round() as u8),
            t.surface.sunken,
        );
        egui_pop::core::a11y::name(&r, egui::Role::Button, "IMG_0042");
        Indicator::flag(Flag::Pinned).on(ui, &r, Anchor::TopStart);
        Indicator::swatch(Color32::from_rgb(0xd9, 0x48, 0x3d))
            .name("red label")
            .size(Size::Small)
            .on(ui, &r, Anchor::TopEnd);
        Indicator::text("RAW")
            .tone(Tone::Neutral)
            .overhang(false)
            .on(ui, &r, Anchor::BottomEnd);
        Indicator::pip(Pip::Busy)
            .size(Size::Small)
            .on(ui, &r, Anchor::BottomStart);
    });
    // A menu entry: the swatch drawn beside the entry's label, its
    // colour added to the entry's name by hand.
    ui.horizontal(|ui| {
        let entry = widgets::selectable_label(ui, false, "Accent colour");
        ui.add(Indicator::swatch(t.accent.default).size(Size::Small));
        indicator::annotate(&ctx, entry.id, "the accent");
    });
    ui.weak("Badges and dots overhang a corner on a layer above their host, never clipped; the others sit inside. Right to left (the playground) moves start and end.");
    indicator::set_direction(&ctx, Direction::LeftToRight);

    section(ui, "A status bar");
    status_bar(ui);

    section(ui, "Playground");
    ui.horizontal(|ui| {
        let host = ui.button("Host widget");
        chosen(state).on(ui, &host, state.anchor);
        ui.add_space(t.space.xl);
        ui.add(chosen(state).id("playground.inline"));
        ui.weak("Inline");
    });
    ui.weak("Every mark with an id shows in the debug page's `indicators` collector.");
}

/// A pretend status bar: a node link cycling through its states, a job's
/// pip and a notifications count, on the motion clock.
fn status_bar(ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let now = motion::now(&ctx);
    let step = (now / 2.0).floor();
    motion::wake_at(&ctx, (step + 1.0) * 2.0);
    let link = [
        Led::Busy,
        Led::On,
        Led::On,
        Led::Warning,
        Led::Error,
        Led::Off,
    ][step as usize % 6];
    egui::Frame::new()
        .fill(t.surface.raised)
        .inner_margin(t.space.s)
        .corner_radius(t.radius.widget)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    Indicator::led(link)
                        .id("status.node")
                        .tooltip("The render node's link"),
                );
                ui.label("Node");
                ui.separator();
                ui.add(
                    Indicator::led(Led::On)
                        .tone(Tone::Record)
                        .name("Recording")
                        .id("status.record"),
                );
                ui.label("REC");
                ui.separator();
                ui.add(
                    Indicator::pip(Pip::Progress(((now / 10.0).fract()) as f32))
                        .id("status.export"),
                );
                ui.label("Exporting");
                ui.separator();
                let unread = (step as u64 * 3) % 14;
                ui.button("Notifications").with_indicator(
                    ui,
                    Indicator::count(unread)
                        .tone(Tone::Danger)
                        .id("status.unread"),
                    Anchor::TopEnd,
                );
            });
        });
}
