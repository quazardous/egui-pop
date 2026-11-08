//! The Unix socket: private to the user, or not started.

use std::io::{self, BufReader, ErrorKind};
use std::os::unix::fs::{FileTypeExt as _, PermissionsExt as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

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
        std::thread::Builder::new()
            .name("egui-pop-agent".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    if stopping.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    let tx = tx.clone();
                    let wake = wake.clone();
                    let _ = std::thread::Builder::new()
                        .name("egui-pop-agent-conn".into())
                        .spawn(move || {
                            let Ok(read) = stream.try_clone() else { return };
                            crate::rpc::serve(BufReader::new(read), stream, &tx, &wake);
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
