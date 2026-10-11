/*!
A crate for rendering PDF files.

This crate allows you to render pages of a PDF file into bitmaps. It is supposed to be relatively
lightweight, since we do not have any dependencies on the GPU. All the rendering happens on the CPU.

The ultimate goal of this crate is to be a *feature-complete* and *performant* PDF rasterizer.
With that said, we are currently still very far away from reaching that goal: So far, no effort
has been put into performance optimizations, as we are still working on implementing missing features.
However, this crate is currently the most comprehensive and feature-complete
implementation of a PDF rasterizer in pure Rust. This claim is supported by the fact that we currently
include over 1000 PDF files in our regression test suite. The majority of those have been scraped
from the `pdf.js` and `PDFBOX` test suites and therefore represent a very large and diverse sample
of PDF files.

As mentioned, there are still some serious limitations, including lack of support for
encrypted/password-protected PDF files, blending and isolation, knockout groups as well as a range
of smaller features such as color key masking. But you should be able to render the vast majority
of PDF files without too many issues.

## Safety
This crate forbids unsafe code via a crate-level attribute.

## Examples
For usage examples, see the [example](https://github.com/LaurenzV/hayro/tree/master/hayro/examples) in
the GitHub repository.

## Cargo features
This crate has one optional feature:
- `embed-fonts`: See the description of [`hayro-interpret`](https://docs.rs/hayro-interpret/latest/hayro_interpret/#cargo-features) for more information.
*/

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use crate::renderer::Renderer;
use hayro_interpret::Device;
use hayro_interpret::FillRule;
use hayro_interpret::InterpreterCache;
use hayro_interpret::InterpreterSettings;
use hayro_interpret::hayro_syntax::Pdf;
use hayro_interpret::hayro_syntax::page::Page;
use hayro_interpret::util::{RectExt, TransformExt};
use hayro_interpret::{BlendMode, Context};
use hayro_interpret::{ClipPath, interpret_page};
use kurbo::{Affine, Rect, Shape};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::rc::Rc;

pub use hayro_interpret;
pub use hayro_interpret::hayro_syntax;
pub use vello_cpu;

use vello_cpu::color::AlphaColor;
use vello_cpu::color::Srgb;
use vello_cpu::color::palette::css::TRANSPARENT;
use vello_cpu::color::palette::css::WHITE;
use vello_cpu::{Level, Pixmap, RenderMode};

mod renderer;
/// PdfCraft patch: exported for its regression test.
pub use renderer::{image_resampling_size, tiling_cell_scale};
/// PdfCraft patch: exported for its regression test.
#[doc(hidden)]
pub use renderer::set_skip_offscreen_paths;

/// A cache used by the renderer.
///
/// Ideally, such a cache should be constructed once per PDF and then reused across
/// multiple render invocations on the same document.
#[derive(Clone, Default)]
pub struct RenderCache<'a> {
    pub(crate) interpreter_cache: InterpreterCache<'a>,
    pub(crate) outline_cache: Rc<RefCell<HashMap<u128, Rc<kurbo::BezPath>>>>,
}

impl<'a> RenderCache<'a> {
    /// Create a new render cache.
    pub fn new() -> Self {
        Self {
            interpreter_cache: InterpreterCache::new(),
            outline_cache: Rc::new(RefCell::new(HashMap::new())),
        }
    }
}

/// PdfCraft patch: replacement colours for the "reading colours" view setting (Acrobat's
/// "Replace Document Colors"). Solid paints are mapped by luminance: dark paints move towards
/// `text`, light paints towards `background`. Images, shadings and patterns never pass through
/// the mapping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorReplacement {
    /// The sRGB colour that dark paints (low luminance) are mapped towards.
    pub text: [u8; 3],
    /// The sRGB colour that light paints (high luminance) are mapped towards.
    pub background: [u8; 3],
    /// When `true`, paints with a luminance above 10% are left unchanged, so only near-black
    /// text and line art is recoloured.
    pub only_black: bool,
    /// When `true`, line-art paints are mapped in addition to text paints.
    pub line_art: bool,
}

impl ColorReplacement {
    /// Maps a solid paint colour. `is_text` distinguishes glyph fills/strokes from
    /// plain path art. Alpha is copied verbatim. Never panics.
    pub fn map(&self, rgba8: [u8; 4], is_text: bool) -> [u8; 4] {
        if !is_text && !self.line_art {
            return rgba8;
        }
        let [r, g, b, a] = rgba8;
        // Luminance of the gamma-encoded sRGB bytes, normalized to `0.0..=1.0`.
        let t = (0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)) / 255.0;
        if self.only_black && t > 0.10 {
            return rgba8;
        }
        let [text_r, text_g, text_b] = self.text;
        let [background_r, background_g, background_b] = self.background;
        let channel = |text_ch: u8, background_ch: u8| {
            let mixed = f32::from(text_ch) * (1.0 - t) + f32::from(background_ch) * t;
            // The mixture of two channel values with weights summing to one stays within
            // `0.0..=255.0` up to float noise; the clamp keeps the cast total anyway.
            mixed.round().clamp(0.0, 255.0) as u8
        };
        [
            channel(text_r, background_r),
            channel(text_g, background_g),
            channel(text_b, background_b),
            a,
        ]
    }
}

/// Settings to apply during rendering.
#[derive(Clone, Copy)]
pub struct RenderSettings {
    /// How much the contents should be scaled into the x direction.
    pub x_scale: f32,
    /// How much the contents should be scaled into the y direction.
    pub y_scale: f32,
    /// The width of the viewport. If this is set to `None`, the width will be chosen
    /// automatically based on the scale factor and the dimensions of the PDF.
    pub width: Option<u16>,
    /// The height of the viewport. If this is set to `None`, the height will be chosen
    /// automatically based on the scale factor and the dimensions of the PDF.
    pub height: Option<u16>,
    /// The background color. Determines the color of the base
    /// rectangle during rendering to a pixmap.
    pub bg_color: AlphaColor<Srgb>,
    /// PdfCraft patch: top-left corner of the rendered region, in device pixels of the scaled
    /// page. Together with `width`/`height` this renders one tile of a large page.
    pub x_offset: f32,
    /// See `x_offset`.
    pub y_offset: f32,
    /// PdfCraft patch: when set, solid text and line-art paint colours are remapped while
    /// rendering; `None` renders exactly as without the patch.
    pub color_replacement: Option<ColorReplacement>,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            x_scale: 1.0,
            y_scale: 1.0,
            width: None,
            height: None,
            bg_color: TRANSPARENT,
            x_offset: 0.0,
            y_offset: 0.0,
            color_replacement: None,
        }
    }
}

/// Render the page with the given settings to a pixmap.
pub fn render<'a>(
    page: &'a Page<'a>,
    cache: &RenderCache<'a>,
    interpreter_settings: &InterpreterSettings,
    render_settings: &RenderSettings,
) -> Pixmap {
    let (pix_width, pix_height) = render_size(page, render_settings);
    let mut pixmap = Pixmap::new(pix_width, pix_height);
    render_into(
        page,
        cache,
        interpreter_settings,
        render_settings,
        pixmap.data_as_u8_slice_mut(),
    );
    pixmap
}

/// PdfCraft patch: the size in pixels of what [`render`] produces with these settings.
pub fn render_size(page: &Page<'_>, render_settings: &RenderSettings) -> (u16, u16) {
    let (width, height) = page.render_dimensions();
    let (scaled_width, scaled_height) = (
        (width * render_settings.x_scale) as f64,
        (height * render_settings.y_scale) as f64,
    );
    (
        render_settings.width.unwrap_or(scaled_width.floor() as u16),
        render_settings
            .height
            .unwrap_or(scaled_height.floor() as u16),
    )
}

/// PdfCraft patch: [`render`] into `buffer`, premultiplied RGBA8 rows of [`render_size`]
/// pixels, so the caller chooses the allocation (one aligned to whole pixels can then become
/// texture data without a copy). `buffer.len()` must be width × height × 4; any other length
/// leaves `buffer` untouched (vello_cpu asserts on the length, and a panic must not be reachable).
pub fn render_into<'a>(
    page: &'a Page<'a>,
    cache: &RenderCache<'a>,
    interpreter_settings: &InterpreterSettings,
    render_settings: &RenderSettings,
    buffer: &mut [u8],
) {
    let (x_scale, y_scale) = (render_settings.x_scale, render_settings.y_scale);
    let initial_transform = Affine::translate((-render_settings.x_offset as f64, -render_settings.y_offset as f64))
        * Affine::scale_non_uniform(x_scale as f64, y_scale as f64)
        * page.initial_transform(true).to_kurbo();

    let (pix_width, pix_height) = render_size(page, render_settings);
    let expected = (pix_width as usize)
        .checked_mul(pix_height as usize)
        .and_then(|n| n.checked_mul(4));
    if expected != Some(buffer.len()) {
        return;
    }
    let mut state = Context::new(
        initial_transform,
        Rect::new(0.0, 0.0, pix_width as f64, pix_height as f64),
        &cache.interpreter_cache,
        page.xref(),
        interpreter_settings.clone(),
    );

    let vc_settings = vello_cpu::RenderSettings {
        level: Level::new(),
        num_threads: 0,
        render_mode: RenderMode::OptimizeSpeed,
    };

    let mut device = Renderer::new(pix_width, pix_height, vc_settings, cache);

    device.ctx.set_paint(render_settings.bg_color);
    device
        .ctx
        .fill_rect(&Rect::new(0.0, 0.0, pix_width as f64, pix_height as f64));
    let mut clip_path = page.intersected_crop_box().to_kurbo().to_path(0.1);
    clip_path.apply_affine(initial_transform);
    device.push_clip_path(&ClipPath {
        path: clip_path,
        fill: FillRule::NonZero,
    });

    device.push_transparency_group(1.0, None, BlendMode::Normal);
    interpret_page(page, &mut state, &mut device);

    device.pop_transparency_group();

    device.pop_clip_path();

    let mut resources = vello_cpu::Resources::default();
    device.ctx.render_to_buffer(
        &mut resources,
        buffer,
        pix_width,
        pix_height,
        vc_settings.render_mode,
    );
}

// Just a convenience method for testing.
#[doc(hidden)]
pub fn render_pdf(
    pdf: &Pdf,
    scale: f32,
    settings: InterpreterSettings,
    range: Option<RangeInclusive<usize>>,
) -> Option<Vec<Pixmap>> {
    let cache = RenderCache::new();
    let rendered = pdf
        .pages()
        .iter()
        .enumerate()
        .flat_map(|(idx, page)| {
            if range.clone().is_some_and(|range| !range.contains(&idx)) {
                return None;
            }

            let pixmap = render(
                page,
                &cache,
                &settings,
                &RenderSettings {
                    x_scale: scale,
                    y_scale: scale,
                    bg_color: WHITE,
                    ..Default::default()
                },
            );

            Some(pixmap)
        })
        .collect();

    Some(rendered)
}

pub(crate) fn derive_settings(settings: &vello_cpu::RenderSettings) -> vello_cpu::RenderSettings {
    vello_cpu::RenderSettings {
        num_threads: 0,
        ..*settings
    }
}

// PdfCraft patch: unit tests for the reading-colours mapping.
#[cfg(test)]
mod color_replacement_tests {
    use super::ColorReplacement;

    fn config(
        text: [u8; 3],
        background: [u8; 3],
        only_black: bool,
        line_art: bool,
    ) -> ColorReplacement {
        ColorReplacement {
            text,
            background,
            only_black,
            line_art,
        }
    }

    #[test]
    fn black_maps_to_text_colour() {
        let r = config([0x12, 0x34, 0x56], [0xff, 0xff, 0xff], false, true);
        assert_eq!(r.map([0x00, 0x00, 0x00, 0xff], true), [0x12, 0x34, 0x56, 0xff]);
    }

    #[test]
    fn white_maps_to_background() {
        let r = config([0x00, 0x00, 0x00], [0xab, 0xcd, 0xef], false, true);
        assert_eq!(r.map([0xff, 0xff, 0xff, 0xff], true), [0xab, 0xcd, 0xef, 0xff]);
    }

    #[test]
    fn midgrey_interpolates() {
        // 0x80 grey between black text and a white background lands near 0x80.
        let r = config([0x00, 0x00, 0x00], [0xff, 0xff, 0xff], false, true);
        let [mapped_r, mapped_g, mapped_b, mapped_a] = r.map([0x80, 0x80, 0x80, 0xff], true);
        assert!(mapped_r.abs_diff(0x80) <= 2);
        assert!(mapped_g.abs_diff(0x80) <= 2);
        assert!(mapped_b.abs_diff(0x80) <= 2);
        assert_eq!(mapped_a, 0xff);
    }

    #[test]
    fn alpha_copied_verbatim() {
        let r = config([0xff, 0x00, 0x00], [0x00, 0x00, 0xff], false, true);
        assert_eq!(r.map([0x00, 0x00, 0x00, 137], true), [0xff, 0x00, 0x00, 137]);
        assert_eq!(r.map([0xff, 0xff, 0xff, 0], true), [0x00, 0x00, 0xff, 0]);
    }

    #[test]
    fn only_black_threshold() {
        // White text on black: a mapped grey v becomes 255 - v, so mapped and skipped
        // values are distinguishable.
        let r = config([0xff, 0xff, 0xff], [0x00, 0x00, 0x00], true, true);
        // Luminance t = 25/255 ≈ 0.098 is just under the 0.10 threshold: mapped.
        assert_eq!(r.map([25, 25, 25, 255], true), [230, 230, 230, 255]);
        // Luminance t = 27/255 ≈ 0.106 is just over it: left unchanged.
        assert_eq!(r.map([27, 27, 27, 255], true), [27, 27, 27, 255]);
    }

    #[test]
    fn line_art_false_skips_paths_but_not_text() {
        let r = config([0xff, 0xff, 0xff], [0x00, 0x00, 0x00], false, false);
        // A plain path fill is left unchanged when line art is off.
        assert_eq!(r.map([0x00, 0x00, 0x00, 255], false), [0x00, 0x00, 0x00, 255]);
        // Text fills/strokes still map.
        assert_eq!(r.map([0x00, 0x00, 0x00, 255], true), [0xff, 0xff, 0xff, 255]);
    }
}
