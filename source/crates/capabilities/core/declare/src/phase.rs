use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use campfire_sim::SimSet;

campfire_common::enum_all! {
    /// The steps of the tick's stages that the modules' sets run in: the one list of the tick's
    /// order. The phases of a stage run in the order listed; the sets of one phase, of any module,
    /// in the order the schedule picks, so two that conflict sit in two phases, as the ambiguity
    /// check holds. A set that needs a step between two others gets a phase of its own here.
    #[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Phase {
        /// The tick begins: modifiers that hold no longer end, pools regenerate, the statics
        /// moved are tracked and the dead due respawn.
        Begin,
        /// The mode reads its players' inputs.
        ModeInputs,
        /// Toggled abilities pay for the tick.
        Toggles,
        /// The tick's commands learn ranks.
        Learn,
        /// The tick's commands order production.
        Produce,
        /// The tick's commands buy, sell and swap items.
        Trade,
        /// The tick's commands order units.
        Order,
        /// Carried items push their grants.
        HoldItems,
        /// The sources hold their grants, then the grants apply.
        HoldAtInputs,
        GrantAtInputs,
        /// The build orders are checked against what the tick's commands changed.
        CheckBuilds,
        /// AI of the units due this tick issues orders.
        Think,
        /// Units chase their targets, attacks wind up, actions start, then builds.
        Chase,
        Attack,
        Start,
        StartBuilds,
        /// Units move.
        Move,
        /// The mode's collision backend resolves overlaps.
        Collide,
        /// Projectiles fly, attacks hit, deliveries trigger and deliver.
        Fly,
        Hits,
        Trigger,
        Deliver,
        /// Attacks pay, the sources hold their grants, the grants apply, then attacks strike.
        Pay,
        HoldAtStrike,
        GrantAtStrike,
        Strike,
        /// Actions fire, casts resolve, deliveries launch and land, and intervals tick.
        Fire,
        CastResolve,
        Launch,
        Land,
        Interval,
        /// Damage is dealt and actions resolve.
        Damage,
        Resolve,
        /// The sources hold their grants, the grants apply, units die and the dead's modifiers
        /// end.
        HoldAtResolve,
        GrantAtResolve,
        Die,
        ClearDead,
        /// Areas, then auras, players and held modifiers, hold their modifiers.
        HoldInside,
        HoldModifiers,
        /// Production finishes, then the mode's hooks run.
        Finish,
        ModeHooks,
        /// The sources hold their grants and the grants apply.
        HoldAtEnd,
        GrantAtEnd,
        /// The tick ends: units leave play, despawn and end their lifespans, supply is counted
        /// and the mode's vision backend marks what each team may see.
        End,
    }
    pub const ALL;
}

impl Phase {
    /// The stage it runs in.
    pub const fn stage(self) -> SimSet {
        match self {
            Self::Begin
            | Self::ModeInputs
            | Self::Toggles
            | Self::Learn
            | Self::Produce
            | Self::Trade
            | Self::Order
            | Self::HoldItems
            | Self::HoldAtInputs
            | Self::GrantAtInputs
            | Self::CheckBuilds => SimSet::Inputs,
            Self::Think => SimSet::Think,
            Self::Chase | Self::Attack | Self::Start | Self::StartBuilds => SimSet::Act,
            Self::Move => SimSet::Move,
            Self::Collide => SimSet::Collide,
            Self::Fly
            | Self::Hits
            | Self::Trigger
            | Self::Deliver
            | Self::Pay
            | Self::HoldAtStrike
            | Self::GrantAtStrike
            | Self::Strike
            | Self::Fire
            | Self::CastResolve
            | Self::Launch
            | Self::Land
            | Self::Interval => SimSet::Hit,
            Self::Damage
            | Self::Resolve
            | Self::HoldAtResolve
            | Self::GrantAtResolve
            | Self::Die
            | Self::ClearDead
            | Self::HoldInside
            | Self::HoldModifiers => SimSet::Resolve,
            Self::Finish | Self::ModeHooks => SimSet::Mode,
            Self::HoldAtEnd | Self::GrantAtEnd | Self::End => SimSet::Vision,
        }
    }

    /// Puts the phase into its stage, after the phase before it in the list when that one is of
    /// the same stage.
    pub fn configure(self, schedule: &mut Schedule) {
        let at = Self::ALL.iter().position(|&phase| phase == self).unwrap();
        let before = at.checked_sub(1).map(|before| Self::ALL[before]);
        match before.filter(|before| before.stage() == self.stage()) {
            Some(before) => schedule.configure_sets(self.in_set(self.stage()).after(before)),
            None => schedule.configure_sets(self.in_set(self.stage())),
        };
    }
}

#[cfg(test)]
mod tests {
    use campfire_sim::SimSet;

    use crate::phase::Phase;

    #[test]
    fn the_phases_of_each_stage_are_one_run_in_the_order_of_the_stages() {
        let mut runs: Vec<SimSet> = Phase::ALL.iter().map(|phase| phase.stage()).collect();
        runs.dedup();
        assert_eq!(runs, SimSet::ALL);
    }
}
