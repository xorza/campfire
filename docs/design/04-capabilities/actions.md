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
| `windup_ms` | The time between its start and its delivery: an attack's windup, a cast time, a train's time in its unit's queue |
| `cooldown_ms`, `charges` | `{ max, recharge_ms }` |
| `cost` | Per pool of the unit or player resource of its player: `{ mana = 60 }`, `{ rage = 30, combo = 1 }`, `{ minerals = 50, supply = 1 }`; the mode's pools and player resources never share a name |
| `clamp_to_range` | A target beyond range is moved in, instead of the unit walking |
| `toggle` | `{ cost_per_attack }` or `{ cost_per_second }`, each per pool; turned off at an empty pool and at death |
| `channel` | `{ duration_ms, tick_ms }`; starts after the action resolves; `on_channel_tick` each tick of it |
| `hold` | A modifier held while the toggle is on or the channel runs |
| `charge` | `{ max_ms }`: a charged action. It resolves at release, with `ctx.charge` from 0 to 1 and `ctx.origin` and the target as they were when the charge began |
| `passive_modifier`, `passive_while_ready` | A modifier held while the action has a rank; with `passive_while_ready`, only while it is off cooldown |
| `delivery` | `instant` (the default), `ray`, `sweep`, `{ projectile = "<unit type>" }` or `{ area = "<unit type>" }`, a type of the action's own package; a projectile may add `count` and `spread_deg`, a fan of that many spread evenly over the angle around the aim. It launches when the action delivers, before `on_resolve`. A homing projectile flies alone, at a unit target; a weapon delivers at once, by a homing projectile, a ray or a sweep, never by an area ([Weapons](combat.md#weapons)); an action that aims at nothing delivers no projectile, and one that aims along a direction no area ([Deliveries](#deliveries)) |
| `on_resolve`, `on_hit`, `on_end` | Effects in data ([Effects](#effects)) |
| `[params]` | Script values, read as `ctx.p.<name>` |

A kind adds its own fields: an attack's `rate`, `damage` and `damage_kind`, and it takes `[params]` and `on_hit` alone of the fields above that a cast's script and lists use ([Combat](combat.md#data)), a use's channel and range, a train's unit type and time.

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

A unit type fills them with actions of its package, the mode's for a mode's unit type: `slots = { weapon = ["claws"], basic = ["fan_of_frost", "snow_owl", "chill_arrows"], ultimate = ["glacier_arrow"] }`. A unit's slots go kind after kind in the mode's order, each kind's actions in the order the unit type lists them, so a client binds its keys to the same slots on every unit. A slot kind with no `ranks` has one rank, learned from the spawn; one with `ranks` starts unlearned. An action has the ranks of the kind it sits in, and its per-rank values that many entries; the load refuses an action that unit types put in kinds of different ranks, and loads each action once, however many unit types hold it. A loadout fills a slot kind with the actions a player chose ([Choices](control.md#choices)): `ctx.grant(unit, kind, ids)` puts them after the unit's slots of that kind, at the kind's first rank; an item's actions sit with the item ([Items](items.md)). `levels` loads, and the release does not enforce it until progression ([Progression](progression.md)).

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

- **Projectile:** `speed`, `width`, `range` (the action's by default), `homing`, `gravity`, `stop_on_hit`, `once_per_cast`, `hits` (a filter, or `none`); its script state is its unit type's `[state]` ([Script state](../03-game-scripting.md#script-state)). It flies in the Hit stage, a fixed distance each tick, from the tick after it launches, in the map's metric: on a planar map along the ground plane, at the height it launched from, its direction and a homing one's steps and distance with no height in them; flights run before attacks deliver, and the tick's launches after. A homing one flies at its target's position of that tick, and hits where its step ends once its body, half its width across, reaches the target's body, in that tick's Resolve, as a line's hits do; one whose target dies, despawns, becomes blocked as a target or disjoints before it lands ends without a hit. A teleport, and a forced move longer than an engine constant, disjoint: they end the homing projectiles that target the unit, since the move speed cap does not limit them. Every homing projectile flies faster than the mode's `max_move_speed`, which the load checks, so it always catches its target and the number of live projectiles is bounded. One along a line hits each unit its `hits` selects, `enemies` by default, whose body comes within half its width of its path in the tick, in the order of the nearest point of the path to each, compared exactly, then of stable id; each unit once, and once a cast with `once_per_cast`. It ends at its first hit with `stop_on_hit`, and at its range, which a global range, and every range, cuts at the edge of the map's bounds; a fan aimed at a point ends there at the latest. Projectiles take stable ids in the order of their source's id, so every run numbers them alike, and a cast's projectiles share the id of its first as their cast. A projectile has its source's team and player, its type's tags and the engine's `projectile` tag, and no pools: no attack or effect reaches it. With `hits = "none"` it hits no unit and flies to its end, as Snow Owl flies to its point: the load refuses `none` beside `width`, `stop_on_hit`, `once_per_cast` or `homing`, and for an action with an `on_hit` list or hook, since none of them could apply; `none` is the word `targeting` takes for no target, and Dota 2 says the same with a linear projectile's target team of none.
- **Area:** `radius`, `delay_ms` and `duration_ms` (0 by default), `affects` (a filter, `enemies` by default), `inside = { self, allies, enemies }` (modifiers of its package held on the units inside); its script state is its unit type's `[state]`. A cone or a line is a projectile. It lands on the point its action aimed at, where the unit it aimed at stands, or where its caster stands for an action that aims at nothing, after the tick's projectiles launch. It triggers once, in Hit after flights, its delay after the tick it lands and one tick at the least: it reaches each living unit `affects` selects whose body comes within its radius, a unit whose tags block it as a target among them, by stable id. It ends at its trigger or at the end of its duration, whichever is later. From the tick it lands until it ends, in Resolve, it holds `self` on its caster, `allies` on the caster's other allies and `enemies` on the units that may be attacked, each while its body is within the radius, as an aura holds its modifier ([Stats](stats.md#modifiers)), from its caster and with its action's params at its rank: as a Dota thinker carries an aura. An area has its source's team and player, its type's tags and the engine's `area` tag, and no pools; areas take stable ids after the tick's projectiles, in the order of their source's id.

A projectile or an area type may have a `vision` section, as any unit type does: its unit reveals to its source's group what it sees while it lasts, by the rule every unit follows ([Vision](vision.md#grid-fog-of-war)), as Snow Owl reveals where it flies and Dota 2 gives projectiles and thinkers vision.

Each delivery records how it reached a unit in a **hit**: the projectile or area unit that delivered it, or `()` at once; the unit the action aimed at; where it hit, or where its delivery ended, an area's centre for an area; the distance flown, 0 for an area; the direction, none for an area; and, for a ray or a sweep, the body part. The `on_hit` and `on_end` hooks receive it, and the damage the delivery deals carries it as `d.hit` ([Combat](combat.md#damage-and-heals)).

### Effects

An effect list is an array of effects, each one table; the effects queue in order, as the calls of a script do:

| Effect | Does |
| --- | --- |
| `damage = { amount, kind }` | Damage to the unit reached |
| `heal = { amount }` | A heal of its life pool |
| `restore = { pool, amount }` | Adds to one of its pools |
| `modifier = { id, duration_ms }` | Applies a modifier from the acting unit |
| `purge = { tag }` | Ends the applications of its modifiers that grant the tag, a tag the mode declares and not the engine's, whatever their source: an instance a passive, an aura, an area or a player holds stays, as its holder would apply it again, as Dota 2's dispels remove applied buffs and never passives or auras |
| `spawn = { unit_type }` | Spawns a summon where it stands, on the source's team and of its player. Planned with summons, whose unit types in packages other than the mode, and whose timed life, are not designed yet |
| `launch = { area, on_hit, on_end }` | Lands an area of the action's package where it stands, which runs its own lists |
| `move = { to, speed }`, `move = { from, distance, ms }` | Forced movement: a dash or a knock back ([Navigation](navigation.md#forced-movement)) |
| `xp = { track, amount }` | Experience on a track ([Progression](progression.md)) |
| `loot = { table, level }` | Rolls a random table and drops what it gives where the unit stands ([Random tables](00-overview.md#random-tables)) |
| `noise = { radius }` | A noise that units within the radius hear ([Senses](vision.md#senses)) |

A launched area comes from the acting unit, with the action's params at its rank, as the action's own area does, and lands as an area that `ctx.area` queues at the same place does. Its `on_hit` list runs on each unit it reaches and its `on_end` list at its end; it runs no hook, as the action's hooks are for the action's own delivery. A list it holds may launch again, so a chain in data is as deep as its nesting and ends there, as StarCraft II's effects chain by name. A projectile has no launch effect: data knows no aim for it, and a script aims one with `ctx.projectile`, as Cinder's Chain Fire picks its next target.

Every effect takes `to = "source"` to apply to the acting unit instead; a list that reaches no unit, `on_end` or the `on_resolve` of an action that aims at no unit, holds only such effects. Numbers take `{ param = "<name>" }`, and are not negative at any rank; a scaling param that its source's stats take below zero counts as zero. A modifier's `duration_ms` is whole milliseconds. An effect list runs before the script hook of the same name, in one call with it: the list's effects queue first, then the hook's, and a hook that fails applies neither, as a cast that fails spends nothing; a list whose hook the script does not define applies alone. So a hook adds only what data cannot say. The release runs the lists of a cast and a weapon's `on_hit` ([Weapons](combat.md#weapons)), and the effects the [reference](../08-script-api-reference.md) marks as running; the load refuses a planned effect. The action pipeline holds the lists, and each capability registers how its own effects queue, so a mode of weapons alone, with no `abilities`, runs them too. One example, Rime's Fan of Frost, as her package holds it:

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

An order names an action and a target ([Orders](control.md#orders)); a button of a `character` input frame names a slot, aimed where the character looks ([Character](control.md#character)); an AI orders as a player does. So one action serves a MOBA hero, a first-person character and an RTS unit.

## State and derived

- **State:** each unit's slots (the action, its rank, the tick it is ready, its charges), its action in progress (the slot, the target, the tick it delivers), its toggles and channels; each projectile's and area's unit.
- **Derived:** the values at each rank, from data, when the package loads; a scaling param from the source's stats when it is read.

## Script API

`ctx.p`, `ctx.range`, `ctx.charge`, `ctx.origin`; `ctx.projectile(from, direction)` for a line type and `(from, unit)` for a homing type, the other form failing the call, `ctx.area(pos)`, each returning the new unit ([New units at once](../08-script-api.md#rules)); `ctx.reduce_cooldown(unit, id, ms)`, `ctx.reduce_cooldowns(unit, slot_kind, fraction)`, `ctx.add_charge(unit, id)`, `ctx.learn(unit, slot)`; the hooks `on_resolve(ctx, unit, target)`, `on_hit(ctx, unit, target, hit)`, `on_end(ctx, unit, hit)`, `on_channel_tick(ctx, unit)` and `on_interrupt(ctx, unit, target)`. Projectiles and areas are unit handles, with the fields of their sections.

## Network

Cooldowns and charges go to the owner, or the team, as the mode sets. Projectiles and areas replicate as units, to the clients whose group sees them. A client predicts the start of its own actions; their effects appear when the server confirms them.

## Cost

Each action in progress costs its checks when it starts and again when it delivers. Each live projectile costs a step and a hit test a tick; each area a query of its radius when it triggers and a tick while it holds modifiers. A ray costs a test against the hitboxes near its line at the tick it names.

## Genres

A MOBA's attacks, abilities, item actives and player spells; a shooter's guns (attack actions with a ray delivery), grenades (projectiles) and the bomb (a use); an RTS's weapons, abilities and production; an MMO's spells, auto attack and quest dialogue; a battle royale's weapons, consumables and the zone (an area).
