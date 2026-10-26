use campfire_common::DeclaredName;
use campfire_values::{FilterData, FilterTag, RelationSet, Stat, TagFilterData};

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
    /// The tag of a type that counts for a multiplayer victory (09-zero-hour.md, Defeat by the
    /// last building that counts).
    pub(crate) counts_for_victory: DeclaredName,
    /// The tags of a structure, a wall, a base defense and infantry, of the objects' `KindOf`
    /// and `FenceWidth`.
    pub(crate) structure: DeclaredName,
    pub(crate) wall: DeclaredName,
    pub(crate) base_defense: DeclaredName,
    pub(crate) infantry: DeclaredName,
    /// The tags an upgrade module gives its unit as it sets the armor set's or the weapon set's
    /// `PLAYER_UPGRADE` (09-zero-hour.md, Zero Hour's upgrade flags are tags and stats).
    pub(crate) armor_upgrade: DeclaredName,
    pub(crate) weapon_upgrade: DeclaredName,
    /// The tests of a unit of each, and of one of no `weapon_upgrade`.
    pub(crate) armor_upgraded: TagFilterData,
    pub(crate) weapon_upgraded: TagFilterData,
    pub(crate) weapon_unupgraded: TagFilterData,
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
    /// The slot kinds of a type's weapons and of its trains, builds and gather.
    pub(crate) weapon_slot: DeclaredName,
    pub(crate) command_slot: DeclaredName,
    /// Every unit, of no tag.
    pub(crate) every: FilterData,
    /// Every unit of a team the unit's own regards as hostile.
    pub(crate) hostiles: FilterData,
    /// Every unit its own team may attack.
    pub(crate) enemies: FilterData,
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
        let structure = name("structure");
        let wall = name("wall");
        let armor_upgrade = name("armor_upgrade");
        let weapon_upgrade = name("weapon_upgrade");
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
            counts_for_victory: name("counts_for_victory"),
            base_defense: name("base_defense"),
            infantry: name("infantry"),
            start: name("start"),
            rally: name("rally"),
            player_slot: name("player_slot"),
            waypoint: name("waypoint"),
            waypoint_name: name("name"),
            weapon_slot: name("weapon"),
            command_slot: name("command"),
            every: FilterData::all(),
            hostiles: filter(RelationSet::Hostiles, None),
            enemies: filter(RelationSet::Enemies, None),
            allies: filter(RelationSet::Allies, None),
            no_structures: filter(RelationSet::All, Some((&structure, true))),
            enemy_structures: filter(RelationSet::Enemies, Some((&structure, false))),
            allied_structures: filter(RelationSet::Allies, Some((&structure, false))),
            walls: filter(RelationSet::All, Some((&wall, false))),
            armor_upgraded: tagged(&armor_upgrade, false),
            weapon_upgraded: tagged(&weapon_upgrade, false),
            weapon_unupgraded: tagged(&weapon_upgrade, true),
            armor_upgrade,
            weapon_upgrade,
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
        ] {
            assert_eq!(*filter.filter(), FilterData::parse(text).unwrap(), "{text}");
        }
    }
}
