#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use egui::{Color32, FontFamily, FontId};
use egui_kittest::{Harness, kittest::Queryable};
use pdfcraft_ui_egui::{PdfCraftApp, control::ControlClient, theme, ui_font::CUSTOM_FONT};
use serde_json::{Value, json};

fn font_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts/Inter-Regular.ttf").canonicalize().unwrap()
}

#[test]
fn font_choice_persists_and_bad_files_preserve_the_current_choice() {
    let path = font_path();
    let mut app = PdfCraftApp::new();
    app.set_option("ui-font", path.to_str().unwrap()).unwrap();
    let saved = app.persist();
    let mut restored = PdfCraftApp::new();
    restored.restore(&saved);
    assert_eq!(restored.ui_font.choice(), app.ui_font.choice());
    assert!(restored.ui_font.error().is_none());
    assert!(app.set_option("ui-font", "/nonexistent/pdfcraft-font.ttf").is_err());
    assert!(app.set_option("ui-font", "relative.ttf").is_err());
    assert!(app.set_option("ui-font", path.parent().unwrap().to_str().unwrap()).is_err());
    assert!(app.set_option("ui-font-face", "63").is_err());
    assert_eq!(app.ui_font.choice().unwrap().path, path);
    assert_eq!(app.ui_font.choice().unwrap().index, 0);

    let mut missing: Value = serde_json::from_str(&saved).unwrap();
    missing["ui_font"]["path"] = json!("/nonexistent/pdfcraft-font.ttf");
    restored.restore(&missing.to_string());
    assert!(restored.ui_font.choice().is_none());
    assert!(restored.ui_font.error().is_some());
    restored.restore("{}");
    assert!(restored.ui_font.choice().is_none());
    assert!(restored.ui_font.error().is_none());
}

#[test]
fn corrupt_and_oversized_font_files_are_rejected() {
    let file = std::env::temp_dir().join(format!("pdfcraft-ui-font-{}.ttf", std::process::id()));
    let mut out = std::fs::OpenOptions::new().create_new(true).write(true).open(&file).unwrap();
    use std::io::Write as _;
    out.write_all(b"This is not a font.").unwrap();
    out.flush().unwrap();
    let mut app = PdfCraftApp::new();
    assert!(app.set_option("ui-font", file.to_str().unwrap()).is_err());
    out.set_len((128 << 20) + 1).unwrap();
    assert!(app.set_option("ui-font", file.to_str().unwrap()).is_err());
    drop(out);
    std::fs::remove_file(file).unwrap();
    assert!(app.ui_font.choice().is_none());
}

#[test]
fn publication_opt_out_disables_the_selected_font() {
    // Use a subprocess: changing process-wide environment in parallel tests is unsafe.
    const CHILD: &str = "PDFCRAFT_FONT_OPT_OUT_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "publication_opt_out_disables_the_selected_font", "--nocapture"])
            .env(CHILD, "1")
            .env("PDFCRAFT_SYSTEM_FONTS", "0")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        return;
    }
    let mut app = PdfCraftApp::new();
    app.set_option("ui-font", font_path().to_str().unwrap()).unwrap();
    let defs = app.ui_font.definitions(theme::CjkPreference::SimplifiedChinese);
    assert!(!defs.font_data.contains_key(CUSTOM_FONT));
    assert_eq!(defs.families[&FontFamily::Proportional][0], "Inter");
    assert!(app.ui_font.choice().is_some(), "publication mode must not erase the saved preference");
}

#[test]
fn selected_latin_face_keeps_chinese_fallback_and_monospace() {
    use egui::epaint::text::{Fonts, TextOptions};
    let mut app = PdfCraftApp::new();
    app.set_option("ui-font", font_path().with_file_name("JetBrainsMono-Regular.ttf").to_str().unwrap()).unwrap();
    let defs = app.ui_font.definitions(theme::CjkPreference::SimplifiedChinese);
    assert_eq!(defs.families[&FontFamily::Proportional][0], CUSTOM_FONT);
    assert_eq!(defs.families[&FontFamily::Monospace][0], "JetBrainsMono");
    let system = defs.font_data.contains_key(theme::SYSTEM_FALLBACK);
    let offset = defs.font_data.get("user-aligned-system-fallback").map(|d| d.tweak.y_offset_factor);
    let mut fonts = Fonts::new(TextOptions::default(), defs);
    let id = FontId::proportional(16.0);
    assert!(fonts.has_glyphs(&id, "PdfCraft 123"));
    if system {
        assert!(fonts.has_glyphs(&id, "简体中文欢迎"));
        let galley = fonts.with_pixels_per_point(2.0).layout_no_wrap("H中文".into(), id, Color32::BLACK);
        let glyphs = &galley.rows[0].glyphs;
        let baseline_gap = glyphs[1].pos.y + 16.0 * offset.unwrap() - glyphs[0].pos.y;
        assert!(baseline_gap.abs() < 0.6, "Chinese must align with the chosen Latin face: {baseline_gap}");
    }
    app.set_option("ui-font", "default").unwrap();
    assert!(!app.ui_font.definitions(theme::CjkPreference::SimplifiedChinese).font_data.contains_key(CUSTOM_FONT));
}

#[cfg(target_os = "macos")]
#[test]
fn chinese_collection_face_selection_is_saved() {
    let path = "/System/Library/Fonts/Hiragino Sans GB.ttc";
    if !std::path::Path::new(path).exists() {
        eprintln!("skipping collection check: Hiragino Sans GB is not installed");
        return;
    }
    let mut app = PdfCraftApp::new();
    app.set_option("ui-font", path).unwrap();
    app.set_option("ui-font-face", "1").unwrap();
    let mut restored = PdfCraftApp::new();
    restored.restore(&app.persist());
    assert_eq!(restored.ui_font.choice().unwrap().index, 1);
    assert_eq!(restored.ui_font.definitions(theme::CjkPreference::SimplifiedChinese).font_data[CUSTOM_FONT].index, 1);
}

fn call(h: &mut Harness<'static, PdfCraftApp>, client: &ControlClient, method: &str, params: Value) -> Value {
    let rx = client.send(method, params);
    for _ in 0..30 {
        h.step();
        if let Ok(reply) = rx.try_recv() {
            return reply.unwrap();
        }
    }
    panic!("no control reply for {method}");
}

#[test]
fn live_control_and_preferences_reset_font_without_restarting() {
    let slot = Arc::new(Mutex::new(None));
    let output = slot.clone();
    let mut h = Harness::builder().with_size(egui::vec2(1000.0, 900.0)).build_eframe(move |cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        *output.lock().unwrap() = Some(app.attach_control(&cc.egui_ctx));
        app
    });
    h.run_steps(4);
    let client = slot.lock().unwrap().take().unwrap();
    let path = font_path();
    call(&mut h, &client, "ui.set", json!({"key":"ui-font", "value":path.to_str().unwrap()}));
    h.run_steps(4);
    let state = call(&mut h, &client, "ui.state", json!({}));
    assert_eq!(state["ui_font"]["path"], json!(path));
    assert!(state["ui_font_error"].is_null());
    call(&mut h, &client, "ui.command", json!({"id":"app.preferences"}));
    h.run_steps(5);
    assert!(h.query_by_label("Interface font").is_some());
    assert!(h.query_by_label("Choose font…").is_some());
    h.get_by_label("Use default font").click();
    h.run_steps(5);
    assert!(call(&mut h, &client, "ui.state", json!({}))["ui_font"].is_null());
}
