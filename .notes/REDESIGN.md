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
| R5 Stable order and one exactness rule | Some systems spend or allocate in query order, and some arithmetic rounds its own way | R§4, R§12 |
| R6 A limit on work per tick, and fresh shared indexes | Navigation, vision, deliveries and the view have no work limit and no shared index | R§5, R§12 |
| T Proof and test redesign | Refactors have no permanent proof of equal behaviour, and every module writes its own harness | T§1 to T§8 |

## Rules

These rules state what "fixed" means. When agreed, they go into `design/02-engine-core.md`. The last column names the test that fails when the rule is broken. A rule that only a review can check is a rule that drifts again.

| # | Rule | Enforced by |
|---|---|---|
| 1 | One owner for each fact. A fact from the packages lives in one immutable book, a fact of the match lives in state, and a fact of the running call lives in the frame. Nothing else holds a copy. | The state table test and the behaviour golden (A3) |
| 2 | A name becomes an id where it enters. After the load, no system looks up a name. A script call resolves its name once per call, with no allocation. | `Books::build` tests (C5), and the allowlist test of name lookups (C7) |
| 3 | The load refuses everything that a match can refuse. A match start fails only on session terms: players, tick rate and seed. | `StartError` has no data case (C5b) |
| 4 | A layer calls a higher layer only through a hook that the higher layer registers. | The layer test (A4) |
| 5 | Every order that matters is by stable id, and every rounding uses one helper. | The archetype-shuffle test (B2) |
| 6 | Each tick's work has a fixed limit, or a cost in proportion to the units that take part. No tick pays for a scan or a rebuild that the other ticks do not pay for. | The worst-tick record (A3) and the navigation bench |
| 7 | Restored state is checked like package data: a restore gives an error for every flaw, and it never panics. | Every state type's check, a required method; the snapshot fuzz |
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

The layer test holds the order below and fails on any import from a higher layer. A capability adds to the script view through its column, to a call's frame through its part, to the effect dispatch, and to the script API through its row of the capability table.

```
values
core:      units (types, teams, owners, spawner, script view core), scripts (runtime, frame, effect order), players (resources)
           stats
           actions (the action pipeline, `Targets`, costs, `Purse`, `Payer`)
           combat
           deliveries, projectiles, areas, abilities, navigation, vision, progression, production
           orders (AI)
mode
capability_set, books
```

### Decided

- **Action kinds.** Each capability starts the actions of its kind: combat the attacks, then abilities the ordered casts in `ActionsSet::Start`, then production the trains, which pay at once. Each resolves its own kind. The book's load still maps a kind to its data, as the kinds are the actions layer's own enum.
- **One table of capabilities.** `CAPABILITIES` holds the install order, the needs, the effects and the API of each capability. The layers stay in the layer test, which checks that each capability builds only on lower layers installed before it. The modules in `lib.rs` are declarations, which no table can give without a macro.

## R5. Stable order and one exactness rule

### Problem

- `start_trains` and `strike` pay shared player resources in Bevy query order, which a restore can change.
- Three systems walk the whole `EntityIndex` to get an order, and they visit projectiles and areas too.
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

## R6. A limit on work per tick, and fresh shared indexes

### Problem

- Every script batch rebuilds every row.

### Shape

- **The view: incremental rows, the same snapshot.**
  - A batch must keep today's meaning: its calls see the world as it was when the batch began. So rows are not filled lazily on read, because a late read would see the effects of earlier calls.
  - The view keeps its rows from one build to the next. Each build refreshes only the rows whose source components changed since the last build, through Bevy's change ticks, and the rows of new or gone units.
  - Filters stay parsed at each call: the parse allocates nothing and costs a split and a scan of the tag names, far below the scan of the units the query makes, and a cache by text would grow with every text a script builds.
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
  - The state table test.
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

### Track I: independent steps

These need only Stage A and Stage B, and run in any order. Each is small enough to fill a session.

| Step | Change | Needs | Size |
|---|---|---|---|
| F3 | Ids at call time, so `ctx.projectile` and `ctx.area` return handles whose `.state` the call writes | Unit script state: a unit's `[state]` and `unit.state`, which the API does not have yet, so a handle alone would give a script nothing to use | M, changes behaviour |
| H4 | Incremental view rows | B | M |
| J | The local fixes in the appendix, and T§5 to T§8 | any time | S each |

### Order

```
Track I:  H4      F3 after unit script state
```

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
  - package count: C2;
  - unlimited work before a refusal: B4;
  - lobby crash: B4;
  - several seats: B4.
- **R§4:**
  - train order: B2;
  - homing and line types: B2;
- **R§5:**
  - stall and lost order: B3;
  - chaser starved: B3;
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
  - tags twice: with the harness (T§2.1);
  - call package: D4;
- **R§8:**
  - script runtime: D4;
  - layer list: A4.
- **R§9:**
  - tag names: C3;
  - action strings: C3;
  - mode and map strings: C3;
  - default filter: C6a;
  - `PackagePath`: C1;
  - `LoadError` path: C2;
  - avatar display name: C2.
- **R§10:**
  - Mode-stage events: B2;
  - `finish_trains`: B2;
  - `renew` interval: B2;
  - ms to ticks: C6a;
  - hook arity: C4;
  - damage-kind limit: C3;
- **R§11:**
  - `Package` data: C2;
  - load error shapes: C5a;
  - small shapes: J;
  - `ctx.projectile` returns `()`: F3;
  - capability set table: A4.
- **R§12:**
  - batch rebuilds: H4;
  - entity-index walks: B2;
- **R§13:**
  - applied handle: D5;
  - scripts parsed twice: C5b.
- **R§14:**
  - one struct per file: D2, D5, J;
  - server exit code: B4;
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
  - one tick style, one failure reader, positions, spawns and the two rates: J.
- **T§5:** stronger assertions, in J. Exact readers come with the harness.
- **T§6:** hermetic fixtures. A2 gives the proving match. The rest is in J.
- **T§7:** time, in J.
- **T§8:** layout, in J.
- **T§9:** decision 9.
