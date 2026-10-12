//! Direction for interface text that may hold right-to-left script (file names, document titles,
//! comments, the Arabic and Hebrew catalogs).
//!
//! The text layout puts right-to-left text in display order itself (the vendored epaint's bidi
//! patch, `vendor/epaint/src/text/bidi.rs`): a paragraph takes the direction of its first strong
//! character, and wrapped lines and text fields are laid out right to left. Strings therefore
//! stay in logical (typing) order everywhere; this module only chooses a paragraph's direction
//! where the first-strong rule would pick the wrong one, with an invisible direction mark, and
//! breaks multi-line text into lines that keep their paragraph's direction ([`wrap`]).

use std::borrow::Cow;
use std::ops::Range;

use unicode_bidi::BidiInfo;

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

/// One line of `text` with its direction from its first strong character (an author's name).
/// The layout already reads a line that way, so the text comes back borrowed and unchanged; the
/// function marks the places that rely on it.
pub fn visual_auto(text: &str) -> Cow<'_, str> {
    Cow::Borrowed(text)
}

/// A translation of a right-to-left interface language, made a right-to-left paragraph even
/// when it starts with a placeholder, a product name or a number (`{name} נשמר`).
pub fn rtl_label(text: &str) -> String {
    if text.starts_with(RLM) { text.to_owned() } else { format!("{RLM}{text}") }
}

/// One line of wrapped text (see [`wrap`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// The line in logical order, starting with a direction mark when it holds right-to-left
    /// script, so that it keeps its paragraph's direction when laid out on its own.
    pub text: String,
    /// The paragraph reads right to left: the line belongs against the right edge.
    pub rtl: bool,
}

/// Whether `text` holds any right-to-left script, so [`wrap`] is needed.
pub fn has_rtl(text: &str) -> bool {
    text.chars().any(is_rtl_script)
}

/// Multi-line text with right-to-left script (a comment in Persian, Arabic, Hebrew), broken
/// into lines that `fits` accepts (it measures a line as it will be laid out).
///
/// Every paragraph takes its direction from its first strong character, as `dir="auto"` does,
/// so a Persian comment reads right to left with its Latin words and numbers left to right
/// inside it. Lines break between words, in reading order, and a word wider than a line
/// between characters. Each line starts with a direction mark for its paragraph, so a line of a
/// Persian paragraph that starts with a Latin word still reads right to left. Use it for
/// painting; copying and accessibility keep the plain text.
pub fn wrap(text: &str, mut fits: impl FnMut(&str) -> bool) -> Vec<Line> {
    let info = BidiInfo::new(text, None);
    let mut lines = Vec::new();
    for para in &info.paragraphs {
        let whole = text.get(para.range.clone()).unwrap_or_default();
        // unicode-bidi ends a paragraph at the `\r` of `\r\n`; the `\n` is no new paragraph.
        if whole == "\n" && text.get(..para.range.start).is_some_and(|before| before.ends_with('\r')) {
            continue;
        }
        let content = para.range.start..para.range.start.saturating_add(whole.trim_end_matches(is_paragraph_separator).len());
        let rtl = para.level.is_rtl();
        let mark = if !has_rtl(whole) {
            ""
        } else if rtl {
            "\u{200F}"
        } else {
            "\u{200E}"
        };
        let mut measured = |s: &str| fits(&format!("{mark}{s}"));
        for line in break_lines(text, content, &mut measured) {
            let piece = text.get(line).unwrap_or_default();
            let text = if piece.is_empty() { String::new() } else { format!("{mark}{piece}") };
            lines.push(Line { text, rtl });
        }
    }
    lines
}

/// The byte ranges of the lines `range` of `text` breaks into: as many words as `fits` accepts
/// per line, a word too wide for any line split between characters. Always at least one line.
fn break_lines(text: &str, range: Range<usize>, fits: &mut impl FnMut(&str) -> bool) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut current: Option<Range<usize>> = None;
    for word in words(text, range.clone()) {
        if let Some(line) = current.as_mut() {
            if fits(text.get(line.start..word.end).unwrap_or_default()) {
                line.end = word.end;
                continue;
            }
            lines.push(line.clone());
        }
        // The word starts a line.
        let mut start = word.start;
        loop {
            let end = fitting_prefix(text, start..word.end, fits);
            if end >= word.end {
                current = Some(start..word.end);
                break;
            }
            lines.push(start..end);
            start = end;
        }
    }
    lines.push(current.unwrap_or(range.start..range.start));
    lines
}

/// The end of the longest start of `range` that `fits`, at least one character.
fn fitting_prefix(text: &str, range: Range<usize>, fits: &mut impl FnMut(&str) -> bool) -> usize {
    let piece = text.get(range.clone()).unwrap_or_default();
    let mut end = range.end;
    for (i, (at, c)) in piece.char_indices().enumerate() {
        let next = range.start.saturating_add(at).saturating_add(c.len_utf8());
        if i > 0 && !fits(text.get(range.start..next).unwrap_or_default()) {
            end = range.start.saturating_add(at);
            break;
        }
    }
    end
}

/// The byte ranges of the whitespace-separated words of `range` in `text`.
fn words(text: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let mut words = Vec::new();
    let mut start = None;
    for (at, c) in text.get(range.clone()).unwrap_or_default().char_indices() {
        let at = range.start.saturating_add(at);
        match (c.is_whitespace(), start) {
            (true, Some(s)) => {
                words.push(s..at);
                start = None;
            }
            (false, None) => start = Some(at),
            _ => {}
        }
    }
    if let Some(s) = start {
        words.push(s..range.end);
    }
    words
}

/// Characters that end a paragraph (bidi class B).
fn is_paragraph_separator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{1C}'..='\u{1E}' | '\u{85}' | '\u{2029}')
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
    fn single_lines_stay_in_logical_order() {
        assert!(matches!(visual_auto("Ada Lovelace"), Cow::Borrowed("Ada Lovelace")));
        assert!(matches!(visual_auto("علی رضایی"), Cow::Borrowed("علی رضایی")));
    }

    /// `wrap` with a width of `n` characters, direction marks not counted.
    fn wrap_at(text: &str, n: usize) -> Vec<Line> {
        wrap(text, |s| s.chars().filter(|c| !matches!(c, '\u{200E}' | '\u{200F}')).count() <= n)
    }

    fn rtl(text: &str) -> Line {
        Line { text: format!("\u{200F}{text}"), rtl: true }
    }

    fn ltr(text: &str) -> Line {
        Line { text: text.to_owned(), rtl: false }
    }

    #[test]
    fn rtl_lines_break_in_reading_order_and_keep_their_direction() {
        // The first words go on the first line, in logical order; the layout reorders them.
        assert_eq!(wrap_at("واحد اثنين ثلاثة أربعة", 11), [rtl("واحد اثنين"), rtl("ثلاثة أربعة")]);
        assert_eq!(wrap_at("واحد اثنين ثلاثة أربعة", 80), [rtl("واحد اثنين ثلاثة أربعة")]);
        // A line of a right-to-left paragraph that starts with a Latin word still reads right to left.
        assert_eq!(wrap_at("سلام Hello World", 6), [rtl("سلام"), rtl("Hello"), rtl("World")]);
    }

    #[test]
    fn base_direction_comes_from_the_first_strong_character() {
        // Latin first: a left-to-right paragraph with an Arabic phrase inside it.
        assert_eq!(wrap_at("See واحد اثنين here", 80), [Line { text: "\u{200E}See واحد اثنين here".into(), rtl: false }]);
        // Leading neutrals and numbers don't decide it.
        assert_eq!(wrap_at("12. واحد اثنين", 80), [rtl("12. واحد اثنين")]);
        // Each paragraph finds its own direction.
        assert_eq!(wrap_at("واحد اثنين\nOK then\n\nשלום עולם", 80), [rtl("واحد اثنين"), ltr("OK then"), ltr(""), rtl("שלום עולם")]);
    }

    #[test]
    fn wrapping_handles_odd_widths_and_input() {
        assert_eq!(wrap_at("", 10), Vec::<Line>::new());
        // A word wider than the line is broken between characters.
        assert_eq!(wrap_at("abcdefghij", 4), [ltr("abcd"), ltr("efgh"), ltr("ij")]);
        // Extra spaces between words don't make empty lines; \r\n is one break.
        assert_eq!(wrap_at("واحد    اثنين\r\nب", 5), [rtl("واحد"), rtl("اثنين"), rtl("ب")]);
        // Nothing fits: one character per line, never an endless loop.
        assert_eq!(wrap("واحد", |_| false).len(), 4);
        let long = "ا ب ۱ (ج) ".repeat(5_000);
        for s in ["\u{202E}ا\u{202C}b", "\u{064B}", "\n\n", "\u{2029}ا", long.as_str()] {
            let lines = wrap_at(s, 7);
            let shown: usize = lines.iter().map(|l| l.text.chars().filter(|c| !matches!(c, '\u{200E}' | '\u{200F}')).count()).sum();
            assert!(shown <= s.chars().count(), "{:?}", s.chars().take(12).collect::<String>());
        }
    }

    #[test]
    fn odd_input_never_panics() {
        let long = "ا ب ".repeat(20_000);
        for s in ["\u{064B}", "\u{202E}\u{202E}ا", "ا\u{10FFFF}\u{E000}ب", "((((ا]]]]", long.as_str()] {
            assert_eq!(visual(s).chars().count(), s.chars().count() + 1);
        }
    }
}
