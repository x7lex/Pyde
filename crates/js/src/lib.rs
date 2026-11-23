//! Plain-object messaging shared by the page and its two workers: messages are
//! JS objects with a string `type` and flat fields.

use js_sys::{Object, Reflect};
use wasm_bindgen::prelude::*;
use web_sys::{Event, MessageEvent, Worker, WorkerOptions, WorkerType};

/// Reads `key`, or `undefined` if it is missing.
pub fn field(object: &JsValue, key: &str) -> JsValue {
    Reflect::get(object, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED)
}

/// Reads `key` as a string, or `""` if it isn't one.
pub fn text(object: &JsValue, key: &str) -> String {
    field(object, key).as_string().unwrap_or_default()
}

/// Reads `key` as a whole number, or `0` if it isn't one.
pub fn uint(object: &JsValue, key: &str) -> u64 {
    field(object, key).as_f64().unwrap_or(0.0) as u64
}

/// Reads `key` as a boolean, treating anything falsy as `false`.
pub fn flag(object: &JsValue, key: &str) -> bool {
    field(object, key).is_truthy()
}

pub fn set(object: &Object, key: &str, value: &JsValue) {
    let _ = Reflect::set(object, &JsValue::from_str(key), value);
}

/// A message of the given `type` carrying `fields`.
pub fn message(kind: &str, fields: &[(&str, JsValue)]) -> Object {
    let message = Object::new();
    set(&message, "type", &JsValue::from_str(kind));
    for (key, value) in fields {
        set(&message, key, value);
    }
    message
}

/// Starts the module worker served at `url`, wired to the two handlers.
pub fn spawn_worker(
    url: &str,
    on_message: &Closure<dyn Fn(MessageEvent)>,
    on_error: &Closure<dyn Fn(Event)>,
) -> Worker {
    let options = WorkerOptions::new();
    options.set_type(WorkerType::Module);
    let worker = Worker::new_with_options(url, &options).expect("browser supports module workers");
    worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    worker
}
