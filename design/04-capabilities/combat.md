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

- **Damage** has a source, or none for a modifier the mode applied, a target, an amount and a kind from the mode's list (for the reference MOBA: `physical`, `magic`, `true`). A mode with the combat capability declares at least one kind, and its `attack_kind`, one of them. An attack deals the `attack_kind`; an ability or modifier script names its kind. It is an attack when an attack or `ctx.attack_hit` dealt it, and it names the ability whose cast, projectile, area or modifier dealt it.
- **One pass a tick.** Every damage of a tick applies in Resolve, in one queue: first the tick's damage, the damage with no source first, then by its source's stable id, and then in the order it was queued, then the damage the events of the pass queue, first in, first out. Every damage of the pass reads the units as the pass began, so two units can kill each other in one tick, and a modifier an event adds takes effect from the next stage.
- **Each damage, in order:** nothing happens to a target at zero health; the mode's `calc_damage(ctx, d)` turns the amount into the final one, `d.amount` the raw amount and `d.crit` set, and a negative result counts as 0; with no `calc_damage`, or a call that fails, the amount stays as it was, and the failure is recorded. `calc_damage` counts against no script pool, only the limit per call: its calls grow with the damage of a tick, and a crowded fight must not spend the mode's pool and so change how damage is weighed. Shields absorb it next, the one that ends soonest first, as League of Legends spends them, a shield with no end last, and shields with the same end in the order the carrier keeps its modifiers; what is left comes off health. The source, when it exists, is recorded as the target's attacker.
- **Life steal and spell vamp.** The source heals by `life_steal` times the health an attack took, or `spell_vamp` times the health an ability or modifier's damage took: post-mitigation, and not what shields absorbed, as League of Legends' life steal counts it.
- **Heals.** `ctx.heal(unit, amount)` adds to health, `ctx.restore(unit, amount)` to the resource; a heal, life steal included, is scaled by the healed unit's `healing_received_pct`, a restore is not. Neither reaches a unit at zero health. A heal applies when the effects of its call apply: a cast's in Hit, before the damage of that tick, and an event's in its place in the pass.
- **Crits.** An attack rolls its crit once, when its windup ends, with the attacker's `crit_chance`, on the secret stream for the attacker in that tick: a ranged attack's projectile carries it. Each roll is independent, as design 07 sets, so the seed alone decides it, and no client learns it before the server applies it. `calc_damage` sees it as `d.crit` and decides what it does: the reference MOBA doubles the amount. `ctx.attack_hit` rolls none.
- **A source that is gone.** A damage whose source no longer exists, as from a projectile whose source despawned, has `d.source` of `()`, and no killer.
- **Recent attackers.** Each damage records its source with its target, and the tick it landed in. A unit keeps each attacker once, with its last damage, and forgets one that no longer exists; `unit.recent_attackers(ms)` reads them.
- **Death.** A unit at zero health dies at the end of Resolve. A unit type says whether it stays dead to respawn, as heroes do, or despawns, as creeps do; one that despawns goes at the end of the tick it died in, after the Mode stage saw it. A dead unit takes no orders and is no target.
- **Kill credit.** The source whose damage took the health to zero is the killer, when it still exists. The other units that damaged the victim within the mode's `assist_window_ms` assisted, by stable id; without a window no one assisted. The mode receives both in `on_unit_died`, in the Mode stage of the tick, in the order the units died.
- **Respawn.** Each unit keeps the place it spawned at. `ctx.respawn(unit, ms)` brings a dead unit that stays back at the start of the tick that time later, rounded up and at least one tick after the end of the current one: at its spawn place, with full health and no attacker on record.

## Combat events

A unit's modifiers hear its combat events, each modifier by its script's hook, in the order the modifiers are kept: by id, then source. A hook runs in the script pool of the modifier source's player; in the `think` pool when no player controls the source, or it is gone; and in the mode's when the modifier has no source.

| Event | When | Hook, on whose modifiers |
| --- | --- | --- |
| An attack goes off | Hit, as its windup ends, before it strikes or fires | `on_attack(ctx, m, target)`, the attacker's |
| A modifier's interval | Hit, after the attacks, by carrier's stable id, then modifier | `on_interval(ctx, m)`, its own |
| An attack hits | Resolve, after its damage | `on_attack_hit(ctx, m, d)`, the attacker's |
| Damage is taken | Resolve, after it | `on_damage_taken(ctx, m, d)`, the target's |
| A kill | Resolve, after the damage that killed | `on_kill(ctx, m, victim)`, the killer's; then `on_takedown(ctx, m, victim)`, the killer's and each assister's by stable id |

After one damage of the pass, its events run in this order: `on_attack_hit`, then `on_damage_taken`, then `on_kill` and `on_takedown`. They run for every damage that reached a unit above zero health, all absorbed by shields or not. `d.amount` in a hook is the amount after `calc_damage`, before shields. A hook's effects apply in the order it queued them, when it returns: a heal or a modifier at once, and the damage joins the end of the pass.

- **A hook's `ctx`** is the one every script gets ([One source](../08-script-api.md#one-source)). Its acting unit is the modifier's source; `ctx.p` reads the modifier's params, then those of its ability, at the instance's rank; damage it deals names the modifier's ability. Writes to `m` apply when the hook returns, as a handle's do.
- **Intervals.** A modifier with `interval_ms` calls `on_interval` that many ticks after it was applied, rounded up and at least one, and again each interval while it holds; a refresh keeps the count.
- **Depth.** An attack, a cast and the tick's other damage are at depth 0, `on_attack` and `on_interval` at depth 1, and a hook an event of a damage at depth `k` causes runs at `k + 1`; the damage it deals is at that depth. `ctx.attack_hit(target)` deals the acting unit's attack damage to `target` as an attack: `d.attack` and `d.extra` set, no crit, and `on_attack_hit` follows, but not `on_attack`; a hook that should not answer it reads `d.extra`, as Dota 2 marks reflected damage so a reflection never reflects. A hook at depth 16 fails with a script error and does not run: no designed chain is that deep, and the pass must end within the tick. `ctx.attack_hit` uses the acting unit's attack damage as its effect applies; a unit with no attack fails the call.

## Stats and modifiers

Stats, modifiers, states and levels: [Stats](stats.md). A passive is a modifier a unit always carries; the carrier's events (attack, hit, damage, kill, takedown) go to modifier scripts.

## Sent to clients

Health, visible modifiers and the current animation go to everyone who sees the unit; resources such as mana go to the owner or the team, as the mode sets. Damage and deaths appear on a client only when the server confirms them.
