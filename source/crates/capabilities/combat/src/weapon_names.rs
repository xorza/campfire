use std::num::NonZeroU16;

use campfire_actions::{Action, DeliveryShape, LoadedAction, ProjectileFlight};
use campfire_common::{DeclaredName, Ticks};
use campfire_declare::BookError;
use campfire_math::Num;
use campfire_script::{TagNames, UnknownName};
use campfire_sim::TickRate;
use campfire_stats::StatBook;
use campfire_units::StatId;
use campfire_values::{
    DamageKind, NameKind, Named, Names, Number, Rank, Ranked, Scalar, Stat, Tag, TimeTooLarge,
};

use crate::attack_data::{AttackData, WeaponValue};
use crate::error::CombatDataError;
use crate::weapon::{ClipRule, Pitch, Rate, Scatter, ScatterTag, Weapon, WeaponDamage};

/// What an attack's weapon resolves by: its action as the load holds it and as the book holds
/// it, the match's rate, and the names of the match's stats, damage kinds and tags.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WeaponNames<'c> {
    pub(crate) loaded: LoadedAction<'c>,
    pub(crate) action: &'c Action,
    pub(crate) rate: TickRate,
    pub(crate) stats: Option<&'c StatBook>,
    pub(crate) kinds: Option<&'c Names<DamageKind>>,
    pub(crate) tags: &'c TagNames,
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
        Ok(Weapon {
            rate,
            damage,
            kind,
            clip: clip.transpose()?,
            splash: data.splash,
            scatter: scatter.transpose()?,
            pitch: data.pitch_deg.map(|pitch| Pitch::of(pitch.low, pitch.high)),
            speed: data.speed,
        })
    }

    /// The rules of `data`'s fields that its action delivers by: a weapon's delivery is a homing
    /// projectile or a curve, a splash goes off where a projectile of its delivery ends, a
    /// scatter aims a curve of its delivery, and a speed delays the hit of a weapon of no
    /// delivery (04-capabilities/combat.md, An attack may splash; 04-capabilities/combat.md, An
    /// attack may scatter; 04-capabilities/combat.md, A slow weapon delays its damage).
    fn rule(self, data: &AttackData) -> Result<(), CombatDataError> {
        let delivery = self.action.delivery.as_ref();
        let flight = delivery.and_then(|delivery| match delivery.shape {
            DeliveryShape::Projectile { flight, .. } => Some(flight),
            DeliveryShape::Area => None,
        });
        if delivery.is_some() && !flight.is_some_and(ProjectileFlight::is_at_unit) {
            Err(CombatDataError::Delivery)
        } else if data.splash.is_some() && flight.is_none() {
            Err(CombatDataError::Splash)
        } else if data.scatter.is_some() && flight != Some(ProjectileFlight::Curve) {
            Err(CombatDataError::Scatter)
        } else if data.speed.is_some() && delivery.is_some() {
            Err(CombatDataError::Speed)
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
        Ok(self.rate.ticks(ms).ok_or(TimeTooLarge)?)
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
