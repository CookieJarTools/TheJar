use std::path::{Path, PathBuf};

#[cfg(windows)]
const FOLDER: &str = "The Jar";
#[cfg(not(windows))]
const FOLDER: &str = "the-jar";

fn base(dir: Option<PathBuf>) -> PathBuf {
    dir.unwrap_or_else(std::env::temp_dir).join(FOLDER)
}

/// Settings live here.
pub fn config_dir() -> PathBuf {
    base(dirs::config_dir())
}

/// Release lists, icons, the app list and downloads in progress. Safe to delete.
pub fn cache_dir() -> PathBuf {
    base(dirs::cache_dir())
}

/// Installed AppImages and the record of what The Jar installed.
#[cfg(not(windows))]
pub fn data_dir() -> PathBuf {
    base(dirs::data_dir())
}

#[cfg(not(windows))]
pub fn apps_dir() -> PathBuf {
    data_dir().join("apps")
}

/// Where menu entries go on Linux: `~/.local/share/applications`.
#[cfg(not(windows))]
pub fn applications_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("applications")
}

/// Shows a path with the home folder as `~`, for display only.
pub fn pretty(path: &Path) -> String {
    if !cfg!(windows)
        && let Some(home) = dirs::home_dir()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

/// Writes a file so readers never see half of it: write beside it, then rename over it.
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    std::fs::write(&temp, contents)?;
    std::fs::rename(&temp, path)
}
