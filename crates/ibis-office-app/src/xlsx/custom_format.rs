// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 表示形式 > ユーザー定義: a number format code typed for the selected
//! cells, with what the selected cell would show.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{BorderLine, CellAlign, CellVerticalAlign, StyleChange};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// The tabs of セルの書式設定 (Excel's dialog).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FormatTab {
    #[default]
    Number,
    Alignment,
    Font,
    Border,
    Fill,
    Protection,
}

impl FormatTab {
    const ALL: [FormatTab; 6] = [Self::Number, Self::Alignment, Self::Font, Self::Border, Self::Fill, Self::Protection];

    fn label(self) -> &'static str {
        match self {
            Self::Number => "_xlsx.number-2",
            Self::Alignment => "_xlsx.alignment",
            Self::Font => "_shell.font",
            Self::Border => "pptx.custom-format.borders",
            Self::Fill => "pptx.common.fill",
            Self::Protection => "_xlsx.protection",
        }
    }
}

pub(crate) struct FormatPanel {
    pub(crate) code: Entity<InputState>,
    pub(crate) tab: FormatTab,
    /// The font's name and size (点) typed in フォント.
    pub(crate) font: Entity<InputState>,
    pub(crate) size: Entity<InputState>,
    /// Where the buttons are (set when painted), for the scenarios.
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    pub(crate) fn open_custom_format(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_cell_format(FormatTab::Number, window, cx);
    }

    /// セルの書式設定 (⌘1), on the tab `tab`.
    pub(crate) fn open_cell_format(&mut self, tab: FormatTab, window: &mut Window, cx: &mut Context<Self>) {
        let style = self.current_style();
        let code = cx.new(|cx| InputState::new(window, cx).placeholder("#,##0"));
        code.update(cx, |i, cx| {
            i.set_value(style.number_format.clone(), window, cx);
            if tab == FormatTab::Number {
                i.focus(window, cx);
            }
        });
        let font = cx.new(|cx| InputState::new(window, cx).placeholder("游ゴシック")); // i18n-ok（フォント名の見本（入力欄の placeholder）。どの言語でも同じ）
        font.update(cx, |i, cx| i.set_value(style.font.clone().unwrap_or_default(), window, cx));
        let size = cx.new(|cx| InputState::new(window, cx).placeholder("11"));
        size.update(cx, |i, cx| i.set_value(format!("{}", style.font_size as f32 / 100.0), window, cx));
        if let Some(s) = self.sheet_state_mut() {
            s.names = None;
            s.validation = None;
            s.find = None;
            s.link = None;
            s.page_setup = false;
            s.custom_format = Some(FormatPanel { code, tab, font, size, ui: Default::default() });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn custom_format_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.sheet_state()?.custom_format.as_ref()?;
        let (code, font, size, tab, ui) = (panel.code.clone(), panel.font.clone(), panel.size.clone(), panel.tab, panel.ui.clone());
        // A button with its place noted when painted (`ui`).
        let mark_ui = move |id: &str, child: Button| -> AnyElement {
            let ui = ui.clone();
            let id = id.to_string();
            div().relative().child(child).child(canvas(|_, _, _| {}, move |bounds, _, _, _| { ui.borrow_mut().insert(id.clone(), bounds); }).absolute().top_0().left_0().size_full()).into_any_element()
        };
        let style = self.current_style();
        let text = |s: &str| SharedString::from(crate::shell::i18n::text(s).to_string());
        let mark = |on: bool, label: &str| SharedString::from(format!("{} {}", if on { "●" } else { "○" }, crate::shell::i18n::text(label)));
        // A button that changes the selected cells' format.
        let change = |id: String, label: SharedString, change: StyleChange, cx: &mut Context<Self>| {
            mark_ui(&id.clone(), Button::new(SharedString::from(id)).small().label(label).on_click(cx.listener(move |this, _, _, cx| this.format_cells(change.clone(), cx))))
        };
        let mut tabs = h_flex().gap_1().flex_wrap();
        for t in FormatTab::ALL {
            tabs = tabs.child(mark_ui(&format!("cell-format-tab-{t:?}"), Button::new(SharedString::from(format!("cell-format-tab-{t:?}"))).small().label(mark(t == tab, t.label())).on_click(cx.listener(move |this, _, _, cx| {
                if let Some(p) = this.sheet_state_mut().and_then(|s| s.custom_format.as_mut()) {
                    p.tab = t;
                }
                cx.notify();
            }))));
        }
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).child(tabs);
        match tab {
            FormatTab::Number => {
                let typed = code.read(cx).value().to_string();
                // What the selected cell shows in it.
                let sample = self.current_cell().and_then(|(c, r)| self.current_sheet().and_then(|(_, s)| s.cell(r, c).map(|x| x.value.clone()))).map(|v| ibis_calc::numfmt::format(&v, &typed, false).text).unwrap_or_default();
                let c2 = code.clone();
                let mut kinds = h_flex().gap_1().flex_wrap();
                for (i, &(name, format)) in super::toolbar::NUMBER_FORMATS.iter().enumerate() {
                    kinds = kinds.child(change(format!("cell-format-number-{i}"), mark(style.number_format == format, &crate::shell::i18n::text(name)), StyleChange::NumberFormat(format.into()), cx));
                }
                body = body
                    .child(kinds)
                    .child(Input::new(&code).small())
                    .child(div().text_size(px(12.)).child(SharedString::from(format!("{}{sample}", crate::shell::i18n::text("_xlsx.sample")))))
                    .child(Button::new("custom-format-apply").primary().small().label(text("_xlsx.apply-to-selection")).on_click(cx.listener(move |this, _, _, cx| {
                        let code = c2.read(cx).value().trim().to_string();
                        if !code.is_empty() {
                            this.format_cells(StyleChange::NumberFormat(code), cx);
                        }
                    })));
            }
            FormatTab::Alignment => {
                let horizontal = [("_xlsx.general", CellAlign::General), ("_xlsx.left-indent", CellAlign::Left), ("_shell.center-2", CellAlign::Center), ("_xlsx.right-indent", CellAlign::Right), ("_xlsx.fill", CellAlign::Fill), ("align.justify", CellAlign::Justify), ("_xlsx.center-across-selection", CellAlign::CenterAcross), ("_xlsx.distributed-indent", CellAlign::Distributed)];
                let vertical = [("_xlsx.top-2", CellVerticalAlign::Top), ("_shell.center-2", CellVerticalAlign::Center), ("_xlsx.bottom-2", CellVerticalAlign::Bottom), ("align.justify", CellVerticalAlign::Justify), ("_shell.distributed", CellVerticalAlign::Distributed)];
                let mut h = h_flex().gap_1().flex_wrap();
                for (i, (name, a)) in horizontal.into_iter().enumerate() {
                    h = h.child(change(format!("cell-format-halign-{i}"), mark(style.align == a, name), StyleChange::Align(a), cx));
                }
                let mut v = h_flex().gap_1().flex_wrap();
                for (i, (name, a)) in vertical.into_iter().enumerate() {
                    v = v.child(change(format!("cell-format-valign-{i}"), mark(style.vertical == a, name), StyleChange::Vertical(a), cx));
                }
                let mut turn = h_flex().gap_1().flex_wrap();
                for (i, (name, degrees)) in [("0°", 0u8), ("45°", 45), ("90°", 90), ("135°", 135), ("180°", 180), ("_xlsx.vertical-text", 255)].into_iter().enumerate() {
                    turn = turn.child(change(format!("cell-format-rotation-{i}"), mark(style.rotation == degrees, name), StyleChange::Rotation(degrees), cx));
                }
                body = body
                    .child(text("_xlsx.horizontal"))
                    .child(h)
                    .child(text("_xlsx.vertical"))
                    .child(v)
                    .child(h_flex().gap_1().flex_wrap()
                        .child(change("cell-format-wrap".into(), mark(style.wrap, "_xlsx.wrap-text"), StyleChange::Wrap(!style.wrap), cx))
                        .child(change("cell-format-shrink".into(), mark(style.shrink, "_xlsx.shrink-to-fit"), StyleChange::Shrink(!style.shrink), cx)))
                    .child(h_flex().gap_1().items_center()
                        .child(text("_xlsx.indent"))
                        .child(change("cell-format-indent-less".into(), "−".into(), StyleChange::Indent(style.indent.saturating_sub(1)), cx))
                        .child(SharedString::from(format!("{}", style.indent)))
                        .child(change("cell-format-indent-more".into(), "+".into(), StyleChange::Indent(style.indent + 1), cx)))
                    .child(text("_xlsx.text-direction"))
                    .child(turn);
            }
            FormatTab::Font => {
                let (f2, s2) = (font.clone(), size.clone());
                body = body
                    .child(text("_xlsx.font-name"))
                    .child(Input::new(&font).small())
                    .child(Button::new("cell-format-font-set").small().label(text("_xlsx.set-font")).on_click(cx.listener(move |this, _, _, cx| {
                        let name = f2.read(cx).value().trim().to_string();
                        this.format_cells(StyleChange::Font((!name.is_empty()).then_some(name)), cx);
                    })))
                    .child(text("_xlsx.size-points"))
                    .child(Input::new(&size).small())
                    .child(Button::new("cell-format-size-set").small().label(text("_xlsx.set-size")).on_click(cx.listener(move |this, _, _, cx| {
                        if let Ok(points) = s2.read(cx).value().trim().parse::<f32>()
                            && (1.0..=409.0).contains(&points)
                        {
                            this.format_cells(StyleChange::FontSize((points * 100.0).round() as u32), cx);
                        }
                    })))
                    .child(h_flex().gap_1().flex_wrap()
                        .child(change("cell-format-bold".into(), mark(style.bold, "_shell.bold"), StyleChange::Bold(!style.bold), cx))
                        .child(change("cell-format-italic".into(), mark(style.italic, "docx.common.italic"), StyleChange::Italic(!style.italic), cx))
                        .child(change("cell-format-underline".into(), mark(style.underline.enabled(), "_xlsx.underline"), StyleChange::Underline(!style.underline.enabled()), cx))
                        .child(change("cell-format-strike".into(), mark(style.strike, "pptx.chrome.strikethrough"), StyleChange::Strike(!style.strike), cx)))
                    .child(text("docx.common.color"));
                let mut colors = h_flex().gap_1().flex_wrap();
                for (i, &(name, rgb_value)) in super::toolbar::COLORS.iter().enumerate() {
                    colors = colors.child(change(format!("cell-format-color-{i}"), mark(style.color.0 == rgb_value && style.color_theme.is_none(), &crate::shell::i18n::text(name)), StyleChange::Color(ibis_doc_model::Rgb(rgb_value)), cx));
                }
                body = body.child(colors);
            }
            FormatTab::Border => {
                let mut kinds = h_flex().gap_1().flex_wrap();
                for (i, (name, line)) in [("pptx.chrome.solid", BorderLine::Thin), ("_shell.thick", BorderLine::Thick), ("_shell.double", BorderLine::Double), ("_shell.dotted", BorderLine::Dotted), ("_shell.dashed", BorderLine::Dashed)].into_iter().enumerate() {
                    let on = self.wb().border_line == line;
                    kinds = kinds.child(Button::new(SharedString::from(format!("cell-format-line-{i}"))).small().label(mark(on, name)).on_click(cx.listener(move |this, _, _, cx| {
                        this.wb_mut().border_line = line;
                        cx.notify();
                    })));
                }
                let mut places = h_flex().gap_1().flex_wrap();
                for (name, kind) in [("_shell.none", "none"), ("_xlsx.outside-borders", "outside"), ("_xlsx.all-borders", "all"), ("_xlsx.top-border-2", "top"), ("_xlsx.bottom-border-2", "bottom"), ("_xlsx.left-border-2", "left"), ("_xlsx.right-border-2", "right"), ("_xlsx.diagonal-up-border-2", "diagonal-up"), ("_xlsx.diagonal-down-border-2", "diagonal-down")] {
                    places = places.child(mark_ui(&format!("cell-format-border-{kind}"), Button::new(SharedString::from(format!("cell-format-border-{kind}"))).small().label(text(name)).on_click(cx.listener(move |this, _, _, cx| this.set_borders(kind, cx)))));
                }
                let mut colors = h_flex().gap_1().flex_wrap();
                for (i, &(name, rgb_value)) in super::toolbar::COLORS.iter().enumerate() {
                    let on = self.wb().border_color.0 == rgb_value;
                    colors = colors.child(Button::new(SharedString::from(format!("cell-format-border-color-{i}"))).small().label(mark(on, name)).on_click(cx.listener(move |this, _, _, cx| {
                        this.wb_mut().border_color = ibis_doc_model::Rgb(rgb_value);
                        cx.notify();
                    })));
                }
                body = body.child(text("_xlsx.line-style")).child(kinds).child(text("docx.common.color")).child(colors).child(text("_xlsx.borders")).child(places);
            }
            FormatTab::Fill => {
                let mut colors = h_flex().gap_1().flex_wrap();
                colors = colors.child(change("cell-format-fill-none".into(), mark(style.fill.is_none(), "pptx.chrome.no-color"), StyleChange::Fill(None), cx));
                for (i, &(name, rgb_value)) in super::toolbar::FILLS.iter().enumerate() {
                    colors = colors.child(change(format!("cell-format-fill-{i}"), mark(style.fill.is_some_and(|f| f.0 == rgb_value) && style.fill_theme.is_none(), &crate::shell::i18n::text(name)), StyleChange::Fill(Some(ibis_doc_model::Rgb(rgb_value))), cx));
                }
                body = body.child(text("_xlsx.background-color")).child(colors);
            }
            FormatTab::Protection => {
                body = body
                    .child(change("cell-format-locked".into(), mark(!style.unlocked, "_xlsx.locked"), StyleChange::Locked(style.unlocked), cx))
                    .child(change("cell-format-hidden".into(), mark(style.formula_hidden, "_xlsx.hidden-hide-the-formula"), StyleChange::FormulaHidden(!style.formula_hidden), cx))
                    .child(div().text_size(px(12.)).child(text("_xlsx.locked-and-hidden-take-effect-when-the-sheet-is")));
            }
        }
        Some(SidebarPanel { title: "fix.protection.format-cells".into(), body: body.into_any_element() })
    }
}
