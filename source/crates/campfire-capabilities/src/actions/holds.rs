use bevy_ecs::change_detection::{DetectChanges, Ref};
use bevy_ecs::entity::Entity;
use bevy_ecs::system::{Local, Query, Res};
use campfire_sim::{SimTick, StableId, TickRate};

use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::{ActionSlots, SlotCharges};
use crate::actions::ready_waits::ReadyWaits;
use crate::stats::applier::Applier;
use crate::stats::carried_mut::CarriedMut;
use crate::stats::lifetime::Hold;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::modifier_spec::ParamPlace;
use crate::stats::modifiers::Modifiers;
use crate::stats::param_book::ParamBook;
use crate::stats::param_sources::ParamSources;
use crate::stats::stat_book::StatBook;
use crate::values::rank::Rank;

/// The holds of every action: its charges as each tick starts, and its passives then and as each
/// stage that changes ranks or cooldowns ends.
#[derive(Debug)]
pub(super) struct Holds;

impl Holds {
    /// Keeps each unit's charges as its slots stand, as each tick starts, after the orders that
    /// learn ranks: a slot that came to a rank of an action with charges fills, and each charge
    /// whose time came comes back. A slot changes only when its charges do, as the slots replicate.
    pub(super) fn hold_charges(
        actions: Res<'_, ActionBook>,
        tick: Res<'_, SimTick>,
        mut units: Query<'_, '_, &mut ActionSlots>,
        mut due: Local<'_, Vec<(u8, Option<SlotCharges>)>>,
    ) {
        let now = tick.start();
        for mut slots in &mut units {
            due.clear();
            due.extend(slots.charges_due(&actions, now));
            for &(slot, charges) in &*due {
                slots.set_charges(slot, charges);
            }
        }
    }

    /// Keeps each unit's passives and holds as its slots stand: the passive of each action with a
    /// rank, and with `passive_while_ready` off cooldown, and the `hold` of each action whose
    /// toggle is on or whose channel runs, each from the unit itself at the action's rank, applied
    /// again when the rank changes; and none other. It runs as each tick starts, after the casts
    /// resolve and the attacks strike, and after the mode's calls, which learn ranks. A passive's
    /// or a hold's params are the match's param book's. What a unit holds follows from its slots,
    /// its modifiers and the books, and from the tick only for a passive that waits for its
    /// action's cooldown: so a run visits only the units whose slots or modifiers changed since its
    /// last, and those that wait, unless a book changed.
    pub(super) fn hold_passives(
        actions: Res<'_, ActionBook>,
        book: Option<Res<'_, ModifierBook>>,
        stats: Option<Res<'_, StatBook>>,
        (tick, rate): (Res<'_, SimTick>, Res<'_, TickRate>),
        (params, sources): (Res<'_, ParamBook>, ParamSources<'_, '_>),
        mut units: Query<
            '_,
            '_,
            (
                Entity,
                &StableId,
                Ref<'_, ActionSlots>,
                &mut Modifiers,
                &mut ModifierClocks,
            ),
        >,
        mut waiting: Local<'_, ReadyWaits>,
    ) {
        let Some(stats) = stats else {
            return;
        };
        let Some(book) = book else {
            return;
        };
        let every =
            actions.is_changed() || book.is_changed() || stats.is_changed() || params.is_changed();
        let now = tick.start();
        for (entity, &id, slots, modifiers, clocks) in &mut units {
            if !(every || slots.is_changed() || modifiers.is_changed() || waiting.waits(entity)) {
                continue;
            }
            let mut waits = false;
            let mut carried = CarriedMut::new(modifiers, clocks);
            for (index, slot) in slots.indexed() {
                let Some(ability) = slot.action else {
                    continue;
                };
                let action = actions
                    .get(ability)
                    .expect("a slot's action is in the book");
                // The modifier is held at `holds`, its action's rank, or released with none.
                let mut keep = |modifier, hold, holds: Option<Rank>| {
                    let held = carried
                        .modifiers()
                        .get(modifier, Some(id))
                        .filter(|instance| instance.lifetime.held_by(hold))
                        .map(|instance| instance.rank);
                    let Some(rank) = holds else {
                        if held.is_some() {
                            carried.release(modifier, Some(id), hold);
                        }
                        return;
                    };
                    if held == Some(rank) {
                        return;
                    }
                    let applier = Applier {
                        source: Some(id),
                        ability: Some(ability),
                        rank,
                        hold: Some(hold),
                    };
                    let source = sources.get(id);
                    let param = |place: &ParamPlace| {
                        let ability = Some(ability);
                        params.modifier_param(modifier, ability, rank, place, source.as_ref())
                    };
                    carried.apply(book.application(modifier, applier, None, now, *rate, param));
                };
                if let Some(passive) = action.passive {
                    let cooling = passive.while_ready && slot.ready_at > now;
                    waits |= slot.rank.is_some() && cooling;
                    keep(
                        passive.modifier,
                        Hold::Passive,
                        slot.rank.filter(|_| !cooling),
                    );
                }
                if let Some(hold) = action.hold {
                    let runs = slot.toggle.is_some() || slots.channeling() == Some(index);
                    keep(hold, Hold::Running, slot.rank.filter(|_| runs));
                }
            }
            waiting.set(entity, waits);
        }
    }
}
