# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Upgrades

The slice of [Upgrades](docs/design/upgrades.md): a player in a Zero Hour skirmish researches player and object upgrades from the panel, and sees them change units' armor, weapon sets, weapon damage and life. Four batches left: the types' upgrades, then armor and actions by tags, each with its scripted match; then the import; then the panel, the check and the fold. Its checks are that document's.

### U2. A type's upgrades (stats)

- [ ] A unit takes no modifier by what its player or it holds but by a player modifier's filter (stats.md, Player modifiers are upgrades) → a unit type's `upgrades` lists entries of `triggers`, `all`, `conflicts` and `modifier`, and a unit holds each entry's modifier while its player's modifiers and its own meet it, under a new `Hold::Upgrade` (`stats/src/lifetime.rs:31`); the test runs as a unit spawns, as its site completes, and as it or its player gains or loses a modifier, and visits only the units of the types that name that modifier, by an index of them built at the load (upgrades.md, A unit type takes modifiers by upgrades). Builds: a pass visits what changed; a name is resolved once.
- Checks: the scripted match: an entry holds at a spawn after its research, at a research after its spawn, and as a site completes, not before; an entry of any trigger, of `all` and of a conflict holds exactly as its triggers say; a player modifier taken away releases the entries it met; a unit of a type that names no trigger is never visited, by a count of the units the pass visits.

### U3. Armor and actions by tags (combat, actions)

- [ ] A unit type names one armor (`combat/src/combat_data.rs:14`) → it may also name `armors`, entries of a tag test and an armor, the first that passes replacing `armor`, derived as the unit's tags change (upgrades.md, Armor by tags).
- [ ] Every action of a unit's slots may start whatever the unit's tags, but for those its tags block → an action's `when`, a tag test in the filter's syntax, refuses its start and keeps it out of each weapon choice and scan while it fails, its slot kept (upgrades.md, An action may need tags).
- Checks: the scripted match: a unit given the tag its `armors` test takes the second armor's share of a damage, computed by hand, and the first's once it loses it; a unit of a weapon of `when` and one of `!` the same tag fires the first while it holds the tag and the second otherwise, by the damage each deals; its slots stay as they were.

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
