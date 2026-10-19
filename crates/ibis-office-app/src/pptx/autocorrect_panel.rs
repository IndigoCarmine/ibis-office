// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ツール > オートコレクトのオプション… (PPTX-FILE-014): the presentation's side of the AutoCorrect that
//! `shell::autocorrect` holds (the settings, the corrections and the panel), with the switches for
//! correcting and for marking misspelled words as text is typed.

use gpui_kit::*;

use crate::shell::autocorrect::{AutoCorrectPanel, PanelAccess, render_panel, set_settings};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

impl SlideEditor {
    pub(crate) fn open_pptx_autocorrect_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let panel = AutoCorrectPanel::new(window, cx);
        if let Some(s) = self.pptx_mut() {
            s.ink = None;
            s.autocorrect = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn apply_pptx_autocorrect_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.autocorrect.take()) {
            set_settings(p.working);
        }
        cx.notify();
    }

    pub(crate) fn pptx_autocorrect_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.pptx_mut()?.autocorrect.as_ref()?;
        render_panel(
            self,
            PanelAccess {
                get: |e| e.pptx_mut().and_then(|s| s.autocorrect.as_mut()),
                apply: |e, cx| e.apply_pptx_autocorrect_panel(cx),
                close: |e| {
                    if let Some(s) = e.pptx_mut() {
                        s.autocorrect = None;
                    }
                },
            },
            cx,
        )
    }
}
