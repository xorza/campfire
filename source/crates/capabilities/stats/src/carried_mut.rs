use bevy_ecs::change_detection::{DetectChangesMut, Mut};
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::Num;
use campfire_script::StateValue;
use campfire_sim::StableId;
use campfire_values::ModifierId;

use crate::application::{Adding, Application, ApplyAt};
use crate::instance::StatShare;
use crate::interval_revival::IntervalRevival;
use crate::lifetime::Hold;
use crate::modifier_book::ModifierBook;
use crate::modifier_dues::ModifierDues;
use crate::modifiers::modifier_clocks::ModifierClocks;
use crate::modifiers::{Modifiers, Touched};

/// A unit's modifiers and their clocks, to change together: each component counts as changed
/// only when an operation changed it, so a change of a clock alone derives no stats again; each
/// change pushes the unit into the indexes of the ends and the intervals.
#[derive(Debug)]
pub struct CarriedMut<'w> {
    modifiers: Mut<'w, Modifiers>,
    clocks: Mut<'w, ModifierClocks>,
    dues: ModifierDues<'w>,
}

impl<'w> CarriedMut<'w> {
    pub(crate) const fn new(
        modifiers: Mut<'w, Modifiers>,
        clocks: Mut<'w, ModifierClocks>,
        dues: ModifierDues<'w>,
    ) -> Self {
        CarriedMut {
            modifiers,
            clocks,
            dues,
        }
    }

    pub(crate) fn modifiers(&self) -> &Modifiers {
        &self.modifiers
    }

    /// See `Modifiers::apply`.
    pub fn apply<S, T>(&mut self, application: Application<S, T>)
    where
        S: IntoIterator<Item = StatShare, IntoIter: ExactSizeIterator>,
        T: IntoIterator<Item = StateValue, IntoIter: ExactSizeIterator>,
    {
        let Parts { modifiers, clocks } = self.parts();
        let touched = modifiers.apply(clocks, application);
        self.mark(touched);
    }

    /// Applies `adding` as `book` makes it `at`, as `Modifiers::apply` does: a new instance, or
    /// one more application of the one its source holds, whose hold joins it.
    pub(crate) fn add(&mut self, book: &ModifierBook, at: ApplyAt<'_>, adding: Adding<'_>) {
        self.apply(book.application(at, adding));
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
    pub fn remove(&mut self, id: ModifierId, source: Option<StableId>) {
        let Parts { modifiers, clocks } = self.parts();
        if modifiers.remove(clocks, id, source) {
            self.mark(Touched {
                stats: true,
                clocks: true,
            });
        }
    }

    /// See `Modifiers::purge`.
    pub(crate) fn purge(&mut self, grants: impl Fn(ModifierId) -> bool) {
        let Parts { modifiers, clocks } = self.parts();
        if modifiers.purge(clocks, grants) {
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
    pub fn absorb(&mut self, amount: Num, takes_effect: impl Fn(ModifierId) -> bool) -> Num {
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

    /// See `ModifierClocks::advance_intervals`; the unit is pushed at its next interval, as the
    /// pass that visits it took it.
    pub fn advance_intervals(
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
        self.dues.push(&self.modifiers, &self.clocks);
    }

    /// See `ModifierClocks::skip_intervals`; the unit is pushed at its next interval.
    pub fn skip_intervals(&mut self, now: Tick) {
        if self.clocks.bypass_change_detection().skip_intervals(now) {
            self.clocks.set_changed();
        }
        self.dues.push(&self.modifiers, &self.clocks);
    }

    /// See `ModifierClocks::revive_intervals`.
    pub fn revive_intervals(&mut self, now: Tick, revival: impl Fn(ModifierId) -> IntervalRevival) {
        let modifiers = &*self.modifiers;
        let clocks = self.clocks.bypass_change_detection();
        if clocks.revive_intervals(modifiers, now, revival) {
            self.clocks.set_changed();
        }
        self.dues.push(&self.modifiers, &self.clocks);
    }

    /// Writes the stacks and the state a call wrote to the instance of `id` from `source`, in
    /// tick `now`, if it still carries it.
    pub fn write(
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
        self.dues.push(&self.modifiers, &self.clocks);
    }

    fn parts(&mut self) -> Parts<'_> {
        Parts {
            modifiers: self.modifiers.bypass_change_detection(),
            clocks: self.clocks.bypass_change_detection(),
        }
    }

    /// Marks what `touched` changed, and pushes the unit as its modifiers and clocks stand.
    fn mark(&mut self, touched: Touched) {
        if touched.stats {
            self.modifiers.set_changed();
        }
        if touched.clocks {
            self.clocks.set_changed();
        }
        self.dues.push(&self.modifiers, &self.clocks);
    }

    /// Runs `change` on the modifiers and clocks of `entity` in `world`, as an exclusive system
    /// or an effect changes them; none when it carries none.
    pub fn with<R>(
        world: &mut World,
        entity: Entity,
        change: impl FnOnce(&mut CarriedMut<'_>) -> R,
    ) -> Option<R> {
        let &id = world.get::<StableId>(entity)?;
        ModifierDues::with(world, id, entity, |world, dues| {
            let (modifiers, clocks) = world
                .get_entity_mut(entity)
                .ok()?
                .into_components_mut::<(&mut Modifiers, &mut ModifierClocks)>()
                .ok()?;
            Some(change(&mut CarriedMut::new(
                modifiers,
                clocks,
                dues.reborrow(),
            )))
        })
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
    use campfire_common::Ticks;
    use campfire_sim::{DueIndex, EntityIndex, IdAllocator};
    use campfire_values::Rank;

    use super::*;
    use crate::application::NewInstance;
    use crate::interval::Interval;
    use crate::lifetime::{Ends, Lifetime};
    use crate::modifier_data::Reapply;

    #[test]
    fn a_change_of_clocks_alone_leaves_the_modifiers_unchanged() {
        // A modifier of an interval of 2 ticks, next in tick 2, and a shield of 10.
        let application = Application {
            instance: NewInstance {
                id: ModifierId::new(0),
                source: None,
                action: None,
                rank: Rank::FIRST,
                lifetime: Lifetime::applied(Ends::Never),
                aura_radius: None,
                stacks: 1,
                stack_life: None,
                stack_end: None,
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
        world.init_resource::<EntityIndex>();
        world.init_resource::<DueIndex<Modifiers>>();
        world.init_resource::<DueIndex<ModifierClocks>>();
        let id = IdAllocator::default().allocate();
        let unit = world.spawn((id, Modifiers::bundle([application]))).id();
        // The insert pushes the unit at its interval, in tick 2, and at no end, as it never ends.
        let held = |world: &World| {
            let ends = world.resource::<DueIndex<Modifiers>>().due_at(id);
            let intervals = world.resource::<DueIndex<ModifierClocks>>().due_at(id);
            [ends, intervals]
        };
        assert_eq!(held(&world), [None, Some(Tick::new(2))]);
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
        // interval not yet due changes nothing. A shield spent to 0 ends its instance: both. Each
        // operation, taken out of the index as a pass takes it, pushes the unit again at its next
        // interval: 2 while not yet due, 4 once tick 2 came, and none once the instance ends.
        let cases: [(&dyn Fn(&mut CarriedMut<'_>), Touched, Option<u64>); 4] = [
            (
                &|carried| carried.advance_intervals(Tick::new(1), |_| true, |_, _| {}),
                Touched::default(),
                Some(2),
            ),
            (
                &|carried| carried.advance_intervals(Tick::new(2), |_| true, |_, _| {}),
                Touched {
                    stats: false,
                    clocks: true,
                },
                Some(4),
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
                Some(4),
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
                None,
            ),
        ];
        for (operation, expected, next) in cases {
            world.clear_trackers();
            let mut taken = Vec::new();
            let mut intervals = world.resource_mut::<DueIndex<ModifierClocks>>();
            intervals.take_due(Tick::LIMIT, &mut taken);
            assert_eq!(taken.len(), 1);
            CarriedMut::with(&mut world, unit, operation).unwrap();
            assert_eq!(changed(&world), expected);
            assert_eq!(held(&world), [None, next.map(Tick::new)]);
        }
        assert_eq!(world.get::<Modifiers>(unit).unwrap().len(), 0);
    }
}
