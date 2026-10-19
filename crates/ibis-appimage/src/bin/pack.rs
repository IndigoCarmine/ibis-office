// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Make an AppImage: `ibis-appimage-pack <runtime> <AppDir> <output>`.
//!
//! The runtime (`ibis-appimage-runtime`) with AppImage's mark in its ELF
//! header, then the AppDir as a SquashFS image (gzip), made executable.

use std::fs::File;
use std::io::Write as _;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::Path;
use std::process::ExitCode;

use backhand::compression::Compressor;
use backhand::{FilesystemCompressor, FilesystemWriter, NodeHeader};
use ibis_appimage::{MAGIC, MAGIC_OFFSET, elf_end};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [runtime, appdir, output] = args.as_slice() else {
        eprintln!("usage: ibis-appimage-pack <runtime> <AppDir> <output>");
        return ExitCode::FAILURE;
    };
    match pack(Path::new(runtime), Path::new(appdir), Path::new(output)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("ibis-appimage-pack: {err}");
            ExitCode::FAILURE
        }
    }
}

fn pack(runtime: &Path, appdir: &Path, output: &Path) -> Result<(), String> {
    let mut program = std::fs::read(runtime).map_err(|e| format!("{}: {e}", runtime.display()))?;
    // The runtime finds the image at the end of its ELF part: nothing may follow it.
    let end = elf_end(&program).ok_or_else(|| format!("{} is not a 64-bit ELF program", runtime.display()))?;
    if end != program.len() as u64 {
        return Err(format!("{} has {} bytes after its ELF part", runtime.display(), program.len() as u64 - end));
    }
    if !appdir.join("AppRun").exists() {
        return Err(format!("{} has no AppRun", appdir.display()));
    }
    program[MAGIC_OFFSET..MAGIC_OFFSET + MAGIC.len()].copy_from_slice(&MAGIC);

    let mut fs = FilesystemWriter::default();
    fs.set_compressor(FilesystemCompressor::new(Compressor::Gzip, None).map_err(|e| e.to_string())?);
    fs.set_block_size(1 << 20);
    fs.set_current_time();
    fs.set_root_mode(0o755);
    add_dir(&mut fs, appdir, Path::new(""))?;

    let mut out = File::create(output).map_err(|e| format!("{}: {e}", output.display()))?;
    out.write_all(&program).map_err(|e| format!("{}: {e}", output.display()))?;
    fs.write_with_offset(&mut out, program.len() as u64).map_err(|e| format!("writing the image: {e}"))?;
    out.set_permissions(std::fs::Permissions::from_mode(0o755)).map_err(|e| format!("{}: {e}", output.display()))?;
    Ok(())
}

/// Add what is in `dir` (at `rel` in the image), in name order, owned by root.
fn add_dir(fs: &mut FilesystemWriter, dir: &Path, rel: &Path) -> Result<(), String> {
    let err = |path: &Path, e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let mut entries: Vec<_> = std::fs::read_dir(dir).map_err(|e| err(dir, &e))?.collect::<Result<_, _>>().map_err(|e| err(dir, &e))?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let at = rel.join(entry.file_name());
        let meta = std::fs::symlink_metadata(&path).map_err(|e| err(&path, &e))?;
        let header = NodeHeader::new((meta.mode() & 0o7777) as u16, 0, 0, meta.mtime() as u32);
        if meta.file_type().is_symlink() {
            let link = std::fs::read_link(&path).map_err(|e| err(&path, &e))?;
            fs.push_symlink(link, &at, header).map_err(|e| err(&path, &e))?;
        } else if meta.is_dir() {
            fs.push_dir(&at, header).map_err(|e| err(&path, &e))?;
            add_dir(fs, &path, &at)?;
        } else if meta.is_file() {
            fs.push_file_from_path(&path, &at, header).map_err(|e| err(&path, &e))?;
        } else {
            return Err(format!("{}: not a file, folder or link", path.display()));
        }
    }
    Ok(())
}
