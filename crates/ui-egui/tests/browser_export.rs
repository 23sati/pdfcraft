//! Browser API/recovery exports use the same engine and retain unsaved-work protection.
use pdfcraft_ui_egui::PdfCraftApp;

#[test]
fn export_keeps_dirty_tabs_and_active_document() {
    let mut app = PdfCraftApp::new();
    app.open_bytes("first.txt", None, b"First document".to_vec()).unwrap();
    app.open_bytes("second.txt", None, b"Second document".to_vec()).unwrap();
    let active = app.active;
    let (name, bytes) = app.export_pdf(0).unwrap();
    assert!(name.contains("first"));
    assert!(bytes.starts_with(b"%PDF-"));
    assert_eq!(app.active, active);
    assert!(app.first_dirty().is_some(), "export is not a host persistence acknowledgement");
    let mut reopened = PdfCraftApp::new();
    reopened.open_bytes(&name, None, bytes).unwrap();
    assert_eq!(reopened.session.docs()[0].info.pages.len(), 1);
    assert!(app.export_pdf(99).is_err());
    assert_eq!(app.views.len(), 2);
}
