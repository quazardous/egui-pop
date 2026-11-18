//! The Unix socket: private to the user, or not started; a client that
//! is not the user's own process is refused before any request.

use std::io::{self, BufReader, ErrorKind};
use std::os::unix::fs::{FileTypeExt as _, PermissionsExt as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use crate::limits::Connections;
use crate::rpc::{self, RpcError};
use crate::{Job, Waker};

/// A socket being served; dropping it stops listening and removes the
/// file.
#[derive(Debug)]
pub(crate) struct Listening {
    path: PathBuf,
    stop: Arc<AtomicBool>,
}

impl Listening {
    pub(crate) fn start(path: &Path, tx: Sender<Job>, wake: Waker) -> io::Result<Self> {
        let listener = bind_private(path)?;
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
                    let Ok(stream) = stream else { continue };
                    match same_user(&stream) {
                        Ok(true) => {}
                        Ok(false) => {
                            log::warn!(target: "egui_pop_agent", "agent client refused: another user's process");
                            rpc::refuse(&stream, RpcError::not_the_user());
                            continue;
                        }
                        Err(e) => {
                            log::warn!(target: "egui_pop_agent", "agent client refused: its user is unknown ({e})");
                            rpc::refuse(&stream, RpcError::not_the_user());
                            continue;
                        }
                    }
                    let Some(slot) = connections.take() else {
                        log::warn!(target: "egui_pop_agent", "agent client refused: too many connected");
                        rpc::refuse(&stream, RpcError::too_many_connections());
                        continue;
                    };
                    let tx = tx.clone();
                    let wake = wake.clone();
                    let _ = std::thread::Builder::new()
                        .name("egui-pop-agent-conn".into())
                        .spawn(move || {
                            let _slot = slot;
                            let Ok(read) = stream.try_clone() else { return };
                            rpc::serve(BufReader::new(read), stream, &tx, &wake);
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
        let _ = UnixStream::connect(&self.path);
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Binds `path`, mode 0600, in a directory only its owner may write to;
/// refuses otherwise. A socket left by an instance that is gone is
/// replaced; one that answers is refused.
pub(crate) fn bind_private(path: &Path) -> io::Result<UnixListener> {
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    let mode = std::fs::metadata(dir)?.permissions().mode();
    if mode & 0o022 != 0 {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            format!(
                "{} may be written by others (mode {:o}): the socket needs a private directory",
                dir.display(),
                mode & 0o777
            ),
        ));
    }
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if !meta.file_type().is_socket() {
            return Err(io::Error::new(
                ErrorKind::AlreadyExists,
                format!("{} exists and is not a socket", path.display()),
            ));
        }
        if UnixStream::connect(path).is_ok() {
            return Err(io::Error::new(
                ErrorKind::AddrInUse,
                format!("another instance listens at {}", path.display()),
            ));
        }
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    let private = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .and_then(|()| std::fs::metadata(path))
        .map(|m| m.permissions().mode() & 0o777 == 0o600)
        .unwrap_or(false);
    if !private {
        let _ = std::fs::remove_file(path);
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            format!("{} could not be made private (mode 0600)", path.display()),
        ));
    }
    Ok(listener)
}

/// Whether the process at the other end of `stream` runs as this one's
/// user (its effective uid). Unknown on a platform without peer
/// credentials, which refuses every client.
pub(crate) fn same_user(stream: &UnixStream) -> io::Result<bool> {
    peer_uid(stream).map(|uid| uid == nix::unistd::geteuid().as_raw())
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
    Ok(getsockopt(stream, PeerCredentials)
        .map_err(io::Error::from)?
        .uid())
}

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    let (uid, _) = nix::unistd::getpeereid(stream).map_err(io::Error::from)?;
    Ok(uid.as_raw())
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
)))]
fn peer_uid(_: &UnixStream) -> io::Result<u32> {
    Err(io::Error::new(
        ErrorKind::Unsupported,
        "no peer credentials on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_of_this_process_is_the_same_user() {
        let (a, b) = UnixStream::pair().unwrap();
        assert!(same_user(&a).unwrap());
        assert!(same_user(&b).unwrap());
    }
}
