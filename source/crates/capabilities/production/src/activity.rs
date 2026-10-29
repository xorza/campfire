use bevy_ecs::world::EntityRef;
use campfire_actions::{ActionProgress, InProgress};
use campfire_units::{AtWork, Destination};

use crate::builder::Builder;
use crate::gatherer::Gatherer;

/// What a unit does, as a match's probe, its console and a bot read it, one classifier for all:
/// it fights while it holds an attack target or attacks, acts while another action is under way,
/// works while its building goes on, walks while it has a destination, works too while a gather
/// or a build order waits, as at a blocked site, and else stands idle: a walker idle here is one
/// steering takes as an idle friend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Idle,
    Walking,
    Working,
    Fighting,
    Acting,
}

/// The parts of a unit its activity reads, each none for a unit that holds none.
#[derive(Debug, Clone, Copy, Default)]
pub struct ActivityParts<'a> {
    pub progress: Option<&'a ActionProgress>,
    pub destination: Option<&'a Destination>,
    pub at_work: Option<&'a AtWork>,
    pub gatherer: Option<&'a Gatherer>,
    pub builder: Option<&'a Builder>,
}

impl<'a> ActivityParts<'a> {
    /// The parts `unit` holds.
    pub fn of(unit: &EntityRef<'a>) -> Self {
        Self {
            progress: unit.get(),
            destination: unit.get(),
            at_work: unit.get(),
            gatherer: unit.get(),
            builder: unit.get(),
        }
    }

    pub fn activity(self) -> Activity {
        let underway = self.progress.and_then(ActionProgress::in_progress);
        let attacks = self
            .progress
            .and_then(ActionProgress::attack_target)
            .is_some()
            || matches!(underway, Some(InProgress::Attack { .. }));
        let walks = (self.destination).is_some_and(|destination| destination.get().is_some());
        let ordered = self
            .gatherer
            .is_some_and(|gatherer| gatherer.order().is_some())
            || self
                .builder
                .is_some_and(|builder| builder.order().is_some());
        if attacks {
            Activity::Fighting
        } else if underway.is_some() {
            Activity::Acting
        } else if self.at_work.is_some_and(|at_work| at_work.get()) {
            Activity::Working
        } else if walks {
            Activity::Walking
        } else if ordered {
            Activity::Working
        } else {
            Activity::Idle
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_common::PlayerSlot;
    use campfire_math::Vec3;
    use campfire_sim::StableId;
    use campfire_units::BuildTarget;

    use super::*;
    use crate::builder::BuildOrder;

    #[test]
    fn a_unit_works_while_its_build_waits_and_stands_idle_with_none() {
        // A unit of no part stands idle; one walking walks; one at work, or one whose build
        // waits at a blocked site with no walk, works, as a bot and a probe read it alike.
        let walk = Destination::to(Some(Vec3::ZERO));
        let at_work = AtWork::new(true);
        let order = BuildOrder {
            slot: 0,
            target: BuildTarget::Site(StableId::of_player(PlayerSlot::new(0))),
        };
        let builder = Builder::of(Some(order));
        let idle = Builder::of(None);
        let activity = |parts: ActivityParts<'_>| parts.activity();
        assert_eq!(activity(ActivityParts::default()), Activity::Idle);
        let walking = ActivityParts {
            destination: Some(&walk),
            ..ActivityParts::default()
        };
        assert_eq!(activity(walking), Activity::Walking);
        let working = ActivityParts {
            at_work: Some(&at_work),
            ..ActivityParts::default()
        };
        assert_eq!(activity(working), Activity::Working);
        let waiting = ActivityParts {
            builder: Some(&builder),
            ..ActivityParts::default()
        };
        assert_eq!(activity(waiting), Activity::Working);
        let unordered = ActivityParts {
            builder: Some(&idle),
            ..ActivityParts::default()
        };
        assert_eq!(activity(unordered), Activity::Idle);
    }
}
