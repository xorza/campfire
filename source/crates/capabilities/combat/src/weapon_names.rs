use std::num::NonZeroU16;

use campfire_actions::{Action, DeliveryAim, LoadedAction, RangeBonus};
use campfire_common::{DeclaredName, Ticks};
use campfire_declarations::BookError;
use campfire_math::Num;
use campfire_names::{Filter, TagNames, UnknownName};
use campfire_sim::MatchRate;
use campfire_stats::{ModifierBook, StatBook};
use campfire_units::StatId;
use campfire_values::{
    DamageKind, ModifierId, NameKind, Named, Names, Number, Rank, Ranked, Scalar, Stat, Tag,
    TimeTooLarge,
};

use crate::attack_data::{AttackData, SplashData, StreakData, WeaponValue};
use crate::bonus_stats::{BonusData, BonusStats};
use crate::error::CombatDataError;
use crate::weapon::{
    ClipRule, OuterRing, Pitch, PreAttackRule, Rate, Scatter, ScatterTag, Splash, Streak,
    StreakFall, StreakStage, Weapon, WeaponDamage,
};

/// What an attack's weapon resolves by: its action as the load holds it and as the book holds
/// it, the match's rate, and the names of the match's stats, damage kinds, tags and modifiers.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WeaponNames<'c> {
    pub(crate) loaded: LoadedAction<'c>,
    pub(crate) action: &'c Action,
    pub(crate) tick_rate: MatchRate,
    pub(crate) stats: Option<&'c StatBook>,
    pub(crate) kinds: Option<&'c Names<DamageKind>>,
    pub(crate) tags: &'c TagNames,
    pub(crate) modifiers: Option<&'c ModifierBook>,
}

impl WeaponNames<'_> {
    /// The weapon `data` gives, as its rules allow it.
    pub(crate) fn weapon(self, data: &AttackData) -> Result<Weapon, BookError> {
        self.rule(data)
            .map_err(|problem| self.loaded.broken(problem))?;
        let rate = match &data.rate {
            WeaponValue::Stat(stat) => Rate::Stat(self.stat(stat)?),
            WeaponValue::Own(periods) => Rate::Period(self.at_ranks(|rank| {
                let ms = self
                    .loaded
                    .data
                    .whole_at(rank, periods)
                    .filter(|&ms| ms > 0);
                self.ticks(ms.ok_or(CombatDataError::Period)?)
            })?),
        };
        let damage = match &data.damage {
            WeaponValue::Stat(stat) => WeaponDamage::Stat(self.stat(stat)?),
            WeaponValue::Own(amounts) => WeaponDamage::Amount(self.at_ranks(|rank| {
                let amount = self.loaded.data.value_at(rank, amounts);
                let amount = amount.and_then(Scalar::to_num);
                amount
                    .filter(|amount| *amount >= Num::ZERO)
                    .ok_or(CombatDataError::Amount)
            })?),
        };
        let kind = self
            .kinds
            .and_then(|kinds| kinds.named(data.damage_kind.as_str()));
        let kind = kind.ok_or_else(|| self.unknown(DamageKind::NAMES, &data.damage_kind))?;
        let clip = data.clip.as_ref().map(|clip| {
            self.at_ranks(|rank| {
                let whole = |ranked: &Ranked<Number>| self.loaded.data.whole_at(rank, ranked);
                let shots = whole(&clip.shots).and_then(|shots| u16::try_from(shots).ok());
                Ok(ClipRule {
                    shots: shots
                        .and_then(NonZeroU16::new)
                        .ok_or(CombatDataError::Clip)?,
                    reload: self.ticks(whole(&clip.reload_ms).ok_or(CombatDataError::Clip)?)?,
                    refill: clip.refill,
                })
            })
        });
        let scatter = data.scatter.as_ref().map(|scatter| {
            let tags = scatter.tags.iter().map(|(name, &radius)| {
                Ok(ScatterTag {
                    tag: self.tag(name)?,
                    radius,
                })
            });
            Ok::<_, BookError>(Scatter {
                radius: scatter.radius,
                tags: tags.collect::<Result<_, _>>()?,
            })
        });
        let bonus = |bonus: &BonusData| {
            let stats = bonus.stats().iter().map(|stat| self.stat(stat));
            stats.collect::<Result<BonusStats, BookError>>()
        };
        let rate_bonus = data.rate_bonus.as_ref().map(bonus);
        let range_bonus = data.range_bonus.as_ref().map(|bonus| {
            let stats = bonus.stats().iter().map(|stat| self.stat(stat));
            stats.collect::<Result<RangeBonus, BookError>>()
        });
        let damage_bonus = data.damage_bonus.as_ref().map(bonus);
        let streak = data.streak.as_ref().map(|streak| self.streak(streak));
        let pre_attack = data.pre_attack.as_ref().map(|pre_attack| {
            let ticks = self.at_ranks(|rank| {
                let ms = self.loaded.data.whole_at(rank, &pre_attack.ms);
                self.ticks(ms.ok_or(CombatDataError::PreAttack)?)
            })?;
            Ok::<_, BookError>(PreAttackRule {
                ticks,
                each: pre_attack.each,
            })
        });
        Ok(Weapon {
            rate,
            rate_bonus: rate_bonus.transpose()?,
            range_bonus: range_bonus.transpose()?,
            damage,
            damage_bonus: damage_bonus.transpose()?,
            kind,
            prefers: data
                .prefers
                .as_ref()
                .map(|prefers| Filter::resolve(prefers.filter(), self.tags))
                .transpose()
                .map_err(|name| self.loaded.unknown(name))?,
            clip: clip.transpose()?,
            splash: data
                .splash
                .as_ref()
                .map(|splash| self.splash(splash))
                .transpose()?,
            scatter: scatter.transpose()?,
            pitch: data.pitch_deg.map(|pitch| Pitch::of(pitch.low, pitch.high)),
            speed: data.speed,
            streak: streak.transpose()?,
            pre_attack: pre_attack.transpose()?,
            aim: data.aim_deg,
        })
    }

    /// The splash `data` gives, its outer ring's amount at each rank.
    fn splash(self, data: &SplashData) -> Result<Splash, BookError> {
        let outer = data.outer.as_ref().map(|outer| {
            let amounts = self.at_ranks(|rank| {
                let amount = self.loaded.data.value_at(rank, &outer.amount);
                let amount = amount.and_then(Scalar::to_num);
                amount
                    .filter(|amount| *amount >= Num::ZERO)
                    .ok_or(CombatDataError::OuterAmount)
            })?;
            Ok::<_, BookError>(OuterRing {
                radius: outer.radius,
                amounts,
            })
        });
        Ok(Splash {
            radius: data.radius,
            affects: data.affects,
            spares_source: data.spares_source,
            outer: outer.transpose()?,
        })
    }

    /// The streak `data` gives, its coast and its fall's hold in ticks, and each stage's modifier
    /// and its fall's one of its package's.
    fn streak(self, data: &StreakData) -> Result<Streak, BookError> {
        let broken = |problem| self.loaded.broken(problem);
        let coast = self.ticks(data.coast_ms).map_err(broken)?;
        let stages = data.stages.iter().map(|stage| {
            let names = self.modifiers.map(ModifierBook::names);
            let named =
                names.and_then(|names| names.named((self.loaded.package, stage.modifier.as_str())));
            Ok(StreakStage {
                shots: stage.shots,
                modifier: named.ok_or_else(|| self.unknown(ModifierId::NAMES, &stage.modifier))?,
            })
        });
        let fall = data.fall.as_ref().map(|fall| {
            let names = self.modifiers.map(ModifierBook::names);
            let named =
                names.and_then(|names| names.named((self.loaded.package, fall.modifier.as_str())));
            Ok(StreakFall {
                modifier: named.ok_or_else(|| self.unknown(ModifierId::NAMES, &fall.modifier))?,
                hold: self.ticks(fall.ms).map_err(broken)?,
            })
        });
        Ok(Streak {
            coast,
            stages: stages.collect::<Result<_, BookError>>()?,
            fall: fall.transpose()?,
        })
    }

    /// The rules of `data`'s fields: a weapon's delivery is a projectile launched at a unit, a
    /// splash goes off where a projectile of its delivery ends, a scatter aims a projectile of its
    /// delivery that flies to a point, a speed delays the hit of a weapon of no delivery, a rate
    /// bonus speeds a period of its own, a damage bonus and an outer ring go with an amount of its
    /// own, and a streak's stages are some, their shots strictly growing
    /// (04-capabilities/combat.md, An attack may splash; 04-capabilities/combat.md, An attack may
    /// scatter; 04-capabilities/combat.md, A slow weapon delays its damage;
    /// 04-capabilities/combat.md, A weapon's own period may speed up by a stat;
    /// 04-capabilities/combat.md, One bonus scales both rings;
    /// 04-capabilities/combat.md, A shot streak gives a stage's modifier).
    fn rule(self, data: &AttackData) -> Result<(), CombatDataError> {
        let delivery = self.action.delivery.as_ref();
        let aim = delivery.map(|delivery| delivery.aim);
        if delivery.is_some() && !aim.is_some_and(DeliveryAim::is_at_unit) {
            Err(CombatDataError::Delivery)
        } else if data.splash.is_some() && !aim.is_some_and(DeliveryAim::flies) {
            Err(CombatDataError::Splash)
        } else if data.scatter.is_some() && !aim.is_some_and(DeliveryAim::flies_to_point) {
            Err(CombatDataError::Scatter)
        } else if data.speed.is_some() && delivery.is_some() {
            Err(CombatDataError::Speed)
        } else if data.rate_bonus.is_some() && data.rate.own().is_none() {
            Err(CombatDataError::RateBonus)
        } else if data.damage_bonus.is_some() && data.damage.own().is_none() {
            Err(CombatDataError::DamageBonus)
        } else if data.damage.own().is_none()
            && data
                .splash
                .as_ref()
                .is_some_and(|splash| splash.outer.is_some())
        {
            Err(CombatDataError::OuterBesideStat)
        } else if data.streak.as_ref().is_some_and(|streak| {
            streak.stages.is_empty()
                || !streak
                    .stages
                    .is_sorted_by(|low, high| low.shots < high.shots)
        }) {
            Err(CombatDataError::Stages)
        } else {
            Ok(())
        }
    }

    /// The value `at` gives at each of its action's ranks, from 1 to its last; the first error
    /// it gives at a rank.
    fn at_ranks<T>(
        self,
        at: impl Fn(Rank) -> Result<T, CombatDataError>,
    ) -> Result<Box<[T]>, BookError> {
        let ranks = 1..=self.action.ranks.len();
        let ranks = ranks.map(|rank| Rank::new(u8::try_from(rank).expect("ranks fit u8")));
        ranks
            .map(|rank| at(rank.expect("a rank from 1")))
            .collect::<Result<_, _>>()
            .map_err(|problem| self.loaded.broken(problem))
    }

    /// `ms` in ticks at the match's rate.
    fn ticks(self, ms: u64) -> Result<Ticks, CombatDataError> {
        Ok(self.tick_rate.ticks(ms).ok_or(TimeTooLarge)?)
    }

    fn stat(self, stat: &Stat) -> Result<StatId, BookError> {
        let Some(stats) = self.stats else {
            return Err(self.unknown(StatId::NAMES, stat));
        };
        stats.named_in(self.loaded.package, self.loaded.place(), stat)
    }

    fn tag(self, name: &DeclaredName) -> Result<Tag, BookError> {
        let tag = self.tags.named(name.as_str());
        tag.ok_or_else(|| self.unknown(Tag::NAMES, name))
    }

    /// The error of `name`, a name of kind `of` that the match does not have.
    fn unknown(self, of: NameKind, name: &(impl ToString + ?Sized)) -> BookError {
        self.loaded.unknown(UnknownName::new(of, name))
    }
}
