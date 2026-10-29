use egui::{Align, Align2, CornerRadius, CursorIcon, Layout, Response, Sense, Stroke, Ui, vec2};

use super::theme::{self, *};
use super::widgets::{self, Kind};
use super::{Action, JarApp, SelfUpdate, View};
use crate::selfupdate;

pub fn show(app: &JarApp, ui: &mut Ui, out: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        let logo = egui::Image::from_bytes("bytes://thejar-logo.png", selfupdate::ICON_PNG);
        ui.add(logo.fit_to_exact_size(vec2(22.0, 22.0)));
        ui.add_space(2.0);
        widgets::label(ui, "The Jar", theme::semibold(14.0), FG);
    });
    ui.add_space(20.0);

    ui.spacing_mut().item_spacing.y = 2.0;
    if nav_item(ui, "Library", app.view == View::Library, app.update_count()).clicked() {
        out.push(Action::Show(View::Library));
    }
    if nav_item(ui, "Settings", app.view == View::Settings, 0).clicked() {
        out.push(Action::Show(View::Settings));
    }

    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        ui.add_space(2.0);
        widgets::label(ui, format!("The Jar {}", selfupdate::current()), theme::regular(11.5), FG_FAINT);
        ui.add_space(8.0);
        self_update_card(app, ui, out);
    });
}

fn nav_item(ui: &mut Ui, text: &str, selected: bool, badge: usize) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
    let hovered = response.hovered();
    if selected || hovered {
        let fill = if selected { BG_ACTIVE } else { BG_HOVER };
        ui.painter().rect_filled(rect, CornerRadius::same(RADIUS), fill);
    }
    let color = if selected { FG } else { FG_MUTED };
    let font = if selected { theme::medium(13.0) } else { theme::regular(13.0) };
    ui.painter().text(rect.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, text, font, color);
    if badge > 0 {
        let galley = ui.painter().layout_no_wrap(badge.to_string(), theme::medium(11.0), ACCENT_TEXT);
        let pill = egui::Rect::from_center_size(
            rect.right_center() - vec2(10.0 + galley.size().x.max(10.0) / 2.0 + 4.0, 0.0),
            vec2(galley.size().x.max(10.0) + 10.0, 18.0),
        );
        ui.painter().rect_filled(pill, CornerRadius::same(9), ACCENT_SOFT);
        ui.painter().galley(pill.center() - galley.size() / 2.0, galley, ACCENT_TEXT);
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn self_update_card(app: &JarApp, ui: &mut Ui, out: &mut Vec<Action>) {
    if matches!(app.self_update, SelfUpdate::Idle) {
        return;
    }
    egui::Frame::new().fill(BG_PANEL).stroke(Stroke::new(1.0, LINE)).corner_radius(8).inner_margin(10).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.with_layout(Layout::top_down(Align::Min), |ui| card_contents(app, ui, out));
    });
}

fn card_contents(app: &JarApp, ui: &mut Ui, out: &mut Vec<Action>) {
    ui.spacing_mut().item_spacing.y = 6.0;
    match &app.self_update {
        SelfUpdate::Idle => {}
        SelfUpdate::Available(release) => {
            widgets::label(ui, format!("The Jar {} is out", release.version), theme::medium(12.5), FG);
            if selfupdate::can_update_itself() {
                if widgets::small_button(ui, "Update", Kind::Accent).clicked() {
                    out.push(Action::SelfUpdate);
                }
            } else if widgets::small_button(ui, "Download", Kind::Secondary).clicked() {
                out.push(Action::OpenUrl(release.page_url.clone()));
            }
        }
        SelfUpdate::Downloading { release, done, total, .. } => {
            widgets::label(ui, format!("Downloading {}", release.version), theme::medium(12.5), FG);
            let fraction = (*total > 0).then(|| *done as f32 / *total as f32);
            widgets::progress(ui, fraction, ui.available_width());
            if widgets::small_button(ui, "Cancel", Kind::Ghost).clicked() {
                out.push(Action::SelfCancel);
            }
        }
        SelfUpdate::Ready(release, _) => {
            widgets::label(ui, format!("{} is ready", release.version), theme::medium(12.5), FG);
            if widgets::small_button(ui, "Restart now", Kind::Primary).clicked() {
                out.push(Action::SelfRestart);
            }
        }
        SelfUpdate::Failed(_, err) => {
            ui.add(egui::Label::new(egui::RichText::new(err).font(theme::regular(12.0)).color(DANGER)).wrap());
            if widgets::small_button(ui, "Try again", Kind::Secondary).clicked() {
                out.push(Action::SelfUpdate);
            }
        }
    }
}
