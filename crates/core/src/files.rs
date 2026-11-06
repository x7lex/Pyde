//! Rust side of `files.js`: open and save one file at a time.

use js_sys::{Error, Reflect};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/src/files.js")]
extern "C" {
    #[wasm_bindgen(catch, js_name = openFile)]
    async fn open_file() -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = saveFile)]
    async fn save_file(handle: JsValue, name: String, text: String) -> Result<JsValue, JsValue>;
}

/// A browser file handle. Null when the browser can't write back to the file.
pub type Handle = JsValue;

pub struct Opened {
    pub name: String,
    pub text: String,
    pub handle: Handle,
}

pub struct Saved {
    pub name: String,
    pub handle: Handle,
}

/// Asks for a file and reads it. `Ok(None)` means the user cancelled.
pub async fn open() -> Result<Option<Opened>, String> {
    let file = open_file().await.map_err(describe)?;
    if file.is_null() {
        return Ok(None);
    }
    Ok(Some(Opened {
        name: string_field(&file, "name"),
        text: string_field(&file, "text"),
        handle: field(&file, "handle"),
    }))
}

/// Writes to `handle`, or asks where to save if it is null. `Ok(None)` means the
/// user cancelled.
pub async fn save(handle: Handle, name: String, text: String) -> Result<Option<Saved>, String> {
    let saved = save_file(handle, name, text).await.map_err(describe)?;
    if saved.is_null() {
        return Ok(None);
    }
    Ok(Some(Saved {
        name: string_field(&saved, "name"),
        handle: field(&saved, "handle"),
    }))
}

fn field(object: &JsValue, key: &str) -> JsValue {
    Reflect::get(object, &JsValue::from_str(key)).unwrap_or(JsValue::NULL)
}

fn string_field(object: &JsValue, key: &str) -> String {
    field(object, key).as_string().unwrap_or_default()
}

fn describe(error: JsValue) -> String {
    match error.dyn_into::<Error>() {
        Ok(error) => String::from(error.message()),
        Err(other) => format!("{other:?}"),
    }
}
