//! Visual and logical order for Indic scripts in Edit text (Devanagari, Bengali, Gurmukhi, Gujarati, Oriya, Tamil, Malayalam).
//!
//! Legacy PDF producers often output glyphs in *visual* display order when using fonts without
//! OpenType GSUB layout tables. In visual order, pre-base vowel signs (such as Devanagari short 'i'
//! `ि` U+093F) appear in the stream *before* the consonant cluster they modify, matching their visual
//! placement to the left of the glyph.
//!
//! Standard Unicode text is *logical* (typing order): the pre-base vowel sign follows the consonant
//! cluster. The editor presents and edits text in logical order. When a source font is flagged with
//! `visual_order`, [`to_logical`] converts extracted stream text to logical order for editing, and
//! [`to_visual`] converts logical edits back into the visual glyph sequence the font expects.

/// Whether `c` is an Indic consonant across covered scripts.
pub fn is_indic_consonant(c: char) -> bool {
    matches!(
        c,
        '\u{0915}'..='\u{0939}' | '\u{0958}'..='\u{095F}' | '\u{0978}'..='\u{097F}' // Devanagari
        | '\u{0995}'..='\u{09B9}' | '\u{09DC}'..='\u{09DF}' // Bengali
        | '\u{0A15}'..='\u{0A39}' | '\u{0A59}'..='\u{0A5E}' // Gurmukhi
        | '\u{0A95}'..='\u{0AB9}' // Gujarati
        | '\u{0B15}'..='\u{0B39}' | '\u{0B5C}'..='\u{0B5F}' // Oriya
        | '\u{0B95}'..='\u{0BB9}' // Tamil
        | '\u{0C15}'..='\u{0C39}' | '\u{0C58}'..='\u{0C5A}' // Telugu
        | '\u{0C95}'..='\u{0CB9}' | '\u{0CDE}' // Kannada
        | '\u{0D15}'..='\u{0D3A}' // Malayalam
    )
}

/// Whether `c` is an Indic nukta (diacritic dot modifying a consonant).
pub fn is_indic_nukta(c: char) -> bool {
    matches!(
        c,
        '\u{093C}' // Devanagari
        | '\u{09BC}' // Bengali
        | '\u{0A3C}' // Gurmukhi
        | '\u{0ABC}' // Gujarati
        | '\u{0B3C}' // Oriya
    )
}

/// Whether `c` is an Indic virama (halant / vowel killer).
pub fn is_indic_virama(c: char) -> bool {
    matches!(
        c,
        '\u{094D}' // Devanagari
        | '\u{09CD}' // Bengali
        | '\u{0A4D}' // Gurmukhi
        | '\u{0ACD}' // Gujarati
        | '\u{0B4D}' // Oriya
        | '\u{0BCD}' // Tamil
        | '\u{0C4D}' // Telugu
        | '\u{0CCD}' // Kannada
        | '\u{0D4D}' // Malayalam
    )
}

/// Whether `c` is an Indic pre-base vowel sign that appears visually to the left of its consonant.
pub fn is_indic_pre_base_vowel(c: char) -> bool {
    matches!(
        c,
        '\u{093F}' // Devanagari short I
        | '\u{09BF}' | '\u{09C7}' | '\u{09C8}' // Bengali short I, E, AI
        | '\u{0A3F}' // Gurmukhi short I
        | '\u{0ABF}' // Gujarati short I
        | '\u{0B3F}' // Oriya short I
        | '\u{0BC6}' | '\u{0BC7}' | '\u{0BC8}' // Tamil E, EE, AI
        | '\u{0D46}' | '\u{0D47}' | '\u{0D48}' // Malayalam E, EE, AI
    )
}

/// Modifiers that attach to an Indic vowel sign (such as candrabindu, anusvara, or visarga).
fn is_indic_vowel_modifier(c: char) -> bool {
    matches!(
        c,
        '\u{0901}'..='\u{0903}' // Devanagari candrabindu, anusvara, visarga
        | '\u{0981}'..='\u{0983}' // Bengali
        | '\u{0A01}'..='\u{0A03}' // Gurmukhi
        | '\u{0A81}'..='\u{0A83}' // Gujarati
        | '\u{0B01}'..='\u{0B03}' // Oriya
    )
}

/// Convert Indic text from visual glyph order (pre-base vowels before their consonant cluster)
/// to standard Unicode logical order.
pub fn to_logical(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if is_indic_pre_base_vowel(ch) && i + 1 < chars.len() {
            let mut v_end = i + 1;
            while v_end < chars.len() && is_indic_vowel_modifier(chars[v_end]) {
                v_end += 1;
            }
            if v_end < chars.len() && is_indic_consonant(chars[v_end]) {
                let mut c_end = v_end;
                while c_end < chars.len() {
                    if is_indic_consonant(chars[c_end]) {
                        c_end += 1;
                        if c_end < chars.len() && is_indic_nukta(chars[c_end]) {
                            c_end += 1;
                        }
                        if c_end < chars.len() && is_indic_virama(chars[c_end]) {
                            c_end += 1;
                            if c_end < chars.len() && matches!(chars[c_end], '\u{200C}' | '\u{200D}') {
                                c_end += 1;
                            }
                            if c_end < chars.len() && is_indic_consonant(chars[c_end]) {
                                continue;
                            }
                        }
                        break;
                    } else {
                        break;
                    }
                }
                for &c in &chars[v_end..c_end] {
                    out.push(c);
                }
                for &v in &chars[i..v_end] {
                    out.push(v);
                }
                i = c_end;
                continue;
            }
        }
        out.push(ch);
        i += 1;
    }
    out.into_iter().collect()
}

/// Convert Indic text from standard Unicode logical order to visual display order for fonts
/// that expect pre-base vowels before their consonant cluster.
pub fn to_visual(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if is_indic_consonant(chars[i]) {
            let cluster_start = i;
            let mut c_end = i;
            while c_end < chars.len() {
                if is_indic_consonant(chars[c_end]) {
                    c_end += 1;
                    if c_end < chars.len() && is_indic_nukta(chars[c_end]) {
                        c_end += 1;
                    }
                    if c_end < chars.len() && is_indic_virama(chars[c_end]) {
                        c_end += 1;
                        if c_end < chars.len() && matches!(chars[c_end], '\u{200C}' | '\u{200D}') {
                            c_end += 1;
                        }
                        if c_end < chars.len() && is_indic_consonant(chars[c_end]) {
                            continue;
                        }
                    }
                    break;
                } else {
                    break;
                }
            }
            if c_end < chars.len() && is_indic_pre_base_vowel(chars[c_end]) {
                let v_start = c_end;
                let mut v_end = v_start + 1;
                while v_end < chars.len() && is_indic_vowel_modifier(chars[v_end]) {
                    v_end += 1;
                }
                for &v in &chars[v_start..v_end] {
                    out.push(v);
                }
                for &c in &chars[cluster_start..c_end] {
                    out.push(c);
                }
                i = v_end;
                continue;
            } else {
                for &c in &chars[cluster_start..c_end] {
                    out.push(c);
                }
                i = c_end;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_words_roundtrip() {
        // किताब (kitab): k + i + t + aa + b
        let visual = "\u{093F}\u{0915}\u{0924}\u{093E}\u{092C}";
        let logical = "\u{0915}\u{093F}\u{0924}\u{093E}\u{092C}";
        assert_eq!(to_logical(visual), logical);
        assert_eq!(to_visual(logical), visual);

        // दिन (din): d + i + n
        let visual_din = "\u{093F}\u{0926}\u{0928}";
        let logical_din = "\u{0926}\u{093F}\u{0928}";
        assert_eq!(to_logical(visual_din), logical_din);
        assert_eq!(to_visual(logical_din), visual_din);
    }

    #[test]
    fn decomposed_nukta_with_pre_base_vowel() {
        // Decomposed ड़ि: DDA (U+0921) + NUKTA (U+093C) + Short I (U+093F)
        let logical = "\u{0921}\u{093C}\u{093F}";
        let visual = "\u{093F}\u{0921}\u{093C}";
        assert_eq!(to_logical(visual), logical);
        assert_eq!(to_visual(logical), visual);

        // Decomposed ढ़ि: DDHA (U+0922) + NUKTA (U+093C) + Short I (U+093F)
        let logical_ddhi = "\u{0922}\u{093C}\u{093F}";
        let visual_ddhi = "\u{093F}\u{0922}\u{093C}";
        assert_eq!(to_logical(visual_ddhi), logical_ddhi);
        assert_eq!(to_visual(logical_ddhi), visual_ddhi);
    }

    #[test]
    fn precomposed_nukta_with_pre_base_vowel() {
        // Precomposed ड़ि (U+095C + U+093F)
        let logical = "\u{095C}\u{093F}";
        let visual = "\u{093F}\u{095C}";
        assert_eq!(to_logical(visual), logical);
        assert_eq!(to_visual(logical), visual);
    }

    #[test]
    fn full_words_with_nukta_and_pre_base_vowel() {
        // बढ़िया (badhiya): ba + ddha + nukta + short_i + ya + aa
        let logical = "\u{092C}\u{0922}\u{093C}\u{093F}\u{092F}\u{093E}";
        let visual = "\u{092C}\u{093F}\u{0922}\u{093C}\u{092F}\u{093E}";
        assert_eq!(to_logical(visual), logical);
        assert_eq!(to_visual(logical), visual);

        // खिड़की (khirki): kha + short_i + dda + nukta + ka + long_ii
        let logical_khirki = "\u{0916}\u{093F}\u{0921}\u{093C}\u{0915}\u{0940}";
        let visual_khirki = "\u{093F}\u{0916}\u{0921}\u{093C}\u{0915}\u{0940}";
        assert_eq!(to_logical(visual_khirki), logical_khirki);
        assert_eq!(to_visual(logical_khirki), visual_khirki);
    }

    #[test]
    fn complex_conjuncts_with_pre_base_vowel() {
        // स्थिति (sthiti): sa + virama + tha + short_i + ta + short_i
        let logical = "\u{0938}\u{094D}\u{0925}\u{093F}\u{0924}\u{093F}";
        let visual = "\u{093F}\u{0938}\u{094D}\u{0925}\u{093F}\u{0924}";
        assert_eq!(to_logical(visual), logical);
        assert_eq!(to_visual(logical), visual);
    }

    #[test]
    fn other_indic_scripts_pre_base_vowels() {
        // Bengali কি (ki): KA (U+0995) + I (U+09BF)
        let b_logical = "\u{0995}\u{09BF}";
        let b_visual = "\u{09BF}\u{0995}";
        assert_eq!(to_logical(b_visual), b_logical);
        assert_eq!(to_visual(b_logical), b_visual);

        // Gurmukhi ਕਿ (ki): KA (U+0A15) + I (U+0A3F)
        let g_logical = "\u{0A15}\u{0A3F}";
        let g_visual = "\u{0A3F}\u{0A15}";
        assert_eq!(to_logical(g_visual), g_logical);
        assert_eq!(to_visual(g_logical), g_visual);
    }

    #[test]
    fn non_indic_text_is_untouched() {
        for s in ["", "Hello world!", "PDF 123", "日本語テキスト", "123.45"] {
            assert_eq!(to_logical(s), s);
            assert_eq!(to_visual(s), s);
        }
    }
}
