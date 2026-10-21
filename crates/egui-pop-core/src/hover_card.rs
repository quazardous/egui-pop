//! Rich hover cards: the tier between a tooltip and a popover. A card
//! opens after the hover delay on a widget, stays open while the pointer
//! moves onto it (so its links can be clicked), and closes on leave
//! after a short grace, on Escape or on a click elsewhere.
//!
//! Its content is data ([`Card`]): formatted text (a small Markdown
//! subset: `**bold**`, `` `code` ``, `- ` list lines), rows of key and
//! value, progress, separators and command links. Being data, an agent
//! reads it without hovering ([`Card::plain_text`], or the card
//! serialised), and the same card is the widget's accessible
//! description. Live values come for free: the owner builds the card
//! each pass from its state.
//!
//! [`HoverCard::show`] draws it next to any widget (a status item, a
//! toolbar button, a row, a port) and returns the link clicked, which
//! the owner runs (an egui-pop `App` runs a command with its `when` and
//! its steps, as anywhere else). [`open`] opens one from the keyboard:
//! the focus then goes to its first link, and Tab moves through them.
//!
//! ```
//! use egui_pop_core::hover_card::{Card, HoverCard};
//! use egui_pop_model::ids::CommandId;
//!
//! fn vault(ui: &mut egui::Ui, images: u32) {
//!     let r = ui.button("Vault");
//!     let card = Card::new()
//!         .text("**Vault** is open")
//!         .row("Images", images.to_string())
//!         .row("Last import", "07:12")
//!         .separator()
//!         .link("Show imports", CommandId::from_static("vault.imports"), serde_json::Value::Null);
//!     if let Some(link) = HoverCard::new("vault").show(ui, &r, &card) {
//!         let _ = link.command; // run it
//!     }
//! }
//! ```

use egui::text::LayoutJob;
use egui::{Context, FontId, Id, Key, Modifiers, Response, TextFormat, Ui, WidgetType};
use egui_pop_model::ids::CommandId;
use serde::Serialize;
use serde_json::Value as Json;

use crate::placement::{Placement, Side, place};

/// A link in a card: a command run with its arguments.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Link {
    pub label: String,
    pub command: CommandId,
    #[serde(skip_serializing_if = "Json::is_null")]
    pub args: Json,
}

/// One row of a card's key and value table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Row {
    pub key: String,
    pub value: String,
}

/// A part of a card, top to bottom.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Block {
    /// Formatted text: `**bold**`, `` `code` ``, and lines starting with
    /// `- ` as list items.
    Text {
        text: String,
    },
    /// A table of key and value.
    Rows {
        rows: Vec<Row>,
    },
    /// A progress line, determinate (0 to 1) or not.
    Progress {
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        fraction: Option<f32>,
    },
    Separator,
    /// Command links, on one line.
    Links {
        links: Vec<Link>,
    },
}

/// What a hover card shows.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Card {
    pub blocks: Vec<Block>,
}

impl Card {
    pub fn new() -> Self {
        Self::default()
    }

    /// Formatted text (`**bold**`, `` `code` ``, `- ` list lines).
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.blocks.push(Block::Text { text: text.into() });
        self
    }

    /// A row of key and value; rows in a run make one table.
    pub fn row(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let row = Row {
            key: key.into(),
            value: value.into(),
        };
        match self.blocks.last_mut() {
            Some(Block::Rows { rows }) => rows.push(row),
            _ => self.blocks.push(Block::Rows { rows: vec![row] }),
        }
        self
    }

    /// A progress line (`None`: indeterminate).
    pub fn progress(mut self, label: impl Into<String>, fraction: Option<f32>) -> Self {
        self.blocks.push(Block::Progress {
            label: label.into(),
            fraction: fraction.map(|f| f.clamp(0.0, 1.0)),
        });
        self
    }

    pub fn separator(mut self) -> Self {
        self.blocks.push(Block::Separator);
        self
    }

    /// A command link; links in a run share a line.
    pub fn link(mut self, label: impl Into<String>, command: CommandId, args: Json) -> Self {
        let link = Link {
            label: label.into(),
            command,
            args,
        };
        match self.blocks.last_mut() {
            Some(Block::Links { links }) => links.push(link),
            _ => self.blocks.push(Block::Links { links: vec![link] }),
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Its links, in order.
    pub fn links(&self) -> impl Iterator<Item = &Link> {
        self.blocks.iter().flat_map(|b| match b {
            Block::Links { links } => links.as_slice(),
            _ => &[],
        })
    }

    /// Its text without formatting, a line per line of text, row or
    /// progress: the anchor's accessible description, and what an agent
    /// reads. Links are left out (they are buttons of their own).
    pub fn plain_text(&self) -> String {
        let mut out: Vec<String> = Vec::new();
        for b in &self.blocks {
            match b {
                Block::Text { text } => {
                    for line in lines(text) {
                        let s: String = line.spans.iter().map(Span::text).collect();
                        out.push(if line.bullet { format!("• {s}") } else { s });
                    }
                }
                Block::Rows { rows } => {
                    out.extend(rows.iter().map(|r| format!("{}: {}", r.key, r.value)));
                }
                Block::Progress { label, fraction } => out.push(match fraction {
                    Some(f) => format!("{label} {}%", (f * 100.0).round()),
                    None => label.clone(),
                }),
                Block::Separator | Block::Links { .. } => {}
            }
        }
        out.join("\n")
    }
}

/// A run of text in a line, by style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Span {
    Plain(String),
    Bold(String),
    Code(String),
}

impl Span {
    pub fn text(&self) -> &str {
        match self {
            Span::Plain(s) | Span::Bold(s) | Span::Code(s) => s,
        }
    }
}

/// A line of formatted text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// A list item (`- `).
    pub bullet: bool,
    pub spans: Vec<Span>,
}

/// Formatted text's lines: `- ` starts a list item, `**…**` is bold,
/// `` `…` `` is code (no formatting inside); a mark left open is kept as
/// text.
pub fn lines(text: &str) -> Vec<Line> {
    text.lines()
        .map(|l| match l.strip_prefix("- ") {
            Some(rest) => Line {
                bullet: true,
                spans: spans(rest),
            },
            None => Line {
                bullet: false,
                spans: spans(l),
            },
        })
        .collect()
}

/// A line's spans.
pub fn spans(text: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        let (mark, make): (&str, fn(String) -> Span) = if rest.starts_with("**") {
            ("**", Span::Bold)
        } else if rest.starts_with('`') {
            ("`", Span::Code)
        } else {
            let c = rest.chars().next().unwrap_or_default();
            plain.push(c);
            rest = &rest[c.len_utf8()..];
            continue;
        };
        let inner = &rest[mark.len()..];
        match inner.find(mark) {
            Some(end) if end > 0 => {
                if !plain.is_empty() {
                    out.push(Span::Plain(std::mem::take(&mut plain)));
                }
                out.push(make(inner[..end].to_string()));
                rest = &inner[end + mark.len()..];
            }
            _ => {
                plain.push_str(mark);
                rest = inner;
            }
        }
    }
    if !plain.is_empty() {
        out.push(Span::Plain(plain));
    }
    out
}

/// A card's state, between passes.
#[derive(Clone, Copy, Debug, Default)]
struct State {
    open: bool,
    /// Opened from the keyboard: stays until Escape or a click elsewhere.
    by_key: bool,
    /// The focus goes to the first link next pass.
    focus: bool,
    /// Since when the pointer rests on the anchor.
    hover_since: Option<f64>,
    /// Since when the pointer left both the anchor and the card.
    left_at: Option<f64>,
    /// Clicked: does not open again until the pointer leaves.
    suppressed: bool,
}

fn load(ctx: &Context, id: Id) -> State {
    ctx.data(|d| d.get_temp(id)).unwrap_or_default()
}

fn store(ctx: &Context, id: Id, s: State) {
    ctx.data_mut(|d| d.insert_temp(id, s));
}

/// Opens a card from the keyboard (or for an agent): it stays open until
/// Escape or a click elsewhere, and its first link takes the focus.
pub fn open(ctx: &Context, id: Id) {
    let mut s = load(ctx, id);
    s.open = true;
    s.by_key = true;
    s.focus = true;
    store(ctx, id, s);
    ctx.request_repaint();
}

/// Closes a card.
pub fn close(ctx: &Context, id: Id) {
    let mut s = load(ctx, id);
    s.open = false;
    s.by_key = false;
    s.left_at = None;
    store(ctx, id, s);
}

/// Whether a card is open.
pub fn is_open(ctx: &Context, id: Id) -> bool {
    load(ctx, id).open
}

/// A hover card next to a widget.
#[derive(Clone, Copy, Debug)]
#[must_use = "show it with `show`"]
pub struct HoverCard {
    id: Id,
    side: Side,
}

impl HoverCard {
    pub fn new(id: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Self {
            id: Id::new(id),
            side: Side::Bottom,
        }
    }

    /// The card's id, for [`open`], [`close`] and [`is_open`].
    pub fn id(&self) -> Id {
        self.id
    }

    /// Which side of the widget it opens on (below by default; flipped
    /// when there is no room).
    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }

    /// Follows the pointer over `anchor`, draws the card while it is
    /// open, and gives the widget the card's text as its accessible
    /// description. Returns the link clicked, for the owner to run. An
    /// empty card never opens.
    pub fn show(self, ui: &Ui, anchor: &Response, card: &Card) -> Option<Link> {
        let ctx = ui.ctx().clone();
        let id = self.id;
        let mut s = load(&ctx, id);
        if card.is_empty() {
            if s.open {
                close(&ctx, id);
            }
            return None;
        }
        let description = card.plain_text();
        ctx.accesskit_node_builder(anchor.id, |node| node.set_description(description));
        let now = ctx.input(|i| i.time);
        let delay = f64::from(ui.style().interaction.tooltip_delay);
        let grace = f64::from(ui.style().interaction.tooltip_grace_time.max(0.2));
        let pointer = ctx.input(|i| i.pointer.hover_pos());
        let card_rect = ctx.memory(|m| m.area_rect(id));
        let on_anchor = anchor.contains_pointer();
        let on_card = s.open
            && card_rect
                .zip(pointer)
                .is_some_and(|(r, p)| r.expand(2.0).contains(p));
        if anchor.clicked() {
            s.open = false;
            s.by_key = false;
            s.suppressed = true;
        }
        if on_anchor {
            s.left_at = None;
            if !s.open && !s.suppressed {
                let since = *s.hover_since.get_or_insert(now);
                if now - since >= delay {
                    s.open = true;
                    s.by_key = false;
                } else {
                    ctx.request_repaint_after_secs((delay - (now - since)) as f32);
                }
            }
        } else {
            s.hover_since = None;
            s.suppressed = false;
        }
        if s.open && !s.by_key {
            if on_anchor || on_card {
                s.left_at = None;
            } else {
                let left = *s.left_at.get_or_insert(now);
                if now - left >= grace {
                    s.open = false;
                    s.left_at = None;
                } else {
                    ctx.request_repaint_after_secs((grace - (now - left)) as f32);
                }
            }
        }
        if s.open && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            s.open = false;
            s.by_key = false;
            // Back to the widget it hovers.
            ctx.memory_mut(|m| m.request_focus(anchor.id));
        }
        if s.open && !on_card && !on_anchor && ctx.input(|i| i.pointer.any_pressed()) {
            s.open = false;
            s.by_key = false;
        }
        let mut clicked = None;
        if s.open {
            let screen = ctx.content_rect();
            let size = card_rect.map_or(egui::vec2(ui.spacing().tooltip_width, 80.0), |r| r.size());
            let placed = place(&Placement::new(anchor.rect, size, screen).side(self.side));
            let focus = std::mem::take(&mut s.focus);
            egui::Area::new(id)
                .order(egui::Order::Foreground)
                .fixed_pos(placed.rect.min)
                .show(&ctx, |ui| {
                    let kit = crate::kit::Kit::of(ui.ctx());
                    let shadow = ui.visuals().popup_shadow;
                    kit.popover.show(ui, crate::kit::State::REST, shadow, |ui| {
                        ui.set_max_width(ui.spacing().tooltip_width);
                        clicked = paint(ui, card, focus);
                    });
                });
        }
        if clicked.is_some() {
            s.open = false;
            s.by_key = false;
        }
        store(&ctx, id, s);
        clicked
    }
}

/// Draws a card's content; the link clicked. `focus` gives the first
/// link the keyboard focus.
pub fn paint(ui: &mut Ui, card: &Card, focus: bool) -> Option<Link> {
    let t = crate::theme::Tokens::of(ui.ctx());
    let mut clicked = None;
    let mut first_link = focus;
    for (k, b) in card.blocks.iter().enumerate() {
        match b {
            Block::Text { text } => {
                for line in lines(text) {
                    let mut job = LayoutJob::default();
                    let plain = TextFormat {
                        font_id: FontId::proportional(t.font.body),
                        color: t.text.default,
                        ..Default::default()
                    };
                    if line.bullet {
                        job.append("• ", 0.0, plain.clone());
                    }
                    for span in &line.spans {
                        let format = match span {
                            Span::Plain(_) => plain.clone(),
                            Span::Bold(_) => TextFormat {
                                color: t.text.strong,
                                ..plain.clone()
                            },
                            Span::Code(_) => TextFormat {
                                font_id: FontId::monospace(t.font.mono),
                                background: ui.visuals().code_bg_color,
                                ..plain.clone()
                            },
                        };
                        job.append(span.text(), 0.0, format);
                    }
                    job.wrap.max_width = ui.available_width();
                    ui.label(job);
                }
            }
            Block::Rows { rows } => {
                egui::Grid::new(("egui_pop_hover_card_rows", k))
                    .num_columns(2)
                    .spacing(egui::vec2(t.space.m, t.space.xs))
                    .show(ui, |ui| {
                        for r in rows {
                            ui.label(egui::RichText::new(&r.key).color(t.text.muted));
                            ui.label(&r.value);
                            ui.end_row();
                        }
                    });
            }
            Block::Progress { label, fraction } => {
                ui.add(
                    crate::progress::ProgressBar::new(*fraction)
                        .label(label.clone())
                        .line(),
                );
                let text = match fraction {
                    Some(f) => format!("{label} {}%", (f * 100.0).round()),
                    None => label.clone(),
                };
                ui.label(egui::RichText::new(text).small().color(t.text.muted));
            }
            Block::Separator => {
                ui.separator();
            }
            Block::Links { links } => {
                ui.horizontal_wrapped(|ui| {
                    for l in links {
                        let r = ui.link(&l.label);
                        crate::a11y::name(&r, WidgetType::Button, &l.label);
                        if std::mem::take(&mut first_link) {
                            r.request_focus();
                        }
                        if r.clicked() {
                            clicked = Some(l.clone());
                        }
                    }
                });
            }
        }
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_markdown_subset_gives_bold_code_and_list_items() {
        assert_eq!(
            spans("a **b** `c` d"),
            vec![
                Span::Plain("a ".into()),
                Span::Bold("b".into()),
                Span::Plain(" ".into()),
                Span::Code("c".into()),
                Span::Plain(" d".into()),
            ]
        );
        // No formatting inside code; a mark left open stays text.
        assert_eq!(spans("`**x**`"), vec![Span::Code("**x**".into())]);
        assert_eq!(spans("2 ** 3"), vec![Span::Plain("2 ** 3".into())]);
        assert_eq!(spans("****"), vec![Span::Plain("****".into())]);
        let l = lines("Title\n- one\n- **two**");
        assert!(!l[0].bullet);
        assert!(l[1].bullet && l[2].bullet);
        assert_eq!(l[2].spans, vec![Span::Bold("two".into())]);
    }

    #[test]
    fn a_card_reads_as_plain_text_without_its_links() {
        let card = Card::new()
            .text("**Vault** open\n- `raw` files")
            .row("Images", "1,204")
            .row("Stacks", "3")
            .progress("Import", Some(0.5))
            .separator()
            .link("Open", CommandId::from_static("vault.open"), Json::Null)
            .link("Close", CommandId::from_static("vault.close"), Json::Null);
        // Rows and links in a run are one block.
        assert_eq!(card.blocks.len(), 5);
        assert_eq!(card.links().count(), 2);
        assert_eq!(
            card.plain_text(),
            "Vault open\n• raw files\nImages: 1,204\nStacks: 3\nImport 50%"
        );
        let json = serde_json::to_value(&card).unwrap();
        assert_eq!(json["blocks"][0]["kind"], "text");
        assert_eq!(json["blocks"][4]["links"][0]["command"], "vault.open");
    }
}
