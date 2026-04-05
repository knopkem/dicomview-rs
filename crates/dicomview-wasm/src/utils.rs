//! Shared wasm utility helpers.

use wasm_bindgen::JsValue;

/// Installs the panic hook once for better browser error messages.
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

/// Builds a string-based JavaScript error.
#[must_use]
pub fn js_error(message: impl Into<String>) -> JsValue {
    #[cfg(target_arch = "wasm32")]
    {
        JsValue::from_str(&message.into())
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = message.into();
        JsValue::NULL
    }
}
