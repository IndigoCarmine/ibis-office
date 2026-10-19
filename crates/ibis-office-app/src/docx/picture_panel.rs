// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Numeric size controls for the inline picture at the caret.
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input,InputEvent,InputState};
use gpui_kit::component::{Icon, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{FlowPos,FlowRunKind,FlowCommand,EmuSize,MediaId,PictureLook,Crop,Stroke,Color,Rgb,LineDash};
use crate::shell::{chrome::SidebarPanel,editor::SlideEditor,icons::Material};

pub(crate) const PICTURE_BORDER_WIDTHS: [(&str, i64); 7] = [
    ("0.5 pt", 6_350),
    ("1 pt", 12_700),
    ("1.5 pt", 19_050),
    ("2.25 pt", 28_575),
    ("3 pt", 38_100),
    ("4.5 pt", 57_150),
    ("6 pt", 76_200),
];

pub(crate) const PICTURE_BORDER_COLORS: [(&str, u32); 6] = [
    ("_docx.black", 0x000000),
    ("_docx.gray-2", 0x757575),
    ("_docx.red", 0xD32F2F),
    ("_docx.blue", 0x1B65D4),
    ("_docx.green", 0x2E7D32),
    ("_docx.orange", 0xF57C00),
];

pub(crate) const PICTURE_BORDER_DASHES: [(&str, LineDash); 5] = [
    ("pptx.chrome.solid", LineDash::Solid),
    ("_docx.round-dot", LineDash::Dot),
    ("_docx.dash", LineDash::Dash),
    ("pptx.chrome.long-dash", LineDash::LgDash),
    ("_docx.dash-dot", LineDash::DashDot),
];

/// A recolouring of a picture (Word's 色).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PictureRecolor {
    Grayscale,
    Sepia,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ZOrderAction {
    BringToFront,
    BringForward,
    SendBackward,
    SendToBack,
}

#[derive(Clone, Debug)]
pub(crate) struct FloatingPictureInfo {
    pub(crate) at: FlowPos,
    pub(crate) size: EmuSize,
    pub(crate) anchor: ibis_doc_model::FloatingAnchor,
}

pub(crate) struct PictureEditState {
    pub(crate) name_field: Entity<InputState>,
    pub(crate) initial_name: String,
    pub(crate) fields:[Entity<InputState>;2],
    /// The cut at the top, bottom, left and right, in percent.
    pub(crate) crop:[Entity<InputState>;4],
    pub(crate) keep_ratio:bool,
    pub(crate) alt_title: Entity<InputState>,
    pub(crate) alt_desc: Entity<InputState>,
    pub(crate) initial_alt: ibis_doc_model::AltText,
    pub(crate) decorative: bool,
    pub(crate) initial_decorative: bool,
    target:(FlowPos,MediaId,EmuSize),
    initial:[String;2],
    error:Option<String>,
    _subscriptions:Vec<Subscription>,
}
impl SlideEditor {
    pub(crate) fn picture_at_caret(&self)->Option<(FlowPos,MediaId,EmuSize)> {
        let (start,_) = self.ordered_selection()?;
        let p=self.paragraph_of(start.para)?;
        let mut offset=0;
        let mut previous=None;
        for run in &p.runs {
            match &run.kind {
                FlowRunKind::Picture {media,size,..}=> {
                    let found=Some((FlowPos::new(p.id,offset),*media,*size));
                    if offset==start.offset {return found;}
                    if offset+1==start.offset {previous=found;}
                }
                FlowRunKind::Shape(shape) => {
                    if let ibis_doc_model::ShapeKind::Picture(pic) = &shape.kind {
                        let size = EmuSize::new(shape.frame.width, shape.frame.height);
                        let found = Some((FlowPos::new(p.id, offset), pic.media, size));
                        if offset == start.offset { return found; }
                        if offset + 1 == start.offset { previous = found; }
                    }
                }
                _ => {}
            }
            offset+=run.len();
        }
        previous
    }
    pub(crate) fn open_picture_edit(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        let Some(target)=self.picture_at_caret() else{return};
        let initial=[format!("{:.2}",target.2.width as f64/12700.),format!("{:.2}",target.2.height as f64/12700.)];
        let fields=initial.clone().map(|value|cx.new(|cx|{
            let mut field=InputState::new(window,cx);field.set_value(value,window,cx);field
        }));
        let mut subscriptions: Vec<Subscription> = fields.iter().map(|field|cx.subscribe_in(field,window,|this,_,event:&InputEvent,window,cx|{
            if let InputEvent::PressEnter {..}=event {this.apply_picture_edit(window,cx);}
        })).collect();
        let look_now = self.picture_look_at_caret().unwrap_or_default();
        let crop_now = look_now.crop;
        let initial_alt = look_now.alt_text.clone();
        let decorative = look_now.decorative;
        let initial_name = look_now.name.clone();
        let name_field = cx.new(|cx| {
            let mut field = InputState::new(window, cx);
            field.set_value(initial_name.clone(), window, cx);
            field
        });
        subscriptions.push(cx.subscribe_in(&name_field, window, |this, _, event: &InputEvent, _window, cx| {
            if let InputEvent::PressEnter { .. } = event { this.apply_picture_name(cx); }
        }));
        let percent=|v:i32|{let p=v as f64/1000.;if p.fract()==0.{format!("{}",p as i64)}else{format!("{p:.1}")}};
        let crop=[crop_now.top,crop_now.bottom,crop_now.left,crop_now.right].map(|v|cx.new(|cx|{
            let mut field=InputState::new(window,cx);field.set_value(percent(v),window,cx);field
        }));
        let alt_title = cx.new(|cx| {
            let mut field = InputState::new(window, cx);
            field.set_value(initial_alt.title.clone(), window, cx);
            field
        });
        let alt_desc = cx.new(|cx| {
            let mut field = InputState::new(window, cx);
            field.set_value(initial_alt.description.clone(), window, cx);
            field
        });
        subscriptions.push(cx.subscribe_in(&alt_title, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event { this.apply_picture_edit(window, cx); }
        }));
        subscriptions.push(cx.subscribe_in(&alt_desc, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event { this.apply_picture_edit(window, cx); }
        }));
        if let Some(state)=self.doc_state_mut() {
            state.picture_edit=Some(PictureEditState {name_field,initial_name,fields,crop,keep_ratio:true,alt_title,alt_desc,initial_alt,decorative,initial_decorative:decorative,target,initial,error:None,_subscriptions:subscriptions});
            state.table_size=None;state.font_edit=None; state.color_edit=None; state.paragraph_edit=None;state.style_edit=None;state.link=None;state.find=None;state.comments=None;
        }
        self.sidebar_open=true;cx.notify();
    }
    /// How the picture at the caret is turned, flipped and cut.
    pub(crate) fn picture_look_at_caret(&self)->Option<PictureLook> {
        let (at,..)=self.picture_at_caret()?;
        let p=self.paragraph_of(at.para)?;
        let mut offset=0;
        for run in &p.runs {
            match &run.kind {
                FlowRunKind::Picture {look,..}=> {
                    if offset==at.offset {return Some(look.clone());}
                }
                FlowRunKind::Shape(shape) => {
                    if offset==at.offset && let ibis_doc_model::ShapeKind::Picture(pic) = &shape.kind {
                        return Some(PictureLook {
                            rotation: shape.rotation,
                            flip_h: shape.flip_h,
                            flip_v: shape.flip_v,
                            crop: pic.crop,
                            stroke: pic.stroke.clone(),
                            luminance: pic.luminance,
                            alpha: pic.alpha,
                            duotone: pic.duotone,
                            alt_text: pic.alt_text.clone(),
                            decorative: pic.decorative,
                            name: pic.name.clone(),
                            ole: pic.ole,
                        });
                    }
                }
                _ => {}
            }
            offset+=run.len();
        }
        None
    }

    /// The text wrapping style of the picture at the caret.
    pub(crate) fn picture_wrap_at_caret(&self) -> Option<ibis_doc_model::PictureWrapType> {
        let (at, ..) = self.picture_at_caret()?;
        let p = self.paragraph_of(at.para)?;
        let mut offset = 0;
        for run in &p.runs {
            if offset == at.offset {
                return match &run.kind {
                    FlowRunKind::Picture { .. } => Some(ibis_doc_model::PictureWrapType::Inline),
                    FlowRunKind::Shape(shape) => {
                        let anchor = shape.floating.as_ref();
                        match anchor {
                            None => Some(ibis_doc_model::PictureWrapType::Inline),
                            Some(f) => Some(ibis_doc_model::PictureWrapType::from_anchor(f)),
                        }
                    }
                    _ => None,
                };
            }
            offset += run.len();
        }
        None
    }

    /// The floating anchor of the picture at the caret, if floating.
    pub(crate) fn picture_floating_at_caret(&self) -> Option<ibis_doc_model::FloatingAnchor> {
        let (at, ..) = self.picture_at_caret()?;
        let p = self.paragraph_of(at.para)?;
        let mut offset = 0;
        for run in &p.runs {
            if offset == at.offset {
                if let FlowRunKind::Shape(shape) = &run.kind {
                    return shape.floating.clone();
                }
            }
            offset += run.len();
        }
        None
    }

    /// Set the text wrapping style of the picture at the caret.
    pub(crate) fn set_picture_wrap(&mut self, wrap: ibis_doc_model::PictureWrapType, cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        self.run_flow(vec![FlowCommand::SetPictureWrap { at, wrap }], cx);
    }

    /// Set the floating distance from text of the picture at the caret.
    pub(crate) fn set_picture_floating_distance(&mut self, distance: [ibis_doc_model::Emu; 4], cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        let mut anchor = self.picture_floating_at_caret().unwrap_or_default();
        anchor.dist_l = distance[0];
        anchor.dist_r = distance[1];
        anchor.dist_t = distance[2];
        anchor.dist_b = distance[3];
        self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
    }

    /// Set the horizontal alignment of the floating picture at the caret.
    pub(crate) fn set_picture_floating_align_h(&mut self, align: Option<&str>, cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        let mut anchor = self.picture_floating_at_caret().unwrap_or_default();
        anchor.align_h = align.map(str::to_string);
        self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
    }

    /// Set the vertical alignment of the floating picture at the caret.
    pub(crate) fn set_picture_floating_align_v(&mut self, align: Option<&str>, cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        let mut anchor = self.picture_floating_at_caret().unwrap_or_default();
        anchor.align_v = align.map(str::to_string);
        self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
    }

    /// Toggle allow overlap for the floating picture at the caret.
    pub(crate) fn toggle_picture_allow_overlap(&mut self, cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        let mut anchor = self.picture_floating_at_caret().unwrap_or_default();
        anchor.allow_overlap = !anchor.allow_overlap;
        self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
    }

    /// Toggle locked anchor for the floating picture at the caret.
    pub(crate) fn toggle_picture_locked(&mut self, cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        let mut anchor = self.picture_floating_at_caret().unwrap_or_default();
        anchor.locked = !anchor.locked;
        self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
    }

    /// Toggle layout in cell for the floating picture at the caret.
    pub(crate) fn toggle_picture_layout_in_cell(&mut self, cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        let mut anchor = self.picture_floating_at_caret().unwrap_or_default();
        anchor.layout_in_cell = !anchor.layout_in_cell;
        self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
    }
    /// Set whether the floating picture moves with text (relativeFrom="paragraph") or has a fixed position on page (relativeFrom="page").
    pub(crate) fn set_picture_move_with_text(&mut self, move_with_text: bool, cx: &mut Context<Self>) {
        let rel = if move_with_text { "paragraph" } else { "page" };
        self.set_picture_floating_relative_v(rel, cx);
    }

    /// Set vertical positioning reference for the floating picture at the caret.
    pub(crate) fn set_picture_floating_relative_v(&mut self, relative_v: &str, cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        let mut anchor = self.picture_floating_at_caret().unwrap_or_default();
        anchor.relative_v = relative_v.to_string();
        self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
    }

    /// Collect all relative_height values of floating shapes in the document.
    pub(crate) fn document_floating_relative_heights(&self) -> Vec<u32> {
        let mut heights = Vec::new();
        let Some(flow) = self.flow() else { return heights };
        let mut scan_paragraph = |p: &ibis_doc_model::FlowParagraph| {
            for r in &p.runs {
                if let FlowRunKind::Shape(ref s) = r.kind {
                    if let Some(ref fl) = s.floating {
                        heights.push(fl.relative_height);
                    }
                }
            }
        };
        for b in &flow.body {
            match b {
                ibis_doc_model::Block::Paragraph(p) => scan_paragraph(p),
                ibis_doc_model::Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            for cb in &cell.blocks {
                                if let ibis_doc_model::Block::Paragraph(p) = cb {
                                    scan_paragraph(p);
                                }
                            }
                        }
                    }
                }
            }
        }
        heights
    }

    /// Reorder the floating picture at the caret (front, forward, backward, back).
    pub(crate) fn reorder_picture_zorder(&mut self, action: ZOrderAction, cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        let mut anchor = self.picture_floating_at_caret().unwrap_or_default();
        let cur = anchor.relative_height;
        let heights = self.document_floating_relative_heights();
        let new_height = match action {
            ZOrderAction::BringToFront => {
                let max = heights.iter().copied().max().unwrap_or(cur);
                max.saturating_add(1).max(cur + 1)
            }
            ZOrderAction::BringForward => {
                let next = heights.iter().copied().filter(|&h| h > cur).min();
                next.map_or(cur.saturating_add(1), |h| h.saturating_add(1))
            }
            ZOrderAction::SendBackward => {
                let prev = heights.iter().copied().filter(|&h| h < cur).max();
                prev.map_or(cur.saturating_sub(1), |h| h.saturating_sub(1))
            }
            ZOrderAction::SendToBack => {
                let min = heights.iter().copied().min().unwrap_or(cur);
                min.saturating_sub(1)
            }
        };
        anchor.relative_height = new_height;
        self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
    }

    /// Collect all floating pictures in the document with their position, size and anchor.
    pub(crate) fn document_floating_pictures(&self) -> Vec<FloatingPictureInfo> {
        let mut pictures = Vec::new();
        let Some(flow) = self.flow() else { return pictures };
        let mut scan_paragraph = |p: &ibis_doc_model::FlowParagraph| {
            let mut offset = 0;
            for r in &p.runs {
                if let FlowRunKind::Shape(ref s) = r.kind {
                    if let ibis_doc_model::ShapeKind::Picture(_) = &s.kind {
                        if let Some(ref fl) = s.floating {
                            let size = EmuSize::new(s.frame.width, s.frame.height);
                            pictures.push(FloatingPictureInfo {
                                at: FlowPos::new(p.id, offset),
                                size,
                                anchor: fl.clone(),
                            });
                        }
                    }
                }
                offset += r.len();
            }
        };
        for b in &flow.body {
            match b {
                ibis_doc_model::Block::Paragraph(p) => scan_paragraph(p),
                ibis_doc_model::Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            for cb in &cell.blocks {
                                if let ibis_doc_model::Block::Paragraph(p) = cb {
                                    scan_paragraph(p);
                                }
                            }
                        }
                    }
                }
            }
        }
        pictures
    }

    /// Distribute floating pictures horizontally.
    /// If 3 or more pictures exist, places intermediate pictures at equal intervals between
    /// the leftmost and rightmost pictures. If 1 or 2 pictures exist, centres or balances them.
    pub(crate) fn distribute_pictures_horizontally(&mut self, cx: &mut Context<Self>) {
        let mut pics = self.document_floating_pictures();
        if pics.is_empty() { return; }
        if pics.len() == 1 {
            let mut anchor = pics[0].anchor.clone();
            anchor.align_h = Some("center".to_string());
            self.run_flow(vec![FlowCommand::SetPictureFloating { at: pics[0].at, anchor: Some(anchor) }], cx);
            return;
        }
        pics.sort_by_key(|p| p.anchor.offset_x);
        let n = pics.len();
        let min_x = pics[0].anchor.offset_x;
        let max_x = pics[n - 1].anchor.offset_x;
        let total_intermediate_w: i64 = pics[1..n - 1].iter().map(|p| p.size.width).sum();
        let available_gap = (max_x - min_x - pics[0].size.width - total_intermediate_w).max(0);
        let gap = if n > 1 { available_gap / ((n - 1) as i64) } else { 0 };
        let mut commands = Vec::new();
        if available_gap > 0 {
            let mut cur_x = min_x;
            for p in &mut pics {
                p.anchor.offset_x = cur_x;
                p.anchor.align_h = None;
                cur_x += p.size.width + gap;
                commands.push(FlowCommand::SetPictureFloating { at: p.at, anchor: Some(p.anchor.clone()) });
            }
        } else {
            let step = if max_x > min_x { (max_x - min_x) / ((n - 1) as i64) } else { 100_000 };
            for (i, p) in pics.iter_mut().enumerate() {
                let new_x = min_x + step * (i as i64);
                p.anchor.offset_x = new_x;
                p.anchor.align_h = None;
                commands.push(FlowCommand::SetPictureFloating { at: p.at, anchor: Some(p.anchor.clone()) });
            }
        }
        self.run_flow(commands, cx);
    }

    /// Distribute floating pictures vertically.
    /// If 3 or more pictures exist, places intermediate pictures at equal intervals between
    /// the topmost and bottommost pictures. If 1 or 2 pictures exist, centres or balances them.
    pub(crate) fn distribute_pictures_vertically(&mut self, cx: &mut Context<Self>) {
        let mut pics = self.document_floating_pictures();
        if pics.is_empty() { return; }
        if pics.len() == 1 {
            let mut anchor = pics[0].anchor.clone();
            anchor.align_v = Some("center".to_string());
            self.run_flow(vec![FlowCommand::SetPictureFloating { at: pics[0].at, anchor: Some(anchor) }], cx);
            return;
        }
        pics.sort_by_key(|p| p.anchor.offset_y);
        let n = pics.len();
        let min_y = pics[0].anchor.offset_y;
        let max_y = pics[n - 1].anchor.offset_y;
        let total_intermediate_h: i64 = pics[1..n - 1].iter().map(|p| p.size.height).sum();
        let available_gap = (max_y - min_y - pics[0].size.height - total_intermediate_h).max(0);
        let gap = if n > 1 { available_gap / ((n - 1) as i64) } else { 0 };
        let mut commands = Vec::new();
        if available_gap > 0 {
            let mut cur_y = min_y;
            for p in &mut pics {
                p.anchor.offset_y = cur_y;
                p.anchor.align_v = None;
                cur_y += p.size.height + gap;
                commands.push(FlowCommand::SetPictureFloating { at: p.at, anchor: Some(p.anchor.clone()) });
            }
        } else {
            let step = if max_y > min_y { (max_y - min_y) / ((n - 1) as i64) } else { 100_000 };
            for (i, p) in pics.iter_mut().enumerate() {
                let new_y = min_y + step * (i as i64);
                p.anchor.offset_y = new_y;
                p.anchor.align_v = None;
                commands.push(FlowCommand::SetPictureFloating { at: p.at, anchor: Some(p.anchor.clone()) });
            }
        }
        self.run_flow(commands, cx);
    }

    /// Set alignment reference ("margin" or "page") for floating picture at caret.
    pub(crate) fn set_picture_align_relative(&mut self, relative: &str, cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        let mut anchor = self.picture_floating_at_caret().unwrap_or_default();
        anchor.relative_h = relative.to_string();
        anchor.relative_v = relative.to_string();
        self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
    }

    /// Change the look of the picture at the caret by `f` (one step to undo).
    pub(crate) fn change_picture_look(&mut self,f:impl FnOnce(&mut PictureLook),cx:&mut Context<Self>) {
        let (Some((at,..)),Some(mut look))=(self.picture_at_caret(),self.picture_look_at_caret()) else{return};
        f(&mut look);
        self.run_flow(vec![FlowCommand::SetPictureLook {at,look}],cx);
        cx.notify();
    }
    /// Outline (border) of the picture at the caret, if any.
    pub(crate) fn picture_stroke_at_caret(&self)->Option<Stroke> {
        self.picture_look_at_caret().and_then(|l| l.stroke)
    }
    /// 明るさ・コントラスト: the picture's brightness (`bright`) or contrast changed by `delta` percent (±100).
    pub(crate) fn adjust_picture_luminance(&mut self, bright: bool, delta: i32, cx: &mut Context<Self>) {
        self.change_picture_look(|l| {
            let (b, c) = l.luminance.unwrap_or((0, 0));
            let step = delta * 1000;
            let (b, c) = if bright { ((b + step).clamp(-100_000, 100_000), c) } else { (b, (c + step).clamp(-100_000, 100_000)) };
            l.luminance = (b != 0 || c != 0).then_some((b, c));
        }, cx);
    }
    /// 透明度: the picture's whole transparency, in percent (0 for opaque).
    pub(crate) fn set_picture_transparency(&mut self, percent: u32, cx: &mut Context<Self>) {
        self.change_picture_look(|l| l.alpha = (percent > 0).then(|| 100_000 - percent.min(100) * 1000), cx);
    }
    /// 色: the picture recoloured (grayscale, sepia) or as it was (`None`).
    pub(crate) fn recolor_picture(&mut self, kind: Option<PictureRecolor>, cx: &mut Context<Self>) {
        self.change_picture_look(|l| {
            l.duotone = kind.map(|k| match k {
                PictureRecolor::Grayscale => (Rgb(0x000000), Rgb(0xFFFFFF)),
                PictureRecolor::Sepia => (Rgb(0x3F2300), Rgb(0xFFF3D6)),
            });
        }, cx);
    }
    /// Set or change the border width (EMU) of the picture at the caret.
    pub(crate) fn set_picture_border_width(&mut self, width: i64, cx: &mut Context<Self>) {
        self.change_picture_look(|l| {
            if let Some(s) = l.stroke.as_mut() {
                s.width = width;
            } else {
                l.stroke = Some(Stroke::solid(Color::BLACK, width));
            }
        }, cx);
    }
    /// Set or change the border color of the picture at the caret.
    pub(crate) fn set_picture_border_color(&mut self, color: Color, cx: &mut Context<Self>) {
        self.change_picture_look(|l| {
            if let Some(s) = l.stroke.as_mut() {
                s.color = color;
            } else {
                l.stroke = Some(Stroke::solid(color, Stroke::DEFAULT_WIDTH));
            }
        }, cx);
    }
    /// Set or change the border dash style of the picture at the caret.
    pub(crate) fn set_picture_border_dash(&mut self, dash: LineDash, cx: &mut Context<Self>) {
        self.change_picture_look(|l| {
            if let Some(s) = l.stroke.as_mut() {
                s.dash = dash;
                s.dashed = dash != LineDash::Solid;
            } else {
                l.stroke = Some(Stroke::solid(Color::BLACK, Stroke::DEFAULT_WIDTH).with_dash(dash));
            }
        }, cx);
    }
    /// Remove the border around the picture at the caret.
    pub(crate) fn clear_picture_border(&mut self, cx: &mut Context<Self>) {
        self.change_picture_look(|l| l.stroke = None, cx);
    }
    /// Turn the picture a quarter (`turns` 1 clockwise, -1 anticlockwise).
    pub(crate) fn turn_picture(&mut self,turns:i32,cx:&mut Context<Self>) {
        self.change_picture_look(|l|l.rotation=(l.rotation+turns*5_400_000).rem_euclid(21_600_000),cx);
    }
    pub(crate) fn flip_picture(&mut self,horizontal:bool,cx:&mut Context<Self>) {
        self.change_picture_look(|l|if horizontal {l.flip_h=!l.flip_h} else {l.flip_v=!l.flip_v},cx);
    }
    /// Cut the picture as the four fields say (percent of its size; each 0 to 99, together under 100 a side).
    pub(crate) fn apply_picture_crop(&mut self,cx:&mut Context<Self>) {
        let Some(fields)=self.doc_state().and_then(|s|s.picture_edit.as_ref()).map(|e|e.crop.clone()) else{return};
        let values=fields.map(|f|f.read(cx).value().trim().parse::<f64>().ok().filter(|v|v.is_finite()&&*v>=0.&&*v<100.).map(|v|(v*1000.).round() as i32));
        let [Some(top),Some(bottom),Some(left),Some(right)]=values else {
            if let Some(edit)=self.doc_state_mut().and_then(|s|s.picture_edit.as_mut()) {edit.error=Some(crate::shell::i18n::text("fix.picture-panel.enter-crop-number-0-up").into());}
            cx.notify();return;
        };
        if top+bottom>=100_000||left+right>=100_000 {
            if let Some(edit)=self.doc_state_mut().and_then(|s|s.picture_edit.as_mut()) {edit.error=Some(crate::shell::i18n::text("fix.picture-panel.top-plus-bottom-left-plus").into());}
            cx.notify();return;
        }
        if let Some(edit)=self.doc_state_mut().and_then(|s|s.picture_edit.as_mut()) {edit.error=None;}
        self.change_picture_look(|l|l.crop=Crop {left,top,right,bottom},cx);
    }
    pub(crate) fn toggle_picture_ratio(&mut self,cx:&mut Context<Self>) {
        if let Some(edit)=self.doc_state_mut().and_then(|s|s.picture_edit.as_mut()) {edit.keep_ratio=!edit.keep_ratio;}
        cx.notify();
    }
    pub(crate) fn apply_picture_edit(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        let Some(edit)=self.doc_state().and_then(|s|s.picture_edit.as_ref()) else{return};
        let values=edit.fields.clone().map(|f|f.read(cx).value().trim().to_string());
        let target=edit.target;
        let changed=[values[0]!=edit.initial[0],values[1]!=edit.initial[1]];
        let parsed=values.clone().map(|v|v.parse::<f64>().ok().filter(|v|v.is_finite() && *v>=0.01 && *v<=1584.));
        let mut size=match parsed { [Some(w),Some(h)]=>Some(EmuSize::new((w*12700.).round() as i64,(h*12700.).round() as i64)),_=>None };
        if edit.keep_ratio && changed.iter().any(|v|*v) {
            size=size.and_then(|mut size|{
                if target.2.width<=0 || target.2.height<=0 {return None;}
                match changed {
                    [true,false]=>size.height=(size.width as f64*target.2.height as f64/target.2.width as f64).round() as i64,
                    [false,true]=>size.width=(size.height as f64*target.2.width as f64/target.2.height as f64).round() as i64,
                    [true,true]=>if (size.width as f64/size.height as f64-target.2.width as f64/target.2.height as f64).abs()>0.0001 {return None;},
                    _=>{}
                }
                Some(size)
            });
        }
        let size=size.filter(|s|s.width>=127 && s.height>=127 && s.width<=20116800 && s.height<=20116800);
        if self.picture_at_caret()!=Some(target) || size.is_none() {
            if let Some(edit)=self.doc_state_mut().and_then(|s|s.picture_edit.as_mut()) {edit.error=Some(crate::shell::i18n::text("docx.picture-panel.check-picture-selection-value").into());}
            cx.notify();return;
        }
        let size=size.unwrap();
        if !changed.iter().any(|v|*v) {
            self.apply_picture_alt_text(cx);
            self.focus_handle.focus(window,cx);
            return;
        }
        if self.run_flow(vec![FlowCommand::SetPictureSize {at:target.0,size}],cx) {
            let values=[format!("{:.2}",size.width as f64/12700.),format!("{:.2}",size.height as f64/12700.)];
            if let Some(edit)=self.doc_state_mut().and_then(|s|s.picture_edit.as_mut()) {
                edit.target=(target.0,target.1,size);edit.initial=values.clone();edit.error=None;
                let fields=edit.fields.clone();
                for (field,value) in fields.into_iter().zip(values) {field.update(cx,|i,cx|i.set_value(value,window,cx));}
            }
            self.apply_picture_alt_text(cx);
            self.apply_picture_name(cx);
            self.focus_handle.focus(window,cx);
        }
        cx.notify();
    }
    pub(crate) fn picture_name_at_caret(&self) -> Option<String> {
        self.picture_look_at_caret().map(|l| l.name)
    }
    pub(crate) fn set_picture_name(&mut self, name: String, cx: &mut Context<Self>) {
        self.change_picture_look(|l| l.name = name, cx);
    }
    pub(crate) fn apply_picture_name(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state().and_then(|s| s.picture_edit.as_ref()) else { return };
        let new_name = edit.name_field.read(cx).value().trim().to_string();
        if new_name != edit.initial_name {
            self.set_picture_name(new_name.clone(), cx);
            if let Some(edit) = self.doc_state_mut().and_then(|s| s.picture_edit.as_mut()) {
                edit.initial_name = new_name;
            }
        }
    }
    pub(crate) fn open_picture_alt_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_picture_edit(window, cx);
        if let Some(state) = self.doc_state().and_then(|s| s.picture_edit.as_ref()) {
            state.alt_desc.update(cx, |f, cx| f.focus(window, cx));
        }
    }
    pub(crate) fn picture_alt_text_at_caret(&self) -> Option<ibis_doc_model::AltText> {
        self.picture_look_at_caret().map(|l| l.alt_text)
    }
    pub(crate) fn set_picture_alt_text(&mut self, alt_text: ibis_doc_model::AltText, cx: &mut Context<Self>) {
        self.change_picture_look(|l| l.alt_text = alt_text, cx);
    }
    pub(crate) fn picture_decorative_at_caret(&self) -> Option<bool> {
        self.picture_look_at_caret().map(|l| l.decorative)
    }
    pub(crate) fn set_picture_decorative(&mut self, decorative: bool, cx: &mut Context<Self>) {
        self.change_picture_look(|l| l.decorative = decorative, cx);
    }
    pub(crate) fn toggle_picture_decorative(&mut self, cx: &mut Context<Self>) {
        let (current_dec, has_panel) = if let Some(edit) = self.doc_state().and_then(|s| s.picture_edit.as_ref()) {
            (edit.decorative, true)
        } else {
            (self.picture_decorative_at_caret().unwrap_or(false), false)
        };
        let new_dec = !current_dec;
        self.set_picture_decorative(new_dec, cx);
        if has_panel {
            if let Some(edit) = self.doc_state_mut().and_then(|s| s.picture_edit.as_mut()) {
                edit.decorative = new_dec;
                edit.initial_decorative = new_dec;
            }
        }
        cx.notify();
    }
    pub(crate) fn apply_picture_alt_text(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state().and_then(|s| s.picture_edit.as_ref()) else { return };
        if edit.decorative {
            return;
        }
        let title = edit.alt_title.read(cx).value().trim().to_string();
        let description = edit.alt_desc.read(cx).value().trim().to_string();
        let new_alt = ibis_doc_model::AltText { title, description };
        if new_alt != edit.initial_alt {
            self.set_picture_alt_text(new_alt.clone(), cx);
            if let Some(edit) = self.doc_state_mut().and_then(|s| s.picture_edit.as_mut()) {
                edit.initial_alt = new_alt;
            }
        }
    }
    pub(crate) fn picture_panel(&mut self,cx:&mut Context<Self>)->Option<SidebarPanel> {
        let edit=self.doc_state()?.picture_edit.as_ref()?;
        let name_field = edit.name_field.clone();
        let fields=edit.fields.clone();let crop=edit.crop.clone();let ratio=edit.keep_ratio;let error=edit.error.clone();
        let alt_title = edit.alt_title.clone();
        let alt_desc = edit.alt_desc.clone();
        let decorative = edit.decorative;
        let stroke_now = self.picture_stroke_at_caret();
        let duotone_now = self.picture_look_at_caret().and_then(|l| l.duotone);
        let alpha_now = self.picture_look_at_caret().and_then(|l| l.alpha);
        let row=|label:&'static str,field:&Entity<InputState>|h_flex().gap_2().child(div().w(px(80.)).text_sm().child(crate::shell::i18n::text(label))).child(Input::new(field).small());
        let mut body=v_flex().gap_2().px(px(18.)).py(px(8.))
            .child(h_flex().gap_2()
                .child(row("docx.picture.name", &name_field))
                .child(Button::new("picture-name-apply").primary().small().label(crate::shell::i18n::text("_shell.apply")).on_click(cx.listener(|this, _, _, cx| this.apply_picture_name(cx)))))
            .child(row("docx.picture-panel.width-pt",&fields[0])).child(row("docx.picture-panel.height-pt",&fields[1]))
            .child(Button::new("picture-ratio").small().label(crate::shell::i18n::text("docx.picture-panel.lock-aspect-ratio")).selected(ratio).on_click(cx.listener(|this,_,_,cx|this.toggle_picture_ratio(cx))))
            .children(error.map(|e|div().text_sm().child(e)))
            .child(h_flex().gap_2()
                .child(Button::new("picture-size-apply").primary().small().label(crate::shell::i18n::text("_shell.apply")).on_click(cx.listener(|this,_,window,cx|this.apply_picture_edit(window,cx))))
                .child(Button::new("picture-replace").small().label(crate::shell::i18n::text("_docx.change-picture")).on_click(cx.listener(|this,_,window,cx|this.doc_pick_replacement_picture(window,cx)))))
            .child(div().pt_2().text_sm().child(crate::shell::i18n::text("fix.picture-panel.rotate-flip")))
            .child(h_flex().gap_1().flex_wrap()
                .child(Button::new("picture-turn-left").small().label(crate::shell::i18n::text("fix.picture-panel.rotate-left-90")).on_click(cx.listener(|this,_,_,cx|this.turn_picture(-1,cx))))
                .child(Button::new("picture-turn-right").small().label(crate::shell::i18n::text("fix.picture-panel.rotate-right-90")).on_click(cx.listener(|this,_,_,cx|this.turn_picture(1,cx))))
                .child(Button::new("picture-flip-h").small().label(crate::shell::i18n::text("fix.picture-panel.flip-horizontal")).on_click(cx.listener(|this,_,_,cx|this.flip_picture(true,cx))))
                .child(Button::new("picture-flip-v").small().label(crate::shell::i18n::text("fix.picture-panel.flip-vertical")).on_click(cx.listener(|this,_,_,cx|this.flip_picture(false,cx)))))
            .child(div().pt_2().text_sm().child(crate::shell::i18n::text("fix.picture-panel.crop")))
            .child(row("_shell.top",&crop[0])).child(row("_shell.bottom",&crop[1])).child(row("_shell.left",&crop[2])).child(row("_shell.right",&crop[3]))
            .child(Button::new("picture-crop-apply").primary().small().label(crate::shell::i18n::text("fix.picture-panel.apply-crop")).on_click(cx.listener(|this,_,_,cx|this.apply_picture_crop(cx))))
            .child(div().pt_2().text_sm().child(crate::shell::i18n::text("docx.picture-panel.picture-corrections")))
            .child(h_flex().gap_1().flex_wrap()
                .child(Button::new("pic-bright-down").small().label(crate::shell::i18n::text("docx.picture-panel.brightness-10-percent")).on_click(cx.listener(|this,_,_,cx|this.adjust_picture_luminance(true,-10,cx))))
                .child(Button::new("pic-bright-up").small().label(crate::shell::i18n::text("docx.picture-panel.brightness-plus-10-percent")).on_click(cx.listener(|this,_,_,cx|this.adjust_picture_luminance(true,10,cx))))
                .child(Button::new("pic-contrast-down").small().label(crate::shell::i18n::text("docx.picture-panel.contrast-10-percent")).on_click(cx.listener(|this,_,_,cx|this.adjust_picture_luminance(false,-10,cx))))
                .child(Button::new("pic-contrast-up").small().label(crate::shell::i18n::text("docx.picture-panel.contrast-plus-10-percent")).on_click(cx.listener(|this,_,_,cx|this.adjust_picture_luminance(false,10,cx)))))
            .child(h_flex().gap_1().flex_wrap()
                .child(Button::new("pic-recolor-none").small().label(crate::shell::i18n::text("docx.picture-panel.no-recolor")).selected(duotone_now.is_none()).on_click(cx.listener(|this,_,_,cx|this.recolor_picture(None,cx))))
                .child(Button::new("pic-recolor-gray").small().label(crate::shell::i18n::text("pptx.print-panel.grayscale")).selected(duotone_now==Some((Rgb(0x000000),Rgb(0xFFFFFF)))).on_click(cx.listener(|this,_,_,cx|this.recolor_picture(Some(PictureRecolor::Grayscale),cx))))
                .child(Button::new("pic-recolor-sepia").small().label(crate::shell::i18n::text("docx.picture-panel.sepia")).selected(duotone_now==Some((Rgb(0x3F2300),Rgb(0xFFF3D6)))).on_click(cx.listener(|this,_,_,cx|this.recolor_picture(Some(PictureRecolor::Sepia),cx)))))
            .child(div().pt_2().text_sm().child(crate::shell::i18n::text("docx.picture-panel.transparency")))
            .child(h_flex().gap_1().flex_wrap().children([0u32, 15, 30, 50, 80].map(|percent| {
                let opacity = (percent > 0).then(|| 100_000 - percent * 1000);
                Button::new(("pic-transparency", percent as usize)).small().label(format!("{percent}%")).selected(alpha_now == opacity).on_click(cx.listener(move |this,_,_,cx|this.set_picture_transparency(percent,cx)))
            })))
            .child(div().pt_2().text_sm().child(crate::shell::i18n::text("docx.picture.border.label")))
            .child(h_flex().gap_1().flex_wrap()
                .child(Button::new("pic-border-none").small().label(crate::shell::i18n::text("pptx.chrome.no-border")).selected(stroke_now.is_none()).on_click(cx.listener(|this,_,_,cx|this.clear_picture_border(cx))))
                .child(Button::new("pic-border-w1").small().label("1 pt").selected(stroke_now.as_ref().is_some_and(|s| s.width == 12_700)).on_click(cx.listener(|this,_,_,cx|this.set_picture_border_width(12_700, cx))))
                .child(Button::new("pic-border-w2").small().label("1.5 pt").selected(stroke_now.as_ref().is_some_and(|s| s.width == 19_050)).on_click(cx.listener(|this,_,_,cx|this.set_picture_border_width(19_050, cx))))
                .child(Button::new("pic-border-w3").small().label("3 pt").selected(stroke_now.as_ref().is_some_and(|s| s.width == 38_100)).on_click(cx.listener(|this,_,_,cx|this.set_picture_border_width(38_100, cx))))
                .child(Button::new("pic-border-w6").small().label("6 pt").selected(stroke_now.as_ref().is_some_and(|s| s.width == 76_200)).on_click(cx.listener(|this,_,_,cx|this.set_picture_border_width(76_200, cx)))))
            .child(div().pt_2().text_sm().child(crate::shell::i18n::text("docx.picture.alt-text")))
            .child(Button::new("picture-decorative").small().label(crate::shell::i18n::text("docx.picture.decorative")).selected(decorative).on_click(cx.listener(|this,_,_,cx|this.toggle_picture_decorative(cx))));
        if decorative {
            body = body.child(div().text_xs().text_color(rgb(0x5f6368)).child(crate::shell::i18n::text("docx.picture.decorative.hint")));
        } else {
            body = body
                .child(div().text_xs().text_color(rgb(0x5f6368)).child(crate::shell::i18n::text("docx.picture.alt-text.hint")))
                .child(row("docx.picture.alt-text.title",&alt_title))
                .child(row("docx.picture.alt-text.desc",&alt_desc))
                .child(Button::new("picture-alt-text-apply").primary().small().label(crate::shell::i18n::text("docx.picture.alt-text.apply")).on_click(cx.listener(|this,_,_,cx|this.apply_picture_alt_text(cx))));
        }
        Some(SidebarPanel {title:crate::shell::i18n::text("docx.picture-panel.picture-size"),body:body.into_any_element()})
    }

    /// Collect all pictures/shapes in the document for the Selection Pane.
    pub(crate) fn document_picture_objects(&self) -> Vec<PictureObjectInfo> {
        let mut list = Vec::new();
        let Some(flow) = self.flow() else { return list };
        for b in &flow.body {
            if let ibis_doc_model::Block::Paragraph(p) = b {
                let mut offset = 0;
                for r in &p.runs {
                    match &r.kind {
                        FlowRunKind::Picture { size, look, .. } => {
                            let name = if look.name.is_empty() {
                                crate::shell::i18n::format("_docx.picture-n", &[("n", &(list.len() + 1))])
                            } else {
                                look.name.clone()
                            };
                            list.push(PictureObjectInfo {
                                at: FlowPos::new(p.id, offset),
                                name,
                                is_floating: false,
                                size: *size,
                            });
                        }
                        FlowRunKind::Shape(s) => {
                            if let ibis_doc_model::ShapeKind::Picture(pic) = &s.kind {
                                let name = if pic.name.is_empty() {
                                    crate::shell::i18n::format("_docx.picture-n", &[("n", &(list.len() + 1))])
                                } else {
                                    pic.name.clone()
                                };
                                let size = EmuSize::new(s.frame.width, s.frame.height);
                                list.push(PictureObjectInfo {
                                    at: FlowPos::new(p.id, offset),
                                    name,
                                    is_floating: s.floating.is_some(),
                                    size,
                                });
                            }
                        }
                        _ => {}
                    }
                    offset += r.len();
                }
            }
        }
        list
    }

    /// Select and jump caret to picture at `at`.
    pub(crate) fn select_picture_at(&mut self, at: FlowPos, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.anchor = Some(at);
            state.caret = Some(FlowPos::new(at.para, at.offset + 1));
        }
        cx.notify();
    }

    /// Open the Selection Pane (オブジェクトの選択と表示).
    pub(crate) fn open_selection_pane(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.selection_pane = true;
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

    /// Selection Pane panel (オブジェクトの選択と表示).
    pub(crate) fn selection_pane_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let is_open = self.doc_state().is_some_and(|s| s.selection_pane);
        if !is_open { return None; }
        let objects = self.document_picture_objects();
        let current_at = self.picture_at_caret().map(|(at, ..)| at);
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.));
        if objects.is_empty() {
            body = body.child(div().text_sm().text_color(rgb(0x5f6368)).child(crate::shell::i18n::text("docx.selection-pane.empty")));
        } else {
            body = body.child(div().text_xs().text_color(rgb(0x5f6368)).child(crate::shell::i18n::format("_docx.shapes-and-pictures-n", &[("n", &objects.len().to_string())])));
            for obj in objects {
                let at = obj.at;
                let is_selected = current_at == Some(at);
                let kind_label = if obj.is_floating { crate::shell::i18n::text("_docx.floating") } else { crate::shell::i18n::text("_docx.in-line-with-text") };
                let bg_color = if is_selected { rgb(0xe8f0fe) } else { rgb(0xffffff) };
                let id_key = (at.para as usize) << 16 | at.offset;
                let item = h_flex()
                    .id(("selection-item", id_key))
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(bg_color)
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(0xf1f3f4)))
                    .child(Icon::new(Material(if obj.is_floating { "layers" } else { "image" })).size(px(16.)))
                    .child(div().flex_1().text_sm().child(obj.name))
                    .child(div().text_xs().text_color(rgb(0x80868b)).child(kind_label))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_picture_at(at, cx);
                    }));
                body = body.child(item);
            }
        }
        Some(SidebarPanel { title: crate::shell::i18n::text("docx.selection-pane.title"), body: body.into_any_element() })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PictureObjectInfo {
    pub(crate) at: FlowPos,
    pub(crate) name: String,
    pub(crate) is_floating: bool,
    #[allow(dead_code)]
    pub(crate) size: EmuSize,
}
