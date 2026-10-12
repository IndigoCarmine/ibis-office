// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ハイパーリンク (挿入 > リンク…, ⌘K): the text selected, or the shape
//! selected, made a link to a web page or file, or to a slide of the
//! presentation (or the next, previous, first or last), or the link taken
//! away. A link of text takes the look the file gives it (underlined, in
//! the theme's link colour: read back through the file). In the slide show
//! a click on a link follows it; in the editor ⌘ and a click.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{CharacterFormat, Command, EmuPoint, ShapeId, SlideId, SlideLink};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// What the link goes on.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LinkTarget {
    /// Characters `start..end` of a shape's text.
    Text { shape: ShapeId, start: usize, end: usize },
    Shape(ShapeId),
}

/// The panel's choices.
pub(crate) struct LinkPanel {
    pub(crate) target: LinkTarget,
    pub(crate) slide: SlideId,
    /// A web page or file (`true`), else a place in the presentation.
    pub(crate) web: bool,
    pub(crate) url: Entity<InputState>,
    /// The place chosen in the presentation.
    pub(crate) place: Option<SlideLink>,
    /// ScreenTip (ヒント).
    pub(crate) tooltip: Entity<InputState>,
}

/// The link of `shape` on `slide`: its own, else its text's when the text
/// has one link only.
pub(crate) fn link_of_shape(slide: &ibis_doc_model::Slide, shape: ShapeId) -> Option<SlideLink> {
    if let Some(l) = slide.shape_links.get(&shape) {
        return Some(l.clone());
    }
    let body = slide.shape(shape)?.text_body()?;
    let mut links = body.paragraphs.iter().flat_map(|p| p.runs.iter()).filter_map(|r| r.style.link.clone());
    let first = links.next()?;
    links.all(|l| l == first).then_some(first)
}

/// The link tooltip of `shape` on `slide`: its own, else its text's when the text has a link tooltip.
pub(crate) fn link_tooltip_of_shape(slide: &ibis_doc_model::Slide, shape: ShapeId) -> Option<String> {
    if let Some(t) = slide.shape_link_tooltips.get(&shape) {
        return Some(t.clone());
    }
    let body = slide.shape(shape)?.text_body()?;
    body.paragraphs.iter().flat_map(|p| p.runs.iter()).find_map(|r| r.style.link_tooltip.clone())
}

impl SlideEditor {
    /// ⌘K: the panel for the text selected (while editing) or the shape selected.
    pub(crate) fn slide_open_link_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let target = match self.text_editing() {
            Some(e) if e.cell.is_none() && !e.range().is_empty() => LinkTarget::Text { shape: e.shape, start: e.range().start, end: e.range().end },
            Some(e) if e.cell.is_none() => LinkTarget::Shape(e.shape),
            Some(_) => return self.set_status(crate::shell::i18n::text("pptx.links.links-text-table-cells-not").to_string(), cx),
            None => match self.selected_ids().as_slice() {
                [one] => LinkTarget::Shape(*one),
                _ => return,
            },
        };
        if self.text_editing().is_some() {
            self.end_text_edit(window, cx);
        }
        let shape = match &target {
            LinkTarget::Text { shape, .. } | LinkTarget::Shape(shape) => *shape,
        };
        let existing = link_of_shape(self.slide(), shape);
        let existing_tooltip = link_tooltip_of_shape(self.slide(), shape);
        let url_text = match &existing {
            Some(SlideLink::Url(u)) => u.clone(),
            _ => String::new(),
        };
        let web = !matches!(existing, Some(ref l) if !matches!(l, SlideLink::Url(_)));
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("https://").default_value(url_text));
        url.update(cx, |f, cx| f.focus(window, cx));
        let tooltip_text = existing_tooltip.unwrap_or_default();
        let tooltip = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("pptx.links.screentip-text")).default_value(tooltip_text));
        let place = existing.filter(|l| !matches!(l, SlideLink::Url(_)));
        if let Some(s) = self.pptx_mut() {
            s.link_panel = Some(LinkPanel { target, slide, web, url, place, tooltip });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn slide_close_link_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.link_panel = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// The link chosen in the panel, if one is.
    fn slide_chosen_link(&self, cx: &App) -> Option<SlideLink> {
        let p = self.pptx()?.link_panel.as_ref()?;
        if p.web {
            let url = p.url.read(cx).value().trim().to_string();
            (!url.is_empty()).then(|| SlideLink::Url(if url.contains(':') { url } else { format!("https://{url}") }))
        } else {
            p.place.clone()
        }
    }

    /// 適用 (`link`: the panel's choice) or リンクの削除 (`None`).
    pub(crate) fn slide_apply_link(&mut self, link: Option<SlideLink>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.pptx().and_then(|s| s.link_panel.as_ref()) else { return };
        let (target, slide) = (p.target.clone(), p.slide);
        let tooltip_str = p.tooltip.read(cx).value().trim().to_string();
        let tooltip = if link.is_some() && !tooltip_str.is_empty() {
            Some(tooltip_str)
        } else {
            None
        };
        match target {
            LinkTarget::Shape(shape) => {
                let mut cmds = Vec::new();
                if self.presentation().slide(slide).and_then(|s| s.shape_links.get(&shape)) != link.as_ref() {
                    cmds.push(Command::SetShapeLink { slide, shape, link });
                }
                if self.presentation().slide(slide).and_then(|s| s.shape_link_tooltips.get(&shape)).cloned() != tooltip {
                    cmds.push(Command::SetShapeLinkTooltip { slide, shape, tooltip });
                }
                if !cmds.is_empty() {
                    self.run(cmds, cx);
                }
            }
            LinkTarget::Text { shape, start, end } => {
                let Some(mut changed) = self.presentation().slide(slide).and_then(|s| s.shape(shape)).cloned() else { return };
                let Some(body) = changed.text_body_mut() else { return };
                // Taken away: the text as the rest of its body (no underline, its colour).
                let format = match &link {
                    Some(l) => CharacterFormat {
                        link: Some(Some(l.clone())),
                        link_tooltip: Some(tooltip),
                        ..Default::default()
                    },
                    None => CharacterFormat {
                        link: Some(None),
                        link_tooltip: Some(None),
                        underline: Some(false),
                        color: Some(body.style.color),
                        ..Default::default()
                    },
                };
                body.format_runs(start, end, &format);
                let kind = match &self.source {
                    Some(source) => {
                        let mut p = self.presentation().clone();
                        if let Some(t) = p.slides.iter_mut().find(|s| s.id == slide).and_then(|s| s.shapes.iter_mut().find(|x| x.id == shape)) {
                            *t = changed.clone();
                        }
                        match source.as_read(&p, slide, shape) {
                            Ok(read) => read.kind,
                            Err(err) => {
                                log::warn!("Could not add the link: {err}");
                                changed.kind
                            }
                        }
                    }
                    None => changed.kind,
                };
                if self.presentation().slide(slide).and_then(|s| s.shape(shape)).is_some_and(|s| s.kind != kind) {
                    self.run(vec![Command::SetKind { slide, shape, kind: Box::new(kind) }], cx);
                }
            }
        }
        self.slide_close_link_panel(window, cx);
    }

    /// Follow `link`: a page opened in the browser, or the slide gone to
    /// (in the show, or in the editor).
    pub(crate) fn slide_follow_link(&mut self, link: &SlideLink, window: &mut Window, cx: &mut Context<Self>) {
        let slides = &self.presentation().slides;
        let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        let to = match link {
            SlideLink::Url(url) => {
                cx.open_url(url);
                return;
            }
            SlideLink::Slide(id) => slides.iter().position(|s| s.id == *id),
            SlideLink::NextSlide => (index + 1 < slides.len()).then_some(index + 1),
            SlideLink::PreviousSlide => index.checked_sub(1),
            SlideLink::FirstSlide => Some(0),
            SlideLink::LastSlide => Some(slides.len() - 1),
            SlideLink::LastSlideViewed => index.checked_sub(1),
            SlideLink::EndShow => None,
        };
        let Some(to) = to else { return };
        self.show_slide_number(to + 1, window, cx);
        cx.notify();
    }

    /// The link under `p` (slide coordinates) of the slide shown, if any.
    pub(crate) fn slide_link_at(&self, p: EmuPoint) -> Option<SlideLink> {
        self.slide_shape_and_link_at(p).map(|(_, l)| l)
    }

    /// The shape and its link under `p` (slide coordinates) of the slide shown, if any.
    pub(crate) fn slide_shape_and_link_at(&self, p: EmuPoint) -> Option<(ibis_doc_model::ShapeId, SlideLink)> {
        let slide = self.slide();
        let shape = slide.hit_test(p)?;
        let link = link_of_shape(slide, shape)?;
        Some((shape, link))
    }

    pub(crate) fn slide_link_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.link_panel.as_ref()?;
        let t = crate::shell::i18n::text;
        let web = p.web;
        let place = p.place.clone();
        let tab = |id: &'static str, label: &'static str, on: bool, web: bool| {
            Button::new(id).small().label(t(label)).when(!on, |b| b.ghost()).on_click(cx.listener(move |e, _, _, cx| {
                if let Some(p) = e.pptx_mut().and_then(|s| s.link_panel.as_mut()) {
                    p.web = web;
                }
                cx.notify();
            }))
        };
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).child(h_flex().gap_1().child(tab("link-web", "pptx.links.web-page-file", web, true)).child(tab("link-place", "pptx.links.place-document", !web, false)));
        if web {
            body = body.child(div().text_sm().child(t("pptx.links.address"))).child(Input::new(&p.url).small());
        } else {
            let mut places: Vec<(SlideLink, String)> = vec![
                (SlideLink::FirstSlide, t("pptx.links.first-slide").to_string()),
                (SlideLink::LastSlide, t("pptx.links.last-slide").to_string()),
                (SlideLink::NextSlide, t("pptx.links.next-slide").to_string()),
                (SlideLink::PreviousSlide, t("pptx.links.previous-slide").to_string()),
                (SlideLink::LastSlideViewed, t("pptx.links.last-slide-viewed").to_string()),
                (SlideLink::EndShow, t("pptx.links.end-show").to_string()),
            ];
            for (i, s) in self.presentation().slides.iter().enumerate() {
                let title = s.shapes.iter().find(|sh| s.placeholders.get(&sh.id).is_some_and(|ph| matches!(ph.placeholder.normalized(), "title" | "ctrTitle"))).and_then(|sh| sh.text_body()).map(|b| b.text().replace('\n', " ")).unwrap_or_default();
                places.push((SlideLink::Slide(s.id), format!("{} {}. {title}", t("_shell.slide"), i + 1)));
            }
            let list = v_flex().gap_0p5().children(places.into_iter().enumerate().map(|(i, (link, label))| {
                let on = place.as_ref() == Some(&link);
                div()
                    .id(("link-place-item", i))
                    .px_1()
                    .rounded(px(3.))
                    .text_sm()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .cursor_pointer()
                    .when(on, |d| d.bg(rgb(0xd3e3fd)))
                    .on_click(cx.listener(move |e, _, _, cx| {
                        if let Some(p) = e.pptx_mut().and_then(|s| s.link_panel.as_mut()) {
                            p.place = Some(link.clone());
                        }
                        cx.notify();
                    }))
                    .child(label)
            }));
            body = body.child(list);
        }
        body = body.child(div().text_sm().child(t("pptx.links.screentip"))).child(Input::new(&p.tooltip).small());
        body = body.child(
            h_flex()
                .gap_1()
                .child(Button::new("link-apply").primary().small().label(t("_shell.apply")).on_click(cx.listener(|e, _, w, cx| {
                    if let Some(link) = e.slide_chosen_link(cx) {
                        e.slide_apply_link(Some(link), w, cx);
                    }
                })))
                .child(Button::new("link-remove").small().label(t("pptx.links.remove-link")).on_click(cx.listener(|e, _, w, cx| e.slide_apply_link(None, w, cx))))
                .child(Button::new("link-cancel").small().ghost().label(t("_shell.cancel")).on_click(cx.listener(|e, _, w, cx| e.slide_close_link_panel(w, cx)))),
        );
        Some(SidebarPanel { title: t("pptx.links.hyperlink"), body: body.into_any_element() })
    }
}

