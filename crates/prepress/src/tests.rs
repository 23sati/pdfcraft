//! Tests for the ink walk, on synthetic PDFs.

use super::*;

fn build(objs: &[&str]) -> Document {
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    Document::open(std::sync::Arc::new(out)).unwrap()
}

/// The names in a report, in order.
fn names(r: &Report) -> Vec<&str> {
    r.inks.iter().map(|i| i.name.as_str()).collect()
}

fn ink<'a>(r: &'a Report, name: &str) -> &'a Ink {
    r.inks.iter().find(|i| i.name == name).unwrap_or_else(|| panic!("no ink {name:?} in {:?}", names(r)))
}

#[test]
fn cmyk_operators_report_four_process_inks_with_their_tints() {
    let doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>",
        "<< /Length 44 >>\nstream\n0.2 0.3 0.4 1 k 0 0 10 10 re f\nendstream",
    ]);
    let r = inks(&doc);
    assert_eq!(names(&r), ["Cyan", "Magenta", "Yellow", "Black"], "process inks come first, in channel order");
    assert_eq!(ink(&r, "Cyan").max_tint, Some(0.2));
    assert_eq!(ink(&r, "Black").max_tint, Some(1.0));
    assert_eq!(ink(&r, "Black").kind, Kind::Process);
    assert_eq!(ink(&r, "Cyan").pages, [0]);
    assert!(r.overprint_pages.is_empty());
}

#[test]
fn gray_rgb_and_q_restore() {
    // The gray sets the fill; q/Q nests a red that must not leak past Q.
    let doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>",
        "<< /Length 57 >>\nstream\n0.5 g 0 0 5 5 re f q 1 0 0 rg 0 0 5 5 re f Q\nendstream",
    ]);
    let r = inks(&doc);
    assert_eq!(names(&r), ["Gray", "Red", "Green", "Blue"]);
    assert_eq!(ink(&r, "Gray").max_tint, Some(0.5));
    assert_eq!(ink(&r, "Red").max_tint, Some(1.0));
    assert_eq!(ink(&r, "Green").max_tint, Some(0.0));
}

#[test]
fn a_spot_separation_ink_is_named_and_its_tint_kept() {
    let doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
        "<< /Type /Page /Parent 2 0 R /Resources << /ColorSpace << /CS1 5 0 R >> >> /Contents 4 0 R >>",
        "<< /Length 46 >>\nstream\n/CS1 cs 0.75 scn 0 0 50 50 re f\nendstream",
        // A Separation space: name, alternate space, a one-in identity tint transform.
        "[/Separation /PANTONE#20300#20C /DeviceCMYK 6 0 R]",
        "<< /FunctionType 2 /Domain [0 1] /C0 [0 0 0 0] /C1 [0 1 1 0] /N 1 >>",
    ]);
    let r = inks(&doc);
    assert_eq!(names(&r), ["PANTONE 300 C"]);
    assert_eq!(ink(&r, "PANTONE 300 C").kind, Kind::Spot);
    assert_eq!(ink(&r, "PANTONE 300 C").max_tint, Some(0.75));
}

#[test]
fn the_none_and_all_special_inks_keep_their_kind() {
    let doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
        "<< /Type /Page /Parent 2 0 R /Resources << /ColorSpace << /None1 5 0 R /All1 6 0 R >> >> /Contents 4 0 R >>",
        "<< /Length 40 >>\nstream\n/None1 cs 1 scn /All1 cs 1 scn\nendstream",
        "[/Separation /None /DeviceCMYK 7 0 R]",
        "[/Separation /All /DeviceCMYK 7 0 R]",
        "<< /FunctionType 2 /Domain [0 1] /C0 [0 0 0 0] /C1 [1 1 1 1] /N 1 >>",
    ]);
    let r = inks(&doc);
    assert_eq!(ink(&r, "None").kind, Kind::None);
    assert_eq!(ink(&r, "All").kind, Kind::All);
}

#[test]
fn devicen_lists_every_ink_of_the_space() {
    let doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
        "<< /Type /Page /Parent 2 0 R /Resources << /ColorSpace << /DN 5 0 R >> >> /Contents 4 0 R >>",
        "<< /Length 34 >>\nstream\n/DN cs 1 0.5 scn\nendstream",
        // DeviceN over two inks, one of them the special None.
        "[/DeviceN [/Varnish /None] /DeviceCMYK 6 0 R]",
        "<< /FunctionType 2 /Domain [0 1 0 1] /C0 [0 0 0 0] /C1 [0 0 0 1] /N 1 >>",
    ]);
    let r = inks(&doc);
    assert_eq!(ink(&r, "Varnish").kind, Kind::Spot);
    assert_eq!(ink(&r, "Varnish").max_tint, Some(1.0));
    assert_eq!(ink(&r, "None").kind, Kind::None);
}

#[test]
fn overprint_marks_the_page_that_paints_under_it() {
    let doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
        "<< /Type /Page /Parent 2 0 R /Resources << /ExtGState << /GS1 5 0 R >> >> /Contents 4 0 R >>",
        // Overprint for fills only; the stroke after it must not mark the page again.
        "<< /Length 48 >>\nstream\n/GS1 gs 0 g 0 0 9 9 re f 0 G 0 0 m 9 9 l S\nendstream",
        "<< /Type /ExtGState /OP true /op false >>",
    ]);
    assert_eq!(inks(&doc).overprint_pages, [0]);
}

#[test]
fn overprint_off_means_no_overprint_page() {
    let doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
        "<< /Type /Page /Parent 2 0 R /Resources << /ExtGState << /GS1 5 0 R >> >> /Contents 4 0 R >>",
        "<< /Length 32 >>\nstream\n/GS1 gs 0 g 0 0 9 9 re f\nendstream",
        "<< /Type /ExtGState /OP false >>",
    ]);
    assert!(inks(&doc).overprint_pages.is_empty());
}

#[test]
fn a_form_xobject_is_walked_with_the_pages_resources() {
    let doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
        // The form has no /Resources of its own; the spot space is the page's.
        "<< /Type /Page /Parent 2 0 R /Resources << /ColorSpace << /CS1 5 0 R >> /XObject << /Fm1 6 0 R >> >> /Contents 4 0 R >>",
        "<< /Length 17 >>\nstream\n/Fm1 Do 0.5 g\nendstream",
        "[/Separation /Gold /DeviceCMYK 7 0 R]",
        "<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] /Length 24 >>\nstream\n/CS1 cs 0.9 scn\nendstream",
        "<< /FunctionType 2 /Domain [0 1] /C0 [0 0 0 0] /C1 [0 0.3 0.8 0.1] /N 1 >>",
    ]);
    let r = inks(&doc);
    assert_eq!(names(&r), ["Gray", "Gold"]);
    assert_eq!(ink(&r, "Gold").max_tint, Some(0.9));
    assert_eq!(ink(&r, "Gold").pages, [0]);
}

#[test]
fn a_cyclic_form_terminates() {
    // Object 5's form content draws object 5 again: the depth bound must stop it.
    let doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
        "<< /Type /Page /Parent 2 0 R /Resources << /XObject << /Fm1 5 0 R >> >> /Contents 4 0 R >>",
        "<< /Length 8 >>\nstream\n/Fm1 Do\nendstream",
        "<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] /Length 17 >>\nstream\n/Fm1 Do 1 g\nendstream",
    ]);
    assert_eq!(names(&inks(&doc)), ["Gray"]);
}

#[test]
fn an_image_records_its_inks_without_a_tint() {
    let doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
        "<< /Type /Page /Parent 2 0 R /Resources << /XObject << /Im1 5 0 R >> >> /Contents 4 0 R >>",
        "<< /Length 8 >>\nstream\n/Im1 Do\nendstream",
        "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length 3 >>\nstream\n\u{0}\u{0}\u{0}\nendstream",
    ]);
    let r = inks(&doc);
    assert_eq!(names(&r), ["Red", "Green", "Blue"]);
    assert_eq!(ink(&r, "Red").max_tint, None, "an image's tints are not in the content stream");
}

#[test]
fn iccbased_and_indexed_spaces_resolve_to_their_channels() {
    let doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
        "<< /Type /Page /Parent 2 0 R /Resources << /ColorSpace << /ICC 5 0 R /Idx 6 0 R >> >> /Contents 4 0 R >>",
        "<< /Length 44 >>\nstream\n/ICC cs 0 0 0 0.8 scn /Idx cs 3 scn\nendstream",
        // A four-component ICC profile stream, /N 4: its channels are the CMYK process inks.
        "[/ICCBased 7 0 R]",
        // Indexed over the ICC space: the index's tints are unknown, the inks are not.
        "[/Indexed /ICC 255 <00>]",
        "<< /N 4 /Alternate /DeviceCMYK /Length 0 >>\nstream\n\nendstream",
    ]);
    let r = inks(&doc);
    assert_eq!(names(&r), ["Cyan", "Magenta", "Yellow", "Black"]);
    assert_eq!(ink(&r, "Black").max_tint, Some(0.8));
}

#[test]
fn one_page_can_be_read_on_its_own() {
    let doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 /MediaBox [0 0 200 200] >>",
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>",
        "<< /Length 14 >>\nstream\n1 0 0 0 k\nendstream",
        "<< /Type /Page /Parent 2 0 R /Contents 6 0 R >>",
        "<< /Length 12 >>\nstream\n0 1 0 rg\nendstream",
    ]);
    assert_eq!(names(&inks(&doc)), ["Red", "Green", "Blue", "Cyan", "Magenta", "Yellow", "Black"]);
    assert_eq!(names(&inks_of(&doc, 1)), ["Red", "Green", "Blue"]);
    assert_eq!(ink(&inks_of(&doc, 0), "Cyan").pages, [0]);
    assert!(inks_of(&doc, 9).inks.is_empty(), "an out-of-range page reports nothing");
}
