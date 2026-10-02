use std::time::Duration;

use bevy_ecs::component::Component;
use bevy_ecs::system::Query;
use lightyear::link::SendPayload;
use lightyear::prelude::{Link, PingManager};

use crate::local_match::link_model::LinkModel;

/// Holds the packets a link sends for as long as its model says, counted in the frames of its
/// app, and loses those the model loses.
#[derive(Component, Debug)]
pub(crate) struct DelayLine {
    model: LinkModel,
    /// Frames per step of the match.
    frames: u32,
    frame: u64,
    /// The state of a `SplitMix64` draw, seeded by the model.
    draw: u64,
    /// Packets not delivered yet, with the frame each is due in and the order it was sent in.
    held: Vec<Held>,
    sent: u64,
}

#[derive(Debug)]
struct Held {
    due: u64,
    order: u64,
    payload: SendPayload,
}

impl DelayLine {
    /// A line of `model` in an app that runs `frames` frames a step; `stream` keeps the draws of
    /// two lines with the same model apart.
    pub(crate) const fn new(model: LinkModel, frames: u32, stream: u64) -> DelayLine {
        DelayLine {
            model,
            frames,
            frame: 0,
            draw: model.seed ^ stream.wrapping_mul(0x9E37_79B9_7F4A_7C15),
            held: Vec::new(),
            sent: 0,
        }
    }

    /// Takes the packets each link sent this frame into its line, and gives back to the link those
    /// due by now, in the order they are due, then in the order they were sent.
    pub(crate) fn pass(mut links: Query<'_, '_, (&mut Link, &mut DelayLine)>) {
        for (mut link, mut line) in &mut links {
            line.frame += 1;
            while let Some(payload) = link.send.pop() {
                line.hold(payload);
            }
            let now = line.frame;
            line.held.sort_by_key(|held| (held.due, held.order));
            let due = line.held.partition_point(|held| held.due <= now);
            for held in line.held.drain(..due) {
                link.send.push(held.payload);
            }
        }
    }

    /// Sets every link's measured round trip and jitter to zero. Lightyear measures them by the
    /// wall clock, which a step of a local match hardly takes, so under load they vary from run
    /// to run and change the input timeline. The `=0.30.1` pin of Lightyear keeps the two fields
    /// this writes where they are; the modeled round trip reaches the timeline through the sync
    /// margin instead.
    pub(crate) fn pin_round_trip(mut links: Query<'_, '_, (&mut Link, Option<&mut PingManager>)>) {
        for (mut link, ping) in &mut links {
            link.stats.rtt = Duration::ZERO;
            link.stats.jitter = Duration::ZERO;
            if let Some(mut ping) = ping {
                let stats = &mut ping.rtt_estimator_ewma.final_stats;
                stats.rtt = Duration::ZERO;
                stats.jitter = Duration::ZERO;
            }
        }
    }

    fn hold(&mut self, payload: SendPayload) {
        let LinkModel {
            delay,
            jitter,
            loss_per_mille,
            ..
        } = self.model;
        if self.next() % 1000 < u64::from(loss_per_mille) {
            return;
        }
        let steps = u64::from(delay) + self.next() % (u64::from(jitter) + 1);
        self.held.push(Held {
            due: self.frame + steps * u64::from(self.frames),
            order: self.sent,
            payload,
        });
        self.sent += 1;
    }

    /// The next `SplitMix64` draw.
    const fn next(&mut self) -> u64 {
        self.draw = self.draw.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut mixed = self.draw;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        mixed ^ (mixed >> 31)
    }
}
