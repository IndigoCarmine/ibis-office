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
use ibis_doc_model::merge::{
    EnvelopePreset, LabelPreset, MergePrintRange, envelope_blocks, label_sheet_table, merge_field_instruction,
    merge_field_placeholder, merge_fields_used, merged_letters, parse_table,
};
use ibis_doc_model::{Container, FlowCommand, FlowPos, FlowRunKind};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// 差し込み印刷のエラーチェック結果.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MergeCheckResult {
    pub(crate) recipient_count: usize,
    pub(crate) fields_used: Vec<String>,
    pub(crate) missing_fields: Vec<String>,
    pub(crate) empty_record_indices: Vec<usize>,
}

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
    /// 抽出: the conditions (`項目名=値 AND 項目名~値 OR …`).
    pub(crate) filter: Option<ibis_doc_model::merge::RecordFilter>,
    /// The page of the recipient list shown (`RECORDS_PER_PAGE` records on each).
    pub(crate) list_page: usize,
    pub(crate) sort_field: Entity<InputState>,
    pub(crate) filter_field: Entity<InputState>,
    /// 宛先の追加: the name of a field to add; the record number to go to; the text to find a recipient by.
    pub(crate) new_field: Entity<InputState>,
    pub(crate) goto_field: Entity<InputState>,
    pub(crate) find_field: Entity<InputState>,
    /// 規則（IF など）: the instruction of a field to insert (`IF { MERGEFIELD 年齢 } >= 20 "成人" "未成年"`).
    pub(crate) rule_field: Entity<InputState>,
    /// 封筒サイズと宛名配置 (DOCX-MERGE-016).
    pub(crate) envelope_preset: EnvelopePreset,
    pub(crate) sender_field: Entity<InputState>,
    /// ラベル規格と差込ラベル (DOCX-MERGE-017).
    pub(crate) label_preset: LabelPreset,
    /// フィールドと列の対応 (DOCX-MERGE-009).
    pub(crate) match_fields_open: bool,
    /// 差し込み印刷の印刷範囲 (DOCX-MERGE-014).
    pub(crate) print_range: MergePrintRange,
    pub(crate) print_from_field: Entity<InputState>,
    pub(crate) print_to_field: Entity<InputState>,
}

/// How many records of the list the panel shows at once.
pub(crate) const RECORDS_PER_PAGE: usize = 50;

impl MergeState {
    /// The records merged, as indices into `records`: those chosen (宛先の選択) that pass the filter (宛先の抽出),
    /// in the order of the sort (宛先の並べ替え).
    pub(crate) fn selected(&self) -> Vec<usize> {
        let mut out: Vec<usize> = (0..self.records.len())
            .filter(|&i| self.included.get(i).copied().unwrap_or(true))
            .filter(|&i| self.filter.as_ref().is_none_or(|f| f.matches(&self.records[i])))
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
                let rule_field = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.merge-panel.rule-placeholder")));
                let new_field = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.merge-panel.new-field-name")));
                let goto_field = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.merge-panel.go-to-number")));
                let find_field = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.merge-panel.find-recipient-placeholder")));
                let sender_field = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.merge-panel.sender-address")));
                let print_from_field = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.merge-panel.print-from")));
                let print_to_field = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.merge-panel.print-to")));
                state.merge = Some(MergeState {
                    text,
                    fields: Vec::new(),
                    records: Vec::new(),
                    current: 0,
                    preview: false,
                    included: Vec::new(),
                    sort: None,
                    filter: None,
                    list_page: 0,
                    sort_field,
                    filter_field,
                    new_field,
                    goto_field,
                    find_field,
                    rule_field,
                    envelope_preset: EnvelopePreset::Chou3,
                    sender_field,
                    label_preset: LabelPreset::A4_24,
                    match_fields_open: false,
                    print_range: MergePrintRange::All,
                    print_from_field,
                    print_to_field,
                });
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
            m.list_page = 0;
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
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: Vec::new(), multiple: false, prompt: Some(crate::shell::i18n::text("_shell.open")) });
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

    /// 差し込み印刷の開始: the kind of the main document (`formLetters`, `email`, `fax`, `envelopes`, `mailingLabels`, `catalog`; `None`:
    /// an ordinary document), written to the file's settings (`w:mailMerge/w:mainDocumentType`).
    pub(crate) fn set_mail_merge_type(&mut self, kind: Option<&str>, cx: &mut Context<Self>) {
        if self.flow().is_some_and(|d| d.mail_merge_type.as_deref() == kind) {
            return;
        }
        self.run_flow(vec![FlowCommand::SetMailMergeType(kind.map(str::to_string))], cx);
    }

    /// 封筒の設定 (DOCX-MERGE-016): 封筒サイズと宛名配置を設定する
    pub(crate) fn setup_envelope(&mut self, preset: EnvelopePreset, return_address: Option<&str>, cx: &mut Context<Self>) {
        let (format, body_len, first_id) = {
            let fields = self.merge_state().map(|m| m.fields.clone()).unwrap_or_default();
            let mapping = self.flow().map(|d| d.mail_merge_field_map.clone()).unwrap_or_default();
            let format = ibis_doc_model::merge::default_address_format_with_mapping(&fields, &mapping);
            let len = self.flow().map_or(0, |d| d.body.len());
            let first_id = self.flow().and_then(|d| d.body.first()).map(|b| match b {
                ibis_doc_model::flow::Block::Paragraph(p) => p.id,
                ibis_doc_model::flow::Block::Table(t) => t.id,
            });
            (format, len, first_id)
        };
        let mut next_id = self.flow().map_or(1, |d| d.new_id());
        let mut blocks = envelope_blocks(preset, return_address, &format, &mut next_id);
        if let Some(fid) = first_id && let Some(ibis_doc_model::flow::Block::Paragraph(p)) = blocks.first_mut() {
            p.id = fid;
        }
        let commands = vec![
            FlowCommand::SetMailMergeType(Some("envelopes".to_string())),
            FlowCommand::SetPage { page: preset.page_setup() },
            FlowCommand::ReplaceBlocks { container: Container::Body, index: 0, remove: body_len, blocks },
        ];
        self.run_flow(commands, cx);
    }

    /// ラベル規格と差込ラベル (DOCX-MERGE-017): ラベル規格に応じた表グリッドと NEXT/ADDRESSBLOCK フィールドを生成する
    pub(crate) fn setup_labels(&mut self, preset: LabelPreset, cx: &mut Context<Self>) {
        let (format, body_len, first_id) = {
            let fields = self.merge_state().map(|m| m.fields.clone()).unwrap_or_default();
            let mapping = self.flow().map(|d| d.mail_merge_field_map.clone()).unwrap_or_default();
            let format = ibis_doc_model::merge::default_address_format_with_mapping(&fields, &mapping);
            let len = self.flow().map_or(0, |d| d.body.len());
            let first_id = self.flow().and_then(|d| d.body.first()).map(|b| match b {
                ibis_doc_model::flow::Block::Paragraph(p) => p.id,
                ibis_doc_model::flow::Block::Table(t) => t.id,
            });
            (format, len, first_id)
        };
        let mut next_id = self.flow().map_or(1, |d| d.new_id());
        let table = label_sheet_table(preset, &format, &mut next_id);
        let trailing_p = ibis_doc_model::FlowParagraph {
            id: first_id.unwrap_or_else(|| { let id = next_id; next_id += 1; id }),
            mark: ibis_doc_model::flow::FlowRunStyle {
                size: 200,
                ..ibis_doc_model::flow::FlowRunStyle::default()
            },
            ..ibis_doc_model::FlowParagraph::default()
        };
        let blocks = vec![
            ibis_doc_model::flow::Block::Table(table),
            ibis_doc_model::flow::Block::Paragraph(trailing_p),
        ];
        let commands = vec![
            FlowCommand::SetMailMergeType(Some("mailingLabels".to_string())),
            FlowCommand::SetPage { page: preset.page_setup() },
            FlowCommand::ReplaceBlocks { container: Container::Body, index: 0, remove: body_len, blocks },
        ];
        self.run_flow(commands, cx);
    }

    /// 差し込みフィールドの挿入: the field `name` at the caret.
    pub(crate) fn insert_merge_field(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let kind = FlowRunKind::Field { instruction: merge_field_instruction(name), shown: merge_field_placeholder(name) };
        if self.run_flow(vec![FlowCommand::InsertField { at: caret, kind }], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
        }
    }

    /// 住所ブロック / 挨拶文の挿入: a block of the fields the recipient list has, at the caret.
    pub(crate) fn insert_merge_block(&mut self, address: bool, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let Some(fields) = self.merge_state().map(|m| m.fields.clone()) else { return };
        let (instruction, shown) = if address {
            let mapping = self.flow().map(|d| d.mail_merge_field_map.as_slice()).unwrap_or(&[]);
            (ibis_doc_model::merge::address_block_instruction(&ibis_doc_model::merge::default_address_format_with_mapping(&fields, mapping)), crate::shell::i18n::text("docx.merge-panel.address-block-shown"))
        } else {
            let mapping = self.flow().map(|d| d.mail_merge_field_map.as_slice()).unwrap_or(&[]);
            (ibis_doc_model::merge::greeting_line_instruction(&ibis_doc_model::merge::default_greeting_format_with_mapping(&fields, mapping)), crate::shell::i18n::text("docx.merge-panel.greeting-line-shown"))
        };
        let kind = FlowRunKind::Field { instruction, shown: shown.to_string() };
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
        let field_map = self.flow().map(|d| d.mail_merge_field_map.clone()).unwrap_or_default();
        let values = self.merge_state().filter(|m| m.preview && !m.selected().is_empty()).map(|m| {
            let selected = m.selected();
            let record = &m.records[selected[m.current.min(selected.len() - 1)]];
            let mut map: std::collections::HashMap<String, String> = m.fields.iter().cloned().zip(record.iter().cloned()).collect();
            for fmd in &field_map {
                if let Some(val) = map.get(&fmd.name).cloned() {
                    map.entry(fmd.mapped_name.clone()).or_insert(val);
                }
            }
            Arc::new(map)
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

    /// 宛先の抽出: conditions `項目名=値`, `!=`, `~` (has), `!~`, `<`, `<=`, `>`, `>=`, joined with AND・OR (`merge::parse_filter`); empty: all.
    pub(crate) fn filter_merge_records(&mut self, cx: &mut Context<Self>) {
        let text = self.merge_state().map(|m| m.filter_field.read(cx).value().trim().to_string()).unwrap_or_default();
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.filter = ibis_doc_model::merge::parse_filter(&text, &m.fields);
            m.current = 0;
        }
        self.sync_merge_preview(cx);
    }

    /// The panel's text made again from the lists (after a record or a field is added or taken away).
    fn write_merge_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((input, text)) = self.merge_state().map(|m| (m.text.clone(), ibis_doc_model::merge::serialize_table(&m.fields, &m.records))) else { return };
        input.update(cx, |a, cx| a.set_value(text, window, cx));
    }

    /// 宛先の追加: an empty record at the end of the list.
    pub(crate) fn add_merge_record(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) else { return };
        if m.fields.is_empty() {
            return;
        }
        m.records.push(vec![String::new(); m.fields.len()]);
        m.included.push(true);
        m.list_page = (m.records.len() - 1) / RECORDS_PER_PAGE;
        self.write_merge_text(window, cx);
        self.sync_merge_preview(cx);
    }

    /// 宛先の削除: record `index` taken out of the list.
    pub(crate) fn delete_merge_record(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) else { return };
        if index >= m.records.len() {
            return;
        }
        m.records.remove(index);
        if index < m.included.len() {
            m.included.remove(index);
        }
        m.current = 0;
        m.list_page = m.list_page.min(m.records.len().saturating_sub(1) / RECORDS_PER_PAGE);
        self.write_merge_text(window, cx);
        self.sync_merge_preview(cx);
    }

    /// 項目の追加: a field `name` (empty in every record) at the end; the sort and the filter are cleared (they name fields by place).
    pub(crate) fn add_merge_field(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let name = name.trim();
        let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) else { return };
        if name.is_empty() || m.fields.iter().any(|f| f == name) {
            return;
        }
        m.fields.push(name.to_string());
        for r in &mut m.records {
            r.push(String::new());
        }
        m.sort = None;
        m.filter = None;
        let input = m.new_field.clone();
        input.update(cx, |a, cx| a.set_value("", window, cx));
        self.write_merge_text(window, cx);
        self.sync_merge_preview(cx);
    }

    /// 項目の削除: field `index` taken out of the list and of every record.
    pub(crate) fn delete_merge_field(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) else { return };
        if index >= m.fields.len() {
            return;
        }
        m.fields.remove(index);
        for r in &mut m.records {
            if index < r.len() {
                r.remove(index);
            }
        }
        m.sort = None;
        m.filter = None;
        self.write_merge_text(window, cx);
        self.sync_merge_preview(cx);
    }

    /// 宛先の選択: every record in (`on`) or out.
    pub(crate) fn set_all_merge_records(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.included = vec![on; m.records.len()];
            m.current = 0;
        }
        self.sync_merge_preview(cx);
    }

    /// The page of the list shown, `delta` pages on.
    pub(crate) fn step_merge_page(&mut self, delta: i64, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            let pages = m.records.len().div_ceil(RECORDS_PER_PAGE).max(1) as i64;
            m.list_page = (m.list_page as i64 + delta).clamp(0, pages - 1) as usize;
        }
        cx.notify();
    }

    /// The preview at the first (`0`) or last record, or at record number `n` (1 the first) of those merged.
    pub(crate) fn goto_merge_record(&mut self, n: usize, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            let count = m.selected().len();
            if count > 0 {
                m.current = n.saturating_sub(1).min(count - 1);
            }
        }
        self.sync_merge_preview(cx);
    }

    /// 宛先の検索: the preview goes to the next record (after the one shown, round again) one of whose values has the panel's text.
    pub(crate) fn find_merge_record(&mut self, cx: &mut Context<Self>) -> bool {
        let text = self.merge_state().map(|m| m.find_field.read(cx).value().trim().to_string()).unwrap_or_default();
        let found = self.merge_state().and_then(|m| {
            let selected = m.selected();
            if text.is_empty() || selected.is_empty() {
                return None;
            }
            (1..=selected.len()).map(|k| (m.current + k) % selected.len()).find(|&at| m.records[selected[at]].iter().any(|v| v.contains(text.as_str())))
        });
        let Some(at) = found else {
            self.status = Some(crate::shell::i18n::text("docx.merge-panel.recipient-not-found").into());
            cx.notify();
            return false;
        };
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.current = at;
        }
        self.sync_merge_preview(cx);
        true
    }

    /// フィールドと列の対応の表示切り替え (DOCX-MERGE-009).
    pub(crate) fn toggle_match_fields(&mut self, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.match_fields_open = !m.match_fields_open;
            cx.notify();
        }
    }

    /// フィールドと列の対応 (DOCX-MERGE-009): 宛先一覧の列名から標準差し込みフィールドを自動対応付けする
    pub(crate) fn auto_match_merge_fields(&mut self, cx: &mut Context<Self>) {
        let fields = self.merge_state().map(|m| m.fields.clone()).unwrap_or_default();
        let map = ibis_doc_model::merge::auto_match_fields(&fields);
        if self.run_flow(vec![FlowCommand::SetMailMergeFieldMap(map)], cx) {
            self.sync_merge_preview(cx);
        }
    }

    /// フィールドと列の対応 (DOCX-MERGE-009): フィールドと列の対応をクリアする
    pub(crate) fn clear_merge_field_map(&mut self, cx: &mut Context<Self>) {
        if self.run_flow(vec![FlowCommand::SetMailMergeFieldMap(Vec::new())], cx) {
            self.sync_merge_preview(cx);
        }
    }

    /// フィールドと列の対応 (DOCX-MERGE-009): 指定の標準フィールドに対応する列を次へ切り替える（未対応 -> 列0 -> 列1 -> ... -> 未対応）
    pub(crate) fn cycle_merge_field_mapping(&mut self, standard_name: &str, cx: &mut Context<Self>) {
        let fields = self.merge_state().map(|m| m.fields.clone()).unwrap_or_default();
        if fields.is_empty() {
            return;
        }
        let current_map = self.flow().map(|d| d.mail_merge_field_map.clone()).unwrap_or_default();
        let current_mapped = current_map.iter().find(|m| m.mapped_name == standard_name).map(|m| m.name.as_str());
        let next_mapped: Option<&str> = match current_mapped {
            None => fields.first().map(|s| s.as_str()),
            Some(curr) => {
                if let Some(pos) = fields.iter().position(|f| f == curr) {
                    if pos + 1 < fields.len() {
                        Some(fields[pos + 1].as_str())
                    } else {
                        None
                    }
                } else {
                    fields.first().map(|s| s.as_str())
                }
            }
        };
        let mut new_map: Vec<ibis_doc_model::merge::FieldMapData> = current_map.into_iter().filter(|m| m.mapped_name != standard_name).collect();
        if let Some(col_name) = next_mapped {
            let col_idx = fields.iter().position(|f| f == col_name).unwrap_or(0);
            new_map.push(ibis_doc_model::merge::FieldMapData {
                name: col_name.to_string(),
                mapped_name: standard_name.to_string(),
                column: col_idx,
            });
        }
        new_map.sort_by(|a, b| a.mapped_name.cmp(&b.mapped_name));
        if self.run_flow(vec![FlowCommand::SetMailMergeFieldMap(new_map)], cx) {
            self.sync_merge_preview(cx);
        }
    }

    /// フィールドと列の対応 (DOCX-MERGE-009): 指定の標準フィールドに対応する列を明示設定する
    #[allow(dead_code)]
    pub(crate) fn set_merge_field_mapping(&mut self, standard_name: &str, mapped_column: Option<&str>, cx: &mut Context<Self>) {
        let fields = self.merge_state().map(|m| m.fields.clone()).unwrap_or_default();
        let current_map = self.flow().map(|d| d.mail_merge_field_map.clone()).unwrap_or_default();
        let mut new_map: Vec<ibis_doc_model::merge::FieldMapData> = current_map.into_iter().filter(|m| m.mapped_name != standard_name).collect();
        if let Some(col_name) = mapped_column {
            let col_idx = fields.iter().position(|f| f == col_name).unwrap_or(0);
            new_map.push(ibis_doc_model::merge::FieldMapData {
                name: col_name.to_string(),
                mapped_name: standard_name.to_string(),
                column: col_idx,
            });
        }
        new_map.sort_by(|a, b| a.mapped_name.cmp(&b.mapped_name));
        if self.run_flow(vec![FlowCommand::SetMailMergeFieldMap(new_map)], cx) {
            self.sync_merge_preview(cx);
        }
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

    /// 規則の挿入: the field of the instruction in the panel's rule field (`IF`, `COMPARE`, `SKIPIF`… with `{ MERGEFIELD 項目 }` in
    /// it) at the caret; it shows «規則» until previewed.
    pub(crate) fn insert_merge_rule(&mut self, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let instruction = self.merge_state().map(|m| m.rule_field.read(cx).value().trim().to_string()).unwrap_or_default();
        if instruction.is_empty() {
            return;
        }
        let word = instruction.split_whitespace().next().unwrap_or("").to_ascii_uppercase();
        let kind = FlowRunKind::Field { instruction: format!("{instruction} "), shown: format!("«{word}»") };
        if self.run_flow(vec![FlowCommand::InsertField { at: caret, kind }], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
        }
    }

    /// 差し込み印刷のエラーチェック (Word: 差し込み文書 > エラーチェック / docx.merge-panel.check-errors).
    /// 文書内の MERGEFIELD・ADDRESSBLOCK・GREETINGLINE フィールドを宛先リストと照合し、
    /// 未定義フィールド、未読込宛先リスト、空白レコード等を検出して結果を通知する。
    pub(crate) fn check_merge_errors(&mut self, cx: &mut Context<Self>) -> MergeCheckResult {
        let Some(doc) = self.flow().cloned() else {
            let msg = crate::shell::i18n::text("docx.merge-panel.there-no-document");
            self.status = Some(msg.into());
            cx.notify();
            return MergeCheckResult {
                recipient_count: 0,
                fields_used: Vec::new(),
                missing_fields: Vec::new(),
                empty_record_indices: Vec::new(),
            };
        };

        let fields_used = ibis_doc_model::merge::merge_fields_used(&doc);
        let Some(m) = self.merge_state() else {
            let msg = crate::shell::i18n::text("docx.merge-panel.error-check.no-recipients");
            self.status = Some(msg.into());
            cx.notify();
            return MergeCheckResult {
                recipient_count: 0,
                fields_used,
                missing_fields: Vec::new(),
                empty_record_indices: Vec::new(),
            };
        };

        if m.records.is_empty() {
            let msg = crate::shell::i18n::text("docx.merge-panel.error-check.no-recipients");
            self.status = Some(msg.into());
            cx.notify();
            return MergeCheckResult {
                recipient_count: 0,
                fields_used,
                missing_fields: Vec::new(),
                empty_record_indices: Vec::new(),
            };
        }

        let recipient_count = m.records.len();
        let field_map = &doc.mail_merge_field_map;

        // Check for missing fields
        let mut missing_fields = Vec::new();
        for f in &fields_used {
            let found_in_fields = m.fields.iter().any(|col| col.eq_ignore_ascii_case(f));
            let found_in_map = field_map.iter().any(|mapping| {
                mapping.mapped_name.eq_ignore_ascii_case(f) && m.fields.iter().any(|col| col.eq_ignore_ascii_case(&mapping.name))
            });
            if !found_in_fields && !found_in_map {
                missing_fields.push(f.clone());
            }
        }

        // Check for completely empty records among selected records
        let mut empty_record_indices = Vec::new();
        for (idx, record) in m.records.iter().enumerate() {
            if m.included.get(idx).copied().unwrap_or(true) {
                if record.iter().all(|val| val.trim().is_empty()) {
                    empty_record_indices.push(idx);
                }
            }
        }

        // Status message
        if !missing_fields.is_empty() {
            let list = missing_fields.join(", ");
            let msg = crate::shell::i18n::format("docx.merge-panel.error-check.missing-fields", &[("fields", &list)]);
            self.status = Some(msg.into());
        } else if !empty_record_indices.is_empty() {
            let count_str = empty_record_indices.len().to_string();
            let msg = crate::shell::i18n::format("docx.merge-panel.error-check.empty-records", &[("count", &count_str)]);
            self.status = Some(msg.into());
        } else if fields_used.is_empty() {
            let msg = crate::shell::i18n::text("docx.merge-panel.error-check.no-fields");
            self.status = Some(msg.into());
        } else {
            let count_str = recipient_count.to_string();
            let fcount_str = fields_used.len().to_string();
            let msg = crate::shell::i18n::format(
                "docx.merge-panel.error-check.passed",
                &[("count", &count_str), ("field_count", &fcount_str)],
            );
            self.status = Some(msg.into());
        }

        cx.notify();
        MergeCheckResult {
            recipient_count,
            fields_used,
            missing_fields,
            empty_record_indices,
        }
    }

    /// 宛先ごとの別のファイル: one file in `directory` for each record chosen (`{stem} 1.docx`…), its own copy of the main
    /// document; a record a `SKIPIF` leaves out has none. How many files.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn write_merged_separately(&self, directory: &std::path::Path, stem: &str) -> Result<usize, String> {
        let Some(m) = self.merge_state() else { return Err(crate::shell::i18n::text("docx.merge-panel.there-no-recipient-list").to_string()) };
        let selected = m.selected();
        if selected.is_empty() {
            return Err(crate::shell::i18n::text("docx.merge-panel.there-no-recipient-list").to_string());
        }
        let (fields, records) = (m.fields.clone(), selected.iter().map(|&i| m.records[i].clone()).collect::<Vec<_>>());
        let source = self.source.clone().ok_or_else(|| crate::shell::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string())?;
        let doc = self.flow().ok_or_else(|| crate::shell::i18n::text("docx.merge-panel.there-no-document").to_string())?.clone();
        let mut count = 0;
        for record in &records {
            if ibis_doc_model::merge::record_skipped(&doc, &fields, record) {
                continue;
            }
            count += 1;
            let merged = merged_letters(&doc, &fields, std::slice::from_ref(record));
            let mut presentation = self.presentation().clone();
            presentation.flow = Some(Arc::new(merged));
            let bytes = source.save(&presentation, ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?;
            crate::shell::save::write_replacing(&directory.join(format!("{stem} {count}.docx")), &bytes).map_err(|e| e.to_string())?;
        }
        Ok(count)
    }

    /// 宛先ごとに別のファイルで作成…: choose the folder, and write the files.
    pub(crate) fn save_merged_separately(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, extensions: Vec::new(), multiple: false, prompt: None });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(directory) = paths.into_iter().next() else { return };
                this.update(cx, |this, cx| {
                    let stem = this.file_name.as_ref().map(|n| n.trim_end_matches(".docx").to_string()).unwrap_or_else(|| crate::shell::i18n::text("_docx.merged-result-stem").to_string());
                    this.status = Some(match this.write_merged_separately(&directory, &stem) {
                        Ok(n) => crate::shell::i18n::format("merge.done", &[("n", &n.to_string())]).into(),
                        Err(e) => e.into(),
                    });
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = (window, cx);
        }
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

    /// 差し込み印刷の印刷範囲を設定する (DOCX-MERGE-014).
    pub(crate) fn set_merge_print_range(&mut self, range: ibis_doc_model::merge::MergePrintRange, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.print_range = range;
            cx.notify();
        }
    }

    /// 現在選択されている印刷範囲を解決する (DOCX-MERGE-014).
    pub(crate) fn current_merge_print_range(&self, cx: &App) -> ibis_doc_model::merge::MergePrintRange {
        let Some(m) = self.merge_state() else { return ibis_doc_model::merge::MergePrintRange::All; };
        match m.print_range {
            ibis_doc_model::merge::MergePrintRange::All => ibis_doc_model::merge::MergePrintRange::All,
            ibis_doc_model::merge::MergePrintRange::Current => ibis_doc_model::merge::MergePrintRange::Current,
            ibis_doc_model::merge::MergePrintRange::FromTo(_, _) => {
                let from_str = m.print_from_field.read(cx).value().trim().to_string();
                let to_str = m.print_to_field.read(cx).value().trim().to_string();
                let start = from_str.parse::<usize>().unwrap_or(1);
                let end = to_str.parse::<usize>().unwrap_or(usize::MAX);
                ibis_doc_model::merge::MergePrintRange::FromTo(start, end)
            }
        }
    }

    /// 差し込み印刷用の FlowDocument を生成する (DOCX-MERGE-014).
    pub(crate) fn merged_document_for_range(&self, range: ibis_doc_model::merge::MergePrintRange) -> Result<(ibis_doc_model::FlowDocument, usize), String> {
        let Some(m) = self.merge_state() else {
            return Err(crate::shell::i18n::text("docx.merge-panel.there-no-recipient-list").to_string());
        };
        let selected = m.selected();
        if selected.is_empty() {
            return Err(crate::shell::i18n::text("docx.merge-panel.there-no-recipient-list").to_string());
        }
        let (fields, records) = (m.fields.clone(), selected.iter().map(|&i| m.records[i].clone()).collect::<Vec<_>>());
        let doc = self.flow().ok_or_else(|| crate::shell::i18n::text("docx.merge-panel.there-no-document").to_string())?;
        ibis_doc_model::merge::merged_letters_for_range(doc, &fields, &records, range, m.current)
    }

    /// 差し込み印刷の PDF を書き出す (DOCX-MERGE-014).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn write_merged_pdf(&self, pdf_path: &std::path::Path, range: ibis_doc_model::merge::MergePrintRange) -> Result<usize, String> {
        let Some(m) = self.merge_state() else {
            return Err(crate::shell::i18n::text("docx.merge-panel.there-no-recipient-list").to_string());
        };
        let selected = m.selected();
        if selected.is_empty() {
            return Err(crate::shell::i18n::text("docx.merge-panel.there-no-recipient-list").to_string());
        }
        let (fields, records) = (m.fields.clone(), selected.iter().map(|&i| m.records[i].clone()).collect::<Vec<_>>());
        let source = self.source.clone().ok_or_else(|| crate::shell::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string())?;
        let doc = self.flow().ok_or_else(|| crate::shell::i18n::text("docx.merge-panel.there-no-document").to_string())?;
        let (merged, count) = ibis_doc_model::merge::merged_letters_for_range(doc, &fields, &records, range, m.current)?;

        let mut presentation = self.presentation().clone();
        presentation.flow = Some(Arc::new(merged));
        let docx_bytes = source.save(&presentation, ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?;

        let dir = std::env::temp_dir().join(format!("ibisoffice-merge-pdf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let out = dir.join("out");
        std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        let temp_docx = dir.join("merged.docx");
        std::fs::write(&temp_docx, &docx_bytes).map_err(|e| e.to_string())?;

        let program = crate::shell::file_actions::export_program()?;
        let mut cmd = std::process::Command::new(program);
        cmd.args(["--export", "pdf", "--out-dir"]).arg(&out).arg(&temp_docx);
        let status = cmd.status().map_err(|e| e.to_string())?;
        if !status.success() {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(format!("PDF export failed with status: {status}"));
        }
        let made = std::fs::read_dir(&out)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
            .ok_or_else(|| "Could not find exported PDF".to_string())?;
        let data = std::fs::read(&made).map_err(|e| e.to_string())?;
        crate::shell::save::write_replacing(pdf_path, &data).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir_all(&dir);
        Ok(count)
    }

    /// 差し込み印刷を実行する（macOS は Preview の印刷ダイアログ、DOCX-MERGE-014）.
    pub(crate) fn print_merged_documents(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let range = self.current_merge_print_range(cx);
            let temp_pdf = std::env::temp_dir().join(format!("ibisoffice-print-merge-{}.pdf", std::process::id()));
            match self.write_merged_pdf(&temp_pdf, range) {
                Ok(count) => {
                    #[cfg(target_os = "macos")]
                    if std::env::var("IBIS_HEADLESS").is_err() && !std::env::args().any(|a| a.contains("test") || a.contains("scenarios")) {
                        let script = format!(
                            "tell application \"Preview\"\nactivate\nopen POSIX file \"{}\"\nprint front document with print dialog\nend tell",
                            temp_pdf.display().to_string().replace('"', "\\\"")
                        );
                        let _ = std::process::Command::new("osascript").args(["-e", &script]).output();
                    }
                    self.status = Some(crate::shell::i18n::format("docx.merge.printed-count", &[("n", &count.to_string())]).into());
                    cx.notify();
                }
                Err(e) => {
                    self.status = Some(crate::shell::i18n::format("docx.merge.print-failed", &[("error", &e)]).into());
                    cx.notify();
                }
            }
        }
        #[cfg(target_family = "wasm")]
        let _ = cx;
    }

    /// 差し込み印刷を PDF に書き出す (DOCX-MERGE-014).
    pub(crate) fn export_merged_pdf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let range = self.current_merge_print_range(cx);
            let directory = self.path.as_deref().and_then(|p| p.parent()).map(std::path::Path::to_path_buf).unwrap_or_else(|| std::env::var_os("HOME").map(Into::into).unwrap_or_else(|| ".".into()));
            let name = self.file_name.as_ref().map(|n| format!("{}-merged.pdf", n.trim_end_matches(".docx"))).unwrap_or_else(|| "merged.pdf".to_string());
            let picked = cx.prompt_for_new_path(&directory, Some(&name));
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(path))) = picked.await else { return };
                this.update(cx, |this, cx| {
                    this.status = Some(crate::shell::i18n::text("docx.merge.printing").into());
                    cx.notify();
                    let result = this.write_merged_pdf(&path, range);
                    this.status = Some(match result {
                        Ok(n) => crate::shell::i18n::format("docx.merge.exported-pdf-count", &[("n", &n.to_string())]).into(),
                        Err(e) => crate::shell::i18n::format("docx.merge.pdf-failed", &[("error", &e)]).into(),
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
        let (sort_field, filter_field, rule_field) = (m.sort_field.clone(), m.filter_field.clone(), m.rule_field.clone());
        let (new_field, goto_field, find_field) = (m.new_field.clone(), m.goto_field.clone(), m.find_field.clone());
        let (print_range, print_from_field, print_to_field) = (m.print_range, m.print_from_field.clone(), m.print_to_field.clone());
        let (page, total) = (m.list_page, m.records.len());
        let record_rows: Vec<AnyElement> = m.records.iter().enumerate().skip(page * RECORDS_PER_PAGE).take(RECORDS_PER_PAGE).map(|(i, record)| {
            let on = m.included.get(i).copied().unwrap_or(true);
            let label = format!("{} {} {}", if on { "☑" } else { "☐" }, i + 1, record.iter().take(3).cloned().collect::<Vec<_>>().join(" / "));
            h_flex()
                .gap_0p5()
                .child(Button::new(("merge-record", i)).small().ghost().label(label).on_click(cx.listener(move |this, _, _, cx| this.toggle_merge_record(i, cx))))
                .child(Button::new(("merge-record-delete", i)).small().ghost().label("×").on_click(cx.listener(move |this, _, window, cx| this.delete_merge_record(i, window, cx))))
                .into_any_element()
        }).collect();
        let pages = total.div_ceil(RECORDS_PER_PAGE).max(1);
        let used = self.flow().map(|d| merge_fields_used(d)).unwrap_or_default();
        let field_buttons: Vec<AnyElement> = fields
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let name = name.clone();
                let label = if used.contains(&name) { format!("«{name}» ✓") } else { format!("«{name}»") };
                h_flex()
                    .gap_0p5()
                    .child(Button::new(("merge-field", i)).small().ghost().label(label).on_click(cx.listener(move |this, _, _, cx| this.insert_merge_field(&name, cx))))
                    .child(Button::new(("merge-field-delete", i)).small().ghost().label("×").on_click(cx.listener(move |this, _, window, cx| this.delete_merge_field(i, window, cx))))
                    .into_any_element()
            })
            .collect();
        let current_type = self.flow().and_then(|d| d.mail_merge_type.clone());
        let type_buttons: Vec<AnyElement> = [("docx.merge-panel.type-letters", Some("formLetters")), ("docx.merge-panel.type-email", Some("email")), ("docx.merge-panel.type-envelopes", Some("envelopes")), ("docx.merge-panel.type-labels", Some("mailingLabels")), ("docx.merge-panel.type-directory", Some("catalog")), ("docx.merge-panel.type-normal", None)]
            .into_iter()
            .enumerate()
            .map(|(i, (label, kind))| {
                let on = current_type.as_deref() == kind;
                Button::new(("merge-type", i)).small().ghost().label(crate::shell::i18n::text(label)).selected(on).on_click(cx.listener(move |this, _, _, cx| this.set_mail_merge_type(kind, cx))).into_any_element()
            })
            .collect();
        let envelope_section: Option<AnyElement> = if current_type.as_deref() == Some("envelopes") {
            let active_preset = m.envelope_preset;
            let sender_field = m.sender_field.clone();
            let preset_buttons: Vec<AnyElement> = EnvelopePreset::ALL.into_iter().enumerate().map(|(i, preset)| {
                let on = preset == active_preset;
                Button::new(("merge-env-preset", i))
                    .small()
                    .ghost()
                    .label(preset.label())
                    .selected(on)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(m) = this.doc_state_mut().and_then(|s| s.merge.as_mut()) {
                            m.envelope_preset = preset;
                            cx.notify();
                        }
                    }))
                    .into_any_element()
            }).collect();
            Some(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child(crate::shell::i18n::text("docx.merge-panel.envelope-setup")))
                    .child(h_flex().flex_wrap().gap_0p5().children(preset_buttons))
                    .child(Input::new(&sender_field).small())
                    .child(
                        Button::new("merge-apply-envelope")
                            .primary()
                            .small()
                            .label(crate::shell::i18n::text("docx.merge-panel.apply-envelope"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let (preset, sender) = if let Some(m) = this.merge_state() {
                                    (m.envelope_preset, m.sender_field.read(cx).value().trim().to_string())
                                } else {
                                    (EnvelopePreset::Chou3, String::new())
                                };
                                this.setup_envelope(preset, (!sender.is_empty()).then_some(&sender), cx);
                            }))
                    )
                    .into_any_element()
            )
        } else {
            None
        };
        let label_section: Option<AnyElement> = if current_type.as_deref() == Some("mailingLabels") {
            let active_preset = m.label_preset;
            let preset_buttons: Vec<AnyElement> = LabelPreset::ALL.into_iter().enumerate().map(|(i, preset)| {
                let on = preset == active_preset;
                Button::new(("merge-lbl-preset", i))
                    .small()
                    .ghost()
                    .label(preset.label())
                    .selected(on)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(m) = this.doc_state_mut().and_then(|s| s.merge.as_mut()) {
                            m.label_preset = preset;
                            cx.notify();
                        }
                    }))
                    .into_any_element()
            }).collect();
            Some(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child(crate::shell::i18n::text("docx.merge-panel.label-setup")))
                    .child(h_flex().flex_wrap().gap_0p5().children(preset_buttons))
                    .child(
                        Button::new("merge-create-labels")
                            .primary()
                            .small()
                            .label(crate::shell::i18n::text("docx.merge-panel.create-labels"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let preset = this.merge_state().map(|m| m.label_preset).unwrap_or(LabelPreset::A4_24);
                                this.setup_labels(preset, cx);
                            }))
                    )
                    .into_any_element()
            )
        } else {
            None
        };
        let match_fields_open = m.match_fields_open;
        let mapping = self.flow().map(|d| d.mail_merge_field_map.clone()).unwrap_or_default();
        let match_fields_section: Option<AnyElement> = if match_fields_open {
            let unmatched = crate::shell::i18n::text("docx.merge-panel.unmatched");
            let rows: Vec<AnyElement> = ibis_doc_model::merge::STANDARD_MERGE_FIELDS.iter().enumerate().map(|(i, std_field)| {
                let mapped = mapping.iter().find(|map| map.mapped_name == std_field.mapped_name).map(|map| map.name.as_str());
                let col_text = mapped.unwrap_or(unmatched.as_ref());
                let std_mapped_name = std_field.mapped_name;
                h_flex()
                    .gap_1()
                    .justify_between()
                    .child(div().text_sm().child(format!("{} ({})", std_field.mapped_name, std_field.label_ja)))
                    .child(
                        Button::new(("merge-map-col", i))
                            .small()
                            .ghost()
                            .label(format!("→ {col_text}"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.cycle_merge_field_mapping(std_mapped_name, cx);
                            }))
                    )
                    .into_any_element()
            }).collect();
            Some(
                v_flex()
                    .gap_0p5()
                    .p(px(8.))
                    .border_1()
                    .rounded_md()
                    .child(div().text_xs().child(crate::shell::i18n::text("docx.merge-panel.match-fields-guide")))
                    .children(rows)
                    .into_any_element()
            )
        } else {
            None
        };
        let status = if count == 0 { crate::shell::i18n::text("docx.merge-panel.no-recipients").to_string() } else { format!("{} / {}", current + 1, count) };
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .child(div().text_sm().child(crate::shell::i18n::text("docx.merge-panel.document-type")))
            .child(h_flex().flex_wrap().gap_1().children(type_buttons))
            .children(envelope_section)
            .children(label_section)
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
            .child(
                h_flex()
                    .gap_1()
                    .child(Button::new("merge-add-record").small().ghost().label(crate::shell::i18n::text("docx.merge-panel.add-recipient")).on_click(cx.listener(|this, _, window, cx| this.add_merge_record(window, cx))))
                    .child(Button::new("merge-all").small().ghost().label(crate::shell::i18n::text("docx.merge-panel.select-all")).on_click(cx.listener(|this, _, _, cx| this.set_all_merge_records(true, cx))))
                    .child(Button::new("merge-none").small().ghost().label(crate::shell::i18n::text("docx.merge-panel.select-none")).on_click(cx.listener(|this, _, _, cx| this.set_all_merge_records(false, cx))))
                    .child(Button::new("merge-page-prev").small().ghost().label("◀").on_click(cx.listener(|this, _, _, cx| this.step_merge_page(-1, cx))))
                    .child(div().text_sm().child(format!("{} / {}", page + 1, pages)))
                    .child(Button::new("merge-page-next").small().ghost().label("▶").on_click(cx.listener(|this, _, _, cx| this.step_merge_page(1, cx)))),
            )
            .child(h_flex().gap_1().child(Input::new(&new_field).small()).child(Button::new("merge-add-field").small().ghost().label(crate::shell::i18n::text("docx.merge-panel.add-field")).on_click(cx.listener(|this, _, window, cx| {
                let name = this.merge_state().map(|m| m.new_field.read(cx).value().to_string()).unwrap_or_default();
                this.add_merge_field(&name, window, cx);
            }))))
            .child(h_flex().gap_1().child(Input::new(&sort_field).small()).child(Button::new("merge-sort-up").small().ghost().label("↑").on_click(cx.listener(|this, _, _, cx| this.sort_merge_records(true, cx)))).child(Button::new("merge-sort-down").small().ghost().label("↓").on_click(cx.listener(|this, _, _, cx| this.sort_merge_records(false, cx)))))
            .child(h_flex().gap_1().child(Input::new(&filter_field).small()).child(Button::new("merge-filter").small().ghost().label(crate::shell::i18n::text("docx.merge-panel.filter")).on_click(cx.listener(|this, _, _, cx| this.filter_merge_records(cx)))))
            .child(div().text_sm().child(crate::shell::i18n::text("docx.merge-panel.insert-merge-field")))
            .child(h_flex().flex_wrap().gap_1().children(field_buttons))
            .child(h_flex().gap_1().child(Input::new(&rule_field).small()).child(Button::new("merge-rule").small().ghost().label(crate::shell::i18n::text("docx.merge-panel.insert-rule")).on_click(cx.listener(|this, _, _, cx| this.insert_merge_rule(cx)))))
            .child(
                h_flex()
                    .gap_1()
                    .child(Button::new("merge-address").small().ghost().label(crate::shell::i18n::text("docx.merge-panel.address-block")).on_click(cx.listener(|this, _, _, cx| this.insert_merge_block(true, cx))))
                    .child(Button::new("merge-greeting").small().ghost().label(crate::shell::i18n::text("docx.merge-panel.greeting-line")).on_click(cx.listener(|this, _, _, cx| this.insert_merge_block(false, cx)))),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(Button::new("merge-match-fields").small().ghost().selected(match_fields_open).label(crate::shell::i18n::text("docx.merge-panel.match-fields")).on_click(cx.listener(|this, _, _, cx| this.toggle_match_fields(cx))))
                    .child(Button::new("merge-auto-match").small().ghost().label(crate::shell::i18n::text("docx.merge-panel.auto-match")).on_click(cx.listener(|this, _, _, cx| this.auto_match_merge_fields(cx))))
                    .child(Button::new("merge-clear-match").small().ghost().label(crate::shell::i18n::text("docx.merge-panel.clear-match")).on_click(cx.listener(|this, _, _, cx| this.clear_merge_field_map(cx)))),
            )
            .children(match_fields_section)
            .child(
                h_flex()
                    .gap_1()
                    .child(Button::new("merge-preview").small().label(crate::shell::i18n::text("docx.merge-panel.preview-results")).selected(preview).on_click(cx.listener(move |this, _, _, cx| this.set_merge_preview(!preview, cx))))
                    .child(Button::new("merge-prev").small().ghost().label("◀").on_click(cx.listener(|this, _, _, cx| this.step_merge_record(-1, cx))))
                    .child(div().text_sm().child(status))
                    .child(Button::new("merge-next").small().ghost().label("▶").on_click(cx.listener(|this, _, _, cx| this.step_merge_record(1, cx)))),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(Button::new("merge-first").small().ghost().label("⏮").on_click(cx.listener(|this, _, _, cx| this.goto_merge_record(1, cx))))
                    .child(Button::new("merge-last").small().ghost().label("⏭").on_click(cx.listener(|this, _, _, cx| this.goto_merge_record(usize::MAX, cx))))
                    .child(Input::new(&goto_field).small())
                    .child(Button::new("merge-goto").small().ghost().label(crate::shell::i18n::text("docx.merge-panel.go-to")).on_click(cx.listener(|this, _, _, cx| {
                        let n = this.merge_state().and_then(|m| m.goto_field.read(cx).value().trim().parse::<usize>().ok()).unwrap_or(1);
                        this.goto_merge_record(n, cx);
                    }))),
            )
            .child(h_flex().gap_1().child(Input::new(&find_field).small()).child(Button::new("merge-find").small().ghost().label(crate::shell::i18n::text("docx.merge-panel.find-recipient")).on_click(cx.listener(|this, _, _, cx| { this.find_merge_record(cx); }))))
            .child(
                h_flex()
                    .gap_1()
                    .child(Button::new("merge-check-errors").small().ghost().label(crate::shell::i18n::text("docx.merge-panel.check-errors")).on_click(cx.listener(|this, _, _, cx| { this.check_merge_errors(cx); })))
                    .child(Button::new("merge-separate").small().label(crate::shell::i18n::text("docx.merge-panel.create-separate-documents")).on_click(cx.listener(|this, _, window, cx| this.save_merged_separately(window, cx))))
            )
            .child(Button::new("merge-finish").primary().small().label(crate::shell::i18n::text("docx.merge-panel.create-individual-documents")).on_click(cx.listener(|this, _, window, cx| this.save_merged_documents(window, cx))));
        let is_range_all = matches!(print_range, MergePrintRange::All);
        let is_range_current = matches!(print_range, MergePrintRange::Current);
        let is_range_from_to = matches!(print_range, MergePrintRange::FromTo(_, _));
        let print_range_from_to_inputs: Option<AnyElement> = if is_range_from_to {
            Some(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(div().text_xs().child(crate::shell::i18n::text("docx.merge-panel.print-from")))
                    .child(Input::new(&print_from_field).small())
                    .child(div().text_xs().child(crate::shell::i18n::text("docx.merge-panel.print-to")))
                    .child(Input::new(&print_to_field).small())
                    .into_any_element(),
            )
        } else {
            None
        };
        let print_range_section = v_flex()
            .gap_1()
            .child(div().text_sm().child(crate::shell::i18n::text("docx.merge-panel.print-range")))
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new("merge-range-all")
                            .small()
                            .ghost()
                            .label(crate::shell::i18n::text("docx.merge-panel.range-all"))
                            .selected(is_range_all)
                            .on_click(cx.listener(|this, _, _, cx| this.set_merge_print_range(MergePrintRange::All, cx))),
                    )
                    .child(
                        Button::new("merge-range-current")
                            .small()
                            .ghost()
                            .label(crate::shell::i18n::text("docx.merge-panel.range-current"))
                            .selected(is_range_current)
                            .on_click(cx.listener(|this, _, _, cx| this.set_merge_print_range(MergePrintRange::Current, cx))),
                    )
                    .child(
                        Button::new("merge-range-from-to")
                            .small()
                            .ghost()
                            .label(crate::shell::i18n::text("docx.merge-panel.range-from-to"))
                            .selected(is_range_from_to)
                            .on_click(cx.listener(|this, _, _, cx| this.set_merge_print_range(MergePrintRange::FromTo(1, 1), cx))),
                    ),
            )
            .children(print_range_from_to_inputs);
        let body = body
            .child(print_range_section)
            .child(
                h_flex()
                    .gap_1()
                    .child(Button::new("merge-print").small().label(crate::shell::i18n::text("docx.merge-panel.print-documents")).on_click(cx.listener(|this, _, window, cx| this.print_merged_documents(window, cx))))
                    .child(Button::new("merge-export-pdf").small().label(crate::shell::i18n::text("docx.merge-panel.export-pdf")).on_click(cx.listener(|this, _, window, cx| this.export_merged_pdf(window, cx)))),
            );
        Some(SidebarPanel { title: crate::shell::i18n::text("docx.merge-panel.mail-merge"), body: body.into_any_element() })
    }
}
