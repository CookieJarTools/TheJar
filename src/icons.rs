use std::path::PathBuf;
use std::time::Duration;

use crate::catalog::AppSpec;
use crate::http::Http;
use crate::paths;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const REFRESH_AFTER: Duration = Duration::from_secs(7 * 24 * 60 * 60);

fn cache_path(app: &AppSpec) -> PathBuf {
    paths::cache_dir().join("icons").join(format!("{}.png", app.id))
}

pub fn cached(app: &AppSpec) -> Option<Vec<u8>> {
    std::fs::read(cache_path(app)).ok().filter(|bytes| bytes.starts_with(PNG_SIGNATURE))
}

/// The saved icon, fetched again once a week in case the app's icon changed.
pub fn refresh(http: &Http, app: &AppSpec) -> Option<Vec<u8>> {
    let path = cache_path(app);
    let age =
        std::fs::metadata(&path).and_then(|meta| meta.modified()).ok().and_then(|modified| modified.elapsed().ok());
    let saved = cached(app);
    if saved.is_some() && age.is_some_and(|age| age < REFRESH_AFTER) {
        return saved;
    }
    let url = app.icon.as_deref()?;
    match http.get_bytes(url, 4 << 20) {
        Ok(bytes) if bytes.starts_with(PNG_SIGNATURE) => {
            let _ = paths::write_atomic(&path, &bytes);
            Some(bytes)
        }
        _ => saved,
    }
}
