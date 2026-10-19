// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The runtime of ibisOffice's AppImage (see the crate's docs): unpacks the
//! image after it once per version into `~/.cache/ibisOffice/app/<id>/`
//! and starts its `AppRun` with the arguments given, as AppImage's own
//! runtime does (`APPIMAGE`, `APPDIR`, `ARGV0` and `OWD` set).
//!
//! Its own options, as AppImage's runtime has them:
//! `--appimage-extract [path]` unpacks into `./squashfs-root` (only `path`
//! and what is under it, when given), `--appimage-offset` prints where the
//! image starts, `--appimage-help` lists them.

use std::ffi::OsString;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use backhand::{FilesystemReader, InnerNode};
use ibis_appimage::{cache_root, elf_end, image_id};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("ibisOffice: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let appimage = std::env::current_exe().map_err(|e| format!("this program's path: {e}"))?;
    let mut args = std::env::args_os();
    let argv0 = args.next().unwrap_or_default();
    let args: Vec<OsString> = args.collect();

    let offset = image_offset(&appimage)?;
    match args.first().and_then(|a| a.to_str()) {
        Some("--appimage-help") => {
            println!("--appimage-extract [path]  unpack into ./squashfs-root");
            println!("--appimage-offset          where the image starts");
            println!("--appimage-help            this list");
            return Ok(());
        }
        Some("--appimage-offset") => {
            println!("{offset}");
            return Ok(());
        }
        Some("--appimage-extract") => {
            let only = args.get(1).map(PathBuf::from);
            let fs = open_image(&appimage, offset)?;
            unpack(&fs, Path::new("squashfs-root"), only.as_deref())?;
            println!("squashfs-root");
            return Ok(());
        }
        // Unpacking and running is what this runtime always does.
        Some("--appimage-extract-and-run") => return start(&appimage, offset, argv0, &args[1..]),
        _ => {}
    }
    start(&appimage, offset, argv0, &args)
}

/// Start the app: from its folder in the cache, unpacked first if it is not there.
fn start(appimage: &Path, offset: u64, argv0: OsString, args: &[OsString]) -> Result<(), String> {
    let root = cache_root().ok_or("no home folder (HOME) to unpack the app into")?;
    let id = {
        let mut superblock = [0u8; 96];
        let mut file = File::open(appimage).map_err(|e| format!("{}: {e}", appimage.display()))?;
        file.seek(SeekFrom::Start(offset)).and_then(|_| file.read_exact(&mut superblock)).map_err(|e| format!("reading the image: {e}"))?;
        image_id(&superblock).ok_or("the image is not a SquashFS image")?
    };
    let dir = root.join(&id);
    if !dir.join("AppRun").exists() {
        std::fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
        // Unpacked beside it under another name and renamed when complete,
        // so a folder under the final name is always whole (also when two
        // copies start at once: the second to finish drops its own).
        let partial = root.join(format!("{id}.partial-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&partial);
        let fs = open_image(appimage, offset)?;
        if let Err(err) = unpack(&fs, &partial, None) {
            let _ = std::fs::remove_dir_all(&partial);
            return Err(err);
        }
        if std::fs::rename(&partial, &dir).is_err() {
            let _ = std::fs::remove_dir_all(&partial);
            if !dir.join("AppRun").exists() {
                return Err(format!("could not move the app into {}", dir.display()));
            }
        }
    }
    let owd = std::env::current_dir().unwrap_or_default();
    let err = Command::new(dir.join("AppRun"))
        .arg0(&argv0)
        .args(args)
        .env("APPIMAGE", appimage)
        .env("APPDIR", &dir)
        .env("ARGV0", &argv0)
        .env("OWD", owd)
        .exec();
    Err(format!("starting {}: {err}", dir.join("AppRun").display()))
}

/// Where the image starts: after the ELF program (`elf_end`).
fn image_offset(appimage: &Path) -> Result<u64, String> {
    let mut head = Vec::new();
    File::open(appimage)
        .and_then(|f| f.take(64).read_to_end(&mut head))
        .map_err(|e| format!("{}: {e}", appimage.display()))?;
    elf_end(&head).ok_or_else(|| format!("{} is not an ELF program", appimage.display()))
}

fn open_image(appimage: &Path, offset: u64) -> Result<FilesystemReader<'static>, String> {
    let file = File::open(appimage).map_err(|e| format!("{}: {e}", appimage.display()))?;
    FilesystemReader::from_reader_with_offset(BufReader::new(file), offset).map_err(|e| format!("reading the image: {e}"))
}

/// Unpack the image into `into` (made), keeping the files' permissions;
/// only `only` and what is under it when given.
fn unpack(fs: &FilesystemReader, into: &Path, only: Option<&Path>) -> Result<(), String> {
    let err = |path: &Path, e: std::io::Error| format!("{}: {e}", path.display());
    std::fs::create_dir_all(into).map_err(|e| err(into, e))?;
    // Directories' permissions are set last: a read-only one could not be filled.
    let mut dirs = Vec::new();
    for node in fs.files() {
        let rel = node.fullpath.strip_prefix("/").unwrap_or(&node.fullpath);
        if rel.as_os_str().is_empty() {
            continue;
        }
        if let Some(only) = only
            && !rel.starts_with(only)
            && !only.starts_with(rel)
        {
            continue;
        }
        let path = into.join(rel);
        let mode = u32::from(node.header.permissions) & 0o7777;
        match &node.inner {
            InnerNode::Dir(_) => {
                std::fs::create_dir_all(&path).map_err(|e| err(&path, e))?;
                dirs.push((path, mode));
            }
            InnerNode::File(file) => {
                let mut out = File::create(&path).map_err(|e| err(&path, e))?;
                let mut reader = fs.file(file).reader();
                std::io::copy(&mut reader, &mut out).map_err(|e| err(&path, e))?;
                out.set_permissions(std::fs::Permissions::from_mode(mode)).map_err(|e| err(&path, e))?;
            }
            InnerNode::Symlink(link) => {
                std::os::unix::fs::symlink(&link.link, &path).map_err(|e| err(&path, e))?;
            }
            // Devices, pipes and sockets have no place in an app.
            _ => {}
        }
    }
    for (path, mode) in dirs.into_iter().rev() {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).map_err(|e| err(&path, e))?;
    }
    Ok(())
}
