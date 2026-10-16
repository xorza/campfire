use std::time::Duration;

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use campfire_common::PlayerSlot;

/// Each slot's seat, as the server seats players after the start.
#[derive(Resource, Debug)]
pub(crate) struct Seats(Vec<Seat>);

/// A slot's seat: the link its player plays through, and, while their link is gone, the
/// server's real time it went at, from which the grace period runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Seat {
    pub(crate) link: Option<Entity>,
    pub(crate) gone_since: Option<Duration>,
}

impl Seats {
    /// The seats of `slots` slots, each empty, as a start finds them before it seats its
    /// players.
    pub(crate) fn empty(slots: u32) -> Self {
        Self(vec![
            Seat::default();
            usize::try_from(slots).expect("slots fit usize")
        ])
    }

    /// The seats of `slots` slots, whose players' links all went at `now`, as a restore finds
    /// them.
    pub(crate) fn gone(
        slots: u32,
        players: impl IntoIterator<Item = PlayerSlot>,
        now: Duration,
    ) -> Self {
        let mut seats = vec![Seat::default(); usize::try_from(slots).expect("slots fit usize")];
        for slot in players {
            seats[slot.index()].gone_since = Some(now);
        }
        Self(seats)
    }

    pub(crate) fn get(&self, slot: PlayerSlot) -> Seat {
        self.0[slot.index()]
    }

    pub(crate) fn set(&mut self, slot: PlayerSlot, seat: Seat) {
        self.0[slot.index()] = seat;
    }

    /// Each slot, with its seat.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (PlayerSlot, Seat)> + '_ {
        (0..).map(PlayerSlot::new).zip(self.0.iter().copied())
    }
}
