//! Tones: what a part says (accent, info, success, warning, danger,
//! muted, record), a second axis beside the interaction state. It is
//! egui-pop's one tone: progress, status bar items, library badges and
//! micro-indicators all take this enum.
//!
//! A tone is not a state: a danger badge is danger whether or not it is
//! under the pointer. It is compiled ahead like the states: a component's
//! tokens give, per tone, only what differs from its own box, and
//! [`install`](super::install) merges every tone with every combination
//! of states into a table read by index at draw.
//!
//! Tokens: `component.tone.<tone>.<term>` (what the tone changes in every
//! state) and `component.tone.<tone>.<state>.<term>` (what it changes in
//! one state). A box of a tone and states is merged in this order: `rest`,
//! the tone, then for each state in [`State::ORDER`] the state's patch
//! followed by the tone's own patch for that state. The tone stands in
//! for the resting look; a state still wins over it for what the state
//! gives (a `disabled` badge looks disabled), unless the tone gives that
//! state too.
//!
//! A component whose tokens give no tone has no table and pays nothing; a
//! tone a component does not give draws its own box (`Neutral`).

use serde::Serialize;

use super::{BoxPatch, BoxStyle, State, Tokens};

/// What a part says, the same names for every component. `Neutral` is the
/// component's own box (no tone); the others are read from
/// `component.tone.<name>`, their defaults aliasing the system's status
/// tokens (`accent.default`, `info`, `success`, `warning`, `danger`,
/// `text.muted`; `record` aliases `danger` too). Serialized by its token
/// name (`"danger"`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    /// The component's own box.
    #[default]
    Neutral,
    /// Running, active, chosen: the accent.
    Accent,
    Info,
    Success,
    Warning,
    Danger,
    /// Paused, skipped, secondary.
    Muted,
    /// Recording: not an error, but drawn in the danger colour unless a
    /// kit tells them apart (`component.tone.record.*`).
    Record,
}

impl Tone {
    /// The tones a component's tokens can give, with their token names,
    /// in their table's order.
    pub const NAMED: [(Tone, &'static str); 7] = [
        (Tone::Accent, "accent"),
        (Tone::Info, "info"),
        (Tone::Success, "success"),
        (Tone::Warning, "warning"),
        (Tone::Danger, "danger"),
        (Tone::Muted, "muted"),
        (Tone::Record, "record"),
    ];

    /// Its token name (`neutral` has none: it is the component's box).
    pub fn name(self) -> &'static str {
        match self {
            Tone::Neutral => "neutral",
            Tone::Accent => "accent",
            Tone::Info => "info",
            Tone::Success => "success",
            Tone::Warning => "warning",
            Tone::Danger => "danger",
            Tone::Muted => "muted",
            Tone::Record => "record",
        }
    }

    /// The tone of a token name ([`Tone::name`]), `neutral` included.
    pub fn from_name(name: &str) -> Option<Tone> {
        if name == "neutral" {
            return Some(Tone::Neutral);
        }
        Tone::NAMED
            .iter()
            .find(|(_, n)| *n == name)
            .map(|(tone, _)| *tone)
    }

    fn from_bits(bits: u8) -> Tone {
        match bits {
            1 => Tone::Accent,
            2 => Tone::Info,
            3 => Tone::Success,
            4 => Tone::Warning,
            5 => Tone::Danger,
            6 => Tone::Muted,
            7 => Tone::Record,
            _ => Tone::Neutral,
        }
    }
}

/// The state flags take the low five bits of a [`State`]; its tone the
/// three above them, so a part's tone travels with its state (in
/// [`Part::state`](super::Part)) and every handler's `states.get` honours
/// it without a new argument.
const TONE_SHIFT: u8 = 5;
const FLAGS: u8 = (1 << TONE_SHIFT) - 1;

impl State {
    /// `self` in `tone` (replacing the tone it had).
    #[must_use]
    pub fn toned(self, tone: Tone) -> State {
        State((self.0 & FLAGS) | ((tone as u8) << TONE_SHIFT))
    }

    /// Its tone (`Neutral` unless set with [`State::toned`]).
    pub fn tone(self) -> Tone {
        Tone::from_bits(self.0 >> TONE_SHIFT)
    }

    /// Its flags only, the index of its box among 32.
    pub(super) fn flags(self) -> usize {
        usize::from(self.0 & FLAGS)
    }
}

/// A toned component's boxes: per tone of [`Tone::NAMED`], every
/// combination of states.
pub(super) type Tones = [[BoxStyle; 32]; 7];

/// The tone table of a component, or none when its tokens give no tone.
pub(super) fn compile(
    t: &Tokens,
    component: &str,
    rest: &BoxStyle,
    states: &[(State, BoxPatch)],
) -> Option<Box<Tones>> {
    let tones: Vec<(BoxPatch, Vec<BoxPatch>)> = Tone::NAMED
        .iter()
        .map(|(_, name)| {
            let prefix = format!("{component}.tone.{name}");
            let own = BoxPatch::read(t, &prefix);
            let per_state = State::ORDER
                .iter()
                .map(|(_, s)| BoxPatch::read(t, &format!("{prefix}.{s}")))
                .collect();
            (own, per_state)
        })
        .collect();
    let none = BoxPatch::default();
    if tones
        .iter()
        .all(|(own, per)| *own == none && per.iter().all(|p| *p == none))
    {
        return None;
    }
    let table = std::array::from_fn(|k| {
        let (own, per_state) = &tones[k];
        std::array::from_fn(|bits| {
            let state = State(bits as u8);
            let mut b = *rest;
            own.apply(&mut b);
            for ((s, patch), toned) in states.iter().zip(per_state) {
                if state.contains(*s) {
                    patch.apply(&mut b);
                    toned.apply(&mut b);
                }
            }
            b
        })
    });
    Some(Box::new(table))
}

/// The box of a state in its tone, from a component's boxes and its tone
/// table.
pub(super) fn get<'a>(
    boxes: &'a [BoxStyle; 32],
    tones: Option<&'a Tones>,
    state: State,
) -> &'a BoxStyle {
    let i = state.flags();
    match (tones, state.0 >> TONE_SHIFT) {
        (Some(table), n @ 1..=7) => &table[usize::from(n - 1)][i],
        _ => &boxes[i],
    }
}
