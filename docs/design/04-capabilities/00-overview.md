# Campfire — Capabilities

The engine has no genres. It has **capabilities**, each one mechanism; a package declares the ones it uses, and its scripts and data make the genre.

## The model

- **Native mechanisms, scripted combinations.** A capability is native code that ships in a release; a package adds no native code, as the verifier runs only code the release pins. A new combination needs no release; a new mechanism does.
- **One model for every capability.** Every capability says its mechanism in the same terms, so capabilities meet in one match and a genre is only data and scripts: a **unit** is anything with a stable id, a position and a type; a **tag** has properties only when the mode gives them; a **stat** is a number the mode declares; a **pool** an amount up to a stat; a **modifier** an instance on a unit from a source for a time; an **action** anything a unit does on purpose, through one pipeline; an **effect** a change the sim applies, which data lists and scripts queue alike; an **event** what happened, heard by the units it concerns and the mode.
- **No genre words in the core.** A mode declares every name its packages use: damage kinds, stats and pools, tags and their effects, slot kinds, teams and relations, choices, player resources, tracks and random tables.

## Capabilities

| Capability | Status | Adds | Doc |
| --- | --- | --- | --- |
| `combat` | built | Life, weapons, the damage and heal pass, deaths, respawns | [Combat](combat.md) |
| `stats` | built | Stats, pools, modifiers, tags, levels | [Stats](stats.md) |
| `abilities` | built | Cast actions | [Actions](actions.md) |
| `projectiles`, `areas` | built | Deliveries that fly and that hold an area | [Actions](actions.md) |
| `orders` | built | Units that take orders; AI `on_think` | [Control](control.md) |
| `character` | planned | Units a player drives directly | [Control](control.md) |
| `hitboxes` | planned | Ray and sweep deliveries, lag compensation | [Hitboxes](hitboxes.md) |
| `navigation` | built | Layers, routes, steering, forced movement, collision | [Navigation](navigation.md) |
| `vision` | built | What each vision group sees | [Vision](vision.md) |
| `items` | built | Inventories, item types, recipes, shops | [Items](items.md) |
| `progression` | built | Experience, levels, points | [Progression](progression.md) |
| `quests` | planned | Quests, objectives, dialogue | [Quests](quests.md) |
| `interaction` | built | The `use` action on objects | [Interaction](interaction.md) |
| `production` | built | Train, build and gather; construction, supply, tech | [Production](production.md) |
| `physics` | planned | Vehicles and rigid bodies | [Physics](physics.md) |
| `world` | planned | Large worlds | [World](world.md) |

- **Built is what the release installs.** A test holds this table's status column equal to the release.
- **A planned capability is the design's alone.** `Capability` holds only the capabilities of a module the release builds, as a manifest that names another fails as it reads and a command carries no index of one; a planned capability joins the enum with its module. Refused: variants for the planned capabilities, which every match, command and log encoded beside the built ones, and which only a load check refused.

## Relations

- **A unit holds up to 256 tags.** A tag set is a fixed set of 256 bits, which a rollback copies with no allocation, as a set of teams is.
- **Teams are indices.** A team is an index into the mode's list of up to 256, so a set of teams is a fixed bitset a rollback copies without allocating.
- **Attitudes between teams.** The mode declares how pairs regard each other, as Unreal's team attitudes do; a pair not named is hostile, and a team is friendly to itself.
- **Neutral is attackable but passive.** A neutral unit may be attacked but seeks no fight: an AI chooses targets among hostiles.
- **Vision groups.** Friendly teams share vision unless their relation says not; teams that share vision form a vision group.

## Space and map

- **Two metrics.** A map is `planar`, measured on the ground plane with heights from the terrain, or `spatial`, measured in 3D; every distance uses it.
- **The sim's ground is the map's height grid.** `map/<name>/heights.bin` is data of the sim, as Spring's `CGround` and OpenRA's `Map.DistanceAboveTerrain` keep the heights in the simulation: a point's height is the plane of its cell's triangle, every cell split on the one diagonal the map's `[ground] split` names, computed from the samples in one step and rounded once ([Determinism core](../07-determinism-core.md)); `[ground] high` gives coarse cells of the highest samples they hold, which `Ground::highest_along` walks by Bresenham's line, as Zero Hour's partition cells and `iterateCellsAlongLine`. A map of no grid is flat, and a point off the grid takes the nearest point's height. Its samples are 8 bits above 0; samples of 16 bits with a base height come when a game needs them, planned.
- **A body has a height.** A unit type's `collision` `height` makes its body a volume from its ground point up, its center halfway, as Zero Hour's `getCenterPosition` raises an object by half its geometry's height; a shot aims at the center, and a splash measures to the volume.
- **Circles and boxes.** A body is a circle, or, for a type that does not walk on a planar map, a box, as 0 A.D. keeps a structure as a static square while a unit stays a circle; pathing, placement, collision and reach all read the box. StarCraft II keeps a radius for reach beside a footprint for pathing, which the engine avoids.
- **A box rounds once.** A box turns to any angle as it spawns, and its rounded half edges are its state, so no later test rounds again; degrees, not radians, make the right angles exact.
- **One bound for every body.** A body's bound is at most 2,048 m, the same for a walker and a structure, so no rule of collision asks which kind it is, and the math at that bound stays within one `i128` step ([Determinism core](../07-determinism-core.md)).
- **Spatial indexes in levels by size.** Each spatial index keeps its bodies in levels by size, as one cell size suits only its widest body (Ericson, *Real-Time Collision Detection*, chapter 7), each level's cell twice the one below, and visits only the levels that hold a body. Refused: a ratio of 4, slower on every collision scene, and of 8, faster for walkers alone and 55 % slower with static boxes ([Cell levels](../../research/cell-levels.md)); one level, which a wide walker makes wide for every body.
- **One reach rule.** A range reaches from the edge of one body to the edge of another, exactly, decided squared with no root, as League of Legends measures edge to edge; weapons, actions, areas, auras and script queries all follow it.
- **One order of a body pair, two tests of a path.** Two bodies have one exact order of their distance against a reach, which `reaches` reads as at most and `closer` as below. A path past a body answers two questions, so it keeps two forms: whether it comes closer than a reach, exactly, as a route's clearance needs, and where along it a body within the reach lies nearest, each distance to the nearest bit, as a hit needs its place. Refused: one order of a path, as no exact test of a path and a box gives the place a hit needs.
- **Layers.** Collision and pathing work within a layer the mode declares, so air passes over ground.
- **Bounds.** No unit is ever outside the map's bounds: orders clamp to them, and a spawn outside fails.
- **Markers.** A map's named points and regions, with tags, a team and params, which scripts read by tag.
- **Generated maps.** The mode's `on_generate` builds a map or a region from the secret stream, so the seed decides it, as Diablo builds a dungeon level; the map checks run on what it built.
- **A spatial map has no sim ground.** A map's space is a planar ground or spatial, and the load refuses `[ground]` on a spatial one. Refused: a load rule alone, which leaves a struct that holds a ground no code reads.

## Game clock

- **Game time is state.** The calendar's time advances with the ticks; `ctx.advance_time` moves it without ticks, while durations keep counting ticks. The sim never reads the wall clock.

## Layers of capabilities

- **A fixed graph.** Capabilities depend on each other in a graph with no cycle, each a crate on its layer ([Modules](../02-engine-core.md#modules)); only the declared capabilities' systems run, and an effect of an undeclared capability fails its load.

## Tick stages

- **Fixed stages.** The engine fixes the stages, and each capability puts its systems into them; two systems with no order and conflicting access fail the schedule build.

| # | Stage | Runs |
| --- | --- | --- |
| 1 | Inputs | The tick's commands reach their capabilities; modifiers end |
| 2 | Think | AI of the units due this tick |
| 3 | Act | Orders start actions; attacks, then casts, then production, by stable id, as each pays at once |
| 4 | Move | Steering, forced movement |
| 5 | Collide | Overlaps resolved within each layer |
| 6 | Hit | Deliveries and their effects; modifier intervals |
| 7 | Resolve | The damage and heal pass; deaths, auras |
| 8 | Mode | Timers, then events in the order they happened; spawns; `ctx.win` and `ctx.draw` |
| 9 | Vision | What each vision group sees; despawns |

- **One due index for each timed process.** A process whose units come due at a tick, as a timed life ends or a charge comes back, pushes each unit where it sets its time into its own index, which gives its pass the units due by tick, then stable id, each once at its earliest push, so the pass visits only those and in the same order in every world; an entry is a visit, not a promise, and the pass pushes a unit whose time moved again. A process whose units several passes run, as the resolves of attacks and of casts, keeps one index, and each pass takes its own units and leaves the others' due. Each process states the tick a unit is due at from its parts, and each index builds again from them, with one scan, whenever the state is written from outside the sim: as a restore ends, as a client's rollback restores the parts, and as a match installs over units a test spawned first; a unit the client starts to predict again is pushed, and a debug build checks at each tick's end that every unit due is held. A process whose times derive from parts that many writers change, as a slot's charges and a passive that waits for a cooldown, has its pass follow those parts' changes by their change ticks, as the passes that follow changes do, and push each unit it visits at its time: such scans cost at most 5 % of the 3v3's mean tick ([Change scans](../../research/change-scans.md)). Refused: a scan of every unit in each tick to find the timers due; `Changed` filters on the parts that hold the times to find the timers due, which read every unit's change ticks each tick; a mark from each writer of slots, times and modifiers, which saves at most those 5 % for a change across seven crates; and the network stack's resource rollback of each index, which restores the client's own predicted index, not one that follows a part the server corrected in place (decided 2026-10-18).
- **Every change scan is measured against one budget.** Each pass that follows change ticks is a case of the change-scans bench, so the 5 % above covers them all. Refused: an index for one pass below the budget.
- **A standing order is due at once.** A unit whose order runs in each tick until it ends, as a channel, a build, a gather loop or an attack target, is held in its process's index at tick 0 while the order stands, and a pass forgets it once the order ends or the unit is gone. A pass whose whole population runs in each tick, as the path walkers, keeps its query, as an index would hold every unit it visits. Refused: a scan in each tick of every unit that may hold such an order, most of which hold none.
- **Stats derive at every edge.** Before the first stage and after each, every unit whose level, modifiers or type changed derives its stats and tags again ([Stats](stats.md)).

## Commands

- **A command names its capability by a byte.** A player input is a list of commands, each naming its capability by its index in a list that only grows, so an old log decodes the same; a command of an undeclared capability, or one that does not decode, is ignored, as a client can send anything.

## Control

- **Control is a relation to units.** Direct, by input frames the client predicts, or by orders that name their units, so one player orders many; AI and bots send the same orders as players. A unit has one controller, its owner; a relation of shared control comes when a game needs it, planned.

## Unit types

- **A section for each capability.** A unit type is one section for each capability it uses, in one schema wherever it is written; a section of an undeclared capability fails the load.
- **Times become whole ticks at load, rounded up,** so a type behaves the same at any rate to within one tick.
- **A unit type may vary its units.** A type's `variations` each give a body, and its client look each a look; `UnitCore::spawn`, every unit's one spawn, draws one on the stream `units.variation` of the new unit's stable id, its core carries the draw to every part, and `Variation`, state every client receives once, keeps it, so every rule, filter, count and bot list reads the one type, as Zero Hour holds a template equal to its build variations (`ThingTemplate::isEquivalentTo`) and 0 A.D. varies one unit's look ([Zero Hour](../../research/zero-hour.md#build-variations)). Refused: types of their own that every comparison matches as their parent, which a comparison that forgets it breaks, as a script's spawn and a bot's count broke; a draw in each part's start, which opens one stream twice in a tick; the body alone as the record, which names no look.
- **The type's body bounds its variations.** What reads one body of a type before its draw, a spawn's room, a map's walk check and a teleport's cell, reads the largest of its variations, which every variation fits; every variation lies on one layer, as one type moves on one layer, and a route takes its unit's own body. Refused: the first variation, which a larger draw does not fit; a body for each variation in each such reader, which has no draw to pick by.
- **A placement may give its variation.** A map's placed unit may name its place among its type's variations, which its spawn takes in place of a draw, and the map's load refuses a place its type lacks, as Warcraft III's `war3mapUnits.doo` and 0 A.D.'s actor seed store a placed unit's choice, and Bevy keeps a component given at spawn over a required one ([Zero Hour](../../research/zero-hour.md#build-variations)). Refused: a draw for every placed unit, which loses what a map places; a type of its own for a placed variation, which every count of its parent misses.

## Tick rate

- **The mode picks its rate.** Within its manifest's range: a shooter 64 to 128 Hz, a MOBA or Zero Hour 30, an MMO 10 to 20; every capability works at any rate, and the worst tick is its budget.

## Scripted systems

- **A mechanism in Rhai.** A mode may declare a scripted system: a script that runs each interval in a stage over the units a filter selects, under the `systems` pool, as Dota 2's custom games add thinkers in Lua. It is called once an interval with all its units, so Rhai's call cost is one an interval, and its filter's units are kept, not searched.

## Testing

- **One builder of a rules match.** `Match::builder` takes the players, the limits, the rate, the role and whether the match starts, and adds each capability's needs, so a test runs a set a package can declare, which `Release::capability_set` still checks.
- **One spec of a spawned unit.** `units::internals::Spawn` takes the type, the place, the team, the facing, the walk and the parts, and its `place` is a test's one path to the spawner; a capability's helper is a preset that returns one. Refused: positional helpers of up to 7 arguments.
- **A mode test loads its package.** A package set in `internals` holds a test mode's files as text, and `Match::of_package` builds its match through `Release::books` and the mode's install, as a session's load does; a test changes the mode by an edit of its data, and derives each value a stat or a pool's maximum sets. Refused: a hand-made install of the books, which allowed what no package can hold, a mode's action in an avatar's slot, and drifted from the load.
- **No module harness.** A tier that installs one capability and its needs makes a cycle, as the mode's core lives above combat, navigation and production, or tests a composition no release runs.
- **The capabilities keep their own short reader of their group's manifests,** which checks each capability's needs against its crate's dependencies. Refused: the reader of `rules` as a library, which puts it in the shipped build.
- **Every pair of capabilities meets in a test.** Every pair a mode may declare together meets in a test mode with a golden, as all-pairs testing covers every two-way combination with few cases.
