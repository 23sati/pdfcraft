//! Browser-owned controls that remain usable when the WebGL canvas cannot draw.
use eframe::{
    wasm_bindgen::{JsCast, JsValue, closure::Closure},
    web_sys,
};
use pdfcraft_ui_egui::PdfCraftApp;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn error(text: &str) -> JsValue {
    js_sys::Error::new(text).into()
}
fn field(v: &JsValue, key: &str) -> Result<JsValue, JsValue> {
    js_sys::Reflect::get(v, &key.into())
}
fn set(o: &js_sys::Object, key: &str, value: &JsValue) -> Result<(), JsValue> {
    js_sys::Reflect::set(o, &key.into(), value).map(|_| ())
}
fn text(v: &JsValue, key: &str) -> Option<String> {
    field(v, key).ok()?.as_string()
}

pub fn byte_length(bytes: &JsValue) -> Result<u32, JsValue> {
    if !bytes.is_instance_of::<js_sys::Uint8Array>() && !bytes.is_instance_of::<js_sys::ArrayBuffer>() {
        return Err(error("Expected an ArrayBuffer or Uint8Array"));
    }
    // Inspect the JS number before narrowing: modern engines can allocate buffers beyond u32.
    let length = field(bytes, "byteLength")?.as_f64().ok_or_else(|| error("Invalid PDF byte length"))?;
    if !length.is_finite() || !(1.0..=f64::from(super::policy::MAX_BYTES)).contains(&length) || length.fract() != 0.0 {
        return Err(error("PDF input must be between 1 byte and 128 MiB"));
    }
    let len = length as u32; // bounded above before the cast
    if !super::policy::valid_size(len) {
        return Err(error("Invalid PDF input size"));
    }
    Ok(len)
}

/// An embedding deployment can opt into ONE exact parent origin; never accept '*' or opaque origins.
pub fn parent_origin(window: &web_sys::Window) -> Result<String, JsValue> {
    let configured = window
        .document()
        .ok_or_else(|| error("No document"))?
        .query_selector("meta[name=\"pdfcraft-parent-origin\"]")?
        .and_then(|element| element.get_attribute("content"));
    // Only the deployment's own HTML grants cross-origin access. An arbitrary iframe URL
    // query must never grant a foreign parent access to documents fetched with this origin's cookies.
    super::policy::parent_origin(&window.location().origin()?, configured.as_deref()).map_err(|e| error(&e))
}

#[derive(Clone)]
pub struct Handle {
    pub app: Rc<RefCell<PdfCraftApp>>,
    pub suspended: Rc<Cell<bool>>,
    pub ctx: egui::Context,
}
fn perform(handle: &Handle, operation: &str, bytes: JsValue, name: JsValue) -> Result<JsValue, JsValue> {
    let mut app = handle.app.try_borrow_mut().map_err(|_| error("PdfCraft is busy; retry the request"))?;
    match operation {
        "open" => {
            if handle.suspended.get() {
                return Err(error("Graphics are unavailable; download your open documents before reloading"));
            }
            byte_length(&bytes)?;
            let name = name
                .as_string()
                .filter(|n| !n.trim().is_empty() && n.chars().count() <= 200)
                .ok_or_else(|| error("A document name of 1 to 200 characters is required"))?;
            let bytes = if let Some(a) = bytes.dyn_ref::<js_sys::Uint8Array>() { a.to_vec() } else { js_sys::Uint8Array::new(&bytes).to_vec() };
            app.open_bytes(&name, None, bytes).map_err(|e| error(&e))?;
            handle.ctx.request_repaint();
            Ok(JsValue::UNDEFINED)
        }
        "save" => {
            let index = app.active.ok_or_else(|| error("No active PDF"))?;
            export(&mut app, index)
        }
        "isDirty" => Ok(JsValue::from_bool(app.first_dirty().is_some())),
        _ => Err(error("Unsupported PdfCraft operation")),
    }
}
fn export(app: &mut PdfCraftApp, index: usize) -> Result<JsValue, JsValue> {
    let (name, bytes) = app.export_pdf(index).map_err(|e| error(&e))?;
    let result = js_sys::Object::new();
    set(&result, "name", &name.into())?;
    set(&result, "bytes", &js_sys::Uint8Array::from(bytes.as_slice()).buffer().into())?;
    Ok(result.into())
}
fn promise(result: Result<JsValue, JsValue>) -> js_sys::Promise {
    match result {
        Ok(v) => js_sys::Promise::resolve(&v),
        Err(e) => js_sys::Promise::reject(&e),
    }
}

pub fn install(handle: Handle, canvas: &web_sys::HtmlCanvasElement) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| error("No browser window"))?;
    let api = js_sys::Object::new();
    let r = handle.clone();
    let open = Closure::<dyn FnMut(JsValue, JsValue) -> js_sys::Promise>::new(move |bytes, name| promise(perform(&r, "open", bytes, name)));
    set(&api, "open", open.as_ref())?;
    open.forget();
    let r = handle.clone();
    let save = Closure::<dyn FnMut() -> js_sys::Promise>::new(move || promise(perform(&r, "save", JsValue::UNDEFINED, JsValue::UNDEFINED)));
    set(&api, "save", save.as_ref())?;
    save.forget();
    let r = handle.clone();
    let dirty = Closure::<dyn FnMut() -> Result<JsValue, JsValue>>::new(move || perform(&r, "isDirty", JsValue::UNDEFINED, JsValue::UNDEFINED));
    set(&api, "isDirty", dirty.as_ref())?;
    dirty.forget();
    set(&api, "version", &env!("CARGO_PKG_VERSION").into())?;
    js_sys::Object::freeze(&api);
    js_sys::Reflect::set(&window, &"pdfcraft".into(), &api)?;
    if let Ok(mut app) = handle.app.try_borrow_mut() {
        let mut previous = app.host_dirty.take();
        let w = window.clone();
        let mut last = None;
        app.host_dirty = Some(Box::new(move |dirty| {
            if let Some(report) = previous.as_mut() {
                report(dirty);
            }
            if last != Some(dirty) {
                last = Some(dirty);
                let detail = web_sys::CustomEventInit::new();
                detail.set_detail(&dirty.into());
                if let Ok(event) = web_sys::CustomEvent::new_with_event_init_dict("pdfcraft:dirty", &detail) {
                    let _ = w.dispatch_event(&event);
                }
            }
        }));
    }
    install_messages(&handle, &window)?;
    install_loss(&handle, canvas, &window)?;
    let ready = web_sys::CustomEvent::new("pdfcraft:ready")?;
    window.dispatch_event(&ready)?;
    Ok(())
}

fn install_messages(handle: &Handle, window: &web_sys::Window) -> Result<(), JsValue> {
    if super::query_param("host").as_deref() != Some("parent") {
        return Ok(());
    }
    let parent = window.parent()?.ok_or_else(|| error("No parent window"))?;
    if js_sys::Object::is(&parent, window) {
        return Ok(());
    }
    let origin = parent_origin(window)?;
    let r = handle.clone();
    let handler = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
        if !super::policy::accepts_message(&origin, &event.origin(), event.source().is_some_and(|s| js_sys::Object::is(&s, &parent))) {
            return;
        }
        let data = event.data();
        let Some(id) = text(&data, "requestId").filter(|s| !s.is_empty() && s.len() <= 200) else { return };
        let Some(kind) = text(&data, "type") else { return };
        let operation = match kind.as_str() {
            "pdfcraft:open" => "open",
            "pdfcraft:save" => "save",
            "pdfcraft:isDirty" => "isDirty",
            _ => return,
        };
        let answer = js_sys::Object::new();
        let result = perform(&r, operation, field(&data, "bytes").unwrap_or(JsValue::UNDEFINED), field(&data, "name").unwrap_or(JsValue::UNDEFINED));
        let built = (|| -> Result<(), JsValue> {
            set(&answer, "type", &"pdfcraft:result".into())?;
            set(&answer, "requestId", &id.into())?;
            match result {
                Ok(value) => {
                    set(&answer, "ok", &true.into())?;
                    set(&answer, "result", &value)?;
                }
                Err(e) => {
                    set(&answer, "ok", &false.into())?;
                    let message = text(&e, "message").unwrap_or_else(|| "PdfCraft request failed".into());
                    set(&answer, "error", &message.into())?;
                }
            }
            parent.post_message(&answer, &origin)
        })();
        if let Err(e) = built {
            web_sys::console::error_1(&e);
        }
    });
    window.add_event_listener_with_callback("message", handler.as_ref().unchecked_ref())?;
    handler.forget();
    Ok(())
}

fn install_loss(handle: &Handle, canvas: &web_sys::HtmlCanvasElement, window: &web_sys::Window) -> Result<(), JsValue> {
    let document = window.document().ok_or_else(|| error("No document"))?;
    let overlay = document.create_element("section")?;
    overlay.set_id("pdfcraft-graphics-recovery");
    overlay.set_attribute("role", "alertdialog")?;
    overlay.set_attribute("aria-labelledby", "graphics-recovery-title")?;
    overlay.set_attribute("tabindex", "-1")?;
    overlay.set_attribute("hidden", "")?;
    let title = document.create_element("h1")?;
    title.set_id("graphics-recovery-title");
    title.set_text_content(Some("Graphics connection lost"));
    overlay.append_child(&title)?;
    let description = document.create_element("p")?;
    description.set_text_content(Some("Editing is paused. Your open documents are still in memory. Download each PDF below, then reload and reopen your downloads. Reloading discards the in-memory session and undo history."));
    overlay.append_child(&description)?;
    let files = document.create_element("div")?;
    overlay.append_child(&files)?;
    let status = document.create_element("p")?;
    status.set_attribute("role", "status")?;
    overlay.append_child(&status)?;
    let reload = document.create_element("button")?;
    reload.set_text_content(Some("Reload editor"));
    overlay.append_child(&reload)?;
    let w = window.clone();
    let click = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        let _ = w.location().reload();
    });
    reload.add_event_listener_with_callback("click", click.as_ref().unchecked_ref())?;
    click.forget();
    document.body().ok_or_else(|| error("No body"))?.append_child(&overlay)?;
    let r = handle.clone();
    let c = canvas.clone();
    let handler = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
        event.prevent_default();
        if r.suspended.replace(true) {
            return;
        }
        let _ = c.set_attribute("inert", "");
        let _ = overlay.remove_attribute("hidden");
        if let Some(el) = overlay.dyn_ref::<web_sys::HtmlElement>() {
            let _ = el.focus();
        }
        let Ok(app) = r.app.try_borrow() else {
            status.set_text_content(Some("The editor is unavailable. Keep this page open while you recover any saved copies."));
            return;
        };
        for (index, view) in app.views.iter().enumerate() {
            let name = app.session.get(view.id).map(|d| d.name.clone()).unwrap_or_else(|| "document.pdf".into());
            let Ok(button) = document.create_element("button") else { continue };
            button.set_text_content(Some(&format!("Download {name}")));
            let r = r.clone();
            let status = status.clone();
            let click = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                let result = (|| -> Result<(), JsValue> {
                    let mut app = r.app.try_borrow_mut().map_err(|_| error("Editor busy; retry download"))?;
                    let value = export(&mut app, index)?;
                    let bytes = field(&value, "bytes")?;
                    let name = text(&value, "name").unwrap_or_else(|| "document.pdf".into());
                    download(&name, &bytes)?;
                    status.set_text_content(Some(&format!("Downloaded {name}. Keep the download before reloading.")));
                    Ok(())
                })();
                if let Err(e) = result {
                    status.set_text_content(Some(&text(&e, "message").unwrap_or_else(|| "Download failed; do not reload yet".into())));
                }
            });
            if button.add_event_listener_with_callback("click", click.as_ref().unchecked_ref()).is_ok() {
                click.forget();
                let _ = files.append_child(&button);
            }
        }
    });
    canvas.add_event_listener_with_callback("webglcontextlost", handler.as_ref().unchecked_ref())?;
    handler.forget();
    Ok(())
}

fn download(name: &str, bytes: &JsValue) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| error("No window"))?;
    let document = window.document().ok_or_else(|| error("No document"))?;
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/pdf");
    let blob = web_sys::Blob::new_with_buffer_source_sequence_and_options(&js_sys::Array::of1(bytes), &options)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;
    let result = (|| -> Result<(), JsValue> {
        let a = document.create_element("a")?.dyn_into::<web_sys::HtmlAnchorElement>()?;
        a.set_href(&url);
        a.set_download(name);
        document.body().ok_or_else(|| error("No body"))?.append_child(&a)?;
        a.click();
        a.remove();
        Ok(())
    })();
    // Revoke on a later turn, after the browser has consumed the link's Blob URL.
    let revoke = Closure::once(move || {
        let _ = web_sys::Url::revoke_object_url(&url);
    });
    window.set_timeout_with_callback_and_timeout_and_arguments_0(revoke.as_ref().unchecked_ref(), 1000)?;
    revoke.forget();
    result
}
