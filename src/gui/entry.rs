use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::catalog::AppSpec;
use crate::github::{Release, Target};
use crate::http::size_text;
use crate::ops::Phase;
use crate::platform::Installed;

/// One app in the library and everything known about it right now.
pub struct Entry {
    pub spec: AppSpec,
    pub installed: Option<Installed>,
    /// `None` until a check has answered for this app.
    pub latest: Option<Result<Option<Release>, String>>,
    pub icon: Option<Icon>,
    pub task: Option<Task>,
    pub error: Option<String>,
    pub notice: Option<String>,
}

pub struct Icon {
    pub uri: String,
    pub bytes: Vec<u8>,
}

pub struct Task {
    pub kind: TaskKind,
    pub phase: Phase,
    pub done: u64,
    pub total: u64,
    pub cancel: Arc<AtomicBool>,
    /// On Linux the old copy keeps running after an update, so the user is told to reopen it.
    pub was_running: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TaskKind {
    Install,
    Update,
    Uninstall,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Status {
    Checking,
    NotInstalled,
    UpToDate,
    Update,
    NoBuild,
    Unreachable,
}

impl Entry {
    pub fn new(spec: AppSpec) -> Self {
        Self { spec, installed: None, latest: None, icon: None, task: None, error: None, notice: None }
    }

    pub fn release(&self) -> Option<&Release> {
        self.latest.as_ref()?.as_ref().ok()?.as_ref()
    }

    pub fn check_error(&self) -> Option<&str> {
        self.latest.as_ref()?.as_ref().err().map(String::as_str)
    }

    pub fn status(&self) -> Status {
        let Some(latest) = &self.latest else {
            return Status::Checking;
        };
        match (&self.installed, latest) {
            (_, Err(_)) => Status::Unreachable,
            (None, Ok(None)) => Status::NoBuild,
            (None, Ok(Some(_))) => Status::NotInstalled,
            (Some(_), Ok(None)) => Status::UpToDate,
            (Some(installed), Ok(Some(release))) => {
                if installed.version.as_ref().is_none_or(|v| release.version > *v) {
                    Status::Update
                } else {
                    Status::UpToDate
                }
            }
        }
    }

    pub fn icon_uri(&self) -> Option<&str> {
        self.icon.as_ref().map(|icon| icon.uri.as_str())
    }

    pub fn installed_text(&self) -> String {
        match &self.installed {
            Some(installed) => {
                installed.version.as_ref().map_or_else(|| installed.version_text.clone(), ToString::to_string)
            }
            None => "Not installed".to_owned(),
        }
    }

    /// The short line beside the buttons, such as "0.1.0 → 0.2.0".
    pub fn status_text(&self) -> String {
        let release = self.release();
        match self.status() {
            Status::Checking => "Checking…".to_owned(),
            Status::Update => {
                format!("{} → {}", self.installed_text(), release.map(|r| r.version.to_string()).unwrap_or_default())
            }
            Status::UpToDate => self.installed_text(),
            Status::NotInstalled => {
                release.map(|r| format!("{} · {}", r.version, size_text(r.asset.size))).unwrap_or_default()
            }
            Status::NoBuild => format!("No {} build yet", Target::current().os_name()),
            Status::Unreachable => "Couldn't check".to_owned(),
        }
    }
}

impl Task {
    pub fn fraction(&self) -> Option<f32> {
        (self.phase == Phase::Downloading && self.total > 0).then(|| self.done as f32 / self.total as f32)
    }

    pub fn text(&self) -> String {
        match self.phase {
            Phase::Downloading if self.total > 0 => {
                format!("{} of {}", size_text(self.done), size_text(self.total))
            }
            Phase::Downloading => "Downloading…".to_owned(),
            Phase::Installing if self.kind == TaskKind::Update => "Updating…".to_owned(),
            Phase::Installing => "Installing…".to_owned(),
            Phase::Removing => "Uninstalling…".to_owned(),
        }
    }
}
