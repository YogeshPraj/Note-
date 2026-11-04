//! A web page embedded in the window that the application scripts: the preview pane and the whiteboard.
//!
//! The pane owns the "can I script it yet?" handshake: scripts sent before the page is ready are kept and run, in
//! order, when it is ([`WebPane::mark_ready`]); loading a page starts the wait again. It also remembers which
//! theme it was last told, so a caller only re-sends it when it changed. Script building and the ready-queue are in
//! [`script`] and [`queue`], which have no toolkit in them and are unit tested.
//!
//! ```ignore
//! let pane = WebPane::new(&panel);
//! pane.load_html("<h1>Hello</h1>");
//! pane.eval("document.title = 'x'"); // queued until the page is ready
//! ```

pub mod nav;
pub mod queue;
pub mod script;

use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;

use wxdragon::event::WebViewEvents;
use wxdragon::prelude::*;
use wxdragon::widgets::webview::WebView;
use wxdragon_sys as ffi;

use self::queue::ScriptQueue;

/// Size of the pieces a large library is handed over in.
const BIG_CHUNK: usize = 200_000;

#[derive(Default)]
struct State {
    queue: ScriptQueue,
    loaded: bool,
    /// The theme the page was last told (None until it has been told, or after a reload).
    dark: Option<bool>,
    /// What has been set up in the page since it loaded (libraries handed over; see [`WebPane::first_time`]).
    given: Vec<&'static str>,
    /// The application just asked for an address: the next navigation is that one.
    asked: bool,
}

impl std::fmt::Debug for WebPane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebPane").finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct WebPane {
    view: WebView,
    st: Rc<RefCell<State>>,
}

impl WebPane {
    /// A web view in `parent` that can post messages back through `window.ipc`. It goes only where the application
    /// sends it: the page runs with that bridge, so a link in a document must not take it to a site.
    pub fn new(parent: &impl WxWidget) -> WebPane {
        let view = WebView::builder(parent).build();
        view.add_script_message_handler("ipc");
        let st = Rc::new(RefCell::new(State::default()));
        let state = st.clone();
        view.on_navigating(move |e| {
            let asked = state.try_borrow_mut().is_ok_and(|mut s| std::mem::take(&mut s.asked));
            if !asked && !nav::stays_in_app(&e.get_url().unwrap_or_default()) {
                e.event.event.veto();
            }
        });
        WebPane { view, st }
    }

    pub fn is_loaded(&self) -> bool {
        self.st.borrow().loaded
    }

    pub fn is_ready(&self) -> bool {
        self.st.borrow().queue.is_ready()
    }

    fn begin_load(&self) {
        let mut s = self.st.borrow_mut();
        s.queue.reset();
        s.loaded = true;
        s.dark = None;
        s.given.clear();
    }

    /// True the first time `what` is asked for since the page was loaded: a library is handed over once per page, and
    /// again after a reload.
    pub fn first_time(&self, what: &'static str) -> bool {
        let mut s = self.st.borrow_mut();
        let first = !s.given.contains(&what);
        if first {
            s.given.push(what);
        }
        first
    }

    /// Show `html` as a new page.
    pub fn load_html(&self, html: &str) {
        self.begin_load();
        self.view.add_script_message_handler("ipc");
        self.view.set_page(html, "about:blank");
    }

    /// Show the page at `url`.
    pub fn load_url(&self, url: &str) {
        self.begin_load();
        self.st.borrow_mut().asked = true;
        self.view.load_url(url);
    }

    /// The page can be scripted now: run what was waiting, in order.
    pub fn mark_ready(&self) {
        let waiting = self.st.borrow_mut().queue.mark_ready();
        for js in waiting {
            let _ = self.view.run_script(&js);
        }
    }

    /// Run `js` in the page, or keep it until the page is ready.
    pub fn eval(&self, js: &str) {
        let now = self.st.borrow_mut().queue.submit(js.to_string());
        if let Some(js) = now {
            let _ = self.view.run_script(&js);
        }
    }

    /// Call a function of the page with string arguments, e.g. `call("window.np.render", &["markdown", text])`.
    pub fn call(&self, path: &str, args: &[&str]) {
        self.eval(&script::call(path, args));
    }

    /// Hand a large library to the page in pieces and evaluate it there.
    pub fn eval_big(&self, js: &str) {
        for statement in script::big_script(js, BIG_CHUNK) {
            self.eval(&statement);
        }
    }

    /// Call `path(prefix..., data)` in the page with a large string `data` (a picture), sent in pieces.
    pub fn call_big(&self, path: &str, prefix: &str, data: &str) {
        for statement in script::big_call(path, prefix, data, BIG_CHUNK) {
            self.eval(&statement);
        }
    }

    /// Record the theme: true when the page has to be told (it was not told this one yet).
    pub fn sync_dark(&self, dark: bool) -> bool {
        let mut s = self.st.borrow_mut();
        let changed = s.dark != Some(dark);
        s.dark = Some(dark);
        changed
    }

    /// Forget which theme the page was told, so the next [`WebPane::sync_dark`] says it must be sent again.
    pub fn forget_dark(&self) {
        self.st.borrow_mut().dark = None;
    }
}

impl Deref for WebPane {
    type Target = WebView;
    fn deref(&self) -> &WebView {
        &self.view
    }
}

impl WxWidget for WebPane {
    fn handle_ptr(&self) -> *mut ffi::wxd_Window_t {
        self.view.handle_ptr()
    }
    fn is_valid(&self) -> bool {
        self.view.is_valid()
    }
}
