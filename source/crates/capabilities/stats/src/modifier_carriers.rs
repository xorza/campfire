use bevy_ecs::change_detection::{DetectChanges, Ref};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::Allow;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_declarations::Cache;
use campfire_sim::{Removals, Unpredicted};

use crate::lifetime::Hold;
use crate::modifier_book::ModifierBook;
use crate::modifiers::Modifiers;

/// The units that carry an instance of an aura, and those that carry an instance an aura or
/// another capability holds, `Hold::Held`, each by entity, in order, dead or not: the units the
/// held pass reads auras from, and those whose holds it may let go of. A run keeps both as the
/// units' modifiers change, so the pass visits no unit that carries neither and is held nothing.
#[derive(Resource, Debug, Default)]
pub(crate) struct ModifierCarriers {
    auras: Vec<Entity>,
    held: Vec<Entity>,
}

impl Cache for ModifierCarriers {
    fn installed(_: &World) -> Option<Self> {
        Some(Self::default())
    }

    fn clear(&mut self) {
        self.auras.clear();
        self.held.clear();
    }
}

impl ModifierCarriers {
    /// Keeps the carriers as the units' modifiers stand: each unit whose modifiers changed since
    /// the last run, or every unit when the book did, joins the auras' when it carries an
    /// instance of an aura with a radius, and the holds' when it carries a held instance, and
    /// leaves each when it carries none; a unit gone leaves both. A predicting client's units it
    /// holds as the server sent them are among them, as their auras reach the units it predicts.
    pub(crate) fn track(
        book: Res<'_, ModifierBook>,
        units: Query<'_, '_, (Entity, Ref<'_, Modifiers>), Allow<Unpredicted>>,
        mut gone: Removals<'_, '_, Modifiers>,
        mut carriers: ResMut<'_, Self>,
    ) {
        for entity in gone.read() {
            Self::keep(&mut carriers.auras, entity, false);
            Self::keep(&mut carriers.held, entity, false);
        }
        let every = book.is_changed();
        for (entity, modifiers) in &units {
            if !(every || modifiers.is_changed()) {
                continue;
            }
            let auras = modifiers.iter().any(|carried| {
                let instance = carried.instance;
                book.get(instance.id).spec.aura.is_some() && instance.aura_radius.is_some()
            });
            let held = modifiers
                .iter()
                .any(|carried| carried.instance.lifetime.held_by(Hold::Held));
            Self::keep(&mut carriers.auras, entity, auras);
            Self::keep(&mut carriers.held, entity, held);
        }
    }

    /// Adds `entity` to `set` when it `carries`, and takes it out when not.
    fn keep(set: &mut Vec<Entity>, entity: Entity, carries: bool) {
        match (set.binary_search(&entity), carries) {
            (Err(at), true) => set.insert(at, entity),
            (Ok(at), false) => {
                set.remove(at);
            }
            (Ok(_), true) | (Err(_), false) => {}
        }
    }

    /// The units that carry an aura.
    pub(crate) fn auras(&self) -> impl ExactSizeIterator<Item = Entity> + '_ {
        self.auras.iter().copied()
    }

    /// The units that carry a held instance.
    pub(crate) fn held(&self) -> impl ExactSizeIterator<Item = Entity> + '_ {
        self.held.iter().copied()
    }
}
