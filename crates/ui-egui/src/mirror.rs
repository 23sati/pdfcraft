//! Hand-painted widgets in a right-to-left interface.
//!
//! Layouts mirror themselves (the vendored egui's `Style::mirror_layout`), but widgets that paint
//! at offsets from a rect's left edge (an icon, then a label) do not. They measure from the
//! *start* edge through these helpers instead: the left edge in a left-to-right interface, the
//! right edge in a right-to-left one. The document canvas never uses them: pages are not mirrored.

use egui::{Align, Align2, Pos2, Rect, Vec2, pos2};

/// Whether the interface is laid out right to left (the current language is).
pub fn rtl() -> bool {
    crate::i18n::current().rtl()
}

/// The point `offset` from the rect's start (top-left in left-to-right, top-right in
/// right-to-left).
pub fn at(rect: Rect, offset: Vec2) -> Pos2 {
    if rtl() { pos2(rect.max.x - offset.x, rect.min.y + offset.y) } else { rect.min + offset }
}

/// The point `dx` in from the rect's start edge, vertically centred.
pub fn start_center(rect: Rect, dx: f32) -> Pos2 {
    at(rect, egui::vec2(dx, rect.height() * 0.5))
}

/// The point `dx` in from the rect's end edge (right in left-to-right), vertically centred.
pub fn end_center(rect: Rect, dx: f32) -> Pos2 {
    at(rect, egui::vec2(rect.width() - dx, rect.height() * 0.5))
}

/// A `size` box placed `offset` from the rect's start.
pub fn rect_at(rect: Rect, offset: Vec2, size: Vec2) -> Rect {
    if rtl() {
        Rect::from_min_size(pos2(rect.max.x - offset.x - size.x, rect.min.y + offset.y), size)
    } else {
        Rect::from_min_size(rect.min + offset, size)
    }
}

/// `align` with left and right swapped in a right-to-left interface, for text anchored at a
/// start-edge point: `LEFT_CENTER` text grows away from the start edge.
pub fn align(align: Align2) -> Align2 {
    if !rtl() {
        return align;
    }
    let x = match align.x() {
        Align::Min => Align::Max,
        Align::Center => Align::Center,
        Align::Max => Align::Min,
    };
    Align2([x, align.y()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Lang, set_current};
    use egui::vec2;

    #[test]
    fn positions_follow_the_interface_direction() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(100.0, 30.0));
        let previous = crate::i18n::current();
        set_current(Lang::EN);
        assert_eq!(at(rect, vec2(6.0, 4.0)), pos2(16.0, 24.0));
        assert_eq!(rect_at(rect, vec2(6.0, 4.0), vec2(16.0, 16.0)).min, pos2(16.0, 24.0));
        assert_eq!(align(Align2::LEFT_CENTER), Align2::LEFT_CENTER);
        set_current(Lang::from_code("he").unwrap());
        assert_eq!(at(rect, vec2(6.0, 4.0)), pos2(104.0, 24.0));
        assert_eq!(rect_at(rect, vec2(6.0, 4.0), vec2(16.0, 16.0)).min, pos2(88.0, 24.0));
        assert_eq!(start_center(rect, 34.0), pos2(76.0, 35.0));
        assert_eq!(align(Align2::LEFT_CENTER), Align2::RIGHT_CENTER);
        assert_eq!(align(Align2::CENTER_TOP), Align2::CENTER_TOP);
        set_current(previous);
    }
}
