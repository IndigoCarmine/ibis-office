// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 開発 > XML: XML ソース (XML maps panel) および XML マップのプロパティ
//! (XML map properties panel).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Selectable as _, Sizable as _, v_flex, h_flex};
use gpui_kit::*;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// Information on an XML Map in the workbook.
#[derive(Clone, Debug)]
pub(crate) struct XmlMapInfo {
    pub(crate) name: String,
    pub(crate) root_element: String,
    pub(crate) schema: String,
    pub(crate) validate: bool,
    pub(crate) overwrite: bool,
}

impl Default for XmlMapInfo {
    fn default() -> Self {
        Self {
            name: "Map1".to_string(),
            root_element: "Root".to_string(),
            schema: "Schema1".to_string(),
            validate: true,
            overwrite: true,
        }
    }
}

pub(crate) struct XmlSourcePanel {
    pub(crate) maps: Vec<XmlMapInfo>,
    pub(crate) selected: Option<usize>,
    #[allow(dead_code)]
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

pub(crate) struct XmlMapPropertiesPanel {
    pub(crate) map_index: Option<usize>,
    pub(crate) name: Entity<InputState>,
    pub(crate) root_element: String,
    pub(crate) schema: String,
    pub(crate) validate: bool,
    pub(crate) overwrite: bool,
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    /// 開発 > XML > ソース: open the XML Source panel.
    pub(crate) fn open_xml_source(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let state = self.wb_mut();
        state.panels.close();
        let mut maps = Vec::new();
        // If no XML map exists, seed a default Map1 for user management.
        maps.push(XmlMapInfo::default());
        state.panels.xml_source = Some(XmlSourcePanel {
            maps,
            selected: Some(0),
            message: String::new(),
            ui: Rc::new(RefCell::new(BTreeMap::new())),
        });
        cx.notify();
    }

    /// 開発 > XML > XML マップのプロパティ: open the XML Map Properties panel.
    pub(crate) fn open_xml_map_properties(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (map_index, map) = if let Some(p) = self.wb().panels.xml_source.as_ref() {
            let idx = p.selected.unwrap_or(0);
            let m = p.maps.get(idx).cloned().unwrap_or_default();
            (Some(idx), m)
        } else {
            (None, XmlMapInfo::default())
        };
        let name_state = cx.new(|cx| InputState::new(window, cx));
        name_state.update(cx, |inp, cx| inp.set_value(map.name.clone(), window, cx));

        let state = self.wb_mut();
        state.panels.close();
        state.panels.xml_map_properties = Some(XmlMapPropertiesPanel {
            map_index,
            name: name_state,
            root_element: map.root_element,
            schema: map.schema,
            validate: map.validate,
            overwrite: map.overwrite,
            message: String::new(),
            ui: Rc::new(RefCell::new(BTreeMap::new())),
        });
        cx.notify();
    }

    /// Apply the edited XML Map properties.
    pub(crate) fn apply_xml_map_properties(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.wb().panels.xml_map_properties.as_ref() else { return };
        let new_name = p.name.read(cx).value().trim().to_string();
        if new_name.is_empty() {
            if let Some(p_mut) = self.wb_mut().panels.xml_map_properties.as_mut() {
                p_mut.message = text("xlsx.xml-map-properties.name-required").to_string();
            }
            cx.notify();
            return;
        }
        let validate = p.validate;
        let overwrite = p.overwrite;
        let map_index = p.map_index;

        if let Some(source) = self.wb_mut().panels.xml_source.as_mut() {
            if let Some(idx) = map_index {
                if let Some(m) = source.maps.get_mut(idx) {
                    m.name = new_name;
                    m.validate = validate;
                    m.overwrite = overwrite;
                }
            }
        }
        self.wb_mut().panels.xml_map_properties = None;
        cx.notify();
    }

    /// Render the XML Source sidebar panel.
    pub(crate) fn xml_source_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.wb().panels.xml_source.as_ref()?;
        let ui = p.ui.clone();
        let mark = |id: &str, child: AnyElement| {
            let ui = ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
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

        let mut body = v_flex().gap_2().p(px(8.));

        body = body.child(div().text_size(px(12.)).font_weight(FontWeight::BOLD).child(text("xlsx.xml-source.maps-in-workbook")));

        if p.maps.is_empty() {
            body = body.child(div().text_size(px(11.)).text_color(rgb(0x888888)).child(text("xlsx.xml-source.no-maps")));
        } else {
            for (i, m) in p.maps.iter().enumerate() {
                let is_sel = p.selected == Some(i);
                let label = format!("{} ({}: {})", m.name, text("xlsx.xml-map-properties.root-element"), m.root_element);
                body = body.child(
                    Button::new(ElementId::Name(format!("xml-map-{i}").into()))
                        .small()
                        .label(label)
                        .selected(is_sel)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(source) = this.wb_mut().panels.xml_source.as_mut() {
                                source.selected = Some(i);
                            }
                            cx.notify();
                        }))
                );
            }
        }

        body = body.child(h_flex().gap_2().pt(px(6.)).child(
            mark("xml-map-props-btn", Button::new("xml-map-props-btn")
                .small()
                .label(text("xlsx.xml-map-properties.title"))
                .on_click(cx.listener(|this, _, window, cx| this.open_xml_map_properties(window, cx)))
                .into_any_element())
        ).child(
            Button::new("xml-source-close")
                .small()
                .label(text("_shell.close"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.wb_mut().panels.xml_source = None;
                    cx.notify();
                }))
        ));

        Some(SidebarPanel {
            title: text("xlsx.xml-source.title"),
            body: body.into_any_element(),
        })
    }

    /// Render the XML Map Properties sidebar panel.
    pub(crate) fn xml_map_properties_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.wb().panels.xml_map_properties.as_ref()?;
        let ui = p.ui.clone();
        let mark = |id: &str, child: AnyElement| {
            let ui = ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
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

        let check = |on: bool, label: &str| format!("{} {}", if on { "☑" } else { "☐" }, text(label));
        let validate = p.validate;
        let overwrite = p.overwrite;
        let root = p.root_element.clone();
        let schema = p.schema.clone();

        let mut body = v_flex().gap_2().p(px(8.));

        body = body.child(div().text_size(px(11.)).font_weight(FontWeight::BOLD).child(text("xlsx.xml-map-properties.name")));
        body = body.child(mark("xml-map-name-input", Input::new(&p.name).small().into_any_element()));

        body = body.child(div().text_size(px(11.)).text_color(rgb(0x666666)).child(format!("{}: {}", text("xlsx.xml-map-properties.root-element"), root)));
        body = body.child(div().text_size(px(11.)).text_color(rgb(0x666666)).child(format!("{}: {}", text("xlsx.xml-map-properties.schema"), schema)));

        body = body.child(
            Button::new("xml-map-validate-toggle")
                .small()
                .label(check(validate, "xlsx.xml-map-properties.validate"))
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(p) = this.wb_mut().panels.xml_map_properties.as_mut() {
                        p.validate = !p.validate;
                    }
                    cx.notify();
                }))
        );

        body = body.child(
            Button::new("xml-map-overwrite-toggle")
                .small()
                .label(check(overwrite, "xlsx.xml-map-properties.overwrite"))
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(p) = this.wb_mut().panels.xml_map_properties.as_mut() {
                        p.overwrite = !p.overwrite;
                    }
                    cx.notify();
                }))
        );

        if !p.message.is_empty() {
            body = body.child(div().text_size(px(11.)).text_color(rgb(0xD9534F)).child(SharedString::from(p.message.clone())));
        }

        body = body.child(h_flex().gap_2().pt(px(6.)).child(
            mark("xml-map-ok-btn", Button::new("xml-map-ok-btn")
                .primary()
                .small()
                .label(text("OK"))
                .on_click(cx.listener(|this, _, _, cx| this.apply_xml_map_properties(cx)))
                .into_any_element())
        ).child(
            Button::new("xml-map-close-btn")
                .small()
                .label(text("_shell.close"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.wb_mut().panels.xml_map_properties = None;
                    cx.notify();
                }))
        ));

        Some(SidebarPanel {
            title: text("xlsx.xml-map-properties.title"),
            body: body.into_any_element(),
        })
    }
}
