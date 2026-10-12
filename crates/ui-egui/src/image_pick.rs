//! Select tool: click a picture on the page to select it, then ⌘C / Ctrl+C (or the right-click
//! menu) copies it in its original format: a JPEG image as the JPEG file stored in the PDF, any
//! other image as a lossless PNG (with its transparency). The clipboard gets the file, as when a
//! file is copied in Finder or Explorer; Copy Image copies the pixels instead.

use egui::{Color32, CornerRadius, Pos2, Stroke};

use crate::PdfCraftApp;
use crate::canvas::{DocView, PageXform};
use pdfcraft_render::DocInfo;

/// The selected picture: page, index in `page_images` and its box in user space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickedImage {
    pub page: usize,
    pub index: usize,
    pub rect: [f64; 4],
}

const BLUE: Color32 = Color32::from_rgb(0x3A, 0x7B, 0xF0);

/// A click on page `page` (Select tool, not taken by anything else) selects the picture under
/// it, even with text drawn over it (scans with an OCR layer, captions); dragging or a double
/// click still selects text, and clears the picture.
pub(crate) fn page_input(
    resp: &egui::Response,
    xf: &PageXform,
    page: usize,
    info: &DocInfo,
    doc: &pdfcraft_engine::Document,
    pointer: Option<Pos2>,
    view: &mut DocView,
) {
    let Some(p) = pointer.filter(|p| xf.rect.contains(*p)) else { return };
    if resp.drag_started() || resp.double_clicked() || resp.triple_clicked() {
        view.picked_image = None;
        return;
    }
    if !resp.clicked() {
        return;
    }
    let Some(info_page) = info.pages.get(page) else { return };
    let (vx, vy) = xf.screen_to_view(p);
    let u = info_page.view_to_user(vx, vy);
    let at = [f64::from(u[0]), f64::from(u[1])];
    view.picked_image =
        doc.page_image_at(page, at).and_then(|index| doc.page_images(page).get(index).map(|i| PickedImage { page, index, rect: i.rect }));
}

/// The selected picture's highlight on page `page`.
pub(crate) fn paint(painter: &egui::Painter, xf: &PageXform, page: usize, info: &DocInfo, view: &DocView) {
    let Some(picked) = view.picked_image.filter(|p| p.page == page) else { return };
    if info.pages.get(page).is_none() || !picked.rect.iter().all(|v| v.is_finite()) {
        return;
    }
    let r = xf.user_rect(info, page, picked.rect.map(|v| v as f32));
    painter.rect_filled(r, CornerRadius::ZERO, Color32::from_rgba_unmultiplied(0x3A, 0x7B, 0xF0, 50));
    painter.rect_stroke(r, CornerRadius::ZERO, Stroke::new(2.0, BLUE), egui::StrokeKind::Outside);
}

/// A file name without characters file systems refuse.
fn safe_name(s: &str) -> String {
    let cleaned: String = s.chars().map(|c| if c.is_control() || r#"/\:*?"<>|"#.contains(c) { '_' } else { c }).take(80).collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() { "PdfCraft".into() } else { cleaned }
}

impl PdfCraftApp {
    /// ⌘C / Ctrl+C copies the selected picture in its original format; Escape deselects it.
    pub(crate) fn image_pick_keys(&mut self, i: usize, ctx: &egui::Context) {
        let Some(picked) = self.views.get(i).and_then(|v| v.picked_image) else { return };
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        if ctx.input(|inp| inp.key_pressed(egui::Key::Escape)) {
            self.views[i].picked_image = None;
            return;
        }
        let copy = ctx.input(|inp| inp.events.iter().any(|e| matches!(e, egui::Event::Copy)));
        if !copy || self.views[i].selected_text().is_some() {
            return;
        }
        // The document may have changed since the click: the same picture must still be there.
        let current = self.session.get(self.views[i].id).and_then(|d| d.page_images(picked.page).get(picked.index).map(|im| im.rect));
        if current == Some(picked.rect) {
            self.copy_page_image_file(picked.page, picked.index);
        } else {
            self.views[i].picked_image = None;
            self.notify_tr("The page has changed; select the image again");
        }
    }

    /// Copy Image in Original Format: image `index` on `page` as a file on the clipboard (JPEG
    /// as stored in the PDF, otherwise PNG).
    pub(crate) fn copy_page_image_file(&mut self, page: usize, index: usize) {
        let Some((_, id)) = self.active_ids() else { return };
        if !self.may_copy(id) {
            return;
        }
        let Some(doc) = self.session.get(id) else { return };
        let stem = safe_name(doc.name.rsplit_once('.').map_or(doc.name.as_str(), |(s, _)| s));
        let file = doc.page_image_copy_file(page, index);
        let copied = file.and_then(|(ext, bytes)| {
            let name = format!("{stem} p{} img{}.{ext}", page.saturating_add(1), index.saturating_add(1));
            self.put_file_on_clipboard(&name, &bytes)?;
            self.last_copied_file = Some((name, bytes));
            Ok(ext)
        });
        match copied {
            Ok(ext) => self.notify_fmt("The image has been copied as a {ext} file", &[("ext", &ext.to_uppercase())]),
            Err(e) => self.notify_fmt("Couldn't copy the image: {e}", &[("e", &e)]),
        }
    }

    /// Writes `bytes` to a PdfCraft folder in the temporary directory (replacing the previous
    /// copy) and puts that file on the system clipboard.
    #[cfg(not(target_arch = "wasm32"))]
    fn put_file_on_clipboard(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        if !self.system_clipboard {
            return Ok(());
        }
        let dir = std::env::temp_dir().join("pdfcraft-clipboard");
        // Only the latest copy needs to exist; older ones are removed (best effort).
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let _ = std::fs::remove_file(e.path());
            }
        }
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(name);
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        arboard::Clipboard::new().and_then(|mut c| c.set().file_list(&[path])).map_err(|e| e.to_string())
    }

    #[cfg(target_arch = "wasm32")]
    fn put_file_on_clipboard(&self, _name: &str, _bytes: &[u8]) -> Result<(), String> {
        if self.system_clipboard {
            return Err("the web version can't put files on the clipboard yet; use Save Image As".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::safe_name;

    #[test]
    fn file_names_are_safe() {
        assert_eq!(safe_name("a/b:c"), "a_b_c");
        assert_eq!(safe_name("  ..  "), "PdfCraft");
        assert_eq!(safe_name("Relazione 2026"), "Relazione 2026");
    }
}
