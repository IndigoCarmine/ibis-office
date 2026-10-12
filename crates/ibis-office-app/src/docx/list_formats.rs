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
}
