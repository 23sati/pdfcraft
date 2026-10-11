use pdfcraft_ui_egui::PdfCraftApp;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub struct Shell {
    pub app: Rc<RefCell<PdfCraftApp>>,
    pub suspended: Rc<Cell<bool>>,
}
impl eframe::App for Shell {
    #[cfg(target_arch = "wasm32")]
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if self.suspended.get() {
            raw.events.clear();
            raw.dropped_files.clear();
        } else if let Ok(mut app) = self.app.try_borrow_mut() {
            app.raw_input_hook(ctx, raw);
        }
    }
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if !self.suspended.get()
            && let Ok(mut app) = self.app.try_borrow_mut()
        {
            app.logic(ctx, frame);
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if !self.suspended.get()
            && let Ok(mut app) = self.app.try_borrow_mut()
        {
            app.ui(ui, frame);
        }
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Ok(mut app) = self.app.try_borrow_mut() {
            app.save(storage);
        }
    }
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        self.app.try_borrow().map(|app| app.clear_color(visuals)).unwrap_or([0.0, 0.0, 0.0, 1.0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn context_loss_freezes_pending_edits_and_keeps_documents_exportable() {
        let app = Rc::new(RefCell::new(PdfCraftApp::new()));
        app.borrow_mut().open_bytes("page.txt", None, b"Synthetic document".to_vec()).unwrap();
        let suspended = Rc::new(Cell::new(false));
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1000.0, 700.0))
            .build_eframe(|_| Shell { app: app.clone(), suspended: suspended.clone() });
        harness.run_steps(3);
        let id = app.borrow().views[0].id;
        suspended.set(true);
        app.borrow_mut().views[0].pending_edit = Some(pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: 90 });
        harness.run_steps(3);
        assert_eq!(app.borrow().session.get(id).unwrap().info.pages[0].rotation, 0);
        // Shared app ownership survives renderer teardown, and recovery can still export.
        drop(harness);
        let (_, bytes) = app.borrow_mut().export_pdf(0).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert_eq!(app.borrow().session.get(id).unwrap().info.pages[0].rotation, 90);
    }
}
