mod detail;
mod entry;
mod library;
mod settings_view;
mod sidebar;
mod theme;
mod widgets;
mod worker;

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use egui::{Align, Color32, Frame, Key, Layout, Margin, Stroke, vec2};
use egui_commonmark::CommonMarkCache;

use crate::catalog::{self, Source};
use crate::github::{self, Release};
use crate::http::{CANCELLED, Http};
use crate::icons;
use crate::platform;
use crate::selfupdate::{self, Ready};
use crate::settings::Settings;
use entry::{Entry, Icon, Status, Task, TaskKind};
use theme::*;
use widgets::Kind;
use worker::{Bus, Msg};

const RECHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

pub fn run() -> eframe::Result {
    #[cfg(not(windows))]
    if let selfupdate::Kind::AppImage(appimage) = selfupdate::kind() {
        platform::integrate_self(&selfupdate::spec(), &appimage, selfupdate::ICON_PNG);
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("The Jar")
        .with_app_id("thejar")
        .with_inner_size([1040.0, 660.0])
        .with_min_inner_size([780.0, 480.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(selfupdate::ICON_PNG) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("The Jar", options, Box::new(|cc| Ok(Box::new(JarApp::new(cc)))))
}

#[derive(Clone, Copy, PartialEq)]
pub enum View {
    Library,
    Settings,
}

pub enum SelfUpdate {
    Idle,
    Available(Release),
    Downloading { release: Release, done: u64, total: u64, cancel: Arc<AtomicBool> },
    Ready(Release, Ready),
    Failed(Release, String),
}

#[derive(Clone)]
enum Confirm {
    Uninstall(String),
    CloseFirst(String),
}

/// What a click asked for. Views only read state and push these; [`JarApp::apply`] acts on them.
pub enum Action {
    Install(String),
    InstallConfirmed(String),
    Uninstall(String),
    UninstallConfirmed(String),
    Open(String),
    Cancel(String),
    Select(Option<String>),
    Check,
    UpdateAll,
    Show(View),
    ShowFolder(PathBuf),
    OpenUrl(String),
    DismissError(String),
    Settings(Settings),
    SelfUpdate,
    SelfCancel,
    SelfRestart,
    CloseDialog,
}

pub struct JarApp {
    ctx: egui::Context,
    http: Arc<Http>,
    bus: Bus,
    rx: Receiver<Msg>,
    settings: Settings,
    source: Source,
    entries: Vec<Entry>,
    selected: Option<String>,
    view: View,
    checking: bool,
    last_check: Option<Instant>,
    self_update: SelfUpdate,
    confirm: Option<Confirm>,
    markdown: RefCell<CommonMarkCache>,
    icon_serial: u32,
}

impl JarApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let (tx, rx) = mpsc::channel();
        let saved = catalog::load_saved();

        let mut app = Self {
            ctx: cc.egui_ctx.clone(),
            http: Arc::new(Http::new()),
            bus: Bus::new(tx, cc.egui_ctx.clone()),
            rx,
            settings: Settings::load(),
            source: saved.source,
            entries: Vec::new(),
            selected: None,
            view: View::Library,
            checking: false,
            last_check: None,
            self_update: SelfUpdate::Idle,
            confirm: None,
            markdown: RefCell::new(CommonMarkCache::default()),
            icon_serial: 0,
        };

        // Fill the window from what's on disk straight away; the check below brings it up to date.
        let include_prereleases = app.settings.include_prereleases;
        for spec in saved.apps {
            let mut entry = Entry::new(spec);
            entry.installed = platform::detect(&entry.spec);
            entry.latest =
                github::latest_saved(&entry.spec.repo, entry.spec.asset_pattern(), include_prereleases).map(Ok);
            let icon = icons::cached(&entry.spec);
            let id = entry.spec.id.clone();
            app.entries.push(entry);
            if let Some(icon) = icon {
                app.set_icon(&id, icon);
            }
        }

        if app.settings.check_on_start {
            app.check_now();
        }
        app
    }

    fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.spec.id == id)
    }

    fn entry_mut(&mut self, id: &str) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.spec.id == id)
    }

    fn selected_entry(&self) -> Option<&Entry> {
        self.selected.as_deref().and_then(|id| self.entry(id))
    }

    fn update_count(&self) -> usize {
        self.entries.iter().filter(|e| e.status() == Status::Update).count()
    }

    fn check_now(&mut self) {
        if self.checking {
            return;
        }
        self.checking = true;
        worker::check(self.bus.clone(), self.http.clone(), self.settings.include_prereleases);
    }

    fn set_icon(&mut self, id: &str, bytes: Vec<u8>) {
        let ctx = self.ctx.clone();
        self.icon_serial += 1;
        let uri = format!("bytes://icons/{id}-{}.png", self.icon_serial);
        let Some(entry) = self.entry_mut(id) else {
            return;
        };
        if entry.icon.as_ref().is_some_and(|icon| icon.bytes == bytes) {
            return;
        }
        if let Some(old) = entry.icon.take() {
            ctx.forget_image(&old.uri);
        }
        ctx.include_bytes(uri.clone(), bytes.clone());
        entry.icon = Some(Icon { uri, bytes });
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Catalog(loaded) => self.take_catalog(loaded),
            Msg::Detected(id, installed) => {
                if let Some(entry) = self.entry_mut(&id).filter(|e| e.task.is_none()) {
                    entry.installed = installed;
                }
            }
            Msg::Icon(id, bytes) => self.set_icon(&id, bytes),
            Msg::Latest(id, latest) => {
                if let Some(entry) = self.entry_mut(&id) {
                    // A failed check keeps showing what the last good one found.
                    let keep_old = latest.is_err() && entry.release().is_some();
                    if !keep_old {
                        entry.latest = Some(latest);
                    }
                }
            }
            Msg::SelfLatest(result) => {
                if matches!(self.self_update, SelfUpdate::Idle | SelfUpdate::Available(_)) {
                    self.self_update = match result {
                        Ok(Some(release)) => SelfUpdate::Available(release),
                        _ => SelfUpdate::Idle,
                    };
                }
            }
            Msg::CheckDone => {
                self.checking = false;
                self.last_check = Some(Instant::now());
            }
            Msg::Progress(id, phase, done, total) => {
                if let Some(task) = self.entry_mut(&id).and_then(|e| e.task.as_mut()) {
                    task.phase = phase;
                    task.done = done;
                    task.total = total;
                }
            }
            Msg::Finished(id, result) => self.finish(&id, result),
            Msg::SelfProgress(done, total) => {
                if let SelfUpdate::Downloading { done: d, total: t, .. } = &mut self.self_update {
                    *d = done;
                    *t = total;
                }
            }
            Msg::SelfReady(result) => {
                let state = std::mem::replace(&mut self.self_update, SelfUpdate::Idle);
                let SelfUpdate::Downloading { release, .. } = state else {
                    return;
                };
                self.self_update = match result {
                    Ok(ready) => SelfUpdate::Ready(release, ready),
                    Err(err) if err == CANCELLED => SelfUpdate::Available(release),
                    Err(err) => SelfUpdate::Failed(release, err),
                };
            }
        }
    }

    fn take_catalog(&mut self, loaded: catalog::Loaded) {
        self.source = loaded.source;
        let mut old: Vec<Entry> = std::mem::take(&mut self.entries);
        for spec in loaded.apps {
            match old.iter().position(|e| e.spec.id == spec.id) {
                Some(at) => {
                    let mut entry = old.remove(at);
                    entry.spec = spec;
                    self.entries.push(entry);
                }
                None => self.entries.push(Entry::new(spec)),
            }
        }
        // Apps dropped from the list stay while something is still happening to them.
        self.entries.extend(old.into_iter().filter(|e| e.task.is_some()));
        if self.selected.as_deref().is_some_and(|id| self.entry(id).is_none()) {
            self.selected = None;
        }
    }

    fn finish(&mut self, id: &str, result: Result<Option<platform::Installed>, String>) {
        let Some(entry) = self.entry_mut(id) else {
            return;
        };
        let Some(task) = entry.task.take() else {
            return;
        };
        match result {
            Ok(installed) => {
                entry.error = None;
                entry.notice = match (&installed, task.was_running) {
                    (Some(now), true) => Some(format!(
                        "{} is still running the old version. Reopen it to use {}.",
                        entry.spec.name,
                        now.version.as_ref().map_or(now.version_text.clone(), ToString::to_string)
                    )),
                    _ => None,
                };
                entry.installed = installed;
            }
            Err(err) if err == CANCELLED => {}
            Err(err) => entry.error = Some(err),
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Install(id) => self.request_install(&id),
            Action::InstallConfirmed(id) => {
                self.confirm = None;
                self.start_install(&id);
            }
            Action::Uninstall(id) => self.confirm = Some(Confirm::Uninstall(id)),
            Action::UninstallConfirmed(id) => {
                self.confirm = None;
                self.start_uninstall(&id);
            }
            Action::Open(id) => self.open(&id),
            Action::Cancel(id) => {
                if let Some(task) = self.entry(&id).and_then(|e| e.task.as_ref()) {
                    task.cancel.store(true, Ordering::Relaxed);
                }
            }
            Action::Select(id) => self.selected = id,
            Action::Check => self.check_now(),
            Action::UpdateAll => {
                let ids: Vec<String> = self
                    .entries
                    .iter()
                    .filter(|e| e.status() == Status::Update && e.task.is_none())
                    .map(|e| e.spec.id.clone())
                    .collect();
                for id in ids {
                    self.request_install(&id);
                }
            }
            Action::Show(view) => self.view = view,
            Action::ShowFolder(path) => {
                let _ = open::that_detached(path);
            }
            Action::OpenUrl(url) => {
                let _ = open::that_detached(url);
            }
            Action::DismissError(id) => {
                if let Some(entry) = self.entry_mut(&id) {
                    entry.error = None;
                    entry.notice = None;
                }
            }
            Action::Settings(settings) => {
                let recheck = settings.include_prereleases != self.settings.include_prereleases;
                self.settings = settings;
                let _ = self.settings.save();
                if recheck {
                    self.check_now();
                }
            }
            Action::SelfUpdate => self.start_self_update(),
            Action::SelfCancel => {
                if let SelfUpdate::Downloading { cancel, .. } = &self.self_update {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            Action::SelfRestart => {
                if let SelfUpdate::Ready(release, ready) = &self.self_update {
                    match selfupdate::restart(ready) {
                        Ok(()) => self.ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                        Err(err) => self.self_update = SelfUpdate::Failed(release.clone(), err),
                    }
                }
            }
            Action::CloseDialog => self.confirm = None,
        }
    }

    fn request_install(&mut self, id: &str) {
        let Some(entry) = self.entry(id) else {
            return;
        };
        let running = entry.installed.as_ref().is_some_and(|installed| platform::is_running(&entry.spec, installed));
        if running && platform::CLOSES_APP_TO_UPDATE {
            // Only one question at a time; anything else waits for the next click.
            if self.confirm.is_none() {
                self.confirm = Some(Confirm::CloseFirst(id.to_owned()));
            }
            return;
        }
        self.start_install(id);
    }

    fn start_install(&mut self, id: &str) {
        let (bus, http) = (self.bus.clone(), self.http.clone());
        let Some(entry) = self.entry_mut(id) else {
            return;
        };
        let Some(release) = entry.release().cloned() else {
            return;
        };
        if entry.task.is_some() {
            return;
        }
        let existing = entry.installed.clone();
        let was_running = existing.as_ref().is_some_and(|i| platform::is_running(&entry.spec, i));
        let cancel = Arc::new(AtomicBool::new(false));
        entry.error = None;
        entry.notice = None;
        entry.task = Some(Task {
            kind: if existing.is_some() { TaskKind::Update } else { TaskKind::Install },
            phase: crate::ops::Phase::Downloading,
            done: 0,
            total: release.asset.size,
            cancel: cancel.clone(),
            was_running,
        });
        worker::install(bus, http, entry.spec.clone(), release, existing, cancel);
    }

    fn start_uninstall(&mut self, id: &str) {
        let bus = self.bus.clone();
        let Some(entry) = self.entry_mut(id) else {
            return;
        };
        let Some(installed) = entry.installed.clone() else {
            return;
        };
        if entry.task.is_some() {
            return;
        }
        entry.error = None;
        entry.notice = None;
        entry.task = Some(Task {
            kind: TaskKind::Uninstall,
            phase: crate::ops::Phase::Removing,
            done: 0,
            total: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            was_running: false,
        });
        worker::uninstall(bus, entry.spec.clone(), installed);
    }

    fn open(&mut self, id: &str) {
        let Some(entry) = self.entry_mut(id) else {
            return;
        };
        let Some(installed) = &entry.installed else {
            return;
        };
        if let Err(err) = platform::launch(installed) {
            entry.error = Some(err);
        }
    }

    fn start_self_update(&mut self) {
        let release = match &self.self_update {
            SelfUpdate::Available(release) | SelfUpdate::Failed(release, _) => release.clone(),
            _ => return,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.self_update = SelfUpdate::Downloading {
            release: release.clone(),
            done: 0,
            total: release.asset.size,
            cancel: cancel.clone(),
        };
        worker::update_self(self.bus.clone(), self.http.clone(), release, cancel);
    }

    fn confirm_dialog(&self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let Some(confirm) = &self.confirm else {
            return;
        };
        let (id, yes, kind) = match confirm {
            Confirm::Uninstall(id) => (id, "Uninstall", Kind::Destructive),
            Confirm::CloseFirst(id) => (id, "Close and update", Kind::Primary),
        };
        let name = self.entry(id).map_or(id.as_str(), |e| e.spec.name.as_str());
        let (title, body) = match confirm {
            Confirm::Uninstall(_) => (
                format!("Uninstall {name}?"),
                if cfg!(windows) {
                    format!("This runs {name}'s uninstaller. Its settings and data are kept.")
                } else {
                    format!("This removes {name} and its menu entry. Its settings and data are kept.")
                },
            ),
            Confirm::CloseFirst(_) => (
                format!("{name} is open"),
                format!("Updating closes {name} first, so save anything you're working on."),
            ),
        };

        let frame = Frame::new()
            .fill(BG_CARD)
            .stroke(Stroke::new(1.0, LINE_STRONG))
            .corner_radius(18)
            .inner_margin(Margin::same(22));
        let response = egui::Modal::new(egui::Id::unique("confirm"))
            .backdrop_color(Color32::from_black_alpha(170))
            .frame(frame)
            .show(ctx, |ui| {
                ui.set_width(380.0);
                widgets::label(ui, title, theme::display(19.0), FG);
                ui.add_space(4.0);
                widgets::note(ui, body);
                ui.add_space(18.0);
                ui.allocate_ui_with_layout(
                    vec2(ui.available_width(), 28.0),
                    Layout::right_to_left(Align::Center),
                    |ui| {
                        if widgets::button(ui, yes, kind).clicked() {
                            actions.push(match confirm {
                                Confirm::Uninstall(id) => Action::UninstallConfirmed(id.clone()),
                                Confirm::CloseFirst(id) => Action::InstallConfirmed(id.clone()),
                            });
                        }
                        if widgets::button(ui, "Cancel", Kind::Ghost).clicked() {
                            actions.push(Action::CloseDialog);
                        }
                    },
                );
            });
        if response.should_close() {
            actions.push(Action::CloseDialog);
        }
    }
}

impl eframe::App for JarApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        while let Ok(msg) = self.rx.try_recv() {
            self.handle(msg);
        }
        let ctx = ui.ctx().clone();
        let refresh_keys = ctx.input(|i| i.key_pressed(Key::F5) || (i.modifiers.command && i.key_pressed(Key::R)));
        let due = self.last_check.is_some_and(|at| at.elapsed() > RECHECK_EVERY);
        if refresh_keys || due {
            self.check_now();
        }
        ctx.request_repaint_after(Duration::from_secs(60));

        let mut actions = Vec::new();
        egui::Panel::left("sidebar")
            .exact_size(220.0)
            .resizable(false)
            .frame(Frame::new().fill(BG_SOFT).inner_margin(Margin::symmetric(14, 18)))
            .show(ui, |ui| sidebar::show(self, ui, &mut actions));
        if self.view == View::Library && self.selected_entry().is_some() {
            egui::Panel::right("detail")
                .exact_size(380.0)
                .resizable(false)
                .frame(Frame::new().fill(BG_SOFT).inner_margin(Margin::symmetric(22, 20)))
                .show(ui, |ui| detail::show(self, ui, &mut actions));
        }
        egui::CentralPanel::default().frame(Frame::new().fill(BG).inner_margin(Margin::symmetric(28, 26))).show(
            ui,
            |ui| match self.view {
                View::Library => library::show(self, ui, &mut actions),
                View::Settings => settings_view::show(self, ui, &mut actions),
            },
        );
        self.confirm_dialog(&ctx, &mut actions);

        for action in actions {
            self.apply(action);
        }
    }
}
