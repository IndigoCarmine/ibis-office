// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Arbitrary RGB for text and underline, using existing text edit commands.
use gpui_kit::*;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input,InputEvent,InputState};
use gpui_kit::component::{Sizable as _,v_flex};
use ibis_doc_model::{FlowPos,Rgb};
use crate::shell::{chrome::SidebarPanel,editor::SlideEditor};
pub(crate) struct ColorEditState {
    pub(crate) field:Entity<InputState>,
    target:(FlowPos,FlowPos),
    underline:bool,
    applied:Option<(String,std::sync::Arc<ibis_doc_model::FlowDocument>)>,
    pub(crate) error:Option<String>,
    _subscription:Subscription,
}
impl SlideEditor {
    pub(crate) fn open_color_edit(&mut self,underline:bool,window:&mut Window,cx:&mut Context<Self>) {
        let Some(target)=self.ordered_selection() else{return};
        let Some(style)=self.current_format().map(|(_,style)|style) else{return};
        let color=if underline {style.underline_color.unwrap_or(style.color)} else {style.color};
        let initial=format!("{:06X}",color.0);
        let field=cx.new(|cx| {let mut input=InputState::new(window,cx);input.set_value(initial.clone(),window,cx);input});
        let subscription=cx.subscribe_in(&field,window,|this,_,event:&InputEvent,window,cx| {
            if let InputEvent::PressEnter {..}=event {this.apply_color_edit(window,cx);}
        });
        if let Some(state)=self.doc_state_mut() {
            state.color_edit=Some(ColorEditState {field,target,underline,applied:None,error:None,_subscription:subscription});
            state.font_edit=None; state.paragraph_edit=None;state.picture_edit=None;state.table_size=None;state.style_edit=None;state.link=None;state.find=None;state.comments=None;
        }
        self.sidebar_open=true;cx.notify();
    }
    pub(crate) fn apply_color_edit(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        let Some(edit)=self.doc_state().and_then(|s|s.color_edit.as_ref()) else{return};
        let value=edit.field.read(cx).value().trim().strip_prefix('#').unwrap_or(edit.field.read(cx).value().trim()).to_string();
        let target=edit.target;let underline=edit.underline;
        let color=(value.len()==6 && value.bytes().all(|b|b.is_ascii_hexdigit())).then(||u32::from_str_radix(&value,16).ok()).flatten();
        if color.is_none() || self.ordered_selection()!=Some(target) {
            if let Some(edit)=self.doc_state_mut().and_then(|s|s.color_edit.as_mut()) {edit.error=Some(crate::shell::i18n::text("docx.color-panel.check-6-digit-rgb-value").into());}
            cx.notify();return;
        }
        let normalized=format!("{:06X}",color.unwrap());
        if !edit.applied.as_ref().is_some_and(|(value,doc)| value==&normalized && self.flow()==Some(doc)) {
            if underline {self.set_underline_color(Some(Rgb(color.unwrap())),cx);} else {self.set_text_color(Rgb(color.unwrap()),cx);}
        }
        let applied=self.flow().cloned().map(|doc|(normalized,doc));
        if let Some(edit)=self.doc_state_mut().and_then(|s|s.color_edit.as_mut()) {edit.applied=applied;edit.error=None;}
        self.focus_handle.focus(window,cx);cx.notify();
    }
    pub(crate) fn color_panel(&mut self,cx:&mut Context<Self>)->Option<SidebarPanel> {
        let edit=self.doc_state()?.color_edit.as_ref()?;
        let body=v_flex().gap_2().px(px(18.)).py(px(8.))
            .child(div().text_sm().child(crate::shell::i18n::text("docx.color-panel.rgb-6-digit-hex")))
            .child(Input::new(&edit.field).small())
            .children(edit.error.clone().map(|error|div().text_sm().child(error)))
            .child(Button::new("docx-color-apply").primary().small().label(crate::shell::i18n::text("_shell.apply")).on_click(cx.listener(|this,_,window,cx|this.apply_color_edit(window,cx))));
        Some(SidebarPanel {title:crate::shell::i18n::text(if edit.underline {"docx.color-panel.underline-color"} else {"docx.toolbar.automatic"}),body:body.into_any_element()})
    }
}
