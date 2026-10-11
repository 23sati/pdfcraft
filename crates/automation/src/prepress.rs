//! Print-production tools: the inks a document uses (Output preview).

use pdfcraft_engine::prepress::{Kind, Report};
use serde_json::{Value, json};

use crate::{Args, Automation, Result};

fn kind_id(k: Kind) -> &'static str {
    match k {
        Kind::Process => "process",
        Kind::Spot => "spot",
        Kind::All => "all",
        Kind::None => "none",
    }
}

fn report_json(r: &Report) -> Value {
    let inks: Vec<Value> = r
        .inks
        .iter()
        .map(|i| {
            json!({
                "name": i.name,
                "kind": kind_id(i.kind),
                // Images and shadings draw their inks outside the content stream.
                "max_tint": i.max_tint,
                "pages": i.pages.iter().map(|p| p + 1).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({ "inks": inks, "overprint_pages": r.overprint_pages.iter().map(|p| p + 1).collect::<Vec<_>>() })
}

impl Automation {
    pub(crate) fn prepress_inks(&self, a: &Args) -> Result<Value> {
        let doc = self.doc(a)?;
        match a.opt_int("page")? {
            Some(p) => {
                let n = doc.info.pages.len();
                if p < 1 || p as usize > n {
                    return Err(crate::ToolError::InvalidArgs(format!("page {p} is out of range: the document has {n} pages")));
                }
                Ok(report_json(&doc.inks_of(p as usize - 1)))
            }
            None => Ok(report_json(&doc.inks())),
        }
    }
}
