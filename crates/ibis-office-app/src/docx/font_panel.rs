// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Independent Latin, East Asian, and Complex Script font names without font enumeration.
use gpui_kit::*;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input,InputEvent,InputState};
use gpui_kit::component::{Sizable as _,h_flex,v_flex};
use ibis_doc_model::{FlowPos,RunFormat,FlowDocument,Block,RevisionKind};
use ibis_doc_model::flow_command;
use std::sync::Arc;
use crate::shell::{chrome::SidebarPanel,editor::SlideEditor};
pub(crate) struct FontEditState {
    pub(crate) fields:[Entity<InputState>;3],
    target:(FlowPos,FlowPos),
    initial:[String;3],
    document:Arc<FlowDocument>,
    pub(crate) error:Option<String>,
    _subscriptions:Vec<Subscription>,
}
impl SlideEditor {
    fn selected_font_names(&self)->Option<[String;3]> {
        let (start,end)=self.ordered_selection()?;
        if start==end {
            let (_,style)=self.current_format()?;
            return Some([style.font.unwrap_or_default(),style.east_asian_font.unwrap_or_default(),style.cs_font.unwrap_or_default()]);
        }
        let doc=self.flow()?;let (container,first)=flow_command::locate(doc,start.para)?;
        let (other,last)=flow_command::locate(doc,end.para)?;
        if container!=other || first>last {return None;}
        let blocks=flow_command::blocks(doc,container)?;
        let mut fonts:Option<[Option<String>;3]>=None;
        for (index,block) in blocks[first..=last].iter().enumerate() {
            let selected=match block {
                Block::Paragraph(p)=>vec![(p,if index==0 {start.offset} else {0},if first+index==last {end.offset} else {p.len()})],
                Block::Table(_)=>super::block_tree::paragraphs(std::slice::from_ref(block)).into_iter().map(|p|(p,0,p.len())).collect(),
            };
            for (p,a,b) in selected {
            let mut offset=0;
            for run in &p.runs {
                let next=offset+run.len();
                let hidden=!super::flow_layout::showing_markup() && matches!(run.revision.as_ref().map(|r|&r.kind),Some(RevisionKind::Delete));
                if offset<b && next>a && !hidden {
                    let names=[run.style.font.clone(),run.style.east_asian_font.clone(),run.style.cs_font.clone()];
                    if let Some(common)=fonts.as_mut() {for i in 0..3 {if common[i]!=names[i] {common[i]=None;}}} else {fonts=Some(names);}
                }
                offset=next;
            }
            }
        }
        fonts.map(|values|values.map(Option::unwrap_or_default))
    }
    pub(crate) fn open_font_edit(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        let Some(target)=self.ordered_selection() else{return};let Some(document)=self.flow().cloned() else{return};
        let Some(initial)=self.selected_font_names() else{return};
        let fields=initial.clone().map(|value|cx.new(|cx| {let mut input=InputState::new(window,cx);input.set_value(value,window,cx);input}));
        let subscriptions=fields.iter().map(|field|cx.subscribe_in(field,window,|this,_,event:&InputEvent,window,cx| {
            if let InputEvent::PressEnter {..}=event {this.apply_font_edit(window,cx);}
        })).collect();
        if let Some(state)=self.doc_state_mut() {
            state.font_edit=Some(FontEditState {fields,target,initial,document,error:None,_subscriptions:subscriptions});
            state.color_edit=None;state.paragraph_edit=None;state.picture_edit=None;state.table_size=None;state.style_edit=None;state.link=None;state.find=None;state.comments=None;
        }
        self.sidebar_open=true;cx.notify();
    }
    pub(crate) fn apply_font_edit(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        let Some(edit)=self.doc_state().and_then(|s|s.font_edit.as_ref()) else{return};
        let values=edit.fields.clone().map(|field|field.read(cx).value().trim().to_string());
        let changed=[values[0]!=edit.initial[0],values[1]!=edit.initial[1],values[2]!=edit.initial[2]];
        let invalid=values.iter().enumerate().any(|(i,value)|changed[i] && (value.is_empty() || value.chars().count()>128 || value.chars().any(char::is_control)));
        if invalid || self.ordered_selection()!=Some(edit.target) || self.flow()!=Some(&edit.document) {
            if let Some(edit)=self.doc_state_mut().and_then(|s|s.font_edit.as_mut()) {edit.error=Some(crate::shell::i18n::text("docx.font-panel.check-font-name-selection-if").into());}
            cx.notify();return;
        }
        let properties=RunFormat {font:changed[0].then(||values[0].clone()),east_asian_font:changed[1].then(||values[1].clone()),cs_font:changed[2].then(||values[2].clone()),..Default::default()};
        if !changed.into_iter().any(|v|v) || self.apply_text_properties(properties,cx) {
            let document=self.flow().unwrap().clone();
            if let Some(edit)=self.doc_state_mut().and_then(|s|s.font_edit.as_mut()) {edit.initial=values;edit.document=document;edit.error=None;}
            self.focus_handle.focus(window,cx);
        }
        cx.notify();
    }
    pub(crate) fn font_panel(&mut self,cx:&mut Context<Self>)->Option<SidebarPanel> {
        let edit=self.doc_state()?.font_edit.as_ref()?;
        let row=|label:&'static str,field:&Entity<InputState>|h_flex().gap_2().child(div().w(px(140.)).text_sm().child(crate::shell::i18n::text(label))).child(Input::new(field).small());
        let body=v_flex().gap_2().px(px(18.)).py(px(8.))
            .child(div().text_sm().child(crate::shell::i18n::text("docx.font-panel.blank-means-not-specified-mixed")))
            .child(row("docx.font-panel.latin-font",&edit.fields[0]))
            .child(row("docx.font-panel.east-asian-font",&edit.fields[1]))
            .child(row("docx.font-panel.complex-script-font",&edit.fields[2]))
            .children(edit.error.clone().map(|error|div().text_sm().child(error)))
            .child(Button::new("docx-font-apply").primary().small().label(crate::shell::i18n::text("_shell.apply")).on_click(cx.listener(|this,_,window,cx|this.apply_font_edit(window,cx))));
        Some(SidebarPanel {title:crate::shell::i18n::text("_shell.font"),body:body.into_any_element()})
    }
}
