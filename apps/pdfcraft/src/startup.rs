//! Native launch arguments and per-file view overrides (#1354).
use pdfcraft_ui_egui::PdfCraftApp;

#[derive(Default)]
pub(super) struct Arguments {
    pub files: Vec<String>,
    pub options: Vec<(String, String)>,
    pub control_file: Option<String>,
    pub create_images: bool,
    pub new_window: bool,
    pub two_paged: bool,
    pub version: bool,
}

impl Arguments {
    pub fn may_hand_off(&self) -> bool {
        !self.new_window && !self.files.is_empty() && self.options.is_empty() && self.control_file.is_none() && !self.create_images && !self.two_paged
    }

    pub fn parse(args: impl IntoIterator<Item = String>) -> Self {
        let mut parsed = Self::default();
        let mut args = args.into_iter();
        while let Some(a) = args.next() {
            match a.as_str() {
                "--version" => {
                    parsed.version = true;
                    break;
                }
                "--control" => parsed.control_file = args.next(),
                "--create-images" => parsed.create_images = true,
                "--new-window" => parsed.new_window = true,
                "--two-paged" => parsed.two_paged = true,
                flag if flag.starts_with("--two-paged=") => {
                    // It has no value: do not consume the following filename on malformed use.
                    log::warn!("{flag}: use --two-paged without a value");
                }
                flag if flag.starts_with("--") => {
                    let value = args.next().unwrap_or_default();
                    parsed.options.push((flag.trim_start_matches("--").to_string(), value));
                }
                _ => parsed.files.push(super::path_from_arg(a)),
            }
        }
        parsed
    }
}

pub(super) fn open_files(app: &mut PdfCraftApp, files: &[String], two_paged: bool) {
    for path in files {
        if two_paged {
            // The per-file override wins over initial view without changing saved defaults.
            app.open_path_with_display(path, pdfcraft_ui_egui::canvas::PageLayout::TwoUp, false);
        } else {
            app.open_path(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Arguments {
        Arguments::parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn standalone_flag_keeps_files_in_either_position() {
        for args in
            [["--two-paged", "first.pdf", "second.pdf"], ["first.pdf", "--two-paged", "second.pdf"], ["first.pdf", "second.pdf", "--two-paged"]]
        {
            let parsed = parse(&args);
            assert!(parsed.two_paged);
            assert_eq!(parsed.files, ["first.pdf", "second.pdf"]);
            assert!(parsed.options.is_empty());
            assert!(!parsed.may_hand_off());
        }
        let empty = parse(&["--two-paged"]);
        assert!(empty.two_paged && empty.files.is_empty());
        let normal = parse(&["first.pdf", "second.pdf"]);
        assert!(!normal.two_paged);
        assert!(normal.may_hand_off());
        for args in [
            &["--new-window", "first.pdf"][..],
            &["--layout", "two-page", "first.pdf"],
            &["--control", "control.json", "first.pdf"],
            &["--create-images", "first.png"],
        ] {
            assert!(!parse(args).may_hand_off());
        }
    }

    #[test]
    fn existing_layout_options_are_preserved_in_either_order() {
        for args in [["--two-paged", "--layout", "single", "file.pdf"], ["--layout", "single", "--two-paged", "file.pdf"]] {
            let parsed = parse(&args);
            assert!(parsed.two_paged);
            assert_eq!(parsed.files, ["file.pdf"]);
            assert_eq!(parsed.options, [("layout".into(), "single".into())]);
        }
        let parsed = parse(&["--two-paged", "--default-layout", "two-up", "file.pdf"]);
        assert_eq!(parsed.options, [("default-layout".into(), "two-up".into())]);
    }

    #[test]
    fn malformed_boolean_does_not_eat_a_file_or_option() {
        let args = parse(&["--two-paged=false", "first.pdf", "--layout", "single", "second.pdf"]);
        assert!(!args.two_paged);
        assert_eq!(args.files, ["first.pdf", "second.pdf"]);
        assert_eq!(args.options, [("layout".into(), "single".into())]);
    }

    #[test]
    fn startup_override_is_per_file_and_does_not_persist() {
        use std::io::Write as _;
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("pdfcraft-two-paged-{}-{nonce}.pdf", std::process::id()));
        let mut file = std::fs::OpenOptions::new().create_new(true).write(true).open(&path).unwrap();
        file.write_all(include_bytes!("../../../crates/ui-egui/tests/data/form.pdf")).unwrap();
        drop(file);
        let text = path.to_str().unwrap();
        for enabled in [false, true] {
            let mut app = PdfCraftApp::new();
            app.set_option("default-layout", "single").unwrap();
            app.open_bytes("restored.pdf", None, include_bytes!("../../../crates/ui-egui/tests/data/form.pdf").to_vec()).unwrap();
            let restored = app.views[0].id;
            open_files(&mut app, &[text.into(), text.into()], enabled);
            assert_eq!(app.views.len(), 3);
            assert_eq!(app.views[0].id, restored);
            assert_eq!(app.views[0].layout, pdfcraft_ui_egui::canvas::PageLayout::Single);
            for view in app.views.iter().skip(1) {
                assert_eq!(
                    view.layout,
                    if enabled { pdfcraft_ui_egui::canvas::PageLayout::TwoUp } else { pdfcraft_ui_egui::canvas::PageLayout::Single }
                );
                assert!(!view.continuous());
            }
            let persisted: serde_json::Value = serde_json::from_str(&app.persist()).unwrap();
            assert_eq!(persisted["default_layout"], "single");
            assert!(persisted.get("two_paged").is_none());
            // Existing --layout applies after the flag to the active document only.
            app.set_option("layout", "continuous").unwrap();
            assert_eq!(app.views[2].layout, pdfcraft_ui_egui::canvas::PageLayout::Single);
            assert!(app.views[2].continuous());
            app.set_option("default-layout", "two-up").unwrap();
            assert_eq!(app.view_defaults.layout, pdfcraft_ui_egui::canvas::PageLayout::TwoUp);
            assert!(app.view_defaults.continuous);
            open_files(&mut app, &[], enabled);
            assert_eq!(app.views.len(), 3);
        }
        std::fs::remove_file(path).unwrap();
    }
}
