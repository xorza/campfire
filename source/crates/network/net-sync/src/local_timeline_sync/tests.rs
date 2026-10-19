use super::*;
use crate::fixtures::sync_stack::SyncStack;
use crate::input_delay_config::InputDelayConfig;
use crate::remote_timeline::RemoteTimeline;
use campfire_net_core::{NetTick, PositiveTickDelta};
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperConfig;
use campfire_net_timeline::LocalTimeline;
use core::time::Duration;

use crate::synced_local_timeline::SyncedLocalTimeline;
use bevy_app::{App, Update};

use bevy_utils::default;

use campfire_net_timeline::NetworkTimeline;

#[derive(Debug, Resource, Default)]
struct SyncedParamRuns(u8);

#[test]
fn synced_local_timeline_checks_local_sync_readiness() {
    fn require_synced_timeline(
        timeline: SyncedLocalTimeline<'_, '_>,
        mut runs: ResMut<'_, SyncedParamRuns>,
    ) {
        assert_eq!(timeline.input_delay(), 3);
        runs.0 += 1;
    }

    let mut app = App::new();
    app.init_resource::<LocalTimeline>();
    app.init_resource::<LocalTimelineSync>();
    app.init_resource::<SyncedParamRuns>();
    app.add_systems(Update, require_synced_timeline);

    app.update();
    assert_eq!(app.world().resource::<SyncedParamRuns>().0, 0);

    {
        let mut sync = app.world_mut().resource_mut::<LocalTimelineSync>();
        sync.input_delay_ticks = 3;
        sync.set_synced(true);
    }

    app.update();
    assert_eq!(app.world().resource::<SyncedParamRuns>().0, 1);
}

#[test]
fn local_timeline_objective_preserves_margin_after_sync_deadband() {
    let tick_rate = TickRate::try_from(100).unwrap();
    let mut remote = RemoteTimeline::default();
    remote.set_now(TickInstant::from(NetTick(100)));

    let estimate = LinkEstimate::new(Duration::from_millis(40), Duration::from_millis(5));

    let mut config = InputTimelineConfig::default();
    config.sync.jitter_multiple = 2;
    config.sync.jitter_margin = PositiveTickDelta::lit("1");
    config.sync.error_margin = PositiveTickDelta::lit("0.75");

    let objective = LocalTimelineSync::default().sync_objective(&SyncSample {
        remote: &remote,
        config: &config,
        estimate,
        tick_rate,
    });

    // remote 100 + RTT/2 2 ticks + jitter margin 2 ticks
    // + server input pipeline 1 tick + controller deadband 0.75 ticks.
    assert_eq!(objective, TickInstant::lit("105.75"));

    let earliest_uncorrected_timeline = objective - TickDelta::lit("0.75");
    // Even if the sync controller chooses not to correct a -0.75 tick error,
    // the client is still at the delivery objective that includes RTT/2 and
    // jitter margin plus the server's one-tick input pipeline delay.
    assert_eq!(earliest_uncorrected_timeline, TickInstant::lit("105"));
}

#[test]
fn input_delay_still_offsets_local_timeline_objective() {
    let tick_rate = TickRate::try_from(100).unwrap();
    let mut remote = RemoteTimeline::default();
    remote.set_now(TickInstant::from(NetTick(100)));

    let estimate = LinkEstimate::new(Duration::from_millis(40), Duration::from_millis(5));

    let mut config =
        InputTimelineConfig::default().with_input_delay(InputDelayConfig::fixed_input_delay(2));
    config.sync.jitter_multiple = 2;
    config.sync.jitter_margin = PositiveTickDelta::lit("1");
    config.sync.error_margin = PositiveTickDelta::lit("0.75");

    let timeline = LocalTimelineSync {
        input_delay_ticks: 2,
        ..Default::default()
    };

    let objective = timeline.sync_objective(&SyncSample {
        remote: &remote,
        config: &config,
        estimate,
        tick_rate,
    });

    assert_eq!(objective, TickInstant::lit("103.75"));
}

/// The server reads inputs in `FixedPreUpdate`, after receiving packets in
/// `PreUpdate` and advancing its tick in `FixedFirst`. Inputs sent by the
/// client must therefore target at least `remote + 1`, even under
/// worst-case controller drift (`offset = -error_margin`).
///
/// The post-replicon `+sync_error_margin` term in the objective
/// cancels with the symmetric controller drift, leaving the safety
/// margin riding on the explicit one-tick server input pipeline margin
/// plus `network_delay + jitter_margin`.
#[test]
fn sync_objective_keeps_sent_input_tick_ahead_under_worst_case_drift() {
    let tick_rate = TickRate::try_from(100).unwrap();
    let mut remote = RemoteTimeline::default();
    remote.set_now(TickInstant::from(NetTick(100)));

    // Localhost — zero RTT, zero jitter.
    let estimate = LinkEstimate::new(Duration::ZERO, Duration::ZERO);

    // User tightens `jitter_margin` below 1.0 for snappier sync, while
    // still using input delay. The local tick itself can be behind
    // `remote + 1`; the sent input tick must not be.
    let mut config =
        InputTimelineConfig::default().with_input_delay(InputDelayConfig::fixed_input_delay(2));
    config.sync.jitter_margin = PositiveTickDelta::lit("0.5");
    assert!(
        config.sync.error_margin >= PositiveTickDelta::lit("1"),
        "test premise: error_margin is at least 1 tick"
    );

    let timeline = LocalTimelineSync {
        input_delay_ticks: 2,
        ..Default::default()
    };
    let objective = timeline.sync_objective(&SyncSample {
        remote: &remote,
        config: &config,
        estimate,
        tick_rate,
    });

    // Controller may legitimately let `local - objective` reach
    // `-error_margin` without correcting (see `SyncHysteresis::speed_adjustment`).
    let worst_case_drift = TickDelta::try_from(config.sync.error_margin).unwrap();
    let worst_case_local = objective - worst_case_drift;
    let sent_input_tick = worst_case_local + TickDelta::from(2);

    let required_input_tick = TickInstant::from(NetTick(101));
    assert!(
        sent_input_tick >= required_input_tick,
        "worst-case sent input tick is {sent_input_tick:?}, but the \
         server reads inputs in FixedPreUpdate after receiving packets in \
         PreUpdate and advancing in FixedFirst, so the packet must contain \
         input for at least {required_input_tick:?} (= remote + 1).",
    );
}

/// At 50 Hz a tick is 20 ms, and the default jitter margin adds one tick to the RTT, so the delay
/// covers `⌈(rtt + 20 ms) / 20 ms⌉` ticks within the bounds 2, 3 and 7.
#[test]
fn the_input_delay_follows_the_rtt_within_its_bounds() {
    let sync_config = SyncConfig::default();
    let config_1 = InputDelayConfig::new(2, 3, 7);
    let rate = TickRate::try_from(50).unwrap();
    let delay = |config: InputDelayConfig, rtt| {
        config.input_delay_ticks(
            Some(LinkEstimate::new(rtt, Duration::ZERO)),
            &sync_config,
            rate,
        )
    };
    // (10 + 20) / 20 = 1.5, so 2 ticks: the minimum, 2.
    assert_eq!(delay(config_1, Duration::from_millis(10)), 2);
    // 80 / 20 = 4 ticks, past the 3 before prediction and within 3 + 7: the delay stops at 3.
    assert_eq!(delay(config_1, Duration::from_millis(60)), 3);
    // 220 / 20 = 11 and 320 / 20 = 16 ticks, past 3 + 7: the delay takes all but the 7 predicted.
    assert_eq!(delay(config_1, Duration::from_millis(200)), 4);
    assert_eq!(delay(config_1, Duration::from_millis(300)), 9);
    // An RTT past `u16::MAX` ticks saturates there, and no prediction covers any of it.
    assert_eq!(
        delay(InputDelayConfig::new(0, 0, 0), Duration::from_secs(10_000)),
        u16::MAX
    );
}

#[test]
#[should_panic(expected = "the minimum input delay exceeds the maximum before prediction")]
fn input_delay_config_rejects_a_minimum_past_the_maximum() {
    InputDelayConfig::new(3, 2, 7);
}

const TICK_RATE: TickRate = TickRate::new(core::num::NonZeroU16::new(100).unwrap());

fn remote_timeline_at(tick: u32) -> RemoteTimeline {
    let mut remote = RemoteTimeline::default();
    remote.set_now(TickInstant::from(NetTick(tick)));
    remote
}

/// Localhost: zero RTT, zero jitter.
const LOCALHOST: LinkEstimate = LinkEstimate::new(Duration::ZERO, Duration::ZERO);

#[test]
fn input_timeline_initial_sync_resyncs_to_objective() {
    let remote = remote_timeline_at(100);
    let config = InputTimelineConfig::default();
    let mut sync = LocalTimelineSync::default();

    let tick_delta = sync.sync(
        TickInstant::zero(),
        &SyncSample {
            remote: &remote,
            config: &config,
            estimate: LOCALHOST,
            tick_rate: TICK_RATE,
        },
    );

    // remote 100 + no tail margin + server input pipeline 1 tick + deadband 1 tick.
    assert_eq!(tick_delta, Some(102));
    assert!(sync.is_synced());
    assert_eq!(sync.relative_speed(), 1.0);
}

#[test]
fn input_timeline_adjusts_speed_after_repeated_error() {
    let remote = remote_timeline_at(100);
    let config = InputTimelineConfig::default().with_sync_config(SyncConfig {
        error_margin: PositiveTickDelta::lit("0.5"),
        max_error_margin: PositiveTickDelta::lit("10"),
        consecutive_errors_threshold: 2,
        speedup_factor: 1.1,
        ..default()
    });
    let mut sync = LocalTimelineSync::default();

    // The objective is remote 100 + jitter margin 1 + input pipeline 1 + deadband 0.5 = 102.5,
    // and the resync rounds its 102.5-tick delta to 103.
    assert_eq!(
        sync.sync(
            TickInstant::zero(),
            &SyncSample {
                remote: &remote,
                config: &config,
                estimate: LOCALHOST,
                tick_rate: TICK_RATE,
            },
        ),
        Some(103)
    );
    let local_now = TickInstant::from(NetTick(104));

    // An error of 104 - 102.5 = 1.5 ticks is past the 0.5 deadband, but the first one is not
    // repeated yet.
    assert_eq!(
        sync.sync(
            local_now,
            &SyncSample {
                remote: &remote,
                config: &config,
                estimate: LOCALHOST,
                tick_rate: TICK_RATE,
            },
        ),
        None
    );
    assert_eq!(sync.relative_speed(), 1.0);
    assert_eq!(
        sync.sync(
            local_now,
            &SyncSample {
                remote: &remote,
                config: &config,
                estimate: LOCALHOST,
                tick_rate: TICK_RATE,
            },
        ),
        None
    );

    // The adjustment is 1 + (1.1 - 1) * (1.5 / 10) * 2 = 1.03, and a timeline ahead slows down.
    assert_eq!(sync.relative_speed(), 1.0 / 1.03);

    // Back inside the band, 102.5 is 0 ticks off: the speed is exactly nominal again.
    assert_eq!(
        sync.sync(
            TickInstant::lit("102.5"),
            &SyncSample {
                remote: &remote,
                config: &config,
                estimate: LOCALHOST,
                tick_rate: TICK_RATE,
            },
        ),
        None
    );
    assert_eq!(sync.relative_speed(), 1.0);
}

/// The first resync applies the delay the link calls for in the same step as its objective:
/// at delay 0 the objective is 102.5, so at delay 2 it is 100.5, and from 0.5 the shift is 100.
/// A reset clears the delay with the session.
#[test]
fn a_resync_applies_the_link_input_delay_with_its_objective() {
    let remote = remote_timeline_at(100);
    let config = InputTimelineConfig::default().with_sync_config(SyncConfig {
        error_margin: PositiveTickDelta::lit("0.5"),
        ..default()
    });
    let mut sync = LocalTimelineSync {
        link_input_delay_ticks: 2,
        ..default()
    };

    assert_eq!(
        sync.sync(
            TickInstant::lit("0.5"),
            &SyncSample {
                remote: &remote,
                config: &config,
                estimate: LOCALHOST,
                tick_rate: TICK_RATE,
            },
        ),
        Some(100)
    );
    assert_eq!(sync.input_delay(), 2);

    sync.reset();
    assert_eq!(sync.input_delay(), 0);
    assert_eq!(sync.link_input_delay_ticks, 0);
}

#[test]
fn input_delay_config_update_recomputes_public_delay() {
    let mut stepper = Stepper::new(SyncStack, StepperConfig::single());
    stepper.client_app_mut(0).world_mut().insert_resource(
        InputTimelineConfig::default().with_input_delay(InputDelayConfig::fixed_input_delay(3)),
    );

    stepper.frame_step(1);

    let local_sync = stepper
        .client_app(0)
        .world()
        .resource::<LocalTimelineSync>();
    assert_eq!(local_sync.input_delay(), 3);
}
