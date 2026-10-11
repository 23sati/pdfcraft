//! A visual editor for properties of reachable indirect structure tags.

use egui::Ui;
use pdfcraft_engine::a11y::{TagInfo, TagProperty};
use pdfcraft_engine::{DocId, Edit};

use crate::{theme::Tokens, widgets};

#[derive(Clone, Debug, Default)]
pub(crate) struct State {
    doc: Option<DocId>,
    selected: Option<pdfcraft_cos::ObjRef>,
    query: String,
    draft: Draft,
}

#[derive(Clone, Debug, Default)]
struct Draft {
    kind: String,
    title: String,
    actual_text: String,
    alt: String,
    language: String,
}

impl Draft {
    fn from_tag(tag: &TagInfo) -> Self {
        Self {
            kind: tag.kind.clone(),
            title: tag.title.clone().unwrap_or_default(),
            actual_text: tag.actual_text.clone().unwrap_or_default(),
            alt: tag.alt.clone().unwrap_or_default(),
            language: tag.language.clone().unwrap_or_default(),
        }
    }

    fn edit(&self, tag: &TagInfo) -> Option<Edit> {
        let mut edits = Vec::new();
        let mut add = |property, prior: Option<&str>, next: &str| {
            let value = (!next.is_empty()).then(|| next.to_owned());
            if prior != value.as_deref() {
                edits.push(Edit::SetTagProperty { object: tag.object, property, value });
            }
        };
        add(TagProperty::Type, Some(&tag.kind), &self.kind);
        add(TagProperty::Title, tag.title.as_deref(), &self.title);
        add(TagProperty::ActualText, tag.actual_text.as_deref(), &self.actual_text);
        add(TagProperty::AlternateText, tag.alt.as_deref(), &self.alt);
        add(TagProperty::Language, tag.language.as_deref(), &self.language);
        match edits.len() {
            0 => None,
            1 => edits.pop(),
            _ => Some(Edit::Batch { label: "Change tag properties".into(), edits }),
        }
    }
}

/// The chosen page and an edit to apply after the panel releases its document borrow.
pub(crate) fn panel(ui: &mut Ui, t: &Tokens, doc: DocId, tags: &[TagInfo], state: &mut State, can_edit: bool) -> (Option<usize>, Option<Edit>) {
    if state.doc != Some(doc) {
        state.doc = Some(doc);
        state.selected = None;
        state.draft = Draft::default();
        state.query.clear();
    }
    if tags.is_empty() {
        ui.label(tl!("This document has no editable structure tags."));
        return (None, None);
    }
    if state.selected.is_none_or(|r| !tags.iter().any(|t| t.object == r)) {
        state.selected = Some(tags[0].object);
        state.draft = Draft::from_tag(&tags[0]);
    }
    ui.add(egui::TextEdit::singleline(&mut state.query).hint_text(tl!("Find a tag")));
    ui.add_space(4.0);
    let query = state.query.to_lowercase();
    let matching: Vec<&TagInfo> = tags
        .iter()
        .filter(|tag| {
            query.is_empty() || tag.kind.to_lowercase().contains(&query) || tag.title.as_ref().is_some_and(|v| v.to_lowercase().contains(&query))
        })
        .collect();
    let dirty = state.selected.and_then(|r| tags.iter().find(|t| t.object == r)).is_some_and(|tag| state.draft.edit(tag).is_some());
    ui.label(egui::RichText::new(crate::i18n::fmt(tl!("{n} tags"), &[("n", &matching.len().to_string())])).small().color(t.text_muted));
    let mut page = None;
    egui::ScrollArea::vertical().max_height(120.0).show_rows(ui, 26.0, matching.len(), |ui, range| {
        for tag in &matching[range] {
            let title = tag.title.as_deref().filter(|s| !s.is_empty()).unwrap_or(&tag.kind);
            let label = format!("{title} · {}", tag.object.num);
            if ui
                .add_enabled(!dirty || state.selected == Some(tag.object), egui::Button::selectable(state.selected == Some(tag.object), label))
                .clicked()
            {
                state.selected = Some(tag.object);
                state.draft = Draft::from_tag(tag);
                page = tag.page;
            }
        }
    });
    ui.separator();
    let Some(tag) = state.selected.and_then(|r| tags.iter().find(|t| t.object == r)) else { return (page, None) };
    ui.label(egui::RichText::new(format!("{} {} R", tag.object.num, tag.object.generation)).small().color(t.text_faint));
    if let Some(p) = tag.page
        && ui.button(crate::i18n::fmt(tl!("Go to page {p}"), &[("p", &(p + 1).to_string())])).clicked()
    {
        page = Some(p);
    }
    ui.add_enabled_ui(can_edit, |ui| {
        field(ui, tl!("Tag type"), &mut state.draft.kind, false);
        field(ui, tl!("Title"), &mut state.draft.title, false);
        field(ui, tl!("Actual text"), &mut state.draft.actual_text, true);
        field(ui, tl!("Alternate text"), &mut state.draft.alt, true);
        field(ui, tl!("Language"), &mut state.draft.language, false);
    });
    let edit = state.draft.edit(tag);
    let mut save = false;
    ui.horizontal(|ui| {
        save = ui.add_enabled(can_edit && !state.draft.kind.is_empty() && edit.is_some(), egui::Button::new(tl!("Save tag properties"))).clicked();
        if ui.add_enabled(dirty, egui::Button::new(tl!("Revert changes"))).clicked() {
            state.draft = Draft::from_tag(tag);
        }
    });
    (page, if save { edit } else { None })
}

fn field(ui: &mut Ui, label: &str, value: &mut String, multiline: bool) {
    let l = ui.label(label);
    if multiline {
        ui.add(widgets::block(value).desired_rows(2).desired_width(f32::INFINITY)).labelled_by(l.id);
    } else {
        ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY)).labelled_by(l.id);
    }
}
