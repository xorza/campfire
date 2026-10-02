use std::cmp::Ordering;

use bevy_ecs::resource::Resource;
use campfire_script::ScriptId;
use serde::{Deserialize, Serialize};

use campfire_math::Num;
use campfire_sim::{StableId, Tick, Ticks};

use crate::actions::action_book::ActionId;
use crate::scripts::hook::Hook;
use crate::scripts::hook_set::HookSet;
use crate::scripts::script_book::ScriptBook;
use crate::stats::modifier_data::{ModifierData, Reapply};
use crate::stats::modifier_handle::StateField;
use crate::stats::modifiers::{Application, Instance, Interval, StackEnd, StatShare};
use crate::stats::param_read::ParamRead;
use crate::stats::stat_book::StatBook;
use crate::units::script_view::ModifierInfo;
use crate::units::tag_set::TagSet;
use crate::units::unit_types::UnitTypes;
use crate::values::declared_name::DeclaredName;
use crate::values::number::Number;

/// The modifiers a match loaded, of every package: the mode, package 0, and each package it
/// depends on, in the order of its manifest. Package data, not state: a restore loads it from the
/// packages, as a new match does. Ids follow the order of package, then name.
#[derive(Resource, Debug, Default)]
pub(crate) struct ModifierBook {
    entries: Vec<ModifierEntry>,
}

/// A modifier, by its place in the book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModifierId(u16);

/// A loaded modifier: its package, its name there, its data, and its compiled script with the
/// combat events it hears.
#[derive(Debug)]
pub(crate) struct ModifierEntry {
    pub(crate) package: u16,
    pub(crate) name: Box<str>,
    pub(crate) data: ModifierData,
    pub(crate) script: Option<ScriptId>,
    pub(crate) hooks: HookSet,
    /// The tags it grants its carrier.
    pub(crate) tags: TagSet,
}

impl ModifierBook {
    /// Loads `data` as the modifier `name` of `package`, after every modifier of an earlier
    /// package or name, with its `script`, the `hooks` it defines and the `tags` it grants.
    pub(crate) fn load(
        &mut self,
        scripts: &ScriptBook,
        types: &mut UnitTypes,
        package: u16,
        name: &str,
        data: &ModifierData,
        script: Option<ScriptId>,
    ) -> ModifierId {
        let hooks = scripts.defines(script, &MODIFIER_HOOKS);
        let declare = |name: &DeclaredName| types.declare(name.as_str());
        let tags = TagSet::of(data.tags.iter().map(declare));
        assert!(
            self.entries
                .last()
                .is_none_or(|last| last.order(package, name).is_lt()),
            "modifiers load by package, then name"
        );
        let id = ModifierId(u16::try_from(self.entries.len()).expect("modifiers fit u16"));
        self.entries.push(ModifierEntry {
            package,
            name: name.into(),
            data: data.clone(),
            script,
            hooks,
            tags,
        });
        id
    }

    /// Each modifier as scripts name it, by id.
    pub(crate) fn infos(&self) -> impl Iterator<Item = ModifierInfo> + '_ {
        self.entries.iter().map(ModifierEntry::info)
    }

    /// The modifier `name` of `package`.
    pub(crate) fn find(&self, package: u16, name: &str) -> Option<ModifierId> {
        let at = self
            .entries
            .binary_search_by(|entry| entry.order(package, name))
            .ok()?;
        Some(ModifierId(u16::try_from(at).expect("modifiers fit u16")))
    }

    pub(crate) fn get(&self, id: ModifierId) -> &ModifierEntry {
        &self.entries[id.index()]
    }

    /// `id` as applied in tick `now` from `source`, by `ability` at its rank or by none: its
    /// numbers resolved by `param`, from its own params, then the ability's, of its source as it
    /// is now; a stat change that reads a scaling table keeps reading it, live. Its duration is
    /// `duration` when a call names one; `None` when a number does not resolve. A passive or an aura holds while its ability or carrier keeps it, so it has no
    /// duration, and a passive applied again at another rank refreshes; a passive whose stacks
    /// end one by one counts them from none.
    pub(crate) fn application(
        &self,
        id: ModifierId,
        from: Applier,
        duration: Option<Ticks>,
        now: Tick,
        stats: &StatBook,
        param: impl Fn(&str) -> Option<ParamRead>,
    ) -> Option<Application> {
        let entry = self.get(id);
        let data = &entry.data;
        let read = |number: &Number| -> Option<ParamRead> {
            match number {
                Number::Value(value) => Some(ParamRead {
                    value: value.to_num()?,
                    live: None,
                }),
                Number::Param(reference) => param(reference.param.as_str()),
            }
        };
        let number = |number: &Number| read(number).map(|read| read.value);
        let ticks = |field: Option<&Number>| -> Option<Option<Ticks>> {
            let Some(field) = field else {
                return Some(None);
            };
            let ms = u64::try_from(number(field)?.ceil()).ok()?;
            Some(Some(stats.rate().duration(ms)?))
        };
        let value = |field: Option<&Number>| -> Option<Option<Num>> {
            match field {
                Some(field) => Some(Some(number(field)?)),
                None => Some(None),
            }
        };
        let duration = match duration {
            _ if from.passive || from.held => None,
            Some(duration) => Some(duration),
            None => ticks(data.duration_ms.as_ref())?,
        };
        let stack_life = ticks(data.stacks_expire_ms.as_ref())?;
        let interval = ticks(data.interval_ms.as_ref())?.map(|every| Interval {
            every,
            next: now.after(every),
        });
        let shares = data.stats.iter().map(|(stat, change)| {
            let read = read(&change.value)?;
            Some(StatShare {
                stat: stats.index(stat)?,
                op: change.op,
                value: read.value,
                live: read.live,
            })
        });
        let counts = from.passive && stack_life.is_some();
        let first = stack_life.filter(|_| !counts).map(|ticks| StackEnd {
            until: Instance::end(now, ticks),
            count: 1,
        });
        let instance = Instance {
            id,
            source: from.source,
            ability: from.ability,
            rank: from.rank,
            passive: from.passive,
            held: from.held,
            aura_radius: value(data.aura.as_ref().map(|aura| &aura.radius))?,
            stacks: u32::from(!counts),
            until: duration.map(|ticks| Instance::end(now, ticks)),
            stack_life,
            stack_ends: first.into_iter().collect(),
            interval,
            shield: value(data.shield.as_ref())?,
            stats: shares.collect::<Option<_>>()?,
            tags: entry.tags,
            state: data
                .state
                .values()
                .map(|decl| decl.initial.clone())
                .collect(),
        };
        Some(Application {
            instance,
            reapply: if from.passive {
                Reapply::Refresh
            } else {
                data.reapply
            },
            max_stacks: data.max_stacks,
        })
    }
}

/// Who applies a modifier: its source, none from the mode; the ability that applies it, at
/// `rank`, rank 1 with none; and whether it is a passive, or held by an aura or a player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Applier {
    pub(crate) source: Option<StableId>,
    pub(crate) ability: Option<ActionId>,
    pub(crate) rank: u8,
    pub(crate) passive: bool,
    pub(crate) held: bool,
}

impl ModifierEntry {
    /// The modifier as scripts name it.
    pub(crate) fn info(&self) -> ModifierInfo {
        let fields = self.data.state.iter().map(|(name, decl)| StateField {
            name: name.as_str().into(),
            kind: decl.kind,
        });
        let initial = self.data.state.values().map(|decl| decl.initial.clone());
        ModifierInfo {
            package: self.package,
            name: self.name.clone(),
            fields: fields.collect(),
            initial: initial.collect(),
            reapply: self.data.reapply,
            max_stacks: self.data.max_stacks,
        }
    }

    /// How it sorts against the modifier `name` of `package`: by package, then name.
    fn order(&self, package: u16, name: &str) -> Ordering {
        self.package
            .cmp(&package)
            .then_with(|| (*self.name).cmp(name))
    }
}

impl ModifierId {
    pub(crate) const fn new(index: u16) -> ModifierId {
        ModifierId(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
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
