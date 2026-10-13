use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_common::Tick;
use campfire_math::Num;
use campfire_script::ScriptId;
use campfire_sim::{Capability, StableId};

use crate::actions::action::{Action, Aim};
use crate::actions::action_data::ActionData;
use crate::actions::action_parts::ActionParts;
use crate::actions::action_range::ActionRange;
use crate::actions::action_slots::ActionSlots;
use crate::actions::action_target::ActionTarget;
use crate::actions::actions_column::ActionsColumn;
use crate::actions::checked_action::CheckedAction;
use crate::actions::purse::Purse;
use crate::actions::slot_aim::SlotAim;
use crate::actions::slot_times::SlotTimes;
use crate::declare::book::Book;
use crate::scripts::frame::Frame;
use crate::scripts::script_book::ScriptBook;
use crate::scripts::script_role::ScriptRole;
use crate::units::action_id::ActionId;
use crate::units::living_unit::LivingUnit;
use crate::units::tag_set::TagSet;
use crate::units::team::Team;
use crate::units::view::View;
use crate::values::names::Names;
use crate::values::relation::Relation;

/// The actions a match loaded, times in ticks and scripts compiled. Package data, not state: a
/// restore loads it from the packages, as a new match does. A clone shares the actions, as the
/// script view reads them.
#[derive(Resource, Debug, Clone, Default)]
pub(crate) struct ActionBook {
    actions: Arc<Vec<Action>>,
    /// Each action's name in its package.
    names: Arc<Names<ActionId>>,
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
        let hooks = scripts.defines(script, ScriptRole::Action);
        let id = Arc::make_mut(&mut self.names).add((package, name));
        Arc::make_mut(&mut self.actions).push(Action {
            package,
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

    /// Each action's name in its package.
    pub(crate) const fn names(&self) -> &Arc<Names<ActionId>> {
        &self.names
    }

    pub(crate) fn get(&self, id: ActionId) -> Option<&Action> {
        self.actions.get(id.index())
    }

    /// The action `aim` names of a unit with `slots` and their `times`, when it may go on: its
    /// slot holds a learned action that is ready and, with charges, holds one, `purse` affords its cost in each pool
    /// and player resource, and its target is a living unit the action's filter selects, or the
    /// action takes none, which drops any target the order named. `relation` tells how the unit
    /// regards a team, and `living` finds a living unit.
    pub(crate) fn check(
        &self,
        now: Tick,
        (slots, times): (&ActionSlots, &SlotTimes),
        purse: Purse<'_>,
        aim: SlotAim,
        relation: impl Fn(Team) -> Relation,
        living: impl Fn(StableId) -> Option<LivingUnit>,
    ) -> Option<CheckedAction<'_>> {
        let (slot, time) = (slots.slot(aim.slot)?, times.slot(aim.slot)?);
        let (id, rank) = (slot.action?, slot.rank?);
        let action = self.get(id)?;
        let values = *action.values(rank);
        let affords = purse.affords(&values.cost, action.resource_cost(rank));
        let charged =
            values.charges.is_none() || time.charges.is_some_and(|charges| charges.count > 0);
        if now < time.ready_at || !affords || !charged {
            return None;
        }
        let target = match (action.aim, aim.target) {
            (Aim::None, _) => ActionTarget::None,
            (Aim::Point { .. } | Aim::Direction, ActionTarget::Point(at)) => {
                ActionTarget::Point(at)
            }
            (Aim::Unit(filter), ActionTarget::Unit(target))
                if living(target)
                    .is_some_and(|unit| filter.selects(relation(unit.team), unit.tags)) =>
            {
                ActionTarget::Unit(target)
            }
            _ => return None,
        };
        Some(CheckedAction {
            id,
            target,
            action,
            rank,
            values,
        })
    }

    /// The slot of the first learned weapon of `slots` whose filter selects a unit of `tags` its
    /// unit regards with `relation`, ready or not; or with `None`, the first learned weapon at all.
    pub(crate) fn weapon_for(
        &self,
        slots: &ActionSlots,
        target: Option<(Relation, TagSet)>,
    ) -> Option<u8> {
        let at = slots.iter().position(|slot| {
            slot.action.is_some_and(|action| {
                let action = self.get(action).expect("a slot's action is in the book");
                Action::arms(slot.rank, action.weapon_filter(), target)
            })
        })?;
        Some(u8::try_from(at).expect("a unit's slots fit u8"))
    }

    /// The range of the learned action in `slot` of `slots`, at its rank.
    pub(crate) fn range(&self, slots: &ActionSlots, slot: u8) -> ActionRange {
        let slot = slots.slot(slot).expect("a unit's slot");
        slot.values_in(self)
            .expect("a learned slot's action is in the book")
            .range
    }

    /// The range in meters of the action in `slot` of `slots`, one that reaches in meters at
    /// every rank, as a build's and a gather's do.
    pub(crate) fn meters(&self, slots: &ActionSlots, slot: u8) -> Num {
        let ActionRange::Meters(range) = self.range(slots, slot) else {
            panic!("a build's and a gather's ranks reach in meters, as their book checked");
        };
        range
    }
}

impl Book for ActionBook {
    const OWNER: Option<Capability> = None;
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, view: &View, _: Option<&mut Frame>) {
        ActionsColumn::share(view, self.clone());
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::sync::Arc;

    use crate::actions::action::Action;
    use crate::actions::action_book::ActionBook;
    use crate::units::action_id::ActionId;

    impl ActionBook {
        /// Adds `action`, named `name` in its package, or for its place after `name` when an
        /// action before it has that name, as a test adds several of one kind; its id.
        pub(crate) fn push(&mut self, name: &str, action: Action) -> ActionId {
            let names = Arc::make_mut(&mut self.names);
            let id = names.try_add((action.package, name)).unwrap_or_else(|_| {
                let placed = format!("{name} {}", names.len());
                names.add((action.package, placed.as_str()))
            });
            Arc::make_mut(&mut self.actions).push(action);
            id
        }
    }
}
