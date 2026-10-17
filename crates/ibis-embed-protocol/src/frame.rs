// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Framing: each message is a 4-byte little-endian length followed by that
//! many bytes of JSON.

use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{Error, Result};

/// The largest frame either side sends or accepts: 256 MiB, room for an
/// object of about 190 MiB after base64. A longer length prefix means a
/// broken or hostile peer, and nothing is allocated for it.
pub const MAX_FRAME_LEN: usize = 256 << 20;

/// Writes one frame. Fails with [`Error::FrameTooLarge`] (writing nothing)
/// when `payload` is longer than `max_len`.
pub fn write_frame(w: &mut impl Write, payload: &[u8], max_len: usize) -> Result<()> {
    if payload.len() > max_len || u32::try_from(payload.len()).is_err() {
        return Err(Error::FrameTooLarge(payload.len()));
    }
    let mut buf = Vec::with_capacity(4 + payload.len());
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    buf.extend_from_slice(payload);
    w.write_all(&buf)?;
    w.flush()?;
    Ok(())
}

/// Reads one frame. A peer that closed the connection gives
/// [`Error::Disconnected`]; a length over `max_len` gives
/// [`Error::FrameTooLarge`], after which the stream is out of step and should
/// be dropped.
pub fn read_frame(r: &mut impl Read, max_len: usize) -> Result<Vec<u8>> {
    let mut len = [0; 4];
    r.read_exact(&mut len)?;
    let len = u32::from_le_bytes(len) as usize;
    if len > max_len {
        return Err(Error::FrameTooLarge(len));
    }
    let mut payload = vec![0; len];
    r.read_exact(&mut payload)?;
    Ok(payload)
}

/// A connection that carries frames. Implemented over any byte stream by
/// [`StreamTransport`] and in memory by [`MemoryTransport`], so code that
/// drives a session can be tested without a process or a socket.
pub trait Transport: Send {
    fn send_frame(&mut self, payload: &[u8]) -> Result<()>;
    fn recv_frame(&mut self) -> Result<Vec<u8>>;

    fn send<M: Serialize>(&mut self, msg: &M) -> Result<()>
    where
        Self: Sized,
    {
        self.send_frame(&serde_json::to_vec(msg)?)
    }

    fn recv<M: DeserializeOwned>(&mut self) -> Result<M>
    where
        Self: Sized,
    {
        Ok(serde_json::from_slice(&self.recv_frame()?)?)
    }
}

/// Frames over a byte stream (a local socket, or anything `Read + Write`).
pub struct StreamTransport<S> {
    stream: S,
    max_len: usize,
}

impl<S: Read + Write + Send> StreamTransport<S> {
    pub fn new(stream: S) -> Self {
        StreamTransport { stream, max_len: MAX_FRAME_LEN }
    }

    /// Lowers (or raises) the frame size limit, mainly for tests.
    pub fn with_max_len(mut self, max_len: usize) -> Self {
        self.max_len = max_len;
        self
    }

    pub fn get_ref(&self) -> &S {
        &self.stream
    }

    pub fn into_inner(self) -> S {
        self.stream
    }
}

impl<S: Read + Write + Send> Transport for StreamTransport<S> {
    fn send_frame(&mut self, payload: &[u8]) -> Result<()> {
        write_frame(&mut self.stream, payload, self.max_len)
    }

    fn recv_frame(&mut self) -> Result<Vec<u8>> {
        read_frame(&mut self.stream, self.max_len)
    }
}

/// One end of an in-memory connection; see [`MemoryTransport::pair`].
/// Dropping one end makes the other's `recv_frame` fail with
/// [`Error::Disconnected`], like a peer process exiting.
pub struct MemoryTransport {
    tx: mpsc::Sender<Vec<u8>>,
    rx: mpsc::Receiver<Vec<u8>>,
    max_len: usize,
}

impl MemoryTransport {
    pub fn pair() -> (MemoryTransport, MemoryTransport) {
        let (a_tx, b_rx) = mpsc::channel();
        let (b_tx, a_rx) = mpsc::channel();
        let end = |tx, rx| MemoryTransport { tx, rx, max_len: MAX_FRAME_LEN };
        (end(a_tx, a_rx), end(b_tx, b_rx))
    }

    pub fn with_max_len(mut self, max_len: usize) -> Self {
        self.max_len = max_len;
        self
    }

    /// Like `recv_frame`, but gives [`Error::Timeout`] after `timeout`.
    pub fn recv_frame_timeout(&mut self, timeout: Duration) -> Result<Vec<u8>> {
        self.rx.recv_timeout(timeout).map_err(|e| match e {
            mpsc::RecvTimeoutError::Timeout => Error::Timeout,
            mpsc::RecvTimeoutError::Disconnected => Error::Disconnected,
        })
    }
}

impl Transport for MemoryTransport {
    fn send_frame(&mut self, payload: &[u8]) -> Result<()> {
        if payload.len() > self.max_len {
            return Err(Error::FrameTooLarge(payload.len()));
        }
        self.tx.send(payload.to_vec()).map_err(|_| Error::Disconnected)
    }

    fn recv_frame(&mut self) -> Result<Vec<u8>> {
        let frame = self.rx.recv().map_err(|_| Error::Disconnected)?;
        if frame.len() > self.max_len {
            return Err(Error::FrameTooLarge(frame.len()));
        }
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::{HandlerMessage, HostMessage, OleContent, Preview};

    #[test]
    fn frames_round_trip() {
        let mut content = OleContent { prog_id: "Mock.Object.1".into(), ..Default::default() };
        content.set_stream("CONTENTS", b"mock\0\xff".to_vec());
        let msgs = [
            HostMessage::Open { content: content.clone(), title: "Slide 1".into(), locale: "ja".into() },
            HostMessage::Ping,
            HostMessage::Close,
        ];
        let mut wire = Vec::new();
        for m in &msgs {
            write_frame(&mut wire, &serde_json::to_vec(m).unwrap(), MAX_FRAME_LEN).unwrap();
        }
        // The first frame's prefix is its length, little-endian.
        let first_len = u32::from_le_bytes(wire[..4].try_into().unwrap()) as usize;
        assert_eq!(&wire[4..5], b"{");
        assert_eq!(wire[4 + first_len], serde_json::to_vec(&msgs[1]).unwrap().len() as u8);

        let mut t = StreamTransport::new(Cursor::new(wire));
        for m in &msgs {
            assert_eq!(&t.recv::<HostMessage>().unwrap(), m);
        }
        assert!(matches!(t.recv::<HostMessage>(), Err(Error::Disconnected)));
    }

    #[test]
    fn oversized_frames_are_refused_both_ways() {
        // Sending: nothing is written.
        let mut wire = Vec::new();
        let err = write_frame(&mut wire, &[b'x'; 101], 100).unwrap_err();
        assert!(matches!(err, Error::FrameTooLarge(101)));
        assert!(wire.is_empty());

        // Receiving: a huge announced length is refused before allocating.
        let mut wire = Cursor::new(u32::MAX.to_le_bytes().to_vec());
        assert!(matches!(read_frame(&mut wire, MAX_FRAME_LEN), Err(Error::FrameTooLarge(_))));

        let mut wire = Vec::new();
        write_frame(&mut wire, &[b'x'; 101], 1000).unwrap();
        assert!(matches!(read_frame(&mut Cursor::new(wire), 100), Err(Error::FrameTooLarge(101))));
    }

    #[test]
    fn truncated_frame_is_a_disconnect() {
        let mut wire = Vec::new();
        write_frame(&mut wire, b"{\"type\":\"Ping\"}", MAX_FRAME_LEN).unwrap();
        wire.truncate(wire.len() - 3);
        assert!(matches!(read_frame(&mut Cursor::new(wire), MAX_FRAME_LEN), Err(Error::Disconnected)));
    }

    #[test]
    fn memory_transport_round_trip_and_disconnect() {
        let (mut host, mut handler) = MemoryTransport::pair();
        handler.send(&HandlerMessage::Rendered { preview: Preview::svg("<svg/>"), size_emu: Some((1, 2)) }).unwrap();
        assert!(matches!(host.recv::<HandlerMessage>().unwrap(), HandlerMessage::Rendered { size_emu: Some((1, 2)), .. }));
        assert!(matches!(host.recv_frame_timeout(Duration::from_millis(10)), Err(Error::Timeout)));
        drop(handler);
        assert!(matches!(host.recv_frame(), Err(Error::Disconnected)));
        assert!(matches!(host.send(&HostMessage::Ping), Err(Error::Disconnected)));

        let (a, _b) = MemoryTransport::pair();
        let mut a = a.with_max_len(4);
        assert!(matches!(a.send_frame(b"12345"), Err(Error::FrameTooLarge(5))));
    }
}
