//! pdfcraft-prepress — print production: the inks a document uses (ISO 32000-2 §8.7 colour
//! spaces, §11.3 overprint).
//!
//! [`inks`] is the data behind Acrobat's Output Preview: the separations a printer would make
//! — the process channels of DeviceGray/RGB/CMYK and ICC-based spaces, the spot inks of
//! `/Separation` and `/DeviceN` spaces (including the special `None` and `All` inks), the
//! highest tint drawn per ink, and the pages that paint with overprint on.
//!
//! It reads content streams and resources; it does not rasterise. So it reports which inks are
//! used and how strongly, not how much area they cover; a colour-setting operator marks its
//! inks as used even if no path is later painted with them; and image and shading paints
//! record their inks without a tint, because those tints live in the image data or a function
//! rather than in the content stream.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::collections::{BTreeSet, HashMap};

use pdfcraft_content::parse;
use pdfcraft_cos::{Dict, Document, Object, Stream};
use pdfcraft_model::Page;

/// Form XObjects nested this deep are not followed (object graphs can be cyclic).
const MAX_DEPTH: usize = 12;

/// What kind of ink a name is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A process channel (Gray, Red/Green/Blue, Cyan/Magenta/Yellow/Black).
    Process,
    /// A named spot ink.
    Spot,
    /// The special `All` ink (a Separation/DeviceN space that paints every ink).
    All,
    /// The special `None` ink (clears what is underneath; never printed).
    None,
}

/// One ink the document uses.
#[derive(Clone, Debug, PartialEq)]
pub struct Ink {
    /// The ink's name: `Cyan`, `PANTONE 300 C`, `All`, `None`, …
    pub name: String,
    pub kind: Kind,
    /// The highest tint drawn, 0..=1. `None` when the ink is only used by images or shadings,
    /// whose tints live outside the content stream.
    pub max_tint: Option<f64>,
    /// The pages (0-based) that use the ink.
    pub pages: Vec<usize>,
}

/// What a document's pages draw with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// The inks, process first (in channel order), then the spot inks by name.
    pub inks: Vec<Ink>,
    /// The pages (0-based) that paint with overprint on.
    pub overprint_pages: Vec<usize>,
}

/// The inks every page uses.
pub fn inks(doc: &Document) -> Report {
    let pages = pdfcraft_model::pages(doc);
    walk(doc, &pages, None)
}

/// The inks one page (0-based) uses; an out-of-range page reports nothing.
pub fn inks_of(doc: &Document, page: usize) -> Report {
    let pages = pdfcraft_model::pages(doc);
    walk(doc, &pages, Some(page))
}

/// A colour space, resolved just far enough to name its inks.
#[derive(Clone, Debug, PartialEq)]
enum Space {
    Gray,
    Rgb,
    Cmyk,
    /// Its base space's inks are used, but at unknown tints.
    Indexed(Box<Space>),
    Separation(String),
    DeviceN(Vec<String>),
    /// No ink can be named (Lab, patterns without an underlying space, unresolvable).
    Other,
}

/// The graphics state the walk keeps: the current colour spaces and overprint flags.
#[derive(Clone, Debug)]
struct State {
    fill: Space,
    stroke: Space,
    op_fill: bool,
    op_stroke: bool,
}

impl State {
    fn initial() -> Self {
        State { fill: Space::Gray, stroke: Space::Gray, op_fill: false, op_stroke: false }
    }
}

/// The process channels of a device space, in canonical order.
fn channels(space: &Space) -> &'static [(&'static str, Kind)] {
    match space {
        Space::Gray => &[("Gray", Kind::Process)],
        Space::Rgb => &[("Red", Kind::Process), ("Green", Kind::Process), ("Blue", Kind::Process)],
        Space::Cmyk => &[("Cyan", Kind::Process), ("Magenta", Kind::Process), ("Yellow", Kind::Process), ("Black", Kind::Process)],
        _ => &[],
    }
}

/// The special ink a name stands for, if any.
fn special(name: &str) -> Kind {
    match name {
        "All" => Kind::All,
        "None" => Kind::None,
        _ => Kind::Spot,
    }
}

/// The order process inks are listed in, as a printer separates them.
fn process_rank(name: &str) -> usize {
    ["Gray", "Red", "Green", "Blue", "Cyan", "Magenta", "Yellow", "Black"].iter().position(|p| *p == name).unwrap_or(usize::MAX)
}

struct Walk<'a> {
    doc: &'a Document,
    /// (kind, name) → (highest known tint, pages).
    inks: HashMap<(Kind, String), (Option<f64>, BTreeSet<usize>)>,
    overprint: BTreeSet<usize>,
    /// The page's resources, which a form XObject without its own falls back to.
    page_res: Dict,
}

fn walk(doc: &Document, pages: &[Page], only: Option<usize>) -> Report {
    let mut w = Walk { doc, inks: HashMap::new(), overprint: BTreeSet::new(), page_res: Dict::new() };
    for (i, page) in pages.iter().enumerate() {
        if only.is_some_and(|o| o != i) {
            continue;
        }
        let data = content(doc, &page.dict);
        let res = resources(doc, &page.dict);
        w.page_res = res.clone();
        w.run(&data, &res, i, 0, &State::initial());
    }
    w.report()
}

impl<'a> Walk<'a> {
    fn report(self) -> Report {
        let mut inks: Vec<Ink> = self
            .inks
            .into_iter()
            .map(|((kind, name), (tint, pages))| Ink { name, kind, max_tint: tint, pages: pages.into_iter().collect() })
            .collect();
        inks.sort_by_key(|i| match i.kind {
            Kind::Process => (0, process_rank(&i.name), String::new()),
            _ => (1, 0, i.name.clone()),
        });
        Report { inks, overprint_pages: self.overprint.into_iter().collect() }
    }

    /// A colour-setting operator (or an image or shading paint) used `space` at `tints`
    /// (`None` when the tints aren't in the content stream).
    fn use_space(&mut self, space: &Space, tints: Option<&[f64]>, page: usize) {
        let one = |w: &mut Self, name: &str, kind: Kind, tint: Option<f64>| {
            let e = w.inks.entry((kind, name.to_string())).or_insert((None, BTreeSet::new()));
            e.1.insert(page);
            // A tint only ever raises the highest seen; an image or shading paint (no tint)
            // leaves it as it is, or absent.
            if let Some(t) = tint {
                e.0 = Some(e.0.map_or(t, |k| k.max(t)));
            }
        };
        match space {
            Space::Gray | Space::Rgb | Space::Cmyk => {
                for (i, (name, kind)) in channels(space).iter().enumerate() {
                    let t = tints.and_then(|t| t.get(i)).map(|v| v.clamp(0.0, 1.0));
                    one(self, name, *kind, t);
                }
            }
            Space::Indexed(base) => self.use_space(base, None, page),
            Space::Separation(name) => {
                let kind = special(name);
                let t = tints.and_then(|t| t.first()).map(|v| v.clamp(0.0, 1.0));
                one(self, name, kind, t);
            }
            Space::DeviceN(names) => {
                for (i, name) in names.iter().enumerate() {
                    let kind = special(name);
                    let t = tints.and_then(|t| t.get(i)).map(|v| v.clamp(0.0, 1.0));
                    one(self, name, kind, t);
                }
            }
            Space::Other => {}
        }
    }

    /// Paint under overprint marks the page.
    fn painted(&mut self, fill: bool, stroke: bool, state: &State, page: usize) {
        if (fill && state.op_fill) || (stroke && state.op_stroke) {
            self.overprint.insert(page);
        }
    }

    /// Walk one content scope (a page's streams or a form XObject's).
    fn run(&mut self, data: &[u8], res: &Dict, page: usize, depth: usize, state: &State) {
        let cs = sub(res, b"ColorSpace");
        let mut cur = state.clone();
        let mut stack: Vec<State> = Vec::new();
        for op in parse(data).ops {
            match op.op.as_slice() {
                b"q" => stack.push(cur.clone()),
                b"Q" => {
                    if let Some(s) = stack.pop() {
                        cur = s;
                    }
                }
                b"cs" => {
                    if let Some(n) = op.name(0) {
                        cur.fill = self.space(cs.as_ref(), &Object::Name(n.to_vec()), depth);
                    }
                }
                b"CS" => {
                    if let Some(n) = op.name(0) {
                        cur.stroke = self.space(cs.as_ref(), &Object::Name(n.to_vec()), depth);
                    }
                }
                b"g" => {
                    cur.fill = Space::Gray;
                    let v: Vec<f64> = op.operands.iter().filter_map(Object::as_f64).collect();
                    self.use_space(&Space::Gray, Some(&v), page);
                }
                b"G" => {
                    cur.stroke = Space::Gray;
                    let v: Vec<f64> = op.operands.iter().filter_map(Object::as_f64).collect();
                    self.use_space(&Space::Gray, Some(&v), page);
                }
                b"rg" => {
                    cur.fill = Space::Rgb;
                    let v: Vec<f64> = op.operands.iter().filter_map(Object::as_f64).collect();
                    self.use_space(&Space::Rgb, Some(&v), page);
                }
                b"RG" => {
                    cur.stroke = Space::Rgb;
                    let v: Vec<f64> = op.operands.iter().filter_map(Object::as_f64).collect();
                    self.use_space(&Space::Rgb, Some(&v), page);
                }
                b"k" => {
                    cur.fill = Space::Cmyk;
                    let v: Vec<f64> = op.operands.iter().filter_map(Object::as_f64).collect();
                    self.use_space(&Space::Cmyk, Some(&v), page);
                }
                b"K" => {
                    cur.stroke = Space::Cmyk;
                    let v: Vec<f64> = op.operands.iter().filter_map(Object::as_f64).collect();
                    self.use_space(&Space::Cmyk, Some(&v), page);
                }
                b"sc" | b"scn" => {
                    let v: Vec<f64> = op.operands.iter().filter_map(Object::as_f64).collect();
                    self.use_space(&cur.fill, Some(&v), page);
                }
                b"SC" | b"SCN" => {
                    let v: Vec<f64> = op.operands.iter().filter_map(Object::as_f64).collect();
                    self.use_space(&cur.stroke, Some(&v), page);
                }
                b"gs" => {
                    if let Some(n) = op.name(0) {
                        let eg = sub(res, b"ExtGState").and_then(|d| d.get(n).cloned()).map(|o| self.doc.resolve(&o));
                        if let Some(g) = eg.as_deref().and_then(Object::as_dict) {
                            // /op defaults to /OP's value (ISO 32000-2 §11.3.5).
                            let stroking = matches!(g.get(b"OP"), Some(Object::Bool(true)));
                            cur.op_stroke = stroking;
                            cur.op_fill = match g.get(b"op") {
                                Some(Object::Bool(b)) => *b,
                                _ => stroking,
                            };
                        }
                    }
                }
                b"f" | b"F" | b"f*" | b"b" | b"b*" | b"B" | b"B*" => self.painted(true, false, &cur, page),
                b"s" | b"S" => self.painted(false, true, &cur, page),
                b"n" => self.painted(true, true, &cur, page),
                b"Tj" | b"TJ" | b"'" | b"\"" => self.painted(true, false, &cur, page),
                b"sh" => {
                    self.painted(true, false, &cur, page);
                    // A shading names its colour space; its tints live in the shading.
                    if let Some(n) = op.name(0) {
                        let shading = sub(res, b"Shading").and_then(|d| d.get(n).cloned());
                        let space = shading.as_ref().and_then(|o| self.doc.resolve(o).as_dict().and_then(|d| d.get(b"ColorSpace").cloned()));
                        if let Some(space) = space {
                            let space = self.space(cs.as_ref(), &space, depth);
                            self.use_space(&space, None, page);
                        }
                    }
                }
                b"Do" => {
                    let Some(n) = op.name(0) else { continue };
                    let Some(xo) = sub(res, b"XObject").and_then(|d| d.get(n).cloned()) else { continue };
                    let Object::Stream(s) = &*self.doc.resolve(&xo) else { continue };
                    self.do_xobject(s, &cur, page, depth);
                }
                b"BI" => {
                    if let Some((dict, _)) = &op.inline {
                        let space = self.inline_space(dict, cs.as_ref(), depth);
                        self.use_space(&space, None, page);
                    }
                }
                _ => {}
            }
        }
    }

    /// A form XObject's content runs in the caller's state; an image paints its own
    /// colour space at unknown tints (ISO 32000-2 §8.9.7, §8.9.9).
    fn do_xobject(&mut self, s: &Stream, state: &State, page: usize, depth: usize) {
        match s.dict.name(b"Subtype") {
            Some(b"Form") if depth < MAX_DEPTH => {
                // A form names its own resources; one that has none draws with the page's
                // (what viewers do), so it is walked with those.
                let data = s.decoded().unwrap_or_default();
                let res = sub(&s.dict, b"Resources").unwrap_or_else(|| self.page_res.clone());
                self.run(&data, &res, page, depth + 1, state);
            }
            Some(b"Image") => {
                let space = s.dict.get(b"ColorSpace").cloned();
                if let Some(space) = space {
                    let space = self.space(None, &space, depth);
                    self.use_space(&space, None, page);
                }
            }
            _ => {}
        }
    }

    /// An inline image's colour space, with the abbreviated names inline images use
    /// (`/CS /G`, `/CS /RGB`, `/CS /CMYK`, ISO 32000-2 §8.9.7).
    fn inline_space(&mut self, dict: &Dict, cs: Option<&Dict>, depth: usize) -> Space {
        let raw = dict.get(b"CS").or_else(|| dict.get(b"ColorSpace"));
        match raw.and_then(Object::as_name) {
            Some(b"G") => Space::Gray,
            Some(b"RGB") => Space::Rgb,
            Some(b"CMYK") => Space::Cmyk,
            Some(n) => self.space(cs, &Object::Name(n.to_vec()), depth),
            None => match raw {
                Some(o) => self.space(cs, o, depth),
                None => Space::Gray,
            },
        }
    }

    /// Resolve a colour space object to the inks it names.
    fn space(&mut self, cs: Option<&Dict>, o: &Object, depth: usize) -> Space {
        if depth > MAX_DEPTH {
            return Space::Other;
        }
        let resolved = self.doc.resolve(o);
        match &*resolved {
            Object::Name(n) => match n.as_slice() {
                b"DeviceGray" | b"G" => self.defaulted(cs, b"DefaultGray", depth),
                b"DeviceRGB" | b"RGB" => self.defaulted(cs, b"DefaultRGB", depth),
                b"DeviceCMYK" | b"CMYK" => self.defaulted(cs, b"DefaultCMYK", depth),
                b"Pattern" => Space::Other,
                // A named space from the resources; an unknown name has no ink to report.
                other => cs.and_then(|d| d.get(other).cloned()).map(|o| self.space(cs, &o, depth + 1)).unwrap_or(Space::Other),
            },
            Object::Array(a) => {
                let Some(head) = a.first().and_then(|o| self.doc.resolve(o).as_name().map(|n| n.to_vec())) else {
                    return Space::Other;
                };
                match head.as_slice() {
                    b"ICCBased" => match a.get(1).map(|o| self.doc.resolve(o)).as_deref().and_then(|o| match o {
                        Object::Stream(s) => s.dict.int(b"N"),
                        _ => None,
                    }) {
                        Some(1) => Space::Gray,
                        Some(3) => Space::Rgb,
                        Some(4) => Space::Cmyk,
                        _ => Space::Other,
                    },
                    b"Indexed" => a.get(1).map(|o| self.space(cs, o, depth + 1)).map(|s| Space::Indexed(Box::new(s))).unwrap_or(Space::Other),
                    // An uncoloured pattern paints in its underlying space.
                    b"Pattern" => a.get(1).map(|o| self.space(cs, o, depth + 1)).unwrap_or(Space::Other),
                    b"Separation" => a.get(1).and_then(ink_name).map(Space::Separation).unwrap_or(Space::Other),
                    b"DeviceN" => a
                        .get(1)
                        .map(|o| self.doc.resolve(o))
                        .as_deref()
                        .and_then(Object::as_array)
                        .map(|names| names.iter().filter_map(ink_name).collect::<Vec<_>>())
                        .filter(|n| !n.is_empty())
                        .map(Space::DeviceN)
                        .unwrap_or(Space::Other),
                    b"CalGray" => Space::Gray,
                    b"CalRGB" => Space::Rgb,
                    _ => Space::Other,
                }
            }
            _ => Space::Other,
        }
    }

    /// A device space, honouring the resources' `DefaultGray`/`DefaultRGB`/`DefaultCMYK`
    /// (ISO 32000-2 §8.6.3): with a default, drawing in that device space uses the default.
    fn defaulted(&mut self, cs: Option<&Dict>, key: &[u8], depth: usize) -> Space {
        match cs.and_then(|d| d.get(key).cloned()) {
            Some(o) => {
                // A default that names itself would recurse forever; one level is enough.
                let d = self.doc.resolve(&o);
                match &*d {
                    Object::Name(n) if matches!(key, b"DefaultGray" | b"DefaultRGB" | b"DefaultCMYK") => match n.as_slice() {
                        b"DeviceGray" | b"G" => Space::Gray,
                        b"DeviceRGB" | b"RGB" => Space::Rgb,
                        b"DeviceCMYK" | b"CMYK" => Space::Cmyk,
                        other => cs.and_then(|c| c.get(other).cloned()).map(|o| self.space(cs, &o, depth + 1)).unwrap_or(Space::Other),
                    },
                    _ => self.space(cs, &o, depth + 1),
                }
            }
            None => match key {
                b"DefaultGray" => Space::Gray,
                b"DefaultRGB" => Space::Rgb,
                _ => Space::Cmyk,
            },
        }
    }
}

/// A sub-dictionary of a resource dictionary.
fn sub(res: &Dict, key: &[u8]) -> Option<Dict> {
    match res.get(key) {
        Some(Object::Dict(d)) => Some(d.clone()),
        Some(Object::Stream(s)) => Some(s.dict.clone()),
        _ => None,
    }
}

/// An ink name from a name or text string object.
fn ink_name(o: &Object) -> Option<String> {
    match o {
        Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
        Object::String(s) => Some(s.to_text()),
        _ => None,
    }
}

/// A page's content: its `/Contents` stream, or every stream of an array joined with a newline.
fn content(doc: &Document, page: &Dict) -> Vec<u8> {
    let Some(c) = page.get(b"Contents") else { return Vec::new() };
    let mut out = Vec::new();
    match &*doc.resolve(c) {
        Object::Stream(s) => out.extend(s.decoded().unwrap_or_default()),
        Object::Array(a) => {
            for o in a {
                if let Object::Stream(s) = &*doc.resolve(o) {
                    out.extend(s.decoded().unwrap_or_default());
                    out.push(b'\n');
                }
            }
        }
        _ => {}
    }
    out
}

/// A page's resources (inherited ones already filled in by pdfcraft-model).
fn resources(doc: &Document, page: &Dict) -> Dict {
    match page.get(b"Resources").map(|o| doc.resolve(o)).as_deref() {
        Some(Object::Dict(d)) => d.clone(),
        Some(Object::Stream(s)) => s.dict.clone(),
        _ => Dict::new(),
    }
}

#[cfg(test)]
mod tests;
