// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ibisOffice's AppImage (Linux).
//!
//! An AppImage (type 2) is an ELF program, the runtime, with a SquashFS image
//! of the app's folder (the AppDir) after it, and `AI\x02` in the ELF
//! header's padding (bytes 8..11). AppImage's own runtime mounts the image
//! with FUSE every time it runs; it needs libfuse, which is LGPL (statically
//! linked in its current runtime), and FUSE, which Ubuntu 22.04 and later do
//! not install. ibisOffice's runtime (`bin/runtime.rs`) does without both:
//! it unpacks the image once per version into
//! `$XDG_CACHE_HOME/AcaudaOffice/app/<image id>/` (`~/.cache` without it) and
//! starts the app from there every time. `bin/pack.rs` makes the AppImage
//! from an AppDir (scripts/bundle-linux.sh).

use std::path::PathBuf;

/// What AppImage's type 2 puts in the ELF header's padding.
pub const MAGIC: [u8; 3] = *b"AI\x02";
/// Where in the ELF header.
pub const MAGIC_OFFSET: usize = 8;

/// Where the image starts in an AppImage: the end of its ELF part, which is
/// the end of the section header table (the last thing a linker writes).
pub fn elf_end(data: &[u8]) -> Option<u64> {
    if data.len() < 64 || data[..4] != *b"\x7fELF" {
        return None;
    }
    // ELFCLASS64, little-endian (x86_64 and aarch64).
    if data[4] != 2 || data[5] != 1 {
        return None;
    }
    let u16_at = |at: usize| u64::from(u16::from_le_bytes([data[at], data[at + 1]]));
    let u64_at = |at: usize| u64::from_le_bytes(data[at..at + 8].try_into().unwrap());
    let shoff = u64_at(0x28);
    let shentsize = u16_at(0x3a);
    let shnum = u16_at(0x3c);
    Some(shoff + shentsize * shnum)
}

/// The image's identity, for the folder it is unpacked into: its creation
/// time and size from its superblock, which differ between any two builds.
/// `superblock` is the image's first 96 bytes.
pub fn image_id(superblock: &[u8]) -> Option<String> {
    if superblock.len() < 96 || superblock[..4] != *b"hsqs" {
        return None;
    }
    let mod_time = u32::from_le_bytes(superblock[8..12].try_into().unwrap());
    let bytes_used = u64::from_le_bytes(superblock[40..48].try_into().unwrap());
    Some(format!("{mod_time:08x}-{bytes_used:x}"))
}

/// The folder the versions are unpacked into: `$XDG_CACHE_HOME/AcaudaOffice/app`
/// (`~/.cache/AcaudaOffice/app`).
pub fn cache_root() -> Option<PathBuf> {
    let env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let cache = env("XDG_CACHE_HOME").or_else(|| Some(env("HOME")?.join(".cache")))?;
    Some(cache.join("AcaudaOffice").join("app"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn the_end_of_this_test_program_is_found() {
        let exe = std::fs::read(std::env::current_exe().unwrap()).unwrap();
        let end = elf_end(&exe).unwrap();
        assert!(end <= exe.len() as u64 && end > 64);
    }

    #[test]
    fn not_an_elf_file() {
        assert_eq!(elf_end(b"#!/bin/sh\n"), None);
        assert_eq!(elf_end(&[0; 64]), None);
    }

    #[test]
    fn image_ids_come_from_the_superblock() {
        let mut sb = vec![0u8; 96];
        sb[..4].copy_from_slice(b"hsqs");
        sb[8..12].copy_from_slice(&0x6543_2100u32.to_le_bytes());
        sb[40..48].copy_from_slice(&123_456u64.to_le_bytes());
        assert_eq!(image_id(&sb).as_deref(), Some("65432100-1e240"));
        sb[0] = b'x';
        assert_eq!(image_id(&sb), None);
    }
}
