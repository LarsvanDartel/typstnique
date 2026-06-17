//! WASM hydration entry point. cargo-leptos calls the exported `hydrate`.

use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
pub fn hydrate() {
    app::hydrate();
}
