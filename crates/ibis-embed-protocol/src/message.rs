// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The messages. Each enum is internally tagged by `"type"`, so a frame reads
//! like `{"type":"Open","content":{..},"title":"..","locale":"ja"}`.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Bumped on incompatible changes. Sent in [`HandlerMessage::Hello`].
pub const PROTOCOL_VERSION: u32 = 1;

/// A CLSID in the byte order it has in a CFB directory entry, sent as 32 hex
/// digits of those bytes (not the registry's `{…}` text form).
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Clsid(pub [u8; 16]);

impl fmt::Debug for Clsid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Clsid({})", hex(&self.0))
    }
}

impl Serialize for Clsid {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex(&self.0))
    }
}

impl<'de> Deserialize<'de> for Clsid {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        let bytes = unhex(&text)
            .filter(|b| b.len() == 16)
            .ok_or_else(|| serde::de::Error::custom("CLSID must be 32 hex digits"))?;
        let mut out = [0; 16];
        out.copy_from_slice(&bytes);
        Ok(Clsid(out))
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

/// The editable part of an OLE object: its class and its content streams.
/// The host keeps the bookkeeping streams whose names start with `\x01`
/// (`\x01CompObj`, `\x01Ole`, …) to itself and rebuilds them on save.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OleContent {
    pub clsid: Clsid,
    pub prog_id: String,
    /// The human-readable type name, as in `\x01CompObj` ("ChemDraw Document").
    #[serde(default)]
    pub user_type: String,
    pub streams: Vec<OleStream>,
}

impl OleContent {
    /// The stream at `path` ("CONTENTS", or "Storage/Stream" inside storages).
    pub fn stream(&self, path: &str) -> Option<&[u8]> {
        self.streams.iter().find(|s| s.path == path).map(|s| s.bytes.as_slice())
    }

    /// Replaces the stream at `path`, or adds it.
    pub fn set_stream(&mut self, path: &str, bytes: Vec<u8>) {
        match self.streams.iter_mut().find(|s| s.path == path) {
            Some(s) => s.bytes = bytes,
            None => self.streams.push(OleStream { path: path.to_owned(), bytes }),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OleStream {
    /// `/`-separated path from the object's root storage.
    pub path: String,
    #[serde(with = "b64")]
    pub bytes: Vec<u8>,
}

impl fmt::Debug for OleStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OleStream({:?}, {} bytes)", self.path, self.bytes.len())
    }
}

/// What the host draws for the object. At least one of the two is set; the
/// host prefers SVG when it can draw it.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preview {
    #[serde(default, with = "b64_opt", skip_serializing_if = "Option::is_none")]
    pub png: Option<Vec<u8>>,
    #[serde(default, with = "b64_opt", skip_serializing_if = "Option::is_none")]
    pub svg: Option<Vec<u8>>,
}

impl Preview {
    pub fn svg(svg: impl Into<Vec<u8>>) -> Self {
        Preview { png: None, svg: Some(svg.into()) }
    }

    pub fn png(png: impl Into<Vec<u8>>) -> Self {
        Preview { png: Some(png.into()), svg: None }
    }

    pub fn is_empty(&self) -> bool {
        self.png.is_none() && self.svg.is_none()
    }
}

impl fmt::Debug for Preview {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = |b: &Option<Vec<u8>>| b.as_ref().map(Vec::len);
        f.debug_struct("Preview")
            .field("png_bytes", &len(&self.png))
            .field("svg_bytes", &len(&self.svg))
            .finish()
    }
}

/// Host → handler.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HostMessage {
    // Interactive mode.
    /// Show this object for editing. `locale` is "ja" or "en".
    Open { content: OleContent, title: String, locale: String },
    /// The host is done (document closed, or the user asked to stop editing).
    /// The handler should close its window and reply [`HandlerMessage::Closed`].
    Close,
    /// Liveness check; the handler ignores it (a dead peer fails the write).
    Ping,

    // Headless mode: one of these, one reply, then the handler exits.
    /// Make an empty object → [`HandlerMessage::Created`].
    New,
    /// Make an object from a file's bytes (`extension` without the dot)
    /// → [`HandlerMessage::Created`].
    Import {
        extension: String,
        #[serde(with = "b64")]
        data: Vec<u8>,
    },
    /// Convert the object to a file format → [`HandlerMessage::Exported`].
    Export { content: OleContent, extension: String },
    /// Draw the object again → [`HandlerMessage::Rendered`].
    Render { content: OleContent },
}

/// Handler → host.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HandlerMessage {
    /// The first message on every connection.
    Hello { token: String, protocol: u32 },

    // Interactive mode.
    /// The user saved; the host replaces the object. May come many times.
    Update {
        content: OleContent,
        preview: Preview,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size_emu: Option<(i64, i64)>,
    },
    /// The editor window closed (on its own or after [`HostMessage::Close`]).
    Closed,

    // Headless replies.
    Created {
        content: OleContent,
        preview: Preview,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size_emu: Option<(i64, i64)>,
    },
    Exported {
        #[serde(with = "b64")]
        data: Vec<u8>,
    },
    Rendered {
        preview: Preview,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size_emu: Option<(i64, i64)>,
    },

    /// Either mode: the request failed. Shown to the user.
    Error { message: String },
}

mod b64 {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        STANDARD.decode(text.as_bytes()).map_err(serde::de::Error::custom)
    }
}

mod b64_opt {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match bytes {
            Some(b) => super::b64::serialize(b, s),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        #[derive(Deserialize)]
        struct Wrap(#[serde(with = "super::b64")] Vec<u8>);
        Ok(Option::<Wrap>::deserialize(d)?.map(|w| w.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clsid_and_bytes_are_text() {
        let msg = HostMessage::Import { extension: "cdx".into(), data: vec![0, 1, 255] };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"type":"Import","extension":"cdx","data":"AAH/"}"#);
        assert_eq!(serde_json::from_str::<HostMessage>(&json).unwrap(), msg);

        let mut clsid = [0u8; 16];
        clsid[0] = 0xab;
        clsid[15] = 0x01;
        let json = serde_json::to_string(&Clsid(clsid)).unwrap();
        assert_eq!(json, r#""ab000000000000000000000000000001""#);
        assert_eq!(serde_json::from_str::<Clsid>(&json).unwrap(), Clsid(clsid));
        assert!(serde_json::from_str::<Clsid>(r#""abc""#).is_err());
    }

    #[test]
    fn preview_omits_missing_formats() {
        let msg = HandlerMessage::Rendered { preview: Preview::svg("<svg/>"), size_emu: None };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"type":"Rendered","preview":{"svg":"PHN2Zy8+"}}"#);
        assert_eq!(serde_json::from_str::<HandlerMessage>(&json).unwrap(), msg);
    }
}
