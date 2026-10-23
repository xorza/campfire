use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_script::UnknownName;
use campfire_sim::StableId;
use campfire_values::{ModifierId, Named, Rank, UnitType};

use crate::Stats;
use crate::applier::Applier;
use crate::lifetime::Hold;
use crate::modifier_adds::ModifierAdd;
use crate::modifier_book::ModifierBook;
use crate::modifiers::Modifiers;
use crate::passive_data::PassiveData;

/// The modifier a unit type's units hold from their spawn on, from themselves: its passive.
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
    /// from itself at the first rank, as `GrantPass` grants in its passes: the spawn callback of
    /// stats. A unit that spawns holds nothing from itself yet.
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
            hold: Some(Hold::Passive),
        };
        let add = ModifierAdd {
            target: id,
            id: modifier,
            applier,
            duration: None,
        };
        Stats::add_modifier(world, add);
    }
}
