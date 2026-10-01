use std::cmp::Ordering;
use std::rc::Rc;

use bevy_ecs::resource::Resource;
use serde::{Deserialize, Serialize};

use campfire_math::Num;
use campfire_sim::{StableId, Tick, Ticks};

use crate::abilities::ability_book::AbilityId;
use crate::stats::modifier_data::{ModifierData, Reapply};
use crate::stats::modifier_handle::StateField;
use crate::stats::modifiers::{Application, Instance, StackEnd, StatShare};
use crate::stats::stat_book::StatBook;
use crate::units::script_view::ModifierInfo;
use crate::values::number::Number;
use crate::values::scalar::Scalar;

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
pub(crate) struct ModifierId(u16);

/// A loaded modifier: its package, its name there, and its data.
#[derive(Debug)]
pub(crate) struct ModifierEntry {
    pub(crate) package: u16,
    pub(crate) name: Box<str>,
    pub(crate) data: ModifierData,
}

impl ModifierBook {
    /// Loads `data` as the modifier `name` of `package`, after every modifier of an earlier
    /// package or name.
    pub(crate) fn load(&mut self, package: u16, name: &str, data: &ModifierData) -> ModifierId {
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
        });
        id
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

    /// Every modifier as scripts name it, by id.
    pub(crate) fn info(&self) -> Rc<[ModifierInfo]> {
        self.entries
            .iter()
            .map(|entry| {
                let fields = entry.data.state.iter().map(|(name, decl)| StateField {
                    name: name.as_str().into(),
                    kind: decl.kind,
                });
                let initial = entry.data.state.values().map(|decl| decl.initial.clone());
                ModifierInfo {
                    package: entry.package,
                    name: entry.name.clone(),
                    fields: fields.collect(),
                    initial: initial.collect(),
                    reapply: entry.data.reapply,
                    max_stacks: entry.data.max_stacks,
                }
            })
            .collect()
    }

    /// `id` as applied in tick `now` from `source`, by `ability` at its rank or by none: its
    /// numbers resolved from its own params, then the ability's, which `ability_param` reads at
    /// that rank, its duration `duration` when a call names one; `None` when a number does not
    /// resolve. A passive or an aura holds while its ability or carrier keeps it, so it has no
    /// duration, and a passive applied again at another rank refreshes.
    pub(crate) fn application(
        &self,
        id: ModifierId,
        from: Applier,
        duration: Option<Ticks>,
        now: Tick,
        stats: &StatBook,
        ability_param: impl Fn(&str) -> Option<Scalar>,
    ) -> Option<Application> {
        let data = &self.get(id).data;
        let number = |number: &Number| -> Option<Num> {
            match number {
                Number::Value(value) => value.to_num(),
                Number::Param(reference) => {
                    let name = reference.param.as_str();
                    let own = data.params.get(name).map(|param| param.at(from.rank));
                    own.unwrap_or_else(|| ability_param(name))?.to_num()
                }
            }
        };
        let ticks = |field: Option<&Number>| -> Option<Option<Ticks>> {
            let Some(field) = field else {
                return Some(None);
            };
            let ms = u64::try_from(number(field)?.ceil()).ok()?;
            Some(Some(stats.rate().ticks(ms)?.max(Ticks::ONE)))
        };
        let value = |field: Option<&Number>| -> Option<Option<Num>> {
            match field {
                Some(field) => Some(Some(number(field)?)),
                None => Some(None),
            }
        };
        let duration = match duration {
            _ if from.passive || from.aura => None,
            Some(duration) => Some(duration),
            None => ticks(data.duration_ms.as_ref())?,
        };
        let stack_life = ticks(data.stacks_expire_ms.as_ref())?;
        let shares = data.stats.iter().map(|(stat, value)| {
            Some(StatShare {
                stat: stats.index(stat)?,
                value: number(value)?,
            })
        });
        let instance = Instance {
            id,
            source: from.source,
            ability: from.ability,
            rank: from.rank,
            passive: from.passive,
            aura: from.aura,
            aura_radius: value(data.aura.as_ref().map(|aura| &aura.radius))?,
            stacks: 1,
            until: duration.map(|ticks| Instance::end(now, ticks)),
            stack_life,
            stack_ends: stack_life
                .map(|ticks| StackEnd {
                    until: Instance::end(now, ticks),
                    count: 1,
                })
                .into_iter()
                .collect(),
            shield: value(data.shield.as_ref())?,
            stats: shares.collect::<Option<_>>()?,
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
/// `rank`, rank 1 with none; and whether it is a passive, or an aura's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Applier {
    pub(crate) source: Option<StableId>,
    pub(crate) ability: Option<AbilityId>,
    pub(crate) rank: u8,
    pub(crate) passive: bool,
    pub(crate) aura: bool,
}

impl ModifierEntry {
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
