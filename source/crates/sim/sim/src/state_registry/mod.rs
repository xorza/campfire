use std::any::Any;

use bevy_ecs::change_detection::{CheckChangeTicks, DetectChanges, Ref, Tick as ChangeTick};
use bevy_ecs::entity_disabling::Disabled;
use bevy_ecs::lifecycle::Remove;
use bevy_ecs::observer::On;
use bevy_ecs::query::{Allow, QueryState};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, ResMut};
use bevy_ecs::world::{Mut, World};
use campfire_codec::{Binary, BinaryError, Sink, Taken};
use campfire_common::{DomainHasher, HashDomain, Meaning, StateHash, Value32};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::host_rule::HostRule;
use crate::id_allocator::IdAllocator;
use crate::position::Position;
use crate::restoring::Restoring;
use crate::sim_state::{SimComponent, SimResource};
use crate::sim_tick::SimTick;
use crate::stable_id::StableId;
use crate::stable_id::entity_index::EntityIndex;
use crate::stable_id::state_changes::{Removal, StateChanges};
use crate::state_registry::breach::Breach;
use crate::state_registry::copy_queries::CopyQueries;
use crate::state_registry::error::SnapshotError;
use crate::state_registry::hash_sink::HashSink;
use crate::state_registry::state_delta::StateDelta;
use crate::unpredicted::Unpredicted;

#[cfg(feature = "internals")]
pub(crate) mod bench;
pub(crate) mod breach;
pub(crate) mod copy_queries;
pub(crate) mod error;
mod hash_sink;
pub(crate) mod state_delta;

/// Starts the combined hash, so no other BLAKE3 use can produce the same state hash.
const HASH_DOMAIN: HashDomain<StateHash> = HashDomain::new("campfire/state/v1");
/// The domain of `StateRegistry::digest`.
const DIGEST_DOMAIN: HashDomain<StateHash> = HashDomain::new("campfire/digest/v1");
/// Starts every snapshot, so other bytes are refused at once.
const SNAPSHOT_TAG: &[u8] = b"campfire/snapshot/v1";
/// The built-in type listing every live stable id, so an entity with no registered component
/// still survives a snapshot.
const ENTITIES: &str = "sim.entities";
const VERSION_BYTES: usize = size_of::<u32>();
const NAME_LEN_BYTES: usize = size_of::<u32>();
const BODY_LEN_BYTES: usize = size_of::<u64>();

/// The types that make up the simulated state, and what is done with them: hash, snapshot,
/// restore, and copy what changed to another world. Each type walks entities in stable-id order,
/// and a snapshot's section for a type holds exactly the bytes its hash consumes, so a snapshot
/// and its hash cannot disagree.
#[derive(Resource, Debug, Clone)]
pub struct StateRegistry {
    entries: Vec<Entry>,
    foreign: Vec<ForeignCheck>,
    /// Whether a world's changes are recorded by it: the record holds each type by its place in
    /// `entries`, so no type joins them from then on.
    tracking: bool,
}

/// A rule of a registered state type that a capability other than its owner knows, checked as
/// the type's own check is: a rule of the type `name`, or a part or a kind its holders need,
/// which names its own error.
#[derive(Debug, Clone)]
enum ForeignCheck {
    Rule {
        name: &'static str,
        check: fn(&World) -> Result<(), Breach>,
    },
    Shape(fn(&World) -> Result<(), SnapshotError>),
}

#[derive(Debug, Clone)]
struct Entry {
    name: &'static str,
    encode: fn(&World, &mut dyn Sink),
    decode: fn(&mut World, &[u8]) -> Result<(), SnapshotError>,
    /// Whether every value of the type keeps its rules, once everything is decoded; the error
    /// that names the first that breaks them.
    check: fn(&World) -> Result<(), SnapshotError>,
    /// Writes the type's section of a `StateDelta`, through the query `copy_query` made.
    copy: fn(&mut World, &Copying<'_>, Option<&mut CopyQuery>, &mut Vec<u8>),
    /// Makes the query a component type copies its values through, once, as a world starts
    /// recording its changes; none for a resource or the entity list.
    copy_query: Option<fn(&mut World) -> Box<CopyQuery>>,
    /// Applies the type's section of a `StateDelta`, in one of its two passes.
    apply: fn(&mut World, &[u8], Pass),
    /// Records each removal of the type's component, at its place in the registry, in the
    /// world's `StateChanges`.
    watch: fn(&mut World, u16),
    #[cfg(feature = "internals")]
    scramble: fn(&mut internals::Draws, &mut World) -> bool,
    /// Changes where a value of the type sits, as a mutation says.
    #[cfg(feature = "internals")]
    mutate: fn(&mut internals::Draws, &mut World, internals::Mutation) -> bool,
    /// Writes into the buffer the encodings of values of the type the draws make: what its
    /// layout fingerprint digests.
    #[cfg(feature = "internals")]
    layout: fn(&mut internals::Draws, &mut Vec<u8>),
}

/// What a type's copy takes: the world's change tick at the last copy, none for a first copy of
/// every value, and now; the ids gained since, whose every value it takes; and the ids whose
/// component of the type was removed since.
#[derive(Debug)]
struct Copying<'a> {
    since: Option<ChangeTick>,
    now: ChangeTick,
    gained: &'a [StableId],
    removed: &'a [Removal],
}

/// The passes of an apply: every value first, then every removal, so a component that requires
/// another, which an insert adds, never puts back one the copied world lacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pass {
    Values,
    Removals,
}

/// A type's copy query, as a world's `CopyQueries` holds it.
type CopyQuery = dyn Any + Send + Sync;

/// What a component type's copy reads: each entity that holds it, the disabled and the
/// unpredicted among them, as the entity index lists them all, with its stable id.
type Holders<C> =
    QueryState<(&'static StableId, Ref<'static, C>), (Allow<Disabled>, Allow<Unpredicted>)>;

/// The hash of one registered type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeHash {
    pub name: &'static str,
    pub hash: PartHash,
}

/// The BLAKE3 of one state type's encoding, a part the state hash folds after the type's name
/// under the state's domain, which a part needs no domain of its own for.
pub type PartHash = Value32<PartHashMeaning>;

/// The meaning of a `PartHash`.
#[derive(Debug)]
pub enum PartHashMeaning {}

impl Meaning for PartHashMeaning {
    const NAME: &'static str = "PartHash";
}

/// One type's part of a snapshot, and the bytes after it.
#[derive(Debug)]
struct Section<'a> {
    name: &'a [u8],
    body: &'a [u8],
    rest: &'a [u8],
}

/// Bytes split off the front, and the bytes after them.
#[derive(Debug)]
struct Split<'a> {
    head: &'a [u8],
    rest: &'a [u8],
}

impl StateRegistry {
    /// The most bytes a snapshot's file may hold to be read whole: a match's whole state, a few
    /// mebibytes for the test modes' units, so a gibibyte is far past one and refuses a file
    /// of another kind before a restore or a verifier holds it in memory.
    pub const MAX_SNAPSHOT_LEN: usize = 1 << 30;

    /// The data version of the snapshots this release writes and reads, raised whenever a
    /// release changes their format, a type's layout included, as Minecraft's `DataVersion` is:
    /// a snapshot of another is refused, naming both, and the converter from the version before,
    /// which design 02's Saves plans, comes with the first release that raises it.
    pub const DATA_VERSION: u32 = 25;

    /// A registry with the sim's own state: the entity list, the id allocator, the tick and
    /// positions.
    pub fn new() -> Self {
        let mut registry = Self {
            entries: Vec::new(),
            foreign: Vec::new(),
            tracking: false,
        };
        let entities = Entry {
            name: ENTITIES,
            encode: encode_entities,
            decode: decode_entities,
            check: |_| Ok(()),
            copy: |_, _, _, _| {},
            copy_query: None,
            apply: |_, _, _| {},
            watch: |_, _| {},
            #[cfg(feature = "internals")]
            scramble: internals::Draws::scramble_entities,
            #[cfg(feature = "internals")]
            mutate: |_, _, _| false,
            #[cfg(feature = "internals")]
            layout: |_, _| {},
        };
        registry.register(entities);
        registry.register_resource::<IdAllocator>();
        registry.register_resource::<SimTick>();
        registry.register_component::<Position>();
        registry
    }

    pub fn register_component<C: SimComponent>(&mut self) {
        self.register(Entry {
            name: C::NAME,
            encode: encode_component::<C>,
            decode: decode_component::<C>,
            check: check_component::<C>,
            copy: copy_component::<C>,
            copy_query: Some(copy_query::<C>),
            apply: apply_component::<C>,
            watch: watch_component::<C>,
            #[cfg(feature = "internals")]
            scramble: internals::Draws::scramble_component::<C>,
            #[cfg(feature = "internals")]
            mutate: internals::Draws::mutate_component::<C>,
            #[cfg(feature = "internals")]
            layout: internals::Draws::layout_of::<C>,
        });
    }

    /// Requires that every entity holding `H` holds `C`, both registered types, as a restore
    /// checks: a part a kind's entities all hold, or one another part of an entity always comes
    /// with. A snapshot that breaks it fails as `SnapshotError::Lacks`.
    pub fn require<H: SimComponent, C: SimComponent>(&mut self) {
        self.assert_registered::<C>();
        self.foreign.push(ForeignCheck::Shape(holders_hold::<H, C>));
    }

    /// Requires that `C`, a registered type of a crate that knows no kinds, sits on the entities
    /// of `H` alone, as its own `Host` cannot say. A snapshot that breaks it fails as
    /// `SnapshotError::WrongHost`.
    pub fn host<C: SimComponent, H: HostRule>(&mut self) {
        self.assert_registered::<C>();
        self.foreign
            .push(ForeignCheck::Shape(holders_hosted::<C, H>));
    }

    /// Adds `check` to the restore checks of `C`, a registered type: a rule of it that a
    /// capability other than its owner knows, which `C::check` cannot see. A breach on an entity
    /// fails as `SnapshotError::Invalid` of that entity; one of the whole match, of none.
    pub fn add_check<C: SimComponent>(&mut self, check: fn(&World) -> Result<(), Breach>) {
        self.assert_registered::<C>();
        self.foreign.push(ForeignCheck::Rule {
            name: C::NAME,
            check,
        });
    }

    fn assert_registered<C: SimComponent>(&self) {
        assert!(
            self.entries.iter().any(|entry| entry.name == C::NAME),
            "{} is registered before a check is added to it",
            C::NAME
        );
    }

    /// Registers `R` as a resource every state holds, as the sim's own are: the match's install
    /// inserts it, and a snapshot that records it absent does not restore, so no system finds it
    /// missing.
    pub fn register_resource<R: SimResource>(&mut self) {
        self.register(Entry {
            name: R::NAME,
            encode: encode_resource::<R>,
            decode: decode_resource::<R>,
            check: check_resource::<R>,
            copy: copy_resource::<R>,
            copy_query: None,
            apply: apply_resource::<R>,
            watch: |_, _| {},
            #[cfg(feature = "internals")]
            scramble: internals::Draws::scramble_resource::<R>,
            #[cfg(feature = "internals")]
            mutate: internals::Draws::mutate_resource::<R>,
            #[cfg(feature = "internals")]
            layout: internals::Draws::layout_of::<R>,
        });
    }

    /// Whether `C` is registered.
    pub fn holds<C: SimComponent>(&self) -> bool {
        self.entries
            .binary_search_by(|held| held.name.cmp(C::NAME))
            .is_ok()
    }

    /// The hash of `bytes` under a domain of its own: a digest of values that a test or a check
    /// derives from the state, which no state hash can equal.
    pub fn digest(bytes: &[u8]) -> StateHash {
        let mut hasher = DomainHasher::new(DIGEST_DOMAIN);
        hasher.update(bytes);
        hasher.finish()
    }

    pub fn hash(&self, world: &World) -> StateHash {
        self.combine(world, None)
    }

    /// The state hash, and each type's hash into `per_type`, which is cleared first.
    pub fn hash_by_type(&self, world: &World, per_type: &mut Vec<TypeHash>) -> StateHash {
        per_type.clear();
        per_type.reserve_exact(self.entries.len());
        self.combine(world, Some(per_type))
    }

    /// Writes the snapshot of `world` into `out`, which is cleared first: the tag, the `u32` data
    /// version, then for each type in name order a `u32` name length, the name, a `u64` body
    /// length and the body, all little-endian. Its state hash, `hash`'s, from the bodies it
    /// wrote, so a checkpoint encodes its state once.
    pub fn snapshot(&self, world: &World, out: &mut Vec<u8>) -> StateHash {
        out.clear();
        out.extend_from_slice(SNAPSHOT_TAG);
        out.extend_from_slice(&Self::DATA_VERSION.to_le_bytes());
        let mut total = DomainHasher::new(HASH_DOMAIN);
        for entry in &self.entries {
            out.extend_from_slice(&name_len(entry.name).to_le_bytes());
            out.extend_from_slice(entry.name.as_bytes());
            let len_at = out.len();
            out.extend_from_slice(&[0; BODY_LEN_BYTES]);
            (entry.encode)(world, out);
            let body = &out[len_at + BODY_LEN_BYTES..];
            fold(&mut total, entry.name, &HashSink::of(body));
            let body_len = u64::try_from(body.len()).expect("snapshot above 2⁶⁴ bytes");
            out[len_at..len_at + BODY_LEN_BYTES].copy_from_slice(&body_len.to_le_bytes());
        }
        total.finish()
    }

    /// Restores `snapshot` into `world`, which must hold no sim entities, and refuses bytes that
    /// are not the canonical encoding of what they restore, or a value that breaks its type's
    /// rules, each type checked once all are decoded. Every state holds each resource, so one
    /// the snapshot records as absent is refused. On an error the world is left partly restored
    /// and should be discarded.
    pub fn restore(&self, snapshot: &[u8], world: &mut World) -> Result<(), SnapshotError> {
        assert!(
            world
                .get_resource::<EntityIndex>()
                .is_none_or(|index| index.iter().next().is_none()),
            "restore needs a world without sim entities"
        );
        world.init_resource::<EntityIndex>();

        let rest = snapshot
            .strip_prefix(SNAPSHOT_TAG)
            .ok_or(SnapshotError::NotSnapshot)?;
        let (version, mut rest) = rest
            .split_first_chunk::<VERSION_BYTES>()
            .ok_or(SnapshotError::Truncated)?;
        let version = u32::from_le_bytes(*version);
        if version != Self::DATA_VERSION {
            return Err(SnapshotError::OtherVersion {
                found: version,
                read: Self::DATA_VERSION,
            });
        }
        let mut bodies = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            // A snapshot that ends where the registry expects a type lacks that type.
            if rest.is_empty() {
                return Err(SnapshotError::TypesDiffer);
            }
            let section = read_section(rest)?;
            if section.name != entry.name.as_bytes() {
                return Err(SnapshotError::TypesDiffer);
            }
            bodies.push(section.body);
            rest = section.rest;
        }
        if !rest.is_empty() {
            // A whole section after the last type is a type the registry lacks.
            return Err(if read_section(rest).is_ok() {
                SnapshotError::TypesDiffer
            } else {
                SnapshotError::Trailing
            });
        }

        world.insert_resource(Restoring);
        let restored = self.put_in(world, &bodies);
        world.remove_resource::<Restoring>();
        restored?;
        // Postcard accepts some encodings that are not its own, such as an overlong varint, so
        // only a second encoding shows that no other bytes restore to this state.
        let mut canonical = Vec::with_capacity(snapshot.len());
        self.snapshot(world, &mut canonical);
        if canonical != snapshot {
            return Err(SnapshotError::NotCanonical);
        }
        Ok(())
    }

    /// Puts each type's section of `bodies` in `world`, the entities first, and checks the
    /// state.
    fn put_in(&self, world: &mut World, bodies: &[&[u8]]) -> Result<(), SnapshotError> {
        // The entities come first, so components find them whatever the order of the names.
        for (entry, body) in self.entries.iter().zip(bodies) {
            if entry.name == ENTITIES {
                (entry.decode)(world, body)?;
            }
        }
        for (entry, body) in self.entries.iter().zip(bodies) {
            if entry.name != ENTITIES {
                (entry.decode)(world, body)?;
            }
        }

        let last_id = world.resource::<EntityIndex>().last_allocated();
        if let Some(last_id) = last_id
            && !world
                .get_resource::<IdAllocator>()
                .is_some_and(|allocator| allocator.issued(last_id))
        {
            return Err(SnapshotError::AllocatorBehind);
        }
        self.check(world)
    }

    /// Checks the state of `world` as a restore checks it: each type's own rules, then the rules
    /// of it that other modules know, with its host among them.
    pub fn check(&self, world: &World) -> Result<(), SnapshotError> {
        for entry in &self.entries {
            (entry.check)(world)?;
        }
        for foreign in &self.foreign {
            match *foreign {
                ForeignCheck::Rule { name, check } => {
                    check(world).map_err(|breach| SnapshotError::Invalid {
                        ty: name,
                        id: match breach {
                            Breach::Entity(entity) => Some(
                                *world
                                    .get::<StableId>(entity)
                                    .expect("a breach names an entity of the match"),
                            ),
                            Breach::Match => None,
                        },
                    })?;
                }
                ForeignCheck::Shape(check) => check(world)?,
            }
        }
        Ok(())
    }

    /// Starts recording the changes of the state of `world` for copies of it, and writes its
    /// whole state into `delta`, which an empty world takes as the base the later changes apply
    /// to. The registry takes no more type from then on, as the record holds each by its place.
    pub fn track(&mut self, world: &mut World, delta: &mut StateDelta) {
        self.tracking = true;
        assert!(
            !world.contains_resource::<StateChanges>(),
            "a world records its changes once"
        );
        let changes = StateChanges::new(world.resource::<EntityIndex>());
        world.insert_resource(changes);
        world.add_observer(
            |check: On<'_, '_, CheckChangeTicks>, mut changes: ResMut<'_, StateChanges>| {
                changes.check_ticks(check.present_tick());
            },
        );
        // A query names the marker of the unpredicted, which only a predicting world registers.
        world.register_component::<Unpredicted>();
        let mut queries = CopyQueries::default();
        for (at, entry) in self.entries.iter().enumerate() {
            (entry.watch)(world, u16::try_from(at).expect("the types fit u16"));
            queries.push(entry.copy_query.map(|make| make(world)));
        }
        world.insert_resource(queries);
        self.changes(world, delta);
    }

    /// Writes into `delta`, which is cleared first, the state of `world` that changed since the
    /// last copy, or since `track`, and starts recording again. A value counts as changed when
    /// its change tick is newer than the last copy's, so every write of state marks its change.
    pub fn changes(&self, world: &mut World, delta: &mut StateDelta) {
        let now = world.change_tick();
        world.resource_scope(|world, mut changes: Mut<'_, StateChanges>| {
            world.resource_scope(|world, mut queries: Mut<'_, CopyQueries>| {
                changes.settle(world.resource::<EntityIndex>());
                delta.clear();
                delta.lost.extend_from_slice(changes.lost());
                delta.gained.extend_from_slice(changes.gained());
                for (at, entry) in self.entries.iter().enumerate() {
                    let copying = Copying {
                        since: changes.since(),
                        now,
                        gained: &delta.gained,
                        removed: changes.removed(u16::try_from(at).expect("the types fit u16")),
                    };
                    (entry.copy)(world, &copying, queries.get_mut(at), &mut delta.bytes);
                    delta.end_section();
                }
            });
            changes.restart(now);
        });
        // Every write after the copy is newer than its tick.
        world.increment_change_tick();
    }

    /// Makes `world`, a copy of another world's state, follow the changes `delta` holds: it
    /// despawns the ids lost, spawns those gained, writes each changed value, then removes each
    /// removed component.
    pub fn apply(&self, delta: &StateDelta, world: &mut World) {
        assert_eq!(
            delta.sections(),
            self.entries.len(),
            "a delta of this registry's types"
        );
        for &id in &delta.lost {
            let entity = world
                .resource::<EntityIndex>()
                .get(id)
                .expect("a copy holds each id its world lost");
            world.despawn(entity);
        }
        for &id in &delta.gained {
            world.spawn(id);
        }
        for pass in [Pass::Values, Pass::Removals] {
            for (at, entry) in self.entries.iter().enumerate() {
                (entry.apply)(world, delta.section(at), pass);
            }
        }
    }

    fn register(&mut self, entry: Entry) {
        let name = entry.name;
        assert!(
            !self.tracking,
            "state type {name:?} registered after its world's changes are tracked"
        );
        let Err(at) = self.entries.binary_search_by(|held| held.name.cmp(name)) else {
            panic!("state type {name:?} registered twice");
        };
        self.entries.insert(at, entry);
    }

    fn combine(&self, world: &World, mut per_type: Option<&mut Vec<TypeHash>>) -> StateHash {
        let mut total = DomainHasher::new(HASH_DOMAIN);
        let mut sink = HashSink::new();
        for entry in &self.entries {
            (entry.encode)(world, &mut sink);
            let hash = sink.finish();
            fold(&mut total, entry.name, &hash);
            if let Some(per_type) = per_type.as_deref_mut() {
                per_type.push(TypeHash {
                    name: entry.name,
                    hash,
                });
            }
        }
        total.finish()
    }
}

impl Default for StateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Adds the hash of the type `name` to a state hash's `total`.
fn fold(total: &mut DomainHasher<StateHash>, name: &str, hash: &PartHash) {
    total
        .update(&name_len(name).to_le_bytes())
        .update(name.as_bytes())
        .update(hash.as_bytes());
}

fn name_len(name: &str) -> u32 {
    u32::try_from(name.len()).expect("state type name above 4 GiB")
}

fn read_section(bytes: &[u8]) -> Result<Section<'_>, SnapshotError> {
    let (name_len, rest) = bytes
        .split_at_checked(NAME_LEN_BYTES)
        .ok_or(SnapshotError::Truncated)?;
    let name = split_len(
        rest,
        u32::from_le_bytes(name_len.try_into().expect("u32 bytes")).into(),
    )?;
    let (body_len, rest) = name
        .rest
        .split_at_checked(BODY_LEN_BYTES)
        .ok_or(SnapshotError::Truncated)?;
    let body = split_len(
        rest,
        u64::from_le_bytes(body_len.try_into().expect("u64 bytes")),
    )?;
    Ok(Section {
        name: name.head,
        body: body.head,
        rest: body.rest,
    })
}

/// Splits off `len` bytes; a length beyond memory cannot be present, so it is a truncation too.
fn split_len(bytes: &[u8], len: u64) -> Result<Split<'_>, SnapshotError> {
    let (head, rest) = usize::try_from(len)
        .ok()
        .and_then(|len| bytes.split_at_checked(len))
        .ok_or(SnapshotError::Truncated)?;
    Ok(Split { head, rest })
}

/// Reads one value and the bytes after it.
fn take<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<Taken<'_, T>, SnapshotError> {
    Binary::take(bytes).map_err(|error| match error {
        BinaryError::Truncated => SnapshotError::Truncated,
        BinaryError::NotCanonical => SnapshotError::NotCanonical,
        BinaryError::Malformed(_) => SnapshotError::Malformed(error),
    })
}

fn encode_entities(world: &World, sink: &mut dyn Sink) {
    let ids = world.resource::<EntityIndex>().iter().map(|(id, _)| id);
    Binary::encode_each_to(ids, sink);
}

fn decode_entities(world: &mut World, mut body: &[u8]) -> Result<(), SnapshotError> {
    let mut previous = None;
    while !body.is_empty() {
        let Taken { value: id, rest } = take::<StableId>(body)?;
        if previous.is_some_and(|previous| previous >= id) {
            return Err(SnapshotError::NotCanonical);
        }
        world.spawn(id);
        previous = Some(id);
        body = rest;
    }
    Ok(())
}

fn encode_component<C: SimComponent>(world: &World, sink: &mut dyn Sink) {
    if !held::<C>(world) {
        return;
    }
    let index = world.resource::<EntityIndex>().iter();
    let held = index.filter_map(|(id, entity)| Some((id, world.get::<C>(entity)?)));
    Binary::encode_each_to(held, sink);
}

/// Whether an archetype of `world` holds `C`: one that none does has an empty section, with no
/// walk of the entities.
fn held<C: SimComponent>(world: &World) -> bool {
    world.components().component_id::<C>().is_some_and(|id| {
        world
            .archetypes()
            .iter()
            .any(|archetype| !archetype.is_empty() && archetype.contains(id))
    })
}

fn decode_component<C: SimComponent>(
    world: &mut World,
    mut body: &[u8],
) -> Result<(), SnapshotError> {
    let mut previous = None;
    while !body.is_empty() {
        let Taken {
            value: (id, component),
            rest,
        } = take::<(StableId, C)>(body)?;
        if previous.is_some_and(|previous| previous >= id) {
            return Err(SnapshotError::NotCanonical);
        }
        let entity = world
            .resource::<EntityIndex>()
            .get(id)
            .ok_or(SnapshotError::UnknownEntity)?;
        world.entity_mut(entity).insert(component);
        previous = Some(id);
        body = rest;
    }
    Ok(())
}

/// A missing resource encodes differently from an empty one: the value goes in as an `Option`.
fn encode_resource<R: SimResource>(world: &World, sink: &mut dyn Sink) {
    Binary::encode_to(&world.get_resource::<R>(), sink);
}

fn decode_resource<R: SimResource>(world: &mut World, body: &[u8]) -> Result<(), SnapshotError> {
    let Taken { value, rest } = take::<Option<R>>(body)?;
    if !rest.is_empty() {
        return Err(SnapshotError::Trailing);
    }
    world.insert_resource(value.ok_or(SnapshotError::Missing(R::NAME))?);
    Ok(())
}

fn copy_query<C: SimComponent>(world: &mut World) -> Box<CopyQuery> {
    let holders: Holders<C> = world.query_filtered();
    Box::new(holders)
}

fn copy_component<C: SimComponent>(
    world: &mut World,
    copying: &Copying<'_>,
    query: Option<&mut CopyQuery>,
    out: &mut Vec<u8>,
) {
    let holders = query
        .and_then(|query| query.downcast_mut::<Holders<C>>())
        .expect("a component type copies through its own query");
    let changed = holders.iter(world).filter(|(id, value)| {
        let changed = copying
            .since
            .is_none_or(|since| value.last_changed().is_newer_than(since, copying.now));
        changed || copying.gained.binary_search(id).is_ok()
    });
    let index = world.resource::<EntityIndex>();
    let removed = copying.removed.iter().filter(|removal| {
        index
            .get(removal.id)
            .is_some_and(|entity| !world.entity(entity).contains::<C>())
    });
    let values = changed.map(|(&id, value)| (id, Some(value.into_inner())));
    Binary::encode_each_to(values.chain(removed.map(|removal| (removal.id, None))), out);
}

fn apply_component<C: SimComponent>(world: &mut World, mut body: &[u8], pass: Pass) {
    while !body.is_empty() {
        let Taken {
            value: (id, value),
            rest,
        } = take::<(StableId, Option<C>)>(body).expect("a delta decodes");
        body = rest;
        let entity = world
            .resource::<EntityIndex>()
            .get(id)
            .expect("a copy holds each id a delta names");
        match (pass, value) {
            // An insert over a held value puts back each component it requires that is gone,
            // which the copied world may have removed in an earlier delta.
            (Pass::Values, Some(value)) => {
                let mut unit = world.entity_mut(entity);
                if unit.contains::<C>() {
                    unit.modify_component(|held: &mut C| *held = value);
                } else {
                    unit.insert(value);
                }
            }
            (Pass::Removals, None) => drop(world.entity_mut(entity).remove::<C>()),
            (Pass::Values, None) | (Pass::Removals, Some(_)) => {}
        }
    }
}

fn watch_component<C: SimComponent>(world: &mut World, entry: u16) {
    world.add_observer(
        move |removed: On<'_, '_, Remove, C>,
              ids: Query<'_, '_, &StableId>,
              mut changes: ResMut<'_, StateChanges>| {
            if let Ok(&id) = ids.get(removed.entity) {
                changes.remove(Removal { entry, id });
            }
        },
    );
}

fn copy_resource<R: SimResource>(
    world: &mut World,
    copying: &Copying<'_>,
    _: Option<&mut CopyQuery>,
    out: &mut Vec<u8>,
) {
    match world.get_resource_ref::<R>() {
        None => Binary::encode_to(&None::<&R>, out),
        Some(value)
            if copying
                .since
                .is_none_or(|since| value.last_changed().is_newer_than(since, copying.now)) =>
        {
            Binary::encode_to(&Some(&*value), out);
        }
        Some(_) => {}
    }
}

fn apply_resource<R: SimResource>(world: &mut World, body: &[u8], pass: Pass) {
    if pass == Pass::Removals || body.is_empty() {
        return;
    }
    let Taken { value, rest } = take::<Option<R>>(body).expect("a delta decodes");
    debug_assert!(
        rest.is_empty(),
        "a resource's section holds its value alone"
    );
    match value {
        Some(value) => world.insert_resource(value),
        None => drop(world.remove_resource::<R>()),
    }
}

fn check_component<C: SimComponent>(world: &World) -> Result<(), SnapshotError> {
    if !held::<C>(world) {
        return Ok(());
    }
    for (id, entity) in world.resource::<EntityIndex>().iter() {
        let Some(component) = world.get::<C>(entity) else {
            continue;
        };
        if !C::Host::holds(world, entity) {
            return Err(SnapshotError::WrongHost { ty: C::NAME, id });
        }
        if !component.check(world, entity) {
            return Err(SnapshotError::Invalid {
                ty: C::NAME,
                id: Some(id),
            });
        }
    }
    Ok(())
}

/// Whether every entity that holds `C` is of `H`; the error that names the first that is not.
fn holders_hosted<C: SimComponent, H: HostRule>(world: &World) -> Result<(), SnapshotError> {
    let index = world.resource::<EntityIndex>();
    match index
        .iter()
        .find(|&(_, entity)| world.get::<C>(entity).is_some() && !H::holds(world, entity))
    {
        Some((id, _)) => Err(SnapshotError::WrongHost { ty: C::NAME, id }),
        None => Ok(()),
    }
}

/// Whether every entity that holds `H` holds `C`; the error that names the first that does not.
fn holders_hold<H: SimComponent, C: SimComponent>(world: &World) -> Result<(), SnapshotError> {
    let index = world.resource::<EntityIndex>();
    let lacks = index.iter().find(|&(_, entity)| {
        let held = world.entity(entity);
        held.contains::<H>() && !held.contains::<C>()
    });
    match lacks {
        Some((id, _)) => Err(SnapshotError::Lacks {
            holder: H::NAME,
            needs: C::NAME,
            id,
        }),
        None => Ok(()),
    }
}

fn check_resource<R: SimResource>(world: &World) -> Result<(), SnapshotError> {
    if world
        .get_resource::<R>()
        .is_none_or(|resource| resource.check(world))
    {
        Ok(())
    } else {
        Err(SnapshotError::Invalid {
            ty: R::NAME,
            id: None,
        })
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals;

#[cfg(test)]
mod tests;
