// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ファイル > 保存 (⌘S) and 名前を付けて保存… (⌘⇧S).
//!
//! The file is written by `ibis_pptx::Source::save`: the original file with
//! the edits made since it was opened written into it (see
//! `ibis-pptx/src/save/mod.rs`). An edit the format cannot save yet is
//! reported in the status bar and nothing is written, so no edit is lost
//! silently. The file is written next to its destination and then moved
//! over it, so a failed save never leaves a half-written file.

use gpui_kit::*;

use crate::shell::editor::SlideEditor;
use crate::{Save, SaveAs};

/// What is done once a save has written the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AfterSave {
    /// Nothing: the file stays open (保存, 名前を付けて保存…).
    Stay,
    /// Close the file (閉じる's 「保存」): only once it is saved, so a save
    /// that fails, or that waits for a path, never loses the edits.
    Close,
    /// Close the file, then its window (「保存」 when the window is being
    /// closed): only once it is saved, as `Close`.
    CloseWindow,
}

impl AfterSave {
    /// Whether the file is closed after a save that ended with `result`.
    #[cfg(not(target_family = "wasm"))]
    fn closes(self, result: &Result<(), String>) -> bool {
        matches!(self, AfterSave::Close | AfterSave::CloseWindow) && result.is_ok()
    }
}

impl SlideEditor {
    /// ⌘S: save to the file it was opened from (or ask where, for a file
    /// that has no path yet).
    pub(crate) fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        self.save_then(AfterSave::Stay, window, cx);
    }

    /// `save`, then `after` once the file is saved. A file with no path yet
    /// is saved only once a path is picked, so `after` waits for that.
    pub(crate) fn save_then(&mut self, after: AfterSave, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        match self.path.clone() {
            Some(path) => {
                let result = self.save_to(path, window);
                self.finish_save(result, after, window, cx);
            }
            None => self.save_as_then(after, window, cx),
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = (window, after);
            self.status = Some(crate::shell::i18n::text("_shell.saving-is-not-yet-available-in-the-browser-versi").into());
            cx.notify();
        }
    }

    /// ⌘⇧S: ask where, and save there.
    pub(crate) fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        self.save_as_then(AfterSave::Stay, window, cx);
    }

    /// `save_as`, then `after` once the file is saved there: not when the
    /// dialog is cancelled or the save fails.
    fn save_as_then(&mut self, after: AfterSave, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        if self.source.is_none() {
            self.status = Some(crate::shell::i18n::text("_shell.saving-new-documents-is-not-yet-supported").into());
            return cx.notify();
        }
        #[cfg(not(target_family = "wasm"))]
        {
            let directory = save_as_directory(self.path.as_deref(), self.save_as_folder.as_deref()).unwrap_or_else(|| {
                if self.is_workbook() {
                    crate::xlsx::backup_save::effective_default_save_location()
                } else {
                    std::env::var_os("HOME").map(Into::into).unwrap_or_else(|| ".".into())
                }
            });
            let name = self.file_name.clone().unwrap_or_default();
            let picked = cx.prompt_for_new_path(&directory, Some(&name));
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(path))) = picked.await else { return };
                this.update_in(cx, |this, window, cx| {
                    let result = this.save_to(path, window);
                    this.finish_save(result, after, window, cx);
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = (window, after);
            self.status = Some(crate::shell::i18n::text("_shell.saving-is-not-yet-available-in-the-browser-versi").into());
            cx.notify();
        }
    }

    /// Save to `path`, and make it the file's path.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn save_to(&mut self, path: std::path::PathBuf, window: &mut Window) -> Result<(), String> {
        // The master view is closed first: what is saved is the file, with what was made in it.
        if self.is_master_view() || self.is_page_view() {
            return Err(crate::shell::i18n::text("_shell.close-slide-master-view-and-notes-formatting-bef").to_string());
        }
        let source = self.source.clone().ok_or(crate::shell::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string())?;
        // A workbook saved as .csv: the sheet shown, as CSV (Excel's 「CSV UTF-8」).
        let csv = self.is_workbook() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("csv"));
        let tsv = self.is_workbook() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("tsv") || e.eq_ignore_ascii_case("tab") || e.eq_ignore_ascii_case("txt"));
        let ods = self.is_workbook() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ods"));
        let xls = self.is_workbook() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("xls"));
        let xltx = self.is_workbook() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("xltx") || e.eq_ignore_ascii_case("xltm"));
        let ppsx = self.is_presentation() && path.extension().and_then(|e| e.to_str()).is_some_and(ibis_pptx::is_slideshow);
        let potx = self.is_presentation() && path.extension().and_then(|e| e.to_str()).is_some_and(ibis_pptx::is_template);
        let dotx = self.is_document() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("dotx") || e.eq_ignore_ascii_case("dotm"));
        let pptx = self.is_presentation() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pptx") || e.eq_ignore_ascii_case("pptm"));
        let bytes = if csv {
            let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
            ibis_pptx::csv::sheet_to_csv(self.presentation(), index)
        } else if tsv {
            let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
            ibis_pptx::csv::sheet_to_tsv(self.presentation(), index)
        } else if ods {
            ibis_pptx::write_ods_with_source(self.presentation(), &source).map_err(|e| e.to_string())?
        } else if xls {
            let (bytes, report) = ibis_pptx::write_xls_report(self.presentation()).map_err(|e| e.to_string())?;
            self.save_note = crate::xlsx::export_xls::note(&report);
            bytes
        } else if xltx || potx || dotx {
            source.save_template(self.presentation(), ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?
        } else if ppsx {
            source.save_slideshow(self.presentation(), ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?
        } else if pptx {
            source.save_presentation(self.presentation(), ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?
        } else {
            source.save(self.presentation(), ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?
        };
        // 常にバックアップ コピーを作成する (a document's): one that cannot be made
        // stops the save, so that the file is not written over without it.
        if self.is_document() {
            if let Some(copy) = crate::docx::backup::make_backup(&path)? {
                log::info!("Created backup copy at: {}", copy.display());
            }
        }
        // Protected with the password the file has (パスワードを使用して暗号化).
        let bytes = if let (Some(password), false, false, false, false) = (&self.protection_password, csv, tsv, ods, xls) { protect(&bytes, password)? } else { bytes };
        // A workbook's backup (バックアップと保存先の設定): its setting, or the workbook's own.
        let backup = self.is_workbook().then(|| (crate::xlsx::backup_save::load_backup_save_config(), self.sheet_state().and_then(|s| s.always_create_backup)));
        back_up_and_write(&path, &bytes, backup.as_ref().map(|(config, workbook)| (config, *workbook)))?;
        let name: SharedString = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().into();
        window.set_window_title(&format!("{name} - Acauda Office"));
        self.file_name = Some(name);
        self.set_file_path(Some(path));
        // What is open is now what its file holds, so closing it asks
        // nothing until it is edited again (Save As too: the file is the new
        // one, as in Word and Excel). Not when saved as CSV or text, which
        // keep the values of one sheet only: Excel asks again then too, and
        // the other sheets and the formats are not saved.
        if !(csv || tsv) {
            self.document.mark_saved();
        }
        if self.is_workbook() || self.is_presentation() || self.is_document() {
            self.cleanup_active_autorecover_file();
            self.is_recovered = false;
            self.is_repaired = false;
        }
        Ok(())
    }

    /// Report how a save ended, then do `after` if it saved: a failed save
    /// leaves the file open, with its edits and the error in the status bar.
    #[cfg(not(target_family = "wasm"))]
    fn finish_save(&mut self, result: Result<(), String>, after: AfterSave, window: &mut Window, cx: &mut Context<Self>) {
        let close = after.closes(&result);
        if result.is_ok() {
            self.share_saved_with_other_windows(cx);
        }
        self.report_save(result, cx);
        if close {
            self.force_close_file(window, cx);
            if after == AfterSave::CloseWindow {
                window.remove_window();
            }
        }
    }

    /// The other windows of the file (a workbook's or a document's
    /// 新しいウィンドウ) share its document, so they are saved with it too:
    /// the last one closed asks nothing.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn share_saved_with_other_windows(&self, cx: &mut Context<Self>) {
        self.sync_workbook_peers(cx);
        self.sync_document_peers(cx);
    }

    #[cfg(not(target_family = "wasm"))]
    fn report_save(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        self.status = Some(match result {
            Ok(()) => self.save_note.take().unwrap_or_else(|| crate::shell::i18n::text("_shell.saved").into()),
            Err(err) => crate::shell::i18n::format("file.save-failed", &[("error", &err)]).into(),
        });
        cx.notify();
    }
}

/// The folder 名前を付けて保存… starts in: the file's own folder, else the
/// folder kept for a file with no path yet (`save_as_folder`); `None` when
/// there is neither, for the usual default.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn save_as_directory(path: Option<&std::path::Path>, save_as_folder: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    path.and_then(|p| p.parent()).or(save_as_folder).map(std::path::Path::to_path_buf)
}

/// `package` protected with `password`: an OOXML package kept encrypted in a compound file.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn protect(package: &[u8], password: &str) -> Result<Vec<u8>, String> {
    let mut urandom = std::fs::File::open("/dev/urandom").map_err(|_| crate::shell::i18n::text("_shell.passwords-cannot-be-set-in-this-environment").to_string())?;
    let mut failed = false;
    let encrypted = ibis_pptx::encrypt(package, password, &mut |bytes: &mut [u8]| {
        use std::io::Read;
        if urandom.read_exact(bytes).is_err() {
            failed = true;
        }
    });
    if failed {
        return Err(crate::shell::i18n::text("_shell.passwords-cannot-be-set-in-this-environment").to_string());
    }
    Ok(encrypted)
}

/// Back up what `path` holds as `backup` says (a workbook's backup setting
/// and its own override), then replace it with `bytes`. A backup that
/// cannot be made stops the save with an error that says why, so `path` is
/// never replaced without the backup that was asked for (as in Excel).
#[cfg(not(target_family = "wasm"))]
fn back_up_and_write(
    path: &std::path::Path,
    bytes: &[u8],
    backup: Option<(&crate::xlsx::backup_save::BackupSaveConfig, Option<bool>)>,
) -> Result<(), String> {
    if let Some((config, workbook_override)) = backup {
        if let Some(backup_path) = crate::xlsx::backup_save::create_backup_copy(path, config, workbook_override)? {
            log::info!("Created backup copy at: {}", backup_path.display());
        }
    }
    write_replacing(path, bytes).map_err(|e| crate::shell::i18n::format("file.cannot-write", &[("path", &path.display()), ("error", &e)]))
}

/// Write `bytes` to a new file next to `path`, then move it over `path`.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn write_replacing(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temporary = path.with_file_name(format!(".{name}.ibis-saving"));
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}

#[cfg(test)]
mod tests {
    use super::AfterSave;

    #[test]
    fn the_file_is_closed_only_after_it_is_saved() {
        let saved: Result<(), String> = Ok(());
        let failed: Result<(), String> = Err("cannot write".into());
        assert!(AfterSave::Close.closes(&saved));
        // A failed save keeps the file open, with its edits and the error shown.
        assert!(!AfterSave::Close.closes(&failed));
        // So with the window being closed: it stays open when the save fails.
        assert!(AfterSave::CloseWindow.closes(&saved));
        assert!(!AfterSave::CloseWindow.closes(&failed));
        // 保存 and 名前を付けて保存… never close it.
        assert!(!AfterSave::Stay.closes(&saved));
        assert!(!AfterSave::Stay.closes(&failed));
    }

    /// A folder of its own under the system's temporary folder.
    fn temporary_folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ibisoffice-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn always_back_up_to(folder: std::path::PathBuf) -> crate::xlsx::backup_save::BackupSaveConfig {
        use crate::xlsx::backup_save::{BackupLocation, BackupSaveConfig};
        BackupSaveConfig { always_create_backup: true, backup_location: BackupLocation::Custom(folder), ..BackupSaveConfig::default() }
    }

    #[test]
    fn a_backup_that_cannot_be_made_stops_the_save() {
        let dir = temporary_folder("backup-fails");
        let file = dir.join("Book.xlsx");
        std::fs::write(&file, b"previous version").unwrap();
        // The backup folder cannot be made: a file is where its parent would be.
        let blocker = dir.join("not-a-folder");
        std::fs::write(&blocker, b"").unwrap();
        let config = always_back_up_to(blocker.join("backups"));

        let result = super::back_up_and_write(&file, b"new version", Some((&config, None)));

        let error = result.expect_err("the save stops when its backup cannot be made");
        assert!(error.contains(&blocker.join("backups").display().to_string()), "the error names the backup folder: {error}");
        assert_eq!(std::fs::read(&file).unwrap(), b"previous version", "the file is not replaced");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_backup_keeps_the_previous_version() {
        let dir = temporary_folder("backup-made");
        let file = dir.join("Book.xlsx");
        std::fs::write(&file, b"previous version").unwrap();
        let backups = dir.join("backups");
        let config = always_back_up_to(backups.clone());

        super::back_up_and_write(&file, b"new version", Some((&config, None))).expect("saved");

        assert_eq!(std::fs::read(&file).unwrap(), b"new version");
        let backup = std::fs::read_dir(&backups).unwrap().next().expect("a backup was made").unwrap().path();
        assert_eq!(std::fs::read(backup).unwrap(), b"previous version");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_workbook_setting_turns_the_backup_off() {
        let dir = temporary_folder("backup-off");
        let file = dir.join("Book.xlsx");
        std::fs::write(&file, b"previous version").unwrap();
        // Off for this workbook: the folder that could not be made is never tried.
        let blocker = dir.join("not-a-folder");
        std::fs::write(&blocker, b"").unwrap();
        let config = always_back_up_to(blocker.join("backups"));

        super::back_up_and_write(&file, b"new version", Some((&config, Some(false)))).expect("saved without a backup");

        assert_eq!(std::fs::read(&file).unwrap(), b"new version");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
