// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The runtime of ibisOffice's AppImage (see the crate's docs): unpacks the
//! image after it once per version into `~/.cache/AcaudaOffice/app/<id>/`
//! and starts its `AppRun` with the arguments given, as AppImage's own
//! runtime does (`APPIMAGE`, `APPDIR`, `ARGV0` and `OWD` set).
//!
//! Its own options, as AppImage's runtime has them:
//! `--appimage-extract [pattern]` unpacks into `./squashfs-root` (only paths
//! matching `pattern`, which may contain `*`/`?` globs, and what is under them
//! or targeted by matched symlinks), `--appimage-offset` prints where the
//! image starts, `--appimage-portable-home` and `--appimage-portable-config`
//! create `<AppImage>.home` / `<AppImage>.config` beside the AppImage,
//! `--appimage-version`, `--appimage-updateinformation`, `--appimage-signature`,
//! `--appimage-mount`, and `--appimage-help`.

use std::ffi::OsString;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use backhand::{FilesystemReader, InnerNode};
use ibis_appimage::{cache_root, elf_end, image_id};

#[cfg(target_os = "linux")]
#[unsafe(link_section = ".upd_info")]
#[used]
static UPD_INFO: [u8; 1024] = [0; 1024];

#[cfg(target_os = "linux")]
#[unsafe(link_section = ".sha256_sig")]
#[used]
static SHA256_SIG: [u8; 512] = [0; 512];

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("AcaudaOffice: {err}");
            ExitCode::FAILURE
        }
    }
}

fn portable_dir(appimage: &Path, suffix: &str) -> PathBuf {
    let mut s = appimage.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

fn run() -> Result<(), String> {
    let appimage = std::env::current_exe().map_err(|e| format!("this program's path: {e}"))?;
    let mut args = std::env::args_os();
    let argv0 = args.next().unwrap_or_default();
    let args: Vec<OsString> = args.collect();

    let offset = image_offset(&appimage)?;
    match args.first().and_then(|a| a.to_str()) {
        Some("--appimage-help") => {
            println!("AppImage options:");
            println!("  --appimage-extract [<pattern>]  Extract content from embedded filesystem image");
            println!("  --appimage-extract-and-run      Extract and run the AppImage");
            println!("  --appimage-help                 Print this help");
            println!("  --appimage-mount                Mount embedded filesystem image and print mount point");
            println!("  --appimage-offset               Print byte offset to start of embedded filesystem image");
            println!("  --appimage-portable-home        Create a portable home folder to use as $HOME");
            println!("  --appimage-portable-config      Create a portable config folder to use as $XDG_CONFIG_HOME");
            println!("  --appimage-signature            Print digital signature embedded in AppImage");
            println!("  --appimage-updateinformation    Print update info embedded in AppImage");
            println!("  --appimage-version              Print version of AppImageKit");
            return Ok(());
        }
        Some("--appimage-offset") => {
            println!("{offset}");
            return Ok(());
        }
        Some("--appimage-version") => {
            println!("ibis-appimage {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--appimage-updateinformation") => {
            #[cfg(target_os = "linux")]
            {
                let end = UPD_INFO.iter().position(|&b| b == 0).unwrap_or(UPD_INFO.len());
                println!("{}", String::from_utf8_lossy(&UPD_INFO[..end]));
            }
            #[cfg(not(target_os = "linux"))]
            println!();
            return Ok(());
        }
        Some("--appimage-signature") => {
            #[cfg(target_os = "linux")]
            {
                let end = SHA256_SIG.iter().position(|&b| b == 0).unwrap_or(SHA256_SIG.len());
                println!("{}", String::from_utf8_lossy(&SHA256_SIG[..end]));
            }
            #[cfg(not(target_os = "linux"))]
            println!();
            return Ok(());
        }
        Some("--appimage-portable-home") => {
            let dir = portable_dir(&appimage, ".home");
            std::fs::create_dir_all(&dir)
                .and_then(|_| std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)))
                .map_err(|e| format!("{}: {e}", dir.display()))?;
            println!("Portable home directory created at {}", dir.display());
            return Ok(());
        }
        Some("--appimage-portable-config") => {
            let dir = portable_dir(&appimage, ".config");
            std::fs::create_dir_all(&dir)
                .and_then(|_| std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)))
                .map_err(|e| format!("{}: {e}", dir.display()))?;
            println!("Portable config directory created at {}", dir.display());
            return Ok(());
        }
        Some("--appimage-extract") => {
            let pattern = args.get(1).and_then(|s| s.to_str());
            let fs = open_image(&appimage, offset)?;
            unpack(&fs, Path::new("squashfs-root"), pattern)?;
            return Ok(());
        }
        Some("--appimage-mount") => {
            let dir = ensure_unpacked(&appimage, offset)?;
            println!("{}", dir.display());
            let mut buf = [0u8; 1];
            let _ = std::io::stdin().read(&mut buf);
            return Ok(());
        }
        // Unpacking and running is what this runtime always does.
        Some("--appimage-extract-and-run") => return start(&appimage, offset, argv0, &args[1..]),
        _ => {}
    }
    start(&appimage, offset, argv0, &args)
}

/// Ensure the image is unpacked in the cache and return its directory.
fn ensure_unpacked(appimage: &Path, offset: u64) -> Result<PathBuf, String> {
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
    Ok(dir)
}

/// Start the app: from its folder in the cache, unpacked first if it is not there.
fn start(appimage: &Path, offset: u64, argv0: OsString, args: &[OsString]) -> Result<(), String> {
    let dir = ensure_unpacked(appimage, offset)?;
    let owd = std::env::current_dir().unwrap_or_default();
    let mut cmd = Command::new(dir.join("AppRun"));
    cmd.arg0(&argv0)
        .args(args)
        .env("APPIMAGE", appimage)
        .env("APPDIR", &dir)
        .env("ARGV0", &argv0)
        .env("OWD", owd);
    let home_dir = portable_dir(appimage, ".home");
    if home_dir.is_dir() {
        cmd.env("HOME", &home_dir);
    }
    let config_dir = portable_dir(appimage, ".config");
    if config_dir.is_dir() {
        cmd.env("XDG_CONFIG_HOME", &config_dir);
    }
    let err = cmd.exec();
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

/// Glob pattern matching (`*` matches any sequence including `/`, `?` matches any single char).
fn glob_match(pat: &str, text: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star_pi, mut star_ti) = (None, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_pi = Some(pi);
            pi += 1;
            star_ti = ti;
        } else if let Some(sp) = star_pi {
            pi = sp + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

fn matches_pattern(rel: &Path, pattern: &str) -> bool {
    let pat = pattern.trim_start_matches("./").trim_start_matches('/');
    if pat.is_empty() {
        return true;
    }
    let rel_str = rel.to_string_lossy();
    let pat_path = Path::new(pat);
    rel.starts_with(pat_path)
        || pat_path.starts_with(rel)
        || glob_match(pat, &rel_str)
        || rel_str.starts_with(&format!("{pat}/"))
}

/// Unpack the image into `into` (made), keeping the files' permissions;
/// only paths matching `pattern` (and symlink targets of matched symlinks) when given.
fn unpack(fs: &FilesystemReader, into: &Path, pattern: Option<&str>) -> Result<(), String> {
    let err = |path: &Path, e: std::io::Error| format!("{}: {e}", path.display());
    std::fs::create_dir_all(into).map_err(|e| err(into, e))?;
    let mut extra_targets: Vec<PathBuf> = Vec::new();
    if let Some(pat) = pattern {
        for node in fs.files() {
            let rel = node.fullpath.strip_prefix("/").unwrap_or(&node.fullpath);
            if !rel.as_os_str().is_empty()
                && matches_pattern(rel, pat)
                && let InnerNode::Symlink(link) = &node.inner
            {
                let target = match rel.parent() {
                    Some(parent) if !parent.as_os_str().is_empty() => parent.join(&link.link),
                    _ => link.link.clone(),
                };
                extra_targets.push(target);
            }
        }
    }
    // Directories' permissions are set last: a read-only one could not be filled.
    let mut dirs = Vec::new();
    for node in fs.files() {
        let rel = node.fullpath.strip_prefix("/").unwrap_or(&node.fullpath);
        if rel.as_os_str().is_empty() {
            continue;
        }
        if let Some(pat) = pattern {
            let matched = matches_pattern(rel, pat)
                || extra_targets.iter().any(|t| rel == t || rel.starts_with(t) || t.starts_with(rel));
            if !matched {
                continue;
            }
        }
        let path = into.join(rel);
        let mode = u32::from(node.header.permissions) & 0o7777;
        match &node.inner {
            InnerNode::Dir(_) => {
                std::fs::create_dir_all(&path).map_err(|e| err(&path, e))?;
                dirs.push((path, mode));
            }
            InnerNode::File(file) => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| err(parent, e))?;
                }
                let mut out = File::create(&path).map_err(|e| err(&path, e))?;
                let mut reader = fs.file(file).reader();
                std::io::copy(&mut reader, &mut out).map_err(|e| err(&path, e))?;
                out.set_permissions(std::fs::Permissions::from_mode(mode)).map_err(|e| err(&path, e))?;
                if pattern.is_some() {
                    println!("squashfs-root/{}", rel.display());
                }
            }
            InnerNode::Symlink(link) => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| err(parent, e))?;
                }
                let _ = std::fs::remove_file(&path);
                std::os::unix::fs::symlink(&link.link, &path).map_err(|e| err(&path, e))?;
                if pattern.is_some() {
                    println!("squashfs-root/{}", rel.display());
                }
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
