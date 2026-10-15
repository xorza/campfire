use bevy_ecs::change_detection::{DetectChanges, Ref};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::Has;
use bevy_ecs::system::{Local, ParamSet, Query, Res};
use campfire_actions::{ActionBook, ActionSlots, SlotTimes, TogglePer, UseGroup};
use campfire_sim::TickClock;
use campfire_stats::{PoolBook, UnitPools};
use campfire_units::Dead;
use campfire_values::UnitTags;

use crate::Abilities;

/// The toggles that are on, which pay their seconds or turn off as each tick starts.
#[derive(Debug)]
pub struct Toggles;

/// The units whose toggles a run keeps.
type ToggleUnits<'w, 's> = Query<
    'w,
    's,
    (
        &'static ActionSlots,
        &'static mut SlotTimes,
        UnitPools,
        Option<&'static UnitTags>,
        Option<&'static UseGroup>,
        Has<Dead>,
    ),
>;

impl Toggles {
    /// Keeps each toggle that is on, as each tick starts, before the holds follow: death or a tag
    /// that blocks casting turns it off; one that costs a second pays at each whole second from
    /// when it turned on, and one its pools cannot pay turns off. The server alone runs it, as a
    /// client's pools come from the server. Only a unit with a toggle on has one to keep, and a
    /// toggle turns on only as its unit's times change: so a run visits the units of `on`, which
    /// held a toggle on as the last run ended, and those whose times changed since.
    pub(super) fn run_toggles(
        (clock, book, life): (TickClock<'_>, Res<'_, ActionBook>, Res<'_, PoolBook>),
        mut units: ParamSet<
            '_,
            '_,
            (
                Query<'_, '_, (Entity, Ref<'_, SlotTimes>)>,
                ToggleUnits<'_, '_>,
            ),
        >,
        mut on: Local<'_, Vec<Entity>>,
    ) {
        for (entity, times) in &units.p0() {
            if times.is_changed() {
                on.push(entity);
            }
        }
        on.sort_unstable();
        on.dedup();
        let now = clock.start();
        let second = Abilities::second(clock.rate());
        let life = life.life();
        let mut units = units.p1();
        on.retain(|&entity| {
            let Ok((slots, mut times, pools, tags, used, dead)) = units.get_mut(entity) else {
                return false;
            };
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
            times.iter().any(|time| time.toggle.is_some())
        });
    }
}
