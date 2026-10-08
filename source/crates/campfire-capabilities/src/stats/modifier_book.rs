use std::cmp::Ordering;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_math::Num;
use campfire_script::ScriptId;
use campfire_sim::TickRate;

use crate::scripts::error::ParamProblem;
use crate::scripts::hook_set::HookSet;
use crate::scripts::script_book::ScriptBook;
use crate::scripts::script_role::ScriptRole;
use crate::stats::application::{Application, NewInstance};
use crate::stats::applier::Applier;
use crate::stats::error::ModifierError;
use crate::stats::instance::{Instance, StackEnd, StatShare};
use crate::stats::lifetime::{Ends, Hold, Lifetime};
use crate::stats::modifier_clocks::Interval;
use crate::stats::modifier_data::{ModifierData, Reapply};
use crate::stats::modifier_spec::{ModifierSpec, ParamPlace, SpecNames, SpecNumber, SpecTime};
use crate::stats::param_book::ParamBook;
use crate::stats::param_read::ParamRead;
use crate::stats::stat_id::StatId;
use crate::units::action_id::ActionId;
use crate::units::modifier_id::ModifierId;
use crate::units::tag_set::TagSet;
use crate::units::unit_types::UnitTypes;
use crate::values::declared_name::DeclaredName;
use crate::values::rank::Rank;
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
                hooks: scripts.defines(modifier.script, ScriptRole::Modifier),
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

    /// Whether `ability` at `rank`, or no ability at rank 1, applies `id` with every param its
    /// numbers read: each of its own params has a value at `rank`, and `ability` gives each it
    /// does not declare, as a time in ticks at `rate` where a time reads it. The load checks the
    /// ways its packages name; a call checks the way of a name it computes.
    pub(crate) fn check_way(
        &self,
        id: ModifierId,
        ability: Option<ActionId>,
        rank: Rank,
        params: &ParamBook,
        rate: TickRate,
    ) -> Result<(), ParamProblem> {
        if !params.modifiers().holds_rank(id.index(), rank) {
            return Err(ParamProblem::Short);
        }
        let spec = &self.get(id).spec;
        for name in spec.number_params().filter_map(ParamPlace::applier) {
            params.gives(ability, rank, name, None)?;
        }
        for name in spec.time_params().filter_map(ParamPlace::applier) {
            params.gives(ability, rank, name, Some(rate))?;
        }
        Ok(())
    }

    /// Whether `id` and `ability`, when one, are a modifier and an action the books hold, and
    /// `ability` at `rank`, or no ability, applies `id` as `check_way` asks at `rate`.
    pub(crate) fn has_way(
        &self,
        id: ModifierId,
        ability: Option<ActionId>,
        rank: Rank,
        params: &ParamBook,
        rate: TickRate,
    ) -> bool {
        self.entry(id).is_some()
            && ability.is_none_or(|ability| params.has_action(ability))
            && self.check_way(id, ability, rank, params, rate).is_ok()
    }

    /// `has_way` of `world`'s books at its rate.
    pub(crate) fn has_way_in(
        world: &World,
        id: ModifierId,
        ability: Option<ActionId>,
        rank: Rank,
    ) -> bool {
        let (book, params) = (
            world.resource::<ModifierBook>(),
            world.resource::<ParamBook>(),
        );
        book.has_way(id, ability, rank, params, *world.resource::<TickRate>())
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
        param: impl Fn(&ParamPlace) -> ParamRead,
    ) -> Application {
        let entry = self.get(id);
        let spec = &entry.spec;
        let read = |number: &SpecNumber| match number {
            SpecNumber::Value(value) => ParamRead {
                value: *value,
                live: None,
            },
            SpecNumber::Param(place) => param(place),
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
            ability: from.ability,
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
            state: spec.initial.to_vec(),
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

impl ModifierEntry {
    /// How it sorts against the modifier `name` of `package`: by package, then name.
    fn order(&self, package: u16, name: &str) -> Ordering {
        self.package
            .cmp(&package)
            .then_with(|| (*self.name).cmp(name))
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use std::sync::Arc;

    use crate::scripts::hook_set::HookSet;
    use crate::stats::modifier_book::{ModifierBook, ModifierEntry};
    use crate::stats::modifier_data::Reapply;
    use crate::stats::modifier_spec::{ModifierSpec, SpecChange, SpecNumber};
    use std::collections::BTreeMap;

    use bevy_ecs::world::World;
    use campfire_math::Num;

    use crate::stats::param_book::ParamBook;
    use crate::stats::stat_id::StatId;
    use crate::stats::stat_op::StatOp;
    use crate::units::modifier_id::ModifierId;
    use crate::units::tag_set::TagSet;

    impl ModifierBook {
        /// Adds to `world`'s books a modifier of no params, script, times or state that changes
        /// each stat of `changes` by its op, by a value its instances hold, and grants `tags`:
        /// what a test's instances are instances of. Its id, after every modifier before it.
        pub(crate) fn push_changes(
            world: &mut World,
            changes: &[(StatId, StatOp)],
            tags: TagSet,
        ) -> ModifierId {
            let mut modifiers = world.resource_mut::<ModifierBook>();
            let book = Arc::make_mut(&mut modifiers.entries);
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
            let id = ModifierId::nth(at);
            ParamBook::load_modifier(world, id, &BTreeMap::new(), |_| StatId::new(0));
            id
        }

        /// Makes modifier `id` grant `tags`, as a test's data would.
        pub(crate) fn grant_tags(&mut self, id: ModifierId, tags: TagSet) {
            Arc::make_mut(&mut self.entries)[id.index()].tags = tags;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use campfire_math::Num;

    use crate::capability_set::test_match::TestMatch;
    use crate::scripts::error::ParamProblem;
    use crate::scripts::error::ParamProblem::{Missing, Overflow, ScalingTime, Short, Time};
    use crate::scripts::script_book::ScriptBook;
    use crate::stats::live_param::{LiveParam, ParamOwner};
    use crate::stats::modifier_book::{ModifierBook, ModifierLoad, PackageModifier};
    use crate::stats::modifier_data::ModifierData;
    use crate::stats::param_book::{ParamBook, ParamTables};
    use crate::stats::stat_id::StatId;
    use crate::units::action_id::ActionId;
    use crate::units::modifier_id::ModifierId;
    use crate::units::unit_types::UnitTypes;
    use crate::values::declared_name::DeclaredName;
    use crate::values::number::{Number, ParamRef};
    use crate::values::param::{Param, Scaling};
    use crate::values::rank::Rank;
    use crate::values::ranked::Ranked;
    use crate::values::scalar::Scalar;

    fn name(text: &str) -> DeclaredName {
        DeclaredName::new(text).unwrap()
    }

    fn param(text: &str) -> Number {
        Number::Param(ParamRef { param: name(text) })
    }

    #[test]
    fn a_way_applies_a_modifier_only_when_it_gives_each_param_the_modifier_reads() {
        // The action's params: 2⁴⁰, past a number's 2³⁹; −5 ms; 100 and 200 ms at its 2 ranks;
        // and a scaling table of 2 ranks.
        let int = |value| Param::Ranked(Ranked::One(Scalar::Int(value)));
        let scaling = Param::Scaling(Scaling {
            base: Ranked::PerRank(vec![Num::ONE, Num::ONE]),
            per_level: Num::ZERO,
            bonus: BTreeMap::new(),
            ratios: BTreeMap::new(),
        });
        let per_rank = Ranked::PerRank(vec![Scalar::Int(100), Scalar::Int(200)]);
        let action = BTreeMap::from([
            (name("huge"), int(1 << 40)),
            (name("late"), int(-5)),
            (name("pace"), Param::Ranked(per_rank)),
            (name("power"), scaling),
        ]);
        // By name, each modifier reads one param, as a shield or as a duration; `own` declares
        // its param, of one rank.
        let shield = |read| ModifierData {
            shield: Some(param(read)),
            ..ModifierData::default()
        };
        let lasting = |read| ModifierData {
            duration_ms: Some(param(read)),
            ..ModifierData::default()
        };
        let own = ModifierData {
            params: BTreeMap::from([(
                name("own"),
                Param::Ranked(Ranked::PerRank(vec![Scalar::Int(1)])),
            )]),
            ..shield("own")
        };
        let data = [
            ("absent", shield("absent")),
            ("huge", shield("huge")),
            ("late", lasting("late")),
            ("own", own),
            ("pace", lasting("pace")),
            ("power", lasting("power")),
            ("scaled", shield("power")),
        ]
        .map(|(id, data)| (name(id), data));
        let rate = TestMatch::RATE;
        let mut book = ModifierBook::default();
        let load = ModifierLoad {
            scripts: &ScriptBook::default(),
            types: &mut UnitTypes::default(),
            stat: |_: &_| StatId::new(0),
            rate,
        };
        let modifiers = data.each_ref().map(|(name, data)| PackageModifier {
            name,
            data,
            script: None,
        });
        book.load(load, 0, &modifiers).unwrap();
        let mut tables = ParamTables::default();
        tables.push_action(&action, |_| StatId::new(0));
        for (_, data) in &data {
            tables.push_modifier(&data.params, |_| StatId::new(0));
        }
        let params = ParamBook::new(tables);
        let ways = |ability, rank| -> Vec<Option<ParamProblem>> {
            let rank = Rank::new(rank).unwrap();
            let check = |at| book.check_way(ModifierId::nth(at), ability, rank, &params, rate);
            (0..data.len()).map(|at| check(at).err()).collect()
        };
        // By the action at rank 2: no `absent`; 2⁴⁰ past a number; −5 ms no time; `own` has no
        // second rank; 200 ms holds; a time of a scaling table does not, and a shield of it does.
        let action = Some(ActionId::nth(0));
        let by_action = [Missing, Overflow, Time, Short].map(Some);
        let by_action = [by_action.as_slice(), &[None, Some(ScalingTime), None]].concat();
        assert_eq!(ways(action, 2), by_action);
        // At rank 1 `own` holds too; at rank 3 no per-rank param has a value, the action's as
        // its own; with no action, only `own`, which declares its param, holds.
        assert_eq!(ways(action, 1)[3], None);
        let past = [Missing, Overflow, Time, Short, Short, Short, Short].map(Some);
        assert_eq!(ways(action, 3), past);
        let mut no_action = vec![Some(Missing); data.len()];
        no_action[3] = None;
        assert_eq!(ways(None, 1), no_action);
        // A live param is the action's scaling table, at a rank it has: not 2⁴⁰, which is no
        // table, nor a place past its params, nor rank 3.
        let live = |at| LiveParam {
            owner: ParamOwner::Action(ActionId::nth(0)),
            at,
        };
        let [second, third] = [2, 3].map(|rank| Rank::new(rank).unwrap());
        assert!(params.holds_live(live(3), second));
        assert!(!params.holds_live(live(0), second));
        assert!(!params.holds_live(live(4), second));
        assert!(!params.holds_live(live(3), third));
    }
}
