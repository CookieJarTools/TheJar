use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{Align2, Color32, CornerRadius, CursorIcon, FontId, Rect, Response, Sense, Stroke, StrokeKind, Ui, vec2};

use super::theme::{self, *};

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    /// The one main action on screen.
    Primary,
    /// Updates: noticeable without competing with Primary.
    Accent,
    Secondary,
    Ghost,
    Danger,
    /// The confirm button in a dialog that deletes something.
    Destructive,
}

pub fn button(ui: &mut Ui, text: &str, kind: Kind) -> Response {
    button_sized(ui, text, kind, 28.0)
}

pub fn small_button(ui: &mut Ui, text: &str, kind: Kind) -> Response {
    button_sized(ui, text, kind, 24.0)
}

fn button_sized(ui: &mut Ui, text: &str, kind: Kind, height: f32) -> Response {
    let font = theme::medium(if height < 26.0 { 12.0 } else { 13.0 });
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER);
    let padding = if height < 26.0 { 9.0 } else { 12.0 };
    let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + padding * 2.0, height), Sense::click());
    let enabled = ui.is_enabled();

    if ui.is_rect_visible(rect) {
        let hovered = enabled && response.hovered();
        let pressed = enabled && response.is_pointer_button_down_on();
        let (fill, stroke, text_color) = button_colors(kind, hovered, pressed);
        let fade = |c: Color32| if enabled { c } else { c.gamma_multiply(0.45) };
        let stroke = stroke.map_or(Stroke::NONE, |c| Stroke::new(1.0, fade(c)));
        ui.painter().rect(rect, CornerRadius::same(RADIUS), fade(fill), stroke, StrokeKind::Inside);
        let pos = rect.center() - galley.size() / 2.0;
        ui.painter().galley(pos, galley, fade(text_color));
    }
    if enabled { response.on_hover_cursor(CursorIcon::PointingHand) } else { response }
}

fn button_colors(kind: Kind, hovered: bool, pressed: bool) -> (Color32, Option<Color32>, Color32) {
    let pick = |rest: Color32, hover: Color32, down: Color32| match (pressed, hovered) {
        (true, _) => down,
        (_, true) => hover,
        _ => rest,
    };
    match kind {
        Kind::Primary => (pick(ACCENT, ACCENT_HOVER, ACCENT_PRESSED), None, Color32::WHITE),
        Kind::Accent => (pick(ACCENT_SOFT, ACCENT_SOFT_HOVER, ACCENT_SOFT), None, ACCENT_TEXT),
        Kind::Secondary => (pick(BG_HOVER, BG_ACTIVE, BG_PRESSED), Some(LINE_STRONG), FG),
        Kind::Ghost => (pick(Color32::TRANSPARENT, BG_HOVER, BG_ACTIVE), None, if hovered { FG } else { FG_MUTED }),
        Kind::Destructive => {
            (pick(DANGER, DANGER.gamma_multiply(0.9), DANGER.gamma_multiply(0.8)), None, Color32::WHITE)
        }
        Kind::Danger => {
            (pick(Color32::TRANSPARENT, DANGER.gamma_multiply(0.12), DANGER.gamma_multiply(0.2)), None, DANGER)
        }
    }
}

/// A 4px track; `None` animates as "working on it" when there's no size to measure against.
pub fn progress(ui: &mut Ui, fraction: Option<f32>, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 4.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(2), BG_ACTIVE);
    let bar = match fraction {
        Some(f) => Rect::from_min_size(rect.min, vec2(rect.width() * f.clamp(0.0, 1.0), rect.height())),
        None => {
            let t = (ui.input(|i| i.time) as f32 * 0.8).fract();
            let span = rect.width() * 0.3;
            let left = rect.left() - span + (rect.width() + span) * t;
            Rect::from_x_y_ranges(left.max(rect.left())..=(left + span).min(rect.right()), rect.y_range())
        }
    };
    if bar.width() > 0.0 {
        painter.rect_filled(bar, CornerRadius::same(2), ACCENT);
    }
    ui.ctx().request_repaint();
}

pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(30.0, 18.0), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool(response.id, *on);
    let track = egui::lerp(egui::Rgba::from(BG_ACTIVE)..=egui::Rgba::from(ACCENT), t);
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(9),
        Color32::from(track),
        Stroke::new(1.0, if *on { Color32::TRANSPARENT } else { LINE_STRONG }),
        StrokeKind::Inside,
    );
    let x = egui::lerp((rect.left() + 9.0)..=(rect.right() - 9.0), t);
    painter.circle_filled(egui::pos2(x, rect.center().y), 6.0, FG);
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A small pill, such as "Pre-release".
pub fn chip(ui: &mut Ui, text: &str, fg: Color32, bg: Color32) -> Response {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), theme::medium(11.0), fg);
    let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + 12.0, 18.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(4), bg);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, fg);
    response
}

/// The app's icon, or a tile with its initial while the icon hasn't loaded.
pub fn app_icon(ui: &mut Ui, uri: Option<&str>, name: &str, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    paint_app_icon(ui, rect, uri, name);
    response
}

pub fn paint_app_icon(ui: &Ui, rect: Rect, uri: Option<&str>, name: &str) {
    if let Some(uri) = uri {
        egui::Image::new(uri.to_owned()).paint_at(ui, rect);
        return;
    }
    let size = rect.width();
    ui.painter().rect_filled(rect, CornerRadius::same((size * 0.24) as u8), BG_ACTIVE);
    let initial = name.chars().next().unwrap_or('?').to_uppercase().to_string();
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, initial, theme::semibold(size * 0.42), FG_MUTED);
}

/// Text on one line that ends in "…" rather than spilling past `width`.
pub fn paint_truncated(ui: &Ui, pos: egui::Pos2, text: &str, font: FontId, color: Color32, width: f32) -> Rect {
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat::simple(font, color));
    job.wrap = TextWrapping::truncate_at_width(width.max(10.0));
    let galley = ui.painter().layout_job(job);
    let rect = Rect::from_min_size(pos, galley.size());
    ui.painter().galley(pos, galley, color);
    rect
}

pub fn label(ui: &mut Ui, text: impl Into<String>, font: FontId, color: Color32) -> Response {
    ui.label(egui::RichText::new(text).font(font).color(color))
}

/// A wrapping paragraph in the muted colour.
pub fn note(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.add(egui::Label::new(egui::RichText::new(text).font(theme::regular(12.0)).color(FG_MUTED)).wrap())
}
