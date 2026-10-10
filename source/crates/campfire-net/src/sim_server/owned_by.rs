use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_replicon::prelude::VisibilityFilter;
use campfire_capabilities::{Inventory, ModifierClocks, Progress, Respawn, Route, SpawnPoint};
use campfire_common::PlayerSlot;

use crate::sim_server::player_link::PlayerLink;

/// On each replicated unit: the slot whose player owns it, if one does. Replicon reads it to send
/// the components only the owner's prediction reads, or design 04 sends to the owner alone, to
/// the owner's link alone: where the unit respawns and when, its route and its progress along
/// it, its modifiers' clocks, and its inventory. `Route`
/// resends its whole list on a change and `Progress` changes each tick the unit walks, so every
/// other link would pay for state it never reads. Immutable, as replicon requires: a change of
/// owner inserts a new one.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[component(immutable)]
pub(crate) struct OwnedBy(pub(super) Option<PlayerSlot>);

impl VisibilityFilter for OwnedBy {
    type ClientComponent = PlayerLink;
    type Scope = (
        SpawnPoint,
        Respawn,
        Route,
        Progress,
        ModifierClocks,
        Inventory,
    );

    fn is_visible(&self, _client: Entity, link: Option<&PlayerLink>) -> bool {
        link.is_some_and(|link| Some(link.slot()) == self.0)
    }
}

#[cfg(test)]
mod tests {
    use campfire_capabilities::Team;

    use super::*;

    #[test]
    fn only_the_owners_seated_link_sees_the_owners_state() {
        let link = |slot| PlayerLink::new(PlayerSlot::new(slot), Team::new(0));
        let client = Entity::PLACEHOLDER;
        let owned = OwnedBy(Some(PlayerSlot::new(1)));
        assert!(owned.is_visible(client, Some(&link(1))));
        assert!(!owned.is_visible(client, Some(&link(0))));
        // A link with no seat, and a unit no player owns, as a creep: none sees it.
        assert!(!owned.is_visible(client, None));
        assert!(!OwnedBy(None).is_visible(client, Some(&link(1))));
    }
}
