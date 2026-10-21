# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Slice: Zero Hour continuous fire

A China player's Gattling Cannon, ordered nothing, fires at an enemy within its reach, each shot sooner as it keeps firing ([proposal](docs/design/12-zero-hour-continuous-fire.md)). Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. The rate bonus and the shot streak (L)

`combat`, `stats`, the capabilities' tests. First: generic, and the import writes what it reads.

- **The rate bonus.** `AttackData` takes `rate_bonus`, a stat, only beside `period_ms`, bound at the load as `rate` is; `Weapon::period` and the clip's reload divide their ticks by 1 plus the stat's value, rounded down, the divisor at least a bit, and stay longer than the windup. Builds: a name is resolved once, at the load.
- **The streak's data.** `AttackData` takes `streak = { coast_ms, stages = [{ shots, modifier }] }`, read and checked at the load: one stage at least, the stages' shots strictly growing, the coast a time the rate counts, each modifier one of the packages', which the weapon applies with no action in `ModifierAppliers`; each refusal an error of its own. Builds: what a module adds is declared once.
- **The streak.** A part of each unit of a type that holds a streak weapon, its count, its last target, its stage and its coast's end; `Attacks::strike`, on a role that computes state, counts each shot of a streak weapon and sets the stage by the rules of the proposal; a timed process clears the stage in the first tick past the coast, before attacks strike; a want system, before the grant pass of Resolve, pushes the grants of the units whose streak changed, under a new `Hold` of stats'. Builds: a pass visits what changed; Bevy's own mechanisms.
- **Check:** a test of a weapon of 8 ticks and stages above 1 and 5 shots, +1 and +2: hits in ticks 0, 8, 16, 20, 24, 28, 32 and 34, as 8 ÷ 2 = 4 and ⌊8 ÷ 3⌋ = 2, the modifier held from tick 8 and swapped in tick 28; a switch of target within the coast that keeps counting, and one past it that starts at one; the stage's fall in the first tick past the coast, the count kept, and a rise of one stage at each of the next two shots; a weapon of no streak that leaves it; a client's match that counts nothing; a clip whose reload the bonus divides; each load refusal. `StateRegistry::DATA_VERSION` rises for the new part; the test modes hold no streak weapon, so no golden moves.

### 2. Zero Hour's continuous fire (M)

`import`, the rules package, 09. After batch 1, whose data it writes.

- **The weapon's fields.** The weapon INI reads `ContinuousFireCoast` and each `WeaponBonus` line, its condition, its field and its percent, as `parseWeaponBonusSet` sets one field of one condition; an unknown name fails the read, as `scanIndexList` does.
- **The conversion.** A weapon of continuous fire and `RATE_OF_FIRE` bonuses of both stages converts as the proposal says, its two modifiers named after the weapon and its stages and written into the mode's `[modifiers]`; one whose continuous-fire bonus changes another field keeps the drop. The rules package declares `rate_of_fire`.
- **Check:** fixture tests of the Gattling Cannon's weapon, its stages and its modifiers' values, 100 % and 200 % as +1 and +2, and of a weapon whose stage bonus changes its damage, dropped; the drop lists of the import tests lose `GattlingBuildingGun`, `GattlingTankGun` and `Infa_MiniGunnerGun`; `cargo drive run zero-hour`, on request, moves its fingerprints and its counts of imported weapons.

### 3. The Gattling Cannon in the check (M)

`zero-hour-check`, its golden, 09, 04-capabilities/combat.md. Last: it plays what the two batches before it give, and ends the slice.

- **The defense play.** In place of the ranger, China on Tournament Desert builds a power plant and a Gattling Cannon, and the bot's America dozer drives within the cannon's 225 m; the play asserts the ticks between the cannon's first eight hits, 8, 8, 4, 4, 4, 4 and 2, as `GattlingBuildingGun`'s 8 frames, its stages above 1 and 5 shots and its 200 % and 300 % give them, and pins the tick of its first hit and of the dozer's death, 250 hits of 1 damage each against `DozerArmor`.
- **The fold.** The combat decisions go to combat.md, in place of the planned shot streak, the import's and the losses to 09; the proposal is deleted.
- **Check:** `cargo drive run zero-hour` passes with its golden blessed, each moved value named by its cause; the user plays a skirmish in the client and watches a Gattling Cannon spin up on an enemy.

## Foundations
