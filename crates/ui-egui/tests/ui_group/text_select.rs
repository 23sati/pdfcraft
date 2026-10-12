//! Issue #295: quick clicks on text widen the selection, as in Acrobat: two clicks select a
//! word, three the line, four all the text on the page.

use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use pdfcraft_ui_egui::PdfCraftApp;

/// One 300×200 page with two lines of Helvetica text.
fn two_lines() -> Vec<u8> {
    text_pdf("BT /F1 14 Tf 20 150 Td (The quick brown fox) Tj 0 -24 Td (jumps over the dog) Tj ET")
}

fn text_pdf(content: &str) -> Vec<u8> {
    format!(
        "%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >> endobj
4 0 obj << /Length {} >> stream
{content}
endstream endobj
5 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >> endobj
trailer << /Root 1 0 R >>
%%EOF",
        content.len()
    )
    .into_bytes()
}

/// The page on screen and the pointer over the "u" of "quick". Frames are 1/60 s: a click takes
/// two frames (press, release), and egui counts a double-click only within 0.3 s.
fn harness(setup: impl FnOnce(&mut PdfCraftApp) + 'static) -> (Harness<'static, PdfCraftApp>, Pos2) {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).with_step_dt(1.0 / 60.0).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("lines.pdf", None, two_lines()).expect("opens");
        app.set_option("left", "closed").unwrap();
        app.set_option("author", "Tester").unwrap();
        setup(&mut app);
        app
    });
    for _ in 0..200 {
        h.run_steps(2);
        if !h.state().render_pending() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let at = h.state().views[0].glyph_screen_pos(0, 5).expect("text layer loaded");
    h.hover_at(at);
    h.run_steps(1);
    (h, at)
}

/// Press and release without moving or leaving the page.
fn click(h: &mut Harness<'static, PdfCraftApp>, pos: Pos2) {
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(1);
}

#[test]
fn quick_clicks_select_a_word_then_the_line_then_the_page() {
    let (mut h, at) = harness(|_| {});
    let page = "The quick brown fox\njumps over the dog";
    for (n, want) in [None, Some("quick"), Some("The quick brown fox"), Some(page), Some(page)].into_iter().enumerate() {
        click(&mut h, at);
        assert_eq!(h.state().views[0].selected_text().as_deref(), want, "after {} clicks", n + 1);
    }
    // A pause ends the run: the next double-click selects a word again.
    h.run_steps(60);
    click(&mut h, at);
    click(&mut h, at);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("quick"));
}

#[test]
fn quick_clicks_with_the_highlighter_mark_only_the_word() {
    let (mut h, at) = harness(|app| assert!(app.execute("comment.highlight")));
    for _ in 0..4 {
        click(&mut h, at);
        // The double-click's highlight re-renders the page; give its text layer up to 8 frames
        // to come back (still well inside egui's 0.3 s multi-click window at 1/60 s a frame), so
        // a loaded machine doesn't make the later clicks land on a page with no text yet.
        for _ in 0..8 {
            if !h.state().render_pending() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            h.run_steps(1);
        }
    }
    h.run_steps(3);
    // Under load the mark can land a few frames later; wait for it (egui's clock is fixed per
    // frame, so waiting can't turn the clicks into a different run).
    for _ in 0..200 {
        let app = h.state();
        if app.session.get(app.views[0].id).is_some_and(|d| !d.info.annotations.is_empty()) && !app.render_pending() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.run_steps(1);
    }
    let app = h.state();
    let doc = app.session.get(app.views[0].id).unwrap();
    let marks: Vec<(&str, usize)> = doc.info.annotations.iter().map(|a| (a.subtype.as_str(), a.quads.len())).collect();
    assert_eq!(marks, [("Highlight", 1)], "one highlight, on the double-clicked word");
    assert!(app.views[0].selected_text().is_none());
}

/// Press at `from`, move to `to` and release there.
fn drag(h: &mut Harness<'static, PdfCraftApp>, from: Pos2, to: Pos2) {
    h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    for k in 1..=4 {
        h.hover_at(from + (to - from) * (k as f32 / 4.0));
        h.run_steps(1);
    }
    h.event(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
}

/// Click at `pos` with Shift held.
fn shift_click(h: &mut Harness<'static, PdfCraftApp>, pos: Pos2) {
    h.event(Event::ModifiersChanged(Modifiers::SHIFT));
    h.hover_at(pos);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::SHIFT });
    }
    h.run_steps(1);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(1);
}

/// Issue #527: ⇧-click extends the text selection to the click, keeping its anchor.
#[test]
fn shift_click_extends_the_selection_from_its_anchor() {
    let (mut h, _) = harness(|_| {});
    let glyph = |h: &Harness<'static, PdfCraftApp>, g: usize| h.state().views[0].glyph_screen_pos(0, g).expect("glyph on screen");
    // Drag over "quick" (glyphs 4..=8).
    let (q, k) = (glyph(&h, 4), glyph(&h, 8));
    drag(&mut h, q, k);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("quick"));
    h.run_steps(60);
    // ⇧-click on the "n" of "brown": the selection runs from the anchor to it.
    let n = glyph(&h, 14);
    shift_click(&mut h, n);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("quick brown"));
    h.run_steps(60);
    // ⇧-click before the anchor: the anchor stays, the selection now runs back to the click.
    let t = glyph(&h, 0);
    shift_click(&mut h, t);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("The q"));
    h.run_steps(60);
    // A plain click still clears it.
    click(&mut h, n);
    assert_eq!(h.state().views[0].selected_text(), None);
    h.run_steps(60);
    // ⇧-click with nothing selected selects nothing.
    shift_click(&mut h, n);
    assert_eq!(h.state().views[0].selected_text(), None);
}

/// Issue #1024: multi-click selection triggers immediately on pointer press, before release.
#[test]
fn multiclick_selects_on_pointer_press_before_release() {
    let (mut h, at) = harness(|_| {});
    // First click: press and release.
    click(&mut h, at);
    // Second click: press and HOLD (do not release).
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("quick"), "word highlights on press");
    // Release the second click: still selected.
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("quick"), "word stays selected on release");

    // Third click: press and HOLD.
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("The quick brown fox"), "line highlights on press");
    // Release third click.
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("The quick brown fox"), "line stays selected on release");
}

// Issue #1225: page-text Copy belongs in both menus and observes PDF permissions.
fn copied(h: &Harness<'static, PdfCraftApp>, want: &str) -> bool {
    h.output().platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::CopyText(t) if t == want))
}

#[test]
fn selected_text_copy_edit_and_context_menus() {
    use egui_kittest::kittest::Queryable;
    let (mut h, at) = harness(|_| {});
    click(&mut h, at);
    click(&mut h, at);
    h.state_mut().quick_tool = pdfcraft_ui_egui::QuickTool::Hand;
    h.run_steps(2); // Hide floating Copy so this test can only reach menu entries.
    h.get_by_label("Menu").click();
    h.run_steps(2);
    h.get_by_label("Edit ⏵").hover();
    h.run_steps(3);
    h.get_by_label_contains("Copy").click();
    let mut did_copy = false;
    for _ in 0..4 {
        h.step();
        did_copy |= copied(&h, "quick");
    }
    assert!(did_copy, "Edit Copy emits the selected text");
    h.state_mut().quick_tool = pdfcraft_ui_egui::QuickTool::Hand;
    h.run_steps(2); // Hide the floating bar: the remaining Copy must be in the context menu.
    h.event(Event::PointerMoved(at));
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos: at, button: PointerButton::Secondary, pressed, modifiers: Modifiers::NONE });
        h.run_steps(1);
    }
    h.run_steps(2);
    h.get_by_label_contains("Copy").click();
    let mut did_copy = false;
    for _ in 0..4 {
        h.step();
        did_copy |= copied(&h, "quick");
    }
    assert!(did_copy, "context Copy emits the selected text");
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("quick"));
}

#[test]
fn selected_text_copy_command_requires_selection() {
    let (mut h, at) = harness(|_| {});
    assert!(!h.state_mut().execute("edit.copy"));
    click(&mut h, at);
    click(&mut h, at);
    assert!(h.state_mut().execute("edit.copy"));
    h.step();
    assert!(copied(&h, "quick"));
    let mut app = PdfCraftApp::new();
    assert!(!app.execute("edit.copy"));
}

#[test]
fn selected_text_copy_floating_button_honours_permissions() {
    use egui_kittest::kittest::Queryable;
    for password in ["pw", "owner"] {
        let mut doc = pdfcraft_cos::Document::open(std::sync::Arc::new(two_lines())).unwrap();
        doc.set_encryption(&pdfcraft_cos::NewEncryption {
            algorithm: pdfcraft_cos::Algorithm::Aes256,
            user_password: "pw",
            owner_password: "owner",
            permissions: -1 ^ 0b1_0000,
            encrypt_metadata: true,
            seed: [4; 32],
        })
        .unwrap();
        let bytes = pdfcraft_cos::write_full(&doc, &Default::default()).unwrap();
        let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
            let mut app = PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            app.open_bytes("no-copy.pdf", None, bytes).unwrap();
            app.submit_password(Some(password.into()));
            app.set_option("left", "closed").unwrap();
            app
        });
        let mut loaded = false;
        for _ in 0..200 {
            h.run_steps(2);
            if h.state_mut().views[0].select_all() {
                loaded = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(loaded);
        h.run_steps(2);
        h.get_by_label("Copy").click();
        let mut did_copy = false;
        for _ in 0..4 {
            h.step();
            did_copy |= copied(&h, "The quick brown fox\njumps over the dog");
        }
        assert_eq!(did_copy, password == "owner", "floating Copy with {password} password");
        assert_eq!(h.state_mut().execute("edit.copy"), password == "owner");
    }
}

#[test]
fn selected_text_copy_preserves_unicode_and_page_clipboard() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.system_clipboard = false;
        app.open_bytes("unicode.pdf", None, text_pdf(r"BT /F1 14 Tf 20 150 Td (caf\351) Tj ET")).unwrap();
        app
    });
    let mut loaded = false;
    for _ in 0..200 {
        h.run_steps(2);
        if h.state_mut().views[0].select_all() {
            loaded = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(loaded);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("café"));
    assert!(h.state_mut().execute("edit.copy"));
    h.step();
    assert!(copied(&h, "café"));
    assert!(h.state().page_clipboard.is_none());
    assert!(h.state_mut().execute("page.organize"));
    h.run_steps(2);
    assert!(!h.state_mut().execute("edit.copy"), "Organize retains page clipboard ownership");
    assert!(h.state_mut().execute("page.copy"));
    assert_eq!(h.state().page_clipboard.as_ref().map(|c| c.pages.clone()), Some(vec![0]));
}

#[test]
fn selected_text_copy_keeps_focused_input_clipboard_ownership() {
    let (mut h, at) = harness(|_| {});
    click(&mut h, at);
    click(&mut h, at);
    h.state_mut().views[0].open_find();
    h.state_mut().views[0].find.as_mut().unwrap().query = "widget café".into();
    h.run_steps(3);
    h.ctx.memory_mut(|m| m.request_focus(egui::Id::new("find-input")));
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    h.event(Event::Copy);
    let mut widget_copy = false;
    let mut page_copy = false;
    for _ in 0..4 {
        h.step();
        widget_copy |= copied(&h, "widget café");
        page_copy |= copied(&h, "quick");
    }
    assert!(widget_copy, "the focused text input copies its selected text");
    assert!(!page_copy, "page text must not also consume the Copy event");
}
