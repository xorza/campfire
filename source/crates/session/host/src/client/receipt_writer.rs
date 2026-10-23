use std::sync::Arc;

use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Res, ResMut};
use bevy_ecs::world::World;
use campfire_protocol::SignedReceipt;
use campfire_store::{DurableDir, DurableFile, LatestWriter};
use campfire_threads::Job;

use crate::client::client_dir::ClientDir;
use crate::faults::WorkerFaults;
use crate::faults::error::ReceiptWriteError;
use crate::faults::fault::Fault;

/// Writes the newest receipt a client keeps to its data directory, on a worker of its own, so no
/// frame waits for a sync: the main thread hands over each receipt it keeps, which replaces one
/// not yet written, and takes back the newest failure to log. The app closes it as it exits,
/// which writes the receipt it holds, then ends the worker.
#[derive(Resource, Debug)]
pub struct ReceiptWriter(LatestWriter<SignedReceipt, ReceiptWriteError>);

impl ReceiptWriter {
    /// A writer into the data directory `data`, which it holds locked while it runs.
    pub fn start(data: Arc<ClientDir>) -> Self {
        let mut bytes = Vec::new();
        let mut made = false;
        Self(LatestWriter::start(
            Job::Receipts,
            move |receipt: &SignedReceipt| {
                // The directory is made once, with the first receipt.
                if !made {
                    DurableDir::create(&data.layout().receipts_dir())?;
                    made = true;
                }
                let file = data.layout().receipt_file(receipt.receipt.session_id);
                receipt.encode(&mut bytes);
                DurableFile::write(&file, &bytes)?;
                Ok(())
            },
        ))
    }

    /// Hands `receipt` to the writer, in place of one it has not taken yet.
    pub fn give(&self, receipt: SignedReceipt) {
        self.0.give(receipt);
    }

    /// Reports to `WorkerFaults` the newest write that failed since the last frame.
    pub fn report_failure(writer: Option<Res<'_, Self>>, mut faults: ResMut<'_, WorkerFaults>) {
        if let Some(failure) = writer.and_then(|writer| writer.0.take_failure()) {
            faults.report(Fault::Receipt(failure));
        }
    }

    /// Closes the writer as the app exits, once it wrote the receipt it held, and applies its
    /// failure, if one, as each frame's are.
    pub fn close_at_exit(world: &mut World) {
        let Some(writer) = world.remove_resource::<Self>() else {
            return;
        };
        if let Some(failure) = writer.0.close() {
            world
                .resource_mut::<WorkerFaults>()
                .report(Fault::Receipt(failure));
            WorkerFaults::watch(world);
        }
    }
}
