use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{With, Without};
use bevy_ecs::system::{Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_declarations::ById;
use campfire_sim::{SimComponent, StableId};
use campfire_states::{Replication, SentPredicted};
use campfire_stats::{Held, HeldModifiers};
use campfire_units::{Dead, UnitEntity};
use campfire_values::{Rank, UnitType};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::second_life::SecondLife;

/// The mark of a unit in its second life, whose type's modifier it holds, until it respawns
/// (defenses-and-airfields.md, A type may live twice).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InSecondLife;

impl InSecondLife {
    /// Holds the second life's modifier on each living unit in its second life this tick, from
    /// itself, before stats hold the tick's modifiers.
    pub(crate) fn hold(
        lives: Query<'_, '_, (&StableId, &UnitType), (With<Self>, Without<Dead>)>,
        book: Res<'_, ById<UnitType, SecondLife>>,
        mut held: ResMut<'_, HeldModifiers>,
    ) {
        for (&id, &unit_type) in &lives {
            let life = book
                .get(unit_type)
                .expect("a unit in its second life is of a type of one");
            held.hold(Held {
                target: id,
                modifier: life.modifier,
                source: Some(id),
                action: None,
                rank: Rank::FIRST,
            });
        }
    }
}

impl SimComponent for InSecondLife {
    const NAME: &'static str = "combat.in_second_life";
    type Host = UnitEntity;

    // Of a unit whose type lives twice.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let book = world.get_resource::<ById<UnitType, SecondLife>>();
        world
            .get::<UnitType>(entity)
            .is_some_and(|&unit_type| book.is_some_and(|book| book.get(unit_type).is_some()))
    }
}

impl Replication for InSecondLife {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
