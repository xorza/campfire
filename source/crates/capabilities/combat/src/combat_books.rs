use std::error::Error;
use std::sync::Arc;

use campfire_actions::{ActionBook, ActionKind, ActionsData, LoadedAction, Targeting};
use campfire_declarations::{
    BookCx, BookError, BookPlace, ById, ByIdBuilder, NamesListError, Section,
};
use campfire_script::UnknownName;
use campfire_stats::{LifePool, ModifierBook, StatBook, TypePoolsData, UnitStartError};
use campfire_units::{PoolId, UnitTypes};
use campfire_values::{ActionId, DamageKind, Names, NamesError, PackageIndex, UnitType};

use crate::Combat;
use crate::attack_data::AttackData;
use crate::combat_data::CombatData;
use crate::error::CombatDataError;
use crate::mode_combat_data::ModeCombatData;
use crate::on_death::OnDeath;
use crate::weapon::Weapon;
use crate::weapon_names::WeaponNames;

impl Combat {
    /// The rule of every package's attacks: each aims at a unit.
    pub(crate) fn attacks_rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        for (package, source) in cx.packages() {
            let actions = source.content.get::<ActionsData>().into_iter();
            let attacks = actions.flat_map(|actions| &actions.0);
            let mut attacks = attacks.filter(|(_, action)| action.kind() == ActionKind::Attack);
            let broken =
                attacks.find(|(_, action)| !matches!(action.keys.targeting, Targeting::Unit(_)));
            if let Some((id, _)) = broken {
                let problem = CombatDataError::AttackAims;
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
    pub(crate) fn damage_kinds(cx: &BookCx<'_>) -> Result<Arc<Names<DamageKind>>, BookError> {
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
        Ok(Arc::new(names))
    }

    /// The mode's life pool, as its `[combat]` names it; an error for a pool the mode does not
    /// declare.
    pub(crate) fn life_pool(cx: &BookCx<'_>) -> Result<LifePool, BookError> {
        let Some(data) = cx.mode_section::<ModeCombatData>() else {
            return Ok(LifePool(None));
        };
        let pools = cx.names::<PoolId>()?;
        let life = data.life_pool(pools).map_err(Self::unknown)?;
        Ok(LifePool(life))
    }

    /// The error of `name`, which the mode's `[combat]` names and the mode does not declare.
    pub(crate) const fn unknown(name: UnknownName) -> BookError {
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

impl Weapon {
    /// The weapon of each attack, its names resolved and its own values at each of its ranks, in
    /// ticks at the match's rate; an error for a name the match does not have, a value that does
    /// not hold at a rank, or a field its delivery does not serve.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<ActionId, Self>, BookError> {
        let actions = cx.book::<ActionBook>()?;
        let stats = cx.book_if::<StatBook>()?;
        let kinds = cx.names_if::<DamageKind>()?.map(|kinds| &**kinds);
        let tags = cx.fact::<UnitTypes>()?.tags();
        let modifiers = cx.book_if::<ModifierBook>()?;
        let mut book = ByIdBuilder::default();
        for loaded in LoadedAction::all(cx)? {
            let Some(data) = loaded.section::<AttackData>() else {
                continue;
            };
            let names = WeaponNames {
                loaded,
                action: actions
                    .get(loaded.action)
                    .expect("a loaded action is in the book"),
                tick_rate: cx.rate(),
                stats,
                kinds,
                tags,
                modifiers,
            };
            book.set(loaded.action, names.weapon(data)?);
        }
        Ok(book.build())
    }
}

impl OnDeath {
    /// What each unit type that stands with a `combat` section does at its death, an avatar
    /// staying; an error for a type with `combat` and not the life pool, or the life pool and not
    /// `combat`: a unit that can die is one that combat kills.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let life = cx.fact_if::<LifePool>()?.and_then(|life| life.0);
        let pools = cx.names_if::<PoolId>()?;
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let lives = (unit.data.section::<TypePoolsData>()).is_some_and(|held| {
                held.0.iter().any(|name| {
                    let pool = pools.and_then(|pools| pools.named(name.as_str()));
                    pool.is_some() && pool == life
                })
            });
            let fail =
                |error| BookError::rule(unit.package, BookPlace::UnitType(unit.place()), error);
            let on_death = match (unit.data.section::<CombatData>(), lives) {
                (Some(_), true) if unit.is_avatar() => Self::Stay,
                (Some(combat), true) => combat.on_death,
                (Some(_), false) => return Err(fail(UnitStartError::NoLifePool)),
                (None, true) => return Err(fail(UnitStartError::NoCombat)),
                (None, false) => continue,
            };
            book.set(unit.unit_type, on_death);
        }
        Ok(book.build())
    }
}
