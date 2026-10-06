use std::num::NonZeroU8;

use campfire_log::internals::LogCheck;
use campfire_store::DurableError;

use super::*;
use crate::events::checkpoint_failed::CheckpointFailed;
use crate::events::receipt_unsaved::ReceiptUnsaved;
use crate::faults::fault::FaultSource;

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
    let receipt = Fault::Receipt(DurableError::NoName);
    assert_eq!(receipt.source(), FaultSource::Receipt);
    world.resource_mut::<Faults>().report(receipt);
    assert_eq!(Faults::apply(&mut world), None);
    assert_eq!(log.take::<ReceiptUnsaved>().len(), 1);
    // A snapshot not written, with a receipt after it: both logged, and the server ends.
    let snapshot = Fault::Snapshot(DurableError::NoName);
    assert_eq!(snapshot.source(), FaultSource::Snapshot);
    world.resource_mut::<Faults>().report(snapshot);
    world
        .resource_mut::<Faults>()
        .report(Fault::Receipt(DurableError::NoName));
    let code = NonZeroU8::new(ExitStatus::Storage.code()).unwrap();
    assert_eq!(Faults::apply(&mut world), Some(AppExit::Error(code)));
    let failed = log.take::<CheckpointFailed>();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].error, DurableError::NoName.to_string());
    assert_eq!(log.take::<ReceiptUnsaved>().len(), 1);
    // Each is applied once.
    assert_eq!(Faults::apply(&mut world), None);
}
