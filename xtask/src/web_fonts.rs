//! Ensure standalone web packaging embeds the same verified fonts as native releases.
//! The shared release workflow does not provide a web font checkout. Keep acquisition
//! here (Rust build tooling), never in a crate's build.rs or in the application.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, ensure};

const REPOSITORY: &str = "https://github.com/storytold/craft-fonts";
const COMMIT: &str = "3ed348d5faf558cf77e4f978d927cf07d0c9acb3";
// Same sorted `<sha256>  <relative path>\n` digest as `xtask corpus`, over fonts/.
// Includes every font, the selection manifest and all corresponding licence texts.
const MANIFEST_SHA256: &str = "44c7d1cd285b60477758cca0cf55a3e98631d7beac77ac22c7c8da5e91f17b26";
const FILES: usize = 20;

fn git(directory: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git").arg("-C").arg(directory).args(args).output().context("run Git to prepare web fonts")?;
    ensure!(output.status.success(), "preparing web fonts: {}", String::from_utf8_lossy(&output.stderr));
    Ok(())
}

fn verify_digest(digest: &str, files: usize) -> Result<()> {
    ensure!(
        files == FILES && digest == MANIFEST_SHA256,
        "web fonts do not match the approved {COMMIT} content (expected {FILES} files and SHA-256 {MANIFEST_SHA256}, found {files} files and {digest})"
    );
    Ok(())
}

fn verify(directory: &Path) -> Result<()> {
    let (digest, files) =
        crate::corpus::manifest(&directory.join("fonts")).with_context(|| format!("reading web fonts at {}", directory.display()))?;
    verify_digest(&digest, files)
        .with_context(|| format!("refusing unverified font checkout {}; use the pinned checkout or remove this cache and retry", directory.display()))
}

/// Bash consumes this path for both Cargo's environment and license copying. Windows'
/// extended canonical prefix is not a shell path; use the ordinary drive/UNC spelling.
fn package_path(path: &Path) -> Result<String> {
    let text = path.to_str().context("web font directory must have a UTF-8 path for shell packaging")?;
    #[cfg(windows)]
    {
        if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            return Ok(format!("//{}", unc.replace('\\', "/")));
        }
        Ok(text.strip_prefix(r"\\?\").unwrap_or(text).replace('\\', "/"))
    }
    #[cfg(not(windows))]
    {
        Ok(text.to_owned())
    }
}

pub fn run(args: &[String]) -> Result<()> {
    ensure!(args.is_empty(), "usage: cargo xtask web-fonts");
    let root = crate::gates::root();
    let directory = if let Some(path) = std::env::var_os("CRAFT_FONTS_DIR") {
        // A caller-supplied checkout is never changed or silently replaced.
        PathBuf::from(path)
    } else {
        let target = std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| root.join("target"));
        let directory = target.join(format!("web-craft-fonts-{COMMIT}"));
        if !directory.exists() {
            std::fs::create_dir_all(&directory)?;
            git(&directory, &["init", "--quiet"])?;
            git(&directory, &["fetch", "--quiet", "--depth=1", REPOSITORY, COMMIT])?;
            git(&directory, &["-c", "core.autocrlf=false", "checkout", "--quiet", "--detach", "FETCH_HEAD"])?;
        }
        directory
    };
    verify(&directory)?;
    // stdout is only the path, consumed by packaging/web/package.sh.
    println!("{}", package_path(&directory.canonicalize()?)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn canonical_windows_paths_are_usable_by_packaging_bash() {
        assert_eq!(package_path(Path::new(r"\\?\C:\Font cache\checkout")).unwrap(), "C:/Font cache/checkout");
        assert_eq!(package_path(Path::new(r"\\?\UNC\server\share\checkout")).unwrap(), "//server/share/checkout");
        assert_eq!(package_path(Path::new(r"C:\Font cache\checkout")).unwrap(), "C:/Font cache/checkout");
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_packaging_paths_keep_spaces_and_literal_backslashes() {
        assert_eq!(package_path(Path::new("/Font cache/check\\out")).unwrap(), "/Font cache/check\\out");
    }

    #[test]
    fn the_web_font_pin_matches_attribution_and_release() {
        let root = crate::gates::root();
        let attribution = std::fs::read_to_string(root.join("ATTRIBUTION.toml")).unwrap();
        let release = std::fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap();
        assert!(attribution.contains(&format!("{REPOSITORY}/tree/{COMMIT}")));
        assert!(release.contains(&format!("craft-fonts-ref: {COMMIT}")));
    }

    #[test]
    fn changed_missing_or_extra_font_content_is_rejected() {
        assert!(verify_digest(MANIFEST_SHA256, FILES).is_ok());
        assert!(verify_digest(&"0".repeat(64), FILES).is_err());
        assert!(verify_digest(MANIFEST_SHA256, FILES - 1).is_err());
        assert!(verify_digest(MANIFEST_SHA256, FILES + 1).is_err());
    }

    #[test]
    fn web_packaging_prepares_fonts_before_build_and_keeps_notices() {
        let package = std::fs::read_to_string(crate::gates::root().join("packaging/web/package.sh")).unwrap();
        assert!(package.find("cargo xtask web-fonts").unwrap() < package.find("trunk build").unwrap());
        assert!(package.contains("export CRAFT_FONTS_DIR CRAFT_FONTS_REQUIRED=1"));
        assert!(package.contains("copy_docs \"$WORK/$NAME\""));
    }
}
