# Plan — next steps

Open items only, in order. Remove an item when it is done; remove a system when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. Stage context: [ROADMAP.md](ROADMAP.md); design: [design/](design/).

The work is grouped by the system it touches. Each system starts with a design step: find how established engines and games solve it and why, set what this engine promises, and write it into the design, with the steps below revised to match. The implementation steps are drafts until that design passes review.

## Stage 3 close

1. **Vertical slice playtest, fourth round**: the same 1v1 on LAN, with routes and collision. Units that block each other and body block are new to the feel; write down whether the match reads and plays well. This closes stage 3, or names what is missing.
2. **Collision bench**: on a quiet machine, run `cargo bench -p campfire-capabilities --features bench --bench collision`: 1000 bodies, crowded into 40 m square and spread over 120 m square, contacts found and parted. Check that it fits the Collide stage's share of a tick at 30 Hz, and record the numbers beside the collision design. A timing probe on a busy machine gave 0.14 to 0.42 ms, against 2.1 to 2.5 ms for a check of every pair.

## Game model

Touches: every capability, the registry, the reference packages. Design: [The model](design/04-capabilities/00-overview.md#the-model) and the capability docs. Each step rewrites the reference MOBA's packages to the model as it goes, so the 3v3 match keeps running with the same rules; the code that differs from the design until its step is the code's to fix.

1. **Hook names**: `think` becomes `on_think`, `on_cast` becomes `on_resolve`, and the registry plans `calc_heal`, `on_hit`, `on_end` and `on_interrupt`. Test: the reference packages load and the 3v3 match runs; a script with the old name fails the load.
2. **Tags**: one tag set a unit, the type's, the engine's and its modifiers'; the mode's `[tags]` with their effects; `UnitStates` and the fixed `UnitState` list go; signed tags in filters; 256 tags. `ctx.stun`, `ctx.slow` and `ctx.knock_up` leave the registry: the reference MOBA's crowd control is its own modifiers. Test: each effect on and off from a mode tag; an immunity that suppresses a held modifier and lets it act again, with no cycle; a filter with a negated tag; a session in which a script stuns a unit replays to the same hashes.
3. **Stats and pools**: the one formula, and one value at a level for stats and scaling params alike, `base + per_level × (level − 1)`; the mode's pools, the life pool among them; `[combat]` bindings for leech and heal scale; `d.roll` in place of the crit; the reference MOBA's stats rewritten to them. Test: hand-computed values for `add`, `pct` and the strongest `cut`; a unit with two pools paying a cost in each; a crit the 3v3 mode decides from `d.roll` on the same seed as before.
4. **Relations**: teams beyond 63; the mode's relations and `ctx.set_relation`; `enemies`, `allies` and `neutrals` by relation; vision by friendly group. Test: a match of 100 one-player teams; a neutral pair a script turns hostile; two friendly teams that share one fog.
5. **Map and space**: markers, placed units, a path's direction at spawn, the metric and layers. Test: a three-team map whose paths each team walks its own way; an air unit that passes over a ground unit and a wall; a range measured in 3D on a spatial map.
6. **Choices and one unit type schema**: an avatar package holds one unit type in the units schema; `choose`, `chosen`, `available`, `spawn_unit` with a player and `grant` replace the pick calls; player modifiers. Test: the 3v3 pick from choices; an upgrade that reaches every unit of its player its filter selects, and one spawned after.

## Stats and modifiers

Touches: the `stats` capability, the net protocol and the client. Design: [Stats](design/04-capabilities/stats.md#network).

1. **Client stats and tags**: one unit type order that the server and the client read from the packages; the client builds the stat book, the server sends each unit's type once and its level, and the step is derived on both sides, no longer sent. Test: a client whose unit is slowed by a modifier walks in step with the server; a stun that ends on the server ends on the client in the same tick, with no correction.

## Progression

Touches: a new `progression` capability, the mode's data, the `orders` format, the 3v3 data. Closes: the planned `ctx.add_xp`. Design: [Progression](design/04-capabilities/progression.md).

1. **Experience, levels and learning**: `progression` joins the capabilities, after the last; the mode's tracks, the `level` track among them, and its slot kinds' `levels`; `ctx.add_xp` and the `xp` effect for any unit and track; `on_level_up`; a point a level, and the `learn` order with its rule; the 3v3 data gets its curve. Test: experience to hand-computed levels on two tracks, stats and pools; a skill level-up whose `on_level_up` adds character experience; a rank refused before its level and taken at it; `ctx.learn` past the rule.
2. **Perks**: `[perks]` with their requirements and costs, the `perk` order, `ctx.grant_perk`. Test: a perk refused before its track level and its prior perk, then taken, granting its modifier and action.

## Actions

Touches: the action pipeline in the core, `combat`, `abilities`, `projectiles`, `areas` and `vision`. Closes: the issues of missing ability calls, the projectile and area handles, and the planned action hooks. Design: [Actions](design/04-capabilities/actions.md).

1. **One pipeline and weapons**: `[actions]` tables and a unit type's `actions` by slot kind; the core's checks, time, delivery and cost for every action, a cost in pools or player resources; the attack as an action of kind `attack` in a slot, any number of weapons, each with its filter, its `rate`, `damage` and `damage_kind`; slot kinds from the mode. Test: an RTS unit with a ground and an air weapon that attacks each with the right one; the 3v3 heroes attack exactly as before.
2. **Deliveries as units and effects in data**: projectiles and areas as units of their sections, `ctx.projectile` and `ctx.area`; effect lists `on_resolve`, `on_hit` and `on_end` before their hooks; `d.hit`; `calc_heal`. Test: Rime's Fan of Frost from data alone and Cinder's Eruption, hitting exactly the units hand-placed in reach; a heal that `calc_heal` halves.
3. **Action values and bookkeeping**: `ctx.range`, `ctx.charge` and `ctx.origin`; `ctx.chance` and `ctx.pick` on the secret stream; `ctx.reduce_cooldown`, `ctx.reduce_cooldowns` by slot kind and `ctx.add_charge`. Test: each call against hand-computed cooldowns and charges; draws that the same seed repeats and another changes.
4. **Channels, toggles and reveal**: channels and `on_channel_tick`, toggles with their costs, `ctx.reveal`. Test: a channel broken by a stun, which spends nothing more; a toggle that turns off at an empty pool.

## Forced movement

Touches: the `navigation` capability, the effects. Closes: the issues of `dash`, `teleport` and `knock_back`. Design: [Forced movement](design/04-capabilities/navigation.md#forced-movement).

1. **Forced movement**: the `move` effect and `ctx.dash`, `ctx.teleport` and `ctx.knock_back`, each moving its unit over the ground plane through the static bodies' clearance, ending at the nearest place it may stand, and planning its route again after; a dash ends with its action's `on_end`. Test: a dash stopped by a tower, a teleport into a wall that lands beside it, a knock back to its hand-computed end.

## Singleplayer and saves

Touches: the server, the client, the session log, the runner, the mode's data. Design: [Singleplayer and saves](design/01-campfire-design.md#singleplayer-and-saves), [Saves](design/02-engine-core.md#saves).

1. **Checkpoints and saves**: a checkpoint at a tick boundary, written from a copy on a background thread; a save from a player's command, `ctx.save()` or `[saves] autosave_ms`, refused by `[saves] by = "mode"`; a load that restores a save and starts a new segment from it. Test: a save and a load in the middle of a fight that give the same hashes as the session run through; a load that goes back past later ticks; the verifier proving each segment from its checkpoint.
2. **The local server**: the server as a library the client runs on a thread for singleplayer, with no network; pause and game speed, by ticks a real second. Test: a singleplayer match whose log verifies as a LAN match's does; a paused session whose hashes do not change; a match at double speed with the same hashes a tick.
3. **Carry and campaigns**: the mode's `[carry]`, `ctx.carry`, the carry load as an external input, the `campaign` package with its missions and `opens_when`. Test: a hero whose level and items reach the next mission exactly; a session that verifies alone with its carry load.
4. **Save converters**: the snapshot's data version, the converter of each release from the one before, the chain of converters on load, and the segment that names both releases. Test: a save of a format one version back, converted, that continues with hand-checked state; a save from before the last converter refused with its error.
5. **Region events and generated maps**: `on_enter` and `on_leave` for event markers; `on_generate` and `ctx.generate` with the map checks on what it built. Test: a unit that enters and leaves a region exactly once each, by stable id; a dungeon the same seed builds alike and another seed builds otherwise, saved and loaded whole.

## RPG mechanics

Touches: the core's clock and tables, the package loader, `vision`, `items`, a new `quests` capability; the capability list, where `hitscan` becomes `hitboxes` and `persistence` becomes `world`. Design: [Game clock](design/04-capabilities/00-overview.md#game-clock), [Random tables](design/04-capabilities/00-overview.md#random-tables), [Quests](design/04-capabilities/quests.md), [Senses](design/04-capabilities/vision.md#senses), [Items](design/04-capabilities/items.md), [overrides](design/03-game-scripting.md#game-package).

1. **Capability names**: `hitscan` becomes `hitboxes`, `persistence` becomes `world`, `quests` joins after the last. Test: a manifest with an old name fails; every reference package loads.
2. **Game clock and random tables**: game time with the mode's `[clock]`, `ctx.time`, `ctx.advance_time`; `[tables]`, the `loot` effect and tables in `spawn`. Test: the hour after hand-counted ticks; waiting eight hours with a modifier's ticks unchanged; a table whose rolls the same seed repeats, and whose level condition leaves out what it should.
3. **Package overrides**: `load_order` and `[overrides]`, the last loaded winning, the conflicts listed, the session id over the dependencies in load order. Test: a mod that changes a sword's damage; two mods on one record, the later winning, the conflict listed; a log that names another load order refused.
4. **Quests and dialogue**: `[quests]` with stages and objectives completed by events, `[topics]` with conditions and choices, `on_stage_done`. Test: a quest of a `talk`, a `kill` of three and a `reach`, each completing from its event; a topic hidden until its condition holds.
5. **Senses**: `unit.sees` with range, cone and line of sight; light by the game clock; the `noise` effect and `on_heard`. Test: a guard that sees a unit in its cone and not one behind a wall or behind it; a noise heard within its radius and not beyond.
6. **Crafting**: `[recipes]` and the `craft` action kind, items made, and modifiers added to an item as its state. Test: a sword made from hand-counted items; an enchantment kept through a save and a load.

## Packages and creators

Touches: the package loader, the registry, the client, the reference packages. Design: [Game package](design/03-game-scripting.md#game-package), [Creator tools](design/02-engine-core.md#creator-tools).

1. **Package API version**: the manifest's `api` in place of `engine`, the registry's version of each name, the rule of major and minor at load, the version in the reference. Test: a package of an older minor loads; one of another major, and one of a newer minor, fail with their errors.
2. **Human text**: `locale/<language>.ftl`, message ids in data and scripts, the load check that every id has its text, the `locale` package kind; the reference packages' text moved to `en.ftl`. Test: a hero's name in a second language from a `locale` package; a missing id refused at load; a match whose hashes do not change with the language.
3. **Data schemas**: a JSON Schema for each data file, generated from the engine's types and checked in. Test: the schemas the engine writes equal the checked-in ones; every reference package's data passes them.
4. **Hot reload**: a dev session that reloads changed scripts, data and text, its log marked unverifiable. Test: a changed param that takes effect in the running session; the verifier's refusal of a dev log.

## Scripted systems

Touches: the core's script host, the mode's data, the registry. Design: [Scripted systems](design/04-capabilities/00-overview.md#scripted-systems).

1. **Scripted systems**: the mode's `[systems]`, the hook `on_system(ctx, units)`, each system's units kept as their tags change, the `systems` pool, the order by name within a stage. Test: a system that runs exactly each interval in its stage over exactly the units its filter selects, as a unit gains and loses the tag; a spent pool that makes it run first in the next tick; a failed call that changes nothing; the replay gives the same hash.

## AI orders

Touches: the `orders` capability, the unit handle. Closes: the issues of `order_move`, `order_reset` and `spawn_pos`.

1. **Design**: how MOBA and RTS AI leash and reset neutral units, and what an AI order promises against player orders and routes.
2. **AI orders and the camp AI**: `ctx.order_move` and `ctx.order_reset` (walk home, heal, drop the target), and the unit handle's `spawn_pos`, which the view's row reads from the spawn point combat keeps for respawns, so the two never differ; the 3v3 camps stop failing. Test: a camp pulled past its leash range walks home, heals and drops its target; the 3v3 match runs with no failed call.

## Players joining and leaving

Touches: the session, the mode hooks. Closes: the issue of `on_player_join` and `on_player_leave`.

1. **Design**: how established games handle a player that leaves and comes back (control handed to a bot, abandon rules), and how a join or leave enters the session log so a replay repeats it.
2. **Players joining and leaving**: `on_player_join` and `on_player_leave` run when a player's link comes and goes, in the tick the server notes it, as the session log records it. Test: a LAN match where a client leaves and comes back; the replay gives the same hash.

## 3v3 reference content

Touches: the 3v3 mode's scripts and data, design 07. Closes: the issue of creeps that never attack structures.

1. **Design**: the rules of structures in MOBAs: which structure protects which, what creeps choose and in which order (League of Legends' minion target priority, Dota 2's creep aggro), and what the reference game takes from them. The 3v3 creep AI lost its structure step because it is a copy of the lane mode's that drifted: the order of targets becomes the mode's data, a list of filters the one AI script walks, so the two modes differ only in data.
2. **3v3 structures fall in order**: the creep AI reads its target order from the mode's params: after its own hero's attacker and its target, the nearest enemy creep, then structure, then avatar, as design 07 sets. Each lane's structures fall in order, from the outer tower in: a tower or an inhibitor is invulnerable, by a mode modifier, until the one before it on its lane has fallen; the core, while all of its team's inhibitors stand. Untargetable is not enough: an area still reaches an untargetable unit. An inhibitor that comes back protects again. The rules go into design 07. Test: a 3v3 match in which the first waves push a lane: the creeps hit the outer tower and never an inner structure while it stands, and the inner tower takes damage only after the outer one fell.
3. **Whole camps and streak bounties**: each camp is a map marker with its monsters and respawn time; its timer starts when its last monster dies, and the whole camp spawns again; a hero's streak bounty grows from its second kill without dying, up to its cap, and its killer takes it ([Reference MOBA](design/07-reference-moba.md#rules)). Test: a camp with one monster left does not respawn, and comes back whole its respawn time after the last dies; hand-computed gold for a kill of a hero on a streak of four, with two assisters.

## Script API closure

Touches: the registry, the load check. Closes: the planned names, once none is left.

1. **The load check refuses what the release does not run**: the planned list is empty, and goes; a name the registry does not hold fails the load. Test: a package flaw for each kind of name; every reference package loads.
