# Combat and stats

`combat` gives units a side, health and attacks, and runs damage and deaths. `stats` gives them numbers that modifiers change. Almost every other capability builds on them.

## Teams

A unit has a team, a type of the core that every capability shares: its index in the mode's list of teams, which may be of any length. Units of different teams are enemies and there is no other rule, so a neutral team for camps and objectives is one more team, an enemy of every other. Which teams a mode has, and which slots play on each, are in its manifest.

## Attacks

- **Attack order.** An attack names its target by stable id; an order on a unit that is not a living enemy is ignored.
- **Range** is measured on the ground plane, exactly, with no square root, from the edge of the attacker's body to the edge of the target's, as in League of Legends and Dota 2: a range `r` reaches a center `r` plus both radii away. A unit with no body is a point. A unit that can move walks to its target while out of range and stops in range.
- **Windup and period.** An attack starts once the unit is ready and strikes when its windup ends. The next attack may start one period after this one started. A move, or an attack on another target, cancels a windup and spends nothing, so the unit may attack again at once; after the strike, in the back-swing, moving is free. Range counts only at the start: a strike lands unless its target died or despawned.
- **Ranged attacks** fire a homing projectile at the end of the windup, from where the attacker stands, which strikes on arrival ([Abilities](abilities.md#projectiles-and-areas)); in a match without `projectiles`, they strike at the end of the windup.

## Damage and death

- **Damage** has an amount and a kind from the mode's list (for the reference MOBA: `physical`, `magic`, `true`; for a shooter: bullet, explosive; for an MMO: its schools). The mode's `calc_damage` hook turns it into the final amount (armor, resistances, headshots); until scripts run, an attack deals its damage as is.
- **Strikes of one tick apply together** in the Resolve stage, in the order of their source's stable id, so each follows from the state before any of them: two units can kill each other in one tick.
- **Recent attackers.** Each strike records its source with its target, and the tick it landed in. A unit keeps each attacker once, with its last strike, and forgets one that no longer exists; `unit.recent_attackers(ms)` reads them.
- **Death.** A unit at zero health dies at the end of Resolve. A unit type says whether it stays dead to respawn, as heroes do, or despawns, as creeps do; one that despawns goes at the end of the tick it died in, after the Mode stage saw it. A dead unit takes no orders and is no target.
- **Kill credit.** The source whose damage took the health to zero is the killer, when it still exists: a projectile can outlive the unit that launched it, and then no one is. A source that no longer exists is not recorded as an attacker either; the other units that damaged the victim within the mode's `assist_window_ms` assisted, by stable id, and without a window no one assisted. The mode receives both in `on_unit_died`, in the Mode stage of the tick, in the order the units died.
- **Respawn.** Each unit keeps the place it spawned at. `ctx.respawn(unit, ms)` brings a dead unit that stays back at the start of the tick that time later, rounded up and at least one tick after the end of the current one: at its spawn place, with full health and no attacker on record.

## Stats and modifiers

The mode declares its stats; the core knows only those a capability reads, such as move speed. How stats combine and their limits, modifiers (duration, stacks, reapply rules, auras, shields) and states (`stunned`, `rooted`, `silenced`, `disarmed`, `airborne`, `stealthed`, `untargetable`, `slow_immune`) are defined once for every mode: [Script API](../08-script-api.md#data-files). A passive is a modifier a unit always carries; the carrier's events (attack, hit, damage, kill, takedown) go to modifier scripts.

## Sent to clients

Health, visible modifiers and the current animation go to everyone who sees the unit; resources such as mana go to the owner or the team, as the mode sets. Damage and deaths appear on a client only when the server confirms them.
