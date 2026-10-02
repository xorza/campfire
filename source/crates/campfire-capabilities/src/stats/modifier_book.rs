use std::cmp::Ordering;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_common::{Tick, Ticks};
use campfire_script::ScriptId;
use campfire_sim::TickRate;

use crate::scripts::hook::Hook;
use crate::scripts::hook_set::HookSet;
use crate::scripts::script_book::ScriptBook;
use crate::stats::application::{Application, NewInstance};
use crate::stats::applier::Applier;
use crate::stats::error::ModifierError;
use crate::stats::instance::{Instance, StackEnd, StatShare};
use crate::stats::lifetime::{Ends, Hold, Lifetime};
use crate::stats::modifier_clocks::Interval;
use crate::stats::modifier_data::{ModifierData, Reapply};
use crate::stats::modifier_spec::{ModifierSpec, ParamPlace, SpecNames, SpecNumber, SpecTime};
use crate::stats::param_read::ParamRead;
use crate::stats::stat_id::StatId;
use crate::units::modifier_id::ModifierId;
use crate::units::tag_set::TagSet;
use crate::units::unit_types::UnitTypes;
use crate::values::declared_name::DeclaredName;
use crate::values::stat::Stat;

/// The modifiers a match loaded, of every package: the mode, package 0, and each package it
/// depends on, in the order of its manifest. Package data, not state: a restore loads it from the
/// packages, as a new match does. Ids follow the order of package, then name. A clone shares
/// the entries, as the script view reads them.
#[derive(Resource, Debug, Clone, Default)]
pub(crate) struct ModifierBook {
    entries: Arc<Vec<ModifierEntry>>,
}

/// A loaded modifier: its package, its name there, its spec, and its compiled script with the
/// combat events it hears.
#[derive(Debug, Clone)]
pub(crate) struct ModifierEntry {
    pub(crate) package: u16,
    pub(crate) name: Box<str>,
    pub(crate) spec: ModifierSpec,
    pub(crate) script: Option<ScriptId>,
    pub(crate) hooks: HookSet,
    /// The tags it grants its carrier.
    pub(crate) tags: TagSet,
}

/// What a package's modifiers load with: the hooks each script defines, the unit types that
/// declare the tags they grant and resolve their filters, each stat's place, and the tick rate.
#[derive(Debug)]
pub(crate) struct ModifierLoad<'a, S> {
    pub(crate) scripts: &'a ScriptBook,
    pub(crate) types: &'a mut UnitTypes,
    pub(crate) stat: S,
    pub(crate) rate: TickRate,
}

/// A modifier of a package to load: its name, its data, and its compiled script exactly when its
/// data names one.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PackageModifier<'d> {
    pub(crate) name: &'d DeclaredName,
    pub(crate) data: &'d ModifierData,
    pub(crate) script: Option<ScriptId>,
}

impl ModifierBook {
    /// Loads `modifiers`, sorted by name, as those of `package`, after every modifier of an
    /// earlier package or name, their names resolved through `load`; an aura names a modifier
    /// of its own package. A modifier that does not load fails them all.
    pub(crate) fn load<S: Fn(&Stat) -> StatId>(
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
        if let (Some(last), Some(first)) = (self.entries.last(), modifiers.first()) {
            assert!(
                last.order(package, first.name.as_str()).is_lt(),
                "modifiers load by package, then name"
            );
        }
        let tags: Vec<TagSet> = modifiers
            .iter()
            .map(|modifier| {
                let declare = |name: &DeclaredName| types.declare(name.as_str());
                TagSet::of(modifier.data.tags.iter().map(declare))
            })
            .collect();
        let names = SpecNames {
            stat,
            types: &*types,
            rate,
            modifier: |name: &str| {
                let at = modifiers.binary_search_by(|modifier| modifier.name.as_str().cmp(name));
                match at {
                    Ok(at) => ModifierId::nth(start + at),
                    Err(_) => self
                        .named(package, name)
                        .expect("the load checked the aura's modifier"),
                }
            },
        };
        let specs = modifiers
            .iter()
            .map(|modifier| {
                ModifierSpec::of(modifier.data, &names).map_err(|problem| ModifierError {
                    modifier: modifier.name.clone(),
                    problem,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let entries = modifiers.iter().zip(specs).zip(tags);
        let book = Arc::make_mut(&mut self.entries);
        for ((modifier, spec), tags) in entries {
            book.push(ModifierEntry {
                package,
                name: modifier.name.as_str().into(),
                spec,
                script: modifier.script,
                hooks: scripts.defines(modifier.script, &MODIFIER_HOOKS),
                tags,
            });
        }
        Ok(())
    }

    /// The modifier `name` of `package`.
    pub(crate) fn named(&self, package: u16, name: &str) -> Option<ModifierId> {
        let at = self
            .entries
            .binary_search_by(|entry| entry.order(package, name))
            .ok()?;
        Some(ModifierId::nth(at))
    }

    /// The modifier `id`, when the book holds it.
    pub(crate) fn entry(&self, id: ModifierId) -> Option<&ModifierEntry> {
        self.entries.get(id.index())
    }

    /// The tags modifier `id` grants its carrier.
    pub(crate) fn tags(&self, id: ModifierId) -> TagSet {
        self.get(id).tags
    }

    pub(crate) fn get(&self, id: ModifierId) -> &ModifierEntry {
        &self.entries[id.index()]
    }

    /// `id` as applied in tick `now` from `source`, by `ability` at its rank or by none: its
    /// numbers resolved by `param`, from its own params, then the ability's, of its source as it
    /// is now, its times in ticks at `rate`; a stat change that reads a scaling table keeps
    /// reading it, live. Its duration is `duration` when a call names one; `None` when a number
    /// does not resolve. A passive or an aura holds while its ability or carrier keeps it, so it
    /// has no duration, and a passive applied again at another rank refreshes; a passive whose
    /// stacks end one by one counts them from none.
    pub(crate) fn application(
        &self,
        id: ModifierId,
        from: Applier,
        duration: Option<Ticks>,
        now: Tick,
        rate: TickRate,
        param: impl Fn(&ParamPlace) -> Option<ParamRead>,
    ) -> Option<Application> {
        let entry = self.get(id);
        let spec = &entry.spec;
        let read = |number: &SpecNumber| match number {
            SpecNumber::Value(value) => Some(ParamRead {
                value: *value,
                live: None,
            }),
            SpecNumber::Param(place) => param(place),
        };
        let value = |number: Option<&SpecNumber>| match number {
            Some(number) => read(number).map(|read| Some(read.value)),
            None => Some(None),
        };
        let ticks = |time: Option<&SpecTime>| match time {
            Some(time) => time
                .ticks(rate, |place| param(place).map(|read| read.value))
                .map(Some),
            None => Some(None),
        };
        let duration = match duration {
            _ if from.hold.is_some() => None,
            Some(duration) => Some(duration),
            None => ticks(spec.duration.as_ref())?,
        };
        let stack_life = ticks(spec.stacks_expire.as_ref())?;
        let interval = ticks(spec.interval.as_ref())?.map(|every| Interval {
            every,
            next: now.after(every),
        });
        let shares = spec.stats.iter().map(|change| {
            let read = read(&change.value)?;
            Some(StatShare {
                value: read.value,
                live: read.live,
            })
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
            ability: from.ability,
            rank: from.rank,
            lifetime: Lifetime::new(
                from.hold,
                duration.map_or(Ends::Never, |ticks| Ends::At(Instance::end(now, ticks))),
            ),
            aura_radius: value(spec.aura.as_ref().map(|aura| &aura.radius))?,
            stacks: u32::from(!counts),
            stack_life,
            stack_ends: first.into_iter().collect(),
            interval,
            shield: value(spec.shield.as_ref())?,
            stats: shares.collect::<Option<_>>()?,
            state: spec.initial.to_vec(),
        };
        Some(Application {
            instance,
            reapply: if passive {
                Reapply::Refresh
            } else {
                spec.reapply
            },
            max_stacks: spec.max_stacks,
        })
    }
}

impl ModifierEntry {
    /// How it sorts against the modifier `name` of `package`: by package, then name.
    fn order(&self, package: u16, name: &str) -> Ordering {
        self.package
            .cmp(&package)
            .then_with(|| (*self.name).cmp(name))
    }
}

/// The hooks of a modifier's script that combat events call.
const MODIFIER_HOOKS: [Hook; 6] = [
    Hook::OnAttack,
    Hook::OnInterval,
    Hook::OnAttackHit,
    Hook::OnDamageTaken,
    Hook::OnKill,
    Hook::OnTakedown,
];

#[cfg(test)]
pub(crate) mod internals {
    use std::sync::Arc;

    use crate::scripts::hook_set::HookSet;
    use crate::stats::modifier_book::{ModifierBook, ModifierEntry};
    use crate::stats::modifier_data::Reapply;
    use crate::stats::modifier_spec::{ModifierSpec, SpecChange, SpecNumber};
    use crate::stats::stat_id::StatId;
    use crate::stats::stat_op::StatOp;
    use crate::units::modifier_id::ModifierId;
    use crate::units::tag_set::TagSet;
    use campfire_math::Num;

    impl ModifierBook {
        /// Adds a modifier of no script, times or state that changes each stat of `changes` by
        /// its op, by a value its instances hold, and grants `tags`: what a test's instances are
        /// instances of. Its id, after every modifier before it.
        pub(crate) fn push_changes(
            &mut self,
            changes: &[(StatId, StatOp)],
            tags: TagSet,
        ) -> ModifierId {
            let book = Arc::make_mut(&mut self.entries);
            let at = book.len();
            let stats = changes.iter().map(|&(stat, op)| SpecChange {
                stat,
                op,
                value: SpecNumber::Value(Num::ZERO),
            });
            book.push(ModifierEntry {
                package: u16::MAX,
                name: format!("{at:05}").into(),
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
                    fields: Arc::from([]),
                    initial: Box::new([]),
                },
                script: None,
                hooks: HookSet::default(),
                tags,
            });
            ModifierId::nth(at)
        }

        /// Makes modifier `id` grant `tags`, as a test's data would.
        pub(crate) fn grant_tags(&mut self, id: ModifierId, tags: TagSet) {
            Arc::make_mut(&mut self.entries)[id.index()].tags = tags;
        }
    }
}
