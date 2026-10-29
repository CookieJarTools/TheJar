use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::paths;

pub const USER_AGENT: &str = concat!("TheJar/", env!("CARGO_PKG_VERSION"));
pub const CANCELLED: &str = "Cancelled";

/// A file attached to a release.
#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    pub url: String,
    /// Lowercase hex, from the `sha256:` digest GitHub publishes for each upload.
    pub sha256: Option<String>,
}

#[derive(Debug)]
pub struct FetchError {
    pub not_found: bool,
    pub message: String,
}

/// A response body, plus why the saved copy was used instead when the network let us down.
pub struct Fetched {
    pub body: String,
    pub stale: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct CacheEntry {
    etag: Option<String>,
    body: String,
}

/// The body saved by the last successful `get_cached`, without touching the network.
pub fn read_cached(cache: &Path) -> Option<String> {
    let text = std::fs::read_to_string(cache).ok()?;
    serde_json::from_str::<CacheEntry>(&text).ok().map(|entry| entry.body)
}

pub struct Http {
    api: ureq::Agent,
    files: ureq::Agent,
    api_base: String,
    /// Set when THEJAR_API_BASE points somewhere else, which also allows plain-HTTP downloads.
    testing: bool,
}

impl Http {
    pub fn new() -> Self {
        let custom = std::env::var("THEJAR_API_BASE").ok().filter(|base| !base.trim().is_empty());
        Self {
            api: agent(Some(Duration::from_secs(30))),
            files: agent(None),
            testing: custom.is_some(),
            api_base: custom.unwrap_or_else(|| "https://api.github.com".to_owned()).trim_end_matches('/').to_owned(),
        }
    }

    pub fn api_url(&self, path: &str) -> String {
        format!("{}/{}", self.api_base, path.trim_start_matches('/'))
    }

    /// GETs text, keeping the last good copy on disk. GitHub doesn't count a 304 against
    /// the hourly limit, so sending the saved ETag keeps repeat checks free.
    pub fn get_cached(&self, url: &str, cache: &Path, github_api: bool) -> Result<Fetched, FetchError> {
        let saved: Option<CacheEntry> =
            std::fs::read_to_string(cache).ok().and_then(|text| serde_json::from_str(&text).ok());

        let mut request = self.api.get(url);
        if github_api {
            request =
                request.header("Accept", "application/vnd.github+json").header("X-GitHub-Api-Version", "2022-11-28");
        }
        if let Some(etag) = saved.as_ref().and_then(|s| s.etag.as_deref()) {
            request = request.header("If-None-Match", etag);
        }

        let mut response = match request.call() {
            Ok(response) => response,
            Err(err) => return fall_back(saved, false, unreachable(url, &err)),
        };
        let status = response.status().as_u16();

        if status == 304
            && let Some(saved) = saved
        {
            return Ok(Fetched { body: saved.body, stale: None });
        }
        if !(200..300).contains(&status) {
            let message = describe_status(status, response.headers());
            return fall_back(saved, status == 404, message);
        }

        let etag = response.headers().get("etag").and_then(|value| value.to_str().ok()).map(str::to_owned);
        let body = match response.body_mut().with_config().limit(16 << 20).read_to_string() {
            Ok(body) => body,
            Err(err) => return fall_back(saved, false, unreachable(url, &err)),
        };

        let entry = CacheEntry { etag, body };
        if let Ok(text) = serde_json::to_string(&entry) {
            let _ = paths::write_atomic(cache, text.as_bytes());
        }
        Ok(Fetched { body: entry.body, stale: None })
    }

    pub fn get_bytes(&self, url: &str, limit: u64) -> Result<Vec<u8>, String> {
        let mut response = self.api.get(url).call().map_err(|e| unreachable(url, &e))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(describe_status(status, response.headers()));
        }
        response.body_mut().with_config().limit(limit).read_to_vec().map_err(|e| unreachable(url, &e))
    }

    /// Streams an asset to `dest`, checking its size and checksum before it is moved into place.
    /// Nothing is left at `dest` unless the whole file arrived intact.
    pub fn download(
        &self,
        asset: &Asset,
        dest: &Path,
        progress: &mut dyn FnMut(u64, u64),
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        if !self.testing && !asset.url.starts_with("https://") {
            return Err(format!("Refusing to download {} over plain HTTP.", asset.name));
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Couldn't create {}: {e}", parent.display()))?;
        }
        let part = part_path(dest);
        let result = self.stream(asset, &part, progress, cancel);
        if result.is_err() {
            let _ = std::fs::remove_file(&part);
            return result;
        }
        std::fs::rename(&part, dest).map_err(|e| format!("Couldn't move the download into place: {e}"))
    }

    fn stream(
        &self,
        asset: &Asset,
        part: &Path,
        progress: &mut dyn FnMut(u64, u64),
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let response = self.files.get(&asset.url).call().map_err(|e| unreachable(&asset.url, &e))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(describe_status(status, response.headers()));
        }

        let mut reader = response.into_body().into_reader();
        let mut file = File::create(part).map_err(|e| format!("Couldn't write {}: {e}", part.display()))?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 256 * 1024];
        let mut received = 0u64;
        let mut last_report = Instant::now();

        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(CANCELLED.to_owned());
            }
            let read = reader.read(&mut buffer).map_err(|e| format!("The download stopped: {e}"))?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read]).map_err(|e| format!("Couldn't write {}: {e}", part.display()))?;
            hasher.update(&buffer[..read]);
            received += read as u64;
            if last_report.elapsed() > Duration::from_millis(80) {
                progress(received, asset.size);
                last_report = Instant::now();
            }
        }
        progress(received, asset.size);
        file.sync_all().map_err(|e| format!("Couldn't finish writing the download: {e}"))?;

        if asset.size > 0 && received != asset.size {
            return Err(format!("The download was cut short ({} of {}).", size_text(received), size_text(asset.size)));
        }
        let actual = hex(&hasher.finalize());
        if asset.sha256.as_deref().is_some_and(|expected| expected != actual) {
            return Err(
                "The download doesn't match the checksum GitHub published for it, so it was thrown away.".to_owned()
            );
        }
        Ok(())
    }
}

fn agent(global_timeout: Option<Duration>) -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(USER_AGENT)
        .http_status_as_error(false)
        .timeout_global(global_timeout)
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_per_read(Some(Duration::from_secs(60)))
        .tls_config(tls_config())
        .build()
        .into()
}

#[cfg(windows)]
fn tls_config() -> ureq::tls::TlsConfig {
    ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build()
}

#[cfg(not(windows))]
fn tls_config() -> ureq::tls::TlsConfig {
    ureq::tls::TlsConfig::builder().provider(ureq::tls::TlsProvider::Rustls).build()
}

fn fall_back(saved: Option<CacheEntry>, not_found: bool, message: String) -> Result<Fetched, FetchError> {
    match saved {
        Some(saved) if !not_found => Ok(Fetched { body: saved.body, stale: Some(message) }),
        _ => Err(FetchError { not_found, message }),
    }
}

fn unreachable(url: &str, err: &ureq::Error) -> String {
    let host = url.split("://").nth(1).and_then(|rest| rest.split('/').next()).unwrap_or(url);
    match err {
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed | ureq::Error::Timeout(_) | ureq::Error::Io(_) => {
            format!("Couldn't reach {host}. Check your connection.")
        }
        other => format!("Couldn't reach {host}: {other}"),
    }
}

fn describe_status(status: u16, headers: &ureq::http::HeaderMap) -> String {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let limited = matches!(status, 403 | 429) && header("x-ratelimit-remaining") == Some("0");
    if !limited {
        return match status {
            404 => "Not found on GitHub.".to_owned(),
            _ => format!("GitHub answered with HTTP {status}."),
        };
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let minutes = header("x-ratelimit-reset")
        .and_then(|reset| reset.parse::<u64>().ok())
        .map(|reset| reset.saturating_sub(now).div_ceil(60).max(1));
    match minutes {
        Some(minutes) => format!("GitHub's hourly limit on update checks was reached. It resets in {minutes} min."),
        None => "GitHub's hourly limit on update checks was reached. Try again later.".to_owned(),
    }
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.as_os_str().to_owned();
    name.push(".part");
    PathBuf::from(name)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn size_text(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let value = bytes as f64;
    if bytes == 0 {
        "0 KB".to_owned()
    } else if value >= 100.0 * MB {
        format!("{:.0} MB", value / MB)
    } else if value >= MB {
        format!("{:.1} MB", value / MB)
    } else {
        format!("{:.0} KB", (value / 1024.0).max(1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::size_text;

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(size_text(110_848_209), "106 MB");
        assert_eq!(size_text(5 * 1024 * 1024 + 300_000), "5.3 MB");
        assert_eq!(size_text(200), "1 KB");
        assert_eq!(size_text(0), "0 KB");
    }
}
