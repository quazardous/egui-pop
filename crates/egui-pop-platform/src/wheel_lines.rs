//! The lines a notch of the wheel moves on the system, read in the process, to
//! seed egui-pop's wheel speed: winit gives notches, and leaves the system's lines
//! out.
//!
//! | | Read from |
//! |---|---|
//! | Windows | `HKCU\Control Panel\Desktop` `WheelScrollLines`, where the system keeps what `SPI_GETWHEELSCROLLLINES` answers, through the registry API (winreg). "One screen at a time" (`-1`) is no number of lines: 3 |
//! | Other systems | nothing: 3 |
//!
//! Give it to `AppBuilder::system_wheel_lines`, which makes it the default of the
//! wheel speed until the user chooses one:
//!
//! ```no_run
//! let app = egui_pop_api::App::builder()
//!     .system_wheel_lines(egui_pop_platform::wheel_lines::read());
//! ```

use egui_pop_api::pointer::SYSTEM_WHEEL_LINES;

/// The system's lines a notch, now; [`SYSTEM_WHEEL_LINES`] when it says none.
pub fn read() -> u32 {
    read_registry()
        .as_deref()
        .and_then(parse)
        .unwrap_or(SYSTEM_WHEEL_LINES)
}

/// The registry's value as lines: `None` for what is no number of lines, as `-1`,
/// one screen at a time.
pub fn parse(value: &str) -> Option<u32> {
    value.trim().parse().ok()
}

#[cfg(windows)]
fn read_registry() -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Control Panel\\Desktop")
        .ok()?
        .get_value::<String, _>("WheelScrollLines")
        .ok()
}

#[cfg(not(windows))]
fn read_registry() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_read_and_a_screen_is_none() {
        assert_eq!(parse("3"), Some(3));
        assert_eq!(parse(" 12 "), Some(12));
        assert_eq!(parse("-1"), None);
        assert_eq!(parse(""), None);
    }
}
