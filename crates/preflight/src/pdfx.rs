//! PDF/X verification and conversion (ISO 15930).
//!
//! [`verify`] checks a document against ISO 15930 (PDF/X-1a:2001, PDF/X-3:2002, PDF/X-4).
//! [`convert`] repairs what can be repaired without changing visual content (output intent,
//! page boxes, /Trapped key, encryption, forbidden actions, XMP metadata) and returns a report.

use pdfcraft_cos::{Dict, Document, ObjRef, Object, PdfString, Stream};

use crate::{Issue, Report, icc, xmp};

/// Supported PDF/X conformance levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PdfxLevel {
    /// PDF/X-1a:2001 (ISO 15930-1:2001, blind CMYK exchange, no RGB, no transparency).
    X1a2001,
    /// PDF/X-3:2002 (ISO 15930-3:2002, color-managed exchange with ICC profiles, no transparency).
    X32002,
    /// PDF/X-4 (ISO 15930-7:2010, allows PDF 1.6 transparency and layers).
    X4,
}

impl PdfxLevel {
    pub fn label(self) -> &'static str {
        match self {
            PdfxLevel::X1a2001 => "PDF/X-1a:2001",
            PdfxLevel::X32002 => "PDF/X-3:2002",
            PdfxLevel::X4 => "PDF/X-4",
        }
    }

    pub fn version_str(self) -> &'static str {
        match self {
            PdfxLevel::X1a2001 => "PDF/X-1:2001",
            PdfxLevel::X32002 => "PDF/X-3:2002",
            PdfxLevel::X4 => "PDF/X-4",
        }
    }

    pub fn conformance_str(self) -> Option<&'static str> {
        match self {
            PdfxLevel::X1a2001 => Some("PDF/X-1a:2001"),
            PdfxLevel::X32002 => None,
            PdfxLevel::X4 => None,
        }
    }

    pub fn from_id(id: &str) -> Option<PdfxLevel> {
        match id.to_ascii_lowercase().replace(['/', '-', '_', ' ', ':'], "").as_str() {
            "1a" | "pdfx1a" | "x1a" | "x1a2001" | "pdfx1a2001" => Some(PdfxLevel::X1a2001),
            "3" | "pdfx3" | "x3" | "x32002" | "pdfx32002" => Some(PdfxLevel::X32002),
            "4" | "pdfx4" | "x4" => Some(PdfxLevel::X4),
            _ => None,
        }
    }
}

const FORBIDDEN_ACTIONS: &[&[u8]] =
    &[b"JavaScript", b"Launch", b"Sound", b"Movie", b"ResetForm", b"ImportData", b"Hide", b"SetOCGState", b"Rendition", b"Trans", b"GoTo3DView"];

const FORBIDDEN_ANNOTS: &[&[u8]] = &[b"Sound", b"Movie", b"Screen", b"3D", b"RichMedia"];

fn catalog(doc: &Document) -> Dict {
    doc.root().and_then(|r| doc.get(r).as_dict().cloned()).unwrap_or_default()
}

fn info(doc: &Document, key: &str) -> Option<String> {
    let i = doc.trailer().get(b"Info").map(|i| doc.resolve(i))?;
    i.as_dict()?.get(key.as_bytes()).and_then(|v| doc.resolve(v).as_string().map(|s| s.to_text()))
}

fn dict_of(o: &Object) -> Option<&Dict> {
    match o {
        Object::Dict(d) => Some(d),
        Object::Stream(s) => Some(&s.dict),
        _ => None,
    }
}

fn metadata(doc: &Document) -> Option<String> {
    let cat = catalog(doc);
    let m = doc.resolve(cat.get(b"Metadata")?);
    match &*m {
        Object::Stream(s) => s.decoded().ok().map(|b| String::from_utf8_lossy(&b).into_owned()),
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

/// Checks whether any output intent has `/S /GTS_PDFX`.
pub fn has_pdfx_intent(doc: &Document) -> bool {
    pdfx_intent_info(doc).is_some()
}

/// Information about the PDF/X output intent.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfxIntentInfo {
    pub condition_id: String,
    pub channels: Option<i64>,
}

pub fn pdfx_intent_info(doc: &Document) -> Option<PdfxIntentInfo> {
    let cat = catalog(doc);
    let list = cat.get(b"OutputIntents").map(|o| doc.resolve(o)).and_then(|o| o.as_array().cloned()).unwrap_or_default();
    list.iter().find_map(|oi| {
        let d = doc.resolve(oi).as_dict().cloned()?;
        if d.name(b"S") != Some(b"GTS_PDFX") {
            return None;
        }
        let condition_id = d.get(b"OutputConditionIdentifier").and_then(|v| doc.resolve(v).as_string().map(|s| s.to_text())).unwrap_or_default();
        let channels = d.get(b"DestOutputProfile").and_then(|p| {
            let res = doc.resolve(p);
            let Object::Stream(s) = &*res else { return None };
            s.dict.int(b"N").or_else(|| {
                let dec = s.decoded().ok()?;
                match dec.get(16..20)? {
                    b"GRAY" => Some(1),
                    b"RGB " => Some(3),
                    b"CMYK" => Some(4),
                    _ => None,
                }
            })
        });
        Some(PdfxIntentInfo { condition_id, channels })
    })
}

/// Colors used in the document.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ColorUsage {
    gray: bool,
    rgb: bool,
    cmyk: bool,
    spot: bool,
}

fn inspect_colors(doc: &Document, objs: &[(ObjRef, std::sync::Arc<Object>)]) -> ColorUsage {
    let mut usage = ColorUsage::default();
    for (_, o) in objs {
        let Some(d) = dict_of(o) else { continue };
        if let Some(cs) = d.get(b"ColorSpace").map(|c| doc.resolve(c)) {
            match &*cs {
                Object::Name(n) if n == b"DeviceGray" => usage.gray = true,
                Object::Name(n) if n == b"DeviceRGB" => usage.rgb = true,
                Object::Name(n) if n == b"DeviceCMYK" => usage.cmyk = true,
                Object::Name(n) if n == b"Separation" || n == b"DeviceN" => usage.spot = true,
                Object::Array(a) => {
                    if let Some(first) = a.first().and_then(|f| f.as_name()) {
                        if first == b"Separation" || first == b"DeviceN" {
                            usage.spot = true;
                        } else if first == b"DeviceRGB" || first == b"CalRGB" {
                            usage.rgb = true;
                        } else if first == b"DeviceCMYK" {
                            usage.cmyk = true;
                        } else if first == b"DeviceGray" || first == b"CalGray" {
                            usage.gray = true;
                        }
                    }
                }
                _ => {}
            }
        }
        if let Object::Stream(s) = &**o
            && d.get(b"Type").is_none()
            && d.get(b"Subtype").is_none_or(|t| t.as_name() == Some(b"Form"))
            && let Ok(data) = s.decoded()
        {
            for token in data.split(|b| *b == b'\n' || *b == b' ' || *b == b'\r' || *b == b'\t') {
                match token {
                    b"g" | b"G" => usage.gray = true,
                    b"rg" | b"RG" => usage.rgb = true,
                    b"k" | b"K" => usage.cmyk = true,
                    b"cs" | b"CS" | b"scn" | b"SCN" => usage.spot = true,
                    _ => {}
                }
            }
        }
    }
    usage
}

/// Checks if transparency is present in the document.
fn inspect_transparency(doc: &Document, objs: &[(ObjRef, std::sync::Arc<Object>)]) -> bool {
    for (_, o) in objs {
        let Some(d) = dict_of(o) else { continue };
        // Page or Form XObject /Group with /S /Transparency
        if let Some(g) = d.get(b"Group").map(|g| doc.resolve(g))
            && let Some(gd) = g.as_dict()
            && gd.name(b"S") == Some(b"Transparency")
        {
            return true;
        }
        // ExtGState transparency attributes
        if d.name(b"Type") == Some(b"ExtGState") {
            if let Some(ca) = d.get(b"ca").and_then(|c| doc.resolve(c).as_f64())
                && ca < 0.9999
            {
                return true;
            }
            if let Some(ca) = d.get(b"CA").and_then(|c| doc.resolve(c).as_f64())
                && ca < 0.9999
            {
                return true;
            }
            if let Some(bm) = d.name(b"BM")
                && bm != b"Normal"
                && bm != b"Compatible"
            {
                return true;
            }
            if d.get(b"SMask").is_some() {
                return true;
            }
        }
        // Images with Soft Mask
        if let Object::Stream(s) = &**o
            && s.dict.name(b"Subtype") == Some(b"Image")
            && (s.dict.get(b"SMask").is_some() || s.dict.get(b"Mask").is_some())
        {
            return true;
        }
    }
    false
}

/// Verify `doc` against the given PDF/X conformance level.
pub fn verify(doc: &Document, level: PdfxLevel) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut issue = |clause: &'static str, pattern: &'static str, value: Option<String>, page: Option<usize>, fixable: bool| {
        let message = match &value {
            Some(v) => pattern.replacen("{}", v, 1),
            None => pattern.to_owned(),
        };
        out.push(Issue { clause, message, pattern, value, page, fixable });
    };

    // 1. Identification and metadata
    let has_info_version = info(doc, "GTS_PDFXVersion").is_some();
    let meta = metadata(doc);
    let meta_version = meta.as_ref().and_then(|x| xmp::value(x, "pdfxid:GTS_PDFXVersion").or_else(|| xmp::value(x, "pdfx:GTS_PDFXVersion")));

    if !has_info_version && meta_version.is_none() {
        issue("15930-1 §6.1", "The document doesn't identify as {}", Some(level.label().to_owned()), None, true);
    } else if let Some(mv) = &meta_version
        && mv != level.version_str()
    {
        issue("15930-1 §6.1", "The metadata doesn't identify the file as {}", Some(level.label().to_owned()), None, true);
    }

    if level == PdfxLevel::X1a2001 {
        let conf = info(doc, "GTS_PDFXConformance").or_else(|| meta.as_ref().and_then(|x| xmp::value(x, "pdfx:GTS_PDFXConformance")));
        if conf.as_deref() != Some("PDF/X-1a:2001") {
            issue("15930-1 §6.1", "The document lacks PDF/X-1a conformance declaration", None, None, true);
        }
    }

    // 2. Trapped key
    let cat = catalog(doc);
    let trapped =
        info(doc, "Trapped").or_else(|| cat.get(b"Trapped").and_then(|t| doc.resolve(t).as_name().map(|n| String::from_utf8_lossy(n).into_owned())));
    match trapped.as_deref() {
        Some("True") | Some("False") => {}
        _ => issue("15930-1 §6.2", "The document has no /Trapped key, or it is /Unknown", None, None, true),
    }

    // 3. Encryption
    if doc.trailer().get(b"Encrypt").is_some() {
        issue("15930-1 §6.3", "The document is encrypted", None, None, true);
    }

    // 4. Output intent
    match pdfx_intent_info(doc) {
        None => {
            issue("15930-1 §6.4", "The document has no PDF/X output intent (/GTS_PDFX)", None, None, true);
        }
        Some(oi) => {
            if oi.condition_id.is_empty() {
                issue("15930-1 §6.4", "The PDF/X output intent has no OutputConditionIdentifier", None, None, true);
            }
            if level == PdfxLevel::X1a2001 && oi.channels.is_some_and(|ch| ch != 4 && ch != 1) {
                issue("15930-1 §6.4", "PDF/X-1a output intent must specify a CMYK or Gray profile", None, None, false);
            }
        }
    }

    // 5. Page Boxes: Each page must have either TrimBox or ArtBox
    let pages = pdfcraft_model::pages(doc);
    for (i, p) in pages.iter().enumerate() {
        let has_trim = p.dict.get(b"TrimBox").is_some();
        let has_art = p.dict.get(b"ArtBox").is_some();
        if !has_trim && !has_art {
            issue("15930-1 §6.5", "Page {} has neither a TrimBox nor an ArtBox", Some((i + 1).to_string()), Some(i), true);
        }
        if p.dict.get(b"AA").is_some() {
            issue("15930-1 §6.7", "Page {} has additional actions", Some((i + 1).to_string()), Some(i), true);
        }
    }

    let objs = objects(doc);

    // 6. Color restrictions
    let colors = inspect_colors(doc, &objs);
    if level == PdfxLevel::X1a2001 && colors.rgb {
        issue("15930-1 §6.6", "PDF/X-1a forbids RGB colour", None, None, false);
    }

    // 7. Transparency restrictions
    if (level == PdfxLevel::X1a2001 || level == PdfxLevel::X32002) && inspect_transparency(doc, &objs) {
        issue("15930-1 §6.8", "PDF/X-1a and PDF/X-3 forbid transparency", None, None, false);
    }

    // 8. Fonts embedded
    for (_, o) in &objs {
        let Some(d) = dict_of(o) else { continue };
        let ty = d.name(b"Type");
        let sub = d.name(b"Subtype");
        if ty == Some(b"Font") || (d.get(b"BaseFont").is_some() && sub.is_some()) {
            if sub == Some(b"Type3") {
                continue;
            }
            let name = d.name(b"BaseFont").map(|n| String::from_utf8_lossy(n).into_owned());
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
                match name {
                    Some(n) => issue("15930-1 §6.9", "The font {} isn't embedded", Some(n), None, false),
                    None => issue("15930-1 §6.9", "A font isn't embedded", None, None, false),
                }
            }
        }

        // 9. Annotations and actions
        if ty == Some(b"Annot") || (sub.is_some() && d.get(b"Rect").is_some() && d.get(b"P").is_some()) {
            let name = sub.map(|s| String::from_utf8_lossy(s).into_owned()).unwrap_or_default();
            if sub.is_some_and(|s| FORBIDDEN_ANNOTS.contains(&s)) {
                issue("15930-1 §6.10", "{} annotations aren't allowed", Some(name.clone()), None, true);
            }
            let f = d.int(b"F").unwrap_or(0);
            if sub != Some(b"Popup") && (f & 4 == 0 || f & (1 | 2 | 32) != 0) {
                issue("15930-1 §6.10", "A {} annotation isn't set to print, or is hidden", Some(name), None, true);
            }
        }

        if let Some(s) = d.name(b"S")
            && FORBIDDEN_ACTIONS.contains(&s)
        {
            issue("15930-1 §6.11", "A {} action isn't allowed", Some(String::from_utf8_lossy(s).into_owned()), None, true);
        }

        // 10. Streams
        if let Object::Stream(s) = &**o {
            if s.dict.name(b"Subtype") == Some(b"PS") {
                issue("15930-1 §6.12", "PostScript XObjects aren't allowed", None, None, false);
            }
            if s.dict.get(b"OPI").is_some() {
                issue("15930-1 §6.12", "An image has alternates or OPI information", None, None, true);
            }
        }
    }

    let mut seen = std::collections::HashSet::new();
    out.retain(|i| seen.insert((i.clause, i.message.clone(), i.page)));
    out
}

/// Convert `doc` into the specified PDF/X standard.
pub fn convert(doc: &mut Document, level: PdfxLevel) -> Result<Report, pdfcraft_cos::CosError> {
    let mut fixed = Vec::new();

    // 1. Remove encryption
    if doc.trailer().get(b"Encrypt").is_some() {
        doc.remove_encryption();
        fixed.push("Removed encryption".into());
    }

    let root = doc.root().ok_or(pdfcraft_cos::CosError::Syntax { offset: 0, detail: "no catalog".into() })?;
    let mut cat = catalog(doc);

    // 2. Clear catalog actions and JavaScript
    if cat.remove(b"AA").is_some() {
        fixed.push("Removed document additional actions".into());
    }
    if let Some(oa) = cat.get(b"OpenAction").cloned()
        && matches!(&*doc.resolve(&oa), Object::Dict(_))
    {
        cat.remove(b"OpenAction");
        fixed.push("Removed open action".into());
    }
    if let Some(mut names) = cat.get(b"Names").map(|n| doc.resolve(n)).and_then(|n| n.as_dict().cloned())
        && names.remove(b"JavaScript").is_some()
    {
        cat.set(b"Names".to_vec(), Object::Dict(names));
        fixed.push("Removed document-level JavaScript".into());
    }

    // 3. Set Trapped key in Document Info and Catalog
    let mut info_dict = doc.trailer().get(b"Info").map(|i| doc.resolve(i)).and_then(|i| i.as_dict().cloned()).unwrap_or_default();
    info_dict.set(b"Trapped".to_vec(), Object::name("False"));
    info_dict.set(b"GTS_PDFXVersion".to_vec(), PdfString::text(level.version_str()));
    if let Some(conf) = level.conformance_str() {
        info_dict.set(b"GTS_PDFXConformance".to_vec(), PdfString::text(conf));
    }
    let info_ref = doc.add(info_dict);
    doc.trailer_mut().set(b"Info".to_vec(), Object::Ref(info_ref));
    cat.set(b"Trapped".to_vec(), Object::name("False"));
    fixed.push("Set /Trapped to False and recorded PDF/X version in document info".into());

    // 4. Output intent
    if !has_pdfx_intent(doc) {
        let (profile_data, condition_id, num_channels) = match level {
            PdfxLevel::X1a2001 => (icc::cmyk_swop(), "CGATS TR 001", 4),
            PdfxLevel::X32002 | PdfxLevel::X4 => (icc::cmyk_swop(), "FOGRA39", 4),
        };
        let mut icc_dict = Dict::new();
        icc_dict.set(b"N".to_vec(), Object::Int(num_channels));
        let profile = doc.add(Object::Stream(Stream::flate(icc_dict, &profile_data)));

        let mut oi = Dict::new();
        oi.set(b"Type".to_vec(), Object::name("OutputIntent"));
        oi.set(b"S".to_vec(), Object::name("GTS_PDFX"));
        oi.set(b"OutputConditionIdentifier".to_vec(), PdfString::text(condition_id));
        oi.set(b"Info".to_vec(), PdfString::text(condition_id));
        oi.set(b"DestOutputProfile".to_vec(), Object::Ref(profile));

        let mut list = cat.get(b"OutputIntents").map(|o| doc.resolve(o)).and_then(|o| o.as_array().cloned()).unwrap_or_default();
        list.push(Object::Dict(oi));
        cat.set(b"OutputIntents".to_vec(), Object::Array(list));
        fixed.push(format!("Added PDF/X output intent with destination profile ({condition_id})"));
    }

    // 5. Ensure XMP metadata
    let xmp_packet = make_pdfx_xmp(doc, level);
    let mut md = Dict::new();
    md.set(b"Type".to_vec(), Object::name("Metadata"));
    md.set(b"Subtype".to_vec(), Object::name("XML"));
    let m = doc.add(Object::Stream(Stream::from_raw(md, xmp_packet.into_bytes())));
    cat.set(b"Metadata".to_vec(), Object::Ref(m));
    fixed.push(format!("Identified the file as {} in XMP metadata", level.label()));

    doc.update_dict(root, |c| *c = cat)?;

    // 6. Page boxes: TrimBox defaults to CropBox or MediaBox
    let mut page_boxes_fixed = 0;
    for p in pdfcraft_model::pages(doc) {
        let mut pd = p.dict.clone();
        let mut changed = pd.remove(b"AA").is_some();
        if pd.get(b"TrimBox").is_none() && pd.get(b"ArtBox").is_none() {
            let box_rect = pd
                .get(b"CropBox")
                .or_else(|| pd.get(b"MediaBox"))
                .cloned()
                .unwrap_or_else(|| Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]));
            pd.set(b"TrimBox".to_vec(), box_rect);
            changed = true;
            page_boxes_fixed += 1;
        }
        if changed {
            doc.update_dict(p.obj, |d| *d = pd)?;
        }
    }
    if page_boxes_fixed > 0 {
        fixed.push(format!("{page_boxes_fixed} page(s): added missing TrimBox"));
    }

    // 7. Strip forbidden annotations and OPI info
    let (mut annots_fixed, mut images_fixed) = (0, 0);
    for (r, o) in objects(doc) {
        let Some(d) = dict_of(&o).cloned() else { continue };
        let ty = d.name(b"Type").map(<[u8]>::to_vec);
        let sub = d.name(b"Subtype").map(<[u8]>::to_vec);
        let is_annot = ty.as_deref() == Some(b"Annot");

        if is_annot {
            let mut nd = d.clone();
            let mut changed = nd.remove(b"AA").is_some() || nd.remove(b"A").is_some();
            if sub.as_deref() != Some(b"Popup") {
                let f = nd.int(b"F").unwrap_or(0);
                let want = (f | 4) & !(1 | 2 | 32);
                if want != f {
                    nd.set(b"F".to_vec(), Object::Int(want));
                    changed = true;
                }
            }
            if changed {
                doc.set(r, Object::Dict(nd));
                annots_fixed += 1;
            }
        }

        if let Object::Stream(s) = &*o
            && sub.as_deref() == Some(b"Image")
        {
            let mut ns = s.clone();
            if ns.dict.remove(b"OPI").is_some() {
                doc.set(r, Object::Stream(ns));
                images_fixed += 1;
            }
        }
    }

    if annots_fixed > 0 {
        fixed.push(format!("{annots_fixed} annotation(s): normalized print flags and stripped actions"));
    }
    if images_fixed > 0 {
        fixed.push(format!("{images_fixed} image(s): removed OPI dictionary"));
    }

    let remaining = verify(doc, level);
    Ok(Report { fixed, remaining })
}

fn make_pdfx_xmp(doc: &Document, level: PdfxLevel) -> String {
    let mut props = String::new();
    if let Some(t) = info(doc, "Title") {
        props.push_str(&format!("   <dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:title>\n", escape_xml(&t)));
    }
    if let Some(a) = info(doc, "Author") {
        props.push_str(&format!("   <dc:creator><rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq></dc:creator>\n", escape_xml(&a)));
    }
    if let Some(p) = info(doc, "Producer") {
        props.push_str(&format!("   <pdf:Producer>{}</pdf:Producer>\n", escape_xml(&p)));
    }
    if let Some(c) = info(doc, "CreationDate").and_then(|d| xmp::iso_date(&d)) {
        props.push_str(&format!("   <xmp:CreateDate>{c}</xmp:CreateDate>\n"));
    }
    if let Some(m) = info(doc, "ModDate").and_then(|d| xmp::iso_date(&d)) {
        props.push_str(&format!("   <xmp:ModifyDate>{m}</xmp:ModifyDate>\n"));
    }

    let conf_tag = if let Some(conf) = level.conformance_str() {
        format!("   <pdfx:GTS_PDFXConformance>{conf}</pdfx:GTS_PDFXConformance>\n")
    } else {
        String::new()
    };

    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
 <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
  <rdf:Description rdf:about=\"\"\n\
    xmlns:dc=\"http://purl.org/dc/elements/1.1/\"\n\
    xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\"\n\
    xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"\n\
    xmlns:pdfx=\"http://ns.adobe.com/pdfx/1.3/\"\n\
    xmlns:pdfxid=\"http://www.npes.org/pdfx/ns/id/\">\n\
   <pdfxid:GTS_PDFXVersion>{}</pdfxid:GTS_PDFXVersion>\n\
   <pdfx:GTS_PDFXVersion>{}</pdfx:GTS_PDFXVersion>\n\
{conf_tag}\
{props}  </rdf:Description>\n\
 </rdf:RDF>\n\
</x:xmpmeta>\n\
<?xpacket end=\"w\"?>",
        level.version_str(),
        level.version_str()
    )
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
