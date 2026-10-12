// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The menu bar and the menu it opens (2026-10-07, #12 and #15).
//!
//! The bar keeps which menu is open itself (not a `DropdownMenu` button per
//! menu), so that moving the pointer onto another menu's name while one is
//! open closes it and opens the other, as the macOS menu bar does. A menu
//! that does not fit the window shows a window of its lines between two
//! fixed rows ▲ and ▼: hovering a row scrolls, the wheel scrolls, and a
//! bar at the right edge (shown while the pointer is there) scrolls by
//! pressing or dragging.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::{Icon, Selectable as _, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::base::ElementExt as _;
use gpui_kit::*;

use crate::shell::chrome::{APP_BG, MenuEntry, fill_menu};
use crate::shell::editor::SlideEditor;
use crate::shell::icons::Material;

/// A line of a menu is this tall (26 px and the 2 px gap), a separator this.
const ITEM_HEIGHT: f32 = 28.0;
const SEPARATOR_HEIGHT: f32 = 8.0;
/// The menu's padding, and the room left below it and above the window's edge.
const MENU_PADDING: f32 = 8.0;
const WINDOW_MARGIN: f32 = 12.0;
/// Bottom of the menu bar, where a menu starts.
const BAR_BOTTOM: f32 = 36.0 + 28.0;
/// How often a hovered ▲ or ▼ scrolls by one line, in milliseconds.
const SCROLL_MS: u64 = 70;
const MENU_WIDTH: f32 = 280.0;

fn height_of(entry: &MenuEntry) -> f32 {
    match entry {
        MenuEntry::Separator => SEPARATOR_HEIGHT,
        _ => ITEM_HEIGHT,
    }
}

/// How many lines, starting at `start`, fit in `budget` pixels.
fn lines_from(entries: &[MenuEntry], start: usize, budget: f32) -> usize {
    let mut used = 0.0;
    let mut n = 0;
    for entry in &entries[start.min(entries.len())..] {
        used += height_of(entry);
        if used > budget {
            break;
        }
        n += 1;
    }
    n
}

/// The window of a menu: the lines shown, `start..end`, and whether it is cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Visible {
    pub start: usize,
    pub end: usize,
    pub cut: bool,
}

/// Which lines of `entries` show, from `start` (moved into the range that is
/// possible), when the menu may be `height` pixels tall.
pub(crate) fn visible(entries: &[MenuEntry], start: usize, height: f32) -> Visible {
    let all: f32 = entries.iter().map(height_of).sum::<f32>() + MENU_PADDING;
    if all <= height {
        return Visible { start: 0, end: entries.len(), cut: false };
    }
    // Two fixed rows (▲ ▼) take the room of two lines.
    let budget = height - MENU_PADDING - 2.0 * ITEM_HEIGHT;
    // The last start that still fills the window to the end.
    let mut max_start = entries.len();
    let mut used = 0.0;
    while max_start > 0 && used + height_of(&entries[max_start - 1]) <= budget {
        max_start -= 1;
        used += height_of(&entries[max_start]);
    }
    let start = start.min(max_start);
    let end = start + lines_from(entries, start, budget);
    Visible { start, end, cut: true }
}

/// The menu that is open.
pub(crate) struct OpenMenu {
    ix: usize,
    entries: Rc<Vec<MenuEntry>>,
    start: usize,
    bars: Bars,
    popup: Entity<PopupMenu>,
    /// Scrolls while ▲ or ▼ is hovered.
    scroll_task: Option<Task<()>>,
    /// The scroll bar is being dragged.
    dragging: bool,
    /// Where the scroll bar was drawn.
    bar: Rc<std::cell::Cell<Bounds<Pixels>>>,
    _dismiss: Subscription,
}

impl OpenMenu {
    pub(crate) fn index(&self) -> usize {
        self.ix
    }
    /// The first line shown.
    pub(crate) fn start(&self) -> usize {
        self.start
    }

    /// The pointer is on the ▲ (`dir` < 0) or ▼ row. The row moves by up to a
    /// line as the lines shown change (their heights differ), so the room a
    /// line's height inward counts too.
    fn on_bar(&self, dir: i32, at: Point<Pixels>) -> bool {
        let mut bounds = self.bars.borrow()[usize::from(dir > 0)];
        let slack = px(ITEM_HEIGHT);
        if dir > 0 {
            bounds.origin.y -= slack;
        }
        bounds.size.height += slack;
        bounds.contains(&at)
    }
}

/// Where the ▲ and ▼ rows were drawn last.
type Bars = Rc<std::cell::RefCell<[Bounds<Pixels>; 2]>>;

/// The ▲ or ▼ row: fixed, and scrolling while the pointer is on it.
fn scroll_row(dir: i32, editor: &WeakEntity<SlideEditor>, bars: &Bars) -> PopupMenuItem {
    let (editor, bars) = (editor.clone(), bars.clone());
    PopupMenuItem::element(move |_, _| {
        let (editor, bars) = (editor.clone(), bars.clone());
        div()
            .id(if dir < 0 { "menu-scroll-up" } else { "menu-scroll-down" })
            .w_full()
            .h(px(ITEM_HEIGHT - 2.))
            .flex()
            .items_center()
            .justify_center()
            .child(Icon::new(Material(if dir < 0 { "keyboard_arrow_up" } else { "keyboard_arrow_down" })).size(px(18.)).text_color(rgb(0x5f6368)))
            .on_hover(move |hovered, window, cx| {
                editor.update(cx, |this, cx| this.menu_hover_scroll(dir, *hovered, window, cx)).ok();
            })
            // Where the row is drawn, for the scrolling to know the pointer is still on it.
            .on_prepaint(move |bounds, _, _| bars.borrow_mut()[usize::from(dir > 0)] = bounds)
    })
    .disabled(true)
}

fn fill_window(menu: PopupMenu, open: &OpenMenuParts, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let OpenMenuParts { entries, view, editor, bars } = open;
    let mut menu = menu;
    if view.cut && view.start > 0 {
        menu = menu.item(scroll_row(-1, editor, bars));
    }
    menu = fill_menu(menu, &entries[view.start..view.end], editor, window, cx);
    if view.cut && view.end < entries.len() {
        menu = menu.item(scroll_row(1, editor, bars));
    }
    menu
}

struct OpenMenuParts {
    entries: Rc<Vec<MenuEntry>>,
    view: Visible,
    editor: WeakEntity<SlideEditor>,
    bars: Bars,
}

impl SlideEditor {
    /// How tall a menu may be in this window.
    fn menu_room(window: &Window) -> f32 {
        f32::from(window.viewport_size().height) - BAR_BOTTOM - WINDOW_MARGIN
    }

    /// Open menu `ix` (closing the one that is open).
    pub(crate) fn open_menu_at(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let menus = self.menus();
        let Some((_, entries)) = menus.menus.into_iter().nth(ix) else {
            return;
        };
        let entries = Rc::new(entries);
        let editor = cx.entity().downgrade();
        let view = visible(&entries, 0, Self::menu_room(window));
        let bars: Bars = Default::default();
        let parts = OpenMenuParts { entries: entries.clone(), view, editor, bars: bars.clone() };
        let popup = PopupMenu::build(window, cx, move |menu, window, cx| fill_window(menu.min_w(px(MENU_WIDTH)), &parts, window, cx));
        popup.focus_handle(cx).focus(window, cx);
        let dismiss = cx.subscribe_in(&popup, window, |this, _, _: &DismissEvent, window, cx| {
            this.open_menu = None;
            this.menu_dismissed_at = Some(window.mouse_position());
            this.focus_handle.focus(window, cx);
            cx.notify();
        });
        self.open_menu = Some(OpenMenu { ix, entries, start: 0, bars, popup, scroll_task: None, dragging: false, bar: Default::default(), _dismiss: dismiss });
        cx.notify();
    }

    pub(crate) fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.open_menu = None;
        cx.notify();
    }

    /// Show the menu's lines from `start` (moved into the possible range).
    pub(crate) fn scroll_menu_to(&mut self, start: usize, window: &mut Window, cx: &mut Context<Self>) {
        let room = Self::menu_room(window);
        let editor = cx.entity().downgrade();
        let Some(open) = self.open_menu.as_mut() else {
            return;
        };
        let view = visible(&open.entries, start, room);
        if view.start == open.start {
            return;
        }
        open.start = view.start;
        let parts = OpenMenuParts { entries: open.entries.clone(), view, editor, bars: open.bars.clone() };
        open.popup.update(cx, |popup, cx| popup.rebuild(window, cx, move |menu, window, cx| fill_window(menu, &parts, window, cx)));
        cx.notify();
    }

    fn scroll_menu_by(&mut self, lines: i32, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(open) = &self.open_menu {
            let start = (open.start as i64 + lines as i64).max(0) as usize;
            self.scroll_menu_to(start, window, cx);
        }
    }

    /// The pointer entered (or left) the ▲ (`dir` -1) or ▼ (1) row.
    pub(crate) fn menu_hover_scroll(&mut self, dir: i32, hovered: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.open_menu.as_mut() else {
            return;
        };
        if !hovered {
            open.scroll_task = None;
            return;
        }
        if open.scroll_task.is_some() {
            return;
        }
        self.scroll_menu_by(dir, window, cx);
        let task = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(std::time::Duration::from_millis(SCROLL_MS)).await;
                let go = this
                    .update_in(cx, |this, window, cx| {
                        // Still on the row?
                        let on_row = this.open_menu.as_ref().is_some_and(|o| o.on_bar(dir, window.mouse_position()));
                        if !on_row {
                            return false;
                        }
                        let before = this.open_menu.as_ref().map(|o| o.start);
                        this.scroll_menu_by(dir, window, cx);
                        before.is_some() && before != this.open_menu.as_ref().map(|o| o.start)
                    })
                    .unwrap_or(false);
                if !go {
                    // Finished: a hover on a row starts it again.
                    this.update(cx, |this, _| {
                        if let Some(open) = this.open_menu.as_mut() {
                            open.scroll_task = None;
                        }
                    })
                    .ok();
                    break;
                }
            }
        });
        if let Some(open) = self.open_menu.as_mut() {
            open.scroll_task = Some(task);
        }
    }

    /// The scroll bar at the menu's right edge: shows where the window is
    /// in the menu, pressing or dragging moves it.
    fn render_menu_scrollbar(&self, view: Visible, total: usize, cx: &mut Context<Self>) -> impl IntoElement {
        // The track is as tall as the menu's items (the menu's height is
        // that of its lines); the thumb the part of the lines shown.
        let shown = (view.end - view.start) as f32 / total as f32;
        let top = view.start as f32 / total as f32;
        let at = move |this: &mut Self, y: Pixels, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut Context<Self>| {
            let fraction = ((f32::from(y - bounds.origin.y)) / f32::from(bounds.size.height)).clamp(0.0, 1.0);
            // The middle of the thumb follows the pointer.
            let start = ((fraction - shown / 2.0) * total as f32).round().max(0.0) as usize;
            this.scroll_menu_to(start, window, cx);
        };
        let cell = self.open_menu.as_ref().map(|o| o.bar.clone()).unwrap_or_default();
        let (down, drag) = (cell.clone(), cell.clone());
        div()
            .id("menu-scrollbar")
            .group("menu-scrollbar")
            .absolute()
            .top(px(MENU_PADDING / 2.0 + ITEM_HEIGHT))
            .bottom(px(MENU_PADDING / 2.0 + ITEM_HEIGHT))
            .right_0()
            .w(px(12.))
            .on_prepaint(move |bounds, _, _| cell.set(bounds))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                if let Some(open) = this.open_menu.as_mut() {
                    open.dragging = true;
                }
                at(this, event.position.y, down.get(), window, cx);
                cx.stop_propagation();
            }))
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                if event.dragging() && this.open_menu.as_ref().is_some_and(|o| o.dragging) {
                    at(this, event.position.y, drag.get(), window, cx);
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| {
                if let Some(open) = this.open_menu.as_mut() {
                    open.dragging = false;
                }
            }))
            .child(
                div()
                    .absolute()
                    .right(px(2.))
                    .w(px(7.))
                    .top(relative(top))
                    .h(relative(shown))
                    .rounded(px(4.))
                    .bg(hsla(0.0, 0.0, 0.0, 0.0))
                    .group_hover("menu-scrollbar", |s| s.bg(hsla(0.0, 0.0, 0.0, 0.38))),
            )
    }

    /// The menu bar.
    pub(crate) fn render_menu_bar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let menus = self.menus();
        let open_ix = self.open_menu.as_ref().map(|o| o.ix);
        let room = Self::menu_room(window);
        h_flex()
            .h(px(28.))
            .flex_shrink_0()
            .pl(px(45.))
            .items_center()
            .bg(rgb(APP_BG))
            .children(menus.menus.into_iter().enumerate().map(|(ix, (id, _))| {
                let selected = open_ix == Some(ix);
                div()
                    .id(SharedString::from(format!("menu-{}", id.key())))
                    .relative()
                    .child(
                        Button::new(SharedString::from(format!("menu-button-{}", id.key())))
                            .ghost()
                            .small()
                            .label(id.label())
                            .h(px(24.))
                            .px(px(8.))
                            .text_size(px(14.))
                            .rounded(px(4.))
                            .selected(selected)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                    // Stop propagation to avoid dragging the window.
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    // The press that closed the menu (a press outside it
                                    // closes it before this runs) does not open it again.
                                    let pressed_to_close = this.menu_dismissed_at.take().is_some_and(|p| p == event.position);
                                    if selected || pressed_to_close {
                                        this.close_menu(cx);
                                    } else {
                                        this.open_menu_at(ix, window, cx);
                                    }
                                }),
                            ),
                    )
                    // While a menu is open, the pointer on another menu's name
                    // opens that one.
                    .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                        if *hovered && this.open_menu.as_ref().is_some_and(|o| o.ix != ix) {
                            this.open_menu_at(ix, window, cx);
                        }
                    }))
                    .when(selected, |d| d.child(self.render_open_menu(room, cx)))
            }))
    }

    fn render_open_menu(&self, room: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.open_menu.as_ref().expect("a menu is open");
        let total = open.entries.len();
        let view = visible(&open.entries, open.start, room);
        deferred(
            anchored().anchor(Anchor::TopLeft).snap_to_window_with_margin(px(8.)).child(
                div()
                    .occlude()
                    .top_1()
                    .relative()
                    .child(open.popup.clone())
                    // Over the menu (an element with a handler that does not occlude,
                    // as the menu itself occludes what is behind it): the wheel.
                    .child(div().absolute().top_0().left_0().size_full().on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                        let dy = f32::from(event.delta.pixel_delta(px(ITEM_HEIGHT)).y);
                        // The content moves with the fingers: scrolling down shows later lines.
                        let lines = (-dy / ITEM_HEIGHT).round() as i32;
                        let lines = if lines == 0 && dy != 0.0 { if dy < 0.0 { 1 } else { -1 } } else { lines };
                        if lines != 0 {
                            this.scroll_menu_by(lines, window, cx);
                        }
                        cx.stop_propagation();
                    })))
                    .when(view.cut, |d| d.child(self.render_menu_scrollbar(view, total, cx))),
            ),
        )
    }
}
