use std::mem;

use bevy_ecs::resource::Resource;
use campfire_declarations::{Role, Scratch};

/// What a scripted pass runs this tick, which its plain system collects in the order the pass
/// runs it, so its exclusive part only runs the scripts: `T` names the pass by its own item. Not
/// state: the exclusive part empties it within the tick.
#[derive(Resource, Debug)]
pub struct Due<T>(Vec<T>);

impl<T> Default for Due<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T> Due<T> {
    /// Puts `items` in, in the order given, in place of what it held.
    pub fn refill(&mut self, items: impl IntoIterator<Item = T>) {
        self.0.clear();
        self.0.extend(items);
    }

    pub fn push(&mut self, item: T) {
        self.0.push(item);
    }

    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.0
    }

    /// Keeps only the items `keep` holds of.
    pub fn retain(&mut self, keep: impl FnMut(&T) -> bool) {
        self.0.retain(keep);
    }

    /// Takes the items out, for the exclusive part to run while it holds the world, which
    /// `give_back` returns the buffer to.
    pub fn take(&mut self) -> Vec<T> {
        mem::take(&mut self.0)
    }

    /// Takes back the buffer `take` gave, emptied, so a later tick fills it again.
    pub fn give_back(&mut self, mut items: Vec<T>) {
        items.clear();
        self.0 = items;
    }

    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Only a match that runs scripts runs a scripted pass.
impl<T: Send + Sync + 'static> Scratch for Due<T> {
    fn installed(role: &Role) -> Option<Self> {
        role.scripts().then(Self::default)
    }

    fn is_clear(&self) -> bool {
        self.0.is_empty()
    }
}
