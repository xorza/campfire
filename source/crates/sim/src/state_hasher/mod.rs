use bevy_ecs::world::World;
use blake3::Hasher;
use postcard::ser_flavors::Flavor;
use serde::Serialize;

use crate::entity_index::EntityIndex;
use crate::id_allocator::IdAllocator;
use crate::sim_state::{SimComponent, SimResource};

#[cfg(feature = "bench")]
pub(crate) mod bench;

/// Starts the combined hash, so no other BLAKE3 use can produce the same state hash.
const DOMAIN: &[u8] = b"campfire/state/v1";
/// Postcard writes a byte at a time; batching them keeps BLAKE3 from paying per byte.
const WRITE_BUFFER: usize = 64;

/// Hashes the simulated state: one BLAKE3 hash per registered type, then one over those, so the
/// first divergence names its type. Each type walks entities in stable-id order.
#[derive(Debug)]
pub struct StateHasher {
    entries: Vec<Entry>,
}

#[derive(Debug)]
struct Entry {
    name: &'static str,
    hash: fn(&World, &mut Hasher),
}

/// The hash of one registered type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeHash {
    pub name: &'static str,
    pub hash: [u8; 32],
}

/// The hash of the whole simulated state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StateHash([u8; 32]);

impl StateHash {
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl StateHasher {
    /// A hasher with the sim's own state registered.
    pub fn new() -> StateHasher {
        let mut hasher = StateHasher {
            entries: Vec::new(),
        };
        hasher.register_resource::<IdAllocator>();
        hasher
    }

    pub fn register_component<C: SimComponent>(&mut self) {
        self.register(C::NAME, hash_component::<C>);
    }

    pub fn register_resource<R: SimResource>(&mut self) {
        self.register(R::NAME, hash_resource::<R>);
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

    fn register(&mut self, name: &'static str, hash: fn(&World, &mut Hasher)) {
        let Err(at) = self.entries.binary_search_by(|entry| entry.name.cmp(name)) else {
            panic!("state type {name:?} registered twice");
        };
        self.entries.insert(at, Entry { name, hash });
    }

    fn combine(&self, world: &World, mut per_type: Option<&mut Vec<TypeHash>>) -> StateHash {
        let mut total = Hasher::new();
        total.update(DOMAIN);
        for entry in &self.entries {
            let mut type_hasher = Hasher::new();
            (entry.hash)(world, &mut type_hasher);
            let hash = *type_hasher.finalize().as_bytes();
            let name_len = u32::try_from(entry.name.len()).expect("state type name above 4 GiB");
            total
                .update(&name_len.to_le_bytes())
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

impl Default for StateHasher {
    fn default() -> StateHasher {
        StateHasher::new()
    }
}

fn hash_component<C: SimComponent>(world: &World, hasher: &mut Hasher) {
    for (id, entity) in world.resource::<EntityIndex>().iter() {
        if let Some(component) = world.get::<C>(entity) {
            write(hasher, &(id, component));
        }
    }
}

/// A missing resource hashes differently from an empty one: the value goes in as an `Option`.
fn hash_resource<R: SimResource>(world: &World, hasher: &mut Hasher) {
    write(hasher, &world.get_resource::<R>());
}

fn write<T: Serialize>(hasher: &mut Hasher, value: &T) {
    postcard::serialize_with_flavor(
        value,
        HashWriter {
            hasher,
            buffer: [0; WRITE_BUFFER],
            len: 0,
        },
    )
    .expect("postcard into a hasher cannot fail");
}

/// A postcard output that feeds BLAKE3 in batches.
struct HashWriter<'a> {
    hasher: &'a mut Hasher,
    buffer: [u8; WRITE_BUFFER],
    len: usize,
}

impl Flavor for HashWriter<'_> {
    type Output = ();

    fn try_push(&mut self, byte: u8) -> postcard::Result<()> {
        if self.len == WRITE_BUFFER {
            self.hasher.update(&self.buffer);
            self.len = 0;
        }
        self.buffer[self.len] = byte;
        self.len += 1;
        Ok(())
    }

    fn try_extend(&mut self, bytes: &[u8]) -> postcard::Result<()> {
        if self.len + bytes.len() <= WRITE_BUFFER {
            self.buffer[self.len..self.len + bytes.len()].copy_from_slice(bytes);
            self.len += bytes.len();
        } else {
            self.hasher.update(&self.buffer[..self.len]);
            self.len = 0;
            self.hasher.update(bytes);
        }
        Ok(())
    }

    fn finalize(self) -> postcard::Result<()> {
        self.hasher.update(&self.buffer[..self.len]);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
