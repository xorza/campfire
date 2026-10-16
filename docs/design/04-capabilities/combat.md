# Combat

## Mechanism

- **Life, weapons and one damage pass.** `combat` gives units a life pool and weapons, which are actions of the `attack` kind ([Actions](actions.md)), and runs damage, heals, deaths and kill credit; who may hit whom comes from the teams' relations.
- **A type can die exactly when it has life.** A unit type has `combat` exactly when it has the life pool, so every unit that can reach zero life can die.

## Weapons

- **The first weapon that selects the target.** An attack names its target by stable id and uses the first weapon whose filter selects it; an order no weapon selects is ignored.
- **A weapon may give its own damage and period.** An attack takes `amount` in place of its attacker's damage stat, and `period_ms` in place of its rate stat, one of each pair, as StarCraft II holds a weapon's damage and period on the weapon; a MOBA keeps its stats, which items and levels raise.
- **Clips.** A weapon may fire in clips: after the shot that empties its clip it is ready its reload after that shot, in place of its period, as Zero Hour's `Weapon::reloadWithBonus` sets the next shot; a stop, a move or a new target keeps the count.
- **Range counts only at the start.** A delivery happens unless its target died, despawned or became no target, so an attack that starts in range lands.
- **One attack hits one unit.** Its roll, leech and on-hit effects count once; a splash is an area its `on_hit` launches, as StarCraft II's Siege Tank splashes by a search effect beside its damage.
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

## Events

- **A unit's modifiers hear its combat events.** An attack, an interval, a hit, damage taken and a kill each run a hook on the modifiers of the unit concerned, in a fixed order, in the pool of the modifier source's player.
- **A depth bound ends the pass.** A hook an event of depth `k` causes runs at `k + 1`, and one at depth 16 fails, so a chain of reactions ends within the tick; `ctx.attack_hit` marks its damage so a hook can refuse to answer it, as Dota 2 marks reflected damage.

## Network

- **Damage only as the server confirms it.** A client predicts the start of its own attacks, never their damage or a death ([Replication audiences](../14-replication-audiences.md)).
