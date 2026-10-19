use super::*;

use crate::rollback_check_state::RollbackCheckState;

use campfire_net_sync::InputDelayConfig;

#[test]
fn effective_max_rollback_ticks_respects_prediction_limit() {
    let mut policy = RollbackConfig::default();
    assert_eq!(policy.max_rollback_ticks, 20);

    let balanced = InputTimelineConfig::default().with_input_delay(InputDelayConfig::balanced());
    assert_eq!(policy.effective_max_rollback_ticks(&balanced), 7);

    policy.max_rollback_ticks = 5;
    assert_eq!(policy.effective_max_rollback_ticks(&balanced), 5);

    let three_predicted =
        InputTimelineConfig::default().with_input_delay(InputDelayConfig::new(0, 3, 3));
    assert_eq!(policy.effective_max_rollback_ticks(&three_predicted), 3);
}

#[test]
fn confirmed_tick_advancement_uses_last_processed_confirmed_tick() {
    let mut metadata = RollbackCheckState::default();
    assert!(metadata.has_confirmed_tick_advanced(NetTick(10)));

    metadata.set_last_processed_confirmed_tick(NetTick(10));
    assert!(!metadata.has_confirmed_tick_advanced(NetTick(10)));
    assert!(!metadata.has_confirmed_tick_advanced(NetTick(9)));
    assert!(metadata.has_confirmed_tick_advanced(NetTick(11)));
}

#[test]
fn server_mutate_last_tick_can_be_newer_than_latest_complete_tick() {
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
