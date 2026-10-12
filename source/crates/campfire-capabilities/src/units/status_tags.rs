use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentPredicted;
use crate::units::engine_tag::EngineTag;
use crate::units::tag_book::TagBook;
use crate::units::tag_set::TagSet;
use crate::units::unit_entity::UnitEntity;

/// The engine tags a capability gives a unit for what it is now, such as `constructing` to a
/// site, beside those of its type and its modifiers; a change of them derives the unit's tags
/// again.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct StatusTags(TagSet);

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
    type Host = UnitEntity;

    // Engine tags alone, which every match declares first; a match with no tag book has none.
    fn check(&self, world: &World, _: Entity) -> bool {
        world
            .get_resource::<TagBook>()
            .map_or(self.0 == TagSet::default(), |book| {
                self.0.iter().all(|tag| tag.index() < book.engine_count())
            })
    }
}

impl Replication for StatusTags {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::production::Production;

    #[test]
    fn a_turn_changes_its_own_tag_alone() {
        let site = StatusTags::from([Production::CONSTRUCTING]);
        let both = StatusTags::from([Production::CONSTRUCTING, Production::GATHERING]);
        assert_eq!(site.turned(Production::GATHERING, true), both);
        assert_eq!(both.turned(Production::GATHERING, false), site);
        assert_eq!(site.turned(Production::CONSTRUCTING, true), site);
        assert_eq!(
            site.turned(Production::CONSTRUCTING, false),
            StatusTags::default()
        );
    }
}
