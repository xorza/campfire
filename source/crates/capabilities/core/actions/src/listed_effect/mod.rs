use std::any::Any;
use std::error::Error;
use std::fmt::Debug;
use std::sync::Arc;

use campfire_common::DeclaredName;
use campfire_declare::{BookError, BookPlace, ListedSet, UnitTypeData};
use campfire_script::{DataTable, NameKind};
use campfire_sim::Capability;
use campfire_values::{Number, PackageIndex};

use crate::amount::Amount;
use crate::effect_names::EffectNames;
use crate::launch_id::LaunchId;
use crate::list_hook::ListHook;
use crate::listed_effect::effect_data::LaunchLists;
use crate::listed_effect::error::{EffectError, EffectRule};
use crate::listed_effect::listed_spec::ListedSpec;

pub(crate) mod effect_data;
pub(crate) mod error;
pub(crate) mod listed_spec;

/// An effect of an action's list, as its data gives it, the type of the module that registers its
/// key: what the core reads of it to check its action, and its resolve. No type of the core lists
/// the effects (04-capabilities/actions.md, Each capability registers its effects).
pub trait ListedEffect: EffectEq + Any + Debug + Send + Sync {
    /// The capability the mode declares for the effect to run; none for an effect of the core.
    fn needs(&self) -> Option<Capability>;

    /// The numbers it gives, for the checks of its action's params.
    fn numbers(&self) -> Vec<&Number>;

    /// Its number that counts milliseconds, which the load holds whole.
    fn duration(&self) -> Option<&Number>;

    /// The modifier of its package it applies, if it applies one.
    fn modifier(&self) -> Option<&DeclaredName>;

    /// The lists it holds, as a launch holds its own.
    fn lists(&self) -> Option<&LaunchLists>;

    /// Whether it applies at the point its list reached, in a list that reaches no unit.
    fn takes_point(&self) -> bool;

    /// Its own rules, as `cx` reads its action, the rules of the lists it holds among them.
    fn rule(&self, cx: &dyn EffectRuleCx) -> Result<(), BookError>;

    /// What it does, its names resolved as `cx` resolves them.
    fn resolve(&self, cx: &mut dyn EffectResolveCx) -> Result<Arc<dyn ListedSpec>, EffectError>;
}

/// Equality of two effects of the set, whatever their types: the same type, and equal.
pub trait EffectEq {
    fn eq_effect(&self, other: &dyn Any) -> bool;
}

impl<T: Eq + 'static> EffectEq for T {
    fn eq_effect(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<T>() == Some(self)
    }
}

impl PartialEq for dyn ListedEffect {
    fn eq(&self, other: &Self) -> bool {
        self.eq_effect(other)
    }
}

impl Eq for dyn ListedEffect {}

/// The open set of the effects of actions' lists.
#[derive(Debug)]
pub struct ListedEffects;

impl ListedSet for ListedEffects {
    type Value = dyn ListedEffect;

    const TABLE: DataTable = DataTable::Effect;
}

/// What the rules of a listed effect read of its action and its list, which the rules of the
/// actions give.
pub trait EffectRuleCx {
    /// Whether the effect's list reaches a unit.
    fn reaches(&self) -> bool;

    /// The unit type `name` of the action's package, if it has one.
    fn package_type(&self, name: &DeclaredName) -> Option<&UnitTypeData>;

    /// Whether `tag` is one only the engine gives.
    fn engine_tag(&self, tag: &DeclaredName) -> bool;

    /// The error of `rule`, which the effect breaks in its list.
    fn fail(&self, rule: EffectRule) -> BookError;

    /// The error of `name`, a name of kind `of` the match does not have.
    fn unknown(&self, of: NameKind, name: &DeclaredName) -> BookError;

    /// The place of the effect's action, for a rule of the effect's module.
    fn at(&self) -> RuleAt;

    /// The rules of `lists`, which the effect holds: its `on_hit` reaches a unit, and its
    /// `on_end` does not.
    fn lists(&self, lists: &LaunchLists) -> Result<(), BookError>;
}

/// The place of an action whose effect breaks a rule: its package and its entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleAt {
    pub(crate) package: PackageIndex,
    pub(crate) place: BookPlace,
}

impl RuleAt {
    /// The error of `problem`, a rule of the effect's module that the action breaks.
    pub fn broken<E: Error + Clone + PartialEq + Send + Sync + 'static>(
        self,
        problem: E,
    ) -> BookError {
        BookError::rule(self.package, self.place, problem)
    }
}

/// What the resolve of a listed effect reads: the names of its match, its list, and the book
/// its action's lists join, which the lists it holds join first.
pub trait EffectResolveCx {
    /// The match's ids of the names the effect gives.
    fn names(&self) -> &dyn EffectNames;

    /// The list the effect is of.
    fn list(&self) -> ListHook;

    /// The launch of `lists`, which join the book, each resolved as its action's lists are.
    fn launch(&mut self, lists: &LaunchLists) -> Result<LaunchId, EffectError>;
}

impl dyn EffectResolveCx + '_ {
    /// `number`, its param resolved.
    pub fn amount(&self, number: &Number) -> Result<Amount, EffectError> {
        Ok(Amount::of(number, self.names())?)
    }

    /// `number`, when the effect gives it, its param resolved.
    pub fn amount_if(&self, number: Option<&Number>) -> Result<Option<Amount>, EffectError> {
        number.map(|number| self.amount(number)).transpose()
    }

    /// The error of `rule`, which the effect breaks in its list.
    pub fn broken(&self, rule: EffectRule) -> EffectError {
        EffectError::Rule {
            list: self.list(),
            rule,
        }
    }
}
