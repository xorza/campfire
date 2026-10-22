# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

- **Combat.** A splash's outer ring deals its own fixed amount, where its inner ring deals the weapon's damage, which a stat may give, so a stat that raises a weapon's damage leaves its outer ring as it is; Zero Hour's weapon bonus scales both (`WeaponTemplate::getSecondaryDamage`).
- **Import.** Zero Hour's income besides gathering is not imported: a building's periodic cash (`AutoDepositUpdate`, as the Black Market's, an oil derrick's and America's supply drop zone's) and a hacker's internet hack.
- **Import.** The Tomahawk's and the Scud launcher's explosive weapons drop by `PreAttack`: each waits a `PreAttackDelay` before it fires.
- **Net.** On the macOS CI runner, the LAN check fails now and then: a client logs "missing confirmed tick for completed mutate tick" (run 39688090973), or an input of seat 1 stamped 20 takes effect in tick 21 (run 39659975702).

## Ready

- **Combat.** A bot's `unit.recent_attackers` and `unit.attack_target` on a unit give units its team does not see.
- **Net.** A local session of bots alone that ends as its watcher closes the client logs that every player left as it writes its log.
- **Vision.** A bot's `unit.can_see` on an enemy unit it sees tells what the enemy's team sees.
