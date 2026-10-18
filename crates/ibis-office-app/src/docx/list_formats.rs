// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 表示形式 > 段落番号 > 番号の書式 (Word's 番号ライブラリ) and 番号の後
//! (its 番号に続く空白の扱い): the caret's list level numbered in a chosen
//! format (the list's definition changed, as Word changes the list), or,
//! out of a list, the selected paragraphs made a list in it; what follows
//! the number: a tab, a space or nothing.
use gpui_kit::*;
use ibis_doc_model::{FlowCommand, ListSuffix, ParaChange, flow_command};

use crate::shell::editor::SlideEditor;

/// The library: the menu item's id, its label, the format (`w:numFmt`) and
/// the number's text (`{n}` for the level's number).
pub(crate) const LIBRARY: [(&str, &str, &str, &str); 11] = [
    ("docx.list.format-decimal-dot", "1. 2. 3.", "decimal", "{n}."),
    ("docx.list.format-decimal-paren", "1) 2) 3)", "decimal", "{n})"),
    ("docx.list.format-decimal-parens", "(1) (2) (3)", "decimal", "({n})"),
    ("docx.list.format-circled", "① ② ③", "decimalEnclosedCircle", "{n}"),
    ("docx.list.format-fullwidth", "１. ２. ３.", "decimalFullWidth", "{n}."), // i18n-ok（番号の見本（全角数字）。どの言語でも同じ。MenuItem::new は訳さない）
    ("docx.list.format-kanji", "fix.list-formats.japanese-counting", "japaneseCounting", "{n}、"), // i18n-ok（番号の書式（数字の後ろに付く文字）。文書に保存される）
    ("docx.list.format-iroha", "fix.list-formats.iroha", "iroha", "{n}."),
    ("docx.list.format-lower-letter", "a. b. c.", "lowerLetter", "{n}."),
    ("docx.list.format-upper-letter", "A. B. C.", "upperLetter", "{n}."),
    ("docx.list.format-lower-roman", "i. ii. iii.", "lowerRoman", "{n}."),
    ("docx.list.format-upper-roman", "I. II. III.", "upperRoman", "{n}."),
];

/// The bullet library: the menu item's id, its label, the bullet text and optional font.
pub(crate) const BULLET_LIBRARY: [(&str, &str, &str, Option<&str>); 7] = [
    ("docx.bullet.disc", "●", "●", Some("Arial")),
    ("docx.bullet.circle", "○", "○", Some("Courier New")),
    ("docx.bullet.square", "■", "■", Some("Wingdings")),
    ("docx.bullet.diamond", "◆", "◆", Some("Wingdings")),
    ("docx.bullet.arrow", "➢", "➢", Some("Wingdings")),
    ("docx.bullet.check", "✓", "✓", Some("Wingdings")),
    ("docx.bullet.dash", "–", "–", Some("Arial")),
];

impl SlideEditor {
    /// The caret's list (its number, its level) if it is in one.
    fn caret_list_ref(&self) -> Option<ibis_doc_model::ListRef> {
        let (start, _) = self.ordered_selection()?;
        flow_command::paragraph(self.flow()?, start.para)?.list
    }

    /// Number the caret's list level (or the selected paragraphs, out of a
    /// list) in the library's entry `index`.
    pub(crate) fn apply_list_format(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(&(_, _, format, pattern)) = LIBRARY.get(index) else { return };
        let Some(doc) = self.flow().cloned() else { return };
        let mut numbering = doc.styles.numbering.clone();
        let current = self.caret_list_ref().filter(|l| numbering.level(l.num_id, l.level).is_some_and(|lv| lv.format != "bullet"));
        let (num_id, level, mut commands) = match current {
            Some(l) => (l.num_id, l.level, Vec::new()),
            None => {
                let (with, id) = numbering.with_new_list(false);
                numbering = with;
                let Some((first, last)) = self.selected_paragraphs() else { return };
                (id, 0, vec![FlowCommand::SetParagraphFormat { first, last, change: ParaChange::List(Some(id)) }])
            }
        };
        let Some(abstract_id) = numbering.nums.get(&num_id).map(|n| n.abstract_id) else { return };
        let Some(lv) = numbering.abstracts.get_mut(&abstract_id).and_then(|a| a.levels.get_mut(level as usize)) else { return };
        lv.format = format.into();
        lv.text = pattern.replace("{n}", &format!("%{}", level + 1));
        commands.insert(0, FlowCommand::SetNumbering { numbering });
        self.run_flow(commands, cx);
    }

    /// 番号の後: a tab, a space or nothing after the caret's level's number.
    pub(crate) fn set_list_suffix(&mut self, suffix: ListSuffix, cx: &mut Context<Self>) {
        let (Some(l), Some(doc)) = (self.caret_list_ref(), self.flow()) else { return };
        let mut numbering = doc.styles.numbering.clone();
        let Some(abstract_id) = numbering.nums.get(&l.num_id).map(|n| n.abstract_id) else { return };
        let Some(lv) = numbering.abstracts.get_mut(&abstract_id).and_then(|a| a.levels.get_mut(l.level as usize)) else { return };
        if lv.suffix == suffix {
            return;
        }
        lv.suffix = suffix;
        self.run_flow(vec![FlowCommand::SetNumbering { numbering }], cx);
    }

    /// The caret's level's format and suffix, for the menus' check marks.
    pub(crate) fn caret_list_format(&self) -> Option<(String, String, ListSuffix)> {
        let l = self.caret_list_ref()?;
        let lv = self.flow()?.styles.numbering.level(l.num_id, l.level)?;
        Some((lv.format.clone(), lv.text.replace(&format!("%{}", l.level + 1), "{n}"), lv.suffix))
    }
    /// Apply a bullet format from the library entry `index` to the caret's list level,
    /// or turn the selected paragraphs into a bulleted list.
    pub(crate) fn apply_bullet_format(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(&(_, _, text, font)) = BULLET_LIBRARY.get(index) else { return };
        let Some(doc) = self.flow().cloned() else { return };
        let mut numbering = doc.styles.numbering.clone();
        let current = self.caret_list_ref().filter(|l| numbering.level(l.num_id, l.level).is_some_and(|lv| lv.format == "bullet"));
        let (num_id, level, mut commands) = match current {
            Some(l) => (l.num_id, l.level, Vec::new()),
            None => {
                let (with, id) = numbering.with_new_list(true);
                numbering = with;
                let Some((first, last)) = self.selected_paragraphs() else { return };
                (id, 0, vec![FlowCommand::SetParagraphFormat { first, last, change: ParaChange::List(Some(id)) }])
            }
        };
        let Some(abstract_id) = numbering.nums.get(&num_id).map(|n| n.abstract_id) else { return };
        let Some(lv) = numbering.abstracts.get_mut(&abstract_id).and_then(|a| a.levels.get_mut(level as usize)) else { return };
        lv.format = "bullet".into();
        lv.text = text.into();
        lv.run.font = font.map(|f| f.into());
        commands.insert(0, FlowCommand::SetNumbering { numbering });
        self.run_flow(commands, cx);
    }

    /// The caret's bullet symbol and suffix if it is in a bulleted list.
    pub(crate) fn caret_bullet_format(&self) -> Option<(String, Option<String>, ListSuffix)> {
        let l = self.caret_list_ref()?;
        let lv = self.flow()?.styles.numbering.level(l.num_id, l.level)?;
        if lv.format != "bullet" { return None; }
        Some((lv.text.clone(), lv.run.font.clone(), lv.suffix))
    }

    /// 行頭文字の後: a tab, a space or nothing after the caret's level's bullet.
    pub(crate) fn set_bullet_suffix(&mut self, suffix: ListSuffix, cx: &mut Context<Self>) {
        self.set_list_suffix(suffix, cx);
    }

    /// The list styles defined in the document: (style_id, name, num_id).
    pub(crate) fn list_styles(&self) -> Vec<(String, String, u32)> {
        let Some(doc) = self.flow() else { return Vec::new() };
        let mut out = Vec::new();
        // 1. From styles.styles with kind == Numbering and num_id
        for (id, style) in &doc.styles.styles {
            if style.kind == ibis_doc_model::StyleKind::Numbering {
                if let Some(num_id) = style.num_id {
                    out.push((id.clone(), style.name.clone(), num_id));
                } else {
                    // Try to find num_id from numbering abstracts matching num_style_link
                    if let Some((&num_id, _)) = doc.styles.numbering.nums.iter().find(|(_, n)| {
                        doc.styles.numbering.abstracts.get(&n.abstract_id).and_then(|a| a.num_style_link.as_deref()) == Some(id.as_str())
                    }) {
                        out.push((id.clone(), style.name.clone(), num_id));
                    }
                }
            }
        }
        // 2. From numbering abstracts with num_style_link that might not be in styles.styles yet
        for (num_id, n) in &doc.styles.numbering.nums {
            if let Some(abs) = doc.styles.numbering.abstracts.get(&n.abstract_id) {
                if let Some(link) = &abs.num_style_link {
                    if !out.iter().any(|(id, _, _)| id == link) {
                        out.push((link.clone(), link.clone(), *num_id));
                    }
                }
            }
        }
        out.sort_by(|a, b| a.1.cmp(&b.1));
        out
    }

    /// Define a new list style (`DOCX-LIST-011`) with the given name and optional levels,
    /// adding it to both styles.xml and numbering.xml, and applying it to the selection.
    pub(crate) fn define_list_style(&mut self, name: &str, levels: Option<Vec<ibis_doc_model::ListLevel>>, cx: &mut Context<Self>) -> u32 {
        let Some(doc) = self.flow().cloned() else { return 0 };
        let mut style_id = name.trim().replace(' ', "");
        if style_id.is_empty() {
            style_id = "ListStyle".to_string();
        }
        let base_id = style_id.clone();
        let mut counter = 1;
        while doc.styles.styles.contains_key(&style_id) {
            style_id = format!("{base_id}{counter}");
            counter += 1;
        }

        let (numbering, num_id) = doc.styles.numbering.with_new_list_style(&style_id, levels);
        let list_style = ibis_doc_model::FlowStyle {
            kind: ibis_doc_model::StyleKind::Numbering,
            name: name.trim().to_string(),
            num_id: Some(num_id),
            quick: true,
            ..ibis_doc_model::FlowStyle::default()
        };

        let mut commands = vec![
            FlowCommand::SetNumbering { numbering },
            FlowCommand::SetStyle { id: style_id, style: Some(list_style) },
        ];

        if let Some((first, last)) = self.selected_paragraphs() {
            commands.push(FlowCommand::SetParagraphFormat {
                first,
                last,
                change: ParaChange::List(Some(num_id)),
            });
        }

        self.run_flow(commands, cx);
        num_id
    }

    /// Apply an existing list style (by `num_id`) to the selected paragraphs or current caret paragraph.
    pub(crate) fn apply_list_style(&mut self, num_id: u32, cx: &mut Context<Self>) {
        let Some((first, last)) = self.selected_paragraphs() else { return };
        self.run_flow(vec![FlowCommand::SetParagraphFormat {
            first,
            last,
            change: ParaChange::List(Some(num_id)),
        }], cx);
    }

    /// The name of the list style at the caret, if the paragraph is in a list style.
    pub(crate) fn caret_list_style(&self) -> Option<String> {
        let l = self.caret_list_ref()?;
        let doc = self.flow()?;
        let abs_id = doc.styles.numbering.nums.get(&l.num_id)?.abstract_id;
        let abs = doc.styles.numbering.abstracts.get(&abs_id)?;
        if let Some(link) = &abs.num_style_link {
            if let Some(style) = doc.styles.styles.get(link) {
                return Some(style.name.clone());
            }
            return Some(link.clone());
        }
        // Check if any style matches num_id
        for style in doc.styles.styles.values() {
            if style.kind == ibis_doc_model::StyleKind::Numbering && style.num_id == Some(l.num_id) {
                return Some(style.name.clone());
            }
        }
        None
    }
}
