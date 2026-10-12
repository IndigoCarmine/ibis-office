// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Document accessibility checking (DOCX-ACCESS-001, DOCX-ACCESS-004,
//! DOCX-ACCESS-005, DOCX-ACCESS-006): scanning for missing alt text,
//! missing table headers, skipped heading levels, and ambiguous links.

use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Block, FlowDocument, FlowPos, FlowRunKind, ShapeKind};
use crate::shell::{chrome::SidebarPanel, editor::SlideEditor, icons::Material};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IssueCategory {
    Error,
    Warning,
    Notice,
}

#[derive(Clone, Debug)]
pub(crate) struct AccessibilityIssue {
    pub(crate) at: FlowPos,
    pub(crate) category: IssueCategory,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) target_name: String,
}

/// Scan document for accessibility issues.
pub(crate) fn check_document_accessibility(doc: &FlowDocument) -> Vec<AccessibilityIssue> {
    let mut issues = Vec::new();

    // 1. Check headings sequence (DOCX-ACCESS-004: 見出し階層)
    let mut prev_heading_level = 0u32;
    for b in &doc.body {
        if let Block::Paragraph(p) = b {
            if let Some(level) = ibis_doc_model::flow_command::heading_level(&doc.styles, p) {
                if level > prev_heading_level + 1 && prev_heading_level > 0 {
                    let text = p.text();
                    let preview = if text.len() > 20 { format!("{}...", &text[..text.floor_char_boundary(20)]) } else { text };
                    issues.push(AccessibilityIssue {
                        at: FlowPos::new(p.id, 0),
                        category: IssueCategory::Warning,
                        title: crate::shell::i18n::text("docx.accessibility.heading-skip").to_string(),
                        description: crate::shell::i18n::format(
                            "_docx.heading-level-skipped-from-prev-to-curr-title",
                            &[("prev", &prev_heading_level.to_string()), ("curr", &level.to_string()), ("title", &preview)],
                        ).to_string(),
                        target_name: crate::shell::i18n::format("_docx.heading-level", &[("level", &level)]),
                    });
                }
                prev_heading_level = level;
            }
        }
    }

    // 2. Scan all blocks (body and table cells) for pictures, tables, and links
    fn scan_blocks(blocks: &[Block], doc: &FlowDocument, issues: &mut Vec<AccessibilityIssue>) {
        let _ = doc;
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    let mut offset = 0;
                    for r in &p.runs {
                        match &r.kind {
                            FlowRunKind::Picture { look, .. } => {
                                // DOCX-ACCESS-002, DOCX-ACCESS-003: Check missing alt text
                                if !look.decorative && look.alt_text.title.trim().is_empty() && look.alt_text.description.trim().is_empty() {
                                    let name = if look.name.is_empty() { crate::shell::i18n::text("docx.common.figure").to_string() } else { look.name.clone() };
                                    issues.push(AccessibilityIssue {
                                        at: FlowPos::new(p.id, offset),
                                        category: IssueCategory::Error,
                                        title: crate::shell::i18n::text("docx.accessibility.missing-alt").to_string(),
                                        description: crate::shell::i18n::text("docx.accessibility.missing-alt.desc").to_string(),
                                        target_name: name,
                                    });
                                }
                            }
                            FlowRunKind::Shape(shape) => {
                                if let ShapeKind::Picture(pic) = &shape.kind {
                                    if !pic.decorative && pic.alt_text.title.trim().is_empty() && pic.alt_text.description.trim().is_empty() {
                                        let name = if pic.name.is_empty() { crate::shell::i18n::text("docx.common.figure").to_string() } else { pic.name.clone() };
                                        issues.push(AccessibilityIssue {
                                            at: FlowPos::new(p.id, offset),
                                            category: IssueCategory::Error,
                                            title: crate::shell::i18n::text("docx.accessibility.missing-alt").to_string(),
                                            description: crate::shell::i18n::text("docx.accessibility.missing-alt.desc").to_string(),
                                            target_name: name,
                                        });
                                    }
                                }
                            }
                            FlowRunKind::Text(text) => {
                                // DOCX-ACCESS-006: Check ambiguous links
                                if r.link.is_some() {
                                    let trimmed = text.trim();
                                    let lower = trimmed.to_lowercase();
                                    let is_url = lower.starts_with("http://") || lower.starts_with("https://");
                                    let is_ambiguous = is_url
                                        || trimmed == "ここをクリック" // i18n-ok（照合の語（リンク文字列の判定に使う））
                                        || trimmed == "クリック" // i18n-ok（照合の語（リンク文字列の判定に使う））
                                        || trimmed == "リンク" // i18n-ok（照合の語（リンク文字列の判定に使う））
                                        || lower == "click here"
                                        || lower == "link"
                                        || lower == "here";
                                    if is_ambiguous {
                                        issues.push(AccessibilityIssue {
                                            at: FlowPos::new(p.id, offset),
                                            category: IssueCategory::Notice,
                                            title: crate::shell::i18n::text("docx.accessibility.ambiguous-link").to_string(),
                                            description: crate::shell::i18n::format(
                                                "_docx.unclear-link-text-text-use-descriptive-text-that",
                                                &[("text", &trimmed)],
                                            ).to_string(),
                                            target_name: trimmed.to_string(),
                                        });
                                    }
                                }
                            }
                            _ => {}
                        }
                        offset += r.len();
                    }
                }
                Block::Table(table) => {
                    // DOCX-ACCESS-005: Table missing header row
                    let has_header = table.rows.first().is_some_and(|r| r.header);
                    if !has_header && !table.rows.is_empty() {
                        let first_pos = table.rows[0].cells.first()
                            .and_then(|c| c.blocks.first())
                            .and_then(|b| if let Block::Paragraph(p) = b { Some(FlowPos::new(p.id, 0)) } else { None })
                            .unwrap_or(FlowPos::new(table.id, 0));
                        issues.push(AccessibilityIssue {
                            at: first_pos,
                            category: IssueCategory::Warning,
                            title: crate::shell::i18n::text("docx.accessibility.table-header").to_string(),
                            description: crate::shell::i18n::text("docx.accessibility.table-header.desc").to_string(),
                            target_name: crate::shell::i18n::format("_docx.table-rows-rows-columns-columns", &[("rows", &table.rows.len()), ("columns", &table.columns.len())]),
                        });
                    }
                    // Scan cells recursively
                    for row in &table.rows {
                        for cell in &row.cells {
                            scan_blocks(&cell.blocks, doc, issues);
                        }
                    }
                }
            }
        }
    }

    scan_blocks(&doc.body, doc, &mut issues);
    issues
}

impl SlideEditor {
    /// Open Accessibility Checker pane.
    pub(crate) fn open_accessibility_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.accessibility_panel = true;
            state.selection_pane = false;
            state.picture_edit = None;
            state.table_size = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.style_edit = None;
            state.link = None;
            state.find = None;
            state.comments = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Returns current accessibility issues in the document.
    #[allow(dead_code)]
    pub(crate) fn accessibility_issues(&self) -> Vec<AccessibilityIssue> {
        let Some(flow) = self.flow() else { return Vec::new(); };
        check_document_accessibility(flow)
    }

    /// Jump to accessibility issue location and select target.
    pub(crate) fn jump_to_accessibility_issue(&mut self, at: FlowPos, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.anchor = Some(at);
            state.caret = Some(FlowPos::new(at.para, at.offset + 1));
        }
        cx.notify();
    }

    /// Accessibility sidebar panel UI.
    pub(crate) fn accessibility_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let is_open = self.doc_state().is_some_and(|s| s.accessibility_panel);
        if !is_open { return None; }
        let flow = self.flow()?;
        let issues = check_document_accessibility(flow);

        let mut body = v_flex().gap_3().px(px(18.)).py(px(8.));

        if issues.is_empty() {
            body = body.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .p_3()
                    .rounded_md()
                    .bg(rgb(0xe6f4ea))
                    .child(Icon::new(Material("check_circle")).size(px(20.)).text_color(rgb(0x137333)))
                    .child(
                        v_flex()
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(rgb(0x137333)).child(crate::shell::i18n::text("docx.accessibility.passed")))
                            .child(div().text_xs().text_color(rgb(0x1e8e3e)).child(crate::shell::i18n::text("_docx.no-accessibility-issues-were-found-in-the-docume")))
                    )
            );
        } else {
            let error_count = issues.iter().filter(|i| i.category == IssueCategory::Error).count();
            let warn_count = issues.iter().filter(|i| i.category == IssueCategory::Warning).count();
            let notice_count = issues.iter().filter(|i| i.category == IssueCategory::Notice).count();

            body = body.child(
                div().text_xs().text_color(rgb(0x5f6368)).child(
                    crate::shell::i18n::format(
                        "_docx.check-results-errors-errors-warnings-warnings-no",
                        &[("errors", &error_count.to_string()), ("warnings", &warn_count.to_string()), ("notices", &notice_count.to_string())],
                    )
                )
            );

            for (idx, issue) in issues.into_iter().enumerate() {
                let at = issue.at;
                let (icon_name, icon_color, bg_color) = match issue.category {
                    IssueCategory::Error => ("error", rgb(0xd93025), rgb(0xfce8e6)),
                    IssueCategory::Warning => ("warning", rgb(0xf29900), rgb(0xfef7e0)),
                    IssueCategory::Notice => ("info", rgb(0x1a73e8), rgb(0xe8f0fe)),
                };

                let item = v_flex()
                    .id(("acc-issue", idx))
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .bg(bg_color)
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(0xf1f3f4)))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(Icon::new(Material(icon_name)).size(px(16.)).text_color(icon_color))
                            .child(div().flex_1().text_sm().font_weight(FontWeight::MEDIUM).child(issue.title))
                            .child(div().text_xs().text_color(rgb(0x5f6368)).child(issue.target_name))
                    )
                    .child(
                        div().text_xs().text_color(rgb(0x3c4043)).pl(px(24.)).child(issue.description)
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.jump_to_accessibility_issue(at, cx);
                    }));

                body = body.child(item);
            }
        }

        Some(SidebarPanel {
            title: crate::shell::i18n::text("docx.accessibility.title"),
            body: body.into_any_element(),
        })
    }
}

