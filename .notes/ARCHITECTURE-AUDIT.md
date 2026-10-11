# Architecture audit: recommendations

An audit of the workspace at `source/` after slice 3's A4: the entities and how each is spawned and replicated, how capabilities declare and register their parts, the crate and module graph, the call graph and argument counts, restore robustness, and code quality. Four read-only audits ran in parallel; each main claim below was checked against the code. Line numbers are those of the working tree when the audit ran.

The bugs the audit found are in the issue log; this file holds the structural recommendations.

## Summary

Two of fourteen registration channels are declared by a type and enforced by the compiler; the other twelve are calls in `install` bodies, held together by install order and runtime `expect`s. No entity has a declared kind: five subsystems each decide "is this a unit" by a different component test. Almost every pass reaches through `&mut World` to `!Send` shared handles (`View`, `Ctx`, boxed hooks), so no pass is testable without a whole `TestMatch`, and `now`, `ctx`, `view` and the life pool travel through many call levels. Restore checks look at one value on one entity; cross-entity invariants ("every unit of type T holds state class S", "every player entity holds its resources") are hand-written where they exist at all.

The recommendations, in the order that gives the most for the least:

| # | Recommendation | Makes impossible | Cost |
| --- | --- | --- | --- |
| 1 | One `CapabilityModule` trait and a type visitor; installs take one `InstallCx` (§2.1, §2.2) | an install that skips its state list, a capability missing from a dispatch, drifting install signatures, probes of other capabilities' resources | medium |
| 2 | Entity kind markers `Unit`, `Delivery` beside `MatchEntity`, `Player`, required by their state, and typed filters (§3.1) | a player or match entity taken for a unit, a typeless unit reaching net or the view | low |
| 3 | `SimComponent::Host` and `StateRegistry::require::<H, C>()` (§6.1) | a state type on the wrong entity, a holder missing a part a later system `expect`s | about a day |
| 4 | Move books and hooks out of `View` and `Ctx` into `Send` resources (§4.1) | the `Rc<dyn Any>` downcast of the mode book, restore checks that pass when a handle is missing | medium |
| 5 | `TickClock`, `PoolQuery`, book bundles and a `ScriptPass` param (§5) | `now` passed beside a world that holds it, tuples that dodge Bevy's 16 parameters | low to medium |
| 6 | One unit core and one spawn function (§3.2) | spawn paths that drift in which parts a unit gets | medium |
| 7 | Typed player parts, column-declared sources, owned books (§2.3 to §2.5) | a player part without its check, a column without its source, a silently dropped book | low each |
| 8 | An entity kind registry that net reads for rooms and prediction (§3.3) | a duplicated prediction rule, a stale prediction target after an owner change, a kind with no replication rule | medium |
| 9 | Single sources: `DataKind::audience()`, a `Release` newtype, owned API members (§7) | audiences, releases and section owners kept twice | low each |
| 10 | Shared test builders and probes (§8) | test units no production path makes | half a day |

## 1. Bugs found

Each is in `.notes/ISSUES.md`.

- **Stats.** `Refresh::give_parts` (`stats/refresh.rs:39`) gives `UnitTags::default()` to every `With<UnitType>, Without<UnitStats>` entity at each `SimEdge::After`. A projectile or an area spawns with its type's tags and no `UnitStats`, so its tags are emptied at the next edge, and `Refresh::run` derives none again, as its query needs `Level`, which a delivery lacks. The `projectile` and `area` tags, which filters read to leave deliveries out, and a delivery type's `hidden` and `detects`, are lost on a server with a stat book. §3.1 and §3.2 make the class impossible.
- **Orders.** `SeenTargets` (`orders/seen_targets.rs:20`) takes any entity with a `Team` as a unit: an ally's player entity has a `Team` and no `SeenBy`, so a Cast, Train or Gather order aimed at its named id passes the seen check (`orders/order_inputs.rs:131`). `DeliverySpawner.sources` (`&Team`) would take a player entity as a source. §3.1 makes it impossible.
- **Units.** A restored unit whose type declares fields of a `sync` class may lack that class's `UnitState<S>`: `UnitState::check` (`units/unit_state.rs:105`) checks a value that is present only. The next view read panics (`units/units_column.rs:107`), and a script write too (`units/unit_states.rs:92`).
- **Production.** `TrainQueue::check` (`production/train_queue.rs:106`) needs only a type and a capacity, but `finish_trains` `expect`s a team and a position on every producer (`production/trains.rs:228`, `:292`, `:317`). A gather order may name a slot of no rank, as `ActionSlots::check` and `Gatherer::check` allow, and `GatherView::gather` (`production/gather_loop/gather_view.rs:94`) `expect`s a rank every tick.
- **Combat.** A restored `Respawn` needs nothing beside it, while the respawn system (`combat/dying.rs:75`) needs `Dead` and a `SpawnPoint`: a unit stays dead.

## 2. Declaring and registering a capability

### Channels today

| Channel | Declared | Enforced by |
| --- | --- | --- |
| Install fn | `CAPABILITIES` row; core hard-coded; `Mode::install` in runner, 5 args | compiler (table length), order test |
| Needs | `row.needs` | `CapabilitySet::new`, tests |
| State types | `fn state_types` per module, called by each install **and** by the match in `CapabilitySet::state_types` | compiler (`Replication`), registry test without Mode, hand-kept `STATE` table |
| Restore checks | `registry.add_check` in 3 installs | nothing checks completeness |
| View columns | `add_column` + `add_source` pair, ×8 | runtime `expect` (`units/unit_rows.rs:281`) |
| Call parts | `ctx.frame().add_part` | readers `expect` |
| Player parts | `PlayerParts::add` closures | a hand-written check beside each |
| Effect queues | `EffectQueues::register` | runtime `expect` at first use |
| Deliverers, cast spends | `register_*` | runtime asserts |
| Script API | three lists: core in `ScriptApi::bind`, `apis()`, rows' `registering` | naming test |
| Member gating | `.capability(..)` on each member | nothing |
| Data section owner | hand map in `package/src/load_check.rs:1279` | nothing |
| Books | `Books::install`, `ModeBooks::install` | nothing: `Books::replace` (`books/mod.rs:53`) drops a book whose resource is absent |
| Ordering | a `*Set` per module, `.after` chains restated across modules | Bevy ambiguity detection for combinations a test builds |

Evidence of drift: `Abilities` sits in the "no list" arm of the state dispatch (`capability_set/mod.rs:246`) against the doc claim above it; `Abilities::install` takes an unused registry and `Progression::install` an unused schedule; orders and production probe `BuildSpecs`, `ItemBook` and `BodyIndex` to learn whether another capability is installed, which only table order makes true; `Deliveries` is a pseudo-capability installed idempotently by three others.

### 2.1 One trait per capability, installed by a visitor

```rust
pub(crate) trait CapabilityModule {
    const CAPABILITY: Capability;
    const NEEDS: &'static [Capability];
    fn state_types<T: StateTypes>(types: &mut T);
    fn register_api(api: &mut ApiBuilder<'_>);
    fn install(cx: &mut InstallCx<'_>);
}

trait ModuleVisitor {
    fn visit<M: CapabilityModule>(&mut self);
}
```

`CapabilitySet::each(self, visitor)` visits the core first (`Units`, `Actions`, `Deliveries`), then each declared capability in table order. Installing, listing state types and binding the API become three visitors; the installer calls `M::state_types(registry)` itself, so no install takes `&mut StateRegistry`. `Mode` implements the trait too, so the registry test covers its list. No defaults: a capability with no state writes an empty list.

Makes impossible: an install that forgets its list, a capability in the table and missing from a dispatch, the API side-lists, drifting signatures. Cost: about fifteen modules move code; monomorphized, no runtime cost.

### 2.2 `InstallCx` carries what installs probe for

`InstallCx { world, schedule, set: CapabilitySet, scripts: Option<&ScriptBudgets> }`. `cx.set.contains(Capability::Production)` replaces the probes at `orders/mod.rs:102`, `:111`, `:120`, `production/mod.rs:107`, `abilities/mod.rs:89`, `actions/mod.rs:108`, `stats/mod.rs:129` and `combat/mod.rs:117`. Table order then matters for `needs` alone, and a test installs one capability with a made-up set.

### 2.3 Checks and player parts are declared in the state list

`StateTypes` gains `fn check<C: SimComponent>(&mut self, check: fn(&World) -> bool) {}` and `fn player_part<C: Replication + Default>(&mut self) {}`. The registry's impl adds the check; the installer collects player parts into `PlayerParts` and registers one generic check that every player entity holds every listed part. `player_modifiers_held`, the player half of `Mode::entities_held` and the closures go. A part that needs data, `PlayerResources::new(count)`, takes it from the books at spawn. Makes impossible: a player part with no restore check.

### 2.4 A view column declares its source

`ViewColumn` gains `type Parts: RowParts` and `fn fill(parts: ROQueryItem<'_, '_, Self::Parts>, fill: &mut RowFill<'_, Self>)`; `View::add_column::<C>(world)` registers both. The paired calls, the runtime `expect` and eight copies of three lines go.

### 2.5 A book knows its owner and its copies

```rust
pub(crate) trait Book: Resource {
    const OWNER: Option<Capability>;
    fn share(&self, view: &View, frame: Option<&mut Frame>) {}
}
```

`Books::install(self, world, set)` inserts by `set.contains(OWNER)`, asserts the default resource the install put there, and calls `share` in the same step; the test loaders reuse it. A book in the world but not in its column, a dropped book, and the unconditional `params`, `tracks` and `supply_rules` inserts become impossible. The server-only books (`EffectLists`, `ByType<Ai>`) say so in their impl, not by the presence of `Ctx`.

### 2.6 API members are owned by their capability

The visitor passes `api.owned_by(M::CAPABILITY)`, which sets each member's capability and each data field's owner. About 25 manual `.capability(..)` tags go, and `load_check`'s section map is derived from `ScriptApi::data()`. Cost: a change across `capabilities` and `package`. Makes impossible: an ungated member, and two owners of one section.

### 2.7 Effect queues as trait data

`const QUEUE: Option<QueueListed>` on the trait, filled by the installer, and a test that every `CapabilityDoes` variant's owner has one (an `ALL` list). Deliverers and cast spends follow.

### 2.8 Phase sets per stage

`InputsPhase::{Begin, Refill, ModeInputs, Toggles, Orders, Holds, Checks}` and the like, chained once in `CapabilitySet::install`; each capability puts its systems `.in_set(phase)`. The restated `.after(StatsSet::Regenerate).after(CombatSet::Respawn)` (orders, abilities, mode, production) and orders configuring four other modules' sets go. Cost: edges between phases that some pairs did not need, and so some parallelism; ambiguity detection still checks order within a phase. Take this last.

## 3. Entities

### Kinds today

| Kind | Spawn path | Identity | Replication |
| --- | --- | --- | --- |
| Mode unit (hero, creep, tower, building, node, site, trained unit) | `ModeBook::spawn` via `Spawner`, `spawn_owned`, `Mode::start`, mode effects, `ActionsEffect`, trains, construction | `IdAllocator`, called by each caller, or the script's `Frame::take_id` | `show_units`; predicted by its owner unless a delivery |
| Projectile, area | `DeliverySpawner::spawn` | id the script took, else allocated | `show_units`; never predicted |
| Match | `MatchEntity::spawn` | `StableId::MATCH` | `show_match`, every room |
| Player | `MatchEntity::spawn_player_of` with `PlayerParts` | `StableId::of_player` | `show_match`; `Player` is its filter's owner |

Five tests of "unit": `With<UnitType>` (`give_parts`), `With<Team>, Without<Player>` (`show_units`), `StableId + Position + Team` less deliveries (client `NewUnits`), `Position + Team` (`UnitRows::row`), `&Team` alone (`SeenTargets`). Two spawn paths share a core and differ: `ModeBook::spawn` gives `Facing`, `SpawnPoint`, `Level`, `Modifiers`; `DeliverySpawner` gives none of them, takes sight from another book, and consumes `NewUnitStates`, which `ModeBook::spawn` does not. Despawn happens in five places. Test spawners build units no production path makes (`TestMatch::spawn`, three test `Spawner`s, `production/bench.rs:149`), which is how the delivery tags bug stayed unseen.

### 3.1 Kind markers and typed filters

```rust
#[derive(Component, Debug)] #[component(immutable)] pub struct Unit;
#[derive(Component, Debug)] #[component(immutable)] pub struct Delivery;
```

`#[require(Unit)]` on `UnitType`, `#[require(Delivery)]` on `Projectile` and `Area`; required components come back on restore and on clients by themselves, as their holders are state. Listed with `types.derived::<..>()`. Aliases in `units`: `UnitFilter = With<Unit>`, `LivingUnit = (With<Unit>, Without<Dead>)`, `BodyUnit = (With<Unit>, Without<Delivery>)`, which replace all five tests. The proving match checks that every entity with a `StableId` holds exactly one of `Unit`, `MatchEntity`, `Player` (a delivery holds `Unit` and `Delivery`). Makes impossible: a non-unit taken for a unit, and a new entity kind nobody gave a kind. Cost: two components, about ten queries, and a `UnitType` on every test unit.

### 3.2 One unit core, one spawn function

A `UnitCore { id, pos, unit_type, team, tags }` bundle with one constructor that takes the tag book and the state book and consumes `NewUnitStates`; `Units::spawn(world, UnitSpawn { id: Option<StableId>, unit_type, team, pos, angle, owner })` allocates when `id` is `None`. `ModeBook::spawn` is core plus kit, `DeliverySpawner` core plus spec, and the test spawners call the core. Sight comes from one book. Derived parts are left out of the spawn bundle, so the one derivation after a restore also covers a spawn: `units` derives `UnitTags` for every `Unit` from type, modifiers and `StatusTags`, on a client too, and `stats` derives `UnitStats` for `(With<Unit>, With<Level>)`. This last part overlaps PLAN A6, which must give a client tags as well as stats.

### 3.3 An entity kind registry for net

```rust
pub trait EntityKind: Component {
    const ROOMS: Rooming; // Vision or EverySeat
    const OWNER_PREDICTS: bool;
}
```

Implemented by `Unit` (vision, predicted), `Delivery` (vision, not predicted), `MatchEntity` and `Player` (every seat, not predicted); `CapabilitySet::entity_kinds` lists them, and net registers one `show::<K>` system per kind, a delivery taking precedence over a unit. One `prediction_target` serves `show_units` and the door (today `sim_server/mod.rs:731` and `door.rs:218`), and an observer on `Owner` inserts the prediction target again, as `KindFilter` follows the owner today; a change of owner leaves a stale target now. Design 14 D10 gains a line naming the list.

## 4. Ownership graph

### Crates

The graph follows design 02's map with these exceptions:

- **runner → log.** `Session::run_tick` logs `ScriptCallFailed` (`runner/src/session/mod.rs:319`), while design 02 lets only the binaries, net and the verifier log. `run_tick` should return the failures for its caller to log.
- **server → capabilities, server → sim, client → runner**, each for one pass-through or one check. Each goes once `SimServer::app` takes the packages (§5.6) and the client's rate check moves into `SimClient`'s setup.
- **The server crate is a binary of 933 lines**, while design 02 gives it host config, lifecycle, saves and the local server; those live in `net::sim_server` and `net::local`. The design or the split must change: an item under Decide in the issue log.

### Owners inside `capabilities`

The world, plus four `!Send` handles: `View(Rc<ViewParts>)` with `RefCell`s, `Ctx { Rc<RefCell<Frame>>, View, Rc<OnceCell<Rc<dyn Any>>> }`, `ScriptHost`, and boxed closures (`CombatEvents`, `DamageWeigher`, `HealWeigher`, `Spawner`).

- `View` owns book data: `UnitTypes` changes through `view.types_mut()` (`units/view.rs:312`).
- Facts with two owners: `View.rate` and the `TickRate` resource; the rows' `now`, `bounds` and `metric` and the `SimTick`, `Bounds` and `Metric` resources.
- `ModeBook` is reachable only as `Rc<dyn Any>` in `Ctx` (`scripts/ctx.rs:25`), downcast by every reader; a client, with no `Ctx`, has no mode book, so `Mode::team_of` is `None` there.
- Restore checks pass when a handle is missing: `get_non_send::<View>().is_none_or(..)` in `Team`, `Owner`, `UnitType`, `Player`, `MatchEnd`, `Relations`; `Mode::entities_held` passes with no mode book.

### 4.1 Books and hooks as `Send` resources

`UnitTypes` becomes a `UnitTypeBook` resource, `ModeBook` a resource holding an `Arc<ModeBook>` that `Ctx` clones for script calls, and `View` keeps only the per-phase snapshot (`UnitRows`, `TargetIndex`, script names) and no rate. The downcasts go, systems name what they read, and the restore checks read resources that must exist (§6.2). Cost: about forty call sites in `units`, `mode`, `stats` and `actions`.

### 4.2 Module layering

The layer test rejects imports from a higher layer only; within a layer, modules import each other freely, and some do in both directions: `units ↔ scripts`, `units ↔ players`, `abilities ↔ items` (`items/mod.rs:6` imports `CastSpends`; four `abilities` files import `Inventory`). The `CAPABILITIES` needs table disagrees with the imports: production uses navigation and vision types and needs none; abilities uses items and navigation and needs only combat; orders uses production and items. Recommendations: the layer test forbids cycles within a layer, and checks that each capability's imports are within its `needs` and the lower layers; `CastSpends` or `Inventory` moves down into `actions`; `View` and its API users meet in one layer-1 module.

## 5. Call graph and argument counts

152 production functions take `&mut World` and 96 `&World`, against 13 `SystemParam` derives. About sixty `world.non_send::<View | Ctx>().clone()` calls and `remove_non_send`/`insert_non_send` pairs around passes (`combat/damage_pass.rs:97`, `combat/attacks.rs:150`, `scripts/script_batch.rs:33`) hide every dependency from the signature. 24 systems group parameters in tuples to pass Bevy's limit of 16: `HeldPass::run` 15, `Trains::start_trains` 13, `Refresh::run` 12, then `apply_production_orders`, `force_units`, `strike`, `fly`, `check_player_orders`, `progress_sites`.

### 5.1 `TickClock`

`#[derive(SystemParam)] struct TickClock { tick: Res<SimTick>, rate: Res<TickRate> }` with `start()`, `end()`, `ticks(ms)`, `window(ms)`. It replaces the `(tick, rate)` tuples and about twenty `Res<SimTick>`. `Effect::apply(self, world, frame)` reads the tick once in `Frame::apply`, which drops `now` from about fifteen effect impls (59 `now: Tick` parameters today, beside worlds that hold it).

### 5.2 `PoolQuery`

`PoolQuery<F> { life: Option<Res<LifePool>>, units: Query<UnitPools, F> }` with `get_mut(entity) -> PoolMetersMut`; the life pool leaves nine signatures.

### 5.3 Book bundles

`ActionBooks { actions, params, sources }` and `StatBooks { stats, pools, modifiers, tags }` bring `HeldPass::run`, `Refresh::run` and `start_trains` under eight parameters with no tuples.

### 5.4 `ScriptPass`

An exclusive pass context `{ host, ctx, view, failures, budgets, clock }`, with `ScriptBatch` holding `ctx` and `now`: `hook_call(start, hook, unit, body)` loses two parameters, and the remove-and-insert dance in `ScriptBatch::run` goes.

### 5.5 `MatchWorld` builder

The prepare, schedule, registry and install sequence appears five times (`Session::build`, `Session::snapshot_hash`, `SimClient::build`, `TestMatch::unstarted`, the match build bench). `MatchWorld::new(seed, rate).install(set, budgets).finish()` collapses them, and with §2.1 an install takes one `InstallCx`.

### 5.6 Server orchestration in net

`SimServer::app(tick, &ModePackages) -> App` for the three server compositions (`server/src/main.rs:234`, `net/src/local/local_server/mod.rs:327`, `net/src/harness/in_process_match/mod.rs:332`); one pending restore and system for the identical `Restore` (server) and `PendingRestore` (net); `Session::replay_to(world, tick, on_boundary)` for the two replay loops (`sim_server/mod.rs:351`, `session_dir/mod.rs:232`).

### 5.7 Smaller ones

- `send_orders` (10 parameters, two jobs) splits into `feed_bot_script` and `stamp_orders`.
- The client's HUD and view re-read the own player from `Option<Res<JoinState>>` and `PlayerUnits` in three systems: an `OwnPlayer` param with `slot()`, `team()`, `avatar()`.
- `Session` is a resource inside the world it runs, so its methods take `&self` and that world, and `Runner` is twenty forwarders. Either `Session` splits into a `SessionRecord` outside the world and match state inside it, or the design says why it is one.

## 6. Restore robustness

### 6.1 Declared hosts and required parts

- `SimComponent` gains `type Host: HostRule`, with `trait HostRule { fn holds(world: &World, entity: Entity) -> bool; }`: `OnAny` in `sim`, `OnMatch`, `OnPlayer` and `OnUnit` in `capabilities`. `check_component` runs it before the value's check. No default, so each of about sixty types chooses. The hand-written host tests in `PlayerModifiers`, `PlayerResources`, `Relations`, `MatchEnd`, `ModeState` and `MatchEntity` go, and the `TrainQueue` bug cannot exist.
- `StateRegistry::require::<H, C>()`: every holder of `H` holds `C`. It replaces `relations_held`, `player_modifiers_held` and most of `entities_held`, the hand-written pairs `ActionSlots`/`SlotTimes`, `Projectile`/`FlightState`, `AreaTrigger`/`Area`, `Points`/`Experience`, and covers `Respawn` ⇒ `Dead` + `SpawnPoint`.
- `add_check` stays for rules that are truly custom: `within_bounds`, and "a unit holds each state class its type declares".

### 6.2 One convention for an absent book

`SimComponent::check` runs with the match's books in place, as its doc says and `Session::build` does. Today a missing book panics (`ModifierBook`, `ItemBook`, `ParamBook`), passes (`View` in six checks), or fails unless the value is empty (`action_slots.rs:554`, `slot_times.rs:327`, `experience.rs:98`, branches no install can reach). Recommendation: read every book with `resource` or `non_send`, and delete the dead branches.

### 6.3 Errors name the entity

`SnapshotError::Invalid(&'static str)` names a type, and a foreign check names the type it is registered on: `Mode::entities_held`, on `Player`, fails for a missing mode state. `Invalid { ty, id: Option<StableId> }`, and with §6.1 `Missing { holder, needs, id }`.

### 6.4 Load, then trust

51 builder sites `expect("the load checked …")` what `load_check.rs` (2,275 lines) checked; the two copies can drift. Builders that take checked types would remove them: a larger change for the issue log.

## 7. Single sources of truth

- **Audiences.** A kind's audience is in `DataKind`'s doc comments, and its enforcement in `FilteredKind::ALLIES` and a match in `net/src/sim_server/kind_filter.rs:278`. `const fn DataKind::audience() -> Audience` in `capabilities`, from which net derives `ALLIES`. The scope tuples stay, guarded by their test, as replicon needs the types.
- **Release.** `SessionTerms.release: String` is compared as text in runner and net, and carried as text in two errors. A checked `Release` newtype in `protocol` with `Release::CURRENT` and one comparison.
- **Section owners** (§2.6) and **effect queues** (§2.7).

## 8. Test infrastructure

`TestMatch` is the shared base, and the duplication is in leaf helpers: about fifteen copies of `fn at(x, z) -> Position`, 22 of `NonZeroU32::new(30)`, 35 of `resource::<EntityIndex>().get(id).unwrap()` in integration tests, three near-identical hero probes (`net/tests/scenario.rs:25`, `net/tests/prototype.rs:40`, `verifier/tests/headless.rs:99`), and the same budgets, server, names and players sequence in fourteen harness constructors. Recommendations: `Position::ground(x, z)` and `EntityIndex::entity_of(world, id)` in `sim`'s internals; a `UnitProbe` beside `MatchUnits`; `TestMatch::server_with(declared, players, kinds)`; test units spawned through §3.2's core.

## 9. Hot paths

The tick systems use scratch buffers and `clone_from`; the exceptions are effects that build a `QueryState` per call:

- `NavigationEffect::put` (`navigation/navigation_effect.rs:212`) builds a query to read one entity, and `make_room` (`navigation/mod.rs:191`) calls it once per overlapped walker after building its own query and a fresh `Vec`; `teleport` (`:238`) builds one and scans every projectile.
- `Frame::begin` copies every player's resources and the id allocator, and readies every call part, for every script call, whether or not the call reads them.

Recommendations: `get_components_mut` in `put`, as `abilities/casts.rs:394` does; cached query states in a resource, as `CopyQueries` holds them, with a scratch `Vec`; and a frame that reads player resources on the first write.

## 10. Logging

Net logs through 31 typed `LogEvent`s, and logs some facts ad hoc beside them: the match's end, with other fields on the server (`sim_server/mod.rs:663`) and the client (`sim_client/mod.rs:474`), and a unit's death on the client (`sim_client/mod.rs:494`) though `UnitDied` exists. A `MatchEnded` event and `UnitDied` on both ends.

## Suggested order

1. §3.1 kind markers, with the delivery tags and seen-targets fixes.
2. §2.1, §2.2 and §2.3 together: one declaration per capability, no registry in installs, checks and player parts in the list.
3. §6.1 and §6.2: declared hosts, required parts, one book convention; the restore bugs of §1 go with them.
4. §5.1 to §5.3: the low-cost params.
5. §4.1, then §3.2 within PLAN A6, then §3.3 before A3's spectators need a kind rule.
6. §2.4 to §2.8, §5.4 to §5.7, §7 to §10 as slices touch them.
