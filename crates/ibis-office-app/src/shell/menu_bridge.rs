// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The Format menu's submenus (テキスト, 配置とインデント, 行間隔と段落の間隔,
//! 箇条書きと番号付きリスト, 枠線と線) filled from the toolbar's buttons of
//! the same operations, when the format did not fill them itself: the
//! menu item does what the button does, is grey when the button is, and is
//! checked when the button is pressed. A submenu with no such button stays
//! empty (and the menu leaves it out, `chrome::prune_entries`).
use crate::shell::chrome::{MenuEntry, MenuItem, Menus, Submenu, ToolButton, ToolItem};
use crate::shell::i18n;

/// (submenu id, the toolbar buttons it takes, in order; a `*` before an id
/// means the button's own menu is put in the submenu instead of the button).
const GROUPS: &[(&str, &[&str])] = &[
    ("format.text", &["bold", "italic", "underline", "strikethrough", "text-shadow", "case"]),
    ("format.align", &["*align", "*halign", "valign", "vertical-align", "text-direction", "wrap", "merge", "indent-less", "indent-more"]),
    ("format.spacing", &["*line-spacing"]),
    ("format.lists", &["checklist", "bullets", "bullet-types", "numbered", "number-types"]),
    ("format.borders", &["*borders", "border-color", "line-weight", "line-style"]),
];

fn disable_all(entries: &mut Vec<MenuEntry>) {
    entries.retain(|e| !matches!(e, MenuEntry::Palette(_)));
    for entry in entries {
        match entry {
            MenuEntry::Item(item) => item.disabled = true,
            MenuEntry::Submenu(sub) => disable_all(&mut sub.items),
            _ => {}
        }
    }
}

/// The entries that stand for `button`: its menu itself (`flatten`), a
/// submenu of its menu, or an item.
fn entries_of(button: &ToolButton, flatten: bool) -> Vec<MenuEntry> {
    let label = i18n::text(&button.tooltip);
    let menu = button.menu.clone().filter(|m| !m.is_empty());
    if let Some(mut menu) = menu {
        if flatten {
            if button.disabled {
                disable_all(&mut menu);
            }
            return menu;
        }
        if button.disabled {
            return vec![MenuItem::new(button.id, label).into()];
        }
        return vec![Submenu::new(button.id, button.icon.unwrap_or("tune"), label, menu).into()];
    }
    let mut item = MenuItem::new(button.id, label).checked(button.on).disabled(button.disabled);
    item.icon = button.icon;
    item.handler = button.handler.clone();
    vec![item.into()]
}

/// Fill the empty submenus of `menus` that have buttons in `tools`.
pub(crate) fn fill(menus: &mut Menus, tools: impl FnOnce() -> Vec<ToolItem>) {
    let is_empty = |menus: &Menus, id: &str| matches!(menus.find_entry(id), Some(MenuEntry::Submenu(s)) if s.items.iter().all(|e| matches!(e, MenuEntry::Separator)));
    if !GROUPS.iter().any(|(id, _)| is_empty(menus, id)) {
        return;
    }
    let buttons: Vec<ToolButton> = tools()
        .into_iter()
        .filter_map(|t| match t {
            ToolItem::Button(b) => Some(b),
            _ => None,
        })
        .collect();
    for (id, wanted) in GROUPS {
        if !is_empty(menus, id) {
            continue;
        }
        let mut entries: Vec<MenuEntry> = Vec::new();
        for want in *wanted {
            let (flatten, name) = want.strip_prefix('*').map_or((false, *want), |n| (true, n));
            if let Some(button) = buttons.iter().find(|b| b.id == name) {
                if !entries.is_empty() && flatten {
                    entries.push(MenuEntry::Separator);
                }
                entries.extend(entries_of(button, flatten));
            }
        }
        // Not an item the menus already have (the format's own, with its own state).
        let entries = without_known(entries, menus);
        if entries.is_empty() {
            continue;
        }
        for (_, list) in &mut menus.menus {
            if put(list, id, &entries) {
                break;
            }
        }
    }
}

fn put(list: &mut [MenuEntry], id: &str, entries: &[MenuEntry]) -> bool {
    for entry in list {
        if let MenuEntry::Submenu(sub) = entry {
            if sub.id == id {
                sub.items = entries.to_vec();
                return true;
            }
            if put(&mut sub.items, id, entries) {
                return true;
            }
        }
    }
    false
}

/// `entries` without the items whose ids `menus` already has.
fn without_known(entries: Vec<MenuEntry>, menus: &Menus) -> Vec<MenuEntry> {
    entries
        .into_iter()
        .filter_map(|entry| match entry {
            MenuEntry::Item(item) if menus.find_entry(item.id).is_some() => None,
            MenuEntry::Submenu(mut sub) => {
                sub.items = without_known(std::mem::take(&mut sub.items), menus);
                (!sub.items.is_empty()).then_some(MenuEntry::Submenu(sub))
            }
            other => Some(other),
        })
        .collect()
}
