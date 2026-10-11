//! Small synthetic rendering comparison against Poppler as an external oracle.
//!
//! This is the first slice of the fidelity harness (gap 12): it generates its fixture at runtime,
//! renders it through PdfCraft and `pdftoppm`, and reports image deltas without checking in oracle
//! output or treating one renderer's anti-aliasing as an exact golden.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};

pub fn run(args: &[String]) -> anyhow::Result<()> {
    if !args.is_empty() {
        bail!("usage: cargo xtask render-oracle");
    }
    if Command::new("pdftoppm").arg("-v").output().is_err() {
        bail!("pdftoppm (Poppler) not found; install it to run the rendering oracle (brew install poppler / apt install poppler-utils)");
    }

    let root = super::gates::root();
    let target = super::gates::target_dir();
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .current_dir(&root)
        .args(["build", "--release", "-p", "pdfcraft-cli"])
        .status()
        .context("build pdfcraft-cli for the render oracle")?;
    if !status.success() {
        bail!("cargo build --release -p pdfcraft-cli failed with {status}");
    }

    let temp = TempDir::create()?;
    let input = temp.path.join("synthetic.pdf");
    let ours = temp.path.join("pdfcraft.png");
    let oracle_prefix = temp.path.join("poppler");
    let oracle = temp.path.join("poppler.png");
    std::fs::write(&input, synthetic_pdf())?;

    let cli = target.join("release").join(format!("pdfcraft-cli{}", std::env::consts::EXE_SUFFIX));
    let status = Command::new(&cli)
        .arg("render")
        .arg(&input)
        .args(["--page", "1", "--dpi", "72", "--out"])
        .arg(&ours)
        .status()
        .with_context(|| format!("run {}", cli.display()))?;
    if !status.success() {
        bail!("pdfcraft-cli render failed with {status}");
    }
    let status = Command::new("pdftoppm")
        .args(["-f", "1", "-l", "1", "-r", "72", "-png", "-singlefile"])
        .arg(&input)
        .arg(&oracle_prefix)
        .status()
        .context("run pdftoppm (Poppler)")?;
    if !status.success() {
        bail!("pdftoppm failed with {status}");
    }

    let a = Image::read(&ours)?;
    let b = Image::read(&oracle)?;
    let delta = compare(&a, &b)?;
    println!("synthetic PDF render at 72 dpi: {}×{} pixels", delta.width, delta.height);
    println!(
        "Poppler pixel comparison: mean RGB absolute error {:.3}/255; pixels differing by >16: {:.2}%",
        delta.mean_abs, delta.pixels_over_16_percent
    );
    println!("This is a diagnostic measurement; anti-aliasing differs by renderer and no pass/fail threshold is set yet.");
    Ok(())
}

/// A short, in-memory-generated PDF containing flat fills, a stroke, and standard-14 text.
fn synthetic_pdf() -> Vec<u8> {
    let content = b"q 0.92 g 72 660 240 54 re f Q\nq 0.12 0.34 0.78 rg 72 570 240 48 re f Q\nq 0.1 0.1 0.1 RG 1.5 w 72 540 m 312 540 l S Q\nBT /F1 18 Tf 1 0 0 1 72 495 Tm (PdfCraft synthetic fidelity page) Tj ET\nBT /F1 10 Tf 1 0 0 1 72 475 Tm (Vector fills, stroke, baseline text) Tj ET\n";
    let objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_vec(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        [format!("<< /Length {} >>\nstream\n", content.len()).into_bytes(), content.to_vec(), b"endstream".to_vec()].concat(),
    ];
    let mut pdf = b"%PDF-1.7\n% synthetic fidelity fixture\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    pdf
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
        decoder.set_limits(png::Limits { bytes: 16 * 1024 * 1024 });
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().with_context(|| format!("read PNG header {}", path.display()))?;
        if (reader.info().width, reader.info().height) != (612, 792) {
            bail!("unexpected render dimensions in {}: {}×{} (expected 612×792)", path.display(), reader.info().width, reader.info().height);
        }
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
        Ok(Self { width: info.width, height: info.height, rgb: pixels })
    }
}

struct Delta {
    width: u32,
    height: u32,
    mean_abs: f64,
    pixels_over_16_percent: f64,
}

fn compare(a: &Image, b: &Image) -> anyhow::Result<Delta> {
    if (a.width, a.height) != (b.width, b.height) {
        bail!("render dimensions differ: PdfCraft {}×{}, Poppler {}×{}", a.width, a.height, b.width, b.height);
    }
    if a.rgb.len() != b.rgb.len() || a.rgb.is_empty() {
        bail!("render pixel counts differ or are empty");
    }
    let (mut sum, mut over) = (0u64, 0usize);
    for (left, right) in a.rgb.iter().zip(&b.rgb) {
        let mut pixel_over = false;
        for (x, y) in left.iter().zip(right) {
            let difference = x.abs_diff(*y);
            sum = sum.saturating_add(u64::from(difference));
            pixel_over |= difference > 16;
        }
        over += usize::from(pixel_over);
    }
    let channels = a.rgb.len().saturating_mul(3);
    Ok(Delta {
        width: a.width,
        height: a.height,
        mean_abs: sum as f64 / channels as f64,
        pixels_over_16_percent: over as f64 * 100.0 / a.rgb.len() as f64,
    })
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn create() -> anyhow::Result<Self> {
        let root = std::env::temp_dir();
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).context("system clock is before Unix epoch")?.as_nanos();
        for attempt in 0..16u8 {
            let path = root.join(format!("pdfcraft-render-oracle-{}-{stamp}-{attempt}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e).with_context(|| format!("create temporary directory {}", path.display())),
            }
        }
        bail!("could not choose a unique temporary directory for render-oracle")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_delta_uses_rgb_channels_and_marks_changed_pixels() {
        let a = Image { width: 2, height: 1, rgb: vec![[10, 20, 30], [40, 50, 60]] };
        let b = Image { width: 2, height: 1, rgb: vec![[10, 37, 30], [40, 50, 60]] };
        let delta = compare(&a, &b).expect("same-sized images compare");
        assert_eq!((delta.width, delta.height), (2, 1));
        assert!((delta.mean_abs - 17.0 / 6.0).abs() < 0.001);
        assert_eq!(delta.pixels_over_16_percent, 50.0);
    }
}
