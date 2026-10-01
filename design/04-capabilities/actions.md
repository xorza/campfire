# Actions

## Mechanism

An action is anything a unit does on purpose: an attack, a cast, a shot, the use of an item or of an object, a build, a train or a gather order. The core runs one pipeline for every action; capabilities add **kinds**, which say what is special about an action, and **deliveries**, which say how it reaches what it affects. So one cooldown, one cost, one range check and one interruption rule serve a MOBA hero's ultimate, a rifle, an RTS barracks and an MMO's quest dialogue, as the Gameplay Ability System makes an attack an ability and StarCraft II runs weapons and abilities through the same effects.

## Data

Every action is a table `[actions.<id>]` of a package, in one schema; a field a kind does not use fails the load.

| Field | Meaning |
| --- | --- |
| `kind` | `cast` (the default), `attack`, `use`, `enter`, `train`, `build`, `gather`, `craft` ([Kinds](#kinds)) |
| `script` | The script of its hooks, if it needs one |
| `targeting` | `none`, `point`, `direction`, or a filter for a unit target |
| `range` | Meters in the map's metric, or `"global"` |
| `windup_ms` | The time between its start and its delivery: an attack's windup, a cast time |
| `cooldown_ms`, `charges` | `{ max, recharge_ms }` |
| `cost` | Per pool of the unit or player resource of its player: `{ mana = 60 }`, `{ rage = 30, combo = 1 }`, `{ minerals = 50, supply = 1 }`; the mode's pools and player resources never share a name |
| `clamp_to_range` | A target beyond range is moved in, instead of the unit walking |
| `toggle` | `{ cost_per_attack }` or `{ cost_per_second }`, each per pool; turned off at an empty pool and at death |
| `channel` | `{ duration_ms, tick_ms }`; starts after the action resolves; `on_channel_tick` each tick of it |
| `hold` | A modifier held while the toggle is on or the channel runs |
| `charge` | `{ max_ms }`: a charged action. It resolves at release, with `ctx.charge` from 0 to 1 and `ctx.origin` and the target as they were when the charge began |
| `passive_modifier`, `passive_while_ready` | A modifier held while the action has a rank; with `passive_while_ready`, only while it is off cooldown |
| `delivery` | `instant` (the default), `ray`, `sweep`, `{ projectile = "<unit type>" }` or `{ area = "<unit type>" }`; a projectile may add `count` and `spread_deg`, a fan of that many spread evenly over the angle ([Deliveries](#deliveries)) |
| `on_resolve`, `on_hit`, `on_end` | Effects in data ([Effects](#effects)) |
| `[params]` | Script values, read as `ctx.p.<name>` |

A kind adds its own fields: an attack's `rate`, `damage` and `damage_kind` ([Combat](combat.md#data)), a use's channel and range, a train's unit type and time.

The mode declares its **slot kinds**, where actions sit on a unit:

```toml
[slots.basic]
ranks = 5
levels = [1, 3, 5, 7, 9]   # the level each rank needs; optional
[slots.ultimate]
ranks = 3
levels = [6, 11, 16]
[slots.weapon]
```

A unit type fills them: `actions = { weapon = ["claws"], basic = ["fan_of_frost", "snow_owl", "chill_arrows"], ultimate = ["glacier_arrow"] }`. A slot kind with no `ranks` has one rank, learned from the spawn. A loadout fills a slot kind with the actions a player chose ([Choices](control.md#choices)); an item's actions sit with the item ([Items](items.md)).

## Rules

### The pipeline

1. **Checks** as it starts, in Act: the slot's action is learned and off cooldown or has a charge, its cost is affordable in each pool and player resource it names, no tag of the unit blocks its group, and its target is a unit its filter selects or a point, within range from the edge of the unit's body to the edge of the target's. An action that takes no target ignores one its order names, so a client may always send the unit under the cursor.
2. **Time:** the windup. A tag that blocks the action's group interrupts it, back to the order behind it, and nothing is spent ([Tags](stats.md#tags)); so does a new order.
3. **Delivery,** in Hit, after the tick's attacks: the checks run again, without the range; then the action delivers, its `on_resolve` effects run, and its `on_resolve` hook. The cost, the cooldown and every effect it queued apply together. If the checks fail or the script fails, none of them apply, and the failure goes to the tick's script errors.
4. **Effects** on each unit its delivery reaches: its `on_hit` effects, then its `on_hit` hook; when the delivery ends, `on_end`.
5. **After:** a channel runs, or a toggle stays on, or an attack waits for its period and starts again on its target.

Values of the fields may be one or one per rank, like params; times become whole ticks when the action loads, rounded up.

### Kinds

| Kind | Capability | Group a tag blocks | What is special |
| --- | --- | --- | --- |
| `cast` | `abilities` | `cast` | Ranks, charges, toggles, channels, charged casts |
| `attack` | `combat` | `attack` | Repeats on its target, each period its `rate` stat sets; draws the roll ([Combat](combat.md#weapons)) |
| `use` | `interaction` | `use` | Aims at a unit with a `use` section; a held use is a channel ([Interaction](interaction.md)) |
| `enter` | `interaction` | `use` | The unit rides in or garrisons the target |
| `train` | `production` | `use` | Joins its unit's queue, and spawns a unit type when its time ends ([Production](production.md)) |
| `build` | `production` | `use` | Aims at a point where a footprint fits, and places a building |
| `gather` | `production` | `use` | Walks to a node, gathers, and carries the load to a drop-off, again until stopped |
| `craft` | `items` | `use` | Takes a recipe's items and pools, at a station a filter selects or anywhere, and makes an item, or adds modifiers to one, as enchanting does ([Items](items.md)) |

### Deliveries

| Delivery | Capability | Reaches |
| --- | --- | --- |
| `instant` | core | The target, at once |
| `ray` | `hitboxes` | The first unit, or each unit for a weapon that penetrates, a ray meets against hitboxes, tested at the tick the shooter saw ([Hitboxes](hitboxes.md)) |
| `sweep` | `hitboxes` | Each unit a shape swept along an arc over the windup meets against hitboxes, once a swing: a sword or an axe |
| `projectile` | `projectiles` | What a projectile unit meets: it flies a line, homes on a unit or falls under gravity |
| `area` | `areas` | The units inside an area unit: once after its delay, and while it lasts it holds its `inside` modifiers on them |

Projectiles and areas are units of types with a `projectile` or an `area` section, as a missile is a unit in the [StarCraft II data editor](https://s2editor-guides.readthedocs.io/New_Tutorials/04_Data_Editor/058_Data_Editor_Introduction/), so they have stable ids, positions, tags, script state and vision as every unit does:

- **Projectile:** `speed`, `width`, `range` (the action's by default), `homing`, `gravity`, `stop_on_hit`, `once_per_cast`, `hits` (a filter), `sight_radius`, `collide`, `[state]`. It flies in the Hit stage, a fixed distance each tick, from the tick after it launches; flights run before attacks deliver, and the tick's launches after. A homing one flies at its target's position of that tick, and hits when it reaches it, in that tick's Resolve; one whose target dies, despawns, becomes blocked as a target or disjoints before it lands ends without a hit. A teleport, and a forced move longer than an engine constant, disjoint: they end the homing projectiles that target the unit, since the move speed cap does not limit them. Every homing projectile flies faster than the mode's `max_move_speed`, which the load checks, so it always catches its target and the number of live projectiles is bounded. Projectiles take stable ids in the order of their source's id, so every run numbers them alike.
- **Area:** `radius`, `delay_ms`, `duration_ms`, `affects` (a filter), `inside = { self, allies, enemies }` (modifiers held on the units inside), `[state]`. A cone or a line is a projectile.

Each delivery records how it reached a unit in a **hit**: the projectile or area unit that delivered it, or `()` at once; the unit the action aimed at; where it hit, or where its delivery ended; the distance flown; the direction; and, for a ray or a sweep, the body part. The `on_hit` and `on_end` hooks receive it, and the damage the delivery deals carries it as `d.hit` ([Combat](combat.md#damage-and-heals)).

### Effects

An effect list is an array of effects, each one table; the effects queue in order, as the calls of a script do:

| Effect | Does |
| --- | --- |
| `damage = { amount, kind }` | Damage to the unit reached |
| `heal = { amount }` | A heal of its life pool |
| `restore = { pool, amount }` | Adds to one of its pools |
| `modifier = { id, duration_ms }` | Applies a modifier from the acting unit |
| `purge = { tag }` | Ends its modifiers that grant the tag |
| `spawn = { unit_type, team }` | Spawns a unit where it stands |
| `launch = { projectile }`, `launch = { area }` | Launches a delivery from it |
| `move = { to, speed }`, `move = { from, distance, ms }` | Forced movement: a dash or a knock back ([Navigation](navigation.md#forced-movement)) |
| `xp = { track, amount }` | Experience on a track ([Progression](progression.md)) |
| `loot = { table, level }` | Rolls a random table and drops what it gives where the unit stands ([Random tables](00-overview.md#random-tables)) |
| `noise = { radius }` | A noise that units within the radius hear ([Senses](vision.md#senses)) |

Every effect takes `to = "source"` to apply to the acting unit instead. Numbers take `{ param = "<name>" }`. An effect list runs before the script hook of the same name, so a hook adds only what data cannot say. One example, Rime's Fan of Frost:

```toml
[actions.fan_of_frost]
targeting = "direction"
cost = { mana = [60, 65, 70, 75, 80] }
cooldown_ms = 9000
delivery = { projectile = "frost_arrow", count = 5, spread_deg = 30 }
on_hit = [{ damage = { amount = { param = "damage" }, kind = "physical" } }, { modifier = { id = "chilled" } }]
```

### Who starts an action

An order names an action and a target ([Orders](control.md#orders)); a button of a `character` input frame names a slot, aimed where the character looks ([Character](control.md#character)); an AI orders as a player does. So one action serves a MOBA hero, a first-person character and an RTS unit.

## State and derived

- **State:** each unit's slots (the action, its rank, the tick it is ready, its charges), its action in progress (the slot, the target, the tick it delivers), its toggles and channels; each projectile's and area's unit.
- **Derived:** the values at each rank, from data, when the package loads; a scaling param from the source's stats when it is read.

## Script API

`ctx.p`, `ctx.range`, `ctx.charge`, `ctx.origin`; `ctx.projectile(from, to)` and `(from, to, overrides)`, `ctx.area(pos)`; `ctx.reduce_cooldown(unit, id, ms)`, `ctx.reduce_cooldowns(unit, slot_kind, fraction)`, `ctx.add_charge(unit, id)`, `ctx.learn(unit, slot)`; the hooks `on_resolve(ctx, unit, target)`, `on_hit(ctx, unit, target, hit)`, `on_end(ctx, unit, hit)`, `on_channel_tick(ctx, unit)` and `on_interrupt(ctx, unit, target)`. Projectiles and areas are unit handles, with the fields of their sections.

## Network

Cooldowns and charges go to the owner, or the team, as the mode sets. Projectiles and areas replicate as units, to the clients whose group sees them. A client predicts the start of its own actions; their effects appear when the server confirms them.

## Cost

Each action in progress costs its checks when it starts and again when it delivers. Each live projectile costs a step and a hit test a tick; each area a query of its radius when it triggers and a tick while it holds modifiers. A ray costs a test against the hitboxes near its line at the tick it names.

## Genres

A MOBA's attacks, abilities, item actives and player spells; a shooter's guns (attack actions with a ray delivery), grenades (projectiles) and the bomb (a use); an RTS's weapons, abilities and production; an MMO's spells, auto attack and quest dialogue; a battle royale's weapons, consumables and the zone (an area).
