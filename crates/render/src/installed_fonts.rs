//! Fonts installed on this machine, standing in for fonts a document names but doesn't embed.
//!
//! A PDF that names `CenturyGothic` without embedding it is drawn, by the renderer alone, with
//! one of the standard 14 chosen from the font descriptor's flags: a serif flag makes the whole
//! letter Times. Every other viewer finds Century Gothic on the machine and draws the letter as
//! it was set. This module does the same: the first time an installed font is asked for, the
//! system's font folders are indexed by the names in each file's `name` table (only that table
//! is read), and a document font is matched by its PostScript name, then by family and style.
//!
//! Off by default, so renders in tests, the CLI and the automation tools stay the same on every
//! machine; the desktop app turns it on (`PDFCRAFT_SYSTEM_FONTS=0` keeps it off, as for the
//! interface fallback font). Nothing read here enters a PDF (AGENTS.md §1.4): it is drawn on
//! screen and dropped.

use hayro::hayro_interpret::font::{FallbackFontQuery, FontData, FontQuery};
use skrifa::raw::tables::name::Name;
use skrifa::raw::{FontData as RawData, FontRead};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Turns the installed-font stand-ins on or off for every render in this process.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// Whether installed fonts stand in for fonts documents don't embed.
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// The folders to index instead of the system's (tests), and the index built from them.
struct State {
    directories: Option<Vec<PathBuf>>,
    index: Option<Arc<Index>>,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State { directories: None, index: None }))
}

/// Indexes `directories` instead of the system's font folders from now on (and drops any index
/// already built). For tests.
#[cfg(test)]
pub(crate) fn set_directories(directories: Vec<PathBuf>) {
    let mut state = state().lock().unwrap_or_else(|e| e.into_inner());
    state.directories = Some(directories);
    state.index = None;
}

/// The installed font for a font the document doesn't embed, with its face index, or `None`
/// when stand-ins are off, the query names no font, or no installed font matches.
pub(crate) fn resolve(query: &FontQuery) -> Option<(FontData, u32)> {
    if !enabled() {
        return None;
    }
    let FontQuery::Fallback(query) = query else { return None };
    let index = index();
    let face = index.find(query)?;
    let data = index.load(face)?;
    Some((data, face.index))
}

fn index() -> Arc<Index> {
    let mut state = state().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(index) = &state.index {
        return Arc::clone(index);
    }
    let directories = state.directories.clone().unwrap_or_else(system_directories);
    let index = Arc::new(Index::scan(&directories));
    log::info!("{} installed font faces indexed from {} folders", index.faces.len(), directories.len());
    state.index = Some(Arc::clone(&index));
    index
}

/// The folders the operating system and the user install fonts in.
fn system_directories() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut dirs: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "macos") {
        dirs.extend(home.iter().map(|h| h.join("Library/Fonts")));
        dirs.extend(["/Library/Fonts", "/System/Library/Fonts", "/System/Library/Fonts/Supplemental"].map(PathBuf::from));
    } else if cfg!(target_os = "windows") {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("Microsoft/Windows/Fonts"));
        }
        if let Some(windir) = std::env::var_os("WINDIR") {
            dirs.push(PathBuf::from(windir).join("Fonts"));
        }
    } else {
        dirs.extend(home.iter().flat_map(|h| [h.join(".local/share/fonts"), h.join(".fonts")]));
        dirs.extend(["/usr/local/share/fonts", "/usr/share/fonts"].map(PathBuf::from));
    }
    dirs
}

/// One face of an installed font file.
#[derive(Debug, Clone)]
struct Face {
    path: PathBuf,
    /// The face within a collection (`.ttc`); 0 for a single-face file.
    index: u32,
    /// The PostScript name and the full name, as `key` spells them.
    names: Vec<String>,
    /// The family (typographic family when the file has one), as `key` spells it.
    family: String,
    bold: bool,
    italic: bool,
    /// The family's plain face (`Regular`, `Book`): preferred to `Medium` or `Light` when the
    /// query asks for neither bold nor italic.
    plain: bool,
}

/// The faces found in the font folders, by name and by family.
struct Index {
    faces: Vec<Face>,
    by_name: HashMap<String, usize>,
    by_family: HashMap<String, Vec<usize>>,
    /// Files read so far: a render pool's workers ask for the same font again and again.
    loaded: Mutex<HashMap<PathBuf, Arc<Vec<u8>>>>,
}

/// Folders are walked no deeper than this (Linux font trees nest a few levels).
const MAX_DEPTH: usize = 4;
/// Faces read in one collection file.
const MAX_FACES: u32 = 64;
/// Tables a face's directory may list; real fonts have a few dozen.
const MAX_TABLES: u16 = 512;
/// A `name` table longer than this isn't read: a font path is still untrusted input.
const MAX_NAME_TABLE: u32 = 4 << 20;
/// A font file larger than this is never loaded.
const MAX_FILE_BYTES: u64 = 64 << 20;

impl Index {
    fn scan(directories: &[PathBuf]) -> Self {
        let mut faces = Vec::new();
        for dir in directories {
            walk(dir, 0, &mut faces);
        }
        let mut by_name = HashMap::new();
        let mut by_family: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, face) in faces.iter().enumerate() {
            for name in &face.names {
                by_name.entry(name.clone()).or_insert(i);
            }
            if !face.family.is_empty() {
                by_family.entry(face.family.clone()).or_default().push(i);
            }
        }
        Self { faces, by_name, by_family, loaded: Mutex::new(HashMap::new()) }
    }

    /// The face for `query`: the font's PostScript name (or `/FontName`) first, then its family
    /// (the name's part before `,` or `-`, or the descriptor's `/FontFamily`) in the style the
    /// query asks for.
    fn find(&self, query: &FallbackFontQuery) -> Option<&Face> {
        let names = [query.post_script_name.as_deref(), query.font_name.as_deref()].into_iter().flatten();
        let mut families = Vec::new();
        for name in names {
            if let Some(&i) = self.by_name.get(&key(name)) {
                return self.faces.get(i);
            }
            let (family, bold, italic) = split_style(name);
            families.push((key(family), bold, italic));
        }
        if let Some(family) = &query.font_family {
            families.push((key(family), false, false));
        }
        let bold = query.is_bold || query.font_weight >= 600;
        let italic = query.is_italic;
        for (family, name_bold, name_italic) in families {
            let Some(candidates) = self.by_family.get(&family) else { continue };
            let want = (bold || name_bold, italic || name_italic);
            let face = candidates
                .iter()
                .filter_map(|&i| self.faces.get(i))
                .min_by_key(|f| ((f.bold, f.italic) != want, f.italic != want.1, f.bold || f.italic, !f.plain));
            if face.is_some() {
                return face;
            }
        }
        None
    }

    fn load(&self, face: &Face) -> Option<FontData> {
        let mut loaded = self.loaded.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(bytes) = loaded.get(&face.path) {
            let data: FontData = bytes.clone();
            return Some(data);
        }
        let meta = std::fs::metadata(&face.path).ok()?;
        if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
            return None;
        }
        let bytes = Arc::new(std::fs::read(&face.path).ok()?);
        log::info!("installed font {} (face {}) stands in for a font the document doesn't embed", face.path.display(), face.index);
        loaded.insert(face.path.clone(), Arc::clone(&bytes));
        let data: FontData = bytes;
        Some(data)
    }
}

/// How a name is matched: letters and digits only, lower case, so `Century Gothic Bold`,
/// `CenturyGothic-Bold` and `CenturyGothic,Bold` meet.
fn key(name: &str) -> String {
    name.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()).collect()
}

/// A font name's family and the style its suffix spells: `Arial,BoldItalic`, `Calibri-Bold`,
/// `Verdana-Italic`.
fn split_style(name: &str) -> (&str, bool, bool) {
    let (family, style) = name.split_once([',', '-']).unwrap_or((name, ""));
    let style = style.to_ascii_lowercase();
    (family, is_bold(&style), is_italic(&style))
}

fn is_bold(style: &str) -> bool {
    ["bold", "black", "heavy", "extrabold", "ultrabold"].iter().any(|s| style.contains(s))
}

fn is_italic(style: &str) -> bool {
    style.contains("italic") || style.contains("oblique")
}

fn walk(dir: &Path, depth: usize, faces: &mut Vec<Face>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if depth < MAX_DEPTH {
                walk(&path, depth + 1, faces);
            }
            continue;
        }
        let is_font = path.extension().and_then(|e| e.to_str()).is_some_and(|e| ["ttf", "otf", "ttc"].iter().any(|f| e.eq_ignore_ascii_case(f)));
        if is_font {
            faces.extend(read_faces(&path));
        }
    }
}

/// The faces of one font file, from their `name` tables alone (the file isn't read whole).
fn read_faces(path: &Path) -> Vec<Face> {
    let Ok(mut file) = std::fs::File::open(path) else { return Vec::new() };
    let mut header = [0u8; 12];
    if file.read_exact(&mut header).is_err() {
        return Vec::new();
    }
    let offsets: Vec<u32> = if &header[..4] == b"ttcf" {
        let count = u32::from_be_bytes([header[8], header[9], header[10], header[11]]).min(MAX_FACES);
        let Ok(bytes) = read_at(&mut file, 12, (count as usize).saturating_mul(4)) else { return Vec::new() };
        bytes.as_chunks::<4>().0.iter().map(|c| u32::from_be_bytes(*c)).collect()
    } else {
        vec![0]
    };
    offsets
        .iter()
        .enumerate()
        .filter_map(|(index, &offset)| {
            let names = name_table(&mut file, offset)?;
            face_from_names(path, index as u32, &names)
        })
        .collect()
}

fn read_at(file: &mut std::fs::File, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0u8; len];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// The `name` table of the face whose table directory starts at `offset`.
fn name_table(file: &mut std::fs::File, offset: u32) -> Option<Vec<u8>> {
    let directory = read_at(file, u64::from(offset), 12).ok()?;
    let count = u16::from_be_bytes([directory[4], directory[5]]).min(MAX_TABLES);
    let records = read_at(file, u64::from(offset).checked_add(12)?, usize::from(count).saturating_mul(16)).ok()?;
    let record = records.as_chunks::<16>().0.iter().find(|r| &r[..4] == b"name")?;
    let table_offset = u32::from_be_bytes([record[8], record[9], record[10], record[11]]);
    let length = u32::from_be_bytes([record[12], record[13], record[14], record[15]]);
    if length > MAX_NAME_TABLE {
        return None;
    }
    read_at(file, u64::from(table_offset), length as usize).ok()
}

/// Name ids: PostScript name, full name, family, subfamily, typographic family and subfamily.
const POSTSCRIPT_NAME: u16 = 6;
const FULL_NAME: u16 = 4;
const FAMILY: u16 = 1;
const SUBFAMILY: u16 = 2;
const TYPOGRAPHIC_FAMILY: u16 = 16;
const TYPOGRAPHIC_SUBFAMILY: u16 = 17;

fn face_from_names(path: &Path, index: u32, table: &[u8]) -> Option<Face> {
    let name = Name::read(RawData::new(table)).ok()?;
    let data = name.string_data();
    // The English Windows record first (fonts spell their names there), then any Unicode or
    // Macintosh one.
    let string = |id: u16| {
        let records: Vec<_> = name.name_record().iter().filter(|r| r.name_id().to_u16() == id).collect();
        records
            .iter()
            .find(|r| r.platform_id() == 3 && r.language_id() == 0x409)
            .or_else(|| records.iter().find(|r| r.platform_id() == 3 || r.platform_id() == 0))
            .or_else(|| records.first())
            .and_then(|r| r.string(data).ok())
            .map(|s| s.to_string())
    };
    let postscript = string(POSTSCRIPT_NAME);
    let full = string(FULL_NAME);
    let family = string(TYPOGRAPHIC_FAMILY).or_else(|| string(FAMILY))?;
    let subfamily = string(TYPOGRAPHIC_SUBFAMILY).or_else(|| string(SUBFAMILY)).unwrap_or_default().to_ascii_lowercase();
    let names: Vec<String> = [postscript, full].into_iter().flatten().map(|n| key(&n)).filter(|k| !k.is_empty()).collect();
    if names.is_empty() {
        return None;
    }
    let plain = matches!(subfamily.as_str(), "" | "regular" | "normal" | "roman" | "book" | "plain");
    Some(Face { path: path.to_path_buf(), index, names, family: key(&family), bold: is_bold(&subfamily), italic: is_italic(&subfamily), plain })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> Vec<PathBuf> {
        vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts")]
    }

    fn query(name: &str) -> FallbackFontQuery {
        FallbackFontQuery { post_script_name: Some(name.into()), ..FallbackFontQuery::default() }
    }

    #[test]
    fn faces_are_found_by_postscript_name_then_by_family_and_style() {
        let index = Index::scan(&assets());
        let file = |f: &Face| f.path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        assert_eq!(index.find(&query("Inter-Regular")).map(file).as_deref(), Some("Inter-Regular.ttf"));
        assert_eq!(index.find(&query("JetBrainsMono-Regular")).map(file).as_deref(), Some("JetBrainsMono-Regular.ttf"));
        // Full name, spelled with spaces, and a family with a style suffix.
        assert_eq!(index.find(&query("Inter Medium")).map(file).as_deref(), Some("Inter-Medium.ttf"));
        assert_eq!(index.find(&query("Inter,Bold")).map(file).as_deref(), Some("Inter-SemiBold.ttf"));
        assert_eq!(index.find(&query("Inter-Bold")).map(file).as_deref(), Some("Inter-SemiBold.ttf"));
        // The descriptor's flags choose the style when the name doesn't.
        let bold = FallbackFontQuery { is_bold: true, ..query("Inter") };
        assert_eq!(index.find(&bold).map(file).as_deref(), Some("Inter-SemiBold.ttf"));
        assert_eq!(index.find(&query("Inter")).map(file).as_deref(), Some("Inter-Regular.ttf"));
        // Only the descriptor's family.
        let family = FallbackFontQuery { font_family: Some("Dancing Script".into()), ..FallbackFontQuery::default() };
        assert_eq!(index.find(&family).map(file).as_deref(), Some("DancingScript.ttf"));
        assert!(index.find(&query("NoSuchFont-Bold")).is_none());
        assert!(index.find(&FallbackFontQuery::default()).is_none());
    }

    #[test]
    fn names_are_matched_without_case_spaces_or_punctuation() {
        assert_eq!(key("Century Gothic Bold"), "centurygothicbold");
        assert_eq!(key("CenturyGothic-Bold"), "centurygothicbold");
        assert_eq!(split_style("Arial,BoldItalic"), ("Arial", true, true));
        assert_eq!(split_style("Calibri-Bold"), ("Calibri", true, false));
        assert_eq!(split_style("Verdana"), ("Verdana", false, false));
    }

    #[test]
    fn broken_missing_and_oversized_files_are_skipped() {
        let dir = std::env::temp_dir().join(format!("pdfcraft-installed-fonts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("empty.ttf"), b"").unwrap();
        std::fs::write(dir.join("short.otf"), b"OTTO").unwrap();
        std::fs::write(dir.join("junk.ttc"), b"ttcf\0\0\0\x01\xff\xff\xff\xff\xff\xff\xff\xff").unwrap();
        std::fs::write(dir.join("notafont.ttf"), vec![0u8; 4096]).unwrap();
        assert!(Index::scan(&[dir.clone(), PathBuf::from("/no/such/folder")]).faces.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
