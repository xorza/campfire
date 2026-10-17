use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_common::DeclaredName;
use campfire_declare::{Book, BookCx, BookError, BookPlace, Section};
use campfire_math::Num;
use campfire_script::{
    HookSet, MatchNames, NameKind, ParamProblem, ScriptId, ScriptRole, StateValue, TagNames,
    UnknownName,
};
use campfire_sim::TickRate;
use campfire_units::{ScriptBook, StatId, StatusTags, UnitTypes};
use campfire_values::{ActionId, ModifierId, NameId, Names, PackageIndex, Rank, Stat, TagSet};

use crate::application::{Adding, Application, ApplyAt, NewInstance};
use crate::applier_param::ApplierParam;
use crate::error::{ModifierError, SpecError};
use crate::instance::{Instance, StackEnd, StatShare};
use crate::interval::Interval;
use crate::interval_revival::IntervalRevival;
use crate::lifetime::{Ends, Hold, Lifetime};
use crate::modifier_data::{ModifierData, Reapply};
use crate::modifier_spec::{ModifierSpec, ParamPlace, SpecNames, SpecNumber, SpecTime};
use crate::modifiers_data::ModifiersData;
use crate::param_book::ParamBook;
use crate::param_read::ParamRead;
use crate::stat_book::StatBook;

/// The modifiers a match loaded, of every package: the mode, package 0, and each package it
/// depends on, in the order of its manifest. Package data, not state: a restore loads it from the
/// packages, as a new match does. Ids follow the order of package, then name. A clone shares
/// the entries, as the script view reads them.
#[derive(Resource, Debug, Clone, Default)]
pub struct ModifierBook {
    #[expect(
        clippy::rc_buffer,
        reason = "the load grows the book through `Arc::make_mut`; the clones after it share it"
    )]
    entries: Arc<Vec<ModifierEntry>>,
    /// Each modifier's name in its package.
    names: Arc<Names<ModifierId>>,
    /// The names of the params its modifiers read from the abilities that apply them.
    appliers: Arc<Names<ApplierParam>>,
}

/// A loaded modifier: its package, its spec, and its compiled script with the combat events it
/// hears.
#[derive(Debug, Clone)]
pub struct ModifierEntry {
    pub package: PackageIndex,
    pub(crate) spec: ModifierSpec,
    pub script: Option<ScriptId>,
    pub hooks: HookSet,
    /// The tags it grants its carrier.
    pub tags: TagSet,
}

/// What a package's modifiers load with: the hooks each script defines, the match's tags, which
/// name the tags they grant and resolve their filters, each stat's place, and the tick rate.
#[derive(Debug)]
pub struct ModifierLoad<'a, S> {
    pub scripts: &'a ScriptBook,
    pub tags: &'a TagNames,
    pub stat: S,
    pub rate: TickRate,
}

/// A modifier of a package to load: its name, its data, and its compiled script exactly when its
/// data names one.
#[derive(Debug, Clone, Copy)]
pub struct PackageModifier<'d> {
    pub name: &'d DeclaredName,
    pub data: &'d ModifierData,
    pub script: Option<ScriptId>,
}

impl ModifierBook {
    /// Loads `modifiers`, sorted by name, as those of `package`, after every modifier of an
    /// earlier package or name, their names resolved through `load`; an aura names a modifier
    /// of its own package. A modifier that does not load fails them all.
    pub fn load<S: Fn(&Stat) -> Result<StatId, UnknownName>>(
        &mut self,
        load: ModifierLoad<'_, S>,
        package: PackageIndex,
        modifiers: &[PackageModifier<'_>],
    ) -> Result<(), ModifierError> {
        let ModifierLoad {
            scripts,
            tags,
            stat,
            rate,
        } = load;
        let start = self.entries.len();
        assert!(
            modifiers.is_sorted_by(|a, b| a.name < b.name),
            "a package's modifiers load by name"
        );
        let last = start.checked_sub(1).map(ModifierId::at);
        if let (Some(last), Some(first)) = (last, modifiers.first()) {
            let held = (self.names.scope(last), self.names.name(last));
            assert!(
                held < (package, Some(first.name.as_str())),
                "modifiers load by package, then name"
            );
        }
        let granted: Vec<TagSet> = modifiers
            .iter()
            .map(|modifier| {
                let declared = |name: &DeclaredName| {
                    let tag = tags.named(name.as_str());
                    tag.expect("a match declares each tag its packages name before their modifiers")
                };
                modifier.data.tags.iter().map(declared).collect()
            })
            .collect();
        let names = SpecNames {
            stat,
            tags,
            rate,
            modifier: |name: &str| {
                let at = modifiers.binary_search_by(|modifier| modifier.name.as_str().cmp(name));
                match at {
                    Ok(at) => Ok(ModifierId::at(start + at)),
                    Err(_) => self
                        .names
                        .named((package, name))
                        .ok_or_else(|| UnknownName::new(NameKind::Modifier, name)),
                }
            },
        };
        let appliers = Arc::make_mut(&mut self.appliers);
        let specs = modifiers
            .iter()
            .map(|modifier| {
                ModifierSpec::of(modifier.data, &names, appliers).map_err(|error| ModifierError {
                    modifier: modifier.name.clone(),
                    error,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let entries = modifiers.iter().zip(specs).zip(granted);
        let book = Arc::make_mut(&mut self.entries);
        let names = Arc::make_mut(&mut self.names);
        for ((modifier, spec), tags) in entries {
            names.add((package, modifier.name.as_str()));
            book.push(ModifierEntry {
                package,
                spec,
                script: modifier.script,
                hooks: scripts.defines(modifier.script, ScriptRole::Modifier),
                tags,
            });
        }
        Ok(())
    }

    /// Whether `other` is this book as one load made it, not a book of another load.
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.entries, &other.entries)
            && Arc::ptr_eq(&self.names, &other.names)
            && Arc::ptr_eq(&self.appliers, &other.appliers)
    }

    /// Each modifier's name in its package.
    pub const fn names(&self) -> &Arc<Names<ModifierId>> {
        &self.names
    }

    /// The names of the params its modifiers read from the abilities that apply them.
    pub fn appliers(&self) -> &Names<ApplierParam> {
        &self.appliers
    }

    /// The modifier `id`, when the book holds it.
    pub(crate) fn entry(&self, id: ModifierId) -> Option<&ModifierEntry> {
        self.entries.get(id.index())
    }

    /// The tags modifier `id` grants its carrier.
    pub fn tags(&self, id: ModifierId) -> TagSet {
        self.get(id).tags
    }

    /// Whether a modifier has an interval.
    pub(crate) fn has_intervals(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.spec.interval.is_some())
    }

    /// What the interval of `id` does as its carrier comes back to life.
    pub(crate) fn revival(&self, id: ModifierId) -> IntervalRevival {
        self.get(id).spec.revival
    }

    /// The tag sets a unit's tags derive from beside its type's: those of each modifier of
    /// `stacked`, the modifiers of its instances with a stack, then `status`, which a capability
    /// gives it.
    pub fn granted<'a>(
        &'a self,
        stacked: impl Iterator<Item = ModifierId> + Clone + 'a,
        status: Option<StatusTags>,
    ) -> impl Iterator<Item = TagSet> + Clone + 'a {
        stacked
            .map(|id| self.tags(id))
            .chain(status.map(StatusTags::get))
    }

    pub fn get(&self, id: ModifierId) -> &ModifierEntry {
        &self.entries[id.index()]
    }

    /// Whether `action` at `rank`, or no ability at rank 1, applies `id` with every param its
    /// numbers read: each of its own params has a value at `rank`, and `action` gives each it
    /// does not declare, as a time in ticks at `rate` where a time reads it. The load checks the
    /// ways its packages name; a call checks the way of a name it computes.
    pub fn check_way(
        &self,
        id: ModifierId,
        action: Option<ActionId>,
        rank: Rank,
        params: &ParamBook,
        rate: TickRate,
    ) -> Result<(), ParamProblem> {
        if !params.modifiers().holds_rank(id.index(), rank) {
            return Err(ParamProblem::Short);
        }
        let spec = &self.get(id).spec;
        for param in spec.number_params().filter_map(ParamPlace::applier) {
            params.gives(action, rank, param, None)?;
        }
        for param in spec.time_params().filter_map(ParamPlace::applier) {
            params.gives(action, rank, param, Some(rate))?;
        }
        Ok(())
    }

    /// Whether `id` and `action`, when one, are a modifier and an action the books hold, and
    /// `action` at `rank`, or no ability, applies `id` as `check_way` asks at `rate`.
    pub(crate) fn has_way(
        &self,
        id: ModifierId,
        action: Option<ActionId>,
        rank: Rank,
        params: &ParamBook,
        rate: TickRate,
    ) -> bool {
        self.entry(id).is_some()
            && action.is_none_or(|action| params.has_action(action))
            && self.check_way(id, action, rank, params, rate).is_ok()
    }

    /// `adding` as applied `at`, by its applier's ability at its rank or by none, a way
    /// checked: its numbers resolved from its own params, then the ability's, of its source as
    /// it is now, its times in ticks at `at`'s rate; a stat change that reads a scaling table
    /// keeps reading it, live. Its duration is the one a call names, when it names one. A
    /// passive or an aura holds while its ability or carrier keeps it, so it has no duration,
    /// and a passive applied again at another rank refreshes; a passive whose stacks end one by
    /// one counts them from none.
    pub(crate) fn application<'a>(
        &'a self,
        at: ApplyAt<'a>,
        adding: Adding<'a>,
    ) -> Application<
        impl ExactSizeIterator<Item = StatShare> + 'a,
        impl ExactSizeIterator<Item = StateValue> + 'a,
    > {
        let Adding {
            id,
            applier: from,
            source,
            duration,
        } = adding;
        let ApplyAt { now, rate, params } = at;
        let param = move |place: ParamPlace| {
            params.modifier_param(id, from.action, from.rank, place, source)
        };
        let entry = self.get(id);
        let spec = &entry.spec;
        let read = move |number: &SpecNumber| match number {
            SpecNumber::Value(value) => ParamRead {
                value: *value,
                live: None,
            },
            SpecNumber::Param(place) => param(*place),
        };
        // The load refuses a negative value and rank; a scaling param can still give one.
        let not_negative =
            |number: Option<&SpecNumber>| number.map(|number| read(number).value.max(Num::ZERO));
        let ticks =
            |time: Option<&SpecTime>| time.map(|time| time.ticks(rate, |place| param(place).value));
        let duration = match duration {
            _ if from.hold.is_some() => None,
            Some(duration) => Some(duration),
            None => ticks(spec.duration.as_ref()),
        };
        let stack_life = ticks(spec.stacks_expire.as_ref());
        let interval = ticks(spec.interval.as_ref()).map(|every| Interval {
            every,
            next: now.after(every),
        });
        let shares = spec.stats.iter().map(move |change| {
            let read = read(&change.value);
            StatShare {
                value: read.value,
                live: read.live,
            }
        });
        let passive = from.hold == Some(Hold::Passive);
        let counts = passive && stack_life.is_some();
        let first = stack_life.filter(|_| !counts).map(|ticks| StackEnd {
            until: Instance::end(now, ticks),
            count: 1,
        });
        let instance = NewInstance {
            id,
            source: from.source,
            action: from.action,
            rank: from.rank,
            lifetime: match from.hold {
                Some(hold) => Lifetime::held(hold),
                None => Lifetime::applied(
                    duration.map_or(Ends::Never, |ticks| Ends::At(Instance::end(now, ticks))),
                ),
            },
            aura_radius: not_negative(spec.aura.as_ref().map(|aura| &aura.radius)),
            stacks: u32::from(!counts),
            stack_life,
            stack_end: first,
            interval,
            shield: not_negative(spec.shield.as_ref()),
            stats: shares,
            state: spec.state.initial().iter().cloned(),
        };
        Application {
            instance,
            reapply: if passive {
                Reapply::Refresh
            } else {
                spec.reapply
            },
            max_stacks: spec.max_stacks,
        }
    }
}

impl ModifierBook {
    /// Every package's modifiers, package by package, each by name, with its script; an error for
    /// a name the mode does not declare or a modifier that breaks a rule.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let scripts = cx.book::<ScriptBook>()?;
        let tags = cx.fact::<UnitTypes>()?.tags();
        let stats = cx.book::<StatBook>()?;
        let mut book = Self::default();
        for (index, package) in cx.packages() {
            let Some(modifiers) = package.content.get::<ModifiersData>() else {
                continue;
            };
            let modifiers = modifiers.0.iter().map(|(name, data)| {
                let script = data.script.as_ref().map(|path| cx.script(index, path));
                Ok(PackageModifier {
                    name,
                    data,
                    script: script.transpose()?,
                })
            });
            let modifiers = modifiers.collect::<Result<Vec<_>, BookError>>()?;
            let load = ModifierLoad {
                scripts,
                tags,
                stat: |stat: &Stat| {
                    let found = stats.named(stat);
                    found.ok_or_else(|| UnknownName::new(NameKind::Stat, stat))
                },
                rate: cx.rate(),
            };
            book.load(load, index, &modifiers)
                .map_err(|ModifierError { modifier, error }| {
                    let at = BookPlace::Entry {
                        table: ModifiersData::KEY,
                        name: modifier,
                    };
                    match error {
                        SpecError::Problem(problem) => BookError::rule(index, at, problem),
                        SpecError::Unknown(name) => BookError::unknown(index, at, name),
                    }
                })?;
        }
        Ok(book)
    }
}

impl Book for ModifierBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, names: &mut MatchNames) {
        names.set_table(Arc::clone(self.names()));
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::sync::Arc;

    use campfire_math::Num;
    use campfire_script::HookSet;
    use campfire_units::StatId;
    use campfire_values::{ModifierId, NameId, PackageIndex, TagSet};

    use crate::interval_revival::IntervalRevival;
    use crate::modifier_book::{ModifierBook, ModifierEntry};
    use crate::modifier_data::Reapply;
    use crate::modifier_spec::{ModifierSpec, SpecChange, SpecNumber};
    use crate::stat_op::StatOp;

    impl ModifierBook {
        /// Adds a modifier of no package that makes `changes`, each by a value of zero, and
        /// grants `tags`; its id. Its name is its place in the book.
        pub(crate) fn push_changes(
            &mut self,
            changes: &[(StatId, StatOp)],
            tags: TagSet,
        ) -> ModifierId {
            let book = Arc::make_mut(&mut self.entries);
            let at = book.len();
            let name = format!("{at:05}");
            let none = PackageIndex::new(u16::MAX);
            let id = Arc::make_mut(&mut self.names).add((none, name.as_str()));
            let stats = changes.iter().map(|&(stat, op)| SpecChange {
                stat,
                op,
                value: SpecNumber::Value(Num::ZERO),
            });
            book.push(ModifierEntry {
                package: none,
                spec: ModifierSpec {
                    duration: None,
                    interval: None,
                    revival: IntervalRevival::Continue,
                    stacks_expire: None,
                    reapply: Reapply::Refresh,
                    max_stacks: None,
                    stats: stats.collect(),
                    shield: None,
                    aura: None,
                    affects: None,
                    state: Arc::default(),
                },
                script: None,
                hooks: HookSet::default(),
                tags,
            });
            id
        }

        /// Makes modifier `id` grant `tags`, as a test's data would.
        pub fn grant_tags(&mut self, id: ModifierId, tags: TagSet) {
            Arc::make_mut(&mut self.entries)[id.index()].tags = tags;
        }
    }
}
