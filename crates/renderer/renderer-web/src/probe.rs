//! Whether this browser can open a GPU device at all, asked before wgpu is.
//!
//! wgpu already translates "no adapter" into an error, but the translation is broken in the version this builds against: `JsOption::into_option` treats only `undefined` as absent, and `requestAdapter()` answers **`null`**. So a browser with no adapter hands wgpu an `Adapter` wrapping `null`, and the first property read on it throws a `TypeError` out of the generated glue — a stack trace about `features` where the real answer is "this browser has no WebGPU".
//!
//! Asking first is not a workaround for that alone. A browser that cannot draw is a thing an application has to be *told*, in words, whichever library noticed.

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

/// Why a device could not be opened.
pub enum NoGpu {
    /// `navigator.gpu` is absent: the browser does not implement WebGPU, or the page is not a secure context (WebGPU is unavailable over plain `http` other than to `localhost`).
    Unsupported,
    /// WebGPU is there, but it offered no adapter — no compatible GPU, or one the browser has blocklisted.
    NoAdapter,
}

impl NoGpu {
    pub fn message(&self) -> &'static str {
        match self {
            NoGpu::Unsupported => {
                "this browser has no WebGPU (navigator.gpu is absent). Chrome and Edge have had it since \
                 113, Safari since 26 and Firefox since 141 — and it is only offered to a secure context, \
                 so an app served over plain http from anywhere but localhost will not see it either."
            }
            NoGpu::NoAdapter => {
                "this browser has WebGPU but offered no adapter. On Linux that is usually Vulkan being off: \
                 Chrome draws WebGPU through it, and `chrome://gpu` saying \"WebGPU: Hardware accelerated\" \
                 only reports the feature flag, not that an adapter exists. Turn it on at \
                 `chrome://flags/#enable-vulkan`, or start the browser with `--enable-features=Vulkan`. \
                 Otherwise there is no compatible GPU, or the browser has blocked the one there is."
            }
        }
    }
}

/// Asks the browser for an adapter, and reports whether one came back.
///
/// Goes through `Reflect` rather than `web-sys`'s WebGPU bindings, which are behind an unstable-API cfg: the question is two property reads and a call, and answering it should not make the whole crate opt into an unstable surface.
pub async fn webgpu_available() -> Result<(), NoGpu> {
    let navigator = crate::dom_window().navigator();
    let gpu = js_sys::Reflect::get(&navigator, &JsValue::from_str("gpu"))
        .ok()
        .filter(|gpu| !gpu.is_undefined() && !gpu.is_null())
        .ok_or(NoGpu::Unsupported)?;

    let request = js_sys::Reflect::get(&gpu, &JsValue::from_str("requestAdapter"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .ok_or(NoGpu::Unsupported)?;
    let promise = request
        .call0(&gpu)
        .ok()
        .and_then(|p| p.dyn_into::<js_sys::Promise>().ok())
        .ok_or(NoGpu::Unsupported)?;

    let adapter = JsFuture::from(promise)
        .await
        .map_err(|_| NoGpu::NoAdapter)?;
    if adapter.is_null() || adapter.is_undefined() {
        return Err(NoGpu::NoAdapter);
    }
    Ok(())
}
