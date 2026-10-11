//! The last-resort interface font: one face already installed on this machine.
//!
//! The embedded faces (Inter, egui's defaults, craft-fonts) come first in every family; this one
//! only draws characters none of them has, such as an Arabic file name in a build without
//! craft-fonts. A second installed face covers Hebrew when the first has no Hebrew letters. It is read at runtime and never embedded or shipped (AGENTS.md §1.4), and
//! `PDFCRAFT_SYSTEM_FONTS=0` turns it off (published screenshots do).

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use egui::FontData;

/// Larger files are not read: a font path is still untrusted input.
const MAX_BYTES: u64 = 32 << 20;
/// Faces tried in a collection (`.ttc`).
const MAX_FACES: u32 = 16;
/// Arabic letter alef: the face must have it to be worth loading.
const PROBE: char = '\u{0627}';
/// Hebrew letter alef: the probe for the Hebrew face.
const HEBREW_PROBE: char = '\u{05D0}';

/// The installed fallback face, read once. `None` when it is turned off or no candidate fits.
pub fn fallback() -> Option<Arc<FontData>> {
    static CACHE: OnceLock<Option<Arc<FontData>>> = OnceLock::new();
    CACHE.get_or_init(load).clone()
}

/// An installed face for Hebrew (the Hebrew interface, file names), read once, when
/// [`fallback`] has no Hebrew letters (Noto Sans Arabic, SF Arabic). `None` when it is turned
/// off, the first face already covers Hebrew or no candidate fits.
pub fn hebrew_fallback() -> Option<Arc<FontData>> {
    static CACHE: OnceLock<Option<Arc<FontData>>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            if !enabled() || fallback().is_some_and(|data| covers(&data, HEBREW_PROBE)) {
                return None;
            }
            hebrew_candidates().iter().find_map(|path| read_with(path, HEBREW_PROBE))
        })
        .clone()
}

fn enabled() -> bool {
    !std::env::var_os("PDFCRAFT_SYSTEM_FONTS").is_some_and(|v| v == "0")
}

fn load() -> Option<Arc<FontData>> {
    if !enabled() {
        return None;
    }
    candidates().iter().find_map(|path| read(path))
}

/// Whether the loaded face maps `c`.
fn covers(data: &FontData, c: char) -> bool {
    use skrifa::MetadataProvider as _;
    skrifa::FontRef::from_index(&data.font, data.index).is_ok_and(|font| font.charmap().map(c).is_some())
}

/// Well-known locations of faces with Hebrew letters, best first.
fn hebrew_candidates() -> Vec<PathBuf> {
    if cfg!(windows) {
        let dir = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot")).map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        ["segoeui.ttf", "tahoma.ttf", "arial.ttf"].iter().map(|f| dir.join("Fonts").join(f)).collect()
    } else if cfg!(target_os = "macos") {
        ["/System/Library/Fonts/SFHebrew.ttf", "/System/Library/Fonts/ArialHB.ttc", "/System/Library/Fonts/Supplemental/Arial.ttf"]
            .iter()
            .map(PathBuf::from)
            .collect()
    } else {
        let files = [
            "truetype/noto/NotoSansHebrew-Regular.ttf",
            "noto/NotoSansHebrew-Regular.ttf",
            "google-noto/NotoSansHebrew-Regular.ttf",
            "truetype/noto/NotoSansHebrew[wdth,wght].ttf",
            "truetype/dejavu/DejaVuSans.ttf",
            "TTF/DejaVuSans.ttf",
            "dejavu/DejaVuSans.ttf",
            "dejavu-sans-fonts/DejaVuSans.ttf",
            "truetype/culmus/NachlieliCLM-Light.otf",
        ];
        ["/usr/share/fonts", "/usr/local/share/fonts"].iter().flat_map(|dir| files.iter().map(move |f| Path::new(dir).join(f))).collect()
    }
}

/// Well-known locations of faces with broad script coverage, best first.
fn candidates() -> Vec<PathBuf> {
    if cfg!(windows) {
        let dir = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot")).map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        ["segoeui.ttf", "tahoma.ttf", "arial.ttf"].iter().map(|f| dir.join("Fonts").join(f)).collect()
    } else if cfg!(target_os = "macos") {
        ["/System/Library/Fonts/SFArabic.ttf", "/System/Library/Fonts/GeezaPro.ttc", "/System/Library/Fonts/Supplemental/Arial.ttf"]
            .iter()
            .map(PathBuf::from)
            .collect()
    } else {
        let files = [
            "truetype/noto/NotoSansArabic-Regular.ttf",
            "noto/NotoSansArabic-Regular.ttf",
            "google-noto/NotoSansArabic-Regular.ttf",
            "truetype/dejavu/DejaVuSans.ttf",
            "TTF/DejaVuSans.ttf",
            "dejavu/DejaVuSans.ttf",
            "dejavu-sans-fonts/DejaVuSans.ttf",
        ];
        ["/usr/share/fonts", "/usr/local/share/fonts"].iter().flat_map(|dir| files.iter().map(move |f| Path::new(dir).join(f))).collect()
    }
}

fn read(path: &Path) -> Option<Arc<FontData>> {
    read_with(path, PROBE)
}

/// The first face of the file at `path` that maps `probe`.
fn read_with(path: &Path, probe: char) -> Option<Arc<FontData>> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let index = face_with(&bytes, probe)?;
    let mut data = FontData::from_owned(bytes);
    data.index = index;
    log::info!("interface font fallback: {}", path.display());
    Some(Arc::new(data))
}

/// The first face of the file that parses and maps `c`. egui parses fonts with the same skrifa,
/// so a face accepted here is one it can load.
fn face_with(bytes: &[u8], c: char) -> Option<u32> {
    use skrifa::MetadataProvider as _;
    (0..MAX_FACES).find(|&index| skrifa::FontRef::from_index(bytes, index).is_ok_and(|font| font.charmap().map(c).is_some()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broken_and_missing_files_are_skipped() {
        assert_eq!(face_with(b"", PROBE), None);
        assert_eq!(face_with(b"not a font at all", PROBE), None);
        assert_eq!(face_with(&[0u8; 4096], PROBE), None);
        assert!(read(Path::new("definitely/not/here.ttf")).is_none());
        // A directory is not a font.
        assert!(read(&std::env::temp_dir()).is_none());
    }

    #[test]
    fn a_face_without_the_probe_is_rejected() {
        // Inter is Latin, Greek and Cyrillic only.
        let inter = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
        assert_eq!(face_with(inter, PROBE), None);
        assert_eq!(face_with(inter, 'A'), Some(0));
    }

    #[test]
    fn candidates_are_absolute_font_files() {
        for list in [candidates(), hebrew_candidates()] {
            assert!(!list.is_empty());
            assert!(list.iter().all(|p| p.is_absolute() || cfg!(windows)));
            assert!(list.iter().all(|p| p.extension().is_some_and(|e| e == "ttf" || e == "ttc" || e == "otf")));
        }
    }

    #[test]
    fn the_hebrew_probe_rejects_latin_only_faces() {
        let inter = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
        assert_eq!(face_with(inter, HEBREW_PROBE), None);
        assert!(!covers(&FontData::from_static(inter), HEBREW_PROBE));
        assert!(covers(&FontData::from_static(inter), 'A'));
        assert!(read_with(Path::new("definitely/not/here.ttf"), HEBREW_PROBE).is_none());
    }
}
