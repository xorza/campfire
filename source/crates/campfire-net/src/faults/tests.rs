use std::num::NonZeroU8;

use campfire_log::internals::LogCheck;
use std::path::PathBuf;

use campfire_store::{DurableError, PathError};

use super::*;
use crate::events::checkpoint_failed::CheckpointFailed;
use crate::events::receipt_unsaved::ReceiptUnsaved;
use crate::faults::fault::FaultSource;

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
    assert_eq!(
        [
            FaultSource::Journal,
            FaultSource::Snapshot,
            FaultSource::Receipt
        ]
        .map(FaultSource::policy),
        [
            FaultPolicy::EndServer,
            FaultPolicy::EndServer,
            FaultPolicy::Log
        ]
    );
    // None reported: nothing happens.
    let mut world = World::new();
    world.init_resource::<Faults>();
    assert_eq!(Faults::apply(&mut world), None);
    // A receipt not written is logged, and play goes on.
    let receipt = Fault::Receipt(no_name("receipt"));
    assert_eq!(receipt.source(), FaultSource::Receipt);
    world.resource_mut::<Faults>().report(receipt);
    assert_eq!(Faults::apply(&mut world), None);
    assert_eq!(log.take::<ReceiptUnsaved>().len(), 1);
    // A snapshot not written, with a receipt after it: both logged, and the server ends.
    let snapshot = Fault::Snapshot(no_name("snapshot"));
    assert_eq!(snapshot.source(), FaultSource::Snapshot);
    world.resource_mut::<Faults>().report(snapshot);
    world
        .resource_mut::<Faults>()
        .report(Fault::Receipt(no_name("receipt")));
    let code = NonZeroU8::new(ExitStatus::Storage.code()).unwrap();
    assert_eq!(Faults::apply(&mut world), Some(AppExit::Error(code)));
    let failed = log.take::<CheckpointFailed>();
    assert_eq!(failed.len(), 1);
    // The log names the path, then the step.
    assert_eq!(failed[0].error, "snapshot: the path names no file");
    assert_eq!(log.take::<ReceiptUnsaved>().len(), 1);
    // Each is applied once.
    assert_eq!(Faults::apply(&mut world), None);
}
