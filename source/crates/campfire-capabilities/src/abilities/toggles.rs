use bevy_ecs::query::Has;
use bevy_ecs::system::{Query, Res};
use campfire_sim::{SimTick, TickRate};

use crate::abilities::Abilities;
use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::ActionSlots;
use crate::actions::rank_fields::TogglePer;
use crate::actions::slot_times::SlotTimes;
use crate::items::inventory::Inventory;
use crate::stats::life_pool::LifePool;
use crate::stats::unit_pools::UnitPools;
use crate::units::dead::Dead;
use crate::units::unit_tags::UnitTags;

/// The toggles that are on, which pay their seconds or turn off as each tick starts.
#[derive(Debug)]
pub(super) struct Toggles;

impl Toggles {
    /// Keeps each toggle that is on, as each tick starts, before the holds follow: death or a tag
    /// that blocks casting turns it off; one that costs a second pays at each whole second from
    /// when it turned on, and one its pools cannot pay turns off. The server alone runs it, as a
    /// client's pools come from the server.
    pub(super) fn run_toggles(
        (tick, rate, book, life): (
            Res<'_, SimTick>,
            Res<'_, TickRate>,
            Res<'_, ActionBook>,
            Option<Res<'_, LifePool>>,
        ),
        mut units: Query<
            '_,
            '_,
            (
                &ActionSlots,
                &mut SlotTimes,
                UnitPools,
                Option<&UnitTags>,
                Option<&Inventory>,
                Has<Dead>,
            ),
        >,
    ) {
        let now = tick.start();
        let second = Abilities::second(*rate);
        let life = life.map(|life| life.0);
        for (slots, mut times, pools, tags, inventory, dead) in &mut units {
            let mut pools = pools.of(life);
            for (at, slot) in slots.indexed() {
                let Some(next) = times.slot(at).and_then(|time| time.toggle) else {
                    continue;
                };
                let group = Inventory::group(inventory, slot.kind);
                if dead || UnitTags::properties_of(tags).blocks(group) {
                    times.toggle_off(at);
                    continue;
                }
                let toggle = slot
                    .values_in(&book)
                    .and_then(|values| values.toggle)
                    .expect("a toggle that is on has its rule");
                if toggle.per != TogglePer::Second || next > now {
                    continue;
                }
                if pools.read().affords(&toggle.cost) {
                    pools.pay(&toggle.cost);
                    times.toggle_on(at, next.after(second));
                } else {
                    times.toggle_off(at);
                }
            }
        }
    }
}
