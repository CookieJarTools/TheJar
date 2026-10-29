//! The steps of installing and removing an app, shared by the window and the command line.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::catalog::AppSpec;
use crate::github::Release;
use crate::http::{CANCELLED, Http};
use crate::icons;
use crate::platform::{self, Installed};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Downloading,
    Installing,
    Removing,
}

/// Downloads and installs `release`, replacing `existing` if there is one.
/// Returns what's installed afterwards.
pub fn install(
    http: &Http,
    app: &AppSpec,
    release: &Release,
    existing: Option<&Installed>,
    report: &mut dyn FnMut(Phase, u64, u64),
    cancel: &AtomicBool,
) -> Result<Option<Installed>, String> {
    if let Some(reason) = existing.and_then(Installed::kept_elsewhere) {
        return Err(reason);
    }
    let file = platform::download_path(app, &release.asset)?;
    report(Phase::Downloading, 0, release.asset.size);
    http.download(&release.asset, &file, &mut |done, total| report(Phase::Downloading, done, total), cancel)?;
    if cancel.load(Ordering::Relaxed) {
        let _ = std::fs::remove_file(&file);
        return Err(CANCELLED.to_owned());
    }

    report(Phase::Installing, 0, 0);
    let icon = icons::refresh(http, app);
    platform::install(app, release, &file, icon.as_deref(), existing)?;
    match platform::detect(app) {
        Some(installed) => Ok(Some(installed)),
        None => Err(format!("The installer finished, but {} doesn't show up as installed.", app.name)),
    }
}

pub fn uninstall(app: &AppSpec, installed: &Installed, report: &mut dyn FnMut(Phase, u64, u64)) -> Result<(), String> {
    if let Some(reason) = installed.kept_elsewhere() {
        return Err(reason);
    }
    report(Phase::Removing, 0, 0);
    platform::uninstall(app, installed)
}
