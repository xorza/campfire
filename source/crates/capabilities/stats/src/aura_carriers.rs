use bevy_ecs::change_detection::{DetectChanges, Ref};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::Allow;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_declarations::Cache;
use campfire_sim::{Removals, Unpredicted};

use crate::modifier_book::ModifierBook;
use crate::modifiers::Modifiers;

/// The units that carry an instance of an aura, by entity, in order, dead or not: the units the
/// held pass reads auras from. A run keeps it as the units' modifiers change, so the pass visits
/// no unit that carries none.
#[derive(Resource, Debug, Default)]
pub(crate) struct AuraCarriers(Vec<Entity>);

impl Cache for AuraCarriers {
    fn installed(_: &World) -> Option<Self> {
        Some(Self::default())
    }
}

impl AuraCarriers {
    /// Keeps the carriers as the units' modifiers stand: each unit whose modifiers changed since
    /// the last run, or every unit when the book did, joins when it carries an instance of an
    /// aura with a radius and leaves when it carries none; a unit gone leaves. A predicting
    /// client's units it holds as the server sent them are among them, as their auras reach the
    /// units it predicts.
    pub(crate) fn track(
        book: Res<'_, ModifierBook>,
        units: Query<'_, '_, (Entity, Ref<'_, Modifiers>), Allow<Unpredicted>>,
        mut gone: Removals<'_, '_, Modifiers>,
        mut carriers: ResMut<'_, Self>,
    ) {
        for entity in gone.read() {
            if let Ok(at) = carriers.0.binary_search(&entity) {
                carriers.0.remove(at);
            }
        }
        let every = book.is_changed();
        for (entity, modifiers) in &units {
            if !(every || modifiers.is_changed()) {
                continue;
            }
            let carries = modifiers.iter().any(|carried| {
                let instance = carried.instance;
                book.get(instance.id).spec.aura.is_some() && instance.aura_radius.is_some()
            });
            match (carriers.0.binary_search(&entity), carries) {
                (Err(at), true) => carriers.0.insert(at, entity),
                (Ok(at), false) => {
                    carriers.0.remove(at);
                }
                (Ok(_), true) | (Err(_), false) => {}
            }
        }
    }

    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = Entity> + '_ {
        self.0.iter().copied()
    }
}
