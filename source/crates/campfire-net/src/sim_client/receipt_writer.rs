use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_protocol::SignedReceipt;
use campfire_store::{DurableError, DurableFile, LatestWriter};

use crate::client_data::ClientData;

/// Writes the newest receipt a client keeps to its data directory, on a worker of its own, so no
/// frame waits for a sync: the main thread hands over each receipt it keeps, which replaces one
/// not yet written, and takes back each failure to log. Dropping the writer writes the receipt
/// it holds, then ends the worker.
#[derive(Resource, Debug)]
pub(crate) struct ReceiptWriter(LatestWriter<SignedReceipt, DurableError>);

impl ReceiptWriter {
    /// A writer into the data directory `data`, which it holds locked while it runs.
    pub(crate) fn start(data: Arc<ClientData>) -> ReceiptWriter {
        ReceiptWriter(LatestWriter::start(
            "receipts",
            move |receipt: &SignedReceipt| {
                DurableFile::create_dir(&data.receipts_dir())?;
                let file = data.receipt_file(receipt.receipt.session_id);
                DurableFile::write(&file, &receipt.encode())
            },
        ))
    }

    /// Hands `receipt` to the writer, in place of one it has not taken yet.
    pub(crate) fn give(&self, receipt: SignedReceipt) {
        self.0.give(receipt);
    }

    /// The writes that failed since the last call.
    pub(crate) fn failures(&self) -> Vec<DurableError> {
        self.0.take_failures()
    }
}
