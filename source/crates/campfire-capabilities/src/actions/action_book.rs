use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_common::Tick;
use campfire_math::Num;
use campfire_script::ScriptId;
use campfire_sim::{Position, StableId};

use crate::actions::action::{Action, Aim};
use crate::actions::action_data::ActionData;
use crate::actions::action_parts::ActionParts;
use crate::actions::action_slots::{ActionSlots, SlotAim};
use crate::actions::action_target::ActionTarget;
use crate::actions::purse::Purse;
use crate::actions::range::Range;
use crate::actions::rank_values::RankValues;
use crate::actions::targets::Targets;
use crate::scripts::hook::Hook;
use crate::scripts::script_book::ScriptBook;
use crate::units::action_id::ActionId;
use crate::units::living_unit::LivingUnit;
use crate::units::tag_set::TagSet;
use crate::units::team::Team;
use crate::values::attitude::Attitude;

/// The actions a match loaded, times in ticks and scripts compiled. Package data, not state: a
/// restore loads it from the packages, as a new match does. A clone shares the actions, as the
/// script view reads them.
#[derive(Resource, Debug, Clone, Default)]
pub(crate) struct ActionBook {
    actions: Arc<Vec<Action>>,
}

impl ActionBook {
    /// Loads `data` as the action `name` of `package`, with `parts` resolved against the match;
    /// see `Actions::load`.
    pub(crate) fn load(
        &mut self,
        scripts: &ScriptBook,
        package: u16,
        name: &str,
        data: &ActionData,
        script: Option<ScriptId>,
        parts: ActionParts,
    ) -> ActionId {
        let ActionParts {
            kind,
            passive,
            hold,
            aim,
            ranks,
            delivery,
        } = parts;
        assert_eq!(
            data.script.is_some(),
            script.is_some(),
            "an action has a script exactly when its data names one"
        );
        let hooks = scripts.defines(script, &[Hook::OnResolve, Hook::OnHit, Hook::OnEnd]);
        let id = ActionId::nth(u32::try_from(self.actions.len()).expect("actions fit u32"));
        Arc::make_mut(&mut self.actions).push(Action {
            package,
            name: name.into(),
            kind,
            passive,
            hold,
            aim,
            ranks: ranks.values,
            resource_costs: ranks.resource_costs.into_boxed_slice(),
            script,
            hooks,
            delivery,
        });
        id
    }

    /// Every action's name, by id.
    pub(crate) fn names(&self) -> impl Iterator<Item = &str> {
        self.actions.iter().map(|action| &*action.name)
    }

    pub(crate) fn get(&self, id: ActionId) -> Option<&Action> {
        self.actions.get(id.index())
    }

    /// The action `name` of `package`.
    pub(crate) fn named(&self, package: u16, name: &str) -> Option<ActionId> {
        let at = self
            .actions
            .iter()
            .position(|action| action.package == package && &*action.name == name)?;
        Some(ActionId::nth(u32::try_from(at).expect("actions fit u32")))
    }

    /// The action `aim` names of a unit with `slots`, when it may go on: its slot holds a learned
    /// action that is ready and, with charges, holds one, `purse` affords its cost in each pool
    /// and player resource, and its
    /// target is a living unit the action's filter selects, or the action takes none, which drops
    /// any target the order named. `attitude` tells how the unit regards a team, and `living`
    /// finds a living unit.
    pub(crate) fn check(
        &self,
        now: Tick,
        slots: &ActionSlots,
        purse: Purse<'_>,
        aim: SlotAim,
        attitude: impl Fn(Team) -> Attitude,
        living: impl Fn(StableId) -> Option<LivingUnit>,
    ) -> Option<Checked<'_>> {
        let slot = slots.slot(aim.slot).filter(|slot| slot.rank > 0)?;
        let action = self.get(slot.action)?;
        let values = action.values(slot.rank);
        let affords = purse.affords(&values.cost, action.resource_cost(slot.rank));
        let charged =
            values.charges.is_none() || slot.charges.is_some_and(|charges| charges.count > 0);
        if now < slot.ready_at || !affords || !charged {
            return None;
        }
        let target = match (action.aim, aim.target) {
            (Aim::None, _) => ActionTarget::None,
            (Aim::Point { .. } | Aim::Direction, ActionTarget::Point(at)) => {
                ActionTarget::Point(at)
            }
            (Aim::Unit(filter), ActionTarget::Unit(target))
                if living(target)
                    .is_some_and(|unit| filter.selects(attitude(unit.team), unit.tags)) =>
            {
                ActionTarget::Unit(target)
            }
            _ => return None,
        };
        Some(Checked {
            id: slot.action,
            target,
            action,
            rank: slot.rank,
            values,
        })
    }

    /// The slot of the first learned weapon of `slots` whose filter selects a unit of `tags` its
    /// unit regards with `attitude`, ready or not; or with `None`, the first learned weapon at all.
    pub(crate) fn weapon_for(
        &self,
        slots: &ActionSlots,
        target: Option<(Attitude, TagSet)>,
    ) -> Option<u8> {
        let at = slots.iter().position(|slot| {
            let action = self
                .get(slot.action)
                .expect("a slot's action is in the book");
            Action::arms(slot.rank, action.weapon_filter(), target)
        })?;
        Some(u8::try_from(at).expect("a unit's slots fit u8"))
    }

    /// The range of the learned action in `slot` of `slots`, at its rank.
    pub(crate) fn range(&self, slots: &ActionSlots, slot: u8) -> Range {
        let slot = slots.slot(slot).expect("a unit's slot");
        let action = self
            .get(slot.action)
            .expect("a slot's action is in the book");
        action.values(slot.rank).range
    }
}

/// An action that passes its checks: its id, the target it keeps, none for an action that takes
/// none whatever its order named, and the action and its values at the slot's rank.
#[derive(Debug)]
pub(crate) struct Checked<'a> {
    pub(crate) id: ActionId,
    pub(crate) target: ActionTarget,
    pub(crate) action: &'a Action,
    pub(crate) rank: u8,
    pub(crate) values: RankValues,
}

impl Checked<'_> {
    /// Moves a point it aims at beyond its range in to the range, along the line from the unit
    /// at `position` with a body of `radius`, when its aim clamps, as `targets` measure reach.
    pub(crate) fn clamp(&mut self, position: Position, radius: Num, targets: &Targets<'_, '_>) {
        let (Aim::Point { clamp: true }, Range::Meters(range), ActionTarget::Point(at)) =
            (self.action.aim, self.values.range, self.target)
        else {
            return;
        };
        if targets.reaches_point(position, radius, range, at) {
            return;
        }
        // The step rounds once in each coordinate, so it may end a last bit past the reach;
        // stepping a bit shorter each time ends within it after a few.
        let mut step = range
            .checked_add(radius)
            .expect("a reach past every number reaches every point");
        let mut clamped = targets.toward(position, at, step);
        while !targets.reaches_point(position, radius, range, clamped) {
            step -= Num::from_bits(1);
            clamped = targets.toward(position, at, step);
        }
        self.target = ActionTarget::Point(clamped);
    }

    /// Whether its target is within its range of a unit at `position` with a body of `radius`,
    /// as `targets` measure reach: a unit's body, or a point it aims at; an action of global
    /// reach, one that aims at a direction, or one with no target always is. The range counts
    /// only when an action starts.
    pub(crate) fn in_range(
        &self,
        position: Position,
        radius: Num,
        targets: &Targets<'_, '_>,
    ) -> bool {
        let Range::Meters(range) = self.values.range else {
            return true;
        };
        match (self.action.aim, self.target) {
            (Aim::Unit(_), ActionTarget::Unit(target)) => targets
                .living(target)
                .is_some_and(|unit| targets.reaches(position, radius, range, &unit)),
            (Aim::Point { .. }, ActionTarget::Point(at)) => {
                targets.reaches_point(position, radius, range, at)
            }
            _ => true,
        }
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use std::num::NonZeroU8;
    use std::sync::Arc;

    use bevy_ecs::world::World;
    use campfire_common::Ticks;
    use campfire_math::Num;

    use crate::actions::action::{Action, Aim};
    use crate::actions::action_book::ActionBook;
    use crate::actions::actions_column::ActionsColumn;
    use crate::actions::delivery::{Delivery, DeliveryShape};
    use crate::actions::fan::Fan;
    use crate::actions::kind_spec::KindSpec;
    use crate::actions::range::Range;
    use crate::actions::rank_values::RankValues;
    use crate::actions::weapon::Weapon;
    use crate::players::resource_amount::ResourceAmount;
    use crate::scripts::hook_set::HookSet;
    use crate::stats::pool_cost::PoolCost;
    use crate::stats::stat_id::StatId;
    use crate::units::action_id::ActionId;
    use crate::units::filter::Filter;
    use crate::units::script_view::View;
    use crate::units::unit_type::UnitType;
    use crate::values::damage_kind::DamageKind;

    /// A weapon tests arm units with: what it aims at, its range, its windup, the type of the
    /// homing projectile it fires, if it fires one, the places among its unit's stats of its
    /// rate and its damage, and its cost in pools and in a player resource.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct TestWeapon {
        pub(crate) aim: Filter,
        pub(crate) range: Range,
        pub(crate) windup: Ticks,
        pub(crate) projectile: Option<UnitType>,
        pub(crate) rate: StatId,
        pub(crate) damage: StatId,
        pub(crate) cost: PoolCost,
        pub(crate) resource_cost: Option<ResourceAmount>,
    }

    impl TestWeapon {
        /// A melee weapon at `aim` within `range` that winds up `windup`, of the first two stats'
        /// rate and damage, costing nothing.
        pub(crate) fn new(aim: Filter, range: Range, windup: Ticks) -> TestWeapon {
            TestWeapon {
                aim,
                range,
                windup,
                projectile: None,
                rate: StatId::new(0),
                damage: StatId::new(1),
                cost: PoolCost::default(),
                resource_cost: None,
            }
        }
    }

    /// Adds `weapon` to the action book of `world`, which deals damage of the first kind.
    pub(crate) fn weapon(world: &mut World, weapon: TestWeapon) -> ActionId {
        let delivery = weapon.projectile.map(|unit_type| Delivery {
            unit_type,
            shape: DeliveryShape::Projectile {
                fan: Fan {
                    count: NonZeroU8::MIN,
                    spread_deg: Num::ZERO,
                },
                homes: true,
            },
        });
        push(
            world,
            Action {
                package: 0,
                name: "weapon".into(),
                kind: KindSpec::Attack(Weapon {
                    rate: weapon.rate,
                    damage: weapon.damage,
                    kind: DamageKind::new(0),
                }),
                passive: None,
                hold: None,
                aim: Aim::Unit(weapon.aim),
                ranks: vec![RankValues {
                    range: weapon.range,
                    cooldown: Ticks::ZERO,
                    cost: weapon.cost,
                    windup: weapon.windup,
                    charges: None,
                    toggle: None,
                }],
                resource_costs: weapon.resource_cost.into_iter().collect(),
                script: None,
                hooks: HookSet::default(),
                delivery,
            },
        )
    }

    /// Adds a train of `unit` that takes `time` and costs `resource_cost`, no pool and no
    /// cooldown, to the action book of `world`.
    #[cfg(test)]
    pub(crate) fn train(
        world: &mut World,
        unit: UnitType,
        time: Ticks,
        resource_cost: Option<ResourceAmount>,
    ) -> ActionId {
        push(
            world,
            Action {
                package: 0,
                name: "train".into(),
                kind: KindSpec::Train(unit),
                passive: None,
                hold: None,
                aim: Aim::None,
                ranks: vec![RankValues {
                    range: Range::Global,
                    cooldown: Ticks::ZERO,
                    cost: PoolCost::default(),
                    windup: time,
                    charges: None,
                    toggle: None,
                }],
                resource_costs: resource_cost.into_iter().collect(),
                script: None,
                hooks: HookSet::default(),
                delivery: None,
            },
        )
    }

    /// Adds `action` to the action book of `world`, and shares the book with the script view.
    fn push(world: &mut World, action: Action) -> ActionId {
        let mut book = world.resource_mut::<ActionBook>();
        let id = ActionId::nth(u32::try_from(book.actions.len()).unwrap());
        Arc::make_mut(&mut book.actions).push(action);
        let book = book.clone();
        ActionsColumn::share(world.non_send::<View>(), book);
        id
    }
}
