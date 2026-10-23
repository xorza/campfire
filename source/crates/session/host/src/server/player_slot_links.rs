use std::time::Duration;

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use campfire_common::PlayerSlot;

/// Each player slot's link, as the server player slots players after the start.
#[derive(Resource, Debug)]
pub struct PlayerSlotLinks(Vec<PlayerSlotLink>);

/// A player slot's link: the link its player plays through, and, while their link is gone, the
/// server's real time it went at, from which the grace period runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayerSlotLink {
    pub link: Option<Entity>,
    pub gone_since: Option<Duration>,
}

impl PlayerSlotLinks {
    /// The links of `player_slots` player slots, each empty, as a start finds them before it player
    /// slots its players.
    pub fn empty(player_slots: u32) -> Self {
        Self(vec![
            PlayerSlotLink::default();
            usize::try_from(player_slots)
                .expect("player slots fit usize")
        ])
    }

    /// The links of `player_slots` player slots, whose players' links all went at `now`, as a
    /// restore finds them.
    pub fn gone(
        player_slots: u32,
        players: impl IntoIterator<Item = PlayerSlot>,
        now: Duration,
    ) -> Self {
        let mut links = vec![
            PlayerSlotLink::default();
            usize::try_from(player_slots).expect("player slots fit usize")
        ];
        for player_slot in players {
            links[player_slot.index()].gone_since = Some(now);
        }
        Self(links)
    }

    pub fn get(&self, player_slot: PlayerSlot) -> PlayerSlotLink {
        self.0[player_slot.index()]
    }

    pub fn set(&mut self, player_slot: PlayerSlot, link: PlayerSlotLink) {
        self.0[player_slot.index()] = link;
    }

    /// Each player slot, with its link.
    pub fn iter(&self) -> impl Iterator<Item = (PlayerSlot, PlayerSlotLink)> + '_ {
        (0..).map(PlayerSlot::new).zip(self.0.iter().copied())
    }
}
