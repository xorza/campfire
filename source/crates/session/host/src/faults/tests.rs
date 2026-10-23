use std::io;
use std::num::NonZeroU8;
use std::path::PathBuf;

use campfire_log::internals::LogCheck;
use campfire_store::{AppendError, DurableError, PathError};

use super::*;
use crate::events::checkpoint_failed::CheckpointFailed;
use crate::events::receipt_unsaved::ReceiptUnsaved;

/// A write of the file `name` that found no file name in its path.
fn no_name(name: &str) -> PathError<DurableError> {
    PathError {
        path: PathBuf::from(name),
        error: DurableError::NoName,
    }
}

#[test]
fn each_fault_is_logged_once_and_ends_the_server_as_its_policy_says() {
    let log = LogCheck::start();
    let journal = Fault::Journal(AppendError::Sync(io::Error::other("full")));
    let faults = [
        journal,
        Fault::Snapshot(no_name("s").into()),
        Fault::Receipt(no_name("r").into()),
    ];
    assert_eq!(
        faults.each_ref().map(Fault::policy),
        [
            FaultPolicy::EndServer,
            FaultPolicy::EndServer,
            FaultPolicy::Log
        ]
    );
    // None reported: nothing happens.
    let mut world = World::new();
    world.init_resource::<WorkerFaults>();
    assert_eq!(WorkerFaults::apply(&mut world), None);
    // A receipt not written is logged, and play goes on.
    let receipt = Fault::Receipt(no_name("receipt").into());
    world.resource_mut::<WorkerFaults>().report(receipt);
    assert_eq!(WorkerFaults::apply(&mut world), None);
    assert_eq!(log.take::<ReceiptUnsaved>().len(), 1);
    // A snapshot not written, with a receipt after it: both logged, and the server ends.
    let snapshot = Fault::Snapshot(no_name("snapshot").into());
    world.resource_mut::<WorkerFaults>().report(snapshot);
    world
        .resource_mut::<WorkerFaults>()
        .report(Fault::Receipt(no_name("receipt").into()));
    let code = NonZeroU8::new(ExitStatus::Storage.code()).unwrap();
    assert_eq!(WorkerFaults::apply(&mut world), Some(AppExit::Error(code)));
    let failed = log.take::<CheckpointFailed>();
    assert_eq!(failed.len(), 1);
    // The log names the path, then the step.
    assert_eq!(failed[0].error, "snapshot: the path names no file");
    assert_eq!(log.take::<ReceiptUnsaved>().len(), 1);
    // Each is applied once.
    assert_eq!(WorkerFaults::apply(&mut world), None);
}
