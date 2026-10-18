// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Embedded objects on the Windows clipboard, as OLE applications put them
//! there and take them: the object's compound file ("Embed Source"), what it
//! is and how large ("Object Descriptor", MS-OLEDS / OBJECTDESCRIPTOR) and
//! pictures of it. GPUI's clipboard carries text and pictures only, so
//! these are read and written with the system's calls directly. Elsewhere
//! there is no such clipboard: nothing is read and nothing written.

/// An object copied in another application.
#[derive(Debug, Default)]
pub(crate) struct ClipObject {
    /// The object's compound file ("Embed Source").
    pub(crate) embed_source: Vec<u8>,
    /// Its size, from the "Object Descriptor", in EMU.
    pub(crate) size_emu: Option<(i64, i64)>,
}

/// An object to copy for other applications.
pub(crate) struct ObjectCopy<'a> {
    /// The object's compound file.
    pub(crate) embed_source: &'a [u8],
    pub(crate) clsid: [u8; 16],
    pub(crate) user_type: &'a str,
    /// Its size on the page, in EMU.
    pub(crate) size_emu: (i64, i64),
    /// Its picture, as an EMF and/or a PNG.
    pub(crate) emf: Option<&'a [u8]>,
    pub(crate) png: Option<&'a [u8]>,
}

/// HIMETRIC (0.01 mm) in an EMU.
const EMU_PER_HIMETRIC: i64 = 360;

/// The size in an OBJECTDESCRIPTOR (`sizel`, HIMETRIC), in EMU.
fn descriptor_size(d: &[u8]) -> Option<(i64, i64)> {
    let int = |at: usize| Some(i32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?) as i64);
    let (cx, cy) = (int(24)?, int(28)?);
    (cx > 0 && cy > 0).then_some((cx * EMU_PER_HIMETRIC, cy * EMU_PER_HIMETRIC))
}

/// `png` with the resolution (`pHYs`) at which its pixels cover `size_emu`:
/// Office sizes a pasted object by its picture, not by its descriptor.
fn png_at_size(png: &[u8], size_emu: (i64, i64)) -> Vec<u8> {
    // The signature and IHDR (13 bytes of data) come first in every PNG.
    const IHDR_END: usize = 8 + 8 + 13 + 4;
    let (Some(w), Some(h)) = (png.get(16..20), png.get(20..24)) else { return png.to_vec() };
    if png.get(12..16) != Some(b"IHDR") || size_emu.0 <= 0 || size_emu.1 <= 0 {
        return png.to_vec();
    }
    // Pixels per metre: 36 000 000 EMU to the metre.
    let per_metre = |px: &[u8], emu: i64| ((u32::from_be_bytes(px.try_into().unwrap()) as i64 * 36_000_000) / emu) as u32;
    let mut body = b"pHYs".to_vec();
    body.extend_from_slice(&per_metre(w, size_emu.0).to_be_bytes());
    body.extend_from_slice(&per_metre(h, size_emu.1).to_be_bytes());
    body.push(1); // the unit is the metre
    let mut out = png[..IHDR_END].to_vec();
    out.extend_from_slice(&9u32.to_be_bytes());
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
    // Any resolution the picture had is replaced.
    let mut at = IHDR_END;
    while let Some(len) = png.get(at..at + 4).map(|l| u32::from_be_bytes(l.try_into().unwrap()) as usize) {
        let end = at + 12 + len;
        let Some(chunk) = png.get(at..end) else { break };
        if &chunk[4..8] != b"pHYs" {
            out.extend_from_slice(chunk);
        }
        at = end;
    }
    out
}

/// The CRC of a PNG chunk (ISO 3309).
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { 0xEDB8_8320 ^ (crc >> 1) } else { crc >> 1 };
        }
    }
    !crc
}

/// An OBJECTDESCRIPTOR for `copy`: the class, drawn as content, its size,
/// and the type's name (twice: the full name and the source's).
fn descriptor(copy: &ObjectCopy) -> Vec<u8> {
    let name: Vec<u8> = copy.user_type.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
    const HEADER: u32 = 52;
    let mut d = Vec::with_capacity(HEADER as usize + 2 * name.len());
    d.extend_from_slice(&(HEADER + 2 * name.len() as u32).to_le_bytes());
    d.extend_from_slice(&copy.clsid);
    d.extend_from_slice(&1u32.to_le_bytes()); // DVASPECT_CONTENT
    d.extend_from_slice(&((copy.size_emu.0 / EMU_PER_HIMETRIC) as i32).to_le_bytes());
    d.extend_from_slice(&((copy.size_emu.1 / EMU_PER_HIMETRIC) as i32).to_le_bytes());
    d.extend_from_slice(&[0; 8]); // pointl
    d.extend_from_slice(&0u32.to_le_bytes()); // dwStatus
    d.extend_from_slice(&HEADER.to_le_bytes());
    d.extend_from_slice(&(HEADER + name.len() as u32).to_le_bytes());
    d.extend_from_slice(&name);
    d.extend_from_slice(&name);
    d
}

#[cfg(windows)]
mod sys {
    use std::ffi::c_void;

    pub const CF_ENHMETAFILE: u32 = 14;
    const GMEM_MOVEABLE: u32 = 0x0002;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn OpenClipboard(owner: isize) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn GetClipboardData(format: u32) -> isize;
        fn SetClipboardData(format: u32, data: isize) -> isize;
        fn RegisterClipboardFormatW(name: *const u16) -> u32;
        fn IsClipboardFormatAvailable(format: u32) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
        fn GlobalLock(mem: isize) -> *mut c_void;
        fn GlobalUnlock(mem: isize) -> i32;
        fn GlobalSize(mem: isize) -> usize;
        fn GlobalFree(mem: isize) -> isize;
    }
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn SetEnhMetaFileBits(size: u32, data: *const u8) -> isize;
        fn DeleteEnhMetaFile(emf: isize) -> i32;
    }

    pub fn format(name: &str) -> u32 {
        let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
        unsafe { RegisterClipboardFormatW(wide.as_ptr()) }
    }

    /// The clipboard, open until dropped (another application may hold it a moment).
    pub struct Open;

    impl Open {
        pub fn new() -> Option<Open> {
            for _ in 0..10 {
                if unsafe { OpenClipboard(0) } != 0 {
                    return Some(Open);
                }
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
            None
        }

        pub fn has(&self, format: u32) -> bool {
            unsafe { IsClipboardFormatAvailable(format) != 0 }
        }

        /// The bytes of a format held in global memory.
        pub fn get(&self, format: u32) -> Option<Vec<u8>> {
            if !self.has(format) {
                return None;
            }
            let mem = unsafe { GetClipboardData(format) };
            if mem == 0 {
                return None;
            }
            let size = unsafe { GlobalSize(mem) };
            let data = unsafe { GlobalLock(mem) } as *const u8;
            if data.is_null() {
                return None;
            }
            let bytes = unsafe { std::slice::from_raw_parts(data, size) }.to_vec();
            unsafe { GlobalUnlock(mem) };
            Some(bytes)
        }

        pub fn clear(&self) -> bool {
            unsafe { EmptyClipboard() != 0 }
        }

        /// `bytes` under `format`; the clipboard owns them once set.
        pub fn set(&self, format: u32, bytes: &[u8]) -> bool {
            let mem = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) };
            if mem == 0 {
                return false;
            }
            let data = unsafe { GlobalLock(mem) } as *mut u8;
            if data.is_null() {
                unsafe { GlobalFree(mem) };
                return false;
            }
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
                GlobalUnlock(mem);
            }
            if unsafe { SetClipboardData(format, mem) } == 0 {
                unsafe { GlobalFree(mem) };
                return false;
            }
            true
        }

        /// An enhanced metafile from its bytes.
        pub fn set_emf(&self, bytes: &[u8]) -> bool {
            let emf = unsafe { SetEnhMetaFileBits(bytes.len() as u32, bytes.as_ptr()) };
            if emf == 0 {
                return false;
            }
            if unsafe { SetClipboardData(CF_ENHMETAFILE, emf) } == 0 {
                unsafe { DeleteEnhMetaFile(emf) };
                return false;
            }
            true
        }
    }

    impl Drop for Open {
        fn drop(&mut self) {
            unsafe { CloseClipboard() };
        }
    }
}

/// The object on the clipboard, if another application copied one.
#[cfg(windows)]
pub(crate) fn read() -> Option<ClipObject> {
    let clip = sys::Open::new()?;
    Some(ClipObject {
        embed_source: clip.get(sys::format("Embed Source"))?,
        size_emu: clip.get(sys::format("Object Descriptor")).as_deref().and_then(descriptor_size),
    })
}

#[cfg(not(windows))]
pub(crate) fn read() -> Option<ClipObject> {
    None
}

/// `copy` on the clipboard, as OLE applications put an object there (in the
/// order they prefer it: the object, what it is, pictures).
/// No text: Office would paste that instead.
#[cfg(windows)]
pub(crate) fn write(copy: &ObjectCopy) -> Result<(), String> {
    let clip = sys::Open::new().ok_or("the clipboard is busy")?;
    if !clip.clear() {
        return Err("the clipboard could not be emptied".into());
    }
    let mut ok = clip.set(sys::format("Embed Source"), copy.embed_source);
    ok &= clip.set(sys::format("Object Descriptor"), &descriptor(copy));
    if let Some(emf) = copy.emf {
        clip.set_emf(emf);
    }
    if let Some(png) = copy.png {
        clip.set(sys::format("PNG"), &png_at_size(png, copy.size_emu));
    }
    if ok { Ok(()) } else { Err("the object could not be put on the clipboard".into()) }
}

#[cfg(not(windows))]
pub(crate) fn write(_: &ObjectCopy) -> Result<(), String> {
    Err("no OLE clipboard here".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_descriptor_reads_back_its_size() {
        let copy = ObjectCopy {
            embed_source: &[],
            clsid: [0x21; 16],
            user_type: "Example Drawing",
            size_emu: (1_394_640, 1_581_120),
            emf: None,
            png: None,
        };
        let d = descriptor(&copy);
        assert_eq!(u32::from_le_bytes(d[0..4].try_into().unwrap()) as usize, d.len());
        assert_eq!(&d[4..20], &[0x21; 16]);
        assert_eq!(descriptor_size(&d), Some((1_394_640, 1_581_120)));
        // The names, where the offsets say.
        let full = u32::from_le_bytes(d[44..48].try_into().unwrap()) as usize;
        let name: Vec<u16> = d[full..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&u| u != 0).collect();
        assert_eq!(String::from_utf16(&name).unwrap(), "Example Drawing");
        assert_eq!(descriptor_size(&[0; 10]), None);
    }

    #[test]
    fn a_pictures_resolution_makes_it_the_objects_size() {
        // A 2 × 1 PNG (IHDR, then IDAT and IEND as written by an encoder).
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        let chunk = |png: &mut Vec<u8>, kind: &[u8], data: &[u8]| {
            png.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let mut body = kind.to_vec();
            body.extend_from_slice(data);
            png.extend_from_slice(&body);
            png.extend_from_slice(&crc32(&body).to_be_bytes());
        };
        chunk(&mut png, b"IHDR", &[0, 0, 0, 2, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
        chunk(&mut png, b"pHYs", &[0, 0, 0, 1, 0, 0, 0, 1, 1]);
        chunk(&mut png, b"IEND", &[]);
        // Two pixels across an inch: 2 / 0.0254 m.
        let out = png_at_size(&png, (914_400, 457_200));
        let at = out.windows(4).position(|w| w == b"pHYs").unwrap();
        assert_eq!(out.windows(4).filter(|w| *w == b"pHYs").count(), 1, "replaced, not added");
        assert_eq!(u32::from_be_bytes(out[at + 4..at + 8].try_into().unwrap()), 78);
        assert_eq!(u32::from_be_bytes(out[at + 8..at + 12].try_into().unwrap()), 78);
        assert!(out.ends_with(&png[png.len() - 12..]), "IEND last");
        // The well-known CRC of IEND.
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    }
}
