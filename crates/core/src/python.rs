//! Pyodide bridge: owns the Web Worker built from `crates/worker`.

use std::cell::RefCell;

use js_sys::{Array, Atomics, Int32Array, Object, Reflect, SharedArrayBuffer, Uint8Array};
use wasm_bindgen::prelude::*;
use web_sys::{Event as DomEvent, MessageEvent, Worker, WorkerOptions, WorkerType};

/// Trunk's generated loader for the `pyde-worker` crate.
const WORKER_URL: &str = "pyde-worker_loader.js";
const STDIN_BYTES: u32 = 64 * 1024;
const STDIN_HEADER_BYTES: u32 = 8;

pub enum Event {
    /// Pyodide finished loading. Only now can it accept a run.
    Ready,
    /// Lines of program output, as `(text, is_error)`.
    Output(Vec<(String, bool)>),
    /// The program is blocked in `input()` until `answer` is called.
    Input { prompt: String },
    /// The program finished.
    Done,
    /// Pyodide could not load. `restart` tries again.
    Failed(String),
}

pub struct Python {
    worker: RefCell<Worker>,
    on_message: Closure<dyn Fn(MessageEvent)>,
    on_error: Closure<dyn Fn(DomEvent)>,
    /// Shared with the worker for `input()`. Browsers only allow this on
    /// cross-origin isolated pages.
    stdin: Option<SharedArrayBuffer>,
}

impl Python {
    /// Starts loading Pyodide straight away, so the first Run is quick.
    pub fn new(on_event: impl Fn(Event) + Clone + 'static) -> Self {
        let handler = on_event.clone();
        let on_message = Closure::new(move |e: MessageEvent| handler(parse_event(&e.data())));
        // A worker that fails to load fires a plain Event with no message.
        let on_error = Closure::new(move |e: DomEvent| {
            let message = field(&e, "message")
                .as_string()
                .unwrap_or_else(|| "the Python worker failed to start".to_string());
            on_event(Event::Failed(message));
        });
        let worker = spawn(&on_message, &on_error);
        let isolated = field(&js_sys::global(), "crossOriginIsolated").is_truthy();
        Self {
            worker: RefCell::new(worker),
            on_message,
            on_error,
            stdin: isolated.then(|| SharedArrayBuffer::new(STDIN_BYTES)),
        }
    }

    /// Runs `code` as the script `name`. Call only after `Event::Ready`.
    pub fn run(&self, name: &str, code: &str) {
        let message = Object::new();
        set(&message, "name", &JsValue::from_str(name));
        set(&message, "code", &JsValue::from_str(code));
        if let Some(stdin) = &self.stdin {
            set(&message, "stdin", stdin);
        }
        let _ = self.worker.borrow().post_message(&message);
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

    /// Kills the worker, and any program running in it, then loads a new one.
    pub fn restart(&self) {
        self.worker.borrow().terminate();
        *self.worker.borrow_mut() = spawn(&self.on_message, &self.on_error);
    }
}

fn spawn(
    on_message: &Closure<dyn Fn(MessageEvent)>,
    on_error: &Closure<dyn Fn(DomEvent)>,
) -> Worker {
    let options = WorkerOptions::new();
    options.set_type(WorkerType::Module);
    let worker =
        Worker::new_with_options(WORKER_URL, &options).expect("browser supports module workers");
    worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    worker
}

fn parse_event(data: &JsValue) -> Event {
    let text = |key: &str| field(data, key).as_string().unwrap_or_default();
    match text("type").as_str() {
        "ready" => Event::Ready,
        "output" => Event::Output(
            Array::from(&field(data, "lines"))
                .iter()
                .map(parse_line)
                .collect(),
        ),
        "input" => Event::Input {
            prompt: text("prompt"),
        },
        "done" => Event::Done,
        _ => Event::Failed(text("message")),
    }
}

fn parse_line(line: JsValue) -> (String, bool) {
    (
        field(&line, "text").as_string().unwrap_or_default(),
        field(&line, "isError").is_truthy(),
    )
}

fn field(object: &JsValue, key: &str) -> JsValue {
    Reflect::get(object, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED)
}

fn set(object: &Object, key: &str, value: &JsValue) {
    let _ = Reflect::set(object, &JsValue::from_str(key), value);
}
