use std::any::Any;
use std::collections::VecDeque;

use bevy_ecs::change_detection::{
    CheckChangeTicks, DetectChanges, DetectChangesMut, Ref, Tick as ChangeTick,
};
use bevy_ecs::entity::Entity;
use bevy_ecs::entity_disabling::Disabled;
use bevy_ecs::lifecycle::Remove;
use bevy_ecs::observer::On;
use bevy_ecs::query::{Allow, QueryState};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Query, ResMut};
use bevy_ecs::world::{Mut, World};
use campfire_codec::{Binary, BinaryError, Decoded, Sink};
use campfire_common::{DomainHasher, HashDomain, Meaning, StateHash, Value32};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::host_rule::HostRule;
use crate::id_allocator::IdAllocator;
use crate::prediction_started::PredictionStarted;
use crate::restoring::Restoring;
use crate::sim_state::{SimComponent, SimResource};
use crate::stable_id::StableId;
use crate::stable_id::entity_index::EntityIndex;
use crate::stable_id::state_changes::{Removal, StateChanges};
use crate::state_registry::breach::Breach;
use crate::state_registry::copy_queries::CopyQueries;
use crate::state_registry::decoded_state::DecodedState;
use crate::state_registry::error::SnapshotError;
use crate::state_registry::hash_sink::HashSink;
use crate::state_registry::state_delta::StateDelta;
use crate::state_registry::state_registry_builder::StateRegistryBuilder;
use crate::state_rewritten::StateRewritten;
use crate::unpredicted::Unpredicted;

#[cfg(feature = "internals")]
pub(crate) mod bench;
pub(crate) mod breach;
pub(crate) mod copy_queries;
mod decoded_state;
pub(crate) mod error;
mod hash_sink;
pub(crate) mod state_delta;
pub(crate) mod state_registry_builder;

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
/// and its hash cannot disagree. Its types are fixed, as its `StateRegistryBuilder` built it: a
/// record of a world's changes holds each type by its place among them.
#[derive(Resource, Debug, Clone)]
pub struct StateRegistry {
    entries: Vec<Entry>,
    foreign: Vec<ForeignCheck>,
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

/// How a restore decodes a type's section: the entities' ids, a resource it inserts at once, or
/// a component's values it keeps for the write of each entity.
#[derive(Debug, Clone, Copy)]
enum Decode {
    Entities,
    Resource(fn(&mut World, &[u8]) -> Result<(), SnapshotError>),
    Component(fn(&mut World, &[u8], &mut DecodedState) -> Result<(), SnapshotError>),
}

#[derive(Debug, Clone)]
struct Entry {
    name: &'static str,
    encode: fn(&World, &mut dyn Sink),
    decode: Decode,
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
    /// Marks the type's component of an entity changed, when it holds one.
    mark: fn(&mut World, Entity),
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

impl Entry {
    /// The entry of the built-in list of every live stable id.
    fn entities() -> Self {
        Self {
            name: ENTITIES,
            encode: encode_entities,
            decode: Decode::Entities,
            check: |_| Ok(()),
            copy: |_, _, _, _| {},
            copy_query: None,
            apply: |_, _, _| {},
            watch: |_, _| {},
            mark: |_, _| {},
            #[cfg(feature = "internals")]
            scramble: internals::Draws::scramble_entities,
            #[cfg(feature = "internals")]
            mutate: |_, _, _| false,
            #[cfg(feature = "internals")]
            layout: |_, _| {},
        }
    }

    /// The entry of the component type `C`.
    fn component<C: SimComponent>() -> Self {
        Self {
            name: C::NAME,
            encode: encode_component::<C>,
            decode: Decode::Component(decode_component::<C>),
            check: check_component::<C>,
            copy: copy_component::<C>,
            copy_query: Some(copy_query::<C>),
            apply: apply_component::<C>,
            watch: watch_component::<C>,
            mark: mark_component::<C>,
            #[cfg(feature = "internals")]
            scramble: internals::Draws::scramble_component::<C>,
            #[cfg(feature = "internals")]
            mutate: internals::Draws::mutate_component::<C>,
            #[cfg(feature = "internals")]
            layout: internals::Draws::layout_of::<C>,
        }
    }

    /// The entry of the resource type `R`.
    fn resource<R: SimResource>() -> Self {
        Self {
            name: R::NAME,
            encode: encode_resource::<R>,
            decode: Decode::Resource(decode_resource::<R>),
            check: check_resource::<R>,
            copy: copy_resource::<R>,
            copy_query: None,
            apply: apply_resource::<R>,
            watch: |_, _| {},
            mark: |_, _| {},
            #[cfg(feature = "internals")]
            scramble: internals::Draws::scramble_resource::<R>,
            #[cfg(feature = "internals")]
            mutate: internals::Draws::mutate_resource::<R>,
            #[cfg(feature = "internals")]
            layout: internals::Draws::layout_of::<R>,
        }
    }
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
    pub hash: StateTypeHash,
}

/// The hash of one state type's encoding in its own domain, a part the state hash folds after the
/// type's name under the state's domain.
pub type StateTypeHash = Value32<StateTypeHashMeaning>;

/// The meaning of a `StateTypeHash`.
#[derive(Debug)]
pub enum StateTypeHashMeaning {}

impl Meaning for StateTypeHashMeaning {
    const NAME: &'static str = "StateTypeHash";
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

    /// The data version of the snapshots this release writes and reads, raised whenever a release
    /// changes their format, a type's layout included, as Minecraft's `DataVersion` is: a snapshot
    /// of another is refused, naming both, and the converter from the version before, which saves
    /// plan (02-engine-core.md, One-way converters), comes with the first release that raises it.
    pub const DATA_VERSION: u32 = 96;

    /// The builder of a registry with the sim's own state: the entity list, the id allocator,
    /// the tick and positions.
    pub fn builder() -> StateRegistryBuilder {
        StateRegistryBuilder::new()
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

    /// Restores `snapshot` into `world`, which must hold no sim entities, and gives the restored
    /// state's hash, `hash`'s, from the snapshot's own bodies; refuses bytes that are not the
    /// canonical encoding of what they restore, or a value that breaks its type's rules, each
    /// type checked once all are decoded. Every state holds each resource, so one the snapshot
    /// records as absent is refused. On an error the world is left partly restored and should be
    /// discarded.
    pub fn restore(&self, snapshot: &[u8], world: &mut World) -> Result<StateHash, SnapshotError> {
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
        // The caches that follow the state's changes build again from the restored state.
        world.trigger(StateRewritten);
        // Postcard accepts some encodings that are not its own, such as an overlong varint, so
        // only a second encoding shows that no other bytes restore to this state: each type's
        // again, in one buffer of the largest, against its body in place. The bodies, then the
        // state's, hash as a snapshot's do.
        let mut total = DomainHasher::new(HASH_DOMAIN);
        let mut canonical = Vec::new();
        for (entry, body) in self.entries.iter().zip(&bodies) {
            canonical.clear();
            (entry.encode)(world, &mut canonical);
            if canonical != *body {
                return Err(SnapshotError::NotCanonical);
            }
            fold(&mut total, entry.name, &HashSink::of(body));
        }
        Ok(total.finish())
    }

    /// Puts each type's section of `bodies` in `world`, and checks the state: each resource as
    /// it decodes, then each entity once, with every value of it, once every section is decoded
    /// (02-engine-core.md, A restore writes each entity once).
    fn put_in(&self, world: &mut World, bodies: &[&[u8]]) -> Result<(), SnapshotError> {
        let mut decoded = DecodedState::default();
        for (entry, body) in self.entries.iter().zip(bodies) {
            match entry.decode {
                Decode::Entities => decoded.entities(decode_entities(body)?),
                Decode::Resource(decode) => decode(world, body)?,
                Decode::Component(decode) => decode(world, body, &mut decoded)?,
            }
        }
        decoded.write(world)?;

        let last_id = world.resource::<EntityIndex>().last_allocated();
        if let Some(last_id) = last_id
            && !world
                .get_resource::<IdAllocator>()
                .is_some_and(|allocator| allocator.is_issued(last_id))
        {
            return Err(SnapshotError::AllocatorBehind);
        }
        self.check(world)
    }

    /// Marks each state part of `entity` changed, as a predicting client starts to predict the
    /// unit: its parts' change ticks aged while the client held it as sent, which no pass visits,
    /// so each pass that follows changes takes it at once (02-engine-core.md,
    /// Prediction's start marks the unit's state changed).
    pub(crate) fn mark_changed(&self, world: &mut World, entity: Entity) {
        for entry in &self.entries {
            (entry.mark)(world, entity);
        }
    }

    /// Marks the state of each unit a predicting client starts to predict changed, after the
    /// commands that started it.
    pub(crate) fn predicted(
        started: On<'_, '_, PredictionStarted>,
        mut commands: Commands<'_, '_>,
    ) {
        let entity = started.entity;
        commands.queue(move |world: &mut World| {
            world.resource_scope(|world, registry: Mut<'_, Self>| {
                registry.mark_changed(world, entity);
            });
        });
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
    /// to.
    pub fn track(&self, world: &mut World, delta: &mut StateDelta) {
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

    /// Starts the record of the changes of `world`'s state again, which `track` started, and
    /// writes its whole state into `delta`, as `track` does: a load in place despawns ids and
    /// restores them again, which no delta follows, so a copy starts again from an empty world.
    pub fn track_again(&self, world: &mut World, delta: &mut StateDelta) {
        assert!(
            world.contains_resource::<StateChanges>(),
            "a world records its changes again once it records them"
        );
        let changes = StateChanges::new(world.resource::<EntityIndex>());
        world.insert_resource(changes);
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
        Self::builder().build()
    }
}

/// Adds the hash of the type `name` to a state hash's `total`.
fn fold(total: &mut DomainHasher<StateHash>, name: &str, hash: &StateTypeHash) {
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
fn take<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<Decoded<'_, T>, SnapshotError> {
    Binary::take(bytes).map_err(|error| match error {
        BinaryError::Truncated => SnapshotError::Truncated,
        BinaryError::NotCanonical => SnapshotError::NotCanonical,
        BinaryError::Malformed(malformed) => SnapshotError::Malformed(malformed),
    })
}

fn encode_entities(world: &World, sink: &mut dyn Sink) {
    let ids = world.resource::<EntityIndex>().iter().map(|(id, _)| id);
    Binary::encode_each_to(ids, sink);
}

/// The entities' stable ids, in order.
fn decode_entities(mut body: &[u8]) -> Result<Vec<StableId>, SnapshotError> {
    let mut ids: Vec<StableId> = Vec::new();
    while !body.is_empty() {
        let Decoded { value: id, rest } = take::<StableId>(body)?;
        if ids.last().is_some_and(|&previous| previous >= id) {
            return Err(SnapshotError::NotCanonical);
        }
        ids.push(id);
        body = rest;
    }
    Ok(ids)
}

fn encode_component<C: SimComponent>(world: &World, sink: &mut dyn Sink) {
    if !is_held::<C>(world) {
        return;
    }
    let index = world.resource::<EntityIndex>().iter();
    let held = index.filter_map(|(id, entity)| Some((id, world.get::<C>(entity)?)));
    Binary::encode_each_to(held, sink);
}

/// Whether an archetype of `world` holds `C`: one that none does has an empty section, with no
/// walk of the entities.
fn is_held<C: SimComponent>(world: &World) -> bool {
    world.components().component_id::<C>().is_some_and(|id| {
        world
            .archetypes()
            .iter()
            .any(|archetype| !archetype.is_empty() && archetype.contains(id))
    })
}

/// The values of `C`, each by its entity's id, in order, into `decoded`.
fn decode_component<C: SimComponent>(
    world: &mut World,
    mut body: &[u8],
    decoded: &mut DecodedState,
) -> Result<(), SnapshotError> {
    let mut values: VecDeque<(StableId, C)> = VecDeque::new();
    while !body.is_empty() {
        let Decoded {
            value: (id, component),
            rest,
        } = take::<(StableId, C)>(body)?;
        if values.back().is_some_and(|&(previous, _)| previous >= id) {
            return Err(SnapshotError::NotCanonical);
        }
        values.push_back((id, component));
        body = rest;
    }
    decoded.component(world, values);
    Ok(())
}

/// A missing resource encodes differently from an empty one: the value goes in as an `Option`.
fn encode_resource<R: SimResource>(world: &World, sink: &mut dyn Sink) {
    Binary::encode_to(&world.get_resource::<R>(), sink);
}

fn decode_resource<R: SimResource>(world: &mut World, body: &[u8]) -> Result<(), SnapshotError> {
    let Decoded { value, rest } = take::<Option<R>>(body)?;
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
        let Decoded {
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

/// Marks `entity`'s `C` changed, when it holds one: a change tick its passes take as a write.
fn mark_component<C: SimComponent>(world: &mut World, entity: Entity) {
    let Some(id) = world.component_id::<C>() else {
        return;
    };
    if let Ok(mut held) = world.get_entity_mut(entity)
        && let Ok(mut part) = held.get_mut_by_id(id)
    {
        part.set_changed();
    }
}

fn watch_component<C: SimComponent>(world: &mut World, entry: u16) {
    world.add_observer(
        move |removed: On<'_, '_, Remove<C>>,
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
        None => Binary::encode_into(&None::<&R>, out),
        Some(value)
            if copying
                .since
                .is_none_or(|since| value.last_changed().is_newer_than(since, copying.now)) =>
        {
            Binary::encode_into(&Some(&*value), out);
        }
        Some(_) => {}
    }
}

fn apply_resource<R: SimResource>(world: &mut World, body: &[u8], pass: Pass) {
    if pass == Pass::Removals || body.is_empty() {
        return;
    }
    let Decoded { value, rest } = take::<Option<R>>(body).expect("a delta decodes");
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
    if !is_held::<C>(world) {
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
