use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::system::{Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_players::Player;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentOnChange};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::holdings::{Holdings, OwnedUnits};
use crate::supply_book::SupplyBook;

/// A player's supply, on its entity, as its kind's audience sees it (10-replication-audiences.md,
/// The match and each player are entities): what its living units and queued trains use, and its
/// cap. The server derives it as each tick ends, as a train counts it, and sends it on each change:
/// a client holds only the units it sees, and no hash or snapshot holds it.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerSupply {
    used: u64,
    cap: u64,
}

impl PlayerSupply {
    pub const fn used(self) -> u64 {
        self.used
    }

    pub const fn cap(self) -> u64 {
        self.cap
    }

    /// Writes each player's supply on its entity, where it changed, in a mode that counts it, as
    /// the holdings follow the units. Only the server counts it, which a client receives.
    pub(super) fn refresh(
        rules: Res<'_, SupplyBook>,
        owned: OwnedUnits<'_, '_>,
        mut holdings: ResMut<'_, Holdings>,
        mut players: Query<'_, '_, (&Player, &mut Self)>,
    ) {
        let rules = rules.0.expect("supply is counted in a mode that counts it");
        holdings.follow(&owned);
        let max = u64::from(rules.max);
        for (player, mut held) in &mut players {
            held.set_if_neq(Self {
                used: holdings.used(player.seat()),
                cap: holdings.cap(player.seat(), max),
            });
        }
    }
}

impl SimComponent for PlayerSupply {
    const NAME: &'static str = "production.supply";
    type Host = Player;

    // Derived, so no snapshot holds one for a restore to check.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for PlayerSupply {
    const KIND: DataKind = DataKind::Supply;
    type Sending = SentOnChange;
}
