// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 変形 (Morph, 画面切り替え): the shapes the two slides have in common move,
//! resize and turn from where they were to where they are, the others fade out
//! and in. Shapes are paired by name (`!!name`, PowerPoint's way to say which
//! is which), else by what they hold: their text, their picture, their
//! geometry. 単語・文字単位 is played as by object.
use std::collections::HashMap;

use gpui_kit::*;
use ibis_doc_model::{EmuRect, Geometry, Shape, ShapeKind, Slide};

use crate::render::shape_view::{shape_element, slide_backdrop};
use crate::shell::editor::SlideEditor;

/// Which shapes of the new slide are which of the old one.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Pairing {
    /// (index in the old slide's shown shapes, index in the new one's).
    pub(crate) pairs: Vec<(usize, usize)>,
    pub(crate) old_only: Vec<usize>,
    pub(crate) new_only: Vec<usize>,
}

fn preset(geometry: &Geometry) -> String {
    match geometry {
        Geometry::Rect => "rect".into(),
        Geometry::RoundRect { .. } => "roundRect".into(),
        Geometry::Ellipse => "ellipse".into(),
        Geometry::Formula(f) => f.preset.clone().unwrap_or_else(|| "custom".into()),
    }
}

/// What makes a shape the same as another one: `None` for what is not paired.
fn key(slide: &Slide, shape: &Shape) -> Option<String> {
    if let Some(name) = slide.shape_names.get(&shape.id).filter(|n| n.starts_with("!!")) {
        return Some(format!("name:{name}"));
    }
    match &shape.kind {
        ShapeKind::Auto(auto) => {
            let text = auto.text.as_ref().map(|t| t.text()).unwrap_or_default();
            Some(if text.trim().is_empty() { format!("auto:{}", preset(&auto.geometry)) } else { format!("auto:{}:{text}", preset(&auto.geometry)) })
        }
        ShapeKind::TextBox(body) => Some(format!("text:{}", body.text())),
        ShapeKind::Picture(p) => Some(format!("pic:{:?}", p.media)),
        _ => None,
    }
}

/// The shapes of `new` that are those of `old`, in order: each of the old
/// ones taken once.
pub(crate) fn pair_shapes(old: &Slide, new: &Slide) -> Pairing {
    let olds: Vec<&Shape> = old.shown_shapes().collect();
    let news: Vec<&Shape> = new.shown_shapes().collect();
    let old_keys: Vec<Option<String>> = olds.iter().map(|s| key(old, s)).collect();
    let mut taken = vec![false; olds.len()];
    let mut pairs = Vec::new();
    let mut new_only = Vec::new();
    // Named ones first (they say which is which), then by what they hold.
    let mut matched: HashMap<usize, usize> = HashMap::new();
    for pass in 0..2 {
        for (j, shape) in news.iter().enumerate() {
            if matched.contains_key(&j) {
                continue;
            }
            let Some(k) = key(new, shape) else { continue };
            if (pass == 0) != k.starts_with("name:") {
                continue;
            }
            if let Some(i) = (0..olds.len()).find(|&i| !taken[i] && old_keys[i].as_deref() == Some(k.as_str())) {
                taken[i] = true;
                matched.insert(j, i);
            }
        }
    }
    for j in 0..news.len() {
        match matched.get(&j) {
            Some(&i) => pairs.push((i, j)),
            None => new_only.push(j),
        }
    }
    let old_only = (0..olds.len()).filter(|&i| !taken[i]).collect();
    Pairing { pairs, old_only, new_only }
}

/// The frame and turn of a shape part of the way (`t`, 0 to 1) from `a` to `b`.
pub(crate) fn between(a: &Shape, b: &Shape, t: f64) -> (EmuRect, i32) {
    let lerp = |x: i64, y: i64| (x as f64 + (y - x) as f64 * t).round() as i64;
    let frame = EmuRect::new(lerp(a.frame.x, b.frame.x), lerp(a.frame.y, b.frame.y), lerp(a.frame.width, b.frame.width), lerp(a.frame.height, b.frame.height));
    // The short way round.
    const FULL: i64 = 21_600_000;
    let mut delta = (b.rotation as i64 - a.rotation as i64).rem_euclid(FULL);
    if delta > FULL / 2 {
        delta -= FULL;
    }
    (frame, (a.rotation as f64 + delta as f64 * t).round().rem_euclid(FULL as f64) as i32)
}

impl SlideEditor {
    /// The slide `slide` coming in from `before` as a morph, `t` (0 to 1) of the way.
    pub(crate) fn morph_page(&self, before: &Slide, slide: &Slide, t: f32, scale: f32) -> AnyElement {
        let pairing = pair_shapes(before, slide);
        let olds: Vec<&Shape> = before.shown_shapes().collect();
        let news: Vec<&Shape> = slide.shown_shapes().collect();
        let layer = |opacity: f32, element: AnyElement| div().absolute().top_0().left_0().size_full().opacity(opacity).child(element).into_any_element();
        let mut page = div().absolute().top_0().left_0().size_full().overflow_hidden().bg(white());
        page = page.child(slide_backdrop(before, scale, &self.resources)).child(layer(t, slide_backdrop(slide, scale, &self.resources).into_any_element()));
        for &i in &pairing.old_only {
            page = page.child(layer(1.0 - t, shape_element(olds[i], olds[i].frame, scale, &self.resources, None)));
        }
        for &(i, j) in &pairing.pairs {
            let (frame, rotation) = between(olds[i], news[j], t as f64);
            let mut old = olds[i].clone();
            old.rotation = rotation;
            let mut new = news[j].clone();
            new.rotation = rotation;
            page = page.child(layer(1.0, shape_element(&old, frame, scale, &self.resources, None))).child(layer(t, shape_element(&new, frame, scale, &self.resources, None)));
        }
        for &j in &pairing.new_only {
            page = page.child(layer(t, shape_element(news[j], news[j].frame, scale, &self.resources, None)));
        }
        page.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{between, pair_shapes, Pairing};
    use ibis_doc_model::{AutoShape, EmuRect, Fill, Geometry, Shape, ShapeId, ShapeKind, Slide, SlideId, TextBody};

    fn shape(id: u64, frame: EmuRect, geometry: Geometry, text: &str) -> Shape {
        let mut auto = AutoShape::new(geometry, Fill::None, None);
        if !text.is_empty() {
            auto.text = Some(TextBody::plain(text, Default::default()));
        }
        Shape::new(ShapeId(id), frame, ShapeKind::Auto(auto))
    }

    fn slide(shapes: Vec<Shape>) -> Slide {
        let mut s = Slide::new(SlideId(1));
        s.shapes = shapes;
        s
    }

    #[test]
    fn shapes_are_paired_by_what_they_hold() {
        let f = EmuRect::new(0, 0, 100, 100);
        let old = slide(vec![shape(1, f, Geometry::Rect, "A"), shape(2, f, Geometry::Ellipse, ""), shape(3, f, Geometry::Rect, "gone")]);
        let new = slide(vec![shape(4, f, Geometry::Ellipse, ""), shape(5, f, Geometry::Rect, "A"), shape(6, f, Geometry::Rect, "new")]);
        assert_eq!(pair_shapes(&old, &new), Pairing { pairs: vec![(1, 0), (0, 1)], old_only: vec![2], new_only: vec![2] });
        // Named shapes pair by name, whatever they hold.
        let mut old = slide(vec![shape(1, f, Geometry::Rect, "one"), shape(2, f, Geometry::Rect, "two")]);
        let mut new = slide(vec![shape(3, f, Geometry::Ellipse, "x"), shape(4, f, Geometry::Rect, "two")]);
        old.shape_names.insert(ShapeId(1), "!!title".into());
        new.shape_names.insert(ShapeId(3), "!!title".into());
        assert_eq!(pair_shapes(&old, &new), Pairing { pairs: vec![(0, 0), (1, 1)], old_only: vec![], new_only: vec![] });
        // Each old shape once.
        let old = slide(vec![shape(1, f, Geometry::Rect, "A")]);
        let new = slide(vec![shape(2, f, Geometry::Rect, "A"), shape(3, f, Geometry::Rect, "A")]);
        assert_eq!(pair_shapes(&old, &new).pairs, vec![(0, 0)]);
    }

    #[test]
    fn halfway() {
        let mut a = shape(1, EmuRect::new(0, 0, 100, 100), Geometry::Rect, "");
        let mut b = shape(2, EmuRect::new(200, 100, 300, 100), Geometry::Rect, "");
        a.rotation = 21_600_000 - 600_000;
        b.rotation = 600_000;
        let (frame, rotation) = between(&a, &b, 0.5);
        assert_eq!(frame, EmuRect::new(100, 50, 200, 100));
        // The short way round: through 0, not the long way.
        assert_eq!(rotation, 0);
        assert_eq!(between(&a, &b, 0.0).0, a.frame);
        assert_eq!(between(&a, &b, 1.0).0, b.frame);
    }
}
