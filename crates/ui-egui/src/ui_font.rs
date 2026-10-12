//! A user-selected interface face, read locally at runtime and never embedded in a PDF.

use std::path::PathBuf;
use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};
use serde::{Deserialize, Serialize};

use crate::theme;

pub const CUSTOM_FONT: &str = "user-interface-font";
const MAX_BYTES: u64 = 128 << 20;
const MAX_FACES: u32 = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    pub path: PathBuf,
    #[serde(default)]
    pub index: u32,
}

#[derive(Default)]
pub struct UiFont {
    choice: Option<Choice>,
    data: Option<Arc<FontData>>,
    faces: Vec<(u32, String)>,
    error: Option<String>,
    pub(crate) dirty: bool,
}

impl UiFont {
    pub fn choice(&self) -> Option<&Choice> {
        self.choice.as_ref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Validate before replacing the current face, so a failed choice keeps a readable UI.
    pub fn select(&mut self, choice: Option<Choice>) -> Result<(), String> {
        let loaded = choice.as_ref().map(load).transpose()?;
        self.choice = choice;
        self.data = loaded.as_ref().map(|(data, _)| data.clone());
        self.faces = loaded.map_or_else(Vec::new, |(_, faces)| faces);
        self.error = None;
        self.dirty = true;
        Ok(())
    }

    pub fn restore(&mut self, value: &serde_json::Value) {
        // Older settings have no field. Invalid or missing files safely retain default fonts.
        let choice = serde_json::from_value::<Option<Choice>>(value.clone()).unwrap_or_default();
        if let Err(error) = self.select(choice) {
            let _ = self.select(None);
            self.error = Some(error);
        }
    }

    pub fn definitions(&self, cjk: theme::CjkPreference) -> FontDefinitions {
        let mut defs = theme::installed_font_definitions(cjk);
        let Some(data) = &self.data else { return defs };
        // Publication opt-out covers the selected local face as well as automatic fallbacks.
        if std::env::var_os("PDFCRAFT_SYSTEM_FONTS").is_some_and(|v| v == "0") {
            return defs;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(primary) = crate::system_fonts::metrics(&data.font, data.index) {
            // Give proportional UI families their own fallback copies. Monospace remains the
            // app's code font. Missing Chinese/Latin glyphs still align with the selected face.
            let mut aligned = std::collections::BTreeMap::new();
            for stack in defs.families.iter().filter(|(family, _)| **family != FontFamily::Monospace).map(|(_, stack)| stack) {
                for name in stack {
                    if let Some(face) = defs.font_data.get(name)
                        && let Some(m) = crate::system_fonts::metrics(&face.font, face.index)
                    {
                        aligned.entry(name.clone()).or_insert_with(|| {
                            let mut face = (**face).clone();
                            // Built-in emoji faces have a visual scale below 1.0.
                            let scale = face.tweak.scale;
                            if scale.is_finite() && scale > 0.0 {
                                face.tweak.y_offset_factor = ((primary.0 - m.0 * scale) + 0.5 * (m.1 * scale - primary.1)) / scale;
                            }
                            Arc::new(face)
                        });
                    }
                }
            }
            for (name, face) in aligned {
                let replacement = format!("user-aligned-{name}");
                for (family, stack) in &mut defs.families {
                    if *family != FontFamily::Monospace {
                        for n in stack.iter_mut().filter(|n| **n == name) {
                            *n = replacement.clone();
                        }
                    }
                }
                defs.font_data.insert(replacement, face);
            }
        }
        defs.font_data.insert(CUSTOM_FONT.into(), data.clone());
        for (family, stack) in &mut defs.families {
            if *family != FontFamily::Monospace {
                stack.insert(0, CUSTOM_FONT.into());
            }
        }
        defs
    }

    pub(crate) fn install(&mut self, ctx: &egui::Context, cjk: theme::CjkPreference) {
        ctx.set_fonts(self.definitions(cjk));
        self.dirty = false;
    }
}

type Loaded = (Arc<FontData>, Vec<(u32, String)>);

#[cfg(not(target_arch = "wasm32"))]
fn load(choice: &Choice) -> Result<Loaded, String> {
    use skrifa::{FontRef, MetadataProvider, string::StringId};
    use std::io::Read as _;

    let fail = || tl!("This font file cannot be used. Choose a readable TTF, OTF or TTC file.").to_owned();
    if !choice.path.is_absolute() || choice.path.as_os_str().len() > 4096 || choice.index >= MAX_FACES {
        return Err(fail());
    }
    let meta = std::fs::metadata(&choice.path).map_err(|_| fail())?;
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return Err(fail());
    }
    let file = std::fs::File::open(&choice.path).map_err(|_| fail())?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|_| fail())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(fail());
    }
    let mut faces = Vec::new();
    for index in 0..MAX_FACES {
        let Ok(font) = FontRef::from_index(&bytes, index) else { break };
        let name = font.localized_strings(StringId::FULL_NAME).english_or_first().map(|n| n.to_string());
        faces.push((index, name.unwrap_or_else(|| format!("#{index}"))));
    }
    let font = FontRef::from_index(&bytes, choice.index).map_err(|_| fail())?;
    if crate::system_fonts::metrics(&bytes, choice.index).is_none() {
        return Err(fail());
    }
    if !"ABCabc0123".chars().all(|c| font.charmap().map(c).is_some()) {
        return Err(tl!("The font must include Latin letters and numbers for interface controls.").into());
    }
    let mut data = FontData::from_owned(bytes);
    data.index = choice.index;
    Ok((Arc::new(data), faces))
}

#[cfg(target_arch = "wasm32")]
fn load(_choice: &Choice) -> Result<Loaded, String> {
    Err("Local interface fonts are available in the desktop app.".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn preferences(ui: &mut egui::Ui, app: &mut crate::PdfCraftApp, t: &theme::Tokens) {
    ui.label(egui::RichText::new(tl!("Interface font")).font(theme::semibold(13.0)));
    ui.horizontal_wrapped(|ui| {
        let name = app.ui_font.choice().and_then(|c| c.path.file_name()).map(|n| n.to_string_lossy().into_owned());
        ui.label(name.as_deref().unwrap_or(tl!("Default")));
        if ui.button(tl!("Choose font…")).clicked() {
            let mut dialog = rfd::AsyncFileDialog::new().set_title(tl!("Choose font…")).add_filter(tl!("Fonts"), &["ttf", "otf", "ttc", "otc"]);
            if let Some(parent) = app.ui_font.choice().and_then(|c| c.path.parent()) {
                dialog = dialog.set_directory(parent);
            } else if cfg!(target_os = "macos") {
                dialog = dialog.set_directory("/System/Library/Fonts");
            }
            app.ask_one(crate::pickers::Ask::File(dialog), None, |app, path| {
                if let Err(error) = app.ui_font.select(Some(Choice { path, index: 0 })) {
                    app.ui_font.error = Some(error);
                }
            });
        }
        if ui.add_enabled(app.ui_font.choice().is_some() || app.ui_font.error().is_some(), egui::Button::new(tl!("Use default font"))).clicked() {
            let _ = app.ui_font.select(None);
        }
    });
    if let Some(choice) = app.ui_font.choice().cloned() {
        let mut index = choice.index;
        if app.ui_font.faces.len() > 1 {
            let selected = app.ui_font.faces.iter().find(|(i, _)| *i == index).map(|(_, name)| name.as_str()).unwrap_or("");
            egui::ComboBox::from_id_salt("interface-font-face").selected_text(selected).show_ui(ui, |ui| {
                for (i, name) in &app.ui_font.faces {
                    ui.selectable_value(&mut index, *i, name);
                }
            });
            if index != choice.index
                && let Err(error) = app.ui_font.select(Some(Choice { index, ..choice.clone() }))
            {
                app.ui_font.error = Some(error);
            }
        }
        ui.add(egui::Label::new(egui::RichText::new(choice.path.to_string_lossy()).small().color(t.text_muted)).wrap());
    }
    ui.label(
        egui::RichText::new(tl!("Applied immediately and saved for next time. Keep the font file in its current location."))
            .small()
            .color(t.text_muted),
    );
    ui.label("PdfCraft ABC abc 0123 · 中文字体预览");
    if let Some(error) = app.ui_font.error() {
        ui.colored_label(egui::Color32::from_rgb(0xD1, 0x3B, 0x3B), error);
    }
    ui.add_space(8.0);
}
