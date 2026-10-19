use std::time::Duration;

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use campfire_common::Seat;

/// Each seat's link, as the server seats players after the start.
#[derive(Resource, Debug)]
pub(crate) struct SeatLinks(Vec<SeatLink>);

/// A seat's link: the link its player plays through, and, while their link is gone, the
/// server's real time it went at, from which the grace period runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SeatLink {
    pub(crate) link: Option<Entity>,
    pub(crate) gone_since: Option<Duration>,
}

impl SeatLinks {
    /// The links of `seats` seats, each empty, as a start finds them before it seats its
    /// players.
    pub(crate) fn empty(seats: u32) -> Self {
        Self(vec![
            SeatLink::default();
            usize::try_from(seats).expect("seats fit usize")
        ])
    }

    /// The links of `seats` seats, whose players' links all went at `now`, as a restore finds
    /// them.
    pub(crate) fn gone(seats: u32, players: impl IntoIterator<Item = Seat>, now: Duration) -> Self {
        let mut links = vec![SeatLink::default(); usize::try_from(seats).expect("seats fit usize")];
        for seat in players {
            links[seat.index()].gone_since = Some(now);
        }
        Self(links)
    }

    pub(crate) fn get(&self, seat: Seat) -> SeatLink {
        self.0[seat.index()]
    }

    pub(crate) fn set(&mut self, seat: Seat, link: SeatLink) {
        self.0[seat.index()] = link;
    }

    /// Each seat, with its link.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (Seat, SeatLink)> + '_ {
        (0..).map(Seat::new).zip(self.0.iter().copied())
    }
}
