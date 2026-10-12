// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 右クリックメニュー on the slide: for the shapes under the pointer (cut, copy, paste, duplicate,
//! delete, order, group, size and position, alt text, link), for the text being edited (cut, copy,
//! paste), or for the empty slide (paste).
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::*;

use crate::pptx::SlidePopup;
use crate::pptx::editing::Order;
use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
use crate::shell::editor::SlideEditor;

impl SlideEditor {
    /// The menu's entries for what is selected or edited now.
    pub(crate) fn shape_context_menu_entries(&self) -> Vec<MenuEntry> {
        let editing = self.text_editing().is_some();
        let has_selection = !self.selected_ids().is_empty();
        let range = self.text_editing().is_some_and(|e| !e.range().is_empty());
        let can_copy = if editing { range } else { has_selection && self.selected_shapes().iter().all(|s| ibis_pptx::save::check_shape_insertion(s).is_ok()) };
        let mut entries: Vec<MenuEntry> = vec![
            MenuItem::tr("edit.cut").icon("content_cut").shortcut("⌘X").disabled(!can_copy).on(|e, _, cx| {
                if e.text_editing().is_some() {
                    e.copy_edited_text(true, cx);
                } else {
                    e.cut_selected(cx);
                }
            }).into(),
            MenuItem::tr("edit.copy").icon("content_copy").shortcut("⌘C").disabled(!can_copy).on(|e, _, cx| {
                if e.text_editing().is_some() {
                    e.copy_edited_text(false, cx);
                } else {
                    e.copy_selected();
                    e.write_shapes_to_system_clipboard(cx);
                    e.stamp_shape_clipboard(cx);
                }
            }).into(),
            MenuItem::tr("edit.paste").icon("content_paste").shortcut("⌘V").on(|e, w, cx| {
                if !e.paste_edited_text(cx) {
                    e.paste_from_clipboards(w, cx);
                }
            }).into(),
        ];
        if editing || !has_selection {
            return entries;
        }
        entries.extend([
            MenuItem::tr("edit.duplicate").icon("control_point_duplicate").shortcut("⌘D").on(|e, _, cx| e.duplicate_selected(cx)).into(),
            MenuItem::tr("edit.delete").icon("delete").shortcut("⌫").on(|e, _, cx| e.delete_selected(cx)).into(),
            MenuEntry::Separator,
            Submenu::tr("pptx.order", "flip_to_front", vec![
                MenuItem::tr("pptx.order.front").icon("flip_to_front").on(|e, _, cx| e.order_selected(Order::Front, cx)).into(),
                MenuItem::tr("pptx.order.forward").icon("arrow_upward").on(|e, _, cx| e.order_selected(Order::Forward, cx)).into(),
                MenuItem::tr("pptx.order.backward").icon("arrow_downward").on(|e, _, cx| e.order_selected(Order::Backward, cx)).into(),
                MenuItem::tr("pptx.order.back").icon("flip_to_back").on(|e, _, cx| e.order_selected(Order::Back, cx)).into(),
            ]).into(),
        ]);
        if self.can_group() {
            entries.push(MenuItem::tr("pptx.group").icon("group_work").shortcut("⌥⌘G").on(|e, _, cx| e.group_selected(cx)).into());
        }
        if self.can_ungroup() {
            entries.push(MenuItem::tr("pptx.ungroup").icon("workspaces").shortcut("⌥⇧⌘G").on(|e, _, cx| e.ungroup_selected(cx)).into());
        }
        entries.extend([
            MenuEntry::Separator,
            MenuItem::tr("pptx.link").icon("link").shortcut("⌘K").on(|e, w, cx| e.slide_open_link_panel(w, cx)).into(),
            MenuItem::tr("pptx.frame.settings").icon("straighten").on(|e, w, cx| e.open_frame_panel(w, cx)).into(),
            MenuItem::tr("pptx.alt-text").icon("accessibility").on(|e, w, cx| e.open_alt_text_panel(w, cx)).into(),
        ]);
        entries
    }

    /// A right click on the slide at `position` (the window's): the shape under it is selected (when it is not
    /// part of the selection) and the menu opens there.
    pub(crate) fn on_slide_right_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(layout) = self.slide_layout() else { return };
        if self.pptx_tool() != crate::pptx::editing::Tool::Select || self.slideshow.is_some() {
            return;
        }
        cx.stop_propagation();
        let p = crate::pptx::interaction::P::of(layout.to_emu(event.position));
        let slop = crate::pptx::interaction::LINE_SLOP_PX as f64 / layout.scale as f64;
        let hit = crate::pptx::interaction::hit_test(self.slide(), p, slop);
        let in_edited_text = hit.is_some_and(|id| self.text_editing().is_some_and(|e| e.shape == id));
        if !in_edited_text {
            self.end_text_edit(window, cx);
            if let Some(id) = hit {
                if !self.selected_ids().contains(&id) {
                    let mut target = vec![id];
                    for g in &self.slide().groups {
                        let mut leaves = Vec::new();
                        Self::collect_group_leaves(g, &mut leaves);
                        if leaves.contains(&id) {
                            target = leaves;
                            break;
                        }
                    }
                    self.set_selection(target);
                }
            } else {
                self.set_selection(Vec::new());
            }
        }
        self.focus_handle.focus(window, cx);
        let entries = std::rc::Rc::new(self.shape_context_menu_entries());
        let editor = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, window, cx| crate::shell::chrome::fill_menu(menu.min_w(px(220.)), &entries, &editor, window, cx));
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if let Some(state) = this.pptx_mut() {
                state.slide_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        if let Some(state) = self.pptx_mut() {
            state.slide_popup = Some(SlidePopup { menu, position: event.position, _dismiss: dismiss });
        }
        cx.notify();
    }
}
