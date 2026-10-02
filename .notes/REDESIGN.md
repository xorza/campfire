# Structural redesign

This is a proposal for review. It answers `REVIEW.md` (159 items) and `TEST-REVIEW.md` (about 90 items) at commit `48f1e16`. When you agree to a part, that part moves into `design/` and `PLAN.md`, as the workflow in `AGENTS.md` requires, and this file loses it.

Most findings are not separate defects. They come from a small number of structural causes. This file names each cause, proposes the shape that removes it, and states the rule that keeps it removed. Then it gives a plan that can run safely: each step proves that it changes no behaviour, or names the change it makes. The appendix maps every review item to the step that closes it, so nothing is lost.

References: `R§n` is group n of `REVIEW.md`, `T§n` is group n of `TEST-REVIEW.md`, and a step id such as `C5` is a step of the plan. Paths are relative to `source/crates/`.

## Summary

| Part | Root cause it removes | Main groups |
|---|---|---|
| R1 Immutable books, built at load, shared | Package data is copied into the view, the frame and the state, and looked up by name after the load | R§7, R§9, R§11, R§12 |
| R2 One load pipeline | The load check and the match build each validate and compute | R§6, R§1, R§9 |
| R3 Strict layers with registered hooks | Lower modules import higher ones, and the core names every capability | R§8, R§10 |
| R4 One rule of unit life and reach | "Living", "dead", "target" and "within reach" have several definitions | R§3 |
| R5 Stable order and one exactness rule | Some systems spend or allocate in query order, and some arithmetic rounds its own way | R§4, R§12 |
| R6 A limit on work per tick, and fresh shared indexes | Navigation, vision, deliveries and the view have no work limit and no shared index | R§5, R§12 |
| R7 State holds only state, and a restore is checked | Components store book data, and decoded state is trusted | R§1, R§7, R§10 |
| T Proof and test redesign | Refactors have no permanent proof of equal behaviour, and every module writes its own harness | T§1 to T§8 |

## Rules

These rules state what "fixed" means. When agreed, they go into `design/02-engine-core.md`. The last column names the test that fails when the rule is broken. A rule that only a review can check is a rule that drifts again.

| # | Rule | Enforced by |
|---|---|---|
| 1 | One owner for each fact. A fact from the packages lives in one immutable book, a fact of the match lives in state, and a fact of the running call lives in the frame. Nothing else holds a copy. | The state table test (G1) and the behaviour golden (A3) |
| 2 | A name becomes an id where it enters. After the load, no system looks up a name. A script call resolves its name once per call, with no allocation. | `Books::build` tests (C5), and the allowlist test of name lookups (C7) |
| 3 | The load refuses everything that a match can refuse. A match start fails only on session terms: players, tick rate and seed. | `StartError` has no data case (C5b) |
| 4 | A layer calls a higher layer only through a hook that the higher layer registers. | The layer test (A4) |
| 5 | Every order that matters is by stable id, and every rounding uses one helper. | The archetype-shuffle test (B2) |
| 6 | Each tick's work has a fixed limit, or a cost in proportion to the units that take part. No tick pays for a scan or a rebuild that the other ticks do not pay for. | The worst-tick record (A3) and the navigation bench (H2) |
| 7 | Restored state is checked like package data: a restore gives an error for every flaw, and it never panics. | The state table test (G1) and the snapshot fuzz (G1) |
| 8 | Each rule of a network session has one owner on each side. A client that follows the rules is never refused. | The net scenarios under load (A1, B4) |

## R1. Immutable books, built at load, shared

### Problem

Each match has books: `ActionBook`, `ModifierBook`, `StatBook`, `PoolBook`, `TrackBook`, `UnitTypes`, `TagBook` and `ModeBook`. Parts of each book are copied into other places:

- **The script view** keeps `ModifierInfo`, ability names, `delivers`, track names, and stat, pool, resource and damage-kind names. Those names come through `MatchScripts`, which the runner builds apart from the books.
- **The frame** holds every action's and modifier's param tables, and stats reads those tables through the frame.
- **The state** holds book data: `Instance.tags` and each change's stat and op, `TrainQueue.capacity`, `PlayerResources.resources` and `InProgress.kind`.
- **Several places** resolve names again at run time: aura filters every tick, `unit.stat(name)` on every call, and the stat list, which is held three times.

The copies stay in line only through load order and a `debug_assert`. The client needs the same tables, which today has no way to build them.

### Shape

- **Each book is built once, fully resolved, and never changed after the load.**
  - Each book stays in the module that owns it, as `ActionBook` lives in `actions` today.
  - Books are plain data: `Send`, `Sync`, and with no Rhai value and no `ScriptId`.
  - The match holds each book as `Arc<Book>` in a `Resource`. The view, the frame, the stats engine and the client read the same `Arc`, and keep no copy.
- **Books need no script host.**
  - A script is named by a `ScriptPlace { package, index }`. The match host compiles the package scripts in that order at install, so a place maps to a `ScriptId` by position.
  - The hooks that a script defines come from its `ScriptFacts`, which read function names and arity from the AST.

  This way, the load, the verifier and the client build the same books with no engine.
- **Values that scripts read are cached once per match on the script side.** Mode params, marker params and the `Dynamic` forms of names are Rhai values. Rhai values are not `Send`, so they cannot be in an `Arc` resource. One `ScriptConsts` in the non-send script side builds them once at install, from the books.
- **Every name becomes a typed id at load.**
  - The ids are `StatId` (new), `TagId` (today's `Tag`), `UnitType`, `ActionId`, `ModifierId`, `PoolId`, `ResourceId`, `TrackId`, `DamageKind` and `PathId`.
  - Each id comes from one list in one book. That list also gives the name back, for errors and scripts.
  - A script call finds an id by a binary search with a borrowed `&str`, which allocates nothing.
- **A modifier becomes one runtime spec.**
  - Its stat changes are `(StatId, op, NumberRef)`, where `NumberRef` is a value or a param place.
  - Its filters and its aura's modifier are resolved.
  - Its tags are a `TagSet`.
  - Its param table sits in the book.

  `ModifierBook::application` then needs no `&StatBook` and no names.
- **An action becomes a kind that carries its data.**

  ```rust
  enum ActionKind { Cast, Attack(Weapon), Train { unit: UnitType } }
  struct Delivery { unit_type: UnitType, shape: DeliveryShape }
  enum DeliveryShape { Projectile(Fan), Area }
  ```

  `spawns`, `weapon: Option<Weapon>` and the `expect`s that hold them together go away. The same table of kinds also says which data fields each kind takes. The load check, the script API registry and `ActionData` all read that one table.
- **A unit type becomes one entry that carries its role.**

  ```rust
  enum TypeRole { Unit(UnitKit), Projectile(ProjectileSpec), Area(AreaSpec) }
  ```

  - "Spawn a projectile type" then cannot be expressed.
  - `ByType<ProjectileSpec>` and `ByType<AreaSpec>` go away.
  - The type's tags are held once, in this entry.

  The layer rule blocks this shape: `UnitTypes` is in `units`, and a role that holds `UnitKit`, `ProjectileSpec` and `AreaSpec` imports three higher layers. So each capability keeps its `ByType` book, and an action's delivery says whether its projectiles home (C8). The one store of type tags waits for the test harness (T§2.1): the test worlds build their tag books after they load types, which a single store refuses.
- **Engine tags take reserved `TagId`s:** `avatar`, `projectile`, `area` and, later, `item`. A tag name is a checked `TagName` newtype, so `a:b`, `!x` and `""` fail where they enter. A package that declares an engine tag fails at load.

### What goes away

- `MatchScripts`, which the books replace.
- The view's `ModifierInfo`, `ability_names`, `delivers`, `track_names` and `stat_names`.
- The frame's param tables.
- `UnitTypes.types[].tags`, `TagBook.type_tags`, and `ModeBook.bounds`.
- `StatBook`'s copy of `TickRate`, and the kit's second copy of the stat formulas.
- `Filter::parse("enemies")`, which a `Filter::of(Relation)` constructor replaces.

## R2. One load pipeline

### Problem

- `LoadCheck` validates the packages, then throws away what it computed. `MatchBuild` computes the same values again, with about 20 `.expect(CHECKED)`.
- Some errors show only at match start: `UnitKitError`, `ActionError::TimeTooLarge` and `AiError::NoThink`.

### Shape

```
PackageDir --read once--> PackageFiles     bytes in memory; the fingerprint is over these bytes
           --parse------> PackageData      manifest + PackageContent { actions, modifiers, units, scripts }
ModePackages::load -----> Books::build(&ModePackages) -> Result<Books, LoadProblem>
Match::install(world, &Books, SessionTerms)    cannot fail on data
```

- **Building is the check.**
  - `Books::build` is the only code that turns data into books, and every refusal is an `Err` that it returns.
  - It builds the kits, the actions and the AIs with the same functions the match uses. So `UnitKitError`, `TimeTooLarge` and `NoThink` move to the load, and they keep their tests.
  - It is a pure function of the packages, so it can be tested with no world.
- **Times are checked at the fastest tick rate in the manifest's range.** That rate gives the largest tick counts, so a time that fits at that rate fits at every allowed rate. The tick values themselves are derived at match start, from the session's rate. That derivation cannot fail.
- **`ModePackages` holds the `Books`.** `MatchBuild` becomes `Match::install`, with no `.expect(CHECKED)`. `StartError` keeps only the session-term cases.

  C10 kept the books in ticks: books in milliseconds would move a conversion into every use, a projectile's flight each tick among them. So the load builds the books at the fastest rate to prove them, as design 02 says, and `ModePackages::books` builds them at a session's rate, where the proof makes the build infallible; `ModePackages::compile_scripts` and `ModePackages::mode_script` hold the load's other proofs, so `MatchBuild` expects nothing.

## R3. Strict layers with registered hooks

### Problem

The layer rule in `lib.rs` is broken in several places:

- The core view names every capability.
- `combat` holds `projectiles`' launch list.
- `actions` and `combat` import each other, and so do `production` and `mode`.
- Player resources live in `mode`, the top layer, and lower layers import them.
- The script runtime's `Effect` enum dispatches to seven capabilities.
- `abilities` registers the delivery API, but `deliveries` runs the hooks.
- The running call's package lives in the view, beside the frame.
- Player orders and AI orders are applied by two functions, which check different things.

### Shape

The new order of layers, lowest first. A module imports only from the layers below its own:

```
values
core:      units (types, teams, owners, spawner, script view core), scripts (runtime, frame, effect order), players (resources)
actions:   the action pipeline, kind rules, the target rule (`Targets`), costs, `Purse`, the order applier
           stats, combat
           deliveries, projectiles, areas, abilities, navigation, vision, progression, production
           orders (AI)
mode
capability_set: install, the effect dispatch table, the view column table, the layer table
```

**Decision (D2): `stats` sits below the action pipeline, and `combat` above both.** `stats` needs nothing of `actions` but the id of an action, which moves to the core; the pipeline needs pools, costs and the passives' modifiers. So the layers run `values`; the core; `stats`; `actions`; `combat`; the capabilities above. For this, `ActionId` and `Dead` move to the core, `DamageKind` to `values`, the life pool becomes its own resource in `stats`, `MoveStep` moves to `stats` as the component that holds the move speed's effect, the modifiers' combat hooks move to `combat`, and each ordering of sets is stated by the higher layer.

The parts that call upward use hooks that the higher layer registers:

- **Effects.**
  - Each capability defines its effect type, and registers an `EffectKind` at install.
  - The frame keeps one typed queue for each kind, reached by `Frame::queue::<E>()`, and one order list of `(EffectKind, index)`.
  - `capability_set` registers one `fn(&mut World, &mut Frame, index)` for each kind. `ctx.apply` walks the order list, so effects still apply in the order queued.

  This needs no boxing and no allocation per effect, and `scripts` names no capability.
- **Action kinds.**
  - The action pipeline moves below combat. It owns `start_actions`, `hold_passives`, `Targets`, `Purse` and one `pay`.
  - Each kind registers its rules: its start check, its windup, and what it does when it resolves. Combat registers the attack kind, abilities the cast and production the train.
  - Every kind pays its whole cost at one point of the pipeline.
- **Orders.** One applier in `actions` checks and applies each order kind for every source: a player's command, a bot's input and an AI effect. An attack order needs a weapon whose filter selects the target, through `book.weapon_for`. This follows control.md: "players, bots and AI issue the same orders".
- **View columns.**
  - `UnitRow` keeps only the core: id, position, team, type, owner, tags, life flags and radius.
  - Each capability owns a column, which is a flat `Vec` by row, and the script getters that read it. The capability registers both through the `RowSource` it has today.
  - `RecentAttack`, `SlotRow`, `ModifierRow` and the stacking logic leave the core.
  - A `Unit` handle holds its row index, so a getter copies no row.

  This is what design 04's overview says: "the core names no capability".
- **The delivery script API** is registered where it runs: `deliveries` registers `on_hit`, `on_end` and `Hit`. `ApiOwner::Projectile` and `ApiOwner::Area` go away, because `hit.delivery` is a `Unit`.
- **The call's package moves into the frame.** `CallStart { role, acting, action, rank, package, depth }` is the one argument of `Frame::begin`. `view.set_caller`, and the four call sites that must remember it, go away.
- **One table of capabilities.** The install order, `needs`, the layer of each module and the list in `lib.rs` all come from one table in `capability_set`.

## R4. One rule of unit life and reach

### Problem

- `Targets`, `View::living` and `Combat::living` give three answers to "is this a living target".
- A unit with the life pool and no `[combat]` dies and is never dead.
- `die` leaves a cast under way.
- Script queries and auras measure from centres. Combat, areas and line projectiles measure from body edges. Homing projectiles hit at the centre.
- A new unit is not seen by its own vision group in its first tick.

### Shape

- **The life rule.**
  - A unit type with the life pool needs a `[combat]` section. `Books::build` refuses one without it. No reference unit type breaks this rule today.
  - `Combat::die` is the one place where a unit dies. It inserts `Dead`, records the death, clears the attack target, and stops every action under way or ordered (`ActionSlots::stop`).
  - Navigation and orders react to `Added<Dead>` with a query of their own. They need no hook from combat.
- **The target rule.** `Targets` lives in the `actions` layer, and it is the only predicate of a living target. The view's row holds the life-pool flag and reads the same rule, so `ctx.find`, `nearest_visible` and a cast's check at delivery agree with combat.
- **The reach rule.** One `Reach` helper decides every reach, edge to edge, in the map's metric:

  ```rust
  Reach::within(metric, from, from_radius, range, to, to_radius) -> bool
  ```

  Combat, areas, line projectiles, auras, script queries and homing hits all call it. A homing projectile then hits when its body touches the target's body, not at its centre.
- **The aura rule.** An instance of 0 stacks, or one that an immunity suppresses, projects no aura. stats.md states the rule.
- **The vision rule.** A new unit is seen by its whole group from its spawn: `Vision::seen_by` uses the group, not the team.

Edge-to-edge queries make Lash Out, Wildfire, Tempest and the auras reach up to one body radius farther. The reference content may need new radii. This is decision 2.

## R5. Stable order and one exactness rule

### Problem

- `start_trains` and `strike` pay shared player resources in Bevy query order, which a restore can change.
- Three systems walk the whole `EntityIndex` to get an order, and they visit projectiles and areas too.
- Line hits are ordered by a share cut to 24 bits.
- `ParamTable` rounds ties away from zero, against design D1.
- Script directions and homing steps ignore the map's metric.
- A fan's turn multiplies a rounded degree.
- RNG streams are named by strings.
- `ctx.projectile` and `ctx.area` take their unit's id only later, so they return `()`, and `chain_fire.rhai` cannot use the result.

### Shape

- **An `Ordered` system param.** It collects the `(StableId, Entity)` of a query into a `Local`, sorts them, and gives them in that order. Every system that spends something shared, takes ids or runs scripts uses it:
  - `start_trains`, `strike`, `resolve_casts`, `think` and `finish_trains`;
  - the delivery systems: `fly`, `trigger`, `launch` and `land`.

  The `EntityIndex` walks go away. Each system then touches only the units it concerns, and those still in stable-id order.
- **The archetype-shuffle test.** It plays the proving match (A2) twice. The second run gives half of the units an inert marker component, which changes their archetypes and so the query order. The two behaviour traces must be equal.
  - This test needs a match that runs production and two producers of one player. The 3v3 has no production, so the test uses the proving match, not the 3v3.
  - It lands with `Ordered` (B2). Before that step, it fails on `start_trains`.
- **Ids at call time.** A unit that a call creates takes its id when it is queued, as `spawn_unit` does today. Calls already run in a stable order, so the ids stay in a stable order. `ctx.projectile` and `ctx.area` then return a `Unit` handle that the same call can use.
- **One rounding helper.** `Num` gains `mul_div(a, b, c)` with one rounding, and one `round_ties_even` for wide sums. `ParamTable`, `StatTotals` and the fan turn use them.
- **The metric everywhere.** Script directions are normalized through `Metric::offset`. Homing steps and `flown` are measured in the metric. Line hits sort by the raw `along`, and they divide only to place the hit point.
- **RNG streams** are a closed enum, `RngStream`, with fixed bytes for each. The debug build's set of `String` names goes away.
- **The trig claim.** Design 09 states the error bound that the tests prove, not "correctly rounded".

## R6. A limit on work per tick, and fresh shared indexes

### Problem

- Navigation has no work limit for steering, the region rebuild, smoothing or `Regions::nearest`, and one walker can scan about 3·10⁹ cells.
- Vision clears bitmaps by map size times groups.
- Deliveries, auras and script queries test every unit.
- Every script batch rebuilds every row.
- Change detection on `Modifiers` is too coarse.

### Shape

- **`NavBudget`, one limit per tick.**
  - Units of work are charged by the long planner, by smoothing segment tests, by region scans and by steering plans.
  - Waiting work keeps its ask tick, so a new ask does not move a chaser to the back of the queue. Work waits in ask order, then by stable id.
  - A short route searches only its window.
  - A route that ends short stays "arrived short" until the static bodies change, so a walker does not ask again every tick. A removal of bodies asks again for unreached routes.
  - The steering blocker query reaches the window plus the walker's radius, and it clips each blocker's span to the window.
  - `Progress` resets when a route is asked for or cleared.
  - The exact tests also read the map's blocked cells.
  - The steering algorithm becomes methods on `Steering`, with private fields.
- **Incremental regions.**
  - Each region keeps its cross-chunk edges. A change rebuilds only the dirty chunks and their edges, then renumbers the reachable sets over the region graph, as 0 A.D. does.
  - Routes are tested again only against the bodies that were added.
  - The body index stores a first-cell flag in each entry, and its visitor can stop early.
- **`BodyGrid`, one fresh index of living bodies.**
  - It is a sorted cell index with the shape the broadphase uses.
  - It is rebuilt on its first read after any change of positions, bodies or the set of living units. So every reader sees the current positions, including units spawned in the Mode stage, and a tick rebuilds it at most once for each stage that reads it.
  - Line projectiles read it by the box of their segment. Areas and auras read it by their circle, and so do `ctx.find` and `nearest_visible`. Stuck walkers use it to find each other.
  - The P × U loops and the O(W²) search go away.
  - `struck` and `CastHits` merge into one sorted flat store of `(group, unit)`, where a type without `once_per_cast` uses the projectile's own id as its group.
- **Vision.**
  - Each group keeps a list of the words it set, and the next tick clears only those words.
  - A detection bitmap exists only for a group with a detector.
  - The load limits vision groups to 64, as vision.md says.
- **The view: incremental rows, the same snapshot.**
  - A batch must keep today's meaning: its calls see the world as it was when the batch began. So rows are not filled lazily on read, because a late read would see the effects of earlier calls.
  - The view keeps its rows from one build to the next. Each build refreshes only the rows whose source components changed since the last build, through Bevy's change ticks, and the rows of new or gone units.
  - A batch with no call builds nothing.
  - `deal_damage` reads the view only when the damage queue is not empty.
  - Filters are parsed once for each call site, and kept.
- **Change detection.** `Modifiers` splits in two:
  - `ModifierStats` holds the instances, stacks and changes. Only these trigger a stats refresh.
  - `ModifierClock` holds intervals, shields and state.

  A DoT tick or a shield absorb then does not re-derive the unit's stats.

## R7. State holds only state, and a restore is checked

### Problem

- A restore decodes ids and places that it never checks against the books, and a bad stat place corrupts another unit's totals.
- `Choices`, `ModeState`, `Timers`, `PlayerResources` and `Experience` decode with no check.
- A resource that the snapshot lacks stays in the world.
- An instance that is both held and timed loses one of its two lifetimes.

### Shape

- **A check for each registered type.** `StateRegistry` lives in `sim`, which cannot name the books. So the check is generic:

  ```rust
  register_component_checked::<T>(check: fn(&T, &World) -> Result<(), RestoreError>)
  ```

  and the same for resources.
  - A restore decodes everything, then runs every check, and returns the first error.
  - Every type that holds an id, or a shape that the packages fix, registers a check that reads its book from the world.
  - The state table test (G1) proves that every registered type has a check.
  - A fuzz of snapshot bytes proves that no restore panics.
- **Book data leaves the state** (R1):
  - `Instance` keeps only its values, and reads tags and `(stat, op)` from the book by id;
  - `TrainQueue` reads its capacity from the kit, and stores each entry's time at push;
  - `PlayerResources` reads its count from the books;
  - `InProgress` reads its kind from the book;
  - `Experience` keeps only experience for the level track, and `Level` alone holds the level.
- **An absent resource is removed on restore.**
- **The lifetime of a modifier.**

  ```rust
  struct Lifetime { holds: HoldSet, until: Option<Tick> }   // HoldSet: Passive | Aura | Area | Player, as bits
  ```

  - An instance ends only when no hold remains and its time is up. A timed copy over a held copy keeps both lifetimes.
  - The `passive` and `held` bools go away, and `renew` takes the new interval.
  - One `ParamSource::of` builds a unit as a param source for every caller.
  - The consumer of `HeldModifiers` clears it after `apply_held`.
- **Flat storage.** The nested `Vec`s of `Modifiers` become one flat buffer per carrier with ranges. This is possible once the book data leaves the instance.

## T. Proof and test redesign

A refactor of this size needs a permanent proof that behaviour stays the same. The one-time trace comparisons of the earlier steps go away with their scratch copies. The proof must live in the suite.

- **Two goldens, not one.**
  - **The state golden** is a `HashTrail` of the state hash at each tick, pinned as BLAKE3 digests. It changes when the state's layout changes. R1, R7 and I1 change the layout on purpose, without a change of behaviour.
  - **The behaviour golden** is a trace that does not depend on the layout. At each tick it records:
    - each unit's id, type, team, position, pools and death;
    - each death, with its killer;
    - each damage, with its source, target and amount;
    - each script failure, with its unit, hook and error kind.

    It is pinned as one BLAKE3 digest per tick, so a failure names the first tick and the unit that differ.
  - **The rule.** A step that changes only the layout updates the state golden, and keeps the behaviour golden. A step that changes behaviour updates the behaviour golden, and names the change.
- **The proving match** (A2). It is a mode in `packages/test` that uses every capability the release runs, with scripted bots: combat, stats, abilities with projectiles and areas, production with two producers of one player, progression, navigation with static bodies that change, and vision with several groups. It is small, so it runs in under 0.3 s. The goldens, the archetype-shuffle test, the state table test and the work records run on it, and they do not change when the MOBA's balance changes (T§6).
- **The work record.** At each stage's end, record two measurements in the stage's PLAN notes: the instruction count of the proving match and the 3v3 (`perf stat -e instructions:u`), and the worst tick against the mean tick. A stage that makes either worse by more than 10 % names why.
- **The structure tests.**
  - The layer test (A4) reads each module's `use crate::` lines and checks them against the capability table. It starts with today's breaks as a list of known exceptions, and each step of Stage D removes its own.
  - The archetype-shuffle test (B2).
  - The state table test (G1).
  - An allowlist test of name lookups (C7).
- **The harness.** `TestMatch` becomes the running harness, and data constructors sit beside their types (T§2.1, T§2.2). `Arena` and `FixedSession` come after the books (T§2.4). The detailed items stay in `TEST-REVIEW.md`, and the appendix maps them.

## Implementation plan

### How each step runs

- Each step ends with the full check chain, both goldens, and the structure tests that exist at that point. It stops for your review, as `AGENTS.md` asks.
- **No step is larger than M.** A larger change is split so that each part leaves the tree green.
- **A structural change goes in three moves:**
  1. Build the new shape beside the old one, and check that both give the same answer.
  2. Switch the readers to the new shape.
  3. Delete the old shape.

  So the goldens prove each move, and a move can be stopped with no broken tree.
- **Design text.** Each step lists the design files it changes. Where the code and the design disagree, the step fixes one and says which.
- **Behaviour changes.** A step marked "changes behaviour" names the change in its review, and updates the behaviour golden in the same diff.
- **Temporary code.** A Stage B fix that a later step replaces says so in its commit. The later step deletes it.

### Stage D: layers (track S)

| Step | Change | Size |
|---|---|---|
| D2 | One order applier for players, bots and AI. The path orders (`FollowPath`, `Reset`) are navigation's, above `actions`, so the applier needs a hook for them. | M |
| D5 | View columns for each capability; `Unit` handles hold row indexes; the delivery API registered by `deliveries`, and the two `ApiOwner`s go | M |

Done when the layer test has no exception left.

### Track I: independent steps

These need only Stage A and Stage B. They can run between the steps of track S, in any order that their arrows allow. Each is small enough to fill a session.

| Step | Change | Needs | Size |
|---|---|---|---|
| E1 | The view reads the target rule; navigation and orders react to `Added<Dead>` | B | S |
| E2 | `Reach` everywhere: queries, auras, homing hits; the aura rule; vision from spawn; new reference radii if decision 2 asks for them | E1 | M, changes behaviour |
| F2 | `mul_div` and `round_ties_even`; the metric in scripts and homing; hit order by `along`; `RngStream`; ids at call time, so `ctx.projectile` and `ctx.area` return handles; the trig bound in design 09 | B | M, changes behaviour |
| G1 | `register_*_checked` and a check for every type; absent resources removed; the state table test; the snapshot fuzz | A | M |
| H1 | `BodyGrid`, read by deliveries and auras; one `(group, unit)` hit store; stuck walkers through the grid | B | M |
| H2 | `NavBudget`; short routes within their window; "arrived short"; re-asks after removals; the blocker query and clipping; `Progress` reset; blocked map cells in the exact tests; `Steering` methods; `Route::clear` keeps its buffer | B3 | M, changes behaviour |
| H3 | Incremental regions; routes tested only against added bodies; the body index's first-cell flag and early stop | H2 | M |
| H4 | Vision dirty words, detectors only, and the group limit; incremental view rows; filters parsed once | B | M |

### Joins of the two tracks

| Step | Change | Needs | Size |
|---|---|---|---|
| G2 | Book data out of the state; `Lifetime`; one `ParamSource::of`; flat `Modifiers`; the `ModifierStats` and `ModifierClock` split; the queue's times at push | G1 | M, changes the layout |
| H1b | `ctx.find` and `nearest_visible` read `BodyGrid` | D5, H1 | S |
| J | The local fixes in the appendix, and T§5 to T§8 | any time | S each |

### Order

```
Track S:  D2 → D5

Track I:  E1 → E2      F2      G1      H1, H4      H2 → H3

Joins:    G1 → G2      D5 + H1 → H1b
```

Track S is long and sequential. Track I fills the sessions between its steps.

## Risks

| Risk | Mitigation |
|---|---|
| A refactor changes behaviour without notice. | The behaviour golden over three matches, on every step. |
| The golden is updated so often that it stops being a proof. | Two goldens. A layout-only change may touch only the state golden. A behaviour change must name itself. |
| Performance falls as layers and indirections grow. | The work record at each stage's end, with a 10 % limit to justify. `Ordered` sorts only the units of its query. Columns and `Arc` books remove copies. |
| `Arc` books cannot hold Rhai values. | Books are plain data, and `ScriptConsts` holds the Rhai values on the non-send side. |
| Bevy's ambiguity check misses new orderings, as the logged `mode_inputs` issue shows. | Every new system names its order against the sets it touches. The archetype-shuffle test catches a hidden dependence on query order. |
| Track S blocks the roadmap for a long time. | Track I runs between its steps. |
| The proving match becomes a second content set to maintain. | It is small, it is owned by the tests, and it has no balance to keep. |
| Design 05 changes for input spill, and logs of earlier builds stop replaying. | The release check already refuses logs of another release, and no backward compatibility is kept. |

## Decisions for you

1. **Unit type names across packages.** A mode script names the mode's types and the avatars. A package's delivery types are seen only by that package's actions. So a mode script cannot spawn a hero's delivery type. I recommend this: the reference scripts spawn only mode types and avatars.
2. **Edge-to-edge reach for script queries and auras** (R4). The choices:
   - adopt it, and retune the reference radii;
   - keep centres for scripts, and change the design text.

   I recommend adopting it. One reach rule is what players read.
3. **The effect dispatch** (R3). Typed queues for each capability, as proposed, or keep one `Effect` enum and move it with its dispatch to `capability_set`. With the enum, `scripts` still needs the enum's type, so the cycle stays. I recommend the typed queues.
4. **View columns** (R3). Columns for each capability, as proposed, or change design 04's overview so the core may name capability fields. Columns cost more code now. They are what the client's actions and every later capability (items, interaction) need. I recommend columns.
7. **An attack's damage names its weapon** (R§11). Change the code (`d.ability` is the weapon), or the design (an attack names no action). I recommend the code: a weapon is an action in design 04.
8. **The proving match** (T). A new mode in `packages/test` that uses every capability. I recommend it. The 3v3 cannot test production or two producers, and its balance changes break unrelated tests.
9. **The test review's decisions** (T§9):
   - **Items 1 to 3:** the harness shape, the home of harness types, and the gate. I recommend a named field (`walk.sim.step()`), a gated file of its own for each harness type, and `any(test, feature = "internals")`.
   - **Items 4 to 10** are independent of this redesign.
10. **Stage B first.** Some Stage B fixes are temporary: C2, C8 and D2 replace their code. I recommend doing them anyway, because they are crashes and wrong results today, and each is small.

## Appendix: where each item goes

### REVIEW.md

- **R§1:**
  - spawn of a delivery type: B1;
  - two names for one type: B1, final in C2;
  - modifier data on restore: G1;
  - mode state decode: G1;
  - absent resource: G1;
  - package count: C2;
  - unlimited work before a refusal: B4;
  - lobby crash: B4;
  - several seats: B4.
- **R§3:**
  - cast survives death: B1;
  - die and never dead: B1;
  - three living rules: E1;
  - queries from centres: E2;
  - aura from centre: E2;
  - homing at centre: E2;
  - vision in the first tick: E2.
- **R§4:**
  - train order: B2;
  - homing and line types: B2;
  - truncated share: F2;
  - ties away from zero: F2;
  - metric: F2;
  - fan degree: F2;
  - trig claim: F2.
- **R§5:**
  - stall and lost order: B3;
  - vision bitmaps: H4;
  - steering budget: H2;
  - route budget: H2;
  - region rebuild: H3;
  - routes tested again: H3;
  - chaser starved: B3;
  - unreachable waypoint: H2;
  - short route never planned again: H2;
  - blocker reach: H2;
  - blocker cells: H2;
  - stuck walkers O(W²): H1;
  - body index waste: H3;
  - `Progress`: H2;
  - map-blocked cells: H2.
- **R§6:**
  - name table: C4;
  - `StateSync`: C3;
  - verifier release: C1;
  - one bad package: C1;
  - three read passes: C1;
  - `UnitTypeError`: C5b;
  - `declare`: C3;
  - `Scalar::Int`: C3;
  - life pool placeholder: C6a.
- **R§7:**
  - instance book data: G2;
  - package data in state: G2;
  - level twice: G2;
  - tags twice: with the harness (T§2.1);
  - call package: D4;
  - cast kind in state: G2;
- **R§8:**
  - core names every capability: D5;
  - the view names progression: D2;
  - script runtime: D4;
  - delivery API registration: D5;
  - layer list: A4.
- **R§9:**
  - tag names: C3;
  - action strings: C3;
  - mode and map strings: C3;
  - RNG streams: F2;
  - default filter: C6a;
  - `PackagePath`: C1;
  - `LoadError` path: C2;
  - avatar display name: C2.
- **R§10:**
  - two order appliers: B3, final in D2;
  - Mode-stage events: B2;
  - `finish_trains`: B2;
  - `renew` interval: B2;
  - live change from a gone source: G2, with the rule in stats.md (J);
  - param source three ways: G2;
  - held and timed copies: G2;
  - ms to ticks: C6a;
  - "walks" twice: C2;
  - hook arity: C4;
  - constant in a message: J;
  - damage-kind limit: C3;
  - `walk_to`: J;
  - `IndexedBody`: J.
- **R§11:**
  - `Delivered` hooks: J (`enum Reach`);
  - placeholder group: D3;
  - `Hit::direction`: F2;
  - `passive` and `held`: G2;
  - `UnitKit`: C5a;
  - `Package` data: C2;
  - load error shapes: C5a;
  - small shapes: J;
  - train queue times: G2;
  - kill by last death: J;
  - `ctx.projectile` returns `()`: F2;
  - `HeldModifiers` clear: G2;
  - capability set table: A4.
- **R§12:**
  - no spatial delivery query: H1;
  - batch rebuilds: H4;
  - `Modifiers` change detection: G2;
  - entity-index walks: B2;
  - `cast_hits.keep`: H1;
  - `struck`: H1;
  - `RecentAttackers`: J;
  - client gauges: J;
  - zero-stack live change: G2;
  - commands parsed twice: J;
  - small scans: J.
- **R§13:**
  - `Route::clear`: H2;
  - stable sort: J;
  - applied handle: D5;
  - nested `Modifiers`: G2;
  - fan allocation: H1;
  - scripts parsed twice: C5b.
- **R§14:**
  - one struct per file: D2, D5, J;
  - visibility: J;
  - tuple return and arguments: D4 (`CallStart`), J;
  - `Debug`: J;
  - `const fn`: J;
  - `bench` feature: J;
  - steering methods: H2;
  - raw-bit arithmetic: J;
  - stale comments: J;
  - small simplifications: J;
  - hidden rule: J (stats.md);
  - server exit code: B4;
  - `RUST_LOG`: J;
  - `identity` crate: J;
  - script fact lists: C4.

### TEST-REVIEW.md

- **T§1:** A1 for the net pin, the failures across ticks, the respawn test, refused casts, empty asserts and fixture defects.
- **T§2.1, T§2.2 and T§3:** the harness, constructors and shared helpers, in J. They should come early in track I, because every later step then edits fewer tests.
- **T§2.3:** the mode harness, in J.
- **T§2.4:**
  - `HashTrail`: A3;
- **T§2.5:** the `LocalMatch` methods, in J, after A1.
- **T§2.6:** protocol and package tests, in J.
- **T§4:**
  - the golden for a whole match: A3;
  - the state table test: G1;
  - one tick style, one failure reader, positions, spawns and the two rates: J.
- **T§5:** stronger assertions, in J. Exact readers come with the harness.
- **T§6:** hermetic fixtures. A2 gives the proving match. The rest is in J.
- **T§7:** time, in J.
- **T§8:** layout, in J.
- **T§9:** decision 9.
