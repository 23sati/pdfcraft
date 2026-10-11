//! The right-to-left interface (Hebrew, Arabic): the shell is mirrored and its text runs right to
//! left. Headless (egui_kittest + AccessKit), like `ui.rs`.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::PdfCraftApp;

/// A tiny PDF with two pages.
const FIXTURE: &[u8] = b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] >> endobj
4 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] >> endobj
trailer << /Root 1 0 R >>
%%EOF";

const WIDTH: f32 = 1400.0;

fn harness(language: &'static str, setup: impl FnOnce(&mut PdfCraftApp) + 'static) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(WIDTH, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", language).unwrap();
        setup(&mut app);
        app
    });
    // Fonts install on frame 1 and apply on frame 2; the mirrored layout starts with the frame
    // after the language is set.
    h.run_steps(5);
    h
}

#[test]
fn hebrew_mirrors_the_shell() {
    let h = harness("he", |app| {
        app.open_bytes("a.pdf", None, FIXTURE.to_vec()).expect("fixture opens");
    });
    let home = h.get_by_label_contains("דף הבית").rect();
    let open = h.get_by_label_contains("פתיחה").rect();
    let tab = h.get_by_label("a.pdf").rect();
    let discord = h.get_by_label("Discord").rect();
    // The tab strip runs right to left: Home, the tab, Open, and the controls at the far left.
    assert!(home.center().x > tab.center().x && tab.center().x > open.center().x, "{home:?} {tab:?} {open:?}");
    assert!(discord.center().x < open.center().x && discord.min.x >= 0.0, "{discord:?}");
    assert!(home.max.x > WIDTH - 60.0, "Home hugs the right edge: {home:?}");
    // The panel rail sits on the left.
    let bookmarks = h.get_by_label_contains("סימניות").rect();
    assert!(bookmarks.center().x < 100.0, "{bookmarks:?}");
}

#[test]
fn english_keeps_the_left_to_right_shell() {
    let h = harness("en", |app| {
        app.open_bytes("a.pdf", None, FIXTURE.to_vec()).expect("fixture opens");
    });
    let home = h.get_by_label("Home").rect();
    let tab = h.get_by_label("a.pdf").rect();
    assert!(home.center().x < tab.center().x, "{home:?} {tab:?}");
    assert!(h.get_by_label("Bookmarks").rect().center().x > WIDTH - 100.0);
}

#[test]
fn hebrew_overflowing_tabs_scroll_the_mirrored_way() {
    let mut h = harness("he", |app| {
        for i in 0..20 {
            app.open_bytes(&format!("document-{i:02}.pdf"), None, FIXTURE.to_vec()).unwrap();
        }
    });
    let visible = |r: egui::Rect| r.center().x > 150.0 && r.center().x < WIDTH;
    let last = h.get_by_label("document-19.pdf").rect();
    assert!(visible(last), "the newly opened active tab is in view: {last:?}");
    h.state_mut().active = Some(0);
    h.run_steps(6);
    let first = h.get_by_label("document-00.pdf").rect();
    assert!(visible(first), "the first tab is brought into view: {first:?}");
    // The first tab is right of the later ones.
    let second = h.get_by_label("document-01.pdf").rect();
    assert!(first.center().x > second.center().x, "{first:?} {second:?}");
    h.get_by_label("document-00.pdf").click();
    h.run_steps(3);
    assert_eq!(h.state().active, Some(0));
}
