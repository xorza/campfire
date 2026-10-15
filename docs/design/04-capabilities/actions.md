# Actions

## Mechanism

An action is anything a unit does on purpose: an attack, a cast, a shot, the use of an item or an object, a build, a train or a gather order. The core runs one pipeline for every action; capabilities add **kinds**, what is special about an action, and **deliveries**, how it reaches what it affects. So one cooldown, cost, range check and interruption rule serve a hero's ultimate, a rifle, a barracks and a quest dialogue, as the Gameplay Ability System makes an attack an ability and StarCraft II runs weapons and abilities through the same effects.

## Data

Every action is a table `[actions.<id>]` of a package, in one schema; a field its kind does not use fails the load.

| Field | Meaning |
| --- | --- |
| `kind` | `cast` (the default), `attack`, `use`, `enter`, `train`, `build`, `gather`, `craft` ([Kinds](#kinds)) |
| `script` | The script of its hooks, if any |
| `targeting` | `none`, `point`, `direction`, or a filter for a unit target |
| `range` | Meters in the map's metric, or `"global"` |
| `windup_ms` | From its start to its delivery: an attack's windup, a cast time, a train's time in its queue, a build's time, a gather's trip at its node |
| `cooldown_ms`, `charges` | `charges = { max, recharge_ms }` |
| `cost` | Per pool or player resource: `{ mana = 60 }`, `{ minerals = 50 }`; pools and player resources never share a name, and supply is no cost ([Supply](production.md#supply)) |
| `clamp_to_range` | A target beyond range is moved in, instead of the unit walking |
| `toggle` | `{ cost_per_attack }` or `{ cost_per_second }`, each per pool ([Over time](#over-time)) |
| `channel` | `{ duration_ms, tick_ms }`, from after it resolves ([Over time](#over-time)) |
| `hold` | A modifier held while the toggle is on or the channel runs |
| `charge` | `{ max_ms }`: a charged action, which resolves at release |
| `passive_modifier`, `passive_while_ready` | A modifier held while the action has a rank; with `passive_while_ready`, only off cooldown |
| `delivery` | `instant` (the default), `ray`, `sweep`, `{ projectile = "<unit type>" }` or `{ area = "<unit type>" }`, a type of the action's package; a projectile may add `count` and `spread_deg`, a fan spread evenly over the angle. It launches when the action delivers, before `on_resolve`. A homing projectile flies at a unit target; a weapon delivers at once, by a homing projectile, a ray or a sweep, never an area ([Weapons](combat.md#weapons)); an action that aims at nothing delivers no projectile, and one along a direction no area |
| `on_resolve`, `on_hit`, `on_end` | Effects in data ([Effects](#effects)) |
| `[params]` | Script values, read as `ctx.p.<name>` |

A kind adds its own fields: an attack's `rate`, `damage` and `damage_kind`, which takes only `[params]` and `on_hit` of the fields above ([Combat](combat.md#data)); a train's, a build's and a gather's ([Production](production.md#data)).

The mode declares its **slot kinds**, where actions sit on a unit, in order:

```toml
[[slots]]
name = "weapon"
[[slots]]
name = "basic"
ranks = 5
levels = [1, 3, 5, 7, 9]   # the level each rank needs; optional
[[slots]]
name = "ultimate"
ranks = 3
levels = [6, 11, 16]
```

A unit type fills them with actions of its package: `slots = { weapon = ["claws"], basic = ["fan_of_frost", "snow_owl", "chill_arrows"], ultimate = ["glacier_arrow"] }`. A unit's slots go kind after kind in the mode's order, each kind's actions in the type's order, so a client binds its keys to the same slots on every unit. A kind with no `ranks` has one rank, learned from the spawn; one with `ranks` starts unlearned. An action has the ranks of its kind, and per-rank values that many entries; the load refuses an action that types put in kinds of different ranks. A loadout fills a kind with the actions a player chose: `ctx.grant(unit, kind, ids)` puts them after the unit's slots of that kind, at the first rank ([Choices](control.md#choices)); an item's actions sit with the item ([Items](items.md)). `levels` holds one level for each rank, each at least 1 and at least the one before, on a kind with `ranks`, in a mode with a `level` track. The `learn` order needs the next rank's level; `ctx.learn` does not ([Progression](progression.md#rules)).

## Rules

### The pipeline

1. **Checks** as it starts, in Act: the slot's action is learned and off cooldown or has a charge, its cost is affordable, no tag of the unit blocks its group, and its target is a unit its filter selects, or a point, within range ([Reach](00-overview.md#space-and-map)). An action that takes no target ignores one its order names, so a client may always send the unit under the cursor.
2. **Time:** the windup. A tag that blocks its group interrupts it, back to its order, with nothing spent ([Tags](stats.md#tags)); so does a new order.
3. **Delivery,** in Hit, after the tick's attacks: the checks run again, without the range; then it delivers, and its `on_resolve` effects and hook run. The cost, the cooldown and every effect it queued apply together, or, when the checks or the script fail, none of them, the failure recorded.
4. **Effects** on each unit its delivery reaches: `on_hit` effects, then hook; as the delivery ends, `on_end`.
5. **After:** a channel runs, a toggle stays on, or an attack waits its period and starts again.

A field's value may be one or one per rank; times become whole ticks at load, rounded up.

### Over time

- **Charges.** An action with `charges` spawns full, and each start spends one in place of its cooldown check. While it holds fewer than `max`, one comes back each `recharge_ms`, the timer running from the tick the count fell below `max`, as Dota 2's charge restore; a `cooldown_ms` beside it is a lockout between two casts. `ctx.add_charge(unit, id)` gives one at once, up to `max`, and keeps the timer.
- **Range clamp.** A point aim beyond the range of an action with `clamp_to_range` moves in to the range, along the line from the unit, as League of Legends' Flash does. Any other aim beyond range makes a unit that walks walk in range first: its walk replaces any other, follows a moving unit target, and stops in range, where the cast starts. Failed checks on the way drop the cast and the walk, and a new order ends both; a tag that blocks its group, or a forced move, stops the walk while the cast waits. A unit that does not walk drops the cast.
- **Toggles.** A cast turns it on, its `cost` paid then; a second cast turns it off as it starts, with no windup, script or cost, so an empty pool can still turn one off; its cooldown starts at each. While on, it holds its `hold` modifier. `cost_per_attack` is paid as each attack goes off, before `on_attack`; `cost_per_second` at each whole second from the tick it turned on. A cost the pools cannot pay turns it off, and so do death and a tag that blocks casting: one rule for every toggle, where Dota 2 lets each choose.
- **Channels.** A `channel` runs from the tick after it resolves, for `duration_ms`, holding its `hold` modifier, with `on_channel_tick(ctx, unit, target)` each `tick_ms`, the last at its end at the latest. A new order, death, or a tag that blocks its action's group interrupts it, as Dota 2 and League of Legends do, and with `damage_interrupts = true`, damage of more than 0 to its unit, before shields absorb it, as Dota 2's channeled items end on damage: `on_interrupt(ctx, unit, target)` runs in Hit of the tick it was cut in, or of the next for a death or damage, both of Resolve, and the hold ends. A heal cuts none. A channel that ends whole runs no hook.
- **Charged actions.** A `charge` charges from its start in place of its windup, and resolves when its order is given again or at `max_ms`. A move order keeps it, as Varus walks while he draws; another action's order or a blocking tag cancels it, spending nothing. `ctx.charge`, the share of `max_ms` charged from 0 to 1, and `ctx.origin` and the target, as at its start, reach every hook of the action, its delivery's included.

### Values and bookkeeping

- **Values.** In an action's hooks, `ctx.range` is its range at its rank, `()` for a global one; `ctx.origin` where its unit stood as it started; `ctx.charge` fails for an action with no `charge`.
- **Cooldowns.** `ctx.reduce_cooldown(unit, id, ms)` takes `ms` off an action's cooldown, and `ctx.reduce_cooldowns(unit, kind, fraction)` a fraction of what is left off each action of a slot kind, rounded up to a whole tick, as Master Yi's takedowns do. Each also shortens the time to the next charge, and neither makes a ready tick earlier than now. An action the unit lacks, an undeclared kind, a negative `ms` and a fraction outside 0 to 1 fail the call.

### Kinds

| Kind | Capability | Group a tag blocks | What is special |
| --- | --- | --- | --- |
| `cast` | `abilities` | `cast` | Ranks, charges, toggles, channels, charged casts |
| `attack` | `combat` | `attack` | Repeats on its target each period its `rate` stat sets; draws the roll ([Combat](combat.md#weapons)) |
| `use` | `interaction`, run by `abilities` | `use` | Aims at a unit with a `use` section, from its user's use slot; a held use is a channel ([Interaction](interaction.md)) |
| `enter` | `interaction` | `use` | The unit rides in or garrisons the target |
| `train` | `production` | `use` | Joins its unit's queue, and spawns a unit type when its time ends ([Production](production.md)) |
| `build` | `production` | `use` | Places its building's box at a point, with an angle, as a site, or builds a site ([Construction](production.md#construction)) |
| `gather` | `production` | `use` | Loops from a node to a drop-off ([Gathering](production.md#gathering)) |
| `craft` | `items` | `use` | Takes a recipe's items and pools and makes an item, or adds modifiers to one (planned, [Items](items.md)) |

An action in an item's slot is in the `use` group, whatever its kind ([Items](items.md#rules)).

### Deliveries

| Delivery | Capability | Reaches |
| --- | --- | --- |
| `instant` | core | The target, at once |
| `ray` | `hitboxes` | The first unit a ray meets, or each for a weapon that penetrates, at the tick the shooter saw ([Hitboxes](hitboxes.md)) |
| `sweep` | `hitboxes` | Each unit a shape swept along an arc over the windup meets, once a swing |
| `projectile` | `projectiles` | What a projectile unit meets: it flies a line or homes, or falls under gravity (planned, [Gravity](../13-bodies-uses-shop-gravity.md#gravity)) |
| `area` | `areas` | The units inside an area unit: once after its delay, and while it lasts it holds its `inside` modifiers on them |

Projectiles and areas are units of types with a `projectile` or an `area` section, as a missile is a unit in StarCraft II's data editor, with stable ids, positions, tags, script state and vision. Each has its source's team and player, its type's tags and the engine's `projectile` or `area` tag, and no life: no attack or effect reaches it.

- **Projectile:** `speed`, `width`, `range` (the action's by default), `homing`, `gravity`, `stop_on_hit`, `once_per_cast`, `hits` (a filter, or `none`). It flies in Hit, a fixed distance each tick from the tick after it launches; on a planar map along the ground plane, at its launch height. Flights run before attacks deliver, and the tick's launches after. A homing one flies at its target's place of that tick and hits once its body, half its width across, reaches the target's; one whose target dies, despawns, becomes blocked as a target or disjoints ends without a hit, and a teleport disjoints ([Forced movement](navigation.md#forced-movement)). Every homing projectile flies faster than `max_move_speed`, which the load checks, so it always catches its target. A line one hits each unit `hits` selects, `enemies` by default, whose body comes within half its width of its path in the tick, by the path's nearest point to each, then stable id; each unit once, and once a cast with `once_per_cast`. It ends at its first hit with `stop_on_hit`, at its range, which the map's bounds cut, and at its point for a fan aimed at one. Projectiles take stable ids by their source's id, and a cast's share its first's id as their cast. With `hits = "none"` it hits nothing and flies to its end, as Snow Owl does; the load refuses `none` beside `width`, `stop_on_hit`, `once_per_cast`, `homing`, or an `on_hit` list or hook.
- **Area:** `radius`, `delay_ms` and `duration_ms` (0 by default), `affects` (a filter, `enemies` by default), `inside = { self, allies, enemies }` (modifiers of its package). A cone or a line is a projectile. It lands where its action aimed, where its unit target stands, or at its caster for an action that aims at nothing, after the tick's projectiles launch. It triggers once, in Hit after flights, its delay after the tick it lands, one tick at least: it reaches each living unit `affects` selects within its radius, one whose tags block it as a target among them, by stable id. It ends at its trigger or its duration's end, whichever is later. From landing to its end, in Resolve, it holds `self` on its caster, `allies` on the caster's allies and `enemies` on the units that may be attacked, while each is within the radius, as an aura holds its modifier ([Modifiers](stats.md#modifiers)), from its caster with its action's params at its rank, as a Dota thinker carries an aura. Areas take stable ids after the tick's projectiles, by their source's id.

A projectile or area type may have a `vision` section, revealing to its source's group what it sees while it lasts ([Vision](vision.md#grid-fog-of-war)).

Each delivery records how it reached a unit in a **hit**: the projectile or area unit, or `()` for an instant one; the unit the action aimed at; where it hit or ended, an area's center for an area; the distance flown or a dash's way, 0 for an area; the direction, none for an area; and for a ray or sweep the body part. `on_hit` and `on_end` receive it, and its damage carries it as `d.hit` ([Combat](combat.md#damage-and-heals)).

### Effects

An effect list is an array of tables, queued in order, as a script's calls are:

| Effect | Does |
| --- | --- |
| `damage = { amount, kind }` | Damage to the unit reached |
| `heal = { amount }` | A heal of its life |
| `restore = { pool, amount }` | Adds to one of its pools |
| `modifier = { id, duration_ms }` | Applies a modifier from the acting unit |
| `purge = { tag }` | Ends the applications of its modifiers that grant a tag the mode declares, whatever their source; an instance a passive, an aura, an area or a player holds stays, as Dota 2's dispels never remove passives or auras |
| `spawn = { unit_type, duration_ms }` | Spawns a unit of a type of the action's package that stands, where the unit it applies to stands, or at the point an `on_resolve` aimed at, on the source's team and player; with `duration_ms`, it lives that many ticks, rounded up, from its spawn tick, as a ward does |
| `launch = { area, on_hit, on_end }` | Lands an area of the action's package where it stands, which runs its own lists |
| `move = { to, speed }`, `move = { from, distance, ms }` | A dash at the unit `to` names, or a knock back from the one `from` names, `"source"` or `"reached"` ([Forced movement](navigation.md#forced-movement)) |
| `xp = { track, amount }` | Experience on a track ([Progression](progression.md)) |
| `loot = { table, level }` | Rolls a random table and drops what it gives (planned) |
| `noise = { radius }` | A noise units within the radius hear (planned, [Senses](vision.md#senses)) |

A launched area comes from the acting unit with the action's params at its rank. One in a delivery's list lands in that Hit, after the tick's projectiles launch; one in a weapon's `on_hit`, in the damage pass, lands in Resolve once the pass ends; either triggers its delay after the tick it lands. It runs its `on_hit` and `on_end` lists and no hook, and a list it holds may launch again, so a chain in data is as deep as its nesting. A projectile has no launch effect: data knows no aim for it, and a script aims one with `ctx.projectile`.

Every effect takes `to = "source"` to apply to the acting unit; a list that reaches no unit holds only such effects. Numbers take `{ param = "<name>" }` and are not negative at any rank; a scaling param below zero counts as zero. A list runs before the hook of the same name, in one call: its effects queue first, and a hook that fails applies neither. A list whose hook the script does not define applies alone. The release runs the lists of a cast and a weapon's `on_hit`, and the effects the [reference](../08-script-api-reference.md) marks as running; the load refuses a planned effect. Each capability registers how its effects queue, so a mode of weapons alone runs them too. Rime's Fan of Frost:

```toml
[actions.fan_of_frost]
targeting = "direction"
range = "12.0"
cooldown_ms = [16000, 13000, 10000, 7000, 4000]
cost = { mana = 60 }
windup_ms = 250
delivery = { projectile = "frost_arrow", count = 7, spread_deg = "57.5" }
on_hit = [
    { damage = { amount = { param = "damage" }, kind = "physical" } },
    { modifier = { id = "slow", duration_ms = { param = "slow_ms" } } },
]
```

### Who starts an action

An order names an action and a target ([Orders](control.md#orders)); a `character` input frame's button names a slot, aimed where the character looks ([Character](control.md#character)); an AI orders as a player does.

## State and derived

- **State:** each unit's slots (the action and its rank, `ActionSlots`) and their times (ready tick, charges and the next one's tick, a toggle's next payment, `SlotTimes`); its action in progress (slot, target, delivery tick, origin, a charge's start); its channel (slot, end, next tick); each projectile's and area's unit.
- **Derived:** the values at each rank, at load; a scaling param from the source's stats when read.

## Script API

`ctx.p`, `ctx.range`, `ctx.charge`, `ctx.origin`; `ctx.projectile(from, direction)` for a line type and `(from, unit)` for a homing one, the other form failing, and `ctx.area(pos)`, each returning the new unit ([New units at once](../08-script-api.md#rules)); `ctx.reduce_cooldown`, `ctx.reduce_cooldowns`, `ctx.add_charge`, `ctx.learn(unit, slot)`; the hooks `on_resolve(ctx, unit, target)`, `on_hit(ctx, unit, target, hit)`, `on_end(ctx, unit, hit)`, `on_channel_tick(ctx, unit, target)`, `on_interrupt(ctx, unit, target)`.

## Network

A unit's slots and the action under way go to everyone who sees it; its `SlotTimes` to the owner, or the team, as the mode sets ([Replication audiences](../14-replication-audiences.md), `cooldowns`). Projectiles and areas replicate as units. A client predicts the start of its own actions; their effects come from the server, and it flies no projectile and triggers no area.

## Cost

Each action in progress costs its checks at its start and its delivery. Each live projectile costs a step and a hit test a tick; each area a query of its radius when it triggers and each tick it holds modifiers. A ray costs a test against the hitboxes near its line.

## Genres

A MOBA's attacks, abilities, item actives and spells; a shooter's guns (attacks by ray), grenades (projectiles) and the bomb (a use); an RTS's weapons, abilities and production; an MMO's spells and auto attack; a battle royale's weapons, consumables and zone (an area).
