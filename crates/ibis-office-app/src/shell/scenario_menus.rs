// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of the menus: no menu of the menu bar and no drop-down of the
//! toolbar has a submenu with nothing in it (no items, or only separators).
//! The menus as the shell and the formats build them are listed first
//! (`menus_unpruned`); a submenu that is empty there must be one of the
//! ids in `the hidden lists` (not yet made; the menu leaves it out), and the
//! menus as drawn have none.
use ibis_doc_model::ShapeKind;

use crate::shell::chrome::{MenuEntry, ToolItem};
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::menus_docx_no_empty_submenu", menus_docx_no_empty_submenu),
        ("shell::menus_xlsx_no_empty_submenu", menus_xlsx_no_empty_submenu),
        ("shell::menus_pptx_no_empty_submenu", menus_pptx_no_empty_submenu),
        ("shell::menus_pptx_selected_no_empty_submenu", menus_pptx_selected_no_empty_submenu),
    ]
}

/// Submenus that have no items yet (a feature that is not made): the menu
/// leaves them out. Take an id out of here when its items are made.
/// Excel has no table menu (it is the item 表), nor list and spacing for cells.
const DOCX_HIDDEN: &[&str] = &[];
const XLSX_HIDDEN: &[&str] = &["insert.table", "format.spacing", "format.lists"];
const PPTX_HIDDEN: &[&str] = &[];

fn is_blank(entries: &[MenuEntry]) -> bool {
    entries.iter().all(|e| matches!(e, MenuEntry::Separator))
}

/// The ids of the submenus with nothing in them, in `entries`.
fn empty_ids(entries: &[MenuEntry], out: &mut Vec<&'static str>) {
    for entry in entries {
        if let MenuEntry::Submenu(sub) = entry {
            if is_blank(&sub.items) {
                out.push(sub.id);
            }
            empty_ids(&sub.items, out);
        }
    }
}

fn toolbar_menus(items: Vec<ToolItem>) -> Vec<Vec<MenuEntry>> {
    items
        .into_iter()
        .filter_map(|item| match item {
            ToolItem::Button(b) => b.menu,
            ToolItem::Select { menu, .. } => Some(menu),
            _ => None,
        })
        .collect()
}

/// Check the open document's menus: `what` names the state.
fn check(s: &mut Scenario, what: &str, hidden: &[&str]) {
    let (raw, shown, tools) = s.with(|e, _| {
        let mut raw = Vec::new();
        for (_, entries) in &e.menus_unpruned().menus {
            empty_ids(entries, &mut raw);
        }
        let mut shown = Vec::new();
        for (_, entries) in &e.menus().menus {
            empty_ids(entries, &mut shown);
        }
        let mut tools = Vec::new();
        for menu in toolbar_menus(e.toolbar_items()) {
            empty_ids(&crate::shell::chrome::prune_entries(menu.clone()), &mut tools);
            let mut raw_tool = Vec::new();
            empty_ids(&menu, &mut raw_tool);
            raw.extend(raw_tool);
        }
        (raw, shown, tools)
    });
    assert!(shown.is_empty(), "{what}: the menu bar draws empty submenus {shown:?}");
    assert!(tools.is_empty(), "{what}: a toolbar drop-down draws empty submenus {tools:?}");
    let unknown: Vec<_> = raw.iter().filter(|id| !hidden.contains(id)).collect();
    assert!(unknown.is_empty(), "{what}: submenus with nothing in them (make them, or list them in the hidden list above): {unknown:?}");
}

fn menus_docx_no_empty_submenu() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    check(&mut s, "docx", DOCX_HIDDEN);
}

fn menus_xlsx_no_empty_submenu() {
    let mut s = Scenario::open("test/editing/xlsx-compatibility-v1.xlsx");
    check(&mut s, "xlsx", XLSX_HIDDEN);
}

fn menus_pptx_no_empty_submenu() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    check(&mut s, "pptx, nothing selected", PPTX_HIDDEN);
}

/// Text, a picture, a chart, a table, and two shapes together selected.
fn menus_pptx_selected_no_empty_submenu() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let kinds: [(&str, fn(&ShapeKind) -> bool); 4] = [
        ("text", |k| matches!(k, ShapeKind::Auto(a) if a.text.is_some())),
        ("picture", |k| matches!(k, ShapeKind::Picture(_))),
        ("chart", |k| matches!(k, ShapeKind::Chart(_))),
        ("table", |k| matches!(k, ShapeKind::Table(_))),
    ];
    for (name, is_kind) in kinds {
        let found = s.with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| is_kind(&sh.kind)).map(|j| (i + 1, j))));
        let Some((page, shape)) = found else {
            eprintln!("menus: no {name} in the file");
            continue;
        };
        s.page(page);
        s.click_shape(shape);
        check(&mut s, &format!("pptx, a {name} selected"), PPTX_HIDDEN);
    }
    s.key("cmd-a");
    check(&mut s, "pptx, everything selected", PPTX_HIDDEN);
}
