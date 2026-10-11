# Architecture audit: recommendations

An audit of the workspace at `source/` after slice 3's A4: the entities and how each is spawned and replicated, how capabilities declare and register their parts, the crate and module graph, the call graph and argument counts, restore robustness, and code quality. Four read-only audits ran in parallel; each main claim was checked against the code. Line numbers are those of the working tree when the audit ran. The bugs it found are in the issue log; this file holds the structural recommendations.

## Findings

- **Registration.** Each module declares once what it adds; effect queues, deliverers and cast spends still register in install bodies, and systems order against each other's sets in each install.
- **Entities.** Every unit spawns through one core, the kinds declare their audience and their parts, and net reads the kinds; a unit's stats still derive after its spawn.
- **Ownership.** A script function reaches the match only through `Ctx`, never the world, so `View` keeps what a call reads: names, unit types, and the call's snapshot of the tick, rate, bounds and metric. `ModeBook` holds the script engine's values, which are not `Send` while Rhai builds without `sync`, so systems read it through the non-send `MatchMode`, and script calls through `Ctx`. A script batch holds its `ctx`, and combat's passes take `CombatHooks` out of the world as one group. `deliveries` and `players` have no tests, `actions` 6 for 53 files.
- **Arguments.** Systems take `TickClock`, `ModifierParams`, `StatBooks` and `ProductionBooks`, and the client's `OwnPlayer`, and a call's tick travels in its frame. A unit's life pool still travels beside its pools query in nine systems: the query sits inside each system's own tuple query, which already borrows `UnitPools`, so a separate pools param would conflict with it.

The recommendations consolidate into five systems. Each is one declaration that every consumer reads, enforced by the compiler where it can be, else by one generic test, never by memory.

| System | Replaces | Enforced by |
| --- | --- | --- |
| 1. One declaration per capability (§1) | the registrations left in install bodies, restated orderings | the compiler (visitor with no defaults), the declaration audit test |
| 2. One entity model (§2) | stats derived after the spawn, two sight books | required components, the compiler (`Host` with no default), one spawn core for every unit |
| 3. Explicit dependencies (§3) | five match-world builders, the log in the core, layers with cycles | one builder, the layer test |
| 4. Restore as a checked boundary (§4) | value-only mutations, checks of the load copied in builders | the registry, a mutation fuzz test |
| 5. One session host (§5) | three server compositions, two replay loops, two restore glues | one type |

One policy holds across all five: **what can be forgotten is a type or a trait method with no default.** A rule that no type can hold, as "no spawn outside the kinds' spawn functions" or "a script runs only inside a `ScriptBatch`", is held by a type that makes the right way the short one, a test where one can see the breach, and review. `clippy.toml` bans stay for what they ban today, each group owned by a proxy crate, `store`, `codec` or `schedule`, or banned everywhere.

## 1. One declaration per capability

Each module implements `Declares`, and `Release` visits the core's, each declared capability's and the mode's declarations with one consumer at a time: `Installer` (registry and world), `ApiBinder`, `StateOnly` (net's lists) and the declaration audit test. Installs take `InstallCx` and ask it for the declared capabilities and for scripts. Books name their owner, and `Books::install` refuses one its installed owner did not declare. What stays:

- **Registrations a declaration cannot name yet.** Effect queues, deliverers and cast spends register in install bodies: their function types name `actions`' and `abilities`' types, above `declare`'s layer. The declaration audit checks the queues from a match instead.
- **Audit rules not built.** A column's row parts are declared types; a capability's imports lie within its needs, its optional uses and the lower layers (with §3.6).
- **The map's sections.** The load check takes the owners of the mode's data sections from the script API's records, and names the map's three sections' owners by hand, as no data table records them.
- **Two lists of capabilities.** The install order is `CAPABILITIES` in `release`, and the needs `CapabilitySet::needs` in `declare`; a test holds every capability after what it needs.

### 1.4 Each module orders only its own systems

`orders` configures four other modules' sets, and `.after(StatsSet::Regenerate).after(CombatSet::Respawn)` is restated in four modules. Each module keeps its own `SystemSet`, and orders only its own systems and sets, against the public sets of the modules it builds on, as a Bevy plugin orders its systems against the plugins it depends on; no module configures another's sets. A shared phase enum chained once in the installer is not the fix: its phases would name the capabilities' concerns in the core, and each phase edge orders pairs that never touch, so parallelism goes. Ambiguity detection keeps checking the pairs that touch.

## 2. One entity model

Each kind of entity implements `EntityKind`: who receives it, whether its owner's client predicts it, and the filter of its entities alone. The core visits its kinds once, and net shows each kind by that visit, with one rooms rule and one prediction rule, which follows a change of owner. The match's and the players' entities take their parts from `part::<K, C>(start)` declarations. Every unit, a mode's, a delivery and a test's, spawns through `UnitCore`, which puts on its stable id, place, type, team, owner, its type's tags and its script state; a test match seals its unit types before its first spawn, as a match loads them all first. What stays:

- **Stats at the spawn.** A unit's stats derive in the first refresh after its spawn, not in the spawn, so a unit spawned in the middle of a stage has none until the stage's edge, and a client derives none. This part belongs to PLAN A6.
- **Sight.** A mode's unit takes its sight from its kit, a delivery from the projectile and area books.
- **Despawn** happens in five places.
- **Unit rows.** `UnitRows::row` tests a unit by `Position + Team`, where the other filters name `UnitEntity`.
- **Audit rule.** No rule of the declaration audit checks, on a played match, that every entity with a stable id is of a declared kind.

## 3. Explicit dependencies

### 3.4 One match-world builder

The prepare, schedule, registry and install sequence appears five times (`Session::build`, `Session::snapshot_hash`, `SimClient::build`, `TestMatch::unstarted`, the match build bench). `MatchWorld::new(seed, rate).install(set, budgets).finish()` holds it, built on §1's visitors.

### 3.5 The log stays out of the core

`Session::run_tick` logs `ScriptCallFailed` (`runner/src/session/mod.rs:319`), while design 02 lets only the binaries, net and the verifier log. `run_tick` returns the failures, and its callers log them.

### 3.6 Layers without cycles

The layer test rejects imports from a higher layer only. Within a layer, modules import each other in both directions: `units ↔ scripts`, `units ↔ players`, `abilities ↔ items` (`items/mod.rs:6` imports `CastSpends`; four `abilities` files import `Inventory`). The needs table disagrees with the imports: production uses navigation and vision and needs neither; abilities uses items and navigation; orders uses production and items. The declaration audit adds both checks: no cycle within a layer, and imports within the capability's needs, its optional uses and the lower layers. An optional use is correct: production runs without navigation, with no gathering. So each capability declares `USES`, the capabilities it reads when the match has them, and each such read is guarded by `cx.has`. `CastSpends`, a registry of hooks, moves down into `actions`; `abilities` reads a unit's inventory through a hook that `items` registers, as an item's state has no place in the core. `View` and its API users meet in one layer-1 module.

## 4. Restore as a checked boundary

### 4.4 Load, then trust

51 builder sites `expect("the load checked …")` what `load_check.rs` (2,275 lines) checked; the two copies can drift. Builders that take checked types would remove them. This is a larger change, for the issue log.

## 5. One session host

- **Server compositions.** The task pool, time, states and `SimServer` setup appears three times (`server/src/main.rs:234`, `net/src/local/local_server/mod.rs:327`, `net/src/harness/in_process_match/mod.rs:332`): `SimServer::app(tick, &ModePackages) -> App` holds it, and the server's edges to `capabilities` and `sim`, each used once, go.
- **Restore glue.** The server's `Restore` and net's `PendingRestore` have the same fields and body: one type in net.
- **Replay.** The loop of "take the checkpoint again, stop at a tick, run alone" is in `SimServer::restore_match` (`sim_server/mod.rs:351`) and `SessionDir::abort` (`session_dir/mod.rs:232`): `Session::replay_to(world, tick, on_boundary)`.
- **Tick input.** "Fill `TickInputs`, then run `SimUpdate`" is in `Session::run_tick` and `run_predicted_tick`: one function in `sim`.
- **`Session` and `Runner`.** `Session` is a resource inside the world it runs, so its methods take `&self` and that world, and `Runner` is twenty forwarders. `Session` splits into a `SessionRecord` outside the world (log, seeds, segment) and match state inside it, or design 02 says why it is one.
- **Crate split.** The server crate is a binary of 933 lines, while design 02 gives it the lifecycle, saves and the local server, which live in `net`: an item under Decide in the issue log.
- **Client edge.** `client` depends on `runner` for one rate check, which moves into `SimClient`'s setup.

## Single sources, outside the five systems

- **Audiences.** A kind's audience is in `DataKind`'s doc comments, its enforcement in `FilteredKind::ALLIES` and a match in `net/src/sim_server/kind_filter.rs:278`. `const fn DataKind::audience() -> Audience` in `capabilities`, from which net derives `ALLIES`; the scope tuples stay, guarded by their test, as replicon needs the types.
- **Release.** `SessionTerms.release: String` is compared as text in runner and net and carried as text in two errors: a checked `EngineRelease` newtype in `protocol`, with `EngineRelease::CURRENT` and one comparison; `Release` is the module table of `capabilities`.
- **Log events.** The match's end is logged ad hoc with other fields on the server (`sim_server/mod.rs:663`) and the client (`sim_client/mod.rs:474`), and the client logs a death ad hoc (`sim_client/mod.rs:494`) though `UnitDied` exists: a `MatchEnded` event and `UnitDied` on both ends.

## Tests

`TestMatch` is the shared base; the duplication is in leaf helpers: about fifteen copies of `fn at(x, z) -> Position`, 22 of `NonZeroU32::new(30)`, 35 of `resource::<EntityIndex>().get(id).unwrap()`, three near-identical hero probes (`net/tests/scenario.rs:25`, `net/tests/prototype.rs:40`, `verifier/tests/headless.rs:99`), and one sequence of budgets, server, names and players in fourteen harness constructors. `Position::ground(x, z)` and `EntityIndex::entity_of(world, id)` in `sim`'s internals; a `UnitProbe` beside `MatchUnits`; `TestMatch::server_with(declared, players, kinds)`.

## Hot paths

- `NavigationEffect::put` (`navigation/navigation_effect.rs:212`) builds a `QueryState` to read one entity, and `make_room` (`navigation/mod.rs:191`) calls it once per overlapped walker after building its own query and a fresh `Vec`; `teleport` (`:238`) builds one and scans every projectile. `get_components_mut` in `put`, as `abilities/casts.rs:394` does; cached query states in a resource, as `CopyQueries` holds them; a scratch `Vec`.
- `Frame::begin` copies every player's resources and the id allocator, and readies every call part, for every script call: player resources read on the first write instead.

## Order

1. §2's stats at the spawn, within PLAN A6.
2. §5, §3.4 to §3.6, §1.4 and the rest as slices touch them.
