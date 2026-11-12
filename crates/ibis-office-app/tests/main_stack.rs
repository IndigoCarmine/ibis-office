// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The executable asks Windows for a main thread's stack of 8 MiB (build.rs).
//! With the default 1 MiB, any .docx overflowed the stack on its first frame
//! in a debug build: `picture_panel` alone reserves 2.9 MB on entry.

#![cfg(all(windows, target_env = "msvc"))]

/// `SizeOfStackReserve` of a 64-bit PE file.
fn stack_reserve(exe: &[u8]) -> u64 {
    let pe = u32::from_le_bytes(exe[0x3c..0x40].try_into().unwrap()) as usize;
    assert_eq!(&exe[pe..pe + 4], b"PE\0\0");
    let optional = pe + 24;
    assert_eq!(u16::from_le_bytes(exe[optional..optional + 2].try_into().unwrap()), 0x20b, "PE32+");
    u64::from_le_bytes(exe[optional + 72..optional + 80].try_into().unwrap())
}

#[test]
fn main_thread_has_8_mib_of_stack() {
    let exe = std::fs::read(env!("CARGO_BIN_EXE_AcaudaOffice")).unwrap();
    assert!(stack_reserve(&exe) >= 8 * 1024 * 1024);
}
