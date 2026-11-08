//! The Windows named pipe, through `interprocess`.

use std::io::{self, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use interprocess::local_socket::traits::{ListenerExt as _, Stream as _};
use interprocess::local_socket::{GenericFilePath, ListenerOptions, Stream, ToFsName as _};

use crate::{Job, Waker};

/// A pipe being served; dropping it stops listening.
#[derive(Debug)]
pub(crate) struct Listening {
    path: PathBuf,
    stop: Arc<AtomicBool>,
}

impl Listening {
    pub(crate) fn start(path: &Path, tx: Sender<Job>, wake: Waker) -> io::Result<Self> {
        let name = path.to_owned().to_fs_name::<GenericFilePath>()?;
        let listener = ListenerOptions::new().name(name).create_sync()?;
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
                            let (read, write) = stream.split();
                            crate::rpc::serve(BufReader::new(read), write, &tx, &wake);
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
