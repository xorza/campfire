# Architecture audit: recommendations

An audit of the workspace at `source/` after slice 3's A4: the entities and how each is spawned and replicated, how capabilities declare and register their parts, the crate and module graph, the call graph and argument counts, restore robustness, and code quality. Four read-only audits ran in parallel; each main claim was checked against the code. Line numbers are those of the working tree when the audit ran. The bugs it found are in the issue log; this file holds the structural recommendations.

## Findings

- **Registration.** Each module declares once what it adds; effect queues, deliverers and cast spends still register in install bodies, and systems order against each other's sets in each install.
- **Entities.** Two spawn paths give units different parts, the match's and the players' entities get their parts by hand, and net writes its prediction rule twice.
- **Ownership.** Almost every pass reaches through `&mut World` to `!Send` shared handles (`View`, `Ctx`, boxed hooks): 152 production functions take `&mut World` and 96 `&World`, against 13 `SystemParam` derives. No pass is testable without a whole `TestMatch`; `deliveries` and `players` have no tests, `actions` 6 for 53 files.
- **Arguments.** 59 `now: Tick` parameters sit beside worlds that hold the tick; 57 `ctx: &Ctx` and 57 `view: &View`; 24 systems group parameters in tuples to pass Bevy's limit of 16.
- **Restore.** A check runs with books that may be absent, and an error names the type alone (§4.1, §4.2).

The recommendations consolidate into five systems. Each is one declaration that every consumer reads, enforced by the compiler where it can be, else by one generic test or a clippy rule, never by memory.

| System | Replaces | Enforced by |
| --- | --- | --- |
| 1. One declaration per capability (§1) | the registrations left in install bodies, restated orderings | the compiler (visitor with no defaults), the declaration audit test |
| 2. One entity model (§2) | two spawn paths, parts put on by hand, net's prediction rule | required components, the compiler (`Host` with no default), a clippy rule on spawning |
| 3. Explicit dependencies (§3) | `!Send` reach-through, threaded `now`, tuple parameters, five match-world builders | typed params, a clippy rule on `non_send` |
| 4. Restore as a checked boundary (§4) | silent checks, wrong-type errors | the registry, a mutation fuzz test |
| 5. One session host (§5) | three server compositions, two replay loops, two restore glues | one type |

One policy holds across all five: **what can be forgotten is a type, a trait method with no default, or a disallowed method.** The project already bans `std::fs` and randomly seeded maps through `clippy.toml`; the same list bans the calls the five systems own, outside the one module that owns each.

## 1. One declaration per capability

Each module implements `Declares`, and `Release` visits the core's, each declared capability's and the mode's declarations with one consumer at a time: `Installer` (registry and world), `ApiBinder`, `StateOnly` (net's lists) and the declaration audit test. Installs take `InstallCx` and ask it for the declared capabilities and for scripts. Books name their owner, and `Books::install` refuses one its installed owner did not declare. What stays:

- **Registrations a declaration cannot name yet.** Effect queues, deliverers and cast spends register in install bodies: their function types name `actions`' and `abilities`' types, above `declare`'s layer. The declaration audit checks the queues from a match instead.
- **Audit rules not built.** A column's row parts are declared types; a capability's imports lie within its needs and the lower layers (with §3.6).
- **The map's sections.** The load check takes the owners of the mode's data sections from the script API's records, and names the map's three sections' owners by hand, as no data table records them.
- **Two lists of capabilities.** The install order is `CAPABILITIES` in `release`, and the needs `CapabilitySet::needs` in `declare`; a test holds every capability after what it needs.

### 1.4 Phases per stage, ordered once

`InputsPhase::{Begin, Refill, ModeInputs, Toggles, Orders, Holds, Checks}` and the like, chained once in the installer; a capability's `install` puts each system `.in_set(phase)`. The restated `.after` chains, and orders configuring other modules' sets, go. Cost: edges between phases that some pairs did not need, so some parallelism; ambiguity detection still checks order within a phase. Take this last.

## 2. One entity model

### Today

| Kind | Spawn path | Identity | Replication |
| --- | --- | --- | --- |
| Mode unit (hero, creep, tower, building, node, site, trained unit) | `ModeBook::spawn` via `Spawner`, `spawn_owned`, `Mode::start`, mode effects, `ActionsEffect`, trains, construction | `IdAllocator`, called by each caller, or the script's `Frame::take_id` | `show_units`; predicted by its owner |
| Projectile, area | `DeliverySpawner::spawn` | id the script took, else allocated | `show_units`; never predicted |
| Match | `MatchEntity::spawn` | `StableId::MATCH` | `show_match`, every room |
| Player | `MatchEntity::spawn_player_of` with `PlayerParts` | `StableId::of_player` | `show_match`; `Player` is its filter's owner |

- `UnitRows::row` tests a unit by `Position + Team`, where the other filters name `UnitEntity`.
- `ModeBook::spawn` gives `Facing`, `SpawnPoint`, `Level`, `Modifiers` and the type's derived tags; `DeliverySpawner` gives none of them, takes sight from another book, and consumes `NewUnitStates`, which `ModeBook::spawn` does not.
- Despawn happens in five places.
- Net writes the prediction rule twice (`sim_server/mod.rs:731`, `door.rs:218`) and sets it only for a new unit or a seat, though `Owner` changes.
- Test spawners build units no production path makes (`TestMatch::spawn`, three test `Spawner`s, `production/bench.rs:149`), which is how the delivery tags bug stayed unseen.

### 2.1 Kinds for net, parts with their start, typed filters

- `EntityKind: HostRule`, with `ROOMS` and `OWNER_PREDICTS`, for §2.5.
- Parts declared with their start, `d.part::<K, C>(start)`, so one declaration gives the spawn as well as the check, as `KitPart::ALL` does for a unit's kit; today `require` gives the check, and the spawns of the match's and the players' entities put their parts on by hand.
- Typed filter aliases, `UnitFilter`, `LivingUnit`, `BodyUnit`; today the filters name `With<UnitEntity>` and `Without<DeliveryEntity>` themselves.

### 2.4 One spawn per kind

- `Units::spawn(world, UnitSpawn { id: Option<StableId>, unit_type, team, pos, angle, owner }) -> StableId` allocates when `id` is `None`, inserts the core (`id`, `pos`, `unit_type`, `team`, owner, state from the books, consuming `NewUnitStates`) and the kind's parts; `ModeBook::spawn` adds the kit, `DeliverySpawner` the spec, and the test spawners call it. Sight comes from one book.
- Derived parts are left out of every spawn bundle, so the one derivation after a restore covers a spawn too: `units` derives `UnitTags` for every `Unit`, on a client as well, and `stats` derives `UnitStats` for `(With<Unit>, With<Level>)`. This part belongs to PLAN A6, which must give a client tags as well as stats.
- `MatchEntity::spawn` stays the match's and the players' one spawn, reading the declared parts.
- `clippy.toml` disallows `World::spawn`, `Commands::spawn` and `IdAllocator::allocate` in `campfire-capabilities` outside the kinds' spawn functions and the registry's restore, so a new spawn path cannot skip the model.

### 2.5 Net reads the kinds

Net registers one `show::<K>` system per declared kind, by `K::ROOMS`, and one `prediction_target` for `show_units` and the door, by `K::OWNER_PREDICTS`; an observer on `Owner` sets the target again as `KindFilter` already follows the owner. The duplicated rule, a stale target after an owner change, and a future kind (the team entity of design 14 D7) with no replication rule become impossible. Design 14 D10 names the list.

## 3. Explicit dependencies

### 3.1 Books and hooks are `Send` resources

- `View` owns book data (`UnitTypes` changes through `view.types_mut()`, `units/view.rs:312`), duplicates `TickRate` (`View.rate`), `SimTick`, `Bounds` and `Metric` (the rows' copies), and `ModeBook` is reachable only as `Rc<dyn Any>` in `Ctx` (`scripts/ctx.rs:25`), downcast by every reader; a client, with no `Ctx`, has no mode book.
- Recommendation: `UnitTypes` becomes a `UnitTypeBook` resource; `ModeBook` a resource holding an `Arc<ModeBook>` that `Ctx` clones for script calls; `View` keeps only the per-phase snapshot (`UnitRows`, `TargetIndex`, script names) and no copy of a resource. Cost: about forty call sites in `units`, `mode`, `stats`, `actions`.

### 3.2 `!Send` access lives in one place

The script host, `Ctx` and `View` are reached through one exclusive context, `ScriptPass { host, ctx, view, failures, budgets, clock }`, built once per pass; `ScriptBatch` holds `ctx` and `now`, so `hook_call(start, hook, unit, body)` loses two parameters, and the `remove_non_send`/`insert_non_send` pairs around passes (`combat/damage_pass.rs:97`, `combat/attacks.rs:150`, `scripts/script_batch.rs:33`) go. `clippy.toml` disallows `World::non_send`, `get_non_send`, `non_send_mut`, `remove_non_send` and `insert_non_send` outside `scripts`, so a pass that reaches for a `!Send` handle stops at the lint.

### 3.3 Typed params for what travels together

| Param | Members | Removes |
| --- | --- | --- |
| `TickClock` | `Res<SimTick>`, `Res<TickRate>`; `start()`, `end()`, `ticks(ms)`, `window(ms)` | the `(tick, rate)` tuples, about twenty `Res<SimTick>`, and `now` from about fifteen effect impls, as `Frame::apply` reads the tick once |
| `PoolQuery<F>` | `Option<Res<LifePool>>`, `Query<UnitPools, F>`; `get_mut(entity) -> PoolMetersMut` | the life pool from nine signatures |
| `ActionBooks` | action, param books, `ParamSources` | |
| `StatBooks` | stat, pool, modifier, tag books | with `ActionBooks`, `HeldPass::run` (15), `Refresh::run` (12) and `start_trains` (13) fall under eight parameters, with no tuple |
| `PlayerRows<C>`, `OnMatch<C>` | already written in A4 | |
| `OwnPlayer` (client) | `Option<Res<JoinState>>`, `PlayerUnits`; `slot()`, `team()`, `avatar()` | three HUD and view systems that re-read the own player |

Each param is a unit-test seam: a pass that takes `TickClock`, `PoolQuery` and `StatBooks` runs in a bare `World` holding those resources.

### 3.4 One match-world builder

The prepare, schedule, registry and install sequence appears five times (`Session::build`, `Session::snapshot_hash`, `SimClient::build`, `TestMatch::unstarted`, the match build bench). `MatchWorld::new(seed, rate).install(set, budgets).finish()` holds it, built on §1's visitors.

### 3.5 The log stays out of the core

`Session::run_tick` logs `ScriptCallFailed` (`runner/src/session/mod.rs:319`), while design 02 lets only the binaries, net and the verifier log. `run_tick` returns the failures, and its callers log them.

### 3.6 Layers without cycles

The layer test rejects imports from a higher layer only. Within a layer, modules import each other in both directions: `units ↔ scripts`, `units ↔ players`, `abilities ↔ items` (`items/mod.rs:6` imports `CastSpends`; four `abilities` files import `Inventory`). The needs table disagrees with the imports: production uses navigation and vision and needs neither; abilities uses items and navigation; orders uses production and items. The declaration audit adds both checks: no cycle within a layer, and imports within the capability's needs and the lower layers. `CastSpends` or `Inventory` moves down into `actions`, and `View` and its API users meet in one layer-1 module.

## 4. Restore as a checked boundary

### 4.1 Checks run with every book in place

`SimComponent::check` runs with the match's books in place, as its doc says and `Session::build` does. Today a missing book panics (`ModifierBook`, `ItemBook`, `ParamBook`), passes (`View` in six checks), or fails unless the value is empty (`action_slots.rs:554`, `slot_times.rs:327`, `experience.rs:98`, branches no install reaches). Every check reads its books with `resource` or `non_send`, and the dead branches go: a wiring mistake then crashes the test that makes it.

### 4.2 Errors name the entity and the rule

`SnapshotError::Invalid(&'static str)` names a type, and a foreign check names the type it is registered on: `Mode::kits_held`, on `UnitType`, fails for a missing kit part, and a `require` fails as the part it requires, not its holder. The errors become `Invalid { ty, id: Option<StableId> }`, `WrongHost { ty, id }`, `Missing { holder, needs, id }`.

### 4.3 Two more mutations

The mutation fuzz gains a nested option replaced with `None` where its type allows, as a slot's rank, and plays a hundred ticks, not five, on what restores, within the bound of 1 s for a test.

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
- **Release.** `SessionTerms.release: String` is compared as text in runner and net and carried as text in two errors: a checked `Release` newtype in `protocol`, with `Release::CURRENT` and one comparison.
- **Log events.** The match's end is logged ad hoc with other fields on the server (`sim_server/mod.rs:663`) and the client (`sim_client/mod.rs:474`), and the client logs a death ad hoc (`sim_client/mod.rs:494`) though `UnitDied` exists: a `MatchEnded` event and `UnitDied` on both ends.

## Tests

`TestMatch` is the shared base; the duplication is in leaf helpers: about fifteen copies of `fn at(x, z) -> Position`, 22 of `NonZeroU32::new(30)`, 35 of `resource::<EntityIndex>().get(id).unwrap()`, three near-identical hero probes (`net/tests/scenario.rs:25`, `net/tests/prototype.rs:40`, `verifier/tests/headless.rs:99`), and one sequence of budgets, server, names and players in fourteen harness constructors. `Position::ground(x, z)` and `EntityIndex::entity_of(world, id)` in `sim`'s internals; a `UnitProbe` beside `MatchUnits`; `TestMatch::server_with(declared, players, kinds)`. Test units spawn through §2.4's `Units::spawn`, which the clippy rule then requires.

## Hot paths

- `NavigationEffect::put` (`navigation/navigation_effect.rs:212`) builds a `QueryState` to read one entity, and `make_room` (`navigation/mod.rs:191`) calls it once per overlapped walker after building its own query and a fresh `Vec`; `teleport` (`:238`) builds one and scans every projectile. `get_components_mut` in `put`, as `abilities/casts.rs:394` does; cached query states in a resource, as `CopyQueries` holds them; a scratch `Vec`.
- `Frame::begin` copies every player's resources and the id allocator, and readies every call part, for every script call: player resources read on the first write instead.

## Order

1. §3.3 and §4.1, §4.2: the typed params, one book convention, errors that name the entity.
2. §3.1, §3.2: books out of `View` and `Ctx`, `!Send` access in `ScriptPass`, and its clippy rule.
3. §2.1, §2.4 within PLAN A6, and §2.5 before A3's spectators need a kind rule; §4.3.
4. §5, §3.4 to §3.6, §1 and the rest as slices touch them.
