use std::ops::Range;

use bevy_ecs::resource::Resource;
use campfire_sim::Capability;

use crate::actions::action_data::ActionData;
use crate::actions::amount::Amount;
use crate::actions::capability_does::CapabilityDoes;
use crate::actions::does::Does;
use crate::actions::effect_data::{
    DamageFields, EffectData, EffectTo, Effecting, HealFields, LaunchFields, ModifierFields,
    MoveData, PurgeFields, RestoreFields, SpawnFields, XpFields,
};
use crate::actions::effect_names::EffectNames;
use crate::actions::launch_id::LaunchId;
use crate::actions::lists_of::ListsOf;
use crate::declare::book::Book;
use crate::scripts::hook::Hook;
use crate::units::action_id::ActionId;

/// The effect lists of each action, their names resolved as the action loaded: one buffer, by
/// action id the run of each of its lists, `on_resolve`, `on_hit` and `on_end`, and by launch id
/// the run of each list a launch holds, `on_hit` and `on_end`. An action past the end has none.
/// Package data, not state.
#[derive(Resource, Debug, Default)]
pub(crate) struct EffectLists {
    effects: Vec<Listed>,
    runs: Vec<[Range<u32>; 3]>,
    launches: Vec<[Range<u32>; 2]>,
}

/// An effect of a list: what it does, and to whom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Listed {
    pub(crate) does: Does,
    pub(crate) to: EffectTo,
}

impl EffectLists {
    /// Adds the lists of `action`, which loaded last, from `data`, which the package load
    /// checked, their names resolved by `names`: `on_resolve`, `on_hit` and `on_end`, and the
    /// lists each launch of them holds, however deep.
    pub(crate) fn push(&mut self, action: ActionId, data: &ActionData, names: &impl EffectNames) {
        if self.runs.len() <= action.index() {
            self.runs.resize(action.index() + 1, [0..0, 0..0, 0..0]);
        }
        let mut runs = [0..0, 0..0, 0..0];
        for (run, list) in runs
            .iter_mut()
            .zip([&data.on_resolve, &data.on_hit, &data.on_end])
        {
            *run = self.append(list, names);
        }
        self.runs[action.index()] = runs;
    }

    /// Appends `list`, its names resolved by `names`, after the lists its launches hold, so its
    /// own effects lie together; its run.
    fn append(&mut self, list: &[EffectData], names: &impl EffectNames) -> Range<u32> {
        let resolved: Vec<Listed> = list
            .iter()
            .map(|effect| Listed {
                does: self.resolve(&effect.does, names),
                to: effect.to,
            })
            .collect();
        let start = position(self.effects.len());
        self.effects.extend(resolved);
        start..position(self.effects.len())
    }

    /// What `does` does, its names resolved by `names`; a launch's lists appended first.
    fn resolve(&mut self, does: &Effecting, names: &impl EffectNames) -> Does {
        let amount = |number| Amount::of(number, names);
        let of_capability = match does {
            Effecting::Modifier(ModifierFields { id, duration_ms }) => {
                return Does::Modifier {
                    id: names.modifier(id),
                    duration_ms: duration_ms.as_ref().map(amount),
                };
            }
            Effecting::Purge(PurgeFields { tag }) => {
                return Does::Purge {
                    tag: names.tag(tag),
                };
            }
            Effecting::Spawn(SpawnFields {
                unit_type,
                duration_ms,
            }) => {
                return Does::Spawn {
                    unit_type: names.package_type(unit_type),
                    duration_ms: duration_ms.as_ref().map(amount),
                };
            }
            Effecting::Damage(DamageFields {
                amount: dealt,
                kind,
            }) => CapabilityDoes::Damage {
                amount: amount(dealt),
                kind: names.damage_kind(kind),
            },
            Effecting::Heal(HealFields { amount: healed }) => CapabilityDoes::Heal {
                amount: amount(healed),
            },
            Effecting::Restore(RestoreFields {
                pool,
                amount: restored,
            }) => CapabilityDoes::Restore {
                pool: names.pool(pool),
                amount: amount(restored),
            },
            Effecting::Xp(XpFields {
                track,
                amount: given,
            }) => CapabilityDoes::Xp {
                track: names.track(track),
                amount: amount(given),
            },
            Effecting::Launch(LaunchFields {
                area,
                on_hit,
                on_end,
            }) => {
                let lists = [self.append(on_hit, names), self.append(on_end, names)];
                let launch = LaunchId(position(self.launches.len()));
                self.launches.push(lists);
                CapabilityDoes::Launch {
                    area: names.package_type(area),
                    launch,
                }
            }
            Effecting::Move(MoveData::Dash { to, speed }) => CapabilityDoes::Dash {
                to: *to,
                speed: amount(speed),
            },
            Effecting::Move(MoveData::KnockBack { from, distance, ms }) => {
                CapabilityDoes::KnockBack {
                    from: *from,
                    distance: amount(distance),
                    ms: amount(ms),
                }
            }
            Effecting::Planned(_) => unreachable!("the load refuses a planned effect"),
        };
        Does::Capability(of_capability)
    }

    /// The list of `of` that runs before its `hook`: an action's `on_resolve`, `on_hit` or
    /// `on_end`, or a launch's `on_hit` or `on_end`.
    pub(crate) fn of(&self, of: ListsOf, hook: Hook) -> &[Listed] {
        let run = match (of, hook) {
            (ListsOf::Action(action), _) => {
                let list = match hook {
                    Hook::OnResolve => 0,
                    Hook::OnHit => 1,
                    Hook::OnEnd => 2,
                    _ => unreachable!("only a resolve and a delivery's hit and end have lists"),
                };
                self.runs.get(action.index()).map(|runs| &runs[list])
            }
            (ListsOf::Launch(launch), Hook::OnHit | Hook::OnEnd) => {
                let list = usize::from(hook == Hook::OnEnd);
                Some(&self.launches[launch.0 as usize][list])
            }
            (ListsOf::Launch(_), _) => unreachable!("a launch's area has a hit and an end alone"),
        };
        run.map_or(&[], |run| {
            &self.effects[run.start as usize..run.end as usize]
        })
    }

    /// Whether the match loaded `launch`.
    pub(crate) fn has_launch(&self, launch: LaunchId) -> bool {
        (launch.0 as usize) < self.launches.len()
    }
}

/// A position in the buffer, which a match's data keeps within `u32`.
fn position(len: usize) -> u32 {
    u32::try_from(len).expect("a match's effects fit u32")
}

impl Book for EffectLists {
    const OWNER: Option<Capability> = None;
    const SCRIPTS_ONLY: bool = true;
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::actions::launch_id::LaunchId;

    impl LaunchId {
        /// The launch at `at` among the match's launches.
        pub(crate) const fn nth(at: u32) -> LaunchId {
            LaunchId(at)
        }
    }
}
