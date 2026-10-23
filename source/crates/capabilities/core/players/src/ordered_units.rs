use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryData, QueryFilter, ROQueryItem};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_sim::{EntityIndex, StableId};
use campfire_units::Owner;

use crate::tick_commands::TickCommand;

/// The units of an order that its player controls, which alone carry it out, with what a
/// command system reads of each: `D` of the units `F` admits. Every system that applies the
/// tick's orders reaches their units through it, so none forgets whose they are.
#[derive(SystemParam, Debug)]
pub struct OrderedUnits<'w, 's, D: QueryData + 'static, F: QueryFilter + 'static = ()> {
    index: Res<'w, EntityIndex>,
    units: Query<'w, 's, (&'static Owner, D), F>,
}

impl<D: QueryData, F: QueryFilter> OrderedUnits<'_, '_, D, F> {
    /// Each unit of `order` its player controls that the query admits, by stable id, with its
    /// entity and what it reads of it.
    pub fn controlled<'a, A>(
        &'a self,
        order: &TickCommand<'a, A>,
    ) -> impl Iterator<Item = (StableId, Entity, ROQueryItem<'a, 'a, D>)> {
        order.units.iter().filter_map(move |&unit| {
            let entity = self.index.get(unit)?;
            let (owner, item) = self.units.get(entity).ok()?;
            order.controls(*owner).then_some((unit, entity, item))
        })
    }

    /// Gives `apply` each unit of `order` its player controls that the query admits, by stable
    /// id, with its entity and what it reads and changes of it.
    pub fn for_each_mut<A>(
        &mut self,
        order: &TickCommand<'_, A>,
        mut apply: impl FnMut(StableId, Entity, D::Item<'_, '_>),
    ) {
        for &unit in order.units {
            let Some(entity) = self.index.get(unit) else {
                continue;
            };
            let Ok((owner, item)) = self.units.get_mut(entity) else {
                continue;
            };
            if order.controls(*owner) {
                apply(unit, entity, item);
            }
        }
    }
}
