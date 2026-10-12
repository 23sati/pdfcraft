//! Single preflight fixups and automated profile-based repair.

use pdfcraft_cos::{Dict, Document, ObjRef, Object, PdfString, Stream};
use serde::{Deserialize, Serialize};

use crate::profile::{PreflightProfile, PreflightResult, evaluate_profile};
use crate::{icc, pdfx};

/// A summary of changes made by preflight fixup actions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreflightFixReport {
    pub profile_id: String,
    pub applied_fixups: Vec<String>,
    pub remaining_result: PreflightResult,
}

fn catalog(doc: &Document) -> Dict {
    doc.root().and_then(|r| doc.get(r).as_dict().cloned()).unwrap_or_default()
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

/// Sets `/Trapped` to `/False` in Catalog and Info dictionary.
pub fn fix_missing_trapped(doc: &mut Document) -> bool {
    let mut changed = false;
    let mut info_dict = doc.trailer().get(b"Info").map(|i| doc.resolve(i)).and_then(|i| i.as_dict().cloned()).unwrap_or_default();
    if info_dict.get(b"Trapped").is_none() {
        info_dict.set(b"Trapped".to_vec(), Object::name("False"));
        let r = doc.add(info_dict);
        doc.trailer_mut().set(b"Info".to_vec(), Object::Ref(r));
        changed = true;
    }
    if let Some(root) = doc.root() {
        let _ = doc.update_dict(root, |c| {
            if c.get(b"Trapped").is_none() {
                c.set(b"Trapped".to_vec(), Object::name("False"));
                changed = true;
            }
        });
    }
    changed
}

/// Ensures every page defines TrimBox and BleedBox based on CropBox or MediaBox.
pub fn fix_missing_page_boxes(doc: &mut Document) -> Result<usize, pdfcraft_cos::CosError> {
    let mut fixed = 0;
    for p in pdfcraft_model::pages(doc) {
        let mut pd = p.dict.clone();
        let mut changed = false;
        let base_box = pd
            .get(b"CropBox")
            .or_else(|| pd.get(b"MediaBox"))
            .cloned()
            .unwrap_or_else(|| Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]));
        if pd.get(b"TrimBox").is_none() && pd.get(b"ArtBox").is_none() {
            pd.set(b"TrimBox".to_vec(), base_box.clone());
            changed = true;
        }
        if pd.get(b"BleedBox").is_none() {
            pd.set(b"BleedBox".to_vec(), base_box);
            changed = true;
        }
        if changed {
            doc.update_dict(p.obj, |d| *d = pd)?;
            fixed += 1;
        }
    }
    Ok(fixed)
}

/// Adjusts hairline strokes below `min_width` (default 0.25 pt) in content streams to `min_width`.
pub fn fix_hairlines(doc: &mut Document, min_width: f64) -> Result<usize, pdfcraft_cos::CosError> {
    let mut fixed_pages = 0;
    let min_str = format!("{min_width:.2}");
    for p in pdfcraft_model::pages(doc) {
        let contents_ref = p.dict.get(b"Contents").cloned();
        let Some(contents_obj) = contents_ref else { continue };

        let update_stream = |s: &Stream| -> Option<Stream> {
            let data = s.decoded().ok()?;
            let mut modified = false;
            let mut new_tokens = Vec::new();
            let mut prev_token: Option<Vec<u8>> = None;

            for token in data.split(|b| *b == b'\n' || *b == b' ' || *b == b'\r' || *b == b'\t') {
                if token.is_empty() {
                    continue;
                }
                if token == b"w"
                    && let Some(prev) = &prev_token
                    && let Ok(str_val) = std::str::from_utf8(prev)
                    && let Ok(width) = str_val.parse::<f64>()
                    && width > 0.0
                    && width < min_width
                {
                    // Replace the previous width operand with min_width
                    if let Some(last) = new_tokens.last_mut() {
                        *last = min_str.as_bytes().to_vec();
                    }
                    modified = true;
                }
                new_tokens.push(token.to_vec());
                prev_token = Some(token.to_vec());
            }

            if modified {
                let mut joined = Vec::new();
                for (idx, t) in new_tokens.iter().enumerate() {
                    if idx > 0 {
                        joined.push(b' ');
                    }
                    joined.extend_from_slice(t);
                }
                joined.push(b'\n');
                Some(Stream::flate(s.dict.clone(), &joined))
            } else {
                None
            }
        };

        match contents_obj {
            Object::Ref(r) => {
                if let Object::Stream(s) = &*doc.get(r)
                    && let Some(ns) = update_stream(s)
                {
                    doc.set(r, Object::Stream(ns));
                    fixed_pages += 1;
                }
            }
            Object::Array(arr) => {
                let mut page_changed = false;
                for item in arr {
                    if let Object::Ref(r) = item
                        && let Object::Stream(s) = &*doc.get(r)
                        && let Some(ns) = update_stream(s)
                    {
                        doc.set(r, Object::Stream(ns));
                        page_changed = true;
                    }
                }
                if page_changed {
                    fixed_pages += 1;
                }
            }
            _ => {}
        }
    }
    Ok(fixed_pages)
}

/// Ensures a valid OutputIntent is attached to the catalog.
pub fn fix_output_intent(doc: &mut Document, cmyk_intent: bool) -> Result<bool, pdfcraft_cos::CosError> {
    if pdfx::has_pdfx_intent(doc) || crate::has_pdfa_intent(doc) {
        return Ok(false);
    }
    let root = doc.root().ok_or(pdfcraft_cos::CosError::Syntax { offset: 0, detail: "no catalog".into() })?;
    let mut cat = catalog(doc);

    let (profile_data, condition_id, num_channels, subtype) =
        if cmyk_intent { (icc::cmyk_swop(), "CGATS TR 001", 4, "GTS_PDFX") } else { (icc::srgb(), "sRGB IEC61966-2.1", 3, "GTS_PDFA1") };

    let mut icc_dict = Dict::new();
    icc_dict.set(b"N".to_vec(), Object::Int(num_channels));
    let profile = doc.add(Object::Stream(Stream::flate(icc_dict, &profile_data)));

    let mut oi = Dict::new();
    oi.set(b"Type".to_vec(), Object::name("OutputIntent"));
    oi.set(b"S".to_vec(), Object::name(subtype));
    oi.set(b"OutputConditionIdentifier".to_vec(), PdfString::text(condition_id));
    oi.set(b"Info".to_vec(), PdfString::text(condition_id));
    oi.set(b"DestOutputProfile".to_vec(), Object::Ref(profile));

    let mut list = cat.get(b"OutputIntents").map(|o| doc.resolve(o)).and_then(|o| o.as_array().cloned()).unwrap_or_default();
    list.push(Object::Dict(oi));
    cat.set(b"OutputIntents".to_vec(), Object::Array(list));

    doc.update_dict(root, |c| *c = cat)?;
    Ok(true)
}

/// Strips JavaScript, OpenAction, and additional actions across the document.
pub fn fix_forbidden_actions(doc: &mut Document) -> Result<usize, pdfcraft_cos::CosError> {
    let mut actions = 0;
    if let Some(root) = doc.root() {
        doc.update_dict(root, |c| {
            if c.remove(b"AA").is_some() {
                actions += 1;
            }
            if c.remove(b"OpenAction").is_some() {
                actions += 1;
            }
            if let Some(Object::Dict(mut names)) = c.get(b"Names").cloned()
                && names.remove(b"JavaScript").is_some()
            {
                c.set(b"Names".to_vec(), Object::Dict(names));
                actions += 1;
            }
        })?;
    }
    for p in pdfcraft_model::pages(doc) {
        doc.update_dict(p.obj, |d| {
            if d.remove(b"AA").is_some() {
                actions += 1;
            }
        })?;
    }
    for (r, o) in objects(doc) {
        if let Some(d) = dict_of(&o).cloned()
            && (d.get(b"AA").is_some() || d.get(b"A").is_some())
        {
            let mut nd = d;
            let mut changed = nd.remove(b"AA").is_some();
            changed |= nd.remove(b"A").is_some();
            if changed {
                if matches!(&*o, Object::Dict(_)) {
                    doc.set(r, Object::Dict(nd));
                }
                actions += 1;
            }
        }
    }
    Ok(actions)
}

/// Sets print flags and cleans up forbidden annotation subtypes.
pub fn fix_annotations(doc: &mut Document) -> Result<usize, pdfcraft_cos::CosError> {
    let mut fixed = 0;
    for (r, o) in objects(doc) {
        let Some(d) = dict_of(&o).cloned() else { continue };
        let is_annot = d.name(b"Type") == Some(b"Annot") || (d.get(b"Rect").is_some() && d.get(b"Subtype").is_some());
        if !is_annot {
            continue;
        }
        let sub = d.name(b"Subtype");
        if sub != Some(b"Popup") {
            let f = d.int(b"F").unwrap_or(0);
            let want = (f | 4) & !(1 | 2 | 32);
            if want != f {
                let mut nd = d;
                nd.set(b"F".to_vec(), Object::Int(want));
                if matches!(&*o, Object::Dict(_)) {
                    doc.set(r, Object::Dict(nd));
                    fixed += 1;
                }
            }
        }
    }
    Ok(fixed)
}

/// Removes encryption dictionary from trailer.
pub fn fix_remove_encryption(doc: &mut Document) -> bool {
    if doc.trailer().get(b"Encrypt").is_some() {
        doc.remove_encryption();
        true
    } else {
        false
    }
}

/// Executes all automated fixups configured for the given `profile`.
pub fn run_profile_fixups(doc: &mut Document, profile: &PreflightProfile) -> Result<PreflightFixReport, pdfcraft_cos::CosError> {
    let mut applied_fixups = Vec::new();

    if fix_remove_encryption(doc) {
        applied_fixups.push("Removed document encryption".into());
    }

    if fix_missing_trapped(doc) {
        applied_fixups.push("Added /Trapped False key to document info and catalog".into());
    }

    let boxes_fixed = fix_missing_page_boxes(doc)?;
    if boxes_fixed > 0 {
        applied_fixups.push(format!("{boxes_fixed} page(s): added missing TrimBox and BleedBox"));
    }

    if profile.check_hairlines {
        let hairlines_fixed = fix_hairlines(doc, 0.25)?;
        if hairlines_fixed > 0 {
            applied_fixups.push(format!("{hairlines_fixed} page(s): adjusted hairline strokes to 0.25 pt"));
        }
    }

    if profile.require_output_intent && fix_output_intent(doc, profile.disallow_rgb)? {
        let space = if profile.disallow_rgb { "CMYK (SWOP)" } else { "sRGB" };
        applied_fixups.push(format!("Added default {space} OutputIntent"));
    }

    let actions_fixed = fix_forbidden_actions(doc)?;
    if actions_fixed > 0 {
        applied_fixups.push(format!("{actions_fixed} forbidden or additional action(s) removed"));
    }

    let annots_fixed = fix_annotations(doc)?;
    if annots_fixed > 0 {
        applied_fixups.push(format!("{annots_fixed} annotation(s) set to print and show"));
    }

    let remaining_result = evaluate_profile(doc, profile);

    Ok(PreflightFixReport { profile_id: profile.id.clone(), applied_fixups, remaining_result })
}
