# Plan

The one plan of the project: every open step, in the order it is done, in batches. A batch is one commit: it holds the steps that change the same systems, refactorings, fixes and features alike, and ends with `./drive.py check` passing, and a stop for review. Each batch says what it touches and why it stands where it does. Open items only: a step goes when it is done. The issue log, `.notes/ISSUES.md`, holds every open problem; a problem that a step here takes carries the step's tag there ([Issue log](AGENTS.md#issue-log)). The designs in `docs/design/` stay the source of truth: a step names what the code lacks against its design, or the design it proposes, which moves into `docs/design/` when it is agreed.

One policy holds across the steps: **what can be forgotten is a type or a trait method with no default.** A rule that no type can hold is held by a type that makes the right way the short one, a test where one can see the breach, and review. Line numbers point to the code as each step was written; a step refreshes its references as it starts. A step that states no check holds this one: no golden moves, and `./drive.py check` passes.

## Direction

Every batch moves the engine toward one shape, and each item names the part it builds.

- **A module is one entry of one list.** Each module, the core's `units`, `players`, `actions` and `deliveries`, each capability and the mode, implements `Module`: its capability, its needs and its uses, `declare` and `install`, none with a default (F44 adds the uses). `install` holds only its systems, each in one of its module's declared sets; all else a module adds is a declaration.
- **What a module adds is declared once,** and one walk of the declarations serves the installer, the script API's binder, the network's lists of state types and the declaration audit:

| What | Declaration | Item |
| --- | --- | --- |
| A state type, a state resource | `d.component`, `d.resource` | today |
| A resource that is not state: clear between ticks, built again, or a tick's report for the host | `d.scratch`, `d.cache`, `d.report` | today |
| A registry of callbacks a lower module calls, and each callback in it | `d.calls`, `d.callback` | today |
| A system set's phase in the tick's one list, `Phase::ALL`, or its gap | `d.sets` | today |
| A part every entity of a kind starts with | `d.part` | today |
| Engine tags | `d.engine_tags` | today |
| Package data a match holds | `d.book` | today |
| A column of the script view, a part of a script call | `d.column`, `d.call_part` | today, F37 |
| Script members, hooks and data tables | `d.api` | today |
| An observer, a system a spawn runs out of the schedule, and a derived part another part requires | `d.observer`, `d.one_shot`, `d.requires` | F49 |

- **A kind is a marker,** one on each entity, and an entity spawns whole: each part it carries is declared on its kind and comes with its spawn, so a new entity moves between archetypes once. A part derived from another part comes by Bevy's required components or that part's hook; a part a system derives each tick is a declared part the system writes in place, never one it inserts by command. A marker of a state that an event starts and ends, as `Dead`, `Respawn` or `OutOfPlay`, comes and goes by command; F52 measures it in a sparse set, where it moves no table.
- **A name is resolved once,** at the load: one table of names for each id, `Names`, a builder that is the check of what it resolves, a script argument resolved by its type, `ScriptArg`.
- **One word, one meaning:** module, owner, kind, part, callback (`Hook` is the scripts' alone), `*File`, `*Data`, `*Spec`, `*Book`, which the release tests hold.
- **A pass visits what changed:** no query state, structural change or allocation per item on a hot path (I1), and a fact fixed at install or at the load is a fact of the schedule (F39).
- **Each crate depends only on lower layers,** which `LAYERS` holds: no two crates of one layer depend on each other, and a crate depends on nothing that only its gated code names. No crate's top modules import each other in a cycle, which a rule test holds; the four modules that `campfire-core` holds still import each other, until F50 and F53 split it. What is the sim's lives in `sim`, and `math` holds the exact numbers every crate shares (F63, F64). A capability asks another for its work through a contract below both, never by writing its state (F59).
- **Bevy's own mechanisms, not hand-made ones:** what install or the load fixes is a run condition or an `If` parameter (F39); observers, one-shot systems and required components are declarations (F49); a marker that comes and goes is measured in a sparse set, and the units' order by stable id in one shared order (F52); a composition of an `App` is a plugin group (H2).

So a new capability is one entry of the list; a new state type its type and one `d.component`; a new section of a unit type a `DataSection`, its owner's data record and a `d.part`; a new effect an `Effect` and a `d.callback`. The declaration audit and the release tests fail on each piece left out.

## 1. Math's layout

`math`, `sim`, `geometry`; design 09. What is the sim's leaves `math`, and the exact ratio types of `geometry` become one type of `math`. F64 changes the comparison of hot geometric queries, so it measures them before it lands, and the batch runs in a session where the user asks for the benches.

### F63. The tick's bookkeeping of the random sequences is the sim's

(`math`, `sim`; **Math**.) `math`'s `RngSource` keeps the running tick, the count of the ticks started and the debug check for a sequence opened twice in one tick (`math/src/rng/rng_source.rs:9-40`), and `sim::SimRng` is a one-field wrapper of it (`sim/src/sim_rng.rs:12`), so the sim's lifecycle lives in `math`. `math` keeps `Rng`, its streams and an open of a sequence by its stream, entity and tick, which holds no state, as design 09 keys each sequence; `SimRng` keeps the tick, its starts and the check. Check: `math` names no tick lifecycle; the goldens do not move.

### F64. One exact ratio type in `math`

(`math`, `geometry`; **Math**.) `geometry` holds two exact ratio types of one kind, each compared by its cross products: `Fraction`, an `i128` over an `i128`, a share of a straight path (`geometry/src/fraction.rs:8`), and `SquaredDistance`, a `U256` over a `u128`, a squared distance to a box's edge or a path (`geometry/src/squared_distance.rs:9`). One `Ratio` in `math`, generic over the integers of its numerator and denominator, as `num-rational`'s `Ratio<T>` is, takes their place: it compares exactly by cross products at a width that holds them, an `i256` for today's `Fraction` and a `U384` for `SquaredDistance`, which `math` gains with it. Design 09 gains the type. The step measures the collider's and the boxes' cases, `integration/collision/` and `atomic/` of `geometry`, before and after, as each comparison gets wider. Check: the goldens do not move; `proptest` differential tests against an exact reference, as design 09 asks of every operation.

## 2. The capabilities' graph

`capabilities`: `navigation`, `campfire-core`'s `units`, `abilities`, `production`, `orders`, the release tests; design 02, design 04's navigation and overview. F59 moves the walk's contract into the core, before the batch that splits `campfire-core` (the contract moves with `units`) and before U1 adds a fourth capability that walks a unit; then F44 ties each capability crate's dependencies to its needs and uses, on the graph F59 leaves.

### F59. A walk is asked through a contract of the core

(`capabilities`: `navigation`, `units`, `abilities`, `production`, `orders`; **Navigation**.) Every capability that walks a unit writes navigation's own state: a cast that walks into range or stops (`abilities/src/casts.rs:199`), a train's rally (`production/src/trains.rs:229`), a build and a gather that give up a walk the route ends short of (`production/src/construction/build_view.rs:262`, `gather_loop/place.rs:31`), and the move, stop, chase and path orders (`orders/src/unit_order.rs:106`, `163`; `destinations.rs:46-93`). Each sets `Destination` and reads `Route::arrived_short_of` through `Destination::walk_to` and `Destination::gives_up`, so each depends on `campfire-navigation`; `abilities` takes nothing else of it. 0 A.D.'s components ask a walk of `UnitMotion` through its interface, never its state. The walk's goal and what a walker reports of it, that it arrived short, become a part of the core's `units`, which a capability writes and navigation pursues and answers; `Destination`'s check, which reads navigation's pathing grid (`navigation/src/destination.rs`, `check`), and its prediction stay navigation's. The step writes the contract into design 02 and design 04's [Navigation](docs/design/04-capabilities/navigation.md) for review before the code moves. Check: `abilities` does not depend on `campfire-navigation`, and goes down a layer; `production` and `orders` name no navigation type to walk a unit; no golden moves.

### F44. The capability graph in one table

(`capabilities`, `docs/design/04-capabilities/00-overview.md`.) The graph's needs and the install order are one list of modules, `Release::modules`; the overview points to design 02's table of crates and their layers. Each capability is a crate, whose `Cargo.toml` lists the crates it depends on: Cargo refuses a cycle, and a release test refuses a dependency on the same layer or a higher one; no test ties those dependencies to `Module::NEEDS`.

A capability's edge to another is a need, without which it does not run, or a use, which it reads when the match has it, each read guarded by `cx.has`: production runs without navigation, with no gathering. The table holds both, and the test holds it equal to the manifests: each capability crate's dependencies on other capabilities are exactly its needs and uses. No module names another's sets, so the schedule adds no edge.

```rust
// core/src/declare/module.rs: beside `CAPABILITY` and `NEEDS`
/// The capabilities it reads when the match has them, and runs without.
const USES: &'static [Capability];

// production/src/lib.rs
const USES: &'static [Capability] = &[Capability::Navigation, Capability::Vision];

// capabilities/src/release/tests/mod.rs
#[test]
fn a_capabilitys_crate_depends_on_exactly_what_it_needs_or_uses() {
    for capability in Release::capabilities().iter() {
        // The `campfire-<capability>` dependencies of its crate's manifest, as the layer test reads them.
        let depends: Vec<Capability> = Manifest::of(capability).capabilities();
        let edges = Edges::of(capability); // a `ModuleVisitor` that collects `M::NEEDS` and `M::USES`
        assert_eq!(depends, edges, "{capability:?}");
    }
}
```

The overview's prose points to the table. Check: the test fails on today's `vision` row, which needs combat it does not import.

## 3. The script call's runtime and the script view

`campfire-core`'s `scripts`, `declare`, `units` and `players`; `mode`'s state, `stats`' call, in their crates. The script view becomes the script runtime's own data, which the modules fill, and with it the knot of `campfire-core` comes apart and `campfire-core` splits into four crates; then what a script call costs and how its parts are kept. The design of the view, F50 and F53 together, goes into design 02 and design 03 for review before the code moves. I1 and F38's boxing are measured, so the batch runs in a session where the user asks for the benches.

### F50. One direction between the modules of `campfire-core`

(`capabilities`: `values`, `scripts`, `units`, `players`; **Core**.) The four modules of `campfire-core` import each other in three pairs: `units ↔ scripts` (32 files of `units` import `scripts`; `scripts` imports `View` and `Unit` in four files, and the plain types `ActionId`, `ModifierId`, `Team`, `Block`, `TagProperty`, `TagData`, `CollisionData` and `DashDelivery`), `units ↔ declare` and `scripts ↔ declare` (F53 takes the two of `declare`); and `scripts` imports `players` (`core/src/scripts/frame.rs:7`), against the order below. They share `campfire-core`, so Cargo checks no import between them. The order this step and F53 give them, each importing only those before it, is `scripts`, `declare`, `units`, `players`, with `values` below all four; `campfire-core` then splits into a crate for each, and `scripts` joins `campfire-script`, which then holds every use of Rhai:

- The plain types and ids, and the `NameKind` trait they implement, go to `values`, so no module of the four takes them from another.
- `scripts` imports nothing of `units`: the script view becomes the runtime's own data, rows of plain values that `scripts` defines, with the columns' contracts `ViewColumn`, `RowFill` and `RowReads`, the names and `Book`, which shares a book into the view and the frame (`core/src/declare/book.rs:23`); the fill that reads the units stays in `units`. Today `View` reaches, through its rows, columns, names, filter and target index, 64 of `units`' 86 files, so a move of the view as it is would move the unit model: the view's rows hold values, its names come as tables the load fills, and the unit model stays in `units`. This is I1's view, designed once. The frame's copy of the players' resources (`core/src/scripts/frame.rs:7`) takes them through a contract of `scripts` that `players` fills.

Check: `scripts` imports nothing of `units` or `players`.

### F53. `declare` holds only the contracts a module implements

(`capabilities`: `declare`, `units`, `scripts`; **Core**.) Every module imports `declare`, and `declare` imports `units` for `EntityKind` (`core/src/declare/kind_visitor.rs:1`), `EngineTag` (`declarations.rs:16`) and the context a part is made from, which reads its unit's type, its facing and the books by type (`kind_spawn.rs:9-11`), and imports `scripts` for `ApiBuilder`, `CallPart` and, after F50, the view's contracts and `Book`. `EntityKind`, a contract each kind implements, moves into `declare`, and `EngineTag` to `values`. The context of a part reads the unit model, so it moves to `units`, and `d.part` takes it through a trait of `declare` that `units` implements, as a declaration takes a contract and never the type that fills it; the step writes this into design 02's [Structural rules](docs/design/02-engine-core.md#structural-rules) for review before the code moves. Check: `campfire-core` splits into `campfire-script`, `campfire-declare`, `campfire-units` and `campfire-players`, each on a layer of its own in `LAYERS`, and leaves the exceptions of the cycle test, depending only on those before it, and the workspace builds; no manifest but `campfire-script`'s lists `rhai`, which `campfire-core` and the top crate's tests list until then, so Cargo refuses `rhai::` in any other crate.


### F36. One schema of script state

(`capabilities`: `mode`, `units`, `stats`.) Mode, unit and modifier state each look a field up their own way: `NameTable` and places kept for the mode (`mode/src/mode_schema.rs:20-66`); `UnitStateBook::field`, which counts the field's place on every access (`core/src/units/unit_state_book.rs:53-63`); a binary search of its own for a modifier (`stats/src/modifier_handle.rs:187`). Two `StateField` types differ (`mode_schema.rs:30`, `unit_state_book.rs:26`). `StateValue::Text(String)` allocates a `ScriptText` on every read (`core/src/scripts/state_value.rs:74`), and each mode call clones the whole state and every value again as it commits. One `StateSchema { names: Names<StateField>, places: Vec<StatePlace> }`, built once for each unit type and for the mode; text kept shared, so a read counts a reference; the mode copies a field only when it is written.

### F37. One contract of a call part, and one list by type

(`capabilities`: `scripts`, `units`.) `CallPart::param` is on the trait, and two of four parts return `None`; the split of params by role is written three times (`stats/src/stats_call.rs:65`, `mode/src/mode_call.rs:50`, `core/src/scripts/ctx.rs:120`); `ModeCall::apply` does nothing (`mode/src/mode_call.rs:57`) and the mode commits apart, before the effects apply (`mode/src/calls.rs:114-125`). Three lists keyed by type are built three ways: `CallParts` by the vtable's `type_id` (`core/src/scripts/call_part.rs:40`), `ViewColumns` by a `TypeId` vec (`core/src/units/view_column.rs:28-37`), `Effects` by `as_any().is` (`core/src/scripts/effects.rs:31`); and each of the nine columns writes the same four delegating methods.

```rust
pub trait CallPart {
    fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError>;
    /// Writes what the call changed, in the order `COMMITS` gives: before or after the effects.
    fn commit(&mut self, world: &mut World);
    const COMMITS: Commit;
}

/// The params of one scope of a call, which the frame holds by `ScriptRole::params`.
pub trait ParamSource {
    fn param(&self, name: &str) -> Option<Dynamic>;
}

/// Items of distinct types, each found by its type.
pub(crate) struct TypeSlots<T: ?Sized> {
    types: Vec<TypeId>,
    items: Vec<Box<T>>,
}
```

The columns' shared methods become a blanket impl over a `ViewColumn` that names its rows.

### F38. What a call costs

(`capabilities`: `stats`, `scripts`.) `StatsCall::begin` computes every param of the call before the script reads one (`stats/src/stats_call.rs:30-61`), where design 08 says `ctx.p` resolves when it is read; mode params are kept as `NameTable<ScriptValue>` and cloned on each read (`mode/src/mode_schema.rs:71`), an array deeply; `Ctx` is three `Rc`s where one serves. Params resolve on read, behind a mask of those computed; a mode param is read by reference; `Ctx` holds one `Rc`. The frame keeps its copy of each player's resources, which F50 fills through a contract: a script call holds no world to read them from later, and the copy allocates nothing. Rhai boxes each custom-type value twice (`Box<Box<dyn Variant>>`), which the batch measures before any change.

### I1. The script view's copy

(`capabilities`; **Core**, and Foundations.) Each script batch reads every unit again, 14.2 % of a 3v3 match's time; it goes before the Zero Hour import multiplies the units. Its direction: a batch reads again only the rows whose parts changed since the last, by the change filter each column's `RowReads` already gives and its list of what it reads, `RowReads::components`; the columns' half of F37 lands with it, and the batch measures it.

## 4. What install holds

`capabilities`, every module's `install`; `release`. A sweep, last of the refactorings of `capabilities`' structure, over what a module adds at install: its systems alone, each fact of the build a fact of the schedule, every other addition a declaration.

### F39. What is fixed at install is a run condition

(`capabilities`.) "Return if the book is missing" is written by hand in systems (`actions/src/holds.rs:67-72`; `production/src/player_supply.rs:45`), and "return if the map has no grid" too (`vision/src/seeing.rs:48`); `navigation/src/walking.rs:35-37` takes the map's pathing grid and planner as two `Option`s and pairs them with an `expect` (`78-79`); `pay_attack_toggles` returns at once on a client (`combat/src/attacks.rs:189-190`), though the install knows it predicts (`combat/src/lib.rs:168`); the mode's hooks check its hook set each tick (`mode/src/mode_hooks.rs:44`, `121`, `146`, `177`, `250`), which `Mode::load` fixes (`mode/src/lib.rs:234`), after the install built the schedule. A declared book is always present while its owner is installed, empty until a mode loads, so a guard for a missing book goes, and an empty book gives no work; a structure the map may lack, as the pathing grid with its planner, is taken by `If<(Res<PathingGrid>, ResMut<RoutePlanner>)>`, Bevy 0.19's parameter that skips a system while it is missing; a client's schedule leaves `pay_attack_toggles` out; each hook's system runs under a run condition on the hook set the load fixed, as Bevy's `run_if` skips a system before it reads its data, and the clear of `LevelUps` becomes a system of its own. A system skipped for a missing parameter keeps its last run, so its first run after the book arrives sees every change since the build: each system is checked for that, and the goldens and the archetype shuffle test hold it.

### F49. Observers and runs out of the schedule are declarations

(`capabilities`: `declare`, `release`, `navigation`, `units`, `projectiles`.) Seven observers and two systems run out of the schedule are added by hand in `install`: `StaticsDirty`'s three (`navigation/src/statics_dirty.rs:97-99`), `UnitGrid`'s two (`core/src/units/unit_grid/mod.rs:67-80`), the view's tick check (`core/src/units/mod.rs:244`) and `Flying::lose_teleported` (`projectiles/src/lib.rs:102`); `StaticTrackingRun` and `RoomMakingRun` (`navigation/src/lib.rs:171-174`); and production's requirement that a player spawn with its supply on the server (`production/src/lib.rs:177`). The declaration audit sees none of them. `d.observer(observer)`, `d.one_shot::<R>(system)`, Bevy's one-shot system, whose resource `R` keeps its `SystemId`, and `d.requires::<H, C>()`, Bevy's required component, a derived part `C` that every entity with `H` spawns with, on the server or on every world, declare them, and the installer adds them. Two of the observers' owners keep the same mark by two mechanisms: `StaticsDirty` notes a static body that came or went by observers and finds one that moved by `Changed` filters (`navigation/src/static_tracking.rs:21-29`), and `UnitGrid` compares change ticks and a count with its last build. One tracker of a set of units, its filter, the change ticks of what it holds and the observers of what moves a unit in or out of it, serves both. Check: the declaration audit lists each observer and run.

### F51. The match's role is one value

(`capabilities`: `release`, `declare`; `net`, `runner`.) `MatchSpec` holds `budgets: Option<ScriptBudgets>` and `predicting: bool` (`capabilities/src/release/match_spec.rs:15-18`), and `InstallCx` gives `scripts()` and `predicting()` (`core/src/declare/install_cx.rs:53-66`): four combinations, of which a server's and a predicting client's ship, a server with no scripts serves the tests, and a predicting client with scripts is invalid but builds. Each install branches on the two flags its own way (`orders/src/lib.rs:97`, `production/src/lib.rs:173`, `combat/src/lib.rs:167-168`, `projectiles/src/lib.rs:101`, `vision/src/lib.rs:101`, …). One `Role`, `Server(Option<ScriptBudgets>)` or `Client`, that `MatchSpec` holds and `InstallCx::role` gives; a module adds a system that computes the server's own state through `cx.server(systems)`, which a client's install leaves out, so the prediction test's `SERVER_ONLY` (`capabilities/src/release/tests/prediction_reads.rs:30`) is read from the server's schedule, not written again. Check: the prediction test passes with no hand list of server systems.

## 5. Match records and replays

`runner`, `verifier`; the tests of `runner` and `net`. How a test builds a match, records it and replays its log, in one form each.

### H1. Replay

(`net`, `runner`.) The loop of "take the checkpoint again, stop at a tick, run alone" is in `SimServer::restore_match` (`net/src/sim_server/mod.rs:381`) and `SessionDir::abort` (`net/src/sim_server/session_dir/mod.rs:217`): `SessionWorld::replay_to(tick, on_boundary)` holds it.

### F40. One replay of a published log

(`verifier`, `runner`; the tests of `net` and `runner`.) The verifier's `Replay` (`verifier/src/replay/mod.rs:46-66`) checks each checkpoint's hash and the result; tests that cannot depend on the verifier rebuild it by hand, eight times (`net/tests/scenario.rs:161`, `bots.rs:85`, `checkpoints.rs:93`, `saves.rs:157`, `restore.rs:106`, `prototype.rs:151`; `runner/tests/match_3v3/mod.rs:571`, `resume.rs:259`), three of them repeating its check of checkpoints line for line. `Replay` moves into `runner`, on H1's `replay_to`, as `Replay::of(log, &ModePackages)`, with a trail of hashes in its internals; the verifier keeps `Verification` and its command line, and loses its one use of `capabilities`.

### F41. One record of a played match

(`runner`.) A test puts `CopyCheck::new`, `Golden::new` and `HashTrail` together by hand, and each tick calls `copy.check`, `golden.record` and `trail.record_hashes` (`runner/tests/proving_match.rs:211-226`, `348-365`; `lane_match.rs:66-68`; `match_3v3/mod.rs:164-181`). `MatchRecord { copy, trail, golden }`, with `of(&mut Match, packages, players)`, `tick(&mut Match) -> MatchHashes`, `check(name)` and `assert_replays(&log)`, on F40's replay; `CopyCheck::new` becomes `CopyCheck::of`, as a value made from the world is.

### F42. One match built with no mode started

(`runner`, `common`.) A zero seed, `MatchBuild::spec`, `Match::new` and `install_mode` are written three times (`runner/src/session/mod.rs:112-117`, `match_build/mod.rs:121-126`, `match_build/bench.rs:40-44`), and `MatchBuild` is `pub` only for the internals, with an `expect(unreachable_pub)`. A production `MatchBuild::restore_target(packages, rate, players)` serves all three, the internals add only the `LogCheck`, `SegmentSeed::ZERO` replaces the repeated zero seed, and the lint's suppression goes. F51's `Role` replaces the flags it passes.

## 6. The session's host

`net`, `server`, `client`, `runner`. Its code stays in `net`, which hosts every session (design 02), and the operating system's clock and randomness travel as one value.

### H2. Server compositions

(`net`, `server`.) The task pool, time, states and `SimServer` setup appear three times (`server/src/main.rs:246-256`, `net/src/local/local_server/mod.rs:345-360`, `net/src/harness/in_process_match/mod.rs:343`): one `PluginGroup`, `SimServerPlugins { tick, packages }`, holds them, as Bevy's `MinimalPlugins` groups the task pool, time and the run loop; each composition adds it and its own plugins, a run loop where it runs one. The server's edge to `sim`, used once, goes.

### H3. Restore glue

(`net`, `server`.) The server's `Restore` (`server/src/opening/mod.rs:37`) and net's `PendingRestore` (`net/src/local/local_server/mod.rs:102`) have the same fields and body: one type in `net`.

### H5. Log events

(`net`.) The match's end is logged ad hoc on the server and on the client (`net/src/sim_client/mod.rs:555-559`), and the client logs a death ad hoc though `UnitDied` exists: a `MatchEnded` log record beside `UnitDied` in `net/src/events`, a record of the log and not a Bevy event or message, and `UnitDied` on both ends.

### F43. The clock and the entropy as one value

(`net`, `server`, `client`.) `clock: fn() -> u64` and `entropy: fn(&mut [u8; 32])` travel as two function pointers through some ten types (`SimClient`, `net/src/sim_client/mod.rs:104-106`; `ServerSetup`; `LocalServerSetup`, `local_server/mod.rs:123-125`; `Offering`, `JoinState`, `ServerSigner`, `KeyFile`, `SessionDir::abort`, `server/src/opening/mod.rs:119`). A function pointer holds no state, so the tests' settable clock lives in two `thread_local!`s (`net/src/harness/in_process_match/mod.rs:73-80`, `net/src/sim_client/join_state/tests.rs:16-22`), which the style rules refuse. `Os` (`net/src/os.rs`) becomes a value, `Os { clock: Clock, entropy: Entropy }`, where `Clock` is `System` or `Manual(Arc<AtomicU64>)`; every setup holds one `os: Os`, and the harness keeps the `Arc` as it keeps its pace. Both thread-locals go.

## 7. The tick and the tests, measured

`capabilities`: `navigation`'s broadphase, `vision`'s bitmap, the storage of toggled parts; `package`, `runner`. A bench session: it runs when the user asks for the benches, after the refactorings above, so it measures the code they leave. The tests past their bound go here, as the proving match's spend their time in the broadphase and collider that F46 changes.

### F46. The broadphase in the grid's form, measured

(`capabilities`.) The broadphase (`navigation/src/broadphase.rs:31-43`) sorts a four-field entry, a `Place` of layer, level, row and column beside the collider's index, where `BodyGrid` packs one number per entry, a `CellKey` of level, row and column above the stable id: the step takes `BodyGrid`'s form for the broadphase, and measures again on it the levels by size that make Collide slower for walkers of ordinary sizes (**Navigation**). Check: the goldens do not move, and `./drive.py bench -- integration/collision/` runs before and after.

### I4. Tests past 1 s

(`package`, `runner`; **Package**, **Runner**.) Four tests run at or past the bound of 1 s for a test alone: `mode_package::every_flaw_of_a_package_fails_its_load_with_its_own_problem`, `resume::the_3v3_match_resumes_from_each_checkpoint_to_the_same_hashes`, and the proving match's `a_snapshot_of_drawn_values_restores_or_is_refused_and_never_panics` and `a_snapshot_restores_and_a_flawed_one_never_panics`. Find where each spends its time, after F46, and bring each within its bound, or into `source/.config/nextest.toml` with the reason it takes longer.

### F52. The worst tick, measured part by part

(`capabilities`, `runner`; **Runner**, **Vision**.) Design 02's worst 3v3 tick, 0.83 ms, and design 10's, 0.51 ms, disagree; `Bitmap::set` takes 6.9 % of the mean tick, for runs of one or two words a row. The session measures `integration/server_tick/worst_3v3` again and writes the figure into design 02, finds which part of `Bitmap::set` costs it, and measures two changes the engine's best practice suggests before either lands: parts that come and go while a match runs, `Dead`, `Respawn`, `ForcedMove`, `Resetting`, `OutOfPlay`, `Kept`, `Rally`, `AreaTrigger` and `Unpredicted`, stored in Bevy's sparse sets, where a change moves no table, as Bevy's `StorageType` advises for parts added and removed often; and the twelve systems that sort their units by stable id each tick (`Local<Ordered>`) reading one shared order instead. Each lands only where the worst tick gains. Check: the goldens do not move; the archetype shuffle test passes.

## 8. The shop

`capabilities`: `items`; `package`; `packages/test/moba`. Slice 2 starts here. A shop becomes a unit, then the shop's reader, an item's price and the items' names are built on it once, and the 3v3's golden moves once.

**Slice 2, wide bodies, uses, the shop and gravity,** batches 8 to 12. The user plays a 3v3 match of the MOBA test content: their hero holds a use on the jungle's shrine, buys items from the shop panel, and casts their actives with 1 to 6, a ward at the ground point; a test map holds a 1,117 m box that walkers go around. The design is [Wide bodies, uses, the shop and gravity](docs/design/13-bodies-uses-shop-gravity.md); each step folds its rules into the design 04 file of its capability.

**The capability model's redesign,** M1 to M10, a proposal for review: each step's decisions move into `docs/design/04-capabilities/` as it lands, and each generates the script API reference again. The core model is sound and generic: the action pipeline, the effect lists, stats and their formula, tags, relations and filters, the reach rule, deliveries as units and scripted systems serve every genre, and stay. The faults are at the edges, where a capability was built for its first genre: batches 1 to 6 take its couplings, and M1 to M10 open its closed sets. Each new section of a unit type an M step or U1 adds is built as the Direction gives it: a `DataSection`, its owner's record in `DataTable::UnitType`, its book, its part, `d.part`, and an `Effect` with its callback, `d.callback`, for each new effect.

### M1. Shops as units

(`capabilities`: `items`; `package`; `packages/test/moba`.) A unit type's `shop = { items, resource, reach, buyers }` makes a unit a shop, a part `items` declares: `reach` a radius by the reach rule, `buyers` a filter, which may select the dead, so League of Legends' rule is data. The mode's `[shop]` and its markers go; the 3v3's fountain is a unit with a `shop` section. Warcraft III's shops are units, as a courier or a market is. Check: the 3v3's golden moves by the new unit's id alone.

### S1. Names and readers

(`capabilities`, `package`, `packages/test/moba`; D8; **Package**.) `name`, a message id, on item types, actions, a loadout's spells, the mode, its teams and its choices, as only an avatar names its own today, and the MOBA's names; a public reader of the shop, an item's price for a unit, whether a unit stands where it can buy, and an action's targeting. A player's resources reach it on its player entity, `PlayerResources::of_player`. Check: a price less held components as computed by hand; an item type with no `name`, or one a locale lacks, fails its load.

## 9. The client's shop

`client`. I2 gives the view tests' units their types, and the issue log's **Client** item on the camera's keys is decided, before S2 changes the client's input.

### I2. The client view tests' units

(`client`; **Client**.) They spawn with no unit type, which no client meets, and the view reads the type as optional.

### S2. The shop panel and item keys

(`client`; D9, D10, D11.) Bevy's UI features; the shop panel, the inventory bar, keys 1 to 6, and every cast key aimed as its action takes. Check: the key's target for an action of each targeting; the panel's dimmed items for an avatar alive out of a shop, too poor and with no room; the user buys a ward, places it at a point with its key, sells an item and swaps two slots.

## 10. Use

`capabilities`: `actions`, `abilities`, `items`, `interaction`, the release tests; `package`; `client`. U1 adds the `use` kind, beside `actions`' `UseGroup`. M2 reads the code as the release tests do.

### U1. The `use` kind

(`capabilities`, `package`, `client`; D4, D5.) The `use` and `uses` sections and their load checks; the `use` order; `damage_interrupts`; `on_channel_tick`'s target; the script API reference generated again. Check: a use resolves on its object; a unit without `uses`, one that `users` refuses and one out of range that cannot walk drop the order; a held use is cut by a new order, a block of `use`, a death and, with `damage_interrupts`, by damage, not by a heal; a second use of the same object keeps its slot, and one of another object fills it afresh.

### M2. Genre words stay out of the engine

(`capabilities`: the release tests.) A test reads every capability crate's production code, as the role test does, and fails on an identifier that holds a word of a deny list as a whole word: `hero`, `creep`, `lane`, `tower`, `jungle`, `mineral`, `gold`, `mana`, `barracks`, and the words the list grows by; test modules may hold them. Today the production code holds none.

## 11. The shrine

`packages/test/moba`, `client`.

### U2. The shrine

(`packages/test/moba`, `client`; D6.) The shrine, its action, `shrine_blessing` and `shrine_spent` in design 07 and the 3v3 package; a right click on a unit the avatar may use orders the use; the channel bar. Check: the scripted match has a hero use the shrine and the team's heroes take the blessing, and a second hero's use cut by a hit; the user holds a use on the shrine in the client.

## 12. Gravity

`capabilities`: `projectiles`; `package`; `packages/test/moba`; `client`. Slice 2's last step: the user then plays slice 2.

### G1. Gravity

(`capabilities`, `package`, `packages/test/moba`; D12.) The falling flight; `gravity` past planned, refused with `homing` or not positive; a grenade of the MOBA's items, which arcs to its point. Check: the points of a flight on each metric at its launch, its peak and its end as computed by hand, the planar one back at its launch height at its end; a spatial one hits a unit its chord meets and not one its straight line would; the client draws the grenade's arc.

## 13. Queues and capacities

`capabilities`: `production`, `actions`; `package`; `docs/design/11`. Train, research and supply in one form, before the Zero Hour skirmish, so its rules package and golden are made once, on the general model.

### M3. Queued actions with any output

(`capabilities`: `production`, `actions`; `package`.) Today `train` spawns a unit type alone; research has no kind, so an RTS times it with a mode timer; `craft` is planned as a third kind of "consume, wait, produce". Any unit may have `queue = { size }`. An action with `queue = true` joins the queue in place of a windup: it pays at once, waits its time at the head, then resolves with its `on_resolve` effects and hook. `train` becomes such an action with a `spawn` effect, research one with a `player_modifier = { id }` effect, craft one with a `give_item` effect, planned with items; cancel and refund move to the queue. The `train` kind goes, and no `craft` kind comes. Today's train rules become the queue's: checked as it joins, its costs, requirements and capacities (M4); never under way, so its unit stays free; its time from the tick it reaches the head; it resolves in the Mode stage of the tick its time ends in, and the next entry starts in that tick; a dead producer's queue waits; a producer that despawns takes its queue and refunds nothing. A queued `spawn` places its unit by the train's rule, the producer's body point nearest its rally point and the nearest open cell, and sends it to the rally point; a `spawn` of any other action keeps its own rule. StarCraft II's `CAbilQueue` trains and researches through one queue. Check: production's train, cancel, supply and rally tests pass with their assertions unchanged, on data that states a train as a queued action.

### M4. Capacities

(`capabilities`: `production`; `package`.) One `[supply]` counter serves StarCraft alone. The mode declares `[capacities.<name>] max`; a unit type gives `capacity = { <name> = { uses, provides } }`. Each is derived as supply is now, a pass over the owned units and the queues when a tick's first check needs it, so it keeps no sum and never drifts (design 11, D5). A queued action or a build lists the capacities it checks. Supply becomes one capacity; power, population and housing are more. Scripts read `ctx.capacity_used(player, name)` and `ctx.capacity_max(player, name)`.

## 14. Work and haul

`capabilities`: `production`, `interaction`; `package`; `docs/design/13`. Construction and gather in their general form, after batch 13, whose queued actions a production chain uses.

### M5. One work mechanism

(`capabilities`: `production`, `interaction`; `package`; `docs/design/13`.) Construction's rate table by builders and interaction's shared progress are one mechanism, one native and one planned as script state. A unit type's `work = { rates = [...], max_workers }` makes a unit that others work on: `rates` gives the rate for each count of workers from zero, a count past the table takes its last entry, and `max_workers` refuses a worker past it. The three construction styles become three tables: `alone` `rates = [1]`; `builder` `rates = [0, 1]`, `max_workers = 1`; `builders` `rates = [0, 1, 1.62, 2.16]`. Progress, its rounding of life and its completion tick stay as construction defines them. Completion runs the type's `on_complete` effects and the mode's `on_work_complete(ctx, unit)` hook. A held use with one side's progress, plant, defuse, revive, repair, becomes work, and M5 then amends design 13's D5 and `interaction.md`, which keep shared progress as script state until it lands; slice 2's shrine has no progress, so neither waits on the other. A capture point, by team with contest and decay, is an extension planned with `interaction`. TF2's capture rate by the count of cappers is the precedent. Check: the three styles' construction tests pass, tick for tick.

### M6. Haul

(`capabilities`: `production`; `package`.) Gather fills only a player resource, from a node to a drop-off. A unit type's `stock = { <resource> = { amount, max } }` gives a unit its own amounts. `gather` becomes `haul`: `from` filters units with stock of the resource, a node being one with no `max`; `to` filters units that take it; `take` is the most one trip carries; `windup_ms` the time at the source. A sink's section names where it takes: `drop_off = { resources, into = "player" }` is today's drop-off, `into = "stock"` adds to its own stock within its `max`. The loop, its wait, its `bounce` and its tie rules stay. A production chain is a queued action (M3) with a `stock_cost` from its producer's own stock and a `stock = { resource, amount }` effect: a smelter turns ore into bars. A cost from stock is its own field, as `cost` names pools and player resources, and a stock's resource shares a player resource's name. RimWorld's haul jobs and The Settlers' carriers move goods this way.

The snapshot tests sweep the new state types of M3 to M6 as they sweep the skirmish's (design 11, D14).

## 15. The genre proofs

Each of M7, M8 and M9 goes in the batch of the genre proof that first needs it, and M10, the Hive mode, after them.

### M7. Points as track data

(`capabilities`: `progression`; `package`.) A track gives `points = [...]`, the points each level gives, or `points = 1` for each level; the `level` track keeps one a level, so the MOBA keeps its data; a second track may give points, for Skyrim's perks.

### M8. The player as an actor

(`capabilities`: `units`, `players`, `actions`; `package`.) Each player slot gets a player unit beside its player entity of design 14, which keeps the player's resources and modifiers: a unit of its own kind, `PlayerUnit`, whose marker requires `UnitEntity`, so every system of bodies, `With<BodyUnit>`, passes it by; with no body and no pools, so it collides with nothing and takes no damage; the engine tag `player`, which `players` declares, `d.engine_tags`, with the property that a filter selects its unit only when it names it, as `projectile` and `area` have; `players` comes second in the module list, so every engine tag after `avatar` moves by one, and the state column and the data version move with M8; its own team sees it, so its owner receives its cooldowns and charges; it stands at the centre of the map's bounds, which no rule reads, as its actions aim with global range. Its actions run through the pipeline as it is, and pay from its player entity's resources: tower defense's placement, Generals' powers and card play need no second pipeline. Unreal's GAS gives the PlayerState an ability system for this. A `build` action of the player unit places a site with no builder's walk. A player unit takes a stable id only in a mode with `[player] slots`, so other matches keep their ids and goldens.

### M9. Movement that spends

(`capabilities`: `navigation`, `stats`; `package`.) A unit type's `move_cost = { pool, per_meter }` makes each of its own steps pay from a pool; a push of collision and a forced move pay nothing, as neither is the unit's choice. A step the pool cannot pay is not taken, and the unit stands as a blocked unit does. The rounding is per step, exact, and kept as a pool's carry is.

### M10. The Hive mode, a game no current mode describes

(`packages/test/modes/hive`.) A test package with a scripted match and a golden: two players as hive minds with no avatar place structures (M8), drones haul ore to smelters and bars to a forge (M6), the forge crafts turrets and researches armor (M3), each structure draws power that pylons provide (M4), a beacon is captured faster by more drones (M5), a drone with a bar spends fuel (M9), enemy waves walk paths and turrets shoot them through their AI, and the drones need no `combat`. A later change that breaks a part of the generic model breaks it.

What each step of M1 and M3 to M9 may move of the goldens; a step that moves a column it must not is a bug in the step:

| Step | State column | Behaviour column |
| --- | --- | --- |
| M3 to M6 | Moves where production's state types change | Does not move for the proving match; the skirmish's is new |
| M1 | Moves: a shop becomes a unit | Moves for the 3v3 by the new unit's id, if at all |
| M7, M9 | Does not move: the defaults keep today's rule | Does not move |
| M8 | Does not move in a mode with no `[player] slots` | Does not move |

## 16. The network

`quic`, `net`.

### N1. Limits for each address

(`quic`, `net`; **Quic**.) No limit holds live connections or new handshakes for each address, where design 15's D13 bounds both by the session's seats for each IPv4 address or IPv6 /64, and `net` gives the endpoint no seat count ([QUIC transport](docs/design/15-quic-transport.md), D13). After the bench of the endpoint, the issue log's **Quic** item on benches, sets the cap of new connections on the same path.

### N2. UPnP behind a host firewall

(`quic`; **Quic**.) `igd-next` discovers a router by search alone, and the router's `NOTIFY` announcements, which a host firewall that tracks connections lets in, go unread ([QUIC transport](docs/design/15-quic-transport.md), D12).

Then the network's Research items of the issue log as they show; then P1 to P4, P3 only as D14 of design 15 lets it.

## 17. Planned

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

- **The modules of `campfire-core` import each other** (Core; F50, F53). The rule test of `common` refuses a cycle of a crate's top modules, but `campfire-core`'s four modules import each other, and it names that crate as its one exception: a crate whose modules import each other cannot split along them. Items: Core, "The four modules of `campfire-core` import each other".
- **The script view copies the whole match** (Core; I1). Each script batch reads every unit again, though few change between two batches of a tick: 14.2 % of a 3v3 match's time, a cost that grows with the units times the batches, which the Zero Hour import multiplies. Items: Core, "The script view copies the whole match".
- **The import's materials are glTF's** (Zero Hour). glTF's material model holds no W3D pass past the first, no ambient color and no fallback texture, so the presentation loses each, and each later fidelity gap of a W3D material meets the same limit. Items: Zero Hour, "The import writes W3D's materials into glTF's material model"; Zero Hour, "A texture no archive holds"; Zero Hour, "The import ignores a vertex material's ambient color"; Zero Hour, "The import draws a mesh's first material pass".
- **The import has no object that only the client shows** (Zero Hour). Every object a map places becomes a unit of the sim, where the game spawns props, optimized trees and fluff on the client alone and some objects not at all, so the sim, its log and its state hash hold objects the original never simulates, and the parity oracle meets them. Items: Zero Hour, "The import makes a unit of every object a map places".
