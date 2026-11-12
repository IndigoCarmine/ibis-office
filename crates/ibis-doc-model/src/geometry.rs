// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Slide coordinates.
//!
//! All positions and sizes are stored in EMU (English Metric Units), the unit
//! OOXML uses: 914 400 EMU per inch, 12 700 EMU per point. Keeping the model in
//! EMU means a PPTX round-trip never loses precision to screen pixels.

/// English Metric Unit.
pub type Emu = i64;

pub const EMU_PER_INCH: Emu = 914_400;
pub const EMU_PER_POINT: Emu = 12_700;

/// A point on a slide, in EMU.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EmuPoint {
    pub x: Emu,
    pub y: Emu,
}

impl EmuPoint {
    pub const fn new(x: Emu, y: Emu) -> Self {
        Self { x, y }
    }
}

/// A size, in EMU.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EmuSize {
    pub width: Emu,
    pub height: Emu,
}

impl EmuSize {
    pub const fn new(width: Emu, height: Emu) -> Self {
        Self { width, height }
    }

    /// PowerPoint's default widescreen slide, 13.333 in × 7.5 in (16:9).
    pub const WIDESCREEN: Self = Self::new(12_192_000, 6_858_000);
}

/// An axis-aligned rectangle, in EMU. Width and height are never negative.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EmuRect {
    pub x: Emu,
    pub y: Emu,
    pub width: Emu,
    pub height: Emu,
}

impl EmuRect {
    pub const fn new(x: Emu, y: Emu, width: Emu, height: Emu) -> Self {
        Self { x, y, width, height }
    }

    /// The rectangle spanned by two corners, in any order.
    pub fn from_corners(a: EmuPoint, b: EmuPoint) -> Self {
        Self {
            x: a.x.min(b.x),
            y: a.y.min(b.y),
            width: (a.x - b.x).abs(),
            height: (a.y - b.y).abs(),
        }
    }

    pub const fn right(&self) -> Emu {
        self.x + self.width
    }

    pub const fn bottom(&self) -> Emu {
        self.y + self.height
    }

    pub fn contains(&self, p: EmuPoint) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }

    pub fn translated(&self, dx: Emu, dy: Emu) -> Self {
        Self { x: self.x + dx, y: self.y + dy, ..*self }
    }

    /// Resize by dragging `handle` by (`dx`, `dy`).
    ///
    /// The opposite edge stays fixed. Each side is kept at least `min` long, so
    /// dragging a handle past the opposite edge stops there instead of
    /// flipping the shape (which PowerPoint does for pictures, not text boxes).
    pub fn resized(&self, handle: ResizeHandle, dx: Emu, dy: Emu, min: Emu) -> Self {
        let (mut left, mut top, mut right, mut bottom) = (self.x, self.y, self.right(), self.bottom());
        if handle.moves_left() {
            left = (left + dx).min(right - min);
        }
        if handle.moves_right() {
            right = (right + dx).max(left + min);
        }
        if handle.moves_top() {
            top = (top + dy).min(bottom - min);
        }
        if handle.moves_bottom() {
            bottom = (bottom + dy).max(top + min);
        }
        Self::new(left, top, right - left, bottom - top)
    }

    /// Resize by dragging `handle` by (`dx`, `dy`), preserving the original aspect ratio when
    /// `keep_aspect_on_corners` is true and `handle` is one of the four corners.
    pub fn resized_with_aspect(&self, handle: ResizeHandle, mut dx: Emu, mut dy: Emu, min: Emu, keep_aspect_on_corners: bool) -> Self {
        if keep_aspect_on_corners && handle.is_corner() && self.width > 0 && self.height > 0 {
            let sx = if handle.moves_left() { -1.0 } else { 1.0 };
            let sy = if handle.moves_top() { -1.0 } else { 1.0 };
            let w0 = self.width as f64;
            let h0 = self.height as f64;
            let raw_w = (w0 + sx * dx as f64).max(min as f64);
            let raw_h = (h0 + sy * dy as f64).max(min as f64);
            let min_k = (min as f64 / w0).max(min as f64 / h0);
            let k = if (raw_w / w0 - 1.0).abs() >= (raw_h / h0 - 1.0).abs() {
                (raw_w / w0).max(min_k)
            } else {
                (raw_h / h0).max(min_k)
            };
            dx = (sx * (w0 * k - w0)).round() as Emu;
            dy = (sy * (h0 * k - h0)).round() as Emu;
        }
        self.resized(handle, dx, dy, min)
    }
}

/// The eight resize handles around a selected shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResizeHandle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

impl ResizeHandle {
    pub const ALL: [Self; 8] = [
        Self::TopLeft,
        Self::Top,
        Self::TopRight,
        Self::Right,
        Self::BottomRight,
        Self::Bottom,
        Self::BottomLeft,
        Self::Left,
    ];

    pub fn is_corner(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopRight | Self::BottomRight | Self::BottomLeft)
    }

    pub fn moves_left(self) -> bool {
        matches!(self, Self::TopLeft | Self::Left | Self::BottomLeft)
    }

    pub fn moves_right(self) -> bool {
        matches!(self, Self::TopRight | Self::Right | Self::BottomRight)
    }

    pub fn moves_top(self) -> bool {
        matches!(self, Self::TopLeft | Self::Top | Self::TopRight)
    }

    pub fn moves_bottom(self) -> bool {
        matches!(self, Self::BottomLeft | Self::Bottom | Self::BottomRight)
    }

    /// Position of the handle on the unit square, (0,0) top-left to (1,1)
    /// bottom-right, as halves: 0, 1 or 2.
    pub fn anchor(self) -> (u8, u8) {
        match self {
            Self::TopLeft => (0, 0),
            Self::Top => (1, 0),
            Self::TopRight => (2, 0),
            Self::Right => (2, 1),
            Self::BottomRight => (2, 2),
            Self::Bottom => (1, 2),
            Self::BottomLeft => (0, 2),
            Self::Left => (0, 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_corners_normalizes() {
        let r = EmuRect::from_corners(EmuPoint::new(10, 50), EmuPoint::new(2, 20));
        assert_eq!(r, EmuRect::new(2, 20, 8, 30));
    }

    #[test]
    fn resize_keeps_opposite_edge() {
        let r = EmuRect::new(100, 100, 200, 100);
        let out = r.resized(ResizeHandle::TopLeft, -50, 20, 10);
        assert_eq!(out, EmuRect::new(50, 120, 250, 80));
        assert_eq!(out.right(), r.right());
        assert_eq!(out.bottom(), r.bottom());
    }

    #[test]
    fn resize_clamps_to_minimum() {
        let r = EmuRect::new(0, 0, 100, 100);
        let out = r.resized(ResizeHandle::Right, -500, 0, 10);
        assert_eq!(out, EmuRect::new(0, 0, 10, 100));
        let out = r.resized(ResizeHandle::Top, 0, 500, 10);
        assert_eq!(out, EmuRect::new(0, 90, 100, 10));
    }

    #[test]
    fn edge_handles_move_one_axis() {
        let r = EmuRect::new(0, 0, 100, 100);
        assert_eq!(r.resized(ResizeHandle::Bottom, 40, 40, 1), EmuRect::new(0, 0, 100, 140));
        assert_eq!(r.resized(ResizeHandle::Left, 40, 40, 1), EmuRect::new(40, 0, 60, 100));
    }
}
