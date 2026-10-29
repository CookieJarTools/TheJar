//! Everything slow runs on a thread and reports back through [`Bus`], which also wakes the window.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;

use crate::catalog::{self, AppSpec, Loaded};
use crate::github::{self, Release};
use crate::http::Http;
use crate::icons;
use crate::ops::{self, Phase};
use crate::platform::{self, Installed};
use crate::selfupdate::{self, Ready};

pub enum Msg {
    Catalog(Loaded),
    Detected(String, Option<Installed>),
    Icon(String, Vec<u8>),
    Latest(String, Result<Option<Release>, String>),
    SelfLatest(Result<Option<Release>, String>),
    CheckDone,
    Progress(String, Phase, u64, u64),
    Finished(String, Result<Option<Installed>, String>),
    SelfProgress(u64, u64),
    SelfReady(Result<Ready, String>),
}

#[derive(Clone)]
pub struct Bus {
    tx: Sender<Msg>,
    ctx: egui::Context,
}

impl Bus {
    pub fn new(tx: Sender<Msg>, ctx: egui::Context) -> Self {
        Self { tx, ctx }
    }

    pub fn send(&self, msg: Msg) {
        if self.tx.send(msg).is_ok() {
            self.ctx.request_repaint();
        }
    }
}

pub fn check(bus: Bus, http: Arc<Http>, include_prereleases: bool) {
    std::thread::spawn(move || {
        let loaded = catalog::load(&http);
        let apps = loaded.apps.clone();
        bus.send(Msg::Catalog(loaded));
        for app in &apps {
            bus.send(Msg::Detected(app.id.clone(), platform::detect(app)));
            let latest = github::latest(&http, &app.repo, app.asset_pattern(), include_prereleases);
            bus.send(Msg::Latest(app.id.clone(), latest));
            if let Some(icon) = icons::refresh(&http, app) {
                bus.send(Msg::Icon(app.id.clone(), icon));
            }
        }
        bus.send(Msg::SelfLatest(selfupdate::newer(&http, include_prereleases)));
        bus.send(Msg::CheckDone);
    });
}

pub fn install(
    bus: Bus,
    http: Arc<Http>,
    app: AppSpec,
    release: Release,
    existing: Option<Installed>,
    cancel: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let id = app.id.clone();
        let result = ops::install(
            &http,
            &app,
            &release,
            existing.as_ref(),
            &mut |phase, done, total| bus.send(Msg::Progress(id.clone(), phase, done, total)),
            &cancel,
        );
        bus.send(Msg::Finished(id, result));
    });
}

pub fn uninstall(bus: Bus, app: AppSpec, installed: Installed) {
    std::thread::spawn(move || {
        let id = app.id.clone();
        let result = ops::uninstall(&app, &installed, &mut |phase, done, total| {
            bus.send(Msg::Progress(id.clone(), phase, done, total))
        });
        bus.send(Msg::Finished(id, result.map(|_| None)));
    });
}

pub fn update_self(bus: Bus, http: Arc<Http>, release: Release, cancel: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let result =
            selfupdate::download(&http, &release, &mut |done, total| bus.send(Msg::SelfProgress(done, total)), &cancel);
        bus.send(Msg::SelfReady(result));
    });
}
