# Architecture audit: recommendations

The structural recommendations of the architecture audit of `source/` that are not done, each checked against the code at commit `020b867a`, where its line numbers point. The bugs the audit found are in the issue log; the items that change `net` are in [PLAN_NET.md](../PLAN_NET.md), H1 to H5.

One policy holds across the items: **what can be forgotten is a type or a trait method with no default.** A rule that no type can hold is held by a type that makes the right way the short one, a test where one can see the breach, and review.

## 1. One book of sights

A mode's unit takes its sight from its kit, and a delivery from `ByType<Sight>`, which the books fill for the delivery types alone; both read the same `vision` section of a unit type's file:

```rust
// mode/unit_kit/mod.rs:137, put on by kit_part.rs:46
sight: vision.map(|vision| vision.sight),

// books/book_builder.rs:711, in the build of the delivery types only
if let Some(vision) = file.vision {
    books.sights.set(unit_type, vision.sight);
}

// deliveries/delivery_spawner.rs:66
if let Some(&sight) = self.sights.as_deref().and_then(|book| book.get(unit_type)) {
    unit.insert(sight);
}
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

## 2. The script view reads units by their kind

`UnitRows` takes any entity of the entity index with a position and a team for a unit, where every other reader of units filters by the `UnitEntity` kind:

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

// units/unit_rows.rs:355
let parts = self
    .parts
    .get_manual(world, entity)
    .expect("the core reads optional parts");
let (Some(&pos), Some(&team), body, spawn, unit_type, owner, tags) = parts else {
    return None;
};
```

The core's query takes the kind's filter, and the parts every unit holds become required:

```rust
type CoreParts = (
    &'static Position,
    &'static Team,
    Option<&'static Body>,
    Option<&'static SpawnPoint>,
    Option<&'static UnitType>,
    Option<&'static Owner>,
    Option<&'static UnitTags>,
);

struct CoreSource {
    parts: QueryState<CoreParts, UnitFilter>,
    changed: SystemState<
        Query<'static, 'static, Entity, (UnitFilter, <CoreParts as RowParts>::Changed)>,
    >,
}

let (&pos, &team, body, spawn, unit_type, owner, tags) =
    self.parts.get_manual(world, entity).ok()?;
```

`UnitType` becomes required too once no test spawns a unit of no type into a match: the MOBA arena's units take types of their own.

## 3. Two rules the declaration audit lacks

The declaration audit (`release/tests/declaration_audit.rs`) checks the declarations against each other, but neither of these:

- **A column reads declared types.** A column's `RowParts` may read a component that no module declares as state or as derived, which no snapshot, restore or client holds, so a restored match's scripts would see another value.
- **Every entity is of a declared kind.** No test checks, on a played match, that each entity with a stable id is of exactly one kind that `Release::kinds` visits.

`RowParts` lists the components it reads, and the audit records the derived types it now drops:

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

impl<A: RowParts, B: RowParts> RowParts for (A, B) {
    type Changed = Or<(A::Changed, B::Changed)>;

    fn components(into: &mut Vec<(TypeId, &'static str)>) {
        A::components(into);
        B::components(into);
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

The kinds' check visits the kinds on the proving match once it ends, as it holds every capability:

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

## 4. Layers without cycles, and imports within the needs

The layer test (`a_module_imports_only_from_its_layer_and_below`, `release/tests/mod.rs`) rejects imports from a higher layer only. Within a layer, modules import each other both ways: `units ↔ scripts` and `units ↔ players` in layer 1, `abilities ↔ items` in layer 5 (`items/mod.rs:6` imports `CastSpends`; `abilities/mod.rs`, `casts.rs`, `channels.rs` and `toggles.rs` import `Inventory`). And `CapabilitySet::needs` disagrees with the imports of production code:

| Capability | Needs | Imports besides |
| --- | --- | --- |
| `navigation` | none | `projectiles`, `deliveries` |
| `abilities` | `combat` | `items`, `navigation` |
| `items` | `stats` | `combat`, `abilities` |
| `production` | none | `stats`, `combat`, `navigation`, `vision` |
| `orders` | `combat`, `navigation` | `abilities`, `items`, `production`, `progression`, `vision` |

An optional use is correct where the code guards it: production runs without navigation, with no gathering. So each capability lists the capabilities it uses when the match has them, each read guarded by `cx.has`, and the layer test gains both rules:

```rust
// declare/capability_set.rs
/// The capabilities `capability` reads when the match has them, and runs without.
pub(crate) const fn uses(capability: Capability) -> &'static [Capability] {
    match capability {
        Capability::Navigation => &[Capability::Projectiles],
        Capability::Abilities => &[Capability::Navigation],
        Capability::Items => &[Capability::Combat],
        Capability::Production => &[
            Capability::Stats,
            Capability::Combat,
            Capability::Navigation,
            Capability::Vision,
        ],
        Capability::Orders => &[
            Capability::Abilities,
            Capability::Items,
            Capability::Production,
            Capability::Progression,
            Capability::Vision,
        ],
        // …
    }
}

// release/tests/mod.rs
#[test]
fn a_capability_imports_only_what_it_needs_or_uses() {
    for capability in Capability::ALL.into_iter().filter(|c| layer(c.name()).is_some()) {
        let mut found = Vec::new();
        imports(&src.join(capability.name()), capability.name(), &mut found);
        for (_, to) in found {
            let Some(other) = Capability::ALL.into_iter().find(|c| c.name() == to) else {
                continue;
            };
            let allowed = other == capability
                || CapabilitySet::needs(capability).contains(&other)
                || CapabilitySet::uses(capability).contains(&other);
            assert!(allowed, "{capability:?} imports {other:?}");
        }
    }
}
```

Each row of the table is a need or a use, and the row's owner decides which. The `abilities ↔ items` cycle goes with two moves. `CastSpends`, a registry of hooks with no item in it, moves down into `actions`. The slot kind of an inventory, the one fact `abilities` reads of it, moves out of `Inventory` into a state type of `actions`, which `items` puts on with the inventory:

```rust
// actions/use_slots.rs
/// The kind of slot whose actions the unit uses rather than casts: its inventory's.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct UseSlots(SlotKind);

impl UseSlots {
    /// The group a tag blocks of the action in a slot of `kind`, of a unit with `slots`.
    pub(crate) fn group(slots: Option<&UseSlots>, kind: SlotKind) -> Block {
        if slots.is_some_and(|slots| slots.0 == kind) {
            Block::Use
        } else {
            Block::Cast
        }
    }
}

// abilities/mod.rs:74, was `Inventory::group(inventory, kind)`
let group = kind.map_or(Block::Cast, |kind| UseSlots::group(use_slots, kind));
```

The cycles of layer 1 need a design for each pair: what `units` takes from `scripts` and from `players`, and whether it moves down or the two modules merge.

## 5. Load, then trust

The builders `expect` what `campfire-package/src/load_check.rs` (2,288 lines) checked: 40 `expect(CHECKED)` in `books/book_builder.rs`, and 28 `expect("the load …")` in 21 other files of production code. The two copies of each rule can drift: a check the load loses becomes a panic at the build.

```rust
// stats/pool_book.rs:30
pub(crate) fn new(pools: &BTreeMap<DeclaredName, PoolData>, book: &StatBook) -> PoolBook {
    let index = |stat| book.named(stat).expect("the load checked the pools' stats");
    // …
}

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

A rule between packages, which no single builder sees, stays in the load check. An `expect` that a capability is installed, as `EffectQueues::of` holds, is a rule of the match, not of the data, and stays.

## 6. One hero lookup in the tests

`MatchUnits::hero` finds a player's hero, and the proving match repeats its filter:

```rust
// runner/tests/proving_match.rs:76
fn hero(world: &World, slot: u32) -> (StableId, EntityRef<'_>) {
    world
        .resource::<EntityIndex>()
        .iter()
        .map(|(id, entity)| (id, world.entity(entity)))
        .find(|(_, unit)| {
            unit.contains::<Experience>()
                && unit
                    .get::<Owner>()
                    .is_some_and(|owner| owner.slot().get() == slot)
        })
        .unwrap()
}
```

```rust
// runner/tests/proving_match.rs
fn hero(world: &World, slot: u32) -> (StableId, EntityRef<'_>) {
    let id = MatchUnits::of(world).hero(slot);
    (id, world.entity_ref_of(id).unwrap())
}
```

## 7. Navigation's queries for one entity

`NavigationEffect::put` builds a `QueryState` to read one entity, which allocates, and `make_room` calls it once for each walker its box overlaps. `teleport` builds another and scans every projectile, and is why `navigation` imports `projectiles` (§4):

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

`put` reads the entity it holds, as `abilities/casts.rs:398` does. A teleport triggers an event, and `projectiles` observes it: an observer keeps its query state, and `navigation` names no projectile:

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

`Teleported` sits in `navigation`, which `projectiles` then uses.

## Order

The items in the order they are done; each line says why it stands where it does.

1. **§2**, **then §3's kinds rule**: §2 leaves out of the script view every entity with no `UnitEntity`, which no test unit of a match is now; the kinds rule then holds what §2 takes as given.
2. **§1** and **§6**: small, alone, each in one module or one test.
3. **§7** before §4: the observer takes `navigation`'s import of `projectiles` away, so §4's table loses a row.
4. **§3's column rule**: a new method of `RowParts`, which every column's parts implement through the tuples, and the audit's own check.
5. **§4**, the uses and the two moves out of the `abilities ↔ items` cycle: a change of state types, `UseSlots`, and a new layer rule that fails on each import it does not allow, so it goes once the items above it are in. The layer-1 cycles wait for their own design.
6. **§5** last, as slices touch the books: the largest, every builder and the load check, and no other item depends on it.
