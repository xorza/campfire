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
3. **Stats and pools**: the one formula; the mode's pools, the life pool among them; `[combat]` bindings for leech and heal scale; `d.roll` in place of the crit; the reference MOBA's stats rewritten to them. Test: hand-computed values for `add`, `pct` and the strongest `cut`; a unit with two pools paying a cost in each; a crit the 3v3 mode decides from `d.roll` on the same seed as before.
4. **Relations**: teams beyond 63; the mode's relations and `ctx.set_relation`; `enemies`, `allies` and `neutrals` by relation; vision by friendly group. Test: a match of 100 one-player teams; a neutral pair a script turns hostile; two friendly teams that share one fog.
5. **Map and space**: markers, placed units, a path's direction at spawn, the metric and layers. Test: a three-team map whose paths each team walks its own way; an air unit that passes over a ground unit and a wall; a range measured in 3D on a spatial map.
6. **Choices and one unit type schema**: an avatar package holds one unit type in the units schema; `choose`, `chosen`, `available`, `spawn_unit` with a player and `grant` replace the pick calls; player modifiers. Test: the 3v3 pick from choices; an upgrade that reaches every unit of its player its filter selects, and one spawned after.

## Stats and modifiers

Touches: the `stats` capability, the net protocol and the client. Design: [Stats](design/04-capabilities/stats.md#network).

1. **Client stats and tags**: one unit type order that the server and the client read from the packages; the client builds the stat book, the server sends each unit's type once and its level, and the step is derived on both sides, no longer sent. Test: a client whose unit is slowed by a modifier walks in step with the server; a stun that ends on the server ends on the client in the same tick, with no correction.

## Progression

Touches: a new `progression` capability, the mode's data, the `orders` format, the 3v3 data. Closes: the planned `ctx.add_xp`. Design: [Progression](design/04-capabilities/progression.md).

1. **Experience, levels and learning**: `progression` joins the capabilities, after the last; experience beside each unit's level; the mode's `levels`, and its slot kinds' `levels`; `ctx.add_xp` for any unit; a point a level, and the `learn` order with its rule; the 3v3 data gets its curve. Test: experience to hand-computed levels, stats and pools; a rank refused before its level and taken at it; `ctx.learn` past the rule.

## Actions

Touches: the action pipeline in the core, `combat`, `abilities`, `projectiles`, `areas` and `vision`. Closes: the issues of missing ability calls, the projectile and area handles, and the planned action hooks. Design: [Actions](design/04-capabilities/actions.md).

1. **One pipeline and weapons**: the core's checks, time, delivery and cost for every action; the attack as an action of kind `attack` in a slot, any number of weapons, each with its filter, its `rate`, `damage` and `damage_kind`; slot kinds from the mode. Test: an RTS unit with a ground and an air weapon that attacks each with the right one; the 3v3 heroes attack exactly as before.
2. **Deliveries as units and effects in data**: projectiles and areas as units of their sections, `ctx.projectile` and `ctx.area`; effect lists `on_resolve`, `on_hit` and `on_end` before their hooks; `d.hit`; `calc_heal`. Test: Rime's Fan of Frost from data alone and Cinder's Eruption, hitting exactly the units hand-placed in reach; a heal that `calc_heal` halves.
3. **Action values and bookkeeping**: `ctx.range`, `ctx.charge` and `ctx.origin`; `ctx.chance` and `ctx.pick` on the secret stream; `ctx.reduce_cooldown`, `ctx.reduce_cooldowns` by slot kind and `ctx.add_charge`. Test: each call against hand-computed cooldowns and charges; draws that the same seed repeats and another changes.
4. **Channels, toggles and reveal**: channels and `on_channel_tick`, toggles with their costs, `ctx.reveal`. Test: a channel broken by a stun, which spends nothing more; a toggle that turns off at an empty pool.

## Forced movement

Touches: the `navigation` capability, the effects. Closes: the issues of `dash`, `teleport` and `knock_back`. Design: [Forced movement](design/04-capabilities/navigation.md#forced-movement).

1. **Forced movement**: the `move` effect and `ctx.dash`, `ctx.teleport` and `ctx.knock_back`, each moving its unit over the ground plane through the static bodies' clearance, ending at the nearest place it may stand, and planning its route again after; a dash ends with its action's `on_end`. Test: a dash stopped by a tower, a teleport into a wall that lands beside it, a knock back to its hand-computed end.

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
