# Campfire — Capabilities

The engine has no genres. It has **capabilities**: each is one mechanism, such as health and damage, units that take orders, a first-person character or grid fog of war. A game package declares the capabilities it uses, and its mode scripts and data make the genre. A MOBA, an RTS, an FPS, an MMO, a battle royale and a game that mixes them are the same kind of package: the reference MOBA is only the first.

A capability is native code: components, systems that run in the tick, backends, a data schema for unit types, commands, and the script calls and hooks it adds. Capabilities follow the core's determinism rules and ship in engine releases; packages cannot add native code, because the verifier must run only code the release pins. A new combination of capabilities needs no release; a new mechanism does.

## The model

Every capability says its mechanism in the same few terms, so that capabilities meet in one match and a genre is only data and scripts. Each capability's doc gives the reasons and sources for the form its terms take.

- **Unit.** Anything in the sim with a stable id, a position and a type: a hero, a soldier, a building, a creep, a projectile, an area, an item on the ground, a door, a resource node, a vehicle. The sections of its type decide what it is and what it does. Scripts see every unit through one handle, which has the fields of its type's sections.
- **Tag.** A unit's tags are its type's tags and the tags its modifiers grant. A tag has properties only when the mode gives it some ([Tags](stats.md#tags)): it can block an action kind, moving, being a target or taking damage; hide the unit or detect hidden units; or make the unit immune to the modifiers that grant other tags. The engine tags a unit by some of its sections: `avatar`, `projectile`, `area`, `item`, and the name of the layer it moves on.
- **Stat.** A number the mode declares. A unit's value of a stat is its type's value at its level, changed by its modifiers through one formula ([Stats](stats.md#stats)).
- **Pool.** An amount between 0 and the value of a stat, which a regen stat refills: health, mana, energy, rage, ammunition. The mode declares its pools; a unit type lists those it has ([Pools](stats.md#pools)).
- **Modifier.** An instance on a unit, from a source, for a time: it changes stats, grants tags, runs effects at intervals, and hears the unit's events ([Modifiers](stats.md#modifiers)).
- **Action.** Anything a unit does on purpose: an attack, a cast, a shot, the use of an item or of an object, a build, a train or a gather order. Every action runs one pipeline: checks, time, delivery, effects and cost ([Actions](actions.md)).
- **Effect.** A change the sim applies: damage, a heal, a pool restored, a modifier added or purged, a unit spawned, a projectile or an area launched, a unit moved. Data lists effects; scripts queue the same effects through `ctx`.
- **Event.** What happened in a tick: an action resolved, a delivery hit, damage was taken, a unit died. The units it concerns hear it through their modifiers, and the mode through its hooks.
- **Relation.** How two teams regard each other: `hostile`, `neutral` or `friendly`, and whether they share vision. A team is friendly to itself ([Relations](#relations)).
- **Space.** The map's metric, `planar` or `spatial`, and the layers bodies move on ([Space and map](#space-and-map)).

## Capabilities

| Capability | Status | Adds | Doc |
| --- | --- | --- | --- |
| `combat` | built | The life pool, weapons, the damage and heal pass, deaths, kill credit, respawns | [Combat](combat.md) |
| `stats` | built | Stats and their formula, pools, modifiers, tags and their properties, levels | [Stats](stats.md) |
| `abilities` | built | Cast actions: ranks, charges, toggles, channels, charged casts | [Actions](actions.md#kinds) |
| `projectiles`, `areas` | built | Deliveries: projectiles that fly a line, home or fall; areas that hold modifiers on the units inside | [Actions](actions.md#deliveries) |
| `orders` | built | Units that take orders: move, attack, an action, stop, hold, queues, groups and formations; AI `on_think` | [Control](control.md#orders) |
| `character` | planned | Units a player drives directly: per-tick input frames, capsule controller | [Control](control.md#character) |
| `hitboxes` | planned | The ray and sweep deliveries: shots and swings against hitboxes, lag compensation, spread | [Hitboxes](hitboxes.md) |
| `navigation` | built | Layers, routes on a grid or a navmesh, local steering, waypoint paths | [Navigation](navigation.md) |
| `vision` | built | What each vision group sees: grid fog of war, hidden units and detection, 3D occlusion, relevance | [Vision](vision.md) |
| `items` | built | Inventories, item types, recipes and shops; an item grants modifiers and an action; later equipment, world items and crafting | [Items](items.md) |
| `progression` | built | Experience on tracks, levels, points to learn ranks, perks, veterancy | [Progression](progression.md) |
| `quests` | planned | Quests with stages and objectives, dialogue with topics and choices, campaign objectives | [Quests](quests.md) |
| `interaction` | planned | The use action on objects: doors, containers, plant and defuse, capture points, dialogue, entering vehicles and buildings | [Interaction](interaction.md) |
| `production` | built | The train, build and gather actions, construction on the grid, tech | [Production](production.md) |
| `physics` | planned | Vehicles, rigid bodies, heightmap terrain | [Physics](physics.md) |
| `world` | planned | Large worlds: regions that sleep, parallel regions, streaming | [World](world.md) |

A capability is built when the release installs it, and planned when it does not yet; a built one's doc can still name parts that are planned, which the [script API reference](../08-script-api-reference.md) marks name by name. A test fails when this column and the release differ.

Which capabilities make which genre: [Genres](genres.md).

## Mode vocabulary

The core has no genre words and no genre lists. A mode declares, in its data, every name its packages use:

- **Damage kinds:** physical and magic for a MOBA; bullet and explosive for a shooter; fire, frost and shadow for an MMO.
- **Stats** and **pools:** the numbers its units have, and the amounts they spend. The engine reads only the stats its mechanisms need, and binds the rest by the mode's names.
- **Tags** and their effects: a MOBA's `stunned` and `stealthed`, an RTS's `cloaked` and `detector`, a shooter's `flashed`.
- **Slot kinds:** where a unit's actions sit: a MOBA's `basic` and `ultimate`, a shooter's `primary` and `secondary`, an RTS's command card.
- **Teams** and their **relations**, and the **choices** players make before they spawn.
- **Player resources:** gold, minerals, supply.
- **Tracks** of experience: a MOBA's one level, Skyrim's skills.
- **Random tables:** a battle royale's loot, Diablo's drops, Skyrim's leveled lists.

Neutral core terms: a player's **avatar** (a unit type players choose: a hero, a soldier, a class), a **loadout** (actions a player chooses before spawning), a **spawn group** (a wave, a squad), a **path** (a lane, a patrol route), a **marker** (a named place on the map). A package names them in its own words.

## Relations

A team is an index in the mode's list of teams, up to 256 long, so a battle royale of 100 solo players has 100 teams; a set of teams is then a fixed bitset a rollback copies without an allocation. The mode declares how pairs of teams regard each other, as Unreal's [team attitudes](https://dev.epicgames.com/documentation/unreal-engine/API/Runtime/AIModule/FGenericTeamId) do:

```toml
[[relations]]
teams = ["alliance", "horde"]
relation = "hostile"
```

A pair not named is `hostile`, and a team is `friendly` to itself. `ctx.set_relation(a, b, relation)` changes a pair, `relation` a member of `Relation`, such as `Relation::Hostile` ([Engine enums](../08-script-api.md#engine-enums)), and the relations are state; the server sends them to every client in each tick a script changes them. Filters read them ([Filters](../08-script-api.md#filters)). A neutral unit may be attacked, but does not seek a fight: an AI chooses its targets with `hostiles`, so a neutral monster fights back only when a script makes its team hostile or orders it, as a neutral monster does in WoW. Two friendly teams share vision unless their relation says `vision = false`, as StarCraft's allies choose whether to share it; teams that share vision form a **vision group**, which sees as one. A MOBA's camps are on one more team, hostile to every other.

## Space and map

- **Metric.** A map's `metric` is `planar` or `spatial`. `planar` measures ranges, reach and sight on the ground plane, with the height from the map's terrain; MOBAs and RTS games use it. `spatial` measures in 3D; shooters and flight use it. Every distance of every capability uses the map's metric, but for the cells of the vision grid, which lie on the ground plane on either metric ([Vision](vision.md#grid-fog-of-war)).
- **Bodies.** A unit's body is a circle of its radius on the ground plane, or a **box**: a parallelogram on the ground plane around its position, for a unit type that does not walk, on a `planar` map, as a building's footprint is ([RTS skirmish](../11-rts-skirmish.md#decisions), D1). Its type gives its size, `box = [w, h]` in meters, each at least 2⁻¹⁰ m, so its rounded half edges never lie flat, and its spawn gives an angle in degrees, a `Num` taken modulo 360: a placed unit's `angle`, a build order's, or 0. As it spawns, its two half edges are `(w/2, 0)` and `(0, h/2)` turned by the angle: a multiple of 90° turns them exactly, and any other angle by the sine and cosine math's `sin_cos` gives. Each component is the exact product of a half size and a sine or a cosine, rounded once to a `Num`, to nearest, ties to even, as a `Num` product rounds. The rounded half edges are the box, kept with the body as state: its corners are its position plus and minus each, exactly, and every test reads them as they are, so the one rounding is the shape's and no test rounds again, as 0 A.D. keeps a structure as its centre and two half-edge vectors. Half its diagonal, from its unrounded size, is at most 63 m, so its reach from its position, the longer of the rounded half edges' sum and difference, stays within the widest body's 64 m whatever the rounding, and every bound a circle keeps holds for a box.
- **Reach.** One rule decides every reach: a range from the edge of one body reaches the edge of another, in the map's metric, exactly; a point is a body of radius 0. Between a circle and a box the gap is the distance from the circle's centre to the box's nearest point, less the radius, and 0 when the centre lies inside; between two boxes, the least distance between their points, 0 when they overlap. Each is decided squared, in wide integers, with no root: a point's distance to an edge as a path's approach to a point is, and an overlap by the signs of cross products. Where a path comes nearest a box along a stretch, as one through it does, its nearest point is the first of them along the path. A weapon's and an action's range, an area's and an aura's radius from their centre, a line projectile's half width and a homing projectile's body, and the script queries all follow it, so a script's area of a radius hits what an area of that radius hits. Dota 2 adds the target's bound radius to a ground-targeted area and to an aura, and League of Legends measures attacks and missiles from edge to edge of the units' gameplay radii.
- **Layers.** The mode declares the layers bodies move on, such as `ground` and `air`; the first is the default, and a mode that names none has one, with no name. A unit moves on the layer of its type's `collision`, or on the first. Collision and pathing work within a layer: an RTS's air units pass over ground units and walls. A shooter has one layer. The engine tags each unit with the name of its layer, so the tag limit bounds the layers too.
- **Bounds.** A closed rectangle of the ground plane that no unit is ever outside: move orders clamp to it, the core clamps every unit that moved after Move and Collide, and a spawn outside it fails. On a spatial map, a height has only the world's bound, 2²⁰ m.
- **Map.** A map holds its bounds, its terrain, grid or level geometry, its paths, the units placed at its start, and **markers**: named points and regions, a region a box from `min` to `max`, each with tags, an optional team and params, which scripts read by tag (`ctx.map.markers("spawn")`). A MOBA's team spawns and camps, a shooter's bomb sites and buy zones, an RTS's start locations and resource fields, a battle royale's loot spots and an MMO's zones are markers.
- **Region events.** A region marker with `events = true` tells the mode when a unit enters or leaves it: `on_enter(ctx, marker, unit)` and `on_leave(ctx, marker, unit)`, in the Mode stage, by marker name, then by the unit's stable id. A campaign's triggers, an ambush, a capture zone and a cutscene's start are region events. Only the units that moved test against the event markers, through the same buckets collision uses.
- **Generated maps.** A map may be built, wholly or in part, by the mode's `on_generate(ctx, region)` hook: at the session's start for the whole map, and again when a script asks for a region with `ctx.generate(region)`, as Diablo builds a dungeon level when a player first goes down to it. The hook writes only within its region: blocked cells, markers, paths and placed units; it draws from the secret stream, so the seed decides the map; and the map checks run on what it built, as on a map file. A generated map is state, so a save holds it.

## Game clock

Game time is sim state: the hour, the day and the date of the mode's calendar, which advance with the ticks at the mode's `[clock] time_scale` (Skyrim's is 20 game seconds a real second) from its `[clock] start`. `ctx.time` reads it. `ctx.advance_time(ms)` moves it on without running ticks, for waiting, sleeping and fast travel; the durations of modifiers, timers and cooldowns count ticks and do not move, while what a script schedules by game time, an NPC's day or a shop's restock, sees the new time. A world's regions place their sleeping units by it when they wake ([World](world.md)). The clock is not the wall clock: the same log gives the same time everywhere.

## Random tables

A package's `[tables.<id>]` lists weighted entries, each a unit type, an item type or another table, with conditions such as a level range: Skyrim's leveled lists, Diablo's drops and a battle royale's loot. A `spawn` or `loot` effect, and `on_generate`, roll a table on the secret stream, with the level they name, so the seed decides every roll.

## Layers of capabilities

The dependencies form a fixed graph with no cycle:

- **Base:** `sim` (positions, stable ids, randomness, the state hash, the tick rate) and collision; then the core under every script: unit types, tags, relations, the map and its markers, the action pipeline, the effect queue, the one script host and its tick budget, and the units as scripts see them. A capability adds its fields to that view, so the core names no capability.
- **`combat` and `stats`:** pools, damage, deaths, stats, modifiers, tag properties.
- **Everything else** builds on those: action kinds and deliveries (`abilities`, `projectiles`, `areas`, `hitboxes`, `interaction`, `production`, `items`), who starts actions (`orders`, `character`), and the rest.

The capabilities are modules of one crate; a capability with a heavy dependency, such as physics, gets its own crate. Only the declared capabilities' systems run, so an unused one costs nothing.

## Tick stages

The engine fixes the stages of a tick, and each capability puts its systems into them. Within a stage, a capability orders its systems against those of the capabilities it builds on: in Act, `orders` chases a target before `combat` starts the attack. Two systems with no order and conflicting access fail the schedule build, so no order is left to chance.

| # | Stage | Runs |
| --- | --- | --- |
| 1 | Inputs | Core: the tick's commands reach the capabilities that own them; modifiers end |
| 2 | Think | AI `on_think` of the units due this tick issues orders |
| 3 | Act | Orders and input frames start actions: checks, windups and cast times; path requests. Each capability starts the actions of its kind: first combat the attacks, then abilities the casts that orders asked for, last production the trains, the builds and the gather loops, each in the order of its units' stable ids, as they pay at once |
| 4 | Move | Steering, the character controller, forced movement |
| 5 | Collide | Core: the mode's collision backend resolves overlaps within each layer |
| 6 | Hit | Actions whose time ended deliver: strikes, rays, projectiles, areas; actions resolve with their effects; modifier intervals |
| 7 | Resolve | The damage and heal pass; the areas its weapons' `on_hit` lists launched land; deaths, auras |
| 8 | Mode | Due timers, then the capabilities' events in the order they happened, region events among them; spawns; generated regions; `ctx.end` and `ctx.save` |
| 9 | Vision | `vision` marks what each vision group sees. The tick's end: the dead whose type despawns go, and the units' action passives follow the ranks the Mode stage's calls learned |

Before the first stage and after each, every unit whose level, modifiers or type changed has its stats and tags derived again ([Stats](stats.md#state-and-derived)).

## Commands

A player input's payload is a list of commands. Each command names the capability that owns it by the capability's index in the engine's fixed list, one byte, and holds that capability's format: an order, a `character` input frame. A mode input goes to `mode`, the owner every match has and no manifest declares. The list only grows: a new capability takes the next index, so an old log decodes the same. One packet can carry a first-person frame and an order to a squad. A command of a capability the mode did not declare, or one that does not decode, is ignored: a client can send anything.

## Control

Control is a relation from a player slot to units, of one of two kinds:

- **Direct:** the player drives the unit with `character` input frames, and the client predicts it.
- **Orders:** the unit follows the player's orders (`orders`); an order names the units it goes to, so one player can order many.

A player controls any number of units, or none: an RTS player an army, an MMO player one character, a first-person commander a character and its squads. Both kinds start the same actions. AI scripts issue the same orders; bots send the same commands as players.

## Unit types

A unit type in data is a set of sections, one for each capability it uses, in the same schema wherever it is written: a mode's `data/units.toml`, or an avatar package's one unit type.

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

The team and the owner come from the spawn, not the unit type. A section of a capability the mode did not declare fails the package load. Time is in milliseconds and rates are per second; they become whole ticks when the package loads, rounded up, so a unit type behaves the same at any tick rate to within one tick.

## Script API

`ctx`, the handles and the hooks are made of the declared capabilities' parts: a mode without `combat` has no `ctx.damage` and no `on_unit_died`. The package load checks refuse a call, a field or a hook of a capability the mode did not declare. How hooks are named, and how an event's data list and its hook share a name, is [Script API](../08-script-api.md#rules)'s; the [reference](../08-script-api-reference.md) lists each capability's part.

## Tick rate

The mode picks its tick rate within the range its manifest allows: an FPS wants 64 to 128 Hz, a MOBA 30, an MMO or a large RTS 10 to 20. Every capability works in ticks at any rate, and each states what its worst tick costs, so a mode can see what its combination costs before it ships.

## Capability docs

Each capability doc has the same sections, in this order, and leaves out one it has nothing for:

1. **Mechanism:** what it does, in the model's terms.
2. **Data:** its sections of a unit type and of the mode.
3. **Rules:** how it runs, in which tick stage.
4. **State and derived:** what is hashed, what is computed again.
5. **Script API:** the calls, fields and hooks it adds.
6. **Network:** what goes to which clients, and what a client predicts.
7. **Cost:** the worst tick, in the units and players it counts.
8. **Genres:** what each target game takes from it.

## Scripted systems

A mechanism no capability has needs an engine release, but a package can add one in Rhai as a **scripted system**: a script the mode declares that runs each interval in a stage it names, over the units its filter selects, under a budget. A shield that drains in a zone, a heat meter for weapons or a capture rule are scripted systems, as Dota 2's custom games add rules with thinkers in Lua, and the verifier still runs only the code the release pins.

```toml
[systems.overheat]
script = "scripts/overheat.rhai"
stage = "resolve"        # act, hit, resolve or mode
affects = "all:gun"      # a filter
interval_ms = 100
```

- **One call an interval:** `on_system(ctx, units)` runs once, with the units its filter selects, by stable id, after the capabilities' systems of its stage. One call over a list keeps Rhai's cost of a call to one an interval, not one a unit.
- **The filter is kept, not searched:** the units a filter selects are kept as their tags change, so a call costs the units it gets, not the units in the match.
- **State and effects** are those of every script: the units' and the mode's declared script state, and effects queued through `ctx`, which apply when the call ends; the call is all or nothing.
- **Budget:** the mode's `systems` pool in `script_limits`. A system whose call finds the pool spent stays due and runs first in the next tick, as AI does, so a system runs late under load but never skips a turn.
- **Order:** systems of one stage run in the order of their names.
- **Cost** is Rhai's, which is far slower than native code: a system suits rules over tens or hundreds of units an interval, not per-tick physics over thousands. A mechanism that needs more is a capability.

## Testing combinations

Every pair of capabilities a mode may declare together meets in at least one test mode with a golden: the reference MOBA, the genre proofs, and as many small mixed modes as the pairs need, as all-pairs testing covers every two-way combination of options with few cases. A test lists the pairs the test modes declare and fails when a pair is missing, so a new capability brings its pairs with it. Interactions of three or more capabilities are tested where a mode needs them.
