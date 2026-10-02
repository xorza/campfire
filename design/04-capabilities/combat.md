# Combat

## Mechanism

`combat` gives units a life pool and weapons, and runs damage, heals, deaths and kill credit. A weapon is an action of the `attack` kind ([Actions](actions.md)); damage and heals are effects; who may hit whom comes from the relations of their teams ([Relations](00-overview.md#relations)). Almost every other capability builds on it.

## Data

The mode, in `data/mode.toml`:

```toml
[combat]
damage_kinds = ["physical", "magic", "true"]
assist_window_ms = 10000
life = "health"                                        # the pool that is life
leech = { attack = "life_steal", other = "spell_vamp" } # optional: stats that heal the source
heal_scale = "healing_received_pct"                    # optional: heals scale by one plus this stat
```

A unit type: `pools` that hold the life pool, `combat = { on_death = "stay" }` (`stay` or `despawn`, the default), and its weapons in its action slots. A unit type with the life pool needs its `combat` section, so that every unit that can reach zero life can die; the load refuses one without it. A weapon is an action of kind `attack`:

```toml
[actions.tank_cannon]
kind = "attack"
targeting = "enemies:!air"
range = "9.0"
windup_ms = 400
rate = "attack_speed"      # the stat of attacks a second
damage = "attack_damage"   # the stat of its damage
damage_kind = "physical"
delivery = { projectile = "cannon_shell" }   # a unit type with a homing `projectile` section
```

## Rules

### Weapons

- **Attack order.** An attack names its target by stable id. The unit attacks with the first of its weapons whose filter selects the target; an order on a unit no weapon selects is ignored.
- **Range** is measured in the map's metric, exactly, with no square root, from the edge of the attacker's body to the edge of the target's, as in League of Legends and Dota 2: a range `r` reaches a center `r` plus both radii away. A unit with no body is a point. A unit that can move walks to its target while out of range and stops in range.
- **Windup and period.** An attack starts once the weapon is ready and delivers when its windup ends. The next attack may start one period after this one started. A move, or an attack on another target, cancels a windup and spends nothing, so the unit may attack again at once; after the delivery, in the back-swing, moving is free. Range counts only at the start: a delivery happens unless its target died, despawned or became blocked as a target.
- **Delivery.** At once to the target, a projectile, a ray, a sweep or an area, as the weapon's `delivery` says ([Deliveries](actions.md#deliveries)); a delivery of a capability the mode does not declare fails the load, as any section of one does.
- **Its damage** is the weapon's `damage` stat of the attacker, of the weapon's `damage_kind`, followed by the weapon's `on_hit` effects.
- **The roll.** An attack draws one random number, at least 0 and less than 1, `d.roll`, when its windup ends, on the secret stream for the attacker in that tick: a projectile carries it. The seed alone decides it, and no client learns it before the server applies it. `calc_damage` decides what it means: the reference MOBA crits when it is below the attacker's `crit_chance`.

### Damage and heals

- **Damage** has a source, or none for an effect the mode applied, a target, an amount, a kind from the mode's list, its hit (`d.hit`: how its delivery reached the target, [Deliveries](actions.md#deliveries)), whether an attack dealt it, its roll (`()` when no windup drew one, as for an effect or `ctx.attack_hit`), and the action whose resolve, delivery or modifier dealt it.
- **One pass a tick.** Every damage and heal of a tick applies in Resolve, in one queue: first the tick's effects, those with no source first, then by their source's stable id, and then in the order they were queued, then those the events of the pass queue, first in, first out. Every effect of the pass reads the units as the pass began, so two units can kill each other in one tick, and a modifier an event adds takes effect from the next stage.
- **Each damage, in order:** nothing happens to a target at zero life or whose tags block `damage` ([Tags](stats.md#tags)); the mode's `calc_damage(ctx, d)` turns the amount into the final one, `d.amount` the raw amount, and a negative result counts as 0; with no `calc_damage`, or a call that fails, the amount stays as it was, and the failure is recorded. `calc_damage` counts against no script pool, only the limit per call: its calls grow with the damage of a tick, and a crowded fight must not spend the mode's pool and so change how damage is weighed. Shields absorb it next, the one that ends soonest first, as League of Legends spends them, a shield with no end last, and shields with the same end in the order the carrier keeps its modifiers; what is left comes off the life pool. The source, when it exists, is recorded as the target's attacker.
- **Each heal** passes the mode's `calc_heal(ctx, h)` the same way, then it is multiplied by one plus the `heal_scale` stat of the healed unit, and it adds to the life pool. A restore adds to another pool, unscaled. Neither reaches a unit at zero life.
- **Leech.** With `leech`, the source heals by its `attack` stat times the life an attack took, or its `other` stat times the life any other damage took: after mitigation, and not what shields absorbed, as League of Legends' life steal counts it. The heal joins the end of the pass's queue, and passes `calc_heal` and the scale.
- **A source that is gone.** A damage whose source no longer exists, as from a projectile whose source despawned, has `d.source` of `()`, and no killer.
- **Recent attackers.** Each damage records its source with its target, and the tick it landed in. A unit keeps each attacker once, with its last damage, and forgets one that no longer exists; `unit.recent_attackers(ms)` reads them.

### Death and respawn

- **Death.** A unit at zero life dies at the end of Resolve. A unit type says whether it stays dead to respawn, as heroes do, or despawns, as creeps do; one that despawns goes at the end of the tick the Mode stage answered its death in: that tick, unless the mode's pool was spent and its `on_unit_died` waits for a later tick. A dead unit takes no orders, starts no action and is no target; its death stops its actions under way, ordered or started, as a stop order does.
- **Kill credit.** The source whose damage took the life to zero is the killer, when it still exists. The other units that damaged the victim within the mode's `[combat] assist_window_ms` assisted, by stable id; without a window no one assisted. The mode receives both in `on_unit_died`, in the Mode stage of the tick, in the order the units died.
- **Respawn.** Each unit keeps the place it spawned at. `ctx.respawn(unit, ms)` brings a dead unit that stays back at the start of the tick that time later, rounded up and at least one tick after the end of the current one: at its spawn place, with full pools and no attacker on record.

### Events

A unit's modifiers hear its combat events, each modifier by its script's hook, in the order the modifiers are kept: by id, then source. A hook runs in the script pool of the modifier source's player; in the `think` pool when no player controls the source, or it is gone; and in the mode's when the modifier has no source.

| Event | When | Hook, on whose modifiers |
| --- | --- | --- |
| An attack goes off | Hit, as its windup ends, before it delivers | `on_attack(ctx, m, target)`, the attacker's |
| A modifier's interval | Hit, after the attacks, by carrier's stable id, then modifier | `on_interval(ctx, m)`, its own |
| An attack hits | Resolve, after its damage | `on_attack_hit(ctx, m, d)`, the attacker's |
| Damage is taken | Resolve, after it | `on_damage_taken(ctx, m, d)`, the target's |
| A kill | Resolve, after the damage that killed | `on_kill(ctx, m, victim)`, the killer's; then `on_takedown(ctx, m, victim)`, the killer's and each assister's by stable id |

After one damage of the pass, its events run in this order: `on_attack_hit`, then `on_damage_taken`, then `on_kill` and `on_takedown`. They run for every damage that reached a unit above zero life, all absorbed by shields or not. `d.amount` in a hook is the amount after `calc_damage`, before shields. A hook's effects apply in the order it queued them, when it returns: a heal or a modifier at once, and the damage joins the end of the pass.

- **A hook's `ctx`** is the one every script gets ([One source](../08-script-api.md#one-source)). Its acting unit is the modifier's source; `ctx.p` reads the modifier's params, then those of its action, at the instance's rank; damage it deals names the modifier's action. Writes to `m` apply when the hook returns, as a handle's do.
- **Intervals.** A modifier with `interval_ms` runs its `on_interval` effects and hook that many ticks after it was applied, rounded up and at least one, and again each interval while it holds; a refresh keeps the count.
- **Depth.** An attack, an action and the tick's other damage are at depth 0, `on_attack` and `on_interval` at depth 1, and a hook an event of a damage at depth `k` causes runs at `k + 1`; the damage it deals is at that depth. `ctx.attack_hit(target)` deals the acting unit's first weapon's damage to `target` as an attack: `d.attack` and `d.extra` set, and `on_attack_hit` follows, but not `on_attack`; a hook that should not answer it reads `d.extra`, as Dota 2 marks reflected damage so a reflection never reflects. A hook at depth 16 fails with a script error and does not run: no designed chain is that deep, and the pass must end within the tick. A unit with no weapon fails `ctx.attack_hit`.

## State and derived

- **State:** each unit's life pool, its attack target, its recent attackers, whether it is dead and when it respawns, its spawn place; the tick's deaths. A weapon's windup and the tick it is ready are its unit's slots' ([Actions](actions.md#state-and-derived)).
- **Derived:** each weapon's period and damage, from the stats.

## Script API

`ctx.damage(target, amount, kind)`, `ctx.heal(unit, amount)`, `ctx.restore(unit, pool, amount)`, `ctx.attack_hit(target)`, `ctx.respawn(unit, ms)`; `unit.target`, `unit.attack_range` (its first weapon's), `unit.recent_attackers(ms)`; the damage handle `d`; the heal handle `h`, with its source, target, amount before `calc_heal`, whether leech gave it, and its ability; the hooks above, `calc_damage(ctx, d)`, `calc_heal(ctx, h)` and `on_unit_died(ctx, unit, killer, assisters)`.

## Network

The life pool, visible modifiers and the current animation go to everyone who sees the unit; other pools go to the owner or the team, as the mode sets. Damage and deaths appear on a client only when the server confirms them. A client predicts the start of its own attacks, not their damage.

## Cost

The pass costs one `calc_damage` call for each damage and one `calc_heal` call for each heal, each under the limit per call, and the events' hooks under their pools. Each weapon that has a target costs a range test a tick.

## Genres

Every target game deals damage through one pass: a MOBA's attacks and abilities, a shooter's rays and grenades with headshots from `d.hit`, an RTS's weapons against ground and air, an MMO's auto attack and spells, and a battle royale's zone through a modifier.
