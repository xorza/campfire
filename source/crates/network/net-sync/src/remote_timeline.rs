use crate::link_estimate::LinkEstimate;
use crate::link_estimator::LinkEstimator;
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use campfire_math::TickRate;
use campfire_net_connection::Connected;
use campfire_net_core::NetTick;
use campfire_net_core::TickInstant;
use campfire_net_core::TickSpan;
use campfire_net_link::Linked;
use campfire_net_timeline::Clock;
use campfire_net_timeline::NetworkTimeline;
use campfire_net_timeline::Timeline;
use campfire_net_timeline::TimelineRate;
use campfire_net_timeline::TimelineSetup;
use campfire_net_transport::PacketReceived;
use core::num::NonZeroU32;
use core::time::Duration;
use tracing::trace;

/// This peer's estimate of the remote peer's timeline, from the ticks its packets carry and the
/// link's latency.
#[derive(Debug, Reflect)]
pub struct RemoteEstimate {
    /// Whether a packet of the remote peer arrived this frame.
    received_packet: bool,
    /// Most recent tick received from the Server
    last_received_tick: Option<NetTick>,
    /// Our estimate of the offset of the remote timeline compared with our estimate
    /// if we just updated the timeline without any adjustments
    ///
    /// We don't touch the remote estimate directly but only modify the offset.
    offset: TickSpan,
    /// Indicator for whether this is the first estimate or not
    first_estimate: bool,
}

impl Default for RemoteEstimate {
    fn default() -> Self {
        Self {
            received_packet: false,
            last_received_tick: None,
            offset: TickSpan::from(0),
            first_estimate: true,
        }
    }
}

/// The setup of a [`RemoteTimeline`], beside the timeline it configures.
#[derive(Component, Default, Debug, Reflect)]
#[require(RemoteTimeline)]
pub struct RemoteTimelineSetup;

impl TimelineSetup for RemoteTimelineSetup {
    type State = RemoteEstimate;
}

// We need to wrap the inner Timeline to avoid the orphan rule
#[derive(Component, Default, Debug, Deref, DerefMut, Reflect)]
pub struct RemoteTimeline(Timeline<RemoteTimelineSetup>);

impl RemoteTimeline {
    /// The most recent tick received from the remote peer.
    ///
    /// # Returns
    ///
    /// An Option containing the most recent NetTick if available, or None if no ticks have been
    /// received.
    pub fn last_received_tick(&self) -> Option<NetTick> {
        self.state.last_received_tick
    }

    /// Updates the local estimate after receiving a packet from the remote peer.
    ///
    /// This method uses the received tick and network latency information to
    /// update the estimate of the current time on the remote peer.
    ///
    /// # Arguments
    ///
    /// * `remote_tick` - The tick from the remote peer's timeline included in the received packet
    /// * `estimate` - The link's latency estimate
    ///
    /// # Note
    ///
    /// This method will only update the estimate if the received tick is newer than
    /// the previously received tick.
    pub(crate) fn update(
        &mut self,
        remote_tick: NetTick,
        estimate: LinkEstimate,
        tick_rate: TickRate,
    ) {
        if self
            .state
            .last_received_tick
            .is_none_or(|previous_tick| remote_tick >= previous_tick)
        {
            // only update if the remote tick is newer than the last received tick
            self.state.received_packet = true;
            self.state.last_received_tick = Some(remote_tick);

            // we have received the packet now, so the remote must already be at RTT/2 ahead
            let network_delay = TickSpan::from_duration(estimate.rtt() / 2, tick_rate);
            let new_estimate = TickInstant::from(remote_tick) + network_delay;
            let ideal_estimate = self.now();

            let old_offset = self.offset;

            // The first estimate moves the timeline itself, and the offset stays a small
            // correction: an offset from the local origin would saturate past 2^31 ticks.
            let raw_offset = if self.state.first_estimate {
                self.set_now(new_estimate);
                self.offset = TickSpan::from(0);
                self.state.first_estimate = false;
                None
            } else {
                Some(new_estimate - ideal_estimate)
            };
            if let Some(raw_offset) = raw_offset {
                let gain = ema_gain(estimate.jitter());
                let smoothed_offset =
                    self.offset + (raw_offset - self.offset).mul_ratio(gain, EMA_GAIN_DENOMINATOR);
                trace!(?new_estimate, ?ideal_estimate, old_offset = ?self.offset, new_offset = ?smoothed_offset, jitter = ?estimate.jitter(), gain, "Update RemoteTimeline offset");
                self.offset = smoothed_offset;
            }
            trace!(
                target: "campfire_net_debug::sync",
                kind = "remote_estimate_update",
                schedule = "PreUpdate",
                sample_point = "PreUpdate",
                remote_tick = remote_tick.0,
                estimated_tick = self.current_estimate().tick().0,
                timeline_tick = self.tick().0,
                ?estimate,
                network_delay = ?network_delay,
                new_estimate = ?new_estimate,
                ideal_estimate = ?ideal_estimate,
                raw_offset = ?raw_offset,
                old_offset = ?old_offset,
                new_offset = ?self.offset,
                "remote timeline estimate updated"
            );
        }
    }

    /// On connection, reset the Remote timeline.
    pub(crate) fn handle_connect(
        trigger: On<'_, '_, Add<Connected>>,
        mut query: Query<'_, '_, &mut Self>,
    ) {
        if let Ok(mut timeline) = query.get_mut(trigger.entity) {
            timeline.received_packet = false;
            timeline.offset = TickSpan::from(0);
            timeline.first_estimate = true;
            timeline.last_received_tick = None;
        }
    }

    /// Updates the remote timeline estimate from the remote tick that each received packet carries,
    /// once the link has a latency estimate.
    pub(crate) fn update_from_packet(
        trigger: On<'_, '_, PacketReceived>,
        tick_rate: Res<'_, TimelineRate>,
        mut query: Query<'_, '_, (&mut Self, &LinkEstimator)>,
    ) {
        if let Ok((mut t, estimator)) = query.get_mut(trigger.entity) {
            trace!(
                "Received packet received with remote tick {:?}",
                trigger.remote_tick
            );
            trace!(
                target: "campfire_net_debug::timeline",
                kind = "packet_remote_tick",
                schedule = "PreUpdate",
                sample_point = "PreUpdate",
                entity = ?trigger.entity,
                remote_tick = trigger.remote_tick.0,
                "packet carried remote timeline tick"
            );
            let Some(estimate) = estimator.estimate() else {
                trace!(
                    target: "campfire_net_debug::sync",
                    kind = "remote_estimate_waiting_for_latency_samples",
                    schedule = "PreUpdate",
                    sample_point = "PreUpdate",
                    remote_tick = trigger.remote_tick.0,
                    latency_samples = estimator.samples(),
                    "remote timeline estimate skipped until latency samples are available"
                );
                return;
            };
            t.update(trigger.remote_tick, estimate, tick_rate.0);
        }
    }

    /// Advance our estimate of the remote timeline based on the real time
    ///
    /// Real time passes during a rollback too, so the advance never skips a frame.
    pub(crate) fn advance(
        clock: Res<'_, Clock>,
        tick_rate: Res<'_, TimelineRate>,
        mut query: Query<'_, '_, &mut Self, With<Linked>>,
    ) {
        let delta = clock.delta();
        query.iter_mut().for_each(|mut t| {
            t.apply_duration(delta, tick_rate.0);
        });
    }

    /// Reset the bool that tracks if we received a packet this frame
    pub(crate) fn reset_received_packet(mut query: Query<'_, '_, &mut Self, With<Linked>>) {
        query.iter_mut().for_each(|mut t| {
            t.state.received_packet = false;
        });
    }

    /// The estimate of the remote's current instant.
    pub fn current_estimate(&self) -> TickInstant {
        self.now + self.offset
    }

    /// Whether a packet arrived this frame, which a timeline may sync on.
    pub const fn has_received_packet(&self) -> bool {
        self.0.state.received_packet
    }
}

/// The jitter at or below which the offset EMA's gain is 1/10, which follows a change fast.
const LOW_JITTER: Duration = Duration::from_millis(1);
/// The jitter at or above which the gain is 1/50, which smooths the noise.
const HIGH_JITTER: Duration = Duration::from_millis(10);
/// The span between them, in nanoseconds.
const JITTER_SPAN_NANOS: u32 = 9_000_000;
/// The denominator of [`ema_gain`]: 50 · the span, as 1/10 is 5/50.
const EMA_GAIN_DENOMINATOR: NonZeroU32 = NonZeroU32::new(50 * JITTER_SPAN_NANOS).unwrap();

/// The numerator of the offset EMA's gain at `jitter`, linear between the gains at
/// [`LOW_JITTER`] and [`HIGH_JITTER`]: with `p` the jitter past the low one, clamped to the span,
/// the gain is `1/10 · (span - p) / span + 1/50 · p / span = (5 · (span - p) + p) / (50 · span)`.
fn ema_gain(jitter: Duration) -> u32 {
    let past = jitter
        .saturating_sub(LOW_JITTER)
        .min(HIGH_JITTER - LOW_JITTER);
    let past = u32::try_from(past.as_nanos()).expect("the clamp keeps it under the span");
    5 * (JITTER_SPAN_NANOS - past) + past
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Over the denominator of 450_000_000, 1/10 is 45_000_000 and 1/50 is 9_000_000: each holds
    /// past its bound, 5.5 ms gives their mean, 27_000_000, and each nanosecond past 1 ms takes
    /// 5 - 1 = 4 off the numerator, so 1.5 ms gives 43_000_000.
    #[test]
    fn ema_gain_follows_the_jitter_band() {
        let gain = |micros| ema_gain(Duration::from_micros(micros));
        assert_eq!(gain(500), 45_000_000);
        assert_eq!(gain(1_000), 45_000_000);
        assert_eq!(gain(10_000), 9_000_000);
        assert_eq!(gain(20_000), 9_000_000);
        assert_eq!(gain(5_500), 27_000_000);
        assert_eq!(gain(1_500), 43_000_000);
        assert_eq!(ema_gain(Duration::from_nanos(1_000_001)), 45_000_000 - 4);
    }

    use campfire_net_connection::RemoteId;
    use campfire_net_core::PeerId;

    const TICK: TickRate = TickRate::hertz(NonZeroU32::new(100).unwrap());

    fn span(text: &str) -> TickSpan {
        TickSpan::try_from(text.parse::<campfire_math::Num>().unwrap()).unwrap()
    }

    /// A 20 ms round trip is a 1-tick delay: the remote is at least one tick past the tick its
    /// packet carried.
    fn estimate(jitter: Duration) -> LinkEstimate {
        LinkEstimate::new(Duration::from_millis(20), jitter)
    }

    /// The first estimate moves the timeline to 100 + 1 with no offset; an older tick then
    /// changes nothing, and a tick past 2^31, which an offset from 0 would saturate at, estimates
    /// as exactly as a small one.
    #[test]
    fn the_first_update_takes_the_raw_offset_and_an_older_tick_is_ignored() {
        let mut timeline = RemoteTimeline::default();
        timeline.update(NetTick(100), estimate(Duration::ZERO), TICK);
        assert_eq!(timeline.last_received_tick(), Some(NetTick(100)));
        assert_eq!(timeline.offset, TickSpan::from(0));
        assert_eq!(timeline.current_estimate(), TickInstant::from(NetTick(101)));
        assert!(timeline.has_received_packet());

        timeline.update(NetTick(99), estimate(Duration::ZERO), TICK);
        assert_eq!(timeline.last_received_tick(), Some(NetTick(100)));
        assert_eq!(timeline.current_estimate(), TickInstant::from(NetTick(101)));

        let mut late = RemoteTimeline::default();
        late.update(NetTick(3_000_000_000), estimate(Duration::ZERO), TICK);
        assert_eq!(
            late.current_estimate(),
            TickInstant::from(NetTick(3_000_000_001))
        );
    }

    /// A later estimate moves the offset by the jitter's gain: the timeline is at 101, so 110 + 1
    /// is a raw offset of 10. The gain is 1/10 at 1 ms or less, so 1; 1/50 at 10 ms or more, so
    /// 0.2; and at 5.5 ms, halfway, (5 · 4.5 + 4.5) / (50 · 9) = 0.06, so 0.6. A second update
    /// with the same raw offset moves the offset by the gain of the rest: 1 + 9/10 = 1.9. Each
    /// rounds to the nearest 2^-24 tick, as its literal does: 0.6 to 10,066,330 steps.
    #[test]
    fn a_later_update_smooths_the_offset_by_the_jitter() {
        let smoothed = |jitter, updates| {
            let mut timeline = RemoteTimeline::default();
            timeline.update(NetTick(100), estimate(jitter), TICK);
            for _ in 0..updates {
                timeline.update(NetTick(110), estimate(jitter), TICK);
            }
            timeline.offset
        };
        assert_eq!(smoothed(Duration::ZERO, 1), TickSpan::from(1));
        assert_eq!(smoothed(Duration::from_millis(1), 1), TickSpan::from(1));
        assert_eq!(smoothed(Duration::from_micros(5_500), 1), span("0.6"));
        assert_eq!(smoothed(Duration::from_millis(10), 1), span("0.2"));
        assert_eq!(smoothed(Duration::from_secs(1), 1), span("0.2"));
        assert_eq!(smoothed(Duration::ZERO, 2), span("1.9"));
    }

    /// A connection resets the estimate: the next packet is a first estimate again.
    #[test]
    fn a_connection_resets_the_estimate() {
        let mut world = World::new();
        world.add_observer(RemoteTimeline::handle_connect);
        let mut timeline = RemoteTimeline::default();
        timeline.update(NetTick(100), estimate(Duration::ZERO), TICK);
        let entity = world.spawn((timeline, RemoteId(PeerId::Server))).id();
        world.entity_mut(entity).insert(Connected);
        let timeline = world.get::<RemoteTimeline>(entity).unwrap();
        assert_eq!(timeline.last_received_tick(), None);
        assert_eq!(timeline.offset, TickSpan::from(0));
        assert!(timeline.first_estimate);
        assert!(!timeline.has_received_packet());
    }
}
