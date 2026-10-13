//! The key dispatcher: egui key events become chords, chords resolve
//! through the keymap in the focus chain's context (ADR 0007), and the
//! keys that resolve are consumed before widgets see them.
//!
//! While a text input owns the keyboard
//! ([`crate::focus::text_input_owns_keyboard`]: a text field has the
//! focus, or an open input such as a slider's typed entry claimed it),
//! Escape and Enter without Ctrl or Alt are left to the input: they
//! resolve nothing and stay in the input. A host with a dispatcher of its
//! own should ask the same before it reads those keys.
//!
//! On macOS the Command key counts as `Ctrl` in chords, so one keymap
//! serves every platform (as egui's `Modifiers::command`).

use std::collections::VecDeque;

use egui::{Context, Event, Key, Modifiers};
use egui_pop_model::ids::CommandId;
use egui_pop_model::keymap::{Chord, Keymap, Keys, Resolution, Trace};
use egui_pop_model::when::WhenContext;
use serde::Serialize;

/// The chord of a key press.
pub fn chord_of(key: Key, modifiers: Modifiers) -> Chord {
    let mut c = Chord::key(key.name());
    c.ctrl = modifiers.ctrl || modifiers.mac_cmd || modifiers.command;
    c.alt = modifiers.alt;
    c.shift = modifiers.shift;
    c
}

/// The egui key and modifiers of a chord (for tests and display).
pub fn key_of(chord: &Chord) -> Option<(Modifiers, Key)> {
    let key = Key::ALL
        .iter()
        .copied()
        .find(|k| k.name().eq_ignore_ascii_case(&chord.key))?;
    let modifiers = Modifiers {
        alt: chord.alt,
        ctrl: chord.ctrl,
        shift: chord.shift,
        mac_cmd: false,
        command: chord.ctrl,
    };
    Some((modifiers, key))
}

/// Whether a key press is left to a text input that owns the keyboard:
/// Escape and Enter, without Ctrl or Alt.
pub fn left_to_input(key: Key, modifiers: Modifiers) -> bool {
    matches!(key, Key::Escape | Key::Enter)
        && !modifiers.ctrl
        && !modifiers.alt
        && !modifiers.command
        && !modifiers.mac_cmd
}

/// A command a key asked for.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Dispatch {
    pub command: CommandId,
    pub keys: String,
    pub trace: Trace,
}

/// How many traces the dispatcher keeps.
pub const TRACES: usize = 32;

/// Turns key events into commands, keeping the chords of a shortcut in
/// progress and a trace of each resolution.
#[derive(Debug)]
pub struct KeyDispatcher {
    pending: Vec<Chord>,
    pending_since: f64,
    /// Seconds before a shortcut in progress is dropped.
    pub timeout: f64,
    traces: VecDeque<Trace>,
}

impl Default for KeyDispatcher {
    fn default() -> Self {
        Self {
            pending: Vec::new(),
            pending_since: 0.0,
            timeout: 2.0,
            traces: VecDeque::new(),
        }
    }
}

impl KeyDispatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// The chords of a shortcut in progress (`Ctrl+K` waiting for more).
    pub fn pending(&self) -> Option<Keys> {
        (!self.pending.is_empty()).then(|| Keys(self.pending.clone()))
    }

    /// The last resolutions, oldest first.
    pub fn traces(&self) -> impl Iterator<Item = &Trace> {
        self.traces.iter()
    }

    fn keep(&mut self, trace: Trace) {
        if self.traces.len() == TRACES {
            self.traces.pop_front();
        }
        self.traces.push_back(trace);
    }

    /// Reads this pass's key presses, resolves them and consumes the ones
    /// that did something (ran a command, or started or broke a
    /// shortcut). Returns the commands to run, in order.
    pub fn process(&mut self, ctx: &Context, keymap: &Keymap, cx: &WhenContext) -> Vec<Dispatch> {
        let now = ctx.input(|i| i.time);
        if !self.pending.is_empty() && now - self.pending_since > self.timeout {
            self.pending.clear();
        }
        let owned = crate::focus::text_input_owns_keyboard(ctx);
        let presses: Vec<(usize, Key, Modifiers)> = ctx.input(|i| {
            i.events
                .iter()
                .enumerate()
                .filter_map(|(n, e)| match e {
                    Event::Key {
                        key,
                        pressed: true,
                        repeat: false,
                        modifiers,
                        ..
                    } => Some((n, *key, *modifiers)),
                    _ => None,
                })
                .filter(|(_, key, modifiers)| !(owned && left_to_input(*key, *modifiers)))
                .collect()
        });
        let mut out = Vec::new();
        let mut consumed: Vec<(usize, Chord)> = Vec::new();
        for (n, key, modifiers) in presses {
            let chord = chord_of(key, modifiers);
            let mut keys = self.pending.clone();
            keys.push(chord.clone());
            let keys = Keys(keys);
            let (resolution, trace) = keymap.resolve_traced(&keys, cx);
            let had_pending = !self.pending.is_empty();
            let interesting = had_pending || !trace.candidates.is_empty();
            match resolution {
                Resolution::Run(command) => {
                    self.pending.clear();
                    consumed.push((n, chord));
                    out.push(Dispatch {
                        command,
                        keys: keys.to_string(),
                        trace: trace.clone(),
                    });
                }
                Resolution::Pending => {
                    self.pending = keys.0;
                    self.pending_since = now;
                    consumed.push((n, chord));
                }
                Resolution::Nothing => {
                    if had_pending {
                        // A broken shortcut swallows its last key.
                        consumed.push((n, chord));
                    }
                    self.pending.clear();
                }
            }
            if interesting {
                self.keep(trace);
            }
        }
        if !consumed.is_empty() {
            ctx.input_mut(|i| {
                let mut index = 0;
                let texts: Vec<String> = consumed
                    .iter()
                    .filter(|(_, c)| !c.ctrl && !c.alt && !c.meta)
                    .map(|(_, c)| c.key.clone())
                    .collect();
                i.events.retain(|e| {
                    let n = index;
                    index += 1;
                    if consumed.iter().any(|(c, _)| *c == n) {
                        return false;
                    }
                    // The text a consumed plain key would type.
                    !matches!(e, Event::Text(t) if texts.iter().any(|k| t.eq_ignore_ascii_case(k)))
                });
            });
        }
        out
    }
}
