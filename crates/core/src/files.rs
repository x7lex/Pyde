//! Single-file open and save. Uses the File System Access API where the browser
//! has it (Chromium), so Save can write back to the opened file. Other browsers
//! fall back to a file input and a download.

use std::time::Duration;

use js_sys::{Array, JSON, Promise, Reflect, Uint8Array};
use leptos::prelude::{document, set_timeout, window};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Blob, BlobPropertyBag, File, FileSystemFileHandle, FileSystemWritableFileStream,
    HtmlAnchorElement, HtmlInputElement, Url,
};

const PICKER_TYPES: &str =
    r#"{"types": [{"description": "Python files", "accept": {"text/x-python": [".py"]}}]}"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = window, js_name = showOpenFilePicker)]
    async fn show_open_file_picker(options: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_namespace = window, js_name = showSaveFilePicker)]
    async fn show_save_file_picker(options: JsValue) -> Result<JsValue, JsValue>;
}

pub struct Opened {
    pub name: String,
    pub text: String,
    pub handle: Option<FileSystemFileHandle>,
}

pub struct Saved {
    pub name: String,
    pub handle: Option<FileSystemFileHandle>,
}

/// Asks for a file and reads it. `Ok(None)` means the user cancelled.
pub async fn open() -> Result<Option<Opened>, String> {
    let (file, handle) = if browser_has("showOpenFilePicker") {
        let Some(picked) = cancellable(show_open_file_picker(picker_options())).await? else {
            return Ok(None);
        };
        let handle: FileSystemFileHandle = Array::from(&picked).get(0).unchecked_into();
        let file: File = JsFuture::from(handle.get_file())
            .await
            .map_err(describe)?
            .unchecked_into();
        (file, Some(handle))
    } else {
        let Some(file) = pick_with_input().await else {
            return Ok(None);
        };
        (file, None)
    };

    let buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(describe)?;
    // Decode strictly: lossy decoding would corrupt a binary file on save.
    let text = String::from_utf8(Uint8Array::new(&buffer).to_vec())
        .map_err(|_| format!("{} is not a UTF-8 text file", file.name()))?;
    let text = text
        .strip_prefix('\u{feff}')
        .map(str::to_string)
        .unwrap_or(text);
    Ok(Some(Opened {
        name: file.name(),
        text,
        handle,
    }))
}

/// Writes to `handle`, asking where to save if there is none yet. `Ok(None)` means
/// the user cancelled.
pub async fn save(
    handle: Option<FileSystemFileHandle>,
    name: String,
    text: String,
) -> Result<Option<Saved>, String> {
    if handle.is_none() && !browser_has("showSaveFilePicker") {
        download(&name, &text)?;
        return Ok(Some(Saved { name, handle: None }));
    }
    let handle = match handle {
        Some(handle) => handle,
        None => {
            let options = picker_options();
            let _ = Reflect::set(&options, &"suggestedName".into(), &name.into());
            match cancellable(show_save_file_picker(options)).await? {
                Some(picked) => picked.unchecked_into(),
                None => return Ok(None),
            }
        }
    };
    let writable: FileSystemWritableFileStream = JsFuture::from(handle.create_writable())
        .await
        .map_err(describe)?
        .unchecked_into();
    let written = writable.write_with_str(&text).map_err(describe)?;
    JsFuture::from(written).await.map_err(describe)?;
    JsFuture::from(writable.close()).await.map_err(describe)?;
    Ok(Some(Saved {
        name: handle.name(),
        handle: Some(handle),
    }))
}

fn browser_has(api: &str) -> bool {
    Reflect::get(&window(), &JsValue::from_str(api)).is_ok_and(|value| value.is_function())
}

fn picker_options() -> JsValue {
    JSON::parse(PICKER_TYPES).expect("PICKER_TYPES is valid JSON")
}

/// A cancelled picker gives `Ok(None)` rather than an error.
async fn cancellable(
    picker: impl Future<Output = Result<JsValue, JsValue>>,
) -> Result<Option<JsValue>, String> {
    match picker.await {
        Ok(picked) => Ok(Some(picked)),
        Err(error)
            if Reflect::get(&error, &"name".into())
                .ok()
                .and_then(|n| n.as_string())
                .as_deref()
                == Some("AbortError") =>
        {
            Ok(None)
        }
        Err(error) => Err(describe(error)),
    }
}

async fn pick_with_input() -> Option<File> {
    let input: HtmlInputElement = document().create_element("input").ok()?.unchecked_into();
    input.set_type("file");
    let picked = Promise::new(&mut |resolve, _| {
        let on_cancel = resolve.clone();
        let target = input.clone();
        let on_change = Closure::once_into_js(move || {
            let file = target.files().and_then(|files| files.get(0));
            let _ = resolve.call1(&JsValue::NULL, &file.map_or(JsValue::NULL, Into::into));
        });
        let on_cancel = Closure::once_into_js(move || {
            let _ = on_cancel.call1(&JsValue::NULL, &JsValue::NULL);
        });
        input.set_onchange(Some(on_change.unchecked_ref()));
        input.set_oncancel(Some(on_cancel.unchecked_ref()));
    });
    input.click();
    JsFuture::from(picked).await.ok()?.dyn_into().ok()
}

fn download(name: &str, text: &str) -> Result<(), String> {
    let parts = Array::of1(&JsValue::from_str(text));
    let options = BlobPropertyBag::new();
    options.set_type("text/plain");
    let blob = Blob::new_with_str_sequence_and_options(&parts, &options).map_err(describe)?;
    let url = Url::create_object_url_with_blob(&blob).map_err(describe)?;
    let link: HtmlAnchorElement = document()
        .create_element("a")
        .map_err(describe)?
        .unchecked_into();
    link.set_href(&url);
    link.set_download(name);
    link.click();
    set_timeout(
        move || {
            let _ = Url::revoke_object_url(&url);
        },
        Duration::from_secs(1),
    );
    Ok(())
}

fn describe(error: JsValue) -> String {
    match error.dyn_into::<js_sys::Error>() {
        Ok(error) => String::from(error.message()),
        Err(other) => format!("{other:?}"),
    }
}
