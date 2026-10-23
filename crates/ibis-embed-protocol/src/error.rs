// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use std::fmt;
use std::io;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// The peer closed the connection (or the process on the other end exited).
    Disconnected,
    /// A frame longer than [`crate::MAX_FRAME_LEN`] was sent or announced.
    FrameTooLarge(usize),
    /// A frame was not a message of this protocol.
    Json(serde_json::Error),
    /// No (valid) handler connected before the deadline.
    Timeout,
    /// The handler process exited before it connected.
    ChildExited(Option<i32>),
    /// The handler speaks a different protocol version.
    ProtocolMismatch { ours: u32, theirs: u32 },
    /// The peer sent a message that is not valid at this point.
    Unexpected(String),
    /// Bad command line (handler side) or bad manifest.
    Invalid(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::Disconnected => f.write_str("the other side disconnected"),
            Error::FrameTooLarge(n) => write!(f, "frame of {n} bytes is over the limit"),
            Error::Json(e) => write!(f, "malformed message: {e}"),
            Error::Timeout => f.write_str("timed out"),
            Error::ChildExited(Some(code)) => write!(f, "the editor exited with code {code}"),
            Error::ChildExited(None) => f.write_str("the editor exited"),
            Error::ProtocolMismatch { ours, theirs } => {
                write!(f, "protocol version {theirs} is not supported (expected {ours})")
            }
            Error::Unexpected(m) => write!(f, "unexpected message: {m}"),
            Error::Invalid(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Json(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::UnexpectedEof
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted => Error::Disconnected,
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => Error::Timeout,
            _ => Error::Io(e),
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}
