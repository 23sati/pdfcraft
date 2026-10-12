//! Properties of reachable indirect structure elements.

use pdfcraft_cos::{CosError, Document, ObjRef, Object, PdfString};

use crate::alt;

/// A tag that can be addressed by an edit. Inline dictionaries have no stable object reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagInfo {
    pub object: ObjRef,
    pub kind: String,
    pub title: Option<String>,
    pub actual_text: Option<String>,
    pub alt: Option<String>,
    pub language: Option<String>,
    /// Zero-based page when `/Pg` is known.
    pub page: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagProperty {
    Type,
    Title,
    ActualText,
    AlternateText,
    Language,
}

impl TagProperty {
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "type" => Some(Self::Type),
            "title" => Some(Self::Title),
            "actual_text" => Some(Self::ActualText),
            "alternate_text" => Some(Self::AlternateText),
            "language" => Some(Self::Language),
            _ => None,
        }
    }

    fn key(self) -> &'static [u8] {
        match self {
            Self::Type => b"S",
            Self::Title => b"T",
            Self::ActualText => b"ActualText",
            Self::AlternateText => b"Alt",
            Self::Language => b"Lang",
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum TagError {
    #[error("object {0} is not a reachable indirect structure tag")]
    NotATag(ObjRef),
    #[error("a tag type must be a nonempty PDF name using letters, digits, colon, period, underscore or hyphen")]
    InvalidType,
    #[error("tag property text is too long (maximum 65536 bytes)")]
    TooLong,
    #[error(transparent)]
    Cos(#[from] CosError),
}

fn text(doc: &Document, d: &pdfcraft_cos::Dict, key: &[u8]) -> Option<String> {
    d.get(key).map(|o| doc.resolve(o)).and_then(|o| o.as_string().map(PdfString::to_text))
}

/// List reachable, indirect tags in structure-tree order.
pub fn tags(doc: &Document) -> Vec<TagInfo> {
    let pages = pdfcraft_model::pages(doc);
    let page_index: std::collections::HashMap<ObjRef, usize> = pages.iter().enumerate().map(|(i, p)| (p.obj, i)).collect();
    alt::elements(doc)
        .into_iter()
        .filter_map(|(object, d, page)| {
            Some(TagInfo {
                object,
                kind: String::from_utf8_lossy(d.name(b"S")?).into_owned(),
                title: text(doc, &d, b"T"),
                actual_text: text(doc, &d, b"ActualText"),
                alt: text(doc, &d, b"Alt"),
                language: text(doc, &d, b"Lang"),
                page: page.and_then(|p| page_index.get(&p).copied()),
            })
        })
        .collect()
}

/// Change one property of a reachable tag; `None` removes a text property.
pub fn set_tag_property(doc: &mut Document, object: ObjRef, property: TagProperty, value: Option<&str>) -> Result<(), TagError> {
    if !alt::elements(doc).iter().any(|(r, _, _)| *r == object) {
        return Err(TagError::NotATag(object));
    }
    // ActualText may intentionally contain leading or trailing whitespace; preserve it.
    let value = value.filter(|s| !s.is_empty());
    if value.is_some_and(|s| s.len() > 65_536) {
        return Err(TagError::TooLong);
    }
    if property == TagProperty::Type
        && !value.is_some_and(|s| s.len() <= 127 && s.as_bytes().iter().all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'.' | b'_' | b'-')))
    {
        return Err(TagError::InvalidType);
    }
    doc.update_dict(object, |d| match value {
        Some(s) if property == TagProperty::Type => d.set(property.key().to_vec(), Object::name(s)),
        Some(s) => d.set(property.key().to_vec(), Object::String(PdfString::text(s))),
        None => {
            d.remove(property.key());
        }
    })?;
    Ok(())
}
