//! Reproducible addition to the release-pinned craft-fonts input; no font enters this repository.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use crate::assets;

fn allowed_manifest(dir: &Path, text: &str) -> Result<String> {
    let barred = ["Source Han", "Source Serif", "Source Sans", "Noto Sans CJK", "Noto Serif CJK"];
    for line in text.lines().filter(|line| barred.iter().any(|family| line.trim_start().starts_with(family))) {
        if let Some(file) = line.split(" | ").nth(2)
            && dir.join(file.trim()).exists()
        {
            bail!("prohibited font present in craft-fonts; use the documented filtered sparse checkout");
        }
    }
    Ok(text.lines().filter(|line| !barred.iter().any(|family| line.trim_start().starts_with(family))).map(|line| format!("{line}\n")).collect())
}

pub fn run(args: &[String]) -> Result<()> {
    if args.len() > 1 {
        bail!("usage: cargo xtask fonts [CRAFT_FONTS_DIR]");
    }
    let dir = args.first().map(PathBuf::from).unwrap_or_else(|| assets::root().join("craft-fonts"));
    let manifest_path = dir.join("fonts/manifest.txt");
    let mut text = std::fs::read_to_string(&manifest_path).context("check out craft-fonts at the release-pinned commit first")?;
    // Release/CI use a blob-filtered sparse checkout: the prohibited CJK directory
    // is never fetched. Remove its upstream manifest record before adding our face.
    text = allowed_manifest(&dir, &text)?;
    let mut manifest = assets::load(&assets::root())?;
    manifest.fetched.retain(|f| f.build_input.as_deref() == Some("craft-fonts"));
    if manifest.fetched.len() != 1 {
        bail!("expected one attributed Chinese interface font for craft-fonts");
    }
    let font = manifest.fetched.first().context("missing Chinese font attribution")?;
    let entry = format!(
        "Droid Sans Fallback | Regular | {} | Hans,Latn | {} | {}.LICENCE.txt | {} | {}",
        font.file, font.licence, font.file, font.sha256, font.source
    );
    // Refuse to replace another Chinese font (in particular an unreviewed newer input).
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        if line.split(" | ").nth(3).is_some_and(|s| s.split(',').any(|s| s.trim() == "Hans")) && line.trim() != entry {
            bail!("craft-fonts already contains a different Hans face; use the release-pinned checkout");
        }
    }
    assets::fetch_all(&manifest, &dir, "font")?;
    if !text.lines().any(|line| line.trim() == entry) {
        text.push_str(&format!("\n{entry}\n"));
    }
    std::fs::write(&manifest_path, text)?;
    let attribution_path = dir.join("ATTRIBUTION.md");
    let mut attribution = std::fs::read_to_string(&attribution_path)?;
    if !attribution.contains(&font.file) {
        attribution.push_str(&format!(
            "\n## PdfCraft's pinned Chinese UI input\n\n{} — {}, by {}. Source: {}. Licence: {} ({}.LICENCE.txt). SHA-256: {}.\n",
            font.file, font.title, font.author, font.source, font.licence, font.file, font.sha256
        ));
        std::fs::write(attribution_path, attribution)?;
    }
    println!("fonts: prepared {}; set CRAFT_FONTS_DIR to this directory", dir.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_manifest_excludes_barred_face_and_preserves_arabic() {
        let root = assets::root();
        let text = "# manifest\nNoto Sans CJK SC | Regular | absent.ttf | Hans\nNoto Sans Arabic | Regular | arabic.ttf | Arab\n";
        let clean = allowed_manifest(&root, text).unwrap();
        assert!(!clean.contains("Noto Sans CJK"));
        assert!(clean.contains("Noto Sans Arabic"));
        assert_eq!(allowed_manifest(&root, &clean).unwrap(), clean);
    }

    #[test]
    fn an_existing_barred_file_is_rejected_without_reading_it() {
        // Existing source code stands in for a forbidden file; no prohibited asset is used.
        let text = "Noto Sans CJK SC | Regular | xtask/src/fonts.rs | Hans\n";
        assert!(allowed_manifest(&assets::root(), text).unwrap_err().to_string().contains("prohibited font present"));
    }
}
