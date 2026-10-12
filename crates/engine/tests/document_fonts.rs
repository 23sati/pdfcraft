//! Document font selection and contextual glyph save/reopen acceptance on synthetic PDFs.
mod support {
    pub mod font_fixture;
    pub mod source_subset;
}
use pdfcraft_cos::{Dict, Document, Object, SaveOptions, Stream};
use pdfcraft_engine::{BlockStyle, Edit, Session};
use pdfcraft_render::{PageRenderer, RenderRequest};
use std::sync::Arc;
fn document() -> Arc<Vec<u8>> {
    let mut d = Document::new_empty();
    let pages = d.add(Object::Dict(Dict::new()));
    let contents = d.add(Object::Stream(Stream::flate(Dict::new(), b"BT /F1 18 Tf 30 160 Td (Source) Tj ET")));
    let mut font = Dict::new();
    font.set(b"Type".to_vec(), Object::name("Font"));
    font.set(b"Subtype".to_vec(), Object::name("Type1"));
    font.set(b"BaseFont".to_vec(), Object::name("Helvetica"));
    let mut fonts = Dict::new();
    fonts.set(b"F1".to_vec(), Object::Dict(font));
    let mut res = Dict::new();
    res.set(b"Font".to_vec(), Object::Dict(fonts));
    let mut p = Dict::new();
    p.set(b"Type".to_vec(), Object::name("Page"));
    p.set(b"Parent".to_vec(), Object::Ref(pages));
    p.set(b"Contents".to_vec(), Object::Ref(contents));
    p.set(b"Resources".to_vec(), Object::Dict(res));
    p.set(b"MediaBox".to_vec(), Object::Array([0, 0, 300, 240].map(Object::Int).to_vec()));
    let page = d.add(Object::Dict(p));
    d.update_dict(pages, |p| {
        p.set(b"Type".to_vec(), Object::name("Pages"));
        p.set(b"Kids".to_vec(), Object::Array(vec![Object::Ref(page)]));
        p.set(b"Count".to_vec(), Object::Int(1));
    })
    .unwrap();
    let mut root = Dict::new();
    root.set(b"Type".to_vec(), Object::name("Catalog"));
    root.set(b"Pages".to_vec(), Object::Ref(pages));
    let root = d.add(Object::Dict(root));
    d.trailer_mut().set(b"Root".to_vec(), Object::Ref(root));
    Arc::new(pdfcraft_cos::write_full(&d, &SaveOptions::default()).unwrap())
}
fn pixels(bytes: Arc<Vec<u8>>) -> Vec<u8> {
    let mut renderer = PageRenderer::new(bytes, Default::default());
    let p = renderer.render(RenderRequest { page: 0, scale: 2.0, ..Default::default() });
    assert!(p.error.is_none(), "{:?}", p.error);
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    p.rgba.to_vec()
}
fn exercise(font: Vec<u8>, text: &str, expected_family: &str) {
    let mut s = Session::new();
    let id = s.open("source.pdf", None, document(), None).unwrap();
    let original = pixels(s.get(id).unwrap().display.clone());
    s.apply(id, Edit::RegisterTextFont { page: 0, bytes: Arc::new(font.clone()) }).unwrap();
    let resource = s.get(id).unwrap().text_editor_fonts(0).into_iter().find(|f| f.bytes == font).unwrap().resource;
    let style = BlockStyle { resource: Some(resource), width: Some(240.0), ..Default::default() };
    s.apply(id, Edit::EditTextBlock { page: 0, block: 0, text: text.into(), style: style.clone() }).unwrap();
    let edited = s.get(id).unwrap();
    let lines = edited.text_lines(0);
    assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<String>(), text);
    assert!(lines.iter().all(|l| l.base_font == expected_family));
    assert!((lines[0].baseline()[1] - 160.0).abs() < 0.01);
    let current = pixels(edited.display.clone());
    assert_ne!(current, original);
    let saved = s.save_bytes(id).unwrap();
    let re = s.open("saved.pdf", None, saved, None).unwrap();
    assert_eq!(pixels(s.get(re).unwrap().display.clone()), current);
    assert_eq!(s.get(re).unwrap().text_blocks(0)[0].editor_text, text);
    let twice = format!("{text} {text}");
    s.apply(re, Edit::EditTextBlock { page: 0, block: 0, text: twice.clone(), style: style.clone() }).unwrap();
    assert_eq!(s.get(re).unwrap().text_blocks(0)[0].editor_text, twice);
    let again = s.save_bytes(re).unwrap();
    let next = s.open("twice.pdf", None, again, None).unwrap();
    assert_eq!(s.get(next).unwrap().text_blocks(0)[0].editor_text, twice);
    assert_eq!(pixels(s.get(next).unwrap().display.clone()), pixels(s.get(re).unwrap().display.clone()));
    s.apply(re, Edit::EditTextBlock { page: 0, block: 0, text: String::new(), style }).unwrap();
    assert!(s.get(re).unwrap().text_lines(0).is_empty(), "every contextual glyph is deleted");
    let empty = pixels(s.get(re).unwrap().display.clone());
    assert!(empty.as_chunks::<4>().0.iter().all(|p| p[..3] == [255; 3]));
    s.undo(id).unwrap();
    assert_eq!(pixels(s.get(id).unwrap().display.clone()), original);
}
#[test]
fn opentype_cff_document_font_preserves_program_codes_and_rendering_after_reopen() {
    exercise(support::font_fixture::cff_font(), "A A", "RectFont-Regular");
}
#[test]
fn contextual_arabic_marks_ligatures_and_mixed_bidi_keep_logical_text_after_reopen() {
    if let Some(path) = std::env::var_os("PDFCRAFT_TEST_ARABIC_FONT") {
        exercise(std::fs::read(path).unwrap(), "سَلَام 123", "NotoSansArabic-Regular");
        return;
    }
    let Some(font) = pdfcraft_fonts::document_arabic_font() else {
        eprintln!("set CRAFT_FONTS_DIR to test full Arabic glyph programs");
        return;
    };
    if pdfcraft_fonts::embedding::validate(font.bytes).is_err() {
        eprintln!("set PDFCRAFT_TEST_ARABIC_FONT to an openly licensed static Noto Sans Arabic face");
        return;
    }
    exercise(font.bytes.to_vec(), "سَلَام 123", "NotoSansArabic-Regular");
}

#[test]
fn installed_matching_face_completes_subsets_atomically_and_reopens_without_font_file() {
    use pdfcraft_engine::text_fonts::Catalogue;
    let dir = std::env::temp_dir().join(format!("pdfcraft-auto-matching-font-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("arbitrary-name.ttf");
    std::fs::write(&path, include_bytes!("../../../assets/fonts/Inter-Regular.ttf")).unwrap();
    let catalogue = Arc::new(Catalogue::discover(std::slice::from_ref(&dir)));
    assert_eq!(catalogue.files().count(), 1);
    // Cache bytes once; edits and previews must survive deletion/replacement of the file.
    std::fs::remove_file(path).unwrap();
    let mut session = Session::new();
    let source = support::source_subset::source_subset();
    let id = session.open("source.pdf", None, source.clone(), None).unwrap();
    session.set_text_font_catalogue(catalogue);
    let doc = session.get(id).unwrap();
    assert!(!doc.dirty && doc.can_undo().is_none());
    assert_eq!(doc.bytes, source);
    assert!(doc.text_editor_fonts(0).iter().any(|f| f.resource.starts_with("PCApprovedInstalled")));
    assert!(!doc.text_editor_fonts(0).iter().any(|f| f.resource.starts_with("PCFull")));
    let anchor = doc.text_lines(0)[0].baseline();
    let source_font = doc.text_editor_fonts(0).into_iter().find(|f| f.resource == "F1").unwrap().bytes;
    let original_pixels = pixels(doc.display.clone());
    let style = BlockStyle { width: Some(230.0), ..Default::default() };
    let layout = doc.text_edit_layout(0, 0, "ABΩ", &style).unwrap();
    let again = doc.text_edit_layout(0, 0, "ABΩЖ", &style).unwrap();
    assert!(again.draft_rect.unwrap()[2] > layout.draft_rect.unwrap()[2]);
    assert_eq!(doc.bytes, source);
    assert!(!doc.dirty && doc.can_undo().is_none());
    session.apply(id, Edit::EditTextBlock { page: 0, block: 0, text: "ABΩ".into(), style: style.clone() }).unwrap();
    let doc = session.get(id).unwrap();
    assert_eq!(doc.text_blocks(0)[0].editor_text, "ABΩ");
    assert!(doc.text_lines(0).iter().all(|l| pdfcraft_fonts::embedding::face_key(&l.base_font) == "interregular"));
    assert_eq!(doc.text_lines(0)[0].baseline(), anchor);
    assert_eq!(doc.text_editor_fonts(0).into_iter().find(|f| f.resource == "F1").unwrap().bytes, source_font);
    let live = pixels(doc.display.clone());
    assert_ne!(live, original_pixels);
    let saved = session.save_bytes(id).unwrap();
    let mut portable = Session::new();
    assert!(!portable.has_text_font_catalogue());
    let rid = portable.open("saved.pdf", None, saved, None).unwrap();
    assert_eq!(pixels(portable.get(rid).unwrap().display.clone()), live);
    portable.apply(rid, Edit::EditTextBlock { page: 0, block: 0, text: "ABЖ".into(), style }).unwrap();
    assert_eq!(portable.get(rid).unwrap().text_blocks(0)[0].editor_text, "ABЖ");
    let twice = portable.save_bytes(rid).unwrap();
    let tid = portable.open("twice.pdf", None, twice, None).unwrap();
    assert_eq!(pixels(portable.get(rid).unwrap().display.clone()), pixels(portable.get(tid).unwrap().display.clone()));
    // Font completion and the text form one transaction, with no extra Load font history.
    session.undo(id).unwrap();
    assert_eq!(session.get(id).unwrap().bytes, source);
    assert!(session.get(id).unwrap().can_undo().is_none());
    session.redo(id).unwrap();
    assert_eq!(pixels(session.get(id).unwrap().display.clone()), live);
    let before = session.get(id).unwrap().bytes.clone();
    let generation = session.get(id).unwrap().edit_generation();
    assert!(
        session
            .apply(
                id,
                Edit::EditTextBlock {
                    page: 0,
                    block: 0,
                    text: "AB".into(),
                    style: BlockStyle {
                        ranges: vec![pdfcraft_edit::text::RangeStyle {
                            range: 1..4,
                            family: None,
                            resource: None,
                            size: Some(20.0),
                            color: None,
                            underline: None,
                            char_spacing: None,
                            scale: None
                        }],
                        ..Default::default()
                    }
                }
            )
            .is_err()
    );
    assert_eq!(session.get(id).unwrap().bytes, before);
    assert_eq!(session.get(id).unwrap().edit_generation(), generation);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn installed_catalogue_does_not_match_unrelated_or_wrong_style_faces() {
    use pdfcraft_engine::text_fonts::Catalogue;
    let dir = std::env::temp_dir().join(format!("pdfcraft-auto-unrelated-font-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("looks-like-Inter-Regular.ttf"), include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf")).unwrap();
    std::fs::write(dir.join("broken.otf"), b"not a font").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("../../../assets/fonts/Inter-Regular.ttf", dir.join("alias.ttf")).unwrap();
    let mut session = Session::new();
    session.set_text_font_catalogue(Arc::new(Catalogue::discover(std::slice::from_ref(&dir))));
    let id = session.open("source.pdf", None, support::source_subset::source_subset(), None).unwrap();
    assert!(!session.get(id).unwrap().text_editor_fonts(0).iter().any(|f| f.resource.starts_with("PCApprovedInstalled")));
    session.apply(id, Edit::EditTextBlock { page: 0, block: 0, text: "AB".into(), style: BlockStyle::default() }).unwrap();
    assert!(!session.get(id).unwrap().text_editor_fonts(0).iter().any(|f| f.resource.starts_with("PCFull")));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn automatic_font_discovery_excludes_forbidden_cjk_rebrands_and_missing_licences() {
    use pdfcraft_engine::text_fonts::Catalogue;
    let dir = std::env::temp_dir().join(format!("pdfcraft-auto-font-policy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // These are contributor-original rectangle outlines with deliberately hostile metadata,
    // not programs taken from any of the excluded families.
    for (i, family) in [
        "Source Han Sans",
        "Noto Sans SC",
        "Noto Sans JP",
        "Noto Sans TC",
        "Noto Sans HK",
        "Noto Sans KR",
        "Noto Serif SC",
        "Noto Serif JP",
        "Noto Serif TC",
        "Noto Serif HK",
        "Noto Serif KR",
    ]
    .iter()
    .enumerate()
    {
        let bytes = support::font_fixture::named_cff_font(family, &format!("{family}-Regular"), "MIT License");
        assert!(pdfcraft_fonts::embedding::validate_document_input(&bytes).unwrap_err().contains("asset policy"));
        std::fs::write(dir.join(format!("excluded-{i}.otf")), bytes).unwrap();
    }
    let undeclared = support::font_fixture::named_cff_font("Rect Font", "RectFont-Regular", "");
    assert!(pdfcraft_fonts::embedding::validate_document_input(&undeclared).unwrap_err().contains("openly licensed"));
    std::fs::write(dir.join("undeclared.otf"), undeclared).unwrap();
    assert_eq!(Catalogue::discover(std::slice::from_ref(&dir)).files().count(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}
