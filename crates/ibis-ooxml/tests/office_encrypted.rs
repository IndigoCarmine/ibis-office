// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Files Office itself protected with a password (the password of these test
//! files is `ibis123`): opened, and what ibisOffice writes is made like them.

use ibis_ooxml::{cfb, encryption, Package};

fn file(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn office_files_are_opened() {
    for (name, main_part) in [("xlsx-encrypted.xlsx", "xl/workbook.xml"), ("docx-encrypted.docx", "word/document.xml")] {
        let data = file(name);
        assert!(encryption::is_encrypted(&data), "{name}");
        assert_eq!(encryption::decrypt(&data, "ibis124"), Err(encryption::DecryptError::WrongPassword), "{name}");
        let package = encryption::decrypt(&data, "ibis123").unwrap_or_else(|e| panic!("{name}: {e}"));
        let opened = Package::open(package).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(opened.contains(main_part), "{name}");
    }
}

/// The data space streams are the same in every file Office writes: ours are alike.
#[test]
fn data_spaces_are_those_office_writes() {
    let theirs = file("xlsx-encrypted.xlsx");
    let ours = encryption::encrypt(b"PK", "ibis123", &mut |b: &mut [u8]| b.fill(7));
    for leaf in ["Version", "DataSpaceMap", "StrongEncryptionDataSpace", "\u{6}Primary"] {
        let (a, b) = (cfb::stream(&theirs, leaf).unwrap(), cfb::stream(&ours, leaf).unwrap());
        assert_eq!(a, b, "{leaf:?}\n theirs {a:02x?}\n ours   {b:02x?}");
    }
}
