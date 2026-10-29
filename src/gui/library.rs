use egui::{
    Align, Color32, CornerRadius, Layout, Rect, ScrollArea, Sense, Shadow, Stroke, StrokeKind, Ui, UiBuilder, pos2,
    vec2,
};

use super::entry::{Entry, Status};
use super::theme::{self, *};
use super::widgets::{self, Kind, Tone};
use super::{Action, JarApp};
use crate::catalog::Source;
use crate::ops::Phase;

const HERO_FILL: Color32 = Color32::from_rgb(0x2b, 0x1f, 0x16);
const CARD_HEIGHT: f32 = 156.0;
const CARD_MIN_WIDTH: f32 = 290.0;
const GAP: f32 = 16.0;

pub fn show(app: &JarApp, ui: &mut Ui, out: &mut Vec<Action>) {
    // A big faint cookie in the corner, behind everything else.
    let panel = ui.clip_rect();
    widgets::paint_cookie(ui.painter(), panel.right_bottom() - vec2(40.0, 30.0), 120.0, 0.045, Some(BG));

    hero(app, ui, out);
    source_note(app, ui);
    ui.add_space(24.0);

    if app.entries.is_empty() {
        empty(ui);
        return;
    }
    widgets::label(ui, "In the jar", theme::display(16.0), FG_MUTED);
    ui.add_space(10.0);
    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| grid(app, ui, out));
}

fn hero(app: &JarApp, ui: &mut Ui, out: &mut Vec<Action>) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 132.0), Sense::hover());
    let painter = ui.painter();
    painter.rect(rect, CornerRadius::same(18), HERO_FILL, Stroke::new(1.0, LINE), StrokeKind::Inside);

    let cookies = rect.width() > 520.0;
    if cookies {
        let big = pos2(rect.right() - 86.0, rect.center().y + 8.0);
        widgets::paint_cookie(painter, pos2(rect.right() - 162.0, rect.center().y - 20.0), 28.0, 0.85, None);
        widgets::paint_cookie(painter, big, 50.0, 1.0, Some(HERO_FILL));
        widgets::paint_crumbs(painter, big + vec2(-40.0, 26.0), 22.0, 0.8);
    }

    let room_for_cookies = if cookies { 230.0 } else { 26.0 };
    let content = Rect::from_min_max(rect.min + vec2(26.0, 22.0), rect.max - vec2(room_for_cookies, 18.0));
    let mut ui = ui.new_child(UiBuilder::new().max_rect(content).layout(Layout::top_down(Align::Min)));
    ui.spacing_mut().item_spacing.y = 2.0;
    widgets::label(&mut ui, "Your jar", theme::display(28.0), FG);
    widgets::label(&mut ui, summary(app), theme::regular(13.0), FG_MUTED);
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        let updates = app.update_count();
        if updates > 1 && widgets::button(ui, &format!("Update all ({updates})"), Kind::Primary).clicked() {
            out.push(Action::UpdateAll);
        }
        let label = if app.checking { "Checking the oven…" } else { "Check for updates" };
        let check = ui.add_enabled_ui(!app.checking, |ui| widgets::button(ui, label, Kind::Secondary)).inner;
        if check.on_hover_text("F5").clicked() {
            out.push(Action::Check);
        }
    });
}

fn summary(app: &JarApp) -> String {
    let mut parts = vec![match app.entries.len() {
        1 => "1 app in the jar".to_owned(),
        n => format!("{n} apps in the jar"),
    }];
    match app.update_count() {
        0 if app.last_check.is_some() => parts.push("all fresh".to_owned()),
        0 => {}
        1 => parts.push("1 new batch waiting".to_owned()),
        n => parts.push(format!("{n} new batches waiting")),
    }
    if let Some(at) = app.last_check {
        parts.push(format!("checked {}", ago(at.elapsed().as_secs())));
    }
    parts.join(" · ")
}

fn source_note(app: &JarApp, ui: &mut Ui) {
    let text = match &app.source {
        Source::Bundled(reason) if !reason.is_empty() => format!("Showing the app list built into The Jar. {reason}"),
        Source::Saved(reason) if !reason.is_empty() => format!("Showing the app list from the last check. {reason}"),
        _ => return,
    };
    ui.add_space(8.0);
    widgets::label(ui, text, theme::regular(12.0), FG_SUBTLE);
}

fn grid(app: &JarApp, ui: &mut Ui, out: &mut Vec<Action>) {
    let width = ui.available_width();
    let columns = (((width + GAP) / (CARD_MIN_WIDTH + GAP)).floor() as usize).clamp(1, 3);
    let card_width = (width - GAP * (columns - 1) as f32) / columns as f32;
    // Room for the hover lift and shadow at the top.
    ui.add_space(4.0);
    for row in app.entries.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            for entry in row {
                card(app, entry, ui, out, card_width);
            }
        });
        ui.add_space(GAP);
    }
}

fn card(app: &JarApp, entry: &Entry, ui: &mut Ui, out: &mut Vec<Action>, width: f32) {
    let (rect, response) = ui.allocate_exact_size(vec2(width, CARD_HEIGHT), Sense::click());
    let selected = app.selected.as_deref() == Some(entry.spec.id.as_str());
    let hovered = ui.rect_contains_pointer(rect);
    let lift = ui.ctx().animate_bool(response.id.with("lift"), hovered);
    let card = rect.translate(vec2(0.0, -3.0 * lift));
    if response.clicked() {
        out.push(Action::Select((!selected).then(|| entry.spec.id.clone())));
    }

    let painter = ui.painter();
    let shadow =
        Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(80 + (50.0 * lift) as u8) };
    painter.add(shadow.as_shape(card, CornerRadius::same(16)));
    let stroke = match (selected, hovered) {
        (true, _) => Stroke::new(1.5, ACCENT),
        (false, true) => Stroke::new(1.0, LINE_STRONG),
        _ => Stroke::new(1.0, LINE),
    };
    let fill = if selected { BG_HOVER } else { BG_CARD };
    painter.rect(card, CornerRadius::same(16), fill, stroke, StrokeKind::Inside);

    let inner = card.shrink(18.0);
    let icon = Rect::from_min_size(inner.min, vec2(52.0, 52.0));
    widgets::paint_app_icon(ui, icon, entry.icon_uri(), &entry.spec.name);
    let text_left = icon.right() + 14.0;
    let text_width = inner.right() - text_left;
    let name = widgets::paint_wrapped(
        ui,
        pos2(text_left, inner.top() + 1.0),
        &entry.spec.name,
        theme::display(19.0),
        FG,
        text_width,
        1,
    );
    let tagline_pos = pos2(text_left, name.bottom() + 3.0);
    widgets::paint_wrapped(ui, tagline_pos, &entry.spec.tagline, theme::regular(12.5), FG_MUTED, text_width, 2);

    let bottom = Rect::from_min_max(pos2(inner.left(), inner.bottom() - 30.0), inner.max);
    if let Some(task) = &entry.task {
        task_row(entry, task, ui, bottom, out);
        return;
    }
    let mut buttons = ui.new_child(UiBuilder::new().max_rect(bottom).layout(Layout::right_to_left(Align::Center)));
    card_buttons(entry, &mut buttons, out);
    let status_area = Rect::from_min_max(bottom.min, pos2(buttons.min_rect().left() - 8.0, bottom.bottom()));
    let mut status = ui.new_child(UiBuilder::new().max_rect(status_area).layout(Layout::left_to_right(Align::Center)));
    card_status(entry, &mut status);
}

/// Laid out right to left, so the first button added sits furthest right.
fn card_buttons(entry: &Entry, ui: &mut Ui, out: &mut Vec<Action>) {
    let id = &entry.spec.id;
    let primary = match entry.status() {
        Status::Update => Some(("Update", Kind::Primary)),
        Status::NotInstalled => Some(("Install", Kind::Accent)),
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
}

fn card_status(entry: &Entry, ui: &mut Ui) {
    let (text, tone) = entry.badge();
    let badge = widgets::badge(ui, text, tone);
    if let Some(err) = entry.check_error() {
        badge.on_hover_text(err);
    }
    let version = entry.status_text();
    if !version.is_empty() {
        let color = if entry.status() == Status::Update { ACCENT_TEXT } else { FG_SUBTLE };
        let text = egui::RichText::new(&version).font(theme::regular(12.5)).color(color);
        ui.add(egui::Label::new(text).truncate()).on_hover_text(version);
    }
    if let Some(err) = &entry.error {
        widgets::badge(ui, "Failed", Tone::Warn).on_hover_text(err);
    }
}

fn task_row(entry: &Entry, task: &super::entry::Task, ui: &mut Ui, area: Rect, out: &mut Vec<Action>) {
    let mut row = ui.new_child(UiBuilder::new().max_rect(area).layout(Layout::right_to_left(Align::Center)));
    if task.phase == Phase::Downloading && widgets::small_button(&mut row, "Cancel", Kind::Ghost).clicked() {
        out.push(Action::Cancel(entry.spec.id.clone()));
    }
    let width = (row.available_width() - 4.0).max(60.0);
    row.allocate_ui_with_layout(vec2(width, 30.0), Layout::top_down(Align::Min), |ui| {
        ui.spacing_mut().item_spacing.y = 5.0;
        widgets::label(ui, task.text(), theme::regular(12.0), FG_MUTED);
        widgets::progress(ui, task.fraction(), width);
    });
}

fn empty(ui: &mut Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        let (rect, _) = ui.allocate_exact_size(vec2(90.0, 90.0), Sense::hover());
        widgets::paint_cookie(ui.painter(), rect.center(), 40.0, 1.0, Some(BG));
        ui.add_space(12.0);
        widgets::label(ui, "The jar is empty", theme::display(20.0), FG);
        widgets::note(ui, "Apps appear here as soon as they're added to the app list.");
    });
}

fn ago(seconds: u64) -> String {
    match seconds {
        0..=59 => "just now".to_owned(),
        60..=3599 => format!("{} min ago", seconds / 60),
        _ => format!("{} h ago", seconds / 3600),
    }
}
