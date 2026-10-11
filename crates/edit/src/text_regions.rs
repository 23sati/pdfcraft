//! Geometry-based regions and mixed-style rewriting of existing text.

use super::*;

fn same_plane(a: &TextLine, b: &TextLine) -> bool {
    a.stream == b.stream
        && a.origin.ctm == b.origin.ctm
        && [0, 2].iter().all(|axis| {
            let ax = a.origin.tm.get(*axis).copied().unwrap_or(0.0);
            let ay = a.origin.tm.get(axis + 1).copied().unwrap_or(0.0);
            let bx = b.origin.tm.get(*axis).copied().unwrap_or(0.0);
            let by = b.origin.tm.get(axis + 1).copied().unwrap_or(0.0);
            let al = ax.hypot(ay);
            let bl = bx.hypot(by);
            al > 0.0 && bl > 0.0 && (ax / al - bx / bl).abs() < 0.001 && (ay / al - by / bl).abs() < 0.001
        })
        && (a.origin.state.rise - b.origin.state.rise).abs() < 0.01
}

fn union(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
}

fn row_rect(row: &[usize], lines: &[TextLine]) -> Option<[f64; 4]> {
    row.iter().filter_map(|i| lines.get(*i)).map(|l| l.rect).reduce(union)
}

pub(super) fn group_blocks(lines: &[TextLine]) -> Vec<TextBlock> {
    group_layout(lines, true)
}

/// Exporters currently represent a block with one style. Keep their style boundaries until
/// they support rich runs, while edit selection may encompass the complete visual region.
pub(super) fn group_reading_blocks(lines: &[TextLine]) -> Vec<TextBlock> {
    group_layout(lines, false)
}

fn same_style(a: &TextLine, b: &TextLine) -> bool {
    a.font == b.font && a.base_font == b.base_font && a.bold == b.bold && a.italic == b.italic && a.color == b.color && (a.size - b.size).abs() < 0.01
}

fn group_layout(lines: &[TextLine], mixed: bool) -> Vec<TextBlock> {
    let mut rows: Vec<Vec<usize>> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let joins = rows.last().and_then(|r| r.last()).and_then(|i| lines.get(*i)).is_some_and(|prev| {
            let gap = line.rect[0] - prev.rect[2];
            (mixed || same_style(prev, line))
                && same_plane(prev, line)
                && (prev.origin.baseline - line.origin.baseline).abs() < prev.size.min(line.size) * 0.2
                && gap >= -prev.size.min(line.size) * 0.2
                && gap < prev.size.max(line.size) * 1.5
        });
        if joins && let Some(row) = rows.last_mut() {
            row.push(i);
        } else {
            rows.push(vec![i]);
        }
    }
    let mut blocks: Vec<TextBlock> = Vec::new();
    for row in rows {
        let Some(first) = row.first().and_then(|i| lines.get(*i)) else { continue };
        let Some(rect) = row_rect(&row, lines) else { continue };
        let joins = blocks.last().is_some_and(|block| {
            let Some(prev_row) = block.rows.last() else { return false };
            let Some(prev) = prev_row.first().and_then(|i| lines.get(*i)) else { return false };
            let Some(pr) = row_rect(prev_row, lines) else { return false };
            let gap = prev.origin.baseline - first.origin.baseline;
            let tol = prev.size.max(first.size).max(1.0);
            let aligned = (pr[0] - rect[0]).abs() < tol || ((pr[0] + pr[2] - rect[0] - rect[2]) / 2.0).abs() < tol || (pr[2] - rect[2]).abs() < tol;
            (mixed || same_style(prev, first))
                && same_plane(prev, first)
                && aligned
                && first.size.min(prev.size) >= first.size.max(prev.size) * 0.4
                && gap > first.size.min(prev.size) * 0.8
                && gap < first.size.max(prev.size) * 2.5
        });
        if !joins {
            blocks.push(TextBlock {
                text: String::new(),
                editor_text: String::new(),
                spans: Vec::new(),
                rect,
                base_font: first.base_font.clone(),
                size: first.size,
                bold: first.bold,
                italic: first.italic,
                color: first.color,
                lines: Vec::new(),
                rows: Vec::new(),
            });
        }
        let Some(block) = blocks.last_mut() else { continue };
        let mut row_text = String::new();
        if !block.rows.is_empty() {
            block.editor_text.push('\n');
        }
        for &index in &row {
            let Some(line) = lines.get(index) else { continue };
            // Some producers position separate words without encoding a space.
            if let Some(prev) = block.lines.last().filter(|i| row.contains(i)).and_then(|i| lines.get(*i))
                && !row_text.ends_with(char::is_whitespace)
                && !line.text.starts_with(char::is_whitespace)
                && line.rect[0] - prev.rect[2] > line.size * 0.12
            {
                row_text.push(' ');
                block.editor_text.push(' ');
            }
            let start = block.editor_text.len();
            block.editor_text.push_str(&line.text);
            block.spans.push((start..block.editor_text.len(), index));
            row_text.push_str(&line.text);
            block.lines.push(index);
        }
        if !block.text.is_empty() {
            if block.text.ends_with('-') {
                block.text.pop();
            } else {
                block.text.push(' ');
            }
        }
        block.text.push_str(row_text.trim_matches(paragraph_separator));
        block.rect = union(block.rect, rect);
        block.rows.push(row);
    }
    blocks
}

/// Rewrite visual rows while keeping the source style on each surviving character. Resource
/// names may differ between Unicode subsets of the same face; encoding searches those subsets.
pub(super) fn rewrite_styled_block(
    doc: &mut Document,
    page: usize,
    block: &TextBlock,
    lines: &[TextLine],
    text: Option<&str>,
    style: &BlockStyle,
) -> Result<LineEdit, EditError> {
    let first = block.lines.first().and_then(|i| lines.get(*i)).ok_or_else(|| EditError::Invalid("the paragraph has no source runs".into()))?;
    let draft = match text {
        Some(t) if t != block.text => t,
        _ => &block.editor_text,
    };
    if draft.len() > 4_000_000 {
        return Err(EditError::Invalid("the replacement text is too large".into()));
    }
    if draft == block.editor_text && *style == BlockStyle::default() {
        return Ok(LineEdit { substituted: None });
    }
    let p = page_dict(doc, page)?;
    let mut res = p.dict.get(b"Resources").and_then(|r| doc.resolve(r).as_dict().cloned()).unwrap_or_default();
    let mut fonts = res.get(b"Font").and_then(|r| doc.resolve(r).as_dict().cloned()).unwrap_or_default();
    let mut metrics = std::collections::BTreeMap::new();
    for (name, object) in fonts.iter().take(256) {
        if let Some(font) = doc.resolve(object).as_dict() {
            metrics.insert(String::from_utf8_lossy(name).into_owned(), Metrics::from_dict(doc, font));
        }
    }
    for index in &block.lines {
        if let Some(line) = lines.get(*index)
            && !metrics.contains_key(&line.font)
            && let Some(font) = fonts.get(line.font.as_bytes()).and_then(|f| doc.resolve(f).as_dict().cloned())
        {
            metrics.insert(line.font.clone(), Metrics::from_dict(doc, &font));
        }
    }
    if [style.size, style.line_spacing, style.char_spacing, style.scale, style.width].iter().flatten().any(|v| !v.is_finite())
        || style.color.is_some_and(|c| c.iter().any(|v| !v.is_finite()))
    {
        return Err(EditError::Invalid("the paragraph's formatting must use finite numbers".into()));
    }
    let [dx, dy] = style.offset.unwrap_or([0.0, 0.0]);
    if !dx.is_finite() || !dy.is_finite() || style.width.is_some_and(|w| !w.is_finite()) {
        return Err(EditError::Invalid("the paragraph's position and width must be finite".into()));
    }
    let width = style.width.unwrap_or(block.rect[2] - block.rect[0]).clamp(1.0, 14_400.0);
    // Each tuple is a character, its source style and an encodable resource name.
    let mut chars = Vec::new();
    let mut substituted = None;
    let mut fallback_names: std::collections::BTreeMap<(String, char), String> = std::collections::BTreeMap::new();
    for (range, index) in block.edited_spans(draft) {
        let source = lines.get(index).unwrap_or(first);
        for ch in draft.get(range).unwrap_or_default().chars() {
            if ch == '\n' {
                chars.push((ch, index, source.font.clone()));
                continue;
            }
            let s = ch.to_string();
            let name = if metrics.get(&source.font).is_some_and(|m| source_encodes(m, &s)) {
                source.font.clone()
            } else if let Some((name, _)) =
                metrics.iter().find(|(_, m)| m.base_font.rsplit('+').next() == source.base_font.rsplit('+').next() && source_encodes(m, &s))
            {
                name.clone()
            } else {
                let family = source_family(&source.base_font);
                if needs_type3(&s) {
                    let key = (source.base_font.rsplit('+').next().unwrap_or(&source.base_font).to_owned(), ch);
                    if let Some(name) = fallback_names.get(&key) {
                        chars.push((ch, index, name.clone()));
                        continue;
                    }
                    if fallback_names.len() >= MAX_TYPE3_GLYPHS {
                        return Err(EditError::Invalid("the replacement has too many fallback glyphs".into()));
                    }
                    let fallback = type3_font(doc, &mut fonts, &s, family, source.bold)?;
                    let dict = fonts
                        .get(fallback.name.as_bytes())
                        .and_then(|f| doc.resolve(f).as_dict().cloned())
                        .ok_or_else(|| EditError::Invalid("the fallback font could not be loaded".into()))?;
                    metrics.insert(fallback.name.clone(), Metrics::from_dict(doc, &dict));
                    substituted = Some(fallback.family.to_string());
                    fallback_names.insert(key, fallback.name.clone());
                    fallback.name
                } else {
                    let base = family.base_font(source.bold, source.italic);
                    let name = format!("PCEd{}", base.replace('-', ""));
                    let mut f = Dict::new();
                    f.set(b"Type".to_vec(), Object::name("Font"));
                    f.set(b"Subtype".to_vec(), Object::name("Type1"));
                    f.set(b"BaseFont".to_vec(), Object::name(base));
                    f.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
                    metrics.entry(name.clone()).or_insert_with(|| Metrics::from_dict(doc, &f));
                    fonts.set(name.clone().into_bytes(), Object::Dict(f));
                    substituted = Some(base.to_string());
                    name
                }
            };
            chars.push((ch, index, name));
        }
    }
    let size_of = |source: &TextLine| style.size.unwrap_or(source.size).max(0.1) / source.origin.k.max(1e-6);
    // A source run may carry TJ kerning/justification that is absent from its text state.
    // Keep its measured advance when rebuilding the run instead of making it wrap on entry.
    let mut calibration = HashMap::new();
    for index in &block.lines {
        if let Some(source) = lines.get(*index)
            && let Some(m) = metrics.get(&source.font)
        {
            let nominal = source_width(
                m,
                &source.text,
                source.origin.state.font.as_ref().map_or(1.0, |(_, s)| *s),
                source_space(doc, &fonts, m),
                source.origin.state.char_spacing,
                source.origin.state.word_spacing,
            ) * source.origin.state.scale
                * source.origin.k;
            let factor = if nominal > 0.0 { ((source.rect[2] - source.rect[0]) / nominal).clamp(0.5, 2.0) } else { 1.0 };
            calibration.insert(*index, factor);
        }
    }
    let advance = |item: &(char, usize, String)| -> f64 {
        let source = lines.get(item.1).unwrap_or(first);
        let Some(m) = metrics.get(&item.2) else { return 0.0 };
        source_width(
            m,
            &item.0.to_string(),
            size_of(source),
            source_space(doc, &fonts, m),
            style.char_spacing.map_or(source.origin.state.char_spacing, |c| c / source.origin.k.max(1e-6)),
            source.origin.state.word_spacing,
        ) * style.scale.map_or(source.origin.state.scale, |s| (s / 100.0).clamp(0.1, 10.0))
            * source.origin.k
            * calibration.get(&item.1).copied().unwrap_or(1.0)
    };
    let mut rows: Vec<Vec<(char, usize, String)>> = Vec::new();
    let mut row = Vec::new();
    let mut row_width = 0.0;
    // Wrap at word boundaries, retaining explicit source row breaks and whitespace.
    let mut at = 0;
    while at < chars.len() {
        let Some(item) = chars.get(at) else { break };
        if item.0 == '\n' {
            rows.push(std::mem::take(&mut row));
            row_width = 0.0;
            at += 1;
            continue;
        }
        let mut end = at + 1;
        if !item.0.is_whitespace() {
            while chars.get(end).is_some_and(|c| !c.0.is_whitespace()) {
                end += 1;
            }
        }
        let word = chars.get(at..end).unwrap_or_default();
        let w = word.iter().map(&advance).sum::<f64>();
        if !row.is_empty() && !item.0.is_whitespace() && row_width + w > width + 0.5 {
            while row.last().is_some_and(|c: &(char, usize, String)| c.0.is_whitespace()) {
                row.pop();
            }
            rows.push(std::mem::take(&mut row));
            row_width = 0.0;
        }
        row.extend_from_slice(word);
        row_width += w;
        at = end;
    }
    if !row.is_empty() {
        rows.push(row);
    }
    let n = pdfcraft_content::num;
    let mut block_ops = vec![Op::new("q", vec![])];
    let back = Matrix([first.origin.ctm[0], first.origin.ctm[1], first.origin.ctm[2], first.origin.ctm[3], 0.0, 0.0])
        .invert()
        .ok_or_else(|| EditError::Invalid("the paragraph is drawn in a space it cannot be edited in".into()))?;
    let (tx, ty) = back.apply(dx, dy);
    block_ops.push(Op::new("cm", vec![n(1.0), n(0.0), n(0.0), n(1.0), n(tx), n(ty)]));
    let centre = (block.rect[0] + block.rect[2]) / 2.0;
    let lefts: Vec<_> = block.rows.iter().filter_map(|r| row_rect(r, lines).map(|r| r[0])).collect();
    let left_span = lefts.iter().copied().fold(f64::NEG_INFINITY, f64::max) - lefts.iter().copied().fold(f64::INFINITY, f64::min);
    let centered = block.rows.len() > 1
        && left_span > first.size * 0.5
        && block.rows.iter().all(|row| row_rect(row, lines).is_some_and(|r| ((r[0] + r[2]) / 2.0 - centre).abs() < first.size));
    let source_leading = block
        .rows
        .windows(2)
        .filter_map(|rows| {
            let a = rows[0].first().and_then(|i| lines.get(*i))?;
            let b = rows[1].first().and_then(|i| lines.get(*i))?;
            Some((a.origin.baseline - b.origin.baseline).abs())
        })
        .rfind(|v| *v > 0.0)
        .unwrap_or(first.size * 1.5);
    let mut baseline = first.origin.baseline;
    for (row_index, row) in rows.iter().enumerate() {
        let anchor = block.rows.get(row_index).and_then(|r| r.first()).and_then(|i| lines.get(*i));
        if style.line_spacing.is_none()
            && style.size.is_none()
            && let Some(anchor) = anchor
        {
            baseline = anchor.origin.baseline;
        }
        let anchor = anchor.unwrap_or(first);
        let w = row.iter().map(&advance).sum::<f64>();
        let align = style.align.unwrap_or(if centered { crate::added::Align::Center } else { crate::added::Align::Left });
        let original_rect = block.rows.get(row_index).and_then(|row| row_rect(row, lines)).unwrap_or(block.rect);
        let left = if style.width.is_none() && style.align.is_none() { original_rect[0] } else { block.rect[0] };
        let available = if style.width.is_none() && style.align.is_none() { original_rect[2] - original_rect[0] } else { width };
        let mut x = left
            + match align {
                crate::added::Align::Center => (available - w) / 2.0,
                crate::added::Align::Right => available - w,
                _ => 0.0,
            };
        let mut pos = 0;
        while pos < row.len() {
            let Some(item) = row.get(pos) else { break };
            let source = lines.get(item.1).unwrap_or(first);
            let mut end = pos + 1;
            while row.get(end).is_some_and(|other| other.1 == item.1 && other.2 == item.2) {
                end += 1;
            }
            let segment = row.get(pos..end).unwrap_or_default();
            let string: String = segment.iter().map(|c| c.0).collect();
            let m = metrics.get(&item.2).ok_or_else(|| EditError::Invalid("the paragraph font is missing".into()))?;
            let mut state = source.origin.state.clone();
            state.font = Some((item.2.clone(), size_of(source)));
            if let Some(c) = style.char_spacing {
                state.char_spacing = c / source.origin.k.max(1e-6);
            }
            if let Some(s) = style.scale {
                state.scale = (s / 100.0).clamp(0.1, 10.0);
            }
            state.scale *= calibration.get(&item.1).copied().unwrap_or(1.0);
            if let Some([r, g, b]) = style.color {
                state.fill = vec![Op::new("rg", vec![n(r), n(g), n(b)])];
            }
            let mut tm = source.origin.tm;
            let (ax, ay) = back.apply(x - source.origin.x, baseline - source.origin.baseline);
            tm[4] += ax;
            tm[5] += ay;
            block_ops.push(Op::new("BT", vec![]));
            block_ops.extend(state.ops());
            block_ops.push(Op::new("Tm", tm.iter().map(|v| n(*v)).collect()));
            block_ops.extend(source_show(m, &string, size_of(source), source_space(doc, &fonts, m), state.char_spacing, state.word_spacing)?);
            block_ops.push(Op::new("ET", vec![]));
            let segment_width = segment.iter().map(&advance).sum::<f64>();
            if style.underline == Some(true) {
                let [r, g, b] = style.color.unwrap_or(source.color);
                let (x0, y0) = back.apply(x - first.origin.ctm[4], baseline - first.origin.ctm[5] - style.size.unwrap_or(source.size) * 0.12);
                let (x1, y1) =
                    back.apply(x + segment_width - first.origin.ctm[4], baseline - first.origin.ctm[5] - style.size.unwrap_or(source.size) * 0.12);
                block_ops.push(Op::new("RG", vec![n(r), n(g), n(b)]));
                block_ops
                    .push(Op::new("w", vec![n(style.size.unwrap_or(source.size) * 0.06 / first.origin.ctm[2].hypot(first.origin.ctm[3]).max(1e-6))]));
                block_ops.push(Op::new("m", vec![n(x0), n(y0)]));
                block_ops.push(Op::new("l", vec![n(x1), n(y1)]));
                block_ops.push(Op::new("S", vec![]));
            }
            x += segment_width;
            pos = end;
        }
        baseline -= if style.line_spacing.is_none() && style.size.is_none() {
            source_leading
        } else {
            style.line_spacing.unwrap_or(1.5).clamp(0.5, 5.0) * style.size.unwrap_or(anchor.size)
        };
    }
    block_ops.push(Op::new("Q", vec![]));
    let streams = content_streams(doc, &p.dict);
    let pieces = Pieces::join(&streams.iter().map(|(_, d)| d.as_slice()).collect::<Vec<_>>());
    let ops = pieces.parse().ops;
    let members: Vec<_> = block.lines.iter().filter_map(|i| lines.get(*i)).collect();
    let rects: Vec<_> = members.iter().map(|l| l.rect).collect();
    let mut drop = coincident_ops(lines, &rects);
    drop.extend(members.iter().flat_map(|l| l.ops.iter().copied()));
    let o = &first.origin;
    let start = first.ops.first().copied().unwrap_or(o.bt_op);
    let split = ops
        .get(o.bt_op..start)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .any(|(i, op)| matches!(op.op.as_slice(), b"Tj" | b"TJ" | b"'" | b"\"") && !drop.contains(&(o.bt_op + i)));
    let after = if split { &o.state } else { &o.bt_state };
    block_ops.extend(after.ops());
    if split {
        block_ops.insert(0, Op::new("ET", vec![]));
        block_ops.push(Op::new("BT", vec![]));
        block_ops.push(Op::new("Tm", o.tlm.iter().map(|v| n(*v)).collect()));
    }
    let at = if split { start } else { o.bt_op };
    let data = splice(&pieces, &ops, |i| (if i == at { std::mem::take(&mut block_ops) } else { Vec::new() }, !drop.contains(&i)));
    let contents = rewritten_contents(doc, &streams, data);
    res.set(b"Font".to_vec(), Object::Dict(fonts));
    doc.update_dict(p.obj, |d| {
        d.set(b"Contents".to_vec(), contents);
        d.set(b"Resources".to_vec(), Object::Dict(res));
    })?;
    Ok(LineEdit { substituted })
}
