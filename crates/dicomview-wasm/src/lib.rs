//! wasm-bindgen facade for `dicomview-rs`.

#![deny(missing_docs)]
#![deny(unsafe_code)]

use wasm_bindgen::prelude::*;

mod loader;
mod utils;
mod viewer;

use dicomview_core::decode_dicom_frame;
pub use loader::WadoLoader;
pub use viewer::Viewer;

/// Initializes the panic hook used by the wasm facade.
#[wasm_bindgen]
pub fn init() {
    utils::init_panic_hook();
}

/// Decodes one single-frame DICOM Part 10 payload into signed 16-bit pixels.
#[wasm_bindgen]
pub fn decode_dicom_pixels(bytes: &[u8]) -> Result<Vec<i16>, JsValue> {
    decode_dicom_frame(bytes)
        .map(|frame| frame.pixels)
        .map_err(|error| utils::js_error(error.to_string()))
}
