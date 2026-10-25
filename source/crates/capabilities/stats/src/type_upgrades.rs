use campfire_common::DeclaredName;
use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_script::{Filter, UnknownName};
use campfire_units::UnitTypes;
use campfire_values::{ModifierId, Named, PackageIndex, TagSet, UnitType};

use crate::modifier_book::ModifierBook;
use crate::upgrades_data::UpgradesData;

/// A unit type's upgrades, as its `upgrades` names them (upgrades.md, A unit type takes modifiers
/// by upgrades).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeUpgrades(pub Box<[TypeUpgrade]>);

/// One upgrade of a type: its triggers, whether it needs all of them, its conflicts, the tags its
/// unit must pass, and the modifier it gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeUpgrade {
    pub triggers: Box<[ModifierId]>,
    pub all: bool,
    pub conflicts: Box<[ModifierId]>,
    pub when: Option<Filter>,
    pub modifier: ModifierId,
}

impl TypeUpgrade {
    /// Whether a unit of `tags` that holds each modifier `holds` says it holds meets it: its tags
    /// pass `when`, it holds any of its triggers, or every one with `all`, and none of its
    /// conflicts.
    pub fn met(&self, tags: TagSet, holds: impl Fn(ModifierId) -> bool) -> bool {
        let triggered = if self.all {
            self.triggers.iter().all(|&trigger| holds(trigger))
        } else {
            self.triggers.iter().any(|&trigger| holds(trigger))
        };
        self.when.is_none_or(|when| when.selects_tags(tags))
            && triggered
            && !self.conflicts.iter().any(|&conflict| holds(conflict))
    }
}

impl TypeUpgrades {
    /// The upgrades of each unit type that stands with some, of modifiers and tags of its package;
    /// an error for a name the package does not have.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let modifiers = cx.book::<ModifierBook>()?;
        let tags = cx.fact::<UnitTypes>()?.tags();
        let mut upgrades = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<UpgradesData>() else {
                continue;
            };
            let at = || BookPlace::UnitType(unit.place());
            let fail = |unknown| BookError::unknown(unit.package, at(), unknown);
            let named =
                |name: &DeclaredName| Self::modifier(modifiers, unit.package, name).map_err(fail);
            let mut entries = Vec::with_capacity(data.0.len());
            for entry in &data.0 {
                let when = entry
                    .when
                    .as_ref()
                    .map(|when| Filter::resolve(when.filter(), tags));
                entries.push(TypeUpgrade {
                    triggers: entry.triggers.iter().map(named).collect::<Result<_, _>>()?,
                    all: entry.all,
                    conflicts: entry
                        .conflicts
                        .iter()
                        .map(named)
                        .collect::<Result<_, _>>()?,
                    when: when.transpose().map_err(fail)?,
                    modifier: named(&entry.modifier)?,
                });
            }
            upgrades.set(unit.unit_type, Self(entries.into()));
        }
        Ok(upgrades.build())
    }

    /// The modifier `name` of `package`; an error for one it does not have.
    fn modifier(
        modifiers: &ModifierBook,
        package: PackageIndex,
        name: &DeclaredName,
    ) -> Result<ModifierId, UnknownName> {
        let named = modifiers.names().named((package, name.as_str()));
        named.ok_or_else(|| UnknownName::new(ModifierId::NAMES, name))
    }
}

impl PerId<UnitType> for TypeUpgrades {
    const SCRIPTS_ONLY: bool = false;
}

#[cfg(test)]
mod tests {
    use campfire_script::TagNames;

    use super::*;

    #[test]
    fn an_upgrade_is_met_by_any_trigger_or_all_by_no_conflict_and_by_its_tags() {
        // Modifiers 0, 1 and 2; the tag `constructing`, which a site holds.
        let mut names = TagNames::default();
        let constructing = names.names_mut().add("constructing");
        let [drill, walls, mines] = [0, 1, 2].map(ModifierId::new);
        let entry = |triggers: &[ModifierId], all, conflicts: &[ModifierId], when: Option<&str>| {
            TypeUpgrade {
                triggers: triggers.into(),
                all,
                conflicts: conflicts.into(),
                when: when.map(|text| Filter::parse(text, &names).unwrap()),
                modifier: ModifierId::new(9),
            }
        };
        let none = TagSet::NONE;
        let site = none.with(constructing);
        let holding = |held: &[ModifierId]| {
            let held = held.to_vec();
            move |modifier| held.contains(&modifier)
        };
        let any = entry(&[drill, walls], false, &[], None);
        let both = entry(&[drill, walls], true, &[], None);
        let unmined = entry(&[drill], false, &[mines], None);
        let built = entry(&[drill], false, &[], Some("all:!constructing"));
        // Any trigger: one of them, not none.
        assert!(any.met(none, holding(&[walls])) && !any.met(none, holding(&[mines])));
        // All: both, not one.
        assert!(both.met(none, holding(&[drill, walls])) && !both.met(none, holding(&[drill])));
        // A conflict held refuses what its trigger meets.
        assert!(unmined.met(none, holding(&[drill])));
        assert!(!unmined.met(none, holding(&[drill, mines])));
        // `when`: a site's tag refuses it, a complete unit's lack of it passes.
        assert!(built.met(none, holding(&[drill])) && !built.met(site, holding(&[drill])));
    }
}
