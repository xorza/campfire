use bevy_ecs::change_detection::{DetectChangesMut, Mut};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{MatchRate, SimComponent};
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::progress::Progress;

/// The tick a unit last found no place to step aside to for a walker in its way, none for one
/// that walked since: it is firm while a walker that met it may still ask routes, so routes price
/// it as one no walker may move and the route a stuck walker asks walls it, and after that a
/// walker asks it aside again, as the crowd round it has moved on
/// (04-capabilities/navigation.md, A friend that cannot yield is firm, and a stuck walker's
/// route walls it). The search that finds no place runs in one tick, and the stuck walker asks
/// in a later one, so the mark is a part.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Hemmed(Option<Tick>);

impl Hemmed {
    /// Whether the unit is hemmed in at `now`, at `rate`: it found no place within the ticks a
    /// walker stuck by it asks every route it may in, `Progress::asks_within`.
    pub fn holds(self, now: Tick, rate: MatchRate) -> bool {
        self.0.is_some_and(|found| {
            now.since(found)
                .is_some_and(|ticks| ticks.get() < u64::from(Progress::asks_within(rate)))
        })
    }

    /// Marks the unit of `hemmed` as one that found no place at `now`.
    pub(crate) fn mark(hemmed: &mut Mut<'_, Self>, now: Tick) {
        hemmed.set_if_neq(Self(Some(now)));
    }

    /// Marks the unit of `hemmed` as one that walks, so it is hemmed in no more.
    pub(crate) fn free(hemmed: &mut Mut<'_, Self>) {
        hemmed.set_if_neq(Self(None));
    }
}

impl SimComponent for Hemmed {
    const NAME: &'static str = "navigation.hemmed";
    type Host = UnitEntity;

    // Any tick, or none, is a state a unit may be in: a mark of a tick past the match's holds
    // at no tick before it.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Hemmed {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;

    #[test]
    fn a_mark_holds_while_a_walker_stuck_by_it_asks_routes_and_never_before_it() {
        // At 30 ticks a second, 150 ms stuck and 5 asks of 1 s are 5,150 ms, 154.5 ticks, rounded
        // up to 155: a mark of tick 100 holds from tick 100 through 254, and not at 255, nor
        // before it was made, nor with none.
        let rate = MatchRate::new(NonZeroU32::new(30).unwrap());
        let marked = Hemmed(Some(Tick::new(100)));
        let holds = |tick| marked.holds(Tick::new(tick), rate);
        assert_eq!([99, 100, 254, 255].map(holds), [false, true, true, false]);
        assert!(!Hemmed::default().holds(Tick::new(100), rate));
    }
}
