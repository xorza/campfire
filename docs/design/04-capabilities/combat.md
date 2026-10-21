# Combat

## Mechanism

- **Life, weapons and one damage pass.** `combat` gives units a life pool and weapons, which are actions of the `attack` kind ([Actions](actions.md)), and runs damage, heals, deaths and kill credit; who may hit whom comes from the teams' relations.
- **A type can die exactly when it has life.** A unit type has `combat` exactly when it has the life pool, so every unit that can reach zero life can die.
- **What a death does is the type's.** A death reads `on_death` through its unit's type, as a hit reads its armor, and a unit dies when its life reaches zero, so no part of a unit repeats either. Refused: `on_death` copied onto each unit as state, which a snapshot and the hash carried for every unit, and a marker of the units that can die that `Life` requires, which says only that the unit has life, as the death's query reads it already.

## Weapons

- **The first weapon that selects the target.** An attack names its target by stable id and uses the first weapon whose filter selects it; an order no weapon selects is ignored.
- **A weapon may give its own damage and period.** An attack takes `amount` in place of its attacker's damage stat, and `period_ms` in place of its rate stat, one of each pair, as StarCraft II holds a weapon's damage and period on the weapon; a MOBA keeps its stats, which items and levels raise.
- **Clips.** A weapon may fire in clips: after the shot that empties its clip it is ready its reload after that shot, in place of its period, as Zero Hour's `Weapon::reloadWithBonus` sets the next shot; a stop, a move or a new target keeps the count.
- **Range counts only at the start.** A delivery happens unless its target died, despawned or became no target, so an attack that starts in range lands.
- **One attack hits one unit,** of a weapon of no splash: its roll, leech and on-hit effects count once.
- **An attack may splash.** An `attack` of a projectile takes `splash = { radius, affects, spares_source }`: where its projectile ends it deals its damage to each living unit of the relations `affects` names whose volume lies within `radius`, its source too unless it spares it, and its hit deals nothing more, as OpenRA's weapon holds a `SpreadDamageWarhead` beside its projectile and 0 A.D.'s attack a `Splash`; data of the weapon, no script, and the damage its source's, as Zero Hour's `dealDamageInternal` counts it, where an area its `on_hit` launches would need a script for every splash.
- **An attack may scatter.** An `attack` of a curve takes `scatter = { radius, tags }`: a shot at a target of a tag the table names adds the largest of their radii, and above 0 aims at a point drawn on its own stream within it from the target's ground point, as Zero Hour's `fireWeaponTemplate` scatters at infantry; the engine's random sequence is not the original's.
- **An attack may bound its pitch.** An `attack` takes `pitch_deg = [low, high]`: a target whose pitches from its attacker's center to its volume's bottom and top all miss the range is out of reach, as `Weapon::isWithinTargetPitch` meets `MinTargetPitch` and `MaxTargetPitch`; the bound lies within a quarter turn, so the test by the sine of a pitch past it tells each pitch apart, and the chase walks by range alone.
- **A unit type may have a turret.** A type's `turret = { turn_rate, tolerance_deg, slot }` turns its attacks of the slot kind toward the attack's target at its rate, and back toward 0 at half of it when idle, as `TurretAI` turns; an attack of the slot starts its windup only within the tolerance of its bearing, as `TurretAIAimTurretState` fires within 0.035 radians and OpenRA's `AttackTurreted.CanAttack` waits for its turret. The angle is a part of the unit its owner's client predicts, and the client turns the node the look names.
- **A slow weapon delays its damage.** An `attack` of no projectile takes `speed`: its hit lands ⌈distance ÷ speed a tick⌉ ticks after its shot, on the unit it was fired at, which takes nothing if it is dead by then, as Zero Hour's `WeaponStore::setDelayedDamage` holds a weapon's damage; the held hit is state of the match.
- **On-hit effects as League of Legends applies them.** A weapon's `on_hit` runs after its damage whenever the damage reaches a unit above zero life, shields absorbing all of it or not.
- **A secret roll.** An attack draws one number on the secret stream as its windup ends, and no client learns it before the server applies it; `calc_damage` gives it its meaning.

## Damage and heals

- **One pass a tick.** Every damage and heal of a tick applies in Resolve in one queue, in a fixed order of source and queue; a heal a hook or leech gives comes next, once the entry that caused it is done, as Dota 2 and League of Legends apply a reaction's heal at once. Every effect reads the units as the pass began, so two units can kill each other in one tick.
- **Damage queued after the pass is the next tick's.** It is state, which a snapshot at the boundary holds, and the next pass deals it first.
- **Armor by damage kind.** The mode declares its armors, each a share of each damage kind, and a unit type names one; the pass multiplies each damage by its share once, rounded down, before the mode's `calc_damage`, as Warcraft III's table of attack and armor types sits under its scripts.
- **`calc_damage` and `calc_heal` decide the rest.** The mode's pure hooks turn each amount into its final one; with none, or a failed call, the amount stays, and neither counts against a pool.
- **Shields that end soonest absorb first.**
- **Leech counts what life took.** Leech heals by a share of the life an attack or other damage took, not what shields absorbed, as League of Legends' life steal counts.

## Death and respawn

- **A unit lives by one test.** `stats` decides whether a unit lives, not dead and its life above zero, and every pass reads that test: no heal or regen lifts a life at zero, only a respawn.
- **A target lives.** A target is a unit that lives, has a life, and whose tags do not block `target`, so a unit at zero life that has not died yet is no target, as Zero Hour's `isEffectivelyDead` keeps such an object out of targeting; one predicate in `actions` decides it for every check.
- **Deaths at the end of Resolve.** A unit at zero life dies at the end of Resolve; one whose life reaches zero later in the tick, as `ctx.kill` in a mode's hook leaves it, dies at the next tick's.
- **Kill credit.** The source whose damage took the life to zero is the killer; the units that damaged the victim within the mode's assist window assisted.
- **A respawn is at least a tick later,** at its spawn place, its pools full.
- **What aims at a unit learns of its death and its respawn.** A death drops the held hits on its unit and makes the projectiles that home on it lose it, and a respawn tells them it was put elsewhere at once, as a teleport does, since a respawn in the next tick's inputs puts the same unit back before they move again. Refused: each reader's test of life at its own step alone, which a unit that came back passes.
- **A death records its victim,** whose tags bounty and experience filter by, so Zero Hour's rules skip a victim under construction by its tag. Refused: `Has<Site>` in `Bounty::pay`, which names one game's rule in a capability ([A capability serves every game](../../../AGENTS.md#direction)).

## Events

- **A unit's modifiers hear its combat events.** An attack, an interval, a hit, damage taken and a kill each run a hook on the modifiers of the unit concerned, in a fixed order, in the pool of the modifier source's player.
- **A depth bound ends the pass.** A hook an event of depth `k` causes runs at `k + 1`, and one at depth 16 fails, so a chain of reactions ends within the tick; `ctx.attack_hit` marks its damage so a hook can refuse to answer it, as Dota 2 marks reflected damage.
- **Combat's rules as data, planned.** Each comes with the slice of the game that needs it, its default today's behavior: damage stages before `calc_damage`, a bonus by tags and flat or ratio armor, closed and in data; a weapon's own values scaled by a stat; leech rows by cause and kind; what a splash procs, for each weapon, each target by default; a splash edge that the data names, `<` or `≤`; weapon choice criteria; several turrets; line attack projectiles; a clip's reload from its start or its shot, for each weapon, as Zero Hour counts from the shot.
- **A shot streak by modifiers, planned.** A unit's shots in a row, within a coast, apply a modifier at each stage, which a weapon's period scaled by a stat reads, as OpenRA's `GrantConditionOnAttack` and `ReloadDelayMultiplier` do; the same path carries upgrades and veterancy. Refused: a weapon's own continuous-fire steps, as Zero Hour's are, a second way to change a weapon's rate; a unit's weapons share a stage's value, where Zero Hour gives each its own (research/zero-hour-continuous-fire.md).

## Network

- **Damage only as the server confirms it.** A client predicts the start of its own attacks, never their damage or a death ([Replication audiences](../10-replication-audiences.md)).
