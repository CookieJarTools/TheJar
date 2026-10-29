use serde::Deserialize;

use crate::http::{Asset, Http};
use crate::paths;
use crate::version::Version;

#[derive(Clone, Debug)]
pub struct Release {
    pub version: Version,
    /// Markdown, as written on GitHub.
    pub notes: String,
    /// `2026-10-29T18:39:00Z`
    pub published: Option<String>,
    pub prerelease: bool,
    pub page_url: String,
    pub asset: Asset,
}

#[derive(Debug, Deserialize)]
pub struct GhRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    #[serde(default)]
    size: u64,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Os {
    Windows,
    Linux,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Arch {
    X64,
    Arm64,
}

pub struct Target {
    pub os: Os,
    pub arch: Arch,
}

impl Target {
    pub fn current() -> Self {
        Self {
            os: if cfg!(windows) { Os::Windows } else { Os::Linux },
            arch: if cfg!(target_arch = "aarch64") { Arch::Arm64 } else { Arch::X64 },
        }
    }

    pub fn os_name(&self) -> &'static str {
        match self.os {
            Os::Windows => "Windows",
            Os::Linux => "Linux",
        }
    }
}

/// The newest release that has a file for this computer. GitHub's own "latest" skips
/// pre-releases and doesn't care whether a release has a build for this OS, so it isn't used.
pub fn latest(
    http: &Http,
    repo: &str,
    pattern: Option<&str>,
    include_prereleases: bool,
) -> Result<Option<Release>, String> {
    let cache = cache_path(repo);
    let url = http.api_url(&format!("repos/{repo}/releases?per_page=30"));
    let fetched = match http.get_cached(&url, &cache, true) {
        Ok(fetched) => fetched,
        Err(err) if err.not_found => return Ok(None),
        Err(err) => return Err(err.message),
    };
    let releases: Vec<GhRelease> = serde_json::from_str(&fetched.body)
        .map_err(|e| format!("GitHub sent a release list The Jar couldn't read: {e}"))?;
    Ok(pick(releases, pattern, include_prereleases, &Target::current()))
}

/// What the last check found, without touching the network.
pub fn latest_saved(repo: &str, pattern: Option<&str>, include_prereleases: bool) -> Option<Option<Release>> {
    let body = crate::http::read_cached(&cache_path(repo))?;
    let releases: Vec<GhRelease> = serde_json::from_str(&body).ok()?;
    Some(pick(releases, pattern, include_prereleases, &Target::current()))
}

fn cache_path(repo: &str) -> std::path::PathBuf {
    paths::cache_dir().join("releases").join(format!("{}.json", repo.replace('/', "__")))
}

pub fn pick(
    releases: Vec<GhRelease>,
    pattern: Option<&str>,
    include_prereleases: bool,
    target: &Target,
) -> Option<Release> {
    releases
        .into_iter()
        .filter(|r| !r.draft && (include_prereleases || !r.prerelease))
        .filter_map(|r| {
            let version = Version::parse(&r.tag_name)?;
            let asset = best_asset(&r.assets, pattern, target)?;
            Some(Release {
                notes: r.body.unwrap_or_default(),
                published: r.published_at,
                prerelease: r.prerelease || version.is_prerelease(),
                page_url: r.html_url,
                version,
                asset,
            })
        })
        .max_by(|a, b| a.version.cmp(&b.version))
}

fn best_asset(assets: &[GhAsset], pattern: Option<&str>, target: &Target) -> Option<Asset> {
    assets
        .iter()
        .filter_map(|a| asset_score(&a.name, pattern, target).map(|score| (score, a)))
        .max_by_key(|(score, _)| *score)
        .map(|(_, a)| Asset {
            name: a.name.clone(),
            size: a.size,
            url: a.browser_download_url.clone(),
            sha256: a.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")).map(str::to_ascii_lowercase),
        })
}

/// `None` if the file isn't for this computer, otherwise how good a match it is.
pub fn asset_score(name: &str, pattern: Option<&str>, target: &Target) -> Option<i32> {
    let lower = name.to_ascii_lowercase();
    let fits = match pattern {
        Some(pattern) => wildcard(&pattern.to_ascii_lowercase(), &lower),
        None => match target.os {
            Os::Windows => lower.ends_with(".exe"),
            Os::Linux => lower.ends_with(".appimage"),
        },
    };
    if !fits {
        return None;
    }

    let arm = ["arm64", "aarch64"].iter().any(|tag| lower.contains(tag));
    let mut score = 0;
    match target.arch {
        Arch::X64 if arm => return None,
        Arch::Arm64 if arm => score += 4,
        _ => {}
    }
    if target.os == Os::Windows && lower.contains("setup") {
        score += 2;
    }
    if ["x64", "x86_64", "amd64"].iter().any(|tag| lower.contains(tag)) {
        score += 1;
    }
    Some(score)
}

/// `*` matches any run of characters; everything else matches itself.
fn wildcard(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !text.starts_with(first) || !text.ends_with(last) || text.len() < first.len() + last.len() {
        return false;
    }
    let mut rest = &text[first.len()..text.len() - last.len()];
    for middle in &parts[1..parts.len() - 1] {
        match rest.find(middle) {
            Some(at) => rest = &rest[at + middle.len()..],
            None => return false,
        }
    }
    true
}

/// `2026-10-29T18:39:00Z` → `29 Oct 2026`
pub fn date_text(published: &str) -> Option<String> {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let date = published.get(..10)?;
    let mut parts = date.split('-');
    let year = parts.next()?;
    let month: usize = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    Some(format!("{day} {} {year}", MONTHS.get(month.checked_sub(1)?)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CRUMBTREE: &str = include_str!("testdata/crumbtree-releases.json");
    const GAMEBRIDGE: &str = include_str!("testdata/gamebridge-releases.json");

    fn releases(text: &str) -> Vec<GhRelease> {
        serde_json::from_str(text).unwrap()
    }

    const WINDOWS: Target = Target { os: Os::Windows, arch: Arch::X64 };
    const LINUX: Target = Target { os: Os::Linux, arch: Arch::X64 };

    #[test]
    fn picks_the_installer_for_each_os() {
        let windows = pick(releases(CRUMBTREE), None, true, &WINDOWS).unwrap();
        assert_eq!(windows.asset.name, "CrumbTree-Setup-0.2.0.exe");
        assert_eq!(windows.version.to_string(), "0.2.0");
        assert!(windows.asset.sha256.as_deref().is_some_and(|s| s.len() == 64));

        let linux = pick(releases(CRUMBTREE), None, true, &LINUX).unwrap();
        assert_eq!(linux.asset.name, "CrumbTree-0.2.0.AppImage");
        assert!(linux.asset.size > 1_000_000);
    }

    #[test]
    fn skips_releases_without_a_build_for_this_os() {
        // GameBridge 0.4.0 shipped Linux files first; Windows users stay on 0.3.1 until its .exe lands.
        let windows = pick(releases(GAMEBRIDGE), None, true, &WINDOWS).unwrap();
        assert_eq!(windows.version.to_string(), "0.3.1");
        let linux = pick(releases(GAMEBRIDGE), None, true, &LINUX).unwrap();
        assert_eq!(linux.version.to_string(), "0.4.0");
        assert!(linux.prerelease);
    }

    #[test]
    fn can_ignore_prereleases() {
        assert!(pick(releases(CRUMBTREE), None, false, &LINUX).is_none());
        let stable = pick(releases(GAMEBRIDGE), None, false, &WINDOWS).unwrap();
        assert_eq!(stable.version.to_string(), "0.3.1");
        assert!(!stable.prerelease);
    }

    #[test]
    fn asset_rules() {
        assert!(asset_score("CrumbTree-Setup-0.2.0.exe.blockmap", None, &WINDOWS).is_none());
        assert!(asset_score("App-1.0.0-arm64.AppImage", None, &LINUX).is_none());
        assert!(asset_score("crumbtree-0.2.0.pacman", None, &LINUX).is_none());
        assert!(asset_score("App-Setup-1.0.exe", None, &WINDOWS) > asset_score("App-1.0-portable.exe", None, &WINDOWS));
        let arm = Target { os: Os::Linux, arch: Arch::Arm64 };
        assert!(asset_score("App-1.0-arm64.AppImage", None, &arm) > asset_score("App-1.0.AppImage", None, &arm));
        assert!(asset_score("tool-1.0-linux.tar.gz", Some("tool-*-linux.tar.gz"), &LINUX).is_some());
        assert!(asset_score("tool-1.0.AppImage", Some("tool-*-linux.tar.gz"), &LINUX).is_none());
    }

    #[test]
    fn wildcards() {
        assert!(wildcard("a*c", "abc"));
        assert!(wildcard("a*c", "ac"));
        assert!(wildcard("*setup*.exe", "app-setup-1.exe"));
        assert!(!wildcard("a*c", "abd"));
        assert!(!wildcard("ab*ba", "aba"));
        assert!(wildcard("exact.exe", "exact.exe"));
    }

    #[test]
    fn dates() {
        assert_eq!(date_text("2026-10-29T18:39:00Z").as_deref(), Some("29 Oct 2026"));
        assert_eq!(date_text("2026-01-05").as_deref(), Some("5 Jan 2026"));
        assert_eq!(date_text("junk"), None);
    }
}
