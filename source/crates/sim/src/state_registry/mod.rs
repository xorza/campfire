use std::fmt;

use bevy_ecs::world::World;
use blake3::Hasher;
use serde::de::DeserializeOwned;

use crate::entity_index::EntityIndex;
use crate::id_allocator::IdAllocator;
use crate::position::Position;
use crate::sim_state::{SimComponent, SimResource};
use crate::sim_tick::SimTick;
use crate::stable_id::StableId;
use crate::state_registry::error::SnapshotError;
use crate::state_registry::writer::{Sink, write};

#[cfg(feature = "bench")]
pub(crate) mod bench;
pub(crate) mod error;
mod writer;

/// Starts the combined hash, so no other BLAKE3 use can produce the same state hash.
const HASH_DOMAIN: &[u8] = b"campfire/state/v1";
/// Starts every snapshot, so other bytes are refused at once.
const SNAPSHOT_TAG: &[u8] = b"campfire/snapshot/v1";
/// The built-in type listing every live stable id, so an entity with no registered component
/// still survives a snapshot.
const ENTITIES: &str = "sim.entities";
const NAME_LEN_BYTES: usize = size_of::<u32>();
const BODY_LEN_BYTES: usize = size_of::<u64>();

/// The types that make up the simulated state, and the three things done with them: hash,
/// snapshot, restore. Each type walks entities in stable-id order, and a snapshot's section for a
/// type holds exactly the bytes its hash consumes, so a snapshot and its hash cannot disagree.
#[derive(Debug)]
pub struct StateRegistry {
    entries: Vec<Entry>,
}

#[derive(Debug)]
struct Entry {
    name: &'static str,
    encode: fn(&World, &mut dyn Sink),
    decode: fn(&mut World, &[u8]) -> Result<(), SnapshotError>,
}

/// The hash of one registered type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeHash {
    pub name: &'static str,
    pub hash: [u8; 32],
}

/// The hash of the whole simulated state. It writes as 64 lowercase hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StateHash([u8; 32]);

impl StateHash {
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
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

/// A decoded value, and the bytes after it.
#[derive(Debug)]
struct Taken<'a, T> {
    value: T,
    rest: &'a [u8],
}

impl StateRegistry {
    /// A registry with the sim's own state: the entity list, the id allocator, the tick and
    /// positions.
    pub fn new() -> StateRegistry {
        let mut registry = StateRegistry {
            entries: Vec::new(),
        };
        registry.register(ENTITIES, encode_entities, decode_entities);
        registry.register_resource::<IdAllocator>();
        registry.register_resource::<SimTick>();
        registry.register_component::<Position>();
        registry
    }

    pub fn register_component<C: SimComponent>(&mut self) {
        self.register(C::NAME, encode_component::<C>, decode_component::<C>);
    }

    pub fn register_resource<R: SimResource>(&mut self) {
        self.register(R::NAME, encode_resource::<R>, decode_resource::<R>);
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

    /// Writes the snapshot of `world` into `out`, which is cleared first: the tag, then for each
    /// type in name order a `u32` name length, the name, a `u64` body length and the body.
    pub fn snapshot(&self, world: &World, out: &mut Vec<u8>) {
        out.clear();
        out.extend_from_slice(SNAPSHOT_TAG);
        for entry in &self.entries {
            out.extend_from_slice(&name_len(entry.name).to_le_bytes());
            out.extend_from_slice(entry.name.as_bytes());
            let len_at = out.len();
            out.extend_from_slice(&[0; BODY_LEN_BYTES]);
            (entry.encode)(world, out);
            let body_len = u64::try_from(out.len() - len_at - BODY_LEN_BYTES)
                .expect("snapshot above 2⁶⁴ bytes");
            out[len_at..len_at + BODY_LEN_BYTES].copy_from_slice(&body_len.to_le_bytes());
        }
    }

    /// Restores `snapshot` into `world`, which must hold no sim entities. On an error the world is
    /// left partly restored and should be discarded.
    pub fn restore(&self, snapshot: &[u8], world: &mut World) -> Result<(), SnapshotError> {
        assert!(
            world
                .get_resource::<EntityIndex>()
                .is_none_or(|index| index.iter().next().is_none()),
            "restore needs a world without sim entities"
        );
        world.init_resource::<EntityIndex>();

        let mut rest = snapshot
            .strip_prefix(SNAPSHOT_TAG)
            .ok_or(SnapshotError::NotSnapshot)?;
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

        // The entities come first, so components find them whatever the order of the names.
        for (entry, body) in self.entries.iter().zip(&bodies) {
            if entry.name == ENTITIES {
                (entry.decode)(world, body)?;
            }
        }
        for (entry, body) in self.entries.iter().zip(&bodies) {
            if entry.name != ENTITIES {
                (entry.decode)(world, body)?;
            }
        }

        let last_id = world
            .resource::<EntityIndex>()
            .iter()
            .last()
            .map(|(id, _)| id);
        if let Some(last_id) = last_id
            && !world
                .get_resource::<IdAllocator>()
                .is_some_and(|allocator| allocator.issued(last_id))
        {
            return Err(SnapshotError::AllocatorBehind);
        }
        Ok(())
    }

    fn register(
        &mut self,
        name: &'static str,
        encode: fn(&World, &mut dyn Sink),
        decode: fn(&mut World, &[u8]) -> Result<(), SnapshotError>,
    ) {
        let Err(at) = self.entries.binary_search_by(|entry| entry.name.cmp(name)) else {
            panic!("state type {name:?} registered twice");
        };
        self.entries.insert(
            at,
            Entry {
                name,
                encode,
                decode,
            },
        );
    }

    fn combine(&self, world: &World, mut per_type: Option<&mut Vec<TypeHash>>) -> StateHash {
        let mut total = Hasher::new();
        total.update(HASH_DOMAIN);
        for entry in &self.entries {
            let mut type_hasher = Hasher::new();
            (entry.encode)(world, &mut type_hasher);
            let hash = *type_hasher.finalize().as_bytes();
            total
                .update(&name_len(entry.name).to_le_bytes())
                .update(entry.name.as_bytes())
                .update(&hash);
            if let Some(per_type) = per_type.as_deref_mut() {
                per_type.push(TypeHash {
                    name: entry.name,
                    hash,
                });
            }
        }
        StateHash(*total.finalize().as_bytes())
    }
}

impl fmt::Display for StateHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl Default for StateRegistry {
    fn default() -> StateRegistry {
        StateRegistry::new()
    }
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
fn take<T: DeserializeOwned>(bytes: &[u8]) -> Result<Taken<'_, T>, SnapshotError> {
    let (value, rest) = postcard::take_from_bytes(bytes).map_err(SnapshotError::Malformed)?;
    Ok(Taken { value, rest })
}

fn encode_entities(world: &World, sink: &mut dyn Sink) {
    for (id, _) in world.resource::<EntityIndex>().iter() {
        write(sink, &id);
    }
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
    for (id, entity) in world.resource::<EntityIndex>().iter() {
        if let Some(component) = world.get::<C>(entity) {
            write(sink, &(id, component));
        }
    }
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
    write(sink, &world.get_resource::<R>());
}

fn decode_resource<R: SimResource>(world: &mut World, body: &[u8]) -> Result<(), SnapshotError> {
    let Taken { value, rest } = take::<Option<R>>(body)?;
    if !rest.is_empty() {
        return Err(SnapshotError::Trailing);
    }
    if let Some(value) = value {
        world.insert_resource(value);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
