# Plan

The one plan of the project: every open step, in the order it is done, in batches. A batch is one commit: it holds the steps that change the same systems, refactorings, fixes and features alike, and ends with the check chain passing for the crates it touches, and a stop for review. Each batch says what it touches and why it stands where it does. Open items only: a step goes when it is done. The issue log, `.notes/ISSUES.md`, holds every open problem; a problem that a step here takes carries the step's tag there ([Issue log](AGENTS.md#issue-log)). The designs in `docs/design/` stay the source of truth: a step names what the code lacks against its design, or the design it proposes, which moves into `docs/design/` when it is agreed.

One policy holds across the steps: **what can be forgotten is a type or a trait method with no default.** A rule that no type can hold is held by a type that makes the right way the short one, a test where one can see the breach, and review. Line numbers point to the code at commit `6b1b6dea`.

## 1. Slice 3's play

Its steps are done ([Replication audiences](docs/design/14-replication-audiences.md)); it ends as the user plays it in the client ([Workflow](AGENTS.md#workflow)).

## 2. The test guards

`store`, `runner`, `capabilities`, `package`, `protocol`, each in its tests. First, as every batch after it is checked by goldens that must not move, and today a run with `CAMPFIRE_BLESS=0` writes them again.

### F1. The goldens bless only on purpose

(`store`, `runner`, `capabilities`, `package`, `protocol`; issue log, **Runner**.) The tests that pin a file write it again when `CAMPFIRE_BLESS` has any value, `0` and the empty text included, each by its own reading of the variable: the runner's goldens (`runner/src/harness/golden.rs:144`) and layouts (`runner/tests/snapshot_layout.rs:66`), the script API reference (`capabilities/src/scripts/script_api/tests.rs:310`), the package directory's (`package/src/package_dir/tests.rs:368`) and the session log's format (`protocol/src/session_log/tests/format.rs:57`). Each writes through `store`, the proxy of files, so `store`'s internals gain the pinned file, one rule for all five: it compares the file with what the test made, and writes it only under `CAMPFIRE_BLESS=1`.

```rust
// before, at each of the five places
if env::var_os("CAMPFIRE_BLESS").is_some() {
    DurableFile::write(&path, &now).unwrap();
}

// store/src/pinned_file.rs, behind `internals`
/// A file a test pins: it holds what the test makes, or the test fails, and
/// `CAMPFIRE_BLESS=1` alone writes it again.
#[derive(Debug)]
pub struct PinnedFile<'a> {
    pub path: &'a Path,
    /// The bound of a read of it.
    pub limit: usize,
}

impl PinnedFile<'_> {
    /// Whether the file holds `now`: `Ok` when it does or was written again, else what it holds.
    pub fn check(&self, now: &[u8]) -> Result<(), Pinned> { /* … */ }
}
```

Check: a run with `CAMPFIRE_BLESS=0`, and one with it empty, change no pinned file and fail on a difference.

### F2. One hero lookup in the tests

(`runner`.) `MatchUnits::hero` finds a player's hero, and the proving match repeats its filter (`runner/tests/proving_match.rs:76`):

```rust
// after
fn hero(world: &World, slot: u32) -> (StableId, EntityRef<'_>) {
    let id = MatchUnits::of(world).hero(slot);
    (id, world.entity_ref_of(id).unwrap())
}
```

## 3. Units by their kind

`capabilities`: `units`, the declaration audit. Both steps change what the script view reads and what the declaration audit holds of it; the MOBA arena's units now spawn with types, so no unit of a match falls out of the view. No golden moves.

### F3. The script view reads units by their kind

(`capabilities`.) `UnitRows` takes any entity of the entity index with a position and a team for a unit, where every other reader of units filters by the `UnitEntity` kind:

```rust
// units/unit_rows.rs:62
type CoreParts = (
    Option<&'static Position>,
    Option<&'static Team>,
    Option<&'static Body>,
    Option<&'static SpawnPoint>,
    Option<&'static UnitType>,
    Option<&'static Owner>,
    Option<&'static UnitTags>,
);

// units/unit_rows.rs:358
let parts = self
    .parts
    .get_manual(world, entity)
    .expect("the core reads optional parts");
let (Some(&pos), Some(&team), body, spawn, unit_type, owner, tags) = parts else {
    return None;
};
```

The core's query takes the kind's filter, and the parts every unit holds become required, its type among them, as every unit of a match spawns with one:

```rust
type CoreParts = (
    &'static Position,
    &'static Team,
    &'static UnitType,
    Option<&'static Body>,
    Option<&'static SpawnPoint>,
    Option<&'static Owner>,
    Option<&'static UnitTags>,
);

struct CoreSource {
    parts: QueryState<CoreParts, UnitFilter>,
    changed: SystemState<
        Query<'static, 'static, Entity, (UnitFilter, <CoreParts as RowParts>::Changed)>,
    >,
}

let (&pos, &team, &unit_type, body, spawn, owner, tags) =
    self.parts.get_manual(world, entity).ok()?;
```

With it, the declaration audit (`release/tests/declaration_audit.rs`) gains the rule that holds what the view takes as given: on the proving match, which holds every capability, each entity with a stable id is of exactly one kind that `Release::kinds` visits.

```rust
/// The entities of each kind `Release::kinds` visits, each kind's own alone.
struct KindMembers<'w> {
    world: &'w mut World,
    members: Vec<Entity>,
}

impl KindVisitor for KindMembers<'_> {
    fn kind<K: EntityKind>(&mut self) {
        let mut only = self.world.query_filtered::<Entity, (With<K>, K::Only)>();
        self.members.extend(only.iter(self.world));
    }
}

let mut kinds = KindMembers { world, members: Vec::new() };
Release::kinds(&mut kinds);
let KindMembers { world, members } = kinds;
for (id, entity) in world.resource::<EntityIndex>().iter() {
    let count = members.iter().filter(|&&member| member == entity).count();
    assert_eq!(count, 1, "{id:?} is of one declared kind");
}
```

Check: the rule fails on a proving match that spawns a unit of no type; no golden moves.

### F4. A column reads declared types

(`capabilities`.) A column's `RowParts` may read a component that no module declares as state or as derived, which no snapshot, restore or client holds, so a restored match's scripts would read another value. `RowParts` lists the components it reads, and the declaration audit records the derived types it now drops:

```rust
// units/row_parts.rs
pub(crate) trait RowParts: ReadOnlyQueryData + 'static {
    type Changed: QueryFilter + 'static;

    /// Each component it reads, by type and name, for the declaration audit.
    fn components(into: &mut Vec<(TypeId, &'static str)>);
}

impl<T: Component> RowParts for &'static T {
    type Changed = Changed<T>;

    fn components(into: &mut Vec<(TypeId, &'static str)>) {
        into.push((TypeId::of::<T>(), type_name::<T>()));
    }
}

// release/tests/declaration_audit.rs
fn derived<C: Component>(&mut self) {
    self.derived.push(TypeId::of::<C>());
}

fn column<C: ViewColumn + Default, D: RowParts>(&mut self, _: ColumnFill<D, C>) {
    self.columns.push(Named::of::<C>());
    D::components(&mut self.column_reads);
}

// in `problems`
for &(id, name) in &self.column_reads {
    if self.state_of(id).is_none() && !self.derived.contains(&id) {
        problems.push(format!("a column reads {name}, not declared"));
    }
}
```

Check: `each_rule_names_the_declaration_that_breaks_it` gains a column that reads an undeclared type.

## 4. Spawn and teleport

`capabilities`: `vision`, `mode`'s kit, `books`, `deliveries`, `navigation`, `projectiles`. Both steps change what a unit takes as it spawns or moves at once, and how a delivery learns of it; F6 takes `navigation`'s import of `projectiles` out of F12's table. No golden moves.

### F5. One book of sights

(`capabilities`.) A mode's unit takes its sight from its kit, and a delivery from `ByType<Sight>`, which the books fill for the delivery types alone; both read the same `vision` section of a unit type's file:

```rust
// mode/unit_kit/mod.rs:155, put on by kit_part.rs:46
sight: vision.map(|vision| vision.sight),

// books/book_builder.rs:712, in the build of the delivery types only
books.sights.set(unit_type, vision.sight);

// deliveries/delivery_spawner.rs:67
if let Some(&sight) = self.sights.as_deref().and_then(|book| book.get(unit_type)) {
```

The books fill `ByType<Sight>` for every unit type with a `vision` section, `UnitKit` loses `sight`, and both spawns read the book through one function:

```rust
// vision/sight.rs
impl Sight {
    /// The sight of a unit of `unit_type`, from its type's `vision` section; none in a match
    /// with no vision, or for a type with no section.
    pub(crate) fn of(world: &World, unit_type: UnitType) -> Option<Sight> {
        world.get_resource::<ByType<Sight>>()?.get(unit_type).copied()
    }
}
```

Check: no golden moves.

### F6. Navigation reads the entity it holds, and announces a teleport

(`capabilities`.) `NavigationEffect::put` builds a `QueryState` to read one entity, which allocates, and `make_room` calls it once for each walker its box overlaps. `teleport` builds another and scans every projectile, which is why `navigation` imports `projectiles`:

```rust
// navigation/navigation_effect.rs:155
let mut walkers = world.query::<(&Destination, &mut Route, &mut Progress)>();
if let Ok((destination, mut route, mut progress)) = walkers.get_mut(world, entity) {
    route.ask_again(destination, &mut progress, now);
}

// navigation/navigation_effect.rs:237
let mut projectiles = world.query::<(&Projectile, &mut FlightState)>();
for (projectile, mut state) in projectiles.iter_mut(world) {
    if projectile.homes_on(id) {
        state.disjoint();
    }
}
```

`put` reads the entity it holds, as `abilities/casts.rs:398` does. A teleport triggers an event of the core's `deliveries`, which owns what a delivery aims at, and `projectiles` observes it: an observer keeps its query state, and neither capability names the other.

```rust
// navigation/navigation_effect.rs
let mut moved = world.entity_mut(entity);
*moved.get_mut::<Position>().expect("a unit has a place") = pos;
moved.remove::<ForcedMove>();
if let Ok((destination, mut route, mut progress)) =
    moved.get_components_mut::<(&Destination, &mut Route, &mut Progress)>()
{
    route.ask_again(destination, &mut progress, now);
}

// in `teleport`
NavigationEffect::put(world, entity, pos, now);
world.trigger(Teleported { id });

// deliveries/teleported.rs
/// A unit put elsewhere at once, which a delivery that homes on it loses.
#[derive(Event, Debug, Clone, Copy)]
pub(crate) struct Teleported {
    pub(crate) id: StableId,
}

// projectiles/mod.rs, in `install`
world.add_observer(
    |teleported: On<'_, '_, Teleported>,
     mut projectiles: Query<'_, '_, (&Projectile, &mut FlightState)>| {
        for (projectile, mut state) in &mut projectiles {
            if projectile.homes_on(teleported.id) {
                state.disjoint();
            }
        }
    },
);
```

Check: the teleport tests pass unchanged; no golden moves.

## 5. A unit's derived parts

`capabilities`: `units`' tags, `declare`, `stats`' refresh. `Refresh::give_parts` gives a new unit both its tags and its stats: F7 moves the tags' places to declarations, and I1 fixes the stats it gives a unit with no level, before batches 17 and 18 add kinds of unit with pools.

### F7. Engine tags by declaration

(`capabilities`.) `units/engine_tag.rs` fixes `Avatar`, `Projectile`, `Area`, `Constructing`, `Node`, `DropOff` and `Gathering` as the first tags of every match, so each new capability adds to a list of the core, and the overview says the core names no capability. Each module declares its engine tags, with their properties, through `Declares`, as it declares its state types; the tag book gives them their places in declaration order, which is install order. The core keeps `avatar` and the layer tags. The load still refuses a package that gives an engine tag.

```rust
// declare/declarations.rs
/// `tags`, the module's engine tags, which the tag book places after the ones declared
/// before them.
fn engine_tags(&mut self, tags: &'static [EngineTagSpec]);

// production/mod.rs, in `declare`
d.engine_tags(&ProductionTag::SPECS);
```

Check: the tags keep their numbers while the install order stays; no golden moves.

### I1. A unit with pools and no level

(`capabilities`; **Stats**.) `Refresh::regenerate` panics on a unit of a type with a pool and no `Level` (`stats/refresh.rs:277`). Find whether a client can hold a unit's type and pools before its `Level`, and make the empty `UnitStats` that `Refresh::give_parts` gives either filled or never indexed. Before M3 to M6, which add pools to new kinds of unit.

## 6. The action pipeline's seams

`capabilities`: `actions`, `combat`, `projectiles`, `stats`, `items`, `abilities`. Both steps change what `actions` owns: its sub-stages, its delivery requests, and the one pass that grants a source's modifiers. No golden moves.

### F8. Action anchors and the delivery seam

(`capabilities`.) Most capabilities need `combat` only to order their systems against `CombatSet`: `vision` imports nothing of `combat` and needs it; `areas` and `items` order against it alone. `projectiles` reads `combat::shots::Shots` (`projectiles/launching.rs`) and pushes damage to `combat::pass_queue::PassQueue` (`projectiles/flights.rs`, `projectiles/flying.rs`), and `abilities` reads the life pool through `CombatWorld::life_pool`, though `LifePool` is `stats`'.

- `actions` owns the sub-stages that every action kind and delivery orders against: `Start`, `Deliver`, `Launch`, `Land`, `Resolve`. `CombatSet` keeps combat's own steps: the damage pass, deaths and respawn.
- A weapon's shot is a delivery request in `actions`, and its hit resolves through a hook that `combat` registers as it installs, as `EffectQueues` registers each capability's effects: `projectiles` names no type of `combat`.
- `life_pool` moves from `CombatWorld` to `stats`' world trait.

Each capability's needs then are what its code needs: `vision` the core; `areas` and `abilities` `stats`, whose pools and modifiers they change; `projectiles` the core; `orders` `navigation`. F12 states them. Check: no golden moves; the archetype shuffle test and the schedule build's check of conflicts pass.

### F9. One grant mechanism

(`capabilities`.) Three loops keep the modifiers a source wants on a unit: `Holds::hold_passives` (`actions/holds.rs`), `ItemHolds::hold_items` (`items/item_holds.rs`) and `HeldPass` (`stats/held_pass.rs`); the first two are one loop, and perks and equipment are planned as two more sources. A capability registers a grant source as it installs, as `EffectQueues` registers effects, as its types lie above `declare`'s layer: it answers which `(modifier, rank, applier)` a unit wants, and whether that changed since the last pass. `stats` owns one pass that compares each changed unit's wanted set with its held set, and applies or releases. The sources are action passives and holds, carried items and the unit type's passive. Presence grants, auras, areas' insides and player modifiers, stay in `HeldPass`, as space finds them each tick.

```rust
// stats/grant_sources.rs
/// What a source wants on the unit: each modifier, at its rank, from its applier.
pub(crate) type Wanted = fn(EntityRef<'_>, &mut Vec<Grant>);

/// The grant sources of the match, which each capability registers as it installs. Not state.
#[derive(Resource, Debug, Default)]
pub(crate) struct GrantSources(Vec<Wanted>);
```

The pass runs where a source's answer changes today: as each tick starts, after Hit's casts and attacks, after Resolve, and at the tick's end; each run visits only the units whose source or modifiers changed, and those `ReadyWaits` keeps. Two sources that grant one modifier from one source unit share one hold, as two items do; the load still refuses a modifier that two kinds of source grant. Check: no golden moves.

## 7. Commands to their owners

`capabilities`: `orders`, `items`, `production`, `progression`; `client`; the session log's format version. After batch 6, as both change what `orders` orders against.

### F10. Each capability owns its commands

(`capabilities`, `client`.) `Action` (`orders/order/mod.rs`) holds `Learn`, `Buy`, `Sell`, `Swap`, `CancelTrain`, `Rally`, `Build` and `CancelBuild`, and `Orders::install` schedules `ProductionOrders` and `TradeOrders`: a mode that drives its units with no `orders` cannot buy, learn, cancel or rally, and the overview says each command names the capability that owns it.

- `items` decodes `Buy`, `Sell` and `Swap`; `progression` `Learn`; `production` `CancelTrain`, `Rally` and `CancelBuild`.
- `orders` keeps the orders a unit carries out, move, attack, slot, build and stop, and the shared parts, the unit list's decoding and the check that the player controls the units, which each capability calls.
- Each capability's commands apply in Inputs in the capabilities' table order, so a tick's buys and cancels apply as now.

The commands' bytes in a session log change, so the log format's version rises. Check: the goldens' state and behaviour columns do not move.

## 8. Load, then trust

`package`, `capabilities`' books. Before batches 10, 17 and 18 write new builders, so they are written once in the checked form.

### F11. Load, then trust

(`capabilities`, `package`.) The builders `expect` what `campfire-package/src/load_check.rs` (2,288 lines) checked: 40 `expect(CHECKED)` in `books/book_builder.rs`, and 28 `expect("the load …")` in 21 other files. The two copies of each rule can drift: a check the load loses becomes a panic at the build.

```rust
// stats/pool_book.rs:31
let index = |stat| book.named(stat).expect("the load checked the pools' stats");

// load_check.rs:1379, the same rule
self.stats_declared(iter::once(&pool.max).chain(&pool.regen), &at)?;
```

A builder that resolves a name is the check of that name: it returns the case that fails, and the load check drops its copy. The book build already returns `BookError`:

```rust
// stats/pool_book.rs
pub(crate) fn new(
    pools: &BTreeMap<DeclaredName, PoolData>,
    book: &StatBook,
) -> Result<PoolBook, BookError> {
    let index = |pool: &DeclaredName, stat: &Stat| {
        book.named(stat).ok_or_else(|| BookError::PoolStat {
            pool: pool.clone(),
            stat: stat.clone(),
        })
    };
    let stats = pools
        .iter()
        .map(|(name, pool)| {
            Ok(PoolStats {
                max: index(name, &pool.max)?,
                regen: pool.regen.as_ref().map(|regen| index(name, regen)).transpose()?,
            })
        })
        .collect::<Result<Vec<_>, BookError>>()?;
    Ok(PoolBook {
        names: pools.keys().cloned().collect(),
        pools: stats,
    })
}
```

A rule between packages, which no single builder sees, stays in the load check. An `expect` that a capability is installed, as `EffectQueues::of` holds, is a rule of the match, not of the data, and stays. Check: `every_flaw_of_a_package_fails_its_load_with_its_own_problem` passes with each flaw's problem, now from the builder where it moved.

## 9. The session's host

`net`, `server`, `client`, `runner`. Its code stays in `net`, which hosts every session (design 02); independent of the batches around it, here as the refactorings end.

### H1. Server compositions

(`net`, `server`.) The task pool, time, states and `SimServer` setup appear three times (`server/src/main.rs`, `net/src/local/local_server/mod.rs`, `net/src/harness/in_process_match/mod.rs`): `SimServer::app(tick, &ModePackages) -> App` holds them, and the server's edges to `capabilities` and `sim`, each used once, go.

### H2. Restore glue

(`net`, `server`.) The server's `Restore` (`server/src/opening/mod.rs:37`) and net's `PendingRestore` (`net/src/local/local_server/mod.rs:102`) have the same fields and body: one type in `net`.

### H3. Replay

(`net`, `runner`.) The loop of "take the checkpoint again, stop at a tick, run alone" is in `SimServer::restore_match` (`net/src/sim_server/mod.rs:381`) and `SessionDir::abort` (`net/src/sim_server/session_dir/mod.rs:217`): `SessionWorld::replay_to(tick, on_boundary)` holds it.

### H4. Client edge

(`client`, `net`.) `client` depends on `runner` for `SessionRules` alone (`client/src/main.rs:44`), a rate check that moves into `SimClient`'s setup.

### H5. Log events

(`net`.) The match's end is logged ad hoc on the server and on the client (`net/src/sim_client/mod.rs:558`), and the client logs a death ad hoc though `UnitDied` exists: a `MatchEnded` event, and `UnitDied` on both ends.

## 10. The shop

`capabilities`: `items`; `package`; `packages/test/moba`. Slice 2 starts here. A shop becomes a unit, then the shop's reader, an item's price and the items' names are built on it once, and the 3v3's golden moves once.

**Slice 2, wide bodies, uses, the shop and gravity**, batches 10 to 14. The user plays a 3v3 match of the MOBA test content: their hero holds a use on the jungle's shrine, buys items from the shop panel, and casts their actives with 1 to 6, a ward at the ground point; a test map holds a 1,117 m box that walkers go around. The design is [Wide bodies, uses, the shop and gravity](docs/design/13-bodies-uses-shop-gravity.md); each step folds its rules into the design 04 file of its capability.

The steps M1 to M10 are a proposal for review: each one's decisions move into `docs/design/04-capabilities/` as it lands, and each generates the script API reference again. The core model is sound and generic: the action pipeline, the effect lists, stats and their formula, tags, relations and filters, the reach rule, deliveries as units and scripted systems serve every genre, and stay. The faults are at the edges, where a capability was built for its first genre: F6 to F10 and F12 take its couplings, and M1 to M10 open its closed sets.

### M1. Shops as units

(`capabilities`: `items`; `package`; `packages/test/moba`.) A unit type's `shop = { items, resource, reach, buyers }` makes a unit a shop: `reach` a radius by the reach rule, `buyers` a filter, which may select the dead, so League of Legends' rule is data. The mode's `[shop]` and its markers go; the 3v3's fountain is a unit with a `shop` section. Warcraft III's shops are units, as a courier or a market is. Check: the 3v3's golden moves by the new unit's id alone.

### S1. Names and readers

(`capabilities`, `package`, `packages/test/moba`; D8.) `name` on item types and the MOBA's names; a public reader of the shop, an item's price for a unit, whether a unit stands where it can buy, and an action's targeting. A player's resources reach it on its player entity, `PlayerResources::of_player`. Check: a price less held components as computed by hand; an item type with no `name`, or one a locale lacks, fails its load.

## 11. The client's shop

`client`. I2 gives the view tests' units their types before S2 changes the client.

### I2. The client view tests' units

(`client`; **Client**.) They spawn with no unit type, which no client meets, and the view reads the type as optional. Before S2 changes the client.

### S2. The shop panel and item keys

(`client`; D9, D10, D11.) Bevy's UI features; the shop panel, the inventory bar, keys 1 to 6, and every cast key aimed as its action takes. Check: the key's target for an action of each targeting; the panel's dimmed items for an avatar alive out of a shop, too poor and with no room; the user buys a ward, places it at a point with its key, sells an item and swaps two slots.

## 12. Use and the graph

`capabilities`: `actions`, `abilities`, `items`, `interaction`, the release tests; `package`; `client`. F12 moves the group of an inventory's slots into `actions`, and U1 adds the `use` kind, whose actions block as `use` too: one batch settles which actions a `use` block stops. F12 comes after batches 4 to 7, which leave only the true edges; M2 reads the code as F12's graph test does.

### F12. The capability graph in one table

(`capabilities`, `docs/design/04-capabilities/00-overview.md`.) Three statements of one graph exist: `CapabilitySet::needs` (`declare/capability_set.rs`), the install order `CAPABILITIES` (`release/mod.rs`), and the overview's layers in prose. `each_capability_installs_after_the_lower_layers_it_builds_on` ties the first two to the layer table (`LAYERS`, `release/tests/mod.rs`); no test ties them to the imports. The layer test rejects imports from a higher layer only: within a layer, `units ↔ scripts` and `units ↔ players` in layer 1 and `abilities ↔ items` in layer 5 import each other (`items/mod.rs:6` imports `CastSpends`; four `abilities` files import `Inventory`).

A capability's edge to another is a need, without which it does not run, or a use, which it reads when the match has it, each read guarded by `cx.has`: production runs without navigation, with no gathering. The table holds both, and the test holds it symmetric: each import of another capability is a need or a use, and each need and use is imported.

```rust
// declare/capability_set.rs
/// The capabilities `capability` reads when the match has them, and runs without.
pub(crate) const fn uses(capability: Capability) -> &'static [Capability] {
    match capability {
        Capability::Production => &[Capability::Navigation, Capability::Vision],
        // … each row as F6 to F10 leave it
    }
}

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
        let edges = [CapabilitySet::needs(capability), CapabilitySet::uses(capability)].concat();
        for other in &imported {
            assert!(edges.contains(other), "{capability:?} imports {other:?}");
        }
        for edge in &edges {
            assert!(imported.contains(edge), "{capability:?} names {edge:?} and imports none");
        }
    }
}
```

The `abilities ↔ items` cycle goes with two moves. `CastSpends`, a registry of hooks with no item in it, moves into `actions`. The slot kind of an inventory, the one fact `abilities` reads of it, moves out of `Inventory` into a state type of `actions`, which `items` puts on with the inventory. It is not U1's `uses = { kind }`, the slot that holds a use under way, whose action blocks as `use` by its own kind, so the two share batch 12 and not a name:

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

The layer test also refuses a cycle within a layer. The cycles of layer 1 wait in the issue log (**Core**), as each pair needs its own design. The overview's prose points to the table. Check: the test fails on today's `vision` row before F8; the state column moves for `UseGroup`, the behaviour column does not.

### U1. The `use` kind

(`capabilities`, `package`, `client`; D4, D5.) The `use` and `uses` sections and their load checks; the `use` order; `damage_interrupts`; `on_channel_tick`'s target; the script API reference generated again. Check: a use resolves on its object; a unit without `uses`, one that `users` refuses and one out of range that cannot walk drop the order; a held use is cut by a new order, a block of `use`, a death and, with `damage_interrupts`, by damage, not by a heal; a second use of the same object keeps its slot, and one of another object fills it afresh.

### M2. Genre words stay out of the engine

(`capabilities`: the release tests.) A test reads the crate's production code, as the role test does, and fails on an identifier that holds a word of a deny list as a whole word: `hero`, `creep`, `lane`, `tower`, `jungle`, `mineral`, `gold`, `mana`, `barracks`, and the words the list grows by; test modules may hold them. Today the production code holds none.

## 13. The shrine

`packages/test/moba`, `client`. I3 fixes the 3v3's `hero_died` before U2 adds the shrine to the scripted match, whose golden would pin the failure.

### I3. Assist gold to a unit no player owns

(`packages/test/moba`; **MOBA test content**.) The 3v3's `hero_died` fails for an assister that no player owns, a tower, and the kill then gives no gold at all. Before U2 adds to the scripted match, whose golden would pin the failure.

### U2. The shrine

(`packages/test/moba`, `client`; D6.) The shrine, its action, `shrine_blessing` and `shrine_spent` in design 07 and the 3v3 package; a right click on a unit the avatar may use orders the use; the channel bar. Check: the scripted match has a hero use the shrine and the team's heroes take the blessing, and a second hero's use cut by a hit; the user holds a use on the shrine in the client.

## 14. Gravity

`capabilities`: `projectiles`; `package`; `packages/test/moba`; `client`. Slice 2's last step: the user then plays slice 2.

### G1. Gravity

(`capabilities`, `package`, `packages/test/moba`; D12.) The falling flight; `gravity` past planned, refused with `homing` or not positive; a grenade of the MOBA's items, which arcs to its point. Check: the points of a flight on each metric at its launch, its peak and its end as computed by hand, the planar one back at its launch height at its end; a spatial one hits a unit its chord meets and not one its straight line would; the client draws the grenade's arc.

## 15. Test time

`package`, `runner`, and the code each test spends its time in. Before the matches they play grow again.

### I4. Tests past 1 s

(`package`, `runner`; **Package**, **Runner**.) Four tests run at or past the bound of 1 s for a test alone: `mode_package::every_flaw_of_a_package_fails_its_load_with_its_own_problem`, `resume::the_3v3_match_resumes_from_each_checkpoint_to_the_same_hashes`, and the proving match's `a_snapshot_of_drawn_values_restores_or_is_refused_and_never_panics` and `a_snapshot_restores_and_a_flawed_one_never_panics`. Find where each spends its time, and bring each within its bound, or into `source/.config/nextest.toml` with the reason it takes longer.

## 16. The script view's copy

`capabilities`: `units`, `scripts`. Before the Zero Hour import multiplies the units it copies.

### I5. The script view's copy

(`capabilities`; **Core**, and Foundations.) Each script batch reads every unit again, 14.2 % of a 3v3 match's time. Before the Zero Hour import multiplies the units.

## 17. Queues and capacities

`capabilities`: `production`, `actions`; `package`; `docs/design/11`. Train, research and supply in one form, before the Zero Hour skirmish, so its rules package and golden are made once, on the general model.

### M3. Queued actions with any output

(`capabilities`: `production`, `actions`; `package`.) Today `train` spawns a unit type alone; research has no kind, so an RTS times it with a mode timer; `craft` is planned as a third kind of "consume, wait, produce". Any unit may have `queue = { size }`. An action with `queue = true` joins the queue in place of a windup: it pays at once, waits its time at the head, then resolves with its `on_resolve` effects and hook. `train` becomes such an action with a `spawn` effect, research one with a `player_modifier = { id }` effect, craft one with a `give_item` effect, planned with items; cancel and refund move to the queue. The `train` kind goes, and no `craft` kind comes. Today's train rules become the queue's: checked as it joins, its costs, requirements and capacities (M4); never under way, so its unit stays free; its time from the tick it reaches the head; it resolves in the Mode stage of the tick its time ends in, and the next entry starts in that tick; a dead producer's queue waits; a producer that despawns takes its queue and refunds nothing. A queued `spawn` places its unit by the train's rule, the producer's body point nearest its rally point and the nearest open cell, and sends it to the rally point; a `spawn` of any other action keeps its own rule. StarCraft II's `CAbilQueue` trains and researches through one queue. Check: production's train, cancel, supply and rally tests pass with their assertions unchanged, on data that states a train as a queued action.

### M4. Capacities

(`capabilities`: `production`; `package`.) One `[supply]` counter serves StarCraft alone. The mode declares `[capacities.<name>] max`; a unit type gives `capacity = { <name> = { uses, provides } }`. Each is derived as supply is now, a pass over the owned units and the queues when a tick's first check needs it, so it keeps no sum and never drifts (design 11, D5). A queued action or a build lists the capacities it checks. Supply becomes one capacity; power, population and housing are more. Scripts read `ctx.capacity_used(player, name)` and `ctx.capacity_max(player, name)`.

## 18. Work and haul

`capabilities`: `production`, `interaction`; `package`; `docs/design/13`. Construction and gather in their general form, after batch 17, whose queued actions a production chain uses.

### M5. One work mechanism

(`capabilities`: `production`, `interaction`; `package`; `docs/design/13`.) Construction's rate table by builders and interaction's shared progress are one mechanism, one native and one planned as script state. A unit type's `work = { rates = [...], max_workers }` makes a unit that others work on: `rates` gives the rate for each count of workers from zero, a count past the table takes its last entry, and `max_workers` refuses a worker past it. The three construction styles become three tables: `alone` `rates = [1]`; `builder` `rates = [0, 1]`, `max_workers = 1`; `builders` `rates = [0, 1, 1.62, 2.16]`. Progress, its rounding of life and its completion tick stay as construction defines them. Completion runs the type's `on_complete` effects and the mode's `on_work_complete(ctx, unit)` hook. A held use with one side's progress, plant, defuse, revive, repair, becomes work, and M5 then amends design 13's D5 and `interaction.md`, which keep shared progress as script state until it lands; slice 2's shrine has no progress, so neither waits on the other. A capture point, by team with contest and decay, is an extension planned with `interaction`. TF2's capture rate by the count of cappers is the precedent. Check: the three styles' construction tests pass, tick for tick.

### M6. Haul

(`capabilities`: `production`; `package`.) Gather fills only a player resource, from a node to a drop-off. A unit type's `stock = { <resource> = { amount, max } }` gives a unit its own amounts. `gather` becomes `haul`: `from` filters units with stock of the resource, a node being one with no `max`; `to` filters units that take it; `take` is the most one trip carries; `windup_ms` the time at the source. A sink's section names where it takes: `drop_off = { resources, into = "player" }` is today's drop-off, `into = "stock"` adds to its own stock within its `max`. The loop, its wait, its `bounce` and its tie rules stay. A production chain is a queued action (M3) with a `stock_cost` from its producer's own stock and a `stock = { resource, amount }` effect: a smelter turns ore into bars. A cost from stock is its own field, as `cost` names pools and player resources, and a stock's resource shares a player resource's name. RimWorld's haul jobs and The Settlers' carriers move goods this way.

The snapshot tests sweep the new state types of M3 to M6 as they sweep the skirmish's (design 11, D14).

## 19. The genre proofs

Each of M7, M8 and M9 goes in the batch of the genre proof that first needs it, and M10, the Hive mode, after them.

### M7. Points as track data

(`capabilities`: `progression`; `package`.) A track gives `points = [...]`, the points each level gives, or `points = 1` for each level; the `level` track keeps one a level, so the MOBA keeps its data; a second track may give points, for Skyrim's perks.

### M8. The player as an actor

(`capabilities`: `units`, `players`, `actions`; `package`.) Each player slot gets a player unit beside its player entity of design 14, which keeps the player's resources and modifiers: a unit with no body and no pools, so it collides with nothing and takes no damage; the engine tag `player`, which a filter selects only when it names it, as it treats `projectile`, `area` and `item`; its own team sees it, so its owner receives its cooldowns and charges; it stands at the centre of the map's bounds, which no rule reads, as its actions aim with global range. Its actions run through the pipeline as it is, and pay from its player entity's resources: tower defense's placement, Generals' powers and card play need no second pipeline. Unreal's GAS gives the PlayerState an ability system for this. A `build` action of the player unit places a site with no builder's walk. A player unit takes a stable id only in a mode with `[player] slots`, so other matches keep their ids and goldens.

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

## 20. The network

`quic`, `net`. After the issue log's **Quic** item on the endpoint's benches, which sets N1's cap.

### N1. Limits for each address

(`quic`, `net`; **Quic**.) No limit holds live connections or new handshakes for each address, where design 15's D13 bounds both by the session's seats for each IPv4 address or IPv6 /64, and `net` gives the endpoint no seat count ([QUIC transport](docs/design/15-quic-transport.md), D13). After the bench of the endpoint, the issue log's **Quic** item on benches, sets the cap of new connections on the same path.

### N2. UPnP behind a host firewall

(`quic`; **Quic**.) `igd-next` discovers a router by search alone, and the router's `NOTIFY` announcements, which a host firewall that tracks connections lets in, go unread ([QUIC transport](docs/design/15-quic-transport.md), D12).

Then the network's Research items of the issue log as they show; then P1 to P4, P3 only as D14 of design 15 lets it.

## 21. Planned

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

- **The script view copies the whole match** (Core; I5). Each script batch reads every unit again, though few change between two batches of a tick: 14.2 % of a 3v3 match's time, a cost that grows with the units times the batches, which the Zero Hour import multiplies. Items: Core, "The script view copies the whole match".
- **The import's materials are glTF's** (Zero Hour). glTF's material model holds no W3D pass past the first, no ambient color and no fallback texture, so the presentation loses each, and each later fidelity gap of a W3D material meets the same limit. Items: Zero Hour, "The import writes W3D's materials into glTF's material model"; Zero Hour, "A texture no archive holds"; Zero Hour, "The import ignores a vertex material's ambient color"; Zero Hour, "The import draws a mesh's first material pass".
- **The import has no object that only the client shows** (Zero Hour). Every object a map places becomes a unit of the sim, where the game spawns props, optimized trees and fluff on the client alone and some objects not at all, so the sim, its log and its state hash hold objects the original never simulates, and the parity oracle meets them. Items: Zero Hour, "The import makes a unit of every object a map places".
