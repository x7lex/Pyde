//! Pyodide bridge: owns the Web Worker in `web/python-worker.js`.

use std::cell::RefCell;

use js_sys::{Array, Object, Reflect};
use wasm_bindgen::prelude::*;
use web_sys::{ErrorEvent, MessageEvent, Worker, WorkerOptions, WorkerType};

const WORKER_URL: &str = "python-worker.js";

pub enum Event {
    /// Pyodide finished loading.
    Ready,
    /// Lines of program output, as `(text, is_error)`.
    Output(Vec<(String, bool)>),
    /// The program finished.
    Done,
    /// Pyodide could not load. `restart` tries again.
    Failed(String),
}

pub struct Python {
    worker: RefCell<Worker>,
    on_message: Closure<dyn Fn(MessageEvent)>,
    on_error: Closure<dyn Fn(ErrorEvent)>,
}

impl Python {
    /// Starts loading Pyodide straight away, so the first Run is quick.
    pub fn new(on_event: impl Fn(Event) + Clone + 'static) -> Self {
        let handler = on_event.clone();
        let on_message = Closure::new(move |e: MessageEvent| handler(parse_event(&e.data())));
        let on_error = Closure::new(move |e: ErrorEvent| on_event(Event::Failed(e.message())));
        let worker = spawn(&on_message, &on_error);
        Self {
            worker: RefCell::new(worker),
            on_message,
            on_error,
        }
    }

    /// Runs `code` as the script `name`. Runs queued while loading start once
    /// Pyodide is ready.
    pub fn run(&self, name: &str, code: &str) {
        let message = Object::new();
        set(&message, "name", name);
        set(&message, "code", code);
        let _ = self.worker.borrow().post_message(&message);
    }

    /// Kills the worker, and any program running in it, then loads a new one.
    pub fn restart(&self) {
        self.worker.borrow().terminate();
        *self.worker.borrow_mut() = spawn(&self.on_message, &self.on_error);
    }
}

fn spawn(
    on_message: &Closure<dyn Fn(MessageEvent)>,
    on_error: &Closure<dyn Fn(ErrorEvent)>,
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
    let field =
        |key: &str| Reflect::get(data, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED);
    let text = |key: &str| field(key).as_string().unwrap_or_default();
    match text("type").as_str() {
        "ready" => Event::Ready,
        "output" => Event::Output(
            Array::from(&field("lines"))
                .iter()
                .map(parse_line)
                .collect(),
        ),
        "done" => Event::Done,
        _ => Event::Failed(text("message")),
    }
}

fn parse_line(line: JsValue) -> (String, bool) {
    let field =
        |key: &str| Reflect::get(&line, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED);
    (
        field("text").as_string().unwrap_or_default(),
        field("isError").is_truthy(),
    )
}

fn set(object: &Object, key: &str, value: &str) {
    let _ = Reflect::set(object, &JsValue::from_str(key), &JsValue::from_str(value));
}
