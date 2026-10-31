//! Keys as a program in a terminal reads them (xterm's sequences), and
//! the routing that gives a focused terminal every key before the
//! application's keymap sees it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::{Context, Event, Id, Key, Modifiers, RawInput, ViewportId};
use egui_pop_core::state::Slot;
use egui_pop_core::{focus, lock};

/// The xterm modifier parameter (1 + Shift + 2 Alt + 4 Ctrl), when any.
fn modifier_param(m: Modifiers) -> Option<u8> {
    let p = 1 + u8::from(m.shift) + 2 * u8::from(m.alt) + 4 * u8::from(m.ctrl);
    (p > 1).then_some(p)
}

/// `ESC [ 1 ; m X`, `ESC O X` (application cursor) or `ESC [ X`.
fn cursor_key(letter: char, m: Modifiers, application: bool) -> Vec<u8> {
    match modifier_param(m) {
        Some(p) => format!("\x1b[1;{p}{letter}").into_bytes(),
        None if application => format!("\x1bO{letter}").into_bytes(),
        None => format!("\x1b[{letter}").into_bytes(),
    }
}

/// `ESC [ n ~`, or `ESC [ n ; m ~` with modifiers.
fn tilde_key(n: u8, m: Modifiers) -> Vec<u8> {
    match modifier_param(m) {
        Some(p) => format!("\x1b[{n};{p}~").into_bytes(),
        None => format!("\x1b[{n}~").into_bytes(),
    }
}

/// The letter of a letter key.
fn letter(key: Key) -> Option<u8> {
    let name = key.name();
    let b = name.as_bytes();
    (b.len() == 1 && b[0].is_ascii_alphabetic()).then(|| b[0].to_ascii_lowercase())
}

/// The digit of a digit key.
fn digit(key: Key) -> Option<u8> {
    let name = key.name();
    let b = name.as_bytes();
    (b.len() == 1 && b[0].is_ascii_digit()).then_some(b[0])
}

/// The bytes a key sends, or `None` for keys that type text (their
/// text event carries them) and keys a terminal ignores.
/// `application` is the program's application cursor mode.
pub(crate) fn encode(key: Key, m: Modifiers, application: bool) -> Option<Vec<u8>> {
    let alt = |mut bytes: Vec<u8>| {
        if m.alt {
            bytes.insert(0, 0x1b);
        }
        bytes
    };
    let seq = match key {
        Key::ArrowUp => cursor_key('A', m, application),
        Key::ArrowDown => cursor_key('B', m, application),
        Key::ArrowRight => cursor_key('C', m, application),
        Key::ArrowLeft => cursor_key('D', m, application),
        Key::Home => cursor_key('H', m, application),
        Key::End => cursor_key('F', m, application),
        Key::Insert => tilde_key(2, m),
        Key::Delete => tilde_key(3, m),
        Key::PageUp => tilde_key(5, m),
        Key::PageDown => tilde_key(6, m),
        Key::F1 | Key::F2 | Key::F3 | Key::F4 => {
            let c = match key {
                Key::F1 => 'P',
                Key::F2 => 'Q',
                Key::F3 => 'R',
                _ => 'S',
            };
            match modifier_param(m) {
                Some(p) => format!("\x1b[1;{p}{c}").into_bytes(),
                None => format!("\x1bO{c}").into_bytes(),
            }
        }
        Key::F5 => tilde_key(15, m),
        Key::F6 => tilde_key(17, m),
        Key::F7 => tilde_key(18, m),
        Key::F8 => tilde_key(19, m),
        Key::F9 => tilde_key(20, m),
        Key::F10 => tilde_key(21, m),
        Key::F11 => tilde_key(23, m),
        Key::F12 => tilde_key(24, m),
        Key::Enter => alt(vec![b'\r']),
        Key::Tab if m.shift => b"\x1b[Z".to_vec(),
        Key::Tab => alt(vec![b'\t']),
        Key::Backspace if m.ctrl => alt(vec![0x08]),
        Key::Backspace => alt(vec![0x7f]),
        Key::Escape => alt(vec![0x1b]),
        Key::Space if m.ctrl => alt(vec![0]),
        _ if m.ctrl => {
            let code = if let Some(l) = letter(key) {
                l - b'a' + 1
            } else {
                match key {
                    Key::OpenBracket => 0x1b,
                    Key::Backslash => 0x1c,
                    Key::CloseBracket => 0x1d,
                    Key::Num2 => 0,
                    Key::Num6 => 0x1e,
                    Key::Minus | Key::Slash => 0x1f,
                    _ => return None,
                }
            };
            alt(vec![code])
        }
        _ if m.alt => {
            // Alt as Meta: ESC, then the key's character.
            let c = match (letter(key), digit(key)) {
                (Some(l), _) if m.shift => l.to_ascii_uppercase(),
                (Some(l), _) => l,
                (None, Some(d)) => d,
                _ => return None,
            };
            vec![0x1b, c]
        }
        _ => return None,
    };
    Some(seq)
}

/// Whether a key event sent with these modifiers is the application's
/// rather than the terminal's: Ctrl+Shift chords (a terminal cannot tell
/// Ctrl+Shift+K from Ctrl+K, so nothing is lost) except Ctrl+Shift+C
/// (copy), and every Command chord on macOS.
pub(crate) fn for_application(key: Key, m: Modifiers) -> bool {
    m.mac_cmd || (m.ctrl && m.shift && key != Key::C)
}

/// Whether a key event is the chord that leaves the terminal
/// (Shift+Escape).
pub(crate) fn is_leave(key: Key, m: Modifiers) -> bool {
    key == Key::Escape && m.shift && !m.ctrl && !m.alt && !m.mac_cmd
}

/// The terminals a window drew, this pass and the previous. Kept by window:
/// each window counts its own passes.
#[derive(Debug, Default)]
struct Drawn {
    pass: u64,
    this: Vec<Id>,
    last: Vec<Id>,
}

impl Drawn {
    fn holds(&self, id: Id) -> bool {
        self.this.contains(&id) || self.last.contains(&id)
    }
}

/// The terminals drawn and the events taken for the focused one in each window.
#[derive(Debug, Default)]
struct Routing {
    installed: bool,
    drawn: HashMap<ViewportId, Drawn>,
    taken: HashMap<Id, Vec<Event>>,
    left: Vec<Id>,
    /// The terminal the leave chord takes the focus from, once its window's
    /// pass has begun.
    leaving: HashMap<ViewportId, Id>,
}

fn routing(ctx: &Context) -> Arc<Mutex<Routing>> {
    const ROUTING: Slot<Arc<Mutex<Routing>>> = Slot::new("editors.terminal.routing");
    ROUTING.update(ctx, |r| r.clone())
}

/// A terminal is drawn this pass: while it has the focus, the next
/// passes' keys are its own.
pub(crate) fn register(ctx: &Context, id: Id) {
    let r = routing(ctx);
    let mut r = lock(&r);
    let pass = ctx.cumulative_pass_nr();
    let drawn = r.drawn.entry(ctx.viewport_id()).or_default();
    if drawn.pass != pass {
        drawn.pass = pass;
        drawn.last = std::mem::take(&mut drawn.this);
    }
    if !drawn.this.contains(&id) {
        drawn.this.push(id);
    }
    if !r.installed {
        r.installed = true;
        drop(r);
        // The routing asks it what has the focus in each window.
        focus::install(ctx);
        ctx.add_plugin(KeyRouting);
    }
}

/// The events taken for a terminal this pass, and whether the leave
/// chord was pressed.
pub(crate) fn take(ctx: &Context, id: Id) -> (Vec<Event>, bool) {
    let r = routing(ctx);
    let mut r = lock(&r);
    let events = r.taken.remove(&id).unwrap_or_default();
    let before = r.left.len();
    r.left.retain(|l| *l != id);
    (events, r.left.len() != before)
}

/// Takes a focused terminal's keys out of the input before egui and the
/// application's key dispatcher read them.
#[derive(Debug)]
struct KeyRouting;

impl egui::plugin::Plugin for KeyRouting {
    fn debug_name(&self) -> &'static str {
        "egui_pop_editors::terminal"
    }

    /// upstream: the input hook runs before the window's pass has begun, so
    /// the memory is still another window's: this goes by what the focus plugin
    /// noted for the window the input is for, and the leave chord lets go of the
    /// focus when the pass has begun (docs/upstream.md, "egui: a plugin's input
    /// hook runs in another window").
    fn input_hook(&mut self, ctx: &Context, input: &mut RawInput) {
        let window = input.viewport_id;
        let Some(id) = focus::focused_in_window(ctx, window) else {
            return;
        };
        let r = routing(ctx);
        let mut r = lock(&r);
        if !r.drawn.get(&window).is_some_and(|d| d.holds(id)) {
            return;
        }
        let mut taken = Vec::new();
        let mut leave = false;
        input.events.retain(|e| match e {
            Event::Key {
                key,
                modifiers,
                pressed,
                ..
            } => {
                if is_leave(*key, *modifiers) {
                    leave |= *pressed;
                    false
                } else if for_application(*key, *modifiers) {
                    true
                } else {
                    taken.push(e.clone());
                    false
                }
            }
            Event::Text(_) | Event::Paste(_) | Event::Copy | Event::Cut | Event::Ime(_) => {
                taken.push(e.clone());
                false
            }
            _ => true,
        });
        if !taken.is_empty() {
            r.taken.entry(id).or_default().extend(taken);
        }
        if leave {
            r.left.push(id);
            r.taken.remove(&id);
            r.leaving.insert(window, id);
            drop(r);
            ctx.request_repaint_of(window);
        }
    }

    fn on_begin_pass(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx();
        let leaving = lock(&routing(ctx)).leaving.remove(&ctx.viewport_id());
        if let Some(id) = leaving {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_encode_as_xterm_does() {
        let none = Modifiers::NONE;
        assert_eq!(encode(Key::ArrowUp, none, false).unwrap(), b"\x1b[A");
        assert_eq!(encode(Key::ArrowUp, none, true).unwrap(), b"\x1bOA");
        assert_eq!(
            encode(Key::ArrowLeft, Modifiers::CTRL, false).unwrap(),
            b"\x1b[1;5D"
        );
        assert_eq!(encode(Key::Enter, none, false).unwrap(), b"\r");
        assert_eq!(encode(Key::C, Modifiers::CTRL, false).unwrap(), [3]);
        assert_eq!(encode(Key::F5, none, false).unwrap(), b"\x1b[15~");
        assert_eq!(encode(Key::F1, none, false).unwrap(), b"\x1bOP");
        assert_eq!(
            encode(Key::Tab, Modifiers::SHIFT, false).unwrap(),
            b"\x1b[Z"
        );
        assert_eq!(encode(Key::B, Modifiers::ALT, false).unwrap(), b"\x1bb");
        assert_eq!(encode(Key::Escape, none, false).unwrap(), [0x1b]);
        // Keys that type: their text event carries them.
        assert_eq!(encode(Key::A, none, false), None);
        assert_eq!(encode(Key::Space, none, false), None);
    }

    #[test]
    fn ctrl_shift_chords_go_to_the_application_except_copy() {
        let cs = Modifiers::CTRL | Modifiers::SHIFT;
        assert!(for_application(Key::K, cs));
        assert!(!for_application(Key::C, cs));
        assert!(!for_application(Key::K, Modifiers::CTRL));
        assert!(is_leave(Key::Escape, Modifiers::SHIFT));
        assert!(!is_leave(Key::Escape, Modifiers::NONE));
    }
}
