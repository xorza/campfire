use std::collections::{BTreeMap, BTreeSet};

use campfire_common::DeclaredName;
use campfire_declare::{BookCx, BookError, BookPlace, Fact, NamedScripts, ScriptParams, Section};
use campfire_math::Num;
use campfire_script::{NameKind, ParamProblem, ScriptRole, ScriptUses, UnknownName};
use campfire_sim::TickRate;
use campfire_stats::{EdgeError, ModifierData, ModifiersData, StatGraph};
use campfire_values::{PackageIndex, Param};

use crate::action_data::ActionData;
use crate::action_ranks::ActionRanks;
use crate::actions_data::ActionsData;
use crate::error::{AppliedError, Way};
use crate::modifier_appliers::{ModifierAppliers, Pass};

/// The ways each package's modifiers are applied, package by package: those `ModifierAppliers`
/// gives, each passed on until nothing changes.
#[derive(Debug, Default)]
pub struct ModifierWays(Vec<BTreeMap<String, BTreeSet<Way>>>);

impl Fact for ModifierWays {}

/// A param a modifier reads: its name, whether a number of the modifier reads it or only its
/// script, and the fastest rate the mode allows, at which a time counts the most ticks.
#[derive(Debug, Clone, Copy)]
struct ParamRead<'a> {
    modifier: &'a ModifierData,
    param: &'a DeclaredName,
    number: bool,
    rate: TickRate,
}

impl ModifierWays {
    /// The ways of every package's modifiers, of what applies them.
    pub(crate) fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let appliers = cx.fact::<ModifierAppliers>()?;
        let mut ways = Self(cx.packages().map(|_| BTreeMap::new()).collect());
        for applied in &appliers.applied {
            let package = &mut ways.0[usize::from(applied.package)];
            let held = package.entry(applied.modifier.clone()).or_default();
            held.insert(applied.way.clone());
        }
        ways.pass_on(&appliers.passes);
        Ok(ways)
    }

    /// The ways of modifier `id` of the package at `package`; none for one nothing applies.
    pub(crate) fn of(&self, package: PackageIndex, id: &str) -> impl Iterator<Item = &Way> {
        self.0[usize::from(package)].get(id).into_iter().flatten()
    }

    /// The actions of `actions`, the package's at `package`, that apply its modifier `id`, each
    /// by its id.
    pub(crate) fn actions_of<'m>(
        &self,
        package: PackageIndex,
        id: &str,
        actions: &'m BTreeMap<DeclaredName, ActionData>,
    ) -> impl Iterator<Item = (&'m DeclaredName, &'m ActionData)> {
        self.of(package, id).filter_map(|way| match way {
            Way::Action(action) => actions.get_key_value(action),
            Way::NoAction => None,
        })
    }

    /// Adds the script of each package's modifier, which serves the modifier and reads its own
    /// params and those the actions that apply it give.
    pub(crate) fn named_scripts(
        cx: &BookCx<'_>,
        scripts: &mut NamedScripts,
    ) -> Result<(), BookError> {
        let ways = cx.fact::<Self>()?;
        let no_actions = BTreeMap::new();
        for (package, source) in cx.packages() {
            let Some(modifiers) = source.content.get::<ModifiersData>() else {
                continue;
            };
            let actions = source.content.get::<ActionsData>();
            let actions = actions.map_or(&no_actions, |actions| &actions.0);
            for (id, modifier) in &modifiers.0 {
                let Some(script) = &modifier.script else {
                    continue;
                };
                let by = ways.actions_of(package, id.as_str(), actions);
                let given = by.flat_map(|(_, action)| action.params.keys());
                let readable: BTreeSet<&DeclaredName> =
                    modifier.params.keys().chain(given).collect();
                let params = ScriptParams::Named(readable.into_iter().cloned().collect());
                scripts.name(package, script, ScriptRole::Modifier, params);
            }
        }
        Ok(())
    }

    /// The rule of every package's modifiers as their ways apply them: each param a modifier
    /// reads, by its numbers or its script, holds as each way gives it, at each rank of the way's
    /// action; and an aura radius or a shield holds no value below zero.
    pub(crate) fn rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let ways = cx.fact::<Self>()?;
        let ranks = cx.fact::<ActionRanks>()?;
        let uses = cx.given::<ScriptUses>();
        let uses = uses.expect("the load gives the scripts' uses");
        let no_actions = BTreeMap::new();
        for (package, source) in cx.packages() {
            let Some(modifiers) = source.content.get::<ModifiersData>() else {
                continue;
            };
            let actions = source.content.get::<ActionsData>();
            let actions = actions.map_or(&no_actions, |actions| &actions.0);
            for (id, modifier) in &modifiers.0 {
                let at = || BookPlace::Entry {
                    table: ModifiersData::KEY,
                    name: id.clone(),
                };
                let fail = |problem| BookError::rule(package, at(), problem);
                let by: Vec<&ActionData> = ways
                    .actions_of(package, id.as_str(), actions)
                    .map(|(_, action)| action)
                    .collect();
                let held: Vec<&Way> = ways.of(package, id.as_str()).collect();
                let scripted = match &modifier.script {
                    Some(path) => uses.of(cx.script(package, path)?).params.as_slice(),
                    None => &[],
                };
                let scripted = scripted.iter().filter_map(|name| DeclaredName::new(name));
                let reads: BTreeSet<DeclaredName> =
                    modifier.param_refs().cloned().chain(scripted).collect();
                for param in &reads {
                    let number = modifier.param_refs().any(|read| read == param);
                    // A param nothing declares is unknown, not a way's to give: where the
                    // modifier's numbers read it, here; where its script does, as the script's
                    // checks find it.
                    let declared = modifier.params.contains_key(param)
                        || by.iter().any(|action| action.params.contains_key(param));
                    if !declared {
                        if number {
                            let name = UnknownName::new(NameKind::Param, param);
                            return Err(BookError::unknown(package, at(), name));
                        }
                        continue;
                    }
                    let read = ParamRead {
                        modifier,
                        param,
                        number,
                        rate: cx.rate(),
                    };
                    read.check(&held, actions).map_err(|(way, problem)| {
                        let param = param.clone();
                        let way = way.cloned();
                        fail(AppliedError::Param {
                            param,
                            way,
                            problem,
                        })
                    })?;
                }
                for (param, own) in &modifier.params {
                    let short = own.ranks().and_then(|count| {
                        held.iter().copied().find(|way| match way {
                            Way::Action(action) => ranks
                                .of(package, action.as_str())
                                .is_some_and(|ranks| count < usize::from(ranks)),
                            Way::NoAction => false,
                        })
                    });
                    if let Some(way) = short {
                        return Err(fail(AppliedError::Param {
                            param: param.clone(),
                            way: Some(way.clone()),
                            problem: ParamProblem::Short,
                        }));
                    }
                }
                let appliers = by.iter().map(|action| &action.params);
                modifier
                    .check_signs(appliers)
                    .map_err(|problem| fail(AppliedError::Sign(problem)))?;
            }
        }
        Ok(())
    }

    /// Adds to `graph` the edges of each live stat change that reads a param its modifier does
    /// not declare, of each action that applies the modifier and gives the param as a scaling
    /// table; an error for a stat the mode does not have, at the action whose table reads it or
    /// at the modifier that changes it.
    pub(crate) fn stat_edges(cx: &BookCx<'_>, graph: &mut StatGraph) -> Result<(), BookError> {
        let ways = cx.fact::<Self>()?;
        let no_actions = BTreeMap::new();
        for (package, source) in cx.packages() {
            let Some(modifiers) = source.content.get::<ModifiersData>() else {
                continue;
            };
            let actions = source.content.get::<ActionsData>();
            let actions = actions.map_or(&no_actions, |actions| &actions.0);
            for (id, modifier) in &modifiers.0 {
                for (changed, change) in &modifier.stats {
                    let given = change.value.param();
                    let Some(name) = given.filter(|name| !modifier.params.contains_key(*name))
                    else {
                        continue;
                    };
                    for (action, data) in ways.actions_of(package, id.as_str(), actions) {
                        let reads = data.params.get(name).into_iter().flat_map(Param::stats);
                        for read in reads {
                            graph.add(read, changed).map_err(|error| {
                                let (table, name, unknown) = match error {
                                    EdgeError::Read(unknown) => (ActionsData::KEY, action, unknown),
                                    EdgeError::Changed(unknown) => {
                                        (ModifiersData::KEY, id, unknown)
                                    }
                                };
                                let name = name.clone();
                                BookError::unknown(
                                    package,
                                    BookPlace::Entry { table, name },
                                    unknown,
                                )
                            })?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Gives each `to` of `passes` every way of its `from`, until no pass adds one.
    fn pass_on(&mut self, passes: &[Pass]) {
        let mut given = Vec::new();
        loop {
            let mut added = false;
            for pass in passes {
                let package = &mut self.0[usize::from(pass.package)];
                given.clear();
                given.extend(
                    package
                        .get(pass.from.as_str())
                        .into_iter()
                        .flatten()
                        .cloned(),
                );
                let ways = package.entry(pass.to.clone()).or_default();
                #[expect(
                    clippy::iter_with_drain,
                    reason = "`given` is the pass's buffer, kept for the next `from`"
                )]
                for way in given.drain(..) {
                    added |= ways.insert(way);
                }
            }
            if !added {
                return;
            }
        }
    }
}

impl ParamRead<'_> {
    /// Whether the param holds as each of `ways` gives it: the modifier's own, whatever applies
    /// it, or else each way's action's, of `actions`, which no way with no action has. What a
    /// number reads holds a number at every rank, and where a time reads it, no scaling param
    /// and a time that counts in ticks; a failure is of the way, none for the modifier's own
    /// param.
    fn check<'w>(
        self,
        ways: &[&'w Way],
        actions: &BTreeMap<DeclaredName, ActionData>,
    ) -> Result<(), (Option<&'w Way>, ParamProblem)> {
        if let Some(own) = self.modifier.params.get(self.param) {
            return self.holds(own).map_err(|problem| (None, problem));
        }
        for &way in ways {
            let given = match way {
                Way::Action(action) => actions
                    .get(action)
                    .and_then(|action| action.params.get(self.param)),
                Way::NoAction => None,
            };
            let given = given.ok_or((Some(way), ParamProblem::Missing))?;
            self.holds(given).map_err(|problem| (Some(way), problem))?;
        }
        Ok(())
    }

    fn holds(self, param: &Param) -> Result<(), ParamProblem> {
        if !self.number {
            return Ok(());
        }
        let time = (self.modifier.times()).any(|number| number.param() == Some(self.param));
        let numbers: Option<Vec<Num>> = param.numbers().collect();
        let numbers = numbers.ok_or(ParamProblem::Overflow)?;
        if time && matches!(param, Param::Scaling(_)) {
            return Err(ParamProblem::ScalingTime);
        }
        let counts = |&ms: &Num| ModifierData::ticks(ms, self.rate).is_some();
        if time && !numbers.iter().all(counts) {
            return Err(ParamProblem::Time);
        }
        Ok(())
    }
}
