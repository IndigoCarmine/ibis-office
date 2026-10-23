// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ZIP archives as OOXML packages use them: reading the central directory
//! and stored or deflated entries, and writing an archive whose entries are
//! either copied as they were compressed or compressed anew.
//!
//! No encryption, no multi-disk archives, no ZIP64 (packages over 4 GiB are
//! rejected).

use std::sync::Arc;

use crate::Error;

const END_OF_CENTRAL_DIRECTORY: u32 = 0x0605_4b50;
const CENTRAL_DIRECTORY_HEADER: u32 = 0x0201_4b50;
const LOCAL_FILE_HEADER: u32 = 0x0403_4b50;

/// Stored (no compression).
pub const STORED: u16 = 0;
/// Deflate.
pub const DEFLATED: u16 = 8;

/// General purpose flag: sizes and CRC follow the data in a descriptor.
const FLAG_DATA_DESCRIPTOR: u16 = 1 << 3;
/// General purpose flag: the name is UTF-8.
const FLAG_UTF8: u16 = 1 << 11;

/// An entry's header fields, kept to write it back as it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryMeta {
    pub version_made_by: u16,
    pub version_needed: u16,
    pub flags: u16,
    pub time: u16,
    pub date: u16,
    pub internal_attributes: u16,
    pub external_attributes: u32,
}

impl EntryMeta {
    /// For a new entry: 1980-01-01 00:00, as Office writes, so that the same
    /// content always makes the same file.
    pub fn new() -> Self {
        Self { version_made_by: 20, version_needed: 20, flags: 0, time: 0, date: 0x0021, internal_attributes: 0, external_attributes: 0 }
    }
}

impl Default for EntryMeta {
    fn default() -> Self {
        Self::new()
    }
}

/// An entry of an archive that was read: where its compressed data is.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub method: u16,
    pub crc: u32,
    pub compressed_size: usize,
    pub size: usize,
    pub meta: EntryMeta,
    /// Offset of the compressed data in the archive.
    data_offset: usize,
    /// The local header (with name and extra field) as written.
    local_header: std::ops::Range<usize>,
    /// The data descriptor after the data, if any, as written.
    descriptor: std::ops::Range<usize>,
    /// The central directory record as written.
    central: std::ops::Range<usize>,
}

/// An archive read from bytes.
#[derive(Clone)]
pub struct Archive {
    data: Arc<[u8]>,
    /// In the order of the central directory.
    entries: Vec<Entry>,
    /// The archive's comment.
    comment: Vec<u8>,
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

impl Archive {
    pub fn new(data: Arc<[u8]>) -> Result<Self, Error> {
        let bad = || Error::new("ZIP 形式ではないか、壊れています");
        let d = &data[..];
        // The end record is 22 bytes plus a comment of up to 64 KiB.
        let search_from = d.len().saturating_sub(22 + 0xffff);
        let end = (search_from..d.len().saturating_sub(21))
            .rev()
            .find(|&i| u32_at(d, i) == Some(END_OF_CENTRAL_DIRECTORY))
            .ok_or_else(bad)?;
        let count = u16_at(d, end + 10).ok_or_else(bad)? as usize;
        let mut at = u32_at(d, end + 16).ok_or_else(bad)? as usize;
        if at == 0xffff_ffff || count == 0xffff {
            return Err(Error::new("ZIP64 形式には対応していません"));
        }
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            if u32_at(d, at) != Some(CENTRAL_DIRECTORY_HEADER) {
                return Err(bad());
            }
            let f16 = |offset| u16_at(d, at + offset).ok_or_else(bad);
            let f32 = |offset| u32_at(d, at + offset).ok_or_else(bad);
            let meta = EntryMeta {
                version_made_by: f16(4)?,
                version_needed: f16(6)?,
                flags: f16(8)?,
                time: f16(12)?,
                date: f16(14)?,
                internal_attributes: f16(36)?,
                external_attributes: f32(38)?,
            };
            let method = f16(10)?;
            let crc = f32(16)?;
            let compressed_size = f32(20)? as usize;
            let size = f32(24)? as usize;
            let name_len = f16(28)? as usize;
            let extra_len = f16(30)? as usize;
            let comment_len = f16(32)? as usize;
            let local = f32(42)? as usize;
            // The whole record, comment included, must lie in the data: it is
            // copied as is when the part is saved unchanged.
            let central_len = 46 + name_len + extra_len + comment_len;
            if at + central_len > d.len() {
                return Err(bad());
            }
            let name = d.get(at + 46..at + 46 + name_len).ok_or_else(bad)?;
            let name = String::from_utf8_lossy(name).into_owned();
            if u32_at(d, local) != Some(LOCAL_FILE_HEADER) {
                return Err(Error::new(format!("{name} の位置が正しくありません")));
            }
            // The local header repeats name and extra field with its own lengths.
            let local_name = u16_at(d, local + 26).ok_or_else(bad)? as usize;
            let local_extra = u16_at(d, local + 28).ok_or_else(bad)? as usize;
            let data_offset = local + 30 + local_name + local_extra;
            // Checked: the sizes are 32-bit and so is usize on wasm32.
            let data_end = match data_offset.checked_add(compressed_size) {
                Some(end) if end <= d.len() => end,
                _ => return Err(Error::new(format!("{name} が途中で切れています"))),
            };
            // A data descriptor: CRC and sizes, with or without its signature.
            let descriptor_len = if meta.flags & FLAG_DATA_DESCRIPTOR == 0 {
                0
            } else if u32_at(d, data_end) == Some(0x0807_4b50) {
                16
            } else {
                12
            };
            entries.push(Entry {
                name,
                method,
                crc,
                compressed_size,
                size,
                meta,
                data_offset,
                local_header: local..data_offset,
                descriptor: data_end..(data_end + descriptor_len).min(d.len()),
                central: at..at + central_len,
            });
            at += central_len;
        }
        let comment_len = u16_at(d, end + 20).unwrap_or(0) as usize;
        let comment = d.get(end + 22..end + 22 + comment_len).unwrap_or(&[]).to_vec();
        Ok(Self { data, entries, comment })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The archive's comment.
    pub fn comment(&self) -> &[u8] {
        &self.comment
    }

    /// The compressed bytes of `entry`, as stored.
    pub fn raw(&self, entry: &Entry) -> &[u8] {
        &self.data[entry.data_offset..entry.data_offset + entry.compressed_size]
    }

    /// The uncompressed bytes of `entry`.
    pub fn read(&self, entry: &Entry) -> Result<Vec<u8>, Error> {
        let bad = || Error::new(format!("{} を展開できません", entry.name));
        let raw = self.raw(entry);
        let bytes = match entry.method {
            STORED => raw.to_vec(),
            DEFLATED => miniz_oxide::inflate::decompress_to_vec_with_limit(raw, entry.size.max(1)).map_err(|_| bad())?,
            _ => return Err(Error::new(format!("{} の圧縮方式には対応していません", entry.name))),
        };
        if bytes.len() != entry.size {
            return Err(bad());
        }
        Ok(bytes)
    }
}

/// Writes an archive, entry by entry.
pub struct Writer {
    out: Vec<u8>,
    central: Vec<u8>,
    count: usize,
    comment: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self { out: Vec::new(), central: Vec::new(), count: 0, comment: Vec::new() }
    }

    /// Set the archive's comment.
    pub fn set_comment(&mut self, comment: &[u8]) {
        self.comment = comment[..comment.len().min(0xffff)].to_vec();
    }

    /// Copy `entry` of `archive` exactly as it was written there: local
    /// header (with its extra field), data, data descriptor and central
    /// directory record (only its offset changes).
    pub fn copy(&mut self, archive: &Archive, entry: &Entry) -> Result<(), Error> {
        if self.out.len() > u32::MAX as usize {
            return Err(Error::new("4 GiB を超えるファイルは保存できません"));
        }
        let offset = self.out.len() as u32;
        let d = &archive.data;
        self.out.extend_from_slice(&d[entry.local_header.clone()]);
        self.out.extend_from_slice(archive.raw(entry));
        self.out.extend_from_slice(&d[entry.descriptor.clone()]);
        let start = self.central.len();
        self.central.extend_from_slice(&d[entry.central.clone()]);
        self.central[start + 42..start + 46].copy_from_slice(&offset.to_le_bytes());
        self.count += 1;
        Ok(())
    }

    /// Add an entry whose data is already compressed with `method`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_raw(&mut self, name: &str, meta: EntryMeta, method: u16, crc: u32, size: usize, raw: &[u8]) -> Result<(), Error> {
        if self.out.len() > u32::MAX as usize || raw.len() > u32::MAX as usize || size > u32::MAX as usize {
            return Err(Error::new("4 GiB を超えるファイルは保存できません"));
        }
        // Sizes and CRC go in the headers; no data descriptor follows.
        let mut flags = meta.flags & !FLAG_DATA_DESCRIPTOR;
        if !name.is_ascii() {
            flags |= FLAG_UTF8;
        }
        let offset = self.out.len() as u32;
        let name_bytes = name.as_bytes();
        let o = &mut self.out;
        o.extend(LOCAL_FILE_HEADER.to_le_bytes());
        o.extend(meta.version_needed.to_le_bytes());
        o.extend(flags.to_le_bytes());
        o.extend(method.to_le_bytes());
        o.extend(meta.time.to_le_bytes());
        o.extend(meta.date.to_le_bytes());
        o.extend(crc.to_le_bytes());
        o.extend((raw.len() as u32).to_le_bytes());
        o.extend((size as u32).to_le_bytes());
        o.extend((name_bytes.len() as u16).to_le_bytes());
        o.extend(0u16.to_le_bytes());
        o.extend(name_bytes);
        o.extend(raw);

        let c = &mut self.central;
        c.extend(CENTRAL_DIRECTORY_HEADER.to_le_bytes());
        c.extend(meta.version_made_by.to_le_bytes());
        c.extend(meta.version_needed.to_le_bytes());
        c.extend(flags.to_le_bytes());
        c.extend(method.to_le_bytes());
        c.extend(meta.time.to_le_bytes());
        c.extend(meta.date.to_le_bytes());
        c.extend(crc.to_le_bytes());
        c.extend((raw.len() as u32).to_le_bytes());
        c.extend((size as u32).to_le_bytes());
        c.extend((name_bytes.len() as u16).to_le_bytes());
        c.extend(0u16.to_le_bytes()); // extra
        c.extend(0u16.to_le_bytes()); // comment
        c.extend(0u16.to_le_bytes()); // disk
        c.extend(meta.internal_attributes.to_le_bytes());
        c.extend(meta.external_attributes.to_le_bytes());
        c.extend(offset.to_le_bytes());
        c.extend(name_bytes);
        self.count += 1;
        Ok(())
    }

    /// Add an entry, compressing `data` with deflate (or storing it when it is empty).
    pub fn add(&mut self, name: &str, meta: EntryMeta, data: &[u8]) -> Result<(), Error> {
        let crc = crc32(data);
        if data.is_empty() {
            return self.add_raw(name, meta, STORED, crc, 0, data);
        }
        // A big part (a sheet of tens of thousands of cells) is compressed fast: saving it took seconds at level 6, for a few percent more size.
        let compressed = miniz_oxide::deflate::compress_to_vec(data, if data.len() > 2_000_000 { 1 } else { 6 });
        self.add_raw(name, meta, DEFLATED, crc, data.len(), &compressed)
    }

    pub fn finish(mut self) -> Result<Vec<u8>, Error> {
        if self.count > 0xfffe {
            return Err(Error::new("パーツが多すぎて保存できません"));
        }
        let offset = self.out.len() as u32;
        let size = self.central.len() as u32;
        self.out.extend_from_slice(&self.central);
        let o = &mut self.out;
        o.extend(END_OF_CENTRAL_DIRECTORY.to_le_bytes());
        o.extend(0u16.to_le_bytes());
        o.extend(0u16.to_le_bytes());
        o.extend((self.count as u16).to_le_bytes());
        o.extend((self.count as u16).to_le_bytes());
        o.extend(size.to_le_bytes());
        o.extend(offset.to_le_bytes());
        o.extend((self.comment.len() as u16).to_le_bytes());
        o.extend_from_slice(&self.comment);
        Ok(self.out)
    }
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

/// CRC-32 (IEEE 802.3), as ZIP uses.
pub fn crc32(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        for (i, entry) in table.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *entry = c;
        }
        table
    });
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xff) as usize] ^ (crc >> 8);
    }
    crc ^ 0xffff_ffff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_of_known_input() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn written_archive_reads_back() {
        let mut w = Writer::new();
        w.add("[Content_Types].xml", EntryMeta::new(), b"<Types/>").unwrap();
        w.add("a/\u{3042}.xml", EntryMeta::new(), &b"<x>hello hello hello</x>".repeat(50)).unwrap();
        w.add("empty", EntryMeta::new(), b"").unwrap();
        let bytes = w.finish().unwrap();
        let archive = Archive::new(bytes.into()).unwrap();
        let names: Vec<_> = archive.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["[Content_Types].xml", "a/\u{3042}.xml", "empty"]);
        assert_eq!(archive.read(&archive.entries()[1]).unwrap(), b"<x>hello hello hello</x>".repeat(50));
        assert_eq!(archive.entries()[1].method, DEFLATED);
        assert_eq!(archive.read(&archive.entries()[2]).unwrap(), b"");
    }

    #[test]
    fn raw_copy_keeps_the_bytes() {
        let mut w = Writer::new();
        w.add("x", EntryMeta { time: 0x1234, date: 0x5678, ..EntryMeta::new() }, &b"abc".repeat(100)).unwrap();
        let first = w.finish().unwrap();
        let a = Archive::new(first.clone().into()).unwrap();
        let e = &a.entries()[0];
        let mut w = Writer::new();
        w.add_raw(&e.name, e.meta, e.method, e.crc, e.size, a.raw(e)).unwrap();
        assert_eq!(w.finish().unwrap(), first);
    }

    #[test]
    fn verbatim_copy_keeps_extra_fields_and_descriptors() {
        // An entry with a padded local header and a data descriptor, as Excel writes.
        let mut z = Vec::new();
        let body = b"hello";
        let crc = crc32(body);
        z.extend(LOCAL_FILE_HEADER.to_le_bytes());
        z.extend([20, 0, 8, 0, 0, 0, 0, 0, 0, 0]); // version, flags (descriptor), method, time, date
        z.extend([0u8; 12]); // crc and sizes in the descriptor
        z.extend(1u16.to_le_bytes());
        z.extend(4u16.to_le_bytes());
        z.extend(b"a");
        z.extend([0u8; 4]);
        z.extend(body);
        z.extend(0x0807_4b50u32.to_le_bytes());
        z.extend(crc.to_le_bytes());
        z.extend(5u32.to_le_bytes());
        z.extend(5u32.to_le_bytes());
        let cd = z.len() as u32;
        z.extend(CENTRAL_DIRECTORY_HEADER.to_le_bytes());
        z.extend([20, 0, 20, 0, 8, 0, 0, 0, 0, 0, 0, 0]);
        z.extend(crc.to_le_bytes());
        z.extend(5u32.to_le_bytes());
        z.extend(5u32.to_le_bytes());
        z.extend(1u16.to_le_bytes());
        z.extend([0; 12]);
        z.extend(0u32.to_le_bytes());
        z.extend(b"a");
        let cd_len = z.len() as u32 - cd;
        z.extend(END_OF_CENTRAL_DIRECTORY.to_le_bytes());
        z.extend([0, 0, 0, 0, 1, 0, 1, 0]);
        z.extend(cd_len.to_le_bytes());
        z.extend(cd.to_le_bytes());
        z.extend(2u16.to_le_bytes());
        z.extend(b"hi");
        let a = Archive::new(z.clone().into()).unwrap();
        assert_eq!(a.read(&a.entries()[0]).unwrap(), body);
        let mut w = Writer::new();
        w.set_comment(a.comment());
        w.copy(&a, &a.entries()[0]).unwrap();
        assert_eq!(w.finish().unwrap(), z);
    }

    /// One stored entry "a" whose central record has the extra field and
    /// comment given, `comment_len` as its declared comment length, and
    /// `compressed_size` as the declared size of its 5 bytes of data.
    fn one_entry(central_extra: &[u8], central_comment: &[u8], comment_len: u16, compressed_size: u32) -> Vec<u8> {
        let body = b"hello";
        let crc = crc32(body);
        let mut z = Vec::new();
        z.extend(LOCAL_FILE_HEADER.to_le_bytes());
        z.extend([20, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // version, flags, method, time, date
        z.extend(crc.to_le_bytes());
        z.extend(compressed_size.to_le_bytes());
        z.extend(5u32.to_le_bytes());
        z.extend(1u16.to_le_bytes());
        z.extend(0u16.to_le_bytes());
        z.extend(b"a");
        z.extend(body);
        let cd = z.len() as u32;
        z.extend(CENTRAL_DIRECTORY_HEADER.to_le_bytes());
        z.extend([20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend(crc.to_le_bytes());
        z.extend(compressed_size.to_le_bytes());
        z.extend(5u32.to_le_bytes());
        z.extend(1u16.to_le_bytes());
        z.extend((central_extra.len() as u16).to_le_bytes());
        z.extend(comment_len.to_le_bytes());
        z.extend([0; 8]); // disk, internal and external attributes
        z.extend(0u32.to_le_bytes());
        z.extend(b"a");
        z.extend(central_extra);
        z.extend(central_comment);
        let cd_len = z.len() as u32 - cd;
        z.extend(END_OF_CENTRAL_DIRECTORY.to_le_bytes());
        z.extend([0, 0, 0, 0, 1, 0, 1, 0]);
        z.extend(cd_len.to_le_bytes());
        z.extend(cd.to_le_bytes());
        z.extend(0u16.to_le_bytes());
        z
    }

    #[test]
    fn verbatim_copy_keeps_central_extra_field_and_comment() {
        let z = one_entry(&[0xfe, 0xca, 0, 0], b"note", 4, 5);
        let a = Archive::new(z.clone().into()).unwrap();
        assert_eq!(a.read(&a.entries()[0]).unwrap(), b"hello");
        let mut w = Writer::new();
        w.copy(&a, &a.entries()[0]).unwrap();
        assert_eq!(w.finish().unwrap(), z);
    }

    #[test]
    fn rejects_central_record_past_the_end() {
        // The last record's comment would run past the end of the file.
        assert!(Archive::new(one_entry(&[], &[], 0xffff, 5).into()).is_err());
    }

    #[test]
    fn rejects_data_past_the_end() {
        assert!(Archive::new(one_entry(&[], &[], 0, 1000).into()).is_err());
        assert!(Archive::new(one_entry(&[], &[], 0, u32::MAX).into()).is_err());
    }

    #[test]
    fn rejects_non_zip() {
        assert!(Archive::new(Arc::from(&b"not a zip file at all, just some text"[..])).is_err());
    }
}
