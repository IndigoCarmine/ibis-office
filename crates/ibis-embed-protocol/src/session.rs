// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The two ends of a session over a local socket. Everything here blocks, so
//! the host runs it on a background thread (GPUI's executor is not tokio).

use std::ffi::OsString;
use std::io;
use std::marker::PhantomData;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{
    GenericFilePath, GenericNamespaced, ListenerNonblockingMode, ListenerOptions, Name, RecvHalf,
    SendHalf,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::clock::{self, Instant};
use crate::frame::{MAX_FRAME_LEN, read_frame, write_frame};
use crate::message::hex;
use crate::{Error, HandlerMessage, HostMessage, PROTOCOL_VERSION, Result, StreamTransport, Transport};

const ARG_SOCKET: &str = "--ibis-embed";
const ARG_TOKEN: &str = "--token";
const ARG_MODE: &str = "--mode";

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// The editor shows a window; see [`HostMessage::Open`].
    Interactive,
    /// No UI: one request, one reply, then exit.
    Headless,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Interactive => "interactive",
            Mode::Headless => "headless",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "interactive" => Some(Mode::Interactive),
            "headless" => Some(Mode::Headless),
            _ => None,
        }
    }
}

/// Socket names: `GenericNamespaced` where the platform has a namespace for
/// them (and `/tmp` elsewhere, which interprocess does for us); a path in
/// the temp directory otherwise. The handler tells them apart by whether the
/// name has a path separator.
fn socket_name(name: &str) -> io::Result<Name<'_>> {
    if name.contains(['/', '\\']) {
        name.to_fs_name::<GenericFilePath>()
    } else {
        name.to_ns_name::<GenericNamespaced>()
    }
}

fn new_socket_name() -> String {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    let name = format!("ibisoffice-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
    if GenericNamespaced::is_supported() {
        name
    } else {
        std::env::temp_dir().join(format!("{name}.sock")).to_string_lossy().into_owned()
    }
}

fn new_token() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| Error::Io(io::Error::other(e.to_string())))?;
    Ok(hex(&bytes))
}

fn same_token(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The host's listening socket for one session, before the editor has
/// connected. [`HostSession::spawn`] covers the usual case; use this directly
/// to launch the editor some other way.
pub struct HostListener {
    listener: LocalSocketListener,
    name: String,
    token: String,
}

impl HostListener {
    pub fn bind() -> Result<Self> {
        let name = new_socket_name();
        let listener = ListenerOptions::new()
            .name(socket_name(&name)?)
            .nonblocking(ListenerNonblockingMode::Accept)
            .try_overwrite(true)
            .create_sync()?;
        Ok(HostListener { listener, name, token: new_token()? })
    }

    /// The name passed to the editor after `--ibis-embed`.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// `--ibis-embed <name> --token <token> --mode <mode>`.
    pub fn handler_args(&self, mode: Mode) -> [String; 6] {
        [
            ARG_SOCKET.into(),
            self.name.clone(),
            ARG_TOKEN.into(),
            self.token.clone(),
            ARG_MODE.into(),
            mode.as_str().into(),
        ]
    }

    /// Waits for an editor to connect and say hello with the right token.
    /// Connections with a wrong token (or none within the deadline) are
    /// dropped and waiting goes on. Gives [`Error::Timeout`] after `timeout`
    /// and [`Error::ChildExited`] as soon as `child` fails. A `child` that
    /// exits with success has handed the session to an instance of the editor
    /// already running (see [`crate::claim_instance`]): waiting goes on.
    pub fn accept(&self, timeout: Duration, mut child: Option<&mut Child>) -> Result<HostSession> {
        let deadline = clock::now() + timeout;
        loop {
            match self.listener.accept() {
                Ok(stream) => {
                    if let Some(session) = self.handshake(stream, deadline)? {
                        return Ok(session);
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(Error::Io(e)),
            }
            if let Some(status) = child.as_deref_mut().map(Child::try_wait).transpose()?.flatten() {
                if !status.success() {
                    return Err(Error::ChildExited(status.code()));
                }
                child = None;
            }
            if clock::now() >= deadline {
                return Err(Error::Timeout);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// `Ok(None)`: not our editor, keep waiting.
    fn handshake(&self, stream: LocalSocketStream, deadline: Instant) -> Result<Option<HostSession>> {
        stream.set_nonblocking(false)?;
        let left = deadline.saturating_duration_since(clock::now()).max(Duration::from_millis(50));
        // macOS refuses socket options (EINVAL) once the peer has hung up; its
        // hello may still be buffered, so read on regardless.
        let _ = stream.set_recv_timeout(Some(left));
        // A stranger may send anything; keep its first frame small.
        let mut transport = StreamTransport::new(stream).with_max_len(4096);
        let Ok(HandlerMessage::Hello { token, protocol }) = transport.recv::<HandlerMessage>() else {
            return Ok(None);
        };
        if !same_token(&token, &self.token) {
            return Ok(None);
        }
        if protocol != PROTOCOL_VERSION {
            return Err(Error::ProtocolMismatch { ours: PROTOCOL_VERSION, theirs: protocol });
        }
        let stream = transport.into_inner();
        let _ = stream.set_recv_timeout(None);
        Ok(Some(HostSession { transport: StreamTransport::new(stream), process: None }))
    }
}

/// The editor process of a session. Dropping it does not kill the editor
/// (send [`HostMessage::Close`] for that); it is reaped in the background.
pub struct EditorProcess {
    child: Option<Child>,
}

impl EditorProcess {
    /// The exit status if the editor has exited.
    pub fn try_exit_status(&mut self) -> Option<ExitStatus> {
        self.child.as_mut()?.try_wait().ok().flatten()
    }

    /// Waits up to `timeout` for the editor to exit.
    pub fn wait_timeout(&mut self, timeout: Duration) -> Option<ExitStatus> {
        let deadline = clock::now() + timeout;
        loop {
            if let Some(status) = self.try_exit_status() {
                return Some(status);
            }
            if clock::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn kill(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    pub fn id(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }
}

impl Drop for EditorProcess {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take()
            && !matches!(child.try_wait(), Ok(Some(_)))
        {
            std::thread::spawn(move || child.wait());
        }
    }
}

/// The host end of a connected session.
pub struct HostSession {
    transport: StreamTransport<LocalSocketStream>,
    process: Option<EditorProcess>,
}

impl HostSession {
    /// Launches `command args… --ibis-embed … --mode <mode>` and waits up to
    /// `timeout` for it to connect.
    pub fn spawn(
        command: impl AsRef<Path>,
        args: &[impl AsRef<std::ffi::OsStr>],
        mode: Mode,
        timeout: Duration,
    ) -> Result<Self> {
        let mut cmd = Command::new(command.as_ref());
        cmd.args(args).stdin(Stdio::null());
        Self::spawn_command(cmd, mode, timeout)
    }

    /// Like [`spawn`](Self::spawn) for a prepared `Command` (working
    /// directory, environment, output redirection); the protocol arguments
    /// are appended to it.
    pub fn spawn_command(mut cmd: Command, mode: Mode, timeout: Duration) -> Result<Self> {
        let listener = HostListener::bind()?;
        cmd.args(listener.handler_args(mode).map(OsString::from));
        let mut child = cmd.spawn()?;
        match listener.accept(timeout, Some(&mut child)) {
            Ok(mut session) => {
                session.process = Some(EditorProcess { child: Some(child) });
                Ok(session)
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(e)
            }
        }
    }

    pub fn send(&mut self, msg: &HostMessage) -> Result<()> {
        self.transport.send(msg)
    }

    /// Waits for the next message. [`Error::Disconnected`] when the editor
    /// closed the connection or exited.
    pub fn recv(&mut self) -> Result<HandlerMessage> {
        self.transport.recv()
    }

    /// Like [`recv`](Self::recv) but gives [`Error::Timeout`] after
    /// `timeout`. A timeout in the middle of a frame leaves the connection
    /// out of step, so only use this where the editor is expected to be idle
    /// or the session is abandoned on timeout.
    pub fn recv_timeout(&mut self, timeout: Duration) -> Result<HandlerMessage> {
        let stream = self.transport.get_ref();
        // Fails only when the editor is gone, which the read then reports.
        let _ = stream.set_recv_timeout(Some(timeout.max(Duration::from_millis(1))));
        let result = self.transport.recv();
        let _ = self.transport.get_ref().set_recv_timeout(None);
        result
    }

    /// Headless mode: sends `request` and returns the reply.
    /// [`HandlerMessage::Error`] comes back as `Ok`; the caller shows it.
    pub fn request(&mut self, request: &HostMessage) -> Result<HandlerMessage> {
        self.send(request)?;
        self.recv()
    }

    /// The editor process, when this session launched it.
    pub fn process(&mut self) -> Option<&mut EditorProcess> {
        self.process.as_mut()
    }

    /// Splits into halves that can live on different threads: typically a
    /// reader thread blocked in `recv` and the UI sending `Close`.
    pub fn split(self) -> (HostSender, HostReceiver, Option<EditorProcess>) {
        let (recv, send) = self.transport.into_inner().split();
        (MsgSender::new(send), MsgReceiver::new(recv), self.process)
    }
}

impl Transport for HostSession {
    fn send_frame(&mut self, payload: &[u8]) -> Result<()> {
        self.transport.send_frame(payload)
    }

    fn recv_frame(&mut self) -> Result<Vec<u8>> {
        self.transport.recv_frame()
    }
}

/// The sending half of a split session.
pub struct MsgSender<M> {
    half: SendHalf,
    _msg: PhantomData<fn(M)>,
}

impl<M: Serialize> MsgSender<M> {
    fn new(half: SendHalf) -> Self {
        MsgSender { half, _msg: PhantomData }
    }

    pub fn send(&mut self, msg: &M) -> Result<()> {
        write_frame(&mut self.half, &serde_json::to_vec(msg)?, MAX_FRAME_LEN)
    }
}

/// The receiving half of a split session.
pub struct MsgReceiver<M> {
    half: RecvHalf,
    _msg: PhantomData<fn() -> M>,
}

impl<M: DeserializeOwned> MsgReceiver<M> {
    fn new(half: RecvHalf) -> Self {
        MsgReceiver { half, _msg: PhantomData }
    }

    pub fn recv(&mut self) -> Result<M> {
        Ok(serde_json::from_slice(&read_frame(&mut self.half, MAX_FRAME_LEN)?)?)
    }
}

pub type HostSender = MsgSender<HostMessage>;
pub type HostReceiver = MsgReceiver<HandlerMessage>;
pub type HandlerSender = MsgSender<HandlerMessage>;
pub type HandlerReceiver = MsgReceiver<HostMessage>;

/// What the host put on the editor's command line.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HandlerArgs {
    pub socket: String,
    pub token: String,
    pub mode: Mode,
}

impl HandlerArgs {
    /// From this process's arguments; `Ok(None)` when it was not started by
    /// ibisOffice (no `--ibis-embed`), so the editor can run on its own.
    pub fn from_env() -> Result<Option<Self>> {
        Self::parse(std::env::args().skip(1))
    }

    /// Picks the protocol arguments out of `args` (other arguments are left
    /// for the editor). Accepts `--flag value` and `--flag=value`.
    pub fn parse(args: impl IntoIterator<Item = impl Into<String>>) -> Result<Option<Self>> {
        let (mut socket, mut token, mut mode) = (None, None, None);
        let mut args = args.into_iter().map(Into::into);
        while let Some(arg) = args.next() {
            let (flag, inline) = match arg.split_once('=') {
                Some((f, v)) => (f.to_owned(), Some(v.to_owned())),
                None => (arg, None),
            };
            let slot = match flag.as_str() {
                ARG_SOCKET => &mut socket,
                ARG_TOKEN => &mut token,
                ARG_MODE => &mut mode,
                _ => continue,
            };
            let value = inline
                .or_else(|| args.next())
                .ok_or_else(|| Error::Invalid(format!("{flag} needs a value")))?;
            *slot = Some(value);
        }
        let Some(socket) = socket else { return Ok(None) };
        let token = token.ok_or_else(|| Error::Invalid(format!("{ARG_TOKEN} is missing")))?;
        let mode = match mode {
            None => Mode::Interactive,
            Some(m) => Mode::parse(&m).ok_or_else(|| Error::Invalid(format!("unknown mode {m:?}")))?,
        };
        Ok(Some(HandlerArgs { socket, token, mode }))
    }
}

/// The editor end of a session.
///
/// ```no_run
/// use ibis_embed_protocol::{HandlerMessage, HandlerSession, HostMessage};
/// let Some(mut session) = HandlerSession::from_env()? else {
///     return Ok(()); // started by the user, not by ibisOffice
/// };
/// while let Ok(msg) = session.recv() {
///     match msg {
///         HostMessage::Close => break,
///         _ => { /* … */ }
///     }
/// }
/// session.send(&HandlerMessage::Closed)?;
/// # Ok::<(), ibis_embed_protocol::Error>(())
/// ```
pub struct HandlerSession {
    transport: StreamTransport<LocalSocketStream>,
    mode: Mode,
}

impl HandlerSession {
    /// Connects using this process's arguments; `Ok(None)` when there are
    /// none (see [`HandlerArgs::from_env`]).
    pub fn from_env() -> Result<Option<Self>> {
        HandlerArgs::from_env()?.map(|a| Self::connect(&a)).transpose()
    }

    /// Connects to the host and says hello.
    pub fn connect(args: &HandlerArgs) -> Result<Self> {
        let stream = LocalSocketStream::connect(socket_name(&args.socket)?)?;
        let mut transport = StreamTransport::new(stream);
        transport.send(&HandlerMessage::Hello { token: args.token.clone(), protocol: PROTOCOL_VERSION })?;
        Ok(HandlerSession { transport, mode: args.mode })
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn send(&mut self, msg: &HandlerMessage) -> Result<()> {
        self.transport.send(msg)
    }

    /// Waits for the next message. [`Error::Disconnected`] means the host is
    /// gone; an interactive editor should then close (after offering to
    /// save elsewhere, if it likes).
    pub fn recv(&mut self) -> Result<HostMessage> {
        self.transport.recv()
    }

    /// Splits into halves for different threads: typically a reader thread
    /// waiting for `Close` and the UI thread sending `Update`s.
    pub fn split(self) -> (HandlerSender, HandlerReceiver) {
        let (recv, send) = self.transport.into_inner().split();
        (MsgSender::new(send), MsgReceiver::new(recv))
    }
}

impl Transport for HandlerSession {
    fn send_frame(&mut self, payload: &[u8]) -> Result<()> {
        self.transport.send_frame(payload)
    }

    fn recv_frame(&mut self) -> Result<Vec<u8>> {
        self.transport.recv_frame()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_handler_args() {
        let a = HandlerArgs::parse(["file.cdx", "--ibis-embed", "s", "--token=ab", "--mode", "headless"])
            .unwrap()
            .unwrap();
        assert_eq!(a, HandlerArgs { socket: "s".into(), token: "ab".into(), mode: Mode::Headless });
        assert_eq!(HandlerArgs::parse(["file.cdx"]).unwrap(), None);
        assert!(HandlerArgs::parse(["--ibis-embed", "s"]).is_err());
        assert!(HandlerArgs::parse(["--ibis-embed", "s", "--token", "t", "--mode", "x"]).is_err());
        assert!(HandlerArgs::parse(["--ibis-embed"]).is_err());
    }

    #[test]
    fn tokens_are_128_bit_hex() {
        let (a, b) = (new_token().unwrap(), new_token().unwrap());
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
        assert!(same_token(&a, &a.clone()) && !same_token(&a, &b) && !same_token(&a, ""));
    }
}
