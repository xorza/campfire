use std::collections::{BTreeMap, BTreeSet};

use campfire_combat::SecondLifeData;
use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_values::{Number, Stat};
use serde::Serialize;

use crate::zero_hour::format::ini::damage_type::DamageType;
use crate::zero_hour::rules_names::RulesNames;

/// The modifiers of the types' second lives, one for each raise of their most life, shared among
/// the types of that raise, `second_life_<n>` from 1 in the raises' order, each giving the tag of
/// a unit in its second life (defenses-and-airfields.md, A type may live twice).
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct SecondLifeImport {
    names: BTreeMap<Num, DeclaredName>,
    pub(crate) modifiers: BTreeMap<DeclaredName, SecondLifeModifier>,
}

/// A second life's modifier as the rules data writes it: what it adds to the life stat, and the
/// tag it gives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SecondLifeModifier {
    stats: BTreeMap<Stat, Number>,
    tags: Vec<DeclaredName>,
}

impl SecondLifeImport {
    /// The modifiers of `raises`, those of the import's types that live twice, of `names`' life
    /// stat and second life's tag.
    pub(crate) fn of(raises: impl IntoIterator<Item = Num>, names: &RulesNames) -> Self {
        let raises: BTreeSet<Num> = raises.into_iter().collect();
        let mut import = Self::default();
        for (at, raise) in (1..).zip(raises) {
            let name = DeclaredName::new(&format!("second_life_{at}"))
                .expect("a modifier's name the import gives");
            let modifier = SecondLifeModifier {
                stats: BTreeMap::from([(names.life_stat.clone(), Number::from(raise))]),
                tags: vec![names.second_life.clone()],
            };
            import.names.insert(raise, name.clone());
            import.modifiers.insert(name, modifier);
        }
        import
    }

    /// The `second_life` of a type whose second life raises its most life by `raise`: its
    /// modifier, and `UNRESISTABLE` damage killing through it, as `UndeadBody::attemptDamage`
    /// passes it.
    pub(crate) fn data(&self, raise: Num) -> SecondLifeData {
        SecondLifeData {
            modifier: self.names[&raise].clone(),
            lethal: vec![DamageType::Unresistable.kind()],
        }
    }
}

#[cfg(test)]
mod tests;
