use pdfcraft_engine::{Edit, Session};
use serde_json::json;
#[test]
fn craft_session_keeps_pdf_tabs_and_both_history_stacks() {
    let mut s = Session::new();
    let bytes = s.create_blank(200., 200., 1).unwrap();
    let id = s.open("first.pdf", None, bytes, None).unwrap();
    s.apply(id, Edit::SetInfo { key: "Title".into(), value: "First edit".into() }).unwrap();
    s.apply(id, Edit::SetInfo { key: "Title".into(), value: "Second edit".into() }).unwrap();
    s.undo(id).unwrap();
    let other = s.create_blank(100., 100., 2).unwrap();
    s.open("second.pdf", None, other, None).unwrap();
    let bytes = s.craft_capture(json!({"active":0})).unwrap();
    let mut r = Session::new();
    assert_eq!(r.craft_restore(&bytes).unwrap(), json!({"active":0}));
    assert_eq!(r.docs().len(), 2);
    assert_eq!(r.get(id).unwrap().info.title.as_deref(), Some("First edit"));
    r.redo(id).unwrap();
    assert_eq!(r.get(id).unwrap().info.title.as_deref(), Some("Second edit"));
    r.undo(id).unwrap();
    r.undo(id).unwrap();
    assert_eq!(r.get(id).unwrap().info.title, None);
    assert!(r.undo(id).is_err());
    assert!(r.craft_restore(&bytes[..20]).is_err());
    assert_eq!(r.docs().len(), 2);
}

#[test]
fn recovery_keeps_localizable_history_label_and_document_value() {
    let mut session = Session::new();
    let bytes = session.create_blank(100., 100., 1).unwrap();
    let id = session.open("labels.pdf", None, bytes, None).unwrap();
    session.apply(id, Edit::SetInfo { key: "Custom {field}".into(), value: "value".into() }).unwrap();
    let checkpoint = session.craft_capture(json!({})).unwrap();
    let mut restored = Session::new();
    restored.craft_restore(&checkpoint).unwrap();
    let label = restored.undo(id).unwrap();
    assert_eq!(label.wording, "Change {key}");
    assert_eq!(label.value.as_deref(), Some("Custom {field}"));
    assert_eq!(label.said("Changed {key}"), "Changed Custom {field}");
    assert_eq!(restored.redo(id).unwrap(), label);
}
