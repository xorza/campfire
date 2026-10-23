use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryData, QueryFilter, ROQueryItem, With};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_sim::{EntityIndex, StableId};
use campfire_units::{Leader, LivingSpawns, Owner};

use crate::tick_commands::TickCommand;

/// The units of an order that its player controls, which alone carry it out, with what a
/// command system reads of each: `D` of the units `F` admits. A leader of spawns carries out no
/// order of its own: its living spawns carry it out in its place
/// (04-capabilities/control.md, An order to a leader is its spawns' order). Every system that
/// applies the tick's orders reaches their units through it, so none forgets whose they are.
#[derive(SystemParam, Debug)]
pub struct OrderedUnits<'w, 's, D: QueryData + 'static, F: QueryFilter + 'static = ()> {
    index: Res<'w, EntityIndex>,
    units: Query<'w, 's, (&'static Owner, D), F>,
    leaders: Query<'w, 's, &'static LivingSpawns, With<Leader>>,
}

impl<D: QueryData, F: QueryFilter> OrderedUnits<'_, '_, D, F> {
    /// Each unit of `order` its player controls that the query admits, a leader's living spawns
    /// in its place, by stable id, with its entity and what it reads of it.
    pub fn controlled<'a, A>(
        &'a self,
        order: &TickCommand<'a, A>,
    ) -> impl Iterator<Item = (StableId, Entity, ROQueryItem<'a, 'a, D>)> {
        let carriers = order
            .units
            .iter()
            .flat_map(move |&unit| carriers(&self.index, &self.leaders, unit, order.units));
        carriers.filter_map(move |unit| {
            let entity = self.index.get(unit)?;
            let (owner, item) = self.units.get(entity).ok()?;
            order.controls(*owner).then_some((unit, entity, item))
        })
    }

    /// Gives `apply` each unit of `order` its player controls that the query admits, a leader's
    /// living spawns in its place, by stable id, with its entity and what it reads and changes of
    /// it.
    pub fn for_each_mut<A>(
        &mut self,
        order: &TickCommand<'_, A>,
        mut apply: impl FnMut(StableId, Entity, D::Item<'_, '_>),
    ) {
        let Self {
            index,
            units,
            leaders,
        } = self;
        for &named in order.units {
            for unit in carriers(index, leaders, named, order.units) {
                let Some(entity) = index.get(unit) else {
                    continue;
                };
                let Ok((owner, item)) = units.get_mut(entity) else {
                    continue;
                };
                if order.controls(*owner) {
                    apply(unit, entity, item);
                }
            }
        }
    }
}

/// The units that carry out `unit`'s share of an order of `named` units: its living spawns, but
/// those the order names itself, for a leader, else the unit itself.
fn carriers<'a>(
    index: &'a EntityIndex,
    leaders: &'a Query<'_, '_, &'static LivingSpawns, With<Leader>>,
    unit: StableId,
    named: &'a [StableId],
) -> impl Iterator<Item = StableId> + 'a {
    let led = index.get(unit).and_then(|entity| leaders.get(entity).ok());
    let spawns = led.into_iter().flat_map(LivingSpawns::iter).copied();
    let spawns = spawns.filter(move |spawn| !named.contains(spawn));
    spawns.chain(led.is_none().then_some(unit))
}
