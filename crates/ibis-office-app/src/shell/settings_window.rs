// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ツール > 設定… (⌘,): the settings window, master–detail as the system's
//! settings are: the categories on the left (一般 for now), the selected
//! one's settings on the right. 一般 > 言語 chooses the UI's language (the
//! system's, English or Japanese); every window shows it at once. 外部エディター
//! lists the editors of embedded objects registered (their manifests' folder).
use gpui_kit::component::button::Button;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Root, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::shell::i18n::{self, Language, LanguageSetting};

pub(crate) struct SettingsWindow {
    /// The category shown (0: 一般).
    category: usize,
    focus: FocusHandle,
}

thread_local! {
    /// The settings window, while it is open (one at a time).
    static OPEN: std::cell::RefCell<Option<WindowHandle<Root>>> = const { std::cell::RefCell::new(None) };
}

/// Open the settings window, or bring it to the front.
pub(crate) fn open(cx: &mut App) {
    let parent = cx.active_window().and_then(|w| w.update(cx, |_, window, cx| (window.bounds(), window.display(cx).map(|d| d.id()))).ok());
    open_over(parent.map(|p| p.0), parent.and_then(|p| p.1), cx);
}

/// Open the settings window in the middle of `parent` (the window the user
/// is in, on the same display: a window's bounds are relative to its display,
/// so the display is given too), or bring it to the front.
pub(crate) fn open_over(parent: Option<Bounds<Pixels>>, display: Option<DisplayId>, cx: &mut App) {
    if let Some(handle) = OPEN.with(|o| *o.borrow())
        && handle.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        return;
    }
    // In the middle of the window the user is in.
    let (width, height) = (px(640.), px(420.));
    let bounds = match parent {
        Some(p) => Bounds::new(point(p.origin.x + (p.size.width - width) / 2.0, p.origin.y + (p.size.height - height) / 2.0), size(width, height)),
        None => Bounds::centered(None, size(width, height), cx),
    };
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        display_id: display,
        titlebar: Some(TitlebarOptions { title: Some(i18n::text("settings.title")), ..Default::default() }),
        ..Default::default()
    };
    let opened = cx.open_window(options, |window, cx| {
        let view = cx.new(|cx| SettingsWindow { category: 0, focus: cx.focus_handle() });
        let focus = view.read(cx).focus.clone();
        focus.focus(window, cx);
        cx.new(|cx| Root::new(view, window, cx))
    });
    if let Ok(handle) = opened {
        OPEN.with(|o| *o.borrow_mut() = Some(handle));
    }
}

/// The settings window, while it is open.
#[allow(dead_code)]
pub(crate) fn handle() -> Option<WindowHandle<Root>> {
    OPEN.with(|o| *o.borrow())
}

/// Whether the settings window is open.
#[allow(dead_code)]
pub(crate) fn is_open() -> bool {
    OPEN.with(|o| o.borrow().is_some())
}

/// Choose the UI's language: saved, and every window drawn again in it (the
/// app's own menu bar too).
pub(crate) fn choose_language(setting: LanguageSetting, cx: &mut App) {
    i18n::set_setting(setting);
    crate::set_app_menus(cx);
    cx.refresh_windows();
}

/// The name of a choice of language, in the UI's language.
fn choice_label(setting: LanguageSetting) -> SharedString {
    match setting {
        LanguageSetting::System => i18n::text("settings.language.system"),
        // A language's own name, as systems list them.
        LanguageSetting::Fixed(Language::English) => "English".into(),
        LanguageSetting::Fixed(Language::Japanese) => "日本語".into(), // i18n-ok（言語の自称（母語での表記）。English と並べて自称のまま。迷った：母語表記）
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title(&i18n::text("settings.title"));
        #[cfg(not(target_family = "wasm"))]
        let editors = (self.category == 1).then(|| editors_detail(cx));
        let theme = cx.theme();
        #[cfg(not(target_family = "wasm"))]
        let categories = [i18n::text("settings.general"), i18n::text("settings.editors")];
        #[cfg(target_family = "wasm")]
        let categories = [i18n::text("settings.general")];
        let current = i18n::setting();
        let list = v_flex().w(px(180.)).h_full().p_2().gap_1().justify_start().items_stretch().bg(theme.sidebar).border_r_1().border_color(theme.border).children(categories.iter().enumerate().map(|(i, name)| {
            let selected = i == self.category;
            div()
                .id(("settings-category", i))
                .px_2()
                .py_1()
                .rounded_md()
                .when(selected, |d| d.bg(theme.accent))
                .child(name.clone())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.category = i;
                    cx.notify();
                }))
        }));
        let choices = [LanguageSetting::System, LanguageSetting::Fixed(Language::English), LanguageSetting::Fixed(Language::Japanese)];
        let language = h_flex()
            .gap_3()
            .items_center()
            .child(div().w(px(80.)).child(i18n::text("settings.language")))
            .child(Button::new("settings-language").small().label(choice_label(current)).dropdown_menu(move |menu, _, _| {
                choices.iter().fold(menu, |menu, &choice| {
                    menu.item(PopupMenuItem::new(choice_label(choice)).checked(choice == current).on_click(move |_, _, cx| choose_language(choice, cx)))
                })
            }));
        let detail = v_flex()
            .flex_1()
            .p_4()
            .gap_3()
            .child(div().text_lg().child(categories[self.category.min(categories.len() - 1)].clone()))
            .child(language)
            .child(div().text_sm().text_color(theme.muted_foreground).child(i18n::text("settings.language.note")));
        // 起動時に更新を確かめる (no update in the browser).
        #[cfg(not(target_family = "wasm"))]
        let detail = detail
            .child(
                div().mt_3().child(
                    Checkbox::new("settings-check-updates")
                        .label(i18n::text("fix.update.setting"))
                        .checked(crate::shell::update::check_enabled())
                        .on_click(cx.listener(|_, checked: &bool, _, cx| {
                            crate::shell::update::set_check_enabled(*checked);
                            cx.notify();
                        })),
                ),
            )
            .child(div().text_sm().text_color(theme.muted_foreground).child(i18n::text("fix.update.setting.note")));
        #[cfg(not(target_family = "wasm"))]
        let detail = editors.unwrap_or(detail);
        h_flex().id("settings").track_focus(&self.focus).size_full().items_start().bg(theme.background).text_color(theme.foreground).child(list).child(detail)
    }
}

/// 外部エディター: the editors registered, the manifests that could not be read,
/// and their folder.
#[cfg(not(target_family = "wasm"))]
fn editors_detail(cx: &mut Context<SettingsWindow>) -> Div {
    let (muted, danger) = (cx.theme().muted_foreground, cx.theme().danger);
    let reload = cx.listener(|_, _: &ClickEvent, _, cx| {
        crate::shell::embed::reload_handlers();
        cx.notify();
    });
    let dir = crate::shell::embed::handlers_dir();
    let read = dir.as_deref().map(ibis_embed_protocol::load_manifest_dir).unwrap_or_default();
    let english = i18n::language() == Language::English;
    let rows = read.into_iter().map(|(path, manifest)| match manifest {
        Ok(m) => v_flex()
            .gap_0p5()
            .child(div().child(if english { m.name.en.clone() } else { m.name.ja.clone() }))
            .child(div().text_sm().text_color(muted).child(format!("{} — {}", m.prog_ids.join(", "), m.command.display()))),
        Err(e) => v_flex().gap_0p5().child(div().text_color(danger).child(path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())).child(div().text_sm().text_color(muted).child(e.to_string())),
    });
    let rows: Vec<Div> = rows.collect();
    let empty = rows.is_empty();
    let folder = dir.clone();
    v_flex()
        .flex_1()
        .p_4()
        .gap_3()
        .child(div().text_lg().child(i18n::text("settings.editors")))
        .child(div().text_sm().text_color(muted).child(i18n::format("settings.editors.note", &[("dir", &dir.as_ref().map(|d| d.display().to_string()).unwrap_or_default())])))
        .when(empty, |d| d.child(div().text_color(muted).child(i18n::text("settings.editors.none"))))
        .children(rows)
        .child(
            h_flex()
                .gap_2()
                .child(Button::new("settings-editors-folder").small().label(i18n::text("settings.editors.open-folder")).on_click(move |_, _, cx| {
                    if let Some(dir) = &folder {
                        if let Err(e) = std::fs::create_dir_all(dir) {
                            log::warn!("{}: {e}", dir.display());
                        }
                        cx.open_with_system(dir);
                    }
                }))
                .child(Button::new("settings-editors-reload").small().label(i18n::text("settings.editors.reload")).on_click(reload)),
        )
}
