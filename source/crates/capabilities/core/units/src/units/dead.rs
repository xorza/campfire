use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, Replication, SentPredicted};
use campfire_values::{EngineTag, TagProperties, TagSet};
use serde::{Deserialize, Serialize};

use crate::units::unit_entity::UnitEntity;

/// Marks a unit that died and stays for the mode to respawn. A dead unit takes no orders, does not
/// move and is no target.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dead;

impl Dead {
    /// The engine tag of a dead unit, which a filter names to select the dead, or to leave them
    /// out, as a shop's buyers do. Every filter that does not name it selects the dead as the
    /// living, and only a shop's buyers meet a dead unit: every query skips them.
    pub const TAG: EngineTag = EngineTag::new(1, "dead", TagProperties::NONE);

    /// `tags`, with `TAG` when `dead`, else without it: a unit's tags as it stands now, which its
    /// derived tags hold once their next refresh ran.
    pub const fn tagged(tags: TagSet, dead: bool) -> TagSet {
        if dead {
            tags.with(Dead::TAG.tag())
        } else {
            tags.without(Dead::TAG.tag())
        }
    }
}

impl SimComponent for Dead {
    const NAME: &'static str = "units.dead";
    type Host = UnitEntity;

    // A mark, which holds nothing.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Dead {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
