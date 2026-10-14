use bevy_ecs::query::Has;
use bevy_ecs::system::{Query, Res};
use campfire_actions::{ActionBook, ActionSlots, SlotTimes, TogglePer, UseGroup};
use campfire_sim::TickClock;
use campfire_stats::{PoolBook, UnitPools};
use campfire_units::Dead;
use campfire_values::UnitTags;

use crate::Abilities;

/// The toggles that are on, which pay their seconds or turn off as each tick starts.
#[derive(Debug)]
pub struct Toggles;

impl Toggles {
    /// Keeps each toggle that is on, as each tick starts, before the holds follow: death or a tag
    /// that blocks casting turns it off; one that costs a second pays at each whole second from
    /// when it turned on, and one its pools cannot pay turns off. The server alone runs it, as a
    /// client's pools come from the server.
    pub(super) fn run_toggles(
        (clock, book, life): (TickClock<'_>, Res<'_, ActionBook>, Res<'_, PoolBook>),
        mut units: Query<
            '_,
            '_,
            (
                &ActionSlots,
                &mut SlotTimes,
                UnitPools,
                Option<&UnitTags>,
                Option<&UseGroup>,
                Has<Dead>,
            ),
        >,
    ) {
        let now = clock.start();
        let second = Abilities::second(clock.rate());
        let life = life.life();
        for (slots, mut times, pools, tags, used, dead) in &mut units {
            let mut pools = pools.of(life);
            for (at, slot) in slots.indexed() {
                let Some(next) = times.slot(at).and_then(|time| time.toggle) else {
                    continue;
                };
                let group = UseGroup::group(used, slot, &book);
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
