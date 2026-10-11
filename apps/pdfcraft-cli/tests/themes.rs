//! Theme management is available headlessly through the existing JSON tool runner.
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_pdfcraft-cli");

#[test]
fn theme_tool_failures_are_machine_readable_and_nonzero() {
    let store = std::env::temp_dir().join(format!("pdfcraft-cli-themes-{}.json", std::process::id()));
    let output = Command::new(BIN).args(["run", "theme_create", "name=", &format!("store={}", store.display())]).output().expect("run CLI");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let body: serde_json::Value = serde_json::from_slice(&output.stderr).expect("structured JSON error");
    assert_eq!(body["error"]["code"], "invalid_args");
    assert_eq!(body["error"]["tool"], "theme_create");
    assert!(body["error"]["message"].as_str().is_some_and(|message| !message.is_empty()));
    let _ = std::fs::remove_file(&store);
    let _ = std::fs::remove_file(format!("{}.lock", store.display()));
}

#[test]
fn tools_catalog_lists_theme_tool_schemas() {
    let output = Command::new(BIN).arg("tools").output().expect("run CLI");
    assert!(output.status.success());
    let tools: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).expect("tool list JSON");
    for name in [
        "theme_list",
        "theme_inspect",
        "theme_create",
        "theme_clone",
        "theme_update",
        "theme_rename",
        "theme_activate",
        "theme_delete",
        "theme_export",
        "theme_import",
    ] {
        let tool = tools.iter().find(|tool| tool["name"] == name).unwrap_or_else(|| panic!("missing tool {name}"));
        assert!(tool["input_schema"]["properties"]["store"].is_object(), "{name}");
    }
}

#[test]
fn complete_theme_workflow_survives_separate_cli_processes() {
    use serde_json::{Value, json};
    let root = std::env::temp_dir().join(format!("pdfcraft-cli-theme-workflow-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let run = |tool: &str, args: Value| -> Value {
        let mut command = Command::new(BIN);
        command.args(["run", tool, "--root", root.to_str().unwrap(), "store=themes.json"]);
        for (key, value) in args.as_object().unwrap() {
            command.arg(format!("{key}={value}"));
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        serde_json::from_slice(&output.stdout).unwrap()
    };
    assert_eq!(run("theme_list", json!({}))["themes"].as_array().unwrap().len(), 9);
    let palette = run("theme_inspect", json!({"id":"ocean"}))["palette"].clone();
    assert_eq!(palette.as_object().unwrap().len(), 31);
    let created = run("theme_create", json!({"name":"Professional navy","base_theme_id":"ocean","palette":palette}));
    let id = created["id"].as_str().unwrap();
    run("theme_update", json!({"id":id,"palette":{"text":"#fff4df","accent":"#efbd88"},"expected_revision":created["revision"]}));
    run("theme_rename", json!({"id":id,"name":"Navy and cream"}));
    run("theme_activate", json!({"id":id}));
    let exported = run("theme_export", json!({"id":id}));
    assert_eq!(exported["palette"]["text"], "#fff4df");
    let cloned = run("theme_clone", json!({"id":id,"name":"Navy and cream"}));
    assert_ne!(cloned["id"], id);
    run("theme_delete", json!({"id":id}));
    assert_eq!(run("theme_list", json!({}))["active_theme"], "ocean");
    run("theme_import", json!({"document":exported}));
    assert_eq!(run("theme_inspect", json!({"id":id}))["palette"]["text"], "#fff4df");
    for id in ["system", "light", "dark"] {
        run("theme_activate", json!({"id":id}));
        assert_eq!(run("theme_list", json!({}))["active_theme"], id);
    }
    let before = std::fs::read(root.join("themes.json")).unwrap();
    let failed = Command::new(BIN)
        .args(["run", "theme_import", "--root", root.to_str().unwrap(), "store=themes.json", "document={malformed"])
        .output()
        .unwrap();
    assert!(!failed.status.success());
    let error: Value = serde_json::from_slice(&failed.stderr).unwrap();
    assert_eq!(error["error"]["code"], "invalid_args");
    assert_eq!(std::fs::read(root.join("themes.json")).unwrap(), before);
    let _ = std::fs::remove_dir_all(root);
}
