//! A terminal: [`Terminal`] draws a [`TerminalState`] (an emulator,
//! vt100's screen and scrollback) fed by a [`TerminalSource`]: a program
//! in a pseudo-terminal (`PtySource`, feature `pty`), or a read-only
//! [`Console`] the application writes logs and agent output to.
//!
//! - ANSI colours from the theme's `terminal.*` tokens (the 256-colour
//!   cube and RGB colours as programs give them), bold, dim, italics,
//!   underline, inverse; the cursor;
//! - scrollback (the wheel, Shift+Page Up and Page Down), selection by
//!   dragging (a word on a double click, a line on a triple click),
//!   copy, links (Ctrl+click opens);
//! - the program told the terminal's size as it changes;
//! - every key to the program while focused, Escape included:
//!   Shift+Escape leaves (see [`Terminal`]);
//! - steps `terminal.spawn`, `terminal.input` and `terminal.exit`,
//!   commands for agents (`terminal.read`, `terminal.send`,
//!   `terminal.clear`...) and collectors, declared by
//!   [`TerminalPlugin`];
//! - each job's log console in the jobs centre, as a terminal
//!   ([`jobs::JobConsolesPlugin`]).

pub mod jobs;
mod keys;
pub mod links;
#[cfg(feature = "pty")]
pub mod pty;
pub mod source;
pub mod state;
pub mod steps;
pub mod tokens;
mod view;

#[cfg(feature = "pty")]
pub use pty::PtySource;
pub use source::{CONSOLE_PENDING, CommandLine, Console, Sink, Size, Spawn, TerminalSource};
pub use state::{Cell, InputKind, TerminalState};
pub use steps::{TERMINALS, TerminalPlugin, Terminals};
pub use view::{Terminal, TerminalOutput, TerminalSignal};
