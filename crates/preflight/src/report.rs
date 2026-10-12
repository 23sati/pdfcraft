//! Preflight reports generation in XML, plain text, and PDF format.

use pdfcraft_cos::{Dict, Document, ObjRef, Object, PdfString, Stream};
use serde::{Deserialize, Serialize};

use crate::profile::{PreflightResult, Severity};

/// Formats available for exporting preflight reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportFormat {
    Xml,
    Text,
    Pdf,
}

impl ReportFormat {
    pub fn from_str_id(s: &str) -> Option<ReportFormat> {
        match s.to_ascii_lowercase().trim() {
            "xml" => Some(ReportFormat::Xml),
            "txt" | "text" => Some(ReportFormat::Text),
            "pdf" => Some(ReportFormat::Pdf),
            _ => None,
        }
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Generates a machine-readable XML report of the preflight findings.
pub fn export_xml(result: &PreflightResult, doc_title: &str) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<preflightReport profileId=\"{}\" profileName=\"{}\" status=\"{}\" totalIssues=\"{}\" fixableCount=\"{}\">\n",
        escape_xml(&result.profile_id),
        escape_xml(&result.profile_name),
        if result.passed { "PASSED" } else { "FAILED" },
        result.total_issues,
        result.fixable_count
    ));
    out.push_str(&format!("  <document title=\"{}\"/>\n", escape_xml(doc_title)));
    out.push_str(&format!("  <summary errors=\"{}\" warnings=\"{}\" info=\"{}\"/>\n", result.errors.len(), result.warnings.len(), result.info.len()));

    out.push_str("  <findings>\n");
    for (category_name, items) in [("errors", &result.errors), ("warnings", &result.warnings), ("info", &result.info)] {
        if items.is_empty() {
            continue;
        }
        out.push_str(&format!("    <group severity=\"{category_name}\" count=\"{}\">\n", items.len()));
        for f in items {
            let page_attr = match f.page {
                Some(p) => format!(" page=\"{}\"", p + 1),
                None => String::new(),
            };
            out.push_str(&format!(
                "      <finding ruleId=\"{}\" category=\"{}\" severity=\"{}\" fixable=\"{}\"{}>\n",
                escape_xml(&f.rule_id),
                escape_xml(f.category.label()),
                escape_xml(f.severity.label()),
                f.fixable,
                page_attr
            ));
            out.push_str(&format!("        <message>{}</message>\n", escape_xml(&f.message)));
            out.push_str("      </finding>\n");
        }
        out.push_str("    </group>\n");
    }
    out.push_str("  </findings>\n");
    out.push_str("</preflightReport>\n");
    out
}

/// Generates a human-readable text report.
pub fn export_text(result: &PreflightResult, doc_title: &str) -> String {
    let mut out = String::new();
    out.push_str("================================================================================\n");
    out.push_str("PdfCraft Preflight Inspection Report\n");
    out.push_str("================================================================================\n");
    out.push_str(&format!("Document:       {}\n", if doc_title.is_empty() { "Untitled" } else { doc_title }));
    out.push_str(&format!("Profile:        {}\n", result.profile_name));
    out.push_str(&format!("Status:         {}\n", if result.passed { "PASSED" } else { "FAILED" }));
    out.push_str(&format!(
        "Total Issues:   {} ({} errors, {} warnings, {} info; {} fixable)\n",
        result.total_issues,
        result.errors.len(),
        result.warnings.len(),
        result.info.len(),
        result.fixable_count
    ));
    out.push_str("--------------------------------------------------------------------------------\n\n");

    if result.passed && result.warnings.is_empty() && result.info.is_empty() {
        out.push_str("No issues or rule violations found. Document complies with this profile.\n");
        return out;
    }

    if !result.errors.is_empty() {
        out.push_str(&format!("ERRORS ({})\n", result.errors.len()));
        out.push_str("------\n");
        for (idx, e) in result.errors.iter().enumerate() {
            let page_str = match e.page {
                Some(p) => format!(" [Page {}]", p + 1),
                None => String::new(),
            };
            let fix_str = if e.fixable { " [Fixable]" } else { "" };
            out.push_str(&format!("{}. [{}] {}{}{}\n", idx + 1, e.category.label(), e.message, page_str, fix_str));
        }
        out.push('\n');
    }

    if !result.warnings.is_empty() {
        out.push_str(&format!("WARNINGS ({})\n", result.warnings.len()));
        out.push_str("--------\n");
        for (idx, w) in result.warnings.iter().enumerate() {
            let page_str = match w.page {
                Some(p) => format!(" [Page {}]", p + 1),
                None => String::new(),
            };
            let fix_str = if w.fixable { " [Fixable]" } else { "" };
            out.push_str(&format!("{}. [{}] {}{}{}\n", idx + 1, w.category.label(), w.message, page_str, fix_str));
        }
        out.push('\n');
    }

    if !result.info.is_empty() {
        out.push_str(&format!("INFORMATION ({})\n", result.info.len()));
        out.push_str("-----------\n");
        for (idx, inf) in result.info.iter().enumerate() {
            let page_str = match inf.page {
                Some(p) => format!(" [Page {}]", p + 1),
                None => String::new(),
            };
            out.push_str(&format!("{}. [{}] {}{}\n", idx + 1, inf.category.label(), inf.message, page_str));
        }
        out.push('\n');
    }

    out
}

/// Generates a standalone, formatted PDF report document.
pub fn export_pdf(result: &PreflightResult, doc_title: &str) -> Document {
    let mut doc = Document::new_empty();

    let title_text = if doc_title.is_empty() { "Untitled Document" } else { doc_title };

    // Set Info dictionary
    let mut info = Dict::new();
    info.set(b"Title".to_vec(), PdfString::text(&format!("Preflight Report - {title_text}")));
    info.set(b"Producer".to_vec(), PdfString::text("PdfCraft Preflight Engine"));
    let info_ref = doc.add(info);
    doc.trailer_mut().set(b"Info".to_vec(), Object::Ref(info_ref));

    let pages_ref = doc.root().and_then(|r| doc.get(r).as_dict().and_then(|d| d.reference(b"Pages"))).unwrap_or(ObjRef::new(1, 0));

    // Standard 14 Helvetica font
    let mut font_dict = Dict::new();
    font_dict.set(b"Type".to_vec(), Object::name("Font"));
    font_dict.set(b"Subtype".to_vec(), Object::name("Type1"));
    font_dict.set(b"BaseFont".to_vec(), Object::name("Helvetica"));
    font_dict.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
    let font_ref = doc.add(font_dict);

    let mut font_bold = Dict::new();
    font_bold.set(b"Type".to_vec(), Object::name("Font"));
    font_bold.set(b"Subtype".to_vec(), Object::name("Type1"));
    font_bold.set(b"BaseFont".to_vec(), Object::name("Helvetica-Bold"));
    font_bold.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
    let font_bold_ref = doc.add(font_bold);

    let mut font_res = Dict::new();
    font_res.set(b"F1".to_vec(), Object::Ref(font_ref));
    font_res.set(b"F2".to_vec(), Object::Ref(font_bold_ref));

    let mut resources = Dict::new();
    resources.set(b"Font".to_vec(), Object::Dict(font_res));

    // Construct content stream for page 1
    let mut stream_content = String::new();

    // 1. Header Banner: Navy rectangle (0.1, 0.2, 0.35)
    stream_content.push_str("q\n");
    stream_content.push_str("0.12 0.22 0.38 rg\n");
    stream_content.push_str("36 710 540 46 re f\n");
    stream_content.push_str("1 1 1 rg\n");
    stream_content.push_str("BT /F2 16 Tf 48 728 Td (PdfCraft Preflight Inspection Report) Tj ET\n");
    stream_content.push_str("Q\n");

    // 2. Metadata Box
    stream_content.push_str("BT\n");
    stream_content.push_str("0.2 0.2 0.2 rg\n");
    stream_content.push_str("/F2 10 Tf 40 685 Td (Document:) Tj\n");
    stream_content.push_str(&format!("/F1 10 Tf 65 0 Td ({}) Tj\n", escape_pdf_str(title_text)));
    stream_content.push_str("/F2 10 Tf -65 -15 Td (Profile:) Tj\n");
    stream_content.push_str(&format!("/F1 10 Tf 50 0 Td ({}) Tj\n", escape_pdf_str(&result.profile_name)));
    stream_content.push_str("ET\n");

    // 3. Status Badge: Green (0.15 0.65 0.25) or Red (0.85 0.2 0.2)
    stream_content.push_str("q\n");
    if result.passed {
        stream_content.push_str("0.18 0.65 0.28 rg\n");
        stream_content.push_str("470 660 100 24 re f\n");
        stream_content.push_str("1 1 1 rg\n");
        stream_content.push_str("BT /F2 11 Tf 498 668 Td (PASSED) Tj ET\n");
    } else {
        stream_content.push_str("0.85 0.22 0.22 rg\n");
        stream_content.push_str("470 660 100 24 re f\n");
        stream_content.push_str("1 1 1 rg\n");
        stream_content.push_str("BT /F2 11 Tf 502 668 Td (FAILED) Tj ET\n");
    }
    stream_content.push_str("Q\n");

    // 4. Metrics Summary Strip
    stream_content.push_str("q\n");
    stream_content.push_str("0.94 0.95 0.96 rg\n");
    stream_content.push_str("36 620 540 24 re f\n");
    stream_content.push_str("0.2 0.2 0.2 rg\n");
    stream_content.push_str(&format!(
        "BT /F2 9 Tf 48 628 Td (Summary: ) Tj /F1 9 Tf ({} Errors    {} Warnings    {} Info    ({} Fixable)) Tj ET\n",
        result.errors.len(),
        result.warnings.len(),
        result.info.len(),
        result.fixable_count
    ));
    stream_content.push_str("Q\n");

    // 5. Findings List
    let mut y = 590.0;
    stream_content.push_str("BT\n");
    stream_content.push_str("/F2 11 Tf 40 595 Td (Rule Findings:) Tj\n");
    stream_content.push_str("ET\n");

    let all_findings = result.errors.iter().chain(result.warnings.iter()).chain(result.info.iter());
    let mut count = 0;

    for f in all_findings {
        if y < 60.0 {
            break; // Single-page summary limit
        }
        let (tag, tag_r, tag_g, tag_b) = match f.severity {
            Severity::Error => ("[ERROR]", 0.85, 0.1, 0.1),
            Severity::Warning => ("[WARN]", 0.85, 0.55, 0.0),
            Severity::Info => ("[INFO]", 0.1, 0.45, 0.8),
        };
        let page_text = match f.page {
            Some(p) => format!(" (p. {})", p + 1),
            None => String::new(),
        };

        stream_content.push_str("q\n");
        stream_content.push_str(&format!("{tag_r} {tag_g} {tag_b} rg\n"));
        stream_content.push_str(&format!("BT /F2 8 Tf 40 {y:.1} Td ({tag}) Tj ET\n"));
        stream_content.push_str("0.15 0.15 0.15 rg\n");
        let truncated_msg: String = f.message.chars().take(80).collect();
        stream_content.push_str(&format!("BT /F1 8 Tf 85 {y:.1} Td ({}{}) Tj ET\n", escape_pdf_str(&truncated_msg), escape_pdf_str(&page_text)));
        stream_content.push_str("Q\n");

        y -= 14.0;
        count += 1;
    }

    if count == 0 {
        stream_content.push_str("BT /F1 10 Tf 40 560 Td (No issues identified. The document fully complies with this profile.) Tj ET\n");
    } else if result.total_issues > count {
        stream_content.push_str(&format!(
            "BT /F1 8 Tf 40 {y:.1} Td (... and {} additional findings recorded in full XML report) Tj ET\n",
            result.total_issues - count
        ));
    }

    // Footer
    stream_content.push_str("BT /F1 8 Tf 40 30 Td (Generated by PdfCraft Preflight & Standards Engine) Tj ET\n");

    // Build Page Object
    let content_stream = doc.add(Object::Stream(Stream::flate(Dict::new(), stream_content.as_bytes())));

    let mut page = Dict::new();
    page.set(b"Type".to_vec(), Object::name("Page"));
    page.set(b"Parent".to_vec(), Object::Ref(pages_ref));
    page.set(b"MediaBox".to_vec(), Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]));
    page.set(b"Resources".to_vec(), Object::Dict(resources));
    page.set(b"Contents".to_vec(), Object::Ref(content_stream));

    let page_ref = doc.add(page);

    let _ = doc.update_dict(pages_ref, |d| {
        let mut kids = d.get(b"Kids").and_then(|k| k.as_array().cloned()).unwrap_or_default();
        kids.push(Object::Ref(page_ref));
        d.set(b"Count".to_vec(), Object::Int(kids.len() as i64));
        d.set(b"Kids".to_vec(), Object::Array(kids));
    });

    doc
}

/// Generates a standalone, formatted PDF report as bytes.
pub fn export_pdf_bytes(result: &PreflightResult, doc_title: &str) -> Result<Vec<u8>, String> {
    let doc = export_pdf(result, doc_title);
    pdfcraft_cos::write_full(&doc, &pdfcraft_cos::SaveOptions::default()).map_err(|e| e.to_string())
}

fn escape_pdf_str(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            '\r' | '\n' => out.push(' '),
            _ if c.is_ascii() => out.push(c),
            _ => out.push('?'),
        }
    }
    out
}
