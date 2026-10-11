//! The Print dialog in the real shell (egui_kittest): settings, preview sheets, Save as PDF.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::{Dialog, PdfCraftApp};

fn harness() -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("form.pdf", None, include_bytes!("data/form.pdf").to_vec()).unwrap();
        app
    });
    h.run_steps(4);
    h
}

#[test]
fn print_dialog_lays_out_sheets_and_saves_a_pdf() {
    let mut h = harness();
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(Dialog::Print));
    h.get_by_label("Pages to Print".to_uppercase().as_str());
    h.get_by_label("Sheet 1 of 1");
    // Two copies of the one page, as a 2-up poster-free layout: Multiple.
    h.get_by_label("Multiple").click();
    h.run_steps(2);
    h.get_by_label("Sheet 1 of 1");
    // No printer on a test machine: Save as PDF.
    let out = std::env::temp_dir().join(format!("pdfcraft-print-test-{}.pdf", std::process::id()));
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.state_mut().print_draft.printer = None;
    h.run_steps(1);
    h.get_by_label("Save as PDF").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, None);
    let bytes = std::fs::read(&out).expect("saved");
    let doc = pdfcraft_cos::Document::open(std::sync::Arc::new(bytes)).unwrap();
    assert_eq!(pdfcraft_model::pages(&doc).len(), 1);
    let _ = std::fs::remove_file(out);
}

/// Pages picked beforehand are what the dialog offers to print (Acrobat's "Selected pages").
#[test]
fn print_dialog_prints_the_selected_pages() {
    use pdfcraft_ui_egui::PrintWhich;
    let mut h = harness();
    let bytes = h.state().session.create_blank(200.0, 300.0, 5).unwrap();
    h.state_mut().open_bytes("five.pdf", None, bytes.as_ref().clone()).unwrap();
    h.run_steps(3);
    let tab = h.state().views.len() - 1;
    // Nothing picked: the whole document, and no "Selected pages" choice.
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    assert_eq!(h.state().print_draft.which, PrintWhich::All);
    assert!(h.query_by_label_contains("Selected pages").is_none());
    h.get_by_label("Sheet 1 of 5");
    h.state_mut().dialog = None;
    h.run_steps(2);
    // Pages 2 and 4 picked.
    h.state_mut().views[tab].select_pages(&[1, 3]);
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    assert_eq!(h.state().print_draft.which, PrintWhich::Selected);
    h.get_by_label("Selected pages (2)");
    h.get_by_label("Sheet 1 of 2");
    let out = std::env::temp_dir().join(format!("pdfcraft-print-selected-{}.pdf", std::process::id()));
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.state_mut().print_draft.printer = None;
    h.run_steps(1);
    h.get_by_label("Save as PDF").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, None);
    let saved = std::fs::read(&out).expect("saved");
    let doc = pdfcraft_cos::Document::open(std::sync::Arc::new(saved)).unwrap();
    assert_eq!(pdfcraft_model::pages(&doc).len(), 2);
    let _ = std::fs::remove_file(out);
    // The selection gone, the remembered choice falls back to the whole document.
    h.state_mut().views[tab].select_pages(&[]);
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    assert_eq!(h.state().print_draft.which, PrintWhich::All);
    assert!(h.state().print_draft.selected.is_empty());
}

#[test]
fn invalid_ranges_are_explained_in_the_preview() {
    let mut h = harness();
    h.state_mut().execute("print.dialog");
    h.run_steps(2);
    h.state_mut().print_draft.which = pdfcraft_ui_egui::PrintWhich::Range;
    h.state_mut().print_draft.range = "7".into();
    h.run_steps(2);
    h.get_by_label_contains("out of range");
}

#[test]
fn cut_stack_dialog_previews_saves_and_refuses_duplex() {
    use pdfcraft_engine::print::{PageOrder, spool::Duplex};
    let mut h = harness();
    // Ten source pages, generated using the headless engine.
    let bytes = h.state().session.create_blank(200.0, 300.0, 10).unwrap();
    h.state_mut().open_bytes("numbered.pdf", None, bytes.as_ref().clone()).unwrap();
    h.state_mut().execute("print.dialog");
    h.run_steps(3);
    h.get_by_label("Multiple").click();
    h.run_steps(2);
    // Pick the new order through the actual widgets.
    h.get_by_value("Horizontal").click();
    h.run_steps(2);
    h.get_by_label("Cut and stack").click();
    h.run_steps(2);
    assert_eq!(h.state().print_draft.order, PageOrder::CutStack);
    h.state_mut().print_draft.per_sheet = 4;
    h.run_steps(2);
    h.get_by_label("Sheet 1 of 3");
    h.get_by_label_contains("Keep the sheets in order");
    h.get_by_label("›").click();
    h.run_steps(2);
    h.get_by_label("Sheet 2 of 3");
    h.state_mut().print_draft.duplex = Duplex::LongEdge;
    h.run_steps(2);
    h.get_by_label_contains("Cut and stack needs Two-sided: Off");
    assert!(!h.state_mut().print_now());
    h.state_mut().print_draft.duplex = Duplex::Off;
    h.state_mut().print_draft.printer = None;
    let out = std::env::temp_dir().join(format!("pdfcraft-cut-stack-ui-{}.pdf", std::process::id()));
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.run_steps(2);
    h.get_by_label("Save as PDF").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, None);
    let printed = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(&out).unwrap())).unwrap();
    assert_eq!(pdfcraft_model::pages(&printed).len(), 3);
    let _ = std::fs::remove_file(out);
}

#[test]
fn a_failed_save_as_pdf_keeps_the_dialog_open() {
    let mut h = harness();
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    h.state_mut().print_draft.printer = None;
    // A folder that doesn't exist: the write fails.
    let out = std::env::temp_dir().join(format!("pdfcraft-missing-{}", std::process::id())).join("x.pdf");
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.run_steps(2);
    h.get_by_label("Save as PDF").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(Dialog::Print), "the dialog stays open, with its settings");
    let toast = h.state().toast.as_ref().map(|(m, _)| m.clone()).unwrap_or_default();
    assert!(toast.contains("Could not save"), "the user is told why: {toast:?}");
}

/// Properties… shows the printer driver's own options (CUPS) and sends only those changed.
#[test]
fn print_dialog_shows_the_printer_driver_options() {
    use pdfcraft_engine::print::spool;
    let mut h = harness();
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    h.state_mut().print_draft.printer = None;
    h.run_steps(1);
    assert!(h.query_by(|n| n.label().as_deref() == Some("Properties…") && n.is_disabled()).is_some(), "Save as PDF has no driver");
    let tray = spool::PrinterOption {
        key: "InputSlot".into(),
        label: "Paper tray".into(),
        group: "Media".into(),
        choices: vec![("AutoSelect".into(), "Auto tray select".into()), ("Tray2".into(), "Tray 2".into())],
        default: "AutoSelect".into(),
    };
    {
        let d = &mut h.state_mut().print_draft;
        d.printer = Some("Test_Queue".into());
        d.driver_options = Some(("Test_Queue".into(), vec![tray]));
        d.driver_choices.insert("InputSlot".into(), "Tray2".into());
        d.show_driver_options = true;
    }
    h.run_steps(2);
    h.get_by_label("Printer properties");
    h.get_by_label("Media");
    h.get_by_label("Paper tray");
    assert_eq!(h.state().print_draft.job("form.pdf").options, [("InputSlot".to_string(), "Tray2".to_string())]);
    h.get_by_label("Reset to printer defaults").click();
    h.run_steps(2);
    assert!(h.state().print_draft.job("form.pdf").options.is_empty(), "defaults are not sent");
    // Options read for another printer never go with this one's job.
    h.state_mut().print_draft.driver_choices.insert("InputSlot".into(), "Tray2".into());
    h.state_mut().print_draft.printer = Some("Other_Queue".into());
    assert!(h.state().print_draft.job("form.pdf").options.is_empty());
}

/// The desktop app lists printers in the background (PowerShell on Windows takes a second or
/// more): the dialog opens at once, then preselects the default when the list arrives — unless
/// the user has already picked a printer. "Check again" lists them in the background too.
#[test]
fn printers_listed_in_the_background_preselect_the_default_unless_one_was_picked() {
    use pdfcraft_engine::print::spool::{PrinterDetail, PrinterStatus};
    use std::sync::{Arc, Mutex};
    let printer = |name: &str, default: bool, status: PrinterStatus| PrinterDetail {
        name: name.into(),
        default,
        status,
        port: String::new(),
        driver: String::new(),
        network: false,
    };
    let mut h = harness();
    // The source waits until the test lets it answer, so the dialog is seen while it looks.
    let gate = Arc::new(Mutex::new(()));
    let hold = gate.lock().unwrap();
    let wait = gate.clone();
    let listed = vec![printer("Office", false, PrinterStatus::Ready), printer("Front Desk", true, PrinterStatus::Ready)];
    let answer = Arc::new(Mutex::new(listed));
    let source = answer.clone();
    h.state_mut().printer_source = Some(Arc::new(move || {
        let _open = wait.lock();
        source.lock().map(|l| l.clone()).unwrap_or_default()
    }));
    fn settle(h: &mut Harness<'static, PdfCraftApp>) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while h.state().print_draft.printers_loading && std::time::Instant::now() < deadline {
            h.run_steps(1);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!h.state().print_draft.printers_loading);
    }
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(2);
    assert_eq!(h.state().dialog, Some(Dialog::Print), "the dialog doesn't wait for the printers");
    assert!(h.state().print_draft.printers_loading);
    assert_eq!(h.state().print_draft.printer, None);
    drop(hold);
    settle(&mut h);
    assert_eq!(h.state().print_draft.printers.len(), 2);
    assert_eq!(h.state().print_draft.printer_details.len(), 2, "the status comes with the list");
    assert_eq!(h.state().print_draft.printer.as_deref(), Some("Front Desk"));

    // Opened again: last time's list shows at once; a choice made before the fresh list
    // arrives is kept.
    h.state_mut().dialog = None;
    let hold = gate.lock().unwrap();
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(2);
    assert_eq!(h.state().print_draft.printers.len(), 2, "the last list shows while looking");
    {
        let d = &mut h.state_mut().print_draft;
        d.printer = None;
        d.printer_picked = true;
    }
    drop(hold);
    settle(&mut h);
    assert_eq!(h.state().print_draft.printer, None, "Save as PDF, as picked");

    // Check again, with no default left and Front Desk offline: the dialog keeps working while
    // it looks, a printer still listed stays chosen, and Save as PDF moves to an online printer.
    if let Ok(mut l) = answer.lock() {
        *l = vec![printer("Front Desk", false, PrinterStatus::Offline), printer("Office", false, PrinterStatus::Ready)];
    }
    h.state_mut().print_draft.printer = Some("Front Desk".into());
    let hold = gate.lock().unwrap();
    h.state_mut().print_draft.recheck_printers = true;
    h.run_steps(2);
    assert!(h.state().print_draft.printers_loading, "Check again doesn't hold up the dialog");
    drop(hold);
    settle(&mut h);
    assert_eq!(h.state().print_draft.printer.as_deref(), Some("Front Desk"), "a listed printer stays chosen");
    h.state_mut().print_draft.printer = None;
    h.state_mut().print_draft.recheck_printers = true;
    h.run_steps(2);
    settle(&mut h);
    assert_eq!(h.state().print_draft.printer.as_deref(), Some("Office"), "an online printer before an offline one");
}

/// Under the Printer list the dialog says what the chosen printer is (network printer, port,
/// driver) and, translated, when it can't print; an empty list says what to do instead of nothing.
#[test]
fn printer_row_describes_network_printers_and_an_empty_list() {
    use pdfcraft_engine::print::spool::{PrinterDetail, PrinterStatus};
    use std::sync::{Arc, Mutex};
    fn settle(h: &mut Harness<'static, PdfCraftApp>) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while h.state().print_draft.printers_loading && std::time::Instant::now() < deadline {
            h.run_steps(1);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!h.state().print_draft.printers_loading);
        h.run_steps(2);
    }
    let mut h = harness();
    let shared = PrinterDetail {
        name: r"\\print01\Floor2".into(),
        default: true,
        status: PrinterStatus::Ready,
        port: "IP_10.10.20.15".into(),
        driver: "Ricoh PCL6 UniversalDriver".into(),
        network: true,
    };
    let answer = Arc::new(Mutex::new(vec![shared]));
    let source = answer.clone();
    h.state_mut().printer_source = Some(Arc::new(move || source.lock().map(|l| l.clone()).unwrap_or_default()));
    assert!(h.state_mut().execute("print.dialog"));
    settle(&mut h);
    assert_eq!(h.state().print_draft.printer.as_deref(), Some(r"\\print01\Floor2"));
    h.get_by_label_contains("Network printer  ·  Port: IP_10.10.20.15  ·  Driver: Ricoh PCL6 UniversalDriver");
    assert!(h.query_by_label_contains("Printer status:").is_none(), "no warning for a ready printer");

    h.state_mut().print_draft.printer_details[0].status = PrinterStatus::Offline;
    h.run_steps(2);
    h.get_by_label_contains("Printer status: Offline. Check the printer's connection before printing.");

    // Nothing found: Save as PDF, and a hint instead of an empty list.
    if let Ok(mut l) = answer.lock() {
        l.clear();
    }
    h.state_mut().print_draft.printer = None;
    h.state_mut().print_draft.recheck_printers = true;
    h.run_steps(2);
    settle(&mut h);
    assert!(h.state().print_draft.printers.is_empty());
    assert_eq!(h.state().print_draft.printer, None);
    h.get_by_label_contains("No printers were found.");
    assert!(h.query_by_label_contains("Network printer").is_none());
}
