use super::*;
#[cfg(target_arch = "wasm32")]
use serde_json::{Value, json};
#[derive(serde::Serialize, serde::Deserialize)]
struct SavedView {
    id: u64,
    zoom: f32,
    scroll: [f32; 2],
    selected: std::collections::BTreeSet<usize>,
    fit: canvas::Fit,
    layout: canvas::PageLayout,
    #[serde(default)]
    continuous: bool,
    rotation: u16,
    current: usize,
    organize: bool,
    highlight: bool,
    cover: bool,
    back: Vec<usize>,
    forward: Vec<usize>,
}
impl PdfCraftApp {
    #[cfg(target_arch = "wasm32")]
    fn craft_state(&self) -> Value {
        json!({"prefs":self.persist(),"active":self.active,"views":self.views.iter().map(|v|SavedView{id:v.id.0,zoom:v.zoom,scroll:v.craft_scroll,selected:v.selected.clone(),fit:v.fit,layout:v.layout,continuous:v.continuous,rotation:v.rotation,current:v.current,organize:v.organize,highlight:v.highlight_fields,cover:v.cover,back:v.back.clone(),forward:v.forward.clone()}).collect::<Vec<_>>()})
    }
    pub fn craft_restore(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.craft_restore_with_passwords(bytes, &[])
    }
    pub fn craft_restore_with_passwords(&mut self, bytes: &[u8], passwords: &[String]) -> Result<(), String> {
        let v = Session::craft_ui(bytes)?;
        let views: Vec<SavedView> = serde_json::from_value(v["views"].clone()).map_err(|e| e.to_string())?;
        let active: Option<usize> = serde_json::from_value(v["active"].clone()).map_err(|e| e.to_string())?;
        if active.is_some_and(|i| i >= views.len()) {
            return Err("Invalid active tab".into());
        }
        if views.iter().any(|v| !v.zoom.is_finite() || v.zoom <= 0.0 || v.scroll.iter().any(|x| !x.is_finite() || *x < 0.0)) {
            return Err("Invalid PDF view".into());
        }
        self.session.craft_restore_with_passwords(bytes, passwords)?;
        let mut restored = Vec::new();
        for s in views {
            let d = self.session.get(DocId(s.id)).ok_or("Missing PDF tab")?;
            let mut view = DocView::new(d.id, &d.info, self.view_defaults);
            view.zoom = s.zoom;
            view.fit = s.fit;
            view.layout = s.layout;
            view.continuous = s.continuous;
            view.rotation = s.rotation;
            view.current = s.current.min(d.info.pages.len().saturating_sub(1));
            view.craft_scroll = s.scroll;
            view.craft_restore_scroll = true;
            view.selected = s.selected.into_iter().filter(|p| *p < d.info.pages.len()).collect();
            view.organize = s.organize;
            view.highlight_fields = s.highlight;
            view.cover = s.cover;
            view.back = s.back.into_iter().filter(|p| *p < d.info.pages.len()).collect();
            view.forward = s.forward.into_iter().filter(|p| *p < d.info.pages.len()).collect();
            restored.push(view);
        }
        if let Some(prefs) = v["prefs"].as_str() {
            self.restore(prefs);
        }
        self.views = restored;
        self.active = active;
        self.reopen_last_session = false;
        Ok(())
    }
    #[cfg(target_arch = "wasm32")]
    pub fn craft_wake(&self, ctx: egui::Context) {
        self.browser_session.start(ctx);
    }
    #[cfg(target_arch = "wasm32")]
    pub(super) fn craft_save(&self, ctx: &egui::Context) {
        let v = self.craft_state();
        self.browser_session.save(self.session.craft_signature(&v), || self.session.craft_capture(v), ctx);
    }
}
