use campfire_common::DeclaredName;
use campfire_values::TagFilterData;

use crate::zero_hour::format::ini::object_ini::fight_data::WeaponSet;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::rules_names::RulesNames;
use crate::zero_hour::weapon_drop_cause::WeaponDropCause;

/// Which of its unit's weapon sets a weapon of a slot serves, as the tag of `PLAYER_UPGRADE`
/// chooses between them (04-capabilities/actions.md, An action may need tags): each weapon's action
/// is one of its gate, as one weapon may serve one type's starting set and another's upgrade set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WeaponGate {
    /// Both sets, or the starting set of a type of no upgrade set.
    Both,
    /// The starting set alone, while its unit lacks the tag.
    Starting,
    /// The upgrade set alone, while its unit holds the tag.
    Upgraded,
}

/// A weapon of a slot, by the slot's place, from 0 for `PRIMARY`, its gate, and the set it is of,
/// the starting set for both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct GatedWeapon<'a> {
    pub(super) slot: usize,
    pub(super) weapon: &'a str,
    pub(super) gate: WeaponGate,
    pub(super) set: &'a WeaponSet,
}

impl WeaponGate {
    /// The weapons of the starting set `starting` and of the upgrade set `upgrade`, slot by slot,
    /// each with its gate: a slot's weapon both sets hold alike serves both, and otherwise each
    /// set's weapon serves its own set, the starting set's first, as an upgrade set replaces the
    /// whole set it upgrades (`WeaponSet::updateWeaponSet`).
    pub(super) fn of<'a>(
        starting: &'a WeaponSet,
        upgrade: Option<&'a WeaponSet>,
    ) -> Vec<GatedWeapon<'a>> {
        let mut gated = Vec::with_capacity(2 * starting.weapons.len());
        for (slot, held) in starting.weapons.iter().enumerate() {
            let held = held.as_deref();
            let mut serve = |weapon: Option<&'a str>, gate, set| {
                gated.extend(weapon.map(|weapon| GatedWeapon {
                    slot,
                    weapon,
                    gate,
                    set,
                }));
            };
            let Some(upgrade) = upgrade else {
                serve(held, Self::Both, starting);
                continue;
            };
            let upgraded = upgrade.weapons[slot].as_deref();
            if held == upgraded {
                serve(held, Self::Both, starting);
            } else {
                serve(held, Self::Starting, starting);
                serve(upgraded, Self::Upgraded, upgrade);
            }
        }
        gated
    }

    /// The action of the weapon of the action `id` that serves it: `id` for both sets, else of
    /// `_starting` or `_upgraded` after it; a long name for one past the bytes a name holds.
    pub(super) fn id(self, id: &DeclaredName) -> Result<DeclaredName, WeaponDropCause> {
        let suffix = match self {
            Self::Both => return Ok(id.clone()),
            Self::Starting => "starting",
            Self::Upgraded => "upgraded",
        };
        let name = ImportName::of(id.as_str().as_bytes());
        name.suffixed(suffix).map_err(|_| WeaponDropCause::LongName)
    }

    /// The test of its unit's tags its action takes, of `names`' tag of the weapon set's
    /// `PLAYER_UPGRADE`: none for both sets.
    pub(super) fn when(self, names: &RulesNames) -> Option<TagFilterData> {
        match self {
            Self::Both => None,
            Self::Starting => Some(names.weapon_unupgraded.clone()),
            Self::Upgraded => Some(names.weapon_upgraded.clone()),
        }
    }
}

#[cfg(test)]
mod tests;
