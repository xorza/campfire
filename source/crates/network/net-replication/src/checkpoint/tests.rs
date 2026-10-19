use super::*;
use crate::replication_checkpoint_map::ReplicationCheckpointMap;
use bevy_replicon::client::UserdataReceived;
use bevy_replicon::prelude::RepliconTick;

#[test]
fn authoritative_tick_userdata_roundtrips() {
    let bytes = encode_authoritative_tick(NetTick(42));

    assert_eq!(decode_authoritative_tick(&bytes), Ok(NetTick(42)));
}

#[test]
fn invalid_authoritative_tick_userdata_is_rejected() {
    assert_eq!(
        decode_authoritative_tick(&[1, 2, 3]),
        Err(CheckpointError::InvalidUserdataLength(3))
    );
}

/// Userdata that does not decode records nothing and counts a fault.
#[test]
fn invalid_userdata_is_counted() {
    let mut app = bevy_app::App::new();
    app.init_resource::<ReplicationCheckpointMap>();
    app.add_observer(ReplicationCheckpointMap::record_authoritative_tick);

    app.world_mut().trigger(UserdataReceived {
        message_tick: RepliconTick::new(5),
        bytes: bytes::Bytes::from_static(&[1, 2, 3]),
    });

    let checkpoints = app.world().resource::<ReplicationCheckpointMap>();
    assert_eq!(checkpoints.get(RepliconTick::new(5)), None);
    assert_eq!(checkpoints.faults().invalid_userdata, 1);
}
