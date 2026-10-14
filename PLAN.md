# Plan

The one plan of the project: every open step, in the order it is done, in batches. A batch is one commit: it holds the steps that change the same systems, refactorings, fixes and features alike, and ends with `./drive.py check` passing, and a stop for review. Each batch says what it touches and why it stands where it does. Open items only: a step goes when it is done. The issue log, `.notes/ISSUES.md`, holds every open problem; a problem that a step here takes carries the step's tag there ([Issue log](AGENTS.md#issue-log)). The designs in `docs/design/` stay the source of truth: a step names what the code lacks against its design, or the design it proposes, which moves into `docs/design/` when it is agreed.

Every batch moves the engine toward the [Direction](AGENTS.md#direction), and each step names the part it builds. Line numbers point to the code as each step was written; a step refreshes its references as it starts. A step that states no check holds this one: no golden moves, and `./drive.py check` passes.

## 1. The shrine

`packages/test/moba`, `client`.

**Slice 2, wide bodies, uses, the shop and gravity,** batches 1 and 2. The user plays a 3v3 match of the MOBA test content: their hero holds a use on the jungle's shrine, buys items from the shop panel, and casts their actives with 1 to 6, a ward at the ground point; a test map holds a 1,117 m box that walkers go around. The design is [Wide bodies, uses, the shop and gravity](docs/design/13-bodies-uses-shop-gravity.md); each step folds its rules into the design 04 file of its capability.

**The capability model's redesign,** M3 to M10, a proposal for review: each step's decisions move into `docs/design/04-capabilities/` as it lands, and each generates the script API reference again. The core model is sound and generic: the action pipeline, the effect lists, stats and their formula, tags, relations and filters, the reach rule, deliveries as units and scripted systems serve every genre, and stay. The faults are at the edges, where a capability was built for its first genre: the refactorings before them took its couplings, and M3 to M10 open its closed sets. Each new section of a unit type an M step adds is built as the [Direction](AGENTS.md#direction) gives it: a `DataSection`, its owner's record in `DataTable::UnitType`, its book, its part, `d.part`, when its units carry state of it, and an `Effect` with its callback, `d.callback`, for each new effect.

### U2. The shrine

(`packages/test/moba`, `client`; D6.) The shrine, its action, `shrine_blessing` and `shrine_spent` in design 07 and the 3v3 package; a right click on a unit the avatar may use orders the use; the channel bar. Check: the scripted match has a hero use the shrine and the team's heroes take the blessing, and a second hero's use cut by a hit; the user holds a use on the shrine in the client.

## 2. Gravity

`capabilities`: `projectiles`; `package`; `packages/test/moba`; `client`. Slice 2's last step: the user then plays slice 2.

### G1. Gravity

(`capabilities`, `package`, `packages/test/moba`; D13.) The falling flight; `gravity` past planned, refused with `homing` or not positive; a grenade of the MOBA's items, which arcs to its point. Check: the points of a flight on each metric at its launch, its peak and its end as computed by hand, the planar one back at its launch height at its end; a spatial one hits a unit its chord meets and not one its straight line would; the client draws the grenade's arc.

## 3. Queues and capacities

`capabilities`: `production`, `actions`; `package`; `docs/design/11`. Train, research and supply in one form, before the Zero Hour skirmish, so its rules package and golden are made once, on the general model.

### M3. Queued actions with any output

(`capabilities`: `production`, `actions`; `package`.) Today `train` spawns a unit type alone; research has no kind, so an RTS times it with a mode timer; `craft` is planned as a third kind of "consume, wait, produce". Any unit may have `queue = { size }`. An action with `queue = true` joins the queue in place of a windup: it pays at once, waits its time at the head, then resolves with its `on_resolve` effects and hook. `train` becomes such an action with a `spawn` effect, research one with a `player_modifier = { id }` effect, craft one with a `give_item` effect, planned with items; cancel and refund move to the queue. The `train` kind goes, and no `craft` kind comes. Today's train rules become the queue's: checked as it joins, its costs, requirements and capacities (M4); never under way, so its unit stays free; its time from the tick it reaches the head; it resolves in the Mode stage of the tick its time ends in, and the next entry starts in that tick; a dead producer's queue waits; a producer that despawns takes its queue and refunds nothing. A queued `spawn` places its unit by the train's rule, the producer's body point nearest its rally point and the nearest open cell, and sends it to the rally point; a `spawn` of any other action keeps its own rule. StarCraft II's `CAbilQueue` trains and researches through one queue. Check: production's train, cancel, supply and rally tests pass with their assertions unchanged, on data that states a train as a queued action.

### M4. Capacities

(`capabilities`: `production`; `package`.) One `[supply]` counter serves StarCraft alone. The mode declares `[capacities.<name>] max`; a unit type gives `capacity = { <name> = { uses, provides } }`. Each is derived as supply is now, a pass over the owned units and the queues when a tick's first check needs it, so it keeps no sum and never drifts (design 11, D5). A queued action or a build lists the capacities it checks. Supply becomes one capacity; power, population and housing are more. Scripts read `ctx.capacity_used(player, name)` and `ctx.capacity_max(player, name)`.

## 4. Work and haul

`capabilities`: `production`, `interaction`; `package`; `docs/design/13`. Construction and gather in their general form, after batch 3, whose queued actions a production chain uses.

### M5. One work mechanism

(`capabilities`: `production`, `interaction`; `package`; `docs/design/13`.) Construction's rate table by builders and interaction's shared progress are one mechanism, one native and one planned as script state. A unit type's `work = { rates = [...], max_workers }` makes a unit that others work on: `rates` gives the rate for each count of workers from zero, a count past the table takes its last entry, and `max_workers` refuses a worker past it. The three construction styles become three tables: `alone` `rates = [1]`; `builder` `rates = [0, 1]`, `max_workers = 1`; `builders` `rates = [0, 1, 1.62, 2.16]`. Progress, its rounding of life and its completion tick stay as construction defines them. Completion runs the type's `on_complete` effects and the mode's `on_work_complete(ctx, unit)` hook. A held use with one side's progress, plant, defuse, revive, repair, becomes work, and M5 then amends design 13's D5 and `interaction.md`, which keep shared progress as script state until it lands; slice 2's shrine has no progress, so neither waits on the other. A capture point, by team with contest and decay, is an extension planned with `interaction`. TF2's capture rate by the count of cappers is the precedent. Check: the three styles' construction tests pass, tick for tick.

### M6. Haul

(`capabilities`: `production`; `package`.) Gather fills only a player resource, from a node to a drop-off. A unit type's `stock = { <resource> = { amount, max } }` gives a unit its own amounts. `gather` becomes `haul`: `from` filters units with stock of the resource, a node being one with no `max`; `to` filters units that take it; `take` is the most one trip carries; `windup_ms` the time at the source. A sink's section names where it takes: `drop_off = { resources, into = "player" }` is today's drop-off, `into = "stock"` adds to its own stock within its `max`. The loop, its wait, its `bounce` and its tie rules stay. A production chain is a queued action (M3) with a `stock_cost` from its producer's own stock and a `stock = { resource, amount }` effect: a smelter turns ore into bars. A cost from stock is its own field, as `cost` names pools and player resources, and a stock's resource shares a player resource's name. RimWorld's haul jobs and The Settlers' carriers move goods this way.

The snapshot tests sweep the new state types of M3 to M6 as they sweep the skirmish's (design 11, D14).

## 5. The genre proofs

Each of M7, M8 and M9 goes in the batch of the genre proof that first needs it, and M10, the Hive mode, after them.

### M7. Points as track data

(`capabilities`: `progression`; `package`.) A track gives `points = [...]`, the points each level gives, or `points = 1` for each level; the `level` track keeps one a level, so the MOBA keeps its data; a second track may give points, for Skyrim's perks.

### M8. The player as an actor

(`capabilities`: `units`, `players`, `actions`; `package`.) Each player slot gets a player unit beside its player entity of design 14, which keeps the player's resources and modifiers: a unit of its own kind, `PlayerUnit`, whose marker requires `UnitEntity`, so every system of bodies, `With<BodyUnit>`, passes it by; with no body and no pools, so it collides with nothing and takes no damage; the engine tag `player`, which `players` declares, `d.engine_tags`, with the property that a filter selects its unit only when it names it, as `projectile` and `area` have; `players` comes second in the module list, so every engine tag after `avatar` moves by one, and the state column and the data version move with M8; its own team sees it, so its owner receives its cooldowns and charges; it stands at the centre of the map's bounds, which no rule reads, as its actions aim with global range. Its actions run through the pipeline as it is, and pay from its player entity's resources: tower defense's placement, Generals' powers and card play need no second pipeline. Unreal's GAS gives the PlayerState an ability system for this. A `build` action of the player unit places a site with no builder's walk. A player unit takes a stable id only in a mode with `[player] slots`, so other matches keep their ids and goldens.

### M9. Movement that spends

(`capabilities`: `navigation`, `stats`; `package`.) A unit type's `move_cost = { pool, per_meter }` makes each of its own steps pay from a pool; a push of collision and a forced move pay nothing, as neither is the unit's choice. A step the pool cannot pay is not taken, and the unit stands as a blocked unit does. The rounding is per step, exact, and kept as a pool's carry is.

### M10. The Hive mode, a game no current mode describes

(`packages/test/modes/hive`.) A test package with a scripted match and a golden: two players as hive minds with no avatar place structures (M8), drones haul ore to smelters and bars to a forge (M6), the forge crafts turrets and researches armor (M3), each structure draws power that pylons provide (M4), a beacon is captured faster by more drones (M5), a drone with a bar spends fuel (M9), enemy waves walk paths and turrets shoot them through their AI, and the drones need no `combat`. A later change that breaks a part of the generic model breaks it.

What each step of M3 to M9 may move of the goldens; a step that moves a column it must not is a bug in the step:

| Step | State column | Behaviour column |
| --- | --- | --- |
| M3 to M6 | Moves where production's state types change | Does not move for the proving match; the skirmish's is new |
| M7, M9 | Does not move: the defaults keep today's rule | Does not move |
| M8 | Does not move in a mode with no `[player] slots` | Does not move |

## 6. The network

`quic`, `net`.

### N1. Limits for each address

(`quic`, `net`; **Quic**.) No limit holds live connections or new handshakes for each address, where design 15's D13 bounds both by the session's seats for each IPv4 address or IPv6 /64, and `net` gives the endpoint no seat count ([QUIC transport](docs/design/15-quic-transport.md), D13). After the bench of the endpoint, the issue log's **Quic** item on benches, sets the cap of new connections on the same path.

### N2. UPnP behind a host firewall

(`quic`; **Quic**.) `igd-next` discovers a router by search alone, and the router's `NOTIFY` announcements, which a host firewall that tracks connections lets in, go unread ([QUIC transport](docs/design/15-quic-transport.md), D12).

Then the network's Research items of the issue log as they show; then P1 to P4, P3 only as D14 of design 15 lets it.

## 7. Planned

### P1. LAN listing

([Protocol](docs/design/05-protocol-spec.md#connection), step 4.) A LAN server announces its signed listing over mDNS; no code announces or reads one.

### P2. Multiplayer pause

([Sessions](docs/design/10-sessions.md#decisions), D6.) Pause and game speed work on the local server only; D6 leaves a pause of a multiplayer session, by the host's setting, to the Zero Hour skirmish.

### P3. A puzzle before the handshake

([QUIC transport](docs/design/15-quic-transport.md), D14.) Adaptive, dormant at effort 0, ordered by effort, memory-hard, in its own exchange on the server's port before QUIC, as QUIC's Retry carries no work; only once the flood's bench shows a reachable flood starves the server. Its puzzle needs a license Campfire can ship, which Arti's Equi-X, LGPL-3.0, is not.

### P4. Milestone 2, the open network

([Vision](docs/design/01-campfire-design.md#milestones), [Decentralized server network](docs/design/01-campfire-design.md#decentralized-server-network), [Nostr events](docs/design/05-protocol-spec.md#nostr-events).) Nostr listings and discovery, the `launcher` and its server browser with signed releases, packages over Blossom (`content`), reputation and ban lists (`identity`), each a planned module of [design 02](docs/design/02-engine-core.md#module-map).

## Foundations

The open problems whose cause is in the architecture or in a format the project chose, not in one piece of code: each shows as several items of the issue log, or limits every system built after it. Each entry names its cause, what it limits, and the issue-log items it holds ([Issue log](AGENTS.md#issue-log)).

- **The import's materials are glTF's** (Zero Hour). glTF's material model holds no W3D pass past the first, no ambient color and no fallback texture, so the presentation loses each, and each later fidelity gap of a W3D material meets the same limit. Items: Zero Hour, "The import writes W3D's materials into glTF's material model"; Zero Hour, "A texture no archive holds"; Zero Hour, "The import ignores a vertex material's ambient color"; Zero Hour, "The import draws a mesh's first material pass".
- **The import has no object that only the client shows** (Zero Hour). Every object a map places becomes a unit of the sim, where the game spawns props, optimized trees and fluff on the client alone and some objects not at all, so the sim, its log and its state hash hold objects the original never simulates, and the parity oracle meets them. Items: Zero Hour, "The import makes a unit of every object a map places".
