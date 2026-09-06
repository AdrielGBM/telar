//! Fetching asset bytes over HTTP.

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
    agent: ureq::Agent,
    max_attempts: u32,
    retry_delay: Duration,
}

impl HttpTransport {
    /// A transport that fetches from `url_template`. Pair it with a [`crate::DiskCache`] unless the assets genuinely should not touch disk — without one, every miss is a round trip and a restart re-downloads the lot.
    pub fn new(url_template: impl Into<String>) -> Self {
        Self {
            url_template: url_template.into(),
            agent: agent(DEFAULT_TIMEOUT),
            max_attempts: DEFAULT_ATTEMPTS,
            retry_delay: DEFAULT_RETRY_DELAY,
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
        self.agent = agent(timeout);
        self
    }

    fn url_for(&self, id: &str) -> String {
        self.url_template.replace("{name}", id)
    }

    fn fetch(&self, id: &str) -> Result<Vec<u8>, AssetError> {
        let url = self.url_for(id);
        let mut attempts = 1;
        loop {
            let error = match self.get(&url) {
                Ok(bytes) => return Ok(bytes),
                Err(error) => error,
            };
            if attempts >= self.max_attempts {
                tracing::warn!(
                    "asset '{id}' gave up after {attempts} attempts ({error}); check the name and the endpoint"
                );
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

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_substitutes_the_whole_id() {
        let transport = HttpTransport::new("https://api.iconify.design/{name}.svg");
        assert_eq!(
            transport.url_for("mdi/home"),
            "https://api.iconify.design/mdi/home.svg"
        );
    }

    /// Zero attempts would mean never asking at all, which is not what anybody turning retries off means.
    #[test]
    fn retries_cannot_be_set_below_one_attempt() {
        let transport =
            HttpTransport::new("http://example.invalid/{name}").with_retry(0, Duration::ZERO);
        assert_eq!(transport.max_attempts, 1);
    }
}
