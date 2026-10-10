//! Design tokens and egui style (plan/acrobat/02-ui-ux.md §1: neutral chrome, white panels,
//! one blue accent; Dark Gray keeps pages white). Every custom widget reads `Tokens::get`.

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, Visuals};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ThemeKind {
    #[default]
    Light,
    Dark,
}

/// The saved user choice, independent of the light/dark colours currently displayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ThemePreference {
    System,
    #[default]
    Light,
    Dark,
    #[serde(rename = "catppuccin")]
    Catppuccin,
    #[serde(rename = "solarized")]
    Solarized,
    #[serde(rename = "forest")]
    Forest,
    #[serde(rename = "ocean")]
    Ocean,
    #[serde(rename = "sunset")]
    Sunset,
    #[serde(rename = "paper-ink")]
    PaperInk,
    Custom(CustomThemeId),
}

mod library;
pub use library::*;

impl ThemePreference {
    pub fn resolve(self, system: Option<egui::Theme>, fallback: ThemeKind) -> ThemeKind {
        match self {
            Self::Light => ThemeKind::Light,
            Self::Dark | Self::Catppuccin | Self::Solarized | Self::Forest | Self::Ocean | Self::Sunset => ThemeKind::Dark,
            Self::PaperInk => ThemeKind::Light,
            Self::Custom(_) => fallback,
            Self::System => match system {
                Some(egui::Theme::Light) => ThemeKind::Light,
                Some(egui::Theme::Dark) => ThemeKind::Dark,
                None => fallback,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub kind: ThemeKind,
    /// Title/tab strip.
    pub titlebar: Color32,
    /// Mode bar, panels.
    pub chrome: Color32,
    pub panel: Color32,
    /// Document area behind pages.
    pub pasteboard: Color32,
    pub card: Color32,
    pub border: Color32,
    pub divider: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    pub icon: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub selected: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub accent_soft: Color32,
    pub field: Color32,
    pub badge_new: Color32,
    pub page_shadow: Color32,
    pub radius: u8,
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub info: Color32,
    pub on_accent: Color32,
    pub notice_bg: Color32,
    pub notice_text: Color32,
    pub focus: Color32,
    pub selection_text: Color32,
    pub window_shadow: Color32,
    pub popup_shadow: Color32,
    /// Preserve egui's historical defaults for the two original palettes.
    pub legacy: bool,
}

pub type Tokens = Palette;

impl Palette {
    pub fn for_kind(kind: ThemeKind) -> Self {
        match kind {
            ThemeKind::Light => Self {
                kind,
                titlebar: Color32::from_rgb(0xE9, 0xE9, 0xEB),
                chrome: Color32::from_rgb(0xFF, 0xFF, 0xFF),
                panel: Color32::from_rgb(0xFF, 0xFF, 0xFF),
                pasteboard: Color32::from_rgb(0xF1, 0xF1, 0xF3),
                card: Color32::from_rgb(0xFF, 0xFF, 0xFF),
                border: Color32::from_rgb(0xDA, 0xDA, 0xDE),
                divider: Color32::from_rgb(0xE8, 0xE8, 0xEB),
                text: Color32::from_rgb(0x22, 0x22, 0x26),
                text_muted: Color32::from_rgb(0x5E, 0x5E, 0x66),
                text_faint: Color32::from_rgb(0x6B, 0x6B, 0x73),
                icon: Color32::from_rgb(0x44, 0x44, 0x4B),
                hover: Color32::from_rgb(0xF0, 0xF0, 0xF3),
                pressed: Color32::from_rgb(0xE4, 0xE4, 0xE9),
                selected: Color32::from_rgb(0xE6, 0xEE, 0xFD),
                accent: Color32::from_rgb(0x1B, 0x63, 0xE0),
                accent_text: Color32::from_rgb(0x17, 0x55, 0xC4),
                accent_soft: Color32::from_rgb(0xE3, 0xEC, 0xFD),
                field: Color32::from_rgb(0xFF, 0xFF, 0xFF),
                badge_new: Color32::from_rgb(0x1B, 0x63, 0xE0),
                page_shadow: Color32::from_black_alpha(34),
                radius: 6,
                success: Color32::from_rgb(40, 140, 80),
                warning: Color32::from_rgb(200, 130, 20),
                error: Color32::from_rgb(200, 50, 50),
                info: Color32::from_rgb(40, 110, 200),
                on_accent: Color32::WHITE,
                notice_bg: Color32::from_rgb(42, 42, 47),
                notice_text: Color32::WHITE,
                focus: Color32::from_rgb(27, 99, 224),
                selection_text: Color32::from_rgb(27, 99, 224),
                window_shadow: Color32::from_black_alpha(38),
                popup_shadow: Color32::from_black_alpha(30),
                legacy: true,
            },
            ThemeKind::Dark => Self {
                kind,
                titlebar: Color32::from_rgb(0x1B, 0x1B, 0x1E),
                chrome: Color32::from_rgb(0x26, 0x26, 0x2A),
                panel: Color32::from_rgb(0x26, 0x26, 0x2A),
                pasteboard: Color32::from_rgb(0x19, 0x19, 0x1C),
                card: Color32::from_rgb(0x2E, 0x2E, 0x33),
                border: Color32::from_rgb(0x3C, 0x3C, 0x43),
                divider: Color32::from_rgb(0x33, 0x33, 0x39),
                text: Color32::from_rgb(0xEC, 0xEC, 0xEF),
                text_muted: Color32::from_rgb(0xAE, 0xAE, 0xB6),
                text_faint: Color32::from_rgb(0x97, 0x97, 0x9E),
                icon: Color32::from_rgb(0xD4, 0xD4, 0xDA),
                hover: Color32::from_rgb(0x34, 0x34, 0x3A),
                pressed: Color32::from_rgb(0x3E, 0x3E, 0x45),
                selected: Color32::from_rgb(0x23, 0x3A, 0x63),
                accent: Color32::from_rgb(0x4B, 0x8B, 0xF5),
                accent_text: Color32::from_rgb(0x8C, 0xB6, 0xFA),
                accent_soft: Color32::from_rgb(0x24, 0x36, 0x57),
                field: Color32::from_rgb(0x1E, 0x1E, 0x22),
                badge_new: Color32::from_rgb(0x3D, 0x7D, 0xEE),
                page_shadow: Color32::from_black_alpha(120),
                radius: 6,
                success: Color32::from_rgb(40, 140, 80),
                warning: Color32::from_rgb(200, 130, 20),
                error: Color32::from_rgb(200, 50, 50),
                info: Color32::from_rgb(40, 110, 200),
                on_accent: Color32::WHITE,
                notice_bg: Color32::from_rgb(236, 236, 239),
                notice_text: Color32::from_rgb(34, 34, 38),
                focus: Color32::from_rgb(75, 139, 245),
                selection_text: Color32::from_rgb(75, 139, 245),
                window_shadow: Color32::from_black_alpha(110),
                popup_shadow: Color32::from_black_alpha(90),
                legacy: true,
            },
        }
    }

    pub fn get(ctx: &egui::Context) -> Self {
        ctx.data(|d| d.get_temp::<Tokens>(egui::Id::new("pdfcraft-theme"))).unwrap_or_else(|| Self::for_kind(ThemeKind::Light))
    }

    /// Keep the original Light/Dark chrome exact while routing extended palettes
    /// through a semantic role. Document content must not call this adapter.
    pub fn compatible(&self, original: Color32, token: ColorToken) -> Color32 {
        if self.overrides(token) { token.get(self) } else { original }
    }

    pub fn overrides(&self, token: ColorToken) -> bool {
        !self.legacy || token.get(self) != token.get(&Self::for_kind(self.kind))
    }

    pub fn dark(&self) -> bool {
        self.kind == ThemeKind::Dark
    }
}

pub fn install_fonts(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions());
}

/// Install the interface fonts with the CJK fallback order for the UI language (Chinese
/// first for Simplified Chinese, Japanese first otherwise). Call it when the language
/// changes; the new faces take effect next frame.
pub fn install_fonts_for(ctx: &egui::Context, prefer_hans: bool) {
    ctx.set_fonts(installed_font_definitions(prefer_hans));
}

/// The name of the installed face [`installed_font_definitions`] may add after the embedded ones.
pub const SYSTEM_FALLBACK: &str = "system-fallback";

/// What [`install_fonts_for`] installs: [`font_definitions_for`], then, on desktop, one face
/// already installed on this machine as the last fallback of every family. It only draws
/// characters no embedded face has (an Arabic file name in a build without craft-fonts);
/// `PDFCRAFT_SYSTEM_FONTS=0` leaves it out.
pub fn installed_font_definitions(prefer_hans: bool) -> FontDefinitions {
    #[cfg_attr(target_arch = "wasm32", expect(unused_mut))]
    let mut fonts = font_definitions_for(prefer_hans);
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(data) = crate::system_fonts::fallback() {
        fonts.font_data.insert(SYSTEM_FALLBACK.to_owned(), data);
        for stack in fonts.families.values_mut() {
            stack.push(SYSTEM_FALLBACK.to_owned());
        }
    }
    fonts
}

/// The interface fonts: Inter (and JetBrains Mono for code) first, then egui's defaults, then
/// the CJK, Arabic and Telugu faces of the optional craft-fonts build input as the last fallback
/// in every family. Without craft-fonts there is no Japanese, Chinese, Arabic or Telugu face here.
pub fn font_definitions() -> FontDefinitions {
    font_definitions_for(false)
}

/// [`font_definitions`] with the CJK fallback order for the UI language. Simplified Chinese
/// must come first in Chinese mode: otherwise shared characters render in the Japanese face
/// while Simplified-only characters (e.g. U+6B22 欢) fall through to the Chinese face, and
/// the mixed vertical metrics sink them below the line.
pub fn font_definitions_for(prefer_hans: bool) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut fonts, "Inter", include_bytes!("../../../assets/fonts/Inter-Regular.ttf"));
    add(&mut fonts, "Inter-Medium", include_bytes!("../../../assets/fonts/Inter-Medium.ttf"));
    add(&mut fonts, "Inter-SemiBold", include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf"));
    add(&mut fonts, "JetBrainsMono", include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf"));
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "Inter".to_owned());
    fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "JetBrainsMono".to_owned());
    // The same static bytes pdfcraft-fonts uses for Japanese/Chinese text in PDFs: one copy, not two.
    for face in pdfcraft_fonts::ui_cjk_fonts(prefer_hans) {
        let name = face.name();
        // Japanese and Chinese faces have distinct family names, so no collision here.
        if !fonts.font_data.contains_key(&name) {
            add(&mut fonts, &name, face.bytes);
        }
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push(name.clone());
        }
    }
    // Arabic-script faces (file names, document titles) and Telugu faces (the Telugu interface,
    // file names) after the CJK ones; the ranges don't overlap.
    for face in pdfcraft_fonts::ui_arabic_fonts().into_iter().chain(pdfcraft_fonts::ui_telugu_fonts()) {
        let name = face.name();
        if !fonts.font_data.contains_key(&name) {
            add(&mut fonts, &name, face.bytes);
        }
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            let stack = fonts.families.entry(family).or_default();
            if !stack.contains(&name) {
                stack.push(name.clone());
            }
        }
    }
    let fallback: Vec<String> = fonts.families[&FontFamily::Proportional].clone();
    for (fam, primary) in [("medium", "Inter-Medium"), ("semibold", "Inter-SemiBold")] {
        let mut stack = vec![primary.to_owned()];
        stack.extend(fallback.iter().cloned());
        fonts.families.insert(FontFamily::Name(fam.into()), stack);
    }
    fonts
}

pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

pub fn apply(ctx: &egui::Context, kind: ThemeKind) {
    apply_palette(ctx, Tokens::for_kind(kind));
}

pub fn apply_palette(ctx: &egui::Context, t: Palette) {
    let kind = t.kind;
    // egui must use the same theme for popup/menu styles as our custom chrome.
    ctx.set_theme(if kind == ThemeKind::Dark { egui::Theme::Dark } else { egui::Theme::Light });
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("pdfcraft-theme"), t));
    let mut v = if t.dark() { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = t.panel;
    v.window_fill = t.card;
    v.window_stroke = Stroke::new(1.0, t.border);
    v.extreme_bg_color = t.field;
    v.faint_bg_color = t.hover;
    v.selection.bg_fill = t.accent_soft;
    v.selection.stroke = Stroke::new(1.0, t.accent);
    v.hyperlink_color = t.accent_text;
    v.override_text_color = Some(t.text);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_shadow = egui::Shadow { offset: [0, 8], blur: 28, spread: 0, color: t.window_shadow };
    v.popup_shadow = egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: t.popup_shadow };
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::same(t.radius);
        w.fg_stroke.color = t.text;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, t.divider);
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.inactive.bg_fill = t.field;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.weak_bg_fill = t.hover;
    v.widgets.hovered.bg_fill = t.hover;
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.widgets.active.weak_bg_fill = t.pressed;
    v.widgets.active.bg_fill = t.pressed;
    v.widgets.open.weak_bg_fill = t.hover;
    let original = Palette::for_kind(t.kind);
    let changed = |token: ColorToken| !t.legacy || token.get(&t) != token.get(&original);
    if changed(ColorToken::TextMuted) {
        v.weak_text_color = Some(t.text_muted);
        v.ime_composition.inactive_underline_stroke.color = t.text_muted;
    }
    if changed(ColorToken::Field) {
        v.code_bg_color = t.field;
    }
    if changed(ColorToken::Warning) {
        v.warn_fg_color = t.warning;
    }
    if changed(ColorToken::Error) {
        v.error_fg_color = t.error;
    }
    if changed(ColorToken::SelectionText) {
        v.selection.stroke.color = t.selection_text;
    }
    if changed(ColorToken::Focus) {
        v.text_cursor.stroke.color = t.focus;
        v.ime_composition.active_underline_stroke.color = t.focus;
        v.widgets.active.bg_stroke = Stroke::new(1.0, t.focus);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, t.focus);
        v.widgets.open.bg_stroke = Stroke::new(1.0, t.focus);
    }
    if changed(ColorToken::Panel) {
        v.widgets.noninteractive.bg_fill = t.panel;
        v.widgets.noninteractive.weak_bg_fill = t.panel;
    }
    if changed(ColorToken::Hover) {
        v.widgets.open.bg_fill = t.hover;
    }
    // Crisper text at 100–150 % scaling (#76): glyphs sit on whole pixels instead of being
    // rendered at quarter-pixel offsets, which egui notes blurs them. In the light theme, a mild
    // gamma darkens the antialiased edges of dark text (egui's default is linear, which reads thin
    // and grey next to the system's text); the dark theme keeps egui's own curve. At 200 % both
    // make little difference.
    v.text_options.subpixel_binning = false;
    if !t.dark() {
        v.text_options.color_transfer_function = egui::epaint::FontColorTransferFunction::Gamma(0.75);
    }
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.menu_margin = egui::Margin::same(6);
        s.spacing.scroll.bar_width = 8.0;
        s.spacing.scroll.floating = true;
        s.text_styles.insert(egui::TextStyle::Body, regular(13.0));
        s.text_styles.insert(egui::TextStyle::Button, regular(13.0));
        s.text_styles.insert(egui::TextStyle::Small, regular(11.0));
        s.text_styles.insert(egui::TextStyle::Heading, semibold(17.0));
        s.interaction.tooltip_delay = 0.35;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG 2 contrast ratio between two opaque colours.
    fn contrast(a: Color32, b: Color32) -> f32 {
        let lum = |c: Color32| {
            let lin = |v: u8| {
                let s = v as f32 / 255.0;
                if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
        };
        let (x, y) = (lum(a) + 0.05, lum(b) + 0.05);
        x.max(y) / x.min(y)
    }

    #[test]
    fn text_is_readable_on_every_surface() {
        // #76: the faintest text (hints, zoom level, captions) was 3.3:1 in the light theme.
        for kind in [ThemeKind::Light, ThemeKind::Dark] {
            let t = Tokens::for_kind(kind);
            for (name, fg) in [("text", t.text), ("text_muted", t.text_muted), ("text_faint", t.text_faint)] {
                for bg in [t.chrome, t.panel, t.card, t.pasteboard] {
                    let r = contrast(fg, bg);
                    assert!(r >= 4.5, "{kind:?} {name} on {bg:?}: {r:.2}:1, WCAG AA needs 4.5:1");
                }
            }
        }
    }
}
