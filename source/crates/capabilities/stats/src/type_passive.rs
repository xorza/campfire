use bevy_ecs::change_detection::{DetectChanges, Ref};
use bevy_ecs::entity::Entity;
use bevy_ecs::system::{Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_names::UnknownName;
use campfire_sim::StableId;
use campfire_values::{ModifierId, Named, Rank, UnitType};

use crate::Stats;
use crate::applier::Applier;
use crate::grants::{GrantFrom, Grants};
use crate::lifetime::Hold;
use crate::modifier_adds::ModifierAdd;
use crate::modifier_book::ModifierBook;
use crate::modifiers::Modifiers;
use crate::passive_data::PassiveData;

/// The modifier a unit type's units hold from their spawn on, from themselves, under the type's own
/// hold: its passive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypePassive(pub ModifierId);

impl TypePassive {
    /// The passive of each unit type that stands with one, a modifier of its package; an error for
    /// a modifier the package does not have.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let modifiers = cx.book::<ModifierBook>()?;
        let mut passives = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(passive) = unit.data.section::<PassiveData>() else {
                continue;
            };
            let named = modifiers.names().named((unit.package, passive.0.as_str()));
            let unknown = || UnknownName::new(ModifierId::NAMES, &passive.0);
            let at = || BookPlace::UnitType(unit.place());
            let id = named.ok_or_else(|| BookError::unknown(unit.package, at(), unknown()))?;
            passives.set(unit.unit_type, Self(id));
        }
        Ok(passives.build())
    }
}

impl PerId<UnitType> for TypePassive {
    const SCRIPTS_ONLY: bool = false;
}

impl TypePassive {
    /// Grants the unit of `entity`, which the mode's spawner spawned, its type's passive, held
    /// from itself at the first rank under the type's hold, as `GrantPass` grants in its passes:
    /// the spawn callback of stats, so it holds it from its first tick. A unit that spawns holds
    /// nothing from itself yet.
    pub(crate) fn grant(world: &mut World, entity: Entity) {
        let unit = world.entity(entity);
        let unit_type = *unit.get::<UnitType>().expect("a unit has a type");
        let id = *unit.get::<StableId>().expect("a unit has a stable id");
        let passive = world.resource::<ById<UnitType, Self>>().get(unit_type);
        let Some(&Self(modifier)) = passive else {
            return;
        };
        debug_assert!(
            unit.get::<Modifiers>()
                .is_some_and(|held| held.get(modifier, Some(id)).is_none()),
            "a unit that spawns holds its passive from no grant yet"
        );
        let applier = Applier {
            source: Some(id),
            action: None,
            rank: Rank::FIRST,
            hold: Some(Hold::Type),
        };
        let add = ModifierAdd {
            target: id,
            id: modifier,
            applier,
            duration: None,
        };
        Stats::add_modifier(world, add);
    }

    /// Pushes the grants of each unit's type's passive, under the type's hold from the unit itself
    /// at rank 1, as `want` of every grant source does, so a passive a script removed comes back
    /// at the next pass, and no action's passive of the same modifier releases it
    /// (04-capabilities/stats.md, Passives by one pass). What a unit wants follows from its type
    /// and its modifiers, so a run visits only the units whose modifiers changed, unless a book
    /// changed.
    pub(crate) fn want(
        (passives, book): (Res<'_, ById<UnitType, Self>>, Res<'_, ModifierBook>),
        mut grants: ResMut<'_, Grants>,
        units: Query<'_, '_, (Entity, &StableId, &UnitType, Ref<'_, Modifiers>)>,
    ) {
        if passives.is_empty() {
            return;
        }
        let every = passives.is_changed() || book.is_changed();
        for (entity, &id, &unit_type, modifiers) in &units {
            if !(every || modifiers.is_changed()) {
                continue;
            }
            let wanted = passives.get(unit_type).map(|passive| passive.0);
            grants.of(entity, id).hold_exactly(
                Hold::Type,
                GrantFrom::Itself,
                wanted.as_slice(),
                Some(&modifiers),
            );
        }
    }
}
