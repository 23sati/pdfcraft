use super::*;
use crate::craft_codec as codec;
use serde_json::{Value, json};
impl Session {
    pub fn craft_signature(&self, ui: &Value) -> String {
        json!([ui, self.docs.iter().map(|d| (d.id.0, d.generation, d.dirty)).collect::<Vec<_>>()]).to_string()
    }
    pub fn craft_capture(&self, ui: Value) -> Result<Vec<u8>, String> {
        let mut blobs = Vec::new();
        let mut docs = Vec::new();
        for d in &self.docs {
            let current = codec::push(&mut blobs, self.save_bytes(d.id).map_err(|e| e.to_string())?.as_ref().clone())?;
            let editor = if let Some(e) = &d.editor {
                let mut stack = |s: &Vec<Snapshot>| -> Result<Vec<Value>, String> {
                    s.iter()
                        .map(|(label, cos, keys, scope)| {
                            let i = codec::push(&mut blobs, write_incremental(cos, &SaveOptions::default()).map_err(|e| e.to_string())?)?;
                            Ok(json!([label, i, keys, scope]))
                        })
                        .collect()
                };
                json!({"undo":stack(&e.undo)?,"redo":stack(&e.redo)?,"keys":e.keys})
            } else {
                Value::Null
            };
            docs.push(json!({"blob":current,"id":d.id.0,"name":d.name,"path":d.path,"password":d.password,"dirty":d.dirty,"editor":editor}));
        }
        codec::pack(&json!({"app":"pdfcraft","schema":1,"ui":ui,"docs":docs,"nextId":self.next_id}), &blobs)
    }
    pub fn craft_restore(&mut self, bytes: &[u8]) -> Result<Value, String> {
        let (v, blobs) = codec::unpack(bytes)?;
        if v["app"] != "pdfcraft" || v["schema"] != 1 {
            return Err("Incompatible PDFCraft session".into());
        }
        let mut next = Session::new();
        next.trust = self.trust.clone();
        next.js_off = self.js_off;
        for m in v["docs"].as_array().ok_or("Missing documents")? {
            let password: Option<String> = codec::decode(&m["password"])?;
            let keys: Option<Keys> = if m["editor"].is_null() { None } else { Some(codec::decode(&m["editor"]["keys"])?) };
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
                            let keys: Keys = codec::decode(&h[2])?;
                            let cos = pdfcraft_cos::Document::open_with_password(
                                Arc::new(codec::blob(&blobs, codec::decode(&h[1])?)?.to_vec()),
                                keys.reopen.as_deref(),
                            )
                            .map_err(|e| e.to_string())?;
                            Ok((codec::decode(&h[0])?, cos, keys, codec::decode(&h[3])?))
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
