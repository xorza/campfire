use super::*;
use crate::fixtures::sync_stack::SyncStack;
use crate::input_delay_setup::InputDelaySetup;
use crate::remote_timeline::RemoteTimeline;
use campfire_net_core::{NetTick, PositiveTickSpan};
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;
use campfire_net_timeline::LocalTimeline;
use core::time::Duration;

use crate::synced_local_timeline::SyncedLocalTimeline;
use bevy_app::{App, Update};

use bevy_utils::default;

use campfire_net_timeline::NetworkTimeline;

fn num(text: &str) -> campfire_math::Num {
    text.parse().unwrap()
}

fn positive(text: &str) -> PositiveTickSpan {
    PositiveTickSpan::try_from(num(text)).unwrap()
}

fn instant(text: &str) -> TickInstant {
    TickInstant::try_from(num(text)).unwrap()
}

#[derive(Debug, Resource, Default)]
struct SyncedParamRuns(u8);

#[test]
fn the_synced_local_timeline_waits_for_its_sync() {
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
fn the_local_objective_leads_by_its_delays_and_two_ticks_whatever_its_band() {
    let tick_rate = TickRate::try_from(100).unwrap();
    let mut remote = RemoteTimeline::default();
    remote.set_now(TickInstant::from(NetTick(100)));

    let estimate = LinkEstimate::new(Duration::from_millis(40), Duration::from_millis(5));

    let objective = |error_margin| {
        let mut setup = InputTimelineSetup::default();
        setup.sync.jitter_multiple = 2;
        setup.sync.jitter_margin = positive("1");
        setup.sync.error_margin = positive(error_margin);
        LocalTimelineSync::default().sync_objective(&SyncSample {
            remote: &remote,
            setup: &setup,
            estimate,
            tick_rate,
        })
    };

    // remote 100 + RTT/2 2 ticks + jitter margin 2 ticks + server input pipeline 1 tick + a tick
    // early 1 tick; the band, which lies ahead of the objective, moves it none.
    assert_eq!(objective("0.75"), instant("106"));
    assert_eq!(objective("1"), instant("106"));
}

#[test]
fn the_input_delay_still_offsets_the_local_objective() {
    let tick_rate = TickRate::try_from(100).unwrap();
    let mut remote = RemoteTimeline::default();
    remote.set_now(TickInstant::from(NetTick(100)));

    let estimate = LinkEstimate::new(Duration::from_millis(40), Duration::from_millis(5));

    let mut setup =
        InputTimelineSetup::default().with_input_delay(InputDelaySetup::fixed_input_delay(2));
    setup.sync.jitter_multiple = 2;
    setup.sync.jitter_margin = positive("1");
    setup.sync.error_margin = positive("0.75");

    let timeline = LocalTimelineSync {
        input_delay_ticks: 2,
        ..Default::default()
    };

    let objective = timeline.sync_objective(&SyncSample {
        remote: &remote,
        setup: &setup,
        estimate,
        tick_rate,
    });

    assert_eq!(objective, instant("104"));
}

/// The server reads inputs in `FixedPreUpdate`, after receiving packets in `PreUpdate` and
/// advancing its tick in `FixedFirst`, so an input must target `remote + 1` at least, and the
/// objective keeps one tick more for a stall of every process. The controller steps any deficit
/// (`SyncHysteresis::speed_adjustment`), so the local timeline stands at its objective at the
/// least, and the input it sends there targets `remote + 2`.
#[test]
fn the_sync_objective_keeps_a_sent_input_a_tick_early() {
    let tick_rate = TickRate::try_from(100).unwrap();
    let mut remote = RemoteTimeline::default();
    remote.set_now(TickInstant::from(NetTick(100)));

    // Localhost — zero RTT, zero jitter.
    let estimate = LinkEstimate::new(Duration::ZERO, Duration::ZERO);

    // A `jitter_margin` tightened below 1 tick, with an input delay of 2: the local tick may stand
    // behind `remote + 2`; the input it sends may not.
    let mut setup =
        InputTimelineSetup::default().with_input_delay(InputDelaySetup::fixed_input_delay(2));
    setup.sync.jitter_margin = positive("0.5");

    let timeline = LocalTimelineSync {
        input_delay_ticks: 2,
        ..Default::default()
    };
    let objective = timeline.sync_objective(&SyncSample {
        remote: &remote,
        setup: &setup,
        estimate,
        tick_rate,
    });
    // 100 + 0.5 + 2 − 2.
    assert_eq!(objective, instant("100.5"));
    let sent_input_tick = objective + TickSpan::from(2);
    assert!(sent_input_tick >= TickInstant::from(NetTick(102)));
}

/// At 50 Hz a tick is 20 ms, and the default jitter margin adds one tick to the RTT, so the delay
/// covers `⌈(rtt + 20 ms) / 20 ms⌉` ticks within the bounds 2, 3 and 7.
#[test]
fn the_input_delay_follows_the_rtt_within_its_bounds() {
    let sync_setup = SyncSetup::default();
    let config_1 = InputDelaySetup::new(2, 3, 7);
    let rate = TickRate::try_from(50).unwrap();
    let delay = |setup: InputDelaySetup, rtt| {
        setup.input_delay_ticks(
            Some(LinkEstimate::new(rtt, Duration::ZERO)),
            &sync_setup,
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
        delay(InputDelaySetup::new(0, 0, 0), Duration::from_secs(10_000)),
        u16::MAX
    );
}

#[test]
#[should_panic(expected = "the minimum input delay exceeds the maximum before prediction")]
fn an_input_delay_setup_refuses_a_min_above_its_max() {
    InputDelaySetup::new(3, 2, 7);
}

const TICK_RATE: TickRate = TickRate::hertz(core::num::NonZeroU32::new(100).unwrap());

fn remote_timeline_at(tick: u32) -> RemoteTimeline {
    let mut remote = RemoteTimeline::default();
    remote.set_now(TickInstant::from(NetTick(tick)));
    remote
}

/// Localhost: zero RTT, zero jitter.
const LOCALHOST: LinkEstimate = LinkEstimate::new(Duration::ZERO, Duration::ZERO);

#[test]
fn the_first_sync_of_the_input_timeline_jumps_to_its_objective() {
    let remote = remote_timeline_at(100);
    let setup = InputTimelineSetup::default();
    let mut sync = LocalTimelineSync::default();

    let tick_span = sync.sync(
        TickInstant::ZERO,
        &SyncSample {
            remote: &remote,
            setup: &setup,
            estimate: LOCALHOST,
            tick_rate: TICK_RATE,
        },
    );

    // remote 100 + no tail margin + server input pipeline 1 tick + deadband 1 tick.
    assert_eq!(tick_span, Some(102));
    assert!(sync.is_synced());
    assert_eq!(sync.relative_speed(), 1.0);
}

#[test]
fn the_input_timeline_changes_its_speed_after_a_repeated_error() {
    let remote = remote_timeline_at(100);
    let setup = InputTimelineSetup::default().with_sync_setup(SyncSetup {
        error_margin: positive("0.5"),
        max_error_margin: positive("10"),
        consecutive_errors_threshold: 2,
        speedup_factor: 1.1,
        ..default()
    });
    let mut sync = LocalTimelineSync::default();

    // The objective is remote 100 + jitter margin 1 + input pipeline 1 + a tick early 1 = 103,
    // and the resync shifts by its 103 ticks.
    assert_eq!(
        sync.sync(
            TickInstant::ZERO,
            &SyncSample {
                remote: &remote,
                setup: &setup,
                estimate: LOCALHOST,
                tick_rate: TICK_RATE,
            },
        ),
        Some(103)
    );
    let local_now = TickInstant::from(NetTick(104));

    // An error of 104 - 103 = 1 tick is past the 0.5 band ahead, but the first one is not
    // repeated yet.
    assert_eq!(
        sync.sync(
            local_now,
            &SyncSample {
                remote: &remote,
                setup: &setup,
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
                setup: &setup,
                estimate: LOCALHOST,
                tick_rate: TICK_RATE,
            },
        ),
        None
    );

    // The adjustment is 1 + (1.1 - 1) * (1 / 10) * 2 = 1.02, and a timeline ahead slows down.
    assert_eq!(sync.relative_speed(), 1.0 / 1.02);

    // Back inside the band, 103.25 is a quarter of a tick ahead: the speed is exactly nominal
    // again.
    assert_eq!(
        sync.sync(
            instant("103.25"),
            &SyncSample {
                remote: &remote,
                setup: &setup,
                estimate: LOCALHOST,
                tick_rate: TICK_RATE,
            },
        ),
        None
    );
    assert_eq!(sync.relative_speed(), 1.0);
    // A quarter of a tick behind, inside what a band about the objective would keep, it steps
    // up to its objective: 0.25 ticks, rounded up to 1.
    assert_eq!(
        sync.sync(
            instant("102.75"),
            &SyncSample {
                remote: &remote,
                setup: &setup,
                estimate: LOCALHOST,
                tick_rate: TICK_RATE,
            },
        ),
        Some(1)
    );
}

/// The first resync applies the delay the link calls for in the same step as its objective:
/// at delay 0 the objective is 103, so at delay 2 it is 101, and from 0.5 the shift of 100.5 is
/// rounded up to 101, ahead of the objective.
/// A reset clears the delay with the link.
#[test]
fn a_resync_applies_the_link_input_delay_with_its_objective() {
    let remote = remote_timeline_at(100);
    let setup = InputTimelineSetup::default().with_sync_setup(SyncSetup {
        error_margin: positive("0.5"),
        ..default()
    });
    let mut sync = LocalTimelineSync {
        link_input_delay_ticks: 2,
        ..default()
    };

    assert_eq!(
        sync.sync(
            instant("0.5"),
            &SyncSample {
                remote: &remote,
                setup: &setup,
                estimate: LOCALHOST,
                tick_rate: TICK_RATE,
            },
        ),
        Some(101)
    );
    assert_eq!(sync.input_delay(), 2);

    sync.reset();
    assert_eq!(sync.input_delay(), 0);
    assert_eq!(sync.link_input_delay_ticks, 0);
}

#[test]
fn a_change_of_the_input_delay_setup_computes_the_delay_again() {
    let mut stepper = Stepper::new(SyncStack, StepperSetup::single());
    stepper.client_world_mut(0).insert_resource(
        InputTimelineSetup::default().with_input_delay(InputDelaySetup::fixed_input_delay(3)),
    );

    stepper.frame_step(1);

    let local_sync = stepper.client_world(0).resource::<LocalTimelineSync>();
    assert_eq!(local_sync.input_delay(), 3);
}
