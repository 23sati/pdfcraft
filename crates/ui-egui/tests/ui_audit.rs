//! Accessibility and spacing audit of the real shell through its AccessKit tree (egui_kittest):
//! every control a person can operate has a name a screen reader can say, and none is smaller
//! than WCAG 2.2's 24 × 24 px minimum target (SC 2.5.8, AA).

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::NodeT;
use pdfcraft_ui_egui::theme::ThemeKind;
use pdfcraft_ui_egui::{Dialog, PdfCraftApp, RightPanel};

const FIXTURE: &[u8] = b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 100] >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >> endobj
4 0 obj << /Length 37 >> stream
BT /F1 12 Tf 20 50 Td (Hello) Tj ET
endstream endobj
5 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj
trailer << /Root 1 0 R >>
%%EOF";

/// The smallest side a pointer target may have.
const MIN_TARGET: f32 = pdfcraft_ui_egui::theme::MIN_TARGET;

fn operable(role: Role) -> bool {
    matches!(
        role,
        Role::Button
            | Role::CheckBox
            | Role::RadioButton
            | Role::Switch
            | Role::Tab
            | Role::MenuItem
            | Role::MenuItemCheckBox
            | Role::MenuItemRadio
            | Role::Link
            | Role::ComboBox
            | Role::TextInput
            | Role::MultilineTextInput
            | Role::SpinButton
            | Role::Slider
    )
}

/// One finding per control that has no accessible name or a target under [`MIN_TARGET`].
fn audit(h: &Harness<'static, PdfCraftApp>, state: &str) -> Vec<String> {
    let mut found = Vec::new();
    for node in h.root().children_recursive() {
        let ak = node.accesskit_node();
        if ak.bounding_box().is_none() {
            continue;
        }
        let r = node.rect();
        // A small clickable thing with no role (a painted close button) is a control too; the
        // large ones are drag areas and the canvas.
        let bare = ak.role() == Role::Unknown && ak.data().supports_action(egui::accesskit::Action::Click) && r.width() < 100.0 && r.height() < 100.0;
        if !(operable(ak.role()) || bare) || ak.is_hidden() {
            continue;
        }
        let name = ak.label().or_else(|| ak.value()).unwrap_or_default();
        if name.trim().is_empty() {
            found.push(format!("[{state}] {:?} at {r:?} has no accessible name", ak.role()));
        }
        if r.width() < MIN_TARGET || r.height() < MIN_TARGET {
            found.push(format!("[{state}] {:?} \"{name}\" is {:.0}×{:.0}", ak.role(), r.width(), r.height()));
        }
    }
    found
}

fn harness(theme: ThemeKind) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.set_option("theme", if theme == ThemeKind::Dark { "dark" } else { "light" }).unwrap();
        app.open_bytes("notes.pdf", None, FIXTURE.to_vec()).unwrap();
        app
    });
    h.run_steps(5);
    h
}

/// Every control in the document view, each side panel and the common dialogs, in both themes,
/// has an accessible name and a target of at least 24 × 24 px.
#[test]
fn every_control_is_named_and_big_enough() {
    let mut all = Vec::new();
    for theme in [ThemeKind::Light, ThemeKind::Dark] {
        let mut h = harness(theme);
        all.extend(audit(&h, &format!("{theme:?} document")));
        for panel in [
            RightPanel::Comments,
            RightPanel::Bookmarks,
            RightPanel::Pages,
            RightPanel::Fields,
            RightPanel::Layers,
            RightPanel::Attachments,
            RightPanel::Signatures,
        ] {
            h.state_mut().right = Some(panel);
            h.run_steps(3);
            all.extend(audit(&h, &format!("{theme:?} {panel:?}")));
        }
        for dialog in [Dialog::About, Dialog::Shortcuts, Dialog::Split, Dialog::Protect, Dialog::PageBoxes] {
            h.state_mut().dialog = Some(dialog);
            h.run_steps(3);
            all.extend(audit(&h, &format!("{theme:?} {dialog:?}")));
            h.state_mut().dialog = None;
            h.run_steps(2);
        }
    }
    all.sort();
    all.dedup();
    assert!(all.is_empty(), "{} findings:\n{}", all.len(), all.join("\n"));
}

/// Dialog content keeps at least 16 px from the dialog's left and right edges (egui's own modal
/// frame left 6 px, which put titles and fields almost on the edge) and no dialog is taller than
/// the window.
#[test]
fn dialogs_have_room_around_their_content() {
    let mut h = harness(ThemeKind::Light);
    let screen = egui::vec2(1400.0, 900.0);
    // What the window shows without a dialog: those nodes are the page behind it.
    let key = |node: &egui_kittest::Node<'_>| {
        format!("{:?}{:?}{:?}", node.accesskit_node().role(), node.accesskit_node().label(), node.accesskit_node().bounding_box())
    };
    let behind: std::collections::HashSet<String> = h.root().children_recursive().map(|n| key(&n)).collect();
    for dialog in [Dialog::About, Dialog::Shortcuts, Dialog::Split, Dialog::Protect, Dialog::PageBoxes, Dialog::Preferences] {
        h.state_mut().dialog = Some(dialog);
        h.run_steps(8);
        let area = h.ctx.memory(|m| m.area_rect(egui::Id::new("dialog"))).expect("the dialog is open");
        assert!(area.height() <= screen.y && area.width() <= screen.x, "{dialog:?} is {:?} in a {screen:?} window", area.size());
        let mut checked = 0;
        for node in h.root().children_recursive() {
            let ak = node.accesskit_node();
            if !matches!(ak.role(), Role::Label | Role::Button | Role::TextInput | Role::RadioButton | Role::CheckBox | Role::ComboBox | Role::Tab)
                || ak.is_hidden()
                || ak.bounding_box().is_none()
            {
                continue;
            }
            let r = node.rect();
            if behind.contains(&key(&node)) {
                continue;
            }
            checked += 1;
            assert!(
                r.left() - area.left() >= 16.0,
                "{dialog:?}: {:?} {:?} is {:.0} px from the left edge",
                ak.role(),
                ak.label(),
                r.left() - area.left()
            );
            assert!(
                area.right() - r.right() >= 16.0,
                "{dialog:?}: {:?} {:?} is {:.0} px from the right edge",
                ak.role(),
                ak.label(),
                area.right() - r.right()
            );
        }
        assert!(checked > 3, "{dialog:?}: only {checked} controls found");
        h.state_mut().dialog = None;
        h.run_steps(2);
    }
}
