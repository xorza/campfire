use bevy_ecs::change_detection::{DetectChangesMut, Mut};
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Num, Tick};
use campfire_sim::StableId;

use crate::scripts::state_value::StateValue;
use crate::stats::application::Application;
use crate::stats::lifetime::Hold;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::modifiers::{Modifiers, Touched};
use crate::units::modifier_id::ModifierId;

/// A unit's modifiers and their clocks, to change together: each component counts as changed
/// only when an operation changed it, so a change of a clock alone derives no stats again.
#[derive(Debug)]
pub(crate) struct CarriedMut<'w> {
    modifiers: Mut<'w, Modifiers>,
    clocks: Mut<'w, ModifierClocks>,
}

impl<'w> CarriedMut<'w> {
    pub(crate) const fn new(
        modifiers: Mut<'w, Modifiers>,
        clocks: Mut<'w, ModifierClocks>,
    ) -> CarriedMut<'w> {
        CarriedMut { modifiers, clocks }
    }

    /// The modifiers and clocks of `entity` in `world`; none when it carries none.
    pub(crate) fn of(world: &'w mut World, entity: Entity) -> Option<CarriedMut<'w>> {
        let (modifiers, clocks) = world
            .get_entity_mut(entity)
            .ok()?
            .into_components_mut::<(&mut Modifiers, &mut ModifierClocks)>()
            .ok()?;
        Some(CarriedMut { modifiers, clocks })
    }

    pub(crate) fn modifiers(&self) -> &Modifiers {
        &self.modifiers
    }

    /// See `Modifiers::apply`.
    pub(crate) fn apply(&mut self, application: Application) {
        let Parts { modifiers, clocks } = self.parts();
        let touched = modifiers.apply(clocks, application);
        self.mark(touched);
    }

    /// See `Modifiers::hold`.
    pub(crate) fn hold(&mut self, id: ModifierId, source: Option<StableId>, hold: Hold) {
        self.modifiers.hold(id, source, hold);
    }

    /// See `Modifiers::release`.
    pub(crate) fn release(&mut self, id: ModifierId, source: Option<StableId>, hold: Hold) {
        let Parts { modifiers, clocks } = self.parts();
        modifiers.release(clocks, id, source, hold);
        self.mark(Touched {
            stats: true,
            clocks: true,
        });
    }

    /// See `Modifiers::remove`.
    pub(crate) fn remove(&mut self, id: ModifierId, source: Option<StableId>) {
        let Parts { modifiers, clocks } = self.parts();
        if modifiers.remove(clocks, id, source) {
            self.mark(Touched {
                stats: true,
                clocks: true,
            });
        }
    }

    /// See `Modifiers::expire`.
    pub(crate) fn expire(&mut self, now: Tick) {
        let Parts { modifiers, clocks } = self.parts();
        let touched = modifiers.expire(clocks, now);
        self.mark(touched);
    }

    /// See `Modifiers::absorb`: what is left of `amount`.
    pub(crate) fn absorb(&mut self, amount: Num, takes_effect: impl Fn(ModifierId) -> bool) -> Num {
        let Parts { modifiers, clocks } = self.parts();
        let absorbed = modifiers.absorb(clocks, amount, takes_effect);
        self.mark(absorbed.touched);
        absorbed.left
    }

    /// See `Modifiers::clear_on_death`.
    pub(crate) fn clear_on_death(&mut self) {
        let Parts { modifiers, clocks } = self.parts();
        modifiers.clear_on_death(clocks);
        self.mark(Touched {
            stats: true,
            clocks: true,
        });
    }

    /// See `Modifiers::release_held`.
    pub(crate) fn release_held(&mut self, holds: impl FnMut(ModifierId, Option<StableId>) -> bool) {
        let Parts { modifiers, clocks } = self.parts();
        if modifiers.release_held(clocks, holds) {
            self.mark(Touched {
                stats: true,
                clocks: true,
            });
        }
    }

    /// See `ModifierClocks::advance_intervals`.
    pub(crate) fn advance_intervals(
        &mut self,
        now: Tick,
        takes_effect: impl Fn(ModifierId) -> bool,
        due: impl FnMut(ModifierId, Option<StableId>),
    ) {
        let modifiers = &*self.modifiers;
        let clocks = self.clocks.bypass_change_detection();
        if clocks.advance_intervals(modifiers, now, takes_effect, due) {
            self.clocks.set_changed();
        }
    }

    /// Writes the stacks and the state a call wrote to the instance of `id` from `source`, in
    /// tick `now`, if it still carries it.
    pub(crate) fn write(
        &mut self,
        id: ModifierId,
        source: Option<StableId>,
        stacks: u32,
        state: &[StateValue],
        now: Tick,
    ) {
        let Some(at) = self.modifiers.position(id, source) else {
            return;
        };
        if self
            .modifiers
            .get(id, source)
            .is_some_and(|held| held.stacks != stacks)
        {
            self.modifiers.set_stacks(id, source, stacks, now);
        }
        if self.clocks.state(at) != state {
            self.clocks.set_state(at, state);
        }
    }

    fn parts(&mut self) -> Parts<'_> {
        Parts {
            modifiers: self.modifiers.bypass_change_detection(),
            clocks: self.clocks.bypass_change_detection(),
        }
    }

    fn mark(&mut self, touched: Touched) {
        if touched.stats {
            self.modifiers.set_changed();
        }
        if touched.clocks {
            self.clocks.set_changed();
        }
    }
}

/// Both components, past change detection.
#[derive(Debug)]
struct Parts<'a> {
    modifiers: &'a mut Modifiers,
    clocks: &'a mut ModifierClocks,
}

#[cfg(test)]
mod tests {
    use bevy_ecs::change_detection::DetectChanges;
    use campfire_math::Ticks;

    use super::*;
    use crate::stats::application::NewInstance;
    use crate::stats::lifetime::{Ends, Lifetime};
    use crate::stats::modifier_clocks::Interval;
    use crate::stats::modifier_data::Reapply;

    #[test]
    fn a_change_of_clocks_alone_leaves_the_modifiers_unchanged() {
        // A modifier of an interval of 2 ticks, next in tick 2, and a shield of 10.
        let application = Application {
            instance: NewInstance {
                id: ModifierId::new(0),
                source: None,
                ability: None,
                rank: 1,
                lifetime: Lifetime::new(None, Ends::Never),
                aura_radius: None,
                stacks: 1,
                stack_life: None,
                stack_ends: Vec::new(),
                interval: Some(Interval {
                    every: Ticks::new(2),
                    next: Tick::new(2),
                }),
                shield: Some(Num::from_int(10).unwrap()),
                stats: Vec::new(),
                state: Vec::new(),
            },
            reapply: Reapply::Refresh,
            max_stacks: None,
        };
        let mut world = World::new();
        let unit = world.spawn(Modifiers::bundle([application])).id();
        let changed = |world: &World| {
            let unit = world.entity(unit);
            let modifiers = unit.get_ref::<Modifiers>().unwrap().is_changed();
            let clocks = unit.get_ref::<ModifierClocks>().unwrap().is_changed();
            Touched {
                stats: modifiers,
                clocks,
            }
        };
        // The interval comes in tick 2, and a shield absorbs 4 of 10: clocks alone change. An
        // interval not yet due changes nothing. A shield spent to 0 ends its instance: both.
        let cases: [(&dyn Fn(&mut CarriedMut<'_>), Touched); 4] = [
            (
                &|carried| carried.advance_intervals(Tick::new(1), |_| true, |_, _| {}),
                Touched::default(),
            ),
            (
                &|carried| carried.advance_intervals(Tick::new(2), |_| true, |_, _| {}),
                Touched {
                    stats: false,
                    clocks: true,
                },
            ),
            (
                &|carried| {
                    let left = carried.absorb(Num::from_int(4).unwrap(), |_| true);
                    assert_eq!(left, Num::ZERO);
                },
                Touched {
                    stats: false,
                    clocks: true,
                },
            ),
            (
                &|carried| {
                    // 6 of the shield's 10 are left: 9 spends them, and 3 pass.
                    let left = carried.absorb(Num::from_int(9).unwrap(), |_| true);
                    assert_eq!(left, Num::from_int(3).unwrap());
                },
                Touched {
                    stats: true,
                    clocks: true,
                },
            ),
        ];
        for (operation, expected) in cases {
            world.clear_trackers();
            operation(&mut CarriedMut::of(&mut world, unit).unwrap());
            assert_eq!(changed(&world), expected);
        }
        assert_eq!(world.get::<Modifiers>(unit).unwrap().len(), 0);
    }
}
