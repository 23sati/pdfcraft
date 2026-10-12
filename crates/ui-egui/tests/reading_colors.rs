//! Preferences ▸ Reading colours: the app-wide preference (Acrobat's Replace Document Colors),
//! how it survives a restart, and how documents open with it.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_render::ReadingColors;
use pdfcraft_ui_egui::PdfCraftApp;

const PDF: &[u8] = b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >> endobj
4 0 obj << /Length 55 >> stream
BT /F1 14 Tf 20 150 Td (The quick brown fox) Tj ET
endstream endobj
5 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj
trailer << /Root 1 0 R >>
%%EOF";

/// The Reading colours combo of the preferences dialog: it shows the chosen mode as its value,
/// and no other combo in the dialog shows `Off` while the setting is off.
fn combo(h: &Harness<'static, PdfCraftApp>) {
    h.get_by(|n| n.role() == Role::ComboBox && n.value().is_some_and(|v| v == "Off")).click();
}

#[test]
fn reading_colours_preference_round_trips() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("doc.pdf", None, PDF.to_vec()).expect("opens");
        app.set_option("dialog", "preferences").unwrap();
        app
    });
    h.run_steps(4);
    assert_eq!(h.state().reading_colors, ReadingColors::default());
    // Switch the preset to white-on-black in the dialog's Reading colours combo. The open
    // document keeps repainting, so run a fixed number of steps and let the popup settle.
    combo(&h);
    h.run_steps(20);
    h.get_by_label("White on black").click();
    h.run_steps(2);
    assert_eq!(h.state().reading_colors, ReadingColors::WHITE_ON_BLACK);
    // The preference is app-wide: the already open document follows it too.
    let id = h.state().views[0].id;
    assert_eq!(h.state().session.reading_colors(id), Some(ReadingColors::WHITE_ON_BLACK));
    // Closing the dialog keeps everything.
    h.state_mut().set_option("dialog", "none").unwrap();
    h.run_steps(1);

    // The choice survives a restart through the same path the settings file uses.
    let mut restored = PdfCraftApp::new();
    restored.restore(&h.state().persist());
    assert_eq!(restored.reading_colors, ReadingColors::WHITE_ON_BLACK);
    // A settings file without the field keeps Off.
    let mut fresh = PdfCraftApp::new();
    fresh.restore(r#"{"theme": "dark"}"#);
    assert_eq!(fresh.reading_colors, ReadingColors::default());
    // A corrupt value fails to deserialize and falls back to Off instead of panicking.
    fresh.restore(r#"{"reading_colors": {"params": {"text": "zzz", "background": [0, 0, 0], "only_black": false, "line_art": true}}}"#);
    assert_eq!(fresh.reading_colors, ReadingColors::default());
    fresh.restore(r#"{"reading_colors": 7}"#);
    assert_eq!(fresh.reading_colors, ReadingColors::default());
}

#[test]
fn reading_colours_seed_on_open() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.reading_colors = ReadingColors::WHITE_ON_BLACK;
        app.open_bytes("doc.pdf", None, PDF.to_vec()).expect("opens");
        app
    });
    h.run_steps(4);
    // The document opened with the preference already applied.
    let id = h.state().views[0].id;
    assert_eq!(h.state().session.reading_colors(id), Some(ReadingColors::WHITE_ON_BLACK));
    // Changing the preference applies it to the open document as well.
    h.state_mut().set_reading_colors(ReadingColors::YELLOW_ON_BLACK);
    h.run_steps(1);
    assert_eq!(h.state().session.reading_colors(id), Some(ReadingColors::YELLOW_ON_BLACK));

    // Revisions and the signed version open through their own session calls, bypassing the
    // ordinary open: they seed the new document the same way. A second revision comes from an
    // incremental save.
    {
        let s = &mut h.state_mut().session;
        s.apply(id, pdfcraft_engine::Edit::SetInfo { key: "Title".into(), value: "Second".into() }).unwrap();
        let saved = s.save_bytes(id).unwrap();
        s.mark_saved(id, saved, None).unwrap();
    }
    h.state_mut().open_revision(1);
    let revision = h.state().views.last().unwrap().id;
    assert_ne!(revision, id);
    assert_eq!(h.state().session.reading_colors(revision), Some(ReadingColors::YELLOW_ON_BLACK));
    let len = h.state().session.get(id).unwrap().bytes.len();
    h.state_mut().view_signed_version(len);
    let signed = h.state().views.last().unwrap().id;
    assert_ne!(signed, id);
    assert_eq!(h.state().session.reading_colors(signed), Some(ReadingColors::YELLOW_ON_BLACK));

    // And back off, for every open document.
    h.state_mut().set_reading_colors(ReadingColors::default());
    h.run_steps(1);
    for view in &h.state().views {
        assert_eq!(h.state().session.reading_colors(view.id), Some(ReadingColors::default()));
    }
}
