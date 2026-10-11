//! `cargo xtask render-oracle`: render generated test pages with PdfCraft, Poppler (`pdftoppm`) and
//! MuPDF (`mutool draw`), and fail when PdfCraft strays too far from both.
//!
//! Part of the fidelity harness (gap 12). The pages are built here at run time, so nothing binary
//! is committed, and each aims at a known weak spot of the renderer: thin and dashed strokes,
//! standard-14 and Type 3 text, shadings, tiling patterns (TikZ hatching, #794), transparency, images,
//! clipping and colour spaces. Poppler and MuPDF run only as external processes (clean-room, ADR-0001).
//!
//! Renderers anti-alias differently, so a pixel only counts as different when no pixel within one
//! step of it in the other image is close to it (`TOLERANCE`), checked both ways. A page passes
//! when its share of such pixels against the closer oracle stays within its limit. The limits come
//! from measuring the current renderer against the oracle versions CI installs (Ubuntu 24.04:
//! Poppler 24.02, MuPDF 1.23), plus headroom; other versions or substitute fonts move the numbers.
//! A blank page must also fail every limit clearly, so no limit can hide a page that isn't drawn.
//!
//! What it can't see: colour errors below `TOLERANCE` on every channel, and an error both oracles
//! happen to share or one of them is close to (PdfCraft's Lab blue is 45 levels off the CIE formula
//! on red, but within the tolerance of MuPDF's, so the Lab page passes).
//!
//! Each run writes the page, the renders and a difference image (red where PdfCraft differs from
//! the closer oracle) for every page to `--out` (default `target/render-oracle/`).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, bail};

use crate::gates;

/// Page size in points and the render resolution: 200 pt at 144 dpi is 400 × 400 pixels.
const PAGE_PT: u32 = 200;
const DPI: u32 = 144;
/// How far apart two pixel values may be on any channel and still count as the same.
const TOLERANCE: u8 = 32;
/// A render that takes longer than this is a failure.
const RENDER_TIMEOUT: Duration = Duration::from_secs(60);
/// A blank page must differ from the oracle by at least this many times a page's limit.
const BLANK_FACTOR: f64 = 2.5;

struct Fixture {
    name: &'static str,
    what: &'static str,
    build: fn() -> Vec<u8>,
    /// The largest share of differing pixels (percent) against the closer oracle that still passes:
    /// the measured share times 1.5 or plus 0.5, whichever is larger, rounded up to a half.
    limit_percent: f64,
}

// Measured 2026-10-11 against the closer of Poppler 24.02 and MuPDF 1.23.10 (Ubuntu 24.04):
// vector 0.77, text-standard14 3.82 (substitute fonts differ), text-type3 0.20, shading 0.05,
// tiling-pattern 4.24 (2.41 of it the thin rotated hatch, drawn grey and soft where both oracles
// draw it crisp), transparency 0.04, images 0.00, clipping 0.70, color-spaces 0.00, lab 0.00.

const FIXTURES: &[Fixture] = &[
    Fixture { name: "vector", what: "fills, caps, joins, dashes, hairlines, even-odd", build: vector, limit_percent: 1.5 },
    Fixture { name: "text-standard14", what: "Helvetica, Times, Courier, outline and rotated text", build: text_standard14, limit_percent: 6.0 },
    Fixture { name: "text-type3", what: "Type 3 glyphs (d0 and d1), spacing, scaling, skew", build: text_type3, limit_percent: 1.0 },
    Fixture { name: "shading", what: "axial, stitched and radial shadings, shading pattern", build: shading, limit_percent: 1.0 },
    Fixture { name: "tiling-pattern", what: "checks, TikZ and rotated hatching, uncoloured dots", build: tiling_pattern, limit_percent: 6.5 },
    Fixture { name: "transparency", what: "alpha, blend modes, group opacity, luminosity soft mask", build: transparency, limit_percent: 1.0 },
    Fixture { name: "images", what: "upscaled RGB, gray, stencil, indexed and rotated images", build: images, limit_percent: 0.5 },
    Fixture { name: "clipping", what: "nested, even-odd, rotated and text clipping", build: clipping, limit_percent: 1.5 },
    Fixture { name: "color-spaces", what: "DeviceCMYK, Separation and CalRGB", build: color_spaces, limit_percent: 0.5 },
    Fixture { name: "lab", what: "CIE Lab colours", build: lab, limit_percent: 0.5 },
];

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let out = match args {
        [] => gates::target_dir().join("render-oracle"),
        [flag, dir] if flag == "--out" => PathBuf::from(dir),
        _ => bail!("usage: cargo xtask render-oracle [--out DIR]"),
    };
    let oracles: Vec<Oracle> = ORACLES.iter().copied().filter(Oracle::available).collect();
    for oracle in ORACLES.iter().filter(|o| !o.available()) {
        eprintln!("render-oracle: {} not found, skipping it ({})", oracle.name, oracle.install);
    }
    if oracles.is_empty() {
        bail!("neither pdftoppm (Poppler) nor mutool (MuPDF) was found; install poppler-utils and mupdf-tools");
    }
    gates::run_args(&["build", "--release", "-p", "pdfcraft-cli"])?;
    std::fs::create_dir_all(&out).with_context(|| format!("create {}", out.display()))?;
    let cli = gates::release_cli();

    let mut table = format!("{:<16} {:>8} {:>8} {:>8} {:>7} {:>7}  result\n", "page", "Poppler", "MuPDF", "oracles", "blank", "limit");
    let mut failures = Vec::new();
    for fixture in FIXTURES {
        let row = match check(fixture, &cli, &oracles, &out) {
            Ok(row) => row,
            Err(e) => {
                failures.push(format!("{}: {e:#}", fixture.name));
                let _ = writeln!(table, "{:<16} {:>42}  ERROR", fixture.name, "");
                continue;
            }
        };
        let verdict = if row.best > fixture.limit_percent {
            failures.push(format!("{}: {:.2}% of pixels differ from the closer oracle (limit {}%)", fixture.name, row.best, fixture.limit_percent));
            "FAIL"
        } else if row.blank < fixture.limit_percent * BLANK_FACTOR {
            failures.push(format!(
                "{}: a blank page differs by only {:.2}%, too close to the {}% limit to catch a page that isn't drawn",
                fixture.name, row.blank, fixture.limit_percent
            ));
            "WEAK"
        } else {
            "ok"
        };
        let percent = |v: Option<f64>| v.map_or_else(|| "-".to_string(), |v| format!("{v:.2}%"));
        let _ = writeln!(
            table,
            "{:<16} {:>8} {:>8} {:>8} {:>6.1}% {:>6.1}%  {verdict:<4}  {}",
            fixture.name,
            percent(row.poppler),
            percent(row.mupdf),
            percent(row.oracles),
            row.blank,
            fixture.limit_percent,
            fixture.what
        );
    }
    println!("\nShare of pixels that differ beyond anti-aliasing at {DPI} dpi (lower is better; 'oracles' is Poppler against MuPDF):\n");
    print!("{table}");
    println!("\nRenders and difference images: {}", out.display());
    if !failures.is_empty() {
        for failure in &failures {
            eprintln!("render-oracle: {failure}");
        }
        bail!("{} of {} pages failed", failures.len(), FIXTURES.len());
    }
    println!("render-oracle: all {} pages within their limits", FIXTURES.len());
    Ok(())
}

struct Row {
    poppler: Option<f64>,
    mupdf: Option<f64>,
    oracles: Option<f64>,
    /// Against the closer oracle.
    best: f64,
    /// A blank white page against the closer oracle.
    blank: f64,
}

fn check(fixture: &Fixture, cli: &Path, oracles: &[Oracle], out: &Path) -> anyhow::Result<Row> {
    let pdf = out.join(format!("{}.pdf", fixture.name));
    std::fs::write(&pdf, (fixture.build)()).with_context(|| format!("write {}", pdf.display()))?;
    let ours_png = out.join(format!("{}-pdfcraft.png", fixture.name));
    let mut render = Command::new(cli);
    render.arg("render").arg(&pdf).args(["--page", "1", "--dpi", &DPI.to_string(), "--out"]).arg(&ours_png);
    gates::run_status_timeout(&mut render, RENDER_TIMEOUT).context("pdfcraft-cli render")?;
    let ours = Image::read(&ours_png)?;

    let mut renders = Vec::new();
    for oracle in oracles {
        let png = out.join(format!("{}-{}.png", fixture.name, oracle.file));
        gates::run_status_timeout(&mut oracle.command(&pdf, &png), RENDER_TIMEOUT).context(oracle.name)?;
        let image = Image::read(&png)?;
        let delta = compare(&ours, &image)?;
        renders.push((oracle.file, image, delta));
    }
    let (best_file, best_image, best) = renders.iter().min_by(|a, b| a.2.percent.total_cmp(&b.2.percent)).context("no oracle render")?;
    write_png(&out.join(format!("{}-diff.png", fixture.name)), best_image.width, best_image.height, &diff_image(best_image, best))?;
    let blank = Image { width: ours.width, height: ours.height, rgb: vec![[255; 3]; ours.rgb.len()] };
    let blank = compare(&blank, best_image)?.percent;
    let find = |file: &str| renders.iter().find(|r| r.0 == file);
    let oracles = match (find("poppler"), find("mupdf")) {
        (Some(p), Some(m)) => Some(compare(&p.1, &m.1)?.percent),
        _ => None,
    };
    eprintln!("render-oracle: {} is closest to {best_file} ({:.2}%, mean error {:.2}/255)", fixture.name, best.percent, best.mean_abs);
    Ok(Row { poppler: find("poppler").map(|r| r.2.percent), mupdf: find("mupdf").map(|r| r.2.percent), oracles, best: best.percent, blank })
}

#[derive(Clone, Copy)]
struct Oracle {
    name: &'static str,
    file: &'static str,
    program: &'static str,
    install: &'static str,
}

const ORACLES: &[Oracle] = &[
    Oracle { name: "Poppler", file: "poppler", program: "pdftoppm", install: "brew install poppler / apt install poppler-utils" },
    Oracle { name: "MuPDF", file: "mupdf", program: "mutool", install: "brew install mupdf / apt install mupdf-tools" },
];

impl Oracle {
    fn available(&self) -> bool {
        // mutool exits non-zero after printing its version, so being able to spawn it is the test.
        Command::new(self.program).arg("-v").output().is_ok()
    }

    fn command(&self, pdf: &Path, png: &Path) -> Command {
        let mut c = Command::new(self.program);
        let dpi = DPI.to_string();
        if self.program == "pdftoppm" {
            // pdftoppm appends ".png" to the prefix it is given.
            c.args(["-f", "1", "-l", "1", "-r", &dpi, "-png", "-singlefile"]).arg(pdf).arg(png.with_extension(""));
        } else {
            c.args(["draw", "-q", "-r", &dpi, "-c", "rgb", "-o"]).arg(png).arg(pdf).arg("1");
        }
        c
    }
}

struct Image {
    width: u32,
    height: u32,
    rgb: Vec<[u8; 3]>,
}

impl Image {
    fn read(path: &Path) -> anyhow::Result<Self> {
        let file = std::fs::File::open(path).with_context(|| format!("open PNG {}", path.display()))?;
        let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
        decoder.set_limits(png::Limits { bytes: 64 * 1024 * 1024 });
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().with_context(|| format!("read PNG header {}", path.display()))?;
        let size = reader.output_buffer_size().context("PNG output size overflow")?;
        let mut data = vec![0; size];
        let info = reader.next_frame(&mut data).with_context(|| format!("decode PNG {}", path.display()))?;
        let frame = data.get(..info.buffer_size()).context("PNG frame exceeds its output buffer")?;
        let pixels = frame
            .chunks(info.color_type.samples())
            .map(|sample| {
                let channel = |index: usize| sample.get(index).copied().context("short PNG pixel");
                match info.color_type {
                    png::ColorType::Rgb | png::ColorType::Rgba => Ok([channel(0)?, channel(1)?, channel(2)?]),
                    png::ColorType::Grayscale | png::ColorType::GrayscaleAlpha => Ok([channel(0)?; 3]),
                    png::ColorType::Indexed => bail!("PNG palette was not expanded"),
                }
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        if pixels.len() != (info.width as usize).saturating_mul(info.height as usize) {
            bail!("{}: {} pixels for {}×{}", path.display(), pixels.len(), info.width, info.height);
        }
        Ok(Self { width: info.width, height: info.height, rgb: pixels })
    }

    fn at(&self, x: i64, y: i64) -> Option<[u8; 3]> {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return None;
        }
        let index = usize::try_from(y.saturating_mul(i64::from(self.width)).saturating_add(x)).ok()?;
        self.rgb.get(index).copied()
    }

    /// The smallest channel distance from `pixel` to this image's pixels within one step of (x, y).
    fn nearest(&self, x: i64, y: i64, pixel: [u8; 3]) -> u8 {
        let mut best = u8::MAX;
        for dy in -1..=1 {
            for dx in -1..=1 {
                if let Some(other) = self.at(x + dx, y + dy) {
                    best = best.min(distance(pixel, other));
                }
            }
        }
        best
    }
}

fn distance(a: [u8; 3], b: [u8; 3]) -> u8 {
    a.iter().zip(b).map(|(x, y)| x.abs_diff(y)).max().unwrap_or(0)
}

struct Delta {
    /// Share of pixels (percent) that differ beyond anti-aliasing.
    percent: f64,
    /// Plain mean absolute error per channel, for reference.
    mean_abs: f64,
    /// Per pixel: its distance to the closest nearby pixel of the other image, the larger of both ways.
    distances: Vec<u8>,
}

fn compare(a: &Image, b: &Image) -> anyhow::Result<Delta> {
    if (a.width, a.height) != (b.width, b.height) {
        bail!("render sizes differ: {}×{} and {}×{}", a.width, a.height, b.width, b.height);
    }
    if a.rgb.is_empty() || a.rgb.len() != b.rgb.len() {
        bail!("render pixel counts differ or are empty");
    }
    let (mut sum, mut over) = (0u64, 0usize);
    let mut distances = Vec::with_capacity(a.rgb.len());
    for y in 0..i64::from(a.height) {
        for x in 0..i64::from(a.width) {
            let (Some(left), Some(right)) = (a.at(x, y), b.at(x, y)) else { bail!("pixel ({x}, {y}) out of range") };
            sum = sum.saturating_add(left.iter().zip(right).map(|(l, r)| u64::from(l.abs_diff(r))).sum());
            let d = b.nearest(x, y, left).max(a.nearest(x, y, right));
            over += usize::from(d > TOLERANCE);
            distances.push(d);
        }
    }
    let pixels = a.rgb.len() as f64;
    Ok(Delta { percent: over as f64 * 100.0 / pixels, mean_abs: sum as f64 / (pixels * 3.0), distances })
}

/// The oracle render in faint grey, with the pixels that differ in red (brighter is further off).
fn diff_image(oracle: &Image, delta: &Delta) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(oracle.rgb.len().saturating_mul(3));
    for (&[r, g, b], &d) in oracle.rgb.iter().zip(&delta.distances) {
        if d > TOLERANCE {
            rgb.extend_from_slice(&[128u8.saturating_add(d / 2), 0, 0]);
        } else {
            let luma = (u16::from(r) * 3 + u16::from(g) * 6 + u16::from(b)) / 10;
            rgb.extend_from_slice(&[u8::try_from(192 + luma / 4).unwrap_or(u8::MAX); 3]);
        }
    }
    rgb
}

fn write_png(path: &Path, width: u32, height: u32, rgb: &[u8]) -> anyhow::Result<()> {
    let file = std::fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgb).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

// ---- The generated pages ----

/// A one-page PDF. `resources` is the page's resource dictionary; it may refer to `objects`, which
/// are numbered from 4 (`4 0 R` is the first).
fn page_pdf(resources: &str, content: &str, objects: &[Vec<u8>]) -> Vec<u8> {
    let mut all = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_PT} {PAGE_PT}] /Resources {resources} /Contents {} 0 R >>", objects.len() + 4)
            .into_bytes(),
    ];
    all.extend(objects.iter().cloned());
    all.push(stream("", content.as_bytes()));
    let mut pdf = b"%PDF-1.7\n% render-oracle fixture\n".to_vec();
    let mut offsets = Vec::with_capacity(all.len());
    for (index, body) in all.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", all.len() + 1).as_bytes());
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", all.len() + 1).as_bytes());
    pdf
}

/// A stream object; `dict` holds its entries besides `/Length`.
fn stream(dict: &str, data: &[u8]) -> Vec<u8> {
    [format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes(), data.to_vec(), b"\nendstream".to_vec()].concat()
}

/// A closed circle path of four Bézier curves, ready to be painted.
fn circle(cx: f64, cy: f64, r: f64) -> String {
    let k = r * 0.552_284_75;
    let (l, rt, t, b) = (cx - r, cx + r, cy + r, cy - r);
    format!(
        "{rt} {cy} m {rt} {} {} {t} {cx} {t} c {} {t} {l} {} {l} {cy} c {l} {} {} {b} {cx} {b} c {} {b} {rt} {} {rt} {cy} c h ",
        cy + k,
        cx + k,
        cx - k,
        cy + k,
        cy - k,
        cx - k,
        cx + k,
        cy - k
    )
}

const STANDARD_FONTS: &str = "/Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> \
     /F2 << /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >> /F3 << /Type /Font /Subtype /Type1 /BaseFont /Courier >> >>";

fn vector() -> Vec<u8> {
    let content = "\
q 0.85 0.2 0.2 rg 15 125 60 60 re f Q
q 0 0.4 0.8 RG 10 w 0 J 0 j 95 135 m 120 180 l 145 135 l S Q
q 0 0.6 0.3 RG 10 w 1 J 1 j 140 135 m 165 180 l 190 135 l S Q
q 0.6 0.3 0 RG 10 w 2 J 2 j 100 110 m 185 110 l S Q
q 0.2 G 3 w [6 4] 0 d 15 95 m 185 95 l S Q
q 0.2 G 2 w [1 3] 0.5 d 1 J 15 85 m 185 85 l S Q
q 0.4 0 0.6 rg 50 75 m 20 15 l 80 55 l 20 55 l 80 15 l h f* Q
q 0 0.5 0.5 rg 100 15 m 190 15 190 75 145 75 c 100 75 125 45 100 15 c f Q
q 0 G 0.1 w 100 5 m 190 5 l S 0.5 w 100 8 m 190 8 l S Q
q 0 G 0 w 5 5 m 5 195 l S Q
";
    page_pdf("<< >>", content, &[])
}

fn text_standard14() -> Vec<u8> {
    let content = "\
BT /F1 38 Tf 6 164 Td (Hamburg) Tj ET
BT /F2 36 Tf 6 132 Td (AV Wa fig.) Tj ET
BT /F3 26 Tf 6 106 Td (0O1lI{}) Tj ET
BT /F1 34 Tf 1 Tr 1.5 w 0 0 0.8 RG 6 72 Td (Outline) Tj ET
BT /F2 34 Tf 2 Tr 0.8 w 0.8 0 0 RG 0 0.5 0 rg 6 40 Td (Fill+Stroke) Tj ET
BT /F1 14 Tf 0.866 0.5 -0.5 0.866 125 8 Tm (rotated 30) Tj ET
BT /F2 7 Tf 8 24 Td (Seven point Times) Tj ET
BT /F1 12 Tf 3 Tc 8 8 Td (spaced) Tj ET
";
    page_pdf(&format!("<< {STANDARD_FONTS} >>"), content, &[])
}

fn text_type3() -> Vec<u8> {
    let square = stream("", b"1000 0 0 0 1000 1000 d1 100 100 800 800 re f");
    let triangle = stream("", b"1000 0 d0 0.85 0.1 0.1 rg 100 100 m 900 100 l 500 900 l f");
    let ring = stream("", format!("1000 0 0 0 1000 1000 d1 {}{}f*", circle(500.0, 500.0, 420.0), circle(500.0, 500.0, 220.0)).as_bytes());
    let font = "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 1000 1000] /FontMatrix [0.001 0 0 0.001 0 0] \
         /CharProcs << /sq 4 0 R /tri 5 0 R /ring 6 0 R >> /Encoding << /Type /Encoding /Differences [97 /sq /tri /ring] >> \
         /FirstChar 97 /LastChar 99 /Widths [1000 1000 1000] /Resources << >> >>";
    let content = "\
BT /T3 24 Tf 0 0 0.7 rg 12 160 Td (abcabc) Tj ET
BT /T3 12 Tf 4 Tc 0.1 0.5 0.1 rg 12 132 Td (cbacbacba) Tj ET
BT /T3 18 Tf 50 Tz 0 g 12 104 Td (abcabcabc) Tj ET
BT /T3 36 Tf 1 0 0.4 1 12 50 Tm 0.5 0 0.5 rg (abc) Tj ET
BT /T3 7 Tf 0 g 12 20 Td (abcabcabcabcabcabc) Tj ET
BT /T3 30 Tf 0.7071 0.7071 -0.7071 0.7071 150 20 Tm 0 0.4 0.6 rg (ac) Tj ET
";
    page_pdf(&format!("<< /Font << /T3 {font} >> >>"), content, &[square, triangle, ring])
}

fn shading() -> Vec<u8> {
    let axial = "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [40 0 160 0] /Extend [true true] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [0.9 0.1 0.1] /C1 [0.1 0.1 0.9] /N 1 >> >>";
    let stitched = "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 95 0 135] \
         /Function << /FunctionType 3 /Domain [0 1] /Bounds [0.4] /Encode [0 1 0 1] /Functions [\
         << /FunctionType 2 /Domain [0 1] /C0 [1 1 0] /C1 [0 0.6 0] /N 1 >> \
         << /FunctionType 2 /Domain [0 1] /C0 [0 0.6 0] /C1 [0 0 0.4] /N 2 >>] >> >>";
    let radial = "<< /ShadingType 3 /ColorSpace /DeviceRGB /Coords [50 45 4 50 45 38] /Extend [false true] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [1 1 1] /C1 [0.8 0.3 0] /N 1 >> >>";
    let focal = "<< /PatternType 2 /Shading << /ShadingType 3 /ColorSpace /DeviceRGB /Coords [135 30 0 150 45 38] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [0.2 0.9 0.9] /C1 [0.1 0.1 0.4] /N 1 >> /Extend [true true] >> >>";
    let content = format!(
        "q 10 145 180 45 re W n /Sh1 sh Q\nq 10 95 180 40 re W n /Sh2 sh Q\nq 10 10 80 75 re W n /Sh3 sh Q\nq /Pattern cs /P1 scn {}f Q\n",
        circle(150.0, 47.0, 36.0)
    );
    page_pdf(&format!("<< /Shading << /Sh1 {axial} /Sh2 {stitched} /Sh3 {radial} >> /Pattern << /P1 {focal} >> >>"), &content, &[])
}

fn tiling_pattern() -> Vec<u8> {
    let checks = stream(
        "/Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 20 20] /XStep 20 /YStep 20 /Resources << >>",
        b"0.85 0.1 0.1 rg 0 0 10 10 re f 0.1 0.1 0.85 rg 10 10 10 10 re f",
    );
    // Thin rotated hatching, as TikZ draws it (#794).
    let hatch = stream(
        "/Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 6 6] /XStep 6 /YStep 6 \
         /Matrix [0.7071 0.7071 -0.7071 0.7071 0 0] /Resources << >>",
        b"0 G 0.6 w 0 3 m 6 3 l S",
    );
    // TikZ's "north east lines": a diagonal in an upright cell whose box overlaps its neighbours.
    let north_east = stream(
        "/Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [-1 -1 7 7] /XStep 6 /YStep 6 /Resources << >>",
        b"0 0 0.6 RG 1.2 w 0 0 m 6.2 6.2 l S",
    );
    let dots = stream(
        "/Type /Pattern /PatternType 1 /PaintType 2 /TilingType 1 /BBox [0 0 8 8] /XStep 8 /YStep 8 /Resources << >>",
        format!("{}f", circle(4.0, 4.0, 2.5)).as_bytes(),
    );
    let ring = circle(147.0, 55.0, 40.0);
    let content = format!(
        "q /Pattern cs /P1 scn 10 110 85 80 re f Q\nq /Pattern cs /P4 scn 105 110 85 80 re f Q\n\
         q /Cs1 cs 0 0.55 0.2 /P3 scn 10 10 85 90 re f Q\nq /Pattern cs /P2 scn {ring}f Q\nq 0 G 1 w {ring}S Q\n"
    );
    page_pdf(
        "<< /Pattern << /P1 4 0 R /P2 5 0 R /P3 6 0 R /P4 7 0 R >> /ColorSpace << /Cs1 [/Pattern /DeviceRGB] >> >>",
        &content,
        &[checks, hatch, dots, north_east],
    )
}

fn transparency() -> Vec<u8> {
    let mask_group = stream(
        "/Type /XObject /Subtype /Form /BBox [0 0 200 200] /Group << /S /Transparency /CS /DeviceGray >> \
         /Resources << /Shading << /Sh << /ShadingType 2 /ColorSpace /DeviceGray /Coords [20 0 180 0] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> >> >> >>",
        b"/Sh sh",
    );
    // Two overlapping opaque rectangles painted at 60%: the overlap must not come out darker.
    let group = stream(
        "/Type /XObject /Subtype /Form /BBox [0 0 200 200] /Group << /S /Transparency /I true >> /Resources << >>",
        b"0 0.6 0.2 rg 20 52 90 36 re f 0 0.2 0.8 rg 80 60 100 36 re f",
    );
    let gstates = "/ExtGState << /A << /ca 0.5 /CA 0.5 >> /M << /BM /Multiply >> /S << /BM /Screen >> /D << /BM /Difference >> \
         /G << /ca 0.6 >> /SM << /SMask << /S /Luminosity /G 4 0 R >> >> >>";
    let content = format!(
        "0.95 0.8 0.3 rg 0 100 200 100 re f\n\
         q /A gs 1 0 0 rg {}f 0 0 1 rg {}f 0 G 4 w 20 105 m 100 195 l S Q\n\
         q /M gs 0 0.8 0.8 rg {}f Q\nq /S gs 0.7 0 0.7 rg {}f Q\nq /D gs 0.2 0.5 1 rg 110 104 80 22 re f Q\n\
         q /G gs /Fm Do Q\nq /SM gs 0.6 0 0 rg 20 8 160 36 re f Q\n",
        circle(50.0, 150.0, 30.0),
        circle(80.0, 150.0, 30.0),
        circle(140.0, 158.0, 28.0),
        circle(168.0, 140.0, 24.0)
    );
    page_pdf(&format!("<< {gstates} /XObject << /Fm 5 0 R >> >>"), &content, &[mask_group, group])
}

/// Bytes as ASCIIHex data with its `>` end marker, for inline images.
fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2 + 1);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s.push('>');
    s
}

fn images() -> Vec<u8> {
    let checker: Vec<u8> = (0..16u8).flat_map(|i| if (i % 4 + i / 4) % 2 == 0 { [230, 40, 40] } else { [40, 40, 200] }).collect();
    let gray: Vec<u8> = (0..16u8).map(|i| i * 17).collect();
    // An 8 × 8 face; 0 bits are painted.
    let stencil = [0xc3u8, 0x81, 0x24, 0x00, 0x00, 0x24, 0x99, 0xc3];
    // Two bits per pixel, a row per byte: indices 0123, 1230, 2301, 3012.
    let indexed = [0x1bu8, 0x6c, 0xb1, 0xc6];
    let corners = [255u8, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0];
    let content = format!(
        "q 80 0 0 80 10 110 cm BI /W 4 /H 4 /CS /RGB /BPC 8 /F /AHx ID {} EI Q\n\
         q 180 0 0 18 10 82 cm BI /W 16 /H 1 /CS /G /BPC 8 /F /AHx ID {} EI Q\n\
         q 0.8 0.1 0.3 rg 80 0 0 80 110 110 cm BI /W 8 /H 8 /IM true /BPC 1 /F /AHx ID {} EI Q\n\
         q 60 0 0 60 10 12 cm BI /W 4 /H 4 /CS [/I /RGB 3 <ff8000 00a050 2040ff f0f0f0>] /BPC 2 /F /AHx ID {} EI Q\n\
         q 1 0 0 1 140 15 cm 0.866 0.5 -0.5 0.866 0 0 cm 45 0 0 45 0 0 cm BI /W 2 /H 2 /CS /RGB /BPC 8 /F /AHx ID {} EI Q\n",
        hex(&checker),
        hex(&gray),
        hex(&stencil),
        hex(&indexed),
        hex(&corners)
    );
    page_pdf("<< >>", &content, &[])
}

fn clipping() -> Vec<u8> {
    let stripes: String = (0..12).map(|i| format!("{} rg {} 90 8 110 re f ", if i % 2 == 0 { "0 0.4 0.9" } else { "1 0.8 0" }, i * 8 + 12)).collect();
    let text_shading = "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [12 0 120 0] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [0.9 0 0.4] /C1 [0 0.3 0.9] /N 1 >> >>";
    let content = format!(
        "q {}W n {stripes}Q\n\
         q 110 100 80 90 re W n {}{}W* n 1 0.5 0 rg 0 0 200 200 re f Q\n\
         q BT /F1 44 Tf 7 Tr 12 22 Td (CLIP) Tj ET /Sh sh Q\n\
         q 1 0 0 1 160 50 cm 0.7071 0.7071 -0.7071 0.7071 0 0 cm -22 -22 44 44 re W n 0.9 0.3 0 rg -40 -40 80 80 re f \
         0 0 0.7 rg -40 -40 30 80 re f Q\n",
        circle(56.0, 140.0, 45.0),
        circle(150.0, 145.0, 38.0),
        circle(150.0, 145.0, 18.0)
    );
    page_pdf(&format!("<< {STANDARD_FONTS} /Shading << /Sh {text_shading} >> >>"), &content, &[])
}

fn color_spaces() -> Vec<u8> {
    let mut content = String::new();
    let cmyk = ["1 0 0 0", "0 1 0 0", "0 0 1 0", "0 0 0 1", "0.5 0.5 0 0", "0 0.5 1 0", "0.2 0.9 0.6 0.3", "0.6 0.1 0.1 0.5"];
    for (i, c) in cmyk.iter().enumerate() {
        let _ = writeln!(content, "{c} k {} 150 20 40 re f", 12 + i * 22);
    }
    let rows = [("Spot", 80, ["0.25", "0.5", "0.75", "1"]), ("Cal", 15, ["0.8 0.2 0.2", "0.2 0.7 0.3", "0.2 0.3 0.8", "0.5 0.5 0.5"])];
    for (space, y, colours) in rows {
        for (i, colour) in colours.iter().enumerate() {
            let _ = writeln!(content, "/{space} cs {colour} sc {} {y} 40 50 re f", 12 + i * 46);
        }
    }
    let spaces = "/ColorSpace << \
         /Spot [/Separation /Spot /DeviceCMYK << /FunctionType 2 /Domain [0 1] /C0 [0 0 0 0] /C1 [0 0.8 0.6 0.1] /N 1 >>] \
         /Cal [/CalRGB << /WhitePoint [0.9505 1 1.089] /Gamma [2.2 2.2 2.2] >>] >>";
    page_pdf(&format!("<< {spaces} >>"), &content, &[])
}

fn lab() -> Vec<u8> {
    let mut content = String::new();
    let colours = ["50 60 40", "70 -50 30", "40 20 -60", "90 0 0", "60 0 -40", "30 40 20", "80 10 70", "20 0 0", "65 -20 -30"];
    for (i, colour) in colours.iter().enumerate() {
        let _ = writeln!(content, "{colour} sc {} {} 56 56 re f", 10 + (i % 3) * 64, 10 + (i / 3) * 64);
    }
    page_pdf("<< /ColorSpace << /L [/Lab << /WhitePoint [0.9505 1 1.089] /Range [-100 100 -100 100] >>] >> >>", &format!("/L cs\n{content}"), &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 3]) -> Image {
        Image { width, height, rgb: (0..height).flat_map(|y| (0..width).map(move |x| (x, y))).map(|(x, y)| pixel(x, y)).collect() }
    }

    #[test]
    fn a_one_pixel_shift_is_anti_aliasing_not_a_difference() {
        let a = image(10, 10, |x, _| if x < 5 { [0; 3] } else { [255; 3] });
        let b = image(10, 10, |x, _| if x < 6 { [0; 3] } else { [255; 3] });
        let delta = compare(&a, &b).expect("same size");
        assert_eq!(delta.percent, 0.0);
        assert!(delta.mean_abs > 0.0, "the plain error still sees the shift");
    }

    #[test]
    fn missing_content_counts_every_pixel_it_covered() {
        let drawn = image(10, 10, |x, y| if (2..6).contains(&x) && (2..6).contains(&y) { [200, 0, 0] } else { [255; 3] });
        let blank = image(10, 10, |_, _| [255; 3]);
        // Checked both ways, so the square's edge counts too, although white lies next to it.
        assert_eq!(compare(&blank, &drawn).expect("same size").percent, 16.0);
        let close = image(10, 10, |x, y| if (2..6).contains(&x) && (2..6).contains(&y) { [200, 20, 20] } else { [255; 3] });
        assert_eq!(compare(&close, &drawn).expect("same size").percent, 0.0, "within the tolerance");
    }

    #[test]
    fn different_sizes_are_an_error() {
        assert!(compare(&image(2, 2, |_, _| [0; 3]), &image(2, 3, |_, _| [0; 3])).is_err());
    }

    #[test]
    fn diff_image_marks_differences_in_red() {
        let ours = image(3, 1, |x, _| if x == 1 { [0; 3] } else { [255; 3] });
        let oracle = image(3, 1, |_, _| [255; 3]);
        let diff = diff_image(&oracle, &compare(&ours, &oracle).expect("same size"));
        assert_eq!(diff, [255, 255, 255, 255, 0, 0, 255, 255, 255]);
    }

    #[test]
    fn fixtures_are_well_formed_one_page_pdfs() {
        for fixture in FIXTURES {
            let doc = lopdf::Document::load_mem(&(fixture.build)()).unwrap_or_else(|e| panic!("{}: {e}", fixture.name));
            assert_eq!(doc.get_pages().len(), 1, "{}", fixture.name);
            for (id, object) in &doc.objects {
                if let Ok(s) = object.as_stream() {
                    let length = s.dict.get(b"Length").and_then(lopdf::Object::as_i64).expect("stream length");
                    assert_eq!(usize::try_from(length).ok(), Some(s.content.len()), "{} object {id:?}", fixture.name);
                }
            }
            assert!(fixture.limit_percent > 0.0 && fixture.limit_percent * BLANK_FACTOR < 100.0, "{}", fixture.name);
        }
    }
}
