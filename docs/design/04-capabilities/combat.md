# Combat

## Mechanism

`combat` gives units a life pool and weapons, and runs damage, heals, deaths and kill credit. A weapon is an action of the `attack` kind ([Actions](actions.md)); damage and heals are effects; who may hit whom comes from the teams' relations ([Relations](00-overview.md#relations)).

## Data

The mode, in `data/mode.toml`:

```toml
[combat]
damage_kinds = ["physical", "magic", "true"]
assist_window_ms = 10000
life = "health"                                        # the pool that is life
leech = { attack = "life_steal", other = "spell_vamp" } # optional: stats that heal the source
heal_scale = "healing_received_pct"                    # optional: heals scale by one plus this stat
armors = { plate = { physical = "0.5", magic = "2" } } # optional: a share of each kind's damage
```

A unit type: `pools` that hold the life pool, `combat = { on_death = "stay", armor = "plate" }` (`stay` or `despawn`, the default; an armor of the mode's, none taking every damage whole), and its weapons in its slots. A type has `combat` exactly when it has the life pool, so every unit that can reach zero life can die. A weapon:

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

A weapon may give its own damage and period, in place of the stats, `amount = "5"` for `damage` and `period_ms = 100` for `rate`, one of each pair, and a clip, `clip = { shots = 3, reload_ms = 700 }`, each one value or one per rank ([Zero Hour combat](../19-zero-hour-combat.md), D2, D3).

## Rules

### Weapons

- **Attack order.** An attack names its target by stable id. The unit attacks with the first weapon whose filter selects the target; an order on a unit no weapon selects is ignored.
- **Range** follows the reach rule, from edge to edge ([Reach](00-overview.md#space-and-map)). A unit that can move walks to its target while out of range and stops in range.
- **Windup and period.** An attack starts once the weapon is ready and delivers when its windup ends; the next may start one period after this one started, its own period or the tick rate over its rate stat, either longer than its windup. A weapon of a clip counts its shots: the shot that empties its clip makes it ready its reload after the attack's start in place of its period, its clip full again; a stop, a move or a new target keeps the count. A move, or an attack on another target, cancels a windup and spends nothing; after the delivery, moving is free. Range counts only at the start: a delivery happens unless its target died, despawned or became blocked as a target.
- **Delivery.** At once, by a homing projectile, a ray or a sweep ([Deliveries](actions.md#deliveries)); never by an area. One attack hits one unit, so its roll, leech and `on_attack_hit` count once; a splash is a `launch = { area }` in the weapon's `on_hit`, as StarCraft II's Siege Tank splashes by a search effect beside its damage.
- **Its damage** is the attacker's `damage` stat, or the weapon's own `amount`, of the weapon's `damage_kind`, followed by the weapon's `on_hit` effects, which run in the damage pass after the damage and before `on_attack_hit`, whenever the damage reaches a unit above zero life, shields absorbing all of it or not, as League of Legends applies on-hit effects: in one call, to the target, from the attacker, at the weapon's rank, with the attack's hit, at the attack's depth plus one, so its damage joins the end of the pass. `ctx.attack_hit` runs it too. A weapon takes `[params]` for its effects and the modifiers it applies, and no script, `on_resolve` or `on_end`.
- **The roll.** An attack draws one number in [0, 1), `d.roll`, when its windup ends, on the secret stream for the attacker in that tick; a projectile carries it, and no client learns it before the server applies it. `calc_damage` gives it its meaning: the MOBA test content crits below the attacker's `crit_chance`.

### Damage and heals

- **Damage** has a source, or none for an effect the mode applied; a target; an amount; a kind; its hit (`d.hit`, `()` when no delivery dealt it); whether an attack dealt it; its roll (`()` with no windup); and the action whose resolve, delivery or modifier dealt it, `()` for none.
- **One pass a tick.** Every damage and heal of a tick applies in Resolve, in one queue: the tick's effects, those with no source first, then by source's stable id, then queue order; then those the pass's events queue, first in, first out. A heal a hook or leech gives is dealt next, once the entry that caused it and its events are done, as Dota 2 and League of Legends apply a reaction's heal and life steal at once. Every effect reads the units as the pass began, so two units can kill each other in one tick, and a modifier an event adds takes effect from the next stage. Damage and heals queued after the pass, as a mode's hook queues them, are the next tick's damage: state, which a snapshot at the boundary holds, and which the next tick's pass deals first, in the order queued.
- **Each damage, in order:** nothing happens to a target at zero life or whose tags block `damage`; the armor of the target's type takes its share of the damage's kind, the amount times the share exactly, rounded down; the mode's `calc_damage(ctx, d)` turns that `d.amount` into the final amount, a negative result counting as 0, and with no `calc_damage`, or a failed call, the amount stays and the failure is recorded; it counts against no pool ([Scripting](../02-engine-core.md#scripting)). Shields absorb it next, the one that ends soonest first, one with no end last, ties in the carrier's modifier order; the rest comes off the life. The source, when it exists, is recorded as the target's attacker.
- **Each heal** passes the mode's `calc_heal(ctx, h)` the same way, is multiplied by one plus the healed unit's `heal_scale`, and adds to its life. A restore adds to a pool, unscaled. Neither reaches a unit at zero life.
- **Leech.** The source heals by its `attack` stat times the life an attack took, or its `other` stat times the life any other damage took, after mitigation and not what shields absorbed, as League of Legends' life steal counts; the heal passes `calc_heal` and the scale.
- **A source that is gone**, as a projectile's source that despawned: `d.source` is `()`, and there is no killer.
- **Recent attackers.** Each damage records its source with its target and tick, each attacker once with its last damage; `unit.recent_attackers(ms)` reads them.

### Death and respawn

- **Death.** A unit at zero life dies at the end of Resolve. A life that reaches zero later in the tick, as `ctx.kill` in a mode's hook leaves it, dies at the next tick's: until then the unit lives no more, though not yet dead. One test in `stats` decides whether a unit lives, not dead and its life above zero, which the pass, regen, the stats' refresh, the attacks and the casts read: no heal, restore, regen or rise of its maximum lifts a life at zero, only a respawn, and such a unit takes no damage, and starts and strikes no attack or cast. One that despawns goes at the end of the tick the Mode stage answered its death in, later if the mode's pool was spent. A dead unit takes no orders, starts no action and is no target; its death stops what it had under way, as a stop does. In the next Move it forgets where it walked to, and in the next Think a reset it was on ends, with no refill.
- **A target.** A living target is a unit that is not dead, has a life, and whose tags do not block `target`; a unit at zero life is one until it dies. One predicate in `actions` decides it, for every check: an action's start and delivery, and `find`, `find_visible` and `nearest_visible`.
- **Kill credit.** The source whose damage took the life to zero is the killer, when it exists. The units that damaged the victim within `assist_window_ms` assisted, by stable id; with no window, none. The mode receives both in `on_unit_died`, in Mode, in the order the units died.
- **Respawn.** `ctx.respawn(unit, ms)` brings a dead unit that stays back at the start of the tick that time later, rounded up and at least one tick after the current one: at its spawn place, its life and pools full, with no attacker on record.

### Events

A unit's modifiers hear its combat events, each by its script's hook, in the order the modifiers are kept: by id, then source. A hook runs in the pool of its modifier source's player; in `think` when no player controls the source, or it is gone; in the mode's when the modifier has no source.

| Event | When | Hook, on whose modifiers |
| --- | --- | --- |
| An attack goes off | Hit, as its windup ends, before it delivers | `on_attack(ctx, m, target)`, the attacker's |
| A modifier's interval | Hit, after the attacks, by carrier's stable id, then modifier | `on_interval(ctx, m)`, its own |
| An attack hits | Resolve, after its damage | `on_attack_hit(ctx, m, d)`, the attacker's |
| Damage is taken | Resolve, after it | `on_damage_taken(ctx, m, d)`, the target's |
| A kill | Resolve, after the damage that killed | `on_kill(ctx, m, victim)`, the killer's; then `on_takedown(ctx, m, victim)`, the killer's and each assister's by stable id |

After one damage, its events run in the table's order, for every damage that reached a unit above zero life, absorbed or not; `d.amount` in a hook is after `calc_damage`, before shields. A hook's effects apply when it returns: a modifier and a restore at once, a heal next, damage at the end of the pass.

- **A hook's `ctx`** is every script's ([One source](../08-script-api.md#one-source)). Its acting unit is the modifier's source; `ctx.p` reads the modifier's params, then its action's, at the instance's rank; its damage names the modifier's action. Writes to `m` apply when it returns.
- **Intervals.** A modifier with `interval_ms` runs its `on_interval` effects and hook that many ticks after it was applied, rounded up and at least one, and each interval after; a refresh keeps the count.
- **Depth.** Attacks, actions and the tick's other damage are at depth 0, `on_attack` and `on_interval` at 1, and a hook an event of depth `k` causes runs at `k + 1`, its damage at that depth. `ctx.attack_hit(target)` deals the acting unit's first weapon's damage as an attack, with `d.attack` and `d.extra` set, followed by `on_attack_hit` but not `on_attack`, so a hook can refuse to answer it, as Dota 2 marks reflected damage. A hook at depth 16 fails with a script error, so the pass ends within the tick. A unit with no weapon fails `ctx.attack_hit`.

## State and derived

- **State:** each unit's life, attack target, recent attackers, the shots each of its weapons with a clip fired, death and respawn tick, spawn place; the tick's deaths. A weapon's windup and ready tick are its slots' ([Actions](actions.md#state-and-derived)).
- **Derived:** each weapon's period and damage, from the stats. An attack's damage, and its projectile, name its weapon slot's rank, at which `on_hit` reads the weapon's params.

## Script API

`ctx.damage(target, amount, kind)`, `ctx.heal(unit, amount)`, `ctx.restore(unit, pool, amount)`, `ctx.attack_hit(target)`, `ctx.respawn(unit, ms)`; `unit.target`, `unit.attack_range` (its first weapon's), `unit.recent_attackers(ms)`; the damage handle `d`; the heal handle `h`, with its source, target, amount before `calc_heal`, whether leech gave it, and its ability; the hooks above, `calc_damage(ctx, d)`, `calc_heal(ctx, h)` and `on_unit_died(ctx, unit, killer, assisters)`.

## Network

A unit's `Life`, visible modifiers and animation go to everyone who sees it; its `Pools` to the owner, or the team, as the mode sets ([Replication audiences](../14-replication-audiences.md), `life` and `pools`). Damage and deaths appear on a client only as the server confirms them; a client predicts the start of its own attacks, not their damage. What becomes of a unit at its death goes to its owner, of the kind `prediction`; its recent attackers stay the server's, and only the server despawns the dead and forgets a respawned unit's attackers ([Replication audiences](../14-replication-audiences.md), D12).

## Cost

One `calc_damage` call for each damage and one `calc_heal` for each heal, under the limit per call, and the events' hooks under their pools; a range test a tick for each weapon with a target.

## Genres

Every genre deals damage through one pass: a MOBA's attacks and abilities, a shooter's rays and grenades with headshots from `d.hit`, an RTS's weapons against ground and air, an MMO's auto attack and spells, a battle royale's zone through a modifier.
