use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::system::{Local, Query, Res};
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, Replication, SentOnChange};
use campfire_units::Player;
use serde::{Deserialize, Serialize};

use crate::supply::{CountedUnit, Supply};
use crate::supply_book::SupplyBook;
use crate::supply_cost_book::{SupplyCostBook, SupplyUnit};
use crate::trains::Counted;

/// A player's supply, on its entity, as its kind's audience sees it (design 14, D7): what its
/// living units and queued trains use, and its cap. The server derives it as each tick ends,
/// as a train counts it, and sends it on each change: a client holds only the units it sees, and
/// no hash or snapshot holds it.
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

    /// Counts each player's supply anew, in a mode that counts it, and writes it on each player's
    /// entity where it changed. Only the server counts it, which a client receives.
    pub(super) fn refresh(
        rules: Res<'_, SupplyBook>,
        costs: Res<'_, SupplyCostBook>,
        units: Query<'_, '_, Counted>,
        mut players: Query<'_, '_, (&Player, &mut Self)>,
        mut supply: Local<'_, Supply>,
    ) {
        let rules = rules.0.expect("supply is counted in a mode that counts it");
        let counted = units
            .iter()
            .map(|(&unit_type, owner, dead, queue, site)| CountedUnit {
                owner: owner.slot(),
                unit: SupplyUnit {
                    unit_type,
                    dead,
                    complete: !site,
                    queue,
                },
            });
        supply.count(&costs, rules, counted);
        for (player, mut held) in &mut players {
            held.set_if_neq(Self {
                used: supply.used(player.slot()),
                cap: supply.cap(player.slot()),
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
