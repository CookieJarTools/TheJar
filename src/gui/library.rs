use egui::{Align, CornerRadius, Layout, Rect, ScrollArea, Sense, Stroke, Ui, UiBuilder, pos2, vec2};

use super::entry::{Entry, Status};
use super::theme::{self, *};
use super::widgets::{self, Kind};
use super::{Action, JarApp};
use crate::catalog::Source;
use crate::ops::Phase;

const ROW_HEIGHT: f32 = 64.0;

pub fn show(app: &JarApp, ui: &mut Ui, out: &mut Vec<Action>) {
    header(app, ui, out);
    ui.add_space(18.0);

    if app.entries.is_empty() {
        widgets::note(ui, "There are no apps in the list yet.");
        return;
    }
    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Frame::new().fill(BG_SOFT).stroke(Stroke::new(1.0, LINE)).corner_radius(8).inner_margin(4).show(
            ui,
            |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, entry) in app.entries.iter().enumerate() {
                    if i > 0 {
                        divider(ui);
                    }
                    row(app, entry, ui, out);
                }
            },
        );
    });
}

fn header(app: &JarApp, ui: &mut Ui, out: &mut Vec<Action>) {
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 30.0), Layout::left_to_right(Align::Center), |ui| {
        widgets::label(ui, "Library", theme::semibold(21.0), FG);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let updates = app.update_count();
            if updates > 1 && widgets::button(ui, &format!("Update all ({updates})"), Kind::Primary).clicked() {
                out.push(Action::UpdateAll);
            }
            let label = if app.checking { "Checking…" } else { "Check for updates" };
            let check = ui.add_enabled_ui(!app.checking, |ui| widgets::button(ui, label, Kind::Secondary)).inner;
            if check.on_hover_text("F5").clicked() {
                out.push(Action::Check);
            }
        });
    });

    let mut parts = vec![match app.entries.len() {
        1 => "1 app".to_owned(),
        n => format!("{n} apps"),
    }];
    match app.update_count() {
        0 => {}
        1 => parts.push("1 update".to_owned()),
        n => parts.push(format!("{n} updates")),
    }
    if let Some(at) = app.last_check {
        parts.push(format!("checked {}", ago(at.elapsed().as_secs())));
    }
    widgets::label(ui, parts.join(" · "), theme::regular(12.5), FG_MUTED);

    let source_note = match &app.source {
        Source::Bundled(reason) if !reason.is_empty() => {
            Some(format!("Showing the app list built into The Jar. {reason}"))
        }
        Source::Saved(reason) if !reason.is_empty() => {
            Some(format!("Showing the app list from the last check. {reason}"))
        }
        _ => None,
    };
    if let Some(text) = source_note {
        ui.add_space(2.0);
        widgets::label(ui, text, theme::regular(12.0), FG_SUBTLE);
    }
}

fn row(app: &JarApp, entry: &Entry, ui: &mut Ui, out: &mut Vec<Action>) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    let selected = app.selected.as_deref() == Some(entry.spec.id.as_str());
    let hovered = ui.rect_contains_pointer(rect);
    if selected || hovered {
        let fill = if selected { BG_ACTIVE } else { BG_HOVER };
        ui.painter().rect_filled(rect, CornerRadius::same(6), fill);
    }
    if response.clicked() {
        let id = (!selected).then(|| entry.spec.id.clone());
        out.push(Action::Select(id));
    }

    let icon = Rect::from_min_size(pos2(rect.left() + 12.0, rect.center().y - 18.0), vec2(36.0, 36.0));
    widgets::paint_app_icon(ui, icon, entry.icon_uri(), &entry.spec.name);

    let actions_rect = rect.shrink2(vec2(14.0, 0.0));
    let mut actions_ui =
        ui.new_child(UiBuilder::new().max_rect(actions_rect).layout(Layout::right_to_left(Align::Center)));
    row_actions(entry, &mut actions_ui, out);

    let text_left = icon.right() + 14.0;
    let text_width = actions_ui.min_rect().left() - 20.0 - text_left;
    let name = theme::medium(14.0);
    widgets::paint_truncated(ui, pos2(text_left, rect.center().y - 18.0), &entry.spec.name, name, FG, text_width);
    let tagline = theme::regular(12.5);
    widgets::paint_truncated(
        ui,
        pos2(text_left, rect.center().y + 2.0),
        &entry.spec.tagline,
        tagline,
        FG_MUTED,
        text_width,
    );
}

/// Laid out right to left, so the first thing added sits furthest right.
fn row_actions(entry: &Entry, ui: &mut Ui, out: &mut Vec<Action>) {
    let id = &entry.spec.id;
    if let Some(task) = &entry.task {
        if task.phase == Phase::Downloading && widgets::small_button(ui, "Cancel", Kind::Ghost).clicked() {
            out.push(Action::Cancel(id.clone()));
        }
        ui.allocate_ui_with_layout(vec2(150.0, 32.0), Layout::top_down(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            widgets::label(ui, task.text(), theme::regular(12.0), FG_MUTED);
            widgets::progress(ui, task.fraction(), 150.0);
        });
        return;
    }

    let status = entry.status();
    let primary = match status {
        Status::Update => Some(("Update", Kind::Accent)),
        Status::NotInstalled => Some(("Install", Kind::Secondary)),
        _ => None,
    };
    if let Some((text, kind)) = primary
        && widgets::button(ui, text, kind).clicked()
    {
        out.push(Action::Install(id.clone()));
    }
    if entry.installed.is_some() && widgets::button(ui, "Open", Kind::Secondary).clicked() {
        out.push(Action::Open(id.clone()));
    }

    ui.add_space(6.0);
    let color = match status {
        Status::Update => ACCENT_TEXT,
        Status::Unreachable => WARN,
        _ => FG_SUBTLE,
    };
    let text = widgets::label(ui, entry.status_text(), theme::regular(12.5), color);
    if let Some(err) = entry.check_error() {
        text.on_hover_text(err);
    }
    if let Some(err) = &entry.error {
        widgets::chip(ui, "Failed", DANGER, DANGER.gamma_multiply(0.14)).on_hover_text(err);
    }
}

fn divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    let y = rect.center().y;
    ui.painter().line_segment([pos2(rect.left() + 60.0, y), pos2(rect.right() - 8.0, y)], Stroke::new(1.0, LINE));
}

fn ago(seconds: u64) -> String {
    match seconds {
        0..=59 => "just now".to_owned(),
        60..=3599 => format!("{} min ago", seconds / 60),
        _ => format!("{} h ago", seconds / 3600),
    }
}
