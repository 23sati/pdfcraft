//! Direction for interface text that may hold right-to-left script (file names, document titles,
//! the Arabic and Hebrew catalogs).
//!
//! The text layout puts right-to-left text in display order itself (the vendored epaint's bidi
//! patch, `vendor/epaint/src/text/bidi.rs`): a paragraph takes the direction of its first strong
//! character, and wrapped lines and text fields are laid out right to left. Strings therefore
//! stay in logical (typing) order everywhere; this module only chooses a paragraph's direction
//! where the first-strong rule would pick the wrong one, with an invisible direction mark.

use std::borrow::Cow;

/// LEFT-TO-RIGHT MARK: a zero-width strong left-to-right character.
pub const LRM: char = '\u{200E}';
/// RIGHT-TO-LEFT MARK: a zero-width strong right-to-left character.
pub const RLM: char = '\u{200F}';

/// `text` to show as a left-to-right paragraph (file names and paths): a Hebrew or Arabic name
/// keeps its extension at the end, the way the window title shows it. Text without
/// right-to-left characters comes back borrowed and unchanged. Use it for painting only;
/// accessibility labels and anything stored keep the plain text.
pub fn visual(text: &str) -> Cow<'_, str> {
    if !text.chars().any(is_rtl_script) || text.starts_with(LRM) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(format!("{LRM}{text}"))
}

/// A translation of a right-to-left interface language, made a right-to-left paragraph even
/// when it starts with a placeholder, a product name or a number (`{name} נשמר`).
pub fn rtl_label(text: &str) -> String {
    if text.starts_with(RLM) { text.to_owned() } else { format!("{RLM}{text}") }
}

/// Characters of the right-to-left scripts' blocks: letters, marks and the scripts' own
/// punctuation.
fn is_rtl_script(c: char) -> bool {
    matches!(u32::from(c), 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFC | 0x1_0800..=0x1_0FFF | 0x1_E800..=0x1_EFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_without_rtl_is_borrowed_unchanged() {
        for s in ["", "report.pdf", "日本語の文字.pdf", "a (b) [c]", "caf\u{00E9} \u{200E}x"] {
            assert!(matches!(visual(s), Cow::Borrowed(v) if v == s), "{s:?}");
        }
    }

    #[test]
    fn rtl_names_become_left_to_right_paragraphs_in_logical_order() {
        assert_eq!(visual("واحد اثنين.pdf"), "\u{200E}واحد اثنين.pdf");
        assert_eq!(visual("שלום עולם.pdf"), "\u{200E}שלום עולם.pdf");
        // Already marked: not marked twice.
        assert_eq!(visual("\u{200E}שלום.pdf"), "\u{200E}שלום.pdf");
    }

    #[test]
    fn rtl_labels_start_with_one_mark() {
        assert_eq!(rtl_label("{name} נשמר"), "\u{200F}{name} נשמר");
        assert_eq!(rtl_label("\u{200F}שמירה"), "\u{200F}שמירה");
        assert_eq!(rtl_label(""), "\u{200F}");
    }

    #[test]
    fn odd_input_never_panics() {
        let long = "ا ب ".repeat(20_000);
        for s in ["\u{064B}", "\u{202E}\u{202E}ا", "ا\u{10FFFF}\u{E000}ب", "((((ا]]]]", long.as_str()] {
            assert_eq!(visual(s).chars().count(), s.chars().count() + 1);
        }
    }
}
