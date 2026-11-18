//! The Windows named pipe, through `interprocess`: its access list lets
//! only its owner (the user the application runs as) and the system in;
//! remote clients are rejected (`PIPE_REJECT_REMOTE_CLIENTS`, as
//! `interprocess` creates a pipe by default), and the first instance is
//! created with `FILE_FLAG_FIRST_PIPE_INSTANCE`, so a pipe another
//! process created first under the name is not taken over.

use std::io::{self, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use interprocess::local_socket::traits::{ListenerExt as _, Stream as _};
use interprocess::local_socket::{GenericFilePath, ListenerOptions, Stream, ToFsName as _};
use interprocess::os::windows::local_socket::ListenerOptionsExt as _;
use interprocess::os::windows::security_descriptor::SecurityDescriptor;

use crate::limits::Connections;
use crate::rpc::{self, RpcError};
use crate::{Job, Waker};

/// The pipe's security descriptor: a protected access list (not
/// inherited) granting all access to the owner rights (the user who
/// created it) and the local system, and nothing to anyone else.
pub(crate) const PIPE_SDDL: &str = "D:P(A;;GA;;;OW)(A;;GA;;;SY)";

/// A pipe being served; dropping it stops listening.
#[derive(Debug)]
pub(crate) struct Listening {
    path: PathBuf,
    stop: Arc<AtomicBool>,
}

impl Listening {
    pub(crate) fn start(path: &Path, tx: Sender<Job>, wake: Waker) -> io::Result<Self> {
        let name = path.to_owned().to_fs_name::<GenericFilePath>()?;
        let sddl = widestring::U16CString::from_str(PIPE_SDDL)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let sd = SecurityDescriptor::deserialize(&sddl)?;
        let listener = ListenerOptions::new()
            .name(name)
            .security_descriptor(sd)
            .create_sync()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let connections = Connections::default();
        std::thread::Builder::new()
            .name("egui-pop-agent".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    if stopping.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(mut stream) = stream else { continue };
                    let Some(slot) = connections.take() else {
                        log::warn!(target: "egui_pop_agent", "agent client refused: too many connected");
                        rpc::refuse(&mut stream, RpcError::too_many_connections());
                        continue;
                    };
                    let tx = tx.clone();
                    let wake = wake.clone();
                    let _ = std::thread::Builder::new()
                        .name("egui-pop-agent-conn".into())
                        .spawn(move || {
                            let _slot = slot;
                            let (read, write) = stream.split();
                            rpc::serve(BufReader::new(read), write, &tx, &wake);
                        });
                }
            })?;
        Ok(Self {
            path: path.to_owned(),
            stop,
        })
    }
}

impl Drop for Listening {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Wakes the accepting thread, which then stops.
        if let Ok(name) = self.path.clone().to_fs_name::<GenericFilePath>() {
            let _ = Stream::connect(name);
        }
    }
}
