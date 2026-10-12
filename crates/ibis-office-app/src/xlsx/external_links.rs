// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > ブックのリンク…: the external workbooks the formulas read, each
//! with 値の更新 (the cached cells made again from its file, found by the
//! path the link names or chosen) and リンクの解除 (its formulas made values).
//! Until updated, the formulas calculate from the values the file cached.
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use gpui_kit::component::button::Button;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Command, SheetCommand};

pub(crate) struct ExternalLinksPanel {
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    pub(crate) fn open_external_links_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let state = self.wb_mut();
        state.custom_format = None;
        state.page_setup = false;
        state.names = None;
        state.validation = None;
        state.find = None;
        state.link = None;
        state.conditional_edit = None;
        state.sort_edit = None;
        state.panels.close();
        state.panels.external_links = Some(ExternalLinksPanel { message: String::new(), ui: Default::default() });
        self.sidebar_open = true;
        cx.notify();
    }

    fn external_links_message(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.external_links.as_mut() {
            p.message = text;
        }
        cx.notify();
    }

    /// The links of the workbook still in use: (index, book).
    pub(crate) fn live_external_books(&self) -> Vec<(usize, ibis_doc_model::ExternalBook)> {
        self.presentation().workbook.as_ref().map(|w| w.external_books.iter().cloned().enumerate().filter(|(_, b)| !b.broken && !b.name.is_empty()).collect()).unwrap_or_default()
    }

    /// 値の更新 of link `index` from the file `data` (its bytes).
    pub(crate) fn refresh_external_book_from(&mut self, index: usize, data: &[u8], cx: &mut Context<Self>) {
        let Some(book) = self.presentation().workbook.as_ref().and_then(|w| w.external_books.get(index)).cloned() else { return };
        match ibis_pptx::refreshed_sheets(&book, data) {
            Ok(sheets) => {
                self.run_sheet(vec![Command::Sheet(SheetCommand::RefreshExternalBook { index, sheets })], cx);
                self.external_links_message(crate::shell::i18n::format("external-links.updated", &[("name", &book.name)]), cx);
            }
            Err(e) => self.external_links_message(e.to_string(), cx),
        }
    }

    /// 値の更新 from the file the link names.
    pub(crate) fn refresh_external_book(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(book) = self.presentation().workbook.as_ref().and_then(|w| w.external_books.get(index)).cloned() else { return };
        let dir = self.file_path().and_then(|p| p.parent().map(std::path::Path::to_path_buf));
        let found = ibis_pptx::link_path(&book.path, dir.as_deref()).and_then(|p| std::fs::read(&p).ok().map(|d| (p, d)));
        match found {
            Some((_, data)) => self.refresh_external_book_from(index, &data, cx),
            None => self.external_links_message(crate::shell::i18n::format("external-links.not-found", &[("path", &book.path)]), cx),
        }
    }

    /// 値の更新 from a file chosen.
    pub(crate) fn refresh_external_book_picked(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some(crate::shell::i18n::text("_xlsx.update").to_string().into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                this.update(cx, |this, cx| this.refresh_external_book_from(index, &data, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Some((_, data))) = crate::web::pick_file().await else { return };
                this.update(cx, |this, cx| this.refresh_external_book_from(index, &data, cx)).ok();
            })
            .detach();
        }
    }

    /// リンクの解除.
    pub(crate) fn break_external_link(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.protection_refuses(crate::xlsx::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(name) = self.presentation().workbook.as_ref().and_then(|w| w.external_books.get(index)).map(|b| b.name.clone()) else { return };
        self.run_sheet(vec![Command::Sheet(SheetCommand::BreakExternalLink { index })], cx);
        self.external_links_message(crate::shell::i18n::format("external-links.broken", &[("name", &name)]), cx);
    }

    pub(crate) fn external_links_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let books = self.live_external_books();
        let p = self.sheet_state()?.panels.external_links.as_ref()?;
        let mark = |id: String, child: AnyElement| {
            let ui = p.ui.clone();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.));
        if books.is_empty() {
            body = body.child(crate::shell::i18n::text("_xlsx.this-workbook-has-no-links"));
        }
        for (index, book) in books {
            let sheets = book.sheets.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ");
            body = body
                .child(div().font_weight(FontWeight::BOLD).child(SharedString::from(book.name.clone())))
                .child(div().text_size(px(12.)).child(SharedString::from(ibis_pptx::link_display(&book.path))))
                .child(div().text_size(px(12.)).child(crate::shell::i18n::format("external-links.sheets", &[("sheets", &sheets)])))
                .child(
                    h_flex()
                        .gap_1()
                        .flex_wrap()
                        .child(mark(format!("update-{index}"), Button::new(SharedString::from(format!("external-update-{index}"))).small().label(crate::shell::i18n::text("_xlsx.update-values")).on_click(cx.listener(move |this, _, _, cx| this.refresh_external_book(index, cx))).into_any_element()))
                        .child(mark(format!("pick-{index}"), Button::new(SharedString::from(format!("external-pick-{index}"))).small().label(crate::shell::i18n::text("_xlsx.update-from-file")).on_click(cx.listener(move |this, _, window, cx| this.refresh_external_book_picked(index, window, cx))).into_any_element()))
                        .child(mark(format!("break-{index}"), Button::new(SharedString::from(format!("external-break-{index}"))).small().label(crate::shell::i18n::text("_xlsx.break-link")).on_click(cx.listener(move |this, _, _, cx| this.break_external_link(index, cx))).into_any_element())),
                );
        }
        body = body.child(div().text_size(px(12.)).child(SharedString::from(p.message.clone()))).child(mark(
            "close".into(),
            Button::new("external-links-close")
                .small()
                .label(crate::shell::i18n::text("_shell.close"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.wb_mut().panels.external_links = None;
                    cx.notify();
                }))
                .into_any_element(),
        ));
        Some(SidebarPanel { title: crate::shell::i18n::text("_xlsx.workbook-links").into(), body: body.into_any_element() })
    }
}
