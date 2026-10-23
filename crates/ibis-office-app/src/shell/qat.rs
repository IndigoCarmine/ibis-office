// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The quick access toolbar (クイック アクセス ツール バー): commands the user puts
//! in the title row beside undo and redo (ツール > 設定… > クイック アクセス ツール
//! バー). A command is a menu item by its id; the row shows the ones the open
//! document's menus have.
use std::cell::RefCell;

use crate::shell::settings;

/// The commands to choose from (menu item ids, each with a label of its own id in the language files).
pub(crate) const CANDIDATES: &[&str] = &[
    "file.open",
    "file.save",
    "file.save-as",
    "file.print",
    "file.export.pdf",
    "edit.cut",
    "edit.copy",
    "edit.paste",
    "edit.paste-plain",
    "edit.select-all",
    "edit.find-replace",
    "format.clear",
    "view.zoom-in",
    "view.zoom-out",
    "insert.link",
    "insert.comment",
    "xlsx.insert-row-above",
    "xlsx.delete-row",
    "xlsx.insert-table",
    "xlsx.insert-picture",
    "xlsx.pivot-insert",
    "xlsx.slicer-insert",
    "xlsx.fill-down",
    "xlsx.freeze",
    "xlsx.gridlines",
    "xlsx.zoom-selection",
    "xlsx.copy-as-picture",
    "xlsx.ink",
    "xlsx.sort",
    "xlsx.sort-descending",
    "xlsx.filter",
    "xlsx.calculate-now",
    "xlsx.custom-views",
    "xlsx.solver",
    "xlsx.forecast-sheet",
    "xlsx.validation",
    "docx.track.record",
    "docx.track.accept",
    "docx.track.reject",
    "docx.accessibility-check",
    "docx.read-aloud",
    "docx.inspect-document",
    "docx.compatibility-check",
    "docx.print-settings",
    "docx.file.convert-format",
    "pptx.new-slide",
    "pptx.duplicate-slide",
    "pptx.delete-slide",
    "pptx.selection-pane",
    "pptx.reading-order",
    "pptx.check-accessibility",
    "pptx.inspect-document",
    "pptx.3d-model",
    "pptx.record-preview",
    "pptx.screen-reader",
];

const KEY: &str = "qat";

thread_local! {
    static CACHE: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// The commands chosen, in order.
pub(crate) fn commands() -> Vec<String> {
    CACHE.with(|c| {
        c.borrow_mut()
            .get_or_insert_with(|| settings::get(KEY).unwrap_or_default().split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect())
            .clone()
    })
}

/// The commands chosen replaced (kept).
pub(crate) fn set(ids: &[String]) {
    settings::set(KEY, &ids.join(","));
    CACHE.with(|c| *c.borrow_mut() = Some(ids.to_vec()));
}

/// A command put in the row, or taken out.
pub(crate) fn toggle(id: &str) {
    let mut ids = commands();
    match ids.iter().position(|i| i == id) {
        Some(at) => {
            ids.remove(at);
        }
        None => ids.push(id.to_string()),
    }
    set(&ids);
}

/// Move a command up (earlier) in the order. Returns true if moved.
pub(crate) fn move_up(id: &str) -> bool {
    let mut ids = commands();
    if let Some(pos) = ids.iter().position(|i| i == id) {
        if pos > 0 {
            ids.swap(pos, pos - 1);
            set(&ids);
            return true;
        }
    }
    false
}

/// Move a command down (later) in the order. Returns true if moved.
pub(crate) fn move_down(id: &str) -> bool {
    let mut ids = commands();
    if let Some(pos) = ids.iter().position(|i| i == id) {
        if pos + 1 < ids.len() {
            ids.swap(pos, pos + 1);
            set(&ids);
            return true;
        }
    }
    false
}

/// Reset QAT commands to empty.
pub(crate) fn reset() {
    set(&[]);
}

/// Forget what is cached (the settings changed under it).
#[allow(dead_code)]
pub(crate) fn reload() {
    CACHE.with(|c| *c.borrow_mut() = None);
}
