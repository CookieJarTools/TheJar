use serde::Deserialize;

use crate::http::Http;
use crate::paths;

/// The list in the repo, fetched on every check so new apps show up without a new release of The Jar.
pub const CATALOG_URL: &str = "https://raw.githubusercontent.com/CookieJarTools/TheJar/HEAD/apps.json";
const BUNDLED: &str = include_str!("../apps.json");

#[derive(Clone, Debug, Deserialize)]
pub struct Catalog {
    #[serde(default)]
    pub apps: Vec<AppSpec>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AppSpec {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub tagline: String,
    /// `owner/name` on GitHub.
    pub repo: String,
    /// A PNG, usually raw.githubusercontent.com/<repo>/HEAD/<path>.
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub windows: WindowsSpec,
    #[serde(default)]
    pub linux: LinuxSpec,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct WindowsSpec {
    /// Release file to pick, with `*` wildcards. Defaults to the `.exe`, preferring one named Setup.
    pub asset: Option<String>,
    /// The name the installer registers under in Apps & Features. Defaults to `name`.
    pub product_name: Option<String>,
    /// The executable inside the install folder. Defaults to `<product_name>.exe`.
    pub exe: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct LinuxSpec {
    /// Release file to pick, with `*` wildcards. Defaults to the x86_64 `.AppImage`.
    pub asset: Option<String>,
    /// The .desktop file name without the extension. It should match the app's Wayland app_id
    /// so the window gets the right icon. Defaults to `id`.
    pub desktop_name: Option<String>,
    pub categories: Option<String>,
    /// URL schemes the app handles, such as `crumbtree` for `crumbtree://` links.
    #[serde(default)]
    pub protocols: Vec<String>,
}

impl AppSpec {
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn product_name(&self) -> &str {
        self.windows.product_name.as_deref().unwrap_or(&self.name)
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn exe_name(&self) -> String {
        self.windows.exe.clone().unwrap_or_else(|| format!("{}.exe", self.product_name()))
    }

    pub fn desktop_name(&self) -> &str {
        self.linux.desktop_name.as_deref().unwrap_or(&self.id)
    }

    pub fn asset_pattern(&self) -> Option<&str> {
        if cfg!(windows) { self.windows.asset.as_deref() } else { self.linux.asset.as_deref() }
    }

    pub fn repo_url(&self) -> String {
        format!("https://github.com/{}", self.repo)
    }

    pub fn releases_url(&self) -> String {
        format!("https://github.com/{}/releases", self.repo)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Remote,
    /// The last list fetched, because this time the fetch failed.
    Saved(String),
    /// The copy built into The Jar.
    Bundled(String),
}

pub struct Loaded {
    pub apps: Vec<AppSpec>,
    pub source: Source,
}

pub fn catalog_url() -> String {
    std::env::var("THEJAR_CATALOG_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
        .unwrap_or_else(|| CATALOG_URL.to_owned())
}

fn cache_path() -> std::path::PathBuf {
    paths::cache_dir().join("apps.json")
}

/// The list fetched last time, or the built-in one, so the window can fill in before the network answers.
pub fn load_saved() -> Loaded {
    if let Some(apps) = crate::http::read_cached(&cache_path()).and_then(|text| parse(&text).ok()) {
        return Loaded { apps, source: Source::Saved(String::new()) };
    }
    Loaded { apps: parse(BUNDLED).expect("the bundled apps.json is valid"), source: Source::Bundled(String::new()) }
}

pub fn load(http: &Http) -> Loaded {
    let cache = cache_path();
    let reason = match http.get_cached(&catalog_url(), &cache, false) {
        Ok(fetched) => match parse(&fetched.body) {
            Ok(apps) => {
                let source = fetched.stale.map_or(Source::Remote, Source::Saved);
                return Loaded { apps, source };
            }
            Err(err) => format!("The app list online isn't valid: {err}"),
        },
        Err(err) if err.not_found => "The app list isn't online yet.".to_owned(),
        Err(err) => err.message,
    };
    Loaded { apps: parse(BUNDLED).expect("the bundled apps.json is valid"), source: Source::Bundled(reason) }
}

pub fn parse(text: &str) -> Result<Vec<AppSpec>, String> {
    let catalog: Catalog = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let mut seen = std::collections::HashSet::new();
    let mut apps = Vec::new();
    for app in catalog.apps {
        // Ids and desktop names end up in file paths, so anything odd is skipped, not trusted.
        if !is_safe_name(&app.id) || !is_safe_name(app.desktop_name()) || !is_repo(&app.repo) {
            continue;
        }
        if seen.insert(app.id.clone()) {
            apps.push(app);
        }
    }
    Ok(apps)
}

fn is_safe_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !name.contains("..")
}

fn is_repo(repo: &str) -> bool {
    match repo.split_once('/') {
        Some((owner, name)) => is_safe_name(owner) && is_safe_name(name),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_list_is_valid() {
        let apps = parse(BUNDLED).unwrap();
        let ids: Vec<&str> = apps.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["crumbtree", "gamebridge", "postie"]);
        assert_eq!(apps[0].exe_name(), "CrumbTree.exe");
        assert_eq!(apps[0].linux.protocols, ["crumbtree"]);
        assert_eq!(apps[1].desktop_name(), "gamebridge");
        assert_eq!(apps[2].desktop_name(), "postie");
        assert_eq!(apps[2].linux.protocols, ["mailto"]);
    }

    #[test]
    fn skips_entries_that_could_escape_their_folder() {
        let text = r#"{"apps": [
            {"id": "../evil", "name": "Evil", "repo": "a/b"},
            {"id": "ok", "name": "Fine", "repo": "a/b", "linux": {"desktop_name": "x/../../y"}},
            {"id": "noowner", "name": "No owner", "repo": "justname"},
            {"id": "good", "name": "Good", "repo": "CookieJarTools/Good", "future_field": 1},
            {"id": "good", "name": "Duplicate", "repo": "a/b"}
        ]}"#;
        let apps = parse(text).unwrap();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "Good");
    }
}
