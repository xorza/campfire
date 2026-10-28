use std::num::NonZeroU16;

use campfire_actions::RangeBonus;
use campfire_common::Ticks;
use campfire_declarations::PerId;
use campfire_geometry::Volume;
use campfire_math::{Num, Rounding, SinCos, Vec3};
use campfire_units::StatId;
use campfire_values::{
    ActionId, DamageKind, ModifierId, NameId, Rank, RelationSet, Speed, Tag, TagSet,
};

use campfire_script::Filter;

use crate::armor::Armor;
use crate::attack_data::{ClipRefill, PreAttackEach};
use crate::bonus_stats::BonusStats;

/// What makes an action of kind `attack` a weapon, as the match runs it: where its rate and its
/// damage come from, the stat that speeds its own period, the kind of damage it deals, the filter
/// of the targets it prefers, its clip at each rank, its projectile's splash and scatter, its
/// bound of pitch, the speed that delays
/// the hit of a weapon of no projectile, and its shot streak, if it has them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weapon {
    pub(crate) rate: Rate,
    pub(crate) rate_bonus: Option<BonusStats>,
    pub(crate) range_bonus: Option<RangeBonus>,
    pub(crate) damage: WeaponDamage,
    pub(crate) damage_bonus: Option<BonusStats>,
    pub kind: DamageKind,
    pub(crate) prefers: Option<Filter>,
    pub(crate) clip: Option<Box<[ClipRule]>>,
    pub splash: Option<Splash>,
    pub scatter: Option<Scatter>,
    pub pitch: Option<Pitch>,
    pub speed: Option<Speed>,
    pub streak: Option<Streak>,
    pub(crate) pre_attack: Option<PreAttackRule>,
    pub(crate) aim: Option<Num>,
}

/// A weapon's pre-attack as the match runs it (04-capabilities/combat.md, A pre-attack): its
/// ticks at each rank, and what starts it again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreAttackRule {
    pub(crate) ticks: Box<[Ticks]>,
    pub(crate) each: PreAttackEach,
}

/// A weapon's splash as the match runs it (04-capabilities/combat.md, An attack may splash):
/// where its projectile ends, its damage to each living unit of the relations `affects` names
/// whose volume lies within `radius` meters, and to its source unless `spares_source`; and its
/// outer ring, if it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Splash {
    pub(crate) radius: Num,
    pub(crate) affects: RelationSet,
    pub(crate) spares_source: bool,
    pub(crate) outer: Option<OuterRing>,
}

/// A splash's outer ring (04-capabilities/combat.md, A splash may have an outer ring): its radius,
/// past its splash's, and its amount at each rank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OuterRing {
    pub(crate) radius: Num,
    pub(crate) amounts: Box<[Num]>,
}

/// A weapon's shot streak (04-capabilities/combat.md, A shot streak gives a stage's modifier): the
/// ticks after the weapon is ready again that a streak holds without a shot, its stages, their
/// shots strictly growing, and its fall, if it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Streak {
    pub(crate) coast: Ticks,
    pub(crate) stages: Box<[StreakStage]>,
    pub(crate) fall: Option<StreakFall>,
}

/// A streak's fall (04-capabilities/combat.md, A streak's fall holds a modifier): the modifier its
/// unit holds from a fall of its stage, and the ticks after a fall past the coast it holds
/// without a shot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StreakFall {
    pub(crate) modifier: ModifierId,
    pub(crate) hold: Ticks,
}

/// A stage of a streak: the shots in a row past which its unit holds `modifier`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StreakStage {
    pub(crate) shots: u32,
    pub(crate) modifier: ModifierId,
}

/// Where a weapon's rate comes from: a place among its unit's stats, its attacks a second, or its
/// own time from one attack's start to the next at each rank, a tick at least.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rate {
    Stat(StatId),
    Period(Box<[Ticks]>),
}

/// Where a weapon's damage comes from: a place among its unit's stats, or its own amount at each
/// rank, at least 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeaponDamage {
    Stat(StatId),
    Amount(Box<[Num]>),
}

/// A weapon's clip at one rank: the shots it fires before it reloads, the ticks of its reload,
/// and how it refills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipRule {
    pub shots: NonZeroU16,
    pub reload: Ticks,
    pub refill: ClipRefill,
}

/// How far from its target's ground point a weapon's projectile may aim
/// (04-capabilities/combat.md, An attack may scatter): `radius`, and for a target of a tag of
/// `tags`, the largest of their radii more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scatter {
    pub(crate) radius: Num,
    pub(crate) tags: Box<[ScatterTag]>,
}

/// A tag a scatter widens at, and its radius.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScatterTag {
    pub(crate) tag: Tag,
    pub(crate) radius: Num,
}

/// The pitches a weapon's target lies within (04-capabilities/combat.md, An attack may bound its
/// pitch): the sines and cosines of its least and its most, fixed at the load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pitch {
    pub(crate) low: SinCos,
    pub(crate) high: SinCos,
}

impl Scatter {
    /// Its radius at a target of `tags`.
    pub fn radius(&self, tags: TagSet) -> Num {
        let widest = self.tags.iter().filter(|tag| tags.contains(tag.tag));
        self.radius + widest.map(|tag| tag.radius).max().unwrap_or(Num::ZERO)
    }

    /// The point of the ground a shot at a unit at `position` of `tags` aims at, when its radius
    /// there is above 0: at a distance from the unit's ground point of the radius times the first
    /// of `draw`'s fractions, at an angle of a turn times the second, from `x` toward `−z` as a
    /// facing turns, each axis's offset the product of the radius, the fraction and the angle's
    /// cosine or sine rounded once, as Zero Hour's `fireWeaponTemplate` scatters a shot. None at a
    /// radius of 0, which draws nothing.
    pub fn point(
        &self,
        position: Vec3,
        tags: TagSet,
        draw: impl FnOnce() -> [Num; 2],
    ) -> Option<Vec3> {
        let radius = self.radius(tags);
        if radius <= Num::ZERO {
            return None;
        }
        let [distance, turn] = draw();
        debug_assert!(
            [distance, turn]
                .iter()
                .all(|draw| (Num::ZERO..Num::ONE).contains(draw)),
            "a draw is a fraction"
        );
        let SinCos { sin, cos } = SinCos::of_degrees(Num::int(360) * turn);
        let offset = |axis: Num| {
            radius
                .checked_mul_mul(distance, axis, Rounding::NearestEven)
                .expect("a scatter's offset is within its radius")
        };
        let x = position.x + offset(cos);
        let z = position.z - offset(sin);
        Some(Vec3::new(x, Num::ZERO, z))
    }
}

impl Pitch {
    /// The bound of `low` and `high` degrees, as the load reads them.
    pub fn of(low: Num, high: Num) -> Self {
        Self {
            low: SinCos::of_degrees(low),
            high: SinCos::of_degrees(high),
        }
    }

    /// Whether `target` lies within its pitches from the point `from`, against the ground plane,
    /// as Zero Hour's `Weapon::isWithinTargetPitch` measures a target: the pitches from `from` to
    /// the volume's bottom and to its top, at its place on the ground plane, meet its least and
    /// its most. A pitch `φ`, its rise `y` over its run on the ground `r`, is at least `θ` exactly
    /// when `sin(φ − θ) ≥ 0`, so `y cos θ ≥ r sin θ`, the run to the nearest bit and each product
    /// exact.
    pub fn reaches(&self, from: Vec3, target: &Volume) -> bool {
        let bottom = target.base.position;
        let top = i128::from((bottom.y + target.height).to_bits());
        let run = i128::from(from.ground_offset(bottom).length().to_bits());
        let side = |bound: SinCos, height: i128| {
            let rise = height - i128::from(from.y.to_bits());
            rise * i128::from(bound.cos.to_bits()) - run * i128::from(bound.sin.to_bits())
        };
        side(self.low, top) >= 0 && side(self.high, i128::from(bottom.y.to_bits())) <= 0
    }
}

impl Weapon {
    /// The ticks from the start of one attack to the next at the earliest, at `hz` ticks a second,
    /// for a unit of stats `stats` and the weapon at `rank`, of a windup of `windup`: its own
    /// period, sped by its rate bonus, or the tick rate over its rate exactly, rounded up, at
    /// least one bit of a second; either longer than its windup.
    pub fn period(&self, stats: &[Num], rank: Rank, windup: Ticks, hz: u32) -> Ticks {
        let period = match &self.rate {
            Rate::Period(periods) => self.sped(stats, periods[rank.index()]),
            Rate::Stat(rate) => {
                let rate = stats.get(rate.index()).copied().unwrap_or(Num::ZERO);
                // A count of ticks, not a number: up to 2³² · 2²⁴ at the least rate, past a `Num`.
                let rate = i128::from(rate.to_bits().max(1));
                let ticks = Rounding::Ceiling.divide(i128::from(hz) << Num::FRAC_BITS, rate);
                Ticks::new(u64::try_from(ticks).expect("2⁵⁶ ticks at most"))
            }
        };
        Ticks::new(period.get().max(windup.get() + 1))
    }

    /// The ticks its hit lands after its shot at a target `distance` meters away, at `hz` ticks a
    /// second (04-capabilities/combat.md, A slow weapon delays its damage): ⌈`d` ÷ `s`⌉, for `s`
    /// its speed a tick, and none when `d` ÷ `s` is below 1, as Zero Hour's `fireWeaponTemplate`
    /// deals such a hit at once; none for a weapon of no speed. Exact: `d` ÷ `s` is `d · hz` over
    /// its speed a second, in bits.
    pub fn delay(&self, distance: Num, hz: u32) -> Option<Ticks> {
        let speed = i128::from(self.speed?.get().to_bits());
        let scaled = i128::from(distance.to_bits()) * i128::from(hz);
        (scaled >= speed).then(|| {
            let ticks = Rounding::Ceiling.divide(scaled, speed);
            Ticks::new(u64::try_from(ticks).expect("a delay within the world's bound"))
        })
    }

    /// The damage it deals at `rank` for a unit of stats `stats`: its own amount under its damage
    /// bonus, or its stat, not negative.
    pub fn damage(&self, stats: &[Num], rank: Rank) -> Num {
        match &self.damage {
            WeaponDamage::Amount(amounts) => self.bonused(stats, amounts[rank.index()]),
            WeaponDamage::Stat(damage) => {
                let damage = stats.get(damage.index()).copied();
                damage.unwrap_or(Num::ZERO).max(Num::ZERO)
            }
        }
    }

    /// The amount its splash's outer ring deals at `rank` for a unit of stats `stats`, under its
    /// damage bonus, if it has one.
    pub(crate) fn outer_amount(&self, stats: &[Num], rank: Rank) -> Option<Num> {
        let outer = self.splash.as_ref()?.outer.as_ref()?;
        Some(self.bonused(stats, outer.amounts[rank.index()]))
    }

    /// What it would deal at `rank` for a unit of stats `stats` to a target of `armor`, its
    /// damage as the armor takes its kind, the whole for none (combat-response.md, An estimate is
    /// the weapon's damage as the armor takes it).
    pub(crate) fn estimate(&self, stats: &[Num], rank: Rank, armor: Option<&Armor>) -> Num {
        let damage = self.damage(stats, rank);
        armor.map_or(damage, |armor| armor.take(self.kind, damage))
    }

    /// Whether it prefers a target of `tags`, which its filter of preference selects.
    pub(crate) fn prefers(&self, tags: TagSet) -> bool {
        self.prefers
            .is_some_and(|prefers| prefers.selects_tags(tags))
    }

    /// `amount` times 1 plus the value of its damage bonus for a unit of stats `stats`, from the
    /// exact product, rounded once, as Zero Hour's weapon bonus scales its primary and its
    /// secondary damage alike (04-capabilities/combat.md, One bonus scales both rings); a factor
    /// below 0 counts as 0, and one past the range of numbers saturates.
    fn bonused(&self, stats: &[Num], amount: Num) -> Num {
        let Some(bonus) = &self.damage_bonus else {
            return amount;
        };
        let bonus = bonus.value(stats);
        let factor = Num::ONE
            .checked_add(bonus)
            .unwrap_or(Num::MAX)
            .max(Num::ZERO);
        amount.saturating_mul(factor, Rounding::NearestEven)
    }

    /// Whether it fires in clips, at every rank.
    pub const fn has_clip(&self) -> bool {
        self.clip.is_some()
    }

    /// The shots of its clip at `rank`, if it has one.
    pub fn clip_shots(&self, rank: Rank) -> Option<NonZeroU16> {
        Some(self.clip.as_ref()?[rank.index()].shots)
    }

    /// Its clip at `rank`, if it has one, its reload sped by its rate bonus for a unit of stats
    /// `stats`.
    pub fn clip(&self, stats: &[Num], rank: Rank) -> Option<ClipRule> {
        let clip = self.clip.as_ref()?[rank.index()];
        Some(ClipRule {
            reload: self.sped(stats, clip.reload),
            ..clip
        })
    }

    /// `ticks` of its own, for a unit of stats `stats`, divided by 1 plus the value of its rate
    /// bonus and rounded down, as Zero Hour's `getDelayBetweenShots` floors its frames over its
    /// rate of fire (04-capabilities/combat.md, A weapon's own period may speed up by a
    /// stat); a divisor below one bit counts as one bit, and the ticks stop at 2⁵⁶, as long as
    /// the stat rate's longest period.
    fn sped(&self, stats: &[Num], ticks: Ticks) -> Ticks {
        let Some(bonus) = &self.rate_bonus else {
            return ticks;
        };
        let bonus = bonus.value(stats);
        let divisor = (i128::from(Num::ONE.to_bits()) + i128::from(bonus.to_bits())).max(1);
        let sped = (i128::from(ticks.get()) << Num::FRAC_BITS) / divisor;
        Ticks::new(u64::try_from(sped.min(1 << 56)).expect("2⁵⁶ ticks at most"))
    }
}

impl Splash {
    /// The farthest it reaches: its outer ring's radius, or its own.
    pub(crate) fn reach(&self) -> Num {
        self.outer
            .as_ref()
            .map_or(self.radius, |outer| outer.radius)
    }
}

impl PerId<ActionId> for Weapon {
    const SCRIPTS_ONLY: bool = false;
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_common::Ticks;
    use campfire_geometry::MAX_BOUND;
    use campfire_math::Num;
    use campfire_values::{ModifierId, RelationSet};

    use crate::attack_data::PreAttackEach;
    use crate::weapon::{OuterRing, PreAttackRule, Splash, Streak, StreakFall, StreakStage};

    impl PreAttackRule {
        /// A pre-attack of `ticks` at each rank, started again `each`.
        pub const fn new(ticks: Box<[Ticks]>, each: PreAttackEach) -> Self {
            Self { ticks, each }
        }
    }

    impl Splash {
        /// A splash of `radius` to the units of `affects`, and to its source unless
        /// `spares_source`; `None` for a radius below 0 or past `MAX_BOUND`.
        pub fn new(radius: Num, affects: RelationSet, spares_source: bool) -> Option<Self> {
            (Num::ZERO..=MAX_BOUND).contains(&radius).then_some(Self {
                radius,
                affects,
                spares_source,
                outer: None,
            })
        }

        /// The same splash with an outer ring of `radius` dealing `amounts` at each rank; `None`
        /// for a ring not past its radius or past `MAX_BOUND`.
        #[must_use]
        pub fn with_outer(self, radius: Num, amounts: Box<[Num]>) -> Option<Self> {
            (self.radius < radius && radius <= MAX_BOUND).then_some(Self {
                outer: Some(OuterRing { radius, amounts }),
                ..self
            })
        }
    }

    impl Streak {
        /// A streak of `coast` and of `stages`, each its shots and its modifier, their shots
        /// strictly growing.
        pub fn new(coast: Ticks, stages: &[(u32, ModifierId)]) -> Self {
            debug_assert!(
                !stages.is_empty() && stages.is_sorted_by(|low, high| low.0 < high.0),
                "a streak's stages are some, their shots strictly growing"
            );
            let stages = stages
                .iter()
                .map(|&(shots, modifier)| StreakStage { shots, modifier });
            Self {
                coast,
                stages: stages.collect(),
                fall: None,
            }
        }

        /// It, with a fall of `modifier` held `hold` ticks after a fall past the coast.
        #[must_use]
        pub fn with_fall(self, modifier: ModifierId, hold: Ticks) -> Self {
            Self {
                fall: Some(StreakFall { modifier, hold }),
                ..self
            }
        }
    }
}

#[cfg(test)]
mod tests;
