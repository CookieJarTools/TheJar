use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Most CookieJarTools releases are published as pre-releases, so this starts on.
    pub include_prereleases: bool,
    pub check_on_start: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { include_prereleases: true, check_on_start: true }
    }
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string(paths::config_dir().join("settings.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        paths::write_atomic(&paths::config_dir().join("settings.json"), text.as_bytes())
            .map_err(|e| format!("Couldn't save settings: {e}"))
    }
}
