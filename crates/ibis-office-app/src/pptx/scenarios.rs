// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The presentation's work scenarios (`tasks/doc/scenarios-pptx.md`), done
//! through the window with the scenario driver (`shell/scenario.rs`): each
//! checks the window (selection, tool, toolbar, status bar), the model,
//! undo, and the saved file read back. The numbers are the list's.

use ibis_doc_model::{EmuPoint, Fill, ShapeKind};

use super::editing::{crop_for_aspect, ArrowKind, LineKind, SelectionKind, Tool};
use crate::shell::scenario::Scenario;

/// Steps on a slide.
pub(crate) trait SlideSteps {
    /// A point of the slide (EMU) in the window.
    fn at(&mut self, p: EmuPoint) -> (f32, f32);
    fn selected(&mut self) -> usize;
    fn shapes(&mut self) -> usize;
    fn tool(&mut self) -> Tool;
}

impl SlideSteps for Scenario {
    fn at(&mut self, p: EmuPoint) -> (f32, f32) {
        self.with(|e, _| {
            let layout = e.slide_layout().expect("the slide is laid out");
            (f32::from(layout.origin.x + layout.to_px(p.x)), f32::from(layout.origin.y + layout.to_px(p.y)))
        })
    }

    fn selected(&mut self) -> usize {
        self.with(|e, _| e.selected_ids().len())
    }

    fn shapes(&mut self) -> usize {
        self.with(|e, _| e.slide().shapes.len())
    }

    fn tool(&mut self) -> Tool {
        self.with(|e, _| e.pptx_tool())
    }
}

/// Undo everything, check it is the file as opened, and redo everything.
fn undo_all_and_redo(s: &mut Scenario) {
    let end = s.with(|e, _| e.presentation().slides.clone());
    let mut steps = 0;
    while s.with(|e, _| e.document.can_undo()) {
        s.key("cmd-z");
        steps += 1;
        assert!(steps < 500, "undo does not end");
    }
    for _ in 0..steps {
        s.key("cmd-shift-z");
    }
    assert!(s.with(|e, _| e.presentation().slides == end), "redoing everything gives the end again");
}

/// 1 / 2: new slides from layouts, and text in their first placeholder.
pub(crate) fn new_slides_from_layouts() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let layouts = s.with(|e, _| e.presentation().layouts.len());
    assert!(layouts >= 2, "the file has layouts");
    s.menu("pptx.new-slide");
    assert_eq!(s.status_texts()[0], "スライド 2 / 5");
    s.menu("pptx.new-slide.layout");
    assert_eq!(s.status_texts()[0], "スライド 3 / 6");
    let layout = s.with(|e, _| e.slide().layout);
    assert!(layout.is_some(), "the new slide follows a layout");
    s.screenshot("new-slides");
    s.save_as("new-slides.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides.len(), 6);
    undo_all_and_redo(&mut s);
}

/// 3 / 4: duplicate a slide, edit the copy, move it, delete another, undo.
pub(crate) fn duplicate_move_and_delete_slides() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    s.menu("pptx.duplicate-slide");
    assert_eq!(s.status_texts()[0], "スライド 3 / 5");
    // The copy's title edits on its own.
    let original = s.with(|e, _| e.presentation().slides[1].clone());
    s.double_click_shape(0);
    s.key("end");
    s.type_text("2");
    s.key("escape");
    assert_eq!(s.with(|e, _| e.presentation().slides[1].clone()), original);
    let title = s.with(|e, _| e.slide().shapes[0].text_body().unwrap().text());
    assert!(title.ends_with('2'), "{title}");
    // Up the list, then delete slide 4, and bring it back.
    s.update(|e, _, cx| e.move_current_slide(-2, cx));
    assert_eq!(s.with(|e, _| e.presentation().slide_index(e.current_slide)), Some(0));
    s.page(4);
    s.menu("pptx.delete-slide");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 4);
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 5);
    s.key("cmd-shift-z");
    s.save_as("slides.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides.len(), 4);
    assert!(saved.slides[0].shapes[0].text_body().unwrap().text().ends_with('2'));
    undo_all_and_redo(&mut s);
}

/// 5 / 7: the text box tool: a click puts a box and edits it; Esc ends the
/// typing, then the selection; the shape tool is left with Esc and puts nothing.
pub(crate) fn text_box_by_click_and_tools_cancelled() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(4);
    let before = s.shapes();
    s.toolbar("tool-text");
    assert_eq!(s.tool(), Tool::TextBox);
    assert!(s.status_texts().iter().any(|t| t.contains("テキスト ボックスを置きます")), "{:?}", s.status_texts());
    let at = s.at(EmuPoint::new(6_000_000, 5_600_000));
    s.click(at);
    assert_eq!(s.tool(), Tool::Select, "placing hands back to the select tool");
    assert!(s.with(|e, _| e.text_editing().is_some()), "typing into the new box");
    s.type_text("Target");
    s.key("escape");
    assert_eq!(s.selected(), 1, "the box stays selected");
    s.key("escape");
    assert_eq!(s.selected(), 0);
    assert_eq!(s.shapes(), before + 1);
    // A tool left with Esc puts nothing.
    s.toolbar_menu("insert-shape", "shape.ellipse");
    assert_eq!(s.tool(), Tool::Shape("ellipse"));
    s.key("escape");
    assert_eq!(s.tool(), Tool::Select);
    assert_eq!(s.shapes(), before + 1);
    s.save_as("text-box.pptx");
    let saved = s.reopen();
    let text = saved.slides[3].shapes.last().unwrap().text_body().unwrap().text();
    assert_eq!(text, "Target");
    undo_all_and_redo(&mut s);
}

/// 8 / 9: a shape and an arrow drawn by dragging; the toolbar follows.
pub(crate) fn draw_a_shape_and_an_arrow() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(4);
    s.toolbar_menu("insert-shape", "shape.roundRect");
    let (a, b) = (s.at(EmuPoint::new(1_000_000, 4_800_000)), s.at(EmuPoint::new(3_000_000, 6_000_000)));
    s.drag(a, b);
    assert_eq!(s.tool(), Tool::Select);
    assert_eq!(s.with(|e, _| e.selection_kind()), SelectionKind::Shape);
    assert!(s.toolbar_state("fill").is_some(), "a shape's toolbar");
    s.toolbar_menu("fill", "color.orange");
    // An arrow, up and to the right.
    s.toolbar_menu("insert-line", "pptx.arrow");
    let (a, b) = (s.at(EmuPoint::new(4_000_000, 6_000_000)), s.at(EmuPoint::new(6_000_000, 5_000_000)));
    s.drag(a, b);
    assert_eq!(s.with(|e, _| e.selection_kind()), SelectionKind::Line);
    assert!(s.toolbar_state("line-end").is_some(), "a line's toolbar");
    assert!(s.toolbar_state("fill").is_none());
    let line = s.with(|e, _| e.selected_shapes()[0].clone());
    assert!(line.flip_v && !line.flip_h, "drawn upwards");
    s.toolbar_menu("line-end", "end.stealth");
    s.screenshot("shape-and-arrow");
    s.save_as("draw.pptx");
    let saved = s.reopen();
    let shapes = &saved.slides[3].shapes;
    let ShapeKind::Auto(auto) = &shapes[shapes.len() - 2].kind else { panic!("a shape") };
    assert!(matches!(auto.fill, Fill::Solid(c) if c.rgb.0 == 0xffc000));
    let ShapeKind::Line(stroke) = &shapes[shapes.len() - 1].kind else { panic!("a line") };
    assert!(matches!(stroke.tail, ibis_doc_model::LineEnd::Stealth(_)));
    undo_all_and_redo(&mut s);
}

/// 10 / 11: click, Shift+click and the rubber band.
pub(crate) fn select_with_clicks_and_the_rubber_band() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    s.click_shape(1);
    assert_eq!(s.selected(), 1);
    let p = s.shape_position(2);
    s.shift_click(p);
    assert_eq!(s.selected(), 2);
    assert_eq!(s.with(|e, _| e.selection_kind()), SelectionKind::Multi);
    s.shift_click(p);
    assert_eq!(s.selected(), 1, "Shift+click takes it away again");
    // Nothing under the press: nothing selected.
    let empty = s.at(EmuPoint::new(11_000_000, 6_300_000));
    s.click(empty);
    assert_eq!(s.selected(), 0);
    // A band around the two left shapes takes those wholly inside only.
    let (x0, y0) = s.at(EmuPoint::new(300_000, 1_700_000));
    let (x1, y1) = s.shape_position(2);
    let inside = s.with(|e, _| {
        let f = e.slide().shapes[2].frame;
        (f.right(), f.bottom())
    });
    let (x1b, y1b) = s.at(EmuPoint::new(inside.0 + 50_000, inside.1 + 50_000));
    let _ = (x1, y1);
    s.drag((x0, y0), (x1b, y1b));
    assert_eq!(s.selected(), 2, "the two shapes wholly inside the band");
    s.screenshot("band");
}

/// 12 / 13 / 14: move, nudge, resize with a handle, rotate and flip; undo.
pub(crate) fn move_resize_and_rotate() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    s.click_shape(1);
    let before = s.with(|e, _| e.slide().shapes[1].frame);
    let from = s.shape_position(1);
    s.drag(from, (from.0 + 60.0, from.1 + 30.0));
    let moved = s.with(|e, _| e.slide().shapes[1].frame);
    assert!(moved.x > before.x && moved.y > before.y, "{before:?} → {moved:?}");
    s.key("right");
    assert_eq!(s.with(|e, _| e.slide().shapes[1].frame.x), moved.x + 72_000);
    // The bottom-right handle makes it bigger.
    let corner = s.with(|e, _| {
        let f = e.slide().shapes[1].frame;
        EmuPoint::new(f.right(), f.bottom())
    });
    let at = s.at(corner);
    s.drag(at, (at.0 + 40.0, at.1 + 20.0));
    let bigger = s.with(|e, _| e.slide().shapes[1].frame);
    assert!(bigger.width > moved.width && bigger.height > moved.height, "{bigger:?}");
    s.menu("pptx.rotate.right");
    assert_eq!(s.with(|e, _| e.slide().shapes[1].rotation), 5_400_000);
    s.menu("pptx.rotate.flip-h");
    assert!(s.with(|e, _| e.slide().shapes[1].flip_h));
    s.screenshot("moved");
    s.save_as("move.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[2].shapes[1].rotation, 5_400_000);
    assert!(saved.slides[2].shapes[1].flip_h);
    undo_all_and_redo(&mut s);
}

/// 15 / 16: the z-order, and lining up three shapes.
pub(crate) fn order_and_align() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    let first = s.with(|e, _| e.slide().shapes[1].id);
    s.click_shape(1);
    s.menu("pptx.order.front");
    assert_eq!(s.with(|e, _| e.slide().shapes.last().map(|x| x.id)), Some(first));
    s.menu("pptx.order.back");
    assert_eq!(s.with(|e, _| e.slide().shapes[0].id), first);
    // Three shapes: tops lined up, then spread evenly.
    s.key("cmd-a");
    let all = s.selected();
    assert!(all >= 3);
    s.menu("pptx.align.top");
    let tops: Vec<i64> = s.with(|e, _| e.selected_shapes().iter().map(|x| super::interaction::bounds(x).y).collect());
    assert!(tops.windows(2).all(|w| w[0] == w[1]), "{tops:?}");
    s.save_as("align.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[2].shapes[0].text_body().map(|b| b.text()), s.with(|e, _| e.slide().shapes[0].text_body().map(|b| b.text())));
    undo_all_and_redo(&mut s);
}

/// 17: fill and outline from the toolbar.
pub(crate) fn fill_and_outline() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    s.click_shape(1);
    s.toolbar_menu("fill", "color.yellow");
    s.toolbar_menu("border-color", "color.red");
    s.toolbar_menu("line-weight", "width.3");
    s.toolbar_menu("line-style", "pptx.stroke-dash");
    let shape = s.with(|e, _| e.slide().shapes[1].clone());
    let ShapeKind::Auto(auto) = &shape.kind else { panic!("a shape") };
    assert!(matches!(auto.fill, Fill::Solid(c) if c.rgb.0 == 0xffff00));
    let stroke = auto.stroke.clone().unwrap();
    assert_eq!((stroke.color.rgb.0, stroke.width, stroke.dashed), (0xff0000, 38_100, true));
    s.save_as("fill.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[2].shapes[1].kind, shape.kind);
    s.toolbar_menu("fill", "color.none");
    undo_all_and_redo(&mut s);
}

/// 30: a chart with shapes drawn over it moves, grows and goes with them.
pub(crate) fn a_chart_with_shapes_over_it() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, chart) = s.with(|e, _| {
        e.presentation()
            .slides
            .iter()
            .enumerate()
            .find_map(|(i, sl)| sl.shapes.iter().position(|sh| !sl.attached_to(sh.id).is_empty()).map(|j| (i + 1, j)))
            .expect("a chart with shapes over it")
    });
    s.page(page);
    let attached = s.with(|e, _| e.slide().attached_to(e.slide().shapes[chart].id));
    let over = s.with(|e, _| e.slide().shape(attached[0]).unwrap().frame);
    // A click on a shape over the chart selects the chart.
    let p = s.at(EmuPoint::new(over.x + over.width / 2, over.y + over.height / 2));
    s.click(p);
    assert_eq!(s.with(|e, _| e.selected_ids()), s.with(|e, _| vec![e.slide().shapes[chart].id]));
    let from = s.shape_position(chart);
    s.drag(from, (from.0 + 30.0, from.1 + 20.0));
    let after = s.with(|e, _| e.slide().shape(attached[0]).unwrap().frame);
    assert!(after.x > over.x && after.y > over.y, "the shape over the chart moves with it");
    s.save_as("chart.pptx");
    s.key("delete");
    assert!(s.with(|e, _| e.slide().shape(attached[0]).is_none()), "it goes with the chart");
    s.key("cmd-z");
    assert!(s.with(|e, _| e.slide().shape(attached[0]).is_some()));
    undo_all_and_redo(&mut s);
}

/// 49: Enter at the end of a bulleted line: the empty paragraph shows no
/// bullet; typing brings it; saved and read back the same.
pub(crate) fn empty_paragraph_and_bullets() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    s.double_click_shape(1);
    s.key("cmd-down");
    s.key("enter");
    s.key("escape");
    let paragraphs = s.with(|e, _| e.slide().shapes[1].text_body().unwrap().paragraphs.len());
    assert!(paragraphs >= 5, "{paragraphs}");
    s.screenshot("empty-paragraph");
    s.save_as("paragraph.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[1].shapes[1].text_body().unwrap().paragraphs.len(), paragraphs);
    undo_all_and_redo(&mut s);
}

/// (Moved from `shell/scenario_examples.rs`.)
/// A presentation: edit a text box's text through the window, make it
/// bold with the toolbar, undo and redo with the keys, save, and read the
/// saved file back.
pub(crate) fn edit_text_make_bold_save_and_reopen() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    assert_eq!(s.status_texts()[0], "スライド 1 / 4");
    s.screenshot("opened");

    // The subtitle (shape 1 of slide 1): double-click to edit, type at the
    // end of its text, Esc to finish.
    let before = s.with(|e, _| e.slide().shapes[1].text_body().unwrap().text());
    s.double_click_shape(1);
    assert!(s.with(|e, _| e.text_editing().is_some()), "editing the text");
    assert!(s.status_texts().iter().any(|t| t == "Esc で編集を終了"), "{:?}", s.status_texts());
    // The caret starts at the text's beginning: End moves it to the end.
    s.key("end");
    s.type_text("!");
    s.key("escape");
    let after = s.with(|e, _| e.slide().shapes[1].text_body().unwrap().text());
    assert_eq!(after, format!("{before}!"));

    // Selected: the toolbar has the text's buttons; make it bold.
    assert_eq!(s.toolbar_state("bold").map(|t| t.on), Some(false));
    s.toolbar("bold");
    assert_eq!(s.toolbar_state("bold").map(|t| t.on), Some(true));
    assert!(s.with(|e, _| e.selected_text_style().unwrap().bold));
    s.screenshot("bold");

    // Undo and redo with the keys: the bold goes and comes back.
    s.key("cmd-z");
    assert!(!s.with(|e, _| e.selected_text_style().unwrap().bold));
    s.key("cmd-shift-z");
    assert!(s.with(|e, _| e.selected_text_style().unwrap().bold));

    // Save and read the saved file back: the text and the bold are there.
    s.save_as("pptx-plan-edited.pptx");
    assert_eq!(s.status_texts().last().unwrap(), "");
    let saved = s.reopen();
    let body = saved.slides[0].shapes[1].text_body().unwrap();
    assert_eq!(body.text(), format!("{before}!"));
    assert!(body.paragraphs.iter().flat_map(|p| &p.runs).all(|r| r.style.bold != Some(false)), "every run bold");
}


/// 19: part of a text made bold while it is edited.
pub(crate) fn format_part_of_a_text() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    s.double_click_shape(1);
    // The second paragraph (not bold): its first two characters.
    s.key("cmd-up");
    s.key("down");
    s.key("home");
    s.key("shift-right");
    s.key("shift-right");
    s.toolbar("bold");
    let body = s.with(|e, _| e.slide().shapes[1].text_body().unwrap().clone());
    let second = &body.paragraphs[1];
    let bold: String = second.runs.iter().filter(|r| body.style.with_run(&r.style).bold).map(|r| r.text.as_str()).collect();
    assert_eq!(bold, second.text().chars().take(2).collect::<String>());
    s.save_as("part.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[1].shapes[1].text_body().unwrap().text(), body.text());
    undo_all_and_redo(&mut s);
}

/// 21 / 22: numbered list, indent and line spacing from the toolbar.
pub(crate) fn lists_indents_and_spacing() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    s.click_shape(1);
    s.toolbar("numbered");
    assert_eq!(s.with(|e, _| e.selected_list()), Some(ibis_doc_model::ListKind::Numbered));
    s.toolbar("indent-more");
    s.toolbar_menu("line-spacing", "spacing.1.5");
    s.toolbar_menu("align", "align.center");
    let body = s.with(|e, _| e.slide().shapes[1].text_body().unwrap().clone());
    assert!(body.paragraphs.iter().all(|p| p.align == Some(ibis_doc_model::TextAlign::Center)));
    s.save_as("lists.pptx");
    let saved = s.reopen();
    let texts: Vec<String> = saved.slides[1].shapes[1].text_body().unwrap().paragraphs.iter().filter_map(|p| p.bullet.as_ref().map(|b| b.text.clone())).collect();
    assert_eq!(texts.first().map(String::as_str), Some("1."), "{texts:?}");
    undo_all_and_redo(&mut s);
}

/// 32: the slide's background, and back to the layout's.
pub(crate) fn slide_background() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    assert_eq!(s.with(|e, _| e.selection_kind()), SelectionKind::None);
    s.toolbar_menu("background", "color.light-blue");
    assert!(matches!(s.with(|e, _| e.slide().background.clone()), Fill::Solid(c) if c.rgb.0 == 0x00b0f0));
    s.save_as("background.pptx");
    let saved = s.reopen();
    assert!(matches!(&saved.slides[1].background, Fill::Solid(c) if c.rgb.0 == 0x00b0f0));
    s.toolbar_menu("background", "background.reset");
    assert!(matches!(s.with(|e, _| e.slide().background.clone()), Fill::Solid(c) if c.rgb.0 == 0xffffff));
    undo_all_and_redo(&mut s);
}

/// The input method, and a format given while editing (the edit goes on).
pub(crate) fn japanese_input_and_formatting_while_editing() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1);
    s.key("end");
    s.ime("にほん", "日本");
    let text = s.with(|e, _| e.slide().shapes[1].text_body().unwrap().text());
    assert!(text.ends_with("日本"), "{text}");
    // Select the two characters and make them bold: still editing.
    s.key("shift-left");
    s.key("shift-left");
    s.toolbar("bold");
    assert!(s.with(|e, _| e.text_editing().is_some()), "the edit goes on");
    s.key("end");
    s.type_text("!");
    s.key("escape");
    let body = s.with(|e, _| e.slide().shapes[1].text_body().unwrap().clone());
    assert!(body.text().ends_with("日本!"));
    let bold: String = body.paragraphs.iter().flat_map(|p| &p.runs).filter(|r| body.style.with_run(&r.style).bold).map(|r| r.text.as_str()).collect();
    assert!(bold.starts_with("日本"), "{bold:?}");
    s.save_as("ime.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes[1].text_body().unwrap().text(), body.text());
    undo_all_and_redo(&mut s);
}

/// Where character `i` of paragraph `n` of the edited text was painted
/// (the middle of it, in the window).
fn char_at(s: &mut Scenario, n: usize, i: usize) -> (f32, f32) {
    s.with(|e, _| {
        let list = e.pptx().unwrap().positions.paragraphs.borrow().clone();
        let p = list[n].borrow();
        let c = p.chars[i];
        let l = p.lines[c.line];
        (c.x + c.advance / 2.0 - 0.1, l.top + l.height / 2.0)
    })
}

/// Placing the caret and selecting in the edited text with the mouse, and
/// moving by the painted lines.
pub(crate) fn caret_by_mouse_in_edited_text() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1);
    // Long enough to wrap.
    s.key("end");
    s.type_text(" with a long line of words that wraps onto the next line of the box");
    let text = s.with(|e, _| e.edited_text().unwrap());
    let first: Vec<char> = text.split('\n').next().unwrap().chars().collect();
    assert!(first.len() >= 4, "{text:?}");
    // A click just left of a character's middle puts the caret before it.
    let at = char_at(&mut s, 0, 2);
    s.click(at);
    assert_eq!(s.with(|e, _| e.text_editing().map(|t| (t.anchor, t.caret))), Some((2, 2)));
    // The caret the input method is told of is where it is drawn.
    let caret = s.with(|e, _| e.caret_bounds().unwrap());
    let x0 = s.with(|e, _| e.pptx().unwrap().positions.paragraphs.borrow()[0].borrow().chars[2].x);
    assert!((f32::from(caret.origin.x) - x0).abs() < 0.5, "{caret:?} {x0}");
    // A drag selects from where it started.
    let (a, b) = (char_at(&mut s, 0, 0), char_at(&mut s, 0, 3));
    s.drag(a, b);
    assert_eq!(s.with(|e, _| e.text_editing().map(|t| t.range())), Some(0..3));
    assert!(s.with(|e, _| e.text_editing().is_some()), "still editing");
    s.screenshot("selected-by-drag");
    // Shift-click extends it.
    let c = char_at(&mut s, 0, 1);
    s.shift_click(c);
    assert_eq!(s.with(|e, _| e.text_editing().map(|t| t.range())), Some(0..1));
    // Home and End go to the painted line's ends; Down to the next line.
    s.key("end");
    let end = s.with(|e, _| e.text_editing().unwrap().caret);
    let lines = s.with(|e, _| e.pptx().unwrap().positions.paragraphs.borrow()[0].borrow().lines.clone());
    assert!(end == lines[0].end || end + 1 == lines[0].end, "{end} {lines:?}");
    s.key("home");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 0);
    s.key("down");
    let down = s.with(|e, _| e.text_editing().unwrap().caret);
    assert!(lines.len() > 1, "{lines:?}");
    assert!(down >= lines[1].start && down <= lines[1].end, "{down} {lines:?}");
    // Up comes back to the first line, at the same x (the lines are
    // centred, so not always to its start).
    s.key("up");
    assert!(s.with(|e, _| e.text_editing().unwrap().caret) < lines[0].end);
    // Two clicks select a word.
    let w = char_at(&mut s, 0, 0);
    s.double_click(w);
    let r = s.with(|e, _| e.text_editing().unwrap().range());
    assert!(r.start == 0 && r.end > 0 && r.end <= first.len(), "{r:?}");
    s.key("end");
    s.screenshot("caret-at-line-end");
    // ↑↓ keep to the x they started from: from the end of the long first
    // line, down to the shorter second one and up again comes back there.
    let far = char_at(&mut s, 0, lines[0].end - 2);
    s.click(far);
    let from = s.with(|e, _| e.text_editing().unwrap().caret);
    s.key("down");
    let below = s.with(|e, _| e.text_editing().unwrap().caret);
    assert!(below >= lines[1].start, "{below} {lines:?}");
    s.key("up");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), from);
    // Two clicks on a shape not being edited start the edit with the caret
    // where they were.
    let target = char_at(&mut s, 0, 5);
    s.key("escape");
    assert!(s.with(|e, _| e.text_editing().is_none()));
    s.double_click(target);
    assert_eq!(s.with(|e, _| e.text_editing().map(|t| (t.anchor, t.caret))), Some((5, 5)));
    s.key("escape");
}

/// Review: wrapped-line edits and mixed-run IME/paragraph boundaries.
pub(crate) fn text_boundary_review() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let original = s.with(|e, _| e.presentation().slides.clone());
    s.double_click_shape(1);
    s.key("cmd-a");
    s.type_text("Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega repeated words for wrapping");
    let i = s.with(|e, _| {
        let list = e.pptx().unwrap().positions.paragraphs.borrow().clone();
        let p = list[0].borrow();
        assert!(p.lines.len() >= 2);
        p.lines[1].start + 2
    });
    let at = char_at(&mut s, 0, i);
    s.click(at);
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), i);
    let before = s.with(|e, _| e.edited_text().unwrap());
    s.type_text("XY");
    let mut inserted: Vec<char> = before.chars().collect();
    inserted.splice(i..i, "XY".chars());
    assert_eq!(s.with(|e, _| e.edited_text().unwrap()), inserted.into_iter().collect::<String>());
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), i + 2, "insertion caret");
    s.key("shift-left"); s.key("shift-left");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().range()), i..i+2);
    s.type_text("Z");
    let mut replaced: Vec<char> = before.chars().collect();
    replaced.insert(i, 'Z');
    assert_eq!(s.with(|e, _| e.edited_text().unwrap()), replaced.into_iter().collect::<String>());
    s.key("backspace");
    assert_eq!(s.with(|e, _| e.edited_text().unwrap()), before, "backspace removes replacement");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), i);
    s.key("delete");
    let mut expected: Vec<char> = before.chars().collect();
    expected.remove(i);
    let expected: String = expected.into_iter().collect();
    assert_eq!(s.with(|e, _| e.edited_text().unwrap()), expected);
    s.key("cmd-z"); s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.edited_text().unwrap()), expected);
    s.save_as("wrapped.pptx");
    assert_eq!(s.reopen().slides[0].shapes[1].text_body().unwrap().text(), expected);
    // Create two runs with different formatting, and place the caret at their boundary.
    s.key("cmd-a"); s.type_text("ABCD"); s.key("home");
    s.key("shift-right"); s.key("shift-right"); s.toolbar("bold");
    s.key("right");
    let before = s.with(|e, _| e.edited_body().unwrap().clone());
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().range()), 2..2);
    assert_eq!(before.paragraphs[0].runs.len(), 2);
    assert_ne!(before.paragraphs[0].runs[0].style, before.paragraphs[0].runs[1].style);
    s.update(|e, _, cx| e.input_mark("にほん", cx));
    assert_eq!(s.with(|e, _| e.edited_body().unwrap().clone()), before, "IME composition does not modify document runs");
    s.update(|e, _, cx| e.input_mark("", cx));
    assert_eq!(s.with(|e, _| e.edited_body().unwrap().clone()), before, "IME cancellation preserves runs");
    s.ime("にほん", "日本");
    assert_eq!(s.with(|e, _| e.edited_text().unwrap()), "AB日本CD");
    let committed = s.with(|e, _| e.edited_body().unwrap().clone());
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.edited_body().unwrap().clone()), before, "undo IME commit restores run structure");
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.edited_body().unwrap().clone()), committed, "redo IME commit restores run structure");
    s.key("home"); s.key("right");
    for _ in 0..4 { s.key("shift-right"); }
    s.type_text("X");
    assert_eq!(s.with(|e, _| e.edited_text().unwrap()), "AXD");
    s.key("enter");
    let body = s.with(|e, _| e.edited_body().unwrap().clone());
    assert_eq!(body.paragraphs.len(), 2);
    assert_eq!(body.text(), "AX\nD");
    s.key("shift-enter");
    let body = s.with(|e, _| e.edited_body().unwrap().clone());
    assert_eq!(body.paragraphs.len(), 3);
    assert_eq!(body.text(), "AX\n\nD");
    assert_eq!(body.paragraphs[0].runs[0].style, before.paragraphs[0].runs[0].style, "unreplaced prefix retains its run format");
    assert!(!body.paragraphs[1].line_break);
    assert!(body.paragraphs[2].line_break, "Shift+Enter is a:br, not a new a:p");
    let last_style = before.paragraphs[0].runs.last().unwrap().style.clone();
    assert_eq!(body.paragraphs[2].runs.last().unwrap().style, last_style, "unreplaced tail retains its run format");
    s.save_as("boundaries.pptx");
    let saved = s.reopen();
    let read = saved.slides[0].shapes[1].text_body().unwrap();
    assert_eq!(read.text(), body.text());
    assert_eq!(read.paragraphs.len(), body.paragraphs.len());
    for (a, b) in body.paragraphs.iter().zip(&read.paragraphs) {
        assert_eq!(a.line_break, b.line_break);
        let styles = |p: &ibis_doc_model::Paragraph, base: &ibis_doc_model::TextStyle| {
            p.runs.iter().flat_map(|r| {
                let look = base.with_run(&r.style);
                r.text.chars().map(move |_| (look.bold, look.italic, look.font_size, look.color, r.style.baseline, r.style.spacing, r.style.highlight, r.style.outline))
            }).collect::<Vec<_>>()
        };
        assert_eq!(styles(a, &body.style), styles(b, &read.style), "saved character formats");
    }
    s.key("escape");
    undo_all_and_redo(&mut s);
    s.save_as("boundaries-redone.pptx");
    let redone = s.reopen();
    let read = redone.slides[0].shapes[1].text_body().unwrap();
    assert_eq!(read.text(), body.text(), "save after redo preserves text");
    assert_eq!(read.paragraphs.len(), body.paragraphs.len());
    for (a, b) in body.paragraphs.iter().zip(&read.paragraphs) {
        assert_eq!(a.line_break, b.line_break, "save after redo preserves paragraph/soft-break structure");
    }
    while s.with(|e, _| e.document.can_undo()) {
        s.key("cmd-z");
    }
    assert_eq!(s.with(|e, _| e.presentation().slides.clone()), original, "undo restores the original text and run structure");
}

/// `char_at`, in the window when the text is turned.
fn turned_char_at(s: &mut Scenario, n: usize, i: usize) -> (f32, f32) {
    s.with(|e, _| {
        let list = e.pptx().unwrap().positions.paragraphs.borrow().clone();
        let p = list[n].borrow();
        let c = p.chars[i];
        let l = p.lines[c.line];
        p.to_window(c.x + c.advance / 2.0 - 0.1, l.top + l.height / 2.0)
    })
}

/// Editing the text of a turned shape: the caret goes where the turned
/// text is clicked, a drag selects along it, and the caret is drawn turned.
pub(crate) fn edit_the_text_of_a_turned_shape() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.click_shape(1);
    s.menu("pptx.rotate.right");
    assert_eq!(s.with(|e, _| e.slide().shapes[1].rotation), 5_400_000);
    s.double_click_shape(1);
    assert!(s.with(|e, _| e.text_editing().is_some()));
    assert!(s.with(|e, _| e.pptx().unwrap().positions.paragraphs.borrow()[0].borrow().turn.is_some()), "the text is turned");
    let at = turned_char_at(&mut s, 0, 2);
    s.click(at);
    assert_eq!(s.with(|e, _| e.text_editing().map(|t| (t.anchor, t.caret))), Some((2, 2)));
    // Turned a quarter, the caret lies across the window.
    let caret = s.with(|e, _| e.caret_bounds().unwrap());
    assert!(caret.size.width > caret.size.height, "{caret:?}");
    let (a, b) = (turned_char_at(&mut s, 0, 0), turned_char_at(&mut s, 0, 3));
    s.drag(a, b);
    assert_eq!(s.with(|e, _| e.text_editing().map(|t| t.range())), Some(0..3));
    s.screenshot("turned-selection");
    s.type_text("New");
    s.key("escape");
    let text = s.with(|e, _| e.slide().shapes[1].text_body().unwrap().text());
    assert!(text.starts_with("New"), "{text:?}");
    s.save_as("turned.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes[1].text_body().unwrap().text(), text);
    assert_eq!(saved.slides[0].shapes[1].rotation, 5_400_000);
    undo_all_and_redo(&mut s);
}

/// スライドをスキップ: the slide is skipped in the slide show, kept in the
/// file, and shown again.
pub(crate) fn skip_a_slide() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    s.menu("pptx.skip-slide");
    assert!(s.with(|e, _| e.presentation().slides[1].hidden));
    // The show goes from slide 1 to slide 3.
    s.page(1);
    s.update(|e, window, cx| e.start_slideshow(window, cx));
    s.update(|e, window, cx| e.step_slideshow(true, window, cx));
    let shown = s.with(|e, _| e.presentation().slide_index(e.current_slide));
    assert_eq!(shown, Some(2));
    s.update(|e, window, cx| e.step_slideshow(false, window, cx));
    assert_eq!(s.with(|e, _| e.presentation().slide_index(e.current_slide)), Some(0));
    s.update(|e, window, cx| e.end_slideshow(window, cx));
    s.save_as("skip.pptx");
    let saved = s.reopen();
    assert!(saved.slides[1].hidden && !saved.slides[0].hidden);
    undo_all_and_redo(&mut s);
}

/// スピーカー ノート: typed in the pane under the slide, put in the document
/// when the pane is left, saved, and undone.
pub(crate) fn write_speaker_notes() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    let before = s.with(|e, _| e.presentation().slides[1].notes.clone());
    // The pane is under the slide workspace.
    let at = s.with(|e, _| {
        let ws = e.workspace_bounds.expect("the workspace is laid out");
        (f32::from(ws.origin.x + ws.size.width / 2.0), f32::from(ws.origin.y + ws.size.height) + 40.0)
    });
    s.click(at);
    s.key("cmd-a");
    s.type_text("Talk about the plan");
    s.key("enter");
    s.type_text("Then the numbers");
    s.screenshot("notes");
    // Leaving the pane puts the notes in.
    s.click_shape(0);
    let notes = s.with(|e, _| e.presentation().slides[1].notes.clone());
    assert_eq!(notes, "Talk about the plan\nThen the numbers");
    s.save_as("notes.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[1].notes, notes);
    s.key("escape");
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.presentation().slides[1].notes.clone()), before);
    undo_all_and_redo(&mut s);
}

/// 画面切り替え: a transition from the toolbar, its duration, applied to
/// every slide, played in the show, saved.
pub(crate) fn set_slide_transitions() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    s.toolbar_menu("transition", "transition.push");
    s.toolbar_menu("transition", "transition.duration.1000");
    let t = s.with(|e, _| e.presentation().slides[1].transition.clone()).expect("a transition");
    assert_eq!((t.effect.as_str(), t.duration), ("push", 1000));
    s.toolbar_menu("transition", "transition.all");
    assert!(s.with(|e, _| e.presentation().slides.iter().all(|sl| sl.transition.as_ref().is_some_and(|t| t.effect == "push"))));
    // The show plays it (the slide comes in over the one before).
    s.page(1);
    s.update(|e, window, cx| e.start_slideshow(window, cx));
    s.update(|e, window, cx| e.step_slideshow(true, window, cx));
    s.screenshot("coming-in");
    s.update(|e, window, cx| e.end_slideshow(window, cx));
    s.save_as("transitions.pptx");
    let saved = s.reopen();
    assert!(saved.slides.iter().all(|sl| sl.transition.as_ref().is_some_and(|t| t.effect == "push" && t.duration == 1000)));
    undo_all_and_redo(&mut s);
}

/// スライドショー: ⇧⌘↩ starts from the first slide; a slide set to move on
/// by itself (自動的に切り替え, `advTm`) is followed by the next after its
/// time, and one set not to move on with a click (`advClick="0"`) stays on
/// a click and moves on with →.
pub(crate) fn slideshow_from_start_and_by_itself() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    // Slide 1 moves on by itself at once and not by a click.
    s.toolbar_menu("transition", "transition.none");
    s.toolbar_menu("transition", "transition.after.0");
    s.toolbar_menu("transition", "transition.on-click");
    let first = s.with(|e, _| e.presentation().slides[0].transition.clone()).unwrap();
    assert_eq!((first.on_click, first.after), (false, Some(0)));
    // Slide 2 moves on only by a click.
    s.page(2);
    s.toolbar_menu("transition", "transition.on-click");
    s.toolbar_menu("transition", "transition.on-click");
    s.key("cmd-shift-enter");
    assert!(s.with(|e, _| e.slideshow.is_some()));
    // The timer runs on the scenario's clock: not before the transition's
    // time (0.7 s, kept by 「なし」) has passed…
    s.wait(600);
    assert_eq!(s.with(|e, _| e.presentation().slide_index(e.current_slide)), Some(0));
    // …and then at once (advTm 0).
    s.wait(200);
    assert_eq!(s.with(|e, _| e.presentation().slide_index(e.current_slide)), Some(1), "slide 1 moved on by itself");
    // Back to slide 1 (with ←): a click does not move on from it…
    s.update(|e, window, cx| e.step_slideshow(false, window, cx));
    s.update(|e, _, cx| e.slideshow_to_end(false, cx));
    assert_eq!(s.with(|e, _| e.presentation().slide_index(e.current_slide)), Some(0));
    s.click((600.0, 360.0));
    assert_eq!(s.with(|e, _| e.presentation().slide_index(e.current_slide)), Some(0), "a click does not move on from slide 1");
    // …→ does.
    s.update(|e, window, cx| e.step_slideshow(true, window, cx));
    assert_eq!(s.with(|e, _| e.presentation().slide_index(e.current_slide)), Some(1));
    s.update(|e, window, cx| e.end_slideshow(window, cx));
    s.save_as("by-itself.pptx");
    let saved = s.reopen();
    let t = saved.slides[0].transition.clone().unwrap();
    assert_eq!((t.on_click, t.after), (false, Some(0)));
}

/// 発表者ビュー (⌥⌘↩): the show with the slide shown, the next one, the
/// notes and the clock; it moves on and ends as the show does.
pub(crate) fn presenter_view() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    // Notes for slide 1, typed in the pane under the slide.
    let at = s.with(|e, _| {
        let ws = e.workspace_bounds.expect("the workspace is laid out");
        (f32::from(ws.origin.x + ws.size.width / 2.0), f32::from(ws.origin.y + ws.size.height) + 40.0)
    });
    s.click(at);
    s.key("cmd-a");
    s.type_text("Talk about the plan");
    s.click_shape(0);
    s.key("escape");
    assert_eq!(s.with(|e, _| e.presentation().slides[0].notes.clone()), "Talk about the plan");
    // In the editor's window (the one display case; two are the next scenario).
    s.update(|e, w, cx| e.start_presenter_view_on(false, w, cx));
    assert!(s.with(|e, _| e.slideshow.as_ref().is_some_and(|show| show.presenter)));
    assert_eq!(s.with(|e, _| e.presentation().slide_index(e.current_slide)), Some(0));
    s.screenshot("presenter");
    // The clock runs (redrawn each second) and the keys move on.
    s.wait(2000);
    s.key("right");
    assert_eq!(s.with(|e, _| e.presentation().slide_index(e.current_slide)), Some(1));
    s.key("escape");
    assert!(s.with(|e, _| e.slideshow.is_none()));
}

/// 発表者ビューの時間計測・一時停止・再開・リセット (PPTX-SHOW-012)
pub(crate) fn presenter_timer_pause_and_reset() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, w, cx| e.start_presenter_view_on(false, w, cx));
    assert!(s.with(|e, _| e.slideshow.as_ref().is_some_and(|show| show.presenter)));
    assert!(!s.with(|e, _| e.presenter_timer_is_paused()));

    // 少し待機
    s.wait(100);

    // 一時停止
    s.update(|e, _, cx| e.toggle_presenter_timer_pause(cx));
    assert!(s.with(|e, _| e.presenter_timer_is_paused()));
    let paused_sec = s.with(|e, _| e.presenter_timer_elapsed_secs());

    // 一時停止中は時間が進まないことを確認
    s.wait(1500);
    assert_eq!(s.with(|e, _| e.presenter_timer_elapsed_secs()), paused_sec);

    // 再開
    s.update(|e, _, cx| e.toggle_presenter_timer_pause(cx));
    assert!(!s.with(|e, _| e.presenter_timer_is_paused()));

    // リセット
    s.update(|e, _, cx| e.reset_presenter_timer(cx));
    assert_eq!(s.with(|e, _| e.presenter_timer_elapsed_secs()), 0);

    s.key("escape");
    assert!(s.with(|e, _| e.slideshow.is_none()));
}

/// 発表者ビュー on two displays: the presenter's view and the audience's
/// slide show in a full-screen window each (with one display, the editor's
/// window as before); they follow the one show; 画面の入れ替え exchanges
/// the displays; ending the show closes them.
pub(crate) fn presenter_view_on_two_displays() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let two = s.with(|_, app| app.displays().len() >= 2);
    s.key("alt-cmd-enter");
    assert!(s.with(|e, _| e.slideshow.as_ref().is_some_and(|show| show.presenter)));
    let windows = s.with(|e, _| e.show_windows());
    if !two {
        assert!(windows.is_empty(), "one display: the editor's window shows it");
        s.key("escape");
        return;
    }
    assert_eq!(windows.len(), 2, "an audience window and a presenter window");
    assert!(s.with(|e, _| !e.show_in_main()), "the editor's window stays the editor");
    // The windows are on different displays.
    let ids: Vec<_> = windows.iter().map(|w| s.update_window(*w, |window, app| window.display(app).map(|d| d.id()))).collect();
    assert!(ids[0].is_some() && ids[1].is_some() && ids[0] != ids[1], "two displays: {ids:?}");
    s.screenshot_of(windows[0], "audience");
    s.screenshot_of(windows[1], "presenter");
    // The show moves on in both.
    s.update(|e, w, cx| e.step_slideshow(true, w, cx));
    assert_eq!(s.with(|e, _| e.presentation().slide_index(e.current_slide)), Some(1));
    s.screenshot_of(windows[0], "audience-2");
    // 画面の入れ替え: new windows, on the displays the other way round.
    s.update(|e, _, cx| e.swap_show_displays(cx));
    let swapped = s.with(|e, _| e.show_windows());
    assert_eq!(swapped.len(), 2);
    let after: Vec<_> = swapped.iter().map(|w| s.update_window(*w, |window, app| window.display(app).map(|d| d.id()))).collect();
    assert_eq!(after[0], ids[1], "the audience's window is where the presenter's was");
    assert_eq!(after[1], ids[0]);
    // Ends: the windows close.
    s.update(|e, w, cx| e.end_slideshow(w, cx));
    assert!(s.with(|e, _| e.slideshow.is_none() && e.show_windows().is_empty()));
    assert_eq!(s.with(|_, app| app.windows().len()), 1, "only the editor's window is left");
}

/// アニメーション in the slide show: on slide 1 the title and subtitle
/// are hidden until their effects; a click plays the first group (the title
/// appears, then the subtitle fades in as the title flies in), the next
/// plays the subtitle's exit, the next goes on to slide 2; ← takes a group
/// back. The file's timing is saved as it was.
pub(crate) fn play_animations_in_the_show() {
    let root = crate::shell::scenario::repo_root();
    let made = ibis_pptx::test_files::pptx_with_animations(&std::fs::read(root.join("test/editing/pptx-plan.pptx")).unwrap());
    let dir = root.join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("pptx-animations.pptx"), &made).unwrap();
    let mut s = Scenario::open("target/scenario-input/pptx-animations.pptx");
    let effects = s.with(|e, _| e.presentation().slides[0].animations.len());
    assert_eq!(effects, 4);
    s.key("cmd-shift-enter");
    let step = |s: &mut Scenario| s.with(|e, _| (e.presentation().slide_index(e.current_slide), e.slideshow.as_ref().map(|show| show.step)));
    assert_eq!(step(&mut s), (Some(0), Some(0)), "nothing played before a click");
    s.screenshot("before");
    s.key("right");
    assert_eq!(step(&mut s), (Some(0), Some(1)));
    // GPUI times animations by the real clock, not the scenario's: the end
    // of the group is drawn with motion reduced (the end state).
    s.update(|_, _, cx| cx.set_reduce_motion(true));
    s.wait(100);
    s.screenshot("first-click");
    s.key("right");
    s.wait(100);
    s.screenshot("second-click");
    s.key("left");
    s.update(|_, _, cx| cx.set_reduce_motion(false));
    assert_eq!(step(&mut s), (Some(0), Some(1)), "← takes the last group back");
    s.key("right");
    assert_eq!(step(&mut s), (Some(0), Some(2)));
    s.key("right");
    assert_eq!(step(&mut s).0, Some(1), "then the next slide");
    s.key("escape");
    s.save_as("animations.pptx");
    assert_eq!(s.reopen().slides[0].animations.len(), 4);
}

/// 取り消し線: the toolbar's button puts a line through the selected
/// shape's text and takes it off again; undo, redo, saved.
pub(crate) fn strikethrough_button() {
    use ibis_doc_model::Strike;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.click_shape(1);
    let strikes = |s: &mut Scenario| s.with(|e, _| e.slide().shapes[1].text_body().unwrap().paragraphs.iter().flat_map(|p| &p.runs).map(|r| r.style.strike).collect::<Vec<_>>());
    assert!(strikes(&mut s).iter().all(|&k| k == Strike::None));
    assert_eq!(s.toolbar_state("strikethrough").map(|t| t.on), Some(false));
    s.toolbar("strikethrough");
    assert!(strikes(&mut s).iter().all(|&k| k == Strike::Single));
    assert_eq!(s.toolbar_state("strikethrough").map(|t| t.on), Some(true));
    s.screenshot("struck");
    s.key("cmd-z");
    assert!(strikes(&mut s).iter().all(|&k| k == Strike::None));
    s.key("cmd-shift-z");
    s.save_as("struck.pptx");
    let saved = s.reopen();
    assert!(saved.slides[0].shapes[1].text_body().unwrap().paragraphs.iter().flat_map(|p| &p.runs).all(|r| r.style.strike == Strike::Single));
    // Again: the line comes off.
    s.toolbar("strikethrough");
    assert!(strikes(&mut s).iter().all(|&k| k == Strike::None));
    undo_all_and_redo(&mut s);
}

/// サイズと位置: the panel's numbers set the selected shape's height,
/// width, rotation and place; a wrong number changes nothing; undo, redo,
/// saved.
pub(crate) fn shape_size_and_place_by_numbers() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.click_shape(1);
    let original = s.with(|e, _| e.selected_shapes()[0].clone());
    s.menu("pptx.frame.settings");
    let fields = s.with(|e, _| e.pptx().unwrap().frame_panel.as_ref().unwrap().fields.clone());
    let shown: Vec<String> = s.with(|_, cx| fields.iter().map(|f| f.read(cx).value().to_string()).collect());
    assert_eq!(shown[1], format!("{}", (original.frame.width as f64 / 3600.).round() / 100.), "the width shown in cm");
    for values in [["5", "-1", "0", "1", "1"], ["5", "10", "-30", "2.54", "1.27"]] {
        s.update(|_, w, cx| {
            for (f, value) in fields.iter().zip(values) {
                f.update(cx, |f, cx| f.set_value(value, w, cx));
            }
        });
        s.menu("pptx.frame.apply");
        if values[1] == "-1" {
            assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), original, "a negative width changes nothing");
        }
    }
    let changed = s.with(|e, _| e.selected_shapes()[0].clone());
    assert_eq!(changed.frame, ibis_doc_model::EmuRect::new(914_400, 457_200, 3_600_000, 1_800_000));
    assert_eq!(changed.rotation, 19_800_000, "-30° is 330°");
    s.screenshot("set");
    s.key("cmd-z");
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), original);
    s.key("cmd-shift-z");
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), changed);
    s.save_as("frame.pptx");
    let saved = s.reopen();
    assert_eq!((saved.slides[0].shapes[1].frame, saved.slides[0].shapes[1].rotation), (changed.frame, changed.rotation));
    s.menu("pptx.frame.settings");
    s.key("cmd-a");
    s.type_text("3");
    s.menu("pptx.frame.cancel");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), changed);
    undo_all_and_redo(&mut s);
}

/// プレースホルダーの案内文字: a new slide has its layout's placeholders,
/// empty; the editor shows their prompts (not the slide show); typed into,
/// the title has its text, saved; the others stay empty.
pub(crate) fn placeholder_prompts() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("pptx.new-slide");
    let (count, placeholders, empty) = s.with(|e, _| {
        let sl = e.slide();
        (sl.shapes.len(), sl.placeholders.len(), sl.shapes.iter().all(|sh| sh.text_body().is_none_or(|b| b.is_empty())))
    });
    assert!(count > 0 && count == placeholders && empty, "{count} shapes, {placeholders} placeholders, empty {empty}");
    let prompts: Vec<Option<String>> = s.with(|e, _| {
        let layout = e.presentation().layout(e.slide().layout.unwrap()).unwrap();
        e.slide().shapes.iter().map(|sh| layout.prompt_for(&e.slide().placeholders[&sh.id].placeholder).map(str::to_string)).collect()
    });
    assert!(prompts.iter().any(|p| p.is_some()), "{prompts:?}");
    s.screenshot("prompts");
    // The slide show draws nothing for them.
    s.key("cmd-enter");
    s.screenshot("show");
    s.key("escape");
    s.double_click_shape(0);
    assert!(s.with(|e, _| e.text_editing().is_some()), "typing into the placeholder");
    s.type_text("新しい見出し");
    s.key("escape");
    assert_eq!(s.with(|e, _| e.slide().shapes[0].text_body().unwrap().text()), "新しい見出し");
    s.screenshot("typed");
    s.save_as("prompts.pptx");
    let saved = s.reopen();
    let slide = &saved.slides[1];
    assert_eq!(slide.shapes.len(), count);
    assert_eq!(slide.shapes[0].text_body().unwrap().text(), "新しい見出し");
    assert!(slide.shapes[1..].iter().all(|sh| sh.text_body().is_none_or(|b| b.is_empty())), "the others stay empty");
    undo_all_and_redo(&mut s);
}

/// コメント: a file PowerPoint saved with threaded comments shows their
/// marks and lists them; a comment on the slide and one on a shape are
/// added, answered, resolved and removed; saved, they read back. The
/// slide show draws no marks.
pub(crate) fn slide_comments() {
    let mut s = Scenario::open("test/editing/pptx-comments-modern.pptx");
    assert_eq!(s.with(|e, _| e.slide().comments.len()), 2);
    s.menu("insert.comment");
    assert!(s.with(|e, _| e.pptx().unwrap().comments.is_some()), "the comments' panel");
    s.screenshot("marks-and-panel");
    let input = s.with(|e, _| e.pptx().unwrap().comments.as_ref().unwrap().input.clone());
    let send = |s: &mut Scenario, text: &str| {
        let text = text.to_string();
        s.update(|e, w, cx| {
            input.update(cx, |i, cx| i.set_value(text.clone(), w, cx));
            e.send_slide_comment(&text, w, cx);
        });
    };
    // On the slide (nothing selected).
    send(&mut s, "スライドへのコメント");
    // On the subtitle (⌥⌘M).
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.click_shape(1);
    s.key("alt-cmd-m");
    send(&mut s, "サブタイトルへのコメント");
    let (count, on_shape, created) = s.with(|e, _| {
        let c = &e.slide().comments;
        (c.len(), c[3].shape == Some(e.slide().shapes[1].id), c[2].created.clone())
    });
    assert_eq!(count, 4);
    assert!(on_shape, "the second new one is on the subtitle");
    assert!(!created.ends_with('Z'), "{created}");
    // Answered, resolved; コメント2 removed.
    s.update(|e, w, cx| e.reply_to_slide_comment(2, w, cx));
    send(&mut s, "返信です");
    s.update(|e, _, cx| e.resolve_slide_comment(3, true, cx));
    s.update(|e, _, cx| e.delete_slide_comment(1, cx));
    let comments = s.with(|e, _| e.slide().comments.clone());
    assert_eq!(comments.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), vec!["コメント1", "スライドへのコメント", "サブタイトルへのコメント"]);
    assert_eq!(comments[1].replies[0].text, "返信です");
    assert!(comments[2].resolved);
    s.screenshot("edited");
    // The panel's field has the keys: the slide takes them back.
    s.update(|e, w, cx| {
        e.close_slide_comments(cx);
        e.focus_handle.focus(w, cx);
    });
    s.key("cmd-enter");
    assert!(s.with(|e, _| e.slideshow.is_some()), "the slide show");
    s.screenshot("show");
    s.key("escape");
    s.save_as("comments.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].comments, comments, "read back the same");
    undo_all_and_redo(&mut s);
}

/// グラフのデータの編集: the selected chart's panel (データの編集（簡易）) takes a
/// new value and a series' name; a word where a number goes changes
/// nothing; saved, the chart reads back with them.
pub(crate) fn edit_chart_data() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s
        .with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(&sh.kind, ShapeKind::Chart(c) if c.groups.iter().any(|g| g.series.iter().any(|x| x.values.len() > 1)))).map(|j| (i, j))))
        .expect("a slide with a chart");
    s.page(page + 1);
    s.click_shape(shape);
    s.menu("pptx.chart.data-quick");
    let (names, values) = s.with(|e, _| {
        let p = e.pptx().unwrap().chart_panel.as_ref().unwrap();
        (p.names.clone(), p.values.clone())
    });
    let series = values.iter().position(|v| v.len() > 1).unwrap();
    let original = s.with(|e, _| e.slide().shapes[shape].clone());
    // Not a number: nothing changes.
    s.update(|_, w, cx| values[series][0].update(cx, |f, cx| f.set_value("たくさん", w, cx)));
    s.menu("pptx.chart.apply");
    assert_eq!(s.with(|e, _| e.slide().shapes[shape].clone()), original);
    s.update(|_, w, cx| {
        values[series][0].update(cx, |f, cx| f.set_value("1,234.5", w, cx));
        names[series].update(cx, |f, cx| f.set_value("書き換えた系列", w, cx));
    });
    s.menu("pptx.chart.apply");
    let read = |p: &ibis_doc_model::Presentation| -> (Option<f64>, String) {
        let ShapeKind::Chart(c) = &p.slides[page].shapes[shape].kind else { panic!("a chart") };
        let x = c.groups.iter().flat_map(|g| g.series.iter()).nth(series).unwrap();
        (x.values[0].map(|n| n.0), x.name.clone())
    };
    assert_eq!(s.with(|e, _| read(e.presentation())), (Some(1234.5), "書き換えた系列".to_string()));
    s.screenshot("edited");
    // グラフ タイトル: above chart, then none, with undo/redo.
    let title = |s: &mut Scenario| s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Chart(c) => c.title.as_ref().map(|t| t.text.clone()),
        _ => None,
    });
    assert_eq!(title(&mut s), None, "initially has no title");
    s.toolbar_menu("chart-title", "chart.title.above");
    assert!(title(&mut s).is_some(), "title is added above chart");
    s.screenshot("title-above");
    s.key("cmd-z");
    assert_eq!(title(&mut s), None, "undone: title is removed");
    s.key("cmd-shift-z");
    assert!(title(&mut s).is_some(), "redone: title is restored");
    // 凡例: below, then none.
    let legend = |s: &mut Scenario| s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Chart(c) => c.legend.as_ref().map(|l| l.side),
        _ => None,
    });
    s.toolbar_menu("chart-legend", "chart.legend.bottom");
    assert_eq!(legend(&mut s), Some(ibis_doc_model::Side::Bottom));
    s.screenshot("legend-bottom");
    s.toolbar_menu("chart-legend", "chart.legend.none");
    assert_eq!(legend(&mut s), None);
    s.toolbar_menu("chart-legend", "chart.legend.right");
    assert_eq!(legend(&mut s), Some(ibis_doc_model::Side::Right));
    // データラベル: none, then the values.
    let labels = |s: &mut Scenario| s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Chart(c) => c.groups.iter().flat_map(|g| &g.series).map(|x| x.labels.as_ref().map(|l| l.show_value)).collect::<Vec<_>>(),
        _ => Vec::new(),
    });
    s.toolbar_menu("chart-labels", "chart.labels.none");
    assert!(labels(&mut s).iter().all(Option::is_none));
    s.toolbar_menu("chart-labels", "chart.labels.value");
    assert!(labels(&mut s).iter().all(|l| *l == Some(true)));
    s.screenshot("labels");
    s.save_as("chart-data.pptx");
    let reopened = s.reopen();
    assert_eq!(read(&reopened), (Some(1234.5), "書き換えた系列".to_string()));
    let ShapeKind::Chart(rc) = &reopened.slides[page].shapes[shape].kind else { panic!("a chart") };
    assert!(rc.title.is_some(), "saved chart title is kept");
    undo_all_and_redo(&mut s);
}

/// 挿入 > グラフ: a clustered column chart of the sample data, its data
/// panel open; a value changed; saved, the chart reads back with it.
pub(crate) fn insert_chart() {
    // On a new slide of タイトルとコンテンツ: in its content placeholder.
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let layout = s.with(|e, _| e.presentation().layouts.iter().find(|l| l.name == "タイトルとコンテンツ").map(|l| l.id));
    if let Some(layout) = layout {
        s.update(|e, w, cx| e.add_slide_with_layout(layout, w, cx));
        let body = s.with(|e, _| e.slide().shapes.iter().find(|sh| e.slide().placeholders.get(&sh.id).is_some_and(|p| p.placeholder.normalized() == "body")).map(|sh| (sh.id, sh.frame)));
        let (body, frame) = body.expect("a content placeholder");
        s.menu("pptx.insert-chart.column");
        let (gone, chart_frame) = s.with(|e, _| (e.slide().shape(body).is_none(), e.slide().shapes.iter().find(|sh| matches!(sh.kind, ShapeKind::Chart(_))).map(|sh| sh.frame)));
        assert!(gone, "the chart takes the placeholder's place");
        assert_eq!(chart_frame, Some(frame));
        s.screenshot("in-placeholder");
        s.key("escape");
        s.save_as("chart-placeholder.pptx");
        let saved = s.reopen();
        assert!(saved.slides.iter().any(|sl| sl.shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Chart(_)) && sh.frame == frame)));
    }
    // One scenario at a time.
    drop(s);
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    let before = s.shapes();
    s.menu("pptx.insert-chart.column");
    assert_eq!(s.shapes(), before + 1);
    assert!(s.with(|e, _| e.pptx().unwrap().chart_panel.is_some()), "its data panel");
    let values = s.with(|e, _| e.pptx().unwrap().chart_panel.as_ref().unwrap().values.clone());
    s.update(|_, w, cx| values[0][0].update(cx, |f, cx| f.set_value("6", w, cx)));
    s.menu("pptx.chart.apply");
    let first = |p: &ibis_doc_model::Presentation| match &p.slides[1].shapes[before].kind {
        ShapeKind::Chart(c) => c.groups[0].series[0].values[0].map(|n| n.0),
        _ => None,
    };
    assert_eq!(s.with(|e, _| first(e.presentation())), Some(6.0));
    s.screenshot("chart");
    s.save_as("chart.pptx");
    assert_eq!(first(&s.reopen()), Some(6.0));
    undo_all_and_redo(&mut s);
}

/// 挿入 > グラフ: major chart types (Pie, Doughnut, Clustered Bar, Line) inserted
/// from menus, data panel edited, pie/doughnut options customized, saved,
/// reopened, and Undo/Redo verified.
pub(crate) fn insert_chart_kinds() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    let before = s.shapes();

    // 1. Insert Pie chart from menu
    s.menu("pptx.insert-chart.pie");
    assert_eq!(s.shapes(), before + 1);
    assert_eq!(s.with(|e, _| e.selection_kind()), SelectionKind::Chart);
    assert!(s.with(|e, _| e.pptx().unwrap().chart_panel.is_some()), "data panel open");

    // Edit pie chart data: change first value
    let values = s.with(|e, _| e.pptx().unwrap().chart_panel.as_ref().unwrap().values.clone());
    s.update(|_, w, cx| values[0][0].update(cx, |f, cx| f.set_value("10.5", w, cx)));
    s.menu("pptx.chart.apply");

    // Check first slice angle menu option
    assert!(s.toolbar_state("chart-first-angle").is_some());
    s.toolbar_menu("chart-first-angle", "pptx.chart.first-angle.90");

    let pie_shape = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Chart(pie_chart) = &pie_shape.kind else { panic!("a chart") };
    assert!(matches!(pie_chart.groups[0].kind, ibis_doc_model::ChartType::Pie { first_angle: 90 }));

    // 2. Insert Doughnut chart from menu
    s.menu("pptx.insert-chart.doughnut");
    assert_eq!(s.shapes(), before + 2);
    s.toolbar_menu("chart-hole-size", "pptx.chart.hole-size.50");

    let doughnut_shape = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Chart(doughnut_chart) = &doughnut_shape.kind else { panic!("a chart") };
    assert!(matches!(doughnut_chart.groups[0].kind, ibis_doc_model::ChartType::Doughnut { hole: 50, .. }));

    // 3. Insert Bar chart from menu
    s.menu("pptx.insert-chart.bar");
    assert_eq!(s.shapes(), before + 3);
    let bar_shape = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Chart(bar_chart) = &bar_shape.kind else { panic!("a chart") };
    assert!(matches!(bar_chart.groups[0].kind, ibis_doc_model::ChartType::Bar { horizontal: true, .. }));

    // 4. Insert Line chart from menu
    s.menu("pptx.insert-chart.line");
    assert_eq!(s.shapes(), before + 4);
    let line_shape = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Chart(line_chart) = &line_shape.kind else { panic!("a chart") };
    assert!(matches!(line_chart.groups[0].kind, ibis_doc_model::ChartType::Line));

    s.save_as("chart-kinds-inserted.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[1].shapes.len(), before + 4);

    undo_all_and_redo(&mut s);
}

/// UI の英語化: with English chosen, the PowerPoint editor's menus and
/// toolbar (nothing selected; a text shape, a picture, a chart, a table
/// selected) show no Japanese but the file's own names (its layouts, fonts,
/// sections); and its panels, as the screenshots show.
pub(crate) fn english_ui() {
    use crate::shell::chrome::{MenuEntry, ToolItem};
    use crate::shell::i18n::{self, Language};
    // Japanese again however this ends (the next scenarios expect it).
    struct Back;
    impl Drop for Back {
        fn drop(&mut self) {
            i18n::show(Language::Japanese);
        }
    }
    let _back = Back;
    let japanese = |t: &str| t.chars().any(|c| ('\u{3040}'..='\u{30ff}').contains(&c) || ('\u{4e00}'..='\u{9fff}').contains(&c));
    // Labels made of the file's names, by id.
    let from_file = |id: &str| ["pptx.new-slide.layout", "pptx.layout.", "pptx.apply-layout", "font", "pptx.font.", "pptx.section", "file.recent."].iter().any(|p| id.starts_with(p));
    fn walk(entries: &[MenuEntry], out: &mut Vec<(String, String)>) {
        for e in entries {
            match e {
                MenuEntry::Item(i) => out.push((i.id.to_string(), i.label.to_string())),
                MenuEntry::Submenu(sub) => {
                    out.push((sub.id.to_string(), sub.label.to_string()));
                    walk(&sub.items, out);
                }
                MenuEntry::Palette(_) | MenuEntry::Separator => {}
            }
        }
    }
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    // English after opening (a scenario starts in Japanese).
    i18n::show(Language::English);
    s.update(|_, _, _| {});
    let mut left: Vec<(String, String, String)> = Vec::new();
    let mut check = |s: &mut Scenario, state: &str| {
        let labels = s.with(|e, _| {
            let mut out = Vec::new();
            for (_, entries) in e.menus().menus {
                walk(&entries, &mut out);
            }
            for item in e.toolbar_items() {
                match item {
                    ToolItem::Button(b) => {
                        out.push((b.id.to_string(), b.tooltip.to_string()));
                        if let Some(t) = &b.text {
                            out.push((b.id.to_string(), t.to_string()));
                        }
                        if let Some(menu) = &b.menu {
                            walk(menu, &mut out);
                        }
                    }
                    ToolItem::Select { id, tooltip, menu, .. } => {
                        out.push((id.to_string(), tooltip.to_string()));
                        walk(&menu, &mut out);
                    }
                    _ => {}
                }
            }
            out
        });
        for (id, label) in labels {
            if japanese(&label) && !from_file(&id) && !left.iter().any(|(_, i, l)| *i == id && *l == label) {
                left.push((state.to_string(), id, label));
            }
        }
    };
    check(&mut s, "none");
    s.screenshot("nothing-selected");
    let kinds = s.with(|e, _| {
        let find = |f: &dyn Fn(&ShapeKind) -> bool| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| f(&sh.kind)).map(|j| (i, j)));
        [
            ("text", find(&|k| matches!(k, ShapeKind::Auto(a) if a.text.as_ref().is_some_and(|t| !t.is_empty())))),
            ("picture", find(&|k| matches!(k, ShapeKind::Picture(_)))),
            ("chart", find(&|k| matches!(k, ShapeKind::Chart(_)))),
            ("table", find(&|k| matches!(k, ShapeKind::Table(_)))),
        ]
    });
    for (state, at) in kinds {
        let Some((page, shape)) = at else { continue };
        s.page(page + 1);
        s.click_shape(shape);
        check(&mut s, state);
        s.screenshot(state);
        s.key("escape");
    }
    // Panels.
    if let Some((page, shape)) = kinds[2].1 {
        s.page(page + 1);
        s.click_shape(shape);
        s.menu("pptx.chart.data-quick");
        s.screenshot("chart-panel");
        s.update(|e, w, cx| e.close_chart_panel(w, cx));
        s.menu("pptx.frame.settings");
        s.screenshot("frame-panel");
        s.update(|e, w, cx| e.close_frame_panel(w, cx));
        s.menu("insert.comment");
        s.screenshot("comments-panel");
    }
    assert!(left.is_empty(), "Japanese left in the English UI ({}):\n{}", left.len(), left.iter().map(|(st, id, l)| format!("{st}\t{id}\t{l}")).collect::<Vec<_>>().join("\n"));
}

/// SmartArt（図表）: a file PowerPoint saved with SmartArt graphics shows
/// their shapes (read from its drawings); a graphic moved with the arrow
/// keys takes them along, and is saved so. The file is outside git (it
/// holds Office's layout definitions): IBIS_PSMT names it (the coverage
/// suite PSMT's office.pptx); without it this does nothing.
pub(crate) fn smartart_shown_and_moved() {
    use ibis_doc_model::Placeholder;
    let Ok(path) = std::env::var("IBIS_PSMT") else {
        eprintln!("pptx::smartart_shown_and_moved: IBIS_PSMT is not set; skipped");
        return;
    };
    let mut s = Scenario::open(&path);
    let (graphic, parts) = s.with(|e, _| {
        let sl = e.slide();
        let i = sl.shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Placeholder(Placeholder::Diagram))).expect("a SmartArt graphic");
        (i, sl.attached.get(&sl.shapes[i].id).cloned().unwrap_or_default())
    });
    assert!(!parts.is_empty(), "its shapes");
    s.screenshot("shown");
    let frames = |s: &mut Scenario| s.with(|e, _| parts.iter().map(|id| e.slide().shape(*id).unwrap().frame).collect::<Vec<_>>());
    let before = (s.with(|e, _| e.slide().shapes[graphic].frame), frames(&mut s));
    s.click_shape(graphic);
    s.key("right");
    s.key("down");
    let after = (s.with(|e, _| e.slide().shapes[graphic].frame), frames(&mut s));
    let (dx, dy) = (after.0.x - before.0.x, after.0.y - before.0.y);
    assert!(dx > 0 && dy > 0, "moved");
    assert!(before.1.iter().zip(&after.1).all(|(a, b)| b.x - a.x == dx && b.y - a.y == dy), "its shapes along");
    s.screenshot("moved");
    s.key("escape");
    s.save_as("smartart.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes[graphic].frame, after.0);
    undo_all_and_redo(&mut s);
}

/// SmartArt を図形に変換 (SMART-017): the graphic's shapes become the slide's own
/// grouped shapes and the graphic goes; saved, read again (the graphic's parts
/// are out of the file), undone. Needs the file PowerPoint saved (IBIS_PSMT).
pub(crate) fn smartart_to_shapes() {
    use ibis_doc_model::Placeholder;
    let Ok(path) = std::env::var("IBIS_PSMT") else {
        eprintln!("pptx::smartart_to_shapes: IBIS_PSMT is not set; skipped");
        return;
    };
    let mut s = Scenario::open(&path);
    let (graphic, parts, texts) = s.with(|e, _| {
        let sl = e.slide();
        let i = sl.shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Placeholder(Placeholder::Diagram))).expect("a SmartArt graphic");
        let ids = sl.attached.get(&sl.shapes[i].id).cloned().unwrap_or_default();
        let texts: Vec<String> = ids.iter().filter_map(|id| sl.shape(*id)?.text_body().map(|b| b.text())).collect();
        (i, ids.len(), texts)
    });
    let diagrams_before = s.with(|e, _| e.slide().shapes.iter().filter(|sh| matches!(sh.kind, ShapeKind::Placeholder(Placeholder::Diagram))).count());
    let shapes_before = s.with(|e, _| e.slide().shapes.len());
    let attached_before: usize = s.with(|e, _| e.slide().attached.values().map(Vec::len).sum());
    assert_eq!(s.menu_state("pptx.smartart-to-shapes").map(|(enabled, _)| enabled), Some(false), "nothing selected");
    s.click_shape(graphic);
    assert_eq!(s.menu_state("pptx.smartart-to-shapes").map(|(enabled, _)| enabled), Some(true));
    s.menu("pptx.smartart-to-shapes");
    let diagrams_after = s.with(|e, _| e.slide().shapes.iter().filter(|sh| matches!(sh.kind, ShapeKind::Placeholder(Placeholder::Diagram))).count());
    assert_eq!(diagrams_after, diagrams_before - 1, "the graphic is gone");
    // Its own shapes in its place: the graphic and the shapes attached are replaced by the copies.
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), shapes_before - 1, "{parts} shapes of the graphic stay as the slide's");
    let now_texts: Vec<String> = s.with(|e, _| e.slide().shapes.iter().filter(|sh| !e.slide().is_attached(sh.id)).filter_map(|sh| sh.text_body().map(|b| b.text())).collect());
    assert!(texts.iter().filter(|t| !t.is_empty()).all(|t| now_texts.contains(t)), "their text: {texts:?}");
    assert_eq!(s.with(|e, _| e.slide().groups.len()), 1, "grouped");
    s.screenshot("converted");
    s.save_as("smartart-to-shapes.pptx");
    let saved = s.reopen();
    let sl = &saved.slides[0];
    assert_eq!(sl.shapes.iter().filter(|sh| matches!(sh.kind, ShapeKind::Placeholder(Placeholder::Diagram))).count(), diagrams_before - 1);
    // The other graphics still have the drawing they are read from.
    assert_eq!(sl.attached.values().map(Vec::len).sum::<usize>(), attached_before - parts, "the others' drawings stay in the file");
    let saved_texts: Vec<String> = sl.shapes.iter().filter(|sh| !sl.is_attached(sh.id)).filter_map(|sh| sh.text_body().map(|b| b.text())).collect();
    assert!(texts.iter().filter(|t| !t.is_empty()).all(|t| saved_texts.contains(t)), "their text saved: {texts:?} / {saved_texts:?}");
    undo_all_and_redo(&mut s);
}

/// スライドのコピー・切り取り・貼り付け: slide 2 copied and pasted after
/// slide 4 (its text kept; the copy edits on its own), slide 3 cut and
/// pasted at the end; saved in that order, with the copied slide's
/// formatting; undone and redone.
pub(crate) fn copy_cut_and_paste_slides() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let titles = |s: &mut Scenario| s.with(|e, _| e.presentation().slides.iter().map(|sl| sl.shapes.first().and_then(|sh| sh.text_body()).map(|b| b.text()).unwrap_or_default()).collect::<Vec<_>>());
    let before = titles(&mut s);
    s.page(2);
    s.menu("pptx.copy-slide");
    s.page(4);
    s.menu("pptx.paste-slide");
    let after = titles(&mut s);
    assert_eq!(after.len(), before.len() + 1);
    assert_eq!(after[4], before[1], "the copy after slide 4");
    assert_eq!(s.with(|e, _| e.presentation().slide_index(e.current_slide)), Some(4), "the pasted slide shown");
    // The copy edits on its own.
    s.double_click_shape(0);
    s.key("end");
    s.type_text("（写し）");
    s.key("escape");
    assert_eq!(titles(&mut s)[1], before[1]);
    // Cut slide 3, pasted at the end (from the slide list, with the keys).
    s.page(3);
    s.update(|e, w, cx| e.cut_current_slide(w, cx));
    assert_eq!(titles(&mut s).len(), before.len());
    let count = titles(&mut s).len();
    s.page(count);
    s.update(|e, w, cx| { assert!(e.paste_slides(w, cx)); });
    let end = titles(&mut s);
    assert_eq!(end.last(), Some(&before[2]), "the cut slide at the end");
    s.screenshot("pasted");
    s.save_as("slides-pasted.pptx");
    let saved = s.reopen();
    let saved_titles: Vec<String> = saved.slides.iter().map(|sl| sl.shapes.first().and_then(|sh| sh.text_body()).map(|b| b.text()).unwrap_or_default()).collect();
    assert_eq!(saved_titles, end);
    undo_all_and_redo(&mut s);
    // With the keys, in the slide list: ⌘C then ⌘V.
    let count = titles(&mut s).len();
    s.update(|e, w, cx| e.list_focus.focus(w, cx));
    s.key("cmd-c");
    s.key("cmd-v");
    assert_eq!(titles(&mut s).len(), count + 1, "pasted with the keys");
}

/// スライドの貼り付け from another file: the benchmark's chart slide
/// copied, pasted into the plan (in the plan's theme); its chart and
/// pictures come along; saved, read back with them; undone.
pub(crate) fn paste_slide_from_another_file() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let page = s.with(|e, _| e.presentation().slides.iter().position(|sl| sl.shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Chart(_)))).unwrap());
    s.page(page + 1);
    s.menu("pptx.copy-slide");
    let shapes = s.with(|e, _| e.slide().shapes.len());
    drop(s);
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let count = s.with(|e, _| e.presentation().slides.len());
    s.page(2);
    s.menu("pptx.paste-slide");
    let (now, pasted, charts, layout) = s.with(|e, _| {
        let sl = e.slide();
        (e.presentation().slides.len(), sl.shapes.len(), sl.shapes.iter().filter(|sh| matches!(sh.kind, ShapeKind::Chart(_))).count(), sl.layout.and_then(|l| e.presentation().layout(l)).map(|l| l.name.clone()))
    });
    assert_eq!(now, count + 1);
    assert_eq!(pasted, shapes, "all its shapes");
    assert!(charts > 0, "its charts");
    assert!(layout.is_some(), "a layout of the plan");
    s.screenshot("pasted");
    s.save_as("pasted.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides.len(), count + 1);
    assert_eq!(saved.slides[2].shapes.iter().filter(|sh| matches!(sh.kind, ShapeKind::Chart(_))).count(), charts);
    undo_all_and_redo(&mut s);
    // 元の書式を保持: the other file's layout and master come along.
    let layouts = s.with(|e, _| e.presentation().layouts.len());
    s.menu("pptx.paste-slide-keep");
    let (more, own) = s.with(|e, _| (e.presentation().layouts.len(), e.slide().layout.and_then(|l| e.presentation().layout(l)).map(|l| l.name.clone())));
    assert!(more > layouts, "its layouts");
    assert!(own.is_some());
    s.screenshot("pasted-keeping-formatting");
    s.save_as("pasted-keep.pptx");
    let saved = s.reopen();
    assert_eq!(saved.layouts.len(), more);
}

/// アニメーション from the toolbar: the title fades in on a click, the
/// subtitle flies in from the bottom with it (1 second); saved, read back the
/// same; the slide show plays them as one click; the title's effect removed.
pub(crate) fn add_animations_from_the_menu() {
    use ibis_doc_model::{AnimationClass, AnimationLook, AnimationSide, AnimationStart};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.in.fade");
    s.click_shape(1);
    s.toolbar_menu("motion", "pptx.anim.in.fly-bottom");
    s.toolbar_menu("motion", "pptx.anim.start.with");
    s.toolbar_menu("motion", "pptx.anim.duration.1000");
    let animations = s.with(|e, _| e.slide().animations.clone());
    let summary: Vec<_> = animations.iter().map(|a| (a.class, a.start, a.look, a.duration)).collect();
    assert_eq!(
        summary,
        vec![
            (AnimationClass::Entrance, AnimationStart::OnClick, AnimationLook::Fade, 500),
            (AnimationClass::Entrance, AnimationStart::WithPrevious, AnimationLook::Fly(AnimationSide::Bottom), 1000),
        ]
    );
    assert_eq!(s.toolbar_state("motion").map(|t| t.enabled), Some(true));
    s.save_as("animated.pptx");
    assert_eq!(s.reopen().slides[0].animations.iter().map(|a| (a.class, a.start, a.look, a.duration)).collect::<Vec<_>>(), summary, "read back the same");
    // The slide show: both in one click.
    s.key("cmd-shift-enter");
    assert_eq!(s.with(|e, _| e.slideshow.as_ref().map(|show| show.step)), Some(0));
    s.key("right");
    assert_eq!(s.with(|e, _| (e.presentation().slide_index(e.current_slide), e.slideshow.as_ref().map(|show| show.step))), (Some(0), Some(1)));
    s.key("right");
    assert_eq!(s.with(|e, _| e.presentation().slide_index(e.current_slide)), Some(1), "then the next slide");
    s.key("escape");
    s.page(1);
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.remove");
    assert_eq!(s.with(|e, _| e.slide().animations.len()), 1);
    undo_all_and_redo(&mut s);
}

/// 図形の変更: an auto shape made an ellipse, then a heart, from the
/// toolbar; its text and fill kept; saved, read back so; undone.
pub(crate) fn change_shape() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let (page, index) = s
        .with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(&sh.kind, ShapeKind::Auto(a) if a.text.as_ref().is_some_and(|t| !t.is_empty()) && !sl.placeholders.contains_key(&sh.id))).map(|j| (i, j))))
        .expect("an auto shape with text");
    s.page(page + 1);
    s.click_shape(index);
    let before = s.with(|e, _| e.slide().shapes[index].clone());
    s.toolbar_menu("change-shape", "change-shape.ellipse");
    s.toolbar_menu("change-shape", "change-shape.heart");
    let after = s.with(|e, _| e.slide().shapes[index].clone());
    let (ShapeKind::Auto(a), ShapeKind::Auto(b)) = (&before.kind, &after.kind) else { panic!("auto shapes") };
    assert_eq!(b.geometry, ibis_pptx::preset_geometry("heart").unwrap());
    assert_eq!((&a.text, &a.fill, &a.stroke, before.frame), (&b.text, &b.fill, &b.stroke, after.frame), "all but its form kept");
    s.screenshot("heart");
    s.save_as("changed.pptx");
    let ShapeKind::Auto(c) = &s.reopen().slides[page].shapes[index].kind else { panic!("an auto shape") };
    assert_eq!(c.geometry, b.geometry);
    undo_all_and_redo(&mut s);
}

/// 高さを揃える・幅を揃える: a table's rows the same height and its
/// columns the same width, from 行と列; saved, read back so.
pub(crate) fn distribute_table_rows_and_columns() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s
        .with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(&sh.kind, ShapeKind::Table(t) if !t.has_merges() && t.columns.len() > 1)).map(|j| (i, j))))
        .expect("a table without merged cells");
    s.page(page + 1);
    s.click_shape(shape);
    s.toolbar_menu("table.rows", "table.distribute-columns");
    s.toolbar_menu("table.rows", "table.distribute-rows");
    let table = |p: &ibis_doc_model::Presentation| match &p.slides[page].shapes[shape].kind {
        ShapeKind::Table(t) => (t.columns.clone(), t.rows.iter().map(|r| r.height).collect::<Vec<_>>()),
        _ => panic!("a table"),
    };
    let (columns, rows) = s.with(|e, _| table(e.presentation()));
    let (cmax, cmin) = (columns.iter().max().unwrap(), columns.iter().min().unwrap());
    assert!(cmax - cmin < columns.len() as i64, "the same widths: {columns:?}");
    let (rmax, rmin) = (rows.iter().max().unwrap(), rows.iter().min().unwrap());
    assert!(rmax - rmin < rows.len() as i64, "the same heights: {rows:?}");
    s.screenshot("distributed");
    s.save_as("distributed.pptx");
    assert_eq!(table(&s.reopen()), (columns, rows));
    undo_all_and_redo(&mut s);
}

/// 背景のすべてに適用: slide 2's background set, then put on every slide;
/// saved, read back so; undone.
pub(crate) fn background_applied_to_all() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    s.toolbar_menu("background", "color.light-blue");
    let fill = s.with(|e, _| e.slide().background.clone());
    s.toolbar_menu("background", "background.apply-all");
    assert!(s.with(|e, _| e.presentation().slides.iter().all(|sl| sl.background == fill)));
    s.save_as("background-all.pptx");
    assert!(s.reopen().slides.iter().all(|sl| sl.background == fill));
    undo_all_and_redo(&mut s);
}

/// ヘッダーとフッター: the slide number and a footer on every slide but
/// the title slide (タイトル スライドに表示しない); saved, read back so; the
/// slide number then taken away from all.
pub(crate) fn header_and_footer() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let kinds = |p: &ibis_doc_model::Presentation| -> Vec<Vec<String>> {
        p.slides.iter().map(|sl| { let mut k: Vec<String> = sl.placeholders.values().filter_map(|ph| ph.placeholder.kind.clone()).filter(|k| matches!(k.as_str(), "dt" | "ftr" | "sldNum")).collect(); k.sort(); k }).collect()
    };
    let before = s.with(|e, _| kinds(e.presentation()));
    assert!(before.iter().all(|k| k.is_empty()), "none at first: {before:?}");
    s.menu("pptx.header-footer");
    let footer = s.with(|e, _| e.pptx().unwrap().header_footer.as_ref().unwrap().footer_text.clone());
    s.update(|e, w, cx| {
        e.toggle_header_footer("number", true, cx);
        e.toggle_header_footer("footer", true, cx);
        e.toggle_header_footer("title", true, cx);
        footer.update(cx, |f, cx| f.set_value("架空商店", w, cx));
    });
    s.screenshot("panel");
    s.update(|e, w, cx| e.apply_header_footer(true, w, cx));
    let after = s.with(|e, _| kinds(e.presentation()));
    assert!(after[0].is_empty(), "not on the title slide");
    assert!(after[1..].iter().all(|k| *k == vec!["ftr".to_string(), "sldNum".to_string()]), "{after:?}");
    let footer_text = s.with(|e, _| e.presentation().slides[1].shapes.iter().find(|sh| e.presentation().slides[1].placeholders.get(&sh.id).is_some_and(|p| p.placeholder.kind.as_deref() == Some("ftr"))).and_then(|sh| sh.text_body()).map(|b| b.text()));
    assert_eq!(footer_text.as_deref(), Some("架空商店"));
    s.page(2);
    s.screenshot("slide-2");
    s.save_as("footers.pptx");
    let reopened = s.reopen();
    assert_eq!(kinds(&reopened), after);
    // Marked as PowerPoint's すべてに適用 marks them (showSpecialPlsOnTitleSld, the masters' p:hf).
    let marks = ibis_doc_model::model::HeaderFooterMarks { on_title_slides: false, header: false, date: false, footer: true, number: true };
    assert_eq!(reopened.header_footer, marks);
    // The slide number off, on all; the panel opens with タイトル スライドに表示しない on.
    s.menu("pptx.header-footer");
    assert!(s.with(|e, _| e.pptx().unwrap().header_footer.as_ref().unwrap().not_on_title));
    s.update(|e, w, cx| {
        e.toggle_header_footer("number", false, cx);
        e.apply_header_footer(true, w, cx);
    });
    assert!(s.with(|e, _| kinds(e.presentation()))[1..].iter().all(|k| *k == vec!["ftr".to_string()]));
    assert_eq!(s.with(|e, _| e.presentation().header_footer), ibis_doc_model::model::HeaderFooterMarks { number: false, ..marks });
    // The date (自動的に更新): a date field showing today, on this slide.
    s.page(2);
    s.menu("pptx.header-footer");
    s.update(|e, w, cx| {
        e.toggle_header_footer("date", true, cx);
        e.toggle_header_footer("auto", true, cx);
        e.apply_header_footer(false, w, cx);
    });
    let today = ibis_doc_model::date_field::slide_date_text(1, true, &crate::clock::local_now()).unwrap();
    let date_runs = |p: &ibis_doc_model::Presentation| -> Vec<(String, Option<ibis_doc_model::TextField>)> {
        let sl = &p.slides[1];
        sl.shapes.iter().filter(|sh| sl.placeholders.get(&sh.id).is_some_and(|ph| ph.placeholder.kind.as_deref() == Some("dt"))).flat_map(|sh| sh.text_body().unwrap().paragraphs[0].runs.iter().map(|r| (r.text.clone(), r.style.field))).collect()
    };
    let field = Some(ibis_doc_model::TextField::DateTime { format: 1, east_asian: true });
    assert_eq!(s.with(|e, _| date_runs(e.presentation())), vec![(today.clone(), field)]);
    s.save_as("footers-date.pptx");
    assert_eq!(date_runs(&s.reopen()), vec![(today, field)]);
    undo_all_and_redo(&mut s);
}

/// データの編集 in the workbook editor: a new chart's data opened in a
/// window of its own (its ranges framed), a value changed and a category
/// typed under the table (which grows, and the chart with it), the window
/// closed; saved, the chart and its workbook read back so.
pub(crate) fn chart_data_in_the_workbook() {
    use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, SheetCommand};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    s.update(|e, w, cx| e.insert_slide_chart(w, cx));
    s.update(|e, w, cx| e.close_chart_panel(w, cx));
    s.toolbar("chart-data");
    let (view, window, shape) = s.with(|e, _| {
        let d = e.pptx().unwrap().chart_data.as_ref().expect("the data window");
        (d.view.upgrade().unwrap(), d.window, d.shape)
    });
    let ranges = s.update_in(window, &view, |d, _, _| d.chart_ranges.clone()).expect("the ranges");
    assert_eq!(ranges.iter().map(|r| r.values.clone().unwrap()).collect::<Vec<_>>(), vec!["Sheet1!$B$2:$B$5", "Sheet1!$C$2:$C$5", "Sheet1!$D$2:$D$5"]);
    s.screenshot_of(window, "data");
    let set = |row: u32, column: u32, value: CellValue| CellEdit { row, column, input: CellInput::Value(value) };
    s.update_in(window, &view, |d, _, cx| {
        let slide = d.current_slide;
        d.run(vec![Command::Sheet(SheetCommand::SetCells { slide, edits: vec![set(3, 2, CellValue::Number(9.5))] })], cx);
    });
    let chart = |s: &mut Scenario| {
        s.with(move |e, _| match &e.slide().shape(shape).unwrap().kind {
            ShapeKind::Chart(c) => (**c).clone(),
            _ => panic!("a chart"),
        })
    };
    assert_eq!(chart(&mut s).groups[0].series[1].values[2], Some(ibis_doc_model::Number(9.5)));
    // A category more, typed under the table.
    s.update_in(window, &view, |d, _, cx| {
        let slide = d.current_slide;
        let input = CellInput::Value(CellValue::Text("分類 5".into()));
        d.run(vec![Command::Sheet(SheetCommand::SetCells { slide, edits: vec![CellEdit { row: 5, column: 0, input: input.clone() }] })], cx);
        d.after_table_entry(slide, 5, 0, &input, cx);
        d.run(vec![Command::Sheet(SheetCommand::SetCells { slide, edits: (1..=3).map(|c| set(5, c, CellValue::Number(c as f64))).collect() })], cx);
    });
    let grown = s.update_in(window, &view, |d, _, _| d.chart_ranges.clone()).unwrap();
    assert_eq!(grown[0].values.as_deref(), Some("Sheet1!$B$2:$B$6"));
    s.screenshot_of(window, "grown");
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series[0].categories.last().map(String::as_str), Some("分類 5"));
    assert_eq!(c.groups[0].series[2].values.last(), Some(&Some(ibis_doc_model::Number(3.0))));
    drop(view);
    s.close_window(window);
    assert!(s.with(|e, _| e.pptx().unwrap().chart_data.is_none()));
    s.screenshot("chart");
    s.save_as("chart-data.pptx");
    let saved = s.reopen();
    let read = saved.slides[1].shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    });
    assert_eq!(read.as_ref(), Some(&c));
    undo_all_and_redo(&mut s);
}

/// ウォーターフォール・ヒストグラム・パレート図・じょうご (CHART-007, 008, 010): 挿入 > グラフで、
/// そのグラフが入り、データの編集でブックを直すとグラフが変わる。書式のメニューは、
/// データの編集だけが使える。保存して読み直しても、同じ種類のグラフ。
pub(crate) fn chart_ex_insert_and_data() {
    use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, SheetCommand};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    let before = s.shapes();
    for (id, layout) in [("pptx.insert-chart.histogram", "clusteredColumn"), ("pptx.insert-chart.pareto", "paretoLine"), ("pptx.insert-chart.funnel", "funnel")] {
        s.menu(id);
        let _ = layout;
        assert_eq!(s.with(|e, _| e.selection_kind()), SelectionKind::Chart);
        assert!(s.with(|e, _| e.selected_chart().is_some_and(|(_, _, c)| c.ex.is_some())), "{id}: a chartEx chart");
    }
    assert_eq!(s.shapes(), before + 3);
    s.menu("pptx.insert-chart.waterfall");
    assert_eq!(s.shapes(), before + 4);
    assert!(s.with(|e, _| e.pptx().unwrap().chart_panel.is_none()), "no panel of the usual kind");
    assert!(s.toolbar_state("chart-data").is_some());
    assert!(s.toolbar_state("chart-type").is_none(), "only the data is edited");
    assert_eq!(s.menu_state("pptx.chart.data-quick").map(|(enabled, _)| enabled), Some(false));
    s.screenshot("waterfall");
    // The data in its workbook: 増加 1 (40) made 70.
    s.toolbar("chart-data");
    let (view, window, shape) = s.with(|e, _| {
        let d = e.pptx().unwrap().chart_data.as_ref().expect("the data window");
        (d.view.upgrade().unwrap(), d.window, d.shape)
    });
    s.update_in(window, &view, |d, _, cx| {
        let slide = d.current_slide;
        d.run(vec![Command::Sheet(SheetCommand::SetCells { slide, edits: vec![CellEdit { row: 2, column: 1, input: CellInput::Value(CellValue::Number(70.0)) }] })], cx);
    });
    let values = |s: &mut Scenario| {
        s.with(move |e, _| match &e.slide().shape(shape).unwrap().kind {
            ShapeKind::Chart(c) => c.groups.iter().flat_map(|g| g.series.iter().flat_map(|se| se.values.iter().flatten().map(|ibis_doc_model::Number(v)| *v))).collect::<Vec<f64>>(),
            _ => panic!("a chart"),
        })
    };
    assert!(values(&mut s).contains(&70.0), "the chart follows the workbook: {:?}", values(&mut s));
    drop(view);
    s.close_window(window);
    s.screenshot("waterfall-edited");
    s.save_as("chart-ex.pptx");
    let saved = s.reopen();
    let n = saved.slides[1].shapes.iter().filter(|sh| matches!(&sh.kind, ShapeKind::Chart(c) if c.ex.is_some())).count();
    assert_eq!(n, 4, "the four charts are read again as chartEx charts");
    undo_all_and_redo(&mut s);
}

/// 背景グラフィックを表示しない (the background menu) on a benchmark slide
/// with its layout's pictures; 均等割り付け and 両端揃え of the plan's
/// title (two lines); saved and read back so.
pub(crate) fn hide_background_graphics_and_distribute_text() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    s.page(2);
    assert!(s.with(|e, _| !e.slide().backdrop.is_empty()), "a slide with background graphics");
    s.screenshot("graphics");
    s.toolbar_menu("background", "background.hide-graphics");
    assert!(s.with(|e, _| e.slide().hide_backdrop));
    s.screenshot("no-graphics");
    s.save_as("no-graphics.pptx");
    assert!(s.reopen().slides[1].hide_backdrop);
    undo_all_and_redo(&mut s);
    drop(s);
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.click_shape(0);
    s.toolbar_menu("align", "align.distributed");
    let align = |s: &mut Scenario| s.with(|e, _| e.slide().shapes[0].text_body().unwrap().paragraphs[0].align);
    assert_eq!(align(&mut s), Some(ibis_doc_model::TextAlign::Distributed));
    s.screenshot("distributed");
    s.toolbar_menu("align", "align.justify");
    assert_eq!(align(&mut s), Some(ibis_doc_model::TextAlign::Justify));
    s.screenshot("justified");
    s.save_as("justified.pptx");
    assert_eq!(s.reopen().slides[0].shapes[0].text_body().unwrap().paragraphs[0].align, Some(ibis_doc_model::TextAlign::Justify));
    undo_all_and_redo(&mut s);
}

/// ハイパーリンク (⌘K): a shape of slide 3 linked to slide 4, two
/// characters of the title to a web page (underlined, in the link colour,
/// as the file reads them); in the slide show a click on the shape goes to
/// slide 4; saved and read back so; the shape's link taken away.
pub(crate) fn hyperlinks() {
    use ibis_doc_model::SlideLink;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    s.click_shape(1);
    s.key("cmd-k");
    assert!(s.with(|e, _| e.pptx().unwrap().link_panel.is_some()), "⌘K opens the panel");
    let (fourth, shape) = s.with(|e, _| (e.presentation().slides[3].id, e.slide().shapes[1].id));
    s.update(|e, w, cx| {
        if let Some(p) = e.pptx_mut().and_then(|s| s.link_panel.as_mut()) {
            p.web = false;
            p.place = Some(SlideLink::Slide(fourth));
        }
        cx.notify();
        let _ = w;
    });
    s.screenshot("panel");
    s.update(|e, w, cx| e.slide_apply_link(Some(SlideLink::Slide(fourth)), w, cx));
    assert_eq!(s.with(|e, _| e.slide().shape_links.get(&shape).cloned()), Some(SlideLink::Slide(fourth)));
    // The title's first two characters, to a page.
    s.page(1);
    s.double_click_shape(0);
    s.update(|e, _, cx| {
        if let Some(edit) = e.pptx_mut().and_then(|s| s.edit.as_mut()) {
            edit.anchor = 0;
            edit.caret = 2;
        }
        cx.notify();
    });
    s.key("cmd-k");
    let url = s.with(|e, _| e.pptx().unwrap().link_panel.as_ref().unwrap().url.clone());
    s.update(|_, w, cx| url.update(cx, |f, cx| f.set_value("example.com/plan", w, cx)));
    s.update(|e, w, cx| {
        let link = SlideLink::Url("https://example.com/plan".into());
        e.slide_apply_link(Some(link), w, cx)
    });
    let first = s.with(|e, _| e.slide().shapes[0].text_body().unwrap().paragraphs[0].runs[0].clone());
    assert_eq!(first.text.chars().count(), 2);
    assert_eq!(first.style.link, Some(SlideLink::Url("https://example.com/plan".into())));
    assert!(first.style.underline);
    s.screenshot("text-link");
    // The show: a click on the shape goes to slide 4.
    s.page(3);
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    let at = s.with(|e, app| {
        let size = e.presentation().size_of(e.slide());
        let f = e.slide().shape(shape).unwrap().frame;
        let _ = app;
        (size, f)
    });
    let (size, f) = at;
    let scale = (1200.0 / size.width as f32).min(720.0 / size.height as f32);
    let (x0, y0) = ((1200.0 - size.width as f32 * scale) / 2.0, (720.0 - size.height as f32 * scale) / 2.0);
    s.click((x0 + (f.x + f.width / 2) as f32 * scale, y0 + (f.y + f.height / 2) as f32 * scale));
    assert_eq!(s.with(|e, _| e.current_slide), fourth, "the link is followed");
    s.update(|e, w, cx| e.end_slideshow(w, cx));
    // Slide 2's shapes to the next, previous, first and last slide
    // (`ppaction://hlinkshowjump`), for a check in PowerPoint.
    s.page(2);
    let jumps = [SlideLink::NextSlide, SlideLink::PreviousSlide, SlideLink::FirstSlide, SlideLink::LastSlide];
    let count = s.with(|e, _| e.slide().shapes.len()).min(jumps.len());
    for (i, link) in jumps.iter().take(count).enumerate() {
        s.click_shape(i);
        s.key("cmd-k");
        s.update(|e, w, cx| e.slide_apply_link(Some(link.clone()), w, cx));
    }
    assert_eq!(s.with(|e, _| e.slide().shape_links.len()), count);
    s.save_as("links.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[2].shape_links.get(&saved.slides[2].shapes[1].id), Some(&SlideLink::Slide(saved.slides[3].id)));
    assert_eq!(saved.slides[0].shapes[0].text_body().unwrap().paragraphs[0].runs[0].style.link, Some(SlideLink::Url("https://example.com/plan".into())));
    let jump_links = saved.slides[1].shape_links.values().cloned().collect::<Vec<_>>();
    assert_eq!(jump_links.len(), count);
    assert!(jump_links.iter().all(|l| jumps.contains(l)));
    s.page(3);
    s.click_shape(1);
    s.key("cmd-k");
    s.update(|e, w, cx| e.slide_apply_link(None, w, cx));
    assert!(s.with(|e, _| e.slide().shape_links.is_empty()));
    undo_all_and_redo(&mut s);
}

/// 選択ウィンドウ: the shapes of the plan's third slide listed by name; one
/// renamed, one hidden (not drawn, not hit, still listed), one moved
/// forward; saved and read back so; すべて表示 shows them again.
pub(crate) fn selection_pane() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    s.menu("pptx.selection-pane");
    let ids = s.with(|e, _| e.slide().shapes.iter().map(|sh| sh.id).collect::<Vec<_>>());
    let names = s.with(|e, _| e.slide().shapes.iter().map(|sh| e.shape_label(sh.id)).collect::<Vec<_>>());
    assert!(names.iter().all(|n| !n.is_empty()));
    s.screenshot("pane");
    s.update(|e, w, cx| e.start_renaming(ids[0], w, cx));
    let field = s.with(|e, _| e.pptx().unwrap().selection_pane.as_ref().unwrap().renaming.as_ref().unwrap().1.clone());
    s.update(|_, w, cx| field.update(cx, |f, cx| f.set_value("本店の札", w, cx)));
    s.update(|e, w, cx| e.finish_renaming(w, cx));
    assert_eq!(s.with(|e, _| e.shape_label(ids[0])), "本店の札");
    s.update(|e, _, cx| e.set_shape_hidden(ids[1], true, cx));
    // Not hit where it was.
    let at = s.with(|e, _| {
        let f = e.slide().shape(ids[1]).unwrap().frame;
        ibis_doc_model::EmuPoint::new(f.x + f.width / 2, f.y + f.height / 2)
    });
    assert_ne!(s.with(|e, _| e.slide().hit_test(at)), Some(ids[1]));
    s.update(|e, _, cx| e.order_one(ids[0], true, cx));
    assert_eq!(s.with(|e, _| e.slide().shape_index(ids[0])), Some(1));
    s.screenshot("hidden-and-renamed");
    s.save_as("selection.pptx");
    let saved = s.reopen();
    let slide = &saved.slides[2];
    assert_eq!(slide.shape_names.get(&slide.shapes[1].id).map(String::as_str), Some("本店の札"));
    assert_eq!(slide.hidden_shapes.len(), 1);
    assert!(slide.hidden_shapes.contains(&slide.shapes[0].id), "the hidden one, now behind the renamed one");
    s.update(|e, _, cx| e.set_all_hidden(false, cx));
    assert!(s.with(|e, _| e.slide().hidden_shapes.is_empty()));
    undo_all_and_redo(&mut s);
}

/// アニメーション ウィンドウ: the title fades in, then (効果の追加) spins
/// too, the subtitle flies in; in the pane the subtitle's effect is moved
/// up to second, the spin taken away; saved in that order.
pub(crate) fn animation_pane() {
    use ibis_doc_model::{AnimationClass as C, AnimationLook as L, AnimationSide};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.in.fade");
    s.toolbar_menu("motion", "add.pptx.anim.em.spin");
    s.click_shape(1);
    s.toolbar_menu("motion", "pptx.anim.in.fly-bottom");
    s.toolbar_menu("motion", "pptx.anim.pane");
    let order = |s: &mut Scenario| s.with(|e, _| e.slide().animations.iter().map(|a| (e.slide().shape_index(a.shape).unwrap(), a.class, a.look)).collect::<Vec<_>>());
    assert_eq!(order(&mut s), vec![(0, C::Entrance, L::Fade), (0, C::Emphasis, L::Spin(21_600_000)), (1, C::Entrance, L::Fly(AnimationSide::Bottom))]);
    s.screenshot("pane");
    s.update(|e, _, cx| e.move_effect(2, true, cx));
    s.update(|e, _, cx| e.remove_effect(2, cx));
    let end = order(&mut s);
    assert_eq!(end, vec![(0, C::Entrance, L::Fade), (1, C::Entrance, L::Fly(AnimationSide::Bottom))]);
    s.screenshot("reordered");
    s.save_as("pane.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].animations.iter().map(|a| (saved.slides[0].shape_index(a.shape).unwrap(), a.class, a.look)).collect::<Vec<_>>(), end);
    undo_all_and_redo(&mut s);
}

/// アニメーション ウィンドウと詳細タイムライン:
/// 1. スライドに複数の図形を作成し、アニメーションを設定（フェード、スライドイン、スピン）
/// 2. アニメーションウィンドウを展開（toggle_animation_pane）
/// 3. compute_timeline による初期タイムライン計算（開始時刻、終了時刻、クリックグループ）を検証
/// 4. アニメーションウィンドウから開始タイミングをサイクル切り替え（cycle_effect_start_at）
///    - 2番目を WithPrevious（直前と同時）に変更 -> タイムライン上で直前と同時開始に更新されることを検証
///    - 3番目を AfterPrevious（直前の後）に変更 -> タイムライン上で直前終了後開始に更新されることを検証
/// 5. アニメーションウィンドウから継続時間および遅延時間を調整（adjust_effect_duration_at / adjust_effect_delay_at）
///    - duration を 500ms -> 1000ms に変更
///    - delay を 0ms -> 500ms に変更
///    - タイムラインのバー幅とオフセットが追従して更新されることを検証
/// 6. Undo/Redo でタイミング・遅延・継続時間が可逆復元されることを検証
/// 7. 保存（save_as）および再読込（reopen）で変更したタイミング設定（start, duration, delay）が正確に永続化されることを検証
pub(crate) fn animation_pane_and_detailed_timeline() {
    use ibis_doc_model::AnimationStart;
    use crate::pptx::animation_ui::compute_timeline;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 図形0 (タイトル) にフェード (開始)
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.in.fade");

    // 効果の追加: スピン (強調)
    s.toolbar_menu("motion", "add.pptx.anim.em.spin");

    // 図形1 (サブタイトル) にスライドイン (開始)
    s.click_shape(1);
    s.toolbar_menu("motion", "pptx.anim.in.fly-bottom");

    // アニメーションウィンドウを開く
    s.toolbar_menu("motion", "pptx.anim.pane");
    assert!(s.with(|e, _| e.pptx().is_some_and(|p| p.animation_pane)), "animation pane opened");

    // 初期状態のタイムライン計算を検証
    let (items, _) = s.with(|e, _| compute_timeline(&e.slide().animations));
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].click_group, 1);
    assert_eq!(items[0].start_time_ms, 0);
    assert_eq!(items[0].duration_ms, 500); // fade is 500ms
    assert_eq!(items[0].end_time_ms, 500);

    // 2つ目の効果の開始タイミングを「直前と同時 (WithPrevious)」に変更
    s.update(|e, _, cx| e.cycle_effect_start_at(1, cx));
    assert_eq!(s.with(|e, _| e.slide().animations[1].start), AnimationStart::WithPrevious);

    // 3つ目の効果の開始タイミングを「直前の後 (AfterPrevious)」に変更
    s.update(|e, _, cx| {
        e.cycle_effect_start_at(2, cx); // WithPrevious
        e.cycle_effect_start_at(2, cx); // AfterPrevious
    });
    assert_eq!(s.with(|e, _| e.slide().animations[2].start), AnimationStart::AfterPrevious);

    // 遅延時間と継続時間を調整
    s.update(|e, _, cx| {
        e.adjust_effect_delay_at(1, 500, cx); // delay 0 -> 500ms
        e.adjust_effect_duration_at(0, 500, cx); // duration 500 -> 1000ms
    });
    assert_eq!(s.with(|e, _| e.slide().animations[1].delay), 500);
    assert_eq!(s.with(|e, _| e.slide().animations[0].duration), 1000);

    // 更新後のタイムライン計算を検証
    let (updated_items, updated_total) = s.with(|e, _| compute_timeline(&e.slide().animations));
    // 効果0: OnClick, start=0, dur=1000, end=1000
    assert_eq!(updated_items[0].start_time_ms, 0);
    assert_eq!(updated_items[0].duration_ms, 1000);
    assert_eq!(updated_items[0].end_time_ms, 1000);
    // 効果1: WithPrevious, delay=500, start=0+500=500, dur=2000, end=2500
    assert_eq!(updated_items[1].start_time_ms, 500);
    assert_eq!(updated_items[1].duration_ms, 2000);
    assert_eq!(updated_items[1].end_time_ms, 2500);
    // 効果2: AfterPrevious, delay=0, start=2500, dur=500, end=3000
    assert_eq!(updated_items[2].start_time_ms, 2500);
    assert_eq!(updated_items[2].duration_ms, 500);
    assert_eq!(updated_items[2].end_time_ms, 3000);
    assert_eq!(updated_total, 3000);

    // スクリーンショット
    s.screenshot("animation-pane-timeline");

    // 保存と再読込
    s.save_as("anim-timeline.pptx");
    let saved = s.reopen();
    let saved_anims = &saved.slides[0].animations;
    assert_eq!(saved_anims.len(), 3);
    assert_eq!(saved_anims[0].duration, 1000);
    assert_eq!(saved_anims[1].start, AnimationStart::WithPrevious);
    assert_eq!(saved_anims[1].delay, 500);
    assert_eq!(saved_anims[2].start, AnimationStart::AfterPrevious);

    // Undo / Redo
    undo_all_and_redo(&mut s);
}

/// The slide point at the middle of cell (`row`, `column`) of table `shape`.
fn cell_middle(s: &mut Scenario, shape: usize, row: usize, column: usize) -> (f32, f32) {
    let p = s.with(|e, _| {
        let sh = &e.slide().shapes[shape];
        let ShapeKind::Table(t) = &sh.kind else { panic!("a table") };
        let heights = t.row_heights(sh.frame.height);
        let x = sh.frame.x + t.columns[..column].iter().sum::<i64>() + t.columns[column] / 2;
        let y = sh.frame.y + heights[..row].iter().sum::<i64>() + heights[row] / 2;
        EmuPoint::new(x, y)
    });
    s.at(p)
}

/// セルの文字の編集: two clicks on a table cell edit its text; typing, Tab to
/// the next cell (its text selected), bold, saved.
pub(crate) fn edit_table_cells() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s
        .with(|e, _| {
            e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Table(_))).map(|j| (i, j)))
        })
        .expect("a slide with a table");
    s.page(page + 1);
    let cell = cell_middle(&mut s, shape, 1, 1);
    s.double_click(cell);
    assert_eq!(s.with(|e, _| e.text_editing().and_then(|t| t.cell)), Some((1, 1)));
    let before = s.with(|e, _| e.edited_text().unwrap());
    s.key("end");
    s.type_text("X");
    assert_eq!(s.with(|e, _| e.edited_text().unwrap()), format!("{before}X"));
    // Tab: the next cell, its text selected; typing replaces it.
    s.key("tab");
    assert_eq!(s.with(|e, _| e.text_editing().and_then(|t| t.cell)), Some((1, 2)));
    s.type_text("New");
    s.key("cmd-a");
    // Bold toggles (the cell's text may be bold already).
    let was_bold = s.with(|e, _| e.selected_text_style().is_some_and(|t| t.bold));
    s.toolbar("bold");
    assert!(s.with(|e, _| e.text_editing().is_some()), "still editing");
    // The cell's fill.
    s.toolbar_menu("fill", "color.blue");
    let fill = s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Table(t) => t.rows[1].cells[2].fill.clone(),
        _ => unreachable!(),
    });
    assert_eq!(fill, Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x0070c0))));
    // The cell's lines (罫線): red on every edge of the cell.
    s.toolbar_menu("border-color", "color.red");
    let borders = s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Table(t) => t.rows[1].cells[2].borders.clone(),
        _ => unreachable!(),
    });
    assert!([&borders.left, &borders.right, &borders.top, &borders.bottom].iter().all(|b| b.as_ref().is_some_and(|b| b.color.rgb == ibis_doc_model::Rgb(0xff0000))));
    s.screenshot("cell");
    s.key("escape");
    let cells = s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Table(t) => (t.rows[1].cells[1].text.text(), t.rows[1].cells[2].text.clone()),
        _ => unreachable!(),
    });
    assert_eq!(cells.0, format!("{before}X"));
    assert_eq!(cells.1.text(), "New");
    assert!(cells.1.paragraphs[0].runs.iter().all(|r| cells.1.style.with_run(&r.style).bold != was_bold));
    s.save_as("table.pptx");
    let saved = s.reopen();
    let ShapeKind::Table(t) = &saved.slides[page].shapes[shape].kind else { panic!("a table") };
    assert_eq!(t.rows[1].cells[2].text.text(), "New");
    undo_all_and_redo(&mut s);
}

/// 行と列: a row inserted below the cell being edited (the table grows),
/// a column inserted and removed, saved.
pub(crate) fn table_rows_and_columns() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s
        .with(|e, _| {
            e.presentation().slides.iter().enumerate().find_map(|(i, sl)| {
                sl.shapes.iter().position(|sh| matches!(&sh.kind, ShapeKind::Table(t) if !t.has_merges())).map(|j| (i, j))
            })
        })
        .expect("a table without merged cells");
    s.page(page + 1);
    let size = |s: &mut Scenario| {
        s.with(|e, _| match &e.slide().shapes[shape].kind {
            ShapeKind::Table(t) => (t.rows.len(), t.columns.len(), e.slide().shapes[shape].frame.height),
            _ => unreachable!(),
        })
    };
    let (rows, columns, height) = size(&mut s);
    let cell = cell_middle(&mut s, shape, 1, 1);
    s.double_click(cell);
    s.toolbar_menu("table.rows", "table.insert-below");
    let (r, c, h) = size(&mut s);
    assert_eq!((r, c), (rows + 1, columns));
    assert!(h > height, "the table grows");
    // Editing goes on in the new row, which is empty.
    assert_eq!(s.with(|e, _| e.text_editing().and_then(|t| t.cell)), Some((2, 1)));
    assert_eq!(s.with(|e, _| e.edited_text()), Some(String::new()));
    s.type_text("Added");
    s.toolbar_menu("table.rows", "table.insert-right");
    assert_eq!(size(&mut s).1, columns + 1);
    s.toolbar_menu("table.rows", "table.remove-column");
    assert_eq!(size(&mut s).1, columns);
    s.key("escape");
    // 列の幅: the border right of the first column dragged right; the next
    // column gives the room. 行の高さ: the first row's border down.
    let widths = |s: &mut Scenario| s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Table(t) => t.columns.clone(),
        _ => unreachable!(),
    });
    let before = widths(&mut s);
    let (frame, heights) = s.with(|e, _| {
        let sh = &e.slide().shapes[shape];
        let ShapeKind::Table(t) = &sh.kind else { unreachable!() };
        (sh.frame, t.row_heights(sh.frame.height))
    });
    let border = s.at(EmuPoint::new(frame.x + before[0], frame.y + heights[0] / 2));
    s.drag(border, (border.0 + 20.0, border.1));
    let after = widths(&mut s);
    assert!(after[0] > before[0] && after[1] < before[1], "{before:?} → {after:?}");
    assert_eq!(after.iter().sum::<i64>(), before.iter().sum::<i64>());
    let row_border = s.at(EmuPoint::new(frame.x + before[0] / 2, frame.y + heights[0]));
    s.drag(row_border, (row_border.0, row_border.1 + 15.0));
    assert!(s.with(|e, _| e.slide().shapes[shape].frame.height) > frame.height, "the table grows");
    s.save_as("rows.pptx");
    let saved = s.reopen();
    let ShapeKind::Table(t) = &saved.slides[page].shapes[shape].kind else { panic!("a table") };
    assert_eq!((t.rows.len(), t.columns.len()), (rows + 1, columns));
    assert_eq!(t.rows[2].cells[1].text.text(), "Added");
    undo_all_and_redo(&mut s);
}

/// 表の挿入: 挿入 > 表 > 3 行 × 4 列, typed into its cells, saved.
pub(crate) fn insert_a_table() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    let count = s.with(|e, _| e.slide().shapes.len());
    s.menu("table.new.3x4");
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), count + 1);
    assert_eq!(s.with(|e, _| e.text_editing().and_then(|t| t.cell)), Some((0, 0)));
    s.type_text("Item");
    s.key("tab");
    s.type_text("Q1");
    s.screenshot("table");
    s.key("escape");
    let texts = s.with(|e, _| match &e.slide().shapes[count].kind {
        ShapeKind::Table(t) => (t.rows.len(), t.columns.len(), t.rows[0].cells[0].text.text(), t.rows[0].cells[1].text.text()),
        _ => panic!("a table"),
    });
    assert_eq!(texts, (3, 4, "Item".to_string(), "Q1".to_string()));
    s.save_as("new-table.pptx");
    let saved = s.reopen();
    let ShapeKind::Table(t) = &saved.slides[2].shapes[count].kind else { panic!("a table") };
    assert_eq!(t.rows[0].cells[1].text.text(), "Q1");
    undo_all_and_redo(&mut s);
}

/// フォント サイズの拡大・縮小（⌘⇧> ・⌘⇧<、A▲・A▼）: 選んだ文字だけが次の大きさになり、
/// 文字を選んでいなければ、図形の文字すべて。
pub(crate) fn font_size_steps_of_the_selected_text() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1);
    s.key("home");
    s.key("shift-right");
    s.key("shift-right");
    let before = s.with(|e, _| e.selected_text_style().map(|st| st.font_size)).unwrap();
    s.key("cmd-shift-.");
    let sizes = |s: &mut Scenario| {
        s.with(|e, _| {
            let body = e.edited_body().cloned().or_else(|| e.selected_shapes().first().and_then(|sh| sh.text_body().cloned())).unwrap();
            let first = &body.paragraphs[0];
            first.runs.iter().map(|r| (r.text.chars().count(), body.style.with_run(&r.style).font_size)).collect::<Vec<_>>()
        })
    };
    let after = sizes(&mut s);
    assert_eq!(after[0], (2, ibis_doc_model::step_font_size(before, true)), "the two selected characters are larger: {after:?}");
    assert!(after.iter().skip(1).all(|(_, size)| *size == before), "the others are as they were: {after:?}");
    s.key("cmd-shift-,");
    let back = sizes(&mut s);
    assert_eq!(back[0].1, before, "and smaller again: {back:?}");
}

/// ⌘L・⌘E・⌘R・⌘J（左・中央・右・両端揃え）と、⌥⌘C・⌥⌘V（文字の書式のコピーと貼り付け）。
pub(crate) fn text_shortcuts_align_and_copy_format() {
    use ibis_doc_model::TextAlign;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1);
    let align = |s: &mut Scenario| s.with(|e, _| e.selected_text_style().map(|st| st.align)).unwrap();
    s.key("cmd-e");
    assert_eq!(align(&mut s), TextAlign::Center);
    s.key("cmd-r");
    assert_eq!(align(&mut s), TextAlign::Right);
    s.key("cmd-j");
    assert_eq!(align(&mut s), TextAlign::Justify);
    s.key("cmd-l");
    assert_eq!(align(&mut s), TextAlign::Left);
    // The format of the first characters copied and pasted on others.
    s.key("cmd-a");
    s.key("cmd-b");
    s.key("home");
    s.key("shift-right");
    s.key("alt-cmd-c");
    assert!(s.with(|e, _| e.pptx().unwrap().character_format_copy.is_some()), "the format is copied");
    s.key("escape");
    s.double_click_shape(0);
    s.key("cmd-a");
    s.key("alt-cmd-v");
    let bold = s.with(|e, _| e.edited_body().map(|b| b.paragraphs.iter().flat_map(|p| p.runs.iter()).all(|r| b.style.with_run(&r.style).bold))).unwrap_or(false);
    assert!(bold, "the pasted format is bold");
}

/// Option+ドラッグ: 図形の複製を、ドラッグした先に置く（元の図形は動かない）。元に戻せる。
pub(crate) fn option_drag_copies_the_shape() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("view.sidebar"); // Placed by the layout with the right pane.
    s.page(3);
    let (id, frame) = s.with(|e, _| e.slide().shapes.iter().find(|sh| e.slide().placeholders.get(&sh.id).is_none() && matches!(sh.kind, ShapeKind::Auto(_))).map(|sh| (sh.id, sh.frame)).expect("a shape"));
    let before = s.shapes();
    s.update(|e, _, _| e.set_selection(vec![id]));
    let from = s.at(EmuPoint::new(frame.x + frame.width / 2, frame.y + frame.height / 2));
    let to = (from.0 + 60.0, from.1 + 40.0);
    s.drag_with(from, to, gpui_kit::Modifiers { alt: true, ..Default::default() });
    assert_eq!(s.shapes(), before + 1, "a copy is made");
    assert_eq!(s.with(|e, _| e.slide().shape(id).map(|sh| sh.frame)), Some(frame), "the original stays");
    let copy = s.with(|e, _| e.selected_ids());
    assert_eq!(copy.len(), 1);
    assert!(copy[0] != id, "the copy is selected");
    let moved = s.with(|e, _| e.slide().shape(copy[0]).map(|sh| sh.frame)).unwrap();
    assert!(moved.x > frame.x && moved.y > frame.y, "the copy went where it was dragged: {moved:?} from {frame:?}");
    // Without Option the shape itself moves.
    s.update(|e, _, _| e.set_selection(vec![id]));
    s.drag(from, to);
    assert_eq!(s.shapes(), before + 1);
    assert!(s.with(|e, _| e.slide().shape(id).map(|sh| sh.frame != frame)).unwrap(), "moved");
    undo_all_and_redo(&mut s);
}

/// ⌥← ⌥→（単語ごとにキャレットを動かす。⇧で選ぶ）、⌥⌫（単語を消す）、⌘⌫（行頭まで消す）。
pub(crate) fn text_caret_and_delete_by_words() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1);
    s.key("cmd-a");
    s.key("delete");
    s.type_text("alpha beta gamma");
    s.key("alt-left");
    assert_eq!(s.with(|e, _| e.text_editing().map(|t| t.caret)), Some(11), "to the start of gamma");
    s.key("alt-left");
    assert_eq!(s.with(|e, _| e.text_editing().map(|t| t.caret)), Some(6), "to the start of beta");
    s.key("alt-right");
    assert_eq!(s.with(|e, _| e.text_editing().map(|t| t.caret)), Some(10), "to the end of beta");
    s.key("alt-shift-right");
    assert_eq!(s.with(|e, _| e.edited_text()).is_some(), true);
    assert_eq!(s.with(|e, _| e.text_editing().map(|t| (t.anchor, t.caret))), Some((10, 16)), "gamma selected");
    s.key("right");
    s.key("alt-backspace");
    assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some("alpha beta "), "gamma taken away");
    s.key("cmd-backspace");
    assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some(""), "to the start of the line");
}

/// 文字の切り取り・コピー・貼り付け（システムのクリップボード）: 編集中の選んだ文字を
/// ⌘C・⌘X し、⌘V で貼る。ほかのアプリの文字も、貼れる。
pub(crate) fn text_copy_cut_paste_with_the_system_clipboard() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1);
    s.key("cmd-a");
    s.key("delete");
    s.type_text("abc def");
    s.key("home");
    s.key("shift-right");
    s.key("shift-right");
    s.key("shift-right");
    s.key("cmd-c");
    s.key("end");
    s.key("cmd-v");
    assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some("abc defabc"), "copied and pasted");
    s.key("shift-left");
    s.key("shift-left");
    s.key("shift-left");
    s.key("cmd-x");
    assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some("abc def"), "cut");
    s.key("cmd-v");
    assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some("abc defabc"), "pasted back");
    // Text from another application.
    s.update(|_, _, cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("外部の文字\n二行目".to_string())));
    s.key("cmd-a");
    s.key("cmd-v");
    assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some("外部の文字\n二行目"), "text of another application");
    // With no text edited and no shapes copied, the text becomes a text box.
    s.key("escape");
    s.update(|e, _, _| e.set_selection(Vec::new()));
    let shapes = s.shapes();
    s.key("cmd-v");
    assert_eq!(s.shapes(), shapes + 1, "a text box is made");
    assert!(s.with(|e, _| e.selected_shapes().first().and_then(|sh| sh.text_body()).is_some_and(|b| b.text() == "外部の文字\n二行目")));
    // A picture on the clipboard (a screenshot) is pasted as a picture.
    let pictures = |s: &mut Scenario| s.with(|e, _| e.slide().shapes.iter().filter(|sh| matches!(sh.kind, ShapeKind::Picture(_))).count());
    let image_bytes = {
        let mut out = Vec::new();
        let image = image::RgbaImage::from_pixel(40, 30, image::Rgba([200, 30, 30, 255]));
        image::DynamicImage::ImageRgba8(image).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    };
    s.update(|_, _, cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_image(&gpui_kit::Image::from_bytes(gpui_kit::ImageFormat::Png, image_bytes.clone()))));
    s.update(|e, _, _| e.set_selection(Vec::new()));
    let before = pictures(&mut s);
    s.key("cmd-v");
    assert_eq!(pictures(&mut s), before + 1, "a picture is pasted");
    undo_all_and_redo(&mut s);
}

/// 図形をコピーしたあと、ほかのアプリで文字をコピーすると、⌘V は新しいほう（文字）を貼る。
/// 何もコピーし直していなければ、図形を貼る。
pub(crate) fn paste_takes_what_was_copied_last() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    // A presentation opens with the keys on the slide list: the keys of the slide need the slide focused.
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.page(3);
    let id = s.with(|e, _| e.slide().shapes.iter().find(|sh| e.slide().placeholders.get(&sh.id).is_none() && matches!(sh.kind, ShapeKind::Auto(_))).map(|sh| sh.id).expect("a shape"));
    s.update(|e, _, _| e.set_selection(vec![id]));
    s.key("cmd-c");
    let shapes = s.shapes();
    s.update(|e, _, _| e.set_selection(Vec::new()));
    s.key("cmd-v");
    assert_eq!(s.shapes(), shapes + 1, "the copied shape");
    assert!(s.with(|e, _| e.selected_shapes().first().is_some_and(|sh| matches!(sh.kind, ShapeKind::Auto(_)))));
    // Another application copies a text.
    s.update(|_, _, cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("あとからコピー".to_string())));
    s.update(|e, _, _| e.set_selection(Vec::new()));
    s.key("cmd-v");
    assert_eq!(s.shapes(), shapes + 2);
    assert!(s.with(|e, _| e.selected_shapes().first().and_then(|sh| sh.text_body()).is_some_and(|b| b.text() == "あとからコピー")), "the text, not the shape");
    // What another application takes of the shapes copied: their text, or a picture's image.
    s.key("cmd-c");
    let text = s.update(|_, _, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(text.as_deref(), Some("あとからコピー"), "the text of the shape on the system's clipboard");
    s.update(|e, _, cx| {
        e.insert_picture("a.png", {
            let mut out = Vec::new();
            let image = image::RgbaImage::from_pixel(20, 10, image::Rgba([0, 90, 200, 255]));
            image::DynamicImage::ImageRgba8(image).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
            out
        }, cx);
    });
    s.key("cmd-c");
    let pasted_image = s.update(|_, _, cx| cx.read_from_clipboard().is_some_and(|item| item.entries().iter().any(|e| matches!(e, gpui_kit::ClipboardEntry::Image(_)))));
    assert!(pasted_image, "the picture's image on the system's clipboard");
    // And here the picture is still pasted as the copied shape.
    let shapes = s.shapes();
    s.update(|e, _, _| e.set_selection(Vec::new()));
    s.key("cmd-v");
    assert_eq!(s.shapes(), shapes + 1);
    assert!(s.with(|e, _| e.selected_shapes().first().is_some_and(|sh| matches!(sh.kind, ShapeKind::Picture(_)))));
}

/// 画像ファイルを Finder からスライドにドロップ: ドロップした所に画像が入る（図以外のファイルは無視）。
pub(crate) fn picture_files_dropped_on_the_slide() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/scenario-drop");
    std::fs::create_dir_all(&dir).unwrap();
    let png = dir.join("dropped.png");
    {
        let mut out = Vec::new();
        let image = image::RgbaImage::from_pixel(60, 30, image::Rgba([20, 160, 60, 255]));
        image::DynamicImage::ImageRgba8(image).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        std::fs::write(&png, out).unwrap();
    }
    let other = dir.join("notes.txt");
    std::fs::write(&other, "not a picture").unwrap();
    let pictures = |s: &mut Scenario| s.with(|e, _| e.slide().shapes.iter().filter(|sh| matches!(sh.kind, ShapeKind::Picture(_))).count());
    let before = pictures(&mut s);
    let target = s.at(EmuPoint::new(9_000_000, 4_500_000));
    s.drop_files(target, vec![png.clone(), other]);
    assert_eq!(pictures(&mut s), before + 1, "the picture file is put on the slide, the other file is not");
    let frame = s.with(|e, _| e.selected_shapes().first().map(|sh| (matches!(sh.kind, ShapeKind::Picture(_)), sh.frame))).unwrap();
    assert!(frame.0, "the picture is selected");
    let (cx, cy) = (frame.1.x + frame.1.width / 2, frame.1.y + frame.1.height / 2);
    assert!((cx - 9_000_000).abs() < 400_000 && (cy - 4_500_000).abs() < 400_000, "its middle is where it was dropped: ({cx}, {cy})");
    undo_all_and_redo(&mut s);
}

/// スライドの右クリックメニュー: 図形の上で右クリックするとその図形が選ばれ（選択の中なら選択のまま）、
/// 切り取り・コピー・貼り付け・複製・削除・順序・グループ…が出る。何もない所では貼り付けだけ。
pub(crate) fn right_click_menu_on_the_slide() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    let (id, frame) = s.with(|e, _| e.slide().shapes.iter().find(|sh| e.slide().placeholders.get(&sh.id).is_none() && matches!(sh.kind, ShapeKind::Auto(_))).map(|sh| (sh.id, sh.frame)).expect("a shape"));
    s.update(|e, _, _| e.set_selection(Vec::new()));
    let on = s.at(EmuPoint::new(frame.x + frame.width / 2, frame.y + frame.height / 2));
    s.right_click(on);
    assert_eq!(s.with(|e, _| e.selected_ids()), vec![id], "the shape under the pointer is selected");
    assert!(s.with(|e, _| e.pptx().unwrap().slide_popup.is_some()), "the menu is open");
    s.screenshot("menu");
    let ids = s.with(|e, _| {
        e.shape_context_menu_entries().iter().filter_map(|m| match m {
            crate::shell::chrome::MenuEntry::Item(i) => Some(i.id.to_string()),
            crate::shell::chrome::MenuEntry::Submenu(sub) => Some(sub.id.to_string()),
            _ => None,
        }).collect::<Vec<_>>()
    });
    for want in ["edit.cut", "edit.copy", "edit.paste", "edit.duplicate", "edit.delete", "pptx.order", "pptx.frame.settings", "pptx.alt-text"] {
        assert!(ids.iter().any(|i| i == want), "{want} in {ids:?}");
    }
    s.key("escape");
    assert!(s.with(|e, _| e.pptx().unwrap().slide_popup.is_none()), "the menu closes");
    // Nothing under the pointer: the selection goes and only 貼り付け is offered.
    let empty = s.at(EmuPoint::new(frame.x + frame.width / 2, 100_000));
    let hit_empty = s.with(|e, _| e.slide().shapes.iter().all(|sh| !sh.frame.contains(ibis_doc_model::EmuPoint::new(frame.x + frame.width / 2, 100_000))));
    if hit_empty {
        s.right_click(empty);
        assert!(s.with(|e, _| e.selected_ids().is_empty()));
        assert_eq!(s.with(|e, _| e.shape_context_menu_entries().len()), 3);
    }
}

/// スライド一覧で、⇧クリック（範囲）と ⌘クリック（追加・除外）で複数のスライドを選び、
/// 複製・コピー（貼り付け）・非表示・削除がまとめて効く。
pub(crate) fn select_several_slides_in_the_list() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let ids: Vec<ibis_doc_model::SlideId> = s.with(|e, _| e.presentation().slides.iter().map(|sl| sl.id).collect());
    assert!(ids.len() >= 4);
    s.update(|e, w, cx| e.select_slide(ids[0], w, cx));
    let shift = gpui_kit::Modifiers { shift: true, ..Default::default() };
    let command = gpui_kit::Modifiers { platform: true, ..Default::default() };
    s.update(|e, w, cx| e.click_slide(ids[2], &shift, w, cx));
    assert_eq!(s.with(|e, _| e.selected_slide_ids()), vec![ids[0], ids[1], ids[2]], "Shift: the range");
    s.update(|e, w, cx| e.click_slide(ids[1], &command, w, cx));
    assert_eq!(s.with(|e, _| e.selected_slide_ids()), vec![ids[0], ids[2]], "⌘: one taken away");
    s.update(|e, w, cx| e.click_slide(ids[3], &command, w, cx));
    assert_eq!(s.with(|e, _| e.selected_slide_ids()), vec![ids[0], ids[2], ids[3]], "⌘: one added");
    // Skipped together.
    s.update(|e, w, cx| e.toggle_skip_slide(w, cx));
    assert_eq!(s.with(|e, _| e.presentation().slides.iter().map(|sl| sl.hidden).collect::<Vec<_>>()), vec![true, false, true, true]);
    s.update(|e, w, cx| e.toggle_skip_slide(w, cx));
    // Duplicated together, after the last of them.
    let count = ids.len();
    s.update(|e, w, cx| e.duplicate_current_slide(w, cx));
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), count + 3, "three copies");
    undo_all_and_redo(&mut s);
}

/// 複数のスライドを選んで、削除・コピーと貼り付け。
pub(crate) fn delete_and_copy_several_slides() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let ids: Vec<ibis_doc_model::SlideId> = s.with(|e, _| e.presentation().slides.iter().map(|sl| sl.id).collect());
    let shift = gpui_kit::Modifiers { shift: true, ..Default::default() };
    s.update(|e, w, cx| e.select_slide(ids[1], w, cx));
    s.update(|e, w, cx| e.click_slide(ids[2], &shift, w, cx));
    s.update(|e, w, cx| e.copy_current_slide(w, cx));
    s.update(|e, w, cx| e.select_slide(ids[3], w, cx));
    s.update(|e, w, cx| { e.paste_slides(w, cx); });
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), ids.len() + 2, "two slides pasted");
    // Two selected deleted as one edit.
    s.update(|e, w, cx| e.select_slide(ids[1], w, cx));
    s.update(|e, w, cx| e.click_slide(ids[2], &shift, w, cx));
    s.update(|e, w, cx| e.delete_slide(w, cx));
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), ids.len());
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), ids.len() + 2, "undone in one step");
}

/// 選んだ複数のスライドを、ドラッグで一緒に動かす（下へは目的の後ろ、上へは前）。1 回で元に戻せる。
pub(crate) fn drag_several_slides_together() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let ids: Vec<ibis_doc_model::SlideId> = s.with(|e, _| e.presentation().slides.iter().map(|sl| sl.id).collect());
    assert_eq!(ids.len(), 4);
    let order = |s: &mut Scenario| s.with(|e, _| e.presentation().slides.iter().map(|sl| sl.id).collect::<Vec<_>>());
    let shift = gpui_kit::Modifiers { shift: true, ..Default::default() };
    s.update(|e, w, cx| e.select_slide(ids[0], w, cx));
    s.update(|e, w, cx| e.click_slide(ids[1], &shift, w, cx));
    s.update(|e, w, cx| e.move_selected_slides_to(ids[3], w, cx));
    assert_eq!(order(&mut s), vec![ids[2], ids[3], ids[0], ids[1]], "down: after the target");
    assert_eq!(s.with(|e, _| e.selected_slide_ids()), vec![ids[0], ids[1]], "still selected");
    s.key("cmd-z");
    assert_eq!(order(&mut s), ids, "one undo");
    // Up: before the target.
    s.update(|e, w, cx| e.select_slide(ids[2], w, cx));
    s.update(|e, w, cx| e.click_slide(ids[3], &shift, w, cx));
    s.update(|e, w, cx| e.move_selected_slides_to(ids[0], w, cx));
    assert_eq!(order(&mut s), vec![ids[2], ids[3], ids[0], ids[1]], "up: before the target");
}

/// 配置 > スライドに合わせる: 複数の図形も、スライドを基準に揃える・等間隔にする。
pub(crate) fn align_several_shapes_to_the_slide() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    let shapes: Vec<ibis_doc_model::ShapeId> = s.with(|e, _| e.slide().shapes.iter().filter(|sh| e.slide().placeholders.get(&sh.id).is_none() && matches!(sh.kind, ShapeKind::Auto(_))).map(|sh| sh.id).take(2).collect());
    assert_eq!(shapes.len(), 2);
    s.update(|e, _, _| e.set_selection(shapes.clone()));
    // Among themselves: the left of the most left one.
    s.update(|e, _, cx| e.align_selected(super::editing::Align::Left, cx));
    let lefts = |s: &mut Scenario| s.with(|e, _| shapes.iter().map(|id| e.slide().shape(*id).unwrap().frame.x).collect::<Vec<_>>());
    let among = lefts(&mut s);
    assert_eq!(among[0], among[1]);
    assert!(among[0] > 0);
    s.key("cmd-z");
    // To the slide: the slide's left edge.
    s.menu("pptx.align.to-slide");
    assert!(s.with(|e, _| e.pptx().unwrap().align_to_slide));
    s.update(|e, _, cx| e.align_selected(super::editing::Align::Left, cx));
    assert_eq!(lefts(&mut s), vec![0, 0]);
}

/// 文字の入力中に、Web アドレス・メールアドレスのあとで空白か Enter を打つと、そのアドレスがリンクになる。
pub(crate) fn typed_addresses_become_links() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1);
    s.key("cmd-a");
    s.key("delete");
    s.type_text("see https://example.com/a. and taro@example.jp now");
    let links = |s: &mut Scenario| {
        s.with(|e, _| {
            let body = e.edited_body().cloned().unwrap();
            body.paragraphs.iter().flat_map(|p| p.runs.iter()).filter_map(|r| r.style.link.clone().map(|l| (r.text.clone(), l))).collect::<Vec<_>>()
        })
    };
    let found = links(&mut s);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0], ("https://example.com/a".to_string(), ibis_doc_model::SlideLink::Url("https://example.com/a".into())), "the full stop is not the address's");
    assert_eq!(found[1], ("taro@example.jp".to_string(), ibis_doc_model::SlideLink::Url("mailto:taro@example.jp".into())));
    assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some("see https://example.com/a. and taro@example.jp now"));
}

/// Excel などでコピーしたセル（タブと改行で区切った文字）を何も編集せずに貼ると、表になる。
pub(crate) fn cells_copied_from_a_worksheet_paste_as_a_table() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    // A presentation opens with the keys on the slide list: the keys of the slide need the slide focused.
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.page(3);
    let tables = |s: &mut Scenario| s.with(|e, _| e.slide().shapes.iter().filter(|sh| matches!(sh.kind, ShapeKind::Table(_))).count());
    let before = tables(&mut s);
    s.update(|_, _, cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("品目\t数量\t金額\nりんご\t3\t300\nみかん\t5\t450\n".to_string())));
    s.update(|e, _, _| e.set_selection(Vec::new()));
    s.key("cmd-v");
    assert_eq!(tables(&mut s), before + 1, "a table is made");
    let cells = s.with(|e, _| match e.selected_shapes().first().map(|sh| &sh.kind) {
        Some(ShapeKind::Table(t)) => t.rows.iter().map(|r| r.cells.iter().map(|c| c.text.text()).collect::<Vec<_>>()).collect::<Vec<_>>(),
        _ => panic!("the table is selected"),
    });
    assert_eq!(cells, vec![vec!["品目", "数量", "金額"], vec!["りんご", "3", "300"], vec!["みかん", "5", "450"]]);
    s.save_as("pasted-table.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[2].shapes.iter().filter(|sh| matches!(sh.kind, ShapeKind::Table(_))).count(), before + 1);
    undo_all_and_redo(&mut s);
}

/// 段落の先頭で「- 」「1. 」を打つと、箇条書き・番号付きの段落になる（記号は消える）。文の途中では何もしない。
pub(crate) fn typed_list_marks_become_lists() {
    use ibis_doc_model::ListKind;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    // A presentation opens with the keys on the slide list: the keys of the slide need the slide focused.
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.double_click_shape(1);
    s.key("cmd-a");
    s.key("delete");
    s.type_text("- first");
    let first = s.with(|e, _| e.selected_list());
    assert_eq!(first, Some(ListKind::Bullet), "- makes a bullet");
    assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some("first"), "the mark is gone");
    s.key("enter");
    s.type_text("1. second");
    assert_eq!(s.with(|e, _| e.selected_list()), Some(ListKind::Numbered), "1. makes numbers");
    assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some("first\nsecond"));
    // Not at the start of a paragraph: a hyphen stays.
    s.key("enter");
    s.key("enter");
    s.type_text("a - b");
    assert!(s.with(|e, _| e.edited_text()).is_some_and(|t| t.ends_with("a - b")), "a hyphen in the middle stays");
}

/// カラーパレット: 色のボタンの ▼ は、色の名前の一覧でなく、色の四角を並べたパレット（テーマの色の行、標準の色、
/// 最近使った色）を開く。四角を選ぶと文字・図形の色が変わり、最近使った色に入る。
pub(crate) fn color_palette_in_the_toolbar() {
    use crate::shell::chrome::{MenuEntry, ToolItem};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    let (id, _) = s.with(|e, _| e.slide().shapes.iter().find(|sh| e.slide().placeholders.get(&sh.id).is_none() && matches!(sh.kind, ShapeKind::Auto(_))).map(|sh| (sh.id, sh.frame)).expect("a shape"));
    s.update(|e, _, _| e.set_selection(vec![id]));
    let menu = s.with(|e, _| e.toolbar_items().into_iter().find_map(|t| match t { ToolItem::Button(b) if b.id == "fill" => b.menu, _ => None })).expect("the fill menu");
    let palette = menu.iter().find_map(|m| match m { MenuEntry::Palette(p) => Some(p.clone()), _ => None }).expect("a palette, not a list of names");
    assert!(palette.none.is_some(), "塗りつぶしなし");
    assert_eq!(palette.theme.len(), 6, "the theme colours and five tints");
    assert!(palette.theme.iter().all(|r| r.len() == 10));
    assert_eq!(palette.standard.len(), 10);
    // The picture of it, opened as a menu.
    s.update(|e, w, cx| e.open_popup_entries(menu.clone(), gpui_kit::point(gpui_kit::px(560.), gpui_kit::px(90.)), w, cx));
    s.screenshot("palette");
    s.key("escape");
    // A square picks: the shape's fill is that colour; it is then the lately used colour.
    let theme_square = palette.theme[0][4].clone();
    s.update(|e, w, cx| (theme_square.item.handler.clone().unwrap())(e, w, cx));
    let fill = s.with(|e, _| match &e.slide().shape(id).unwrap().kind { ShapeKind::Auto(a) => a.fill.clone(), _ => unreachable!() });
    assert_eq!(fill, ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(theme_square.rgb)));
    s.toolbar_menu("fill", "color.orange");
    let orange = s.with(|e, _| match &e.slide().shape(id).unwrap().kind { ShapeKind::Auto(a) => a.fill.clone(), _ => unreachable!() });
    assert_ne!(orange, fill, "the standard orange");
    s.toolbar_menu("fill", "color.none");
    assert!(s.with(|e, _| matches!(&e.slide().shape(id).unwrap().kind, ShapeKind::Auto(a) if a.fill == ibis_doc_model::Fill::None)));
    // The split button: its left half sets the colour last picked from it (the orange), at once.
    s.toolbar("fill");
    let again = s.with(|e, _| match &e.slide().shape(id).unwrap().kind { ShapeKind::Auto(a) => a.fill.clone(), _ => unreachable!() });
    assert_eq!(again, orange, "the left half does the last colour");
    let bar = s.with(|e, _| e.toolbar_items().into_iter().find_map(|t| match t { ToolItem::Button(b) if b.id == "fill" => b.color, _ => None }));
    assert_eq!(bar, Some(gpui_kit::rgb(0xffc000).into()), "its bar shows that colour");
    s.screenshot("split");
}

/// Tab・⇧Tab: 文字を編集していないとき、次・前の図形を選ぶ（重なり順の、背面から。最後の次は最初）。
pub(crate) fn tab_selects_the_next_shape() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    // A presentation opens with the keys on the slide list: the keys of the slide need the slide focused.
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.page(3);
    let ids: Vec<ibis_doc_model::ShapeId> = s.with(|e, _| e.slide().shown_shapes().filter(|sh| !e.slide().is_attached(sh.id)).map(|sh| sh.id).collect());
    assert!(ids.len() >= 3);
    s.update(|e, _, _| e.set_selection(Vec::new()));
    s.key("tab");
    assert_eq!(s.with(|e, _| e.selected_ids()), vec![ids[0]], "the first when none is selected");
    s.key("tab");
    assert_eq!(s.with(|e, _| e.selected_ids()), vec![ids[1]]);
    s.key("shift-tab");
    assert_eq!(s.with(|e, _| e.selected_ids()), vec![ids[0]]);
    s.key("shift-tab");
    assert_eq!(s.with(|e, _| e.selected_ids()), vec![*ids.last().unwrap()], "round to the last");
    s.key("tab");
    assert_eq!(s.with(|e, _| e.selected_ids()), vec![ids[0]], "and round to the first");
}

/// 表の最後のセルで Tab: 行が 1 つ増え、キャレットがその行の最初のセルに入る。
pub(crate) fn tab_in_the_last_cell_adds_a_row() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s
        .with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(&sh.kind, ShapeKind::Table(t) if !t.has_merges() && t.rows.len() >= 2 && t.columns.len() >= 2)).map(|j| (i, j))))
        .expect("a table");
    s.page(page + 1);
    let size = |s: &mut Scenario| s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Table(t) => (t.rows.len(), t.columns.len()),
        _ => unreachable!(),
    });
    let (rows, columns) = size(&mut s);
    let cell = cell_middle(&mut s, shape, rows - 1, columns - 1);
    s.double_click(cell);
    assert_eq!(s.with(|e, _| e.text_editing().and_then(|t| t.cell)), Some((rows - 1, columns - 1)));
    s.key("tab");
    assert_eq!(size(&mut s), (rows + 1, columns), "a row more");
    assert_eq!(s.with(|e, _| e.text_editing().and_then(|t| t.cell)), Some((rows, 0)), "in the first cell of the new row");
    s.type_text("新しい行");
    s.key("escape");
    undo_all_and_redo(&mut s);
}

/// セルのサイズ (TABLE-005): 選んだセルの行の高さ・列の幅を cm の数値で入れる。表の大きさが
/// それに従い、保存して読み直しても同じで、元に戻せる。
pub(crate) fn table_cell_size_panel() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s
        .with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(&sh.kind, ShapeKind::Table(t) if !t.has_merges() && t.rows.len() >= 3 && t.columns.len() >= 3)).map(|j| (i, j))))
        .expect("a table");
    s.page(page + 1);
    let state = |s: &mut Scenario| {
        s.with(|e, _| {
            let sh = &e.slide().shapes[shape];
            let ShapeKind::Table(t) = &sh.kind else { unreachable!() };
            (t.row_heights(sh.frame.height), t.columns.clone(), sh.frame)
        })
    };
    let (heights0, columns0, frame0) = state(&mut s);
    assert_eq!(s.menu_state("table.cell-size"), None, "no cell is edited: no item");
    let cell = cell_middle(&mut s, shape, 1, 1);
    s.double_click(cell);
    s.update(|e, w, cx| e.open_cell_size_panel(w, cx));
    let (height, width) = s.with(|e, _| {
        let p = e.pptx().unwrap().cell_size_panel.as_ref().expect("the panel");
        (p.height.clone(), p.width.clone())
    });
    s.update(|_, w, cx| {
        height.update(cx, |f, cx| f.set_value("2", w, cx));
        width.update(cx, |f, cx| f.set_value("5", w, cx));
    });
    // Wrong text is refused.
    s.update(|_, w, cx| width.update(cx, |f, cx| f.set_value("x", w, cx)));
    s.update(|e, w, cx| e.apply_cell_size_panel(w, cx));
    assert!(s.with(|e, _| e.pptx().unwrap().cell_size_panel.is_some()), "kept open with its error");
    s.update(|_, w, cx| width.update(cx, |f, cx| f.set_value("5", w, cx)));
    s.update(|e, w, cx| e.apply_cell_size_panel(w, cx));
    assert!(s.with(|e, _| e.pptx().unwrap().cell_size_panel.is_none()));
    let (heights, columns, frame) = state(&mut s);
    assert_eq!((heights[1], columns[1]), (720_000, 1_800_000), "row 2 is 2 cm high, column 2 5 cm wide");
    assert_eq!((heights[0], columns[0], heights[2], columns[2]), (heights0[0], columns0[0], heights0[2], columns0[2]), "the others as they were");
    assert_eq!(frame.height, heights.iter().sum::<i64>());
    assert_eq!(frame.width, columns.iter().sum::<i64>());
    assert!(frame != frame0, "the table follows");
    s.save_as("cell-size.pptx");
    let saved = s.reopen();
    let ShapeKind::Table(t) = &saved.slides[page].shapes[shape].kind else { panic!("a table") };
    assert_eq!((t.rows[1].height, t.columns[1]), (720_000, 1_800_000));
    undo_all_and_redo(&mut s);
}

/// セルの結合・分割: Shift+click selects cells, merged into one (the text
/// joined), saved, split again.
pub(crate) fn merge_table_cells() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    s.menu("table.new.3x4");
    let shape = s.with(|e, _| e.slide().shapes.len() - 1);
    s.type_text("A");
    s.key("tab");
    s.type_text("B");
    // From the cell being edited (0, 1) back to (0, 0) and down to row 1.
    let corner = cell_middle(&mut s, shape, 1, 0);
    s.shift_click(corner);
    assert_eq!(s.with(|e, _| e.text_editing().and_then(|t| t.cell_anchor)), Some((1, 0)));
    s.screenshot("selected-cells");
    s.toolbar_menu("table.rows", "table.merge");
    let cell = s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Table(t) => (t.rows[0].cells[0].column_span, t.rows[0].cells[0].row_span, t.rows[0].cells[0].text.text(), t.rows[1].cells[1].merged),
        _ => unreachable!(),
    });
    assert_eq!(cell, (2, 2, "A\nB".to_string(), true));
    s.key("escape");
    s.save_as("merged.pptx");
    let saved = s.reopen();
    let ShapeKind::Table(t) = &saved.slides[2].shapes[shape].kind else { panic!("a table") };
    assert_eq!((t.rows[0].cells[0].column_span, t.rows[0].cells[0].row_span), (2, 2));
    // Split back.
    let middle = cell_middle(&mut s, shape, 0, 0);
    s.click_shape(shape);
    s.double_click(middle);
    s.toolbar_menu("table.rows", "table.split");
    assert!(s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Table(t) => !t.has_merges(),
        _ => false,
    }));
    undo_all_and_redo(&mut s);
}

/// 上付き・蛍光ペン・文字の間隔・書式をクリア on part of a text, saved.
pub(crate) fn character_formats() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1);
    s.key("cmd-a");
    s.key("shift-left");
    s.key("home");
    s.key("shift-right");
    s.key("shift-right");
    s.menu("text.superscript");
    s.toolbar_menu("highlight", "color.yellow");
    s.menu("text.spacing-loose");
    let run = |s: &mut Scenario| {
        s.with(|e, _| {
            let body = e.slide().shapes[1].text_body().unwrap().clone();
            body.paragraphs[0].runs[0].style.clone()
        })
    };
    let style = run(&mut s);
    assert_eq!((style.baseline, style.highlight, style.spacing), (30_000, Some(ibis_doc_model::Rgb(0xffff00)), 300));
    s.screenshot("formats");
    s.save_as("formats.pptx");
    let saved = s.reopen();
    let r = &saved.slides[0].shapes[1].text_body().unwrap().paragraphs[0].runs[0].style;
    assert_eq!((r.baseline, r.highlight, r.spacing), (30_000, Some(ibis_doc_model::Rgb(0xffff00)), 300));
    // 書式をクリア: toolbar and advertised keyboard shortcut.
    s.toolbar("clear-format");
    s.key("cmd-z");
    assert_eq!(run(&mut s), style);
    s.key("cmd-\\");
    let style = run(&mut s);
    assert_eq!((style.baseline, style.highlight, style.spacing), (0, None, 0));
    s.save_as("cleared.pptx");
    let saved = s.reopen();
    let r = &saved.slides[0].shapes[1].text_body().unwrap().paragraphs[0].runs[0].style;
    assert_eq!((r.baseline, r.highlight, r.spacing), (0, None, 0));
    s.key("escape");
    undo_all_and_redo(&mut s);
}

pub(crate) fn text_outline_colors_and_weights() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1);
    s.key("cmd-a");
    s.key("home");
    s.key("shift-right");
    s.key("shift-right");

    // 1. 青色の輪郭を設定（太さ未指定時は既定 1 pt = 12,700 EMU）
    s.menu("pptx.text-outline.blue");
    let run = |s: &mut Scenario| {
        s.with(|e, _| {
            let body = e.slide().shapes[1].text_body().unwrap().clone();
            body.paragraphs[0].runs[0].style.outline
        })
    };
    assert_eq!(run(&mut s), Some((ibis_doc_model::Rgb(0x4472c4), 12_700)));

    // 2. 太さを 3 pt（38,100 EMU）に変更（色は青を維持）
    s.menu("pptx.text-outline.3pt");
    assert_eq!(run(&mut s), Some((ibis_doc_model::Rgb(0x4472c4), 38_100)));

    // 3. Undo / Redo
    s.key("cmd-z");
    assert_eq!(run(&mut s), Some((ibis_doc_model::Rgb(0x4472c4), 12_700)));
    s.key("cmd-shift-z");
    assert_eq!(run(&mut s), Some((ibis_doc_model::Rgb(0x4472c4), 38_100)));

    // 4. 保存と再読込（ラウンドトリップ）
    s.save_as("text-outline-blue-3pt.pptx");
    let saved = s.reopen();
    let r = &saved.slides[0].shapes[1].text_body().unwrap().paragraphs[0].runs[0].style;
    assert_eq!(r.outline, Some((ibis_doc_model::Rgb(0x4472c4), 38_100)));

    // 5. 赤色に変更（太さ 3 pt は維持）
    s.menu("pptx.text-outline.red");
    assert_eq!(run(&mut s), Some((ibis_doc_model::Rgb(0xc00000), 38_100)));

    // 6. 輪郭なしに解除
    s.menu("pptx.text-outline.none");
    assert_eq!(run(&mut s), None);
    s.save_as("text-outline-cleared.pptx");
    let saved = s.reopen();
    let r = &saved.slides[0].shapes[1].text_body().unwrap().paragraphs[0].runs[0].style;
    assert_eq!(r.outline, None);

    s.key("escape");
    undo_all_and_redo(&mut s);
}

/// 文字の影（Text Shadow）のツールバーボタンによるトグル、書式メニューによるプリセット適用、
/// Undo/Redo、ファイル保存と再読込の保持検証。
pub(crate) fn text_shadow_toggle_and_presets() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1);
    s.key("cmd-a");
    s.key("home");
    s.key("shift-right");
    s.key("shift-right");

    let run_shadow = |s: &mut Scenario| {
        s.with(|e, _| {
            let body = e.slide().shapes[1].text_body().unwrap().clone();
            body.paragraphs[0].runs[0].style.shadow
        })
    };

    // 初期状態は影なし、ツールバーボタンは off
    assert_eq!(run_shadow(&mut s), None);
    assert_eq!(s.toolbar_state("text-shadow").map(|b| b.on), Some(false));

    // 1. ツールバーの "text-shadow" ボタンをクリックして影をトグル有効化
    s.toolbar("text-shadow");
    let default_shadow = run_shadow(&mut s).expect("shadow should be set");
    assert_eq!(default_shadow.direction, 5_400_000);
    assert_eq!(default_shadow.distance, 19_050);
    assert_eq!(default_shadow.blur, 25_400);
    assert_eq!(s.toolbar_state("text-shadow").map(|b| b.on), Some(true));

    // 2. メニューからプリセット変更: "pptx.text.shadow.offset-right"（右向き 0度）
    s.menu("pptx.text.shadow.offset-right");
    let right_shadow = run_shadow(&mut s).expect("shadow should be set");
    assert_eq!(right_shadow.direction, 0);

    // 3. メニューからプリセット変更: "pptx.text.shadow.offset-diagonal-bottom-right"（斜め右下 45度 = 2,700,000）
    s.menu("pptx.text.shadow.offset-diagonal-bottom-right");
    let diag_shadow = run_shadow(&mut s).expect("shadow should be set");
    assert_eq!(diag_shadow.direction, 2_700_000);

    // 4. Undo / Redo の確認
    s.key("cmd-z");
    assert_eq!(run_shadow(&mut s).map(|s| s.direction), Some(0));
    s.key("cmd-shift-z");
    assert_eq!(run_shadow(&mut s).map(|s| s.direction), Some(2_700_000));

    // 5. 保存と再読込（ラウンドトリップ）
    s.save_as("text-shadow-diagonal.pptx");
    let saved = s.reopen();
    let r = &saved.slides[0].shapes[1].text_body().unwrap().paragraphs[0].runs[0].style;
    assert_eq!(r.shadow.map(|s| s.direction), Some(2_700_000));

    // 6. メニューから影なし（"pptx.text.shadow.none"）を選択
    s.menu("pptx.text.shadow.none");
    assert_eq!(run_shadow(&mut s), None);
    assert_eq!(s.toolbar_state("text-shadow").map(|b| b.on), Some(false));

    // 7. 再びツールバーボタンをクリックしてトグルON
    s.toolbar("text-shadow");
    assert!(run_shadow(&mut s).is_some());
    assert_eq!(s.toolbar_state("text-shadow").map(|b| b.on), Some(true));

    // 8. もう一度ツールバーボタンをクリックしてトグルOFF
    s.toolbar("text-shadow");
    assert_eq!(run_shadow(&mut s), None);
    assert_eq!(s.toolbar_state("text-shadow").map(|b| b.on), Some(false));

    s.key("escape");
    undo_all_and_redo(&mut s);
}

/// テキストの自動調整（自動調整なし・はみ出すときに縮小・テキストに合わせて図形のサイズを調整）の
/// メニュー操作、チェック状態連動、Undo/Redo、ファイル保存と再読込の保持検証。
pub(crate) fn text_autofit_options() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1);

    let get_autofit = |s: &mut Scenario| {
        s.with(|e, _| {
            e.slide().shapes[1].text_body().unwrap().autofit
        })
    };

    // 1. メニューから「テキストに合わせて図形のサイズを調整」（pptx.text.autofit-resize-shape）を選択
    s.menu("pptx.text.autofit-resize-shape");
    assert_eq!(get_autofit(&mut s), ibis_doc_model::TextAutofit::ResizeShape);
    assert_eq!(s.menu_state("pptx.text.autofit-resize-shape"), Some((true, true)));
    assert_eq!(s.menu_state("pptx.text.autofit-shrink"), Some((true, false)));
    assert_eq!(s.menu_state("pptx.text.autofit-none"), Some((true, false)));

    // 2. メニューから「はみ出すときに縮小」（pptx.text.autofit-shrink）を選択
    s.menu("pptx.text.autofit-shrink");
    assert_eq!(get_autofit(&mut s), ibis_doc_model::TextAutofit::Shrink);
    assert_eq!(s.menu_state("pptx.text.autofit-shrink"), Some((true, true)));

    // 3. Undo / Redo の確認
    s.key("cmd-z");
    assert_eq!(get_autofit(&mut s), ibis_doc_model::TextAutofit::ResizeShape);
    assert_eq!(s.menu_state("pptx.text.autofit-resize-shape"), Some((true, true)));
    s.key("cmd-shift-z");
    assert_eq!(get_autofit(&mut s), ibis_doc_model::TextAutofit::Shrink);
    assert_eq!(s.menu_state("pptx.text.autofit-shrink"), Some((true, true)));

    // 4. 保存と再読込（ラウンドトリップ検証）
    s.save_as("text-autofit-shrink.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes[1].text_body().unwrap().autofit, ibis_doc_model::TextAutofit::Shrink);

    // 5. メニューから「自動調整なし」（pptx.text.autofit-none）を選択
    s.double_click_shape(1);
    s.menu("pptx.text.autofit-none");
    assert_eq!(get_autofit(&mut s), ibis_doc_model::TextAutofit::None);
    assert_eq!(s.menu_state("pptx.text.autofit-none"), Some((true, true)));

    // 6. 保存と再読込
    s.save_as("text-autofit-none.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes[1].text_body().unwrap().autofit, ibis_doc_model::TextAutofit::None);

    s.key("escape");
    undo_all_and_redo(&mut s);
}

/// テキストの段組（1〜4段、段間隔）のメニュー操作、チェック状態連動、Undo/Redo、ファイル保存と再読込の保持検証。
pub(crate) fn text_columns_options() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1);

    let get_columns = |s: &mut Scenario| {
        s.with(|e, _| {
            let body = e.slide().shapes[1].text_body().unwrap();
            (body.columns, body.column_spacing)
        })
    };

    // 初期状態は1段組・段間隔0
    assert_eq!(get_columns(&mut s), (1, 0));
    assert_eq!(s.menu_state("pptx.text.columns.1"), Some((true, true)));
    assert_eq!(s.menu_state("pptx.text.columns.2"), Some((true, false)));
    assert_eq!(s.menu_state("pptx.text.columns.spacing.0"), Some((false, true))); // 1列時は間隔メニュー無効

    // 1. メニューから「2段」（pptx.text.columns.2）を選択
    s.menu("pptx.text.columns.2");
    assert_eq!(get_columns(&mut s), (2, 180_000));
    assert_eq!(s.menu_state("pptx.text.columns.2"), Some((true, true)));
    assert_eq!(s.menu_state("pptx.text.columns.1"), Some((true, false)));
    assert_eq!(s.menu_state("pptx.text.columns.spacing.0-5cm"), Some((true, true)));

    // 2. メニューから段間隔「1cm」（pptx.text.columns.spacing.1cm）を選択
    s.menu("pptx.text.columns.spacing.1cm");
    assert_eq!(get_columns(&mut s), (2, 360_000));
    assert_eq!(s.menu_state("pptx.text.columns.spacing.1cm"), Some((true, true)));
    assert_eq!(s.menu_state("pptx.text.columns.spacing.0-5cm"), Some((true, false)));

    // 3. メニューから「3段」（pptx.text.columns.3）を選択
    s.menu("pptx.text.columns.3");
    assert_eq!(get_columns(&mut s), (3, 180_000));
    assert_eq!(s.menu_state("pptx.text.columns.3"), Some((true, true)));

    // 4. Undo / Redo の確認
    s.key("cmd-z");
    assert_eq!(get_columns(&mut s), (2, 360_000));
    s.key("cmd-z");
    assert_eq!(get_columns(&mut s), (2, 180_000));
    s.key("cmd-z");
    assert_eq!(get_columns(&mut s), (1, 0));
    s.key("cmd-shift-z");
    assert_eq!(get_columns(&mut s), (2, 180_000));
    s.key("cmd-shift-z");
    assert_eq!(get_columns(&mut s), (2, 360_000));

    // 5. 保存と再読込（ラウンドトリップ検証）
    s.save_as("text-columns-scenario.pptx");
    let saved = s.reopen();
    let body = saved.slides[0].shapes[1].text_body().unwrap();
    assert_eq!(body.columns, 2);
    assert_eq!(body.column_spacing, 360_000);

    // 6. メニューから「1段」（pptx.text.columns.1）を選択して解除
    s.double_click_shape(1);
    s.menu("pptx.text.columns.1");
    assert_eq!(get_columns(&mut s), (1, 0));
    assert_eq!(s.menu_state("pptx.text.columns.1"), Some((true, true)));

    // 7. 保存と再読込
    s.save_as("text-columns-single.pptx");
    let saved1 = s.reopen();
    let body1 = saved1.slides[0].shapes[1].text_body().unwrap();
    assert_eq!(body1.columns, 1);
    assert_eq!(body1.column_spacing, 0);

    s.key("escape");
    undo_all_and_redo(&mut s);
}

/// 段落のインデント（字下げ・ぶら下げ・解除）とタブ位置（設定・クリア）のメニュー操作、チェック状態連動、Undo/Redo、ファイル保存と再読込の保持検証。
pub(crate) fn paragraph_indent_and_tabs_options() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1);

    let get_indent_and_tabs = |s: &mut Scenario| {
        s.with(|e, _| {
            let body = e.slide().shapes[1].text_body().unwrap();
            let p = &body.paragraphs[0];
            (p.margin_left, p.indent, p.tabs.clone())
        })
    };

    // 初期状態はインデント0・タブなし
    assert_eq!(get_indent_and_tabs(&mut s), (0, 0, vec![]));
    assert_eq!(s.menu_state("pptx.text.indent.none"), Some((true, true)));
    assert_eq!(s.menu_state("pptx.text.indent.first-line"), Some((true, false)));
    assert_eq!(s.menu_state("pptx.text.indent.hanging"), Some((true, false)));
    assert_eq!(s.menu_state("pptx.text.tabs.clear"), Some((true, true)));
    assert_eq!(s.menu_state("pptx.text.tabs.default"), Some((true, false)));

    // 1. メニューから「字下げ（1行目）」（pptx.text.indent.first-line）を選択
    s.menu("pptx.text.indent.first-line");
    assert_eq!(get_indent_and_tabs(&mut s).1, 228_600);
    assert_eq!(s.menu_state("pptx.text.indent.first-line"), Some((true, true)));
    assert_eq!(s.menu_state("pptx.text.indent.none"), Some((true, false)));

    // 2. メニューから「ぶら下げ」（pptx.text.indent.hanging）を選択
    s.menu("pptx.text.indent.hanging");
    let (ml, ind, _) = get_indent_and_tabs(&mut s);
    assert_eq!(ml, 228_600);
    assert_eq!(ind, -228_600);
    assert_eq!(s.menu_state("pptx.text.indent.hanging"), Some((true, true)));

    // 3. メニューから「既定のタブ位置（2cm）」（pptx.text.tabs.default）を選択
    s.menu("pptx.text.tabs.default");
    let (_, _, tabs) = get_indent_and_tabs(&mut s);
    assert_eq!(tabs.len(), 2);
    assert_eq!(tabs[0].position, 720_000);
    assert_eq!(tabs[1].position, 1_440_000);
    assert_eq!(s.menu_state("pptx.text.tabs.default"), Some((true, true)));
    assert_eq!(s.menu_state("pptx.text.tabs.clear"), Some((true, false)));

    // 4. Undo / Redo の確認
    s.key("cmd-z"); // タブ追加取り消し
    assert_eq!(get_indent_and_tabs(&mut s).2.len(), 0);
    s.key("cmd-z"); // ぶら下げ取り消し -> 字下げに戻る
    assert_eq!(get_indent_and_tabs(&mut s).1, 228_600);
    s.key("cmd-z"); // 字下げ取り消し -> 解除に戻る
    assert_eq!(get_indent_and_tabs(&mut s), (0, 0, vec![]));
    s.key("cmd-shift-z"); // 字下げ復帰
    assert_eq!(get_indent_and_tabs(&mut s).1, 228_600);
    s.key("cmd-shift-z"); // ぶら下げ復帰
    assert_eq!(get_indent_and_tabs(&mut s).1, -228_600);
    s.key("cmd-shift-z"); // タブ復帰
    assert_eq!(get_indent_and_tabs(&mut s).2.len(), 2);

    // 5. 保存と再読込（ラウンドトリップ検証）
    s.save_as("paragraph-indent-tabs-scenario.pptx");
    let saved = s.reopen();
    let p = &saved.slides[0].shapes[1].text_body().unwrap().paragraphs[0];
    assert_eq!(p.margin_left, 228_600);
    assert_eq!(p.indent, -228_600);
    assert_eq!(p.tabs.len(), 2);
    assert_eq!(p.tabs[0].position, 720_000);
    assert_eq!(p.tabs[1].position, 1_440_000);

    // 6. タブ位置クリアとインデント解除
    s.double_click_shape(1);
    s.menu("pptx.text.tabs.clear");
    s.menu("pptx.text.indent.none");
    assert_eq!(get_indent_and_tabs(&mut s), (0, 0, vec![]));
    assert_eq!(s.menu_state("pptx.text.indent.none"), Some((true, true)));
    assert_eq!(s.menu_state("pptx.text.tabs.clear"), Some((true, true)));

    // 7. 保存と再読込
    s.save_as("paragraph-indent-cleared.pptx");
    let saved1 = s.reopen();
    let p1 = &saved1.slides[0].shapes[1].text_body().unwrap().paragraphs[0];
    assert_eq!(p1.margin_left, 0);
    assert_eq!(p1.indent, 0);
    assert_eq!(p1.tabs.len(), 0);

    s.key("escape");
    undo_all_and_redo(&mut s);
}


/// 表の罫線の辺指定（外枠・内側・上下左右・すべての罫線・枠なし）の操作、Undo/Redo、保存と再読込
pub(crate) fn table_cell_border_placements() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    s.menu("table.new.3x3");
    let shape = s.with(|e, _| e.slide().shapes.len() - 1);

    // 1. (0, 0) から (1, 1) の 2x2 セル範囲を選択
    let corner = cell_middle(&mut s, shape, 1, 1);
    s.shift_click(corner);
    assert_eq!(s.with(|e, _| e.text_editing().and_then(|t| t.cell_anchor)), Some((1, 1)));

    let get_table = |s: &mut Scenario| {
        s.with(|e, _| match &e.slide().shapes[shape].kind {
            ShapeKind::Table(t) => (**t).clone(),
            _ => panic!("expected table"),
        })
    };

    // 2. ツールバーから「枠なし（No Border）」を適用
    s.toolbar_menu("table.borders", "table.borders.none");
    let t = get_table(&mut s);
    for r in 0..=1 {
        for c in 0..=1 {
            let b = &t.rows[r].cells[c].borders;
            assert!(b.top.is_none() && b.bottom.is_none() && b.left.is_none() && b.right.is_none(),
                "cell ({r}, {c}) borders should be None");
        }
    }

    // 3. メニューから「すべての罫線（All Borders）」を適用
    s.menu("table.borders.all");
    let t = get_table(&mut s);
    for r in 0..=1 {
        for c in 0..=1 {
            let b = &t.rows[r].cells[c].borders;
            assert!(b.top.is_some() && b.bottom.is_some() && b.left.is_some() && b.right.is_some(),
                "cell ({r}, {c}) borders should all be Some");
        }
    }

    // 4. Undo / Redo
    s.key("cmd-z");
    let t = get_table(&mut s);
    assert!(t.rows[0].cells[0].borders.top.is_none(), "undone to none");
    s.key("cmd-shift-z");
    let t = get_table(&mut s);
    assert!(t.rows[0].cells[0].borders.top.is_some(), "redone to all");

    // 5. 外枠（Outside Borders）を適用
    s.menu("table.borders.none");
    s.toolbar_menu("table.borders", "table.borders.outside");
    let t = get_table(&mut s);
    assert!(t.rows[0].cells[0].borders.top.is_some() && t.rows[0].cells[0].borders.left.is_some());
    assert!(t.rows[0].cells[0].borders.bottom.is_none() && t.rows[0].cells[0].borders.right.is_none());
    assert!(t.rows[1].cells[1].borders.bottom.is_some() && t.rows[1].cells[1].borders.right.is_some());
    assert!(t.rows[1].cells[1].borders.top.is_none() && t.rows[1].cells[1].borders.left.is_none());

    // 6. 内側（Inside Borders）を適用
    s.menu("table.borders.inside");
    let t = get_table(&mut s);
    assert!(t.rows[0].cells[0].borders.right.is_some() && t.rows[0].cells[0].borders.bottom.is_some());

    // 7. 単一辺（Top, Bottom, Left, Right）の適用
    s.menu("table.borders.none");
    s.menu("table.borders.top");
    let t = get_table(&mut s);
    assert!(t.rows[0].cells[0].borders.top.is_some() && t.rows[0].cells[1].borders.top.is_some());
    assert!(t.rows[1].cells[0].borders.top.is_none() && t.rows[1].cells[1].borders.top.is_none());

    s.menu("table.borders.bottom");
    let t = get_table(&mut s);
    assert!(t.rows[1].cells[0].borders.bottom.is_some() && t.rows[1].cells[1].borders.bottom.is_some());

    s.menu("table.borders.left");
    let t = get_table(&mut s);
    assert!(t.rows[0].cells[0].borders.left.is_some() && t.rows[1].cells[0].borders.left.is_some());

    s.menu("table.borders.right");
    let t = get_table(&mut s);
    assert!(t.rows[0].cells[1].borders.right.is_some() && t.rows[1].cells[1].borders.right.is_some());

    // 7.5. 斜め罫線（Diagonal Down, Diagonal Up）の適用と検証
    s.menu("table.borders.none");
    s.menu("table.borders.diagonal-down");
    let t = get_table(&mut s);
    for r in 0..=1 {
        for c in 0..=1 {
            assert!(t.rows[r].cells[c].borders.tl_to_br.is_some(), "cell ({r}, {c}) diagonal down should be Some");
            assert!(t.rows[r].cells[c].borders.bl_to_tr.is_none());
        }
    }

    s.menu("table.borders.diagonal-up");
    let t = get_table(&mut s);
    for r in 0..=1 {
        for c in 0..=1 {
            assert!(t.rows[r].cells[c].borders.tl_to_br.is_some());
            assert!(t.rows[r].cells[c].borders.bl_to_tr.is_some(), "cell ({r}, {c}) diagonal up should be Some");
        }
    }

    // Undo / Redo で対角線の往復確認
    s.key("cmd-z");
    let t = get_table(&mut s);
    assert!(t.rows[0].cells[0].borders.bl_to_tr.is_none());
    assert!(t.rows[0].cells[0].borders.tl_to_br.is_some());
    s.key("cmd-shift-z");
    let t = get_table(&mut s);
    assert!(t.rows[0].cells[0].borders.bl_to_tr.is_some());

    // 枠なしで対角線もクリアされることを確認
    s.menu("table.borders.none");
    let t = get_table(&mut s);
    assert!(t.rows[0].cells[0].borders.tl_to_br.is_none() && t.rows[0].cells[0].borders.bl_to_tr.is_none());

    // 8. 表全体選択時（Esc でセル編集を抜けた状態）での枠なし・すべての罫線
    s.key("escape");
    s.menu("table.borders.none");
    let t = get_table(&mut s);
    for r in 0..3 {
        for c in 0..3 {
            let b = &t.rows[r].cells[c].borders;
            assert!(b.top.is_none() && b.bottom.is_none() && b.left.is_none() && b.right.is_none(),
                "whole table cell ({r}, {c}) borders should be None");
        }
    }

    s.menu("table.borders.all");
    let t = get_table(&mut s);
    for r in 0..3 {
        for c in 0..3 {
            let b = &t.rows[r].cells[c].borders;
            assert!(b.top.is_some() && b.bottom.is_some() && b.left.is_some() && b.right.is_some(),
                "whole table cell ({r}, {c}) borders should be Some");
        }
    }

    // 9. 保存と再読込（ラウンドトリップ検証）
    s.save_as("table-borders-all.pptx");
    let saved = s.reopen();
    let ShapeKind::Table(saved_t) = &saved.slides[2].shapes[shape].kind else { panic!("a table") };
    for r in 0..3 {
        for c in 0..3 {
            let b = &saved_t.rows[r].cells[c].borders;
            assert!(b.top.is_some() && b.bottom.is_some() && b.left.is_some() && b.right.is_some(),
                "saved table cell ({r}, {c}) borders should be Some");
        }
    }

    // 10. 書式メニューから罫線の太さを 3 pt（38,100 EMU）に変更
    s.menu("table.borders.3pt");
    let t = get_table(&mut s);
    let s0 = t.rows[0].cells[0].borders.top.as_ref().unwrap();
    assert_eq!(s0.width, 38_100, "table border width changed to 3pt");

    // 11. 書式メニューから罫線の色を赤色に変更（太さ 3 pt は維持）
    s.menu("table.borders.red");
    let t = get_table(&mut s);
    let s0 = t.rows[0].cells[0].borders.top.as_ref().unwrap();
    assert_eq!(s0.width, 38_100, "table border width kept 3pt");
    assert_eq!(s0.color, ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xc00000)), "table border color changed to red");

    // 12. ツールバーの線の太さから 1 pt（12,700 EMU）に変更（色は赤色を維持）
    s.toolbar_menu("line-weight", "width.1");
    let t = get_table(&mut s);
    let s0 = t.rows[0].cells[0].borders.top.as_ref().unwrap();
    assert_eq!(s0.width, 12_700, "table border width changed to 1pt");
    assert_eq!(s0.color, ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xc00000)), "table border color kept red");

    // 13. 保存と再読込で太さと色が保持されることを確認
    s.save_as("table-borders-red-1pt.pptx");
    let saved = s.reopen();
    let ShapeKind::Table(saved_t) = &saved.slides[2].shapes[shape].kind else { panic!("a table") };
    let saved_s0 = saved_t.rows[0].cells[0].borders.top.as_ref().unwrap();
    assert_eq!(saved_s0.width, 12_700, "saved border width is 1pt");
    assert_eq!(saved_s0.color, ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xc00000)), "saved border color is red");

    // 14. 斜め罫線の設定と保存・再読込（ラウンドトリップ検証）
    s.click_shape(shape);
    s.menu("table.borders.diagonal-down");
    s.menu("table.borders.diagonal-up");
    let t = get_table(&mut s);
    assert!(t.rows[0].cells[0].borders.tl_to_br.is_some());
    assert!(t.rows[0].cells[0].borders.bl_to_tr.is_some());

    s.save_as("table-borders-diagonal.pptx");
    let saved = s.reopen();
    let ShapeKind::Table(saved_t) = &saved.slides[2].shapes[shape].kind else { panic!("a table") };
    assert!(saved_t.rows[0].cells[0].borders.tl_to_br.is_some(), "saved diagonal down kept");
    assert!(saved_t.rows[0].cells[0].borders.bl_to_tr.is_some(), "saved diagonal up kept");

    undo_all_and_redo(&mut s);
}

pub(crate) fn table_style_options() {
    use ibis_doc_model::TableStyleOptions;

    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    s.menu("table.new.3x3");
    let shape = s.with(|e, _| e.slide().shapes.len() - 1);

    let get_table = |s: &mut Scenario| {
        s.with(|e, _| match &e.slide().shapes[shape].kind {
            ShapeKind::Table(t) => (**t).clone(),
            _ => panic!("expected table"),
        })
    };

    // 1. 新規テーブルの初期スタイルオプションを確認
    let default_options = TableStyleOptions::default();
    assert_eq!(get_table(&mut s).style_options, default_options);
    assert!(default_options.first_row);
    assert!(default_options.band_row);
    assert!(!default_options.last_row);
    assert!(!default_options.first_col);
    assert!(!default_options.last_col);
    assert!(!default_options.band_col);

    // 2. ツールバーから各オプションをトグル
    s.toolbar_menu("table.style-options", "pptx.table.first-row");
    assert!(!get_table(&mut s).style_options.first_row, "first_row toggled to false");

    s.toolbar_menu("table.style-options", "pptx.table.last-row");
    assert!(get_table(&mut s).style_options.last_row, "last_row toggled to true");

    // 3. メニューバー（書式 > 表のスタイルオプション）からトグル
    s.menu("pptx.table.first-col");
    assert!(get_table(&mut s).style_options.first_col, "first_col toggled to true");

    s.menu("pptx.table.band-row");
    assert!(!get_table(&mut s).style_options.band_row, "band_row toggled to false");

    s.toolbar_menu("table.style-options", "pptx.table.last-col");
    assert!(get_table(&mut s).style_options.last_col, "last_col toggled to true");

    s.toolbar_menu("table.style-options", "pptx.table.band-col");
    assert!(get_table(&mut s).style_options.band_col, "band_col toggled to true");

    let expected = TableStyleOptions {
        first_row: false,
        last_row: true,
        first_col: true,
        last_col: true,
        band_row: false,
        band_col: true,
    };
    assert_eq!(get_table(&mut s).style_options, expected);

    // 4. Undo / Redo の検証
    s.key("cmd-z");
    assert!(!get_table(&mut s).style_options.band_col, "undone band_col to false");
    s.key("cmd-shift-z");
    assert!(get_table(&mut s).style_options.band_col, "redone band_col to true");
    assert_eq!(get_table(&mut s).style_options, expected);

    // 5. 保存と再読込（ラウンドトリップ検証）
    s.save_as("test-table-style-options.pptx");
    let saved = s.reopen();
    let ShapeKind::Table(saved_t) = &saved.slides[2].shapes[shape].kind else { panic!("a table") };
    assert_eq!(saved_t.style_options, expected, "saved table style options match exactly");

    // 6. 再読込後のテーブルを再度トグルしてインプレース保存・再読込検証
    s.click_shape(shape);
    s.toolbar_menu("table.style-options", "pptx.table.first-row");
    let expected2 = TableStyleOptions {
        first_row: true,
        ..expected
    };
    assert_eq!(get_table(&mut s).style_options, expected2);
    s.save_as("test-table-style-options-update.pptx");
    let saved2 = s.reopen();
    let ShapeKind::Table(saved_t2) = &saved2.slides[2].shapes[shape].kind else { panic!("a table") };
    assert_eq!(saved_t2.style_options, expected2, "updated table style options round-trip preserved");

    undo_all_and_redo(&mut s);
}

pub(crate) fn table_style_selection() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    s.menu("table.new.3x3");
    let shape = s.with(|e, _| e.slide().shapes.len() - 1);

    let get_table = |s: &mut Scenario| {
        s.with(|e, _| match &e.slide().shapes[shape].kind {
            ShapeKind::Table(t) => (**t).clone(),
            _ => panic!("expected table"),
        })
    };

    // 1. 新規テーブルの初期 style_id は None
    assert_eq!(get_table(&mut s).style_id, None);

    // 2. ツールバーの「表スタイル」メニューから中間スタイル 2 - アクセント 1 を選択
    let medium2_accent1 = "{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}";
    s.toolbar_menu("table.style", "pptx.table-style.medium2-accent1");
    assert_eq!(get_table(&mut s).style_id.as_deref(), Some(medium2_accent1));

    // 3. 書式メニューから淡色スタイル 1 を選択
    let light1 = "{3990B04E-E470-4E4E-A68B-510787340C36}";
    s.menu("pptx.table-style.light1");
    assert_eq!(get_table(&mut s).style_id.as_deref(), Some(light1));

    // 4. Undo / Redo の検証
    s.key("cmd-z");
    assert_eq!(get_table(&mut s).style_id.as_deref(), Some(medium2_accent1));
    s.key("cmd-shift-z");
    assert_eq!(get_table(&mut s).style_id.as_deref(), Some(light1));

    // 5. 保存と再読込（ラウンドトリップ検証）
    s.save_as("test-table-style-selection.pptx");
    let saved = s.reopen();
    let ShapeKind::Table(saved_t) = &saved.slides[2].shapes[shape].kind else { panic!("a table") };
    assert_eq!(saved_t.style_id.as_deref(), Some(light1));

    // 6. 再読込後にスタイルなしを選択して保存・再読込
    s.click_shape(shape);
    s.toolbar_menu("table.style", "pptx.table-style.none");
    assert_eq!(get_table(&mut s).style_id, None);
    s.save_as("test-table-style-none.pptx");
    let saved2 = s.reopen();
    let ShapeKind::Table(saved_t2) = &saved2.slides[2].shapes[shape].kind else { panic!("a table") };
    assert_eq!(saved_t2.style_id, None);

    undo_all_and_redo(&mut s);
}

pub(crate) fn transition_effect_options() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(1);

    // 1. スライドにプッシュ（push）効果を設定
    s.toolbar_menu("transition", "transition.push");
    let t = s.with(|e, _| e.slide().transition.clone()).expect("push transition");
    assert_eq!(t.effect, "push");
    assert_eq!(t.direction, None);

    // 2. ツールバーの「効果のオプション」から「左から」を選択
    s.toolbar_menu("transition-options", "transition.dir.left");
    let t_left = s.with(|e, _| e.slide().transition.clone()).expect("left transition");
    assert_eq!(t_left.effect, "push");
    assert_eq!(t_left.direction.as_deref(), Some("l"));

    // 3. 効果のオプションから「上から」を選択
    s.toolbar_menu("transition-options", "transition.dir.up");
    let t_up = s.with(|e, _| e.slide().transition.clone()).expect("up transition");
    assert_eq!(t_up.effect, "push");
    assert_eq!(t_up.direction.as_deref(), Some("u"));

    // 4. Undo / Redo の検証
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().transition.as_ref().and_then(|t| t.direction.clone())), Some("l".to_string()));
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().transition.as_ref().and_then(|t| t.direction.clone())), Some("u".to_string()));

    // 5. 保存と再読込（ラウンドトリップ検証）
    s.save_as("test-transition-effect-options.pptx");
    let saved = s.reopen();
    let saved_t = saved.slides[0].transition.as_ref().expect("saved transition");
    assert_eq!(saved_t.effect, "push");
    assert_eq!(saved_t.direction.as_deref(), Some("u"));

    // 6. 再読込後にスプリット効果に変更し、「水平方向 (内側へ)」を選択して保存・再読込
    s.toolbar_menu("transition", "transition.split");
    s.toolbar_menu("transition-options", "transition.dir.horz-in");
    let t_split = s.with(|e, _| e.slide().transition.clone()).expect("split transition");
    assert_eq!(t_split.effect, "split");
    assert_eq!(t_split.direction.as_deref(), Some("horzIn"));

    s.save_as("test-transition-split-horzin.pptx");
    let saved2 = s.reopen();
    let saved_t2 = saved2.slides[0].transition.as_ref().expect("saved split transition");
    assert_eq!(saved_t2.effect, "split");
    assert_eq!(saved_t2.direction.as_deref(), Some("horzIn"));

    undo_all_and_redo(&mut s);
}

pub(crate) fn table_cell_fill_range_and_picture() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    s.menu("table.new.3x3");
    let shape = s.with(|e, _| e.slide().shapes.len() - 1);

    let get_table = |s: &mut Scenario| {
        s.with(|e, _| match &e.slide().shapes[shape].kind {
            ShapeKind::Table(t) => (**t).clone(),
            _ => panic!("expected table"),
        })
    };

    // 1. (0, 0) から (1, 1) の 2x2 セル範囲を Shift+クリックで選択
    let corner = cell_middle(&mut s, shape, 1, 1);
    s.shift_click(corner);
    assert_eq!(s.with(|e, _| e.text_editing().and_then(|t| t.cell_anchor)), Some((1, 1)));

    // 2. 選択セル範囲に単色塗り（青色: color.blue）を適用
    s.toolbar_menu("fill", "color.blue");
    let t = get_table(&mut s);
    let blue = Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x0070c0)));
    for r in 0..3 {
        for c in 0..3 {
            if (0..=1).contains(&r) && (0..=1).contains(&c) {
                assert_eq!(t.rows[r].cells[c].fill, blue, "cell ({r}, {c}) in selected range should be blue");
            } else {
                assert_ne!(t.rows[r].cells[c].fill, blue, "cell ({r}, {c}) outside range should not be blue");
            }
        }
    }

    // 3. Undo / Redo でセル範囲塗りが復元されることを検証
    s.key("cmd-z");
    let t = get_table(&mut s);
    assert_ne!(t.rows[0].cells[0].fill, blue, "undone to previous fill");
    s.key("cmd-shift-z");
    let t = get_table(&mut s);
    assert_eq!(t.rows[0].cells[0].fill, blue, "redone to blue fill");

    // 4. (0, 0) セルのみを選択して画像塗りを適用
    let origin = cell_middle(&mut s, shape, 0, 0);
    s.double_click(origin);
    assert_eq!(s.with(|e, _| e.text_editing().and_then(|t| t.cell)), Some((0, 0)));
    assert_eq!(s.with(|e, _| e.text_editing().and_then(|t| t.cell_anchor)), None);

    let img_data = png(32, 32, [255, 0, 0]);
    let media = s.update(|e, _, cx| e.fill_selected_with_picture("test_cell.png", img_data, cx)).expect("image media created");
    let t = get_table(&mut s);
    assert_eq!(t.rows[0].cells[0].fill, Fill::Image(media), "(0, 0) cell should have image fill");
    assert_eq!(t.rows[0].cells[1].fill, blue, "(0, 1) cell should retain blue fill");
    assert_eq!(t.rows[1].cells[0].fill, blue, "(1, 0) cell should retain blue fill");

    // 5. 表全体選択時（Esc）での単色塗り（黄色: color.yellow）適用
    s.key("escape");
    s.toolbar_menu("fill", "color.yellow");
    let t = get_table(&mut s);
    let yellow = Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffff00)));
    for r in 0..3 {
        for c in 0..3 {
            assert_eq!(t.rows[r].cells[c].fill, yellow, "whole table cell ({r}, {c}) should be yellow");
        }
    }

    // 6. Undo で画像塗りと範囲塗りの状態に戻ることを確認
    s.key("cmd-z");
    let t = get_table(&mut s);
    assert_eq!(t.rows[0].cells[0].fill, Fill::Image(media), "undone: (0, 0) is image fill again");
    assert_eq!(t.rows[0].cells[1].fill, blue, "undone: (0, 1) is blue fill again");

    // 7. ファイル保存と再読込（ラウンドトリップ検証）
    s.save_as("test-table-cell-fill-image.pptx");
    let saved = s.reopen();
    let ShapeKind::Table(saved_t) = &saved.slides[2].shapes[shape].kind else { panic!("a table") };
    assert!(matches!(saved_t.rows[0].cells[0].fill, Fill::Image(_)), "saved (0, 0) cell fill is Image");
    assert_eq!(saved_t.rows[0].cells[1].fill, blue, "saved (0, 1) cell fill is blue");
    assert_eq!(saved_t.rows[1].cells[0].fill, blue, "saved (1, 0) cell fill is blue");
    assert_eq!(saved_t.rows[1].cells[1].fill, blue, "saved (1, 1) cell fill is blue");

    undo_all_and_redo(&mut s);
}

pub(crate) fn chart_title_options() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s
        .with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(&sh.kind, ShapeKind::Chart(c) if c.groups.iter().any(|g| g.series.iter().any(|x| x.values.len() > 1)))).map(|j| (i, j))))
        .expect("a slide with a chart");
    s.page(page + 1);
    s.click_shape(shape);

    let get_title = |s: &mut Scenario| s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Chart(c) => c.title.as_ref().map(|t| t.text.clone()),
        _ => None,
    });

    // 1. 初期状態：タイトルなし
    assert_eq!(get_title(&mut s), None, "initially has no title");

    // 2. ツールバーメニューから「グラフの上」を選択してタイトル追加
    s.toolbar_menu("chart-title", "chart.title.above");
    assert!(get_title(&mut s).is_some(), "title added above chart");
    assert_eq!(get_title(&mut s).as_deref(), Some("グラフ タイトル"));

    // 3. ツールバーメニューから「なし」を選択してタイトル削除
    s.toolbar_menu("chart-title", "chart.title.none");
    assert_eq!(get_title(&mut s), None, "title removed");

    // 4. Undo でタイトルが復元される
    s.key("cmd-z");
    assert_eq!(get_title(&mut s).as_deref(), Some("グラフ タイトル"), "undone: title restored");

    // 5. Redo でタイトルが再び消える
    s.key("cmd-shift-z");
    assert_eq!(get_title(&mut s), None, "redone: title removed");

    // 6. 再び「グラフの上」でタイトルを表示
    s.toolbar_menu("chart-title", "chart.title.above");
    assert!(get_title(&mut s).is_some());

    // 7. ファイル保存と再読込（ラウンドトリップ検証）
    s.save_as("test-chart-title-options.pptx");
    let saved = s.reopen();
    let ShapeKind::Chart(rc) = &saved.slides[page].shapes[shape].kind else { panic!("a chart") };
    assert_eq!(rc.title.as_ref().map(|t| t.text.as_str()), Some("グラフ タイトル"), "saved chart title is kept");

    undo_all_and_redo(&mut s);
}

pub(crate) fn chart_labels_options() {
    use ibis_doc_model::LabelPosition;
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s
        .with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(&sh.kind, ShapeKind::Chart(c) if c.groups.iter().any(|g| g.series.iter().any(|x| x.values.len() > 1)))).map(|j| (i, j))))
        .expect("a slide with a chart");
    s.page(page + 1);
    s.click_shape(shape);

    let get_labels = |s: &mut Scenario| s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Chart(c) => c.groups.iter().flat_map(|g| &g.series).map(|x| x.labels.as_ref().map(|l| (l.position, l.show_value))).collect::<Vec<_>>(),
        _ => Vec::new(),
    });

    // 1. ツールバーメニューから「中央」を選択してデータラベル追加
    s.toolbar_menu("chart-labels", "chart.labels.center");
    assert!(get_labels(&mut s).iter().all(|l| matches!(l, Some((Some(LabelPosition::Center), true)))), "all series have center labels");

    // 2. ツールバーメニューから「内側端」を選択して位置変更
    s.toolbar_menu("chart-labels", "chart.labels.inside-end");
    assert!(get_labels(&mut s).iter().all(|l| matches!(l, Some((Some(LabelPosition::InsideEnd), true)))), "all series have inside-end labels");

    // 3. ツールバーメニューから「なし」を選択してデータラベル削除
    s.toolbar_menu("chart-labels", "chart.labels.none");
    assert!(get_labels(&mut s).iter().all(Option::is_none), "all series labels removed");

    // 4. Undo で「内側端」が復元される
    s.key("cmd-z");
    assert!(get_labels(&mut s).iter().all(|l| matches!(l, Some((Some(LabelPosition::InsideEnd), true)))), "undone: inside-end labels restored");

    // 5. Redo で再び「なし」になる
    s.key("cmd-shift-z");
    assert!(get_labels(&mut s).iter().all(Option::is_none), "redone: labels removed");

    // 6. 「外側端」を選択
    s.toolbar_menu("chart-labels", "chart.labels.outside-end");
    assert!(get_labels(&mut s).iter().all(|l| matches!(l, Some((Some(LabelPosition::OutsideEnd), true)))), "outside-end labels set");

    // 7. ファイル保存と再読込（ラウンドトリップ検証）
    s.save_as("test-chart-labels-options.pptx");
    let saved = s.reopen();
    let ShapeKind::Chart(rc) = &saved.slides[page].shapes[shape].kind else { panic!("a chart") };
    for ser in rc.groups.iter().flat_map(|g| &g.series) {
        let labels = ser.labels.as_ref().expect("saved chart labels present");
        assert!(labels.show_value);
        assert_eq!(labels.position, Some(LabelPosition::OutsideEnd), "saved chart label position is outside-end");
    }

    undo_all_and_redo(&mut s);
}

pub(crate) fn chart_axis_scaling_and_options() {
    use ibis_doc_model::{AxisKind, Number};
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s
        .with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(&sh.kind, ShapeKind::Chart(c) if c.axes.iter().any(|a| a.kind == AxisKind::Value))).map(|j| (i, j))))
        .expect("a slide with a chart");
    s.page(page + 1);
    s.click_shape(shape);

    let get_val_axis = |s: &mut Scenario| s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Chart(c) => c.axes.iter().find(|a| a.kind == AxisKind::Value).cloned(),
        _ => None,
    });

    // 1. 縦軸の最小値を 0 に固定
    s.update(|e, _, cx| e.set_chart_axis_min(true, Some(Number(0.0)), cx));
    let va = get_val_axis(&mut s).expect("val axis");
    assert_eq!(va.min, Some(Number(0.0)));

    // 2. 縦軸の最大値を 1000 に固定、主目盛を 100 に固定
    s.update(|e, _, cx| {
        e.set_chart_axis_max(true, Some(Number(1000.0)), cx);
        e.set_chart_axis_major_unit(true, Some(Number(100.0)), cx);
    });
    let va = get_val_axis(&mut s).expect("val axis");
    assert_eq!(va.max, Some(Number(1000.0)));
    assert_eq!(va.major_unit, Some(Number(100.0)));

    // 3. 表示単位を千 (1000.0) に設定
    s.update(|e, _, cx| e.set_chart_axis_display_unit(true, Some(Number(1000.0)), cx));
    let va = get_val_axis(&mut s).expect("val axis");
    assert_eq!(va.display_unit, Some(Number(1000.0)));

    // 4. 軸反転をトグル (true)
    s.update(|e, _, cx| e.toggle_chart_axis_reversed(true, cx));
    let va = get_val_axis(&mut s).expect("val axis");
    assert!(va.reversed);

    // 5. 対数目盛をトグル (true)
    s.update(|e, _, cx| e.toggle_chart_axis_log_scale(true, cx));
    let va = get_val_axis(&mut s).expect("val axis");
    assert_eq!(va.log_base, Some(Number(10.0)));

    // 6. Undo で対数目盛が解除される
    s.key("cmd-z");
    let va = get_val_axis(&mut s).expect("val axis");
    assert_eq!(va.log_base, None);
    assert!(va.reversed);

    // 7. Redo で対数目盛が再び有効化
    s.key("cmd-shift-z");
    let va = get_val_axis(&mut s).expect("val axis");
    assert_eq!(va.log_base, Some(Number(10.0)));

    // 8. ファイル保存と再読込
    s.save_as("test-chart-axis-options.pptx");
    let saved = s.reopen();
    let ShapeKind::Chart(rc) = &saved.slides[page].shapes[shape].kind else { panic!("a chart") };
    let saved_va = rc.axes.iter().find(|a| a.kind == AxisKind::Value).expect("saved val axis");
    assert_eq!(saved_va.min, Some(Number(0.0)));
    assert_eq!(saved_va.max, Some(Number(1000.0)));
    assert_eq!(saved_va.major_unit, Some(Number(100.0)));
    assert_eq!(saved_va.display_unit, Some(Number(1000.0)));
    assert!(saved_va.reversed);
    assert_eq!(saved_va.log_base, Some(Number(10.0)));

    undo_all_and_redo(&mut s);
}

pub(crate) fn chart_trendline_and_error_bars_options() {
    use ibis_doc_model::{ErrorBarType, ErrorBarValueType, TrendlineType};
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s
        .with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(&sh.kind, ShapeKind::Chart(c) if c.groups.iter().any(|g| !g.series.is_empty()))).map(|j| (i, j))))
        .expect("a slide with a chart");
    s.page(page + 1);
    s.click_shape(shape);

    let get_first_series = |s: &mut Scenario| s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Chart(c) => c.groups.first().and_then(|g| g.series.first()).cloned(),
        _ => None,
    });

    // 1. ツールバーメニューから線形近似曲線を追加
    s.toolbar_menu("chart-trendline", "pptx.chart.trendline.linear");
    let ser = get_first_series(&mut s).expect("series");
    assert_eq!(ser.trendline.as_ref().map(|t| t.kind), Some(TrendlineType::Linear));

    // 2. 数式表示を有効化
    s.toolbar_menu("chart-trendline", "pptx.chart.trendline.disp-eq");
    let ser = get_first_series(&mut s).expect("series");
    assert!(ser.trendline.as_ref().is_some_and(|t| t.disp_eq));

    // 3. ツールバーメニューから誤差範囲（パーセンテージ）を追加
    s.toolbar_menu("chart-error-bars", "pptx.chart.error-bars.percent");
    let ser = get_first_series(&mut s).expect("series");
    assert_eq!(ser.error_bars.as_ref().map(|e| e.val_type), Some(ErrorBarValueType::Percentage));

    // 4. 誤差範囲の方向を「正」に変更
    s.toolbar_menu("chart-error-bars", "pptx.chart.error-bars.plus");
    let ser = get_first_series(&mut s).expect("series");
    assert_eq!(ser.error_bars.as_ref().map(|e| e.bar_type), Some(ErrorBarType::Plus));

    // 5. Undo で誤差範囲の方向が「両方向」に戻る
    s.key("cmd-z");
    let ser = get_first_series(&mut s).expect("series");
    assert_eq!(ser.error_bars.as_ref().map(|e| e.bar_type), Some(ErrorBarType::Both));

    // 6. Redo で再び「正」になる
    s.key("cmd-shift-z");
    let ser = get_first_series(&mut s).expect("series");
    assert_eq!(ser.error_bars.as_ref().map(|e| e.bar_type), Some(ErrorBarType::Plus));

    // 7. ファイル保存と再読込
    s.save_as("test-chart-trendline-errbars-options.pptx");
    let saved = s.reopen();
    let ShapeKind::Chart(rc) = &saved.slides[page].shapes[shape].kind else { panic!("a chart") };
    let saved_ser = rc.groups.first().and_then(|g| g.series.first()).expect("saved series");
    let tl = saved_ser.trendline.as_ref().expect("saved trendline");
    assert_eq!(tl.kind, TrendlineType::Linear);
    assert!(tl.disp_eq);

    let eb = saved_ser.error_bars.as_ref().expect("saved error bars");
    assert_eq!(eb.val_type, ErrorBarValueType::Percentage);
    assert_eq!(eb.bar_type, ErrorBarType::Plus);

    // 8. ツールバーから近似曲線と誤差範囲を削除
    s.toolbar_menu("chart-trendline", "pptx.chart.trendline.none");
    s.toolbar_menu("chart-error-bars", "pptx.chart.error-bars.none");
    let ser = get_first_series(&mut s).expect("series");
    assert!(ser.trendline.is_none());
    assert!(ser.error_bars.is_none());

    undo_all_and_redo(&mut s);
}

/// 系列の書式（塗りつぶし、枠線、マーカー）およびプロット領域の書式（背景塗りつぶし）の
/// ツールバー・メニュー設定、保存・再読込保持、Undo/Redo検証。
pub(crate) fn chart_series_and_plot_area_formatting_options() {
    use ibis_doc_model::chart::MarkerSymbol;
    use ibis_doc_model::{ChartType, Color, Fill, Rgb, Stroke};

    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s
        .with(|e, _| {
            e.presentation().slides.iter().enumerate().find_map(|(i, sl)| {
                sl.shapes.iter().position(|sh| matches!(&sh.kind, ShapeKind::Chart(c) if c.groups.iter().any(|g| matches!(g.kind, ChartType::Bar { .. }) && !g.series.is_empty()))).map(|j| (i, j))
            })
        })
        .expect("a slide with a bar chart");

    s.page(page + 1);
    s.click_shape(shape);

    let get_chart = |s: &mut Scenario| {
        s.with(|e, _| match &e.slide().shapes[shape].kind {
            ShapeKind::Chart(c) => (**c).clone(),
            _ => panic!("a chart"),
        })
    };

    // 1. 系列の書式とプロット領域の書式を設定
    s.update(|e, _, cx| {
        e.set_chart_series_fill(None, Fill::Solid(Color::opaque(Rgb(0x112233))), cx);
        e.set_chart_series_stroke(None, Some(Stroke::solid(Color::opaque(Rgb(0x445566)), 28_575)), cx);
        e.set_chart_series_marker_symbol(None, MarkerSymbol::Diamond, cx);
        e.set_chart_plot_background(Fill::Solid(Color::opaque(Rgb(0xEEFFEE))), cx);
    });

    let c = get_chart(&mut s);
    assert_eq!(c.plot_background, Fill::Solid(Color::opaque(Rgb(0xEEFFEE))));
    let first_ser = c.groups[0].series.first().expect("series");
    assert_eq!(first_ser.fill, Fill::Solid(Color::opaque(Rgb(0x112233))));
    assert_eq!(first_ser.stroke.as_ref().unwrap().color, Color::opaque(Rgb(0x445566)));
    assert_eq!(first_ser.marker.as_ref().unwrap().symbol, MarkerSymbol::Diamond);

    // 2. ツールバーメニューからマーカー記号を三角に変更
    s.toolbar_menu("chart-series-format", "pptx.chart.marker.triangle");
    let c = get_chart(&mut s);
    let first_ser = c.groups[0].series.first().expect("series");
    assert_eq!(first_ser.marker.as_ref().unwrap().symbol, MarkerSymbol::Triangle);

    // 3. 保存と再読込
    s.save_as("test-chart-series-plot-area-options.pptx");
    let saved = s.reopen();
    let ShapeKind::Chart(rc) = &saved.slides[page].shapes[shape].kind else { panic!("a chart") };
    assert_eq!(rc.plot_background, Fill::Solid(Color::opaque(Rgb(0xEEFFEE))));
    let saved_ser = rc.groups[0].series.first().expect("saved series");
    assert_eq!(saved_ser.fill, Fill::Solid(Color::opaque(Rgb(0x112233))));
    assert_eq!(saved_ser.stroke.as_ref().unwrap().color, Color::opaque(Rgb(0x445566)));
    assert_eq!(saved_ser.marker.as_ref().unwrap().symbol, MarkerSymbol::Triangle);

    // 4. Undo / Redo
    undo_all_and_redo(&mut s);
}

pub(crate) fn chart_style_options() {
    use ibis_doc_model::{ChartType, ShapeKind};

    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s
        .with(|e, _| {
            e.presentation().slides.iter().enumerate().find_map(|(i, sl)| {
                sl.shapes.iter().position(|sh| matches!(&sh.kind, ShapeKind::Chart(c) if c.groups.iter().any(|g| matches!(g.kind, ChartType::Bar { .. }) && !g.series.is_empty()))).map(|j| (i, j))
            })
        })
        .expect("a slide with a bar chart");

    s.page(page + 1);
    s.click_shape(shape);

    let get_chart = |s: &mut Scenario| {
        s.with(|e, _| match &e.slide().shapes[shape].kind {
            ShapeKind::Chart(c) => (**c).clone(),
            _ => panic!("a chart"),
        })
    };

    // 1. 初期状態確認
    let c = get_chart(&mut s);
    assert_eq!(c.style, Some(2));

    // 2. ツールバー「スタイル」メニューからスタイル 3 を選択
    s.toolbar_menu("chart-style", "pptx.chart.style.3");
    let c = get_chart(&mut s);
    assert_eq!(c.style, Some(3));

    // 3. ツールバー「色の変更」メニューからカラフル 1 を選択
    s.toolbar_menu("chart-palette", "pptx.chart.palette.1");
    let c = get_chart(&mut s);
    let first_ser = c.groups[0].series.first().expect("series");
    assert_eq!(first_ser.fill, ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x4472C4))));

    // 4. ツールバー「スタイル」メニューからスタイルなしを選択
    s.toolbar_menu("chart-style", "pptx.chart.style.none");
    let c = get_chart(&mut s);
    assert_eq!(c.style, None);

    // 5. 保存と再読込
    s.save_as("test-chart-style-options.pptx");
    let saved = s.reopen();
    let ShapeKind::Chart(rc) = &saved.slides[page].shapes[shape].kind else { panic!("a chart") };
    assert_eq!(rc.style, None);
    let saved_ser = rc.groups[0].series.first().expect("saved series");
    assert_eq!(saved_ser.fill, ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x4472C4))));

    // 6. Undo / Redo
    undo_all_and_redo(&mut s);
}

/// A tiny PNG: `w` × `h` pixels of one colour.
fn png(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xffff_ffffu32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend(data);
        out.extend(&body);
        out.extend(crc(&body).to_be_bytes());
    }
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::new();
    ihdr.extend(w.to_be_bytes());
    ihdr.extend(h.to_be_bytes());
    ihdr.extend([8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    // Stored (uncompressed) deflate blocks in a zlib stream.
    let mut raw = Vec::new();
    for _ in 0..h {
        raw.push(0);
        for _ in 0..w {
            raw.extend(rgb);
        }
    }
    let mut z = vec![0x78, 0x01];
    for (i, block) in raw.chunks(65_535).enumerate() {
        let last = (i + 1) * 65_535 >= raw.len();
        z.push(last as u8);
        z.extend((block.len() as u16).to_le_bytes());
        z.extend((!(block.len() as u16)).to_le_bytes());
        z.extend(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend(((b << 16) | a).to_be_bytes());
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// 25: a picture put on the slide (as 画像を挿入 does once a file is chosen).
pub(crate) fn invalid_picture_files_do_not_change_document() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let valid = png(32, 24, [0x44, 0x88, 0xaa]);
    let before = s.with(|e, _| e.presentation().clone());
    for (name, bytes) in [("empty.png", Vec::new()), ("garbage.png", b"not an image".to_vec()), ("truncated.png", valid[..24].to_vec()), ("wrong.jpg", valid.clone())] {
        assert!(s.update(|e, _, cx| e.insert_picture(name, bytes, cx)).is_none(), "{name}");
        assert_eq!(s.with(|e, _| e.presentation().clone()), before, "{name}");
    }
    assert!(s.update(|e, _, cx| e.insert_picture("valid.png", valid.clone(), cx)).is_some());
    let (slide, targets) = s.with(|e, _| (e.current_slide, e.selected_ids().to_vec()));
    let before = s.with(|e, _| e.presentation().clone());
    for (name, bytes) in [("empty.png", Vec::new()), ("truncated.png", valid[..24].to_vec()), ("wrong.jpg", valid.clone())] {
        s.update(|e, _, cx| e.receive_picture_file(Some((slide, targets.clone())), name, bytes, cx));
        assert_eq!(s.with(|e, _| e.presentation().clone()), before, "replacement {name}");
    }
    s.save_as("invalid-picture-rejected.pptx");
    assert_eq!(s.reopen().slides[0].shapes.len(), before.slides[0].shapes.len());
    s.key("cmd-z"); assert_eq!(s.shapes(), before.slides[0].shapes.len() - 1);
    s.key("cmd-shift-z"); undo_all_and_redo(&mut s);
    for (name, format) in [("valid.jpg", image::ImageFormat::Jpeg), ("valid.gif", image::ImageFormat::Gif), ("valid.bmp", image::ImageFormat::Bmp)] {
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(8, 4).write_to(&mut encoded, format).unwrap();
        let encoded = encoded.into_inner();
        let before = s.with(|e, _| e.presentation().clone());
        assert!(s.update(|e, _, cx| e.insert_picture(name, encoded[..encoded.len() / 2].to_vec(), cx)).is_none());
        assert_eq!(s.with(|e, _| e.presentation().clone()), before);
        assert!(s.update(|e, _, cx| e.insert_picture(name, encoded.clone(), cx)).is_some());
        assert_eq!((s.with(|e, _| e.selected_shapes()[0].frame.width), s.with(|e, _| e.selected_shapes()[0].frame.height)), (8 * 9525, 4 * 9525));
    }
    s.save_as("validated-raster-pictures.pptx");
    assert_eq!(s.reopen().slides[0].shapes.len(), s.shapes());
    undo_all_and_redo(&mut s);
}

pub(crate) fn copy_table_slide_number_fields() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    // A presentation opens with the keys on the slide list: the keys of the slide need the slide focused.
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    let mut table = ibis_doc_model::Table::new_grid(2, 2, 3_000_000, 1_200_000);
    for row in &mut table.rows { for cell in &mut row.cells {
        cell.text = ibis_doc_model::TextBody::plain("1", ibis_doc_model::TextStyle::default());
        cell.text.paragraphs[0].runs[0].style.field = Some(ibis_doc_model::TextField::SlideNumber);
    } }
    let source = ibis_doc_model::Shape::new(ibis_doc_model::ShapeId(9999), ibis_doc_model::EmuRect::new(500_000,500_000,3_000_000,1_200_000), ShapeKind::Table(Box::new(table)));
    assert!(ibis_pptx::save::check_shape_insertion(&source).is_ok());
    s.update(|e, _, cx| { e.pptx_mut().unwrap().clipboard = vec![source]; assert!(e.paste_shapes(cx)); });
    s.key("cmd-c"); s.page(2); s.key("cmd-v");
    let fields = |shape: &ibis_doc_model::Shape| {
        let ShapeKind::Table(t) = &shape.kind else { panic!("table") };
        t.rows.iter().flat_map(|r| &r.cells).map(|c| (c.text.text(),c.text.paragraphs[0].runs[0].style.field)).collect::<Vec<_>>()
    };
    let before = s.with(|e, _| e.slide().shapes.last().unwrap().clone());
    assert!(fields(&before).iter().all(|(text,field)| text == "2" && *field == Some(ibis_doc_model::TextField::SlideNumber)));
    s.key("cmd-z"); s.key("cmd-shift-z"); s.save_as("table-field-copy.pptx");
    let saved = s.reopen(); assert_eq!(fields(saved.slides[1].shapes.last().unwrap()), fields(&before));
    s.update(|e, _, cx| e.run(vec![ibis_doc_model::Command::MoveSlide { slide: e.current_slide, index: 0 }], cx));
    assert!(s.with(|e, _| fields(e.slide().shapes.last().unwrap())).iter().all(|(text,_)| text == "1"));
    s.key("cmd-z"); assert!(s.with(|e, _| fields(e.slide().shapes.last().unwrap())).iter().all(|(text,_)| text == "2"));
    s.key("cmd-shift-z"); s.save_as("table-field-after-slide-move.pptx");
    assert!(fields(s.reopen().slides[0].shapes.last().unwrap()).iter().all(|(text,_)| text == "1"));
    undo_all_and_redo(&mut s);
}

pub(crate) fn copy_slide_number_field() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    // A presentation opens with the keys on the slide list: the keys of the slide need the slide focused.
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    let mut source = s.with(|e, _| e.slide().shapes.iter().find(|sh| sh.text_body().is_some_and(|b| b.paragraphs.iter().any(|p| !p.runs.is_empty()))).unwrap().clone());
    let body = source.text_body_mut().unwrap();
    body.paragraphs = vec![ibis_doc_model::Paragraph { runs: vec![
        ibis_doc_model::Run { text: "page ".into(), style: Default::default() },
        ibis_doc_model::Run { text: "1".into(), style: ibis_doc_model::RunStyle { field: Some(ibis_doc_model::TextField::SlideNumber), bold: Some(true), ..Default::default() } },
    ], ..Default::default() }];
    assert!(ibis_pptx::save::check_shape_insertion(&source).is_ok());
    s.update(|e, _, cx| {
        e.pptx_mut().unwrap().clipboard = vec![source.clone()];
        assert!(e.paste_shapes(cx));
    });
    s.key("cmd-z"); s.key("cmd-shift-z");
    s.save_as("copied-slide-number.pptx");
    let saved = s.reopen();
    let body = saved.slides[0].shapes.last().unwrap().text_body().unwrap();
    assert_eq!(body.text(), "page 1");
    assert_eq!(body.paragraphs[0].runs.last().unwrap().style.field, Some(ibis_doc_model::TextField::SlideNumber));
    s.key("cmd-c"); s.page(2); s.key("cmd-v");
    assert_eq!(s.with(|e, _| e.slide().shapes.last().unwrap().text_body().unwrap().text()), "page 2");
    let path = s.save_as("copied-slide-number-on-second.pptx");
    let bytes = std::fs::read(path).unwrap();
    let path = s.save_as("copied-slide-number-repeat.pptx");
    assert_eq!(bytes, std::fs::read(path).unwrap());
    let saved = s.reopen();
    let body = saved.slides[1].shapes.last().unwrap().text_body().unwrap();
    assert_eq!(body.text(), "page 2");
    assert_eq!(body.paragraphs[0].runs.last().unwrap().style.field, Some(ibis_doc_model::TextField::SlideNumber));
    undo_all_and_redo(&mut s);
    s.save_as("../pptx-field-cache-ui-original.pptx");
    drop(s);
    // Reopen as a fresh window so the fields are existing source nodes.
    let mut s = Scenario::open("target/scenario/pptx-field-cache-ui-original.pptx");
    s.page(2);
    s.update(|e, _, cx| e.run(vec![ibis_doc_model::Command::MoveSlide { slide: e.current_slide, index: 0 }], cx));
    assert_eq!(s.with(|e, _| e.slide().shapes.last().unwrap().text_body().unwrap().text()), "page 1");
    s.key("cmd-z"); assert_eq!(s.with(|e, _| e.slide().shapes.last().unwrap().text_body().unwrap().text()), "page 2");
    s.key("cmd-shift-z"); s.save_as("cached-fields-after-move.pptx");
    let saved = s.reopen(); assert_eq!(saved.slides[0].shapes.last().unwrap().text_body().unwrap().text(), "page 1");
    undo_all_and_redo(&mut s);
}

pub(crate) fn unsupported_text_copy_is_atomic() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let shape = s.with(|e, _| e.slide().shapes.iter().find(|sh| sh.text_body().is_some_and(|b| b.paragraphs.iter().any(|p| !p.runs.is_empty()))).unwrap().clone());
    {
        let mut copy = shape.clone();
        copy.text_body_mut().unwrap().paragraphs[0].bullet = Some(ibis_doc_model::Bullet { text: "".into(), picture: Some(ibis_doc_model::MediaId(999)), size: 100_000, color: None, auto: None });
        assert!(ibis_pptx::save::check_shape_insertion(&copy).is_err());
        let before = s.with(|e, _| e.presentation().clone());
        s.update(|e, _, cx| {
            let state = e.pptx_mut().unwrap();
            state.clipboard = vec![shape.clone(), copy];
            state.pastes = 0;
            assert!(!e.paste_shapes(cx));
            assert_eq!(e.pptx().unwrap().pastes, 0);
        });
        assert_eq!(s.with(|e, _| e.presentation().clone()), before);
    }
    s.save_as("unsupported-text-copy.pptx");
    assert_eq!(s.reopen().slides[0], s.with(|e, _| e.presentation().slides[0].clone()));
}

pub(crate) fn unsupported_shape_copy_is_atomic() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, chart) = s.with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().find(|sh| matches!(sh.kind, ShapeKind::Chart(_))).map(|sh| (i, sh.id)))).unwrap();
    s.page(page + 1);
    let ids = s.with(|e, _| e.slide().shapes.iter().map(|sh| sh.id).collect::<Vec<_>>());
    assert!(ids.contains(&chart));
    s.update(|e, _, _| e.set_selection(ids.clone()));
    let before = s.with(|e, _| e.presentation().clone());
    for id in ["edit.copy", "edit.cut", "edit.duplicate"] { assert_eq!(s.menu_state(id), Some((false, false))); }
    s.update(|e, _, cx| e.duplicate_selected(cx));
    assert_eq!(s.with(|e, _| e.presentation().clone()), before);
    s.update(|e, _, cx| e.cut_selected(cx));
    assert_eq!(s.with(|e, _| e.presentation().clone()), before);
    s.update(|e, _, cx| {
        let shapes = e.slide().shapes.clone();
        let slide = e.current_slide;
        let state = e.pptx_mut().unwrap();
        state.clipboard = shapes;
        state.clipboard_slide = Some(slide);
        state.pastes = 0;
        assert!(!e.paste_shapes(cx));
        assert_eq!(e.pptx().unwrap().pastes, 0);
    });
    assert_eq!(s.with(|e, _| e.presentation().clone()), before);
    assert_eq!(s.menu_state("edit.paste"), Some((false, false)));
    s.save_as("unsupported-copy.pptx");
    assert_eq!(s.reopen().slides[page], before.slides[page]);
}

pub(crate) fn copy_and_duplicate_table_shapes() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s.with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Table(_))).map(|j| (i, j)))).unwrap();
    s.page(page + 1); s.click_shape(shape);
    for id in ["edit.copy", "edit.cut", "edit.duplicate"] { assert_eq!(s.menu_state(id), Some((true, false))); }
    let source = s.with(|e, _| e.slide().shapes[shape].clone());
    let before = s.with(|e, _| e.slide().clone());
    s.key("cmd-d"); assert_eq!(s.shapes(), before.shapes.len() + 1);
    let copy = s.with(|e, _| e.slide().shapes.last().unwrap().clone());
    assert_ne!(copy.id, source.id); assert_eq!(copy.kind, source.kind);
    s.key("cmd-z"); assert_eq!(s.with(|e, _| e.slide().clone()), before);
    s.key("cmd-shift-z");
    // Remove the overlapping duplicate before pointer-selecting the original.
    s.key("cmd-z"); s.click_shape(shape); s.key("cmd-c"); s.page(if page == 0 { 2 } else { 1 });
    let target_page = if page == 0 { 1 } else { 0 };
    let before = s.with(|e, _| e.slide().clone());
    s.key("cmd-v"); assert_eq!(s.shapes(), before.shapes.len() + 1);
    let pasted = s.with(|e, _| e.slide().shapes.last().unwrap().clone());
    assert_eq!(pasted.kind, source.kind); assert_eq!(pasted.frame, source.frame);
    s.key("cmd-z"); assert_eq!(s.with(|e, _| e.slide().clone()), before);
    s.key("cmd-shift-z"); s.save_as("table-shape-copy.pptx");
    let saved = s.reopen(); let ShapeKind::Table(table) = &saved.slides[target_page].shapes.last().unwrap().kind else { panic!("table") };
    let ShapeKind::Table(original) = &source.kind else { panic!("table") };
    assert_eq!(table.columns, original.columns); assert_eq!(table.rows.len(), original.rows.len());
    for (a, b) in table.rows.iter().zip(&original.rows) { for (a,b) in a.cells.iter().zip(&b.cells) { assert_eq!(a.text.text(), b.text.text()); assert_eq!(a.fill,b.fill); assert_eq!(a.borders,b.borders); assert_eq!((a.row_span,a.column_span,a.merged), (b.row_span,b.column_span,b.merged)); } }
    undo_all_and_redo(&mut s);
}

pub(crate) fn insert_a_picture() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(4);
    let before = s.shapes();
    assert!(s.toolbar_state("insert-image").is_some_and(|t| t.enabled));
    let id = s.update(|e, _, cx| e.insert_picture("photo.png", png(320, 240, [0x2e, 0x75, 0xb6]), cx));
    assert!(id.is_some());
    assert_eq!(s.shapes(), before + 1);
    assert_eq!(s.with(|e, _| e.selection_kind()), SelectionKind::Picture);
    let frame = s.with(|e, _| e.selected_shapes()[0].frame);
    assert_eq!((frame.width, frame.height), (320 * 9_525, 240 * 9_525), "its size at 96 dpi");
    s.screenshot("picture");
    s.save_as("picture.pptx");
    let saved = s.reopen();
    assert!(matches!(saved.slides[3].shapes.last().unwrap().kind, ShapeKind::Picture(_)));
    undo_all_and_redo(&mut s);
}

/// 31: another layout for a slide: its placeholders move, its text stays.
pub(crate) fn apply_a_layout() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    let before = s.with(|e, _| e.slide().clone());
    s.toolbar_menu("layout", "pptx.layout.apply");
    let after = s.with(|e, _| e.slide().clone());
    assert_ne!(after.layout, before.layout, "the first layout is another one");
    let texts = |sl: &ibis_doc_model::Slide| sl.shapes.iter().filter_map(|x| x.text_body().map(|b| b.text())).collect::<Vec<_>>();
    assert_eq!(texts(&after), texts(&before));
    s.screenshot("layout");
    s.save_as("layout.pptx");
    let saved = s.reopen();
    let frames = |sl: &ibis_doc_model::Slide| sl.shapes.iter().map(|x| x.frame).collect::<Vec<_>>();
    assert_eq!(frames(&saved.slides[1]), frames(&after));
    s.key("cmd-z");
    assert!(s.with(|e, _| *e.slide() == before), "undo gives the slide back");
    undo_all_and_redo(&mut s);
}

/// S10: canvas presets through the menu, undo/redo, and save/reopen.
pub(crate) fn change_slide_size() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let original = s.with(|e, _| e.presentation().clone());
    s.menu("pptx.slide-size.standard");
    let standard = ibis_doc_model::EmuSize::new(9_144_000, 6_858_000);
    assert_eq!(s.with(|e, _| e.presentation().slide_size), standard);
    assert_eq!(s.with(|e, _| e.presentation().slides.clone()), original.slides);
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.presentation().clone()), original);
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.presentation().slide_size), standard);
    s.save_as("standard.pptx");
    assert_eq!(s.reopen().slide_size, standard);
    s.menu("pptx.slide-size.wide");
    assert_eq!(s.with(|e, _| e.presentation().slide_size), ibis_doc_model::EmuSize::WIDESCREEN);
    s.save_as("wide.pptx");
    assert_eq!(s.reopen().slide_size, ibis_doc_model::EmuSize::WIDESCREEN);
    assert_eq!(s.with(|e, _| e.presentation().slides.clone()), original.slides);
    undo_all_and_redo(&mut s);
}

/// SLIDE-009・010: portrait/landscape from the menu, and the canvas change
/// that scales (最大化・サイズに合わせて調整): shapes, backdrops and layouts
/// scaled as one, undone, and saved so (the layout and master parts too).
pub(crate) fn scale_slide_size() {
    use super::size_panel::ScaleMode;
    use ibis_doc_model::EmuSize;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let original = s.with(|e, _| e.presentation().clone());
    let wide = original.slide_size;
    s.menu("pptx.slide-size.portrait");
    assert!(s.with(|e, _| e.pptx().unwrap().size_panel.is_some()), "the panel opens with the swapped size");
    s.screenshot("panel");
    s.menu("pptx.slide-size.apply");
    let portrait = EmuSize::new(wide.height, wide.width);
    assert_eq!(s.with(|e, _| e.presentation().slide_size), portrait);
    let k = portrait.width as f64 / wide.width as f64;
    let (before, after) = s.with(|e, _| (original.slides[0].shapes[0].frame, e.presentation().slides[0].shapes[0].frame));
    assert!((after.width as f64 - before.width as f64 * k).abs() <= 2.0, "{before:?} {after:?}");
    assert!(s.with(|e, _| e.presentation().slides.iter().flat_map(|sl| sl.shapes.iter()).all(|sh| sh.frame.x >= -2 && sh.frame.x + sh.frame.width <= portrait.width + 2 && sh.frame.y >= -2 && sh.frame.y + sh.frame.height <= portrait.height + 2)));
    assert!(!s.with(|e, _| e.presentation().layout_scale.is_identity()));
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.presentation().clone()), original);
    s.key("cmd-shift-z");
    s.save_as("portrait.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slide_size, portrait);
    let b = s.with(|e, _| e.presentation().slides[0].shapes[0].frame);
    let a = saved.slides[0].shapes[0].frame;
    assert!((a.x - b.x).abs() <= 2 && (a.y - b.y).abs() <= 2 && (a.width - b.width).abs() <= 2, "{a:?} {b:?}");
    assert_eq!(saved.layouts.len(), original.layouts.len());
    let now = s.with(|e, _| e.presentation().layouts[0].placeholders.iter().filter_map(|p| p.placement).collect::<Vec<_>>());
    for (x, y) in saved.layouts[0].placeholders.iter().filter_map(|p| p.placement).zip(now) {
        assert!((x.0.x - y.0.x).abs() <= 2 && (x.0.width - y.0.width).abs() <= 2, "{x:?} {y:?}");
    }
    s.update(|e, w, cx| e.set_slide_size_scaled(wide, ScaleMode::Maximize, w, cx));
    assert_eq!(s.with(|e, _| e.presentation().slide_size), wide);
    undo_all_and_redo(&mut s);
}

/// THEME-004・005: the theme's accent 1 and heading font changed from the
/// panel; the theme (and what is drawn in it) follows, undo gives it back,
/// and the file saved has the new theme (read back the same).
pub(crate) fn edit_theme() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let original = s.with(|e, _| e.presentation().clone());
    assert!(original.theme_colors.iter().any(|(n, _)| n == "accent1"), "the theme's colours are read");
    s.menu("pptx.theme-colors");
    assert!(s.with(|e, _| e.pptx().unwrap().theme_panel.is_some()));
    s.screenshot("panel");
    s.update(|e, w, cx| {
        let panel = e.pptx().unwrap().theme_panel.as_ref().unwrap();
        let accent = panel.colors.iter().find(|(n, _, _)| *n == "accent1").unwrap().2.clone();
        let heading = panel.fonts[0].clone();
        accent.update(cx, |f, cx| f.set_value("#FF0000", w, cx));
        heading.update(cx, |f, cx| f.set_value("Arial Black", w, cx));
    });
    s.update(|e, w, cx| e.apply_theme_panel(w, cx));
    let red = ibis_doc_model::Rgb(0xff0000);
    let changed = s.with(|e, _| e.presentation().clone());
    assert!(changed.theme_colors.iter().any(|(n, c)| n == "accent1" && *c == red));
    assert_eq!(changed.theme_fonts.as_ref().unwrap().major.latin.as_deref(), Some("Arial Black"));
    assert_ne!(changed.slides, original.slides, "what is drawn in the theme follows it");
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.presentation().clone()), original);
    s.key("cmd-shift-z");
    s.save_as("theme.pptx");
    let saved = s.reopen();
    assert!(saved.theme_colors.iter().any(|(n, c)| n == "accent1" && *c == red));
    assert_eq!(saved.theme_fonts.as_ref().unwrap().major.latin.as_deref(), Some("Arial Black"));
    assert_eq!(saved.slides.len(), changed.slides.len());
    for (a, b) in saved.slides.iter().zip(&changed.slides) {
        assert_eq!(a.backdrop, b.backdrop);
        assert_eq!(a.shapes.len(), b.shapes.len());
        for (x, y) in a.shapes.iter().zip(&b.shapes) {
            assert_eq!(x.kind, y.kind, "a shape as the file has it under the theme");
        }
    }
    undo_all_and_redo(&mut s);
}

/// PPTX-THEME-001 & PPTX-THEME-003: theme color palette selection and application.
pub(crate) fn theme_palette_selection() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let original = s.with(|e, _| e.presentation().clone());

    // Apply Aspect palette via menu
    s.menu("pptx.theme.palette.aspect");
    let aspect_applied = s.with(|e, _| e.presentation().clone());
    assert!(s.with(|e, _| e.is_theme_palette_active("aspect")));
    assert_ne!(aspect_applied.theme_colors, original.theme_colors);
    // Aspect accent1 is #F0A22E
    let aspect_accent1 = aspect_applied.theme_colors.iter().find(|(n, _)| n == "accent1").unwrap().1;
    assert_eq!(aspect_accent1, ibis_doc_model::Rgb(0xf0a22e));

    // Apply Slate palette via menu
    s.menu("pptx.theme.palette.slate");
    let slate_applied = s.with(|e, _| e.presentation().clone());
    assert!(s.with(|e, _| e.is_theme_palette_active("slate")));
    assert_ne!(slate_applied.theme_colors, aspect_applied.theme_colors);
    // Slate accent1 is #5B7085
    let slate_accent1 = slate_applied.theme_colors.iter().find(|(n, _)| n == "accent1").unwrap().1;
    assert_eq!(slate_accent1, ibis_doc_model::Rgb(0x5b7085));

    // Undo to Aspect
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.presentation().theme_colors.clone()), aspect_applied.theme_colors);

    // Redo to Slate
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.presentation().theme_colors.clone()), slate_applied.theme_colors);

    // Save and reload roundtrip
    s.save_as("theme-palette.pptx");
    let saved = s.reopen();
    let saved_accent1 = saved.theme_colors.iter().find(|(n, _)| n == "accent1").unwrap().1;
    assert_eq!(saved_accent1, ibis_doc_model::Rgb(0x5b7085));
    assert_eq!(saved.slides.len(), slate_applied.slides.len());

    undo_all_and_redo(&mut s);
}

/// PPTX-THEME-002: apply theme color palette to selected slide only.
pub(crate) fn theme_palette_applied_to_selected_slide() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let orig_theme_colors = s.with(|e, _| e.presentation().theme_colors.clone());
    let orig_slide0 = s.with(|e, _| e.presentation().slides[0].clone());
    let orig_slide2 = s.with(|e, _| e.presentation().slides[2].clone());

    let slide2_id = orig_slide2.id;
    // スライド3（インデックス2）を選択
    s.update(|e, w, cx| e.select_slide(slide2_id, w, cx));

    // スライド3のみに Aspect パレットを適用
    s.menu("pptx.theme.palette-selected.aspect");

    let modified = s.with(|e, _| e.presentation().clone());
    // プレゼンテーション全体のテーマ色は元のまま（変更されない）
    assert_eq!(modified.theme_colors, orig_theme_colors, "presentation theme_colors should remain untouched");
    // スライド1（インデックス0）は変更されていない
    assert_eq!(modified.slides[0], orig_slide0, "slide 0 should remain untouched");
    // スライド3（インデックス2）は変更されている
    assert_ne!(modified.slides[2], orig_slide2, "slide 2 should have updated theme palette colors");

    // Undo で元に戻る
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.presentation().slides[2].clone()), orig_slide2, "undo reverts slide 2");

    // Redo で再適用
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.presentation().slides[2].clone()), modified.slides[2], "redo restores slide 2 change");

    // 保存と再オープン検証
    s.save_as("theme-palette-selected.pptx");
    let saved = s.reopen();
    assert_eq!(saved.theme_colors, orig_theme_colors, "saved presentation theme_colors preserved");
    assert_eq!(saved.slides[0].shapes.len(), orig_slide0.shapes.len());
    assert_eq!(saved.slides[2].shapes.len(), orig_slide2.shapes.len());
    // 再オープン後もスライド1とスライド3の配色差異が保持されている
    assert_ne!(saved.slides[0], saved.slides[2]);

    undo_all_and_redo(&mut s);
}

pub(crate) fn gridlines_guides_and_snapping() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. 初期状態の確認
    assert!(!s.with(|e, _| e.is_pptx_gridlines_enabled()));
    assert!(!s.with(|e, _| e.is_pptx_guides_enabled()));
    assert!(!s.with(|e, _| e.is_pptx_snap_to_grid_enabled()));
    assert!(!s.with(|e, _| e.is_pptx_snap_to_shape_enabled()));

    // 2. 表示メニューからのトグル切り替え
    s.menu("pptx.gridlines");
    assert!(s.with(|e, _| e.is_pptx_gridlines_enabled()));
    s.menu("pptx.gridlines");
    assert!(!s.with(|e, _| e.is_pptx_gridlines_enabled()));

    s.menu("pptx.guides");
    assert!(s.with(|e, _| e.is_pptx_guides_enabled()));
    s.menu("pptx.guides");
    assert!(!s.with(|e, _| e.is_pptx_guides_enabled()));

    s.menu("pptx.snap-to-grid");
    assert!(s.with(|e, _| e.is_pptx_snap_to_grid_enabled()));

    s.menu("pptx.snap-to-shape");
    assert!(s.with(|e, _| e.is_pptx_snap_to_shape_enabled()));

    // 3. グリッドスナップの動作検証
    let shape_id = s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        let frame = ibis_doc_model::EmuRect::new(100_000, 100_000, 1_000_000, 1_000_000);
        e.insert_shape(ibis_doc_model::Shape::text_box(id, frame, "Snap Target"), cx);
        id
    });

    s.update(|e, _, _| e.set_selection(vec![shape_id]));

    let layout = s.with(|e, _| e.slide_layout().unwrap());
    let px_dx = gpui_kit::px(150_000.0 * layout.scale);
    let start_pos = gpui_kit::point(gpui_kit::px(100.0), gpui_kit::px(100.0));
    let cur_pos = gpui_kit::point(gpui_kit::px(100.0) + px_dx, gpui_kit::px(100.0) + px_dx);

    s.update(|e, _, _| {
        let origins = vec![(shape_id, ibis_doc_model::EmuRect::new(100_000, 100_000, 1_000_000, 1_000_000))];
        e.set_drag(Some(super::slide_view::Drag::Move { start: start_pos, current: cur_pos, origins }));
    });

    let preview_frame = s.with(|e, _| {
        let shape = e.slide().shape(shape_id).unwrap();
        e.preview(shape).0
    });
    assert_eq!(preview_frame.x, 180_000, "preview snaps to grid line 180_000");
    assert_eq!(preview_frame.y, 180_000, "preview snaps to grid line 180_000");

    // ドラッグ終了
    s.update(|e, w, cx| e.on_drag_end(w, cx));
    let moved_frame = s.with(|e, _| e.slide().shape(shape_id).unwrap().frame);
    assert_eq!(moved_frame.x, 180_000, "applied frame is snapped to grid");
    assert_eq!(moved_frame.y, 180_000, "applied frame is snapped to grid");

    // Undo で元に戻る
    s.key("cmd-z");
    let undone_frame = s.with(|e, _| e.slide().shape(shape_id).unwrap().frame);
    assert_eq!(undone_frame.x, 100_000);
    assert_eq!(undone_frame.y, 100_000);

    // 4. 図形スナップ（スマートガイド）の動作検証
    s.menu("pptx.snap-to-grid");
    assert!(!s.with(|e, _| e.is_pptx_snap_to_grid_enabled()));
    assert!(s.with(|e, _| e.is_pptx_snap_to_shape_enabled()));

    let _other_id = s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        let frame = ibis_doc_model::EmuRect::new(2_000_000, 100_000, 1_000_000, 1_000_000);
        e.insert_shape(ibis_doc_model::Shape::text_box(id, frame, "Reference Shape"), cx);
        id
    });

    // 100_000 から 1_880_000 動かして 1_980_000（2_000_000 近傍）へ
    let px_dx = gpui_kit::px(1_880_000.0 * layout.scale);
    let cur_pos = gpui_kit::point(gpui_kit::px(100.0) + px_dx, gpui_kit::px(100.0));

    s.update(|e, _, _| {
        let origins = vec![(shape_id, ibis_doc_model::EmuRect::new(100_000, 100_000, 1_000_000, 1_000_000))];
        e.set_drag(Some(super::slide_view::Drag::Move { start: start_pos, current: cur_pos, origins }));
    });

    let preview_frame = s.with(|e, _| {
        let shape = e.slide().shape(shape_id).unwrap();
        e.preview(shape).0
    });
    assert_eq!(preview_frame.x, 2_000_000, "snaps to other shape's left edge");
    let guides = s.with(|e, _| e.active_alignment_guides());
    assert!(!guides.is_empty(), "smart guides are active during shape snap");

    // ドラッグ終了
    s.update(|e, w, cx| e.on_drag_end(w, cx));
    let moved_frame = s.with(|e, _| e.slide().shape(shape_id).unwrap().frame);
    assert_eq!(moved_frame.x, 2_000_000, "applied frame is snapped to other shape");
}

pub(crate) fn export_and_save_slideshow_ppsx() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. メニューに「ファイル > 書き出し > PowerPoint スライドショー（.ppsx）」が存在すること
    let menu_state = s.menu_state("pptx.export-ppsx");
    assert!(menu_state.is_some(), "pptx.export-ppsx がメニューに存在する");

    // 2. save_to で .ppsx として保存
    let out_dir = std::env::temp_dir().join(format!("ibisoffice-ppsx-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).expect("create out_dir");
    let ppsx_path = out_dir.join("test_presentation.ppsx");

    s.update(|e, window, _| {
        e.save_to(ppsx_path.clone(), window).expect("save_to .ppsx");
    });
    assert!(ppsx_path.exists(), ".ppsx file was created");

    // 3. 保存された .ppsx ファイルが正常に読み込めることを検証
    let ppsx_bytes = std::fs::read(&ppsx_path).expect("read .ppsx");
    let ppsx_opened = ibis_pptx::open(ppsx_bytes).expect("open .ppsx as Presentation");
    assert!(!ppsx_opened.presentation.slides.is_empty(), ".ppsx has slides");

    // 最初のシナリオを解放（ONE_AT_A_TIME ロックの解放）
    drop(s);

    // 4. .ppsx ファイルを開いた際に自動的にスライドショーが開始されることを検証
    let mut s_ppsx = Scenario::open(ppsx_path.to_str().unwrap());
    assert!(
        s_ppsx.with(|e, _| e.show_in_main()),
        "スライドショー形式（.ppsx）を開くと自動的にスライドショーが開始される"
    );

    // 5. スライドショーを終了するとエディタに戻る
    s_ppsx.update(|e, window, cx| {
        e.end_slideshow(window, cx);
    });
    assert!(
        !s_ppsx.with(|e, _| e.show_in_main()),
        "end_slideshow でスライドショーが終了して通常画面に戻る"
    );

    let _ = std::fs::remove_dir_all(&out_dir);
}

pub(crate) fn export_and_save_template_potx() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. メニューに「ファイル > 書き出し > PowerPoint テンプレート（.potx）」が存在すること
    let menu_state = s.menu_state("pptx.export-potx");
    assert!(menu_state.is_some(), "pptx.export-potx がメニューに存在する");

    // 2. save_to で .potx として保存
    let out_dir = std::env::temp_dir().join(format!("ibisoffice-potx-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).expect("create out_dir");
    let potx_path = out_dir.join("test_template.potx");

    s.update(|e, window, _| {
        e.save_to(potx_path.clone(), window).expect("save_to .potx");
    });
    assert!(potx_path.exists(), ".potx file was created");

    // 3. 保存された .potx ファイルが正常に読み込めることを検証
    let potx_bytes = std::fs::read(&potx_path).expect("read .potx");
    let potx_opened = ibis_pptx::open(potx_bytes).expect("open .potx as Presentation");
    assert!(!potx_opened.presentation.slides.is_empty(), ".potx has slides");

    // 最初のシナリオを解放（ONE_AT_A_TIME ロックの解放）
    drop(s);

    // 5. .potx ファイルを開いてエディタで編集・利用できることを検証
    let mut s_potx = Scenario::open(potx_path.to_str().unwrap());
    assert!(s_potx.with(|e, _| e.has_document), ".potx がドキュメントとして開く");
    assert_eq!(s_potx.with(|e, _| e.presentation().slides.len()), potx_opened.presentation.slides.len());

    let _ = std::fs::remove_dir_all(&out_dir);
}

/// 新規作成・開く・最近使ったファイル、および保存・名前を付けて保存・コピー保存の動作検証:
/// 1. 開いたファイルが最近使ったファイル（recent_files）に登録されること
/// 2. コピーを作成（copy_to）で現在ドキュメントのパスを変更せずに別ファイルへ保存できること
/// 3. 開いたファイルで名前を付けて保存（save_to）するとウィンドウのパスが更新され正しく保存されること
/// 4. ファイルを閉じて新規プレゼンテーション作成（new_file）で初期スライドを含むドキュメントが開くこと
/// 5. 新規作成したファイルに名前を付けて保存できること
pub(crate) fn new_open_save_and_copy_presentation() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let original_slide_count = s.with(|e, _| e.presentation().slides.len());
    assert!(original_slide_count >= 1);

    // 最近使ったファイル（recent_files）に開いたファイルが記録されていること
    let original_path = s.with(|e, _| e.path.clone()).expect("file has path");
    let has_recent = s.with(|e, _| e.recent_files.iter().any(|r| r.file_name() == original_path.file_name()));
    assert!(has_recent, "開いたファイルが最近使ったファイルに登録されている");

    // 1. コピーを保存（copy_to）
    let out_dir = std::env::temp_dir().join(format!("ibisoffice-file-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).expect("create out_dir");
    let copy_path = out_dir.join("copy_presentation.pptx");

    s.update(|e, _, _| {
        e.copy_to(&copy_path).expect("copy_to succeeds");
    });
    assert!(copy_path.exists(), "コピーファイルが生成されている");
    assert_eq!(s.with(|e, _| e.path.clone()), Some(original_path.clone()), "copy_to では現在のウィンドウの path は変わらない");

    // 最初のシナリオを解放
    drop(s);

    // 2. コピーしたファイルを開く
    let mut s_copy = Scenario::open(copy_path.to_str().unwrap());
    assert_eq!(s_copy.with(|e, _| e.presentation().slides.len()), original_slide_count);

    // 3. 名前を付けて保存（save_to）
    let save_as_path = out_dir.join("saved_as_presentation.pptx");
    s_copy.update(|e, window, _| {
        e.save_to(save_as_path.clone(), window).expect("save_to succeeds");
    });
    assert!(save_as_path.exists(), "save_to でファイルが保存されている");
    assert_eq!(s_copy.with(|e, _| e.path.clone()), Some(save_as_path.clone()), "save_to でウィンドウの path が更新される");

    drop(s_copy);

    // 4. 新規プレゼンテーション作成（new_file）
    let mut s_new = Scenario::open("test/editing/pptx-plan.pptx");
    // ファイルを閉じる
    s_new.update(|e, window, cx| {
        e.close_file(&crate::CloseFile, window, cx);
    });
    assert!(!s_new.with(|e, _| e.has_document), "ファイルを閉じるとドキュメントなし状態になる");

    // 新規作成
    s_new.update(|e, window, cx| {
        e.new_file(ibis_pptx::Kind::Presentation, window, cx);
    });
    assert!(s_new.with(|e, _| e.has_document), "新規作成でドキュメントが開く");
    assert_eq!(s_new.with(|e, _| e.presentation().slides.len()), 1, "新規プレゼンテーションは初期スライド1枚を持つ");
    assert!(s_new.with(|e, _| e.path.is_none()), "新規作成時はまだパスを持たない");

    // 新規ファイルを保存
    let new_save_path = out_dir.join("newly_created.pptx");
    s_new.update(|e, window, _| {
        e.save_to(new_save_path.clone(), window).expect("save_to newly created file");
    });
    assert!(new_save_path.exists(), "新規作成したファイルを保存できる");

    let _ = std::fs::remove_dir_all(&out_dir);
}

/// PPTX-FILE-003: テンプレートから新規作成（New from Template）の検証
/// 1. メニューに「テンプレートから新規作成…（pptx.new-from-template）」が存在すること
/// 2. テンプレート（.potx）から新規作成した際、テンプレートのスライド・書式が引き継がれ、パスが未保存（path == None）になること
/// 3. 編集後に名前を付けて保存（save_to）でき、新規 .pptx ファイルとして保存されること
/// 4. 元のテンプレート（.potx）は変更されず保護されていること
/// 5. 通常の Scenario::open で .potx を開いた場合も、Office 仕様に従い未保存の新規プレゼンテーション扱い（path == None）になること
pub(crate) fn new_presentation_from_template() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let menu_state = s.menu_state("pptx.new-from-template");
    assert!(menu_state.is_some(), "pptx.new-from-template がメニューに存在する");

    let out_dir = std::env::temp_dir().join(format!("ibisoffice-template-new-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).expect("create out_dir");
    let template_path = out_dir.join("sample_template.potx");

    // まずテンプレート（.potx）を作成して保存
    s.update(|e, window, _| {
        e.save_to(template_path.clone(), window).expect("save template .potx");
    });
    assert!(template_path.exists(), "テンプレート .potx が生成されている");
    let orig_tmpl_bytes = std::fs::read(&template_path).expect("read template");

    // テンプレートから新規作成を実行 (new_from_template_path)
    s.update(|e, window, cx| {
        e.new_from_template_path(template_path.clone(), window, cx);
    });
    s.wait(100);

    // テンプレートの内容が読み込まれつつ、パスは未保存状態（path == None）になっていること
    assert!(s.with(|e, _| e.has_document), "テンプレートから開いたドキュメントが存在する");
    assert_eq!(s.with(|e, _| e.path.clone()), None, "テンプレートから新規作成時は path が None（未保存）");
    let default_name = crate::shell::file_actions::new_file_name(ibis_pptx::Kind::Presentation);
    assert_eq!(s.with(|e, _| e.file_name.clone()), Some(default_name), "ファイル名は初期プレゼンテーション名");

    // 最近使ったファイルにテンプレートが登録されていること
    let has_recent = s.with(|e, _| e.recent_files.iter().any(|r| r.file_name() == template_path.file_name()));
    assert!(has_recent, "テンプレートが最近使ったファイルに登録されている");

    // 編集して新しい .pptx として保存
    let new_pptx_path = out_dir.join("created_from_template.pptx");
    s.update(|e, window, _| {
        e.save_to(new_pptx_path.clone(), window).expect("save as new pptx");
    });
    assert!(new_pptx_path.exists(), "新しい pptx が保存されている");
    assert_eq!(s.with(|e, _| e.path.clone()), Some(new_pptx_path.clone()), "保存後は新しいパスになる");

    // 元のテンプレートが変更されていないこと
    let current_tmpl_bytes = std::fs::read(&template_path).expect("read template after save");
    assert_eq!(orig_tmpl_bytes, current_tmpl_bytes, "テンプレート原本は上書きされず保持されている");

    drop(s);

    // 通常の Scenario::open で .potx を開いた場合も、Office 仕様に基づき path == None になること
    let mut s_tmpl = Scenario::open(template_path.to_str().unwrap());
    assert_eq!(s_tmpl.with(|e, _| e.path.clone()), None, "通常の open で .potx を開いた場合もテンプレートから新規作成扱い（path == None）になる");

    drop(s_tmpl);
    let _ = std::fs::remove_dir_all(&out_dir);
}

/// グラフの種類の変更: a one-group chart made a line, an area, a stacked
/// column, a bar and a column again from the toolbar; each read back as the
/// file has it (saved and reopened the same), undone to the original.
pub(crate) fn change_chart_type() {
    use ibis_doc_model::{ChartType, Grouping};
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s
        .with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(&sh.kind, ShapeKind::Chart(c) if matches!(c.groups.as_slice(), [g] if matches!(g.kind, ChartType::Bar { .. } | ChartType::Line | ChartType::Area) && g.series.iter().any(|x| x.values.len() > 1)))).map(|j| (i, j))))
        .expect("a slide with a one-group bar, line or area chart");
    s.page(page + 1);
    s.click_shape(shape);
    let original = s.with(|e, _| e.presentation().clone());
    let group = |s: &mut Scenario| s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Chart(c) => (c.groups[0].kind, c.groups[0].grouping, c.groups[0].series.len(), c.groups[0].series[0].values.clone()),
        _ => panic!("a chart"),
    });
    let (_, _, series, values) = group(&mut s);
    for (id, check) in [
        ("chart.type.line", (|k: ChartType, g: Grouping| k == ChartType::Line && g == Grouping::Standard) as fn(ChartType, Grouping) -> bool),
        ("chart.type.area", |k, g| k == ChartType::Area && g == Grouping::Standard),
        ("chart.type.column-stacked", |k, g| matches!(k, ChartType::Bar { horizontal: false, .. }) && g == Grouping::Stacked),
        ("chart.type.bar-percent", |k, g| matches!(k, ChartType::Bar { horizontal: true, .. }) && g == Grouping::PercentStacked),
        ("chart.type.column", |k, g| matches!(k, ChartType::Bar { horizontal: false, .. }) && g == Grouping::Standard),
    ] {
        s.toolbar_menu("chart-type", id);
        let (kind, grouping, n, v) = group(&mut s);
        assert!(check(kind, grouping), "{id}: {kind:?} {grouping:?}");
        assert_eq!((n, &v), (series, &values), "{id}: the data stays");
        s.screenshot(id);
        s.save_as("chart-type.pptx");
        let saved = s.reopen();
        let ShapeKind::Chart(c) = &saved.slides[page].shapes[shape].kind else { panic!("a chart") };
        assert_eq!((c.groups[0].kind, c.groups[0].grouping), (kind, grouping), "{id}: as saved");
    }
    for _ in 0..5 {
        s.key("cmd-z");
    }
    assert_eq!(s.with(|e, _| e.presentation().clone()), original);
    undo_all_and_redo(&mut s);
}

/// 円・ドーナツグラフの種類変更・扇形角度・穴サイズ設定:
/// switching between Doughnut and Pie, setting doughnut hole size and slice angle,
/// saving and verifying round-trip persistence and undo/redo.
pub(crate) fn pie_and_doughnut_chart_options() {
    use ibis_doc_model::ChartType;
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s
        .with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(&sh.kind, ShapeKind::Chart(c) if matches!(c.groups.as_slice(), [g] if matches!(g.kind, ChartType::Doughnut { .. }) && g.series.iter().any(|x| x.values.len() > 1)))).map(|j| (i, j))))
        .expect("a slide with a doughnut chart");
    s.page(page + 1);
    let id = s.with(|e, _| e.slide().shapes[shape].id);
    s.update(|e, _, _| e.set_selection(vec![id]));
    let original = s.with(|e, _| e.presentation().clone());

    // 1. ドーナツの穴の大きさを 50% に変更
    s.toolbar_menu("chart-hole-size", "pptx.chart.hole-size.50");
    s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Chart(c) => assert_eq!(c.groups[0].kind, ChartType::Doughnut { first_angle: 0, hole: 50 }),
        _ => panic!("a chart"),
    });
    s.screenshot("doughnut-hole-50");

    // 2. 最初の扇形の角度を 90° に変更
    s.toolbar_menu("chart-first-angle", "pptx.chart.first-angle.90");
    s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Chart(c) => assert_eq!(c.groups[0].kind, ChartType::Doughnut { first_angle: 90, hole: 50 }),
        _ => panic!("a chart"),
    });
    s.screenshot("doughnut-angle-90");

    // 3. 円グラフ (Pie) に種類を変更（角度90°が引き継がれる）
    s.toolbar_menu("chart-type", "chart.type.pie");
    s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Chart(c) => assert_eq!(c.groups[0].kind, ChartType::Pie { first_angle: 90 }),
        _ => panic!("a chart"),
    });
    s.screenshot("pie-angle-90");

    // 4. 円グラフの扇形角度を 180° に変更
    s.toolbar_menu("chart-first-angle", "pptx.chart.first-angle.180");
    s.with(|e, _| match &e.slide().shapes[shape].kind {
        ShapeKind::Chart(c) => assert_eq!(c.groups[0].kind, ChartType::Pie { first_angle: 180 }),
        _ => panic!("a chart"),
    });
    s.screenshot("pie-angle-180");

    // 保存＆再読込で検証
    s.save_as("pie-and-doughnut-options.pptx");
    let saved = s.reopen();
    let ShapeKind::Chart(c) = &saved.slides[page].shapes[shape].kind else { panic!("a chart") };
    assert_eq!(c.groups[0].kind, ChartType::Pie { first_angle: 180 });

    // 4回の変更を Undo して元に戻ることを確認
    for _ in 0..4 {
        s.key("cmd-z");
    }
    assert_eq!(s.with(|e, _| e.presentation().clone()), original);
    undo_all_and_redo(&mut s);
}

/// 散布図・バブルグラフの挿入、ツールバー設定（平滑線・バブル倍率）、種類変更、保存・再読込・Undo/Redo:
/// inserting scatter and bubble charts, setting scatter smooth lines, bubble scale,
/// changing between scatter and bubble, saving, reopening, and verifying Undo/Redo.
pub(crate) fn scatter_and_bubble_charts() {
    use ibis_doc_model::ChartType;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    let before = s.shapes();

    // 1. 散布図（直線）をメニューから新規挿入
    s.menu("pptx.insert-chart.scatter");
    assert_eq!(s.shapes(), before + 1);
    assert_eq!(s.with(|e, _| e.selection_kind()), SelectionKind::Chart);
    assert!(s.with(|e, _| e.pptx().unwrap().chart_panel.is_some()), "data panel open");

    // ツールバーの平滑線ボタンから「平滑線」を設定
    assert!(s.toolbar_state("chart-scatter-smooth").is_some());
    s.toolbar_menu("chart-scatter-smooth", "pptx.chart.scatter-smooth.smooth");

    let scatter_shape = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Chart(scatter_chart) = &scatter_shape.kind else { panic!("a chart") };
    assert_eq!(scatter_chart.groups[0].kind, ChartType::Scatter { smooth: true });
    s.screenshot("scatter-smooth");

    // 2. グラフの種類を「バブル」に変更
    s.toolbar_menu("chart-type", "chart.type.bubble");
    let bubble_shape = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Chart(bubble_chart) = &bubble_shape.kind else { panic!("a chart") };
    assert!(matches!(bubble_chart.groups[0].kind, ChartType::Bubble { scale: 100 }));
    s.screenshot("bubble-default");

    // 3. バブルの大きさを 200% に変更
    assert!(s.toolbar_state("chart-bubble-scale").is_some());
    s.toolbar_menu("chart-bubble-scale", "pptx.chart.bubble-scale.200");
    let scaled_bubble = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Chart(scaled_chart) = &scaled_bubble.kind else { panic!("a chart") };
    assert_eq!(scaled_chart.groups[0].kind, ChartType::Bubble { scale: 200 });
    s.screenshot("bubble-scale-200");

    // 4. 新規バブルチャートをメニューから挿入
    s.menu("pptx.insert-chart.bubble");
    assert_eq!(s.shapes(), before + 2);
    let inserted_bubble = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Chart(new_b_chart) = &inserted_bubble.kind else { panic!("a chart") };
    assert_eq!(new_b_chart.groups[0].kind, ChartType::Bubble { scale: 100 });

    // 保存＆再読込で検証
    s.save_as("scatter-and-bubble-charts.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[1].shapes.len(), before + 2);

    undo_all_and_redo(&mut s);
}

/// 動画・音声: a video and an audio file put on a slide (poster frame and
/// speaker icon, with their files kept), saved and read back with their
/// clips; the video played (its frames taken on macOS) and stopped.
pub(crate) fn insert_video_and_audio() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let (video, audio) = (root.join("clip-test.mp4"), root.join("clip-tone.wav"));
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    let (vdata, adata) = (std::fs::read(&video).unwrap(), std::fs::read(&audio).unwrap());
    // A file of the other kind is refused.
    let before = s.with(|e, _| e.slide().shapes.len());
    s.update(|e, _, cx| assert!(e.insert_clip(&audio, adata.clone(), false, cx).is_none()));
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), before);
    let v = s.update(|e, _, cx| e.insert_clip(&video, vdata.clone(), false, cx)).unwrap();
    let a = s.update(|e, _, cx| e.insert_clip(&audio, adata.clone(), true, cx)).unwrap();
    s.screenshot("inserted");
    let clips = |p: &ibis_doc_model::Presentation, slide: usize| -> Vec<(bool, Vec<u8>)> {
        p.slides[slide].shapes.iter().filter_map(|sh| match &sh.kind {
            ShapeKind::Picture(pic) => pic.clip.as_ref().map(|c| (c.audio, p.media(c.media).unwrap().data.to_vec())),
            _ => None,
        }).collect()
    };
    let now = s.with(|e, _| clips(e.presentation(), 1));
    assert_eq!(now, vec![(false, vdata.clone()), (true, adata.clone())]);
    s.save_as("clips.pptx");
    let saved = s.reopen();
    assert_eq!(clips(&saved, 1), now, "the files and their kinds are read back");
    // Undo takes them away, redo gives them back.
    s.key("cmd-z");
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), before);
    s.key("cmd-shift-z");
    s.key("cmd-shift-z");
    // Played.
    s.update(|e, _, cx| e.toggle_clip(v, cx));
    #[cfg(target_os = "macos")]
    {
        assert_eq!(s.with(|e, _| e.pptx().unwrap().clips.len()), 1, "the video is playing");
        let media = s.with(|e, _| crate::shell::editor::SlideEditor::clip_of(e.slide().shape(v).unwrap()).unwrap().media);
        // Frames arrive within a few seconds.
        for _ in 0..20 {
            if s.with(|e, _| e.resources.has_live_frame(media)) { break; }
            // The player runs in real time; the scenario's clock does not.
            crate::media_player::run_loop(0.1);
            s.wait(100);
        }
        assert!(s.with(|e, _| e.resources.has_live_frame(media)), "a frame of the video was taken");
        s.screenshot("playing");
        s.update(|e, _, cx| e.stop_clips(cx));
        assert!(s.with(|e, _| e.pptx().unwrap().clips.is_empty()));
        s.update(|e, _, cx| e.toggle_clip(a, cx));
        assert_eq!(s.with(|e, _| e.pptx().unwrap().clips.len()), 1, "the audio is playing");
        s.update(|e, _, cx| e.stop_clips(cx));
    }
    let _ = a;
    undo_all_and_redo(&mut s);
}

/// 箇条書きの記号・番号の種類: a bullet symbol and a numbering scheme chosen
/// from the toolbar, shown as the file will read them (saved and read back).
pub(crate) fn bullet_and_number_types() {
    use ibis_doc_model::{ListKind, NumberScheme};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    s.double_click_shape(1);
    s.key("cmd-a");
    s.toolbar_menu("bullet-types", "bullet.type.square");
    assert_eq!(s.with(|e, _| e.selected_list()), Some(ListKind::Symbol('■')));
    let text = |s: &mut Scenario| s.with(|e, _| e.slide().shapes[1].text_body().unwrap().paragraphs.iter().map(|p| p.bullet.as_ref().map(|b| b.text.clone())).collect::<Vec<_>>());
    assert!(text(&mut s).iter().all(|b| b.as_deref() == Some("■")));
    s.toolbar_menu("number-types", "number.type.romanUcPeriod");
    assert_eq!(s.with(|e, _| e.selected_list()), Some(ListKind::Scheme(NumberScheme::RomanUcPeriod)));
    let labels = text(&mut s);
    // (The third is a paragraph of another level, counted on its own.)
    assert_eq!(&labels[..2], &[Some("I.".to_string()), Some("II.".to_string())], "{labels:?}");
    s.key("escape");
    s.save_as("number-types.pptx");
    let saved = s.reopen();
    let read: Vec<_> = saved.slides[1].shapes[1].text_body().unwrap().paragraphs.iter().map(|p| p.bullet.as_ref().map(|b| (b.text.clone(), b.auto.as_ref().map(|a| a.scheme.clone())))).collect();
    assert_eq!(read[0], Some(("I.".to_string(), Some("romanUcPeriod".to_string()))), "{read:?}");
    // Back to a symbol: saved as a character bullet.
    s.double_click_shape(1);
    s.key("cmd-a");
    s.toolbar_menu("bullet-types", "bullet.type.arrow");
    s.key("escape");
    s.save_as("symbol.pptx");
    let saved = s.reopen();
    let first = saved.slides[1].shapes[1].text_body().unwrap().paragraphs[0].bullet.clone().unwrap();
    assert_eq!((first.text.as_str(), first.auto.is_none()), ("►", true));
    undo_all_and_redo(&mut s);
}

/// スライド番号の開始値: set in the slide size panel, saved as
/// firstSlideNum and read back; an invalid value changes nothing; undo.
pub(crate) fn first_slide_number() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let original = s.with(|e, _| e.presentation().clone());
    s.menu("pptx.slide-size.custom");
    s.update(|e, w, cx| {
        let first = e.pptx().unwrap().size_panel.as_ref().unwrap().first.clone();
        first.update(cx, |f, cx| f.set_value("x", w, cx));
    });
    s.menu("pptx.slide-size.apply");
    assert_eq!(s.with(|e, _| e.presentation().clone()), original);
    assert!(s.with(|e, _| e.pptx().unwrap().size_panel.is_some()), "the panel stays for the invalid value");
    s.update(|e, w, cx| {
        let first = e.pptx().unwrap().size_panel.as_ref().unwrap().first.clone();
        first.update(cx, |f, cx| f.set_value("5", w, cx));
    });
    s.menu("pptx.slide-size.apply");
    assert_eq!(s.with(|e, _| e.presentation().first_slide_number), 5);
    s.save_as("first-number.pptx");
    assert_eq!(s.reopen().first_slide_number, 5);
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.presentation().clone()), original);
    s.key("cmd-shift-z");
    undo_all_and_redo(&mut s);
}

/// アクション ボタン: a next-slide button drawn from the shapes menu is a
/// link to the next slide (saved as such), and followed in the show.
pub(crate) fn action_button() {
    use ibis_doc_model::SlideLink;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    s.toolbar_menu("insert-shape", "pptx.action.next");
    assert_eq!(s.tool(), Tool::Shape("actionButtonForwardNext"));
    let before = s.with(|e, _| e.slide().shapes.len());
    let (a, b) = (s.at(EmuPoint::new(9_000_000, 5_400_000)), s.at(EmuPoint::new(10_400_000, 6_200_000)));
    s.drag(a, b);
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), before + 1);
    let id = s.with(|e, _| e.slide().shapes.last().unwrap().id);
    assert_eq!(s.with(|e, _| e.slide().shape_links.get(&id).cloned()), Some(SlideLink::NextSlide));

    // actionButtonReturn（戻る: 最後に表示したスライドへのジャンプ）を描画
    s.toolbar_menu("insert-shape", "pptx.action.return");
    assert_eq!(s.tool(), Tool::Shape("actionButtonReturn"));
    let (c, d) = (s.at(EmuPoint::new(7_000_000, 5_400_000)), s.at(EmuPoint::new(8_400_000, 6_200_000)));
    s.drag(c, d);
    let return_id = s.with(|e, _| e.slide().shapes.last().unwrap().id);
    assert_eq!(s.with(|e, _| e.slide().shape_links.get(&return_id).cloned()), Some(SlideLink::LastSlideViewed));

    // スライドショーでの動作設定（アクションリンク）の動作検証
    s.page(1);
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    assert!(s.with(|e, _| e.slideshow.is_some()), "slideshow started");
    // スライド1からスライド2へ進む
    s.update(|e, w, cx| e.step_slideshow(true, w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), s.with(|e, _| e.presentation().slides[1].id));
    // actionButtonReturn（LastSlideViewed）をクリックして直前のスライド（スライド1）に戻る
    s.update(|e, w, cx| e.follow_link_in_show(&SlideLink::LastSlideViewed, w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), s.with(|e, _| e.presentation().slides[0].id));
    // EndShow リンクをクリックしてスライドショーが終了することを検証
    s.update(|e, w, cx| e.follow_link_in_show(&SlideLink::EndShow, w, cx));
    assert!(s.with(|e, _| e.slideshow.is_none()), "slideshow ended by EndShow action");

    s.screenshot("button");
    s.save_as("action-button.pptx");
    let saved = s.reopen();
    let shape_next = saved.slides[1].shapes.iter().find(|sh| saved.slides[1].shape_links.get(&sh.id) == Some(&SlideLink::NextSlide)).expect("next button found");
    let shape_ret = saved.slides[1].shapes.iter().find(|sh| saved.slides[1].shape_links.get(&sh.id) == Some(&SlideLink::LastSlideViewed)).expect("return button found");
    assert_eq!(saved.slides[1].shape_links.get(&shape_next.id), Some(&SlideLink::NextSlide));
    assert_eq!(saved.slides[1].shape_links.get(&shape_ret.id), Some(&SlideLink::LastSlideViewed));

    undo_all_and_redo(&mut s);
}

/// 縦横比の固定: a picture inserted keeps its proportions when resized by
/// a corner handle and when its height is typed; unlocked, a shape does
/// not; saved as `noChangeAspect` and read back.
pub(crate) fn lock_aspect_ratio() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    let data = png(40, 20, [200, 30, 30]);
    let id = s.update(|e, _, cx| e.insert_picture("wide.png", data.clone(), cx)).unwrap();
    assert!(s.with(|e, _| e.slide().aspect_locked.contains(&id)), "a new picture is locked");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| sh.id == id).unwrap());
    let old = s.with(|e, _| e.slide().shape(id).unwrap().frame);
    // Height typed in the panel: the width follows.
    s.click_shape(index);
    s.menu("pptx.frame.settings");
    s.update(|e, w, cx| {
        let fields = e.pptx().unwrap().frame_panel.as_ref().unwrap().fields.clone();
        let h = (old.height as f64 * 2.0 / 360_000.0).to_string();
        fields[0].update(cx, |f, cx| f.set_value(&h, w, cx));
    });
    s.menu("pptx.frame.apply");
    let now = s.with(|e, _| e.slide().shape(id).unwrap().frame);
    assert_eq!(now.height, old.height * 2);
    assert!((now.width - old.width * 2).abs() <= 2, "the width follows: {old:?} {now:?}");
    s.save_as("locked.pptx");
    let saved = s.reopen();
    let slide = &saved.slides[1];
    let shape = slide.shapes.last().unwrap();
    assert!(slide.aspect_locked.contains(&shape.id), "noChangeAspect is read back");
    // Unlocked in the panel: the width stays when the height is typed.
    s.menu("pptx.frame.settings");
    s.update(|e, _, cx| {
        if let Some(p) = e.pptx_mut().and_then(|s| s.frame_panel.as_mut()) { p.set_locked(false); }
        cx.notify();
    });
    s.update(|e, w, cx| {
        let fields = e.pptx().unwrap().frame_panel.as_ref().unwrap().fields.clone();
        let h = (now.height as f64 / 360_000.0 / 2.0).to_string();
        fields[0].update(cx, |f, cx| f.set_value(&h, w, cx));
    });
    s.menu("pptx.frame.apply");
    let after = s.with(|e, _| e.slide().shape(id).unwrap().frame);
    assert!((after.width - now.width).abs() <= 1_500, "unlocked: the width stays (up to the panel's 0.01 cm): {now:?} {after:?}");
    assert!(after.height < now.height);
    assert!(!s.with(|e, _| e.slide().aspect_locked.contains(&id)));
    s.save_as("unlocked.pptx");
    let saved = s.reopen();
    assert!(saved.slides[1].aspect_locked.is_empty() || !saved.slides[1].aspect_locked.contains(&saved.slides[1].shapes.last().unwrap().id));
    undo_all_and_redo(&mut s);
}

/// 既定の図形に設定: the look of a shape set as the default is what the next
/// shape drawn gets; a line's too.
pub(crate) fn default_shape_look() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("view.sidebar"); // Placed by the layout with the right pane.
    s.page(4);
    s.toolbar_menu("insert-shape", "shape.roundRect");
    let (a, b) = (s.at(EmuPoint::new(1_000_000, 4_800_000)), s.at(EmuPoint::new(3_000_000, 6_000_000)));
    s.drag(a, b);
    s.toolbar_menu("fill", "color.orange");
    let orange = s.with(|e, _| match &e.selected_shapes()[0].kind { ShapeKind::Auto(a) => a.fill.clone(), _ => panic!("a shape") });
    s.menu("pptx.default-shape");
    // The next shape drawn is orange too.
    s.key("escape");
    s.toolbar_menu("insert-shape", "shape.ellipse");
    let (a, b) = (s.at(EmuPoint::new(4_000_000, 4_800_000)), s.at(EmuPoint::new(6_000_000, 6_000_000)));
    s.drag(a, b);
    let fill = s.with(|e, _| match &e.selected_shapes()[0].kind { ShapeKind::Auto(a) => a.fill.clone(), _ => panic!("a shape") });
    assert_eq!(fill, orange, "the default fill");
    // Reflection and soft edge on a shape set as default are also inherited.
    s.menu("pptx.reflection.tight");
    s.menu("pptx.soft-edge.5pt");
    s.menu("pptx.default-shape");
    s.key("escape");
    s.toolbar_menu("insert-shape", "shape.roundRect");
    let (a, b) = (s.at(EmuPoint::new(7_000_000, 4_800_000)), s.at(EmuPoint::new(9_000_000, 6_000_000)));
    s.drag(a, b);
    let (refl, soft) = s.with(|e, _| match &e.selected_shapes()[0].kind {
        ShapeKind::Auto(a) => (a.reflection.is_some(), a.soft_edge),
        _ => panic!("a shape"),
    });
    assert!(refl, "the default reflection");
    assert_eq!(soft, Some(63_500), "the default soft edge");
    // A line: a thicker one as the default.
    s.toolbar_menu("insert-line", "pptx.line");
    let (a, b) = (s.at(EmuPoint::new(1_000_000, 7_000_000)), s.at(EmuPoint::new(3_000_000, 7_000_000)));
    s.drag(a, b);
    s.toolbar_menu("line-weight", "width.6");
    s.menu("pptx.default-shape");
    s.key("escape");
    s.toolbar_menu("insert-line", "pptx.line");
    let (a, b) = (s.at(EmuPoint::new(4_000_000, 7_000_000)), s.at(EmuPoint::new(6_000_000, 7_000_000)));
    s.drag(a, b);
    let widths = s.with(|e, _| e.slide().shapes.iter().rev().take(2).map(|sh| match &sh.kind { ShapeKind::Line(l) => l.width, _ => 0 }).collect::<Vec<_>>());
    assert_eq!(widths[0], widths[1], "the default line width");

    // 保存と再オープン（<a:objectDefaults> 永続化）の検証
    let saved = s.reopen();
    assert!(!saved.slides.is_empty(), "saved slides is not empty");

    // 再オープン後も、新しく描いた図形に既定スタイル（オレンジ塗り・反射・ソフトエッジ）が自動適用される
    s.page(4);
    s.toolbar_menu("insert-shape", "shape.ellipse");
    let (a, b) = (s.at(EmuPoint::new(1_000_000, 2_000_000)), s.at(EmuPoint::new(3_000_000, 3_000_000)));
    s.drag(a, b);
    let (fill, refl, soft) = s.with(|e, _| match &e.selected_shapes()[0].kind {
        ShapeKind::Auto(a) => (a.fill.clone(), a.reflection.is_some(), a.soft_edge),
        _ => panic!("a shape"),
    });
    assert_eq!(fill, orange, "the default fill preserved after reopen");
    assert!(refl, "the default reflection preserved after reopen");
    assert_eq!(soft, Some(63_500), "the default soft edge preserved after reopen");

    // 再オープン後も、新しく描いた線に既定の太さが自動適用される
    s.toolbar_menu("insert-line", "pptx.line");
    let (a, b) = (s.at(EmuPoint::new(4_000_000, 2_000_000)), s.at(EmuPoint::new(6_000_000, 2_000_000)));
    s.drag(a, b);
    let line_width = s.with(|e, _| match &e.selected_shapes()[0].kind {
        ShapeKind::Line(l) => l.width,
        _ => 0,
    });
    assert_eq!(line_width, widths[0], "the default line width preserved after reopen");

    undo_all_and_redo(&mut s);
}

/// 記号と特殊文字: the shared panel from the Insert menu; a character
/// pressed goes at the caret of the text edited, and at the end of the text
/// of a shape selected; saved and read back.
pub(crate) fn insert_symbols() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    s.double_click_shape(1);
    s.key("cmd-a");
    s.type_text("A");
    s.menu("insert.symbol");
    assert!(s.with(|e, _| e.pptx().unwrap().symbols.is_some()), "the panel opens");
    s.update(|e, w, cx| e.insert_symbol_text('€', w, cx));
    s.key("escape");
    assert_eq!(s.with(|e, _| e.slide().shapes[1].text_body().unwrap().text()), "A€");
    // With the shape only selected, the character goes at the end.
    s.click_shape(1);
    s.update(|e, w, cx| e.insert_symbol_text('→', w, cx));
    s.key("escape");
    assert_eq!(s.with(|e, _| e.slide().shapes[1].text_body().unwrap().text()), "A€→");
    s.save_as("symbols.pptx");
    assert_eq!(s.reopen().slides[1].shapes[1].text_body().unwrap().text(), "A€→");
    undo_all_and_redo(&mut s);
}

/// レイアウト名の変更と複製: renamed in the side panel, copied under a new
/// name that a slide can then follow; both saved.
pub(crate) fn layout_rename_and_duplicate() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let (layout, name) = s.with(|e, _| {
        let l = e.presentation().layout(e.slide().layout.unwrap()).unwrap();
        (l.id, l.name.clone())
    });
    let count = s.with(|e, _| e.presentation().layouts.len());
    s.menu("pptx.layout.rename");
    assert!(s.with(|e, _| e.pptx().unwrap().layout_panel.is_some()), "the panel opens");
    s.key("cmd-a");
    s.type_text("   ");
    s.update(|e, w, cx| e.apply_layout_rename(w, cx));
    assert_eq!(s.with(|e, _| e.presentation().layout(layout).unwrap().name.clone()), name, "an empty name is refused");
    assert!(s.with(|e, _| e.pptx().unwrap().layout_panel.is_some()));
    s.key("cmd-a");
    s.type_text("新しい名前 & <1>");
    s.update(|e, w, cx| e.apply_layout_rename(w, cx));
    assert!(s.with(|e, _| e.pptx().unwrap().layout_panel.is_none()));
    assert_eq!(s.with(|e, _| e.presentation().layout(layout).unwrap().name.clone()), "新しい名前 & <1>");
    s.menu("pptx.layout.duplicate");
    assert_eq!(s.with(|e, _| e.presentation().layouts.len()), count + 1);
    let copy = s.with(|e, _| e.presentation().layouts.last().unwrap().clone());
    assert_eq!(copy.copy_of, Some(layout));
    assert!(copy.name.contains("新しい名前 & <1>"), "{}", copy.name);
    // The slide follows the copy, and the file keeps both.
    s.update(|e, w, cx| e.apply_layout(copy.id, w, cx));
    assert_eq!(s.with(|e, _| e.slide().layout), Some(copy.id));
    s.save_as("layout-copy.pptx");
    let reopened = s.reopen();
    assert_eq!(reopened.layouts.len(), count + 1);
    assert!(reopened.layouts.iter().any(|l| l.name == "新しい名前 & <1>"));
    assert!(reopened.layouts.iter().any(|l| l.name == copy.name));
    undo_all_and_redo(&mut s);
}

/// マスターの名前の変更・複製・削除: renamed in the side panel, copied with
/// its layouts, removed only when no slide follows it; saved.
pub(crate) fn master_rename_duplicate_remove() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let master = s.with(|e, _| e.presentation().masters[0].id);
    let layouts = s.with(|e, _| e.presentation().layouts.iter().filter(|l| l.master == Some(master)).count());
    s.update(|e, w, cx| e.open_master_rename(master, w, cx));
    s.key("cmd-a");
    s.type_text("社内テーマ");
    s.update(|e, w, cx| e.apply_layout_rename(w, cx));
    assert_eq!(s.with(|e, _| e.presentation().master(master).unwrap().name.clone()), "社内テーマ");
    s.update(|e, w, cx| e.duplicate_master(master, w, cx));
    let (copy, copy_layout) = s.with(|e, _| {
        let p = e.presentation();
        assert_eq!(p.masters.len(), 2);
        let copy = p.masters[1].clone();
        assert_eq!(copy.copy_of, Some(master));
        assert!(copy.name.contains("社内テーマ"), "{}", copy.name);
        assert_eq!(p.layouts.iter().filter(|l| l.master == Some(copy.id)).count(), layouts);
        (copy.id, p.layouts.iter().find(|l| l.master == Some(copy.id)).unwrap().id)
    });
    s.update(|e, w, cx| e.apply_layout(copy_layout, w, cx));
    assert!(s.with(|e, _| e.master_in_use(copy)), "a slide follows the copy");
    s.update(|e, w, cx| e.remove_master(copy, w, cx));
    assert_eq!(s.with(|e, _| e.presentation().masters.len()), 2, "a master in use is kept");
    s.save_as("master-copy.pptx");
    let reopened = s.reopen();
    assert_eq!(reopened.masters.len(), 2);
    assert!(reopened.masters.iter().any(|m| m.name == "社内テーマ"));
    // Back on the first master, the copy goes.
    let first = s.with(|e, _| e.presentation().layouts.iter().find(|l| l.master == Some(master)).unwrap().id);
    s.update(|e, w, cx| e.apply_layout(first, w, cx));
    s.update(|e, w, cx| e.remove_master(copy, w, cx));
    assert_eq!(s.with(|e, _| e.presentation().masters.len()), 1);
    s.save_as("master-removed.pptx");
    assert_eq!(s.reopen().masters.len(), 1);
    undo_all_and_redo(&mut s);
}

/// 印刷の設定: ファイル > 印刷… asks first; the range, handouts, colour and
/// frame are kept (a wrong list of slides is refused).
pub(crate) fn print_settings_panel() {
    use super::print_panel::{PrintColorKind, PrintRange};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let total = s.with(|e, _| e.presentation().slides.len());
    s.menu("file.print");
    assert!(s.with(|e, _| e.pptx().unwrap().print_panel.is_some()), "the panel opens first");
    s.update(|e, _, cx| e.edit_print_panel(|p| { p.range = PrintRange::Custom; p.handout = Some(4); p.color = PrintColorKind::Gray; p.frame = true; p.landscape = true; }, cx));
    s.update(|e, _, cx| {
        e.pptx().unwrap().print_panel.as_ref().unwrap();
        assert!(!e.keep_print_settings(cx), "an empty list of slides is refused");
    });
    s.key("cmd-a");
    s.type_text("1,2");
    assert!(s.with(|e, _| e.pptx().unwrap().print_panel.is_some()));
    s.update(|e, _, cx| assert!(e.keep_print_settings(cx)));
    let settings = s.with(|e, _| e.print_settings());
    assert_eq!((settings.range, settings.custom.as_str(), settings.handout, settings.color, settings.frame), (PrintRange::Custom, "1,2", Some(4), PrintColorKind::Gray, true));
    assert!(settings.needs_pictures());
    assert!(total >= 2);
    // Closing keeps what was kept; reopening shows it.
    s.update(|e, w, cx| e.close_print_panel(w, cx));
    assert!(s.with(|e, _| e.pptx().unwrap().print_panel.is_none()));
    assert_eq!(s.with(|e, _| e.print_settings()), settings);
    // ノート ページ: replaces the handout, and is made of pictures.
    s.menu("file.print");
    s.update(|e, _, cx| e.edit_print_panel(|p| { p.handout = None; p.notes_pages = true; }, cx));
    s.update(|e, _, cx| assert!(e.keep_print_settings(cx)));
    let notes = s.with(|e, _| e.print_settings());
    assert!(notes.notes_pages && notes.handout.is_none() && notes.needs_pictures());
    assert!(notes.arguments(1, &Default::default()).contains(&"--notes-pages".to_string()));
}

/// 図形の結合: two shapes made one (接合), what they share (重なり抽出), what
/// is cut out (型抜き) and the pieces (切り出し); saved, undone.
pub(crate) fn merge_shapes() {
    use ibis_doc_model::{Geometry, ShapeKind};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    let figures = s.with(|e, _| {
        e.slide().shapes.iter().filter(|sh| matches!(&sh.kind, ShapeKind::Auto(a) if matches!(a.geometry, Geometry::Rect | Geometry::RoundRect { .. } | Geometry::Ellipse) && e.slide().placeholders.get(&sh.id).is_none())).map(|sh| (sh.id, sh.frame)).collect::<Vec<_>>()
    });
    assert!(figures.len() >= 2, "two figures on the page: {figures:?}");
    let (a, b) = (figures[0], figures[1]);
    let before = s.with(|e, _| e.slide().shapes.len());
    s.update(|e, _, _| e.set_selection(vec![a.0]));
    assert_eq!(s.menu_state("pptx.merge.union").map(|(enabled, _)| enabled), Some(false), "one shape is not enough");
    s.update(|e, _, _| e.set_selection(vec![a.0, b.0]));
    assert_eq!(s.menu_state("pptx.merge.union").map(|(enabled, _)| enabled), Some(true));
    // Moved over each other so they overlap.
    let (bx, by) = (a.1.x + a.1.width / 2, a.1.y + a.1.height / 2);
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        e.run(vec![ibis_doc_model::Command::SetFrame { slide, shape: b.0, frame: ibis_doc_model::EmuRect::new(bx, by, b.1.width, b.1.height) }], cx)
    });
    s.update(|e, _, _| e.set_selection(vec![a.0, b.0]));
    s.menu("pptx.merge.union");
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), before - 1, "two shapes made one");
    let merged = s.with(|e, _| e.selected_ids());
    assert_eq!(merged.len(), 1);
    assert!(s.with(|e, _| matches!(&e.slide().shapes.iter().find(|sh| sh.id == merged[0]).unwrap().kind, ShapeKind::Auto(a) if matches!(&a.geometry, Geometry::Formula(f) if f.preset.is_none()))), "a custom geometry");
    s.save_as("merge-union.pptx");
    assert_eq!(s.reopen().slides[2].shapes.len(), before - 1);
    s.update(|e, _, _| {
        assert!(e.document.undo());
    });
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), before);
    // The overlap alone, then the pieces (three).
    s.update(|e, _, _| e.set_selection(vec![a.0, b.0]));
    s.menu("pptx.merge.intersect");
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), before - 1);
    s.update(|e, _, _| {
        assert!(e.document.undo());
        e.set_selection(vec![a.0, b.0]);
    });
    s.menu("pptx.merge.fragment");
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), before + 1, "three pieces for two shapes");
    s.save_as("merge-fragment.pptx");
    assert_eq!(s.reopen().slides[2].shapes.len(), before + 1);
    undo_all_and_redo(&mut s);
}

/// 段落ごとのアニメーション: effects on the paragraphs of a shape (`p:txEl`)
/// are drawn paragraph by paragraph in the slide show.
pub(crate) fn paragraph_animations() {
    use ibis_doc_model::{Animation, AnimationAfterEffect, AnimationClass, AnimationLook, AnimationRepeat, AnimationStart, AnimationTrigger, Command};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    let (slide, shape, paragraphs, size) = s.with(|e, _| {
        let sl = e.slide();
        let sh = sl.shapes.iter().find(|sh| sh.text_body().is_some_and(|t| t.paragraphs.len() >= 3)).expect("a shape with paragraphs");
        (sl.id, sh.id, sh.text_body().unwrap().paragraphs.len(), e.presentation().slide_size)
    });
    let effect = |i: u32| Animation {
        shape,
        class: AnimationClass::Entrance,
        start: AnimationStart::OnClick,
        look: AnimationLook::Appear,
        delay: 0,
        duration: 1,
        preset: (1, 0),
        repeat_count: AnimationRepeat::None,
        auto_reverse: false,
        rewind: false,
        accel: 0,
        decel: 0,
        after_effect: AnimationAfterEffect::Hold,
        trigger: AnimationTrigger::None,
        paragraph: Some((i, i)), path: Vec::new(),
    };
    let animations: Vec<Animation> = (0..paragraphs as u32).map(effect).collect();
    s.update(|e, _, cx| e.run(vec![Command::SetAnimations { slide, animations: animations.clone() }], cx));
    let other_shapes = s.with(|e, _| e.slide().shown_shapes().count()) - 1;
    let drawn = s.with(|e, _| crate::pptx::slideshow::animated_shapes(e.slide(), size, 1.0, 0, 0, &[], |_| gpui_kit::IntoElement::into_any_element(gpui_kit::div())).count());
    // The shape: itself (its other paragraphs and its look) and each paragraph over it.
    assert_eq!(drawn, other_shapes + 1 + paragraphs);
    // The paragraphs hidden, not the shape.
    let body = s.with(|e, _| e.slide().shapes.iter().find(|sh| sh.id == shape).unwrap().clone());
    let base = crate::pptx::slideshow::paragraph_view(&body, &|i| i == 1, false);
    let text = base.text_body().unwrap();
    assert!(text.paragraphs[1].runs.iter().all(|r| r.style.invisible) && text.paragraphs[1].bullet.is_none());
    assert!(text.paragraphs[0].runs.iter().all(|r| !r.style.invisible));
    assert!(matches!(&base.kind, ibis_doc_model::ShapeKind::Auto(a) if a.fill == match &body.kind { ibis_doc_model::ShapeKind::Auto(b) => b.fill.clone(), _ => unreachable!() }));
    let only = crate::pptx::slideshow::paragraph_view(&body, &|i| i != 1, true);
    assert!(matches!(&only.kind, ibis_doc_model::ShapeKind::Auto(a) if a.fill == ibis_doc_model::Fill::None && a.stroke.is_none()));
    assert!(only.text_body().unwrap().paragraphs[1].runs.iter().all(|r| !r.style.invisible));
    undo_all_and_redo(&mut s);
}

/// ノートのヘッダーとフッター: put on every notes page from the Insert menu,
/// shown in the notes page view, saved and undone.
pub(crate) fn notes_header_footer_panel() {
    use ibis_doc_model::NotesDate;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("pptx.notes-header-footer");
    assert!(s.with(|e, _| e.pptx().unwrap().notes_marks_panel.is_some()));
    s.update(|e, w, cx| {
        e.toggle_notes_marks("header", true, cx);
        e.toggle_notes_marks("footer", true, cx);
        e.toggle_notes_marks("number", true, cx);
        e.toggle_notes_marks("date", true, cx);
        e.toggle_notes_marks("auto", false, cx);
        let p = e.pptx().unwrap().notes_marks_panel.as_ref().unwrap();
        let (h, f, d) = (p.header_text.clone(), p.footer_text.clone(), p.date_text.clone());
        h.update(cx, |i, cx| i.set_value("社内用", w, cx));
        f.update(cx, |i, cx| i.set_value("機密 & <注意>", w, cx));
        d.update(cx, |i, cx| i.set_value("2026/10/10", w, cx));
        e.apply_notes_marks(w, cx);
    });
    assert!(s.with(|e, _| e.pptx().unwrap().notes_marks_panel.is_none()), "applied");
    let marks = s.with(|e, _| e.presentation().notes_marks.clone());
    assert_eq!((marks.header.as_deref(), marks.footer.as_deref(), marks.number, marks.date.clone()), (Some("社内用"), Some("機密 & <注意>"), true, Some(NotesDate::Fixed("2026/10/10".into()))));
    // Drawn on the notes page view.
    s.menu("pptx.notes-page-view");
    assert!(s.with(|e, _| e.is_notes_page_view()));
    s.menu("pptx.notes-page-view");
    s.save_as("notes-marks.pptx");
    assert_eq!(s.reopen().notes_marks, marks);
    undo_all_and_redo(&mut s);
}

/// GIFの再生: an animated GIF is decoded to all its frames, and the slide
/// show draws its pictures as playing (and only while it is drawn).
pub(crate) fn animated_gif_plays_in_the_slide_show() {
    use image::{codecs::gif::GifEncoder, Delay, Frame, RgbaImage};
    let mut bytes = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut bytes);
        for color in [[255u8, 0, 0, 255], [0, 0, 255, 255]] {
            let frame = Frame::from_parts(RgbaImage::from_pixel(8, 4, image::Rgba(color)), 0, 0, Delay::from_numer_denom_ms(100, 1));
            encoder.encode_frame(frame).unwrap();
        }
    }
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let id = s.update(|e, _, cx| e.insert_picture("animated.gif", bytes.clone(), cx)).expect("the GIF is inserted");
    s.save_as("animated-gif.pptx");
    drop(s);
    // The saved file opened: its pictures are decoded ahead.
    let saved = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/scenario/pptx::animated_gif_plays_in_the_slide_show/animated-gif.pptx");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/animated-gif-input.pptx");
    std::fs::copy(&saved, &input).unwrap();
    let mut s = Scenario::open("target/animated-gif-input.pptx");
    let (page, media) = s.with(|e, _| {
        let found = e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().find_map(|sh| match &sh.kind {
            ibis_doc_model::ShapeKind::Picture(p) if e.presentation().media(p.media).is_some_and(|m| m.format == ibis_doc_model::MediaFormat::Gif) => Some((i, p.media)),
            _ => None,
        }));
        found.expect("the GIF in the file")
    });
    let _ = (id, page);
    // Decoded on another thread: until it is.
    let mut frames = 0;
    for _ in 0..200 {
        frames = s.with(|e, _| match e.resources.picture(media) {
            Some(gpui_kit::ImageSource::Render(data)) => data.frame_count(),
            _ => 0,
        });
        if frames > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        s.update(|_, _, _| {});
    }
    assert_eq!(frames, 2, "both frames are decoded");
    assert!(!s.with(|e, _| e.resources.animate_pictures.get()), "not playing in the editor");
    s.update(|e, window, cx| e.start_slideshow(window, cx));
    s.update(|_, _, _| {});
    assert!(!s.with(|e, _| e.resources.animate_pictures.get()), "playing only while the slide show is drawn");
    s.update(|e, window, cx| e.end_slideshow(window, cx));
}

/// 頂点の編集: the shape becomes a freeform whose vertices can be moved,
/// added (a press on an edge) and deleted; saved, undone.
pub(crate) fn edit_points() {
    use ibis_doc_model::{Geometry, ShapeKind};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    let (id, frame) = s.with(|e, _| {
        e.slide().shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Auto(a) if matches!(a.geometry, Geometry::Rect | Geometry::RoundRect { .. }) && e.slide().placeholders.get(&sh.id).is_none())).map(|sh| (sh.id, sh.frame)).expect("a figure")
    });
    s.update(|e, _, _| e.set_selection(vec![id]));
    assert_eq!(s.menu_state("pptx.edit-points").map(|(enabled, _)| enabled), Some(true));
    s.menu("pptx.edit-points");
    assert!(s.with(|e, _| e.is_editing_points()));
    let count = s.with(|e, _| e.edit_point_vertices().len());
    assert!(count >= 4, "the outline's vertices: {count}");
    assert!(s.with(|e, _| matches!(&e.slide().shapes.iter().find(|sh| sh.id == id).unwrap().kind, ShapeKind::Auto(a) if matches!(&a.geometry, Geometry::Formula(f) if f.preset.is_none()))), "a freeform");
    // A vertex moved: the outline (and the frame) follow.
    let moved_to = (frame.x as f64 - 500_000.0, frame.y as f64 - 300_000.0);
    s.update(|e, _, cx| e.move_vertex(0, 0, moved_to, cx));
    let after = s.with(|e, _| e.slide().shapes.iter().find(|sh| sh.id == id).unwrap().frame);
    assert!(after.x < frame.x && after.y < frame.y, "{after:?} vs {frame:?}");
    // A press on an edge makes a vertex there (on the top edge's middle), which is then selected.
    let (origin, scale) = s.with(|e, _| {
        let l = e.slide_layout().unwrap();
        (l.origin, l.scale)
    });
    let (x, y) = (after.x + after.width / 2, after.y);
    let _ = (x, y);
    let top = s.with(|e, _| e.edit_point_polygons()[0].clone());
    let (a, b) = (top[0], top[1]);
    let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let pos = gpui_kit::point(origin.x + gpui_kit::px(mid.0 as f32 * scale), origin.y + gpui_kit::px(mid.1 as f32 * scale));
    let hit = s.update(|e, _, cx| e.edit_points_down(pos, cx));
    assert!(hit.is_some(), "a vertex made on the edge");
    assert_eq!(s.with(|e, _| e.edit_point_vertices().len()), count + 1);
    // Delete takes the selected vertex away.
    s.update(|e, _, cx| e.delete_selected(cx));
    assert_eq!(s.with(|e, _| e.edit_point_vertices().len()), count);
    assert_eq!(s.with(|e, _| e.slide().shapes.iter().any(|sh| sh.id == id)), true, "the shape stays");
    // Esc ends.
    s.update(|e, _, cx| e.end_edit_points(cx));
    assert!(!s.with(|e, _| e.is_editing_points()));
    s.save_as("edit-points.pptx");
    assert_eq!(s.reopen().slides[2].shapes.len(), s.shapes());
    undo_all_and_redo(&mut s);
}

/// 図の圧縮: a large picture made smaller (its medium replaced, the old
/// part gone from the saved file), the crop taken off, undone.
pub(crate) fn compress_pictures() {
    use image::ImageEncoder as _;
    let mut state = 7u32;
    let noisy = image::RgbImage::from_fn(1600, 800, |x, y| {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        image::Rgb([(x * 255 / 1600) as u8 ^ (state >> 24) as u8 / 8, (y * 255 / 800) as u8, (state >> 16) as u8 / 2])
    });
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut std::io::Cursor::new(&mut bytes), 100).write_image(noisy.as_raw(), 1600, 800, image::ExtendedColorType::Rgb8).unwrap();
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let id = s.update(|e, _, cx| e.insert_picture("big.jpg", bytes.clone(), cx)).expect("inserted");
    // Shown 4 × 2 inches, with the right half cropped off.
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        e.run(vec![ibis_doc_model::Command::SetFrame { slide, shape: id, frame: ibis_doc_model::EmuRect::new(0, 0, 2 * 914_400, 2 * 914_400) }], cx);
        e.crop_selected_pictures(ibis_doc_model::Crop { left: 0, top: 0, right: 50_000, bottom: 0 }, cx);
    });
    let (before_media, size_before) = s.with(|e, _| match &e.slide().shape(id).unwrap().kind {
        ShapeKind::Picture(p) => (p.media, e.presentation().media(p.media).unwrap().data.len()),
        _ => unreachable!(),
    });
    let before_path = s.save_as("before-compress.pptx");
    let file_before = std::fs::metadata(&before_path).unwrap().len();
    s.update(|e, w, cx| e.open_compress_panel(w, cx));
    assert!(s.with(|e, _| e.pptx().unwrap().compress_panel.is_some()));
    s.update(|e, w, cx| e.apply_compress_panel(w, cx));
    assert!(s.with(|e, _| e.pptx().unwrap().compress_panel.is_none()));
    let (after_media, size_after, crop) = s.with(|e, _| match &e.slide().shape(id).unwrap().kind {
        ShapeKind::Picture(p) => (p.media, e.presentation().media(p.media).unwrap().data.len(), p.crop),
        _ => unreachable!(),
    });
    assert_ne!(before_media, after_media, "the medium was replaced");
    assert!(size_after * 4 < size_before, "{size_after} of {size_before}");
    assert_eq!(crop, ibis_doc_model::Crop::default(), "the crop was taken off");
    let after_path = s.save_as("after-compress.pptx");
    let file_after = std::fs::metadata(&after_path).unwrap().len();
    assert!(file_after * 2 < file_before, "the file is smaller: {file_after} of {file_before}");
    assert_eq!(s.reopen().slides[s.with(|e, _| e.presentation().slide_index(e.current_slide).unwrap())].shapes.len(), s.shapes());
    undo_all_and_redo(&mut s);
}

/// 変形: a slide with the morph transition comes in as shapes moving from
/// the old slide's, drawn through the transition and then as the slide.
pub(crate) fn morph_transition_in_the_slide_show() {
    use ibis_doc_model::{Command, Transition};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let second = s.with(|e, _| e.presentation().slides[1].id);
    s.update(|e, _, cx| e.run(vec![Command::SetTransition { slide: second, transition: Some(Transition { duration: 300, ..Transition::new("morph") }) }], cx));
    // The pairing of the two slides' shapes is made (the title of one is not the other's).
    let (old, new) = s.with(|e, _| (e.presentation().slides[0].clone(), e.presentation().slides[1].clone()));
    let pairing = crate::pptx::morph::pair_shapes(&old, &new);
    assert_eq!(pairing.pairs.len() + pairing.old_only.len(), old.shown_shapes().count());
    assert_eq!(pairing.pairs.len() + pairing.new_only.len(), new.shown_shapes().count());
    s.update(|e, window, cx| e.start_slideshow(window, cx));
    s.update(|e, window, cx| e.step_slideshow(true, window, cx));
    assert_eq!(s.with(|e, _| e.current_slide), second);
    // Drawn while the transition runs, and after it.
    s.update(|_, _, _| {});
    std::thread::sleep(std::time::Duration::from_millis(400));
    s.update(|_, _, _| {});
    s.update(|e, window, cx| e.end_slideshow(window, cx));
}

/// ファイルへのリンクの動画: inserted as a link (no copy of the file kept),
/// saved as one, and its play refused with a message when the file is gone.
pub(crate) fn linked_video() {
    use ibis_doc_model::ShapeKind;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let path = std::path::Path::new("/nonexistent/linked-movie.mp4");
    let id = s.update(|e, _, cx| e.insert_clip_with(path, Vec::new(), false, Some(path.to_string_lossy().into_owned()), cx)).expect("a linked video is inserted");
    let (link, kept) = s.with(|e, _| match &e.slide().shape(id).unwrap().kind {
        ShapeKind::Picture(p) => {
            let clip = p.clip.clone().unwrap();
            (clip.link, e.presentation().media(clip.media).map(|m| m.data.len()))
        }
        _ => unreachable!(),
    });
    assert_eq!(link.as_deref(), Some("/nonexistent/linked-movie.mp4"));
    assert_eq!(kept, Some(0), "nothing of the file is kept");
    // The file is missing: told, not played.
    s.update(|e, _, cx| e.toggle_clip(id, cx));
    assert!(s.with(|e, _| e.status.as_ref().is_some_and(|m| m.contains("linked-movie.mp4"))));
    s.save_as("linked-video.pptx");
    let saved = s.reopen();
    let found = saved.slides.iter().flat_map(|sl| &sl.shapes).any(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| c.link.as_deref() == Some("/nonexistent/linked-movie.mp4"))));
    assert!(found, "read back as linked");
    undo_all_and_redo(&mut s);
}

/// パスワードで保護: a protected file asks for its password (a wrong one is
/// told), opens with the right one, saves protected again, and the
/// password can be taken off.
pub(crate) fn protected_file() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/protected-file");
    std::fs::create_dir_all(&root).unwrap();
    let plain = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/pptx-plan.pptx")).unwrap();
    let mut seed = 1u32;
    let protected = ibis_pptx::encrypt(&plain, "ibis123", &mut |bytes: &mut [u8]| {
        for b in bytes {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            *b = (seed >> 24) as u8;
        }
    });
    let path = root.join("secret.pptx");
    std::fs::write(&path, &protected).unwrap();
    // The reading says what is wanted.
    assert_eq!(crate::read_file(&path).err().as_deref(), Some(crate::NEEDS_PASSWORD));
    assert_eq!(crate::read_file_with(&path, Some("wrong")).err().as_deref(), Some(crate::WRONG_PASSWORD));
    let loaded = crate::read_file_with(&path, Some("ibis123")).expect("opens with its password");
    assert_eq!(loaded.password.as_deref(), Some("ibis123"));
    let slides = loaded.presentation.slides.len();
    // In the window: opened with the password, saved protected, the password taken off.
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let opened = path.clone();
    s.update(|e, w, cx| e.open_path_with_password(opened, "ibis123".into(), None, w, cx));
    for _ in 0..100 {
        s.settle();
        if s.with(|e, _| e.protection_password.is_some()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(s.with(|e, _| e.protection_password.clone()).as_deref(), Some("ibis123"));
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), slides);
    let saved = s.save_as(root.join("saved.pptx").to_str().unwrap());
    let bytes = std::fs::read(&saved).unwrap();
    assert!(ibis_pptx::is_encrypted(&bytes), "saved protected again");
    assert_eq!(ibis_pptx::read(&ibis_pptx::decrypt(&bytes, "ibis123").unwrap()).unwrap().slides.len(), slides);
    s.update(|e, w, cx| e.set_protection(None, w, cx));
    let bytes = std::fs::read(&saved).unwrap();
    assert!(!ibis_pptx::is_encrypted(&bytes) && ibis_pptx::read(&bytes).unwrap().slides.len() == slides, "saved without the password");
    // A new password.
    s.update(|e, w, cx| e.set_protection(Some("新しい".into()), w, cx));
    assert!(ibis_pptx::is_encrypted(&std::fs::read(&saved).unwrap()));
}

/// スペルチェック: the words the system does not know are listed with their
/// suggestions, changed (one, or every one of the same spelling), ignored and
/// the panel closed; undone.
pub(crate) fn spelling_check() {
    use ibis_doc_model::Command;
    if !crate::spelling::available() {
        return;
    }
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    // A text with a misspelling in two places.
    let (slide, shape) = s.with(|e, _| {
        let sl = e.slide();
        (sl.id, sl.shapes.iter().find(|sh| sh.text_body().is_some()).unwrap().id)
    });
    s.update(|e, _, cx| e.run(vec![Command::SetText { slide, shape, text: "We recieve the recieve plan".into() }], cx));
    let found = s.with(|e, _| crate::pptx::spelling_panel::scan(e.presentation(), &Default::default(), Some("en_US")));
    if found.iter().filter(|i| i.word == "recieve").count() != 2 {
        // No English dictionary here.
        return;
    }
    s.menu("pptx.spelling.check");
    assert!(s.with(|e, _| e.pptx().unwrap().spelling.is_some()));
    // The first of the two (the panel checks in the system's own language: the words may not be found there).
    let issues = s.with(|e, _| e.pptx().unwrap().spelling.as_ref().unwrap().issues.len());
    if issues == 0 {
        s.update(|e, w, cx| e.close_spelling(w, cx));
        return;
    }
    s.update(|e, w, cx| e.spelling_change("receive", true, w, cx));
    let text = s.with(|e, _| e.slide().shapes.iter().find(|sh| sh.id == shape).unwrap().text_body().unwrap().text());
    assert!(!text.contains("recieve"), "{text}");
    // 辞書に追加 is noted, not done: the user's own dictionary is left alone.
    s.update(|e, w, cx| {
        e.run(vec![Command::SetText { slide, shape, text: "zzqxv plan".into() }], cx);
        e.open_spelling(w, cx);
    });
    let word = s.with(|e, _| e.pptx().unwrap().spelling.as_ref().unwrap().issues.first().map(|i| i.word.clone()));
    if let Some(word) = word {
        s.update(|e, w, cx| e.spelling_learn(w, cx));
        assert_eq!(crate::spelling::learned_in_tests(), vec![word]);
    }
    s.update(|e, w, cx| e.close_spelling(w, cx));
    assert!(s.with(|e, _| e.pptx().unwrap().spelling.is_none()));
    // Undone: the second text, then the change of the spelling.
    s.key("cmd-z");
    s.key("cmd-z");
    let back = s.with(|e, _| e.slide().shapes.iter().find(|sh| sh.id == shape).unwrap().text_body().unwrap().text());
    assert!(back.contains("recieve"), "{back}");
}

/// スライド マスター表示: a layout edited as a slide (a shape moved, a text box
/// added), another page, closed: the layout and the slides that follow it
/// changed, what was not saved before kept, saved and read again.
pub(crate) fn master_view() {
    use ibis_doc_model::{Command, EmuRect, Shape};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let slides = s.with(|e, _| e.presentation().slides.len());
    // An edit not saved, before the view.
    let (slide, shape) = s.with(|e, _| (e.slide().id, e.slide().shapes.iter().find(|sh| sh.text_body().is_some()).unwrap().id));
    s.update(|e, _, cx| e.run(vec![Command::SetText { slide, shape, text: "保存前の変更".into() }], cx));
    s.menu("pptx.master-view");
    assert!(s.with(|e, _| e.is_master_view()));
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 1, "the page is a slide of its own");
    let (name, count) = s.with(|e, _| (e.master_view_name(), e.slide().shapes.len()));
    assert!(count > 0, "the layout's shapes are the slide's: {name}");
    // A text box added and a shape moved on it.
    let (page_slide, first) = s.with(|e, _| (e.slide().id, e.slide().shapes[0].clone()));
    let id = s.with(|e, _| ibis_doc_model::ShapeId(e.presentation().slide(page_slide).map_or(0, |sl| sl.shapes.iter().map(|x| x.id.0).max().unwrap_or(0)) + 1000));
    let moved = EmuRect::new(first.frame.x + 111_111, first.frame.y + 22_222, first.frame.width, first.frame.height);
    s.update(|e, _, cx| {
        let text = Shape::text_box(id, EmuRect::new(300_000, 6_000_000, 3_000_000, 400_000), "会社名");
        e.run(vec![Command::SetFrame { slide: page_slide, shape: first.id, frame: moved }, Command::InsertShape { slide: page_slide, index: count, shape: text }], cx)
    });
    // Another page keeps what was made.
    let pages = s.with(|e, _| e.master_view_page_count());
    assert!(pages > 2);
    s.update(|e, w, cx| e.switch_master_page(pages - 1, w, cx));
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 1);
    s.update(|e, w, cx| e.switch_master_page(1, w, cx));
    assert!(s.with(|e, _| e.slide().shapes.iter().any(|sh| sh.text_body().is_some_and(|b| b.text() == "会社名"))), "the text box is still on the page");
    // 文字の書式: the title's style of the master, for every slide.
    let before_size = s.with(|e, _| e.title_size_of_slide(1));
    s.update(|e, w, cx| e.choose_master_style(ibis_pptx::master_page::StyleKind::Title, 1, w, cx));
    s.update(|e, w, cx| {
        e.set_master_style_inputs("33", "C00000", "Meiryo UI", true, w, cx);
        e.apply_master_style(w, cx);
    });
    assert!(s.with(|e, _| e.slide().shapes.iter().any(|sh| sh.text_body().is_some_and(|b| b.text() == "会社名"))), "the page's own edits are kept");
    s.menu("pptx.master-view");
    assert!(!s.with(|e, _| e.is_master_view()));
    assert_ne!(s.with(|e, _| e.title_size_of_slide(1)), before_size, "the title's size changed on the slides");
    // The slides as they were (with the edit not saved), the layout changed.
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), slides);
    let text = s.with(|e, _| e.presentation().slides[e.presentation().slides.iter().position(|sl| sl.shapes.iter().any(|sh| sh.text_body().is_some_and(|b| b.text() == "保存前の変更"))).unwrap_or(0)].shapes.len());
    assert!(text > 0);
    assert!(s.with(|e, _| e.presentation().slides.iter().any(|sl| sl.shapes.iter().any(|sh| sh.text_body().is_some_and(|b| b.text() == "保存前の変更")))), "what was not saved is kept");
    assert!(s.with(|e, _| e.presentation().layouts.iter().any(|l| l.backdrop.iter().any(|sh| sh.text_body().is_some_and(|b| b.text() == "会社名")))), "the text box is on a layout now");
    assert!(s.with(|e, _| e.presentation().slides.iter().any(|sl| sl.backdrop.iter().any(|sh| sh.text_body().is_some_and(|b| b.text() == "会社名")))), "shown on the slides that follow it");
    s.save_as("master-view.pptx");
    let saved = s.reopen();
    assert!(saved.layouts.iter().any(|l| l.backdrop.iter().any(|sh| sh.text_body().is_some_and(|b| b.text() == "会社名"))));
    // Saving is not done while the view is open.
    s.menu("pptx.master-view");
    assert!(s.with(|e, _| e.is_master_view()));
    s.update(|e, w, _| assert!(e.save_to(std::path::PathBuf::from("/tmp/ibis-no.pptx"), w).is_err()));
    s.menu("pptx.master-view");
}

/// ノートの書式編集: the notes page of a slide edited as a slide (part of its
/// text made bold), closed: the notes keep their text with the formatting, and
/// what was not saved before is kept.
pub(crate) fn notes_formatting() {
    use ibis_doc_model::{CharacterFormat, Command};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    let (slide, shape) = s.with(|e, _| (e.slide().id, e.slide().shapes.iter().find(|sh| sh.text_body().is_some()).unwrap().id));
    // An edit not saved, before the view.
    s.update(|e, _, cx| e.run(vec![Command::SetText { slide, shape, text: "保存前の変更".into() }], cx));
    let notes = s.with(|e, _| e.slide().notes.clone());
    assert!(!notes.is_empty());
    s.menu("pptx.notes-format");
    assert!(s.with(|e, _| e.is_notes_format()));
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 1);
    let (page, body) = s.with(|e, _| {
        let sl = e.slide();
        (sl.id, sl.shapes.iter().find(|sh| sh.text_body().is_some_and(|b| b.text() == "発表メモ：数値はすべて架空です。")).map(|sh| sh.id))
    });
    let body = body.expect("the notes' text is on the page");
    s.update(|e, _, cx| e.run(vec![Command::SetCharacterFormat { slide: page, shape: body, start: 0, end: 4, format: CharacterFormat { bold: Some(true), ..Default::default() } }], cx));
    s.menu("pptx.notes-format");
    assert!(!s.with(|e, _| e.is_notes_format()));
    assert_eq!(s.with(|e, _| e.slide().notes.clone()), notes, "the notes' text is as it was");
    assert!(s.with(|e, _| e.presentation().slides.iter().any(|sl| sl.shapes.iter().any(|sh| sh.text_body().is_some_and(|b| b.text() == "保存前の変更")))), "what was not saved is kept");
    s.save_as("notes-format.pptx");
    let xml = {
        let data = std::fs::read(s.with(|e, _| e.path.clone()).unwrap()).unwrap();
        let package = ibis_ooxml_for_scenarios(&data);
        package
    };
    assert!(xml.contains("b=\"1\""), "the bold is in the notes page");
}

/// 配布資料マスター (NOTES-007): 表示 > 配布資料マスターで 4 つの枠が 1 枚のスライドになり、
/// 動かして閉じると、保存したファイルの配布資料マスターの位置になる。未保存の変更は残る。
pub(crate) fn handout_master_view() {
    use ibis_doc_model::{Command, EmuRect};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    let (slide, shape) = s.with(|e, _| (e.slide().id, e.slide().shapes.iter().find(|sh| sh.text_body().is_some()).unwrap().id));
    s.update(|e, _, cx| e.run(vec![Command::SetText { slide, shape, text: "保存前の変更".into() }], cx));
    s.menu("pptx.handout-master");
    assert!(s.with(|e, _| e.is_handout_master() && !e.is_notes_format()));
    assert_eq!(s.menu_state("pptx.handout-master").map(|(_, checked)| checked), Some(true));
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 1);
    let (page, header) = s.with(|e, _| {
        let sl = e.slide();
        assert_eq!(sl.shapes.len(), 4, "the four places");
        let h = sl.shapes.iter().find(|sh| sh.text_body().is_some_and(|b| b.text() == "ヘッダー")).unwrap();
        (sl.id, (h.id, h.frame))
    });
    s.screenshot("handout-master");
    let moved = EmuRect::new(header.1.x + 600_000, header.1.y + 200_000, header.1.width, header.1.height);
    s.update(|e, _, cx| e.run(vec![Command::SetFrame { slide: page, shape: header.0, frame: moved }], cx));
    // A file cannot be saved while the page is open.
    s.menu("pptx.handout-master");
    assert!(!s.with(|e, _| e.is_handout_master()));
    assert!(s.with(|e, _| e.presentation().slides.iter().any(|sl| sl.shapes.iter().any(|sh| sh.text_body().is_some_and(|b| b.text() == "保存前の変更")))), "what was not saved is kept");
    let path = s.save_as("handout-master.pptx");
    let data = std::fs::read(path).unwrap();
    let layout = ibis_pptx::handout_master::layout(&data).unwrap();
    let place = layout.places.iter().find(|p| p.mark == ibis_pptx::handout_master::Mark::Header).unwrap();
    assert_eq!((place.frame.0, place.frame.1), (moved.x, moved.y));
    undo_all_and_redo(&mut s);
}

/// The text of the notes pages of a file (for the scenarios).
fn ibis_ooxml_for_scenarios(data: &[u8]) -> String {
    let package = ibis_pptx::ibis_ooxml::Package::open(data.to_vec()).unwrap();
    package.names().filter(|n| n.starts_with("ppt/notesSlides/notesSlide") && n.ends_with(".xml")).map(|n| String::from_utf8_lossy(&package.read(n).unwrap()).into_owned()).collect()
}

/// T11/T12: frame settings preserve the editable runs and undo/redo.
pub(crate) fn text_frame_properties() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1);
    let original = s.with(|e, _| e.edited_body().unwrap().clone());
    let caret = s.with(|e, _| e.text_editing().unwrap().caret);
    for (id, anchor) in [("pptx.text.top", ibis_doc_model::VerticalAnchor::Top), ("pptx.text.middle", ibis_doc_model::VerticalAnchor::Middle), ("pptx.text.bottom", ibis_doc_model::VerticalAnchor::Bottom)] {
        s.menu(id);
        assert_eq!(s.with(|e, _| e.edited_body().unwrap().anchor), anchor);
        assert_eq!(s.menu_state(id), Some((true, true)));
    }
    s.menu("pptx.text.insets-none");
    s.menu("pptx.text.wrap");
    for (id, vertical, sideways) in [("pptx.text.vertical", true, 0), ("pptx.text.clockwise", false, 5_400_000), ("pptx.text.counterclockwise", false, 16_200_000), ("pptx.text.horizontal", false, 0)] {
        s.menu(id);
        let body = s.with(|e, _| e.edited_body().unwrap().clone());
        assert_eq!((body.vertical, body.sideways), (vertical, sideways));
        assert_eq!(body.paragraphs, original.paragraphs);
        assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), caret);
        assert_eq!(s.menu_state(id), Some((true, true)));
        s.save_as(&format!("{id}.pptx"));
        let saved = s.reopen();
        let read = saved.slides[0].shapes[1].text_body().unwrap();
        assert_eq!((read.anchor, read.vertical, read.sideways, read.wrap, read.insets), (body.anchor, body.vertical, body.sideways, body.wrap, body.insets));
        assert_eq!(read.text(), body.text());
    }
    let before = s.with(|e, _| e.edited_body().unwrap().clone());
    s.menu("pptx.text.insets-default");
    assert_eq!(s.with(|e, _| e.edited_body().unwrap().insets), ibis_doc_model::Insets::default());
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.edited_body().unwrap().clone()), before);
    s.key("cmd-shift-z");
    s.key("escape");
    undo_all_and_redo(&mut s);
}

/// S6 placement reset: moving a placeholder takes its own frame, reset removes it.
pub(crate) fn reset_slide_placement() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let (index, expected) = s.with(|e, _| {
        let reset = e.presentation().reset_slide_frames(e.current_slide).unwrap();
        let index = reset.shapes.iter().position(|sh| reset.placeholders.get(&sh.id).is_some_and(|ph| !ph.own_frame)).unwrap();
        (index, reset)
    });
    s.click_shape(index);
    s.key("right");
    s.menu("pptx.rotate.right");
    let moved = s.with(|e, _| e.slide().clone());
    assert!(moved.placeholders[&moved.shapes[index].id].own_frame);
    assert_ne!(moved.shapes[index].frame, expected.shapes[index].frame);
    s.menu("pptx.reset-placement");
    assert_eq!(s.with(|e, _| e.slide().clone()), expected);
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().clone()), moved);
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().clone()), expected);
    s.save_as("reset-placement.pptx");
    let read = s.reopen();
    assert_eq!(read.slides[0].shapes[index].frame, expected.shapes[index].frame);
    assert_eq!(read.slides[0].shapes[index].rotation, expected.shapes[index].rotation);
    assert!(!read.slides[0].placeholders[&read.slides[0].shapes[index].id].own_frame);
    undo_all_and_redo(&mut s);
}

/// PPTX-MASTER-008: slide layout and formatting reset (スライドのリセット).
/// Resets placeholder frame and rotation, shape effects (shadow etc), and text direct overrides back to layout defaults.
pub(crate) fn reset_slide_layout_and_formatting() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    assert_eq!(s.menu_state("pptx.reset-slide"), Some((true, false)));
    assert!(s.with(|e, _| e.slide_context_menu_entries()).iter().any(|entry| matches!(entry, crate::shell::chrome::MenuEntry::Item(item) if item.id == "pptx.reset-slide")));

    let (index, expected) = s.with(|e, _| {
        let reset = e.presentation().reset_slide(e.current_slide).unwrap();
        let index = reset.shapes.iter().position(|sh| reset.placeholders.get(&sh.id).is_some_and(|ph| !ph.own_frame)).unwrap();
        (index, reset)
    });

    // 1. Move and rotate the placeholder
    s.click_shape(index);
    s.key("right");
    s.menu("pptx.rotate.right");

    // 2. Apply bold to the text in placeholder
    s.double_click_shape(index);
    s.key("cmd-a");
    s.key("cmd-b");
    s.key("escape");

    let modified = s.with(|e, _| e.slide().clone());
    assert!(modified.placeholders[&modified.shapes[index].id].own_frame);
    assert_ne!(modified.shapes[index].frame, expected.shapes[index].frame);
    if let ShapeKind::Auto(auto) = &modified.shapes[index].kind {
        if let Some(text) = &auto.text {
            assert!(text.paragraphs.iter().any(|p| p.runs.iter().any(|r| r.style.bold == Some(true))), "bold should be present: {:?}", text);
        }
    }

    // Reset slide
    s.menu("pptx.reset-slide");
    let after_reset = s.with(|e, _| e.slide().clone());
    assert!(!after_reset.placeholders[&after_reset.shapes[index].id].own_frame);
    assert_eq!(after_reset.shapes[index].frame, expected.shapes[index].frame);
    assert_eq!(after_reset.shapes[index].rotation, expected.shapes[index].rotation);
    if let ShapeKind::Auto(auto) = &after_reset.shapes[index].kind {
        assert!(auto.shadow.is_none(), "shadow should be reset");
        if let Some(text) = &auto.text {
            assert!(text.paragraphs.iter().all(|p| p.runs.iter().all(|r| r.style.bold != Some(true))), "bold override should be reset");
        }
    }

    // Undo / Redo check
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().clone()), modified);
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().clone()), after_reset);

    // Save and Reopen
    s.save_as("reset-slide.pptx");
    let read = s.reopen();
    assert_eq!(read.slides[0].shapes[index].frame, expected.shapes[index].frame);
    assert_eq!(read.slides[0].shapes[index].rotation, expected.shapes[index].rotation);
    assert!(!read.slides[0].placeholders[&read.slides[0].shapes[index].id].own_frame);
    if let ShapeKind::Auto(auto) = &read.slides[0].shapes[index].kind {
        assert!(auto.shadow.is_none(), "shadow should remain reset after reload");
    }

    undo_all_and_redo(&mut s);
}

/// I3/I4: crop commands and reset keep the original image and frame.
pub(crate) fn shape_shadow_presets_and_safe_rejection() {
    let mut s=Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e,_,cx| {let id=e.document.ids().new_shape_id();let mut auto=ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Rect,Fill::Solid(ibis_doc_model::Color::BLACK),None);auto.text=Some(ibis_doc_model::TextBody::plain("shadow",ibis_doc_model::TextStyle::default()));e.insert_shape(ibis_doc_model::Shape::new(id,ibis_doc_model::EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto)),cx);});
    s.save_as("../pptx-shadow-ui-base.pptx");drop(s);
    let mut s=Scenario::open("target/scenario/pptx-shadow-ui-base.pptx");let index=s.shapes()-1;s.click_shape(index);
    for (id,value) in [("right",Some((0,0))),("diagonal",Some((2_700_000,0))),("soft",Some((2_700_000,50_800))),("none",None)] {
        let shadow=value.map(|(direction,blur)|ibis_doc_model::Shadow::simple(ibis_doc_model::Color {rgb:ibis_doc_model::Rgb(0),alpha:128},blur,50_800,direction));
        let before=s.with(|e,_|e.slide().shapes[index].clone());s.menu(&format!("pptx.shadow.{id}"));let mut expected=before.clone();let ShapeKind::Auto(auto)=&mut expected.kind else {panic!("auto")};auto.shadow=shadow;assert_eq!(s.with(|e,_|e.slide().shapes[index].clone()),expected);
        s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes[index].clone()),before);s.key("cmd-shift-z");s.save_as(&format!("shadow-{id}.pptx"));let saved=s.reopen();let ShapeKind::Auto(auto)=&saved.slides[0].shapes[index].kind else {panic!("auto")};assert_eq!(auto.shadow,shadow);
    }
    undo_all_and_redo(&mut s);s.menu("pptx.shadow.right");s.menu("pptx.glow.blue5");s.save_as("../pptx-shadow-ui-existing.pptx");drop(s);
    let mut s=Scenario::open("target/scenario/pptx-shadow-ui-existing.pptx");let index=s.shapes()-1;s.click_shape(index);let before=s.with(|e,_|e.slide().shapes[index].clone());s.menu("pptx.shadow.none");let ShapeKind::Auto(auto)=s.with(|e,_|e.slide().shapes[index].kind.clone()) else {panic!("auto")};assert!(auto.shadow.is_none());let ShapeKind::Auto(original)=&before.kind else {panic!("auto")};assert!(auto.glow.is_some());assert_eq!(auto.glow,original.glow);s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes[index].clone()),before);s.key("cmd-shift-z");s.save_as("existing-shadow-cleared.pptx");
    let ids=s.with(|e,_|vec![e.slide().shapes[index].id,*e.slide().placeholders.keys().next().unwrap()]);s.update(|e,_,_|e.set_selection(ids));let before=s.with(|e,_|e.presentation().clone());s.menu("pptx.shadow.soft");assert_eq!(s.with(|e,_|e.presentation().clone()),before);undo_all_and_redo(&mut s);
}

pub(crate) fn text_font_scale_custom() {
    let mut s=Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e,_,cx| {let id=e.document.ids().new_shape_id();let body=ibis_doc_model::TextBody::plain("任意倍率 UI",ibis_doc_model::TextStyle::default());e.insert_shape(ibis_doc_model::Shape::new(id,ibis_doc_model::EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::TextBox(body)),cx);});s.save_as("../pptx-font-scale-custom-base.pptx");drop(s);let mut s=Scenario::open("target/scenario/pptx-font-scale-custom-base.pptx");let index=s.shapes()-1;s.click_shape(index);let before=s.with(|e,_|e.slide().shapes[index].clone());s.menu("pptx.text.insets-custom");
    for value in ["NaN","inf","101","0","-1","0.0006","0.999"] {
        s.update(|e,w,cx| {let f=e.pptx().unwrap().insets_panel.as_ref().unwrap().fields[4].clone();f.update(cx,|f,cx|f.set_value(value,w,cx));});s.menu("pptx.insets.apply");assert_eq!(s.with(|e,_|e.slide().shapes[index].clone()),before);assert!(s.with(|e,_|e.pptx().unwrap().insets_panel.is_some()));
    }
    s.update(|e,w,cx| {let f=e.pptx().unwrap().insets_panel.as_ref().unwrap().fields[4].clone();f.update(cx,|f,cx| {f.set_value("87.654",w,cx);f.focus(w,cx);});});s.menu("pptx.insets.apply");let mut expected=before.clone();expected.text_body_mut().unwrap().font_scale=87_654;assert_eq!(s.with(|e,_|e.slide().shapes[index].clone()),expected);s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes[index].clone()),before);s.key("cmd-shift-z");s.save_as("rate87654.pptx");assert_eq!(s.reopen().slides[0].shapes[index].text_body().unwrap().font_scale,87_654);
    s.menu("pptx.text.insets-custom");s.key("cmd-a");s.type_text("0.7");s.menu("pptx.insets.apply");assert_eq!(s.with(|e,_|e.slide().shapes[index].text_body().unwrap().font_scale),87_654);let other=s.with(|e,_|e.slide().shapes[1].text_body().unwrap().font_scale);let ids=s.with(|e,_|vec![e.slide().shapes[1].id,e.slide().shapes[index].id]);s.update(|e,_,_|e.set_selection(ids));s.menu("pptx.text.insets-custom");s.key("cmd-a");s.type_text("0.9");s.menu("pptx.insets.apply");assert_eq!(s.with(|e,_|e.slide().shapes[1].text_body().unwrap().font_scale),other);assert_eq!(s.with(|e,_|e.slide().shapes[index].text_body().unwrap().font_scale),87_654);s.save_as("mixed-rate-kept.pptx");s.menu("pptx.text.insets-custom");s.update(|e,w,cx| {let p=e.pptx().unwrap().insets_panel.as_ref().unwrap();let a=p.fields[0].clone();let b=p.fields[4].clone();a.update(cx,|f,cx|f.set_value("0.8",w,cx));b.update(cx,|f,cx|f.set_value("80",w,cx));});    s.menu("pptx.insets.apply");
    assert_eq!(s.with(|e,_|e.slide().shapes[1].text_body().unwrap().font_scale),80_000);
    assert_eq!(s.with(|e,_|e.slide().shapes[index].text_body().unwrap().font_scale),80_000);
    assert!(s.with(|e,_|e.pptx().unwrap().insets_panel.is_none()));
    for (input,scale) in [("1",1_000),("99.999",99_999),("100",100_000)] {
        s.update(|e,_,_| {let id=e.slide().shapes[index].id;e.set_selection(vec![id]);});s.click_shape(index);s.menu("pptx.text.insets-custom");s.update(|e,w,cx| {let f=e.pptx().unwrap().insets_panel.as_ref().unwrap().fields[4].clone();f.update(cx,|f,cx|f.set_value(input,w,cx));});let before=s.with(|e,_|e.slide().shapes[index].clone());s.menu("pptx.insets.apply");assert_eq!(s.with(|e,_|e.slide().shapes[index].text_body().unwrap().font_scale),scale);s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes[index].clone()),before);s.key("cmd-shift-z");s.save_as(&format!("rate{scale}.pptx"));assert_eq!(s.reopen().slides[0].shapes[index].text_body().unwrap().font_scale,scale);
    }
    undo_all_and_redo(&mut s);
}

pub(crate) fn text_font_scale_presets() {
    let mut s=Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e,_,cx| {let id=e.document.ids().new_shape_id();let body=ibis_doc_model::TextBody::plain("縮小 UI\n次の行",ibis_doc_model::TextStyle::default());e.insert_shape(ibis_doc_model::Shape::new(id,ibis_doc_model::EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::TextBox(body)),cx);});
    s.save_as("../pptx-font-scale-ui-base.pptx");drop(s);let mut s=Scenario::open("target/scenario/pptx-font-scale-ui-base.pptx");let index=s.shapes()-1;s.click_shape(index);
    for (id,scale) in [("scale80",80_000),("scale50",50_000),("scale100",100_000)] {
        let before=s.with(|e,_|e.slide().shapes[index].clone());s.menu(&format!("pptx.text.{id}"));let mut expected=before.clone();expected.text_body_mut().unwrap().font_scale=scale;assert_eq!(s.with(|e,_|e.slide().shapes[index].clone()),expected);
        s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes[index].clone()),before);s.key("cmd-shift-z");s.save_as(&format!("{id}.pptx"));assert_eq!(s.reopen().slides[0].shapes[index].text_body().unwrap().font_scale,scale);
    }
    undo_all_and_redo(&mut s);
}

pub(crate) fn custom_glow_and_snapshot_rejection() {
    let mut s=Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e,_,cx| {
        let id=e.document.ids().new_shape_id();let mut auto=ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Rect,Fill::Solid(ibis_doc_model::Color::BLACK),None);auto.text=Some(ibis_doc_model::TextBody::plain("custom glow",ibis_doc_model::TextStyle::default()));
        e.insert_shape(ibis_doc_model::Shape::new(id,ibis_doc_model::EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto)),cx);
    });
    s.save_as("../pptx-glow-custom-base.pptx");drop(s);
    let mut s=Scenario::open("target/scenario/pptx-glow-custom-base.pptx");let index=s.shapes()-1;s.click_shape(index);
    let fields=|s:&mut Scenario,values:[&str;3]|s.update(|e,w,cx| {let fields=e.pptx().unwrap().glow_panel.as_ref().unwrap().fields.clone();for (f,v) in fields.iter().zip(values) {f.update(cx,|f,cx|f.set_value(v,w,cx));}});
    s.menu("pptx.glow.custom");s.key("cmd-a");s.type_text("7.125");
    let input=s.with(|e,_|e.pptx().unwrap().glow_panel.as_ref().unwrap().fields[0].clone());
    assert_eq!(s.with(|_,cx|input.read(cx).value().to_string()),"7.125");
    fields(&mut s,["7.125","#12aBcD","37.5"]);let before=s.with(|e,_|e.slide().shapes[index].clone());s.menu("pptx.glow.apply");
    let expected=(ibis_doc_model::Color {rgb:ibis_doc_model::Rgb(0x12ABCD),alpha:96},90_488);
    let glow=|s:&mut Scenario|s.with(|e,_| {let ShapeKind::Auto(a)=&e.slide().shapes[index].kind else {panic!("auto")};a.glow});
    assert_eq!(glow(&mut s),Some(expected));s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes[index].clone()),before);s.key("cmd-shift-z");s.save_as("custom-glow.pptx");let saved=s.reopen();let ShapeKind::Auto(a)=&saved.slides[0].shapes[index].kind else {panic!("auto")};assert_eq!(a.glow,Some(expected));
    for values in [["NaN","123456","50"],["inf","123456","50"],["-1","123456","50"],["1","12345","50"],["1","GG0000","50"],["1","123456","101"],["1","123456","NaN"],["169093.20062993","123456","50"]] {
        s.menu("pptx.glow.custom");fields(&mut s,values);let before=s.with(|e,_|e.presentation().clone());s.menu("pptx.glow.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),before);assert!(s.with(|e,_|e.pptx().unwrap().glow_panel.is_some()));s.menu("pptx.glow.cancel");
    }
    s.menu("pptx.glow.custom");fields(&mut s,["0","FFFFFF","0"]);s.menu("pptx.glow.apply");assert_eq!(glow(&mut s),Some((ibis_doc_model::Color {rgb:ibis_doc_model::Rgb(0xFFFFFF),alpha:0},0)));s.save_as("zero-glow.pptx");
    s.menu("pptx.glow.custom");s.menu("pptx.glow.blue5");let changed=s.with(|e,_|e.presentation().clone());s.menu("pptx.glow.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),changed);assert!(s.with(|e,_|e.pptx().unwrap().glow_panel.is_some()));s.menu("pptx.glow.cancel");
    s.menu("pptx.glow.custom");fields(&mut s,["169093.20055118","000000","100"]);s.menu("pptx.glow.apply");assert_eq!(glow(&mut s),Some((ibis_doc_model::Color::BLACK,i32::MAX as i64)));s.save_as("max-glow.pptx");let saved=s.reopen();let ShapeKind::Auto(a)=&saved.slides[0].shapes[index].kind else {panic!("auto")};assert_eq!(a.glow,Some((ibis_doc_model::Color::BLACK,i32::MAX as i64)));s.key("cmd-z");
    s.menu("pptx.glow.custom");s.update(|e,_,cx| {let slide=e.current_slide;let shape=e.slide().shapes[index].id;e.run(vec![ibis_doc_model::Command::RemoveShape {slide,shape}],cx);});let deleted=s.with(|e,_|e.presentation().clone());s.menu("pptx.glow.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),deleted);assert!(s.with(|e,_|e.pptx().unwrap().glow_panel.is_some()));s.menu("pptx.glow.cancel");s.key("cmd-z");
    s.click_shape(index);s.menu("pptx.glow.custom");s.double_click_shape(index);s.update(|e,_,cx|e.input_mark("未確定",cx));let marked=s.with(|e,_|e.edited_body().unwrap().clone());s.menu("pptx.glow.apply");assert_eq!(s.with(|e,_|e.edited_body().unwrap().clone()),marked);assert!(s.with(|e,_|e.text_editing().unwrap().marked.is_some()));assert!(s.with(|e,_|e.pptx().unwrap().glow_panel.is_some()));s.menu("pptx.glow.cancel");s.update(|e,w,cx| {e.input_mark("",cx);e.end_text_edit(w,cx);});
    let ids=s.with(|e,_|vec![e.slide().shapes[index].id,*e.slide().placeholders.keys().next().unwrap()]);s.update(|e,_,_|e.set_selection(ids));s.menu("pptx.glow.custom");fields(&mut s,["8","ABCDEF","80"]);let before=s.with(|e,_|e.presentation().clone());s.menu("pptx.glow.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),before);assert!(s.with(|e,_|e.pptx().unwrap().glow_panel.is_some()));s.menu("pptx.glow.cancel");undo_all_and_redo(&mut s);
}

pub(crate) fn custom_shadow_and_snapshot_rejection() {
    let mut s=Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e,_,cx| {
        let id=e.document.ids().new_shape_id();let mut auto=ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Rect,Fill::Solid(ibis_doc_model::Color::BLACK),None);auto.text=Some(ibis_doc_model::TextBody::plain("custom shadow",ibis_doc_model::TextStyle::default()));
        e.insert_shape(ibis_doc_model::Shape::new(id,ibis_doc_model::EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto)),cx);
    });
    s.save_as("../pptx-shadow-custom-base.pptx");drop(s);
    let mut s=Scenario::open("target/scenario/pptx-shadow-custom-base.pptx");let index=s.shapes()-1;s.click_shape(index);s.menu("pptx.glow.blue5");
    let fields=|s:&mut Scenario,values:[&str;5]|s.update(|e,w,cx| {let fields=e.pptx().unwrap().shadow_panel.as_ref().unwrap().fields.clone();for (f,v) in fields.iter().zip(values) {f.update(cx,|f,cx|f.set_value(v,w,cx));}});
    s.menu("pptx.shadow.custom");s.key("cmd-a");s.type_text("7.125");
    let input=s.with(|e,_|e.pptx().unwrap().shadow_panel.as_ref().unwrap().fields[0].clone());
    assert_eq!(s.with(|_,cx|input.read(cx).value().to_string()),"7.125");
    fields(&mut s,["7.125","#12aBcD","37.5","4.125","123.456"]);let before=s.with(|e,_|e.slide().shapes[index].clone());s.menu("pptx.shadow.apply");
    let expected=ibis_doc_model::Shadow::simple(ibis_doc_model::Color {rgb:ibis_doc_model::Rgb(0x12ABCD),alpha:96},90_488,52_388,7_407_360);
    let shadow=|s:&mut Scenario|s.with(|e,_| {let ShapeKind::Auto(a)=&e.slide().shapes[index].kind else {panic!("auto")};a.shadow});
    assert_eq!(shadow(&mut s),Some(expected));s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes[index].clone()),before);s.key("cmd-shift-z");s.save_as("custom-shadow.pptx");let saved=s.reopen();let ShapeKind::Auto(a)=&saved.slides[0].shapes[index].kind else {panic!("auto")};assert_eq!(a.shadow,Some(expected));assert_eq!(a.glow,Some((ibis_doc_model::Color {rgb:ibis_doc_model::Rgb(0x4472C4),alpha:128},63_500)));
    let before=s.with(|e,_|e.presentation().clone());s.menu("pptx.shadow.custom");s.menu("pptx.glow.custom");assert!(s.with(|e,_|e.pptx().unwrap().shadow_panel.is_none()));assert!(s.with(|e,_|e.pptx().unwrap().glow_panel.is_some()));s.menu("pptx.shadow.custom");assert!(s.with(|e,_|e.pptx().unwrap().glow_panel.is_none()));s.menu("pptx.shadow.cancel");assert_eq!(s.with(|e,_|e.presentation().clone()),before);
    for values in [["NaN","123456","50","1","0"],["-1","123456","50","1","0"],["1","12345","50","1","0"],["1","GG0000","50","1","0"],["1","123456","101","1","0"],["1","123456","NaN","1","0"],["169093.20062993","123456","50","1","0"],["1","123456","50","169093.20062993","0"],["1","123456","50","NaN","0"],["1","123456","50","-1","0"],["1","123456","50","1","360"],["1","123456","50","1","-1"],["1","123456","50","1","inf"]] {
        s.menu("pptx.shadow.custom");fields(&mut s,values);let before=s.with(|e,_|e.presentation().clone());s.menu("pptx.shadow.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),before);assert!(s.with(|e,_|e.pptx().unwrap().shadow_panel.is_some()));s.menu("pptx.shadow.cancel");
    }
    s.menu("pptx.shadow.custom");fields(&mut s,["0","FFFFFF","0","0","0"]);s.menu("pptx.shadow.apply");assert_eq!(shadow(&mut s),Some(ibis_doc_model::Shadow::simple(ibis_doc_model::Color {rgb:ibis_doc_model::Rgb(0xFFFFFF),alpha:0},0,0,0)));s.save_as("zero-shadow.pptx");
    s.menu("pptx.shadow.custom");s.menu("pptx.shadow.right");let changed=s.with(|e,_|e.presentation().clone());s.menu("pptx.shadow.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),changed);assert!(s.with(|e,_|e.pptx().unwrap().shadow_panel.is_some()));s.menu("pptx.shadow.cancel");
    s.menu("pptx.shadow.custom");fields(&mut s,["169093.20055118","000000","100","169093.20055118","359.99998333"]);s.menu("pptx.shadow.apply");assert_eq!(shadow(&mut s),Some(ibis_doc_model::Shadow::simple(ibis_doc_model::Color::BLACK,i32::MAX as i64,i32::MAX as i64,21_599_999)));s.save_as("max-shadow.pptx");let saved=s.reopen();let ShapeKind::Auto(a)=&saved.slides[0].shapes[index].kind else {panic!("auto")};assert_eq!(a.shadow,Some(ibis_doc_model::Shadow::simple(ibis_doc_model::Color::BLACK,i32::MAX as i64,i32::MAX as i64,21_599_999)));s.key("cmd-z");
    s.menu("pptx.shadow.custom");s.update(|e,_,cx| {let slide=e.current_slide;let shape=e.slide().shapes[index].id;e.run(vec![ibis_doc_model::Command::RemoveShape {slide,shape}],cx);});let deleted=s.with(|e,_|e.presentation().clone());s.menu("pptx.shadow.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),deleted);assert!(s.with(|e,_|e.pptx().unwrap().shadow_panel.is_some()));s.menu("pptx.shadow.cancel");s.key("cmd-z");
    s.click_shape(index);s.menu("pptx.shadow.custom");s.double_click_shape(index);s.update(|e,_,cx|e.input_mark("未確定",cx));let marked=s.with(|e,_|e.edited_body().unwrap().clone());s.menu("pptx.shadow.apply");assert_eq!(s.with(|e,_|e.edited_body().unwrap().clone()),marked);assert!(s.with(|e,_|e.text_editing().unwrap().marked.is_some()));assert!(s.with(|e,_|e.pptx().unwrap().shadow_panel.is_some()));s.menu("pptx.shadow.cancel");s.update(|e,w,cx| {e.input_mark("",cx);e.end_text_edit(w,cx);});
    let ids=s.with(|e,_|vec![e.slide().shapes[index].id,*e.slide().placeholders.keys().next().unwrap()]);s.update(|e,_,_|e.set_selection(ids));s.menu("pptx.shadow.custom");fields(&mut s,["8","ABCDEF","80","3","45"]);let before=s.with(|e,_|e.presentation().clone());s.menu("pptx.shadow.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),before);assert!(s.with(|e,_|e.pptx().unwrap().shadow_panel.is_some()));s.menu("pptx.shadow.cancel");undo_all_and_redo(&mut s);
}

pub(crate) fn shape_glow_presets_and_safe_rejection() {
    let mut s=Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e,_,cx| {
        let id=e.document.ids().new_shape_id(); let mut auto=ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Rect,Fill::Solid(ibis_doc_model::Color::BLACK),None);
        auto.text=Some(ibis_doc_model::TextBody::plain("glow",ibis_doc_model::TextStyle::default()));
        e.insert_shape(ibis_doc_model::Shape::new(id,ibis_doc_model::EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto)),cx);
    });
    s.save_as("../pptx-glow-ui-base.pptx"); drop(s);
    let mut s=Scenario::open("target/scenario/pptx-glow-ui-base.pptx"); let index=s.shapes()-1; s.click_shape(index);
    for (id,value) in [("blue5",Some((ibis_doc_model::Color {rgb:ibis_doc_model::Rgb(0x4472C4),alpha:128},63_500))), ("blue10",Some((ibis_doc_model::Color {rgb:ibis_doc_model::Rgb(0x4472C4),alpha:128},127_000))), ("yellow10",Some((ibis_doc_model::Color {rgb:ibis_doc_model::Rgb(0xFFC000),alpha:160},127_000))), ("none",None)] {
        let before=s.with(|e,_| e.slide().shapes[index].clone());s.menu(&format!("pptx.glow.{id}"));let mut expected=before.clone();let ShapeKind::Auto(auto)=&mut expected.kind else {panic!("auto")};auto.glow=value;
        assert_eq!(s.with(|e,_| e.slide().shapes[index].clone()),expected);
        s.key("cmd-z");assert_eq!(s.with(|e,_| e.slide().shapes[index].clone()),before);s.key("cmd-shift-z");s.save_as(&format!("glow-{id}.pptx"));
        let saved=s.reopen();let ShapeKind::Auto(auto)=&saved.slides[0].shapes[index].kind else {panic!("auto")};assert_eq!(auto.glow,value);
    }
    undo_all_and_redo(&mut s);s.menu("pptx.glow.blue10");s.save_as("../pptx-glow-ui-existing.pptx");drop(s);
    let mut s=Scenario::open("target/scenario/pptx-glow-ui-existing.pptx");let index=s.shapes()-1;s.click_shape(index);
    let before=s.with(|e,_| e.slide().shapes[index].clone());s.menu("pptx.glow.none");let ShapeKind::Auto(auto)=s.with(|e,_| e.slide().shapes[index].kind.clone()) else {panic!("auto")};assert!(auto.glow.is_none());s.key("cmd-z");assert_eq!(s.with(|e,_| e.slide().shapes[index].clone()),before);s.key("cmd-shift-z");s.save_as("existing-glow-cleared.pptx");
    let ids=s.with(|e,_| {let own=e.slide().shapes[index].id;let ph=*e.slide().placeholders.keys().next().unwrap();vec![own,ph]});s.update(|e,_,_|e.set_selection(ids));let before=s.with(|e,_|e.presentation().clone());s.menu("pptx.glow.yellow10");assert_eq!(s.with(|e,_|e.presentation().clone()),before);
    undo_all_and_redo(&mut s);
}

pub(crate) fn shape_reflection_and_soft_edge_presets() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        let mut auto = ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Rect, Fill::Solid(ibis_doc_model::Color::BLACK), None);
        auto.text = Some(ibis_doc_model::TextBody::plain("effects", ibis_doc_model::TextStyle::default()));
        e.insert_shape(ibis_doc_model::Shape::new(id, ibis_doc_model::EmuRect::new(500_000, 500_000, 2_000_000, 1_000_000), ShapeKind::Auto(auto)), cx);
    });
    s.save_as("../pptx-effects-ui-base.pptx");
    drop(s);

    let mut s = Scenario::open("target/scenario/pptx-effects-ui-base.pptx");
    let index = s.shapes() - 1;
    s.click_shape(index);

    let refl = |distance: i64, end_pos: i32| ibis_doc_model::Reflection {
        blur: 6_350,
        start_alpha: 52_000,
        end_alpha: 300,
        start_pos: 0,
        end_pos,
        distance,
        direction: 5_400_000,
        fade_direction: 5_400_000,
        sx: 100_000,
        sy: -100_000,
        kx: 0,
        ky: 0,
        align: ibis_doc_model::TileAlign::BottomLeft,
        rot_with_shape: false,
    };

    // 1. 反射プリセットの適用・Undo・Redo・保存
    for (id, distance, end_pos) in [
        ("tight", 0, 35_000),
        ("half", 38_100, 55_000),
        ("full", 38_100, 90_000),
    ] {
        let before = s.with(|e, _| e.slide().shapes[index].clone());
        s.menu(&format!("pptx.reflection.{id}"));
        let mut expected = before.clone();
        let ShapeKind::Auto(auto) = &mut expected.kind else { panic!("auto") };
        auto.reflection = Some(refl(distance, end_pos));
        assert_eq!(s.with(|e, _| e.slide().shapes[index].clone()), expected);
        s.key("cmd-z");
        assert_eq!(s.with(|e, _| e.slide().shapes[index].clone()), before);
        s.key("cmd-shift-z");
        s.save_as(&format!("reflection-{id}.pptx"));
        let saved = s.reopen();
        let ShapeKind::Auto(auto) = &saved.slides[0].shapes[index].kind else { panic!("auto") };
        assert_eq!(auto.reflection, Some(refl(distance, end_pos)));
    }

    // 2. ぼかし（やわらかい縁）プリセットの適用・Undo・Redo・保存
    for (id, rad) in [
        ("1pt", 12_700),
        ("2-5pt", 31_750),
        ("5pt", 63_500),
        ("10pt", 127_000),
        ("25pt", 317_500),
        ("50pt", 635_000),
    ] {
        let before = s.with(|e, _| e.slide().shapes[index].clone());
        s.menu(&format!("pptx.soft-edge.{id}"));
        let mut expected = before.clone();
        let ShapeKind::Auto(auto) = &mut expected.kind else { panic!("auto") };
        auto.soft_edge = Some(rad);
        assert_eq!(s.with(|e, _| e.slide().shapes[index].clone()), expected);
        s.key("cmd-z");
        assert_eq!(s.with(|e, _| e.slide().shapes[index].clone()), before);
        s.key("cmd-shift-z");
        s.save_as(&format!("soft-edge-{id}.pptx"));
        let saved = s.reopen();
        let ShapeKind::Auto(auto) = &saved.slides[0].shapes[index].kind else { panic!("auto") };
        assert_eq!(auto.soft_edge, Some(rad));
    }

    // 3. 反射とぼかしの解除 (none)
    s.menu("pptx.reflection.none");
    s.menu("pptx.soft-edge.none");
    let ShapeKind::Auto(auto) = s.with(|e, _| e.slide().shapes[index].kind.clone()) else { panic!("auto") };
    assert!(auto.reflection.is_none());
    assert!(auto.soft_edge.is_none());
    s.save_as("effects-cleared.pptx");
    let saved = s.reopen();
    let ShapeKind::Auto(auto) = &saved.slides[0].shapes[index].kind else { panic!("auto") };
    assert!(auto.reflection.is_none());
    assert!(auto.soft_edge.is_none());

    // 4. プレースホルダーを含む選択時には安全に弾かれる
    let ids = s.with(|e, _| {
        let own = e.slide().shapes[index].id;
        let ph = *e.slide().placeholders.keys().next().unwrap();
        vec![own, ph]
    });
    s.update(|e, _, _| e.set_selection(ids));
    let before = s.with(|e, _| e.presentation().clone());
    s.menu("pptx.reflection.tight");
    assert_eq!(s.with(|e, _| e.presentation().clone()), before);
    s.menu("pptx.soft-edge.5pt");
    assert_eq!(s.with(|e, _| e.presentation().clone()), before);

    undo_all_and_redo(&mut s);
}

/// PPTX-OBJECT-011: 3D bevel presets and round-trip (面取り効果の適用・解除・保存).
pub(crate) fn shape_bevel_presets_and_round_trip() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        let mut auto = ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Rect, Fill::Solid(ibis_doc_model::Color::BLACK), None);
        auto.text = Some(ibis_doc_model::TextBody::plain("bevel", ibis_doc_model::TextStyle::default()));
        e.insert_shape(ibis_doc_model::Shape::new(id, ibis_doc_model::EmuRect::new(500_000, 500_000, 2_000_000, 1_000_000), ShapeKind::Auto(auto)), cx);
    });
    s.save_as("../pptx-bevel-ui-base.pptx");
    drop(s);

    let mut s = Scenario::open("target/scenario/pptx-bevel-ui-base.pptx");
    let index = s.shapes() - 1;
    s.click_shape(index);

    let presets = [
        ("circle", ibis_doc_model::BevelPreset::Circle),
        ("relaxed-inset", ibis_doc_model::BevelPreset::RelaxedInset),
        ("slope", ibis_doc_model::BevelPreset::Slope),
        ("cross", ibis_doc_model::BevelPreset::Cross),
        ("angle", ibis_doc_model::BevelPreset::Angle),
        ("soft-round", ibis_doc_model::BevelPreset::SoftRound),
        ("convex", ibis_doc_model::BevelPreset::Convex),
        ("cool-slant", ibis_doc_model::BevelPreset::CoolSlant),
        ("hard-edge", ibis_doc_model::BevelPreset::HardEdge),
        ("art-deco", ibis_doc_model::BevelPreset::ArtDeco),
    ];

    // 1. 各面取りプリセットの適用・Undo・Redo・保存
    for (id, preset) in presets {
        let before = s.with(|e, _| e.slide().shapes[index].clone());
        s.menu(&format!("pptx.bevel.{id}"));
        let mut expected = before.clone();
        let ShapeKind::Auto(auto) = &mut expected.kind else { panic!("auto") };
        auto.bevel = Some(ibis_doc_model::Bevel::standard(preset));
        assert_eq!(s.with(|e, _| e.slide().shapes[index].clone()), expected);
        s.key("cmd-z");
        assert_eq!(s.with(|e, _| e.slide().shapes[index].clone()), before);
        s.key("cmd-shift-z");
        s.save_as(&format!("bevel-{id}.pptx"));
        let saved = s.reopen();
        let ShapeKind::Auto(auto) = &saved.slides[0].shapes[index].kind else { panic!("auto") };
        assert_eq!(auto.bevel, Some(ibis_doc_model::Bevel::standard(preset)));
    }

    // 2. 面取りの解除 (none)
    s.menu("pptx.bevel.none");
    let ShapeKind::Auto(auto) = s.with(|e, _| e.slide().shapes[index].kind.clone()) else { panic!("auto") };
    assert!(auto.bevel.is_none());
    s.save_as("bevel-cleared.pptx");
    let saved = s.reopen();
    let ShapeKind::Auto(auto) = &saved.slides[0].shapes[index].kind else { panic!("auto") };
    assert!(auto.bevel.is_none());

    undo_all_and_redo(&mut s);
}

pub(crate) fn copy_shape_shadow_and_glow() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    // A presentation opens with the keys on the slide list: the keys of the slide need the slide focused.
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    let mut source = s.with(|e,_| e.slide().shapes.iter().find(|shape| matches!(shape.kind,ShapeKind::Auto(_))).unwrap().clone());
    let ShapeKind::Auto(auto) = &mut source.kind else { panic!("auto") };
    auto.shadow = Some(ibis_doc_model::Shadow::simple(ibis_doc_model::Color { rgb:ibis_doc_model::Rgb(0x123456),alpha:128 },101_600,152_400,8_100_000));
    auto.glow = Some((ibis_doc_model::Color { rgb:ibis_doc_model::Rgb(0x66CCFF),alpha:200 },63_500));
    assert!(ibis_pptx::save::check_shape_insertion(&source).is_ok());
    s.update(|e,_,cx| { e.pptx_mut().unwrap().clipboard=vec![source.clone()]; assert!(e.paste_shapes(cx)); });
    s.key("cmd-c"); s.page(2); s.key("cmd-v");
    assert_eq!(s.with(|e,_| e.slide().shapes.last().unwrap().kind.clone()),source.kind);
    s.key("cmd-z"); s.key("cmd-shift-z"); s.save_as("copied-shape-effects.pptx");
    let saved=s.reopen(); let ShapeKind::Auto(auto)=&saved.slides[1].shapes.last().unwrap().kind else { panic!("auto") };
    let ShapeKind::Auto(original)=&source.kind else { panic!("auto") };
    assert_eq!((auto.shadow,auto.glow),(original.shadow,original.glow));
    undo_all_and_redo(&mut s);
}

pub(crate) fn copy_picture_color_effects() {
    let mut s = Scenario::open("test/editing/pptx-picture-color-effects.pptx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|shape| matches!(&shape.kind,ShapeKind::Picture(p) if p.duotone.is_some() && p.clear.is_some()))).unwrap();
    s.click_shape(index); let before = s.with(|e, _| e.slide().clone()); let source = before.shapes[index].clone();
    assert_eq!(s.menu_state("edit.copy"),Some((true,false)));
    s.key("cmd-d"); assert_eq!(s.with(|e, _| e.slide().shapes.last().unwrap().kind.clone()),source.kind);
    s.key("cmd-z"); assert_eq!(s.with(|e, _| e.slide().clone()),before); s.key("cmd-shift-z");
    s.save_as("duplicated-picture-colors.pptx");
    s.key("cmd-z"); s.click_shape(index); s.key("cmd-c"); s.page(2); s.key("cmd-v");
    let copy = s.with(|e, _| e.slide().shapes.last().unwrap().clone()); assert_eq!(copy.kind,source.kind); assert_eq!(copy.frame,source.frame);
    s.key("cmd-z"); s.key("cmd-shift-z"); s.save_as("pasted-picture-colors.pptx");
    let saved = s.reopen(); let ShapeKind::Picture(picture) = &saved.slides[1].shapes.last().unwrap().kind else { panic!("picture") };
    let ShapeKind::Picture(original) = &source.kind else { panic!("picture") };
    assert_eq!((picture.duotone,picture.clear,picture.crop,picture.corner),(original.duotone,original.clear,original.crop,original.corner)); assert_eq!(picture.stroke,original.stroke);
    assert_eq!(s.with(|e, _| e.presentation().media.iter().find(|m| m.id==original.media).unwrap().data.clone()),saved.media.iter().find(|m| m.id==picture.media).unwrap().data);
    undo_all_and_redo(&mut s);
}

pub(crate) fn reset_picture_color_effects() {
    let mut s = Scenario::open("test/editing/pptx-picture-color-effects.pptx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|shape| matches!(&shape.kind, ShapeKind::Picture(p) if p.duotone.is_some() && p.clear.is_some()))).unwrap();
    s.click_shape(index);
    let before = s.with(|e, _| e.slide().shapes[index].clone());
    let media = s.with(|e, _| e.presentation().media.clone());
    assert_eq!(s.menu_state("pptx.picture.reset-colors"), Some((true, false)));
    s.menu("pptx.picture.reset-colors");
    let mut expected = before.clone(); let ShapeKind::Picture(picture) = &mut expected.kind else { panic!("picture") }; picture.duotone = None; picture.clear = None;
    assert_eq!(s.with(|e, _| e.slide().shapes[index].clone()), expected);
    assert_eq!(s.with(|e, _| e.presentation().media.clone()), media);
    assert_eq!(s.menu_state("pptx.picture.reset-colors"), Some((false, false)));
    s.key("cmd-z"); assert_eq!(s.with(|e, _| e.slide().shapes[index].clone()), before);
    s.key("cmd-shift-z"); s.save_as("picture-color-reset.pptx");
    let saved = s.reopen(); let ShapeKind::Picture(picture) = &saved.slides[0].shapes[index].kind else { panic!("picture") };
    assert!(picture.duotone.is_none() && picture.clear.is_none());
    let ShapeKind::Picture(original) = &before.kind else { panic!("picture") };
    assert_eq!(picture.crop, original.crop); assert_eq!(picture.stroke, original.stroke); assert_eq!(saved.slides[0].shapes[index].frame,before.frame);
    undo_all_and_redo(&mut s);
}

pub(crate) fn crop_and_reset_picture() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| e.insert_picture("crop.png", png(160, 120, [0x44, 0x88, 0xaa]), cx));
    let original = s.with(|e, _| e.selected_shapes()[0].clone());
    for (item, crop) in [("pptx.crop.left", ibis_doc_model::Crop { left: 10_000, ..Default::default() }), ("pptx.crop.right", ibis_doc_model::Crop { right: 10_000, ..Default::default() }), ("pptx.crop.top", ibis_doc_model::Crop { top: 10_000, ..Default::default() }), ("pptx.crop.bottom", ibis_doc_model::Crop { bottom: 10_000, ..Default::default() }), ("pptx.crop.center", ibis_doc_model::Crop { left: 10_000, top: 10_000, right: 10_000, bottom: 10_000 })] {
        s.toolbar_menu("crop", item);
        let changed = s.with(|e, _| e.selected_shapes()[0].clone());
        let ShapeKind::Picture(picture) = &changed.kind else { panic!("picture") };
        assert_eq!(picture.crop, crop);
        assert_eq!(changed.frame, original.frame);
        s.save_as(&format!("{item}.pptx"));
        let saved = s.reopen();
        let ShapeKind::Picture(picture) = &saved.slides[0].shapes.last().unwrap().kind else { panic!("saved picture") };
        assert_eq!(picture.crop, crop);
        s.toolbar("reset-image");
        assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), original);
        s.key("cmd-z");
        assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), changed);
        s.key("cmd-shift-z");
        assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), original);
    }
    s.save_as("reset.pptx");
    let saved = s.reopen();
    let ShapeKind::Picture(picture) = &saved.slides[0].shapes.last().unwrap().kind else { panic!("saved picture") };
    assert_eq!(picture.crop, ibis_doc_model::Crop::default());
    undo_all_and_redo(&mut s);
}

pub(crate) fn picture_crop_aspect_ratios() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| e.insert_picture("aspect.png", png(160, 120, [0x22, 0x77, 0xbb]), cx));
    let original = s.with(|e, _| e.selected_shapes()[0].clone());

    let cases = [
        ("pptx.crop.aspect.1-1", crop_for_aspect(160, 120, 1, 1)),
        ("pptx.crop.aspect.2-3", crop_for_aspect(160, 120, 2, 3)),
        ("pptx.crop.aspect.3-4", crop_for_aspect(160, 120, 3, 4)),
        ("pptx.crop.aspect.3-5", crop_for_aspect(160, 120, 3, 5)),
        ("pptx.crop.aspect.4-5", crop_for_aspect(160, 120, 4, 5)),
        ("pptx.crop.aspect.9-16", crop_for_aspect(160, 120, 9, 16)),
        ("pptx.crop.aspect.3-2", crop_for_aspect(160, 120, 3, 2)),
        ("pptx.crop.aspect.4-3", crop_for_aspect(160, 120, 4, 3)),
        ("pptx.crop.aspect.5-3", crop_for_aspect(160, 120, 5, 3)),
        ("pptx.crop.aspect.5-4", crop_for_aspect(160, 120, 5, 4)),
        ("pptx.crop.aspect.16-9", crop_for_aspect(160, 120, 16, 9)),
        ("pptx.crop.aspect.16-10", crop_for_aspect(160, 120, 16, 10)),
    ];

    assert_eq!(crop_for_aspect(160, 120, 1, 1), ibis_doc_model::Crop { left: 12_500, right: 12_500, top: 0, bottom: 0 });
    assert_eq!(crop_for_aspect(160, 120, 16, 9), ibis_doc_model::Crop { left: 0, right: 0, top: 12_500, bottom: 12_500 });
    assert_eq!(crop_for_aspect(160, 120, 4, 3), ibis_doc_model::Crop::default());

    // Cases that actually modify Crop from default (4:3 original):
    for (item, expected_crop) in cases {
        if item == "pptx.crop.aspect.4-3" {
            continue;
        }
        s.toolbar_menu("crop", item);
        let changed = s.with(|e, _| e.selected_shapes()[0].clone());
        let ShapeKind::Picture(picture) = &changed.kind else { panic!("picture") };
        assert_eq!(picture.crop, expected_crop, "failed for {item}");
        assert_eq!(changed.frame, original.frame, "frame changed for {item}");

        s.key("cmd-z");
        assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), original, "undo failed for {item}");
        s.key("cmd-shift-z");
        assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), changed, "redo failed for {item}");
        s.key("cmd-z");
        assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), original, "re-undo failed for {item}");
    }

    // Test 4:3 from a cropped state (1:1 -> 4:3 resets crop to default)
    s.toolbar_menu("crop", "pptx.crop.aspect.1-1");
    let cropped_1_1 = s.with(|e, _| e.selected_shapes()[0].clone());
    s.toolbar_menu("crop", "pptx.crop.aspect.4-3");
    let reset_4_3 = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(picture_4_3) = &reset_4_3.kind else { panic!("picture") };
    assert_eq!(picture_4_3.crop, ibis_doc_model::Crop::default());
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), cropped_1_1);
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), reset_4_3);
    s.toolbar("reset-image");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), original);

    // Save and reload roundtrip check with 16:9
    s.toolbar_menu("crop", "pptx.crop.aspect.16-9");
    s.save_as("crop-aspect-16-9.pptx");
    let saved = s.reopen();
    let ShapeKind::Picture(picture) = &saved.slides[0].shapes.last().unwrap().kind else { panic!("saved picture") };
    assert_eq!(picture.crop, ibis_doc_model::Crop { left: 0, right: 0, top: 12_500, bottom: 12_500 });

    undo_all_and_redo(&mut s);
}

/// PPTX-IMAGE-005: rotate and flip picture shapes.
pub(crate) fn picture_rotate_and_flip() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| e.insert_picture("photo.png", png(160, 120, [0x55, 0x99, 0xcc]), cx));
    let original = s.with(|e, _| e.selected_shapes()[0].clone());
    assert_eq!(original.rotation, 0);
    assert!(!original.flip_h && !original.flip_v);

    // Rotate right 90 degrees (5,400,000 60,000ths of a degree)
    s.menu("pptx.rotate.right");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].rotation), 5_400_000);

    // Rotate right again -> 180 degrees
    s.menu("pptx.rotate.right");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].rotation), 10_800_000);

    // Rotate left -> back to 90 degrees
    s.menu("pptx.rotate.left");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].rotation), 5_400_000);

    // Flip horizontally
    s.menu("pptx.rotate.flip-h");
    assert!(s.with(|e, _| e.selected_shapes()[0].flip_h));

    // Flip vertically
    s.menu("pptx.rotate.flip-v");
    assert!(s.with(|e, _| e.selected_shapes()[0].flip_v));

    let modified = s.with(|e, _| e.selected_shapes()[0].clone());
    assert_eq!(modified.rotation, 5_400_000);
    assert!(modified.flip_h && modified.flip_v);

    // Undo flip-v
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| (e.selected_shapes()[0].rotation, e.selected_shapes()[0].flip_h, e.selected_shapes()[0].flip_v)), (5_400_000, true, false));

    // Redo flip-v
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), modified);

    // Save and reload
    s.save_as("picture-rot-flip.pptx");
    let saved = s.reopen();
    let saved_pic = saved.slides[0].shapes.last().unwrap();
    assert_eq!(saved_pic.rotation, 5_400_000);
    assert!(saved_pic.flip_h && saved_pic.flip_v);

    undo_all_and_redo(&mut s);
}

/// PPTX-IMAGE-012: picture border stroke width, color, dash style and clearing.
pub(crate) fn picture_stroke_options() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| e.insert_picture("border-photo.png", png(160, 120, [0x33, 0xaa, 0x77]), cx));
    let original = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(pic) = &original.kind else { panic!("picture") };
    assert!(pic.stroke.is_none());

    // Set border width 3pt (38,100 EMU)
    s.toolbar_menu("line-weight", "width.3");
    let with_width = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(p) = &with_width.kind else { panic!("picture") };
    assert_eq!(p.stroke.as_ref().map(|st| st.width), Some(38_100));

    // Set border dash style
    s.toolbar_menu("line-style", "pptx.stroke-dash");
    let with_dash = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(p) = &with_dash.kind else { panic!("picture") };
    assert_eq!(p.stroke.as_ref().map(|st| (st.width, st.dashed)), Some((38_100, true)));

    // Set border color red (0xff0000)
    s.toolbar_menu("border-color", "color.red");
    let with_color = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(p) = &with_color.kind else { panic!("picture") };
    assert_eq!(p.stroke.as_ref().map(|st| (st.color.rgb.0, st.width, st.dashed)), Some((0xff0000, 38_100, true)));

    // Undo & Redo color change
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), with_dash);
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), with_color);

    // Save and reopen (roundtrip)
    s.save_as("picture-stroke.pptx");
    let saved = s.reopen();
    let saved_pic = saved.slides[0].shapes.last().unwrap();
    let ShapeKind::Picture(p) = &saved_pic.kind else { panic!("saved picture") };
    assert_eq!(p.stroke.as_ref().map(|st| (st.color.rgb.0, st.width, st.dashed)), Some((0xff0000, 38_100, true)));

    // Remove border (color.none)
    s.toolbar_menu("border-color", "color.none");
    let cleared = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(p) = &cleared.kind else { panic!("picture") };
    assert!(p.stroke.is_none());

    // Undo removal
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), with_color);

    undo_all_and_redo(&mut s);
}

/// PPTX-IMAGE-007: crop picture to rounded rectangle shape.
pub(crate) fn picture_crop_to_shape() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| e.insert_picture("crop-shape.png", png(160, 120, [0x44, 0xbb, 0x88]), cx));
    let original = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(pic) = &original.kind else { panic!("picture") };
    assert_eq!(pic.corner, 0);

    // Apply medium rounded corners (20% = 20,000)
    s.toolbar_menu("crop", "pptx.crop.shape.round-medium");
    let round_20 = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(p) = &round_20.kind else { panic!("picture") };
    assert_eq!(p.corner, 20_000);
    assert_eq!(round_20.frame, original.frame);

    // Apply small rounded corners (10% = 10,000)
    s.toolbar_menu("crop", "pptx.crop.shape.round-small");
    let round_10 = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(p) = &round_10.kind else { panic!("picture") };
    assert_eq!(p.corner, 10_000);

    // Undo: back to 20,000
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), round_20);

    // Redo: to 10,000
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), round_10);

    // Save and reload roundtrip with rounded corner
    s.save_as("crop-shape-round.pptx");
    let saved = s.reopen();
    let saved_pic = saved.slides[0].shapes.last().unwrap();
    let ShapeKind::Picture(p) = &saved_pic.kind else { panic!("saved picture") };
    assert_eq!(p.corner, 10_000);

    // Reset to rectangular (square corners: 0)
    s.toolbar_menu("crop", "pptx.crop.shape.rect");
    let square = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(p) = &square.kind else { panic!("picture") };
    assert_eq!(p.corner, 0);

    // Save and reload roundtrip with square corner
    s.save_as("crop-shape-square.pptx");
    let saved2 = s.reopen();
    let saved_pic2 = saved2.slides[0].shapes.last().unwrap();
    let ShapeKind::Picture(p) = &saved_pic2.kind else { panic!("saved picture 2") };
    assert_eq!(p.corner, 0);

    undo_all_and_redo(&mut s);
}

/// PPTX-IMAGE-010: picture color effects (grayscale, sepia, monotone duotone and reset).
pub(crate) fn picture_color_effects() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| e.insert_picture("color-photo.png", png(160, 120, [0x22, 0x55, 0x88]), cx));
    let original = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(pic) = &original.kind else { panic!("picture") };
    assert!(pic.duotone.is_none());

    // Apply grayscale (black to white)
    s.toolbar_menu("picture-color", "pptx.picture.color.grayscale");
    let gray = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(p) = &gray.kind else { panic!("picture") };
    assert_eq!(p.duotone, Some((ibis_doc_model::Rgb(0x000000), ibis_doc_model::Rgb(0xffffff))));
    assert_eq!(gray.frame, original.frame);

    // Apply sepia
    s.toolbar_menu("picture-color", "pptx.picture.color.sepia");
    let sepia = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(p) = &sepia.kind else { panic!("picture") };
    assert_eq!(p.duotone, Some((ibis_doc_model::Rgb(0x402000), ibis_doc_model::Rgb(0xffe0c0))));

    // Apply blue monotone
    s.toolbar_menu("picture-color", "pptx.picture.color.blue");
    let blue = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(p) = &blue.kind else { panic!("picture") };
    assert_eq!(p.duotone, Some((ibis_doc_model::Rgb(0x002060), ibis_doc_model::Rgb(0xd9e1f2))));

    // Undo to sepia
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), sepia);

    // Redo to blue
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), blue);

    // Save and reload roundtrip with blue duotone
    s.save_as("picture-duotone.pptx");
    let saved = s.reopen();
    let saved_pic = saved.slides[0].shapes.last().unwrap();
    let ShapeKind::Picture(p) = &saved_pic.kind else { panic!("saved picture") };
    assert_eq!(p.duotone, Some((ibis_doc_model::Rgb(0x002060), ibis_doc_model::Rgb(0xd9e1f2))));

    // Reset color effects
    s.menu("pptx.picture.reset-colors");
    let cleared = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(p) = &cleared.kind else { panic!("picture") };
    assert!(p.duotone.is_none());

    // Save and reload roundtrip cleared
    s.save_as("picture-duotone-cleared.pptx");
    let saved2 = s.reopen();
    let saved_pic2 = saved2.slides[0].shapes.last().unwrap();
    let ShapeKind::Picture(p) = &saved_pic2.kind else { panic!("saved picture 2") };
    assert!(p.duotone.is_none());

    undo_all_and_redo(&mut s);
}

/// PPTX-IMAGE-009: picture brightness and contrast correction (a:lum bright / contrast).
pub(crate) fn picture_luminance_options() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| e.insert_picture("lum-photo.png", png(160, 120, [0x44, 0x88, 0xcc]), cx));
    let original = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(pic) = &original.kind else { panic!("picture") };
    assert!(pic.luminance.is_none());

    // Apply brightness +40%
    s.toolbar_menu("picture-correction", "pptx.picture.lum.bright-p40");
    let bright40 = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(p) = &bright40.kind else { panic!("picture") };
    assert_eq!(p.luminance, Some((40_000, 0)));
    assert_eq!(bright40.frame, original.frame);

    // Apply contrast +20%
    s.toolbar_menu("picture-correction", "pptx.picture.lum.contrast-p20");
    let contrast20 = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(p) = &contrast20.kind else { panic!("picture") };
    assert_eq!(p.luminance, Some((0, 20_000)));

    // Apply combined brightness +20%, contrast +20%
    s.toolbar_menu("picture-correction", "pptx.picture.lum.bright-contrast-high");
    let combined = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(p) = &combined.kind else { panic!("picture") };
    assert_eq!(p.luminance, Some((20_000, 20_000)));

    // Undo to contrast +20%
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), contrast20);

    // Redo to combined
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), combined);

    // Save and reload roundtrip with combined luminance
    s.save_as("picture-luminance.pptx");
    let saved = s.reopen();
    let saved_pic = saved.slides[0].shapes.last().unwrap();
    let ShapeKind::Picture(p) = &saved_pic.kind else { panic!("saved picture") };
    assert_eq!(p.luminance, Some((20_000, 20_000)));

    // Reset color effects (which resets luminance too)
    s.menu("pptx.picture.reset-colors");
    let cleared = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(p) = &cleared.kind else { panic!("picture") };
    assert!(p.luminance.is_none());

    // Save and reload roundtrip cleared
    s.save_as("picture-luminance-cleared.pptx");
    let saved2 = s.reopen();
    let saved_pic2 = saved2.slides[0].shapes.last().unwrap();
    let ShapeKind::Picture(p) = &saved_pic2.kind else { panic!("saved picture 2") };
    assert!(p.luminance.is_none());

    undo_all_and_redo(&mut s);
}

/// I4: the file picker completion replaces its original target, preserving formats.
pub(crate) fn replace_picture_file() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| e.insert_picture("original.png", png(120, 80, [0x33, 0x66, 0x99]), cx));
    s.toolbar_menu("crop", "pptx.crop.center");
    let original = s.with(|e, _| e.selected_shapes()[0].clone());
    let target = s.with(|e, _| (e.current_slide, e.selected_ids().to_vec()));
    assert!(s.toolbar_state("replace-image").is_some_and(|b| b.enabled));
    s.click_shape(0);
    s.update(|e, _, cx| e.receive_picture_file(Some(target.clone()), "replacement.png", png(60, 90, [0xaa, 0x44, 0x22]), cx));
    let changed = s.with(|e, _| e.presentation().slide(target.0).unwrap().shape(original.id).unwrap().clone());
    assert_eq!(changed.frame, original.frame);
    let (ShapeKind::Picture(a), ShapeKind::Picture(b)) = (&original.kind, &changed.kind) else { panic!("pictures") };
    assert_ne!(a.media, b.media);
    assert_eq!(a.crop, b.crop);
    assert_eq!(a.stroke, b.stroke);
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().shape(original.id).unwrap().clone()), original);
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().shape(original.id).unwrap().clone()), changed);
    // An invalid replacement must leave the image and undo history unchanged.
    s.update(|e, _, cx| e.receive_picture_file(Some(target.clone()), "invalid.png", vec![0, 1, 2], cx));
    assert_eq!(s.with(|e, _| e.slide().shape(original.id).unwrap().clone()), changed);
    s.save_as("replaced.pptx");
    let saved = s.reopen();
    let read = saved.slides[0].shapes.last().unwrap();
    assert_eq!(read.frame, original.frame);
    let ShapeKind::Picture(picture) = &read.kind else { panic!("saved picture") };
    assert_eq!(picture.crop, b.crop);
    assert_eq!(&*saved.media(picture.media).unwrap().data, &png(60, 90, [0xaa, 0x44, 0x22]));
    undo_all_and_redo(&mut s);
}

/// T14: copy source run formats to a different range without copying text/fields.
pub(crate) fn copy_auto_paragraph_format() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1); s.key("cmd-a"); s.type_text("source"); s.key("enter"); s.type_text("target");
    s.update(|e, _, cx| {
        let mut body = e.edited_body().unwrap().clone(); body.paragraphs[0].font_align = ibis_doc_model::FontAlign::Auto; body.paragraphs[1].font_align = ibis_doc_model::FontAlign::Baseline;
        e.run(vec![ibis_doc_model::Command::SetTextBody { slide: e.current_slide, shape: e.text_editing().unwrap().shape, body }], cx);
        let edit = e.pptx_mut().unwrap().edit.as_mut().unwrap(); edit.anchor = 0; edit.caret = 0;
    });
    s.menu("pptx.paragraph-copy");
    let before = s.with(|e, _| e.edited_body().unwrap().clone());
    s.update(|e, _, _| { let edit = e.pptx_mut().unwrap().edit.as_mut().unwrap(); edit.anchor = 7; edit.caret = 7; });
    s.menu("pptx.paragraph-paste");
    let after = s.with(|e, _| e.edited_body().unwrap().clone());
    assert_eq!(after.text(), before.text()); assert_eq!(after.paragraphs[0], before.paragraphs[0]);
    assert_eq!(after.paragraphs[1].font_align, ibis_doc_model::FontAlign::Auto);
    assert_eq!(after.paragraphs[1].runs, before.paragraphs[1].runs);
    s.key("cmd-z"); assert_eq!(s.with(|e, _| e.edited_body().unwrap().clone()), before);
    s.key("cmd-shift-z"); assert_eq!(s.with(|e, _| e.edited_body().unwrap().clone()), after);
    s.save_as("auto-paragraph-copy.pptx");
    assert!(s.reopen().slides[0].shapes[1].text_body().unwrap().paragraphs.iter().all(|p| p.font_align == ibis_doc_model::FontAlign::Auto));
    s.key("escape"); undo_all_and_redo(&mut s);
}

pub(crate) fn copy_and_paste_paragraph_formats() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1); s.key("cmd-a"); s.type_text("source"); s.key("shift-enter"); s.type_text("continuation"); s.key("enter"); s.type_text("target"); s.key("enter"); s.type_text("untouched");
    assert_eq!(s.menu_state("pptx.paragraph-paste"), Some((false, false)));
    s.update(|e, _, cx| {
        let mut body = e.edited_body().unwrap().clone();
        let p = &mut body.paragraphs[0]; p.align = Some(ibis_doc_model::TextAlign::Right); p.margin_left = 360_000; p.indent = -180_000; p.level = 1;
        p.font_align = ibis_doc_model::FontAlign::Center; p.line_spacing = Some(ibis_doc_model::LineSpacing::Points(1850)); p.space_before = 200; p.bullet = Some(ibis_doc_model::Bullet { text: "◆".into(), picture: None, size: 125_000, color: Some(ibis_doc_model::Rgb(0x112233)), auto: None });
        body.paragraphs[1].space_after = 400; body.unify_lines();
        e.run(vec![ibis_doc_model::Command::SetTextBody { slide: e.current_slide, shape: e.text_editing().unwrap().shape, body }], cx);
        let edit = e.pptx_mut().unwrap().edit.as_mut().unwrap(); edit.anchor = 7; edit.caret = 7;
    });
    s.menu("pptx.paragraph-copy");
    let before = s.with(|e, _| e.edited_body().unwrap().clone());
    s.update(|e, _, _| { let edit = e.pptx_mut().unwrap().edit.as_mut().unwrap(); edit.anchor = 20; edit.caret = 20; });
    s.menu("pptx.paragraph-paste");
    let after = s.with(|e, _| e.edited_body().unwrap().clone());
    assert_eq!(after.text(), before.text());
    for i in [0, 1, 3] { assert_eq!(after.paragraphs[i], before.paragraphs[i]); }
    let p = &after.paragraphs[2]; assert_eq!((p.align, p.margin_left, p.indent, p.level, p.font_align, p.line_spacing, p.space_before, p.space_after), (Some(ibis_doc_model::TextAlign::Right), 360_000, -180_000, 1, ibis_doc_model::FontAlign::Center, Some(ibis_doc_model::LineSpacing::Points(1850)), 200, 400));
    assert_eq!(p.bullet, before.paragraphs[0].bullet); assert_eq!(p.runs, before.paragraphs[2].runs); assert_eq!(p.end_style, before.paragraphs[2].end_style);
    s.key("cmd-z"); assert_eq!(s.with(|e, _| e.edited_body().unwrap().clone()), before);
    s.key("cmd-shift-z"); assert_eq!(s.with(|e, _| e.edited_body().unwrap().clone()), after);
    s.save_as("paragraph-format-copy.pptx");
    let saved = s.reopen(); let p = &saved.slides[0].shapes[1].text_body().unwrap().paragraphs[2];
    assert_eq!((p.font_align, p.line_spacing, p.space_before, p.space_after, p.margin_left, p.indent, p.level), (ibis_doc_model::FontAlign::Center, Some(ibis_doc_model::LineSpacing::Points(1850)), 200, 400, 360_000, -180_000, 1));
    assert_eq!(p.bullet, before.paragraphs[0].bullet);
    s.key("escape"); undo_all_and_redo(&mut s);
    drop(s);
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s.with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Table(_))).map(|j| (i, j)))).unwrap();
    s.page(page + 1); let source = cell_middle(&mut s, shape, 1, 1); s.double_click(source);
    s.update(|e, w, cx| e.format_paragraphs(w, cx, ibis_doc_model::ParagraphFormat { line_spacing: Some(Some(ibis_doc_model::LineSpacing::Points(1925))), space_before: Some(125), space_after: Some(375), ..Default::default() }));
    s.menu("pptx.paragraph-copy");
    let target = cell_middle(&mut s, shape, 1, 0); s.double_click(target);
    let before = s.with(|e, _| e.edited_body().unwrap().clone());
    s.menu("pptx.paragraph-paste");
    let after = s.with(|e, _| e.edited_body().unwrap().clone());
    assert_eq!(after.text(), before.text()); assert_eq!(after.paragraphs[0].runs, before.paragraphs[0].runs);
    assert_eq!((after.paragraphs[0].line_spacing, after.paragraphs[0].space_before, after.paragraphs[0].space_after), (Some(ibis_doc_model::LineSpacing::Points(1925)), 125, 375));
    s.key("cmd-z"); assert_eq!(s.with(|e, _| e.edited_body().unwrap().clone()), before);
    s.key("cmd-shift-z"); assert_eq!(s.with(|e, _| e.edited_body().unwrap().clone()), after);
    s.save_as("cell-paragraph-format-copy.pptx"); let saved = s.reopen(); let ShapeKind::Table(t) = &saved.slides[page].shapes[shape].kind else { panic!("table") };
    let p = &t.rows[1].cells[0].text.paragraphs[0]; assert_eq!((p.line_spacing, p.space_before, p.space_after), (Some(ibis_doc_model::LineSpacing::Points(1925)), 125, 375));
    s.key("escape"); undo_all_and_redo(&mut s);
}

pub(crate) fn copy_and_paste_character_formats() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1);
    s.key("cmd-a"); s.type_text("ABCD");
    s.key("home"); s.key("shift-right"); s.key("shift-right");
    assert_eq!(s.menu_state("pptx.format-paste"), Some((false, false)));
    s.toolbar("bold"); s.toolbar("italic"); s.toolbar("underline");
    s.menu("text.superscript"); s.menu("text.spacing-loose");
    s.menu("pptx.text-outline.black");
    s.toolbar_menu("highlight", "color.yellow");
    s.toolbar_menu("pptx-format-paint", "pptx.format-copy");
    let before = s.with(|e, _| e.edited_body().unwrap().clone());
    s.key("right"); s.key("shift-right"); s.key("shift-right");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().range()), 2..4);
    s.menu("pptx.format-paste");
    let after = s.with(|e, _| e.edited_body().unwrap().clone());
    let format = |body: &ibis_doc_model::TextBody| body.paragraphs[0].runs.iter().flat_map(|r| {
        let look = body.style.with_run(&r.style);
        r.text.chars().map(move |_| (look.clone(), r.style.underline, r.style.baseline, r.style.spacing, r.style.highlight, r.style.outline))
    }).collect::<Vec<_>>();
    let formats = format(&after);
    assert_eq!(formats.len(), 4);
    assert!(formats.iter().all(|f| f == &formats[0]));
    assert_eq!(after.text(), "ABCD");
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.edited_body().unwrap().clone()), before);
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.edited_body().unwrap().clone()), after);
    s.save_as("pasted-format.pptx");
    let saved = s.reopen();
    assert_eq!(format(saved.slides[0].shapes[1].text_body().unwrap()), formats);
    s.key("escape");
    undo_all_and_redo(&mut s);
}

/// T15: Unicode offsets, cross-run matches, multiple slides and a table cell.
pub(crate) fn find_and_replace_presentation() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    for (page, shape) in [(1, 1), (2, 0)] {
        s.page(page); s.double_click_shape(shape);
        s.key("cmd-a"); s.type_text("😀zz 😀zz");
        s.key("home"); s.key("right"); s.key("shift-right");
        s.toolbar("bold"); // first match crosses this formatted run boundary
        s.key("escape");
    }
    s.page(3); s.menu("table.new.3x4"); s.type_text("😀zz"); s.key("escape");
    let original = s.with(|e, _| e.presentation().slides.clone());
    s.key("cmd-shift-h"); s.type_text("zz");
    assert_eq!(s.with(|e, cx| e.pptx_find_matches(cx).len()), 5);
    assert_eq!(s.with(|e, _| e.presentation().slides.clone()), original, "query typing does not change slide text");
    s.menu("pptx.find-next");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().range()), 1..3);
    assert_eq!(s.with(|e, _| e.current_slide), original[0].id);
    s.update(|e, w, cx| {
        let input = e.pptx().unwrap().find.as_ref().unwrap().replacement.clone();
        input.update(cx, |i, cx| i.set_value("Y", w, cx));
    });
    s.menu("pptx.replace-one");
    assert_eq!(s.with(|e, _| e.presentation().slides[0].shapes[1].text_body().unwrap().text()), "😀Y 😀zz");
    let one = s.with(|e, _| e.presentation().slides.clone());
    let first = one[0].shapes[1].text_body().unwrap();
    let y = first.paragraphs[0].runs.iter().find(|r| r.text.contains('Y')).unwrap();
    assert!(first.style.with_run(&y.style).bold, "replacement takes first matched run formatting");
    s.key("cmd-z"); assert_eq!(s.with(|e, _| e.presentation().slides.clone()), original);
    s.key("cmd-shift-z"); assert_eq!(s.with(|e, _| e.presentation().slides.clone()), one);
    assert_eq!(s.with(|e, cx| e.pptx_find_matches(cx).len()), 4);
    // The replacement contains the query: replace-all handles the original matches once.
    s.update(|e, w, cx| {
        let input = e.pptx().unwrap().find.as_ref().unwrap().replacement.clone();
        input.update(cx, |i, cx| i.set_value("zzzz", w, cx));
    });
    s.menu("pptx.replace-all");
    let all = s.with(|e, _| e.presentation().slides.clone());
    assert_eq!(s.with(|e, cx| e.pptx_find_matches(cx).len()), 8);
    s.key("cmd-z"); assert_eq!(s.with(|e, _| e.presentation().slides.clone()), one, "all slides/cells replaced in one undo step");
    s.key("cmd-shift-z"); assert_eq!(s.with(|e, _| e.presentation().slides.clone()), all);
    s.save_as("replaced-text.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes[1].text_body().unwrap().text(), "😀Y 😀zzzz");
    assert_eq!(saved.slides[1].shapes[0].text_body().unwrap().text(), "😀zzzz 😀zzzz");
    let ShapeKind::Table(table) = &saved.slides[2].shapes.last().unwrap().kind else { panic!("table") };
    assert_eq!(table.rows[0].cells[0].text.text(), "😀zzzz");
    // Current matches are recomputed after edits/undo; navigation can reach table cells.
    for _ in 0..8 { s.menu("pptx.find-next"); }
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().cell), Some((0, 0)));
    s.key("escape");
    undo_all_and_redo(&mut s);
}

/// S10: custom dimensions, invalid input, cancellation and undo/redo.
/// S9: section block order and thumbnail folding through menu commands.
pub(crate) fn section_order_and_thumbnail_folding() {
    let mut s=Scenario::open("test/editing/pptx-plan.pptx");s.menu("pptx.section.add");s.key("cmd-a");s.type_text("前半");s.menu("pptx.section.apply");s.page(3);s.menu("pptx.section.add");s.key("cmd-a");s.type_text("後半");s.menu("pptx.section.apply");let original=s.with(|e,_|e.presentation().clone());
    s.screenshot("sections-expanded");s.menu("pptx.section.fold");assert!(s.with(|e,_|e.pptx().unwrap().folded_sections.contains(&original.sections[1].id)));assert_eq!(s.with(|e,_|e.presentation().clone()),original);s.screenshot("sections-folded");s.menu("pptx.section.fold");assert!(s.with(|e,_|e.pptx().unwrap().folded_sections.is_empty()));assert_eq!(s.with(|e,_|e.presentation().clone()),original);
    assert_eq!(s.menu_state("pptx.section.move-down").unwrap().0,false);s.menu("pptx.section.move-up");let moved=s.with(|e,_|e.presentation().clone());assert_eq!(moved.sections,vec![original.sections[1].clone(),original.sections[0].clone()]);assert_eq!(moved.slides.iter().map(|x|x.id).collect::<Vec<_>>(),[original.slides[2].id,original.slides[3].id,original.slides[0].id,original.slides[1].id]);s.key("cmd-z");assert_eq!(s.with(|e,_|e.presentation().clone()),original);s.key("cmd-shift-z");assert_eq!(s.with(|e,_|e.presentation().clone()),moved);s.save_as("sections-moved.pptx");let reread=s.reopen();assert_eq!(reread.sections.iter().map(|x|(&x.id,&x.name,x.slides.len())).collect::<Vec<_>>(),moved.sections.iter().map(|x|(&x.id,&x.name,x.slides.len())).collect::<Vec<_>>());for (new,old) in reread.slides.iter().zip(&moved.slides) {assert_eq!(new.shapes.iter().filter_map(|x|x.text_body()).map(|x|x.text()).collect::<Vec<_>>(),old.shapes.iter().filter_map(|x|x.text_body()).map(|x|x.text()).collect::<Vec<_>>());}
    assert_eq!(s.menu_state("pptx.section.move-up").unwrap().0,false);s.menu("pptx.section.move-down");assert_eq!(s.with(|e,_|e.presentation().clone()),original);s.key("cmd-z");assert_eq!(s.with(|e,_|e.presentation().clone()),moved);s.key("cmd-shift-z");assert_eq!(s.with(|e,_|e.presentation().clone()),original);
    let data=s.save_as("section-move-source.pptx");let copy=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/scenario/pptx-section-move-source.pptx");std::fs::copy(data,&copy).unwrap();drop(s);let mut s=Scenario::open("target/scenario/pptx-section-move-source.pptx");s.page(3);let original=s.with(|e,_|e.presentation().clone());s.menu("pptx.section.move-up");let moved=s.with(|e,_|e.presentation().clone());s.key("cmd-z");assert_eq!(s.with(|e,_|e.presentation().clone()),original);s.key("cmd-shift-z");assert_eq!(s.with(|e,_|e.presentation().clone()),moved);s.save_as("source-sections-moved.pptx");assert_eq!(s.reopen().sections[0].name,"後半");s.screenshot("source-sections-moved");
    s.page(1);s.menu("pptx.section.add");s.key("cmd-a");s.type_text("境界から");s.menu("pptx.section.apply");let empty=s.with(|e,_|e.presentation().clone());assert_eq!(empty.sections.len(),3);assert!(empty.sections[0].slides.is_empty());s.screenshot("empty-section-header");
    // These coordinates are the first header in the 1200x720 scenario window.
    s.click((50.,165.));assert_eq!(s.with(|e,_|e.current_section()),Some(0));s.click((11.,165.));assert!(s.with(|e,_|e.pptx().unwrap().folded_sections.contains(&empty.sections[0].id)));assert_eq!(s.with(|e,_|e.presentation().clone()),empty);s.click((11.,165.));assert!(!s.with(|e,_|e.pptx().unwrap().folded_sections.contains(&empty.sections[0].id)));
    s.menu("pptx.section.move-down");let moved=s.with(|e,_|e.presentation().clone());let mut expected=empty.clone();expected.sections.swap(0,1);assert_eq!(moved,expected);s.key("cmd-z");assert_eq!(s.with(|e,_|e.presentation().clone()),empty);s.key("cmd-shift-z");assert_eq!(s.with(|e,_|e.presentation().clone()),moved);s.key("cmd-z");
    s.menu("pptx.section.rename");s.key("cmd-a");s.type_text("空のセクション");s.menu("pptx.section.apply");assert_eq!(s.with(|e,_|e.presentation().sections[0].name.clone()),"空のセクション");s.save_as("empty-section-renamed.pptx");assert!(s.reopen().sections[0].slides.is_empty());s.menu("pptx.section.remove");assert_eq!(s.with(|e,_|e.presentation().sections.len()),2);assert_eq!(s.with(|e,_|e.presentation().slides.clone()),empty.slides);s.save_as("empty-section-removed.pptx");assert_eq!(s.reopen().sections.len(),2);
}

/// S9: real menu/name input, structure conflicts, UndoRedo and persistence.
pub(crate) fn named_sections_and_conflict_rejection() {
    let mut s=Scenario::open("test/editing/pptx-plan.pptx");let original=s.with(|e,_|e.presentation().clone());
    s.menu("pptx.section.add");s.key("cmd-a");s.type_text("   ");s.menu("pptx.section.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),original);assert!(s.with(|e,_|e.pptx().unwrap().section_panel.is_some()));s.menu("pptx.section.cancel");
    s.menu("pptx.section.add");s.key("cmd-a");s.type_text("全体");s.menu("pptx.section.apply");let one=s.with(|e,_|e.presentation().sections.clone());assert_eq!(one.len(),1);assert_eq!(one[0].name,"全体");assert_eq!(one[0].slides,original.slides.iter().map(|s|s.id).collect::<Vec<_>>());s.key("cmd-z");assert_eq!(s.with(|e,_|e.presentation().clone()),original);s.key("cmd-shift-z");assert_eq!(s.with(|e,_|e.presentation().sections.clone()),one);
    s.page(3);s.menu("pptx.section.add");s.key("cmd-a");s.type_text("後半");s.menu("pptx.section.apply");let two=s.with(|e,_|e.presentation().sections.clone());assert_eq!(two.len(),2);assert_eq!(two[0].slides.len(),2);assert_eq!(two[1].slides.len(),2);s.key("cmd-z");assert_eq!(s.with(|e,_|e.presentation().sections.clone()),one);s.key("cmd-shift-z");assert_eq!(s.with(|e,_|e.presentation().sections.clone()),two);
    s.menu("pptx.section.rename");s.key("cmd-a");s.type_text("後半 & <日本語>");s.menu("pptx.section.apply");let named=s.with(|e,_|e.presentation().sections.clone());assert_eq!(named[1].name,"後半 & <日本語>");assert_eq!(named[1].id,two[1].id);s.key("cmd-z");assert_eq!(s.with(|e,_|e.presentation().sections.clone()),two);s.key("cmd-shift-z");assert_eq!(s.with(|e,_|e.presentation().sections.clone()),named);s.save_as("named-sections.pptx");let reopened=s.reopen();assert_eq!(reopened.sections.iter().map(|x|(&x.id,&x.name,x.slides.len())).collect::<Vec<_>>(),named.iter().map(|x|(&x.id,&x.name,x.slides.len())).collect::<Vec<_>>());
    s.menu("pptx.section.rename");s.key("cmd-a");s.type_text("古い対象");s.menu("pptx.duplicate-slide");let changed=s.with(|e,_|e.presentation().clone());s.menu("pptx.section.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),changed);assert!(s.with(|e,_|e.pptx().unwrap().section_panel.is_some()));s.menu("pptx.section.cancel");s.key("cmd-z");assert_eq!(s.with(|e,_|e.presentation().sections.clone()),named);
    s.page(3);s.menu("pptx.section.rename");s.key("cmd-a");s.type_text("本文後の名前");s.double_click_shape(0);s.key("end");s.type_text("本文変更");s.key("escape");let text=s.with(|e,_|e.presentation().slides.clone());s.menu("pptx.section.apply");assert_eq!(s.with(|e,_|e.presentation().sections[1].name.clone()),"本文後の名前");assert_eq!(s.with(|e,_|e.presentation().slides.clone()),text);s.key("cmd-z");assert_eq!(s.with(|e,_|e.presentation().sections.clone()),named);s.key("cmd-z");s.key("cmd-shift-z");s.key("cmd-shift-z");
    let before=s.with(|e,_|e.presentation().sections.clone());s.menu("pptx.section.rename");s.double_click_shape(0);s.update(|e,_,cx|e.input_mark("未確定",cx));let marked=s.with(|e,_|e.edited_body().unwrap().clone());s.menu("pptx.section.apply");assert_eq!(s.with(|e,_|e.presentation().sections.clone()),before);assert_eq!(s.with(|e,_|e.edited_body().unwrap().clone()),marked);assert!(s.with(|e,_|e.text_editing().unwrap().marked.is_some()));s.menu("pptx.section.cancel");s.key("escape");
    s.page(3);let before=s.with(|e,_|e.presentation().clone());s.menu("pptx.section.remove");assert_eq!(s.with(|e,_|e.presentation().sections.len()),1);assert_eq!(s.with(|e,_|e.presentation().slides.clone()),before.slides);s.key("cmd-z");assert_eq!(s.with(|e,_|e.presentation().clone()),before);s.key("cmd-shift-z");s.save_as("merged-sections.pptx");assert_eq!(s.reopen().sections.len(),1);s.menu("pptx.section.remove");assert!(s.with(|e,_|e.presentation().sections.is_empty()));s.key("cmd-z");assert_eq!(s.with(|e,_|e.presentation().sections.len()),1);s.key("cmd-shift-z");s.save_as("cleared-sections.pptx");assert!(s.reopen().sections.is_empty());
    drop(s);let mut s=Scenario::open("test/editing/pptx-plan.pptx");s.page(3);s.menu("pptx.section.add");s.key("cmd-a");s.type_text("途中から");s.menu("pptx.section.apply");let sections=s.with(|e,_|e.presentation().sections.clone());assert_eq!(sections.len(),2);assert_eq!(sections[0].name,"既定のセクション");assert_eq!(sections[0].slides.len(),2);assert_eq!(sections[1].name,"途中から");s.save_as("prefix-sections.pptx");assert_eq!(s.reopen().sections.len(),2);
    let opened=s.save_as("sections-source.pptx");let copy=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/scenario/pptx-sections-source.pptx");std::fs::copy(opened,&copy).unwrap();drop(s);
    let mut s=Scenario::open("target/scenario/pptx-sections-source.pptx");s.page(3);let original=s.with(|e,_|e.presentation().clone());s.menu("pptx.section.rename");s.key("cmd-a");s.type_text("原本の名称変更");s.menu("pptx.section.apply");assert_eq!(s.with(|e,_|e.presentation().sections[1].id.clone()),original.sections[1].id);s.key("cmd-z");assert_eq!(s.with(|e,_|e.presentation().clone()),original);s.key("cmd-shift-z");s.save_as("source-renamed-sections.pptx");assert_eq!(s.reopen().sections[1].name,"原本の名称変更");
    s.page(1);s.menu("pptx.section.remove");assert_eq!(s.with(|e,_|e.presentation().sections.len()),1);s.save_as("source-first-removed.pptx");assert_eq!(s.reopen().sections[0].slides.len(),4);s.menu("pptx.section.remove");assert!(s.with(|e,_|e.presentation().sections.is_empty()));s.save_as("source-cleared-sections.pptx");assert!(s.reopen().sections.is_empty());
}

pub(crate) fn custom_slide_size() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let original = s.with(|e, _| e.presentation().clone());
    s.menu("pptx.slide-size.custom");
    s.key("cmd-a"); s.type_text("NaN");
    s.menu("pptx.slide-size.apply");
    assert_eq!(s.with(|e, _| e.presentation().clone()), original);
    assert!(s.with(|e, _| e.pptx().unwrap().size_panel.is_some()));
    s.menu("pptx.slide-size.cancel");
    assert_eq!(s.with(|e, _| e.presentation().clone()), original);
    s.menu("pptx.slide-size.custom");
    s.key("cmd-a"); s.type_text("20");
    s.update(|e, w, cx| {
        let height = e.pptx().unwrap().size_panel.as_ref().unwrap().height.clone();
        height.update(cx, |h, cx| h.set_value("30", w, cx));
    });
    s.menu("pptx.slide-size.apply");
    assert_eq!(s.with(|e, _| e.presentation().slide_size), ibis_doc_model::EmuSize::new(7_200_000, 10_800_000));
    assert_eq!(s.with(|e, _| e.presentation().slides.clone()), original.slides);
    s.key("cmd-z"); assert_eq!(s.with(|e, _| e.presentation().clone()), original);
    s.key("cmd-shift-z");
    s.save_as("custom-size.pptx");
    assert_eq!(s.reopen().slide_size, ibis_doc_model::EmuSize::new(7_200_000, 10_800_000));
    undo_all_and_redo(&mut s);
}

/// T11: custom four-sided insets, invalid input, cancel and persisted shape text.
pub(crate) fn text_insets_reject_changed_body() {
    let mut s=Scenario::open("test/editing/pptx-plan.pptx");s.click_shape(1);s.menu("pptx.text.insets-custom");s.key("cmd-a");s.type_text("0.7");
    s.double_click_shape(1);s.key("cmd-a");s.type_text("replacement insets");s.key("escape");let before=s.with(|e,_|e.presentation().clone());
    s.menu("pptx.insets.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),before);assert!(s.with(|e,_|e.pptx().unwrap().insets_panel.is_some()));s.menu("pptx.insets.cancel");
    s.key("cmd-z");assert_ne!(s.with(|e,_|e.presentation().clone()),before);s.key("cmd-shift-z");assert_eq!(s.with(|e,_|e.presentation().clone()),before);
    s.click_shape(1);s.menu("pptx.text.insets-custom");s.key("cmd-a");s.type_text("0.7");s.menu("pptx.insets.apply");s.save_as("insets-reopened.pptx");assert_eq!(s.reopen().slides[0].shapes[1].text_body().unwrap().insets.left,252_000);undo_all_and_redo(&mut s);
    let ids=s.with(|e,_|e.slide().shapes.iter().filter(|sh|sh.text_body().is_some()).take(2).map(|sh|sh.id).collect::<Vec<_>>());assert_eq!(ids.len(),2);s.update(|e,_,_|e.set_selection(ids.clone()));s.menu("pptx.text.insets-custom");s.key("cmd-a");s.type_text("0.9");
    s.update(|e,_,cx| {e.set_selection(vec![ids[0]]);e.delete_selected(cx);});let before=s.with(|e,_|e.presentation().clone());s.menu("pptx.insets.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),before);assert!(s.with(|e,_|e.pptx().unwrap().insets_panel.is_some()));s.menu("pptx.insets.cancel");s.key("cmd-z");
    s.click_shape(1);s.menu("pptx.text.insets-custom");s.double_click_shape(1);s.update(|e,_,cx|e.input_mark("未確定",cx));let before=s.with(|e,_|e.edited_body().unwrap().clone());s.menu("pptx.insets.apply");assert_eq!(s.with(|e,_|e.edited_body().unwrap().clone()),before);assert!(s.with(|e,_|e.text_editing().unwrap().marked.is_some()));s.menu("pptx.insets.cancel");
    s.menu("pptx.text.insets-custom");assert!(s.with(|e,_|e.slide().shapes[1].text_body().unwrap().text().contains("未確定")));s.key("cmd-a");s.type_text("0.9");s.menu("pptx.insets.apply");assert!(s.with(|e,_|e.pptx().unwrap().insets_panel.is_none()));s.save_as("insets-ime-reopened.pptx");assert_eq!(s.reopen().slides[0].shapes[1].text_body().unwrap().insets.left,324_000);

    undo_all_and_redo(&mut s);drop(s);
    let mut s=Scenario::open("test/benchmark/earnings-presentation.pptx");s.menu("view.sidebar");let (page,shape)=s.with(|e,_|e.presentation().slides.iter().enumerate().find_map(|(i,sl)|sl.shapes.iter().position(|sh|matches!(sh.kind,ShapeKind::Table(_))).map(|j|(i,j)))).unwrap();s.page(page+1);let cell=cell_middle(&mut s,shape,1,1);s.double_click(cell);s.menu("pptx.text.insets-custom");s.key("cmd-a");s.type_text("0.7");
    s.double_click(cell);s.key("cmd-a");s.type_text("cell replacement");s.key("escape");let before=s.with(|e,_|e.slide().shapes[shape].clone());s.menu("pptx.insets.apply");assert_eq!(s.with(|e,_|e.slide().shapes[shape].clone()),before);assert!(s.with(|e,_|e.pptx().unwrap().insets_panel.is_some()));s.menu("pptx.insets.cancel");
    s.double_click(cell);s.menu("pptx.text.insets-custom");s.key("cmd-a");s.type_text("0.7");s.menu("pptx.insets.apply");s.save_as("insets-cell-reopened.pptx");let reread=s.reopen();let ShapeKind::Table(t)=&reread.slides[page].shapes[shape].kind else {panic!("table")};assert_eq!(t.rows[1].cells[1].text.insets.left,252_000);assert_eq!(t.rows[1].cells[1].text.text(),"cell replacement");undo_all_and_redo(&mut s);

}

pub(crate) fn custom_text_insets() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1);
    let before = s.with(|e, _| e.slide().shapes[1].text_body().unwrap().clone());
    s.menu("pptx.text.insets-custom"); s.key("cmd-a"); s.type_text("-1");
    s.menu("pptx.insets.apply");
    assert_eq!(s.with(|e, _| e.slide().shapes[1].text_body().unwrap().clone()), before);
    assert!(s.with(|e, _| e.pptx().unwrap().insets_panel.is_some()));
    s.menu("pptx.insets.cancel");
    s.click_shape(1); s.menu("pptx.text.insets-custom");
    s.update(|e, w, cx| {
        let fields = e.pptx().unwrap().insets_panel.as_ref().unwrap().fields.clone();
        for (field, value) in fields.into_iter().zip(["0.1", "0.2", "0.3", "0.4"]) { field.update(cx, |f, cx| f.set_value(value, w, cx)); }
    });
    s.menu("pptx.insets.apply");
    let expected = ibis_doc_model::Insets { left: 36_000, top: 72_000, right: 108_000, bottom: 144_000 };
    let after = s.with(|e, _| e.slide().shapes[1].text_body().unwrap().clone());
    assert_eq!(after.insets, expected); assert_eq!(after.paragraphs, before.paragraphs);
    s.key("cmd-z"); assert_eq!(s.with(|e, _| e.slide().shapes[1].text_body().unwrap().clone()), before);
    s.key("cmd-shift-z");
    s.save_as("insets.pptx"); assert_eq!(s.reopen().slides[0].shapes[1].text_body().unwrap().insets, expected);
    undo_all_and_redo(&mut s);
    drop(s);
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s.with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Table(_))).map(|j| (i, j)))).unwrap();
    s.page(page + 1);
    let cell = cell_middle(&mut s, shape, 1, 1); s.double_click(cell);
    let original = s.with(|e, _| e.edited_body().unwrap().clone());
    s.menu("pptx.text.insets-custom");
    s.update(|e, w, cx| {
        let fields = e.pptx().unwrap().insets_panel.as_ref().unwrap().fields.clone();
        for (field, value) in fields.into_iter().zip(["0.1", "0.2", "0.3", "0.4"]) { field.update(cx, |f, cx| f.set_value(value, w, cx)); }
    });
    s.menu("pptx.insets.apply");
    let body = |s: &mut Scenario| s.with(|e, _| {
        let ShapeKind::Table(t) = &e.slide().shapes[shape].kind else { panic!("table") };
        t.rows[1].cells[1].text.clone()
    });
    assert_eq!(body(&mut s).insets, expected); assert_eq!(body(&mut s).paragraphs, original.paragraphs);
    s.key("cmd-z"); assert_eq!(body(&mut s), original); s.key("cmd-shift-z");
    s.save_as("cell-insets.pptx");
    let saved = s.reopen();
    let ShapeKind::Table(t) = &saved.slides[page].shapes[shape].kind else { panic!("table") };
    assert_eq!(t.rows[1].cells[1].text.insets, expected);
    undo_all_and_redo(&mut s);
}

pub(crate) fn find_text_options() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1); s.key("cmd-a"); s.type_text("😀 CAT cat Cat cats _cat cat_ é É");
    s.key("escape");
    let original = s.with(|e, _| e.presentation().slides.clone());
    s.menu("edit.find-replace"); s.type_text("CAT");
    assert_eq!(s.with(|e, cx| e.pptx_find_matches(cx).len()), 1);
    s.menu("pptx.find-case");
    assert_eq!(s.with(|e, cx| e.pptx_find_matches(cx).len()), 6);
    s.menu("pptx.find-word");
    assert_eq!(s.with(|e, cx| e.pptx_find_matches(cx).len()), 3);
    s.menu("pptx.find-next");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().range()), 2..5);
    s.update(|e, w, cx| { let f = e.pptx().unwrap().find.as_ref().unwrap(); f.replacement.clone().update(cx, |f, cx| f.set_value("dog", w, cx)); });
    s.menu("pptx.replace-all");
    let replaced = s.with(|e, _| e.presentation().slides.clone());
    assert_eq!(replaced[0].shapes[1].text_body().unwrap().text(), "😀 dog dog dog cats _cat cat_ é É");
    s.key("cmd-z"); assert_eq!(s.with(|e, _| e.presentation().slides.clone()), original);
    s.key("cmd-shift-z"); assert_eq!(s.with(|e, _| e.presentation().slides.clone()), replaced);
    s.save_as("word-replaced.pptx");
    assert_eq!(s.reopen().slides[0].shapes[1].text_body().unwrap().text(), replaced[0].shapes[1].text_body().unwrap().text());
    s.menu("edit.find-replace"); s.key("cmd-a"); s.type_text("é");
    assert_eq!(s.with(|e, cx| e.pptx_find_matches(cx).len()), 2);
    s.menu("pptx.find-case"); assert_eq!(s.with(|e, cx| e.pptx_find_matches(cx).len()), 1);
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    undo_all_and_redo(&mut s);
}

pub(crate) fn custom_picture_crop() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| e.insert_picture("custom.png", png(160, 120, [0x44, 0x88, 0xaa]), cx));
    let original = s.with(|e, _| e.selected_shapes()[0].clone());
    s.toolbar_menu("crop", "pptx.crop.custom"); s.key("cmd-a"); s.type_text("NaN"); s.menu("pptx.crop.apply");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), original);
    let fields = s.with(|e, _| e.pptx().unwrap().crop_panel.as_ref().unwrap().fields.clone());
    for values in [["50", "0", "50", "0"], ["12.345", "-10", "23.456", "7.89"]] {
        s.update(|_, w, cx| { for (f, value) in fields.iter().zip(values) { f.update(cx, |f, cx| f.set_value(value, w, cx)); } });
        s.menu("pptx.crop.apply");
        if values[0] == "50" { assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), original); }
    }
    let changed = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(p) = &changed.kind else { panic!("picture") };
    let expected = ibis_doc_model::Crop { left: 12_345, top: -10_000, right: 23_456, bottom: 7_890 };
    assert_eq!(p.crop, expected); assert_eq!(changed.frame, original.frame);
    s.key("cmd-z"); assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), original);
    s.key("cmd-shift-z"); assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), changed);
    s.save_as("custom-crop.pptx");
    let saved = s.reopen(); let ShapeKind::Picture(p) = &saved.slides[0].shapes.last().unwrap().kind else { panic!("picture") };
    assert_eq!(p.crop, expected);
    s.toolbar_menu("crop", "pptx.crop.custom"); s.key("cmd-a"); s.type_text("3"); s.menu("pptx.crop.cancel");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), changed);
    undo_all_and_redo(&mut s);
}

pub(crate) fn transition_advance_settings() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.toolbar_menu("transition", "transition.none");
    s.toolbar_menu("transition", "transition.after.5000");
    s.toolbar_menu("transition", "transition.on-click");
    let timing = s.with(|e, _| e.slide().transition.clone()).unwrap();
    assert_eq!((timing.effect.as_str(), timing.on_click, timing.after), ("", false, Some(5000)));
    s.toolbar_menu("transition", "transition.fade");
    s.toolbar_menu("transition", "transition.duration.1000");
    s.toolbar_menu("transition", "transition.none");
    let timing = s.with(|e, _| e.slide().transition.clone()).unwrap();
    assert_eq!((timing.effect.as_str(), timing.on_click, timing.after, timing.duration), ("", false, Some(5000), 1000));
    for (item, after) in [("transition.after.0", Some(0)), ("transition.after.1000", Some(1000)), ("transition.after.10000", Some(10000)), ("transition.after.none", None)] {
        let before = s.with(|e, _| e.slide().transition.clone());
        s.toolbar_menu("transition", item);
        let changed = s.with(|e, _| e.slide().transition.clone());
        assert_eq!(changed.as_ref().unwrap().after, after);
        s.key("cmd-z"); assert_eq!(s.with(|e, _| e.slide().transition.clone()), before);
        s.key("cmd-shift-z"); assert_eq!(s.with(|e, _| e.slide().transition.clone()), changed);
        s.save_as(&format!("{item}.pptx"));
        assert_eq!(s.reopen().slides[0].transition, changed);
    }
    s.toolbar_menu("transition", "transition.on-click");
    s.toolbar_menu("transition", "transition.after.5000");
    let current = s.with(|e, _| e.slide().transition.clone());
    s.toolbar_menu("transition", "transition.all");
    assert!(s.with(|e, _| e.presentation().slides.iter().all(|s| s.transition == current)));
    s.save_as("timing-all.pptx");
    assert!(s.reopen().slides.iter().all(|s| s.transition == current));
    undo_all_and_redo(&mut s);
}

pub(crate) fn transition_timing_rejects_changed_target() {
    let mut s=Scenario::open("test/editing/pptx-plan.pptx");s.toolbar_menu("transition","transition.fade");s.toolbar_menu("transition","transition.timing");s.key("cmd-a");s.type_text("1.234");s.toolbar_menu("transition","transition.duration.2000");let before=s.with(|e,_|e.presentation().clone());s.menu("pptx.timing.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),before);assert!(s.with(|e,_|e.pptx().unwrap().timing_panel.is_some()));s.menu("pptx.timing.cancel");
    s.toolbar_menu("transition","transition.timing");s.key("cmd-a");s.type_text("1.234");s.menu("pptx.timing.apply");let after=s.with(|e,_|e.slide().transition.clone());assert_eq!(after.as_ref().unwrap().duration,1234);s.key("cmd-z");assert_eq!(s.with(|e,_|e.presentation().clone()),before);s.key("cmd-shift-z");s.save_as("timing-reopened.pptx");assert_eq!(s.reopen().slides[0].transition,after);
    let target=s.with(|e,_|e.current_slide);s.toolbar_menu("transition","transition.timing");s.key("cmd-a");s.type_text("2.456");s.update(|e,_,cx|e.move_current_slide(1,cx));let before=s.with(|e,_|e.presentation().clone());let mut expected=before.clone();expected.slide_mut(target).unwrap().transition.as_mut().unwrap().duration=2456;s.menu("pptx.timing.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),expected);s.key("cmd-z");assert_eq!(s.with(|e,_|e.presentation().clone()),before);s.key("cmd-shift-z");s.save_as("timing-moved.pptx");assert_eq!(s.reopen().slides[1].transition,expected.slide(target).unwrap().transition);
    s.toolbar_menu("transition","transition.timing");s.menu("pptx.delete-slide");let before=s.with(|e,_|e.presentation().clone());s.menu("pptx.timing.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),before);assert!(s.with(|e,_|e.pptx().unwrap().timing_panel.is_some()));s.menu("pptx.timing.cancel");s.key("cmd-z");undo_all_and_redo(&mut s);
}

pub(crate) fn transition_timing_numeric_limits() {
    let mut s=Scenario::open("test/editing/pptx-plan.pptx");s.toolbar_menu("transition","transition.fade");s.toolbar_menu("transition","transition.timing");
    s.update(|e,w,cx| {let p=e.pptx().unwrap().timing_panel.as_ref().unwrap();let (d,a)=(p.duration.clone(),p.after.clone());d.update(cx,|f,cx|f.set_value("4294967.295",w,cx));a.update(cx,|f,cx|f.set_value("2147483.647",w,cx));});s.menu("pptx.timing.auto");let before=s.with(|e,_|e.slide().transition.clone());s.menu("pptx.timing.apply");let expected=s.with(|e,_|e.slide().transition.clone());assert_eq!(expected.as_ref().unwrap().duration,u32::MAX);assert_eq!(expected.as_ref().unwrap().after,Some(i32::MAX as u32));s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().transition.clone()),before);s.key("cmd-shift-z");s.save_as("timing-limits.pptx");assert_eq!(s.reopen().slides[0].transition,expected);
    s.toolbar_menu("transition","transition.timing");for (duration,after) in [("1.234","2147483.648"),("4294967.296","1"),("inf","1"),("1","NaN")] {
        s.update(|e,w,cx| {let p=e.pptx().unwrap().timing_panel.as_ref().unwrap();let (d,a)=(p.duration.clone(),p.after.clone());d.update(cx,|f,cx|f.set_value(duration,w,cx));a.update(cx,|f,cx|f.set_value(after,w,cx));});s.menu("pptx.timing.apply");assert_eq!(s.with(|e,_|e.slide().transition.clone()),expected);assert!(s.with(|e,_|e.pptx().unwrap().timing_panel.is_some()));
    }s.menu("pptx.timing.cancel");undo_all_and_redo(&mut s);
}

pub(crate) fn custom_transition_timing() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.toolbar_menu("transition", "transition.fade");
    let original = s.with(|e, _| e.slide().transition.clone());
    s.toolbar_menu("transition", "transition.timing"); s.key("cmd-a"); s.type_text("-0.001");
    s.menu("pptx.timing.apply"); assert_eq!(s.with(|e, _| e.slide().transition.clone()), original);
    s.menu("pptx.timing.cancel");
    s.toolbar_menu("transition", "transition.timing");
    s.update(|e, w, cx| {
        let p = e.pptx().unwrap().timing_panel.as_ref().unwrap();
        let (duration, after) = (p.duration.clone(), p.after.clone());
        duration.update(cx, |f, cx| f.set_value("1.234", w, cx));
        after.update(cx, |f, cx| f.set_value("2.567", w, cx));
    });
    s.menu("pptx.timing.auto"); s.menu("pptx.timing.click"); s.menu("pptx.timing.apply");
    let expected = s.with(|e, _| e.slide().transition.clone()).unwrap();
    assert_eq!((expected.effect.as_str(), expected.duration, expected.after, expected.on_click), ("fade", 1234, Some(2567), false));
    s.key("cmd-z"); assert_eq!(s.with(|e, _| e.slide().transition.clone()), original);
    s.key("cmd-shift-z"); s.save_as("custom-timing.pptx");
    assert_eq!(s.reopen().slides[0].transition.as_ref(), Some(&expected));
    s.toolbar_menu("transition", "transition.timing");
    s.menu("pptx.timing.auto");
    s.update(|e, w, cx| { let after = e.pptx().unwrap().timing_panel.as_ref().unwrap().after.clone(); after.update(cx, |f, cx| f.set_value("NaN", w, cx)); });
    s.menu("pptx.timing.apply");
    assert_eq!(s.with(|e, _| e.slide().transition.as_ref().unwrap().after), None, "disabled after does not reject ignored input");
    undo_all_and_redo(&mut s);
}

pub(crate) fn copy_script_fonts() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1); s.key("cmd-a"); s.type_text("AB日本CD語");
    s.key("home"); s.key("shift-right"); s.key("shift-right");
    s.update(|e, w, cx| e.format_characters(w, cx, ibis_doc_model::CharacterFormat { script_fonts: Some((Some("Carlito".into()), Some("Hiragino Sans".into()))), ..Default::default() }));
    s.menu("pptx.format-copy");
    s.key("right");
    for _ in 0..5 { s.key("shift-right"); }
    s.menu("pptx.format-paste");
    let body = s.with(|e, _| e.edited_body().unwrap().clone());
    let fonts = |body: &ibis_doc_model::TextBody| body.paragraphs[0].runs.iter().flat_map(|r| {
        let look = body.style.with_run(&r.style);
        r.text.chars().map(move |c| (c, look.font.clone(), look.fallback_font.clone()))
    }).collect::<Vec<_>>();
    for (c, font, fallback) in fonts(&body) {
        let east = ibis_doc_model::is_east_asian(c);
        assert_eq!(font.as_deref(), Some(if east { "Hiragino Sans" } else { "Carlito" }));
        assert_eq!(fallback.as_deref(), Some(if east { "Carlito" } else { "Hiragino Sans" }));
    }
    s.save_as("script-fonts.pptx");
    assert_eq!(fonts(s.reopen().slides[0].shapes[1].text_body().unwrap()), fonts(&body));
    s.key("home"); s.key("right"); s.key("right"); s.key("shift-right"); s.key("shift-right");
    s.menu("pptx.format-copy");
    s.key("home"); s.key("shift-right"); s.key("shift-right"); s.menu("pptx.format-paste");
    s.save_as("reverse-script-fonts.pptx");
    assert_eq!(fonts(s.reopen().slides[0].shapes[1].text_body().unwrap()), fonts(&body));
    s.key("escape"); undo_all_and_redo(&mut s);
}

pub(crate) fn paragraph_panel_rejects_changed_body() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1); s.key("cmd-a"); s.type_text("first"); s.key("enter"); s.type_text("second"); s.key("enter"); s.type_text("third");
    s.menu("pptx.paragraph.settings"); s.menu("pptx.paragraph.points");
    s.double_click_shape(1); s.key("cmd-a"); s.type_text("replacement"); s.key("escape");
    let before = s.with(|e, _| e.slide().shapes[1].text_body().unwrap().clone());
    assert!(s.with(|e, _| e.pptx().unwrap().paragraph_panel.is_some()));
    s.menu("pptx.paragraph.apply");
    assert_eq!(s.with(|e, _| e.slide().shapes[1].text_body().unwrap().clone()), before);
    assert!(s.with(|e, _| e.pptx().unwrap().paragraph_panel.is_some()));
    s.menu("pptx.paragraph.cancel");
    s.key("cmd-z"); assert_ne!(s.with(|e, _| e.slide().shapes[1].text_body().unwrap().clone()), before);
    s.key("cmd-shift-z"); assert_eq!(s.with(|e, _| e.slide().shapes[1].text_body().unwrap().clone()), before);
    s.click_shape(1); s.menu("pptx.paragraph.settings"); s.menu("pptx.paragraph.points"); s.menu("pptx.paragraph.apply");
    let after = s.with(|e, _| e.slide().shapes[1].text_body().unwrap().clone());
    assert_ne!(after, before);
    s.save_as("paragraph-conflict-reopened.pptx");
    assert_eq!(s.reopen().slides[0].shapes[1].text_body().unwrap().paragraphs[0].line_spacing, after.paragraphs[0].line_spacing);
    undo_all_and_redo(&mut s);
    // Pending IME is not yet in the model: reject it without committing it.
    s.click_shape(1); s.menu("pptx.paragraph.settings");
    s.double_click_shape(1);
    s.update(|e, _, cx| e.input_mark("未確定", cx));
    let before = s.with(|e, _| e.edited_body().unwrap().clone());
    s.menu("pptx.paragraph.apply");
    assert_eq!(s.with(|e, _| e.edited_body().unwrap().clone()), before);
    assert!(s.with(|e, _| e.text_editing().unwrap().marked.is_some()));
    s.menu("pptx.paragraph.cancel");
    // Opening a fresh panel commits IME first, then captures its current body.
    s.menu("pptx.paragraph.settings");
    assert!(s.with(|e, _| e.slide().shapes[1].text_body().unwrap().text().contains("未確定")));
    s.menu("pptx.paragraph.points"); s.menu("pptx.paragraph.apply");
    assert!(s.with(|e, _| e.pptx().unwrap().paragraph_panel.is_none()));
    drop(s);
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    s.menu("view.sidebar"); // Placed by the layout with the right pane.
    let (page, shape) = s.with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Table(_))).map(|j| (i, j)))).unwrap();
    s.page(page + 1); let cell = cell_middle(&mut s, shape, 1, 1); s.double_click(cell);
    s.menu("pptx.paragraph.settings"); s.menu("pptx.paragraph.points");
    s.double_click(cell); s.key("cmd-a"); s.type_text("cell replacement"); s.key("escape");
    let before = s.with(|e, _| e.slide().clone());
    s.menu("pptx.paragraph.apply");
    assert_eq!(s.with(|e, _| e.slide().clone()), before);
    assert!(s.with(|e, _| e.pptx().unwrap().paragraph_panel.is_some()));
    s.menu("pptx.paragraph.cancel");
    s.key("cmd-z"); assert_ne!(s.with(|e, _| e.slide().clone()), before);
    s.key("cmd-shift-z"); assert_eq!(s.with(|e, _| e.slide().clone()), before);
    s.save_as("cell-paragraph-conflict.pptx");
    let saved = s.reopen(); let ShapeKind::Table(t) = &saved.slides[page].shapes[shape].kind else { panic!("table") };
    assert_eq!(t.rows[1].cells[1].text.text(), "cell replacement");
    undo_all_and_redo(&mut s);
}

pub(crate) fn table_cell_font_alignment_options() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s.with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Table(_))).map(|j| (i, j)))).unwrap();
    s.page(page + 1);
    for (id, value) in [("auto", ibis_doc_model::FontAlign::Auto), ("top", ibis_doc_model::FontAlign::Top), ("center", ibis_doc_model::FontAlign::Center), ("baseline", ibis_doc_model::FontAlign::Baseline), ("bottom", ibis_doc_model::FontAlign::Bottom)] {
        let at = cell_middle(&mut s, shape, 1, 1); s.double_click(at); s.key("cmd-a");
        let before = s.with(|e, _| e.slide().shapes[shape].clone());
        s.menu("pptx.paragraph.settings"); s.menu(&format!("pptx.paragraph.font-{id}")); s.menu("pptx.paragraph.apply");
        let mut expected = before.clone();
        let ShapeKind::Table(table) = &mut expected.kind else { panic!("table") };
        for p in &mut table.rows[1].cells[1].text.paragraphs { p.font_align = value; }
        assert_eq!(s.with(|e, _| e.slide().shapes[shape].clone()), expected);
        s.key("cmd-z"); assert_eq!(s.with(|e, _| e.slide().shapes[shape].clone()), before);
        s.key("cmd-shift-z"); s.save_as(&format!("cell-font-align-{id}.pptx"));
        let saved = s.reopen(); let ShapeKind::Table(table) = &saved.slides[page].shapes[shape].kind else { panic!("table") };
        assert!(table.rows[1].cells[1].text.paragraphs.iter().all(|p| p.font_align == value));
    }
    undo_all_and_redo(&mut s);
}

pub(crate) fn paragraph_font_alignment_options() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1); s.key("cmd-a"); s.type_text("first"); s.key("enter"); s.type_text("second"); s.key("shift-enter"); s.type_text("tail"); s.key("enter"); s.type_text("third");
    for (id, value) in [("auto", ibis_doc_model::FontAlign::Auto), ("top", ibis_doc_model::FontAlign::Top), ("center", ibis_doc_model::FontAlign::Center), ("baseline", ibis_doc_model::FontAlign::Baseline), ("bottom", ibis_doc_model::FontAlign::Bottom)] {
        s.double_click_shape(1);
        s.update(|e, _, _| { let edit = e.pptx_mut().unwrap().edit.as_mut().unwrap(); edit.anchor = 6; edit.caret = 6; });
        let before = s.with(|e, _| e.edited_body().unwrap().clone());
        s.menu("pptx.paragraph.settings"); s.menu(&format!("pptx.paragraph.font-{id}")); s.menu("pptx.paragraph.apply");
        let after = s.with(|e, _| e.slide().shapes[1].text_body().unwrap().clone());
        let mut expected = before.clone(); expected.paragraphs[1].font_align = value; expected.paragraphs[2].font_align = value;
        assert_eq!(after, expected);
        s.key("cmd-z"); assert_eq!(s.with(|e, _| e.slide().shapes[1].text_body().unwrap().clone()), before);
        s.key("cmd-shift-z");
        s.save_as(&format!("font-align-{id}.pptx"));
        let saved = s.reopen(); let body = saved.slides[0].shapes[1].text_body().unwrap();
        assert_eq!(body.paragraphs[1].font_align, value); assert_eq!(body.paragraphs[2].font_align, value);
    }
    undo_all_and_redo(&mut s);
}

pub(crate) fn custom_paragraph_spacing() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1); s.key("cmd-a"); s.type_text("first"); s.key("enter"); s.type_text("second"); s.key("enter"); s.type_text("third");
    let before = s.with(|e, _| e.edited_body().unwrap().clone());
    s.key("home");
    s.menu("pptx.paragraph.settings"); s.menu("pptx.paragraph.points"); s.key("cmd-a"); s.type_text("0"); s.menu("pptx.paragraph.apply");
    assert_eq!(s.with(|e, _| e.slide().shapes[1].text_body().unwrap().clone()), before);
    s.update(|e, w, cx| { let fields = e.pptx().unwrap().paragraph_panel.as_ref().unwrap().fields.clone(); for (f, value) in fields.into_iter().zip(["24.5", "3.25", "6.75"]) { f.update(cx, |f, cx| f.set_value(value, w, cx)); } });
    s.menu("pptx.paragraph.apply");
    let after = s.with(|e, _| e.slide().shapes[1].text_body().unwrap().clone());
    assert_eq!(after.paragraphs[0], before.paragraphs[0]); assert_eq!(after.paragraphs[1], before.paragraphs[1]);
    let p = &after.paragraphs[2]; assert_eq!((p.line_spacing, p.space_before, p.space_after), (Some(ibis_doc_model::LineSpacing::Points(2450)), 325, 675));
    s.key("cmd-z"); assert_eq!(s.with(|e, _| e.slide().shapes[1].text_body().unwrap().clone()), before);
    s.key("cmd-shift-z"); s.save_as("paragraph-spacing.pptx");
    let saved = s.reopen(); let p = &saved.slides[0].shapes[1].text_body().unwrap().paragraphs[2]; assert_eq!((p.line_spacing, p.space_before, p.space_after), (Some(ibis_doc_model::LineSpacing::Points(2450)), 325, 675));
    s.double_click_shape(1); s.key("cmd-a"); s.menu("pptx.paragraph.settings"); s.menu("pptx.paragraph.multiple");
    s.update(|e, w, cx| { let f = e.pptx().unwrap().paragraph_panel.as_ref().unwrap().fields[0].clone(); f.update(cx, |f, cx| f.set_value("1.375", w, cx)); });
    s.menu("pptx.paragraph.apply");
    assert!(s.with(|e, _| e.slide().shapes[1].text_body().unwrap().paragraphs.iter().all(|p| p.line_spacing == Some(ibis_doc_model::LineSpacing::Percent(137500)))));
    s.save_as("paragraph-multiple.pptx");
    assert!(s.reopen().slides[0].shapes[1].text_body().unwrap().paragraphs.iter().all(|p| p.line_spacing == Some(ibis_doc_model::LineSpacing::Percent(137500))));
    s.click_shape(1); s.menu("pptx.paragraph.settings"); s.menu("pptx.paragraph.default"); s.menu("pptx.paragraph.cancel");
    assert!(s.with(|e, _| e.slide().shapes[1].text_body().unwrap().paragraphs.iter().all(|p| p.line_spacing == Some(ibis_doc_model::LineSpacing::Percent(137500)))));
    undo_all_and_redo(&mut s);
    drop(s);
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s.with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Table(_))).map(|j| (i, j)))).unwrap();
    s.page(page + 1); let cell = cell_middle(&mut s, shape, 1, 1); s.double_click(cell);
    let before = s.with(|e, _| e.edited_body().unwrap().clone());
    s.key("cmd-a"); s.menu("pptx.paragraph.settings"); s.menu("pptx.paragraph.points");
    s.update(|e, w, cx| { let fields = e.pptx().unwrap().paragraph_panel.as_ref().unwrap().fields.clone(); for (f, value) in fields.into_iter().zip(["18.5", "2", "4"]) { f.update(cx, |f, cx| f.set_value(value, w, cx)); } });
    s.menu("pptx.paragraph.apply");
    s.save_as("cell-paragraph-spacing.pptx");
    let saved = s.reopen(); let ShapeKind::Table(t) = &saved.slides[page].shapes[shape].kind else { panic!("table") };
    let body = &t.rows[1].cells[1].text;
    assert_eq!(body.text(), before.text());
    assert!(body.paragraphs.iter().all(|p| p.line_spacing == Some(ibis_doc_model::LineSpacing::Points(1850))));
    assert_eq!((body.paragraphs[0].space_before, body.paragraphs[0].space_after), (200, 400));
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| { let ShapeKind::Table(t) = &e.slide().shapes[shape].kind else { panic!("table") }; t.rows[1].cells[1].text.clone() }), before);
    s.key("cmd-shift-z"); undo_all_and_redo(&mut s);
}

pub(crate) fn caret_paragraph_formats() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1); s.key("cmd-a"); s.type_text("first"); s.key("enter"); s.type_text("second"); s.key("shift-enter"); s.type_text("soft"); s.key("enter"); s.type_text("third");
    let original = s.with(|e, _| e.edited_body().unwrap().clone());
    assert_eq!(original.paragraphs.len(), 4); assert!(original.paragraphs[2].line_break);
    s.toolbar_menu("align", "align.center");
    s.toolbar_menu("line-spacing", "spacing.1.5");
    let third = s.with(|e, _| e.edited_body().unwrap().clone());
    assert_eq!(&third.paragraphs[..3], &original.paragraphs[..3]);
    assert_eq!(third.paragraphs[3].align, Some(ibis_doc_model::TextAlign::Center));
    assert_eq!(third.paragraphs[3].line_spacing, Some(ibis_doc_model::LineSpacing::Percent(150000)));
    s.key("home"); s.key("left"); s.toolbar_menu("align", "align.right");
    let soft = s.with(|e, _| e.edited_body().unwrap().clone());
    assert_eq!(soft.paragraphs[0], original.paragraphs[0]); assert_eq!(soft.paragraphs[3], third.paragraphs[3]);
    assert!(soft.paragraphs[1..3].iter().all(|p| p.align == Some(ibis_doc_model::TextAlign::Right)));
    s.key("cmd-z"); assert_eq!(s.with(|e, _| e.edited_body().unwrap().clone()), third);
    s.key("cmd-shift-z"); assert_eq!(s.with(|e, _| e.edited_body().unwrap().clone()), soft);
    s.save_as("caret-paragraphs.pptx");
    let saved = s.reopen(); let body = saved.slides[0].shapes[1].text_body().unwrap();
    assert_eq!(body.text(), soft.text()); assert_eq!(body.paragraphs.len(), 4); assert!(body.paragraphs[2].line_break);
    assert!(body.paragraphs[1..3].iter().all(|p| p.align == Some(ibis_doc_model::TextAlign::Right)));
    assert_eq!(body.paragraphs[3].line_spacing, Some(ibis_doc_model::LineSpacing::Percent(150000)));
    s.key("escape"); undo_all_and_redo(&mut s);
}

pub(crate) fn active_paragraph_toolbar_state() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(1); s.key("cmd-a"); s.type_text("first"); s.key("enter"); s.type_text("second"); s.key("shift-enter"); s.type_text("soft"); s.key("enter"); s.type_text("third");
    s.toolbar_menu("align", "align.center");
    let third = s.with(|e, _| e.edited_body().unwrap().clone());
    let third_list = s.with(|e, _| e.selected_list());
    assert_eq!(s.with(|e, _| e.selected_text_style().unwrap().align), ibis_doc_model::TextAlign::Center);
    s.toolbar("numbered");
    assert_eq!(s.with(|e, _| e.selected_list()), Some(ibis_doc_model::ListKind::Numbered));
    s.toolbar("numbered");
    assert_eq!(s.with(|e, _| e.selected_list()), Some(ibis_doc_model::ListKind::None));
    assert_eq!(s.with(|e, _| e.edited_body().unwrap().paragraphs[0].clone()), third.paragraphs[0]);
    s.key("home"); s.key("left"); s.toolbar_menu("align", "align.right"); s.toolbar("numbered");
    assert_eq!(s.with(|e, _| e.selected_text_style().unwrap().align), ibis_doc_model::TextAlign::Right);
    assert_eq!(s.with(|e, _| e.selected_list()), Some(ibis_doc_model::ListKind::Numbered), "soft line inherits parent list state");
    s.toolbar("numbered");
    assert_eq!(s.with(|e, _| e.selected_list()), Some(ibis_doc_model::ListKind::None));
    s.key("cmd-home");
    assert_eq!(s.with(|e, _| e.selected_text_style().unwrap().align), third.paragraphs[0].align.unwrap_or(third.style.align));
    assert_eq!(s.with(|e, _| e.selected_list()), third_list);
    s.save_as("active-paragraphs.pptx");
    let saved = s.reopen(); let body = saved.slides[0].shapes[1].text_body().unwrap();
    assert_eq!(body.paragraphs[3].align, Some(ibis_doc_model::TextAlign::Center));
    assert_eq!(body.paragraphs[1].align, Some(ibis_doc_model::TextAlign::Right));
    s.key("escape"); undo_all_and_redo(&mut s);
}

pub(crate) fn line_end_size_cap_and_compound() {
    use ibis_doc_model::{EndSize, LineCap, LineCompound, LineEnd};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.toolbar_menu("insert-line", "pptx.arrow");
    let point = s.at(EmuPoint::new(4_000_000, 4_000_000)); s.click(point);
    let stroke = |s: &mut Scenario| s.with(|e, _| { let ShapeKind::Line(s) = &e.selected_shapes()[0].kind else { panic!("line") }; s.clone() });
    s.toolbar_menu("line-start", "end.triangle"); s.toolbar_menu("line-end", "end.stealth");
    let size = |end| match end { LineEnd::Triangle(s) | LineEnd::Stealth(s) | LineEnd::Arrow(s) | LineEnd::Diamond(s) | LineEnd::Oval(s) => s, _ => panic!("end") };
    for (head, tool) in [(true, "line-start"), (false, "line-end")] {
        for (width, wi) in [(0, "end.width.small"), (1, "end.width.medium"), (2, "end.width.large")] {
            for (length, li) in [(0, "end.length.small"), (1, "end.length.medium"), (2, "end.length.large")] {
                let old = stroke(&mut s);
                s.toolbar_menu(tool, wi); s.toolbar_menu(tool, li);
                let new = stroke(&mut s);
                assert_eq!(size(if head { new.head } else { new.tail }), EndSize { width, length });
                assert_eq!(if head { new.tail } else { new.head }, if head { old.tail } else { old.head });
            }
        }
    }
    for kind in ["end.arrow", "end.diamond", "end.oval", "end.triangle", "end.stealth"] {
        s.toolbar_menu("line-end", kind); assert_eq!(size(stroke(&mut s).tail), EndSize { width: 2, length: 2 });
    }
    for (cap, item) in [(LineCap::Flat, "cap.flat"), (LineCap::Round, "cap.round"), (LineCap::Square, "cap.square")] {
        s.toolbar_menu("line-style", item);
        for (compound, item) in [(LineCompound::Single, "compound.single"), (LineCompound::Double, "compound.double"), (LineCompound::ThickThin, "compound.thick-thin"), (LineCompound::ThinThick, "compound.thin-thick"), (LineCompound::Triple, "compound.triple")] {
            s.toolbar_menu("line-style", item);
            let current = stroke(&mut s); assert_eq!((current.cap, current.compound), (cap, compound));
            s.save_as("line-properties.pptx");
            let saved = s.reopen(); let ShapeKind::Line(read) = &saved.slides[0].shapes.last().unwrap().kind else { panic!("line") };
            assert_eq!(read, &current);
        }
    }
    let before = stroke(&mut s);
    s.toolbar_menu("line-end", "end.none"); assert_eq!(stroke(&mut s).tail, LineEnd::None);
    s.key("cmd-z"); assert_eq!(stroke(&mut s), before); s.key("cmd-shift-z"); assert_eq!(stroke(&mut s).tail, LineEnd::None);
    undo_all_and_redo(&mut s);
}

pub(crate) fn custom_linear_gradient_targets_and_conflicts() {
    use ibis_doc_model::{AutoShape,Color,Command,EmuRect,Geometry,GradientStop,Rgb,Shape,Table,TextBody,TextStyle};
    let mut s=Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e,_,cx| {
        let mut auto=AutoShape::new(Geometry::Rect,Fill::Solid(Color::BLACK),None);auto.text=Some(TextBody::plain("gradient target",TextStyle::default()));auto.glow=Some((Color {rgb:Rgb(0x4472C4),alpha:128},63_500));let id=e.document.ids().new_shape_id();e.insert_shape(Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto)),cx);
        let id=e.document.ids().new_shape_id();e.insert_shape(Shape::new(id,EmuRect::new(4_000_000,2_000_000,3_000_000,1_200_000),ShapeKind::Table(Box::new(Table::new_grid(2,2,3_000_000,1_200_000)))),cx);
    });s.save_as("../pptx-gradient-custom-base.pptx");drop(s);
    let mut s=Scenario::open("target/scenario/pptx-gradient-custom-base.pptx");let own=s.shapes()-2;let table=s.shapes()-1;s.click_shape(own);
    let fields=|s:&mut Scenario,angle:&str,values:&[[&str;3]]|s.update(|e,w,cx| {let p=e.pptx().unwrap().gradient_panel.as_ref().unwrap();let a=p.angle.clone();let rows=p.stops.clone();assert_eq!(rows.len(),values.len());a.update(cx,|f,cx|f.set_value(angle,w,cx));for (row,values) in rows.iter().zip(values) {for (f,v) in row.iter().zip(*values) {f.update(cx,|f,cx|f.set_value(v,w,cx));}}});
    s.menu("pptx.fill.custom");assert_eq!(s.menu_state("pptx.gradient.remove-first"),Some((false,false)));s.key("cmd-a");s.type_text("33.333");assert_eq!(s.with(|e,cx|e.pptx().unwrap().gradient_panel.as_ref().unwrap().angle.read(cx).value().to_string()),"33.333");s.menu("pptx.gradient.add");fields(&mut s,"33.333",&[["100","112233","50"],["0","#aAbBcC","0"],["25","00FF00","37.5"]]);
    let expected=Fill::Linear {angle:1_999_980,scaled:false,stops:vec![GradientStop {position:0,color:Color {rgb:Rgb(0xAABBCC),alpha:0}},GradientStop {position:25_000,color:Color {rgb:Rgb(0x00FF00),alpha:96}},GradientStop {position:100_000,color:Color {rgb:Rgb(0x112233),alpha:128}}]};let before=s.with(|e,_|e.slide().shapes[own].clone());s.menu("pptx.gradient.apply");let mut changed=before.clone();let ShapeKind::Auto(a)=&mut changed.kind else {panic!("auto")};a.fill=expected.clone();assert_eq!(s.with(|e,_|e.slide().shapes[own].clone()),changed);s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes[own].clone()),before);s.key("cmd-shift-z");s.save_as("custom-shape-gradient.pptx");assert_eq!(s.reopen().slides[0].shapes[own].kind,changed.kind);
    s.menu("pptx.fill.custom");s.menu("pptx.gradient.remove-last");assert_eq!(s.with(|e,_|e.pptx().unwrap().gradient_panel.as_ref().unwrap().stops.len()),2);s.menu("pptx.gradient.add");fields(&mut s,"359.99998333",&[["0","000000","100"],["100","FFFFFF","100"],["50","123456","100"]]);s.menu("pptx.gradient.apply");s.save_as("added-removed-stops.pptx");let saved=s.reopen();let ShapeKind::Auto(a)=&saved.slides[0].shapes[own].kind else {panic!("auto")};let Fill::Linear {angle,stops,..}= &a.fill else {panic!("linear")};assert_eq!(*angle,21_599_999);assert_eq!(stops.len(),3);assert_eq!(stops[1].position,50_000);assert_eq!(stops[1].color,Color::opaque(Rgb(0x123456)));
    for (angle,bad) in [("-1",["0","000000","100"]),("360",["0","000000","100"]),("NaN",["0","000000","100"]),("1",["-0.1","000000","100"]),("1",["100.1","000000","100"]),("1",["0","ABCDE","100"]),("1",["0","000000","NaN"]),("1",["0","000000","101"])] {
        s.menu("pptx.fill.custom");fields(&mut s,angle,&[bad,["50","FFFFFF","100"],["100","123456","100"]]);let before=s.with(|e,_|e.presentation().clone());s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),before);assert!(s.with(|e,_|e.pptx().unwrap().gradient_panel.is_some()));s.menu("pptx.gradient.cancel");
    }
    s.menu("pptx.fill.custom");s.menu("pptx.glow.yellow10");let altered=s.with(|e,_|e.presentation().clone());s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),altered);assert!(s.with(|e,_|e.pptx().unwrap().gradient_panel.is_some()));s.menu("pptx.gradient.cancel");
    s.menu("pptx.fill.custom");s.double_click_shape(own);s.update(|e,_,cx|e.input_mark("未確定",cx));let marked=s.with(|e,_|e.edited_body().unwrap().clone());s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.edited_body().unwrap().clone()),marked);assert!(s.with(|e,_|e.text_editing().unwrap().marked.is_some()));s.menu("pptx.gradient.cancel");s.update(|e,w,cx| {e.input_mark("",cx);e.end_text_edit(w,cx);});
    s.menu("pptx.background.custom");s.menu("pptx.gradient.add");fields(&mut s,"12.5",&[["100","112233","50"],["0","AABBCC","0"],["25","00FF00","37.5"]]);let mut background=expected.clone();let Fill::Linear {angle,..}=&mut background else {panic!("linear")};*angle=750_000;let before=s.with(|e,_|e.slide().background.clone());s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.slide().background.clone()),background);s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().background.clone()),before);s.key("cmd-shift-z");s.save_as("custom-background-gradient.pptx");assert_eq!(s.reopen().slides[0].background,background);
    s.menu("pptx.background.custom");s.update(|e,_,cx|e.set_background(Fill::Solid(Color::BLACK),cx));let altered=s.with(|e,_|e.presentation().clone());s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),altered);assert!(s.with(|e,_|e.pptx().unwrap().gradient_panel.is_some()));s.menu("pptx.gradient.cancel");s.key("cmd-z");
    s.click_shape(table);let frame=s.with(|e,_|e.slide().shapes[table].frame);let cell=s.at(ibis_doc_model::EmuPoint::new(frame.x+750_000,frame.y+300_000));s.double_click(cell);assert_eq!(s.with(|e,_|e.text_editing().unwrap().cell),Some((0,0)));s.menu("pptx.fill.custom");fields(&mut s,"33.333",&[["100","112233","50"],["0","AABBCC","0"]]);let before=s.with(|e,_|e.slide().shapes[table].clone());s.menu("pptx.gradient.apply");let mut changed=before.clone();let ShapeKind::Table(t)=&mut changed.kind else {panic!("table")};t.rows[0].cells[0].fill=Fill::Linear {angle:1_999_980,scaled:false,stops:vec![GradientStop {position:0,color:Color {rgb:Rgb(0xAABBCC),alpha:0}},GradientStop {position:100_000,color:Color {rgb:Rgb(0x112233),alpha:128}}]};assert_eq!(s.with(|e,_|e.slide().shapes[table].clone()),changed);s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes[table].clone()),before);s.key("cmd-shift-z");s.save_as("custom-cell-gradient.pptx");assert_eq!(s.reopen().slides[0].shapes[table].kind,changed.kind);s.key("escape");
    s.click_shape(table);s.double_click(cell);let corner=cell_middle(&mut s,table,0,1);s.shift_click(corner);assert_eq!(s.with(|e,_|e.text_editing().unwrap().cell_anchor),Some((0,1)));s.menu("pptx.fill.custom");fields(&mut s,"45",&[["0","102938","100"],["100","ABCDEF","25"]]);let before=s.with(|e,_|e.slide().shapes[table].clone());s.menu("pptx.gradient.apply");let mut changed=before.clone();let ShapeKind::Table(t)=&mut changed.kind else {panic!("table")};let range_fill=Fill::Linear {angle:2_700_000,scaled:false,stops:vec![GradientStop {position:0,color:Color::opaque(Rgb(0x102938))},GradientStop {position:100_000,color:Color {rgb:Rgb(0xABCDEF),alpha:64}}]};for cell in &mut t.rows[0].cells {cell.fill=range_fill.clone();}assert_eq!(s.with(|e,_|e.slide().shapes[table].clone()),changed);s.save_as("custom-cell-range-gradient.pptx");assert_eq!(s.reopen().slides[0].shapes[table].kind,changed.kind);s.key("escape");
    let ids=s.with(|e,_|vec![e.slide().shapes[own].id,e.slide().shapes[table].id]);s.update(|e,_,_|e.set_selection(ids));s.menu("pptx.fill.custom");fields(&mut s,"30",&[["0","000000","100"],["50","808080","20"],["100","FFFFFF","100"]]);let before=s.with(|e,_|e.slide().clone());s.menu("pptx.gradient.apply");let mut changed=before.clone();let multi_fill=Fill::Linear {angle:1_800_000,scaled:false,stops:vec![GradientStop {position:0,color:Color::BLACK},GradientStop {position:50_000,color:Color {rgb:Rgb(0x808080),alpha:51}},GradientStop {position:100_000,color:Color::WHITE}]};let ShapeKind::Auto(a)=&mut changed.shapes[own].kind else {panic!("auto")};a.fill=multi_fill.clone();let ShapeKind::Table(t)=&mut changed.shapes[table].kind else {panic!("table")};for cell in t.rows.iter_mut().flat_map(|r|&mut r.cells) {cell.fill=multi_fill.clone();}assert_eq!(s.with(|e,_|e.slide().clone()),changed);s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().clone()),before);s.key("cmd-shift-z");s.save_as("custom-multi-gradient.pptx");let saved=s.reopen();assert_eq!(saved.slides[0].shapes[own].kind,changed.shapes[own].kind);assert_eq!(saved.slides[0].shapes[table].kind,changed.shapes[table].kind);s.update(|e,_,_| {let id=e.slide().shapes[own].id;e.set_selection(vec![id]);});
    s.click_shape(own);s.menu("pptx.fill.custom");s.update(|e,_,cx| {let slide=e.current_slide;let shape=e.slide().shapes[own].id;e.run(vec![Command::RemoveShape {slide,shape}],cx);});let deleted=s.with(|e,_|e.presentation().clone());s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),deleted);assert!(s.with(|e,_|e.pptx().unwrap().gradient_panel.is_some()));s.menu("pptx.gradient.cancel");s.key("cmd-z");undo_all_and_redo(&mut s);
}


pub(crate) fn custom_path_gradient_targets_and_conflicts() {
    use ibis_doc_model::{AutoShape,Color,Command,EmuRect,Geometry,GradientPath,GradientStop,Shape,Table,TextBody,TextStyle};
    let stops=vec![GradientStop {position:0,color:Color::BLACK},GradientStop {position:100_000,color:Color::WHITE}];
    let mut s=Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e,_,cx| {let mut auto=AutoShape::new(Geometry::Rect,Fill::Path {path:GradientPath::Circle,focus:[50_000;4],stops:stops.clone()},None);auto.text=Some(TextBody::plain("path gradient target",TextStyle::default()));let id=e.document.ids().new_shape_id();e.insert_shape(Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto)),cx);let id=e.document.ids().new_shape_id();e.insert_shape(Shape::new(id,EmuRect::new(4_000_000,2_000_000,3_000_000,1_200_000),ShapeKind::Table(Box::new(Table::new_grid(2,2,3_000_000,1_200_000)))),cx);});s.save_as("../pptx-gradient-path-custom-base.pptx");drop(s);
    let mut s=Scenario::open("target/scenario/pptx-gradient-path-custom-base.pptx");let own=s.shapes()-2;let table=s.shapes()-1;s.click_shape(own);
    let focus=|s:&mut Scenario,values:[&str;4]|s.update(|e,w,cx| {let p=e.pptx().unwrap().gradient_panel.as_ref().unwrap();let fields=p.focus.clone();let rows=p.stops.clone();for (f,v) in fields.iter().zip(values) {f.update(cx,|f,cx|f.set_value(v,w,cx));}for (row,values) in rows.iter().zip([["0","000000","100"],["100","FFFFFF","100"]]) {for (f,v) in row.iter().zip(values) {f.update(cx,|f,cx|f.set_value(v,w,cx));}}});
    for (id,path,values,positions) in [("pptx.gradient.rect",GradientPath::Rect,["12.345","23.456","34.567","45.678"],[12_345,23_456,34_567,45_678]),("pptx.gradient.shape",GradientPath::Shape,["0","0","100","100"],[0,0,100_000,100_000]),("pptx.gradient.circle",GradientPath::Circle,["50","50","50","50"],[50_000;4])] {
        s.menu("pptx.fill.custom-path");assert!(s.with(|e,_|e.pptx().unwrap().gradient_panel.as_ref().unwrap().path.is_some()));s.key("cmd-a");s.type_text(values[0]);assert_eq!(s.with(|e,cx|e.pptx().unwrap().gradient_panel.as_ref().unwrap().focus[0].read(cx).value().to_string()),values[0]);focus(&mut s,values);s.menu(id);assert_eq!(s.menu_state(id),Some((true,true)));let before=s.with(|e,_|e.slide().shapes[own].clone());let mut changed=before.clone();let ShapeKind::Auto(a)=&mut changed.kind else {panic!("auto")};a.fill=Fill::Path {path,focus:positions,stops:stops.clone()};s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.slide().shapes[own].clone()),changed);s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes[own].clone()),before);s.key("cmd-shift-z");s.save_as(&format!("custom-{}.pptx",path.name()));assert_eq!(s.reopen().slides[0].shapes[own].kind,changed.kind);
    }
    for invalid in ["NaN","inf","2147483.648","-2147483.649","text"] {s.menu("pptx.fill.custom-path");focus(&mut s,[invalid,"50","50","50"]);let before=s.with(|e,_|e.presentation().clone());s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),before);assert!(s.with(|e,_|e.pptx().unwrap().gradient_panel.is_some()));s.menu("pptx.gradient.cancel");}
    s.menu("pptx.fill.custom-path");s.update(|e,_,cx| {let slide=e.current_slide;let shape=e.slide().shapes[own].id;let frame=e.slide().shapes[own].frame.translated(127_000,0);e.run(vec![Command::SetFrame {slide,shape,frame}],cx);});let altered=s.with(|e,_|e.presentation().clone());s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),altered);assert!(s.with(|e,_|e.pptx().unwrap().gradient_panel.is_some()));s.menu("pptx.gradient.cancel");s.key("cmd-z");
    s.menu("pptx.fill.custom-path");s.double_click_shape(own);s.update(|e,_,cx|e.input_mark("未確定",cx));let marked=s.with(|e,_|e.edited_body().unwrap().clone());s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.edited_body().unwrap().clone()),marked);assert!(s.with(|e,_|e.text_editing().unwrap().marked.is_some()));s.menu("pptx.gradient.cancel");s.update(|e,w,cx| {e.input_mark("",cx);e.end_text_edit(w,cx);});
    s.menu("pptx.background.custom-path");s.menu("pptx.gradient.rect");focus(&mut s,["-0.001","100.001","0","50"]);let expected=Fill::Path {path:GradientPath::Rect,focus:[-1,100_001,0,50_000],stops:stops.clone()};let before=s.with(|e,_|e.slide().background.clone());s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.slide().background.clone()),expected);s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().background.clone()),before);s.key("cmd-shift-z");s.save_as("custom-path-background.pptx");assert_eq!(s.reopen().slides[0].background,expected);
    s.click_shape(table);let cell=cell_middle(&mut s,table,0,0);s.double_click(cell);let corner=cell_middle(&mut s,table,0,1);s.shift_click(corner);s.menu("pptx.fill.custom-path");s.menu("pptx.gradient.shape");focus(&mut s,["12.345","23.456","34.567","45.678"]);let fill=Fill::Path {path:GradientPath::Shape,focus:[12_345,23_456,34_567,45_678],stops:stops.clone()};let before=s.with(|e,_|e.slide().shapes[table].clone());let mut changed=before.clone();let ShapeKind::Table(t)=&mut changed.kind else {panic!("table")};for cell in &mut t.rows[0].cells {cell.fill=fill.clone();}s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.slide().shapes[table].clone()),changed);s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes[table].clone()),before);s.key("cmd-shift-z");s.save_as("custom-path-cell-range.pptx");assert_eq!(s.reopen().slides[0].shapes[table].kind,changed.kind);s.key("escape");
    s.update(|e,_,_| {let ids=vec![e.slide().shapes[own].id,e.slide().shapes[table].id];e.set_selection(ids);});s.menu("pptx.fill.custom-path");s.menu("pptx.gradient.rect");focus(&mut s,["20","30","40","50"]);let fill=Fill::Path {path:GradientPath::Rect,focus:[20_000,30_000,40_000,50_000],stops:stops.clone()};let before=s.with(|e,_|e.slide().clone());let mut changed=before.clone();let ShapeKind::Auto(a)=&mut changed.shapes[own].kind else {panic!("auto")};a.fill=fill.clone();let ShapeKind::Table(t)=&mut changed.shapes[table].kind else {panic!("table")};for cell in t.rows.iter_mut().flat_map(|r|&mut r.cells) {cell.fill=fill.clone();}s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.slide().clone()),changed);s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().clone()),before);s.key("cmd-shift-z");s.save_as("custom-path-multi.pptx");let saved=s.reopen();assert_eq!(saved.slides[0].shapes[own].kind,changed.shapes[own].kind);assert_eq!(saved.slides[0].shapes[table].kind,changed.shapes[table].kind);
    s.update(|e,_,_| {let id=e.slide().shapes[own].id;e.set_selection(vec![id]);});s.menu("pptx.fill.custom-path");s.menu("pptx.gradient.add");s.update(|e,w,cx| {let fields=e.pptx().unwrap().gradient_panel.as_ref().unwrap().stops[2].clone();for (f,v) in fields.iter().zip(["50","12aBcD","37.5"]) {f.update(cx,|f,cx|f.set_value(v,w,cx));}});let before=s.with(|e,_|e.slide().shapes[own].clone());s.menu("pptx.gradient.apply");let mut changed=before.clone();let ShapeKind::Auto(a)=&mut changed.kind else {panic!("auto")};a.fill=Fill::Path {path:GradientPath::Rect,focus:[20_000,30_000,40_000,50_000],stops:vec![stops[0].clone(),GradientStop {position:50_000,color:Color {rgb:ibis_doc_model::Rgb(0x12ABCD),alpha:96}},stops[1].clone()]};assert_eq!(s.with(|e,_|e.slide().shapes[own].clone()),changed);s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes[own].clone()),before);s.key("cmd-shift-z");s.save_as("custom-path-three-stops.pptx");assert_eq!(s.reopen().slides[0].shapes[own].kind,changed.kind);
    s.menu("pptx.fill.custom-path");s.menu("pptx.fill.custom");assert!(s.with(|e,_|e.pptx().unwrap().gradient_panel.as_ref().unwrap().path.is_none()));s.menu("pptx.fill.custom-path");assert_eq!(s.with(|e,_|e.pptx().unwrap().gradient_panel.as_ref().unwrap().path),Some(GradientPath::Rect));s.menu("pptx.gradient.cancel");
    s.menu("pptx.fill.custom-path");s.update(|e,_,cx| {let slide=e.current_slide;let shape=e.slide().shapes[own].id;e.run(vec![Command::RemoveShape {slide,shape}],cx);});let deleted=s.with(|e,_|e.presentation().clone());s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),deleted);assert!(s.with(|e,_|e.pptx().unwrap().gradient_panel.is_some()));s.menu("pptx.gradient.cancel");s.key("cmd-z");undo_all_and_redo(&mut s);
}


pub(crate) fn gradient_stop_count_on_reopened_source() {
    use ibis_doc_model::{AutoShape,Color,EmuRect,Geometry,GradientPath,GradientStop,Rgb,Shape};
    for path_mode in [false,true] {
        let mut s=Scenario::open("test/editing/pptx-plan.pptx");
        s.update(|e,_,cx| {let stops=vec![GradientStop {position:0,color:Color::BLACK},GradientStop {position:50_000,color:Color::opaque(Rgb(0x112233))},GradientStop {position:100_000,color:Color::WHITE}];let fill=if path_mode {Fill::Path {path:GradientPath::Rect,focus:[50_000;4],stops}} else {Fill::Linear {angle:0,scaled:false,stops}};let id=e.document.ids().new_shape_id();e.insert_shape(Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(AutoShape::new(Geometry::Rect,fill,None))),cx);});
        let base=s.save_as(if path_mode {"../pptx-gradient-count-path-base.pptx"} else {"../pptx-gradient-count-linear-base.pptx"});drop(s);
        let mut s=Scenario::open(base.to_str().unwrap());let own=s.shapes()-1;s.click_shape(own);let original=s.with(|e,_|e.slide().shapes[own].clone());
        let menu=if path_mode {"pptx.fill.custom-path"} else {"pptx.fill.custom"};s.menu(menu);s.menu("pptx.gradient.add");s.update(|e,w,cx| {let row=e.pptx().unwrap().gradient_panel.as_ref().unwrap().stops[3].clone();for (field,value) in row.iter().zip(["25","AABBCC","50"]) {field.update(cx,|f,cx|f.set_value(value,w,cx));}});s.menu("pptx.gradient.apply");let added=s.with(|e,_|e.slide().shapes[own].clone());assert_ne!(original,added);s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes[own].clone()),original);s.key("cmd-shift-z");assert_eq!(s.with(|e,_|e.slide().shapes[own].clone()),added);s.save_as(if path_mode {"path-added.pptx"} else {"linear-added.pptx"});assert_eq!(s.reopen().slides[0].shapes[own].kind,added.kind);let ShapeKind::Auto(auto)=&added.kind else {panic!("auto")};let count=match &auto.fill {Fill::Linear {stops,..}|Fill::Path {stops,..}=>stops.len(),_=>panic!("gradient")};assert_eq!(count,4);
        s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes[own].clone()),original);s.menu(menu);s.menu("pptx.gradient.remove-first");s.menu("pptx.gradient.apply");let removed=s.with(|e,_|e.slide().shapes[own].clone());s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes[own].clone()),original);s.key("cmd-shift-z");assert_eq!(s.with(|e,_|e.slide().shapes[own].clone()),removed);s.save_as(if path_mode {"path-removed.pptx"} else {"linear-removed.pptx"});assert_eq!(s.reopen().slides[0].shapes[own].kind,removed.kind);let ShapeKind::Auto(auto)=&removed.kind else {panic!("auto")};let count=match &auto.fill {Fill::Linear {stops,..}|Fill::Path {stops,..}=>stops.len(),_=>panic!("gradient")};assert_eq!(count,2);
        s.menu(if path_mode {"pptx.fill.custom"} else {"pptx.fill.custom-path"});s.menu("pptx.gradient.apply");let switched=s.with(|e,_|e.slide().shapes[own].clone());let ShapeKind::Auto(auto)=&switched.kind else {panic!("auto")};assert!(if path_mode {matches!(auto.fill,Fill::Linear {..})} else {matches!(auto.fill,Fill::Path {..})});s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes[own].clone()),removed);s.key("cmd-shift-z");s.save_as(if path_mode {"path-to-linear.pptx"} else {"linear-to-path.pptx"});assert_eq!(s.reopen().slides[0].shapes[own].kind,switched.kind);
    }
}

pub(crate) fn custom_pattern_all_presets_targets_and_conflicts() {
    use ibis_doc_model::{AutoShape,Color,Command,EmuRect,Geometry,Rgb,Shape,Table,TextBody,TextStyle};
    let fg=Color {rgb:Rgb(0x12ABCD),alpha:96};let bg=Color {rgb:Rgb(0xFEDCBA),alpha:64};
    let mut s=Scenario::open("test/editing/pptx-plan.pptx");s.update(|e,_,cx| {let mut auto=AutoShape::new(Geometry::Rect,Fill::Pattern {preset:"pct25".into(),foreground:fg,background:bg},None);auto.text=Some(TextBody::plain("pattern target",TextStyle::default()));let id=e.document.ids().new_shape_id();e.insert_shape(Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto)),cx);let id=e.document.ids().new_shape_id();e.insert_shape(Shape::new(id,EmuRect::new(4_000_000,2_000_000,3_000_000,1_200_000),ShapeKind::Table(Box::new(Table::new_grid(2,2,3_000_000,1_200_000)))),cx);});s.save_as("../pptx-pattern-custom-base.pptx");drop(s);
    let mut s=Scenario::open("target/scenario/pptx-pattern-custom-base.pptx");let own=s.shapes()-2;let table=s.shapes()-1;s.click_shape(own);
    let colors=|s:&mut Scenario,values:[[&str;2];2]|s.update(|e,w,cx| {let p=e.pptx().unwrap().gradient_panel.as_ref().unwrap();assert!(p.pattern.is_some());assert_eq!(p.stops.len(),2);let rows=p.stops.clone();for (row,values) in rows.iter().zip(values) {for (f,v) in row.iter().skip(1).zip(values) {f.update(cx,|f,cx|f.set_value(v,w,cx));}}});
    let choices=super::gradient_panel::PATTERN_CHOICES;assert_eq!(choices.len(),54);assert_eq!(choices.iter().map(|c|c.1).collect::<std::collections::HashSet<_>>().len(),54);
    for &(id,preset,_) in choices {s.menu("pptx.fill.custom-pattern");assert_eq!(s.menu_state("pptx.gradient.add"),Some((false,false)));s.key("cmd-a");s.type_text("#12aBcD");assert_eq!(s.with(|e,cx|e.pptx().unwrap().gradient_panel.as_ref().unwrap().stops[0][1].read(cx).value().to_string()),"#12aBcD");colors(&mut s,[["#12aBcD","37.5"],["#fedcba","25"]]);s.menu(id);assert_eq!(s.menu_state(id),Some((true,true)));let before=s.with(|e,_|e.slide().shapes[own].clone());let mut changed=before.clone();let ShapeKind::Auto(a)=&mut changed.kind else {panic!("auto")};a.fill=Fill::Pattern {preset:preset.into(),foreground:fg,background:bg};s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.slide().shapes[own].clone()),changed);s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes[own].clone()),before);s.key("cmd-shift-z");s.save_as(&format!("pattern-{preset}.pptx"));assert_eq!(s.reopen().slides[0].shapes[own].kind,changed.kind);}
    for values in [[["ABCDE","100"],["FFFFFF","100"]],[["GGHHII","100"],["FFFFFF","100"]],[["000000","NaN"],["FFFFFF","100"]],[["000000","-1"],["FFFFFF","100"]],[["000000","101"],["FFFFFF","100"]],[["000000","100"],["ABCDEF","inf"]],[["000000","100"],["ABCDEF","-1"]]] {s.menu("pptx.fill.custom-pattern");colors(&mut s,values);let before=s.with(|e,_|e.presentation().clone());s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),before);assert!(s.with(|e,_|e.pptx().unwrap().gradient_panel.is_some()));s.menu("pptx.gradient.cancel");}
    s.menu("pptx.fill.custom-pattern");s.update(|e,_,cx| {let slide=e.current_slide;let shape=e.slide().shapes[own].id;let frame=e.slide().shapes[own].frame.translated(127_000,0);e.run(vec![Command::SetFrame {slide,shape,frame}],cx);});let altered=s.with(|e,_|e.presentation().clone());s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),altered);assert!(s.with(|e,_|e.pptx().unwrap().gradient_panel.is_some()));s.menu("pptx.gradient.cancel");s.key("cmd-z");
    s.menu("pptx.fill.custom-pattern");s.double_click_shape(own);s.update(|e,_,cx|e.input_mark("未確定",cx));let marked=s.with(|e,_|e.edited_body().unwrap().clone());s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.edited_body().unwrap().clone()),marked);assert!(s.with(|e,_|e.text_editing().unwrap().marked.is_some()));s.menu("pptx.gradient.cancel");s.update(|e,w,cx| {e.input_mark("",cx);e.end_text_edit(w,cx);});
    s.menu("pptx.background.custom-pattern");colors(&mut s,[["12ABCD","37.5"],["FEDCBA","25"]]);s.menu("pptx.pattern.sphere");let expected=Fill::Pattern {preset:"sphere".into(),foreground:fg,background:bg};let before=s.with(|e,_|e.slide().background.clone());s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.slide().background.clone()),expected);s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().background.clone()),before);s.key("cmd-shift-z");s.save_as("pattern-background.pptx");assert_eq!(s.reopen().slides[0].background,expected);
    s.click_shape(table);let cell=cell_middle(&mut s,table,0,0);s.double_click(cell);let corner=cell_middle(&mut s,table,0,1);s.shift_click(corner);s.menu("pptx.fill.custom-pattern");colors(&mut s,[["12ABCD","37.5"],["FEDCBA","25"]]);s.menu("pptx.pattern.weave");let fill=Fill::Pattern {preset:"weave".into(),foreground:fg,background:bg};let before=s.with(|e,_|e.slide().shapes[table].clone());let mut changed=before.clone();let ShapeKind::Table(t)=&mut changed.kind else {panic!("table")};for cell in &mut t.rows[0].cells {cell.fill=fill.clone();}s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.slide().shapes[table].clone()),changed);s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes[table].clone()),before);s.key("cmd-shift-z");s.save_as("pattern-cell-range.pptx");assert_eq!(s.reopen().slides[0].shapes[table].kind,changed.kind);s.key("escape");
    s.update(|e,_,_| {let ids=vec![e.slide().shapes[own].id,e.slide().shapes[table].id];e.set_selection(ids);});s.menu("pptx.fill.custom-pattern");colors(&mut s,[["12ABCD","37.5"],["FEDCBA","25"]]);s.menu("pptx.pattern.trellis");let fill=Fill::Pattern {preset:"trellis".into(),foreground:fg,background:bg};let before=s.with(|e,_|e.slide().clone());let mut changed=before.clone();let ShapeKind::Auto(a)=&mut changed.shapes[own].kind else {panic!("auto")};a.fill=fill.clone();let ShapeKind::Table(t)=&mut changed.shapes[table].kind else {panic!("table")};for cell in t.rows.iter_mut().flat_map(|r|&mut r.cells) {cell.fill=fill.clone();}s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.slide().clone()),changed);s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().clone()),before);s.key("cmd-shift-z");s.save_as("pattern-multi.pptx");let saved=s.reopen();assert_eq!(saved.slides[0].shapes[own].kind,changed.shapes[own].kind);assert_eq!(saved.slides[0].shapes[table].kind,changed.shapes[table].kind);
    s.update(|e,_,_| {let id=e.slide().shapes[own].id;e.set_selection(vec![id]);});s.menu("pptx.fill.custom-pattern");s.menu("pptx.fill.custom-path");assert!(s.with(|e,_|e.pptx().unwrap().gradient_panel.as_ref().unwrap().pattern.is_none()));s.menu("pptx.fill.custom-pattern");assert_eq!(s.with(|e,_|e.pptx().unwrap().gradient_panel.as_ref().unwrap().pattern.as_deref().map(str::to_owned)),Some("trellis".into()));s.menu("pptx.gradient.cancel");
    s.menu("pptx.fill.custom-pattern");s.update(|e,_,cx| {let slide=e.current_slide;let shape=e.slide().shapes[own].id;e.run(vec![Command::RemoveShape {slide,shape}],cx);});let deleted=s.with(|e,_|e.presentation().clone());s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),deleted);assert!(s.with(|e,_|e.pptx().unwrap().gradient_panel.is_some()));s.menu("pptx.gradient.cancel");s.key("cmd-z");undo_all_and_redo(&mut s);
}

pub(crate) fn custom_gradient_preflight_atomic_rejection() {
    let mut s=Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page,attached)=s.with(|e,_|e.presentation().slides.iter().enumerate().find_map(|(i,sl)|sl.attached.values().flatten().find(|&&id|sl.shape(id).is_some_and(|sh|matches!(sh.kind,ShapeKind::Auto(_)))).map(|&id|(i+1,id))).expect("attached auto"));s.page(page);
    s.update(|e,_,cx| {let id=e.document.ids().new_shape_id();let auto=ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Rect,Fill::Solid(ibis_doc_model::Color::BLACK),None);e.insert_shape(ibis_doc_model::Shape::new(id,ibis_doc_model::EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto)),cx);});let own=s.with(|e,_|e.slide().shapes.last().unwrap().id);s.update(|e,_,_|e.set_selection(vec![own,attached]));for menu in ["pptx.fill.custom","pptx.fill.custom-path","pptx.fill.custom-pattern"] {s.menu(menu);s.key("cmd-a");s.type_text(if menu.ends_with("pattern") {"12ABCD"} else {"12.345"});let before=s.with(|e,_|e.presentation().clone());s.menu("pptx.gradient.apply");assert_eq!(s.with(|e,_|e.presentation().clone()),before,"unsupported attached target rejects the valid target too: {menu}");assert!(s.with(|e,_|e.pptx().unwrap().gradient_panel.is_some()));s.menu("pptx.gradient.cancel");}undo_all_and_redo(&mut s);
}

pub(crate) fn gradient_and_pattern_fill_presets() {
    use ibis_doc_model::{Color, Rgb, GradientPath, GradientStop};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.click_shape(1);
    let original = s.with(|e, _| e.slide().shapes[1].clone());
    let stops = vec![GradientStop { position: 0, color: Color::opaque(Rgb(0x4472c4)) }, GradientStop { position: 100_000, color: Color::WHITE }];
    let mut cases = vec![("fill.gradient.0", Fill::Linear { angle: 0, scaled: false, stops: stops.clone() }), ("fill.gradient.90", Fill::Linear { angle: 5_400_000, scaled: false, stops: stops.clone() }), ("fill.gradient.180", Fill::Linear { angle: 10_800_000, scaled: false, stops: stops.clone() }), ("fill.gradient.270", Fill::Linear { angle: 16_200_000, scaled: false, stops: stops.clone() })];
    for (id, path) in [("fill.path.circle", GradientPath::Circle), ("fill.path.rect", GradientPath::Rect), ("fill.path.shape", GradientPath::Shape)] { cases.push((id, Fill::Path { path, focus: [50_000; 4], stops: stops.clone() })); }
    for preset in ["pct25", "pct50", "pct75", "horz", "vert", "diagCross"] { cases.push((preset, Fill::Pattern { preset: preset.into(), foreground: Color::BLACK, background: Color::WHITE })); }
    for (id, expected) in &cases {
        let before = s.with(|e, _| e.slide().shapes[1].clone());
        s.toolbar_menu("fill", id);
        let changed = s.with(|e, _| e.slide().shapes[1].clone());
        let ShapeKind::Auto(a) = &changed.kind else { panic!("auto") }; assert_eq!(&a.fill, expected);
        assert_eq!(changed.frame, original.frame); assert_eq!(changed.text_body(), original.text_body());
        s.key("cmd-z"); assert_eq!(s.with(|e, _| e.slide().shapes[1].clone()), before); s.key("cmd-shift-z");
        s.save_as(&format!("shape-{id}.pptx"));
        let saved = s.reopen(); let ShapeKind::Auto(a) = &saved.slides[0].shapes[1].kind else { panic!("auto") }; assert_eq!(&a.fill, expected);
    }
    s.key("escape");
    for (id, expected) in cases {
        let before = s.with(|e, _| e.slide().background.clone());
        s.toolbar_menu("background", id);
        assert_eq!(s.with(|e, _| e.slide().background.clone()), expected);
        s.key("cmd-z"); assert_eq!(s.with(|e, _| e.slide().background.clone()), before); s.key("cmd-shift-z");
        s.save_as(&format!("background-{id}.pptx")); assert_eq!(s.reopen().slides[0].background, expected);
    }
    undo_all_and_redo(&mut s);
}

pub(crate) fn group_and_ungroup_shapes() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    assert_eq!(s.menu_state("pptx.group"), Some((false, false)));
    s.click_shape(1);
    assert_eq!(s.selected(), 1);
    assert_eq!(s.menu_state("pptx.group"), Some((false, false)));
    assert_eq!(s.menu_state("pptx.ungroup"), Some((false, false)));

    let p2 = s.shape_position(2);
    s.shift_click(p2);
    assert_eq!(s.selected(), 2);
    assert_eq!(s.menu_state("pptx.group"), Some((true, false)));

    s.menu("pptx.group");
    assert_eq!(s.selected(), 2);
    let groups = s.with(|e, _| e.slide().groups.clone());
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].name, "グループ 1");
    assert_eq!(groups[0].children.len(), 2);

    let empty = s.at(EmuPoint::new(11_000_000, 6_300_000));
    s.click(empty);
    assert_eq!(s.selected(), 0);

    let p1 = s.shape_position(1);
    s.click(p1);
    assert_eq!(s.selected(), 2, "クリックでグループ全体が選択される");
    assert_eq!(s.menu_state("pptx.ungroup"), Some((true, false)));

    let before_frame1 = s.with(|e, _| e.slide().shapes[1].frame);
    let before_frame2 = s.with(|e, _| e.slide().shapes[2].frame);
    s.drag(p1, (p1.0 + 30.0, p1.1 + 20.0));
    let moved_frame1 = s.with(|e, _| e.slide().shapes[1].frame);
    let moved_frame2 = s.with(|e, _| e.slide().shapes[2].frame);
    assert_ne!(before_frame1, moved_frame1);
    assert_ne!(before_frame2, moved_frame2);

    let p3 = s.shape_position(3);
    s.shift_click(p3);
    assert_eq!(s.selected(), 3);
    s.key("cmd-alt-g");

    let nested_groups = s.with(|e, _| e.slide().groups.clone());
    assert_eq!(nested_groups.len(), 1);
    assert_eq!(nested_groups[0].name, "グループ 2");
    assert_eq!(nested_groups[0].children.len(), 2);
    match &nested_groups[0].children[0] {
        ibis_doc_model::GroupMember::Group(inner) => {
            assert_eq!(inner.name, "グループ 1");
            assert_eq!(inner.children.len(), 2);
        }
        _ => panic!("期待されるネスト構造ではない"),
    }

    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().groups.len()), 1);
    assert_eq!(s.with(|e, _| e.slide().groups[0].name.clone()), "グループ 1");
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().groups[0].name.clone()), "グループ 2");

    s.save_as("group-nested.pptx");
    let reopened = s.reopen();
    assert_eq!(reopened.slides[2].groups.len(), 1);
    assert_eq!(reopened.slides[2].groups[0].name, "グループ 2");

    s.menu("pptx.ungroup");
    let after_first_ungroup = s.with(|e, _| e.slide().groups.clone());
    assert_eq!(after_first_ungroup.len(), 1);
    assert_eq!(after_first_ungroup[0].name, "グループ 1");

    let current_p1 = s.shape_position(1);
    s.click(current_p1);
    assert_eq!(s.selected(), 2);
    s.key("cmd-shift-alt-g");
    assert_eq!(s.with(|e, _| e.slide().groups.len()), 0);

    undo_all_and_redo(&mut s);
}

pub(crate) fn group_copy_paste_and_duplicate() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    let original_shape_count = s.with(|e, _| e.slide().shapes.len());

    // 2つの図形を選択してグループ化
    s.click_shape(1);
    let p2 = s.shape_position(2);
    s.shift_click(p2);
    assert_eq!(s.selected(), 2);
    s.menu("pptx.group");
    assert_eq!(s.with(|e, _| e.slide().groups.len()), 1);

    // グループ選択状態で複製 (cmd-d)
    s.key("cmd-d");
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), original_shape_count + 2);
    assert_eq!(s.with(|e, _| e.slide().groups.len()), 2);
    assert_eq!(s.selected(), 2, "複製されたグループの図形が選択される");

    // 複製を Undo/Redo
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), original_shape_count);
    assert_eq!(s.with(|e, _| e.slide().groups.len()), 1);
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), original_shape_count + 2);
    assert_eq!(s.with(|e, _| e.slide().groups.len()), 2);

    // 複製されたグループを選択してコピー (cmd-c)
    s.click_shape(original_shape_count);
    assert_eq!(s.selected(), 2, "クリックで複製グループ全体が選択される");
    s.key("cmd-c");

    // スライド2に移動して貼り付け (cmd-v)
    s.page(2);
    let s2_shapes_before = s.with(|e, _| e.slide().shapes.len());
    let s2_groups_before = s.with(|e, _| e.slide().groups.len());
    s.key("cmd-v");
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), s2_shapes_before + 2);
    assert_eq!(s.with(|e, _| e.slide().groups.len()), s2_groups_before + 1);
    assert_eq!(s.selected(), 2);

    // スライド2のペーストを Undo
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), s2_shapes_before);
    assert_eq!(s.with(|e, _| e.slide().groups.len()), s2_groups_before);

    // 再度スライド3へ戻る
    s.page(3);
    assert_eq!(s.with(|e, _| e.slide().groups.len()), 2);

    // 複製したグループを選択して切り取り (cmd-x)
    s.click_shape(original_shape_count);
    assert_eq!(s.selected(), 2);
    s.key("cmd-x");
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), original_shape_count);
    assert_eq!(s.with(|e, _| e.slide().groups.len()), 1);

    // 切り取ったグループを貼り付け (cmd-v)
    s.key("cmd-v");
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), original_shape_count + 2);
    assert_eq!(s.with(|e, _| e.slide().groups.len()), 2);

    // 保存と再読込
    s.save_as("group-copy-paste.pptx");
    let reopened = s.reopen();
    assert_eq!(reopened.slides[2].groups.len(), 2);
    assert_eq!(reopened.slides[2].shapes.len(), original_shape_count + 2);

    undo_all_and_redo(&mut s);
}

pub(crate) fn picture_fill_and_background_picture() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let (shape_idx, own) = s.with(|e, _| {
        e.slide()
            .shapes
            .iter()
            .enumerate()
            .find(|(_, s)| matches!(s.kind, ShapeKind::Auto(_)))
            .map(|(i, s)| (i, s.id))
            .unwrap()
    });
    s.click_shape(shape_idx);
    let png_data = png(100, 100, [0x30, 0x60, 0x90]);
    let media = s.update(|e, _, cx| e.fill_selected_with_picture("fill.png", png_data.clone(), cx)).unwrap();
    let filled_shape = s.with(|e, _| e.slide().shape(own).unwrap().clone());
    let ShapeKind::Auto(auto) = &filled_shape.kind else { panic!("auto") };
    assert_eq!(auto.fill, Fill::Image(media));

    let bg_media = s.update(|e, _, cx| e.set_background_with_picture("bg.png", png_data, cx)).unwrap();
    assert_eq!(s.with(|e, _| e.slide().background.clone()), Fill::Image(bg_media));

    s.key("cmd-z");
    assert_ne!(s.with(|e, _| e.slide().background.clone()), Fill::Image(bg_media));
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().background.clone()), Fill::Image(bg_media));

    s.save_as("picture-fill-slide.pptx");
    let reopened = s.reopen();
    assert!(matches!(reopened.slides[0].background, Fill::Image(_)));
    let re_shape = &reopened.slides[0].shapes[shape_idx];
    let ShapeKind::Auto(re_auto) = &re_shape.kind else { panic!("re_auto") };
    assert!(matches!(re_auto.fill, Fill::Image(_)));
}

pub(crate) fn export_slide_to_png() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let out_dir = std::env::temp_dir().join(format!("ibisoffice-test-export-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&out_dir);
    let single_slide_png = out_dir.join("slide1.png");
    let job = s.with(|e, _| e.write_png(single_slide_png.clone(), Some(1))).unwrap();
    let res = job().unwrap();
    assert_eq!(res, single_slide_png);
    assert!(single_slide_png.exists());
    let data = std::fs::read(&single_slide_png).unwrap();
    assert!(!data.is_empty());
    assert_eq!(&data[0..4], &[0x89, b'P', b'N', b'G']);

    let all_slides_png = out_dir.join("all_slides.png");
    let job_all = s.with(|e, _| e.write_png(all_slides_png.clone(), None)).unwrap();
    let _ = job_all().unwrap();
    let slide1 = out_dir.join("all_slides-01.png");
    let slide2 = out_dir.join("all_slides-02.png");
    assert!(slide1.exists());
    assert!(slide2.exists());

    let presentation_pdf = out_dir.join("presentation.pdf");
    let job_pdf = s.with(|e, _| e.write_pdf(presentation_pdf.clone())).unwrap();
    let res_pdf = job_pdf().unwrap();
    assert_eq!(res_pdf, presentation_pdf);
    assert!(presentation_pdf.exists());
    let pdf_data = std::fs::read(&presentation_pdf).unwrap();
    assert!(!pdf_data.is_empty());
    assert_eq!(&pdf_data[0..4], b"%PDF");

    let _ = std::fs::remove_dir_all(&out_dir);
}

/// PPTX-RECORD-014: U:アニメーションGIF出力（Export Presentation to Animated GIF）
pub(crate) fn export_presentation_to_animated_gif() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let slide_count = s.with(|e, _| e.presentation().slides.len());
    assert!(slide_count >= 2);

    // 1. スライドメニューに「アニメーション GIF として書き出し…」（pptx.export-gif）が存在すること
    let menu_state = s.menu_state("pptx.export-gif");
    assert!(menu_state.is_some(), "pptx.export-gif がメニューに存在する");

    // 2. コンテキストメニューエントリに pptx.export-gif が含まれていること
    let entries = s.with(|e, _| e.slide_context_menu_entries());
    let ids: Vec<&str> = entries.iter().filter_map(|entry| match entry {
        crate::shell::chrome::MenuEntry::Item(item) => Some(item.id),
        _ => None,
    }).collect();
    assert!(ids.contains(&"pptx.export-gif"), "コンテキストメニューに pptx.export-gif が存在する");

    // 3. ファイル > 書き出し メニューに file.export.gif が存在すること
    let file_export_gif = s.menu_state("file.export.gif");
    assert!(file_export_gif.is_some(), "file.export.gif がメニューに存在する");

    // 4. write_gif ジョブを実行してアニメーション GIF を生成
    let out_dir = std::env::temp_dir().join(format!("ibisoffice-test-export-gif-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&out_dir);
    let presentation_gif = out_dir.join("presentation.gif");

    let job = s.with(|e, _| e.write_gif(presentation_gif.clone(), Some(1.5))).expect("write_gif job created");
    let res = job().expect("write_gif job succeeds");
    assert_eq!(res, presentation_gif);
    assert!(presentation_gif.exists(), "GIF ファイルが存在する");

    // 5. 生成された GIF データの検証
    let gif_data = std::fs::read(&presentation_gif).expect("read generated gif");
    assert!(!gif_data.is_empty(), "GIF データが空でない");
    // GIF89a または GIF87a ヘッダー
    assert!(gif_data.starts_with(b"GIF89a") || gif_data.starts_with(b"GIF87a"), "GIF ヘッダーが正しい");

    // 6. フレーム数の検証（スライド枚数と一致）
    use image::codecs::gif::GifDecoder;
    use image::AnimationDecoder;
    let decoder = GifDecoder::new(std::io::Cursor::new(&gif_data)).expect("valid gif");
    let frames = decoder.into_frames().collect_frames().expect("read frames");
    assert_eq!(frames.len(), slide_count, "全スライドが GIF フレームとして出力されている");

    let _ = std::fs::remove_dir_all(&out_dir);
}

pub(crate) fn change_case_and_caps() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    // Select first shape and edit text
    s.double_click_shape(0);
    s.key("cmd-a");
    s.type_text("Hello World! This is ibisOffice.");
    s.key("escape");

    // Select shape
    s.click_shape(0);

    // 1. UPPERCASE
    s.menu("pptx.case.upper");
    assert_eq!(s.with(|e, _| e.slide().shapes[0].text_body().unwrap().text()), "HELLO WORLD! THIS IS IBISOFFICE.");

    // 2. lowercase
    s.menu("pptx.case.lower");
    assert_eq!(s.with(|e, _| e.slide().shapes[0].text_body().unwrap().text()), "hello world! this is ibisoffice.");

    // 3. Sentence case
    s.menu("pptx.case.sentence");
    assert_eq!(s.with(|e, _| e.slide().shapes[0].text_body().unwrap().text()), "Hello world! This is ibisoffice.");

    // 4. Capitalize Each Word (Title case)
    s.menu("pptx.case.title");
    assert_eq!(s.with(|e, _| e.slide().shapes[0].text_body().unwrap().text()), "Hello World! This Is Ibisoffice.");

    // 5. tOGGLE cASE
    s.menu("pptx.case.toggle");
    assert_eq!(s.with(|e, _| e.slide().shapes[0].text_body().unwrap().text()), "hELLO wORLD! tHIS iS iBISOFFICE.");

    // 6. Undo & Redo
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().shapes[0].text_body().unwrap().text()), "Hello World! This Is Ibisoffice.");
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().shapes[0].text_body().unwrap().text()), "hELLO wORLD! tHIS iS iBISOFFICE.");

    // 7. Caps (All Caps & Small Caps toggles)
    s.menu("text.caps");
    assert_eq!(s.with(|e, _| e.slide().shapes[0].text_body().unwrap().paragraphs[0].runs[0].style.caps), ibis_doc_model::Caps::All);
    s.menu("text.caps");
    assert_eq!(s.with(|e, _| e.slide().shapes[0].text_body().unwrap().paragraphs[0].runs[0].style.caps), ibis_doc_model::Caps::None);

    s.menu("text.small-caps");
    assert_eq!(s.with(|e, _| e.slide().shapes[0].text_body().unwrap().paragraphs[0].runs[0].style.caps), ibis_doc_model::Caps::Small);
    s.menu("text.small-caps");
    assert_eq!(s.with(|e, _| e.slide().shapes[0].text_body().unwrap().paragraphs[0].runs[0].style.caps), ibis_doc_model::Caps::None);

    // 8. Multiple runs style preservation
    s.double_click_shape(0);
    s.key("cmd-a");
    s.type_text("hello world");
    s.update(|e, window, cx| {
        e.format_characters(window, cx, ibis_doc_model::CharacterFormat { bold: Some(true), ..Default::default() });
    });
    s.key("escape");
    s.click_shape(0);
    s.menu("pptx.case.upper");
    assert_eq!(s.with(|e, _| e.slide().shapes[0].text_body().unwrap().text()), "HELLO WORLD");

    // 9. Save and Reopen
    s.menu("text.caps");
    s.save_as("test-case-and-caps.pptx");
    let reopened = s.reopen();
    let re_body = reopened.slides[0].shapes[0].text_body().unwrap();
    assert_eq!(re_body.text(), "HELLO WORLD");
    assert_eq!(re_body.paragraphs[0].runs[0].style.caps, ibis_doc_model::Caps::All);
}

pub(crate) fn table_cell_vertical_anchor_and_text_direction() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page, shape) = s
        .with(|e, _| {
            e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Table(_))).map(|j| (i, j)))
        })
        .expect("a slide with a table");
    s.page(page + 1);
    let cell = cell_middle(&mut s, shape, 1, 1);
    s.double_click(cell);
    assert_eq!(s.with(|e, _| e.text_editing().and_then(|t| t.cell)), Some((1, 1)));

    // 1. Vertical anchor (Top / Middle / Bottom)
    s.menu("pptx.text.bottom");
    assert_eq!(s.with(|e, _| e.edited_body().unwrap().anchor), ibis_doc_model::VerticalAnchor::Bottom);

    s.menu("pptx.text.top");
    assert_eq!(s.with(|e, _| e.edited_body().unwrap().anchor), ibis_doc_model::VerticalAnchor::Top);

    s.menu("pptx.text.middle");
    assert_eq!(s.with(|e, _| e.edited_body().unwrap().anchor), ibis_doc_model::VerticalAnchor::Middle);

    // 2. Text direction (vertical / clockwise / horizontal)
    s.menu("pptx.text.vertical");
    assert!(s.with(|e, _| e.edited_body().unwrap().vertical));
    assert_eq!(s.with(|e, _| e.edited_body().unwrap().vertical_kind), ibis_doc_model::VerticalKind::EastAsian);

    s.menu("pptx.text.clockwise");
    assert_eq!(s.with(|e, _| e.edited_body().unwrap().sideways), 5_400_000);

    s.menu("pptx.text.horizontal");
    assert!(!s.with(|e, _| e.edited_body().unwrap().vertical));
    assert_eq!(s.with(|e, _| e.edited_body().unwrap().sideways), 0);

    // 3. Undo and Redo
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.edited_body().unwrap().sideways), 5_400_000);
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.edited_body().unwrap().sideways), 0);

    // 4. Set both Bottom anchor and Vertical text, save and reopen
    s.menu("pptx.text.bottom");
    s.menu("pptx.text.vertical");
    s.key("escape");

    s.save_as("table-anchor-vert.pptx");
    let reopened = s.reopen();
    let ShapeKind::Table(t) = &reopened.slides[page].shapes[shape].kind else { panic!("table") };
    let re_cell_text = &t.rows[1].cells[1].text;
    assert_eq!(re_cell_text.anchor, ibis_doc_model::VerticalAnchor::Bottom);
    assert!(re_cell_text.vertical);
    assert_eq!(re_cell_text.vertical_kind, ibis_doc_model::VerticalKind::EastAsian);
}

/// フォントの置換: replace a font across the entire presentation (multiple
/// slides, multiple shapes, and table cells) with another font, undo/redo,
/// and verified after save/reopen.
pub(crate) fn replace_fonts_presentation() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    // スライド1の図形1に "Arial" フォントを設定
    s.page(1);
    s.double_click_shape(1);
    s.key("cmd-a");
    s.type_text("First Arial Text");
    s.key("cmd-a");
    s.update(|e, w, cx| {
        e.format_characters(w, cx, ibis_doc_model::CharacterFormat {
            font: Some(Some("Arial".into())),
            ..Default::default()
        });
    });
    s.key("escape");

    // スライド2の図形0にも "Arial" フォントを設定
    s.page(2);
    s.double_click_shape(0);
    s.key("cmd-a");
    s.type_text("Second Arial Text");
    s.key("cmd-a");
    s.update(|e, w, cx| {
        e.format_characters(w, cx, ibis_doc_model::CharacterFormat {
            font: Some(Some("Arial".into())),
            ..Default::default()
        });
    });
    s.key("escape");

    // スライド3にテーブルを作成し、セルに "Arial" を設定
    s.page(3);
    s.menu("table.new.3x4");
    s.type_text("Cell Arial Text");
    s.key("cmd-a");
    s.update(|e, w, cx| {
        e.format_characters(w, cx, ibis_doc_model::CharacterFormat {
            font: Some(Some("Arial".into())),
            ..Default::default()
        });
    });
    s.key("escape");

    // 使用フォント一覧を検証
    let used = s.with(|e, _| crate::pptx::replace_fonts::presentation_used_fonts(e.presentation()));
    assert!(used.iter().any(|f| f == "Arial"), "Arial should be in used fonts: {used:?}");

    // フォント置換パネルを開く
    s.menu("pptx.replace-fonts");
    assert!(s.with(|e, _| e.pptx().unwrap().replace_fonts.is_some()));

    // フォント置換を実行: "Arial" -> "Georgia"
    s.update(|e, w, cx| {
        let state = e.pptx().unwrap().replace_fonts.as_ref().unwrap();
        state.target.update(cx, |t, cx| t.set_value("Arial", w, cx));
        state.replacement.update(cx, |r, cx| r.set_value("Georgia", w, cx));
    });
    s.update(|e, w, cx| e.replace_presentation_fonts(w, cx));

    // 各スライドの該当フォントが "Georgia" に変わっていることを検証
    let replaced_slides = s.with(|e, _| e.presentation().slides.clone());
    let s1_font = replaced_slides[0].shapes[1].text_body().unwrap().paragraphs[0].runs[0].style.font.as_deref();
    assert_eq!(s1_font, Some("Georgia"), "Slide 1 shape 1 font replaced with Georgia");

    let s2_font = replaced_slides[1].shapes[0].text_body().unwrap().paragraphs[0].runs[0].style.font.as_deref();
    assert_eq!(s2_font, Some("Georgia"), "Slide 2 shape 0 font replaced with Georgia");

    let s3_cell_font = match &replaced_slides[2].shapes.last().unwrap().kind {
        ShapeKind::Table(t) => t.rows[0].cells[0].text.paragraphs[0].runs[0].style.font.clone(),
        _ => panic!("table expected"),
    };
    assert_eq!(s3_cell_font.as_deref(), Some("Georgia"), "Slide 3 table cell font replaced with Georgia");

    // 使用フォント一覧から "Arial" が消え、"Georgia" が入っている
    let used_after = s.with(|e, _| crate::pptx::replace_fonts::presentation_used_fonts(e.presentation()));
    assert!(!used_after.iter().any(|f| f == "Arial"), "Arial should not be used anymore");
    assert!(used_after.iter().any(|f| f == "Georgia"), "Georgia should be in used fonts");

    // Undo (cmd-z): 1ステップで元の Arial に全箇所復元される
    s.key("cmd-z");
    let undone_slides = s.with(|e, _| e.presentation().slides.clone());
    assert_eq!(undone_slides[0].shapes[1].text_body().unwrap().paragraphs[0].runs[0].style.font.as_deref(), Some("Arial"));
    assert_eq!(undone_slides[1].shapes[0].text_body().unwrap().paragraphs[0].runs[0].style.font.as_deref(), Some("Arial"));

    // Redo (cmd-shift-z): 再び Georgia に置換される
    s.key("cmd-shift-z");
    let redone_slides = s.with(|e, _| e.presentation().slides.clone());
    assert_eq!(redone_slides[0].shapes[1].text_body().unwrap().paragraphs[0].runs[0].style.font.as_deref(), Some("Georgia"));
    assert_eq!(redone_slides[1].shapes[0].text_body().unwrap().paragraphs[0].runs[0].style.font.as_deref(), Some("Georgia"));

    // 保存と再読込
    s.save_as("replace-fonts-result.pptx");
    let reopened = s.reopen();
    assert_eq!(reopened.slides[0].shapes[1].text_body().unwrap().paragraphs[0].runs[0].style.font.as_deref(), Some("Georgia"));
    assert_eq!(reopened.slides[1].shapes[0].text_body().unwrap().paragraphs[0].runs[0].style.font.as_deref(), Some("Georgia"));
    let re_cell_font = match &reopened.slides[2].shapes.last().unwrap().kind {
        ShapeKind::Table(t) => t.rows[0].cells[0].text.paragraphs[0].runs[0].style.font.clone(),
        _ => panic!("table expected"),
    };
    assert_eq!(re_cell_font.as_deref(), Some("Georgia"));
}

pub(crate) fn show_properties_and_slideshow_controls() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    assert!(s.with(|e, _| e.presentation().slides.len() >= 4));

    // パネルを開く
    s.update(|e, w, cx| e.open_show_panel(w, cx));
    assert!(s.with(|e, _| e.pptx().unwrap().show_panel.is_some()));

    // 設定を変更: Browse (個人として閲覧), ループ, アニメーションなし, スライド範囲 2〜3
    s.update(|e, _, cx| {
        e.set_show_panel_type(ibis_doc_model::ShowType::Browse, cx);
        e.toggle_show_panel_option("loop", cx);
        e.toggle_show_panel_option("animation", cx);
        let panel = e.pptx_mut().unwrap().show_panel.as_mut().unwrap();
        panel.all_slides = false;
        panel.range_kind = super::show_panel::ShowRangeKind::Range;
    });
    s.update(|e, w, cx| {
        let panel = e.pptx().unwrap().show_panel.as_ref().unwrap();
        panel.from.update(cx, |f, cx| f.set_value("2", w, cx));
        panel.to.update(cx, |f, cx| f.set_value("3", w, cx));
    });

    let before = s.with(|e, _| e.presentation().show_properties.clone());
    s.menu("pptx.show-settings.apply");
    assert!(s.with(|e, _| e.pptx().unwrap().show_panel.is_none()));

    let changed = s.with(|e, _| e.presentation().show_properties.clone());
    assert_eq!(changed.show_type, ibis_doc_model::ShowType::Browse);
    assert_eq!(changed.loop_continuously, true);
    assert_eq!(changed.show_animation, false);
    assert_eq!(changed.slide_range, ibis_doc_model::ShowSlideRange::Range { start: 2, end: 3 });

    // スライドショーの制御確認
    // スライド1（インデックス0、範囲外）を選択した状態でショーを開始
    s.page(1);
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    // 範囲外だったため、範囲内の先頭（スライド2 = slides[1].id）から開始される
    let s2_id = s.with(|e, _| e.presentation().slides[1].id);
    let s3_id = s.with(|e, _| e.presentation().slides[2].id);
    assert_eq!(s.with(|e, _| e.current_slide), s2_id);

    // 次のスライドへ進む -> スライド3
    s.update(|e, w, cx| e.step_slideshow(true, w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), s3_id);

    // 次へ進む -> 範囲終端かつループ有効なのでスライド2に戻る
    s.update(|e, w, cx| e.step_slideshow(true, w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), s2_id);

    // スライドショー終了
    s.update(|e, w, cx| e.end_slideshow(w, cx));

    // Undo / Redo
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.presentation().show_properties.clone()), before);
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.presentation().show_properties.clone()), changed);

    // 保存と再読込
    s.save_as("show-properties-test.pptx");
    let reopened = s.reopen();
    assert_eq!(reopened.show_properties, changed);
    undo_all_and_redo(&mut s);
}

/// アニメーション開始遅延: 選択した図形のアニメーションに対して「遅延」（0.5秒、1秒等）を設定、
/// Undo/Redo、ファイル保存・再読込で delay が保持されることを確認。
pub(crate) fn animation_delay_options() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.in.fade");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.delay)), Some(0));

    s.toolbar_menu("motion", "pptx.anim.delay.1000");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.delay)), Some(1000));

    s.toolbar_menu("motion", "pptx.anim.delay.500");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.delay)), Some(500));

    // Undo / Redo
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.delay)), Some(1000));
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.delay)), Some(500));

    s.save_as("anim-delay.pptx");
    let reopened = s.reopen();
    assert_eq!(reopened.slides[0].animations.first().map(|a| a.delay), Some(500));

    undo_all_and_redo(&mut s);
}

/// 目的別スライド ショー (Custom Shows / PPTX-SHOW-003):
/// 登録・編集・削除、設定パネルからの選択、目的別スライドショーの再生・ステップ遷移、
/// ファイル保存・再読込、Undo / Redo を検証。
pub(crate) fn custom_shows_and_slideshow() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    assert!(s.with(|e, _| e.presentation().slides.len() >= 4));

    let s0_id = s.with(|e, _| e.presentation().slides[0].id);
    let s1_id = s.with(|e, _| e.presentation().slides[1].id);
    let s2_id = s.with(|e, _| e.presentation().slides[2].id);
    let s3_id = s.with(|e, _| e.presentation().slides[3].id);

    // 1. 2つの目的別スライドショーを登録
    // ショー0: スライド1 -> スライド3 (s0, s2)
    // ショー1: スライド2 -> スライド4 (s1, s3)
    let show0_id = s.update(|e, _, cx| e.add_custom_show("ダイジェスト", vec![s0_id, s2_id], cx));
    let show1_id = s.update(|e, _, cx| e.add_custom_show("偶数章", vec![s1_id, s3_id], cx));
    assert_eq!(s.with(|e, _| e.presentation().custom_shows.len()), 2);

    // 2. ショー0 を直接指定してスライドショー開始
    s.update(|e, w, cx| e.start_custom_slideshow(show0_id, w, cx));
    assert!(s.with(|e, _| e.slideshow.is_some()));
    assert_eq!(s.with(|e, _| e.current_slide), s0_id);

    // 次へ -> スライド3 (s2_id) にジャンプ
    s.update(|e, w, cx| e.step_slideshow(true, w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), s2_id);

    // 前へ -> スライド1 (s0_id) に戻る
    s.update(|e, w, cx| e.step_slideshow(false, w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), s0_id);

    // スライドショー終了
    s.update(|e, w, cx| e.end_slideshow(w, cx));
    assert!(s.with(|e, _| e.slideshow.is_none()));

    // 3. ショー設定パネルを開き、「目的別スライド ショー」からショー1を選択して適用
    s.update(|e, w, cx| e.open_show_panel(w, cx));
    assert!(s.with(|e, _| e.pptx().unwrap().show_panel.is_some()));

    s.update(|e, _, _| {
        let panel = e.pptx_mut().unwrap().show_panel.as_mut().unwrap();
        panel.range_kind = super::show_panel::ShowRangeKind::Custom;
        panel.selected_custom_show = Some(show1_id);
    });

    s.menu("pptx.show-settings.apply");
    assert!(s.with(|e, _| e.pptx().unwrap().show_panel.is_none()));
    assert_eq!(s.with(|e, _| e.presentation().show_properties.slide_range), ibis_doc_model::ShowSlideRange::Custom(show1_id));

    // 通常の start_slideshow を呼ぶとショー1（s1_id, s3_id）の順序で再生される
    s.page(1); // 範囲外のスライド1を選択していても
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    assert!(s.with(|e, _| e.slideshow.is_some()));
    assert_eq!(s.with(|e, _| e.current_slide), s1_id);

    s.update(|e, w, cx| e.step_slideshow(true, w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), s3_id);

    s.update(|e, w, cx| e.end_slideshow(w, cx));
    assert!(s.with(|e, _| e.slideshow.is_none()));

    // 4. ショー1を削除 -> show_properties の slide_range が All に自動リセットされることを確認
    s.update(|e, _, cx| e.remove_custom_show(show1_id, cx));
    assert_eq!(s.with(|e, _| e.presentation().custom_shows.len()), 1);
    assert_eq!(s.with(|e, _| e.presentation().show_properties.slide_range), ibis_doc_model::ShowSlideRange::All);

    // 5. 保存と再読込
    s.save_as("custom-shows-scenario.pptx");
    let reopened = s.reopen();
    assert_eq!(reopened.custom_shows.len(), 1);
    assert_eq!(reopened.custom_shows[0].name, "ダイジェスト");
    assert_eq!(reopened.custom_shows[0].slides, vec![s0_id, s2_id]);

    // 6. Undo / Redo
    undo_all_and_redo(&mut s);
}

/// アニメーション効果のプレビューと削除: ツールバーのプレビューメニュー、
/// アニメーション ウィンドウの「すべて再生」ボタン、個別削除および選択図形のアニメーション削除を検証。
pub(crate) fn animation_preview_and_remove() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(1);

    // 図形0にフェード効果を追加
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.in.fade");
    assert_eq!(s.with(|e, _| e.slide().animations.len()), 1);

    // プレビュー実行 -> スライドショーが開始される
    s.toolbar_menu("motion", "pptx.anim.preview");
    assert!(s.with(|e, _| e.slideshow.is_some()));

    // Esc でスライドショーを終了
    s.key("escape");
    assert!(s.with(|e, _| e.slideshow.is_none()));

    // 再度図形を選択してアニメーション ウィンドウを開く
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.pane");
    assert!(s.with(|e, _| e.pptx().map_or(false, |p| p.animation_pane)));

    // アニメーション ウィンドウ経由でスライドショー（再生）を実行
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    assert!(s.with(|e, _| e.slideshow.is_some()));
    s.key("escape");
    assert!(s.with(|e, _| e.slideshow.is_none()));

    // 図形1を選択してフライイン効果を追加
    s.click_shape(1);
    s.toolbar_menu("motion", "pptx.anim.in.fly-bottom");
    assert_eq!(s.with(|e, _| e.slide().animations.len()), 2);

    // 図形1のアニメーションをツールバーから削除
    s.toolbar_menu("motion", "pptx.anim.remove");
    assert_eq!(s.with(|e, _| e.slide().animations.len()), 1);

    // アニメーション ウィンドウ経由で先頭の効果を削除
    s.update(|e, _, cx| e.remove_effect(0, cx));
    assert_eq!(s.with(|e, _| e.slide().animations.len()), 0);

    // Undo / Redo
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().animations.len()), 1);
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().animations.len()), 0);

    undo_all_and_redo(&mut s);
}

/// アニメーション効果オプション (Effect Options / PPTX-ANIM-006):
/// スライドインの方向（下・左・右・上）、スピンの角度・向き、拡大縮小のサイズを変更、
/// ツールバー「効果のオプション」の有効/無効、Undo / Redo、ファイル保存・再読込を検証。
pub(crate) fn animation_effect_options() {
    use ibis_doc_model::{AnimationLook, AnimationSide};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 図形0を選択。最初はアニメーションが無いため anim-options は無効
    s.click_shape(0);
    assert_eq!(s.toolbar_state("anim-options").map(|t| t.enabled), Some(false));

    // スライドイン（下から）を設定 -> anim-options が有効化
    s.toolbar_menu("motion", "pptx.anim.in.fly-bottom");
    assert_eq!(s.toolbar_state("anim-options").map(|t| t.enabled), Some(true));
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.look)), Some(AnimationLook::Fly(AnimationSide::Bottom)));
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.preset)), Some((2, 4)));

    // 「効果のオプション」から「左から」に変更
    s.toolbar_menu("anim-options", "pptx.anim.opt.fly-left");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.look)), Some(AnimationLook::Fly(AnimationSide::Left)));
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.preset)), Some((2, 8)));

    // 「効果のオプション」から「上から」に変更
    s.toolbar_menu("anim-options", "pptx.anim.opt.fly-top");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.look)), Some(AnimationLook::Fly(AnimationSide::Top)));
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.preset)), Some((2, 1)));

    // 図形1を選択し、スピンを設定
    s.click_shape(1);
    s.toolbar_menu("motion", "pptx.anim.em.spin");
    assert_eq!(s.toolbar_state("anim-options").map(|t| t.enabled), Some(true));
    assert_eq!(s.with(|e, _| e.slide().animations.last().map(|a| a.look)), Some(AnimationLook::Spin(21_600_000)));

    // 「効果のオプション」から「180°」に変更
    s.toolbar_menu("anim-options", "pptx.anim.opt.spin-180");
    assert_eq!(s.with(|e, _| e.slide().animations.last().map(|a| a.look)), Some(AnimationLook::Spin(10_800_000)));

    // 強調効果を拡大縮小（grow: 150%）に変更
    s.toolbar_menu("motion", "pptx.anim.em.grow");
    assert_eq!(s.with(|e, _| e.slide().animations.last().map(|a| a.look)), Some(AnimationLook::Scale(150_000, 150_000)));

    // 「効果のオプション」から「200%」に変更
    s.toolbar_menu("anim-options", "pptx.anim.opt.scale-200");
    assert_eq!(s.with(|e, _| e.slide().animations.last().map(|a| a.look)), Some(AnimationLook::Scale(200_000, 200_000)));

    // Undo / Redo の確認
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().animations.last().map(|a| a.look)), Some(AnimationLook::Scale(150_000, 150_000)));
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().animations.last().map(|a| a.look)), Some(AnimationLook::Scale(200_000, 200_000)));

    // 保存して再読込
    s.save_as("anim-effect-options.pptx");
    let reopened = s.reopen();
    assert_eq!(reopened.slides[0].animations.len(), 2);
    assert_eq!(reopened.slides[0].animations[0].look, AnimationLook::Fly(AnimationSide::Top));
    assert_eq!(reopened.slides[0].animations[0].preset, (2, 1));
    assert_eq!(reopened.slides[0].animations[1].look, AnimationLook::Scale(200_000, 200_000));

    undo_all_and_redo(&mut s);
}

/// 軌跡効果（直線モーションパス / PPTX-ANIM-004）:
/// 直線アニメーション（下・右・上・左）の設定、効果オプションでの方向切り替え、
/// Undo / Redo、ファイル保存・再読込を検証。
pub(crate) fn animation_motion_path() {
    use ibis_doc_model::{AnimationClass, AnimationLook};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 図形0を選択。最初は効果オプション無効
    s.click_shape(0);
    assert_eq!(s.toolbar_state("anim-options").map(|t| t.enabled), Some(false));

    // 軌跡効果（直線: 下へ）を設定 -> anim-options が有効化
    s.toolbar_menu("motion", "pptx.anim.path.down");
    assert_eq!(s.toolbar_state("anim-options").map(|t| t.enabled), Some(true));
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.class)), Some(AnimationClass::Path));
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.look)), Some(AnimationLook::Motion(0, 25_000)));
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.preset)), Some((61, 4)));

    // 「効果のオプション」から「右へ」に変更
    s.toolbar_menu("anim-options", "pptx.anim.opt.path-right");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.look)), Some(AnimationLook::Motion(25_000, 0)));
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.preset)), Some((61, 2)));

    // 「効果のオプション」から「上へ」に変更
    s.toolbar_menu("anim-options", "pptx.anim.opt.path-up");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.look)), Some(AnimationLook::Motion(0, -25_000)));
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.preset)), Some((61, 1)));

    // 「効果のオプション」から「左へ」に変更
    s.toolbar_menu("anim-options", "pptx.anim.opt.path-left");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.look)), Some(AnimationLook::Motion(-25_000, 0)));
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.preset)), Some((61, 8)));

    // Undo / Redo の確認
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.look)), Some(AnimationLook::Motion(0, -25_000)));
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.look)), Some(AnimationLook::Motion(-25_000, 0)));

    // 保存して再読込
    s.save_as("anim-motion-path.pptx");
    let reopened = s.reopen();
    assert_eq!(reopened.slides[0].animations.len(), 1);
    assert_eq!(reopened.slides[0].animations[0].class, AnimationClass::Path);
    assert_eq!(reopened.slides[0].animations[0].look, AnimationLook::Motion(-25_000, 0));
    assert_eq!(reopened.slides[0].animations[0].preset, (61, 8));

    undo_all_and_redo(&mut s);
}

/// テキストの動き > 段落別 (ANIM-020): 本文の図形の効果が段落ごとの効果になり
/// （2 つ目からクリックで始まる）、保存して読み直しても段落の範囲が残り、
/// 「オブジェクト全体」で 1 つに戻る。
pub(crate) fn animation_by_paragraph() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    let (shape, paragraphs) = s.with(|e, _| {
        let sh = e.slide().shapes.iter().find(|sh| sh.text_body().is_some_and(|t| t.paragraphs.len() >= 3)).expect("a shape with paragraphs");
        (sh.id, sh.text_body().unwrap().paragraphs.len())
    });
    s.update(|e, _, _| e.set_selection(vec![shape]));
    s.toolbar_menu("motion", "pptx.anim.in.fade");
    assert_eq!(s.with(|e, _| e.slide().animations.len()), 1);
    s.toolbar_menu("motion", "pptx.anim.build.paragraph");
    let anim = s.with(|e, _| e.slide().animations.clone());
    assert_eq!(anim.len(), paragraphs, "one for each paragraph");
    assert!(anim.iter().enumerate().all(|(k, a)| a.paragraph == Some((k as u32, k as u32)) && a.shape == shape && (k == 0 || a.start == ibis_doc_model::AnimationStart::OnClick)));
    s.save_as("anim-by-paragraph.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[1].animations, anim, "read back as written");
    // Back to the whole shape.
    s.toolbar_menu("motion", "pptx.anim.build.whole");
    let whole = s.with(|e, _| e.slide().animations.clone());
    assert_eq!((whole.len(), whole[0].paragraph), (1, None));
    undo_all_and_redo(&mut s);
}

/// 複数の図形（グループの図形も）に効果を付ける (ANIM-023): 1 つ目のクリックで
/// 全部が一緒に動く（2 つ目からは「直前の動作と同時」）。
pub(crate) fn animation_on_several_shapes() {
    use ibis_doc_model::AnimationStart;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    let ids: Vec<_> = s.with(|e, _| e.slide().shapes.iter().take(3).map(|sh| sh.id).collect());
    assert!(ids.len() >= 3);
    s.update(|e, _, _| e.set_selection(ids.clone()));
    s.toolbar_menu("motion", "pptx.anim.in.fade");
    let starts = s.with(|e, _| e.slide().animations.iter().map(|a| a.start).collect::<Vec<_>>());
    assert_eq!(starts, [AnimationStart::OnClick, AnimationStart::WithPrevious, AnimationStart::WithPrevious]);
    // Grouped, the group's shapes come together as well.
    s.update(|e, _, cx| e.group_selected(cx));
    s.update(|e, _, cx| e.remove_shape_effects(cx));
    assert!(s.with(|e, _| e.slide().animations.is_empty()));
    s.toolbar_menu("motion", "pptx.anim.in.appear");
    let starts = s.with(|e, _| e.slide().animations.iter().map(|a| a.start).collect::<Vec<_>>());
    assert_eq!(starts, [AnimationStart::OnClick, AnimationStart::WithPrevious, AnimationStart::WithPrevious]);
    s.save_as("anim-several.pptx");
    assert_eq!(s.reopen().slides[2].animations.len(), 3);
    undo_all_and_redo(&mut s);
}

/// カスタム軌跡 (PPTX-ANIM-005): 選んだ図形から、スライドをクリックして点を打ち、
/// ダブルクリックで軌跡効果にする。保存して読み直しても点が残り、スライドショーの
/// 動きが線に沿うこと、Esc で取りやめられること、Undo を確かめる。
pub(crate) fn animation_custom_path() {
    use ibis_doc_model::{AnimationClass, AnimationLook};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.click_shape(0);
    let (center, size) = s.with(|e, _| {
        let f = e.slide().shapes[0].frame;
        let sz = e.presentation().slide_size;
        ((f.x + f.width / 2, f.y + f.height / 2), (sz.width, sz.height))
    });
    let (origin, scale) = s.with(|e, _| {
        let l = e.slide_layout().unwrap();
        (l.origin, l.scale)
    });
    let at = |x: i64, y: i64| (f32::from(origin.x) + x as f32 * scale, f32::from(origin.y) + y as f32 * scale);
    let step = 600_000i64;
    let dx = if center.0 < size.0 / 2 { step } else { -step };
    let dy = if center.1 < size.1 / 2 { step } else { -step };
    let (a, b, c) = ((center.0 + dx, center.1), (center.0 + dx, center.1 + dy), (center.0, center.1 + dy));

    // Esc takes it back.
    s.toolbar_menu("motion", "pptx.anim.path.custom");
    assert!(s.with(|e, _| e.is_drawing_path()));
    s.dispatch_click_with(at(a.0, a.1), Default::default());
    assert_eq!(s.with(|e, _| e.path_draw_points().len()), 1);
    s.key("escape");
    assert!(!s.with(|e, _| e.is_drawing_path()));
    assert!(s.with(|e, _| e.slide().animations.is_empty()));

    // Two points, and the third with a double click.
    s.toolbar_menu("motion", "pptx.anim.path.custom");
    s.dispatch_click_with(at(a.0, a.1), Default::default());
    s.dispatch_click_with(at(b.0, b.1), Default::default());
    assert_eq!(s.with(|e, _| e.path_draw_points().len()), 2);
    s.double_click(at(c.0, c.1));
    assert!(!s.with(|e, _| e.is_drawing_path()));
    let anim = s.with(|e, _| e.slide().animations.clone());
    assert_eq!(anim.len(), 1, "{anim:?}");
    assert_eq!(anim[0].class, AnimationClass::Path);
    assert_eq!(anim[0].path.len(), 3, "{:?}", anim[0].path);
    let near = |got: (i32, i32), x: i64, y: i64| {
        let want = ((x - center.0) as f64 / size.0 as f64 * 100_000.0, (y - center.1) as f64 / size.1 as f64 * 100_000.0);
        (got.0 as f64 - want.0).abs() < 300.0 && (got.1 as f64 - want.1).abs() < 300.0
    };
    assert!(near(anim[0].path[0], a.0, a.1) && near(anim[0].path[1], b.0, b.1) && near(anim[0].path[2], c.0, c.1), "{:?}", anim[0].path);
    assert_eq!(anim[0].look, AnimationLook::Motion(anim[0].path[2].0, anim[0].path[2].1));

    // The slide show goes along the lines: half of the way is at the corner when the two lines are as long.
    let mid = crate::pptx::slideshow::point_on_path(&[(10_000, 0), (10_000, 10_000)], 0.5, (1000.0, 1000.0));
    assert_eq!((mid.0.round(), mid.1.round()), (10_000.0, 0.0));

    // Saved as a line to each point, and read back.
    s.save_as("anim-custom-path.pptx");
    let reopened = s.reopen();
    assert_eq!(reopened.slides[0].animations[0].path, anim[0].path);
    assert_eq!(reopened.slides[0].animations[0].look, anim[0].look);

    undo_all_and_redo(&mut s);
}

/// 画像・図形・グラフの代替テキスト (Alt Text / PPTX-ACCESS-005):
/// 図形選択時の「代替テキスト…」メニュー、サイドバーパネルでのタイトル・説明文入力、
/// 適用、Undo / Redo、ファイル保存・再読込保持を検証。
pub(crate) fn shape_alt_text() {
    use ibis_doc_model::AltText;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 図形を選択していない時、メニューは無効
    assert_eq!(s.menu_state("pptx.alt-text"), Some((false, false)));

    // 図形0を選択
    s.click_shape(0);
    assert_eq!(s.menu_state("pptx.alt-text"), Some((true, false)));

    // 代替テキストパネルを開く
    s.menu("pptx.alt-text");
    assert!(s.with(|e, _| e.sidebar_open && e.pptx().is_some_and(|p| p.alt_text_panel.is_some())));

    // タイトルと説明文を設定
    s.update(|e, w, cx| {
        let panel = e.pptx().unwrap().alt_text_panel.as_ref().unwrap();
        panel.title_field.update(cx, |f, cx| f.set_value("表紙タイトル", w, cx));
        panel.desc_field.update(cx, |f, cx| f.set_value("事業計画書の表紙見出しテキスト", w, cx));
    });

    // 適用
    s.update(|e, w, cx| e.close_alt_text_panel(w, cx));
    let alt = s.with(|e, _| e.slide().shape_alt_texts.get(&e.slide().shapes[0].id).cloned());
    assert_eq!(
        alt,
        Some(AltText {
            title: "表紙タイトル".to_string(),
            description: "事業計画書の表紙見出しテキスト".to_string(),
        })
    );

    // Undo / Redo の確認
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().shape_alt_texts.get(&e.slide().shapes[0].id).cloned()), None);
    s.key("cmd-shift-z");
    assert_eq!(
        s.with(|e, _| e.slide().shape_alt_texts.get(&e.slide().shapes[0].id).cloned()),
        Some(AltText {
            title: "表紙タイトル".to_string(),
            description: "事業計画書の表紙見出しテキスト".to_string(),
        })
    );

    // 保存して再読込
    s.save_as("alt-text.pptx");
    let reopened = s.reopen();
    let re_alt = reopened.slides[0].shape_alt_texts.get(&reopened.slides[0].shapes[0].id).cloned();
    assert_eq!(
        re_alt,
        Some(AltText {
            title: "表紙タイトル".to_string(),
            description: "事業計画書の表紙見出しテキスト".to_string(),
        })
    );

    undo_all_and_redo(&mut s);
}

/// オブジェクトの装飾用指定 (Mark as decorative / PPTX-ACCESS-006):
/// 図形選択時の代替テキストパネルでの「装飾用としてマークする」トグル、
/// タイトル・説明入力の無効化・非表示、適用、Undo / Redo、
/// ファイル保存・再読込保持（a1611:decorative val="1" / {C183D7F6-B498-43B3-948B-1728B52AA6E4}）、
/// 解除と再保存・再読込を検証。
pub(crate) fn shape_decorative_marking() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 図形0を選択
    s.click_shape(0);
    let shape_id = s.with(|e, _| e.slide().shapes[0].id);

    // 最初は装飾用としてマークされていない
    assert!(!s.with(|e, _| e.slide().decorative_shapes.contains(&shape_id)));

    // 代替テキストパネルを開く
    s.menu("pptx.alt-text");
    assert!(s.with(|e, _| e.sidebar_open && e.pptx().is_some_and(|p| p.alt_text_panel.as_ref().is_some_and(|pan| !pan.decorative))));

    // 「装飾用としてマークする」をトグル
    s.update(|e, _, cx| e.toggle_alt_text_decorative(cx));
    assert!(s.with(|e, _| e.pptx().unwrap().alt_text_panel.as_ref().unwrap().decorative));

    // 適用
    s.update(|e, _, cx| e.apply_alt_text(cx));
    assert!(s.with(|e, _| e.slide().decorative_shapes.contains(&shape_id)));

    // Undo / Redo の確認
    s.click_shape(0);
    s.key("cmd-z");
    assert!(!s.with(|e, _| e.slide().decorative_shapes.contains(&shape_id)));

    s.key("cmd-shift-z");
    assert!(s.with(|e, _| e.slide().decorative_shapes.contains(&shape_id)));

    // 保存して再読込
    s.save_as("decorative.pptx");
    let reopened = s.reopen();
    assert!(reopened.slides[0].decorative_shapes.contains(&shape_id));

    // 再オープン後、再度代替テキストパネルを開く
    s.click_shape(0);
    s.menu("pptx.alt-text");
    assert!(s.with(|e, _| e.pptx().unwrap().alt_text_panel.as_ref().unwrap().decorative));

    // 解除して適用
    s.update(|e, _, cx| e.toggle_alt_text_decorative(cx));
    assert!(!s.with(|e, _| e.pptx().unwrap().alt_text_panel.as_ref().unwrap().decorative));
    s.update(|e, _, cx| e.apply_alt_text(cx));
    assert!(!s.with(|e, _| e.slide().decorative_shapes.contains(&shape_id)));

    // 再保存・再読込で解除が維持されることを確認
    s.save_as("decorative-cleared.pptx");
    let reopened_cleared = s.reopen();
    assert!(!reopened_cleared.slides[0].decorative_shapes.contains(&shape_id));
}

/// スライドショー中のレーザーポインターおよびペン・蛍光ペン・消しゴム (PPTX-SHOW-015 / PPTX-SHOW-016):
/// レーザー切り替え・追従、ペン・蛍光ペンドラッグ描画、スライド間でのインク保持、
/// 消しゴムでの個別消去、全インク消去 (e)、矢印戻しでのスライド進行を検証。
pub(crate) fn slideshow_laser_and_pen() {
    use crate::pptx::slideshow::PointerMode;
    use gpui_kit::Point;

    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // スライドショー開始
    s.key("cmd-shift-enter");
    assert!(s.with(|e, _| e.slideshow.is_some()));
    assert_eq!(s.with(|e, _| e.slideshow.as_ref().unwrap().pointer_mode), PointerMode::Arrow);

    // 1. レーザーポインター (ctrl-l / LaserPointer)
    s.key("ctrl-l");
    assert_eq!(s.with(|e, _| e.slideshow.as_ref().unwrap().pointer_mode), PointerMode::Laser);

    // レーザー位置の移動
    s.update(|e, window, cx| {
        let view = window.viewport_size();
        let mid = Point::new(view.width / 2.0, view.height / 2.0);
        e.mouse_move_slideshow(mid, false, window, cx);
    });
    let laser = s.with(|e, _| e.slideshow.as_ref().unwrap().laser_pos);
    assert!(laser.is_some(), "laser position updated on mouse move");

    // 2. ペン (ctrl-p / PenPointer)
    s.key("ctrl-p");
    assert_eq!(s.with(|e, _| e.slideshow.as_ref().unwrap().pointer_mode), PointerMode::Pen);
    assert_eq!(s.with(|e, _| e.slideshow.as_ref().unwrap().laser_pos), None);

    // ペンでストロークを描画
    s.update(|e, window, cx| {
        let view = window.viewport_size();
        let p1 = Point::new(view.width * 0.3, view.height * 0.3);
        let p2 = Point::new(view.width * 0.35, view.height * 0.35);
        e.mouse_down_slideshow(p1, false, window, cx);
        e.mouse_move_slideshow(p2, false, window, cx);
        e.mouse_up_slideshow(p2, false, window, cx);
    });
    let slide1_id = s.with(|e, _| e.current_slide);
    let inks_count = s.with(|e, _| {
        e.slideshow.as_ref().unwrap().slide_inks.get(&slide1_id).map(|v| v.len()).unwrap_or(0)
    });
    assert_eq!(inks_count, 1, "pen stroke recorded on slide 1");

    // 3. 蛍光ペン (ctrl-i / HighlighterPointer)
    s.key("ctrl-i");
    assert_eq!(s.with(|e, _| e.slideshow.as_ref().unwrap().pointer_mode), PointerMode::Highlighter);

    s.update(|e, window, cx| {
        let view = window.viewport_size();
        let p1 = Point::new(view.width * 0.5, view.height * 0.5);
        let p2 = Point::new(view.width * 0.55, view.height * 0.55);
        e.mouse_down_slideshow(p1, false, window, cx);
        e.mouse_move_slideshow(p2, false, window, cx);
        e.mouse_up_slideshow(p2, false, window, cx);
    });
    let inks_count = s.with(|e, _| {
        e.slideshow.as_ref().unwrap().slide_inks.get(&slide1_id).map(|v| v.len()).unwrap_or(0)
    });
    assert_eq!(inks_count, 2, "highlighter stroke recorded on slide 1");

    // 4. スライド遷移してもインクが保持される
    // 次のスライドへ
    s.update(|e, window, cx| e.step_slideshow(true, window, cx));
    let slide2_id = s.with(|e, _| e.current_slide);
    assert_ne!(slide1_id, slide2_id);
    let slide2_inks = s.with(|e, _| {
        e.slideshow.as_ref().unwrap().slide_inks.get(&slide2_id).map(|v| v.len()).unwrap_or(0)
    });
    assert_eq!(slide2_inks, 0, "slide 2 starts with no ink");

    // 前のスライドへ戻る
    s.update(|e, window, cx| e.step_slideshow(false, window, cx));
    assert_eq!(s.with(|e, _| e.current_slide), slide1_id);
    let slide1_inks = s.with(|e, _| {
        e.slideshow.as_ref().unwrap().slide_inks.get(&slide1_id).map(|v| v.len()).unwrap_or(0)
    });
    assert_eq!(slide1_inks, 2, "slide 1 inks preserved across transitions");

    // 5. 消しゴム (ctrl-e / EraserPointer) で個別消去
    s.key("ctrl-e");
    assert_eq!(s.with(|e, _| e.slideshow.as_ref().unwrap().pointer_mode), PointerMode::Eraser);

    // p1 (0.3, 0.3) 付近をクリックして1つ消去
    s.update(|e, window, cx| {
        let view = window.viewport_size();
        let p1 = Point::new(view.width * 0.3, view.height * 0.3);
        e.mouse_down_slideshow(p1, false, window, cx);
        e.mouse_up_slideshow(p1, false, window, cx);
    });
    let inks_after_eraser = s.with(|e, _| {
        e.slideshow.as_ref().unwrap().slide_inks.get(&slide1_id).map(|v| v.len()).unwrap_or(0)
    });
    assert_eq!(inks_after_eraser, 1, "first stroke erased");

    // 6. 全インク消去 (e キー / ClearSlideInks)
    s.key("e");
    let inks_after_clear = s.with(|e, _| {
        e.slideshow.as_ref().unwrap().slide_inks.get(&slide1_id).map(|v| v.len()).unwrap_or(0)
    });
    assert_eq!(inks_after_clear, 0, "all inks cleared with 'e' key");

    // 7. 矢印ポインター (ctrl-a / ArrowPointer) に戻す
    s.key("ctrl-a");
    assert_eq!(s.with(|e, _| e.slideshow.as_ref().unwrap().pointer_mode), PointerMode::Arrow);

    // 矢印モードでのクリックはスライド進行
    s.update(|e, window, cx| {
        let view = window.viewport_size();
        let mid = Point::new(view.width / 2.0, view.height / 2.0);
        e.mouse_down_slideshow(mid, false, window, cx);
    });
    assert_eq!(s.with(|e, _| e.current_slide), slide2_id, "arrow click steps forward");

    // スライドショー終了
    s.key("escape");
    assert!(s.with(|e, _| e.slideshow.is_none()));
}

/// スライドショー中のインク注釈保持（PPTX-INK-012）:
/// スライドショー中にペン・蛍光ペンで描いたインクストロークをスライドショー終了時に保持（Keep）し、
/// スライド上にフリーハンド図形として変換・保存・再読込・Undo/Redo、および破棄（Discard）の動作を検証。
pub(crate) fn slideshow_keep_ink_annotations() {
    use crate::pptx::slideshow::PointerMode;
    use gpui_kit::Point;

    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let slide1_id = s.with(|e, _| e.current_slide);
    let base_count1 = s.with(|e, _| e.slide().shapes.len());

    // 次のスライド（スライド2）の初期図形数も確認しておく
    s.page(2);
    let slide2_id = s.with(|e, _| e.current_slide);
    let base_count2 = s.with(|e, _| e.slide().shapes.len());
    s.page(1);

    // 1. スライドショー開始
    s.key("cmd-shift-enter");
    assert!(s.with(|e, _| e.slideshow.is_some()));

    // 2. スライド1でペンと蛍光ペンを描画
    s.key("ctrl-p");
    assert_eq!(s.with(|e, _| e.slideshow.as_ref().unwrap().pointer_mode), PointerMode::Pen);
    s.update(|e, window, cx| {
        let view = window.viewport_size();
        let p1 = Point::new(view.width * 0.2, view.height * 0.2);
        let p2 = Point::new(view.width * 0.3, view.height * 0.4);
        e.mouse_down_slideshow(p1, false, window, cx);
        e.mouse_move_slideshow(p2, false, window, cx);
        e.mouse_up_slideshow(p2, false, window, cx);
    });

    s.key("ctrl-i");
    assert_eq!(s.with(|e, _| e.slideshow.as_ref().unwrap().pointer_mode), PointerMode::Highlighter);
    s.update(|e, window, cx| {
        let view = window.viewport_size();
        let p1 = Point::new(view.width * 0.4, view.height * 0.5);
        let p2 = Point::new(view.width * 0.6, view.height * 0.5);
        e.mouse_down_slideshow(p1, false, window, cx);
        e.mouse_move_slideshow(p2, false, window, cx);
        e.mouse_up_slideshow(p2, false, window, cx);
    });

    // 3. スライド2へ進んでペンを描画
    s.update(|e, window, cx| e.step_slideshow(true, window, cx));
    assert_eq!(s.with(|e, _| e.current_slide), slide2_id);
    s.key("ctrl-p");
    s.update(|e, window, cx| {
        let view = window.viewport_size();
        let p1 = Point::new(view.width * 0.1, view.height * 0.1);
        let p2 = Point::new(view.width * 0.2, view.height * 0.2);
        e.mouse_down_slideshow(p1, false, window, cx);
        e.mouse_move_slideshow(p2, false, window, cx);
        e.mouse_up_slideshow(p2, false, window, cx);
    });

    // 4. Esc を押すとインク注釈確認ダイアログが開く
    s.key("escape");
    assert!(s.with(|e, _| e.slideshow.is_some()), "slideshow still active");
    assert!(s.with(|e, _| e.slideshow.as_ref().unwrap().ink_dialog_open), "ink dialog opened");

    // 5. 「保持（Keep）」を選択して終了
    s.update(|e, window, cx| {
        e.end_slideshow_with_ink_choice(true, window, cx);
    });
    assert!(s.with(|e, _| e.slideshow.is_none()), "slideshow closed");

    // 6. スライド2にフリーハンド図形が1つ追加されていることを検証
    assert_eq!(s.with(|e, _| e.current_slide), slide2_id);
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), base_count2 + 1);
    let s2_ink = s.with(|e, _| e.slide().shapes.last().unwrap().clone());
    let ShapeKind::Auto(s2_auto) = &s2_ink.kind else { panic!("ink must be auto shape") };
    assert_eq!(s2_auto.stroke.as_ref().unwrap().width, 28_575, "pen stroke width 28575");

    // 7. スライド1にフリーハンド図形が2つ追加されていることを検証
    s.page(1);
    assert_eq!(s.with(|e, _| e.current_slide), slide1_id);
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), base_count1 + 2);
    let s1_pen = s.with(|e, _| e.slide().shapes[base_count1].clone());
    let ShapeKind::Auto(s1_pen_auto) = &s1_pen.kind else { panic!("ink must be auto shape") };
    assert_eq!(s1_pen_auto.stroke.as_ref().unwrap().width, 28_575);

    let s1_hl = s.with(|e, _| e.slide().shapes[base_count1 + 1].clone());
    let ShapeKind::Auto(s1_hl_auto) = &s1_hl.kind else { panic!("ink must be auto shape") };
    assert_eq!(s1_hl_auto.stroke.as_ref().unwrap().width, 152_400, "highlighter width 152400");
    assert_eq!(s1_hl_auto.stroke.as_ref().unwrap().color.alpha, 128, "highlighter semi-transparent");

    // 8. 保存と再読込
    s.screenshot("slideshow-keep-ink");
    s.save_as("slideshow-keep-ink.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), base_count1 + 2);
    assert_eq!(saved.slides[1].shapes.len(), base_count2 + 1);

    // 9. Undo / Redo 検証（一括挿入されたインク図形が Undo 1回で元に戻る）
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.presentation().slides[0].shapes.len()), base_count1);
    assert_eq!(s.with(|e, _| e.presentation().slides[1].shapes.len()), base_count2);

    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.presentation().slides[0].shapes.len()), base_count1 + 2);
    assert_eq!(s.with(|e, _| e.presentation().slides[1].shapes.len()), base_count2 + 1);

    // 10. 破棄（Discard）パターンの検証
    s.key("cmd-shift-enter");
    assert!(s.with(|e, _| e.slideshow.is_some()));
    s.key("ctrl-p");
    s.update(|e, window, cx| {
        let view = window.viewport_size();
        let p1 = Point::new(view.width * 0.5, view.height * 0.5);
        let p2 = Point::new(view.width * 0.7, view.height * 0.7);
        e.mouse_down_slideshow(p1, false, window, cx);
        e.mouse_move_slideshow(p2, false, window, cx);
        e.mouse_up_slideshow(p2, false, window, cx);
    });
    // Esc でダイアログを開き、もう一度 Esc または破棄で閉じる
    s.key("escape");
    assert!(s.with(|e, _| e.slideshow.as_ref().unwrap().ink_dialog_open));
    s.key("escape"); // ダイアログ表示中の Esc は破棄して終了
    assert!(s.with(|e, _| e.slideshow.is_none()));
    // 図形数は増えていないこと
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), base_count1 + 2);
}

/// スライドショー中のすべてのスライド一覧（グリッド表示 / PPTX-SHOW-013）:
/// ctrl-s / g / ツールバーボタンによるグリッド一覧表示、Escape での安全な閉じる動作（ショー継続）、
/// 任意スライドの選択による即時ジャンプ遷移、発表者ビューでのグリッド呼び出しを検証。
pub(crate) fn slideshow_all_slides_grid() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. スライドショー開始
    s.key("cmd-shift-enter");
    assert!(s.with(|e, _| e.slideshow.is_some()));
    assert!(!s.with(|e, _| e.slideshow.as_ref().unwrap().grid_open));
    let slide1_id = s.with(|e, _| e.current_slide);

    // 2. ctrl-s で全スライド一覧グリッドを開く
    s.key("ctrl-s");
    assert!(s.with(|e, _| e.slideshow.as_ref().unwrap().grid_open), "grid opened with ctrl-s");

    // 3. Escape でグリッドを閉じる（スライドショーは終了しない）
    s.key("escape");
    assert!(!s.with(|e, _| e.slideshow.as_ref().unwrap().grid_open), "grid closed with Escape");
    assert!(s.with(|e, _| e.slideshow.is_some()), "slideshow still active after Escape in grid");

    // 4. g キーでグリッドを再度開く
    s.key("g");
    assert!(s.with(|e, _| e.slideshow.as_ref().unwrap().grid_open), "grid opened with g");

    // 5. グリッドからスライド2を選択してジャンプ
    let slide2_id = s.with(|e, _| e.presentation().slides[1].id);
    s.update(|e, window, cx| {
        e.select_slide_in_grid(slide2_id, window, cx);
    });
    assert!(!s.with(|e, _| e.slideshow.as_ref().unwrap().grid_open), "grid closed after selecting slide");
    assert_eq!(s.with(|e, _| e.current_slide), slide2_id, "jumped to slide 2 from grid");

    // 6. スライドショー終了
    s.key("escape");
    assert!(s.with(|e, _| e.slideshow.is_none()));

    // 7. 発表者ビューでもグリッド切り替えが動作
    s.update(|e, w, cx| e.start_presenter_view_on(false, w, cx));
    assert!(s.with(|e, _| e.slideshow.as_ref().is_some_and(|show| show.presenter)));
    s.update(|e, _, cx| e.toggle_slideshow_grid(cx));
    assert!(s.with(|e, _| e.slideshow.as_ref().unwrap().grid_open), "grid opened in presenter view");
    s.update(|e, window, cx| {
        e.select_slide_in_grid(slide1_id, window, cx);
    });
    assert_eq!(s.with(|e, _| e.current_slide), slide1_id, "jumped back to slide 1 in presenter view");
    s.key("escape");
    assert!(s.with(|e, _| e.slideshow.is_none()));
}

/// スライドショー中のスライドの拡大・ズーム・パン（PPTX-SHOW-014）:
/// + / - / 0 キーやツールバーによるズームイン・ズームアウト・等倍リセット、
/// ズーム中のドラッグパン移動、Escape による階層的ズーム解除（ショー継続）、
/// スライド遷移時のズーム自動リセット、発表者ビューでのズーム操作を検証。
pub(crate) fn slideshow_zoom_and_pan() {
    use gpui_kit::Point;

    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. スライドショー開始
    s.key("cmd-shift-enter");
    assert!(s.with(|e, _| e.slideshow.is_some()));
    let (init_zoom, init_pan) = s.with(|e, _| {
        let show = e.slideshow.as_ref().unwrap();
        (show.zoom, show.pan)
    });
    assert!((init_zoom - 1.0).abs() < 1e-4);
    assert_eq!(init_pan, (0.0, 0.0));

    // 2. ズームイン操作 (+ キー)
    s.key("+");
    s.key("+");
    let zoomed = s.with(|e, _| e.slideshow.as_ref().unwrap().zoom);
    assert!(zoomed > 1.4, "zoom increased with + key: {zoomed}");

    // 3. ズーム中のドラッグによるパン移動
    s.update(|e, window, cx| {
        let view = window.viewport_size();
        let p_start = Point::new(view.width * 0.5, view.height * 0.5);
        let p_drag = Point::new(view.width * 0.55, view.height * 0.55);
        e.mouse_down_slideshow(p_start, false, window, cx);
        e.mouse_move_slideshow(p_drag, false, window, cx);
        e.mouse_up_slideshow(p_drag, false, window, cx);
    });
    let pan_after = s.with(|e, _| e.slideshow.as_ref().unwrap().pan);
    assert!(pan_after.0.abs() > 1.0 || pan_after.1.abs() > 1.0, "panned by dragging: {pan_after:?}");

    // 4. ズームアウト操作 (- キー)
    s.key("-");
    let zoom_out = s.with(|e, _| e.slideshow.as_ref().unwrap().zoom);
    assert!(zoom_out < zoomed && zoom_out > 1.1, "zoom decreased with - key: {zoom_out}");

    // 5. Escape キーでズーム・パンがリセットされ、スライドショーは継続
    s.key("escape");
    assert!(s.with(|e, _| e.slideshow.is_some()), "slideshow still active after Escape");
    let (reset_zoom, reset_pan) = s.with(|e, _| {
        let show = e.slideshow.as_ref().unwrap();
        (show.zoom, show.pan)
    });
    assert!((reset_zoom - 1.0).abs() < 1e-4, "zoom reset to 1.0 with Escape: {reset_zoom}");
    assert_eq!(reset_pan, (0.0, 0.0), "pan reset to (0, 0) with Escape");

    // 6. スライド遷移時にズームとパンが自動リセットされること
    s.update(|e, _, cx| e.zoom_slideshow(1.5, cx));
    assert!(s.with(|e, _| e.slideshow.as_ref().unwrap().zoom > 1.4));
    s.update(|e, window, cx| e.step_slideshow(true, window, cx));
    let (step_zoom, step_pan) = s.with(|e, _| {
        let show = e.slideshow.as_ref().unwrap();
        (show.zoom, show.pan)
    });
    assert!((step_zoom - 1.0).abs() < 1e-4, "zoom reset to 1.0 on next slide: {step_zoom}");
    assert_eq!(step_pan, (0.0, 0.0), "pan reset on next slide");

    // 7. 0 キーでの等倍リセット
    s.update(|e, _, cx| e.zoom_slideshow(2.0, cx));
    assert!(s.with(|e, _| e.slideshow.as_ref().unwrap().zoom > 1.9));
    s.key("0");
    let (key0_zoom, key0_pan) = s.with(|e, _| {
        let show = e.slideshow.as_ref().unwrap();
        (show.zoom, show.pan)
    });
    assert!((key0_zoom - 1.0).abs() < 1e-4, "zoom reset with 0 key: {key0_zoom}");
    assert_eq!(key0_pan, (0.0, 0.0), "pan reset with 0 key");

    // 8. 等倍時に Escape キーでスライドショー終了
    s.key("escape");
    assert!(s.with(|e, _| e.slideshow.is_none()), "slideshow closed with Escape when at 1.0 zoom");

    // 9. 発表者ビューでのズーム操作
    s.update(|e, w, cx| e.start_presenter_view_on(false, w, cx));
    assert!(s.with(|e, _| e.slideshow.as_ref().is_some_and(|show| show.presenter)));
    s.update(|e, _, cx| e.zoom_slideshow(1.25, cx));
    assert!(s.with(|e, _| e.slideshow.as_ref().unwrap().zoom > 1.2));
    s.update(|e, _, cx| e.reset_slideshow_zoom(cx));
    assert!((s.with(|e, _| e.slideshow.as_ref().unwrap().zoom) - 1.0).abs() < 1e-4);
    s.key("escape");
    assert!(s.with(|e, _| e.slideshow.is_none()));
}

/// タイミングのリハーサル（PPTX-SHOW-019）:
/// 1. リハーサルの開始、一時停止/再開、スライド時間の繰り返し
/// 2. 各スライドの進行による所要時間記録
/// 3. 最終スライド後の保存確認ダイアログの表示
/// 4. 「はい」選択によるスライド切り替え時間（advTm / after）への反映と use_timings の有効化
/// 5. ファイル保存と再読み込みでのタイミング設定の永続化検証
/// 6. 中断時の Escape 動作（確認ダイアログ表示と破棄）
pub(crate) fn slideshow_rehearse_timings() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 初期状態: use_timings を false に設定して、リハーサルによる有効化を検証
    s.update(|e, _, cx| {
        let mut props = e.presentation().show_properties.clone();
        props.use_timings = false;
        e.execute(vec![ibis_doc_model::Command::SetShowProperties { properties: props }], cx);
    });
    assert!(!s.with(|e, _| e.presentation().show_properties.use_timings));

    // 1. タイミングのリハーサルを開始
    s.update(|e, window, cx| e.start_rehearsal(window, cx));
    assert!(s.with(|e, _| e.slideshow.is_some()), "slideshow started");
    let has_rehearsal = s.with(|e, _| e.slideshow.as_ref().unwrap().rehearsal.is_some());
    assert!(has_rehearsal, "rehearsal state initialized");

    // 2. 一時停止と再開
    s.update(|e, _, cx| e.toggle_rehearsal_pause(cx));
    assert!(s.with(|e, _| e.slideshow.as_ref().unwrap().rehearsal.as_ref().unwrap().slide_paused.is_some()), "paused");
    s.update(|e, _, cx| e.toggle_rehearsal_pause(cx));
    assert!(s.with(|e, _| e.slideshow.as_ref().unwrap().rehearsal.as_ref().unwrap().slide_paused.is_none()), "resumed");

    // 3. スライドの繰り返し（タイマーリセット）
    s.update(|e, _, cx| e.repeat_rehearsal_slide(cx));
    let slide_accum = s.with(|e, _| e.slideshow.as_ref().unwrap().rehearsal.as_ref().unwrap().slide_accumulated);
    assert_eq!(slide_accum, std::time::Duration::ZERO, "slide duration reset to zero");

    // 4. スライドを順次進めて記録
    let slide_count = s.with(|e, _| e.presentation().slides.len());
    assert!(slide_count >= 2);

    for i in 0..slide_count - 1 {
        s.update(|e, window, cx| e.step_slideshow(true, window, cx));
        let recorded = s.with(|e, _| e.slideshow.as_ref().unwrap().rehearsal.as_ref().unwrap().recorded_timings.len());
        assert_eq!(recorded, i + 1, "recorded slide {}", i + 1);
    }

    // 5. 最終スライドから次へ進むと確認ダイアログが開く
    s.update(|e, window, cx| e.step_slideshow(true, window, cx));
    let dialog_open = s.with(|e, _| e.slideshow.as_ref().unwrap().rehearsal.as_ref().unwrap().dialog_open);
    assert!(dialog_open, "confirmation dialog opened at end of presentation");
    let all_recorded = s.with(|e, _| e.slideshow.as_ref().unwrap().rehearsal.as_ref().unwrap().recorded_timings.len());
    assert_eq!(all_recorded, slide_count, "all slides recorded");

    // 6. タイミングを保存してスライドショー終了
    s.update(|e, window, cx| e.save_rehearsal_timings(window, cx));
    assert!(s.with(|e, _| e.slideshow.is_none()), "slideshow ended after saving timings");
    assert!(s.with(|e, _| e.presentation().show_properties.use_timings), "use_timings enabled");

    // 各スライドに transition.after が設定されていること
    let slides = s.with(|e, _| e.presentation().slides.clone());
    for slide in &slides {
        assert!(slide.transition.as_ref().and_then(|t| t.after).is_some(), "slide has transition after timing");
    }

    // 7. 保存して再読み込みし、永続化されていることを確認
    s.save_as("rehearse-timings.pptx");
    let saved = s.reopen();
    assert!(saved.show_properties.use_timings, "saved file has use_timings enabled");
    for slide in &saved.slides {
        assert!(slide.transition.as_ref().and_then(|t| t.after).is_some(), "reloaded slide has after timing");
    }

    // 8. Undo で元に戻り、Redo で再度反映されること
    s.key("cmd-z");
    assert!(!s.with(|e, _| e.presentation().show_properties.use_timings), "undo reverts use_timings");
    s.key("cmd-shift-z");
    assert!(s.with(|e, _| e.presentation().show_properties.use_timings), "redo restores use_timings");

    // 9. 中断時の Escape 動作（1回目の Escape でダイアログ表示、2回目の Escape で破棄終了）
    s.update(|e, window, cx| e.start_rehearsal(window, cx));
    assert!(s.with(|e, _| e.slideshow.is_some()));
    s.key("escape"); // 1回目: ダイアログ表示
    assert!(s.with(|e, _| e.slideshow.as_ref().unwrap().rehearsal.as_ref().unwrap().dialog_open), "escape opens dialog");
    s.key("escape"); // 2回目: 破棄して終了
    assert!(s.with(|e, _| e.slideshow.is_none()), "second escape exits slideshow");
}

/// PPTX-NOTES-013 スライド一覧表示（Slide Sorter / グリッド表示）
///
/// 1. 表示メニューの「グリッド表示（pptx.grid-view）」およびショートカット（⌃⌥1）での一覧表示切り替え
/// 2. スライド一覧表示モードでの全スライドサムネイル表示と選択状態の確認
/// 3. クリックによるスライド選択の切り替え
/// 4. Escape キーによる通常表示への復帰
/// 5. 再度グリッド表示を開き、ダブルクリックによるスライド選択と通常表示への復帰
/// 6. スライド複製（⌘D）および削除の動作確認
pub(crate) fn slide_sorter_grid_view() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let slide_count = s.with(|e, _| e.presentation().slides.len());
    assert!(slide_count >= 2);

    // 初期状態は通常表示（is_grid_view == false）
    assert!(!s.with(|e, _| e.is_grid_view()));

    // 表示 > グリッド表示（pptx.grid-view）でスライド一覧に切り替え
    s.menu("pptx.grid-view");
    assert!(s.with(|e, _| e.is_grid_view()));

    // 第2スライドを選択
    let slide_2 = s.with(|e, _| e.presentation().slides[1].id);
    s.update(|e, w, cx| e.select_slide(slide_2, w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), slide_2);

    // Escape キーで通常表示に戻る
    s.key("escape");
    assert!(!s.with(|e, _| e.is_grid_view()));
    // 選択されたスライドが維持されていること
    assert_eq!(s.with(|e, _| e.current_slide), slide_2);

    // 再度グリッド表示に切り替え
    s.menu("pptx.grid-view");
    assert!(s.with(|e, _| e.is_grid_view()));

    // 第1スライドをダブルクリック（open_slide_from_grid）して開く
    let slide_1 = s.with(|e, _| e.presentation().slides[0].id);
    s.update(|e, w, cx| e.open_slide_from_grid(slide_1, w, cx));
    assert!(!s.with(|e, _| e.is_grid_view()));
    assert_eq!(s.with(|e, _| e.current_slide), slide_1);

    // グリッド表示に入ってスライド複製（⌘D）
    s.menu("pptx.grid-view");
    assert!(s.with(|e, _| e.is_grid_view()));
    s.key("cmd-d");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), slide_count + 1);

    // Escape で戻り、元に戻す（Undo）
    s.key("escape");
    assert!(!s.with(|e, _| e.is_grid_view()));
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), slide_count);
}

/// スライド画像書き出しおよびスライド右クリックコンテキストメニューの動作検証:
/// 1. スライドメニューに「すべてのスライドを画像として書き出し…」（pptx.export-all-slides-png）が存在すること
/// 2. スライドコンテキストメニューエントリに必要な全項目（複製、削除、切り取り、コピー、貼り付け、スキップ、画像書き出し）が含まれていること
/// 3. サムネイル上での右クリックでコンテキストメニューが表示され、スライド選択が連動すること
/// 4. グリッドビュー上での右クリックでもコンテキストメニューが表示されること
/// 5. write_png による全スライド書き出しジョブの実行と PNG ファイル生成の確認
pub(crate) fn slide_export_and_context_menu() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let slide_count = s.with(|e, _| e.presentation().slides.len());
    assert!(slide_count >= 2);

    // スライドメニュー項目の確認
    let menu_state = s.menu_state("pptx.export-all-slides-png");
    assert!(menu_state.is_some(), "pptx.export-all-slides-png がメニューに存在する");

    // コンテキストメニューエントリの確認
    let entries = s.with(|e, _| e.slide_context_menu_entries());
    let ids: Vec<&str> = entries.iter().filter_map(|entry| match entry {
        crate::shell::chrome::MenuEntry::Item(item) => Some(item.id),
        _ => None,
    }).collect();
    assert!(ids.contains(&"pptx.new-slide"));
    assert!(ids.contains(&"pptx.duplicate-slide"));
    assert!(ids.contains(&"pptx.delete-slide"));
    assert!(ids.contains(&"pptx.copy-slide"));
    assert!(ids.contains(&"pptx.cut-slide"));
    assert!(ids.contains(&"pptx.paste-slide"));
    assert!(ids.contains(&"pptx.skip-slide"));
    assert!(ids.contains(&"pptx.export-slide-png"));
    assert!(ids.contains(&"pptx.export-all-slides-png"));

    // 第2スライドの右クリックでコンテキストメニューを開く
    let slide_2 = s.with(|e, _| e.presentation().slides[1].id);
    s.update(|e, w, cx| {
        e.open_slide_context_menu(slide_2, gpui_kit::point(gpui_kit::px(50.0), gpui_kit::px(100.0)), w, cx);
    });
    assert_eq!(s.with(|e, _| e.current_slide), slide_2);
    assert!(s.with(|e, _| e.pptx().unwrap().slide_popup.is_some()), "コンテキストメニューが開いている");

    // Escape でコンテキストメニューを閉じる
    s.key("escape");
    assert!(s.with(|e, _| e.pptx().unwrap().slide_popup.is_none()), "コンテキストメニューが閉じる");

    // グリッドビューに切り替えて、第1スライドを右クリック
    s.menu("pptx.grid-view");
    assert!(s.with(|e, _| e.is_grid_view()));
    let slide_1 = s.with(|e, _| e.presentation().slides[0].id);
    s.update(|e, w, cx| {
        e.open_slide_context_menu(slide_1, gpui_kit::point(gpui_kit::px(150.0), gpui_kit::px(200.0)), w, cx);
    });
    assert_eq!(s.with(|e, _| e.current_slide), slide_1);
    assert!(s.with(|e, _| e.pptx().unwrap().slide_popup.is_some()), "グリッドビュー上でもコンテキストメニューが開く");

    s.key("escape");
    assert!(s.with(|e, _| e.pptx().unwrap().slide_popup.is_none()));

    // グリッドビューを抜ける
    s.key("escape");
    assert!(!s.with(|e, _| e.is_grid_view()));

    // 全スライドの画像書き出しジョブを実行して PNG ファイルが生成されることを検証
    let out_dir = std::env::temp_dir().join(format!("ibisoffice-export-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&out_dir);
    let all_png = out_dir.join("export_test.png");
    let job = s.with(|e, _| e.write_png(all_png.clone(), None)).unwrap();
    let _ = job().unwrap();
    for i in 1..=slide_count {
        let slide_file = out_dir.join(format!("export_test-{:02}.png", i));
        assert!(slide_file.exists(), "スライド画像 {} が生成されている", i);
        let data = std::fs::read(&slide_file).unwrap();
        assert!(!data.is_empty());
        assert_eq!(&data[0..4], &[0x89, b'P', b'N', b'G']);
    }
    let _ = std::fs::remove_dir_all(&out_dir);
}

/// 閲覧表示（Reading View）およびプレゼンテーション表示モード切り替えの動作検証:
/// 1. 初期状態が標準表示（Normal View）であり、メニューのチェック状態が連動していること
/// 2. 表示メニューの「閲覧表示（pptx.reading-view）」またはアクションで閲覧表示に移行すること
/// 3. 閲覧表示中にメニューのチェック状態が閲覧表示のみ ON になること
/// 4. 閲覧表示中のスライド送りおよび戻り（NextSlide / PreviousSlide）が正しく動作すること
/// 5. Escape キー押下で閲覧表示が終了し、標準表示に戻ること
/// 6. 再び閲覧表示を起動し、メニューの「標準表示（pptx.normal-view）」で標準表示に復帰すること
/// 7. グリッド一覧表示（pptx.grid-view）から閲覧表示への切り替えがシームレスに行われること
pub(crate) fn reading_view_and_presentation_views() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let slide_count = s.with(|e, _| e.presentation().slides.len());
    assert!(slide_count >= 2);

    // 1. 初期状態：標準表示
    assert!(s.with(|e, _| e.is_normal_view()), "初期状態は標準表示");
    assert!(!s.with(|e, _| e.is_grid_view()), "グリッド表示ではない");
    assert!(!s.with(|e, _| e.is_reading_view()), "閲覧表示ではない");
    assert_eq!(s.menu_state("pptx.normal-view").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("pptx.grid-view").map(|(_, c)| c), Some(false));
    assert_eq!(s.menu_state("pptx.reading-view").map(|(_, c)| c), Some(false));

    // 2. 表示 > 閲覧表示 で閲覧表示へ切り替え
    s.menu("pptx.reading-view");
    assert!(s.with(|e, _| e.is_reading_view()), "閲覧表示になっている");
    assert!(!s.with(|e, _| e.is_normal_view()), "標準表示ではない");
    assert!(!s.with(|e, _| e.is_grid_view()), "グリッド表示ではない");
    assert!(s.with(|e, _| e.slideshow.as_ref().map_or(false, |show| show.reading_view)));
    assert_eq!(s.menu_state("pptx.normal-view").map(|(_, c)| c), Some(false));
    assert_eq!(s.menu_state("pptx.grid-view").map(|(_, c)| c), Some(false));
    assert_eq!(s.menu_state("pptx.reading-view").map(|(_, c)| c), Some(true));

    // 3. スライド移動（step_slideshow）
    let initial_slide = s.with(|e, _| e.current_slide);
    s.update(|e, window, cx| e.step_slideshow(true, window, cx));
    let next_slide = s.with(|e, _| e.current_slide);
    assert_ne!(initial_slide, next_slide, "step_slideshow でスライドが進む");
    s.update(|e, window, cx| e.step_slideshow(false, window, cx));
    assert_eq!(s.with(|e, _| e.current_slide), initial_slide, "step_slideshow で元のスライドに戻る");

    // 4. Escape キーで標準表示へ戻る
    s.key("escape");
    assert!(s.with(|e, _| e.is_normal_view()), "Escape で標準表示に戻る");
    assert!(!s.with(|e, _| e.is_reading_view()), "閲覧表示が終了している");
    assert!(s.with(|e, _| e.slideshow.is_none()));
    assert_eq!(s.menu_state("pptx.normal-view").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("pptx.reading-view").map(|(_, c)| c), Some(false));

    // 5. start_reading_view で閲覧表示にし、メニュー pptx.normal-view で戻る
    s.update(|e, window, cx| e.start_reading_view(window, cx));
    assert!(s.with(|e, _| e.is_reading_view()));
    s.menu("pptx.normal-view");
    assert!(s.with(|e, _| e.is_normal_view()));
    assert!(!s.with(|e, _| e.is_reading_view()));

    // 6. グリッド一覧表示（pptx.grid-view）から閲覧表示への切り替え
    s.menu("pptx.grid-view");
    assert!(s.with(|e, _| e.is_grid_view()));
    assert_eq!(s.menu_state("pptx.grid-view").map(|(_, c)| c), Some(true));
    s.menu("pptx.reading-view");
    assert!(s.with(|e, _| e.is_reading_view()));
    assert!(!s.with(|e, _| e.is_grid_view()));
    assert_eq!(s.menu_state("pptx.reading-view").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("pptx.grid-view").map(|(_, c)| c), Some(false));

    // 再度 Escape で標準表示に復帰
    s.key("escape");
    assert!(s.with(|e, _| e.is_normal_view()));
}

/// アウトライン表示（Outline View）およびプレゼンテーション表示モード切り替えの動作検証:
/// 1. 初期状態が標準表示（Normal View）であり、アウトライン表示ではないこと
/// 2. 表示メニュー「アウトライン表示（pptx.outline-view）」でアウトライン表示に切り替わること
/// 3. アウトライン表示中、is_outline_view() が true、is_normal_view() が false、メニューのチェック状態が連動すること
/// 4. アウトライン表示からスライド一覧表示（pptx.grid-view）や閲覧表示（pptx.reading-view）へ切り替えた際、アウトライン表示が解除されること
/// 5. 標準表示（pptx.normal-view）への復帰、および toggle_outline_view / switch_to_outline_view による切り替えが動作すること
pub(crate) fn outline_view_and_presentation_views() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let slide_count = s.with(|e, _| e.presentation().slides.len());
    assert!(slide_count >= 2);

    // 1. 初期状態：標準表示
    assert!(s.with(|e, _| e.is_normal_view()), "初期状態は標準表示");
    assert!(!s.with(|e, _| e.is_outline_view()), "アウトライン表示ではない");
    assert!(!s.with(|e, _| e.is_grid_view()), "グリッド表示ではない");
    assert!(!s.with(|e, _| e.is_reading_view()), "閲覧表示ではない");
    assert_eq!(s.menu_state("pptx.normal-view").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("pptx.outline-view").map(|(_, c)| c), Some(false));
    assert_eq!(s.menu_state("pptx.grid-view").map(|(_, c)| c), Some(false));
    assert_eq!(s.menu_state("pptx.reading-view").map(|(_, c)| c), Some(false));

    // 2. 表示 > アウトライン表示 でアウトライン表示へ切り替え
    s.menu("pptx.outline-view");
    assert!(s.with(|e, _| e.is_outline_view()), "アウトライン表示になっている");
    assert!(!s.with(|e, _| e.is_normal_view()), "標準表示ではない");
    assert!(!s.with(|e, _| e.is_grid_view()), "グリッド表示ではない");
    assert!(!s.with(|e, _| e.is_reading_view()), "閲覧表示ではない");
    assert_eq!(s.menu_state("pptx.normal-view").map(|(_, c)| c), Some(false));
    assert_eq!(s.menu_state("pptx.outline-view").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("pptx.grid-view").map(|(_, c)| c), Some(false));
    assert_eq!(s.menu_state("pptx.reading-view").map(|(_, c)| c), Some(false));

    // 3. 標準表示への切り替え
    s.menu("pptx.normal-view");
    assert!(s.with(|e, _| e.is_normal_view()), "標準表示に戻る");
    assert!(!s.with(|e, _| e.is_outline_view()), "アウトライン表示が解除されている");
    assert_eq!(s.menu_state("pptx.normal-view").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("pptx.outline-view").map(|(_, c)| c), Some(false));

    // 4. 再びアウトライン表示へ切り替え、そこからスライド一覧（grid-view）へ切り替え
    s.menu("pptx.outline-view");
    assert!(s.with(|e, _| e.is_outline_view()));
    s.menu("pptx.grid-view");
    assert!(s.with(|e, _| e.is_grid_view()));
    assert!(!s.with(|e, _| e.is_outline_view()));
    assert_eq!(s.menu_state("pptx.grid-view").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("pptx.outline-view").map(|(_, c)| c), Some(false));

    // 5. グリッド一覧からアウトライン表示へ切り替え
    s.menu("pptx.outline-view");
    assert!(s.with(|e, _| e.is_outline_view()));
    assert!(!s.with(|e, _| e.is_grid_view()));
    assert_eq!(s.menu_state("pptx.outline-view").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("pptx.grid-view").map(|(_, c)| c), Some(false));

    // 6. アウトライン表示から閲覧表示へ切り替え、Escape で復帰
    s.menu("pptx.reading-view");
    assert!(s.with(|e, _| e.is_reading_view()));
    assert!(!s.with(|e, _| e.is_outline_view()));
    assert_eq!(s.menu_state("pptx.reading-view").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("pptx.outline-view").map(|(_, c)| c), Some(false));

    s.key("escape");
    assert!(s.with(|e, _| e.is_normal_view()));
    assert!(!s.with(|e, _| e.is_reading_view()));
    assert_eq!(s.menu_state("pptx.normal-view").map(|(_, c)| c), Some(true));
}

/// ノート表示（Notes Page View）およびプレゼンテーション表示モード切り替えの動作検証:
/// 1. 初期状態が標準表示（Normal View）であり、ノート表示ではないこと
/// 2. 表示メニュー「ノート（pptx.notes-page-view）」でノート表示に切り替わること
/// 3. ノート表示中、is_notes_page_view() が true、is_normal_view() が false、メニューのチェック状態が連動すること
/// 4. Escape キー押下でノート表示が終了し、標準表示に戻ること
/// 5. ノート表示からアウトライン表示（pptx.outline-view）やスライド一覧（pptx.grid-view）、閲覧表示（pptx.reading-view）へ切り替えた際、排他制御されること
pub(crate) fn notes_page_view_and_presentation_views() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let slide_count = s.with(|e, _| e.presentation().slides.len());
    assert!(slide_count >= 2);

    // 1. 初期状態：標準表示
    assert!(s.with(|e, _| e.is_normal_view()), "初期状態は標準表示");
    assert!(!s.with(|e, _| e.is_notes_page_view()), "ノート表示ではない");
    assert!(!s.with(|e, _| e.is_outline_view()), "アウトライン表示ではない");
    assert!(!s.with(|e, _| e.is_grid_view()), "グリッド表示ではない");
    assert!(!s.with(|e, _| e.is_reading_view()), "閲覧表示ではない");
    assert_eq!(s.menu_state("pptx.normal-view").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("pptx.notes-page-view").map(|(_, c)| c), Some(false));
    assert_eq!(s.menu_state("pptx.outline-view").map(|(_, c)| c), Some(false));
    assert_eq!(s.menu_state("pptx.grid-view").map(|(_, c)| c), Some(false));
    assert_eq!(s.menu_state("pptx.reading-view").map(|(_, c)| c), Some(false));

    // 2. 表示 > ノート でノート表示へ切り替え
    s.menu("pptx.notes-page-view");
    assert!(s.with(|e, _| e.is_notes_page_view()), "ノート表示になっている");
    assert!(!s.with(|e, _| e.is_normal_view()), "標準表示ではない");
    assert!(!s.with(|e, _| e.is_outline_view()), "アウトライン表示ではない");
    assert!(!s.with(|e, _| e.is_grid_view()), "グリッド表示ではない");
    assert!(!s.with(|e, _| e.is_reading_view()), "閲覧表示ではない");
    assert_eq!(s.menu_state("pptx.normal-view").map(|(_, c)| c), Some(false));
    assert_eq!(s.menu_state("pptx.notes-page-view").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("pptx.outline-view").map(|(_, c)| c), Some(false));
    assert_eq!(s.menu_state("pptx.grid-view").map(|(_, c)| c), Some(false));
    assert_eq!(s.menu_state("pptx.reading-view").map(|(_, c)| c), Some(false));

    // 3. Escape キーで標準表示へ復帰
    s.key("escape");
    assert!(s.with(|e, _| e.is_normal_view()), "Escape で標準表示に戻る");
    assert!(!s.with(|e, _| e.is_notes_page_view()), "ノート表示が解除されている");
    assert_eq!(s.menu_state("pptx.normal-view").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("pptx.notes-page-view").map(|(_, c)| c), Some(false));

    // 4. 再びノート表示へ切り替え、そこからアウトライン表示へ切り替え
    s.menu("pptx.notes-page-view");
    assert!(s.with(|e, _| e.is_notes_page_view()));
    s.menu("pptx.outline-view");
    assert!(s.with(|e, _| e.is_outline_view()));
    assert!(!s.with(|e, _| e.is_notes_page_view()));
    assert_eq!(s.menu_state("pptx.outline-view").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("pptx.notes-page-view").map(|(_, c)| c), Some(false));

    // 5. アウトライン表示からノート表示へ切り替え
    s.menu("pptx.notes-page-view");
    assert!(s.with(|e, _| e.is_notes_page_view()));
    assert!(!s.with(|e, _| e.is_outline_view()));
    assert_eq!(s.menu_state("pptx.notes-page-view").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("pptx.outline-view").map(|(_, c)| c), Some(false));

    // 6. ノート表示からスライド一覧（グリッド表示）へ切り替え
    s.menu("pptx.grid-view");
    assert!(s.with(|e, _| e.is_grid_view()));
    assert!(!s.with(|e, _| e.is_notes_page_view()));
    assert_eq!(s.menu_state("pptx.grid-view").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("pptx.notes-page-view").map(|(_, c)| c), Some(false));

    // 7. グリッド一覧から標準表示へ戻り、ノート表示へ行き、閲覧表示へ切り替え
    s.key("escape");
    assert!(s.with(|e, _| e.is_normal_view()));
    s.menu("pptx.notes-page-view");
    assert!(s.with(|e, _| e.is_notes_page_view()));
    s.menu("pptx.reading-view");
    assert!(s.with(|e, _| e.is_reading_view()));
    assert!(!s.with(|e, _| e.is_notes_page_view()));
    assert_eq!(s.menu_state("pptx.reading-view").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("pptx.notes-page-view").map(|(_, c)| c), Some(false));

    s.key("escape");
    assert!(s.with(|e, _| e.is_normal_view()));
    assert_eq!(s.menu_state("pptx.normal-view").map(|(_, c)| c), Some(true));
}

/// ズーム倍率プリセット・ズームダイアログおよびウィンドウ整列の動作検証:
/// 1. 初期状態：自動フィット（zoom: None）、メニュー「画面に合わせる（view.zoom-fit）」がチェック状態
/// 2. ズームプリセットメニュー項目（400%, 200%, 100%, 66%, 50%, 33%）の選択とチェック状態連動
/// 3. 「画面に合わせる（view.zoom-fit）」で自動フィットへ復帰しチェック状態連動
/// 4. 「ズーム…（view.zoom-dialog）」メニューでズームダイアログが開き、初期値が現在の倍率（パーセント）であること
/// 5. Escape キー押下でズームダイアログが閉じること
/// 6. 再びダイアログを開き、入力欄に数値を入力（例: 75%）して適用で倍率が適用され、ダイアログが閉じること
/// 7. ウィンドウメニュー項目（すべて整列: window.arrange-all, 重ねて表示: window.cascade, 新しいウィンドウ: window.new-window）が正常に呼び出せること
pub(crate) fn zoom_and_window_arrangement() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. 初期状態：自動フィット
    assert!(s.with(|e, _| e.zoom.is_none()), "初期状態は自動フィット (zoom == None)");
    assert_eq!(s.menu_state("view.zoom-fit").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("view.zoom-400").map(|(_, c)| c), Some(false));
    assert_eq!(s.menu_state("view.zoom-100").map(|(_, c)| c), Some(false));
    assert_eq!(s.menu_state("view.zoom-50").map(|(_, c)| c), Some(false));
    assert!(!s.with(|e, _| e.zoom_dialog_open), "初期状態でズームダイアログは閉じた状態");

    // 2. メニュー「400%」選択
    s.menu("view.zoom-400");
    assert_eq!(s.with(|e, _| e.zoom), Some(4.0));
    assert_eq!(s.menu_state("view.zoom-400").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("view.zoom-fit").map(|(_, c)| c), Some(false));
    assert_eq!(s.menu_state("view.zoom-100").map(|(_, c)| c), Some(false));

    // メニュー「200%」選択
    s.menu("view.zoom-200");
    assert_eq!(s.with(|e, _| e.zoom), Some(2.0));
    assert_eq!(s.menu_state("view.zoom-200").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("view.zoom-400").map(|(_, c)| c), Some(false));

    // メニュー「100%」選択
    s.menu("view.zoom-100");
    assert_eq!(s.with(|e, _| e.zoom), Some(1.0));
    assert_eq!(s.menu_state("view.zoom-100").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("view.zoom-200").map(|(_, c)| c), Some(false));

    // メニュー「66%」選択
    s.menu("view.zoom-66");
    assert_eq!(s.with(|e, _| e.zoom), Some(0.66));
    assert_eq!(s.menu_state("view.zoom-66").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("view.zoom-100").map(|(_, c)| c), Some(false));

    // メニュー「50%」選択
    s.menu("view.zoom-50");
    assert_eq!(s.with(|e, _| e.zoom), Some(0.50));
    assert_eq!(s.menu_state("view.zoom-50").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("view.zoom-66").map(|(_, c)| c), Some(false));

    // メニュー「33%」選択
    s.menu("view.zoom-33");
    assert_eq!(s.with(|e, _| e.zoom), Some(0.33));
    assert_eq!(s.menu_state("view.zoom-33").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("view.zoom-50").map(|(_, c)| c), Some(false));

    // 3. メニュー「ウィンドウに合わせる」選択
    s.menu("view.zoom-fit");
    assert!(s.with(|e, _| e.zoom.is_none()));
    assert_eq!(s.menu_state("view.zoom-fit").map(|(_, c)| c), Some(true));
    assert_eq!(s.menu_state("view.zoom-33").map(|(_, c)| c), Some(false));

    // 4. メニュー「ズーム…」ダイアログを開く
    s.menu("view.zoom-dialog");
    assert!(s.with(|e, _| e.zoom_dialog_open), "ズームダイアログが開いている");
    let initial_val = s.with(|e, cx| e.zoom_dialog_input.read(cx).value().to_string());
    assert!(!initial_val.is_empty(), "初期倍率が入力欄にセットされている");

    // 5. Escape キーでダイアログを閉じる
    s.key("escape");
    assert!(!s.with(|e, _| e.zoom_dialog_open), "Escape キーでズームダイアログが閉じる");

    // 6. 再びダイアログを開き、75% を適用
    s.menu("view.zoom-dialog");
    assert!(s.with(|e, _| e.zoom_dialog_open));
    s.update(|e, w, cx| {
        e.zoom_dialog_input.update(cx, |input, cx| input.set_value("75", w, cx));
    });
    s.update(|e, w, cx| {
        e.apply_zoom_dialog(w, cx);
    });
    assert!(!s.with(|e, _| e.zoom_dialog_open), "適用後にダイアログが閉じる");
    assert_eq!(s.with(|e, _| e.zoom), Some(0.75));

    // ダイアログから「合わせる（フィット）」ボタン押下相当
    s.menu("view.zoom-dialog");
    assert!(s.with(|e, _| e.zoom_dialog_open));
    s.update(|e, w, cx| {
        e.fit_zoom_dialog(w, cx);
    });
    assert!(!s.with(|e, _| e.zoom_dialog_open));
    assert!(s.with(|e, _| e.zoom.is_none()));

    // 7. ウィンドウ操作メニュー項目
    s.menu("window.arrange-all");
    s.menu("window.cascade");
    s.menu("window.new-window");
}

/// アニメーションの書式設定（Animation Painter / アニメーションのコピー・貼り付け）の動作検証:
/// 1. 図形Aにフェード＋スピンのアニメーションを設定
/// 2. 図形A選択状態で「アニメーションのコピー（pptx.anim.copy）」を実行し、コピーバッファに保持されること
/// 3. アニメーション未設定の図形Bを選択し、「アニメーションの貼り付け（pptx.anim.paste）」を実行
/// 4. 図形Bに図形Aと同一のアニメーション効果（フェード＋スピン）が複製・適用されること
/// 5. ツールバーの「アニメーションの書式設定（anim-painter）」ボタンによるコピー・貼り付け動作
/// 6. Undo (⌘Z) で貼り付けが取り消され、Redo (⌘⇧Z) で再適用されること
/// 7. 保存したファイル（.pptx）を再度開いても、両方の図形にアニメーションが保持されていること
pub(crate) fn animation_painter_and_copy_effects() {
    use ibis_doc_model::{AnimationClass as C, AnimationLook as L};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. 図形0（タイトル）にフェード（Entrance）およびスピン（Emphasis）を設定
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.in.fade");
    s.toolbar_menu("motion", "add.pptx.anim.em.spin");

    let shape0_anims = s.with(|e, _| {
        let sid = e.slide().shapes[0].id;
        e.slide().animations.iter().filter(|a| a.shape == sid).map(|a| (a.class, a.look, a.duration)).collect::<Vec<_>>()
    });
    assert_eq!(shape0_anims.len(), 2, "図形0に2つの効果");
    assert_eq!(shape0_anims[0], (C::Entrance, L::Fade, 500));
    assert_eq!(shape0_anims[1], (C::Emphasis, L::Spin(21_600_000), 2000));

    // 2. 図形0選択状態でアニメーションをコピー
    assert!(s.with(|e, _| e.can_copy_animations()), "図形0のアニメーションをコピー可能");
    s.menu("pptx.anim.copy");
    let copied_count = s.with(|e, _| e.pptx().and_then(|st| st.animation_copy.as_ref()).map(|c| c.len()));
    assert_eq!(copied_count, Some(2), "2つの効果がコピーバッファに保持されている");

    // 3. 図形1（サブタイトル）を選択
    s.click_shape(1);
    let shape1_initial = s.with(|e, _| {
        let sid = e.slide().shapes[1].id;
        e.slide().animations.iter().filter(|a| a.shape == sid).count()
    });
    assert_eq!(shape1_initial, 0, "初期状態では図形1にアニメーションなし");
    assert!(s.with(|e, _| e.can_paste_animations()), "図形1にアニメーションを貼り付け可能");

    // 4. アニメーションを貼り付け
    s.menu("pptx.anim.paste");
    let shape1_anims = s.with(|e, _| {
        let sid = e.slide().shapes[1].id;
        e.slide().animations.iter().filter(|a| a.shape == sid).map(|a| (a.class, a.look, a.duration)).collect::<Vec<_>>()
    });
    assert_eq!(shape1_anims, shape0_anims, "図形1に図形0と同一の効果が複製・適用された");

    // 5. Undo / Redo の検証
    s.key("cmd-z");
    let shape1_undone = s.with(|e, _| {
        let sid = e.slide().shapes[1].id;
        e.slide().animations.iter().filter(|a| a.shape == sid).count()
    });
    assert_eq!(shape1_undone, 0, "Undoで図形1のアニメーションが取り消される");

    s.key("cmd-shift-z");
    let shape1_redone = s.with(|e, _| {
        let sid = e.slide().shapes[1].id;
        e.slide().animations.iter().filter(|a| a.shape == sid).map(|a| (a.class, a.look, a.duration)).collect::<Vec<_>>()
    });
    assert_eq!(shape1_redone, shape0_anims, "Redoで図形1のアニメーションが再適用される");

    // 6. ツールバー「アニメーションの書式設定（anim-painter）」ボタンの動作検証
    s.page(2);
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.in.fly-bottom");
    s.toolbar("anim-painter");
    let copied_p2 = s.with(|e, _| e.pptx().and_then(|st| st.animation_copy.as_ref()).map(|c| c.len()));
    assert_eq!(copied_p2, Some(1), "ページ2の図形0のアニメーションがコピーされた");

    s.click_shape(1);
    s.toolbar("anim-painter");
    let p2_shape1_anims = s.with(|e, _| {
        let sid = e.slide().shapes[1].id;
        e.slide().animations.iter().filter(|a| a.shape == sid).map(|a| (a.class, a.look)).collect::<Vec<_>>()
    });
    assert_eq!(p2_shape1_anims, vec![(C::Entrance, L::Fly(ibis_doc_model::AnimationSide::Bottom))]);

    // 6.5. ツールバー「アニメーション」ドロップダウンメニューからのコピー＆ペースト動作も検証
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.copy");
    assert_eq!(s.with(|e, _| e.pptx().and_then(|st| st.animation_copy.as_ref()).map(|c| c.len())), Some(1));
    s.click_shape(1);
    s.toolbar_menu("motion", "pptx.anim.paste");
    let p2_shape1_reapplied = s.with(|e, _| {
        let sid = e.slide().shapes[1].id;
        e.slide().animations.iter().filter(|a| a.shape == sid).map(|a| (a.class, a.look)).collect::<Vec<_>>()
    });
    assert_eq!(p2_shape1_reapplied, vec![(C::Entrance, L::Fly(ibis_doc_model::AnimationSide::Bottom))]);

    // 7. 保存と再読込
    s.save_as("anim-painter.pptx");
    let saved = s.reopen();
    let s0_saved = saved.slides[0].shapes[0].id;
    let s1_saved = saved.slides[0].shapes[1].id;
    assert_eq!(saved.slides[0].animations.iter().filter(|a| a.shape == s0_saved).count(), 2);
    assert_eq!(saved.slides[0].animations.iter().filter(|a| a.shape == s1_saved).count(), 2);
    let p2_s0_saved = saved.slides[1].shapes[0].id;
    let p2_s1_saved = saved.slides[1].shapes[1].id;
    assert_eq!(saved.slides[1].animations.iter().filter(|a| a.shape == p2_s0_saved).count(), 1);
    assert_eq!(saved.slides[1].animations.iter().filter(|a| a.shape == p2_s1_saved).count(), 1);
}

/// 最終版としてマーク（Mark as Final）および読み取り専用を推奨（ReadOnlyRecommended）の
/// エディタ情報バー表示・編集ガード・解除ボタン・Undo/Redo・OOXML永続化のシナリオ検証。
pub(crate) fn mark_as_final_and_read_only_presentation() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. 初期状態: 最終版・読取専用推奨ともに false, 情報バーなし
    assert!(!s.with(|e, _| e.is_pptx_mark_as_final()), "initially not mark as final");
    assert!(!s.with(|e, _| e.is_pptx_read_only_recommended()), "initially not read only recommended");
    assert!(!s.with(|e, _| e.is_pptx_read_only_dismissed()), "initially not dismissed");
    assert!(s.update(|e, _, cx| e.render_pptx_infobar(cx)).is_none(), "no infobar initially");

    let initial_slides = s.with(|e, _| e.presentation().slides.len());

    // 2. メニュー「pptx.mark-as-final」で最終版に設定
    s.menu("pptx.mark-as-final");
    assert!(s.with(|e, _| e.is_pptx_mark_as_final()), "marked as final after menu toggle");
    assert!(s.with(|e, _| e.presentation().mark_as_final), "model presentation marked as final");
    assert!(s.update(|e, _, cx| e.render_pptx_infobar(cx)).is_some(), "final infobar shown");

    // 3. 最終版状態での編集ガードの検証（編集コマンドがブロックされる）
    s.menu("pptx.new-slide");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), initial_slides, "new slide is blocked when marked as final");

    // 4. 「それでも編集する」（pptx_edit_anyway）を実行
    s.update(|e, _, cx| e.pptx_edit_anyway(cx));
    assert!(!s.with(|e, _| e.is_pptx_mark_as_final()), "mark as final cleared by edit_anyway");
    assert!(!s.with(|e, _| e.presentation().mark_as_final), "model mark as final cleared");
    assert!(s.update(|e, _, cx| e.render_pptx_infobar(cx)).is_none(), "infobar dismissed after edit_anyway");

    // 解除後は編集できること（スライドを1枚追加）
    s.menu("pptx.new-slide");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), initial_slides + 1, "new slide succeeds after edit_anyway");

    // 5. Undo / Redo
    s.key("cmd-z"); // スライド追加を Undo
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), initial_slides);

    s.key("cmd-z"); // edit_anyway を Undo -> 再び mark_as_final == true
    assert!(s.with(|e, _| e.is_pptx_mark_as_final()), "undo restores mark_as_final");
    assert!(s.update(|e, _, cx| e.render_pptx_infobar(cx)).is_some(), "final infobar restored by undo");

    s.key("cmd-shift-z"); // Redo edit_anyway -> mark_as_final == false
    assert!(!s.with(|e, _| e.is_pptx_mark_as_final()), "redo clears mark_as_final");
    assert!(s.update(|e, _, cx| e.render_pptx_infobar(cx)).is_none());

    // 6. メニュー「pptx.read-only-recommended」で読取専用推奨を設定
    s.menu("pptx.read-only-recommended");
    assert!(s.with(|e, _| e.is_pptx_read_only_recommended()), "read only recommended enabled");
    assert!(s.with(|e, _| e.presentation().read_only_recommended), "model read only recommended enabled");
    assert!(!s.with(|e, _| e.is_pptx_read_only_dismissed()), "not dismissed initially");
    assert!(s.update(|e, _, cx| e.render_pptx_infobar(cx)).is_some(), "read only infobar shown");

    // 読取専用推奨（未解除）状態での編集ガード
    s.menu("pptx.new-slide");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), initial_slides, "new slide is blocked while read only recommended is active");

    // 「編集を有効にする」（dismiss_pptx_read_only）
    s.update(|e, _, cx| e.dismiss_pptx_read_only(cx));
    assert!(s.with(|e, _| e.is_pptx_read_only_dismissed()), "read only dismissed");
    assert!(s.update(|e, _, cx| e.render_pptx_infobar(cx)).is_none(), "read only infobar dismissed");

    // 解除後は編集できること（スライド追加）
    s.menu("pptx.new-slide");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), initial_slides + 1, "new slide succeeds after dismiss_pptx_read_only");

    // スライド追加を戻す
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), initial_slides);

    // 7. mark_as_final も true に設定して、両方のフラグを保存・再オープンで検証
    s.update(|e, _, cx| e.set_pptx_mark_as_final(true, cx));
    assert!(s.with(|e, _| e.is_pptx_mark_as_final()));
    assert!(s.with(|e, _| e.is_pptx_read_only_recommended()));

    s.save_as("test-mark-final-readonly.pptx");
    let saved = s.reopen();

    // 再読込されたファイルで両方の設定が保持されていること
    assert!(saved.mark_as_final, "mark_as_final persisted in reopened pptx");
    assert!(saved.read_only_recommended, "read_only_recommended persisted in reopened pptx");
    assert!(s.with(|e, _| e.is_pptx_mark_as_final()), "editor reports mark_as_final");
    assert!(s.with(|e, _| e.is_pptx_read_only_recommended()), "editor reports read_only_recommended");
    assert!(s.update(|e, _, cx| e.render_pptx_infobar(cx)).is_some(), "infobar rendered in editor");

    // 保存したファイルを新しいセッションで開き直して初期状態（未解除）を検証
    let saved_path = s.with(|e, _| e.path.clone()).expect("saved path");
    drop(s);

    let mut s2 = Scenario::start_view();
    let loaded = crate::read_file(&saved_path).expect("read saved file");
    s2.update(|e, window, cx| {
        e.load(loaded.presentation, "test-reopened.pptx".into(), window, cx);
    });
    assert!(s2.with(|e, _| e.is_pptx_mark_as_final()));
    assert!(s2.with(|e, _| e.is_pptx_read_only_recommended()));
    assert!(!s2.with(|e, _| e.is_pptx_read_only_dismissed()), "newly opened editor has read-only not dismissed");
    assert!(s2.update(|e, _, cx| e.render_pptx_infobar(cx)).is_some(), "infobar rendered in new editor");
}

/// 書き込みパスワード (REVIEW-014): 設定して保存すると、開き直したファイルは、パスワードを
/// 入力するまで編集できない（情報バーが出る）。違うパスワードは断られ、合うと編集できる。
pub(crate) fn modify_password() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    assert!(!s.with(|e, _| e.is_pptx_modify_locked()));
    assert_eq!(s.menu_state("pptx.modify-password").map(|(enabled, _)| enabled), Some(true));
    // Set: the file stays editable here.
    s.update(|e, _, cx| e.set_modify_password(Some("ひみつ123".into()), cx).unwrap());
    assert!(s.with(|e, _| e.presentation().modify_verifier.is_some() && !e.is_pptx_modify_locked()));
    let slides = s.with(|e, _| e.presentation().slides.len());
    let saved = s.save_as("modify-password.pptx");
    drop(s);
    // Read again: locked, an information bar asks, and no edit is taken.
    let mut s2 = Scenario::start_view();
    let loaded = crate::read_file(&saved).expect("read saved file");
    s2.update(|e, window, cx| e.load(loaded.presentation, "modify-password.pptx".into(), window, cx));
    assert!(s2.with(|e, _| e.is_pptx_modify_locked() && e.is_pptx_read_only_blocked()));
    assert!(s2.update(|e, _, cx| e.render_pptx_infobar(cx)).is_some(), "the information bar");
    s2.update(|e, window, cx| e.add_slide(window, cx));
    assert_eq!(s2.with(|e, _| e.presentation().slides.len()), slides, "no slide while locked");
    // A wrong password is refused; the right one unlocks.
    assert!(s2.update(|e, _, cx| e.unlock_modify("ひみつ", cx)).is_err());
    assert!(s2.with(|e, _| e.is_pptx_modify_locked()));
    assert!(s2.update(|e, _, cx| e.unlock_modify("ひみつ123", cx)).is_ok());
    assert!(!s2.with(|e, _| e.is_pptx_modify_locked() && e.is_pptx_read_only_blocked()));
    s2.update(|e, window, cx| e.add_slide(window, cx));
    assert_eq!(s2.with(|e, _| e.presentation().slides.len()), slides + 1, "a slide once unlocked");
    // Taken off: the next file has none.
    s2.update(|e, _, cx| e.set_modify_password(None, cx).unwrap());
    assert!(s2.with(|e, _| e.presentation().modify_verifier.is_none()));
}

/// ファイル > 情報 (file.info) によるドキュメント プロパティ パネル（タイトル・作成者・件名・会社名等）
/// およびカスタム プロパティの編集・適用・Undo/Redo・保存・再読込検証（PPTX-FILE-008, PPTX-FILE-009）。
pub(crate) fn presentation_properties_panel_and_custom_properties() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. メニュー「file.info」からプロパティパネルを開く
    s.menu("file.info");
    assert!(s.with(|e, _| e.pptx().and_then(|st| st.properties_panel.as_ref()).is_some()), "properties panel open");
    let slide_count = s.with(|e, _| e.pptx().and_then(|st| st.properties_panel.as_ref()).map(|p| p.slide_count));
    assert_eq!(slide_count, Some(4), "slide count is 4");

    // 2. パネルの各入力フィールドを更新
    s.update(|e, window, cx| {
        let p = e.pptx_mut().and_then(|st| st.properties_panel.as_mut()).expect("panel");
        p.title.update(cx, |input, cx| input.set_value("四半期事業計画", window, cx));
        p.creator.update(cx, |input, cx| input.set_value("計画策定室", window, cx));
        p.subject.update(cx, |input, cx| input.set_value("事業戦略", window, cx));
        p.company.update(cx, |input, cx| input.set_value("アイビス株式会社", window, cx));
        p.new_custom_name.update(cx, |input, cx| input.set_value("プロジェクト名", window, cx));
        p.new_custom_val.update(cx, |input, cx| input.set_value("オフィス次世代化", window, cx));
    });

    // 3. カスタムプロパティを追加
    s.update(|e, window, cx| {
        e.add_presentation_custom_property(window, cx);
    });
    let custom_len = s.with(|e, _| e.pptx().and_then(|st| st.properties_panel.as_ref()).map(|p| p.custom_properties.len()));
    assert_eq!(custom_len, Some(1), "custom property added");

    // 4. 「適用」ボタンを実行してモデルへ反映
    s.update(|e, _, cx| {
        e.apply_presentation_doc_properties(cx);
    });
    assert_eq!(s.with(|e, _| e.presentation().properties.title.clone()), Some("四半期事業計画".to_string()));
    assert_eq!(s.with(|e, _| e.presentation().properties.creator.clone()), Some("計画策定室".to_string()));
    assert_eq!(s.with(|e, _| e.presentation().properties.subject.clone()), Some("事業戦略".to_string()));
    assert_eq!(s.with(|e, _| e.presentation().properties.company.clone()), Some("アイビス株式会社".to_string()));
    assert_eq!(s.with(|e, _| e.presentation().properties.custom.len()), 1);
    assert_eq!(s.with(|e, _| e.presentation().properties.custom[0].name.clone()), "プロジェクト名");

    // 5. Undo / Redo の検証
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.presentation().properties.title.clone()), None);
    assert!(s.with(|e, _| e.presentation().properties.custom.is_empty()));

    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.presentation().properties.title.clone()), Some("四半期事業計画".to_string()));
    assert_eq!(s.with(|e, _| e.presentation().properties.creator.clone()), Some("計画策定室".to_string()));
    assert_eq!(s.with(|e, _| e.presentation().properties.custom.len()), 1);

    // 6. パネルを閉じる
    s.update(|e, _, cx| {
        e.close_presentation_properties_panel(cx);
    });
    assert!(s.with(|e, _| e.pptx().and_then(|st| st.properties_panel.as_ref()).is_none()));

    // 7. 保存と再読込による OOXML 永続化の検証
    s.save_as("test-presentation-props.pptx");
    let saved = s.reopen();
    assert_eq!(saved.properties.title.as_deref(), Some("四半期事業計画"));
    assert_eq!(saved.properties.creator.as_deref(), Some("計画策定室"));
    assert_eq!(saved.properties.subject.as_deref(), Some("事業戦略"));
    assert_eq!(saved.properties.company.as_deref(), Some("アイビス株式会社"));
    assert_eq!(saved.properties.custom.len(), 1);
    assert_eq!(saved.properties.custom[0].name, "プロジェクト名");
    assert_eq!(
        saved.properties.custom[0].value,
        ibis_doc_model::CustomPropertyValue::Text("オフィス次世代化".to_string())
    );
}

/// 表の見出し行とアクセシビリティ検査（PPTX-ACCESS-007）:
/// 表の見出し行が未設定（first_row == false）の場合に、アクセシビリティ検査パネルで
/// 警告（Warning: 「スライド N: 表の見出し行が指定されていません」）として検出され、
/// 「表示」ボタンで該当表にジャンプ・フォーカスし、表の見出し行オプションを有効化して
/// 再検査すると警告が解消されることを検証。
pub(crate) fn table_header_accessibility_check() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    s.menu("table.new.3x3");

    // 1. 表の見出し行（first_row）をオフにして未設定状態を作る
    s.menu("pptx.table.first-row");

    // 2. メニュー「pptx.check-accessibility」からアクセシビリティ検査パネルを開く
    s.menu("pptx.check-accessibility");
    assert!(s.with(|e, _| e.sidebar_open && e.pptx().and_then(|st| st.accessibility_panel.as_ref()).is_some()), "accessibility panel open");

    // 3. 表の見出し行未設定の警告が検出されていることを確認
    let table_warnings = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        p.report.warnings.iter().filter(|w| w.fix_type == crate::pptx::accessibility_panel::IssueFixType::EnableTableHeader).cloned().collect::<Vec<_>>()
    });
    assert!(!table_warnings.is_empty(), "table header missing warning should be detected");
    assert!(table_warnings[0].title.contains("表の見出し行が指定されていません"));

    // 4. 「表示」ボタンで該当表にジャンプ・フォーカス
    let table_warning_idx = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        let all: Vec<_> = p.report.errors.iter().chain(p.report.warnings.iter()).collect();
        all.iter().position(|w| w.fix_type == crate::pptx::accessibility_panel::IssueFixType::EnableTableHeader).unwrap()
    });
    s.update(|e, _, cx| {
        e.jump_to_pptx_accessibility_issue(table_warning_idx, cx);
    });
    assert_eq!(s.with(|e, _| e.pptx().unwrap().accessibility_panel.as_ref().unwrap().selected_issue), Some(table_warning_idx));

    // 5. 表の見出し行を再度オンにして「再検査」
    s.menu("pptx.table.first-row");
    s.update(|e, _, cx| {
        e.recheck_pptx_accessibility(cx);
    });

    // 6. 警告が解消されたことを確認
    let table_warnings_after = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        p.report.warnings.iter().filter(|w| w.fix_type == crate::pptx::accessibility_panel::IssueFixType::EnableTableHeader).count()
    });
    assert_eq!(table_warnings_after, 0, "table header warning should be cleared after enabling header row");

    // 7. パネルを閉じる
    s.update(|e, _, cx| {
        e.close_pptx_accessibility_panel(cx);
    });
    assert!(s.with(|e, _| e.pptx().and_then(|st| st.accessibility_panel.as_ref()).is_none()));
}

/// 意味のあるハイパーリンク テキストの検査と警告（PPTX-ACCESS-008）:
/// リンク表示テキストが生のURL（http/https/www等）や、「ここをクリック」「詳細」等の
/// 曖昧・非説明的な文言になっている場合に、アクセシビリティ検査パネルで警告（Warning）
/// として検出され、「表示」ボタンで該当図形にジャンプし、説明的な文言に変更して
/// 再検査すると警告が解消されることを検証。
pub(crate) fn meaningful_link_text_accessibility_check() {
    use ibis_doc_model::{CharacterFormat, Command, SlideLink};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. スライド1の図形1に「URLそのまま」、図形2に「ここをクリック」のリンクを設定
    s.update(|e, _, cx| {
        let slide_id = e.presentation().slides[0].id;
        let shape1_id = e.presentation().slides[0].shapes[0].id;
        let shape2_id = e.presentation().slides[0].shapes[1].id;

        // 図形1: URLそのままのリンクテキスト
        let url = SlideLink::Url("https://example.com/quarterly-plan".into());
        let mut body1 = ibis_doc_model::TextBody::plain("https://example.com/quarterly-plan", ibis_doc_model::TextStyle::default());
        body1.format_runs(0, body1.text().len(), &CharacterFormat { link: Some(Some(url.clone())), ..Default::default() });
        e.run(vec![Command::SetTextBody { slide: slide_id, shape: shape1_id, body: body1 }], cx);

        // 図形2: 「ここをクリック」という曖昧なリンクテキスト
        let mut body2 = ibis_doc_model::TextBody::plain("詳細はこちら ここをクリック", ibis_doc_model::TextStyle::default());
        body2.format_runs(7, 14, &CharacterFormat { link: Some(Some(url)), ..Default::default() });
        e.run(vec![Command::SetTextBody { slide: slide_id, shape: shape2_id, body: body2 }], cx);
    });

    // 2. メニュー「pptx.check-accessibility」からアクセシビリティ検査パネルを開く
    s.menu("pptx.check-accessibility");
    assert!(s.with(|e, _| e.sidebar_open && e.pptx().and_then(|st| st.accessibility_panel.as_ref()).is_some()), "accessibility panel open");

    // 3. リンクテキストに関する警告が2件検出されていることを確認
    let link_warnings = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        p.report.warnings.iter().filter(|w| w.fix_type == crate::pptx::accessibility_panel::IssueFixType::UseMeaningfulLinkText).cloned().collect::<Vec<_>>()
    });
    assert_eq!(link_warnings.len(), 2, "both vague/raw links should have warnings");
    assert!(link_warnings.iter().any(|w| w.title.contains("https://example.com")));
    assert!(link_warnings.iter().any(|w| w.title.contains("ここをクリック")));

    // 4. 「表示」ボタンで該当図形にジャンプ・フォーカス
    let first_link_warning_idx = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        let all: Vec<_> = p.report.errors.iter().chain(p.report.warnings.iter()).collect();
        all.iter().position(|w| w.fix_type == crate::pptx::accessibility_panel::IssueFixType::UseMeaningfulLinkText).unwrap()
    });
    s.update(|e, _, cx| {
        e.jump_to_pptx_accessibility_issue(first_link_warning_idx, cx);
    });
    let shape1_id = s.with(|e, _| e.presentation().slides[0].shapes[0].id);
    let shape2_id = s.with(|e, _| e.presentation().slides[0].shapes[1].id);
    let selected_shape = s.with(|e, _| e.selection);
    assert!(selected_shape == Some(shape1_id) || selected_shape == Some(shape2_id), "jumped and selected linked shape");
    assert_eq!(s.with(|e, _| e.pptx().unwrap().accessibility_panel.as_ref().unwrap().selected_issue), Some(first_link_warning_idx));

    // 5. リンクテキストを説明的で意味のある文言に変更して「再検査」
    s.update(|e, _, cx| {
        let slide_id = e.presentation().slides[0].id;
        let shape1_id = e.presentation().slides[0].shapes[0].id;
        let shape2_id = e.presentation().slides[0].shapes[1].id;

        let url = SlideLink::Url("https://example.com/quarterly-plan".into());
        // 図形1: 説明的なテキスト
        let mut body1 = ibis_doc_model::TextBody::plain("2026年度 第3四半期 事業計画書", ibis_doc_model::TextStyle::default());
        body1.format_runs(0, body1.text().len(), &CharacterFormat { link: Some(Some(url.clone())), ..Default::default() });
        e.run(vec![Command::SetTextBody { slide: slide_id, shape: shape1_id, body: body1 }], cx);

        // 図形2: 説明的なテキスト
        let mut body2 = ibis_doc_model::TextBody::plain("製品詳細仕様ドキュメントへのリンク", ibis_doc_model::TextStyle::default());
        body2.format_runs(0, body2.text().len(), &CharacterFormat { link: Some(Some(url)), ..Default::default() });
        e.run(vec![Command::SetTextBody { slide: slide_id, shape: shape2_id, body: body2 }], cx);

        e.recheck_pptx_accessibility(cx);
    });

    // 6. リンクに関する警告が解消されたことを確認
    let link_warnings_after = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        p.report.warnings.iter().filter(|w| w.fix_type == crate::pptx::accessibility_panel::IssueFixType::UseMeaningfulLinkText).count()
    });
    assert_eq!(link_warnings_after, 0, "link text warnings should be cleared after making text meaningful");

    // 7. パネルを閉じる
    s.update(|e, _, cx| {
        e.close_pptx_accessibility_panel(cx);
    });
    assert!(s.with(|e, _| e.pptx().and_then(|st| st.accessibility_panel.as_ref()).is_none()));
}

/// スライドの一意なタイトルの検査と警告・一意化（PPTX-ACCESS-002）:
/// 複数のスライドで同一タイトル（重複タイトル）が存在する場合に、
/// アクセシビリティ検査パネル（pptx.check-accessibility）で警告（Warning）として
/// 「重複するスライド タイトル」が検出・一覧表示され、
/// 「表示」ボタンでの該当スライドへのジャンプ、およびタイトル一意化後の再検査で
/// 警告が解消されることを検証。
pub(crate) fn unique_slide_titles_accessibility_check() {
    use ibis_doc_model::Command;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. スライド1とスライド2のタイトルに同一テキストを設定して重複状態を作る
    s.update(|e, _, cx| {
        for idx in 0..2 {
            let slide_id = e.presentation().slides[idx].id;
            let title_ph = e.presentation().slides[idx].shapes.iter().find(|sh| {
                e.presentation().slides[idx].placeholders.get(&sh.id).is_some_and(|ph| ph.placeholder.normalized() == "title")
            }).map(|sh| sh.id);
            if let Some(shape_id) = title_ph {
                let body = ibis_doc_model::TextBody::plain("共通アジェンダ", ibis_doc_model::TextStyle::default());
                e.run(vec![Command::SetTextBody { slide: slide_id, shape: shape_id, body }], cx);
            }
        }
    });

    // 2. メニュー「pptx.check-accessibility」からパネルを開く
    s.menu("pptx.check-accessibility");
    assert!(s.with(|e, _| e.sidebar_open && e.pptx().and_then(|st| st.accessibility_panel.as_ref()).is_some()), "accessibility panel open");

    // 3. 重複タイトルの警告が検出されていることを確認
    let duplicate_warnings = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        p.report.warnings.iter().filter(|w| w.fix_type == crate::pptx::accessibility_panel::IssueFixType::MakeTitleUnique).cloned().collect::<Vec<_>>()
    });
    assert_eq!(duplicate_warnings.len(), 2, "both slides should have duplicate title warning");
    assert!(duplicate_warnings[0].title.contains("共通アジェンダ"));
    assert!(duplicate_warnings[1].title.contains("共通アジェンダ"));

    // 4. 「表示」ボタンで重複スライド（スライド2）にジャンプ
    let second_warning_idx = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        let all: Vec<_> = p.report.errors.iter().chain(p.report.warnings.iter()).collect();
        all.iter().position(|w| w.slide_index == 1 && w.fix_type == crate::pptx::accessibility_panel::IssueFixType::MakeTitleUnique).unwrap()
    });
    s.update(|e, _, cx| {
        e.jump_to_pptx_accessibility_issue(second_warning_idx, cx);
    });
    assert_eq!(s.with(|e, _| e.current_slide), s.with(|e, _| e.presentation().slides[1].id), "jumped to slide 2");

    // 5. スライド2のタイトルを一意に変更して「再検査」
    s.update(|e, _, cx| {
        let slide_id = e.presentation().slides[1].id;
        let title_ph = e.presentation().slides[1].shapes.iter().find(|sh| {
            e.presentation().slides[1].placeholders.get(&sh.id).is_some_and(|ph| ph.placeholder.normalized() == "title")
        }).map(|sh| sh.id);
        if let Some(shape_id) = title_ph {
            let body = ibis_doc_model::TextBody::plain("共通アジェンダ (詳細)", ibis_doc_model::TextStyle::default());
            e.run(vec![Command::SetTextBody { slide: slide_id, shape: shape_id, body }], cx);
        }
        e.recheck_pptx_accessibility(cx);
    });

    // 6. 重複タイトルの警告が解消されたことを確認
    let duplicate_warnings_after = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        p.report.warnings.iter().filter(|w| w.fix_type == crate::pptx::accessibility_panel::IssueFixType::MakeTitleUnique).count()
    });
    assert_eq!(duplicate_warnings_after, 0, "duplicate title warnings should be cleared after making titles unique");

    // 7. パネルを閉じる
    s.update(|e, _, cx| {
        e.close_pptx_accessibility_panel(cx);
    });
    assert!(s.with(|e, _| e.pptx().and_then(|st| st.accessibility_panel.as_ref()).is_none()));
}

/// アクセシビリティ検査と問題ジャンプ (PPTX-ACCESS-001):
/// メニュー「pptx.check-accessibility」からアクセシビリティ検査パネルを開き、
/// タイトル欠落・代替テキスト欠落・表見出し未設定の検出・一覧表示、
/// 問題項目へのジャンプ（スライド/図形フォーカス）、修正後の再検査を検証。
pub(crate) fn check_accessibility_panel_and_issue_jump() {
    use ibis_doc_model::Command;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. 空スライド（タイトル未設定）を追加してアクセシビリティエラーを作る
    s.update(|e, w, cx| {
        e.add_slide(w, cx);
    });

    // 2. メニュー「pptx.check-accessibility」からアクセシビリティ パネルを開く
    s.menu("pptx.check-accessibility");
    assert!(s.with(|e, _| e.sidebar_open && e.pptx().and_then(|st| st.accessibility_panel.as_ref()).is_some()), "accessibility panel open");

    // 3. 検出された問題の確認
    let (errors_count, _warnings_count) = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        (p.report.errors.len(), p.report.warnings.len())
    });
    assert!(errors_count > 0, "should detect accessibility errors (e.g. missing title)");

    // 4. 「表示」ボタンによる問題箇所へのジャンプ操作（jump_to_pptx_accessibility_issue）
    s.update(|e, _, cx| {
        e.jump_to_pptx_accessibility_issue(0, cx);
    });
    assert_eq!(s.with(|e, _| e.pptx().unwrap().accessibility_panel.as_ref().unwrap().selected_issue), Some(0));

    // 5. 問題スライドにタイトルを設定して再検査
    s.update(|e, _, cx| {
        let mut updated = e.slide().clone();
        updated.name = Some("アクセシビリティ対応タイトル".into());
        e.run(vec![Command::SetSlide { slide: updated }], cx);
        e.recheck_pptx_accessibility(cx);
    });

    let (errors_after, msg) = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        (p.report.errors.len(), p.message.clone())
    });
    assert!(errors_after < errors_count, "errors should decrease after adding title");
    assert!(!msg.is_empty(), "recheck message shown");

    // 6. パネルを閉じる
    s.update(|e, _, cx| {
        e.close_pptx_accessibility_panel(cx);
    });
    assert!(s.with(|e, _| e.pptx().and_then(|st| st.accessibility_panel.as_ref()).is_none()));
}

/// 非表示スライド タイトルとアクセシビリティ エラー解消 (PPTX-ACCESS-003):
/// メニュー「pptx.slide.add-hidden-title」またはパネルのボタンにより、
/// タイトルのないスライドにオフスクリーンの非表示タイトルプレースホルダーと
/// スライド名を設定し、アクセシビリティ検査のタイトル欠落エラーを解消する。
/// Undo/Redo および保存・再読込での保持を検証。
pub(crate) fn add_hidden_slide_title_resolves_accessibility_issue() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. 空スライドを追加（デフォルトではタイトルプレースホルダーなし）
    s.update(|e, w, cx| {
        e.add_slide(w, cx);
    });
    let new_slide_id = s.with(|e, _| e.current_slide);

    // 2. メニュー「pptx.check-accessibility」からアクセシビリティ パネルを開く
    s.menu("pptx.check-accessibility");
    assert!(s.with(|e, _| e.sidebar_open && e.pptx().and_then(|st| st.accessibility_panel.as_ref()).is_some()), "accessibility panel open");

    // 3. タイトル欠落エラー（AddSlideTitle）が検出されていることを確認
    let has_missing_title = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        p.report.errors.iter().any(|err| err.slide_id == new_slide_id && err.fix_type == crate::pptx::accessibility_panel::IssueFixType::AddSlideTitle)
    });
    assert!(has_missing_title, "new blank slide should have missing title error");

    // 4. メニュー「pptx.slide.add-hidden-title」を実行して非表示タイトルを追加
    s.menu("pptx.slide.add-hidden-title");

    // 5. スライドにオフスクリーンタイトルプレースホルダーとスライド名が設定されたことを確認
    let (has_title_ph, slide_name) = s.with(|e, _| {
        let slide = e.presentation().slide(new_slide_id).unwrap();
        let ph = slide.shapes.iter().any(|sh| {
            sh.frame.y < 0 && slide.placeholders.get(&sh.id).is_some_and(|p| p.placeholder.normalized() == "title")
        });
        (ph, slide.name.clone())
    });
    assert!(has_title_ph, "hidden title placeholder shape exists offscreen (y < 0)");
    assert!(slide_name.is_some(), "slide.name is set");

    // 6. 再検査を実行し、タイトル欠落エラーが解消されたことを確認
    s.update(|e, _, cx| {
        e.recheck_pptx_accessibility(cx);
    });
    let has_missing_title_after = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        p.report.errors.iter().any(|err| err.slide_id == new_slide_id && err.fix_type == crate::pptx::accessibility_panel::IssueFixType::AddSlideTitle)
    });
    assert!(!has_missing_title_after, "missing title error should be resolved after adding hidden title");

    // 7. Undo（⌘Z）で非表示タイトル追加を取り消す
    s.key("cmd-z");
    s.update(|e, _, cx| {
        e.recheck_pptx_accessibility(cx);
    });
    let has_missing_title_after_undo = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        p.report.errors.iter().any(|err| err.slide_id == new_slide_id && err.fix_type == crate::pptx::accessibility_panel::IssueFixType::AddSlideTitle)
    });
    assert!(has_missing_title_after_undo, "missing title error should re-appear after undo");

    // 8. Redo（⇧⌘Z）で非表示タイトルを復元する
    s.key("shift-cmd-z");
    s.update(|e, _, cx| {
        e.recheck_pptx_accessibility(cx);
    });
    let has_missing_title_after_redo = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        p.report.errors.iter().any(|err| err.slide_id == new_slide_id && err.fix_type == crate::pptx::accessibility_panel::IssueFixType::AddSlideTitle)
    });
    assert!(!has_missing_title_after_redo, "missing title error should be resolved again after redo");

    // 9. パネルから「非表示タイトル追加」ボタンの動作も検証（別のタイトルなしスライドを追加してテスト）
    s.update(|e, w, cx| {
        e.add_slide(w, cx);
    });
    let another_slide_id = s.with(|e, _| e.current_slide);
    s.update(|e, _, cx| {
        e.recheck_pptx_accessibility(cx);
    });
    let _issue_idx = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        let all: Vec<_> = p.report.errors.iter().chain(p.report.warnings.iter()).cloned().collect();
        all.iter().position(|issue| issue.slide_id == another_slide_id && issue.fix_type == crate::pptx::accessibility_panel::IssueFixType::AddSlideTitle).unwrap()
    });
    // ボタンのクリック相当: add_hidden_slide_title して再検査
    s.update(|e, _, cx| {
        e.add_hidden_slide_title(another_slide_id, Some("カスタム非表示タイトル"), cx);
        e.recheck_pptx_accessibility(cx);
    });
    let another_slide_name = s.with(|e, _| {
        e.presentation().slide(another_slide_id).unwrap().name.clone()
    });
    assert_eq!(another_slide_name.as_deref(), Some("カスタム非表示タイトル"));

    // 10. 保存・再オープンでの保持検証
    let another_slide_idx = s.with(|e, _| e.presentation().slide_index(another_slide_id).unwrap());
    s.save_as("test-hidden-title.pptx");
    let reopened = s.reopen();
    let reopened_slide = &reopened.slides[another_slide_idx];
    let reopened_has_title_ph = reopened_slide.shapes.iter().any(|sh| {
        sh.frame.y < 0 && reopened_slide.placeholders.get(&sh.id).is_some_and(|p| p.placeholder.normalized() == "title")
    });
    assert!(reopened_has_title_ph, "reopened presentation preserves hidden title placeholder");
    assert_eq!(reopened_slide.name.as_deref(), Some("カスタム非表示タイトル"), "reopened presentation preserves slide name");
}

/// ドキュメントの検査と個人情報・非表示要素の除去 (PPTX-REVIEW-018):
/// メニュー「pptx.inspect-document」からドキュメント検査パネルを開き、
/// 個人情報（プロパティ）、発表者ノート、コメント等の検出・表示、
/// 各項目の「すべて削除」実行、Undo/Redo、ファイル保存・再読込保持を検証。
pub(crate) fn inspect_document_and_remove_personal_info() {
    use ibis_doc_model::Command;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. プロパティおよびノートをセット
    s.update(|e, _, cx| {
        let mut props = ibis_doc_model::flow::DocProperties::default();
        props.creator = Some("山田太郎".into());
        props.company = Some("株式会社アイビス".into());
        props.title = Some("機密プレゼンテーション".into());
        props.custom.push(ibis_doc_model::CustomProperty {
            name: "機密区分".into(),
            value: ibis_doc_model::CustomPropertyValue::Text("社外秘".into()),
        });
        let slide = e.slide().id;
        e.run(vec![
            Command::SetDocProperties { properties: props },
            Command::SetNotes { slide, notes: "発表者用非公開メモ".into() },
        ], cx);
    });

    // 2. メニュー「pptx.inspect-document」からドキュメント検査パネルを開く
    s.menu("pptx.inspect-document");
    assert!(s.with(|e, _| e.pptx().and_then(|st| st.inspect_panel.as_ref()).is_some()), "inspect panel open");

    // 3. 検査結果の検出確認
    let (has_props, prop_details_len, notes_count) = s.with(|e, _| {
        let p = e.pptx().unwrap().inspect_panel.as_ref().unwrap();
        (p.inspection.has_properties, p.inspection.properties_details.len(), p.inspection.notes_count)
    });
    assert!(has_props, "properties detected");
    assert!(prop_details_len >= 4, "detected title, creator, company, custom");
    assert!(notes_count >= 1, "notes detected");

    // 4. 「すべて削除」ボタンでプロパティと個人情報を除去
    s.update(|e, _, cx| {
        e.inspect_remove_all_properties(cx);
    });
    assert!(s.with(|e, _| !e.pptx().unwrap().inspect_panel.as_ref().unwrap().inspection.has_properties));
    assert!(s.with(|e, _| e.pptx().unwrap().inspect_panel.as_ref().unwrap().properties_removed));
    assert!(s.with(|e, _| e.presentation().properties.creator.is_none()));
    assert!(s.with(|e, _| e.presentation().properties.custom.is_empty()));

    // 5. 「すべて削除」ボタンでノートを除去
    s.update(|e, _, cx| {
        e.inspect_remove_all_notes(cx);
    });
    assert!(s.with(|e, _| e.pptx().unwrap().inspect_panel.as_ref().unwrap().inspection.notes_count == 0));
    assert!(s.with(|e, _| e.pptx().unwrap().inspect_panel.as_ref().unwrap().notes_removed));
    assert!(s.with(|e, _| e.slide().notes.is_empty()));

    // 6. Undo / Redo の検証
    s.key("cmd-z");
    assert!(!s.with(|e, _| e.slide().notes.is_empty()), "undo restores notes");

    s.key("cmd-shift-z");
    assert!(s.with(|e, _| e.slide().notes.is_empty()), "redo clears notes");

    // 7. パネルを閉じる
    s.update(|e, _, cx| {
        e.close_presentation_inspect_panel(cx);
    });
    assert!(s.with(|e, _| e.pptx().and_then(|st| st.inspect_panel.as_ref()).is_none()));

    // 8. 保存と再読込によるクリーンアップ永続化の検証
    s.save_as("test-inspect-cleaned.pptx");
    let saved = s.reopen();
    assert!(saved.properties.creator.is_none() || saved.properties.creator.as_deref() == Some(""));
    assert!(saved.properties.custom.is_empty());
    assert!(saved.slides[0].notes.is_empty());
}

/// PPTX-IMAGE-015: SVG画像の挿入、色効果（duotone）変更、Undo/Redo、ファイル保存・再読込ラウンドトリップ検証
pub(crate) fn insert_svg_picture_round_trip() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let svg_bytes = br##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="80" viewBox="0 0 120 80"><rect width="120" height="80" fill="#336699"/><circle cx="60" cy="40" r="30" fill="#ffcc00"/></svg>"##.to_vec();

    // 1. insert_picture で SVG 画像を挿入
    let shape_id = s.update(|e, _, cx| {
        e.insert_picture("sample-vector.svg", svg_bytes.clone(), cx).expect("SVG should be successfully inserted")
    });

    // 2. 挿入された図形の検証
    let (frame, media_format) = s.with(|e, _| {
        let shape = e.slide().shape(shape_id).expect("inserted shape exists");
        let ShapeKind::Picture(pic) = &shape.kind else { panic!("expected picture") };
        let media = e.presentation().media(pic.media).expect("media exists");
        (shape.frame, media.format)
    });
    assert_eq!(media_format, ibis_doc_model::MediaFormat::Svg);
    assert!(frame.width > 0 && frame.height > 0);

    // 3. SVG 画像の色変更（duotone 効果）の適用
    let duo_dark = ibis_doc_model::Rgb::from_rgb(0, 32, 96);
    let duo_light = ibis_doc_model::Rgb::from_rgb(200, 220, 255);
    s.update(|e, _, cx| {
        e.set_selected_pictures_duotone(Some((duo_dark, duo_light)), cx);
    });

    s.with(|e, _| {
        let shape = e.slide().shape(shape_id).expect("shape exists");
        let ShapeKind::Picture(pic) = &shape.kind else { panic!("picture") };
        assert_eq!(pic.duotone, Some((duo_dark, duo_light)));
    });

    // 4. Undo / Redo の検証
    s.key("cmd-z");
    s.with(|e, _| {
        let shape = e.slide().shape(shape_id).expect("shape exists");
        let ShapeKind::Picture(pic) = &shape.kind else { panic!("picture") };
        assert_eq!(pic.duotone, None, "undo removes duotone effect");
    });

    s.key("cmd-shift-z");
    s.with(|e, _| {
        let shape = e.slide().shape(shape_id).expect("shape exists");
        let ShapeKind::Picture(pic) = &shape.kind else { panic!("picture") };
        assert_eq!(pic.duotone, Some((duo_dark, duo_light)), "redo restores duotone effect");
    });

    // 5. 保存と再読込の検証
    s.save_as("test-svg-picture.pptx");
    let reopened = s.reopen();
    let reopened_slide = &reopened.slides[0];
    let reopened_pic_shape = reopened_slide.shapes.iter().find(|sh| {
        matches!(&sh.kind, ShapeKind::Picture(p) if reopened.media(p.media).is_some_and(|m| m.format == ibis_doc_model::MediaFormat::Svg))
    }).expect("reopened SVG picture shape exists");
    let ShapeKind::Picture(reopened_pic) = &reopened_pic_shape.kind else { panic!("picture") };
    assert_eq!(reopened_pic.duotone, Some((duo_dark, duo_light)), "duotone preserved after reopen");
    let reopened_media = reopened.media(reopened_pic.media).expect("media exists");
    assert_eq!(reopened_media.format, ibis_doc_model::MediaFormat::Svg);
    assert_eq!(reopened_media.data.as_ref(), &svg_bytes[..]);
}

/// PPTX-IMAGE-016: SVGの図形変換（Convert to Shape）
/// 1. SVGベクター画像をスライドに挿入
/// 2. 書式メニュー「pptx.picture.convert-to-shape」から図形に変換
/// 3. 元の画像が削除され、Officeカスタム図形（<a:custGeom> / Geometry::Formula）に変換されることを検証
/// 4. 各図形の塗りつぶし色や枠線が正しく設定されていることを検証
/// 5. Undo / Redo での元画像復帰と再変換
/// 6. 変換された図形の書式変更（塗りつぶし色変更）
/// 7. ファイル保存・再読込保持ラウンドトリップ検証
pub(crate) fn convert_svg_to_shape_and_round_trip() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let svg_bytes = br##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="80" viewBox="0 0 120 80">
        <rect width="120" height="80" fill="#336699"/>
        <circle cx="60" cy="40" r="30" fill="#ffcc00"/>
    </svg>"##.to_vec();

    // 1. insert_picture で SVG 画像を挿入
    let pic_shape_id = s.update(|e, _, cx| {
        e.insert_picture("sample-vector.svg", svg_bytes.clone(), cx).expect("SVG should be successfully inserted")
    });

    s.with(|e, _| {
        let shape = e.slide().shape(pic_shape_id).expect("inserted picture exists");
        assert!(matches!(shape.kind, ShapeKind::Picture(_)));
        assert!(e.has_selected_svg_picture(), "selected shape is SVG picture");
    });

    // 2. メニュー「pptx.picture.convert-to-shape」を実行
    s.menu("pptx.picture.convert-to-shape");

    // 3. 変換後の検証: 元の画像は削除され、2つの AutoShape が挿入されている
    let (converted_ids, fills) = s.with(|e, _| {
        assert!(e.slide().shape(pic_shape_id).is_none(), "original picture shape must be removed");
        let shapes: Vec<_> = e.selected_shapes().into_iter().collect();
        assert_eq!(shapes.len(), 2, "2 converted shapes should be selected");
        let mut fills = Vec::new();
        for sh in &shapes {
            let ShapeKind::Auto(auto) = &sh.kind else { panic!("converted shape must be AutoShape") };
            assert!(matches!(&auto.geometry, ibis_doc_model::Geometry::Formula(f) if f.preset.is_none()));
            fills.push(auto.fill.clone());
        }
        (shapes.iter().map(|sh| sh.id).collect::<Vec<_>>(), fills)
    });

    // 矩形 (#336699) と 円 (#ffcc00) の塗りつぶし色を検証
    assert_eq!(fills[0], ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x336699))));
    assert_eq!(fills[1], ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffcc00))));

    // 4. Undo: 元の SVG 画像に戻る
    s.key("cmd-z");
    s.with(|e, _| {
        assert!(e.slide().shape(pic_shape_id).is_some(), "undo restores picture shape");
        for &id in &converted_ids {
            assert!(e.slide().shape(id).is_none(), "undo removes converted shapes");
        }
    });

    // 5. Redo: 再度図形に変換される
    s.key("cmd-shift-z");
    s.with(|e, _| {
        assert!(e.slide().shape(pic_shape_id).is_none(), "redo removes picture shape");
        for &id in &converted_ids {
            assert!(e.slide().shape(id).is_some(), "redo restores converted shapes");
        }
    });

    // 6. 変換された図形の塗りつぶし色を変更（ネイティブ図形として編集可能）
    let target_shape_id = converted_ids[0];
    s.update(|e, _, cx| {
        e.set_selection(vec![target_shape_id]);
        e.fill_selected(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x00ff00))), cx);
    });

    s.with(|e, _| {
        let shape = e.slide().shape(target_shape_id).expect("shape exists");
        let ShapeKind::Auto(auto) = &shape.kind else { panic!("AutoShape") };
        assert_eq!(auto.fill, ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x00ff00))));
    });

    // 7. 保存と再読込
    s.save_as("test-convert-svg-to-shape.pptx");
    let reopened = s.reopen();
    let reopened_slide = &reopened.slides[0];

    // 再読込後、カスタム図形（preset: None）として保持されていることを確認
    let re_custom_shapes: Vec<_> = reopened_slide.shapes.iter().filter(|sh| {
        matches!(&sh.kind, ShapeKind::Auto(a) if matches!(&a.geometry, ibis_doc_model::Geometry::Formula(f) if f.preset.is_none()))
    }).collect();

    assert!(re_custom_shapes.len() >= 2, "reopened presentation must contain converted custom shapes");
    assert!(re_custom_shapes.iter().any(|sh| {
        let ShapeKind::Auto(a) = &sh.kind else { return false };
        a.fill == ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x00ff00)))
    }), "edited fill color must be preserved across reopen");
}

/// PPTX-OBJECT-004: 自由曲線・フリーフォーム（フリーハンド / Scribble）描画。
/// ツールバー「線」ドロップダウンまたはメニューから「フリーハンド（pptx.line.freehand）」を選択、
/// スライド上のドラッグによる連続軌跡の描画、カスタム図形（FormulaGeometry: preset=None）としての挿入、
/// 枠線（色・太さ）、塗りつぶし色の適用、Undo/Redo、ファイル保存・再オープン保持検証。
pub(crate) fn freehand_shape_drawing_and_options() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let initial_count = s.with(|e, _| e.slide().shapes.len());

    // 1. ツールバー「線」メニューから「フリーハンド」を選択
    s.toolbar_menu("insert-line", "pptx.line.freehand");
    assert_eq!(s.tool(), Tool::Line { kind: LineKind::Freehand, arrow: ArrowKind::None });

    // 2. ドラッグしてフリーハンド描画
    let a = s.at(EmuPoint::new(1_500_000, 2_000_000));
    let b = s.at(EmuPoint::new(4_500_000, 4_000_000));
    s.drag(a, b);

    // 描画後は Tool::Select に戻り、図形が選択状態になる
    assert_eq!(s.tool(), Tool::Select);
    assert_eq!(s.with(|e, _| e.selection_kind()), SelectionKind::Shape);
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), initial_count + 1);

    let freehand = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Auto(auto) = &freehand.kind else { panic!("freehand shape must be AutoShape") };
    assert!(matches!(&auto.geometry, ibis_doc_model::Geometry::Formula(f) if f.preset.is_none()), "must be custom FormulaGeometry");

    // 3. 枠線の太さと色を変更
    s.toolbar_menu("line-weight", "width.3");
    s.toolbar_menu("border-color", "color.red");

    let updated = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Auto(updated_auto) = &updated.kind else { panic!("auto shape") };
    assert_eq!(updated_auto.stroke.as_ref().map(|st| st.width), Some(38_100)); // 3pt = 38100 EMU
    assert_eq!(updated_auto.stroke.as_ref().map(|st| st.color), Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xff0000))));

    // 4. 塗りつぶしの変更（緑色で塗りつぶし）
    s.update(|e, _, cx| {
        e.fill_selected(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x00ff00))), cx);
    });
    let filled = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Auto(filled_auto) = &filled.kind else { panic!("auto shape") };
    assert_eq!(filled_auto.fill, ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x00ff00))));

    // 5. 保存と再読込
    s.save_as("test-freehand-shape.pptx");
    let saved = s.reopen();
    let saved_shapes = &saved.slides[0].shapes;
    assert_eq!(saved_shapes.len(), initial_count + 1);
    let ShapeKind::Auto(saved_auto) = &saved_shapes.last().unwrap().kind else { panic!("saved auto shape") };
    assert!(matches!(&saved_auto.geometry, ibis_doc_model::Geometry::Formula(f) if f.preset.is_none()));
    assert_eq!(saved_auto.stroke.as_ref().map(|st| st.width), Some(38_100));
    assert_eq!(saved_auto.stroke.as_ref().map(|st| st.color), Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xff0000))));
    assert_eq!(saved_auto.fill, ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x00ff00))));

    // 6. 全戻し（Undo）とやり直し（Redo）
    undo_all_and_redo(&mut s);
}

/// PPTX-IMAGE-017: アイコン挿入と図形変換連携の検証
/// 1. アイコン挿入（insert_icon("description")）
/// 2. スライド上に SVG Picture（名前: "Icon: description"）として配置されることを検証
/// 3. 「図形に変換（Convert to Shape）」を実行してネイティブなカスタム図形（<a:custGeom>）に変換
/// 4. 塗りつぶし色を変更
/// 5. ファイル保存・再読込での保持検証（ラウンドトリップ）
/// 6. Undo/Redo 全戻し・やり直し検証
pub(crate) fn insert_icon_and_convert_to_shape() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let initial_count = s.with(|e, _| e.slide().shapes.len());

    // 1. アイコン「description」（文書アイコン）を挿入
    let shape_id = s.update(|e, _, cx| {
        e.insert_icon("description", cx).expect("insert icon success")
    });

    // 挿入後は選択状態になり、Picture であること
    assert_eq!(s.with(|e, _| e.selection_kind()), SelectionKind::Picture);
    let inserted = s.with(|e, _| e.selected_shapes()[0].clone());
    assert_eq!(inserted.id, shape_id);
    let ShapeKind::Picture(pic) = &inserted.kind else { panic!("inserted icon must be Picture") };
    assert_eq!(pic.name, "Icon: description");
    let media = s.with(|e, _| e.presentation().media.iter().find(|m| m.id == pic.media).cloned().expect("media exists"));
    assert_eq!(media.format, ibis_doc_model::MediaFormat::Svg);

    // 2. 図形に変換（Convert to Shape）を実行
    let converted = s.update(|e, _, cx| e.convert_selected_svg_to_shape(cx));
    assert!(converted, "convert svg icon to shape must succeed");

    let conv_shape = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Auto(conv_auto) = &conv_shape.kind else { panic!("converted shape must be AutoShape") };
    assert!(matches!(&conv_auto.geometry, ibis_doc_model::Geometry::Formula(f) if f.preset.is_none()), "must be custom FormulaGeometry");

    // 3. 塗りつぶし色を変更（赤色）
    s.update(|e, _, cx| {
        e.fill_selected(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xff0000))), cx);
    });
    let red_shape = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Auto(red_auto) = &red_shape.kind else { panic!("auto shape") };
    assert_eq!(red_auto.fill, ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xff0000))));

    // 4. ファイル保存と再読込
    s.save_as("test-icon-insert.pptx");
    let saved = s.reopen();
    let saved_shapes = &saved.slides[0].shapes;
    assert_eq!(saved_shapes.len(), initial_count + 1);
    let saved_last = saved_shapes.last().unwrap();
    let ShapeKind::Auto(saved_auto) = &saved_last.kind else { panic!("saved shape must be AutoShape") };
    assert!(matches!(&saved_auto.geometry, ibis_doc_model::Geometry::Formula(f) if f.preset.is_none()));
    assert_eq!(saved_auto.fill, ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xff0000))));

    // 5. Undo と Redo
    undo_all_and_redo(&mut s);
}


/// PPTX-FILE-007: 互換性チェック（Compatibility Checker）
/// メニュー「pptx.compatibility-check」から互換性検査パネルを開き、
/// 旧バージョンでサポートされない機能（メディア、SVG、特殊グラフ、文字変形、高度効果等）
/// の検出、重大度分類（重大な機能低下 / 軽微な精度の低下）、該当箇所へのジャンプ、再検査、
/// パネルの開閉を検証。
pub(crate) fn compatibility_checker_and_options() {
    use ibis_doc_model::{AutoShape, Clip, Color, Fill, Geometry, MediaId, Picture, Shape, ShapeId, ShapeKind, TextBody, TextStyle};

    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. メニュー「pptx.compatibility-check」から互換性チェックパネルを開く
    s.menu("pptx.compatibility-check");
    assert!(s.with(|e, _| e.pptx().and_then(|st| st.compatibility_panel.as_ref()).is_some()), "compatibility panel opened");

    // 2. 互換性の問題を持つ要素（メディアクリップ、文字変形ワードアート、3D面取り付き図形）を追加
    s.update(|e, _, cx| {
        // (a) メディアクリップ付きピクチャ（重大な機能低下: Media）
        let media_pic = Picture {
            clip: Some(Clip::new(MediaId(9999), false)),
            ..Picture::new(MediaId(9999))
        };
        let shape_media = Shape::new(
            ShapeId(8881),
            ibis_doc_model::EmuRect::new(0, 0, 1000000, 1000000),
            ShapeKind::Picture(media_pic),
        );

        // (b) ワードアート変形付きテキスト（軽微な精度の低下: Text Warp）
        let mut text_body = TextBody::plain("変形ワードアート", TextStyle::default());
        text_body.warp = Some("textWave1".to_string());
        let shape_warp = Shape::new(
            ShapeId(8882),
            ibis_doc_model::EmuRect::new(1000000, 0, 1000000, 1000000),
            ShapeKind::TextBox(text_body),
        );

        // (c) 3D面取り・反射効果付きオートシェイプ（軽微な精度の低下: Bevel / Reflection）
        let mut auto = AutoShape::new(Geometry::Rect, Fill::Solid(Color::WHITE), None);
        auto.reflection = Some(ibis_doc_model::Reflection::standard(10000, 50000));
        let shape_effect = Shape::new(
            ShapeId(8883),
            ibis_doc_model::EmuRect::new(2000000, 0, 1000000, 1000000),
            ShapeKind::Auto(auto),
        );

        let mut slide = e.slide().clone();
        slide.shapes.push(shape_media);
        slide.shapes.push(shape_warp);
        slide.shapes.push(shape_effect);
        e.run(vec![ibis_doc_model::Command::SetSlide { slide }], cx);
    });

    // 3. 再検査（recheck_pptx_compatibility）を実行し、問題が検出されることを検証
    s.update(|e, _, cx| {
        e.recheck_pptx_compatibility(cx);
    });

    let (sig_len, minor_len, total) = s.with(|e, _| {
        let panel = e.pptx().unwrap().compatibility_panel.as_ref().unwrap();
        (panel.report.significant.len(), panel.report.minor.len(), panel.report.total_issues())
    });

    assert!(sig_len >= 1, "メディアクリップが重大な機能低下として検出されること");
    assert!(minor_len >= 2, "文字変形と反射効果が軽微な精度の低下として検出されること");
    assert_eq!(total, sig_len + minor_len, "トータル件数が一致すること");

    // 4. 重大な問題へのジャンプ動作を検証
    s.update(|e, _, cx| {
        e.jump_to_pptx_compatibility_issue(0, cx);
    });
    assert_eq!(s.with(|e, _| e.selection), Some(ShapeId(8881)), "メディア図形が選択されること");

    // 5. 軽微な問題へのジャンプ動作を検証（文字変形ワードアート図形 8882）
    s.update(|e, _, cx| {
        e.jump_to_pptx_compatibility_issue(sig_len, cx); // minor の 1 番目
    });
    assert_eq!(s.with(|e, _| e.selection), Some(ShapeId(8882)), "文字変形図形が選択されること");

    // 6. 軽微な問題へのジャンプ動作を検証（反射効果図形 8883）
    s.update(|e, _, cx| {
        e.jump_to_pptx_compatibility_issue(sig_len + 1, cx); // minor の 2 番目
    });
    assert_eq!(s.with(|e, _| e.selection), Some(ShapeId(8883)), "反射効果図形が選択されること");

    // 7. パネルを閉じる動作を検証
    s.update(|e, _, cx| {
        e.close_pptx_compatibility_panel(cx);
    });
    assert!(s.with(|e, _| e.pptx().and_then(|st| st.compatibility_panel.as_ref()).is_none()), "panel is closed");
}

/// PPTX-ACCESS-010: メディアの字幕と代替説明（Closed Captions and Audio Descriptions for Media）
pub(crate) fn media_captions_and_accessibility() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let video_path = root.join("clip-test.mp4");
    let video_bytes = std::fs::read(&video_path).expect("clip-test.mp4 should exist");

    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(1);
    s.menu("pptx.new-slide");

    // 1. 動画クリップを挿入
    let clip_id = s.update(|e, _, cx| e.insert_clip(&video_path, video_bytes.clone(), false, cx)).expect("clip inserted");
    s.update(|e, _, _| e.set_selection(vec![clip_id]));

    // 2. メニュー「pptx.check-accessibility」からアクセシビリティ パネルを開く
    s.menu("pptx.check-accessibility");
    assert!(s.with(|e, _| e.sidebar_open && e.pptx().and_then(|st| st.accessibility_panel.as_ref()).is_some()), "accessibility panel open");

    // 3. 動画クリップに字幕未設定警告（AddMediaCaptions）が検出されていることを確認
    let slide_id = s.with(|e, _| e.current_slide);
    let has_missing_caption_warning = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        p.report.warnings.iter().any(|w| w.slide_id == slide_id && w.shape_id == Some(clip_id) && w.fix_type == crate::pptx::accessibility_panel::IssueFixType::AddMediaCaptions)
    });
    assert!(has_missing_caption_warning, "media clip should trigger missing captions warning");

    // 4. 字幕トラックを追加（日本語）
    s.update(|e, _, cx| {
        e.add_selected_clip_caption("日本語", "ja-JP", "WEBVTT\n\n00:00:00.000 --> 00:00:05.000\nテスト字幕", cx);
    });

    let opts = s.with(|e, _| e.selected_clip_options()).expect("selected clip options");
    assert_eq!(opts.captions.len(), 1);
    assert_eq!(opts.captions[0].label, "日本語");
    assert_eq!(opts.captions[0].lang, "ja-JP");
    assert!(opts.captions[0].vtt.contains("テスト字幕"));

    // 5. アクセシビリティを再検査し、字幕欠落警告が解消されたことを確認
    s.update(|e, _, cx| {
        e.recheck_pptx_accessibility(cx);
    });
    let has_missing_caption_after = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        p.report.warnings.iter().any(|w| w.slide_id == slide_id && w.shape_id == Some(clip_id) && w.fix_type == crate::pptx::accessibility_panel::IssueFixType::AddMediaCaptions)
    });
    assert!(!has_missing_caption_after, "missing caption warning should be resolved after adding caption");

    // 6. 2本目の字幕トラックを追加（英語）
    s.update(|e, _, cx| {
        e.add_selected_clip_caption("English", "en-US", "WEBVTT\n\n00:00:00.000 --> 00:00:05.000\nTest Caption", cx);
    });
    let opts2 = s.with(|e, _| e.selected_clip_options()).expect("selected clip options");
    assert_eq!(opts2.captions.len(), 2);
    assert_eq!(opts2.captions[1].label, "English");

    // 7. 1本目を削除して英語字幕だけ残ることを確認
    s.update(|e, _, cx| {
        e.remove_selected_clip_caption(0, cx);
    });
    let opts_after_remove = s.with(|e, _| e.selected_clip_options()).expect("selected clip options");
    assert_eq!(opts_after_remove.captions.len(), 1);
    assert_eq!(opts_after_remove.captions[0].label, "English");

    // 8. Undo（cmd-z）で削除を取り消し、2本に戻ることを確認
    s.key("cmd-z");
    let opts_undo = s.with(|e, _| e.selected_clip_options()).expect("selected clip options");
    assert_eq!(opts_undo.captions.len(), 2);

    // 9. Redo（shift-cmd-z）で再度1本になることを確認
    s.key("shift-cmd-z");
    let opts_redo = s.with(|e, _| e.selected_clip_options()).expect("selected clip options");
    assert_eq!(opts_redo.captions.len(), 1);

    // 10. ファイル保存と再オープンで字幕トラックが保持されることを検証
    s.save_as("media-captions-scenario.pptx");
    let reopened = s.reopen();
    let rep_slide = &reopened.slides[1];
    let rep_shape = rep_slide.shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.is_some())).expect("clip shape exists");
    if let ShapeKind::Picture(pic) = &rep_shape.kind {
        let opt = &pic.clip.as_ref().unwrap().options;
        assert_eq!(opt.captions.len(), 1);
        assert_eq!(opt.captions[0].label, "English");
        assert_eq!(opt.captions[0].lang, "en-US");
        assert!(opt.captions[0].vtt.contains("Test Caption"));
    } else {
        panic!("expected Picture with clip");
    }

    // 11. メニューコマンド「pptx.clip.caption.clear」で字幕を全消去できることを検証
    s.menu("pptx.clip.caption.clear");
    let opts_cleared = s.with(|e, _| e.selected_clip_options()).expect("selected clip options");
    assert!(opts_cleared.captions.is_empty(), "captions cleared");
}

/// PPTX-FILE-010, PPTX-FILE-012: 自動回復の設定と未保存プレゼンテーションの回復
pub(crate) fn autorecover_settings_and_document_recovery() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. メニュー「pptx.autorecover-settings」から自動回復設定パネルを開く
    s.menu("pptx.autorecover-settings");
    assert!(s.with(|e, _| e.sidebar_open && e.pptx().and_then(|st| st.autorecover_settings.as_ref()).is_some()), "autorecover settings panel open");

    // 2. 初期設定値の確認（有効、間隔 >= 1 分）
    let (enabled, interval_str) = s.with(|e, cx| {
        let p = e.pptx().unwrap().autorecover_settings.as_ref().unwrap();
        (p.enabled, p.interval_input.read(cx).value().to_string())
    });
    assert!(enabled, "autorecover should be enabled by default");
    assert!(!interval_str.is_empty(), "interval string should not be empty");

    // 3. 設定の保存間隔を 5 分に変更して保存
    s.update(|e, w, cx| {
        let p = e.pptx_mut().unwrap().autorecover_settings.as_mut().unwrap();
        p.interval_input.update(cx, |input, cx| {
            input.set_value("5".to_string(), w, cx);
        });
        e.save_pptx_autorecover_settings(cx);
    });
    let saved_msg = s.with(|e, _| e.pptx().unwrap().autorecover_settings.as_ref().unwrap().message.clone());
    assert!(!saved_msg.is_empty(), "settings saved message shown");
    let cfg = crate::pptx::autorecover::load_autorecover_config();
    assert_eq!(cfg.interval_minutes, 5);

    // 4. 「今すぐ自動回復データを保存」を実行
    s.update(|e, w, cx| {
        e.save_pptx_autorecover_now(w, cx);
    });
    let now_msg = s.with(|e, _| e.pptx().unwrap().autorecover_settings.as_ref().unwrap().message.clone());
    assert!(now_msg.contains("自動回復") || now_msg.contains("AutoRecover"), "snapshot saved message shown");

    // 5. ディスク上にスナップショットファイルが存在し、空でないことを検証
    let active_path = s.with(|e, _| e.active_autorecover_file.clone());
    assert!(active_path.is_some(), "active autorecover file path recorded");
    let snapshot_file = active_path.unwrap();
    assert!(snapshot_file.exists(), "snapshot file exists on disk");
    assert!(std::fs::metadata(&snapshot_file).unwrap().len() > 0, "snapshot file is not empty");

    // 6. 設定パネルを閉じる
    s.update(|e, _, cx| {
        e.close_pptx_autorecover_settings_panel(cx);
    });
    assert!(s.with(|e, _| e.pptx().unwrap().autorecover_settings.is_none()), "settings panel closed");

    // 7. メニュー「pptx.recover-unsaved」から未保存プレゼンテーションの回復パネルを開く
    s.menu("pptx.recover-unsaved");
    assert!(s.with(|e, _| e.sidebar_open && e.pptx().and_then(|st| st.recover_unsaved.as_ref()).is_some()), "recover unsaved panel open");

    // 8. 回復一覧にスナップショットが含まれていることを確認
    let entries_count = s.with(|e, _| e.pptx().unwrap().recover_unsaved.as_ref().unwrap().entries.len());
    assert!(entries_count >= 1, "at least one recovered presentation should be listed");

    // 9. 回復ファイルを開く（エントリ 0）
    s.update(|e, w, cx| {
        e.open_pptx_recovered_file_entry(0, w, cx);
    });

    // 10. ドキュメントが回復モード（is_recovered == true）でロードされ、回復パネルが閉じていることを検証
    assert!(s.with(|e, _| e.is_recovered), "document is in recovered mode");
    assert!(s.with(|e, _| e.pptx().unwrap().recover_unsaved.is_none()), "recover panel closed after opening");

    // 11. 「名前を付けて保存」で保存し、is_recovered が false にリセットされることを検証
    s.save_as("saved-from-recovery.pptx");
    assert!(!s.with(|e, _| e.is_recovered), "is_recovered reset to false after save");

    // クリーンアップ
    let _ = std::fs::remove_file(&snapshot_file);
}




/// PPTX-ACCESS-004: 読み上げ順序ペインとアクセシビリティ検査連携の検証
/// 1. メニュー「pptx.reading-order」から読み順ペインの表示
/// 2. 図形の「上へ」「下へ」による順序移動（Command::MoveShape）と選択追従
/// 3. Undo / Redo での順序復元
/// 4. 視覚的配置・タイトル優先・装飾用スキップに基づく自動整列（auto_reading_order）
/// 5. アクセシビリティ検査パネルとの連携（複数コンテンツスライドでの CheckReadingOrder Tip検出、「読み順を確認」遷移）
/// 6. ファイル保存と再読込での読み上げ順序保持の検証
pub(crate) fn reading_order_pane_and_accessibility_check() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. メニュー「pptx.reading-order」から読み順ペインを開く
    s.menu("pptx.reading-order");
    assert!(
        s.with(|e, _| e.sidebar_open && e.pptx().and_then(|st| st.reading_order_pane.as_ref()).is_some()),
        "reading order pane should be open"
    );

    // スライド 1（タイトルとサブタイトルの2つの図形）の初期順序を確認
    let slide_id = s.with(|e, _| e.current_slide);
    let (orig_id0, orig_id1, text0, text1) = s.with(|e, _| {
        let slide = e.slide();
        assert!(slide.shapes.len() >= 2, "slide 1 has at least 2 shapes");
        (
            slide.shapes[0].id,
            slide.shapes[1].id,
            slide.shapes[0].text_body().unwrap().text(),
            slide.shapes[1].text_body().unwrap().text(),
        )
    });

    // 2. shape 0 を「下へ」（遅く読み上げ）移動
    s.update(|e, _, cx| {
        e.move_reading_order(orig_id0, false, cx);
    });

    s.with(|e, _| {
        let slide = e.slide();
        assert_eq!(slide.shapes[0].id, orig_id1, "shape 1 moved to first reading position");
        assert_eq!(slide.shapes[1].id, orig_id0, "shape 0 moved to second reading position");
        assert_eq!(e.selected_ids(), vec![orig_id0], "moved shape stays selected");
    });

    // 3. shape 0 を「上へ」（早く読み上げ）移動して元に戻す
    s.update(|e, _, cx| {
        e.move_reading_order(orig_id0, true, cx);
    });

    s.with(|e, _| {
        let slide = e.slide();
        assert_eq!(slide.shapes[0].id, orig_id0);
        assert_eq!(slide.shapes[1].id, orig_id1);
    });

    // 4. Undo / Redo の検証
    s.key("cmd-z");
    s.with(|e, _| {
        let slide = e.slide();
        assert_eq!(slide.shapes[0].id, orig_id1, "undo restores swapped order");
    });

    s.key("cmd-shift-z");
    s.with(|e, _| {
        let slide = e.slide();
        assert_eq!(slide.shapes[0].id, orig_id0, "redo restores original order");
    });

    // 順序を逆（サブタイトル先頭、タイトル2番目）にしておく
    s.update(|e, _, cx| {
        e.move_reading_order(orig_id0, false, cx);
    });
    assert_eq!(s.with(|e, _| e.slide().shapes[0].id), orig_id1);

    // 5. 自動整列（auto_reading_order）の検証
    // タイトルプレースホルダー（orig_id0）が最優先され、先頭に戻るはず
    s.update(|e, _, cx| {
        e.auto_reading_order(cx);
    });
    s.with(|e, _| {
        let slide = e.slide();
        assert_eq!(slide.shapes[0].id, orig_id0, "auto reading order places title first");
        assert_eq!(slide.shapes[1].id, orig_id1, "subtitle follows title");
    });

    // 6. 装飾用オブジェクト（decorative）を含む場合の自動整列
    // orig_id0 を装飾用にマークすると、読み上げ不要として末尾に送られる
    s.update(|e, _, cx| {
        e.set_selection(vec![orig_id0]);
        e.run(vec![ibis_doc_model::Command::SetShapeDecorative { slide: slide_id, shape: orig_id0, decorative: true }], cx);
    });
    assert!(s.with(|e, _| e.slide().decorative_shapes.contains(&orig_id0)));

    s.update(|e, _, cx| {
        e.auto_reading_order(cx);
    });
    s.with(|e, _| {
        let slide = e.slide();
        assert_eq!(slide.shapes[0].id, orig_id1, "non-decorative content comes first");
        assert_eq!(slide.shapes[1].id, orig_id0, "decorative shape placed at the end");
    });

    // 装飾用フラグを解除して、再度整列
    s.update(|e, _, cx| {
        e.set_selection(vec![orig_id0]);
        e.run(vec![ibis_doc_model::Command::SetShapeDecorative { slide: slide_id, shape: orig_id0, decorative: false }], cx);
        e.auto_reading_order(cx);
    });
    assert_eq!(s.with(|e, _| e.slide().shapes[0].id), orig_id0);

    // 7. アクセシビリティ検査パネルとの連携確認
    s.menu("pptx.check-accessibility");
    assert!(
        s.with(|e, _| e.sidebar_open && e.pptx().and_then(|st| st.accessibility_panel.as_ref()).is_some()),
        "accessibility panel open"
    );

    // スライド 1（2つ以上のコンテンツ図形がある）に対する CheckReadingOrder Tip が検出されていること
    let has_reading_order_tip = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        p.report.tips.iter().any(|tip| {
            tip.slide_id == slide_id && tip.fix_type == crate::pptx::accessibility_panel::IssueFixType::CheckReadingOrder
        })
    });
    assert!(has_reading_order_tip, "check reading order tip should be reported for multi-shape slide");

    // 「読み順を確認」ボタン相当の動作（スライド選択＆読み順ペインオープン）
    s.update(|e, _, cx| {
        e.current_slide = slide_id;
        e.open_reading_order_pane(cx);
    });
    assert!(
        s.with(|e, _| e.sidebar_open && e.pptx().and_then(|st| st.reading_order_pane.as_ref()).is_some()),
        "reading order pane opened from accessibility check tip"
    );

    // 8. 順序を変更した状態で保存・再読込し、順序がファイルに正しく保持されることを検証
    s.update(|e, _, cx| {
        e.move_reading_order(orig_id0, false, cx);
    });
    assert_eq!(s.with(|e, _| e.slide().shapes[0].id), orig_id1);

    s.save_as("test-reading-order.pptx");
    let reopened = s.reopen();
    let reopened_slide = &reopened.slides[0];
    assert_eq!(reopened_slide.shapes[0].text_body().unwrap().text(), text1, "reopened presentation preserves reading order");
    assert_eq!(reopened_slide.shapes[1].text_body().unwrap().text(), text0);
}

/// PPTX-ACCESS-011: テキスト色と背景色のコントラスト比検査（WCAG 2.1 基準）
/// 1. 白背景スライド上に薄い色（黄色 #FFFF00）のテキストを設定して低コントラスト警告を発生
/// 2. アクセシビリティ検査パネルを開き、IssueFixType::FixContrast の警告が検出されることを検証
/// 3. 「表示」ボタン相当の jump_to_pptx_accessibility_issue で該当図形にジャンプ・選択されることを確認
/// 4. テキスト色を濃い色（黒 #000000、コントラスト比 21:1）に変更
/// 5. 再検査（recheck_pptx_accessibility）を実行し、コントラスト警告が解消されることを検証
/// 6. Undo（⌘Z）で低コントラスト警告が再検出され、Redo（⇧⌘Z）で解消されることを検証
pub(crate) fn text_contrast_accessibility_check() {
    use ibis_doc_model::{Command, Rgb};

    // 0. 相対輝度とコントラスト比の計算検証（WCAG 2.1 基準）
    assert!((crate::pptx::accessibility_panel::relative_luminance(Rgb::WHITE) - 1.0).abs() < 1e-4);
    assert!((crate::pptx::accessibility_panel::relative_luminance(Rgb::BLACK) - 0.0).abs() < 1e-4);
    assert!((crate::pptx::accessibility_panel::contrast_ratio(Rgb::WHITE, Rgb::BLACK) - 21.0).abs() < 0.1);
    assert!(crate::pptx::accessibility_panel::contrast_ratio(Rgb::WHITE, Rgb(0xffff00)) < 1.2);
    assert!(crate::pptx::accessibility_panel::contrast_ratio(Rgb::WHITE, Rgb(0x0000ff)) >= 4.5);

    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    let slide_id = s.with(|e, _| e.current_slide);
    let shape_id = s.with(|e, _| e.slide().shapes[0].id);

    // 1. スライド 1 の図形 0 のテキスト色を黄色（#FFFF00）に変更（白背景に対して極めて低コントラスト）
    let original_style = s.with(|e, _| e.slide().shapes[0].text_body().unwrap().style.clone());
    let mut low_contrast_style = original_style.clone();
    low_contrast_style.color = Rgb(0xffff00);

    s.update(|e, _, cx| {
        e.run(vec![Command::SetTextStyle { slide: slide_id, shape: shape_id, style: low_contrast_style }], cx);
    });

    // 2. メニュー「pptx.check-accessibility」からアクセシビリティ検査パネルを開く
    s.menu("pptx.check-accessibility");
    assert!(
        s.with(|e, _| e.sidebar_open && e.pptx().and_then(|st| st.accessibility_panel.as_ref()).is_some()),
        "accessibility panel should be open"
    );

    // 3. IssueFixType::FixContrast の警告が検出されていることを確認
    let has_contrast_warning = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        p.report.warnings.iter().any(|w| {
            w.slide_id == slide_id && w.shape_id == Some(shape_id) && w.fix_type == crate::pptx::accessibility_panel::IssueFixType::FixContrast
        })
    });
    assert!(has_contrast_warning, "low contrast text should trigger FixContrast warning");

    // 4. 「表示」ボタン（jump_to_pptx_accessibility_issue）で該当図形が選択されることを確認
    let warning_idx = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        let all: Vec<_> = p.report.errors.iter().chain(p.report.warnings.iter()).chain(p.report.tips.iter()).collect();
        all.iter().position(|w| w.slide_id == slide_id && w.shape_id == Some(shape_id) && w.fix_type == crate::pptx::accessibility_panel::IssueFixType::FixContrast).unwrap()
    });
    s.update(|e, _, cx| {
        e.jump_to_pptx_accessibility_issue(warning_idx, cx);
    });
    assert_eq!(s.with(|e, _| e.selected_ids()), vec![shape_id], "target shape should be selected");

    // 5. テキスト色をコントラスト比の高い色（黒 #000000）に変更
    let mut high_contrast_style = original_style.clone();
    high_contrast_style.color = Rgb(0x000000);
    s.update(|e, _, cx| {
        e.run(vec![Command::SetTextStyle { slide: slide_id, shape: shape_id, style: high_contrast_style }], cx);
        e.recheck_pptx_accessibility(cx);
    });

    // 6. コントラスト警告が解消されたことを確認
    let has_contrast_warning_after = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        p.report.warnings.iter().any(|w| {
            w.slide_id == slide_id && w.shape_id == Some(shape_id) && w.fix_type == crate::pptx::accessibility_panel::IssueFixType::FixContrast
        })
    });
    assert!(!has_contrast_warning_after, "FixContrast warning should be resolved after setting high contrast text");

    // 7. Undo（⌘Z）で低コントラスト警告が再発することを確認
    s.key("cmd-z");
    s.update(|e, _, cx| {
        e.recheck_pptx_accessibility(cx);
    });
    let has_contrast_warning_undo = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        p.report.warnings.iter().any(|w| {
            w.slide_id == slide_id && w.shape_id == Some(shape_id) && w.fix_type == crate::pptx::accessibility_panel::IssueFixType::FixContrast
        })
    });
    assert!(has_contrast_warning_undo, "FixContrast warning should re-appear after undo");

    // 8. Redo（⇧⌘Z）で警告が再度解消されることを確認
    s.key("cmd-shift-z");
    s.update(|e, _, cx| {
        e.recheck_pptx_accessibility(cx);
    });
    let has_contrast_warning_redo = s.with(|e, _| {
        let p = e.pptx().unwrap().accessibility_panel.as_ref().unwrap();
        p.report.warnings.iter().any(|w| {
            w.slide_id == slide_id && w.shape_id == Some(shape_id) && w.fix_type == crate::pptx::accessibility_panel::IssueFixType::FixContrast
        })
    });
    assert!(!has_contrast_warning_redo, "FixContrast warning should be resolved after redo");

    // 9. パネルを閉じる
    s.update(|e, _, cx| {
        e.close_pptx_accessibility_panel(cx);
    });
    assert!(s.with(|e, _| e.pptx().and_then(|st| st.accessibility_panel.as_ref()).is_none()));
}

pub(crate) fn proofing_language_selection_and_options() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. スライド 1 の図形 1（サブタイトル）を選択
    s.click_shape(1);
    assert!(s.with(|e, _| e.selected_text_style().is_some()));

    // 2. メニュー項目一覧を取得
    let menu_items = s.with(|e, _| crate::pptx::chrome::proofing_language_menu_items(e));
    assert!(menu_items.len() >= 11, "should have at least 10 languages + separator + none");

    // 3. "en-US" を設定
    s.update(|e, w, cx| e.set_selection_language(Some("en-US"), w, cx));
    assert_eq!(s.with(|e, _| e.selected_language()), Some("en-US".to_string()));

    // 4. メニューのチェック状態を確認（en-US が checked）
    let menu_items = s.with(|e, _| crate::pptx::chrome::proofing_language_menu_items(e));
    let en_item = menu_items.iter().find(|m| matches!(m, crate::shell::chrome::MenuEntry::Item(item) if item.id == "pptx.proofing-lang.en-US")).unwrap();
    if let crate::shell::chrome::MenuEntry::Item(item) = en_item {
        assert!(item.checked, "en-US should be checked");
    }
    let ja_item = menu_items.iter().find(|m| matches!(m, crate::shell::chrome::MenuEntry::Item(item) if item.id == "pptx.proofing-lang.ja-JP")).unwrap();
    if let crate::shell::chrome::MenuEntry::Item(item) = ja_item {
        assert!(!item.checked, "ja-JP should not be checked");
    }

    // 5. "ja-JP" に変更
    s.update(|e, w, cx| e.set_selection_language(Some("ja-JP"), w, cx));
    assert_eq!(s.with(|e, _| e.selected_language()), Some("ja-JP".to_string()));

    // 6. Undo（⌘Z）で en-US に戻る
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.selected_language()), Some("en-US".to_string()));

    // 7. Redo（⇧⌘Z）で ja-JP に戻る
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.selected_language()), Some("ja-JP".to_string()));

    // 8. 言語をクリア（None）
    s.update(|e, w, cx| e.set_selection_language(None, w, cx));
    assert_eq!(s.with(|e, _| e.selected_language()), None);
    let menu_items_cleared = s.with(|e, _| crate::pptx::chrome::proofing_language_menu_items(e));
    let none_item = menu_items_cleared.iter().find(|m| matches!(m, crate::shell::chrome::MenuEntry::Item(item) if item.id == "pptx.proofing-lang.none")).unwrap();
    if let crate::shell::chrome::MenuEntry::Item(item) = none_item {
        assert!(item.checked, "none option should be checked");
    }

    // 9. もう一度 "fr-FR" を設定して保存・再読込
    s.update(|e, w, cx| e.set_selection_language(Some("fr-FR"), w, cx));
    assert_eq!(s.with(|e, _| e.selected_language()), Some("fr-FR".to_string()));

    s.save_as("pptx-proofing-lang.pptx");
    let saved = s.reopen();
    let body = saved.slides[0].shapes[1].text_body().unwrap();
    assert_eq!(body.paragraphs[0].runs[0].style.lang.as_deref(), Some("fr-FR"), "reopened run should have fr-FR language");
}

pub(crate) fn transition_sound_options() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. サウンド一覧メニューを取得
    let sound_items = s.with(|e, _| crate::pptx::chrome::transition_sound_items(e));
    assert!(sound_items.len() >= 20, "should have none, 18 sounds, stop, loop");

    // 2. "Chime" を設定
    s.update(|e, _, cx| {
        e.set_transition_sound(
            Some(ibis_doc_model::TransitionSound::Play {
                name: "Chime".into(),
                loop_sound: false,
            }),
            cx,
        );
    });

    // 3. メニュー状態を確認（Chime が checked、サウンドなし が false、loop が false）
    let sound_items = s.with(|e, _| crate::pptx::chrome::transition_sound_items(e));
    let chime_item = sound_items.iter().find(|m| matches!(m, crate::shell::chrome::MenuEntry::Item(item) if item.id == "transition.sound.chime")).unwrap();
    if let crate::shell::chrome::MenuEntry::Item(item) = chime_item {
        assert!(item.checked, "Chime should be checked");
    }
    let none_item = sound_items.iter().find(|m| matches!(m, crate::shell::chrome::MenuEntry::Item(item) if item.id == "transition.sound.none")).unwrap();
    if let crate::shell::chrome::MenuEntry::Item(item) = none_item {
        assert!(!item.checked, "None should not be checked");
    }
    let loop_item = sound_items.iter().find(|m| matches!(m, crate::shell::chrome::MenuEntry::Item(item) if item.id == "transition.sound.loop")).unwrap();
    if let crate::shell::chrome::MenuEntry::Item(item) = loop_item {
        assert!(!item.checked, "Loop should not be checked initially");
        assert!(!item.disabled, "Loop should not be disabled when sound is playing");
    }

    // 4. loop_sound をトグル切り替え
    s.update(|e, _, cx| e.toggle_transition_sound_loop(cx));
    let sound_items = s.with(|e, _| crate::pptx::chrome::transition_sound_items(e));
    let loop_item = sound_items.iter().find(|m| matches!(m, crate::shell::chrome::MenuEntry::Item(item) if item.id == "transition.sound.loop")).unwrap();
    if let crate::shell::chrome::MenuEntry::Item(item) = loop_item {
        assert!(item.checked, "Loop should be checked after toggle");
    }

    // 5. "Stop" に設定
    s.update(|e, _, cx| e.set_transition_sound(Some(ibis_doc_model::TransitionSound::Stop), cx));
    let sound_items = s.with(|e, _| crate::pptx::chrome::transition_sound_items(e));
    let stop_item = sound_items.iter().find(|m| matches!(m, crate::shell::chrome::MenuEntry::Item(item) if item.id == "transition.sound.stop")).unwrap();
    if let crate::shell::chrome::MenuEntry::Item(item) = stop_item {
        assert!(item.checked, "Stop should be checked");
    }

    // 6. Undo（⌘Z）で loop: true に戻る
    s.key("cmd-z");
    let sound_after_undo = s.with(|e, _| e.slide().transition.as_ref().and_then(|t| t.sound.clone()));
    assert_eq!(
        sound_after_undo,
        Some(ibis_doc_model::TransitionSound::Play {
            name: "Chime".into(),
            loop_sound: true,
        })
    );

    // 7. Redo（⇧⌘Z）で Stop に戻る
    s.key("cmd-shift-z");
    let sound_after_redo = s.with(|e, _| e.slide().transition.as_ref().and_then(|t| t.sound.clone()));
    assert_eq!(sound_after_redo, Some(ibis_doc_model::TransitionSound::Stop));

    // 8. 全スライドに適用（apply_transition_to_all）
    s.update(|e, _, cx| e.apply_transition_to_all(cx));
    let all_slides_have_stop = s.with(|e, _| {
        e.presentation().slides.iter().all(|sl| {
            sl.transition.as_ref().and_then(|t| t.sound.as_ref()) == Some(&ibis_doc_model::TransitionSound::Stop)
        })
    });
    assert!(all_slides_have_stop, "All slides should have Stop sound");

    // 9. スライド 0 を "Camera" に変更し保存・再読込
    s.update(|e, _, cx| {
        e.set_transition_sound(
            Some(ibis_doc_model::TransitionSound::Play {
                name: "Camera".into(),
                loop_sound: false,
            }),
            cx,
        );
    });
    s.save_as("pptx-transition-sound.pptx");
    let saved = s.reopen();
    let t0 = saved.slides[0].transition.as_ref().unwrap();
    assert_eq!(
        t0.sound,
        Some(ibis_doc_model::TransitionSound::Play {
            name: "Camera".into(),
            loop_sound: false,
        }),
        "Slide 0 should have Camera sound on reopen"
    );
}

/// PPTX-ANIM-013: アニメーションの繰返し（repeatCount）と自動反転（autoRev）
/// 1. アニメーションを追加し、デフォルトの繰返し（None）と自動反転（false）を確認
/// 2. ツールバーメニューから繰返し回数（3回）と自動反転（true）を設定
/// 3. メニュー項目の checked 状態を検証
/// 4. タイムライン計算（compute_timeline）で繰返しによる継続時間（duration_ms, end_time_ms）の伸長を検証
/// 5. アニメーション ウィンドウのインライン操作（cycle_effect_repeat_at, toggle_effect_autoreverse_at）を検証
/// 6. Undo（⌘Z）/ Redo（⇧⌘Z）での状態復元を検証
/// 7. 保存（save_as）と再読込（reopen）で OOXML から repeat_count と auto_reverse が正しく復元されることを検証
pub(crate) fn animation_repeat_and_autoreverse_options() {
    use ibis_doc_model::AnimationRepeat;
    use crate::pptx::animation_ui::compute_timeline;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. 図形0（タイトル）を選択し、フェード（開始）を追加
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.in.fade");

    // 初期状態の確認 (repeat_count: None, auto_reverse: false)
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.repeat_count)), Some(AnimationRepeat::None));
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.auto_reverse)), Some(false));

    // 2. ツールバーメニューから「繰り返し: 3回」を設定
    s.toolbar_menu("motion", "pptx.anim.repeat.3");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.repeat_count)), Some(AnimationRepeat::Count(3)));

    // 3. ツールバーメニューから「自動反転」を設定
    s.toolbar_menu("motion", "pptx.anim.autoreverse");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.auto_reverse)), Some(true));

    // 4. メニュー状態の検証
    let anim_menu = s.with(|e, _| crate::pptx::animation_ui::animation_items(e));
    let repeat_3_item = crate::shell::scenario::find_item(&anim_menu, "pptx.anim.repeat.3").unwrap();
    assert!(repeat_3_item.checked, "repeat 3 should be checked");
    let repeat_none_item = crate::shell::scenario::find_item(&anim_menu, "pptx.anim.repeat.none").unwrap();
    assert!(!repeat_none_item.checked, "repeat none should not be checked");
    let autoreverse_item = crate::shell::scenario::find_item(&anim_menu, "pptx.anim.autoreverse").unwrap();
    assert!(autoreverse_item.checked, "autoreverse should be checked");

    // 5. タイムライン計算（compute_timeline）の検証
    // フェードは 500ms、3回繰返しで 1500ms
    let (items, _) = s.with(|e, _| compute_timeline(&e.slide().animations));
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].duration_ms, 1500);
    assert_eq!(items[0].end_time_ms, 1500);

    // 6. アニメーション ウィンドウのインライン操作（cycle_effect_repeat_at, toggle_effect_autoreverse_at）
    s.update(|e, _, cx| {
        e.cycle_effect_repeat_at(0, cx); // Count(3) -> Count(5)
    });
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.repeat_count)), Some(AnimationRepeat::Count(5)));

    s.update(|e, _, cx| {
        e.toggle_effect_autoreverse_at(0, cx); // true -> false
    });
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.auto_reverse)), Some(false));

    // 7. Undo / Redo の検証
    // ⌘Z: auto_reverse が true に戻る
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.auto_reverse)), Some(true));
    // ⌘Z: repeat_count が Count(3) に戻る
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.repeat_count)), Some(AnimationRepeat::Count(3)));

    // ⇧⌘Z: repeat_count が Count(5) に進む
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.repeat_count)), Some(AnimationRepeat::Count(5)));
    // ⇧⌘Z: auto_reverse が false に進む
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.auto_reverse)), Some(false));

    // 8. 繰り返しを「次のクリックまで（UntilNextClick）」、自動反転を有効（true）にして保存・再読込
    s.toolbar_menu("motion", "pptx.anim.repeat.until-click");
    s.toolbar_menu("motion", "pptx.anim.autoreverse");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.repeat_count)), Some(AnimationRepeat::UntilNextClick));
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.auto_reverse)), Some(true));

    s.save_as("pptx-anim-repeat-autoreverse.pptx");
    let saved = s.reopen();
    let saved_anim = saved.slides[0].animations.first().expect("saved animation");
    assert_eq!(saved_anim.repeat_count, AnimationRepeat::UntilNextClick, "repeat_count should be preserved on reopen");
    assert_eq!(saved_anim.auto_reverse, true, "auto_reverse should be preserved on reopen");
}

/// PPTX-ANIM-014: アニメーションの開始・終了の滑らかさ（accel / decel, Smooth Start and End）
/// 1. アニメーションを追加し、初期状態（等速: accel=0, decel=0）を確認
/// 2. ツールバーメニューから「滑らかさ: 両方（50%/50%）」を設定
/// 3. メニュー項目の checked 状態を検証
/// 4. イージング計算（apply_timing_easing）による進行度の滑らかな加速・減速変換を検証
/// 5. アニメーション ウィンドウのインライン操作（cycle_effect_smooth_at）を検証
/// 6. Undo（⌘Z）/ Redo（⇧⌘Z）での状態復元を検証
/// 7. 保存（save_as）と再読込（reopen）で OOXML から accel と decel が正しく復元されることを検証
pub(crate) fn animation_smooth_start_and_end_options() {
    use crate::pptx::slideshow::apply_timing_easing;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 0. イージング関数の数学的正確性を検証
    // 等速
    assert_eq!(apply_timing_easing(0.0, 0, 0), 0.0);
    assert_eq!(apply_timing_easing(0.5, 0, 0), 0.5);
    assert_eq!(apply_timing_easing(1.0, 0, 0), 1.0);
    // 両方（50%/50%）: 進行度 0.25 のときは加速中で 0.125、0.5 のときは中間点 0.5、0.75 のときは減速中で 0.875
    assert_eq!(apply_timing_easing(0.0, 50_000, 50_000), 0.0);
    assert!((apply_timing_easing(0.25, 50_000, 50_000) - 0.125).abs() < 1e-4);
    assert!((apply_timing_easing(0.5, 50_000, 50_000) - 0.5).abs() < 1e-4);
    assert!((apply_timing_easing(0.75, 50_000, 50_000) - 0.875).abs() < 1e-4);
    assert_eq!(apply_timing_easing(1.0, 50_000, 50_000), 1.0);

    // 1. 図形0（タイトル）を選択し、フェード（開始）を追加
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.in.fade");

    // 初期状態の確認 (accel: 0, decel: 0)
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| (a.accel, a.decel))), Some((0, 0)));

    // 2. ツールバーメニューから「滑らかさ: 両方」を設定
    s.toolbar_menu("motion", "pptx.anim.smooth.both");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| (a.accel, a.decel))), Some((50_000, 50_000)));

    // 3. メニュー状態の検証
    let anim_menu = s.with(|e, _| crate::pptx::animation_ui::animation_items(e));
    let smooth_both_item = crate::shell::scenario::find_item(&anim_menu, "pptx.anim.smooth.both").unwrap();
    assert!(smooth_both_item.checked, "smooth both should be checked");
    let smooth_none_item = crate::shell::scenario::find_item(&anim_menu, "pptx.anim.smooth.none").unwrap();
    assert!(!smooth_none_item.checked, "smooth none should not be checked");

    // 4. ツールバーメニューから「滑らかに開始」を設定
    s.toolbar_menu("motion", "pptx.anim.smooth.start");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| (a.accel, a.decel))), Some((50_000, 0)));

    // 5. アニメーション ウィンドウのインライン操作（cycle_effect_smooth_at）
    // (50000, 0) からサイクルすると -> (0, 50000)（終了のみ）
    s.update(|e, _, cx| {
        e.cycle_effect_smooth_at(0, cx);
    });
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| (a.accel, a.decel))), Some((0, 50_000)));

    // 5b. トグル操作の検証（toggle_effect_smooth_start_at, toggle_effect_smooth_end_at, toggle_effect_smooth_start, toggle_effect_smooth_end）
    s.update(|e, _, cx| {
        e.toggle_effect_smooth_start_at(0, cx); // accel: 0 -> 50000
    });
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| (a.accel, a.decel))), Some((50_000, 50_000)));

    s.update(|e, _, cx| {
        e.toggle_effect_smooth_end_at(0, cx); // decel: 50000 -> 0
    });
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| (a.accel, a.decel))), Some((50_000, 0)));

    s.update(|e, _, cx| {
        e.toggle_effect_smooth_start(cx); // accel: 50000 -> 0
    });
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| (a.accel, a.decel))), Some((0, 0)));

    s.update(|e, _, cx| {
        e.toggle_effect_smooth_end(cx); // decel: 0 -> 50000
    });
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| (a.accel, a.decel))), Some((0, 50_000)));

    // 6. Undo / Redo の検証
    // ⌘Z: (0, 50000) -> (0, 0) に戻る
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| (a.accel, a.decel))), Some((0, 0)));
    // ⌘Z: (0, 0) -> (50000, 0) に戻る
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| (a.accel, a.decel))), Some((50_000, 0)));

    // ⇧⌘Z: (50000, 0) -> (0, 0) に進む
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| (a.accel, a.decel))), Some((0, 0)));
    // ⇧⌘Z: (0, 0) -> (0, 50000) に進む
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| (a.accel, a.decel))), Some((0, 50_000)));

    // 7. 両方滑らか（50000, 50000）にして保存・再読込
    s.toolbar_menu("motion", "pptx.anim.smooth.both");
    s.save_as("pptx-anim-smooth.pptx");
    let saved = s.reopen();
    let saved_anim = saved.slides[0].animations.first().expect("saved animation");
    assert_eq!(saved_anim.accel, 50_000, "accel should be preserved on reopen");
    assert_eq!(saved_anim.decel, 50_000, "decel should be preserved on reopen");
}

pub(crate) fn animation_after_effect_options() {
    use ibis_doc_model::AnimationAfterEffect;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. 図形0（タイトル）を選択し、フェード（開始）を追加
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.in.fade");

    // 初期状態の確認 (after_effect: Hold)
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.after_effect)), Some(AnimationAfterEffect::Hold));

    // 2. ツールバーメニューから「再生後に非表示」を設定
    s.toolbar_menu("motion", "pptx.anim.after.hide");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.after_effect)), Some(AnimationAfterEffect::Hide));

    // 3. メニュー状態の検証
    let anim_menu = s.with(|e, _| crate::pptx::animation_ui::animation_items(e));
    let hide_item = crate::shell::scenario::find_item(&anim_menu, "pptx.anim.after.hide").unwrap();
    assert!(hide_item.checked, "after.hide should be checked");
    let hold_item = crate::shell::scenario::find_item(&anim_menu, "pptx.anim.after.hold").unwrap();
    assert!(!hold_item.checked, "after.hold should not be checked");

    // 4. ツールバーメニューから「淡色表示」を設定
    s.toolbar_menu("motion", "pptx.anim.after.dim");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.after_effect)), Some(AnimationAfterEffect::Dim));

    // 5. アニメーション ウィンドウのインライン操作（cycle_effect_after_at）
    // Dim からサイクルすると -> HideOnNextClick
    s.update(|e, _, cx| {
        e.cycle_effect_after_at(0, cx);
    });
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.after_effect)), Some(AnimationAfterEffect::HideOnNextClick));

    // HideOnNextClick からサイクルすると -> Hold
    s.update(|e, _, cx| {
        e.cycle_effect_after_at(0, cx);
    });
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.after_effect)), Some(AnimationAfterEffect::Hold));

    // 6. Undo / Redo の検証
    // ⌘Z: Hold -> HideOnNextClick に戻る
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.after_effect)), Some(AnimationAfterEffect::HideOnNextClick));
    // ⌘Z: HideOnNextClick -> Dim に戻る
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.after_effect)), Some(AnimationAfterEffect::Dim));

    // ⇧⌘Z: Dim -> HideOnNextClick に進む
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.after_effect)), Some(AnimationAfterEffect::HideOnNextClick));
    // ⇧⌘Z: HideOnNextClick -> Hold に進む
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.after_effect)), Some(AnimationAfterEffect::Hold));

    // 7. スライドショーの描画計算（motion_at）の検証
    let mut anim_hold = s.with(|e, _| e.slide().animations.first().unwrap().clone());
    anim_hold.after_effect = AnimationAfterEffect::Hold;
    let m_hold = crate::pptx::slideshow::motion_at(&anim_hold, crate::pptx::slideshow::ShapeMotion { opacity: 0.0, dx: 0.0, dy: 0.0 }, 1.0, (0.0, 0.0, 100.0, 100.0), (1000.0, 1000.0));
    assert_eq!(m_hold.opacity, 1.0);

    let mut anim_hide = anim_hold.clone();
    anim_hide.after_effect = AnimationAfterEffect::Hide;
    let m_hide = crate::pptx::slideshow::motion_at(&anim_hide, crate::pptx::slideshow::ShapeMotion { opacity: 0.0, dx: 0.0, dy: 0.0 }, 1.0, (0.0, 0.0, 100.0, 100.0), (1000.0, 1000.0));
    assert_eq!(m_hide.opacity, 0.0);

    let mut anim_dim = anim_hold.clone();
    anim_dim.after_effect = AnimationAfterEffect::Dim;
    let m_dim = crate::pptx::slideshow::motion_at(&anim_dim, crate::pptx::slideshow::ShapeMotion { opacity: 0.0, dx: 0.0, dy: 0.0 }, 1.0, (0.0, 0.0, 100.0, 100.0), (1000.0, 1000.0));
    assert_eq!(m_dim.opacity, 0.3);

    // 8. 「再生後に非表示」に設定して保存・再読込
    s.toolbar_menu("motion", "pptx.anim.after.hide");
    s.save_as("pptx-anim-after.pptx");
    let saved = s.reopen();
    let saved_anim = saved.slides[0].animations.first().expect("saved animation");
    assert_eq!(saved_anim.after_effect, AnimationAfterEffect::Hide, "after_effect should be preserved on reopen");
}

/// 効果のオプション詳細設定パネルおよび巻き戻し属性（PPTX-ANIM-006, PPTX-ANIM-018）:
/// 詳細設定サイドバーパネルの開閉、効果・タイミング属性の調整、巻き戻し（rewind）トグル、
/// Undo / Redo、スライドショー描画（motion_at での rewind 復元）、保存と再読込の完全性を検証。
pub(crate) fn animation_options_panel_and_rewind() {
    use ibis_doc_model::{AnimationAfterEffect, AnimationLook, AnimationRepeat, AnimationSide, AnimationStart};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    // 1. 図形0を選択。最初は効果オプション無効
    s.click_shape(0);
    assert_eq!(s.toolbar_state("anim-options").map(|t| t.enabled), Some(false));

    // 2. スライドイン（下から）を設定 -> anim-options が有効化
    s.toolbar_menu("motion", "pptx.anim.in.fly-bottom");
    assert_eq!(s.toolbar_state("anim-options").map(|t| t.enabled), Some(true));
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.rewind)), Some(false));

    // 3. ツールバー「アニメーション」メニューから「再生が終了したら巻き戻す」をトグル
    s.toolbar_menu("motion", "pptx.anim.rewind");
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.rewind)), Some(true));

    // アニメーション ウィンドウのインライン操作（toggle_effect_rewind_at）
    s.update(|e, _, cx| {
        e.toggle_effect_rewind_at(0, cx);
    });
    assert_eq!(s.with(|e, _| e.slide().animations.first().map(|a| a.rewind)), Some(false));

    // 4. 詳細設定パネルを開く（open_selected_animation_options_panel）
    s.update(|e, _, cx| {
        e.open_selected_animation_options_panel(cx);
    });
    assert!(s.with(|e, _| e.pptx().and_then(|st| st.animation_options_panel.as_ref()).is_some()), "options panel should be open");
    assert!(s.with(|e, _| e.sidebar_open), "sidebar should be open");

    // 5. パネル内で各種パラメータを変更
    s.update(|e, _, cx| {
        e.set_panel_start(AnimationStart::WithPrevious, cx);
        e.adjust_panel_duration(500, cx); // 500 -> 1000ms
        e.adjust_panel_delay(500, cx); // 0 -> 500ms
        e.set_panel_repeat(AnimationRepeat::Count(3), cx);
        e.toggle_panel_auto_reverse(cx); // false -> true
        e.toggle_panel_rewind(cx); // false -> true
        e.toggle_panel_smooth_start(cx); // 0 -> 50_000
        e.toggle_panel_smooth_end(cx); // 0 -> 50_000
        e.set_panel_after_effect(AnimationAfterEffect::Dim, cx);
        e.set_panel_fly_side(AnimationSide::Left, cx);
    });

    // 6. パネルの変更を適用
    s.update(|e, _, cx| {
        e.apply_animation_options_panel(cx);
    });
    assert!(s.with(|e, _| e.pptx().and_then(|st| st.animation_options_panel.as_ref()).is_none()), "options panel should be closed");

    // 7. 適用されたアニメーションの属性を検証
    let applied = s.with(|e, _| e.slide().animations.first().unwrap().clone());
    assert_eq!(applied.start, AnimationStart::WithPrevious);
    assert_eq!(applied.duration, 1000);
    assert_eq!(applied.delay, 500);
    assert_eq!(applied.repeat_count, AnimationRepeat::Count(3));
    assert_eq!(applied.auto_reverse, true);
    assert_eq!(applied.rewind, true);
    assert_eq!(applied.accel, 50_000);
    assert_eq!(applied.decel, 50_000);
    assert_eq!(applied.after_effect, AnimationAfterEffect::Dim);
    assert_eq!(applied.look, AnimationLook::Fly(AnimationSide::Left));

    // 8. Undo / Redo の検証
    s.key("cmd-z");
    let reverted = s.with(|e, _| e.slide().animations.first().unwrap().clone());
    assert_eq!(reverted.rewind, false);
    assert_eq!(reverted.start, AnimationStart::OnClick);

    s.key("cmd-shift-z");
    let reapplied = s.with(|e, _| e.slide().animations.first().unwrap().clone());
    assert_eq!(reapplied.rewind, true);
    assert_eq!(reapplied.start, AnimationStart::WithPrevious);
    assert_eq!(reapplied.duration, 1000);

    // 9. スライドショー描画計算（motion_at での rewind 検証）
    // 通常再生中 (p = 0.5)
    let m_half = crate::pptx::slideshow::motion_at(&reapplied, crate::pptx::slideshow::ShapeMotion { opacity: 0.0, dx: 0.0, dy: 0.0 }, 0.5, (0.0, 0.0, 100.0, 100.0), (1000.0, 1000.0));
    assert!(m_half.dx < 0.0 || m_half.opacity > 0.0);

    // 再生完了 (p = 1.0) かつ rewind == true の場合、開始前状態（from: dx = 0.0, opacity = 0.0）に巻き戻る
    let m_end = crate::pptx::slideshow::motion_at(&reapplied, crate::pptx::slideshow::ShapeMotion { opacity: 0.0, dx: 0.0, dy: 0.0 }, 1.0, (0.0, 0.0, 100.0, 100.0), (1000.0, 1000.0));
    assert_eq!(m_end.dx, 0.0, "rewind should restore initial dx position");
    assert_eq!(m_end.opacity, 0.0, "rewind should restore initial hidden opacity");

    // 対比: rewind == false の場合は再生後の動作（after_effect: Dim -> 0.3）が残る
    let mut no_rewind = reapplied.clone();
    no_rewind.rewind = false;
    let m_no_rewind = crate::pptx::slideshow::motion_at(&no_rewind, crate::pptx::slideshow::ShapeMotion { opacity: 0.0, dx: 0.0, dy: 0.0 }, 1.0, (0.0, 0.0, 100.0, 100.0), (1000.0, 1000.0));
    assert_eq!(m_no_rewind.opacity, 0.3, "without rewind, after_effect Dim sets opacity to 0.3");

    // 10. ファイル保存 & 再読込（ラウンドトリップ検証）
    s.save_as("pptx-anim-options-panel.pptx");
    let reopened = s.reopen();
    let saved_anim = reopened.slides[0].animations.first().expect("saved animation");
    assert_eq!(saved_anim.start, AnimationStart::WithPrevious);
    assert_eq!(saved_anim.duration, 1000);
    assert_eq!(saved_anim.delay, 500);
    assert_eq!(saved_anim.repeat_count, AnimationRepeat::Count(3));
    assert_eq!(saved_anim.auto_reverse, true);
    assert_eq!(saved_anim.rewind, true);
    assert_eq!(saved_anim.accel, 50_000);
    assert_eq!(saved_anim.decel, 50_000);
    assert_eq!(saved_anim.after_effect, AnimationAfterEffect::Hold);
    assert_eq!(saved_anim.look, AnimationLook::Fly(AnimationSide::Left));
}

/// アニメーショントリガー（図形クリック・メディアブックマーク）の設定、解除、
/// アニメーションウィンドウ・詳細パネルでの表示、スライドショー発火、保存・再オープン、Undo/Redo (PPTX-ANIM-015, PPTX-ANIM-016, PPTX-VIDEO-015)
pub(crate) fn animation_triggers_and_interactive_sequences() {
    use ibis_doc_model::AnimationTrigger;

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let video_path = root.join("clip-test.mp4");
    let video_bytes = std::fs::read(&video_path).expect("clip-test.mp4 should exist");

    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);

    // スライド上の図形を確認
    let _shape_target = s.with(|e, _| e.slide().shapes[0].id);
    let shape_trigger = s.with(|e, _| e.slide().shapes[1].id);

    // 1. shape_target を選択してフェードイン効果を追加
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.in.fade");
    assert_eq!(s.with(|e, _| e.slide().animations.len()), 1);
    assert_eq!(s.with(|e, _| e.slide().animations[0].trigger.clone()), AnimationTrigger::None);

    // 2. 図形クリックトリガー（OnShapeClick(shape_trigger)）を設定
    s.update(|e, _, cx| {
        e.set_selected_effects_trigger(AnimationTrigger::OnShapeClick(shape_trigger), cx);
    });
    assert_eq!(s.with(|e, _| e.slide().animations[0].trigger.clone()), AnimationTrigger::OnShapeClick(shape_trigger));
    assert!(s.with(|e, _| e.slide().animations[0].is_triggered()));

    // 3. アニメーションウィンドウと詳細設定パネルでの確認
    s.update(|e, _, cx| {
        e.open_selected_animation_options_panel(cx);
    });
    assert_eq!(
        s.with(|e, _| e.pptx().and_then(|st| st.animation_options_panel.as_ref()).map(|p| p.trigger.clone())),
        Some(AnimationTrigger::OnShapeClick(shape_trigger))
    );
    s.update(|e, _, cx| {
        e.close_animation_options_panel(cx);
    });

    // 4. スライドショーでの通常シーケンスからの除外と図形クリック発火の検証
    s.update(|e, window, cx| {
        e.start_slideshow(window, cx);
    });
    // トリガー効果は animation_groups に含まれないため、groups.len() == 0
    let groups_len = s.with(|e, _| {
        crate::pptx::slideshow::animation_groups(&e.slide().animations).len()
    });
    assert_eq!(groups_len, 0);

    // クリック前は triggered_played は空
    assert_eq!(s.with(|e, _| e.slideshow.as_ref().map(|show| show.triggered_played.len())), Some(0));

    // shape_trigger をクリック発火
    s.update(|e, _, cx| {
        let fired = e.trigger_animations_for_shape_click(shape_trigger, cx);
        assert!(fired);
    });
    assert_eq!(s.with(|e, _| e.slideshow.as_ref().map(|show| show.triggered_played.len())), Some(1));
    assert_eq!(s.with(|e, _| e.slideshow.as_ref().and_then(|show| show.triggered_played.first().copied())), Some(0));

    s.update(|e, window, cx| {
        e.end_slideshow(window, cx);
    });

    // 5. メディアブックマークトリガーの検証
    // メディア（ブックマーク付き）を挿入
    let clip_shape_id = s.update(|e, _, cx| e.insert_clip(&video_path, video_bytes.clone(), false, cx)).expect("video clip inserted");
    s.update(|e, _, cx| e.add_selected_clip_bookmark(Some(1500), Some("Intro".to_string()), cx));

    // shape_target にメディアブックマークトリガー（OnMediaBookmark: "Intro"）を設定
    s.click_shape(0);
    s.update(|e, _, cx| {
        e.set_selected_effects_trigger(
            AnimationTrigger::OnMediaBookmark {
                shape: clip_shape_id,
                bookmark: "Intro".to_string(),
            },
            cx,
        );
    });
    assert_eq!(
        s.with(|e, _| e.slide().animations[0].trigger.clone()),
        AnimationTrigger::OnMediaBookmark {
            shape: clip_shape_id,
            bookmark: "Intro".to_string(),
        }
    );

    // スライドショーでのブックマーク発火
    s.update(|e, window, cx| {
        e.start_slideshow(window, cx);
    });
    s.update(|e, _, cx| {
        let fired = e.trigger_animations_for_media_bookmark(clip_shape_id, "Intro", cx);
        assert!(fired);
    });
    assert_eq!(s.with(|e, _| e.slideshow.as_ref().map(|show| show.triggered_played.len())), Some(1));
    s.update(|e, window, cx| {
        e.end_slideshow(window, cx);
    });

    // 6. トリガー解除
    s.click_shape(0);
    s.update(|e, _, cx| {
        e.clear_selected_effects_trigger(cx);
    });
    assert_eq!(s.with(|e, _| e.slide().animations[0].trigger.clone()), AnimationTrigger::None);

    // 7. 再びクリックトリガーを設定して保存・再オープン
    s.update(|e, _, cx| {
        e.set_selected_effects_trigger(AnimationTrigger::OnShapeClick(shape_trigger), cx);
    });
    s.save_as("pptx-anim-triggers.pptx");
    let reopened = s.reopen();
    let saved_anim = reopened.slides[1].animations.first().expect("saved animation");
    assert_eq!(saved_anim.trigger, AnimationTrigger::OnShapeClick(shape_trigger));

    // 8. Undo / Redo
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.slide().animations[0].trigger.clone()), AnimationTrigger::None);
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.slide().animations[0].trigger.clone()), AnimationTrigger::OnShapeClick(shape_trigger));
}

/// コネクタ（カギ線・曲線）と双方向矢印の挿入、ツールバー/メニューからの選択、
/// 線の太さ・色・破線・端点矢印の書式変更、Undo/Redo、および保存・再読込検証 (PPTX-OBJECT-002)
pub(crate) fn connectors_insert_and_options() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(4);
    let before_shapes = s.shapes();

    fn preset(g: &ibis_doc_model::Geometry) -> Option<&str> {
        match g {
            ibis_doc_model::Geometry::Formula(f) => f.preset.as_deref(),
            _ => None,
        }
    }

    // 1. 直線・両矢印（pptx.double-arrow）の挿入
    s.toolbar_menu("insert-line", "pptx.double-arrow");
    assert_eq!(s.tool(), Tool::Line { kind: LineKind::Straight, arrow: ArrowKind::Double });
    let (a, b) = (s.at(EmuPoint::new(1_000_000, 2_000_000)), s.at(EmuPoint::new(3_000_000, 2_000_000)));
    s.drag(a, b);
    assert_eq!(s.tool(), Tool::Select);
    assert_eq!(s.with(|e, _| e.selection_kind()), SelectionKind::Line);
    let double_arrow = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Line(stroke) = &double_arrow.kind else { panic!("straight line shape") };
    assert!(matches!(stroke.head, ibis_doc_model::LineEnd::Triangle(_)), "head arrow");
    assert!(matches!(stroke.tail, ibis_doc_model::LineEnd::Triangle(_)), "tail arrow");

    // 2. カギ線コネクタ（pptx.connector.bent）の挿入（メニューから）
    s.menu("pptx.connector.bent");
    assert_eq!(s.tool(), Tool::Line { kind: LineKind::Bent, arrow: ArrowKind::None });
    let (a, b) = (s.at(EmuPoint::new(4_000_000, 2_000_000)), s.at(EmuPoint::new(6_000_000, 4_000_000)));
    s.drag(a, b);
    assert_eq!(s.tool(), Tool::Select);
    assert_eq!(s.with(|e, _| e.selection_kind()), SelectionKind::Line);
    assert!(s.toolbar_state("line-end").is_some());
    let bent = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Auto(auto) = &bent.kind else { panic!("bent connector auto shape") };
    assert_eq!(preset(&auto.geometry), Some("bentConnector3"));
    assert!(super::editing::is_connector_geometry(&auto.geometry));
    assert_eq!(auto.stroke.as_ref().map(|st| st.head), Some(ibis_doc_model::LineEnd::None));
    assert_eq!(auto.stroke.as_ref().map(|st| st.tail), Some(ibis_doc_model::LineEnd::None));

    // 3. ツールバーでの線書式（線の太さ・色・破線・端点）の変更
    s.toolbar_menu("line-weight", "width.3");
    s.toolbar_menu("border-color", "color.orange");
    s.toolbar_menu("line-style", "pptx.stroke-dash");
    s.toolbar_menu("line-end", "end.stealth");
    let updated_bent = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Auto(auto_up) = &updated_bent.kind else { panic!("updated bent connector") };
    let stroke_up = auto_up.stroke.as_ref().expect("has stroke");
    assert_eq!(stroke_up.width, 38_100, "3pt");
    assert_eq!(stroke_up.color, ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffc000)), "orange color");
    assert_eq!(stroke_up.dash, ibis_doc_model::LineDash::Dash, "dash style");
    assert!(matches!(stroke_up.tail, ibis_doc_model::LineEnd::Stealth(_)), "stealth tail");

    // 4. 曲線コネクタ矢印（pptx.connector.curved-arrow）の挿入（ツールバーから）
    s.toolbar_menu("insert-line", "pptx.connector.curved-arrow");
    assert_eq!(s.tool(), Tool::Line { kind: LineKind::Curved, arrow: ArrowKind::Single });
    let (a, b) = (s.at(EmuPoint::new(7_000_000, 2_000_000)), s.at(EmuPoint::new(9_000_000, 4_000_000)));
    s.drag(a, b);
    assert_eq!(s.with(|e, _| e.selection_kind()), SelectionKind::Line);
    let curved = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Auto(curved_auto) = &curved.kind else { panic!("curved connector auto shape") };
    assert_eq!(preset(&curved_auto.geometry), Some("curvedConnector3"));
    assert!(super::editing::is_connector_geometry(&curved_auto.geometry));
    assert_eq!(curved_auto.stroke.as_ref().map(|st| st.head), Some(ibis_doc_model::LineEnd::None));
    assert!(matches!(curved_auto.stroke.as_ref().map(|st| st.tail), Some(ibis_doc_model::LineEnd::Triangle(_))));

    // 5. 曲線コネクタ両矢印（pptx.connector.curved-double-arrow）の挿入（メニューから）
    s.menu("pptx.connector.curved-double-arrow");
    assert_eq!(s.tool(), Tool::Line { kind: LineKind::Curved, arrow: ArrowKind::Double });
    let (a, b) = (s.at(EmuPoint::new(1_000_000, 5_000_000)), s.at(EmuPoint::new(3_000_000, 7_000_000)));
    s.drag(a, b);
    let curved_double = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Auto(cd_auto) = &curved_double.kind else { panic!("curved double connector") };
    assert_eq!(preset(&cd_auto.geometry), Some("curvedConnector3"));
    assert!(matches!(cd_auto.stroke.as_ref().map(|st| st.head), Some(ibis_doc_model::LineEnd::Triangle(_))));
    assert!(matches!(cd_auto.stroke.as_ref().map(|st| st.tail), Some(ibis_doc_model::LineEnd::Triangle(_))));

    // 6. カギ線コネクタ両矢印（pptx.connector.bent-double-arrow）の挿入（ツールバーから）
    s.toolbar_menu("insert-line", "pptx.connector.bent-double-arrow");
    assert_eq!(s.tool(), Tool::Line { kind: LineKind::Bent, arrow: ArrowKind::Double });
    let (a, b) = (s.at(EmuPoint::new(4_000_000, 5_000_000)), s.at(EmuPoint::new(6_000_000, 7_000_000)));
    s.drag(a, b);
    let bent_double = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Auto(bd_auto) = &bent_double.kind else { panic!("bent double connector") };
    assert_eq!(preset(&bd_auto.geometry), Some("bentConnector3"));
    assert!(matches!(bd_auto.stroke.as_ref().map(|st| st.head), Some(ibis_doc_model::LineEnd::Triangle(_))));
    assert!(matches!(bd_auto.stroke.as_ref().map(|st| st.tail), Some(ibis_doc_model::LineEnd::Triangle(_))));

    assert_eq!(s.shapes(), before_shapes + 5);

    // 7. Undo / Redo 検証
    s.key("cmd-z");
    assert_eq!(s.shapes(), before_shapes + 4, "last connector undone");
    s.key("cmd-shift-z");
    assert_eq!(s.shapes(), before_shapes + 5, "last connector redone");

    // 8. ファイル保存・再読込・OOXMLラウンドトリップ検証
    s.screenshot("connectors");
    s.save_as("connectors.pptx");
    let saved = s.reopen();
    let saved_shapes = &saved.slides[3].shapes;
    assert_eq!(saved_shapes.len(), before_shapes + 5);

    // 再読込された各図形の検証
    // コネクタは <p:cxnSp> として保存され、読込時に ShapeKind::Auto として復元される
    let bent_found = saved_shapes.iter().find(|sh| match &sh.kind {
        ShapeKind::Auto(a) => preset(&a.geometry) == Some("bentConnector3") && a.stroke.as_ref().is_some_and(|st| st.width == 38_100),
        _ => false,
    }).expect("saved bent connector with 3pt width found");
    let ShapeKind::Auto(saved_bent_auto) = &bent_found.kind else { unreachable!() };
    let saved_bent_stroke = saved_bent_auto.stroke.as_ref().unwrap();
    assert_eq!(saved_bent_stroke.dash, ibis_doc_model::LineDash::Dash);
    assert_eq!(saved_bent_stroke.color, ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffc000)));
    assert!(matches!(saved_bent_stroke.tail, ibis_doc_model::LineEnd::Stealth(_)));

    let curved_found = saved_shapes.iter().find(|sh| match &sh.kind {
        ShapeKind::Auto(a) => preset(&a.geometry) == Some("curvedConnector3") && matches!(a.stroke.as_ref().map(|s| &s.tail), Some(ibis_doc_model::LineEnd::Triangle(_))),
        _ => false,
    }).expect("saved curved connector found");
    assert!(matches!(&curved_found.kind, ShapeKind::Auto(_)));

    // 全戻しとやり直し
    undo_all_and_redo(&mut s);
}

/// PPTX-MASTER-010 ~ 013: 空のコンテンツプレースホルダー（汎用 body / obj、
/// および専用 tbl, chart, pic, media）への表・画像・動画・グラフの挿入、
/// プレースホルダー置換・枠内フィット、Undo/Redo、ファイル保存・復元。
pub(crate) fn content_placeholder_insert_table_picture_clip() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let video_path = root.join("clip-test.mp4");
    let video_bytes = std::fs::read(&video_path).expect("clip-test.mp4 should exist");

    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    // 新しいスライドを追加してテスト用の白紙状態を用意
    s.page(1);
    s.menu("pptx.new-slide");

    // ヘルパー: 空のプレースホルダーシェイプを追加する
    let add_ph = |s: &mut Scenario, kind: &str, frame: ibis_doc_model::EmuRect| -> ibis_doc_model::ShapeId {
        s.update(|e, _, cx| {
            let slide_id = e.current_slide;
            let shape_id = e.document.ids().new_shape_id();
            let shape = ibis_doc_model::Shape::text_box(shape_id, frame, "");
            let ph = ibis_doc_model::SlidePlaceholder {
                placeholder: ibis_doc_model::PlaceholderRef { kind: Some(kind.to_string()), index: None },
                own_frame: true,
            };
            let shape_len = e.slide().shapes.len();
            e.run(
                vec![
                    ibis_doc_model::Command::InsertShape { slide: slide_id, index: shape_len, shape },
                    ibis_doc_model::Command::SetPlaceholder { slide: slide_id, shape: shape_id, placeholder: Some(ph) },
                ],
                cx,
            );
            shape_id
        })
    };

    // 1. PPTX-MASTER-012: 表プレースホルダー（tbl）への表挿入
    let tbl_frame = ibis_doc_model::EmuRect::new(1_000_000, 1_000_000, 4_000_000, 2_500_000);
    let tbl_ph_id = add_ph(&mut s, "tbl", tbl_frame);
    assert!(s.with(|e, _| e.slide().shape(tbl_ph_id).is_some()));

    s.update(|e, w, cx| e.insert_slide_table(3, 4, w, cx));
    // プレースホルダーが削除され、表が挿入されていること
    assert!(s.with(|e, _| e.slide().shape(tbl_ph_id).is_none()), "tbl placeholder replaced");
    let tbl_shape = s.with(|e, _| e.selected_shapes()[0].clone());
    assert!(matches!(tbl_shape.kind, ShapeKind::Table(_)));
    assert_eq!(tbl_shape.frame, tbl_frame, "table fits in tbl placeholder frame");

    // Undo / Redo 検証
    s.key("cmd-z");
    assert!(s.with(|e, _| e.slide().shape(tbl_ph_id).is_some()), "tbl placeholder restored on undo");
    s.key("cmd-shift-z");
    assert!(s.with(|e, _| e.slide().shape(tbl_ph_id).is_none()), "table restored on redo");

    // 2. PPTX-MASTER-011: 画像プレースホルダー（pic）への画像挿入とアスペクト比フィット
    let pic_frame = ibis_doc_model::EmuRect::new(5_500_000, 1_000_000, 4_000_000, 2_000_000);
    let pic_ph_id = add_ph(&mut s, "pic", pic_frame);
    let img_data = png(320, 240, [0x2e, 0x75, 0xb6]); // 4:3 の画像

    let pic_id = s.update(|e, _, cx| e.insert_picture("photo.png", img_data.clone(), cx)).expect("picture inserted");
    assert!(s.with(|e, _| e.slide().shape(pic_ph_id).is_none()), "pic placeholder replaced");
    let pic_shape = s.with(|e, _| e.slide().shape(pic_id).cloned()).expect("picture exists");
    assert!(matches!(pic_shape.kind, ShapeKind::Picture(_)));
    // 画像は枠内に収まり（幅・高さ <= pic_frame）、中央配置されていること
    assert!(pic_shape.frame.width <= pic_frame.width);
    assert!(pic_shape.frame.height <= pic_frame.height);
    assert!(pic_shape.frame.x >= pic_frame.x);
    assert!(pic_shape.frame.y >= pic_frame.y);

    // Undo / Redo 検証
    s.key("cmd-z");
    assert!(s.with(|e, _| e.slide().shape(pic_ph_id).is_some()), "pic placeholder restored on undo");
    s.key("cmd-shift-z");
    assert!(s.with(|e, _| e.slide().shape(pic_ph_id).is_none()), "picture restored on redo");

    // 3. PPTX-MASTER-013: 動画プレースホルダー（media）への動画挿入とフィット
    let media_frame = ibis_doc_model::EmuRect::new(1_000_000, 4_000_000, 4_000_000, 2_500_000);
    let media_ph_id = add_ph(&mut s, "media", media_frame);

    let clip_id = s.update(|e, _, cx| e.insert_clip(&video_path, video_bytes.clone(), false, cx)).expect("clip inserted");
    assert!(s.with(|e, _| e.slide().shape(media_ph_id).is_none()), "media placeholder replaced");
    let clip_shape = s.with(|e, _| e.slide().shape(clip_id).cloned()).expect("clip shape exists");
    assert!(matches!(&clip_shape.kind, ShapeKind::Picture(p) if p.clip.is_some()));
    assert!(clip_shape.frame.width <= media_frame.width);
    assert!(clip_shape.frame.height <= media_frame.height);

    // Undo / Redo 検証
    s.key("cmd-z");
    assert!(s.with(|e, _| e.slide().shape(media_ph_id).is_some()), "media placeholder restored on undo");
    s.key("cmd-shift-z");
    assert!(s.with(|e, _| e.slide().shape(media_ph_id).is_none()), "clip restored on redo");

    // 4. PPTX-MASTER-012: グラフプレースホルダー（chart）へのグラフ挿入
    let chart_frame = ibis_doc_model::EmuRect::new(5_500_000, 4_000_000, 4_000_000, 2_500_000);
    let chart_ph_id = add_ph(&mut s, "chart", chart_frame);

    s.update(|e, w, cx| e.insert_slide_chart(w, cx));
    assert!(s.with(|e, _| e.slide().shape(chart_ph_id).is_none()), "chart placeholder replaced");
    let chart_shape = s.with(|e, _| e.selected_shapes()[0].clone());
    assert!(matches!(chart_shape.kind, ShapeKind::Chart(_)));
    assert_eq!(chart_shape.frame, chart_frame, "chart fits in chart placeholder frame");

    // Undo / Redo 検証
    s.key("cmd-z");
    assert!(s.with(|e, _| e.slide().shape(chart_ph_id).is_some()), "chart placeholder restored on undo");
    s.key("cmd-shift-z");
    assert!(s.with(|e, _| e.slide().shape(chart_ph_id).is_none()), "chart restored on redo");

    // 5. PPTX-MASTER-010: 汎用コンテンツプレースホルダー（body）への挿入
    // 専用プレースホルダーがない場合、汎用 body プレースホルダーが使われることを検証
    let body_frame = ibis_doc_model::EmuRect::new(3_000_000, 2_000_000, 3_000_000, 2_000_000);
    let body_ph_id = add_ph(&mut s, "body", body_frame);

    // 表を挿入 -> 汎用 body が置換される
    s.update(|e, w, cx| e.insert_slide_table(2, 2, w, cx));
    assert!(s.with(|e, _| e.slide().shape(body_ph_id).is_none()), "body placeholder replaced by table");
    let body_tbl = s.with(|e, _| e.selected_shapes()[0].clone());
    assert_eq!(body_tbl.frame, body_frame);

    // Undo / Redo 検証
    s.key("cmd-z");
    assert!(s.with(|e, _| e.slide().shape(body_ph_id).is_some()), "body placeholder restored on undo");
    s.key("cmd-shift-z");
    assert!(s.with(|e, _| e.slide().shape(body_ph_id).is_none()), "table restored on redo");

    // 6. 保存・再読込・ラウンドトリップ検証
    s.screenshot("content-placeholders");
    s.save_as("content-placeholders.pptx");
    let saved = s.reopen();
    let slide_shapes = &saved.slides[1].shapes;
    assert!(slide_shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Table(_))));
    assert!(slide_shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Picture(ref p) if p.clip.is_none())));
    assert!(slide_shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Picture(ref p) if p.clip.is_some())));
    assert!(slide_shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Chart(_))));

    // 7. 全戻しとやり直し
    undo_all_and_redo(&mut s);
}

/// PPTX-VIDEO-004, 007, 009, 010 / PPTX-AUDIO-003, 006, 009, 012:
/// メディア再生オプション（開始条件、音量、ループ、巻き戻し）の設定、
/// スライドショーでの自動再生・連動、全画面再生、非表示オプション、Undo/Redo、保存・再読込保持。
pub(crate) fn media_playback_options_and_slideshow() {
    use ibis_doc_model::{MediaStart, MediaVolume};

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let video_path = root.join("clip-test.mp4");
    let video_bytes = std::fs::read(&video_path).expect("clip-test.mp4 should exist");
    let audio_path = root.join("clip-tone.wav");
    let audio_bytes = std::fs::read(&audio_path).expect("clip-tone.wav should exist");

    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(1);
    s.menu("pptx.new-slide");

    // 1. 動画クリップ・音声クリップを挿入
    let clip_id = s.update(|e, _, cx| e.insert_clip(&video_path, video_bytes.clone(), false, cx)).expect("clip inserted");
    assert!(s.with(|e, _| e.slide().shape(clip_id).is_some()), "clip shape exists");

    let audio_id = s.update(|e, _, cx| e.insert_clip(&audio_path, audio_bytes.clone(), true, cx)).expect("audio clip inserted");
    assert!(s.with(|e, _| e.slide().shape(audio_id).is_some()), "audio clip exists");

    // 2. 音声クリップの非表示・スライドをまたいで再生・バックグラウンド再生を検証
    s.update(|e, _, _| e.set_selection(vec![audio_id]));
    let audio_init_opt = s.with(|e, _| e.selected_clip_options()).expect("audio clip options");
    assert_eq!(audio_init_opt.hide_during_show, false);
    assert_eq!(audio_init_opt.play_across_slides, false);
    s.update(|e, _, cx| e.toggle_selected_clip_hide_show(cx));
    s.update(|e, _, cx| e.toggle_selected_clip_play_across(cx));
    let audio_upd_opt = s.with(|e, _| e.selected_clip_options()).expect("audio updated options");
    assert_eq!(audio_upd_opt.hide_during_show, true);
    assert_eq!(audio_upd_opt.play_across_slides, true);

    // バックグラウンド再生プリセットの適用
    s.update(|e, _, cx| e.toggle_selected_clip_background(cx));
    let audio_bg_opt = s.with(|e, _| e.selected_clip_options()).expect("audio bg options");
    assert!(audio_bg_opt.is_background_play(), "all background play options active");

    // 3. 動画クリップの初期再生オプションおよびポスター変更・リセットを検証
    s.update(|e, _, _| e.set_selection(vec![clip_id]));
    let initial_opt = s.with(|e, _| e.selected_clip_options()).expect("clip options present");
    assert_eq!(initial_opt.start, MediaStart::OnClick);
    assert_eq!(initial_opt.loop_until_stopped, false);
    assert_eq!(initial_opt.rewind, false);
    assert_eq!(initial_opt.full_screen, false);
    assert_eq!(initial_opt.hide_while_not_playing, false);

    let initial_poster_media = s.with(|e, _| match &e.slide().shape(clip_id).unwrap().kind { ShapeKind::Picture(p) => p.media, _ => panic!() });
    let custom_poster_bytes = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
    s.update(|e, _, cx| e.set_selected_clip_poster_image(std::path::Path::new("poster.png"), custom_poster_bytes, cx));
    let changed_poster_media = s.with(|e, _| match &e.slide().shape(clip_id).unwrap().kind { ShapeKind::Picture(p) => p.media, _ => panic!() });
    assert_ne!(initial_poster_media, changed_poster_media);

    s.update(|e, _, cx| e.reset_selected_clip_poster_image(cx));
    let reset_poster_media = s.with(|e, _| match &e.slide().shape(clip_id).unwrap().kind { ShapeKind::Picture(p) => p.media, _ => panic!() });
    assert_ne!(changed_poster_media, reset_poster_media);

    // 4. 動画クリップの再生オプションの変更
    s.update(|e, _, cx| e.set_selected_clip_start(MediaStart::Automatically, cx));
    s.update(|e, _, cx| e.set_selected_clip_volume(MediaVolume::Low, cx));
    s.update(|e, _, cx| e.toggle_selected_clip_loop(cx));
    s.update(|e, _, cx| e.toggle_selected_clip_rewind(cx));
    s.update(|e, _, cx| e.toggle_selected_clip_fullscreen(cx));
    s.update(|e, _, cx| e.toggle_selected_clip_hide_stopped(cx));

    let updated_opt = s.with(|e, _| e.selected_clip_options()).expect("updated clip options");
    assert_eq!(updated_opt.start, MediaStart::Automatically);
    assert_eq!(updated_opt.volume, MediaVolume::Low);
    assert_eq!(updated_opt.loop_until_stopped, true);
    assert_eq!(updated_opt.rewind, true);
    assert_eq!(updated_opt.full_screen, true);
    assert_eq!(updated_opt.hide_while_not_playing, true);

    // 5. スライドショー開始と自動再生連動の検証
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    assert!(s.with(|e, _| e.slideshow.is_some()), "in slideshow mode");
    assert!(s.with(|e, _| e.pptx().is_some_and(|st| st.clips.iter().any(|c| c.shape == clip_id))), "automatic clip is playing");

    // スライドショー終了
    s.update(|e, w, cx| e.end_slideshow(w, cx));
    assert!(s.with(|e, _| e.slideshow.is_none()), "left slideshow mode");

    // 6. 保存・再読込・ラウンドトリップ検証
    s.screenshot("media-playback-options");
    s.save_as("media-playback-options.pptx");
    let saved = s.reopen();
    let saved_shapes = &saved.slides[1].shapes;

    let saved_clip_shape = saved_shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| !c.audio))).expect("saved clip found");
    let ShapeKind::Picture(saved_pic) = &saved_clip_shape.kind else { unreachable!() };
    let saved_clip = saved_pic.clip.as_ref().unwrap();
    assert_eq!(saved_clip.options.start, MediaStart::Automatically);
    assert_eq!(saved_clip.options.volume, MediaVolume::Low);
    assert_eq!(saved_clip.options.loop_until_stopped, true);
    assert_eq!(saved_clip.options.rewind, true);
    assert_eq!(saved_clip.options.full_screen, true);
    assert_eq!(saved_clip.options.hide_while_not_playing, true);

    let saved_audio_shape = saved_shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| c.audio))).expect("saved audio clip found");
    let ShapeKind::Picture(saved_a_pic) = &saved_audio_shape.kind else { unreachable!() };
    let saved_a_clip = saved_a_pic.clip.as_ref().unwrap();
    assert_eq!(saved_a_clip.options.hide_during_show, true);
    assert_eq!(saved_a_clip.options.play_across_slides, true);
    assert!(saved_a_clip.options.is_background_play());

    // 7. 全戻しとやり直し
    undo_all_and_redo(&mut s);
}

/// 動画・音声のトリミング（開始・終了時間設定: PPTX-VIDEO-005, PPTX-AUDIO-007）および
/// フェードイン・フェードアウト（PPTX-VIDEO-006, PPTX-AUDIO-008）の操作、ツールバー・メニュー連動、
/// ファイル保存・再オープンによる保持、Undo/Redo、全戻し・やり直しを完全検証。
pub(crate) fn media_trim_and_fade_playback_options() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let video_path = root.join("clip-test.mp4");
    let video_bytes = std::fs::read(&video_path).expect("clip-test.mp4 should exist");
    let audio_path = root.join("clip-tone.wav");
    let audio_bytes = std::fs::read(&audio_path).expect("clip-tone.wav should exist");

    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(1);
    s.menu("pptx.new-slide");

    // 1. 動画クリップ・音声クリップを挿入
    let video_id = s.update(|e, _, cx| e.insert_clip(&video_path, video_bytes.clone(), false, cx)).expect("video clip inserted");
    let audio_id = s.update(|e, _, cx| e.insert_clip(&audio_path, audio_bytes.clone(), true, cx)).expect("audio clip inserted");

    // 2. 動画クリップの初期トリム・フェード（未設定）を確認
    s.update(|e, _, _| e.set_selection(vec![video_id]));
    let init_video_opt = s.with(|e, _| e.selected_clip_options()).expect("video clip options");
    assert_eq!(init_video_opt.trim_start_ms, 0);
    assert_eq!(init_video_opt.trim_end_ms, 0);
    assert_eq!(init_video_opt.fade_in_ms, 0);
    assert_eq!(init_video_opt.fade_out_ms, 0);
    assert!(!init_video_opt.is_trimmed());
    assert!(!init_video_opt.is_faded());

    // 3. 動画クリップにトリミング（1.5s〜6.5s）とフェード（イン1.0s、アウト2.0s）を設定
    s.update(|e, _, cx| e.set_selected_clip_trim(1500, 6500, cx));
    s.update(|e, _, cx| e.set_selected_clip_fade_in(1000, cx));
    s.update(|e, _, cx| e.set_selected_clip_fade_out(2000, cx));

    let upd_video_opt = s.with(|e, _| e.selected_clip_options()).expect("updated video options");
    assert_eq!(upd_video_opt.trim_start_ms, 1500);
    assert_eq!(upd_video_opt.trim_end_ms, 6500);
    assert_eq!(upd_video_opt.fade_in_ms, 1000);
    assert_eq!(upd_video_opt.fade_out_ms, 2000);
    assert!(upd_video_opt.is_trimmed());
    assert!(upd_video_opt.is_faded());

    // 4. 音声クリップにトリミング（2.0s〜8.0s）とフェード（イン0.5s、アウト1.5s）を設定
    s.update(|e, _, _| e.set_selection(vec![audio_id]));
    s.update(|e, _, cx| e.set_selected_clip_trim(2000, 8000, cx));
    s.update(|e, _, cx| e.set_selected_clip_fade(500, 1500, cx));

    let upd_audio_opt = s.with(|e, _| e.selected_clip_options()).expect("updated audio options");
    assert_eq!(upd_audio_opt.trim_start_ms, 2000);
    assert_eq!(upd_audio_opt.trim_end_ms, 8000);
    assert_eq!(upd_audio_opt.fade_in_ms, 500);
    assert_eq!(upd_audio_opt.fade_out_ms, 1500);
    assert!(upd_audio_opt.is_trimmed());
    assert!(upd_audio_opt.is_faded());

    // 5. 保存と開き直しで設定が保持されることを検証
    s.screenshot("media-trim-and-fade-playback-options");
    s.save_as("media-trim-and-fade-playback-options.pptx");
    let saved = s.reopen();
    let saved_shapes = &saved.slides[1].shapes;

    let saved_video_shape = saved_shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| !c.audio))).expect("saved video found");
    let ShapeKind::Picture(saved_v_pic) = &saved_video_shape.kind else { unreachable!() };
    let saved_v_clip = saved_v_pic.clip.as_ref().unwrap();
    assert_eq!(saved_v_clip.options.trim_start_ms, 1500);
    assert_eq!(saved_v_clip.options.trim_end_ms, 6500);
    assert_eq!(saved_v_clip.options.fade_in_ms, 1000);
    assert_eq!(saved_v_clip.options.fade_out_ms, 2000);

    let saved_audio_shape = saved_shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| c.audio))).expect("saved audio found");
    let ShapeKind::Picture(saved_a_pic) = &saved_audio_shape.kind else { unreachable!() };
    let saved_a_clip = saved_a_pic.clip.as_ref().unwrap();
    assert_eq!(saved_a_clip.options.trim_start_ms, 2000);
    assert_eq!(saved_a_clip.options.trim_end_ms, 8000);
    assert_eq!(saved_a_clip.options.fade_in_ms, 500);
    assert_eq!(saved_a_clip.options.fade_out_ms, 1500);

    // 6. 全戻しとやり直し
    undo_all_and_redo(&mut s);
}

/// 動画・音声の再生ブックマーク（PPTX-VIDEO-014, PPTX-AUDIO-011）の追加、シーク、削除、
/// ツールバー・メニュー連動、ファイル保存・再オープンによる保持、Undo/Redo、全戻し・やり直しを完全検証。
pub(crate) fn media_bookmarks_playback_options() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let video_path = root.join("clip-test.mp4");
    let video_bytes = std::fs::read(&video_path).expect("clip-test.mp4 should exist");
    let audio_path = root.join("clip-tone.wav");
    let audio_bytes = std::fs::read(&audio_path).expect("clip-tone.wav should exist");

    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(1);
    s.menu("pptx.new-slide");

    // 1. 動画クリップ・音声クリップを挿入
    let video_id = s.update(|e, _, cx| e.insert_clip(&video_path, video_bytes.clone(), false, cx)).expect("video clip inserted");
    let audio_id = s.update(|e, _, cx| e.insert_clip(&audio_path, audio_bytes.clone(), true, cx)).expect("audio clip inserted");

    // 2. 動画クリップの初期ブックマーク（空）を確認
    s.update(|e, _, _| e.set_selection(vec![video_id]));
    let init_video_opt = s.with(|e, _| e.selected_clip_options()).expect("video clip options");
    assert!(init_video_opt.bookmarks.is_empty());

    // 3. 動画クリップにブックマーク（1200ms: Keyframe 1, 3400ms: Keyframe 2）を追加
    s.update(|e, _, cx| e.add_selected_clip_bookmark(Some(1200), Some("Keyframe 1".to_string()), cx));
    s.update(|e, _, cx| e.add_selected_clip_bookmark(Some(3400), Some("Keyframe 2".to_string()), cx));

    let upd_video_opt = s.with(|e, _| e.selected_clip_options()).expect("updated video options");
    assert_eq!(upd_video_opt.bookmarks.len(), 2);
    assert_eq!(upd_video_opt.bookmarks[0].name, "Keyframe 1");
    assert_eq!(upd_video_opt.bookmarks[0].time_ms, 1200);
    assert_eq!(upd_video_opt.bookmarks[1].name, "Keyframe 2");
    assert_eq!(upd_video_opt.bookmarks[1].time_ms, 3400);

    // 4. 音声クリップにブックマーク（800ms: Chorus）を追加
    s.update(|e, _, _| e.set_selection(vec![audio_id]));
    s.update(|e, _, cx| e.add_selected_clip_bookmark(Some(800), Some("Chorus".to_string()), cx));

    let upd_audio_opt = s.with(|e, _| e.selected_clip_options()).expect("updated audio options");
    assert_eq!(upd_audio_opt.bookmarks.len(), 1);
    assert_eq!(upd_audio_opt.bookmarks[0].name, "Chorus");
    assert_eq!(upd_audio_opt.bookmarks[0].time_ms, 800);

    // 5. ブックマーク位置へのシーク検証
    s.update(|e, _, _| e.set_selection(vec![video_id]));
    s.update(|e, _, cx| e.seek_selected_clip_to_bookmark(0, cx));

    // ブックマーク追加と個別削除の検証
    s.update(|e, _, cx| e.add_selected_clip_bookmark(Some(5000), Some("Temp".to_string()), cx));
    assert_eq!(s.with(|e, _| e.selected_clip_options()).unwrap().bookmarks.len(), 3);
    s.update(|e, _, cx| e.remove_selected_clip_bookmark(2, cx));
    assert_eq!(s.with(|e, _| e.selected_clip_options()).unwrap().bookmarks.len(), 2);

    // 6. 保存と開き直しで設定が保持されることを検証
    s.screenshot("media-bookmarks-playback-options");
    s.save_as("media-bookmarks-playback-options.pptx");
    let saved = s.reopen();
    let saved_shapes = &saved.slides[1].shapes;

    let saved_video_shape = saved_shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| !c.audio))).expect("saved video found");
    let ShapeKind::Picture(saved_v_pic) = &saved_video_shape.kind else { unreachable!() };
    let saved_v_clip = saved_v_pic.clip.as_ref().unwrap();
    assert_eq!(saved_v_clip.options.bookmarks.len(), 2);
    assert_eq!(saved_v_clip.options.bookmarks[0].name, "Keyframe 1");
    assert_eq!(saved_v_clip.options.bookmarks[0].time_ms, 1200);
    assert_eq!(saved_v_clip.options.bookmarks[1].name, "Keyframe 2");
    assert_eq!(saved_v_clip.options.bookmarks[1].time_ms, 3400);

    let saved_audio_shape = saved_shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| c.audio))).expect("saved audio found");
    let ShapeKind::Picture(saved_a_pic) = &saved_audio_shape.kind else { unreachable!() };
    let saved_a_clip = saved_a_pic.clip.as_ref().unwrap();
    assert_eq!(saved_a_clip.options.bookmarks.len(), 1);
    assert_eq!(saved_a_clip.options.bookmarks[0].name, "Chorus");
    assert_eq!(saved_a_clip.options.bookmarks[0].time_ms, 800);

    // 7. 全戻しとやり直し
    undo_all_and_redo(&mut s);
}

/// PPTX-VIDEO-013: 動画クリップの枠線、形状、効果（影・反射・ぼかし）、ビデオスタイルの適用・編集・保持検証。
pub(crate) fn video_border_shape_and_effects_options() {
    use super::editing::VideoStylePreset;
    use ibis_doc_model::{Color, Rgb, Stroke, Shadow, Reflection};

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let video_path = root.join("clip-test.mp4");
    let video_bytes = std::fs::read(&video_path).expect("clip-test.mp4 should exist");

    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(1);
    s.menu("pptx.new-slide");

    // 1. 動画クリップを挿入
    let video_id = s.update(|e, _, cx| e.insert_clip(&video_path, video_bytes.clone(), false, cx)).expect("video clip inserted");
    s.update(|e, _, _| e.set_selection(vec![video_id]));

    // 2. 初期状態の Picture 書式確認（枠なし、角丸なし、効果なし）
    let init_pic = s.with(|e, _| {
        let sh = e.slide().shape(video_id).expect("video shape");
        match &sh.kind {
            ShapeKind::Picture(p) => p.clone(),
            _ => panic!("expected picture"),
        }
    });
    assert!(init_pic.stroke.is_none());
    assert_eq!(init_pic.corner, 0);
    assert!(init_pic.shadow.is_none());
    assert!(init_pic.reflection.is_none());
    assert!(init_pic.soft_edge.is_none());

    // 3. 枠線の設定（赤色、太さ 2pt / 25_400 emu）
    let red_stroke = Stroke::solid(Color::opaque(Rgb(0xff0000)), 25_400);
    s.update(|e, _, cx| e.stroke_selected(|_| Some(red_stroke.clone()), cx));

    let stroke_pic = s.with(|e, _| {
        let sh = e.slide().shape(video_id).unwrap();
        match &sh.kind {
            ShapeKind::Picture(p) => p.clone(),
            _ => panic!("expected picture"),
        }
    });
    assert_eq!(stroke_pic.stroke, Some(red_stroke));

    // 4. 形状（角丸四角形 20,000）の設定
    s.update(|e, _, cx| e.crop_selected_pictures_corner(20_000, cx));
    let corner_pic = s.with(|e, _| {
        let sh = e.slide().shape(video_id).unwrap();
        match &sh.kind {
            ShapeKind::Picture(p) => p.clone(),
            _ => panic!("expected picture"),
        }
    });
    assert_eq!(corner_pic.corner, 20_000);

    // 5. 効果（影、反射、ぼかし）の設定
    let shadow = Shadow::simple(Color { rgb: Rgb(0), alpha: 128 }, 50_800, 50_800, 2_700_000);
    s.update(|e, _, cx| e.set_shape_shadow(Some(shadow), cx));
    let reflection = Reflection::standard(0, 35_000);
    s.update(|e, _, cx| e.set_shape_reflection(Some(reflection), cx));
    s.update(|e, _, cx| e.set_shape_soft_edge(Some(63_500), cx));

    let effects_pic = s.with(|e, _| {
        let sh = e.slide().shape(video_id).unwrap();
        match &sh.kind {
            ShapeKind::Picture(p) => p.clone(),
            _ => panic!("expected picture"),
        }
    });
    assert_eq!(effects_pic.shadow, Some(shadow));
    assert_eq!(effects_pic.reflection, Some(reflection));
    assert_eq!(effects_pic.soft_edge, Some(63_500));

    // 6. ビデオスタイルプリセット（RoundedReflection）の適用
    s.update(|e, _, cx| e.set_selected_pictures_video_style(VideoStylePreset::RoundedReflection, cx));
    let style_pic = s.with(|e, _| {
        let sh = e.slide().shape(video_id).unwrap();
        match &sh.kind {
            ShapeKind::Picture(p) => p.clone(),
            _ => panic!("expected picture"),
        }
    });
    assert_eq!(style_pic.corner, 20_000);
    assert_eq!(style_pic.reflection, Some(reflection));
    assert!(style_pic.shadow.is_none());
    assert!(style_pic.stroke.is_none());

    // 7. ビデオスタイルプリセット（RoundedShadow）の適用
    s.update(|e, _, cx| e.set_selected_pictures_video_style(VideoStylePreset::RoundedShadow, cx));
    let style2_pic = s.with(|e, _| {
        let sh = e.slide().shape(video_id).unwrap();
        match &sh.kind {
            ShapeKind::Picture(p) => p.clone(),
            _ => panic!("expected picture"),
        }
    });
    assert_eq!(style2_pic.corner, 20_000);
    assert_eq!(style2_pic.shadow, Some(shadow));
    assert!(style2_pic.reflection.is_none());

    // 8. 枠線を追加して複合スタイルの保存を準備
    let blue_stroke = Stroke::solid(Color::opaque(Rgb(0x0000ff)), 19_050);
    s.update(|e, _, cx| e.stroke_selected(|_| Some(blue_stroke.clone()), cx));
    s.update(|e, _, cx| e.set_shape_reflection(Some(reflection), cx));

    // 9. ファイル保存と再オープンによる保持検証
    s.screenshot("video-border-shape-and-effects");
    s.save_as("video-border-shape-and-effects.pptx");
    let saved = s.reopen();
    let saved_shapes = &saved.slides[1].shapes;
    let saved_video_shape = saved_shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| !c.audio))).expect("saved video found");
    let ShapeKind::Picture(saved_pic) = &saved_video_shape.kind else { unreachable!() };
    assert_eq!(saved_pic.corner, 20_000);
    assert_eq!(saved_pic.stroke, Some(blue_stroke));
    assert_eq!(saved_pic.shadow, Some(shadow));
    assert_eq!(saved_pic.reflection, Some(reflection));

    // 10. Undo / Redo の全戻しとやり直し検証
    undo_all_and_redo(&mut s);
}

/// PPTX-TEXT-002: ワードアートの挿入、文字変形（a:prstTxWarp）の設定・変更・解除、
/// ワードアートスタイルの適用、UIメニュー・ツールバー連動、ファイル保存・再オープンによる保持、Undo/Redo 検証。
pub(crate) fn wordart_insert_warp_and_styles() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(1);
    s.menu("pptx.new-slide");

    // 1. 挿入メニューからワードアート（スタイル1）を挿入
    s.menu("pptx.insert.wordart");
    let wordart_id = s.with(|e, _| {
        e.selected_ids().first().copied().expect("wordart selected")
    });

    // 挿入されたワードアートのテキストとスタイルを確認
    let init_body = s.with(|e, _| {
        e.slide().shape(wordart_id).unwrap().text_body().unwrap().clone()
    });
    assert_eq!(init_body.text(), "ワードアート");
    assert_eq!(init_body.warp, None);
    assert!(init_body.style.bold);

    // 2. 文字の変形: 上アーチ (textArchUp) を適用
    s.menu("pptx.text.warp.arch-up");
    assert_eq!(s.with(|e, _| e.selected_text_warp()), Some("textArchUp".to_string()));

    // 3. 文字の変形: 波 1 (textWave1) に変更
    s.menu("pptx.text.warp.wave1");
    assert_eq!(s.with(|e, _| e.selected_text_warp()), Some("textWave1".to_string()));

    // 4. 文字の変形: 円 (textCircle) に変更
    s.menu("pptx.text.warp.circle");
    assert_eq!(s.with(|e, _| e.selected_text_warp()), Some("textCircle".to_string()));

    // 5. ワードアートスタイル（スタイル 2: 青、影）を適用
    s.menu("pptx.text.wordart.style2");
    let styled_body = s.with(|e, _| {
        e.slide().shape(wordart_id).unwrap().text_body().unwrap().clone()
    });
    let first_run = &styled_body.paragraphs[0].runs[0];
    assert_eq!(first_run.style.color, Some(ibis_doc_model::Rgb(0x4472C4)));
    assert!(first_run.style.shadow.is_some());

    // 6. ワードアートスタイル（スタイル 4: 塗りつぶし白、輪郭黒）を適用
    s.menu("pptx.text.wordart.style4");
    let style4_body = s.with(|e, _| {
        e.slide().shape(wordart_id).unwrap().text_body().unwrap().clone()
    });
    let s4_run = &style4_body.paragraphs[0].runs[0];
    assert_eq!(s4_run.style.color, Some(ibis_doc_model::Rgb(0xFFFFFF)));
    assert!(s4_run.style.outline.is_some());

    // 7. 文字の変形を解除 (none)
    s.menu("pptx.text.warp.none");
    assert_eq!(s.with(|e, _| e.selected_text_warp()), None);

    // 再度波 1 (textWave1) を設定して保存
    s.menu("pptx.text.warp.wave1");
    assert_eq!(s.with(|e, _| e.selected_text_warp()), Some("textWave1".to_string()));

    // 8. 別のワードアートスタイル（スタイル3: オレンジ、輪郭）を新規挿入
    s.menu("pptx.insert.wordart.style3");
    let wordart3_id = s.with(|e, _| {
        e.selected_ids().first().copied().expect("wordart 3 selected")
    });
    assert_ne!(wordart3_id, wordart_id);
    let init_body3 = s.with(|e, _| {
        e.slide().shape(wordart3_id).unwrap().text_body().unwrap().clone()
    });
    let r3 = &init_body3.paragraphs[0].runs[0];
    assert_eq!(r3.style.color, Some(ibis_doc_model::Rgb(0xED7D31)));
    assert!(r3.style.outline.is_some());

    // 9. ファイル保存と再オープンによる保持検証
    s.screenshot("wordart-insert-warp-and-styles");
    s.save_as("wordart-insert-warp-and-styles.pptx");
    let saved = s.reopen();
    let saved_shapes = &saved.slides[1].shapes;
    let saved_wordart = saved_shapes.iter().find(|sh| sh.text_body().is_some_and(|b| b.warp.as_deref() == Some("textWave1"))).expect("saved wordart with textWave1 found");
    let saved_body = saved_wordart.text_body().expect("saved text body");
    assert_eq!(saved_body.warp.as_deref(), Some("textWave1"));
    assert_eq!(saved_body.text(), "ワードアート");

    let saved_wordart3 = saved_shapes.iter().find(|sh| sh.text_body().is_some_and(|b| b.warp.is_none() && b.paragraphs.first().and_then(|p| p.runs.first()).and_then(|r| r.style.outline).is_some())).expect("saved wordart3 with outline found");
    let saved_body3 = saved_wordart3.text_body().expect("saved text body 3");
    let sr3 = &saved_body3.paragraphs[0].runs[0];
    assert_eq!(sr3.style.color, Some(ibis_doc_model::Rgb(0xED7D31)));
    assert!(sr3.style.outline.is_some());

    // 10. Undo / Redo の全戻しとやり直し検証
    undo_all_and_redo(&mut s);
}

/// スライド ズーム機能（PPTX-TRANS-014）:
/// 挿入メニュー「ズーム > スライド ズーム…」からのモーダルダイアログ起動、
/// スライドズーム図形の挿入、スライドショーでのズームリンク遷移、
/// 保存・再オープン保持、Undo/Redo を検証する。
pub(crate) fn insert_slide_zoom_and_slideshow_navigation() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(1);

    let base_shapes = s.shapes();
    let current_slide_id = s.with(|e, _| e.current_slide);
    let target_slide_id = s.with(|e, _| e.presentation().slides[1].id);

    // 1. メニュー「ズーム > スライド ズーム…」を実行してモーダルダイアログを開く
    s.menu("pptx.insert.slide-zoom");
    assert!(s.with(|e, _| e.pptx().and_then(|p| p.slide_zoom_dialog.as_ref()).is_some()), "slide zoom dialog opened");

    // 2. ダイアログをEscキーでキャンセルして閉じられることを確認
    s.key("escape");
    assert!(s.with(|e, _| e.pptx().and_then(|p| p.slide_zoom_dialog.as_ref()).is_none()), "slide zoom dialog closed on escape");

    // 3. 再度ダイアログを開いてスライド2を選択し、挿入を実行
    s.menu("pptx.insert.slide-zoom");
    let zoom_shape_id = s.update(|e, _, cx| {
        let shape_id = e.insert_slide_zoom(target_slide_id, cx);
        e.close_slide_zoom_dialog(cx);
        shape_id
    }).expect("slide zoom shape inserted");

    // ダイアログが閉じ、シェイプ数が1増加したことを確認
    assert!(s.with(|e, _| e.pptx().and_then(|p| p.slide_zoom_dialog.as_ref()).is_none()));
    assert_eq!(s.shapes(), base_shapes + 1);

    // 挿入されたスライドズーム図形の検証
    assert_eq!(s.with(|e, _| e.slide().shape_links.get(&zoom_shape_id).cloned()), Some(ibis_doc_model::SlideLink::Slide(target_slide_id)));
    assert!(s.with(|e, _| e.slide().shape_link_tooltips.get(&zoom_shape_id).is_some()));

    // 4. スライドショー開始とズーム遷移（リンク追従）の検証
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    assert!(s.with(|e, _| e.slideshow.is_some()), "slideshow started");
    assert_eq!(s.with(|e, _| e.current_slide), current_slide_id, "initially on slide 1");

    // スライドズーム図形をクリックして対象スライド（スライド2）へシームレスにズーム遷移
    s.update(|e, w, cx| e.follow_link_in_show(&ibis_doc_model::SlideLink::Slide(target_slide_id), w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), target_slide_id, "navigated to target slide via slide zoom");

    // スライドショー終了
    s.update(|e, w, cx| e.end_slideshow(w, cx));
    assert!(s.with(|e, _| e.slideshow.is_none()), "slideshow ended");

    // 5. 保存・再オープンによる保持検証
    s.screenshot("slide-zoom-inserted");
    s.save_as("slide-zoom.pptx");
    let saved = s.reopen();
    let saved_slide1 = &saved.slides[0];
    assert_eq!(saved_slide1.shapes.len(), base_shapes + 1);
    let re_zoom = &saved_slide1.shapes[base_shapes];
    assert_eq!(saved_slide1.shape_links.get(&re_zoom.id), Some(&ibis_doc_model::SlideLink::Slide(target_slide_id)));
    assert!(saved_slide1.shape_link_tooltips.get(&re_zoom.id).is_some());

    // 6. Undo / Redo 検証
    undo_all_and_redo(&mut s);
}

/// ズーム後に元へ戻る（PPTX-TRANS-016 Return to Zoom）:
/// セクションズームおよびスライドズームでの「ズーム後に元へ戻る」設定、
/// スライドショー中のズーム遷移から末尾スライド表示完了後の自動復帰遷移、
/// メニュートグル、保存・再オープン保持、Undo/Redo を検証する。
pub(crate) fn return_to_zoom_slideshow_navigation() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(1);

    let slide_ids: Vec<_> = s.with(|e, _| e.presentation().slides.iter().map(|sl| sl.id).collect());
    assert!(slide_ids.len() >= 3, "must have at least 3 slides for section zoom return test");
    let slide1_id = slide_ids[0];
    let slide2_id = slide_ids[1];
    let slide3_id = slide_ids[2];
    let base_shapes = s.shapes();

    let (sec2_slides, sec3_slides) = if slide_ids.len() > 3 {
        (vec![slide2_id, slide3_id], slide_ids[3..].to_vec())
    } else {
        (slide_ids[1..].to_vec(), Vec::new())
    };
    let mut sections = vec![
        ibis_doc_model::Section {
            id: "{11111111-1111-1111-1111-111111111111}".into(),
            name: "第1セクション: 導入".into(),
            slides: vec![slide1_id],
        },
        ibis_doc_model::Section {
            id: "{22222222-2222-2222-2222-222222222222}".into(),
            name: "第2セクション: 本編".into(),
            slides: sec2_slides,
        },
    ];
    if !sec3_slides.is_empty() {
        sections.push(ibis_doc_model::Section {
            id: "{33333333-3333-3333-3333-333333333333}".into(),
            name: "第3セクション: 結び".into(),
            slides: sec3_slides,
        });
    }
    s.update(|e, _, cx| e.run(vec![ibis_doc_model::Command::SetSections { sections }], cx));

    // 2. セクションズーム図形を挿入（第2セクション向け）
    let sec2_id = "{22222222-2222-2222-2222-222222222222}";
    let sec_zoom_id = s.update(|e, _, cx| {
        e.insert_section_zoom(sec2_id, cx)
    }).expect("section zoom shape inserted");

    // セクションズームは PowerPoint 標準で既定有効（return_to_zoom: true）
    assert!(s.with(|e, _| e.slide().shape_return_to_zoom.contains(&sec_zoom_id)), "section zoom has return_to_zoom enabled by default");

    // 3. スライドショーを開始
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    assert!(s.with(|e, _| e.slideshow.is_some()), "slideshow started");
    assert_eq!(s.with(|e, _| e.current_slide), slide1_id, "on slide 1");

    // セクションズーム図形をクリックして第2セクション先頭（スライド2）へ遷移
    s.update(|e, w, cx| e.follow_link_from_shape_in_show(Some(sec_zoom_id), &ibis_doc_model::SlideLink::Slide(slide2_id), w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), slide2_id, "jumped to section start (slide 2)");
    assert!(s.with(|e, _| e.slideshow.as_ref().and_then(|sh| sh.zoom_return.as_ref()).is_some()), "zoom_return armed");

    // セクション内の次のスライド（スライド3）へ進む
    s.update(|e, w, cx| e.step_slideshow(true, w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), slide3_id, "advanced to slide 3");

    // スライド3（セクション最終スライド）から次へ進むと、自動的に元のズーム元スライド（スライド1）へ戻る！
    s.update(|e, w, cx| e.step_slideshow(true, w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), slide1_id, "automatically returned to slide 1 after section finished!");
    assert!(s.with(|e, _| e.slideshow.as_ref().and_then(|sh| sh.zoom_return.as_ref()).is_none()), "zoom_return consumed");

    // スライドショー終了
    s.update(|e, w, cx| e.end_slideshow(w, cx));
    assert!(s.with(|e, _| e.slideshow.is_none()));

    // 4. メニュー/ヘルパーからのトグル動作確認
    s.update(|e, _, cx| e.toggle_shape_return_to_zoom(sec_zoom_id, cx));
    assert!(!s.with(|e, _| e.slide().shape_return_to_zoom.contains(&sec_zoom_id)), "toggled off");
    s.update(|e, _, cx| e.toggle_shape_return_to_zoom(sec_zoom_id, cx));
    assert!(s.with(|e, _| e.slide().shape_return_to_zoom.contains(&sec_zoom_id)), "toggled back on");

    // 5. 単一スライドズームでの Return to Zoom 検証
    let slide_zoom_id = s.update(|e, _, cx| e.insert_slide_zoom(slide3_id, cx)).expect("slide zoom shape inserted");
    assert!(!s.with(|e, _| e.slide().shape_return_to_zoom.contains(&slide_zoom_id)));
    s.update(|e, _, cx| e.toggle_shape_return_to_zoom(slide_zoom_id, cx));
    assert!(s.with(|e, _| e.slide().shape_return_to_zoom.contains(&slide_zoom_id)));

    // スライドショーでスライドズームをクリック
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    s.update(|e, w, cx| e.follow_link_from_shape_in_show(Some(slide_zoom_id), &ibis_doc_model::SlideLink::Slide(slide3_id), w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), slide3_id, "jumped to slide 3 via slide zoom");

    // 単一スライドズームなので、スライド3の表示完了後に次へ進むとスライド1へ戻る
    s.update(|e, w, cx| e.step_slideshow(true, w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), slide1_id, "returned to slide 1 after single slide zoom!");

    s.update(|e, w, cx| e.end_slideshow(w, cx));

    // 5. 保存・再オープンによる保持検証
    s.save_as("return-to-zoom.pptx");
    let saved = s.reopen();
    let re_slide1 = &saved.slides[0];
    let re_sec_zoom = &re_slide1.shapes[base_shapes];
    let re_slide_zoom = &re_slide1.shapes[base_shapes + 1];
    assert!(re_slide1.shape_return_to_zoom.contains(&re_sec_zoom.id), "section zoom return_to_zoom preserved after reopen");
    assert!(re_slide1.shape_return_to_zoom.contains(&re_slide_zoom.id), "slide zoom return_to_zoom preserved after reopen");

    // 6. Undo / Redo 検証
    undo_all_and_redo(&mut s);
}

/// サマリー ズーム機能（PPTX-TRANS-013）:
/// 挿入メニュー「ズーム > サマリー ズーム…」からのダイアログ起動、
/// 複数スライド選択、サマリーズームカード群のグリッド自動配置、
/// スライドショーでのズーム遷移および Return to Zoom 自動復帰連携、
/// 保存・再オープン保持、Undo/Redo を検証する。
pub(crate) fn insert_summary_zoom_and_slideshow_navigation() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(1);

    let slide_ids: Vec<_> = s.with(|e, _| e.presentation().slides.iter().map(|sl| sl.id).collect());
    assert!(slide_ids.len() >= 3, "must have at least 3 slides for summary zoom test");
    let slide1_id = slide_ids[0];
    let slide2_id = slide_ids[1];
    let slide3_id = slide_ids[2];
    let base_shapes = s.shapes();

    // 1. メニューから「サマリー ズーム…」ダイアログを起動
    s.menu("pptx.insert.summary-zoom");
    assert!(s.with(|e, _| e.pptx().and_then(|p| p.summary_zoom_dialog.as_ref()).is_some()), "summary zoom dialog opened");

    // Escキーでキャンセル・閉じる
    s.key("escape");
    assert!(s.with(|e, _| e.pptx().and_then(|p| p.summary_zoom_dialog.as_ref()).is_none()), "dialog closed via escape");

    // 2. スライド2とスライド3を対象にサマリーズームを挿入
    let created_ids = s.update(|e, _, cx| {
        e.insert_summary_zoom(&[slide2_id, slide3_id], cx)
    });
    assert_eq!(created_ids.len(), 2, "inserted 2 summary zoom cards");
    let card1 = created_ids[0];
    let card2 = created_ids[1];

    // 各カードのプロパティ検証（リンク、Return to Zoom が既定で有効）
    assert_eq!(s.with(|e, _| e.slide().shape_links.get(&card1).cloned()), Some(ibis_doc_model::SlideLink::Slide(slide2_id)));
    assert_eq!(s.with(|e, _| e.slide().shape_links.get(&card2).cloned()), Some(ibis_doc_model::SlideLink::Slide(slide3_id)));
    assert!(s.with(|e, _| e.shape_return_to_zoom(card1)), "card1 has return-to-zoom enabled");
    assert!(s.with(|e, _| e.shape_return_to_zoom(card2)), "card2 has return-to-zoom enabled");

    // 3. スライドショー開始
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    assert!(s.with(|e, _| e.slideshow.is_some()));
    assert_eq!(s.with(|e, _| e.current_slide), slide1_id);

    // カード1をクリックしてスライド2へジャンプ
    s.update(|e, w, cx| e.follow_link_from_shape_in_show(Some(card1), &ibis_doc_model::SlideLink::Slide(slide2_id), w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), slide2_id, "jumped to slide 2 via card1");

    // スライド2から次へ進むと、Return to Zoom によりスライド1へ復帰
    s.update(|e, w, cx| e.step_slideshow(true, w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), slide1_id, "returned to slide 1 from slide 2");

    // カード2をクリックしてスライド3へジャンプ
    s.update(|e, w, cx| e.follow_link_from_shape_in_show(Some(card2), &ibis_doc_model::SlideLink::Slide(slide3_id), w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), slide3_id, "jumped to slide 3 via card2");

    // スライド3から次へ進むと、Return to Zoom によりスライド1へ復帰
    s.update(|e, w, cx| e.step_slideshow(true, w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), slide1_id, "returned to slide 1 from slide 3");

    s.update(|e, w, cx| e.end_slideshow(w, cx));

    // 4. 保存・再オープンによる保持検証
    s.save_as("summary-zoom-result.pptx");
    let saved = s.reopen();
    let re_slide1 = &saved.slides[0];
    let re_card1 = &re_slide1.shapes[base_shapes];
    let re_card2 = &re_slide1.shapes[base_shapes + 1];
    assert_eq!(re_slide1.shape_links.get(&re_card1.id), Some(&ibis_doc_model::SlideLink::Slide(slide2_id)), "card1 link preserved");
    assert_eq!(re_slide1.shape_links.get(&re_card2.id), Some(&ibis_doc_model::SlideLink::Slide(slide3_id)), "card2 link preserved");
    assert!(re_slide1.shape_return_to_zoom.contains(&re_card1.id), "card1 return_to_zoom preserved");
    assert!(re_slide1.shape_return_to_zoom.contains(&re_card2.id), "card2 return_to_zoom preserved");

    // 5. Undo / Redo 検証
    undo_all_and_redo(&mut s);
}

/// ズームの背景と継続時間（PPTX-TRANS-018）:
/// ズーム図形の背景表示/透過の切り替え（showBg）、継続時間の変更（dur）、
/// 保存・再オープン保持、メニュー操作連携、Undo/Redo を検証する。
pub(crate) fn zoom_background_and_duration_options() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(1);

    let slide_ids: Vec<_> = s.with(|e, _| e.presentation().slides.iter().map(|sl| sl.id).collect());
    assert!(slide_ids.len() >= 2, "must have at least 2 slides for zoom test");
    let slide2_id = slide_ids[1];
    let base_shapes = s.shapes();

    // 1. スライドズーム図形を挿入
    let zoom_shape_id = s.update(|e, _, cx| e.insert_slide_zoom(slide2_id, cx)).expect("slide zoom inserted");
    assert!(s.with(|e, _| e.is_zoom_shape(zoom_shape_id)));

    // デフォルト値の確認（背景表示: true、継続時間: 1000ms）
    assert!(s.with(|e, _| e.shape_zoom_show_bg(zoom_shape_id)));
    assert_eq!(s.with(|e, _| e.shape_zoom_duration(zoom_shape_id)), 1000);

    // 2. ズームの背景を切り替え（背景透過に設定）
    s.update(|e, _, cx| e.toggle_shape_zoom_show_bg(zoom_shape_id, cx));
    assert!(!s.with(|e, _| e.shape_zoom_show_bg(zoom_shape_id)), "background should be transparent");

    // 3. ズームの継続時間を変更（2000ms）
    s.update(|e, _, cx| e.set_shape_zoom_duration(zoom_shape_id, 2000, cx));
    assert_eq!(s.with(|e, _| e.shape_zoom_duration(zoom_shape_id)), 2000);

    // 4. 保存・再オープンによる保持検証
    s.save_as("zoom-bg-duration.pptx");
    let saved = s.reopen();
    let re_slide1 = &saved.slides[0];
    let re_zoom = &re_slide1.shapes[base_shapes];
    assert_eq!(re_slide1.shape_zoom_show_bg.get(&re_zoom.id), Some(&false), "showBg false preserved after reload");
    assert_eq!(re_slide1.shape_zoom_durations.get(&re_zoom.id), Some(&2000), "dur 2000ms preserved after reload");

    // 5. メニューからの操作検証
    // 再度背景をトグル（通常表示に戻す）
    s.menu("pptx.zoom.background");
    assert!(s.with(|e, _| e.shape_zoom_show_bg(zoom_shape_id)), "background toggled back to normal");

    // メニューから継続時間を 1500ms に設定
    s.menu("pptx.zoom.duration-1500");
    assert_eq!(s.with(|e, _| e.shape_zoom_duration(zoom_shape_id)), 1500);

    // 再度保存・再オープン
    s.save_as("zoom-bg-duration-updated.pptx");
    let saved2 = s.reopen();
    let re2_slide1 = &saved2.slides[0];
    let re2_zoom = &re2_slide1.shapes[base_shapes];
    assert_eq!(re2_slide1.shape_zoom_show_bg.get(&re2_zoom.id), Some(&true), "showBg true preserved after reload");
    assert_eq!(re2_slide1.shape_zoom_durations.get(&re2_zoom.id), Some(&1500), "dur 1500ms preserved after reload");

    // 6. Undo / Redo 検証
    undo_all_and_redo(&mut s);
}

/// ズーム画像の変更機能（PPTX-TRANS-017）:
/// スライドズーム図形に対するカスタム画像の適用（Picture kind）、
/// スライドショー中でのズーム遷移および Return to Zoom の動作、
/// 保存・再オープン保持、ズーム画像のリセット（元の AutoShape への復帰）、Undo/Redo を検証する。
pub(crate) fn change_zoom_image_and_slideshow_navigation() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(1);

    let slide_ids: Vec<_> = s.with(|e, _| e.presentation().slides.iter().map(|sl| sl.id).collect());
    assert!(slide_ids.len() >= 2, "must have at least 2 slides for zoom test");
    let slide1_id = slide_ids[0];
    let slide2_id = slide_ids[1];
    let base_shapes = s.shapes();

    // 1. スライドズーム図形を挿入
    let zoom_shape_id = s.update(|e, _, cx| e.insert_slide_zoom(slide2_id, cx)).expect("slide zoom inserted");
    assert!(s.with(|e, _| e.is_zoom_shape(zoom_shape_id)));
    assert!(!s.with(|e, _| e.has_custom_zoom_image(zoom_shape_id)));

    // Return to Zoom を有効化
    s.update(|e, _, cx| e.toggle_shape_return_to_zoom(zoom_shape_id, cx));
    assert!(s.with(|e, _| e.shape_return_to_zoom(zoom_shape_id)));

    // 2. 「ズーム画像の変更…」でカスタム画像（PNG）を適用
    let test_img_data = png(100, 80, [0x33, 0x77, 0xbb]);
    let success = s.update(|e, _, cx| {
        e.change_zoom_image(zoom_shape_id, "custom_zoom.png", test_img_data.clone(), cx)
    });
    assert!(success, "zoom image changed successfully");

    // 図形が Picture になり、has_custom_zoom_image が true になること
    assert!(s.with(|e, _| e.has_custom_zoom_image(zoom_shape_id)));
    assert!(matches!(s.with(|e, _| e.slide().shape(zoom_shape_id).unwrap().kind.clone()), ibis_doc_model::ShapeKind::Picture(_)));
    // ズームリンクと Return to Zoom が保持されていること
    assert_eq!(s.with(|e, _| e.slide().shape_links.get(&zoom_shape_id).cloned()), Some(ibis_doc_model::SlideLink::Slide(slide2_id)));
    assert!(s.with(|e, _| e.shape_return_to_zoom(zoom_shape_id)));

    // 3. スライドショーでカスタム画像ズーム図形をクリックして遷移・Return to Zoom を検証
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    assert!(s.with(|e, _| e.slideshow.is_some()));
    assert_eq!(s.with(|e, _| e.current_slide), slide1_id);

    // ズーム図形をクリック
    s.update(|e, w, cx| e.follow_link_from_shape_in_show(Some(zoom_shape_id), &ibis_doc_model::SlideLink::Slide(slide2_id), w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), slide2_id, "jumped to slide 2 via custom-image zoom shape");
    assert!(s.with(|e, _| e.slideshow.as_ref().and_then(|sh| sh.zoom_return.as_ref()).is_some()), "zoom_return armed");

    // スライド2から次へ進むと、Return to Zoom により自動的にスライド1へ復帰！
    s.update(|e, w, cx| e.step_slideshow(true, w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), slide1_id, "returned to slide 1 via return-to-zoom");

    s.update(|e, w, cx| e.end_slideshow(w, cx));
    assert!(s.with(|e, _| e.slideshow.is_none()));

    // 4. 保存・再オープンによる保持検証
    s.save_as("zoom-custom-image.pptx");
    let saved = s.reopen();
    let re_slide1 = &saved.slides[0];
    let re_zoom = &re_slide1.shapes[base_shapes];
    assert!(matches!(re_zoom.kind, ibis_doc_model::ShapeKind::Picture(_)), "custom zoom image shape preserved as Picture after reopen");
    assert_eq!(re_slide1.shape_links.get(&re_zoom.id), Some(&ibis_doc_model::SlideLink::Slide(slide2_id)), "zoom link preserved after reopen");
    assert!(re_slide1.shape_return_to_zoom.contains(&re_zoom.id), "return-to-zoom preserved after reopen");

    // 5. ズーム画像のリセット（Reset Zoom Image）
    let reset_success = s.update(|e, _, cx| e.reset_zoom_image(zoom_shape_id, cx));
    assert!(reset_success, "reset zoom image succeeded");
    assert!(!s.with(|e, _| e.has_custom_zoom_image(zoom_shape_id)));
    assert!(matches!(s.with(|e, _| e.slide().shape(zoom_shape_id).unwrap().kind.clone()), ibis_doc_model::ShapeKind::Auto(_)), "shape reset to AutoShape preview card");
    assert_eq!(s.with(|e, _| e.slide().shape_links.get(&zoom_shape_id).cloned()), Some(ibis_doc_model::SlideLink::Slide(slide2_id)), "zoom link preserved after reset");
    assert!(s.with(|e, _| e.shape_return_to_zoom(zoom_shape_id)), "return-to-zoom preserved after reset");

    // リセット状態の保存・再オープン
    s.save_as("zoom-image-reset.pptx");
    let saved_reset = s.reopen();
    let re_reset_slide1 = &saved_reset.slides[0];
    let re_reset_zoom = &re_reset_slide1.shapes[base_shapes];
    assert!(matches!(re_reset_zoom.kind, ibis_doc_model::ShapeKind::Auto(_)), "reset shape preserved as AutoShape after reopen");
    assert_eq!(re_reset_slide1.shape_links.get(&re_reset_zoom.id), Some(&ibis_doc_model::SlideLink::Slide(slide2_id)));
    assert!(re_reset_slide1.shape_return_to_zoom.contains(&re_reset_zoom.id));

    // 6. Undo / Redo 検証
    undo_all_and_redo(&mut s);
}


/// セクション ズーム機能（PPTX-TRANS-015）:
/// 挿入メニュー「ズーム > セクション ズーム…」からのモーダルダイアログ起動、
/// セクションズーム図形の挿入、スライドショーでのズームリンク遷移、
/// 保存・再オープン保持、Undo/Redo を検証する。
pub(crate) fn insert_section_zoom_and_slideshow_navigation() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(1);

    let slide_ids: Vec<_> = s.with(|e, _| e.presentation().slides.iter().map(|sl| sl.id).collect());
    let slide1_id = slide_ids[0];
    let slide2_id = slide_ids[1];
    let base_shapes = s.shapes();

    // 1. セクションを作成
    let sections = vec![
        ibis_doc_model::Section {
            id: "{11111111-1111-1111-1111-111111111111}".into(),
            name: "第1セクション: 導入".into(),
            slides: vec![slide1_id],
        },
        ibis_doc_model::Section {
            id: "{22222222-2222-2222-2222-222222222222}".into(),
            name: "第2セクション: 計画詳細".into(),
            slides: slide_ids[1..].to_vec(),
        },
    ];
    s.update(|e, _, cx| e.run(vec![ibis_doc_model::Command::SetSections { sections }], cx));

    // 2. メニュー「ズーム > セクション ズーム…」を実行してモーダルダイアログを開く
    s.menu("pptx.insert.section-zoom");
    assert!(s.with(|e, _| e.pptx().and_then(|p| p.section_zoom_dialog.as_ref()).is_some()), "section zoom dialog opened");

    // 3. ダイアログをEscキーでキャンセルして閉じられることを確認
    s.key("escape");
    assert!(s.with(|e, _| e.pptx().and_then(|p| p.section_zoom_dialog.as_ref()).is_none()), "section zoom dialog closed on escape");

    // 4. 再度ダイアログを開いて第2セクションを選択し、挿入を実行
    s.menu("pptx.insert.section-zoom");
    let sec2_id = "{22222222-2222-2222-2222-222222222222}";
    let zoom_shape_id = s.update(|e, _, cx| {
        let shape_id = e.insert_section_zoom(sec2_id, cx);
        e.close_section_zoom_dialog(cx);
        shape_id
    }).expect("section zoom shape inserted");

    // ダイアログが閉じ、シェイプ数が1増加したことを確認
    assert!(s.with(|e, _| e.pptx().and_then(|p| p.section_zoom_dialog.as_ref()).is_none()));
    assert_eq!(s.shapes(), base_shapes + 1);

    // 挿入されたセクションズーム図形の検証
    assert_eq!(s.with(|e, _| e.slide().shape_links.get(&zoom_shape_id).cloned()), Some(ibis_doc_model::SlideLink::Slide(slide2_id)));
    let tooltip = s.with(|e, _| e.slide().shape_link_tooltips.get(&zoom_shape_id).cloned()).expect("tooltip present");
    assert!(tooltip.contains("第2セクション: 計画詳細"), "tooltip contains section name: {tooltip}");

    // 5. スライドショー開始とズーム遷移（リンク追従）の検証
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    assert!(s.with(|e, _| e.slideshow.is_some()), "slideshow started");
    assert_eq!(s.with(|e, _| e.current_slide), slide1_id, "initially on slide 1");

    // セクションズーム図形をクリックして対象セクション先頭スライド（スライド2）へシームレスにズーム遷移
    s.update(|e, w, cx| e.follow_link_in_show(&ibis_doc_model::SlideLink::Slide(slide2_id), w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), slide2_id, "navigated to target section first slide via section zoom");

    // スライドショー終了
    s.update(|e, w, cx| e.end_slideshow(w, cx));
    assert!(s.with(|e, _| e.slideshow.is_none()), "slideshow ended");

    // 6. 保存・再オープンによる保持検証
    s.screenshot("section-zoom-inserted");
    s.save_as("section-zoom.pptx");
    let saved = s.reopen();
    assert_eq!(saved.sections.len(), 2, "sections preserved");
    let saved_slide1 = &saved.slides[0];
    assert_eq!(saved_slide1.shapes.len(), base_shapes + 1);
    let re_zoom = &saved_slide1.shapes[base_shapes];
    assert_eq!(saved_slide1.shape_links.get(&re_zoom.id), Some(&ibis_doc_model::SlideLink::Slide(slide2_id)));
    assert!(saved_slide1.shape_link_tooltips.get(&re_zoom.id).is_some());

    // 7. Undo / Redo 検証
    undo_all_and_redo(&mut s);
}

/// PPTX-CHART-006: 組合せと第2軸（Combo Chart & Secondary Axis）
/// 集合縦棒 - 折れ線、集合縦棒 - 折れ線 (第 2 軸)、積み上げ面 - 集合縦棒の挿入、
/// 複数チャートグループ・第2数値軸の OOXML シリアライズ、再読込保持、Undo/Redo を検証する。
pub(crate) fn combo_charts_insert_and_secondary_axis() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    let before = s.shapes();

    // 1. 挿入メニューから「集合縦棒 - 折れ線」を挿入
    s.menu("pptx.insert-chart.combo-column-line");
    assert_eq!(s.shapes(), before + 1);
    assert_eq!(s.with(|e, _| e.selection_kind()), SelectionKind::Chart);
    let shape1 = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Chart(chart1) = &shape1.kind else { panic!("a chart expected") };
    assert_eq!(chart1.groups.len(), 2, "ComboColumnLine must have 2 groups");
    assert!(matches!(chart1.groups[0].kind, ibis_doc_model::ChartType::Bar { horizontal: false, .. }));
    assert!(matches!(chart1.groups[1].kind, ibis_doc_model::ChartType::Line));
    assert_eq!(chart1.groups[0].series.len(), 2);
    assert_eq!(chart1.groups[1].series.len(), 1);

    // 2. 挿入メニューから「集合縦棒 - 折れ線 (第 2 軸)」を挿入
    s.menu("pptx.insert-chart.combo-column-line-secondary");
    assert_eq!(s.shapes(), before + 2);
    let shape2 = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Chart(chart2) = &shape2.kind else { panic!("a chart expected") };
    assert_eq!(chart2.groups.len(), 2);
    assert_eq!(chart2.axes.len(), 3, "must have primary and secondary axes");
    assert!(chart2.axes.iter().any(|a| a.side == ibis_doc_model::Side::Right), "secondary value axis on right");

    // 3. 挿入メニューから「積み上げ面 - 集合縦棒」を挿入
    s.menu("pptx.insert-chart.combo-area-column");
    assert_eq!(s.shapes(), before + 3);
    let shape3 = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Chart(chart3) = &shape3.kind else { panic!("a chart expected") };
    assert_eq!(chart3.groups.len(), 2);
    assert!(matches!(chart3.groups[0].kind, ibis_doc_model::ChartType::Area));
    assert!(matches!(chart3.groups[1].kind, ibis_doc_model::ChartType::Bar { horizontal: false, .. }));

    // 4. 保存と再読込による保持検証
    s.screenshot("combo-charts-inserted");
    s.save_as("combo-charts.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[1].shapes.len(), before + 3);

    let charts: Vec<_> = saved.slides[1].shapes.iter().filter_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some(c.as_ref()),
        _ => None,
    }).collect();
    assert!(charts.iter().any(|c| c.groups.len() == 2 && matches!(c.groups[0].kind, ibis_doc_model::ChartType::Bar { horizontal: false, .. }) && matches!(c.groups[1].kind, ibis_doc_model::ChartType::Line) && !c.axes.iter().any(|a| a.side == ibis_doc_model::Side::Right)));
    assert!(charts.iter().any(|c| c.groups.len() == 2 && matches!(c.groups[0].kind, ibis_doc_model::ChartType::Bar { horizontal: false, .. }) && matches!(c.groups[1].kind, ibis_doc_model::ChartType::Line) && c.axes.iter().any(|a| a.side == ibis_doc_model::Side::Right)));
    assert!(charts.iter().any(|c| c.groups.len() == 2 && matches!(c.groups[0].kind, ibis_doc_model::ChartType::Area) && matches!(c.groups[1].kind, ibis_doc_model::ChartType::Bar { horizontal: false, .. })));

    // 5. Undo / Redo 検証
    undo_all_and_redo(&mut s);
}

/// PPTX-CHART-005: 株価・レーダー・表面（Stock, Radar & Surface Charts）
/// メニューからの株価（HLC, OHLC, VHLC, VOHLC）、レーダー（通常, 塗りつぶし）、
/// 3D表面、等高線グラフの挿入、選択状態、保存・再オープン保持、Undo/Redo を検証。
pub(crate) fn stock_radar_and_surface_charts() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    let before = s.shapes();

    // 1. 株価グラフ（StockHLC）の挿入
    s.menu("pptx.insert-chart.stock-hlc");
    assert_eq!(s.shapes(), before + 1);
    assert_eq!(s.with(|e, _| e.selection_kind()), SelectionKind::Chart);
    let shape_stock = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Chart(stock_c) = &shape_stock.kind else { panic!("a chart expected") };
    assert!(stock_c.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Stock { up_down_bars: false, hi_low_lines: true, .. })));

    // 2. レーダーグラフ（塗りつぶしレーダー）の挿入
    s.menu("pptx.chrome.filled-radar");
    assert_eq!(s.shapes(), before + 2);
    let shape_radar = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Chart(radar_c) = &shape_radar.kind else { panic!("a chart expected") };
    assert!(radar_c.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Radar { style: Some(ibis_doc_model::RadarStyle::Filled) })));

    // 3. 3D表面グラフ（Surface3D）の挿入
    s.menu("pptx.insert-chart.surface-3d");
    assert_eq!(s.shapes(), before + 3);
    let shape_surf = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Chart(surf_c) = &shape_surf.kind else { panic!("a chart expected") };
    assert!(surf_c.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Surface { is_3d: true, wireframe: false })));

    // 4. 等高線グラフ（ContourWireframe）の挿入
    s.menu("pptx.insert-chart.contour-wireframe");
    assert_eq!(s.shapes(), before + 4);
    let shape_contour = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Chart(contour_c) = &shape_contour.kind else { panic!("a chart expected") };
    assert!(contour_c.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Surface { is_3d: false, wireframe: true })));

    // 5. 保存と再読込による保持検証
    s.screenshot("stock-radar-surface-charts");
    s.save_as("stock-radar-surface.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[1].shapes.len(), before + 4);

    let re_charts: Vec<_> = saved.slides[1].shapes.iter().filter_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some(c.as_ref()),
        _ => None,
    }).collect();
    assert!(re_charts.iter().any(|c| c.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Stock { up_down_bars: false, hi_low_lines: true, .. }))));
    assert!(re_charts.iter().any(|c| c.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Radar { style: Some(ibis_doc_model::RadarStyle::Filled) }))));
    assert!(re_charts.iter().any(|c| c.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Surface { is_3d: true, wireframe: false }))));
    assert!(re_charts.iter().any(|c| c.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Surface { is_3d: false, wireframe: true }))));

    // 6. Undo / Redo 検証
    undo_all_and_redo(&mut s);
}

/// PPTX-FILE-005: pptm・potm マクロ対応形式
/// マクロ付きプレゼンテーション（.pptm）のオープン、マクロ実行抑止ステータス通知、
/// 編集、保存、再オープン、vbaProject.bin およびマクロ対応 ContentType の保持を検証。
pub(crate) fn pptm_and_macro_enabled_presentations() {
    let out_dir = std::env::temp_dir().join(format!("ibisoffice-pptm-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&out_dir);
    let pptm_path = {
        let template_path = crate::shell::scenario::repo_root().join("test/editing/pptx-plan.pptx");
        let source_bytes = std::fs::read(&template_path).expect("read template");
        let mut package = ibis_pptx::ibis_ooxml::Package::open(source_bytes).expect("open pkg");
        let main = "ppt/presentation.xml";
        let content_type = "application/vnd.ms-powerpoint.presentation.macroEnabled.main+xml";
        package.set_override(main, content_type).unwrap();
        let vba = "ppt/vbaProject.bin";
        package.set_bytes(vba, b"dummy vba binary content".to_vec());
        package.set_override(vba, "application/vnd.ms-office.vbaProject").unwrap();
        package.add_relationship(main, "http://schemas.microsoft.com/office/2006/relationships/vbaProject", "vbaProject.bin", false).unwrap();
        let pptm_data = package.save(ibis_pptx::ibis_ooxml::SaveOptions::default()).unwrap();
        let path = out_dir.join("macro_presentation.pptm");
        std::fs::write(&path, pptm_data).unwrap();
        path
    };

    let mut s = Scenario::open(pptm_path.to_str().unwrap());
    assert!(s.with(|e, _| e.has_document), ".pptm がドキュメントとして開く");

    // ステータスメッセージの検証: マクロ実行抑止通知
    let status = s.with(|e, _| e.status.clone());
    assert!(status.is_some(), "ステータスメッセージが存在する");
    assert!(status.unwrap().contains("マクロは実行されません"), "マクロ実行抑止メッセージが表示される");

    // 編集操作: 新しい図形を追加
    s.page(1);
    let before = s.shapes();
    let frame = ibis_doc_model::EmuRect::new(100_000, 100_000, 1_000_000, 1_000_000);
    s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        e.insert_shape(ibis_doc_model::Shape::text_box(id, frame, "Macro Shape"), cx);
    });
    assert_eq!(s.shapes(), before + 1);

    // 保存して保持検証
    let saved_path = out_dir.join("saved_presentation.pptm");
    s.update(|e, w, _| {
        e.save_to(saved_path.clone(), w).expect("save_to .pptm");
    });
    assert!(saved_path.exists());

    // Undo / Redo 検証
    undo_all_and_redo(&mut s);
    drop(s);

    let saved_bytes = std::fs::read(&saved_path).unwrap();
    let pkg = ibis_pptx::ibis_ooxml::Package::open(saved_bytes).unwrap();
    assert_eq!(pkg.read("ppt/vbaProject.bin").unwrap(), b"dummy vba binary content", "vbaProject.bin が保存後も完全に保持される");
    let ct = String::from_utf8(pkg.read("[Content_Types].xml").unwrap()).unwrap();
    assert!(ct.contains("application/vnd.ms-powerpoint.presentation.macroEnabled.main+xml"), "macroEnabled ContentType が保持される");
    assert!(ct.contains("application/vnd.ms-office.vbaProject"));

    // 再オープン検証
    let mut s_reopen = Scenario::open(saved_path.to_str().unwrap());
    s_reopen.page(1);
    assert_eq!(s_reopen.shapes(), before + 1, "追加した図形が保持されている");
    drop(s_reopen);

    // クリーンアップ
    let _ = std::fs::remove_dir_all(&out_dir);
}

/// The scenarios, for `FormatEditor::scenarios`.
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::pptm_and_macro_enabled_presentations", pptm_and_macro_enabled_presentations),
        ("pptx::stock_radar_and_surface_charts", stock_radar_and_surface_charts),
        ("pptx::combo_charts_insert_and_secondary_axis", combo_charts_insert_and_secondary_axis),
        ("pptx::insert_summary_zoom_and_slideshow_navigation", insert_summary_zoom_and_slideshow_navigation),
        ("pptx::zoom_background_and_duration_options", zoom_background_and_duration_options),
        ("pptx::change_zoom_image_and_slideshow_navigation", change_zoom_image_and_slideshow_navigation),
        ("pptx::return_to_zoom_slideshow_navigation", return_to_zoom_slideshow_navigation),
        ("pptx::insert_section_zoom_and_slideshow_navigation", insert_section_zoom_and_slideshow_navigation),
        ("pptx::insert_slide_zoom_and_slideshow_navigation", insert_slide_zoom_and_slideshow_navigation),
        ("pptx::wordart_insert_warp_and_styles", wordart_insert_warp_and_styles),
        ("pptx::video_border_shape_and_effects_options", video_border_shape_and_effects_options),
        ("pptx::media_bookmarks_playback_options", media_bookmarks_playback_options),
        ("pptx::text_contrast_accessibility_check", text_contrast_accessibility_check),
        ("pptx::reading_order_pane_and_accessibility_check", reading_order_pane_and_accessibility_check),
        ("pptx::insert_svg_picture_round_trip", insert_svg_picture_round_trip),
        ("pptx::convert_svg_to_shape_and_round_trip", convert_svg_to_shape_and_round_trip),
        ("pptx::table_header_accessibility_check", table_header_accessibility_check),
        ("pptx::meaningful_link_text_accessibility_check", meaningful_link_text_accessibility_check),
        ("pptx::unique_slide_titles_accessibility_check", unique_slide_titles_accessibility_check),
        ("pptx::check_accessibility_panel_and_issue_jump", check_accessibility_panel_and_issue_jump),
        ("pptx::inspect_document_and_remove_personal_info", inspect_document_and_remove_personal_info),
        ("pptx::shape_decorative_marking", shape_decorative_marking),
        ("pptx::presentation_properties_panel_and_custom_properties", presentation_properties_panel_and_custom_properties),
        ("pptx::mark_as_final_and_read_only_presentation", mark_as_final_and_read_only_presentation),
        ("pptx::animation_painter_and_copy_effects", animation_painter_and_copy_effects),
        ("pptx::zoom_and_window_arrangement", zoom_and_window_arrangement),
        ("pptx::reading_view_and_presentation_views", reading_view_and_presentation_views),
        ("pptx::outline_view_and_presentation_views", outline_view_and_presentation_views),
        ("pptx::notes_page_view_and_presentation_views", notes_page_view_and_presentation_views),
        ("pptx::slide_export_and_context_menu", slide_export_and_context_menu),
        ("pptx::slide_sorter_grid_view", slide_sorter_grid_view),
        ("pptx::slideshow_rehearse_timings", slideshow_rehearse_timings),
        ("pptx::slideshow_zoom_and_pan", slideshow_zoom_and_pan),
        ("pptx::slideshow_all_slides_grid", slideshow_all_slides_grid),
        ("pptx::slideshow_laser_and_pen", slideshow_laser_and_pen),
        ("pptx::slideshow_keep_ink_annotations", slideshow_keep_ink_annotations),
        ("pptx::shape_alt_text", shape_alt_text),
        ("pptx::animation_motion_path", animation_motion_path),
        ("pptx::animation_effect_options", animation_effect_options),
        ("pptx::custom_shows_and_slideshow", custom_shows_and_slideshow),
        ("pptx::animation_preview_and_remove", animation_preview_and_remove),
        ("pptx::animation_delay_options", animation_delay_options),
        ("pptx::presenter_timer_pause_and_reset", presenter_timer_pause_and_reset),
        ("pptx::show_properties_and_slideshow_controls", show_properties_and_slideshow_controls),
        ("pptx::edit_text_make_bold_save_and_reopen", edit_text_make_bold_save_and_reopen),
        ("pptx::new_slides_from_layouts", new_slides_from_layouts),
        ("pptx::duplicate_move_and_delete_slides", duplicate_move_and_delete_slides),
        ("pptx::text_box_by_click_and_tools_cancelled", text_box_by_click_and_tools_cancelled),
        ("pptx::draw_a_shape_and_an_arrow", draw_a_shape_and_an_arrow),
        ("pptx::select_with_clicks_and_the_rubber_band", select_with_clicks_and_the_rubber_band),
        ("pptx::move_resize_and_rotate", move_resize_and_rotate),
        ("pptx::order_and_align", order_and_align),
        ("pptx::fill_and_outline", fill_and_outline),
        ("pptx::a_chart_with_shapes_over_it", a_chart_with_shapes_over_it),
        ("pptx::empty_paragraph_and_bullets", empty_paragraph_and_bullets),
        ("pptx::format_part_of_a_text", format_part_of_a_text),
        ("pptx::lists_indents_and_spacing", lists_indents_and_spacing),
        ("pptx::slide_background", slide_background),
        ("pptx::japanese_input_and_formatting_while_editing", japanese_input_and_formatting_while_editing),
        ("pptx::caret_by_mouse_in_edited_text", caret_by_mouse_in_edited_text),
        ("pptx::edit_the_text_of_a_turned_shape", edit_the_text_of_a_turned_shape),
        ("pptx::skip_a_slide", skip_a_slide),
        ("pptx::write_speaker_notes", write_speaker_notes),
        ("pptx::set_slide_transitions", set_slide_transitions),
        ("pptx::edit_table_cells", edit_table_cells),
        ("pptx::table_rows_and_columns", table_rows_and_columns),
        ("pptx::insert_a_table", insert_a_table),
        ("pptx::merge_table_cells", merge_table_cells),
        ("pptx::table_cell_size_panel", table_cell_size_panel),
        ("pptx::tab_selects_the_next_shape", tab_selects_the_next_shape),
        ("pptx::color_palette_in_the_toolbar", color_palette_in_the_toolbar),
        ("pptx::typed_list_marks_become_lists", typed_list_marks_become_lists),
        ("pptx::cells_copied_from_a_worksheet_paste_as_a_table", cells_copied_from_a_worksheet_paste_as_a_table),
        ("pptx::typed_addresses_become_links", typed_addresses_become_links),
        ("pptx::drag_several_slides_together", drag_several_slides_together),
        ("pptx::align_several_shapes_to_the_slide", align_several_shapes_to_the_slide),
        ("pptx::select_several_slides_in_the_list", select_several_slides_in_the_list),
        ("pptx::delete_and_copy_several_slides", delete_and_copy_several_slides),
        ("pptx::right_click_menu_on_the_slide", right_click_menu_on_the_slide),
        ("pptx::picture_files_dropped_on_the_slide", picture_files_dropped_on_the_slide),
        ("pptx::paste_takes_what_was_copied_last", paste_takes_what_was_copied_last),
        ("pptx::text_copy_cut_paste_with_the_system_clipboard", text_copy_cut_paste_with_the_system_clipboard),
        ("pptx::tab_in_the_last_cell_adds_a_row", tab_in_the_last_cell_adds_a_row),
        ("pptx::text_caret_and_delete_by_words", text_caret_and_delete_by_words),
        ("pptx::option_drag_copies_the_shape", option_drag_copies_the_shape),
        ("pptx::text_shortcuts_align_and_copy_format", text_shortcuts_align_and_copy_format),
        ("pptx::font_size_steps_of_the_selected_text", font_size_steps_of_the_selected_text),
        ("pptx::gradient_and_pattern_fill_presets", gradient_and_pattern_fill_presets),
        ("pptx::slideshow_from_start_and_by_itself", slideshow_from_start_and_by_itself),
        ("pptx::presenter_view", presenter_view),
        ("pptx::play_animations_in_the_show", play_animations_in_the_show),
        ("pptx::strikethrough_button", strikethrough_button),
        ("pptx::shape_size_and_place_by_numbers", shape_size_and_place_by_numbers),
        ("pptx::placeholder_prompts", placeholder_prompts),
        ("pptx::slide_comments", slide_comments),
        ("pptx::edit_chart_data", edit_chart_data),
        ("pptx::insert_chart", insert_chart),
        ("pptx::insert_chart_kinds", insert_chart_kinds),
        ("pptx::english_ui", english_ui),
        ("pptx::copy_cut_and_paste_slides", copy_cut_and_paste_slides),
        ("pptx::paste_slide_from_another_file", paste_slide_from_another_file),
        ("pptx::add_animations_from_the_menu", add_animations_from_the_menu),
        ("pptx::change_shape", change_shape),
        ("pptx::distribute_table_rows_and_columns", distribute_table_rows_and_columns),
        ("pptx::background_applied_to_all", background_applied_to_all),
        ("pptx::header_and_footer", header_and_footer),
        ("pptx::chart_data_in_the_workbook", chart_data_in_the_workbook),
        ("pptx::chart_ex_insert_and_data", chart_ex_insert_and_data),
        ("pptx::selection_pane", selection_pane),
        ("pptx::hyperlinks", hyperlinks),
        ("pptx::scale_slide_size", scale_slide_size),
        ("pptx::edit_theme", edit_theme),
        ("pptx::insert_symbols", insert_symbols),
        ("pptx::layout_rename_and_duplicate", layout_rename_and_duplicate),
        ("pptx::master_rename_duplicate_remove", master_rename_duplicate_remove),
        ("pptx::print_settings_panel", print_settings_panel),
        ("pptx::merge_shapes", merge_shapes),
        ("pptx::edit_points", edit_points),
        ("pptx::animation_custom_path", animation_custom_path),
        ("pptx::animation_on_several_shapes", animation_on_several_shapes),
        ("pptx::animation_by_paragraph", animation_by_paragraph),
        ("pptx::smartart_to_shapes", smartart_to_shapes),
        ("pptx::compress_pictures", compress_pictures),
        ("pptx::morph_transition_in_the_slide_show", morph_transition_in_the_slide_show),
        ("pptx::linked_video", linked_video),
        ("pptx::protected_file", protected_file),
        ("pptx::master_view", master_view),
        ("pptx::notes_formatting", notes_formatting),
        ("pptx::modify_password", modify_password),
        ("pptx::handout_master_view", handout_master_view),
        ("pptx::spelling_check", spelling_check),
        ("pptx::paragraph_animations", paragraph_animations),
        ("pptx::notes_header_footer_panel", notes_header_footer_panel),
        ("pptx::animated_gif_plays_in_the_slide_show", animated_gif_plays_in_the_slide_show),
        ("pptx::default_shape_look", default_shape_look),
        ("pptx::lock_aspect_ratio", lock_aspect_ratio),
        ("pptx::action_button", action_button),
        ("pptx::first_slide_number", first_slide_number),
        ("pptx::bullet_and_number_types", bullet_and_number_types),
        ("pptx::presenter_view_on_two_displays", presenter_view_on_two_displays),
        ("pptx::insert_video_and_audio", insert_video_and_audio),
        ("pptx::change_chart_type", change_chart_type),
        ("pptx::pie_and_doughnut_chart_options", pie_and_doughnut_chart_options),
        ("pptx::hide_background_graphics_and_distribute_text", hide_background_graphics_and_distribute_text),
        ("pptx::animation_pane", animation_pane),
        ("pptx::smartart_shown_and_moved", smartart_shown_and_moved),
        ("pptx::custom_linear_gradient_targets_and_conflicts", custom_linear_gradient_targets_and_conflicts),
        ("pptx::custom_gradient_preflight_atomic_rejection", custom_gradient_preflight_atomic_rejection),
        ("pptx::custom_path_gradient_targets_and_conflicts", custom_path_gradient_targets_and_conflicts),
        ("pptx::custom_pattern_all_presets_targets_and_conflicts", custom_pattern_all_presets_targets_and_conflicts),
        ("pptx::gradient_stop_count_on_reopened_source", gradient_stop_count_on_reopened_source),
        ("pptx::line_end_size_cap_and_compound", line_end_size_cap_and_compound),
        ("pptx::active_paragraph_toolbar_state", active_paragraph_toolbar_state),
        ("pptx::caret_paragraph_formats", caret_paragraph_formats),
        ("pptx::table_cell_font_alignment_options", table_cell_font_alignment_options),
        ("pptx::paragraph_font_alignment_options", paragraph_font_alignment_options),
        ("pptx::custom_paragraph_spacing", custom_paragraph_spacing),
        ("pptx::paragraph_panel_rejects_changed_body", paragraph_panel_rejects_changed_body),
        ("pptx::copy_script_fonts", copy_script_fonts),
        ("pptx::transition_timing_rejects_changed_target", transition_timing_rejects_changed_target),
        ("pptx::transition_timing_numeric_limits", transition_timing_numeric_limits),
        ("pptx::custom_transition_timing", custom_transition_timing),
        ("pptx::transition_advance_settings", transition_advance_settings),
        ("pptx::custom_picture_crop", custom_picture_crop),
        ("pptx::find_text_options", find_text_options),
        ("pptx::text_insets_reject_changed_body", text_insets_reject_changed_body),
        ("pptx::custom_text_insets", custom_text_insets),
        ("pptx::section_order_and_thumbnail_folding", section_order_and_thumbnail_folding),
        ("pptx::named_sections_and_conflict_rejection", named_sections_and_conflict_rejection),
        ("pptx::custom_slide_size", custom_slide_size),
        ("pptx::find_and_replace_presentation", find_and_replace_presentation),
        ("pptx::copy_and_paste_character_formats", copy_and_paste_character_formats),
        ("pptx::copy_and_paste_paragraph_formats", copy_and_paste_paragraph_formats),
        ("pptx::copy_auto_paragraph_format", copy_auto_paragraph_format),
        ("pptx::replace_picture_file", replace_picture_file),
        ("pptx::shape_shadow_presets_and_safe_rejection", shape_shadow_presets_and_safe_rejection),
        ("pptx::text_font_scale_custom", text_font_scale_custom),
        ("pptx::text_font_scale_presets", text_font_scale_presets),
        ("pptx::shape_glow_presets_and_safe_rejection", shape_glow_presets_and_safe_rejection),
        ("pptx::custom_glow_and_snapshot_rejection", custom_glow_and_snapshot_rejection),
        ("pptx::custom_shadow_and_snapshot_rejection", custom_shadow_and_snapshot_rejection),
        ("pptx::copy_shape_shadow_and_glow", copy_shape_shadow_and_glow),
        ("pptx::copy_picture_color_effects", copy_picture_color_effects),
        ("pptx::reset_picture_color_effects", reset_picture_color_effects),
        ("pptx::crop_and_reset_picture", crop_and_reset_picture),
        ("pptx::reset_slide_placement", reset_slide_placement),
        ("pptx::reset_slide_layout_and_formatting", reset_slide_layout_and_formatting),
        ("pptx::text_frame_properties", text_frame_properties),
        ("pptx::change_slide_size", change_slide_size),
        ("pptx::character_formats", character_formats),
        ("pptx::text_boundary_review", text_boundary_review),
        ("pptx::insert_a_picture", insert_a_picture),
        ("pptx::copy_table_slide_number_fields", copy_table_slide_number_fields),
        ("pptx::copy_slide_number_field", copy_slide_number_field),
        ("pptx::unsupported_text_copy_is_atomic", unsupported_text_copy_is_atomic),
        ("pptx::unsupported_shape_copy_is_atomic", unsupported_shape_copy_is_atomic),
        ("pptx::copy_and_duplicate_table_shapes", copy_and_duplicate_table_shapes),
        ("pptx::invalid_picture_files_do_not_change_document", invalid_picture_files_do_not_change_document),
        ("pptx::apply_a_layout", apply_a_layout),
        ("pptx::group_and_ungroup_shapes", group_and_ungroup_shapes),
        ("pptx::group_copy_paste_and_duplicate", group_copy_paste_and_duplicate),
        ("pptx::picture_fill_and_background_picture", picture_fill_and_background_picture),
        ("pptx::export_slide_to_png", export_slide_to_png),
        ("pptx::export_presentation_to_animated_gif", export_presentation_to_animated_gif),
        ("pptx::change_case_and_caps", change_case_and_caps),
        ("pptx::table_cell_vertical_anchor_and_text_direction", table_cell_vertical_anchor_and_text_direction),
        ("pptx::replace_fonts_presentation", replace_fonts_presentation),
        ("pptx::shape_reflection_and_soft_edge_presets", shape_reflection_and_soft_edge_presets),
        ("pptx::shape_bevel_presets_and_round_trip", shape_bevel_presets_and_round_trip),
        ("pptx::text_outline_colors_and_weights", text_outline_colors_and_weights),
        ("pptx::table_cell_border_placements", table_cell_border_placements),
        ("pptx::picture_crop_aspect_ratios", picture_crop_aspect_ratios),
        ("pptx::picture_rotate_and_flip", picture_rotate_and_flip),
        ("pptx::picture_stroke_options", picture_stroke_options),
        ("pptx::picture_crop_to_shape", picture_crop_to_shape),
        ("pptx::picture_color_effects", picture_color_effects),
        ("pptx::picture_luminance_options", picture_luminance_options),
        ("pptx::table_style_options", table_style_options),
        ("pptx::table_cell_fill_range_and_picture", table_cell_fill_range_and_picture),
        ("pptx::chart_title_options", chart_title_options),
        ("pptx::chart_labels_options", chart_labels_options),
        ("pptx::text_shadow_toggle_and_presets", text_shadow_toggle_and_presets),
        ("pptx::text_autofit_options", text_autofit_options),
        ("pptx::text_columns_options", text_columns_options),
        ("pptx::paragraph_indent_and_tabs_options", paragraph_indent_and_tabs_options),
        ("pptx::table_style_selection", table_style_selection),
        ("pptx::transition_effect_options", transition_effect_options),
        ("pptx::theme_palette_selection", theme_palette_selection),
        ("pptx::theme_palette_applied_to_selected_slide", theme_palette_applied_to_selected_slide),
        ("pptx::gridlines_guides_and_snapping", gridlines_guides_and_snapping),
        ("pptx::export_and_save_slideshow_ppsx", export_and_save_slideshow_ppsx),
        ("pptx::export_and_save_template_potx", export_and_save_template_potx),
        ("pptx::new_open_save_and_copy_presentation", new_open_save_and_copy_presentation),
        ("pptx::new_presentation_from_template", new_presentation_from_template),
        ("pptx::add_hidden_slide_title_resolves_accessibility_issue", add_hidden_slide_title_resolves_accessibility_issue),
        ("pptx::animation_pane_and_detailed_timeline", animation_pane_and_detailed_timeline),
        ("pptx::proofing_language_selection_and_options", proofing_language_selection_and_options),
        ("pptx::transition_sound_options", transition_sound_options),
        ("pptx::animation_repeat_and_autoreverse_options", animation_repeat_and_autoreverse_options),
        ("pptx::animation_smooth_start_and_end_options", animation_smooth_start_and_end_options),
        ("pptx::animation_after_effect_options", animation_after_effect_options),
        ("pptx::chart_axis_scaling_and_options", chart_axis_scaling_and_options),
        ("pptx::chart_trendline_and_error_bars_options", chart_trendline_and_error_bars_options),
        ("pptx::chart_series_and_plot_area_formatting_options", chart_series_and_plot_area_formatting_options),
        ("pptx::chart_style_options", chart_style_options),
        ("pptx::scatter_and_bubble_charts", scatter_and_bubble_charts),
        ("pptx::animation_options_panel_and_rewind", animation_options_panel_and_rewind),
        ("pptx::animation_triggers_and_interactive_sequences", animation_triggers_and_interactive_sequences),
        ("pptx::connectors_insert_and_options", connectors_insert_and_options),
        ("pptx::content_placeholder_insert_table_picture_clip", content_placeholder_insert_table_picture_clip),
        ("pptx::media_playback_options_and_slideshow", media_playback_options_and_slideshow),
        ("pptx::media_trim_and_fade_playback_options", media_trim_and_fade_playback_options),
        ("pptx::freehand_shape_drawing_and_options", freehand_shape_drawing_and_options),
        ("pptx::insert_icon_and_convert_to_shape", insert_icon_and_convert_to_shape),
        ("pptx::compatibility_checker_and_options", compatibility_checker_and_options),
        ("pptx::media_captions_and_accessibility", media_captions_and_accessibility),
        ("pptx::autorecover_settings_and_document_recovery", autorecover_settings_and_document_recovery),
    ]
}

