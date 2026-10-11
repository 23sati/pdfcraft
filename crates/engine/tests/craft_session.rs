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

#[test]
fn passwords_are_never_persisted_and_history_requires_reentry() {
    use pdfcraft_engine::Protection;
    let mut session = Session::new();
    let bytes = session.create_blank(100., 100., 1).unwrap();
    let id = session.open("protected.pdf", None, bytes, None).unwrap();
    let protect = |password: &str| Edit::Protect(Protection { open_password: Some(password.into()), ..Protection::default() });
    session.apply(id, protect("earlier-document-secret-9283")).unwrap();
    session.apply(id, Edit::SetInfo { key: "Title".into(), value: "Before change".into() }).unwrap();
    session.apply(id, protect("current-document-secret-4517")).unwrap();
    session.apply(id, Edit::SetInfo { key: "Title".into(), value: "After change".into() }).unwrap();
    session.undo(id).unwrap();
    let bytes = session.craft_capture(json!({})).unwrap();
    for secret in ["earlier-document-secret-9283", "current-document-secret-4517"] {
        assert!(!bytes.windows(secret.len()).any(|w| w == secret.as_bytes()));
    }
    let prompts = Session::craft_credentials(&bytes).unwrap();
    assert_eq!(prompts.len(), 2);
    let passwords = vec!["earlier-document-secret-9283".into(), "current-document-secret-4517".into()];
    let mut restored = Session::new();
    assert!(restored.craft_restore(&bytes).is_err());
    assert!(restored.craft_restore_with_passwords(&bytes, &["wrong".into(), "wrong".into()]).is_err());
    assert!(restored.docs().is_empty());
    restored.craft_restore_with_passwords(&bytes, &passwords).unwrap();
    restored.redo(id).unwrap();
    assert_eq!(restored.get(id).unwrap().info.title.as_deref(), Some("After change"));
    restored.undo(id).unwrap();
    restored.undo(id).unwrap();
    let saved = restored.save_bytes(id).unwrap();
    let mut check = Session::new();
    assert!(check.open("p.pdf", None, saved.clone(), Some("current-document-secret-4517")).is_err());
    check.open("p.pdf", None, saved, Some("earlier-document-secret-9283")).unwrap();
}

#[test]
fn failed_password_restore_keeps_existing_work() {
    let mut protected = Session::new();
    let bytes = protected.create_blank(100., 100., 1).unwrap();
    let id = protected.open("locked.pdf", None, bytes, None).unwrap();
    protected.apply(id, Edit::Protect(pdfcraft_engine::Protection { open_password: Some("protected-secret".into()), ..Default::default() })).unwrap();
    let checkpoint = protected.craft_capture(json!({})).unwrap();
    let mut current = Session::new();
    let bytes = current.create_blank(200., 200., 2).unwrap();
    let id = current.open("keep.pdf", None, bytes, None).unwrap();
    current.apply(id, Edit::SetInfo { key: "Title".into(), value: "Unsaved work".into() }).unwrap();
    assert!(current.craft_restore_with_passwords(&checkpoint, &["incorrect".into()]).is_err());
    assert_eq!(current.get(id).unwrap().info.title.as_deref(), Some("Unsaved work"));
    current.undo(id).unwrap();
}

#[test]
fn owner_password_and_redo_only_password_are_not_stored() {
    let mut session = Session::new();
    let bytes = session.create_blank(100., 100., 1).unwrap();
    let id = session.open("restricted.pdf", None, bytes, None).unwrap();
    session
        .apply(id, Edit::Protect(pdfcraft_engine::Protection { permissions_password: Some("owner-secret-7821".into()), ..Default::default() }))
        .unwrap();
    session.undo(id).unwrap();
    let checkpoint = session.craft_capture(json!({})).unwrap();
    assert!(!checkpoint.windows(17).any(|w| w == b"owner-secret-7821"));
    let mut restored = Session::new();
    restored.craft_restore_with_passwords(&checkpoint, &["owner-secret-7821".into()]).unwrap();
    restored.redo(id).unwrap();
    assert!(restored.get(id).unwrap().allows_security_change());
}
