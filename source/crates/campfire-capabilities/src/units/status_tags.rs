use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::units::engine_tag::EngineTag;
use crate::units::tag_set::TagSet;

/// The engine tags a capability gives a unit for what it is now, such as `constructing` to a
/// site, beside those of its type and its modifiers; a change of them derives the unit's tags
/// again.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct StatusTags(TagSet);

impl StatusTags {
    pub(crate) fn of(tags: impl IntoIterator<Item = EngineTag>) -> StatusTags {
        StatusTags(TagSet::of(tags.into_iter().map(EngineTag::tag)))
    }

    pub(crate) const fn get(self) -> TagSet {
        self.0
    }
}

impl SimComponent for StatusTags {
    const NAME: &'static str = "units.status_tags";

    // Engine tags alone, which every match declares first.
    fn check(&self, _: &World, _: Entity) -> bool {
        self.0.iter().all(|tag| tag.index() < EngineTag::ALL.len())
    }
}
