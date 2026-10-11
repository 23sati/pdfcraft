//! pdfcraft-edit — page content editing (L4). Today: page marks, Acrobat's Header & footer,
//! Watermark, Background and Bates numbering (execution plan M7.6).
//!
//! **How marks are stored.** Each mark is one content stream appended to (or, for content behind
//! the page, prepended to) the page's `/Contents`, drawn in display space (after `/Rotate`) and
//! wrapped in a marked-content artifact (§14.8.2.2) so readers and screen readers skip it:
//! `/Artifact <</Type /Pagination /Subtype /Header /PCMark /HeaderFooter>> BDC … EMC`. The
//! `/PCMark` entry (also in the stream dictionary) names the kind of mark, which is how Update and
//! Remove find them again without decompressing page content. When
//! anything is drawn on top, the page's original content is wrapped in `q … Q` (two tiny streams)
//! so its graphics state can't leak into the mark.
//!
//! Text uses standard Helvetica (WinAnsi) added to the page resources as `/PCHelv`.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use pdfcraft_cos::{Dict, Document, ObjRef, Object, PdfString, Stream};
use pdfcraft_fonts::{helvetica_width, literal, win_ansi};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum EditError {
    #[error("page {0} does not exist")]
    NoSuchPage(usize),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Cos(#[from] pdfcraft_cos::CosError),
}

/// The kinds of page marks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    HeaderFooter,
    Watermark,
    Background,
}

impl MarkKind {
    fn tag(self) -> &'static str {
        match self {
            MarkKind::HeaderFooter => "HeaderFooter",
            MarkKind::Watermark => "Watermark",
            MarkKind::Background => "Background",
        }
    }
}

pub type Rgb = [f64; 3];

/// Header and footer settings (Acrobat's Add Header and Footer dialog).
#[derive(Clone, Debug, PartialEq)]
pub struct HeaderFooter {
    /// Left, centre and right header text, then left, centre and right footer text. Tokens:
    /// `<<1>>` page number, `<<n>>` page count, `<<1 of n>>`, `<<1/n>>`, `<<Page 1>>`,
    /// `<<Page 1 of n>>`, dates `<<m/d/yyyy>>`, `<<mm/dd/yyyy>>`, `<<d/m/yyyy>>`, `<<yyyy-mm-dd>>`,
    /// `<<m/d/yy>>`, `<<mmmm d, yyyy>>`, and Bates numbers `<<Bates Number#6#1#PRE#SUF>>`
    /// (digits, start, prefix, suffix).
    pub text: [String; 6],
    pub font_size: f64,
    pub color: Rgb,
    pub underline: bool,
    /// Margins in points: top, bottom, left, right (the text's distance from the page edges).
    pub margins: [f64; 4],
    /// The number `<<1>>` shows on the first page of the range.
    pub start_number: u32,
}

impl Default for HeaderFooter {
    /// Acrobat's defaults: 8 pt black, margins 0.5 in top and bottom, 1 in left and right.
    fn default() -> Self {
        Self { text: Default::default(), font_size: 8.0, color: [0.0; 3], underline: false, margins: [36.0, 36.0, 72.0, 72.0], start_number: 1 }
    }
}

/// A picture for a background or watermark (Acrobat: Source ▸ File): an XObject already in
/// the document, an image (drawn into a unit square) or a form (a page of a PDF).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkSource {
    pub xobject: pdfcraft_cos::ObjRef,
    /// Its natural size in points.
    pub size: (f64, f64),
    /// An image XObject (unit square); otherwise a form whose `/Matrix` maps it to `size`.
    pub image: bool,
}

/// A watermark: text, or a picture from a file (`source`; the text is then ignored).
#[derive(Clone, Debug, PartialEq)]
pub struct Watermark {
    pub text: String,
    pub source: Option<MarkSource>,
    /// Pictures: size relative to the page (1.0 fits the page).
    pub scale: f64,
    /// 0 = fit: about half the page diagonal.
    pub font_size: f64,
    pub color: Rgb,
    /// 0–1.
    pub opacity: f64,
    /// Degrees counter-clockwise (Acrobat's 45° runs from bottom-left to top-right).
    pub rotation: f64,
    /// Draw behind the page content instead of on top.
    pub behind: bool,
    /// Offset of the centre from the page centre, in points (right, up).
    pub offset: [f64; 2],
    /// Show it on screen, when printing, or both.
    pub visibility: Visibility,
}

/// Where a watermark shows. Both (the default) draws it as before, in no layer of its own; any
/// other choice puts it in an optional-content group (a layer) that tells every reader whether
/// it is visible on screen and whether it is printed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Visibility {
    pub screen: bool,
    pub print: bool,
}

impl Default for Visibility {
    fn default() -> Self {
        Self { screen: true, print: true }
    }
}

impl Default for Watermark {
    fn default() -> Self {
        Self {
            text: String::new(),
            source: None,
            scale: 0.5,
            font_size: 0.0,
            color: [0.6, 0.6, 0.6],
            opacity: 0.5,
            rotation: 45.0,
            behind: false,
            offset: [0.0; 2],
            visibility: Visibility::default(),
        }
    }
}

/// A background behind the page content: a solid colour, or a picture from a file.
#[derive(Clone, Debug, PartialEq)]
pub struct Background {
    pub color: Rgb,
    pub opacity: f64,
    pub source: Option<MarkSource>,
    /// Pictures: size relative to the page (1.0 fits the page).
    pub scale: f64,
}

impl Default for Background {
    fn default() -> Self {
        Self { color: [1.0, 1.0, 0.85], opacity: 1.0, source: None, scale: 1.0 }
    }
}

/// Drawing a picture mark: fitted to the page at `scale`, centred (plus `offset`), turned by
/// `rotation` degrees. Returns the content and the resource name it uses.
fn picture(src: &MarkSource, page: (f64, f64), scale: f64, rotation: f64, offset: [f64; 2]) -> String {
    let (w, h) = page;
    let (sw, sh) = (src.size.0.max(0.01), src.size.1.max(0.01));
    let k = (w / sw).min(h / sh) * if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    let (dw, dh) = (sw * k, sh * k);
    let (s, c) = rotation.to_radians().sin_cos();
    let (cx, cy) = (w / 2.0 + offset[0], h / 2.0 + offset[1]);
    // Centre, rotate, then place the picture's box (unit square for images).
    let place = if src.image {
        format!("{} 0 0 {} {} {} cm", n(dw), n(dh), n(-dw / 2.0), n(-dh / 2.0))
    } else {
        format!("{} 0 0 {} {} {} cm", n(k), n(k), n(-dw / 2.0), n(-dh / 2.0))
    };
    format!("{} {} {} {} {} {} cm\n{place}\n/PCPic{} Do\n", n(c), n(s), n(-s), n(c), n(cx), n(cy), src.xobject.num)
}

/// Register a mark picture in the page's own resources (`/PCPic<num>`).
fn add_picture_resource(doc: &mut Document, page: &pdfcraft_model::Page, src: &MarkSource) -> Result<(), EditError> {
    let p = doc.get(page.obj).as_dict().cloned().unwrap_or_default();
    let mut res = p.get(b"Resources").map(|r| doc.resolve(r)).and_then(|r| r.as_dict().cloned()).unwrap_or_default();
    let mut xo = res.get(b"XObject").map(|x| doc.resolve(x)).and_then(|x| x.as_dict().cloned()).unwrap_or_default();
    xo.set(format!("PCPic{}", src.xobject.num).into_bytes(), Object::Ref(src.xobject));
    res.set(b"XObject".to_vec(), Object::Dict(xo));
    doc.update_dict(page.obj, |d| d.set(b"Resources".to_vec(), Object::Dict(res)))?;
    Ok(())
}

/// The catalog's reference and a copy of its dictionary.
fn catalog(doc: &Document) -> Option<(ObjRef, Dict)> {
    let root = doc.trailer().get(b"Root")?.as_ref()?;
    Some((root, doc.get(root).as_dict()?.clone()))
}

/// The layer configuration `/OCProperties` and its default configuration `/D`, as copies (empty
/// when the document has none).
fn layer_config(doc: &Document, cat: &Dict) -> (Dict, Dict) {
    let props = cat.get(b"OCProperties").map(|p| doc.resolve(p)).and_then(|p| p.as_dict().cloned()).unwrap_or_default();
    let config = props.get(b"D").map(|d| doc.resolve(d)).and_then(|d| d.as_dict().cloned()).unwrap_or_default();
    (props, config)
}

/// A copy of the array `o` (which may be indirect); empty when it is not one.
fn array_of(doc: &Document, o: Option<&Object>) -> Vec<Object> {
    o.map(|o| doc.resolve(o)).and_then(|o| o.as_array().cloned()).unwrap_or_default()
}

/// The resource name a page uses for the layer of a watermark.
fn layer_name(layer: ObjRef) -> String {
    format!("PCOC{}", layer.num)
}

/// Add the optional-content group of a watermark that is not shown everywhere, and record it in
/// the document's layer configuration without touching the layers that are already there.
///
/// The group's usage says whether it is visible on screen and whether it is printed, which is
/// what readers apply for those events (`/AS`). Its default state is its on-screen state, so a
/// viewer that only knows on and off shows a screen-only mark and hides a print-only one.
fn add_layer(doc: &mut Document, visibility: Visibility) -> Result<ObjRef, EditError> {
    let state = |on: bool| Object::name(if on { "ON" } else { "OFF" });
    let (mut view, mut print) = (Dict::new(), Dict::new());
    view.set(b"ViewState".to_vec(), state(visibility.screen));
    print.set(b"PrintState".to_vec(), state(visibility.print));
    let mut usage = Dict::new();
    usage.set(b"View".to_vec(), Object::Dict(view));
    usage.set(b"Print".to_vec(), Object::Dict(print));
    let mut group = Dict::new();
    group.set(b"Type".to_vec(), Object::name("OCG"));
    group.set(b"Name".to_vec(), Object::String(PdfString::text("Watermark")));
    group.set(b"PCMark".to_vec(), Object::name(MarkKind::Watermark.tag()));
    group.set(b"Usage".to_vec(), Object::Dict(usage));
    let layer = doc.add(Object::Dict(group));

    let (root, cat) = catalog(doc).ok_or_else(|| EditError::Invalid("the document has no catalog".into()))?;
    let (mut props, mut config) = layer_config(doc, &cat);
    let mut groups = array_of(doc, props.get(b"OCGs"));
    groups.push(Object::Ref(layer));
    props.set(b"OCGs".to_vec(), Object::Array(groups));
    // Default state: on screen, or not.
    let base_on = config.name(b"BaseState") != Some(&b"OFF"[..]);
    let (key, wanted) = if visibility.screen { (&b"ON"[..], !base_on) } else { (&b"OFF"[..], true) };
    if wanted {
        let mut list = array_of(doc, config.get(key));
        list.push(Object::Ref(layer));
        config.set(key.to_vec(), Object::Array(list));
    }
    // One usage application per event, joined to an existing one for the same event and category.
    let mut applications = array_of(doc, config.get(b"AS"));
    for event in ["View", "Print"] {
        let same = |o: &Object| {
            let entry = doc.resolve(o);
            let Some(d) = entry.as_dict() else { return false };
            let categories = array_of(doc, d.get(b"Category"));
            d.name(b"Event") == Some(event.as_bytes())
                && categories.len() == 1
                && categories.first().and_then(Object::as_name) == Some(event.as_bytes())
        };
        match applications.iter().position(same).and_then(|at| applications.get_mut(at)) {
            Some(slot) => {
                let mut d = doc.resolve(slot).as_dict().cloned().unwrap_or_default();
                let mut covered = array_of(doc, d.get(b"OCGs"));
                covered.push(Object::Ref(layer));
                d.set(b"OCGs".to_vec(), Object::Array(covered));
                *slot = Object::Dict(d);
            }
            None => {
                let mut d = Dict::new();
                d.set(b"Event".to_vec(), Object::name(event));
                d.set(b"Category".to_vec(), Object::Array(vec![Object::name(event)]));
                d.set(b"OCGs".to_vec(), Object::Array(vec![Object::Ref(layer)]));
                applications.push(Object::Dict(d));
            }
        }
    }
    config.set(b"AS".to_vec(), Object::Array(applications));
    props.set(b"D".to_vec(), Object::Dict(config));
    doc.update_dict(root, |c| c.set(b"OCProperties".to_vec(), Object::Dict(props)))?;
    Ok(layer)
}

/// Let a page's marked content name a watermark's layer (`/Properties`, `/PCOC<n>`).
fn add_layer_resource(doc: &mut Document, page: &pdfcraft_model::Page, layer: ObjRef) -> Result<(), EditError> {
    let mut res = page.dict.get(b"Resources").map(|r| doc.resolve(r)).and_then(|r| r.as_dict().cloned()).unwrap_or_default();
    let mut properties = res.get(b"Properties").map(|p| doc.resolve(p)).and_then(|p| p.as_dict().cloned()).unwrap_or_default();
    properties.set(layer_name(layer).into_bytes(), Object::Ref(layer));
    res.set(b"Properties".to_vec(), Object::Dict(properties));
    doc.update_dict(page.obj, |d| d.set(b"Resources".to_vec(), Object::Dict(res)))?;
    Ok(())
}

/// After watermarks were removed from `pages`: take their layers off those pages and, for the
/// groups no page uses any more, out of the document's layer configuration, so the document is
/// as it was before. Layers that are not a watermark's are never touched.
fn release_watermark_layers(doc: &mut Document, pages: &[usize]) -> Result<(), EditError> {
    let Some((root, cat)) = catalog(doc) else { return Ok(()) };
    let (props, _) = layer_config(doc, &cat);
    let ours: Vec<ObjRef> = array_of(doc, props.get(b"OCGs"))
        .iter()
        .filter_map(Object::as_ref)
        .filter(|r| doc.get(*r).as_dict().and_then(|d| d.name(b"PCMark")) == Some(MarkKind::Watermark.tag().as_bytes()))
        .collect();
    if ours.is_empty() {
        return Ok(());
    }
    let pages_now = page_list(doc);
    for &i in pages {
        let Some(page) = pages_now.get(i) else { continue };
        let mut res = page.dict.get(b"Resources").map(|r| doc.resolve(r)).and_then(|r| r.as_dict().cloned()).unwrap_or_default();
        let Some(mut properties) = res.get(b"Properties").map(|p| doc.resolve(p)).and_then(|p| p.as_dict().cloned()) else { continue };
        let gone: Vec<Vec<u8>> = properties
            .iter()
            .filter(|(name, value)| name.starts_with(b"PCOC") && Object::as_ref(value).is_some_and(|r| ours.contains(&r)))
            .map(|(name, _)| name.clone())
            .collect();
        if gone.is_empty() {
            continue;
        }
        for name in &gone {
            properties.remove(name);
        }
        if properties.is_empty() {
            res.remove(b"Properties");
        } else {
            res.set(b"Properties".to_vec(), Object::Dict(properties));
        }
        doc.update_dict(page.obj, |d| d.set(b"Resources".to_vec(), Object::Dict(res)))?;
    }
    // The groups some page still draws.
    let mut used: Vec<ObjRef> = Vec::new();
    for page in page_list(doc) {
        let properties = page.dict.get(b"Resources").map(|r| doc.resolve(r)).and_then(|r| r.as_dict().cloned());
        let properties = properties.and_then(|r| r.get(b"Properties").map(|p| doc.resolve(p))).and_then(|p| p.as_dict().cloned());
        used.extend(properties.iter().flat_map(|p| p.iter()).filter_map(|(_, v)| v.as_ref()).filter(|r| ours.contains(r)));
    }
    let unused: Vec<ObjRef> = ours.into_iter().filter(|r| !used.contains(r)).collect();
    if unused.is_empty() {
        return Ok(());
    }
    let (mut props, mut config) = layer_config(doc, &cat);
    let without = |doc: &Document, o: Option<&Object>| -> Vec<Object> {
        array_of(doc, o).into_iter().filter(|g| g.as_ref().is_none_or(|r| !unused.contains(&r))).collect()
    };
    let groups = without(doc, props.get(b"OCGs"));
    // A list our groups were the only ones in goes too, as it was not there before them.
    for key in [&b"ON"[..], b"OFF", b"Order"] {
        if config.contains(key) {
            let rest = without(doc, config.get(key));
            if rest.is_empty() {
                config.remove(key);
            } else {
                config.set(key.to_vec(), Object::Array(rest));
            }
        }
    }
    if config.contains(b"AS") {
        let mut kept = Vec::new();
        for entry in array_of(doc, config.get(b"AS")) {
            let mut d = doc.resolve(&entry).as_dict().cloned().unwrap_or_default();
            let before = array_of(doc, d.get(b"OCGs")).len();
            let after = without(doc, d.get(b"OCGs"));
            // An application that only covered the removed groups has nothing left to do.
            if before > 0 && after.is_empty() {
                continue;
            }
            if before != after.len() {
                d.set(b"OCGs".to_vec(), Object::Array(after));
                kept.push(Object::Dict(d));
            } else {
                kept.push(entry);
            }
        }
        if kept.is_empty() {
            config.remove(b"AS");
        } else {
            config.set(b"AS".to_vec(), Object::Array(kept));
        }
    }
    if groups.is_empty() {
        doc.update_dict(root, |c| {
            c.remove(b"OCProperties");
        })?;
    } else {
        props.set(b"OCGs".to_vec(), Object::Array(groups));
        props.set(b"D".to_vec(), Object::Dict(config));
        doc.update_dict(root, |c| c.set(b"OCProperties".to_vec(), Object::Dict(props)))?;
    }
    Ok(())
}

/// Values tokens need that come from outside the document.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Context {
    /// Today's date: (year, month 1–12, day 1–31).
    pub date: (i64, u32, u32),
}

fn n(v: f64) -> String {
    let s = format!("{:.3}", if v.abs() < 5e-4 { 0.0 } else { v });
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn rgb(c: Rgb) -> String {
    let c = c.map(|x| x.clamp(0.0, 1.0));
    format!("{} {} {} rg", n(c[0]), n(c[1]), n(c[2]))
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// Expand `<<…>>` tokens for page `index` (0-based within the range) of `count`.
pub fn expand(template: &str, page_number: u32, count: usize, bates: u64, cx: &Context) -> String {
    let (y, m, d) = cx.date;
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("<<") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find(">>") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let token = &after[..end];
        let p = page_number;
        let value = match token {
            "1" => p.to_string(),
            "n" => count.to_string(),
            "1 of n" => format!("{p} of {count}"),
            "1/n" => format!("{p}/{count}"),
            "Page 1" => format!("Page {p}"),
            "Page 1 of n" => format!("Page {p} of {count}"),
            "m/d" => format!("{m}/{d}"),
            "m/d/yy" => format!("{m}/{d}/{:02}", y.rem_euclid(100)),
            "m/d/yyyy" => format!("{m}/{d}/{y}"),
            "mm/dd/yy" => format!("{m:02}/{d:02}/{:02}", y.rem_euclid(100)),
            "mm/dd/yyyy" => format!("{m:02}/{d:02}/{y}"),
            "d/m/yy" => format!("{d}/{m}/{:02}", y.rem_euclid(100)),
            "d/m/yyyy" => format!("{d}/{m}/{y}"),
            "dd/mm/yyyy" => format!("{d:02}/{m:02}/{y}"),
            "yyyy-mm-dd" => format!("{y}-{m:02}-{d:02}"),
            "mmmm d, yyyy" => format!("{} {d}, {y}", MONTHS[(m.clamp(1, 12) - 1) as usize]),
            t if t.starts_with("Bates Number") => {
                // Bates Number#digits#start#prefix#suffix (start is applied by the caller).
                let parts: Vec<&str> = t.split('#').collect();
                let digits = parts.get(1).and_then(|d| d.parse::<usize>().ok()).unwrap_or(6).clamp(1, 15);
                format!("{}{:0digits$}{}", parts.get(3).unwrap_or(&""), bates, parts.get(4).unwrap_or(&""))
            }
            _ => format!("<<{token}>>"),
        };
        out.push_str(&value);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// The Bates start number in a template (`<<Bates Number#digits#start#…>>`), if any.
fn bates_start(template: &str) -> Option<u64> {
    let i = template.find("<<Bates Number")?;
    let t = &template[i + 2..];
    let t = &t[..t.find(">>")?];
    t.split('#').nth(2).and_then(|s| s.parse().ok()).or(Some(1))
}

fn page_list(doc: &Document) -> Vec<pdfcraft_model::Page> {
    pdfcraft_model::pages(doc)
}

fn check(pages: &[usize], count: usize) -> Result<(), EditError> {
    match pages.iter().find(|p| **p >= count) {
        Some(p) => Err(EditError::NoSuchPage(*p)),
        None => Ok(()),
    }
}

/// The page's `/Contents` as a list of stream references (inline content is promoted).
fn contents(doc: &mut Document, page: &pdfcraft_model::Page) -> Result<Vec<Object>, EditError> {
    let obj = doc.get(page.obj);
    let d = obj.as_dict().cloned().unwrap_or_default();
    Ok(match d.get(b"Contents").cloned() {
        None => Vec::new(),
        Some(Object::Ref(r)) => match doc.get(r).as_array() {
            Some(a) => a.clone(),
            None => vec![Object::Ref(r)],
        },
        Some(Object::Array(a)) => a,
        Some(other) => vec![Object::Ref(doc.add(other))],
    })
}

/// Add the font (and an opacity state) to the page's own resources.
fn add_resources(doc: &mut Document, page: &pdfcraft_model::Page, opacity: Option<f64>, content: Option<&[u8]>) -> Result<(), EditError> {
    let mut res = page.dict.get(b"Resources").map(|r| doc.resolve(r)).and_then(|r| r.as_dict().cloned()).unwrap_or_default();
    let mut fonts = res.get(b"Font").map(|f| doc.resolve(f)).and_then(|f| f.as_dict().cloned()).unwrap_or_default();
    for (name, base) in [(&b"PCHelv"[..], "Helvetica"), (b"PCTimes", "Times-Roman"), (b"PCCour", "Courier")] {
        // Helvetica always (marks use it); the others only when the content names them.
        if name != b"PCHelv" && !content.is_some_and(|c| c.windows(name.len() + 1).any(|w| w[0] == b'/' && &w[1..] == name)) {
            continue;
        }
        let mut font = Dict::new();
        font.set(b"Type".to_vec(), Object::name("Font"));
        font.set(b"Subtype".to_vec(), Object::name("Type1"));
        font.set(b"BaseFont".to_vec(), Object::name(base));
        font.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
        fonts.set(name.to_vec(), Object::Dict(font));
    }
    res.set(b"Font".to_vec(), Object::Dict(fonts));
    if let Some(o) = opacity {
        let mut gs = res.get(b"ExtGState").map(|g| doc.resolve(g)).and_then(|g| g.as_dict().cloned()).unwrap_or_default();
        let mut s = Dict::new();
        s.set(b"Type".to_vec(), Object::name("ExtGState"));
        s.set(b"CA".to_vec(), Object::Real(o));
        s.set(b"ca".to_vec(), Object::Real(o));
        let name = format!("PCGS{}", (o * 100.0).round() as i64);
        gs.set(name.into_bytes(), Object::Dict(s));
        res.set(b"ExtGState".to_vec(), Object::Dict(gs));
    }
    // The page gets its own (possibly copied) resources; shared dictionaries stay untouched.
    doc.update_dict(page.obj, |d| d.set(b"Resources".to_vec(), Object::Dict(res)))?;
    Ok(())
}

const WRAP_OPEN: &[u8] = b"q %PdfCraft\n";
const WRAP_CLOSE: &[u8] = b"Q %PdfCraft\n";

#[cfg(test)]
fn stream_bytes(doc: &Document, o: &Object) -> Option<Vec<u8>> {
    match &*doc.resolve(o) {
        Object::Stream(s) => s.decoded().ok(),
        _ => None,
    }
}

/// The `/PCMark` tag in a content stream's dictionary: a mark kind, or `Wrap` for the q/Q
/// wrapper. Read from the dictionary, so finding marks never decompresses page content.
fn stream_tag(doc: &Document, o: &Object) -> Option<String> {
    let obj = doc.resolve(o);
    obj.as_dict()?.name(b"PCMark").map(|n| String::from_utf8_lossy(n).into_owned())
}

fn tagged(tag: &str) -> Dict {
    let mut d = Dict::new();
    d.set(b"PCMark".to_vec(), Object::name(tag));
    d
}

/// Put a mark's content on a page: behind (prepended) or on top (appended, after wrapping the
/// original content in q/Q).
fn place(doc: &mut Document, page: &pdfcraft_model::Page, kind: MarkKind, content: Vec<u8>, behind: bool) -> Result<(), EditError> {
    place_tagged(doc, page, kind.tag(), content, behind)
}

/// Put content on a page, tagged `tag` (see [`place`]).
fn place_tagged(doc: &mut Document, page: &pdfcraft_model::Page, tag: &str, content: Vec<u8>, behind: bool) -> Result<(), EditError> {
    let mut list = contents(doc, page)?;
    let mark = Object::Ref(doc.add(Object::Stream(Stream::flate(tagged(tag), &content))));
    if behind {
        list.insert(0, mark);
    } else {
        let wrapped = list.iter().any(|o| stream_tag(doc, o).as_deref() == Some("Wrap"));
        let original = list.iter().any(|o| stream_tag(doc, o).is_none());
        if !wrapped && original {
            let open = Object::Ref(doc.add(Object::Stream(Stream::from_raw(tagged("Wrap"), WRAP_OPEN.to_vec()))));
            let close = Object::Ref(doc.add(Object::Stream(Stream::from_raw(tagged("Wrap"), WRAP_CLOSE.to_vec()))));
            // Behind-marks stay outside the wrapper.
            let split = list.iter().position(|o| stream_tag(doc, o).is_none()).unwrap_or(list.len());
            list.insert(split, open);
            list.push(close);
        }
        list.push(mark);
    }
    doc.update_dict(page.obj, |d| d.set(b"Contents".to_vec(), Object::Array(list)))?;
    Ok(())
}

/// Draw `content` on top of a page (0-based) as permanent page content, with standard Helvetica
/// available as `/PCHelv`. The stream is tagged `tag` (not a mark kind, so Remove never takes
/// it away); the original content is wrapped in q/Q first. Used for applied redaction boxes.
pub fn stamp(doc: &mut Document, page: usize, tag: &str, content: Vec<u8>) -> Result<(), EditError> {
    let all = page_list(doc);
    check(&[page], all.len())?;
    add_resources(doc, &all[page], None, Some(&content))?;
    let p = page_list(doc).swap_remove(page);
    place_tagged(doc, &p, tag, content, false)
}

fn begin(kind: MarkKind, subtype: &str, matrix: [f64; 6]) -> String {
    format!(
        "q\n/Artifact <</Type /Pagination /Subtype /{subtype} /PCMark /{}>> BDC\n{} {} {} {} {} {} cm\n",
        kind.tag(),
        n(matrix[0]),
        n(matrix[1]),
        n(matrix[2]),
        n(matrix[3]),
        n(matrix[4]),
        n(matrix[5])
    )
}

const END: &str = "EMC\nQ\n";

/// Refuses page text the standard-14 fonts (WinAnsiEncoding) can't draw: written anyway, each
/// such character would become a `?` on the page (#125). `lines` must split the text the way the
/// caller draws it, so a `\r` that would be drawn is refused too.
pub(crate) fn drawable<'a>(lines: impl IntoIterator<Item = &'a str>) -> Result<(), EditError> {
    match lines.into_iter().find_map(pdfcraft_fonts::first_non_win_ansi) {
        Some(c) => Err(undrawable(c)),
        None => Ok(()),
    }
}

/// The refusal for page text with `c`, which nothing here can draw.
pub(crate) fn undrawable(c: char) -> EditError {
    EditError::Invalid(undrawable_message(c))
}

/// The wording of the refusal for text with `c`, which the standard fonts can't draw.
pub fn undrawable_message(c: char) -> String {
    format!("the standard fonts can't draw \"{c}\" (U+{:04X}); only Western European characters can be added as text for now", u32::from(c))
}

/// The first character of plain `text` that Helvetica/WinAnsi can't draw (line and page breaks
/// and tabs are layout, not glyphs).
pub fn first_undrawable_plain(text: &str) -> Option<char> {
    text.chars().find(|c| !matches!(c, '\r' | '\n' | '\t' | '\u{c}') && !pdfcraft_fonts::win_ansi_encodable(*c))
}

fn text_op(x: f64, y: f64, text: &str) -> Vec<u8> {
    let mut v = format!("1 0 0 1 {} {} Tm ", n(x), n(y)).into_bytes();
    v.extend(literal(&win_ansi(text)));
    v.extend_from_slice(b" Tj\n");
    v
}

/// Add a header and footer to `pages` (0-based). With `replace`, existing headers and footers on
/// those pages are removed first (Acrobat's Replace Existing).
pub fn add_header_footer(doc: &mut Document, pages: &[usize], hf: &HeaderFooter, replace: bool, cx: &Context) -> Result<(), EditError> {
    let all = page_list(doc);
    check(pages, all.len())?;
    if hf.text.iter().all(|t| t.trim().is_empty()) {
        return Err(EditError::Invalid("type the header or footer text first".into()));
    }
    // Tokens only add ASCII (Bates prefixes and suffixes are part of the template), so checking
    // the templates, split as `expand`'s output is, covers every page.
    hf.text.iter().try_for_each(|t| drawable(t.lines()))?;
    if !(hf.font_size.is_finite() && hf.font_size > 0.0 && hf.font_size <= 200.0 && hf.margins.iter().all(|m| m.is_finite() && *m >= 0.0)) {
        return Err(EditError::Invalid("invalid font size or margins".into()));
    }
    // Page numbers count up from start_number, so the last selected page needs the most room.
    // Refuse the whole run before drawing anything: headers applied to some pages and then a
    // failed page number would leave the document half changed.
    let last_number = u64::from(hf.start_number)
        .checked_add(pages.len().saturating_sub(1) as u64)
        .ok_or_else(|| EditError::Invalid("the header page numbers are too large".into()))?;
    if last_number > u64::from(u32::MAX) {
        return Err(EditError::Invalid(format!(
            "the header start number {} leaves no room for {} pages (page numbers stop at {})",
            hf.start_number,
            pages.len(),
            u32::MAX
        )));
    }
    if replace {
        remove_marks(doc, pages, MarkKind::HeaderFooter)?;
    }
    let all = page_list(doc);
    let count = all.len();
    let bates0 = hf.text.iter().find_map(|t| bates_start(t)).unwrap_or(1);
    for (k, &i) in pages.iter().enumerate() {
        let page = &all[i];
        let (w, h) = page.display_size(doc);
        let number =
            u32::try_from(u64::from(hf.start_number) + k as u64).map_err(|_| EditError::Invalid("the header page numbers are too large".into()))?;
        let bates = bates0 + k as u64;
        let size = hf.font_size;
        let [top, bottom, left, right] = hf.margins;
        let mut body: Vec<u8> = format!("BT\n/PCHelv {} Tf\n{}\n", n(size), rgb(hf.color)).into_bytes();
        let mut underlines = String::new();
        for (slot, template) in hf.text.iter().enumerate() {
            if template.trim().is_empty() {
                continue;
            }
            let lines: Vec<String> = expand(template, number, count, bates, cx).lines().map(str::to_owned).collect();
            let header = slot < 3;
            let line_h = size * 1.2;
            for (li, line) in lines.iter().enumerate() {
                let tw = helvetica_width(line, size);
                let x = match slot % 3 {
                    0 => left,
                    1 => (w - tw) / 2.0,
                    _ => w - right - tw,
                };
                // Headers hang below the top margin; footers sit on the bottom margin.
                let y = if header { h - top - size * 0.8 - li as f64 * line_h } else { bottom + (lines.len() - 1 - li) as f64 * line_h };
                body.extend(text_op(x, y, line));
                if hf.underline {
                    underlines.push_str(&format!("{} {} {} {} re f\n", n(x), n(y - size * 0.15), n(tw), n((size * 0.06).max(0.4))));
                }
            }
        }
        body.extend_from_slice(b"ET\n");
        if !underlines.is_empty() {
            body.extend(format!("{}\n{underlines}", rgb(hf.color)).bytes());
        }
        let mut content = begin(MarkKind::HeaderFooter, "Header", page.view_matrix(doc)).into_bytes();
        content.extend(body);
        content.extend_from_slice(END.as_bytes());
        add_resources(doc, page, None, None)?;
        let page = &page_list(doc)[i];
        place(doc, page, MarkKind::HeaderFooter, content, false)?;
    }
    Ok(())
}

/// Add a text watermark to `pages`.
pub fn add_watermark(doc: &mut Document, pages: &[usize], wm: &Watermark, replace: bool) -> Result<(), EditError> {
    let all = page_list(doc);
    check(pages, all.len())?;
    let text = wm.text.trim();
    if text.is_empty() && wm.source.is_none() {
        return Err(EditError::Invalid("type the watermark text first".into()));
    }
    if wm.source.is_none() {
        drawable(text.lines())?;
    }
    if !(wm.opacity.is_finite()
        && wm.rotation.is_finite()
        && wm.font_size.is_finite()
        && wm.font_size >= 0.0
        && wm.offset.iter().all(|o| o.is_finite()))
    {
        return Err(EditError::Invalid("invalid watermark settings".into()));
    }
    if !wm.visibility.screen && !wm.visibility.print {
        return Err(EditError::Invalid("show the watermark on screen, when printing, or both".into()));
    }
    if replace {
        remove_marks(doc, pages, MarkKind::Watermark)?;
    }
    // Shown everywhere: no layer. Otherwise one layer for this watermark, whatever the pages.
    let layer = if wm.visibility == Visibility::default() { None } else { Some(add_layer(doc, wm.visibility)?) };
    // The marked content that puts a copy in the layer, and the end of it.
    let enter = layer.map(|l| format!("/OC /{} BDC\n", layer_name(l))).unwrap_or_default();
    let leave = if layer.is_some() { "EMC\n" } else { "" };
    let opacity = wm.opacity.clamp(0.0, 1.0);
    let lines: Vec<&str> = text.lines().collect();
    for &i in pages {
        let page = page_list(doc)[i].clone();
        let (w, h) = page.display_size(doc);
        if let Some(src) = &wm.source {
            let mut content = begin(MarkKind::Watermark, "Watermark", page.view_matrix(doc));
            content.push_str(&enter);
            content.push_str(&format!("/PCGS{} gs\n", (opacity * 100.0).round() as i64));
            content.push_str(&picture(src, (w, h), wm.scale, wm.rotation, wm.offset));
            content.push_str(leave);
            content.push_str(END);
            add_resources(doc, &page, Some(opacity), None)?;
            add_picture_resource(doc, &page_list(doc)[i].clone(), src)?;
            if let Some(l) = layer {
                add_layer_resource(doc, &page_list(doc)[i].clone(), l)?;
            }
            let page = &page_list(doc)[i];
            place(doc, page, MarkKind::Watermark, content.into_bytes(), wm.behind)?;
            continue;
        }
        let widest = lines.iter().map(|l| helvetica_width(l, 1.0)).fold(0.0, f64::max).max(0.01);
        let size = if wm.font_size > 0.0 { wm.font_size } else { ((w * w + h * h).sqrt() * 0.5 / widest).clamp(6.0, 300.0) };
        let (s, c) = wm.rotation.to_radians().sin_cos();
        let (cx, cy) = (w / 2.0 + wm.offset[0], h / 2.0 + wm.offset[1]);
        let mut content = begin(MarkKind::Watermark, "Watermark", page.view_matrix(doc)).into_bytes();
        content.extend(enter.bytes());
        content.extend(
            format!(
                "/PCGS{} gs\n{} {} {} {} {} {} cm\nBT\n/PCHelv {} Tf\n{}\n",
                (opacity * 100.0).round() as i64,
                n(c),
                n(s),
                n(-s),
                n(c),
                n(cx),
                n(cy),
                n(size),
                rgb(wm.color)
            )
            .bytes(),
        );
        let line_h = size * 1.15;
        let total = line_h * (lines.len() as f64 - 1.0);
        for (li, line) in lines.iter().enumerate() {
            let tw = helvetica_width(line, size);
            content.extend(text_op(-tw / 2.0, total / 2.0 - li as f64 * line_h - size * 0.35, line));
        }
        content.extend_from_slice(b"ET\n");
        content.extend_from_slice(leave.as_bytes());
        content.extend_from_slice(END.as_bytes());
        add_resources(doc, &page, Some(opacity), None)?;
        if let Some(l) = layer {
            add_layer_resource(doc, &page_list(doc)[i].clone(), l)?;
        }
        let page = &page_list(doc)[i];
        place(doc, page, MarkKind::Watermark, content, wm.behind)?;
    }
    Ok(())
}

/// Add a solid-colour background behind the content of `pages`.
pub fn add_background(doc: &mut Document, pages: &[usize], bg: &Background, replace: bool) -> Result<(), EditError> {
    let all = page_list(doc);
    check(pages, all.len())?;
    if replace {
        remove_marks(doc, pages, MarkKind::Background)?;
    }
    let opacity = if bg.opacity.is_finite() { bg.opacity.clamp(0.0, 1.0) } else { 1.0 };
    for &i in pages {
        let page = page_list(doc)[i].clone();
        let (w, h) = page.display_size(doc);
        let mut content = begin(MarkKind::Background, "Background", page.view_matrix(doc));
        match &bg.source {
            Some(src) => {
                content.push_str(&format!("/PCGS{} gs\n", (opacity * 100.0).round() as i64));
                content.push_str(&picture(src, (w, h), bg.scale, 0.0, [0.0; 2]));
            }
            None => content.push_str(&format!("/PCGS{} gs\n{}\n0 0 {} {} re f\n", (opacity * 100.0).round() as i64, rgb(bg.color), n(w), n(h))),
        }
        content.push_str(END);
        add_resources(doc, &page, Some(opacity), None)?;
        if let Some(src) = &bg.source {
            add_picture_resource(doc, &page_list(doc)[i].clone(), src)?;
        }
        let page = &page_list(doc)[i];
        place(doc, page, MarkKind::Background, content.into_bytes(), true)?;
    }
    Ok(())
}

/// Remove marks of `kind` from `pages`; returns how many were removed. When no mark is left on
/// top of a page, the q/Q wrapper goes too, restoring the original content list.
pub fn remove_marks(doc: &mut Document, pages: &[usize], kind: MarkKind) -> Result<usize, EditError> {
    let all = page_list(doc);
    check(pages, all.len())?;
    let mut removed = 0;
    for &i in pages {
        let page = &all[i];
        let list = contents(doc, page)?;
        let before = list.len();
        let mut kept: Vec<Object> = list.into_iter().filter(|o| stream_tag(doc, o).as_deref() != Some(kind.tag())).collect();
        removed += before - kept.len();
        // Marks on top come after the closing wrapper; without any, the wrapper has no purpose.
        let is_wrap = |o: &Object| stream_tag(doc, o).as_deref() == Some("Wrap");
        let close = kept.iter().rposition(&is_wrap);
        let marks_on_top = close.is_some_and(|c| kept[c + 1..].iter().any(|o| stream_tag(doc, o).is_some_and(|t| t != "Wrap")));
        if !marks_on_top {
            kept.retain(|o| !is_wrap(o));
        }
        if before != kept.len() {
            doc.update_dict(page.obj, |d| d.set(b"Contents".to_vec(), Object::Array(kept)))?;
        }
    }
    // A watermark that was in a layer of its own takes the layer with it.
    if kind == MarkKind::Watermark && removed > 0 {
        release_watermark_layers(doc, pages)?;
    }
    Ok(removed)
}

/// Which kinds of marks the document has (for enabling Update/Remove).
pub fn marks_present(doc: &Document) -> Vec<MarkKind> {
    added::scan_markers(doc).marks
}

pub mod objects;
pub use objects::{EditableObject, MAX_MOVE_OBJECTS, ObjectKind, ObjectTarget, editable_objects, move_objects};

mod flatten;
pub use flatten::{flatten, flatten_fill_sign};
pub mod images;
pub use images::{ImageChange, PageImage, change_image, page_images, reading_images, rect_to_rect, turn_about_centre};
pub mod text;
pub use text::{BlockStyle, LineEdit, TextBlock, TextLine, reading_blocks, replace_block, replace_line, rewrite_block, text_blocks, text_lines};
pub mod added;
pub use added::{
    Added, AddedImage, AddedText, Align, Content, Family, MarkerScan, add_content, delete_content, first_undrawable, list_added, scan_markers,
    update_content,
};

#[cfg(test)]
mod tests;
