use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_common::{Tick, Ticks};
use campfire_declare::Book;
use campfire_math::Num;
use campfire_script::{HookSet, NameKind, ParamProblem, ScriptId, ScriptRole, UnknownName, View};
use campfire_sim::{Capability, TickRate};
use campfire_units::{ScriptBook, StatusTags, UnitTypeBook};
use campfire_values::{ActionId, DeclaredName, ModifierId, Names, Rank, Stat, TagSet};

use crate::application::{Application, NewInstance};
use crate::applier::Applier;
use crate::applier_param::ApplierParam;
use crate::error::ModifierError;
use crate::instance::{Instance, StackEnd, StatShare};
use crate::interval::Interval;
use crate::lifetime::{Ends, Hold, Lifetime};
use crate::modifier_data::{ModifierData, Reapply};
use crate::modifier_spec::{ModifierSpec, ParamPlace, SpecNames, SpecNumber, SpecTime};
use crate::param_book::ParamBook;
use crate::param_read::ParamRead;
use crate::stat_id::StatId;

/// The modifiers a match loaded, of every package: the mode, package 0, and each package it
/// depends on, in the order of its manifest. Package data, not state: a restore loads it from the
/// packages, as a new match does. Ids follow the order of package, then name. A clone shares
/// the entries, as the script view reads them.
#[derive(Resource, Debug, Clone, Default)]
pub struct ModifierBook {
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
    pub package: u16,
    pub(crate) spec: ModifierSpec,
    pub script: Option<ScriptId>,
    pub hooks: HookSet,
    /// The tags it grants its carrier.
    pub tags: TagSet,
}

/// What a package's modifiers load with: the hooks each script defines, the unit types that
/// declare the tags they grant and resolve their filters, each stat's place, and the tick rate.
#[derive(Debug)]
pub struct ModifierLoad<'a, S> {
    pub scripts: &'a ScriptBook,
    pub types: &'a mut UnitTypeBook,
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
        package: u16,
        modifiers: &[PackageModifier<'_>],
    ) -> Result<(), ModifierError> {
        let ModifierLoad {
            scripts,
            types,
            stat,
            rate,
        } = load;
        let start = self.entries.len();
        assert!(
            modifiers.is_sorted_by(|a, b| a.name < b.name),
            "a package's modifiers load by name"
        );
        let last = start.checked_sub(1).map(ModifierBook::id_at);
        if let (Some(last), Some(first)) = (last, modifiers.first()) {
            let held = (self.names.scope(last), self.names.name(last));
            assert!(
                held < (package, Some(first.name.as_str())),
                "modifiers load by package, then name"
            );
        }
        let tags: Vec<TagSet> = modifiers
            .iter()
            .map(|modifier| {
                let declare = |name: &DeclaredName| {
                    let tag = types.declare(name.as_str());
                    tag.expect("a match declares each tag its packages name before their modifiers")
                };
                modifier.data.tags.iter().map(declare).collect()
            })
            .collect();
        let names = SpecNames {
            stat,
            types: &*types,
            rate,
            modifier: |name: &str| {
                let at = modifiers.binary_search_by(|modifier| modifier.name.as_str().cmp(name));
                match at {
                    Ok(at) => Ok(ModifierBook::id_at(start + at)),
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
        let entries = modifiers.iter().zip(specs).zip(tags);
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
    pub(crate) fn same(&self, other: &ModifierBook) -> bool {
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

    /// The id of the modifier at `at` of the book, which numbers at most `u16` of them.
    pub fn id_at(at: usize) -> ModifierId {
        ModifierId::new(u16::try_from(at).expect("modifiers fit u16"))
    }

    /// The modifier `id`, when the book holds it.
    pub(crate) fn entry(&self, id: ModifierId) -> Option<&ModifierEntry> {
        self.entries.get(id.index())
    }

    /// The tags modifier `id` grants its carrier.
    pub fn tags(&self, id: ModifierId) -> TagSet {
        self.get(id).tags
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

    /// `id` as applied in tick `now` from `source`, by `ability` at its rank or by none, a way
    /// checked: its numbers resolved by `param`, from its own params, then the ability's, of its
    /// source as it is now, its times in ticks at `rate`; a stat change that reads a scaling
    /// table keeps reading it, live. Its duration is `duration` when a call names one. A passive
    /// or an aura holds while its ability or carrier keeps it, so it has no duration, and a
    /// passive applied again at another rank refreshes; a passive whose stacks end one by one
    /// counts them from none.
    pub(crate) fn application(
        &self,
        id: ModifierId,
        from: Applier,
        duration: Option<Ticks>,
        now: Tick,
        rate: TickRate,
        param: impl Fn(ParamPlace) -> ParamRead,
    ) -> Application {
        let entry = self.get(id);
        let spec = &entry.spec;
        let read = |number: &SpecNumber| match number {
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
        let shares = spec.stats.iter().map(|change| {
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
            lifetime: Lifetime::new(
                from.hold,
                duration.map_or(Ends::Never, |ticks| Ends::At(Instance::end(now, ticks))),
            ),
            aura_radius: not_negative(spec.aura.as_ref().map(|aura| &aura.radius)),
            stacks: u32::from(!counts),
            stack_life,
            stack_ends: first.into_iter().collect(),
            interval,
            shield: not_negative(spec.shield.as_ref()),
            stats: shares.collect(),
            state: spec.state.initial().to_vec(),
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

impl Book for ModifierBook {
    const OWNER: Option<Capability> = Some(Capability::Stats);
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, view: &View) {
        view.names_mut().set_table(Arc::clone(self.names()));
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::sync::Arc;

    use campfire_math::Num;
    use campfire_script::HookSet;
    use campfire_values::{ModifierId, TagSet};

    use crate::modifier_book::{ModifierBook, ModifierEntry};
    use crate::modifier_data::Reapply;
    use crate::modifier_spec::{ModifierSpec, SpecChange, SpecNumber};
    use crate::stat_id::StatId;
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
            let id = Arc::make_mut(&mut self.names).add((u16::MAX, name.as_str()));
            let stats = changes.iter().map(|&(stat, op)| SpecChange {
                stat,
                op,
                value: SpecNumber::Value(Num::ZERO),
            });
            book.push(ModifierEntry {
                package: u16::MAX,
                spec: ModifierSpec {
                    duration: None,
                    interval: None,
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
