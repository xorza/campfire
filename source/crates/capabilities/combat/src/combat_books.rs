use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use campfire_actions::{
    ActionKind, ActionRange, ActionsData, DamageKindBook, RangeField, SlotKindBook, Targeting,
};
use campfire_declare::{BookCx, BookError, BookPlace, ByType, NamesListError, Section};
use campfire_script::{NameKind, UnknownName};
use campfire_stats::{LifePool, StatBook, TypePoolsData, UnitStartError};
use campfire_units::PoolNames;
use campfire_values::{Names, NamesError, PackageIndex};

use crate::Combat;
use crate::armor::Armor;
use crate::combat_book::CombatBook;
use crate::combat_data::CombatData;
use crate::error::CombatDataError;
use crate::mode_combat_data::ModeCombatData;
use crate::on_death::OnDeath;
use crate::turret::Turret;
use crate::turret::turret_data::TurretData;

impl Combat {
    /// The rule of every package's attacks: each aims at a unit, at a range in meters at every
    /// rank.
    pub(crate) fn attacks_rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        for (package, source) in cx.packages() {
            let actions = source.content.get::<ActionsData>().into_iter();
            let attacks = actions.flat_map(|actions| &actions.0);
            let mut attacks = attacks.filter(|(_, action)| action.kind == ActionKind::Attack);
            let broken = attacks.find_map(|(id, action)| {
                let global = action.range.as_ref().is_some_and(|range| {
                    range
                        .values()
                        .contains(&RangeField::Range(ActionRange::Global))
                });
                let problem = if !matches!(action.targeting, Targeting::Unit(_)) {
                    CombatDataError::AttackAims
                } else if global {
                    CombatDataError::GlobalAttack
                } else {
                    return None;
                };
                Some((id, problem))
            });
            if let Some((id, problem)) = broken {
                let at = BookPlace::Entry {
                    table: ActionsData::KEY,
                    name: id.clone(),
                };
                return Err(BookError::rule(package, at, problem));
            }
        }
        Ok(())
    }

    /// The rule of the mode's `[combat]`: it names damage kinds and a life pool.
    pub(crate) fn rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let data = cx.mode_section::<ModeCombatData>();
        let problem = if data.is_none_or(|data| data.damage_kinds.is_empty()) {
            Some(CombatDataError::NoDamageKinds)
        } else if data.is_none_or(|data| data.life.is_none()) {
            Some(CombatDataError::NoLifePool)
        } else {
            None
        };
        problem.map_or(Ok(()), |problem| {
            let at = BookPlace::Section(ModeCombatData::KEY);
            Err(BookError::rule(PackageIndex::MODE, at, problem))
        })
    }

    /// The names of the mode's damage kinds; an error for a name twice, or past the most kinds a
    /// match holds.
    pub(crate) fn damage_kinds(cx: &BookCx<'_>) -> Result<DamageKindBook, BookError> {
        let mut names = Names::default();
        let data = cx.mode_section::<ModeCombatData>().into_iter();
        for name in data.flat_map(|data| &data.damage_kinds) {
            names.try_add(name.as_str()).map_err(|error| {
                let error = match error {
                    NamesError::Full => NamesListError::TooMany,
                    NamesError::Repeated => NamesListError::Repeated(name.clone()),
                };
                Self::mode_error(error)
            })?;
        }
        Ok(DamageKindBook(Arc::new(names)))
    }

    /// The mode's life pool, as its `[combat]` names it; an error for a pool the mode does not
    /// declare.
    pub(crate) fn life_pool(cx: &BookCx<'_>) -> Result<LifePool, BookError> {
        let Some(data) = cx.mode_section::<ModeCombatData>() else {
            return Ok(LifePool(None));
        };
        let pools = &cx.fact::<PoolNames>()?.0;
        let life = data.life_pool(pools).map_err(Self::unknown)?;
        Ok(LifePool(life))
    }

    /// The error of `name`, which the mode's `[combat]` names and the mode does not declare.
    const fn unknown(name: UnknownName) -> BookError {
        BookError::unknown(
            PackageIndex::MODE,
            BookPlace::Section(ModeCombatData::KEY),
            name,
        )
    }

    /// The error of `problem`, a rule the mode's `[combat]` breaks.
    fn mode_error<E: Error + Clone + PartialEq + Send + Sync + 'static>(problem: E) -> BookError {
        BookError::rule(
            PackageIndex::MODE,
            BookPlace::Section(ModeCombatData::KEY),
            problem,
        )
    }
}

impl CombatBook {
    /// The mode's `[combat]`, each stat it names resolved; an error for a stat the mode does not
    /// declare.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let none = ModeCombatData::default();
        let data = cx.mode_section::<ModeCombatData>().unwrap_or(&none);
        let stats = cx.book::<StatBook>()?;
        let at = || BookPlace::Section(ModeCombatData::KEY);
        Self::new(data, cx.rate(), |stat| {
            stats.named_in(PackageIndex::MODE, at(), stat)
        })
    }
}

impl Armor {
    /// The armor of each unit type that stands with one, of the mode's armors by name; an error
    /// for an armor or a damage kind the mode does not declare.
    pub fn book(cx: &BookCx<'_>) -> Result<ByType<Self>, BookError> {
        let none = ModeCombatData::default();
        let data = cx.mode_section::<ModeCombatData>().unwrap_or(&none);
        let kinds = cx.book_if::<DamageKindBook>()?;
        let no_kinds = Names::default();
        let kinds = kinds.map_or(&no_kinds, |kinds| &*kinds.0);
        let armors = data.armors.iter().map(|(name, shares)| {
            let armor = Self::of(shares, kinds).map_err(Combat::unknown)?;
            Ok((name, armor))
        });
        let armors = armors.collect::<Result<BTreeMap<_, _>, BookError>>()?;
        let mut book = ByType::default();
        for unit in cx.standing() {
            let Some(name) = unit
                .data
                .section::<CombatData>()
                .and_then(|combat| combat.armor.as_ref())
            else {
                continue;
            };
            let armor = armors.get(name).cloned().ok_or_else(|| {
                let at = BookPlace::UnitType(unit.place());
                BookError::unknown(unit.package, at, UnknownName::new(NameKind::Armor, name))
            })?;
            book.set(unit.unit_type, armor);
        }
        Ok(book)
    }
}

impl OnDeath {
    /// What each unit type that stands with a `combat` section does at its death, an avatar
    /// staying; an error for a type with `combat` and not the life pool, or the life pool and not
    /// `combat`: a unit that can die is one that combat kills.
    pub fn book(cx: &BookCx<'_>) -> Result<ByType<Self>, BookError> {
        let life = cx.fact_if::<LifePool>()?.and_then(|life| life.0);
        let pools = cx.fact_if::<PoolNames>()?;
        let mut book = ByType::default();
        for unit in cx.standing() {
            let lives = (unit.data.section::<TypePoolsData>()).is_some_and(|held| {
                held.0.iter().any(|name| {
                    let pool = pools.and_then(|pools| pools.0.named(name.as_str()));
                    pool.is_some() && pool == life
                })
            });
            let fail =
                |error| BookError::rule(unit.package, BookPlace::UnitType(unit.place()), error);
            let on_death = match (unit.data.section::<CombatData>(), lives) {
                (Some(_), true) if unit.avatar() => Self::Stay,
                (Some(combat), true) => combat.on_death,
                (Some(_), false) => return Err(fail(UnitStartError::NoLifePool)),
                (None, true) => return Err(fail(UnitStartError::NoCombat)),
                (None, false) => continue,
            };
            book.set(unit.unit_type, on_death);
        }
        Ok(book)
    }
}

impl Turret {
    /// The turret of each unit type that stands with one, its attacks those of its slot kind; an
    /// error for a kind the mode does not declare.
    pub fn book(cx: &BookCx<'_>) -> Result<ByType<Self>, BookError> {
        let kinds = &cx.book::<SlotKindBook>()?.0;
        let mut book = ByType::default();
        for unit in cx.standing() {
            let Some(turret) = unit.data.section::<TurretData>() else {
                continue;
            };
            let slot = kinds.names().named(turret.slot.as_str()).ok_or_else(|| {
                let at = BookPlace::UnitType(unit.place());
                let name = UnknownName::new(NameKind::SlotKind, &turret.slot);
                BookError::unknown(unit.package, at, name)
            })?;
            book.set(unit.unit_type, Self::of(turret, slot, cx.rate()));
        }
        Ok(book)
    }
}
