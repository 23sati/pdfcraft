//! Preflight profiles, evaluation rules, and analysis results.

use pdfcraft_cos::{Dict, Document, ObjRef, Object};
use serde::{Deserialize, Serialize};

use crate::pdfx::{self, PdfxLevel};
use crate::{Level, verify as verify_pdfa};

/// Severity of a preflight finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Info => "Info",
            Severity::Warning => "Warning",
            Severity::Error => "Error",
        }
    }
}

/// Category of preflight inspection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleCategory {
    Font,
    Color,
    Image,
    Geometry,
    Graphics,
    Standards,
    Document,
}

impl RuleCategory {
    pub fn label(self) -> &'static str {
        match self {
            RuleCategory::Font => "Fonts",
            RuleCategory::Color => "Colors",
            RuleCategory::Image => "Images",
            RuleCategory::Geometry => "Geometry & Page Boxes",
            RuleCategory::Graphics => "Vector Graphics",
            RuleCategory::Standards => "Standards & Output Intents",
            RuleCategory::Document => "Document & Security",
        }
    }
}

/// A specific violation detected during preflight inspection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreflightFinding {
    pub rule_id: String,
    pub category: RuleCategory,
    pub severity: Severity,
    pub message: String,
    pub page: Option<usize>,
    pub fixable: bool,
}

/// Summary and details of a completed preflight evaluation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreflightResult {
    pub profile_id: String,
    pub profile_name: String,
    pub passed: bool,
    pub errors: Vec<PreflightFinding>,
    pub warnings: Vec<PreflightFinding>,
    pub info: Vec<PreflightFinding>,
    pub fixable_count: usize,
    pub total_issues: usize,
}

/// A preflight rule configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreflightRule {
    pub id: String,
    pub name: String,
    pub category: RuleCategory,
    pub severity: Severity,
    pub fixable: bool,
}

/// A complete preflight profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreflightProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub target_standard: Option<String>,
    pub min_image_dpi: Option<f64>,
    pub max_image_dpi: Option<f64>,
    pub disallow_rgb: bool,
    pub disallow_transparency: bool,
    pub check_hairlines: bool,
    pub require_bleed_box: bool,
    pub require_trim_box: bool,
    pub require_all_fonts_embedded: bool,
    pub require_output_intent: bool,
    pub rules: Vec<PreflightRule>,
}

fn dict_of(o: &Object) -> Option<&Dict> {
    match o {
        Object::Dict(d) => Some(d),
        Object::Stream(s) => Some(&s.dict),
        _ => None,
    }
}

fn objects(doc: &Document) -> Vec<(ObjRef, std::sync::Arc<Object>)> {
    doc.object_numbers()
        .into_iter()
        .map(|n| {
            let r = ObjRef::new(n, doc.generation(n));
            (r, doc.get(r))
        })
        .collect()
}

/// Returns the complete catalog of built-in preflight profiles.
pub fn builtin_profiles() -> Vec<PreflightProfile> {
    vec![
        PreflightProfile {
            id: "prepress-sheetfed-cmyk".into(),
            name: "Prepress — Sheetfed Offset (CMYK, 300 DPI)".into(),
            description: "Commercial printing profile requiring CMYK/Spot, high resolution images (>= 300 DPI), bleed/trim boxes, embedded fonts, and no hairlines.".into(),
            target_standard: Some("PDF/X-1a".into()),
            min_image_dpi: Some(300.0),
            max_image_dpi: Some(450.0),
            disallow_rgb: true,
            disallow_transparency: true,
            check_hairlines: true,
            require_bleed_box: true,
            require_trim_box: true,
            require_all_fonts_embedded: true,
            require_output_intent: true,
            rules: default_prepress_rules(true),
        },
        PreflightProfile {
            id: "prepress-web-offset".into(),
            name: "Prepress — Web Offset (CMYK, 200 DPI)".into(),
            description: "High-speed web offset printing with 200 DPI min image resolution, CMYK/Spot colors, embedded fonts, and bleed geometry.".into(),
            target_standard: Some("PDF/X-1a".into()),
            min_image_dpi: Some(200.0),
            max_image_dpi: None,
            disallow_rgb: true,
            disallow_transparency: true,
            check_hairlines: true,
            require_bleed_box: true,
            require_trim_box: true,
            require_all_fonts_embedded: true,
            require_output_intent: true,
            rules: default_prepress_rules(true),
        },
        PreflightProfile {
            id: "magazine-ads".into(),
            name: "Magazine Ads (PDF/X-1a, CMYK, 300 DPI)".into(),
            description: "Strict magazine advertisement delivery: PDF/X-1a compliant, all CMYK/Spot, no transparency, fonts embedded, bleed boxes.".into(),
            target_standard: Some("PDF/X-1a:2001".into()),
            min_image_dpi: Some(300.0),
            max_image_dpi: None,
            disallow_rgb: true,
            disallow_transparency: true,
            check_hairlines: true,
            require_bleed_box: true,
            require_trim_box: true,
            require_all_fonts_embedded: true,
            require_output_intent: true,
            rules: default_prepress_rules(true),
        },
        PreflightProfile {
            id: "digital-printing".into(),
            name: "Digital Printing (RGB & CMYK allowed, 150 DPI)".into(),
            description: "Digital press and on-demand printing: allows RGB and transparency, requires at least 150 DPI and embedded fonts.".into(),
            target_standard: Some("PDF/X-4".into()),
            min_image_dpi: Some(150.0),
            max_image_dpi: None,
            disallow_rgb: false,
            disallow_transparency: false,
            check_hairlines: true,
            require_bleed_box: false,
            require_trim_box: true,
            require_all_fonts_embedded: true,
            require_output_intent: true,
            rules: default_prepress_rules(false),
        },
        PreflightProfile {
            id: "pdfx-1a-2001".into(),
            name: "PDF/X-1a:2001 Verification".into(),
            description: "ISO 15930-1 compliance check: blind CMYK exchange, no RGB, no transparency, all fonts embedded, OutputIntent present.".into(),
            target_standard: Some("PDF/X-1a:2001".into()),
            min_image_dpi: None,
            max_image_dpi: None,
            disallow_rgb: true,
            disallow_transparency: true,
            check_hairlines: false,
            require_bleed_box: false,
            require_trim_box: true,
            require_all_fonts_embedded: true,
            require_output_intent: true,
            rules: default_standard_rules(),
        },
        PreflightProfile {
            id: "pdfx-3-2002".into(),
            name: "PDF/X-3:2002 Verification".into(),
            description: "ISO 15930-3 compliance check: color-managed exchange, ICC profiles allowed, no transparency, fonts embedded.".into(),
            target_standard: Some("PDF/X-3:2002".into()),
            min_image_dpi: None,
            max_image_dpi: None,
            disallow_rgb: false,
            disallow_transparency: true,
            check_hairlines: false,
            require_bleed_box: false,
            require_trim_box: true,
            require_all_fonts_embedded: true,
            require_output_intent: true,
            rules: default_standard_rules(),
        },
        PreflightProfile {
            id: "pdfx-4".into(),
            name: "PDF/X-4 Verification".into(),
            description: "ISO 15930-7 compliance check: modern PDF/X with transparency and layer support, fonts embedded, OutputIntent present.".into(),
            target_standard: Some("PDF/X-4".into()),
            min_image_dpi: None,
            max_image_dpi: None,
            disallow_rgb: false,
            disallow_transparency: false,
            check_hairlines: false,
            require_bleed_box: false,
            require_trim_box: true,
            require_all_fonts_embedded: true,
            require_output_intent: true,
            rules: default_standard_rules(),
        },
        PreflightProfile {
            id: "pdfa-1b".into(),
            name: "PDF/A-1b Archival Verification".into(),
            description: "ISO 19005-1 Level B archival verification: no transparency, all fonts embedded, no encryption or interactive actions.".into(),
            target_standard: Some("PDF/A-1b".into()),
            min_image_dpi: None,
            max_image_dpi: None,
            disallow_rgb: false,
            disallow_transparency: true,
            check_hairlines: false,
            require_bleed_box: false,
            require_trim_box: false,
            require_all_fonts_embedded: true,
            require_output_intent: true,
            rules: default_standard_rules(),
        },
        PreflightProfile {
            id: "pdfa-2b".into(),
            name: "PDF/A-2b Archival Verification".into(),
            description: "ISO 19005-2 Level B archival verification: embedded fonts, device colour characterized, no encryption or forbidden actions.".into(),
            target_standard: Some("PDF/A-2b".into()),
            min_image_dpi: None,
            max_image_dpi: None,
            disallow_rgb: false,
            disallow_transparency: false,
            check_hairlines: false,
            require_bleed_box: false,
            require_trim_box: false,
            require_all_fonts_embedded: true,
            require_output_intent: true,
            rules: default_standard_rules(),
        },
        PreflightProfile {
            id: "pdfa-3b".into(),
            name: "PDF/A-3b Archival Verification".into(),
            description: "ISO 19005-3 Level B archival verification: permits embedded files of any format with AFRelationship.".into(),
            target_standard: Some("PDF/A-3b".into()),
            min_image_dpi: None,
            max_image_dpi: None,
            disallow_rgb: false,
            disallow_transparency: false,
            check_hairlines: false,
            require_bleed_box: false,
            require_trim_box: false,
            require_all_fonts_embedded: true,
            require_output_intent: true,
            rules: default_standard_rules(),
        },
        PreflightProfile {
            id: "web-friendly".into(),
            name: "Web Friendly (RGB friendly, screen resolution)".into(),
            description: "Screen and web viewing: allows RGB colors and transparency, flags excessive resolution (> 300 DPI) and encryption.".into(),
            target_standard: None,
            min_image_dpi: Some(72.0),
            max_image_dpi: Some(300.0),
            disallow_rgb: false,
            disallow_transparency: false,
            check_hairlines: false,
            require_bleed_box: false,
            require_trim_box: false,
            require_all_fonts_embedded: true,
            require_output_intent: false,
            rules: vec![
                PreflightRule {
                    id: "fonts-embedded".into(),
                    name: "All fonts embedded".into(),
                    category: RuleCategory::Font,
                    severity: Severity::Warning,
                    fixable: false,
                },
                PreflightRule {
                    id: "no-encryption".into(),
                    name: "Document not encrypted".into(),
                    category: RuleCategory::Document,
                    severity: Severity::Warning,
                    fixable: true,
                },
            ],
        },
    ]
}

fn default_prepress_rules(strict_cmyk: bool) -> Vec<PreflightRule> {
    let mut rules = vec![
        PreflightRule {
            id: "fonts-embedded".into(),
            name: "All fonts must be embedded".into(),
            category: RuleCategory::Font,
            severity: Severity::Error,
            fixable: false,
        },
        PreflightRule {
            id: "image-min-dpi".into(),
            name: "Image resolution meets minimum DPI".into(),
            category: RuleCategory::Image,
            severity: Severity::Error,
            fixable: false,
        },
        PreflightRule {
            id: "hairline-strokes".into(),
            name: "No hairlines below 0.25 pt".into(),
            category: RuleCategory::Graphics,
            severity: Severity::Warning,
            fixable: true,
        },
        PreflightRule {
            id: "page-trim-box".into(),
            name: "Every page has a TrimBox or ArtBox".into(),
            category: RuleCategory::Geometry,
            severity: Severity::Error,
            fixable: true,
        },
        PreflightRule {
            id: "output-intent".into(),
            name: "Document has a valid output intent".into(),
            category: RuleCategory::Standards,
            severity: Severity::Error,
            fixable: true,
        },
        PreflightRule {
            id: "no-encryption".into(),
            name: "Document is not encrypted".into(),
            category: RuleCategory::Document,
            severity: Severity::Error,
            fixable: true,
        },
        PreflightRule {
            id: "no-forbidden-actions".into(),
            name: "No interactive or forbidden actions".into(),
            category: RuleCategory::Document,
            severity: Severity::Warning,
            fixable: true,
        },
    ];
    if strict_cmyk {
        rules.push(PreflightRule {
            id: "no-rgb-colors".into(),
            name: "No RGB color objects in CMYK prepress".into(),
            category: RuleCategory::Color,
            severity: Severity::Error,
            fixable: false,
        });
        rules.push(PreflightRule {
            id: "no-transparency".into(),
            name: "No transparent objects or blend modes".into(),
            category: RuleCategory::Graphics,
            severity: Severity::Error,
            fixable: false,
        });
    }
    rules
}

fn default_standard_rules() -> Vec<PreflightRule> {
    vec![
        PreflightRule {
            id: "standard-conformance".into(),
            name: "Standard conformance validation".into(),
            category: RuleCategory::Standards,
            severity: Severity::Error,
            fixable: true,
        },
        PreflightRule {
            id: "fonts-embedded".into(),
            name: "All fonts embedded".into(),
            category: RuleCategory::Font,
            severity: Severity::Error,
            fixable: false,
        },
    ]
}

/// Find a profile by id or alias.
pub fn find_profile(id: &str) -> Option<PreflightProfile> {
    let clean = id.to_ascii_lowercase().replace(['_', ' '], "-");
    builtin_profiles().into_iter().find(|p| p.id == clean || p.id.replace('-', "") == clean.replace('-', ""))
}

/// Evaluates `doc` against the specified preflight `profile`.
pub fn evaluate_profile(doc: &Document, profile: &PreflightProfile) -> PreflightResult {
    let mut findings = Vec::new();
    let objs = objects(doc);
    let pages = pdfcraft_model::pages(doc);

    // 1. Evaluate underlying standards if profile targets PDF/X or PDF/A
    if let Some(target) = &profile.target_standard {
        match target.as_str() {
            "PDF/X-1a" | "PDF/X-1a:2001" => {
                for issue in pdfx::verify(doc, PdfxLevel::X1a2001) {
                    findings.push(PreflightFinding {
                        rule_id: "pdfx-1a-compliance".into(),
                        category: RuleCategory::Standards,
                        severity: Severity::Error,
                        message: format!("PDF/X-1a: {}", issue.message),
                        page: issue.page,
                        fixable: issue.fixable,
                    });
                }
            }
            "PDF/X-3" | "PDF/X-3:2002" => {
                for issue in pdfx::verify(doc, PdfxLevel::X32002) {
                    findings.push(PreflightFinding {
                        rule_id: "pdfx-3-compliance".into(),
                        category: RuleCategory::Standards,
                        severity: Severity::Error,
                        message: format!("PDF/X-3: {}", issue.message),
                        page: issue.page,
                        fixable: issue.fixable,
                    });
                }
            }
            "PDF/X-4" => {
                for issue in pdfx::verify(doc, PdfxLevel::X4) {
                    findings.push(PreflightFinding {
                        rule_id: "pdfx-4-compliance".into(),
                        category: RuleCategory::Standards,
                        severity: Severity::Error,
                        message: format!("PDF/X-4: {}", issue.message),
                        page: issue.page,
                        fixable: issue.fixable,
                    });
                }
            }
            "PDF/A-1b" => {
                for issue in verify_pdfa(doc, Level::A1b) {
                    findings.push(PreflightFinding {
                        rule_id: "pdfa-1b-compliance".into(),
                        category: RuleCategory::Standards,
                        severity: Severity::Error,
                        message: format!("PDF/A-1b: {}", issue.message),
                        page: issue.page,
                        fixable: issue.fixable,
                    });
                }
            }
            "PDF/A-2b" => {
                for issue in verify_pdfa(doc, Level::A2b) {
                    findings.push(PreflightFinding {
                        rule_id: "pdfa-2b-compliance".into(),
                        category: RuleCategory::Standards,
                        severity: Severity::Error,
                        message: format!("PDF/A-2b: {}", issue.message),
                        page: issue.page,
                        fixable: issue.fixable,
                    });
                }
            }
            "PDF/A-3b" => {
                for issue in verify_pdfa(doc, Level::A3b) {
                    findings.push(PreflightFinding {
                        rule_id: "pdfa-3b-compliance".into(),
                        category: RuleCategory::Standards,
                        severity: Severity::Error,
                        message: format!("PDF/A-3b: {}", issue.message),
                        page: issue.page,
                        fixable: issue.fixable,
                    });
                }
            }
            _ => {}
        }
    }

    // 2. Encryption
    if doc.trailer().get(b"Encrypt").is_some() {
        findings.push(PreflightFinding {
            rule_id: "no-encryption".into(),
            category: RuleCategory::Document,
            severity: Severity::Error,
            message: "The document is encrypted with a password or certificates".into(),
            page: None,
            fixable: true,
        });
    }

    // 3. Output intent check
    if profile.require_output_intent && !pdfx::has_pdfx_intent(doc) && !crate::has_pdfa_intent(doc) {
        findings.push(PreflightFinding {
            rule_id: "output-intent".into(),
            category: RuleCategory::Standards,
            severity: Severity::Error,
            message: "Missing standard OutputIntent (/GTS_PDFX or /GTS_PDFA1)".into(),
            page: None,
            fixable: true,
        });
    }

    // 4. Page Boxes
    for (i, p) in pages.iter().enumerate() {
        if profile.require_trim_box && p.dict.get(b"TrimBox").is_none() && p.dict.get(b"ArtBox").is_none() {
            findings.push(PreflightFinding {
                rule_id: "page-trim-box".into(),
                category: RuleCategory::Geometry,
                severity: Severity::Error,
                message: format!("Page {} has no TrimBox or ArtBox", i + 1),
                page: Some(i),
                fixable: true,
            });
        }
        if profile.require_bleed_box && p.dict.get(b"BleedBox").is_none() {
            findings.push(PreflightFinding {
                rule_id: "page-bleed-box".into(),
                category: RuleCategory::Geometry,
                severity: Severity::Warning,
                message: format!("Page {} has no BleedBox", i + 1),
                page: Some(i),
                fixable: true,
            });
        }
    }

    // 5. Font embedding
    if profile.require_all_fonts_embedded {
        for (_, o) in &objs {
            let Some(d) = dict_of(o) else { continue };
            let ty = d.name(b"Type");
            let sub = d.name(b"Subtype");
            if ty == Some(b"Font") || (d.get(b"BaseFont").is_some() && sub.is_some()) {
                if sub == Some(b"Type3") {
                    continue;
                }
                let name = d.name(b"BaseFont").map(|n| String::from_utf8_lossy(n).into_owned()).unwrap_or_else(|| "unnamed".into());
                let descriptor = if sub == Some(b"Type0") {
                    d.get(b"DescendantFonts")
                        .map(|x| doc.resolve(x))
                        .and_then(|a| a.as_array().and_then(|a| a.first().cloned()))
                        .map(|f| doc.resolve(&f))
                        .and_then(|f| f.as_dict().and_then(|fd| fd.get(b"FontDescriptor").cloned()))
                } else {
                    d.get(b"FontDescriptor").cloned()
                };
                let embedded = descriptor
                    .map(|fd| doc.resolve(&fd))
                    .and_then(|fd| fd.as_dict().cloned())
                    .is_some_and(|fd| fd.get(b"FontFile").is_some() || fd.get(b"FontFile2").is_some() || fd.get(b"FontFile3").is_some());
                if !embedded {
                    findings.push(PreflightFinding {
                        rule_id: "fonts-embedded".into(),
                        category: RuleCategory::Font,
                        severity: Severity::Error,
                        message: format!("Font '{name}' is not embedded"),
                        page: None,
                        fixable: false,
                    });
                }
            }
        }
    }

    // 6. Color restrictions
    if profile.disallow_rgb {
        let mut found_rgb = false;
        for (_, o) in &objs {
            let Some(d) = dict_of(o) else { continue };
            if let Some(cs) = d.get(b"ColorSpace").map(|c| doc.resolve(c))
                && (cs.as_name() == Some(b"DeviceRGB") || cs.as_name() == Some(b"CalRGB"))
            {
                found_rgb = true;
                break;
            }
            if let Object::Stream(s) = &**o
                && let Ok(data) = s.decoded()
            {
                for token in data.split(|b| *b == b'\n' || *b == b' ') {
                    if token == b"rg" || token == b"RG" {
                        found_rgb = true;
                        break;
                    }
                }
            }
        }
        if found_rgb {
            findings.push(PreflightFinding {
                rule_id: "no-rgb-colors".into(),
                category: RuleCategory::Color,
                severity: Severity::Error,
                message: "Document contains RGB color objects in a CMYK-only prepress profile".into(),
                page: None,
                fixable: false,
            });
        }
    }

    // 7. Image resolution checks
    if profile.min_image_dpi.is_some() || profile.max_image_dpi.is_some() {
        for (i, p) in pages.iter().enumerate() {
            let page_w = p
                .dict
                .get(b"MediaBox")
                .and_then(|m| doc.resolve(m).as_array().cloned())
                .and_then(|a| a.get(2).and_then(|w| doc.resolve(w).as_f64()))
                .unwrap_or(612.0);
            let page_h = p
                .dict
                .get(b"MediaBox")
                .and_then(|m| doc.resolve(m).as_array().cloned())
                .and_then(|a| a.get(3).and_then(|h| doc.resolve(h).as_f64()))
                .unwrap_or(792.0);

            let res = p.dict.get(b"Resources").map(|r| doc.resolve(r)).and_then(|r| r.as_dict().cloned());
            let xobjs = res.as_ref().and_then(|r| r.get(b"XObject")).map(|x| doc.resolve(x)).and_then(|x| x.as_dict().cloned());
            if let Some(xo) = xobjs {
                for (name, item) in xo.iter() {
                    let resolved = doc.resolve(item);
                    let Object::Stream(s) = &*resolved else { continue };
                    if s.dict.name(b"Subtype") != Some(b"Image") {
                        continue;
                    }
                    let px_w = s.dict.int(b"Width").unwrap_or(0) as f64;
                    let px_h = s.dict.int(b"Height").unwrap_or(0) as f64;
                    if px_w > 0.0 && px_h > 0.0 {
                        // Effective resolution assuming display dimensions bounded by page size
                        let dpi_w = (px_w / page_w) * 72.0;
                        let dpi_h = (px_h / page_h) * 72.0;
                        let effective_dpi = dpi_w.min(dpi_h);

                        if let Some(min_dpi) = profile.min_image_dpi
                            && effective_dpi < min_dpi
                        {
                            findings.push(PreflightFinding {
                                rule_id: "image-min-dpi".into(),
                                category: RuleCategory::Image,
                                severity: Severity::Warning,
                                message: format!(
                                    "Image '{}' on page {} has low resolution (~{:.0} DPI, min required: {:.0} DPI)",
                                    String::from_utf8_lossy(name),
                                    i + 1,
                                    effective_dpi,
                                    min_dpi
                                ),
                                page: Some(i),
                                fixable: false,
                            });
                        }

                        if let Some(max_dpi) = profile.max_image_dpi
                            && effective_dpi > max_dpi
                        {
                            findings.push(PreflightFinding {
                                rule_id: "image-max-dpi".into(),
                                category: RuleCategory::Image,
                                severity: Severity::Info,
                                message: format!(
                                    "Image '{}' on page {} has high resolution (~{:.0} DPI, recommended max: {:.0} DPI)",
                                    String::from_utf8_lossy(name),
                                    i + 1,
                                    effective_dpi,
                                    max_dpi
                                ),
                                page: Some(i),
                                fixable: false,
                            });
                        }
                    }
                }
            }
        }
    }

    // 8. Hairline stroke checks
    if profile.check_hairlines {
        for (i, p) in pages.iter().enumerate() {
            let contents = p.dict.get(b"Contents").map(|c| doc.resolve(c));
            if let Some(c) = contents {
                let stream_bytes = match &*c {
                    Object::Stream(s) => s.decoded().ok(),
                    Object::Array(arr) => {
                        let mut all = Vec::new();
                        for item in arr {
                            if let Object::Stream(s) = &*doc.resolve(item)
                                && let Ok(d) = s.decoded()
                            {
                                all.extend_from_slice(&d);
                            }
                        }
                        Some(all)
                    }
                    _ => None,
                };
                if let Some(data) = stream_bytes {
                    let mut prev_token: Option<&[u8]> = None;
                    for token in data.split(|b| *b == b'\n' || *b == b' ' || *b == b'\r' || *b == b'\t') {
                        if token.is_empty() {
                            continue;
                        }
                        if token == b"w"
                            && let Some(prev) = prev_token
                            && let Ok(str_val) = std::str::from_utf8(prev)
                            && let Ok(width) = str_val.parse::<f64>()
                            && width > 0.0
                            && width < 0.25
                        {
                            findings.push(PreflightFinding {
                                rule_id: "hairline-strokes".into(),
                                category: RuleCategory::Graphics,
                                severity: Severity::Warning,
                                message: format!("Page {} has hairline strokes with width {:.2} pt (< 0.25 pt)", i + 1, width),
                                page: Some(i),
                                fixable: true,
                            });
                            break;
                        }
                        prev_token = Some(token);
                    }
                }
            }
        }
    }

    // Deduplicate findings
    let mut seen = std::collections::HashSet::new();
    findings.retain(|f| seen.insert((f.rule_id.clone(), f.message.clone(), f.page)));

    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut info = Vec::new();
    let mut fixable_count = 0;

    for f in findings {
        if f.fixable {
            fixable_count += 1;
        }
        match f.severity {
            Severity::Error => errors.push(f),
            Severity::Warning => warnings.push(f),
            Severity::Info => info.push(f),
        }
    }

    let total_issues = errors.len() + warnings.len() + info.len();
    let passed = errors.is_empty();

    PreflightResult {
        profile_id: profile.id.clone(),
        profile_name: profile.name.clone(),
        passed,
        errors,
        warnings,
        info,
        fixable_count,
        total_issues,
    }
}
