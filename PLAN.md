# Plan — next steps

Open items only, in order. Remove an item when it is done; remove a system when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. Stage context: [ROADMAP.md](ROADMAP.md); design: [design/](design/).

The work is grouped by the system it touches. Each system starts with a design step: find how established engines and games solve it and why, set what this engine promises, and write it into the design, with the steps below revised to match. The implementation steps are drafts until that design passes review.

## Stage 3 close

1. **Vertical slice playtest, fourth round**: the same 1v1 on LAN, with routes and collision. Units that block each other and body block are new to the feel; write down whether the match reads and plays well. This closes stage 3, or names what is missing.
2. **Collision bench**: on a quiet machine, run `cargo bench -p campfire-capabilities --features bench --bench collision`: 1000 bodies, crowded into 40 m square and spread over 120 m square, contacts found and parted. Check that it fits the Collide stage's share of a tick at 30 Hz, and record the numbers beside the collision design. A timing probe on a busy machine gave 0.14 to 0.42 ms, against 2.1 to 2.5 ms for a check of every pair.

## Stats and modifiers

Touches: the `stats` capability, the combat and navigation systems that read stats and states, `vision` for stealth and true sight, the `orders` format, the script API's handles, the reference packages' data. Closes: the issues of inert states, missing modifier calls and handle fields, and `true_sight`. Design: [Stats](design/04-capabilities/stats.md), from Unreal's Gameplay Ability System, League of Legends and Dota 2.

1. **Client stats**: a client derives its units' stats from the modifiers, levels and types the server sends, as stats.md's "What a client predicts" says: it loads the mode's stat book and runs the same refresh, so its predicted step, attack period and pool maxima follow its modifiers. Test: a client whose unit gets a move speed modifier walks the server's path in step, with no correction; a modifier that ends on the server ends on the client in the same tick.
2. **States take effect**: a unit's states are derived beside its stats, in the same refresh, as one set, and each system asks that set whether its action may run, so no system reads modifiers for itself; every state in stats.md's table where its system runs, with the order kept for after the state; `ctx.stun`, `ctx.slow` and `ctx.knock_up` as engine modifiers, loaded into the book under a reserved package, so they stack, refresh and end as any other. Two design changes come first: an `invulnerable` state, as `untargetable` and taking no damage, areas included, which Dota 2 and League of Legends give a protected structure; and `true_sight` as a state a modifier gives, the unit type's field a passive from itself, so a ward, an item and a unit type see stealth by one mechanism. Stealth and true sight in vision. Test: each state on and off, with the action it stops refused and then taken; two stuns from one source and from two; a stealthed unit seen only through a true-sight unit; the replay gives the same hash.
3. **Levels and experience**: the mode's `levels` and `rank_levels`, `ctx.add_xp`, a point a level, and the `learn` order with its rule; the reference 3v3 data gets its curve. Test: experience to hand-computed levels, stats and pools; a rank refused before its level and taken at it; `ctx.learn` past the rule.

## Abilities

Touches: the `abilities`, `projectiles` and `vision` capabilities. Closes: the issues of missing ability calls, the projectile and area handles, and ability hooks.

1. **Design**: how ability systems structure casting, channels, projectiles and areas (Dota 2's ability and projectile model, League of Legends' spell system, Unreal's Gameplay Ability System), what an ability call may read and change, and how a client predicts its own casts.
2. **Ability values and bookkeeping**: `ctx.range`, `ctx.charge` and `ctx.origin` in an ability call; `ctx.chance` and `ctx.pick` on the secret stream; `ctx.reduce_cooldown`, `ctx.reduce_cooldowns` and `ctx.add_charge`. Test: each call against hand-computed cooldowns and charges; draws that the same seed repeats and another changes.
3. **Ability effects**: ability projectiles, `ctx.projectile`, with the projectile handle, `on_projectile_hit` and `on_projectile_end`, `stop_on_hit`, `once_per_cast` and `[projectile_state]`; areas, `ctx.area`, with the area handle, `delay_ms`, `duration_ms`, the modifiers held `inside`, and `on_area_trigger`; `ctx.reveal`; channels and `on_channel_tick`. Test: Rime's Fan of Frost and Cinder's Eruption from their packages, hitting exactly the units hand-placed in reach; a channel broken by a stun.

## Forced movement

Touches: the `navigation` capability, with states from stats. Closes: the issues of `dash`, `teleport`, `knock_back` and `on_dash_end`.

1. **Design**: how forced movement is resolved against terrain, structures and other units in established games, how it interacts with crowd control and routes, and how a client predicts it.
2. **Forced movement**: `ctx.dash` with `on_dash_end`, `ctx.teleport`, and `ctx.knock_back`, each moving its unit over the ground plane through the static bodies' clearance, ending at the nearest place it may stand, and replanning its route after. Test: a dash stopped by a tower, a teleport into a wall that lands beside it, a knock back to its hand-computed end.

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

## Script API closure

Touches: the registry, the load check. Closes: the planned names, once none is left.

1. **The load check refuses what the release does not run**: the planned list is empty, and goes; a name the registry does not hold fails the load. Test: a package flaw for each kind of name; every reference package loads.
