// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 自動回復の設定 (PPTX-FILE-012), 回復したプレゼンテーションの保存・未保存回復 (PPTX-FILE-010).
//!
//! Provides AutoRecover configuration, automatic background snapshot creation,
//! restoration of unsaved presentations, and a recovery banner with Save As workflow.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;
use crate::shell::icons::Material;
pub(crate) use crate::xlsx::autorecover::{
    default_autorecover_dir, effective_autorecover_dir, format_file_size, format_timestamp_secs,
    load_autorecover_config, save_autorecover_config, AutoRecoverConfig, RecoveredFileEntry,
};

/// Scan the AutoRecover folder for `.pptx` recovery files.
pub(crate) fn scan_recovered_pptx_files(dir: &Path) -> Vec<RecoveredFileEntry> {
    let mut entries = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else { return entries };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("pptx")) {
            let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let metadata = entry.metadata().ok();
            let size_bytes = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
            let size_str = format_file_size(size_bytes);
            let modified_str = metadata
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| format_timestamp_secs(d.as_secs()))
                .unwrap_or_else(|| "-".to_string());
            entries.push(RecoveredFileEntry {
                path,
                file_name,
                modified_str,
                size_bytes,
                size_str,
            });
        }
    }
    entries.sort_by(|a, b| b.path.cmp(&a.path));
    entries
}

/// AutoRecover Settings sidebar panel state (PPTX-FILE-012).
pub(crate) struct AutoRecoverSettingsPanel {
    pub(crate) enabled: bool,
    pub(crate) interval_input: Entity<InputState>,
    pub(crate) keep_last_unsaved: bool,
    pub(crate) location_input: Entity<InputState>,
    pub(crate) disable_for_this_presentation: bool,
    pub(crate) message: String,
    #[allow(dead_code)]
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

/// Recover Unsaved Presentations sidebar panel state (PPTX-FILE-010).
pub(crate) struct RecoverUnsavedPanel {
    pub(crate) entries: Vec<RecoveredFileEntry>,
    #[allow(dead_code)]
    pub(crate) selected_index: Option<usize>,
    pub(crate) message: String,
    #[allow(dead_code)]
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    // ------------------------------------------------------------------------
    // AutoRecover Settings Panel Methods (PPTX-FILE-012)
    // ------------------------------------------------------------------------

    /// ツール > 自動回復の設定…: Open the AutoRecover Settings panel in the sidebar.
    pub(crate) fn open_pptx_autorecover_settings_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let config = load_autorecover_config();
        let interval_val = config.interval_minutes.to_string();
        let loc_val = config
            .custom_location
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| default_autorecover_dir().to_string_lossy().into_owned());

        let interval_input = cx.new(|cx| InputState::new(window, cx).default_value(interval_val));
        let location_input = cx.new(|cx| InputState::new(window, cx).default_value(loc_val));

        let disable_pres = self.pptx().map_or(false, |s| s.disable_autorecover_for_presentation);

        let panel = AutoRecoverSettingsPanel {
            enabled: config.enabled,
            interval_input,
            keep_last_unsaved: config.keep_last_unsaved,
            location_input,
            disable_for_this_presentation: disable_pres,
            message: String::new(),
            ui: Rc::new(RefCell::new(BTreeMap::new())),
        };

        if let Some(s) = self.pptx_mut() {
            s.autorecover_settings = Some(panel);
            s.recover_unsaved = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Close the AutoRecover Settings panel.
    pub(crate) fn close_pptx_autorecover_settings_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.autorecover_settings = None;
        }
        cx.notify();
    }

    /// Save the edited settings to configuration and editor state.
    pub(crate) fn save_pptx_autorecover_settings(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.pptx().and_then(|s| s.autorecover_settings.as_ref()) else { return };
        let interval_str = panel.interval_input.read(cx).value().trim().to_string();
        let loc_str = panel.location_input.read(cx).value().trim().to_string();
        let enabled = panel.enabled;
        let keep_last_unsaved = panel.keep_last_unsaved;
        let disable_pres = panel.disable_for_this_presentation;

        let interval_minutes = match interval_str.parse::<u32>() {
            Ok(val) if (1..=120).contains(&val) => val,
            _ => {
                if let Some(panel) = self.pptx_mut().and_then(|s| s.autorecover_settings.as_mut()) {
                    panel.message = text("pptx.autorecover.error-interval").to_string();
                }
                cx.notify();
                return;
            }
        };

        let custom_location = if loc_str.is_empty() || loc_str == default_autorecover_dir().to_string_lossy() {
            None
        } else {
            let path = PathBuf::from(&loc_str);
            if let Err(e) = std::fs::create_dir_all(&path) {
                if let Some(panel) = self.pptx_mut().and_then(|s| s.autorecover_settings.as_mut()) {
                    panel.message = format!("{}: {e}", text("pptx.autorecover.error-path"));
                }
                cx.notify();
                return;
            }
            Some(path)
        };

        let new_config = AutoRecoverConfig {
            enabled,
            interval_minutes,
            keep_last_unsaved,
            custom_location,
        };
        save_autorecover_config(&new_config);

        if let Some(s) = self.pptx_mut() {
            s.disable_autorecover_for_presentation = disable_pres;
            if let Some(panel) = s.autorecover_settings.as_mut() {
                panel.message = text("pptx.autorecover.saved-msg").to_string();
            }
        }
        cx.notify();
    }

    /// Reset custom location input to default directory.
    pub(crate) fn reset_pptx_autorecover_location_to_default(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let default_loc = default_autorecover_dir().to_string_lossy().into_owned();
        let Some(panel) = self.pptx().and_then(|s| s.autorecover_settings.as_ref()) else { return };
        panel.location_input.update(cx, |input, cx| {
            input.set_value(default_loc, window, cx);
        });
        cx.notify();
    }

    /// Manual trigger: Save an AutoRecover snapshot immediately from the panel.
    pub(crate) fn save_pptx_autorecover_now(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        match self.save_pptx_autorecover_snapshot() {
            Ok(path) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if let Some(panel) = self.pptx_mut().and_then(|s| s.autorecover_settings.as_mut()) {
                    panel.message = format!("{} ({name})", text("pptx.autorecover.now-done"));
                }
            }
            Err(err) => {
                if let Some(panel) = self.pptx_mut().and_then(|s| s.autorecover_settings.as_mut()) {
                    panel.message = format!("{}: {err}", text("pptx.autorecover.now-failed"));
                }
            }
        }
        #[cfg(target_family = "wasm")]
        {
            if let Some(panel) = self.pptx_mut().and_then(|s| s.autorecover_settings.as_mut()) {
                panel.message = text("pptx.autorecover.wasm-save-unsupported").to_string();
            }
        }
        cx.notify();
    }

    // ------------------------------------------------------------------------
    // AutoRecover Engine Snapshot & Cleanup Methods
    // ------------------------------------------------------------------------

    /// Save an AutoRecover snapshot of the current presentation to disk.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn save_pptx_autorecover_snapshot(&mut self) -> Result<PathBuf, String> {
        let (stem, _) = self.name_parts();
        let config = load_autorecover_config();
        let dir = effective_autorecover_dir(&config);
        let _ = std::fs::create_dir_all(&dir);
        let now = crate::clock::unix_millis();
        let target_path = crate::xlsx::autorecover::autorecover_snapshot_path(&dir, &stem, self.document_id, "pptx", self.active_autorecover_file.as_deref());

        self.copy_to(&target_path)?;

        self.active_autorecover_file = Some(target_path.clone());
        if let Some(s) = self.pptx_mut() {
            s.last_autorecover_at = Some(now);
        }
        Ok(target_path)
    }

    /// On close without saving, preserve the recovery file as Unsaved_... if configured.
    pub(crate) fn preserve_or_cleanup_pptx_autorecover_on_discard(&mut self) {
        #[cfg(not(target_family = "wasm"))]
        {
            let config = load_autorecover_config();
            if let Some(path) = self.active_autorecover_file.take() {
                if config.keep_last_unsaved && path.exists() {
                    let dir = effective_autorecover_dir(&config);
                    let (stem, _) = self.name_parts();
                    let timestamp = crate::clock::unix_millis();
                    let unsaved_name = format!("Unsaved_{stem}_{timestamp}.pptx");
                    let unsaved_path = dir.join(unsaved_name);
                    let _ = std::fs::rename(&path, &unsaved_path);
                } else if path.exists() {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }

    /// Dismiss the recovery banner.
    pub(crate) fn dismiss_pptx_recovery_banner(&mut self, cx: &mut Context<Self>) {
        self.is_recovered = false;
        self.cleanup_active_autorecover_file();
        cx.notify();
    }

    /// Render the yellow recovery banner across the top of the workspace.
    pub(crate) fn render_pptx_autorecover_banner(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.is_recovered {
            return None;
        }

        let bg_color = rgb(0xfef3c7);
        let border_color = rgb(0xf59e0b);
        let text_color = rgb(0x92400e);

        Some(
            h_flex()
                .id("autorecover-banner")
                .w_full()
                .py_1()
                .px_3()
                .bg(bg_color)
                .border_b_1()
                .border_color(border_color)
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(Material("history")).size(px(18.)).text_color(rgb(0xd97706)))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(text_color)
                                .child(text("pptx.autorecover.banner-desc")),
                        ),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("autorecover-banner-save-as")
                                .primary()
                                .xsmall()
                                .label(text("pptx.autorecover.save-as"))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.save_as(&crate::SaveAs, window, cx);
                                })),
                        )
                        .child(
                            Button::new("autorecover-banner-dismiss")
                                .ghost()
                                .xsmall()
                                .label(text("pptx.autorecover.discard"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.dismiss_pptx_recovery_banner(cx);
                                })),
                        ),
                )
                .into_any_element(),
        )
    }

    // ------------------------------------------------------------------------
    // Recover Unsaved Presentations Panel Methods (PPTX-FILE-010)
    // ------------------------------------------------------------------------

    /// ファイル/ツール > 未保存のプレゼンテーションの回復…: Open the Recover Unsaved Presentations panel.
    pub(crate) fn open_pptx_recover_unsaved_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let config = load_autorecover_config();
        let dir = effective_autorecover_dir(&config);
        let entries = scan_recovered_pptx_files(&dir);

        let panel = RecoverUnsavedPanel {
            entries,
            selected_index: None,
            message: String::new(),
            ui: Rc::new(RefCell::new(BTreeMap::new())),
        };

        if let Some(s) = self.pptx_mut() {
            s.recover_unsaved = Some(panel);
            s.autorecover_settings = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Close the Recover Unsaved Presentations panel.
    pub(crate) fn close_pptx_recover_unsaved_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.recover_unsaved = None;
        }
        cx.notify();
    }

    /// Open a recovered file entry into the editor with the recovery banner.
    pub(crate) fn open_pptx_recovered_file_entry(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (path, file_name) = {
            let Some(panel) = self.pptx().and_then(|s| s.recover_unsaved.as_ref()) else { return };
            let Some(entry) = panel.entries.get(index) else { return };
            (entry.path.clone(), entry.file_name.clone())
        };
        #[cfg(not(target_family = "wasm"))]
        {
            match crate::read_file(&path) {
                Ok(loaded) => {
                    let display_name: SharedString = crate::shell::i18n::format("_pptx.recovered-file-name", &[("file_name", &file_name)]).into();
                    self.load(loaded.presentation, display_name, window, cx);
                    self.source = Some(loaded.source);
                    self.path = None; // Needs Save As
                    self.is_recovered = true;
                    self.active_autorecover_file = Some(path);
                    self.set_status(text("pptx.recover.opened-status").to_string(), cx);
                    self.close_pptx_recover_unsaved_panel(cx);
                }
                Err(err) => {
                    if let Some(panel) = self.pptx_mut().and_then(|s| s.recover_unsaved.as_mut()) {
                        panel.message = format!("{}: {err}", text("pptx.recover.open-failed"));
                        cx.notify();
                    }
                }
            }
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = (path, file_name, window);
            if let Some(panel) = self.pptx_mut().and_then(|s| s.recover_unsaved.as_mut()) {
                panel.message = "WASM does not support local file reading".to_string();
                cx.notify();
            }
        }
    }

    /// Delete a recovered file from disk and update the list.
    pub(crate) fn delete_pptx_recovered_file_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        let path = {
            let Some(panel) = self.pptx().and_then(|s| s.recover_unsaved.as_ref()) else { return };
            let Some(entry) = panel.entries.get(index) else { return };
            entry.path.clone()
        };
        #[cfg(not(target_family = "wasm"))]
        {
            if path.exists() {
                match std::fs::remove_file(&path) {
                    Ok(()) => {
                        let config = load_autorecover_config();
                        let dir = effective_autorecover_dir(&config);
                        if let Some(panel) = self.pptx_mut().and_then(|s| s.recover_unsaved.as_mut()) {
                            panel.entries = scan_recovered_pptx_files(&dir);
                            panel.message = text("pptx.recover.delete-done").to_string();
                        }
                    }
                    Err(e) => {
                        if let Some(panel) = self.pptx_mut().and_then(|s| s.recover_unsaved.as_mut()) {
                            panel.message = format!("{}: {e}", text("pptx.recover.delete-failed"));
                        }
                    }
                }
            }
        }
        cx.notify();
    }

    // ------------------------------------------------------------------------
    // Sidebar Rendering
    // ------------------------------------------------------------------------

    pub(crate) fn pptx_autorecover_settings_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.pptx().and_then(|s| s.autorecover_settings.as_ref())?;
        let enabled = panel.enabled;
        let keep_last = panel.keep_last_unsaved;
        let disable_pres = panel.disable_for_this_presentation;
        let interval_input = panel.interval_input.clone();
        let location_input = panel.location_input.clone();
        let message = panel.message.clone();

        let element = v_flex()
            .gap(px(14.))
            .p(px(12.))
            .child(
                v_flex()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(cx.theme().foreground)
                            .child(text("pptx.autorecover.section-general")),
                    )
                    .child(
                        Button::new("autorecover-toggle-enabled")
                            .label(format!("{} {}", if enabled { "☑" } else { "☐" }, text("pptx.autorecover.enable-label")))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(p) = this.pptx_mut().and_then(|s| s.autorecover_settings.as_mut()) {
                                    p.enabled = !p.enabled;
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        v_flex()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(text("pptx.autorecover.minutes-label")),
                            )
                            .child(Input::new(&interval_input)),
                    )
                    .child(
                        Button::new("autorecover-toggle-keep-unsaved")
                            .label(format!("{} {}", if keep_last { "☑" } else { "☐" }, text("pptx.autorecover.keep-unsaved-label")))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(p) = this.pptx_mut().and_then(|s| s.autorecover_settings.as_mut()) {
                                    p.keep_last_unsaved = !p.keep_last_unsaved;
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        v_flex()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(text("pptx.autorecover.location-label")),
                            )
                            .child(Input::new(&location_input))
                            .child(
                                Button::new("autorecover-reset-location")
                                    .small()
                                    .label(text("pptx.autorecover.reset-location-btn"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.reset_pptx_autorecover_location_to_default(window, cx);
                                    })),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(cx.theme().foreground)
                            .child(text("pptx.autorecover.section-presentation")),
                    )
                    .child(
                        Button::new("autorecover-toggle-disable-pres")
                            .label(format!("{} {}", if disable_pres { "☑" } else { "☐" }, text("pptx.autorecover.disable-presentation-label")))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(p) = this.pptx_mut().and_then(|s| s.autorecover_settings.as_mut()) {
                                    p.disable_for_this_presentation = !p.disable_for_this_presentation;
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Button::new("autorecover-save-now")
                            .label(text("pptx.autorecover.save-now-btn"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save_pptx_autorecover_now(window, cx);
                            })),
                    ),
            )
            .when(!message.is_empty(), |el| {
                el.child(
                    div()
                        .p(px(8.))
                        .rounded_md()
                        .bg(cx.theme().accent.opacity(0.1))
                        .text_xs()
                        .text_color(cx.theme().accent)
                        .child(message),
                )
            })
            .child(
                h_flex()
                    .gap(px(8.))
                    .child(
                        Button::new("autorecover-cancel")
                            .label(text("pptx.autorecover.cancel"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_pptx_autorecover_settings_panel(cx);
                            })),
                    )
                    .child(
                        Button::new("autorecover-save-btn")
                            .primary()
                            .label(text("pptx.autorecover.save-btn"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.save_pptx_autorecover_settings(cx);
                            })),
                    ),
            );

        Some(SidebarPanel {
            title: text("pptx.autorecover.settings-title").into(),
            body: element.into_any_element(),
        })
    }

    pub(crate) fn pptx_recover_unsaved_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.pptx().and_then(|s| s.recover_unsaved.as_ref())?;
        let entries = panel.entries.clone();
        let message = panel.message.clone();

        let element = v_flex()
            .gap(px(12.))
            .p(px(12.))
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(cx.theme().foreground)
                            .child(format!("{} ({})", text("pptx.recover.panel-title"), entries.len())),
                    )
                    .child(
                        Button::new("recover-refresh-btn")
                            .small()
                            .label(text("pptx.recover.refresh-btn"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_pptx_recover_unsaved_panel(window, cx);
                            })),
                    ),
            )
            .when(!message.is_empty(), |el| {
                el.child(
                    div()
                        .p(px(8.))
                        .rounded_md()
                        .bg(cx.theme().accent.opacity(0.1))
                        .text_xs()
                        .text_color(cx.theme().accent)
                        .child(message),
                )
            })
            .when(entries.is_empty(), |el| {
                el.child(
                    div()
                        .p(px(16.))
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(text("pptx.recover.empty")),
                )
            })
            .children(entries.iter().enumerate().map(|(idx, entry)| {
                let name = entry.file_name.clone();
                let mod_str = entry.modified_str.clone();
                let sz_str = entry.size_str.clone();

                v_flex()
                    .id(("recovered-item", idx))
                    .p(px(8.))
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().background)
                    .gap(px(6.))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(cx.theme().foreground)
                            .child(name),
                    )
                    .child(
                        h_flex()
                            .justify_between()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(mod_str)
                            .child(sz_str),
                    )
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .child(
                                Button::new(("recover-open-btn", idx))
                                    .small()
                                    .primary()
                                    .label(text("pptx.recover.open-btn"))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_pptx_recovered_file_entry(idx, window, cx);
                                    })),
                            )
                            .child(
                                Button::new(("recover-del-btn", idx))
                                    .small()
                                    .ghost()
                                    .label(text("pptx.recover.delete-btn"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.delete_pptx_recovered_file_entry(idx, cx);
                                    })),
                            ),
                    )
            }));

        Some(SidebarPanel {
            title: text("pptx.recover.panel-title").into(),
            body: element.into_any_element(),
        })
    }
}
