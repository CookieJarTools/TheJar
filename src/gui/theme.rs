//! The bakery look: dark chocolate surfaces, vanilla text, caramel for actions, strawberry
//! icing for anything new. Fredoka for headings, Inter for everything you read.

use std::sync::Arc;

use egui::epaint::text::VariationCoords;
use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, FontTweak, Shadow, Stroke, TextStyle,
    Visuals, vec2,
};

pub const BG: Color32 = Color32::from_rgb(0x18, 0x11, 0x0c);
pub const BG_SOFT: Color32 = Color32::from_rgb(0x1e, 0x16, 0x10);
pub const BG_CARD: Color32 = Color32::from_rgb(0x26, 0x1c, 0x15);
pub const BG_HOVER: Color32 = Color32::from_rgb(0x2e, 0x23, 0x1a);
pub const BG_ACTIVE: Color32 = Color32::from_rgb(0x38, 0x2a, 0x1f);
pub const BG_PRESSED: Color32 = Color32::from_rgb(0x40, 0x30, 0x24);

pub const LINE: Color32 = Color32::from_rgb(0x33, 0x27, 0x1d);
pub const LINE_STRONG: Color32 = Color32::from_rgb(0x4a, 0x39, 0x2b);

pub const FG: Color32 = Color32::from_rgb(0xf8, 0xec, 0xdc);
pub const FG_MUTED: Color32 = Color32::from_rgb(0xc4, 0xad, 0x94);
pub const FG_SUBTLE: Color32 = Color32::from_rgb(0x93, 0x80, 0x6c);
pub const FG_FAINT: Color32 = Color32::from_rgb(0x6e, 0x5d, 0x4d);

pub const ACCENT: Color32 = Color32::from_rgb(0xea, 0xa5, 0x4f);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0xf2, 0xb5, 0x66);
pub const ACCENT_PRESSED: Color32 = Color32::from_rgb(0xd8, 0x92, 0x3d);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(0x4a, 0x33, 0x1b);
pub const ACCENT_SOFT_HOVER: Color32 = Color32::from_rgb(0x5a, 0x3f, 0x21);
pub const ACCENT_TEXT: Color32 = Color32::from_rgb(0xf5, 0xc4, 0x86);
/// Text on a caramel button.
pub const ON_ACCENT: Color32 = Color32::from_rgb(0x2b, 0x18, 0x08);

pub const ICING: Color32 = Color32::from_rgb(0xf3, 0x94, 0xb2);
pub const ICING_SOFT: Color32 = Color32::from_rgb(0x4d, 0x24, 0x33);
pub const MINT: Color32 = Color32::from_rgb(0x95, 0xd9, 0xb8);
pub const MINT_SOFT: Color32 = Color32::from_rgb(0x1f, 0x3a, 0x2f);
pub const WARN: Color32 = Color32::from_rgb(0xf2, 0xc9, 0x4c);
pub const DANGER: Color32 = Color32::from_rgb(0xf2, 0x74, 0x66);

pub const DOUGH: Color32 = Color32::from_rgb(0xe2, 0xb1, 0x70);
pub const DOUGH_RAW: Color32 = Color32::from_rgb(0xf0, 0xdc, 0xb4);
pub const BAKED: Color32 = Color32::from_rgb(0xc9, 0x8a, 0x45);
pub const CRUST: Color32 = Color32::from_rgb(0xb4, 0x72, 0x34);
pub const CHIP: Color32 = Color32::from_rgb(0x4a, 0x28, 0x14);

pub const RADIUS: u8 = 10;

const INTER: &[u8] = include_bytes!("../../assets/fonts/InterVariable.ttf");
const FREDOKA: &[u8] = include_bytes!("../../assets/fonts/Fredoka.ttf");
const MEDIUM: &str = "Inter Medium";
const SEMIBOLD: &str = "Inter SemiBold";
const DISPLAY: &str = "Fredoka";

pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// The rounded heading face.
pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(DISPLAY.into()))
}

pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, regular(12.0)),
            (TextStyle::Body, regular(13.0)),
            (TextStyle::Button, medium(13.0)),
            (TextStyle::Heading, display(20.0)),
            (TextStyle::Monospace, FontId::monospace(12.0)),
        ]
        .into();
        style.spacing.item_spacing = vec2(8.0, 6.0);
        style.spacing.button_padding = vec2(12.0, 6.0);
        style.spacing.interact_size.y = 28.0;
        style.visuals = visuals();
    });
}

fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let weighted = |bytes: &'static [u8], wght: f32| {
        Arc::new(
            FontData::from_static(bytes)
                .tweak(FontTweak { coords: VariationCoords::new([(b"wght", wght)]), ..Default::default() }),
        )
    };
    fonts.font_data.insert("Inter".to_owned(), weighted(INTER, 400.0));
    fonts.font_data.insert("Inter-Medium".to_owned(), weighted(INTER, 510.0));
    fonts.font_data.insert("Inter-SemiBold".to_owned(), weighted(INTER, 590.0));
    fonts.font_data.insert("Fredoka-SemiBold".to_owned(), weighted(FREDOKA, 600.0));

    let fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let family = |first: &str| std::iter::once(first.to_owned()).chain(fallbacks.iter().cloned()).collect();
    fonts.families.insert(FontFamily::Proportional, family("Inter"));
    fonts.families.insert(FontFamily::Name(MEDIUM.into()), family("Inter-Medium"));
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), family("Inter-SemiBold"));
    fonts.families.insert(FontFamily::Name(DISPLAY.into()), family("Fredoka-SemiBold"));
    fonts
}

fn visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = BG_CARD;
    v.window_stroke = Stroke::new(1.0, LINE_STRONG);
    v.window_corner_radius = CornerRadius::same(16);
    v.menu_corner_radius = CornerRadius::same(12);
    v.window_shadow = Shadow { offset: [0, 14], blur: 44, spread: 0, color: Color32::from_black_alpha(170) };
    v.popup_shadow = Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(150) };
    v.extreme_bg_color = BG_SOFT;
    v.faint_bg_color = BG_SOFT;
    v.code_bg_color = BG_ACTIVE;
    v.hyperlink_color = ACCENT_TEXT;
    v.warn_fg_color = WARN;
    v.error_fg_color = DANGER;
    v.selection.bg_fill = ACCENT_SOFT;
    v.selection.stroke = Stroke::new(1.0, ACCENT_TEXT);

    let radius = CornerRadius::same(RADIUS);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, FG);
    v.widgets.noninteractive.corner_radius = radius;
    for (state, fill) in [
        (&mut v.widgets.inactive, BG_HOVER),
        (&mut v.widgets.hovered, BG_ACTIVE),
        (&mut v.widgets.active, BG_PRESSED),
        (&mut v.widgets.open, BG_ACTIVE),
    ] {
        state.weak_bg_fill = fill;
        state.bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, LINE_STRONG);
        state.fg_stroke = Stroke::new(1.0, FG);
        state.corner_radius = radius;
        state.expansion = 0.0;
    }
    v
}
