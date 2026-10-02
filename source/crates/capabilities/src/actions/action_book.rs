use std::num::NonZeroU8;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_math::Num;
use campfire_script::ScriptId;
use campfire_sim::{Position, StableId, Tick, TickRate, Ticks};
use serde::{Deserialize, Serialize};

use crate::actions::action_data::{ActionData, CostTarget, Range, Targeting};
use crate::actions::action_kind::ActionKind;
use crate::actions::action_names::ActionNames;
use crate::actions::action_slots::{ActionSlots, ActionTarget, InProgress};
use crate::actions::delivery_data::DeliveryData;
use crate::actions::error::ActionError;
use crate::actions::kind_spec::KindSpec;
use crate::actions::purse::Purse;
use crate::actions::weapon::Weapon;
use crate::combat::targets::Targets;
use crate::mode::resource_id::ResourceAmount;
use crate::scripts::hook::Hook;
use crate::scripts::hook_set::HookSet;
use crate::scripts::script_book::ScriptBook;
use crate::stats::modifier_book::ModifierId;
use crate::stats::pool_cost::PoolCost;
use crate::units::filter::Filter;
use crate::units::living_unit::LivingUnit;
use crate::units::tag_set::TagSet;
use crate::units::team::Team;
use crate::units::unit_type::UnitType;
use crate::values::attitude::Attitude;
use crate::values::declared_name::DeclaredName;

/// The actions a match loaded, times in ticks and scripts compiled. Package data, not state: a
/// restore loads it from the packages, as a new match does. A clone shares the actions, as the
/// script view reads them.
#[derive(Resource, Debug, Clone, Default)]
pub(crate) struct ActionBook {
    actions: Arc<Vec<Action>>,
}

/// An action, by its place in the book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActionId(u32);

/// An action as a match runs it.
#[derive(Debug, Clone)]
pub(crate) struct Action {
    /// Its package: 0 the mode, then each package the mode depends on, and its name there.
    pub(crate) package: u16,
    pub(crate) name: Box<str>,
    pub(crate) kind: KindSpec,
    /// The modifier its unit holds while it has a rank, and whether only while it is ready.
    pub(crate) passive: Option<Passive>,
    pub(crate) aim: Aim,
    /// Its capability fields at each rank, from rank 1.
    pub(crate) ranks: Vec<RankValues>,
    /// Its cost in its caster's player's resources at each rank, one run of the same resources
    /// each, rank after rank.
    resource_costs: Box<[ResourceAmount]>,
    /// Its script, if it has one, and the action hooks the script defines: a script may serve
    /// only the action's modifiers.
    script: Option<ScriptId>,
    hooks: HookSet,
    /// How it delivers, other than at once.
    pub(crate) delivery: Option<Delivery>,
}

/// How an action delivers, as a match runs it: the unit type it delivers, and its shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Delivery {
    pub(crate) unit_type: UnitType,
    pub(crate) shape: DeliveryShape,
}

/// The shape of a delivery: a fan of projectiles, which home on a unit when their type does, or
/// an area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeliveryShape {
    Projectile { fan: Fan, homes: bool },
    Area,
}

/// How a delivery launches its projectiles: `count` of them, spread evenly over `spread_deg`
/// degrees around the aim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fan {
    pub(crate) count: NonZeroU8,
    pub(crate) spread_deg: Num,
}

/// An action's passive modifier, and whether its unit holds it only while the action is off
/// cooldown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Passive {
    pub(crate) modifier: ModifierId,
    pub(crate) while_ready: bool,
}

/// What an action aims at, as a match runs it: a unit target's filter resolved against the
/// match's unit types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Aim {
    None,
    Point,
    Direction,
    Unit(Filter),
}

/// What an action's data gives, resolved against the match: its kind with what the kind needs,
/// its passive, its aim, its fields at each rank, and how it delivers.
#[derive(Debug)]
pub(crate) struct ActionParts {
    pub(crate) kind: KindSpec,
    pub(crate) passive: Option<Passive>,
    pub(crate) aim: Aim,
    pub(crate) ranks: LoadedRanks,
    pub(crate) delivery: Option<Delivery>,
}

impl ActionParts {
    /// The parts of `data`, of `package`, at each of its `ranks` ranks, times in ticks at `rate`,
    /// its names resolved by `names`; an error when a time does not count in ticks.
    pub(crate) fn of(
        data: &ActionData,
        package: u16,
        ranks: u8,
        rate: TickRate,
        names: &impl ActionNames,
    ) -> Result<ActionParts, ActionError> {
        let passive = data.passive_modifier.as_ref().map(|name| Passive {
            modifier: names.modifier(package, name),
            while_ready: data.passive_while_ready,
        });
        let aim = match &data.targeting {
            Targeting::None => Aim::None,
            Targeting::Point => Aim::Point,
            Targeting::Direction => Aim::Direction,
            Targeting::Unit(filter) => Aim::Unit(names.filter(filter)),
        };
        let ranks = RankValues::all(data, ranks, rate, |name| names.cost_target(name))?;
        let kind = match data.kind {
            ActionKind::Cast => KindSpec::Cast,
            ActionKind::Attack => {
                let checked = "the load checked an attack's weapon fields";
                KindSpec::Attack(Weapon {
                    rate: names.stat(data.rate.as_ref().expect(checked)),
                    damage: names.stat(data.damage.as_ref().expect(checked)),
                    kind: names.damage_kind(data.damage_kind.as_ref().expect(checked)),
                })
            }
            ActionKind::Train => {
                let unit = data
                    .unit_type
                    .as_ref()
                    .expect("the load checked a train's unit");
                KindSpec::Train(names.unit_type(package, unit))
            }
            kind => panic!("the load runs no {kind:?}"),
        };
        let delivery = data.delivery.as_ref().map(|delivery| {
            let unit_type = names.unit_type(package, delivery.unit_type());
            let shape = match *delivery {
                DeliveryData::Projectile {
                    count, spread_deg, ..
                } => DeliveryShape::Projectile {
                    fan: Fan { count, spread_deg },
                    homes: names.homes(package, delivery.unit_type()),
                },
                DeliveryData::Area { .. } => DeliveryShape::Area,
            };
            Delivery { unit_type, shape }
        });
        Ok(ActionParts {
            kind,
            passive,
            aim,
            ranks,
            delivery,
        })
    }
}

/// An action's fields at each rank: its values, and its cost in player resources, one run of
/// the same resources a rank.
#[derive(Debug)]
pub(crate) struct LoadedRanks {
    pub(crate) values: Vec<RankValues>,
    pub(crate) resource_costs: Vec<ResourceAmount>,
}

/// An action's capability fields at one rank, times in ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RankValues {
    pub(crate) range: Range,
    pub(crate) cooldown: Ticks,
    pub(crate) cost: PoolCost,
    pub(crate) windup: Ticks,
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
        let id = ActionId(u32::try_from(self.actions.len()).expect("actions fit u32"));
        Arc::make_mut(&mut self.actions).push(Action {
            package,
            name: name.into(),
            kind,
            passive,
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

    /// The action `underway` of a unit with `slots`, when it may go on: its slot holds a learned
    /// action that is ready, `purse` affords its cost in each pool and player resource, and its
    /// target is a living unit the action's filter selects, or the action takes none, which drops
    /// any target the order named. `attitude` tells how the unit regards a team, and `living`
    /// finds a living unit.
    pub(crate) fn check(
        &self,
        now: Tick,
        slots: &ActionSlots,
        purse: Purse<'_>,
        underway: InProgress,
        attitude: impl Fn(Team) -> Attitude,
        living: impl Fn(StableId) -> Option<LivingUnit>,
    ) -> Option<Checked<'_>> {
        let slot = slots.slot(underway.slot).filter(|slot| slot.rank > 0)?;
        let action = self.get(slot.action)?;
        let values = action.values(slot.rank);
        let affords = purse.affords(&values.cost, action.resource_cost(slot.rank));
        if now < slot.ready_at || !affords {
            return None;
        }
        let target = match (action.aim, underway.target) {
            (Aim::None, _) => ActionTarget::None,
            (Aim::Point | Aim::Direction, ActionTarget::Point(at)) => ActionTarget::Point(at),
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
            (Aim::Point, ActionTarget::Point(at)) => {
                targets.reaches_point(position, radius, range, at)
            }
            _ => true,
        }
    }
}

impl Action {
    /// Its script, when it defines `hook`.
    pub(crate) fn hook(&self, hook: Hook) -> Option<ScriptId> {
        self.script.filter(|_| self.hooks.contains(hook))
    }

    /// Its capability fields at `rank`.
    pub(crate) fn values(&self, rank: u8) -> RankValues {
        self.ranks[usize::from(rank - 1)]
    }

    /// Its cost at `rank` in its caster's player's resources.
    pub(crate) fn resource_cost(&self, rank: u8) -> &[ResourceAmount] {
        let per_rank = self.resource_costs.len() / self.ranks.len().max(1);
        let start = usize::from(rank - 1) * per_rank;
        &self.resource_costs[start..start + per_rank]
    }

    /// The filter of the units it may attack, for a weapon, which the load lets aim only at a
    /// unit; `None` for another kind.
    pub(crate) const fn weapon_filter(&self) -> Option<Filter> {
        match (self.kind, self.aim) {
            (KindSpec::Attack(_), Aim::Unit(filter)) => Some(filter),
            _ => None,
        }
    }

    /// Whether a slot at `rank` whose action has the weapon filter `weapon` arms its unit
    /// against a unit of `tags` it regards with `attitude`, or with `None`, against any: the one
    /// rule of `ActionBook::weapon_for` and of the script view.
    pub(crate) fn arms(
        rank: u8,
        weapon: Option<Filter>,
        target: Option<(Attitude, TagSet)>,
    ) -> bool {
        rank > 0
            && weapon.is_some_and(|filter| {
                target.is_none_or(|(attitude, tags)| filter.selects(attitude, tags))
            })
    }
}

impl ActionId {
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

impl RankValues {
    /// The fields of `data`, which the package load checked, at each of its `ranks` ranks,
    /// times in ticks at `rate`, what its cost's names take from by `target`; an error when a
    /// time does not count in ticks.
    pub(crate) fn all(
        data: &ActionData,
        ranks: u8,
        rate: TickRate,
        target: impl Fn(&DeclaredName) -> Option<CostTarget>,
    ) -> Result<LoadedRanks, ActionError> {
        assert!(
            data.check_ranks(usize::from(ranks)),
            "the load checked the ranks"
        );
        let ticks = |ms: u64| rate.ticks(ms).ok_or(ActionError::TimeTooLarge);
        let mut loaded = LoadedRanks {
            values: Vec::with_capacity(usize::from(ranks)),
            resource_costs: Vec::new(),
        };
        for rank in 1..=ranks {
            let fields = data
                .fields_at(rank, &target)
                .expect("the load checked the fields");
            loaded.values.push(RankValues {
                range: fields.range,
                cooldown: ticks(fields.cooldown_ms)?,
                cost: fields.cost,
                windup: ticks(fields.windup_ms)?,
            });
            loaded.resource_costs.extend(fields.resource_cost);
        }
        Ok(loaded)
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use crate::stats::stat_id::StatId;
    use campfire_math::Num;
    use campfire_sim::Ticks;

    use std::num::NonZeroU8;
    use std::sync::Arc;

    use crate::actions::action_book::{
        Action, ActionBook, ActionId, Aim, Delivery, DeliveryShape, Fan, RankValues,
    };
    use crate::actions::action_data::Range;
    use crate::actions::kind_spec::KindSpec;
    use crate::actions::weapon::Weapon;
    use crate::combat::damage_kind::DamageKind;
    use crate::mode::resource_id::ResourceAmount;
    use crate::scripts::hook_set::HookSet;
    use crate::stats::pool_cost::PoolCost;
    use crate::units::filter::Filter;
    use crate::units::script_view::View;
    use crate::units::unit_type::UnitType;
    use bevy_ecs::world::World;

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

    /// Adds `weapon` to the action book of `world`, which deals damage of the first kind.
    pub(crate) fn weapon(world: &mut World, weapon: TestWeapon) -> ActionId {
        let mut book = world.resource_mut::<ActionBook>();
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
        let id = ActionId(u32::try_from(book.actions.len()).unwrap());
        Arc::make_mut(&mut book.actions).push(Action {
            package: 0,
            name: "weapon".into(),
            kind: KindSpec::Attack(Weapon {
                rate: weapon.rate,
                damage: weapon.damage,
                kind: DamageKind::new(0),
            }),
            passive: None,
            aim: Aim::Unit(weapon.aim),
            ranks: vec![RankValues {
                range: weapon.range,
                cooldown: Ticks::ZERO,
                cost: weapon.cost,
                windup: weapon.windup,
            }],
            resource_costs: weapon.resource_cost.into_iter().collect(),
            script: None,
            hooks: HookSet::default(),
            delivery,
        });
        let book = book.clone();
        world.non_send::<View>().set_actions(book);
        id
    }

    impl ActionBook {
        /// The action `name` of `package`.
        pub(crate) fn action_named(&self, package: u16, name: &str) -> Option<ActionId> {
            let at = self
                .actions
                .iter()
                .position(|action| action.package == package && &*action.name == name)?;
            Some(ActionId(u32::try_from(at).expect("actions fit u32")))
        }
    }

    impl ActionId {
        /// The action at `index` of a book.
        #[cfg(test)]
        pub(crate) const fn new(index: u32) -> ActionId {
            ActionId(index)
        }
    }

    /// Adds a train of `unit` of no cost and no time to the action book of `world`.
    #[cfg(test)]
    pub(crate) fn train(world: &mut World, unit: UnitType) -> ActionId {
        let mut book = world.resource_mut::<ActionBook>();
        let id = ActionId(u32::try_from(book.actions.len()).unwrap());
        Arc::make_mut(&mut book.actions).push(Action {
            package: 0,
            name: "train".into(),
            kind: KindSpec::Train(unit),
            passive: None,
            aim: Aim::None,
            ranks: vec![RankValues {
                range: Range::Global,
                cooldown: Ticks::ZERO,
                cost: PoolCost::default(),
                windup: Ticks::ZERO,
            }],
            resource_costs: Box::new([]),
            script: None,
            hooks: HookSet::default(),
            delivery: None,
        });
        let book = book.clone();
        world.non_send::<View>().set_actions(book);
        id
    }
}
