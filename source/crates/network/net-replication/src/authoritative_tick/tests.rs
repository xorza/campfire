use super::*;
use crate::confirmed_tick_map::ConfirmedTickMap;
use bevy_replicon::client::UserdataReceived;
use bevy_replicon::prelude::RepliconTick;

#[test]
fn an_authoritative_tick_reads_back_from_its_userdata() {
    let bytes = encode_authoritative_tick(NetTick(42));

    assert_eq!(decode_authoritative_tick(&bytes), Ok(NetTick(42)));
}

#[test]
fn an_invalid_authoritative_tick_userdata_is_refused() {
    assert_eq!(
        decode_authoritative_tick(&[1, 2, 3]),
        Err(ConfirmedTickError::InvalidUserdataLength(3))
    );
}

/// Userdata that does not decode records nothing and counts a fault.
#[test]
fn an_invalid_userdata_counts_a_fault() {
    let mut app = bevy_app::App::new();
    app.init_resource::<ConfirmedTickMap>();
    app.add_observer(ConfirmedTickMap::record_authoritative_tick);

    app.world_mut().trigger(UserdataReceived {
        message_tick: RepliconTick::new(5),
        bytes: bytes::Bytes::from_static(&[1, 2, 3]),
    });

    let confirmed_ticks = app.world().resource::<ConfirmedTickMap>();
    assert_eq!(confirmed_ticks.get(RepliconTick::new(5)), None);
    assert_eq!(confirmed_ticks.faults().invalid_userdata, 1);
}
