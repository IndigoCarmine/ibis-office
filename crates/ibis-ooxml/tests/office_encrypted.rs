// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Files Office itself protected with a password (the password of these test
//! files is `ibis123`): opened, and what ibisOffice writes is made like them.

#[path = "../../../test/fixtures.rs"]
mod fixtures;

use ibis_ooxml::{cfb, encryption, Package};

/// The test file, or `None` (reported as skipped) when the private test data
/// is not there.
fn file(name: &str) -> Option<Vec<u8>> {
    fixtures::read(&format!("test/editing/{name}"))
}

#[test]
fn office_files_are_opened() {
    for (name, main_part) in [("xlsx-encrypted.xlsx", "xl/workbook.xml"), ("docx-encrypted.docx", "word/document.xml")] {
        let Some(data) = file(name) else { continue };
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
    let Some(theirs) = file("xlsx-encrypted.xlsx") else { return };
    let ours = encryption::encrypt(b"PK", "ibis123", &mut |b: &mut [u8]| b.fill(7));
    for leaf in ["Version", "DataSpaceMap", "StrongEncryptionDataSpace", "\u{6}Primary"] {
        let (a, b) = (cfb::stream(&theirs, leaf).unwrap(), cfb::stream(&ours, leaf).unwrap());
        assert_eq!(a, b, "{leaf:?}\n theirs {a:02x?}\n ours   {b:02x?}");
    }
}
