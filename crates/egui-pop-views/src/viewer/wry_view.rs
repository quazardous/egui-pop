//! The viewer's web view on Linux (feature `webview`): wry over the system's
//! WebKitGTK.
//!
//! - **GTK** starts on the interface's thread the first time a web view opens:
//!   restricted to X11 when the main window is an X11 one, so the web view can
//!   be its child; GTK's own choice otherwise. Its loop is never run: each
//!   application pass turns it once ([`Embedded::pump`]), without blocking.
//! - **A child** of the main window on X11, placed over the panel: an X11 child
//!   window of the main one (made through x11rb, found again by GDK as a foreign
//!   window) holds a GTK window, as wry's `build_as_child` does with Xlib, here
//!   without `unsafe`; **its own GTK window** elsewhere. Both hold the web view
//!   through wry's `build_gtk`.
//! - The page loads with a `Referer` (`with_url_and_headers`); other pages it is
//!   asked to go to are refused and handed back ([`Embedded::take_links`]).
//! - **Its own window** cannot be made smaller than YouTube's minimum player
//!   ([`MIN_PLAYER`] each way, in GTK's pixels, which are the web view's).
//! - wry's web views are not `Send`: they stay in this thread's table, the
//!   viewer holds a handle that reaches them from this thread only.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::thread::{self, ThreadId};

use gtk::{gdk, prelude::*};
use wry::http::{HeaderMap, HeaderValue, header::REFERER};
use wry::{NewWindowResponse, WebViewBuilder, WebViewBuilderExtUnix};
use x11rb::connection::Connection as _;
use x11rb::protocol::xproto::{
    ConfigureWindowAux, ConnectionExt as _, CreateWindowAux, WindowClass,
};
use x11rb::rust_connection::RustConnection;

use super::embed::{EmbedBackend, EmbedRequest, Embedded, MIN_PLAYER, Placement};

/// The web view backend of this build: wry over WebKitGTK.
#[derive(Clone, Copy, Debug, Default)]
pub struct Wry;

/// A web view and what keeps it.
struct Held {
    /// Dropped before its window.
    view: Option<wry::WebView>,
    /// The GTK window holding it: its own, or the one over the panel.
    window: gtk::Window,
    /// The X11 child window of the main window, for a child.
    child: Option<X11Child>,
    closed: Rc<Cell<bool>>,
    links: Rc<RefCell<Vec<String>>>,
}

impl Drop for Held {
    fn drop(&mut self) {
        self.view = None;
        self.window.close();
        // GTK lets go of the child window first: destroyed under it, its next
        // request fails, and GDK ends the program on an X11 error.
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        if let Some(c) = self.child.take() {
            let _ = c.conn.destroy_window(c.window);
            let _ = c.conn.get_input_focus().map(|r| r.reply());
        }
    }
}

thread_local! {
    static HELD: RefCell<HashMap<u64, Held>> = RefCell::new(HashMap::new());
    static NEXT: Cell<u64> = const { Cell::new(1) };
}

/// An X11 child window of the main window, on a connection of its own.
struct X11Child {
    conn: RustConnection,
    window: u32,
    /// Where it was last put.
    at: (i32, i32, i32, i32),
}

fn x11_error(e: impl std::fmt::Display) -> String {
    format!("X11: {e}")
}

/// A GTK window inside an X11 child window of the main window (X11 id
/// `parent`), at `place`: what wry's `build_as_child` makes with Xlib.
fn child_window(parent: u64, place: &Placement) -> Result<(gtk::Window, X11Child), String> {
    let parent = u32::try_from(parent).map_err(|_| format!("`{parent}` is no X11 window"))?;
    let (conn, _) = x11rb::connect(None).map_err(x11_error)?;
    let window = conn.generate_id().map_err(x11_error)?;
    let at = bounds(place);
    let (x, y, w, h) = at;
    conn.create_window(
        x11rb::COPY_DEPTH_FROM_PARENT,
        window,
        parent,
        x.clamp(i16::MIN.into(), i16::MAX.into()) as i16,
        y.clamp(i16::MIN.into(), i16::MAX.into()) as i16,
        w.clamp(1, u16::MAX.into()) as u16,
        h.clamp(1, u16::MAX.into()) as u16,
        0,
        WindowClass::INPUT_OUTPUT,
        x11rb::COPY_FROM_PARENT,
        &CreateWindowAux::new(),
    )
    .map_err(x11_error)?;
    conn.map_window(window).map_err(x11_error)?;
    // A round trip: the window exists before GDK, on its own connection, asks.
    conn.get_input_focus()
        .map_err(x11_error)?
        .reply()
        .map_err(x11_error)?;
    let display = gdk::Display::default()
        .ok_or("no display for GTK")?
        .downcast::<gdkx11::X11Display>()
        .map_err(|_| "GTK is not on X11".to_string())?;
    let foreign = gdkx11::X11Window::foreign_new_for_display(
        &display,
        gdkx11::x11::xlib::Window::from(window),
    );
    let gtk_window = gtk::Window::new(gtk::WindowType::Toplevel);
    let held: gdk::Window = foreign.upcast();
    gtk_window.connect_realize(move |w| w.set_window(held.clone()));
    gtk_window.set_has_window(true);
    gtk_window.realize();
    Ok((gtk_window, X11Child { conn, window, at }))
}

/// GTK on this thread, started on first use.
fn start_gtk(x11: bool) -> Result<(), String> {
    if gtk::is_initialized_main_thread() {
        return Ok(());
    }
    if gtk::is_initialized() {
        return Err("GTK was started on another thread".into());
    }
    if x11 {
        gtk::gdk::set_allowed_backends("x11");
    }
    gtk::init().map_err(|e| format!("GTK cannot start: {e}"))
}

/// Where a child goes, in X11's pixels: never empty, as an X11 window cannot be.
fn bounds(place: &Placement) -> (i32, i32, i32, i32) {
    let r = place.rect;
    (
        r.min.x.round() as i32,
        r.min.y.round() as i32,
        (r.width().round() as i32).max(1),
        (r.height().round() as i32).max(1),
    )
}

/// Whether the web view may go there: YouTube's privacy-enhanced player only.
fn allowed(url: &str) -> bool {
    url.starts_with("https://www.youtube-nocookie.com/embed/") || url == "about:blank"
}

impl EmbedBackend for Wry {
    fn name(&self) -> &str {
        "wry"
    }

    fn open(&self, request: &EmbedRequest) -> Result<Box<dyn Embedded>, String> {
        let parent = request.place.parent_x11.filter(|&p| p != 0);
        start_gtk(parent.is_some())?;
        let mut headers = HeaderMap::new();
        let referrer = HeaderValue::from_str(&request.referrer).map_err(|e| {
            format!(
                "the referrer `{}` is no header value: {e}",
                request.referrer
            )
        })?;
        headers.insert(REFERER, referrer);
        let links = Rc::new(RefCell::new(Vec::new()));
        let (nav, popup) = (links.clone(), links.clone());
        let egui = (request.egui.clone(), request.egui.clone());
        let builder = WebViewBuilder::new()
            .with_url_and_headers(&request.url, headers)
            .with_autoplay(true)
            .with_navigation_handler(move |url| {
                if allowed(&url) {
                    return true;
                }
                nav.borrow_mut().push(url);
                egui.0.request_repaint();
                false
            })
            .with_new_window_req_handler(move |url, _| {
                popup.borrow_mut().push(url);
                egui.1.request_repaint();
                NewWindowResponse::Deny
            });
        let closed = Rc::new(Cell::new(false));
        let (window, child) = match parent {
            Some(p) => {
                let (window, child) = child_window(p, &request.place)?;
                (window, Some(child))
            }
            None => {
                let window = gtk::Window::new(gtk::WindowType::Toplevel);
                window.set_title(&request.title);
                let r = request.place.rect;
                window.set_default_size((r.width() as i32).max(480), (r.height() as i32).max(270));
                let shut = closed.clone();
                window.connect_delete_event(move |_, _| {
                    shut.set(true);
                    gtk::glib::Propagation::Proceed
                });
                (window, None)
            }
        };
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        if child.is_none() {
            // YouTube's minimum player, in GTK's pixels, which are the web
            // view's: its own window cannot be made smaller. A child is sized
            // by the page, which closes it below that.
            let min = MIN_PLAYER as i32;
            vbox.set_size_request(min, min);
        }
        window.add(&vbox);
        let view = builder
            .build_gtk(&vbox)
            .map_err(|e| format!("the web view cannot open: {e}"))?;
        window.show_all();
        if child.is_some() {
            let (_, _, w, h) = bounds(&request.place);
            window.size_allocate(&gtk::Allocation::new(0, 0, w, h));
        }
        let id = NEXT.with(|n| {
            let id = n.get();
            n.set(id + 1);
            id
        });
        HELD.with(|h| {
            h.borrow_mut().insert(
                id,
                Held {
                    view: Some(view),
                    window,
                    child,
                    closed,
                    links,
                },
            )
        });
        Ok(Box::new(Handle {
            id,
            thread: thread::current().id(),
            own_window: parent.is_none(),
        }))
    }
}

/// The viewer's hold on a web view of this thread's table.
#[derive(Debug)]
struct Handle {
    id: u64,
    thread: ThreadId,
    own_window: bool,
}

impl Handle {
    fn with<T>(&self, f: impl FnOnce(&Held) -> T) -> Result<T, String> {
        if thread::current().id() != self.thread {
            return Err("the web view is reached from another thread".into());
        }
        HELD.with(|h| h.borrow().get(&self.id).map(f))
            .ok_or_else(|| "the web view is gone".to_string())
    }
}

impl Embedded for Handle {
    fn place(&mut self, place: &Placement) -> Result<(), String> {
        if self.own_window {
            return Ok(());
        }
        if thread::current().id() != self.thread {
            return Err("the web view is reached from another thread".into());
        }
        HELD.with(|held| {
            let mut held = held.borrow_mut();
            let h = held.get_mut(&self.id).ok_or("the web view is gone")?;
            let window = h.window.clone();
            let Some(c) = h.child.as_mut() else {
                return Ok(());
            };
            let at = bounds(place);
            if c.at == at {
                return Ok(());
            }
            c.at = at;
            let (x, y, w, hh) = at;
            c.conn
                .configure_window(
                    c.window,
                    &ConfigureWindowAux::new()
                        .x(x)
                        .y(y)
                        .width(w.max(1) as u32)
                        .height(hh.max(1) as u32),
                )
                .map_err(x11_error)?;
            c.conn.flush().map_err(x11_error)?;
            window.size_allocate(&gtk::Allocation::new(0, 0, w, hh));
            Ok(())
        })
    }

    fn pump(&mut self) -> Result<(), String> {
        self.with(|_| ())?;
        // A bounded number of events per pass: the interface keeps its turn.
        for _ in 0..64 {
            if !gtk::events_pending() {
                break;
            }
            gtk::main_iteration_do(false);
        }
        Ok(())
    }

    fn own_window(&self) -> bool {
        self.own_window
    }

    fn closed(&self) -> bool {
        self.with(|h| h.closed.get()).unwrap_or(true)
    }

    fn take_links(&mut self) -> Vec<String> {
        self.with(|h| std::mem::take(&mut *h.links.borrow_mut()))
            .unwrap_or_default()
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if thread::current().id() == self.thread {
            // Out of the table before it is dropped: dropping it turns GTK's
            // loop, which may call back into the table.
            let held = HELD.with(|h| h.borrow_mut().remove(&self.id));
            drop(held);
        } else {
            log::warn!("viewer: a web view was let go from another thread; it stays open");
        }
    }
}
