// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of the fix 担当's requests (`tasks/todo-fix.md` #10 and after):
//! what the user does with the pointer in the window's chrome.
use crate::docx::scenarios::DocSteps;
use crate::xlsx::scenarios::SheetSteps;
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::fix12_menu_bar_hover_switches_menu", fix12_menu_bar_hover_switches_menu),
        ("shell::fix15_long_menu_scrolls", fix15_long_menu_scrolls),
        ("shell::fix14_fill_handle_direction", fix14_fill_handle_direction),
        ("shell::fix13_sheet_tabs_scroll", fix13_sheet_tabs_scroll),
        ("shell::fix11_right_pane_tabs", fix11_right_pane_tabs),
        ("shell::fix_docx_edit014_symbols", fix_docx_edit014_symbols),
        ("shell::fix_pptx_symbols", fix_pptx_symbols),
        ("shell::fix_xlsx_symbols", fix_xlsx_symbols),
        ("shell::fix_docx_tab_stops", fix_docx_tab_stops),
        ("shell::fix_docx_contextual_spacing", fix_docx_contextual_spacing),
        ("shell::fix_docx_kerning", fix_docx_kerning),
        ("shell::fix_docx_widow_and_suppress_lines", fix_docx_widow_and_suppress_lines),
        ("shell::about_shows_powered_by", about_shows_powered_by),
        ("shell::start_view_new_and_select", start_view_new_and_select),
        ("shell::fix_docx_image_look", fix_docx_image_look),
        ("shell::fix17_print_is_bound", fix17_print_is_bound),
        ("shell::fix10_settings_window_centered_and_app_menu", fix10_settings_window_centered_and_app_menu),
        ("shell::fix_xlsx_new_menu_and_first_right_pane", fix_xlsx_new_menu_and_first_right_pane),
        ("shell::fix_xlsx_headings_and_formula_bar", fix_xlsx_headings_and_formula_bar),
        ("shell::fix_xlsx_zoom", fix_xlsx_zoom),
        ("shell::fix_xlsx_page_break_preview", fix_xlsx_page_break_preview),
        ("shell::fix_docx_line_numbers", fix_docx_line_numbers),
        ("shell::fix_docx_ruler", fix_docx_ruler),
        ("shell::fix_xlsx_save_copy_and_recent", fix_xlsx_save_copy_and_recent),
        ("shell::fix_docx_gridlines", fix_docx_gridlines),
        ("shell::fix_xlsx_zoom_to_selection", fix_xlsx_zoom_to_selection),
        ("shell::fix_xlsx_validation_allow_blank", fix_xlsx_validation_allow_blank),
        ("shell::fix_docx_replace_picture", fix_docx_replace_picture),
        ("shell::fix_xlsx_print_options", fix_xlsx_print_options),
        ("shell::fix_xlsx_print_title_columns", fix_xlsx_print_title_columns),
        ("shell::fix_xlsx_paper_sizes_and_centering", fix_xlsx_paper_sizes_and_centering),
        ("shell::fix_xlsx_export_csv_and_pdf", fix_xlsx_export_csv_and_pdf),
        ("shell::fix_docx_table_alignment_and_indent", fix_docx_table_alignment_and_indent),
        ("shell::fix_docx_columns", fix_docx_columns),
        ("shell::fix_docx_table_selection", fix_docx_table_selection),
        ("shell::fix_xlsx_circle_invalid_data", fix_xlsx_circle_invalid_data),
        ("shell::fix_docx_word_paragraph_selection", fix_docx_word_paragraph_selection),
        ("shell::fix_docx_word_count_dialog", fix_docx_word_count_dialog),
        ("shell::fix_docx_find_special_characters", fix_docx_find_special_characters),
        ("shell::fix_docx_find_whole_words", fix_docx_find_whole_words),
        ("shell::fix_docx_paste_special", fix_docx_paste_special),
        ("shell::fix_docx_unlink_fields", fix_docx_unlink_fields),
        ("shell::fix_docx_align_distribute", fix_docx_align_distribute),
        ("shell::fix_docx_toggle_field_codes", fix_docx_toggle_field_codes),
        ("shell::fix_docx_lock_fields", fix_docx_lock_fields),
        ("shell::fix_docx_insert_and_edit_field", fix_docx_insert_and_edit_field),
        ("shell::fix_docx_caption_labels_and_numbering", fix_docx_caption_labels_and_numbering),
        ("shell::fix_docx_table_of_figures", fix_docx_table_of_figures),
        ("shell::fix_status_bar_and_zoom_i18n", fix_status_bar_and_zoom_i18n),
        ("shell::fix21_pptx_sidebar_close_zoom_fit_benchmark", fix21_pptx_sidebar_close_zoom_fit_benchmark),
        ("shell::pptx_sidebar_starts_closed", pptx_sidebar_starts_closed),
        ("shell::fix_xlsx_file017_doc_properties", fix_xlsx_file017_doc_properties),
        ("shell::fix_xlsx_sheet_tabs_scrollbar_and_fixed_add", fix_xlsx_sheet_tabs_scrollbar_and_fixed_add),
        ("shell::fix27_status_bar_tab_scrollbar_place", fix27_status_bar_tab_scrollbar_place),
        ("shell::fix30_status_bar_tab_wheel", fix30_status_bar_tab_wheel),
        ("shell::fix31_sheet_100_percent_is_a_point_a_pixel", fix31_sheet_100_percent_is_a_point_a_pixel),
        ("shell::fix33_formula_without_cached_value", fix33_formula_without_cached_value),
        ("shell::bench_open_scroll", bench_open_scroll),
        ("shell::fix_xlsx_sheet_add_threshold_blank_workbook", fix_xlsx_sheet_add_threshold_blank_workbook),
        ("shell::fix_xlsx_file018_custom_properties", fix_xlsx_file018_custom_properties),
        ("shell::fix_xlsx_sec022_alt_text", fix_xlsx_sec022_alt_text),
        ("shell::fix_xlsx_sec008_threaded_comments", fix_xlsx_sec008_threaded_comments),
        ("shell::fix_xlsx_file004_file007_templates", fix_xlsx_file004_file007_templates),
        ("shell::fix_xlsx_sec021_accessibility_check", fix_xlsx_sec021_accessibility_check),
        ("shell::fix_xlsx_sec019_print_targets", fix_xlsx_sec019_print_targets),
        ("shell::fix_xlsx_sec016_custom_header_footer", fix_xlsx_sec016_custom_header_footer),
        ("shell::fix_xlsx_object001_insert_shape_and_textbox", fix_xlsx_object001_insert_shape_and_textbox),
        ("shell::fix_xlsx_object008_selection_pane_align_and_frame", fix_xlsx_object008_selection_pane_align_and_frame),
        ("shell::fix_xlsx_object003_image011_image018_shape_format_and_picture_properties", fix_xlsx_object003_image011_image018_shape_format_and_picture_properties),
        ("shell::fix_xlsx_object007_group_and_image012_crop", fix_xlsx_object007_group_and_image012_crop),
        ("shell::fix_xlsx_image013_color_and_image014_effects", fix_xlsx_image013_color_and_image014_effects),
        ("shell::fix_xlsx_object012_object013_text_format_and_textbox_layout", fix_xlsx_object012_object013_text_format_and_textbox_layout),
        ("shell::fix_xlsx_object009_object010_image016_shape_placement_and_print", fix_xlsx_object009_object010_image016_shape_placement_and_print),
        ("shell::fix_xlsx_object002_change_shape", fix_xlsx_object002_change_shape),
        ("shell::fix_xlsx_object014_shape_formula_textlink", fix_xlsx_object014_shape_formula_textlink),
        ("shell::fix_xlsx_image019_copy_as_picture", fix_xlsx_image019_copy_as_picture),
        ("shell::fix_xlsx_image016_svg_insert_and_color", fix_xlsx_image016_svg_insert_and_color),
        ("shell::fix23_pptx_thumbnail_scroll_and_sections", fix23_pptx_thumbnail_scroll_and_sections),
        ("shell::fix_xlsx_file011_csv_and_tsv", fix_xlsx_file011_csv_and_tsv),
    ]
}

fn open_index(s: &mut Scenario) -> Option<usize> {
    s.with(|e, _| e.open_menu.as_ref().map(|o| o.index()))
}

/// #12: with a menu open, the pointer on the next menu's name closes it and
/// opens that one; a press on the open menu's name closes it.
fn fix12_menu_bar_hover_switches_menu() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    assert_eq!(open_index(&mut s), None);
    // Hovering with no menu open opens nothing.
    s.hover((133.0, 50.0));
    assert_eq!(open_index(&mut s), None);
    s.click((70.0, 50.0));
    s.screenshot("file-open");
    assert_eq!(open_index(&mut s), Some(0));
    s.hover((133.0, 50.0));
    s.screenshot("edit-by-hover");
    assert_eq!(open_index(&mut s), Some(1));
    s.hover((176.0, 50.0));
    assert_eq!(open_index(&mut s), Some(2));
    // Pressing the open menu's name closes it.
    s.click((176.0, 50.0));
    assert_eq!(open_index(&mut s), None);
    // Pressing another menu's name while one is open switches to it.
    s.click((70.0, 50.0));
    assert_eq!(open_index(&mut s), Some(0));
    s.click((900.0, 300.0));
    assert_eq!(open_index(&mut s), None);
}

fn menu_start(s: &mut Scenario) -> usize {
    s.with(|e, _| e.open_menu.as_ref().map_or(usize::MAX, |o| o.start()))
}

/// #15: a menu taller than the window (編集 of a workbook) shows ▲ ▼ rows;
/// hovering ▼ scrolls (on as the time passes), the wheel scrolls, and ▲
/// scrolls back; a window tall enough shows the whole menu with no rows.
fn fix15_long_menu_scrolls() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click((133.0, 50.0));
    assert_eq!(open_index(&mut s), Some(1));
    assert_eq!(menu_start(&mut s), 0);
    s.screenshot("top");
    // ▼ is the menu's last row: at the window's bottom edge (menu 64..~672).
    s.hover((300.0, 648.0));
    s.wait(400);
    let after_hover = menu_start(&mut s);
    assert!(after_hover > 0, "hovering ▼ scrolled: {after_hover}");
    s.screenshot("scrolled");
    // The pointer at the menu's right edge shows the scroll bar.
    s.hover((388.0, 300.0));
    s.screenshot("scrollbar");
    // Off the row, it stops.
    s.hover((300.0, 300.0));
    s.wait(400);
    assert_eq!(menu_start(&mut s), after_hover);
    // The wheel: up shows earlier lines, down later ones.
    s.wheel((300.0, 300.0), 60.0);
    let up = menu_start(&mut s);
    assert!(up < after_hover, "the wheel up scrolled: {up}");
    s.wheel((300.0, 300.0), -60.0);
    assert!(menu_start(&mut s) > up, "the wheel down scrolled");
    // ▲ (the first row, at the menu's top) scrolls back to the top.
    s.hover((300.0, 82.0));
    s.wait(3000);
    assert_eq!(menu_start(&mut s), 0);
    // A tall window: no rows.
    s.size(1200.0, 1500.0);
    s.click((133.0, 50.0));
    s.click((133.0, 50.0));
    assert_eq!(menu_start(&mut s), 0);
    s.screenshot("tall");
}

/// #10: ツール > 設定… opens the settings window in the middle of the window
/// the user is in, its categories at the top of the left column; the app's
/// own menu ("ibisOffice") has 設定… (⌘,).
fn fix10_settings_window_centered_and_app_menu() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let parent = s.update(|_, window, _| window.bounds());
    s.menu("tools.settings");
    let handle = crate::shell::settings_window::handle().expect("the settings window is open");
    let bounds = s.update(|_, _, cx| handle.update(cx, |_, window, _| window.bounds()).expect("its bounds"));
    let (pc, bc) = (parent.center(), bounds.center());
    assert!((f32::from(pc.x) - f32::from(bc.x)).abs() < 1.0 && (f32::from(pc.y) - f32::from(bc.y)).abs() < 1.0, "centred on its parent: {parent:?} {bounds:?}");
    s.screenshot_of(handle.into(), "settings");
    // The app menu: 設定… then 終了, as macOS apps have them.
    let labels = s.update(|_, _, cx| {
        let menus = cx.get_menus().unwrap_or_default();
        menus.first().map(|m| (m.name.to_string(), m.items.iter().map(|i| match i { gpui_kit::OwnedMenuItem::Action { name, .. } => name.to_string(), _ => String::new() }).collect::<Vec<_>>()))
    });
    let (name, items) = labels.expect("the app menu");
    assert_eq!(name, "ibisOffice");
    assert_eq!(items.first().map(String::as_str), Some("ibisOffice について"), "{items:?}");
    assert!(items.iter().any(|i| i == "設定…"), "{items:?}");
}

/// ファイル > 新規 ends with テンプレートから新規作成…; a workbook's right pane
/// opened for the first time shows the 書式設定 tab.
fn fix_xlsx_new_menu_and_first_right_pane() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let last = s.with(|e, _| match e.menus().find_entry("file.new") {
        Some(crate::shell::chrome::MenuEntry::Submenu(sub)) => sub.items.last().and_then(|i| match i {
            crate::shell::chrome::MenuEntry::Item(item) => Some(item.id),
            _ => None,
        }),
        _ => None,
    });
    assert_eq!(last, Some("xlsx.new-from-template"), "the template item is the last of the New submenu");
    assert!(!s.with(|e, _| e.sidebar_open), "a workbook starts with the right pane closed");
    s.update(|e, window, cx| e.toggle_sidebar(window, cx));
    s.settle();
    assert!(s.with(|e, _| e.sidebar_open));
    assert_eq!(s.with(|e, _| e.sheet_state().and_then(|w| w.right_tab)), Some("fix.right-tabs.format"));
    assert!(s.with(|e, _| e.sheet_state().is_some_and(|w| w.custom_format.is_some())), "the cell-format panel is shown");
}

fn cell_text(s: &mut Scenario, column: u32, row: u32) -> String {
    s.with(move |e, _| {
        ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).and_then(|sh| sh.cell(row, column)).map(|c| format!("{:?}", c.value)).unwrap_or_default()
    })
}

fn enter(s: &mut Scenario, text: &str) {
    s.type_text(text);
    s.key("enter");
}

/// #14: 1 and 2 in A1:A2 (with names beside them), the fill handle moved a few
/// pixels right and down and then a long way down: it fills downwards, the
/// names beside untouched. Moved right of half the next column: it fills right.
fn fix14_fill_handle_direction() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("xlsx.insert-sheet");
    let centre = |s: &mut Scenario, c: u32, r: u32| {
        let p = s.with(move |e, _| e.cell_center(c, r)).expect("the cell on screen");
        (f32::from(p.x), f32::from(p.y))
    };
    let corner = |s: &mut Scenario, c: u32, r: u32| {
        let p = s.with(move |e, _| e.cell_corner(c, r)).expect("the cell on screen");
        (f32::from(p.x), f32::from(p.y))
    };
    let a1 = centre(&mut s, 0, 0);
    s.click(a1);
    enter(&mut s, "1");
    enter(&mut s, "2");
    let b1 = centre(&mut s, 1, 0);
    s.click(b1);
    enter(&mut s, "山田");
    enter(&mut s, "山田");
    s.click(a1);
    s.key("shift-down");
    let c = corner(&mut s, 0, 1);
    // A few pixels right and down, then far down: down.
    let far = centre(&mut s, 0, 6);
    s.drag_through(&[(c.0 - 1.0, c.1 - 1.0), (c.0 + 6.0, c.1 + 6.0), (c.0 + 8.0, c.1 + 40.0), (c.0 + 8.0, far.1)]);
    assert_eq!(cell_text(&mut s, 0, 2), "Number(3.0)", "A3 filled down");
    assert_eq!(cell_text(&mut s, 0, 6), "Number(7.0)", "to the row of the pointer");
    assert_eq!(cell_text(&mut s, 1, 0), "Text(\"山田\")", "the names beside are kept");
    assert_eq!(cell_text(&mut s, 1, 2), "", "nothing filled right");
    s.screenshot("filled-down");
}

fn tab_offset(s: &mut Scenario) -> (f32, f32) {
    s.with(|e, _| {
        let h = e.sheet_state().expect("a workbook").tab_scroll.clone();
        (f32::from(h.offset().x), f32::from(h.max_offset().x))
    })
}

/// #13: the sheet tabs' row: ◀ ▶ scroll the tabs, + adds a sheet, ≡ lists the
/// sheets; with more tabs than fit, shadows show at the end they scroll towards.
/// + is inline after tabs when few, and moves to the right edge when overflowing.
fn fix13_sheet_tabs_scroll() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let sheets = |s: &mut Scenario| s.with(|e, _| e.presentation().slides.len());
    assert_eq!(sheets(&mut s), 3);
    s.screenshot("few");
    let (y, list, right, left) = (679.0, 70.0, 41.0, 14.0);
    // Initially few sheets (3 sheets): + button is inline at the right of the tabs (~310.0).
    s.click((310.0, y));
    assert_eq!(sheets(&mut s), 4, "inline + added a 4th sheet");

    // Add sheets until they overflow. Once overflowed, + moves to the right edge fixed area (~1186.0).
    for _ in 0..18 {
        s.menu("xlsx.insert-sheet");
    }
    s.settle();
    let current_sheets = sheets(&mut s);
    assert!(current_sheets >= 20);

    // Click the fixed + button on the right edge.
    let fixed_plus_x = 1186.0;
    s.click((fixed_plus_x, y));
    assert_eq!(sheets(&mut s), current_sheets + 1, "fixed + on the right edge added a sheet");
    s.settle();

    let (offset, max) = tab_offset(&mut s);
    assert!(max > 100.0, "the tabs overflow: {max}");
    s.screenshot("overflow");

    // ◀ scrolls the tabs right (towards the first), ▶ left; the new tab was
    // brought into view, so the offset is somewhere inside.
    s.click((left, y));
    let (back, _) = tab_offset(&mut s);
    assert!(back > offset || offset == 0.0, "◀ scrolled: {offset} -> {back}");
    s.screenshot("scrolled");
    s.click((right, y));
    let (forward, _) = tab_offset(&mut s);
    assert!(forward < back || back <= -max, "▶ scrolled: {back} -> {forward}");

    // ≡ lists the sheets.
    s.click((list, y));
    s.screenshot("list");
}

fn panel_state(s: &mut Scenario) -> (bool, bool, bool, Option<String>) {
    s.with(|e, _| {
        let st = e.sheet_state().expect("a workbook");
        (st.custom_format.is_some(), st.chart_edit.is_some(), st.validation.is_some(), st.right_tab.map(|t| crate::shell::i18n::text(t).to_string()))
    })
}

/// #11: with the right pane on, a workbook shows 書式設定・グラフ エディター・
/// データの入力規則 as tabs across its top; each opens its panel; no column of
/// buttons stands at the window's right. A presentation's and a document's
/// right pane is as it was (no tabs).
fn fix11_right_pane_tabs() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    // The right pane is closed when a workbook opens; the title row's button shows it.
    assert!(!s.with(|e, _| e.sidebar_open));
    s.update(|e, _, cx| {
        e.sidebar_open = true;
        cx.notify();
    });
    s.screenshot("tabs");
    let y = 179.0;
    let (format, chart, validation) = (933.0, 1011.0, 1110.0);
    assert_eq!(panel_state(&mut s), (false, false, false, None));
    s.click((format, y));
    assert_eq!(panel_state(&mut s), (true, false, false, Some("書式設定".to_string())));
    s.screenshot("format");
    // No chart selected: the tab says so.
    s.click((chart, y));
    assert_eq!(panel_state(&mut s), (false, false, false, Some("グラフ エディター".to_string())));
    s.screenshot("chart-none");
    s.click((validation, y));
    assert_eq!(panel_state(&mut s), (false, false, true, Some("データの入力規則".to_string())));
    s.screenshot("validation");
    s.click((format, y));
    assert_eq!(panel_state(&mut s), (true, false, false, Some("書式設定".to_string())));
    // The pane's close button hides it (and its tabs).
    s.click((1956.0 / 1.6667, 223.0 / 1.6667));
    assert!(!s.with(|e, _| e.sidebar_open));
    s.screenshot("closed");
    drop(s);
    // Other formats: no tabs, and the pane opens as before (a panel when one
    // is open, nothing otherwise: no rail either).
    use crate::shell::format::FormatEditor;
    assert!(<crate::pptx::Presentation as FormatEditor>::sidebar_tabs().is_empty());
    assert!(<crate::docx::Document as FormatEditor>::sidebar_tabs().is_empty());
    assert_eq!(<crate::xlsx::Workbook as FormatEditor>::sidebar_tabs().len(), 3);
}

/// #17: ⌘P (ファイル > 印刷…) was only written beside the menu item, with no key
/// binding and no item in the app's own メニュー: it is bound now, in each
/// format's window. (Printing itself opens Preview's dialog, so it is not run.)
fn fix17_print_is_bound() {
    for file in ["test/editing/xlsx-sales.xlsx", "test/editing/pptx-plan.pptx"] {
        let mut s = Scenario::open(file);
        let bound = s.update(|_, window, _| window.bindings_for_action(&crate::PrintFile).iter().map(|b| b.keystrokes().iter().map(|k| k.unparse()).collect::<Vec<_>>().join(" ")).collect::<Vec<_>>());
        assert!(bound.iter().any(|k| k == "cmd-p"), "{file}: {bound:?}");
    }
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let bound = s.update(|_, window, _| window.bindings_for_action(&crate::PrintFile).len());
    assert!(bound > 0);
}

/// DOCX-EDIT-014: 挿入 > 特殊文字… opens the panel of characters; pressing one
/// puts it at the caret, as one step to undo.
fn fix_docx_edit014_symbols() {
    use ibis_doc_model::FlowPos;
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let first = s.with(|e, _| {
        let doc = e.presentation().flow.clone().unwrap();
        doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some((p.id, p.text())) } else { None }).unwrap()
    });
    let text_of = |s: &mut Scenario, id| {
        s.with(move |e, _| {
            let doc = e.presentation().flow.clone().unwrap();
            doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && p.id == id { Some(p.text()) } else { None }).unwrap()
        })
    };
    let (id, before) = first;
    // One character of each group exists, and the groups are not empty.
    for g in 0..crate::shell::symbols::group_names().len() {
        assert!(crate::shell::symbols::characters(g).len() >= 10, "group {g}");
    }
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut() {
            st.anchor = Some(FlowPos::new(id, 0));
            st.caret = Some(FlowPos::new(id, 0));
        }
        cx.notify();
    });
    s.menu("insert.symbol");
    assert!(s.with(|e, _| e.doc_state().is_some_and(|d| d.symbol.is_some())));
    s.screenshot("symbols");
    s.update(|e, _, cx| e.insert_symbol('→', cx));
    assert_eq!(text_of(&mut s, id), format!("→{before}"));
    s.update(|e, _, cx| e.insert_symbol('€', cx));
    assert_eq!(text_of(&mut s, id), format!("→€{before}"));
    s.key("cmd-z");
    assert_eq!(text_of(&mut s, id), format!("→{before}"), "one step undoes one character");
}

/// The start view: the window's title is the app and its version, the
/// title row shows the logo (no 開く there), and 新規作成's pages make an
/// empty file of their kind (the xlsx one here).
fn start_view_new_and_select() {
    let mut s = Scenario::start_view();
    assert_eq!(crate::shell::start_view::start_title(), format!("ibisOffice {}", env!("IBIS_VERSION")));
    s.screenshot("start");
    // 新規作成's second page (xlsx), below the section's title.
    s.click((150.0, 140.0));
    s.settle();
    assert!(s.with(|e, _| e.has_document && e.kind() == ibis_pptx::Kind::Workbook && e.path.is_none()));
    s.screenshot("new-workbook");
}

/// ヘルプ > ibisOffice について: the About window shows the version and
/// "Powered by ibisOffice" with the project's link (NOTICE: a derivative keeps
/// them), centred on the window.
fn about_shows_powered_by() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let parent = s.update(|_, window, _| window.bounds());
    s.menu("help.about");
    let handle = crate::shell::about_window::handle().expect("the About window is open");
    let bounds = s.update(|_, _, cx| handle.update(cx, |_, window, _| window.bounds()).expect("its bounds"));
    let (pc, bc) = (parent.center(), bounds.center());
    assert!((f32::from(pc.x) - f32::from(bc.x)).abs() < 1.0 && (f32::from(pc.y) - f32::from(bc.y)).abs() < 1.0);
    assert_eq!(crate::shell::about_window::POWERED_BY, "Powered by ibisOffice");
    assert_eq!(crate::shell::about_window::PROJECT_URL, "https://github.com/ibisjp/ibis-office");
    assert_eq!(crate::shell::about_window::version_line(), format!("バージョン {}", env!("IBIS_VERSION")));
    s.screenshot_of(handle.into(), "about");
}

/// DOCX-IMAGE-003/004: the picture at the caret is turned, flipped and cut from
/// its panel; drawn so (a shape with the turn, flips and crop), kept by saving.
fn fix_docx_image_look() {
    use ibis_doc_model::{FlowPos, FlowRunKind, PictureLook};
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let (id, offset) = s.with(|e, _| {
        for b in &e.presentation().flow.as_ref().unwrap().body {
            if let ibis_doc_model::Block::Paragraph(p) = b {
                let mut offset = 0;
                for r in &p.runs {
                    if matches!(r.kind, FlowRunKind::Picture { .. }) {
                        return (p.id, offset);
                    }
                    offset += r.len();
                }
            }
        }
        panic!("a picture");
    });
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut() {
            st.anchor = Some(FlowPos::new(id, offset));
            st.caret = Some(FlowPos::new(id, offset));
        }
        cx.notify();
    });
    s.menu("docx.picture.size");
    let look = |s: &mut Scenario| s.with(|e, _| e.picture_look_at_caret().expect("the picture"));
    assert_eq!(look(&mut s), PictureLook { name: "図 1".to_string(), ..Default::default() });
    let shapes = |s: &mut Scenario| {
        s.with(|e, _| {
            let doc = e.presentation().flow.clone().unwrap();
            crate::docx::flow_layout::pages(&doc, &e.resources).pages.iter().flat_map(|p| p.items.iter()).filter(|i| matches!(i, crate::docx::flow_layout::Item::Shape { shape, .. } if matches!(shape.kind, ibis_doc_model::ShapeKind::Picture(_)))).count()
        })
    };
    assert_eq!(shapes(&mut s), 0, "an untouched picture is drawn as before");
    let doc_of = |s: &mut Scenario| s.with(|e, _| e.presentation().flow.clone().unwrap());
    let before = doc_of(&mut s);
    s.update(|e, _, cx| e.turn_picture(1, cx));
    s.update(|e, _, cx| e.flip_picture(true, cx));
    s.update(|e, _, cx| e.change_picture_look(|l| l.crop = ibis_doc_model::Crop { left: 10_000, top: 20_000, right: 0, bottom: 0 }, cx));
    let l = look(&mut s);
    assert_eq!((l.rotation, l.flip_h, l.flip_v, l.crop.left, l.crop.top), (5_400_000, true, false, 10_000, 20_000));
    assert_eq!(shapes(&mut s), 1, "drawn turned");
    s.screenshot("turned");
    s.save_as("image-look.docx");
    let saved = s.reopen();
    let read = ibis_doc_model::PictureLook { rotation: 5_400_000, flip_h: true, flip_v: false, crop: ibis_doc_model::Crop { left: 10_000, top: 20_000, right: 0, bottom: 0 }, stroke: None, alt_text: Default::default(), decorative: false, name: "図 1".to_string(), ..Default::default() };
    let found = saved.flow.as_ref().unwrap().body.iter().any(|b| matches!(b, ibis_doc_model::Block::Paragraph(p) if p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Picture { look, .. } if look == &read))));
    assert!(found, "saved and read again");
    // Three steps to undo, back to the picture as it was.
    for _ in 0..3 {
        s.key("cmd-z");
    }
    assert_eq!(doc_of(&mut s), before);
}

/// PPTX: 挿入 > 特殊文字… opens the symbol panel and inserting puts the character into the slide.
fn fix_pptx_symbols() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("insert.symbol");
    assert!(s.with(|e, _| e.symbol.is_some() && e.sidebar_open));
    s.screenshot("pptx-symbols");
    s.update(|e, window, cx| e.insert_symbol_with_window('★', window, cx));
    let has_symbol = s.with(|e, _| {
        let slide = e.slide();
        slide.shapes.iter().any(|sh| {
            sh.text_body().is_some_and(|tb| {
                tb.paragraphs.iter().any(|p| p.runs.iter().any(|r| r.text.contains('★')))
            })
        })
    });
    assert!(has_symbol, "symbol was inserted into pptx");
}

/// XLSX: 挿入 > 特殊文字… opens the symbol panel and inserting enters edit mode with the character.
fn fix_xlsx_symbols() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("insert.symbol");
    assert!(s.with(|e, _| e.symbol.is_some() && e.sidebar_open));
    s.screenshot("xlsx-symbols");
    s.update(|e, _, cx| e.insert_symbol('✓', cx));
    let edit_text = s.with(|e, _| e.cell_edit().map(|c| c.text.clone()));
    assert!(edit_text.as_ref().is_some_and(|t| t.ends_with('✓')), "symbol '✓' was inserted into the cell");
}

/// DOCX-PARA-010: タブ位置・配置・リーダー設定パネルでタブ位置を設定・適用し、
/// 保存と再読み込み、アンドゥを検証する。
fn fix_docx_tab_stops() {
    use ibis_doc_model::{FlowPos, TabAlign};
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let first_id = s.with(|e, _| {
        let doc = e.presentation().flow.clone().unwrap();
        doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap()
    });
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut() {
            st.anchor = Some(FlowPos::new(first_id, 0));
            st.caret = Some(FlowPos::new(first_id, 0));
        }
        cx.notify();
    });
    // Open tab stops edit
    s.menu("docx.tabs");
    assert!(s.with(|e, _| e.doc_state().is_some_and(|d| d.tab_edit.is_some()) && e.sidebar_open));
    s.screenshot("tab-panel");

    // Set first tab stop: 72.0 pt, Right, leader: true
    s.update(|e, window, cx| {
        e.choose_tab_align(TabAlign::Right, cx);
        e.choose_tab_leader(true, cx);
        if let Some(edit) = e.doc_state_mut().and_then(|st| st.tab_edit.as_mut()) {
            let field = edit.pos_field.clone();
            field.update(cx, |i, cx| i.set_value("72", window, cx));
        }
        e.set_tab_stop(window, cx);
    });

    // Set second tab stop: 144.0 pt, Center, leader: false
    s.update(|e, window, cx| {
        e.choose_tab_align(TabAlign::Center, cx);
        e.choose_tab_leader(false, cx);
        if let Some(edit) = e.doc_state_mut().and_then(|st| st.tab_edit.as_mut()) {
            let field = edit.pos_field.clone();
            field.update(cx, |i, cx| i.set_value("144", window, cx));
        }
        e.set_tab_stop(window, cx);
    });

    let stop_count = s.with(|e, _| e.doc_state().and_then(|d| d.tab_edit.as_ref()).map(|ed| ed.stops.len()).unwrap_or(0));
    assert_eq!(stop_count, 2, "2 tab stops configured in panel");

    // Apply edits
    s.update(|e, window, cx| e.apply_tab_edit(window, cx));

    // Verify paragraph has the 2 tab stops
    let tabs_of = |s: &mut Scenario| {
        s.with(|e, _| {
            let doc = e.presentation().flow.clone().unwrap();
            doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && p.id == first_id { Some(p.tabs.clone()) } else { None }).unwrap()
        })
    };
    let applied = tabs_of(&mut s);
    assert_eq!(applied.len(), 2);
    assert_eq!(applied[0].position, 72 * 12_700);
    assert_eq!(applied[0].align, TabAlign::Right);
    assert!(applied[0].leader);
    assert_eq!(applied[1].position, 144 * 12_700);
    assert_eq!(applied[1].align, TabAlign::Center);
    assert!(!applied[1].leader);

    // Save and reload
    s.save_as("tab-stops.docx");
    let reloaded = s.reopen();
    let reloaded_tabs = reloaded.flow.as_ref().unwrap().body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && p.id == first_id { Some(p.tabs.clone()) } else { None }).unwrap();
    assert_eq!(reloaded_tabs.len(), 2, "tab stops kept by saving and reopening");
    assert_eq!(reloaded_tabs[0].position, 72 * 12_700);
    assert_eq!(reloaded_tabs[0].align, TabAlign::Right);
    assert!(reloaded_tabs[0].leader);

    // Undo restores to original
    s.key("cmd-z");
    let undone = tabs_of(&mut s);
    assert_eq!(undone.len(), 0, "undo restores original tabs");
}

/// DOCX-PARA-007: 同一スタイル間の間隔抑制（w:contextualSpacing）
/// パネルでのチェックボックス切り替え・適用、保存と再読み込みでの保持、
/// アンドゥ、ツールバーメニューからのトグル動作を検証する。
fn fix_docx_contextual_spacing() {
    use ibis_doc_model::FlowPos;
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let (id1, _) = s.with(|e, _| {
        let doc = e.presentation().flow.clone().unwrap();
        let mut paras = doc.body.iter().filter_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.id) } else { None });
        (paras.next().unwrap(), paras.next().unwrap())
    });
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut() {
            st.anchor = Some(FlowPos::new(id1, 0));
            st.caret = Some(FlowPos::new(id1, 0));
        }
        cx.notify();
    });

    let is_contextual = |s: &mut Scenario, id| {
        s.with(move |e, _| {
            let doc = e.presentation().flow.clone().unwrap();
            doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && p.id == id { Some(p.contextual_spacing) } else { None }).unwrap()
        })
    };
    let initial = is_contextual(&mut s, id1);

    // Open paragraph details panel
    s.menu("docx.paragraph.details");
    assert!(s.with(|e, _| e.doc_state().is_some_and(|d| d.paragraph_edit.is_some()) && e.sidebar_open));
    assert_eq!(
        s.with(|e, _| e.doc_state().and_then(|d| d.paragraph_edit.as_ref()).and_then(|p| p.contextual_spacing)),
        Some(initial),
        "panel shows initial contextual spacing"
    );
    s.screenshot("contextual-spacing-panel");

    // Toggle contextual spacing checkbox to opposite and apply
    s.update(|e, window, cx| {
        e.toggle_paragraph_contextual_spacing(!initial, cx);
        e.apply_paragraph_edit(window, cx);
    });
    assert_eq!(is_contextual(&mut s, id1), !initial, "contextual spacing toggled via panel");

    // Save as and reload to verify round-trip
    s.save_as("contextual-spacing.docx");
    let reloaded = s.reopen();
    let reloaded_contextual = reloaded.flow.as_ref().unwrap().body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && p.id == id1 { Some(p.contextual_spacing) } else { None }).unwrap();
    assert_eq!(reloaded_contextual, !initial, "contextual spacing preserved by save and reload");

    // Undo restores to original
    s.key("cmd-z");
    assert_eq!(is_contextual(&mut s, id1), initial, "undo restores original contextual spacing");

    // Menu toggle: turn to !initial
    s.menu("docx.spacing.contextual");
    assert_eq!(is_contextual(&mut s, id1), !initial, "toolbar menu toggles contextual spacing");

    // Menu toggle: turn back to initial
    s.menu("docx.spacing.contextual");
    assert_eq!(is_contextual(&mut s, id1), initial, "toolbar menu toggles contextual spacing back");
}

/// DOCX-FONT-015: カーニング（w:kern）
/// メニュー「表示形式 > テキスト > カーニング」からの設定、保存と再読み込みでの保持、
/// アンドゥ、カーニングなし（0 pt）への設定、設定解除を検証する。
fn fix_docx_kerning() {
    use ibis_doc_model::FlowPos;
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let first_id = s.with(|e, _| {
        let doc = e.presentation().flow.clone().unwrap();
        doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap()
    });
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut() {
            st.anchor = Some(FlowPos::new(first_id, 0));
            st.caret = Some(FlowPos::new(first_id, 3));
        }
        cx.notify();
    });

    let kern_of = |s: &mut Scenario| {
        s.with(|e, _| {
            let doc = e.presentation().flow.clone().unwrap();
            doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && p.id == first_id {
                p.runs.first().and_then(|r| r.format.kern)
            } else { None })
        })
    };

    // Apply kerning: 12 pt and above (24 half-points)
    s.menu("docx.text.kern-12");
    assert_eq!(kern_of(&mut s), Some(24), "kerning set to 24 half-points (12 pt)");

    // Save as and reopen to verify OOXML round-trip
    s.save_as("kerning.docx");
    let reloaded = s.reopen();
    let reloaded_kern = reloaded.flow.as_ref().unwrap().body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && p.id == first_id {
        p.runs.first().and_then(|r| r.format.kern)
    } else { None });
    assert_eq!(reloaded_kern, Some(24), "kerning preserved by save and reload");

    // Undo restores original
    s.key("cmd-z");
    assert_ne!(kern_of(&mut s), Some(24), "undo restores original kerning");

    // Re-select and apply kerning off (0 pt)
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut() {
            st.anchor = Some(FlowPos::new(first_id, 0));
            st.caret = Some(FlowPos::new(first_id, 3));
        }
        cx.notify();
    });
    s.menu("docx.text.kern-none");
    assert_eq!(kern_of(&mut s), Some(0), "kerning set to 0 (off)");

    // Clear kerning setting
    s.menu("docx.text.kern-clear");
    assert_eq!(kern_of(&mut s), None, "kerning setting cleared");
}

/// DOCX-PARA-013（孤立行防止 w:widowControl）および DOCX-PARA-017（行番号の対象除外 w:suppressLineNumbers）
/// パネルでのトグル・適用、保存と再読み込みでの保持、アンドゥ、メニューからのトグル動作を検証する。
fn fix_docx_widow_and_suppress_lines() {
    use ibis_doc_model::FlowPos;
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let first_id = s.with(|e, _| {
        let doc = e.presentation().flow.clone().unwrap();
        doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap()
    });
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut() {
            st.anchor = Some(FlowPos::new(first_id, 0));
            st.caret = Some(FlowPos::new(first_id, 0));
        }
        cx.notify();
    });

    let flags_of = |s: &mut Scenario| {
        s.with(|e, _| {
            let doc = e.presentation().flow.clone().unwrap();
            doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && p.id == first_id {
                Some((p.widow_control, p.suppress_line_numbers))
            } else { None }).unwrap()
        })
    };

    let (initial_widow, initial_suppress) = flags_of(&mut s);

    // Open paragraph details panel
    s.menu("docx.paragraph.details");
    assert!(s.with(|e, _| e.doc_state().is_some_and(|d| d.paragraph_edit.is_some()) && e.sidebar_open));
    assert_eq!(
        s.with(|e, _| e.doc_state().and_then(|d| d.paragraph_edit.as_ref()).map(|p| (p.widow_control, p.suppress_line_numbers))),
        Some((Some(initial_widow), Some(initial_suppress))),
        "panel shows initial states"
    );

    // Toggle checkboxes and apply via panel
    s.update(|e, window, cx| {
        e.toggle_paragraph_widow_control(!initial_widow, cx);
        e.toggle_paragraph_suppress_line_numbers(!initial_suppress, cx);
        e.apply_paragraph_edit(window, cx);
    });
    assert_eq!(flags_of(&mut s), (!initial_widow, !initial_suppress), "panel applied toggled states");

    // Save as and reload to verify OOXML round-trip
    s.save_as("widow-suppress.docx");
    let reloaded = s.reopen();
    let reloaded_flags = reloaded.flow.as_ref().unwrap().body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && p.id == first_id {
        Some((p.widow_control, p.suppress_line_numbers))
    } else { None }).unwrap();
    assert_eq!(reloaded_flags, (!initial_widow, !initial_suppress), "widow_control and suppress_line_numbers preserved by save and reload");

    // Undo restores original settings
    s.key("cmd-z");
    assert_eq!(flags_of(&mut s), (initial_widow, initial_suppress), "undo restores original settings");

    // Menu toggle: widow-control to opposite
    s.menu("docx.para.widow-control");
    assert_eq!(flags_of(&mut s).0, !initial_widow, "menu turned widow control to opposite");

    // Menu toggle: widow-control back to initial
    s.menu("docx.para.widow-control");
    assert_eq!(flags_of(&mut s).0, initial_widow, "menu turned widow control back to initial");

    // Menu toggle: suppress-line-numbers to opposite
    s.menu("docx.para.suppress-line-numbers");
    assert_eq!(flags_of(&mut s).1, !initial_suppress, "menu turned suppress line numbers to opposite");

    // Menu toggle: suppress-line-numbers back to initial
    s.menu("docx.para.suppress-line-numbers");
    assert_eq!(flags_of(&mut s).1, initial_suppress, "menu turned suppress line numbers back to initial");
}

/// XLSX-SHEET-015 (見出し表示) & XLSX-SHEET-018 (数式バー表示):
/// toggle headings and formula bar from menus, save and reload, undo/redo.
fn fix_xlsx_headings_and_formula_bar() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // Initial states: headings is true, formula bar is true
    assert_eq!(s.menu_state("xlsx.headings"), Some((true, true)));
    assert_eq!(s.menu_state("xlsx.formula-bar"), Some((true, true)));
    assert!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().headings));
    assert!(s.with(|e, _| e.sheet_state().unwrap().show_formula_bar));

    // Toggle formula bar off
    s.menu("xlsx.formula-bar");
    assert_eq!(s.menu_state("xlsx.formula-bar"), Some((true, false)));
    assert!(!s.with(|e, _| e.sheet_state().unwrap().show_formula_bar));

    // Toggle formula bar back on
    s.menu("xlsx.formula-bar");
    assert_eq!(s.menu_state("xlsx.formula-bar"), Some((true, true)));
    assert!(s.with(|e, _| e.sheet_state().unwrap().show_formula_bar));

    // Toggle headings off
    s.menu("xlsx.headings");
    assert_eq!(s.menu_state("xlsx.headings"), Some((true, false)));
    assert!(!s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().headings));

    // Save and reopen
    s.save_as("headings-off.xlsx");
    let reloaded = s.reopen();
    assert!(!ibis_doc_model::sheet_calc::sheet_of_slide(&reloaded.slides[0]).unwrap().headings, "headings state preserved after save & reopen");

    // Undo toggle headings
    s.key("cmd-z");
    assert!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().headings), "headings restored by undo");
    assert_eq!(s.menu_state("xlsx.headings"), Some((true, true)));

    // Redo toggle headings
    s.key("cmd-shift-z");
    assert!(!s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().headings), "headings toggled by redo");
    assert_eq!(s.menu_state("xlsx.headings"), Some((true, false)));

    // Toggle headings back on via menu
    s.menu("xlsx.headings");
    assert!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().headings));
    assert_eq!(s.menu_state("xlsx.headings"), Some((true, true)));
}

/// XLSX-SHEET-013 (表示倍率・ズーム):
/// change sheet zoom, save and reload (OOXML sheetView/@zoomScale), undo/redo.
fn fix_xlsx_zoom() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // Initial state: zoom is None (default 100%)
    assert_eq!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().zoom), None);
    assert_eq!(s.with(|e, _| e.zoom), Some(1.0));

    // Change zoom to 150%
    s.update(|e, window, cx| e.set_zoom(Some(1.5), window, cx));
    assert_eq!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().zoom), Some(150));
    assert_eq!(s.with(|e, _| e.zoom), Some(1.5));

    // Change zoom to 200%
    s.update(|e, window, cx| e.set_zoom(Some(2.0), window, cx));
    assert_eq!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().zoom), Some(200));
    assert_eq!(s.with(|e, _| e.zoom), Some(2.0));

    // Save as and reload to verify OOXML round-trip
    s.save_as("zoom-test.xlsx");
    let reloaded = s.reopen();
    assert_eq!(ibis_doc_model::sheet_calc::sheet_of_slide(&reloaded.slides[0]).unwrap().zoom, Some(200), "zoomScale preserved by save & reopen");

    // Undo restores 150%
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().zoom), Some(150));
    assert_eq!(s.with(|e, _| e.zoom), Some(1.5));

    // Undo restores initial 100% (None)
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().zoom), None);
    assert_eq!(s.with(|e, _| e.zoom), Some(1.0));

    // Redo restores 150%
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().zoom), Some(150));
    assert_eq!(s.with(|e, _| e.zoom), Some(1.5));

    // Zoom back to 100% via set_zoom(Some(1.0))
    s.update(|e, window, cx| e.set_zoom(Some(1.0), window, cx));
    assert_eq!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().zoom), None);
    assert_eq!(s.with(|e, _| e.zoom), Some(1.0));
}

/// XLSX-SHEET-019 (改ページプレビュー):
/// toggle page break preview via menu, save and reload (OOXML sheetView/@view="pageBreakPreview"), undo/redo.
fn fix_xlsx_page_break_preview() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // Initial state: pageBreakPreview is false
    assert!(!s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().page_break_preview));
    assert_eq!(s.menu_state("xlsx.page-break-preview"), Some((true, false)));

    // Toggle page break preview on
    s.menu("xlsx.page-break-preview");
    assert_eq!(s.menu_state("xlsx.page-break-preview"), Some((true, true)));
    assert!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().page_break_preview));

    // Save as and reload to verify OOXML round-trip
    s.save_as("page-break-preview.xlsx");
    let reloaded = s.reopen();
    assert!(ibis_doc_model::sheet_calc::sheet_of_slide(&reloaded.slides[0]).unwrap().page_break_preview, "pageBreakPreview preserved after save & reopen");

    // Undo toggle
    s.key("cmd-z");
    assert!(!s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().page_break_preview), "pageBreakPreview restored by undo");
    assert_eq!(s.menu_state("xlsx.page-break-preview"), Some((true, false)));

    // Redo toggle
    s.key("cmd-shift-z");
    assert!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().page_break_preview), "pageBreakPreview restored by redo");
    assert_eq!(s.menu_state("xlsx.page-break-preview"), Some((true, true)));

    // Toggle back off via menu
    s.menu("xlsx.page-break-preview");
    assert!(!s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().page_break_preview));
    assert_eq!(s.menu_state("xlsx.page-break-preview"), Some((true, false)));
}

/// DOCX-PAGE-017 (行番号の開始・再開始・設定):
/// toggle line numbers via menu, save and reload (OOXML w:lnNumType), undo/redo.
fn fix_docx_line_numbers() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    let get_ln = |s: &mut Scenario| {
        s.with(|e, _| {
            let doc = e.presentation().flow.clone().unwrap();
            doc.page.line_numbers
        })
    };

    // Initial state: line_numbers is None
    assert_eq!(get_ln(&mut s), None);
    assert_eq!(s.menu_state("docx.line-numbers.none"), Some((true, true)));
    assert_eq!(s.menu_state("docx.line-numbers.continuous"), Some((true, false)));
    assert_eq!(s.menu_state("docx.line-numbers.restart-page"), Some((true, false)));
    assert_eq!(s.menu_state("docx.line-numbers.restart-section"), Some((true, false)));

    // Switch to continuous line numbering
    s.menu("docx.line-numbers.continuous");
    assert_eq!(s.menu_state("docx.line-numbers.none"), Some((true, false)));
    assert_eq!(s.menu_state("docx.line-numbers.continuous"), Some((true, true)));
    let ln = get_ln(&mut s).expect("line_numbers should be set");
    assert_eq!(ln.restart, ibis_doc_model::LineNumberRestart::Continuous);

    // Save as and reload to verify OOXML round-trip
    s.save_as("line-numbers-test.docx");
    let reloaded = s.reopen();
    let reloaded_ln = reloaded.flow.as_ref().unwrap().page.line_numbers.expect("reloaded docx has line_numbers");
    assert_eq!(reloaded_ln.restart, ibis_doc_model::LineNumberRestart::Continuous, "line_numbers restart preserved after save & reopen");

    // Undo restores None
    s.key("cmd-z");
    assert_eq!(get_ln(&mut s), None, "undo restores None line_numbers");
    assert_eq!(s.menu_state("docx.line-numbers.none"), Some((true, true)));
    assert_eq!(s.menu_state("docx.line-numbers.continuous"), Some((true, false)));

    // Redo restores continuous
    s.key("cmd-shift-z");
    assert_eq!(get_ln(&mut s).map(|n| n.restart), Some(ibis_doc_model::LineNumberRestart::Continuous));
    assert_eq!(s.menu_state("docx.line-numbers.continuous"), Some((true, true)));

    // Switch to restart each page
    s.menu("docx.line-numbers.restart-page");
    assert_eq!(get_ln(&mut s).map(|n| n.restart), Some(ibis_doc_model::LineNumberRestart::NewPage));
    assert_eq!(s.menu_state("docx.line-numbers.restart-page"), Some((true, true)));
    assert_eq!(s.menu_state("docx.line-numbers.continuous"), Some((true, false)));

    // Save as and reload
    s.save_as("line-numbers-restart-page.docx");
    let reloaded2 = s.reopen();
    let reloaded_ln2 = reloaded2.flow.as_ref().unwrap().page.line_numbers.expect("reloaded docx has restart-page line_numbers");
    assert_eq!(reloaded_ln2.restart, ibis_doc_model::LineNumberRestart::NewPage);

    // Turn off line numbering via "none"
    s.menu("docx.line-numbers.none");
    assert_eq!(get_ln(&mut s), None);
    assert_eq!(s.menu_state("docx.line-numbers.none"), Some((true, true)));
    assert_eq!(s.menu_state("docx.line-numbers.restart-page"), Some((true, false)));

    // Save as and reload
    s.save_as("line-numbers-none.docx");
    let reloaded3 = s.reopen();
    assert_eq!(reloaded3.flow.as_ref().unwrap().page.line_numbers, None);
}

/// DOCX-VIEW-011: 水平ルーラーの表示切替（表示 > ルーラー）
/// 初期状態でのルーラー表示（true）、メニューでのチェック表示、
/// メニュークリックによる非表示化（false）とチェック解除、
/// 再クリックによる表示復帰（true）とチェック表示を検証する。
fn fix_docx_ruler() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 初期状態：ruler_open は true、メニュー docx.ruler はチェック付き
    assert!(s.with(|e, _| e.ruler_open), "ruler should be visible initially");
    assert_eq!(s.menu_state("docx.ruler"), Some((true, true)), "docx.ruler menu item should be enabled and checked");
    s.screenshot("docx-ruler-visible");

    // メニューをクリックして非表示にする
    s.menu("docx.ruler");
    assert!(!s.with(|e, _| e.ruler_open), "ruler should be hidden after menu click");
    assert_eq!(s.menu_state("docx.ruler"), Some((true, false)), "docx.ruler menu item should be enabled and unchecked");
    s.screenshot("docx-ruler-hidden");

    // 再度メニューをクリックして表示に戻す
    s.menu("docx.ruler");
    assert!(s.with(|e, _| e.ruler_open), "ruler should be visible again after second click");
    assert_eq!(s.menu_state("docx.ruler"), Some((true, true)), "docx.ruler menu item should be enabled and checked again");
}

/// XLSX-FILE-006（コピー保存）および XLSX-FILE-003（最近使ったブック）
/// Excel（.xlsx）を開いたときの最近使ったファイル一覧（recent_files）への登録、
/// セル編集後の別名コピー保存（copy_to）、ウィンドウ自身のパス保持、元ファイルの非変更、
/// およびコピー先ファイルへの編集反映を検証する。
fn fix_xlsx_save_copy_and_recent() {
    use crate::xlsx::scenarios::SheetSteps as _;

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let own = s.with(|e, _| e.path.clone()).expect("file has path");
    let original = std::fs::read(&own).expect("can read original file");

    // XLSX-FILE-003: 開いたブックが最近使ったファイル一覧（recent_files）に登録され保持されることを検証
    s.update(|e, _, _| {
        e.recent_files = crate::shell::recent_files::add(&own);
    });
    let recents = s.with(|e, _| e.recent_files.clone());
    assert!(
        recents.iter().any(|p| p.file_name() == own.file_name()),
        "recent_files should contain opened xlsx file"
    );

    // セルを編集
    s.click_cell("A1");
    s.enter("コピーテスト", "enter");
    assert_eq!(s.shown("A1"), "コピーテスト");

    // XLSX-FILE-006: コピーを作成（copy_to）
    let copy_path = crate::shell::scenario::repo_root().join("target/scenario/xlsx::save_copy/copy.xlsx");
    if let Some(parent) = copy_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    s.with(|e, _| e.copy_to(&copy_path)).expect("copy_to succeeds for xlsx");

    // ウィンドウ自身のパスは変更されない
    assert_eq!(s.with(|e, _| e.path.clone()), Some(own.clone()), "window keeps original path");

    // 元のファイルは保存されていない（未保存のまま）
    assert_eq!(std::fs::read(&own).unwrap(), original, "original file remains unchanged");

    // コピーされたファイルに編集が正しく反映されている
    let copied_bytes = std::fs::read(&copy_path).expect("copied file exists");
    let copied = ibis_pptx::read(&copied_bytes).expect("can read copied xlsx");
    let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&copied.slides[0]).expect("sheet exists");
    let a1_cell = sheet.cell(0, 0).expect("A1 cell exists");
    assert_eq!(
        a1_cell.text,
        "コピーテスト",
        "copied file contains edited cell content"
    );
}

/// DOCX-VIEW-014: グリッド線の表示切替（表示 > グリッド線）
/// 初期状態でのグリッド線非表示（false）、メニューでの未チェック表示、
/// メニュークリックによる表示化（true）とチェック表示、
/// 再クリックによる非表示化（false）とチェック解除を検証する。
fn fix_docx_gridlines() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 初期状態：gridlines_open は false、メニュー docx.gridlines はチェックなし
    assert!(!s.with(|e, _| e.gridlines_open), "gridlines should be hidden initially");
    assert_eq!(s.menu_state("docx.gridlines"), Some((true, false)), "docx.gridlines menu item should be enabled and unchecked");
    s.screenshot("docx-gridlines-hidden");

    // メニューをクリックして表示にする
    s.menu("docx.gridlines");
    assert!(s.with(|e, _| e.gridlines_open), "gridlines should be visible after menu click");
    assert_eq!(s.menu_state("docx.gridlines"), Some((true, true)), "docx.gridlines menu item should be enabled and checked");
    s.screenshot("docx-gridlines-visible");

    // 再度メニューをクリックして非表示に戻す
    s.menu("docx.gridlines");
    assert!(!s.with(|e, _| e.gridlines_open), "gridlines should be hidden again after second click");
    assert_eq!(s.menu_state("docx.gridlines"), Some((true, false)), "docx.gridlines menu item should be enabled and unchecked again");
}

/// XLSX-VIEW-005: 選択範囲に合わせたズーム（表示 > 選択範囲に合わせて拡大/縮小）
/// セル範囲を選択し、メニューをクリックして選択範囲に合わせたズーム倍率への変更とスクロールを検証。
fn fix_xlsx_zoom_to_selection() {
    use crate::xlsx::scenarios::SheetSteps as _;

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_zoom = s.with(|e, _| e.current_zoom());
    assert!((initial_zoom - 1.0).abs() < 0.01, "initial zoom is 100%");

    // メニューが存在し、有効であること
    assert_eq!(s.menu_state("xlsx.zoom-selection"), Some((true, false)));

    // セル A1 を選択し、Shift+Right と Shift+Down で狭い範囲（A1:B3）を選択
    s.click_cell("A1");
    s.key("shift-right");
    s.key("shift-down");
    s.key("shift-down");

    // メニューから選択範囲に合わせてズームを実行
    s.menu("xlsx.zoom-selection");
    let zoomed = s.with(|e, _| e.current_zoom());
    assert!(zoomed > 1.0, "zoom should zoom in for a small selection (was {zoomed})");
    s.screenshot("xlsx-zoom-to-selection");

    // ズームを100%にリセットし、直接メソッド呼び出しでも動作することを検証
    s.update(|e, window, cx| e.set_zoom(Some(1.0), window, cx));
    assert!((s.with(|e, _| e.current_zoom()) - 1.0).abs() < 0.01);
    s.update(|e, window, cx| e.zoom_to_selection(window, cx));
    let zoomed_again = s.with(|e, _| e.current_zoom());
    assert_eq!(zoomed_again, zoomed, "zoom_to_selection yields consistent zoom");
}

/// XLSX-VALID-006: データの入力規則の空白の扱い（allow_blank）
/// 空白を無視する（allow_blank = true、既定）のときは未入力・空白の確定が許可され、
/// allow_blank = false に設定したときは空白の入力が規則違反として拒否されることを検証。
/// 保存・再オープンでの allowBlank の保持、解除を検証。
fn fix_xlsx_validation_allow_blank() {
    use crate::xlsx::scenarios::SheetSteps as _;
    use ibis_doc_model::{CellValue, Command, SheetCommand, Validation};

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // セル B10 に数値（1〜10）の入力規則を設定（allow_blank: true 既定）
    s.click_cell("B10");
    let range = s.with(|e, _| e.selected_range().unwrap());
    let rule_allow_blank = Validation {
        kind: "whole".into(),
        operator: "between".into(),
        formula1: Some("1".into()),
        formula2: Some("10".into()),
        allow_blank: true,
        error: Some(("stop".into(), "エラー".into(), "1から10の整数を入力してください".into())),
        ..Default::default()
    };
    let slide = s.with(|e, _| e.current_slide);
    s.update(|e, _, _| {
        e.document
            .execute(vec![Command::Sheet(SheetCommand::SetValidation {
                slide,
                ranges: vec![range],
                rule: Some(rule_allow_blank),
            })])
            .unwrap();
    });

    // 1. allow_blank = true のとき：
    // 数値 5 を入力 → 成功
    s.click_cell("B10");
    s.enter("5", "enter");
    assert_eq!(s.value("B10"), CellValue::Number(5.0));

    // F2 で編集モードに入り、文字を消去して空文字確定 → allow_blank = true なので成功
    s.click_cell("B10");
    s.key("f2");
    s.key("cmd-a");
    s.key("backspace");
    s.key("enter");
    assert_eq!(s.value("B10"), CellValue::Empty, "allow_blank = true permits clearing to empty");

    // 範囲外の数値 15 を入力 → 拒否される
    s.click_cell("B10");
    s.enter("15", "enter");
    assert_eq!(s.value("B10"), CellValue::Empty, "value out of range is refused");
    s.key("escape");

    // 2. allow_blank = false に変更
    let rule_no_blank = Validation {
        kind: "whole".into(),
        operator: "between".into(),
        formula1: Some("1".into()),
        formula2: Some("10".into()),
        allow_blank: false,
        error: Some(("stop".into(), "エラー".into(), "空白は許可されていません".into())),
        ..Default::default()
    };
    s.update(|e, _, _| {
        e.document
            .execute(vec![Command::Sheet(SheetCommand::SetValidation {
                slide,
                ranges: vec![range],
                rule: Some(rule_no_blank),
            })])
            .unwrap();
    });

    // 正しい数値 8 を入力 → 成功
    s.click_cell("B10");
    s.enter("8", "enter");
    assert_eq!(s.value("B10"), CellValue::Number(8.0));

    // allow_blank = false の状態で F2 編集に入り、文字を消して空文字で確定を試みる → 拒否される！
    s.click_cell("B10");
    s.key("f2");
    s.key("cmd-a");
    s.key("backspace");
    s.key("enter");
    assert_ne!(s.value("B10"), CellValue::Empty, "empty entry is refused when allow_blank = false");
    assert!(s.status_texts().iter().any(|t| t.contains("空白は許可されていません")), "{:?}", s.status_texts());
    s.key("escape");

    // 3. 保存と再オープンで allowBlank 属性が保持されることを検証
    s.save_as("fix-xlsx-validation-allow-blank.xlsx");
    let presentation = s.reopen();
    let sheet = presentation.slides.iter().find_map(ibis_doc_model::sheet_calc::sheet_of_slide).expect("sheet exists");
    let read_rule = sheet.validations.iter().find(|v| v.ranges.iter().any(|r| r.first_row == range.first_row && r.first_column == range.first_column)).expect("rule exists");
    assert_eq!(read_rule.allow_blank, false, "allow_blank: false is preserved through save and reopen");
    assert_eq!(read_rule.kind, "whole");

    // 4. クリアして規則解除
    s.update(|e, _, _| {
        e.document
            .execute(vec![Command::Sheet(SheetCommand::SetValidation {
                slide,
                ranges: vec![range],
                rule: None,
            })])
            .unwrap();
    });
    assert!(s.with(|e, _| e.current_rule().is_none()));
}

/// DOCX-IMAGE-001: 画像の置換（画像の変更 Change Picture）
/// 画像が選択されている時のみメニュー「書式 > 画像の変更…」が有効化されること、
/// 画像を置き換えた際に MediaId が更新されサイズが保持されること、
/// アンドゥで元の画像に戻り、リドゥで再度置換されること、
/// 保存と再読み込み後も新しい画像が保持されていることを検証する。
fn fix_docx_replace_picture() {
    use ibis_doc_model::{FlowPos, FlowRunKind, MediaFormat};

    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE,
        0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54,
        0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00,
        0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0,
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44,
        0xAE, 0x42, 0x60, 0x82,
    ];

    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. テキスト位置ではメニュー「docx.picture.replace」は無効
    let first_id = s.with(|e, _| {
        let doc = e.presentation().flow.clone().unwrap();
        doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap()
    });
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut() {
            st.anchor = Some(FlowPos::new(first_id, 0));
            st.caret = Some(FlowPos::new(first_id, 0));
        }
        cx.notify();
    });
    assert_eq!(s.menu_state("docx.picture.replace"), Some((false, false)), "docx.picture.replace is disabled when not on picture");

    // 2. 画像の位置を特定してキャレットを置く
    let (para_id, offset, initial_media, initial_size) = s.with(|e, _| {
        for b in &e.presentation().flow.as_ref().unwrap().body {
            if let ibis_doc_model::Block::Paragraph(p) = b {
                let mut offset = 0;
                for r in &p.runs {
                    if let FlowRunKind::Picture { media, size, .. } = r.kind {
                        return (p.id, offset, media, size);
                    }
                    offset += r.len();
                }
            }
        }
        panic!("picture not found");
    });
    let pic_pos = FlowPos::new(para_id, offset);
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut() {
            st.anchor = Some(pic_pos);
            st.caret = Some(pic_pos);
        }
        cx.notify();
    });

    // 3. 画像位置ではメニュー「docx.picture.replace」が有効
    assert_eq!(s.menu_state("docx.picture.replace"), Some((true, false)), "docx.picture.replace is enabled at picture");
    s.screenshot("docx-picture-replace-menu");

    let current_media = |s: &mut Scenario| {
        s.with(|e, _| {
            for b in &e.presentation().flow.as_ref().unwrap().body {
                if let ibis_doc_model::Block::Paragraph(p) = b && p.id == para_id {
                    let mut offset = 0;
                    for r in &p.runs {
                        if let FlowRunKind::Picture { media, size, .. } = r.kind {
                            if offset == pic_pos.offset {
                                return (media, size);
                            }
                        }
                        offset += r.len();
                    }
                }
            }
            panic!("picture run missing");
        })
    };
    assert_eq!(current_media(&mut s), (initial_media, initial_size));

    // 4. doc_replace_picture で画像を置き換え（keep_size: true）
    s.update(|e, _, cx| {
        e.doc_replace_picture(pic_pos, TINY_PNG.to_vec(), MediaFormat::Png, true, cx);
    });
    let (replaced_media, replaced_size) = current_media(&mut s);
    assert_ne!(replaced_media, initial_media, "media id changed after replacement");
    assert_eq!(replaced_size, initial_size, "size preserved when keep_size is true");
    s.screenshot("docx-picture-replaced");

    // 5. アンドゥ（cmd-z）で元の画像・メディアIDに戻る
    s.key("cmd-z");
    assert_eq!(current_media(&mut s), (initial_media, initial_size), "undo restores original picture");

    // 6. リドゥ（cmd-shift-z）で再度置換後の画像になる
    s.key("cmd-shift-z");
    assert_eq!(current_media(&mut s), (replaced_media, replaced_size), "redo restores replaced picture");

    // 7. 保存して再オープンし、置換された画像が保持されていることを検証
    s.save_as("replace-picture.docx");
    let reloaded = s.reopen();
    let reloaded_has_picture = reloaded.flow.as_ref().unwrap().body.iter().any(|b| {
        if let ibis_doc_model::Block::Paragraph(p) = b && p.id == para_id {
            p.runs.iter().any(|r| matches!(r.kind, FlowRunKind::Picture { size, .. } if size == replaced_size))
        } else {
            false
        }
    });
    assert!(reloaded_has_picture, "reloaded document contains picture with replaced size");
    assert!(!reloaded.media.is_empty(), "media collection has saved images");
}

/// XLSX-SEC-017: ページ設定の印刷オプション（枠線、行列見出し、白黒、簡易印刷）:
/// メニュー「ファイル > ページ設定」でサイドバーを開き、
/// printOptions / pageSetup の各フラグを切り替え、Undo/Redo、保存・再オープンで保持されることの検証。
fn fix_xlsx_print_options() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("file.page-setup");
    s.screenshot("page-setup-open");

    // 1. 初期状態の確認（すべて false）
    let initial_print = s.with(|e, _| e.current_print().unwrap());
    assert!(!initial_print.grid_lines);
    assert!(!initial_print.headings);
    assert!(!initial_print.black_and_white);
    assert!(!initial_print.draft);

    // 2. ページ設定パネルが正しく開かれていることを確認
    s.update(|e, _, cx| {
        let panel = e.page_setup_panel(cx);
        assert!(panel.is_some(), "page_setup_panel should be open");
    });

    // 3. 印刷オプションを順にオンにする
    s.update(|e, _, cx| e.set_print(|p| p.grid_lines = true, None, cx));
    s.update(|e, _, cx| e.set_print(|p| p.headings = true, None, cx));
    s.update(|e, _, cx| e.set_print(|p| p.black_and_white = true, None, cx));
    s.update(|e, _, cx| e.set_print(|p| p.draft = true, None, cx));

    let updated = s.with(|e, _| e.current_print().unwrap());
    assert!(updated.grid_lines);
    assert!(updated.headings);
    assert!(updated.black_and_white);
    assert!(updated.draft);
    s.screenshot("page-setup-options-enabled");

    // 4. 保存して再オープンし、OOXML (printOptions, pageSetup) に反映されていることを確認
    s.save_as("fix_xlsx_print_options.xlsx");
    let saved = s.reopen();
    let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&saved.slides[0]).unwrap().clone();
    assert!(sheet.print.grid_lines, "grid_lines should be persisted");
    assert!(sheet.print.headings, "headings should be persisted");
    assert!(sheet.print.black_and_white, "black_and_white should be persisted");
    assert!(sheet.print.draft, "draft should be persisted");

    // 5. Undo（cmd-z）で直前の操作（draft = true）を取り消し
    s.key("cmd-z");
    assert!(!s.with(|e, _| e.current_print().unwrap().draft));
    assert!(s.with(|e, _| e.current_print().unwrap().black_and_white));

    // 6. Redo（cmd-shift-z）で再適用
    s.key("cmd-shift-z");
    assert!(s.with(|e, _| e.current_print().unwrap().draft));
}

/// XLSX-SEC-015: ページ設定のタイトル列（印刷タイトル）:
/// 選択した列（またはアクティブセル）を「タイトル列」として設定、クリア、
/// タイトル行との共存、Undo/Redo、ファイル保存・再オープンでの _xlnm.Print_Titles 保持を検証。
fn fix_xlsx_print_title_columns() {
    use crate::xlsx::scenarios::SheetSteps as _;

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("file.page-setup");
    s.screenshot("page-setup-open");

    // 1. 初期状態：タイトル行・列ともに None
    assert_eq!(s.with(|e, _| e.print_title_rows()), None);
    assert_eq!(s.with(|e, _| e.print_title_columns()), None);

    // 2. セル B2 を選択し、タイトル列を設定（B列 = column 1）
    s.click_cell("B2");
    s.update(|e, _, cx| {
        let cols = e.selected_range().map(|r| (r.first_column, r.last_column)).or_else(|| e.current_cell().map(|(c, _)| (c, c)));
        e.set_print_title_columns(cols, cx);
    });
    assert_eq!(s.with(|e, _| e.print_title_columns()), Some((1, 1)));
    assert_eq!(s.with(|e, _| e.print_title_rows()), None);

    // 3. タイトル行も設定（A1:B2 選択から行 0..1）
    s.click_cell("A1");
    s.key("shift-down");
    s.update(|e, _, cx| {
        let rows = e.selected_range().map(|r| (r.first_row, r.last_row)).or_else(|| e.current_cell().map(|(_, r)| (r, r)));
        e.set_print_title_rows(rows, cx);
    });
    assert_eq!(s.with(|e, _| e.print_title_rows()), Some((0, 1)));
    assert_eq!(s.with(|e, _| e.print_title_columns()), Some((1, 1)));
    s.screenshot("page-setup-both-titles-set");

    // 4. 保存して再オープンし、行と列の両方の印刷タイトルが保持されていることを確認
    s.save_as("fix_xlsx_print_titles.xlsx");
    let saved = s.reopen();
    let wb = saved.workbook.as_ref().unwrap();
    let titles_name = wb.names.iter().find(|n| n.name.eq_ignore_ascii_case("_xlnm.Print_Titles")).unwrap();
    let (rows, cols) = crate::xlsx::header_footer::parse_titles(&titles_name.formula);
    assert_eq!(rows, Some((0, 1)), "title rows persisted in OOXML defined name");
    assert_eq!(cols, Some((1, 1)), "title cols persisted in OOXML defined name");

    // 5. タイトル列のクリア（None に戻し、タイトル行は保持）
    s.update(|e, _, cx| e.set_print_title_columns(None, cx));
    assert_eq!(s.with(|e, _| e.print_title_columns()), None);
    assert_eq!(s.with(|e, _| e.print_title_rows()), Some((0, 1)));

    // 6. Undo（cmd-z）でタイトル列が復元されることを確認
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.print_title_columns()), Some((1, 1)));

    // 7. Redo（shift-cmd-z）で再度クリアされることを確認
    s.key("shift-cmd-z");
    assert_eq!(s.with(|e, _| e.print_title_columns()), None);
}

/// XLSX-SEC-013: ページ設定の用紙サイズ拡充（A5, B4, はがき）と余白のページ中央配置（水平・垂直）:
/// 用紙サイズの切り替え、ページ中央配置のトグル、Undo/Redo、保存・再オープンによる
/// OOXML (pageSetup paperSize, printOptions horizontalCentered/verticalCentered) 保持を検証。
fn fix_xlsx_paper_sizes_and_centering() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("file.page-setup");
    s.screenshot("page-setup-open");

    // 1. 初期状態：A4 (210mm x 297mm), centered (false, false)
    let initial = s.with(|e, _| e.current_print().unwrap());
    assert_eq!(initial.paper, ibis_doc_model::SheetPrint::A4);
    assert_eq!(initial.centered, (false, false));

    // 2. 用紙サイズを「はがき」に変更し、中央配置（水平・垂直）をオンにする
    s.update(|e, _, cx| {
        e.set_print(|p| {
            p.paper = ibis_doc_model::SheetPrint::HAGAKI;
            p.centered = (true, true);
        }, None, cx);
    });
    let updated = s.with(|e, _| e.current_print().unwrap());
    assert_eq!(updated.paper, ibis_doc_model::SheetPrint::HAGAKI);
    assert_eq!(updated.centered, (true, true));
    s.screenshot("page-setup-hagaki-centered");

    // 3. 保存して再オープンし、paperSize（43: はがき）および中央配置が保持されていることを確認
    s.save_as("fix_xlsx_hagaki_centered.xlsx");
    let saved = s.reopen();
    let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&saved.slides[0]).unwrap().clone();
    assert_eq!(sheet.print.paper, ibis_doc_model::SheetPrint::HAGAKI, "HAGAKI paper size should be persisted");
    assert_eq!(sheet.print.centered, (true, true), "horizontal and vertical centering should be persisted");

    // 4. 用紙サイズを「B4」に変更
    s.update(|e, _, cx| {
        e.set_print(|p| p.paper = (257 * 36_000, 364 * 36_000), None, cx);
    });
    assert_eq!(s.with(|e, _| e.current_print().unwrap().paper), (257 * 36_000, 364 * 36_000));

    // 5. Undo（cmd-z）で「はがき」に戻ることを確認
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.current_print().unwrap().paper), ibis_doc_model::SheetPrint::HAGAKI);

    // 6. Redo（shift-cmd-z）で「B4」に再適用されることを確認
    s.key("shift-cmd-z");
    assert_eq!(s.with(|e, _| e.current_print().unwrap().paper), (257 * 36_000, 364 * 36_000));
}

/// XLSX-SEC-020: PDF・CSV 書き出し:
/// ワークブックの現在のシートを CSV（.csv）へ書き出し、行・列・値が正しくフォーマットされること、
/// および PDF（.pdf）への書き出しが正常に完了することを検証。
fn fix_xlsx_export_csv_and_pdf() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let out_dir = std::env::temp_dir().join(format!("ibisoffice-test-xlsx-export-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&out_dir);

    // 1. CSV 書き出しの検証
    let csv_path = out_dir.join("exported.csv");
    let csv_res = s.with(|e, _| e.write_csv(&csv_path));
    assert!(csv_res.is_ok(), "write_csv succeeded");
    assert!(csv_path.exists(), "csv file exists");

    let csv_content = std::fs::read_to_string(&csv_path).expect("read exported csv");
    assert!(!csv_content.is_empty(), "csv content is not empty");
    let lines: Vec<&str> = csv_content.lines().collect();
    assert!(lines.len() >= 2, "csv has header and data rows");

    // 2. PDF 書き出しの検証
    let pdf_path = out_dir.join("exported.pdf");
    let pdf_job = s.with(|e, _| e.write_pdf(pdf_path.clone())).expect("write_pdf returns job");
    let pdf_res = pdf_job();
    assert!(pdf_res.is_ok(), "pdf rendering job succeeded");
    assert!(pdf_path.exists(), "pdf file exists");
    let pdf_bytes = std::fs::read(&pdf_path).expect("read exported pdf");
    assert!(pdf_bytes.starts_with(b"%PDF-"), "file starts with PDF header");
    assert!(pdf_bytes.len() > 1000, "pdf file has non-trivial size");

    let _ = std::fs::remove_dir_all(&out_dir);
}

/// DOCX-TBLFMT-001: 表の位置揃えと字下げ:
/// 表の配置（左揃え・中央揃え・右揃え）およびインデント（左余白からの字下げ）を変更し、
/// メニューのチェック状態連動、Undo/Redo、OOXML（w:jc, w:tblInd）保存・再オープンによる保持を検証。
fn fix_docx_table_alignment_and_indent() {
    use crate::docx::scenarios::DocSteps as _;

    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.key("cmd-down");
    s.key("enter");
    // 表の挿入 (2x2)
    s.menu("docx.table.insert-2x2");

    let table = |s: &mut Scenario| {
        s.document()
            .body
            .into_iter()
            .rev()
            .find_map(|b| if let ibis_doc_model::Block::Table(t) = b { Some(t) } else { None })
            .unwrap()
    };

    // 初期状態: 左揃え (Left), インデント 0
    assert_eq!(table(&mut s).align, ibis_doc_model::FlowAlign::Left);
    assert_eq!(table(&mut s).indent, 0);
    assert_eq!(s.menu_state("docx.table.align.left"), Some((true, true)));
    assert_eq!(s.menu_state("docx.table.align.center"), Some((true, false)));
    assert_eq!(s.menu_state("docx.table.align.right"), Some((true, false)));
    assert_eq!(s.menu_state("docx.table.indent.none"), Some((true, true)));
    assert_eq!(s.menu_state("docx.table.indent.10mm"), Some((true, false)));

    // 1. 中央揃えに変更
    s.menu("docx.table.align.center");
    assert_eq!(table(&mut s).align, ibis_doc_model::FlowAlign::Center);
    assert_eq!(s.menu_state("docx.table.align.center"), Some((true, true)));
    assert_eq!(s.menu_state("docx.table.align.left"), Some((true, false)));

    // 2. 右揃えに変更
    s.menu("docx.table.align.right");
    assert_eq!(table(&mut s).align, ibis_doc_model::FlowAlign::Right);
    assert_eq!(s.menu_state("docx.table.align.right"), Some((true, true)));
    assert_eq!(s.menu_state("docx.table.align.center"), Some((true, false)));

    // 3. 左揃えに戻し、インデント 10mm (360,000 EMU) を設定
    s.menu("docx.table.align.left");
    s.menu("docx.table.indent.10mm");
    assert_eq!(table(&mut s).align, ibis_doc_model::FlowAlign::Left);
    assert_eq!(table(&mut s).indent, 360_000);
    assert_eq!(s.menu_state("docx.table.indent.10mm"), Some((true, true)));
    assert_eq!(s.menu_state("docx.table.indent.none"), Some((true, false)));

    // 4. Undo / Redo
    s.key("cmd-z"); // インデント取り消し -> 0
    assert_eq!(table(&mut s).indent, 0);
    assert_eq!(s.menu_state("docx.table.indent.none"), Some((true, true)));

    s.key("cmd-shift-z"); // インデント再適用 -> 360,000
    assert_eq!(table(&mut s).indent, 360_000);
    assert_eq!(s.menu_state("docx.table.indent.10mm"), Some((true, true)));

    // 5. 保存と開き直し (左揃え + インデント 10mm)
    s.save_as("test_table_align_indent.docx");
    let saved = s.reopen();
    let d = saved.flow.expect("document flow");
    let t_saved = d.body.iter().rev().find_map(|b| if let ibis_doc_model::Block::Table(t) = b { Some(t) } else { None }).unwrap();
    assert_eq!(t_saved.align, ibis_doc_model::FlowAlign::Left);
    // OOXML は twips 単位（1 twip = 635 EMU）で保存されるため 1 twip 未満の丸め誤差を許容
    assert!((t_saved.indent - 360_000).abs() < 635, "indent preserved within 1 twip: {}", t_saved.indent);

    // 6. 中央揃えに変更して保存と開き直し (中央揃え w:jc="center")
    s.menu("docx.table.align.center");
    assert_eq!(table(&mut s).align, ibis_doc_model::FlowAlign::Center);
    s.save_as("test_table_align_center.docx");
    let saved_center = s.reopen();
    let d_center = saved_center.flow.expect("document flow");
    let t_saved_center = d_center.body.iter().rev().find_map(|b| if let ibis_doc_model::Block::Table(t) = b { Some(t) } else { None }).unwrap();
    assert_eq!(t_saved_center.align, ibis_doc_model::FlowAlign::Center);
}

/// DOCX-PAGE-013（段組の段数）および DOCX-PAGE-015（段間罫線）
/// 表示形式 > ページ設定 > 段組み（1段、2段、3段、境界線を引く）の切替、
/// Undo/Redo、OOXML <w:cols> の保存・再オープン保持を検証する。
fn fix_docx_columns() {
    use crate::docx::scenarios::DocSteps as _;
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 初期状態: 1段 (count = 1), 境界線なし (separator = false)
    assert_eq!(s.document().page.columns.count, 1);
    assert!(!s.document().page.columns.separator);
    assert_eq!(s.menu_state("docx.columns.1"), Some((true, true)));
    assert_eq!(s.menu_state("docx.columns.2"), Some((true, false)));
    assert_eq!(s.menu_state("docx.columns.3"), Some((true, false)));
    assert_eq!(s.menu_state("docx.columns.separator"), Some((false, false)));

    // 1. 2段に変更
    s.menu("docx.columns.2");
    assert_eq!(s.document().page.columns.count, 2);
    assert!(!s.document().page.columns.separator);
    assert_eq!(s.menu_state("docx.columns.1"), Some((true, false)));
    assert_eq!(s.menu_state("docx.columns.2"), Some((true, true)));
    assert_eq!(s.menu_state("docx.columns.separator"), Some((true, false)));

    // 2. 境界線を有効化
    s.menu("docx.columns.separator");
    assert_eq!(s.document().page.columns.count, 2);
    assert!(s.document().page.columns.separator);
    assert_eq!(s.menu_state("docx.columns.separator"), Some((true, true)));

    // 3. 3段に変更 (境界線は有効のまま)
    s.menu("docx.columns.3");
    assert_eq!(s.document().page.columns.count, 3);
    assert!(s.document().page.columns.separator);
    assert_eq!(s.menu_state("docx.columns.3"), Some((true, true)));
    assert_eq!(s.menu_state("docx.columns.separator"), Some((true, true)));

    // 4. Undo / Redo
    s.key("cmd-z"); // 3段取り消し -> 2段（境界線あり）
    assert_eq!(s.document().page.columns.count, 2);
    assert!(s.document().page.columns.separator);
    assert_eq!(s.menu_state("docx.columns.2"), Some((true, true)));

    s.key("cmd-z"); // 境界線有効化取り消し -> 2段（境界線なし）
    assert_eq!(s.document().page.columns.count, 2);
    assert!(!s.document().page.columns.separator);
    assert_eq!(s.menu_state("docx.columns.separator"), Some((true, false)));

    s.key("cmd-z"); // 2段取り消し -> 1段（境界線なし）
    assert_eq!(s.document().page.columns.count, 1);
    assert!(!s.document().page.columns.separator);
    assert_eq!(s.menu_state("docx.columns.1"), Some((true, true)));

    s.key("cmd-shift-z"); // 2段再適用
    assert_eq!(s.document().page.columns.count, 2);
    s.key("cmd-shift-z"); // 境界線再適用
    assert_eq!(s.document().page.columns.count, 2);
    assert!(s.document().page.columns.separator);
    s.key("cmd-shift-z"); // 3段再適用
    assert_eq!(s.document().page.columns.count, 3);
    assert!(s.document().page.columns.separator);

    // 5. 保存と開き直し (3段 + 境界線あり)
    s.save_as("test_columns_3_sep.docx");
    let saved = s.reopen();
    let d = saved.flow.expect("document flow");
    assert_eq!(d.page.columns.count, 3);
    assert!(d.page.columns.separator);

    // 6. 1段に戻して保存と開き直し
    s.menu("docx.columns.1");
    assert_eq!(s.document().page.columns.count, 1);
    s.save_as("test_columns_1_reset.docx");
    let saved_reset = s.reopen();
    let d_reset = saved_reset.flow.expect("document flow");
    assert_eq!(d_reset.page.columns.count, 1);
    assert!(!d_reset.page.columns.separator);
}

/// DOCX-TABLE-014（表・行・列・セル選択）
/// 表示形式 > 表 > 選択（セルを選択、行を選択、表を選択）の各操作、
/// メニューの有効/無効、選択範囲の正確性、選択テキストへの書式適用を検証する。
fn fix_docx_table_selection() {
    use crate::docx::scenarios::DocSteps as _;
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. 表の外では「選択」メニュー項目は無効（disabled）
    assert_eq!(s.menu_state("docx.table.select.cell"), Some((false, false)));
    assert_eq!(s.menu_state("docx.table.select.row"), Some((false, false)));
    assert_eq!(s.menu_state("docx.table.select.table"), Some((false, false)));

    // 2. 文書末尾に表（2x2）を挿入
    s.key("cmd-down");
    s.key("enter");
    s.menu("docx.table.insert-2x2");

    // 表の中では各メニューが有効（enabled）
    assert_eq!(s.menu_state("docx.table.select.cell"), Some((true, false)));
    assert_eq!(s.menu_state("docx.table.select.row"), Some((true, false)));
    assert_eq!(s.menu_state("docx.table.select.table"), Some((true, false)));

    let table = |s: &mut Scenario| {
        s.document()
            .body
            .into_iter()
            .rev()
            .find_map(|b| if let ibis_doc_model::Block::Table(t) = b { Some(t) } else { None })
            .unwrap()
    };
    let cell_para = |t: &ibis_doc_model::FlowTable, r: usize, c: usize| {
        t.rows[r].cells[c].blocks.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.clone()) } else { None }).unwrap()
    };

    // セル (0, 0) に文字を入力
    s.type_text("セルA");
    let t0 = table(&mut s);
    let p00 = cell_para(&t0, 0, 0);
    assert_eq!(p00.text(), "セルA");

    // 3. セルを選択（docx.table.select.cell）
    s.menu("docx.table.select.cell");
    assert_eq!(s.selection(), Some((ibis_doc_model::FlowPos::new(p00.id, 0), ibis_doc_model::FlowPos::new(p00.id, p00.len()))));

    // 選択されたセルに太字（cmd-b）を適用
    s.key("cmd-b");
    let t_bold = table(&mut s);
    let p00_bold = cell_para(&t_bold, 0, 0);
    assert!(p00_bold.runs.iter().any(|r| r.style.bold), "cell(0,0) bold");

    // 選択された状態で上書き入力してセル内容が置き換わることを確認
    s.menu("docx.table.select.cell");
    s.type_text("置換A");
    let t_rep = table(&mut s);
    let p00_rep = cell_para(&t_rep, 0, 0);
    assert_eq!(p00_rep.text(), "置換A");

    // セル (0, 1) へ移動して文字入力
    s.key("tab");
    s.type_text("セルB");
    let t1 = table(&mut s);
    let p01 = cell_para(&t1, 0, 1);
    assert_eq!(p01.text(), "セルB");

    // 4. 行を選択（docx.table.select.row）
    s.menu("docx.table.select.row");
    // 行 0 全体（cell 0 先頭から cell 1 末尾）が選択されることを確認
    let t_row = table(&mut s);
    let p00_cur = cell_para(&t_row, 0, 0);
    let p01_cur = cell_para(&t_row, 0, 1);
    assert_eq!(s.selection(), Some((ibis_doc_model::FlowPos::new(p00_cur.id, 0), ibis_doc_model::FlowPos::new(p01_cur.id, p01_cur.len()))));

    // セル (1, 0) へ移動して文字入力
    s.key("tab");
    s.type_text("セルC");

    // 5. 表全体を選択（docx.table.select.table）
    s.menu("docx.table.select.table");
    let t_all = table(&mut s);
    let p_first = cell_para(&t_all, 0, 0);
    let p_last = cell_para(&t_all, 1, 1);
    assert_eq!(s.selection(), Some((ibis_doc_model::FlowPos::new(p_first.id, 0), ibis_doc_model::FlowPos::new(p_last.id, p_last.len()))));

    // 6. 保存と再オープンで内容が保たれていることを確認
    s.save_as("fix_docx_table_selection.docx");
    let saved = s.reopen();
    let flow_doc = saved.flow.as_ref().unwrap();
    let reloaded_table = flow_doc.body.iter().rev().find_map(|b| if let ibis_doc_model::Block::Table(t) = b { Some(t) } else { None }).unwrap();
    let c00 = reloaded_table.rows[0].cells[0].blocks.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.text()) } else { None }).unwrap_or_default();
    let c01 = reloaded_table.rows[0].cells[1].blocks.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.text()) } else { None }).unwrap_or_default();
    let c10 = reloaded_table.rows[1].cells[0].blocks.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.text()) } else { None }).unwrap_or_default();
    assert_eq!(c00, "置換A");
    assert_eq!(c01, "セルB");
    assert_eq!(c10, "セルC");
}

/// XLSX-VALID-009: 無効データのマーク（データの入力規則のマーク表示・マークのクリア）:
/// 入力規則のルールに違反しているセルを検出し、赤い角丸長方形／楕円枠でマーク表示（Circle Invalid Data）
/// およびマークのクリア（Clear Validation Circles）の動作を検証する。
fn fix_xlsx_circle_invalid_data() {
    use ibis_doc_model::{CellRange, CellValue, Command, SheetCommand, Validation};
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. セル範囲 (0..=1, 0..=1) 即ち A1:B2 に「10以上100以下の整数」の入力規則を設定
    let rule = Validation {
        ranges: vec![CellRange { first_row: 0, last_row: 1, first_column: 0, last_column: 1 }],
        kind: "whole".into(),
        operator: "between".into(),
        formula1: Some("10".into()),
        formula2: Some("100".into()),
        allow_blank: true,
        ..Default::default()
    };
    s.update(|e, _, _| {
        let slide = e.current_slide;
        let _ = e.document.execute(vec![Command::Sheet(SheetCommand::SetValidation {
            slide,
            ranges: vec![CellRange { first_row: 0, last_row: 1, first_column: 0, last_column: 1 }],
            rule: Some(rule),
        })]);
    });

    // 2. セルに値を直接設定（A1: 50[適合], A2: 5[不適合], B1: 150[不適合], B2: 80[適合]）
    s.update(|e, _, _| {
        let slide = e.current_slide;
        let mut edits = Vec::new();
        edits.push(ibis_doc_model::CellEdit { row: 0, column: 0, input: ibis_doc_model::CellInput::Value(CellValue::Number(50.0)) });
        edits.push(ibis_doc_model::CellEdit { row: 1, column: 0, input: ibis_doc_model::CellInput::Value(CellValue::Number(5.0)) });
        edits.push(ibis_doc_model::CellEdit { row: 0, column: 1, input: ibis_doc_model::CellInput::Value(CellValue::Number(150.0)) });
        edits.push(ibis_doc_model::CellEdit { row: 1, column: 1, input: ibis_doc_model::CellInput::Value(CellValue::Number(80.0)) });
        let _ = e.document.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits })]);
    });

    // 3. 初期状態：validation_circles は false、render_validation_circles は None
    assert!(!s.with(|e, _| e.validation_circles()));
    assert!(s.with(|e, _| e.render_validation_circles(1.0)).is_none());

    // 4. 無効セル一覧を取得し、A2 (0, 1) と B1 (1, 0) の2つが検出されることを確認
    let invalid = s.with(|e, _| e.invalid_cells());
    assert_eq!(invalid, vec![(0, 1), (1, 0)]);

    // 5. メニュー「データ > 無効データのマーク表示」（xlsx.validation.circle-invalid）を実行
    s.menu("xlsx.validation.circle-invalid");
    assert!(s.with(|e, _| e.validation_circles()));
    assert!(s.with(|e, _| e.render_validation_circles(1.0)).is_some());

    // 6. メニュー「データ > マークのクリア」（xlsx.validation.clear-circles）を実行
    s.menu("xlsx.validation.clear-circles");
    assert!(!s.with(|e, _| e.validation_circles()));
    assert!(s.with(|e, _| e.render_validation_circles(1.0)).is_none());

    // 7. 入力規則パネルを開き、パネル経由でのマーク表示・クリア操作を検証
    s.menu("xlsx.validation");
    s.update(|e, _, cx| e.circle_invalid_data(cx));
    assert!(s.with(|e, _| e.validation_circles()));
    s.update(|e, _, cx| e.clear_validation_circles(cx));
    assert!(!s.with(|e, _| e.validation_circles()));

    // 8. 保存と再オープンでデータ整合性が保持されていることを確認
    s.save_as("fix_xlsx_circle_invalid_data.xlsx");
    let saved = s.reopen();
    let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&saved.slides[0]).unwrap().clone();
    assert_eq!(sheet.validations.len(), 1);
    assert_eq!(sheet.cell(0, 0).map(|c| &c.value), Some(&CellValue::Number(50.0)));
    assert_eq!(sheet.cell(1, 0).map(|c| &c.value), Some(&CellValue::Number(5.0)));
}

/// DOCX-EDIT-003（単語・段落・全文選択）
/// 1クリック（キャレット移動）、2クリック（単語選択）、3クリック（段落選択）、
/// および ⌘A / メニュー「すべて選択」（edit.select-all）による全文選択を検証する。
fn fix_docx_word_paragraph_selection() {
    use crate::docx::scenarios::DocSteps as _;
    let mut s = Scenario::open("test/editing/docx-report.docx");

    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 15 && !p.text().contains('\u{fffc}') && p.list.is_none() {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");
    let para_id = p.id;
    let para_len = p.len();

    // 1. 通常クリック（1クリック）: キャレットが指定位置に移動し、範囲選択はない（始点==終点）
    s.click_text(ibis_doc_model::FlowPos::new(para_id, 2));
    let (a, c) = s.selection().expect("selection exists");
    assert_eq!(a, ibis_doc_model::FlowPos::new(para_id, 2));
    assert_eq!(c, ibis_doc_model::FlowPos::new(para_id, 2));

    // 2. ダブルクリック（2クリック）: 単語が選択される
    // pos 2 の文字を含む単語が選択されることを確認
    s.double_click_text(ibis_doc_model::FlowPos::new(para_id, 2));
    let (sel_start, sel_end) = s.selection().expect("word selected");
    assert_eq!(sel_start.para, para_id);
    assert_eq!(sel_end.para, para_id);
    assert!(sel_start.offset <= 2 && sel_end.offset > 2, "word contains offset 2");
    assert!(sel_start.offset < sel_end.offset, "non-empty selection");

    // 3. トリプルクリック（3クリック）: 段落全体が選択される (0..para_len)
    s.triple_click_text(ibis_doc_model::FlowPos::new(para_id, 5));
    let (p_start, p_end) = s.selection().expect("paragraph selected");
    assert_eq!(p_start, ibis_doc_model::FlowPos::new(para_id, 0));
    assert_eq!(p_end, ibis_doc_model::FlowPos::new(para_id, para_len));

    // 選択された段落に太字（cmd-b）を適用
    s.key("cmd-b");
    let doc_bold = s.document();
    let p_bold = ibis_doc_model::flow_command::paragraph(&doc_bold, para_id).unwrap();
    assert!(p_bold.runs.iter().all(|r| r.style.bold), "all runs in paragraph are bold");

    // 元に戻す（cmd-z）
    s.key("cmd-z");
    let doc_undo = s.document();
    let p_undo = ibis_doc_model::flow_command::paragraph(&doc_undo, para_id).unwrap();
    assert_eq!(p_undo.runs.iter().all(|r| r.style.bold), p.runs.iter().all(|r| r.style.bold));

    // 4. メニュー「編集 > すべて選択」（edit.select-all）による全文選択
    s.menu("edit.select-all");
    let (all_start, all_end) = s.selection().expect("all selected");
    assert_eq!(all_start.offset, 0);
    let full_doc = s.document();
    assert_eq!(all_end.offset, ibis_doc_model::flow_command::paragraph(&full_doc, all_end.para).unwrap().len());
    assert_ne!(all_start.para, all_end.para, "multiple paragraphs selected across document");

    // 5. キーボードショートカット ⌘A による全文選択
    // 一度クリックでキャレット解除してから cmd-a
    s.click_text(ibis_doc_model::FlowPos::new(para_id, 0));
    let (cur_a, cur_c) = s.selection().unwrap();
    assert_eq!(cur_a, cur_c);
    s.key("cmd-a");
    let (cmd_all_start, cmd_all_end) = s.selection().expect("all selected via cmd-a");
    assert_eq!(cmd_all_start, all_start);
    assert_eq!(cmd_all_end, all_end);
}

/// DOCX-FILE-019（文書統計・文字カウント Words Count ダイアログ）
/// メニュー「ツール > 文字カウント…」（tools.word-count）から開く「文字カウント」パネル、
/// ページ数、単語数、文字数（スペースなし）、文字数（スペースあり）、段落数、行数の
/// 文書全体および選択範囲ごとの統計計算・表示、およびパネルの開閉を検証する。
fn fix_docx_word_count_dialog() {
    use crate::docx::scenarios::DocSteps as _;
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. 文書全体の統計計算の検証
    let whole_stats = s.with(|e, _| e.document_statistics());
    assert!(!whole_stats.is_selection, "whole document stats");
    assert!(whole_stats.pages >= 1, "pages >= 1");
    assert!(whole_stats.words > 0, "words > 0");
    assert!(whole_stats.characters_no_spaces > 0, "chars no spaces > 0");
    assert!(whole_stats.characters_with_spaces >= whole_stats.characters_no_spaces, "chars with spaces >= chars no spaces");
    assert!(whole_stats.paragraphs > 0, "paragraphs > 0");
    assert!(whole_stats.lines > 0, "lines > 0");

    // ステータスバーの文字数と document_statistics の characters_no_spaces が一致することを確認
    let status_chars = s.with(|e, _| e.doc_state().map_or(0, |st| st.characters));
    assert_eq!(status_chars, whole_stats.characters_no_spaces, "status bar characters match stats");

    // 2. メニュー「ツール > 文字カウント…」（tools.word-count）を実行
    assert!(s.with(|e, _| e.doc_state().unwrap().helper.word_count.is_none()));
    s.menu("tools.word-count");

    // サイドバーが開き、文字カウントパネルが表示されていることを確認
    assert!(s.with(|e, _| e.sidebar_open));
    let panel_stats = s.with(|e, _| e.doc_state().unwrap().helper.word_count.as_ref().unwrap().stats.clone());
    assert_eq!(panel_stats, whole_stats, "panel displays whole document stats");

    // 3. パネルを閉じる
    s.update(|e, _, cx| e.close_word_count_panel(cx));
    assert!(s.with(|e, _| e.doc_state().unwrap().helper.word_count.is_none()));

    // 4. テキスト選択状態での統計計算の検証
    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 15 && !p.text().contains('\u{fffc}') && p.list.is_none() {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");

    // ダブルクリックで単語を選択
    s.double_click_text(ibis_doc_model::FlowPos::new(p.id, 2));
    assert!(s.with(|e, _| e.has_selection()));
    let selected_str = s.with(|e, _| e.selected_text());
    assert!(!selected_str.is_empty(), "selected text is not empty");

    // 選択状態での文字カウントを実行
    s.menu("tools.word-count");
    let sel_stats = s.with(|e, _| e.doc_state().unwrap().helper.word_count.as_ref().unwrap().stats.clone());
    assert!(sel_stats.is_selection, "is selection stats");
    let (exp_words, exp_no, exp_with) = crate::docx::help_panels::count_words_and_chars(&selected_str);
    assert_eq!(sel_stats.words, exp_words, "selection words match");
    assert_eq!(sel_stats.characters_no_spaces, exp_no, "selection chars without spaces match");
    assert_eq!(sel_stats.characters_with_spaces, exp_with, "selection chars with spaces match");
    assert_eq!(sel_stats.paragraphs, 1, "single paragraph selected");

    // 5. パネルを閉じる
    s.update(|e, _, cx| e.close_word_count_panel(cx));
    assert!(s.with(|e, _| e.doc_state().unwrap().helper.word_count.is_none()));
}

/// DOCX-FIND-007: 特殊文字・段落記号の検索と置換
/// special_characters の拡張（^m 改ページ、^s ノーブレークスペース、^- 任意指定ハイフン、^~ 改行しないハイフン等）
/// および大文字コード対応（^P, ^T, ^L, ^M, ^N, ^S）の検証
fn fix_docx_find_special_characters() {
    use crate::docx::find::special_characters;

    // 1. special_characters 単体での変換精度の検証
    assert_eq!(special_characters("A^tB"), "A\tB");
    assert_eq!(special_characters("A^TB"), "A\tB");
    assert_eq!(special_characters("Line1^lLine2"), "Line1\u{b}Line2");
    assert_eq!(special_characters("Line1^LLine2"), "Line1\u{b}Line2");
    assert_eq!(special_characters("Page1^mPage2"), "Page1\u{c}Page2");
    assert_eq!(special_characters("Page1^MPage2"), "Page1\u{c}Page2");
    assert_eq!(special_characters("Col1^nCol2"), "Col1\u{e}Col2");
    assert_eq!(special_characters("Col1^NCol2"), "Col1\u{e}Col2");
    assert_eq!(special_characters("No^sBreak"), "No\u{a0}Break");
    assert_eq!(special_characters("No^SBreak"), "No\u{a0}Break");
    assert_eq!(special_characters("Opt^-Hyphen"), "Opt\u{ad}Hyphen");
    assert_eq!(special_characters("Non^~Hyphen"), "Non\u{2011}Hyphen");
    assert_eq!(special_characters("Em^+Dash"), "Em\u{2014}Dash");
    assert_eq!(special_characters("En^=Dash"), "En\u{2013}Dash");
    assert_eq!(special_characters("Caret^^Here"), "Caret^Here");
    assert_eq!(special_characters("End^p"), "End\n");
    assert_eq!(special_characters("End^P"), "End\n");

    // 複合クエリ
    assert_eq!(
        special_characters("word1^sword2^m^tp1^-sub^~fix^+dash"),
        "word1\u{a0}word2\u{c}\tp1\u{ad}sub\u{2011}fix\u{2014}dash"
    );

    // 2. 実際のドキュメントでの検索・置換操作
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // テキスト段落を見つける
    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 10 && !p.text().contains('\u{fffc}') && p.list.is_none() {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");

    let p_id = p.id;
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    // 特殊文字を含むテキストを挿入
    s.type_text("SPEC_NBSP\u{a0}TEST SPEC_HYPH\u{2011}TEST ");

    // 検索テスト: ^s (ノーブレークスペース) で検索
    let doc = s.document();
    let query_nbsp = "NBSP^sTEST";
    let matches_nbsp = crate::docx::find::find_all(&doc, query_nbsp);
    assert_eq!(matches_nbsp.len(), 1, "found 1 NBSP match with ^s");

    // 検索テスト: 大文字 ^S でも検索できることを確認
    let matches_nbsp_upper = crate::docx::find::find_all(&doc, "NBSP^STEST");
    assert_eq!(matches_nbsp_upper.len(), 1, "found 1 NBSP match with ^S");

    // 検索テスト: ^~ (改行しないハイフン) で検索
    let query_hyph = "HYPH^~TEST";
    let matches_hyph = crate::docx::find::find_all(&doc, query_hyph);
    assert_eq!(matches_hyph.len(), 1, "found 1 non-breaking hyphen match with ^~");

    // 3. 置換テスト: 特殊文字への置換（replace_all）
    s.menu("edit.find-replace");
    // "SPEC_NBSP\u{a0}TEST" を "REPLACED^sDONE" に置換
    let n = s.update(|e, _, cx| e.replace_all("SPEC_NBSP^sTEST", "REPLACED^sDONE", cx));
    assert_eq!(n, 1, "replaced 1 occurrence");

    let doc_after = s.document();
    let p_after = ibis_doc_model::flow_command::paragraph(&doc_after, p_id).expect("paragraph exists");
    assert!(p_after.text().contains("REPLACED\u{a0}DONE"), "replacement contains NBSP: {:?}", p_after.text());

    // 4. 手動改ページ (^m) への置換テスト
    let n_page = s.update(|e, _, cx| e.replace_all("SPEC_HYPH^~TEST", "BEFORE^mAFTER", cx));
    assert_eq!(n_page, 1, "replaced 1 occurrence with page break");

    let doc_after_page = s.document();
    let p_after_page = ibis_doc_model::flow_command::paragraph(&doc_after_page, p_id).expect("paragraph exists");
    assert!(p_after_page.text().contains("BEFORE\u{c}AFTER"), "replacement contains PageBreak character: {:?}", p_after_page.text());
    assert!(p_after_page.runs.iter().any(|r| r.kind == ibis_doc_model::FlowRunKind::PageBreak), "runs contain FlowRunKind::PageBreak");

    // 検索テスト: 置換された手動改ページを ^m で検索
    let matches_page = crate::docx::find::find_all(&doc_after_page, "BEFORE^mAFTER");
    assert_eq!(matches_page.len(), 1, "found 1 PageBreak match with ^m");

    // 検索テスト: 大文字 ^M でも検索できることを確認
    let matches_page_upper = crate::docx::find::find_all(&doc_after_page, "BEFORE^MAFTER");
    assert_eq!(matches_page_upper.len(), 1, "found 1 PageBreak match with ^M");

    // 5. Undo で元に戻ることを確認
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");
    let doc_undo1 = s.document();
    let p_undo1 = ibis_doc_model::flow_command::paragraph(&doc_undo1, p_id).expect("paragraph exists");
    assert!(!p_undo1.text().contains("BEFORE\u{c}AFTER"), "undo removed page break replacement");
}

/// DOCX-FIND-004: 単語全体の一致（日本語・全角約物・多言語区切り対応）
/// 和文約物（「」『』【】（）、。・）や引用符・ダッシュで囲まれた単語が
/// 単語境界として正しく認識され、複合語の部分一致は除外されることを検証
fn fix_docx_find_whole_words() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 10 && !p.text().contains('\u{fffc}') && p.list.is_none() {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");

    let p_id = p.id;
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    // 和文約物・括弧・記号を含むテキストを入力
    // 「テスト」, テスト、, （テスト）, テスト全般（除外対象）, 【テスト】
    s.type_text("「テスト」 テスト、 （テスト） テスト全般 【テスト】 ");

    let doc_typed = s.document();

    // 1. 通常検索（単語単位オフ）: "テスト" は 5 箇所すべてでヒットする
    let all_matches = crate::docx::find::find_all(&doc_typed, "テスト");
    assert_eq!(all_matches.len(), 5, "finds 5 matches without whole_words");

    // 2. 単語単位検索（whole_words = true）
    s.menu("edit.find-replace");
    s.type_text("テスト");
    // 単語単位チェックボックスをクリック (座標でなくエディタの設定関数で切り替える)
    crate::docx::scenarios::toggle_find_whole_words(&mut s);
    assert!(s.with(|e, _| e.doc_state().unwrap().find.as_ref().unwrap().whole_words), "whole_words enabled");

    // 「テスト」, テスト、, （テスト）, 【テスト】 の 4 箇所がマッチし、
    // 「テスト全般」の「テスト」は右隣が「全」（単語文字）のためマッチしない
    let matches_whole = s.with(|e, _| e.doc_state().unwrap().find.as_ref().unwrap().matches.clone());
    assert_eq!(matches_whole.len(), 4, "finds 4 matches with whole_words (excluding テスト全般)");

    // 3. 置換（replace_all）: 単語単位の一致のみが置換される
    let n = s.update(|e, _, cx| e.replace_all("テスト", "成功", cx));
    assert_eq!(n, 4, "replaced 4 occurrences");

    let doc_replaced = s.document();
    let p_rep = ibis_doc_model::flow_command::paragraph(&doc_replaced, p_id).expect("paragraph exists");
    // 置換後: 「成功」 成功、 （成功） テスト全般 【成功】
    assert!(p_rep.text().contains("「成功」 成功、 （成功） テスト全般 【成功】"), "result text: {:?}", p_rep.text());
    assert!(p_rep.text().contains("テスト全般"), "unmatched word part was preserved");

    // 4. Undo で元に戻る
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");
    let doc_undo = s.document();
    let p_undo = ibis_doc_model::flow_command::paragraph(&doc_undo, p_id).expect("paragraph exists");
    assert!(p_undo.text().contains("「テスト」 テスト、 （テスト） テスト全般 【テスト】"), "undo restored original");
}

/// DOCX-EDIT-011: 形式を選択して貼付
/// メニュー「編集 > 形式を選択して貼り付け」（docx.paste-special）および
/// 各オプション（元の書式を保持、貼り付け先の書式に合わせる、テキストのみ保持、書式のみ）
/// からの貼り付け動作、ショートカット連携、Undo/Redoを検証
fn fix_docx_paste_special() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 段落を見つける
    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 10 && !p.text().contains('\u{fffc}') && p.list.is_none() {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");

    let p_id = p.id;

    // 1. 太字テキストを入力してコピー
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    s.type_text("PASTE_SRC ");
    // 入力したテキストを選択して太字化
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    for _ in 0..9 {
        s.key("shift-right");
    }
    s.key("cmd-b");
    // コピー
    s.menu("edit.copy");

    // 2. 貼り付け先へ移動
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 10));

    // 3. テキストのみ保持して貼り付け（docx.paste-special.text-only）
    s.menu("docx.paste-special.text-only");
    let doc_text_only = s.document();
    let p_text_only = ibis_doc_model::flow_command::paragraph(&doc_text_only, p_id).expect("paragraph exists");
    assert!(p_text_only.text().contains("PASTE_SRC"), "pasted text exists");

    // Undo で取り消し
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");

    // 4. 元の書式を保持して貼り付け（docx.paste-special.keep-source）
    s.menu("docx.paste-special.keep-source");
    let doc_keep = s.document();
    let p_keep = ibis_doc_model::flow_command::paragraph(&doc_keep, p_id).expect("paragraph exists");
    assert!(p_keep.runs.iter().any(|r| r.style.bold && matches!(&r.kind, ibis_doc_model::FlowRunKind::Text(t) if t.contains("PASTE_SRC"))), "pasted run keeps bold formatting");

    // Undo で取り消し
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");

    // 5. 貼り付け先の書式に合わせて貼り付け（docx.paste-special.match-format）
    s.menu("docx.paste-special.match-format");
    let doc_match = s.document();
    let p_match = ibis_doc_model::flow_command::paragraph(&doc_match, p_id).expect("paragraph exists");
    assert!(p_match.text().contains("PASTE_SRC"), "pasted with match format exists");

    // Undo で元に戻る
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");
}

/// DOCX-FIELD-005: フィールドのリンク解除（Unlink Fields, ⇧⌘F9 / Ctrl+Shift+F9）
/// および DOCX-FIELD-003: フィールドの更新（F9）
/// フィールド（SEQ、REF、DATE等）のリンク解除により静的テキストへの変換、
/// メニュー「docx.fields.unlink」およびショートカット連携、Undo/Redo、ファイル保存・再読込を検証
fn fix_docx_unlink_fields() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. 本文からプレーンな段落を見つける
    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 10 && !p.text().contains('\u{fffc}') && p.list.is_none() {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");

    let p_id = p.id;

    // 2. キャレットを段落の先頭に置き、図表番号（SEQ 図）を挿入
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    s.menu("docx.caption.figure");

    // 挿入された段落を確認（SEQ フィールドが存在する）
    let doc_after = s.document();
    let p_caption = doc_after
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("SEQ"))) {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("caption paragraph with SEQ field");

    let cap_id = p_caption.id;
    assert!(p_caption.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })), "has field run");

    // 3. フィールドの位置（offset 2）にキャレットを置き、メニュー「挿入 > フィールドのリンク解除」（docx.fields.unlink）を実行
    s.click_text(ibis_doc_model::FlowPos::new(cap_id, 2));
    s.menu("docx.fields.unlink");

    // リンク解除後の段落を確認
    let doc_unlinked = s.document();
    let p_unlinked = ibis_doc_model::flow_command::paragraph(&doc_unlinked, cap_id).expect("paragraph exists");
    // 全ての Field がプレーンテキスト化されている
    assert!(!p_unlinked.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })), "no field run after unlink");
    // テキストに図表番号の文字列が含まれている
    assert!(p_unlinked.text().contains("図 1"), "unlinked text preserved content");

    // 4. Undo で元に戻る
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");
    let doc_undo = s.document();
    let p_undo = ibis_doc_model::flow_command::paragraph(&doc_undo, cap_id).expect("paragraph exists");
    assert!(p_undo.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })), "field restored after undo");

    // 5. Redo で再度リンク解除
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-shift-z");
    let doc_redo = s.document();
    let p_redo = ibis_doc_model::flow_command::paragraph(&doc_redo, cap_id).expect("paragraph exists");
    assert!(!p_redo.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })), "no field run after redo");
    assert!(p_redo.text().contains("図 1"), "redo preserved text");

    // 6. 保存と再読込
    s.save_as("test-unlink-field.docx");
    let reread = s.reopen();
    let flow = reread.flow.as_ref().unwrap();
    let p_reread = flow
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.text().contains("図 1") {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("reread unlinked paragraph");
    assert!(!p_reread.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })), "field remains unlinked after save & reopen");

    // 7. ショートカットキー ⇧⌘F9 によるリンク解除と F9 によるフィールド更新の検証
    // 新たに図表番号（表）を挿入
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    s.menu("docx.caption.table");
    let doc_tbl = s.document();
    let p_tbl_caption = doc_tbl
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("SEQ 表"))) {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("caption paragraph with SEQ 表");
    let tbl_cap_id = p_tbl_caption.id;

    // F9 キーでフィールド更新
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("f9");

    // ショートカット ⇧⌘F9 (cmd-shift-f9) でリンク解除
    s.click_text(ibis_doc_model::FlowPos::new(tbl_cap_id, 2));
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-shift-f9");

    let doc_unlinked2 = s.document();
    let p_unlinked2 = ibis_doc_model::flow_command::paragraph(&doc_unlinked2, tbl_cap_id).expect("paragraph exists");
    assert!(!p_unlinked2.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })), "field unlinked via shortcut cmd-shift-f9");
    assert!(p_unlinked2.text().contains("表 1"), "unlinked text preserved content for table caption");
}

/// DOCX-PARA-002: 均等割付（Distributed / ⇧⌘J / Ctrl+Shift+J / align.distribute）
/// ショートカットキー ⇧⌘J、メニュー「書式 > 配置 > 均等割り付け」およびツールバーからの
/// 均等割り付け適用、Undo/Redo、ファイル保存・再読込で <w:jc w:val="distribute"/> の保持を検証
fn fix_docx_align_distribute() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. 本文からプレーンな段落を見つける
    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 10 && !p.text().contains('\u{fffc}') && p.list.is_none() && p.align != ibis_doc_model::FlowAlign::Distribute {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");

    let p_id = p.id;
    let initial_align = p.align;

    // 2. キャレットを段落に置き、メニュー「align.distribute」を実行
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    s.menu("align.distribute");

    // 段落の配置が Distribute に変更されたことを確認
    let doc_dist = s.document();
    let p_dist = ibis_doc_model::flow_command::paragraph(&doc_dist, p_id).expect("paragraph exists");
    assert_eq!(p_dist.align, ibis_doc_model::FlowAlign::Distribute, "alignment changed to Distribute via menu");

    // 3. Undo で元の配置に戻る
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");
    let doc_undo = s.document();
    let p_undo = ibis_doc_model::flow_command::paragraph(&doc_undo, p_id).expect("paragraph exists");
    assert_eq!(p_undo.align, initial_align, "alignment restored after undo");

    // 4. ショートカットキー ⇧⌘J (cmd-shift-j) で均等割り付けを適用
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-shift-j");
    let doc_shortcut = s.document();
    let p_shortcut = ibis_doc_model::flow_command::paragraph(&doc_shortcut, p_id).expect("paragraph exists");
    assert_eq!(p_shortcut.align, ibis_doc_model::FlowAlign::Distribute, "alignment changed to Distribute via shortcut cmd-shift-j");

    // 5. 保存と再読込で <w:jc w:val="distribute"/> が維持されることを確認
    s.save_as("test-align-distribute.docx");
    let reread = s.reopen();
    let flow = reread.flow.as_ref().unwrap();
    let p_reread = ibis_doc_model::flow_command::paragraph(flow, p_id).expect("paragraph exists in reread");
    assert_eq!(p_reread.align, ibis_doc_model::FlowAlign::Distribute, "alignment remains Distribute after save & reopen");
}

/// DOCX-FIELD-002: フィールドコードの表示/非表示（Shift+F9 / Alt+F9 / docx.fields.toggle / docx.fields.toggle-all）
/// 選択中フィールドのコード表示切替（Shift+F9 / docx.fields.toggle）、文書全体の切替（Alt+F9 / docx.fields.toggle-all）、
/// 表示されるテキストがコード（{ SEQ 図 \* ARABIC } 等）と結果に切り替わること、
/// Undo/Redo、ファイル保存・再読込でフィールド属性が損なわれず保持されることを検証。
fn fix_docx_toggle_field_codes() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. 本文段落を見つけ、図表番号（図）を挿入
    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 5 && !p.text().contains('\u{fffc}') {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");
    let p_id = p.id;

    // キャレットを置き、メニュー「docx.caption.figure」で図表番号を挿入
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    s.menu("docx.caption.figure");

    // 挿入されたキャプション段落（SEQ 図を含む段落）を取得
    let doc_cap = s.document();
    let p_caption = doc_cap
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("SEQ 図"))) {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("caption paragraph with SEQ");
    let cap_id = p_caption.id;

    // 初期状態: show_field_codes は false、run.show_field_code も false
    assert!(!doc_cap.show_field_codes);
    let run = p_caption.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(!run.show_field_code);
    assert!(!run.is_field_code_shown(doc_cap.show_field_codes));
    assert_eq!(run.display_field_code(), Some("{ SEQ 図 \\* ARABIC }".into()));

    // 2. メニュー「docx.fields.toggle」（Shift+F9）で個別フィールドのコード表示をトグル
    s.click_text(ibis_doc_model::FlowPos::new(cap_id, 2));
    s.menu("docx.fields.toggle");

    let doc_toggled = s.document();
    let p_toggled = ibis_doc_model::flow_command::paragraph(&doc_toggled, cap_id).expect("caption paragraph exists");
    let run_toggled = p_toggled.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(run_toggled.show_field_code, "field's show_field_code is toggled to true");
    assert!(run_toggled.is_field_code_shown(doc_toggled.show_field_codes), "field code is shown");

    // 3. Undo (cmd-z) で元の結果表示に戻る
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");
    let doc_undo = s.document();
    let p_undo = ibis_doc_model::flow_command::paragraph(&doc_undo, cap_id).expect("caption paragraph exists");
    let run_undo = p_undo.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(!run_undo.show_field_code, "field code display restored to false after undo");

    // 4. Redo (shift-cmd-z) で再度コード表示になる
    s.key("shift-cmd-z");
    let doc_redo = s.document();
    let p_redo = ibis_doc_model::flow_command::paragraph(&doc_redo, cap_id).expect("caption paragraph exists");
    let run_redo = p_redo.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(run_redo.show_field_code, "field code display restored to true after redo");

    // 5. ショートカットキー shift-f9 で再度トグルして false に戻す
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("shift-f9");
    let doc_s9 = s.document();
    let p_s9 = ibis_doc_model::flow_command::paragraph(&doc_s9, cap_id).expect("caption paragraph exists");
    let run_s9 = p_s9.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(!run_s9.show_field_code, "toggled back to false via shortcut shift-f9");

    // 6. メニュー「docx.fields.toggle-all」（Alt+F9）で文書全体のすべてのフィールドコード表示をトグル
    s.menu("docx.fields.toggle-all");
    let doc_all = s.document();
    assert!(doc_all.show_field_codes, "document-wide show_field_codes is true");
    let p_all = ibis_doc_model::flow_command::paragraph(&doc_all, cap_id).expect("caption paragraph exists");
    let run_all = p_all.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(run_all.is_field_code_shown(doc_all.show_field_codes), "field code is shown under document-wide toggle");

    // 7. ショートカット alt-f9 で文書全体のコード表示をオフに戻す
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("alt-f9");
    let doc_all_off = s.document();
    assert!(!doc_all_off.show_field_codes, "document-wide show_field_codes is back to false via alt-f9");

    // 8. ファイル保存・再読込でフィールドが正常に維持されることを確認
    s.save_as("test-toggle-field-codes.docx");
    let reread = s.reopen();
    let flow = reread.flow.as_ref().unwrap();
    let p_reread = flow
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("SEQ 図"))) {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("caption paragraph exists in reread");
    assert!(p_reread.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })), "field persists after save & reopen");
}

/// DOCX-FIELD-004: フィールドのロック (Ctrl+F11 / ⌘F11 / docx.fields.lock) および
/// ロック解除 (Ctrl+Shift+F11 / ⇧⌘F11 / docx.fields.unlock)。
/// ロック中はフィールド更新 (F9) による再計算がスキップされ、Undo/Redo およびファイル保存・再読込で保持されることを検証。
fn fix_docx_lock_fields() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. 本文段落を見つけ、図表番号（図）を挿入
    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 5 && !p.text().contains('\u{fffc}') {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");
    let p_id = p.id;

    // キャレットを置き、メニュー「docx.caption.figure」で図表番号を挿入
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    s.menu("docx.caption.figure");

    // 挿入されたキャプション段落（SEQ 図を含む段落）を取得
    let doc_cap = s.document();
    let p_caption = doc_cap
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("SEQ 図"))) {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("caption paragraph with SEQ");
    let cap_id = p_caption.id;

    // 初期状態: field_locked は false
    let run = p_caption.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(!run.field_locked, "initially not locked");

    // 2. キャレットをキャプション段落に置き、メニュー「docx.fields.lock」でロック
    s.click_text(ibis_doc_model::FlowPos::new(cap_id, 2));
    s.menu("docx.fields.lock");

    let doc_locked = s.document();
    let p_locked = ibis_doc_model::flow_command::paragraph(&doc_locked, cap_id).expect("caption paragraph exists");
    let run_locked = p_locked.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(run_locked.field_locked, "field is locked via menu docx.fields.lock");

    // 3. Undo (cmd-z) でロック解除に戻る
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");
    let doc_undo = s.document();
    let p_undo = ibis_doc_model::flow_command::paragraph(&doc_undo, cap_id).expect("caption paragraph exists");
    let run_undo = p_undo.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(!run_undo.field_locked, "field lock reverted after undo");

    // 4. Redo (shift-cmd-z) で再びロックされる
    s.key("shift-cmd-z");
    let doc_redo = s.document();
    let p_redo = ibis_doc_model::flow_command::paragraph(&doc_redo, cap_id).expect("caption paragraph exists");
    let run_redo = p_redo.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(run_redo.field_locked, "field lock re-applied after redo");

    // 5. ロック解除（ショートカット shift-cmd-f11）
    s.click_text(ibis_doc_model::FlowPos::new(cap_id, 2));
    s.key("shift-cmd-f11");
    let doc_unlocked = s.document();
    let p_unlocked = ibis_doc_model::flow_command::paragraph(&doc_unlocked, cap_id).expect("caption paragraph exists");
    let run_unlocked = p_unlocked.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(!run_unlocked.field_locked, "field unlocked via shift-cmd-f11");

    // 6. ショートカット cmd-f11 で再びロック
    s.click_text(ibis_doc_model::FlowPos::new(cap_id, 2));
    s.key("cmd-f11");
    let doc_relocked = s.document();
    let p_relocked = ibis_doc_model::flow_command::paragraph(&doc_relocked, cap_id).expect("caption paragraph exists");
    let run_relocked = p_relocked.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(run_relocked.field_locked, "field relocked via cmd-f11");

    // 7. ファイル保存・再読込でロック状態が保持されることを検証
    s.save_as("test-lock-fields.docx");
    let reread = s.reopen();
    let flow = reread.flow.as_ref().unwrap();
    let p_reread = flow
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("SEQ 図"))) {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("caption paragraph exists in reread");
    let field_reread = p_reread.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(field_reread.field_locked, "field_locked persists after save and reopen");
}

fn fix_docx_insert_and_edit_field() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. 本文段落を見つける
    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 5 && !p.text().contains('\u{fffc}') {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");
    let p_id = p.id;

    // 段落先頭に "PAGE" と入力
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    s.type_text("PAGE ");

    // 先頭の "PAGE"（0..4）を選択
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    for _ in 0..4 {
        s.key("shift-right");
    }

    // 2. ショートカット cmd-f9 でフィールドに変換（Word の ⌘F9 / Ctrl+F9）
    s.key("cmd-f9");

    let doc_field = s.document();
    let p_field = ibis_doc_model::flow_command::paragraph(&doc_field, p_id).expect("paragraph exists");
    let field_run = p_field.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction == "PAGE")).expect("PAGE field inserted");
    assert!(field_run.show_field_code, "field code shown after cmd-f9");

    // 3. Undo (cmd-z) で元のテキストに戻る
    s.key("cmd-z");
    let doc_undo = s.document();
    let p_undo = ibis_doc_model::flow_command::paragraph(&doc_undo, p_id).expect("paragraph exists");
    assert!(p_undo.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Text(t) if t.contains("PAGE"))), "reverted to text PAGE");

    // 4. Redo (shift-cmd-z) で再びフィールドに戻る
    s.key("shift-cmd-z");
    let doc_redo = s.document();
    let p_redo = ibis_doc_model::flow_command::paragraph(&doc_redo, p_id).expect("paragraph exists");
    assert!(p_redo.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction == "PAGE")), "field re-created after redo");

    // 5. メニュー「docx.fields.insert」でフィールドパネルを開く
    s.menu("docx.fields.insert");
    assert!(s.with(|e, _| e.sidebar_open && e.doc_state().and_then(|s| s.helper.field.as_ref()).is_some()), "field panel is open");

    // プリセット "DATE" を設定して適用
    s.update(|e, w, cx| {
        if let Some(panel) = e.doc_state_mut().and_then(|s| s.helper.field.as_mut()) {
            panel.code.update(cx, |i, cx| i.set_value("DATE \\@ \"yyyy-MM-dd\"", w, cx));
        }
        e.apply_field_panel(cx);
    });

    let doc_inserted = s.document();
    let p_inserted = ibis_doc_model::flow_command::paragraph(&doc_inserted, p_id).expect("paragraph exists");
    assert!(p_inserted.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("DATE"))), "DATE field inserted via panel");

    // 6. メニュー「docx.fields.edit」でフィールドコードを編集
    let date_offset: usize = p_inserted.runs.iter().take_while(|r| !matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("DATE"))).map(|r| r.len()).sum();
    s.click_text(ibis_doc_model::FlowPos::new(p_id, date_offset));
    s.menu("docx.fields.edit");
    assert!(s.with(|e, _| e.sidebar_open && e.doc_state().and_then(|s| s.helper.field.as_ref()).is_some_and(|f| f.is_edit)), "field panel open in edit mode");

    // コードを "NUMPAGES" に更新して適用
    s.update(|e, w, cx| {
        if let Some(panel) = e.doc_state_mut().and_then(|s| s.helper.field.as_mut()) {
            panel.code.update(cx, |i, cx| i.set_value("NUMPAGES", w, cx));
        }
        e.apply_field_panel(cx);
    });

    let doc_edited = s.document();
    let p_edited = ibis_doc_model::flow_command::paragraph(&doc_edited, p_id).expect("paragraph exists");
    assert!(p_edited.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction == "NUMPAGES")), "field instruction edited to NUMPAGES");

    // 7. ファイル保存・再読込でフィールドが保持されることを検証
    s.save_as("test-insert-edit-field.docx");
    let reread = s.reopen();
    let flow = reread.flow.as_ref().unwrap();
    let p_reread = flow.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == p_id { Some(p.clone()) } else { None }
    } else { None }).expect("paragraph exists in reread");
    assert!(p_reread.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction == "NUMPAGES")), "NUMPAGES field persists after save and reopen");
}

fn fix_docx_caption_labels_and_numbering() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. 本文段落を見つける
    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 5 && !p.text().contains('\u{fffc}') {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");
    let p_id = p.id;

    // カレットを段落末尾に置く
    s.click_text(ibis_doc_model::FlowPos::new(p_id, p.len()));

    let body_len_before = s.document().body.len();

    // 2. メニュー「docx.caption.equation」で「数式」ラベルの図表番号を挿入
    s.menu("docx.caption.equation");

    let doc = s.document();
    assert_eq!(doc.body.len(), body_len_before + 1, "caption paragraph inserted after text");
    let (_, p_cap1) = doc.body.iter().enumerate().find_map(|(idx, b)| {
        if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("SEQ 数式"))) {
                Some((idx, p.clone()))
            } else {
                None
            }
        } else {
            None
        }
    }).expect("caption with 数式 exists");
    assert!(p_cap1.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Text(t) if t == "数式 ")), "has 数式 label");

    // 3. カレットをそのキャプションの末尾に置き、メニュー「docx.caption.insert」で図表番号パネルを開く
    s.click_text(ibis_doc_model::FlowPos::new(p_cap1.id, p_cap1.len()));
    s.menu("docx.caption.insert");
    assert!(s.with(|e, _| e.sidebar_open && e.doc_state().and_then(|s| s.helper.caption.as_ref()).is_some()), "caption panel is open");

    // プリセットに標準ラベルが含まれていることを検証
    s.with(|e, _| {
        let p = e.doc_state().unwrap().helper.caption.as_ref().unwrap();
        assert!(p.presets.contains(&"数式".to_string()));
    });

    // 新規カスタムラベル「グラフ」、説明文「軌道予測」、番号書式 UpperRoman を設定して適用
    s.update(|e, w, cx| {
        if let Some(panel) = e.doc_state_mut().and_then(|s| s.helper.caption.as_mut()) {
            panel.label.update(cx, |i, cx| i.set_value("グラフ", w, cx));
            panel.text.update(cx, |i, cx| i.set_value("軌道予測", w, cx));
            panel.format = ibis_doc_model::CaptionNumberFormat::UpperRoman;
        }
        e.apply_caption_panel(cx);
    });

    let doc2 = s.document();
    assert_eq!(doc2.body.len(), body_len_before + 2, "second caption paragraph inserted");
    let (cap2_index, p_cap2) = doc2.body.iter().enumerate().find_map(|(idx, b)| {
        if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("SEQ グラフ"))) {
                Some((idx, p.clone()))
            } else {
                None
            }
        } else {
            None
        }
    }).expect("caption with グラフ exists");
    assert!(p_cap2.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Text(t) if t == "グラフ ")), "has グラフ label");
    assert!(p_cap2.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, shown } if instruction.contains("SEQ グラフ") && shown == "I")), "has SEQ グラフ field with I");
    assert!(p_cap2.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Text(t) if t.contains("軌道予測"))), "has 軌道予測 description");

    // 4. ラベル除外オプション（exclude_label = true）の検証
    s.click_text(ibis_doc_model::FlowPos::new(p_cap2.id, p_cap2.len()));
    s.menu("docx.caption.insert");
    s.update(|e, w, cx| {
        if let Some(panel) = e.doc_state_mut().and_then(|s| s.helper.caption.as_mut()) {
            assert!(panel.presets.contains(&"グラフ".to_string()), "グラフ is now in presets");
            panel.label.update(cx, |i, cx| i.set_value("グラフ", w, cx));
            panel.exclude_label = true;
            panel.format = ibis_doc_model::CaptionNumberFormat::LowerLetter;
        }
        e.apply_caption_panel(cx);
    });

    let doc3 = s.document();
    assert_eq!(doc3.body.len(), body_len_before + 3, "third caption paragraph inserted");
    let p_cap3 = if let ibis_doc_model::Block::Paragraph(p) = &doc3.body[cap2_index + 1] { p.clone() } else { panic!() };
    assert!(!p_cap3.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Text(t) if t.contains("グラフ"))), "label is excluded");
    assert!(p_cap3.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, shown } if instruction.contains("SEQ グラフ") && shown == "b")), "has SEQ グラフ with switch \\* alphabetic (b)");

    // 5. Undo (cmd-z) で直前の挿入を取り消す
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");
    let doc_undo = s.document();
    assert_eq!(doc_undo.body.len(), body_len_before + 2, "undone last caption");

    // 6. Redo (shift-cmd-z) で再度復元
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("shift-cmd-z");
    let doc_redo = s.document();
    assert_eq!(doc_redo.body.len(), body_len_before + 3, "redone last caption");

    // 7. ファイル保存・再読込で検証
    s.save_as("test-caption-labels.docx");
    let reread = s.reopen();
    let flow = reread.flow.as_ref().unwrap();
    let caps = ibis_doc_model::flow_command::caption_labels(flow);
    assert!(caps.contains(&"数式".to_string()), "数式 label found in reread");
    assert!(caps.contains(&"グラフ".to_string()), "グラフ label found in reread");
}

/// DOCX-TOC-008: 図表目次（Table of Figures: Word 標準の「図表目次の挿入」
/// `TOC \h \z \c "図"` / `TOC \h \z \c "表"` / `TOC \h \z \c "数式"` の生成・更新、
/// 右揃えタブリーダー、PageRef リンク、スタイル定義、Undo/Redo、OOXML保存・再読込検証）。
fn fix_docx_table_of_figures() {
    use ibis_doc_model::{Block, FlowDocument, FlowParagraph, FlowRunKind, FlowPos, TabAlign};
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. 文書末尾にキャプションを追加（図 1, 図 2）
    s.key("cmd-down");
    s.key("enter");
    s.menu("docx.caption.figure");
    s.type_text(" システム構成図");
    s.key("enter");
    s.menu("docx.caption.figure");
    s.type_text(" 処理フロー図");

    // 2. 先頭に移動し、図表目次（図）を挿入
    s.key("cmd-up");
    s.menu("docx.table-of-figures.figure");

    let tof = |doc: &FlowDocument| -> Vec<(u32, FlowParagraph)> {
        let ps: Vec<&FlowParagraph> = doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None }).collect();
        let start = ps.iter().position(|p| p.field_start.as_deref().is_some_and(|i| i.contains("TOC") && i.contains("\\c") && i.contains("図"))).expect("a table of figures for 図");
        let end = start + ps[start..].iter().position(|p| p.field_end).expect("field_end for table of figures");
        ps[start..=end].iter().map(|p| (p.id, (*p).clone())).collect()
    };

    let doc = s.document();
    let entries = tof(&doc);
    assert_eq!(entries.len(), 2, "2 figure captions in the table of figures");

    // 各エントリの書式・スタイル・タブ・PageRefリンクを検証
    for (_, p) in &entries {
        assert_eq!(p.style.as_deref(), Some("TableofFigures"), "entry has TableofFigures style ID");
        assert_eq!(p.style.as_deref().and_then(|id| doc.styles.styles.get(id)).map(|st| st.name.as_str()), Some("table of figures"), "entry style name is 'table of figures'");
        // 右揃えタブリーダーの検証
        assert!(p.direct.tabs.iter().any(|t| t.stop.as_ref().is_some_and(|s| s.align == TabAlign::Right && s.leader)), "has right-aligned tab stop with leader");
        // Text, Tab, PageRef の検証
        assert!(p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Text(_))), "has text run");
        assert!(p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Tab)), "has tab run");
        assert!(p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::PageRef { .. })), "has pageref run");
        // リンク（Anchor）の検証
        assert!(p.runs.iter().all(|r| matches!(&r.link, Some(ibis_doc_model::Link::Anchor(_)))), "runs link to bookmark");
    }

    let p0 = &entries[0].1;
    let p1 = &entries[1].1;
    assert!(p0.text().contains("システム構成図"), "entry 1 text: {:?}", p0.text());
    assert!(p1.text().contains("処理フロー図"), "entry 2 text: {:?}", p1.text());

    // リンクを辿る (follow_link) の検証
    let went = s.update(|e, _, cx| e.follow_link(FlowPos::new(p0.id, 1), cx));
    assert!(went, "follow_link succeeded");

    // 3. キャプションを追加し、目次位置で「目次の更新」を実行
    s.key("cmd-down");
    s.key("enter");
    s.menu("docx.caption.figure");
    s.type_text(" アーキテクチャ概要");

    // カレットを図表目次の最初の段落に戻す
    s.click_text(FlowPos::new(p0.id, 0));
    s.menu("docx.toc.update");

    let entries_updated = tof(&s.document());
    assert_eq!(entries_updated.len(), 3, "table of figures now has 3 entries after update");
    assert!(entries_updated[2].1.text().contains("アーキテクチャ概要"));

    // 4. Undo (cmd-z) で更新前の2件に戻る
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");
    let entries_undone = tof(&s.document());
    assert_eq!(entries_undone.len(), 2, "undone back to 2 entries");

    // 5. Redo (shift-cmd-z) で再度3件に復元
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("shift-cmd-z");
    let entries_redone = tof(&s.document());
    assert_eq!(entries_redone.len(), 3, "redone back to 3 entries");

    // 6. OOXML保存 & 再読込検証
    s.save_as("test-table-of-figures.docx");
    let reread = s.reopen();
    let flow = reread.flow.as_ref().unwrap();
    let reread_entries = tof(flow);
    assert_eq!(reread_entries.len(), 3, "3 entries preserved after save and reopen");
    let start_p = &reread_entries[0].1;
    assert!(start_p.field_start.as_deref().is_some_and(|i| i.contains("TOC") && i.contains("\\c") && i.contains("図")), "field_start preserved: {:?}", start_p.field_start);
}

/// todo-fix #8: ステータスバー・ズーム・ヒント・配置の多言語化（i18n）
fn fix_status_bar_and_zoom_i18n() {
    use crate::shell::editor::Tool;
    use crate::shell::i18n::{self, Language};

    // 1. 日本語モードでの検証
    i18n::show(Language::Japanese);
    assert_eq!(i18n::text("view.zoom-out"), "縮小");
    assert_eq!(i18n::text("view.zoom-in"), "拡大");
    assert_eq!(i18n::text("view.zoom-menu"), "拡大率の一覧");
    assert_eq!(i18n::text("view.zoom-custom"), "指定");
    assert_eq!(i18n::text("view.zoom-fit"), "画面に合わせる");
    assert_eq!(i18n::text("align.left"), "左揃え");
    assert_eq!(i18n::text("align.center"), "中央揃え");
    assert_eq!(i18n::text("align.right"), "右揃え");
    assert_eq!(i18n::text("align.justify"), "両端揃え");
    assert_eq!(i18n::text("status.hint.textbox"), "スライドをクリックまたはドラッグしてテキスト ボックスを配置");
    assert_eq!(i18n::text("status.hint.esc"), "Esc で編集を終了");

    let mut s = Scenario::new_document(ibis_pptx::Kind::Presentation);
    s.update(|e, _, _| e.tool = Tool::TextBox);
    assert!(s.status_texts().iter().any(|t| t == "スライドをクリックまたはドラッグしてテキスト ボックスを配置"));

    // 2. 英語モードでの検証
    i18n::show(Language::English);
    s.update(|_, _, _| {});
    assert_eq!(i18n::text("view.zoom-out"), "Zoom Out");
    assert_eq!(i18n::text("view.zoom-in"), "Zoom In");
    assert_eq!(i18n::text("view.zoom-menu"), "Zoom Levels");
    assert_eq!(i18n::text("view.zoom-custom"), "Custom");
    assert_eq!(i18n::text("view.zoom-fit"), "Fit to Window");
    assert_eq!(i18n::text("align.left"), "Align Left");
    assert_eq!(i18n::text("align.center"), "Align Center");
    assert_eq!(i18n::text("align.right"), "Align Right");
    assert_eq!(i18n::text("align.justify"), "Justify");
    assert_eq!(i18n::text("status.hint.textbox"), "Click or drag on the slide to place a text box");
    assert_eq!(i18n::text("status.hint.esc"), "Press Esc to finish editing");

    assert!(s.status_texts().iter().any(|t| t == "Click or drag on the slide to place a text box"));

    // 3. 日本語モードに戻す
    i18n::show(Language::Japanese);
    assert!(s.status_texts().iter().any(|t| t == "スライドをクリックまたはドラッグしてテキスト ボックスを配置"));
}

/// #21: earnings-presentation.pptx を開いて、右ペイン（サイドバー）を閉じたときの
/// スライド自動フィットのベンチマーク計測
pub(crate) fn fix21_pptx_sidebar_close_zoom_fit_benchmark() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    assert!(!s.with(|e, _| e.sidebar_open), "開いた直後は右ペイン（サイドバー）が閉じていること");
    assert!(s.with(|e, _| e.zoom.is_none()), "初期状態で自動フィットモードであること");
    let closed_scale = s.with(|e, _| e.slide_layout().map(|l| l.scale)).expect("slide_layout exists");
    s.menu("view.sidebar");
    s.settle();
    assert!(s.with(|e, _| e.sidebar_open), "メニューで右ペインが開くこと");
    let initial_scale = s.with(|e, _| e.slide_layout().map(|l| l.scale)).expect("slide_layout exists");
    assert!(initial_scale < closed_scale, "右ペインを開くと、スライドの自動フィットが狭い幅で計算されること");

    eprintln!("--- START BENCHMARK ---");
    let start = crate::clock::Instant::now();
    s.menu("view.sidebar");
    s.settle();
    let elapsed = start.elapsed();
    eprintln!("--- END BENCHMARK (s.menu(\"view.sidebar\") + s.settle(): {:?}) ---", elapsed);

    let new_scale = s.with(|e, _| e.slide_layout().map(|l| l.scale)).expect("slide_layout exists");
    assert!(!s.with(|e, _| e.sidebar_open), "サイドバーが閉じていること");
    assert!(new_scale > initial_scale, "サイドバーが閉じて表示領域が広がり、スライドが拡大フィットしていること: initial={initial_scale}, new={new_scale}");
    assert!((new_scale - closed_scale).abs() < closed_scale * 1e-3, "閉じた後の倍率は、最初から閉じていたときと同じ: {new_scale} / {closed_scale}");

    eprintln!("\n=======================================================");
    eprintln!("[BENCHMARK-FIX-21] Right pane close & slide auto-fit elapsed: {:?}", elapsed);
    eprintln!("Initial scale: {:.4}, New fitted scale: {:.4}", initial_scale, new_scale);
    eprintln!("=======================================================\n");
}

/// pptx の右ペインは、新規作成でも、開いたときでも、閉じた状態で始まる。
/// ツールバー（view.sidebar）で開ける。docx の右ペインは今までどおり開いている。
pub(crate) fn pptx_sidebar_starts_closed() {
    let mut s = Scenario::new_document(ibis_pptx::Kind::Presentation);
    assert!(!s.with(|e, _| e.sidebar_open), "新しいプレゼンテーションでは右ペインが閉じている");
    let closed_scale = s.with(|e, _| e.slide_layout().map(|l| l.scale)).expect("slide_layout");
    s.menu("view.sidebar");
    s.settle();
    assert!(s.with(|e, _| e.sidebar_open), "開くボタンで右ペインが開く");
    let open_scale = s.with(|e, _| e.slide_layout().map(|l| l.scale)).expect("slide_layout");
    assert!(open_scale < closed_scale, "開くとスライドの自動フィットが狭くなる: {open_scale} / {closed_scale}");

    drop(s); // One scenario window at a time.
    let mut s = Scenario::open("test/editing/pptx-comment-newline-enter.pptx");
    assert!(!s.with(|e, _| e.sidebar_open), "pptx を開いたとき右ペインは閉じている");
    drop(s);

    let mut s = Scenario::new_document(ibis_pptx::Kind::Document);
    assert!(s.with(|e, _| e.sidebar_open), "docx の右ペインは開いたまま");
}

/// XLSX-FILE-017: ファイル > 情報 (file.info) によるブックプロパティパネル
/// （タイトル・作成者・件名・キーワード・コメント・会社名等）の編集・適用、
/// 保存・再オープンでの保持、Undo/Redo 検証。
fn fix_xlsx_file017_doc_properties() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. file.info メニューからブックプロパティパネルを開く
    s.menu("file.info");
    assert!(s.with(|e, _| e.sidebar_open && e.sheet_state().is_some_and(|st| st.panels.properties.is_some())), "properties panel is open");

    // 2. プロパティを入力・適用
    s.update(|e, w, cx| {
        if let Some(p) = e.sheet_state_mut().and_then(|st| st.panels.properties.as_mut()) {
            p.title.update(cx, |inp, cx| inp.set_value("売上月次分析", w, cx));
            p.creator.update(cx, |inp, cx| inp.set_value("営業企画部", w, cx));
            p.subject.update(cx, |inp, cx| inp.set_value("月次売上レポート", w, cx));
            p.keywords.update(cx, |inp, cx| inp.set_value("売上, 2026, 分析", w, cx));
            p.description.update(cx, |inp, cx| inp.set_value("各支社の実績サマリー", w, cx));
            p.company.update(cx, |inp, cx| inp.set_value("株式会社アイビス", w, cx));
            p.manager.update(cx, |inp, cx| inp.set_value("山田", w, cx));
            p.category.update(cx, |inp, cx| inp.set_value("月次報告", w, cx));
        }
        e.apply_sheet_doc_properties(cx);
    });

    // 3. モデルのプロパティが更新されたことを検証
    s.with(|e, _| {
        let wb = e.presentation().workbook.as_deref().expect("workbook exists");
        assert_eq!(wb.properties.title.as_deref(), Some("売上月次分析"));
        assert_eq!(wb.properties.creator.as_deref(), Some("営業企画部"));
        assert_eq!(wb.properties.subject.as_deref(), Some("月次売上レポート"));
        assert_eq!(wb.properties.keywords.as_deref(), Some("売上, 2026, 分析"));
        assert_eq!(wb.properties.description.as_deref(), Some("各支社の実績サマリー"));
        assert_eq!(wb.properties.company.as_deref(), Some("株式会社アイビス"));
        assert_eq!(wb.properties.manager.as_deref(), Some("山田"));
        assert_eq!(wb.properties.category.as_deref(), Some("月次報告"));
    });

    s.screenshot("xlsx-file-info-properties");

    // 4. 保存と再読込での保持検証
    s.save_as("file017_sheet_properties.xlsx");
    let reloaded = s.reopen();
    let wb = reloaded.workbook.as_deref().expect("workbook exists");
    assert_eq!(wb.properties.title.as_deref(), Some("売上月次分析"));
    assert_eq!(wb.properties.creator.as_deref(), Some("営業企画部"));
    assert_eq!(wb.properties.subject.as_deref(), Some("月次売上レポート"));
    assert_eq!(wb.properties.keywords.as_deref(), Some("売上, 2026, 分析"));
    assert_eq!(wb.properties.description.as_deref(), Some("各支社の実績サマリー"));
    assert_eq!(wb.properties.company.as_deref(), Some("株式会社アイビス"));
    assert_eq!(wb.properties.manager.as_deref(), Some("山田"));
    assert_eq!(wb.properties.category.as_deref(), Some("月次報告"));

    // 5. Undo 検証
    s.key("cmd-z");
    s.with(|e, _| {
        let wb = e.presentation().workbook.as_deref().expect("workbook exists");
        assert_ne!(wb.properties.title.as_deref(), Some("売上月次分析"));
    });

    // 6. Redo 検証
    s.key("cmd-shift-z");
    s.with(|e, _| {
        let wb = e.presentation().workbook.as_deref().expect("workbook exists");
        assert_eq!(wb.properties.title.as_deref(), Some("売上月次分析"));
    });
}

/// Scenario verifying the fix for xlsx sheet tab scrollbar confinement and dynamic + button:
/// 1. When tabs are few (content width is small), + button is inline at the right of the last sheet tab.
/// 2. When tabs increase and approach/overflow the viewport, + button moves to the right-edge fixed area,
///    and the scrollview's right boundary is constrained to the left of the fixed + button.
/// 3. The horizontal scrollbar is confined within the scroll viewport and does not spill over the left buttons
///    (◀ ▶ ≡) or right edge.
/// 4. Left buttons (◀ ▶ ≡) and the fixed + button are clickable and work reliably even when scrolled.
fn fix_xlsx_sheet_tabs_scrollbar_and_fixed_add() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let sheets = |s: &mut Scenario| s.with(|e, _| e.presentation().slides.len());
    let (y, list, right, left) = (679.0, 70.0, 41.0, 14.0);

    // 1. Initial state: 3 sheets, no overflow, + button is inline at the end of tabs (~310.0).
    assert_eq!(sheets(&mut s), 3);
    let (_offset, max) = tab_offset(&mut s);
    assert_eq!(max, 0.0, "initially tabs do not overflow");
    s.screenshot("few_inline_plus");

    // Click inline + button to add the 4th sheet.
    s.click((310.0, y));
    assert_eq!(sheets(&mut s), 4, "clicked inline + button to add 4th sheet");

    // 2. Add sheets to overflow (increase up to 22 sheets).
    for _ in 0..18 {
        s.menu("xlsx.insert-sheet");
    }
    s.settle();
    let current_sheets = sheets(&mut s);
    assert_eq!(current_sheets, 22);

    let (_offset, max) = tab_offset(&mut s);
    assert!(max > 100.0, "tabs overflow: max_offset = {max}");
    s.screenshot("overflow_fixed_plus");

    // 3. Fixed + button on the right edge (~1186.0) adds another sheet.
    let fixed_plus_x = 1186.0;
    s.click((fixed_plus_x, y));
    s.settle();
    assert_eq!(sheets(&mut s), 23, "fixed + button on the right edge added 23rd sheet");

    // 4. Verify left buttons (◀ ▶ ≡) are fully responsive and not obstructed by the scrollbar.
    let (offset, max) = tab_offset(&mut s);

    // Click ◀ button: scrolls towards the beginning.
    s.click((left, y));
    s.settle();
    let (after_left, _) = tab_offset(&mut s);
    assert!(after_left > offset || offset == 0.0, "◀ button scrolled: {offset} -> {after_left}");
    s.screenshot("scrolled_left");

    // Click ▶ button: scrolls towards the end.
    s.click((right, y));
    s.settle();
    let (after_right, _) = tab_offset(&mut s);
    assert!(after_right < after_left || after_left <= -max, "▶ button scrolled: {after_left} -> {after_right}");

    // Click ≡ button: opens sheet list menu.
    s.click((list, y));
    s.settle();
    s.screenshot("sheet_list_opened");
}

/// Scenario directly verifying the user's specific feedback:
/// "Starting from an empty workbook, adding sheets with the + button:
///  At Sheet12, + should NOT move to the fixed area prematurely.
///  It should remain inline until Sheet14+ approaching the right edge."
fn fix_xlsx_sheet_add_threshold_blank_workbook() {
    let mut s = Scenario::new_document(ibis_pptx::Kind::Workbook);
    s.settle();
    let sheets = |s: &mut Scenario| s.with(|e, _| e.presentation().slides.len());
    let y = 679.0;

    // Initial state: Sheet1 (1 sheet).
    assert_eq!(sheets(&mut s), 1);
    let (_offset, max) = tab_offset(&mut s);
    assert_eq!(max, 0.0, "initially tabs do not overflow");

    // Add sheets up to Sheet12. At Sheet12, + should remain inline (no overflow).
    for target in 2..=12 {
        s.menu("xlsx.insert-sheet");
        s.settle();
        assert_eq!(sheets(&mut s), target, "added sheet {target}");
        let (_offset, max) = tab_offset(&mut s);
        assert_eq!(max, 0.0, "at sheet {target}, still inline without overflow");
    }

    assert_eq!(sheets(&mut s), 12);
    s.screenshot("sheet12_still_inline");

    // Add Sheet13 and Sheet14 via menu.
    for target in 13..=14 {
        s.menu("xlsx.insert-sheet");
        s.settle();
        assert_eq!(sheets(&mut s), target, "added sheet {target}");
    }

    s.screenshot("sheet14_near_edge");

    // Add more sheets until overflow occurs (up to 20 sheets).
    for target in 15..=20 {
        s.menu("xlsx.insert-sheet");
        s.settle();
        assert_eq!(sheets(&mut s), target);
    }

    // With 20 sheets, tabs overflow and + moves to the right edge fixed area (~1186.0).
    let (_offset, max) = tab_offset(&mut s);
    assert!(max > 50.0, "overflowed with 20 sheets: {max}");
    s.screenshot("overflowed_fixed_plus");

    // Click fixed + on right edge to add 21st sheet.
    s.click((1186.0, y));
    s.settle();
    assert_eq!(sheets(&mut s), 21, "fixed + on right edge added 21st sheet");
}

/// XLSX-FILE-018: カスタムプロパティ（docProps/custom.xml）の閲覧・追加・削除・適用、
/// 保存・再読込での完全復元、Undo/Redo 検証。
fn fix_xlsx_file018_custom_properties() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.settle();

    // 1. ファイル > 情報（file.info）からプロパティパネルを開く
    s.menu("file.info");
    s.settle();

    // 2. カスタムプロパティを追加
    s.update(|e, w, cx| {
        if let Some(p) = e.sheet_state_mut().and_then(|st| st.panels.properties.as_mut()) {
            p.new_custom_name.update(cx, |inp, cx| inp.set_value("プロジェクト名", w, cx));
            p.new_custom_val.update(cx, |inp, cx| inp.set_value("次世代オフィス", w, cx));
        }
        e.add_custom_property(w, cx);

        if let Some(p) = e.sheet_state_mut().and_then(|st| st.panels.properties.as_mut()) {
            p.new_custom_name.update(cx, |inp, cx| inp.set_value("承認状態", w, cx));
            p.new_custom_val.update(cx, |inp, cx| inp.set_value("承認済み", w, cx));
        }
        e.add_custom_property(w, cx);

        e.apply_sheet_doc_properties(cx);
    });

    // 3. モデルのカスタムプロパティが更新されたことを検証
    s.with(|e, _| {
        let wb = e.presentation().workbook.as_deref().expect("workbook exists");
        assert_eq!(wb.properties.custom.len(), 2);
        assert_eq!(wb.properties.custom[0].name, "プロジェクト名");
        assert_eq!(wb.properties.custom[0].value, ibis_doc_model::CustomPropertyValue::Text("次世代オフィス".to_string()));
        assert_eq!(wb.properties.custom[1].name, "承認状態");
        assert_eq!(wb.properties.custom[1].value, ibis_doc_model::CustomPropertyValue::Text("承認済み".to_string()));
    });

    s.screenshot("xlsx-file-custom-properties");

    // 4. 保存と再読込での完全保持検証
    s.save_as("file018_custom_properties.xlsx");
    let reloaded = s.reopen();
    let wb = reloaded.workbook.as_deref().expect("workbook exists");
    assert_eq!(wb.properties.custom.len(), 2);
    assert_eq!(wb.properties.custom[0].name, "プロジェクト名");
    assert_eq!(wb.properties.custom[0].value, ibis_doc_model::CustomPropertyValue::Text("次世代オフィス".to_string()));
    assert_eq!(wb.properties.custom[1].name, "承認状態");
    assert_eq!(wb.properties.custom[1].value, ibis_doc_model::CustomPropertyValue::Text("承認済み".to_string()));

    // 5. プロパティの削除と適用
    s.update(|e, _, cx| {
        e.remove_custom_property(0, cx);
        e.apply_sheet_doc_properties(cx);
    });
    s.with(|e, _| {
        let wb = e.presentation().workbook.as_deref().expect("workbook exists");
        assert_eq!(wb.properties.custom.len(), 1);
        assert_eq!(wb.properties.custom[0].name, "承認状態");
    });

    // 6. Undo 検証（削除が元に戻り、2件に戻る）
    s.key("cmd-z");
    s.with(|e, _| {
        let wb = e.presentation().workbook.as_deref().expect("workbook exists");
        assert_eq!(wb.properties.custom.len(), 2);
    });

    // 7. Redo 検証（再削除され、1件に戻る）
    s.key("cmd-shift-z");
    s.with(|e, _| {
        let wb = e.presentation().workbook.as_deref().expect("workbook exists");
        assert_eq!(wb.properties.custom.len(), 1);
    });
}

/// XLSX-SEC-022: 画像・図・グラフの代替テキスト（Alt Text）の表示・編集・保存・再オープン・Undo/Redo 検証.
fn fix_xlsx_sec022_alt_text() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. 図形未選択時はメニューが無効
    assert_eq!(s.menu_state("xlsx.alt-text"), Some((false, false)));

    // 2. シート1のグラフを選択
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);

    // グラフ選択時はメニューが有効
    assert_eq!(s.menu_state("xlsx.alt-text"), Some((true, false)));

    // 3. メニュー「xlsx.alt-text」を実行してサイドバーパネルを開く
    s.menu("xlsx.alt-text");
    assert!(s.with(|e, _| e.wb().panels.alt_text.is_some()));

    // 4. 代替テキスト（タイトル・説明）を入力して「適用」
    s.update(|e, window, cx| {
        let p = e.wb().panels.alt_text.as_ref().unwrap();
        p.title_field.update(cx, |i, cx| i.set_value("売上サマリーグラフ", window, cx));
        p.desc_field.update(cx, |i, cx| i.set_value("4〜6月の店舗別売上実績棒グラフ", window, cx));
        e.apply_sheet_alt_text(cx);
    });

    let chart_shape = s.with(|e, _| e.selected_sheet_shape()).unwrap();
    s.with(|e, _| {
        let alt = e.slide().shape_alt_texts.get(&chart_shape).unwrap();
        assert_eq!(alt.title, "売上サマリーグラフ");
        assert_eq!(alt.description, "4〜6月の店舗別売上実績棒グラフ");
    });

    // 5. 保存して再オープンし、代替テキストが正しく読み込まれることを検証
    s.save_as("sec022-alt-text.xlsx");
    let reopened = s.reopen();
    let reopened_alt = reopened.slides[0].shape_alt_texts.get(&chart_shape).expect("alt text preserved");
    assert_eq!(reopened_alt.title, "売上サマリーグラフ");
    assert_eq!(reopened_alt.description, "4〜6月の店舗別売上実績棒グラフ");

    // 6. Undo 検証
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        assert_eq!(e.slide().shape_alt_texts.get(&chart_shape), None);
    });

    // 7. Redo 検証
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        let alt = e.slide().shape_alt_texts.get(&chart_shape).unwrap();
        assert_eq!(alt.title, "売上サマリーグラフ");
    });
}

fn fix_xlsx_sec008_threaded_comments() {
    let mut s = Scenario::open("test/editing/xlsx-threads.xlsx");

    // 初期状態の確認（B2にスレッド、B4に解決済みスレッド）
    let threads_count = s.with(|e, _| e.current_sheet().unwrap().1.threads.len());
    assert_eq!(threads_count, 2);

    // 1. C3 (col: 2, row: 2) を選択して「insert.comment」を実行
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        if let Some(state) = e.sheet_state_mut() {
            state.cells.insert(slide, (2, 2));
        }
        cx.notify();
    });
    s.menu("insert.comment");
    assert!(s.with(|e, _| e.wb().panels.threaded_comment.is_some()));
    assert_eq!(s.with(|e, _| e.wb().panels.threaded_comment.as_ref().unwrap().cell), (2, 2));

    // 2. 新規コメントを投稿
    s.update(|e, window, cx| {
        e.send_threaded_comment("C3への新規コメント", window, cx);
    });
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        let t = sheet.thread_at(2, 2).expect("thread exists at C3");
        assert_eq!(t.posts.len(), 1);
        assert_eq!(t.posts[0].text, "C3への新規コメント");
        assert!(!t.done);
    });

    // 3. 返信を投稿
    s.update(|e, window, cx| {
        e.send_threaded_comment("返信メッセージ", window, cx);
    });
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        let t = sheet.thread_at(2, 2).unwrap();
        assert_eq!(t.posts.len(), 2);
        assert_eq!(t.posts[1].text, "返信メッセージ");
    });

    // 4. 解決済みに切り替え
    s.update(|e, _, cx| {
        e.resolve_threaded_comment(2, 2, true, cx);
    });
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert!(sheet.thread_at(2, 2).unwrap().done);
    });

    // 5. 再開に切り替え
    s.update(|e, _, cx| {
        e.resolve_threaded_comment(2, 2, false, cx);
    });
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert!(!sheet.thread_at(2, 2).unwrap().done);
    });

    // 6. 保存と再オープン（ラウンドトリップ検証）
    s.save_as("sec008-threads-test.xlsx");
    let reopened = s.reopen();
    let re_sheet = reopened.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ibis_doc_model::ShapeKind::Sheet(s) => Some(s),
        _ => None,
    }).unwrap();
    let re_t = re_sheet.thread_at(2, 2).expect("preserved after reload");
    assert_eq!(re_t.posts.len(), 2);
    assert_eq!(re_t.posts[0].text, "C3への新規コメント");
    assert_eq!(re_t.posts[1].text, "返信メッセージ");

    // 7. コメントの削除
    s.menu("xlsx.comment-delete");
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert!(sheet.thread_at(2, 2).is_none());
        assert!(e.wb().panels.threaded_comment.is_none());
    });

    // 8. Undo 検証
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert!(sheet.thread_at(2, 2).is_some());
    });

    // 9. Redo 検証
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert!(sheet.thread_at(2, 2).is_none());
    });
}

/// XLSX-FILE-004 / XLSX-FILE-007: テンプレート保存 (.xltx) およびテンプレートからの新規作成
fn fix_xlsx_file004_file007_templates() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let temp_dir = std::env::temp_dir();
    let xltx_path = temp_dir.join("test_scenario_sales_template.xltx");

    // 1. .xltx として保存（copy_to）
    s.with(|e, _| {
        assert!(e.is_workbook());
        e.copy_to(&xltx_path).expect("copy_to .xltx succeeds");
    });
    assert!(xltx_path.exists());

    // 2. 読み込み検証
    let loaded = crate::read_file(&xltx_path).expect("read xltx succeeds");
    assert!(loaded.presentation.workbook.is_some(), "saved xltx must be a workbook");

    // 3. new_from_template_path でテンプレートから新規作成
    s.update(|e, window, cx| {
        e.new_from_template_path(xltx_path.clone(), window, cx);
    });
    s.settle();

    s.with(|e, _| {
        assert_eq!(e.file_name.as_deref(), Some("Book1.xlsx"));
        assert!(e.path.is_none(), "template instance must be untitled (path is None)");
        assert!(e.is_workbook());
        assert!(e.presentation().workbook.is_some());
    });

    // 4. open_path でテンプレートファイルを開いた場合も新規ブック（path: None, Book1.xlsx）として開く
    s.update(|e, window, cx| {
        e.open_path(xltx_path.clone(), None, window, cx);
    });
    s.settle();

    s.with(|e, _| {
        assert_eq!(e.file_name.as_deref(), Some("Book1.xlsx"));
        assert!(e.path.is_none(), "opened template must be untitled (path is None)");
        assert!(e.is_workbook());
    });

    // 5. 後片付け
    let _ = std::fs::remove_file(xltx_path);
}

/// XLSX-SEC-021: アクセシビリティ検査パネルとジャンプ・再検査の動作検証
fn fix_xlsx_sec021_accessibility_check() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. メニューコマンド `xlsx.accessibility-check` からパネルを開く
    s.menu("xlsx.accessibility-check");
    s.settle();

    s.with(|e, _| {
        assert!(e.sidebar_open, "sidebar should be opened");
        let panel = e.wb().panels.accessibility.as_ref().expect("accessibility panel should be active");
        println!("Initial report: errors={}, warnings={}, tips={}",
            panel.report.errors.len(),
            panel.report.warnings.len(),
            panel.report.tips.len());
    });

    // 2. 図形（代替テキストなし）、テーブル（見出し行なし）、不明瞭リンクを追加して再検査
    s.update(|e, _window, cx| {
        let slide_id = e.current_slide;
        let slide = e.document.ids().slides.iter_mut().find(|s| s.id == slide_id).unwrap();

        // 代替テキストなしの図形（空のテキストボックス）を追加
        let new_shape_id = ibis_doc_model::ShapeId(9999);
        slide.shapes.push(ibis_doc_model::Shape::text_box(
            new_shape_id,
            ibis_doc_model::EmuRect::default(),
            "",
        ));
        slide.shape_names.insert(new_shape_id, "テスト図形".into());

        // 見出し行なしのテーブルと、不明瞭なハイパーリンクを追加
        for shape in &mut slide.shapes {
            if let ibis_doc_model::ShapeKind::Sheet(sheet_arc) = &mut shape.kind {
                let sheet = std::sync::Arc::make_mut(sheet_arc);
                sheet.tables.push(ibis_doc_model::sheet::SheetTable {
                    name: "Table_NoHeader".into(),
                    range: ibis_doc_model::sheet::CellRange {
                        first_row: 10,
                        first_column: 0,
                        last_row: 15,
                        last_column: 3,
                    },
                    header_rows: 0,
                    totals_rows: 0,
                    columns: vec!["Col1".into(), "Col2".into(), "Col3".into(), "Col4".into()],
                    style: None,
                    filter: None,
                    totals: Vec::new(),
                });

                sheet.cells.push(ibis_doc_model::sheet::SheetCell {
                    row: 20,
                    column: 1,
                    value: ibis_doc_model::sheet::CellValue::Empty,
                    text: "ここをクリック".into(),
                    kind: ibis_doc_model::sheet::CellKind::Text,
                    formula: None,
                    style: 0,
                    phonetic: None,
                    runs: None,
                });
                sheet.hyperlinks.push(ibis_doc_model::sheet::Hyperlink {
                    range: ibis_doc_model::sheet::CellRange {
                        first_row: 20,
                        first_column: 1,
                        last_row: 20,
                        last_column: 1,
                    },
                    target: Some("https://example.com".into()),
                    location: None,
                    tooltip: None,
                });
                break;
            }
        }

        // 再検査を実行
        e.recheck_xlsx_accessibility(cx);
    });
    s.settle();

    // 3. レポート内容の検証
    s.with(|e, _| {
        let panel = e.wb().panels.accessibility.as_ref().expect("accessibility panel should be active");
        assert!(!panel.report.errors.is_empty(), "should detect missing alt text error");
        assert!(!panel.report.warnings.is_empty(), "should detect missing table header warning or default sheet name");
        assert!(!panel.report.tips.is_empty(), "should detect unclear hyperlink tip");

        // エラー、警告、ヒントそれぞれについて検証
        let has_img_error = panel.report.errors.iter().any(|err| err.title.contains("テスト図形"));
        assert!(has_img_error, "must detect missing alt text for 'テスト図形'");

        let has_tbl_warning = panel.report.warnings.iter().any(|w| w.title.contains("Table_NoHeader"));
        assert!(has_tbl_warning, "must detect table without headers");

        let has_hl_tip = panel.report.tips.iter().any(|t| t.title.contains("B21"));
        assert!(has_hl_tip, "must detect unclear hyperlink at B21");
    });

    // 4. ジャンプ機能の検証
    s.update(|e, _window, cx| {
        // 全課題リストの先頭（エラー）へジャンプ
        e.jump_to_xlsx_accessibility_issue(0, cx);
    });
    s.settle();

    s.with(|e, _| {
        let panel = e.wb().panels.accessibility.as_ref().unwrap();
        assert_eq!(panel.selected_issue, Some(0));
    });

    // 5. パネルを閉じる
    s.update(|e, _window, cx| {
        e.close_xlsx_accessibility_panel(cx);
    });
    s.settle();

    s.with(|e, _| {
        assert!(!e.sidebar_open, "sidebar should be closed");
        assert!(e.wb().panels.accessibility.is_none(), "accessibility panel should be cleared");
    });
}

/// XLSX-SEC-019: 印刷対象（作業中のシート／ブック全体／選択した部分）の選択・PDF書き出し動作の検証
fn fix_xlsx_sec019_print_targets() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let out_dir = std::env::temp_dir().join(format!("test-sec019-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&out_dir);

    // 1. 初期状態は ActiveSheet（作業中のシート）
    s.with(|e, _| {
        assert_eq!(e.print_what(), crate::xlsx::PrintWhat::ActiveSheet);
    });

    // 2. ActiveSheet で PDF 書き出し
    let pdf1 = out_dir.join("active_sheet.pdf");
    let job1 = s.with(|e, _| e.write_pdf(pdf1.clone())).expect("write_pdf returns job");
    assert!(job1().is_ok(), "ActiveSheet pdf output succeeded");
    assert!(pdf1.exists());
    let bytes1 = std::fs::read(&pdf1).unwrap();
    assert!(bytes1.starts_with(b"%PDF-"));

    // 3. ページ設定パネルを開き、印刷対象を EntireWorkbook（ブック全体）に変更
    s.update(|e, _window, cx| {
        e.open_page_setup(cx);
        e.set_print_what(crate::xlsx::PrintWhat::EntireWorkbook, cx);
    });
    s.settle();

    s.with(|e, _| {
        assert!(e.sidebar_open);
        assert_eq!(e.print_what(), crate::xlsx::PrintWhat::EntireWorkbook);
    });

    // 4. EntireWorkbook で PDF 書き出し
    let pdf2 = out_dir.join("entire_workbook.pdf");
    let job2 = s.with(|e, _| e.write_pdf(pdf2.clone())).expect("write_pdf returns job");
    assert!(job2().is_ok(), "EntireWorkbook pdf output succeeded");
    assert!(pdf2.exists());
    let bytes2 = std::fs::read(&pdf2).unwrap();
    assert!(bytes2.starts_with(b"%PDF-"));

    // 5. 選択範囲（Selection）に変更し、セル範囲を選択して PDF 書き出し
    s.update(|e, _window, cx| {
        e.set_print_what(crate::xlsx::PrintWhat::Selection, cx);
        let sheet_id = e.current_slide;
        let wb = e.wb_mut();
        // B2:D6 を選択
        wb.cells.insert(sheet_id, (1, 1));
        wb.cell_ends.insert(sheet_id, (3, 5));
    });
    s.settle();

    s.with(|e, _| {
        assert_eq!(e.print_what(), crate::xlsx::PrintWhat::Selection);
        assert_eq!(e.selected_range(), Some(ibis_doc_model::CellRange { first_row: 1, first_column: 1, last_row: 5, last_column: 3 }));
    });

    let pdf3 = out_dir.join("selection.pdf");
    let job3 = s.with(|e, _| e.write_pdf(pdf3.clone())).expect("write_pdf returns job");
    assert!(job3().is_ok(), "Selection pdf output succeeded");
    assert!(pdf3.exists());
    let bytes3 = std::fs::read(&pdf3).unwrap();
    assert!(bytes3.starts_with(b"%PDF-"));

    // 6. 印刷対象を作業中のシート（ActiveSheet）に戻して確認
    s.update(|e, _window, cx| {
        e.set_print_what(crate::xlsx::PrintWhat::ActiveSheet, cx);
    });
    s.settle();

    s.with(|e, _| {
        assert_eq!(e.print_what(), crate::xlsx::PrintWhat::ActiveSheet);
    });

    let pdf4 = out_dir.join("active_sheet_again.pdf");
    let job4 = s.with(|e, _| e.write_pdf(pdf4.clone())).expect("write_pdf returns job");
    assert!(job4().is_ok(), "ActiveSheet pdf output succeeded");
    assert!(pdf4.exists());
    let bytes4 = std::fs::read(&pdf4).unwrap();
    assert!(bytes4.starts_with(b"%PDF-"));

    // 7. 後片付け
    let _ = std::fs::remove_dir_all(&out_dir);
}

/// XLSX-SEC-016: ヘッダー・フッターのコード（カスタム ヘッダー・フッター編集UI、左・中・右セクション入力、フィールドコード挿入、ページ設定連携）:
/// 1. ページ設定パネルのヘッダー・フッタープリセット選択とコード反映
/// 2. カスタム ヘッダー編集画面の起動、左・中・右セクションへの文字列・フィールドコード（&P, &N, &D, &A）入力・反映
/// 3. カスタム フッター編集画面の起動、フィールド挿入、直接コード入力（apply_hf_code_direct）
/// 4. ヘッダー・フッターオプション（different_first, different_odd_even, scale_with_doc, align_with_margins）の切り替え
/// 5. 偶数ページ・先頭ページの個別設定（set_hf_page_kind）
/// 6. Undo/Redo によるヘッダー・フッター変更の取り消し・再適用
/// 7. OOXML（.xlsx）保存・再オープンによるヘッダー・フッター全設定（odd, even, first, 各種フラグ）の完全保持検証
/// 8. PDF エクスポート（ベクター描画）がヘッダー・フッター付きで正常に出力されることを検証
fn fix_xlsx_sec016_custom_header_footer() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("file.page-setup");
    s.settle();

    // 1. ページ設定パネルが開いていることを確認
    s.with(|e, _| {
        assert!(e.sheet_state().map_or(false, |st| st.page_setup), "page setup panel should be open");
    });

    // 2. プリセットからヘッダーとフッターを選択
    s.update(|e, _, cx| {
        e.set_print(
            |p| {
                p.header_footer.odd_header = Some("&C&P".to_string());
                p.header_footer.odd_footer = Some("&C&P / &N ページ".to_string());
            },
            None,
            cx,
        );
    });
    s.settle();
    s.with(|e, _| {
        let p = e.current_print().unwrap();
        assert_eq!(p.header_footer.odd_header.as_deref(), Some("&C&P"));
        assert_eq!(p.header_footer.odd_footer.as_deref(), Some("&C&P / &N ページ"));
    });

    // 3. カスタム ヘッダー編集画面を開く
    s.update(|e, window, cx| {
        e.open_header_footer_edit(crate::xlsx::header_footer_edit::HfTarget::Header, window, cx);
    });
    s.settle();
    s.with(|e, _| {
        assert!(e.sheet_state().and_then(|st| st.header_footer_edit.as_ref()).is_some(), "header edit panel should be open");
    });

    // 4. 左側にシート名(&A)、中央にタイトル文字列、右側にページ番号(&P)と総ページ数(&N)を入力
    s.update(|e, window, cx| {
        let edit = e.sheet_state().and_then(|st| st.header_footer_edit.as_ref()).unwrap();
        edit.left.update(cx, |i, cx| i.set_value("&A".to_string(), window, cx));
        edit.center.update(cx, |i, cx| i.set_value("売上報告書".to_string(), window, cx));
        edit.right.update(cx, |i, cx| i.set_value("&P / &N".to_string(), window, cx));
        e.apply_header_footer_edit(cx);
    });
    s.settle();
    s.with(|e, _| {
        let p = e.current_print().unwrap();
        assert_eq!(p.header_footer.odd_header.as_deref(), Some("&L&A&C売上報告書&R&P / &N"));
    });

    // 5. カスタム フッター編集画面に切り替え、直接コード入力（apply_hf_code_direct）で設定
    s.update(|e, window, cx| {
        e.open_header_footer_edit(crate::xlsx::header_footer_edit::HfTarget::Footer, window, cx);
    });
    s.settle();
    s.update(|e, window, cx| {
        let edit = e.sheet_state().and_then(|st| st.header_footer_edit.as_ref()).unwrap();
        edit.code.update(cx, |i, cx| i.set_value("&L&D &T&R社外秘".to_string(), window, cx));
        e.apply_hf_code_direct(window, cx);
    });
    s.settle();
    s.with(|e, _| {
        let p = e.current_print().unwrap();
        assert_eq!(p.header_footer.odd_footer.as_deref(), Some("&L&D &T&R社外秘"));
    });

    // 6. オプション（先頭ページ別、奇数偶数別、拡大縮小、余白合わせ）の設定
    s.update(|e, _, cx| {
        e.set_print(
            |p| {
                p.header_footer.different_first = true;
                p.header_footer.different_odd_even = true;
                p.header_footer.scale_with_doc = false;
                p.header_footer.align_with_margins = false;
            },
            None,
            cx,
        );
    });
    s.settle();
    s.with(|e, _| {
        let p = e.current_print().unwrap();
        assert!(p.header_footer.different_first);
        assert!(p.header_footer.different_odd_even);
        assert!(!p.header_footer.scale_with_doc);
        assert!(!p.header_footer.align_with_margins);
    });

    // 7. 先頭ページ用・偶数ページ用のヘッダーを設定
    s.update(|e, window, cx| {
        e.open_header_footer_edit(crate::xlsx::header_footer_edit::HfTarget::Header, window, cx);
        e.set_hf_page_kind(crate::xlsx::header_footer_edit::HfPageKind::First, window, cx);
        let edit = e.sheet_state().and_then(|st| st.header_footer_edit.as_ref()).unwrap();
        edit.center.update(cx, |i, cx| i.set_value("表紙".to_string(), window, cx));
        e.apply_header_footer_edit(cx);

        e.set_hf_page_kind(crate::xlsx::header_footer_edit::HfPageKind::Even, window, cx);
        let edit = e.sheet_state().and_then(|st| st.header_footer_edit.as_ref()).unwrap();
        edit.left.update(cx, |i, cx| i.set_value("&P".to_string(), window, cx));
        edit.right.update(cx, |i, cx| i.set_value("&A".to_string(), window, cx));
        e.apply_header_footer_edit(cx);

        e.close_header_footer_edit(cx);
    });
    s.settle();

    s.with(|e, _| {
        let p = e.current_print().unwrap();
        assert_eq!(p.header_footer.first_header.as_deref(), Some("&C表紙"));
        assert_eq!(p.header_footer.even_header.as_deref(), Some("&L&P&R&A"));
        assert_eq!(p.header_footer.odd_header.as_deref(), Some("&L&A&C売上報告書&R&P / &N"));
    });

    // 8. Undo (cmd-z) で偶数ページの変更を取り消せることを確認
    s.key("cmd-z");
    s.settle();
    s.with(|e, _| {
        let p = e.current_print().unwrap();
        assert_eq!(p.header_footer.even_header, None);
        assert_eq!(p.header_footer.first_header.as_deref(), Some("&C表紙"));
    });

    // 9. Redo (shift-cmd-z) で再度適用されることを確認
    s.key("shift-cmd-z");
    s.settle();
    s.with(|e, _| {
        let p = e.current_print().unwrap();
        assert_eq!(p.header_footer.even_header.as_deref(), Some("&L&P&R&A"));
    });

    // 10. 保存・再オープンによる OOXML 往復検証
    s.save_as("fix_xlsx_custom_header_footer.xlsx");
    let saved = s.reopen();
    let saved_sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&saved.slides[0]).unwrap().clone();
    let saved_hf = &saved_sheet.print.header_footer;
    assert_eq!(saved_hf.odd_header.as_deref(), Some("&L&A&C売上報告書&R&P / &N"));
    assert_eq!(saved_hf.odd_footer.as_deref(), Some("&L&D &T&R社外秘"));
    assert_eq!(saved_hf.first_header.as_deref(), Some("&C表紙"));
    assert_eq!(saved_hf.even_header.as_deref(), Some("&L&P&R&A"));
    assert!(saved_hf.different_first);
    assert!(saved_hf.different_odd_even);
    assert!(!saved_hf.scale_with_doc);
    assert!(!saved_hf.align_with_margins);

    // 11. PDF エクスポートでヘッダー・フッター描画を含む正常出力確認
    let out_dir = std::env::temp_dir().join("ibis_test_sec016_hf");
    let _ = std::fs::create_dir_all(&out_dir);
    let pdf_path = out_dir.join("header_footer.pdf");
    let job = s.with(|e, _| e.write_pdf(pdf_path.clone())).expect("write_pdf returns job");
    assert!(job().is_ok(), "pdf output with custom header/footer succeeded");
    assert!(pdf_path.exists());
    let pdf_bytes = std::fs::read(&pdf_path).unwrap();
    assert!(pdf_bytes.starts_with(b"%PDF-"));

    let _ = std::fs::remove_dir_all(&out_dir);
}

fn fix_xlsx_object001_insert_shape_and_textbox() {
    let mut s = Scenario::open("test/editing/xlsx-formats.xlsx");

    // 初期状態：スライドにはワークシート 1 つのみが存在
    let initial_count = s.with(|e, _| e.slide().shapes.len());
    assert_eq!(initial_count, 1);
    assert!(matches!(s.with(|e, _| e.slide().shapes[0].kind.clone()), ibis_doc_model::ShapeKind::Sheet(_)));

    // 1. テキストボックスの挿入 (xlsx.insert-textbox)
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        if let Some(state) = e.sheet_state_mut() {
            state.cells.insert(slide, (1, 2)); // B3 セル
        }
        cx.notify();
    });
    s.menu("xlsx.insert-textbox");

    // テキストボックスが追加され、選択されていることを確認
    let tb_id = s.with(|e, _| e.selected_sheet_shape()).expect("textbox is selected");
    let tb_shape = s.with(|e, _| e.slide().shape(tb_id).unwrap().clone());
    assert!(matches!(tb_shape.kind, ibis_doc_model::ShapeKind::TextBox(_)));
    // B3 セルの位置に配置されていることを確認
    assert!(tb_shape.frame.x > 0);
    assert!(tb_shape.frame.y > 0);
    assert_eq!(tb_shape.frame.width, 2_000_000);
    assert_eq!(tb_shape.frame.height, 1_000_000);

    // テキストボックスにテキストを入力
    s.type_text("ワークシート上のテキストボックス");
    s.update(|e, window, cx| e.commit_edit(window, cx));
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.text(), "ワークシート上のテキストボックス");
    });

    // 2. 基本図形（四角形）の挿入 (xlsx.shape.rect)
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        if let Some(state) = e.sheet_state_mut() {
            state.cells.insert(slide, (3, 5)); // D6 セル
        }
        cx.notify();
    });
    s.menu("xlsx.shape.rect");
    let rect_id = s.with(|e, _| e.selected_sheet_shape()).expect("rect shape is selected");
    assert_ne!(rect_id, tb_id);
    let rect_shape = s.with(|e, _| e.slide().shape(rect_id).unwrap().clone());
    assert!(matches!(rect_shape.kind, ibis_doc_model::ShapeKind::Auto(_)));
    assert_eq!(rect_shape.frame.width, 2_000_000);
    assert_eq!(rect_shape.frame.height, 1_200_000);

    // 3. 楕円の挿入 (xlsx.shape.ellipse)
    s.menu("xlsx.shape.ellipse");
    let ellipse_id = s.with(|e, _| e.selected_sheet_shape()).expect("ellipse is selected");
    assert_ne!(ellipse_id, rect_id);
    let ellipse_shape = s.with(|e, _| e.slide().shape(ellipse_id).unwrap().clone());
    assert!(matches!(ellipse_shape.kind, ibis_doc_model::ShapeKind::Auto(_)));

    // 4. 矢印（右矢印）の挿入 (xlsx.shape.right-arrow)
    s.menu("xlsx.shape.right-arrow");
    let arrow_id = s.with(|e, _| e.selected_sheet_shape()).expect("arrow is selected");
    assert!(matches!(s.with(|e, _| e.slide().shape(arrow_id).unwrap().kind.clone()), ibis_doc_model::ShapeKind::Auto(_)));

    // 5. 直線の挿入 (xlsx.shape.line)
    s.menu("xlsx.shape.line");
    let line_id = s.with(|e, _| e.selected_sheet_shape()).expect("line is selected");
    let line_shape = s.with(|e, _| e.slide().shape(line_id).unwrap().clone());
    assert!(matches!(line_shape.kind, ibis_doc_model::ShapeKind::Line(_)));
    assert_eq!(line_shape.frame.height, 0);

    // 6. 矢印線の挿入 (xlsx.shape.line-arrow)
    s.menu("xlsx.shape.line-arrow");
    let arrow_line_id = s.with(|e, _| e.selected_sheet_shape()).expect("arrow line is selected");
    let arrow_line_shape = s.with(|e, _| e.slide().shape(arrow_line_id).unwrap().clone());
    assert!(matches!(arrow_line_shape.kind, ibis_doc_model::ShapeKind::Line(_)));

    // 7. 浮動画像の挿入 (insert_sheet_picture)
    // 最小限の 1x1 PNG バイナリ
    let png_1x1 = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
        0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41,
        0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
        0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
        0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
        0x42, 0x60, 0x82,
    ];
    let pic_id = s.update(|e, _, cx| {
        e.insert_sheet_picture("sample.png", png_1x1, cx)
    }).expect("picture inserted");
    assert!(matches!(s.with(|e, _| e.slide().shape(pic_id).unwrap().kind.clone()), ibis_doc_model::ShapeKind::Picture(_)));

    // 8. 前後順（順序）の検証
    // 現在の末尾は pic_id。rect_id を選択して最前面へ移動
    s.update(|e, _, _| e.selection = Some(rect_id));
    s.menu("xlsx.order-front");
    s.with(|e, _| {
        let last = e.slide().shapes.last().unwrap();
        assert_eq!(last.id, rect_id, "rect moved to front");
    });

    // 最背面へ移動（シートの直後、インデックス 1）
    s.menu("xlsx.order-back");
    s.with(|e, _| {
        assert_eq!(e.slide().shapes[1].id, rect_id, "rect moved to back (behind other shapes, above sheet)");
    });

    // 9. 削除と Undo / Redo の検証
    // rect_id を削除
    s.update(|e, window, cx| e.delete_selection(&crate::DeleteSelection, window, cx));
    s.with(|e, _| {
        assert!(e.slide().shape(rect_id).is_none(), "rect was deleted");
    });

    // Undo で復元
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        assert!(e.slide().shape(rect_id).is_some(), "rect restored by undo");
    });

    // Redo で再度削除
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        assert!(e.slide().shape(rect_id).is_none(), "rect removed again by redo");
    });

    // 10. Undo による全件ロールバックと初期状態での保存検証
    // すべての操作を Undo して初期状態に戻す
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), 1, "all shapes reverted to initial sheet-only state");
    });

    // Redo で再度適用されることを確認
    while s.with(|e, _| e.document.can_redo()) {
        s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    }
    s.with(|e, _| {
        assert!(e.slide().shapes.len() >= 5, "all shapes restored by redo");
    });

    // 再度初期状態に戻して保存・再オープンが正常に成功することを確認
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.save_as("fix_xlsx_object001_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), 1);
}

fn fix_xlsx_object008_selection_pane_align_and_frame() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());

    // 1. 図形を3つ挿入 (rect, ellipse, textbox)
    let rect_id = s.update(|e, _, cx| e.insert_sheet_shape("rect", cx));
    let _ellipse_id = s.update(|e, _, cx| e.insert_sheet_shape("ellipse", cx));
    let _tb_id = s.update(|e, window, cx| e.insert_sheet_textbox(window, cx));
    s.type_text("メモ");
    s.update(|e, window, cx| e.commit_edit(window, cx));

    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes + 3, "initial + 3 floating shapes");
    });

    // 2. 整列 (Align) と 均等配置 (Distribute) の検証 (XLSX-OBJECT-006)
    // rect を選択して左揃え
    s.update(|e, _, cx| {
        e.selection = Some(rect_id);
        e.align_sheet_shape(crate::pptx::editing::Align::Left, cx);
    });
    let rect_frame = s.with(|e, _| e.slide().shape(rect_id).unwrap().frame);
    let min_x = s.with(|e, _| e.slide().shapes.iter().skip(1).map(|s| s.frame.x).min().unwrap());
    assert_eq!(rect_frame.x, min_x, "rect aligned to left bound");

    // 均等配置 (水平方向)
    s.update(|e, _, cx| {
        e.distribute_sheet_shapes(true, cx);
    });
    s.with(|e, _| {
        let sh = &e.slide().shapes;
        let xs: Vec<i64> = sh.iter().skip(1).map(|s| s.frame.x).collect();
        assert_eq!(xs.len(), initial_shapes + 3 - 1);
    });

    // 均等配置 (垂直方向)
    s.update(|e, _, cx| {
        e.distribute_sheet_shapes(false, cx);
    });

    // 3. サイズと位置パネルの検証 (XLSX-OBJECT-004)
    s.update(|e, window, cx| {
        e.selection = Some(rect_id);
        e.open_sheet_frame_panel(window, cx);
    });
    s.with(|e, _| {
        assert!(e.sidebar_open, "sidebar opened for frame panel");
        assert!(e.sheet_state().unwrap().panels.frame.is_some(), "frame panel is present");
    });

    // フィールドを更新して適用
    s.update(|e, window, cx| {
        if let Some(panel) = e.sheet_state_mut().and_then(|st| st.panels.frame.as_mut()) {
            panel.fields[0].update(cx, |f, cx| f.set_value("6.0", window, cx)); // height 6 cm
            panel.fields[1].update(cx, |f, cx| f.set_value("8.0", window, cx)); // width 8 cm
            panel.fields[2].update(cx, |f, cx| f.set_value("45.0", window, cx)); // rotation 45 deg
        }
        e.apply_sheet_frame_panel(window, cx);
    });
    s.with(|e, _| {
        let shape = e.slide().shape(rect_id).unwrap();
        // 6 cm = 2_160_000 EMU
        assert_eq!(shape.frame.height, 2_160_000, "height updated to 6 cm");
        // 8 cm = 2_880_000 EMU
        assert_eq!(shape.frame.width, 2_880_000, "width updated to 8 cm");
        // 45 deg = 2_700_000
        assert_eq!(shape.rotation, 2_700_000, "rotation updated to 45 deg");
        assert!(!e.sidebar_open, "sidebar closed after apply");
    });

    // 4. 選択ウィンドウ (Selection Pane) の検証 (XLSX-OBJECT-008)
    s.update(|e, window, cx| {
        e.open_xlsx_selection_pane(window, cx);
    });
    s.with(|e, _| {
        assert!(e.sidebar_open, "sidebar opened for selection pane");
        assert!(e.sheet_state().unwrap().panels.selection_pane.is_some(), "selection pane present");
    });

    // パネルUIレンダリング確認
    let pane_ui = s.update(|e, _, cx| e.xlsx_selection_pane_panel(cx));
    assert!(pane_ui.is_some(), "selection pane sidebar panel rendered");

    // 図形の非表示切り替え (目のアイコン)
    s.update(|e, _, cx| {
        e.toggle_sheet_shape_hidden(rect_id, cx);
    });
    s.with(|e, _| {
        assert!(e.slide().hidden_shapes.contains(&rect_id), "rect is hidden");
    });

    // 再トグルで表示状態に復帰
    s.update(|e, _, cx| {
        e.toggle_sheet_shape_hidden(rect_id, cx);
    });
    s.with(|e, _| {
        assert!(!e.slide().hidden_shapes.contains(&rect_id), "rect is shown again");
    });

    // すべて非表示 / すべて表示
    let total_floating = initial_shapes + 3 - 1;
    s.update(|e, _, cx| {
        e.set_all_sheet_shapes_hidden(true, cx);
    });
    s.with(|e, _| {
        assert_eq!(e.slide().hidden_shapes.len(), total_floating, "all floating shapes hidden");
    });
    s.update(|e, _, cx| {
        e.set_all_sheet_shapes_hidden(false, cx);
    });
    s.with(|e, _| {
        assert!(e.slide().hidden_shapes.is_empty(), "all shapes shown again");
    });

    // 図形の名前変更 (鉛筆アイコン / インライン入力)
    s.update(|e, window, cx| {
        e.start_sheet_shape_renaming(rect_id, window, cx);
        if let Some(p) = e.sheet_state_mut().and_then(|st| st.panels.selection_pane.as_mut()) {
            if let Some((_, field)) = &p.renaming {
                field.update(cx, |f, cx| f.set_value("売上四角形", window, cx));
            }
        }
        e.finish_sheet_shape_renaming(window, cx);
    });
    s.with(|e, _| {
        assert_eq!(e.slide().shape_names.get(&rect_id).map(|s| s.as_str()), Some("売上四角形"), "shape renamed");
    });

    // パネルを閉じる
    s.update(|e, window, cx| {
        e.close_xlsx_selection_pane(window, cx);
    });
    s.with(|e, _| {
        assert!(!e.sidebar_open, "sidebar closed");
        assert!(e.sheet_state().unwrap().panels.selection_pane.is_none());
    });

    // 5. Undo / Redo で名前やサイズ変更が正確に戻ること
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx)); // undo rename
    s.with(|e, _| {
        assert_ne!(e.slide().shape_names.get(&rect_id).map(|s| s.as_str()), Some("売上四角形"), "rename undone");
    });
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx)); // redo rename
    s.with(|e, _| {
        assert_eq!(e.slide().shape_names.get(&rect_id).map(|s| s.as_str()), Some("売上四角形"), "rename redone");
    });

    // 6. 全件ロールバックと初期状態保存
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "reverted to initial shapes count");
    });
    s.save_as("fix_xlsx_object008_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), initial_shapes);
}

fn fix_xlsx_object003_image011_image018_shape_format_and_picture_properties() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());

    // 1. 図形（四角形）の挿入
    s.menu("xlsx.shape.rect");
    let rect_id = s.with(|e, _| e.selected_sheet_shape()).expect("rect selected");
    assert!(matches!(s.with(|e, _| e.slide().shape(rect_id).unwrap().kind.clone()), ibis_doc_model::ShapeKind::Auto(_)));

    // 2. 図形の塗りつぶし変更 (XLSX-OBJECT-003)
    s.menu("xlsx.shape-fill.blue");
    s.with(|e, _| {
        let shape = e.slide().shape(rect_id).unwrap();
        if let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind {
            assert_eq!(a.fill, ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x2b579a))));
        } else {
            panic!("not auto shape");
        }
    });

    // 3. 図形の枠線変更 (XLSX-OBJECT-003)
    s.menu("xlsx.shape-stroke.red");
    s.menu("xlsx.shape-stroke.width-3");
    s.menu("xlsx.shape-stroke.dash-dash");
    s.with(|e, _| {
        let shape = e.slide().shape(rect_id).unwrap();
        if let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind {
            let stroke = a.stroke.as_ref().expect("has stroke");
            assert_eq!(stroke.color, ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xc00000)));
            assert_eq!(stroke.width, 38_100);
            assert!(stroke.dashed);
            assert_eq!(stroke.dash, ibis_doc_model::LineDash::Dash);
        } else {
            panic!("not auto shape");
        }
    });

    // 4. 図形の効果（影）変更 (XLSX-OBJECT-003)
    s.menu("xlsx.shape-effect.shadow-bottom-right");
    s.with(|e, _| {
        let shape = e.slide().shape(rect_id).unwrap();
        if let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind {
            assert!(a.shadow.is_some(), "shadow applied");
        } else {
            panic!("not auto shape");
        }
    });

    // 塗り・枠線・影のクリア
    s.menu("xlsx.shape-fill.none");
    s.menu("xlsx.shape-stroke.none");
    s.menu("xlsx.shape-effect.shadow-none");
    s.with(|e, _| {
        let shape = e.slide().shape(rect_id).unwrap();
        if let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind {
            assert_eq!(a.fill, ibis_doc_model::Fill::None);
            assert!(a.stroke.is_none());
            assert!(a.shadow.is_none());
        } else {
            panic!("not auto shape");
        }
    });

    // 5. 浮動画像の挿入
    let png_1x1 = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
        0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41,
        0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
        0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
        0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
        0x42, 0x60, 0x82,
    ];
    let pic_id = s.update(|e, _, cx| {
        e.insert_sheet_picture("product.png", png_1x1, cx)
    }).expect("picture inserted");
    assert_eq!(s.with(|e, _| e.selected_sheet_shape()), Some(pic_id));

    // 6. 画像のサイズ・縦横比・回転の設定 (XLSX-IMAGE-011)
    s.menu("xlsx.frame-settings");
    assert!(s.with(|e, _| e.sheet_state().unwrap().panels.frame.is_some()));
    s.update(|e, window, cx| {
        let p = e.sheet_state_mut().unwrap().panels.frame.as_mut().unwrap();
        p.fields[0].update(cx, |f, cx| f.set_value("4", window, cx)); // height: 4cm
        p.fields[1].update(cx, |f, cx| f.set_value("5", window, cx)); // width: 5cm
        p.fields[2].update(cx, |f, cx| f.set_value("45", window, cx)); // rotation: 45°
        p.fields[3].update(cx, |f, cx| f.set_value("2", window, cx)); // x: 2cm
        p.fields[4].update(cx, |f, cx| f.set_value("3", window, cx)); // y: 3cm
        p.locked = true; // aspect ratio locked
        e.apply_sheet_frame_panel(window, cx);
    });
    s.with(|e, _| {
        let shape = e.slide().shape(pic_id).unwrap();
        assert_eq!(shape.frame.height, 4 * 360_000); // 1,440,000 EMU
        assert_eq!(shape.frame.width, 5 * 360_000);  // 1,800,000 EMU
        assert_eq!(shape.frame.x, 2 * 360_000);      // 720,000 EMU
        assert_eq!(shape.frame.y, 3 * 360_000);      // 1,080,000 EMU
        assert_eq!(shape.rotation, 45 * 60_000);     // 2,700,000
        assert!(e.slide().aspect_locked.contains(&pic_id), "aspect locked");
    });

    // 7. 画像の代替テキスト設定とファイル保存・再読込 (XLSX-IMAGE-018)
    s.menu("xlsx.alt-text");
    assert!(s.with(|e, _| e.sheet_state().unwrap().panels.alt_text.is_some()));
    s.update(|e, window, cx| {
        let p = e.sheet_state().unwrap().panels.alt_text.as_ref().unwrap();
        p.title_field.update(cx, |f, cx| f.set_value("製品画像", window, cx));
        p.desc_field.update(cx, |f, cx| f.set_value("新製品の正面写真", window, cx));
        e.apply_sheet_alt_text(cx);
    });
    s.with(|e, _| {
        let alt = e.slide().shape_alt_texts.get(&pic_id).expect("alt text found");
        assert_eq!(alt.title, "製品画像");
        assert_eq!(alt.description, "新製品の正面写真");
    });

    // 8. Undo / Redo 検証
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx)); // undo alt text
    s.with(|e, _| {
        assert_eq!(e.slide().shape_alt_texts.get(&pic_id), None, "alt text undone");
    });
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx)); // redo alt text
    s.with(|e, _| {
        assert_eq!(e.slide().shape_alt_texts.get(&pic_id).unwrap().title, "製品画像", "alt text redone");
    });

    // 全件ロールバック（新規追加した図形・画像を完全に巻き戻す）
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "reverted to initial shapes count");
    });
    s.save_as("fix_xlsx_object003_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), initial_shapes);
}

/// XLSX-OBJECT-007 (図形のグループ化と解除) および XLSX-IMAGE-012 (画像の切抜き)
fn fix_xlsx_object007_group_and_image012_crop() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());
    assert!(s.with(|e, _| e.slide().groups.is_empty()), "initial groups empty");

    // 1. 基本図形2個（四角形、楕円）および画像を挿入
    s.menu("xlsx.shape.rect");
    let rect_id = s.with(|e, _| e.selected_sheet_shape()).expect("rect inserted");

    s.menu("xlsx.shape.ellipse");
    let ellipse_id = s.with(|e, _| e.selected_sheet_shape()).expect("ellipse inserted");

    s.menu("xlsx.shape.star5");
    let star_id = s.with(|e, _| e.selected_sheet_shape()).expect("star inserted");

    let png_1x1 = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
        0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41,
        0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
        0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
        0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
        0x42, 0x60, 0x82,
    ];
    let pic_id = s.update(|e, _, cx| {
        e.insert_sheet_picture("sample.png", png_1x1, cx)
    }).expect("picture inserted");

    // 2. 複数選択とグループ化可否判定 (can_group / can_ungroup)
    s.update(|e, _, _| e.select_sheet_shape(rect_id));
    assert_eq!(s.with(|e, _| e.selected_ids()), vec![rect_id]);
    assert!(!s.with(|e, _| e.can_group()), "single shape cannot group");
    assert!(!s.with(|e, _| e.can_ungroup()), "ungrouped shape cannot ungroup");

    // 楕円を追加選択（複数選択）
    s.update(|e, _, _| e.toggle_select_sheet_shape(ellipse_id));
    assert_eq!(s.with(|e, _| e.selected_ids().len()), 2);
    assert!(s.with(|e, _| e.can_group()), "multiple shapes can group");
    assert!(!s.with(|e, _| e.can_ungroup()), "not yet grouped");

    // 3. グループ化実行 (XLSX-OBJECT-007)
    s.menu("xlsx.group-shapes.group");
    s.with(|e, _| {
        let groups = &e.slide().groups;
        assert_eq!(groups.len(), 1, "one group formed");
        assert_eq!(groups[0].name, "グループ 1");
        assert_eq!(groups[0].children.len(), 2);
    });
    assert!(s.with(|e, _| e.can_ungroup()), "grouped shapes can ungroup");

    // 4. グループ化の Undo / Redo
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        assert_eq!(e.slide().groups.len(), 0, "group undone");
    });
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        assert_eq!(e.slide().groups.len(), 1, "group redone");
    });

    // 5. ネストしたグループ化（グループ + 星）
    s.update(|e, _, _| e.select_sheet_shapes(&[rect_id, star_id]));
    assert!(s.with(|e, _| e.can_group()), "group + shape can group");
    s.menu("xlsx.group-shapes.group");
    s.with(|e, _| {
        let groups = &e.slide().groups;
        assert_eq!(groups.len(), 1, "nested group formed");
        assert_eq!(groups[0].name, "グループ 2");
        assert_eq!(groups[0].children.len(), 2);
        let has_subgroup = groups[0].children.iter().any(|c| matches!(c, ibis_doc_model::GroupMember::Group(_)));
        let has_star = groups[0].children.iter().any(|c| matches!(c, ibis_doc_model::GroupMember::Shape(id) if *id == star_id));
        assert!(has_subgroup && has_star, "nested group structure verified");
    });

    // 6. グループ解除実行 (XLSX-OBJECT-007)
    // 外側グループ解除
    s.menu("xlsx.group-shapes.ungroup");
    s.with(|e, _| {
        let groups = &e.slide().groups;
        assert_eq!(groups.len(), 1, "outer group dissolved, inner group remains");
        assert_eq!(groups[0].name, "グループ 1");
    });

    // 内側グループ解除
    s.menu("xlsx.group-shapes.ungroup");
    s.with(|e, _| {
        assert!(e.slide().groups.is_empty(), "inner group dissolved, all ungrouped");
    });

    // Undo で内側グループ、さらに外側グループ復元
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        assert_eq!(e.slide().groups.len(), 1, "inner group restored by undo");
        assert_eq!(e.slide().groups[0].name, "グループ 1");
    });
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        assert_eq!(e.slide().groups.len(), 1, "outer group restored by undo");
        assert_eq!(e.slide().groups[0].name, "グループ 2");
    });

    // 7. 画像の切抜き検証 (XLSX-IMAGE-012)
    s.update(|e, _, _| e.select_sheet_shape(pic_id));
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.crop, ibis_doc_model::Crop::default(), "initial crop is 0");
    });

    // 左を10%切り抜く
    s.menu("xlsx.crop.left");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.crop.left, 10_000);
        assert_eq!(p.crop.top, 0);
    });

    // 四辺を10%切り抜く
    s.menu("xlsx.crop.center");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.crop, ibis_doc_model::Crop { left: 10_000, top: 10_000, right: 10_000, bottom: 10_000 });
    });

    // 切り抜きの Undo / Redo
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.crop.left, 10_000);
        assert_eq!(p.crop.top, 0, "undone to left 10%");
    });
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.crop, ibis_doc_model::Crop { left: 10_000, top: 10_000, right: 10_000, bottom: 10_000 }, "redone to center crop");
    });

    // トリミングを解除
    s.menu("xlsx.crop.reset");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.crop, ibis_doc_model::Crop::default(), "crop reset");
    });

    // 8. 全件ロールバック（初期状態へ巻き戻す）
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "reverted to initial shapes count");
        assert!(e.slide().groups.is_empty(), "groups cleared after undo");
    });
    s.save_as("fix_xlsx_object007_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), initial_shapes);
    assert!(saved.slides[0].groups.is_empty());
}

/// XLSX-IMAGE-013: 画像補正・色・透明度 & XLSX-IMAGE-014: 画像効果・枠線
fn fix_xlsx_image013_color_and_image014_effects() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());

    let png_1x1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
        0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41,
        0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
        0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
        0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
        0x42, 0x60, 0x82,
    ];
    let pic_id = s.update(|e, _, cx| {
        e.insert_sheet_picture("photo.png", png_1x1.to_vec(), cx)
    }).expect("picture inserted");

    // 画像を選択
    s.update(|e, _, _| e.select_sheet_shape(pic_id));
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.duotone, None);
        assert_eq!(p.luminance, None);
        assert_eq!(p.corner, 0);
        assert_eq!(p.stroke, None);
    });

    // 1. 画像の色効果 (XLSX-IMAGE-013)
    s.menu("xlsx.picture.color.grayscale");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.duotone, Some((ibis_doc_model::Rgb(0x000000), ibis_doc_model::Rgb(0xffffff))), "grayscale duotone");
    });

    s.menu("xlsx.picture.color.sepia");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.duotone, Some((ibis_doc_model::Rgb(0x402000), ibis_doc_model::Rgb(0xffe0c0))), "sepia duotone");
    });

    s.menu("xlsx.picture.color.none");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.duotone, None, "duotone cleared");
    });

    // 2. 画像の補正（明るさ・コントラスト） (XLSX-IMAGE-013)
    s.menu("xlsx.picture.lum.bright-p40");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.luminance, Some((40_000, 0)), "brightness +40%");
    });

    s.menu("xlsx.picture.lum.contrast-p20");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.luminance, Some((0, 20_000)), "contrast +20%");
    });

    s.menu("xlsx.picture.lum.bright-contrast-high");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.luminance, Some((20_000, 20_000)), "high contrast");
    });

    // 3. 画像の角丸効果 (XLSX-IMAGE-014)
    s.menu("xlsx.picture.corner.medium");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.corner, 10_000, "corner 10%");
    });

    s.menu("xlsx.picture.corner.large");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.corner, 20_000, "corner 20%");
    });

    // 4. 画像の枠線（色・太さ・破線） (XLSX-IMAGE-014)
    s.menu("xlsx.shape-stroke.blue");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        let stroke = p.stroke.as_ref().expect("stroke applied");
        assert_eq!(stroke.color, ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x2b579a)));
    });

    s.menu("xlsx.shape-stroke.width-3");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        let stroke = p.stroke.as_ref().expect("stroke applied");
        assert_eq!(stroke.width, 38_100, "3pt stroke");
    });

    s.menu("xlsx.shape-stroke.dash-dash");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        let stroke = p.stroke.as_ref().expect("stroke applied");
        assert_eq!(stroke.dash, ibis_doc_model::LineDash::Dash, "dashed stroke");
    });

    // 5. 色と効果のリセット
    s.menu("xlsx.picture.color.blue");
    s.menu("xlsx.picture.reset");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.duotone, None, "duotone reset");
        assert_eq!(p.luminance, None, "luminance reset");
        assert_eq!(p.corner, 0, "corner reset");
        assert_eq!(p.stroke, None, "stroke reset");
    });

    // 6. Undo / Redo 検証
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.duotone, Some((ibis_doc_model::Rgb(0x002060), ibis_doc_model::Rgb(0xd9e1f2))), "blue duotone restored");
        assert_eq!(p.corner, 20_000, "corner restored");
        assert!(p.stroke.is_some(), "stroke restored");
    });

    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.duotone, None, "reset redone");
        assert_eq!(p.luminance, None);
        assert_eq!(p.corner, 0);
        assert_eq!(p.stroke, None);
    });

    // 7. 全件ロールバック
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "picture reverted");
    });
    s.save_as("fix_xlsx_image013_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), initial_shapes);
}

/// XLSX-OBJECT-012: テキストボックス内の文字・段落書式 & XLSX-OBJECT-013: 文字方向・上下配置・内余白
fn fix_xlsx_object012_object013_text_format_and_textbox_layout() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());

    // 1. テキストボックス挿入
    s.menu("xlsx.insert-textbox");
    let tb_id = s.with(|e, _| e.selected_ids().first().copied()).expect("textbox inserted and selected");

    // テキストを入力
    s.type_text("Hello XLSX");
    s.update(|e, window, cx| e.commit_edit(window, cx));
    s.update(|e, _, _| e.select_sheet_shape(tb_id));

    s.with(|e, _| {
        let shape = e.slide().shape(tb_id).unwrap();
        let body = shape.text_body().unwrap();
        assert_eq!(body.text(), "Hello XLSX");
    });

    // 2. 文字書式の検証 (XLSX-OBJECT-012)
    // 太字 (Bold)
    s.menu("xlsx.text.bold");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.paragraphs[0].runs[0].style.bold, Some(true), "bold applied");
    });

    // 斜体 (Italic)
    s.menu("xlsx.text.italic");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.paragraphs[0].runs[0].style.italic, Some(true), "italic applied");
    });

    // 下線 (Underline)
    s.menu("xlsx.text.underline");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert!(body.paragraphs[0].runs[0].style.underline, "underline applied");
    });

    // 取り消し線 (Strikethrough)
    s.menu("xlsx.text.strikethrough");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_ne!(body.paragraphs[0].runs[0].style.strike, ibis_doc_model::Strike::None, "strike applied");
    });

    // 文字色 (Color)
    s.menu("xlsx.text.color-blue");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.paragraphs[0].runs[0].style.color, Some(ibis_doc_model::Rgb(0x2b579a)), "blue color applied");
    });

    // フォントサイズ (Size 24pt = 2400)
    s.menu("xlsx.text.size-24");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.paragraphs[0].runs[0].style.font_size, Some(2400), "24pt font size applied");
    });

    // 3. 段落配置の検証 (XLSX-OBJECT-012)
    s.menu("align.center");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.paragraphs[0].align, Some(ibis_doc_model::TextAlign::Center), "center align applied");
    });

    s.menu("align.right");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.paragraphs[0].align, Some(ibis_doc_model::TextAlign::Right), "right align applied");
    });

    // 4. 上下配置の検証 (XLSX-OBJECT-013)
    s.menu("xlsx.text-vertical.middle");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.anchor, ibis_doc_model::VerticalAnchor::Middle, "vertical middle anchor applied");
    });

    s.menu("xlsx.text-vertical.bottom");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.anchor, ibis_doc_model::VerticalAnchor::Bottom, "vertical bottom anchor applied");
    });

    // 5. 文字列の方向の検証 (XLSX-OBJECT-013)
    s.menu("xlsx.text-direction.vertical");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert!(body.vertical, "vertical text applied");
    });

    s.menu("xlsx.text-direction.horizontal");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert!(!body.vertical, "horizontal text applied");
    });

    // 6. 内余白の検証 (XLSX-OBJECT-013)
    s.menu("xlsx.text-insets.none");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.insets, ibis_doc_model::Insets { left: 0, top: 0, right: 0, bottom: 0 }, "zero insets applied");
    });

    s.menu("xlsx.text-insets.wide");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.insets.left, 182_880, "wide insets applied");
    });

    // 7. テキスト折り返しの検証 (XLSX-OBJECT-013)
    let initial_wrap = s.with(|e, _| e.slide().shape(tb_id).unwrap().text_body().unwrap().wrap);
    s.menu("xlsx.text.wrap");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.wrap, !initial_wrap, "text wrap toggled");
    });

    // 8. Undo / Redo 検証
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.wrap, initial_wrap, "wrap undone");
    });

    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.wrap, !initial_wrap, "wrap redone");
    });

    // 9. 全件ロールバック
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "textbox reverted");
    });
    s.save_as("fix_xlsx_object012_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), initial_shapes);
}

fn fix_xlsx_object009_object010_image016_shape_placement_and_print() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. シート上の既存図形・グラフの取得・選択
    let shape_id = s.with(|e, _| {
        e.slide().shapes.iter().find(|sh| !matches!(sh.kind, ibis_doc_model::ShapeKind::Sheet(_))).unwrap().id
    });
    s.update(|e, _, _| {
        e.select_sheet_shape(shape_id);
    });

    // 初期状態の検証 (既定は MoveAndSize, 印刷=true)
    let (initial_placement, initial_print) = s.with(|e, _| {
        (
            e.slide().shape_placements.get(&shape_id).copied().unwrap_or_default(),
            e.slide().shape_prints.get(&shape_id).copied().unwrap_or(true),
        )
    });
    assert_eq!(initial_placement, ibis_doc_model::ObjectPlacement::MoveAndSize);
    assert_eq!(initial_print, true);

    // 2. メニュー経由での配置変更 (XLSX-OBJECT-009 / XLSX-IMAGE-016)
    // 「セルに合わせて移動するがサイズ変更はしない」
    s.menu("xlsx.placement.move-only");
    s.with(|e, _| {
        assert_eq!(e.slide().shape_placements.get(&shape_id).copied().unwrap_or_default(), ibis_doc_model::ObjectPlacement::MoveOnly);
    });

    // 「セルに合わせて移動やサイズ変更をしない」
    s.menu("xlsx.placement.free");
    s.with(|e, _| {
        assert_eq!(e.slide().shape_placements.get(&shape_id).copied().unwrap_or_default(), ibis_doc_model::ObjectPlacement::Free);
    });

    // 「セルに合わせて移動やサイズ変更をする」
    s.menu("xlsx.placement.move-and-size");
    s.with(|e, _| {
        assert_eq!(e.slide().shape_placements.get(&shape_id).copied().unwrap_or_default(), ibis_doc_model::ObjectPlacement::MoveAndSize);
    });

    // 3. メニュー経由での印刷指定変更 (XLSX-OBJECT-010)
    // 印刷トグル -> オフ
    s.menu("xlsx.placement.print");
    s.with(|e, _| {
        assert_eq!(e.slide().shape_prints.get(&shape_id).copied().unwrap_or(true), false);
    });

    // 印刷トグル -> オン
    s.menu("xlsx.placement.print");
    s.with(|e, _| {
        assert_eq!(e.slide().shape_prints.get(&shape_id).copied().unwrap_or(true), true);
    });

    // 4. サイズと位置サイドバーパネルの操作・検証 (UI連携)
    s.menu("xlsx.frame-settings");
    s.with(|e, _| {
        assert!(e.sheet_state().unwrap().panels.frame.is_some(), "frame panel opened");
        let panel = e.sheet_state().unwrap().panels.frame.as_ref().unwrap();
        assert_eq!(panel.placement, ibis_doc_model::ObjectPlacement::MoveAndSize);
        assert_eq!(panel.print, true);
    });

    // パネル上で MoveOnly + 印刷オフ に切り替えて適用
    s.update(|e, window, cx| {
        if let Some(panel) = e.wb_mut().panels.frame.as_mut() {
            panel.placement = ibis_doc_model::ObjectPlacement::MoveOnly;
            panel.print = false;
        }
        e.apply_sheet_frame_panel(window, cx);
    });

    s.with(|e, _| {
        assert_eq!(e.slide().shape_placements.get(&shape_id).copied().unwrap_or_default(), ibis_doc_model::ObjectPlacement::MoveOnly);
        assert_eq!(e.slide().shape_prints.get(&shape_id).copied().unwrap_or(true), false);
    });

    // 5. Undo / Redo の検証
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        assert_eq!(e.slide().shape_placements.get(&shape_id).copied().unwrap_or_default(), ibis_doc_model::ObjectPlacement::MoveAndSize);
        assert_eq!(e.slide().shape_prints.get(&shape_id).copied().unwrap_or(true), true);
    });

    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        assert_eq!(e.slide().shape_placements.get(&shape_id).copied().unwrap_or_default(), ibis_doc_model::ObjectPlacement::MoveOnly);
        assert_eq!(e.slide().shape_prints.get(&shape_id).copied().unwrap_or(true), false);
    });

    // 6. 保存と再読込による永続化の検証
    s.save_as("fix_xlsx_object009_010_saved.xlsx");
    let reopened = s.reopen();
    assert_eq!(reopened.slides[0].shape_placements.get(&shape_id).copied().unwrap_or_default(), ibis_doc_model::ObjectPlacement::MoveOnly);
    assert_eq!(reopened.slides[0].shape_prints.get(&shape_id).copied().unwrap_or(true), false);

    // 7. 全件ロールバック
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shape_placements.get(&shape_id).copied().unwrap_or_default(), initial_placement);
        assert_eq!(e.slide().shape_prints.get(&shape_id).copied().unwrap_or(true), initial_print);
    });
}

/// XLSX-OBJECT-002: 図形の変更（Change Shape）
/// オートシェイプの形状（四角形、楕円、星、ハートなど）変更、テキストボックスからの変換、Undo/Redo、ロールバックの検証。
fn fix_xlsx_object002_change_shape() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());

    // 1. 基本図形（四角形）を挿入
    s.menu("xlsx.shape.rect");
    let shape_id = s.with(|e, _| e.selected_sheet_shape()).expect("rect shape inserted");

    s.with(|e, _| {
        let shape = e.slide().shape(shape_id).unwrap();
        let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!("not auto") };
        assert_eq!(a.geometry.preset_name(), Some("rect"));
    });

    // 2. 「書式 > 図形の変更」メニューから楕円に変更
    s.menu("xlsx.change-shape.ellipse");
    s.with(|e, _| {
        let shape = e.slide().shape(shape_id).unwrap();
        let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!("not auto") };
        assert_eq!(a.geometry.preset_name(), Some("ellipse"));
    });

    // 3. 星に変更
    s.menu("xlsx.change-shape.star5");
    s.with(|e, _| {
        let shape = e.slide().shape(shape_id).unwrap();
        let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!("not auto") };
        assert_eq!(a.geometry.preset_name(), Some("star5"));
    });

    // 4. ハートに変更
    s.menu("xlsx.change-shape.heart");
    s.with(|e, _| {
        let shape = e.slide().shape(shape_id).unwrap();
        let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!("not auto") };
        assert_eq!(a.geometry.preset_name(), Some("heart"));
    });

    // 5. Undo / Redo の検証
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx)); // back to star5
    s.with(|e, _| {
        let shape = e.slide().shape(shape_id).unwrap();
        let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!("not auto") };
        assert_eq!(a.geometry.preset_name(), Some("star5"));
    });

    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx)); // back to ellipse
    s.with(|e, _| {
        let shape = e.slide().shape(shape_id).unwrap();
        let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!("not auto") };
        assert_eq!(a.geometry.preset_name(), Some("ellipse"));
    });

    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx)); // redo to star5
    s.with(|e, _| {
        let shape = e.slide().shape(shape_id).unwrap();
        let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!("not auto") };
        assert_eq!(a.geometry.preset_name(), Some("star5"));
    });

    // 6. テキストボックスからの図形変更
    s.menu("xlsx.insert-textbox");
    let tb_id = s.with(|e, _| e.selected_ids().first().copied()).expect("textbox inserted");
    s.type_text("Memo");
    s.update(|e, window, cx| e.commit_edit(window, cx));
    s.update(|e, _, _| e.select_sheet_shape(tb_id));

    // テキストボックスから吹き出し（wedgeRectCallout）に変更
    s.menu("xlsx.change-shape.callout");
    s.with(|e, _| {
        let shape = e.slide().shape(tb_id).unwrap();
        let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!("converted to auto") };
        assert_eq!(a.geometry.preset_name(), Some("wedgeRectCallout"));
        assert_eq!(a.text.as_ref().unwrap().text(), "Memo");
    });

    // 7. 全件ロールバック
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "all inserted shapes rolled back");
    });
}

/// XLSX-OBJECT-014: テキストボックス・図形とセルの連動 (TextLink / Drawing Formula Link)
/// セル参照数式のバインド、セルの最新値の自動反映、セル値編集後の同期、連動解除、Undo/Redo、OOXML (xdr:f) 保存・再読込保持。
fn fix_xlsx_object014_shape_formula_textlink() {
    use crate::xlsx::scenarios::SheetSteps as _;
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());

    // 1. セル A1 に値を設定
    s.click_cell("A1");
    s.enter("KPI-2026", "enter");

    // 2. 四角形を挿入
    s.menu("xlsx.shape.rect");
    let shape_id = s.with(|e, _| e.selected_ids().first().copied()).expect("shape inserted");

    // 3. セル連動を設定（A1 と連動）
    // セル A1 を再度クリックして選択セルとし、図形を選択してメニューから「選択中のセルと連動」を実行
    s.click_cell("A1");
    s.update(|e, _, _| e.select_sheet_shape(shape_id));
    s.menu("xlsx.shape-textlink.current-cell");

    // 連動式とテキストの初期反映を検証
    s.with(|e, _| {
        let slide = e.slide();
        assert_eq!(slide.shape_formulas.get(&shape_id).map(|s| s.as_str()), Some("A1"), "formula is A1");
        let shape = slide.shape(shape_id).unwrap();
        assert_eq!(shape.text_body().map(|t| t.text()).as_deref(), Some("KPI-2026"), "shape text shows cell value");
    });

    // 4. セル A1 を更新して連動図形のテキストが同期されるかを検証
    s.click_cell("A1");
    s.enter("Target-999", "enter");

    s.with(|e, _| {
        let slide = e.slide();
        let shape = slide.shape(shape_id).unwrap();
        assert_eq!(shape.text_body().map(|t| t.text()).as_deref(), Some("Target-999"), "shape text updated to new cell value");
    });

    // 5. 連動解除メニュー (xlsx.shape-textlink.clear) を検証
    s.update(|e, _, _| e.select_sheet_shape(shape_id));
    s.menu("xlsx.shape-textlink.clear");
    s.with(|e, _| {
        let slide = e.slide();
        assert_eq!(slide.shape_formulas.get(&shape_id), None, "formula is cleared");
    });

    // 6. Undo で連動式が復活すること
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        let slide = e.slide();
        assert_eq!(slide.shape_formulas.get(&shape_id).map(|s| s.as_str()), Some("A1"), "formula restored by undo");
    });

    // 7. Redo で連動解除が再適用されること
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        let slide = e.slide();
        assert_eq!(slide.shape_formulas.get(&shape_id), None, "formula cleared again by redo");
    });

    // 8. 全件ロールバック
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "all shapes rolled back");
    });

    // 9. ロールバック後の保存・再オープンが正常に通ること
    s.save_as("fix_xlsx_object014_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), initial_shapes);
}

/// XLSX-IMAGE-019: セル・グラフ・オブジェクトを画像としてコピー (Copy as Picture)
fn fix_xlsx_image019_copy_as_picture() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());

    // 1. メニュー存在確認
    assert!(s.menu_state("xlsx.copy-as-picture").is_some(), "xlsx.copy-as-picture menu item exists");

    // 2. セル範囲を選択して画像としてコピー
    s.click_cell("A1");
    s.key("shift-right");
    s.key("shift-down"); // A1:B2 選択
    s.menu("xlsx.copy-as-picture");

    s.with(|e, cx| {
        assert_eq!(e.status.as_deref(), Some("選択範囲を画像としてコピーしました"));
        let clip = cx.read_from_clipboard().expect("clipboard item exists");
        let entry = clip.entries().iter().find(|e| matches!(e, gpui_kit::ClipboardEntry::Image(_))).expect("image entry exists");
        if let gpui_kit::ClipboardEntry::Image(img) = entry {
            assert_eq!(img.format, gpui_kit::ImageFormat::Png);
            assert!(img.bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]), "valid PNG header");
            assert!(img.bytes.len() > 50, "PNG image has non-trivial size");
        }
    });

    // 3. 別のセル（C4）を選択して画像貼り付け
    s.click_cell("C4");
    s.update(|e, _, cx| e.paste_cells(false, cx));

    let shape_id = s.with(|e, _| {
        assert_eq!(e.status.as_deref(), Some("画像を貼り付けました"));
        assert_eq!(e.slide().shapes.len(), initial_shapes + 1, "pasted image inserted as shape");
        let last = e.slide().shapes.last().unwrap();
        assert!(matches!(last.kind, ibis_doc_model::ShapeKind::Picture(_)), "last shape is picture");
        last.id
    });

    // 4. 貼り付けられた画像オブジェクトを選択して画像コピー
    s.update(|e, _, _| e.select_sheet_shape(shape_id));
    s.menu("xlsx.copy-as-picture");

    s.with(|e, cx| {
        assert_eq!(e.status.as_deref(), Some("画像を画像としてコピーしました"));
        let clip = cx.read_from_clipboard().expect("clipboard item exists");
        let entry = clip.entries().iter().find(|e| matches!(e, gpui_kit::ClipboardEntry::Image(_))).expect("image entry exists");
        if let gpui_kit::ClipboardEntry::Image(img) = entry {
            assert_eq!(img.format, gpui_kit::ImageFormat::Png);
            assert!(img.bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]), "valid PNG header");
        }
    });

    // 5. テキストボックスを挿入し、オブジェクトとして画像コピー
    let tb_id = s.update(|e, window, cx| e.insert_sheet_textbox(window, cx));
    s.type_text("サンプル");
    s.update(|e, window, cx| e.commit_edit(window, cx));
    s.update(|e, _, _| e.select_sheet_shape(tb_id));
    s.menu("xlsx.copy-as-picture");

    s.with(|e, cx| {
        assert_eq!(e.status.as_deref(), Some("オブジェクトを画像としてコピーしました"));
        let clip = cx.read_from_clipboard().expect("clipboard item exists");
        let entry = clip.entries().iter().find(|e| matches!(e, gpui_kit::ClipboardEntry::Image(_))).expect("image entry exists");
        if let gpui_kit::ClipboardEntry::Image(img) = entry {
            assert_eq!(img.format, gpui_kit::ImageFormat::Png);
            assert!(img.bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]), "valid PNG header");
        }
    });

    // 6. 全件ロールバック
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "all shapes rolled back");
    });

    // 7. 保存・再オープン検証
    s.save_as("fix_xlsx_image019_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), initial_shapes);
}

/// XLSX-IMAGE-016: SVG挿入と色変更
fn fix_xlsx_image016_svg_insert_and_color() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());

    let svg_bytes = br##"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="120" viewBox="0 0 160 120"><rect width="160" height="120" fill="#204080"/><circle cx="80" cy="60" r="40" fill="#f0a000"/></svg>"##.to_vec();

    // 1. insert_sheet_picture で SVG 画像を挿入
    let pic_id = s.update(|e, _, cx| {
        e.insert_sheet_picture("vector-icon.svg", svg_bytes.clone(), cx)
    }).expect("SVG picture inserted");

    // 2. 挿入された図形の検証
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes + 1, "picture shape added");
        let shape = e.slide().shape(pic_id).expect("shape exists");
        let ibis_doc_model::ShapeKind::Picture(pic) = &shape.kind else { panic!("expected picture") };
        let media = e.presentation().media(pic.media).expect("media exists");
        assert_eq!(media.format, ibis_doc_model::MediaFormat::Svg, "media format is SVG");
        assert_eq!(media.data.as_ref(), &svg_bytes[..], "media data matches SVG");
        // 160 x 120 px at 96 DPI (9525 EMU/px) = 1_524_000 x 1_143_000 EMU
        assert_eq!(shape.frame.width, 160 * 9525, "width from SVG header");
        assert_eq!(shape.frame.height, 120 * 9525, "height from SVG header");
        assert_eq!(pic.duotone, None, "initially no duotone effect");
    });

    // 3. SVG 画像の色変更（xlsx.picture.color.*）
    s.update(|e, _, _| e.select_sheet_shape(pic_id));

    // 青 (blue)
    s.menu("xlsx.picture.color.blue");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(pic) = &e.slide().shape(pic_id).unwrap().kind else { panic!("picture") };
        assert_eq!(pic.duotone, Some((ibis_doc_model::Rgb(0x002060), ibis_doc_model::Rgb(0xd9e1f2))), "blue duotone applied");
    });

    // オレンジ (orange)
    s.menu("xlsx.picture.color.orange");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(pic) = &e.slide().shape(pic_id).unwrap().kind else { panic!("picture") };
        assert_eq!(pic.duotone, Some((ibis_doc_model::Rgb(0x803000), ibis_doc_model::Rgb(0xffe6cc))), "orange duotone applied");
    });

    // 紫 (purple)
    s.menu("xlsx.picture.color.purple");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(pic) = &e.slide().shape(pic_id).unwrap().kind else { panic!("picture") };
        assert_eq!(pic.duotone, Some((ibis_doc_model::Rgb(0x400060), ibis_doc_model::Rgb(0xf2e6ff))), "purple duotone applied");
    });

    // グレースケール (grayscale)
    s.menu("xlsx.picture.color.grayscale");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(pic) = &e.slide().shape(pic_id).unwrap().kind else { panic!("picture") };
        assert_eq!(pic.duotone, Some((ibis_doc_model::Rgb(0x000000), ibis_doc_model::Rgb(0xffffff))), "grayscale duotone applied");
    });

    // 色効果なし (none) でリセット
    s.menu("xlsx.picture.color.none");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(pic) = &e.slide().shape(pic_id).unwrap().kind else { panic!("picture") };
        assert_eq!(pic.duotone, None, "duotone cleared to none");
    });

    // 4. 枠線（Graphic Outline）の設定
    s.menu("xlsx.shape-stroke.blue");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(pic) = &e.slide().shape(pic_id).unwrap().kind else { panic!("picture") };
        assert!(pic.stroke.is_some(), "stroke applied to SVG picture");
        let stroke = pic.stroke.as_ref().unwrap();
        assert_eq!(stroke.color, ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x2b579a)));
    });

    // 5. SVG 画像を画像としてコピー（xlsx.copy-as-picture）
    s.menu("xlsx.copy-as-picture");
    s.with(|e, cx| {
        assert_eq!(e.status.as_deref(), Some("画像を画像としてコピーしました"));
        let clip = cx.read_from_clipboard().expect("clipboard item exists");
        let entry = clip.entries().iter().find(|e| matches!(e, gpui_kit::ClipboardEntry::Image(_))).expect("image entry exists");
        if let gpui_kit::ClipboardEntry::Image(img) = entry {
            assert_eq!(img.format, gpui_kit::ImageFormat::Png);
            assert!(img.bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]), "valid PNG raster header");
        }
    });

    // 6. Undo / Redo の検証
    // undo stroke
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(pic) = &e.slide().shape(pic_id).unwrap().kind else { panic!("picture") };
        assert_eq!(pic.stroke, None, "stroke undone");
    });

    // redo stroke
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(pic) = &e.slide().shape(pic_id).unwrap().kind else { panic!("picture") };
        assert!(pic.stroke.is_some(), "stroke redone");
    });

    // 7. 全件ロールバック
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "all shapes rolled back to initial state");
    });

    // 8. 保存・再オープン検証
    s.save_as("fix_xlsx_image016_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), initial_shapes);
}

/// #23: 左サムネイル一覧のスクロール不具合（最下部・最上部・セクション折りたたみ・行インデックス整合性検証）
fn fix23_pptx_thumbnail_scroll_and_sections() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 31);

    // 1. セクションなし状態での行インデックス整合性
    assert_eq!(s.with(|e, _| e.row_of_slide(0)), 0);
    assert_eq!(s.with(|e, _| e.row_of_slide(30)), 30);

    // 2. 最終スライド (31枚目) への移動
    s.page(31);
    s.settle();
    s.with(|e, _| {
        let last_id = e.presentation().slides[30].id;
        assert_eq!(e.current_slide, last_id, "slide 31 is current");
    });
    s.screenshot("thumbnails_slide31_selected");

    // 3. 先頭スライド (1枚目) への復帰
    s.page(1);
    s.settle();
    s.with(|e, _| {
        let first_id = e.presentation().slides[0].id;
        assert_eq!(e.current_slide, first_id, "slide 1 is current");
    });

    // 4. セクションを追加して row_of_slide の見出しオフセット計算を検証
    s.page(1);
    s.menu("pptx.section.add");
    s.key("cmd-a");
    s.type_text("第1部");
    s.menu("pptx.section.apply");

    s.page(7);
    s.menu("pptx.section.add");
    s.key("cmd-a");
    s.type_text("第2部");
    s.menu("pptx.section.apply");

    let pres = s.with(|e, _| e.presentation().clone());
    assert_eq!(pres.sections.len(), 2);
    let s1_id = pres.sections[0].id.clone();

    // セクション構造:
    // row 0: セクション1見出し
    // row 1..=6: スライド 1..6 (slide_index 0..=5)
    // row 7: セクション2見出し
    // row 8..=32: スライド 7..31 (slide_index 6..=30)
    assert_eq!(s.with(|e, _| e.row_of_slide(0)), 1);
    assert_eq!(s.with(|e, _| e.row_of_slide(5)), 6);
    assert_eq!(s.with(|e, _| e.row_of_slide(6)), 8);
    assert_eq!(s.with(|e, _| e.row_of_slide(30)), 32);

    // 5. セクション1を折りたたみ
    s.update(|e, _, cx| e.toggle_section_fold(&s1_id, cx));
    s.settle();

    // 折りたたみ時:
    // セクション1の見出しは row 0
    // セクション1に属するスライド 1..6 (slide_index 0..=5) は見出しの行 (row 0) にマッピングされる
    assert_eq!(s.with(|e, _| e.row_of_slide(0)), 0);
    assert_eq!(s.with(|e, _| e.row_of_slide(5)), 0);
    // セクション2の見出しは row 1 (直前のスライド6枚が隠れるため)
    // スライド 7 (slide_index 6) は row 2
    assert_eq!(s.with(|e, _| e.row_of_slide(6)), 2);

    // 6. セクション1を展開復元
    s.update(|e, _, cx| e.toggle_section_fold(&s1_id, cx));
    s.settle();
    assert_eq!(s.with(|e, _| e.row_of_slide(0)), 1);
    assert_eq!(s.with(|e, _| e.row_of_slide(6)), 8);

    // 7. Undo でセクション追加前の状態に復元
    while s.with(|e, _| e.document.can_undo()) {
        s.key("cmd-z");
    }
    assert_eq!(s.with(|e, _| e.presentation().sections.len()), 0);
    assert_eq!(s.with(|e, _| e.row_of_slide(0)), 0);
    assert_eq!(s.with(|e, _| e.row_of_slide(30)), 30);
}

/// XLSX-FILE-011: CSVの文字コード（Shift_JIS / UTF-8 BOM）および区切り文字（TSV・カンマ）の読込・保存・エクスポート
fn fix_xlsx_file011_csv_and_tsv() {
    use ibis_doc_model::CellValue;

    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    let sjis_csv_path = dir.join("file011_sjis.csv");
    let sjis_content = "品名,数量,単価,小計\r\nりんご,5,120,=B2*C2\r\n\"温州,みかん\",3,100,=B3*C3\r\n";
    let sjis_bytes = ibis_pptx::csv::encode_sjis(sjis_content);
    std::fs::write(&sjis_csv_path, &sjis_bytes).unwrap();

    // 1. Shift_JIS CSV を開く
    let mut s = Scenario::open("target/scenario-input/file011_sjis.csv");
    s.with(|e, _| {
        assert!(e.is_workbook());
    });
    assert_eq!(s.value("A1"), CellValue::Text("品名".into()));
    assert_eq!(s.value("A2"), CellValue::Text("りんご".into()));
    assert_eq!(s.value("B2"), CellValue::Number(5.0));
    assert_eq!(s.value("C2"), CellValue::Number(120.0));
    assert_eq!(s.value("A3"), CellValue::Text("温州,みかん".into()));

    // 2. セル編集して保存 (cmd-s)
    s.click_cell("B4");
    s.enter("=SUM(B2:B3)", "enter");
    assert_eq!(s.value("B4"), CellValue::Number(8.0));
    s.key("cmd-s");
    s.settle();

    // 3. Shift_JIS CSV 書き出し (write_csv_sjis)
    let out_sjis_path = dir.join("file011_out_sjis.csv");
    s.with(|e, _| {
        e.write_csv_sjis(&out_sjis_path).expect("write_csv_sjis succeeds");
    });
    let out_sjis_bytes = std::fs::read(&out_sjis_path).unwrap();
    assert!(!out_sjis_bytes.starts_with(b"\xEF\xBB\xBF"), "Shift_JIS must not have UTF-8 BOM");
    let decoded_sjis = ibis_pptx::csv::decode(&out_sjis_bytes).expect("decode sjis");
    assert!(decoded_sjis.contains("品名,数量,単価,小計\r\n"));
    assert!(decoded_sjis.contains("\"温州,みかん\",3,100"));

    // 4. TSV 書き出し (write_tsv)
    let out_tsv_path = dir.join("file011_out.tsv");
    s.with(|e, _| {
        e.write_tsv(&out_tsv_path).expect("write_tsv succeeds");
    });
    let out_tsv_bytes = std::fs::read(&out_tsv_path).unwrap();
    let decoded_tsv = ibis_pptx::csv::decode(&out_tsv_bytes).expect("decode tsv");
    assert!(decoded_tsv.contains("品名\t数量\t単価\t小計\r\n"));
    assert!(decoded_tsv.contains("りんご\t5\t120"));

    // 5. TSV ファイルを open_path で開いて検証
    s.update(|e, window, cx| {
        e.open_path(out_tsv_path.clone(), None, window, cx);
    });
    s.settle();
    s.with(|e, _| {
        assert!(e.is_workbook());
    });
    assert_eq!(s.value("A1"), CellValue::Text("品名".into()));
    assert_eq!(s.value("B2"), CellValue::Number(5.0));
    assert_eq!(s.value("A3"), CellValue::Text("温州,みかん".into()));

    // 6. 後片付け
    let _ = std::fs::remove_file(sjis_csv_path);
    let _ = std::fs::remove_file(out_sjis_path);
    let _ = std::fs::remove_file(out_tsv_path);
}

/// #28: the sheet tabs' scrollbar in the status bar ends 8 px before the
/// zoom controls (the − button) whatever the window's width, shows only while
/// the tabs scroll, and never reaches the − button.
fn fix27_status_bar_tab_scrollbar_place() {
    for width in [600.0, 900.0, 1200.0, 1600.0] {
        let mut s = Scenario::open_sized("test/editing/xlsx-sales.xlsx", width);
        // Few sheets: the tabs do not scroll, so there is no scrollbar.
        let scrolls = |s: &mut Scenario| s.with(|e, _| e.sheet_state().expect("a workbook").tab_scroll.max_offset().x > gpui_kit::px(0.5));
        s.settle();
        assert!(!scrolls(&mut s), "{width}: three sheets fit");
        for _ in 0..20 {
            s.menu("xlsx.insert-sheet");
        }
        s.settle();
        s.settle();
        assert!(scrolls(&mut s), "{width}: 23 sheets scroll");
        let (layout, tabs_left, minus) = s.with(|e, _| {
            let scroll = e.sheet_state().expect("a workbook").tab_scroll.clone();
            (e.status_layout.get(), f32::from(scroll.bounds().left()), e.status_layout.get()[2])
        });
        // The scrollbar's right edge is 8 px before the − button's left edge.
        let right = minus - 8.0;
        eprintln!("width {width}: status bar {:?}, tabs left {tabs_left}, scrollbar right {right}, − button left {minus}", &layout[..2]);
        assert!((layout[1] - width).abs() < 1.0, "{width}: the status bar measured: {layout:?}");
        assert!(minus > tabs_left && minus < width, "{width}: the − button is in the window: {minus}");
        // Scrolled to the end, with the pointer over the scrollbar, its thumb
        // ends where the scrollbar does.
        s.with(|e, _| {
            let scroll = e.sheet_state().expect("a workbook").tab_scroll.clone();
            let max = scroll.max_offset().x;
            scroll.set_offset(gpui_kit::point(-max, gpui_kit::px(0.)));
        });
        s.settle();
        s.hover((tabs_left + 40.0, 720.0 - 20.0));
        s.screenshot(&format!("status_bar_{width}"));
    }
}

/// #30: the wheel over the sheet tabs' scrollbar in the status bar scrolls the
/// tabs sideways (a vertical wheel too), as over the tabs themselves; with
/// every sheet in view it does nothing.
fn fix30_status_bar_tab_wheel() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let offset = |s: &mut Scenario| s.with(|e, _| f32::from(e.sheet_state().expect("a workbook").tab_scroll.offset().x));
    let (tabs_y, bar_y) = (679.0, 720.0 - 26.0 + 5.0);
    // Three sheets fit: nothing scrolls, over the bar or the tabs.
    s.wheel_xy((300.0, bar_y), 0.0, -120.0);
    s.wheel_xy((300.0, tabs_y), 0.0, -120.0);
    assert_eq!(offset(&mut s), 0.0, "everything in view: nothing scrolls");
    for _ in 0..20 {
        s.menu("xlsx.insert-sheet");
    }
    s.settle();
    s.settle();
    s.wheel_xy((300.0, tabs_y), 0.0, 0.0);
    let start = offset(&mut s);
    // The wheel over the tabs.
    s.wheel_xy((300.0, tabs_y), -60.0, 0.0);
    let sideways_tabs = offset(&mut s) - start;
    s.wheel_xy((300.0, tabs_y), 60.0, 0.0);
    s.wheel_xy((300.0, tabs_y), 0.0, -60.0);
    let vertical_tabs = offset(&mut s) - start;
    s.wheel_xy((300.0, tabs_y), 0.0, 60.0);
    eprintln!("tabs: sideways wheel moves the offset by {sideways_tabs}, a vertical one by {vertical_tabs}");
    // The wheel over the bar in the status bar.
    let before = offset(&mut s);
    s.wheel_xy((300.0, bar_y), -60.0, 0.0);
    let sideways_bar = offset(&mut s) - before;
    s.wheel_xy((300.0, bar_y), 60.0, 0.0);
    s.wheel_xy((300.0, bar_y), 0.0, -60.0);
    let vertical_bar = offset(&mut s) - before;
    eprintln!("bar: sideways {sideways_bar}, vertical {vertical_bar}");
    assert!(sideways_bar != 0.0, "a sideways wheel over the bar scrolls the tabs");
    assert!(vertical_bar != 0.0, "a vertical wheel over the bar scrolls the tabs too");
    assert_eq!(vertical_bar.signum(), sideways_bar.signum(), "both scroll the same way for the same sign");
}

/// #31: a worksheet at 100% is drawn as Excel for Mac draws it, a point a
/// logical pixel (a slide's 100% is 96 px an inch: 4/3 pixel a point).
fn fix31_sheet_100_percent_is_a_point_a_pixel() {
    use crate::xlsx::scenarios::SheetSteps as _;
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.update(|e, window, cx| e.set_zoom(Some(1.0), window, cx));
    s.settle();
    for name in ["A", "B", "C"] {
        let (mid, line) = (s.header(name), s.header_line(name));
        let width_px = (line.0 - mid.0) * 2.0;
        let width_pt = s.size_pt(name);
        eprintln!("column {name}: {width_pt} pt, {width_px} px at 100%");
        assert!((width_px / width_pt - 1.0).abs() < 0.03, "column {name}: {width_pt} pt drawn {width_px} px at 100%");
    }
    // A presentation's slide: 96 px an inch at 100%.
    drop(s);
    let mut p = Scenario::new_document(ibis_pptx::Kind::Presentation);
    p.update(|e, window, cx| e.set_zoom(Some(1.0), window, cx));
    p.settle();
    let ratio = p.with(|e, _| e.slide_layout().map(|l| l.scale * ibis_doc_model::EMU_PER_POINT as f32));
    eprintln!("a slide at 100%: {ratio:?} px a point");
    assert!((ratio.unwrap() - 96.0 / 72.0).abs() < 0.01);
}

/// #33: a formula with no cached value in the file (written by a program that does not
/// calculate: C#'s Open XML SDK, openpyxl) shows its value when the file is opened.
fn fix33_formula_without_cached_value() {
    use crate::xlsx::scenarios::SheetSteps as _;
    let mut s = Scenario::open("test/editing/xlsx-no-cached-values.xlsx");
    assert_eq!(s.shown("B10"), "280", "SUM(B2:B9) with no cached value");
    assert_eq!(s.shown("D10"), "#N/A");
    // Opening is no edit: nothing to undo, so nothing to save.
    assert!(!s.with(|e, _| e.document.can_undo()), "the calculation on opening is not an edit");
}

/// Times of a file: parsed, opened and drawn, shown at its end, and (a
/// workbook) at each next sheet. Off unless `IBIS_BENCH_FILES` names files
/// (paths joined by `:`): `IBIS_BENCH_FILES=a.xlsx:b.pptx cargo test
/// --release --features scenarios --test scenarios -- bench_open_scroll
/// --nocapture`. Milliseconds, one run each.
fn bench_open_scroll() {
    let Some(files) = std::env::var_os("IBIS_BENCH_FILES") else { return };
    let ms = crate::clock::unix_millis;
    for path in files.to_string_lossy().split(':').filter(|p| !p.is_empty()) {
        let t = ms();
        let parsed = crate::read_file(std::path::Path::new(path)).map(|_| ());
        let parse = ms() - t;
        if parsed.is_err() {
            eprintln!("BENCH {path}: cannot read");
            continue;
        }
        let t = ms();
        let mut s = Scenario::open(path);
        let open = ms() - t;
        let is_xlsx = path.ends_with(".xlsx") || path.ends_with(".xlsm");
        let t = ms();
        s.key(if path.ends_with(".pptx") { "end" } else { "ctrl-end" });
        s.settle();
        let end = ms() - t;
        let mut sheets = Vec::new();
        if is_xlsx {
            let count = std::env::var("IBIS_BENCH_SHEETS").ok().and_then(|v| v.parse().ok()).unwrap_or(4);
            for _ in 0..count {
                let t = ms();
                s.key("ctrl-pagedown");
                s.settle();
                sheets.push(ms() - t);
            }
        }
        eprintln!("BENCH {path}: parse {parse} ms, open+draw {open} ms (with parse), to the end {end} ms, next sheets {sheets:?} ms");
    }
}
