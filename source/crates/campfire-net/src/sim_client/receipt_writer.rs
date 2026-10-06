use std::mem;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};

use bevy_ecs::resource::Resource;
use campfire_protocol::{DurableError, DurableFile, SignedReceipt};

/// Writes the newest receipt a client keeps to its data directory, as
/// `receipts/<session id>.receipt`, on a thread of its own, so no frame waits for a sync: the
/// main thread hands over each receipt it keeps, which replaces one not yet written, and takes
/// back each failure to log. Dropping the writer writes the receipt it holds, then ends the
/// thread.
#[derive(Resource, Debug)]
pub(crate) struct ReceiptWriter {
    shared: Arc<Shared>,
    writer: Option<JoinHandle<()>>,
}

/// What the main thread and the writer share.
#[derive(Debug)]
struct Shared {
    state: Mutex<WriterState>,
    wake: Condvar,
}

#[derive(Debug, Default)]
struct WriterState {
    /// The newest receipt the writer has not taken yet.
    newest: Option<SignedReceipt>,
    /// The failures the main thread has not taken yet.
    failures: Vec<DurableError>,
    closing: bool,
}

impl ReceiptWriter {
    /// A writer into the data directory `data`, made when missing.
    pub(crate) fn start(data: PathBuf) -> ReceiptWriter {
        let shared = Arc::new(Shared {
            state: Mutex::new(WriterState::default()),
            wake: Condvar::new(),
        });
        let writing = Arc::clone(&shared);
        let writer = thread::Builder::new()
            .name("receipts".to_owned())
            .spawn(move || ReceiptWriter::write(&writing, &data))
            .expect("the OS starts a thread");
        ReceiptWriter {
            shared,
            writer: Some(writer),
        }
    }

    /// Hands `receipt` to the writer, in place of one it has not taken yet.
    pub(crate) fn give(&self, receipt: SignedReceipt) {
        self.lock().newest = Some(receipt);
        self.shared.wake.notify_one();
    }

    /// The writes that failed since the last call.
    pub(crate) fn failures(&self) -> Vec<DurableError> {
        mem::take(&mut self.lock().failures)
    }

    fn lock(&self) -> MutexGuard<'_, WriterState> {
        self.shared
            .state
            .lock()
            .expect("the receipt writer does not panic")
    }

    /// The writer thread: writes each receipt the main thread hands over, until the writer
    /// closes.
    fn write(shared: &Shared, data: &Path) {
        let receipts = data.join("receipts");
        loop {
            let mut state = shared
                .state
                .lock()
                .expect("the main thread does not panic holding the receipt writer");
            while state.newest.is_none() && !state.closing {
                state = shared
                    .wake
                    .wait(state)
                    .expect("the main thread does not panic holding the receipt writer");
            }
            let Some(receipt) = state.newest.take() else {
                return;
            };
            drop(state);
            let file = receipts.join(format!("{}.receipt", receipt.receipt.session_id));
            let written = DurableFile::create_dir(data)
                .and_then(|()| DurableFile::create_dir(&receipts))
                .and_then(|()| DurableFile::write(&file, &receipt.encode()));
            if let Err(error) = written {
                shared
                    .state
                    .lock()
                    .expect("the main thread does not panic holding the receipt writer")
                    .failures
                    .push(error);
            }
        }
    }
}

impl Drop for ReceiptWriter {
    fn drop(&mut self) {
        self.lock().closing = true;
        self.shared.wake.notify_one();
        if let Some(writer) = self.writer.take() {
            writer.join().expect("the receipt writer does not panic");
        }
    }
}
