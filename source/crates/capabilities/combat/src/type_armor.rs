use std::collections::BTreeMap;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::DeclaredName;
use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_names::{Filter, UnknownName};
use campfire_units::UnitTypes;
use campfire_values::{DamageKind, Named, Names, TagSet, UnitTags, UnitType};

use crate::Combat;
use crate::armor::Armor;
use crate::combat_data::CombatData;
use crate::mode_combat_data::ModeCombatData;

/// The armors of a unit type: the one its unit takes damage through, and those it takes while its
/// tags pass a test, the first that passes first (04-capabilities/combat.md, Armor by tags).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeArmor {
    pub armor: Option<Armor>,
    pub armors: Box<[TaggedArmor]>,
}

/// An armor a unit takes damage through while its tags pass `when`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggedArmor {
    pub when: Filter,
    pub armor: Armor,
}

impl TypeArmor {
    /// The armor a unit of `tags` takes damage through: the first of `armors` whose test its tags
    /// pass, else `armor`; none takes the whole.
    pub fn of(&self, tags: TagSet) -> Option<&Armor> {
        let tagged = self
            .armors
            .iter()
            .find(|tagged| tagged.when.selects_tags(tags));
        tagged.map(|tagged| &tagged.armor).or(self.armor.as_ref())
    }
}

impl TypeArmor {
    /// The armor the unit `entity` of `world` takes damage through, by its type and its tags; none
    /// takes the whole.
    pub(crate) fn of_unit(world: &World, entity: Entity) -> Option<&Armor> {
        let unit_type = world.get::<UnitType>(entity)?;
        let tags = world.get::<UnitTags>(entity);
        let armors = world.get_resource::<ById<UnitType, Self>>()?;
        armors
            .get(*unit_type)?
            .of(tags.map_or(TagSet::NONE, |tags| tags.tags))
    }
}

impl PerId<UnitType> for TypeArmor {
    const SCRIPTS_ONLY: bool = false;
}

impl TypeArmor {
    /// The armors of each unit type that stands with one, of the mode's armors by name, each
    /// tagged one by its test of the types' tags; an error for an armor, a damage kind or a tag
    /// the mode does not declare.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let none = ModeCombatData::default();
        let data = cx.mode_section::<ModeCombatData>().unwrap_or(&none);
        let kinds = cx.names_if::<DamageKind>()?;
        let no_kinds = Names::default();
        let kinds = kinds.map_or(&no_kinds, |kinds| &**kinds);
        let armors = data.armors.iter().map(|(name, shares)| {
            let armor = Armor::of(shares, kinds).map_err(Combat::unknown)?;
            Ok((name, armor))
        });
        let armors = armors.collect::<Result<BTreeMap<_, _>, BookError>>()?;
        let tags = cx.fact::<UnitTypes>()?.tags();
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(combat) = unit.data.section::<CombatData>() else {
                continue;
            };
            if combat.armor.is_none() && combat.armors.is_empty() {
                continue;
            }
            let fail = |unknown| {
                BookError::unknown(unit.package, BookPlace::UnitType(unit.place()), unknown)
            };
            let named = |name: &DeclaredName| {
                let armor = armors.get(name).cloned();
                armor.ok_or_else(|| fail(UnknownName::new(Armor::NAMES, name)))
            };
            let tagged = combat.armors.iter().map(|tagged| {
                Ok(TaggedArmor {
                    when: Filter::resolve(tagged.when.filter(), tags).map_err(fail)?,
                    armor: named(&tagged.armor)?,
                })
            });
            let armor = Self {
                armor: combat.armor.as_ref().map(named).transpose()?,
                armors: tagged.collect::<Result<_, BookError>>()?,
            };
            book.set(unit.unit_type, armor);
        }
        Ok(book.build())
    }
}
