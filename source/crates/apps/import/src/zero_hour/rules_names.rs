use campfire_common::DeclaredName;
use campfire_values::{FilterData, FilterTag, RelationSet, Stat, TagFilterData};

use crate::zero_hour::format::ini::object_ini::object_kinds::ObjectKind;

/// The names the import's stages write that the rules package beside the mode declares,
/// `source/packages/zero-hour/data/rules.toml`, each checked once as the import's rules are
/// read, and the filters the stages write, made of them: a stage takes a name, never its text.
/// The rules package's test checks that it declares each.
#[derive(Debug, Clone)]
pub(crate) struct RulesNames {
    /// The player resource every cost is in.
    pub(crate) money: DeclaredName,
    /// The mode's pool of life, and its stat of the most life.
    pub(crate) life: DeclaredName,
    pub(crate) life_stat: Stat,
    /// The stat a stage of continuous fire adds to.
    pub(crate) rate_of_fire: Stat,
    /// The stats a level of veterancy adds to every weapon's damage and rate of fire
    /// (04-capabilities/combat.md, A weapon's bonus is a sum of stats).
    pub(crate) veterancy_damage: Stat,
    pub(crate) veterancy_rate: Stat,
    /// The stats a garrison's modifier adds to its riders' weapons' range and damage, and the
    /// modifier, which the import gives of `GameData.ini`'s `GARRISONED` bonuses
    /// (04-capabilities/combat.md, A container's modifier holds on its riders).
    pub(crate) garrison_range: Stat,
    pub(crate) garrison_damage: Stat,
    pub(crate) garrisoned: DeclaredName,
    /// The tag of a type that counts for a multiplayer victory (09-zero-hour.md, Defeat by the
    /// last building that counts).
    pub(crate) counts_for_victory: DeclaredName,
    /// The tags of a structure, a wall, a base defense, infantry, a vehicle and an aircraft, of the
    /// objects' `KindOf` and `FenceWidth`.
    pub(crate) structure: DeclaredName,
    pub(crate) wall: DeclaredName,
    pub(crate) base_defense: DeclaredName,
    pub(crate) infantry: DeclaredName,
    pub(crate) vehicle: DeclaredName,
    pub(crate) aircraft: DeclaredName,
    /// The tags of a huge vehicle and of one no garrison takes, which a container refuses
    /// (04-capabilities/interaction.md, A container says who rides).
    pub(crate) huge_vehicle: DeclaredName,
    pub(crate) no_garrison: DeclaredName,
    /// The tag of a unit that gives its player a radar, which the minimap shows by
    /// (02-engine-core.md, A minimap by a radar tag).
    pub(crate) radar: DeclaredName,
    /// The tags an upgrade module gives its unit as it sets the armor set's or the weapon set's
    /// `PLAYER_UPGRADE` (09-zero-hour.md, Zero Hour's upgrade flags are tags and stats).
    pub(crate) armor_upgrade: DeclaredName,
    pub(crate) weapon_upgrade: DeclaredName,
    /// The tags of a unit at a stage of continuous fire, `MEAN` and `FAST`, and of one fallen from
    /// a stage, `SLOW`.
    pub(crate) continuous_fire: [DeclaredName; 2],
    pub(crate) continuous_fire_slow: DeclaredName,
    /// The tests of a unit of each, and of one of no `weapon_upgrade`.
    pub(crate) armor_upgraded: TagFilterData,
    pub(crate) weapon_upgraded: TagFilterData,
    pub(crate) weapon_unupgraded: TagFilterData,
    /// The tag of a unit in its second life, which its modifier gives and its armor tests, and its
    /// test (defenses-and-airfields.md, A type may live twice).
    pub(crate) second_life: DeclaredName,
    pub(crate) in_second_life: TagFilterData,
    /// The tags of the markers the players start at and their starting units rally at, which a
    /// map's `Player_<n>_Start` and `Player_<n>_Rally` waypoints carry, and their param, the
    /// player slot, from 0 (09-zero-hour.md, A skirmish starts with the faction's start objects).
    pub(crate) start: DeclaredName,
    pub(crate) rally: DeclaredName,
    pub(crate) player_slot: DeclaredName,
    /// The tag of a marker made from a waypoint, which the mode's scripts find the waypoints by,
    /// and its param, the waypoint's name.
    pub(crate) waypoint: DeclaredName,
    pub(crate) waypoint_name: DeclaredName,
    /// The layer a flier's body moves on (04-capabilities/navigation.md, Fliers share a layer).
    pub(crate) air: DeclaredName,
    /// The slot kinds of a type's weapons and of its trains, builds and gather.
    pub(crate) weapon_slot: DeclaredName,
    pub(crate) command_slot: DeclaredName,
    /// The slot kind of a rider's entry, and the entry, an action of the `enter` kind
    /// (04-capabilities/interaction.md, An entry is an action of the container's).
    pub(crate) use_slot: DeclaredName,
    pub(crate) enter: DeclaredName,
    /// Every unit, of no tag.
    pub(crate) every: FilterData,
    /// Every unit of a team the unit's own regards as hostile.
    pub(crate) hostiles: FilterData,
    /// Every unit its own team may attack.
    pub(crate) enemies: FilterData,
    /// Every unit its own team may attack that is, or is not, an air target, the engine's tag
    /// (09-zero-hour.md, A weapon's anti flags are its targets).
    pub(crate) enemies_in_air: FilterData,
    pub(crate) enemies_on_ground: FilterData,
    /// Every unit of a team the unit's own regards as friendly.
    pub(crate) allies: FilterData,
    /// Every unit that is no structure.
    pub(crate) no_structures: FilterData,
    /// Every structure its own team may attack, and every structure of a friendly team.
    pub(crate) enemy_structures: FilterData,
    pub(crate) allied_structures: FilterData,
    /// Every wall.
    pub(crate) walls: FilterData,
}

impl RulesNames {
    /// The tag of each kind of `KindOf` that a tag of its own says, which a type of the kind
    /// holds and a container's filters test.
    pub(crate) const fn kind_tags(&self) -> [(ObjectKind, &DeclaredName); 7] {
        [
            (ObjectKind::Structure, &self.structure),
            (ObjectKind::FsBaseDefense, &self.base_defense),
            (ObjectKind::Infantry, &self.infantry),
            (ObjectKind::Vehicle, &self.vehicle),
            (ObjectKind::Aircraft, &self.aircraft),
            (ObjectKind::HugeVehicle, &self.huge_vehicle),
            (ObjectKind::NoGarrison, &self.no_garrison),
        ]
    }
}

impl Default for RulesNames {
    fn default() -> Self {
        let name = |text| DeclaredName::new(text).expect("the rules package's names are names");
        let stat = |name: &DeclaredName| Stat::named(name.as_str()).expect("a name is a stat's");
        let filter = |relations, tag: Option<(&DeclaredName, bool)>| FilterData {
            relations,
            tags: tag
                .map(|(name, negated)| FilterTag {
                    name: name.clone(),
                    negated,
                })
                .into_iter()
                .collect(),
        };
        let life = name("life");
        let air_target = name("air_target");
        let structure = name("structure");
        let wall = name("wall");
        let armor_upgrade = name("armor_upgrade");
        let weapon_upgrade = name("weapon_upgrade");
        let second_life = name("second_life");
        let tagged = |name: &DeclaredName, negated| {
            TagFilterData::new([FilterTag {
                name: name.clone(),
                negated,
            }])
        };
        Self {
            money: name("money"),
            life_stat: stat(&life),
            rate_of_fire: stat(&name("rate_of_fire")),
            veterancy_damage: stat(&name("veterancy_damage")),
            veterancy_rate: stat(&name("veterancy_rate")),
            garrison_range: stat(&name("garrison_range")),
            garrison_damage: stat(&name("garrison_damage")),
            garrisoned: name("garrisoned"),
            huge_vehicle: name("huge_vehicle"),
            no_garrison: name("no_garrison"),
            use_slot: name("use"),
            enter: name("enter"),
            counts_for_victory: name("counts_for_victory"),
            base_defense: name("base_defense"),
            radar: name("radar"),
            infantry: name("infantry"),
            vehicle: name("vehicle"),
            aircraft: name("aircraft"),
            start: name("start"),
            rally: name("rally"),
            player_slot: name("player_slot"),
            waypoint: name("waypoint"),
            waypoint_name: name("name"),
            air: name("air"),
            weapon_slot: name("weapon"),
            command_slot: name("command"),
            every: FilterData::all(),
            hostiles: filter(RelationSet::Hostiles, None),
            enemies: filter(RelationSet::Enemies, None),
            enemies_in_air: filter(RelationSet::Enemies, Some((&air_target, false))),
            enemies_on_ground: filter(RelationSet::Enemies, Some((&air_target, true))),
            allies: filter(RelationSet::Allies, None),
            no_structures: filter(RelationSet::All, Some((&structure, true))),
            enemy_structures: filter(RelationSet::Enemies, Some((&structure, false))),
            allied_structures: filter(RelationSet::Allies, Some((&structure, false))),
            walls: filter(RelationSet::All, Some((&wall, false))),
            armor_upgraded: tagged(&armor_upgrade, false),
            weapon_upgraded: tagged(&weapon_upgrade, false),
            weapon_unupgraded: tagged(&weapon_upgrade, true),
            in_second_life: tagged(&second_life, false),
            second_life,
            armor_upgrade,
            weapon_upgrade,
            continuous_fire: [name("continuous_fire_mean"), name("continuous_fire_fast")],
            continuous_fire_slow: name("continuous_fire_slow"),
            life,
            structure,
            wall,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_filter_is_the_one_its_text_in_data_gives() {
        let names = RulesNames::default();
        for (filter, text) in [
            (&names.every, "all"),
            (&names.hostiles, "hostiles"),
            (&names.enemies, "enemies"),
            (&names.enemies_in_air, "enemies:air_target"),
            (&names.enemies_on_ground, "enemies:!air_target"),
            (&names.allies, "allies"),
            (&names.no_structures, "all:!structure"),
            (&names.enemy_structures, "enemies:structure"),
            (&names.allied_structures, "allies:structure"),
            (&names.walls, "all:wall"),
        ] {
            assert_eq!(*filter, FilterData::parse(text).unwrap(), "{text}");
        }
        for (filter, text) in [
            (&names.armor_upgraded, "all:armor_upgrade"),
            (&names.weapon_upgraded, "all:weapon_upgrade"),
            (&names.weapon_unupgraded, "all:!weapon_upgrade"),
            (&names.in_second_life, "all:second_life"),
        ] {
            assert_eq!(*filter.filter(), FilterData::parse(text).unwrap(), "{text}");
        }
    }
}
