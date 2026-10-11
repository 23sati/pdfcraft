//! Build-time release gate, also exercised by integration tests. Never reads system fonts.

use std::collections::BTreeSet;
use std::path::Path;

use sha2::{Digest as _, Sha256};
use skrifa::MetadataProvider as _;

pub fn catalog_han(catalog: &str) -> BTreeSet<char> {
    catalog
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split('\t').nth(2))
        .flat_map(str::chars)
        .filter(|c| matches!(*c as u32, 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff | 0x20000..=0x2fa1f | 0x30000..=0x323af))
        .collect()
}

pub fn validate(dir: &Path, catalog: &str) -> Result<(), String> {
    let manifest = dir.join("fonts/manifest.txt");
    println!("cargo::rerun-if-changed={}", manifest.display());
    let text = std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let root = dir.canonicalize().map_err(|e| e.to_string())?;
    let mut missing = catalog_han(catalog);
    let mut faces = 0usize;
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {
        let fields: Vec<_> = line.split(" | ").map(str::trim).collect();
        let [_, _, file, scripts, _, _, hash, ..] = fields.as_slice() else {
            return Err(format!("malformed font manifest line: {line}"));
        };
        if !scripts.split(',').any(|script| script.trim() == "Hans") {
            continue;
        }
        let path = dir.join(file).canonicalize().map_err(|e| format!("{file}: {e}"))?;
        if !path.starts_with(&root) {
            return Err(format!("{file}: font must stay inside craft-fonts"));
        }
        println!("cargo::rerun-if-changed={}", path.display());
        let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.len() > 32 << 20 {
            return Err(format!("{file}: expected a font file of at most 32 MiB"));
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("{file}: {e}"))?;
        let actual: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        if actual != *hash {
            return Err(format!("{file}: SHA-256 mismatch"));
        }
        let font = skrifa::FontRef::from_index(&bytes, 0).map_err(|e| format!("{file}: invalid font: {e}"))?;
        let charmap = font.charmap();
        missing.retain(|c| charmap.map(*c).is_none());
        faces = faces.saturating_add(1);
    }
    if faces == 0 {
        return Err("no Hans face in craft-fonts (the old release input only contains Japanese faces)".to_owned());
    }
    if !missing.is_empty() {
        return Err(format!("Hans faces lack {} catalog characters: {}", missing.len(), missing.iter().take(32).collect::<String>()));
    }
    Ok(())
}
