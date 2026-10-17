# Architecture review and redesign proposal

Date 17 October 2026, at HEAD `b9c3d925a` and the AGENTS.md change `9b2a40b6a`. Paths are relative to `source/`.

This document holds three things:
- the picture of the project's shape today;
- a redesign that removes the causes of the review's findings;
- the plan that implements it.

Eight reviews read every crate, the packages and the checks. They found 214 items. With the duplicates merged, 196 items stay. The [findings register](#12-findings-register) lists each item once, with the plan step that addresses it. The open items of the older `.notes/review-workspace.md` (RW) are in the register too, so this document replaces that file when its plan moves into PLAN.md.

**Status: approved, not implemented.** The user approved every decision of [Decisions](#9-decisions) on 17 October 2026. Nothing is implemented. As each batch lands:
- its decisions fold into the design documents ([Design changes](#7-design-changes));
- its batches move into PLAN.md, as AGENTS.md says;
- its register rows get the plan's tags.

Confidence words in the register:
- **confirmed:** a reviewer read the code path from end to end;
- **checked:** I read the code path again for this document;
- **plausible:** a test or a measurement must prove the item.

Nothing was built or run.

## 1. Summary

The base is sound, and the defects are in the shape above it. Five structural causes explain most of the findings:

1. **The core names every capability.** Closed lists in `actions`, `script`, `declare`, `sim`, the top crate and `package` hold every capability's kinds, effects, hooks, phases, books, sections and load checks. A new game's rule edits the core.
2. **The install runs before the load.** Systems, parts and resources exist before the books say what the mode uses. A game pays for every feature of each capability that it declares, and load facts become checks in each tick.
3. **A pass visits the population.** About twenty passes for each tick scan every unit to find the few that are due. Bevy's `Changed<T>` filter also visits every matching entity.
4. **Match facts live in non-send or exclusive state.** This pins a match to one thread and serializes most of the frame.
5. **Boundaries do not bound what enters.** A peer's declared counts, a restore's relations between parts, and a journal's corrupt frames reach code that trusts them.

The redesign answers each cause with one mechanism:
- **Declarations:** a module declares everything it adds, its data included, and one walk of the declarations serves the load, the install, the checks and the script API.
- **Books first:** the books are built before the install, so the schedule and the parts follow the data.
- **Due indexes and deltas:** a pass visits what is due or what changed, through one due index and through caches that observers keep.
- **Send data:** data and script handles are split, so only the script handles stay non-send, and parallel work runs inside the heavy passes.
- **Checks at each boundary:** a check where the data enters, before any allocation or write.

The plan has 12 phases and 52 batches, and a backlog of capability parameters that slices take:
- **Phases 1 to 3** fix the crash, the divergence, the confirmed bugs and the restore holes. They do not wait for the redesign, and the redesign keeps their fixes.
- **Phases 4 to 7** change the foundations in an order where each phase needs only the earlier ones.
- **Phases 8 to 12** build on those foundations.

## 2. The project's shape today

### 2.1 Layers

The workspace has 48 members and is 22 levels deep, with no cycle (A8's graph, from `cargo metadata`):

| Layer | Crates |
| --- | --- |
| 0 | `codec`, `common-derive`, `rules`, `schedule`, `store` |
| 1 | `common` |
| 2 | `log`, `math`, `protocol` |
| 3 | `quic`, `sim` |
| 4 | `geometry`, `state-types` |
| 5 | `values`, `visibility` |
| 6 | `client-files`, `script` |
| 7 | `declare` |
| 8 | `units` |
| 9 | `players` |
| 10 | `stats`, `vision` |
| 11 | `actions` |
| 12 | `combat`, `deliveries`, `interaction`, `items`, `progression` |
| 13 | `abilities`, `areas`, `navigation`, `projectiles` |
| 14 | `production` |
| 15 | `mode`, `orders` |
| 16 | `capabilities` (the release) |
| 17 | `bots`, `package` |
| 18 | `package-disk`, `runner` |
| 19 | `import`, `net`, `test-packages`, `verifier` |
| 20 | `client`, `harness`, `lan-check`, `server` |
| 21 | `zero-hour-check` |

What the layers do not show:
- **`actions` is called "core" but sits above `stats`.** It holds combat's weapon and every capability's kind data and effects.
- **`package` checks every capability's data.** So it depends on five capability crates.
- **`net` reads combat's deaths and vision's sight.** So it depends on both.
- **`geometry` depends on `sim` for one constant and an id.**

### 2.2 How a match is built and run

The call graph, from the package to a tick:

1. **Load.** `ModePackages::new` reads the packages, runs the central `load_check` (2,237 lines), and builds a `ScriptHost` with the release's whole API, only to parse each script into `ScriptFacts`. Then it drops the AST.
2. **Session.** `SessionRules::of(packages)` checks the host's choices. `Session::build` checks the terms again and keeps only part of the result. On the server, `start_match` and `restore_match` build the server's resources by two paths.
3. **Install.** `MatchBuild` calls `Release::build(world, spec)`:
   - It installs every module of the match's capability set with `Default` books.
   - It configures the 44 `Phase`s.
   - It binds the script API into the non-send `ScriptHost`.
4. **Books.** `MatchBuild::books` compiles every script again and builds every book at the match's rate (`ModePackages::books(rate)`).
5. **Mode.** `Release::install_mode` reopens the schedule, installs the scripted part, loads the mode, and builds the schedule again.
6. **Tick.**
   - **Server:** `FixedUpdate` runs the exclusive `run_sim_tick`, which calls `run_session_tick` and then `run_schedule(SimUpdate)`. The app clears its trackers at the end of each 2 ms frame.
   - **Replay:** `run_tick_alone` clears them once for each tick.
   - **Inside `SimUpdate`:** the systems run in one total order, through 44 phases on the single-threaded executor. Most of them are exclusive, and the script systems reach the non-send `View` and `ScriptHost`.
7. **Frame.** The server's `Update` holds about a dozen exclusive systems: door, checkpoints, receipts, bot driver and boundary. They pair `Session` with `ServerSigner` through `resource_scope`. Lightyear's own systems fan out over the compute pool.

### 2.3 Ownership

| What | Owner today | Defect |
| --- | --- | --- |
| Package facts | `ModePackages`, books shared by `Arc` in `ByType` | Books built again for each match and restore. The session copies players, saves and autosave. |
| The match's names (damage kinds, resources, teams) | The script `View` (non-send `Rc`) | A non-script fact is `!Send` and installed on the client |
| Match state | The `World`'s components and resources, listed by `StateRegistry` | The registry walks entities once for each type. Restores write one type at a time. |
| Schedule order | `Phase` in `declare` | The core names every capability's steps |
| Script runtime | `ScriptHost`, `View`, `Ctx`, `ScriptFailures`, `MatchMode` (non-send) | Pins the world to one thread |
| The running server match | About 12 resources from two build paths | Lifecycle is "which resources exist" |
| The session record | `SessionLog` (21 fields) with the journal | Whole history in RAM. Hidden replay mode. |
| The signer | A `ServerSigner` resource beside `Session` | Forces exclusive systems |
| The client's view of capabilities | One crate with `if`s and `Option` params | No mirror of `Release::modules` |
| Zero Hour's fixed rules | Importer Rust: `format!` and constants | A rules package can hold no mode data |

### 2.4 What holds

The redesign keeps these:
- **Determinism:**
  - The strict schedule refuses every ambiguity, exclusive systems included, and its bypasses are banned.
  - `SimRng` is counter-based and keyed by stream, entity and tick.
  - Ids come from one ordered `IdAllocator`.
  - Hash iteration and floats are denied in the sim's crates.
- **Calls:** one `ctx` for every role, all-or-nothing calls through one `Frame`, typed script arguments, pools for each tick, and a reference generated from the registry. The match binds only its capabilities' API.
- **Spawns:** deliveries spawn through `Commands::queue`. The role gates are install-time. A unit spawns in two moves now, down from eight.
- **Isolation:** no engine crate outside `apps/` names Zero Hour. The sim and the capabilities have no `static` or `thread_local!`.
- **Durability:** disk writes are off the frame, and group commit batches the syncs.
- **Configuration:** the lint list and the `clippy.toml` head match CLAUDE.md.

### 2.5 The causes, deduplicated

The fourteen root causes of the first review merge into ten. Each names its findings. The [register](#12-findings-register) has the detail.

| # | Cause | Findings |
| --- | --- | --- |
| C1 | The core and the session layer name every capability | C-1, C-2, C-3, C-4, C-5, E-3, E-5, E-6, E-11, S-5, S-7, S-8, S-11, K-19, R-20, R-22, R-31, N-26, N-27 |
| C2 | The install runs before the load | E-9, E-10, E-12, E-13, E-14, K-11, K-12, C-7, K-15 |
| C3 | A pass visits the population, not what is due or changed | K-13, C-15, C-17, R-4, R-5, R-6, R-7, R-12, R-30, S-24, E-17, A-10, K-21 |
| C4 | Match facts live in non-send or exclusive state | B-1, E-4, E-7, E-8, C-12, C-13, K-20, N-4 |
| C5 | Tick facts depend on the host's frames, and the executor fixes an undeclared order | E-1, E-2, E-16, N-3, N-7 |
| C6 | An entity does not spawn whole, and no part chooses its storage | C-9, C-11, E-18, E-22, N-9, A-6, A-8, B-24 |
| C7 | Rules of one game are fixed in Rust, and a rules package can hold no data | K-5 to K-10, K-23, R-8, R-9, R-13 to R-19, R-23 to R-25, C-23, C-24, S-9, S-10, S-14, A-3, A-4, A-5, A-16 |
| C8 | The script registry records names, not contracts, and the API is not symmetric | S-1 to S-6, S-12 to S-20, S-26 to S-29, C-27 |
| C9 | Boundaries do not bound what enters | N-1, N-2, N-8, N-15, N-16, N-19, N-28, B-15, B-16, B-17, B-25, R-2, R-3, C-10, C-21 |
| C10 | One value has several homes, or one role several shapes | N-5, N-6, N-14, N-17, N-18, N-20, E-19, A-11, A-19, A-20, C-18 to C-22, K-16 to K-18, R-26, R-28, A-7, B-11 to B-14 |

## 3. Direction

These principles refine the Direction in AGENTS.md and contradict none of it. Each says what it promises and where the practice comes from.

- **P1. A module declares everything it adds, and one walk serves every consumer.** "Everything" means:
  - its parts and systems;
  - its book with its builder, its unit-type section, its map section and its data check;
  - its kind's data and its effect's data;
  - its typed hooks and its mode events;
  - its script API and its order against anchors.

  The visitor with no default methods stays, so a new declaration kind fails to compile in every consumer that does not handle it. Bevy plugins work the same way: each one registers its own asset loaders, reflect types and settings checks.
- **P2. The books come before the install.** Books are pure functions of the packages and the rate (02, Books are one pure function). The install reads them, so a fact of the load is a fact of the schedule (02, Structural rules). A part comes from a predicate on the type's data, and a system only from data that uses it.
- **P3. The core holds pipelines and contracts, and the capabilities hold their concepts.**
  - **The core keeps:** the action pipeline, units, players, deliveries as a contract, the due index and the stages.
  - **Each capability owns:** its data types, its effects, its hooks and its rules.
  - **A meeting point:** where two capabilities meet, a contract below both carries the request, as an event of `units` that the owner observes (02, Contracts through `units`).
- **P4. A pass visits what is due or what changed.**
  - **Timed work:** it goes into one due index, keyed by `(Tick, StableId)`. That is a timing wheel, as Dota schedules next thinks and Factorio schedules its active entities.
  - **Caches:** they follow deltas from lifecycle observers.
  - **Active work:** a sparse-set activity marker carries it.
  - **Population scans:** none in a hot path.
- **P5. A tick's facts are held by the tick.** No sim system reads host bookkeeping that ages for each frame. A removal reaches its reader through an observer. A restore writes each entity whole.
- **P6. Data is `Send`, and only script handles are non-send.**
  - **The sim:** it stays one schedule on one thread, because the design's measurement says so.
  - **Parallel work:** it comes from deterministic kernels inside the heavy passes, with fixed chunks and a merge by stable id. Box2D v3, Rapier and 0 A.D. solve islands and paths this way.
  - **Many matches:** each match runs on its own thread.
- **P7. Each frame's order is declared.** The server's frame is chained sets with ambiguity detection, and its lifecycle is a Bevy `States` enum. The executor then cannot change behavior.
- **P8. A game configures capabilities through data first, then scripts.**
  - **Rules packages:** a rules package carries mode data, scripts and client data. The imported package carries only what the install gives.
  - **Capabilities:** each one exposes its game rules as typed parameters with defaults that keep today's behavior, and as hooks where data cannot express a rule.
  - **Practice:** Factorio's prototypes and StarCraft II's data editor give each class its own schema.
- **P9. The registry records contracts.**
  - **Forms:** each form's argument types and count, the name kind it returns, and a hook's argument shape and runner come from one Rust declaration. Godot's `ClassDB` records the types with the binding.
  - **The API:** it follows one vocabulary in which every write has its read and every add has its remove.
- **P10. Check at the boundary, before the allocation or the write.**
  - **Peers:** a peer's message has a declared largest size. ENet and yojimbo check before they allocate.
  - **Restores:** a restore checks the relations between parts and bounds every stored tick.
  - **Journals:** a journal recovers to the last whole frame and says what it cut, as RocksDB's default point-in-time recovery does.
  - **Secrets:** a secret is owned, never `Copy`, and is zeroed on drop.

## 4. Proposal

Each section gives the target shape of one system, the findings it removes, and the design decisions it adds or changes. [Design changes](#7-design-changes) lists the changes for each document.

### 4.1 Declarations and the release

**Today:**
- **Install before books:** `Release::build` installs with `Default` books, and `install_mode` reopens and rebuilds the schedule after the books arrive.
- **Hand lists:** the books (`BookParts`, `into_books`), the unit-type sections (`UnitTypeData`), the map books (in `mode`) and the load checks (in `package`).
- **One central order:** the order of all sets is `Phase::ALL` in `declare`.

**Target:**
- **The build order.** `MatchBuild` first builds the books: `ModePackages::books(rate)`, which needs no world. Then `Release::build(world, spec, &books)` installs each module once, with its books in place, and builds the schedule once.
  - `Book: Default` goes.
  - Load facts reach `InstallCx` (`cx.book::<B>()`) and decide which systems install. Examples: the mode's hook set, `SupplyBook::counts`, `VisionBook::has_grid`, turrets, clips, slow weapons, intervals, regen, toggles and channels.
  - A run condition remains only for a fact that changes during a match.
- **New declarations.** Each module declares:
  - `d.book::<B>()`, where `B: Book` has `fn build(input: &BookInput<'_>, cx: &mut BookCx) -> Result<B, BookError>`. The builder reads the module's own unit-type section and map section, which the module registers by name.
  - `d.check(...)`: the module's load check of its data, which returns the module's own error inside a `LoadProblem` case of `declare`. `package` walks the module list and keeps only the checks across packages: names, paths and the index.
  - `d.part::<Kind, P>()` with a predicate on the type's data: `ClipShots` only for clipped weapons, `RecentAttackers` only with an assist window or a script read, `StatusTags` from production, and `SpawnPoint` from combat's respawn.
- **The order of sets.**
  - `declare` keeps the stages (`SimSet`) and a few anchors that lower layers publish as `pub` sets, for example stats' grant pass and actions' start and resolve.
  - `ModuleSet::place` names a stage and orders itself after or before the anchors of the modules it `NEEDS`, as Bevy's plugins order against `TransformSystems::Propagate`.
  - Sets of unrelated modules stay unordered, so the strict schedule refuses them when they conflict.
  - `Phase` goes.
- **`Module::Set` becomes an associated type.** `InstallCx<'_, M>::add(M::Set, systems)` puts a system in its module's set by type, so the test `systems_outside` guards only what types cannot.
- **The sim and the release become modules of the list.** A `Sim` core module declares its state resources and `TickInputs`. The match's spec is one `MatchFacts` resource. The hand lists `sim_resources()` and `release_own()` go.
- **Role gates for every declaration.** `Declarations::column` and `non_send` install by role, as `book` does. A state resource is not inserted in a role that does not compute it, so a client system that takes it fails its parameter validation.
- **Capability identity.** `Capability` stays a closed enum: capabilities are Rust modules of the release, and a game configures them, it does not add them. Its five unbuilt variants go (`Character`, `Hitboxes`, `Physics`, `World`, `Quests`), and the design keeps them as planned entries. Each module declares its `DataKind` rows.

**Removes:** E-3, E-5, E-6, E-9, E-10, E-11, E-12, E-13, E-14, E-15, K-11, K-12, K-19, R-22, N-27, B-18, C-7, RW 20 (engine tags by hand, `take_shots` outside the server).

### 4.2 Core capabilities: pipelines and contracts

**Today:**
- **Closed lists in `actions`:** `KindSpec` names `Attack(Weapon)`, `Train`, `Build` and `Gather`. `ActionData` has 41 optional fields, with a hand table of the fields that each kind takes. `CapabilityDoesSpec` lists other capabilities' effects. `Shots` and `ShotHits` are core scratch with `pub` `Vec` fields.
- **Concepts in the wrong crate:** `ForcedMove` carries an action's delivery. `Player` and `MatchEntity` live in `units`. `orders` writes navigation's and production's parts. `production` needs `navigation` for a `Destination` write and two room queries.

**Target:**
- **Kinds.** `ActionData` holds the common fields and a `kind` section. Each kind's section is its capability's own `Deserialize` type with `deny_unknown_fields`, registered through a parser table keyed by kind name. The registration works as `EffectQueues` already does for effects.
  - The hand table goes. What a kind takes is its section's struct, and what it needs is that struct's fields that are not `Option`.
  - `Weapon`, `Scatter` and `Pitch` move to combat. `GatherSpec`, `BuildStart`, `RequiresData`, `ConstructData` and `PlacementData` move to production. `RankValues` keeps only what every kind has. A kind's own rank values live in its section, and the costs for each rank are a `Range` into the action's buffer (C-19).
  - `ActionRange::Meters` holds a `Distance` (C-20).
- **Effects.** One effect registry: each capability declares an effect once, with its data key, its script name and its argument types. Both the data list and the `ctx` call come from that declaration and queue the same effect value.
  - The core keeps only its own effects: `Modifier`, `Purge` and `Spawn`. `restore` moves to stats (K-3).
  - Unreal's `GameplayEffect` and Dota 2's data actions follow this shape: one spec, applied from data or from code.
- **One action pipeline.**
  - `actions` owns the checks for start and go-off: blocks, range, cost, time. It also owns the due index of action times. Kinds plug in through a callback table keyed by `ActionKind`, as `Deliverers` already is.
  - Attacks, casts, trains, builds and gathers all pass the same gate: `ActionKind::block` names the `Block` that stops each kind, and `Block::Produce` stops trains (R-1).
  - This is the shape of GAS's `TryActivateAbility` and `CommitAbility` (K-17).
- **Stats is a contract of the pipeline, not a dependency.** The pipeline asks through contracts in `units`:
  - a `Purse` contract for "can pay, pay";
  - a living test (`Dead` plus a targetable tag property);
  - passives as a hook that stats registers.

  `actions` then does not depend on `stats`. The release test of needs and uses covers core modules too (C-3, E-5).
- **Deliveries.** One `Delivered` record with a payload replaces `ShotHits` and the deliveries' own reports. Combat answers the attack payload, splash included, from the queue that the hooks read (K-18). `Shots` becomes a contract type with `fire` and `drain`, and no `pub` field (C-4).
  - The module type and the scratch type split (C-5). Delivery callbacks are keyed by the delivering capability, so `DeliveryKind` goes.
- **Contracts in `units`.**
  - `UnitOrdered { entity, by }` is an entity event. Navigation and production observe it and clear their own loops. Then `orders` drops its `production` and `interaction` dependencies (R-20, RW 20).
  - Writes to `Destination` go through a `units` method. A "standing room" query (is this box free, nearest standable point) is a contract that navigation installs. Then `production` drops `navigation` (R-31).
  - A death record in `units` replaces `net`'s read of combat's `Deaths`. An audience contract in `visibility`, which vision fills, replaces `net`'s read of `SeenBy` (N-26, B-19).
  - `ForcedMove` keeps only the movement. A dash's delivery is a `DashDelivering(Delivering)` part inserted with it in one bundle, and its end is an observer of `ForcedMove`'s removal. `DashDelivery` goes (C-6).
  - `Player` and `MatchEntity` move to `players` (C-8).
  - `TickCommand::controls` is the one answer to "which units does this player control" (RW 20).
- **Per-type facts stay in the book.** `OnDeath` is read through the unit's type, as `Armor` is, and a `Mortal` marker required by `Life` carries the filter (K-15).
- **A death is an event.** `Dying` triggers a `Died` entity event that projectiles and held hits observe, and a respawn triggers `Teleported` (K-22).

**Removes:** C-1 to C-6, C-8, C-19, C-20, K-3, K-15, K-17, K-18, K-22, R-1, R-20, R-31, N-26, B-19, S-8, RW 20 (deliveries' two shapes, the use slot extended by items, control resolution).

### 4.3 Time and change

**Today:**
- **No due index:** timed work is found by scans. `DueRun` keeps one tick for each entity, keyed by `Entity`, and does not keep the earliest push.
- **Removals:** caches read `RemovedComponents`, which age with the host's frames.
- **The script view:** it walks every entity for each batch.
- **Supply:** it is counted again by two `Local`s.
- **Mixed components:** `Site`, `Route` and `Node` mix progress with configuration.

**Target:**
- **`DueIndex` in `sim`.** It replaces `DueRun`:
  - It is keyed by `(Tick, StableId)`, so the order of visits is deterministic across a restore.
  - It deduplicates, and `push` keeps the earliest tick since the last take, which fixes C-16 by construction.
  - It is held for each timed process: attack resolves, cast dues, channel ticks, modifier expiries and intervals, held hits, respawns, area triggers and ends, trains, sites, lifespans and AI thinks.
  - A process pushes where its time is set, and its system takes only what is due.
  - `Lifespan` becomes `#[component(immutable)]`, with an `on_insert` hook that pushes.
- **An interval's policy is data.** While its unit is dead, an interval's hook does not run. Each modifier names what happens at the respawn, with GAS's three policies (`EGameplayEffectPeriodInhibitionRemovedPolicy`): `continue`, the default, keeps the world clock. `reset` starts one full period at the respawn. `fire_and_reset` runs at once and starts a new period (K-1, decision D13).
- **Removals through observers.** A reader of removals in `SimUpdate` takes them through an `On<Remove, C>` observer into a declared scratch that its system drains (E-1). A rules test bans `RemovedComponents` in sim and capability crates. Bevy documents hooks and observers as the reliable way to see a removal (`bevy_ecs` `lifecycle.rs`).
- **Caches follow deltas.**
  - Supply and holdings become one cache for each player. Observers on `Dead`, `Site`, `Owner`, `UnitType` and `TrainQueue` keep it, so supply stays derived and no system reports a change (R-12).
  - The script view applies the changed, added and removed rows as a merge, and skips a read when nothing changed (C-15, S-24).
  - `UnitGrid` is rebuilt by its own system in a core set, and the readers take `Res` (C-13).
  - The match entity is read through Bevy's `Single`, and the players through a dense table by slot (C-14).
- **Bounded searches use the grid.** The gather loop's node and drop-off searches use `UnitsNear`, and each node keeps its waiting queue (R-5). Room making and static following query only the bounds of the change (R-30). Colliders hold walkers only, and a static contact keeps its `IndexedBody` (R-7).
- **Progress apart from configuration.**
  - `SiteProgress` and the route cursor become their own components (R-26).
  - Route and `ArrivedShort` move as one `QueryData`, so the ten report passes go (R-6).
  - The action slots get a `SlotsMut` `QueryData` and no other `&mut` API, so the three slot components cannot get out of step (C-18).

**Removes:** E-1, K-1, K-13, K-21, C-13, C-14, C-15, C-16, C-17, C-18, R-4, R-5, R-6, R-7, R-12, R-26, R-30, S-24, RW 20 (`ShopUnits` as every living unit).

### 4.4 Spawns, storage and restores

**Target:**
- **One bundle for each spawn.** `SpawnCore` carries the facing angle, and every part is made from `SpawnAt` values. The caller's bundle and the declared parts go into one write (C-9). `PartWriter::push_by_id` and `SyncCell<BundleScratch>` replace the lookups and the unused `Mutex` (E-22).
- **Spawn hooks run on every spawn path,** as `On<Add, UnitType>` observers. The load refuses data that a role cannot hold, for example a passive on a projectile type (C-10).
- **One rule for avatars.** Engine tags carry a "type only" property, and the load refuses them in a modifier's grants (C-21).
- **Storage chosen for each part.**
  - Candidates for `SparseSet`: the markers that toggle during play and that no hot query filters by `Without`: `ForcedMove`, `Resetting`, `Kept`, `Respawn`, `AreaTrigger`, `OutOfPlay`, `HeldOut`, and the activity markers of P4, for example `Building` and `Gathering`.
  - `Dead` stays in a table, because `Without<Dead>` on hot queries is dense only for table storage.
  - The battle benches decide each one (E-21, C-11, K-14, R-27, decision D26).
- **Restores write whole entities.** A restore decodes every section into buffers for each type, then writes each entity once, in stable-id order, through the spawn bundle writer. The checks then run on a whole state, so the `Restoring` flag goes (E-2, E-18).
- **Restores check relations.**
  - `Experience` with the level track requires `Points` (R-2).
  - Stored due ticks are bounded by the rule that made them (R-3). With 4.3's progress parts, most of them are no longer absolute ticks.
- **Spawn tests.** A harness watch counts archetype moves for each spawn and asserts one move for every kind in the 3v3 and the proving match (B-24).
- **Server spawns.** `Replicate` becomes a required component of each kind marker in the server role. An `on_add` hook of the marker computes `Rooms` and `PredictionTarget`, and `ReplicationSender` becomes a required component of `LinkOf` (N-9).

**Removes:** C-9, C-10, C-11, C-21, E-2, E-18, E-21, E-22, K-14, R-2, R-3, R-27, N-9, B-24.

### 4.5 Threads

**Target:**
- **The sim stays single-threaded.** Its schedule keeps the single-threaded executor. A dated research entry records the measurement that the design's decision cites: the 3v3 tick under both executors (E-20).
- **Data out of non-send resources.**
  - **A `Names` book:** the damage-kind, resource and team names become a `Send + Sync` resource of `Arc` tables, built by the books. The script view shares the `Arc`s, and restore checks read the book (E-7).
  - **`MatchMode`, `Spawner`, `ViewReader`:** each splits into its data, a `Send` resource or book, and its script handle (E-8, C-12, R-11). `Spawner` becomes associated functions, with the spawnable types as a load check.
  - **`ScriptFailures`:** it keeps a `Send` summary.
  - **`ScriptHost`'s budgets:** they become a declared scratch, refilled each tick (E-13).
- **Scripted passes split in two.** A plain system collects what is due into a scratch. An exclusive part only runs the scripts (K-20).
- **Rhai stays without `sync`.** A match world stays pinned to the thread that built it. Many matches mean one thread for each match, as `LocalServer` does now. `sync` is measured before any change (B-1, S-21, decision D2).
- **Parallel kernels.** These run with `ComputeTaskPool::scope` over fixed chunks of sorted inputs, with the results merged in chunk order, so the results are bit-identical and no golden moves (R-10):
  - the broadphase by chunks of cells;
  - collision by islands;
  - the pathing grid by walker class;
  - vision by group;
  - route plans computed in parallel and committed in stable order;
  - the pure passes for each unit with `par_iter_mut` and `bevy_utils::Parallel` buffers.

  Each kernel goes in only with a bench that shows its gain.

**Removes:** B-1, E-4, E-7, E-8, E-13, E-20, C-12, K-20, R-10, R-11, S-21.

### 4.6 The server, the session and the network

**Today:**
- **Two build paths:** the server builds a match by two paths with five parameter bags.
- **Checked terms dropped:** it checks the terms up to three times and drops the checked result.
- **Implicit lifecycle:** the lifecycle is "which resources exist".
- **Exclusive frame:** most frame systems are exclusive.
- **The log in RAM:** the whole log stays in memory.
- **Transport limits:** the transport allocates from the peer's counts.

**Target:**
- **Transport bounds.**
  - Each registered message type declares its largest encoding in `net_protocol`. The fork's channel refuses a fragment count past that bound before it allocates, grows its buffer as fragments arrive, caps the half-built messages for each channel and link, and expires them on reliable channels too (N-1).
  - A link carries only the join channel, with a window of one, until it joins. The other channels come with `Joined`, and a message on them before then closes the link (N-2).
  - 05-protocol-spec.md states the bounds.
- **One running match.**
  - `MatchRun::begin(world, MatchSource::{New, Restored})` builds one `ServerMatch` value. It owns the door, seats, receipts, checkpoints, journal watch, clock and bot driver, and is inserted once (N-5).
  - `Session` owns its signer (`None` on a verifier), so `serve`, `record_checkpoint` and `record_result` need no second resource. The door, the checkpoints, the receipts and the bot driver become ordinary systems, and only `run_sim_tick` stays exclusive (N-4).
  - `Session` keeps its `CheckedTerms`, and `MatchVisibility` reads them (N-6).
  - `ModePackages` holds its `SessionRules` and a `SessionPlan` (players, saves, autosave), built at the load (N-20).
- **A declared frame.**
  - The server's frame is chained sets: `Intake → Door → SessionUpkeep → Receipts → Boundary`. They build with `ambiguity_detection: Error`, and Lightyear's sets are marked where they touch only their own components (N-3).
  - `ServerPhase { Lobby, Restoring, Running, Ended, Reloading }` is a Bevy `States` enum. `OnEnter(Running)` builds `ServerMatch` (N-7).
  - The executor stays single-threaded, and the design now says "the order is declared" (decision D36).
- **The session log.**
  - A replay is a `LogReplay` cursor that borrows the log.
  - The signature policy is a parameter of the decode.
  - The journal and its durable heads are a `JournaledLog` wrapper that only the server owns (N-17).
  - The live log keeps its state, the entries since the last checkpoint, and the counts. Publication streams the journal's frames through a bounded reader, as 10-sessions.md says (N-14, decision D34).
  - A record is encoded once, into the journal's frame buffer, and signed over the bytes already written. The journal worker wakes once for each frame (N-13).
  - Begun checkpoints are a FIFO, so `begin` never waits (N-12).
- **Controllers and seats.** `Control` is the one shape of a slot's controller, and `Controller` borrows it. The carry serializes it through one adapter. On the server, a Bevy relationship links a slot's entity and its link (N-18).
- **Journal integrity.**
  - A read stops at the first bad frame, cuts the file there and logs a `JournalCut { bytes }` event with what it dropped, as RocksDB's default `kPointInTimeRecovery` does. The session continues from the last whole frame. A receipt promises that its inputs survive a crash, not a corruption of the medium, and 05 and 10 say so (N-15, decision D33).
  - The private record holds only the seed chain, and the terms come from the journal's header alone (N-8).
  - `AppendWriter` bounds its bytes that are not yet durable. The owner checks a sync that is still running, and past a bound the server ends with its storage failure (N-16, B-15).
  - `DurableDir::remove` renames to a tombstone first (B-17).
- **Keys.** One `ServerKey` owner, not `Clone`, erases its `Keypair` on drop, and an `Arc<Signer>` is shared. `Secret` is not `Copy`, is zeroed on drop, and compares in constant time (N-19, B-25). Signatures draw their aux randomness from a CSPRNG for each signer, seeded once (N-11).
- **Signature checks.** `SessionLog::record` splits into `check` and `apply`. The frame checks different slots' packets in parallel and applies them in slot order (N-11).
- **Small items:**
  - The delegation expiration has one decimal spelling (N-28).
  - `TickHashes` goes behind `internals`, and the misnamed error variant is renamed (N-29).
  - The offer and the ack allocate nothing for each message (N-25).
  - A root span names each session in every log line (N-23).

**Removes:** N-1 to N-8, N-11 to N-20, N-23, N-25, N-28, N-29, B-15, B-17, B-25.

### 4.7 The script system

**Today:**
- **Names, not contracts:** the registry records names and roles.
- **Hooks:** a closed enum in the core, called with untyped tuples.
- **One mode script:** a match runs one mode script.
- **Compiled for each match:** each match binds the API again and compiles every script.

**Target:**
- **Forms with types.**
  - Each `ScriptArg` carries its type name, and a form is its closure's signature with a name for each argument. The load refuses a call whose argument count no form takes (S-3).
  - Each member records the name kind that it returns. Names that the API returns are typed handles, whose comparison with a literal resolves the literal at the load. Comparisons that need the package scope are methods with a checked argument, for example `d.is_action("dusk_mark")` (S-4).
  - Data field lists come from serde at bind time (S-6).
  - Every record takes a `Status`, and the load refuses a name whose `since` is past the package's `api` (S-27).
- **Typed hooks owned by their runner.**
  - `trait HookSpec { const NAME; const ROLE; type Args: ScriptArgs; const PARAMS; }`.
  - Each capability declares its hooks through `Declarations`, so a hook's owner is the module whose system calls it (S-2, S-5).
  - The record carries the pool policy, so `CallStart` computes the payer through one `Pool::of_call`, and `ScriptBatch::run` applies the spent-pool policy for its role (S-25).
  - One generic `HookQueue<E: ModeEvent>` replaces the four hand-written mode queues, and the capability that produces an event declares it (S-11).
  - `ApiOwner`, `DataTable`, `NameKind` and the capability cases of `ApiError` leave `script`. An owner is the type registered through `ty::<T>`, and a capability's refusals use `CapabilityRefusal` (S-7).
- **Script hygiene.**
  - `try`, `catch` and `throw` are disabled. The load refuses top-level statements (S-26, decision D28).
  - Rhai builds with `no_closure` and `no_module`, so anonymous functions with no capture become legal (S-28, B-21).
- **Compile once.** The load keeps each `AST` and shares it. The match's engine comes from a prototype for each release, registered as one global module. Handles drop their `Rc<View>`. Allocation counts are measured on `calc_damage`, `on_think` and `find` before any further change (S-22, S-23, decision D30).
- **Rules scripts compose.** A mode lists its rules scripts in order in `mode.toml`. Each script has its own declared state, and the mode role's hooks run in each, in that order. Only the mode package's script can end the match. Factorio calls every mod's handlers in load order (S-9, decision D10).
- **One vocabulary.** [Section 5](#5-the-script-vocabulary) gives the scheme and the renames.

**Removes:** S-2 to S-11, S-21 to S-28, B-21 (Rhai part).

### 4.8 Game configuration through rules packages

**Today:** a `kind = "rules"` dependency carries scripts and records, but no mode data and no manifest facts. So the importer writes Zero Hour's fixed rules with `format!` templates and Rust constants, and `mode.rhai` repeats their names (A-16).

**Target:**
- **The engine side.**
  - A rules dependency's `data/rules.toml` merges under the mode's data by the existing load-order override rule, and the load checks the merged `ModeData`.
  - A rules package states the capabilities and tick range that it needs, and the load refuses a mode that lacks them (decision D9).
  - A capability's field for each type takes a default at the mode level, declared in the capability's `declare`. Examples: combat's default death, the turret tolerance, navigation's widest walker.
- **The Zero Hour side.**
  - `packages/zero-hour/` holds Zero Hour's fixed mode data: stats, pools, damage kinds, tags, slot kinds, resources, state, params, teams and capability defaults. It also holds the scripts and the client data for the HUD, the keys and the camera input.
  - `game/` holds only what the install gives.
  - The importer reads the rules package's names through the engine's own types, which derive `Serialize` (A-17, RW 18).
  - The import ends with the engine's own load of what it wrote (A-18).
  - One `ZeroHourRules` owner of the parsed install feeds passes in the style of a compiler: collect the closed type set, convert, then emit (A-19).
  - One `ImportLoss` enum replaces nine lists (A-20).
- **Common rules as data.** "Last structure that counts" victory, kill bounty and XP share become data sections with passes that visit deaths, and the script hooks stay for what is special. OpenRA's `ConquestVictoryConditions` and Warcraft III's bounty fields do this (S-10). This changes games.md's "Phases, win conditions and economy are mode scripts".
- **Hooks where data cannot express a rule.** `on_unit_trained`, `on_site_complete` and `on_path_end` are Mode-stage events through `HookQueue`. Reads give a site's progress, a queue and a node's amount (R-17). `PathWalker` gains `once`, `loop` and `back_and_forth`, and navigation owns path following (R-21).
- **Capability parameters.** These come with the slice of the game that needs them, each with a default that keeps today's behavior:

| Capability | Parameters | Findings |
| --- | --- | --- |
| combat | Damage stages before `calc_damage`: bonus by tags, flat or ratio armor. Own values scaled by a stat. Leech rows by cause and kind. Splash `procs`. A splash edge that the data names. Weapon choice criteria. Several turrets. Line attack projectiles. A clip's reload origin. | K-5 to K-10 |
| stats | `Mul` and `Set` with a priority. Hooks and intervals owned by stats. `Option<Meter>` with a niche. | K-2, K-24 |
| abilities, items, areas | Toggle `per_ms`. A cost rule that cannot kill. Prices in every resource. `inside = [AuraSpec]`. | K-16, K-23 |
| production | Capacities that the mode declares. Requirements in CNF. One progress model by rate, with research entries. Node capacity. `passes` as a mode tag property. Placement on a unit, repair, consuming builders. | R-13 to R-16, R-18 |
| orders | Native acquisition from data. Attack-move, patrol and hold. The leash as data. | R-19 |
| navigation | Walker classes that the mode declares. A route budget for each tick. | R-8, R-9 |
| progression | Points for each level as track data. Tracks on players. | R-24 |
| mode | Teams from the session's terms. Winners and placements. Faction offers. Options that data can read. `team.enemies` in place of `enemy_team`. One `Loadout`. | R-25, R-28, S-14 |
| units | Ground samples of 16 bits with a base height, when a game needs them. A shared-control relation, when a game needs it. | C-23, C-24 |

**Removes:** A-16 to A-20, A-22, A-23, S-9, S-10, R-17, R-21, and the rows above when their slices land.

### 4.9 The client

**Target:**
- **A client module list** mirrors `Release::modules`. Each presentation module states the capability it presents, and one walk of the manifest's capability set builds a `ClientPlugins` group. The `if`s in `add_ends`, the `Option` system params and their `expect`s go. A test maps each capability to its client modules (A-1, RW 20 charges).
- **Signals in place of polls.**
  - Always-present resources are `Res` (A-9, RW 21).
  - The input mode is a Bevy `States` enum, and the ghost spawns on `OnEnter(Placing)` (A-13).
  - Systems that run every frame get observers or run conditions, and `CameraRig` is changed only by a real drag (A-10).
  - `Gauged` leaves the sim's entities (A-8).
- **Work done once for each asset.** A `GltfExtensionHandler` dresses each model at the load: the file material, hidden nodes, the turret node path. The import writes each look's top height. Meshes load in the render world only, so the CPU copies go (A-6).
  - Material extras become a glTF vendor extension, and the material files go (A-7, RW 21 sampler).
  - The client takes the map's heights from `ModePackages` and reads client files through `PackageReader` (A-11, RW 21 `ClientData`).
- **Picking by volume.** An analytic ray test against each drawing's `Shape`, with the nearest hit and the stable-id tie (A-2, RW 21 two click rules).
- **The ground by the map's metric.** One function adds the drawn ground only on a planar map (A-12).
- **Presentation as data.**
  - A team color mask in the one pipeline, through `MeshTag` and a buffer of the match's colors. The import writes Zero Hour's `MultiplayerColor`s (A-3). The same mechanism tints the build preview (RW 21).
  - HUD layout data: screen-space bars, and which units show them (A-4).
  - An `InputMap` read from client data, with Zero Hour's edge width and click time in `CameraFile` (A-5).
  - Engine messages through Fluent (A-15).
- **Presentation scripts** become planned entries in 03 and 12 (A-14, S-29).

**Removes:** A-1 to A-15, and the RW 21 items.

### 4.10 Base crates, tests and dependencies

**Target:**
- **One home for each fact.**
  - The coordinate bound is one math constant, and `Position`, `Direction` and `geometry` derive from it. `geometry` then needs `math` alone, with a generic id in `BodyGrid` (B-11, C-26).
  - One checked name newtype with a length bound holds the grammar of `MapName` and `DeclaredName` (B-12).
  - `NamedEnum` emits its member list once, and its index is `self as usize` (B-13).
  - `Value32` serializes as hex in text formats (B-14).
  - `ErrorReport` moves to `common`, and `quic` drops `log` (B-20).
- **Precision as promised.**
  - `step_toward` computes from the exact length, as `normalized` does (B-8).
  - The scatter point rounds once (C-25).
  - One geometry primitive for each shape pair returns `Ordering` against a reach, with named wrappers (C-22).
  - 09 states the rule for float lanes, and the `asm!` division gets a recorded decision (B-9, B-10).
  - `Flat::length_squared` computes in `u128` (B-26).
- **Storage as promised.** Reads open with no-follow flags behind `platform` (B-16).
- **Rules tests that can fail.**
  - The layers come from `cargo metadata`. The tests then check what can break: no crate of the sim or the capabilities has `store`, `tracing` or `log` in its normal closure, and `protocol` has no `store` (B-3).
  - One manifest reader serves every rule (B-4).
  - A test holds the float ban for every crate of the sim's closure (B-5, decision D48).
  - `LogCheck` and the allocation audit become global for each process, because nextest runs each test in its own process (B-2).
  - The ambiguity check without sync points runs for every role of every workspace mode (E-16).
- **Dependencies.**
  - Lightyear builds without its default features, and `interpolation` goes on both ends (N-10).
  - `bevy_ecs` builds with only the features it uses, and `reflect_auto_register` goes (B-21).
  - The forks pin `rev` (B-22).
  - `miniz_oxide` takes Bevy's version (B-23).
  - `bots` derives its TOML from `Action`, names actions by name, and places positions from markers (R-29).
  - The check's bot step is `BotScript::serve`, which the server's driver shares (A-21).
- **Configuration.**
  - `overflow-checks` stays in release as a recorded exception (B-6, decision D45).
  - The plan moves to PLAN.md (B-7).
  - The surfaces of RW 22 narrow last.

**Removes:** B-2 to B-14, B-16, B-20 to B-23, B-26, C-22, C-25, C-26, E-16, R-29, A-21, RW 22, RW 23.

### 4.11 What this proposal does not change

- **The sim's single thread.** The design's measurement stands, and parallel work goes inside passes.
- **The `Declarations` visitor with no default methods.** It already forces every consumer to handle each declaration kind. Option D6 (b) would give the same promise with a different shape, for no gain at a cold path.
- **`Capability` as a closed enum,** less its unbuilt variants.
- **The fixed width of tag sets (256 bits).** The design chose a fixed width that a rollback copies with no allocation.
- **Floats only outside the sim, and the strict schedule.**

## 5. The script vocabulary

### The scheme

1. `ctx` is the match. A change is a verb, `<verb>_<object>`. A query is a noun or `find_*`. A call that changes something returns the handle it creates, or nothing.
2. Handles read themselves: `Unit`, `Player`, `Team`, `Modifier`, `Damage`, `Heal`, `Hit`, `Marker`, `Map`, `Vector`. No handle member changes state, except the declared `state` and `m.stacks`.
3. Every write has its read, and every add has its remove: `set_X`/`X`, `add_X`/`remove_X`, `give_X`/`take_X`.
4. The argument order is subject, then which, then how much, then how long, with an optional source last.
5. Predicates are `is_`, `has_` or `can_`. Everything else is a noun.
6. One word for each concept: action, not ability. Unit, not entity. `params` everywhere. A list query returns living units, unless its name says `all`.
7. Hooks are `on_<event>` with a present noun, and their parameter names are what scripts write.

### The renames

| Old | New | Why |
| --- | --- | --- |
| `ctx.end(team)`, `ctx.end(())` | `ctx.win(team)`, `ctx.draw()`, and the read `ctx.result` | `end` is also `on_end`. A second call is a no-op (S-12). |
| `ctx.players` (a count) | `ctx.players` (a list of `Player` handles) | `ctx.teams` is a list |
| `ctx.team_of(p)`, `units_of(p)`, `held(p)`, `supply_used(p)`, `supply_cap(p)` | `player.team`, `player.units`, `player.seated`, `player.supply_used`, `player.supply_cap` | Handle reads (S-13) |
| none | `player.resource(name)`, `player.is_out`, `ctx.take_resource(player, name, amount)`, refused when short | A read and a checked spend (S-13) |
| `ctx.add_player_modifier(p, id)` | kept, with `ctx.remove_player_modifier(p, id)` | Inverse |
| `ctx.enemy_team(team)` | `team.enemies` and `ctx.relation(a, b)` | No two-team assumption (S-14) |
| `ctx.timer(name, ms, repeat, data)` | `ctx.after(name, ms, data)`, `ctx.every(name, ms, data)`, `ctx.cancel_timer(name)`, with declared names | A flag argument and no inverse (S-19) |
| `ctx.remove(handle)` | `ctx.remove_modifier(m)` | It binds modifiers only (S-1) |
| `ctx.projectile(from, to)`, `ctx.area(pos)`, data `launch = { area }` | `ctx.launch(from, to)`, `ctx.land(pos)`, data `land = { area }` | One verb for each delivery |
| `ctx.damage(target, amount, kind)` | `ctx.damage(target, kind, amount[, source])` | Which, then how much. A named source (S-16). |
| `ctx.reduce_cooldown(u, id, ms)`, `reduce_cooldowns(u, kind, fraction)` | `ctx.cut_cooldown(u, action, ms)`, `ctx.scale_cooldowns(u, kind, fraction)` | Two operations, two names |
| `ctx.charge`, data `charge` | `ctx.charged`, data `charging` | `charge` means stored charges |
| `ctx.available(p, choice, value)` | `ctx.can_choose(p, choice, value)` | Predicate prefix |
| `ctx.order_attack(unit, target)` and the other orders | `ctx.order_attack(target)` and the others, on the acting unit | The argument had one valid value. The AI role becomes reads, orders and randomness (S-17). |
| `ctx.reveal(pos, radius, ms)` | `ctx.reveal(pos, radius, ms[, team])` | The mode can reveal (S-16) |
| `ctx.p` | `ctx.params` | One word with `unit.params` (decision D27) |
| `unit.alive`, `d.attack`, `d.extra`, `h.leech` | `is_alive`, `is_attack`, `is_extra`, `is_leech` | Predicate prefix |
| `d.ability`, `h.ability`, `NameKind::Ability` | `d.action`, `h.action`, `NameKind::Action`, and `d.is_action(name)` | One word. A scoped compare (S-4). |
| none | `unit.modifier(id)` | Find a modifier, to read or remove it |
| `unit.target`, `hit.target` | `unit.attack_target` (registered by combat), `hit.aimed` | One word, one meaning (C-27) |
| state types `entity`, `entity_list` | `unit`, `unit_list`, with `enum`, lists of scalars and state for each player | The model's word, and the missing types (S-20) |
| `spawn_unit` returns `NewUnit` | `spawn_unit` returns `Unit`, whose row is empty until the spawn | One handle that a script can store (S-18) |
| `on_unit_died` | `on_unit_death` | Present noun |
| `on_mode_input` | `on_input` | The only input hook |
| `on_stage_done` (planned) | `on_quest_stage` | Noun |
| hook parameter `unit` for the caster | `caster` | What scripts write |
| `vector(x, y, z)` | kept, with `v.x`, `v.y`, `v.z` and `==` | A read for the constructor |

All lists return living units unless their name says `all` (S-15). The renames change every package's scripts, so they go in one batch, with the reference regenerated from the registry.

## 6. Multithreading

| Level | Target | Prerequisites |
| --- | --- | --- |
| Inside the sim's tick | Deterministic kernels in the heavy passes (4.5) | A bench for each kernel |
| The server's outer frame | Declared sets, ordinary systems over `ServerMatch` and the session, parallel signature checks (4.6). The executor stays single-threaded, because the gain for one match is small. | Phase 6 |
| Many matches in one process | One thread for each match | All of the rows below |
| | `ServerDir::waiting()` returns every unpublished session (N-21) | |
| | One QUIC endpoint for each process, which routes each connection to its match by SNI or ALPN (A6, Multithreading) | |
| | Serial Lightyear passes for each match, or a scope executor for each match (N-22) | |
| | A root span for each match (N-23) | Phase 6 |
| | A runner that wakes on the next tick's deadline or on a datagram (N-24) | |
| | Books built once for each package fingerprint and rate, and shared ASTs (E-23, S-22) | Phases 4 and 9 |
| | A host owns the matches, and a match's fault ends only that match | Decision D37 |

E-1 must be fixed before a host's frame rate can vary, so it comes in phase 1.

## 7. Design changes

Each change folds into its document when its batch lands. Each line names the decision it adds, changes or narrows.

- **02-engine-core.md**
  - Operation pools: a mode input spends its player's pool and fails when that pool is spent (D32).
  - Libraries: shipped x86-64 binaries need x86-64-v3 (D50).
  - Structural rules: "a set's order is its phase's" becomes "a set orders against anchors of the modules it needs" (4.1).
  - Modules: "the books are built before the install" (4.1).
  - The sim is one schedule on one thread: stays, citing a new research entry (4.5).
  - Bevy: "every schedule of the server runs on one thread" becomes "the server's frame order is declared and checked" (4.6).
  - Storage: a read refuses a link at the open (4.10).
  - Secret files: owned secrets, erased on drop (4.6).
  - Session log: the live log keeps state only (4.6).
- **03-game-scripting.md**
  - "One mode script owns the rules" becomes "ordered rules scripts, of which only the mode's ends the match" (4.7).
  - "Overrides by load order" covers mode data from rules dependencies (4.8).
  - Presentation scripts become planned (4.9).
- **04-capabilities/00-overview.md:** a splash edge that the data names, inside the one reach rule (4.8).
- **actions.md:**
  - "A kind's data is its capability's section."
  - "One pipeline gates every kind."
  - "One effect registry for data and scripts" (4.2).
- **combat.md:** damage stages, weapon scaling, splash procs, weapon choice criteria and turrets, as each lands (4.8).
- **production.md:**
  - "Supply is derived" stays, kept by observers (4.3).
  - Capacities, CNF requirements and one progress model, as each lands (4.8).
- **games.md:** "win conditions and economy are mode scripts" becomes "common rules are data, special rules are scripts" (4.8).
- **stats.md:** stats owns modifier hooks, intervals and `restore`. An interval's hook does not run while its unit is dead, and a policy in the modifier's data says what happens at the respawn (4.2, 4.3).
- **08-script-api.md:**
  - The vocabulary (5).
  - An explicit source on effects (S-16).
  - No `try` or `catch` (4.7).
- **09-determinism-core.md:**
  - The rule for float lanes.
  - `step_toward` rounds once.
  - The `asm!` division (4.10).
- **10-sessions.md:**
  - The journal's corruption policy: point-in-time recovery, logged. Receipts promise survival of a crash, not of a corrupt medium (D33).
  - The terms live in the journal's header only (4.6).
- **05-protocol-spec.md:**
  - Receipts: what survives a crash, and what a corrupt medium can lose (D33).
  - The largest encoding of each message.
  - The join-only channel (4.6).
- **12-zero-hour.md:**
  - The rules package holds Zero Hour's fixed data (4.8).
  - Losses: team colors, until A-3 lands.

## 8. Implementation plan

### Rules of the plan

- **Batches.** Each batch is one commit and ends with `./drive.py check` passing and a stop for review, as AGENTS.md says. A batch names what it touches and why it stands where it does.
- **Tests.** A bug's batch first adds a test that fails, built on the bug's failure scenario. Each changed behavior is tested. A pure refactor is not.
- **Goldens and formats.** A batch says when it may move goldens, and it explains each move. A batch that changes a format raises that format's version tag. A batch that changes the Zero Hour fingerprint pins it from a run on Linux and a run on macOS.
- **Designs.** A batch folds its decisions into their documents (section 7) in the same commit.
- **Order.** A phase needs only the phases before it. Inside a phase, the batches are independent unless a batch names another.
- **The first step after approval.** This plan moves into PLAN.md with its tags. The register's rows that ISSUES.md holds get those tags. `.notes/review-workspace.md` goes, because the register holds its open items (B-7).

### Phase 1: the crash and the divergence

**1.1 Transport bounds.** Waits for Q1 in `review-architecture_QUESTIONS.md`: the fix goes in the Lightyear fork.
- **Touches:** the Lightyear fork's transport crate, `net_protocol` and 05-protocol-spec.md.
- **Why first:** one datagram stops the server.
- **Steps:**
  1. A `net` scenario sends a fragment that names 2³⁵ fragments, and a link that sends input messages before it joins. Both fail today.
  2. Each registered message declares its largest encoding. The fork refuses a fragment count past that bound before it allocates, caps the half-built messages for each channel and link, and expires them on reliable channels (N-1).
  3. A link carries only the join channel, with a window of one, until it joins (N-2).
- **Format:** the network protocol's version rises.

### Phase 4: declarations own what a module adds

**4.1 Books before the install.** Touches the release, `declare`, `values` (engine tags).
- `declare` assigns engine tag numbers in declaration order (RW 20). Waits for Q3 in `review-architecture_QUESTIONS.md`.

**4.5 Parts from data.** Touches `combat`, `stats`, `units`, `production`, `sim`, `state-types`.
- Per-type predicates give `ClipShots`, `RecentAttackers` (with its entries bounded by the window) and `SpawnPoint` (K-11, C-7). Waits for Q5 in `review-architecture_QUESTIONS.md`.
- Each module declares its `DataKind` rows (E-6). Waits for Q4 in `review-architecture_QUESTIONS.md`.

### Phase 5: core pipelines and contracts

**5.1 Kind sections owned by capabilities.** Touches `actions`, `combat`, `production`, the top crate and `package`.
- `ActionData` holds the common fields and a `kind` section of the capability's own type. The hand table goes. `Weapon`, `Scatter` and `Pitch` move to combat, and the build and gather types move to production (C-1). Done after 5.3: the core's reach check reads a weapon's pitch, and its slot times a weapon's period, which combat gives once the kinds plug in through 5.3's callbacks.
- **Goldens:** none. Data that a kind ignored without a word is now refused.

**5.2 One effect registry.** Touches `actions`, `combat`, `stats`, `progression`, `areas`, `navigation`, `script`.
- Each capability declares an effect once, with its data key, script name and argument types. The data list and the `ctx` call come from that declaration (C-2, S-8). Waits for Q6 in `review-architecture_QUESTIONS.md`.

**5.3 One action pipeline.** Touches `actions`, `combat`, `abilities`, `production`, `units`, `stats`.
- Waits for Q7 in `review-architecture_QUESTIONS.md`.
- One start and go-off pair in `actions` owns the checks and the times. Kinds plug in by a callback table (K-17).
- The `Purse` contract, the living test and the passive hook are contracts in `units`, and `actions` drops `stats`. The needs-and-uses test covers core modules (C-3, E-5).
- **Goldens:** they may move where the four loops differed. Each move is explained.

**5.4 One delivery report.** Touches `deliveries`, `projectiles`, `combat`, `units`, `values`, `navigation`.
- One `Delivered` record, so combat answers the attack payload from the hooks' queue (K-18).

**5.5 Contracts in `units`.** Touches `units`, `players`, `orders`, `navigation`, `production`, `interaction`, `items`, `net`, `visibility`, `vision`, `combat`.
- `UnitOrdered` is an entity event, and `orders` drops `production` and `interaction` (R-20).
- `Destination` writes and the standing-room query are contracts, and `production` drops `navigation` (R-31).
- A death record and an audience contract, and `net` drops `combat` and `vision` (N-26).
- A `Died` entity event that projectiles and held hits observe, and `Teleported` on a respawn (K-22).
- `Player` and `MatchEntity` move to `players` (C-8).
- `TickCommand::controls` is the one control answer (RW 20).
- The use slot extends through `UseSlot::extend_slots` (RW 20).
- One naming of order inputs and order sets (RW 20).

### Phase 6: `Send` data and the server frame

**6.1 Data out of non-send resources.** Touches `units`, `script`, `mode`, `combat`, `stats`.
- A `Names` book (E-7, S-21).
- `MatchMode`, `Spawner`, `ViewReader` and `CombatHooks` split their data from their script handles (E-8, C-12).
- `ScriptFailures` keeps a `Send` summary.
- Scripted passes split into a plain collect and an exclusive script run (K-20).
- `UnitGrid` is rebuilt by its own system, and its readers take `Res` (C-13).

**6.2 One server match.** Touches `net` (sim server, lobby, session directory), `runner` (session, session rules), `package`, `apps/server`.
- `MatchRun::begin` from `MatchSource::{New, Restored}` builds one `ServerMatch` (N-5).
- `Session` owns its signer, and the frame's systems become ordinary systems (N-4).
- `Session` keeps its `CheckedTerms` (N-6). `ModePackages` holds `SessionRules` and a `SessionPlan` (N-20).
- `TickHashes` goes behind `internals`, and the error variant is renamed (N-29).
- A restore refuses other `--visibility` or `--spectators` settings, as it refuses other picks (ISSUES, Server).

**6.3 A declared server frame.** Touches `net` and 02-engine-core.md.
- Chained sets with `ambiguity_detection: Error` (N-3).
- `ServerPhase` as a `States` enum (N-7).
- A root span for each session (N-23).

**6.4 The session log.** Touches `protocol` (session log, journal, controller, carry) and `net` (seats, player link, checkpoints).
- `LogReplay`, the decode's signature policy, and `JournaledLog` (N-17).
- The live log keeps state, and publication streams the journal (N-14, D34).
- One encode for each record, and one wake of the journal worker for each frame (N-13).
- A FIFO of begun checkpoints (N-12).
- `check` and `apply` split, with parallel checks across slots and aux randomness from a CSPRNG for each signer (N-11).
- One `Control` shape, and a relationship from a slot's entity to its link (N-18).

**6.5 Allocations on the network paths.** Touches `net` and `protocol`.
- The offer keeps its encoded body. The ack has a bounded array. `Version` keeps its tag text. The chain history is a `VecDeque` (N-25).

### Phase 7: visit what is due or changed

**7.1 One due index.** Touches `sim`, `actions`, `combat`, `stats`, `abilities`, `areas`, `items`, `production`, `orders`, `units`.
- `DueIndex` keyed by `(Tick, StableId)` replaces `DueRun`, and every timed process uses it (K-13, C-17, R-4).
- `Lifespan` becomes immutable with an `on_insert` push.
- **Goldens:** they may move where the order of visits was by `Entity`. Each move is explained.

**7.2 Caches that follow deltas.** Touches `production`, `units` (view reader), `players`, `combat`, `items`, `script`.
- Supply and holdings as one cache for each player, kept by observers (R-12).
- The script view merges deltas and reads once for each phase that runs scripts (C-15). Call parts copy on first use, and literal names resolve once for each call site (S-24).
- `Single` for the match entity and a dense player table (C-14).
- The damage pass reads through a cached `SystemState`, and a heard event shares one handle (K-21).
- A shop index (RW 20).

**7.3 Navigation and gathering passes.** Touches `navigation` and `production`.
- `RouteMut`, so the report passes go, and `RouteAsks` (R-6).
- Colliders hold walkers only (R-7).
- Room making and static following query the change's bounds (R-30).
- Gathering searches use the grid, and each node keeps its queue (R-5).

**7.4 Progress apart from configuration.** Touches `production`, `navigation`, `actions`.
- `SiteProgress` and the route cursor become their own parts (R-26).
- `SlotsMut` as a `QueryData` (C-18).
- **Format:** the snapshot's version rises.

**7.5 Whole spawns and whole restores.** Touches `units`, `declare`, `sim`, `net`, the harness.
- One write for each spawn (C-9), and the small spawn costs (E-22).
- Spawn reactions as observers on every spawn path (C-10's mechanism).
- Restores decode first and write each entity once, and `Restoring` goes (E-18, E-2).
- `Replicate` is required by each kind marker on the server (N-9).
- The harness counts archetype moves for each spawn (B-24).

**7.6 Storage for each part.** Touches the markers' crates.
- Sparse-set storage on the candidates of 4.4, each kept only where the battle benches show a gain (E-21, D26).

**7.7 The state registry.** Touches `sim`.
- Cached queries for each type in the hash and the snapshot, measured against the `state_hash` bench (E-17).
- The typestate builder, frozen registry and change tracker (E-19).

### Phase 8: the script system

**8.1 Forms with types.** Touches `script`, every `*_api.rs`, `package`.
- `ScriptArg` type names, forms from closure signatures, and argument counts checked at the load (S-3).
- Typed returned names and scoped comparisons (S-4).
- Field lists from serde (S-6).
- One `Status` shape and version checks for each name (S-27).

**8.2 Typed hooks and one call policy.** Touches `script`, `mode`, `combat`, `deliveries`, `abilities`, `orders`, `stats`.
- `HookSpec`, recorded by the runner's module (S-5).
- The core's capability enums leave `script` (S-7).
- `HookQueue<E>` (S-11).
- `Pool::of_call` and the role's spent-pool policy (S-25, D32).

**8.3 Compile once.** Touches `package`, `runner`, `script`, `units`, the workspace manifest.
- Shared ASTs and an engine prototype for each release (S-22).
- Books built once for each package fingerprint and rate (E-23).
- Handles without `Rc<View>`, and allocation counts measured (S-23, D30).
- Rhai with `no_closure` and `no_module` (S-28, B-21).

**8.4 Ordered rules scripts.** Touches `package`, `mode`, `script`, 03-game-scripting.md.
- A mode lists its rules scripts in order, each with its own state. Only the mode's script ends the match (S-9, D10).

**8.5 One vocabulary.** Touches every `*_api.rs`, `mode`, every package's scripts, the reference.
- The renames of section 5 (S-1, S-12 to S-20, C-27).
- **Goldens:** every package fingerprint moves, and the Zero Hour fingerprint moves too.

### Phase 9: rules packages configure capabilities

**9.1 Mode data from rules packages.** Touches `package`, `mode`, `declare`, 03-game-scripting.md.
- The merge of `data/rules.toml` by load order, the requirements of a rules package, and capability defaults at the mode level (A-16, engine side, D8, D9).

**9.2 Zero Hour's fixed rules as data.** Touches `import`, `packages/zero-hour`, the engine types' serde, 12-zero-hour.md.
- The fixed rules move to `packages/zero-hour/data/rules.toml`, and the importer writes only what the install gives (A-16).
- The engine types derive `Serialize`, and the importer builds them (A-17, RW 18).
- The import ends with the engine's load (A-18).
- Zero Hour's `mode.rhai` reads `ctx.ended` in place of its `ended` state field, which the rules data then drops (S-12).
- `ZeroHourRules` and passes in the style of a compiler (A-19).
- One `ImportLoss` enum (A-20).
- **Goldens:** the Zero Hour fingerprint moves.

**9.3 Common rules as data.** Touches `mode`, `combat`, `progression`, the packages, games.md.
- Victory by "last structure that counts", kill bounty and XP share as data sections (S-10).

**9.4 Production and navigation hooks.** Touches `production`, `navigation`, `orders`, `mode`.
- `on_unit_trained`, `on_site_complete` and `on_path_end`, and their reads (R-17).
- Path modes, with path following owned by navigation (R-21).

**9.5 The Zero Hour check.** Touches `zero-hour-check` and `drive.py`.
- The check takes walls, start points and resource ids from the match (A-22).
- Its measured pins move to a golden file that `./drive.py zero-hour --bless` writes (A-23, D44).

### Phase 10: the client

**10.1 Client modules and signals.** Touches the client.
- The client module list (A-1).
- `Res` for always-present resources (A-9).
- Observers and run conditions in place of polls (A-10).
- `InputMode` states (A-13).
- `Gauged` leaves the sim's entities (A-8).
- RW 21: `face_camera` in world space, the ability keys by slot kind, one ring overlay, one panel widget, one click rule, the plane sized from the map's bounds, `SLOTS` derived, a `GaugeFill` marker, the `site_percent` assert, the args' error enum, the param structs.

**10.2 Models done once for each asset.** Touches the client, `client-files` and `import` (model writer).
- `GltfExtensionHandler`, look heights from the import, meshes in the render world only (A-6).
- A glTF vendor extension for material extras (A-7, D39).
- Heights from `ModePackages`, and client files through `PackageReader` (A-11).
- RW 21: one look resolved for each object, and one packages resource.

**10.3 Presentation as data.** Touches the client, `client-files`, `import`, the packages, 03 and 12.
- Team colors (A-3).
- HUD data with screen-space bars (A-4, D42).
- The `InputMap` and the camera input numbers (A-5, D41).
- Fluent messages (A-15).
- Presentation scripts become planned (A-14).
- The build preview tints its look (RW 21).

### Phase 11: threads at scale

**11.1 Parallel kernels.** Touches `navigation`, `vision`, `stats`, `combat`, `docs/research`.
- A dated research entry first: the 3v3 tick under both executors (E-20).
- Then each kernel, kept only with its bench: the broadphase, collision islands, the pathing grid, vision groups, route plans, the pure passes for each unit (R-10, K-20 part, E-4).

**11.2 Many matches in one process.** Touches `quic`, `net`, `apps/server`, `log`. This batch waits for D1 (b).
- `ServerDir::waiting()` (N-21).
- One endpoint for each process, with routing by SNI or ALPN.
- Serial Lightyear passes for each match (N-22).
- A runner that wakes on a deadline or a datagram (N-24).
- A fault for each match (D37).
- The Rhai `sync` measurement (B-1, D2).

### Phase 12: base, dependencies and configuration

**12.1 One home in base and math.** Touches `math`, `sim`, `geometry`, `common`, `common-derive`, `quic`, `log`, `actions`, 09-determinism-core.md.
- One coordinate bound in math, and `geometry` drops `sim` (B-11).
- `NamedEnum` with one list (B-13).
- `ErrorReport` in `common` (B-20).
- `step_toward` rounds once (B-8).
- The float-lane rule in 09 (B-9).
- The `asm!` decision recorded (B-10).
- One geometry primitive for each shape pair (C-22).
- The scatter point rounds once (C-25).

**12.2 Dependencies.** Touches the workspace manifest, `net`, `bots`.
- Lightyear without its default features (N-10). `net` stays one crate (D38).
- `bevy_ecs` features and no `reflect_auto_register` (B-21).
- `rev` pins (B-22).
- `miniz_oxide` takes Bevy's version (B-23).
- `bots` derives its TOML from `Action` (R-29).

**12.3 Configuration and surfaces.** Touches the workspace manifest, every capability's `lib.rs`, `net`, the docs, ISSUES.md.
- `overflow-checks` stays in release with its comment, and the redundant `dev` line goes (B-6, D45).
- The toolchain step in PLAN.md (B-7).
- ISSUES.md sections moved, and two Foundations entries added ([Open issues](#11-open-issues)).
- RW 22: the published surfaces narrow.
- RW 23: the design text that is still out of date.
- RW 20: small items. `AiError::TimeTooLarge` becomes `Time(#[from] TimeTooLarge)`, and progression's unread `book` field goes.
- K-25: small items in combat, stats and abilities.

### Backlog: capability parameters

The rows of 4.8's parameter table are not batches of this plan. Each comes with the slice of the game that needs it, per AGENTS.md's workflow, after phase 5. Their findings: K-5 to K-10, K-16, K-23, K-24, R-8, R-9, R-13 to R-16, R-18, R-19, R-23, R-24, R-25, R-28, C-23, C-24, S-14 (data part). Each has a default that keeps today's behavior, so no golden moves until a slice uses the parameter.

## 9. Decisions

The user approved every row below on 17 October 2026: D1 to D5, D10, D13, D17, D32 to D34, D38, D45, D48 and D50 one at a time, with the research named in each row, and the others as written. A row marked "backlog" lands with the slice whose game needs it.

| # | Question | Pick | Status |
| --- | --- | --- | --- |
| D1 | Where the server's parallelism comes from | Kernels inside the heavy passes. Many matches for each process, one thread each, when a host needs density. The sim keeps its single-threaded executor. | Approved |
| D2 | Rhai `sync` | Keep `!Send`, split the data out, and measure `sync` in 11.2 | Approved |
| D3 | Where action kinds and effects live | Each capability owns its kind section and its effect | Approved |
| D4 | Books before the install | Yes | Approved |
| D5 | How sets are ordered | Stages and anchors, no `Phase` | Approved |
| D6 | How declarations reach consumers | Keep the visitor with no default methods | Approved |
| D7 | Capability identity | A closed enum, with no unbuilt variants | Approved |
| D8 | How a rules package gives a mode its data | Merge by load order | Approved |
| D9 | A rules package's requirements | The load refuses a mode that lacks them | Approved |
| D10 | Several rules scripts | An ordered list in `mode.toml`. Only the mode's script ends the match. | Approved |
| D11 | The native damage model | Closed data stages before `calc_damage` | Approved, backlog |
| D12 | Splash's edge | An edge that the data names | Approved, backlog |
| D13 | An interval while its unit is dead | The hook does not run. A policy in the modifier's data, as GAS's `EGameplayEffectPeriodInhibitionRemovedPolicy`: `continue` (default), `reset`, `fire_and_reset`. | Approved |
| D14 | Clip reload origin | For each weapon in data, with today's default | Approved, backlog |
| D15 | What a splash procs | For each weapon in data, with "each" as the default | Approved, backlog |
| D16 | Shops of several currencies | Price in every resource | Approved, backlog |
| D17 | How supply is derived | A cache kept by observers | Approved |
| D18 | Walker classes | Classes that the mode declares | Approved, backlog |
| D19 | Idle auto-acquire | Native, from data | Approved, backlog |
| D20 | How a site's train is refused | `Block::Produce` | Approved |
| D21 | Teams | From the session's terms | Approved, backlog |
| D22 | The defaults of gather and progress | Today's behavior, as data | Approved, backlog |
| D23 | The ground format | Record the 8-bit limit now. 16 bits and a base height when a game needs them. | Approved, backlog |
| D24 | Shared control | Record "one controller for each unit" now. A relation when a game needs it. | Approved, backlog |
| D25 | Tag set width | 256 bits | Approved |
| D26 | Which markers take sparse-set storage | The candidates of 4.4, kept by the battle benches | Approved |
| D27 | `ctx.p` | `ctx.params` | Approved |
| D28 | `try` and `catch` | Disabled | Approved |
| D29 | A spawned unit's handle | One `Unit` handle | Approved |
| D30 | Rhai number boxing | Measure, then move hot formulas to data | Approved |
| D31 | The result's shape | Winners and placements, with D21 | Approved, backlog |
| D32 | The pool of mode inputs (ISSUES, Decide) | The player's pool. A call fails when the pool is spent, as an action's hook does. The sender pays for the work it causes, as Ethereum's sender pays the gas. | Approved |
| D33 | The journal's corruption policy | Point-in-time recovery, as RocksDB's default: cut at the first bad frame and log what was cut. Receipts promise survival of a crash, not of a corrupt medium. | Approved |
| D34 | The server's in-memory log | State only, publish from the journal | Approved |
| D35 | Where the transport bound lives | Declared in `net_protocol`, enforced by the fork | Approved |
| D36 | The server frame's executor | Single-threaded, with a declared order | Approved |
| D37 | A journal failure in a host of many matches | End only that match | Approved, with 11.2 |
| D38 | Split `net` | One crate. Lightyear and bevy_replicon also keep both roles in one crate, the workspace build unifies features anyway, and fat LTO drops unreached code. | Approved |
| D39 | Material extras | A glTF vendor extension | Approved |
| D40 | Picking | A ray test against `Shape` | Approved |
| D41 | Key bindings | An `InputMap` in the crate, read from data, with no new dependency | Approved |
| D42 | Health bars | Screen space | Approved |
| D43 | Client modules | A module list in `campfire-client` | Approved |
| D44 | The Zero Hour check's pins | A golden file that `--bless` writes | Approved |
| D45 | `overflow-checks` in release | Keep them as a project decision, with the existing comment that says why. CLAUDE.md's profile block is a recommendation. The redundant `dev` line goes. | Approved |
| D46 | The layer test | Derive the layers and test the rules that can break | Approved |
| D47 | Where the plan lives | PLAN.md | Approved |
| D48 | The float ban and the `clippy.toml` bans against CLAUDE.md's lint list | Keep the determinism bans, each with a comment that says why, since CLAUDE.md's lint list is a recommendation: a crate-root `#![warn(clippy::float_arithmetic)]` in the sim's crates, and `disallowed-types` for hash types. The re-levelled `iter_over_hash_type` goes. A `rules` test holds the attribute. | Approved |
| D49 | Where load checks live | In each module's declaration | Approved |
| D50 | The x86-64-v3 baseline for shipped binaries | Keep v3, and record the CPU requirement as a decision. The September 2026 Steam survey shows AVX2 on 95.95% of systems, and RHEL 10 requires v3. | Approved |

## 10. Measurements

These numbers decide steps, so they come before the steps:

| Measurement | Decides | Before |
| --- | --- | --- |
| The 3v3 tick under both executors, dated in `docs/research/` | D1, the design's single thread | 11.1 |
| `integration/server_tick` and the battle benches, before and after each sparse-set marker | D26 | 7.6 |
| The `state_hash` bench with cached queries | E-17 | 7.7 |
| Allocation counts of `calc_damage`, `on_think` and `find` | D30 | 8.3 |
| `record_inputs` at the rule-bounded burst | N-11's parallel checks | 6.4 |
| The 3v3 tick with Rhai `sync` | D2 | 11.2 |
| Each parallel kernel against its serial form | R-10 | 11.1 |
| The count of distinct walker radii in a Zero Hour import | R-8 | its backlog slice |

## 11. Open issues

All 16 items of `.notes/ISSUES.md` are still true at HEAD (A8). This plan takes these:

| Item | Takes it |
| --- | --- |
| Mode, the pool of `on_mode_input` (Decide) | D32, then 8.2 |
| Stats, `Unpredicted` change ticks (Research) | Stays in Research. The removals of `Unpredicted` now reach their readers by the tick (E-1, fixed); the change ticks that a hidden unit keeps are its open part. |
| Server, a restore keeps its visibility (Ready) | 6.2 |
| Verifier, no replay from `LogCarry` (Ready) | A planned item of design. It goes to PLAN.md as its own step after 6.4. |
| Mode, a spatial map's heights unread (Ready) | Its own decision: refuse at the load, or read in the sim. With A-12 in 2.4, the client is ready for either. |
| Driver, release debug info not split (Ready) | 12.3 |
| Notes, `PERF.md` links a missing file (Ready) | 12.3 |

Section moves that 12.3 makes:
- **Quic, `admit`:** the fix is known, so it moves to Ready.
- **Navigation levels and the three slow Runner tests:** the options are known, so they move to Decide.
- **Two new Foundations entries in PLAN.md:** one for the four Quic items (no load scenario for the QUIC thread), and one for the Runner items with the Navigation item (the cost of the sweep times the replay).
- **Net, the LAN ping flake:** it stays in Research. 3.4 makes warnings from other threads visible to tests.

## 12. Findings register

Each row is one finding, or several findings that state one defect. "Step" is the plan batch that addresses it. "Backlog" is a capability parameter that a game's slice takes. Line numbers are at HEAD `b9c3d925a`.

### The sim, the schedule and the release

| Ids | Finding | Evidence | Confidence | Step |
| --- | --- | --- | --- | --- |
| E-2 | A `Restoring` world flag switches hooks into a trusting mode that each hook must remember | `restoring.rs:1-7`, `entity_kinds.rs:38`, `level.rs:59` | Confirmed | 7.5 |
| E-4 | About 90 functions take `&mut World`, so the tick is bound to one thread whatever the executor | `damage_pass.rs:103-124`, `attacks.rs:262`, `trains.rs:194` | Confirmed | 11.1 |
| E-5 | The core `actions` module depends on `stats`, and the release test of needs skips core modules | `actions/Cargo.toml:31`, `release/tests/mod.rs:311-321` | Confirmed | 5.3 |
| E-6 | `Capability` names five capabilities with no module, and `DataKind` names capability concepts in the lowest layers | `sim/src/capability.rs:12-31`, `data_kind.rs:9-48` | Confirmed | 4.5 |
| E-7 | The script `View` owns the damage-kind, resource and team names that restore checks read | `books/mod.rs:118-133`, `units.rs:208-245` | Confirmed | 6.1 |
| E-8, C-12, R-11 | `MatchMode`, `Spawner`, `ViewReader` and `CombatHooks` are non-send with no script handle in them. `Spawner` holds an `Rc<[bool]>` for one `debug_assert!`. | `mode/src/lib.rs:192-208`, `spawner.rs:19-22, 50`, `view_reader.rs:27-41`, `combat_hooks.rs:12-26` | Confirmed | 6.1 |
| E-17 | The hash and the snapshot walk every entity once for each state type | `state_registry/mod.rs:546-561, 642-660` | Plausible: needs the bench | 7.7 |
| E-18 | A restore moves each entity once for each state type it holds | `state_registry/mod.rs:400-424, 662-684` | Confirmed | 7.5 |
| E-19 | `StateRegistry` is a type list, a codec, a change tracker and a mode switch | `state_registry/mod.rs:57-64` | Confirmed | 7.7 |
| E-20 | The 2.5× figure for the parallel executor has no dated research entry | `02-engine-core.md:168` | Confirmed | 11.1 |
| E-21, C-11, K-14, R-27 | No component uses sparse-set storage, though markers toggle during play | `dying.rs:56, 100`, `forced_moves.rs:153`, `thinking.rs:93`, `out_of_play_marks.rs:46` | Confirmed. The gain is plausible. | 7.6 |
| E-22 | A spawn looks up each part's `ComponentId` again, fetches `ByType` for each part, and holds a `Mutex` that it never locks | `entity_parts.rs:47-79`, `spawn_parts.rs:18, 70-84` | Confirmed | 7.5 |
| E-23 | Every match builds its own books | `mode_packages/mod.rs:127-133` | Confirmed | 8.3 |

### The core capabilities

| Ids | Finding | Evidence | Confidence | Step |
| --- | --- | --- | --- | --- |
| C-1 | Action kinds are a closed enum and a flat `ActionData` of 41 fields, with a hand table of the fields for each kind, and `expect`s that package data can reach | `kind_spec.rs:11-18`, `action_data/mod.rs:36-131`, `action_data_field.rs:16-58`, `kind_data.rs:48-79` | Confirmed | 5.1 |
| C-2, S-8 | Listed effects are a closed core enum of other capabilities' effects, and data effects and script calls are two vocabularies | `capability_does_spec.rs:11-55`, `effect_data.rs:15-42` | Confirmed | 5.2 |
| C-3 | Costs, targets and passives tie the action pipeline to `stats`, and a target must have `Life` | `holds.rs:7-9`, `targets.rs:11, 85-90`, `purse.rs:2` | Confirmed | 5.3 |
| C-7 | `StatusTags` and `SpawnPoint` sit on every body unit, though one capability uses each | `units.rs:159-161` | Confirmed | 4.5 |
| C-8 | `Player` and `MatchEntity` are named `players.*` but live in `units` | `player.rs:16-48`, `match_entity.rs:13-35` | Confirmed | 5.5 |
| C-9 | A unit spawns in two archetype moves, because its facing travels on the half-built entity | `unit_core.rs:35-80`, `facing.rs:39-42` | Confirmed | 7.5 |
| C-10 | Spawn hooks run on the mode's spawn path only, so a passive on a delivery type is dropped with no error | `spawner.rs:54`, `delivery_spawner.rs:66-68` | The load already refuses a passive on a delivery type, as `NotDelivery`; a test pins it. The spawn paths' split stays. | 7.5 |
| C-13 | Each `UnitsNear` reader holds `ResMut`, because the first reader rebuilds the grid | `unit_grid.rs:25-58` | Confirmed | 6.1 |
| C-14 | The match entity and the players are found through the `BTreeMap` index on each call | `on_match.rs:18-25`, `player_rows.rs:25-59`, `targets.rs:49-51` | Confirmed. The cost needs a bench. | 7.2 |
| C-15, S-24 | The script view walks and copies every entity for each script batch. Each call copies the mode state and every player's resources and resolves its literal names again. | `view_reader.rs:136-180`, `scripts_world.rs:15-17`, `mode_call.rs:194-208` | Plausible: needs a bench | 7.2 |
| C-17 | `want_passives` scans every unit in four sets, and `end_lifespans` scans every lifespan in each tick | `actions/src/lib.rs:242-250`, `unit_ticks.rs:31-42` | Confirmed | 7.1 |
| C-18 | Slots, times and progress are three components that stay in step only by index, and each `zip` truncates silently | `action_slots/mod.rs:30-33`, `slot_times.rs:234-354` | Confirmed | 7.4 |
| C-22 | Six geometry predicates answer one question with four names, three result types and both boundary rules | `metric.rs:58-117`, `placed/mod.rs:30-35`, `body_box/mod.rs:143-152` | Confirmed | 12.1 |
| C-23 | The sim's ground is 8-bit samples above 0, with no base height | `height_grid.rs:11-59` | Confirmed | Backlog (D23) |
| C-24 | A unit has exactly one controller, its owner | `owner.rs:12-44`, `tick_commands.rs:70-74` | Confirmed | Backlog (D24) |
| C-25 | The scatter point chains three roundings | `weapon/mod.rs:59-74` | Confirmed | 12.1 |
| C-26, B-11 | The coordinate bound is a `sim` constant and five `1 << 47` literals in math, and `geometry` depends on `sim` to read it | `position.rs:19-32`, `direction/mod.rs:14-17`, `bounds.rs:17-26` | Confirmed | 12.1 |
| C-27 | `unit.target` and `hit.target` mean two things | `actions_api.rs:16-24`, `deliveries_api.rs:30-34` | Confirmed | 8.5 |

### The script system

| Ids | Finding | Evidence | Confidence | Step |
| --- | --- | --- | --- | --- |
| S-1 | `ctx.remove` binds modifiers only, though its text promises projectiles and areas | `stats_api.rs:107-114` | Checked. Its text now promises modifiers only. | 8.5 |
| S-3 | The load checks a call's name and role, not its argument count or types. `unit.params` names are unchecked. | `script_facts/mod.rs:63-67, 253-263`, `member_spec.rs:20-22` | Confirmed | 8.1 |
| S-4 | Names that the API returns are compared as strings that no check reads, and some drop their package scope | `view/mod.rs:199-204`, `damage_handle.rs:70-75` | Confirmed | 8.1 |
| S-5 | The hook contract is a hand-kept core enum, and call sites pass untyped tuples | `hook.rs:9-135`, `script_batch.rs:84-99` | Confirmed | 8.2 |
| S-6 | Data field lists are kept by hand three times beside the serde schema | `api_builder.rs:261-302`, `effect_data.rs:233-235` | Confirmed | 8.1 |
| S-7 | `ApiOwner`, `DataTable`, `NameKind`, capability cases of `ApiError` and `CallStart`'s fields name capabilities in the core | `api_owner.rs:5-16`, `data_table.rs:5-42`, `error.rs:46-58` | Confirmed | 8.2 |
| S-9 | A match runs one rules script, so a rules package and a mode's script cannot both answer one event | `mode_packages/mod.rs:164-186` | Confirmed | 8.4 |
| S-10 | Victory, kill bounty and XP share are written again in each mode's Rhai, with a scan for each death | `packages/zero-hour/scripts/mode.rhai:39-70`, `3v3/scripts/mode.rhai:99-231` | Confirmed | 9.3 |
| S-11 | The mode's event queues are written four times by hand and name combat's and progression's types | `mode_hooks.rs:96-276` | Confirmed | 8.2 |
| S-12 | A second `ctx.end` fails the whole call, and no read of the result exists | `mode_api.rs:351-355, 482-491` | Confirmed. A second end now does nothing, `ctx.ended` reads the end, and the 3v3 dropped its `ended` state; Zero Hour's script keeps its guard until 9.2. | 8.5, 9.2 |
| S-13 | Players and teams have no handles, and `set_relation`, `set_out` and `add_resource` have no reads. `add_resource` can drive gold below zero. | `mode_api.rs:128-411`, `production_api.rs:48-63` | Confirmed | 8.5 |
| S-14 | `enemy_team` assumes two teams | `mode_api.rs:176-190` | Confirmed | 8.5 |
| S-15 | List queries differ in whether they include the dead | `mode_api.rs:788-792`, `view/mod.rs:155-169` | Confirmed | 8.5 |
| S-16 | No effect call takes a source, though the design says "unless a call names another". The mode cannot reveal. | `combat_api.rs:210-327`, `vision_api.rs:108-139` | Confirmed | 8.5 |
| S-17 | Orders take a unit that may only be the acting unit, and AI scripts may apply effects | `orders_api.rs:193-279` | Confirmed | 8.5 |
| S-18 | A spawned unit is a second handle type that a script cannot store | `new_unit.rs:7-30`, `state_value.rs:66-75` | Confirmed | 8.5 |
| S-19 | Timers take a flag argument, have no cancel, and have unchecked names | `mode_api.rs:356-365, 718-831` | Confirmed | 8.5 |
| S-20 | Script state has no enum, list of scalars or state for each player | `state_type.rs`, `state_decl/mod.rs:35-60` | Confirmed | 8.5 |
| S-21, B-1 | Rhai without `sync`, and `Rc` and `RefCell` in the runtime, pin a match to its thread. Name reads are pinned with it. | `Cargo.toml:198`, `view/mod.rs:43-55`, `ctx.rs:19-27`, `script_host/mod.rs:69-80` | Confirmed | 6.1, 11.2 |
| S-22 | Each load binds the whole API to parse, and each match and restore binds again and compiles from source | `mode_packages/mod.rs:145-160, 376-377`, `match_build/mod.rs:52-55` | Confirmed. The cost is plausible. | 8.3 |
| S-23 | Each `Num`, vector and handle in Rhai is two heap allocations, and handles carry an `Rc<View>` | rhai `dynamic.rs:98`, `view/unit.rs:8-13` | Plausible | 8.3 |
| S-25 | The choice of pool and the policy for a spent pool are decided at about eight call sites | `delivery_hooks.rs:81-91`, `modifier_hooks.rs:213-222`, `thinking.rs:117-133` | Confirmed | 8.2 |
| S-27 | The builder records "runs or planned" four ways, and nothing compares a name's `since` with a package's `api` | `api_builder.rs:184-302` | Confirmed | 8.1 |
| S-28 | The ban on function pointers also forbids lambdas that capture nothing | `script_facts/mod.rs:135-191` | Plausible | 8.3 |
| S-29, A-14 | Presentation scripts are designed as current, with no role, no host and no planned entry | `03-game-scripting.md:7-8, 20, 44`, `script_role.rs:287-294` | Confirmed | 10.3 |

### Combat, stats, abilities, areas, projectiles, items

| Ids | Finding | Evidence | Confidence | Step |
| --- | --- | --- | --- | --- |
| K-5 | Splash uses `<`, while the one reach rule is `≤` | `attacks.rs:120-122`, `metric.rs:54-73` | Confirmed | Backlog (D12) |
| K-6 | A clip's reload counts from the attack's start, where the design and Zero Hour count from the shot | `attacks.rs:428-435` | Confirmed | Backlog (D14) |
| K-7 | A weapon's own damage and period cannot be scaled by a stat | `weapon/mod.rs:107-163` | Confirmed | Backlog |
| K-8 | The only damage model past armor shares is a Rhai call for each damage entry, with no pool | `damage_pass.rs:172-190`, `armor.rs:206-244` | Confirmed shape. The cost is plausible. | Backlog (D11) |
| K-9 | Leech has two fixed bindings, one heal scale, and each splashed unit gets on-hit effects and leech | `combat_book.rs:16-21`, `damage_pass.rs:405-442`, `attacks.rs:106-134` | Confirmed | Backlog (D15) |
| K-10 | One rule each: first weapon by slot, one attack at a time, one turret for each type, attack projectiles at a unit | `attacks.rs:215-218`, `turret/mod.rs:19-25`, `load_check.rs:2162-2164` | Confirmed | Backlog |
| K-11 | Units carry `ClipShots`, `RecentAttackers`, `Level`, `Modifiers` and `ModifierClocks` that their mode never reads. `RecentAttackers` keeps every attacker that still exists. | `combat/src/lib.rs:167, 189-191`, `recent_attackers.rs:135-144`, `stats/src/lib.rs:245-248` | Confirmed | 4.5 |
| K-13 | About twenty passes scan every unit in each tick to find timers that are due | `volley.rs:27-49`, `attacks.rs:145-253`, `modifier_ends.rs:16-24`, `casts.rs:55-228`, `area_life.rs:329-334` | Confirmed scans. The cost is plausible. | 7.1 |
| K-16 | An area's `inside` and an aura do one job in two shapes, and disagree on neutrals and lifeless units | `area_spec.rs:94-101`, `area_holds.rs:47-58`, `held_pass.rs:131-139` | Confirmed | Backlog |
| K-17 | The one action pipeline is four hand-built loops | `attacks.rs:145-414`, `casts.rs:55-503` | Confirmed | 5.3 |
| K-18 | A projectile reports through two channels, by its payload, and only one of them splashes | `flights.rs:406-434` | Confirmed | 5.4 |
| K-20 | Scripted passes hold the world for their whole run, and their hook state is non-send | `combat_hooks.rs:12-15`, `modifier_hooks.rs:46-50` | Confirmed shape | 6.1, 11.1 |
| K-21 | The damage pass reads each unit through world lookups, a heard event allocates a handle for each listener, and `NextDamage` drops its buffer | `damage_pass.rs:213-429`, `modifier_hooks.rs:78-159` | Plausible | 7.2 |
| K-22 | A death ends nothing that is aimed at its unit, and a respawn moves the unit with no `Teleported` | `dying.rs:183-190`, `flights.rs:147` | Plausible | 5.5 |
| K-23 | An item's cost may name several resources, and a shop takes one | `item_data.rs:18`, `item_book.rs:107-112`, `load_check.rs:1384-1407` | Confirmed | Backlog (D16) |
| K-24 | `Pools` is a fixed 256-byte array for each unit | `pools.rs:22`, `meter.rs:7-13` | Plausible sizes | Backlog |
| K-25 | Small items: `ModifierBook` cloned past the borrow checker, a `clear` with no caller, a hand-spelled `Changed` scan, mixed argument names | `damage_pass.rs:348`, `pass_queue.rs:329`, `toggles.rs:44-54`, `combat_api.rs:205-232` | Confirmed | 12.3 |

### Navigation, production, orders, progression, interaction, vision, mode, bots

| Ids | Finding | Evidence | Confidence | Step |
| --- | --- | --- | --- | --- |
| R-4 | Exclusive production and AI systems run in each tick and scan their whole population | `trains.rs:94-192`, `construction/mod.rs:82-352`, `gather_loop/mod.rs:144-150`, `destinations.rs:19-111` | Confirmed | 7.1 |
| R-5 | Gathering scans every node and worker, and its loop sees its own writes as changes | `gather_view.rs:148-200`, `gather_loop/mod.rs:137-243` | Confirmed scans. The trigger is plausible. | 7.3 |
| R-6 | `ArrivedShort` is derived again in ten gaps for each tick, and two passes scan every walker | `navigation/src/lib.rs:219-223`, `routing.rs:35-145` | Confirmed | 7.3 |
| R-7 | Collide reads every static body into its collider list in each tick | `walking.rs:251-277`, `broadphase.rs:129-136` | Confirmed | 7.3 |
| R-8 | One clearance and region set for each distinct radius | `walker_kinds.rs:37-42`, `pathing_grid/mod.rs:48-136` | Plausible count | Backlog (D18) |
| R-9 | The route work limit is the map's cell count | `route_planner/mod.rs:24, 173` | Plausible | Backlog |
| R-10 | The heavy navigation and vision passes have deterministic parallel forms | `broadphase.rs:96-128`, `collider/mod.rs:46-53`, `seeing.rs:56-75` | Plausible | 11.1 |
| R-12 | Supply and holdings are counted from scratch twice in each tick, through a shared flag memo | `trains.rs:83-114`, `player_supply.rs:44-62`, `holdings.rs:17-58` | Confirmed | 7.2 |
| R-13 | Supply is one quantity with a constant cap | `supply.rs:18-127` | Confirmed | Backlog |
| R-14 | Requirements are AND only, so Zero Hour's either-or prerequisites are dropped | `requirement_book.rs:12-66`, `production_import.rs:280-294` | Confirmed | Backlog |
| R-15 | A train queue makes units only, at a fixed end tick, while a site progresses by a rate | `train_queue.rs:24-93`, `site.rs:84-105` | Confirmed | Backlog |
| R-16 | One gatherer for each node, pass-through only for `gathering`, and navigation names gatherers | `node.rs:14-17`, `steering.rs:25-100`, `collider/mod.rs:16-74` | Confirmed | Backlog |
| R-17 | Production and navigation fire no script hooks | `hook.rs:10-35` | Confirmed | 9.4 |
| R-18 | No repair, no consumed builder, no build on a unit, no terrain rules, no grid snap | `build_book.rs:36-40`, `placement.rs:59-62` | Confirmed | Backlog |
| R-19 | No attack-move, patrol, hold or auto-acquire, and the MOBA leash is a native order | `unit_order.rs:18-161`, `thinking.rs:91-98` | Confirmed | Backlog (D19) |
| R-20 | `orders` ends other orders by writing navigation's and production's parts | `unit_order.rs:56-233` | Confirmed | 5.5 |
| R-21 | Path walking is split over three crates and walks one way only | `path_walker.rs`, `destinations.rs:19-57`, `spawn_group.rs:23-48` | Confirmed | 9.4 |
| R-23 | Sight is fixed state for each type, not a stat | `sight.rs:16-55` | Confirmed | Backlog |
| R-24 | One point for each level of one track, on units only | `points.rs:18-45` | Confirmed | Backlog |
| R-25 | Teams fixed by the manifest, `Won(Team)` or `Draw`, no factions, options for scripts only | `team_manifest.rs:7-10`, `match_end.rs:27-30`, `option_data.rs:9-13` | Confirmed | Backlog (D21, D31) |
| R-26 | `Site`, `Route` and `Node` mix progress with configuration | `site.rs:19-26`, `route.rs:22-32` | Confirmed | 7.4 |
| R-28 | `LoadoutSetup` and `Loadout` are one shape | `loadout_setup.rs:8-11`, `loadout.rs:9-12` | Confirmed | Backlog |
| R-29 | The bot script copies a subset of orders by hand, by slot numbers and absolute positions | `order_script/mod.rs:141-206` | Confirmed | 12.2 |
| R-30 | Static changes and box spawns walk every walker | `static_tracking.rs:98-124`, `room_making.rs:61-70` | Confirmed | 7.3 |
| R-31 | `production` depends on `navigation` for a `Destination` write and two queries | `trains.rs:12, 330`, `placement.rs:7` | Confirmed | 5.5 |

### Sessions, the network and the server

| Ids | Finding | Evidence | Confidence | Step |
| --- | --- | --- | --- | --- |
| N-1 | A fragment count from the peer sizes an allocation before any check | `lightyear transport/channel/fragments/receive.rs:137-147` at `54e6be2` | Checked | 1.1 |
| N-2 | A link that did not join uses every channel with no budget | `net_protocol/mod.rs:96-117`, `offering.rs:40, 176-193` | Confirmed | 1.1 |
| N-3 | The single-threaded executor fixes the server frame's order, which no set declares | `sim_server_plugins.rs:39-57`, `sim_server/mod.rs:151-200` | Confirmed | 6.3 |
| N-4 | The server frame is a run of exclusive systems, to pair `Session` with `ServerSigner` | `door.rs:64, 231-242`, `checkpoints/mod.rs:101, 209-214` | Confirmed | 6.2 |
| N-5 | Start and restore build the match through two paths and five parameter bags. The abort path writes server resources into a replay world. | `sim_server/mod.rs:258-496`, `session_dir/mod.rs:230-317` | Confirmed | 6.2 |
| N-6 | Terms are checked up to three times, and the checked result is dropped | `session_rules/mod.rs:55-137`, `session/mod.rs:64-87`, `opening/mod.rs:67, 82` | Confirmed | 6.2 |
| N-7 | The server's lifecycle is the set of resources that exist | `sim_server/mod.rs:151-233, 617-630` | Confirmed | 6.3 |
| N-9 | Replicated entities get `Replicate`, `Rooms` and `PredictionTarget` after their spawn | `kind_shows.rs:86-112`, `viewer.rs:136-190` | Confirmed | 7.5 |
| N-10 | Lightyear's default features add interpolation, and the server binary links the client stack | `Cargo.toml:186`, `net/src/lib.rs:108-122` | Confirmed | 12.2 |
| N-11 | Every signature check and signing runs on the tick thread, one slot after another, with a syscall for each signature's aux randomness | `sim_server/mod.rs:503-583`, `signer.rs:81-105` | Plausible cost | 6.4 |
| N-12 | A checkpoint due before the last one's record arrives waits on its fsync | `checkpoints/mod.rs:75-120` | Plausible | 6.4 |
| N-13 | Each record is encoded two or three times, with a wake for each record | `session_log/mod.rs:557-770`, `journal_records.rs:250-255` | Confirmed | 6.4 |
| N-14 | The server keeps the whole session in memory and publishes from it, against 10-sessions.md | `session_log/mod.rs:87-88`, `session_dir/mod.rs:178-220` | Confirmed drift | 6.4 |
| N-17 | `SessionLog` has 21 fields, a hidden replay mode and a signature mode | `session_log/mod.rs:77-120, 1137-1144` | Confirmed | 6.4 |
| N-18 | A slot's controller has four types, and the link-to-slot fact three maps kept equal by hand | `slot_control.rs:30-40`, `controller.rs:184-193`, `log_carry.rs:227-273`, `door.rs:242-293` | Confirmed | 6.4 |
| N-20 | Package facts are copied into the session, and every caller builds the session rules again | `session/mod.rs:34-93`, `session_rules/mod.rs:55-65` | Confirmed | 6.2 |
| N-21 | A data directory holds one unpublished session | `session_dir/mod.rs:111-145` | Confirmed | 11.2 |
| N-22 | All apps in a process share the global compute pool, whose scope runs other apps' tasks | `sim_server_plugins.rs:27`, `bevy_tasks task_pool.rs:310-330` | Plausible | 11.2 |
| N-23 | No log line names its session | `events/input_logged.rs:19-27` | Confirmed | 6.3 |
| N-24 | A 2 ms frame loop runs every Lightyear system 500 times a second for each match | `net_protocol/mod.rs:73-76`, `apps/server/src/main.rs:193` | Plausible | 11.2 |
| N-25 | Small allocations for each message on the network paths | `offering.rs:259-263`, `input_ack.rs:18-27`, `chain_history/mod.rs:45-52` | Confirmed | 6.5 |
| N-26, B-19 | `net` depends on `combat` for a log line and on `vision` for interest | `sim_server/mod.rs:16, 654-665`, `team_rooms.rs:4`, `kind_shows.rs:13` | Confirmed | 5.5 |
| N-29 | A test's per-tick hash runs in the production schedule, and an error variant names the wrong fingerprint | `sim_server/mod.rs:224, 473-476`, `session/error.rs:37` | Confirmed | 6.2 |

### The client, the importer and the checks

| Ids | Finding | Evidence | Confidence | Step |
| --- | --- | --- | --- | --- |
| A-1 | No client module list: `if`s in `add_ends`, `Option` params and `expect`s in shared systems | `client/src/main.rs:196-283`, `hud/mod.rs:23-24`, `view/mod.rs:543-552` | Confirmed | 10.1 |
| A-3 | Models carry no team color, and the palette has three teams | `view/mod.rs:91-92, 324-328` | Confirmed | 10.3 |
| A-4 | The HUD and the fallback shapes are sized for the MOBA | `hud/gauge.rs:80-91`, `view/look.rs:37-71` | Confirmed | 10.3 |
| A-5 | Keys and two Zero Hour camera numbers are Rust constants in the client | `orders.rs:57-71`, `player_camera.rs:43-46`, `camera_rig.rs:77-80` | Confirmed | 10.3 |
| A-6 | Each model instance is dressed after it spawns, a pass polls every frame, and every mesh keeps a CPU copy | `unit_models.rs:162-219`, `view/mod.rs:585-645` | Confirmed path | 10.2 |
| A-7 | A material file restates the whole glTF material, and the sampler comes from the file | `material_file.rs:9-39`, `unit_models.rs:221-246` | Confirmed | 10.2 |
| A-8 | Each projectile and unit gets client components inserted on the sim's entity | `hud/mod.rs:177-189, 328-425` | Confirmed | 10.1 |
| A-9 | `Option<Res<..>>` for resources that the plugins insert at build | `own_player.rs:11`, `view/mod.rs:852-860`, `inventory_bar.rs:103-104` | Confirmed | 10.1 |
| A-10 | Systems run every frame where a change, a run condition or an observer fits, and `CameraRig` is always changed | `view/mod.rs:262-875`, `build_preview.rs:85-242`, `camera_rig.rs:208` | Confirmed | 10.1 |
| A-11 | The client decodes the map's heights a second time and has its own package reader | `client_data/mod.rs:73-168` | Confirmed | 10.2 |
| A-13 | Placing a build is a mode held by whether a resource exists | `placing.rs`, `orders.rs:78-81`, `selecting.rs:68-69` | Confirmed | 10.1 |
| A-15 | The match's end shows as English text in code | `view/mod.rs:851-875` | Confirmed | 10.3 |
| A-16 | Zero Hour's fixed rules come from importer Rust, because a rules package can hold no data | `rules_package.rs:35-177`, `imported_type.rs:130-208`, `mode_packages/mod.rs:188-205` | Confirmed | 9.1, 9.2 |
| A-17 | The importer mirrors engine types by hand, with filters and modes as strings | `imported_type.rs:28-337`, `unit_types.rs:40-67` | Confirmed | 9.2 |
| A-18 | The importer repeats load checks by hand and never loads what it wrote | `imported_type.rs:181-192, 375-384`, `zero_hour/mod.rs:163-325` | Confirmed | 9.2 |
| A-19 | The import's stages share one mutable name table and bags of sources, so their order is a hidden contract | `zero_hour/mod.rs:163-325`, `map_import.rs:85-96` | Confirmed | 9.2 |
| A-20 | The import report is nine lists of different shapes, with paths as strings | `zero_hour/mod.rs:81-111`, `import/src/main.rs:43-127` | Confirmed | 9.2 |
| A-22 | The check computes walls, start markers and the resource table again | `start_drive.rs:130-185`, `skirmish_play.rs:120-129` | Confirmed | 9.5 |
| A-23 | Measured pins with no bless path | `pins.rs:15-115` | Confirmed | 9.5 |
| A-24 | The LAN check shares the server's paths. Its one open item is the ping flake in ISSUES.md. | `lan-check/src/main.rs:57-82` | Confirmed | No change |

### The base crates, math, tests and dependencies

| Ids | Finding | Evidence | Confidence | Step |
| --- | --- | --- | --- | --- |
| B-6 | The profiles add `overflow-checks` to the user's block | `Cargo.toml:238-254` | Confirmed | 12.3 (D45) |
| B-7 | PLAN.md is empty, the open batches live in a review file, and the toolchain pin cites a missing step | `PLAN.md`, `rust-toolchain.toml:1-4` | Confirmed | First step, 12.3 |
| B-8 | `checked_step_toward` divides by a length that was already rounded | `vec3/mod.rs:172-200` | Confirmed path | 12.1 |
| B-9 | The lane root and lane division rest on IEEE default rounding, which 09 says they do not | `simd/u64x4/mod.rs:18-43`, `simd/i64x4/mod.rs:88-97` | Confirmed | 12.1 |
| B-10 | Math's inline `div` is `unsafe` that no outside API asks for, with no recorded decision | `wide_division/x86_64.rs:14-26` | Confirmed | 12.1 |
| B-13 | Two member lists for each named enum, and a linear search on each encode | `common-derive/src/lib.rs:47-105`, `named_enum.rs:19-44` | Confirmed | 12.1 |
| B-20 | `quic` depends on `log` for `ErrorReport` alone | `quic/Cargo.toml`, `error_report/mod.rs` | Confirmed | 12.1 |
| B-21 | Features the build does not use: `bevy_ecs` defaults, `reflect_auto_register`, Rhai's module system | `Cargo.toml:154, 165, 198` | Confirmed | 8.3, 12.2 |
| B-22 | The forks are patched by branch, not by revision | `Cargo.toml:386-416` | Confirmed | 12.2 |
| B-23 | `miniz_oxide` is pinned beside Bevy's own version | `Cargo.toml:188` | Confirmed | 12.2 |
| B-24 | The archetype watch starts after the spawn, so "spawns whole" has no test | `archetype_watch.rs:10-14` | Confirmed | 7.5 |

### The older review's open items

Only the items that A8 found still true at HEAD. The rest of `.notes/review-workspace.md` is fixed, or the batch that takes it rechecks it.

| Item | Finding | Step |
| --- | --- | --- |
| RW 18 | `units.toml`, `mode.toml` and the manifest come from hand-mirrored structs and `format!` templates | 9.2 (with A-17) |
| RW 20 | The deliveries' two shapes, `take_shots` and `launch` on the client, the doc "It builds on combat", the engine tags by hand, the control resolution, the use slot that items extends, the shop index, the naming of order inputs, `AiError::TimeTooLarge`, progression's unread `book` field, charges from progression's `Learning` | 4.1, 5.4, 5.5, 7.2, 10.1, 12.3 |
| RW 21 | The client items: `face_camera`, keys by slot kind, the sampler, the panels each frame, `ClientData`, the packages resource, `place_objects`, the ring overlays, the panel widget, two click rules, the plane's size, `SLOTS`, `Fills`, `Option<Res<UnitLooks>>`, `site_percent`, the args, the wide signatures, the preview's tint | 10.1, 10.2, 10.3 |
| RW 22 | Published surfaces that are wider than their callers | 12.3 |
| RW 23 | 02's `common` row still omits `NamedEnum`, the hashers and the allocation audit. production.md's Supply text differs from the code. | 12.3, 7.2 |
