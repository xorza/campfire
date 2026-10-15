# Stats

## Mechanism

How a unit's numbers, pools and tags come from its type, its level and its modifiers; how it gains levels is [Progression](progression.md)'s. The model follows Unreal's Gameplay Ability System where the field agrees: a base value from type and level, a current value modifiers change through one formula, stacks counted by source, tags that block actions and grant immunity. Movement and pools follow League of Legends, where its rules are exact.

## Data

The mode declares, in `data/mode.toml`, its stats, pools and tag properties:

```toml
[stats.armor]
min = "-100"          # optional, both
max = "500"

[pools.health]
max = "health"         # the stat of its maximum
regen = "health_regen" # optional: the stat of its regen

[tags.stunned]
blocks = ["move", "attack", "cast", "use"]
```

A unit type gives `stats` (`stat = { base, per_level }`), its `pools` and its `tags`. A modifier, `[modifiers.<id>]`, gives `stats` (per stack), `tags` (granted to its carrier), `script`, `duration_ms`, `interval_ms`, `stacks_expire_ms`, `reapply`, `max_stacks`, `shield`, `aura`, `affects` (the units a player modifier holds on, all when absent), `on_interval` (effects), `[params]` and `[state]` ([Data files](../08-script-api.md#data-files)).

## Rules

### Stats

A unit's value of a stat is its type's at its level, `base + per_level × (level − 1)`, changed by its modifiers, each by one operation times its stacks:

```
value = (base + Σ add) × (1 + Σ pct) × (1 − max cut)
```

then clamped to the stat's limits. `add` and `pct` sum, so modifier order never matters; only the largest `cut` counts, as only the strongest slow does in League of Legends, and a cut counts at most 1. Each product rounds once, to nearest; a sum past a number's range stops at its end. A modifier writes a number for `add` and a table for the others: `stats = { armor = 20, move_speed = { cut = "0.3" }, attack_speed = { pct = "0.25" } }`.

The engine reads only the stats its mechanisms need; one the mode does not declare keeps the value its unit's type gave it as it spawned:

| Stat | Engine use |
| --- | --- |
| `move_speed` | Meters a second, within its limits and `max_move_speed`; a step a tick, rounded once |
| `turn_rate` | Degrees a second a walker turns toward its way, at least 0; a turn a tick, rounded once. A walker of none turns at once ([Units that drive](../17-units-that-drive.md#decisions), D2) |
| A pool's `max` and `regen` | Its maximum, and what it gains a second: regen ÷ tick rate a tick, the remainder carried, so a second gains exactly the regen |
| A weapon's `rate` and `damage` | Attacks a second and each one's damage ([Combat](combat.md#weapons)); the period is the tick rate over the rate, rounded up, at least a tick longer than the windup |

Combat binds the stats its rules read by the mode's names in `[combat]`.

### Pools

A mode declares at most eight pools, so a unit's pools are fixed arrays a rollback copies without allocating: `Life`, the life pool `[combat] life` names, and `Pools`, the others by pool id ([Replication audiences](../14-replication-audiences.md), D3). A pool is an amount from 0 to its `max` stat, which `regen` refills while its unit lives. When a maximum rises, the amount rises by as much; when it falls, the amount stays unless it is above the new maximum, League of Legends' rule, where Dota 2 keeps the fraction, which needs a rounding the engine does not take. A unit type's maximum of each pool is positive at level 1, or it fails to load; a modifier that later lowers it to 0 or below leaves it at the least positive number. Costs, restores and shields name pools by id.

### Modifiers

A modifier is an instance on its carrier, from a source: the unit whose action, delivery or aura applied it, or none for the mode's. A carrier holds at most one instance of an id from each source.

- **Applying.** `ctx.add_modifier(unit, id)` and `(unit, id, duration_ms)` return the instance's handle; a data effect applies it the same way. On an instance that exists, `reapply` decides: `refresh` sets its duration whole again; `stack` adds a stack, up to `max_stacks`, and refreshes; `ignore` leaves it.
- **Stacks.** With `stacks_expire_ms`, each stack keeps its own end, and the instance ends with its last. A stack past `max_stacks` replaces the one that ends soonest. A script may write `m.stacks`: it removes the stacks that end soonest, and those it adds end as stacks applied that tick do. At 0 the instance stays, with no stats and no tags.
- **Time.** A duration counts in ticks, rounded up and at least one, from the end of the tick it was applied in: one of `d` ticks applied in tick `t` holds through tick `t + d` and ends as `t + d + 1` starts.
- **Lifetime.** Holds and an application keep an instance: a passive's rank, an aura, an area and a player hold it, and an application keeps it until its duration ends, or for good with none. It ends only when nothing keeps it; every hold of one modifier from one source is one hold.
- **Ending.** `ctx.remove(handle)` ends it at once; `ctx.purge(unit, tag)` ends every instance that grants the tag; so does a spent shield, and the carrier's death, but for a passive.
- **Passives.** A modifier held while a condition holds: an action's `passive_modifier` while it has a rank, with `passive_while_ready` only off cooldown; an equipped item's; a unit type's `passive` from its spawn, at rank 1. Each source pushes what the units it visits want into one list, and one pass applies it and releases what no source wants any more: as each tick starts, before attacks strike, before units die and as the tick ends, each source visiting only the units whose source or modifiers changed. A passive comes from the carrier and keeps its stacks and state across a death. A modifier is the passive of one owner of its package at most, which the load checks. A passive whose stacks end one by one counts something, as Twin Cut counts attacks: it starts with no stacks and stays at none.
- **Auras.** A modifier with an `aura` makes its carrier a source: in Resolve each tick, every living unit within the aura's radius that `affects` selects holds its modifier from the carrier, with no duration, and loses it the tick it leaves. An area's `inside` modifiers hold the same way, by the same reach rule ([Reach](00-overview.md#space-and-map)). An instance projects its aura only while it has a stack and takes effect on its carrier. No linger: the sim has no flicker to hide.
- **Player modifiers.** `ctx.add_player_modifier(player, id)`: in Resolve each tick, each living unit of the player that `affects` selects holds it from no source at rank 1, with no duration; a unit the player gains, or that comes to match, holds it from that tick. An RTS's upgrades and a guild's bonuses are player modifiers.
- **Order.** Instances are kept by id, then source, so every walk is in one order.
- **Ids by package.** A modifier's id is its name within its package; a script names its own package's modifiers.
- **Numbers.** A modifier's numbers, duration, interval, stack expiry, shield, aura radius and `stats`, resolve as it is applied, refreshed or stacked; a new interval counts from the next, so refreshes never put an interval off. A `{ param }` reads the modifier's own params, then those of the action that applied it, at that action's rank on the source, or rank 1 with none. A later rank changes the next application, not one held, as the Gameplay Ability System captures a magnitude. One exception: a `stats` change that reads a scaling param follows its source's level and stats while held, as League of Legends' bonus-scaling passives do, and keeps its last value once its source is gone; so the stats such changes read and change form no loop across the mode's modifiers, which the load refuses, and the engine computes each stat after those it reads. A value given as a number is fixed at load, which refuses one past a number's range or a negative time. An aura's radius and a shield are never negative: the load refuses a negative value or per-rank param, and a scaling param negative as it applies gives 0.
- **Appliers.** Every way a modifier is applied gives each param it reads, so an application never fails after the call that queued it. The ways: an action's `hold`, `passive_modifier`, effect lists, its area's `inside` and its script's `ctx.add_modifier`, at each of its ranks; a unit type's `passive`, `ctx.add_player_modifier`, and the mode's or an AI's `ctx.add_modifier`, with no action at rank 1; a modifier's aura and its script's `ctx.add_modifier`, each way of that modifier. The load refuses a modifier when a way lacks a param its numbers or script read, an own per-rank param shorter than an applier's ranks, and, for a param its numbers read, a value past a number at a rank a way reads, a time that reads a scaling param, and a time negative or too long to count at the fastest rate. A param only its script reads may be any value. A computed name is checked at its call: `ctx.add_modifier` and `ctx.add_player_modifier` fail when the call's way does not give what the load requires. The load numbers each name of a param that a modifier reads from its ability, and resolves where each action holds each of them, so an application reads no param by its name.
- **Its action.** An instance keeps the action whose resolve, delivery or modifier applied it, or whose passive it is; the mode's has none.
- **Handles.** `ctx.add_modifier` returns the handle of the instance it adds or reapplies; one call that applies an instance twice gets one handle, and after `ctx.remove` a new application starts from its defaults. Writes to `stacks` and `state` apply when the call ends, after its adds, and the call reads them back. A modifier's `[state]` starts at its defaults when added and stays across refreshes and stacks.

Intervals, shields and the events modifiers hear are run by the damage pass ([Combat](combat.md#events)).

### Tags

A unit's tags are its type's; the engine's by its sections (`avatar`, `projectile`, `area`, `node`, `drop_off`, `item`, its layer's name) and by what it is now (`constructing` while a site, `gathering` while in a gather loop); and those its modifiers grant, but those of 0 stacks or that an immunity suppresses. A tag has properties only when the mode declares them in `[tags.<name>]`:

| Property | Means | Asked by |
| --- | --- | --- |
| `blocks = ["move"]` | The unit does not move | Move |
| `blocks = ["attack"]`, `["cast"]`, `["use"]` | It starts no action of that group, and one in its time is interrupted ([Kinds](actions.md#kinds)) | The pipeline, at an action's start and delivery |
| `blocks = ["target"]` | Actions, orders, queries and homing projectiles do not choose it; an area still reaches it | Targets and queries |
| `blocks = ["damage"]` | Damage does nothing to it, an area's too | The damage pass |
| `hidden = true` | Other groups see it only through a detector that sees it ([Hidden units](vision.md#hidden-units)) | Vision |
| `detects = true` | Its sight shows hidden units | Vision |
| `immune = [tags]` | Every modifier that grants one of those tags is held with no effect: no stats, tags, intervals or events | The refresh |

- **One question each.** Each system asks only its own question of the tags, and none reads modifiers for a tag.
- **Stopped, not dropped.** A block keeps the order behind what it stops, and the action runs again when it ends, as League of Legends buffers input; an interrupted windup starts again from nothing and spends nothing, as Dota 2's cast point. An action its checks refuse for another reason is dropped.
- **Immunity suppresses.** An immunity holds the modifier instead of refusing it, as tags are derived: it takes effect again when the immunity ends. A modifier that grants an `immune` tag is never suppressed itself, so immunities come from the type's tags and those modifiers first; no order of modifiers changes the result.
- **Filters** read tags with a sign: `enemies:avatar:!stunned` ([Filters](../08-script-api.md#filters)).
- **Names** are declared names: a lowercase letter, then lowercase letters, digits and underscores.
- **The engine's tags** `avatar`, `dead`, `projectile`, `area`, `constructing`, `node`, `drop_off` and `gathering` hold the first places, in that order: each module declares its own, `d.engine_tags`, and the module list's order gives their numbers, the same in every match. A tag's properties say what the core reads of it: a filter selects a unit of `projectile` or `area` only when it names it, and a unit of `gathering` passes through each other one; a unit carries `dead` while it is dead, which its derived tags take from its `Dead` mark, and a filter that does not name it selects the dead as the living. Only the engine gives them; a filter and `[tags]` may name them. `constructing` blocks `attack`, `cast` and `use` of its own.
- A mode declares at most 256 tags, its types', its modifiers' and the engine's seven.

### Levels

A unit's level, from 1, is state; `progression` changes it. Its stats follow at once, and its pools by the rule of a rising maximum.

## State and derived

- **State**, hashed, saved, restored and replicated: each unit's modifiers, level and pools, and each player's modifiers, on its entity ([Replication audiences](../14-replication-audiences.md), D7). A restore refuses an instance, player modifier or area whose way does not give a param it, its aura or its inside modifiers read.
- **Derived**, never state: each unit's stats and tags, computed again whenever its modifiers, level or type change, or a live change's source changes, and after a restore, before any system reads them. The components that hold a stat's effect, a unit's step and its pools' maxima, follow the stats each time they change; they are state, hashed and replicated, from the unit's spawn. A weapon's period and damage are derived where they are read.

## Script API

`ctx.add_modifier`, `ctx.remove`, `ctx.purge`, `ctx.add_player_modifier`; `unit.stat(name)`, `unit.level`, `unit.pool(name)`, `unit.pool_max(name)`, `unit.has_tag(tag)`, `unit.has_modifier(id)`; the modifier handle `m` with `carrier`, `source`, `stacks` and `state`; the hook `on_interval(ctx, m)`.

## Network

Modifiers and life go to everyone who sees the unit, a player's modifiers to every client, the other pools to its owner, or as the mode sets ([Replication audiences](../14-replication-audiences.md)). A client keeps its own units' modifiers as the server sends them and runs their time, so one ends on the client in the tick it ends on the server; it derives their stats, tags, step and pools' maxima as the server does, so its predicted movement and actions obey them; a unit's status tags, `constructing` and `gathering`, go to everyone who sees it, so it derives every unit's tags it holds. It holds each aura's modifier on its own units from every carrier it sees, as an aura follows from where units stand; it creates no other modifier, as only the server runs effects, so a stun learned late costs a correction. It builds the mode's stat book from the packages it holds, with the unit types numbered as the server numbers them; the server sends each unit's type once and its level as it changes.

## Cost

A refresh walks a unit's modifiers once, growing with them and the stats each changes; only units whose modifiers, level or type changed refresh, and those that carry a live change, every pass. Each tick, every unit's pools regenerate, its modifiers' ends and intervals are counted, and each aura carrier finds the units in its radius.

## Genres

A MOBA's items and crowd control, a shooter's armor and flashbangs, an RTS's upgrades and cloak, an MMO's buffs and rage, a battle royale's armor and zone.
