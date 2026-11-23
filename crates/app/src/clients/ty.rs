//! The page's side of the type-checker worker built from `crates/ty-worker`.
//! Every call is a message; answers arrive later through `TyEvent` or the
//! callback passed to `hover`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use js_sys::Object;
use pyde_analysis::protocol::{CheckResult, HoverInfo, from_json};
use pyde_js::{message, spawn_worker, text, uint};
use wasm_bindgen::prelude::*;
use web_sys::{Event as DomEvent, MessageEvent, Worker};

/// Trunk's generated loader for the `pyde-ty-worker` crate.
const WORKER_URL: &str = "pyde-ty-worker_loader.js";

pub enum TyEvent {
    /// The worker has loaded. Only now can it take requests.
    Ready,
    /// A check finished. `id` is the one `check` was called with.
    Checked { id: u64, result: CheckResult },
    /// A running worker died, for example on a panic inside the checker.
    /// Nothing in flight will be answered. `restart` loads a new one.
    Crashed,
}

type HoverReply = Box<dyn FnOnce(Option<HoverInfo>)>;

pub struct TyClient {
    worker: RefCell<Worker>,
    hovers: Rc<RefCell<HashMap<u64, HoverReply>>>,
    next_hover_id: Cell<u64>,
    on_message: Closure<dyn Fn(MessageEvent)>,
    on_error: Closure<dyn Fn(DomEvent)>,
    /// A worker that dies before it is ready failed to load. Reporting that as
    /// a crash would have the page restart it forever.
    ready: Rc<Cell<bool>>,
}

impl TyClient {
    pub fn new(on_event: impl Fn(TyEvent) + Clone + 'static) -> Self {
        let hovers: Rc<RefCell<HashMap<u64, HoverReply>>> = Rc::default();
        let ready = Rc::new(Cell::new(false));

        let on_message = {
            let (hovers, ready, on_event) = (hovers.clone(), ready.clone(), on_event.clone());
            Closure::new(move |e: MessageEvent| {
                let data = e.data();
                let id = uint(&data, "id");
                let json = text(&data, "json");
                match text(&data, "type").as_str() {
                    "ready" => {
                        ready.set(true);
                        on_event(TyEvent::Ready);
                    }
                    "checked" => {
                        if let Some(result) = from_json::<CheckResult>(&json) {
                            on_event(TyEvent::Checked { id, result });
                        }
                    }
                    "hovered" => {
                        let reply = hovers.borrow_mut().remove(&id);
                        if let Some(reply) = reply {
                            reply(from_json::<Option<HoverInfo>>(&json).flatten());
                        }
                    }
                    _ => {}
                }
            })
        };
        let on_error = {
            let (hovers, ready) = (hovers.clone(), ready.clone());
            Closure::new(move |_: DomEvent| {
                if ready.replace(false) {
                    hovers.borrow_mut().clear();
                    on_event(TyEvent::Crashed);
                }
            })
        };

        Self {
            worker: RefCell::new(spawn_worker(WORKER_URL, &on_message, &on_error)),
            hovers,
            next_hover_id: Cell::new(1),
            on_message,
            on_error,
            ready,
        }
    }

    pub fn restart(&self) {
        self.worker.borrow().terminate();
        self.ready.set(false);
        self.hovers.borrow_mut().clear();
        *self.worker.borrow_mut() = spawn_worker(WORKER_URL, &self.on_message, &self.on_error);
    }

    /// Checks `code` as the file `name`. `id` is how the answer, and later
    /// hovers, refer to this version of the text.
    pub fn check(&self, id: u64, name: &str, code: &str) {
        let fields = [
            ("id", (id as f64).into()),
            ("name", name.into()),
            ("code", code.into()),
        ];
        self.post(&message("check", &fields));
    }

    /// Makes installed packages importable. `json` is a `Vec<PackageFiles>`. The
    /// next check sees them.
    pub fn add_packages(&self, json: &str) {
        self.post(&message("packages", &[("json", json.into())]));
    }

    /// Asks for hover info at a byte `offset` into the text that check `version`
    /// saw. `reply` gets `None` if there is nothing there, or if the worker has
    /// moved on to a newer version. It is dropped unanswered if the worker dies.
    pub fn hover(
        &self,
        version: u64,
        offset: usize,
        reply: impl FnOnce(Option<HoverInfo>) + 'static,
    ) {
        let id = self.next_hover_id.get();
        self.next_hover_id.set(id + 1);
        self.hovers.borrow_mut().insert(id, Box::new(reply));

        let fields = [
            ("id", (id as f64).into()),
            ("version", (version as f64).into()),
            ("offset", (offset as f64).into()),
        ];
        self.post(&message("hover", &fields));
    }

    fn post(&self, message: &Object) {
        let _ = self.worker.borrow().post_message(message);
    }
}
