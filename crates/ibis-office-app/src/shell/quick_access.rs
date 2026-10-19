// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The quick access toolbar (クイックアクセス ツールバー), as Office's: the
//! commands the user put in the title row, after undo and redo. A command is
//! a menu item's or a toolbar button's id: each frame the buttons are made
//! again from the menus and the toolbar the format built, so they are greyed
//! and pressed as the originals are. One list per kind of document (Word's,
//! Excel's and PowerPoint's ids differ), kept with the settings
//! (`qat.document`, `qat.workbook`, `qat.presentation`: `id|icon|tooltip`
//! joined by `;`); the icon and the tooltip are kept too, for a command the
//! format does not have at the moment (bold with nothing selected).
//!
//! Commands are added by a toolbar button's right-click menu, the ▼ at the
//! end of the row (the usual commands), or the customize dialog (every
//! command of the menus and the toolbar). Alt pressed and let go alone shows
//! each button's key tip (1–9, then 09–01, then 0A–0Z): typing it runs the
//! command; Alt+1…9 runs one at once (not on a Mac: Option+digit types).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_pptx::Kind;

use crate::shell::chrome::{MenuEntry, MenuItem, ToolButton, ToolItem};
use crate::shell::editor::SlideEditor;
use crate::shell::i18n;
use crate::shell::icons::Material;

/// A command of the quick access toolbar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct QatEntry {
    /// The menu item's or the toolbar button's id.
    pub id: String,
    /// Its icon, for when the format does not have it at the moment.
    pub icon: String,
    /// Its name (a key of the tables, or a text), likewise.
    pub tooltip: String,
}

impl QatEntry {
    pub(crate) fn new(id: &str, icon: &str, tooltip: &str) -> Self {
        let clean = |s: &str| s.replace([';', '|', '\n', '\r'], " ").trim().to_string();
        Self { id: clean(id), icon: clean(icon), tooltip: clean(tooltip) }
    }
}

/// The ids of the menu items that are made for each thing of a list (the
/// recent files, the layouts, …): not one command, so not kept.
const DYNAMIC_IDS: [&str; 10] = [
    "file.recent.item",
    "xlsx.unhide-sheet",
    "xlsx.validation-item",
    "pptx.new-slide.layout",
    "pptx.layout.apply",
    "docx.style.update",
    "xlsx.font",
    "xlsx.function",
    "slideshow.custom-show-item",
    "toggle-menus",
];

/// Whether the command `id` can be put in the quick access toolbar.
pub(crate) fn can_add(id: &str) -> bool {
    !id.is_empty() && !DYNAMIC_IDS.contains(&id) && !id.contains([';', '|'])
}

fn setting_key(kind: Kind) -> &'static str {
    match kind {
        Kind::Document => "qat.document",
        Kind::Workbook => "qat.workbook",
        Kind::Presentation => "qat.presentation",
    }
}

/// The list until the user changes it: 上書き保存.
fn defaults() -> Vec<QatEntry> {
    vec![QatEntry::new("file.save", "save", "file.save")]
}

/// A kept list read back (an entry without an id is left out, as a second
/// one of the same id).
pub(crate) fn parse(text: &str) -> Vec<QatEntry> {
    let mut seen = HashSet::new();
    text.split(';')
        .filter_map(|part| {
            let mut fields = part.split('|');
            let id = fields.next()?.trim();
            let icon = fields.next().unwrap_or("").trim();
            let tooltip = fields.next().unwrap_or("").trim();
            (!id.is_empty() && seen.insert(id.to_string())).then(|| QatEntry::new(id, icon, tooltip))
        })
        .collect()
}

/// The list as it is kept.
pub(crate) fn serialize(entries: &[QatEntry]) -> String {
    entries.iter().map(|e| format!("{}|{}|{}", e.id, e.icon, e.tooltip)).collect::<Vec<_>>().join(";")
}

thread_local! {
    /// The lists read (the settings file is read once per kind).
    static LISTS: RefCell<HashMap<&'static str, Vec<QatEntry>>> = RefCell::new(HashMap::new());
}

/// The quick access toolbar of `kind`.
pub(crate) fn entries(kind: Kind) -> Vec<QatEntry> {
    let key = setting_key(kind);
    LISTS.with(|lists| {
        lists
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| crate::shell::settings::get(key).map(|text| parse(&text)).unwrap_or_else(defaults))
            .clone()
    })
}

/// Keep `list` as the quick access toolbar of `kind`.
fn store(kind: Kind, list: Vec<QatEntry>) {
    let key = setting_key(kind);
    crate::shell::settings::set(key, &serialize(&list));
    LISTS.with(|lists| lists.borrow_mut().insert(key, list));
}

/// Forget the lists read: the next use reads the settings again (the scenario tests).
#[allow(dead_code)]
pub(crate) fn forget() {
    LISTS.with(|lists| lists.borrow_mut().clear());
}

pub(crate) fn contains(kind: Kind, id: &str) -> bool {
    entries(kind).iter().any(|e| e.id == id)
}

/// Add `entry` at the end. Whether it was added (not there yet, and a command that can be).
pub(crate) fn add(kind: Kind, entry: QatEntry) -> bool {
    let mut list = entries(kind);
    if !can_add(&entry.id) || list.iter().any(|e| e.id == entry.id) {
        return false;
    }
    list.push(entry);
    store(kind, list);
    true
}

pub(crate) fn remove(kind: Kind, id: &str) {
    let mut list = entries(kind);
    list.retain(|e| e.id != id);
    store(kind, list);
}

/// Move the entry at `index` by `delta` places (within the list).
pub(crate) fn move_by(kind: Kind, index: usize, delta: isize) {
    let mut list = entries(kind);
    let to = index as isize + delta;
    if index < list.len() && to >= 0 && (to as usize) < list.len() {
        let entry = list.remove(index);
        list.insert(to as usize, entry);
        store(kind, list);
    }
}

/// Back to the list the app starts with.
pub(crate) fn reset(kind: Kind) {
    store(kind, defaults());
}

/// The key tips of `n` buttons, as Office's: 1–9, then 09–01, then 0A–0Z
/// (fewer when there are more buttons than that).
pub(crate) fn key_tip_labels(n: usize) -> Vec<String> {
    (0..n)
        .map_while(|i| match i {
            0..=8 => Some((i + 1).to_string()),
            9..=17 => Some(format!("0{}", 18 - i)),
            _ => (i - 18 < 26).then(|| format!("0{}", (b'A' + (i - 18) as u8) as char)),
        })
        .collect()
}

thread_local! {
    static INTERNED: RefCell<HashSet<&'static str>> = RefCell::new(HashSet::new());
}

/// `s` as a `&'static str` (the toolbar's ids and icons are; each kept id is
/// made one once).
fn intern(s: &str) -> &'static str {
    INTERNED.with(|set| {
        let mut set = set.borrow_mut();
        if let Some(found) = set.get(s) {
            return *found;
        }
        let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
        set.insert(leaked);
        leaked
    })
}

/// A command the customize dialog lists: the group it is in (a menu's name,
/// or the toolbar's), its entry and its name in the UI's language.
#[derive(Clone)]
pub(crate) struct Command {
    pub group: SharedString,
    pub entry: QatEntry,
    pub label: SharedString,
}

/// What a menu item's label shows (as `chrome::menu_item_row`).
fn item_label(item: &MenuItem) -> SharedString {
    i18n::text(&item.label)
}

/// What to keep as a menu item's name: its id when the tables have it (so
/// it follows the UI's language), else its label.
fn item_name_key(item: &MenuItem) -> &str {
    if i18n::is_known_id(item.id) { item.id } else { &item.label }
}

/// The commands the usual ▼ offers, when the format has them.
const USUAL: [&str; 8] = ["file.new.document", "file.open", "file.save", "file.save-as", "file.print", "file.export.pdf", "edit.find-replace", "file.close"];

/// The key tips state: shown, with what was typed of a two-key tip.
#[derive(Clone, Debug, Default)]
pub(crate) struct KeyTips {
    pub typed: String,
}

/// The customize dialog, while it is open.
pub(crate) struct QatDialog {
    search: Entity<InputState>,
    _search_subscription: Subscription,
    /// The command selected in the toolbar's list.
    selected: Option<usize>,
}

/// A menu opened by the chrome (a toolbar button's right-click menu), at a
/// place of the window.
pub(crate) struct ChromePopup {
    pub menu: Entity<PopupMenu>,
    pub position: Point<Pixels>,
    _dismiss: Subscription,
}

impl SlideEditor {
    /// The quick access toolbar's buttons for the open document, in order:
    /// the format's button or menu item of each id, else a grey one.
    pub(crate) fn quick_access_buttons(&self) -> Vec<ToolButton> {
        let list = entries(self.kind());
        if list.is_empty() {
            return Vec::new();
        }
        let mut tools: Vec<ToolButton> = self
            .toolbar_items()
            .into_iter()
            .filter_map(|t| match t {
                ToolItem::Button(b) => Some(b),
                _ => None,
            })
            .collect();
        if self.kind() == Kind::Presentation {
            tools.extend(crate::pptx::chrome::menu_buttons(self).into_iter().filter_map(|t| match t {
                ToolItem::Button(b) => Some(b),
                _ => None,
            }));
        }
        // The menus only when a command is not a toolbar button.
        let menus = list.iter().any(|e| !tools.iter().any(|b| b.id == e.id)).then(|| self.menus());
        list.iter()
            .map(|entry| {
                if let Some(button) = tools.iter().find(|b| b.id == entry.id) {
                    return button.clone();
                }
                match menus.as_ref().and_then(|m| m.find_entry(&entry.id)) {
                    Some(MenuEntry::Item(item)) => {
                        let mut button = ToolButton::new(item.id, item.icon.filter(|i| !i.is_empty()).unwrap_or_else(|| fallback_icon(entry)), item_label(&item)).on(item.checked).disabled(item.disabled);
                        button.handler = item.handler.clone();
                        button
                    }
                    Some(MenuEntry::Submenu(sub)) => ToolButton::new(sub.id, sub.icon.filter(|i| !i.is_empty()).unwrap_or_else(|| fallback_icon(entry)), i18n::text(&sub.label)).menu(sub.items.clone()),
                    _ => ToolButton::new(intern(&entry.id), fallback_icon(entry), i18n::text(&entry.tooltip)).disabled(true),
                }
            })
            .collect()
    }

    /// Every command of the menus and the toolbar that can be put in the
    /// quick access toolbar, by group, each once.
    pub(crate) fn quick_access_commands(&self) -> Vec<Command> {
        let mut out: Vec<Command> = Vec::new();
        let mut seen: HashSet<&'static str> = HashSet::new();
        fn walk(group: &SharedString, entries: &[MenuEntry], seen: &mut HashSet<&'static str>, out: &mut Vec<Command>) {
            for entry in entries {
                match entry {
                    MenuEntry::Item(item) if item.handler.is_some() && can_add(item.id) && seen.insert(item.id) => {
                        let label = item_label(item);
                        out.push(Command { group: group.clone(), entry: QatEntry::new(item.id, item.icon.unwrap_or(""), item_name_key(item)), label });
                    }
                    MenuEntry::Submenu(sub) => walk(group, &sub.items, seen, out),
                    _ => {}
                }
            }
        }
        for (id, entries) in &self.menus().menus {
            walk(&id.label(), entries, &mut seen, &mut out);
        }
        let toolbar = i18n::text("qat.group.toolbar");
        for item in self.toolbar_items() {
            if let ToolItem::Button(b) = item
                && can_add(b.id)
                && (b.handler.is_some() || b.quick.is_some() || b.menu.as_ref().is_some_and(|m| !m.is_empty()))
                && seen.insert(b.id)
            {
                let label = i18n::text(&b.tooltip);
                out.push(Command { group: toolbar.clone(), entry: QatEntry::new(b.id, b.icon.unwrap_or(""), &b.tooltip), label });
            }
        }
        out
    }

    /// The ▼'s menu: the usual commands (checked when in the toolbar), and the dialog.
    fn quick_access_customize_entries(&self) -> Vec<MenuEntry> {
        let kind = self.kind();
        let menus = self.menus();
        let mut entries: Vec<MenuEntry> = Vec::new();
        for id in USUAL {
            let Some(MenuEntry::Item(item)) = menus.find_entry(id) else { continue };
            if item.handler.is_none() {
                continue;
            }
            let entry = QatEntry::new(item.id, item.icon.unwrap_or(""), item_name_key(&item));
            let on = contains(kind, id);
            let mut row = MenuItem::new(item.id, item_label(&item)).checked(on).on(move |this, _, cx| {
                if contains(kind, &entry.id) {
                    remove(kind, &entry.id);
                } else {
                    add(kind, entry.clone());
                }
                this.quick_access_changed(cx);
            });
            row.icon = item.icon;
            entries.push(row.into());
        }
        entries.push(MenuEntry::Separator);
        entries.push(MenuItem::tr("qat.more-commands").icon("tune").on(|this, window, cx| this.open_quick_access_dialog(window, cx)).into());
        entries
    }

    /// Every window draws its quick access toolbar again.
    fn quick_access_changed(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        cx.refresh_windows();
    }

    /// A toolbar button's right-click menu: add it to (or remove it from)
    /// the quick access toolbar, and the dialog.
    pub(crate) fn open_quick_access_context(&mut self, entry: QatEntry, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.has_document || !can_add(&entry.id) {
            return;
        }
        let kind = self.kind();
        let id = entry.id.clone();
        let toggle = if contains(kind, &id) {
            MenuItem::tr("qat.remove").icon("remove").on(move |this, _, cx| {
                remove(kind, &id);
                this.quick_access_changed(cx);
            })
        } else {
            MenuItem::tr("qat.add").icon("add").on(move |this, _, cx| {
                add(kind, entry.clone());
                this.quick_access_changed(cx);
            })
        };
        let entries = vec![toggle.into(), MenuEntry::Separator, MenuItem::tr("qat.customize").icon("tune").on(|this, window, cx| this.open_quick_access_dialog(window, cx)).into()];
        self.open_chrome_popup(entries, position, window, cx);
    }

    /// A menu of `entries` open at `position` (the window's).
    pub(crate) fn open_chrome_popup(&mut self, entries: Vec<MenuEntry>, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let entries = Rc::new(entries);
        let editor = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, window, cx| crate::shell::chrome::fill_menu(menu.min_w(px(220.)), &entries, &editor, window, cx));
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            this.chrome_popup = None;
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.chrome_popup = Some(ChromePopup { menu, position, _dismiss: dismiss });
        cx.notify();
    }

    /// The chrome's menu, while it is open.
    pub(crate) fn render_chrome_popup(&self) -> Option<impl IntoElement> {
        self.chrome_popup.as_ref().map(|p| deferred(anchored().position(p.position).snap_to_window_with_margin(px(8.)).child(p.menu.clone())).with_priority(2))
    }

    /// Run the quick access toolbar's command at `index`: what a click does
    /// (a split button's last choice), or its menu opened under it.
    pub(crate) fn run_quick_access(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(button) = self.quick_access_buttons().into_iter().nth(index) else { return };
        if !button.enabled() {
            return;
        }
        if let Some(handler) = button.quick.clone().or_else(|| button.handler.clone()) {
            handler(self, window, cx);
        } else if let Some(menu) = button.menu.clone() {
            let bounds = self.quick_access_bounds.borrow().get(index).copied().flatten();
            let position = bounds.map_or_else(|| point(px(80.), px(40.)), |b| b.bottom_left());
            self.open_chrome_popup(menu, position, window, cx);
        }
    }

    /// Alt pressed and let go alone: show the key tips (again: hide them).
    pub(crate) fn toggle_key_tips(&mut self, cx: &mut Context<Self>) {
        if self.key_tips.is_some() || !self.has_document || self.slideshow.is_some() || entries(self.kind()).is_empty() {
            self.key_tips = None;
        } else {
            self.key_tips = Some(KeyTips::default());
        }
        cx.notify();
    }

    /// A key while the key tips are shown, or Alt+digit: whether it was taken.
    pub(crate) fn quick_access_key(&mut self, keystroke: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let key = keystroke.key.as_str();
        let single = key.chars().count() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric());
        // Alt+1…9 at once (Option+digit types a character on a Mac).
        let alt_only = keystroke.modifiers.alt && !keystroke.modifiers.control && !keystroke.modifiers.platform && !keystroke.modifiers.shift;
        if self.key_tips.is_none() {
            if alt_only && !crate::is_mac() && self.has_document && self.slideshow.is_none() && matches!(key.parse::<usize>(), Ok(1..=9)) {
                let index = key.parse::<usize>().unwrap_or(1) - 1;
                if index < entries(self.kind()).len() {
                    self.run_quick_access(index, window, cx);
                    return true;
                }
            }
            return false;
        }
        if !single || keystroke.modifiers.control || keystroke.modifiers.platform {
            self.key_tips = None;
            cx.notify();
            return key == "escape";
        }
        let mut typed = self.key_tips.as_ref().map(|k| k.typed.clone()).unwrap_or_default();
        typed.push_str(&key.to_ascii_uppercase());
        let labels = key_tip_labels(entries(self.kind()).len());
        if let Some(index) = labels.iter().position(|l| *l == typed) {
            self.key_tips = None;
            cx.notify();
            self.run_quick_access(index, window, cx);
        } else if labels.iter().any(|l| l.starts_with(&typed)) {
            self.key_tips = Some(KeyTips { typed });
            cx.notify();
        } else {
            self.key_tips = None;
            cx.notify();
        }
        true
    }

    /// The quick access toolbar: its buttons (each with its key tip while
    /// shown) and the ▼ to change it.
    pub(crate) fn render_quick_access(&self, cx: &mut Context<Self>) -> AnyElement {
        let editor = cx.entity().downgrade();
        let buttons = self.quick_access_buttons();
        let labels = key_tip_labels(buttons.len());
        let typed = self.key_tips.as_ref().map(|k| k.typed.clone());
        {
            let mut bounds = self.quick_access_bounds.borrow_mut();
            bounds.clear();
            bounds.resize(buttons.len(), None);
        }
        let caret_editor = editor.clone();
        h_flex()
            .id("quick-access")
            .flex_none()
            .items_center()
            .gap(px(1.))
            .children(buttons.iter().enumerate().map(|(i, button)| {
                let tip = labels.get(i).cloned();
                let record = self.quick_access_bounds.clone();
                let badge = typed.as_ref().zip(tip.clone()).filter(|(typed, tip)| tip.starts_with(typed.as_str())).map(|(_, tip)| key_tip_badge(tip));
                let entry = QatEntry::new(button.id, button.icon.unwrap_or(""), &button.tooltip);
                let mut shown = button.clone();
                if let Some(tip) = &tip {
                    shown.tooltip = format!("{} ({})", i18n::text(&button.tooltip), i18n::format("qat.key-tip", &[("key", tip)])).into();
                }
                let menu_editor = editor.clone();
                div()
                    .relative()
                    .flex_none()
                    .child(crate::shell::chrome::tool_button(&shown, 2000 + i, &editor))
                    .child(crate::shell::editor::bounds_observer(move |b, _, _| {
                        if let Some(slot) = record.borrow_mut().get_mut(i) {
                            *slot = Some(b);
                        }
                    }))
                    .children(badge)
                    .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                        let entry = entry.clone();
                        menu_editor.update(cx, |this, cx| this.open_quick_access_context(entry, event.position, window, cx)).ok();
                    })
            }))
            .child(
                Button::new("quick-access-customize")
                    .ghost()
                    .xsmall()
                    .icon(Icon::new(Material("arrow_drop_down")))
                    .tooltip(i18n::text("qat.customize"))
                    .w(px(18.))
                    .h(px(28.))
                    .rounded(px(4.))
                    .dropdown_menu(move |menu, window, cx| {
                        let entries = caret_editor.upgrade().map(|e| e.read(cx).quick_access_customize_entries()).unwrap_or_default();
                        crate::shell::chrome::fill_menu(menu.min_w(px(240.)), &entries, &caret_editor, window, cx)
                    }),
            )
            .into_any_element()
    }

    // ----- The customize dialog --------------------------------------------

    pub(crate) fn open_quick_access_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.has_document {
            return;
        }
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(i18n::text("qat.dialog.search")));
        let subscription = cx.subscribe_in(&search, window, |_, _, event: &InputEvent, _, cx| {
            if let InputEvent::Change = event {
                cx.notify();
            }
        });
        search.update(cx, |input, cx| input.focus(window, cx));
        self.quick_access_dialog = Some(QatDialog { search, _search_subscription: subscription, selected: None });
        cx.notify();
    }

    fn close_quick_access_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.quick_access_dialog = None;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// The customize dialog: the commands that can be added (by group,
    /// with a search) on the left, the toolbar's on the right.
    pub(crate) fn render_quick_access_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.quick_access_dialog.as_ref()?;
        let kind = self.kind();
        let theme = cx.theme();
        let (border, muted, accent, background) = (theme.border, theme.muted_foreground, theme.accent, theme.background);
        let query = dialog.search.read(cx).value().to_string().to_lowercase();
        let current = entries(kind);
        let commands = self.quick_access_commands();
        let names: HashMap<String, SharedString> = commands.iter().map(|c| (c.entry.id.clone(), c.label.clone())).collect();
        let shown: Vec<Command> = commands.into_iter().filter(|c| query.is_empty() || c.label.to_lowercase().contains(&query) || c.entry.id.contains(&query)).collect();
        let mut available = v_flex().gap(px(1.));
        let mut last_group: Option<SharedString> = None;
        for (i, command) in shown.into_iter().enumerate() {
            if last_group.as_ref() != Some(&command.group) {
                available = available.child(div().pt(px(8.)).pb(px(2.)).px(px(6.)).text_xs().font_weight(FontWeight::MEDIUM).text_color(muted).child(command.group.clone()));
                last_group = Some(command.group.clone());
            }
            let added = current.iter().any(|e| e.id == command.entry.id);
            let entry = command.entry.clone();
            available = available.child(
                command_row(("qat-command", i), &command.entry.icon, command.label.clone(), added, muted)
                    .when(!added, |d| d.cursor_pointer().hover(|s| s.bg(accent)))
                    .when(!added, |d| {
                        d.on_click(cx.listener(move |this, _, _, cx| {
                            add(kind, entry.clone());
                            if let Some(dialog) = this.quick_access_dialog.as_mut() {
                                dialog.selected = Some(entries(kind).len() - 1);
                            }
                            this.quick_access_changed(cx);
                        }))
                    }),
            );
        }
        let selected = dialog.selected.filter(|s| *s < current.len());
        let labels = key_tip_labels(current.len());
        let rows = current.iter().enumerate().map(|(i, entry)| {
            let name = names.get(&entry.id).cloned().unwrap_or_else(|| i18n::text(&entry.tooltip));
            let tip = labels.get(i).cloned().unwrap_or_default();
            h_flex()
                .id(("qat-current", i))
                .gap(px(8.))
                .px(px(6.))
                .h(px(28.))
                .items_center()
                .rounded(px(4.))
                .cursor_pointer()
                .when(selected == Some(i), |d| d.bg(accent))
                .hover(|s| s.bg(accent))
                .child(div().w(px(22.)).text_xs().text_color(muted).child(tip))
                .child(Icon::new(Material(icon_or_default(&entry.icon))).size(px(18.)))
                .child(div().flex_1().truncate().text_sm().child(name))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(dialog) = this.quick_access_dialog.as_mut() {
                        dialog.selected = Some(i);
                    }
                    cx.notify();
                }))
        });
        let step = |id: &'static str, icon: &'static str, tooltip: &'static str, delta: isize| {
            let enabled = selected.is_some_and(|s| {
                let to = s as isize + delta;
                to >= 0 && (to as usize) < current.len()
            });
            Button::new(id).ghost().small().icon(Icon::new(Material(icon))).tooltip(i18n::text(tooltip)).disabled(!enabled).on_click(cx.listener(move |this, _, _, cx| {
                let Some(dialog) = this.quick_access_dialog.as_mut() else { return };
                let Some(s) = dialog.selected else { return };
                move_by(kind, s, delta);
                dialog.selected = Some((s as isize + delta) as usize);
                this.quick_access_changed(cx);
            }))
        };
        let remove_button = Button::new("qat-remove").ghost().small().icon(Icon::new(Material("close"))).tooltip(i18n::text("qat.remove")).disabled(selected.is_none()).on_click(cx.listener(move |this, _, _, cx| {
            let Some(dialog) = this.quick_access_dialog.as_mut() else { return };
            let Some(s) = dialog.selected else { return };
            if let Some(entry) = entries(kind).get(s) {
                remove(kind, &entry.id);
            }
            let left = entries(kind).len();
            dialog.selected = (left > 0).then(|| s.min(left - 1));
            this.quick_access_changed(cx);
        }));
        let column = |title: &'static str| div().text_sm().font_weight(FontWeight::MEDIUM).child(i18n::text(title));
        Some(
            div()
                .id("qat-dialog-backdrop")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgba(0x00000066))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    v_flex()
                        .id("qat-dialog")
                        .w(px(680.))
                        .h(px(480.))
                        .bg(background)
                        .border_1()
                        .border_color(border)
                        .rounded_md()
                        .shadow_xl()
                        .p_5()
                        .gap_3()
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .child(div().font_weight(FontWeight::BOLD).text_base().child(i18n::text("qat.dialog.title")))
                                .child(Button::new("qat-dialog-close").ghost().small().icon(Icon::new(Material("close"))).on_click(cx.listener(|this, _, window, cx| this.close_quick_access_dialog(window, cx)))),
                        )
                        .child(div().text_xs().text_color(muted).child(i18n::text("qat.dialog.note")))
                        .child(
                            h_flex()
                                .flex_1()
                                .min_h_0()
                                .gap_4()
                                .items_start()
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .h_full()
                                        .min_h_0()
                                        .gap_2()
                                        .child(column("qat.dialog.available"))
                                        .child(Input::new(&dialog.search).small())
                                        .child(div().id("qat-available").flex_1().min_h_0().overflow_y_scroll().border_1().border_color(border).rounded(px(4.)).p(px(4.)).child(available)),
                                )
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .h_full()
                                        .min_h_0()
                                        .gap_2()
                                        .child(column("qat.dialog.current"))
                                        .child(div().id("qat-current-list").flex_1().min_h_0().overflow_y_scroll().border_1().border_color(border).rounded(px(4.)).p(px(4.)).child(v_flex().gap(px(1.)).children(rows)))
                                        .child(
                                            h_flex()
                                                .gap_1()
                                                .child(step("qat-up", "arrow_upward", "qat.move-up", -1))
                                                .child(step("qat-down", "arrow_downward", "qat.move-down", 1))
                                                .child(remove_button)
                                                .child(div().flex_1())
                                                .child(Button::new("qat-reset").small().label(i18n::text("qat.reset")).on_click(cx.listener(move |this, _, _, cx| {
                                                    reset(kind);
                                                    if let Some(dialog) = this.quick_access_dialog.as_mut() {
                                                        dialog.selected = None;
                                                    }
                                                    this.quick_access_changed(cx);
                                                }))),
                                        ),
                                ),
                        )
                        .child(h_flex().justify_end().child(Button::new("qat-dialog-done").primary().small().label(i18n::text("qat.dialog.close")).on_click(cx.listener(|this, _, window, cx| this.close_quick_access_dialog(window, cx))))),
                )
                .into_any_element(),
        )
    }
}

/// The icon of a kept entry, or a plain one.
fn icon_or_default(icon: &str) -> &'static str {
    if icon.is_empty() { "tune" } else { intern(icon) }
}

fn fallback_icon(entry: &QatEntry) -> &'static str {
    icon_or_default(&entry.icon)
}

/// A row of the dialog's list of commands.
fn command_row(id: impl Into<ElementId>, icon: &str, label: SharedString, added: bool, muted: Hsla) -> Stateful<Div> {
    h_flex()
        .id(id)
        .gap(px(8.))
        .px(px(6.))
        .h(px(28.))
        .items_center()
        .rounded(px(4.))
        .when(added, |d| d.text_color(muted))
        .child(Icon::new(Material(icon_or_default(icon))).size(px(18.)))
        .child(div().flex_1().truncate().text_sm().child(label))
        .when(added, |d| d.child(Icon::new(Material("check")).size(px(16.))))
}

/// A key tip: the letters in a dark box at the button's lower right.
fn key_tip_badge(tip: String) -> impl IntoElement {
    div()
        .absolute()
        .right(px(-2.))
        .bottom(px(-4.))
        .px(px(3.))
        .rounded(px(3.))
        .bg(rgb(0x3c4043))
        .text_color(rgb(0xffffff))
        .text_size(px(11.))
        .line_height(px(14.))
        .font_weight(FontWeight::MEDIUM)
        .child(tip)
}

#[cfg(test)]
mod tests {
    use super::{QatEntry, can_add, key_tip_labels, parse, serialize};

    #[test]
    fn a_list_reads_back_as_it_was_kept() {
        let list = vec![QatEntry::new("file.save", "save", "file.save"), QatEntry::new("bold", "format_bold", "_shell.bold-b")];
        assert_eq!(parse(&serialize(&list)), list);
        assert_eq!(parse(""), Vec::new());
    }

    #[test]
    fn a_kept_list_has_each_id_once_and_no_empty_ones() {
        let list = parse("file.save|save|file.save;;|x|y;file.save|save|again;bold");
        assert_eq!(list.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["file.save", "bold"]);
        assert_eq!(list[1].icon, "");
    }

    #[test]
    fn the_separators_never_get_into_a_field() {
        let entry = QatEntry::new("a;b", "c|d", "e\nf");
        assert_eq!(parse(&serialize(std::slice::from_ref(&entry))), vec![entry]);
    }

    #[test]
    fn key_tips_are_numbered_as_offices() {
        let tips = key_tip_labels(20);
        assert_eq!(tips[..9], ["1", "2", "3", "4", "5", "6", "7", "8", "9"]);
        assert_eq!(tips[9..18], ["09", "08", "07", "06", "05", "04", "03", "02", "01"]);
        assert_eq!(tips[18..], ["0A", "0B"]);
        assert_eq!(key_tip_labels(100).len(), 44);
        assert_eq!(key_tip_labels(100).last().map(String::as_str), Some("0Z"));
    }

    #[test]
    fn list_items_made_per_thing_cannot_be_added() {
        assert!(can_add("file.save"));
        assert!(!can_add("file.recent.item"));
        assert!(!can_add("pptx.layout.apply"));
        assert!(!can_add(""));
    }
}
