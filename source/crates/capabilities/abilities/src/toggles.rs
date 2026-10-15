use bevy_ecs::change_detection::{DetectChanges, Ref};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::RemovedComponents;
use bevy_ecs::query::Has;
use bevy_ecs::system::{Local, ParamSet, Query, Res};
use campfire_actions::{ActionBook, ActionSlots, SlotTimes, TogglePer, UseGroup};
use campfire_sim::{DueRun, TickClock};
use campfire_stats::{PoolBook, UnitPools};
use campfire_units::Dead;
use campfire_values::UnitTags;

use crate::Abilities;

/// The toggles that are on, which pay their seconds or turn off as each tick starts.
#[derive(Debug)]
pub struct Toggles;

/// What decides whether a unit's toggles stay on, read for its change ticks.
type ToggleInputs<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Ref<'static, ActionSlots>,
        Ref<'static, SlotTimes>,
        Option<Ref<'static, UnitTags>>,
        Option<Ref<'static, UseGroup>>,
        Option<Ref<'static, Dead>>,
    ),
>;

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
    /// client's pools come from the server. Whether a toggle stays on follows from the unit's
    /// slots, times, tags, use group and death and from the book, and from the tick only as a
    /// second comes to pay: so a run visits the units of which one of these changed, or that
    /// lost their tags or use group, since its last, and those whose next second `due` holds,
    /// unless the book changed.
    pub(super) fn run_toggles(
        (clock, book, life): (TickClock<'_>, Res<'_, ActionBook>, Res<'_, PoolBook>),
        mut units: ParamSet<'_, '_, (ToggleInputs<'_, '_>, ToggleUnits<'_, '_>)>,
        (mut lost_tags, mut lost_groups): (
            RemovedComponents<'_, '_, UnitTags>,
            RemovedComponents<'_, '_, UseGroup>,
        ),
        (mut due, mut visits): (Local<'_, DueRun>, Local<'_, Vec<Entity>>),
    ) {
        let now = clock.start();
        let every = book.is_changed();
        if every {
            due.clear();
        }
        visits.clear();
        due.take_due(now, &mut visits);
        visits.extend(lost_tags.read().chain(lost_groups.read()));
        for (entity, slots, times, tags, used, dead) in &units.p0() {
            let changed = slots.is_changed()
                || times.is_changed()
                || tags.is_some_and(|tags| tags.is_changed())
                || used.is_some_and(|used| used.is_changed())
                || dead.is_some_and(|dead| dead.is_added());
            if every || changed {
                visits.push(entity);
            }
        }
        visits.sort_unstable();
        visits.dedup();
        let second = Abilities::second(clock.rate());
        let life = life.life();
        let mut units = units.p1();
        for &entity in &*visits {
            let Ok((slots, mut times, pools, tags, used, dead)) = units.get_mut(entity) else {
                continue;
            };
            let mut pools = pools.of(life);
            let mut next_second = None;
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
                if toggle.per != TogglePer::Second {
                    continue;
                }
                let next = if next > now {
                    next
                } else if pools.read().affords(&toggle.cost) {
                    pools.pay(&toggle.cost);
                    times.toggle_on(at, next.after(second));
                    next.after(second)
                } else {
                    times.toggle_off(at);
                    continue;
                };
                next_second = Some(next_second.map_or(next, |earlier| next.min(earlier)));
            }
            if let Some(next) = next_second {
                due.push(next, entity);
            }
        }
    }
}
