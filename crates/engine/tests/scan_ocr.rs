use pdfcraft_engine::{
    Edit, Session,
    export::Exporter,
    ocr::{self, OcrSettings, PlacedWord},
};

fn render(session: &Session, id: pdfcraft_engine::DocId) -> Vec<u8> {
    Exporter::new(session.get(id).unwrap()).png(0, 150.0).unwrap()
}
fn text(session: &Session, id: pdfcraft_engine::DocId) -> String {
    Exporter::new(session.get(id).unwrap()).text(0).unwrap()
}
fn scan(sentence: &str, rotation: u16, skew: f64) -> (Session, pdfcraft_engine::DocId) {
    let mut session = Session::new();
    let japanese = sentence.chars().any(|c| ('\u{3000}'..='\u{9fff}').contains(&c));
    let pdf = session.create_from_text("Scan fixture", if japanese { "Placeholder" } else { sentence }).unwrap();
    let doc = session.open("text.pdf", None, pdf, None).unwrap();
    if japanese {
        session.apply(doc, Edit::EditTextLine { page: 0, line: 0, text: sentence.into() }).unwrap();
    }
    let image = image::load_from_memory(&render(&session, doc)).unwrap().to_rgb8();
    let image = pdfcraft_ocr::scan::Correction { rotation, deskew: skew }.apply(&image);
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let pdf = session.create_from_images(&[("scan.png".into(), png.into_inner())]).unwrap();
    let id = session.open("scan.pdf", None, pdf, None).unwrap();
    (session, id)
}

#[test]
fn unicode_ocr_survives_save_search_and_undo_without_changing_pixels() {
    let (mut s, id) = scan("Invisible text layer", 0, 0.0);
    let before = render(&s, id);
    let words =
        vec![PlacedWord { text: "Grüße 日本語 français español 😀".into(), origin: [50.0, 50.0], across: [300.0, 0.0], up: [0.0, 20.0] }];
    s.apply(id, Edit::AddOcrText { page: 0, words }).unwrap();
    assert_eq!(before, render(&s, id));
    let found = text(&s, id);
    assert!(found.contains("Grüße 日本語 français español 😀"), "{found:?}");
    let saved = s.save_bytes(id).unwrap();
    let reopened = s.open("saved.pdf", None, saved, None).unwrap();
    assert!(text(&s, reopened).contains("日本語"));
    s.undo(id).unwrap();
    assert!(text(&s, id).trim().is_empty());
    s.redo(id).unwrap();
    assert!(text(&s, id).contains("日本語"));
}

#[test]
fn annotated_scan_refuses_deskew_atomically() {
    let (mut s, id) = scan("Keep comments attached", 0, 0.0);
    s.apply(
        id,
        Edit::AddAnnotation(pdfcraft_engine::NewAnnotation {
            page: 0,
            shape: pdfcraft_engine::Shape::Rectangle { rect: [20.0, 20.0, 100.0, 50.0] },
            style: Default::default(),
            contents: "Keep this comment".into(),
            author: "Test".into(),
        }),
    )
    .unwrap();
    let before = s.get(id).unwrap().bytes.clone();
    let result = s.apply(
        id,
        Edit::Batch {
            label: "Correct scans".into(),
            edits: vec![Edit::RotatePages { pages: vec![0], degrees: 90 }, Edit::DeskewPage { page: 0, degrees: 3.0 }],
        },
    );
    assert!(result.unwrap_err().to_string().contains("annotations"));
    assert_eq!(s.get(id).unwrap().bytes, before);
    for degrees in [f64::NAN, f64::INFINITY, 13.0] {
        assert!(s.apply(id, Edit::DeskewPage { page: 0, degrees }).is_err());
    }
}

#[test]
fn scan_correction_handles_all_quarter_turns_and_stale_results() {
    if !ocr::available() {
        eprintln!("skipped: run cargo xtask models");
        return;
    }
    let sentence = "Document orientation and searchable text\nThe quick brown fox jumps over the lazy dog.\nEvery page should be upright and easy to read.\nPlease keep this document for your records.\nA useful example with several lines of text.";
    for rotation in [0, 90, 180, 270] {
        let (mut s, id) = scan(sentence, rotation, 0.0);
        let before = s.get(id).unwrap().bytes.clone();
        let result = s.recognize_text(id, &[], OcrSettings { deskew: false, ..Default::default() }).unwrap();
        assert_eq!(result[0].correction.rotation, (360 - rotation) % 360, "{result:?}");
        assert!(text(&s, id).to_lowercase().contains("orientation"), "{:?}", result);
        s.undo(id).unwrap();
        assert_eq!(s.get(id).unwrap().bytes, before);
        s.redo(id).unwrap();
        let job = s.ocr_job(id, &[], OcrSettings { skip_text_pages: false, ..Default::default() }).unwrap();
        let found = job.run(&ocr::engine().unwrap(), |_, _| true);
        s.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
        assert!(s.apply_ocr(id, &found).unwrap_err().to_string().contains("changed"));
    }
}

#[test]
fn scan_deskew_without_ocr_persists_and_is_undoable() {
    if !ocr::available() {
        return;
    }
    let sentence = (0..18).map(|_| "Straight lines make this scanned document easier to read.\n").collect::<String>();
    for angle in [-4.0, 3.0] {
        let (mut s, id) = scan(&sentence, 0, angle);
        let before = s.get(id).unwrap().bytes.clone();
        let result = s.recognize_text(id, &[], OcrSettings { auto_rotate: false, recognize: false, ..Default::default() }).unwrap();
        assert!((result[0].correction.deskew + angle).abs() < 0.3, "{result:?}");
        assert!(text(&s, id).trim().is_empty());
        let saved = s.save_bytes(id).unwrap();
        assert!(saved.starts_with(before.as_ref()));
        let after = s.open("after.pdf", None, saved, None).unwrap();
        let img = image::load_from_memory(&render(&s, after)).unwrap().to_rgb8();
        assert!(pdfcraft_ocr::scan::deskew(&img).abs() < 0.2);
        s.undo(id).unwrap();
        assert_eq!(s.get(id).unwrap().bytes, before);
    }
}

#[test]
fn scan_recognizes_accents_in_requested_languages() {
    if !ocr::available() {
        return;
    }
    for (language, sentence, expected) in [
        ("de", "Grüße aus Österreich und schöne Grüße für alle.", "Österreich"),
        ("fr", "Une journée à Paris avec un café français.", "journée"),
        ("es", "El español también tiene acentos y una mañana.", "español"),
    ] {
        let (mut s, id) = scan(sentence, 0, 0.0);
        let found =
            s.recognize_text(id, &[], OcrSettings { language: language.into(), auto_rotate: false, deskew: false, ..Default::default() }).unwrap();
        assert!(text(&s, id).contains(expected), "{language}: {found:?}; extracted {:?}", text(&s, id));
    }
}

#[test]
fn scan_recognizes_japanese_with_release_fonts() {
    if !ocr::available() || pdfcraft_fonts::CRAFT_FONTS.is_empty() {
        eprintln!("skipped: models and CRAFT_FONTS_DIR required");
        return;
    }
    let (mut s, id) = scan("日本語の文字を認識します。\n今日は良い天気です。", 0, 0.0);
    let found = s.recognize_text(id, &[], OcrSettings { language: "ja".into(), auto_rotate: false, deskew: false, ..Default::default() }).unwrap();
    assert!(text(&s, id).contains("日本語"), "{found:?}; extracted {:?}", text(&s, id));
}

#[test]
fn scan_rotation_and_deskew_keep_searchable_text_aligned() {
    if !ocr::available() {
        return;
    }
    let sentence = (0..12).map(|_| "Straight searchable lines stay aligned with the scan.\n").collect::<String>();
    let (mut s, id) = scan(&sentence, 90, 4.0);
    let found = s.recognize_text(id, &[], OcrSettings::default()).unwrap();
    assert_eq!(found[0].correction.rotation, 270);
    assert!((found[0].correction.deskew + 4.0).abs() < 0.3, "{found:?}");
    assert!(text(&s, id).to_lowercase().contains("searchable"));
    let bytes = s.save_bytes(id).unwrap();
    let reopened = s.open("result.pdf", None, bytes, None).unwrap();
    let img = image::load_from_memory(&render(&s, reopened)).unwrap().to_rgb8();
    assert!(pdfcraft_ocr::scan::deskew(&img).abs() < 0.2);
    // The extracted words must lie along horizontal lines after both PDF transformations.
    let doc = s.get(reopened).unwrap();
    let mut renderer = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), doc.export_source().config);
    let text =
        renderer.render(pdfcraft_render::RenderRequest { page: 0, kind: pdfcraft_render::RequestKind::Text, ..Default::default() }).text.unwrap();
    let range = text.find("searchable").into_iter().next().unwrap();
    for glyph in &text.glyphs[range] {
        assert!(glyph.direction[0] > 0.99 && glyph.direction[1].abs() < 0.01, "{glyph:?}");
    }
}
