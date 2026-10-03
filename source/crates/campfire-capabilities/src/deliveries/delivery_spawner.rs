use std::fmt;
use std::iter;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::system::{Commands, Query, Res, ResMut, SystemParam};
use campfire_sim::{EntityIndex, IdAllocator, Position, StableId};

use crate::units::by_type::ByType;
use crate::units::new_unit_states::NewUnitStates;
use crate::units::owner::Owner;
use crate::units::tag_book::TagBook;
use crate::units::team::Team;
use crate::units::unit_state_book::UnitStateBook;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;
use crate::vision::sight::Sight;

/// Spawns the units that actions deliver, projectiles and areas, each for its source.
#[derive(SystemParam)]
pub(crate) struct DeliverySpawner<'w, 's> {
    commands: Commands<'w, 's>,
    ids: ResMut<'w, IdAllocator>,
    index: Res<'w, EntityIndex>,
    tag_book: Option<Res<'w, TagBook>>,
    states: Option<Res<'w, UnitStateBook>>,
    new_states: Option<ResMut<'w, NewUnitStates>>,
    sights: Option<Res<'w, ByType<Sight>>>,
    sources: Query<'w, 's, (&'static Team, Option<&'static Owner>)>,
}

/// `Commands` prints nothing, so a spawner prints only what it is.
impl fmt::Debug for DeliverySpawner<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeliverySpawner")
    }
}

impl DeliverySpawner<'_, '_> {
    /// Spawns a unit of `unit_type` at `at` with what `make` gives for its stable id, `id`, which
    /// a script took for it, or else a new one, of `source`'s team and player, with its type's
    /// tags, its sight if its type has one, and its script state at their defaults but what the
    /// script wrote, and gives its id;
    /// none, and no id taken, when `source` is gone.
    pub(crate) fn spawn<B: Bundle>(
        &mut self,
        source: StableId,
        at: Position,
        unit_type: UnitType,
        id: Option<StableId>,
        make: impl FnOnce(StableId) -> B,
    ) -> Option<StableId> {
        let (&team, owner) = self
            .index
            .get(source)
            .and_then(|entity| self.sources.get(entity).ok())?;
        let id = id.unwrap_or_else(|| self.ids.allocate());
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
        if let Some(&sight) = self.sights.as_deref().and_then(|book| book.get(unit_type)) {
            unit.insert(sight);
        }
        let state = self
            .states
            .as_deref()
            .and_then(|book| book.initial(unit_type));
        if let Some(mut state) = state {
            if let Some(new_states) = self.new_states.as_deref_mut() {
                new_states.take(id, &mut state);
            }
            unit.insert(state);
        }
        Some(id)
    }
}
