//! The release font gate must reject the old Japanese-only input, rather than skip a test.

#[path = "../release_fonts.rs"]
mod release_fonts;

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use sha2::{Digest as _, Sha256};

struct Input(PathBuf);

impl Input {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("pdfcraft-release-fonts-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(dir.join("fonts")).unwrap();
        Self(dir)
    }

    fn manifest(&self, text: &str) {
        std::fs::write(self.0.join("fonts/manifest.txt"), text).unwrap();
    }

    fn face(&self, bytes: &[u8]) {
        std::fs::write(self.0.join("fonts/test.ttf"), bytes).unwrap();
        let hash: String = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
        self.manifest(&format!("Test | Regular | fonts/test.ttf | Hans | OFL-1.1 | OFL.txt | {hash} | test"));
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn old_release_input_is_rejected() {
    let input = Input::new();
    input.manifest("BIZ UDPGothic | Regular | fonts/unused.ttf | Jpan,Latn | OFL-1.1 | OFL.txt | hash | source");
    assert!(release_fonts::validate(&input.0, "\tWelcome\t欢迎").unwrap_err().contains("no Hans face"));
}

#[test]
fn a_hans_label_cannot_hide_missing_glyphs() {
    let input = Input::new();
    input.face(include_bytes!("../../../assets/fonts/Inter-Regular.ttf"));
    assert!(release_fonts::validate(&input.0, "\tWelcome\t欢迎").unwrap_err().contains("欢迎"));
}

#[test]
fn corrupt_fonts_and_hash_mismatches_are_rejected() {
    let input = Input::new();
    input.face(b"not a font");
    assert!(release_fonts::validate(&input.0, "\tWelcome\t欢迎").unwrap_err().contains("invalid font"));
    std::fs::write(input.0.join("fonts/test.ttf"), b"changed").unwrap();
    assert!(release_fonts::validate(&input.0, "\tWelcome\t欢迎").unwrap_err().contains("SHA-256 mismatch"));
}

#[test]
fn only_translated_han_characters_are_required() {
    assert_eq!(release_fonts::catalog_han("# 注释\n\tEnglish 中文\t欢迎 {name}\n"), ['欢', '迎'].into_iter().collect());
}
