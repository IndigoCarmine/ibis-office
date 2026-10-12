// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 表示 > ウィンドウの整列 (Arrange Windows):
//! Arrange multiple workbook windows (Tiled, Horizontal, Vertical, Cascade)
//! with option for windows of active workbook (XLSX-SHEET-017).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum ArrangeKind {
    #[default]
    Tiled,
    Horizontal,
    Vertical,
    Cascade,
}

pub(crate) struct ArrangeWindowsPanel {
    pub(crate) kind: ArrangeKind,
    pub(crate) active_workbook_only: bool,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    pub(crate) fn toggle_arrange_windows_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.wb().panels.arrange_windows.is_some() {
            self.wb_mut().panels.arrange_windows = None;
            self.sidebar_open = false;
            cx.notify();
        } else {
            self.open_arrange_windows_panel(cx);
        }
    }

    pub(crate) fn open_arrange_windows_panel(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        self.wb_mut().panels.close();
        self.wb_mut().panels.arrange_windows = Some(ArrangeWindowsPanel {
            kind: ArrangeKind::Tiled,
            active_workbook_only: false,
            ui: Rc::new(RefCell::new(BTreeMap::new())),
        });
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn set_arrange_kind(&mut self, kind: ArrangeKind, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.arrange_windows.as_mut() {
            p.kind = kind;
        }
        cx.notify();
    }

    pub(crate) fn toggle_arrange_active_workbook_only(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.arrange_windows.as_mut() {
            p.active_workbook_only = !p.active_workbook_only;
        }
        cx.notify();
    }

    pub(crate) fn apply_arrange_windows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (kind, active_only) = match self.wb().panels.arrange_windows.as_ref() {
            Some(p) => (p.kind, p.active_workbook_only),
            None => (ArrangeKind::Tiled, false),
        };
        self.wb_mut().panels.arrange_windows = None;
        self.sidebar_open = false;
        cx.notify();

        let current_handle = window.window_handle();
        let active_path = self.file_path();
        let active_name = self.workbook_display_name();

        Self::arrange_windows_impl(kind, active_only, Some((window, current_handle)), active_path, active_name, cx);
    }

    pub(crate) fn arrange_windows_impl(
        kind: ArrangeKind,
        active_only: bool,
        current: Option<(&mut Window, AnyWindowHandle)>,
        active_path: Option<PathBuf>,
        active_name: String,
        cx: &mut App,
    ) {
        let all_windows = cx.windows();
        if all_windows.is_empty() {
            return;
        }

        let current_h = current.as_ref().map(|(_, h)| *h);
        let mut targets = Vec::new();
        for &w in &all_windows {
            if current_h.is_some_and(|h| w == h) {
                targets.push(w);
            } else if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                let is_target = root
                    .update(cx, |root, _, app| {
                        if let Ok(editor_entity) = root.view().clone().downcast::<SlideEditor>() {
                            let ed = editor_entity.read(app);
                            if active_only {
                                if ed.is_workbook() {
                                    if let (Some(p1), Some(p2)) = (&active_path, &ed.file_path()) {
                                        p1 == p2
                                    } else {
                                        active_name == ed.workbook_display_name()
                                    }
                                } else if ed.is_document() {
                                    if let (Some(p1), Some(p2)) = (&active_path, &ed.file_path()) {
                                        p1 == p2
                                    } else {
                                        active_name == ed.file_name.as_deref().unwrap_or_default()
                                    }
                                } else {
                                    false
                                }
                            } else {
                                ed.is_workbook() || ed.is_document() || ed.is_presentation()
                            }
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);
                if is_target {
                    targets.push(w);
                }
            }
        }

        if targets.is_empty() {
            if let Some(h) = current_h {
                targets.push(h);
            } else {
                targets = all_windows;
            }
        }

        let count = targets.len();
        let (screen_w, screen_h) = cx
            .primary_display()
            .map(|d| {
                let b = d.visible_bounds();
                (f32::from(b.size.width), f32::from(b.size.height))
            })
            .unwrap_or((1280.0, 800.0));

        let (tile_w, tile_h) = match kind {
            ArrangeKind::Tiled => {
                let (cols, rows) = match count {
                    1 => (1, 1),
                    2 => (2, 1),
                    3 | 4 => (2, 2),
                    5 | 6 => (3, 2),
                    _ => {
                        let cols = (count as f32).sqrt().ceil() as usize;
                        let rows = ((count as f32) / (cols as f32)).ceil() as usize;
                        (cols, rows)
                    }
                };
                (screen_w / cols as f32, screen_h / rows as f32)
            }
            ArrangeKind::Horizontal => {
                let cols = 1;
                let rows = count;
                (screen_w / cols as f32, screen_h / rows as f32)
            }
            ArrangeKind::Vertical => {
                let cols = count;
                let rows = 1;
                (screen_w / cols as f32, screen_h / rows as f32)
            }
            ArrangeKind::Cascade => {
                let base_w = (screen_w * 0.8).clamp(640.0, screen_w);
                let base_h = (screen_h * 0.8).clamp(480.0, screen_h);
                (base_w, base_h)
            }
        };

        let (mut cur_window, cur_handle) = match current {
            Some((w, h)) => (Some(w), Some(h)),
            None => (None, None),
        };

        for target in targets {
            if let (Some(w), Some(h)) = (cur_window.as_mut(), cur_handle) {
                if target == h {
                    w.resize(size(px(tile_w), px(tile_h)));
                    w.bounds_changed(cx);
                    continue;
                }
            }
            target
                .update(cx, |_, window, cx| {
                    window.resize(size(px(tile_w), px(tile_h)));
                    window.bounds_changed(cx);
                })
                .ok();
        }
    }

    pub(crate) fn arrange_windows_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.arrange_windows.as_ref()?;
        let kind = p.kind;
        let active_only = p.active_workbook_only;

        let ui = p.ui.clone();
        let mark = move |id: &str, el: AnyElement| {
            let id = id.to_string();
            let ui = ui.clone();
            div()
                .relative()
                .child(el)
                .child(
                    canvas(|_, _, _| {}, move |bounds, _, _, _| {
                        ui.borrow_mut().insert(id.clone(), bounds);
                    })
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
                .into_any_element()
        };

        let mut body = v_flex().gap_2().px(px(16.)).py(px(8.));

        body = body.child(
            div()
                .text_size(px(13.))
                .font_weight(FontWeight::BOLD)
                .child(text("fix.arrange-windows.arrange")),
        );

        // Radio group
        let radio_btn = |mode: ArrangeKind, label_key: &'static str, id_btn: &'static str, id_mark: &'static str| {
            let selected = kind == mode;
            mark(
                id_mark,
                Button::new(id_btn)
                    .small()
                    .ghost()
                    .w_full()
                    .label(format!("{} {}", if selected { "●" } else { "○" }, text(label_key)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_arrange_kind(mode, cx);
                    }))
                    .into_any_element(),
            )
        };

        body = body.child(radio_btn(ArrangeKind::Tiled, "fix.arrange-windows.tiled", "btn-arrange-tiled", "arrange-tiled"));
        body = body.child(radio_btn(ArrangeKind::Horizontal, "fix.arrange-windows.horizontal", "btn-arrange-horizontal", "arrange-horizontal"));
        body = body.child(radio_btn(ArrangeKind::Vertical, "fix.arrange-windows.vertical", "btn-arrange-vertical", "arrange-vertical"));
        body = body.child(radio_btn(ArrangeKind::Cascade, "fix.arrange-windows.cascade", "btn-arrange-cascade", "arrange-cascade"));

        body = body.child(div().h(px(8.)));

        // Checkbox: active workbook only
        body = body.child(mark(
            "arrange-active-only",
            Button::new("btn-arrange-active-only")
                .small()
                .ghost()
                .w_full()
                .label(format!("{} {}", if active_only { "☑" } else { "☐" }, text("fix.arrange-windows.windows-active-workbook")))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_arrange_active_workbook_only(cx);
                }))
                .into_any_element(),
        ));

        // Buttons
        let mut buttons_row = h_flex().gap_2().justify_end().pt(px(12.));
        buttons_row = buttons_row.child(mark(
            "arrange-ok",
            Button::new("btn-arrange-ok").primary()
                .small()
                .label(text("OK"))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.apply_arrange_windows(window, cx);
                }))
                .into_any_element(),
        ));
        buttons_row = buttons_row.child(mark(
            "arrange-cancel",
            Button::new("btn-arrange-cancel")
                .small()
                .ghost()
                .label(text("_shell.cancel"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.wb_mut().panels.arrange_windows = None;
                    this.sidebar_open = false;
                    cx.notify();
                }))
                .into_any_element(),
        ));
        body = body.child(buttons_row);

        Some(SidebarPanel {
            title: text("fix.arrange-windows.arrange-windows").into(),
            body: body.into_any_element(),
        })
    }
}
