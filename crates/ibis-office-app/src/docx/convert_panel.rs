// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Document format conversion panel and workflow (DOCX-SEC-019, DOCX-FILE-010, DOCX-FILE-012, DOCX-FILE-013).
//!
//! Supports bidirectional conversion between `.docx` and legacy Word 97-2003 (`.doc`),
//! Rich Text Format (`.rtf`), Plain Text (`.txt`), Web HTML (`.html`/`.htm`),
//! OpenDocument Text (`.odt`), as well as PDF-to-Word reflow import (`.pdf` -> `.docx`).

use std::path::{Path, PathBuf};

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use ibis_pptx::docx_convert::{DocxConvertFormat, export_docx_format, import_docx_format};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// Sidebar panel state for document format conversion (Export / Import).
pub(crate) struct ConvertDocumentPanel {
    pub(crate) export_format: DocxConvertFormat,
    pub(crate) export_path_input: Entity<InputState>,
    pub(crate) import_path_input: Entity<InputState>,
    pub(crate) status_message: Option<String>,
    pub(crate) error_message: Option<String>,
}

impl ConvertDocumentPanel {
    pub(crate) fn new(
        default_export_path: &str,
        window: &mut Window,
        cx: &mut Context<SlideEditor>,
    ) -> Self {
        let exp_str = default_export_path.to_string();
        let export_path_input = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(exp_str, window, cx);
            s
        });
        let import_path_input = cx.new(|cx| InputState::new(window, cx).default_value(""));
        Self {
            export_format: DocxConvertFormat::Doc,
            export_path_input,
            import_path_input,
            status_message: None,
            error_message: None,
        }
    }
}

impl SlideEditor {
    /// ファイル / ツール > 文書形式の変換…: open the format conversion panel.
    pub(crate) fn open_docx_convert_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.doc_state().is_none() {
            return;
        }
        let default_export = self
            .file_path()
            .map(|p| p.with_extension("doc").to_string_lossy().to_string())
            .unwrap_or_else(|| "Document.doc".to_string());
        let panel = ConvertDocumentPanel::new(&default_export, window, cx);
        if let Some(state) = self.doc_state_mut() {
            state.find = None;
            state.link = None;
            state.style_edit = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.picture_edit = None;
            state.table_size = None;
            state.table_formula = None;
            state.comments = None;
            state.ruby = None;
            state.text_box = None;
            state.merge = None;
            state.helper = super::help_panels::HelpPanels::default();
            state.docx_math = None;
            state.quick_parts = None;
            state.source_manager = None;
            state.thesaurus = None;
            state.print_panel = None;
            state.inspect_panel = None;
            state.compatibility_panel = None;
            state.repair = None;
            state.template_panel = None;
            state.convert_panel = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Close the document format conversion panel.
    pub(crate) fn close_docx_convert_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            if state.convert_panel.take().is_some() {
                cx.notify();
            }
        }
    }

    /// Whether the document format conversion panel is currently open.
    pub(crate) fn is_docx_convert_panel_open(&self) -> bool {
        self.doc_state()
            .and_then(|s| s.convert_panel.as_ref())
            .is_some()
    }

    /// Set the selected export format in the conversion panel and update the file extension.
    pub(crate) fn set_docx_convert_export_format(
        &mut self,
        format: DocxConvertFormat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(panel) = self.doc_state_mut().and_then(|s| s.convert_panel.as_mut()) else {
            return;
        };
        panel.export_format = format;
        let current = panel.export_path_input.read(cx).value().to_string();
        let trimmed = current.trim();
        let next_path = if trimmed.is_empty() {
            format!("Document.{}", format.extension())
        } else {
            PathBuf::from(trimmed)
                .with_extension(format.extension())
                .to_string_lossy()
                .to_string()
        };
        panel
            .export_path_input
            .update(cx, |input, cx| input.set_value(next_path, window, cx));
        cx.notify();
    }

    /// Export the current Word document to the given format and file path.
    pub(crate) fn export_docx_to_file(
        &mut self,
        format: DocxConvertFormat,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> Result<usize, String> {
        let pres = self.presentation().clone();
        let bytes = export_docx_format(&pres, format).map_err(|e| e.to_string())?;
        let len = bytes.len();
        std::fs::write(path, &bytes).map_err(|e| format!("{}: {}", path.display(), e))?;
        self.set_status(
            format!("{}: {}", text("docx.convert.exported"), path.display()),
            cx,
        );
        Ok(len)
    }

    /// Import an external document (`.doc`, `.rtf`, `.txt`, `.html`, `.odt`, `.pdf`) into the editor as a Word document.
    pub(crate) fn import_docx_from_file(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<DocxConvertFormat, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
        let format = DocxConvertFormat::from_path(path)
            .or_else(|| DocxConvertFormat::sniff(&bytes))
            .unwrap_or(DocxConvertFormat::Txt);
        let opened = import_docx_format(&bytes, format).map_err(|e| e.to_string())?;
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Imported".to_string());
        let doc_name: SharedString = format!("{stem}.docx").into();
        self.load(opened.presentation, doc_name, window, cx);
        self.source = Some(std::sync::Arc::new(opened.source));
        self.set_file_path(Some(path.with_extension("docx")));
        self.set_status(
            format!("{}: {}", text("docx.convert.imported"), path.display()),
            cx,
        );
        Ok(format)
    }

    /// Execute export from the conversion panel UI.
    pub(crate) fn execute_docx_export_from_panel(&mut self, cx: &mut Context<Self>) {
        let (format, path_str) = {
            let Some(panel) = self.doc_state().and_then(|s| s.convert_panel.as_ref()) else {
                return;
            };
            (
                panel.export_format,
                panel.export_path_input.read(cx).value().trim().to_string(),
            )
        };
        if path_str.is_empty() {
            if let Some(panel) = self.doc_state_mut().and_then(|s| s.convert_panel.as_mut()) {
                panel.error_message = Some(text("docx.convert.error-no-path").to_string());
                panel.status_message = None;
            }
            cx.notify();
            return;
        }
        let path = PathBuf::from(&path_str);
        match self.export_docx_to_file(format, &path, cx) {
            Ok(bytes_written) => {
                if let Some(panel) = self.doc_state_mut().and_then(|s| s.convert_panel.as_mut()) {
                    panel.status_message = Some(format!(
                        "{}: {} ({} B)",
                        text("docx.convert.exported"),
                        path.display(),
                        bytes_written
                    ));
                    panel.error_message = None;
                }
            }
            Err(err) => {
                if let Some(panel) = self.doc_state_mut().and_then(|s| s.convert_panel.as_mut()) {
                    panel.error_message = Some(err);
                    panel.status_message = None;
                }
            }
        }
        cx.notify();
    }

    /// Execute import from the conversion panel UI.
    pub(crate) fn execute_docx_import_from_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path_str = {
            let Some(panel) = self.doc_state().and_then(|s| s.convert_panel.as_ref()) else {
                return;
            };
            panel.import_path_input.read(cx).value().trim().to_string()
        };
        if path_str.is_empty() {
            if let Some(panel) = self.doc_state_mut().and_then(|s| s.convert_panel.as_mut()) {
                panel.error_message = Some(text("docx.convert.error-no-path").to_string());
                panel.status_message = None;
            }
            cx.notify();
            return;
        }
        let path = PathBuf::from(&path_str);
        match self.import_docx_from_file(&path, window, cx) {
            Ok(_fmt) => {
                let default_export = path.with_extension("doc").to_string_lossy().to_string();
                let mut new_panel = ConvertDocumentPanel::new(&default_export, window, cx);
                new_panel
                    .import_path_input
                    .update(cx, |input, cx| input.set_value(path_str, window, cx));
                new_panel.status_message = Some(format!(
                    "{}: {}",
                    text("docx.convert.imported"),
                    path.display()
                ));
                if let Some(state) = self.doc_state_mut() {
                    state.convert_panel = Some(new_panel);
                }
                self.sidebar_open = true;
            }
            Err(err) => {
                if let Some(panel) = self.doc_state_mut().and_then(|s| s.convert_panel.as_mut()) {
                    panel.error_message = Some(err);
                    panel.status_message = None;
                }
            }
        }
        cx.notify();
    }

    /// Render the document format conversion sidebar panel.
    pub(crate) fn doc_convert_panel(&self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.doc_state()?.convert_panel.as_ref()?;
        let export_format = panel.export_format;
        let export_input = panel.export_path_input.clone();
        let import_input = panel.import_path_input.clone();
        let status = panel.status_message.clone();
        let error = panel.error_message.clone();

        let format_buttons: [(&'static str, &'static str, DocxConvertFormat); 5] = [
            ("conv-fmt-doc", "docx.convert.format.doc", DocxConvertFormat::Doc),
            ("conv-fmt-rtf", "docx.convert.format.rtf", DocxConvertFormat::Rtf),
            ("conv-fmt-txt", "docx.convert.format.txt", DocxConvertFormat::Txt),
            ("conv-fmt-html", "docx.convert.format.html", DocxConvertFormat::Html),
            ("conv-fmt-odt", "docx.convert.format.odt", DocxConvertFormat::Odt),
        ];

        let mut fmt_list = v_flex().gap_1();
        for (btn_id, label_key, fmt) in format_buttons {
            let selected = export_format == fmt;
            fmt_list = fmt_list.child(
                Button::new(btn_id)
                    .label(text(label_key))
                    .small()
                    .when(selected, |b| b.primary())
                    .when(!selected, |b| b.ghost())
                    .on_click(cx.listener(move |editor, _, window, cx| {
                        editor.set_docx_convert_export_format(fmt, window, cx);
                    })),
            );
        }

        let body = v_flex()
            .gap_3()
            .p_3()
            .text_sm()
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(text("docx.convert.desc")),
            )
            // Export section
            .child(
                v_flex()
                    .gap_2()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(text("docx.convert.export-title")),
                    )
                    .child(fmt_list)
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(text("docx.convert.export-path")),
                    )
                    .child(Input::new(&export_input))
                    .child(
                        h_flex().justify_end().child(
                            Button::new("docx-convert-export-btn")
                                .label(text("docx.convert.export-btn"))
                                .small()
                                .primary()
                                .on_click(cx.listener(|editor, _, _, cx| {
                                    editor.execute_docx_export_from_panel(cx);
                                })),
                        ),
                    ),
            )
            // Import section (.doc / .rtf / .txt / .html / .odt / .pdf)
            .child(
                v_flex()
                    .gap_2()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(text("docx.convert.import-title")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(text("docx.convert.import-desc")),
                    )
                    .child(Input::new(&import_input))
                    .child(
                        h_flex().justify_end().child(
                            Button::new("docx-convert-import-btn")
                                .label(text("docx.convert.import-btn"))
                                .small()
                                .primary()
                                .on_click(cx.listener(|editor, _, window, cx| {
                                    editor.execute_docx_import_from_panel(window, cx);
                                })),
                        ),
                    ),
            )
            .when_some(status, |this, msg| {
                this.child(
                    div()
                        .p_2()
                        .rounded_md()
                        .bg(cx.theme().success.opacity(0.15))
                        .text_color(cx.theme().success)
                        .text_xs()
                        .child(msg),
                )
            })
            .when_some(error, |this, err| {
                this.child(
                    div()
                        .p_2()
                        .rounded_md()
                        .bg(cx.theme().danger.opacity(0.15))
                        .text_color(cx.theme().danger)
                        .text_xs()
                        .child(err),
                )
            })
            .child(
                Button::new("convert-close-btn")
                    .label(text("docx.convert.close"))
                    .ghost()
                    .on_click(cx.listener(|editor, _, _, cx| {
                        editor.close_docx_convert_panel(cx);
                    })),
            );

        Some(SidebarPanel {
            title: text("docx.convert.title").into(),
            body: body.into_any_element(),
        })
    }
}
