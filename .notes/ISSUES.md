# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

- **Combat.** A splash's outer ring deals its own fixed amount, where its inner ring deals the weapon's damage, which a stat may give, so a stat that raises a weapon's damage leaves its outer ring as it is; Zero Hour's weapon bonus scales both (`WeaponTemplate::getSecondaryDamage`).
- **Import.** The Tomahawk's and the Scud launcher's explosive weapons drop by `PreAttack`: each waits a `PreAttackDelay` before it fires.

## Ready
- **Net.** A local session of bots alone that ends as its watcher closes the client logs that every player left as it writes its log.
