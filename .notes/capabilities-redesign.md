# Capabilities — review and redesign proposal

Status: proposal for review. Nothing here is agreed. When it is agreed, its decisions move into `docs/design/` and its steps into `PLAN.md`.

Scope: the capability model of `docs/design/04-capabilities/` and the crate `campfire-capabilities` at `462ce89`. The review asks three questions of each capability:

1. **Duplication.** Do two capabilities, or two modes' needs, run the same mechanism in two places?
2. **Reuse.** Can a mode take a capability without the capabilities and rules it does not need?
3. **Genericity.** Can a package build a game that no current mode describes, with data and scripts only?

## Summary

The core model is sound and generic. It does not need a rewrite. These parts are already one mechanism for every genre: the action pipeline, the effect lists and their registration, stats and their formula, tags and their properties, relations and filters, the reach rule, deliveries as units, and scripted systems.

The faults are at the edges, where a capability was built for its first genre:

- **Duplication.** Four mechanisms exist two or three times: modifiers held by a source, progress that workers add to, "consume, wait, produce", and a unit that carries something.
- **Reuse.** Most capabilities require `combat` for schedule anchors only. `orders` owns the commands of `items`, `production` and `progression`, so a mode with no `orders` cannot buy an item or learn a rank. The core lists production's engine tags. Three tables state the capability graph, and no test ties them together.
- **Genericity.** Several genre rules are fixed in native code: one supply counter, one shop at markers, one point per level, an actor that is always a unit, a gather loop that only fills player resources, and a train that only spawns units.

The proposal has 12 changes in three phases, and a 13th that adds the tests which keep the model generic. Phase 1 removes couplings with no change of behaviour. Phase 2 generalizes production. Phase 3 opens the closed sets. Phase 2 changes the model that the Stage 7 skirmish tests. So it should land before the skirmish's match and golden (R9 of the skirmish plan), or the skirmish is built twice.

## Findings

### A. Duplication

**A1. Modifiers held by a source, three times.**
- `actions/holds.rs` (`Holds::hold_passives`) keeps each action's `passive_modifier` and `hold`.
- `items/item_holds.rs` (`ItemHolds::hold_items`) keeps each carried item's modifiers.
- `stats/held_pass.rs` (`HeldPass`) keeps auras, an area's `inside` modifiers and player modifiers.

The first two are the same loop. Each one lists the modifiers its source wants on a unit, compares them with what the unit holds, then applies or releases them. Each one visits only the units whose source or modifiers changed. The unit type's `passive` is a fourth source. Perks and equipment are planned as a fifth and a sixth (`progression.md`, `items.md`), so each would copy the loop again.

**A2. Progress that workers add to, two designs.**
- Construction (`production.md`, Construction): a site's progress grows each tick by a rate from a table indexed by the count of builders in range. A `builder` site takes one builder.
- Interaction (`interaction.md`, planned): "shared progress is the object's script state, which each user's channel adds to". Capture points, plant, defuse and revive use it.

These are one mechanism: a target, the units that work on it, a rate by their count, and an event when it completes. One design is native and one is scripted. TF2's capture rate grows with the count of cappers by a table, as the builders table does.

**A3. "Consume, wait, produce", three kinds.**
- `train` (`production`): a queue entry pays, waits its time, then spawns a unit type.
- Research has no kind. `production.md` says an upgrade is "a player modifier ... which the mode's script gives". So an RTS times its research with a mode timer.
- `craft` (`items`, planned): takes items and pools, waits, then makes an item.

StarCraft II's data editor gives training and research one queue ability (`CAbilQueue`); train and research differ only in what completes.

**A4. A unit that carries something, three models.**
- A worker's load (`production`, Gathering): one amount of one player resource.
- An inventory (`items`): typed entries in slots.
- `enter` (`interaction`, planned): units inside a host.

An item on the ground is planned as a unit that a pickup turns into an entry. A captured flag, a carried ball and a passenger all need "a unit attached to another". No model gives that.

**A5. Player commands applied by parallel systems in `orders`.** `LearnOrders`, `ProductionOrders` and `TradeOrders` each read `TickOrders` and apply one capability's verbs in Inputs. The pattern is fine, but its owner is wrong (B2).

### B. Reuse

**B1. `combat` is a prerequisite only for schedule anchors.** The capability table (`capability_set/mod.rs`) makes `vision`, `projectiles`, `areas`, `abilities` and `orders` need `combat`. Their code uses only `CombatSet` to order their systems, except projectiles. `vision` imports nothing from `combat`. So a game with no damage still needs the life pool rules and the combat schedule: a racing game with boost abilities, a puzzle game with fog of war, or a colony sim whose units take orders.

**B2. `orders` owns other capabilities' commands.**
- `Action` (`orders/order/mod.rs`) holds `Learn`, `Buy`, `Sell`, `Swap`, `CancelTrain`, `Rally`, `Build` and `CancelBuild`.
- `Orders::install` schedules `ProductionOrders` and `TradeOrders` when production and items are present.

The layer test allows this import, as `orders` is layer 6 and `production` and `items` are layer 5 (`LAYERS` in `capability_set/tests.rs`). The fault is ownership, not direction: the verbs live with the one capability that a mode may not declare. A mode that drives characters and has no `orders` cannot buy, sell, learn, cancel or rally. Counter-Strike's buy menu is the first case (Stage 8). The overview already states the rule this breaks: "each command names the capability that owns it" (`00-overview.md`, Commands).

**B3. The core lists capabilities' engine tags.** `units/engine_tag.rs` fixes `Avatar, Projectile, Area, Constructing, Node, DropOff, Gathering` as the first tags of every match. Each new capability adds to a core list. The overview says "the core names no capability".

**B4. Deliveries depend on `combat`.** `projectiles` reads `combat::shots::Shots` and `combat::pass_queue::PassQueue` (`projectiles/launching.rs`). A weapon's projectile is combat's request to a delivery. So the dependency must point from combat to a delivery seam that `actions` owns, not from projectiles to combat.

**B5. Three statements of one dependency graph.** The capability table's `needs` (`CAPABILITIES` in `capability_set/mod.rs`) says what installs before what. The layer table (`LAYERS` in `capability_set/tests.rs`) says what may import what. The overview's "Layers of capabilities" says it in prose. No test ties them together. So `vision` needs `combat` in the first table, but imports nothing from it, and B1 grew unseen.

### C. Genericity: genre rules in native code

| # | Rule fixed in code | Its genre | What a new game needs |
| --- | --- | --- | --- |
| C1 | One `[supply]` counter (`SupplyRules { max }`, `SupplyData { cost, provides }`) | StarCraft | C&C's power, AoE's population and a city builder's housing are more counters of the same form |
| C2 | Construction's closed enum `Alone`, `Builder`, `Builders(table)` | SC2, AoE | One rate table and a worker limit say all three (A2) |
| C3 | Gather fills only a player resource, from a `node` to a `drop_off` | StarCraft, AoE | A production chain moves goods from unit to unit: a mill takes grain and makes flour. A delivery game hauls a package |
| C4 | One `[shop]` per mode, at markers, and the dead may buy | League of Legends | Dota's side and secret shops, Warcraft III's shops as units, a courier, an RTS market |
| C5 | One point for each level of the `level` track | LoL, Dota | Skyrim's perk point per character level only, a game with no points, a talent every fifth level |
| C6 | Every action has a unit as its actor | All current modes | Tower defense placement, Generals' powers, an auto-battler's shop, card play. Unreal's GAS puts player-level abilities on the PlayerState for this reason |
| C7 | Movement spends nothing | All current modes | Fuel, stamina for a sprint, a tactics game's move points |
| C8 | `train` spawns only a unit type | StarCraft | Research (A3), a unit that a building upgrades into, a crafted item |

## Acceptance test: a game no current mode describes

**Hive**: two players co-operate. Each is a hive mind with no avatar.
- They place structures directly on the map (C6).
- Drones haul ore to smelters and bars to a forge (C3).
- The forge crafts turrets (A3).
- Each structure draws power that pylons provide (C1).
- Enemy waves walk paths, and turrets shoot them through their AI.
- A beacon is captured faster by more drones (A2).
- A drone that carries a bar moves slower and spends fuel (C7).
- Research of armor comes from the forge's queue (A3).

| Need | Today | After the redesign |
| --- | --- | --- |
| Place a structure with no builder unit | No: a build action needs a unit actor that walks into range | R11: a player action of kind `build`, range global |
| Drones haul between units | No: gather fills only player resources | R8: `haul` from a unit's stock to another unit's stock |
| Forge crafts a turret item, and researches armor | No: train spawns only units; research and craft do not exist | R5: a queued action whose output is any effect list |
| Power used and provided | No: one supply counter, already named `supply` | R7: a mode's capacities |
| Beacon captured by count | Script only, with the rate kept by hand | R6: `work` with a rate table |
| Fuel spent by distance | No | R12: `move_cost` |
| Enemy waves, turret AI | Yes: spawn groups, paths, `on_think` | — |
| No combat for drones | Drones need a life pool only if they can die. Today `orders` needs `combat` anyway | R2: `orders` needs `navigation` only |

## The redesign

Each change says what it replaces and how much it costs, and gives its source practice. "Breaking" means that data, scripts or goldens change. The project keeps no backward compatibility, so breaking is allowed, but each golden it moves says why.

### Phase 1: couplings, with no behaviour change

**R1. One grant mechanism (A1).** A capability registers a **grant source**. The source answers one question: which `(modifier, rank, applier)` does this unit want, and has its answer changed since the last pass? `stats` owns one system that diffs each changed unit's wanted set against its held set, then applies or releases. The sources are action passives and holds, carried items, the unit type's passive, and later perks and equipment. Presence grants (auras, area insides, player modifiers) stay in `HeldPass`, because they are found by space each tick, not by the unit's own state. This removes `Holds::hold_passives` and `ItemHolds::hold_items` as loops.

The grant pass runs at each point where a source's answer can change today: as each tick starts, after the casts and attacks of Hit, after Resolve, and at the tick's end, as `hold_passives` runs now. Each run visits only the units whose source or modifiers changed, and the units that wait for a cooldown, as `ReadyWaits` keeps them. Two sources that grant one modifier from one source unit share one hold, as two items do now; the load still refuses a modifier that two kinds of source grant, which would make one release end the other's hold. Cost: medium. Check: neither golden column moves.

**R2. Core action anchors, and true needs (B1, B4).**
- `actions` owns the sub-stages that every action kind and delivery orders against: `Start`, `Deliver`, `Launch`, `Land`, `Resolve`. `CombatSet` keeps only combat's own steps: the damage pass, deaths and respawn.
- Weapon shots go through a delivery request queue in `actions`, so `projectiles` stops reading combat.
- Each row of the capability table then lists what its code needs: `vision`, `abilities`, `areas` and `projectiles` need only the core, and `orders` needs `navigation`.

- One source states the graph (B5): the capability table's `needs`. A test derives each module's imports and fails when a capability imports a capability its `needs` do not name, or names one in `needs` that it neither imports nor orders its systems against. The layer table keeps only the base modules, and the overview's prose points to the table.

Cost: medium, mostly schedule sets. Check: neither golden column moves; the archetype-shuffle test and the schedule build's conflict check pass; the new test fails on today's `vision` row before the change.

**R3. Each capability owns its commands (B2, A5).** Commands already carry their owner's index (`00-overview.md`, Commands).
- `items` decodes `Buy`, `Sell` and `Swap` itself.
- `progression` decodes `Learn`.
- `production` decodes `CancelTrain`, `Rally` and `CancelBuild`.
- `orders` keeps only the orders a unit carries out: move, attack, slot, build, stop.
- `orders` keeps the shared parts: decoding the unit list and checking that the player controls the units. Each capability calls them.

Then a character-driven shooter buys with `items` alone. Each capability's commands apply in Inputs in a fixed order, the capabilities' table order, so a tick's buys and cancels apply as they do now. Cost: medium. The command bytes in the session logs change. The goldens should not move, as the commands apply in the same stage and order; a golden that moves must say why.

**R4. Engine tags by registration (B3).** Each capability reserves its engine tags at install, in install order. The tag book gives them their indices. The core keeps only `avatar` and the layer tags. The load still refuses a package that gives an engine tag. Cost: small. The tag numbers stay the same while the install order stays the same.

### Phase 2: production, generalized

**R5. Queued actions with any output (A3, C8).**
- Any unit may have `queue = { size }`. An action with `queue = true` joins the queue in place of a windup: it pays at once, waits its time at the head, then resolves with its `on_resolve` effects and hook.
- `train` becomes such an action with a `spawn` effect.
- Research becomes one with a player-modifier effect (`player_modifier = { id }`, a new effect).
- Craft becomes one with a `give_item` effect, planned with items.
- Cancel and refund move to the queue, so they serve all three.
- The `train` kind goes away, and the `craft` kind is never added. A unit that upgrades into another type is a later effect, `transform = { unit_type }`.

The rules of today's train queue become the queue's rules, for every queued action:
- It is checked as it joins, as a train is: its costs, its requirements and its capacities (R7). It is never under way, so its unit stays free and no tag that blocks `use` stops it.
- Its time runs from the tick it reaches the head, and it resolves in the Mode stage of the tick its time ends in, with its effects and hook, then the next entry starts in the same tick.
- A dead producer's queue waits; a producer that despawns takes its queue with it and refunds nothing.
- A `spawn` effect of a queued action places its unit by the train's spawn place rule, the producer's body point nearest its rally point and the nearest open cell, and sends it to the rally point. A `spawn` of any other action keeps its rule, the unit or point it applies to, so a summon does not move.

This follows StarCraft II's `CAbilQueue`, which trains and researches through one queue. Cost: large, as it changes production's data and the skirmish's plan. Check: production's train, cancel, supply and rally tests pass unchanged in their assertions, on data that says a train as a queued action.

**R6. One work mechanism (A2, C2).** A unit type's `work = { rates = [...], max_workers }` makes a unit that others work on.
- `rates` is the rate for each count of workers, from zero workers; a count past the table takes its last entry.
- `max_workers` limits the workers who count, and a worker past it is refused, as a second SCV is.
- The three styles become three tables: `alone` is `rates = [1]`; `builder` is `rates = [0, 1]` with `max_workers = 1`; `builders` is `rates = [0, 1, 1.62, 2.16]`.
- Progress, its rounding of life and its completion tick stay as construction defines them now.

A site is a unit with `work`. - Completion runs the type's `on_complete` effects and the mode's `on_work_complete(ctx, unit)` hook; a site's completion is today's rule, its `constructing` tag gone and its builders' orders ended.

Interaction's held uses with one side's progress become work too: plant, defuse, revive, a repair. A capture point needs more: progress by team, a contest when enemies stand on it, and a decay when no one works, as TF2's points have. That is an extension of work, `work = { by_team, contest, decay }`, planned with `interaction`; this proposal does not claim it. TF2's capture rate grows by a table of the count of cappers, which is the precedent for `rates`. Cost: medium. Check: the three styles' construction tests pass with the three tables, tick for tick.

**R7. Capacities (C1).** The mode declares `[capacities.<name>] max`. A unit type gives `capacity = { <name> = { uses, provides } }`. Each capacity is derived as supply is now: a pass over the owned units and the queues when the first check of a tick needs it. It never drifts, because it keeps no sum (D5 of design 11 holds for each). A queued action or a build lists the capacities it checks. Supply becomes one capacity, and power, population and housing are more. Scripts read `ctx.capacity_used(player, name)` and `ctx.capacity_max(player, name)`. Cost: small to medium.

**R8. Haul (C3, A4).** A unit type's `stock = { <resource> = { amount, max } }` gives a unit its own resource amounts. `gather` becomes `haul`:
- `from` is a filter of units with stock of the resource. A node is a unit with stock, and no `max`.
- `to` is a filter of units that take the resource. A drop-off whose stock is the player's own is today's `drop_off`.
- `take` is the most one trip carries. `windup_ms` is the time a trip takes at its source.

- A sink takes into one of two places, which its section names: `drop_off = { resources, into = "player" }` is today's drop-off, and `into = "stock"` adds to the unit's own stock, within its `max`.

The loop, its wait, its `bounce` and its tie rules stay as they are. A production chain is a queued action (R5) with a `stock_cost`, paid from its producer's own stock, and an output `stock = { resource, amount }` effect that adds to it: a smelter turns ore into bars. A cost from stock is a field of its own, not a name in `cost`: `cost` names pools and player resources, which never share a name, and a stock's resource is a player resource's name too, so one field for both would be ambiguous. RimWorld's haul jobs and the carriers of The Settlers move goods this way. Cost: large. This is the deepest change to the skirmish, because the node, the drop-off and the load change form.

### Phase 3: closed sets opened

**R9. Shops as units (C4).** A unit type's `shop = { items, resource, reach, buyers }` makes a unit a shop. `reach` is a radius, by the reach rule. `buyers` is a filter, and it may select the dead, which keeps League of Legends' rule as data. The mode's `[shop]` and its markers go away: the 3v3's fountain is a unit with a `shop` section. Warcraft III's shops are units, and a courier or a market is one too. Cost: medium. The 3v3's golden moves.

**R10. Points as track data (C5).** A track gives `points = [...]`, the points each level gives, or `points = 1` for each level. The `level` track's default stays one a level, so the MOBA keeps its data. A second track may give points too, for Skyrim's perks. Cost: small.

**R11. The player as an actor (C6).** Each player slot gets a **player unit**. It holds the mode's `[player] slots`, the player's actions, and it pays from its player's resources. It is made of rules that exist, not of exceptions in every system:
- It has no body and no pools, so it collides with nothing and no damage reaches it.
- It has the engine tag `player`. A filter's sets select it only when the filter names that tag, as they already treat `projectile`, `area` and `item`. So no query, area, aura or weapon finds it.
- Its own team sees it, as every unit is seen by its own group, so its owner receives its cooldowns and charges; other teams never see it.
- It stands at the centre of the map's bounds, which no rule reads, as its actions aim with global range. Its actions aim with global range, by the pipeline as it is: cooldowns, charges, costs, effects and hooks. So placement in tower defense, Generals' powers and card play need no second pipeline. Unreal's GAS gives the PlayerState an ability system for this. A `build` action of the player unit places a site with no builder walk, because its range is global. Cost: medium. A player unit takes a stable id, so the ids of later units shift. The implementation decides whether player units take ids only in a mode with `[player] slots`, so that other matches keep their ids and their goldens.

**R12. Movement that spends (C7).** Only a unit's own step pays: a push of collision and a forced move pay nothing, as neither is the unit's choice. A unit type's `move_cost = { pool, per_meter }` makes each of its steps pay from a pool. A step that the pool cannot pay is not taken, and the unit stands as a blocked unit does. This gives fuel, sprint stamina and a tactics game's move points, with a mode script that refills the pool each turn. Cost: small. The rounding is per step, exact, and kept as a pool's carry is.

### Guards: keeping it generic

**R13. Tests that keep the model generic.** A redesign that no test holds drifts back. Three guards:
- **The Hive mode.** The acceptance test becomes a test package, `packages/test/modes/hive`, with a scripted match and a golden, as each genre proof has. It declares no `combat` for its drones, a player action, haul chains, capacities, work and `move_cost`. It plays only after Phase 3, and a later change that breaks a part of the generic model breaks it.
- **No genre words in engine code.** A test reads the crate's production code, as the role test does, and fails on an identifier that holds a word of a deny list: `hero`, `creep`, `lane`, `tower`, `jungle`, `mineral`, `gold`, `mana`, `barracks`, and others the list grows by. Today the production code holds none; test modules may.
- **The needs table is the graph** (R2): the test of imports against `needs`.

## How each change is checked

The goldens' two columns are the guard. The state column follows the layout; the behaviour column follows what units do: positions, pools, deaths, failures and resources ([Goldens](../docs/design/02-engine-core.md#testing-and-diagnostics)).

| Change | State column | Behaviour column |
| --- | --- | --- |
| R1, R2, R4 | Must not move | Must not move |
| R3 | Must not move: the commands change bytes in the log, not state | Must not move |
| R5, R6, R7, R8 | Moves where production's state types change | Must not move for the proving match; the skirmish is new |
| R9 | Moves: a shop becomes a unit | 3v3 moves only by the new unit's id, if at all |
| R10, R12 | Must not move: the defaults keep today's rule | Must not move |
| R11 | Must not move in a mode with no `[player] slots` | Must not move |

A change that moves a column it must not move is a bug in the change, not a new golden. The snapshot tests sweep the new state types of R5 to R8 as they sweep the skirmish's (design 11, D14).

## What stays as it is

- The action pipeline, the effect registry and its lists.
- Stats, pools, modifiers and tags, with their formula and properties.
- Relations, filters, the reach rule, bodies and layers.
- Deliveries as units, and scripted systems.
- `combat`'s damage pass, events and respawn.
- `navigation`'s routes, groups, forced moves and collision.
- `vision`'s grid fog, brush and detection.

These already pass the three questions.

## Order and its effect on Stage 7

| Phase | Changes | Goldens | When |
| --- | --- | --- | --- |
| 1 | R1, R2, R3, R4 | None expected | Before Stage 8, which needs R3 for the CS buy menu |
| 2 | R5, R6, R7, R8 | The skirmish is new, so nothing moves if Phase 2 lands first | Before R9 of the skirmish plan, the skirmish's match and golden |
| 3 | R9, R10, R11, R12 | The 3v3 moves for R9 | With the genre proof that first needs each one |
| Guards | R13 | The Hive golden is new | The genre-word test and the graph test at once; the Hive mode after Phase 3 |

Each change also changes `docs/design/04-capabilities/` and the script API reference. Each one updates `08-script-api-reference.md` from the registry.

## Decisions for the user

1. **Phase 2 before the skirmish?** Option A: do Phase 2, then the skirmish, so its golden is built once (recommended). Option B: do the skirmish on today's model, then re-bless it after Phase 2.
2. **R8's scope.** Option A: full haul with unit stock and chains (recommended for genericity). Option B: keep gather and add only a sink that is a unit, which costs less and serves the skirmish and a delivery game, but not chains.
3. **R11's form.** Option A: a player unit with no body, which reuses the whole pipeline (recommended). Option B: a second actor kind, the player, in the pipeline, which needs every check to know two kinds of actor.
4. **R9 and the MOBA.** Is the MOBA's shop rule worth a golden change now, or only at Stage 10 with the reference game?
