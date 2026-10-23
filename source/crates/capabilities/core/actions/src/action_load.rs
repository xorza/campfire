use campfire_common::DeclaredName;
use campfire_declarations::{BookCx, BookError, BookPlace, Fact, Section};
use campfire_script::{NameTables, TagNames, UnknownName};
use campfire_units::{ScriptBook, StatId, UnitTypes};
use campfire_values::{ActionId, Named, PackageIndex, Param};

use crate::action_book::{ActionBook, ActionBookBuilder, PackageAction};
use crate::action_data::ActionData;
use crate::action_names::ActionNames;
use crate::action_parts::ActionParts;
use crate::action_ranks::ActionRanks;
use crate::actions_data::ActionsData;
use crate::delivery::DeliveryAims;
use crate::effect_list_book::{EffectListBook, EffectListBuilder};
use crate::effect_names::EffectNames;
use crate::error::ActionError;

/// Every package's actions as a match loads them: the book, and each action's effect lists.
#[derive(Debug, Default)]
pub struct ActionLoad {
    pub book: ActionBook,
    pub effects: EffectListBook,
}

impl Fact for ActionLoad {}

/// An `ActionLoad` as the load fills it.
#[derive(Debug, Default)]
struct ActionLoading {
    book: ActionBookBuilder,
    effects: EffectListBuilder,
}

impl ActionLoad {
    /// Every package's actions, package by package, each by its id, of the ranks its slots give
    /// it, but a rules package's, which no match loads; an error for a name the match does not
    /// have, or an action that breaks a rule.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let names = LoadNames::of(cx)?;
        let ranks = cx.fact::<ActionRanks>()?;
        let mut load = ActionLoading::default();
        for (index, package) in cx.packages() {
            let actions = package.content.get::<ActionsData>().into_iter();
            for (id, data) in actions.flat_map(|actions| &actions.0) {
                let Some(ranks) = ranks.of(index, id.as_str()) else {
                    continue;
                };
                load.action(
                    cx,
                    &names,
                    Loaded {
                        index,
                        id,
                        data,
                        ranks,
                    },
                )?;
            }
        }
        Ok(Self {
            book: load.book.build(),
            effects: load.effects.build(),
        })
    }
}

impl ActionLoading {
    /// Loads `loaded`, with its script, its params and its effect lists.
    fn action(
        &mut self,
        cx: &BookCx<'_>,
        names: &LoadNames<'_>,
        loaded: Loaded<'_>,
    ) -> Result<ActionId, BookError> {
        let Loaded {
            index,
            id,
            data,
            ranks,
        } = loaded;
        let at = || BookPlace::Entry {
            table: ActionsData::KEY,
            name: id.clone(),
        };
        let script = data.keys.script.as_ref().map(|path| cx.script(index, path));
        let script = script.transpose()?;
        let parts = ActionParts::of(data, index, ranks, cx.rate(), &names.action());
        let parts = parts.map_err(|error| ActionLoad::error(index, id, error))?;
        let action = self.book.load(
            names.scripts,
            PackageAction {
                package: index,
                name: id.as_str(),
                data,
                script,
                parts,
            },
        );
        // Each stat a param scales by is the mode's: stats builds the params' table from them.
        let scaled = data.keys.params.values().filter_map(|param| match param {
            Param::Scaling(scaling) => Some(scaling.ratios.keys().chain(scaling.bonus.keys())),
            Param::Ranked(_) => None,
        });
        let stats = names.tables.table::<StatId>();
        let unknown = scaled
            .flatten()
            .find(|stat| stats.is_none_or(|stats| stats.named(stat.name()).is_none()));
        if let Some(stat) = unknown {
            return Err(BookError::unknown(
                index,
                at(),
                UnknownName::new(StatId::NAMES, stat),
            ));
        }
        if !(data.on_resolve.is_empty() && data.on_hit.is_empty() && data.on_end.is_empty()) {
            let pushed = self.effects.push(action, data, names.effects(data, index));
            pushed.map_err(|error| ActionLoad::error(index, id, error))?;
        }
        Ok(action)
    }
}

impl ActionLoad {
    /// The book error of `error`, which the data of `action` of the package at `package` gives.
    fn error(package: PackageIndex, action: &DeclaredName, error: ActionError) -> BookError {
        let at = BookPlace::Entry {
            table: ActionsData::KEY,
            name: action.clone(),
        };
        match error {
            ActionError::Problem(problem) => BookError::rule(package, at, problem),
            ActionError::Unknown(name) => BookError::unknown(package, at, name),
        }
    }

    /// Every package's actions, as their load holds them.
    pub fn book(cx: &BookCx<'_>) -> Result<ActionBook, BookError> {
        Ok(cx.fact::<Self>()?.book.clone())
    }

    /// Every action's effect lists, as their load holds them.
    pub fn effects(cx: &BookCx<'_>) -> Result<EffectListBook, BookError> {
        Ok(cx.fact::<Self>()?.effects.clone())
    }
}

/// An action to load: its package's place, its id, its data, and its ranks.
#[derive(Debug, Clone, Copy)]
struct Loaded<'d> {
    index: PackageIndex,
    id: &'d DeclaredName,
    data: &'d ActionData,
    ranks: u8,
}

/// The tables an action's names resolve by: the match's names of every kind, its unit types'
/// tags, and how its delivery types aim.
#[derive(Debug)]
struct LoadNames<'c> {
    scripts: &'c ScriptBook,
    tables: NameTables,
    tags: &'c TagNames,
    aims: Option<&'c DeliveryAims>,
}

impl<'c> LoadNames<'c> {
    fn of(cx: &'c BookCx<'c>) -> Result<Self, BookError> {
        Ok(Self {
            scripts: cx.book::<ScriptBook>()?,
            tables: cx.name_tables()?,
            tags: cx.fact::<UnitTypes>()?.tags(),
            aims: cx.fact_if::<DeliveryAims>()?,
        })
    }

    /// The names of an action's data.
    const fn action(&self) -> ActionNames<'_> {
        ActionNames::new(&self.tables, self.tags, self.aims)
    }

    /// The names of the lists of `action`, of the package at `package`.
    const fn effects<'n>(
        &'n self,
        action: &'n ActionData,
        package: PackageIndex,
    ) -> EffectNames<'n> {
        EffectNames::new(&self.tables, &action.keys.params, package)
    }
}
