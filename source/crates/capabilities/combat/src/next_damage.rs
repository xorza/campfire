use std::mem;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_actions::ActionBook;
use campfire_script::View;
use campfire_sim::SimResource;
use serde::{Deserialize, Serialize};

use crate::pass_queue::PassEntry;

/// The damage and heals a tick queued after its pass, as a mode's hook deals them, which the next
/// tick's pass deals first, in the order queued. State, as a snapshot at the boundary between
/// the two ticks holds them.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NextDamage(Vec<PassEntry>);

impl NextDamage {
    pub(crate) fn push(&mut self, entry: PassEntry) {
        self.0.push(entry);
    }

    /// Its entries, in the order queued, which it holds no more.
    pub(crate) fn take(&mut self) -> Vec<PassEntry> {
        mem::take(&mut self.0)
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl SimResource for NextDamage {
    const NAME: &'static str = "combat.next_damage";

    // A damage of a kind the mode lacks has no name for `calc_damage` to read, and one of an
    // action the book lacks no list to run.
    fn check(&self, world: &World) -> bool {
        let book = world.resource::<ActionBook>();
        let names = world.get_non_send::<View>().map(View::names);
        self.0.iter().all(|entry| {
            let (action, kind) = match entry {
                PassEntry::Damage(damage) => (damage.action, Some(damage.kind)),
                PassEntry::Heal(heal) => (heal.action, None),
            };
            action.is_none_or(|action| book.get(action).is_some())
                && kind.is_none_or(|kind| {
                    names
                        .as_ref()
                        .is_some_and(|names| names.has_damage_kind(kind))
                })
        })
    }
}
