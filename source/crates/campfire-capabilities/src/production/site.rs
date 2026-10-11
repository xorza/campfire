use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Num, Rounding};
use campfire_sim::{SimComponent, StableId};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::actions::action_book::ActionBook;
use crate::actions::action_kind::ActionKind;
use crate::players::player_resources::PlayerResources;
use crate::players::resource_amount::ResourceAmount;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::NotSent;
use crate::units::action_id::ActionId;
use crate::values::rank::Rank;

/// A building under construction: the build that placed it, at its rank, its progress in ticks
/// toward the build's time, the life it gains over that time, the player resources its build
/// paid, which a cancel gives back a share of, and, of a `builder` site, the builder that holds
/// it.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Site {
    action: ActionId,
    rank: Rank,
    progress: Num,
    gain: Num,
    paid: Vec<ResourceAmount>,
    holder: Option<StableId>,
}

/// What a site's progress in one tick did: the life it gains, and whether it completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Progressed {
    pub(crate) life: Num,
    pub(crate) complete: bool,
}

impl Site {
    /// A site of `action` at `rank`, of no progress, that gains `gain` life over its time, whose
    /// build paid `paid`, held by `holder`.
    pub(crate) fn new(
        action: ActionId,
        rank: Rank,
        gain: Num,
        paid: &[ResourceAmount],
        holder: Option<StableId>,
    ) -> Site {
        debug_assert!(gain >= Num::ZERO);
        Site {
            action,
            rank,
            progress: Num::ZERO,
            gain,
            paid: paid.to_vec(),
            holder,
        }
    }

    /// The builder that holds a `builder` site.
    pub const fn holder(&self) -> Option<StableId> {
        self.holder
    }

    pub(crate) const fn hold(&mut self, holder: StableId) {
        self.holder = Some(holder);
    }

    pub const fn action(&self) -> ActionId {
        self.action
    }

    pub const fn rank(&self) -> Rank {
        self.rank
    }

    pub const fn progress(&self) -> Num {
        self.progress
    }

    pub(crate) fn paid(&self) -> &[ResourceAmount] {
        &self.paid
    }

    /// Adds `rate` to its progress toward `time` ticks, at most `time`: the life it gains is the
    /// gain's share at the new progress less its share at the old, each `gain · progress / time`
    /// computed exactly and rounded down, so a site that completes has gained exactly its gain.
    pub(crate) fn progress_by(&mut self, rate: Num, time: Num) -> Progressed {
        debug_assert!(rate >= Num::ZERO && time >= Num::ZERO);
        if time == Num::ZERO {
            let life = self.gain;
            self.gain = Num::ZERO;
            return Progressed {
                life,
                complete: true,
            };
        }
        let before = self.progress;
        self.progress = (before + rate).min(time);
        let share = |progress: Num| {
            self.gain
                .checked_mul_div(progress, time, Rounding::Floor)
                .expect("a share of the gain fits")
        };
        Progressed {
            life: share(self.progress) - share(before),
            complete: self.progress == time,
        }
    }
}

impl SimComponent for Site {
    const NAME: &'static str = "production.site";

    // A build the book lacks, or of a rank past its ranks, has no time to grow toward, and a
    // progress past that time would take life back; a paid resource the mode lacks, or an amount
    // below 0, has no refund to give.
    fn check(&self, world: &World, _: Entity) -> bool {
        let build = world
            .get_resource::<ActionBook>()
            .and_then(|book| book.get(self.action))
            .is_some_and(|action| {
                action.kind.kind() == ActionKind::Build
                    && action.has_rank(self.rank)
                    && action
                        .windup_ticks(self.rank)
                        .is_some_and(|time| self.progress <= time)
            });
        let resources = world
            .get_resource::<PlayerResources>()
            .map_or(0, PlayerResources::resources);
        let paid = self
            .paid
            .iter()
            .all(|paid| paid.resource.index() < resources && paid.amount >= 0);
        build && paid
    }
}

impl Replication for Site {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}

/// A snapshot is untrusted, so a progress or a gain below 0 fails to decode.
impl<'de> Deserialize<'de> for Site {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Site, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            action: ActionId,
            rank: Rank,
            progress: Num,
            gain: Num,
            paid: Vec<ResourceAmount>,
            holder: Option<StableId>,
        }
        let Fields {
            action,
            rank,
            progress,
            gain,
            paid,
            holder,
        } = Fields::deserialize(deserializer)?;
        if progress < Num::ZERO || gain < Num::ZERO {
            return Err(D::Error::custom("a site's progress and gain from 0"));
        }
        Ok(Site {
            action,
            rank,
            progress,
            gain,
            paid,
            holder,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_site_gains_exactly_its_gain_over_its_time_with_no_rounding_carried() {
        // A gain of 100 over 3 ticks: 33.333… after one, 66.666… after two, rounded down to a
        // bit each, so the ticks add the differences, and the third completes it at exactly 100.
        let mut site = Site::new(ActionId::new(0), Rank::FIRST, Num::int(100), &[], None);
        let time = Num::int(3);
        let third = |of: i64| Num::from_bits(of * Num::int(100).to_bits() / 3);
        let first = site.progress_by(Num::ONE, time);
        assert_eq!(
            first,
            Progressed {
                life: third(1),
                complete: false
            }
        );
        let second = site.progress_by(Num::ONE, time);
        assert_eq!(second.life, third(2) - third(1));
        let last = site.progress_by(Num::int(5), time);
        assert_eq!(
            last,
            Progressed {
                life: Num::int(100) - third(2),
                complete: true
            }
        );
        assert_eq!(first.life + second.life + last.life, Num::int(100));
        assert_eq!(site.progress(), time);
        // A rate of 0 adds nothing.
        let mut idle = Site::new(ActionId::new(0), Rank::FIRST, Num::int(100), &[], None);
        assert_eq!(
            idle.progress_by(Num::ZERO, time),
            Progressed {
                life: Num::ZERO,
                complete: false
            }
        );
    }
}
