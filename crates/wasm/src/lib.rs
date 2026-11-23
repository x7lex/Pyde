//! The page's entry point, which mounts the app once the WebAssembly module loads.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    pyde_app::mount();
}
