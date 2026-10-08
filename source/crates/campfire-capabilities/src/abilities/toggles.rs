use bevy_ecs::query::Has;
use bevy_ecs::system::{Local, Query, Res};
use campfire_sim::{SimTick, TickRate};

use crate::abilities::Abilities;
use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::{ActionSlot, ActionSlots};
use crate::actions::rank_fields::TogglePer;
use crate::items::inventory::Inventory;
use crate::stats::pools::Pools;
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
        (tick, rate, book): (Res<'_, SimTick>, Res<'_, TickRate>, Res<'_, ActionBook>),
        mut units: Query<
            '_,
            '_,
            (
                &mut ActionSlots,
                Option<&mut Pools>,
                Option<&UnitTags>,
                Option<&Inventory>,
                Has<Dead>,
            ),
        >,
        mut on: Local<'_, Vec<(u8, ActionSlot)>>,
    ) {
        let now = tick.start();
        let second = Abilities::second(*rate);
        for (mut slots, mut pools, tags, inventory, dead) in &mut units {
            on.clear();
            on.extend(slots.indexed().filter(|(_, slot)| slot.toggle.is_some()));
            for &(at, slot) in &*on {
                let group = Inventory::group(inventory, slot.kind);
                if dead || UnitTags::properties_of(tags).blocks(group) {
                    slots.toggle_off(at);
                    continue;
                }
                let next = slot.toggle.expect("a toggle that is on");
                let toggle = slot
                    .values_in(&book)
                    .and_then(|values| values.toggle)
                    .expect("a toggle that is on has its rule");
                if toggle.per != TogglePer::Second || next > now {
                    continue;
                }
                match pools.as_deref_mut() {
                    Some(pools) if pools.affords(&toggle.cost) => {
                        pools.pay(&toggle.cost);
                        slots.toggle_on(at, next.after(second));
                    }
                    _ => slots.toggle_off(at),
                }
            }
        }
    }
}
