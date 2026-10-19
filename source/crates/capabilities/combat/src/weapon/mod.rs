use std::num::NonZeroU16;

use campfire_common::Ticks;
use campfire_geometry::Volume;
use campfire_math::{Num, Rounding, SinCos, Vec3};
use campfire_units::StatId;
use campfire_values::{DamageKind, NameId, Rank, Speed, Tag, TagSet};

use crate::attack_data::SplashData;

/// What makes an action of kind `attack` a weapon, as the match runs it: where its rate and its
/// damage come from, the kind of damage it deals, its clip at each rank, its projectile's splash
/// and scatter, its bound of pitch, and the speed that delays the hit of a weapon of no
/// projectile, if it has them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weapon {
    pub(crate) rate: Rate,
    pub(crate) damage: WeaponDamage,
    pub kind: DamageKind,
    pub(crate) clip: Option<Box<[ClipRule]>>,
    pub splash: Option<SplashData>,
    pub scatter: Option<Scatter>,
    pub pitch: Option<Pitch>,
    pub speed: Option<Speed>,
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

/// A weapon's clip at one rank: the shots it fires before it reloads, and the ticks from the shot
/// that empties it to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipRule {
    pub shots: NonZeroU16,
    pub reload: Ticks,
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
    /// period, or the tick rate over its rate exactly, rounded up, at least one bit of a second;
    /// either longer than its windup.
    pub fn period(&self, stats: &[Num], rank: Rank, windup: Ticks, hz: u32) -> Ticks {
        let period = match &self.rate {
            Rate::Period(periods) => periods[rank.index()],
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

    /// The damage it deals at `rank` for a unit of stats `stats`: its own amount, or its stat,
    /// not negative.
    pub fn damage(&self, stats: &[Num], rank: Rank) -> Num {
        match &self.damage {
            WeaponDamage::Amount(amounts) => amounts[rank.index()],
            WeaponDamage::Stat(damage) => {
                let damage = stats.get(damage.index()).copied();
                damage.unwrap_or(Num::ZERO).max(Num::ZERO)
            }
        }
    }

    /// Its clip at `rank`, if it has one.
    pub fn clip(&self, rank: Rank) -> Option<ClipRule> {
        self.clip.as_ref().map(|clips| clips[rank.index()])
    }
}

#[cfg(test)]
mod tests;
