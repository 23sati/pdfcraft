//! PdfCraft patch: bidirectional text (Hebrew, Arabic) in the text layout.
//!
//! Upstream epaint shapes each font-face run in the script's own direction but lays the runs out
//! left to right in logical order, so the words of a right-to-left sentence come out reversed and
//! a wrapped right-to-left paragraph breaks at the wrong end. With this patch:
//!
//! 1. Each paragraph gets Unicode bidi embedding levels (UAX #9). Its base direction is that of
//!    its first strong character (rules P2–P3), so a right-to-left label or a typed Hebrew line
//!    is right to left, and a direction mark (U+200E / U+200F) at its start can force either one.
//! 2. Runs are split where the level changes and shaped in their own direction, but their glyphs
//!    are stored in logical order with increasing x. Line breaking therefore works unchanged, and
//!    glyph `i` of a row is still character `i` (what cursors and selections rely on).
//! 3. Each finished row is reordered for display (rule L2): its clusters keep their shapes and
//!    are moved to their visual positions.
//!
//! Text without right-to-left characters takes none of these steps.

use unicode_bidi::{BidiClass, BidiInfo, bidi_class};

use super::Row;

/// Direction marks, embeddings, overrides and isolates: they steer the reordering and are drawn
/// as nothing.
pub(crate) fn is_bidi_control(c: char) -> bool {
    matches!(
        c,
        '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

/// Whether `text` needs the bidi algorithm at all: it has a right-to-left character, embedding,
/// override or isolate.
fn needs_bidi(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            bidi_class(c),
            BidiClass::R | BidiClass::AL | BidiClass::RLE | BidiClass::RLO | BidiClass::RLI
        )
    })
}

/// The resolved embedding level of every byte of `text` (paragraphs split at `\n`, base direction
/// from the first strong character), or `None` when the text is purely left to right.
pub(crate) fn levels(text: &str) -> Option<Vec<u8>> {
    if !needs_bidi(text) {
        return None;
    }
    let info = BidiInfo::new(text, None);
    Some(info.levels.iter().map(|level| level.number()).collect())
}

/// Puts the glyphs of a laid-out row (logical order, x increasing) at their display positions.
///
/// A cluster is a run of glyphs starting at one with `cluster_start`: a base with its marks, or
/// a ligature with its zero-width continuation glyphs. Clusters move as a whole, so their inner
/// positioning (marks, kerning) is kept. `round` snaps a shift to the pixel grid.
pub(crate) fn reorder_row(row: &mut Row, round: impl Fn(f32) -> f32) {
    if row.glyphs.iter().all(|g| g.bidi_level == 0) {
        return;
    }
    // Clusters as (first glyph, end, level, width).
    let mut clusters: Vec<(usize, usize, u8, f32)> = Vec::new();
    for (i, glyph) in row.glyphs.iter().enumerate() {
        match clusters.last_mut() {
            Some((_, end, _, width)) if !glyph.cluster_start => {
                *end = i + 1;
                *width += glyph.advance_width;
            }
            _ => clusters.push((i, i + 1, glyph.bidi_level, glyph.advance_width)),
        }
    }
    // Rule L1 (in part): trailing whitespace takes the paragraph level, which is the lowest
    // level on the row when it starts with its paragraph; using the row's lowest level keeps
    // the spaces at the end that is the line's end.
    let lowest = clusters.iter().map(|c| c.2).min().unwrap_or(0);
    for cluster in clusters.iter_mut().rev() {
        let blank = row
            .glyphs
            .get(cluster.0..cluster.1)
            .is_some_and(|glyphs| glyphs.iter().all(|g| g.chr.is_whitespace()));
        if !blank {
            break;
        }
        cluster.2 = lowest;
    }
    // Rule L2: from the highest level down to the lowest odd one, reverse every maximal stretch
    // at that level or above.
    let mut order: Vec<usize> = (0..clusters.len()).collect();
    let highest = clusters.iter().map(|c| c.2).max().unwrap_or(0);
    let lowest_odd = (lowest | 1).max(1);
    let mut level = highest;
    while level >= lowest_odd {
        let mut i = 0;
        while i < order.len() {
            if clusters[order[i]].2 >= level {
                let start = i;
                while i < order.len() && clusters[order[i]].2 >= level {
                    i += 1;
                }
                order[start..i].reverse();
            } else {
                i += 1;
            }
        }
        level -= 1;
    }
    // Logical start of each cluster relative to the row's first cluster, then its visual start.
    let mut logical = Vec::with_capacity(clusters.len());
    let mut x = 0.0;
    for cluster in &clusters {
        logical.push(x);
        x += cluster.3;
    }
    let mut visual_x = 0.0;
    for &index in &order {
        let (start, end, _, width) = clusters[index];
        let shift = round(visual_x - logical[index]);
        if shift != 0.0 {
            for glyph in row.glyphs.get_mut(start..end).into_iter().flatten() {
                glyph.pos.x += shift;
            }
        }
        visual_x += width;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_left_to_right_text_needs_nothing() {
        assert_eq!(levels("Save as… report.pdf (1)"), None);
        assert_eq!(levels(""), None);
    }

    #[test]
    fn hebrew_paragraphs_are_right_to_left() {
        let text = "שלום world";
        let resolved = levels(text).unwrap();
        assert_eq!(resolved.len(), text.len());
        assert_eq!(resolved[0], 1, "Hebrew at the paragraph level");
        assert_eq!(*resolved.last().unwrap(), 2, "Latin embedded in a right-to-left paragraph");
        // A left-to-right mark first makes the paragraph left to right.
        let marked = levels("\u{200E}שלום world").unwrap();
        assert_eq!(*marked.last().unwrap(), 0);
    }

    #[test]
    fn controls_are_recognised() {
        assert!(is_bidi_control('\u{200F}') && is_bidi_control('\u{2067}'));
        assert!(!is_bidi_control('א') && !is_bidi_control(' '));
    }
}
