# Zero Hour continuous fire

A proposal for one slice. When it ends, each lasting decision moves into the document of the system it governs, and this document is deleted ([A slice's proposal is temporary](../../AGENTS.md#documents)); its plan is in `PLAN.md`. Its facts are in [Zero Hour continuous fire](../research/zero-hour-continuous-fire.md).

**At the slice's end** a China player's Gattling Cannon, ordered nothing, fires at an enemy that comes within its reach, each shot sooner than the one before as it keeps firing, as the original's does; the Gattling Tank and the minigunner fire the same way.

## Decisions

### Combat

- **A weapon's own period may speed up by a stat.** An `attack` of `period_ms` takes `rate_bonus = "<stat>"`: its period and its clip's reload, in ticks, are divided by 1 plus the stat's value, rounded down, and stay longer than its windup, as Zero Hour's `getDelayBetweenShots` divides its frames by the summed `RATE_OF_FIRE` and floors, and OpenRA's `ReloadDelayMultiplier` truncates once. A bonus of base 0, so a type that holds no value of it fires at its weapon's own period, and modifiers add to it as Zero Hour's bonuses add. Refused: a multiplier of base 1, which every type of the weapon would have to hold, or fire at no rate; a `rate_bonus` beside the stat `rate`, which the stat already speeds.
- **A shot streak gives a stage's modifier.** An `attack` takes `streak = { coast_ms, stages = [{ shots, modifier }] }`, the stages' shots growing: a unit's shot of it counts one more when it hits the target of the shot before, or another before the coast ends, and starts again at one otherwise; at each shot the stage rises by one when the count passes the next stage's shots, and falls to none when the count is below its own stage's; and in the first tick past the coast with no shot, the stage falls to none and the count stays. The unit holds the stage's modifier from itself while the stage lasts. Practice: Zero Hour's `FiringTracker`, and OpenRA's `GrantConditionOnAttack` with a `ReloadDelayMultiplier`. Refused: a weapon's own continuous-fire steps, a second way to change a weapon's rate beside the stats; OpenRA's reset of the count at each fall, which Zero Hour does not do, an option when a game needs it.
- **The streak is the unit's, the stages the weapon's.** One streak a unit, as Zero Hour's tracker is the object's; a shot of a weapon of no streak leaves it as it is. The coast ends `coast_ms` after the tick the shot's weapon is ready again, as the original counts it from the next possible shot.
- **A stage's modifier by a hold of its own.** The streak's grant holds the modifier as an item's holds its own, from the unit itself, under a hold no passive and no item shares, so neither releases the other's. Refused: a timed application refreshed at each shot, which needs its own removal at each fall.
- **The streak is the server's.** Only a role that computes state counts; a predicting client derives its stats from the modifiers the server sends, so its own attacks' period follows a stage a tick late and corrects. Refused: a predicted streak, whose stage would need the server's targets as they are, not as the client holds them.

### The import

- **Continuous fire converts.** A weapon of `ContinuousFireOne` and `ContinuousFireTwo` and a `RATE_OF_FIRE` bonus of `CONTINUOUS_FIRE_MEAN` and `CONTINUOUS_FIRE_FAST` gets `rate_bonus = "rate_of_fire"` and a streak of its coast and two stages, above `One` and above `Two` shots, each of a modifier the import writes into the mode, which adds the bonus less 1 to `rate_of_fire`; the rules package declares the stat. One whose continuous-fire bonus changes another field is dropped as today.
- **Only a weapon of a rate bonus reads the stat.** Each unit holds one `rate_of_fire`, where Zero Hour gives each weapon its own bonus of the object's stage; every imported unit with a streak weapon has no other weapon that reads the stat. Refused: the stat on every weapon, which would speed a unit's other weapons with its streak.

### Losses

What 09 lists once the slice ends:

- **Base defenses.** The Patriot's and the Stinger site's fire missiles, and the Fire Base's keeps a minimum range, so none of them fires; the Gattling Cannon fires.
- **Dropped weapons.** The cause "fire faster as they keep firing" goes.
