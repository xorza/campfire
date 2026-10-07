# Stats

## Mechanism

How a unit's numbers, pools and tags come from its type, its level and the modifiers it carries. How a unit gains levels is [Progression](progression.md)'s. The model follows Unreal's Gameplay Ability System where the field agrees: a base value from the type and level, a current value that modifiers change through one formula, stacks counted by source, tags that block actions and grant immunity. The rules for movement and pools follow League of Legends, where they are exact.

## Data

The mode declares, in `data/mode.toml`, its stats, its pools and the properties of its tags:

```toml
[stats.armor]
min = "-100"          # optional, both
max = "500"

[pools.health]
max = "health"        # the stat of its maximum
regen = "health_regen" # optional: the stat of its regen

[tags.stunned]
blocks = ["move", "attack", "cast", "use"]
```

A unit type gives its values in `stats` (`stat = { base, per_level }`), lists its `pools`, and its `tags`. A modifier, in `[modifiers.<id>]`, gives `stats` (per stack), `tags` (those it grants its carrier), `script`, `duration_ms`, `interval_ms`, `stacks_expire_ms`, `reapply`, `max_stacks`, `shield`, `aura`, `affects` (the units of its player a player modifier holds on, all of them when absent), `on_interval` (effects), `[params]` and `[state]` ([Script API](../08-script-api.md#data-files)).

## Rules

### Stats

A unit's value of a stat comes from its type's value at its level, `base + per_level × (level − 1)`, and the values its modifiers give the stat, each by one operation, times the modifier's stacks:

```
value = (base + Σ add) × (1 + Σ pct) × (1 − max cut)
```

then the stat's limits clamp it. `add` and `pct` add up, so the order of modifiers never matters; only the largest `cut` counts, as only the strongest slow does in League of Legends, and a cut counts at most 1. Each product rounds once, to nearest. A sum that leaves the range of a number stops at its end before the limits. A modifier writes a plain number for `add`, and a table for the others: `stats = { armor = 20, move_speed = { cut = "0.3" }, attack_speed = { pct = "0.25" } }`.

The engine reads only the stats its mechanisms need; each one the mode does not declare keeps the value its unit's kit gave it:

| Stat | Engine use |
| --- | --- |
| `move_speed` | Meters a second, within its limits and the manifest's `max_move_speed`; a step a tick, rounded once |
| A pool's `max` and `regen` stats | The pool's maximum, and what it gains a second: regen ÷ tick rate a tick, the remainder carried, so a second gains exactly the regen |
| A weapon's `rate` and `damage` stats | Attacks a second and the damage of each ([Combat](combat.md#weapons)); the period is the tick rate over the rate, rounded up, and at least a tick longer than the windup |

The damage rules that read a stat bind it by the mode's name in `[combat]` ([Combat](combat.md#data)).

### Pools

A mode declares at most eight pools, so a unit's pools are a fixed array that a rollback copies without an allocation. A pool is an amount from 0 to the value of its `max` stat, which its `regen` stat refills while its unit lives. When a pool's maximum rises, its current amount rises by as much; when the maximum falls, the current amount stays, unless it is now above the maximum. This is League of Legends' rule; Dota 2 keeps the fraction instead, which needs a rounding the engine does not take. A unit type's maximum of each pool is positive at level 1, or the type fails to load; a modifier that later lowers it to 0 or below leaves it at the smallest positive number, so a pool always has a maximum. Health is the pool `[combat] life` names; costs, restores and shields name the others.

### Modifiers

A modifier is an instance on its carrier, from a source: the unit whose action, delivery or aura applied it, or none for one the mode applies. A carrier holds at most one instance of an id from each source, as the Gameplay Ability System's stacking by source does.

- **Applying.** `ctx.add_modifier(unit, id)` and `(unit, id, duration_ms)` return the instance's handle; an effect in data applies it the same way. When the instance exists, `reapply` decides: `refresh` sets its duration whole again; `stack` adds a stack, up to `max_stacks`, and sets the duration whole again; `ignore` leaves it. Either way the handle of the instance comes back.
- **Stacks.** With `stacks_expire_ms`, each stack keeps its own end, and the instance ends with its last stack. A stack past `max_stacks` takes the place of the stack that ends soonest. A script may write `m.stacks`: the stacks it removes are those that end soonest, and those it adds end as stacks applied in that tick do. At 0 the instance stays, with no stats and no tags.
- **Time.** A duration counts in ticks, rounded up and at least one, from the end of the tick it was applied in: a modifier of `d` ticks applied in tick `t` holds through every stage of tick `t + d`, and ends as tick `t + d + 1` starts, as does a stack that ends on its own.
- **Lifetime.** Holds and an application of its own keep an instance: a passive's rank, an aura, an area or a player each hold it, and an application keeps it until its duration ends, or for good with none. It ends only when nothing keeps it, so an instance that an aura holds and an ability also applies with a duration stays while the aura holds it and until the duration ends, whichever is later; every hold of one modifier from one source is one hold.
- **Ending.** `ctx.remove(handle)` ends it at once; `ctx.purge(unit, tag)` ends every instance on the unit that grants the tag; so does a spent shield, and the carrier's death, except for a passive.
- **Passives.** A modifier held while a condition holds: an action's `passive_modifier` while the action has a rank, with `passive_while_ready` only while it is off cooldown; an equipped item's while it is equipped; a unit type's `passive` from its spawn on, with no ability, at rank 1. A passive comes from the carrier itself, and keeps its stacks and state across a death. As the hold is the modifier's from the carrier, a modifier is the passive of one owner of its package at most, one unit type's `passive` or one action's `passive_modifier`; the load refuses a second, which would share the hold. A passive whose stacks end one by one counts something, as Twin Cut counts attacks: it starts with no stacks, and it stays when its last stack ends.
- **Auras.** A modifier with an `aura` makes its carrier a source: in Resolve each tick, every living unit whose body comes within the aura's radius of its carrier's position that `affects` selects holds the aura's modifier from that carrier, with no duration, and loses it the tick it leaves. An area's `inside` modifiers hold the same way, from the area's caster ([Deliveries](actions.md#deliveries)), so one reach rule holds for both ([Reach](00-overview.md#space-and-map)). An instance projects its aura only while it has a stack and takes effect on its carrier: one at 0 stacks, or one whose granted tags the carrier's immunity suppresses, projects none. There is no linger: the sim has no flicker to hide.
- **Player modifiers.** A player may hold a modifier for the units it owns, `ctx.add_player_modifier(player, id)`: in Resolve each tick, each living unit of the player that the modifier's `affects` filter selects, as it selects the unit itself, holds it from no source at rank 1, with no duration, as an aura holds it in a radius; a unit the player gains, or one that comes to match, holds it from that tick. Which players hold which modifiers is state. An RTS's upgrades and an MMO guild's bonuses are player modifiers.
- **Order.** Instances are kept by id, then source, so every walk over them is in the same order on every machine.
- **Ids by package.** A modifier's id is its name within its package, the mode or a package it depends on; a script names the modifiers of its own package, so two packages may each have a `haste`.
- **Numbers.** A modifier's number fields, its duration, interval, stack expiry, shield, aura radius and `stats`, resolve when it is applied, and again when it is refreshed or stacked; a new interval counts from the next, which keeps its tick, so refreshes never put an interval off: a `{ param }` reads the modifier's own params, then those of the action that applied it, at that action's rank on the source, or rank 1 with none. A rank that changes later changes the next application, not one held, as the Gameplay Ability System captures a magnitude when it applies an effect. One exception: a `stats` change that reads a scaling param follows its source's level and stats while it is held, as League of Legends' bonus-scaling passives do, so it is computed again whenever they change; when its source is gone, it keeps the value it last had, as the Gameplay Ability System keeps a captured magnitude. An instance at no stack changes no stat, so its change is not computed. To keep this finite, the stats such changes read and the stats they change form no loop across the mode's modifiers: a change of `armor` may not read `armor`, nor read `attack_damage` while another change of `attack_damage` reads `armor`. The load refuses a loop, and the engine computes the stats in an order where each comes after those it reads. A number that the data gives as a value, not as a `{ param }`, is fixed at load: the load refuses a modifier whose value is past what a number holds, or whose fixed time is negative. An aura's radius and a shield are never negative: the load refuses a negative value, and a param negative at a rank, its own or an applier's, and a scaling param that gives a negative value as it applies gives 0, as a decode of the instance refuses one below 0.
- **Appliers.** Every way a modifier is applied gives each param the modifier reads, so an application always resolves and never fails after the call that queued it. The ways are:
  - an action's `hold`, `passive_modifier` and effect lists, the `inside` of the area it delivers, and its script's `ctx.add_modifier`: that action, at each of its ranks;
  - a unit type's `passive`, `ctx.add_player_modifier`, and the `ctx.add_modifier` of the mode's script or an AI's: no action, at rank 1;
  - the aura of a modifier, and the `ctx.add_modifier` of a modifier's script: each way of that modifier.

  The load refuses a modifier when one of its ways lacks a param its numbers or its script read: an action that does not declare the param, or a way with no action for a param that is not the modifier's own. It also refuses an own param per rank with fewer entries than the ranks of an action that applies the modifier; and, for a param its numbers read, a value past what a number holds at a rank a way reads, a time that reads a scaling param, whose value only its source knows, and a time that is negative, or too large to count in ticks at the fastest rate, at any rank. A param only its script reads is any value, as `ctx.p` gives it. The registry marks which arguments apply a modifier, and with which action, so the load reads the ways of a script's literal names; `has_modifier` only names one. A name a script computes is checked at its call: `ctx.add_modifier` and `ctx.add_player_modifier` fail the call when the call's way, its action at its rank or none, does not give a param the modifier's numbers read as the load requires, or when an own per-rank param has no value at that rank. The stat graph reads the same ways.
- **Its action.** An instance keeps the action whose resolve, delivery or modifier applied it, or whose passive it is; one the mode applies has none.
- **Handles.** `ctx.add_modifier` returns the handle of the instance it will add or reapply. A call that applies one instance twice gets the same handle twice, which shows both applications; after `ctx.remove`, a new application starts it again from its defaults. Writes to a handle's `stacks` and `state` in a call apply when the call ends, after its adds, and the call reads them back; a failed call writes none. A modifier's `[state]` starts at its defaults when it is added and is kept when it is refreshed or stacked.

Intervals, shields and the events modifiers hear are part of [Combat](combat.md#events): the damage pass runs them.

### Tags

A unit's tags are its type's tags, the tags the engine gives it by its sections (`avatar`, `projectile`, `area`, `item`, and its layer's name) and by what it is now (`constructing`, while a site), and the tags its modifiers grant, those of 0 stacks and those an immunity suppresses aside. A tag has properties only when the mode declares them in `[tags.<name>]`:

| Property | Means | Asked by |
| --- | --- | --- |
| `blocks = ["move"]` | The unit does not move | The Move stage |
| `blocks = ["attack"]`, `["cast"]`, `["use"]` | The unit starts no action of that group, and one in its time is interrupted ([Actions](actions.md#kinds)) | The action pipeline, as an action starts and as it delivers |
| `blocks = ["target"]` | Actions, orders, queries and homing projectiles do not choose the unit; an area still reaches it | Targets and queries |
| `blocks = ["damage"]` | Damage does nothing to the unit, from an area either | The damage pass |
| `hidden = true` | Other vision groups see the unit only through a unit with a `detects` tag that sees it ([Vision](vision.md#hidden-units)) | Vision |
| `detects = true` | The unit's sight shows hidden units | Vision |
| `immune = [tags]` | Every modifier that grants one of those tags is held but has no effect: no stats, no tags, no intervals, no events | The refresh that derives stats and tags |

- **One question each.** Each system asks only its own question of the unit's tags, and no system reads modifiers for a tag.
- **Stopped, not dropped.** A block keeps the order behind what it stops: the destination and route, the attack target, the ordered action. The action runs again when the block ends, as League of Legends buffers input. A windup or a cast time it interrupts starts again from nothing, and an interrupted action spends no cost and no cooldown, as Dota 2's cast point does. An action its checks refuse for another reason is dropped.
- **Immunity suppresses.** An immunity follows the Gameplay Ability System's application immunity, but holds the modifier instead of refusing it: tags are derived, so the modifier takes effect again when the immunity ends, with no state to restore. A modifier that grants a tag with an `immune` property is never suppressed itself, so immunities come from the type's tags and those modifiers first, and then suppress the rest; no order of modifiers can change the result.
- **Filters** read tags with a sign: `enemies:avatar:!stunned` selects the avatars that may be attacked and are not stunned ([Filters](../08-script-api.md#filters)).
- **Names.** A tag's name is a declared name: a lowercase letter, then lowercase letters, digits and underscores, so every tag can stand in a filter. The load refuses another name where data gives it.
- **The engine's tags.** `avatar`, `projectile`, `area` and `constructing` hold the first places among a match's tags, in that order, so the engine finds them without their names. Only the engine gives them: the load refuses a unit type or a modifier that carries one. A filter and the mode's `[tags]` may name them, and give them properties; `constructing`, which a site carries ([Construction](production.md#construction)), blocks the `attack`, `cast` and `use` groups of its own, so no action of a site starts.
- A mode declares at most 256 tags together, its types', its modifiers' and the engine's three.

### Levels

A unit's level, from 1, is state beside its modifiers; `progression` changes it. A unit's stats follow its level at once, and its pools by the rule of a maximum that rises.

## State and derived

- **State**, hashed, saved, restored and replicated: each unit's modifiers, its level, the current amount of each pool, and the players' modifiers. A restore refuses an instance, a player's modifier or an area whose way, its action at its rank or none, does not give a param that it, its aura's or its inside modifiers read, as a call checks a computed name: the next application of one would have no value to read.
- **Derived**, never state: each unit's stats and tags. They are computed from the state again whenever its modifiers, level or type change, or a source of a live change it carries changes, and after a restore, before any system reads them, so the server, a client after a rollback and a replay see the same numbers. The components that hold a stat's effect, a unit's step a tick, its weapons' periods and damage and its pools' maxima, take their numbers from the derived stats each time they change; those components exist from the unit's spawn, as state, so their numbers enter the hash as well.

## Script API

`ctx.add_modifier`, `ctx.remove`, `ctx.purge`, `ctx.add_player_modifier`; `unit.stat(name)`, `unit.level`, `unit.pool(name)`, `unit.pool_max(name)`, `unit.has_tag(tag)`, `unit.has_modifier(id)`; the modifier handle `m` with `carrier`, `source`, `stacks` and `state`; the hook `on_interval(ctx, m)`. The [reference](../08-script-api-reference.md) lists each.

## Network

Visible modifiers and pools go to everyone who sees the unit, unless the mode sends a pool to its owner or team only. A client keeps its own units' modifiers as the server sends them and runs their time, so a modifier ends on the client in the tick it ends on the server; it derives their stats and tags as the server does, so its predicted movement and actions obey them. It creates no modifier: those come from effects, which only the server runs, so a stun the client learns late costs a correction, as the Gameplay Ability System also predicts no effect it did not start.

To derive them, a client builds the mode's stat book from the packages it holds, with the unit types numbered as the server numbers them, by one order both read from the packages; the server sends each unit's type once and its level as it changes. A unit's step, its weapons' periods and damage, and its pools' maxima are then derived on both sides, and the server does not send them.

## Cost

A refresh of a unit walks its modifiers once: its cost grows with the modifiers and the stats each changes. Only units whose modifiers, level or type changed refresh, and those that carry a live change, which refresh every pass. Each tick, every unit's pools regenerate, its modifiers' ends and intervals are counted, and each aura carrier finds the units within its radius.

## Genres

Every target game uses stats, pools and modifiers: a MOBA's items and crowd control, a shooter's armor and flashbangs as tags, an RTS's upgrades as player modifiers and cloak and detection as tags, an MMO's buffs, rage and combo points, a battle royale's armor pool and the zone's damage through a modifier.
