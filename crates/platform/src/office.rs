//! Office document conversion shared by PdfCraft's desktop UI and headless tools.
//!
//! PowerPoint uses DeckCraft's Rust reader and PDF exporter. Spreadsheets use GridCraft's Rust
//! reader and print renderer. Word formats use WordCraft's Rust reader and PDF exporter.
//! The caller opens the returned PDF bytes.

use thiserror::Error;

const MAX_SOURCE: usize = 100 * 1024 * 1024;
const MAX_PDF: u64 = 256 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum OfficeError {
    #[error("unsupported Office document type: {0}")]
    Unsupported(String),
    #[error("the Office document exceeds the 100 MiB conversion limit")]
    TooLarge,
    #[error("PowerPoint conversion failed: {0}")]
    PowerPoint(String),
    #[error("spreadsheet conversion failed: {0}")]
    Spreadsheet(String),
    #[error("Word conversion failed: {0}")]
    Word(String),
    #[error("Office conversion produced an invalid or oversized PDF")]
    InvalidPdf,
}

fn extension(name: &str) -> Option<&str> {
    name.rsplit_once('.').map(|(_, ext)| ext).filter(|ext| !ext.is_empty())
}

/// Whether a file can be handed to an Office-to-PDF provider.
pub fn supports(name: &str) -> bool {
    extension(name).is_some_and(|ext| {
        [
            "pptx", "potx", "ppsx", "docx", "docm", "dotx", "dotm", "doc", "dot", "odt", "rtf", "xlsx", "xlsm", "xltx", "xltm", "xlsb", "xls", "xlt",
            "ods",
        ]
        .iter()
        .any(|allowed| ext.eq_ignore_ascii_case(allowed))
    })
}

/// Convert an Office file to a PDF. This is a native desktop service, never run in the browser.
pub fn to_pdf(name: &str, source: &[u8]) -> Result<Vec<u8>, OfficeError> {
    if source.len() > MAX_SOURCE {
        return Err(OfficeError::TooLarge);
    }
    let ext = extension(name).ok_or_else(|| OfficeError::Unsupported(name.into()))?.to_ascii_lowercase();
    let pdf = match ext.as_str() {
        "pptx" | "potx" | "ppsx" => {
            // A malformed presentation must not take the desktop app or MCP server down.
            std::panic::catch_unwind(|| {
                let deck = deckcraft_pptx::import(source).map_err(|e| OfficeError::PowerPoint(e.to_string()))?;
                deckcraft_pdf::export(&deck, &Default::default()).map_err(|e| OfficeError::PowerPoint(e.to_string()))
            })
            .map_err(|_| OfficeError::PowerPoint("the presentation could not be read".into()))?
        }
        "xlsx" | "xlsm" | "xltx" | "xltm" | "xlsb" | "xls" | "xlt" | "ods" => std::panic::catch_unwind(|| {
            let (workbook, _) = gridcraft_engine::io::open_bytes(name, source).map_err(|e| OfficeError::Spreadsheet(e.to_string()))?;
            let options = gridcraft_engine::cmd::print::PrintOptions { all_sheets: true, ..Default::default() };
            let layouts = gridcraft_engine::cmd::print::layouts(&workbook, &options);
            let (pdf, _) = gridcraft_engine::cmd::print::render_pdf(&workbook, &layouts, name, "");
            Ok(pdf)
        })
        .map_err(|_| OfficeError::Spreadsheet("the workbook could not be read".into()))?,
        "docx" | "docm" | "dotx" | "dotm" | "doc" | "dot" | "odt" | "rtf" => std::panic::catch_unwind(|| {
            let doc = wordcraft_engine::io::open_bytes(name, source).map_err(OfficeError::Word)?;
            wordcraft_pdf::export(&doc, &Default::default()).map_err(|e| OfficeError::Word(e.to_string()))
        })
        .map_err(|_| OfficeError::Word("the document could not be read".into()))?,
        _ => Err(OfficeError::Unsupported(name.into())),
    }?;
    if pdf.len() as u64 > MAX_PDF || !pdf.starts_with(b"%PDF-") {
        return Err(OfficeError::InvalidPdf);
    }
    Ok(pdf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn office_types_are_explicit() {
        assert!(supports("report.DOCX"));
        assert!(supports("slides.pptx"));
        assert!(supports("budget.xlsx"));
        assert!(supports("budget.xlsb"));
        assert!(supports("letter.docm"));
        assert!(!supports("document.pdf"));
        assert!(!supports("script.js"));
        assert!(matches!(to_pdf("script.js", b"anything"), Err(OfficeError::Unsupported(_))));
    }

    #[test]
    fn deckcraft_presentation_becomes_a_pdf() {
        let mut deck = deckcraft_model::Presentation::default();
        if let Some(shape) = std::sync::Arc::make_mut(&mut deck.slides[0]).shapes.first_mut() {
            shape.text = Some(deckcraft_model::TextBody::from_text("A synthetic slide"));
        }
        let source = deckcraft_pptx::export(&deck).unwrap();
        let pdf = to_pdf("sample.pptx", &source).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(pdf.len() > 1_000);
    }
}
