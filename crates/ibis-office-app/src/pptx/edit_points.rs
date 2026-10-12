// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 頂点の編集 (配置 > 頂点の編集): the vertices of a shape's outline shown as
//! handles to drag; a press on an edge adds a vertex, Delete takes the
//! selected one away, Esc ends. The shape becomes a custom (freeform) shape
//! of straight lines, unturned: curves are drawn as short lines.
use gpui_kit::*;
use ibis_doc_model::shape_merge::{reshaped, shape_polygons, Polygons};
use ibis_doc_model::{Command, ShapeId};

use crate::pptx::interaction::HANDLE_HIT_PX;
use crate::shell::editor::SlideEditor;

/// The shape whose vertices are edited, and the vertex last pressed.
#[derive(Clone, Debug)]
pub(crate) struct EditPoints {
    pub(crate) shape: ShapeId,
    /// (loop, index) of the selected vertex.
    pub(crate) selected: Option<(usize, usize)>,
}

/// The distance from `p` to segment `a`-`b`, and the nearest point of it.
fn nearest_on_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> (f64, (f64, f64)) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) };
    let q = (a.0 + t * dx, a.1 + t * dy);
    ((p.0 - q.0).hypot(p.1 - q.1), q)
}

impl SlideEditor {
    /// The shape whose vertices are edited, if any.
    fn edit_points_shape(&self) -> Option<&ibis_doc_model::Shape> {
        let id = self.pptx()?.edit_points.as_ref()?.shape;
        self.slide().shape(id)
    }

    /// Whether the selection is one closed figure, which 頂点の編集 takes.
    pub(crate) fn can_edit_points(&self) -> bool {
        match self.selected_shapes().as_slice() {
            [shape] => !self.slide().placeholders.contains_key(&shape.id) && !self.slide().is_attached(shape.id) && shape_polygons(shape).is_some(),
            _ => false,
        }
    }

    pub(crate) fn is_editing_points(&self) -> bool {
        self.edit_points_shape().is_some()
    }

    /// 頂点の編集: the selected shape made a freeform of its outline, its vertices shown.
    pub(crate) fn begin_edit_points(&mut self, cx: &mut Context<Self>) {
        if !self.can_edit_points() {
            return;
        }
        let Some(shape) = self.selected_shapes().first().map(|s| (*s).clone()) else { return };
        let Some(polygons) = shape_polygons(&shape) else { return };
        let id = shape.id;
        // A freeform of straight lines, unturned (a turned shape's outline is as drawn).
        let unchanged = matches!(&shape.kind, ibis_doc_model::ShapeKind::Auto(a) if matches!(&a.geometry, ibis_doc_model::Geometry::Formula(f) if f.preset.is_none())) && shape.rotation == 0 && !shape.flip_h && !shape.flip_v;
        if !unchanged && !self.replace_outline(&shape, &polygons, true, cx) {
            return;
        }
        if let Some(s) = self.pptx_mut() {
            s.edit_points = Some(EditPoints { shape: id, selected: None });
        }
        cx.notify();
    }

    pub(crate) fn end_edit_points(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.edit_points = None;
        }
        cx.notify();
    }

    /// `shape` with `polygons` as its outline (`unturn`: its rotation and flips taken off).
    fn replace_outline(&mut self, shape: &ibis_doc_model::Shape, polygons: &Polygons, unturn: bool, cx: &mut Context<Self>) -> bool {
        let Some(new) = reshaped(shape, polygons) else { return false };
        let slide = self.current_slide;
        let mut commands = vec![Command::SetKind { slide, shape: shape.id, kind: Box::new(new.kind.clone()) }, Command::SetFrame { slide, shape: shape.id, frame: new.frame }];
        if unturn && (shape.rotation != 0 || shape.flip_h || shape.flip_v) {
            commands.push(Command::SetTransform { slide, shape: shape.id, rotation: 0, flip_h: false, flip_v: false });
        }
        self.apply_pptx_commands_checked(vec![Command::Group(commands)], cx)
    }

    /// The vertices as (loop, index, point in slide EMU).
    pub(crate) fn edit_point_vertices(&self) -> Vec<(usize, usize, (f64, f64))> {
        let Some(polygons) = self.edit_points_shape().and_then(shape_polygons) else { return Vec::new() };
        polygons.iter().enumerate().flat_map(|(l, polygon)| polygon.iter().enumerate().map(move |(i, p)| (l, i, *p))).collect()
    }

    /// The outline as polygons in slide EMU.
    pub(crate) fn edit_point_polygons(&self) -> Polygons {
        self.edit_points_shape().and_then(shape_polygons).unwrap_or_default()
    }

    /// The vertex selected.
    pub(crate) fn edit_point_selected(&self) -> Option<(usize, usize)> {
        self.pptx()?.edit_points.as_ref()?.selected
    }

    /// A press while vertices are edited: on a vertex, it is selected (and
    /// can be dragged); on an edge, a vertex is made there; elsewhere the
    /// editing ends. `true` if the press was taken (the vertex pressed, with its drag).
    pub(crate) fn edit_points_down(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) -> Option<(usize, usize)> {
        let layout = self.slide_layout()?;
        let id = self.edit_points_shape()?.id;
        let polygons = self.edit_point_polygons();
        let emu = layout.to_emu(position);
        let p = (emu.x as f64, emu.y as f64);
        let slop = HANDLE_HIT_PX as f64 / layout.scale as f64;
        // A vertex.
        for (l, polygon) in polygons.iter().enumerate() {
            for (i, v) in polygon.iter().enumerate() {
                if (v.0 - p.0).hypot(v.1 - p.1) <= slop {
                    if let Some(e) = self.pptx_mut().and_then(|s| s.edit_points.as_mut()) {
                        e.selected = Some((l, i));
                    }
                    cx.notify();
                    return Some((l, i));
                }
            }
        }
        // An edge: a vertex made on it.
        let mut best: Option<(f64, usize, usize, (f64, f64))> = None;
        for (l, polygon) in polygons.iter().enumerate() {
            let n = polygon.len();
            for i in 0..n {
                let (d, q) = nearest_on_segment(p, polygon[i], polygon[(i + 1) % n]);
                if d <= slop && best.is_none_or(|b| d < b.0) {
                    best = Some((d, l, i, q));
                }
            }
        }
        if let Some((_, l, i, q)) = best {
            let Some(shape) = self.slide().shape(id).cloned() else { return None };
            let mut new = polygons.clone();
            new[l].insert(i + 1, q);
            if self.replace_outline(&shape, &new, false, cx) {
                if let Some(e) = self.pptx_mut().and_then(|s| s.edit_points.as_mut()) {
                    e.selected = Some((l, i + 1));
                }
                return Some((l, i + 1));
            }
            return None;
        }
        self.end_edit_points(cx);
        None
    }

    /// Vertex (`l`, `i`) moved to `to` (slide EMU).
    pub(crate) fn move_vertex(&mut self, l: usize, i: usize, to: (f64, f64), cx: &mut Context<Self>) {
        let Some(shape) = self.edit_points_shape().cloned() else { return };
        let mut polygons = self.edit_point_polygons();
        let Some(v) = polygons.get_mut(l).and_then(|p| p.get_mut(i)) else { return };
        if *v == to {
            return;
        }
        *v = to;
        self.replace_outline(&shape, &polygons, false, cx);
    }

    /// Delete: the selected vertex taken away (a figure keeps three). `true` if there was one.
    pub(crate) fn delete_selected_vertex(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((l, i)) = self.edit_point_selected() else { return false };
        let Some(shape) = self.edit_points_shape().cloned() else { return false };
        let mut polygons = self.edit_point_polygons();
        if polygons.get(l).is_none_or(|p| p.len() <= 3 || i >= p.len()) {
            return true;
        }
        polygons[l].remove(i);
        if self.replace_outline(&shape, &polygons, false, cx)
            && let Some(e) = self.pptx_mut().and_then(|s| s.edit_points.as_mut())
        {
            e.selected = None;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::nearest_on_segment;

    #[test]
    fn nearest_points() {
        let (d, q) = nearest_on_segment((5.0, 3.0), (0.0, 0.0), (10.0, 0.0));
        assert_eq!((d, q), (3.0, (5.0, 0.0)));
        // Past the end: the end.
        let (d, q) = nearest_on_segment((13.0, 4.0), (0.0, 0.0), (10.0, 0.0));
        assert_eq!((d, q), (5.0, (10.0, 0.0)));
        assert_eq!(nearest_on_segment((1.0, 1.0), (2.0, 2.0), (2.0, 2.0)).0, 2f64.sqrt());
    }
}
