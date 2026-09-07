//! Fetching asset bytes over HTTP, natively and in the browser.
//!
//! One feature (`http`) and two bodies: `ureq` off the web, `fetch` on it. Which client a target uses is not a choice an application should have to make, so it is not one it is offered — what differs between them is only how the request is issued and how waiting works.

use std::time::Duration;

use ui_core::{AssetError, AssetKey, AssetTransport, Reply};

/// A transient failure — the shell often starts before the network is up at login — keeps the asset on `Loading` and is retried, so it self-heals once connectivity arrives without hammering the endpoint over a genuine 404.
const DEFAULT_ATTEMPTS: u32 = 8;
const DEFAULT_RETRY_DELAY: Duration = Duration::from_secs(4);
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

/// An [`AssetTransport`] that resolves each id against a URL template.
///
/// `url_template` is the endpoint with `{name}` standing in for the id — `"https://api.iconify.design/{name}.svg"` resolves the id `"mdi/home"` to `https://api.iconify.design/mdi/home.svg`. What an id *means* stays with the application: a set-qualified icon name, a default set, a theme prefix are all decisions this cannot make for it, and all of them come out as the id it is handed.
///
/// Retrying belongs here rather than in the loader because it is a property of *this* source: a directory on disk that fails once fails identically the second time, and re-reading it is only latency. A loader that retried on the transport's behalf would be guessing which it had.
///
/// ```ignore
/// let transport = HttpTransport::new("https://api.iconify.design/{name}.svg")
///     .with_retry(3, Duration::from_secs(2));
/// let icons = AssetLoader::new(Arc::new(transport), None, Arc::new(SvgDecoder));
/// ```
pub struct HttpTransport {
    url_template: String,
    max_attempts: u32,
    retry_delay: Duration,
    timeout: Duration,
    #[cfg(not(target_arch = "wasm32"))]
    agent: ureq::Agent,
}

impl HttpTransport {
    /// A transport that fetches from `url_template`. Pair it with a [`crate::DiskCache`] unless the assets genuinely should not touch disk — without one, every miss is a round trip and a restart re-downloads the lot.
    pub fn new(url_template: impl Into<String>) -> Self {
        Self {
            url_template: url_template.into(),
            max_attempts: DEFAULT_ATTEMPTS,
            retry_delay: DEFAULT_RETRY_DELAY,
            timeout: DEFAULT_TIMEOUT,
            #[cfg(not(target_arch = "wasm32"))]
            agent: agent(DEFAULT_TIMEOUT),
        }
    }

    /// How many times a failing fetch is retried, and how long between attempts. `attempts` counts the first try, so `1` disables retrying; running out reports the last failure, which settles the asset on `Failed`.
    pub fn with_retry(mut self, attempts: u32, delay: Duration) -> Self {
        self.max_attempts = attempts.max(1);
        self.retry_delay = delay;
        self
    }

    /// How long one request may take before it counts as a failure. Defaults to 15s.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.agent = agent(timeout);
        }
        self
    }

    fn url_for(&self, id: &str) -> String {
        self.url_template.replace("{name}", id)
    }
}

/// Whether to try again, and the warning to leave behind when there is nothing left to try. Shared so both bodies count attempts and give up identically — the only thing that differs between them is how the wait is spent.
fn give_up(id: &str, attempts: u32, max_attempts: u32, error: &AssetError) -> bool {
    let exhausted = attempts >= max_attempts;
    if exhausted {
        tracing::warn!(
            "asset '{id}' gave up after {attempts} attempts ({error}); check the name and the endpoint"
        );
    }
    exhausted
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;

    impl HttpTransport {
        fn fetch(&self, id: &str) -> Result<Vec<u8>, AssetError> {
            let url = self.url_for(id);
            let mut attempts = 1;
            loop {
                let error = match self.get(&url) {
                    Ok(bytes) => return Ok(bytes),
                    Err(error) => error,
                };
                if give_up(id, attempts, self.max_attempts, &error) {
                    return Err(error);
                }
                attempts += 1;
                std::thread::sleep(self.retry_delay);
            }
        }

        fn get(&self, url: &str) -> Result<Vec<u8>, AssetError> {
            self.agent
                .get(url)
                .call()
                .map_err(|e| AssetError(format!("{url}: {e}")))?
                .body_mut()
                .read_to_vec()
                .map_err(|e| AssetError(format!("{url}: {e}")))
        }
    }

    impl AssetTransport for HttpTransport {
        /// Blocking on purpose, and only safe because of where it runs: a loader calls this from a job on `reactive_core`'s task pool, which is elastic precisely so that work waiting on I/O still makes progress. Sleeping between retries parks that one pooled thread and nothing else — no timer, no second queue, and a burst of icons still costs one connection at a time.
        fn load(&self, key: &AssetKey, reply: Reply) {
            reply(self.fetch(&key.id));
        }
    }

    pub(super) fn agent(timeout: Duration) -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .build()
            .into()
    }
}

#[cfg(not(target_arch = "wasm32"))]
use native::agent;

#[cfg(target_arch = "wasm32")]
mod web {
    use super::*;

    use wasm_bindgen::{JsCast, JsValue, closure::Closure};
    use wasm_bindgen_futures::JsFuture;

    impl AssetTransport for HttpTransport {
        /// Returns before the request does. There is no pool to park here — the loader's job runs on the page's own event loop, where blocking freezes the frame — so this hands the whole exchange to `spawn_local` and lets `reply` fire from the completion callback.
        ///
        /// **The request is not aborted if the asset stops being wanted.** `Reply` is a one-shot `FnOnce` and carries no cancellation signal, so a transport cannot learn that the loader gave up; what it can do is finish and report into an emitter that has already been dropped, which is inert. So a cancelled asset costs the rest of one download and nothing else — wrong for a page that scrolls through hundreds of them, and the reason the seam would need a cancellation hook before that becomes a real workload.
        fn load(&self, key: &AssetKey, reply: Reply) {
            let url = self.url_for(&key.id);
            let id = key.id.clone();
            let max_attempts = self.max_attempts;
            let retry_delay = self.retry_delay;
            let timeout = self.timeout;
            wasm_bindgen_futures::spawn_local(async move {
                let mut attempts = 1;
                let result = loop {
                    let error = match get(&url, timeout).await {
                        Ok(bytes) => break Ok(bytes),
                        Err(error) => error,
                    };
                    if give_up(&id, attempts, max_attempts, &error) {
                        break Err(error);
                    }
                    attempts += 1;
                    sleep(retry_delay).await;
                };
                reply(result);
            });
        }
    }

    fn window() -> Result<web_sys::Window, AssetError> {
        web_sys::window().ok_or_else(|| AssetError("no browser window to fetch from".to_string()))
    }

    /// `setTimeout` as a future. The retry delay exists so a shell that starts before the network does not hammer the endpoint, and a busy-loop here would defeat it — but there is no thread to park, so the wait has to be a timer the event loop owns.
    async fn sleep(delay: Duration) {
        let Ok(window) = window() else { return };
        let promise = js_sys::Promise::new(&mut |resolve, _reject| {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                &resolve,
                delay.as_millis().min(i32::MAX as u128) as i32,
            );
        });
        let _ = JsFuture::from(promise).await;
    }

    /// The timeout is an `AbortController` fired by a timer, because `fetch` has no timeout of its own — a request left to the browser's own limits can outlive the frame that wanted it by minutes.
    async fn get(url: &str, timeout: Duration) -> Result<Vec<u8>, AssetError> {
        let window = window()?;
        let controller =
            web_sys::AbortController::new().map_err(|e| js_error(url, "AbortController", &e))?;
        let options = web_sys::RequestInit::new();
        options.set_signal(Some(&controller.signal()));
        let request = web_sys::Request::new_with_str_and_init(url, &options)
            .map_err(|e| js_error(url, "Request", &e))?;

        let abort = Closure::once_into_js(move || controller.abort());
        let timer = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            abort.unchecked_ref(),
            timeout.as_millis().min(i32::MAX as u128) as i32,
        );
        let response = JsFuture::from(window.fetch_with_request(&request)).await;
        if let Ok(handle) = timer {
            window.clear_timeout_with_handle(handle);
        }

        let response: web_sys::Response = response
            .map_err(|e| js_error(url, "fetch", &e))?
            .dyn_into()
            .map_err(|e| js_error(url, "fetch", &e))?;
        if !response.ok() {
            return Err(AssetError(format!(
                "{url}: HTTP {} {}",
                response.status(),
                response.status_text()
            )));
        }
        let buffer = JsFuture::from(
            response
                .array_buffer()
                .map_err(|e| js_error(url, "body", &e))?,
        )
        .await
        .map_err(|e| js_error(url, "body", &e))?;
        Ok(js_sys::Uint8Array::new(&buffer).to_vec())
    }

    /// A rejected promise is whatever the page threw, and often not an `Error` at all, so it is stringified rather than downcast.
    fn js_error(url: &str, stage: &str, value: &JsValue) -> AssetError {
        AssetError(format!(
            "{url}: {stage}: {}",
            value.as_string().unwrap_or_else(|| format!("{value:?}"))
        ))
    }
}

#[cfg(test)]
#[path = "http_test.rs"]
mod tests;
