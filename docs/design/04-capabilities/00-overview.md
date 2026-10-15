# Campfire — Capabilities

The engine has no genres. It has **capabilities**, each one mechanism: health and damage, units that take orders, a first-person character, grid fog of war. A package declares the capabilities it uses, and its scripts and data make the genre, so a MOBA, an RTS, an FPS, an MMO, a battle royale and a mix are the same kind of package.

A capability is native code: components, systems in the tick, backends, a data schema, commands, and the script calls and hooks it adds. Capabilities follow the core's determinism rules and ship in engine releases; packages add no native code, as the verifier runs only code the release pins. A new combination needs no release; a new mechanism does.

## The model

Every capability says its mechanism in the same terms, so capabilities meet in one match and a genre is only data and scripts. Each capability's doc gives the reasons and sources for its form.

- **Unit.** Anything in the sim with a stable id, a position and a type: a hero, a building, a creep, a projectile, an area, an item on the ground, a door, a node, a vehicle. Its type's sections decide what it is and does; scripts see every unit through one handle.
- **Tag.** A unit's tags are its type's, the engine's (`avatar`, `projectile`, `area`, `item`, its layer's name and the rest), and those its modifiers grant. A tag has properties only when the mode gives them ([Tags](stats.md#tags)): it blocks an action group, moving, being a target or taking damage; hides or detects; or makes the unit immune to modifiers that grant other tags.
- **Stat.** A number the mode declares; a unit's value is its type's at its level, changed by its modifiers through one formula ([Stats](stats.md#stats)).
- **Pool.** An amount from 0 to a stat's value, refilled by a regen stat: health, mana, energy, ammunition ([Pools](stats.md#pools)).
- **Modifier.** An instance on a unit, from a source, for a time: it changes stats, grants tags, runs effects at intervals and hears the unit's events ([Modifiers](stats.md#modifiers)).
- **Action.** Anything a unit does on purpose; every action runs one pipeline: checks, time, delivery, effects and cost ([Actions](actions.md)).
- **Effect.** A change the sim applies: damage, a heal, a pool restored, a modifier added or purged, a unit spawned, a delivery launched, a unit moved. Data lists effects; scripts queue the same ones through `ctx`.
- **Event.** What happened in a tick: an action resolved, a delivery hit, damage was taken, a unit died. The units it concerns hear it through their modifiers, and the mode through its hooks.
- **Relation.** How two teams regard each other, and whether they share vision ([Relations](#relations)).
- **Space.** The map's metric and the layers bodies move on ([Space and map](#space-and-map)).

## Capabilities

| Capability | Status | Adds | Doc |
| --- | --- | --- | --- |
| `combat` | built | The life pool, weapons, the damage and heal pass, deaths, kill credit, respawns | [Combat](combat.md) |
| `stats` | built | Stats and their formula, pools, modifiers, tags and their properties, levels | [Stats](stats.md) |
| `abilities` | built | Cast actions: ranks, charges, toggles, channels, charged casts | [Actions](actions.md#kinds) |
| `projectiles`, `areas` | built | Deliveries: projectiles that fly a line or home, and fall (planned); areas that hold modifiers on the units inside | [Actions](actions.md#deliveries) |
| `orders` | built | Units that take orders: move, attack, an action, stop, groups; AI `on_think` | [Control](control.md#orders) |
| `character` | planned | Units a player drives directly: per-tick input frames, capsule controller | [Control](control.md#character) |
| `hitboxes` | planned | Ray and sweep deliveries against hitboxes, lag compensation, spread | [Hitboxes](hitboxes.md) |
| `navigation` | built | Layers, routes, local steering, waypoint paths, forced movement, collision | [Navigation](navigation.md) |
| `vision` | built | What each vision group sees: grid fog, hidden units and detection; later 3D occlusion and relevance | [Vision](vision.md) |
| `items` | built | Inventories, item types, recipes and shops; later equipment, world items and crafting | [Items](items.md) |
| `progression` | built | Experience on tracks, levels, points to learn ranks; later perks | [Progression](progression.md) |
| `quests` | planned | Quests and objectives, dialogue, campaign objectives | [Quests](quests.md) |
| `interaction` | built | The `use` action on objects; later the `enter` action | [Interaction](interaction.md) |
| `production` | built | Train, build and gather actions, construction, supply, tech | [Production](production.md) |
| `physics` | planned | Vehicles and rigid bodies against the terrain | [Physics](physics.md) |
| `world` | planned | Large worlds: dormant regions, parallel regions, streaming | [World](world.md) |

A capability is built when the release installs it, and a test fails when this column and the release differ; a built one's doc can still name planned parts, which the [script API reference](../08-script-api-reference.md) marks name by name. Which capabilities each imported game takes: [Games](games.md).

## Mode vocabulary

The core has no genre words and no genre lists. A mode declares every name its packages use: **damage kinds** (physical and magic; bullet and explosive); **stats** and **pools**, of which the engine reads only those its mechanisms need and binds the rest by the mode's names; **tags** and their effects (`stunned`, `cloaked`, `flashed`); **slot kinds**, where a unit's actions sit (`basic` and `ultimate`, `primary` and `secondary`, a command card); **teams**, their **relations**, and the **choices** players make before they spawn; **player resources** (gold, minerals); **tracks** of experience; **random tables**.

Neutral core terms, which a package names in its own words: a player's **avatar** (a unit type players choose), a **loadout** (actions players choose), a **spawn group** (a wave, a squad), a **path** (a lane, a patrol route), a **marker** (a named place on the map).

## Relations

A team is an index in the mode's list of up to 256 teams, so a set of teams is a fixed bitset a rollback copies without allocating. The mode declares how pairs regard each other, as Unreal's [team attitudes](https://dev.epicgames.com/documentation/unreal-engine/API/Runtime/AIModule/FGenericTeamId) do:

```toml
[[relations]]
teams = ["alliance", "horde"]
relation = "hostile"
```

A pair not named is `hostile`, and a team is `friendly` to itself. `ctx.set_relation(a, b, relation)` changes a pair, `relation` a member of `Relation` ([Engine enums](../08-script-api.md#engine-enums)). The relations are state, which reaches every client as the match entity's ([Replication audiences](../14-replication-audiences.md), D7); filters read them ([Filters](../08-script-api.md#filters)). A neutral unit may be attacked but seeks no fight: an AI chooses targets with `hostiles`, so a neutral monster fights back only when a script makes its team hostile or orders it. Friendly teams share vision unless their relation says `vision = false`; teams that share vision form a **vision group**. A MOBA's camps are one more team, hostile to every other.

## Space and map

- **Metric.** A map's `metric` is `planar`, which measures on the ground plane with heights from the terrain (MOBAs, RTS games), or `spatial`, which measures in 3D (shooters, flight). Every distance uses it, but the vision grid's cells, which lie on the ground plane on either metric.
- **Bodies.** A body is a circle of its radius on the ground plane, or, for a type that does not walk on a `planar` map, a **box**, as a building's footprint is ([RTS foundations](../11-rts-foundations.md#decisions), D1). Its type gives `box = [w, h]` in meters, each at least 2⁻¹⁰ m, and its spawn an angle in degrees, taken modulo 360. As it spawns, its half edges `(w/2, 0)` and `(0, h/2)` turn by the angle, exactly for a multiple of 90° and else by `sin_cos`, and each component rounds once to a `Num`, to nearest, ties to even. The rounded half edges are the box, kept as state: every test reads them exactly, as 0 A.D. keeps a structure as a center and two half-edge vectors. Half its diagonal is at most 2,047 m, so its reach stays within the widest body's 2,048 m ([Wide bodies](../13-bodies-uses-shop-gravity.md#wide-bodies)).
- **Facing.** Every unit faces its spawn's angle, in degrees from `x` towards `−z`, counter-clockwise from above, as state the client draws by. Nothing turns a unit yet.
- **Height.** A `planar` map keeps a position's height above the ground for its client; a placed unit's `height` gives it. A `spatial` map's points hold their own height.
- **Reach.** One rule decides every reach: a range from the edge of one body reaches the edge of another, in the map's metric, exactly; a point is a body of radius 0. Between a circle and a box the gap is the distance from the center to the box's nearest point less the radius, 0 inside; between two boxes, their least distance, 0 when they overlap. Each is decided squared, in wide integers, with no root; an overlap by the signs of cross products. Where a path comes nearest a box along a stretch, its nearest point is the first along the path. Weapons' and actions' ranges, areas' and auras' radii, a line projectile's half width, a homing projectile's body and the script queries all follow it, as League of Legends measures from edge to edge.
- **Layers.** The mode declares the layers bodies move on, `[navigation] layers`, the first the default; a mode that names none has one. Collision and pathing work within a layer, so an RTS's air units pass over ground units. The engine tags each unit with its layer's name.
- **Bounds.** A closed rectangle of the ground plane no unit is ever outside: move orders clamp to it, the core clamps every unit that moved after Move and Collide, and a spawn outside it fails. A height has only the world's bound, 2²⁰ m.
- **Map.** A map holds its bounds, its terrain, a heightmap, its grid or level geometry, its paths, the units placed at its start, and **markers**: named points and regions (a box from `min` to `max`), each with tags, an optional team and params, which scripts read by tag (`ctx.map.markers("spawn")`): spawns, camps, bomb sites, start locations, loot spots, zones.
- **Region events** (planned). A region marker with `events = true` will run the mode's `on_enter(ctx, marker, unit)` and `on_leave(ctx, marker, unit)` in the Mode stage, by marker name, then by stable id. Only units that moved will test against them, through collision's buckets.
- **Generated maps.** The mode's `on_generate(ctx, region)` builds a map, wholly or in part: at the start for the whole map, and again for a region a script asks for with `ctx.generate(region)`, as Diablo builds a dungeon level. It writes only within its region, draws from the secret stream so the seed decides the map, and the map checks run on what it built. A generated map is state.

## Game clock

Game time is sim state: the hour, day and date of the mode's calendar, which advance with the ticks at `[clock] time_scale` from `[clock] start`; `ctx.time` reads it. `ctx.advance_time(ms)` moves it on without ticks, for sleeping and fast travel: durations of modifiers, timers and cooldowns count ticks and do not move, while what a script schedules by game time sees the new time. The clock is not the wall clock.

## Random tables

A package's `[tables.<id>]` lists weighted entries, each a unit type, an item type or another table, with conditions such as a level range. A `spawn` or `loot` effect, and `on_generate`, roll a table on the secret stream.

## Layers of capabilities

The dependencies form a fixed graph with no cycle. Under every capability lie `sim` (positions, stable ids, randomness, the state hash, the tick rate) and the core under every script: unit types, tags, relations, the map, the action pipeline, the effect queue, the script host and its budget, and the script view, to which each capability adds its fields, so the core names none.

Each capability is a crate, which depends only on the crates of lower layers; its dependency on another capability is a need, without which it does not run and which a mode that declares it declares, or a use, which it reads when the match has it. Design 02's table of the capabilities' crates gives each crate's layer, needs and uses ([The capabilities' crates](../02-engine-core.md#the-capabilities-crates)); the modules' declarations hold them in code, which release tests hold equal to the crates' manifests. Only the declared capabilities' systems run, and a package's effect of a capability its mode does not declare fails its load.

## Tick stages

The engine fixes the stages, and each capability puts its systems into them, ordered against those of the capabilities it builds on; two systems with no order and conflicting access fail the schedule build.

| # | Stage | Runs |
| --- | --- | --- |
| 1 | Inputs | The tick's commands reach their capabilities; modifiers end |
| 2 | Think | AI `on_think` of the units due this tick |
| 3 | Act | Orders and input frames start actions: checks, windups, path requests. First combat's attacks, then abilities' casts, last production's trains, builds and gather loops, each by its units' stable ids, as they pay at once |
| 4 | Move | Steering, the character controller, forced movement |
| 5 | Collide | The collision backend resolves overlaps within each layer |
| 6 | Hit | Actions whose time ended deliver; their effects; modifier intervals |
| 7 | Resolve | The damage and heal pass; the areas weapons' `on_hit` launched land; deaths, auras |
| 8 | Mode | Due timers, then the capabilities' events in the order they happened; spawns; generated regions; `ctx.end` and `ctx.save` |
| 9 | Vision | What each vision group sees. Then the dead whose type despawns go, and action passives follow the ranks the Mode stage learned |

Before the first stage and after each, every unit whose level, modifiers or type changed has its stats and tags derived again ([Stats](stats.md#state-and-derived)).

## Commands

A player input's payload is a list of commands. Each names its capability by its index in the engine's fixed list, one byte, and holds that capability's format: an order, an input frame. A mode input goes to `mode`, which every match has and no manifest declares. The list only grows, so an old log decodes the same. A command of an undeclared capability, or one that does not decode, is ignored: a client can send anything.

## Control

Control is a relation from a player slot to units: **direct**, by `character` input frames, which the client predicts; or **orders**, which name the units they go to, so one player can order many. A player controls any number of units, or none, and both kinds start the same actions. AI scripts issue the same orders, and bots send the same commands as players.

## Unit types

A unit type is a set of sections, one for each capability it uses, in one schema wherever it is written: a mode's `data/units.toml`, or an avatar package's one unit type.

```toml
[units.siege_tank]
tags = ["vehicle", "mechanical"]
pools = ["health"]
stats = { health = { base = 900 }, attack_damage = { base = 60 }, attack_speed = { base = "0.4" }, move_speed = { base = "3.0" } }
slots = { weapon = ["tank_cannon"], command = ["siege_mode"] }
collision = { radius = "1.2", layer = "ground" }
orders = { ai = "scripts/tank_ai.rhai", think_ms = 250 }
vision = { sight_range = "11" }
```

The team and the owner come from the spawn; the owner is a player of the unit's team, and a change of owner inserts a new one, which moves the unit to the new owner's team ([Replication audiences](../14-replication-audiences.md), D17, D18). A section of an undeclared capability fails the load. Times are in milliseconds and rates per second; they become whole ticks at load, rounded up, so a unit type behaves the same at any tick rate to within one tick.

## Script API

`ctx`, the handles and the hooks are made of the declared capabilities' parts: a mode without `combat` has no `ctx.damage`. The load refuses a call, field or hook of an undeclared capability. Hook naming is [Script API](../08-script-api.md#rules)'s; the [reference](../08-script-api-reference.md) lists each capability's part.

## Tick rate

The mode picks its rate within its manifest's range: an FPS 64 to 128 Hz, a MOBA 30, Zero Hour 30, an MMO 10 to 20. Every capability works in ticks at any rate, and states its worst tick's cost.

## Capability docs

Each capability doc has these sections, in this order, leaving out one it has nothing for: **Mechanism**; **Data**, its sections of a unit type and of the mode; **Rules**, how it runs and in which stage; **State and derived**; **Script API**; **Network**, who receives what and what a client predicts; **Cost**, the worst tick; **Genres**.

## Scripted systems

A package can add a mechanism in Rhai as a **scripted system**: a script the mode declares that runs each interval in a stage, over the units its filter selects, under a budget, as Dota 2's custom games add thinkers in Lua.

```toml
[systems.overheat]
script = "scripts/overheat.rhai"
stage = "resolve"        # act, hit, resolve or mode
affects = "all:gun"      # a filter
interval_ms = 100
```

- **One call an interval:** `on_system(ctx, units)` runs once with its units by stable id, after the capabilities' systems of its stage, so Rhai's call cost is one an interval.
- **The filter is kept, not searched:** its units are kept as their tags change, so a call costs its units, not the match's.
- **State and effects** are every script's: declared script state and effects queued through `ctx`; the call is all or nothing.
- **Budget:** the `systems` pool; a system whose pool is spent stays due and runs first in the next tick, late but never skipped.
- **Order:** systems of one stage run by name.
- **Cost** is Rhai's: rules over tens or hundreds of units an interval, not physics over thousands.

## Testing combinations

Every pair of capabilities a mode may declare together meets in at least one test mode with a golden, as all-pairs testing covers every two-way combination with few cases. A test lists the pairs the test modes declare and fails when one is missing.
