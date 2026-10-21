//! The Windows layer of egui-pop's window frame.
//!
//! winit has no hook to answer the system's hit test (see the register
//! of upstream workarounds, `docs/upstream.md`), so a title bar drawn by
//! the application loses what Windows gives a native one: the Snap
//! Layouts flyout over the maximize button (Windows 11), Aero Snap and
//! shake when the caption is dragged, the resize cursors and edges. This
//! crate subclasses a window (`SetWindowSubclass`) and answers
//! `WM_NCHITTEST` from the frame's hit regions:
//!
//! - **the caption** answers `HTCAPTION`: Windows moves the window, with
//!   Aero Snap. A double click and a right click on it are handed to the
//!   application ([`NcEvent::DoubleClick`], [`NcEvent::ContextMenu`]) so
//!   that its own steps run (maximize, the window menu);
//! - **the buttons** answer `HTMINBUTTON`, `HTMAXBUTTON` and `HTCLOSE`:
//!   Windows shows Snap Layouts over maximize. Their non-client clicks
//!   never reach the application through winit, so they are swallowed
//!   (Windows would draw classic buttons) and handed over as
//!   [`NcEvent::Hover`], [`NcEvent::Press`] and [`NcEvent::Click`];
//! - **the edges** answer the `HT*` resize codes: Windows resizes;
//! - **the rest** answers `HTCLIENT`: winit and egui get the input.
//!
//! The crate knows nothing of egui (it sits under every egui-pop layer):
//! the application gives it a [`Handler`] mapping a point in physical
//! client pixels to an [`Area`] and receiving the events. egui-pop's
//! `FramePlugin` installs it on Windows when the application draws its
//! own frame. On other systems [`install`] answers
//! [`Error::Unsupported`].
//!
//! It also asks Windows 11 to round the corners of a window drawn by the
//! application ([`set_corners`], `DwmSetWindowAttribute` with
//! `DWMWA_WINDOW_CORNER_PREFERENCE`): an undecorated window gets square
//! corners otherwise. Earlier versions refuse the attribute, harmlessly
//! ([`Error::Refused`]).
//!
//! What a message does is decided by [`decide`], and what a corner
//! preference is by [`Corners::preference`] and [`corners_outcome`], plain
//! code tested on every system; the `sys` module only calls Win32.

use std::sync::Arc;

/// What a point of the window is, for the system's hit test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Area {
    /// The application's: winit and egui get the input.
    Client,
    /// The title bar: Windows moves the window.
    Caption,
    Minimize,
    Maximize,
    Close,
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// The `WM_NCHITTEST` answers (`HT*` in `winuser.h`).
pub mod ht {
    pub const CLIENT: u32 = 1;
    pub const CAPTION: u32 = 2;
    pub const MINBUTTON: u32 = 8;
    pub const MAXBUTTON: u32 = 9;
    pub const LEFT: u32 = 10;
    pub const RIGHT: u32 = 11;
    pub const TOP: u32 = 12;
    pub const TOPLEFT: u32 = 13;
    pub const TOPRIGHT: u32 = 14;
    pub const BOTTOM: u32 = 15;
    pub const BOTTOMLEFT: u32 = 16;
    pub const BOTTOMRIGHT: u32 = 17;
    pub const CLOSE: u32 = 20;
}

/// The messages [`decide`] handles (`WM_*` in `winuser.h`).
pub mod wm {
    pub const NCDESTROY: u32 = 0x0082;
    pub const NCHITTEST: u32 = 0x0084;
    pub const NCMOUSEMOVE: u32 = 0x00A0;
    pub const NCLBUTTONDOWN: u32 = 0x00A1;
    pub const NCLBUTTONUP: u32 = 0x00A2;
    pub const NCLBUTTONDBLCLK: u32 = 0x00A3;
    pub const NCRBUTTONDOWN: u32 = 0x00A4;
    pub const NCRBUTTONUP: u32 = 0x00A5;
    pub const NCMOUSELEAVE: u32 = 0x02A2;
}

impl Area {
    /// The `WM_NCHITTEST` answer.
    pub fn code(self) -> u32 {
        match self {
            Area::Client => ht::CLIENT,
            Area::Caption => ht::CAPTION,
            Area::Minimize => ht::MINBUTTON,
            Area::Maximize => ht::MAXBUTTON,
            Area::Close => ht::CLOSE,
            Area::Left => ht::LEFT,
            Area::Right => ht::RIGHT,
            Area::Top => ht::TOP,
            Area::Bottom => ht::BOTTOM,
            Area::TopLeft => ht::TOPLEFT,
            Area::TopRight => ht::TOPRIGHT,
            Area::BottomLeft => ht::BOTTOMLEFT,
            Area::BottomRight => ht::BOTTOMRIGHT,
        }
    }

    /// The area of a `WM_NCHITTEST` answer (the `wParam` of the
    /// non-client mouse messages).
    pub fn from_code(code: u32) -> Option<Area> {
        Some(match code {
            ht::CLIENT => Area::Client,
            ht::CAPTION => Area::Caption,
            ht::MINBUTTON => Area::Minimize,
            ht::MAXBUTTON => Area::Maximize,
            ht::CLOSE => Area::Close,
            ht::LEFT => Area::Left,
            ht::RIGHT => Area::Right,
            ht::TOP => Area::Top,
            ht::BOTTOM => Area::Bottom,
            ht::TOPLEFT => Area::TopLeft,
            ht::TOPRIGHT => Area::TopRight,
            ht::BOTTOMLEFT => Area::BottomLeft,
            ht::BOTTOMRIGHT => Area::BottomRight,
            _ => return None,
        })
    }

    /// Minimize, maximize or close.
    pub fn is_button(self) -> bool {
        matches!(self, Area::Minimize | Area::Maximize | Area::Close)
    }
}

/// What happened on the frame that the application must hear of (the
/// input never reaches it through winit).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NcEvent {
    /// The pointer is over a button, or left them (`None`).
    Hover(Option<Area>),
    /// A button is pressed.
    Press(Area),
    /// A button is clicked (pressed and released over it).
    Click(Area),
    /// The caption is double clicked.
    DoubleClick(Area),
    /// The caption is right clicked, at this point (physical client
    /// pixels).
    ContextMenu { x: i32, y: i32 },
}

/// The application's side.
pub trait Handler: Send + Sync + 'static {
    /// What the point `(x, y)` (physical client pixels) is.
    fn hit(&self, x: i32, y: i32) -> Area;

    /// An event on the frame. Called on the window's thread, from its
    /// window procedure: queue it and wake the interface, never block.
    fn event(&self, event: NcEvent);
}

/// What the window procedure does with a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply {
    /// Pass it on (winit's procedure, then Windows').
    Default,
    /// Answer this, without passing it on.
    Answer(isize),
}

/// What [`decide`] makes of a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decision {
    pub reply: Reply,
    /// The event for the application.
    pub event: Option<NcEvent>,
    /// Ask Windows for `WM_NCMOUSELEAVE` (the pointer entered a button).
    pub track_leave: bool,
}

impl Decision {
    fn pass() -> Decision {
        Decision {
            reply: Reply::Default,
            event: None,
            track_leave: false,
        }
    }

    fn answer(v: isize, event: Option<NcEvent>) -> Decision {
        Decision {
            reply: Reply::Answer(v),
            event,
            track_leave: false,
        }
    }
}

/// What a window remembers between messages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct State {
    /// The button under the pointer.
    pub hovered: Option<Area>,
    /// The button pressed, until released.
    pub pressed: Option<Area>,
}

/// Decides what a message does. `wparam` is the message's (the hit-test
/// code for non-client mouse messages); `point` is the message's point
/// in physical client pixels (the caller converts it from the screen),
/// `None` for messages without one.
pub fn decide(
    state: &mut State,
    msg: u32,
    wparam: usize,
    point: Option<(i32, i32)>,
    handler: &dyn Handler,
) -> Decision {
    let area = Area::from_code(wparam as u32);
    let button = area.filter(|a| a.is_button());
    match msg {
        wm::NCHITTEST => {
            let Some((x, y)) = point else {
                return Decision::pass();
            };
            Decision::answer(handler.hit(x, y).code() as isize, None)
        }
        wm::NCMOUSEMOVE => {
            if state.hovered == button {
                return match button {
                    Some(_) => Decision::answer(0, None),
                    None => Decision::pass(),
                };
            }
            state.hovered = button;
            Decision {
                reply: if button.is_some() {
                    Reply::Answer(0)
                } else {
                    Reply::Default
                },
                event: Some(NcEvent::Hover(button)),
                track_leave: button.is_some(),
            }
        }
        wm::NCMOUSELEAVE => {
            state.pressed = None;
            if state.hovered.take().is_some() {
                Decision {
                    reply: Reply::Default,
                    event: Some(NcEvent::Hover(None)),
                    track_leave: false,
                }
            } else {
                Decision::pass()
            }
        }
        wm::NCLBUTTONDOWN => match button {
            // Swallowed: Windows would track a classic button.
            Some(b) => {
                state.pressed = Some(b);
                Decision::answer(0, Some(NcEvent::Press(b)))
            }
            // The caption and the edges: Windows moves and resizes.
            None => Decision::pass(),
        },
        wm::NCLBUTTONUP => match button {
            Some(b) => {
                let clicked = state.pressed.take() == Some(b);
                Decision::answer(0, clicked.then_some(NcEvent::Click(b)))
            }
            None => {
                state.pressed = None;
                Decision::pass()
            }
        },
        wm::NCLBUTTONDBLCLK => match area {
            Some(Area::Caption) => Decision::answer(0, Some(NcEvent::DoubleClick(Area::Caption))),
            // A quick second click on a button is a press.
            Some(b) if b.is_button() => {
                state.pressed = Some(b);
                Decision::answer(0, Some(NcEvent::Press(b)))
            }
            _ => Decision::pass(),
        },
        wm::NCRBUTTONDOWN if area == Some(Area::Caption) || button.is_some() => {
            Decision::answer(0, None)
        }
        wm::NCRBUTTONUP if area == Some(Area::Caption) => {
            let (x, y) = point.unwrap_or_default();
            Decision::answer(0, Some(NcEvent::ContextMenu { x, y }))
        }
        wm::NCRBUTTONUP if button.is_some() => Decision::answer(0, None),
        _ => Decision::pass(),
    }
}

/// The corners asked of the system for a window
/// (`DWM_WINDOW_CORNER_PREFERENCE`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Corners {
    /// Whatever the system does by default (a window it decorates is
    /// rounded on Windows 11, an undecorated one is not).
    System,
    /// Never rounded.
    Square,
    /// Rounded.
    Round,
    /// Rounded with a small radius.
    Small,
}

/// The `DwmSetWindowAttribute` values (`dwmapi.h`).
pub mod dwm {
    /// `DWMWA_WINDOW_CORNER_PREFERENCE` (Windows 11, build 22000).
    pub const WINDOW_CORNER_PREFERENCE: u32 = 33;
    pub const WCP_DEFAULT: u32 = 0;
    pub const WCP_DONOTROUND: u32 = 1;
    pub const WCP_ROUND: u32 = 2;
    pub const WCP_ROUNDSMALL: u32 = 3;
}

impl Corners {
    /// The `DWMWA_WINDOW_CORNER_PREFERENCE` value.
    pub fn preference(self) -> u32 {
        match self {
            Corners::System => dwm::WCP_DEFAULT,
            Corners::Square => dwm::WCP_DONOTROUND,
            Corners::Round => dwm::WCP_ROUND,
            Corners::Small => dwm::WCP_ROUNDSMALL,
        }
    }
}

/// What `DwmSetWindowAttribute`'s `HRESULT` means: done when it is not
/// negative, else refused (`E_INVALIDARG` before Windows 11, which knows
/// no such attribute).
pub fn corners_outcome(hresult: i32) -> Result<(), Error> {
    if hresult >= 0 {
        Ok(())
    } else {
        Err(Error::Refused(hresult))
    }
}

/// What this layer cannot do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Not on Windows.
    Unsupported,
    /// The handle is null.
    NoWindow,
    /// `SetWindowSubclass` failed (another thread's window).
    Subclass,
    /// The system refused a window attribute, with its `HRESULT` (a
    /// version before Windows 11 for the corners).
    Refused(i32),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Unsupported => f.write_str("this is done on Windows only"),
            Error::NoWindow => f.write_str("the window handle is null"),
            Error::Subclass => {
                f.write_str("SetWindowSubclass failed (the window belongs to another thread)")
            }
            Error::Refused(hr) => write!(
                f,
                "the system refused it (HRESULT {:#010x}; corners need Windows 11)",
                *hr as u32
            ),
        }
    }
}

impl std::error::Error for Error {}

/// Whether this build can subclass windows.
pub const SUPPORTED: bool = cfg!(windows);

/// A subclassed window. Dropping it does not remove the subclass (the
/// window may outlive it); call [`Subclassed::remove`]. The subclass
/// removes itself when the window is destroyed.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Subclassed {
    hwnd: isize,
}

impl Subclassed {
    /// The window (an `HWND`).
    pub fn hwnd(&self) -> isize {
        self.hwnd
    }

    /// Gives the window back its own hit testing.
    pub fn remove(self) {
        #[cfg(windows)]
        sys::remove(self.hwnd);
    }
}

/// Subclasses the window `hwnd` (an `HWND`, as `raw-window-handle`'s
/// `Win32WindowHandle::hwnd` gives it) so that `handler` answers its hit
/// tests. Call it on the thread that created the window (winit's event
/// loop thread). Installing twice on a window replaces the handler.
pub fn install(hwnd: isize, handler: Arc<dyn Handler>) -> Result<Subclassed, Error> {
    if hwnd == 0 {
        return Err(Error::NoWindow);
    }
    #[cfg(windows)]
    {
        sys::install(hwnd, handler)?;
        Ok(Subclassed { hwnd })
    }
    #[cfg(not(windows))]
    {
        let _ = handler;
        Err(Error::Unsupported)
    }
}

/// Asks the system for `corners` on the window `hwnd` (an `HWND`).
/// Harmless where refused: before Windows 11 the answer is
/// [`Error::Refused`] and the window keeps its corners; elsewhere than
/// Windows, [`Error::Unsupported`].
pub fn set_corners(hwnd: isize, corners: Corners) -> Result<(), Error> {
    if hwnd == 0 {
        return Err(Error::NoWindow);
    }
    #[cfg(windows)]
    {
        corners_outcome(sys::set_corners(hwnd, corners.preference()))
    }
    #[cfg(not(windows))]
    {
        let _ = corners;
        Err(Error::Unsupported)
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod sys {
    //! The Win32 calls. Every `unsafe` block is a documented Win32 call
    //! on the window the caller passed, from its own thread.

    use std::cell::RefCell;
    use std::sync::Arc;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent,
    };
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};

    use super::{Decision, Error, Handler, Reply, State, decide, wm};

    /// The subclass's id (one per window).
    const ID: usize = 0x6567_7569; // "egui"

    struct Data {
        handler: Arc<dyn Handler>,
        state: RefCell<State>,
    }

    fn point(hwnd: HWND, lparam: LPARAM) -> (i32, i32) {
        // GET_X_LPARAM / GET_Y_LPARAM: signed, for multiple monitors.
        let x = (lparam & 0xFFFF) as u16 as i16 as i32;
        let y = ((lparam >> 16) & 0xFFFF) as u16 as i16 as i32;
        let mut p = POINT { x, y };
        // SAFETY: `hwnd` is the subclassed window, `p` a valid POINT.
        unsafe { ScreenToClient(hwnd, &mut p) };
        (p.x, p.y)
    }

    unsafe extern "system" fn proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        data: usize,
    ) -> LRESULT {
        // SAFETY: `data` is the `Box<Data>` leaked by `install`, freed
        // only by `remove` or on WM_NCDESTROY, both on this thread.
        let d = unsafe { &*(data as *const Data) };
        if msg == wm::NCDESTROY {
            remove(hwnd as isize);
            // SAFETY: passing on to the next procedure, as required.
            return unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
        }
        let with_point = matches!(
            msg,
            wm::NCHITTEST
                | wm::NCMOUSEMOVE
                | wm::NCLBUTTONDOWN
                | wm::NCLBUTTONUP
                | wm::NCLBUTTONDBLCLK
                | wm::NCRBUTTONDOWN
                | wm::NCRBUTTONUP
        );
        let p = with_point.then(|| point(hwnd, lparam));
        let Decision {
            reply,
            event,
            track_leave,
        } = {
            let Ok(mut state) = d.state.try_borrow_mut() else {
                // Reentered (a handler sent a message): pass it on.
                // SAFETY: as above.
                return unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            };
            decide(&mut state, msg, wparam, p, d.handler.as_ref())
        };
        if track_leave {
            let mut t = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE | TME_NONCLIENT,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            // SAFETY: a valid TRACKMOUSEEVENT for this window.
            unsafe { TrackMouseEvent(&mut t) };
        }
        if let Some(e) = event {
            d.handler.event(e);
        }
        match reply {
            Reply::Answer(v) => v,
            // SAFETY: passing on to the next procedure, as required.
            Reply::Default => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
        }
    }

    thread_local! {
        /// The windows subclassed on this thread, with their data.
        static INSTALLED: RefCell<Vec<(isize, usize)>> = const { RefCell::new(Vec::new()) };
    }

    pub(super) fn install(hwnd: isize, handler: Arc<dyn Handler>) -> Result<(), Error> {
        remove(hwnd);
        let data = Box::into_raw(Box::new(Data {
            handler,
            state: RefCell::new(State::default()),
        })) as usize;
        // SAFETY: `hwnd` is a window of this thread (the caller's
        // contract); `proc` matches SUBCLASSPROC; `data` lives until
        // removed.
        let ok = unsafe { SetWindowSubclass(hwnd as HWND, Some(proc), ID, data) };
        if ok == 0 {
            // SAFETY: `data` was just leaked above and never shared.
            drop(unsafe { Box::from_raw(data as *mut Data) });
            return Err(Error::Subclass);
        }
        INSTALLED.with(|i| i.borrow_mut().push((hwnd, data)));
        Ok(())
    }

    /// `DwmSetWindowAttribute(DWMWA_WINDOW_CORNER_PREFERENCE)`; its
    /// `HRESULT`.
    pub(super) fn set_corners(hwnd: isize, preference: u32) -> i32 {
        let value = preference as i32;
        // SAFETY: `hwnd` is the caller's window; `value` is a valid
        // DWM_WINDOW_CORNER_PREFERENCE that outlives the call, its size
        // given.
        unsafe {
            DwmSetWindowAttribute(
                hwnd as HWND,
                super::dwm::WINDOW_CORNER_PREFERENCE as _,
                (&value as *const i32).cast(),
                std::mem::size_of::<i32>() as u32,
            )
        }
    }

    pub(super) fn remove(hwnd: isize) {
        let data = INSTALLED.with(|i| {
            let mut i = i.borrow_mut();
            let at = i.iter().position(|(h, _)| *h == hwnd)?;
            Some(i.remove(at).1)
        });
        if let Some(data) = data {
            // SAFETY: removing our own subclass from this thread's window,
            // then freeing the data no procedure uses any more.
            unsafe {
                RemoveWindowSubclass(hwnd as HWND, Some(proc), ID);
                drop(Box::from_raw(data as *mut Data));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// A 800 × 600 window: a 32 px caption, three 46 px buttons, 6 px
    /// edges.
    #[derive(Default)]
    struct Fake(Mutex<Vec<NcEvent>>);

    impl Handler for Fake {
        fn hit(&self, x: i32, y: i32) -> Area {
            if x < 6 {
                Area::Left
            } else if y < 6 {
                Area::Top
            } else if y < 32 && x >= 800 - 46 {
                Area::Close
            } else if y < 32 && x >= 800 - 92 {
                Area::Maximize
            } else if y < 32 && x >= 800 - 138 {
                Area::Minimize
            } else if y < 32 {
                Area::Caption
            } else {
                Area::Client
            }
        }

        fn event(&self, event: NcEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    fn run(state: &mut State, f: &Fake, msg: u32, area: Area, at: (i32, i32)) -> Decision {
        let d = decide(state, msg, area.code() as usize, Some(at), f);
        if let Some(e) = d.event {
            f.event(e);
        }
        d
    }

    #[test]
    fn hit_tests_answer_the_regions() {
        let f = Fake::default();
        let mut s = State::default();
        for (at, code) in [
            ((400, 16), ht::CAPTION),
            ((800 - 60, 16), ht::MAXBUTTON),
            ((800 - 10, 16), ht::CLOSE),
            ((800 - 120, 16), ht::MINBUTTON),
            ((2, 300), ht::LEFT),
            ((400, 2), ht::TOP),
            ((400, 300), ht::CLIENT),
        ] {
            let d = decide(&mut s, wm::NCHITTEST, 0, Some(at), &f);
            assert_eq!(d.reply, Reply::Answer(code as isize), "{at:?}");
        }
        for a in [
            Area::Caption,
            Area::Maximize,
            Area::BottomRight,
            Area::Client,
        ] {
            assert_eq!(Area::from_code(a.code()), Some(a));
        }
    }

    #[test]
    fn button_clicks_are_handed_to_the_application() {
        let f = Fake::default();
        let mut s = State::default();
        let at = (800 - 60, 16);
        let d = run(&mut s, &f, wm::NCMOUSEMOVE, Area::Maximize, at);
        assert!(d.track_leave);
        assert_eq!(d.reply, Reply::Answer(0));
        // Moving within the button says nothing new.
        assert_eq!(
            run(&mut s, &f, wm::NCMOUSEMOVE, Area::Maximize, at).event,
            None
        );
        assert_eq!(
            run(&mut s, &f, wm::NCLBUTTONDOWN, Area::Maximize, at).reply,
            Reply::Answer(0)
        );
        run(&mut s, &f, wm::NCLBUTTONUP, Area::Maximize, at);
        // Pressed on close, released on maximize: no click.
        run(&mut s, &f, wm::NCLBUTTONDOWN, Area::Close, at);
        run(&mut s, &f, wm::NCLBUTTONUP, Area::Maximize, at);
        run(&mut s, &f, wm::NCMOUSELEAVE, Area::Client, at);
        assert_eq!(
            *f.0.lock().unwrap(),
            vec![
                NcEvent::Hover(Some(Area::Maximize)),
                NcEvent::Press(Area::Maximize),
                NcEvent::Click(Area::Maximize),
                NcEvent::Press(Area::Close),
                NcEvent::Hover(None),
            ]
        );
    }

    #[test]
    fn the_caption_moves_natively_and_hands_over_its_clicks() {
        let f = Fake::default();
        let mut s = State::default();
        let at = (400, 16);
        // Pressing the caption is Windows' (move, Aero Snap).
        assert_eq!(
            run(&mut s, &f, wm::NCLBUTTONDOWN, Area::Caption, at).reply,
            Reply::Default
        );
        assert_eq!(
            run(&mut s, &f, wm::NCLBUTTONDBLCLK, Area::Caption, at).event,
            Some(NcEvent::DoubleClick(Area::Caption))
        );
        assert_eq!(
            run(&mut s, &f, wm::NCRBUTTONUP, Area::Caption, at).event,
            Some(NcEvent::ContextMenu { x: 400, y: 16 })
        );
        // Edges are Windows' too.
        assert_eq!(
            run(&mut s, &f, wm::NCLBUTTONDOWN, Area::Left, (2, 300)).reply,
            Reply::Default
        );
    }

    #[test]
    fn corners_map_to_the_dwm_preferences() {
        assert_eq!(Corners::System.preference(), 0);
        assert_eq!(Corners::Square.preference(), 1);
        assert_eq!(Corners::Round.preference(), 2);
        assert_eq!(Corners::Small.preference(), 3);
        assert_eq!(dwm::WINDOW_CORNER_PREFERENCE, 33);
        assert_eq!(corners_outcome(0), Ok(()));
        // E_INVALIDARG: Windows 10 knows no corner preference.
        let e_invalidarg = 0x8007_0057_u32 as i32;
        assert_eq!(
            corners_outcome(e_invalidarg),
            Err(Error::Refused(e_invalidarg))
        );
        assert_eq!(
            Error::Refused(e_invalidarg).to_string(),
            "the system refused it (HRESULT 0x80070057; corners need Windows 11)"
        );
        assert_eq!(set_corners(0, Corners::Round), Err(Error::NoWindow));
        if !SUPPORTED {
            assert_eq!(set_corners(1, Corners::Round), Err(Error::Unsupported));
        }
    }

    #[test]
    fn installing_elsewhere_than_windows_says_so() {
        let f: Arc<dyn Handler> = Arc::new(Fake::default());
        assert_eq!(install(0, f.clone()), Err(Error::NoWindow));
        if !SUPPORTED {
            assert_eq!(install(1, f), Err(Error::Unsupported));
        }
    }
}
