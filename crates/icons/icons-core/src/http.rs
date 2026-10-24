//! [`HttpProvider`]: an Iconify-compatible HTTP API as a source, for a host that resolves ids ahead of time.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use crate::{IconError, IconId, IconSet, IconSource, SetInfo, SourcedIcon};

const TIMEOUT: Duration = Duration::from_secs(20);

/// An Iconify-compatible API at `base`: the public one, or the same server self-hosted.
///
/// It asks for a set's icons with `GET {base}/{set}.json?icons=a,b`, one request per set however many of its icons are wanted, and for the set's licence with `GET {base}/collections?prefixes={set}`.
///
/// Only a 404 from `/collections`, or no entry for the set in it, leaves the licence unknown; any other failure fails the set's icons, so a bake never proceeds on a licence it could not check.
///
/// Blocking. It is meant for the baker, on the host; an application resolving ids while it runs fetches through `telar-dynamic`'s transport instead, which has a body for the browser too.
pub struct HttpProvider {
    base: String,
    agent: ureq::Agent,
    infos: Mutex<HashMap<String, Option<SetInfo>>>,
}

fn fetch_error(url: &str, message: impl ToString) -> IconError {
    IconError::Fetch {
        url: url.to_string(),
        message: message.to_string(),
    }
}

impl HttpProvider {
    /// A provider at `base`, such as `https://api.iconify.design` or a self-hosted instance. A trailing `/` is ignored.
    pub fn new(base: impl Into<String>) -> Self {
        let base = base.into();
        Self {
            base: base.trim_end_matches('/').to_string(),
            agent: ureq::Agent::config_builder()
                .timeout_global(Some(TIMEOUT))
                .build()
                .into(),
            infos: Mutex::new(HashMap::new()),
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    fn get(&self, url: &str) -> Result<Option<String>, IconError> {
        match self.agent.get(url).call() {
            Ok(mut response) => response
                .body_mut()
                .read_to_string()
                .map(Some)
                .map_err(|e| fetch_error(url, e)),
            Err(ureq::Error::StatusCode(404)) => Ok(None),
            Err(e) => Err(fetch_error(url, e)),
        }
    }

    fn info(&self, prefix: &str) -> Result<Option<SetInfo>, IconError> {
        let mut infos = self.infos.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = infos.get(prefix) {
            return Ok(cached.clone());
        }
        let url = format!("{}/collections?prefixes={prefix}", self.base);
        let info = match self.get(&url)? {
            Some(json) => {
                let mut collections: HashMap<String, SetInfo> = serde_json::from_str(&json)
                    .map_err(|e| {
                        fetch_error(&url, format!("not an Iconify collections response: {e}"))
                    })?;
                collections.remove(prefix)
            }
            None => None,
        };
        infos.insert(prefix.to_string(), info.clone());
        Ok(info)
    }

    fn set_icons(
        &self,
        prefix: &str,
        names: &[&str],
    ) -> Result<Option<(IconSet, String)>, IconError> {
        let url = format!("{}/{prefix}.json?icons={}", self.base, names.join(","));
        let Some(json) = self.get(&url)? else {
            return Ok(None);
        };
        let set = IconSet::from_json(&json)
            .map_err(|e| fetch_error(&url, format!("not an Iconify icon set: {e}")))?;
        Ok(Some((set, url)))
    }
}

impl IconSource for HttpProvider {
    fn describe(&self) -> String {
        format!("the Iconify provider at {}", self.base)
    }

    fn icon(&self, id: &IconId) -> Result<Option<SourcedIcon>, IconError> {
        self.icons(std::slice::from_ref(id))
            .pop()
            .unwrap_or(Ok(None))
    }

    fn icons(&self, ids: &[IconId]) -> Vec<Result<Option<SourcedIcon>, IconError>> {
        let mut prefixes: Vec<&str> = ids.iter().map(IconId::prefix).collect();
        prefixes.sort_unstable();
        prefixes.dedup();
        let mut answers: HashMap<&IconId, Result<Option<SourcedIcon>, IconError>> = HashMap::new();
        for prefix in prefixes {
            let wanted: Vec<&IconId> = ids.iter().filter(|id| id.prefix() == prefix).collect();
            let mut names: Vec<&str> = wanted.iter().map(|id| id.name()).collect();
            names.sort_unstable();
            names.dedup();
            let fetched = self.set_icons(prefix, &names).and_then(|found| {
                let Some((set, url)) = found else {
                    return Ok(None);
                };
                let info = match &set.info {
                    Some(info) => Some(info.clone()),
                    None => self.info(prefix)?,
                };
                Ok(Some((set, url, info)))
            });
            match fetched {
                Ok(Some((set, url, info))) => {
                    for id in wanted {
                        let icon = set.icon(id.name()).map(|icon| SourcedIcon {
                            svg: icon.to_svg(),
                            set: info.clone(),
                            own: false,
                            origin: url.clone(),
                        });
                        answers.insert(id, Ok(icon));
                    }
                }
                Ok(None) => {
                    for id in wanted {
                        answers.insert(id, Ok(None));
                    }
                }
                Err(error) => {
                    for id in wanted {
                        answers.insert(id, Err(error.clone()));
                    }
                }
            }
        }
        ids.iter()
            .map(|id| answers.get(id).cloned().unwrap_or(Ok(None)))
            .collect()
    }
}

#[cfg(test)]
#[path = "http_test.rs"]
mod tests;
