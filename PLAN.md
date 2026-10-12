# Plan

The one plan of the project: every open step, in the order it is done, in batches. A batch is one commit: it holds the steps that change the same systems, refactorings, fixes and features alike, and ends with `./drive.py check` passing, and a stop for review. Each batch says what it touches and why it stands where it does. Open items only: a step goes when it is done. The issue log, `.notes/ISSUES.md`, holds every open problem; a problem that a step here takes carries the step's tag there ([Issue log](AGENTS.md#issue-log)). The designs in `docs/design/` stay the source of truth: a step names what the code lacks against its design, or the design it proposes, which moves into `docs/design/` when it is agreed.

One policy holds across the steps: **what can be forgotten is a type or a trait method with no default.** A rule that no type can hold is held by a type that makes the right way the short one, a test where one can see the breach, and review. Line numbers point to the code at commit `3de94c9d`; a step refreshes its references as it starts. A step that states no check holds this one: no golden moves, and `./drive.py check` passes.

## Direction

Every batch moves the engine toward one shape, and each item names the part it builds.

- **A module is one entry of one list.** Each module, the core's `units`, `players`, `actions` and `deliveries`, each capability and the mode, implements `Module`: its capability, its needs and its uses, `declare` and `install`, none with a default (F44 adds the uses). `install` holds only its systems, each in one of its module's declared sets (F45); all else a module adds is a declaration.
- **What a module adds is declared once,** and one walk of the declarations serves the installer, the script API's binder, the network's lists of state types and the declaration audit:

| What | Declaration | Item |
| --- | --- | --- |
| A state type, a state resource | `d.component`, `d.resource` | today |
| A resource that is not state: clear between ticks, built again, or a tick's report for the host | `d.scratch`, `d.cache`, `d.report` | today |
| A registry of callbacks a lower module calls, and each callback in it | `d.calls`, `d.callback` | today |
| A system set's stage or edge, and its order against other modules' sets | `d.sets` | F45 |
| A part every entity of a kind starts with | `d.part` | today |
| Engine tags | `d.engine_tags` | today |
| Package data a match holds | `d.book` | today |
| A column of the script view, a part of a script call | `d.column`, `d.call_part` | today, F37 |
| Script members, hooks and data tables | `d.api` | today |

- **A kind is a marker,** one on each entity; a kind's parts are declared; a derived part comes by a hook of the part it derives from.
- **A name is resolved once,** at the load: one table of names for each id, `Names`, a builder that is the check of what it resolves, a script argument resolved by its type, `ScriptArg`.
- **One word, one meaning:** module, owner, kind, part, callback (`Hook` is the scripts' alone), `*File`, `*Data`, `*Spec`, `*Book`, which the release tests hold.
- **A pass visits what changed:** no query state, structural change or allocation per item on a hot path (F33, I1), and a fact fixed at install is a fact of the schedule (F39).

So a new capability is one entry of the list; a new state type its type and one `d.component`; a new section of a unit type a `DataSection`, its owner's data record and a `d.part`; a new effect an `Effect` and a `d.callback`. The declaration audit and the release tests fail on each piece left out.

## 1. The 3v3's assist gold

`packages/test/moba`. A wrong result of the shipped test content, before any step adds to the 3v3's scripted match, whose golden would pin it.

### I3. Assist gold to a unit no player owns

(`packages/test/moba`; **MOBA test content**.) The 3v3's `hero_died` gives assist gold to each of its `assisters` by the assister's `owner`, and fails for an assister that no player owns, a tower: the call changes nothing, and the kill gives no gold, the killer's included. Check: a hero kill that a tower assisted gives its killer and its hero assisters their gold.

## 2. The LAN check's restart

`campfire-lan-check`. A check that fails at times makes each later failure harder to trust.

### I6. A restart after each bot's order of the stamp

(`campfire-lan-check`; **LAN check**.) The check starts bot 1 again as soon as the server logs bot 1's first order, while bot 0's order of the same stamp may still be on its way, and it fails an order that misses its stamp tick: in CI run 37523961750 on Ubuntu, bot 0's order reached the server 115 ms later, in tick 21, as the new bot 1 process started on the same 4 cores. The restart waits until the server logs the order of each bot for that stamp. Check: the LAN check passes on the three platforms of CI.

## 3. Each rule of a package in one home

`capabilities`: the book builders, `books`' errors; `package`'s load check. Untrusted data never reaches a builder's `expect`.

### I5. The builders return each rule's case

(`capabilities`, `package`; **Package**.) The book builders `expect` rules that the load check also checks, which are not names: an action's ranks and its fields at each rank (`RankValues::all`, `ActionParts::of`), the ranks the slots give (`BookBuilder::slotted_ranks`), a shop marker's team and region, a build's box body, a homing delivery's projectile section, an effect's numbers (`Amount::of`) and its durations, a fan's spread, a site's start life, an action's range in meters (`ActionBook`), the pools' limit (`PoolBook::new`), and the tracks' limit and their one level track (`TrackBook::new`). Each rule has two copies, and a check the load loses becomes a panic at the build. Each builder returns the case of the rule that fails, as it does for an unknown name; `BookError` gains a case for each, which `LoadCheck::book_error` turns into its `LoadProblem`, and the load check's own copy of each rule goes. Check: each flaw of `mode_package`'s sweep fails its load with the problem it fails with today, and none panics.

## 4. The action pipeline's seams

`capabilities`: `actions`, `combat`, `projectiles`, `stats`, `items`, `abilities`, `areas`, `mode`, `orders`, `production`, `declare`, every module's install, the declaration audit; `sim`'s stages. What `actions` owns, one fixed order of steps in each stage, and each module's sets declared with their place and their order. No golden moves.

### F30. Action anchors and the delivery seam

(`capabilities`.) Most capabilities need `combat` only to order their systems against `CombatSet`: `vision` imports nothing of `combat` and needs it; `areas` and `items` order against it alone. `projectiles` reads `combat::shots::Shots` (`projectiles/launching.rs`) and pushes damage to `combat::pass_queue::PassQueue` (`projectiles/flights.rs`, `projectiles/flying.rs`), and `abilities` reads the life pool through `CombatWorld::life_pool`, though `LifePool` is `stats`'.

- `actions` owns the sub-stages that every action kind and delivery orders against: `Start`, `Deliver`, `Launch`, `Land`, `Resolve`. `CombatSet` keeps combat's own steps: the damage pass, deaths and respawn.
- A weapon's shot is a delivery request in `actions`, and its hit resolves through a callback that `combat` declares, `d.callback`: `projectiles` names no type of `combat`.
- `life_pool` moves from `CombatWorld` to `stats`' world trait.

Each capability's needs then are what its code needs: `vision` the core; `areas` and `abilities` `stats`, whose pools and modifiers they change; `projectiles` the core; `orders` `navigation`. F44 states them. Check: no golden moves; the archetype shuffle test and the schedule build's check of conflicts pass.

The anchors are the action pipeline's own steps, which every capability that acts orders against: the core names them, and they name no capability. Inputs keeps each module's own sets, ordered against the sets of the modules it builds on, as F45 declares them: one chained enum of every stage's steps would name the capabilities' concerns, `Toggles`, `Orders`, `ModeInputs`, in the core, against the rule that the core names no capability, and order every pair of its steps, though most never touch, so no order would show which pairs must keep it. The mode, the top of the list, orders against eight capabilities' sets, each a use F44 states.

### F45. Each module's sets declared with their place and order

(`capabilities`: `declare`, every module's install and system sets, the release tests; `sim`'s stage test.) A system names its stage and its order against other modules' sets by hand: 93 `.after` and `.before` and 88 `.in_set` on single systems, against 5 `configure_sets`. The same anchors come again system after system (`ActionsSet::HoldAtInputs` 5 times, `StatsSet::Regenerate` 4, `CombatSet::Launch` 4), the mode's `mode_inputs` carries nine constraints (`mode/mod.rs`), and `abilities` writes the order of Hit twice, for the server's casts and for a client's (`abilities/mod.rs`). No check holds a copy to the others, or an order to the module's needs. Bevy's practice is to order sets and put systems in them; a set's place and order become a declaration:

```rust
// declare/module_set.rs
/// A module's system set: its place in the sim schedule and its order against other modules'
/// sets, stated once. Each method matches every variant, so a new step does not compile until it
/// has both.
pub(crate) trait ModuleSet: SystemSet + Copy {
    /// The capability of the module that owns it; `None` for the core's.
    const CAPABILITY: Option<Capability>;
    /// Every variant, from `enum_all!`.
    const ALL: &'static [Self];

    /// The stage or the edge it runs in.
    fn place(self) -> SimPlace;

    /// The sets it runs after and before, each recorded with its module's capability.
    fn order(self, o: &mut SetOrder);
}

// abilities/mod.rs
impl ModuleSet for AbilitiesSet {
    const CAPABILITY: Option<Capability> = Some(Capability::Abilities);
    const ALL: &'static [AbilitiesSet] = &AbilitiesSet::ALL;

    fn place(self) -> SimPlace {
        match self {
            AbilitiesSet::Toggles => SimPlace::Stage(SimSet::Inputs),
            AbilitiesSet::Resolve => SimPlace::Stage(SimSet::Hit),
        }
    }

    fn order(self, o: &mut SetOrder) {
        match self {
            AbilitiesSet::Toggles => {
                o.after(StatsSet::Regenerate)
                    .after(CombatSet::Respawn)
                    .before(ActionsSet::HoldAtInputs);
            }
            AbilitiesSet::Resolve => {
                o.after(ActionsSet::Deliver).before(ActionsSet::Launch);
            }
        }
    }
}

fn declare<D: Declarations>(d: &mut D) {
    d.sets::<AbilitiesSet>();
    d.api(AbilitiesApi::register);
}

fn install(cx: &mut InstallCx<'_>) {
    // …
    schedule.add_systems(Casts::start_casts.in_set(ActionsSet::Start));
    if !cx.scripts() {
        let predict = (Casts::predict_casts, Channels::predict_channels).chain();
        schedule.add_systems(predict.in_set(AbilitiesSet::Resolve));
        return;
    }
    schedule.add_systems((
        (Casts::resolve_casts, Channels::run_channels)
            .chain()
            .in_set(AbilitiesSet::Resolve),
        Toggles::run_toggles.in_set(AbilitiesSet::Toggles),
    ));
}
```

- A system names one set of its own module or of an anchor, and orders within its module by `.chain()` alone; it names no stage, edge or other module's set.
- `sim` gains `SimPlace`, a stage or an edge, the one home a set names.
- `Declarations::sets` has no default. The installer, which gains the schedule `InstallCx` holds, configures each variant in its place and order; `combat`'s `configure_sets` and every set configured in an install go. The declaration audit records each set with its place and the capability of each set it orders against, and fails on a set two modules declare; F44 checks the capabilities.
- What the install knows stays a plain `if` in `install`, which chooses the systems that go in, as F39 asks: scripts, prediction and the declared capabilities.
- `systems_outside_stages` (`sim/src/sim_world.rs`) becomes the test that each game system sits in a declared module set, an anchor among them.

Check: no golden moves; the schedule build's check of conflicts, the archetype shuffle test and `every_order_the_proving_match_relies_on_is_stated` pass.

### F31. One grant mechanism

(`capabilities`.) Three loops keep the modifiers a source wants on a unit: `Holds::hold_passives` (`actions/holds.rs`), `ItemHolds::hold_items` (`items/item_holds.rs`) and `HeldPass` (`stats/held_pass.rs`); the first two are one loop, and perks and equipment are planned as two more sources. A capability declares a grant source, a callback, `d.callback`: it answers which `(modifier, rank, applier)` a unit wants, and whether that changed since the last pass. `stats` owns one pass that compares each changed unit's wanted set with its held set, and applies or releases. The sources are action passives and holds, carried items and the unit type's passive, which `ModeUnits::spawner` applies today. Presence grants, auras, areas' insides and player modifiers, stay in `HeldPass`, as space finds them each tick.

```rust
// stats/grant_sources.rs
/// What a source wants on the unit: each modifier, at its rank, from its applier.
pub(crate) type Wanted = fn(EntityRef<'_>, &mut Vec<Grant>);

/// The grant sources of the match, the callbacks each capability declares. Not state.
#[derive(Resource, Debug, Default)]
pub(crate) struct GrantSources(Vec<Wanted>);

impl Callbacks for GrantSources {
    type Key = Capability;
    type Callback = Wanted;
    // …
}
```

The pass runs where a source's answer changes today: as each tick starts, after Hit's casts and attacks, after Resolve, and at the tick's end; each run visits only the units whose source or modifiers changed, and those `ReadyWaits` keeps. Two sources that grant one modifier from one source unit share one hold, as two items do; the load still refuses a modifier that two kinds of source grant. Check: no golden moves.

## 5. Test units from data

`capabilities`: the tests of each capability, `units`' test spawns, `declare`'s restore check. After batch 4, whose grants change how passives and holds reach a unit.

### I7. Each test unit spawns as a mode's unit

(`capabilities`; **Core**.) The test spawns, `Units::spawn`, `Units::spawn_of` and `Units::spawn_delivery_of`, give a unit none of the parts the modules declare, so many tests check units that no production path makes, with `UnitStats` set by hand, and a restore checks the declared parts only of a delivery and of a unit of a type the mode's spawner spawns. The tests load their unit types as data, their stats, pools and combat among them, and spawn through the mode's spawner, as production's tests do; the bare test spawns and the restore check's exception for their units go. Check: each test keeps its assertions; no golden moves.

## 6. Commands to their owners

`capabilities`: `orders`, `items`, `production`, `progression`; `client`; the session log's format version. After batch 4, as both change what `orders` orders against.

### F32. Each capability owns its commands

(`capabilities`, `client`.) `Action` (`orders/order/mod.rs`) holds `Learn`, `Buy`, `Sell`, `Swap`, `CancelTrain`, `Rally`, `Build` and `CancelBuild`, and `Orders::install` schedules `ProductionOrders` and `TradeOrders`: a mode that drives its units with no `orders` cannot buy, learn, cancel or rally, and the overview says each command names the capability that owns it.

- `items` decodes `Buy`, `Sell` and `Swap`; `progression` `Learn`; `production` `CancelTrain`, `Rally` and `CancelBuild`.
- `orders` keeps the orders a unit carries out, move, attack, slot, build and stop, and the shared parts, the unit list's decoding and the check that the player controls the units, which each capability calls.
- Each capability's commands apply in Inputs in the module list's order, so a tick's buys and cancels apply as now.

The commands' bytes in a session log change, so the log format's version rises. Check: the goldens' state and behaviour columns do not move.

## 7. One grid of units

`capabilities`: `units`, `actions`' targets, `projectiles`, `areas`, `stats`' held pass, `navigation`'s `make_room`. After batch 4, whose grants settle what `HeldPass` keeps. The grid is a cache, `d.cache`.

### F33. One grid of units, rebuilt when units move

(`capabilities`.) `Flying::fly` (`projectiles/flying.rs:64`), `AreaLife::trigger` (`areas/area_life.rs:62`), `AreaHolds::hold_inside` (`areas/area_holds.rs:41`) and `HeldPass::run` (`stats/held_pass.rs:152`) each keep a `Local<BodyGrid>` and rebuild it, a sort of every living unit (`units/body_grid/mod.rs:126-160`), all after Collide, the last stage that moves a unit: up to four sorts a tick. `StaticTracking` (`navigation/static_tracking.rs:59`) already rebuilds only on change. One derived `UnitGrid` in `units`, of every unit with a place, the dead among them, keyed by `Entity`, rebuilds when a unit came, went, moved or changed its body since its last build, by change ticks and an observer's mark as `StaticsDirty` keeps them; each reader checks team, tags, life and death as it visits, through its own query:

```rust
grid.current(&units, this_run).visit_near(pos, radius, |body| {
    let Some(unit) = targets.unit(body.key) else { return };
    // the existing exact test of reach
});
```

`Navigation::make_room` asks the grid for the walkers a new box overlaps, in place of the scan of every walker that `RoomMaking::make_room` runs. Each reader already runs an exact test of reach and sorts its results (`area_life.rs:90`, `held_pass.rs:174`, `flights.rs`), so a wider set of candidates adds only ones the exact test refuses, and the result is the same bit for bit. `TargetIndex`, the script view's index over rows, stays apart, as its order of visits is the grid's. The broadphase (`navigation/broadphase.rs:28-41`) sorts a four-field entry where `BodyGrid` packs one number per entry: the step takes `BodyGrid`'s form for both, and measures again on it the levels by size that make Collide slower for walkers of ordinary sizes (**Navigation**). Check: the goldens do not move.

## 8. State types and their lists

`capabilities`: `state_types`, `release`; `net`'s replication. What each reader of the state types handles, and each list of them stated once. No golden moves.

### F34. One call for each state type

(`capabilities`, `net`.) `StateTypes` has nine methods, one for each way of sending, and ten implementations (`release/installer.rs:51`, `release/state_only.rs:24`, `release/mod.rs:287`, net's `kind_filter.rs:287` and `425`, `net/tests/leak.rs:137`, …), all but `Replicated` (`net_protocol.rs:117`) treating the five ways alike. `StateTypes` takes `state<C>`, `sim_state<C>`, `derived<C>`, `derived_sent<C>` and `resource<R>`; only `Replicated` implements a second trait of the five ways, which `C::Sending` visits. Some 250 lines go, and `sending.rs`'s checks at compile time stay. Check: no golden moves.

### F35. Net's data kinds listed once

(`net`.) A data kind is listed in five places: `filtered_kinds!` (`net/src/sim_server/kind_filter.rs:49`), the register calls (`:235-249`), `FilterObservers`' match (`:261`), the test's asserts (`:477`) and `DataKind::ALL`. The macro generates the registration, the exhaustive dispatch with its arm of unfiltered kinds, and the test's list, and `enum_all!` keeps `DataKind::ALL` complete. Each filtered kind's `Add` observer inserts its `KindFilter<K>` by command, a move between archetypes for each kind at every spawn on the server: a required component with a closed default, whose `on_add` hook writes its value in place, adds it in the spawn's move, once replicon's `on_insert` observer is checked against it. Check: the leak test passes.

## 9. The script call's runtime

`capabilities`: `scripts`, `units`' view and columns, `mode`'s state, `stats`' call. What a script call costs and how its parts are kept, with the view's copy of the match.

### F36. One schema of script state

(`capabilities`: `mode`, `units`, `stats`.) Mode, unit and modifier state each look a field up their own way: `NameTable` and places kept for the mode (`mode/mode_schema.rs:25-71`); `UnitStateBook::field`, which counts the field's place on every access (`units/unit_state_book.rs:50-63`); a binary search of its own for a modifier (`stats/modifier_handle.rs:189`). Two `StateField` types differ (`mode_schema.rs:35`, `unit_state_book.rs:23`). `StateValue::Text(String)` allocates an `ImmutableString` on every read (`scripts/state_value.rs:72`), and each mode call clones the whole state and every value again as it commits. One `StateSchema { names: Names<StateField>, places: Vec<StatePlace> }`, built once for each unit type and for the mode; text kept shared, so a read counts a reference; the mode copies a field only when it is written. Check: no golden moves.

### F37. One contract of a call part, and one list by type

(`capabilities`: `scripts`, `units`.) `CallPart::param` is on the trait, and two of four parts return `None`; the split of params by role is written three times (`stats/stats_call.rs:70`, `mode/mode_call.rs:55`, `scripts/ctx.rs:112`); `ModeCall::apply` does nothing and the mode commits apart, before the effects apply (`mode/calls.rs:116-128`). Three lists keyed by type are built three ways: `CallParts` by the vtable's `type_id` (`scripts/call_part.rs:36`), `ViewColumns` by a `TypeId` vec (`units/view_column.rs:28`), `Effects` by `as_any().is`; and each of the eight columns writes the same four delegating methods.

```rust
pub(crate) trait CallPart {
    fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError>;
    /// Writes what the call changed, in the order `COMMITS` gives: before or after the effects.
    fn commit(&mut self, world: &mut World);
    const COMMITS: Commit;
}

/// The params of one scope of a call, which the frame holds by `ScriptRole::params`.
pub(crate) trait ParamSource {
    fn param(&self, name: &str) -> Option<Dynamic>;
}

/// Items of distinct types, each found by its type.
pub(crate) struct TypeSlots<T: ?Sized> {
    types: Vec<TypeId>,
    items: Vec<Box<T>>,
}
```

The columns' shared methods become a blanket impl over a `ViewColumn` that names its rows. Check: no golden moves.

### F38. What a call costs

(`capabilities`: `stats`, `scripts`.) `StatsCall::begin` computes every param of the call before the script reads one (`stats/stats_call.rs:35-66`), where design 08 says `ctx.p` resolves when it is read; mode params are kept as `NameTable<Dynamic>` and cloned on each read (`mode/mode_schema.rs:76`), an array deeply; `Ctx` is three `Rc`s where one serves. Params resolve on read, behind a mask of those computed; a mode param is read by reference; `Ctx` holds one `Rc`. The frame's copy of each player's resources stays as it is: a script call holds no world to read them from later, and the copy allocates nothing. Rhai boxes each custom-type value twice (`Box<Box<dyn Variant>>`), which a bench session the user starts measures before any change. Check: no golden moves.

### I1. The script view's copy

(`capabilities`; **Core**, and Foundations.) Each script batch reads every unit again, 14.2 % of a 3v3 match's time. Before the Zero Hour import multiplies the units. Its direction: a batch reads again only the rows whose parts changed since the last, by the change filter each column's `RowReads` already gives and its list of what it reads, `RowReads::components`; the columns' half of F37 lands with it. A bench session the user starts measures it.

## 10. Run conditions

`capabilities`, every system the batches above leave. A sweep, last of the refactorings, over what the schedule can hold at install.

### F39. What is fixed at install is a run condition

(`capabilities`.) "Return if the book is missing" is written by hand in systems (`actions/holds.rs:74-80`; `vision/seeing.rs:53`; `production/player_supply.rs:47`; `players/out_of_play.rs:44`), and `navigation/walking.rs:80-85` pairs two `Option`s of the map's pathing grid and planner with an `expect`; `pay_attack_toggles` returns at once on a client (`combat/attacks.rs:194`), though the install knows it predicts (`combat/mod.rs:121`); the mode's hooks check its hook set each tick (`mode/mode_hooks.rs:44`, `171`), which the install knows (`mode/mod.rs:207`). A declared book is always present while its owner is installed, empty until a mode loads, so a guard for a missing book goes, and an empty book gives no work; a structure the map may lack, as the pathing grid with its planner, is taken by `If<(Res<PathingGrid>, ResMut<RoutePlanner>)>`, Bevy 0.19's parameter that skips a system while it is missing; a client's schedule leaves `pay_attack_toggles` out; the mode schedules only the hooks it defines, and the clear of `LevelUps` becomes a system of its own. A system skipped for a missing parameter keeps its last run, so its first run after the book arrives sees every change since the build: each system is checked for that, and the goldens and the archetype shuffle test hold it.

## 11. Match records and replays

`runner`, `verifier`; the tests of `runner` and `net`. How a test builds a match, records it and replays its log, in one form each.

### H1. Replay

(`net`, `runner`.) The loop of "take the checkpoint again, stop at a tick, run alone" is in `SimServer::restore_match` (`net/src/sim_server/mod.rs:381`) and `SessionDir::abort` (`net/src/sim_server/session_dir/mod.rs:217`): `SessionWorld::replay_to(tick, on_boundary)` holds it.

### F40. One replay of a published log

(`verifier`, `runner`; the tests of `net` and `runner`.) The verifier's `Replay` (`verifier/src/replay/mod.rs:46-66`) checks each checkpoint's hash and the result; tests that cannot depend on the verifier rebuild it by hand, eight times (`net/tests/scenario.rs:161`, `bots.rs:85`, `checkpoints.rs:93`, `saves.rs:157`, `restore.rs:106`, `prototype.rs:151`; `runner/tests/match_3v3/mod.rs:571`, `resume.rs:259`), three of them repeating its check of checkpoints line for line. `Replay` moves into `runner`, on H1's `replay_to`, as `Replay::of(log, &ModePackages)`, with a trail of hashes in its internals; the verifier keeps `Verification` and its command line, and loses its one use of `capabilities`.

### F41. One record of a played match

(`runner`.) A test puts `CopyCheck::new`, `Golden::new` and `HashTrail` together by hand, and each tick calls `copy.check`, `golden.record` and `trail.record_hashes` (`runner/tests/proving_match.rs:211-226`, `348-365`; `lane_match.rs:66-68`; `match_3v3/mod.rs:164-181`). `MatchRecord { copy, trail, golden }`, with `of(&mut Match, packages, players)`, `tick(&mut Match) -> MatchHashes`, `check(name)` and `assert_replays(&log)`, on F40's replay; `CopyCheck::new` becomes `CopyCheck::of`, as a value made from the world is.

### F42. One match built with no mode started

(`runner`, `common`.) A zero seed, `MatchBuild::spec`, `Match::new` and `install_mode` are written three times (`runner/src/session/mod.rs:112-117`, `match_build/mod.rs:121-126`, `match_build/bench.rs:40-44`), and `MatchBuild` is `pub` only for the internals, with an `expect(unreachable_pub)`. A production `MatchBuild::restore_target(packages, rate, players)` serves all three, the internals add only the `LogCheck`, `SegmentSeed::ZERO` replaces the repeated zero seed, and the lint's suppression goes.

## 12. The session's host

`net`, `server`, `client`, `runner`. Its code stays in `net`, which hosts every session (design 02), and the operating system's clock and randomness travel as one value.

### H2. Server compositions

(`net`, `server`.) The task pool, time, states and `SimServer` setup appear three times (`server/src/main.rs`, `net/src/local/local_server/mod.rs`, `net/src/harness/in_process_match/mod.rs`): `SimServer::app(tick, &ModePackages) -> App` holds them, and the server's edges to `capabilities` and `sim`, each used once, go.

### H3. Restore glue

(`net`, `server`.) The server's `Restore` (`server/src/opening/mod.rs:37`) and net's `PendingRestore` (`net/src/local/local_server/mod.rs:102`) have the same fields and body: one type in `net`.

### H4. Client edge

(`client`, `net`.) `client` depends on `runner` for `SessionRules` alone (`client/src/main.rs:44`), a rate check that moves into `SimClient`'s setup.

### H5. Log events

(`net`.) The match's end is logged ad hoc on the server and on the client (`net/src/sim_client/mod.rs:558`), and the client logs a death ad hoc though `UnitDied` exists: a `MatchEnded` event, and `UnitDied` on both ends.

### F43. The clock and the entropy as one value

(`net`, `server`, `client`.) `clock: fn() -> u64` and `entropy: fn(&mut [u8; 32])` travel as two function pointers through some ten types (`SimClient`, `net/src/sim_client/mod.rs:104-106`; `ServerSetup`; `LocalServerSetup`, `local_server/mod.rs:123-125`; `Offering`, `JoinState`, `ServerSigner`, `KeyFile`, `SessionDir::abort`, `server/src/opening/mod.rs:119`). A function pointer holds no state, so the tests' settable clock lives in two `thread_local!`s (`net/src/harness/in_process_match/mod.rs:73-80`, `net/src/sim_client/join_state/tests.rs:16-22`), which the style rules refuse. `Os` (`net/src/os.rs`) becomes a value, `Os { clock: Clock, entropy: Entropy }`, where `Clock` is `System` or `Manual(Arc<AtomicU64>)`; every setup holds one `os: Os`, and the harness keeps the `Arc` as it keeps its pace. Both thread-locals go.

## 13. Test time

`package`, `runner`, and the code each test spends its time in. Before the matches they play grow again.

### I4. Tests past 1 s

(`package`, `runner`; **Package**, **Runner**.) Four tests run at or past the bound of 1 s for a test alone: `mode_package::every_flaw_of_a_package_fails_its_load_with_its_own_problem`, `resume::the_3v3_match_resumes_from_each_checkpoint_to_the_same_hashes`, and the proving match's `a_snapshot_of_drawn_values_restores_or_is_refused_and_never_panics` and `a_snapshot_restores_and_a_flawed_one_never_panics`. Find where each spends its time, and bring each within its bound, or into `source/.config/nextest.toml` with the reason it takes longer.

## 14. The shop

`capabilities`: `items`; `package`; `packages/test/moba`. Slice 2 starts here. A shop becomes a unit, then the shop's reader, an item's price and the items' names are built on it once, and the 3v3's golden moves once.

**Slice 2, wide bodies, uses, the shop and gravity,** batches 14 to 18. The user plays a 3v3 match of the MOBA test content: their hero holds a use on the jungle's shrine, buys items from the shop panel, and casts their actives with 1 to 6, a ward at the ground point; a test map holds a 1,117 m box that walkers go around. The design is [Wide bodies, uses, the shop and gravity](docs/design/13-bodies-uses-shop-gravity.md); each step folds its rules into the design 04 file of its capability.

**The capability model's redesign,** M1 to M10, a proposal for review: each step's decisions move into `docs/design/04-capabilities/` as it lands, and each generates the script API reference again. The core model is sound and generic: the action pipeline, the effect lists, stats and their formula, tags, relations and filters, the reach rule, deliveries as units and scripted systems serve every genre, and stay. The faults are at the edges, where a capability was built for its first genre: batches 4 to 12 and F44 take its couplings, and M1 to M10 open its closed sets. Each new section of a unit type an M step or U1 adds is built as the Direction gives it: a `DataSection`, its owner's record in `DataTable::UnitType`, its book, its part, `d.part`, and an `Effect` with its callback, `d.callback`, for each new effect.

### M1. Shops as units

(`capabilities`: `items`; `package`; `packages/test/moba`.) A unit type's `shop = { items, resource, reach, buyers }` makes a unit a shop, a part `items` declares: `reach` a radius by the reach rule, `buyers` a filter, which may select the dead, so League of Legends' rule is data. The mode's `[shop]` and its markers go; the 3v3's fountain is a unit with a `shop` section. Warcraft III's shops are units, as a courier or a market is. Check: the 3v3's golden moves by the new unit's id alone.

### S1. Names and readers

(`capabilities`, `package`, `packages/test/moba`; D8; **Package**.) `name`, a message id, on item types, actions, a loadout's spells, the mode, its teams and its choices, as only an avatar names its own today, and the MOBA's names; a public reader of the shop, an item's price for a unit, whether a unit stands where it can buy, and an action's targeting. A player's resources reach it on its player entity, `PlayerResources::of_player`. Check: a price less held components as computed by hand; an item type with no `name`, or one a locale lacks, fails its load.

## 15. The client's shop

`client`. I2 gives the view tests' units their types before S2 changes the client.

### I2. The client view tests' units

(`client`; **Client**.) They spawn with no unit type, which no client meets, and the view reads the type as optional. Before S2 changes the client.

### S2. The shop panel and item keys

(`client`; D9, D10, D11.) Bevy's UI features; the shop panel, the inventory bar, keys 1 to 6, and every cast key aimed as its action takes. Check: the key's target for an action of each targeting; the panel's dimmed items for an avatar alive out of a shop, too poor and with no room; the user buys a ward, places it at a point with its key, sells an item and swaps two slots.

## 16. Use and the graph

`capabilities`: `actions`, `abilities`, `items`, `interaction`, the release tests; `package`; `client`. F44 moves the group of an inventory's slots into `actions`, and U1 adds the `use` kind, whose actions block as `use` too: one batch settles which actions a `use` block stops. F44 comes after batches 4, 6 and 7, which leave only the true edges; M2 reads the code as F44's graph test does.

### F44. The capability graph in one table

(`capabilities`, `docs/design/04-capabilities/00-overview.md`.) The graph's needs and the install order are one list of modules, `Release::modules`; the overview's layers stay in prose. `each_capability_installs_after_the_lower_layers_it_builds_on` ties the first two to the layer table (`LAYERS`, `release/tests/mod.rs`); no test ties them to the imports. The layer test rejects imports from a higher layer only: within a layer, `units ↔ scripts` and `units ↔ players` in layer 1 and `abilities ↔ items` in layer 5 import each other (`items/mod.rs:6` imports `CastSpends`; four `abilities` files import `Inventory`).

A capability's edge to another is a need, without which it does not run, or a use, which it reads when the match has it, each read guarded by `cx.has`: production runs without navigation, with no gathering. The table holds both, and the test holds it symmetric: each import of another capability is a need or a use, and each need and use is imported. The order of the schedule keeps to the same edges: each set a module's sets order against, as F45's declarations record it, is the core's, the module's own, or of a need or a use.

```rust
// declare/module.rs: beside `CAPABILITY` and `NEEDS`
/// The capabilities it reads when the match has them, and runs without.
const USES: &'static [Capability];

// production/mod.rs
const USES: &'static [Capability] = &[Capability::Navigation, Capability::Vision];

// release/tests/mod.rs
#[test]
fn a_capability_imports_exactly_what_it_needs_or_uses() {
    for capability in Capability::ALL.into_iter().filter(|c| layer(c.name()).is_some()) {
        let mut found = Vec::new();
        imports(&src.join(capability.name()), capability.name(), &mut found);
        let imported: Vec<Capability> = found
            .iter()
            .filter_map(|(_, to)| Capability::ALL.into_iter().find(|c| c.name() == to))
            .filter(|&other| other != capability)
            .collect();
        let edges = Edges::of(capability); // a `ModuleVisitor` that collects `M::NEEDS` and `M::USES`
        for other in &imported {
            assert!(edges.contains(other), "{capability:?} imports {other:?}");
        }
        for edge in &edges {
            assert!(imported.contains(edge), "{capability:?} names {edge:?} and imports none");
        }
    }
}
```

The `abilities ↔ items` cycle goes with two moves. `CastSpends`, a registry of callbacks with no item in it, moves into `actions`. The slot kind of an inventory, the one fact `abilities` reads of it, moves out of `Inventory` into a state type of `actions`, which `items` puts on with the inventory. It is not U1's `uses = { kind }`, the slot that holds a use under way, whose action blocks as `use` by its own kind, so the two share batch 16 and not a name:

```rust
// actions/use_group.rs
/// The kind of slot whose actions the unit uses rather than casts: its inventory's.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct UseGroup(SlotKind);

impl UseGroup {
    /// The group a tag blocks of the action in a slot of `kind`, of a unit with `used`.
    pub(crate) fn group(used: Option<&UseGroup>, kind: SlotKind) -> Block {
        if used.is_some_and(|used| used.0 == kind) {
            Block::Use
        } else {
            Block::Cast
        }
    }
}

// abilities/mod.rs:74, was `Inventory::group(inventory, kind)`
let group = kind.map_or(Block::Cast, |kind| UseGroup::group(used, kind));
```

The layer test also refuses a cycle within a layer. The cycles of layer 1 wait in the issue log (**Core**), as each pair needs its own design. The overview's prose points to the table. Check: the test fails on today's `vision` row before F30; the state column moves for `UseGroup`, the behaviour column does not.

### U1. The `use` kind

(`capabilities`, `package`, `client`; D4, D5.) The `use` and `uses` sections and their load checks; the `use` order; `damage_interrupts`; `on_channel_tick`'s target; the script API reference generated again. Check: a use resolves on its object; a unit without `uses`, one that `users` refuses and one out of range that cannot walk drop the order; a held use is cut by a new order, a block of `use`, a death and, with `damage_interrupts`, by damage, not by a heal; a second use of the same object keeps its slot, and one of another object fills it afresh.

### M2. Genre words stay out of the engine

(`capabilities`: the release tests.) A test reads the crate's production code, as the role test does, and fails on an identifier that holds a word of a deny list as a whole word: `hero`, `creep`, `lane`, `tower`, `jungle`, `mineral`, `gold`, `mana`, `barracks`, and the words the list grows by; test modules may hold them. Today the production code holds none.

## 17. The shrine

`packages/test/moba`, `client`.

### U2. The shrine

(`packages/test/moba`, `client`; D6.) The shrine, its action, `shrine_blessing` and `shrine_spent` in design 07 and the 3v3 package; a right click on a unit the avatar may use orders the use; the channel bar. Check: the scripted match has a hero use the shrine and the team's heroes take the blessing, and a second hero's use cut by a hit; the user holds a use on the shrine in the client.

## 18. Gravity

`capabilities`: `projectiles`; `package`; `packages/test/moba`; `client`. Slice 2's last step: the user then plays slice 2.

### G1. Gravity

(`capabilities`, `package`, `packages/test/moba`; D12.) The falling flight; `gravity` past planned, refused with `homing` or not positive; a grenade of the MOBA's items, which arcs to its point. Check: the points of a flight on each metric at its launch, its peak and its end as computed by hand, the planar one back at its launch height at its end; a spatial one hits a unit its chord meets and not one its straight line would; the client draws the grenade's arc.

## 19. Queues and capacities

`capabilities`: `production`, `actions`; `package`; `docs/design/11`. Train, research and supply in one form, before the Zero Hour skirmish, so its rules package and golden are made once, on the general model.

### M3. Queued actions with any output

(`capabilities`: `production`, `actions`; `package`.) Today `train` spawns a unit type alone; research has no kind, so an RTS times it with a mode timer; `craft` is planned as a third kind of "consume, wait, produce". Any unit may have `queue = { size }`. An action with `queue = true` joins the queue in place of a windup: it pays at once, waits its time at the head, then resolves with its `on_resolve` effects and hook. `train` becomes such an action with a `spawn` effect, research one with a `player_modifier = { id }` effect, craft one with a `give_item` effect, planned with items; cancel and refund move to the queue. The `train` kind goes, and no `craft` kind comes. Today's train rules become the queue's: checked as it joins, its costs, requirements and capacities (M4); never under way, so its unit stays free; its time from the tick it reaches the head; it resolves in the Mode stage of the tick its time ends in, and the next entry starts in that tick; a dead producer's queue waits; a producer that despawns takes its queue and refunds nothing. A queued `spawn` places its unit by the train's rule, the producer's body point nearest its rally point and the nearest open cell, and sends it to the rally point; a `spawn` of any other action keeps its own rule. StarCraft II's `CAbilQueue` trains and researches through one queue. Check: production's train, cancel, supply and rally tests pass with their assertions unchanged, on data that states a train as a queued action.

### M4. Capacities

(`capabilities`: `production`; `package`.) One `[supply]` counter serves StarCraft alone. The mode declares `[capacities.<name>] max`; a unit type gives `capacity = { <name> = { uses, provides } }`. Each is derived as supply is now, a pass over the owned units and the queues when a tick's first check needs it, so it keeps no sum and never drifts (design 11, D5). A queued action or a build lists the capacities it checks. Supply becomes one capacity; power, population and housing are more. Scripts read `ctx.capacity_used(player, name)` and `ctx.capacity_max(player, name)`.

## 20. Work and haul

`capabilities`: `production`, `interaction`; `package`; `docs/design/13`. Construction and gather in their general form, after batch 19, whose queued actions a production chain uses.

### M5. One work mechanism

(`capabilities`: `production`, `interaction`; `package`; `docs/design/13`.) Construction's rate table by builders and interaction's shared progress are one mechanism, one native and one planned as script state. A unit type's `work = { rates = [...], max_workers }` makes a unit that others work on: `rates` gives the rate for each count of workers from zero, a count past the table takes its last entry, and `max_workers` refuses a worker past it. The three construction styles become three tables: `alone` `rates = [1]`; `builder` `rates = [0, 1]`, `max_workers = 1`; `builders` `rates = [0, 1, 1.62, 2.16]`. Progress, its rounding of life and its completion tick stay as construction defines them. Completion runs the type's `on_complete` effects and the mode's `on_work_complete(ctx, unit)` hook. A held use with one side's progress, plant, defuse, revive, repair, becomes work, and M5 then amends design 13's D5 and `interaction.md`, which keep shared progress as script state until it lands; slice 2's shrine has no progress, so neither waits on the other. A capture point, by team with contest and decay, is an extension planned with `interaction`. TF2's capture rate by the count of cappers is the precedent. Check: the three styles' construction tests pass, tick for tick.

### M6. Haul

(`capabilities`: `production`; `package`.) Gather fills only a player resource, from a node to a drop-off. A unit type's `stock = { <resource> = { amount, max } }` gives a unit its own amounts. `gather` becomes `haul`: `from` filters units with stock of the resource, a node being one with no `max`; `to` filters units that take it; `take` is the most one trip carries; `windup_ms` the time at the source. A sink's section names where it takes: `drop_off = { resources, into = "player" }` is today's drop-off, `into = "stock"` adds to its own stock within its `max`. The loop, its wait, its `bounce` and its tie rules stay. A production chain is a queued action (M3) with a `stock_cost` from its producer's own stock and a `stock = { resource, amount }` effect: a smelter turns ore into bars. A cost from stock is its own field, as `cost` names pools and player resources, and a stock's resource shares a player resource's name. RimWorld's haul jobs and The Settlers' carriers move goods this way.

The snapshot tests sweep the new state types of M3 to M6 as they sweep the skirmish's (design 11, D14).

## 21. The genre proofs

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

## 22. The network

`quic`, `net`. After the issue log's **Quic** item on the endpoint's benches, which sets N1's cap.

### N1. Limits for each address

(`quic`, `net`; **Quic**.) No limit holds live connections or new handshakes for each address, where design 15's D13 bounds both by the session's seats for each IPv4 address or IPv6 /64, and `net` gives the endpoint no seat count ([QUIC transport](docs/design/15-quic-transport.md), D13). After the bench of the endpoint, the issue log's **Quic** item on benches, sets the cap of new connections on the same path.

### N2. UPnP behind a host firewall

(`quic`; **Quic**.) `igd-next` discovers a router by search alone, and the router's `NOTIFY` announcements, which a host firewall that tracks connections lets in, go unread ([QUIC transport](docs/design/15-quic-transport.md), D12).

Then the network's Research items of the issue log as they show; then P1 to P4, P3 only as D14 of design 15 lets it.

## 23. Planned

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

- **The script view copies the whole match** (Core; I1). Each script batch reads every unit again, though few change between two batches of a tick: 14.2 % of a 3v3 match's time, a cost that grows with the units times the batches, which the Zero Hour import multiplies. Items: Core, "The script view copies the whole match".
- **The import's materials are glTF's** (Zero Hour). glTF's material model holds no W3D pass past the first, no ambient color and no fallback texture, so the presentation loses each, and each later fidelity gap of a W3D material meets the same limit. Items: Zero Hour, "The import writes W3D's materials into glTF's material model"; Zero Hour, "A texture no archive holds"; Zero Hour, "The import ignores a vertex material's ambient color"; Zero Hour, "The import draws a mesh's first material pass".
- **The import has no object that only the client shows** (Zero Hour). Every object a map places becomes a unit of the sim, where the game spawns props, optimized trees and fluff on the client alone and some objects not at all, so the sim, its log and its state hash hold objects the original never simulates, and the parity oracle meets them. Items: Zero Hour, "The import makes a unit of every object a map places".
