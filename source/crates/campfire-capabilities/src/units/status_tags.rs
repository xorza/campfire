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

    /// It with `tag` when `on`, else without it; every other tag stays, as each belongs to the
    /// capability that gives it.
    #[must_use]
    pub(crate) fn turned(self, tag: EngineTag, on: bool) -> StatusTags {
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

    // Engine tags alone, which every match declares first.
    fn check(&self, _: &World, _: Entity) -> bool {
        self.0.iter().all(|tag| tag.index() < EngineTag::ALL.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_changes_its_own_tag_alone() {
        let site = StatusTags::of([EngineTag::Constructing]);
        let both = StatusTags::of([EngineTag::Constructing, EngineTag::Gathering]);
        assert_eq!(site.turned(EngineTag::Gathering, true), both);
        assert_eq!(both.turned(EngineTag::Gathering, false), site);
        assert_eq!(site.turned(EngineTag::Constructing, true), site);
        assert_eq!(
            site.turned(EngineTag::Constructing, false),
            StatusTags::default()
        );
    }
}
