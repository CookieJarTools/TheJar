use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Painter, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui,
    pos2, vec2,
};

use super::theme::{self, *};

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    /// Caramel: the thing to press.
    Primary,
    /// Softer caramel, for actions that matter but aren't urgent.
    Accent,
    Secondary,
    Ghost,
    Danger,
    /// The confirm button in a dialog that deletes something.
    Destructive,
}

/// What a badge is saying, which decides its colour.
#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Fresh,
    New,
    Plain,
    Warn,
}

pub fn button(ui: &mut Ui, text: &str, kind: Kind) -> Response {
    button_sized(ui, text, kind, 30.0)
}

pub fn small_button(ui: &mut Ui, text: &str, kind: Kind) -> Response {
    button_sized(ui, text, kind, 25.0)
}

fn button_sized(ui: &mut Ui, text: &str, kind: Kind, height: f32) -> Response {
    let small = height < 28.0;
    let font = theme::medium(if small { 12.0 } else { 13.0 });
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER);
    let padding = if small { 10.0 } else { 14.0 };
    let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + padding * 2.0, height), Sense::click());
    let enabled = ui.is_enabled();

    if ui.is_rect_visible(rect) {
        let hovered = enabled && response.hovered();
        let pressed = enabled && response.is_pointer_button_down_on();
        let (fill, stroke, text_color) = button_colors(kind, hovered, pressed);
        let fade = |c: Color32| if enabled { c } else { c.gamma_multiply(0.45) };
        let stroke = stroke.map_or(Stroke::NONE, |c| Stroke::new(1.0, fade(c)));
        let radius = CornerRadius::same(if small { 8 } else { RADIUS });
        ui.painter().rect(rect, radius, fade(fill), stroke, StrokeKind::Inside);
        // Caramel buttons get a lighter top edge, like a glaze catching the light.
        if matches!(kind, Kind::Primary) && enabled {
            let glaze = Rect::from_min_size(
                rect.min + vec2(radius.nw as f32, 1.0),
                vec2(rect.width() - 2.0 * radius.nw as f32, 1.0),
            );
            ui.painter().rect_filled(glaze, 0, Color32::from_white_alpha(70));
        }
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
        Kind::Primary => (pick(ACCENT, ACCENT_HOVER, ACCENT_PRESSED), None, ON_ACCENT),
        Kind::Accent => (pick(ACCENT_SOFT, ACCENT_SOFT_HOVER, ACCENT_SOFT), None, ACCENT_TEXT),
        Kind::Secondary => (pick(BG_HOVER, BG_ACTIVE, BG_PRESSED), Some(LINE_STRONG), FG),
        Kind::Ghost => (pick(Color32::TRANSPARENT, BG_HOVER, BG_ACTIVE), None, if hovered { FG } else { FG_MUTED }),
        Kind::Destructive => (pick(DANGER, DANGER.gamma_multiply(0.9), DANGER.gamma_multiply(0.8)), None, ON_ACCENT),
        Kind::Danger => {
            (pick(Color32::TRANSPARENT, DANGER.gamma_multiply(0.12), DANGER.gamma_multiply(0.2)), None, DANGER)
        }
    }
}

/// A pill such as "Fresh" or "New batch".
pub fn badge(ui: &mut Ui, text: &str, tone: Tone) -> Response {
    let (fg, bg) = match tone {
        Tone::Fresh => (MINT, MINT_SOFT),
        Tone::New => (ICING, ICING_SOFT),
        Tone::Plain => (FG_MUTED, BG_ACTIVE),
        Tone::Warn => (WARN, WARN.gamma_multiply(0.14)),
    };
    let galley = ui.painter().layout_no_wrap(text.to_owned(), theme::medium(11.5), fg);
    let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + 16.0, 20.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(10), bg);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, fg);
    response
}

/// A progress bar that bakes: pale dough at the start, golden brown when it's done, with a
/// chocolate chip every so often. `None` slides a lump of dough back and forth instead.
pub fn progress(ui: &mut Ui, fraction: Option<f32>, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 10.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(5), BG_ACTIVE);

    let (bar, colour) = match fraction {
        Some(f) => {
            let f = f.clamp(0.0, 1.0);
            let colour = lerp_colour(DOUGH_RAW, BAKED, f);
            (Rect::from_min_size(rect.min, vec2(rect.width() * f, rect.height())), colour)
        }
        None => {
            let t = (ui.input(|i| i.time) as f32 * 0.7).fract();
            let span = rect.width() * 0.3;
            let left = rect.left() - span + (rect.width() + span) * t;
            let bar = Rect::from_x_y_ranges(left.max(rect.left())..=(left + span).min(rect.right()), rect.y_range());
            (bar, DOUGH)
        }
    };
    if bar.width() > 1.0 {
        painter.rect_filled(bar, CornerRadius::same(5), colour);
        let mut x = rect.left() + 9.0;
        let mut up = true;
        while x < bar.right() - 4.0 {
            if x > bar.left() + 3.0 {
                let y = rect.center().y + if up { -1.5 } else { 1.5 };
                painter.circle_filled(pos2(x, y), 1.6, CHIP.gamma_multiply(0.85));
            }
            x += 17.0;
            up = !up;
        }
    }
    ui.ctx().request_repaint();
}

fn lerp_colour(from: Color32, to: Color32, t: f32) -> Color32 {
    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color32::from_rgb(mix(from.r(), to.r()), mix(from.g(), to.g()), mix(from.b(), to.b()))
}

pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(36.0, 20.0), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool(response.id, *on);
    let painter = ui.painter();
    let track = lerp_colour(BG_ACTIVE, ACCENT, t);
    let edge = if *on { Color32::TRANSPARENT } else { LINE_STRONG };
    painter.rect(rect, CornerRadius::same(10), track, Stroke::new(1.0, edge), StrokeKind::Inside);
    let x = egui::lerp((rect.left() + 10.0)..=(rect.right() - 10.0), t);
    painter.circle_filled(pos2(x, rect.center().y), 7.0, FG);
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// The app's icon, or a cookie-coloured tile with its initial while the icon hasn't loaded.
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
    ui.painter().rect_filled(rect.shrink(size * 0.04), CornerRadius::same((size * 0.24) as u8), ACCENT_SOFT);
    let initial = name.chars().next().unwrap_or('?').to_uppercase().to_string();
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, initial, theme::display(size * 0.46), ACCENT_TEXT);
}

/// A chocolate chip cookie. With `bite`, a bite is taken out of the top right, filled with
/// that colour, so it only looks right on a plain background of that colour.
pub fn paint_cookie(painter: &Painter, centre: Pos2, radius: f32, opacity: f32, bite: Option<Color32>) {
    const CHIPS: [(f32, f32, f32); 8] = [
        (-0.38, -0.30, 0.13),
        (0.18, -0.46, 0.10),
        (0.40, 0.08, 0.12),
        (-0.05, 0.02, 0.10),
        (-0.48, 0.28, 0.10),
        (0.08, 0.48, 0.12),
        (-0.18, -0.62, 0.07),
        (0.52, 0.42, 0.08),
    ];
    let shade = |c: Color32| c.gamma_multiply(opacity);
    painter.circle_filled(centre, radius, shade(CRUST));
    painter.circle_filled(centre + vec2(-radius * 0.04, -radius * 0.05), radius * 0.88, shade(DOUGH));
    for (x, y, size) in CHIPS {
        let at = centre + vec2(x, y) * radius;
        painter.circle_filled(at, radius * size, shade(CHIP));
        painter.circle_filled(
            at + vec2(-0.3, -0.35) * radius * size,
            radius * size * 0.35,
            shade(CHIP.gamma_multiply(1.6)),
        );
    }
    if let Some(background) = bite {
        for (x, y, size) in [(0.80, -0.60, 0.30), (0.98, -0.22, 0.24), (0.48, -0.92, 0.24)] {
            painter.circle_filled(centre + vec2(x, y) * radius, radius * size, background);
        }
    }
}

/// A few crumbs scattered around a point, for decoration.
pub fn paint_crumbs(painter: &Painter, centre: Pos2, spread: f32, opacity: f32) {
    for (x, y, size) in [(-0.9, 0.6, 2.6), (-0.6, 0.95, 1.8), (0.7, 0.85, 2.2), (1.0, 0.3, 1.6), (-1.1, -0.1, 1.5)] {
        painter.circle_filled(centre + vec2(x, y) * spread, size, DOUGH.gamma_multiply(opacity));
    }
}

/// Text that wraps onto at most `rows` lines and ends in "…" if there's more.
pub fn paint_wrapped(ui: &Ui, pos: Pos2, text: &str, font: FontId, color: Color32, width: f32, rows: usize) -> Rect {
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat::simple(font, color));
    job.wrap = TextWrapping {
        max_width: width.max(10.0),
        max_rows: rows,
        break_anywhere: false,
        overflow_character: Some('…'),
    };
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
    ui.add(egui::Label::new(egui::RichText::new(text).font(theme::regular(12.5)).color(FG_MUTED)).wrap())
}

/// A dotted rule, like the tear-off line on a recipe card.
pub fn dotted_rule(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 5.0), Sense::hover());
    let mut x = rect.left() + 2.0;
    while x < rect.right() {
        ui.painter().circle_filled(pos2(x, rect.center().y), 1.0, LINE_STRONG);
        x += 6.0;
    }
}
