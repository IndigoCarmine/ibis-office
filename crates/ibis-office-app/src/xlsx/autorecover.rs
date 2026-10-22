// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 自動回復の設定 (XLSX-FILE-014), 回復したブックの保存 (XLSX-FILE-015),
//! 未保存ブックの回復 (XLSX-FILE-016).
//!
//! Provides AutoRecover configuration, automatic background snapshot creation,
//! restoration of unsaved workbooks, and a recovery banner with Save As workflow.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;
use crate::shell::icons::Material;

/// AutoRecover configuration stored persistently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AutoRecoverConfig {
    /// 次の間隔で自動回復用データを保存する (Save AutoRecover info every X minutes).
    pub(crate) enabled: bool,
    /// 保存間隔（分、1〜120分、デフォルト10分）.
    pub(crate) interval_minutes: u32,
    /// 保存しないで終了する場合、最後に自動回復されたバージョンを残す (Keep last unsaved version).
    pub(crate) keep_last_unsaved: bool,
    /// カスタム保存先ディレクトリ（Noneの場合はデフォルト）.
    pub(crate) custom_location: Option<PathBuf>,
}

impl Default for AutoRecoverConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_minutes: 10,
            keep_last_unsaved: true,
            custom_location: None,
        }
    }
}

/// The default directory for AutoRecover files.
pub(crate) fn default_autorecover_dir() -> PathBuf {
    let env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = env("HOME").map(|home| home.join("Library/Application Support"));
    #[cfg(target_os = "windows")]
    let base = env("APPDATA");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let base = env("XDG_DATA_HOME").or_else(|| env("HOME").map(|home| home.join(".local/share")));
    let dir = base.unwrap_or_else(|| PathBuf::from(".")).join("ibisOffice").join("AutoRecover");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Where the config file is stored.
fn config_store_path() -> Option<PathBuf> {
    let env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = env("HOME").map(|home| home.join("Library/Application Support"));
    #[cfg(target_os = "windows")]
    let base = env("APPDATA");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let base = env("XDG_DATA_HOME").or_else(|| env("HOME").map(|home| home.join(".local/share")));
    Some(base?.join("ibisOffice").join("autorecover-config.tsv"))
}

/// Load configuration from disk, or return default.
pub(crate) fn load_autorecover_config() -> AutoRecoverConfig {
    let Some(path) = config_store_path() else { return AutoRecoverConfig::default() };
    let Ok(content) = std::fs::read_to_string(path) else { return AutoRecoverConfig::default() };
    let mut config = AutoRecoverConfig::default();
    for line in content.lines() {
        let parts: Vec<&str> = line.splitn(2, '\t').collect();
        if parts.len() == 2 {
            match parts[0] {
                "enabled" => config.enabled = parts[1] == "1" || parts[1].eq_ignore_ascii_case("true"),
                "interval_minutes" => {
                    if let Ok(val) = parts[1].parse::<u32>() {
                        config.interval_minutes = val.clamp(1, 120);
                    }
                }
                "keep_last_unsaved" => config.keep_last_unsaved = parts[1] == "1" || parts[1].eq_ignore_ascii_case("true"),
                "custom_location" => {
                    config.custom_location = if parts[1].trim().is_empty() {
                        None
                    } else {
                        Some(PathBuf::from(parts[1].trim()))
                    };
                }
                _ => {}
            }
        }
    }
    config
}

/// Save configuration to disk.
pub(crate) fn save_autorecover_config(config: &AutoRecoverConfig) {
    let Some(path) = config_store_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let loc_str = config.custom_location.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let text = format!(
        "enabled\t{}\ninterval_minutes\t{}\nkeep_last_unsaved\t{}\ncustom_location\t{}\n",
        if config.enabled { "1" } else { "0" },
        config.interval_minutes,
        if config.keep_last_unsaved { "1" } else { "0" },
        loc_str
    );
    let _ = std::fs::write(path, text);
}

/// Get effective directory for AutoRecover files.
pub(crate) fn effective_autorecover_dir(config: &AutoRecoverConfig) -> PathBuf {
    if let Some(custom) = &config.custom_location {
        if custom.is_dir() || std::fs::create_dir_all(custom).is_ok() {
            return custom.clone();
        }
    }
    default_autorecover_dir()
}

/// This run of the app, as it appears in AutoRecover file names: the time
/// it started and its process ID. The editors' IDs start again from 1 in
/// every run, so a name made of them alone is the same as a file a run that
/// crashed left behind.
#[cfg(not(target_family = "wasm"))]
fn autorecover_session() -> &'static str {
    static SESSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SESSION.get_or_init(|| format!("{}-{}", crate::clock::unix_millis(), std::process::id()))
}

/// The file the AutoRecover snapshot of the editor with `id` goes to:
/// `{stem}_autorecover_{session}_{id}.{extension}`, the same file for every
/// snapshot of one document in one run. A file of that name that is not this
/// editor's own snapshot (`active`) is never written over.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn autorecover_snapshot_path(dir: &Path, stem: &str, id: u64, extension: &str, active: Option<&Path>) -> PathBuf {
    snapshot_path_in_session(dir, autorecover_session(), stem, id, extension, active)
}

#[cfg(not(target_family = "wasm"))]
fn snapshot_path_in_session(dir: &Path, session: &str, stem: &str, id: u64, extension: &str, active: Option<&Path>) -> PathBuf {
    let base = format!("{stem}_autorecover_{session}_{id}");
    let mut candidate = dir.join(format!("{base}.{extension}"));
    let mut n = 2u64;
    while candidate.exists() && active != Some(candidate.as_path()) {
        candidate = dir.join(format!("{base}_{n}.{extension}"));
        n += 1;
    }
    candidate
}

/// Format file size for human display.
pub(crate) fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Format timestamp from seconds.
pub(crate) fn format_timestamp_secs(secs: u64) -> String {
    let lt = ibis_doc_model::date_field::LocalTime::from_seconds(secs as i64);
    format!("{:04}-{:02}-{:02} {:02}:{:02}", lt.year, lt.month, lt.day, lt.hour, lt.minute)
}

/// An entry found in the AutoRecover directory.
#[derive(Clone, Debug)]
pub(crate) struct RecoveredFileEntry {
    pub(crate) path: PathBuf,
    pub(crate) file_name: String,
    pub(crate) modified_str: String,
    #[allow(dead_code)]
    pub(crate) size_bytes: u64,
    pub(crate) size_str: String,
}

/// Scan the AutoRecover folder for `.xlsx` recovery files.
pub(crate) fn scan_recovered_files(dir: &Path) -> Vec<RecoveredFileEntry> {
    let mut entries = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else { return entries };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("xlsx")) {
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

/// AutoRecover Settings sidebar panel state (XLSX-FILE-014).
pub(crate) struct AutoRecoverSettingsPanel {
    pub(crate) enabled: bool,
    pub(crate) interval_input: Entity<InputState>,
    pub(crate) keep_last_unsaved: bool,
    pub(crate) location_input: Entity<InputState>,
    pub(crate) disable_for_this_workbook: bool,
    pub(crate) message: String,
    #[allow(dead_code)]
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

/// Recover Unsaved Workbooks sidebar panel state (XLSX-FILE-016).
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
    // AutoRecover Settings Panel Methods (XLSX-FILE-014)
    // ------------------------------------------------------------------------

    /// ツール > 自動回復の設定…: Open the AutoRecover Settings panel in the sidebar.
    pub(crate) fn open_xlsx_autorecover_settings_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let config = load_autorecover_config();
        let interval_val = config.interval_minutes.to_string();
        let loc_val = config
            .custom_location
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| default_autorecover_dir().to_string_lossy().into_owned());

        let interval_input = cx.new(|cx| InputState::new(window, cx).default_value(interval_val));
        let location_input = cx.new(|cx| InputState::new(window, cx).default_value(loc_val));

        let disable_wb = self.sheet_state().map_or(false, |s| s.disable_autorecover_for_workbook);

        let panel = AutoRecoverSettingsPanel {
            enabled: config.enabled,
            interval_input,
            keep_last_unsaved: config.keep_last_unsaved,
            location_input,
            disable_for_this_workbook: disable_wb,
            message: String::new(),
            ui: Rc::new(RefCell::new(BTreeMap::new())),
        };

        self.wb_mut().panels.close();
        self.wb_mut().panels.autorecover_settings = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// Close the AutoRecover Settings panel.
    pub(crate) fn close_xlsx_autorecover_settings_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(wb) = self.sheet_state_mut() {
            wb.panels.autorecover_settings = None;
        }
        cx.notify();
    }

    /// Save the edited settings to configuration and editor state.
    pub(crate) fn save_xlsx_autorecover_settings(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.wb().panels.autorecover_settings.as_ref() else { return };
        let interval_str = panel.interval_input.read(cx).value().trim().to_string();
        let loc_str = panel.location_input.read(cx).value().trim().to_string();
        let enabled = panel.enabled;
        let keep_last_unsaved = panel.keep_last_unsaved;
        let disable_wb = panel.disable_for_this_workbook;

        let interval_minutes = match interval_str.parse::<u32>() {
            Ok(val) if (1..=120).contains(&val) => val,
            _ => {
                if let Some(panel) = self.wb_mut().panels.autorecover_settings.as_mut() {
                    panel.message = text("xlsx.autorecover.error-interval").to_string();
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
                if let Some(panel) = self.wb_mut().panels.autorecover_settings.as_mut() {
                    panel.message = format!("{}: {e}", text("xlsx.autorecover.error-path"));
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

        if let Some(wb) = self.sheet_state_mut() {
            wb.disable_autorecover_for_workbook = disable_wb;
            if let Some(panel) = wb.panels.autorecover_settings.as_mut() {
                panel.message = text("xlsx.autorecover.saved-msg").to_string();
            }
        }
        cx.notify();
    }

    /// Reset custom location input to default directory.
    pub(crate) fn reset_xlsx_autorecover_location_to_default(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let default_loc = default_autorecover_dir().to_string_lossy().into_owned();
        let Some(panel) = self.wb().panels.autorecover_settings.as_ref() else { return };
        panel.location_input.update(cx, |input, cx| {
            input.set_value(default_loc, window, cx);
        });
        cx.notify();
    }

    /// Manually trigger an immediate AutoRecover snapshot save.
    pub(crate) fn save_xlsx_autorecover_now(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        match self.save_autorecover_snapshot() {
            Ok(path) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if let Some(panel) = self.wb_mut().panels.autorecover_settings.as_mut() {
                    panel.message = format!("{} ({name})", text("xlsx.autorecover.now-done"));
                }
            }
            Err(err) => {
                if let Some(panel) = self.wb_mut().panels.autorecover_settings.as_mut() {
                    panel.message = format!("{}: {err}", text("xlsx.autorecover.now-failed"));
                }
            }
        }
        #[cfg(target_family = "wasm")]
        {
            if let Some(panel) = self.wb_mut().panels.autorecover_settings.as_mut() {
                panel.message = text("xlsx.autorecover.wasm-save-unsupported").to_string();
            }
        }
        cx.notify();
    }

    // ------------------------------------------------------------------------
    // AutoRecover Engine Snapshot & Cleanup Methods
    // ------------------------------------------------------------------------

    /// Save an AutoRecover snapshot of the current workbook to disk.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn save_autorecover_snapshot(&mut self) -> Result<PathBuf, String> {
        let (stem, _) = self.name_parts();
        let config = load_autorecover_config();
        let dir = effective_autorecover_dir(&config);
        let _ = std::fs::create_dir_all(&dir);
        let now = crate::clock::unix_millis();
        let target_path = autorecover_snapshot_path(&dir, &stem, self.workbook_id, "xlsx", self.active_autorecover_file.as_deref());

        self.copy_to(&target_path)?;

        self.active_autorecover_file = Some(target_path.clone());
        if let Some(wb) = self.sheet_state_mut() {
            wb.last_autorecover_at = Some(now);
        }
        Ok(target_path)
    }

    /// Remove the active autorecover file on clean save or discard.
    pub(crate) fn cleanup_active_autorecover_file(&mut self) {
        #[cfg(not(target_family = "wasm"))]
        if let Some(path) = self.active_autorecover_file.take() {
            if path.exists() {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    /// On close without saving, preserve the recovery file as Unsaved_... if configured.
    pub(crate) fn preserve_or_cleanup_autorecover_on_discard(&mut self) {
        #[cfg(not(target_family = "wasm"))]
        {
            let config = load_autorecover_config();
            if let Some(path) = self.active_autorecover_file.take() {
                if config.keep_last_unsaved && self.document.can_undo() && path.exists() {
                    let dir = path.parent().unwrap_or(Path::new("."));
                    let (stem, _) = self.name_parts();
                    let now = crate::clock::unix_millis();
                    let unsaved_name = format!("Unsaved_{stem}_{now}.xlsx");
                    let unsaved_path = dir.join(unsaved_name);
                    let _ = std::fs::rename(&path, &unsaved_path);
                } else if path.exists() {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
    }

    /// Dismiss the Recovery Banner (XLSX-FILE-015).
    pub(crate) fn dismiss_recovery_banner(&mut self, cx: &mut Context<Self>) {
        self.is_recovered = false;
        self.cleanup_active_autorecover_file();
        cx.notify();
    }

    /// Render the AutoRecover banner above the worksheet (XLSX-FILE-015).
    pub(crate) fn render_autorecover_banner(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bg_color = rgb(0xfff8e1); // warm light amber
        let border_color = rgb(0xffd54f);
        let text_color = rgb(0x5d4037);

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
                            .child(text("xlsx.autorecover.banner-text")),
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
                            .label(text("xlsx.autorecover.save-as"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save_as(&crate::SaveAs, window, cx);
                            })),
                    )
                    .child(
                        Button::new("autorecover-banner-dismiss")
                            .ghost()
                            .xsmall()
                            .label(text("xlsx.autorecover.discard"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dismiss_recovery_banner(cx);
                            })),
                    ),
            )
    }

    // ------------------------------------------------------------------------
    // Recover Unsaved Workbooks Panel Methods (XLSX-FILE-016)
    // ------------------------------------------------------------------------

    /// ファイル/ツール > 未保存のブックの回復…: Open the Recover Unsaved Workbooks panel.
    pub(crate) fn open_xlsx_recover_unsaved_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let config = load_autorecover_config();
        let dir = effective_autorecover_dir(&config);
        let entries = scan_recovered_files(&dir);

        let panel = RecoverUnsavedPanel {
            entries,
            selected_index: None,
            message: String::new(),
            ui: Rc::new(RefCell::new(BTreeMap::new())),
        };

        self.wb_mut().panels.close();
        self.wb_mut().panels.recover_unsaved = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// Close the Recover Unsaved Workbooks panel.
    pub(crate) fn close_xlsx_recover_unsaved_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(wb) = self.sheet_state_mut() {
            wb.panels.recover_unsaved = None;
        }
        cx.notify();
    }

    /// Open a recovered file entry into the editor with the recovery banner.
    pub(crate) fn open_recovered_file_entry(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (path, file_name) = {
            let Some(panel) = self.wb().panels.recover_unsaved.as_ref() else { return };
            let Some(entry) = panel.entries.get(index) else { return };
            (entry.path.clone(), entry.file_name.clone())
        };
        #[cfg(not(target_family = "wasm"))]
        {
            match crate::read_file(&path) {
                Ok(loaded) => {
                    let display_name: SharedString = crate::shell::i18n::format("_xlsx.recovered-file-name", &[("file_name", &file_name)]).into();
                    self.load(loaded.presentation, display_name, window, cx);
                    self.source = Some(loaded.source);
                    self.path = None; // Needs Save As
                    self.is_recovered = true;
                    self.active_autorecover_file = Some(path);
                    self.set_status(text("xlsx.recover.opened-status").to_string(), cx);
                    self.close_xlsx_recover_unsaved_panel(cx);
                }
                Err(err) => {
                    if let Some(panel) = self.wb_mut().panels.recover_unsaved.as_mut() {
                        panel.message = format!("{}: {err}", text("xlsx.recover.open-failed"));
                        cx.notify();
                    }
                }
            }
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = (window, path, file_name);
            if let Some(panel) = self.wb_mut().panels.recover_unsaved.as_mut() {
                panel.message = text("xlsx.recover.wasm-unsupported").to_string();
                cx.notify();
            }
        }
    }

    /// Delete a single recovered file entry.
    pub(crate) fn delete_recovered_file_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        let path = {
            let Some(panel) = self.wb().panels.recover_unsaved.as_ref() else { return };
            panel.entries.get(index).map(|e| e.path.clone())
        };
        if let Some(path) = path {
            let _ = std::fs::remove_file(&path);
        }
        self.rescan_recovered_files(cx);
        if let Some(panel) = self.wb_mut().panels.recover_unsaved.as_mut() {
            panel.message = text("xlsx.recover.deleted-msg").to_string();
        }
        cx.notify();
    }

    /// Delete all recovered files in the AutoRecover folder.
    pub(crate) fn clear_all_recovered_files(&mut self, cx: &mut Context<Self>) {
        let paths: Vec<PathBuf> = {
            let Some(panel) = self.wb().panels.recover_unsaved.as_ref() else { return };
            panel.entries.iter().map(|e| e.path.clone()).collect()
        };
        for path in paths {
            let _ = std::fs::remove_file(path);
        }
        self.rescan_recovered_files(cx);
        if let Some(panel) = self.wb_mut().panels.recover_unsaved.as_mut() {
            panel.message = text("xlsx.recover.all-deleted-msg").to_string();
        }
        cx.notify();
    }

    /// Rescan recovered files list in the panel.
    pub(crate) fn rescan_recovered_files(&mut self, cx: &mut Context<Self>) {
        let config = load_autorecover_config();
        let dir = effective_autorecover_dir(&config);
        let entries = scan_recovered_files(&dir);
        if let Some(panel) = self.wb_mut().panels.recover_unsaved.as_mut() {
            panel.entries = entries;
        }
        cx.notify();
    }

    // ------------------------------------------------------------------------
    // Sidebar Panel Rendering
    // ------------------------------------------------------------------------

    /// Render the AutoRecover Settings panel in the sidebar.
    pub(crate) fn xlsx_autorecover_settings_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.sheet_state()?.panels.autorecover_settings.as_ref()?;
        let theme = cx.theme();

        let enabled = panel.enabled;
        let keep_last = panel.keep_last_unsaved;
        let disable_wb = panel.disable_for_this_workbook;
        let message = panel.message.clone();

        let interval_input = panel.interval_input.clone();
        let location_input = panel.location_input.clone();

        let body = v_flex()
            .p_3()
            .gap_3()
            .child(
                // Section 1: AutoRecover operation settings
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.foreground)
                            .child(text("xlsx.autorecover.section-general")),
                    )
                    .child(
                        Button::new("autorecover-toggle-enabled")
                            .small()
                            .ghost()
                            .w_full()
                            .label(format!("{} {}", if enabled { "☑" } else { "☐" }, text("xlsx.autorecover.enabled-label")))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(p) = this.wb_mut().panels.autorecover_settings.as_mut() {
                                    p.enabled = !p.enabled;
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(text("xlsx.autorecover.interval-label")),
                            )
                            .child(
                                div()
                                    .w(px(64.))
                                    .child(Input::new(&interval_input).small()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(text("xlsx.autorecover.interval-minutes-unit")),
                            ),
                    )
                    .child(
                        Button::new("autorecover-toggle-keep-unsaved")
                            .small()
                            .ghost()
                            .w_full()
                            .label(format!("{} {}", if keep_last { "☑" } else { "☐" }, text("xlsx.autorecover.keep-unsaved-label")))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(p) = this.wb_mut().panels.autorecover_settings.as_mut() {
                                    p.keep_last_unsaved = !p.keep_last_unsaved;
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .child(
                // Section 2: File location settings
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.foreground)
                            .child(text("xlsx.autorecover.section-location")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(text("xlsx.autorecover.location-label")),
                    )
                    .child(Input::new(&location_input).small())
                    .child(
                        h_flex()
                            .justify_end()
                            .child(
                                Button::new("autorecover-reset-location")
                                    .ghost()
                                    .xsmall()
                                    .label(text("xlsx.autorecover.reset-location"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.reset_xlsx_autorecover_location_to_default(window, cx);
                                    })),
                            ),
                    ),
            )
            .child(
                // Section 3: Workbook-specific setting
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.foreground)
                            .child(text("xlsx.autorecover.section-workbook")),
                    )
                    .child(
                        Button::new("autorecover-toggle-disable-wb")
                            .small()
                            .ghost()
                            .w_full()
                            .label(format!("{} {}", if disable_wb { "☑" } else { "☐" }, text("xlsx.autorecover.disable-workbook-label")))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(p) = this.wb_mut().panels.autorecover_settings.as_mut() {
                                    p.disable_for_this_workbook = !p.disable_for_this_workbook;
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .child(
                // Section 4: Manual snapshot save trigger
                v_flex()
                    .gap_2()
                    .pt_1()
                    .child(
                        Button::new("autorecover-save-now")
                            .outline()
                            .small()
                            .label(text("xlsx.autorecover.save-now-btn"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save_xlsx_autorecover_now(window, cx);
                            })),
                    ),
            )
            .when(!message.is_empty(), |d| {
                d.child(
                    div()
                        .p_2()
                        .rounded(px(4.))
                        .bg(theme.muted)
                        .border_1()
                        .border_color(theme.border)
                        .text_xs()
                        .text_color(theme.foreground)
                        .child(message),
                )
            })
            .child(
                // Section 5: Save & Close buttons
                h_flex()
                    .justify_end()
                    .gap_2()
                    .pt_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        Button::new("autorecover-cancel")
                            .ghost()
                            .small()
                            .label(text("_shell.cancel"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_xlsx_autorecover_settings_panel(cx);
                            })),
                    )
                    .child(
                        Button::new("autorecover-save-btn")
                            .primary()
                            .small()
                            .label(text("xlsx.autorecover.save-btn"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.save_xlsx_autorecover_settings(cx);
                            })),
                    ),
            );

        Some(SidebarPanel {
            title: text("xlsx.autorecover.settings-title").into(),
            body: body.into_any_element(),
        })
    }

    /// Render the Recover Unsaved Workbooks panel in the sidebar.
    pub(crate) fn xlsx_recover_unsaved_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.sheet_state()?.panels.recover_unsaved.as_ref()?;
        let theme = cx.theme();
        let entries = panel.entries.clone();
        let message = panel.message.clone();

        let entries_list: Vec<AnyElement> = if entries.is_empty() {
            vec![
                v_flex()
                    .py_8()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .child(Icon::new(Material("inbox")).size(px(32.)).text_color(theme.muted_foreground))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .child(text("xlsx.recover.empty-title")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(text("xlsx.recover.empty-desc")),
                    )
                    .into_any_element()
            ]
        } else {
            entries
                .iter()
                .enumerate()
                .map(|(i, entry)| {
                    let file_name = entry.file_name.clone();
                    let date_str = entry.modified_str.clone();
                    let size_str = entry.size_str.clone();

                    v_flex()
                        .p_2()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.background)
                        .gap_1()
                        .child(
                            h_flex()
                                .items_center()
                                .gap_2()
                                .child(Icon::new(Material("table_chart")).size(px(18.)).text_color(rgb(0x1e8e3e)))
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme.foreground)
                                        .truncate()
                                        .child(file_name),
                                ),
                        )
                        .child(
                            h_flex()
                                .justify_between()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!("{}: {date_str}", text("xlsx.recover.date-label")))
                                .child(format!("{}: {size_str}", text("xlsx.recover.size-label"))),
                        )
                        .child(
                            h_flex()
                                .justify_end()
                                .gap_2()
                                .pt_1()
                                .child(
                                    Button::new(("recover-entry-delete", i))
                                        .ghost()
                                        .xsmall()
                                        .label(text("_shell.delete"))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.delete_recovered_file_entry(i, cx);
                                        })),
                                )
                                .child(
                                    Button::new(("recover-entry-open", i))
                                        .primary()
                                        .xsmall()
                                        .label(text("_shell.open"))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open_recovered_file_entry(i, window, cx);
                                        })),
                                ),
                        )
                        .into_any_element()
                })
                .collect()
        };

        let body = v_flex()
            .p_3()
            .gap_3()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(text("xlsx.recover.panel-desc")),
            )
            .child(
                v_flex()
                    .gap_2()
                    .children(entries_list),
            )
            .when(!message.is_empty(), |d| {
                d.child(
                    div()
                        .p_2()
                        .rounded(px(4.))
                        .bg(theme.muted)
                        .border_1()
                        .border_color(theme.border)
                        .text_xs()
                        .text_color(theme.foreground)
                        .child(message),
                )
            })
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .pt_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        Button::new("recover-rescan-btn")
                            .ghost()
                            .small()
                            .label(text("xlsx.recover.rescan"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.rescan_recovered_files(cx);
                            })),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("recover-clear-all-btn")
                                    .outline()
                                    .small()
                                    .disabled(entries.is_empty())
                                    .label(text("xlsx.recover.clear-all"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.clear_all_recovered_files(cx);
                                    })),
                            )
                            .child(
                                Button::new("recover-close-btn")
                                    .ghost()
                                    .small()
                                    .label(text("_shell.close"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_xlsx_recover_unsaved_panel(cx);
                                    })),
                            ),
                    ),
            );

        Some(SidebarPanel {
            title: text("xlsx.recover.title").into(),
            body: body.into_any_element(),
        })
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    // Not `super::*`: that brings in gpui_kit's own `test` attribute.
    use super::{autorecover_session, snapshot_path_in_session};
    use std::path::PathBuf;

    /// A new empty folder of its own under the system's temporary folder.
    fn temporary_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ibisOffice-autorecover-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn snapshot_name_differs_from_the_files_an_earlier_run_left() {
        let dir = temporary_dir("earlier-run");
        // What a run that crashed left: its first workbook, Book1, under the
        // old name (the editor's ID alone) and under a name of this kind.
        std::fs::write(dir.join("Book1_autorecover_1.xlsx"), b"earlier").unwrap();
        std::fs::write(dir.join("Book1_autorecover_1000-42_1.xlsx"), b"earlier").unwrap();

        let path = snapshot_path_in_session(&dir, "2000-42", "Book1", 1, "xlsx", None);
        assert!(!path.exists(), "{} is a file another run wrote", path.display());
        assert_eq!(path.parent(), Some(dir.as_path()));
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("xlsx"));
        assert_eq!(std::fs::read(dir.join("Book1_autorecover_1.xlsx")).unwrap(), b"earlier");
        assert_eq!(std::fs::read(dir.join("Book1_autorecover_1000-42_1.xlsx")).unwrap(), b"earlier");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_of_the_same_name_that_is_not_this_editors_is_not_written_over() {
        let dir = temporary_dir("same-name");
        let first = snapshot_path_in_session(&dir, "3000-7", "Book1", 1, "pptx", None);
        std::fs::write(&first, b"not ours").unwrap();

        let path = snapshot_path_in_session(&dir, "3000-7", "Book1", 1, "pptx", None);
        assert_ne!(path, first);
        assert!(!path.exists());
        assert_eq!(std::fs::read(&first).unwrap(), b"not ours");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_snapshot_of_one_editor_goes_to_the_same_file() {
        let dir = temporary_dir("same-editor");
        std::fs::write(dir.join("Book1_autorecover_4000-9_3.xlsx"), b"not ours").unwrap();
        let first = snapshot_path_in_session(&dir, "4000-9", "Book1", 3, "xlsx", None);
        std::fs::write(&first, b"snapshot 1").unwrap();

        let again = snapshot_path_in_session(&dir, "4000-9", "Book1", 3, "xlsx", Some(&first));
        assert_eq!(again, first);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_session_is_the_same_for_the_whole_run_and_names_this_process() {
        let session = autorecover_session();
        assert_eq!(session, autorecover_session());
        assert!(session.ends_with(&format!("-{}", std::process::id())), "{session}");
    }
}
