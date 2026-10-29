use campfire_net_sync::InputDelaySetup;

use super::*;
use crate::rollback_check_state::RollbackCheckState;

#[test]
fn the_rollback_depth_keeps_to_the_prediction_limit() {
    let mut policy = RollbackSetup::default();
    assert_eq!(policy.max_rollback_ticks, 20);

    let balanced = InputTimelineSetup::default().with_input_delay(InputDelaySetup::balanced());
    assert_eq!(policy.effective_max_rollback_ticks(&balanced), 7);

    policy.max_rollback_ticks = 5;
    assert_eq!(policy.effective_max_rollback_ticks(&balanced), 5);

    let three_predicted =
        InputTimelineSetup::default().with_input_delay(InputDelaySetup::new(0, 3, 3));
    assert_eq!(policy.effective_max_rollback_ticks(&three_predicted), 3);
}

#[test]
fn a_confirmed_tick_advances_from_the_last_one_processed() {
    let mut metadata = RollbackCheckState::default();
    assert!(metadata.has_confirmed_tick_advanced(NetTick(10)));

    metadata.set_last_processed_confirmed_tick(NetTick(10));
    assert!(!metadata.has_confirmed_tick_advanced(NetTick(10)));
    assert!(!metadata.has_confirmed_tick_advanced(NetTick(9)));
    assert!(metadata.has_confirmed_tick_advanced(NetTick(11)));
}

#[test]
fn the_last_mutate_tick_of_the_server_can_be_past_the_latest_complete_tick() {
    use bevy_replicon::client::server_mutate_ticks::ServerMutateTicks;
    use bevy_replicon::prelude::RepliconTick;

    let complete_tick = RepliconTick::new(9);
    let incomplete_tick = RepliconTick::new(10);

    let mut server_mutate_ticks = ServerMutateTicks::default();
    assert!(server_mutate_ticks.confirm(complete_tick, 1));
    assert!(!server_mutate_ticks.confirm(incomplete_tick, 2));

    assert_eq!(server_mutate_ticks.last_tick(), incomplete_tick);
    assert!(server_mutate_ticks.contains(complete_tick));
    assert!(!server_mutate_ticks.contains(incomplete_tick));
}
