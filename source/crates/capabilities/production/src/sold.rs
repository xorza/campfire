use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_declarations::ById;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_values::UnitType;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::sell_data::SellSpec;
use crate::site::Site;

/// A building its player sold: the tick its sale started and the tick it ends, when its refund is
/// paid and it goes with no death. Every client that holds its unit receives it, so each sinks
/// its drawing as its sale runs (04-capabilities/production.md, A building may be sold).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(storage = "SparseSet")]
pub struct Sold {
    since: Tick,
    until: Tick,
}

impl Sold {
    /// A sale from `since` for `time`.
    pub const fn new(since: Tick, time: Ticks) -> Self {
        Self {
            since,
            until: since.after(time),
        }
    }

    pub const fn since(self) -> Tick {
        self.since
    }

    pub const fn until(self) -> Tick {
        self.until
    }
}

impl SimComponent for Sold {
    const NAME: &'static str = "production.sold";
    type Host = UnitEntity;

    // A sale of a building whose type sells, no site, that ends its type's time after it starts.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let sales = world.resource::<ById<UnitType, SellSpec>>();
        let spec = world
            .get::<UnitType>(entity)
            .and_then(|&unit_type| sales.get(unit_type));
        spec.is_some_and(|spec| self.until == self.since.after(spec.time))
            && !world.entity(entity).contains::<Site>()
    }
}

impl Replication for Sold {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
