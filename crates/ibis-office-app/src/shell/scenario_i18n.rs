// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of the UI's language (#8): ツール > 設定… opens the settings
//! window; choosing English there turns the menus English at once; the
//! choice is kept and read again at the next start.
use crate::shell::chrome::MenuId;
use crate::shell::i18n::{self, Language, LanguageSetting};
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("shell::i8_language_switch_and_kept", i8_language_switch_and_kept), ("shell::i8_panels_and_status_in_english", i8_panels_and_status_in_english), ("shell::fix16_font_names_in_english", fix16_font_names_in_english), ("shell::i8_docx_panels_in_english", i8_docx_panels_in_english)]
}

/// The label of the menu item `id` and of the ファイル menu, as shown now.
fn labels(s: &mut Scenario, id: &str) -> (String, String) {
    let id = id.to_string();
    s.with(move |e, _| {
        let mut menus = e.menus();
        (menus.find_mut(&id).map(|i| i.label.to_string()).unwrap_or_default(), MenuId::File.label().to_string())
    })
}

fn i8_language_switch_and_kept() {
    let mut s = Scenario::new_document(ibis_pptx::Kind::Workbook);
    // The settings are kept in the scenario's folder, not the user's.
    let dir = crate::shell::scenario::repo_root().join("target/scenario/shell::i8_language_switch_and_kept/settings");
    let _ = std::fs::remove_dir_all(&dir);
    // SAFETY: the scenarios run one at a time on the main thread; nothing
    // else reads the environment meanwhile.
    unsafe { std::env::set_var("IBIS_SETTINGS_DIR", &dir) };
    assert_eq!(labels(&mut s, "file.save"), ("保存".into(), "ファイル".into()));
    assert_eq!(labels(&mut s, "xlsx.remove-duplicates").0, "重複の削除…");
    // ツール > 設定… opens the settings window.
    s.menu("tools.settings");
    assert!(crate::shell::settings_window::is_open());
    // English, at once.
    s.update(|_, _, cx| crate::shell::settings_window::choose_language(LanguageSetting::Fixed(Language::English), cx));
    assert_eq!(labels(&mut s, "file.save"), ("Save".into(), "File".into()));
    assert_eq!(labels(&mut s, "tools.settings").0, "Settings…");
    assert_eq!(labels(&mut s, "xlsx.remove-duplicates").0, "Remove Duplicates…");
    s.screenshot("english-menus");
    // Kept: read again as at the next start.
    i18n::forget();
    assert_eq!(i18n::setting(), LanguageSetting::Fixed(Language::English));
    assert_eq!(i18n::language(), Language::English);
    assert!(std::fs::read_to_string(dir.join("settings.txt")).unwrap_or_default().contains("language=en"));
    // Back to Japanese; the system's language when chosen so.
    s.update(|_, _, cx| crate::shell::settings_window::choose_language(LanguageSetting::Fixed(Language::Japanese), cx));
    assert_eq!(labels(&mut s, "file.save").0, "保存");
    s.update(|_, _, cx| crate::shell::settings_window::choose_language(LanguageSetting::System, cx));
    assert_eq!(i18n::language(), i18n::system_language());
    unsafe { std::env::remove_var("IBIS_SETTINGS_DIR") };
    i18n::show(Language::Japanese);
}

/// In English, the status bar (シート → Sheet), a panel's title and its
/// buttons, the toolbar's tooltips: drawn from the tables by their
/// Japanese text (a picture to look at).
fn i8_panels_and_status_in_english() {
    let mut s = Scenario::new_document(ibis_pptx::Kind::Workbook);
    i18n::show(Language::English);
    s.update(|_, _, _| {});
    let status = s.status_texts();
    assert!(status[0].starts_with("Sheet 1"), "{status:?}");
    s.menu("xlsx.remove-duplicates");
    s.screenshot("english-panel");
    i18n::show(Language::Japanese);
    s.update(|_, _, _| {});
    assert!(s.status_texts()[0].starts_with("シート 1"));
}

/// #16: the English UI's font lists show the English name of a font that has
/// one (a picture: the font box of a new document), the Japanese UI and the
/// document keep the Japanese name.
fn fix16_font_names_in_english() {
    let mut s = Scenario::new_document(ibis_pptx::Kind::Document);
    i18n::show(Language::English);
    assert_eq!(i18n::font_label("游明朝"), "Yu Mincho");
    assert_eq!(i18n::font_label("ＭＳ ゴシック"), "MS Gothic");
    assert_eq!(i18n::font_label("Arial"), "Arial");
    s.update(|_, _, _| {});
    s.screenshot("english-font-box");
    i18n::show(Language::Japanese);
    assert_eq!(i18n::font_label("游明朝"), "游明朝");
    assert_eq!(i18n::font_label("BIZ UDPゴシック"), "BIZ UDPゴシック");
}

/// A document's page setup panel and watermark panel in English (pictures to
/// look at): their labels are drawn from the tables by id.
fn i8_docx_panels_in_english() {
    let mut s = Scenario::new_document(ibis_pptx::Kind::Document);
    i18n::show(Language::English);
    s.update(|e, window, cx| e.open_page_setup_panel(window, cx));
    s.screenshot("english-page-setup");
    s.update(|e, window, cx| e.open_watermark_panel(window, cx));
    s.screenshot("english-watermark");
    i18n::show(Language::Japanese);
}
