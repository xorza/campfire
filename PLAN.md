# Plan

The one plan of the project: every open step, in the order it is done, in batches. A batch is one commit: it holds the steps that change the same systems, refactorings, fixes and features alike, and ends with the check chain passing for the crates it touches, and a stop for review. Each batch says what it touches and why it stands where it does. Open items only: a step goes when it is done. The issue log, `.notes/ISSUES.md`, holds every open problem; a problem that a step here takes carries the step's tag there ([Issue log](AGENTS.md#issue-log)). The designs in `docs/design/` stay the source of truth: a step names what the code lacks against its design, or the design it proposes, which moves into `docs/design/` when it is agreed.

One policy holds across the steps: **what can be forgotten is a type or a trait method with no default.** A rule that no type can hold is held by a type that makes the right way the short one, a test where one can see the breach, and review. Line numbers point to the code at commit `3de94c9d`. A step that states no check holds this one: no golden moves, and the check chain passes for its crates.

## Direction

Every batch moves the engine toward one shape, and each item names the part it builds.

- **A module is one entry of one list.** Each module, the core's `units`, `players`, `actions` and `deliveries`, each capability and the mode, implements `Module`: its capability, its needs and its uses, `declare` and `install`, none with a default (F7, F44). `install` holds only its systems and their sets; all else a module adds is a declaration.
- **What a module adds is declared once,** and one walk of the declarations serves the installer, the script API's binder, the network's lists of state types and the declaration audit:

| What | Declaration | Item |
| --- | --- | --- |
| A state type, a state resource | `d.component`, `d.resource` | today |
| A resource that is not state: clear between ticks, or rebuilt from state | `d.scratch`, `d.cache` | F9 |
| Callbacks a lower module calls | `d.callback` | F11 |
| A part every entity of a kind starts with | `d.part` | F8, F25 |
| Engine tags | `d.engine_tags` | F17 |
| Package data a match holds | `d.book` | F24 |
| A column of the script view, a part of a script call | `d.column`, `d.call_part` | today, F14, F37 |
| Script members, hooks and data tables | `d.api` | today, F20, F21 |

- **A kind is a marker,** one on each entity (F13); a kind's parts are declared (F25); a derived part comes by a hook of the part it derives from (F16).
- **A name is resolved once,** at the load: one table of names for each id (F19), a builder that is the check of what it resolves (F23), a script argument resolved by its type (F20).
- **One word, one meaning:** module, owner, kind, part, callback (`Hook` is the scripts' alone), `*File`, `*Data`, `*Spec`, `*Book` (F15, F22).
- **A pass visits what changed:** no query state, structural change or allocation per item on a hot path (F18, F27 to F29, F33, I1), and a fact fixed at install is a fact of the schedule (F39).

So a new capability is one entry of the list; a new state type its type and one `d.component`; a new section of a unit type a `DataSection`, its owner's data record and a `d.part`; a new effect an `Effect` and a `d.callback`. The declaration audit and the release tests fail on each piece left out.

## 1. Slice 3's play

Its steps are done ([Replication audiences](docs/design/14-replication-audiences.md)); it ends as the user plays it in the client ([Workflow](AGENTS.md#workflow)).

## 2. The test fixtures

`package`, `protocol`, `sim`, `codec`, `capabilities`' and `runner`'s internals; the tests of `runner`, `net`, `verifier`, `package`; `checks`. First, as the batches below write their fixtures in it: one vocabulary for what a test builds. No golden moves.

### F3. One edited test package

(`package`; the tests of `runner`, `net`, `verifier`, `package`.) Eight places read the workspace's packages, edit a file, index them again and load a mode, each by hand (`runner/src/harness/arena.rs:31-43`, `net/src/harness/in_process_match/mod.rs:454-479`, `runner/tests/lane_match.rs:150-160` and `175-186`, `verifier/tests/headless.rs:769-780`, `net/tests/short_3v3.rs:40-62`, `package/tests/rules.rs:19-37`, `package/tests/moba.rs:30-80`). `package/tests/moba.rs` has an `Edit` enum no other crate reaches, and the test modes' paths and maps are written out some 20 times. `package`'s internals gain one tree and one list of test modes:

```rust
// campfire-package/src/package_tree.rs, behind `internals`
/// The workspace's packages under a path, as a test edits them before it loads a mode.
#[derive(Debug, Clone)]
pub struct PackageTree(BTreeMap<PathBuf, Vec<u8>>);

impl PackageTree {
    pub fn workspace(path: &str) -> PackageTree { /* … */ }
    pub fn edit(&mut self, file: &str, edit: Edit) -> &mut PackageTree { /* … */ }
    /// The mode at `root` on `map`, its packages indexed again.
    pub fn mode(&self, root: &str, map: &str) -> ModePackages { /* … */ }
}

/// An edit of one file: `Append`, `Replace`, `Create`, `Set`, `Remove`, `Rename`.
pub enum Edit { /* … */ }

/// The test modes, each with its root and its map.
pub enum TestMode { Lane, Proving, Moba3v3, Ruled }
```

`Arena::load` takes a `PackageTree`. Check: each of the eight sites builds through it, and a forgotten index no longer compiles.

### F4. One test identity

(`protocol`, `runner`, `net`.) `FixedSession` (`runner/src/harness/fixed_session.rs:31-38`, `163-182`) and `InProcessMatch` (`net/src/harness/in_process_match/mod.rs:248`, `913-945`) each number the test players' keys their own way, and `1_700_000_000`, the tests' time, is written in eight places. `TestKey` (`protocol`'s internals) gains `player(slot) -> PlayerKeys { main, session }`, sorted so slot order is key order, `spectator(n)`, `NOW` and `seed_chain(room)`, and both harnesses take them.

### F5. Methods, points and one home in the test facades

(`capabilities`, `sim`, `runner`.) The capabilities' internals export free functions: `spent_life` (`stats/life.rs:89`), `spent` (`stats/pools.rs:99`), `seen_by_all` (`vision/seen_by.rs:51`), `reaches`, `reaches_bound` and `sinks_into` (`units/body.rs:142-165`); they become `Life::spent`, `Pools::spent`, `SeenBy::all` and methods of `Body`. `HarnessWorldInternals::restore_in_place` (`runner/src/harness/harness_world.rs`) reads only `EntityIndex` and `SimWorld::restore`, and moves into `SimWorldInternals`. The harness takes a point as `x: i64, z: i64` (`MatchUnits::standing_at`, `spawned_at`, `match_units.rs:63-80`), and tests outside `capabilities` write `Position::new(Vec3::new(Num::int(..` 18 times: every harness takes `impl Into<Position>`, as design 02 gives it.

### F6. Crate edges used by nothing

(`checks`, `import`, `net`, `codec`.) `campfire-lan-check` depends on `campfire-sim` and no source file uses it. `import`'s dev-dependencies turn on `campfire-log/internals` and `campfire-package/internals`, which no test uses. `net` depends on `toml` only to name `toml::de::Error` (`net/src/order_script/error.rs:5`), as `codec`'s `Toml::parse` returns the foreign error: `codec` exports `TomlError` and `JsonError`, and `net` drops `toml`. Check: the workspace builds with each edge gone.

## 3. One list of modules

`capabilities`: `declare`, `release`, `players`, every module's `declare` and `install`; `sim` and each crate with an `ALL`. The base of every declaration below: one list of modules, each with no step that can be skipped, every resource of a match declared, and every enum's `ALL` complete, as the list and the callbacks index by `Capability`.

### F7. One module list

(`capabilities`: `declare`, `release`.) `CAPABILITIES` (`release/mod.rs:77`) holds each capability's install function, `declare_capability` (`release/mod.rs:168`) maps the same capabilities to their types in a second list, and the core's three modules are listed twice, in `Release::core` and in `Release::build`. A built capability that `declare_capability` leaves in its `=> {}` arm installs with no declarations, and nothing fails. The idea of an owner has four names: `MODULE`, `OWNER`, `owner`, `module`. One trait holds both halves of a module, neither with a default, and one ordered list visits them, as a Bevy plugin group lists its plugins:

```rust
// declare/module.rs
pub(crate) trait Module {
    /// The capability it is; `None` for the core's.
    const CAPABILITY: Option<Capability>;
    /// The capabilities it needs, without which it does not run.
    const NEEDS: &'static [Capability];
    fn declare<D: Declarations>(d: &mut D);
    fn install(cx: &mut InstallCx<'_>);
}

pub(crate) trait ModuleVisitor {
    fn module<M: Module>(&mut self);
}

// release/mod.rs: the one list, in install order
fn modules<V: ModuleVisitor>(v: &mut V) {
    v.module::<Units>();
    v.module::<Players>();
    v.module::<Actions>();
    v.module::<Deliveries>();
    v.module::<Stats>();
    // … each capability, then the mode
}
```

`Release::declare`, `Release::build`, `Release::capabilities` and the table tests become visitors of the list; `Row`, `planned` and `declare_capability` go, `CapabilitySet::needs`' table becomes each module's `NEEDS`, and the owner is `CAPABILITY` everywhere. A module that another's declaration calls for, as `Deliveries` for a deliverer (F11), stands in the list and installs when it is called for. Check: no golden moves.

### F8. `players` declares its own

(`capabilities`: `players`, `units`, `mode`.) `Player`, `HeldOut`, `OutOfPlay` and `PlayerResources` live in `players/`, and the mode declares them (`mode/mod.rs:131-134`); `units` declares `MatchEntity` and the player's team (`units/mod.rs:152`, `172`), and `Units::open` sets up each kind's `KindParts` by hand (`units/mod.rs:248-249`). `Players` becomes a core module of F7's list, which declares its kinds and their state, and the installer sets up the parts of every kind `Units::kinds` visits. Check: a test match with no mode holds the player's state; no golden moves.

### F9. Every resource declared

(`capabilities`: `declare`, `release`, every install.) A state resource is declared, `d.resource`, and inserted again by hand in each install, seven times of seven (as `mode/mod.rs:204-206`, `vision/mod.rs:68`, `progression/mod.rs:46`); a missing insert shows only in that capability's restore test. Some twenty other resources are inserted in install bodies as "not state", which nothing checks: one that carried data from tick to tick would make a restored match differ, with no error. `unlisted_components` (`release/mod.rs:333`) audits components alone.

- The installer inserts each declared resource at its default, as it puts a declared book.
- A resource that is not state is declared by what it is. Scratch, `d.scratch::<R: Scratch>()`, is clear between ticks, and its trait's `fn is_clear(&self) -> bool` has no default; a cache, `d.cache::<R: Cache>()`, as the static index or F33's grid, is rebuilt from state, so a restore needs none of it; a book (F24) and a registry of callbacks (F11) are declared as such.
- `Release::unlisted_resources`, over `World::iter_resources`, fails the proving match on a resource no declaration names, and on a scratch resource not clear between ticks; the restore tests, which compare a restored match's hashes with the original's, hold that a cache is rebuilt.

`NewUnitStates`, cleared as the next tick starts, and `Deaths`, which keeps a match's last deaths after its end, are settled by the step: each is state, or clears where the rule says. Check: no golden moves.

### F10. Every variant in its `ALL`

(`sim`, `capabilities`, `net`, and each crate with an `ALL`.) Sixteen enums list their variants by hand in an `ALL`, as `Capability::ALL` (`sim/src/capability.rs:32`) and `DataKind::ALL`: a variant left out compiles, and `EffectQueues`' `[_; Capability::ALL.len()]` then panics at run time, or a data kind cannot be named in `[visibility]`. One declarative macro, `macro_rules!` with no new dependency, states each enum and its `ALL` together, so neither can lack a variant. Check: each `ALL` is the macro's.

## 4. Callbacks by declaration

`capabilities`: `declare`, `actions`, `deliveries`, `abilities`, `items`, `combat`, `navigation`, `areas`, `progression`, `projectiles`. Before batch 13, whose delivery seam and grant sources are callbacks too.

### F11. Callbacks by declaration

(`capabilities`.) Three registries of callbacks have three shapes and four ways in: `EffectQueues`, an array by capability (`actions/effect_queues.rs:19`), `Deliverers`, a field for each shape (`deliveries/deliverers.rs:16`), and `CastSpends`, one `Option` (`abilities/cast_spends.rs:12`); each is inserted by `insert_resource`, `init_resource` or `get_resource_or_init`, and each registration sits in an install body. A missing one panics at run time; only `EffectQueues` has a test, which builds seven matches (`release/tests/declaration_audit.rs:355`). Their function types name types above `declare`'s layer, which is why they stayed out of the declarations; an associated type solves that, as `book<B: Book>` already takes navigation's `ByType<Walker>`:

```rust
// declare/callbacks.rs
/// A registry of callbacks, each under a key, which modules declare.
pub(crate) trait Callbacks: Resource + Default {
    type Key: Copy + Eq + Debug;
    type Callback: Copy;
    /// The module that may declare the callback of `key`.
    fn owner(key: Self::Key) -> Option<Capability>;
    fn register(&mut self, key: Self::Key, callback: Self::Callback);
}

// Declarations
fn callback<C: Callbacks>(&mut self, key: C::Key, callback: C::Callback);

// combat/mod.rs, in `declare`
d.callback::<EffectQueues>(Capability::Combat, CombatEffect::queue_listed);
```

The installer registers each; the declaration audit holds that a key's owner declares it, and once, which replaces the seven matches. `Deliveries` installs when a module declares a deliverer, in place of the list `[Projectiles, Areas, Abilities]` (`deliveries/mod.rs:88-92`). The word is `callback`, as `Hook` names the scripts' hooks. Check: no golden moves.

## 5. Entity kinds

`sim`, `capabilities`: `units`, `players`, the declaration audit; `net`'s kind shows and filters. What makes an entity a unit, a delivery, a player or the match, held where it cannot be wrong, and one word for kinds and parts before batches 6 and 10 write more of them. The state column moves, and the data version rises.

### F12. The script view reads units by their kind

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

Check: no golden moves.

### F13. Each entity is of one kind, by its marker

(`sim`, `capabilities`: `units`, `players`; `net`.) "A unit but not a delivery" is written out three times (`units/kind_filters.rs:13`, `stats/refresh.rs:53`, `mode/mod.rs:270-271`), and M8's player unit would need each one; the four kinds have the same hand-written `HostRule`; `UnitEntity` is state and `DeliveryEntity` derived. A marker trait gives every kind its host rule, each kind is its own marker, and a hook refuses a second kind on one entity:

```rust
// sim: every kind's marker hosts its own state
pub trait KindMarker: Component {}
impl<K: KindMarker> HostRule for K { /* the entity holds `K` */ }

// capabilities: each kind's marker, the wider `UnitEntity` a group they require
#[derive(Component)]
#[require(UnitEntity)]
#[component(on_add = EntityKinds::one)]
pub struct BodyUnit;

pub trait EntityKind: KindMarker {
    const AUDIENCE: KindAudience;
    const OWNER_PREDICTS: bool;
}
```

`EntityKind::Only` goes, as a kind's entities are `With<K>`. A second kind on an entity panics in every test, not only in the proving match, so the declaration audit keeps one rule of kinds, on the proving match: each entity with a stable id has a kind.

```rust
for (id, entity) in world.resource::<EntityIndex>().iter() {
    assert!(EntityKinds::of(world.entity(entity)).is_some(), "{id:?} has a kind");
}
```

Check: the hook panics on a test that adds `DeliveryEntity` to a `BodyUnit`; the state column moves for `BodyUnit`, the behaviour column does not.

### F14. A column reads declared types

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

### F15. One word for each meaning

(`capabilities`, `net`.) "Kind filter" names an entity's query filter (`units/kind_filters.rs`) and a data kind's replication filter (`net/src/sim_server/kind_filter.rs`); "part" names a component of an entity (`KindParts`, `PartedKind`, `world.part`) and a piece of a script call or row (`CallPart`, `RowParts`). The renames: `entity_filters.rs`, `DataFilter<K>`, `EntityParts<K>`, `RowReads`; `PartedKind` becomes a constant of `EntityKind`. Pure renames.

## 6. A unit's derived parts

`capabilities`: `units`' tags, `stats`' refresh, `declare`; `client`'s HUD. A unit's tags and stats come with the parts they derive from, on every path: spawn, restore and the client's replication.

### F16. Derived parts at insert

(`capabilities`: `units`, `stats`; issue log, **Stats**.) A unit's derived parts come three ways: `Refresh::give_parts` (`stats/refresh.rs:44`), which runs at `SimEdge::Start` and after each of the nine stages through `Commands`, with a sync point at each edge (`stats/mod.rs:149-157`); `UnitTags::give_delivery_tags` (`units/mod.rs:208`); and the spawns (`units/unit_core.rs:50`, the kit's spawn). Two faults follow. `Refresh::regenerate` panics on a unit of a type with a pool and no `Level` (`stats/refresh.rs:277`), as `give_parts` gives it an empty `UnitStats`. And only `stats` installs `give_parts`, which returns early with no `StatBook`: in a match with no stats, as the `ruled` test mode, a restored or replicated unit that is not a delivery never gets its `UnitTags`, while a spawned one has its type's, so a block of its tags, its walk (`navigation/walking.rs:101`) and its script row (`units/unit_rows.rs:373`) read none. Component hooks give each derived part as its source arrives:

```rust
#[derive(Component)]
#[component(immutable, on_insert = UnitType::give_tags)]
#[require(UnitEntity)]
pub struct UnitType(u16);

#[derive(Component)]
#[component(on_add = Level::give_stats)]
pub struct Level(u32);
```

`UnitType`'s hook inserts its type's tags from the tag book, and an `Avatar` marker when its tags hold `avatar`, so `PlayerUnits::avatar` (`units/player_units.rs:44`), which the client's HUD calls three times a frame and `send_orders` each tick, queries `With<Avatar>` in place of a scan of every owned unit. `Level`'s hook inserts the base stats of its unit's type at its level, as the mode's spawn computes them now, so a reader in the stage of the spawn finds them, and `UnitStats` exists exactly when `Level` does: `regenerate` meets no unit with stats it cannot index. `give_parts`, `give_delivery_tags` and the spawns' inserts of stats and tags go, with ten systems a tick and their sync points. Check: a restored unit and a replicated one hold the tags and stats a spawned one holds, in a match with stats and in one without; the goldens do not move.

### F17. Engine tags by declaration

(`capabilities`.) `units/engine_tag.rs` fixes `Avatar`, `Projectile`, `Area`, `Constructing`, `Node`, `DropOff` and `Gathering` as the first tags of every match, so each new capability adds to a list of the core, and the overview says the core names no capability. Each module declares its engine tags, with their properties, in its `declare`, as it declares its state types; the tag book gives them their places in declaration order, which is install order. The core keeps `avatar` and the layer tags. The load still refuses a package that gives an engine tag.

```rust
// declare/declarations.rs
/// `tags`, the module's engine tags, which the tag book places after the ones declared
/// before them.
fn engine_tags(&mut self, tags: &'static [EngineTagSpec]);

// production/mod.rs, in `declare`
d.engine_tags(&ProductionTag::SPECS);
```

`players` declares `player`, the tag of M8's player unit. Check: the tags keep their numbers while the install order stays; no golden moves.

### F18. A live change refreshes only when its source changes

(`capabilities`: `stats`.) `Refresh::run` puts every unit with a live change in its visits, and back for the next pass (`stats/refresh.rs:144`, `179`; `stats/live_carriers.rs:4`): such a unit derives all its stats again in each of the ten passes of a tick, where `stats.md` asks a refresh only when a live change's source changes. `LiveCarriers` keeps `(source, carrier)` pairs sorted by source, and a pass adds the carriers whose source it visits, again until none joins, which covers chains:

```rust
loop {
    let before = visits.len();
    carriers.join_sources(&visited_ids, &mut visits);
    visits.sort_unstable();
    visits.dedup();
    if visits.len() == before {
        break;
    }
}
```

A live value reads only its source's type, level and stats, which this system alone writes, so a skipped carrier would compute the values it holds. Check: the goldens do not move; count the 3v3's live carriers before and after.

## 7. Names

`capabilities`: `values`, `units`' view, `scripts`, each `*_api.rs`, `books`; `package`'s load check and data types. One table and one spelling for each name a package declares, and one name for each kind of data, before F23's builders and F24's books are written in them.

### F19. One name table for every id

(`capabilities`, `package`.) Each id has its own store and lookup at three costs: linear scans (`ResourceId::named`, `players/resource_id.rs:16`; `PoolId::named`, `stats/pool_id.rs:26`; `ItemId::named`; `TrackBook::named`; `SlotKinds::named`; `NameList::named`, for teams, paths and tags; `ScriptNames::position`; `StatsColumn::pool_id_named`), binary searches (`StatBook`, `ActionBook`, `ModifierBook`, `UnitTypes`, `NameTable`), and `View`'s twelve methods that forward to `ViewNames` (`units/view.rs:197-306`). `ViewNames::team_name` and `path_name` allocate an `ImmutableString` on every call (`units/view_names.rs:137`, `171`). The load check states each name's existence again on the raw data (`package/src/load_check.rs:829-890`), and the spellings vary: `named`, `get_named`, `sorted_named`, `field_named`, `pool_named` returning `Checked` beside `pool_id_named` returning `Option`.

```rust
// values/names.rs
pub(crate) trait NameId: Copy {
    const LIMIT: usize;
    fn at(index: usize) -> Self;
    fn index(self) -> usize;
}

/// The names of the ids of one kind: each id's text, and the ids sorted by it.
pub(crate) struct Names<I: NameId> {
    text: Vec<ImmutableString>,
    sorted: Vec<I>,
}

impl<I: NameId> Names<I> {
    /// The id named `name`, by a binary search.
    pub(crate) fn named(&self, name: &str) -> Option<I> { /* … */ }
    /// The text of `id`, shared, with no allocation.
    pub(crate) fn name(&self, id: I) -> &ImmutableString { /* … */ }
}
```

Every book holds a `Names<I>`; `ViewNames` becomes `Names` fields, and `View`'s forwarding methods go; the load builds the same tables once, so its check of a script's name is `tables.has(kind, text)`. Check: `LOOKUPS` shrinks to the spelling `named`; no golden moves.

### F20. A script's argument is resolved by its type

(`capabilities`: `scripts`, each `*_api.rs`.) Each name argument is written twice: in its spec, `.name(2, NameKind::DamageKind)` (`combat/combat_api.rs:37-53`), and in its body, `view.damage_kind_named(kind).ok_or(ApiError::UnknownDamageKind)` (`combat_api.rs:146-151`); `ApiError` holds some sixteen `Unknown*` cases beside `NameKind`, `UnknownAction` and `UnknownAbility` both. Each argument that takes a `Num` is bound a second time with `INT` by hand, seventeen times, and a missing twin fails only when a script passes an integer. The arity macro of `scripts/ctx_fn.rs` converts each argument through one trait:

```rust
pub(crate) trait ScriptArg: Sized {
    type Raw: Variant + Clone;
    /// The kind of name it is, which the member's spec records.
    const NAMES: Option<NameKind>;
    fn take(raw: Self::Raw, ctx: &Ctx) -> Checked<Self>;
}

// before: |ctx, target: Unit, amount: Num, kind: &str|
// after:  |ctx, target: Unit, amount: Num, kind: DamageKind|
```

`bind` fills the spec's names from the argument types, `ApiError::Unknown(NameKind)` replaces the parallel cases, and the same wrapper binds the `INT` form of each `Num` argument. Check: the script API reference does not change; a script that passes an integer to each `Num` argument runs.

### F21. Owners from the registry

(`capabilities`: `scripts`, `books`; `package`'s load check.) `Hook::capability` is a hand table (`scripts/hook.rs:153-178`) that disagrees with the modules that record the hooks: it gives `OnHit` and `OnEnd` to `abilities`, which `deliveries`, a core module, records (`deliveries/deliveries_api.rs:15-16`), and `CalcDamage`, `CalcHeal` and `OnUnitDied` to `combat`, which the mode records (`mode/mode_api.rs:62-64`); the load check and the reference read the hand table. `LoadCheck::sections` pairs each section of a unit type with its capability by hand (`load_check.rs:1424-1447`), where the mode's sections, above it, ask the registry (`data_owner`), as `DataTable` has no table of unit types; `UnitTypeFile`'s own `Deserialize` writes its 18 fields three times, and `delivery_only` a fourth.

- `ApiBuilder::hook` records its hook with the builder's owner, and `Hook::capability` goes. Each of the five hooks takes its owner by a decision in this step: the module that calls it.
- `DataTable::UnitType` records each capability's section of a unit type, and the load check asks `data_owner`, as for the mode's.
- `UnitTypeFile` derives `Deserialize` with `deny_unknown_fields`, its core fields flat.

Check: a section with no owner fails `the_data_fields_are_the_schemas`.

### F22. One vocabulary of data

(`package`, `capabilities`.) One concept takes several names: `ModeFile` is a whole file, and `UnitsData` and `AvatarData` are whole files too, while `UnitTypeFile` is one table; `CombatRules`, `NavigationRules` and `SupplyRules` are the tables `DataTable::ModeCombat`, `ModeNavigation` and `ModeSupply`; one effect is `Effecting`, then `CapabilityDoes`, then `*Effect`; `ByType<ProductionData>` is a book of the match; `StatBook`, `TagBook`, `ModeBook` and `ScriptBook` are not `Book`s, and `BuildSpecs`, `EffectLists` and `Requirements` are. One rule:

| Suffix | Meaning |
| --- | --- |
| `*File` | a whole data file |
| `*Data` | the serde table of a `DataTable` |
| `*Spec` | one record, its names resolved |
| `*Book` | exactly the types that implement `Book` |

```rust
/// A table of package data, by the `DataTable` the script API records it under.
pub(crate) trait DataSection: DeserializeOwned {
    const TABLE: DataTable;
}
```

`ROLES` gains `impl Book for` and `impl DataSection for`, so the role test holds the rule. Pure renames. Check: no golden moves.

## 8. Load, then trust

`package`, `capabilities`' books. Before the books' path and the unit parts are written, so each builder is written once in the checked form, and resolves through F19's names.

### F23. Load, then trust

(`capabilities`, `package`.) The builders `expect` what `campfire-package/src/load_check.rs` (2,288 lines) checked: 40 `expect(CHECKED)` in `books/book_builder.rs`, and 28 `expect("the load …")` in 21 other files. The two copies of each rule can drift: a check the load loses becomes a panic at the build.

```rust
// stats/pool_book.rs:31
let index = |stat| book.named(stat).expect("the load checked the pools' stats");

// load_check.rs:1379, the same rule
self.stats_declared(iter::once(&pool.max).chain(&pool.regen), &at)?;
```

A builder that resolves a name, through F19's `Names`, is the check of that name: it returns the case that fails, and the load check drops its copy. The book build already returns `BookError`:

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

## 9. Books

`capabilities`: `books`, `mode`, `declare`, `stats`, `actions`, `navigation`, `units`. One path for every piece of package data a match holds.

### F24. One path for every book

(`capabilities`.) `Book` covers 18 types (`declaration_audit.rs:272`); a second family bypasses it. `ModeBooks::install` inserts `LifePool`, `CombatBindings`, `StatBook`, `PoolBook`, `TagBook`, `SlotKinds` and `Shop` (`mode/mode_books.rs:92-104`), `MapGround` `Metric` and `Bounds` (`mode/map_ground.rs:30`), `Mode::install` `Paths` and `AssistWindow` (`mode/mod.rs:198-201`), `Books::install` `SupplyRules` (`books/mod.rs:135`), and installs insert empty defaults by hand (`actions/mod.rs:115`, `navigation/mod.rs:126`, `units/mod.rs:201-203`). `Books::install` puts each of 18 books by a line of its own, and eight hand calls share books into the view and the frame, again in fixtures (`actions/mod.rs:211`, `progression/mod.rs:70`, `stats/mod.rs:237`). A book declared and never put stays empty, silently.

```rust
pub(crate) trait Book: Resource + Default {
    const OWNER: Option<Capability>;
    const SCRIPTS_ONLY: bool;
    /// Gives the view and the frame what they copy of it; no default.
    fn share(&self, view: &View, frame: Option<&mut Frame>);
}

// books/book_fill.rs: the install, a walk of the declarations
impl Declarations for BookFill<'_> {
    fn book<B: Book>(&mut self) {
        let book = self.built.take::<B>().expect("each declared book is built");
        book.share(self.view, self.frame.as_deref_mut());
        Books::put(self.world, book);
    }
    // …
}
// after the walk: assert!(self.built.is_empty(), "each built book is declared");
```

Every type above becomes a book; fixtures load through the same walk; a guard on a book's presence, as `contains_resource::<StatBook>()` (`stats/mod.rs:190`), becomes a check of an empty book. Check: a book built and not declared, or declared and not built, panics at install; no golden moves.

## 10. A unit's parts by declaration

`capabilities`: `mode`'s kit, `books`, `declare`, and each capability that gives a unit a part; `runner`'s arena. A unit takes its parts as the player and the match take theirs: each capability declares them.

### F25. Each capability declares the parts of its units

(`capabilities`.) `KitPart::ALL` (`mode/unit_kit/kit_part.rs:34`) lists 22 parts of seven capabilities by hand, `UnitKit` and `KitSections` hold a field for each, `Mode::kits_held` (`mode/mod.rs:263`) checks a restore apart from the declarations, and slots, the passive and stats come apart again in `UnitKit::spawn`. A part a type gives is missed silently, and the declaration audit sees none. A mode's unit takes its sight from its kit, and a delivery from `ByType<Sight>`, which the books fill for delivery types alone (`books/book_builder.rs:712`, `deliveries/delivery_spawner.rs:67`). The player and the match take theirs by `d.part::<K, C>` (`release/installer.rs`), which also registers the restore check; units take theirs the same way, from their type's books:

```rust
// declare/declarations.rs: one declaration for a part of every kind
fn part<K: EntityKind, C: SimComponent>(&mut self, start: fn(&KindSpawn<'_, K>) -> Option<C>);

// vision/mod.rs, in `declare`: every unit type's sight, a delivery's among them, one book
d.part::<UnitEntity, Sight>(|at| at.of_type::<Sight>().copied());

// navigation/mod.rs
d.part::<BodyUnit, MoveStep>(|at| at.of_type::<MoveStep>().copied());
d.part::<BodyUnit, Route>(|at| at.of_type::<MoveStep>().map(|_| Route::default()));
```

Slots become a part `actions` declares; a unit's stats are F16's; its type's passive is applied at the spawn until F31 makes it a grant source. The books fill a `ByType` of each part for every unit type, the delivery types' among them, so one `ByType<Sight>` serves a mode's unit and a delivery; `UnitKit`, `KitSections`, `KitPart` and `Mode::kits_held` go, and each part's restore check comes with its declaration. A spawn inserts one bundle per capability, some 8 moves between archetypes where the kit's one insert per part makes some 20 (`kit_part.rs:70`). Check: no golden moves; the declaration audit holds each unit part as state of the kind it names.

### F26. One spawn of a mode's unit

(`capabilities`: `mode`, `production`'s tests; `runner`.) Four spawns stand for "as the mode spawns", in two orders of arguments: `ModeUnits::spawn(world, type, pos, team, parts)` (`mode/mode_units.rs:30`), which repeats `ModeBook::spawn`; the arena's, which keeps `ModeUnits` in the world as a non-send `ArenaUnits` (`runner/src/harness/arena.rs:57-71`, `137`); `Mode::spawn_typed(world, name, team, pos)` through the `Spawner` (`mode/mod.rs:338`); and the test-only `Units::spawn(world, pos, team, parts)`. Three tests of production install `Spawner`s of their own that skip the kit (`production/tests/mod.rs:90`, `gathering.rs:459`, `construction.rs:93`): units no production path makes. `ModeUnits` builds the one `Spawner`, which `Mode::install` and `MatchBuild::arena` both install; `Arena::spawn` calls `Mode::spawn_typed(world, name, pos, team, parts)`, one order; `ArenaUnits` goes; the production tests load their unit types as data and spawn through it. Check: no golden moves.

## 11. Navigation

`capabilities`: `navigation`, `deliveries`, `projectiles`. Each navigation pass reads and writes only the units it changes.

### F27. Navigation reads the entity it holds, and announces a teleport

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

`Navigation::make_room` (`navigation/mod.rs:203-212`) builds a `query_filtered` and collects a fresh `Vec` of every walker on each box's spawn: it runs as a cached system, `world.run_system_cached_with`, whose query state and scratch persist.

### F28. Move and collide visit what they change

(`capabilities`: `navigation`.) `Walking::steer` sorts every living walker by stable id (`navigation/walking.rs:108-109`), then skips those that do not walk, wait for a route or have nothing ahead; `Walking::collide` calls `get_mut` on every collider (`walking.rs:269`), statics among them, though `Collider::part` never moves a body that cannot be pushed (`collider/mod.rs:139-149`). `steer` filters by the three tests before its sort; `collide` writes back only the colliders that `contacts` names, their indices sorted and deduplicated in a scratch buffer. The same units in the same order of stable ids, and the writes use `set_if_neq`. Check: the goldens do not move.

## 12. Read, then write

`capabilities`: `combat`, `abilities`, `progression`, `items`, `units`, `mode`. Before batch 13, which edits the damage pass and the attacks again.

### F29. No resource scope on a hot path

(`capabilities`.) In Bevy 0.19 a resource is a component of an entity of its own, and `World::resource_scope` takes it off and puts it back: two moves between archetypes and a flush. It runs for each damage dealt (`combat/damage_pass.rs:286`), each kill (`:293`), each channel step (`abilities/channels.rs:77`), each effect or use (`abilities/abilities_effect.rs:45`, `progression/progression_effect.rs:63`, `items/mod.rs:77`), each spawn (`units/unit_core.rs:59`) and the mode's hooks (`mode/mode_hooks.rs:110`, `182`). The same need is met two other ways: a clone of an `Arc` book (`damage_pass.rs:267`, `modifier_intervals.rs:44`), and a non-send taken out and put back (`attacks.rs:153-179`, `damage_pass.rs:90-100`). One rule: read with shared borrows, then write; where a book is read while a write runs, clone its handle, and `ItemBook` becomes an `Arc` as `ActionBook`, `TrackBook` and `ModifierBook` are.

```rust
// abilities/channels.rs, in `step_channel`
let tick = {
    let slots = world.get::<ActionSlots>(entity).expect("a unit whose channel runs has slots");
    Self::channel_tick(world.resource::<ActionBook>(), slots)
};
world
    .get_mut::<ActionSlots>(entity)
    .expect("a unit whose channel runs has slots")
    .step_channel(now, blocked, tick);
```

`DamagePass::deal` reads first, into a scratch list the pass keeps, the attackers the index no longer holds, then writes. `Channels::predict_channels` then needs no `&mut World`, and becomes a system of queries, which takes one of the eight exclusive systems out of the prediction test's `EXCLUSIVE` (the issue log, **Core**). Check: no golden moves.

## 13. The action pipeline's seams

`capabilities`: `actions`, `combat`, `projectiles`, `stats`, `items`, `abilities`, `areas`, `mode`, `orders`, `production`, `sim`'s stages. What `actions` owns, and one fixed order of steps in each stage. No golden moves.

### F30. Action anchors and the delivery seam

(`capabilities`.) Most capabilities need `combat` only to order their systems against `CombatSet`: `vision` imports nothing of `combat` and needs it; `areas` and `items` order against it alone. `projectiles` reads `combat::shots::Shots` (`projectiles/launching.rs`) and pushes damage to `combat::pass_queue::PassQueue` (`projectiles/flights.rs`, `projectiles/flying.rs`), and `abilities` reads the life pool through `CombatWorld::life_pool`, though `LifePool` is `stats`'.

- `actions` owns the sub-stages that every action kind and delivery orders against: `Start`, `Deliver`, `Launch`, `Land`, `Resolve`. `CombatSet` keeps combat's own steps: the damage pass, deaths and respawn.
- A weapon's shot is a delivery request in `actions`, and its hit resolves through a callback that `combat` declares (F11): `projectiles` names no type of `combat`.
- `life_pool` moves from `CombatWorld` to `stats`' world trait.

Each capability's needs then are what its code needs: `vision` the core; `areas` and `abilities` `stats`, whose pools and modifiers they change; `projectiles` the core; `orders` `navigation`. F44 states them. Check: no golden moves; the archetype shuffle test and the schedule build's check of conflicts pass.

The anchors are the action pipeline's own steps, which every capability that acts orders against: the core names them, and they name no capability. Inputs keeps each module's own sets, ordered against the sets of the modules it builds on, as the module order of e43f95af holds them: one chained enum of every stage's steps would name the capabilities' concerns, `Toggles`, `Orders`, `ModeInputs`, in the core, against F17's rule, and order every pair of its steps, which never touch, so the schedule's parallelism goes. The mode, the top of the list, orders against eight capabilities' sets, each a use F44 states.

### F31. One grant mechanism

(`capabilities`.) Three loops keep the modifiers a source wants on a unit: `Holds::hold_passives` (`actions/holds.rs`), `ItemHolds::hold_items` (`items/item_holds.rs`) and `HeldPass` (`stats/held_pass.rs`); the first two are one loop, and perks and equipment are planned as two more sources. A capability declares a grant source, a callback of F11: it answers which `(modifier, rank, applier)` a unit wants, and whether that changed since the last pass. `stats` owns one pass that compares each changed unit's wanted set with its held set, and applies or releases. The sources are action passives and holds, carried items and the unit type's passive. Presence grants, auras, areas' insides and player modifiers, stay in `HeldPass`, as space finds them each tick.

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

## 14. Commands to their owners

`capabilities`: `orders`, `items`, `production`, `progression`; `client`; the session log's format version. After batch 13, as both change what `orders` orders against.

### F32. Each capability owns its commands

(`capabilities`, `client`.) `Action` (`orders/order/mod.rs`) holds `Learn`, `Buy`, `Sell`, `Swap`, `CancelTrain`, `Rally`, `Build` and `CancelBuild`, and `Orders::install` schedules `ProductionOrders` and `TradeOrders`: a mode that drives its units with no `orders` cannot buy, learn, cancel or rally, and the overview says each command names the capability that owns it.

- `items` decodes `Buy`, `Sell` and `Swap`; `progression` `Learn`; `production` `CancelTrain`, `Rally` and `CancelBuild`.
- `orders` keeps the orders a unit carries out, move, attack, slot, build and stop, and the shared parts, the unit list's decoding and the check that the player controls the units, which each capability calls.
- Each capability's commands apply in Inputs in F7's module order, so a tick's buys and cancels apply as now.

The commands' bytes in a session log change, so the log format's version rises. Check: the goldens' state and behaviour columns do not move.

## 15. One grid of units

`capabilities`: `units`, `actions`' targets, `projectiles`, `areas`, `stats`' held pass, `navigation`'s `make_room`. After batch 13, whose grants settle what `HeldPass` keeps. The grid is a cache of F9.

### F33. One grid of units, rebuilt when units move

(`capabilities`.) `Flying::fly` (`projectiles/flying.rs:64`), `AreaLife::trigger` (`areas/area_life.rs:62`), `AreaHolds::hold_inside` (`areas/area_holds.rs:41`) and `HeldPass::run` (`stats/held_pass.rs:152`) each keep a `Local<BodyGrid>` and rebuild it, a sort of every living unit (`units/body_grid/mod.rs:126-160`), all after Collide, the last stage that moves a unit: up to four sorts a tick. `StaticTracking` (`navigation/static_tracking.rs:59`) already rebuilds only on change. One derived `UnitGrid` in `units`, of every unit with a place, the dead among them, keyed by `Entity`, rebuilds when a unit came, went, moved or changed its body since its last build, by change ticks and an observer's mark as `StaticsDirty` keeps them; each reader checks team, tags, life and death as it visits, through its own query:

```rust
grid.current(&units, this_run).visit_near(pos, radius, |body| {
    let Some(unit) = targets.unit(body.key) else { return };
    // the existing exact test of reach
});
```

`Navigation::make_room` asks the grid for the walkers a new box overlaps, in place of F27's scan of every walker. Each reader already runs an exact test of reach and sorts its results (`area_life.rs:90`, `held_pass.rs:174`, `flights.rs`), so a wider set of candidates adds only ones the exact test refuses, and the result is the same bit for bit. `TargetIndex`, the script view's index over rows, stays apart, as its order of visits is the grid's. The broadphase (`navigation/broadphase.rs:28-41`) sorts a four-field entry where `BodyGrid` packs one number per entry: the step takes `BodyGrid`'s form for both. Check: the goldens do not move.

## 16. State types and their lists

`capabilities`: `state_types`, `release`; `net`'s replication. What each reader of the state types handles, and each list of them stated once. No golden moves.

### F34. One call for each state type

(`capabilities`, `net`.) `StateTypes` has nine methods, one for each way of sending, and ten implementations (`release/installer.rs:51`, `release/state_only.rs:24`, `release/mod.rs:287`, net's `kind_filter.rs:287` and `425`, `net/tests/leak.rs:137`, …), all but `Replicated` (`net_protocol.rs:117`) treating the five ways alike. `StateTypes` takes `state<C>`, `sim_state<C>`, `derived<C>`, `derived_sent<C>` and `resource<R>`; only `Replicated` implements a second trait of the five ways, which `C::Sending` visits. Some 250 lines go, and `sending.rs`'s checks at compile time stay. Check: no golden moves.

### F35. Net's data kinds listed once

(`net`.) A data kind is listed in five places: `filtered_kinds!` (`net/src/sim_server/kind_filter.rs:49`), the register calls (`:235-249`), `FilterObservers`' match (`:261`), the test's asserts (`:477`) and `DataKind::ALL`. The macro generates the registration, the exhaustive dispatch with its arm of unfiltered kinds, and the test's list, and F10's macro keeps `DataKind::ALL` complete. Each filtered kind's `Add` observer inserts its `KindFilter<K>` by command, a move between archetypes for each kind at every spawn on the server: a required component with a closed default, whose `on_add` hook writes its value in place, adds it in the spawn's move, once replicon's `on_insert` observer is checked against it. Check: the leak test passes.

## 17. The script call's runtime

`capabilities`: `scripts`, `units`' view and columns, `mode`'s state, `stats`' call. What a script call costs and how its parts are kept, with the view's copy of the match.

### F36. One schema of script state

(`capabilities`: `mode`, `units`, `stats`.) Mode, unit and modifier state each look a field up their own way: `NameTable` and places kept for the mode (`mode/mode_schema.rs:25-71`); `UnitStateBook::field_named`, which counts the field's place on every access (`units/unit_state_book.rs:50-63`); a binary search of its own for a modifier (`stats/modifier_handle.rs:189`). Two `StateField` types differ (`mode_schema.rs:35`, `unit_state_book.rs:23`). `StateValue::Text(String)` allocates an `ImmutableString` on every read (`scripts/state_value.rs:72`), and each mode call clones the whole state and every value again as it commits. One `StateSchema { names: Names<StateField>, places: Vec<StatePlace> }`, built once for each unit type and for the mode; text kept shared, so a read counts a reference; the mode copies a field only when it is written. Check: no golden moves.

### F37. One contract of a call part, and one list by type

(`capabilities`: `scripts`, `units`.) `CallPart::param_named` is on the trait, and two of four parts return `None`; the split of params by role is written three times (`stats/stats_call.rs:70`, `mode/mode_call.rs:55`, `scripts/ctx.rs:112`); `ModeCall::apply` does nothing and the mode commits apart, before the effects apply (`mode/calls.rs:116-128`). Three lists keyed by type are built three ways: `CallParts` by the vtable's `type_id` (`scripts/call_part.rs:36`), `ViewColumns` by a `TypeId` vec (`units/view_column.rs:28`), `Effects` by `as_any().is`; and each of the eight columns writes the same four delegating methods.

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

(`capabilities`; **Core**, and Foundations.) Each script batch reads every unit again, 14.2 % of a 3v3 match's time. Before the Zero Hour import multiplies the units. Its direction: a batch reads again only the rows whose parts changed since the last, by the change filter each column's `RowReads` already gives and F14's lists of what each column reads; the columns' half of F37 lands with it. A bench session the user starts measures it.

## 18. Run conditions

`capabilities`, every system the batches above leave. A sweep, last of the refactorings, over what the schedule can hold at install.

### F39. What is fixed at install is a run condition

(`capabilities`.) "Return if the book is missing" is written by hand in systems (`stats/refresh.rs:59`, `137`, `268`; `stats/held_pass.rs:117-119`; `actions/holds.rs:74-80`; `vision/seeing.rs:53`; `production/player_supply.rs:47`; `players/out_of_play.rs:44`), and `navigation/walking.rs:80-85` pairs two `Option`s of the map's pathing grid and planner with an `expect`; `pay_attack_toggles` returns at once on a client (`combat/attacks.rs:194`), though the install knows it predicts (`combat/mod.rs:121`); the mode's hooks check its hook set each tick (`mode/mode_hooks.rs:44`, `171`), which the install knows (`mode/mod.rs:207`). After F24 a declared book is always present, empty until a mode loads, so a guard for a missing book goes, and an empty book gives no work; a structure the map may lack, as the pathing grid with its planner, is taken by `If<(Res<PathingGrid>, ResMut<RoutePlanner>)>`, Bevy 0.19's parameter that skips a system while it is missing; a client's schedule leaves `pay_attack_toggles` out; the mode schedules only the hooks it defines, and the clear of `LevelUps` becomes a system of its own. A system skipped for a missing parameter keeps its last run, so its first run after the book arrives sees every change since the build: each system is checked for that, and the goldens and the archetype shuffle test hold it.

## 19. Match records and replays

`runner`, `verifier`; the tests of `runner` and `net`. How a test builds a match, records it and replays its log, in one form each.

### H1. Replay

(`net`, `runner`.) The loop of "take the checkpoint again, stop at a tick, run alone" is in `SimServer::restore_match` (`net/src/sim_server/mod.rs:381`) and `SessionDir::abort` (`net/src/sim_server/session_dir/mod.rs:217`): `SessionWorld::replay_to(tick, on_boundary)` holds it.

### F40. One replay of a published log

(`verifier`, `runner`; the tests of `net` and `runner`.) The verifier's `Replay` (`verifier/src/replay/mod.rs:46-66`) checks each checkpoint's hash and the result; tests that cannot depend on the verifier rebuild it by hand, eight times (`net/tests/scenario.rs:161`, `bots.rs:85`, `checkpoints.rs:93`, `saves.rs:157`, `restore.rs:106`, `prototype.rs:151`; `runner/tests/match_3v3/mod.rs:571`, `resume.rs:259`), three of them repeating its check of checkpoints line for line. `Replay` moves into `runner`, on H1's `replay_to`, as `Replay::of(log, &ModePackages)`, with a trail of hashes in its internals; the verifier keeps `Verification` and its command line, and loses its one use of `capabilities`.

### F41. One record of a played match

(`runner`.) A test puts `CopyCheck::new`, `Golden::new` and `HashTrail` together by hand, and each tick calls `copy.check`, `golden.record` and `trail.record_hashes` (`runner/tests/proving_match.rs:211-226`, `348-365`; `lane_match.rs:66-68`; `match_3v3/mod.rs:164-181`). `MatchRecord { copy, trail, golden }`, with `of(&mut Match, packages, players)`, `tick(&mut Match) -> MatchHashes`, `check(name)` and `assert_replays(&log)`, on F40's replay; `CopyCheck::new` becomes `CopyCheck::of`, as a value made from the world is.

### F42. One match built with no mode started

(`runner`, `common`.) A zero seed, `MatchBuild::spec`, `Match::new` and `install_mode` are written three times (`runner/src/session/mod.rs:112-117`, `match_build/mod.rs:121-126`, `match_build/bench.rs:40-44`), and `MatchBuild` is `pub` only for the internals, with an `expect(unreachable_pub)`. A production `MatchBuild::restore_target(packages, rate, players)` serves all three, the internals add only the `LogCheck`, `SegmentSeed::ZERO` replaces the repeated zero seed, and the lint's suppression goes.

## 20. The session's host

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

## 21. The shop

`capabilities`: `items`; `package`; `packages/test/moba`. Slice 2 starts here. A shop becomes a unit, then the shop's reader, an item's price and the items' names are built on it once, and the 3v3's golden moves once.

**Slice 2, wide bodies, uses, the shop and gravity,** batches 21 to 25. The user plays a 3v3 match of the MOBA test content: their hero holds a use on the jungle's shrine, buys items from the shop panel, and casts their actives with 1 to 6, a ward at the ground point; a test map holds a 1,117 m box that walkers go around. The design is [Wide bodies, uses, the shop and gravity](docs/design/13-bodies-uses-shop-gravity.md); each step folds its rules into the design 04 file of its capability.

**The capability model's redesign,** M1 to M10, a proposal for review: each step's decisions move into `docs/design/04-capabilities/` as it lands, and each generates the script API reference again. The core model is sound and generic: the action pipeline, the effect lists, stats and their formula, tags, relations and filters, the reach rule, deliveries as units and scripted systems serve every genre, and stay. The faults are at the edges, where a capability was built for its first genre: batches 3 to 20 and F44 take its couplings, and M1 to M10 open its closed sets. Each new section of a unit type an M step or U1 adds is built as the Direction gives it: a `DataSection` (F22), its owner's record in `DataTable::UnitType` (F21), its book (F24), its part (F25), and an `Effect` with its callback for each new effect (F11).

### M1. Shops as units

(`capabilities`: `items`; `package`; `packages/test/moba`.) A unit type's `shop = { items, resource, reach, buyers }` makes a unit a shop, a part `items` declares: `reach` a radius by the reach rule, `buyers` a filter, which may select the dead, so League of Legends' rule is data. The mode's `[shop]` and its markers go; the 3v3's fountain is a unit with a `shop` section. Warcraft III's shops are units, as a courier or a market is. Check: the 3v3's golden moves by the new unit's id alone.

### S1. Names and readers

(`capabilities`, `package`, `packages/test/moba`; D8.) `name` on item types and the MOBA's names; a public reader of the shop, an item's price for a unit, whether a unit stands where it can buy, and an action's targeting. A player's resources reach it on its player entity, `PlayerResources::of_player`. Check: a price less held components as computed by hand; an item type with no `name`, or one a locale lacks, fails its load.

## 22. The client's shop

`client`. I2 gives the view tests' units their types before S2 changes the client.

### I2. The client view tests' units

(`client`; **Client**.) They spawn with no unit type, which no client meets, and the view reads the type as optional. Before S2 changes the client.

### S2. The shop panel and item keys

(`client`; D9, D10, D11.) Bevy's UI features; the shop panel, the inventory bar, keys 1 to 6, and every cast key aimed as its action takes. Check: the key's target for an action of each targeting; the panel's dimmed items for an avatar alive out of a shop, too poor and with no room; the user buys a ward, places it at a point with its key, sells an item and swaps two slots.

## 23. Use and the graph

`capabilities`: `actions`, `abilities`, `items`, `interaction`, the release tests; `package`; `client`. F44 moves the group of an inventory's slots into `actions`, and U1 adds the `use` kind, whose actions block as `use` too: one batch settles which actions a `use` block stops. F44 comes after batches 11 to 15, which leave only the true edges; M2 reads the code as F44's graph test does.

### F44. The capability graph in one table

(`capabilities`, `docs/design/04-capabilities/00-overview.md`.) After F7, the graph's needs and the install order are one list of modules; the overview's layers stay in prose. `each_capability_installs_after_the_lower_layers_it_builds_on` ties the first two to the layer table (`LAYERS`, `release/tests/mod.rs`); no test ties them to the imports. The layer test rejects imports from a higher layer only: within a layer, `units ↔ scripts` and `units ↔ players` in layer 1 and `abilities ↔ items` in layer 5 import each other (`items/mod.rs:6` imports `CastSpends`; four `abilities` files import `Inventory`).

A capability's edge to another is a need, without which it does not run, or a use, which it reads when the match has it, each read guarded by `cx.has`: production runs without navigation, with no gathering. The table holds both, and the test holds it symmetric: each import of another capability is a need or a use, and each need and use is imported.

```rust
// declare/module.rs: beside F7's `CAPABILITY` and `NEEDS`
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

The `abilities ↔ items` cycle goes with two moves. `CastSpends`, a registry of callbacks (F11) with no item in it, moves into `actions`. The slot kind of an inventory, the one fact `abilities` reads of it, moves out of `Inventory` into a state type of `actions`, which `items` puts on with the inventory. It is not U1's `uses = { kind }`, the slot that holds a use under way, whose action blocks as `use` by its own kind, so the two share batch 23 and not a name:

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

## 24. The shrine

`packages/test/moba`, `client`. I3 fixes the 3v3's `hero_died` before U2 adds the shrine to the scripted match, whose golden would pin the failure.

### I3. Assist gold to a unit no player owns

(`packages/test/moba`; **MOBA test content**.) The 3v3's `hero_died` fails for an assister that no player owns, a tower, and the kill then gives no gold at all. Before U2 adds to the scripted match, whose golden would pin the failure.

### U2. The shrine

(`packages/test/moba`, `client`; D6.) The shrine, its action, `shrine_blessing` and `shrine_spent` in design 07 and the 3v3 package; a right click on a unit the avatar may use orders the use; the channel bar. Check: the scripted match has a hero use the shrine and the team's heroes take the blessing, and a second hero's use cut by a hit; the user holds a use on the shrine in the client.

## 25. Gravity

`capabilities`: `projectiles`; `package`; `packages/test/moba`; `client`. Slice 2's last step: the user then plays slice 2.

### G1. Gravity

(`capabilities`, `package`, `packages/test/moba`; D12.) The falling flight; `gravity` past planned, refused with `homing` or not positive; a grenade of the MOBA's items, which arcs to its point. Check: the points of a flight on each metric at its launch, its peak and its end as computed by hand, the planar one back at its launch height at its end; a spatial one hits a unit its chord meets and not one its straight line would; the client draws the grenade's arc.

## 26. Test time

`package`, `runner`, and the code each test spends its time in. Before the matches they play grow again.

### I4. Tests past 1 s

(`package`, `runner`; **Package**, **Runner**.) Four tests run at or past the bound of 1 s for a test alone: `mode_package::every_flaw_of_a_package_fails_its_load_with_its_own_problem`, `resume::the_3v3_match_resumes_from_each_checkpoint_to_the_same_hashes`, and the proving match's `a_snapshot_of_drawn_values_restores_or_is_refused_and_never_panics` and `a_snapshot_restores_and_a_flawed_one_never_panics`. Find where each spends its time, and bring each within its bound, or into `source/.config/nextest.toml` with the reason it takes longer.

## 27. Queues and capacities

`capabilities`: `production`, `actions`; `package`; `docs/design/11`. Train, research and supply in one form, before the Zero Hour skirmish, so its rules package and golden are made once, on the general model.

### M3. Queued actions with any output

(`capabilities`: `production`, `actions`; `package`.) Today `train` spawns a unit type alone; research has no kind, so an RTS times it with a mode timer; `craft` is planned as a third kind of "consume, wait, produce". Any unit may have `queue = { size }`. An action with `queue = true` joins the queue in place of a windup: it pays at once, waits its time at the head, then resolves with its `on_resolve` effects and hook. `train` becomes such an action with a `spawn` effect, research one with a `player_modifier = { id }` effect, craft one with a `give_item` effect, planned with items; cancel and refund move to the queue. The `train` kind goes, and no `craft` kind comes. Today's train rules become the queue's: checked as it joins, its costs, requirements and capacities (M4); never under way, so its unit stays free; its time from the tick it reaches the head; it resolves in the Mode stage of the tick its time ends in, and the next entry starts in that tick; a dead producer's queue waits; a producer that despawns takes its queue and refunds nothing. A queued `spawn` places its unit by the train's rule, the producer's body point nearest its rally point and the nearest open cell, and sends it to the rally point; a `spawn` of any other action keeps its own rule. StarCraft II's `CAbilQueue` trains and researches through one queue. Check: production's train, cancel, supply and rally tests pass with their assertions unchanged, on data that states a train as a queued action.

### M4. Capacities

(`capabilities`: `production`; `package`.) One `[supply]` counter serves StarCraft alone. The mode declares `[capacities.<name>] max`; a unit type gives `capacity = { <name> = { uses, provides } }`. Each is derived as supply is now, a pass over the owned units and the queues when a tick's first check needs it, so it keeps no sum and never drifts (design 11, D5). A queued action or a build lists the capacities it checks. Supply becomes one capacity; power, population and housing are more. Scripts read `ctx.capacity_used(player, name)` and `ctx.capacity_max(player, name)`.

## 28. Work and haul

`capabilities`: `production`, `interaction`; `package`; `docs/design/13`. Construction and gather in their general form, after batch 27, whose queued actions a production chain uses.

### M5. One work mechanism

(`capabilities`: `production`, `interaction`; `package`; `docs/design/13`.) Construction's rate table by builders and interaction's shared progress are one mechanism, one native and one planned as script state. A unit type's `work = { rates = [...], max_workers }` makes a unit that others work on: `rates` gives the rate for each count of workers from zero, a count past the table takes its last entry, and `max_workers` refuses a worker past it. The three construction styles become three tables: `alone` `rates = [1]`; `builder` `rates = [0, 1]`, `max_workers = 1`; `builders` `rates = [0, 1, 1.62, 2.16]`. Progress, its rounding of life and its completion tick stay as construction defines them. Completion runs the type's `on_complete` effects and the mode's `on_work_complete(ctx, unit)` hook. A held use with one side's progress, plant, defuse, revive, repair, becomes work, and M5 then amends design 13's D5 and `interaction.md`, which keep shared progress as script state until it lands; slice 2's shrine has no progress, so neither waits on the other. A capture point, by team with contest and decay, is an extension planned with `interaction`. TF2's capture rate by the count of cappers is the precedent. Check: the three styles' construction tests pass, tick for tick.

### M6. Haul

(`capabilities`: `production`; `package`.) Gather fills only a player resource, from a node to a drop-off. A unit type's `stock = { <resource> = { amount, max } }` gives a unit its own amounts. `gather` becomes `haul`: `from` filters units with stock of the resource, a node being one with no `max`; `to` filters units that take it; `take` is the most one trip carries; `windup_ms` the time at the source. A sink's section names where it takes: `drop_off = { resources, into = "player" }` is today's drop-off, `into = "stock"` adds to its own stock within its `max`. The loop, its wait, its `bounce` and its tie rules stay. A production chain is a queued action (M3) with a `stock_cost` from its producer's own stock and a `stock = { resource, amount }` effect: a smelter turns ore into bars. A cost from stock is its own field, as `cost` names pools and player resources, and a stock's resource shares a player resource's name. RimWorld's haul jobs and The Settlers' carriers move goods this way.

The snapshot tests sweep the new state types of M3 to M6 as they sweep the skirmish's (design 11, D14).

## 29. The genre proofs

Each of M7, M8 and M9 goes in the batch of the genre proof that first needs it, and M10, the Hive mode, after them.

### M7. Points as track data

(`capabilities`: `progression`; `package`.) A track gives `points = [...]`, the points each level gives, or `points = 1` for each level; the `level` track keeps one a level, so the MOBA keeps its data; a second track may give points, for Skyrim's perks.

### M8. The player as an actor

(`capabilities`: `units`, `players`, `actions`; `package`.) Each player slot gets a player unit beside its player entity of design 14, which keeps the player's resources and modifiers: a unit of its own kind, `PlayerUnit`, whose marker requires `UnitEntity` (F13), so every system of bodies, `With<BodyUnit>`, passes it by; with no body and no pools, so it collides with nothing and takes no damage; the engine tag `player` (F17), which a script's filter selects only when it names it, as it treats `projectile`, `area` and `item`; its own team sees it, so its owner receives its cooldowns and charges; it stands at the centre of the map's bounds, which no rule reads, as its actions aim with global range. Its actions run through the pipeline as it is, and pay from its player entity's resources: tower defense's placement, Generals' powers and card play need no second pipeline. Unreal's GAS gives the PlayerState an ability system for this. A `build` action of the player unit places a site with no builder's walk. A player unit takes a stable id only in a mode with `[player] slots`, so other matches keep their ids and goldens.

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

## 30. The network

`quic`, `net`. After the issue log's **Quic** item on the endpoint's benches, which sets N1's cap.

### N1. Limits for each address

(`quic`, `net`; **Quic**.) No limit holds live connections or new handshakes for each address, where design 15's D13 bounds both by the session's seats for each IPv4 address or IPv6 /64, and `net` gives the endpoint no seat count ([QUIC transport](docs/design/15-quic-transport.md), D13). After the bench of the endpoint, the issue log's **Quic** item on benches, sets the cap of new connections on the same path.

### N2. UPnP behind a host firewall

(`quic`; **Quic**.) `igd-next` discovers a router by search alone, and the router's `NOTIFY` announcements, which a host firewall that tracks connections lets in, go unread ([QUIC transport](docs/design/15-quic-transport.md), D12).

Then the network's Research items of the issue log as they show; then P1 to P4, P3 only as D14 of design 15 lets it.

## 31. Planned

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
