// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! One window for every object: an editor started for a second object hands
//! it to the instance already running and exits with success (which the
//! host takes as "wait on"), and that instance connects to the host for it.
//!
//! ```no_run
//! use ibis_embed_protocol::{HandlerArgs, Instance, claim_instance};
//! let args = HandlerArgs::from_env()?.expect("started by ibisOffice");
//! match claim_instance("com.example.editor", &args)? {
//!     Instance::HandedOver => return Ok(()), // the running editor has it
//!     Instance::First(requests) => {
//!         // Connect for `args`, then for each `requests.recv()` as they come.
//!     }
//! }
//! # Ok::<(), ibis_embed_protocol::Error>(())
//! ```

use std::sync::mpsc;

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{GenericFilePath, GenericNamespaced, ListenerOptions, Name};

use crate::{Error, HandlerArgs, Result, StreamTransport, Transport};

/// What [`claim_instance`] found.
pub enum Instance {
    /// This is the editor's only instance: the sessions handed to it later come here.
    First(InstanceRequests),
    /// An instance was running and has the session now: this process can exit.
    HandedOver,
}

/// The sessions other launches of the editor handed to this instance.
pub struct InstanceRequests {
    rx: mpsc::Receiver<HandlerArgs>,
}

impl InstanceRequests {
    /// The next one, if one came.
    pub fn try_recv(&self) -> Option<HandlerArgs> {
        self.rx.try_recv().ok()
    }

    /// Waits for the next one; `None` once none can come.
    pub fn recv(&self) -> Option<HandlerArgs> {
        self.rx.recv().ok()
    }
}

/// The instance socket's name: of the editor (`app_id`, reverse-DNS) and the user.
fn instance_name(app_id: &str) -> String {
    let user = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default();
    let clean = |s: &str| s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect::<String>();
    let name = format!("ibis-embed-instance-{}-{}", clean(app_id), clean(&user));
    if GenericNamespaced::is_supported() { name } else { std::env::temp_dir().join(format!("{name}.sock")).to_string_lossy().into_owned() }
}

fn name_of(name: &str) -> std::io::Result<Name<'_>> {
    if name.contains(['/', '\\']) { name.to_fs_name::<GenericFilePath>() } else { name.to_ns_name::<GenericNamespaced>() }
}

/// Hands `args` to the instance of the editor `app_id` that is running, or
/// makes this process that instance.
pub fn claim_instance(app_id: &str, args: &HandlerArgs) -> Result<Instance> {
    let name = instance_name(app_id);
    if let Ok(stream) = LocalSocketStream::connect(name_of(&name)?) {
        let mut transport = StreamTransport::new(stream);
        transport.send(args)?;
        // Its answer says it took the session.
        let taken: bool = transport.recv()?;
        if taken {
            return Ok(Instance::HandedOver);
        }
        return Err(Error::Unexpected("the running editor refused the object".into()));
    }
    // None is running (a socket left by one that crashed is replaced).
    let listener = ListenerOptions::new().name(name_of(&name)?).try_overwrite(true).create_sync()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().filter_map(std::result::Result::ok) {
            let mut transport = StreamTransport::new(stream).with_max_len(64 * 1024);
            let Ok(args) = transport.recv::<HandlerArgs>() else { continue };
            let taken = tx.send(args).is_ok();
            let _ = transport.send(&taken);
            if !taken {
                break;
            }
        }
    });
    Ok(Instance::First(InstanceRequests { rx }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Mode;

    #[test]
    fn a_second_launch_hands_over() {
        let id = format!("test.instance.{}", std::process::id());
        let first = HandlerArgs { socket: "one".into(), token: "t1".into(), mode: Mode::Interactive };
        let second = HandlerArgs { socket: "two".into(), token: "t2".into(), mode: Mode::Interactive };
        let Instance::First(requests) = claim_instance(&id, &first).unwrap() else { panic!("not the first") };
        assert!(matches!(claim_instance(&id, &second).unwrap(), Instance::HandedOver));
        assert_eq!(requests.recv(), Some(second));
    }
}
