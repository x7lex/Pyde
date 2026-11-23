//! The page's side of the Pyodide worker built from `crates/python-worker`.
//! Every call is a message; answers arrive later through `PythonEvent`.

use std::cell::RefCell;

use js_sys::{Array, Atomics, Int32Array, SharedArrayBuffer, Uint8Array};
use pyde_js::{field, flag, message, spawn_worker, text};
use wasm_bindgen::prelude::*;
use web_sys::{Event as DomEvent, MessageEvent, Worker};

/// Trunk's generated loader for the `pyde-python-worker` crate.
const WORKER_URL: &str = "pyde-python-worker_loader.js";
/// Size of the shared buffer that carries `input()` replies, header included.
const STDIN_BYTES: u32 = 64 * 1024;
/// Bytes at the start of that buffer for the flag and the reply length.
const STDIN_HEADER_BYTES: u32 = 8;

pub enum PythonEvent {
    /// Pyodide has loaded. Only now can it accept a run.
    Ready,
    /// Lines of program output, as `(text, is_error)`.
    Output(Vec<(String, bool)>),
    /// The program is blocked in `input()` until `answer` is called.
    Input {
        prompt: String,
    },
    Done,
    /// Packages are being loaded for type checking: their names.
    Prefetching(String),
    /// Their type information, as JSON text for the type checker. `failed` means
    /// they could not be loaded.
    Prefetched {
        json: String,
        failed: bool,
    },
    /// Pyodide could not load. `restart` tries again.
    Failed(String),
}

pub struct PythonClient {
    worker: RefCell<Worker>,
    on_message: Closure<dyn Fn(MessageEvent)>,
    on_error: Closure<dyn Fn(DomEvent)>,
    /// Shared with the worker for `input()`. Browsers only allow this on
    /// cross-origin isolated pages.
    stdin: Option<SharedArrayBuffer>,
}

impl PythonClient {
    /// Starts loading Pyodide straight away, so the first Run is quick.
    pub fn new(on_event: impl Fn(PythonEvent) + Clone + 'static) -> Self {
        let handler = on_event.clone();
        let on_message = Closure::new(move |e: MessageEvent| handler(parse_event(&e.data())));
        // A worker that fails to load fires a plain Event with no message.
        let on_error = Closure::new(move |e: DomEvent| {
            let message = field(&e, "message")
                .as_string()
                .unwrap_or_else(|| "the Python worker failed to start".to_string());
            on_event(PythonEvent::Failed(message));
        });
        let worker = spawn_worker(WORKER_URL, &on_message, &on_error);
        let isolated = flag(&js_sys::global(), "crossOriginIsolated");
        Self {
            worker: RefCell::new(worker),
            on_message,
            on_error,
            stdin: isolated.then(|| SharedArrayBuffer::new(STDIN_BYTES)),
        }
    }

    /// Kills the worker, and any program running in it, then loads a new one.
    pub fn restart(&self) {
        self.worker.borrow().terminate();
        *self.worker.borrow_mut() = spawn_worker(WORKER_URL, &self.on_message, &self.on_error);
    }

    /// Runs `code` as the script `name`. Call only after `PythonEvent::Ready`.
    pub fn run(&self, name: &str, code: &str) {
        let mut fields = vec![("name", name.into()), ("code", code.into())];
        if let Some(stdin) = &self.stdin {
            fields.push(("stdin", stdin.clone().into()));
        }
        self.post(&message("run", &fields));
    }

    /// Loads the packages `code` imports and reports their type information.
    /// Call only after `PythonEvent::Ready`.
    pub fn prefetch(&self, code: &str) {
        self.post(&message("prefetch", &[("code", code.into())]));
    }

    /// Wakes a program blocked in `input()`.
    pub fn answer(&self, text: &str) {
        let Some(stdin) = &self.stdin else { return };
        let mut length = text.len().min((STDIN_BYTES - STDIN_HEADER_BYTES) as usize);
        while !text.is_char_boundary(length) {
            length -= 1;
        }
        let bytes =
            Uint8Array::new_with_byte_offset_and_length(stdin, STDIN_HEADER_BYTES, length as u32);
        bytes.copy_from(&text.as_bytes()[..length]);
        let flags = Int32Array::new(stdin);
        flags.set_index(1, length as i32);
        let _ = Atomics::store(&flags, 0, 1);
        let _ = Atomics::notify(&flags, 0);
    }

    fn post(&self, message: &js_sys::Object) {
        let _ = self.worker.borrow().post_message(message);
    }
}

fn parse_event(data: &JsValue) -> PythonEvent {
    match text(data, "type").as_str() {
        "ready" => PythonEvent::Ready,
        "output" => PythonEvent::Output(
            Array::from(&field(data, "lines"))
                .iter()
                .map(|line| (text(&line, "text"), flag(&line, "isError")))
                .collect(),
        ),
        "input" => PythonEvent::Input {
            prompt: text(data, "prompt"),
        },
        "done" => PythonEvent::Done,
        "prefetching" => PythonEvent::Prefetching(text(data, "names")),
        "prefetched" => PythonEvent::Prefetched {
            json: text(data, "json"),
            failed: flag(data, "failed"),
        },
        _ => PythonEvent::Failed(text(data, "message")),
    }
}
