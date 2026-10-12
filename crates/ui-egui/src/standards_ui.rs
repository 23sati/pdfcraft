//! Standards ▸ PDF/A and PDF/X verification and conversion, plus Preflight inspection.

use egui::{Align, Layout};
use pdfcraft_engine::pdfa::{Issue, Level};
use pdfcraft_engine::pdfx::PdfxLevel;
use pdfcraft_engine::profile::{PreflightResult, builtin_profiles};

use crate::theme::{self, Tokens};
use crate::{PdfCraftApp, widgets};

/// The dialog's state: the chosen level and the last result.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfaState {
    pub level: Level,
    pub issues: Option<Vec<Issue>>,
    pub fixed: Vec<String>,
}

impl Default for PdfaState {
    fn default() -> Self {
        PdfaState { level: Level::A2b, issues: None, fixed: Vec::new() }
    }
}

/// The PDF/X dialog's state: the chosen level and the last result.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfxState {
    pub level: PdfxLevel,
    pub issues: Option<Vec<Issue>>,
    pub fixed: Vec<String>,
}

impl Default for PdfxState {
    fn default() -> Self {
        PdfxState { level: PdfxLevel::X1a2001, issues: None, fixed: Vec::new() }
    }
}

/// The Preflight dialog's state: the chosen profile and the last result.
#[derive(Clone, Debug, PartialEq)]
pub struct PreflightState {
    pub selected_profile: String,
    pub result: Option<PreflightResult>,
}

impl Default for PreflightState {
    fn default() -> Self {
        PreflightState { selected_profile: "prepress-sheetfed-cmyk".to_string(), result: None }
    }
}

impl PdfCraftApp {
    pub fn pdfa_verify(&mut self) {
        let Some((_, id)) = self.active_ids() else { return };
        let level = self.pdfa.level;
        self.pdfa.issues = self.session.get(id).map(|d| d.pdfa_verify(level));
        self.pdfa.fixed.clear();
    }

    pub fn pdfa_convert(&mut self) {
        let level = self.pdfa.level;
        if self.apply_edit(pdfcraft_engine::Edit::ConvertPdfA { level }) {
            self.pdfa_verify();
            let left = self.pdfa.issues.as_ref().map_or(0, Vec::len);
            self.notify(if left == 0 {
                crate::i18n::fmt(tl!("The document now conforms to {level}; save it to keep the changes"), &[("level", level.label())])
            } else if left == 1 {
                crate::i18n::fmt(tl!("Fixed what could be fixed; 1 problem remains"), &[])
            } else {
                crate::i18n::fmt(tl!("Fixed what could be fixed; {n} problems remain"), &[("n", &left.to_string())])
            });
        }
    }

    pub fn pdfx_verify(&mut self) {
        let Some((_, id)) = self.active_ids() else { return };
        let level = self.pdfx.level;
        self.pdfx.issues = self.session.get(id).map(|d| d.pdfx_verify(level));
        self.pdfx.fixed.clear();
    }

    pub fn pdfx_convert(&mut self) {
        let level = self.pdfx.level;
        if self.apply_edit(pdfcraft_engine::Edit::ConvertPdfX { level }) {
            self.pdfx_verify();
            let left = self.pdfx.issues.as_ref().map_or(0, Vec::len);
            self.notify(if left == 0 {
                format!("The document now conforms to {}; save it to keep the changes", level.label())
            } else if left == 1 {
                "Fixed what could be fixed; 1 problem remains".to_string()
            } else {
                format!("Fixed what could be fixed; {left} problems remain")
            });
        }
    }

    pub fn preflight_run(&mut self) {
        let Some((_, id)) = self.active_ids() else { return };
        let profile_id = self.preflight.selected_profile.clone();
        self.preflight.result = self.session.get(id).and_then(|d| d.preflight_run(&profile_id));
    }

    pub fn preflight_fix(&mut self) {
        let profile_id = self.preflight.selected_profile.clone();
        if self.apply_edit(pdfcraft_engine::Edit::PreflightFix { profile_id: profile_id.clone() }) {
            self.preflight_run();
            let left = self.preflight.result.as_ref().map_or(0, |r| r.errors.len());
            self.notify(if left == 0 {
                "Preflight fixups completed: document passed all rules".to_string()
            } else {
                format!("Preflight fixups completed; {left} errors remain")
            });
        }
    }
}

/// Returns `true` to close.
pub(crate) fn body(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) -> bool {
    ui.label(egui::RichText::new("PDF/A").font(theme::semibold(18.0)));
    ui.add_space(6.0);
    let declared = app.active_ids().and_then(|(_, id)| app.session.get(id)).map(|d| d.standards()).unwrap_or_default();
    let shown = declared.pdfa.as_ref().map_or(tl!("none").to_string(), |(p, c)| format!("PDF/A-{p}{}", c.to_lowercase()));
    ui.label(crate::i18n::fmt(tl!("Declared conformance: {shown}"), &[("shown", &shown)]));
    if !declared.output_intents.is_empty() {
        ui.label(
            egui::RichText::new(crate::i18n::fmt(tl!("Output intent: {list}"), &[("list", &declared.output_intents.join(", "))])).color(t.text_muted),
        );
    }
    ui.add_space(6.0);
    let before = app.pdfa.level;
    ui.horizontal(|ui| {
        ui.label(tl!("Conformance level:"));
        egui::ComboBox::from_id_salt("pdfa-level").selected_text(app.pdfa.level.label()).show_ui(ui, |ui| {
            for l in [Level::A2b, Level::A3b] {
                ui.selectable_value(&mut app.pdfa.level, l, l.label());
            }
        });
    });
    if app.pdfa.level != before {
        app.pdfa.issues = None;
    }
    ui.add_space(6.0);
    egui::Frame::new().fill(t.hover).corner_radius(egui::CornerRadius::same(6)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            ui.set_min_height(120.0);
            match &app.pdfa.issues {
                None => {
                    ui.label(egui::RichText::new(tl!("Verify to see what the document needs.")).color(t.text_muted));
                }
                Some(v) if v.is_empty() => {
                    ui.label(crate::i18n::fmt(tl!("No problems found: the document conforms to {level}."), &[("level", app.pdfa.level.label())]));
                }
                Some(v) => {
                    for i in v {
                        let page = i.page.map(|p| crate::i18n::fmt(tl!(" (page {p})"), &[("p", &(p + 1).to_string())])).unwrap_or_default();
                        let fix = if i.fixable { String::new() } else { tl!("  · not fixable here").to_string() };
                        // The verifier hands the wording over whole, with the document's own name
                        // apart from it, so the problem reads in this language and the name as it is.
                        ui.label(format!("{}{page}", i.said(&crate::i18n::words)));
                        ui.label(egui::RichText::new(format!("ISO 19005 {}{fix}", i.clause)).small().color(t.text_muted));
                    }
                }
            }
        });
    });
    ui.add_space(10.0);
    let mut close = false;
    let mut action = None;
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::pill_button(ui, tl!("Save as PDF/A"), true).clicked() {
                action = Some(true);
            }
            if widgets::pill_button(ui, tl!("Verify"), false).clicked() {
                action = Some(false);
            }
            if widgets::pill_button(ui, tl!("Close"), false).clicked() {
                close = true;
            }
        });
    });
    match action {
        Some(true) => app.pdfa_convert(),
        Some(false) => app.pdfa_verify(),
        None => {}
    }
    close
}

/// Returns `true` to close.
pub(crate) fn pdfx_body(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) -> bool {
    ui.label(egui::RichText::new("PDF/X").font(theme::semibold(18.0)));
    ui.add_space(6.0);
    let declared = app.active_ids().and_then(|(_, id)| app.session.get(id)).map(|d| d.standards()).unwrap_or_default();
    let shown = declared.pdfx.as_deref().unwrap_or_else(|| tl!("none"));
    ui.label(crate::i18n::fmt(tl!("Declared conformance: {shown}"), &[("shown", shown)]));
    if !declared.output_intents.is_empty() {
        ui.label(
            egui::RichText::new(crate::i18n::fmt(tl!("Output intent: {list}"), &[("list", &declared.output_intents.join(", "))])).color(t.text_muted),
        );
    }
    ui.add_space(6.0);
    let before = app.pdfx.level;
    ui.horizontal(|ui| {
        ui.label(tl!("Conformance level:"));
        egui::ComboBox::from_id_salt("pdfx-level").selected_text(app.pdfx.level.label()).show_ui(ui, |ui| {
            for l in [PdfxLevel::X1a2001, PdfxLevel::X32002, PdfxLevel::X4] {
                ui.selectable_value(&mut app.pdfx.level, l, l.label());
            }
        });
    });
    if app.pdfx.level != before {
        app.pdfx.issues = None;
    }
    ui.add_space(6.0);
    egui::Frame::new().fill(t.hover).corner_radius(egui::CornerRadius::same(6)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            ui.set_min_height(120.0);
            match &app.pdfx.issues {
                None => {
                    ui.label(egui::RichText::new("Verify to see what the document needs.").color(t.text_muted));
                }
                Some(v) if v.is_empty() => {
                    ui.label(format!("No problems found: the document conforms to {}.", app.pdfx.level.label()));
                }
                Some(v) => {
                    for i in v {
                        let page = i.page.map(|p| format!(" (page {})", p + 1)).unwrap_or_default();
                        let fix = if i.fixable { "" } else { "  · not fixable here" };
                        ui.label(format!("{}{page}", i.said(&crate::i18n::words)));
                        ui.label(egui::RichText::new(format!("ISO 15930 {}{fix}", i.clause)).small().color(t.text_muted));
                    }
                }
            }
        });
    });
    ui.add_space(10.0);
    let mut close = false;
    let mut action = None;
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::pill_button(ui, "Save as PDF/X", true).clicked() {
                action = Some(true);
            }
            if widgets::pill_button(ui, tl!("Verify"), false).clicked() {
                action = Some(false);
            }
            if widgets::pill_button(ui, tl!("Close"), false).clicked() {
                close = true;
            }
        });
    });
    match action {
        Some(true) => app.pdfx_convert(),
        Some(false) => app.pdfx_verify(),
        None => {}
    }
    close
}

/// Preflight dialog UI. Returns `true` to close.
pub(crate) fn preflight_body(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) -> bool {
    ui.label(egui::RichText::new("Preflight").font(theme::semibold(18.0)));
    ui.add_space(6.0);

    let profiles = builtin_profiles();
    let current_profile = profiles.iter().find(|p| p.id == app.preflight.selected_profile).cloned().unwrap_or_else(|| profiles[0].clone());

    ui.horizontal(|ui| {
        ui.label("Profile:");
        egui::ComboBox::from_id_salt("preflight-profile").selected_text(&current_profile.name).show_ui(ui, |ui| {
            for p in &profiles {
                if ui.selectable_label(app.preflight.selected_profile == p.id, &p.name).clicked() {
                    app.preflight.selected_profile = p.id.clone();
                    app.preflight.result = None;
                }
            }
        });
    });
    ui.label(egui::RichText::new(&current_profile.description).small().color(t.text_muted));
    ui.add_space(6.0);

    egui::Frame::new().fill(t.hover).corner_radius(egui::CornerRadius::same(6)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
            ui.set_min_height(140.0);
            match &app.preflight.result {
                None => {
                    ui.label(egui::RichText::new("Analyze to inspect document against the selected profile.").color(t.text_muted));
                }
                Some(res) => {
                    let status_color = if res.passed { egui::Color32::from_rgb(0, 180, 80) } else { egui::Color32::from_rgb(220, 50, 50) };
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(if res.passed { "STATUS: PASSED" } else { "STATUS: FAILED" }).strong().color(status_color));
                        ui.label(format!(
                            "({} errors, {} warnings, {} info; {} fixable)",
                            res.errors.len(),
                            res.warnings.len(),
                            res.info.len(),
                            res.fixable_count
                        ));
                    });
                    ui.add_space(4.0);

                    if res.total_issues == 0 {
                        ui.label("No problems or warnings found for this profile.");
                    } else {
                        for err in &res.errors {
                            let page_str = err.page.map(|p| format!(" (page {})", p + 1)).unwrap_or_default();
                            let fix_str = if err.fixable { " [Fixable]" } else { "" };
                            ui.label(
                                egui::RichText::new(format!("[Error: {}] {}{page_str}{fix_str}", err.category.label(), err.message))
                                    .color(egui::Color32::from_rgb(220, 50, 50)),
                            );
                        }
                        for warn in &res.warnings {
                            let page_str = warn.page.map(|p| format!(" (page {})", p + 1)).unwrap_or_default();
                            let fix_str = if warn.fixable { " [Fixable]" } else { "" };
                            ui.label(
                                egui::RichText::new(format!("[Warning: {}] {}{page_str}{fix_str}", warn.category.label(), warn.message))
                                    .color(egui::Color32::from_rgb(200, 140, 20)),
                            );
                        }
                        for info in &res.info {
                            let page_str = info.page.map(|p| format!(" (page {})", p + 1)).unwrap_or_default();
                            ui.label(
                                egui::RichText::new(format!("[Info: {}] {}{page_str}", info.category.label(), info.message)).color(t.text_muted),
                            );
                        }
                    }
                }
            }
        });
    });

    ui.add_space(10.0);
    let mut close = false;
    let mut action = None;
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::pill_button(ui, "Analyze and Fix", true).clicked() {
                action = Some(2);
            }
            if widgets::pill_button(ui, "Analyze", false).clicked() {
                action = Some(1);
            }
            if widgets::pill_button(ui, tl!("Close"), false).clicked() {
                close = true;
            }
        });
    });
    match action {
        Some(1) => app.preflight_run(),
        Some(2) => app.preflight_fix(),
        _ => {}
    }
    close
}
