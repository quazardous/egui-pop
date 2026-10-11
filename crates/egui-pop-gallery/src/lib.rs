//! The egui-pop gallery: a catalogue application with no purpose of its
//! own. It shows and exercises every component in every variant and state,
//! under every theme, density and accent.
//!
//! Layout: a toolbar with the theme, density, accent and motion switchers;
//! a sidebar listing the pages (one per component family); the page; and a
//! playground beside it where the page's options are toggled live.
//!
//! Every page is a pair of plain functions (`show` and `playground`) over
//! its own state, so tests (egui_kittest snapshots) and scripted runs reuse
//! the very same pages as fixtures.

use std::io::Write as _;

use egui::{Id, Ui};
use egui_pop::theme::{self, Density, ThemeSpec, Themes, Tokens};

mod options;
pub mod pages;

pub use options::{Options, USAGE};

/// The window title, also printed in the start-up line.
pub const TITLE: &str = "egui-pop gallery";

/// A page of the gallery.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Page {
    /// The theme at a glance: surfaces, text, accents, spacing, contrast.
    #[default]
    Overview,
    /// Every resolved token, by tier.
    Tokens,
    /// egui's own widgets, drawn with the theme.
    Builtins,
}

impl Page {
    pub const ALL: [Page; 3] = [Page::Overview, Page::Tokens, Page::Builtins];

    /// The name used on the command line and in snapshot names.
    pub fn name(self) -> &'static str {
        match self {
            Page::Overview => "overview",
            Page::Tokens => "tokens",
            Page::Builtins => "builtins",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Page::Overview => "Theme overview",
            Page::Tokens => "Tokens table",
            Page::Builtins => "egui built-ins",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Page::Overview => {
                "Surfaces, text, accents, status colours, spacing and the contrast checks of the current theme."
            }
            Page::Tokens => "Every token of the current theme: palette, roles and components.",
            Page::Builtins => {
                "egui's own widgets, styled from the theme's tokens with no per-widget code."
            }
        }
    }

    /// Draws the page.
    pub fn show(self, ui: &mut Ui, state: &mut PageState) {
        match self {
            Page::Overview => pages::overview::show(ui, &mut state.overview),
            Page::Tokens => pages::tokens::show(ui, &mut state.tokens),
            Page::Builtins => pages::builtins::show(ui, &mut state.builtins),
        }
    }

    /// Draws the page's playground: its live options.
    pub fn playground(self, ui: &mut Ui, state: &mut PageState) {
        match self {
            Page::Overview => pages::overview::playground(ui, &mut state.overview),
            Page::Tokens => pages::tokens::playground(ui, &mut state.tokens),
            Page::Builtins => pages::builtins::playground(ui, &mut state.builtins),
        }
    }
}

impl std::str::FromStr for Page {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Page::ALL
            .into_iter()
            .find(|p| p.name() == s)
            .ok_or_else(|| format!("`{s}` is not a page (overview, tokens, builtins)"))
    }
}

/// The state of every page's playground.
#[derive(Clone, Debug, Default)]
pub struct PageState {
    pub overview: pages::overview::State,
    pub tokens: pages::tokens::State,
    pub builtins: pages::builtins::State,
}

/// The gallery application.
#[derive(Debug)]
pub struct Gallery {
    themes: Themes,
    /// The theme asked for; installed between frames when it changes.
    pub spec: ThemeSpec,
    installed: ThemeSpec,
    pub page: Page,
    pub pages: PageState,
    frames: u64,
    quit_after: Option<u64>,
    announce: bool,
}

impl Gallery {
    /// The gallery on a context, its theme installed.
    pub fn new(ctx: &egui::Context, options: Options) -> Gallery {
        let mut spec = ThemeSpec::new(options.theme.clone())
            .density(options.density)
            .reduced_motion(options.reduced_motion);
        spec.accent = options.accent.clone();
        let themes = Themes::default();
        theme::install_with(ctx, &themes, &spec);
        if options.announce {
            let line = serde_json::json!({
                "event": "ready",
                "app": "egui-pop-gallery",
                "version": env!("CARGO_PKG_VERSION"),
                "title": TITLE,
                "theme": spec.base,
                "density": spec.density.name(),
                "accent": spec.accent,
                "reduced_motion": spec.reduced_motion,
                "page": options.page.name(),
                "quit_after": options.quit_after,
            });
            announce(&line);
        }
        Gallery {
            themes,
            installed: spec.clone(),
            spec,
            page: options.page,
            pages: PageState::default(),
            frames: 0,
            quit_after: options.quit_after,
            announce: options.announce,
        }
    }

    /// The theme switchers.
    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.strong(TITLE);
            ui.separator();
            ui.label("Theme");
            egui::ComboBox::from_id_salt("theme")
                .selected_text(self.spec.base.clone())
                .show_ui(ui, |ui| {
                    let names: Vec<String> = theme::BUILT_IN_THEMES
                        .iter()
                        .map(|s| s.to_string())
                        .chain(
                            self.themes
                                .names()
                                .filter(|n| !theme::BUILT_IN_THEMES.contains(n))
                                .map(String::from),
                        )
                        .collect();
                    for name in names {
                        ui.selectable_value(&mut self.spec.base, name.clone(), name);
                    }
                });
            ui.label("Density");
            egui::ComboBox::from_id_salt("density")
                .selected_text(self.spec.density.name())
                .show_ui(ui, |ui| {
                    for d in Density::ALL {
                        ui.selectable_value(&mut self.spec.density, d, d.name());
                    }
                });
            ui.label("Accent");
            egui::ComboBox::from_id_salt("accent")
                .selected_text(self.spec.accent.as_deref().unwrap_or("theme's own"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.spec.accent, None, "theme's own");
                    for scale in self.themes.accents(&self.spec.base) {
                        ui.selectable_value(&mut self.spec.accent, Some(scale.clone()), scale);
                    }
                });
            ui.checkbox(&mut self.spec.reduced_motion, "Reduced motion");
        });
    }

    fn sidebar(&mut self, ui: &mut Ui) {
        ui.add_space(Tokens::of(ui.ctx()).space.s);
        for page in Page::ALL {
            ui.selectable_value(&mut self.page, page, page.title());
        }
    }

    /// Draws the whole gallery in a `Ui` covering the window.
    pub fn show(&mut self, ui: &mut Ui) {
        egui::Panel::top(Id::new("gallery.toolbar")).show(ui, |ui| self.toolbar(ui));
        egui::Panel::left(Id::new("gallery.pages"))
            .resizable(false)
            .exact_size(150.0)
            .show(ui, |ui| self.sidebar(ui));
        egui::Panel::right(Id::new("gallery.playground"))
            .resizable(false)
            .exact_size(220.0)
            .show(ui, |ui| {
                ui.heading("Playground");
                ui.separator();
                self.page.playground(ui, &mut self.pages);
            });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading(self.page.title());
            ui.weak(self.page.description());
            ui.separator();
            self.page.show(ui, &mut self.pages);
        });
    }

    /// Between frames: installs a newly picked theme, counts frames.
    pub fn logic(&mut self, ctx: &egui::Context) {
        if self.spec != self.installed {
            theme::install_with(ctx, &self.themes, &self.spec);
            self.installed = self.spec.clone();
        }
        self.frames += 1;
        if let Some(limit) = self.quit_after {
            if self.frames >= limit {
                if self.announce {
                    announce(&serde_json::json!({ "event": "quit", "frames": self.frames }));
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.request_repaint();
            }
        }
    }
}

fn announce(line: &serde_json::Value) {
    let mut out = std::io::stdout().lock();
    // A closed stdout is no reason to stop the gallery.
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

impl eframe::App for Gallery {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        Gallery::logic(self, ctx);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }

    #[cfg(target_arch = "wasm32")]
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(&mut *self)
    }
}
