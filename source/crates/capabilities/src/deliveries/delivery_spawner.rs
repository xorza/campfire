use std::fmt;
use std::iter;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::system::{Commands, Query, Res, ResMut, SystemParam};
use campfire_sim::{EntityIndex, IdAllocator, Position, StableId};

use crate::units::owner::Owner;
use crate::units::tag_book::TagBook;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;

/// Spawns the units that actions deliver, projectiles and areas, each for its source.
#[derive(SystemParam)]
pub(crate) struct DeliverySpawner<'w, 's> {
    commands: Commands<'w, 's>,
    ids: ResMut<'w, IdAllocator>,
    index: Res<'w, EntityIndex>,
    tag_book: Option<Res<'w, TagBook>>,
    sources: Query<'w, 's, (&'static Team, Option<&'static Owner>)>,
}

/// `Commands` prints nothing, so a spawner prints only what it is.
impl fmt::Debug for DeliverySpawner<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeliverySpawner")
    }
}

impl DeliverySpawner<'_, '_> {
    /// Spawns a unit of `unit_type` at `at` with what `make` gives for its new stable id, of
    /// `source`'s team and player, with its type's tags, and gives its id; none, and no id
    /// taken, when `source` is gone.
    pub(crate) fn spawn<B: Bundle>(
        &mut self,
        source: StableId,
        at: Position,
        unit_type: UnitType,
        make: impl FnOnce(StableId) -> B,
    ) -> Option<StableId> {
        let (&team, owner) = self
            .index
            .get(source)
            .and_then(|entity| self.sources.get(entity).ok())?;
        let id = self.ids.allocate();
        let tags = self
            .tag_book
            .as_deref()
            .map_or(UnitTags::default(), |book| {
                book.unit_tags(unit_type, iter::empty())
            });
        let mut unit = self
            .commands
            .spawn((id, at, unit_type, team, tags, make(id)));
        if let Some(&owner) = owner {
            unit.insert(owner);
        }
        Some(id)
    }
}
