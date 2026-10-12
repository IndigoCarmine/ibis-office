// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 差し込み印刷 (Word's 差し込み文書): the sidebar panel with the 宛先一覧 (typed, pasted or read from a CSV
//! / TSV / Excel file), the 差し込みフィールド to put at the caret, the 結果のプレビュー of one record after
//! another, and 個々のドキュメントの編集 (a new file with one copy of the letter for each record).

use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::{Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::merge::{merge_field_instruction, merge_field_placeholder, merge_fields_used, merged_letters, parse_table};
use ibis_doc_model::{FlowCommand, FlowPos, FlowRunKind};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// The merge's data and what it shows.
pub(crate) struct MergeState {
    /// The 宛先一覧 as text: the first line the field names (comma, tab or semicolon separated).
    pub(crate) text: Entity<TextareaState>,
    pub(crate) fields: Vec<String>,
    pub(crate) records: Vec<Vec<String>>,
    /// The record the preview shows.
    pub(crate) current: usize,
    pub(crate) preview: bool,
    /// 宛先の選択: the records left out are `false` (by record).
    pub(crate) included: Vec<bool>,
    /// 並べ替え: the field and whether ascending.
    pub(crate) sort: Option<(usize, bool)>,
    /// 抽出: the field, whether the value is part of the field's text (else equal to it), and the value.
    pub(crate) filter: Option<(usize, bool, String)>,
    pub(crate) sort_field: Entity<InputState>,
    pub(crate) filter_field: Entity<InputState>,
}

impl MergeState {
    /// The records merged, as indices into `records`: those chosen (宛先の選択) that pass the filter (宛先の抽出),
    /// in the order of the sort (宛先の並べ替え).
    pub(crate) fn selected(&self) -> Vec<usize> {
        let mut out: Vec<usize> = (0..self.records.len())
            .filter(|&i| self.included.get(i).copied().unwrap_or(true))
            .filter(|&i| match &self.filter {
                Some((field, contains, value)) => {
                    let text = self.records[i].get(*field).map(String::as_str).unwrap_or("");
                    if *contains { text.contains(value.as_str()) } else { text == value }
                }
                None => true,
            })
            .collect();
        if let Some((field, ascending)) = self.sort {
            let key = |i: &usize| self.records[*i].get(field).cloned().unwrap_or_default();
            out.sort_by(|a, b| {
                // Numbers by value, text by characters.
                let (x, y) = (key(a), key(b));
                let order = match (x.trim().parse::<f64>(), y.trim().parse::<f64>()) {
                    (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                    _ => x.cmp(&y),
                };
                if ascending { order } else { order.reverse() }
            });
        }
        out
    }
}

impl SlideEditor {
    fn merge_state(&self) -> Option<&MergeState> {
        self.doc_state()?.merge.as_ref()
    }

    /// ツール > 差し込み印刷…: the panel (its data kept while the document is open).
    pub(crate) fn open_merge(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.doc_state().is_some_and(|s| s.merge.is_none()) {
            let text = cx.new(|cx| TextareaState::new(window, cx));
            if let Some(state) = self.doc_state_mut() {
                let sort_field = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.merge-panel.field-sort")));
                let filter_field = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.merge-panel.field-value-contains-field-value")));
                state.merge = Some(MergeState { text, fields: Vec::new(), records: Vec::new(), current: 0, preview: false, included: Vec::new(), sort: None, filter: None, sort_field, filter_field });
            }
        }
        if let Some(state) = self.doc_state_mut() {
            state.find = None;
            state.link = None;
            state.style_edit = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.picture_edit = None;
            state.table_size = None;
            state.comments = None;
            state.ruby = None;
            state.text_box = None;
            state.helper = Default::default();
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// The panel's text read as the 宛先一覧: the first line its field names, the others its records.
    pub(crate) fn apply_merge_data(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.merge_state().map(|m| m.text.read(cx).value().to_string()) else { return };
        let (fields, records) = parse_table(&text);
        self.set_merge_data(fields, records, cx);
    }

    /// The 宛先一覧: `fields` and the `records` (rows of values).
    pub(crate) fn set_merge_data(&mut self, fields: Vec<String>, records: Vec<Vec<String>>, cx: &mut Context<Self>) {
        let preview = self.merge_state().is_some_and(|m| m.preview);
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.fields = fields;
            m.included = vec![true; records.len()];
            m.records = records;
            m.sort = None;
            m.filter = None;
            m.current = 0;
        }
        if preview {
            self.sync_merge_preview(cx);
        }
        cx.notify();
    }

    /// 宛先の選択: the records of a CSV, TSV, text or Excel file (its first sheet, the first row the field names).
    pub(crate) fn pick_merge_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some(crate::shell::i18n::text("_shell.open")) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update_in(cx, |this, window, cx| this.read_merge_file(&path, window, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        let _ = (window, cx);
    }

    /// The 宛先一覧 from the file at `path`.
    pub(crate) fn read_merge_file(&mut self, path: &std::path::Path, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(data) = std::fs::read(path) else {
            self.status = Some(crate::shell::i18n::text("docx.merge-panel.cannot-read-file").into());
            return cx.notify();
        };
        let excel = path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "xlsx" | "xlsm" | "xls"));
        let text = if excel {
            match ibis_pptx::read(&data) {
                Ok(book) => String::from_utf8_lossy(&ibis_pptx::csv::sheet_to_csv(&book, 0)).into_owned(),
                Err(_) => {
                    self.status = Some(crate::shell::i18n::text("docx.merge-panel.cannot-read-file").into());
                    return cx.notify();
                }
            }
        } else {
            // Shift_JIS files (Excel's 「CSV」) when the bytes are not UTF-8.
            ibis_pptx::csv::decode(&data).unwrap_or_else(|_| String::from_utf8_lossy(&data).into_owned())
        };
        if let Some(input) = self.merge_state().map(|m| m.text.clone()) {
            input.update(cx, |a, cx| a.set_value(text, window, cx));
        }
        self.apply_merge_data(cx);
    }

    /// 差し込みフィールドの挿入: the field `name` at the caret.
    pub(crate) fn insert_merge_field(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let kind = FlowRunKind::Field { instruction: merge_field_instruction(name), shown: merge_field_placeholder(name) };
        if self.run_flow(vec![FlowCommand::InsertField { at: caret, kind }], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
        }
    }

    /// 結果のプレビュー on or off: the fields show the current record's values.
    pub(crate) fn set_merge_preview(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.preview = on;
        }
        self.sync_merge_preview(cx);
    }

    /// The previous or next record in the preview.
    pub(crate) fn step_merge_record(&mut self, delta: i64, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            let count = m.selected().len();
            if count > 0 {
                m.current = (m.current as i64 + delta).rem_euclid(count as i64) as usize;
            }
        }
        self.sync_merge_preview(cx);
    }

    /// The layout's values from the state: the current record when previewing.
    fn sync_merge_preview(&mut self, cx: &mut Context<Self>) {
        let values = self.merge_state().filter(|m| m.preview && !m.selected().is_empty()).map(|m| {
            let selected = m.selected();
            let record = &m.records[selected[m.current.min(selected.len() - 1)]];
            Arc::new(m.fields.iter().cloned().zip(record.iter().cloned()).collect::<std::collections::HashMap<String, String>>())
        });
        self.resources.merge_values = values;
        self.relayout_now(cx);
        cx.notify();
    }

    /// 宛先の選択: record `index` in or out.
    pub(crate) fn toggle_merge_record(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut())
            && let Some(flag) = m.included.get_mut(index)
        {
            *flag = !*flag;
            m.current = 0;
        }
        self.sync_merge_preview(cx);
    }

    /// 宛先の並べ替え: by the field the panel names, ascending or descending (an empty or unknown name: as the list is).
    pub(crate) fn sort_merge_records(&mut self, ascending: bool, cx: &mut Context<Self>) {
        let name = self.merge_state().map(|m| m.sort_field.read(cx).value().trim().to_string()).unwrap_or_default();
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.sort = m.fields.iter().position(|f| *f == name).map(|i| (i, ascending));
            m.current = 0;
        }
        self.sync_merge_preview(cx);
    }

    /// 宛先の抽出: `項目名=値` (the field is the value) or `項目名~値` (the field has it); empty: all.
    pub(crate) fn filter_merge_records(&mut self, cx: &mut Context<Self>) {
        let text = self.merge_state().map(|m| m.filter_field.read(cx).value().trim().to_string()).unwrap_or_default();
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.filter = text.find(['=', '~']).and_then(|at| {
                let (name, rest) = text.split_at(at);
                let contains = rest.starts_with('~');
                m.fields.iter().position(|f| f.as_str() == name.trim()).map(|i| (i, contains, rest[1..].trim().to_string()))
            });
            m.current = 0;
        }
        self.sync_merge_preview(cx);
    }

    /// 個々のドキュメントの編集: the file `path` with a copy of the main document for each record (what the fields
    /// say not used: their values), each on its own page.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn write_merged_documents(&mut self, path: &std::path::Path) -> Result<usize, String> {
        let Some(m) = self.merge_state() else { return Err(crate::shell::i18n::text("docx.merge-panel.there-no-recipient-list").to_string()) };
        if m.records.is_empty() {
            return Err(crate::shell::i18n::text("docx.merge-panel.there-no-recipient-list").to_string());
        }
        let selected = m.selected();
        if selected.is_empty() {
            return Err(crate::shell::i18n::text("docx.merge-panel.there-no-recipient-list").to_string());
        }
        let (fields, records) = (m.fields.clone(), selected.iter().map(|&i| m.records[i].clone()).collect::<Vec<_>>());
        let source = self.source.clone().ok_or_else(|| crate::shell::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string())?;
        let doc = self.flow().ok_or_else(|| crate::shell::i18n::text("docx.merge-panel.there-no-document").to_string())?;
        let merged = merged_letters(doc, &fields, &records);
        let mut presentation = self.presentation().clone();
        presentation.flow = Some(Arc::new(merged));
        let bytes = source.save(&presentation, ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?;
        crate::shell::save::write_replacing(path, &bytes).map_err(|e| e.to_string())?;
        Ok(records.len())
    }

    /// 個々のドキュメントの編集…: choose where the merged file goes, and write it.
    pub(crate) fn save_merged_documents(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let directory = self.path.as_deref().and_then(|p| p.parent()).map(std::path::Path::to_path_buf).unwrap_or_else(|| std::env::var_os("HOME").map(Into::into).unwrap_or_else(|| ".".into()));
            let name = self.file_name.as_ref().map(|n| crate::shell::i18n::format("_docx.merged-result-n", &[("n", &n)])).unwrap_or_else(|| crate::shell::i18n::text("_docx.merged-result-docx").to_string());
            let picked = cx.prompt_for_new_path(&directory, Some(&name));
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(path))) = picked.await else { return };
                this.update(cx, |this, cx| {
                    this.status = Some(match this.write_merged_documents(&path) {
                        Ok(n) => crate::shell::i18n::format("merge.done", &[("n", &n)]).into(),
                        Err(e) => crate::shell::i18n::format("file.save-failed", &[("error", &e)]).into(),
                    });
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        let _ = (window, cx);
    }

    pub(crate) fn merge_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let m = self.merge_state()?;
        let selected = m.selected();
        let (text, fields, count, current, preview) = (m.text.clone(), m.fields.clone(), selected.len(), m.current, m.preview);
        let (sort_field, filter_field) = (m.sort_field.clone(), m.filter_field.clone());
        let record_rows: Vec<AnyElement> = m.records.iter().enumerate().take(50).map(|(i, record)| {
            let on = m.included.get(i).copied().unwrap_or(true);
            let shown = selected.contains(&i);
            let label = format!("{} {} {}", if on { "☑" } else { "☐" }, i + 1, record.iter().take(3).cloned().collect::<Vec<_>>().join(" / "));
            let _ = shown;
            Button::new(("merge-record", i)).small().ghost().label(label).on_click(cx.listener(move |this, _, _, cx| this.toggle_merge_record(i, cx))).into_any_element()
        }).collect();
        let used = self.flow().map(|d| merge_fields_used(d)).unwrap_or_default();
        let field_buttons: Vec<AnyElement> = fields
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let name = name.clone();
                let label = if used.contains(&name) { format!("«{name}» ✓") } else { format!("«{name}»") };
                Button::new(("merge-field", i)).small().ghost().label(label).on_click(cx.listener(move |this, _, _, cx| this.insert_merge_field(&name, cx))).into_any_element()
            })
            .collect();
        let status = if count == 0 { crate::shell::i18n::text("docx.merge-panel.no-recipients").to_string() } else { format!("{} / {}", current + 1, count) };
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .child(div().text_sm().child(crate::shell::i18n::text("docx.merge-panel.recipient-list-first-line-field")))
            .child(Textarea::new(&text).h(px(140.)))
            .child(
                h_flex()
                    .gap_1()
                    .child(Button::new("merge-apply").primary().small().label(crate::shell::i18n::text("_shell.apply")).on_click(cx.listener(|this, _, _, cx| this.apply_merge_data(cx))))
                    .child(Button::new("merge-file").small().ghost().label(crate::shell::i18n::text("_shell.from-file")).on_click(cx.listener(|this, _, window, cx| this.pick_merge_file(window, cx)))),
            )
            .child(div().text_sm().child(crate::shell::i18n::text("docx.merge-panel.recipients-click-leave-out-add")))
            .child(v_flex().gap_0p5().children(record_rows))
            .child(h_flex().gap_1().child(Input::new(&sort_field).small()).child(Button::new("merge-sort-up").small().ghost().label("↑").on_click(cx.listener(|this, _, _, cx| this.sort_merge_records(true, cx)))).child(Button::new("merge-sort-down").small().ghost().label("↓").on_click(cx.listener(|this, _, _, cx| this.sort_merge_records(false, cx)))))
            .child(h_flex().gap_1().child(Input::new(&filter_field).small()).child(Button::new("merge-filter").small().ghost().label(crate::shell::i18n::text("docx.merge-panel.filter")).on_click(cx.listener(|this, _, _, cx| this.filter_merge_records(cx)))))
            .child(div().text_sm().child(crate::shell::i18n::text("docx.merge-panel.insert-merge-field")))
            .child(h_flex().flex_wrap().gap_1().children(field_buttons))
            .child(
                h_flex()
                    .gap_1()
                    .child(Button::new("merge-preview").small().label(crate::shell::i18n::text("docx.merge-panel.preview-results")).selected(preview).on_click(cx.listener(move |this, _, _, cx| this.set_merge_preview(!preview, cx))))
                    .child(Button::new("merge-prev").small().ghost().label("◀").on_click(cx.listener(|this, _, _, cx| this.step_merge_record(-1, cx))))
                    .child(div().text_sm().child(status))
                    .child(Button::new("merge-next").small().ghost().label("▶").on_click(cx.listener(|this, _, _, cx| this.step_merge_record(1, cx)))),
            )
            .child(Button::new("merge-finish").primary().small().label(crate::shell::i18n::text("docx.merge-panel.create-individual-documents")).on_click(cx.listener(|this, _, window, cx| this.save_merged_documents(window, cx))));
        Some(SidebarPanel { title: crate::shell::i18n::text("docx.merge-panel.mail-merge"), body: body.into_any_element() })
    }
}
