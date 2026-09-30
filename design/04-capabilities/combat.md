# Combat and stats

`combat` gives units a side, health and attacks, and runs damage and deaths. `stats` gives them numbers that modifiers change. Almost every other capability builds on them.

## Teams

A unit has a team: its index in the mode's list of teams, which may be of any length. Units of different teams are enemies and there is no other rule, so a neutral team for camps and objectives is one more team, an enemy of every other. Which teams a mode has, and which slots play on each, are in its manifest.

## Attacks

- **Attack order.** An attack names its target by stable id; an order on a unit that is not a living enemy is ignored.
- **Range** is measured on the ground plane, exactly, with no square root. A unit that can move walks to its target while out of range and stops in range.
- **Windup and period.** An attack starts once the unit is ready and strikes when its windup ends. The next attack may start one period after this one started. A move, or an attack on another target, cancels a windup and spends nothing, so the unit may attack again at once; after the strike, in the back-swing, moving is free. Range counts only at the start: a strike lands unless its target died or despawned.
- **Ranged attacks** fire a projectile ([Abilities](abilities.md#projectiles-and-areas)); until projectiles exist, they strike at the end of the windup.

## Damage and death

- **Damage** has an amount and a kind: `physical`, `magic` or `true`. The mode's `calc_damage` hook turns it into the final amount (armor, resistances); until scripts run, an attack deals its damage as is.
- **Strikes of one tick apply together** in the Resolve stage, in the order of their source's stable id, so each follows from the state before any of them: two units can kill each other in one tick.
- **Death.** A unit at zero health dies at the end of Resolve. A unit type says whether it stays dead to respawn, as heroes do, or despawns, as creeps do. A dead unit takes no orders and is no target.
- **Kill credit.** The source whose damage took the health to zero is the killer; the units that damaged the victim within the mode's `assist_window_ms` assisted. The mode receives both in `on_unit_died`.

## Stats and modifiers

Stats, how they combine and their limits, modifiers (duration, stacks, reapply rules, auras, shields) and states (`stunned`, `rooted`, `silenced`, `disarmed`, `airborne`, `stealthed`, `untargetable`, `slow_immune`) are defined once for every mode: [Script API](../08-script-api.md#data-files). A passive is a modifier a unit always carries; the carrier's events (attack, hit, damage, kill, takedown) go to modifier scripts.

## Sent to clients

Health, visible modifiers and the current animation go to everyone who sees the unit; resources such as mana go to the owner or the team, as the mode sets. Damage and deaths appear on a client only when the server confirms them.
