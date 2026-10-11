use super::*;
use crate::craft_codec as codec;
use serde_json::{Value, json};
/// Passwords exist only in this temporary capture registry. Checkpoints contain indices
/// and prompts, never credentials, including credentials from older undo/redo states.
#[derive(Default)]
struct Credentials {
    values: Vec<String>,
    prompts: Vec<String>,
}
impl Credentials {
    fn reference(&mut self, value: Option<&str>, prompt: &str) -> Option<usize> {
        let value = value.filter(|p| !p.is_empty())?;
        if let Some(index) = self.values.iter().position(|p| p == value) {
            return Some(index);
        }
        let index = self.values.len();
        self.values.push(value.to_owned());
        self.prompts.push(prompt.to_owned());
        Some(index)
    }
    fn keys(&mut self, keys: &Keys, prompt: &str) -> Value {
        json!({"render": self.reference(keys.render.as_deref(), &format!("{prompt}: opening password")),
               "reopen": self.reference(keys.reopen.as_deref(), &format!("{prompt}: editing/owner password"))})
    }
}
fn password(value: &Value, supplied: &[String]) -> Result<Option<String>, String> {
    let index: Option<usize> = codec::decode(value)?;
    index.map(|i| supplied.get(i).cloned().ok_or_else(|| "A document password is required".into())).transpose()
}
fn restore_keys(value: &Value, supplied: &[String]) -> Result<Keys, String> {
    Ok(Keys { render: password(&value["render"], supplied)?, reopen: password(&value["reopen"], supplied)? })
}
impl Session {
    pub fn craft_signature(&self, ui: &Value) -> String {
        json!([ui, self.docs.iter().map(|d| (d.id.0, d.generation, d.dirty)).collect::<Vec<_>>()]).to_string()
    }
    pub fn craft_capture(&self, ui: Value) -> Result<Vec<u8>, String> {
        let mut credentials = Credentials::default();
        let mut blobs = Vec::new();
        let mut docs = Vec::new();
        for d in &self.docs {
            let current = codec::push(&mut blobs, self.save_bytes(d.id).map_err(|e| e.to_string())?.as_ref().clone())?;
            let editor = if let Some(e) = &d.editor {
                let mut stack = |s: &Vec<Snapshot>| -> Result<Vec<Value>, String> {
                    s.iter()
                        .map(|(label, cos, keys, scope)| {
                            let i = codec::push(&mut blobs, write_incremental(cos, &SaveOptions::default()).map_err(|e| e.to_string())?)?;
                            Ok(json!([{ "wording": label.wording, "value": label.value }, i, credentials.keys(keys, &format!("{} — {}", d.name, label.wording)), scope]))
                        })
                        .collect()
                };
                let undo = stack(&e.undo)?;
                let redo = stack(&e.redo)?;
                json!({"undo":undo,"redo":redo,"keys":credentials.keys(&e.keys, &d.name)})
            } else {
                Value::Null
            };
            docs.push(json!({"blob":current,"id":d.id.0,"name":d.name,"path":d.path,"password":credentials.reference(d.password.as_deref(), &format!("{}: opening password", d.name)),"dirty":d.dirty,"editor":editor}));
        }
        codec::pack(&json!({"app":"pdfcraft","schema":2,"credentials":credentials.prompts,"ui":ui,"docs":docs,"nextId":self.next_id}), &blobs)
    }
    pub fn craft_restore(&mut self, bytes: &[u8]) -> Result<Value, String> {
        self.craft_restore_with_passwords(bytes, &[])
    }
    /// Prompt labels in credential order. The caller must keep supplied passwords in memory only.
    pub fn craft_credentials(bytes: &[u8]) -> Result<Vec<String>, String> {
        let (v, _) = codec::unpack(bytes)?;
        if v["app"] != "pdfcraft" || v["schema"] != 2 {
            return Err("Unsupported recovery version; the original copy has not been changed".into());
        }
        let prompts: Vec<String> = codec::decode(&v["credentials"])?;
        if prompts.len() > 4096 {
            return Err("Too many recovery credentials".into());
        }
        Ok(prompts)
    }
    /// Restore atomically: a wrong/missing password leaves the current documents untouched.
    pub fn craft_restore_with_passwords(&mut self, bytes: &[u8], supplied: &[String]) -> Result<Value, String> {
        let prompts = Self::craft_credentials(bytes)?;
        if prompts.len() != supplied.len() {
            return Err("Document passwords are required to restore this session".into());
        }
        let (v, blobs) = codec::unpack(bytes)?;
        if v["app"] != "pdfcraft" || v["schema"] != 2 {
            return Err("Incompatible PDFCraft session".into());
        }
        let mut next = Session::new();
        next.trust = self.trust.clone();
        next.js_off = self.js_off;
        for m in v["docs"].as_array().ok_or("Missing documents")? {
            let password = password(&m["password"], supplied)?;
            let keys: Option<Keys> = if m["editor"].is_null() { None } else { Some(restore_keys(&m["editor"]["keys"], supplied)?) };
            let open_password = keys.as_ref().and_then(|k| k.reopen.as_deref()).or(password.as_deref());
            let _id = next
                .open(
                    codec::decode::<String>(&m["name"])?,
                    codec::decode(&m["path"])?,
                    Arc::new(codec::blob(&blobs, codec::decode(&m["blob"])?)?.to_vec()),
                    open_password,
                )
                .map_err(|e| e.to_string())?;
            let d = next.docs.last_mut().ok_or("Missing restored document")?;
            if let Some(keys) = keys {
                let stack = |s: &Value| -> Result<Vec<Snapshot>, String> {
                    s.as_array()
                        .ok_or("Missing history")?
                        .iter()
                        .map(|h| {
                            let keys = restore_keys(&h[2], supplied)?;
                            let cos = pdfcraft_cos::Document::open_with_password(
                                Arc::new(codec::blob(&blobs, codec::decode(&h[1])?)?.to_vec()),
                                keys.reopen.as_deref(),
                            )
                            .map_err(|e| e.to_string())?;
                            Ok((restore_label(&h[0])?, cos, keys, codec::decode(&h[3])?))
                        })
                        .collect()
                };
                let e = d.editor.as_mut().ok_or("Could not restore editable PDF")?;
                e.undo = stack(&m["editor"]["undo"])?;
                e.redo = stack(&m["editor"]["redo"])?;
                e.keys = keys;
            }
            d.id = DocId(codec::decode(&m["id"])?);
            d.dirty = codec::decode(&m["dirty"])?;
        }
        if next.docs.iter().map(|d| d.id.0).collect::<std::collections::BTreeSet<_>>().len() != next.docs.len() {
            return Err("Duplicate PDF tab identifiers".into());
        }
        next.next_id = codec::decode(&v["nextId"])?;
        if let Some(views) = v["ui"]["views"].as_array() {
            for view in views {
                let id: u64 = codec::decode(&view["id"])?;
                if !next.docs.iter().any(|d| d.id.0 == id) {
                    return Err("Missing PDF tab".into());
                }
            }
        }
        if next.docs.iter().any(|d| d.id.0 > next.next_id || d.id.0 == 0) {
            return Err("Invalid PDF identifier sequence".into());
        }
        self.docs = next.docs;
        self.next_id = next.next_id;
        Ok(v["ui"].clone())
    }
}

impl Session {
    pub fn craft_ui(bytes: &[u8]) -> Result<Value, String> {
        let (v, _) = crate::craft_codec::unpack(bytes)?;
        Ok(v["ui"].clone())
    }
}

fn restore_label(value: &Value) -> Result<Label, String> {
    if let Some(text) = value.as_str() {
        // Legacy checkpoints stored the complete English label.
        return Ok(Label::with("{batch}", text));
    }
    let wording = value["wording"].as_str().ok_or("Missing history label")?;
    let wording = EDIT_LABELS.iter().copied().chain(std::iter::once("{batch}")).find(|known| *known == wording).ok_or("Unknown history label")?;
    let supplied: Option<String> = codec::decode(&value["value"])?;
    Ok(match supplied {
        Some(v) => Label::with(wording, v),
        None => Label::of(wording),
    })
}
