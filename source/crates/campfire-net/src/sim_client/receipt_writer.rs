use std::path::PathBuf;

use bevy_ecs::resource::Resource;
use campfire_protocol::SignedReceipt;
use campfire_store::{DurableError, DurableFile, LatestWriter};

/// Writes the newest receipt a client keeps to its data directory, as
/// `receipts/<session id>.receipt`, on a worker of its own, so no frame waits for a sync: the
/// main thread hands over each receipt it keeps, which replaces one not yet written, and takes
/// back each failure to log. Dropping the writer writes the receipt it holds, then ends the
/// worker.
#[derive(Resource, Debug)]
pub(crate) struct ReceiptWriter(LatestWriter<SignedReceipt, DurableError>);

impl ReceiptWriter {
    /// A writer into the data directory `data`, made when missing.
    pub(crate) fn start(data: PathBuf) -> ReceiptWriter {
        let receipts = data.join("receipts");
        ReceiptWriter(LatestWriter::start(
            "receipts",
            move |receipt: &SignedReceipt| {
                let file = receipts.join(format!("{}.receipt", receipt.receipt.session_id));
                DurableFile::create_dir(&data)?;
                DurableFile::create_dir(&receipts)?;
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
