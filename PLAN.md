# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Upgrades

The slice of [Upgrades](docs/design/upgrades.md): a player in a Zero Hour skirmish researches player and object upgrades from the panel, and sees them change units' armor, weapon sets, weapon damage and life. Two batches left: the import; then the panel, the check and the fold. Its checks are that document's.

### U4. Zero Hour's upgrades (import)

- [ ] The import reads no `Upgrade.ini`, converts `UNIT_BUILD` and `DOZER_CONSTRUCT` buttons alone (`format/ini/command_button_ini.rs:48`) and takes the weapon and armor sets of no condition alone (`format/ini/object_ini/fight_data.rs:343`, `:348`) → it reads the upgrades, turns each `PLAYER_UPGRADE` and `OBJECT_UPGRADE` button of a command set into a research of its producer, with its cost, time and requirements, and each `ArmorUpgrade`, `WeaponSetUpgrade`, `WeaponBonusUpgrade` and `MaxHealthUpgrade` into its type's `upgrades` and a modifier shared by the types it changes alike; the sets of `PLAYER_UPGRADE` become `armors` and `when`, and a weapon's `PLAYER_UPGRADE` damage bonus its `damage_bonus` stat (upgrades.md, Zero Hour's upgrade flags are tags and stats). Every other upgrade module, and a set of any other condition, is dropped and counted by cause.
- [ ] `09-zero-hour.md`'s losses "Weapon sets and bonuses" and "The panel" lose what the slice imports and gain "Upgrades' other effects", the modules dropped.
- Checks: the importer's tests convert a hand-written upgrade, its buttons and its four modules on two types; the Zero Hour check counts the upgrades, research buttons and modules imported and dropped, and its golden is written again with `CAMPFIRE_BLESS=1`.

### U5. The panel and the check (client, the Zero Hour check)

- [ ] The command panel shows a unit's trains and builds (`apps/client/src/command_panel/mod.rs`) → it shows a research as an action with its cost and time, and one its player or unit holds as done (upgrades.md, The panel shows a research as an action).
- [ ] The Zero Hour check's skirmish play researches a player upgrade of each effect the slice imports and an object upgrade, and pins its units' armor, weapons, damage bonus and life after them.
- [ ] The slice's decisions move into `production.md`, `stats.md`, `combat.md`, `actions.md` and `09-zero-hour.md`, and `upgrades.md` is deleted.
- Checks: the slice's, in [Upgrades](docs/design/upgrades.md#checks); the slice ends when a player researches Composite Armor and Uranium Shells in the client.

## Foundations
