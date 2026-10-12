// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! SmartArt を図形に変換 (SmartArt のデザイン > 変換 > 図形に変換): the shapes
//! drawn for a SmartArt graphic (read from PowerPoint's `dsp:drawing`) become
//! the slide's own shapes, grouped, and the graphic is gone: they are then
//! edited as any shapes. The graphic's parts leave the file when it is saved.
use gpui_kit::*;
use ibis_doc_model::{Command, Placeholder, Shape, ShapeId, ShapeKind};

use crate::shell::editor::SlideEditor;

impl SlideEditor {
    /// The selected SmartArt graphic's frame and the shapes drawn for it.
    fn selected_diagram(&self) -> Option<(ShapeId, Vec<Shape>)> {
        let selected = self.selected_shapes();
        let [shape] = selected.as_slice() else { return None };
        if !matches!(shape.kind, ShapeKind::Placeholder(Placeholder::Diagram)) {
            return None;
        }
        let slide = self.slide();
        let ids = slide.attached.get(&shape.id)?;
        let parts: Vec<Shape> = slide.shapes.iter().filter(|s| ids.contains(&s.id)).cloned().collect();
        (!parts.is_empty()).then_some((shape.id, parts))
    }

    /// Whether the selection is a SmartArt graphic that has its drawing.
    pub(crate) fn can_convert_diagram(&self) -> bool {
        self.selected_diagram().is_some()
    }

    /// 図形に変換: the graphic's shapes, as copies of their own (grouped), in the graphic's place.
    pub(crate) fn convert_diagram_to_shapes(&mut self, cx: &mut Context<Self>) {
        let Some((owner, parts)) = self.selected_diagram() else { return };
        if !parts.iter().all(|s| ibis_pptx::save::check_shape_insertion(s).is_ok()) {
            return self.set_status(crate::shell::i18n::text("pptx.smartart.smartart-has-parts-cannot").to_string(), cx);
        }
        let slide = self.current_slide;
        let Some(at) = self.slide().shape_index(owner) else { return };
        // Shapes before the graphic stay before it: the others' places are those it and its shapes had.
        let before = self.slide().shapes[..at].iter().filter(|s| !parts.iter().any(|p| p.id == s.id)).count();
        let copies: Vec<Shape> = parts
            .into_iter()
            .map(|mut s| {
                s.id = self.document.ids().new_shape_id();
                s
            })
            .collect();
        let ids: Vec<ShapeId> = copies.iter().map(|s| s.id).collect();
        let mut commands = vec![Command::RemoveShape { slide, shape: owner }];
        commands.extend(copies.into_iter().enumerate().map(|(i, shape)| Command::InsertShape { slide, index: before + i, shape }));
        if self.apply_pptx_commands_checked(vec![Command::Group(commands)], cx) {
            self.set_selection(ids);
            self.group_selected(cx);
        }
    }
}
