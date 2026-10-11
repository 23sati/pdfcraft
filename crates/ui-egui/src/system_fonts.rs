//! The last-resort interface faces: already-installed fonts used only for missing scripts.
//!
//! The embedded faces (Inter, egui's defaults, craft-fonts) come first in every family; this one
//! only draws characters none of them has, such as Chinese UI text or an Arabic file name in a
//! build without craft-fonts. It is read at runtime and never embedded or shipped (AGENTS.md
//! §1.4), and `PDFCRAFT_SYSTEM_FONTS=0` turns it off (published screenshots do).

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use egui::FontData;

/// Larger files are not read: a font path is still untrusted input.
const MAX_BYTES: u64 = 32 << 20;
/// Faces tried in a collection (`.ttc`).
const MAX_FACES: u32 = 16;
/// Arabic letter alef: the face must have it to be worth loading.
const ARABIC_PROBE: char = '\u{0627}';
/// A common Han character: a Chinese face must have it to be worth loading.
const HANS_PROBE: char = '\u{4e2d}';

/// The installed fallback face, read once per script. `None` when it is turned off or no
/// candidate fits.
pub fn fallback(prefer_hans: bool) -> Option<Arc<FontData>> {
    static ARABIC_CACHE: OnceLock<Option<Arc<FontData>>> = OnceLock::new();
    static HANS_CACHE: OnceLock<Option<Arc<FontData>>> = OnceLock::new();
    let cache = if prefer_hans { &HANS_CACHE } else { &ARABIC_CACHE };
    cache.get_or_init(|| load(prefer_hans)).clone()
}

fn load(prefer_hans: bool) -> Option<Arc<FontData>> {
    if std::env::var_os("PDFCRAFT_SYSTEM_FONTS").is_some_and(|v| v == "0") {
        return None;
    }
    candidates(prefer_hans).iter().find_map(|path| read(path, if prefer_hans { HANS_PROBE } else { ARABIC_PROBE }))
}

/// Well-known locations of faces with broad script coverage, best first.
fn candidates(prefer_hans: bool) -> Vec<PathBuf> {
    if prefer_hans && cfg!(windows) {
        let dir = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot")).map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        ["msyh.ttc", "msyhbd.ttc", "simsun.ttc", "simhei.ttf"].iter().map(|f| dir.join("Fonts").join(f)).collect()
    } else if prefer_hans && cfg!(target_os = "macos") {
        [
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/System/Library/Fonts/STHeiti Medium.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        ]
        .iter()
        .map(PathBuf::from)
        .collect()
    } else if prefer_hans {
        [
            "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
            "/usr/share/fonts/truetype/arphic/uming.ttc",
            "/usr/share/fonts/truetype/arphic/ukai.ttc",
            "/usr/share/fonts/opentype/unifont/unifont.otf",
        ]
        .iter()
        .map(PathBuf::from)
        .collect()
    } else if cfg!(windows) {
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

fn read(path: &Path, probe: char) -> Option<Arc<FontData>> {
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
    use skrifa::{MetadataProvider as _, string::StringId};

    let mut first_match = None;
    for index in 0..MAX_FACES {
        let Ok(font) = skrifa::FontRef::from_index(bytes, index) else {
            continue;
        };
        if font.charmap().map(c).is_none() {
            continue;
        }
        if first_match.is_none() {
            first_match = Some(index);
        }
        // macOS ships a metrics-normalized Interface face alongside the regular Hiragino face.
        // Prefer it for UI fallback so Han glyphs do not look oversized next to Inter.
        if font.localized_strings(StringId::FAMILY_NAME).any(|name| name.to_string().contains("Interface")) {
            return Some(index);
        }
    }
    first_match
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broken_and_missing_files_are_skipped() {
        assert_eq!(face_with(b"", ARABIC_PROBE), None);
        assert_eq!(face_with(b"not a font at all", ARABIC_PROBE), None);
        assert_eq!(face_with(&[0u8; 4096], ARABIC_PROBE), None);
        assert!(read(Path::new("definitely/not/here.ttf"), ARABIC_PROBE).is_none());
        // A directory is not a font.
        assert!(read(&std::env::temp_dir(), ARABIC_PROBE).is_none());
    }

    #[test]
    fn a_face_without_the_probe_is_rejected() {
        // Inter is Latin, Greek and Cyrillic only.
        let inter = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
        assert_eq!(face_with(inter, ARABIC_PROBE), None);
        assert_eq!(face_with(inter, HANS_PROBE), None);
        assert_eq!(face_with(inter, 'A'), Some(0));
    }

    #[test]
    fn candidates_are_absolute_font_files() {
        let list = candidates(false);
        assert!(!list.is_empty());
        assert!(list.iter().all(|p| p.extension().is_some_and(|e| e == "ttf" || e == "ttc")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn chinese_candidates_include_a_system_hans_face() {
        let list = candidates(true);
        assert!(list.iter().any(|p| p.ends_with("Hiragino Sans GB.ttc") || p.ends_with("STHeiti Medium.ttc") || p.ends_with("Arial Unicode.ttf")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn hiragino_prefers_its_interface_face() {
        let path = Path::new("/System/Library/Fonts/Hiragino Sans GB.ttc");
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        assert_eq!(face_with(&bytes, HANS_PROBE), Some(1));
    }
}
