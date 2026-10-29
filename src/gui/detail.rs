use egui::{Align, Align2, Layout, Margin, ScrollArea, Sense, Stroke, Ui, vec2};
use egui_commonmark::CommonMarkViewer;

use super::entry::{Entry, Status};
use super::theme::{self, *};
use super::widgets::{self, Kind, Tone};
use super::{Action, JarApp};
use crate::github::{self, Target};
use crate::http::size_text;
use crate::ops::Phase;
use crate::paths;

pub fn show(app: &JarApp, ui: &mut Ui, out: &mut Vec<Action>) {
    let Some(entry) = app.selected_entry() else {
        return;
    };
    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        heading(entry, ui, out);
        ui.add_space(12.0);
        badges(entry, ui);
        ui.add_space(14.0);
        actions(entry, ui, out);
        messages(entry, ui, out);
        ui.add_space(20.0);
        facts(entry, ui, out);
        ui.add_space(20.0);
        notes(app, entry, ui);
    });
}

fn heading(entry: &Entry, ui: &mut Ui, out: &mut Vec<Action>) {
    ui.horizontal_top(|ui| {
        widgets::app_icon(ui, entry.icon_uri(), &entry.spec.name, 60.0);
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - 56.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            widgets::label(ui, &entry.spec.name, theme::display(23.0), FG);
            widgets::note(ui, &entry.spec.tagline);
        });
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            if widgets::small_button(ui, "Close", Kind::Ghost).clicked() {
                out.push(Action::Select(None));
            }
        });
    });
}

fn badges(entry: &Entry, ui: &mut Ui) {
    ui.horizontal(|ui| {
        let (text, tone) = entry.badge();
        widgets::badge(ui, text, tone);
        if entry.release().is_some_and(|r| r.prerelease) {
            widgets::badge(ui, "Pre-release", Tone::Plain);
        }
    });
}

fn actions(entry: &Entry, ui: &mut Ui, out: &mut Vec<Action>) {
    let id = &entry.spec.id;
    if let Some(task) = &entry.task {
        widgets::label(ui, task.text(), theme::regular(12.5), FG_MUTED);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            widgets::progress(ui, task.fraction(), (ui.available_width() - 80.0).max(60.0));
            if task.phase == Phase::Downloading && widgets::small_button(ui, "Cancel", Kind::Ghost).clicked() {
                out.push(Action::Cancel(id.clone()));
            }
        });
        return;
    }
    ui.horizontal(|ui| {
        match entry.status() {
            Status::Update => {
                let text = format!("Update to {}", entry.release().map(|r| r.version.to_string()).unwrap_or_default());
                if widgets::button(ui, &text, Kind::Primary).clicked() {
                    out.push(Action::Install(id.clone()));
                }
            }
            Status::NotInstalled if widgets::button(ui, "Install", Kind::Primary).clicked() => {
                out.push(Action::Install(id.clone()));
            }
            _ => {}
        }
        if entry.installed.is_some() && widgets::button(ui, "Open", Kind::Secondary).clicked() {
            out.push(Action::Open(id.clone()));
        }
    });
}

fn messages(entry: &Entry, ui: &mut Ui, out: &mut Vec<Action>) {
    let (text, color) = match (&entry.error, &entry.notice) {
        (Some(error), _) => (error, DANGER),
        (None, Some(notice)) => (notice, FG_MUTED),
        _ => return,
    };
    ui.add_space(12.0);
    egui::Frame::new()
        .fill(color.gamma_multiply(0.08))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.25)))
        .corner_radius(12)
        .inner_margin(12)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(egui::RichText::new(text).font(theme::regular(12.5)).color(color)).wrap());
            ui.add_space(4.0);
            if widgets::small_button(ui, "Dismiss", Kind::Ghost).clicked() {
                out.push(Action::DismissError(entry.spec.id.clone()));
            }
        });
}

fn facts(entry: &Entry, ui: &mut Ui, out: &mut Vec<Action>) {
    let release = entry.release();
    let newest = match (release, entry.check_error(), &entry.latest) {
        (Some(release), _, _) => release.version.to_string(),
        (None, Some(err), _) => format!("Couldn't check: {err}"),
        (None, None, None) => "Checking…".to_owned(),
        (None, None, Some(_)) => format!("No {} build yet", Target::current().os_name()),
    };
    let mut rows = vec![("Installed", entry.installed_text()), ("Newest", newest)];
    if let Some(date) = release.and_then(|r| r.published.as_deref()).and_then(github::date_text) {
        rows.push(("Baked on", date));
    }
    if let Some(release) = release {
        rows.push(("File", format!("{} · {}", release.asset.name, size_text(release.asset.size))));
    }
    if let Some(installed) = &entry.installed {
        rows.push(("Folder", paths::pretty(&installed.location)));
    }

    egui::Frame::new()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(14)
        .inner_margin(Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 3.0;
            widgets::label(ui, "The recipe", theme::display(15.0), ACCENT_TEXT);
            ui.add_space(2.0);
            for (label, value) in &rows {
                widgets::dotted_rule(ui);
                fact(ui, label, value);
            }
        });

    ui.add_space(12.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let page = release.map_or_else(|| entry.spec.releases_url(), |r| r.page_url.clone());
        if widgets::small_button(ui, "Release page", Kind::Ghost).clicked() {
            out.push(Action::OpenUrl(page));
        }
        if widgets::small_button(ui, "Repository", Kind::Ghost).clicked() {
            out.push(Action::OpenUrl(entry.spec.repo_url()));
        }
        if let Some(installed) = &entry.installed {
            if widgets::small_button(ui, "Show folder", Kind::Ghost).clicked() {
                out.push(Action::ShowFolder(installed.location.clone()));
            }
            if entry.task.is_none() && widgets::small_button(ui, "Uninstall", Kind::Danger).clicked() {
                out.push(Action::Uninstall(entry.spec.id.clone()));
            }
        }
    });
}

fn fact(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(74.0, 18.0), Sense::hover());
        ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, label, theme::regular(12.5), FG_SUBTLE);
        let text = egui::RichText::new(value).font(theme::regular(12.5)).color(FG);
        ui.add(egui::Label::new(text).truncate()).on_hover_text(value);
    });
}

fn notes(app: &JarApp, entry: &Entry, ui: &mut Ui) {
    let Some(release) = entry.release() else {
        return;
    };
    widgets::label(ui, format!("What's fresh in {}", release.version), theme::display(17.0), FG);
    ui.add_space(8.0);
    if release.notes.trim().is_empty() {
        widgets::note(ui, "This release has no notes.");
        return;
    }
    let mut cache = app.markdown.borrow_mut();
    CommonMarkViewer::new().show(ui, &mut cache, &release.notes);
}
