use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use campfire_sim::{Capability, SimSet};

use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::ActionSlots;
use crate::actions::actions_api::ActionsApi;
use crate::actions::actions_column::{ActionsColumn, RowParts};
use crate::actions::effect_lists::EffectLists;
use crate::actions::effect_queues::EffectQueues;
use crate::actions::holds::Holds;
use crate::actions::slot_kinds::SlotKinds;
use crate::actions::slot_times::SlotTimes;
use crate::declare::declarations::Declarations;
use crate::declare::declares::Declares;
use crate::declare::install_cx::InstallCx;
use crate::stats::StatsSet;
use crate::stats::param_book::ParamBook;

pub(crate) mod action;
pub(crate) mod action_book;
pub(crate) mod action_call;
pub(crate) mod action_data;
pub(crate) mod action_data_field;
pub(crate) mod action_kind;
pub(crate) mod action_names;
pub(crate) mod action_parts;
pub(crate) mod action_range;
pub(crate) mod action_slots;
pub(crate) mod action_target;
pub(crate) mod actions_api;
pub(crate) mod actions_column;
pub(crate) mod actions_effect;
pub(crate) mod actions_world;
pub(crate) mod amount;
pub(crate) mod capability_does;
pub(crate) mod channel_call;
pub(crate) mod checked_action;
pub(crate) mod construct_data;
pub(crate) mod cost_target;
pub(crate) mod delivery;
pub(crate) mod delivery_data;
pub(crate) mod does;
pub(crate) mod effect_data;
pub(crate) mod effect_lists;
pub(crate) mod effect_names;
pub(crate) mod effect_queues;
pub(crate) mod error;
pub(crate) mod fan;
pub(crate) mod gather_spec;
pub(crate) mod holds;
pub(crate) mod in_progress;
pub(crate) mod kind_data;
pub(crate) mod kind_spec;
pub(crate) mod launch_id;
pub(crate) mod lists_of;
pub(crate) mod payer;
pub(crate) mod placement_data;
pub(crate) mod purse;
pub(crate) mod range_field;
pub(crate) mod rank_fields;
pub(crate) mod rank_values;
pub(crate) mod ready_waits;
pub(crate) mod requires_data;
pub(crate) mod slot_aim;
pub(crate) mod slot_kind;
pub(crate) mod slot_kind_data;
pub(crate) mod slot_kinds;
pub(crate) mod slot_times;
pub(crate) mod targeting;
pub(crate) mod targets;
pub(crate) mod weapon;

/// The core's actions: every action a match loads, and the slots units hold them in.
#[derive(Debug)]
pub struct Actions;

/// The systems of the action pipeline, for the capabilities built on it to order theirs against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ActionsSet {
    /// In `SimSet::Act`, after attacks start: what each unit was ordered starts, each kind's
    /// orders by the capability that runs the kind.
    Start,
    /// In `SimSet::Inputs`, once the tick's orders and expiries are in: passives hold as the
    /// slots stand.
    HoldAtInputs,
    /// In `SimSet::Resolve`, once the tick's damage is dealt and before units die: passives hold
    /// as the slots stand.
    HoldAtResolve,
    /// In `SimSet::Hit`, once the attacks going off paid their toggles and before their events:
    /// holds follow the toggles an unpaid cost turned off.
    HoldAtStrike,
}

impl Declares for Actions {
    const MODULE: Option<Capability> = None;

    fn declare<D: Declarations>(d: &mut D) {
        d.component::<ActionSlots>();
        d.component::<SlotTimes>();
        d.require::<ActionSlots, SlotTimes>();
        d.book::<ActionBook>();
        d.book::<ParamBook>();
        d.book::<EffectLists>();
        d.column::<ActionsColumn, RowParts>(ActionsColumn::fill_row);
        d.api(ActionsApi::register);
    }
}

impl Actions {
    /// Adds the actions to a match whose core is installed, with none loaded yet.
    pub(crate) fn install(cx: &mut InstallCx<'_>) {
        cx.world.insert_resource(SlotKinds::default());
        cx.world.insert_resource(EffectQueues::default());
    }

    /// Adds to `schedule` the holds of every action: its charges as each tick starts, and its
    /// passives then and as each stage that changes ranks or cooldowns ends. Combat calls it, as
    /// every action's unit target is one combat finds; the start of an action is the abilities'.
    pub(crate) fn schedule(schedule: &mut Schedule) {
        schedule.add_systems((
            Holds::hold_charges
                .in_set(SimSet::Inputs)
                .in_set(ActionsSet::HoldAtInputs),
            Holds::hold_passives
                .in_set(SimSet::Inputs)
                .in_set(ActionsSet::HoldAtInputs)
                .after(StatsSet::Expire)
                .after(Holds::hold_charges),
            Holds::hold_passives
                .in_set(SimSet::Hit)
                .in_set(ActionsSet::HoldAtStrike),
            Holds::hold_passives
                .in_set(SimSet::Resolve)
                .in_set(ActionsSet::HoldAtResolve),
            Holds::hold_passives.in_set(SimSet::Vision),
        ));
    }
}
