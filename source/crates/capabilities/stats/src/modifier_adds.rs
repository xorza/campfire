use std::fmt;

use bevy_ecs::query::{Allow, Without};
use bevy_ecs::system::{Query, Res, SystemParam, SystemState};
use bevy_ecs::world::World;
use campfire_common::Ticks;
use campfire_sim::{EntityIndex, StableId, TickClock, Unpredicted};
use campfire_units::Dead;
use campfire_values::ModifierId;

use crate::application::{Adding, ApplyAt};
use crate::applier::Applier;
use crate::carried_mut::CarriedMut;
use crate::modifier_book::ModifierBook;
use crate::modifier_params::ModifierParams;
use crate::modifiers::Modifiers;
use crate::modifiers::modifier_clocks::ModifierClocks;

/// A modifier an effect adds: `id` on `target`, from `applier`, for `duration` when a call names
/// one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ModifierAdd {
    pub(crate) target: StableId,
    pub(crate) id: ModifierId,
    pub(crate) applier: Applier,
    pub(crate) duration: Option<Ticks>,
}

/// What an added modifier reads of the match and writes: the book, the params and the units
/// they read from, the tick and its rate, and the living units' modifiers, those a predicting
/// client holds as the server sent them among them.
#[derive(SystemParam, Debug)]
pub(crate) struct ModifierAdds<'w, 's> {
    book: Res<'w, ModifierBook>,
    params: ModifierParams<'w, 's>,
    clock: TickClock<'w>,
    index: Res<'w, EntityIndex>,
    carriers: Query<
        'w,
        's,
        (&'static mut Modifiers, &'static mut ModifierClocks),
        (Without<Dead>, Allow<Unpredicted>),
    >,
}

/// `ModifierAdds` for the effects of a call, which hold the world itself: its queries made as
/// the first add needs them and kept between adds, so an add writes its instance straight into
/// its carrier's buffers.
#[derive(Default)]
pub(crate) struct HeldModifierAdds(Option<SystemState<ModifierAdds<'static, 'static>>>);

impl HeldModifierAdds {
    /// Adds `add`: its numbers resolve now, its params read from the param book, of its source
    /// as it is now; nothing is added to a dead or gone unit, or one that carries no modifiers.
    pub(crate) fn add(&mut self, world: &mut World, add: ModifierAdd) {
        let state = self.0.get_or_insert_with(|| SystemState::new(world));
        let ModifierAdds {
            book,
            params,
            clock,
            index,
            mut carriers,
        } = state
            .get_mut(world)
            .expect("the adds of modifiers are always valid");
        let Some(Ok((modifiers, clocks))) =
            index.get(add.target).map(|entity| carriers.get_mut(entity))
        else {
            return;
        };
        let at = ApplyAt {
            now: clock.start(),
            rate: clock.rate(),
            params: params.book(),
        };
        let source = add.applier.source.and_then(|source| params.source(source));
        let adding = Adding {
            id: add.id,
            applier: add.applier,
            source: source.as_ref(),
            duration: add.duration,
        };
        CarriedMut::new(modifiers, clocks).add(&book, at, adding);
    }
}

impl fmt::Debug for HeldModifierAdds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("HeldModifierAdds").finish_non_exhaustive()
    }
}
