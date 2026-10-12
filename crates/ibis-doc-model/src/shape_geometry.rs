// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! DrawingML shape geometry: guide formulas and paths.
//!
//! Preset shapes (`a:prstGeom`) and custom shapes (`a:custGeom`) describe
//! their outline with named guides computed from the shape's size and
//! adjust values, and paths whose points refer to those guides. The outline
//! is computed for the size a shape is drawn at, so it follows resizing.

use std::collections::HashMap;
use std::sync::Arc;

/// A named value and the formula that computes it, such as
/// `("x1", "*/ w adj1 100000")`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Guide {
    pub name: String,
    pub formula: String,
}

/// One step of a geometry path. Arguments are guide names or numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathOp {
    MoveTo(String, String),
    LineTo(String, String),
    /// Elliptical arc from the current point: radii, start angle and sweep
    /// (60 000ths of a degree), as `a:arcTo`.
    ArcTo { wr: String, hr: String, start: String, sweep: String },
    QuadTo([String; 4]),
    CubicTo([String; 6]),
    Close,
}

/// How a path is filled, as `a:path fill`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PathFill {
    #[default]
    Normal,
    None,
    Lighten,
    LightenLess,
    Darken,
    DarkenLess,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeometryPath {
    /// Coordinate space of the points; 0 means the shape's own size.
    pub width: i64,
    pub height: i64,
    pub fill: PathFill,
    pub stroke: bool,
    pub ops: Vec<PathOp>,
}

/// A shape's geometry definition: default adjust values, guides and paths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GeometryDef {
    pub adjust: Vec<Guide>,
    pub guides: Vec<Guide>,
    pub paths: Vec<GeometryPath>,
    /// The text rectangle (`a:rect`): left, top, right and bottom, as guide
    /// names or formulas' values; `None` is the whole shape.
    pub text_rect: Option<[String; 4]>,
}

/// The geometry of one shape: a definition (shared between shapes using the
/// same preset) and the shape's own adjust values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaGeometry {
    /// The preset's name (`bentUpArrow`), or `None` for a custom geometry.
    pub preset: Option<String>,
    pub adjust: Vec<Guide>,
    pub def: Arc<GeometryDef>,
}

/// A path computed for a size: subpaths of points in shape coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Outline {
    pub subpaths: Vec<Subpath>,
    pub fill: PathFill,
    pub stroke: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Subpath {
    pub points: Vec<(f64, f64)>,
    pub closed: bool,
}

/// 60 000ths of a degree to radians.
fn radians(angle: f64) -> f64 {
    (angle / 60_000.0).to_radians()
}

fn angle_of(r: f64) -> f64 {
    r.to_degrees() * 60_000.0
}

struct Values {
    values: HashMap<String, f64>,
}

impl Values {
    /// The built-in variables for a shape `w` × `h` (ECMA-376 §20.1.10.56).
    fn new(w: f64, h: f64) -> Self {
        let ss = w.min(h);
        let mut values = HashMap::new();
        let mut set = |name: &str, v: f64| {
            values.insert(name.to_string(), v);
        };
        set("w", w);
        set("h", h);
        set("l", 0.0);
        set("t", 0.0);
        set("r", w);
        set("b", h);
        set("hc", w / 2.0);
        set("vc", h / 2.0);
        set("ss", ss);
        set("ls", w.max(h));
        for d in [2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 32.0] {
            set(&format!("wd{d}"), w / d);
            set(&format!("hd{d}"), h / d);
        }
        for d in [2.0, 4.0, 6.0, 8.0, 16.0, 32.0] {
            set(&format!("ssd{d}"), ss / d);
        }
        set("cd2", 10_800_000.0);
        set("cd4", 5_400_000.0);
        set("cd8", 2_700_000.0);
        set("3cd4", 16_200_000.0);
        set("3cd8", 8_100_000.0);
        set("5cd8", 13_500_000.0);
        set("7cd8", 18_900_000.0);
        Self { values }
    }

    fn get(&self, arg: &str) -> f64 {
        match self.values.get(arg) {
            Some(v) => *v,
            None => arg.parse().unwrap_or(0.0),
        }
    }

    /// Evaluate one guide formula (ECMA-376 §20.1.9.11).
    fn eval(&self, formula: &str) -> f64 {
        let mut parts = formula.split_whitespace();
        let op = parts.next().unwrap_or("val");
        let a: Vec<f64> = parts.map(|p| self.get(p)).collect();
        let arg = |i: usize| a.get(i).copied().unwrap_or(0.0);
        let (x, y, z) = (arg(0), arg(1), arg(2));
        match op {
            "val" => x,
            "*/" => if z == 0.0 { 0.0 } else { x * y / z },
            "+-" => x + y - z,
            "+/" => if z == 0.0 { 0.0 } else { (x + y) / z },
            "?:" => if x > 0.0 { y } else { z },
            "abs" => x.abs(),
            "at2" => angle_of(y.atan2(x)),
            "cat2" => x * z.atan2(y).cos(),
            "sat2" => x * z.atan2(y).sin(),
            "cos" => x * radians(y).cos(),
            "sin" => x * radians(y).sin(),
            "tan" => x * radians(y).tan(),
            "max" => x.max(y),
            "min" => x.min(y),
            "mod" => (x * x + y * y + z * z).sqrt(),
            "pin" => {
                if y < x {
                    x
                } else if y > z {
                    z
                } else {
                    y
                }
            }
            "sqrt" => x.max(0.0).sqrt(),
            _ => 0.0,
        }
    }

    fn set_all(&mut self, guides: &[Guide]) {
        for g in guides {
            let v = self.eval(&g.formula);
            self.values.insert(g.name.clone(), v);
        }
    }
}

impl FormulaGeometry {
    /// The outline for a shape `w` × `h`, in the shape's coordinates.
    pub fn outline(&self, w: f64, h: f64) -> Vec<Outline> {
        let mut v = Values::new(w, h);
        v.set_all(&self.def.adjust);
        // The shape's adjust values replace the defaults of the same name.
        v.set_all(&self.adjust);
        v.set_all(&self.def.guides);
        self.def.paths.iter().map(|p| outline_path(p, &v, w, h)).collect()
    }

    /// The text rectangle for a shape `w` × `h` as left, top, right, bottom.
    pub fn text_rect(&self, w: f64, h: f64) -> (f64, f64, f64, f64) {
        if matches!(self.preset.as_deref(), Some("line" | "lineInv" | "straightConnector1")) {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let Some([l, t, r, b]) = &self.def.text_rect else { return (0.0, 0.0, w, h) };
        let mut v = Values::new(w, h);
        v.set_all(&self.def.adjust);
        v.set_all(&self.adjust);
        v.set_all(&self.def.guides);
        (v.get(l), v.get(t), v.get(r), v.get(b))
    }
}

fn outline_path(path: &GeometryPath, v: &Values, w: f64, h: f64) -> Outline {
    let sx = if path.width > 0 { w / path.width as f64 } else { 1.0 };
    let sy = if path.height > 0 { h / path.height as f64 } else { 1.0 };
    let pt = |x: &str, y: &str| (v.get(x) * sx, v.get(y) * sy);
    let mut subpaths: Vec<Subpath> = Vec::new();
    let mut current = (0.0, 0.0);
    let open = |subpaths: &mut Vec<Subpath>, at: (f64, f64)| {
        if subpaths.last().is_none_or(|s| s.closed || !s.points.is_empty()) {
            subpaths.push(Subpath { points: vec![at], closed: false });
        } else if let Some(s) = subpaths.last_mut() {
            s.points.push(at);
        }
    };
    for op in &path.ops {
        match op {
            PathOp::MoveTo(x, y) => {
                current = pt(x, y);
                subpaths.push(Subpath { points: vec![current], closed: false });
            }
            PathOp::LineTo(x, y) => {
                if subpaths.last().is_none_or(|s| s.closed) {
                    open(&mut subpaths, current);
                }
                current = pt(x, y);
                subpaths.last_mut().expect("a subpath").points.push(current);
            }
            PathOp::QuadTo([cx, cy, x, y]) => {
                if subpaths.last().is_none_or(|s| s.closed) {
                    open(&mut subpaths, current);
                }
                let (c, t) = (pt(cx, cy), pt(x, y));
                let points = &mut subpaths.last_mut().expect("a subpath").points;
                for i in 1..=12 {
                    let s = i as f64 / 12.0;
                    let u = 1.0 - s;
                    points.push((u * u * current.0 + 2.0 * u * s * c.0 + s * s * t.0, u * u * current.1 + 2.0 * u * s * c.1 + s * s * t.1));
                }
                current = t;
            }
            PathOp::CubicTo([ax, ay, bx, by, x, y]) => {
                if subpaths.last().is_none_or(|s| s.closed) {
                    open(&mut subpaths, current);
                }
                let (c1, c2, t) = (pt(ax, ay), pt(bx, by), pt(x, y));
                let points = &mut subpaths.last_mut().expect("a subpath").points;
                for i in 1..=16 {
                    let s = i as f64 / 16.0;
                    let u = 1.0 - s;
                    let x = u * u * u * current.0 + 3.0 * u * u * s * c1.0 + 3.0 * u * s * s * c2.0 + s * s * s * t.0;
                    let y = u * u * u * current.1 + 3.0 * u * u * s * c1.1 + 3.0 * u * s * s * c2.1 + s * s * s * t.1;
                    points.push((x, y));
                }
                current = t;
            }
            PathOp::ArcTo { wr, hr, start, sweep } => {
                if subpaths.last().is_none_or(|s| s.closed) {
                    open(&mut subpaths, current);
                }
                let (rx, ry) = (v.get(wr) * sx, v.get(hr) * sy);
                let (start, sweep) = (v.get(start), v.get(sweep));
                let arc = arc_points(current, rx, ry, start, sweep);
                if let Some(last) = arc.last() {
                    current = *last;
                }
                subpaths.last_mut().expect("a subpath").points.extend(arc);
            }
            PathOp::Close => {
                if let Some(s) = subpaths.last_mut() {
                    s.closed = true;
                    if let Some(first) = s.points.first() {
                        current = *first;
                    }
                }
            }
        }
    }
    subpaths.retain(|s| s.points.len() > 1);
    Outline { subpaths, fill: path.fill, stroke: path.stroke }
}

/// Points of an `a:arcTo` from `from`. Angles are measured as on the
/// ellipse's circle before it is stretched (so 45° on a wide ellipse is
/// the point in its corner direction), in 60 000ths of a degree.
fn arc_points(from: (f64, f64), rx: f64, ry: f64, start: f64, sweep: f64) -> Vec<(f64, f64)> {
    // The parametric angle of a point at `angle` from the centre.
    let param = |angle: f64| {
        let a = radians(angle);
        (a.sin() * rx).atan2(a.cos() * ry)
    };
    let t0 = param(start);
    let center = (from.0 - rx * t0.cos(), from.1 - ry * t0.sin());
    let steps = ((sweep.abs() / 60_000.0 / 5.0).ceil() as usize).clamp(2, 128);
    let tau = std::f64::consts::TAU;
    let mut previous = t0;
    (1..=steps)
        .map(|i| {
            // Unwrap: the nearest turn of the angle to the previous point's.
            let mut t = param(start + sweep * i as f64 / steps as f64);
            t += ((previous - t) / tau).round() * tau;
            previous = t;
            (center.0 + rx * t.cos(), center.1 + ry * t.sin())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guide(name: &str, formula: &str) -> Guide {
        Guide { name: name.into(), formula: formula.into() }
    }

    fn s(v: &str) -> String {
        v.into()
    }

    #[test]
    fn formulas() {
        let v = Values::new(200.0, 100.0);
        assert_eq!(v.eval("*/ w 1 4"), 50.0);
        assert_eq!(v.eval("+- w h 50"), 250.0);
        assert_eq!(v.eval("+/ w h 3"), 100.0);
        assert_eq!(v.eval("?: -1 5 7"), 7.0);
        assert_eq!(v.eval("pin 0 150 100"), 100.0);
        assert_eq!(v.eval("min w h"), 100.0);
        assert!((v.eval("at2 1 1") - 2_700_000.0).abs() < 1e-6);
        assert!((v.eval("cos 100 cd4")).abs() < 1e-9);
        assert_eq!(v.eval("mod 3 4 0"), 5.0);
        assert_eq!(v.get("ssd4"), 25.0);
    }

    #[test]
    fn adjust_values_override_defaults() {
        let def = GeometryDef {
            text_rect: None,
            adjust: vec![guide("adj", "val 25000")],
            guides: vec![guide("x1", "*/ w adj 100000")],
            paths: vec![GeometryPath {
                width: 0,
                height: 0,
                fill: PathFill::Normal,
                stroke: true,
                ops: vec![PathOp::MoveTo(s("l"), s("t")), PathOp::LineTo(s("x1"), s("t")), PathOp::LineTo(s("x1"), s("b")), PathOp::Close],
            }],
        };
        let geometry = FormulaGeometry { preset: None, adjust: vec![guide("adj", "val 50000")], def: Arc::new(def) };
        let outline = geometry.outline(200.0, 100.0);
        assert_eq!(outline[0].subpaths[0].points, vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]);
        assert!(outline[0].subpaths[0].closed);
    }

    #[test]
    fn quarter_arc_ends_where_expected() {
        // From the top of a circle of radius 10 centred at (10, 10), sweep 90° clockwise.
        let points = arc_points((10.0, 0.0), 10.0, 10.0, 16_200_000.0, 5_400_000.0);
        let end = *points.last().unwrap();
        assert!((end.0 - 20.0).abs() < 1e-6 && (end.1 - 10.0).abs() < 1e-6, "{end:?}");
    }

    #[test]
    fn half_arc_on_an_ellipse() {
        // From the left end of a 20 × 10 ellipse centred at (20, 10), sweep 180°.
        let points = arc_points((0.0, 10.0), 20.0, 10.0, 10_800_000.0, 10_800_000.0);
        let end = *points.last().unwrap();
        assert!((end.0 - 40.0).abs() < 1e-6 && (end.1 - 10.0).abs() < 1e-6, "{end:?}");
        // It passes through the top of the ellipse.
        assert!(points.iter().any(|p| (p.1 - 0.0).abs() < 0.5));
    }
}
