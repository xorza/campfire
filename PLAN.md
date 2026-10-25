# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Upgrades

The slice of [Upgrades](docs/design/upgrades.md): a player in a Zero Hour skirmish researches player and object upgrades from the panel, and sees them change units' armor, weapon sets, weapon damage and life. One batch left: the panel, the check and the fold. Its checks are that document's.

### U5. The panel and the check (client, the Zero Hour check)

- [ ] The command panel shows a unit's trains and builds (`apps/client/src/command_panel/mod.rs`) → it shows a research as an action with its cost and time, and one its player or unit holds as done (upgrades.md, The panel shows a research as an action).
- [ ] `09-zero-hour.md`'s loss "The panel" loses the upgrades it shows.
- [ ] The Zero Hour check's skirmish play researches a player upgrade of each effect the slice imports and an object upgrade, and pins its units' armor, weapons, damage bonus and life after them.
- [ ] The slice's decisions move into `production.md`, `stats.md`, `combat.md`, `actions.md` and `09-zero-hour.md`, and `upgrades.md` is deleted.
- Checks: the slice's, in [Upgrades](docs/design/upgrades.md#checks); the slice ends when a player researches Composite Armor and Uranium Shells in the client.

## Foundations
