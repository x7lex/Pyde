//! The type-checker worker. Loading typeshed and checking can freeze the page,
//! so the page talks to it only by message.
//!
//! Receives: `{ type: "check", id, name, code }`
//!           `{ type: "hover", id, version, offset }`
//!           `{ type: "packages", json }`  (`json` is a `Vec<PackageFiles>`; check again after)
//! Sends:    `{ type: "ready" }`
//!           `{ type: "checked", id, json }`  (`json` is a `CheckResult`)
//!           `{ type: "hovered", id, json }`  (`json` is an `Option<HoverInfo>`)
//!
//! A check's `id` doubles as the version of the text it checked. A hover names
//! the version it was computed against, and gets `null` if the worker has since
//! moved on, because its offsets would point into different text.

pub mod ty_engine;

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use pyde_analysis::protocol::{CheckResult, PackageFiles, from_json, to_json};
use pyde_js::{message, text, uint};
use wasm_bindgen::prelude::*;
use web_sys::{DedicatedWorkerGlobalScope, MessageEvent};

use ty_engine::TyEngine;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();

    let engine = Rc::new(RefCell::new(TyEngine::new()));
    let version = Rc::new(Cell::new(0u64));

    let on_message = Closure::<dyn Fn(MessageEvent)>::new(move |e: MessageEvent| {
        let data = e.data();
        let id = uint(&data, "id");
        match text(&data, "type").as_str() {
            "check" => {
                let (name, code) = (text(&data, "name"), text(&data, "code"));
                let (diagnostics, dead_code) = engine.borrow_mut().check(&name, &code);
                version.set(id);
                let result = CheckResult {
                    diagnostics,
                    dead_code,
                };
                post("checked", id, &to_json(&result));
            }
            "packages" => {
                if let Some(packages) = from_json::<Vec<PackageFiles>>(&text(&data, "json")) {
                    engine.borrow_mut().add_packages(&packages);
                }
            }
            "hover" => {
                let hover = if uint(&data, "version") == version.get() {
                    engine.borrow().hover(uint(&data, "offset") as usize)
                } else {
                    None
                };
                post("hovered", id, &to_json(&hover));
            }
            _ => {}
        }
    });
    scope().set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();

    let _ = scope().post_message(&message("ready", &[]));
}

fn post(kind: &str, id: u64, json: &str) {
    let fields = [("id", (id as f64).into()), ("json", json.into())];
    let _ = scope().post_message(&message(kind, &fields));
}

fn scope() -> DedicatedWorkerGlobalScope {
    js_sys::global().unchecked_into()
}
