use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::RemovedComponents;
use bevy_ecs::query::{Added, Changed, Or};
use bevy_ecs::system::{Local, Query, Res, SystemParam};
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, Replication, SentOnChange};
use campfire_units::{Dead, Owner, Player};
use campfire_values::UnitType;
use serde::{Deserialize, Serialize};

use crate::site::Site;
use crate::supply::{Counted, Supply};
use crate::supply_book::SupplyBook;
use crate::supply_cost_book::SupplyCostBook;
use crate::train_queue::TrainQueue;

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

    /// Counts each player's supply anew, in a mode that counts it, when a part it counts or a
    /// player changed since the last run, and writes it on each player's entity where it
    /// changed. Only the server counts it, which a client receives.
    pub(super) fn refresh(
        (rules, costs): (Res<'_, SupplyBook>, Res<'_, SupplyCostBook>),
        units: Query<'_, '_, Counted>,
        mut players: Query<'_, '_, (&Player, &mut Self)>,
        mut changes: SupplyChanges<'_, '_>,
        mut supply: Local<'_, Supply>,
    ) {
        if !changes.any() {
            return;
        }
        let rules = rules.0.expect("supply is counted in a mode that counts it");
        supply.count(&costs, rules, units.iter());
        for (player, mut held) in &mut players {
            held.set_if_neq(Self {
                used: supply.used(player.slot()),
                cap: supply.cap(player.slot()),
            });
        }
    }
}

/// What changes a player's supply: a unit's owner, type or queue that changed, or one that died,
/// came back, became a site or completed, or went; and a player that joined. No `Debug`:
/// `RemovedComponents` has none.
#[derive(SystemParam)]
pub(super) struct SupplyChanges<'w, 's> {
    changed: Query<
        'w,
        's,
        (),
        Or<(
            Changed<Owner>,
            Changed<UnitType>,
            Changed<TrainQueue>,
            Added<Dead>,
            Added<Site>,
        )>,
    >,
    players: Query<'w, 's, (), Added<Player>>,
    lived: RemovedComponents<'w, 's, Dead>,
    completed: RemovedComponents<'w, 's, Site>,
    unowned: RemovedComponents<'w, 's, Owner>,
    untyped: RemovedComponents<'w, 's, UnitType>,
    unqueued: RemovedComponents<'w, 's, TrainQueue>,
}

impl SupplyChanges<'_, '_> {
    /// Whether anything supply counts changed since the last run; it reads every removal, so
    /// the next run reads only those after it.
    fn any(&mut self) -> bool {
        let removed = [
            self.lived.read().count(),
            self.completed.read().count(),
            self.unowned.read().count(),
            self.untyped.read().count(),
            self.unqueued.read().count(),
        ];
        removed.iter().any(|&count| count != 0)
            || !self.changed.is_empty()
            || !self.players.is_empty()
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
