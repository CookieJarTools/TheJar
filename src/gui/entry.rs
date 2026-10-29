use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::catalog::AppSpec;
use crate::github::{Release, Target};
use crate::http::size_text;
use crate::ops::Phase;
use crate::platform::{Installed, Origin};

use super::widgets::Tone;

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
    /// Installed by the system's package manager, which updates it instead of The Jar.
    System,
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
        if self.installed.as_ref().is_some_and(|installed| matches!(installed.origin, Origin::System(_))) {
            return Status::System;
        }
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

    /// A copy The Jar didn't install and can't tell the version of, so updating it is a leap.
    pub fn replaces_unknown(&self) -> bool {
        self.installed
            .as_ref()
            .is_some_and(|installed| installed.origin == Origin::Found && installed.version.is_none())
    }

    pub fn icon_uri(&self) -> Option<&str> {
        self.icon.as_ref().map(|icon| icon.uri.as_str())
    }

    pub fn installed_text(&self) -> String {
        match &self.installed {
            Some(installed) => match &installed.version {
                Some(version) => version.to_string(),
                None if installed.version_text.is_empty() => "Unknown".to_owned(),
                None => installed.version_text.clone(),
            },
            None => "Not installed".to_owned(),
        }
    }

    /// The badge on the app's card: what state it's in, at a glance.
    pub fn badge(&self) -> (&'static str, Tone) {
        match self.status() {
            Status::Checking => ("Checking the oven", Tone::Plain),
            Status::Update => ("New batch", Tone::New),
            Status::UpToDate => ("Fresh", Tone::Fresh),
            Status::NotInstalled => ("Not in the jar yet", Tone::Plain),
            Status::NoBuild => ("Still in the oven", Tone::Plain),
            Status::Unreachable => ("Couldn't check", Tone::Warn),
            Status::System => ("Store-bought", Tone::Plain),
        }
    }

    /// The version line beside the badge, such as "0.1.0 → 0.2.0".
    pub fn status_text(&self) -> String {
        let release = self.release();
        match self.status() {
            Status::Checking => String::new(),
            Status::Update => {
                let newest = release.map(|r| r.version.to_string()).unwrap_or_default();
                match self.installed.as_ref().and_then(|i| i.version.as_ref()) {
                    Some(version) => format!("{version} → {newest}"),
                    None => format!("{newest} is out"),
                }
            }
            Status::UpToDate | Status::Unreachable => {
                if self.installed.is_some() {
                    self.installed_text()
                } else {
                    String::new()
                }
            }
            Status::NotInstalled => {
                release.map(|r| format!("{} · {}", r.version, size_text(r.asset.size))).unwrap_or_default()
            }
            Status::NoBuild => format!("No {} build yet", Target::current().os_name()),
            Status::System => {
                let installed = self.installed.as_ref().and_then(|i| i.version.as_ref());
                match release {
                    Some(release) if installed.is_none_or(|v| release.version > *v) => {
                        format!("{} · {} is out", self.installed_text(), release.version)
                    }
                    _ => self.installed_text(),
                }
            }
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
                format!("Baking · {} of {}", size_text(self.done), size_text(self.total))
            }
            Phase::Downloading => "Baking…".to_owned(),
            Phase::Installing if self.kind == TaskKind::Update => "Swapping in the new batch…".to_owned(),
            Phase::Installing => "Cooling on the rack…".to_owned(),
            Phase::Removing => "Taking it out of the jar…".to_owned(),
        }
    }
}
