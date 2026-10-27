# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

## Ready

- **Bots.** A bot orders its units to attack a target none of their weapons selects, as a helicopter in flight is to a tank: the Zero Hour check's bases' bots drop 166 such orders as `no_weapon`.

- **HUD.** Every mode draws the MOBA HUD's hit mark, a flat yellow ring as wide as a unit's reach at its drawing for 350 ms each time its life drops, which Zero Hour does not draw, and a flier's ring stands in the air at its height (02-engine-core.md, The mode chooses its hit mark, planned).

- **Import.** A Zero Hour particle system's `ColorScale` is written to the effects file as the INI gives it, in 255ths of a channel a frame, where the file's `color_scale` is in whole channels a step, so 108 of 559 systems, as the Stinger and Comanche missiles' `BuggyMissileExplosion`, go dark or white after their first step.
