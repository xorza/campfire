use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_actions::{ActionBook, ActionKind};
use campfire_math::{Num, Rounding};
use campfire_players::{PlayersWorld, ResourceAmount};
use campfire_sim::{SimComponent, StableId};
use campfire_state_types::{DataKind, Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_values::{ActionId, NameId, Rank};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// A building under construction: the build that placed it, at its rank, the build's time in ticks,
/// the life it gains over that time, the player resources its build paid, which a cancel gives back
/// a share of, and, of a `builder` site, the builder that holds it. Its progress, which changes in
/// each tick its builders build it, is its `SiteProgress`, so a change of the progress leaves the
/// site unchanged.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize)]
#[require(SiteProgress)]
pub struct Site {
    action: ActionId,
    rank: Rank,
    time: Num,
    gain: Num,
    paid: Vec<ResourceAmount>,
    holder: Option<StableId>,
}

/// A site's progress in ticks toward its build's time.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct SiteProgress(Num);

/// What a build places a site of: its build, at its rank, the build's time in ticks, the life the
/// site gains over that time, the player resources the build paid, and the builder that holds a
/// `builder` site.
#[derive(Debug, Clone, Copy)]
pub struct SiteStart<'a> {
    pub action: ActionId,
    pub rank: Rank,
    pub time: Num,
    pub gain: Num,
    pub paid: &'a [ResourceAmount],
    pub holder: Option<StableId>,
}

/// What a site's progress in one tick did: the life it gains, and whether it completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progressed {
    pub(crate) life: Num,
    pub(crate) complete: bool,
}

impl Site {
    /// A site of no progress, as `start` places it.
    pub fn new(start: SiteStart<'_>) -> Self {
        let SiteStart {
            action,
            rank,
            time,
            gain,
            paid,
            holder,
        } = start;
        debug_assert!(gain >= Num::ZERO && time >= Num::ZERO);
        Self {
            action,
            rank,
            time,
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

    /// Its build's time in ticks.
    pub const fn time(&self) -> Num {
        self.time
    }

    pub(crate) fn paid(&self) -> &[ResourceAmount] {
        &self.paid
    }
}

impl SiteProgress {
    pub const fn get(self) -> Num {
        self.0
    }

    /// The share of `site`'s time its progress reached, from 0 to 1, rounded down.
    pub fn share(self, site: &Site) -> Num {
        if site.time == Num::ZERO {
            return Num::ONE;
        }
        self.0
            .checked_mul_div(Num::ONE, site.time, Rounding::Floor)
            .expect("a share from 0 to 1 fits")
    }

    /// Adds `rate` to the progress of `site` toward its time, at most its time: the life it
    /// gains is the site's gain's share at the new progress less its share at the old, each
    /// `gain · progress / time` computed exactly and rounded down, so a site that completes has
    /// gained exactly its gain. A site that completed takes no more, as it is a site no more.
    pub fn grow(&mut self, site: &Site, rate: Num) -> Progressed {
        debug_assert!(rate >= Num::ZERO);
        let time = site.time;
        if time == Num::ZERO {
            return Progressed {
                life: site.gain,
                complete: true,
            };
        }
        let before = self.0;
        self.0 = (before + rate).min(time);
        let share = |progress: Num| {
            site.gain
                .checked_mul_div(progress, time, Rounding::Floor)
                .expect("a share of the gain fits")
        };
        Progressed {
            life: share(self.0) - share(before),
            complete: self.0 == time,
        }
    }
}

impl SimComponent for Site {
    const NAME: &'static str = "production.site";
    type Host = UnitEntity;

    // A build the book lacks, or of a rank past its ranks, has no time to grow toward, and a time
    // other than its build's would grow another site; a paid resource the mode lacks, or an
    // amount below 0, has no refund to give.
    fn check(&self, world: &World, _: Entity) -> bool {
        let build = world
            .resource::<ActionBook>()
            .get(self.action)
            .is_some_and(|action| {
                action.kind == ActionKind::Build
                    && action.has_rank(self.rank)
                    && action.windup_ticks(self.rank) == Some(self.time)
            });
        let resources = world.declared_resources();
        let paid = self
            .paid
            .iter()
            .all(|paid| paid.resource.index() < resources && paid.amount >= 0);
        build && paid
    }
}

impl Replication for Site {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentPredicted;
}

impl SimComponent for SiteProgress {
    const NAME: &'static str = "production.site_progress";
    type Host = UnitEntity;

    // A site's, whose time it reaches at most: a progress past that time would take life back.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world
            .get::<Site>(entity)
            .is_some_and(|site| self.0 <= site.time)
    }
}

impl Replication for SiteProgress {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentPredicted;
}

/// A snapshot is untrusted, so a progress below 0 fails to decode.
impl<'de> Deserialize<'de> for SiteProgress {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let progress = Num::deserialize(deserializer)?;
        if progress < Num::ZERO {
            return Err(D::Error::custom("a site's progress from 0"));
        }
        Ok(Self(progress))
    }
}

/// A snapshot is untrusted, so a time or a gain below 0 fails to decode.
impl<'de> Deserialize<'de> for Site {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            action: ActionId,
            rank: Rank,
            time: Num,
            gain: Num,
            paid: Vec<ResourceAmount>,
            holder: Option<StableId>,
        }
        let Fields {
            action,
            rank,
            time,
            gain,
            paid,
            holder,
        } = Fields::deserialize(deserializer)?;
        if time < Num::ZERO || gain < Num::ZERO {
            return Err(D::Error::custom("a site's time and gain from 0"));
        }
        Ok(Self {
            action,
            rank,
            time,
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
        let time = Num::int(3);
        let start = |time| SiteStart {
            action: ActionId::new(0),
            rank: Rank::FIRST,
            time,
            gain: Num::int(100),
            paid: &[],
            holder: None,
        };
        let site = Site::new(start(time));
        let mut progress = SiteProgress::default();
        let third = |of: i64| Num::from_bits(of * Num::int(100).to_bits() / 3);
        let first = progress.grow(&site, Num::ONE);
        assert_eq!(
            first,
            Progressed {
                life: third(1),
                complete: false
            }
        );
        // Its share of the time is a third, rounded down to a bit.
        assert_eq!(
            progress.share(&site),
            Num::from_bits(Num::ONE.to_bits() / 3)
        );
        let second = progress.grow(&site, Num::ONE);
        assert_eq!(second.life, third(2) - third(1));
        let last = progress.grow(&site, Num::int(5));
        assert_eq!(
            last,
            Progressed {
                life: Num::int(100) - third(2),
                complete: true
            }
        );
        assert_eq!(first.life + second.life + last.life, Num::int(100));
        assert_eq!((progress.get(), progress.share(&site)), (time, Num::ONE));
        // A rate of 0 adds nothing; a time of 0 completes at once with the whole gain.
        let mut idle = SiteProgress::default();
        assert_eq!(
            idle.grow(&site, Num::ZERO),
            Progressed {
                life: Num::ZERO,
                complete: false
            }
        );
        let at_once = Site::new(start(Num::ZERO));
        assert_eq!(
            idle.grow(&at_once, Num::ZERO),
            Progressed {
                life: Num::int(100),
                complete: true
            }
        );
        assert_eq!(idle.share(&at_once), Num::ONE);
    }
}
