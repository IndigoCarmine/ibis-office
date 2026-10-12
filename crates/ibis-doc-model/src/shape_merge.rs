// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 図形の結合 (PowerPoint の「図形の書式」> 図形の結合): shapes made one by
//! the union, combination (合成), intersection (重なり抽出) or subtraction
//! (型抜き) of their outlines, or cut into the pieces they overlap in
//! (切り出し).
//!
//! The outlines are flattened to polygons (curves to short lines), cut into
//! vertical slabs between every x where an edge starts, ends or crosses
//! another; in a slab the edges do not cross, so the bands between them are
//! each inside or outside each shape, and the bands the operation keeps are
//! joined into the loops of the result.

use std::collections::HashMap;
use std::sync::Arc;

use crate::drawing::Geometry;
use crate::geometry::EmuRect;
use crate::model::{AutoShape, Shape, ShapeId, ShapeKind};
use crate::shape_geometry::{FormulaGeometry, GeometryDef, GeometryPath, PathFill, PathOp};

/// A point in EMU.
type Point = (f64, f64);
/// Closed polygons (the last point joins the first); a hole turns the other way.
pub type Polygons = Vec<Vec<Point>>;

/// What a merge makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeOp {
    /// 接合: everything the shapes cover.
    Union,
    /// 合成: what only one of the shapes covers.
    Combine,
    /// 重なり抽出: what all of them cover.
    Intersect,
    /// 型抜き: what the first covers and the others do not.
    Subtract,
    /// 切り出し: the pieces the shapes overlap in (and, for two, each alone).
    Fragment,
}

const EPS: f64 = 1e-6;

struct Edge {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    /// 0 for the first shape, 1 for the other.
    side: usize,
}

impl Edge {
    fn y_at(&self, x: f64) -> f64 {
        if x <= self.x0 {
            self.y0
        } else if x >= self.x1 {
            self.y1
        } else {
            self.y0 + (self.y1 - self.y0) * (x - self.x0) / (self.x1 - self.x0)
        }
    }
}

fn edges_of(polygons: &Polygons, side: usize, out: &mut Vec<Edge>) {
    for polygon in polygons {
        let n = polygon.len();
        for i in 0..n {
            let (a, b) = (polygon[i], polygon[(i + 1) % n]);
            // A vertical edge bounds no band (the slabs' sides stand for it).
            if (a.0 - b.0).abs() < EPS {
                continue;
            }
            let (l, r) = if a.0 < b.0 { (a, b) } else { (b, a) };
            out.push(Edge { x0: l.0, y0: l.1, x1: r.0, y1: r.1, side });
        }
    }
}

/// The x where edges `a` and `b` cross, if inside both.
fn crossing_x(a: &Edge, b: &Edge) -> Option<f64> {
    let (dax, day) = (a.x1 - a.x0, a.y1 - a.y0);
    let (dbx, dby) = (b.x1 - b.x0, b.y1 - b.y0);
    let denominator = dax * dby - day * dbx;
    if denominator.abs() < 1e-12 {
        return None;
    }
    let (ex, ey) = (b.x0 - a.x0, b.y0 - a.y0);
    let t = (ex * dby - ey * dbx) / denominator;
    let u = (ex * day - ey * dax) / denominator;
    ((-1e-9..=1.0 + 1e-9).contains(&t) && (-1e-9..=1.0 + 1e-9).contains(&u)).then(|| a.x0 + t * dax)
}

/// The key of a point on a grid of a thousandth of an EMU.
fn key(p: Point) -> (i64, i64) {
    ((p.0 * 1000.0).round() as i64, (p.1 * 1000.0).round() as i64)
}

struct Segments {
    list: Vec<(Point, Point)>,
}

/// Intervals `a` without those of `b` (both sorted, each merged).
fn minus(a: &[(f64, f64)], b: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for &(mut lo, hi) in a {
        for &(blo, bhi) in b {
            if bhi <= lo + EPS || blo >= hi - EPS {
                continue;
            }
            if blo > lo + EPS {
                out.push((lo, blo));
            }
            lo = lo.max(bhi);
        }
        if hi > lo + EPS {
            out.push((lo, hi));
        }
    }
    out
}

fn merge_intervals(mut v: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (lo, hi) in v {
        match out.last_mut() {
            Some(last) if lo <= last.1 + EPS => last.1 = last.1.max(hi),
            _ => out.push((lo, hi)),
        }
    }
    out
}

/// The polygons covered where `keep(in a, in b)` holds.
pub fn boolean(a: &Polygons, b: &Polygons, keep: impl Fn(bool, bool) -> bool) -> Polygons {
    let mut edges = Vec::new();
    edges_of(a, 0, &mut edges);
    edges_of(b, 1, &mut edges);
    if edges.is_empty() {
        return Vec::new();
    }
    // The x where the slabs begin and end.
    let mut xs: Vec<f64> = edges.iter().flat_map(|e| [e.x0, e.x1]).collect();
    for i in 0..edges.len() {
        for j in i + 1..edges.len() {
            if let Some(x) = crossing_x(&edges[i], &edges[j]) {
                xs.push(x);
            }
        }
    }
    // Vertical edges' x bound the slabs too.
    for polygons in [a, b] {
        for polygon in polygons {
            xs.extend(polygon.iter().map(|p| p.0));
        }
    }
    xs.sort_by(|p, q| p.total_cmp(q));
    xs.dedup_by(|p, q| (*p - *q).abs() < EPS);
    let mut segments = Segments { list: Vec::new() };
    // The intervals each slab keeps along its left and right sides.
    let mut sides: Vec<(Vec<(f64, f64)>, Vec<(f64, f64)>)> = Vec::new();
    for pair in xs.windows(2) {
        let (xl, xr) = (pair[0], pair[1]);
        let xm = (xl + xr) / 2.0;
        let mut active: Vec<&Edge> = edges.iter().filter(|e| e.x0 <= xl + EPS && e.x1 >= xr - EPS).collect();
        active.sort_by(|p, q| p.y_at(xm).total_cmp(&q.y_at(xm)).then(p.y_at(xl).total_cmp(&q.y_at(xl))));
        let mut inside = [false, false];
        let mut kept: Vec<bool> = Vec::new();
        let (mut left, mut right) = (Vec::new(), Vec::new());
        // Band `i` lies above `active[i]` (band 0 below the first edge: outside).
        let mut below_kept = false;
        for (i, edge) in active.iter().enumerate() {
            inside[edge.side] = !inside[edge.side];
            let above_kept = i + 1 < active.len() && keep(inside[0], inside[1]) && {
                let next = active[i + 1];
                next.y_at(xl) - edge.y_at(xl) > EPS || next.y_at(xr) - edge.y_at(xr) > EPS
            };
            kept.push(above_kept);
            if above_kept {
                let next = active[i + 1];
                left.push((edge.y_at(xl), next.y_at(xl)));
                right.push((edge.y_at(xr), next.y_at(xr)));
            }
            // A band's lower side runs left to right, its upper side right to left.
            if above_kept && !below_kept {
                segments.list.push(((xl, edge.y_at(xl)), (xr, edge.y_at(xr))));
            }
            if below_kept && !above_kept {
                segments.list.push(((xr, edge.y_at(xr)), (xl, edge.y_at(xl))));
            }
            below_kept = above_kept;
        }
        sides.push((merge_intervals(left), merge_intervals(right)));
    }
    // The sides between slabs: what one side keeps and the other does not.
    let empty: Vec<(f64, f64)> = Vec::new();
    for (k, &x) in xs.iter().enumerate() {
        let before = if k == 0 { &empty } else { &sides[k - 1].1 };
        let after = if k < sides.len() { &sides[k].0 } else { &empty };
        for (lo, hi) in minus(before, after) {
            segments.list.push(((x, lo), (x, hi)));
        }
        for (lo, hi) in minus(after, before) {
            segments.list.push(((x, hi), (x, lo)));
        }
    }
    join(segments.list)
}

/// The loops the directed segments make, without the segments met by their own reverse.
fn join(list: Vec<(Point, Point)>) -> Polygons {
    // Cells of the grid by the key of their start.
    let near = |index: &HashMap<(i64, i64), Vec<usize>>, p: Point| -> Vec<usize> {
        let (kx, ky) = key(p);
        let mut found = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(v) = index.get(&(kx + dx, ky + dy)) {
                    found.extend(v);
                }
            }
        }
        found
    };
    let close = |p: Point, q: Point| (p.0 - q.0).abs() < 2e-3 && (p.1 - q.1).abs() < 2e-3;
    let mut index: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, s) in list.iter().enumerate() {
        index.entry(key(s.0)).or_default().push(i);
    }
    let mut alive = vec![true; list.len()];
    // A segment and its reverse are no boundary.
    for i in 0..list.len() {
        if !alive[i] {
            continue;
        }
        let (p, q) = list[i];
        if let Some(&j) = near(&index, q).iter().find(|&&j| j != i && alive[j] && close(list[j].0, q) && close(list[j].1, p)) {
            alive[i] = false;
            alive[j] = false;
        }
    }
    let mut polygons: Polygons = Vec::new();
    for start in 0..list.len() {
        if !alive[start] {
            continue;
        }
        alive[start] = false;
        let first = list[start].0;
        let mut loop_points = vec![first];
        let mut end = list[start].1;
        let mut guard = 0;
        while !close(end, first) && guard < list.len() + 2 {
            guard += 1;
            loop_points.push(end);
            let Some(next) = near(&index, end).into_iter().find(|&j| alive[j] && close(list[j].0, end)) else { break };
            alive[next] = false;
            end = list[next].1;
        }
        if close(end, first) {
            polygons.push(simplify(loop_points));
        }
    }
    polygons.retain(|p| p.len() >= 3 && area(p).abs() > 1e-3);
    polygons
}

/// The points of a loop without the ones on the line between their neighbours.
fn simplify(points: Vec<Point>) -> Vec<Point> {
    let mut points = points;
    loop {
        let n = points.len();
        if n < 4 {
            return points;
        }
        let mut removed = false;
        let mut i = 0;
        while i < points.len() && points.len() > 3 {
            let n = points.len();
            let (p, q, r) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
            let (ux, uy, vx, vy) = (q.0 - p.0, q.1 - p.1, r.0 - q.0, r.1 - q.1);
            let length = ux.hypot(uy).max(vx.hypot(vy)).max(1e-9);
            // On the line: the distance of `q` from `p`-`r` is below a hundredth of an EMU.
            let cross = (ux * vy - uy * vx).abs();
            if cross / (p.0 - r.0).hypot(p.1 - r.1).max(1e-9) < 0.01 && (ux * vx + uy * vy) > 0.0 || length < 1e-6 {
                points.remove(i);
                removed = true;
            } else {
                i += 1;
            }
        }
        if !removed {
            return points;
        }
    }
}

/// The signed area of a polygon (shoelace).
pub fn area(polygon: &[Point]) -> f64 {
    let n = polygon.len();
    (0..n).map(|i| polygon[i].0 * polygon[(i + 1) % n].1 - polygon[(i + 1) % n].0 * polygon[i].1).sum::<f64>() / 2.0
}

/// The area the polygons cover (holes turn the other way, so they take theirs off).
pub fn covered(polygons: &Polygons) -> f64 {
    // Outer loops and holes are told apart by their direction.
    let total: f64 = polygons.iter().map(|p| area(p)).sum();
    let outer = polygons.iter().map(|p| area(p)).fold(0.0, |s: f64, a| if a > 0.0 { s + a } else { s });
    let inner = polygons.iter().map(|p| area(p)).fold(0.0, |s: f64, a| if a < 0.0 { s + a } else { s });
    if outer.abs() >= inner.abs() { total } else { -total }
}

/// A shape's outline as polygons in slide coordinates; `None` for a shape that is not a
/// closed figure (a picture, a table, a line).
pub fn shape_polygons(shape: &Shape) -> Option<Polygons> {
    let ShapeKind::Auto(auto) = &shape.kind else { return None };
    let (w, h) = (shape.frame.width as f64, shape.frame.height as f64);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let mut local: Polygons = Vec::new();
    match &auto.geometry {
        Geometry::Rect => local.push(vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]),
        Geometry::Ellipse => local.push((0..72).map(|i| {
            let t = i as f64 / 72.0 * std::f64::consts::TAU;
            (w / 2.0 + w / 2.0 * t.cos(), h / 2.0 + h / 2.0 * t.sin())
        }).collect()),
        Geometry::RoundRect { radius } => {
            let r = (w.min(h) * *radius as f64 / 100_000.0).min(w.min(h) / 2.0);
            let mut points = Vec::new();
            for (cx, cy, start) in [(w - r, r, -90.0f64), (w - r, h - r, 0.0), (r, h - r, 90.0), (r, r, 180.0)] {
                for i in 0..=8 {
                    let t = (start + i as f64 * 90.0 / 8.0).to_radians();
                    points.push((cx + r * t.cos(), cy + r * t.sin()));
                }
            }
            local.push(points);
        }
        Geometry::Formula(f) => {
            for outline in f.outline(w, h) {
                if outline.fill == PathFill::None {
                    continue;
                }
                for sub in outline.subpaths {
                    if sub.points.len() >= 3 {
                        local.push(sub.points);
                    }
                }
            }
        }
    }
    if local.is_empty() {
        return None;
    }
    let angle = (shape.rotation as f64 / 60_000.0).to_radians();
    let (sin, cos) = angle.sin_cos();
    let (cx, cy) = (w / 2.0, h / 2.0);
    for polygon in &mut local {
        for p in polygon.iter_mut() {
            let (mut x, mut y) = *p;
            if shape.flip_h {
                x = w - x;
            }
            if shape.flip_v {
                y = h - y;
            }
            let (dx, dy) = (x - cx, y - cy);
            *p = (shape.frame.x as f64 + cx + dx * cos - dy * sin, shape.frame.y as f64 + cy + dx * sin + dy * cos);
        }
        // Flips turn a polygon round.
        if shape.flip_h != shape.flip_v {
            polygon.reverse();
        }
    }
    Some(local)
}

/// A shape of `polygons` (slide coordinates) with the look of `like` (fill,
/// outline, effects; no text), as a custom geometry.
pub fn shape_of(id: ShapeId, like: &Shape, polygons: &Polygons) -> Option<Shape> {
    build(id, like, polygons, false)
}

/// As [`shape_of`], keeping the text of `like` (頂点の編集 changes the outline only).
pub fn reshaped(like: &Shape, polygons: &Polygons) -> Option<Shape> {
    build(like.id, like, polygons, true)
}

fn build(id: ShapeId, like: &Shape, polygons: &Polygons, keep_text: bool) -> Option<Shape> {
    let ShapeKind::Auto(auto) = &like.kind else { return None };
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in polygons.iter().flatten() {
        min_x = min_x.min(p.0);
        min_y = min_y.min(p.1);
        max_x = max_x.max(p.0);
        max_y = max_y.max(p.1);
    }
    if polygons.is_empty() || max_x - min_x < 1.0 || max_y - min_y < 1.0 {
        return None;
    }
    let (ox, oy) = (min_x.round() as i64, min_y.round() as i64);
    let frame = EmuRect::new(ox, oy, (max_x.round() as i64 - ox).max(1), (max_y.round() as i64 - oy).max(1));
    let mut ops = Vec::new();
    for polygon in polygons {
        for (i, p) in polygon.iter().enumerate() {
            let (x, y) = ((p.0.round() as i64 - ox).clamp(0, frame.width), (p.1.round() as i64 - oy).clamp(0, frame.height));
            ops.push(if i == 0 { PathOp::MoveTo(x.to_string(), y.to_string()) } else { PathOp::LineTo(x.to_string(), y.to_string()) });
        }
        ops.push(PathOp::Close);
    }
    let path = GeometryPath { width: frame.width, height: frame.height, fill: PathFill::Normal, stroke: true, ops };
    let def = GeometryDef { adjust: Vec::new(), guides: Vec::new(), paths: vec![path], text_rect: None };
    let geometry = Geometry::Formula(FormulaGeometry { preset: None, adjust: Vec::new(), def: Arc::new(def) });
    let shape_auto = AutoShape { geometry, text: if keep_text { auto.text.clone() } else { None }, ..auto.clone() };
    Some(Shape::new(id, frame, ShapeKind::Auto(shape_auto)))
}

/// The shapes `op` makes of `shapes` (at least two, in the order they were
/// chosen: the first gives the look), with ids from `new_id`; `None` if one is not a
/// closed figure or nothing is left.
pub fn merge(shapes: &[&Shape], op: MergeOp, mut new_id: impl FnMut() -> ShapeId) -> Option<Vec<Shape>> {
    if shapes.len() < 2 {
        return None;
    }
    let polygons: Vec<Polygons> = shapes.iter().map(|s| shape_polygons(s)).collect::<Option<_>>()?;
    let results: Vec<Polygons> = match op {
        MergeOp::Union | MergeOp::Intersect | MergeOp::Subtract | MergeOp::Combine => {
            let keep: fn(bool, bool) -> bool = match op {
                MergeOp::Union => |a, b| a || b,
                MergeOp::Intersect => |a, b| a && b,
                MergeOp::Subtract => |a, b| a && !b,
                _ => |a, b| a != b,
            };
            let mut result = polygons[0].clone();
            for next in &polygons[1..] {
                result = boolean(&result, next, keep);
            }
            vec![result]
        }
        MergeOp::Fragment => {
            // The pieces of all the shapes: each region covered by the same shapes.
            // Two at a time, left to right: what is in the first of each only, both, and the second only.
            let mut pieces: Vec<Polygons> = vec![polygons[0].clone()];
            for next in &polygons[1..] {
                let mut made: Vec<Polygons> = Vec::new();
                for piece in &pieces {
                    made.push(boolean(piece, next, |a, b| a && !b));
                    made.push(boolean(piece, next, |a, b| a && b));
                }
                made.push(boolean(&none_union(&pieces), next, |a, b| !a && b));
                pieces = made;
            }
            pieces
        }
    };
    let shapes_made: Vec<Shape> = results.iter().filter(|r| !r.is_empty()).filter_map(|r| shape_of(new_id(), shapes[0], r)).collect();
    (!shapes_made.is_empty()).then_some(shapes_made)
}

/// The union of pieces.
fn none_union(pieces: &[Polygons]) -> Polygons {
    let mut union: Polygons = Vec::new();
    for piece in pieces {
        union = boolean(&union, piece, |a, b| a || b);
    }
    union
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Polygons {
        vec![vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)]]
    }

    fn square(x: i64, y: i64, size: i64) -> Shape {
        Shape::new(ShapeId(1), EmuRect::new(x, y, size, size), ShapeKind::Auto(AutoShape::new(Geometry::Rect, crate::Fill::None, None)))
    }

    #[test]
    fn rectangles() {
        let (a, b) = (rect(0.0, 0.0, 100.0, 100.0), rect(50.0, 50.0, 100.0, 100.0));
        let union = boolean(&a, &b, |p, q| p || q);
        assert!((covered(&union) - 17_500.0).abs() < 1e-6, "{}", covered(&union));
        assert_eq!(union.len(), 1);
        // An L-shaped union has six corners.
        assert_eq!(union[0].len(), 8);
        let both = boolean(&a, &b, |p, q| p && q);
        assert!((covered(&both) - 2_500.0).abs() < 1e-6);
        assert_eq!(both[0].len(), 4);
        let minus = boolean(&a, &b, |p, q| p && !q);
        assert!((covered(&minus) - 7_500.0).abs() < 1e-6);
        let xor = boolean(&a, &b, |p, q| p != q);
        assert!((covered(&xor) - 15_000.0).abs() < 1e-6);
        // Apart: nothing in common.
        assert!(boolean(&a, &rect(500.0, 0.0, 10.0, 10.0), |p, q| p && q).is_empty());
    }

    #[test]
    fn a_hole() {
        // A small square inside a large one: subtracting leaves a ring (two loops).
        let ring = boolean(&rect(0.0, 0.0, 100.0, 100.0), &rect(30.0, 30.0, 40.0, 40.0), |p, q| p && !q);
        assert_eq!(ring.len(), 2);
        assert!((covered(&ring) - (10_000.0 - 1_600.0)).abs() < 1e-6, "{}", covered(&ring));
        // Shapes sharing an edge join into one.
        let joined = boolean(&rect(0.0, 0.0, 50.0, 100.0), &rect(50.0, 0.0, 50.0, 100.0), |p, q| p || q);
        assert_eq!(joined.len(), 1);
        assert_eq!(joined[0].len(), 4);
        assert!((covered(&joined) - 10_000.0).abs() < 1e-6);
    }

    #[test]
    fn circles() {
        let a = shape_polygons(&Shape::new(ShapeId(1), EmuRect::new(0, 0, 1000, 1000), ShapeKind::Auto(AutoShape::new(Geometry::Ellipse, crate::Fill::None, None)))).unwrap();
        let b = shape_polygons(&Shape::new(ShapeId(2), EmuRect::new(500, 0, 1000, 1000), ShapeKind::Auto(AutoShape::new(Geometry::Ellipse, crate::Fill::None, None)))).unwrap();
        let (one, two) = (covered(&a), covered(&b));
        let union = covered(&boolean(&a, &b, |p, q| p || q));
        let both = covered(&boolean(&a, &b, |p, q| p && q));
        // Inclusion and exclusion.
        assert!((union + both - one - two).abs() < one * 1e-6, "{union} {both} {one} {two}");
        assert!(both > 0.0 && union > one);
    }

    /// A star-like polygon (concave) with `n` points, from a seed.
    fn star(seed: u64, n: usize, cx: f64, cy: f64, size: f64) -> Polygons {
        let mut state = seed;
        let mut next = || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (state >> 33) as f64 / (1u64 << 31) as f64
        };
        vec![(0..n).map(|i| {
            let t = i as f64 / n as f64 * std::f64::consts::TAU;
            let r = size * (0.35 + 0.65 * next());
            (cx + r * t.cos(), cy + r * t.sin())
        }).collect()]
    }

    /// Whatever the shapes, the union and the overlap together cover what the two cover; a
    /// subtraction is the first less the overlap; a combination the union less the overlap.
    #[test]
    fn areas_add_up() {
        for seed in 1..60u64 {
            let (a, b) = (star(seed, 9, 0.0, 0.0, 1000.0), star(seed * 7 + 3, 11, 300.0 + seed as f64 * 3.0, 150.0, 900.0));
            let (area_a, area_b) = (area(&a[0]).abs(), area(&b[0]).abs());
            let union = covered(&boolean(&a, &b, |p, q| p || q)).abs();
            let both = covered(&boolean(&a, &b, |p, q| p && q)).abs();
            let minus = covered(&boolean(&a, &b, |p, q| p && !q)).abs();
            let xor = covered(&boolean(&a, &b, |p, q| p != q)).abs();
            let tolerance = (area_a + area_b) * 1e-6;
            assert!((union + both - area_a - area_b).abs() < tolerance, "seed {seed}: {union} + {both} vs {area_a} + {area_b}");
            assert!((minus - (area_a - both)).abs() < tolerance, "seed {seed}: minus {minus}");
            assert!((xor - (union - both)).abs() < tolerance, "seed {seed}: xor {xor}");
        }
    }

    #[test]
    fn merged_shapes() {
        let (a, b) = (square(0, 0, 1000), square(500, 500, 1000));
        let mut next = 10;
        let made = merge(&[&a, &b], MergeOp::Union, || {
            next += 1;
            ShapeId(next)
        })
        .unwrap();
        assert_eq!(made.len(), 1);
        assert_eq!(made[0].frame, EmuRect::new(0, 0, 1500, 1500));
        let fragments = merge(&[&a, &b], MergeOp::Fragment, || ShapeId(5)).unwrap();
        // The first alone, the overlap, the second alone.
        assert_eq!(fragments.len(), 3);
        // A rotated square about its centre keeps its area.
        let mut turned = square(0, 0, 1000);
        turned.rotation = 45 * 60_000;
        let polygons = shape_polygons(&turned).unwrap();
        assert!((area(&polygons[0]).abs() - 1_000_000.0).abs() < 1.0);
        // Not a figure: no merge.
        assert!(merge(&[&a], MergeOp::Union, || ShapeId(1)).is_none());
    }
}
