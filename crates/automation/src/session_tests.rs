use super::*;
use pdfcraft_platform::staging::STAGING_ATTEMPTS;

/// `n` pages, page i showing "Page i".
fn numbered_pages(n: usize) -> Vec<u8> {
    let mut objs: Vec<String> = vec!["<< /Type /Catalog /Pages 2 0 R >>".into()];
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
    objs.push(format!("<< /Type /Pages /Kids [{}] /Count {n} /MediaBox [0 0 200 300] >>", kids.join(" ")));
    objs.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
    for i in 0..n {
        objs.push(format!("<< /Type /Page /Parent 2 0 R /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> >> >>", 5 + 2 * i));
        let body = format!("BT /F1 24 Tf 20 150 Td (Page {}) Tj ET", i + 1);
        objs.push(format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()));
    }
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn json(a: &mut Automation, tool: &str, args: Value) -> Value {
    match a.call(tool, &args).map(|mut c| c.remove(0)) {
        Ok(Content::Json(v)) => v,
        other => panic!("{tool} {args}: {other:?}"),
    }
}

#[test]
fn a_bounded_text_find_reads_only_the_pages_it_needs_and_says_it_stopped() {
    let dir = std::env::temp_dir().join(format!("pdfcraft-automation-find-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("long.pdf"), numbered_pages(200)).unwrap();
    let mut a = Automation::new().with_root(&dir).unwrap();
    let doc = json(&mut a, "doc_open", json!({ "path": "long.pdf" }))["doc"].clone();
    let extracted = |a: &Automation| a.texts.values().map(|(_, pages)| pages.iter().filter(|t| t.is_some()).count()).sum::<usize>();

    let first = json(&mut a, "text_find", json!({ "doc": doc, "query": "Page 1", "limit": 1 }));
    assert_eq!(first["count"], 1);
    assert_eq!(first["matches"][0]["page"], 1);
    assert_eq!(first["complete"], false, "{first}");
    assert_eq!(first["next"], json!({ "start_page": 10, "skip": 0 }));
    assert!(extracted(&a) <= 3 * FIND_FIRST_BATCH, "{} pages extracted for one match", extracted(&a));

    let all = json(&mut a, "text_find", json!({ "doc": doc, "query": "1", "limit": 10_000 }));
    assert_eq!(all["complete"], true);
    assert!(all.get("next").is_none());
    let mut pieced = Vec::new();
    let mut args = json!({ "doc": doc, "query": "1", "limit": 3 });
    loop {
        let found = json(&mut a, "text_find", args.clone());
        pieced.extend(found["matches"].as_array().unwrap().iter().map(|m| (m["page"].clone(), m["rects"].clone())));
        if found["complete"] == true {
            break;
        }
        args["start_page"] = found["next"]["start_page"].clone();
        args["skip"] = found["next"]["skip"].clone();
    }
    let expected: Vec<_> = all["matches"].as_array().unwrap().iter().map(|m| (m["page"].clone(), m["rects"].clone())).collect();
    assert_eq!(all["count"], expected.len());
    assert_eq!(pieced, expected);

    let few = json(&mut a, "text_find", json!({ "doc": doc, "query": "Page 200", "start_page": 150 }));
    assert_eq!((few["count"].clone(), few["complete"].clone()), (json!(1), json!(true)));
    assert!(a.call("text_find", &json!({ "doc": doc, "query": "x", "start_page": 201 })).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
