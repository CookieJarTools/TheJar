use egui::{ScrollArea, Ui};

use super::theme::{self, *};
use super::widgets::{self, Kind};
use super::{Action, JarApp};
use crate::catalog::{self, Source};
use crate::selfupdate;

pub fn show(app: &JarApp, ui: &mut Ui, out: &mut Vec<Action>) {
    widgets::label(ui, "Settings", theme::semibold(21.0), FG);
    ui.add_space(22.0);

    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.set_max_width(580.0);
        let mut settings = app.settings.clone();

        section(ui, "Updates");
        switch(
            ui,
            &mut settings.include_prereleases,
            "Include pre-releases",
            "Most CookieJarTools releases are marked as pre-releases. Turn this off to only get stable ones.",
        );
        ui.add_space(14.0);
        switch(
            ui,
            &mut settings.check_on_start,
            "Check when The Jar opens",
            "While it's open, The Jar also checks every six hours. Press F5 to check now.",
        );
        if settings != app.settings {
            out.push(Action::Settings(settings));
        }

        ui.add_space(28.0);
        section(ui, "Where apps go");
        install_location(ui, out);

        ui.add_space(28.0);
        section(ui, "App list");
        let source = match &app.source {
            Source::Remote => "Fetched from the CookieJarTools/TheJar repository.".to_owned(),
            Source::Saved(reason) => format!("Using the copy saved by the last check. {reason}"),
            Source::Bundled(reason) => format!("Using the list built into The Jar. {reason}"),
        };
        widgets::note(ui, source);
        widgets::label(ui, catalog::catalog_url(), theme::regular(12.0), FG_SUBTLE);
        ui.add_space(8.0);
        if widgets::small_button(ui, "Reload", Kind::Secondary).clicked() {
            out.push(Action::Check);
        }

        ui.add_space(28.0);
        section(ui, "About");
        widgets::label(ui, format!("The Jar {}", selfupdate::current()), theme::medium(13.0), FG);
        widgets::note(ui, "Licensed under the PolyForm Shield License 1.0.0.");
        ui.add_space(8.0);
        if widgets::small_button(ui, "GitHub", Kind::Secondary).clicked() {
            out.push(Action::OpenUrl(format!("https://github.com/{}", selfupdate::REPO)));
        }
        ui.add_space(12.0);
    });
}

#[cfg(not(windows))]
fn install_location(ui: &mut Ui, out: &mut Vec<Action>) {
    use crate::paths;

    widgets::label(ui, paths::pretty(&paths::apps_dir()), theme::regular(13.0), FG);
    widgets::note(
        ui,
        format!(
            "Apps are AppImages kept in this folder, with menu entries in {}. Each app keeps its own settings, and uninstalling leaves them alone.",
            paths::pretty(&paths::applications_dir())
        ),
    );
    ui.add_space(8.0);
    if widgets::small_button(ui, "Open folder", Kind::Secondary).clicked() {
        let _ = std::fs::create_dir_all(paths::apps_dir());
        out.push(Action::ShowFolder(paths::apps_dir()));
    }
}

#[cfg(windows)]
fn install_location(ui: &mut Ui, _out: &mut Vec<Action>) {
    widgets::note(
        ui,
        "Each app's own installer runs for your user account, the same as running it yourself. Apps appear in the Start menu and in Settings › Apps, and keep their settings when uninstalled.",
    );
}

fn section(ui: &mut Ui, title: &str) {
    widgets::label(ui, title, theme::medium(12.5), FG_SUBTLE);
    ui.add_space(10.0);
}

fn switch(ui: &mut Ui, value: &mut bool, title: &str, detail: &str) {
    ui.horizontal_top(|ui| {
        widgets::toggle(ui, value);
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            widgets::label(ui, title, theme::medium(13.0), FG);
            widgets::note(ui, detail);
        });
    });
}
