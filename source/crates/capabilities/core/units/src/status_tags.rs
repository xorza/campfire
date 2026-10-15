use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, Replication, SentPredicted};
use campfire_values::{EngineTag, NameId, TagSet};
use serde::{Deserialize, Serialize};

use crate::tag_book::TagBook;
use crate::unit_entity::UnitEntity;

/// The engine tags a capability gives a unit for what it is now, such as `constructing` to a
/// site, beside those of its type and its modifiers; a change of them derives the unit's tags
/// again.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StatusTags(TagSet);

impl FromIterator<EngineTag> for StatusTags {
    fn from_iter<I: IntoIterator<Item = EngineTag>>(tags: I) -> StatusTags {
        StatusTags(tags.into_iter().map(EngineTag::tag).collect())
    }
}

impl<const N: usize> From<[EngineTag; N]> for StatusTags {
    fn from(tags: [EngineTag; N]) -> StatusTags {
        tags.into_iter().collect()
    }
}

impl StatusTags {
    pub const fn get(self) -> TagSet {
        self.0
    }

    /// It with `tag` when `on`, else without it; every other tag stays, as each belongs to the
    /// capability that gives it.
    #[must_use]
    pub const fn turned(self, tag: EngineTag, on: bool) -> StatusTags {
        let tag = tag.tag();
        StatusTags(if on {
            self.0.with(tag)
        } else {
            self.0.without(tag)
        })
    }
}

impl SimComponent for StatusTags {
    const NAME: &'static str = "units.status_tags";
    type Host = UnitEntity;

    // Engine tags alone, which every match declares first; a match whose tag book is empty, or
    // a world with none, as a copy of a match's state, has none.
    fn check(&self, world: &World, _: Entity) -> bool {
        let engine = world
            .get_resource::<TagBook>()
            .map_or(0, TagBook::engine_count);
        self.0.iter().all(|tag| tag.index() < engine)
    }
}

impl Replication for StatusTags {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
