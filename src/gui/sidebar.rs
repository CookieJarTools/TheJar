use egui::{Align2, CornerRadius, CursorIcon, Painter, Pos2, Rect, Response, Sense, Stroke, Ui, pos2, vec2};

use super::theme::{self, *};
use super::widgets::{self, Kind};
use super::{Action, JarApp, SelfUpdate, View};
use crate::selfupdate;

#[derive(Clone, Copy)]
enum Glyph {
    Cookie,
    RollingPin,
}

pub fn show(app: &JarApp, ui: &mut Ui, out: &mut Vec<Action>) {
    // The footer is its own panel so it sizes to its contents at the bottom of the sidebar.
    egui::Panel::bottom("sidebar-footer").frame(egui::Frame::NONE).show_separator_line(false).show(ui, |ui| {
        self_update_card(app, ui, out);
        widgets::label(ui, format!("The Jar {}", selfupdate::current()), theme::regular(11.5), FG_FAINT);
    });

    ui.horizontal(|ui| {
        ui.add_space(2.0);
        let logo = egui::Image::from_bytes("bytes://thejar-logo.png", selfupdate::ICON_PNG);
        ui.add(logo.fit_to_exact_size(vec2(34.0, 34.0)));
        ui.add_space(2.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            widgets::label(ui, "The Jar", theme::display(20.0), FG);
            widgets::label(ui, "Freshly baked apps", theme::regular(11.5), FG_SUBTLE);
        });
    });
    ui.add_space(24.0);

    ui.spacing_mut().item_spacing.y = 4.0;
    let library = nav_item(ui, "Your jar", Glyph::Cookie, app.view == View::Library, app.update_count());
    if library.clicked() {
        out.push(Action::Show(View::Library));
    }
    if nav_item(ui, "Settings", Glyph::RollingPin, app.view == View::Settings, 0).clicked() {
        out.push(Action::Show(View::Settings));
    }
}

fn nav_item(ui: &mut Ui, text: &str, glyph: Glyph, selected: bool, badge: usize) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    if selected || response.hovered() {
        let fill = if selected { ACCENT_SOFT } else { BG_HOVER };
        ui.painter().rect_filled(rect, CornerRadius::same(RADIUS), fill);
    }
    let color = if selected { ACCENT_TEXT } else { FG_MUTED };
    paint_glyph(ui.painter(), glyph, rect.left_center() + vec2(20.0, 0.0), selected);
    let font = if selected { theme::medium(13.5) } else { theme::regular(13.5) };
    ui.painter().text(rect.left_center() + vec2(38.0, 0.0), Align2::LEFT_CENTER, text, font, color);

    if badge > 0 {
        let galley = ui.painter().layout_no_wrap(badge.to_string(), theme::semibold(11.0), ICING);
        let width = galley.size().x.max(8.0) + 12.0;
        let pill = Rect::from_center_size(rect.right_center() - vec2(10.0 + width / 2.0, 0.0), vec2(width, 20.0));
        ui.painter().rect_filled(pill, CornerRadius::same(10), ICING_SOFT);
        ui.painter().galley(pill.center() - galley.size() / 2.0, galley, ICING);
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn paint_glyph(painter: &Painter, glyph: Glyph, centre: Pos2, selected: bool) {
    let opacity = if selected { 1.0 } else { 0.7 };
    match glyph {
        Glyph::Cookie => widgets::paint_cookie(painter, centre, 8.0, opacity, None),
        Glyph::RollingPin => {
            let wood = DOUGH.gamma_multiply(opacity);
            let handle = CRUST.gamma_multiply(opacity);
            let barrel = Rect::from_center_size(centre, vec2(12.0, 7.0));
            painter.line_segment([centre - vec2(10.0, 0.0), centre + vec2(10.0, 0.0)], Stroke::new(2.5, handle));
            painter.rect_filled(barrel, CornerRadius::same(3), wood);
            painter.circle_filled(pos2(centre.x - 10.0, centre.y), 1.8, handle);
            painter.circle_filled(pos2(centre.x + 10.0, centre.y), 1.8, handle);
        }
    }
}

fn self_update_card(app: &JarApp, ui: &mut Ui, out: &mut Vec<Action>) {
    if matches!(app.self_update, SelfUpdate::Idle) {
        return;
    }
    egui::Frame::new().fill(BG_CARD).stroke(Stroke::new(1.0, LINE_STRONG)).corner_radius(14).inner_margin(12).show(
        ui,
        |ui| {
            ui.set_width(ui.available_width());
            card_contents(app, ui, out);
        },
    );
    ui.add_space(10.0);
}

fn card_contents(app: &JarApp, ui: &mut Ui, out: &mut Vec<Action>) {
    ui.spacing_mut().item_spacing.y = 6.0;
    match &app.self_update {
        SelfUpdate::Idle => {}
        SelfUpdate::Available(release) => {
            widgets::label(ui, "A fresh batch of The Jar", theme::display(14.0), FG);
            widgets::label(
                ui,
                format!("Version {} is ready to bake.", release.version),
                theme::regular(12.0),
                FG_MUTED,
            );
            if selfupdate::can_update_itself() {
                if widgets::small_button(ui, "Update", Kind::Primary).clicked() {
                    out.push(Action::SelfUpdate);
                }
            } else if widgets::small_button(ui, "Download", Kind::Secondary).clicked() {
                out.push(Action::OpenUrl(release.page_url.clone()));
            }
        }
        SelfUpdate::Downloading { release, done, total, .. } => {
            widgets::label(ui, format!("Baking {}", release.version), theme::display(14.0), FG);
            let fraction = (*total > 0).then(|| *done as f32 / *total as f32);
            widgets::progress(ui, fraction, ui.available_width());
            if widgets::small_button(ui, "Cancel", Kind::Ghost).clicked() {
                out.push(Action::SelfCancel);
            }
        }
        SelfUpdate::Ready(release, _) => {
            widgets::label(ui, format!("{} is out of the oven", release.version), theme::display(14.0), FG);
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
